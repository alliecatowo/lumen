//! Tool dispatch interface for external tool invocations.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("tool not found: {0}")]
    NotFound(String),
    #[error("tool invocation failed: {0}")]
    InvocationFailed(String),
    #[error("policy violation: {0}")]
    PolicyViolation(String),
    #[error("rate limit exceeded for tool: {0}")]
    RateLimit(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest {
    pub tool_id: String,
    pub version: String,
    pub args: serde_json::Value,
    pub policy: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponse {
    pub outputs: serde_json::Value,
    pub latency_ms: u64,
}

/// Tool dispatch trait — implementations handle HTTP, MCP, or built-in tool calls.
pub trait ToolDispatcher: Send + Sync {
    fn dispatch(&self, request: &ToolRequest) -> Result<ToolResponse, ToolError>;
}

/// Stub tool dispatcher for testing (returns configured responses).
pub struct StubDispatcher {
    responses: std::collections::HashMap<String, serde_json::Value>,
}

impl StubDispatcher {
    pub fn new() -> Self { Self { responses: std::collections::HashMap::new() } }

    pub fn set_response(&mut self, tool_id: &str, response: serde_json::Value) {
        self.responses.insert(tool_id.to_string(), response);
    }
}

impl ToolDispatcher for StubDispatcher {
    fn dispatch(&self, request: &ToolRequest) -> Result<ToolResponse, ToolError> {
        if let Some(response) = self.responses.get(&request.tool_id) {
            Ok(ToolResponse { outputs: response.clone(), latency_ms: 0 })
        } else {
            Err(ToolError::NotFound(request.tool_id.clone()))
        }
    }
}
