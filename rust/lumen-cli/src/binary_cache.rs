//! Binary caching layer for compiled Lumen packages.
//!
//! Avoids redundant compilation by caching compiled artifacts keyed by
//! a content-addressable hash that incorporates source content, compiler
//! version, target triple, and optimization level.
//!
//! ## Cache Structure
//!
//! ```text
//! <cache_dir>/
//! +-- index.json           # Serialized entry metadata
//! +-- artifacts/
//!     +-- <hex_key>.bin    # Compiled artifact blobs
//! ```

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

// =============================================================================
// Optimization Level
// =============================================================================

/// Optimization level used during compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OptLevel {
    /// Debug build — no optimizations.
    Debug,
    /// Release build — standard optimizations.
    Release,
    /// Size-optimized build.
    Size,
}

impl fmt::Display for OptLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OptLevel::Debug => write!(f, "debug"),
            OptLevel::Release => write!(f, "release"),
            OptLevel::Size => write!(f, "size"),
        }
    }
}

impl std::str::FromStr for OptLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "debug" => Ok(OptLevel::Debug),
            "release" => Ok(OptLevel::Release),
            "size" => Ok(OptLevel::Size),
            other => Err(format!("invalid optimization level: '{}'", other)),
        }
    }
}

impl serde::Serialize for OptLevel {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for OptLevel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

// =============================================================================
// CacheKey
// =============================================================================

/// Content-addressable key for a cached compilation artifact.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CacheKey {
    /// SHA-256 hex digest of the source content.
    pub source_hash: String,
    /// Compiler version string (e.g. "0.4.0").
    pub compiler_version: String,
    /// Target triple or "default".
    pub target: String,
    /// Optimization level used during compilation.
    pub optimization_level: OptLevel,
}

impl CacheKey {
    /// Derive the canonical string representation used for indexing.
    pub fn to_hex(&self) -> String {
        let combined = format!(
            "{}:{}:{}:{}",
            self.source_hash, self.compiler_version, self.target, self.optimization_level
        );
        let digest = Sha256::digest(combined.as_bytes());
        hex_encode(&digest)
    }
}

impl fmt::Display for CacheKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex = self.to_hex();
        write!(f, "{}", &hex[..16.min(hex.len())])
    }
}

// =============================================================================
// CacheEntry
// =============================================================================

/// Metadata for a single cached artifact.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheEntry {
    /// The key that produced this entry.
    pub key: CacheKey,
    /// Relative path to the artifact file inside the cache directory.
    pub artifact_path: PathBuf,
    /// Unix timestamp when the entry was created.
    pub created_at: u64,
    /// Size of the artifact in bytes.
    pub size_bytes: u64,
    /// Hashes of dependency artifacts that were used.
    pub dependencies: Vec<String>,
    /// Last time this entry was accessed (for LRU eviction).
    #[serde(default)]
    pub last_accessed: u64,
}

// =============================================================================
// CacheError
// =============================================================================

/// Errors produced by [`BinaryCache`] operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheError {
    /// An I/O operation failed.
    IoError(String),
    /// Serialization or deserialization failed.
    SerializationError(String),
    /// A cached entry failed integrity checks.
    CorruptedEntry(String),
    /// The cache is full; contains current size.
    CacheFull(u64),
    /// The provided key is invalid.
    InvalidKey(String),
}

impl fmt::Display for CacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CacheError::IoError(msg) => write!(f, "cache I/O error: {}", msg),
            CacheError::SerializationError(msg) => {
                write!(f, "cache serialization error: {}", msg)
            }
            CacheError::CorruptedEntry(msg) => write!(f, "corrupted cache entry: {}", msg),
            CacheError::CacheFull(size) => {
                write!(f, "cache is full ({} bytes used)", size)
            }
            CacheError::InvalidKey(msg) => write!(f, "invalid cache key: {}", msg),
        }
    }
}

impl std::error::Error for CacheError {}

// =============================================================================
// BinaryCache
// =============================================================================

/// Default maximum cache size: 1 GiB.
const DEFAULT_MAX_SIZE: u64 = 1024 * 1024 * 1024;

/// Main binary cache manager.
///
/// Stores compiled artifacts keyed by content hash, compiler version, target,
/// and optimization level.  Supports LRU eviction, expiration pruning, and
/// persistence of the index to disk.
#[derive(Debug)]
pub struct BinaryCache {
    cache_dir: PathBuf,
    max_size_bytes: u64,
    entries: HashMap<String, CacheEntry>,
}

impl BinaryCache {
    /// Create a new cache rooted at `cache_dir` with the default 1 GiB limit.
    pub fn new(cache_dir: PathBuf) -> Result<Self, CacheError> {
        Self::with_max_size(cache_dir, DEFAULT_MAX_SIZE)
    }

    /// Create a new cache with an explicit size limit.
    pub fn with_max_size(cache_dir: PathBuf, max_bytes: u64) -> Result<Self, CacheError> {
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| CacheError::IoError(format!("failed to create cache dir: {}", e)))?;

        let entries = Self::load_index(&cache_dir).unwrap_or_default();

        Ok(Self {
            cache_dir,
            max_size_bytes: max_bytes,
            entries,
        })
    }

    // -- core operations -----------------------------------------------------

    /// Look up a cached entry by key.  Returns `None` on miss.
    pub fn get(&self, key: &CacheKey) -> Option<&CacheEntry> {
        let hex = key.to_hex();
        self.entries.get(&hex)
    }

    /// Insert (or replace) a compiled artifact into the cache.
    ///
    /// The raw `artifact_data` bytes are written to disk under the cache
    /// directory and the index is updated accordingly.
    pub fn put(&mut self, key: CacheKey, artifact_data: &[u8]) -> Result<CacheEntry, CacheError> {
        let hex = key.to_hex();
        if hex.is_empty() {
            return Err(CacheError::InvalidKey("empty key hash".into()));
        }

        let data_size = artifact_data.len() as u64;

        // Evict if necessary
        if self.max_size_bytes > 0 {
            let current = self.total_size();
            let needed = current.saturating_add(data_size);
            if needed > self.max_size_bytes {
                let overshoot = needed.saturating_sub(self.max_size_bytes);
                let evicted = self.evict_lru(overshoot);
                // After eviction, check again
                if evicted == 0 && data_size > self.max_size_bytes {
                    return Err(CacheError::CacheFull(self.total_size()));
                }
            }
        }

        // Write artifact to disk
        let artifacts_dir = self.cache_dir.join("artifacts");
        std::fs::create_dir_all(&artifacts_dir)
            .map_err(|e| CacheError::IoError(format!("mkdir artifacts: {}", e)))?;

        let artifact_filename = format!("{}.bin", hex);
        let artifact_path = artifacts_dir.join(&artifact_filename);
        std::fs::write(&artifact_path, artifact_data)
            .map_err(|e| CacheError::IoError(format!("write artifact: {}", e)))?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let entry = CacheEntry {
            key,
            artifact_path: PathBuf::from("artifacts").join(&artifact_filename),
            created_at: now,
            size_bytes: data_size,
            dependencies: Vec::new(),
            last_accessed: now,
        };

        self.entries.insert(hex, entry.clone());
        Ok(entry)
    }

    /// Remove a single entry.  Returns `true` if the entry existed.
    pub fn invalidate(&mut self, key: &CacheKey) -> bool {
        let hex = key.to_hex();
        if let Some(entry) = self.entries.remove(&hex) {
            let full_path = self.cache_dir.join(&entry.artifact_path);
            let _ = std::fs::remove_file(full_path);
            true
        } else {
            false
        }
    }

    /// Remove **all** entries.  Returns the number of entries removed.
    pub fn invalidate_all(&mut self) -> usize {
        let count = self.entries.len();
        for entry in self.entries.values() {
            let full_path = self.cache_dir.join(&entry.artifact_path);
            let _ = std::fs::remove_file(full_path);
        }
        self.entries.clear();
        count
    }

    // -- maintenance ---------------------------------------------------------

    /// Evict least-recently-used entries until at least `target_bytes` have
    /// been freed.  Returns the number of entries evicted.
    pub fn evict_lru(&mut self, target_bytes: u64) -> usize {
        if target_bytes == 0 {
            return 0;
        }

        // Sort entries by last_accessed ascending (oldest first)
        let mut sorted: Vec<(String, u64, u64)> = self
            .entries
            .iter()
            .map(|(k, e)| (k.clone(), e.last_accessed, e.size_bytes))
            .collect();
        sorted.sort_by_key(|(_, ts, _)| *ts);

        let mut freed: u64 = 0;
        let mut evicted: usize = 0;

        for (hex, _, _size) in &sorted {
            if freed >= target_bytes {
                break;
            }
            if let Some(entry) = self.entries.remove(hex) {
                let full_path = self.cache_dir.join(&entry.artifact_path);
                let _ = std::fs::remove_file(full_path);
                freed = freed.saturating_add(entry.size_bytes);
                evicted = evicted.saturating_add(1);
            }
        }

        evicted
    }

    /// Total bytes consumed by all cached artifacts.
    pub fn total_size(&self) -> u64 {
        self.entries
            .values()
            .fold(0u64, |acc, e| acc.saturating_add(e.size_bytes))
    }

    /// Number of entries currently in the cache.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Remove entries older than `max_age_secs`.  Returns the count removed.
    pub fn prune_expired(&mut self, max_age_secs: u64) -> usize {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let stale: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| now.saturating_sub(e.created_at) > max_age_secs)
            .map(|(k, _)| k.clone())
            .collect();

        let count = stale.len();
        for hex in stale {
            if let Some(entry) = self.entries.remove(&hex) {
                let full_path = self.cache_dir.join(&entry.artifact_path);
                let _ = std::fs::remove_file(full_path);
            }
        }
        count
    }

    // -- persistence ---------------------------------------------------------

    /// Persist the in-memory index to `<cache_dir>/index.json`.
    pub fn save_index(&self) -> Result<(), CacheError> {
        let index_path = self.cache_dir.join("index.json");
        let json = serde_json::to_string_pretty(&self.entries)
            .map_err(|e| CacheError::SerializationError(e.to_string()))?;
        std::fs::write(&index_path, json)
            .map_err(|e| CacheError::IoError(format!("write index: {}", e)))?;
        Ok(())
    }

    /// Load a previously-persisted index from disk.
    pub fn load_index(cache_dir: &Path) -> Result<HashMap<String, CacheEntry>, CacheError> {
        let index_path = cache_dir.join("index.json");
        if !index_path.exists() {
            return Ok(HashMap::new());
        }
        let content = std::fs::read_to_string(&index_path)
            .map_err(|e| CacheError::IoError(format!("read index: {}", e)))?;
        let entries: HashMap<String, CacheEntry> = serde_json::from_str(&content)
            .map_err(|e| CacheError::SerializationError(e.to_string()))?;
        Ok(entries)
    }

    // -- utilities -----------------------------------------------------------

    /// Compute the SHA-256 hex digest of a source string.
    pub fn compute_source_hash(source: &str) -> String {
        let digest = Sha256::digest(source.as_bytes());
        hex_encode(&digest)
    }

    /// Derive the on-disk path fragment for a given key.
    pub fn cache_key_to_path(key: &CacheKey) -> String {
        let hex = key.to_hex();
        format!("artifacts/{}.bin", hex)
    }

    // -- accessors (used by tests) -------------------------------------------

    /// Borrow the underlying entries map.
    pub fn entries(&self) -> &HashMap<String, CacheEntry> {
        &self.entries
    }

    /// Mutably borrow the underlying entries map (for testing).
    pub fn entries_mut(&mut self) -> &mut HashMap<String, CacheEntry> {
        &mut self.entries
    }

    /// Return the configured cache directory.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Return the configured maximum size.
    pub fn max_size_bytes(&self) -> u64 {
        self.max_size_bytes
    }
}

// =============================================================================
// Helpers
// =============================================================================

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len().saturating_mul(2));
    for &b in bytes {
        s.push(nibble_to_hex(b >> 4));
        s.push(nibble_to_hex(b & 0x0f));
    }
    s
}

fn nibble_to_hex(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + (nibble - 10)) as char,
        _ => '0',
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_key(src: &str, ver: &str, target: &str, opt: OptLevel) -> CacheKey {
        CacheKey {
            source_hash: BinaryCache::compute_source_hash(src),
            compiler_version: ver.to_string(),
            target: target.to_string(),
            optimization_level: opt,
        }
    }

    #[test]
    fn test_compute_source_hash_deterministic() {
        let a = BinaryCache::compute_source_hash("hello world");
        let b = BinaryCache::compute_source_hash("hello world");
        assert_eq!(a, b);
    }

    #[test]
    fn test_compute_source_hash_different_inputs() {
        let a = BinaryCache::compute_source_hash("hello");
        let b = BinaryCache::compute_source_hash("world");
        assert_ne!(a, b);
    }

    #[test]
    fn test_cache_key_hex_stable() {
        let k1 = make_key("src", "0.4.0", "default", OptLevel::Debug);
        let k2 = make_key("src", "0.4.0", "default", OptLevel::Debug);
        assert_eq!(k1.to_hex(), k2.to_hex());
    }

    #[test]
    fn test_cache_key_hex_varies_by_opt() {
        let a = make_key("src", "0.4.0", "default", OptLevel::Debug);
        let b = make_key("src", "0.4.0", "default", OptLevel::Release);
        assert_ne!(a.to_hex(), b.to_hex());
    }

    #[test]
    fn test_opt_level_display_roundtrip() {
        for opt in [OptLevel::Debug, OptLevel::Release, OptLevel::Size] {
            let s = opt.to_string();
            let parsed: OptLevel = s.parse().unwrap();
            assert_eq!(parsed, opt);
        }
    }

    #[test]
    fn test_opt_level_parse_invalid() {
        let result = "turbo".parse::<OptLevel>();
        assert!(result.is_err());
    }
}
