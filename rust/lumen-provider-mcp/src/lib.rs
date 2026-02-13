//! MCP (Model Context Protocol) bridge provider for Lumen.
//!
//! This provider allows Lumen programs to call any MCP-compatible tool server.
//! It implements the ToolProvider trait by discovering tools from an MCP server
//! and forwarding tool calls via JSON-RPC.

use lumen_runtime::tools::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// MCP Transport abstraction
// ---------------------------------------------------------------------------

/// Transport layer for MCP communication.
/// Implementations handle the actual communication mechanism (stdio, HTTP, etc.).
pub trait McpTransport: Send + Sync {
    /// Send a JSON-RPC request to the MCP server.
    fn send_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
}

// ---------------------------------------------------------------------------
// MCP Tool Schema
// ---------------------------------------------------------------------------

/// Schema for an MCP tool as returned by the tools/list endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolSchema {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Stdio Transport
// ---------------------------------------------------------------------------

// TODO: implement McpTransport trait for StdioTransport — currently only stores config
/// Stdio-based MCP transport that spawns a subprocess.
///
/// In a real implementation, this would manage the child process lifecycle,
/// send requests via stdin, and read responses from stdout.
/// For now, this is a placeholder that stores configuration.
pub struct StdioTransport {
    command: String,
    args: Vec<String>,
}

impl StdioTransport {
    pub fn new(command: &str, args: &[&str]) -> Self {
        Self {
            command: command.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[allow(dead_code)]
    pub fn command(&self) -> &str {
        &self.command
    }

    #[allow(dead_code)]
    pub fn args(&self) -> &[String] {
        &self.args
    }
}

// ---------------------------------------------------------------------------
// Mock Transport (for testing)
// ---------------------------------------------------------------------------

/// Mock transport for testing that returns pre-configured responses.
pub struct MockTransport {
    responses: HashMap<String, serde_json::Value>,
}

impl MockTransport {
    pub fn new() -> Self {
        Self {
            responses: HashMap::new(),
        }
    }

    /// Configure a response for a given method.
    pub fn set_response(&mut self, method: &str, response: serde_json::Value) {
        self.responses.insert(method.to_string(), response);
    }
}

impl Default for MockTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl McpTransport for MockTransport {
    fn send_request(
        &self,
        method: &str,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.responses
            .get(method)
            .cloned()
            .ok_or_else(|| format!("No mock response for method: {}", method))
    }
}

// ---------------------------------------------------------------------------
// MCP Provider (single tool)
// ---------------------------------------------------------------------------

/// A single tool provider backed by an MCP server.
///
/// Each MCP tool is wrapped in its own provider instance with a qualified name
/// (server_name.tool_name). Multiple tools from the same server are registered
/// as separate providers in the ProviderRegistry.
pub struct McpToolProvider {
    server_name: String,
    tool_schema: McpToolSchema,
    transport: std::sync::Arc<dyn McpTransport>,
}

impl McpToolProvider {
    pub fn new(
        server_name: &str,
        tool_schema: McpToolSchema,
        transport: std::sync::Arc<dyn McpTransport>,
    ) -> Self {
        Self {
            server_name: server_name.to_string(),
            tool_schema,
            transport,
        }
    }

    /// Get the qualified tool name (server_name.tool_name).
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.server_name, self.tool_schema.name)
    }

    /// Convert MCP tool schema to Lumen ToolSchema.
    fn to_lumen_schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.qualified_name(),
            description: self
                .tool_schema
                .description
                .clone()
                .unwrap_or_default(),
            input_schema: self.tool_schema.input_schema.clone(),
            output_schema: serde_json::Value::Null,
            effects: vec!["mcp".to_string()],
        }
    }
}

impl ToolProvider for McpToolProvider {
    fn name(&self) -> &str {
        &self.server_name
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn schema(&self) -> &ToolSchema {
        // We need to return a reference, but we're computing it dynamically.
        // For now, we'll leak a boxed value (acceptable for tool providers
        // which are typically long-lived).
        Box::leak(Box::new(self.to_lumen_schema()))
    }

    fn call(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        // Forward the call to the MCP server using the unqualified tool name.
        let params = serde_json::json!({
            "name": self.tool_schema.name,
            "arguments": input,
        });

        self.transport
            .send_request("tools/call", params)
            .map_err(|e| ToolError::InvocationFailed(e))
    }

    fn effects(&self) -> Vec<String> {
        vec!["mcp".to_string()]
    }
}

// ---------------------------------------------------------------------------
// MCP Server Discovery
// ---------------------------------------------------------------------------

/// Discover all tools from an MCP server and create providers for each.
pub fn discover_tools(
    server_name: &str,
    transport: std::sync::Arc<dyn McpTransport>,
) -> Result<Vec<McpToolProvider>, String> {
    // Send tools/list request to discover available tools.
    let response = transport.send_request("tools/list", serde_json::json!({}))?;

    let tools = response
        .get("tools")
        .and_then(|t| t.as_array())
        .ok_or_else(|| "tools/list response missing 'tools' array".to_string())?;

    let mut providers = Vec::new();
    for tool_value in tools {
        match serde_json::from_value::<McpToolSchema>(tool_value.clone()) {
            Ok(schema) => {
                providers.push(McpToolProvider::new(server_name, schema, transport.clone()));
            }
            Err(e) => {
                eprintln!("Warning: failed to parse tool schema: {}", e);
            }
        }
    }

    Ok(providers)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mock_transport_with_tools(tools: Vec<serde_json::Value>) -> MockTransport {
        let mut transport = MockTransport::new();
        transport.set_response("tools/list", json!({ "tools": tools }));
        transport
    }

    #[test]
    fn mock_transport_works() {
        let mut transport = MockTransport::new();
        transport.set_response("test", json!({"result": "ok"}));

        let result = transport.send_request("test", json!({})).unwrap();
        assert_eq!(result, json!({"result": "ok"}));
    }

    #[test]
    fn mock_transport_returns_error_for_unknown_method() {
        let transport = MockTransport::new();
        let err = transport.send_request("unknown", json!({})).unwrap_err();
        assert!(err.contains("No mock response"));
    }

    #[test]
    fn discover_tools_from_mock_transport() {
        let tools = vec![
            json!({
                "name": "search",
                "description": "Search the web",
                "input_schema": {"type": "object"}
            }),
            json!({
                "name": "fetch",
                "description": "Fetch a URL",
                "input_schema": {"type": "object"}
            }),
        ];

        let transport = std::sync::Arc::new(mock_transport_with_tools(tools));
        let providers = discover_tools("test_server", transport).unwrap();

        assert_eq!(providers.len(), 2);
        assert_eq!(providers[0].tool_schema.name, "search");
        assert_eq!(providers[1].tool_schema.name, "fetch");
    }

    #[test]
    fn discover_tools_handles_empty_list() {
        let transport = std::sync::Arc::new(mock_transport_with_tools(vec![]));
        let providers = discover_tools("empty_server", transport).unwrap();
        assert_eq!(providers.len(), 0);
    }

    #[test]
    fn mcp_tool_provider_metadata() {
        let schema = McpToolSchema {
            name: "test_tool".to_string(),
            description: Some("A test tool".to_string()),
            input_schema: json!({"type": "string"}),
        };

        let transport = std::sync::Arc::new(MockTransport::new());
        let provider = McpToolProvider::new("server", schema, transport);

        assert_eq!(provider.name(), "server");
        assert_eq!(provider.version(), "0.1.0");
        assert_eq!(provider.qualified_name(), "server.test_tool");
    }

    #[test]
    fn mcp_tool_provider_schema_generation() {
        let schema = McpToolSchema {
            name: "my_tool".to_string(),
            description: Some("Description".to_string()),
            input_schema: json!({"type": "object"}),
        };

        let transport = std::sync::Arc::new(MockTransport::new());
        let provider = McpToolProvider::new("srv", schema, transport);
        let lumen_schema = provider.schema();

        assert_eq!(lumen_schema.name, "srv.my_tool");
        assert_eq!(lumen_schema.description, "Description");
        assert_eq!(lumen_schema.effects, vec!["mcp"]);
    }

    #[test]
    fn mcp_tool_provider_effects_list() {
        let schema = McpToolSchema {
            name: "tool".to_string(),
            description: None,
            input_schema: json!({}),
        };

        let transport = std::sync::Arc::new(MockTransport::new());
        let provider = McpToolProvider::new("srv", schema, transport);

        assert_eq!(provider.effects(), vec!["mcp"]);
    }

    #[test]
    fn mcp_tool_provider_forwards_call() {
        let schema = McpToolSchema {
            name: "echo".to_string(),
            description: None,
            input_schema: json!({}),
        };

        let mut transport = MockTransport::new();
        transport.set_response("tools/call", json!({"result": "success"}));

        let provider =
            McpToolProvider::new("srv", schema, std::sync::Arc::new(transport));
        let result = provider.call(json!({"input": "data"})).unwrap();

        assert_eq!(result, json!({"result": "success"}));
    }

    #[test]
    fn mcp_tool_provider_call_error_handling() {
        let schema = McpToolSchema {
            name: "failing".to_string(),
            description: None,
            input_schema: json!({}),
        };

        // Transport with no configured response will fail.
        let transport = std::sync::Arc::new(MockTransport::new());
        let provider = McpToolProvider::new("srv", schema, transport);

        let err = provider.call(json!({})).unwrap_err();
        match err {
            ToolError::InvocationFailed(msg) => {
                assert!(msg.contains("No mock response"));
            }
            other => panic!("Expected InvocationFailed, got: {:?}", other),
        }
    }

    #[test]
    fn stdio_transport_stores_configuration() {
        let transport = StdioTransport::new("node", &["server.js", "--port", "3000"]);
        assert_eq!(transport.command(), "node");
        assert_eq!(transport.args(), &["server.js", "--port", "3000"]);
    }
}
