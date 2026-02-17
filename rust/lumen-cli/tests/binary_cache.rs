//! Integration tests for the binary cache module.

use lumen_cli::binary_cache::*;

// =============================================================================
// Helper
// =============================================================================

fn temp_cache_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir()
        .join("lumen_test_binary_cache")
        .join(name);
    // Clean up from any prior run
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn make_key(src: &str, ver: &str, target: &str, opt: OptLevel) -> CacheKey {
    CacheKey {
        source_hash: BinaryCache::compute_source_hash(src),
        compiler_version: ver.to_string(),
        target: target.to_string(),
        optimization_level: opt,
    }
}

// =============================================================================
// OptLevel
// =============================================================================

#[test]
fn wave24_binary_cache_opt_level_display_debug() {
    assert_eq!(OptLevel::Debug.to_string(), "debug");
}

#[test]
fn wave24_binary_cache_opt_level_display_release() {
    assert_eq!(OptLevel::Release.to_string(), "release");
}

#[test]
fn wave24_binary_cache_opt_level_display_size() {
    assert_eq!(OptLevel::Size.to_string(), "size");
}

#[test]
fn wave24_binary_cache_opt_level_parse_valid() {
    assert_eq!("debug".parse::<OptLevel>().unwrap(), OptLevel::Debug);
    assert_eq!("release".parse::<OptLevel>().unwrap(), OptLevel::Release);
    assert_eq!("size".parse::<OptLevel>().unwrap(), OptLevel::Size);
}

#[test]
fn wave24_binary_cache_opt_level_parse_invalid() {
    assert!("turbo".parse::<OptLevel>().is_err());
    assert!("".parse::<OptLevel>().is_err());
}

#[test]
fn wave24_binary_cache_opt_level_serde_roundtrip() {
    for opt in [OptLevel::Debug, OptLevel::Release, OptLevel::Size] {
        let json = serde_json::to_string(&opt).unwrap();
        let back: OptLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, opt);
    }
}

// =============================================================================
// CacheKey
// =============================================================================

#[test]
fn wave24_binary_cache_key_hex_deterministic() {
    let k1 = make_key("fn main() end", "0.4.0", "default", OptLevel::Debug);
    let k2 = make_key("fn main() end", "0.4.0", "default", OptLevel::Debug);
    assert_eq!(k1.to_hex(), k2.to_hex());
}

#[test]
fn wave24_binary_cache_key_hex_varies_source() {
    let k1 = make_key("source_a", "0.4.0", "default", OptLevel::Debug);
    let k2 = make_key("source_b", "0.4.0", "default", OptLevel::Debug);
    assert_ne!(k1.to_hex(), k2.to_hex());
}

#[test]
fn wave24_binary_cache_key_hex_varies_version() {
    let k1 = make_key("src", "0.4.0", "default", OptLevel::Debug);
    let k2 = make_key("src", "0.5.0", "default", OptLevel::Debug);
    assert_ne!(k1.to_hex(), k2.to_hex());
}

#[test]
fn wave24_binary_cache_key_hex_varies_target() {
    let k1 = make_key("src", "0.4.0", "x86_64-linux", OptLevel::Debug);
    let k2 = make_key("src", "0.4.0", "wasm32-wasi", OptLevel::Debug);
    assert_ne!(k1.to_hex(), k2.to_hex());
}

#[test]
fn wave24_binary_cache_key_hex_varies_opt() {
    let k1 = make_key("src", "0.4.0", "default", OptLevel::Debug);
    let k2 = make_key("src", "0.4.0", "default", OptLevel::Release);
    assert_ne!(k1.to_hex(), k2.to_hex());
}

#[test]
fn wave24_binary_cache_key_display_truncates() {
    let k = make_key("src", "0.4.0", "default", OptLevel::Debug);
    let display = format!("{}", k);
    assert_eq!(display.len(), 16);
}

#[test]
fn wave24_binary_cache_key_serde_roundtrip() {
    let k = make_key("src", "0.4.0", "default", OptLevel::Release);
    let json = serde_json::to_string(&k).unwrap();
    let back: CacheKey = serde_json::from_str(&json).unwrap();
    assert_eq!(back, k);
}

// =============================================================================
// compute_source_hash
// =============================================================================

#[test]
fn wave24_binary_cache_source_hash_deterministic() {
    let h1 = BinaryCache::compute_source_hash("cell main() -> Int 42 end");
    let h2 = BinaryCache::compute_source_hash("cell main() -> Int 42 end");
    assert_eq!(h1, h2);
}

#[test]
fn wave24_binary_cache_source_hash_differs() {
    let h1 = BinaryCache::compute_source_hash("aaa");
    let h2 = BinaryCache::compute_source_hash("bbb");
    assert_ne!(h1, h2);
}

#[test]
fn wave24_binary_cache_source_hash_length() {
    // SHA-256 produces 64 hex characters
    let h = BinaryCache::compute_source_hash("test");
    assert_eq!(h.len(), 64);
}

#[test]
fn wave24_binary_cache_source_hash_empty_input() {
    let h = BinaryCache::compute_source_hash("");
    assert_eq!(h.len(), 64);
}

// =============================================================================
// cache_key_to_path
// =============================================================================

#[test]
fn wave24_binary_cache_key_to_path_format() {
    let k = make_key("src", "0.4.0", "default", OptLevel::Debug);
    let p = BinaryCache::cache_key_to_path(&k);
    assert!(p.starts_with("artifacts/"));
    assert!(p.ends_with(".bin"));
}

// =============================================================================
// BinaryCache — new / with_max_size
// =============================================================================

#[test]
fn wave24_binary_cache_new_creates_dir() {
    let dir = temp_cache_dir("new_creates_dir");
    let cache = BinaryCache::new(dir.clone()).unwrap();
    assert!(dir.exists());
    assert_eq!(cache.entry_count(), 0);
    assert_eq!(cache.total_size(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave24_binary_cache_with_max_size() {
    let dir = temp_cache_dir("with_max_size");
    let cache = BinaryCache::with_max_size(dir.clone(), 512).unwrap();
    assert_eq!(cache.max_size_bytes(), 512);
    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// BinaryCache — put / get
// =============================================================================

#[test]
fn wave24_binary_cache_put_and_get() {
    let dir = temp_cache_dir("put_and_get");
    let mut cache = BinaryCache::new(dir.clone()).unwrap();

    let key = make_key("hello", "0.4.0", "default", OptLevel::Debug);
    let data = b"compiled bytecode";
    let entry = cache.put(key.clone(), data).unwrap();

    assert_eq!(entry.size_bytes, data.len() as u64);
    assert!(cache.get(&key).is_some());
    assert_eq!(cache.entry_count(), 1);
    assert_eq!(cache.total_size(), data.len() as u64);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave24_binary_cache_get_miss() {
    let dir = temp_cache_dir("get_miss");
    let cache = BinaryCache::new(dir.clone()).unwrap();
    let key = make_key("missing", "0.4.0", "default", OptLevel::Debug);
    assert!(cache.get(&key).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave24_binary_cache_put_overwrites() {
    let dir = temp_cache_dir("put_overwrites");
    let mut cache = BinaryCache::new(dir.clone()).unwrap();

    let key = make_key("src", "0.4.0", "default", OptLevel::Debug);
    cache.put(key.clone(), b"v1").unwrap();
    cache.put(key.clone(), b"v2v2").unwrap();

    let entry = cache.get(&key).unwrap();
    assert_eq!(entry.size_bytes, 4);
    assert_eq!(cache.entry_count(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave24_binary_cache_put_multiple_keys() {
    let dir = temp_cache_dir("put_multiple");
    let mut cache = BinaryCache::new(dir.clone()).unwrap();

    let k1 = make_key("a", "0.4.0", "default", OptLevel::Debug);
    let k2 = make_key("b", "0.4.0", "default", OptLevel::Debug);
    cache.put(k1.clone(), b"aaa").unwrap();
    cache.put(k2.clone(), b"bbbb").unwrap();

    assert_eq!(cache.entry_count(), 2);
    assert_eq!(cache.total_size(), 7);
    assert!(cache.get(&k1).is_some());
    assert!(cache.get(&k2).is_some());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave24_binary_cache_put_artifact_on_disk() {
    let dir = temp_cache_dir("artifact_on_disk");
    let mut cache = BinaryCache::new(dir.clone()).unwrap();

    let key = make_key("disk_check", "0.4.0", "default", OptLevel::Release);
    let data = b"on disk artifact";
    cache.put(key.clone(), data).unwrap();

    let entry = cache.get(&key).unwrap();
    let full = dir.join(&entry.artifact_path);
    assert!(full.exists());
    let on_disk = std::fs::read(&full).unwrap();
    assert_eq!(on_disk, data);

    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// BinaryCache — invalidate
// =============================================================================

#[test]
fn wave24_binary_cache_invalidate_existing() {
    let dir = temp_cache_dir("invalidate_existing");
    let mut cache = BinaryCache::new(dir.clone()).unwrap();

    let key = make_key("rm_me", "0.4.0", "default", OptLevel::Debug);
    cache.put(key.clone(), b"bye").unwrap();
    assert!(cache.invalidate(&key));
    assert!(cache.get(&key).is_none());
    assert_eq!(cache.entry_count(), 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave24_binary_cache_invalidate_missing() {
    let dir = temp_cache_dir("invalidate_missing");
    let mut cache = BinaryCache::new(dir.clone()).unwrap();
    let key = make_key("no_such", "0.4.0", "default", OptLevel::Debug);
    assert!(!cache.invalidate(&key));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave24_binary_cache_invalidate_all() {
    let dir = temp_cache_dir("invalidate_all");
    let mut cache = BinaryCache::new(dir.clone()).unwrap();

    for i in 0..5 {
        let key = make_key(&format!("src{}", i), "0.4.0", "default", OptLevel::Debug);
        cache.put(key, b"data").unwrap();
    }
    assert_eq!(cache.entry_count(), 5);
    let removed = cache.invalidate_all();
    assert_eq!(removed, 5);
    assert_eq!(cache.entry_count(), 0);
    assert_eq!(cache.total_size(), 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave24_binary_cache_invalidate_all_empty() {
    let dir = temp_cache_dir("invalidate_all_empty");
    let mut cache = BinaryCache::new(dir.clone()).unwrap();
    assert_eq!(cache.invalidate_all(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// BinaryCache — evict_lru
// =============================================================================

#[test]
fn wave24_binary_cache_evict_lru_frees_space() {
    let dir = temp_cache_dir("evict_lru");
    let mut cache = BinaryCache::with_max_size(dir.clone(), 1_000_000).unwrap();

    // Insert with manually varied last_accessed so LRU order is deterministic
    let k1 = make_key("old", "0.4.0", "default", OptLevel::Debug);
    let k2 = make_key("new", "0.4.0", "default", OptLevel::Debug);
    cache.put(k1.clone(), &[0u8; 100]).unwrap();
    cache.put(k2.clone(), &[0u8; 100]).unwrap();

    // Make k1 the oldest
    let hex1 = k1.to_hex();
    if let Some(e) = cache.entries_mut().get_mut(&hex1) {
        e.last_accessed = 1;
    }

    let evicted = cache.evict_lru(100);
    assert_eq!(evicted, 1);
    assert!(cache.get(&k1).is_none());
    assert!(cache.get(&k2).is_some());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave24_binary_cache_evict_lru_zero_target() {
    let dir = temp_cache_dir("evict_zero");
    let mut cache = BinaryCache::new(dir.clone()).unwrap();
    assert_eq!(cache.evict_lru(0), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave24_binary_cache_evict_lru_all() {
    let dir = temp_cache_dir("evict_all");
    let mut cache = BinaryCache::new(dir.clone()).unwrap();
    for i in 0..3 {
        let key = make_key(&format!("e{}", i), "0.4.0", "default", OptLevel::Debug);
        cache.put(key, &[0u8; 50]).unwrap();
    }
    let evicted = cache.evict_lru(200);
    assert_eq!(evicted, 3);
    assert_eq!(cache.entry_count(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// BinaryCache — prune_expired
// =============================================================================

#[test]
fn wave24_binary_cache_prune_expired_removes_old() {
    let dir = temp_cache_dir("prune_old");
    let mut cache = BinaryCache::new(dir.clone()).unwrap();

    let key = make_key("old_entry", "0.4.0", "default", OptLevel::Debug);
    cache.put(key.clone(), b"stale").unwrap();

    // Backdate the entry to make it expired
    let hex = key.to_hex();
    if let Some(e) = cache.entries_mut().get_mut(&hex) {
        e.created_at = 1; // epoch + 1s
    }

    // Prune anything older than 10 seconds — the backdated entry qualifies.
    let pruned = cache.prune_expired(10);
    assert_eq!(pruned, 1);
    assert_eq!(cache.entry_count(), 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave24_binary_cache_prune_expired_keeps_fresh() {
    let dir = temp_cache_dir("prune_fresh");
    let mut cache = BinaryCache::new(dir.clone()).unwrap();

    let key = make_key("fresh", "0.4.0", "default", OptLevel::Debug);
    cache.put(key.clone(), b"fresh_data").unwrap();

    // Very large max_age — nothing should be pruned
    let pruned = cache.prune_expired(999_999_999);
    assert_eq!(pruned, 0);
    assert_eq!(cache.entry_count(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// BinaryCache — total_size / entry_count
// =============================================================================

#[test]
fn wave24_binary_cache_total_size_accumulates() {
    let dir = temp_cache_dir("total_size");
    let mut cache = BinaryCache::new(dir.clone()).unwrap();

    let k1 = make_key("a", "0.4.0", "default", OptLevel::Debug);
    let k2 = make_key("b", "0.4.0", "default", OptLevel::Debug);
    cache.put(k1, &[0u8; 100]).unwrap();
    cache.put(k2, &[0u8; 200]).unwrap();

    assert_eq!(cache.total_size(), 300);
    assert_eq!(cache.entry_count(), 2);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave24_binary_cache_size_after_invalidate() {
    let dir = temp_cache_dir("size_after_inv");
    let mut cache = BinaryCache::new(dir.clone()).unwrap();

    let key = make_key("tmp", "0.4.0", "default", OptLevel::Debug);
    cache.put(key.clone(), &[0u8; 100]).unwrap();
    assert_eq!(cache.total_size(), 100);

    cache.invalidate(&key);
    assert_eq!(cache.total_size(), 0);

    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// BinaryCache — save_index / load_index persistence
// =============================================================================

#[test]
fn wave24_binary_cache_save_and_load_index() {
    let dir = temp_cache_dir("save_load_idx");
    {
        let mut cache = BinaryCache::new(dir.clone()).unwrap();
        let key = make_key("persist", "0.4.0", "default", OptLevel::Release);
        cache.put(key, b"bytes").unwrap();
        cache.save_index().unwrap();
    }

    // Re-open from disk
    let loaded = BinaryCache::load_index(&dir).unwrap();
    assert_eq!(loaded.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave24_binary_cache_load_index_empty_dir() {
    let dir = temp_cache_dir("load_empty");
    std::fs::create_dir_all(&dir).unwrap();
    let loaded = BinaryCache::load_index(&dir).unwrap();
    assert!(loaded.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave24_binary_cache_load_index_corrupt_json() {
    let dir = temp_cache_dir("corrupt_index");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("index.json"), "not valid json!!!").unwrap();
    let result = BinaryCache::load_index(&dir);
    assert!(result.is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// BinaryCache — capacity enforcement
// =============================================================================

#[test]
fn wave24_binary_cache_put_evicts_when_full() {
    let dir = temp_cache_dir("evict_when_full");
    // Max 150 bytes
    let mut cache = BinaryCache::with_max_size(dir.clone(), 150).unwrap();

    let k1 = make_key("first", "0.4.0", "default", OptLevel::Debug);
    cache.put(k1.clone(), &[0u8; 100]).unwrap();

    // Force k1 to be oldest
    let hex1 = k1.to_hex();
    if let Some(e) = cache.entries_mut().get_mut(&hex1) {
        e.last_accessed = 1;
    }

    // This needs 100 more bytes but only 50 free → must evict k1
    let k2 = make_key("second", "0.4.0", "default", OptLevel::Debug);
    cache.put(k2.clone(), &[0u8; 100]).unwrap();

    assert!(cache.get(&k1).is_none());
    assert!(cache.get(&k2).is_some());

    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// CacheError Display
// =============================================================================

#[test]
fn wave24_binary_cache_error_display_io() {
    let e = CacheError::IoError("disk full".into());
    assert!(e.to_string().contains("disk full"));
}

#[test]
fn wave24_binary_cache_error_display_serialization() {
    let e = CacheError::SerializationError("bad json".into());
    assert!(e.to_string().contains("bad json"));
}

#[test]
fn wave24_binary_cache_error_display_corrupted() {
    let e = CacheError::CorruptedEntry("bad checksum".into());
    assert!(e.to_string().contains("bad checksum"));
}

#[test]
fn wave24_binary_cache_error_display_full() {
    let e = CacheError::CacheFull(999);
    let s = e.to_string();
    assert!(s.contains("999"));
}

#[test]
fn wave24_binary_cache_error_display_invalid_key() {
    let e = CacheError::InvalidKey("empty".into());
    assert!(e.to_string().contains("empty"));
}

#[test]
fn wave24_binary_cache_error_eq() {
    let a = CacheError::IoError("x".into());
    let b = CacheError::IoError("x".into());
    assert_eq!(a, b);
    let c = CacheError::IoError("y".into());
    assert_ne!(a, c);
}

// =============================================================================
// CacheEntry fields
// =============================================================================

#[test]
fn wave24_binary_cache_entry_has_timestamp() {
    let dir = temp_cache_dir("entry_ts");
    let mut cache = BinaryCache::new(dir.clone()).unwrap();
    let key = make_key("ts", "0.4.0", "default", OptLevel::Debug);
    let entry = cache.put(key, b"x").unwrap();
    assert!(entry.created_at > 0);
    assert!(entry.last_accessed > 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave24_binary_cache_entry_dependencies_default_empty() {
    let dir = temp_cache_dir("entry_deps");
    let mut cache = BinaryCache::new(dir.clone()).unwrap();
    let key = make_key("deps", "0.4.0", "default", OptLevel::Debug);
    let entry = cache.put(key, b"x").unwrap();
    assert!(entry.dependencies.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave24_binary_cache_entry_artifact_path_relative() {
    let dir = temp_cache_dir("entry_relpath");
    let mut cache = BinaryCache::new(dir.clone()).unwrap();
    let key = make_key("rel", "0.4.0", "default", OptLevel::Debug);
    let entry = cache.put(key, b"data").unwrap();
    assert!(entry.artifact_path.starts_with("artifacts/"));
    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================
// Edge cases
// =============================================================================

#[test]
fn wave24_binary_cache_empty_artifact_data() {
    let dir = temp_cache_dir("empty_data");
    let mut cache = BinaryCache::new(dir.clone()).unwrap();
    let key = make_key("empty", "0.4.0", "default", OptLevel::Debug);
    let entry = cache.put(key, b"").unwrap();
    assert_eq!(entry.size_bytes, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave24_binary_cache_large_source_hash() {
    // Ensure hashing a large string works without panicking
    let big = "x".repeat(1_000_000);
    let h = BinaryCache::compute_source_hash(&big);
    assert_eq!(h.len(), 64);
}

#[test]
fn wave24_binary_cache_unicode_source_hash() {
    let h1 = BinaryCache::compute_source_hash("hello unicode");
    let h2 = BinaryCache::compute_source_hash("hello unicode");
    assert_eq!(h1, h2);
    assert_eq!(h1.len(), 64);
}

#[test]
fn wave24_binary_cache_reopen_preserves_entries() {
    let dir = temp_cache_dir("reopen");
    {
        let mut cache = BinaryCache::new(dir.clone()).unwrap();
        let key = make_key("keep", "0.4.0", "default", OptLevel::Debug);
        cache.put(key, b"keep_me").unwrap();
        cache.save_index().unwrap();
    }
    // Reopen
    let cache2 = BinaryCache::new(dir.clone()).unwrap();
    assert_eq!(cache2.entry_count(), 1);
    assert_eq!(cache2.total_size(), 7);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave24_binary_cache_cache_dir_accessor() {
    let dir = temp_cache_dir("accessor_dir");
    let cache = BinaryCache::new(dir.clone()).unwrap();
    assert_eq!(cache.cache_dir(), dir.as_path());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave24_binary_cache_max_size_accessor() {
    let dir = temp_cache_dir("accessor_maxsz");
    let cache = BinaryCache::with_max_size(dir.clone(), 42).unwrap();
    assert_eq!(cache.max_size_bytes(), 42);
    let _ = std::fs::remove_dir_all(&dir);
}
