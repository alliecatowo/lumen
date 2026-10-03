//! Content-addressed cache for tool invocation results.

use crate::trace::hasher::canonical_hash;
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

/// Write `bytes` to `path` atomically: write a sibling temp file, fsync it, then
/// rename over the destination so readers (and a crash mid-write) never observe a
/// truncated file.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "cache".to_string());
    let tmp = dir.join(format!(".{file_name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// File-name stem for a cache key: the part after any `algo:` prefix with every
/// character that is not `[A-Za-z0-9_-]` dropped, capped at 64 characters. Keys
/// that sanitize to nothing (or collide) fall back to a hash of the whole key.
fn cache_file_stem(key: &str) -> String {
    let tail = key.rsplit(':').next().unwrap_or(key);
    let cleaned: String = tail
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(64)
        .collect();
    if cleaned.is_empty() || cleaned.len() < 8 || tail.len() != cleaned.len() {
        // Too short or altered by sanitising: make the name unambiguous.
        let digest = canonical_hash(&serde_json::Value::String(key.to_string()));
        let digest: String = digest
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(32)
            .collect();
        return format!("k-{digest}");
    }
    cleaned
}

impl CacheStore {
    /// Open (creating if needed) the cache under `base_dir/cache` and load every
    /// valid entry already on disk. Malformed files are skipped, never deleted.
    pub fn new(base_dir: &Path) -> Self {
        let cache_dir = base_dir.join("cache");
        fs::create_dir_all(&cache_dir).ok();
        let mut memory = HashMap::new();
        if let Ok(rd) = fs::read_dir(&cache_dir) {
            for entry in rd.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(text) = fs::read_to_string(&path) {
                    if let Ok(e) = serde_json::from_str::<CacheEntry>(&text) {
                        memory.insert(e.key.clone(), e);
                    }
                }
            }
        }
        Self { cache_dir, memory }
    }

    pub fn get(&self, key: &str) -> Option<&CacheEntry> {
        self.memory.get(key)
    }

    /// Store an entry in memory and on disk (atomically). The in-memory entry is
    /// kept even if the disk write fails; the error is returned so callers can
    /// surface it.
    pub fn put(&mut self, entry: CacheEntry) -> std::io::Result<()> {
        let path = self
            .cache_dir
            .join(format!("{}.json", cache_file_stem(&entry.key)));
        let written = serde_json::to_vec_pretty(&entry)
            .map_err(std::io::Error::other)
            .and_then(|json| atomic_write(&path, &json));
        self.memory.insert(entry.key.clone(), entry);
        written
    }

    pub fn lookup(
        &self,
        tool_id: &str,
        version: &str,
        policy_hash: &str,
        args: &serde_json::Value,
    ) -> Option<&CacheEntry> {
        let args_hash = canonical_hash(args);
        let key = crate::trace::hasher::cache_key(tool_id, version, policy_hash, &args_hash);
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
    /// If the file does not exist the cache starts empty. A malformed file is
    /// moved aside to `<path>.corrupt` (see [`open`](Self::open)) rather than
    /// being overwritten, and the cache starts empty.
    pub fn new(path: PathBuf) -> Self {
        match Self::open(path.clone()) {
            Ok(cache) => cache,
            Err(_) => Self {
                path,
                data: HashMap::new(),
            },
        }
    }

    /// Like [`new`](Self::new) but reports problems. A missing file is an empty
    /// cache. A file that exists but cannot be parsed is moved aside to
    /// `<path>.corrupt` (never overwritten by later writes) and an error is
    /// returned.
    pub fn open(path: PathBuf) -> Result<Self, std::io::Error> {
        let contents = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    path,
                    data: HashMap::new(),
                })
            }
            Err(e) => return Err(e),
        };
        match serde_json::from_str::<HashMap<String, String>>(&contents) {
            Ok(data) => Ok(Self { path, data }),
            Err(e) => {
                let mut quarantine = path.clone().into_os_string();
                quarantine.push(".corrupt");
                let _ = fs::rename(&path, PathBuf::from(&quarantine));
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "cache file {} is malformed ({e}); moved to {}",
                        path.display(),
                        PathBuf::from(quarantine).display()
                    ),
                ))
            }
        }
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

    /// Flush current in-memory data to disk as pretty-printed JSON.
    fn flush(&self) -> Result<(), std::io::Error> {
        // Ensure parent directory exists.
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(&self.data).map_err(std::io::Error::other)?;
        atomic_write(&self.path, &json)
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

    // =====================================================================
    // Durability: CacheStore
    // =====================================================================
    fn entry(key: &str) -> CacheEntry {
        CacheEntry {
            key: key.to_string(),
            tool_id: "t".into(),
            version: "1".into(),
            policy_hash: "p".into(),
            inputs_hash: "i".into(),
            outputs: serde_json::json!({"ok": true}),
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lumen_cache_store_{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn cache_store_accepts_short_and_odd_keys() {
        let dir = temp_dir("short");
        let mut store = CacheStore::new(&dir);
        for key in [
            "",
            "a",
            "abc",
            "sha256:ab",
            "../../escape",
            "with space/and:colons",
        ] {
            store
                .put(entry(key))
                .unwrap_or_else(|e| panic!("key {key:?}: {e}"));
            assert_eq!(store.get(key).unwrap().key, key);
        }
        // Nothing escaped the cache directory.
        assert!(!dir.join("escape.json").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_store_reloads_entries_from_disk() {
        let dir = temp_dir("reload");
        let key = format!("sha256:{}", "a1b2c3d4".repeat(8));
        {
            let mut store = CacheStore::new(&dir);
            store.put(entry(&key)).unwrap();
            store.put(entry("short")).unwrap();
        }
        // A malformed sibling must not prevent loading the rest.
        fs::write(dir.join("cache").join("garbage.json"), "{not json").unwrap();
        let store = CacheStore::new(&dir);
        assert!(store.get(&key).is_some());
        assert!(store.get("short").is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_store_surfaces_write_errors() {
        let dir = temp_dir("err");
        let mut store = CacheStore::new(&dir);
        // Replace the cache directory with a file so writes must fail.
        fs::remove_dir_all(dir.join("cache")).unwrap();
        fs::write(dir.join("cache"), "not a dir").unwrap();
        assert!(store.put(entry("sha256:deadbeefdeadbeef")).is_err());
        // The in-memory entry is still served.
        assert!(store.get("sha256:deadbeefdeadbeef").is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    // =====================================================================
    // Durability: PersistentCache
    // =====================================================================
    #[test]
    fn persistent_cache_flush_leaves_no_temp_files() {
        let dir = temp_dir("atomic");
        let path = dir.join("c.json");
        let mut cache = PersistentCache::new(path.clone());
        for i in 0..20 {
            cache.set(&format!("k{i}"), "v".repeat(i)).unwrap();
        }
        let names: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["c.json".to_string()], "stray files: {names:?}");
        assert_eq!(PersistentCache::new(path).len(), 20);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_cache_file_is_quarantined_not_overwritten() {
        let dir = temp_dir("corrupt");
        let path = dir.join("c.json");
        fs::write(&path, "{\"half\": \"writ").unwrap();
        let err = PersistentCache::open(path.clone()).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        // The damaged bytes are preserved for inspection...
        let kept = fs::read_to_string(dir.join("c.json.corrupt")).unwrap();
        assert!(kept.contains("half"));
        // ...and a fresh cache can be created and written afterwards.
        let mut cache = PersistentCache::new(path.clone());
        cache.set("a", "b".into()).unwrap();
        assert_eq!(PersistentCache::new(path).get("a"), Some(&"b".to_string()));
        assert!(dir.join("c.json.corrupt").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
