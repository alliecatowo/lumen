//! Content-addressed cache for tool invocation results.

use crate::services::trace::hasher::canonical_hash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub key: String,
    pub tool_id: String,
    pub version: String,
    pub policy_hash: String,
    pub inputs_hash: String,
    pub outputs: serde_json::Value,
}

pub struct CacheStore {
    cache_dir: PathBuf,
    memory: HashMap<String, CacheEntry>,
}

impl CacheStore {
    pub fn new(base_dir: &Path) -> Self {
        let cache_dir = base_dir.join("cache");
        fs::create_dir_all(&cache_dir).ok();
        Self {
            cache_dir,
            memory: HashMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&CacheEntry> {
        self.memory.get(key)
    }

    pub fn put(&mut self, entry: CacheEntry) {
        let path = self
            .cache_dir
            .join(format!("{}.json", &entry.key[7..71.min(entry.key.len())]));
        if let Ok(json) = serde_json::to_string_pretty(&entry) {
            fs::write(&path, json).ok();
        }
        self.memory.insert(entry.key.clone(), entry);
    }

    pub fn lookup(
        &self,
        tool_id: &str,
        version: &str,
        policy_hash: &str,
        args: &serde_json::Value,
    ) -> Option<&CacheEntry> {
        let args_hash = canonical_hash(args);
        let key = crate::services::trace::hasher::cache_key(tool_id, version, policy_hash, &args_hash);
        self.get(&key)
    }
}

// ===========================================================================
// PersistentCache — simple key-value cache backed by a JSON file
// ===========================================================================

/// A persistent key-value string cache backed by a JSON file on disk.
///
/// On construction, any existing cache file is loaded into memory. Writes are
/// flushed to disk immediately (write-through). The file format is a single
/// JSON object mapping string keys to string values.
///
/// # Thread Safety
///
/// `PersistentCache` is **not** thread-safe. Wrap in `Mutex` if shared across
/// threads.
pub struct PersistentCache {
    path: PathBuf,
    data: HashMap<String, String>,
}

impl PersistentCache {
    /// Create or load a persistent cache at `path`.
    ///
    /// If the file exists and contains valid JSON, the cache is pre-populated.
    /// If the file does not exist or is malformed, the cache starts empty (the
    /// file will be created/overwritten on the next write).
    pub fn new(path: PathBuf) -> Self {
        let data = Self::load_from_disk(&path).unwrap_or_default();
        Self { path, data }
    }

    /// Get a value by key. Returns `None` if the key is not present.
    pub fn get(&self, key: &str) -> Option<&String> {
        self.data.get(key)
    }

    /// Set a key-value pair. Flushes to disk immediately.
    ///
    /// Returns `Err` if the disk write fails.
    pub fn set(&mut self, key: &str, value: String) -> Result<(), std::io::Error> {
        self.data.insert(key.to_string(), value);
        self.flush()
    }

    /// Remove a key from the cache. Flushes to disk immediately.
    ///
    /// Returns `true` if the key was present, `false` otherwise.
    pub fn invalidate(&mut self, key: &str) -> Result<bool, std::io::Error> {
        let removed = self.data.remove(key).is_some();
        if removed {
            self.flush()?;
        }
        Ok(removed)
    }

    /// Remove all entries from the cache. Flushes to disk immediately.
    pub fn clear(&mut self) -> Result<(), std::io::Error> {
        self.data.clear();
        self.flush()
    }

    /// Number of entries in the cache.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Return an iterator over all keys.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.data.keys()
    }

    /// Return `true` if the cache contains the given key.
    pub fn contains_key(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    // -- internal ---------------------------------------------------------

    /// Load cache data from a JSON file. Returns `None` if the file doesn't
    /// exist or is malformed.
    fn load_from_disk(path: &Path) -> Option<HashMap<String, String>> {
        let contents = fs::read_to_string(path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    /// Flush current in-memory data to disk as pretty-printed JSON.
    fn flush(&self) -> Result<(), std::io::Error> {
        // Ensure parent directory exists.
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.data).map_err(std::io::Error::other)?;
        fs::write(&self.path, json)
    }
}

impl fmt::Debug for PersistentCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PersistentCache")
            .field("path", &self.path)
            .field("entries", &self.data.len())
            .finish()
    }
}

// ===========================================================================
// Tests for PersistentCache
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Create a temp file path in a temp directory. The directory is created
    /// automatically; the caller is responsible for cleanup.
    fn temp_cache_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("lumen_cache_tests");
        fs::create_dir_all(&dir).unwrap();
        dir.join(format!("{}.json", name))
    }

    fn cleanup(path: &Path) {
        let _ = fs::remove_file(path);
    }

    // =====================================================================
    // 1. New cache starts empty
    // =====================================================================
    #[test]
    fn new_cache_is_empty() {
        let path = temp_cache_path("empty");
        cleanup(&path);
        let cache = PersistentCache::new(path.clone());
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        cleanup(&path);
    }

    // =====================================================================
    // 2. set and get
    // =====================================================================
    #[test]
    fn set_and_get() {
        let path = temp_cache_path("set_get");
        cleanup(&path);
        let mut cache = PersistentCache::new(path.clone());
        cache.set("key1", "value1".to_string()).unwrap();
        assert_eq!(cache.get("key1"), Some(&"value1".to_string()));
        assert_eq!(cache.get("missing"), None);
        cleanup(&path);
    }

    // =====================================================================
    // 3. Persistence across instances
    // =====================================================================
    #[test]
    fn persistence_across_instances() {
        let path = temp_cache_path("persist");
        cleanup(&path);

        {
            let mut cache = PersistentCache::new(path.clone());
            cache.set("alpha", "A".to_string()).unwrap();
            cache.set("beta", "B".to_string()).unwrap();
        }

        // New instance should load from disk
        let cache2 = PersistentCache::new(path.clone());
        assert_eq!(cache2.len(), 2);
        assert_eq!(cache2.get("alpha"), Some(&"A".to_string()));
        assert_eq!(cache2.get("beta"), Some(&"B".to_string()));
        cleanup(&path);
    }

    // =====================================================================
    // 4. invalidate removes key
    // =====================================================================
    #[test]
    fn invalidate_key() {
        let path = temp_cache_path("invalidate");
        cleanup(&path);
        let mut cache = PersistentCache::new(path.clone());
        cache.set("k", "v".to_string()).unwrap();
        assert!(cache.contains_key("k"));

        let removed = cache.invalidate("k").unwrap();
        assert!(removed);
        assert!(!cache.contains_key("k"));
        assert!(cache.is_empty());

        // Non-existent key
        let removed2 = cache.invalidate("nope").unwrap();
        assert!(!removed2);

        cleanup(&path);
    }

    // =====================================================================
    // 5. invalidate persists
    // =====================================================================
    #[test]
    fn invalidate_persists() {
        let path = temp_cache_path("inv_persist");
        cleanup(&path);
        let mut cache = PersistentCache::new(path.clone());
        cache.set("a", "1".to_string()).unwrap();
        cache.set("b", "2".to_string()).unwrap();
        cache.invalidate("a").unwrap();
        drop(cache);

        let cache2 = PersistentCache::new(path.clone());
        assert_eq!(cache2.len(), 1);
        assert_eq!(cache2.get("a"), None);
        assert_eq!(cache2.get("b"), Some(&"2".to_string()));
        cleanup(&path);
    }

    // =====================================================================
    // 6. clear removes all entries
    // =====================================================================
    #[test]
    fn clear_all() {
        let path = temp_cache_path("clear");
        cleanup(&path);
        let mut cache = PersistentCache::new(path.clone());
        cache.set("x", "1".to_string()).unwrap();
        cache.set("y", "2".to_string()).unwrap();
        cache.clear().unwrap();
        assert!(cache.is_empty());
        drop(cache);

        let cache2 = PersistentCache::new(path.clone());
        assert!(cache2.is_empty());
        cleanup(&path);
    }

    // =====================================================================
    // 7. Overwrite existing key
    // =====================================================================
    #[test]
    fn overwrite_existing_key() {
        let path = temp_cache_path("overwrite");
        cleanup(&path);
        let mut cache = PersistentCache::new(path.clone());
        cache.set("k", "old".to_string()).unwrap();
        cache.set("k", "new".to_string()).unwrap();
        assert_eq!(cache.get("k"), Some(&"new".to_string()));

        drop(cache);
        let cache2 = PersistentCache::new(path.clone());
        assert_eq!(cache2.get("k"), Some(&"new".to_string()));
        cleanup(&path);
    }

    // =====================================================================
    // 8. Malformed file on disk starts empty
    // =====================================================================
    #[test]
    fn malformed_file_starts_empty() {
        let path = temp_cache_path("malformed");
        fs::write(&path, "this is not json {{{").unwrap();

        let cache = PersistentCache::new(path.clone());
        assert!(cache.is_empty());
        cleanup(&path);
    }

    // =====================================================================
    // 9. keys() iterator
    // =====================================================================
    #[test]
    fn keys_iterator() {
        let path = temp_cache_path("keys_iter");
        cleanup(&path);
        let mut cache = PersistentCache::new(path.clone());
        cache.set("a", "1".to_string()).unwrap();
        cache.set("b", "2".to_string()).unwrap();
        cache.set("c", "3".to_string()).unwrap();

        let mut keys: Vec<&String> = cache.keys().collect();
        keys.sort();
        assert_eq!(keys, vec!["a", "b", "c"]);
        cleanup(&path);
    }

    // =====================================================================
    // 10. contains_key
    // =====================================================================
    #[test]
    fn contains_key_works() {
        let path = temp_cache_path("contains");
        cleanup(&path);
        let mut cache = PersistentCache::new(path.clone());
        assert!(!cache.contains_key("k"));
        cache.set("k", "v".to_string()).unwrap();
        assert!(cache.contains_key("k"));
        cleanup(&path);
    }

    // =====================================================================
    // 11. Debug format
    // =====================================================================
    #[test]
    fn debug_format() {
        let path = temp_cache_path("debug_fmt");
        cleanup(&path);
        let cache = PersistentCache::new(path.clone());
        let dbg = format!("{:?}", cache);
        assert!(dbg.contains("PersistentCache"));
        assert!(dbg.contains("entries: 0"));
        cleanup(&path);
    }

    // =====================================================================
    // 12. File format is valid JSON
    // =====================================================================
    #[test]
    fn file_format_is_json() {
        let path = temp_cache_path("json_fmt");
        cleanup(&path);
        let mut cache = PersistentCache::new(path.clone());
        cache.set("hello", "world".to_string()).unwrap();
        drop(cache);

        let contents = fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed["hello"], "world");
        cleanup(&path);
    }

    // =====================================================================
    // 13. CacheStore existing tests still work
    // =====================================================================
    #[test]
    fn cache_store_put_and_get() {
        let dir = std::env::temp_dir().join("lumen_cache_tests_store");
        let store = CacheStore::new(&dir);
        assert!(store.get("nonexistent").is_none());
    }
}
