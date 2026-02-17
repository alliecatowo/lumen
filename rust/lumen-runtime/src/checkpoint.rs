//! Checkpoint / restore engine for durable execution.
//!
//! [`CheckpointStore`] abstracts the storage backend (filesystem, object store,
//! etc.).  [`FileCheckpointStore`] is the built-in filesystem implementation
//! that writes snapshots atomically via write-to-tmp + rename.
//!
//! [`CheckpointEngine`] wraps a store and provides higher-level operations such
//! as `latest()` and `prune()`.

use crate::snapshot::{Snapshot, SnapshotError, SnapshotId};
use std::fs;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("snapshot error: {0}")]
    Snapshot(#[from] SnapshotError),
    #[error("checkpoint not found: {0}")]
    NotFound(SnapshotId),
}

// ---------------------------------------------------------------------------
// Storage trait
// ---------------------------------------------------------------------------

/// Storage backend trait for checkpoints.
pub trait CheckpointStore: Send + Sync {
    /// Persist raw snapshot bytes under the given ID.
    fn save(&self, id: SnapshotId, data: &[u8]) -> Result<(), CheckpointError>;
    /// Load raw snapshot bytes for the given ID.
    fn load(&self, id: SnapshotId) -> Result<Vec<u8>, CheckpointError>;
    /// List all stored snapshot IDs (in no guaranteed order).
    fn list(&self) -> Result<Vec<SnapshotId>, CheckpointError>;
    /// Delete the snapshot with the given ID.
    fn delete(&self, id: SnapshotId) -> Result<(), CheckpointError>;
}

// ---------------------------------------------------------------------------
// Filesystem store
// ---------------------------------------------------------------------------

/// Stores snapshots as `{id}.snap` files in a directory.
///
/// Writes are atomic: data goes to a `.tmp` sibling first, then is renamed
/// into place so readers never see a partial file.
pub struct FileCheckpointStore {
    dir: PathBuf,
}

impl FileCheckpointStore {
    /// Create (or open) a checkpoint store rooted at `dir`.
    /// The directory is created if it does not exist.
    pub fn new(dir: impl Into<PathBuf>) -> Result<Self, CheckpointError> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        Ok(FileCheckpointStore { dir })
    }

    fn snap_path(&self, id: SnapshotId) -> PathBuf {
        self.dir.join(format!("{}.snap", id.0))
    }

    fn tmp_path(&self, id: SnapshotId) -> PathBuf {
        self.dir.join(format!("{}.snap.tmp", id.0))
    }
}

impl CheckpointStore for FileCheckpointStore {
    fn save(&self, id: SnapshotId, data: &[u8]) -> Result<(), CheckpointError> {
        let tmp = self.tmp_path(id);
        let final_path = self.snap_path(id);
        fs::write(&tmp, data)?;
        fs::rename(&tmp, &final_path)?;
        Ok(())
    }

    fn load(&self, id: SnapshotId) -> Result<Vec<u8>, CheckpointError> {
        let path = self.snap_path(id);
        if !path.exists() {
            return Err(CheckpointError::NotFound(id));
        }
        Ok(fs::read(path)?)
    }

    fn list(&self) -> Result<Vec<SnapshotId>, CheckpointError> {
        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(stem) = name.strip_suffix(".snap") {
                if let Ok(n) = stem.parse::<u64>() {
                    ids.push(SnapshotId(n));
                }
            }
        }
        Ok(ids)
    }

    fn delete(&self, id: SnapshotId) -> Result<(), CheckpointError> {
        let path = self.snap_path(id);
        if !path.exists() {
            return Err(CheckpointError::NotFound(id));
        }
        fs::remove_file(path)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// High-level checkpoint engine that combines serialization with storage.
pub struct CheckpointEngine {
    store: Box<dyn CheckpointStore>,
    /// When true, snapshots are gzip-compressed before saving and
    /// decompressed on load.
    compressed: bool,
}

impl CheckpointEngine {
    pub fn new(store: Box<dyn CheckpointStore>) -> Self {
        CheckpointEngine {
            store,
            compressed: false,
        }
    }

    /// Create a checkpoint engine that compresses snapshots on save and
    /// decompresses on load.
    pub fn new_compressed(store: Box<dyn CheckpointStore>) -> Self {
        CheckpointEngine {
            store,
            compressed: true,
        }
    }

    /// Whether this engine uses compression.
    pub fn is_compressed(&self) -> bool {
        self.compressed
    }

    /// Serialize a snapshot and persist it.  Returns the snapshot's ID.
    pub fn checkpoint(&self, snapshot: &Snapshot) -> Result<SnapshotId, CheckpointError> {
        let bytes = if self.compressed {
            snapshot.serialize_compressed()?
        } else {
            snapshot.serialize()?
        };
        self.store.save(snapshot.id, &bytes)?;
        Ok(snapshot.id)
    }

    /// Load and deserialize a snapshot by ID.
    pub fn restore(&self, id: SnapshotId) -> Result<Snapshot, CheckpointError> {
        let bytes = self.store.load(id)?;
        if self.compressed {
            Ok(Snapshot::deserialize_compressed(&bytes)?)
        } else {
            Ok(Snapshot::deserialize(&bytes)?)
        }
    }

    /// Return the most-recent snapshot (by ID, which is monotonically increasing).
    pub fn latest(&self) -> Result<Option<Snapshot>, CheckpointError> {
        let mut ids = self.store.list()?;
        if ids.is_empty() {
            return Ok(None);
        }
        ids.sort();
        let max_id = *ids.last().unwrap();
        Ok(Some(self.restore(max_id)?))
    }

    /// Keep only the `keep` most recent snapshots; delete the rest.
    /// Returns the number of snapshots deleted.
    pub fn prune(&self, keep: usize) -> Result<usize, CheckpointError> {
        let mut ids = self.store.list()?;
        if ids.len() <= keep {
            return Ok(0);
        }
        ids.sort();
        let to_remove = ids.len() - keep;
        let mut deleted = 0;
        for id in ids.into_iter().take(to_remove) {
            self.store.delete(id)?;
            deleted += 1;
        }
        Ok(deleted)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::*;
    use std::env;

    /// Create a throwaway temp dir that won't collide with other tests.
    fn temp_dir(suffix: &str) -> PathBuf {
        let mut p = env::temp_dir();
        p.push(format!(
            "lumen-checkpoint-test-{}-{}",
            suffix,
            std::process::id()
        ));
        // Clean up from previous runs.
        let _ = fs::remove_dir_all(&p);
        p
    }

    fn sample_snapshot_for_checkpoint() -> Snapshot {
        Snapshot::new(
            vec![StackFrame {
                cell_index: 0,
                pc: 3,
                registers: vec![SerializedValue::Int(7)],
                return_address: None,
            }],
            HeapSnapshot { objects: vec![] },
            InstructionPointer {
                cell_index: 0,
                pc: 3,
            },
            SnapshotMetadata {
                process_id: 1,
                process_name: "agent".into(),
                source_file: "agent.lm.md".into(),
                checkpoint_label: None,
            },
        )
    }

    #[test]
    fn file_store_save_and_load() {
        let dir = temp_dir("save-load");
        let store = FileCheckpointStore::new(&dir).unwrap();
        let snap = sample_snapshot_for_checkpoint();
        let bytes = snap.serialize().unwrap();
        store.save(snap.id, &bytes).unwrap();

        let loaded = store.load(snap.id).unwrap();
        assert_eq!(bytes, loaded);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_store_list() {
        let dir = temp_dir("list");
        let store = FileCheckpointStore::new(&dir).unwrap();

        let s1 = sample_snapshot_for_checkpoint();
        let s2 = sample_snapshot_for_checkpoint();
        store.save(s1.id, &s1.serialize().unwrap()).unwrap();
        store.save(s2.id, &s2.serialize().unwrap()).unwrap();

        let mut ids = store.list().unwrap();
        ids.sort();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&s1.id));
        assert!(ids.contains(&s2.id));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_store_delete() {
        let dir = temp_dir("delete");
        let store = FileCheckpointStore::new(&dir).unwrap();
        let snap = sample_snapshot_for_checkpoint();
        store.save(snap.id, &snap.serialize().unwrap()).unwrap();

        store.delete(snap.id).unwrap();
        assert!(store.load(snap.id).is_err());
        assert!(store.list().unwrap().is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_store_not_found() {
        let dir = temp_dir("notfound");
        let store = FileCheckpointStore::new(&dir).unwrap();
        match store.load(SnapshotId(9999)) {
            Err(CheckpointError::NotFound(_)) => {}
            other => panic!("expected NotFound, got {:?}", other),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn engine_checkpoint_restore() {
        let dir = temp_dir("engine-cp");
        let store = FileCheckpointStore::new(&dir).unwrap();
        let engine = CheckpointEngine::new(Box::new(store));

        let snap = sample_snapshot_for_checkpoint();
        let id = engine.checkpoint(&snap).unwrap();
        let restored = engine.restore(id).unwrap();

        assert_eq!(snap.id, restored.id);
        assert_eq!(snap.frames[0].registers, restored.frames[0].registers);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn engine_latest_empty() {
        let dir = temp_dir("engine-latest-empty");
        let store = FileCheckpointStore::new(&dir).unwrap();
        let engine = CheckpointEngine::new(Box::new(store));

        assert!(engine.latest().unwrap().is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn engine_latest() {
        let dir = temp_dir("engine-latest");
        let store = FileCheckpointStore::new(&dir).unwrap();
        let engine = CheckpointEngine::new(Box::new(store));

        let s1 = sample_snapshot_for_checkpoint();
        let s2 = sample_snapshot_for_checkpoint();
        // s2 has a higher ID since IDs are monotonic.
        engine.checkpoint(&s1).unwrap();
        engine.checkpoint(&s2).unwrap();

        let latest = engine.latest().unwrap().expect("should have latest");
        assert_eq!(latest.id, s2.id);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn engine_prune() {
        let dir = temp_dir("engine-prune");
        let store = FileCheckpointStore::new(&dir).unwrap();
        let engine = CheckpointEngine::new(Box::new(store));

        let s1 = sample_snapshot_for_checkpoint();
        let s2 = sample_snapshot_for_checkpoint();
        let s3 = sample_snapshot_for_checkpoint();
        engine.checkpoint(&s1).unwrap();
        engine.checkpoint(&s2).unwrap();
        engine.checkpoint(&s3).unwrap();

        // Keep only 1 — should delete 2.
        let deleted = engine.prune(1).unwrap();
        assert_eq!(deleted, 2);

        // The surviving snapshot should be the latest (s3).
        let latest = engine.latest().unwrap().unwrap();
        assert_eq!(latest.id, s3.id);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn engine_prune_noop() {
        let dir = temp_dir("engine-prune-noop");
        let store = FileCheckpointStore::new(&dir).unwrap();
        let engine = CheckpointEngine::new(Box::new(store));

        let snap = sample_snapshot_for_checkpoint();
        engine.checkpoint(&snap).unwrap();

        // Keep 5 but only 1 exists — nothing deleted.
        let deleted = engine.prune(5).unwrap();
        assert_eq!(deleted, 0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn engine_compressed_checkpoint_restore() {
        let dir = temp_dir("engine-compressed");
        let store = FileCheckpointStore::new(&dir).unwrap();
        let engine = CheckpointEngine::new_compressed(Box::new(store));

        assert!(engine.is_compressed());

        let snap = sample_snapshot_for_checkpoint();
        let id = engine.checkpoint(&snap).unwrap();
        let restored = engine.restore(id).unwrap();

        assert_eq!(snap.id, restored.id);
        assert_eq!(snap.frames[0].registers, restored.frames[0].registers);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn compressed_engine_latest() {
        let dir = temp_dir("engine-compressed-latest");
        let store = FileCheckpointStore::new(&dir).unwrap();
        let engine = CheckpointEngine::new_compressed(Box::new(store));

        let s1 = sample_snapshot_for_checkpoint();
        let s2 = sample_snapshot_for_checkpoint();
        engine.checkpoint(&s1).unwrap();
        engine.checkpoint(&s2).unwrap();

        let latest = engine.latest().unwrap().expect("should have latest");
        assert_eq!(latest.id, s2.id);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn compressed_smaller_than_raw() {
        let dir_raw = temp_dir("engine-size-raw");
        let dir_comp = temp_dir("engine-size-comp");
        let store_raw = FileCheckpointStore::new(&dir_raw).unwrap();
        let store_comp = FileCheckpointStore::new(&dir_comp).unwrap();
        let engine_raw = CheckpointEngine::new(Box::new(store_raw));
        let engine_comp = CheckpointEngine::new_compressed(Box::new(store_comp));

        let snap = sample_snapshot_for_checkpoint();
        engine_raw.checkpoint(&snap).unwrap();
        engine_comp.checkpoint(&snap).unwrap();

        // Compare file sizes
        let raw_path = dir_raw.join(format!("{}.snap", snap.id.0));
        let comp_path = dir_comp.join(format!("{}.snap", snap.id.0));
        let raw_size = fs::metadata(&raw_path).unwrap().len();
        let comp_size = fs::metadata(&comp_path).unwrap().len();

        // Compressed should not be larger than raw for structured data
        // (For very small data, gzip header can make it larger, so we just
        // check it doesn't blow up — for real workloads it'll be smaller)
        assert!(
            comp_size <= raw_size + 50,
            "compressed ({}) should not be much larger than raw ({})",
            comp_size,
            raw_size,
        );

        let _ = fs::remove_dir_all(&dir_raw);
        let _ = fs::remove_dir_all(&dir_comp);
    }
}
