//! JSON manipulation provider for Lumen.
//!
//! Provides tools for parsing, stringifying, and manipulating JSON data
//! with dot-path navigation and deep merge capabilities.

use lumen_runtime::tools::{ToolError, ToolProvider, ToolSchema};
use serde_json::{json, Value};

/// JSON manipulation provider implementing common JSON operations.
pub struct JsonProvider {
    schema: ToolSchema,
}

impl JsonProvider {
    pub fn new() -> Self {
        Self {
            schema: ToolSchema {
                name: "json".to_string(),
                description: "JSON manipulation tools".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "operation": {
                            "type": "string",
                            "enum": ["parse", "stringify", "get", "set", "merge"]
                        }
                    },
                    "required": ["operation"]
                }),
                output_schema: json!({"type": ["object", "string", "null"]}),
                effects: vec![],
            },
        }
    }

    /// Parse a JSON string into a Value.
    fn parse(&self, input: &str) -> Result<Value, ToolError> {
        serde_json::from_str(input).map_err(|e| {
            ToolError::InvocationFailed(format!("JSON parse error: {}", e))
        })
    }

    /// Stringify a Value to JSON.
    fn stringify(&self, value: &Value) -> Result<String, ToolError> {
        serde_json::to_string(value).map_err(|e| {
            ToolError::InvocationFailed(format!("JSON stringify error: {}", e))
        })
    }

    /// Get a value from a JSON object using dot-path notation (e.g., "a.b.c").
    fn get(&self, json: &Value, path: &str) -> Result<Value, ToolError> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = json;

        for part in parts {
            if part.is_empty() {
                continue;
            }
            current = current.get(part).ok_or_else(|| {
                ToolError::InvocationFailed(format!("Path not found: {}", path))
            })?;
        }

        Ok(current.clone())
    }

    /// Set a value in a JSON object using dot-path notation.
    /// Creates intermediate objects as needed.
    fn set(&self, json: &Value, path: &str, value: Value) -> Result<Value, ToolError> {
        let parts: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();

        if parts.is_empty() {
            return Err(ToolError::InvocationFailed("Empty path".to_string()));
        }

        let mut result = json.clone();
        self.set_recursive(&mut result, &parts, value)?;
        Ok(result)
    }

    /// Recursive helper for setting values along a path.
    fn set_recursive(&self, current: &mut Value, path: &[&str], value: Value) -> Result<(), ToolError> {
        if path.is_empty() {
            return Ok(());
        }

        if path.len() == 1 {
            // Base case: set the value
            if let Value::Object(ref mut map) = current {
                map.insert(path[0].to_string(), value);
                Ok(())
            } else {
                Err(ToolError::InvocationFailed(
                    "Cannot set property on non-object".to_string()
                ))
            }
        } else {
            // Recursive case: navigate deeper
            if !current.is_object() {
                *current = Value::Object(serde_json::Map::new());
            }

            if let Value::Object(ref mut map) = current {
                let key = path[0].to_string();
                let next = map.entry(key.clone()).or_insert(Value::Object(serde_json::Map::new()));
                self.set_recursive(next, &path[1..], value)
            } else {
                Err(ToolError::InvocationFailed(
                    "Cannot navigate non-object".to_string()
                ))
            }
        }
    }

    /// Deep merge two JSON values. For objects, recursively merges keys.
    /// For arrays and primitives, `b` overwrites `a`.
    fn merge(&self, a: &Value, b: &Value) -> Result<Value, ToolError> {
        match (a, b) {
            (Value::Object(a_map), Value::Object(b_map)) => {
                let mut result = a_map.clone();
                for (key, b_value) in b_map {
                    if let Some(a_value) = result.get(key) {
                        // Recursively merge if both are objects
                        result.insert(key.clone(), self.merge(a_value, b_value)?);
                    } else {
                        // Key only in b, insert it
                        result.insert(key.clone(), b_value.clone());
                    }
                }
                Ok(Value::Object(result))
            }
            _ => {
                // For non-objects, b overwrites a
                Ok(b.clone())
            }
        }
    }
}

impl Default for JsonProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolProvider for JsonProvider {
    fn name(&self) -> &str {
        "json"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn schema(&self) -> &ToolSchema {
        &self.schema
    }

    fn call(&self, input: Value) -> Result<Value, ToolError> {
        let operation = input
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvocationFailed("Missing 'operation' field".to_string()))?;

        match operation {
            "parse" => {
                let input_str = input
                    .get("input")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvocationFailed("Missing 'input' string".to_string()))?;
                self.parse(input_str)
            }
            "stringify" => {
                let value = input
                    .get("value")
                    .ok_or_else(|| ToolError::InvocationFailed("Missing 'value' field".to_string()))?;
                let result = self.stringify(value)?;
                Ok(json!(result))
            }
            "get" => {
                let json_val = input
                    .get("json")
                    .ok_or_else(|| ToolError::InvocationFailed("Missing 'json' field".to_string()))?;
                let path = input
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvocationFailed("Missing 'path' string".to_string()))?;
                self.get(json_val, path)
            }
            "set" => {
                let json_val = input
                    .get("json")
                    .ok_or_else(|| ToolError::InvocationFailed("Missing 'json' field".to_string()))?;
                let path = input
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvocationFailed("Missing 'path' string".to_string()))?;
                let value = input
                    .get("value")
                    .ok_or_else(|| ToolError::InvocationFailed("Missing 'value' field".to_string()))?
                    .clone();
                self.set(json_val, path, value)
            }
            "merge" => {
                let a = input
                    .get("a")
                    .ok_or_else(|| ToolError::InvocationFailed("Missing 'a' field".to_string()))?;
                let b = input
                    .get("b")
                    .ok_or_else(|| ToolError::InvocationFailed("Missing 'b' field".to_string()))?;
                self.merge(a, b)
            }
            _ => Err(ToolError::InvocationFailed(format!(
                "Unknown operation: {}",
                operation
            ))),
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
    fn test_parse_valid_json() {
        let provider = JsonProvider::new();
        let result = provider.call(json!({
            "operation": "parse",
            "input": r#"{"name": "Alice", "age": 30}"#
        })).unwrap();

        assert_eq!(result.get("name").unwrap().as_str().unwrap(), "Alice");
        assert_eq!(result.get("age").unwrap().as_i64().unwrap(), 30);
    }

    #[test]
    fn test_parse_invalid_json() {
        let provider = JsonProvider::new();
        let result = provider.call(json!({
            "operation": "parse",
            "input": "not valid json"
        }));

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("parse error"));
    }

    #[test]
    fn test_stringify() {
        let provider = JsonProvider::new();
        let result = provider.call(json!({
            "operation": "stringify",
            "value": {"name": "Bob", "count": 42}
        })).unwrap();

        let json_str = result.as_str().unwrap();
        assert!(json_str.contains("Bob"));
        assert!(json_str.contains("42"));
    }

    #[test]
    fn test_get_simple_path() {
        let provider = JsonProvider::new();
        let result = provider.call(json!({
            "operation": "get",
            "json": {"user": {"name": "Charlie"}},
            "path": "user.name"
        })).unwrap();

        assert_eq!(result.as_str().unwrap(), "Charlie");
    }

    #[test]
    fn test_get_nested_path() {
        let provider = JsonProvider::new();
        let result = provider.call(json!({
            "operation": "get",
            "json": {"a": {"b": {"c": "deep"}}},
            "path": "a.b.c"
        })).unwrap();

        assert_eq!(result.as_str().unwrap(), "deep");
    }

    #[test]
    fn test_get_missing_path() {
        let provider = JsonProvider::new();
        let result = provider.call(json!({
            "operation": "get",
            "json": {"x": 1},
            "path": "y.z"
        }));

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Path not found"));
    }

    #[test]
    fn test_set_simple_path() {
        let provider = JsonProvider::new();
        let result = provider.call(json!({
            "operation": "set",
            "json": {"existing": "value"},
            "path": "new",
            "value": "data"
        })).unwrap();

        assert_eq!(result.get("existing").unwrap().as_str().unwrap(), "value");
        assert_eq!(result.get("new").unwrap().as_str().unwrap(), "data");
    }

    #[test]
    fn test_set_nested_path_creates_intermediates() {
        let provider = JsonProvider::new();
        let result = provider.call(json!({
            "operation": "set",
            "json": {},
            "path": "a.b.c",
            "value": 123
        })).unwrap();

        assert_eq!(result["a"]["b"]["c"].as_i64().unwrap(), 123);
    }

    #[test]
    fn test_merge_objects() {
        let provider = JsonProvider::new();
        let result = provider.call(json!({
            "operation": "merge",
            "a": {"x": 1, "y": {"z": 2}},
            "b": {"y": {"w": 3}, "q": 4}
        })).unwrap();

        // x from a
        assert_eq!(result.get("x").unwrap().as_i64().unwrap(), 1);
        // y merged recursively (z from a, w from b)
        assert_eq!(result["y"]["z"].as_i64().unwrap(), 2);
        assert_eq!(result["y"]["w"].as_i64().unwrap(), 3);
        // q from b
        assert_eq!(result.get("q").unwrap().as_i64().unwrap(), 4);
    }

    #[test]
    fn test_merge_overwrites_non_objects() {
        let provider = JsonProvider::new();
        let result = provider.call(json!({
            "operation": "merge",
            "a": {"key": "old"},
            "b": {"key": "new"}
        })).unwrap();

        assert_eq!(result.get("key").unwrap().as_str().unwrap(), "new");
    }

    #[test]
    fn test_provider_metadata() {
        let provider = JsonProvider::new();
        assert_eq!(provider.name(), "json");
        assert_eq!(provider.version(), "0.1.0");
        assert_eq!(provider.schema().name, "json");
        assert!(provider.effects().is_empty());
    }
}
