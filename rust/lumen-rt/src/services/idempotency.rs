//! Idempotency key tracking for side effects during replay.
//!
//! During deterministic replay, side effects that have already been executed
//! should return cached results instead of re-executing. The [`IdempotencyStore`]
//! maps string keys to cached serialized results and provides a
//! [`check_or_execute`](IdempotencyStore::check_or_execute) method that either
//! returns the cached result or executes the side effect and caches the result.
//!
//! # Example
//!
//! ```rust
//! use lumen_runtime::idempotency::IdempotencyStore;
//!
//! let mut store = IdempotencyStore::new();
//! let result = store.check_or_execute("fetch-user-42", || {
//!     "Alice".to_string()
//! });
//! assert_eq!(result.unwrap(), "Alice");
//!
//! // Second call returns cached result without re-executing
//! let mut executed = false;
//! let result = store.check_or_execute("fetch-user-42", || {
//!     executed = true;
//!     "Bob".to_string()
//! });
//! assert_eq!(result.unwrap(), "Alice");
//! assert!(!executed);
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors from idempotency store operations.
#[derive(Debug, thiserror::Error)]
pub enum IdempotencyError {
    #[error("serialization failed: {0}")]
    Serialize(String),
    #[error("deserialization failed: {0}")]
    Deserialize(String),
}

// ---------------------------------------------------------------------------
// IdempotencyStore
// ---------------------------------------------------------------------------

/// Stores cached results of side effects keyed by idempotency keys.
///
/// During replay, side effects with a previously-seen key return the cached
/// result immediately, avoiding re-execution.
#[derive(Debug, Clone)]
pub struct IdempotencyStore {
    /// Maps idempotency keys to serialized (bincode) result bytes.
    entries: HashMap<String, Vec<u8>>,
}

impl IdempotencyStore {
    /// Create an empty idempotency store.
    pub fn new() -> Self {
        IdempotencyStore {
            entries: HashMap::new(),
        }
    }

    /// Check whether a result is cached for `key`. If so, deserialize and
    /// return it. Otherwise, execute `f()`, serialize and cache the result,
    /// then return it.
    ///
    /// # Errors
    /// Returns [`IdempotencyError`] if serialization or deserialization fails.
    pub fn check_or_execute<F, R>(&mut self, key: &str, f: F) -> Result<R, IdempotencyError>
    where
        F: FnOnce() -> R,
        R: Serialize + for<'de> Deserialize<'de>,
    {
        if let Some(cached) = self.entries.get(key) {
            let result: R = bincode::deserialize(cached)
                .map_err(|e| IdempotencyError::Deserialize(e.to_string()))?;
            return Ok(result);
        }

        let result = f();
        let bytes =
            bincode::serialize(&result).map_err(|e| IdempotencyError::Serialize(e.to_string()))?;
        self.entries.insert(key.to_string(), bytes);
        Ok(result)
    }

    /// Invalidate (remove) a cached result for `key`.
    ///
    /// Returns `true` if the key was present and removed.
    pub fn invalidate(&mut self, key: &str) -> bool {
        self.entries.remove(key).is_some()
    }

    /// Remove all cached results.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Check whether a cached result exists for `key`.
    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// Number of cached results.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over all stored keys.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(|s| s.as_str())
    }

    /// Get the raw cached bytes for a key (for debugging / inspection).
    pub fn get_raw(&self, key: &str) -> Option<&[u8]> {
        self.entries.get(key).map(|v| v.as_slice())
    }

    /// Insert a pre-serialized result directly (useful when hydrating from
    /// a replay log or external source).
    pub fn insert_raw(&mut self, key: String, data: Vec<u8>) {
        self.entries.insert(key, data);
    }
}

impl Default for IdempotencyStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_or_execute_caches_result() {
        let mut store = IdempotencyStore::new();

        let result = store.check_or_execute("key1", || 42i64).unwrap();
        assert_eq!(result, 42);
        assert!(store.contains("key1"));
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn check_or_execute_returns_cached_on_second_call() {
        let mut store = IdempotencyStore::new();

        let r1 = store
            .check_or_execute("key1", || "first".to_string())
            .unwrap();
        assert_eq!(r1, "first");

        // Second call should return the cached value, not execute the closure
        let mut executed = false;
        let r2 = store
            .check_or_execute("key1", || {
                executed = true;
                "second".to_string()
            })
            .unwrap();
        assert_eq!(r2, "first");
        assert!(!executed, "closure should not have been executed");
    }

    #[test]
    fn different_keys_independent() {
        let mut store = IdempotencyStore::new();

        store.check_or_execute("a", || 1i32).unwrap();
        store.check_or_execute("b", || 2i32).unwrap();

        assert_eq!(store.len(), 2);
        assert!(store.contains("a"));
        assert!(store.contains("b"));

        // Values are independent
        let a: i32 = store.check_or_execute("a", || 99).unwrap();
        let b: i32 = store.check_or_execute("b", || 99).unwrap();
        assert_eq!(a, 1);
        assert_eq!(b, 2);
    }

    #[test]
    fn invalidate_removes_key() {
        let mut store = IdempotencyStore::new();
        store.check_or_execute("k", || 10i32).unwrap();
        assert!(store.contains("k"));

        assert!(store.invalidate("k"));
        assert!(!store.contains("k"));
        assert!(store.is_empty());
    }

    #[test]
    fn invalidate_nonexistent_returns_false() {
        let mut store = IdempotencyStore::new();
        assert!(!store.invalidate("nonexistent"));
    }

    #[test]
    fn invalidate_allows_re_execution() {
        let mut store = IdempotencyStore::new();

        store.check_or_execute("k", || "old".to_string()).unwrap();
        store.invalidate("k");

        let result = store.check_or_execute("k", || "new".to_string()).unwrap();
        assert_eq!(result, "new");
    }

    #[test]
    fn clear_removes_all() {
        let mut store = IdempotencyStore::new();
        store.check_or_execute("a", || 1i32).unwrap();
        store.check_or_execute("b", || 2i32).unwrap();
        store.check_or_execute("c", || 3i32).unwrap();
        assert_eq!(store.len(), 3);

        store.clear();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn contains_works() {
        let mut store = IdempotencyStore::new();
        assert!(!store.contains("key"));
        store.check_or_execute("key", || 0i32).unwrap();
        assert!(store.contains("key"));
    }

    #[test]
    fn keys_iterator() {
        let mut store = IdempotencyStore::new();
        store.check_or_execute("alpha", || 1i32).unwrap();
        store.check_or_execute("beta", || 2i32).unwrap();
        store.check_or_execute("gamma", || 3i32).unwrap();

        let mut keys: Vec<&str> = store.keys().collect();
        keys.sort();
        assert_eq!(keys, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn insert_raw_and_retrieve() {
        let mut store = IdempotencyStore::new();

        let data = bincode::serialize(&42i64).unwrap();
        store.insert_raw("preloaded".to_string(), data);

        assert!(store.contains("preloaded"));
        let result: i64 = store.check_or_execute("preloaded", || 99).unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn get_raw_returns_bytes() {
        let mut store = IdempotencyStore::new();
        store.check_or_execute("k", || 123i32).unwrap();

        let raw = store.get_raw("k").unwrap();
        let val: i32 = bincode::deserialize(raw).unwrap();
        assert_eq!(val, 123);

        assert!(store.get_raw("nonexistent").is_none());
    }

    #[test]
    fn default_is_empty() {
        let store = IdempotencyStore::default();
        assert!(store.is_empty());
    }

    #[test]
    fn complex_types() {
        let mut store = IdempotencyStore::new();

        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Point {
            x: f64,
            y: f64,
            label: String,
        }

        let result = store
            .check_or_execute("point", || Point {
                x: 1.5,
                y: 2.5,
                label: "origin".to_string(),
            })
            .unwrap();
        assert_eq!(
            result,
            Point {
                x: 1.5,
                y: 2.5,
                label: "origin".to_string()
            }
        );

        // Second call returns cached
        let cached: Point = store
            .check_or_execute("point", || Point {
                x: 0.0,
                y: 0.0,
                label: "different".to_string(),
            })
            .unwrap();
        assert_eq!(cached.label, "origin");
    }

    #[test]
    fn vec_types() {
        let mut store = IdempotencyStore::new();
        let result: Vec<String> = store
            .check_or_execute("list", || vec!["a".into(), "b".into(), "c".into()])
            .unwrap();
        assert_eq!(result, vec!["a", "b", "c"]);

        // Cached
        let cached: Vec<String> = store.check_or_execute("list", || vec![]).unwrap();
        assert_eq!(cached, vec!["a", "b", "c"]);
    }

    #[test]
    fn replay_simulation() {
        // Simulate a replay scenario: first run records, second run replays
        let mut store = IdempotencyStore::new();
        let mut execution_count = 0;

        // First execution — "live" run
        let r1: String = store
            .check_or_execute("tool-call-1", || {
                execution_count += 1;
                "response-from-tool".to_string()
            })
            .unwrap();
        assert_eq!(r1, "response-from-tool");
        assert_eq!(execution_count, 1);

        // Second execution — "replay" run (same store, same key)
        let r2: String = store
            .check_or_execute("tool-call-1", || {
                execution_count += 1;
                "different-response".to_string()
            })
            .unwrap();
        assert_eq!(r2, "response-from-tool");
        assert_eq!(execution_count, 1, "should not have executed again");
    }
}
