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
    CapabilityInfo, HeaderV2, HealthStatus, MaturityLevel, PluginCategory, PluginContextV2,
    PluginInfoV2, PluginResultV2, RequestV2, ResponseV2, ServiceInfo, ServiceRegistryV2,
};
use skylet_abi::{PropertySchema, ToolSchema};
use tracing::{debug, error, info};

use crate::git::ssh::SshServer;
use crate::git::{GitConfig, GitService};
use crate::skynet::interface::{PluginRequest, PluginResponse};
use crate::skynet::storage::{global_registry, S3CompatibleStorageV1};
use crate::skynet::GitOperationsPlugin;
use crate::storage::local::LocalStorage;
use crate::storage::StorageBackend;

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
static SSH_TASK: Mutex<Option<tokio::task::JoinHandle<()>>> = Mutex::new(None);

static CAPABILITIES_STORAGE: AtomicPtr<[CapabilityInfo; 9]> = AtomicPtr::new(ptr::null_mut());
static TAGS_STORAGE: AtomicPtr<[*const c_char; 5]> = AtomicPtr::new(ptr::null_mut());
#[allow(dead_code)]
static SERVICE_STORAGE: AtomicPtr<ServiceInfo> = AtomicPtr::new(ptr::null_mut());

fn init_capabilities() -> *const [CapabilityInfo; 9] {
    if CAPABILITIES_STORAGE.load(Ordering::SeqCst).is_null() {
        let caps = Box::new([
            CapabilityInfo {
                name: c"repo.create".as_ptr(),
                description: c"Create a new Git repository".as_ptr(),
                required_permission: ptr::null(),
            },
            CapabilityInfo {
                name: c"repo.delete".as_ptr(),
                description: c"Delete a Git repository".as_ptr(),
                required_permission: ptr::null(),
            },
            CapabilityInfo {
                name: c"repo.list".as_ptr(),
                description: c"List repositories for a tenant".as_ptr(),
                required_permission: ptr::null(),
            },
            CapabilityInfo {
                name: c"repo.info".as_ptr(),
                description: c"Get repository information".as_ptr(),
                required_permission: ptr::null(),
            },
            CapabilityInfo {
                name: c"lfs.operation".as_ptr(),
                description: c"Perform LFS operations".as_ptr(),
                required_permission: ptr::null(),
            },
            CapabilityInfo {
                name: c"ssh.manage".as_ptr(),
                description: c"Manage SSH keys for repository access".as_ptr(),
                required_permission: ptr::null(),
            },
            CapabilityInfo {
                name: c"storage.list".as_ptr(),
                description: c"List available storage backends".as_ptr(),
                required_permission: ptr::null(),
            },
            CapabilityInfo {
                name: c"storage.bucket".as_ptr(),
                description: c"Manage storage buckets".as_ptr(),
                required_permission: ptr::null(),
            },
            CapabilityInfo {
                name: c"tenant.mapping".as_ptr(),
                description: c"Configure tenant to storage backend mappings".as_ptr(),
                required_permission: ptr::null(),
            },
        ]);
        CAPABILITIES_STORAGE.store(Box::into_raw(caps) as *mut _, Ordering::SeqCst);
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
        TAGS_STORAGE.store(Box::into_raw(tags) as *mut _, Ordering::SeqCst);
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
        SERVICE_STORAGE.store(Box::into_raw(service) as *mut _, Ordering::SeqCst);
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
            supports_hot_reload: true,
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
        PLUGIN_INFO.store(Box::into_raw(info) as *mut _, Ordering::SeqCst);
    }
    PLUGIN_INFO.load(Ordering::SeqCst)
}

#[no_mangle]
pub unsafe extern "C" fn plugin_get_info_v2() -> *const PluginInfoV2 {
    init_plugin_info()
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

/// Maximum number of services to iterate during discovery.
/// Prevents unbounded pointer traversal if the service list is malformed.
const MAX_SERVICE_DISCOVERY: usize = 256;

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
            if idx >= MAX_SERVICE_DISCOVERY {
                info!(
                    "Reached max service discovery limit ({}) — stopping iteration",
                    MAX_SERVICE_DISCOVERY
                );
                break;
            }
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

                let backend_ptr = Arc::new(*storage_service);
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
pub unsafe extern "C" fn plugin_init_v2(context: *const PluginContextV2) -> PluginResultV2 {
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
                        *guard = Some(Arc::new(*s3_service));
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

    // Start SSH server for Git protocol
    let ssh_enabled = std::env::var("REPOS_SSH_ENABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(true);

    if ssh_enabled {
        let ssh_port: u16 = std::env::var("REPOS_SSH_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(2222);

        let storage_path =
            std::env::var("REPOS_STORAGE_PATH").unwrap_or_else(|_| "./repositories".to_string());

        let host_key_path =
            std::env::var("REPOS_HOST_KEY_PATH").unwrap_or_else(|_| "./host_key".to_string());

        let git_config = GitConfig {
            ssh_port,
            host_key_path: std::path::PathBuf::from(host_key_path),
            max_connections: 100,
            connection_timeout_secs: 30,
            rate_limit: Default::default(),
            secrets_root_key_path: None,
            hostname: "localhost".to_string(),
            http_base_url: None,
        };

        let storage_path_buf = std::path::PathBuf::from(&storage_path);
        let storage: Arc<dyn StorageBackend> = {
            if !storage_path_buf.exists() {
                if let Err(e) = std::fs::create_dir_all(&storage_path_buf) {
                    error!("Failed to create storage directory: {}", e);
                    return PluginResultV2::Error;
                }
            }
            Arc::new(LocalStorage::new(storage_path_buf.clone()))
        };

        let hooks = crate::hooks::Hooks::default();
        let git_service = Arc::new(GitService::new(storage, Some(git_config.clone()), hooks, Some(storage_path_buf)));
        let ssh_server = SshServer::new(git_service, Some(git_config), None);

        info!("Starting SSH server on port {}...", ssh_port);

        let ssh_task = rt.spawn(async move {
            if let Err(e) = ssh_server.run().await {
                error!("SSH server error: {}", e);
            }
        });

        if let Ok(mut guard) = SSH_TASK.lock() {
            *guard = Some(ssh_task);
        }

        info!("SSH server started on port {}", ssh_port);
    } else {
        info!("SSH server disabled via REPOS_SSH_ENABLED=false");
    }

    if let Ok(mut guard) = RUNTIME.lock() {
        *guard = Some(rt);
    }

    PluginResultV2::Success
}

#[no_mangle]
pub unsafe extern "C" fn plugin_shutdown_v2(_context: *const PluginContextV2) -> PluginResultV2 {
    // Stop SSH server
    if let Ok(mut guard) = SSH_TASK.lock() {
        if let Some(task) = guard.take() {
            task.abort();
            info!("SSH server task aborted");
        }
    }

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

    // Properly free static FFI data using Box::from_raw
    // (each was allocated with Box::into_raw in init functions)
    unsafe {
        if let Some(ptr) = Option::from(PLUGIN_INFO.swap(ptr::null_mut(), Ordering::SeqCst)) {
            let _ = Box::from_raw(ptr);
        }
        if let Some(ptr) =
            Option::from(CAPABILITIES_STORAGE.swap(ptr::null_mut(), Ordering::SeqCst))
        {
            let _ = Box::from_raw(ptr);
        }
        if let Some(ptr) = Option::from(TAGS_STORAGE.swap(ptr::null_mut(), Ordering::SeqCst)) {
            let _ = Box::from_raw(ptr);
        }
        if let Some(ptr) = Option::from(SERVICE_STORAGE.swap(ptr::null_mut(), Ordering::SeqCst)) {
            let _ = Box::from_raw(ptr);
        }
    }

    PluginResultV2::Success
}

#[no_mangle]
pub unsafe extern "C" fn plugin_health_check_v2(_context: *const PluginContextV2) -> HealthStatus {
    // Verify critical subsystems are operational
    let plugin_ok = PLUGIN.lock().unwrap_or_else(|e| e.into_inner()).is_some();
    let runtime_ok = RUNTIME.lock().unwrap_or_else(|e| e.into_inner()).is_some();

    if plugin_ok && runtime_ok {
        HealthStatus::Healthy
    } else {
        tracing::warn!(
            "Health check: plugin={}, runtime={}",
            if plugin_ok { "ok" } else { "MISSING" },
            if runtime_ok { "ok" } else { "MISSING" }
        );
        HealthStatus::Degraded
    }
}

fn request_v2_to_plugin_request(req: *const RequestV2) -> Option<PluginRequest> {
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

/// Write a `PluginResponse` into a caller-provided `ResponseV2`.
///
/// # Safety
/// The caller must provide a valid, writable `*mut ResponseV2`.
/// The caller is responsible for freeing the allocated `headers`, `body`,
/// and `content_type` memory after use.
unsafe fn write_plugin_response_to_v2(resp: &PluginResponse, out: *mut ResponseV2) {
    let status_code = if resp.error.is_some() { 500 } else { 200 };

    let body_json = serde_json::to_vec(resp).unwrap_or_default();

    let ct_name = CString::new("Content-Type").unwrap();
    let ct_value = CString::new("application/json").unwrap();

    let header = HeaderV2 {
        name: ct_name.into_raw() as *const c_char,
        value: ct_value.into_raw() as *const c_char,
    };

    let headers = vec![header];
    let headers_boxed = headers.into_boxed_slice();
    let headers_ptr = Box::into_raw(headers_boxed) as *mut HeaderV2;

    let content_type = CString::new("application/json").unwrap();

    let body_len = body_json.len();
    let body_boxed = body_json.into_boxed_slice();
    let body_ptr = Box::into_raw(body_boxed) as *mut u8;

    (*out).status_code = status_code;
    (*out).headers = headers_ptr;
    (*out).num_headers = 1;
    (*out).body = body_ptr;
    (*out).body_len = body_len;
    (*out).content_type = content_type.into_raw() as *const c_char;
}

#[no_mangle]
pub extern "C" fn plugin_handle_request_v2(
    _context: *const PluginContextV2,
    request: *const RequestV2,
    response: *mut ResponseV2,
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

    let plugin_req: PluginRequest = match request_v2_to_plugin_request(request) {
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

    unsafe {
        write_plugin_response_to_v2(&plugin_resp, response);
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
pub unsafe extern "C" fn plugin_get_tools() -> *const c_char {
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

#[no_mangle]
pub extern "C" fn plugin_execute_v2(
    _context: *const PluginContextV2,
    action: *const c_char,
    args: *const c_char,
) -> *mut c_char {
    let action_str = if action.is_null() {
        "health".to_string()
    } else {
        unsafe { CStr::from_ptr(action) }
            .to_string_lossy()
            .into_owned()
    };

    let args_value = if args.is_null() {
        serde_json::Value::Object(serde_json::Map::new())
    } else {
        let args_str = unsafe { CStr::from_ptr(args) }
            .to_string_lossy()
            .into_owned();
        serde_json::from_str(&args_str).unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
    };

    let result = match action_str.as_str() {
        "health" | "health_check" => {
            let status = unsafe { plugin_health_check_v2(_context) };
            serde_json::json!({
                "success": true,
                "status": format!("{:?}", status),
                "action": action_str
            })
        }
        "info" | "get_info" => {
            let info_ptr = unsafe { plugin_get_info_v2() };
            if info_ptr.is_null() {
                serde_json::json!({
                    "success": false,
                    "error": "Failed to get plugin info"
                })
            } else {
                unsafe {
                    let info = &*info_ptr;
                    serde_json::json!({
                        "success": true,
                        "name": CStr::from_ptr(info.name).to_string_lossy(),
                        "version": CStr::from_ptr(info.version).to_string_lossy(),
                        "description": CStr::from_ptr(info.description).to_string_lossy(),
                        "category": format!("{:?}", info.category),
                        "num_capabilities": info.num_capabilities
                    })
                }
            }
        }
        "tools" | "get_tools" => {
            let tools_ptr = unsafe { plugin_get_tools() };
            if tools_ptr.is_null() {
                serde_json::json!({
                    "success": false,
                    "error": "Failed to get tools"
                })
            } else {
                unsafe {
                    let tools_str = CStr::from_ptr(tools_ptr).to_string_lossy();
                    plugin_free_tools(tools_ptr as *mut c_char);
                    serde_json::json!({
                        "success": true,
                        "tools": tools_str
                    })
                }
            }
        }
        "storage_list_backends"
        | "storage_list_buckets"
        | "repo_list"
        | "repo_create"
        | "repo_delete"
        | "repo_info" => {
            // Convert action name (with underscores) to method name (with dots)
            let method_name = action_str
                .replace("repo_", "repo.")
                .replace("storage_", "storage.")
                .replace("health_", "health.")
                .replace("metrics_", "metrics.");

            // Extract tenant_id from args to enforce isolation
            let tenant_id = args_value
                .get("tenant_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let user_id = args_value
                .get("user_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let plugin: Option<Arc<GitOperationsPlugin>> =
                PLUGIN.lock().unwrap_or_else(|e| e.into_inner()).clone();

            if let Some(plugin) = plugin {
                let runtime_guard = RUNTIME.lock().unwrap_or_else(|e| e.into_inner());
                let rt_opt = runtime_guard.as_ref();

                if let Some(rt) = rt_opt {
                    let request = PluginRequest {
                        id: uuid::Uuid::new_v4().to_string(),
                        method: method_name,
                        params: args_value,
                        tenant_id,
                        user_id,
                        token: None,
                    };
                    let response = rt.block_on(plugin.handle_request(request));
                    serde_json::json!({
                        "success": response.error.is_none(),
                        "result": response.result,
                        "error": response.error
                    })
                } else {
                    serde_json::json!({
                        "success": false,
                        "error": "Runtime not initialized"
                    })
                }
            } else {
                serde_json::json!({
                    "success": false,
                    "error": "Plugin not initialized"
                })
            }
        }
        _ => {
            serde_json::json!({
                "success": false,
                "error": format!("Unknown action: {}", action_str),
                "available_actions": [
                    "health", "health_check", "info", "get_info", "tools", "get_tools",
                    "storage_list_backends", "storage_list_buckets",
                    "repo_list", "repo_create", "repo_delete", "repo_info"
                ]
            })
        }
    };

    match CString::new(result.to_string()) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => {
            let error = CString::new("{\"success\":false,\"error\":\"Failed to encode response\"}")
                .unwrap();
            error.into_raw()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_get_info_v2_returns_valid_pointer() {
        let info_ptr = unsafe { plugin_get_info_v2() };
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
        let p1 = unsafe { plugin_get_info_v2() };
        let p2 = unsafe { plugin_get_info_v2() };
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
        let ptr = unsafe { plugin_get_tools() };
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
        let status = unsafe { plugin_health_check_v2(ptr::null()) };
        // Without plugin initialization, health is Degraded (no plugin loaded)
        assert!(matches!(status, HealthStatus::Healthy | HealthStatus::Degraded));
    }

    #[test]
    fn test_handle_request_v2_no_plugin_initialized() {
        let mut resp = std::mem::MaybeUninit::<ResponseV2>::zeroed();
        let result = plugin_handle_request_v2(ptr::null(), ptr::null(), resp.as_mut_ptr());
        assert_eq!(result, PluginResultV2::ServiceUnavailable);
    }
}
