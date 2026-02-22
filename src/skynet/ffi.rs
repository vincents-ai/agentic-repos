//! FFI Bridge for Skylet Plugin ABI
//!
//! This module provides the C FFI interface required by the Skylet plugin system.
//! It bridges the async Rust `GitOperationsPlugin` to the synchronous `extern "C"`
//! ABI that Skylet's `Plugin::load()` expects.
//!
//! # Exported symbols
//!
//! - `plugin_init` — Initialize the plugin with a `PluginContext`
//! - `plugin_shutdown` — Graceful shutdown
//! - `plugin_get_info` — Return static `PluginInfo` metadata
//! - `plugin_handle_request` — Handle an `HttpRequest` and produce an `HttpResponse`
//! - `plugin_get_tools` — Return MCP tool schemas as JSON
//! - `plugin_free_tools` — Free the string returned by `plugin_get_tools`

use std::ffi::{c_char, CStr, CString};
use std::ptr;
use std::sync::{Arc, Mutex, OnceLock};

use skylet_abi::{
    HttpHeader, HttpRequest, HttpResponse, PluginContext, PluginInfo, PluginResult, PluginType,
    PropertySchema, ToolSchema,
};

use crate::config::AppConfig;
use crate::skynet::interface::{GitOperationsPlugin, PluginRequest, PluginResponse};
use crate::skynet::{PLUGIN_AUTHOR, PLUGIN_DESCRIPTION, PLUGIN_NAME, PLUGIN_VERSION};

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

/// Global plugin instance, initialized by `plugin_init`.
static PLUGIN: Mutex<Option<Arc<GitOperationsPlugin>>> = Mutex::new(None);

/// Tokio runtime for bridging async code into the synchronous FFI boundary.
static RUNTIME: Mutex<Option<tokio::runtime::Runtime>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Static CStrings for PluginInfo (must outlive the returned pointer)
// ---------------------------------------------------------------------------

/// Helper to create a leaked CString that lives for the duration of the process.
/// This is intentional: PluginInfo returns raw pointers that the host reads at
/// any time, so the backing memory must never be freed.
fn leak_cstring(s: &str) -> *const c_char {
    CString::new(s)
        .unwrap_or_else(|_| CString::new("").unwrap())
        .into_raw() as *const c_char
}

// ---------------------------------------------------------------------------
// plugin_get_info
// ---------------------------------------------------------------------------

/// Wrapper to allow PluginInfo (which contains raw pointers) in OnceLock.
///
/// SAFETY: The raw pointers in PluginInfo point to leaked CStrings that are
/// truly static and immutable for the process lifetime. The backing Vec pointers
/// for capabilities/tags/operations are also leaked and immutable.
struct PluginInfoHolder {
    info: PluginInfo,
    // Keep the backing Vecs alive (leaked via Box::leak, so they're 'static).
    // We store them here to make ownership explicit, even though they're never dropped.
    _caps: &'static [*const c_char],
    _tags: &'static [*const c_char],
    _ops: &'static [*const c_char],
}

// SAFETY: All pointers in PluginInfo point to leaked, immutable, process-lifetime memory.
unsafe impl Send for PluginInfoHolder {}
unsafe impl Sync for PluginInfoHolder {}

/// Lazily initialized plugin info.
static PLUGIN_INFO: OnceLock<PluginInfoHolder> = OnceLock::new();

fn build_plugin_info() -> PluginInfoHolder {
    let caps: Vec<*const c_char> = vec![
        leak_cstring("repo.create"),
        leak_cstring("repo.delete"),
        leak_cstring("repo.list"),
        leak_cstring("repo.info"),
        leak_cstring("health.check"),
        leak_cstring("metrics.get"),
    ];
    let caps: &'static [*const c_char] = Box::leak(caps.into_boxed_slice());

    let tags: Vec<*const c_char> = vec![
        leak_cstring("git"),
        leak_cstring("multi-tenant"),
        leak_cstring("lfs"),
        leak_cstring("ssh"),
        leak_cstring("s3"),
        leak_cstring("storage"),
    ];
    let tags: &'static [*const c_char] = Box::leak(tags.into_boxed_slice());

    let ops: Vec<*const c_char> = vec![
        leak_cstring("repo.create"),
        leak_cstring("repo.delete"),
        leak_cstring("repo.list"),
        leak_cstring("repo.info"),
        leak_cstring("health.check"),
        leak_cstring("metrics.get"),
    ];
    let ops: &'static [*const c_char] = Box::leak(ops.into_boxed_slice());

    let info = PluginInfo {
        name: leak_cstring(PLUGIN_NAME),
        version: leak_cstring(PLUGIN_VERSION),
        description: leak_cstring(PLUGIN_DESCRIPTION),
        author: leak_cstring(PLUGIN_AUTHOR),
        license: leak_cstring("AGPL-3.0"),
        homepage: leak_cstring("https://vincents.ai"),
        skynet_version_min: leak_cstring("0.2.0"),
        skynet_version_max: leak_cstring(""),
        abi_version: leak_cstring("1.0.0"),
        dependencies: ptr::null(),
        num_dependencies: 0,
        provides_services: ptr::null(),
        num_provides_services: 0,
        requires_services: ptr::null(),
        num_requires_services: 0,
        supported_operations: ops.as_ptr(),
        num_supported_operations: ops.len(),
        capabilities: caps.as_ptr(),
        num_capabilities: caps.len(),
        resource_requirements: ptr::null(),
        max_concurrency: 64,
        supports_hot_reload: false,
        supports_async: true,
        supports_streaming: false,
        plugin_type: PluginType::Application,
        tags: tags.as_ptr(),
        num_tags: tags.len(),
        build_timestamp: leak_cstring(env!("CARGO_PKG_VERSION")),
        build_hash: leak_cstring(""),
        git_commit: leak_cstring(""),
        build_environment: leak_cstring("nix"),
        metadata: leak_cstring("{}"),
    };

    PluginInfoHolder {
        info,
        _caps: caps,
        _tags: tags,
        _ops: ops,
    }
}

#[no_mangle]
pub extern "C" fn plugin_get_info() -> *const PluginInfo {
    let holder = PLUGIN_INFO.get_or_init(build_plugin_info);
    &holder.info as *const PluginInfo
}

// ---------------------------------------------------------------------------
// plugin_init
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn plugin_init(_context: *const PluginContext) -> PluginResult {
    // Build a tokio runtime for async bridging
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return PluginResult::Error,
    };

    // Initialize the plugin with default config.
    // In a production integration the config would be read from the PluginContext,
    // but for the initial shim we use defaults.
    let config = AppConfig::default();

    let plugin = match rt.block_on(GitOperationsPlugin::new(config)) {
        Ok(p) => p,
        Err(_) => return PluginResult::Error,
    };

    if let Ok(mut guard) = PLUGIN.lock() {
        *guard = Some(Arc::new(plugin));
    } else {
        return PluginResult::Error;
    }

    if let Ok(mut guard) = RUNTIME.lock() {
        *guard = Some(rt);
    } else {
        return PluginResult::Error;
    }

    PluginResult::Success
}

// ---------------------------------------------------------------------------
// plugin_shutdown
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn plugin_shutdown(_context: *const PluginContext) -> PluginResult {
    // Drop the plugin first, then the runtime
    if let Ok(mut guard) = PLUGIN.lock() {
        *guard = None;
    }
    if let Ok(mut guard) = RUNTIME.lock() {
        *guard = None;
    }
    PluginResult::Success
}

// ---------------------------------------------------------------------------
// HttpRequest -> PluginRequest bridging
// ---------------------------------------------------------------------------

/// Extract the JSON body from an `HttpRequest` and convert it to a `PluginRequest`.
///
/// The FFI `HttpRequest` carries an HTTP method + path + optional JSON body.
/// We map HTTP conventions to the internal RPC-style `PluginRequest`:
///
/// - `POST /repo.create` with JSON body `{"name":"foo"}` becomes
///   `PluginRequest { method: "repo.create", params: {"name":"foo"}, ... }`
///
/// Headers named `X-Tenant-Id`, `X-User-Id`, and `Authorization` are extracted
/// and mapped to `tenant_id`, `user_id`, and `token` respectively.
///
/// # Safety
///
/// The caller must ensure `req` is either null or points to a valid `HttpRequest`
/// with valid pointer fields (path, body, headers).
unsafe fn http_request_to_plugin_request(req: *const HttpRequest) -> Option<PluginRequest> {
    if req.is_null() {
        return None;
    }

    let req = &*req;

    // Extract path as the RPC method name.
    // Strip leading '/' if present.
    let path = if req.path.is_null() {
        String::new()
    } else {
        let s = CStr::from_ptr(req.path).to_string_lossy().into_owned();
        s.trim_start_matches('/').to_string()
    };

    // Parse body as JSON params (default to empty object).
    let params = if req.body.is_null() || req.body_len == 0 {
        serde_json::Value::Object(serde_json::Map::new())
    } else {
        let body_slice = std::slice::from_raw_parts(req.body, req.body_len);
        serde_json::from_slice(body_slice)
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
    };

    // Extract well-known headers
    let mut tenant_id = None;
    let mut user_id = None;
    let mut token = None;

    if !req.headers.is_null() && req.num_headers > 0 {
        let headers = std::slice::from_raw_parts(req.headers, req.num_headers);
        for h in headers {
            if h.name.is_null() || h.value.is_null() {
                continue;
            }
            let name = CStr::from_ptr(h.name).to_string_lossy().to_lowercase();
            let value = CStr::from_ptr(h.value).to_string_lossy().into_owned();
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

// ---------------------------------------------------------------------------
// PluginResponse -> HttpResponse bridging
// ---------------------------------------------------------------------------

/// Convert a `PluginResponse` into a heap-allocated `HttpResponse`.
///
/// The caller (Skylet host) is responsible for eventually freeing the returned
/// `HttpResponse` and its buffers.
fn plugin_response_to_http_response(resp: &PluginResponse) -> *mut HttpResponse {
    let status_code = if resp.error.is_some() { 500 } else { 200 };

    // Serialize the whole response as JSON
    let body_json = serde_json::to_vec(resp).unwrap_or_default();

    // Create Content-Type header
    let ct_name = CString::new("Content-Type").unwrap().into_raw();
    let ct_value = CString::new("application/json").unwrap().into_raw();

    let headers = vec![HttpHeader {
        name: ct_name,
        value: ct_value,
    }];
    let num_headers = headers.len();
    let headers_boxed = headers.into_boxed_slice();
    let headers_ptr = Box::into_raw(headers_boxed) as *mut HttpHeader;

    let body_len = body_json.len();
    let body_boxed = body_json.into_boxed_slice();
    let body_ptr = Box::into_raw(body_boxed) as *mut u8;

    let http_resp = Box::new(HttpResponse {
        status_code,
        headers: headers_ptr,
        num_headers,
        body: body_ptr,
        body_len,
    });

    Box::into_raw(http_resp)
}

// ---------------------------------------------------------------------------
// plugin_handle_request
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn plugin_handle_request(
    _context: *const PluginContext,
    request: *const HttpRequest,
    response: *mut *mut HttpResponse,
) -> PluginResult {
    if response.is_null() {
        return PluginResult::InvalidRequest;
    }

    // Extract plugin and runtime references under the lock, then release the lock
    // before doing any blocking work.
    let plugin = {
        let guard = match PLUGIN.lock() {
            Ok(g) => g,
            Err(_) => return PluginResult::ServiceUnavailable,
        };
        match guard.as_ref() {
            Some(p) => Arc::clone(p),
            None => return PluginResult::ServiceUnavailable,
        }
    };

    // Convert HttpRequest -> PluginRequest
    let plugin_req = match unsafe { http_request_to_plugin_request(request) } {
        Some(r) => r,
        None => return PluginResult::InvalidRequest,
    };

    // Dispatch through the async plugin, blocking on the runtime.
    // We hold the RUNTIME lock only for the duration of block_on.
    let plugin_resp = {
        let guard = match RUNTIME.lock() {
            Ok(g) => g,
            Err(_) => return PluginResult::ServiceUnavailable,
        };
        match guard.as_ref() {
            Some(rt) => rt.block_on(plugin.handle_request(plugin_req)),
            None => return PluginResult::ServiceUnavailable,
        }
    };

    // Convert PluginResponse -> HttpResponse
    let http_resp = plugin_response_to_http_response(&plugin_resp);

    unsafe {
        *response = http_resp;
    }

    if plugin_resp.error.is_some() {
        PluginResult::Error
    } else {
        PluginResult::Success
    }
}

// ---------------------------------------------------------------------------
// MCP Tool Discovery (optional ABI symbols)
// ---------------------------------------------------------------------------

/// Build MCP tool schemas for the operations this plugin supports.
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_get_info_returns_valid_pointer() {
        let info_ptr = plugin_get_info();
        assert!(!info_ptr.is_null());

        unsafe {
            let info = &*info_ptr;
            let name = CStr::from_ptr(info.name).to_string_lossy();
            assert_eq!(name, "git-operations");

            let author = CStr::from_ptr(info.author).to_string_lossy();
            assert_eq!(author, "Vincents.ai Team");

            assert_eq!(info.plugin_type, PluginType::Application);
            assert_eq!(info.num_capabilities, 6);
            assert_eq!(info.num_tags, 6);
            assert_eq!(info.num_supported_operations, 6);
        }
    }

    #[test]
    fn test_plugin_get_info_idempotent() {
        let p1 = plugin_get_info();
        let p2 = plugin_get_info();
        assert_eq!(p1, p2, "plugin_get_info should return the same pointer");
    }

    #[test]
    fn test_build_tool_schemas() {
        let schemas = build_tool_schemas();
        assert_eq!(schemas.len(), 6);

        let names: Vec<&str> = schemas.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"repo_create"));
        assert!(names.contains(&"repo_delete"));
        assert!(names.contains(&"repo_list"));
        assert!(names.contains(&"repo_info"));
        assert!(names.contains(&"health_check"));
        assert!(names.contains(&"metrics_get"));
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
            assert_eq!(schemas.len(), 6);

            // Free the string
            plugin_free_tools(ptr as *mut c_char);
        }
    }

    #[test]
    fn test_plugin_response_to_http_response_success() {
        let resp = PluginResponse::success("test-1", serde_json::json!({"status": "ok"}));
        let http_resp_ptr = plugin_response_to_http_response(&resp);
        assert!(!http_resp_ptr.is_null());

        unsafe {
            let http_resp = Box::from_raw(http_resp_ptr);
            assert_eq!(http_resp.status_code, 200);
            assert_eq!(http_resp.num_headers, 1);
            assert!(http_resp.body_len > 0);

            // Parse the body JSON
            let body = std::slice::from_raw_parts(http_resp.body, http_resp.body_len);
            let parsed: serde_json::Value = serde_json::from_slice(body).unwrap();
            assert_eq!(parsed["id"], "test-1");
            assert!(parsed["result"].is_object());

            // Clean up headers
            let headers = std::slice::from_raw_parts(http_resp.headers, http_resp.num_headers);
            for h in headers {
                let _ = CString::from_raw(h.name);
                let _ = CString::from_raw(h.value);
            }
            let _ = Box::from_raw(http_resp.headers as *mut HttpHeader);
            let _ = Box::from_raw(
                std::slice::from_raw_parts_mut(http_resp.body, http_resp.body_len).as_mut_ptr(),
            );
        }
    }

    #[test]
    fn test_plugin_response_to_http_response_error() {
        let resp = PluginResponse::error("test-2", "ERR_CODE", "something failed");
        let http_resp_ptr = plugin_response_to_http_response(&resp);
        assert!(!http_resp_ptr.is_null());

        unsafe {
            let http_resp = Box::from_raw(http_resp_ptr);
            assert_eq!(http_resp.status_code, 500);

            let body = std::slice::from_raw_parts(http_resp.body, http_resp.body_len);
            let parsed: serde_json::Value = serde_json::from_slice(body).unwrap();
            assert!(parsed["error"].is_object());

            // Clean up
            let headers = std::slice::from_raw_parts(http_resp.headers, http_resp.num_headers);
            for h in headers {
                let _ = CString::from_raw(h.name);
                let _ = CString::from_raw(h.value);
            }
            let _ = Box::from_raw(http_resp.headers as *mut HttpHeader);
            let _ = Box::from_raw(
                std::slice::from_raw_parts_mut(http_resp.body, http_resp.body_len).as_mut_ptr(),
            );
        }
    }

    #[test]
    fn test_handle_request_null_response_ptr() {
        let result = plugin_handle_request(ptr::null(), ptr::null(), ptr::null_mut());
        assert_eq!(result, PluginResult::InvalidRequest);
    }

    #[test]
    fn test_handle_request_no_plugin_initialized() {
        let mut resp_ptr: *mut HttpResponse = ptr::null_mut();
        let result = plugin_handle_request(
            ptr::null(),
            ptr::null(),
            &mut resp_ptr as *mut *mut HttpResponse,
        );
        // Plugin not initialized yet => ServiceUnavailable or InvalidRequest
        assert!(
            result == PluginResult::ServiceUnavailable || result == PluginResult::InvalidRequest
        );
    }
}
