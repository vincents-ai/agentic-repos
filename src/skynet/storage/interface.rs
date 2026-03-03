//! Unified S3-compatible storage interface for FFI integration
//!
//! This module provides a common FFI-compatible interface that ANY S3-compatible
//! storage plugin can implement (Garage, SeaweedFS, MinIO, Amazon S3, etc.).

use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr;
use std::sync::{Arc, RwLock};

pub type StorageResultCode = c_int;

pub const STORAGE_SUCCESS: StorageResultCode = 0;
pub const STORAGE_ERROR: StorageResultCode = -1;
pub const STORAGE_NOT_FOUND: StorageResultCode = -2;
pub const STORAGE_ALREADY_EXISTS: StorageResultCode = -3;
pub const STORAGE_PERMISSION_DENIED: StorageResultCode = -4;
pub const STORAGE_NOT_CONNECTED: StorageResultCode = -5;
pub const STORAGE_INVALID_CONFIG: StorageResultCode = -6;

#[repr(C)]
pub struct StorageResult {
    pub code: StorageResultCode,
    pub error_message: *const c_char,
}

impl StorageResult {
    pub fn success() -> Self {
        Self {
            code: STORAGE_SUCCESS,
            error_message: ptr::null(),
        }
    }

    pub fn error(msg: &str) -> Self {
        let cstr = CString::new(msg).unwrap_or_else(|_| CString::new("error").unwrap());
        Self {
            code: STORAGE_ERROR,
            error_message: cstr.into_raw(),
        }
    }

    pub fn not_found(msg: &str) -> Self {
        let cstr = CString::new(msg).unwrap_or_else(|_| CString::new("not found").unwrap());
        Self {
            code: STORAGE_NOT_FOUND,
            error_message: cstr.into_raw(),
        }
    }

    pub fn already_exists(msg: &str) -> Self {
        let cstr = CString::new(msg).unwrap_or_else(|_| CString::new("already exists").unwrap());
        Self {
            code: STORAGE_ALREADY_EXISTS,
            error_message: cstr.into_raw(),
        }
    }

    pub fn permission_denied(msg: &str) -> Self {
        let cstr = CString::new(msg).unwrap_or_else(|_| CString::new("permission denied").unwrap());
        Self {
            code: STORAGE_PERMISSION_DENIED,
            error_message: cstr.into_raw(),
        }
    }

    pub fn not_connected(msg: &str) -> Self {
        let cstr = CString::new(msg).unwrap_or_else(|_| CString::new("not connected").unwrap());
        Self {
            code: STORAGE_NOT_CONNECTED,
            error_message: cstr.into_raw(),
        }
    }

    pub fn invalid_config(msg: &str) -> Self {
        let cstr = CString::new(msg).unwrap_or_else(|_| CString::new("invalid config").unwrap());
        Self {
            code: STORAGE_INVALID_CONFIG,
            error_message: cstr.into_raw(),
        }
    }

    pub fn is_success(&self) -> bool {
        self.code == STORAGE_SUCCESS
    }

    pub fn get_error(&self) -> Option<String> {
        if self.error_message.is_null() {
            None
        } else {
            unsafe {
                Some(
                    CStr::from_ptr(self.error_message)
                        .to_string_lossy()
                        .into_owned(),
                )
            }
        }
    }
}

#[repr(C)]
pub struct BoolResult {
    pub value: c_int,
    pub error: StorageResult,
}

impl BoolResult {
    pub fn ok(value: bool) -> Self {
        Self {
            value: if value { 1 } else { 0 },
            error: StorageResult::success(),
        }
    }

    pub fn err(code: StorageResultCode, msg: &str) -> Self {
        Self {
            value: 0,
            error: StorageResult {
                code,
                error_message: CString::new(msg)
                    .unwrap_or_else(|_| CString::new("error").unwrap())
                    .into_raw(),
            },
        }
    }
}

#[repr(C)]
pub struct ObjectResult {
    pub data: *mut u8,
    pub len: usize,
    pub error: StorageResult,
}

impl ObjectResult {
    pub fn ok(data: Vec<u8>) -> Self {
        let boxed = data.into_boxed_slice();
        let len = boxed.len();
        let ptr = Box::into_raw(boxed) as *mut u8;
        Self {
            data: ptr,
            len,
            error: StorageResult::success(),
        }
    }

    pub fn not_found(msg: &str) -> Self {
        Self {
            data: ptr::null_mut(),
            len: 0,
            error: StorageResult::not_found(msg),
        }
    }

    pub fn err(code: StorageResultCode, msg: &str) -> Self {
        Self {
            data: ptr::null_mut(),
            len: 0,
            error: StorageResult {
                code,
                error_message: CString::new(msg)
                    .unwrap_or_else(|_| CString::new("error").unwrap())
                    .into_raw(),
            },
        }
    }
}

#[repr(C)]
pub struct BucketsResult {
    pub buckets: *mut *mut c_char,
    pub count: usize,
    pub error: StorageResult,
}

impl BucketsResult {
    pub fn ok(buckets: Vec<String>) -> Self {
        let ptrs: Vec<*mut c_char> = buckets
            .into_iter()
            .filter_map(|b| CString::new(b).ok().map(|c| c.into_raw()))
            .collect();
        let count = ptrs.len();
        let boxed = ptrs.into_boxed_slice();
        let ptr = Box::into_raw(boxed) as *mut *mut c_char;
        Self {
            buckets: ptr,
            count,
            error: StorageResult::success(),
        }
    }

    pub fn err(code: StorageResultCode, msg: &str) -> Self {
        Self {
            buckets: ptr::null_mut(),
            count: 0,
            error: StorageResult {
                code,
                error_message: CString::new(msg)
                    .unwrap_or_else(|_| CString::new("error").unwrap())
                    .into_raw(),
            },
        }
    }
}

#[repr(C)]
pub struct ObjectsResult {
    pub keys: *mut *mut c_char,
    pub count: usize,
    pub error: StorageResult,
}

impl ObjectsResult {
    pub fn ok(keys: Vec<String>) -> Self {
        let ptrs: Vec<*mut c_char> = keys
            .into_iter()
            .filter_map(|k| CString::new(k).ok().map(|c| c.into_raw()))
            .collect();
        let count = ptrs.len();
        let boxed = ptrs.into_boxed_slice();
        let ptr = Box::into_raw(boxed) as *mut *mut c_char;
        Self {
            keys: ptr,
            count,
            error: StorageResult::success(),
        }
    }

    pub fn err(code: StorageResultCode, msg: &str) -> Self {
        Self {
            keys: ptr::null_mut(),
            count: 0,
            error: StorageResult {
                code,
                error_message: CString::new(msg)
                    .unwrap_or_else(|_| CString::new("error").unwrap())
                    .into_raw(),
            },
        }
    }
}

#[repr(C)]
pub struct HealthResult {
    pub is_healthy: c_int,
    pub latency_ms: u64,
    pub error: StorageResult,
}

impl HealthResult {
    pub fn healthy(latency_ms: u64) -> Self {
        Self {
            is_healthy: 1,
            latency_ms,
            error: StorageResult::success(),
        }
    }

    pub fn unhealthy(msg: &str) -> Self {
        Self {
            is_healthy: 0,
            latency_ms: 0,
            error: StorageResult::error(msg),
        }
    }
}

#[repr(C)]
pub struct StorageConfig {
    pub endpoint_url: *const c_char,
    pub region: *const c_char,
    pub access_key_id: *const c_char,
    pub secret_access_key: *const c_char,
    pub tenant_id: *const c_char,
    pub bucket_prefix: *const c_char,
    pub public_url: *const c_char,
    pub max_connections: u32,
    pub timeout_secs: u32,
}

impl StorageConfig {
    pub fn get_endpoint(&self) -> Option<String> {
        if self.endpoint_url.is_null() {
            None
        } else {
            unsafe {
                Some(
                    CStr::from_ptr(self.endpoint_url)
                        .to_string_lossy()
                        .into_owned(),
                )
            }
        }
    }

    pub fn get_region(&self) -> Option<String> {
        if self.region.is_null() {
            None
        } else {
            unsafe { Some(CStr::from_ptr(self.region).to_string_lossy().into_owned()) }
        }
    }

    pub fn get_access_key(&self) -> Option<String> {
        if self.access_key_id.is_null() {
            None
        } else {
            unsafe {
                Some(
                    CStr::from_ptr(self.access_key_id)
                        .to_string_lossy()
                        .into_owned(),
                )
            }
        }
    }

    pub fn get_secret_key(&self) -> Option<String> {
        if self.secret_access_key.is_null() {
            None
        } else {
            unsafe {
                Some(
                    CStr::from_ptr(self.secret_access_key)
                        .to_string_lossy()
                        .into_owned(),
                )
            }
        }
    }

    pub fn get_tenant_id(&self) -> Option<String> {
        if self.tenant_id.is_null() {
            None
        } else {
            unsafe {
                Some(
                    CStr::from_ptr(self.tenant_id)
                        .to_string_lossy()
                        .into_owned(),
                )
            }
        }
    }

    pub fn get_bucket_prefix(&self) -> Option<String> {
        if self.bucket_prefix.is_null() {
            None
        } else {
            unsafe {
                Some(
                    CStr::from_ptr(self.bucket_prefix)
                        .to_string_lossy()
                        .into_owned(),
                )
            }
        }
    }

    pub fn get_public_url(&self) -> Option<String> {
        if self.public_url.is_null() {
            None
        } else {
            unsafe {
                Some(
                    CStr::from_ptr(self.public_url)
                        .to_string_lossy()
                        .into_owned(),
                )
            }
        }
    }
}

#[repr(C)]
pub struct BucketConfig {
    pub versioning_enabled: c_int,
    pub public_access: c_int,
    pub retention_days: u32,
}

impl BucketConfig {
    pub fn new() -> Self {
        Self {
            versioning_enabled: 0,
            public_access: 0,
            retention_days: 0,
        }
    }
}

impl Default for BucketConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[repr(C)]
pub struct S3Capability {
    pub name: *const c_char,
    pub description: *const c_char,
}

#[repr(C)]
pub struct S3BackendInfo {
    pub backend_name: *const c_char,
    pub backend_type: *const c_char,
    pub version: *const c_char,
    pub capabilities: *const S3Capability,
    pub num_capabilities: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct S3CompatibleStorageV1 {
    pub backend_name: *const c_char,
    pub backend_type: *const c_char,
    pub context: *mut c_void,
    pub configure: extern "C" fn(ctx: *mut c_void, config: *const StorageConfig) -> StorageResult,
    pub connect: extern "C" fn(ctx: *mut c_void) -> StorageResult,
    pub disconnect: extern "C" fn(ctx: *mut c_void) -> StorageResult,
    pub list_buckets: extern "C" fn(ctx: *mut c_void) -> BucketsResult,
    pub create_bucket: extern "C" fn(
        ctx: *mut c_void,
        name: *const c_char,
        config: *const BucketConfig,
    ) -> StorageResult,
    pub delete_bucket: extern "C" fn(ctx: *mut c_void, name: *const c_char) -> StorageResult,
    pub bucket_exists: extern "C" fn(ctx: *mut c_void, name: *const c_char) -> BoolResult,
    pub put_object: extern "C" fn(
        ctx: *mut c_void,
        bucket: *const c_char,
        key: *const c_char,
        data: *const u8,
        len: usize,
    ) -> StorageResult,
    pub get_object:
        extern "C" fn(ctx: *mut c_void, bucket: *const c_char, key: *const c_char) -> ObjectResult,
    pub delete_object:
        extern "C" fn(ctx: *mut c_void, bucket: *const c_char, key: *const c_char) -> StorageResult,
    pub list_objects: extern "C" fn(
        ctx: *mut c_void,
        bucket: *const c_char,
        prefix: *const c_char,
    ) -> ObjectsResult,
    pub health_check: extern "C" fn(ctx: *mut c_void) -> HealthResult,
    pub get_endpoint_url: extern "C" fn(ctx: *mut c_void) -> *const c_char,
    pub free_result: extern "C" fn(ctx: *mut c_void, result: *mut c_void),
}

unsafe impl Send for S3CompatibleStorageV1 {}
unsafe impl Sync for S3CompatibleStorageV1 {}

impl S3CompatibleStorageV1 {
    pub fn get_backend_name(&self) -> Option<String> {
        if self.backend_name.is_null() {
            None
        } else {
            unsafe {
                Some(
                    CStr::from_ptr(self.backend_name)
                        .to_string_lossy()
                        .into_owned(),
                )
            }
        }
    }

    pub fn get_backend_type(&self) -> Option<String> {
        if self.backend_type.is_null() {
            None
        } else {
            unsafe {
                Some(
                    CStr::from_ptr(self.backend_type)
                        .to_string_lossy()
                        .into_owned(),
                )
            }
        }
    }
}

pub type S3StoragePtr = Arc<S3CompatibleStorageV1>;

#[derive(Clone)]
pub struct RegisteredBackend {
    pub backend: S3StoragePtr,
    pub tenant_id: Option<String>,
    pub is_default: bool,
    pub priority: u32,
}

pub struct S3StorageRegistry {
    backends: RwLock<Vec<RegisteredBackend>>,
    tenant_mapping: RwLock<HashMap<String, String>>,
}

impl S3StorageRegistry {
    pub fn new() -> Self {
        Self {
            backends: RwLock::new(Vec::new()),
            tenant_mapping: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(
        &self,
        backend: S3StoragePtr,
        tenant_id: Option<String>,
        is_default: bool,
        priority: u32,
    ) {
        let registered = RegisteredBackend {
            backend,
            tenant_id,
            is_default,
            priority,
        };

        let mut backends = self.backends.write().unwrap();
        backends.push(registered);
        backends.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    pub fn register_default(&self, backend: S3StoragePtr) {
        self.register(backend, None, true, 0);
    }

    pub fn register_for_tenant(&self, backend: S3StoragePtr, tenant_id: String, priority: u32) {
        let backend_name = backend.get_backend_name().unwrap_or_default();
        self.register(backend, Some(tenant_id.clone()), false, priority);

        let mut mapping = self.tenant_mapping.write().unwrap();
        mapping.insert(tenant_id, backend_name);
    }

    pub fn get_for_tenant(&self, tenant_id: &str) -> Option<S3StoragePtr> {
        {
            let mapping = self.tenant_mapping.read().unwrap();
            if let Some(backend_name) = mapping.get(tenant_id) {
                let backends = self.backends.read().unwrap();
                for registered in backends.iter() {
                    if registered.backend.get_backend_name().as_deref()
                        == Some(backend_name.as_str())
                    {
                        return Some(registered.backend.clone());
                    }
                }
            }
        }

        {
            let backends = self.backends.read().unwrap();
            for registered in backends.iter() {
                if registered.tenant_id.as_deref() == Some(tenant_id) {
                    return Some(registered.backend.clone());
                }
            }
        }

        self.get_default()
    }

    pub fn get_default(&self) -> Option<S3StoragePtr> {
        let backends = self.backends.read().unwrap();
        for registered in backends.iter() {
            if registered.is_default {
                return Some(registered.backend.clone());
            }
        }
        backends.first().map(|r| r.backend.clone())
    }

    pub fn list_backends(&self) -> Vec<(String, String, bool)> {
        self.backends
            .read()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r.backend.get_backend_name().unwrap_or_default(),
                    r.backend.get_backend_type().unwrap_or_default(),
                    r.is_default,
                )
            })
            .collect()
    }

    pub fn unregister(&self, backend_name: &str) -> bool {
        let mut backends = self.backends.write().unwrap();
        let initial_len = backends.len();
        backends.retain(|r| r.backend.get_backend_name().as_deref() != Some(backend_name));

        let mut mapping = self.tenant_mapping.write().unwrap();
        mapping.retain(|_, v| v != backend_name);

        backends.len() != initial_len
    }

    pub fn clear(&self) {
        let mut backends = self.backends.write().unwrap();
        backends.clear();

        let mut mapping = self.tenant_mapping.write().unwrap();
        mapping.clear();
    }
}

impl Default for S3StorageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

lazy_static::lazy_static! {
    static ref GLOBAL_REGISTRY: S3StorageRegistry = S3StorageRegistry::new();
}

pub fn global_registry() -> &'static S3StorageRegistry {
    &GLOBAL_REGISTRY
}

#[no_mangle]
pub extern "C" fn s3_storage_registry_register(
    backend: *const S3CompatibleStorageV1,
    tenant_id: *const c_char,
    is_default: c_int,
    priority: u32,
) -> StorageResult {
    if backend.is_null() {
        return StorageResult::invalid_config("backend pointer is null");
    }

    let tenant = if tenant_id.is_null() {
        None
    } else {
        unsafe { Some(CStr::from_ptr(tenant_id).to_string_lossy().into_owned()) }
    };

    let backend_ref = unsafe { &*backend };
    let backend_ptr = Arc::new(*backend_ref);

    global_registry().register(backend_ptr, tenant, is_default != 0, priority);

    StorageResult::success()
}

#[no_mangle]
pub extern "C" fn s3_storage_registry_get_for_tenant(
    tenant_id: *const c_char,
) -> *const S3CompatibleStorageV1 {
    if tenant_id.is_null() {
        return ptr::null();
    }

    let tenant = unsafe { CStr::from_ptr(tenant_id).to_string_lossy().into_owned() };

    match global_registry().get_for_tenant(&tenant) {
        Some(backend) => Arc::into_raw(backend) as *const S3CompatibleStorageV1,
        None => ptr::null(),
    }
}

#[no_mangle]
pub extern "C" fn s3_storage_registry_get_default() -> *const S3CompatibleStorageV1 {
    match global_registry().get_default() {
        Some(backend) => Arc::into_raw(backend) as *const S3CompatibleStorageV1,
        None => ptr::null(),
    }
}

#[no_mangle]
pub extern "C" fn s3_storage_registry_unregister(backend_name: *const c_char) -> c_int {
    if backend_name.is_null() {
        return 0;
    }

    let name = unsafe { CStr::from_ptr(backend_name).to_string_lossy().into_owned() };

    if global_registry().unregister(&name) {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn s3_storage_free_object_result(result: *mut ObjectResult) {
    if result.is_null() {
        return;
    }

    unsafe {
        let result = &mut *result;
        if !result.data.is_null() && result.len > 0 {
            let _ = Vec::from_raw_parts(result.data, result.len, result.len);
        }
        if !result.error.error_message.is_null() {
            let _ = CString::from_raw(result.error.error_message as *mut c_char);
        }
    }
}

#[no_mangle]
pub extern "C" fn s3_storage_free_buckets_result(result: *mut BucketsResult) {
    if result.is_null() {
        return;
    }

    unsafe {
        let result = &mut *result;
        if !result.buckets.is_null() && result.count > 0 {
            let slice = std::slice::from_raw_parts_mut(result.buckets, result.count);
            for ptr in slice {
                if !ptr.is_null() {
                    let _ = CString::from_raw(*ptr);
                }
            }
            let _ = Vec::from_raw_parts(result.buckets, result.count, result.count);
        }
        if !result.error.error_message.is_null() {
            let _ = CString::from_raw(result.error.error_message as *mut c_char);
        }
    }
}

#[no_mangle]
pub extern "C" fn s3_storage_free_objects_result(result: *mut ObjectsResult) {
    if result.is_null() {
        return;
    }

    unsafe {
        let result = &mut *result;
        if !result.keys.is_null() && result.count > 0 {
            let slice = std::slice::from_raw_parts_mut(result.keys, result.count);
            for ptr in slice {
                if !ptr.is_null() {
                    let _ = CString::from_raw(*ptr);
                }
            }
            let _ = Vec::from_raw_parts(result.keys, result.count, result.count);
        }
        if !result.error.error_message.is_null() {
            let _ = CString::from_raw(result.error.error_message as *mut c_char);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_result_success() {
        let result = StorageResult::success();
        assert!(result.is_success());
        assert!(result.get_error().is_none());
    }

    #[test]
    fn test_storage_result_error() {
        let result = StorageResult::error("test error");
        assert!(!result.is_success());
        assert_eq!(result.get_error(), Some("test error".to_string()));
    }

    #[test]
    fn test_bool_result() {
        let ok_true = BoolResult::ok(true);
        assert_eq!(ok_true.value, 1);
        assert!(ok_true.error.is_success());

        let ok_false = BoolResult::ok(false);
        assert_eq!(ok_false.value, 0);
        assert!(ok_false.error.is_success());
    }

    #[test]
    fn test_object_result() {
        let data = vec![1u8, 2, 3, 4, 5];
        let result = ObjectResult::ok(data);
        assert!(result.error.is_success());
        assert_eq!(result.len, 5);
    }

    fn make_test_backend(name: &str, backend_type: &str) -> S3CompatibleStorageV1 {
        // Use Box::leak to give each call its own stable pointer,
        // avoiding shared static mut state across parallel tests.
        let name_ptr: &'static CStr = Box::leak(CString::new(name).unwrap().into_boxed_c_str());
        let type_ptr: &'static CStr =
            Box::leak(CString::new(backend_type).unwrap().into_boxed_c_str());

        // Standalone extern "C" functions for FFI fn pointers
        extern "C" fn noop_configure(
            _ctx: *mut c_void,
            _config: *const StorageConfig,
        ) -> StorageResult {
            StorageResult::success()
        }
        extern "C" fn noop_connect(_ctx: *mut c_void) -> StorageResult {
            StorageResult::success()
        }
        extern "C" fn noop_disconnect(_ctx: *mut c_void) -> StorageResult {
            StorageResult::success()
        }
        extern "C" fn noop_list_buckets(_ctx: *mut c_void) -> BucketsResult {
            BucketsResult::ok(vec![])
        }
        extern "C" fn noop_create_bucket(
            _ctx: *mut c_void,
            _name: *const c_char,
            _config: *const BucketConfig,
        ) -> StorageResult {
            StorageResult::success()
        }
        extern "C" fn noop_delete_bucket(_ctx: *mut c_void, _name: *const c_char) -> StorageResult {
            StorageResult::success()
        }
        extern "C" fn noop_bucket_exists(_ctx: *mut c_void, _name: *const c_char) -> BoolResult {
            BoolResult::ok(false)
        }
        extern "C" fn noop_put_object(
            _ctx: *mut c_void,
            _bucket: *const c_char,
            _key: *const c_char,
            _data: *const u8,
            _len: usize,
        ) -> StorageResult {
            StorageResult::success()
        }
        extern "C" fn noop_get_object(
            _ctx: *mut c_void,
            _bucket: *const c_char,
            _key: *const c_char,
        ) -> ObjectResult {
            ObjectResult::not_found("not found")
        }
        extern "C" fn noop_delete_object(
            _ctx: *mut c_void,
            _bucket: *const c_char,
            _key: *const c_char,
        ) -> StorageResult {
            StorageResult::success()
        }
        extern "C" fn noop_list_objects(
            _ctx: *mut c_void,
            _bucket: *const c_char,
            _prefix: *const c_char,
        ) -> ObjectsResult {
            ObjectsResult::ok(vec![])
        }
        extern "C" fn noop_health_check(_ctx: *mut c_void) -> HealthResult {
            HealthResult::healthy(0)
        }
        extern "C" fn noop_get_endpoint_url(_ctx: *mut c_void) -> *const c_char {
            ptr::null()
        }
        extern "C" fn noop_free_result(_ctx: *mut c_void, _result: *mut c_void) {}

        S3CompatibleStorageV1 {
            backend_name: name_ptr.as_ptr(),
            backend_type: type_ptr.as_ptr(),
            context: ptr::null_mut(),
            configure: noop_configure,
            connect: noop_connect,
            disconnect: noop_disconnect,
            list_buckets: noop_list_buckets,
            create_bucket: noop_create_bucket,
            delete_bucket: noop_delete_bucket,
            bucket_exists: noop_bucket_exists,
            put_object: noop_put_object,
            get_object: noop_get_object,
            delete_object: noop_delete_object,
            list_objects: noop_list_objects,
            health_check: noop_health_check,
            get_endpoint_url: noop_get_endpoint_url,
            free_result: noop_free_result,
        }
    }

    #[test]
    fn test_registry_registration() {
        let registry = S3StorageRegistry::new();

        let backend = make_test_backend("test-backend", "garage");
        registry.register_default(Arc::new(backend));

        let default = registry.get_default();
        assert!(default.is_some());
    }

    #[test]
    fn test_registry_tenant_lookup() {
        let registry = S3StorageRegistry::new();

        let backend = make_test_backend("tenant1-storage", "garage");
        registry.register_for_tenant(Arc::new(backend), "tenant-1".to_string(), 10);

        let found = registry.get_for_tenant("tenant-1");
        assert!(found.is_some());

        // Unknown tenant falls back to first registered backend (by design).
        // Only an empty registry returns None.
        let empty_registry = S3StorageRegistry::new();
        let not_found = empty_registry.get_for_tenant("tenant-2");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_registry_fallback_to_default() {
        let registry = S3StorageRegistry::new();

        let default_backend = make_test_backend("default-storage", "local");
        registry.register_default(Arc::new(default_backend));

        let result = registry.get_for_tenant("unknown-tenant");
        assert!(result.is_some());
    }
}
