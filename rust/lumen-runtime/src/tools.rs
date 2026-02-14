//! Tool dispatch interface for external tool invocations.
//!
//! This module provides two layers:
//! - **`ToolDispatcher`** — the low-level dispatch trait used by the VM.
//! - **`ToolProvider`** — the high-level pluggable provider trait for external integrations.
//!
//! A [`ProviderRegistry`] collects named providers and implements `ToolDispatcher`,
//! so it can be plugged directly into the VM's `tool_dispatcher` slot.

use serde::{Deserialize, Serialize};
use std::{collections::HashMap, future::Future, pin::Pin, time::Instant};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("tool not found: {0}")]
    NotFound(String),
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("tool execution failed: {0}")]
    ExecutionFailed(String),
    #[error("policy violation: {0}")]
    PolicyViolation(String),
    #[error("rate limit exceeded: {message}")]
    RateLimit {
        retry_after_ms: Option<u64>,
        message: String,
    },
    #[error("authentication failed: {message}")]
    AuthError { message: String },
    #[error("model not found: {model} (provider: {provider})")]
    ModelNotFound { model: String, provider: String },
    #[error("timeout: elapsed {elapsed_ms}ms, limit {limit_ms}ms")]
    Timeout { elapsed_ms: u64, limit_ms: u64 },
    #[error("provider unavailable: {provider} ({reason})")]
    ProviderUnavailable { provider: String, reason: String },
    #[error("output validation failed: expected {expected_schema}, got {actual}")]
    OutputValidationFailed {
        expected_schema: String,
        actual: String,
    },
    #[error("provider not registered: {0}")]
    NotRegistered(String),
    // Legacy variant for backward compatibility
    #[error("tool invocation failed: {0}")]
    InvocationFailed(String),
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

/// Boxed async result used by tool dispatcher and provider async paths.
pub type ToolFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, ToolError>> + Send + 'a>>;

/// Tool dispatch trait — implementations handle HTTP, MCP, or built-in tool calls.
pub trait ToolDispatcher: Send + Sync {
    fn dispatch(&self, request: &ToolRequest) -> Result<ToolResponse, ToolError>;

    /// Async dispatch hook.
    ///
    /// Default implementation preserves backwards compatibility by delegating
    /// to sync `dispatch`.
    fn dispatch_async<'a>(&'a self, request: &'a ToolRequest) -> ToolFuture<'a, ToolResponse> {
        Box::pin(async move { self.dispatch(request) })
    }
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

/// Retry policy for tool calls.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 100,
            max_delay_ms: 10_000,
        }
    }
}

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

/// Capability supported by a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    TextGeneration,
    Chat,
    Embedding,
    Vision,
    ToolUse,
    StructuredOutput,
    Streaming,
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

    /// Async execution hook.
    ///
    /// Default implementation preserves backwards compatibility by delegating
    /// to sync `call`.
    fn call_async<'a>(&'a self, input: serde_json::Value) -> ToolFuture<'a, serde_json::Value> {
        Box::pin(async move { self.call(input) })
    }

    /// Declared effect kinds this provider may trigger.
    fn effects(&self) -> Vec<String> {
        self.schema().effects.clone()
    }

    /// Capabilities supported by this provider (default: empty).
    fn capabilities(&self) -> Vec<Capability> {
        vec![]
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

fn validate_provider_output(
    schema: &serde_json::Value,
    output: &serde_json::Value,
) -> Result<(), ToolError> {
    if let Err(reason) = validate_schema_value(schema, output, "$") {
        let expected_schema = serde_json::to_string(schema).unwrap_or_else(|_| "<schema>".into());
        let actual_output = serde_json::to_string(output).unwrap_or_else(|_| "<output>".into());
        return Err(ToolError::OutputValidationFailed {
            expected_schema,
            actual: format!("{actual_output} ({reason})"),
        });
    }
    Ok(())
}

fn validate_schema_value(
    schema: &serde_json::Value,
    value: &serde_json::Value,
    path: &str,
) -> Result<(), String> {
    let schema_obj = match schema {
        serde_json::Value::Null => return Ok(()),
        serde_json::Value::Bool(true) => return Ok(()),
        serde_json::Value::Bool(false) => {
            return Err(format!("{path}: schema is false"));
        }
        serde_json::Value::Object(map) if map.is_empty() => return Ok(()),
        serde_json::Value::Object(map) => map,
        _ => return Ok(()),
    };

    if let Some(const_value) = schema_obj.get("const") {
        if const_value != value {
            return Err(format!("{path}: value does not match const"));
        }
    }

    if let Some(enum_values) = schema_obj.get("enum").and_then(|v| v.as_array()) {
        if !enum_values.iter().any(|candidate| candidate == value) {
            return Err(format!("{path}: value is not in enum"));
        }
    }

    if let Some(type_decl) = schema_obj.get("type") {
        let type_matches = match type_decl {
            serde_json::Value::String(expected) => value_matches_type(value, expected),
            serde_json::Value::Array(candidates) => candidates
                .iter()
                .filter_map(|candidate| candidate.as_str())
                .any(|expected| value_matches_type(value, expected)),
            _ => true,
        };
        if !type_matches {
            return Err(format!(
                "{path}: expected type {}, got {}",
                type_decl,
                value_type_name(value)
            ));
        }
    }

    if let Some(obj) = value.as_object() {
        if let Some(required_fields) = schema_obj.get("required").and_then(|v| v.as_array()) {
            for required in required_fields.iter().filter_map(|field| field.as_str()) {
                if !obj.contains_key(required) {
                    return Err(format!("{path}: missing required property '{required}'"));
                }
            }
        }

        let props = schema_obj.get("properties").and_then(|v| v.as_object());
        if let Some(props) = props {
            for (name, prop_schema) in props {
                if let Some(prop_value) = obj.get(name) {
                    let prop_path = format!("{path}.{name}");
                    validate_schema_value(prop_schema, prop_value, &prop_path)?;
                }
            }
        }

        if let Some(additional) = schema_obj.get("additionalProperties") {
            for (key, extra_value) in obj {
                if props.is_some_and(|p| p.contains_key(key)) {
                    continue;
                }
                match additional {
                    serde_json::Value::Bool(true) => {}
                    serde_json::Value::Bool(false) => {
                        return Err(format!(
                            "{path}: additional property '{key}' is not allowed"
                        ));
                    }
                    schema => {
                        let prop_path = format!("{path}.{key}");
                        validate_schema_value(schema, extra_value, &prop_path)?;
                    }
                }
            }
        }
    }

    if let (Some(items_schema), Some(items)) = (schema_obj.get("items"), value.as_array()) {
        for (index, item) in items.iter().enumerate() {
            let item_path = format!("{path}[{index}]");
            validate_schema_value(items_schema, item, &item_path)?;
        }
    }

    Ok(())
}

fn value_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(number) => {
            if number.is_i64() || number.is_u64() {
                "integer"
            } else {
                "number"
            }
        }
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

fn value_matches_type(value: &serde_json::Value, expected_type: &str) -> bool {
    match expected_type {
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "string" => value.is_string(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => true,
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

        // Check capabilities (future: validate against request requirements)
        let _capabilities = provider.capabilities();

        let start = Instant::now();
        let output = provider.call(request.args.clone())?;
        let latency_ms = start.elapsed().as_millis() as u64;
        validate_provider_output(&provider.schema().output_schema, &output)?;

        Ok(ToolResponse {
            outputs: output,
            latency_ms,
        })
    }

    fn dispatch_async<'a>(&'a self, request: &'a ToolRequest) -> ToolFuture<'a, ToolResponse> {
        Box::pin(async move {
            let provider = self
                .providers
                .get(&request.tool_id)
                .ok_or_else(|| ToolError::NotRegistered(request.tool_id.clone()))?;

            // Check capabilities (future: validate against request requirements)
            let _capabilities = provider.capabilities();

            let start = Instant::now();
            let output = provider.call_async(request.args.clone()).await?;
            let latency_ms = start.elapsed().as_millis() as u64;
            validate_provider_output(&provider.schema().output_schema, &output)?;

            Ok(ToolResponse {
                outputs: output,
                latency_ms,
            })
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
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

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

    /// Provider with distinct sync/async behavior so tests can verify dispatch path.
    struct DualPathProvider {
        schema: ToolSchema,
    }

    impl DualPathProvider {
        fn new(name: &str) -> Self {
            Self {
                schema: ToolSchema {
                    name: name.to_string(),
                    description: "Provider with distinct sync/async responses".to_string(),
                    input_schema: json!({"type": "object"}),
                    output_schema: json!({"type": "object"}),
                    effects: vec!["test".to_string()],
                },
            }
        }
    }

    impl ToolProvider for DualPathProvider {
        fn name(&self) -> &str {
            &self.schema.name
        }
        fn version(&self) -> &str {
            "1.0.0"
        }
        fn schema(&self) -> &ToolSchema {
            &self.schema
        }
        fn call(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
            Ok(json!({ "path": "sync", "echo": input }))
        }
        fn call_async<'a>(&'a self, input: serde_json::Value) -> ToolFuture<'a, serde_json::Value> {
            Box::pin(async move { Ok(json!({ "path": "async", "echo": input })) })
        }
    }

    struct UnionOutputProvider {
        schema: ToolSchema,
        output: serde_json::Value,
    }

    impl UnionOutputProvider {
        fn new(name: &str, output_schema: serde_json::Value, output: serde_json::Value) -> Self {
            Self {
                schema: ToolSchema {
                    name: name.to_string(),
                    description: "Provider used for schema validation tests".to_string(),
                    input_schema: json!({"type": "object"}),
                    output_schema,
                    effects: vec!["test".to_string()],
                },
                output,
            }
        }
    }

    impl ToolProvider for UnionOutputProvider {
        fn name(&self) -> &str {
            &self.schema.name
        }
        fn version(&self) -> &str {
            "1.0.0"
        }
        fn schema(&self) -> &ToolSchema {
            &self.schema
        }
        fn call(&self, _input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
            Ok(self.output.clone())
        }
        fn call_async<'a>(
            &'a self,
            _input: serde_json::Value,
        ) -> ToolFuture<'a, serde_json::Value> {
            let output = self.output.clone();
            Box::pin(async move { Ok(output) })
        }
    }

    fn noop_waker() -> Waker {
        fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(std::ptr::null(), &VTABLE)
        }
        fn wake(_: *const ()) {}
        fn wake_by_ref(_: *const ()) {}
        fn drop(_: *const ()) {}

        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);
        // SAFETY: The no-op vtable never dereferences the data pointer.
        unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = Box::pin(future);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::thread::yield_now(),
            }
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

    #[test]
    fn registry_dispatch_rejects_schema_mismatch_output() {
        let mut reg = ProviderRegistry::new();
        reg.register(
            "bad",
            Box::new(UnionOutputProvider::new(
                "bad",
                json!({"type": "string"}),
                json!({"status": "wrong-shape"}),
            )),
        );

        let request = ToolRequest {
            tool_id: "bad".to_string(),
            version: "1.0.0".to_string(),
            args: json!({}),
            policy: json!({}),
        };

        let err = reg
            .dispatch(&request)
            .expect_err("schema mismatch should fail");
        match err {
            ToolError::OutputValidationFailed {
                expected_schema,
                actual,
            } => {
                assert!(expected_schema.contains("\"string\""));
                assert!(actual.contains("wrong-shape"));
            }
            other => panic!("expected OutputValidationFailed, got: {other}"),
        }
    }

    #[test]
    fn registry_dispatch_async_rejects_schema_mismatch_output() {
        let mut reg = ProviderRegistry::new();
        reg.register(
            "bad_async",
            Box::new(UnionOutputProvider::new(
                "bad_async",
                json!({"type": "object", "required": ["ok"]}),
                json!({"missing_ok": true}),
            )),
        );

        let request = ToolRequest {
            tool_id: "bad_async".to_string(),
            version: "1.0.0".to_string(),
            args: json!({}),
            policy: json!({}),
        };

        let err = block_on(reg.dispatch_async(&request)).expect_err("schema mismatch should fail");
        match err {
            ToolError::OutputValidationFailed { actual, .. } => {
                assert!(actual.contains("missing required property"));
            }
            other => panic!("expected OutputValidationFailed, got: {other}"),
        }
    }

    #[test]
    fn registry_dispatch_accepts_union_output_type() {
        let mut reg = ProviderRegistry::new();
        reg.register(
            "union",
            Box::new(UnionOutputProvider::new(
                "union",
                json!({"type": ["object", "string", "null"]}),
                json!("ok"),
            )),
        );

        let request = ToolRequest {
            tool_id: "union".to_string(),
            version: "1.0.0".to_string(),
            args: json!({}),
            policy: json!({}),
        };

        let response = reg
            .dispatch(&request)
            .expect("union schema should validate");
        assert_eq!(response.outputs, json!("ok"));
    }

    #[test]
    fn registry_dispatch_async_uses_provider_async_path() {
        let mut reg = ProviderRegistry::new();
        reg.register("dual", Box::new(DualPathProvider::new("dual")));

        let request = ToolRequest {
            tool_id: "dual".to_string(),
            version: "1.0.0".to_string(),
            args: json!({"hello": "world"}),
            policy: json!({}),
        };

        let sync_response = reg.dispatch(&request).unwrap();
        assert_eq!(
            sync_response.outputs,
            json!({"path": "sync", "echo": {"hello": "world"}})
        );

        let async_response = block_on(reg.dispatch_async(&request)).unwrap();
        assert_eq!(
            async_response.outputs,
            json!({"path": "async", "echo": {"hello": "world"}})
        );
    }

    #[test]
    fn registry_dispatch_async_missing_tool_returns_not_registered() {
        let reg = ProviderRegistry::new();
        let request = ToolRequest {
            tool_id: "missing".to_string(),
            version: "".to_string(),
            args: json!({}),
            policy: json!({}),
        };

        let err = block_on(reg.dispatch_async(&request)).unwrap_err();
        match err {
            ToolError::NotRegistered(name) => assert_eq!(name, "missing"),
            other => panic!("expected NotRegistered, got: {}", other),
        }
    }

    #[test]
    fn registry_dispatch_async_propagates_provider_error() {
        let mut reg = ProviderRegistry::new();
        reg.register("fail", Box::new(FailingProvider));

        let request = ToolRequest {
            tool_id: "fail".to_string(),
            version: "".to_string(),
            args: json!({}),
            policy: json!({}),
        };

        let err = block_on(reg.dispatch_async(&request)).unwrap_err();
        match err {
            ToolError::InvocationFailed(msg) => assert!(msg.contains("intentional")),
            other => panic!("expected InvocationFailed, got: {}", other),
        }
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

    // -- Error type tests --------------------------------------------------

    #[test]
    fn error_rate_limit_with_retry_after() {
        let err = ToolError::RateLimit {
            retry_after_ms: Some(5000),
            message: "Too many requests".to_string(),
        };
        let err_str = err.to_string();
        assert!(err_str.contains("rate limit exceeded"));
        assert!(err_str.contains("Too many requests"));
    }

    #[test]
    fn error_rate_limit_without_retry_after() {
        let err = ToolError::RateLimit {
            retry_after_ms: None,
            message: "Rate limited".to_string(),
        };
        assert!(err.to_string().contains("Rate limited"));
    }

    #[test]
    fn error_auth_error() {
        let err = ToolError::AuthError {
            message: "Invalid API key".to_string(),
        };
        assert!(err.to_string().contains("authentication failed"));
        assert!(err.to_string().contains("Invalid API key"));
    }

    #[test]
    fn error_model_not_found() {
        let err = ToolError::ModelNotFound {
            model: "gpt-5".to_string(),
            provider: "openai".to_string(),
        };
        let err_str = err.to_string();
        assert!(err_str.contains("model not found"));
        assert!(err_str.contains("gpt-5"));
        assert!(err_str.contains("openai"));
    }

    #[test]
    fn error_timeout() {
        let err = ToolError::Timeout {
            elapsed_ms: 35000,
            limit_ms: 30000,
        };
        let err_str = err.to_string();
        assert!(err_str.contains("timeout"));
        assert!(err_str.contains("35000"));
        assert!(err_str.contains("30000"));
    }

    #[test]
    fn error_provider_unavailable() {
        let err = ToolError::ProviderUnavailable {
            provider: "gemini".to_string(),
            reason: "Service under maintenance".to_string(),
        };
        let err_str = err.to_string();
        assert!(err_str.contains("provider unavailable"));
        assert!(err_str.contains("gemini"));
        assert!(err_str.contains("Service under maintenance"));
    }

    #[test]
    fn error_output_validation_failed() {
        let err = ToolError::OutputValidationFailed {
            expected_schema: r#"{"type": "string"}"#.to_string(),
            actual: r#"{"value": 123}"#.to_string(),
        };
        let err_str = err.to_string();
        assert!(err_str.contains("output validation failed"));
        assert!(err_str.contains("expected"));
        assert!(err_str.contains("got"));
    }

    #[test]
    fn error_invalid_args() {
        let err = ToolError::InvalidArgs("Missing required field 'prompt'".to_string());
        assert!(err.to_string().contains("invalid arguments"));
        assert!(err.to_string().contains("Missing required field"));
    }

    #[test]
    fn error_execution_failed() {
        let err = ToolError::ExecutionFailed("Network timeout".to_string());
        assert!(err.to_string().contains("execution failed"));
        assert!(err.to_string().contains("Network timeout"));
    }

    // -- Capability tests --------------------------------------------------

    #[test]
    fn capability_equality() {
        assert_eq!(Capability::TextGeneration, Capability::TextGeneration);
        assert_ne!(Capability::Chat, Capability::Embedding);
    }

    #[test]
    fn capability_in_vector() {
        let caps = [
            Capability::Chat,
            Capability::TextGeneration,
            Capability::Vision,
        ];
        assert!(caps.contains(&Capability::Chat));
        assert!(caps.contains(&Capability::Vision));
        assert!(!caps.contains(&Capability::Streaming));
    }

    // -- RetryPolicy tests -------------------------------------------------

    #[test]
    fn retry_policy_default() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_retries, 3);
        assert_eq!(policy.base_delay_ms, 100);
        assert_eq!(policy.max_delay_ms, 10_000);
    }

    #[test]
    fn retry_policy_custom() {
        let policy = RetryPolicy {
            max_retries: 5,
            base_delay_ms: 200,
            max_delay_ms: 30_000,
        };
        assert_eq!(policy.max_retries, 5);
        assert_eq!(policy.base_delay_ms, 200);
        assert_eq!(policy.max_delay_ms, 30_000);
    }
}
