//! FFI Bridge for Skylet Plugin ABI v2
//!
//! This module provides the C FFI interface required by the Skylet plugin system (v2 ABI).
//! It bridges the async Rust `GitOperationsPlugin` to the synchronous `extern "C"`
//! ABI that Skylet's PluginLoader expects.
//!
//! # v2 ABI Exported symbols
//!
//! - `plugin_get_info_v2` — Return static `PluginInfoV2` metadata
//! - `plugin_init_v2` — Initialize the plugin with a `PluginContextV2`
//! - `plugin_shutdown_v2` — Graceful shutdown
//! - `plugin_handle_request_v2` — Handle an HTTP request and produce an HTTP response
//! - `plugin_get_tools` — Return MCP tool schemas as JSON (optional)
//! - `plugin_free_tools` — Free the string returned by `plugin_get_tools` (optional)

use std::ffi::{c_char, CStr, CString};
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Arc, Mutex};

use skylet_abi::v2_spec::{
    CapabilityInfo, HealthStatus, MaturityLevel, PluginCategory, PluginContextV2, PluginInfoV2,
    PluginResultV2, ServiceInfo, ServiceRegistryV2,
};
use skylet_abi::{HttpHeader, HttpRequest, HttpResponse, PropertySchema, ToolSchema};
use tracing::{debug, info};

use crate::skynet::interface::{PluginRequest, PluginResponse};
use crate::skynet::storage::{global_registry, S3CompatibleStorageV1};
use crate::skynet::GitOperationsPlugin;

const PLUGIN_NAME: &[u8] = b"git-operations\0";
const PLUGIN_VERSION: &[u8] = b"0.1.0\0";
const PLUGIN_DESCRIPTION: &[u8] =
    b"Multi-tenant Git server with repository management, SSH access, and LFS support\0";
const PLUGIN_AUTHOR: &[u8] = b"Vincents AI Team\0";
const PLUGIN_LICENSE: &[u8] = b"AGPL-3.0\0";
const PLUGIN_HOMEPAGE: &[u8] = b"https://vincents.ai\0";
const PLUGIN_ABI_VERSION: &[u8] = b"2.0\0";
const PLUGIN_SKYLET_MIN: &[u8] = b"0.2.0\0";

const TAG_GIT: &[u8] = b"git\0";
const TAG_MULTITENANT: &[u8] = b"multi-tenant\0";
const TAG_LFS: &[u8] = b"lfs\0";
const TAG_SSH: &[u8] = b"ssh\0";
const TAG_STORAGE: &[u8] = b"storage\0";

const SERVICE_NAME: &[u8] = b"git-operations\0";
const SERVICE_VERSION: &[u8] = b"1.0.0\0";
#[allow(dead_code)]
const SERVICE_DESCRIPTION: &[u8] = b"Multi-tenant Git repository operations service\0";
#[allow(dead_code)]
const SERVICE_SPEC: &[u8] = b"git-operations-v1\0";

#[allow(dead_code)]
const SERVICE_TYPE_S3_STORAGE: &str = "s3-storage";
const SERVICE_TYPE_STORAGE_V1: &str = "StorageServiceV1";

static PLUGIN_INFO: AtomicPtr<PluginInfoV2> = AtomicPtr::new(ptr::null_mut());
static RUNTIME: Mutex<Option<tokio::runtime::Runtime>> = Mutex::new(None);

static CAPABILITIES_STORAGE: AtomicPtr<[CapabilityInfo; 9]> = AtomicPtr::new(ptr::null_mut());
static TAGS_STORAGE: AtomicPtr<[*const c_char; 5]> = AtomicPtr::new(ptr::null_mut());
#[allow(dead_code)]
static SERVICE_STORAGE: AtomicPtr<ServiceInfo> = AtomicPtr::new(ptr::null_mut());

fn init_capabilities() -> *const [CapabilityInfo; 9] {
    if CAPABILITIES_STORAGE.load(Ordering::SeqCst).is_null() {
        let caps = Box::new([
            CapabilityInfo {
                name: b"repo.create\0".as_ptr() as *const c_char,
                description: b"Create a new Git repository\0".as_ptr() as *const c_char,
                required_permission: ptr::null(),
            },
            CapabilityInfo {
                name: b"repo.delete\0".as_ptr() as *const c_char,
                description: b"Delete a Git repository\0".as_ptr() as *const c_char,
                required_permission: ptr::null(),
            },
            CapabilityInfo {
                name: b"repo.list\0".as_ptr() as *const c_char,
                description: b"List repositories for a tenant\0".as_ptr() as *const c_char,
                required_permission: ptr::null(),
            },
            CapabilityInfo {
                name: b"repo.info\0".as_ptr() as *const c_char,
                description: b"Get repository information\0".as_ptr() as *const c_char,
                required_permission: ptr::null(),
            },
            CapabilityInfo {
                name: b"lfs.operation\0".as_ptr() as *const c_char,
                description: b"Perform LFS operations\0".as_ptr() as *const c_char,
                required_permission: ptr::null(),
            },
            CapabilityInfo {
                name: b"ssh.manage\0".as_ptr() as *const c_char,
                description: b"Manage SSH keys for repository access\0".as_ptr() as *const c_char,
                required_permission: ptr::null(),
            },
            CapabilityInfo {
                name: b"storage.list\0".as_ptr() as *const c_char,
                description: b"List available storage backends\0".as_ptr() as *const c_char,
                required_permission: ptr::null(),
            },
            CapabilityInfo {
                name: b"storage.bucket\0".as_ptr() as *const c_char,
                description: b"Manage storage buckets\0".as_ptr() as *const c_char,
                required_permission: ptr::null(),
            },
            CapabilityInfo {
                name: b"tenant.mapping\0".as_ptr() as *const c_char,
                description: b"Configure tenant to storage backend mappings\0".as_ptr()
                    as *const c_char,
                required_permission: ptr::null(),
            },
        ]);
        CAPABILITIES_STORAGE.store(Box::leak(caps) as *mut _, Ordering::SeqCst);
    }
    CAPABILITIES_STORAGE.load(Ordering::SeqCst)
}

fn init_tags() -> *const [*const c_char; 5] {
    if TAGS_STORAGE.load(Ordering::SeqCst).is_null() {
        let tags = Box::new([
            TAG_GIT.as_ptr() as *const c_char,
            TAG_MULTITENANT.as_ptr() as *const c_char,
            TAG_LFS.as_ptr() as *const c_char,
            TAG_SSH.as_ptr() as *const c_char,
            TAG_STORAGE.as_ptr() as *const c_char,
        ]);
        TAGS_STORAGE.store(Box::leak(tags) as *mut _, Ordering::SeqCst);
    }
    TAGS_STORAGE.load(Ordering::SeqCst)
}

#[allow(dead_code)]
fn init_service() -> *const ServiceInfo {
    if SERVICE_STORAGE.load(Ordering::SeqCst).is_null() {
        let service = Box::new(ServiceInfo {
            name: SERVICE_NAME.as_ptr() as *const c_char,
            version: SERVICE_VERSION.as_ptr() as *const c_char,
            description: SERVICE_DESCRIPTION.as_ptr() as *const c_char,
            interface_spec: SERVICE_SPEC.as_ptr() as *const c_char,
        });
        SERVICE_STORAGE.store(Box::leak(service) as *mut _, Ordering::SeqCst);
    }
    SERVICE_STORAGE.load(Ordering::SeqCst)
}

fn init_plugin_info() -> *const PluginInfoV2 {
    if PLUGIN_INFO.load(Ordering::SeqCst).is_null() {
        let info = Box::new(PluginInfoV2 {
            name: PLUGIN_NAME.as_ptr() as *const c_char,
            version: PLUGIN_VERSION.as_ptr() as *const c_char,
            description: PLUGIN_DESCRIPTION.as_ptr() as *const c_char,
            author: PLUGIN_AUTHOR.as_ptr() as *const c_char,
            license: PLUGIN_LICENSE.as_ptr() as *const c_char,
            homepage: PLUGIN_HOMEPAGE.as_ptr() as *const c_char,
            skylet_version_min: PLUGIN_SKYLET_MIN.as_ptr() as *const c_char,
            skylet_version_max: ptr::null(),
            abi_version: PLUGIN_ABI_VERSION.as_ptr() as *const c_char,
            dependencies: ptr::null(),
            num_dependencies: 0,
            provides_services: ptr::null(),
            num_provides_services: 0,
            requires_services: ptr::null(),
            num_requires_services: 0,
            capabilities: init_capabilities() as *const CapabilityInfo,
            num_capabilities: 9,
            min_resources: ptr::null(),
            max_resources: ptr::null(),
            tags: init_tags() as *const *const c_char,
            num_tags: 5,
            category: PluginCategory::Integration,
            supports_hot_reload: false,
            supports_async: true,
            supports_streaming: false,
            max_concurrency: 64,
            tagline: ptr::null(),
            icon_url: ptr::null(),
            maturity_level: MaturityLevel::Stable,
            build_timestamp: ptr::null(),
            build_hash: ptr::null(),
            git_commit: ptr::null(),
            build_environment: ptr::null(),
            metadata: ptr::null(),
        });
        PLUGIN_INFO.store(Box::leak(info) as *mut _, Ordering::SeqCst);
    }
    PLUGIN_INFO.load(Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn plugin_get_info_v2() -> *const PluginInfoV2 {
    init_plugin_info() as *const PluginInfoV2
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct GitOpsService {
    pub create_service: extern "C" fn(config: *const GitServiceConfig) -> GitServiceResult,
    pub start_service: extern "C" fn() -> GitServiceResult,
    pub stop_service: extern "C" fn() -> GitServiceResult,
    pub is_running: extern "C" fn() -> bool,
    pub get_status: extern "C" fn() -> *mut c_char,
}

#[repr(C)]
pub struct GitServiceConfig {
    pub enable_ssh: bool,
    pub enable_http: bool,
    pub http_listen_addr: *const c_char,
    pub ssh_port: u16,
    pub storage_path: *const c_char,
    pub lfs_enabled: bool,
}

#[repr(C)]
pub struct GitServiceResult {
    pub success: bool,
    pub error_message: *const c_char,
}

static GIT_OPS_SERVICE: Mutex<Option<Arc<GitOpsService>>> = Mutex::new(None);

#[derive(Clone, Copy)]
#[repr(C)]
pub struct S3StorageService {
    pub initialize: extern "C" fn(config: *const S3ServiceConfig) -> S3ServiceResult,
    pub create_bucket: extern "C" fn(bucket_name: *const c_char) -> S3ServiceResult,
    pub delete_bucket: extern "C" fn(bucket_name: *const c_char) -> S3ServiceResult,
    pub list_buckets: extern "C" fn() -> *mut c_char,
    pub put_object: extern "C" fn(
        bucket: *const c_char,
        key: *const c_char,
        data: *const u8,
        len: usize,
    ) -> S3ServiceResult,
    pub get_object: extern "C" fn(bucket: *const c_char, key: *const c_char) -> *mut S3ObjectResult,
    pub delete_object: extern "C" fn(bucket: *const c_char, key: *const c_char) -> S3ServiceResult,
    pub generate_presigned_url:
        extern "C" fn(bucket: *const c_char, key: *const c_char, expires_secs: u64) -> *mut c_char,
    pub is_ready: extern "C" fn() -> bool,
}

#[repr(C)]
pub struct S3ServiceConfig {
    pub endpoint: *const c_char,
    pub region: *const c_char,
    pub access_key_id: *const c_char,
    pub secret_access_key: *const c_char,
    pub bucket_prefix: *const c_char,
    pub public_url: *const c_char,
}

#[repr(C)]
pub struct S3ServiceResult {
    pub success: bool,
    pub error_message: *const c_char,
}

#[repr(C)]
pub struct S3ObjectResult {
    pub success: bool,
    pub data: *mut u8,
    pub len: usize,
    pub error_message: *const c_char,
}

static S3_OPS_SERVICE: Mutex<Option<Arc<S3StorageService>>> = Mutex::new(None);

static PLUGIN: Mutex<Option<Arc<GitOperationsPlugin>>> = Mutex::new(None);

static DISCOVERED_STORAGE_SERVICES: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

fn discover_storage_services(registry: *const ServiceRegistryV2, context: *const PluginContextV2) {
    if registry.is_null() || context.is_null() {
        return;
    }

    let registry = unsafe { &*registry };

    let service_list = (registry.list_services)(context);
    if service_list.is_null() {
        info!("No services available in service registry");
        return;
    }

    let mut discovered_count = 0;
    let mut storage_backends: Vec<(String, String)> = Vec::new();

    unsafe {
        let mut idx = 0;
        loop {
            let service_name_ptr = *service_list.add(idx);
            if service_name_ptr.is_null() {
                break;
            }

            let service_name = CStr::from_ptr(service_name_ptr)
                .to_string_lossy()
                .into_owned();

            let service_type_storage = CString::new(SERVICE_TYPE_STORAGE_V1).unwrap();
            let service_ptr =
                (registry.get)(context, service_name_ptr, service_type_storage.as_ptr());

            if !service_ptr.is_null() {
                let storage_service = &*(service_ptr as *const S3CompatibleStorageV1);

                let backend_name = storage_service
                    .get_backend_name()
                    .unwrap_or_else(|| service_name.clone());
                let backend_type = storage_service
                    .get_backend_type()
                    .unwrap_or_else(|| "unknown".to_string());

                let is_default = storage_backends.is_empty();

                let backend_ptr = Arc::new(storage_service.clone());
                global_registry().register(
                    backend_ptr,
                    None,
                    is_default,
                    if is_default { 100 } else { 50 },
                );

                storage_backends.push((backend_name.clone(), backend_type.clone()));
                discovered_count += 1;

                info!(
                    "Discovered and registered storage backend: {} (type: {}, default: {})",
                    backend_name, backend_type, is_default
                );
            }

            let s3_type = CString::new("S3StorageService").unwrap();
            let s3_ptr = (registry.get)(context, service_name_ptr, s3_type.as_ptr());

            if !s3_ptr.is_null() {
                let backend_name = service_name.clone();
                let backend_type = "s3-compatible".to_string();

                storage_backends.push((backend_name.clone(), backend_type.clone()));
                discovered_count += 1;

                info!(
                    "Discovered S3 storage service: {} (will use legacy interface)",
                    backend_name
                );
            }

            idx += 1;
        }

        (registry.free_service_list)(service_list, idx);
    }

    if let Ok(mut guard) = DISCOVERED_STORAGE_SERVICES.lock() {
        *guard = storage_backends;
    }

    info!(
        "Storage discovery complete: {} backends found",
        discovered_count
    );
}

fn register_storage_from_env() {
    let endpoint = std::env::var("REPOS_S3_ENDPOINT").ok();
    let _region = std::env::var("REPOS_S3_REGION").ok();
    let access_key = std::env::var("REPOS_S3_ACCESS_KEY").ok();
    let _secret_key = std::env::var("REPOS_S3_SECRET_KEY").ok();

    if endpoint.is_none() || access_key.is_none() {
        debug!("No S3 storage configured via environment variables");
        return;
    }

    info!("Legacy S3 storage configuration found in environment variables");
}

#[no_mangle]
pub extern "C" fn plugin_init_v2(context: *const PluginContextV2) -> PluginResultV2 {
    if context.is_null() {
        return PluginResultV2::InvalidRequest;
    }

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return PluginResultV2::Error,
    };

    let config = match crate::config::AppConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            info!("Failed to load config, using defaults: {}", e);
            crate::config::AppConfig::default()
        }
    };

    let plugin = match rt.block_on(GitOperationsPlugin::new(config)) {
        Ok(p) => Arc::new(p),
        Err(e) => {
            info!("Failed to initialize GitOperationsPlugin: {}", e);
            return PluginResultV2::Error;
        }
    };

    if let Ok(mut guard) = PLUGIN.lock() {
        *guard = Some(plugin);
    }

    unsafe {
        let ctx = &*context;
        let registry = ctx.service_registry;

        if !registry.is_null() {
            info!("Discovering storage services from Skylet service registry...");
            discover_storage_services(registry, context);

            let s3_service_name = CString::new("s3-storage").unwrap();
            let s3_service_type = CString::new("S3StorageService").unwrap();

            let s3_service_ptr =
                ((*registry).get)(context, s3_service_name.as_ptr(), s3_service_type.as_ptr());

            if !s3_service_ptr.is_null() {
                let s3_service = &*(s3_service_ptr as *const S3StorageService);

                let s3_endpoint = std::env::var("REPOS_S3_ENDPOINT")
                    .unwrap_or_else(|_| "http://localhost:3900".to_string());
                let s3_region =
                    std::env::var("REPOS_S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());
                let s3_access_key = std::env::var("REPOS_S3_ACCESS_KEY").unwrap_or_default();
                let s3_secret_key = std::env::var("REPOS_S3_SECRET_KEY").unwrap_or_default();
                let s3_bucket_prefix =
                    std::env::var("REPOS_S3_BUCKET_PREFIX").unwrap_or_else(|_| "repos".to_string());
                let s3_public_url = std::env::var("REPOS_S3_PUBLIC_URL").unwrap_or_default();

                let endpoint_cstr = CString::new(s3_endpoint).unwrap();
                let region_cstr = CString::new(s3_region).unwrap();
                let access_key_cstr = CString::new(s3_access_key).unwrap();
                let secret_key_cstr = CString::new(s3_secret_key).unwrap();
                let bucket_prefix_cstr = CString::new(s3_bucket_prefix).unwrap();
                let public_url_cstr = CString::new(s3_public_url).unwrap();

                let s3_config = S3ServiceConfig {
                    endpoint: endpoint_cstr.as_ptr(),
                    region: region_cstr.as_ptr(),
                    access_key_id: access_key_cstr.as_ptr(),
                    secret_access_key: secret_key_cstr.as_ptr(),
                    bucket_prefix: bucket_prefix_cstr.as_ptr(),
                    public_url: public_url_cstr.as_ptr(),
                };

                let s3_result = (s3_service.initialize)(&s3_config);
                if s3_result.success {
                    if let Ok(mut guard) = S3_OPS_SERVICE.lock() {
                        *guard = Some(Arc::new(s3_service.clone()));
                    }
                    info!("S3 storage service initialized for LFS");
                } else {
                    let err_msg = if !s3_result.error_message.is_null() {
                        CStr::from_ptr(s3_result.error_message)
                            .to_string_lossy()
                            .into_owned()
                    } else {
                        "unknown error".to_string()
                    };
                    info!(
                        "S3 storage service initialization failed: {} - LFS will use local storage",
                        err_msg
                    );
                }
            } else {
                info!("S3 storage service not available - LFS will use local storage");
            }
        } else {
            info!("Service registry not available - using environment-based storage configuration");
            register_storage_from_env();
        }
    }

    {
        let backends = global_registry().list_backends();
        info!(
            "Available storage backends after initialization: {:?}",
            backends
        );
    }

    if let Ok(mut guard) = RUNTIME.lock() {
        *guard = Some(rt);
    }

    PluginResultV2::Success
}

#[no_mangle]
pub extern "C" fn plugin_shutdown_v2(_context: *const PluginContextV2) -> PluginResultV2 {
    if let Ok(guard) = GIT_OPS_SERVICE.lock() {
        if let Some(ref service) = *guard {
            let _ = (service.stop_service)();
        }
    }

    if let Ok(mut guard) = GIT_OPS_SERVICE.lock() {
        *guard = None;
    }
    if let Ok(mut guard) = S3_OPS_SERVICE.lock() {
        *guard = None;
    }
    if let Ok(mut guard) = PLUGIN.lock() {
        *guard = None;
    }
    if let Ok(mut guard) = RUNTIME.lock() {
        *guard = None;
    }
    if let Ok(mut guard) = DISCOVERED_STORAGE_SERVICES.lock() {
        guard.clear();
    }

    global_registry().clear();

    PluginResultV2::Success
}

#[no_mangle]
pub extern "C" fn plugin_health_check_v2() -> HealthStatus {
    HealthStatus::Healthy
}

fn http_request_to_plugin_request(req: *const HttpRequest) -> Option<PluginRequest> {
    if req.is_null() {
        return None;
    }

    let req = unsafe { &*req };

    let path = if req.path.is_null() {
        String::new()
    } else {
        let s = unsafe { CStr::from_ptr(req.path) }
            .to_string_lossy()
            .into_owned();
        s.trim_start_matches('/').to_string()
    };

    let params = if req.body.is_null() || req.body_len == 0 {
        serde_json::Value::Object(serde_json::Map::new())
    } else {
        let body_slice = unsafe { std::slice::from_raw_parts(req.body, req.body_len) };
        serde_json::from_slice(body_slice)
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
    };

    let mut tenant_id = None;
    let mut user_id = None;
    let mut token = None;

    if !req.headers.is_null() && req.num_headers > 0 {
        let headers = unsafe { std::slice::from_raw_parts(req.headers, req.num_headers) };
        for h in headers {
            if h.name.is_null() || h.value.is_null() {
                continue;
            }
            let name = unsafe { CStr::from_ptr(h.name) }
                .to_string_lossy()
                .to_lowercase();
            let value = unsafe { CStr::from_ptr(h.value) }
                .to_string_lossy()
                .into_owned();
            match name.as_str() {
                "x-tenant-id" => tenant_id = Some(value),
                "x-user-id" => user_id = Some(value),
                "authorization" => token = Some(value),
                _ => {}
            }
        }
    }

    Some(PluginRequest {
        id: uuid::Uuid::new_v4().to_string(),
        method: path,
        params,
        tenant_id,
        user_id,
        token,
    })
}

fn plugin_response_to_http_response(resp: &PluginResponse) -> *mut HttpResponse {
    let status_code = if resp.error.is_some() { 500 } else { 200 };

    let body_json = serde_json::to_vec(resp).unwrap_or_default();

    let ct_name = CString::new("Content-Type").unwrap().into_raw();
    let ct_value = CString::new("application/json").unwrap().into_raw();

    let headers = vec![HttpHeader {
        name: ct_name,
        value: ct_value,
    }];
    let headers_boxed = headers.into_boxed_slice();
    let headers_ptr = Box::into_raw(headers_boxed) as *mut HttpHeader;

    let body_len = body_json.len();
    let body_boxed = body_json.into_boxed_slice();
    let body_ptr = Box::into_raw(body_boxed) as *mut u8;

    let http_resp = Box::new(HttpResponse {
        status_code,
        headers: headers_ptr,
        num_headers: 1,
        body: body_ptr,
        body_len,
    });

    Box::into_raw(http_resp)
}

#[no_mangle]
pub extern "C" fn plugin_handle_request_v2(
    _context: *const PluginContextV2,
    request: *const HttpRequest,
    response: *mut *mut HttpResponse,
) -> PluginResultV2 {
    if response.is_null() {
        return PluginResultV2::InvalidRequest;
    }

    let plugin: Arc<GitOperationsPlugin> = {
        let guard: std::sync::MutexGuard<'_, Option<Arc<GitOperationsPlugin>>> = match PLUGIN.lock()
        {
            Ok(g) => g,
            Err(_) => return PluginResultV2::ServiceUnavailable,
        };
        match guard.as_ref() {
            Some(p) => Arc::clone(p),
            None => return PluginResultV2::ServiceUnavailable,
        }
    };

    let plugin_req: PluginRequest = match http_request_to_plugin_request(request) {
        Some(r) => r,
        None => return PluginResultV2::InvalidRequest,
    };

    let plugin_resp: PluginResponse = {
        let guard = match RUNTIME.lock() {
            Ok(g) => g,
            Err(_) => return PluginResultV2::ServiceUnavailable,
        };
        match guard.as_ref() {
            Some(rt) => rt.block_on(plugin.handle_request(plugin_req)),
            None => return PluginResultV2::ServiceUnavailable,
        }
    };

    let http_resp = plugin_response_to_http_response(&plugin_resp);

    unsafe {
        *response = http_resp;
    }

    if plugin_resp.error.is_some() {
        PluginResultV2::Error
    } else {
        PluginResultV2::Success
    }
}

fn build_tool_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema::new("repo_create", "Create a new Git repository for a tenant")
            .required_param("tenant_id", PropertySchema::string("Tenant identifier"))
            .required_param("name", PropertySchema::string("Repository name")),
        ToolSchema::new("repo_delete", "Delete a Git repository")
            .required_param("tenant_id", PropertySchema::string("Tenant identifier"))
            .required_param("name", PropertySchema::string("Repository name")),
        ToolSchema::new("repo_list", "List all repositories for a tenant")
            .required_param("tenant_id", PropertySchema::string("Tenant identifier")),
        ToolSchema::new("repo_info", "Get information about a repository")
            .required_param("tenant_id", PropertySchema::string("Tenant identifier"))
            .required_param("name", PropertySchema::string("Repository name")),
        ToolSchema::new("health_check", "Check plugin health status"),
        ToolSchema::new("metrics_get", "Get plugin metrics"),
        ToolSchema::new(
            "storage_list_backends",
            "List all available storage backends",
        )
        .param(
            "include_details",
            PropertySchema::boolean("Include endpoint and capability details"),
        ),
        ToolSchema::new(
            "storage_list_buckets",
            "List all buckets across storage backends",
        )
        .param(
            "backend_name",
            PropertySchema::string(
                "Specific backend to query (optional, uses default if not specified)",
            ),
        ),
        ToolSchema::new(
            "storage_create_bucket",
            "Create a bucket in a specific storage backend",
        )
        .required_param(
            "bucket_name",
            PropertySchema::string("Name of the bucket to create"),
        )
        .param(
            "backend_name",
            PropertySchema::string("Backend to create in (uses tenant's backend or default)"),
        )
        .param(
            "tenant_id",
            PropertySchema::string("Tenant ID for tenant-specific bucket"),
        )
        .param(
            "versioning_enabled",
            PropertySchema::boolean("Enable versioning"),
        )
        .param(
            "public_access",
            PropertySchema::boolean("Allow public read access"),
        ),
        ToolSchema::new(
            "storage_delete_bucket",
            "Delete a bucket from a storage backend",
        )
        .required_param(
            "bucket_name",
            PropertySchema::string("Name of the bucket to delete"),
        )
        .param(
            "backend_name",
            PropertySchema::string("Backend containing the bucket"),
        ),
        ToolSchema::new(
            "storage_set_tenant_mapping",
            "Configure tenant to storage backend mapping",
        )
        .required_param("tenant_id", PropertySchema::string("Tenant identifier"))
        .required_param(
            "backend_name",
            PropertySchema::string("Storage backend to use for this tenant"),
        )
        .param(
            "bucket_prefix",
            PropertySchema::string("Prefix for all buckets created by this tenant"),
        ),
        ToolSchema::new(
            "storage_get_tenant_mapping",
            "Get the storage backend mapping for a tenant",
        )
        .required_param("tenant_id", PropertySchema::string("Tenant identifier")),
        ToolSchema::new(
            "storage_remove_tenant_mapping",
            "Remove a tenant's storage backend mapping (will use default)",
        )
        .required_param("tenant_id", PropertySchema::string("Tenant identifier")),
        ToolSchema::new(
            "storage_set_default_backend",
            "Set the default storage backend for tenants without mapping",
        )
        .required_param(
            "backend_name",
            PropertySchema::string("Name of the backend to set as default"),
        ),
    ]
}

#[no_mangle]
pub extern "C" fn plugin_get_tools() -> *const c_char {
    let schemas = build_tool_schemas();
    match serde_json::to_string(&schemas) {
        Ok(json) => match CString::new(json) {
            Ok(cs) => cs.into_raw() as *const c_char,
            Err(_) => ptr::null(),
        },
        Err(_) => ptr::null(),
    }
}

#[no_mangle]
pub extern "C" fn plugin_free_tools(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_get_info_v2_returns_valid_pointer() {
        let info_ptr = plugin_get_info_v2();
        assert!(!info_ptr.is_null());

        unsafe {
            let info = &*info_ptr;
            let name = CStr::from_ptr(info.name).to_string_lossy();
            assert_eq!(name, "git-operations");

            let author = CStr::from_ptr(info.author).to_string_lossy();
            assert_eq!(author, "Vincents AI Team");

            assert_eq!(info.category, PluginCategory::Integration);
            assert_eq!(info.num_capabilities, 9);
            assert_eq!(info.num_tags, 5);
            assert_eq!(info.num_provides_services, 0);
        }
    }

    #[test]
    fn test_plugin_get_info_v2_idempotent() {
        let p1 = plugin_get_info_v2();
        let p2 = plugin_get_info_v2();
        assert_eq!(p1, p2);
    }

    #[test]
    fn test_build_tool_schemas() {
        let schemas = build_tool_schemas();
        assert!(schemas.len() >= 6);

        let names: Vec<&str> = schemas.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"repo_create"));
        assert!(names.contains(&"repo_delete"));
        assert!(names.contains(&"repo_list"));
        assert!(names.contains(&"repo_info"));
        assert!(names.contains(&"health_check"));
        assert!(names.contains(&"metrics_get"));
        assert!(names.contains(&"storage_list_backends"));
        assert!(names.contains(&"storage_list_buckets"));
        assert!(names.contains(&"storage_create_bucket"));
        assert!(names.contains(&"storage_set_tenant_mapping"));
    }

    #[test]
    fn test_plugin_get_tools_returns_valid_json() {
        let ptr = plugin_get_tools();
        assert!(!ptr.is_null());

        unsafe {
            let cstr = CStr::from_ptr(ptr);
            let json_str = cstr.to_str().expect("valid UTF-8");
            let schemas: Vec<ToolSchema> =
                serde_json::from_str(json_str).expect("valid tool schema JSON");
            assert!(schemas.len() >= 6);

            plugin_free_tools(ptr as *mut c_char);
        }
    }

    #[test]
    fn test_plugin_health_check_v2() {
        let status = plugin_health_check_v2();
        assert_eq!(status, HealthStatus::Healthy);
    }

    #[test]
    fn test_handle_request_v2_no_plugin_initialized() {
        let mut resp_ptr: *mut HttpResponse = ptr::null_mut();
        let result = plugin_handle_request_v2(
            ptr::null(),
            ptr::null(),
            &mut resp_ptr as *mut *mut HttpResponse,
        );
        assert_eq!(result, PluginResultV2::ServiceUnavailable);
    }
}
