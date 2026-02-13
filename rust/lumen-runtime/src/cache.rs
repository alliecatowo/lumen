//! Content-addressed cache for tool invocation results.

use crate::trace::hasher::canonical_hash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
        let key = crate::trace::hasher::cache_key(tool_id, version, policy_hash, &args_hash);
        self.get(&key)
    }
}
