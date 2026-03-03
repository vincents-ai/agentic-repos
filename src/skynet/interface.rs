//! Skynet v2 Plugin Interface Implementation
//!
//! This module implements the Skynet v2 PluginApi trait for Git Operations,
//! enabling integration with the Skynet v2 orchestration platform.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info};

use crate::config::AppConfig;
use crate::skynet::error::SkynetPluginResult;
use crate::skynet::metadata::PluginMetadata;
use crate::skynet::storage::global_registry;
use crate::skynet::{
    git_ops::GitOperationService, health::HealthChecker, monitoring::MetricsCollector,
    PLUGIN_NAME, PLUGIN_VERSION,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TenantStorageMapping {
    tenant_id: String,
    backend_name: String,
    bucket_prefix: Option<String>,
}

/// Skynet v2 standard request structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRequest {
    /// Request ID for correlation
    pub id: String,

    /// RPC method name
    pub method: String,

    /// Method parameters
    #[serde(default)]
    pub params: Value,

    /// Tenant context (optional)
    #[serde(default)]
    pub tenant_id: Option<String>,

    /// User context (optional)
    #[serde(default)]
    pub user_id: Option<String>,

    /// API token for authentication
    #[serde(default)]
    pub token: Option<String>,
}

/// Skynet v2 standard response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginResponse {
    /// Request ID (echoed from request)
    pub id: String,

    /// Response result (if successful)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,

    /// Error (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

impl PluginResponse {
    /// Create a successful response
    pub fn success(id: impl Into<String>, result: Value) -> Self {
        Self {
            id: id.into(),
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response
    pub fn error(id: impl Into<String>, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            result: None,
            error: Some(json!({
                "code": code.into(),
                "message": message.into(),
            })),
        }
    }
}

/// Health status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Overall health status: "healthy", "degraded", "unhealthy"
    pub status: String,

    /// Plugin version
    pub version: String,

    /// Uptime in seconds
    pub uptime_secs: u64,

    /// Active repositories
    pub active_repositories: usize,

    /// Active sessions
    pub active_sessions: usize,

    /// Storage usage in bytes
    pub storage_bytes: u64,

    /// Health check timestamp
    pub checked_at: String,

    /// Detailed error messages (if not healthy)
    #[serde(default)]
    pub errors: Vec<String>,
}

/// Git Operations Plugin for Skynet v2
pub struct GitOperationsPlugin {
    /// Plugin metadata
    metadata: PluginMetadata,

    /// Configuration
    #[allow(dead_code)]
    config: AppConfig,

    /// Git operation service
    git_service: Arc<GitOperationService>,

    /// Health checker
    health_checker: Arc<HealthChecker>,

    /// Metrics collector
    metrics: Arc<MetricsCollector>,

    /// Plugin start time
    start_time: Instant,

    /// Tenant to storage backend mappings
    tenant_mappings: Arc<std::sync::RwLock<HashMap<String, TenantStorageMapping>>>,

    /// Default storage backend name
    default_backend: Arc<std::sync::RwLock<Option<String>>>,
}

impl GitOperationsPlugin {
    /// Create a new Git Operations Plugin
    pub async fn new(config: AppConfig) -> SkynetPluginResult<Self> {
        info!("Initializing GitOperationsPlugin");

        let git_service = Arc::new(GitOperationService::new(config.clone()).await?);
        let health_checker = Arc::new(HealthChecker::new());
        let metrics = Arc::new(MetricsCollector::new());

        Ok(Self {
            metadata: PluginMetadata::git_operations(),
            config,
            git_service,
            health_checker,
            metrics,
            start_time: Instant::now(),
            tenant_mappings: Arc::new(std::sync::RwLock::new(HashMap::new())),
            default_backend: Arc::new(std::sync::RwLock::new(None)),
        })
    }

    /// Get plugin metadata
    pub fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    /// Handle a plugin request
    pub async fn handle_request(&self, request: PluginRequest) -> PluginResponse {
        debug!("Handling request: method={}, id={}", request.method, request.id);

        // Record request metric
        self.metrics.record_request(&request.method);

        // Route to appropriate handler
        let result = match request.method.as_str() {
            // Repository operations
            "repo.create" => self.handle_repo_create(&request).await,
            "repo.delete" => self.handle_repo_delete(&request).await,
            "repo.list" => self.handle_repo_list(&request).await,
            "repo.info" => self.handle_repo_info(&request).await,

            // Health and monitoring
            "health.check" => self.handle_health_check(&request).await,
            "metrics.get" => self.handle_metrics_get(&request).await,

            // Storage operations
            "storage.list_backends" => self.handle_storage_list_backends(&request).await,
            "storage.list_buckets" => self.handle_storage_list_buckets(&request).await,
            "storage.create_bucket" => self.handle_storage_create_bucket(&request).await,
            "storage.delete_bucket" => self.handle_storage_delete_bucket(&request).await,
            "storage.set_tenant_mapping" => self.handle_storage_set_tenant_mapping(&request).await,
            "storage.get_tenant_mapping" => self.handle_storage_get_tenant_mapping(&request).await,
            "storage.remove_tenant_mapping" => self.handle_storage_remove_tenant_mapping(&request).await,
            "storage.set_default_backend" => self.handle_storage_set_default_backend(&request).await,

            // Unknown method
            _ => Err(crate::skynet::error::SkynetPluginError::invalid_request(
                format!("Unknown method: {}", request.method),
            )),
        };

        // Convert result to response
        match result {
            Ok(result) => {
                self.metrics.record_success(&request.method);
                PluginResponse::success(&request.id, result)
            }
            Err(err) => {
                self.metrics.record_error(&request.method, &err.error_code());
                error!("Request failed: {}", err);
                PluginResponse::error(&request.id, &err.error_code(), err.to_string())
            }
        }
    }

    // Handler implementations
    async fn handle_repo_create(&self, request: &PluginRequest) -> SkynetPluginResult<Value> {
        let tenant_id = request
            .tenant_id
            .as_ref()
            .ok_or_else(|| crate::skynet::error::SkynetPluginError::invalid_request("tenant_id required"))?
            .clone();

        let repo_name = request
            .params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::skynet::error::SkynetPluginError::invalid_request("name parameter required"))?
            .to_string();

        self.git_service
            .create_repository(&tenant_id, &repo_name)
            .await?;

        Ok(json!({
            "repository": {
                "tenant_id": tenant_id,
                "name": repo_name,
                "created_at": chrono::Utc::now().to_rfc3339(),
            }
        }))
    }

    async fn handle_repo_delete(&self, request: &PluginRequest) -> SkynetPluginResult<Value> {
        let tenant_id = request
            .tenant_id
            .as_ref()
            .ok_or_else(|| crate::skynet::error::SkynetPluginError::invalid_request("tenant_id required"))?
            .clone();

        let repo_name = request
            .params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::skynet::error::SkynetPluginError::invalid_request("name parameter required"))?
            .to_string();

        self.git_service
            .delete_repository(&tenant_id, &repo_name)
            .await?;

        Ok(json!({
            "status": "deleted",
            "repository": {
                "tenant_id": tenant_id,
                "name": repo_name,
            }
        }))
    }

    async fn handle_repo_list(&self, request: &PluginRequest) -> SkynetPluginResult<Value> {
        let tenant_id = request
            .tenant_id
            .as_ref()
            .ok_or_else(|| crate::skynet::error::SkynetPluginError::invalid_request("tenant_id required"))?
            .clone();

        let repos = self.git_service.list_repositories(&tenant_id).await?;

        Ok(json!({
            "repositories": repos,
            "count": repos.len(),
        }))
    }

    async fn handle_repo_info(&self, request: &PluginRequest) -> SkynetPluginResult<Value> {
        let tenant_id = request
            .tenant_id
            .as_ref()
            .ok_or_else(|| crate::skynet::error::SkynetPluginError::invalid_request("tenant_id required"))?
            .clone();

        let repo_name = request
            .params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::skynet::error::SkynetPluginError::invalid_request("name parameter required"))?
            .to_string();

        let info = self
            .git_service
            .get_repository_info(&tenant_id, &repo_name)
            .await?;

        Ok(json!(info))
    }

    async fn handle_health_check(&self, _request: &PluginRequest) -> SkynetPluginResult<Value> {
        let status = self.health_checker.check().await?;

        Ok(json!({
            "status": status.status,
            "version": PLUGIN_VERSION,
            "uptime_secs": self.start_time.elapsed().as_secs(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }))
    }

    async fn handle_metrics_get(&self, _request: &PluginRequest) -> SkynetPluginResult<Value> {
        let metrics = self.metrics.get_metrics();

        Ok(json!({
            "metrics": metrics,
            "uptime_secs": self.start_time.elapsed().as_secs(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }))
    }

    async fn handle_storage_list_backends(&self, request: &PluginRequest) -> SkynetPluginResult<Value> {
        let include_details = request
            .params
            .get("include_details")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let backends = global_registry().list_backends();

        Ok(json!({
            "backends": backends.iter().map(|(name, backend_type, is_default)| {
                if include_details {
                    json!({
                        "name": name,
                        "type": backend_type,
                        "is_default": is_default,
                    })
                } else {
                    json!({
                        "name": name,
                        "type": backend_type,
                        "is_default": is_default,
                    })
                }
            }).collect::<Vec<_>>(),
            "count": backends.len(),
        }))
    }

    async fn handle_storage_list_buckets(&self, request: &PluginRequest) -> SkynetPluginResult<Value> {
        let backend_name = request
            .params
            .get("backend_name")
            .and_then(|v| v.as_str())
            .map(String::from);

        let backend = if let Some(name) = &backend_name {
            global_registry().get_for_tenant(name)
        } else {
            global_registry().get_default()
        };

        match backend {
            Some(service) => {
                let result = (service.list_buckets)(service.context);
                let bucket_names: Vec<String> = unsafe {
                    if result.count > 0 {
                        let slice = std::slice::from_raw_parts_mut(result.buckets, result.count);
                        let mut names = Vec::new();
                        for ptr in slice {
                            if !ptr.is_null() {
                                names.push(std::ffi::CStr::from_ptr(*ptr).to_string_lossy().into_owned());
                            }
                        }
                        names
                    } else {
                        Vec::new()
                    }
                };

                Ok(json!({
                    "backend": service.get_backend_name().unwrap_or_else(|| "unknown".to_string()),
                    "buckets": bucket_names,
                    "count": bucket_names.len(),
                }))
            }
            None => Ok(json!({
                "backend": backend_name,
                "buckets": [],
                "count": 0,
                "error": "No storage backend available",
            })),
        }
    }

    async fn handle_storage_create_bucket(&self, request: &PluginRequest) -> SkynetPluginResult<Value> {
        let bucket_name = request
            .params
            .get("bucket_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::skynet::error::SkynetPluginError::invalid_request("bucket_name parameter required"))?
            .to_string();

        let backend_name = request
            .params
            .get("backend_name")
            .and_then(|v| v.as_str())
            .map(String::from);

        let tenant_id = request.tenant_id.clone();

        let backend = if let Some(name) = &backend_name {
            global_registry().get_for_tenant(name)
        } else if let Some(tid) = &tenant_id {
            global_registry().get_for_tenant(tid)
        } else {
            global_registry().get_default()
        };

        match backend {
            Some(service) => {
                let bucket_config = crate::skynet::storage::BucketConfig::default();
                let bucket_name_cstr = std::ffi::CString::new(bucket_name.clone())
                    .map_err(|_| crate::skynet::error::SkynetPluginError::invalid_request("Invalid bucket name"))?;
                
                let create_result = (service.create_bucket)(
                    service.context,
                    bucket_name_cstr.as_ptr(),
                    &bucket_config,
                );

                if create_result.is_success() {
                    Ok(json!({
                        "bucket": bucket_name,
                        "backend": service.get_backend_name().unwrap_or_else(|| "unknown".to_string()),
                        "created_at": chrono::Utc::now().to_rfc3339(),
                    }))
                } else {
                    let error_msg = create_result.get_error().unwrap_or("Failed to create bucket".to_string());
                    Err(crate::skynet::error::SkynetPluginError::storage(error_msg))
                }
            }
            None => Err(crate::skynet::error::SkynetPluginError::storage("No storage backend available")),
        }
    }

    async fn handle_storage_delete_bucket(&self, request: &PluginRequest) -> SkynetPluginResult<Value> {
        let bucket_name = request
            .params
            .get("bucket_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::skynet::error::SkynetPluginError::invalid_request("bucket_name parameter required"))?
            .to_string();

        let backend_name = request
            .params
            .get("backend_name")
            .and_then(|v| v.as_str())
            .map(String::from);

        let backend = if let Some(name) = &backend_name {
            global_registry().get_for_tenant(name)
        } else {
            global_registry().get_default()
        };

        match backend {
            Some(service) => {
                let bucket_name_cstr = std::ffi::CString::new(bucket_name.clone())
                    .map_err(|_| crate::skynet::error::SkynetPluginError::invalid_request("Invalid bucket name"))?;
                let delete_result = (service.delete_bucket)(service.context, bucket_name_cstr.as_ptr());

                if delete_result.is_success() {
                    Ok(json!({
                        "bucket": bucket_name,
                        "backend": service.get_backend_name().unwrap_or_else(|| "unknown".to_string()),
                        "deleted_at": chrono::Utc::now().to_rfc3339(),
                    }))
                } else {
                    let error_msg = delete_result.get_error().unwrap_or("Failed to delete bucket".to_string());
                    Err(crate::skynet::error::SkynetPluginError::storage(error_msg))
                }
            }
            None => Err(crate::skynet::error::SkynetPluginError::storage("No storage backend available")),
        }
    }

    async fn handle_storage_set_tenant_mapping(
        &self,
        request: &PluginRequest,
    ) -> SkynetPluginResult<Value> {
        let tenant_id = request
            .tenant_id
            .as_ref()
            .ok_or_else(|| crate::skynet::error::SkynetPluginError::invalid_request("tenant_id required"))?
            .clone();

        let backend_name = request
            .params
            .get("backend_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::skynet::error::SkynetPluginError::invalid_request("backend_name parameter required"))?
            .to_string();

        let bucket_prefix = request
            .params
            .get("bucket_prefix")
            .and_then(|v| v.as_str())
            .map(String::from);

        let backends = global_registry().list_backends();
        let backend_exists = backends.iter().any(|(name, _, _)| name == &backend_name);

        if !backend_exists {
            return Err(crate::skynet::error::SkynetPluginError::invalid_request(
                format!("Storage backend '{}' not found", backend_name)
            ));
        }

        let mut tenant_mapping = self.tenant_mappings.write().unwrap().clone();
        tenant_mapping.insert(
            tenant_id.clone(),
            TenantStorageMapping {
                tenant_id: tenant_id.clone(),
                backend_name: backend_name.clone(),
                bucket_prefix: bucket_prefix.clone(),
            },
        );
        *(self.tenant_mappings.write().unwrap()) = tenant_mapping;

        Ok(json!({
            "mapping": {
                "tenant_id": tenant_id,
                "backend_name": backend_name,
                "bucket_prefix": bucket_prefix,
            },
            "configured_at": chrono::Utc::now().to_rfc3339(),
        }))
    }

    async fn handle_storage_get_tenant_mapping(
        &self,
        request: &PluginRequest,
    ) -> SkynetPluginResult<Value> {
        let tenant_id = request
            .tenant_id
            .as_ref()
            .ok_or_else(|| crate::skynet::error::SkynetPluginError::invalid_request("tenant_id required"))?
            .clone();

        let mappings = self.tenant_mappings.read().unwrap();
        
        if let Some(mapping) = mappings.get(&tenant_id) {
            Ok(json!({
                "tenant_id": mapping.tenant_id,
                "backend_name": mapping.backend_name,
                "bucket_prefix": mapping.bucket_prefix,
            }))
        } else {
            let backends = global_registry().list_backends();
            for (name, backend_type, is_default) in backends {
                if is_default {
                    return Ok(json!({
                        "tenant_id": tenant_id,
                        "backend_name": name,
                        "backend_type": backend_type,
                        "is_default": true,
                        "note": "Using default backend (no explicit mapping configured)",
                    }));
                }
            }
            
            Ok(json!({
                "tenant_id": tenant_id,
                "backend_name": null,
                "backend_type": null,
                "is_default": false,
                "note": "No mapping found and no default backend configured",
            }))
        }
    }

    async fn handle_storage_remove_tenant_mapping(
        &self,
        request: &PluginRequest,
    ) -> SkynetPluginResult<Value> {
        let tenant_id = request
            .tenant_id
            .as_ref()
            .ok_or_else(|| crate::skynet::error::SkynetPluginError::invalid_request("tenant_id required"))?
            .clone();

        let mut mappings = self.tenant_mappings.write().unwrap().clone();
        let removed = mappings.remove(&tenant_id).is_some();
        *(self.tenant_mappings.write().unwrap()) = mappings;

        Ok(json!({
            "tenant_id": tenant_id,
            "removed": removed,
            "message": if removed {
                "Tenant mapping removed successfully"
            } else {
                "No mapping was configured for this tenant"
            },
        }))
    }

    async fn handle_storage_set_default_backend(
        &self,
        request: &PluginRequest,
    ) -> SkynetPluginResult<Value> {
        let backend_name = request
            .params
            .get("backend_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::skynet::error::SkynetPluginError::invalid_request("backend_name parameter required"))?
            .to_string();

        let backends = global_registry().list_backends();
        let backend_exists = backends.iter().any(|(name, _, _)| name == &backend_name);

        if !backend_exists {
            return Err(crate::skynet::error::SkynetPluginError::invalid_request(
                format!("Storage backend '{}' not found", backend_name)
            ));
        }

        *self.default_backend.write().unwrap() = Some(backend_name.clone());

        Ok(json!({
            "default_backend": backend_name,
            "configured_at": chrono::Utc::now().to_rfc3339(),
            "message": "Default backend updated. Note: This is a runtime setting for this instance.",
        }))
    }
}

/// Async trait implementation for Skynet v2 compatibility
#[async_trait]
pub trait PluginApi: Send + Sync {
    /// Get plugin metadata
    async fn metadata(&self) -> PluginMetadata;

    /// Initialize plugin
    async fn init(&self) -> SkynetPluginResult<()>;

    /// Shutdown plugin gracefully
    async fn shutdown(&self) -> SkynetPluginResult<()>;

    /// Health check
    async fn health_check(&self) -> SkynetPluginResult<HealthStatus>;

    /// Execute a command
    async fn execute(&self, request: PluginRequest) -> SkynetPluginResult<PluginResponse>;
}

#[async_trait]
impl PluginApi for GitOperationsPlugin {
    async fn metadata(&self) -> PluginMetadata {
        self.metadata.clone()
    }

    async fn init(&self) -> SkynetPluginResult<()> {
        info!("Initializing {} plugin", PLUGIN_NAME);
        // Perform any initialization logic
        Ok(())
    }

    async fn shutdown(&self) -> SkynetPluginResult<()> {
        info!("Shutting down {} plugin", PLUGIN_NAME);
        // Perform cleanup
        Ok(())
    }

    async fn health_check(&self) -> SkynetPluginResult<HealthStatus> {
        self.health_checker.check().await
    }

    async fn execute(&self, request: PluginRequest) -> SkynetPluginResult<PluginResponse> {
        Ok(self.handle_request(request).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_response_success() {
        let response = PluginResponse::success("req-1", json!({"result": "ok"}));
        assert_eq!(response.id, "req-1");
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_plugin_response_error() {
        let response = PluginResponse::error("req-2", "TEST_ERROR", "test message");
        assert_eq!(response.id, "req-2");
        assert!(response.result.is_none());
        assert!(response.error.is_some());
    }
}
