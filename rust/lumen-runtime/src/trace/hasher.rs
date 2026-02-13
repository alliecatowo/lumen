//! Canonical hashing for trace events and cache keys.

use sha2::{Sha256, Digest};

/// Hash a string value using SHA-256.
pub fn sha256_hash(data: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(data.as_bytes()))
}

/// Compute a canonical hash of a JSON value for caching.
pub fn canonical_hash(value: &serde_json::Value) -> String {
    let canonical = canonical_json(value);
    sha256_hash(&canonical)
}

/// Serialize a JSON value in canonical form (sorted keys, no whitespace).
pub fn canonical_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut pairs: Vec<_> = map.iter().collect();
            pairs.sort_by_key(|(k, _)| k.as_str());
            let entries: Vec<String> = pairs.iter()
                .map(|(k, v)| format!("{}:{}", serde_json::to_string(k).unwrap(), canonical_json(v)))
                .collect();
            format!("{{{}}}", entries.join(","))
        }
        serde_json::Value::Array(arr) => {
            let entries: Vec<String> = arr.iter().map(canonical_json).collect();
            format!("[{}]", entries.join(","))
        }
        _ => serde_json::to_string(value).unwrap(),
    }
}

/// Compute a cache key from tool_id, version, policy, and args.
pub fn cache_key(tool_id: &str, version: &str, policy_hash: &str, args_hash: &str) -> String {
    sha256_hash(&format!("{}:{}:{}:{}", tool_id, version, policy_hash, args_hash))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256() {
        let h = sha256_hash("hello");
        assert!(h.starts_with("sha256:"));
        assert_eq!(h.len(), 7 + 64); // "sha256:" + 64 hex chars
    }

    #[test]
    fn test_canonical_json_sorted_keys() {
        let val: serde_json::Value = serde_json::json!({"b": 2, "a": 1});
        let canonical = canonical_json(&val);
        assert_eq!(canonical, r#"{"a":1,"b":2}"#);
    }
}
