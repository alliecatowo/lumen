//! Mock effect handler for testing Lumen code that uses algebraic effects.
//!
//! Provides a [`MockEffectHandler`] to register canned responses for effect
//! operations, and a [`MockToolDispatcher`] that implements `ToolDispatcher`
//! while recording all calls for post-hoc verification.

use crate::tools::{ToolDispatcher, ToolError, ToolRequest, ToolResponse};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Type aliases for complex function types
// ---------------------------------------------------------------------------

/// A dynamic handler for mock effect operations.
type EffectHandlerFn = Box<dyn Fn(&[serde_json::Value]) -> serde_json::Value>;

/// A dynamic handler for mock tool dispatch.
type ToolHandlerFn =
    Box<dyn Fn(&ToolRequest) -> Result<serde_json::Value, ToolError> + Send + Sync>;

// ---------------------------------------------------------------------------
// Effect key
// ---------------------------------------------------------------------------

/// Composite key identifying an effect operation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EffectKey {
    effect: String,
    operation: String,
}

impl EffectKey {
    fn new(effect: &str, operation: &str) -> Self {
        Self {
            effect: effect.to_string(),
            operation: operation.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// MockEffectHandler
// ---------------------------------------------------------------------------

/// Registers mock responses for effect operations and tracks call counts.
pub struct MockEffectHandler {
    /// Static (constant) responses.
    static_responses: HashMap<EffectKey, serde_json::Value>,
    /// Dynamic (function-based) handlers.
    dynamic_handlers: HashMap<EffectKey, EffectHandlerFn>,
    /// Number of times each effect operation was invoked.
    call_counts: HashMap<EffectKey, usize>,
    /// Ordered log of all calls.
    call_log: Vec<(EffectKey, Vec<serde_json::Value>)>,
}

impl MockEffectHandler {
    /// Create a new, empty mock handler.
    pub fn new() -> Self {
        Self {
            static_responses: HashMap::new(),
            dynamic_handlers: HashMap::new(),
            call_counts: HashMap::new(),
            call_log: Vec::new(),
        }
    }

    /// Register a static mock response for a given effect and operation.
    pub fn when(
        &mut self,
        effect: &str,
        operation: &str,
        response: serde_json::Value,
    ) -> &mut Self {
        let key = EffectKey::new(effect, operation);
        self.static_responses.insert(key, response);
        self
    }

    /// Register a dynamic mock handler for a given effect and operation.
    pub fn when_fn(
        &mut self,
        effect: &str,
        operation: &str,
        handler: EffectHandlerFn,
    ) -> &mut Self {
        let key = EffectKey::new(effect, operation);
        self.dynamic_handlers.insert(key, handler);
        self
    }

    /// Dispatch an effect operation, returning the mock response.
    ///
    /// Dynamic handlers take precedence over static responses.
    pub fn handle(
        &mut self,
        effect: &str,
        operation: &str,
        args: &[serde_json::Value],
    ) -> Option<serde_json::Value> {
        let key = EffectKey::new(effect, operation);

        // Record call.
        *self.call_counts.entry(key.clone()).or_insert(0) += 1;
        self.call_log.push((key.clone(), args.to_vec()));

        // Dynamic handler first.
        if let Some(handler) = self.dynamic_handlers.get(&key) {
            return Some(handler(args));
        }
        // Then static response.
        self.static_responses.get(&key).cloned()
    }

    /// Check if a particular effect operation was ever invoked.
    pub fn verify_called(&self, effect: &str, operation: &str) -> bool {
        let key = EffectKey::new(effect, operation);
        self.call_counts.get(&key).copied().unwrap_or(0) > 0
    }

    /// Get the number of times a particular effect operation was invoked.
    pub fn call_count(&self, effect: &str, operation: &str) -> usize {
        let key = EffectKey::new(effect, operation);
        self.call_counts.get(&key).copied().unwrap_or(0)
    }

    /// Get the ordered call log.
    pub fn call_log(&self) -> &[(EffectKey, Vec<serde_json::Value>)] {
        &self.call_log
    }

    /// Reset all mocks, call counts, and logs.
    pub fn reset(&mut self) -> &mut Self {
        self.static_responses.clear();
        self.dynamic_handlers.clear();
        self.call_counts.clear();
        self.call_log.clear();
        self
    }
}

impl Default for MockEffectHandler {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// MockToolDispatcher
// ---------------------------------------------------------------------------

/// A tool dispatcher backed by mock responses. Implements `ToolDispatcher` so
/// it can be plugged into the VM in place of a real provider registry.
///
/// Calls are recorded for later verification.
pub struct MockToolDispatcher {
    /// Static tool responses keyed by tool_id.
    responses: HashMap<String, serde_json::Value>,
    /// Dynamic tool handlers keyed by tool_id.
    handlers: HashMap<String, ToolHandlerFn>,
    /// Ordered log of dispatched requests.
    log: std::sync::Mutex<Vec<ToolRequest>>,
}

impl MockToolDispatcher {
    /// Create a new mock dispatcher.
    pub fn new() -> Self {
        Self {
            responses: HashMap::new(),
            handlers: HashMap::new(),
            log: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Register a static response for a given tool ID.
    pub fn when(&mut self, tool_id: &str, response: serde_json::Value) -> &mut Self {
        self.responses.insert(tool_id.to_string(), response);
        self
    }

    /// Register a dynamic handler for a given tool ID.
    pub fn when_fn(&mut self, tool_id: &str, handler: ToolHandlerFn) -> &mut Self {
        self.handlers.insert(tool_id.to_string(), handler);
        self
    }

    /// Get the ordered log of all dispatched requests.
    pub fn call_log(&self) -> Vec<ToolRequest> {
        self.log.lock().unwrap().clone()
    }

    /// Number of total calls dispatched.
    pub fn call_count(&self) -> usize {
        self.log.lock().unwrap().len()
    }

    /// Number of calls for a specific tool ID.
    pub fn call_count_for(&self, tool_id: &str) -> usize {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.tool_id == tool_id)
            .count()
    }

    /// Check if a specific tool was ever called.
    pub fn was_called(&self, tool_id: &str) -> bool {
        self.call_count_for(tool_id) > 0
    }

    /// Reset all mocks and logs.
    pub fn reset(&mut self) -> &mut Self {
        self.responses.clear();
        self.handlers.clear();
        self.log.lock().unwrap().clear();
        self
    }
}

impl Default for MockToolDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolDispatcher for MockToolDispatcher {
    fn dispatch(&self, request: &ToolRequest) -> Result<ToolResponse, ToolError> {
        // Record.
        self.log.lock().unwrap().push(request.clone());

        // Dynamic handler first.
        if let Some(handler) = self.handlers.get(&request.tool_id) {
            let output = handler(request)?;
            return Ok(ToolResponse {
                outputs: output,
                latency_ms: 0,
            });
        }

        // Static response.
        if let Some(response) = self.responses.get(&request.tool_id) {
            return Ok(ToolResponse {
                outputs: response.clone(),
                latency_ms: 0,
            });
        }

        Err(ToolError::NotFound(request.tool_id.clone()))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- MockEffectHandler --------------------------------------------------

    #[test]
    fn new_handler_has_no_mocks() {
        let handler = MockEffectHandler::new();
        assert!(!handler.verify_called("http", "get"));
        assert_eq!(handler.call_count("http", "get"), 0);
    }

    #[test]
    fn static_mock_returns_response() {
        let mut handler = MockEffectHandler::new();
        handler.when("http", "get", json!({"status": 200}));
        let result = handler.handle("http", "get", &[]);
        assert_eq!(result, Some(json!({"status": 200})));
    }

    #[test]
    fn static_mock_missing_operation_returns_none() {
        let mut handler = MockEffectHandler::new();
        handler.when("http", "get", json!({}));
        let result = handler.handle("http", "post", &[]);
        assert_eq!(result, None);
    }

    #[test]
    fn dynamic_mock_receives_args() {
        let mut handler = MockEffectHandler::new();
        handler.when_fn(
            "math",
            "add",
            Box::new(|args| {
                let a = args.first().and_then(|v| v.as_i64()).unwrap_or(0);
                let b = args.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
                json!(a + b)
            }),
        );
        let result = handler.handle("math", "add", &[json!(3), json!(4)]);
        assert_eq!(result, Some(json!(7)));
    }

    #[test]
    fn dynamic_mock_takes_precedence_over_static() {
        let mut handler = MockEffectHandler::new();
        handler.when("http", "get", json!("static"));
        handler.when_fn("http", "get", Box::new(|_| json!("dynamic")));
        let result = handler.handle("http", "get", &[]);
        assert_eq!(result, Some(json!("dynamic")));
    }

    #[test]
    fn verify_called_tracks_invocations() {
        let mut handler = MockEffectHandler::new();
        handler.when("db", "query", json!([]));
        assert!(!handler.verify_called("db", "query"));
        handler.handle("db", "query", &[]);
        assert!(handler.verify_called("db", "query"));
    }

    #[test]
    fn call_count_increments() {
        let mut handler = MockEffectHandler::new();
        handler.when("io", "read", json!("data"));
        handler.handle("io", "read", &[]);
        handler.handle("io", "read", &[]);
        handler.handle("io", "read", &[]);
        assert_eq!(handler.call_count("io", "read"), 3);
    }

    #[test]
    fn call_count_for_unregistered_is_zero() {
        let handler = MockEffectHandler::new();
        assert_eq!(handler.call_count("nonexistent", "op"), 0);
    }

    #[test]
    fn reset_clears_everything() {
        let mut handler = MockEffectHandler::new();
        handler.when("http", "get", json!({}));
        handler.handle("http", "get", &[]);
        assert!(handler.verify_called("http", "get"));

        handler.reset();
        assert!(!handler.verify_called("http", "get"));
        assert_eq!(handler.call_count("http", "get"), 0);
        assert!(handler.call_log().is_empty());
        // Static mock should also be cleared.
        let result = handler.handle("http", "get", &[]);
        assert_eq!(result, None);
    }

    #[test]
    fn call_log_records_order() {
        let mut handler = MockEffectHandler::new();
        handler.when("a", "op1", json!(1));
        handler.when("b", "op2", json!(2));
        handler.handle("a", "op1", &[json!("arg1")]);
        handler.handle("b", "op2", &[json!("arg2")]);
        let log = handler.call_log();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].0.effect, "a");
        assert_eq!(log[0].0.operation, "op1");
        assert_eq!(log[1].0.effect, "b");
        assert_eq!(log[1].0.operation, "op2");
    }

    // -- MockToolDispatcher -------------------------------------------------

    #[test]
    fn mock_dispatcher_returns_static_response() {
        let mut dispatcher = MockToolDispatcher::new();
        dispatcher.when("echo", json!({"ok": true}));
        let request = ToolRequest {
            tool_id: "echo".to_string(),
            version: "1".to_string(),
            args: json!({}),
            policy: json!({}),
        };
        let response = dispatcher.dispatch(&request).unwrap();
        assert_eq!(response.outputs, json!({"ok": true}));
        assert_eq!(response.latency_ms, 0);
    }

    #[test]
    fn mock_dispatcher_returns_not_found_for_unknown() {
        let dispatcher = MockToolDispatcher::new();
        let request = ToolRequest {
            tool_id: "unknown".to_string(),
            version: "".to_string(),
            args: json!({}),
            policy: json!({}),
        };
        let err = dispatcher.dispatch(&request).unwrap_err();
        match err {
            ToolError::NotFound(name) => assert_eq!(name, "unknown"),
            other => panic!("expected NotFound, got: {}", other),
        }
    }

    #[test]
    fn mock_dispatcher_dynamic_handler() {
        let mut dispatcher = MockToolDispatcher::new();
        dispatcher.when_fn(
            "adder",
            Box::new(|req| {
                let a = req.args.get("a").and_then(|v| v.as_i64()).unwrap_or(0);
                let b = req.args.get("b").and_then(|v| v.as_i64()).unwrap_or(0);
                Ok(json!({"sum": a + b}))
            }),
        );
        let request = ToolRequest {
            tool_id: "adder".to_string(),
            version: "1".to_string(),
            args: json!({"a": 3, "b": 7}),
            policy: json!({}),
        };
        let response = dispatcher.dispatch(&request).unwrap();
        assert_eq!(response.outputs, json!({"sum": 10}));
    }

    #[test]
    fn mock_dispatcher_dynamic_handler_takes_precedence() {
        let mut dispatcher = MockToolDispatcher::new();
        dispatcher.when("tool", json!("static"));
        dispatcher.when_fn("tool", Box::new(|_| Ok(json!("dynamic"))));
        let request = ToolRequest {
            tool_id: "tool".to_string(),
            version: "1".to_string(),
            args: json!({}),
            policy: json!({}),
        };
        let response = dispatcher.dispatch(&request).unwrap();
        assert_eq!(response.outputs, json!("dynamic"));
    }

    #[test]
    fn mock_dispatcher_records_calls() {
        let mut dispatcher = MockToolDispatcher::new();
        dispatcher.when("a", json!(1));
        dispatcher.when("b", json!(2));
        let req_a = ToolRequest {
            tool_id: "a".to_string(),
            version: "1".to_string(),
            args: json!({"x": 1}),
            policy: json!({}),
        };
        let req_b = ToolRequest {
            tool_id: "b".to_string(),
            version: "1".to_string(),
            args: json!({"y": 2}),
            policy: json!({}),
        };
        dispatcher.dispatch(&req_a).unwrap();
        dispatcher.dispatch(&req_b).unwrap();
        dispatcher.dispatch(&req_a).unwrap();

        assert_eq!(dispatcher.call_count(), 3);
        assert_eq!(dispatcher.call_count_for("a"), 2);
        assert_eq!(dispatcher.call_count_for("b"), 1);
        assert!(dispatcher.was_called("a"));
        assert!(dispatcher.was_called("b"));
        assert!(!dispatcher.was_called("c"));
    }

    #[test]
    fn mock_dispatcher_call_log_preserves_order() {
        let mut dispatcher = MockToolDispatcher::new();
        dispatcher.when("x", json!(null));
        let req = ToolRequest {
            tool_id: "x".to_string(),
            version: "1".to_string(),
            args: json!({"seq": 1}),
            policy: json!({}),
        };
        dispatcher.dispatch(&req).unwrap();
        let log = dispatcher.call_log();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].tool_id, "x");
    }

    #[test]
    fn mock_dispatcher_dynamic_handler_can_return_error() {
        let mut dispatcher = MockToolDispatcher::new();
        dispatcher.when_fn(
            "fail",
            Box::new(|_| Err(ToolError::ExecutionFailed("boom".into()))),
        );
        let request = ToolRequest {
            tool_id: "fail".to_string(),
            version: "1".to_string(),
            args: json!({}),
            policy: json!({}),
        };
        let err = dispatcher.dispatch(&request).unwrap_err();
        match err {
            ToolError::ExecutionFailed(msg) => assert_eq!(msg, "boom"),
            other => panic!("expected ExecutionFailed, got: {}", other),
        }
    }

    #[test]
    fn mock_dispatcher_reset() {
        let mut dispatcher = MockToolDispatcher::new();
        dispatcher.when("tool", json!("response"));
        let req = ToolRequest {
            tool_id: "tool".to_string(),
            version: "1".to_string(),
            args: json!({}),
            policy: json!({}),
        };
        dispatcher.dispatch(&req).unwrap();
        assert_eq!(dispatcher.call_count(), 1);

        dispatcher.reset();
        assert_eq!(dispatcher.call_count(), 0);
        assert!(dispatcher.call_log().is_empty());
        // Static mock should be cleared.
        let err = dispatcher.dispatch(&req).unwrap_err();
        assert!(matches!(err, ToolError::NotFound(_)));
    }

    #[test]
    fn mock_dispatcher_default() {
        let dispatcher = MockToolDispatcher::default();
        assert_eq!(dispatcher.call_count(), 0);
    }

    #[test]
    fn mock_effect_handler_default() {
        let handler = MockEffectHandler::default();
        assert_eq!(handler.call_count("any", "op"), 0);
    }
}
