//! JSON fast-path builtins for the Lumen runtime.
//!
//! Provides `json_get`, `json_merge`, `json_flatten`, and `json_diff`
//! operating on [`serde_json::Value`]. These are the runtime primitives
//! backing the VM's built-in JSON operations.
//!
//! # Example
//!
//! ```rust
//! use lumen_runtime::json_ops::{json_get, json_merge, json_flatten, json_diff};
//! use serde_json::json;
//!
//! let data = json!({"user": {"name": "Alice", "age": 30}});
//! assert_eq!(json_get(&data, "user.name"), json!("Alice"));
//! ```

use serde_json::{Map, Value};

// ---------------------------------------------------------------------------
// json_get — dot-path access
// ---------------------------------------------------------------------------

/// Access a nested value using a dot-separated path.
///
/// Returns `Value::Null` if any segment along the path is missing or if the
/// path is empty. Array indices can be used as numeric path segments
/// (e.g. `"items.0.name"`).
///
/// # Examples
///
/// ```rust
/// use lumen_runtime::json_ops::json_get;
/// use serde_json::{json, Value};
///
/// let v = json!({"a": {"b": [10, 20, 30]}});
/// assert_eq!(json_get(&v, "a.b.1"), json!(20));
/// assert_eq!(json_get(&v, "a.missing"), Value::Null);
/// ```
pub fn json_get(value: &Value, path: &str) -> Value {
    if path.is_empty() {
        return value.clone();
    }

    let mut current = value;
    for segment in path.split('.') {
        match current {
            Value::Object(map) => {
                if let Some(next) = map.get(segment) {
                    current = next;
                } else {
                    return Value::Null;
                }
            }
            Value::Array(arr) => {
                if let Ok(idx) = segment.parse::<usize>() {
                    if let Some(next) = arr.get(idx) {
                        current = next;
                    } else {
                        return Value::Null;
                    }
                } else {
                    return Value::Null;
                }
            }
            _ => return Value::Null,
        }
    }
    current.clone()
}

// ---------------------------------------------------------------------------
// json_merge — deep merge
// ---------------------------------------------------------------------------

/// Deep-merge two JSON values.
///
/// - If both `a` and `b` are objects, keys from `b` override keys in `a`,
///   with nested objects merged recursively.
/// - In all other cases, `b` takes precedence (overrides `a`).
///
/// # Examples
///
/// ```rust
/// use lumen_runtime::json_ops::json_merge;
/// use serde_json::json;
///
/// let a = json!({"x": 1, "nested": {"a": 1}});
/// let b = json!({"y": 2, "nested": {"b": 2}});
/// let merged = json_merge(&a, &b);
/// assert_eq!(merged, json!({"x": 1, "y": 2, "nested": {"a": 1, "b": 2}}));
/// ```
pub fn json_merge(a: &Value, b: &Value) -> Value {
    match (a, b) {
        (Value::Object(map_a), Value::Object(map_b)) => {
            let mut result = map_a.clone();
            for (key, val_b) in map_b {
                let merged_val = if let Some(val_a) = result.get(key) {
                    json_merge(val_a, val_b)
                } else {
                    val_b.clone()
                };
                result.insert(key.clone(), merged_val);
            }
            Value::Object(result)
        }
        // Non-object: b wins
        _ => b.clone(),
    }
}

// ---------------------------------------------------------------------------
// json_flatten — nested → flat keys
// ---------------------------------------------------------------------------

/// Flatten a nested JSON value into a flat object with dot-separated keys.
///
/// `prefix` is prepended to all keys (pass `""` for the root level).
/// Non-object, non-array leaves become values in the output. Arrays are
/// flattened with numeric indices (e.g. `"items.0"`, `"items.1"`).
///
/// # Examples
///
/// ```rust
/// use lumen_runtime::json_ops::json_flatten;
/// use serde_json::json;
///
/// let v = json!({"a": {"b": 1, "c": [2, 3]}});
/// let flat = json_flatten(&v, "");
/// assert_eq!(flat, json!({"a.b": 1, "a.c.0": 2, "a.c.1": 3}));
/// ```
pub fn json_flatten(value: &Value, prefix: &str) -> Value {
    let mut out = Map::new();
    flatten_recursive(value, prefix, &mut out);
    Value::Object(out)
}

fn flatten_recursive(value: &Value, prefix: &str, out: &mut Map<String, Value>) {
    match value {
        Value::Object(map) => {
            for (key, val) in map {
                let new_prefix = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_recursive(val, &new_prefix, out);
            }
        }
        Value::Array(arr) => {
            for (i, val) in arr.iter().enumerate() {
                let new_prefix = if prefix.is_empty() {
                    i.to_string()
                } else {
                    format!("{prefix}.{i}")
                };
                flatten_recursive(val, &new_prefix, out);
            }
        }
        _ => {
            let key = if prefix.is_empty() {
                // Scalar at root level — use empty key (edge case)
                String::new()
            } else {
                prefix.to_string()
            };
            out.insert(key, value.clone());
        }
    }
}

// ---------------------------------------------------------------------------
// json_diff — structural diff
// ---------------------------------------------------------------------------

/// Produce a diff between two JSON values.
///
/// Returns an object describing the differences:
/// - `"added"`: keys/values present in `b` but not `a`
/// - `"removed"`: keys/values present in `a` but not `b`
/// - `"changed"`: keys where the value differs (with `"from"` and `"to"`)
///
/// If the values are equal, all three fields are empty objects/arrays.
/// For non-object values, the diff compares them directly.
///
/// # Examples
///
/// ```rust
/// use lumen_runtime::json_ops::json_diff;
/// use serde_json::json;
///
/// let a = json!({"x": 1, "y": 2});
/// let b = json!({"x": 1, "z": 3});
/// let diff = json_diff(&a, &b);
/// assert_eq!(diff["added"], json!({"z": 3}));
/// assert_eq!(diff["removed"], json!({"y": 2}));
/// assert_eq!(diff["changed"], json!({}));
/// ```
pub fn json_diff(a: &Value, b: &Value) -> Value {
    match (a, b) {
        (Value::Object(map_a), Value::Object(map_b)) => {
            let mut added = Map::new();
            let mut removed = Map::new();
            let mut changed = Map::new();

            // Keys in b but not a → added
            for (key, val_b) in map_b {
                if !map_a.contains_key(key) {
                    added.insert(key.clone(), val_b.clone());
                }
            }

            // Keys in a but not b → removed
            for (key, val_a) in map_a {
                if !map_b.contains_key(key) {
                    removed.insert(key.clone(), val_a.clone());
                }
            }

            // Keys in both — check for changes
            for (key, val_a) in map_a {
                if let Some(val_b) = map_b.get(key) {
                    if val_a != val_b {
                        changed.insert(
                            key.clone(),
                            serde_json::json!({
                                "from": val_a,
                                "to": val_b,
                            }),
                        );
                    }
                }
            }

            serde_json::json!({
                "added": Value::Object(added),
                "removed": Value::Object(removed),
                "changed": Value::Object(changed),
            })
        }
        _ => {
            // Non-object comparison
            if a == b {
                serde_json::json!({
                    "added": {},
                    "removed": {},
                    "changed": {},
                })
            } else {
                serde_json::json!({
                    "added": {},
                    "removed": {},
                    "changed": {
                        "value": {
                            "from": a,
                            "to": b,
                        }
                    },
                })
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
    use serde_json::json;

    // -- json_get ---------------------------------------------------------

    #[test]
    fn get_simple_key() {
        let v = json!({"name": "Alice"});
        assert_eq!(json_get(&v, "name"), json!("Alice"));
    }

    #[test]
    fn get_nested_key() {
        let v = json!({"user": {"profile": {"age": 30}}});
        assert_eq!(json_get(&v, "user.profile.age"), json!(30));
    }

    #[test]
    fn get_missing_key_returns_null() {
        let v = json!({"a": 1});
        assert_eq!(json_get(&v, "b"), Value::Null);
        assert_eq!(json_get(&v, "a.b"), Value::Null);
    }

    #[test]
    fn get_array_index() {
        let v = json!({"items": [10, 20, 30]});
        assert_eq!(json_get(&v, "items.1"), json!(20));
    }

    #[test]
    fn get_array_index_out_of_bounds() {
        let v = json!({"items": [1, 2]});
        assert_eq!(json_get(&v, "items.5"), Value::Null);
    }

    #[test]
    fn get_empty_path_returns_root() {
        let v = json!({"x": 1});
        assert_eq!(json_get(&v, ""), v);
    }

    #[test]
    fn get_scalar_root() {
        let v = json!(42);
        assert_eq!(json_get(&v, "anything"), Value::Null);
    }

    // -- json_merge -------------------------------------------------------

    #[test]
    fn merge_disjoint_objects() {
        let a = json!({"x": 1});
        let b = json!({"y": 2});
        assert_eq!(json_merge(&a, &b), json!({"x": 1, "y": 2}));
    }

    #[test]
    fn merge_overlapping_keys() {
        let a = json!({"x": 1, "y": 2});
        let b = json!({"y": 3, "z": 4});
        let merged = json_merge(&a, &b);
        assert_eq!(merged, json!({"x": 1, "y": 3, "z": 4}));
    }

    #[test]
    fn merge_deep_nesting() {
        let a = json!({"config": {"db": {"host": "localhost"}, "port": 8080}});
        let b = json!({"config": {"db": {"port": 5432}, "debug": true}});
        let merged = json_merge(&a, &b);
        assert_eq!(
            merged,
            json!({"config": {"db": {"host": "localhost", "port": 5432}, "port": 8080, "debug": true}})
        );
    }

    #[test]
    fn merge_non_object_b_wins() {
        let a = json!({"x": 1});
        let b = json!("override");
        assert_eq!(json_merge(&a, &b), json!("override"));
    }

    #[test]
    fn merge_both_scalars() {
        assert_eq!(json_merge(&json!(1), &json!(2)), json!(2));
    }

    // -- json_flatten -----------------------------------------------------

    #[test]
    fn flatten_simple_object() {
        let v = json!({"a": 1, "b": 2});
        let flat = json_flatten(&v, "");
        assert_eq!(flat, json!({"a": 1, "b": 2}));
    }

    #[test]
    fn flatten_nested_object() {
        let v = json!({"a": {"b": {"c": 42}}});
        let flat = json_flatten(&v, "");
        assert_eq!(flat, json!({"a.b.c": 42}));
    }

    #[test]
    fn flatten_with_arrays() {
        let v = json!({"items": [1, 2, 3]});
        let flat = json_flatten(&v, "");
        assert_eq!(flat, json!({"items.0": 1, "items.1": 2, "items.2": 3}));
    }

    #[test]
    fn flatten_with_prefix() {
        let v = json!({"x": 1});
        let flat = json_flatten(&v, "root");
        assert_eq!(flat, json!({"root.x": 1}));
    }

    #[test]
    fn flatten_mixed_nesting() {
        let v = json!({"a": {"b": 1, "c": [2, 3]}});
        let flat = json_flatten(&v, "");
        assert_eq!(flat, json!({"a.b": 1, "a.c.0": 2, "a.c.1": 3}));
    }

    // -- json_diff --------------------------------------------------------

    #[test]
    fn diff_identical_objects() {
        let v = json!({"x": 1, "y": 2});
        let diff = json_diff(&v, &v);
        assert_eq!(diff["added"], json!({}));
        assert_eq!(diff["removed"], json!({}));
        assert_eq!(diff["changed"], json!({}));
    }

    #[test]
    fn diff_added_keys() {
        let a = json!({"x": 1});
        let b = json!({"x": 1, "y": 2});
        let diff = json_diff(&a, &b);
        assert_eq!(diff["added"], json!({"y": 2}));
        assert_eq!(diff["removed"], json!({}));
        assert_eq!(diff["changed"], json!({}));
    }

    #[test]
    fn diff_removed_keys() {
        let a = json!({"x": 1, "y": 2});
        let b = json!({"x": 1});
        let diff = json_diff(&a, &b);
        assert_eq!(diff["added"], json!({}));
        assert_eq!(diff["removed"], json!({"y": 2}));
        assert_eq!(diff["changed"], json!({}));
    }

    #[test]
    fn diff_changed_values() {
        let a = json!({"x": 1, "y": "old"});
        let b = json!({"x": 1, "y": "new"});
        let diff = json_diff(&a, &b);
        assert_eq!(diff["added"], json!({}));
        assert_eq!(diff["removed"], json!({}));
        assert_eq!(diff["changed"]["y"]["from"], json!("old"));
        assert_eq!(diff["changed"]["y"]["to"], json!("new"));
    }

    #[test]
    fn diff_mixed_changes() {
        let a = json!({"keep": 1, "remove": 2, "change": "a"});
        let b = json!({"keep": 1, "add": 3, "change": "b"});
        let diff = json_diff(&a, &b);
        assert_eq!(diff["added"], json!({"add": 3}));
        assert_eq!(diff["removed"], json!({"remove": 2}));
        assert_eq!(diff["changed"]["change"]["from"], json!("a"));
        assert_eq!(diff["changed"]["change"]["to"], json!("b"));
    }

    #[test]
    fn diff_scalars_equal() {
        let diff = json_diff(&json!(42), &json!(42));
        assert_eq!(diff["added"], json!({}));
        assert_eq!(diff["removed"], json!({}));
        assert_eq!(diff["changed"], json!({}));
    }

    #[test]
    fn diff_scalars_different() {
        let diff = json_diff(&json!("hello"), &json!("world"));
        assert_eq!(diff["changed"]["value"]["from"], json!("hello"));
        assert_eq!(diff["changed"]["value"]["to"], json!("world"));
    }

    #[test]
    fn diff_empty_objects() {
        let diff = json_diff(&json!({}), &json!({}));
        assert_eq!(diff["added"], json!({}));
        assert_eq!(diff["removed"], json!({}));
        assert_eq!(diff["changed"], json!({}));
    }
}
