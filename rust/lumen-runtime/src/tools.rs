//! Tool dispatch interface for external tool invocations.
//!
//! This module provides two layers:
//! - **`ToolDispatcher`** — the low-level dispatch trait used by the VM.
//! - **`ToolProvider`** — the high-level pluggable provider trait for external integrations.
//!
//! A [`ProviderRegistry`] collects named providers and implements `ToolDispatcher`,
//! so it can be plugged directly into the VM's `tool_dispatcher` slot.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

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
    #[error("provider not registered: {0}")]
    NotRegistered(String),
}

// ---------------------------------------------------------------------------
// Low-level dispatch (consumed by the VM)
// ---------------------------------------------------------------------------

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
#[derive(Default)]
pub struct StubDispatcher {
    responses: HashMap<String, serde_json::Value>,
}

impl StubDispatcher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_response(&mut self, tool_id: &str, response: serde_json::Value) {
        self.responses.insert(tool_id.to_string(), response);
    }
}

impl ToolDispatcher for StubDispatcher {
    fn dispatch(&self, request: &ToolRequest) -> Result<ToolResponse, ToolError> {
        if let Some(response) = self.responses.get(&request.tool_id) {
            Ok(ToolResponse {
                outputs: response.clone(),
                latency_ms: 0,
            })
        } else {
            Err(ToolError::NotFound(request.tool_id.clone()))
        }
    }
}

// ---------------------------------------------------------------------------
// High-level pluggable provider trait
// ---------------------------------------------------------------------------

/// Schema describing a tool's input/output types and declared effects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's input.
    pub input_schema: serde_json::Value,
    /// JSON Schema for the tool's output.
    pub output_schema: serde_json::Value,
    /// Declared effect kinds (e.g. `["http", "trace"]`).
    pub effects: Vec<String>,
}

/// A pluggable tool provider. Implementations live in separate crates
/// (e.g. an HTTP provider, an MCP provider, a mock provider).
pub trait ToolProvider: Send + Sync {
    /// Human-readable provider name (e.g. `"openai"`, `"anthropic"`).
    fn name(&self) -> &str;

    /// Semver version of the provider implementation.
    fn version(&self) -> &str;

    /// Schema describing the tool this provider exposes.
    fn schema(&self) -> &ToolSchema;

    /// Execute the tool with the given JSON input, returning JSON output.
    fn call(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError>;

    /// Declared effect kinds this provider may trigger.
    fn effects(&self) -> Vec<String> {
        self.schema().effects.clone()
    }
}

// ---------------------------------------------------------------------------
// NullProvider — returns an error for unregistered tools
// ---------------------------------------------------------------------------

/// A sentinel provider that always returns `ToolError::NotRegistered`.
/// Used as a placeholder when no real provider has been registered for a tool.
pub struct NullProvider {
    tool_name: String,
    schema: ToolSchema,
}

impl NullProvider {
    pub fn new(tool_name: &str) -> Self {
        Self {
            tool_name: tool_name.to_string(),
            schema: ToolSchema {
                name: tool_name.to_string(),
                description: format!("Unregistered tool: {}", tool_name),
                input_schema: serde_json::Value::Null,
                output_schema: serde_json::Value::Null,
                effects: vec![],
            },
        }
    }
}

impl ToolProvider for NullProvider {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn version(&self) -> &str {
        "0.0.0"
    }

    fn schema(&self) -> &ToolSchema {
        &self.schema
    }

    fn call(&self, _input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        Err(ToolError::NotRegistered(self.tool_name.clone()))
    }
}

// ---------------------------------------------------------------------------
// ProviderRegistry
// ---------------------------------------------------------------------------

/// A registry of named tool providers. Implements `ToolDispatcher` so it can
/// be plugged directly into the VM.
pub struct ProviderRegistry {
    providers: HashMap<String, Box<dyn ToolProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Register a provider under the given name, replacing any previous one.
    pub fn register(&mut self, name: &str, provider: Box<dyn ToolProvider>) {
        self.providers.insert(name.to_string(), provider);
    }

    /// Look up a provider by name.
    pub fn get(&self, name: &str) -> Option<&dyn ToolProvider> {
        self.providers.get(name).map(|p| p.as_ref())
    }

    /// Return the names of all registered providers.
    pub fn list(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.providers.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }

    /// Check whether a provider is registered under the given name.
    pub fn has(&self, name: &str) -> bool {
        self.providers.contains_key(name)
    }

    /// Remove a provider by name, returning `true` if it existed.
    pub fn unregister(&mut self, name: &str) -> bool {
        self.providers.remove(name).is_some()
    }

    /// Number of registered providers.
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// The registry doubles as a `ToolDispatcher`.  It resolves `request.tool_id`
/// to a registered provider, forwards the call, and wraps the result in a
/// `ToolResponse`.
impl ToolDispatcher for ProviderRegistry {
    fn dispatch(&self, request: &ToolRequest) -> Result<ToolResponse, ToolError> {
        let provider = self
            .providers
            .get(&request.tool_id)
            .ok_or_else(|| ToolError::NotRegistered(request.tool_id.clone()))?;

        let start = std::time::Instant::now();
        let output = provider.call(request.args.clone())?;
        let latency_ms = start.elapsed().as_millis() as u64;

        Ok(ToolResponse {
            outputs: output,
            latency_ms,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- helpers ----------------------------------------------------------

    /// A simple in-memory provider for testing.
    struct EchoProvider {
        provider_name: String,
        provider_version: String,
        schema: ToolSchema,
    }

    impl EchoProvider {
        fn new(name: &str) -> Self {
            Self {
                provider_name: name.to_string(),
                provider_version: "1.0.0".to_string(),
                schema: ToolSchema {
                    name: name.to_string(),
                    description: format!("Echo provider: {}", name),
                    input_schema: json!({"type": "object"}),
                    output_schema: json!({"type": "object"}),
                    effects: vec!["echo".to_string()],
                },
            }
        }
    }

    impl ToolProvider for EchoProvider {
        fn name(&self) -> &str {
            &self.provider_name
        }
        fn version(&self) -> &str {
            &self.provider_version
        }
        fn schema(&self) -> &ToolSchema {
            &self.schema
        }
        fn call(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
            Ok(json!({ "echo": input }))
        }
    }

    /// A provider that always fails.
    struct FailingProvider;

    impl ToolProvider for FailingProvider {
        fn name(&self) -> &str {
            "failing"
        }
        fn version(&self) -> &str {
            "0.1.0"
        }
        fn schema(&self) -> &ToolSchema {
            // Leak a static schema for testing convenience.
            // (Tests don't care about the small allocation.)
            Box::leak(Box::new(ToolSchema {
                name: "failing".to_string(),
                description: "Always fails".to_string(),
                input_schema: json!({}),
                output_schema: json!({}),
                effects: vec![],
            }))
        }
        fn call(&self, _input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
            Err(ToolError::InvocationFailed("intentional failure".into()))
        }
    }

    // -- ToolProvider trait ------------------------------------------------

    #[test]
    fn echo_provider_returns_wrapped_input() {
        let provider = EchoProvider::new("test_echo");
        let result = provider.call(json!({"x": 1})).unwrap();
        assert_eq!(result, json!({"echo": {"x": 1}}));
    }

    #[test]
    fn provider_metadata_accessors() {
        let provider = EchoProvider::new("my_tool");
        assert_eq!(provider.name(), "my_tool");
        assert_eq!(provider.version(), "1.0.0");
        assert_eq!(provider.schema().name, "my_tool");
        assert_eq!(provider.effects(), vec!["echo".to_string()]);
    }

    // -- NullProvider -----------------------------------------------------

    #[test]
    fn null_provider_returns_not_registered() {
        let null = NullProvider::new("missing_tool");
        assert_eq!(null.name(), "missing_tool");
        assert_eq!(null.version(), "0.0.0");
        let err = null.call(json!({})).unwrap_err();
        match err {
            ToolError::NotRegistered(name) => assert_eq!(name, "missing_tool"),
            other => panic!("expected NotRegistered, got: {}", other),
        }
    }

    #[test]
    fn null_provider_schema_describes_unregistered() {
        let null = NullProvider::new("xyz");
        let schema = null.schema();
        assert_eq!(schema.name, "xyz");
        assert!(schema.description.contains("Unregistered"));
        assert_eq!(schema.input_schema, serde_json::Value::Null);
        assert!(schema.effects.is_empty());
    }

    // -- ProviderRegistry -------------------------------------------------

    #[test]
    fn registry_starts_empty() {
        let reg = ProviderRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
        assert!(reg.list().is_empty());
    }

    #[test]
    fn registry_register_and_get() {
        let mut reg = ProviderRegistry::new();
        reg.register("echo", Box::new(EchoProvider::new("echo")));
        assert!(reg.has("echo"));
        assert!(!reg.has("other"));
        assert_eq!(reg.len(), 1);

        let provider = reg.get("echo").unwrap();
        assert_eq!(provider.name(), "echo");
    }

    #[test]
    fn registry_get_missing_returns_none() {
        let reg = ProviderRegistry::new();
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn registry_list_returns_sorted_names() {
        let mut reg = ProviderRegistry::new();
        reg.register("zebra", Box::new(EchoProvider::new("zebra")));
        reg.register("alpha", Box::new(EchoProvider::new("alpha")));
        reg.register("mid", Box::new(EchoProvider::new("mid")));
        assert_eq!(reg.list(), vec!["alpha", "mid", "zebra"]);
    }

    #[test]
    fn registry_replace_existing_provider() {
        let mut reg = ProviderRegistry::new();
        reg.register("tool", Box::new(EchoProvider::new("v1")));
        assert_eq!(reg.get("tool").unwrap().name(), "v1");

        reg.register("tool", Box::new(EchoProvider::new("v2")));
        assert_eq!(reg.get("tool").unwrap().name(), "v2");
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn registry_unregister() {
        let mut reg = ProviderRegistry::new();
        reg.register("tool", Box::new(EchoProvider::new("tool")));
        assert!(reg.unregister("tool"));
        assert!(!reg.has("tool"));
        assert!(!reg.unregister("tool")); // second time returns false
    }

    #[test]
    fn registry_multiple_providers() {
        let mut reg = ProviderRegistry::new();
        reg.register("a", Box::new(EchoProvider::new("a")));
        reg.register("b", Box::new(EchoProvider::new("b")));
        reg.register("c", Box::new(EchoProvider::new("c")));
        assert_eq!(reg.len(), 3);
        assert!(reg.has("a"));
        assert!(reg.has("b"));
        assert!(reg.has("c"));
    }

    // -- ProviderRegistry as ToolDispatcher --------------------------------

    #[test]
    fn registry_dispatches_to_registered_provider() {
        let mut reg = ProviderRegistry::new();
        reg.register("echo", Box::new(EchoProvider::new("echo")));

        let request = ToolRequest {
            tool_id: "echo".to_string(),
            version: "1.0.0".to_string(),
            args: json!({"hello": "world"}),
            policy: json!({}),
        };
        let response = reg.dispatch(&request).unwrap();
        assert_eq!(response.outputs, json!({"echo": {"hello": "world"}}));
    }

    #[test]
    fn registry_dispatch_missing_tool_returns_not_registered() {
        let reg = ProviderRegistry::new();
        let request = ToolRequest {
            tool_id: "missing".to_string(),
            version: "".to_string(),
            args: json!({}),
            policy: json!({}),
        };
        let err = reg.dispatch(&request).unwrap_err();
        match err {
            ToolError::NotRegistered(name) => assert_eq!(name, "missing"),
            other => panic!("expected NotRegistered, got: {}", other),
        }
    }

    #[test]
    fn registry_dispatch_propagates_provider_error() {
        let mut reg = ProviderRegistry::new();
        reg.register("fail", Box::new(FailingProvider));

        let request = ToolRequest {
            tool_id: "fail".to_string(),
            version: "".to_string(),
            args: json!({}),
            policy: json!({}),
        };
        let err = reg.dispatch(&request).unwrap_err();
        match err {
            ToolError::InvocationFailed(msg) => assert!(msg.contains("intentional")),
            other => panic!("expected InvocationFailed, got: {}", other),
        }
    }

    #[test]
    fn registry_dispatch_measures_latency() {
        let mut reg = ProviderRegistry::new();
        reg.register("echo", Box::new(EchoProvider::new("echo")));

        let request = ToolRequest {
            tool_id: "echo".to_string(),
            version: "1.0.0".to_string(),
            args: json!({}),
            policy: json!({}),
        };
        let response = reg.dispatch(&request).unwrap();
        // Latency should be very small but non-negative
        assert!(response.latency_ms < 1000);
    }

    // -- Provider schema access -------------------------------------------

    #[test]
    fn provider_schema_round_trip() {
        let provider = EchoProvider::new("my_tool");
        let schema = provider.schema();
        assert_eq!(schema.name, "my_tool");
        assert_eq!(schema.description, "Echo provider: my_tool");
        assert_eq!(schema.input_schema, json!({"type": "object"}));
        assert_eq!(schema.output_schema, json!({"type": "object"}));
        assert_eq!(schema.effects, vec!["echo"]);
    }

    #[test]
    fn schema_serialization() {
        let schema = ToolSchema {
            name: "test".to_string(),
            description: "A test tool".to_string(),
            input_schema: json!({"type": "string"}),
            output_schema: json!({"type": "number"}),
            effects: vec!["io".to_string()],
        };
        let json_str = serde_json::to_string(&schema).unwrap();
        let roundtrip: ToolSchema = serde_json::from_str(&json_str).unwrap();
        assert_eq!(roundtrip.name, "test");
        assert_eq!(roundtrip.effects, vec!["io"]);
    }

    // -- Default trait impl -----------------------------------------------

    #[test]
    fn registry_default_is_empty() {
        let reg = ProviderRegistry::default();
        assert!(reg.is_empty());
    }

    // -- StubDispatcher (pre-existing, verify still works) -----------------

    #[test]
    fn stub_dispatcher_returns_configured_response() {
        let mut stub = StubDispatcher::new();
        stub.set_response("tool_a", json!({"ok": true}));
        let req = ToolRequest {
            tool_id: "tool_a".to_string(),
            version: "1".to_string(),
            args: json!({}),
            policy: json!({}),
        };
        let resp = stub.dispatch(&req).unwrap();
        assert_eq!(resp.outputs, json!({"ok": true}));
        assert_eq!(resp.latency_ms, 0);
    }

    #[test]
    fn stub_dispatcher_returns_not_found_for_unknown() {
        let stub = StubDispatcher::new();
        let req = ToolRequest {
            tool_id: "unknown".to_string(),
            version: "".to_string(),
            args: json!({}),
            policy: json!({}),
        };
        let err = stub.dispatch(&req).unwrap_err();
        match err {
            ToolError::NotFound(name) => assert_eq!(name, "unknown"),
            other => panic!("expected NotFound, got: {}", other),
        }
    }
}
