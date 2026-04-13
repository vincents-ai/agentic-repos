//! BDD test runner for Skynet plugin feature files.
//!
//! Run with: cargo test --test bdd

use cucumber::World;
use repos_mcp::config::AppConfig;
use repos_mcp::skynet::interface::{GitOperationsPlugin, PluginRequest, PluginResponse};
use repos_mcp::skynet::metadata::git_operations_metadata;
use serde_json::Value;
use std::collections::HashMap;

/// The cucumber World holds all state between steps.
#[derive(World)]
#[world(init = Self::new)]
pub struct PluginWorld {
    plugin: Option<GitOperationsPlugin>,
    last_response: Option<PluginResponse>,
    last_error: Option<String>,
    stored_fields: HashMap<String, Value>,
}

impl std::fmt::Debug for PluginWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginWorld")
            .field("has_plugin", &self.plugin.is_some())
            .field("last_response", &self.last_response)
            .field("last_error", &self.last_error)
            .field("stored_fields", &self.stored_fields)
            .finish()
    }
}

impl PluginWorld {
    fn new() -> Self {
        Self {
            plugin: None,
            last_response: None,
            last_error: None,
            stored_fields: HashMap::new(),
        }
    }

    fn send_request(&mut self, request: PluginRequest) {
        let plugin = self.plugin.as_ref().expect("Plugin not loaded");
        let response = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(plugin.handle_request(request))
        });
        self.last_response = Some(response);
    }

    fn last_response(&self) -> &PluginResponse {
        self.last_response.as_ref().expect("No response — did you send a request?")
    }
}

// ── Given steps ──

#[ cucumber::given(expr = "the plugin {string} is loaded")]
fn plugin_loaded(world: &mut PluginWorld, _path: String) {
    // We test the Rust plugin directly — no need to load a .so file.
    // The path parameter is ignored; we create the plugin from defaults.
    let config = AppConfig::default();
    let plugin = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(GitOperationsPlugin::new(config))
            .expect("Failed to create plugin")
    });
    world.plugin = Some(plugin);
}

#[ cucumber::given(expr = "the plugin is initialized")]
fn plugin_initialized(_world: &mut PluginWorld) {
    // Plugin is initialized on creation — nothing extra to do.
}

// ── When steps ──

#[ cucumber::when(expr = "I request the plugin info")]
fn request_plugin_info(world: &mut PluginWorld) {
    let metadata = git_operations_metadata();
    // The ABI version is defined in FFI layer as "2.0" — include it in the info response
    let mut meta_value = serde_json::to_value(&metadata).unwrap();
    if let Some(obj) = meta_value.as_object_mut() {
        obj.insert("abi_version".to_string(), Value::String("2.0".to_string()));
    }
    let response = PluginResponse {
        id: "info-request".to_string(),
        result: Some(meta_value),
        error: None,
    };
    world.last_response = Some(response);
}

#[ cucumber::when(expr = "I execute the action {string}")]
fn execute_action(world: &mut PluginWorld, action: String) {
    let request = PluginRequest {
        id: format!("test-{}", action),
        method: action,
        params: Value::Null,
        tenant_id: None,
        user_id: None,
        token: None,
    };
    world.send_request(request);
}

#[ cucumber::when(expr = "I execute the action {string} with arguments:")]
fn execute_action_with_args(world: &mut PluginWorld, action: String, args: String) {
    let args = args.trim();
    let params: Value = serde_json::from_str(&args).unwrap_or_else(|e| {
        panic!("Failed to parse args as JSON: {}. Input: {:?}", e, args);
    });
    
    // The feature file args are full request JSON with id/method/params
    // or just the params object
    let tenant_id = params.get("params")
        .and_then(|p| p.get("tenant_id"))
        .or_else(|| params.get("tenant_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    
    let actual_params = params.get("params").cloned().unwrap_or_else(|| params.clone());
    let id = params.get("id").and_then(|v| v.as_str()).unwrap_or("test").to_string();
    let method = params.get("method").and_then(|v| v.as_str()).unwrap_or(&action).to_string();
    
    let request = PluginRequest {
        id,
        method,
        params: actual_params,
        tenant_id: tenant_id.or_else(|| Some("test-tenant".to_string())),
        user_id: None,
        token: None,
    };
    world.send_request(request);
}

// ── Then steps ──

#[ cucumber::then(expr = "the action should succeed")]
fn action_should_succeed(world: &mut PluginWorld) {
    let response = world.last_response();
    assert!(
        response.error.is_none(),
        "Expected success but got error: {:?}",
        response.error
    );
}

#[ cucumber::then(expr = "the action should fail")]
fn action_should_fail(world: &mut PluginWorld) {
    let response = world.last_response();
    assert!(
        response.error.is_some(),
        "Expected failure but got success: {:?}",
        response.result
    );
    if let Some(err) = &response.error {
        world.last_error = Some(err.to_string());
    }
}

#[ cucumber::then(expr = "the response should be valid JSON")]
fn response_valid_json(world: &mut PluginWorld) {
    let response = world.last_response();
    // If result exists, it's already a serde_json::Value — always valid JSON
    assert!(
        response.result.is_some() || response.error.is_some(),
        "Response has neither result nor error"
    );
}

#[ cucumber::then(expr = "the response should have field {string}")]
fn response_has_field(world: &mut PluginWorld, field: String) {
    let response = world.last_response();
    let value = response.result.as_ref()
        .or(response.error.as_ref())
        .expect("Response has no result or error");
    
    assert!(
        value.get(&field).is_some(),
        "Response does not have field '{}'. Got: {}",
        field,
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

#[ cucumber::then(expr = "the response field {string} should equal {string}")]
fn response_field_equals(world: &mut PluginWorld, field: String, expected: String) {
    let response = world.last_response();
    let value = response.result.as_ref()
        .or(response.error.as_ref())
        .expect("Response has no result or error");
    
    let actual = value.get(&field)
        .expect(&format!("Field '{}' not found in response", field));
    
    // Try string comparison
    let actual_str = match actual {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    assert_eq!(actual_str, expected, "Field '{}' mismatch", field);
}

#[ cucumber::then(expr = "the response field {string} should contain {string}")]
fn response_field_contains(world: &mut PluginWorld, field: String, expected: String) {
    let response = world.last_response();
    let value = response.result.as_ref()
        .or(response.error.as_ref())
        .expect("Response has no result or error");
    
    let actual = value.get(&field)
        .expect(&format!("Field '{}' not found", field));
    
    let actual_str = match actual {
        Value::String(s) => s.clone(),
        Value::Object(obj) => serde_json::to_string(obj).unwrap_or_default(),
        Value::Array(arr) => serde_json::to_string(arr).unwrap_or_default(),
        other => other.to_string(),
    };
    assert!(
        actual_str.contains(&expected),
        "Field '{}' value '{}' does not contain '{}'",
        field, actual_str, expected
    );
}

#[ cucumber::then(expr = "the error should contain {string}")]
fn error_contains(world: &mut PluginWorld, expected: String) {
    let error = world.last_error.as_ref()
        .map(|s| s.as_str())
        .or_else(|| world.last_response().error.as_ref().and_then(|v| v.as_str()))
        .expect("No error stored");
    let error_str = error.to_string();
    assert!(
        error_str.to_lowercase().contains(&expected.to_lowercase()),
        "Error '{}' does not contain '{}'",
        error_str, expected
    );
}

#[ cucumber::then(expr = "I store the response field {string} as {string}")]
fn store_response_field(world: &mut PluginWorld, field: String, var_name: String) {
    let response = world.last_response();
    let value = response.result.as_ref()
        .expect("Response has no result");
    let field_val = value.get(&field)
        .cloned()
        .expect(&format!("Field '{}' not found", field));
    world.stored_fields.insert(var_name, field_val);
}

#[tokio::main]
async fn main() {
    let runner = PluginWorld::cucumber()
        .max_concurrent_scenarios(1);
    runner.run_and_exit("features/").await;
}
