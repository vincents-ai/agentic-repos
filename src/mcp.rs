use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MCPMessage {
    #[serde(rename = "initialize")]
    Initialize { id: String, params: InitParams },

    #[serde(rename = "initialized")]
    Initialized,

    #[serde(rename = "call_tool")]
    CallTool { id: String, params: ToolCallParams },

    #[serde(rename = "result")]
    Result { id: String, result: Value },

    #[serde(rename = "error")]
    Error { id: String, error: ErrorDetails },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitParams {
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    #[serde(default)]
    pub client_info: Option<ClientInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCapabilities {
    #[serde(default)]
    pub tools: ToolCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolCapabilities {
    #[serde(default)]
    pub list: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetails {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListToolsResponse {
    pub tools: Vec<ToolDefinition>,
}
