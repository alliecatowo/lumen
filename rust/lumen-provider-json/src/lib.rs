//! JSON manipulation provider for Lumen.
//!
//! Provides tools for parsing, stringifying, and manipulating JSON data
//! with dot-path navigation, JSONPath-style lookup, deep merge, flatten,
//! and diff capabilities.

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
                            "enum": ["parse", "stringify", "get", "set", "merge", "flatten", "diff"]
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
        serde_json::from_str(input)
            .map_err(|e| ToolError::InvocationFailed(format!("JSON parse error: {}", e)))
    }

    /// Stringify a Value to JSON.
    fn stringify(&self, value: &Value) -> Result<String, ToolError> {
        serde_json::to_string(value)
            .map_err(|e| ToolError::InvocationFailed(format!("JSON stringify error: {}", e)))
    }

    /// Get a value from a JSON object using JSONPath-style notation.
    ///
    /// Supports:
    /// - Simple dot paths: `"a.b.c"`
    /// - Array indexing: `"users[0].name"`, `"items[2]"`
    /// - Root-prefixed: `"$.users[0].name"` (leading `$` is stripped)
    /// - Negative indices: `"items[-1]"` (last element)
    fn get(&self, json: &Value, path: &str) -> Result<Value, ToolError> {
        let segments = parse_path_segments(path)?;
        let mut current = json;

        for segment in &segments {
            match segment {
                PathSegment::Key(key) => {
                    current = current.get(key.as_str()).ok_or_else(|| {
                        ToolError::InvocationFailed(format!("Path not found: {}", path))
                    })?;
                }
                PathSegment::Index(idx) => {
                    let arr = current.as_array().ok_or_else(|| {
                        ToolError::InvocationFailed(format!(
                            "Expected array at index [{}] in path: {}",
                            idx, path
                        ))
                    })?;

                    let resolved_idx = if *idx < 0 {
                        let len = arr.len() as i64;
                        let pos = len + idx;
                        if pos < 0 {
                            return Err(ToolError::InvocationFailed(format!(
                                "Negative index {} out of bounds for array of length {} in path: {}",
                                idx,
                                arr.len(),
                                path
                            )));
                        }
                        pos as usize
                    } else {
                        *idx as usize
                    };

                    current = arr.get(resolved_idx).ok_or_else(|| {
                        ToolError::InvocationFailed(format!(
                            "Index {} out of bounds for array of length {} in path: {}",
                            idx,
                            arr.len(),
                            path
                        ))
                    })?;
                }
            }
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
    fn set_recursive(
        &self,
        current: &mut Value,
        path: &[&str],
        value: Value,
    ) -> Result<(), ToolError> {
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
                    "Cannot set property on non-object".to_string(),
                ))
            }
        } else {
            // Recursive case: navigate deeper
            if !current.is_object() {
                *current = Value::Object(serde_json::Map::new());
            }

            if let Value::Object(ref mut map) = current {
                let key = path[0].to_string();
                let next = map
                    .entry(key.clone())
                    .or_insert(Value::Object(serde_json::Map::new()));
                self.set_recursive(next, &path[1..], value)
            } else {
                Err(ToolError::InvocationFailed(
                    "Cannot navigate non-object".to_string(),
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

    /// Flatten a nested JSON object into dot-notation keys.
    ///
    /// Example:
    /// ```json
    /// {"a": {"b": 1, "c": [2, 3]}} → {"a.b": 1, "a.c[0]": 2, "a.c[1]": 3}
    /// ```
    fn flatten(&self, value: &Value) -> Result<Value, ToolError> {
        let mut result = serde_json::Map::new();
        flatten_recursive(value, String::new(), &mut result);
        Ok(Value::Object(result))
    }

    /// Compute the diff between two JSON values.
    ///
    /// Returns an object with:
    /// - `additions`: keys/values present in `b` but not `a`
    /// - `deletions`: keys/values present in `a` but not `b`
    /// - `changes`: keys where the value differs between `a` and `b`
    fn diff(&self, a: &Value, b: &Value) -> Result<Value, ToolError> {
        let mut additions = serde_json::Map::new();
        let mut deletions = serde_json::Map::new();
        let mut changes = serde_json::Map::new();

        diff_recursive(
            a,
            b,
            String::new(),
            &mut additions,
            &mut deletions,
            &mut changes,
        );

        Ok(json!({
            "additions": Value::Object(additions),
            "deletions": Value::Object(deletions),
            "changes": Value::Object(changes),
        }))
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
                let input_str = input.get("input").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvocationFailed("Missing 'input' string".to_string())
                })?;
                self.parse(input_str)
            }
            "stringify" => {
                let value = input.get("value").ok_or_else(|| {
                    ToolError::InvocationFailed("Missing 'value' field".to_string())
                })?;
                let result = self.stringify(value)?;
                Ok(json!(result))
            }
            "get" => {
                let json_val = input.get("json").ok_or_else(|| {
                    ToolError::InvocationFailed("Missing 'json' field".to_string())
                })?;
                let path = input.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvocationFailed("Missing 'path' string".to_string())
                })?;
                self.get(json_val, path)
            }
            "set" => {
                let json_val = input.get("json").ok_or_else(|| {
                    ToolError::InvocationFailed("Missing 'json' field".to_string())
                })?;
                let path = input.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvocationFailed("Missing 'path' string".to_string())
                })?;
                let value = input
                    .get("value")
                    .ok_or_else(|| {
                        ToolError::InvocationFailed("Missing 'value' field".to_string())
                    })?
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
            "flatten" => {
                let value = input.get("value").ok_or_else(|| {
                    ToolError::InvocationFailed("Missing 'value' field".to_string())
                })?;
                self.flatten(value)
            }
            "diff" => {
                let a = input
                    .get("a")
                    .ok_or_else(|| ToolError::InvocationFailed("Missing 'a' field".to_string()))?;
                let b = input
                    .get("b")
                    .ok_or_else(|| ToolError::InvocationFailed("Missing 'b' field".to_string()))?;
                self.diff(a, b)
            }
            _ => Err(ToolError::InvocationFailed(format!(
                "Unknown operation: {}",
                operation
            ))),
        }
    }
}

// =============================================================================
// Path Parsing (JSONPath-style)
// =============================================================================

/// A segment of a parsed JSON path.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PathSegment {
    /// Object key access (e.g., "name").
    Key(String),
    /// Array index access (e.g., [0], [-1]).
    Index(i64),
}

/// Parse a JSONPath-style path string into segments.
///
/// Supports:
/// - `"a.b.c"` → [Key("a"), Key("b"), Key("c")]
/// - `"users[0].name"` → [Key("users"), Index(0), Key("name")]
/// - `"$.users[0].name"` → [Key("users"), Index(0), Key("name")]
/// - `"items[-1]"` → [Key("items"), Index(-1)]
fn parse_path_segments(path: &str) -> Result<Vec<PathSegment>, ToolError> {
    // Strip leading "$." or "$"
    let path = if let Some(rest) = path.strip_prefix("$.") {
        rest
    } else if let Some(rest) = path.strip_prefix('$') {
        rest
    } else {
        path
    };

    if path.is_empty() {
        return Err(ToolError::InvocationFailed("Empty path".to_string()));
    }

    let mut segments = Vec::new();
    let mut chars = path.chars().peekable();
    let mut current_key = String::new();

    while let Some(&ch) = chars.peek() {
        match ch {
            '.' => {
                // Dot separator — push current key if non-empty
                if !current_key.is_empty() {
                    segments.push(PathSegment::Key(current_key.clone()));
                    current_key.clear();
                }
                chars.next();
            }
            '[' => {
                // Array index start — push current key first if non-empty
                if !current_key.is_empty() {
                    segments.push(PathSegment::Key(current_key.clone()));
                    current_key.clear();
                }
                chars.next(); // consume '['

                // Read the index value
                let mut index_str = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ']' {
                        chars.next(); // consume ']'
                        break;
                    }
                    index_str.push(c);
                    chars.next();
                }

                let index: i64 = index_str.parse().map_err(|_| {
                    ToolError::InvocationFailed(format!(
                        "Invalid array index '{}' in path",
                        index_str
                    ))
                })?;

                segments.push(PathSegment::Index(index));
            }
            _ => {
                current_key.push(ch);
                chars.next();
            }
        }
    }

    // Push remaining key
    if !current_key.is_empty() {
        segments.push(PathSegment::Key(current_key));
    }

    if segments.is_empty() {
        return Err(ToolError::InvocationFailed(
            "Empty path after parsing".to_string(),
        ));
    }

    Ok(segments)
}

// =============================================================================
// Flatten Helpers
// =============================================================================

/// Recursively flatten a JSON value into dot-notation keys.
fn flatten_recursive(value: &Value, prefix: String, result: &mut serde_json::Map<String, Value>) {
    match value {
        Value::Object(map) => {
            for (key, val) in map {
                let new_prefix = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                flatten_recursive(val, new_prefix, result);
            }
        }
        Value::Array(arr) => {
            for (i, val) in arr.iter().enumerate() {
                let new_prefix = format!("{}[{}]", prefix, i);
                flatten_recursive(val, new_prefix, result);
            }
        }
        _ => {
            // Leaf value — store with the accumulated path
            result.insert(prefix, value.clone());
        }
    }
}

// =============================================================================
// Diff Helpers
// =============================================================================

/// Recursively compute the diff between two JSON values.
fn diff_recursive(
    a: &Value,
    b: &Value,
    prefix: String,
    additions: &mut serde_json::Map<String, Value>,
    deletions: &mut serde_json::Map<String, Value>,
    changes: &mut serde_json::Map<String, Value>,
) {
    match (a, b) {
        (Value::Object(a_map), Value::Object(b_map)) => {
            // Check for deletions and changes
            for (key, a_val) in a_map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };

                match b_map.get(key) {
                    Some(b_val) => {
                        // Key exists in both — recurse
                        diff_recursive(a_val, b_val, path, additions, deletions, changes);
                    }
                    None => {
                        // Key only in a → deletion
                        deletions.insert(path, a_val.clone());
                    }
                }
            }

            // Check for additions
            for (key, b_val) in b_map {
                if !a_map.contains_key(key) {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", prefix, key)
                    };
                    additions.insert(path, b_val.clone());
                }
            }
        }
        _ => {
            // Non-object comparison: check if values differ
            if a != b {
                let path = if prefix.is_empty() {
                    "<root>".to_string()
                } else {
                    prefix
                };
                changes.insert(
                    path,
                    json!({
                        "old": a,
                        "new": b,
                    }),
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Existing tests (kept intact)
    // =========================================================================

    #[test]
    fn test_parse_valid_json() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "parse",
                "input": r#"{"name": "Alice", "age": 30}"#
            }))
            .unwrap();

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
        let result = provider
            .call(json!({
                "operation": "stringify",
                "value": {"name": "Bob", "count": 42}
            }))
            .unwrap();

        let json_str = result.as_str().unwrap();
        assert!(json_str.contains("Bob"));
        assert!(json_str.contains("42"));
    }

    #[test]
    fn test_get_simple_path() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "get",
                "json": {"user": {"name": "Charlie"}},
                "path": "user.name"
            }))
            .unwrap();

        assert_eq!(result.as_str().unwrap(), "Charlie");
    }

    #[test]
    fn test_get_nested_path() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "get",
                "json": {"a": {"b": {"c": "deep"}}},
                "path": "a.b.c"
            }))
            .unwrap();

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
        let result = provider
            .call(json!({
                "operation": "set",
                "json": {"existing": "value"},
                "path": "new",
                "value": "data"
            }))
            .unwrap();

        assert_eq!(result.get("existing").unwrap().as_str().unwrap(), "value");
        assert_eq!(result.get("new").unwrap().as_str().unwrap(), "data");
    }

    #[test]
    fn test_set_nested_path_creates_intermediates() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "set",
                "json": {},
                "path": "a.b.c",
                "value": 123
            }))
            .unwrap();

        assert_eq!(result["a"]["b"]["c"].as_i64().unwrap(), 123);
    }

    #[test]
    fn test_merge_objects() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "merge",
                "a": {"x": 1, "y": {"z": 2}},
                "b": {"y": {"w": 3}, "q": 4}
            }))
            .unwrap();

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
        let result = provider
            .call(json!({
                "operation": "merge",
                "a": {"key": "old"},
                "b": {"key": "new"}
            }))
            .unwrap();

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

    // =========================================================================
    // JSONPath-style get tests (T132)
    // =========================================================================

    #[test]
    fn test_get_with_array_index() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "get",
                "json": {"users": [{"name": "Alice"}, {"name": "Bob"}]},
                "path": "users[0].name"
            }))
            .unwrap();

        assert_eq!(result.as_str().unwrap(), "Alice");
    }

    #[test]
    fn test_get_with_array_index_second_element() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "get",
                "json": {"users": [{"name": "Alice"}, {"name": "Bob"}]},
                "path": "users[1].name"
            }))
            .unwrap();

        assert_eq!(result.as_str().unwrap(), "Bob");
    }

    #[test]
    fn test_get_with_dollar_prefix() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "get",
                "json": {"users": [{"name": "Alice"}]},
                "path": "$.users[0].name"
            }))
            .unwrap();

        assert_eq!(result.as_str().unwrap(), "Alice");
    }

    #[test]
    fn test_get_with_negative_index() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "get",
                "json": {"items": [10, 20, 30]},
                "path": "items[-1]"
            }))
            .unwrap();

        assert_eq!(result.as_i64().unwrap(), 30);
    }

    #[test]
    fn test_get_with_negative_index_second_from_end() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "get",
                "json": {"items": [10, 20, 30]},
                "path": "items[-2]"
            }))
            .unwrap();

        assert_eq!(result.as_i64().unwrap(), 20);
    }

    #[test]
    fn test_get_array_index_out_of_bounds() {
        let provider = JsonProvider::new();
        let result = provider.call(json!({
            "operation": "get",
            "json": {"items": [1, 2]},
            "path": "items[5]"
        }));

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("out of bounds"));
    }

    #[test]
    fn test_get_negative_index_out_of_bounds() {
        let provider = JsonProvider::new();
        let result = provider.call(json!({
            "operation": "get",
            "json": {"items": [1, 2]},
            "path": "items[-5]"
        }));

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("out of bounds"));
    }

    #[test]
    fn test_get_nested_arrays() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "get",
                "json": {"matrix": [[1, 2], [3, 4]]},
                "path": "matrix[1][0]"
            }))
            .unwrap();

        assert_eq!(result.as_i64().unwrap(), 3);
    }

    #[test]
    fn test_get_complex_jsonpath() {
        let provider = JsonProvider::new();
        let data = json!({
            "store": {
                "books": [
                    {"title": "Dune", "price": 9.99},
                    {"title": "Foundation", "price": 12.99}
                ]
            }
        });

        let result = provider
            .call(json!({
                "operation": "get",
                "json": data,
                "path": "$.store.books[1].title"
            }))
            .unwrap();

        assert_eq!(result.as_str().unwrap(), "Foundation");
    }

    #[test]
    fn test_get_on_non_array_with_index() {
        let provider = JsonProvider::new();
        let result = provider.call(json!({
            "operation": "get",
            "json": {"x": "not an array"},
            "path": "x[0]"
        }));

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Expected array"));
    }

    // =========================================================================
    // Flatten tests (T132)
    // =========================================================================

    #[test]
    fn test_flatten_simple_object() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "flatten",
                "value": {"a": 1, "b": "hello"}
            }))
            .unwrap();

        assert_eq!(result.get("a").unwrap().as_i64().unwrap(), 1);
        assert_eq!(result.get("b").unwrap().as_str().unwrap(), "hello");
    }

    #[test]
    fn test_flatten_nested_object() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "flatten",
                "value": {"a": {"b": 1, "c": {"d": 2}}}
            }))
            .unwrap();

        assert_eq!(result.get("a.b").unwrap().as_i64().unwrap(), 1);
        assert_eq!(result.get("a.c.d").unwrap().as_i64().unwrap(), 2);
    }

    #[test]
    fn test_flatten_with_arrays() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "flatten",
                "value": {"items": [10, 20, 30]}
            }))
            .unwrap();

        assert_eq!(result.get("items[0]").unwrap().as_i64().unwrap(), 10);
        assert_eq!(result.get("items[1]").unwrap().as_i64().unwrap(), 20);
        assert_eq!(result.get("items[2]").unwrap().as_i64().unwrap(), 30);
    }

    #[test]
    fn test_flatten_mixed() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "flatten",
                "value": {
                    "user": {
                        "name": "Alice",
                        "scores": [100, 95]
                    }
                }
            }))
            .unwrap();

        assert_eq!(result.get("user.name").unwrap().as_str().unwrap(), "Alice");
        assert_eq!(result.get("user.scores[0]").unwrap().as_i64().unwrap(), 100);
        assert_eq!(result.get("user.scores[1]").unwrap().as_i64().unwrap(), 95);
    }

    #[test]
    fn test_flatten_empty_object() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "flatten",
                "value": {}
            }))
            .unwrap();

        assert!(result.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_flatten_scalar() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "flatten",
                "value": 42
            }))
            .unwrap();

        // Scalar at root level gets empty key
        assert_eq!(result.get("").unwrap().as_i64().unwrap(), 42);
    }

    // =========================================================================
    // Diff tests (T132)
    // =========================================================================

    #[test]
    fn test_diff_identical() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "diff",
                "a": {"x": 1, "y": 2},
                "b": {"x": 1, "y": 2}
            }))
            .unwrap();

        assert!(result
            .get("additions")
            .unwrap()
            .as_object()
            .unwrap()
            .is_empty());
        assert!(result
            .get("deletions")
            .unwrap()
            .as_object()
            .unwrap()
            .is_empty());
        assert!(result
            .get("changes")
            .unwrap()
            .as_object()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_diff_additions() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "diff",
                "a": {"x": 1},
                "b": {"x": 1, "y": 2}
            }))
            .unwrap();

        let additions = result.get("additions").unwrap().as_object().unwrap();
        assert_eq!(additions.get("y").unwrap().as_i64().unwrap(), 2);

        assert!(result
            .get("deletions")
            .unwrap()
            .as_object()
            .unwrap()
            .is_empty());
        assert!(result
            .get("changes")
            .unwrap()
            .as_object()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_diff_deletions() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "diff",
                "a": {"x": 1, "y": 2},
                "b": {"x": 1}
            }))
            .unwrap();

        let deletions = result.get("deletions").unwrap().as_object().unwrap();
        assert_eq!(deletions.get("y").unwrap().as_i64().unwrap(), 2);

        assert!(result
            .get("additions")
            .unwrap()
            .as_object()
            .unwrap()
            .is_empty());
        assert!(result
            .get("changes")
            .unwrap()
            .as_object()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_diff_changes() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "diff",
                "a": {"x": 1, "y": "old"},
                "b": {"x": 1, "y": "new"}
            }))
            .unwrap();

        let changes = result.get("changes").unwrap().as_object().unwrap();
        let y_change = changes.get("y").unwrap();
        assert_eq!(y_change.get("old").unwrap().as_str().unwrap(), "old");
        assert_eq!(y_change.get("new").unwrap().as_str().unwrap(), "new");

        assert!(result
            .get("additions")
            .unwrap()
            .as_object()
            .unwrap()
            .is_empty());
        assert!(result
            .get("deletions")
            .unwrap()
            .as_object()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_diff_nested() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "diff",
                "a": {"config": {"host": "old.com", "port": 80}},
                "b": {"config": {"host": "new.com", "port": 80, "ssl": true}}
            }))
            .unwrap();

        let additions = result.get("additions").unwrap().as_object().unwrap();
        assert_eq!(
            additions.get("config.ssl").unwrap().as_bool().unwrap(),
            true
        );

        let changes = result.get("changes").unwrap().as_object().unwrap();
        let host_change = changes.get("config.host").unwrap();
        assert_eq!(host_change.get("old").unwrap().as_str().unwrap(), "old.com");
        assert_eq!(host_change.get("new").unwrap().as_str().unwrap(), "new.com");
    }

    #[test]
    fn test_diff_mixed() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "diff",
                "a": {"a": 1, "b": 2, "c": 3},
                "b": {"b": 20, "c": 3, "d": 4}
            }))
            .unwrap();

        let additions = result.get("additions").unwrap().as_object().unwrap();
        let deletions = result.get("deletions").unwrap().as_object().unwrap();
        let changes = result.get("changes").unwrap().as_object().unwrap();

        assert_eq!(additions.len(), 1); // d added
        assert_eq!(deletions.len(), 1); // a deleted
        assert_eq!(changes.len(), 1); // b changed

        assert_eq!(additions.get("d").unwrap().as_i64().unwrap(), 4);
        assert_eq!(deletions.get("a").unwrap().as_i64().unwrap(), 1);
    }

    #[test]
    fn test_diff_empty_objects() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "diff",
                "a": {},
                "b": {}
            }))
            .unwrap();

        assert!(result
            .get("additions")
            .unwrap()
            .as_object()
            .unwrap()
            .is_empty());
        assert!(result
            .get("deletions")
            .unwrap()
            .as_object()
            .unwrap()
            .is_empty());
        assert!(result
            .get("changes")
            .unwrap()
            .as_object()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_diff_scalar_root() {
        let provider = JsonProvider::new();
        let result = provider
            .call(json!({
                "operation": "diff",
                "a": 1,
                "b": 2
            }))
            .unwrap();

        let changes = result.get("changes").unwrap().as_object().unwrap();
        assert_eq!(changes.len(), 1);
        let root_change = changes.get("<root>").unwrap();
        assert_eq!(root_change.get("old").unwrap().as_i64().unwrap(), 1);
        assert_eq!(root_change.get("new").unwrap().as_i64().unwrap(), 2);
    }

    // =========================================================================
    // Path parsing tests
    // =========================================================================

    #[test]
    fn test_parse_path_simple() {
        let segments = parse_path_segments("a.b.c").unwrap();
        assert_eq!(
            segments,
            vec![
                PathSegment::Key("a".to_string()),
                PathSegment::Key("b".to_string()),
                PathSegment::Key("c".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_path_with_index() {
        let segments = parse_path_segments("users[0].name").unwrap();
        assert_eq!(
            segments,
            vec![
                PathSegment::Key("users".to_string()),
                PathSegment::Index(0),
                PathSegment::Key("name".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_path_dollar_prefix() {
        let segments = parse_path_segments("$.store.books[1]").unwrap();
        assert_eq!(
            segments,
            vec![
                PathSegment::Key("store".to_string()),
                PathSegment::Key("books".to_string()),
                PathSegment::Index(1),
            ]
        );
    }

    #[test]
    fn test_parse_path_negative_index() {
        let segments = parse_path_segments("items[-1]").unwrap();
        assert_eq!(
            segments,
            vec![
                PathSegment::Key("items".to_string()),
                PathSegment::Index(-1),
            ]
        );
    }

    #[test]
    fn test_parse_path_multiple_indices() {
        let segments = parse_path_segments("matrix[0][1]").unwrap();
        assert_eq!(
            segments,
            vec![
                PathSegment::Key("matrix".to_string()),
                PathSegment::Index(0),
                PathSegment::Index(1),
            ]
        );
    }

    #[test]
    fn test_parse_path_empty() {
        assert!(parse_path_segments("").is_err());
        assert!(parse_path_segments("$").is_err());
        assert!(parse_path_segments("$.").is_err());
    }

    #[test]
    fn test_unknown_operation() {
        let provider = JsonProvider::new();
        let result = provider.call(json!({
            "operation": "invalid"
        }));
        assert!(result.is_err());
    }
}
