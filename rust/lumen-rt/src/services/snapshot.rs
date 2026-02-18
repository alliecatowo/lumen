//! Snapshot format for durable execution.
//!
//! A [`Snapshot`] captures enough VM state to resume a suspended computation
//! after process death.  The format is versioned so that older snapshots can
//! be migrated forward when the runtime evolves.
//!
//! Values are stored as [`SerializedValue`], a fully-owned mirror of the VM's
//! `Value` enum with no `Arc` or other shared-ownership wrappers.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Snapshot ID
// ---------------------------------------------------------------------------

static NEXT_SNAPSHOT_ID: AtomicU64 = AtomicU64::new(1);

/// Unique, monotonically-increasing snapshot identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SnapshotId(pub u64);

impl SnapshotId {
    /// Generate a fresh, process-unique snapshot ID.
    pub fn next() -> Self {
        SnapshotId(NEXT_SNAPSHOT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

impl std::fmt::Display for SnapshotId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Serialized value (owned, no Arc)
// ---------------------------------------------------------------------------

/// Fully-owned mirror of the VM `Value` enum.
///
/// Non-serializable variants (closures, futures, trace-refs) are intentionally
/// excluded — they cannot survive a process boundary.  Callers that encounter
/// such values should either skip them or return [`SnapshotError::UnsupportedValue`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SerializedValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<SerializedValue>),
    Tuple(Vec<SerializedValue>),
    Set(Vec<SerializedValue>),
    Map(BTreeMap<String, SerializedValue>),
    Record {
        type_name: String,
        fields: BTreeMap<String, SerializedValue>,
    },
    Union {
        tag: String,
        payload: Box<SerializedValue>,
    },
}

// ---------------------------------------------------------------------------
// Supporting types
// ---------------------------------------------------------------------------

/// A saved instruction pointer — identifies the exact resumption point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstructionPointer {
    /// Index of the cell (function) in the LIR module.
    pub cell_index: usize,
    /// Program counter within that cell.
    pub pc: usize,
}

/// A single saved call-frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StackFrame {
    /// Which cell this frame belongs to.
    pub cell_index: usize,
    /// Saved program counter (return address within this frame's cell).
    pub pc: usize,
    /// Register file for this frame.
    pub registers: Vec<SerializedValue>,
    /// Where to return to in the *caller* frame.
    pub return_address: Option<InstructionPointer>,
}

/// A flat collection of reachable heap objects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeapSnapshot {
    pub objects: Vec<HeapObject>,
}

/// One heap-allocated object captured during snapshotting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeapObject {
    /// Logical object id (assigned during the snapshot walk).
    pub id: u64,
    /// Serialized payload bytes (bincode of [`SerializedValue`]).
    pub data: Vec<u8>,
    /// Human-readable type tag for diagnostics.
    pub type_tag: String,
}

/// Process-level metadata attached to every snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub process_id: u64,
    pub process_name: String,
    pub source_file: String,
    /// Optional user-supplied label (e.g. `"before-tool-call"`).
    pub checkpoint_label: Option<String>,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during snapshot creation / restoration.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("serialization failed: {0}")]
    Serialize(String),
    #[error("deserialization failed: {0}")]
    Deserialize(String),
    #[error("unsupported value type for snapshotting: {0}")]
    UnsupportedValue(String),
    #[error("version mismatch: snapshot v{found}, runtime v{expected}")]
    VersionMismatch { expected: u32, found: u32 },
}

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

/// Current snapshot format version.
pub const SNAPSHOT_VERSION: u32 = 1;

/// A snapshot captures enough state to resume a suspended computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Format version for migration compatibility.
    pub version: u32,
    /// Unique identifier for this snapshot.
    pub id: SnapshotId,
    /// Unix-epoch timestamp (seconds) when this snapshot was taken.
    pub timestamp: u64,
    /// Serialized stack frames.
    pub frames: Vec<StackFrame>,
    /// Serialized heap objects reachable from stack.
    pub heap: HeapSnapshot,
    /// Current instruction pointer (cell index + pc).
    pub ip: InstructionPointer,
    /// Process metadata.
    pub metadata: SnapshotMetadata,
}

impl Snapshot {
    /// Create a new snapshot with the current version and a fresh ID.
    pub fn new(
        frames: Vec<StackFrame>,
        heap: HeapSnapshot,
        ip: InstructionPointer,
        metadata: SnapshotMetadata,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Snapshot {
            version: SNAPSHOT_VERSION,
            id: SnapshotId::next(),
            timestamp: now,
            frames,
            heap,
            ip,
            metadata,
        }
    }

    /// Serialize this snapshot to a byte vector (bincode).
    pub fn serialize(&self) -> Result<Vec<u8>, SnapshotError> {
        bincode::serialize(self).map_err(|e| SnapshotError::Serialize(e.to_string()))
    }

    /// Deserialize a snapshot from bytes, checking the version tag.
    pub fn deserialize(bytes: &[u8]) -> Result<Self, SnapshotError> {
        let snap: Snapshot =
            bincode::deserialize(bytes).map_err(|e| SnapshotError::Deserialize(e.to_string()))?;
        if snap.version != SNAPSHOT_VERSION {
            return Err(SnapshotError::VersionMismatch {
                expected: SNAPSHOT_VERSION,
                found: snap.version,
            });
        }
        Ok(snap)
    }

    /// Approximate in-memory size of the serialized form (bytes).
    pub fn size_bytes(&self) -> usize {
        // We actually serialize to get an accurate byte count.  For very hot
        // paths the caller could cache this, but correctness > micro-opt here.
        self.serialize().map(|v| v.len()).unwrap_or(0)
    }

    /// Serialize and compress this snapshot to a byte vector (bincode + gzip).
    pub fn serialize_compressed(&self) -> Result<Vec<u8>, SnapshotError> {
        let raw = self.serialize()?;
        compress(&raw).map_err(|e| SnapshotError::Serialize(e.to_string()))
    }

    /// Decompress and deserialize a snapshot from compressed bytes.
    pub fn deserialize_compressed(bytes: &[u8]) -> Result<Self, SnapshotError> {
        let raw = decompress(bytes).map_err(|e| SnapshotError::Deserialize(e.to_string()))?;
        Self::deserialize(&raw)
    }
}

// ---------------------------------------------------------------------------
// Compression / decompression (gzip via flate2)
// ---------------------------------------------------------------------------

/// Compress data using gzip (flate2, compression level 6).
pub fn compress(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data)?;
    encoder.finish()
}

/// Decompress gzip-compressed data.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut decoder = flate2::read::GzDecoder::new(data);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    Ok(out)
}

// ---------------------------------------------------------------------------
// Snapshot pruner
// ---------------------------------------------------------------------------

/// Prunes snapshot collections to keep only the N most recent entries.
///
/// Works with any ordered collection of [`Snapshot`] objects (in-memory) or
/// with a [`CheckpointStore`](crate::services::checkpoint::CheckpointStore) (on-disk).
pub struct SnapshotPruner {
    /// Maximum number of snapshots to retain.
    max_retained: usize,
}

impl SnapshotPruner {
    /// Create a pruner that keeps at most `max_retained` snapshots.
    ///
    /// # Panics
    /// Panics if `max_retained` is zero.
    pub fn new(max_retained: usize) -> Self {
        assert!(max_retained > 0, "max_retained must be at least 1");
        SnapshotPruner { max_retained }
    }

    /// Maximum number of snapshots this pruner retains.
    pub fn max_retained(&self) -> usize {
        self.max_retained
    }

    /// Prune an in-memory list of snapshots, keeping only the
    /// `max_retained` most recent (by [`SnapshotId`]).
    ///
    /// Returns the IDs of removed snapshots. The input `Vec` is modified
    /// in-place with only the retained snapshots remaining.
    pub fn prune_in_memory(&self, snapshots: &mut Vec<Snapshot>) -> Vec<SnapshotId> {
        if snapshots.len() <= self.max_retained {
            return Vec::new();
        }

        // Sort by ID ascending
        snapshots.sort_by_key(|s| s.id);

        let to_remove = snapshots.len() - self.max_retained;
        let removed: Vec<SnapshotId> = snapshots[..to_remove].iter().map(|s| s.id).collect();
        snapshots.drain(..to_remove);
        removed
    }

    /// Prune snapshots from a [`CheckpointStore`](crate::services::checkpoint::CheckpointStore),
    /// keeping only the `max_retained` most recent snapshot IDs.
    ///
    /// Returns the number of snapshots deleted.
    pub fn prune_store(
        &self,
        store: &dyn crate::services::checkpoint::CheckpointStore,
    ) -> Result<usize, crate::services::checkpoint::CheckpointError> {
        let mut ids = store.list()?;
        if ids.len() <= self.max_retained {
            return Ok(0);
        }
        ids.sort();
        let to_remove = ids.len() - self.max_retained;
        let mut deleted = 0;
        for id in ids.into_iter().take(to_remove) {
            store.delete(id)?;
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

    fn sample_metadata() -> SnapshotMetadata {
        SnapshotMetadata {
            process_id: 42,
            process_name: "test-proc".into(),
            source_file: "main.lm".into(),
            checkpoint_label: Some("before-tool".into()),
        }
    }

    fn sample_snapshot() -> Snapshot {
        let frame = StackFrame {
            cell_index: 0,
            pc: 5,
            registers: vec![
                SerializedValue::Int(42),
                SerializedValue::String("hello".into()),
                SerializedValue::Null,
            ],
            return_address: None,
        };

        let heap = HeapSnapshot {
            objects: vec![HeapObject {
                id: 1,
                data: vec![0xDE, 0xAD],
                type_tag: "List".into(),
            }],
        };

        let ip = InstructionPointer {
            cell_index: 0,
            pc: 5,
        };

        Snapshot::new(vec![frame], heap, ip, sample_metadata())
    }

    #[test]
    fn snapshot_round_trip() {
        let snap = sample_snapshot();
        let bytes = snap.serialize().expect("serialize");
        let restored = Snapshot::deserialize(&bytes).expect("deserialize");
        assert_eq!(snap.id, restored.id);
        assert_eq!(snap.version, restored.version);
        assert_eq!(snap.frames.len(), restored.frames.len());
        assert_eq!(snap.frames[0].registers, restored.frames[0].registers);
        assert_eq!(snap.heap, restored.heap);
        assert_eq!(snap.ip, restored.ip);
        assert_eq!(snap.metadata, restored.metadata);
    }

    #[test]
    fn snapshot_version_check() {
        let mut snap = sample_snapshot();
        snap.version = 999;
        let bytes = bincode::serialize(&snap).unwrap();
        let err = Snapshot::deserialize(&bytes).unwrap_err();
        match err {
            SnapshotError::VersionMismatch {
                expected: 1,
                found: 999,
            } => {}
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn snapshot_size_bytes() {
        let snap = sample_snapshot();
        let sz = snap.size_bytes();
        assert!(sz > 0, "size_bytes must be positive");
        // Should equal actual serialized length.
        let bytes = snap.serialize().unwrap();
        assert_eq!(sz, bytes.len());
    }

    #[test]
    fn snapshot_id_monotonic() {
        let a = SnapshotId::next();
        let b = SnapshotId::next();
        let c = SnapshotId::next();
        assert!(a < b);
        assert!(b < c);
    }

    #[test]
    fn serialized_value_nested_round_trip() {
        let val = SerializedValue::Record {
            type_name: "Point".into(),
            fields: {
                let mut m = BTreeMap::new();
                m.insert("x".into(), SerializedValue::Float(1.5));
                m.insert(
                    "y".into(),
                    SerializedValue::List(vec![
                        SerializedValue::Int(10),
                        SerializedValue::Null,
                        SerializedValue::Map({
                            let mut inner = BTreeMap::new();
                            inner.insert("key".into(), SerializedValue::Bool(true));
                            inner
                        }),
                    ]),
                );
                m
            },
        };

        let bytes = bincode::serialize(&val).unwrap();
        let restored: SerializedValue = bincode::deserialize(&bytes).unwrap();
        assert_eq!(val, restored);
    }

    #[test]
    fn serialized_value_union_round_trip() {
        let val = SerializedValue::Union {
            tag: "Some".into(),
            payload: Box::new(SerializedValue::String("data".into())),
        };
        let bytes = bincode::serialize(&val).unwrap();
        let restored: SerializedValue = bincode::deserialize(&bytes).unwrap();
        assert_eq!(val, restored);
    }

    #[test]
    fn serialized_value_set_round_trip() {
        let val = SerializedValue::Set(vec![
            SerializedValue::Int(3),
            SerializedValue::Int(1),
            SerializedValue::Int(2),
        ]);
        let bytes = bincode::serialize(&val).unwrap();
        let restored: SerializedValue = bincode::deserialize(&bytes).unwrap();
        assert_eq!(val, restored);
    }

    #[test]
    fn serialized_value_bytes_round_trip() {
        let val = SerializedValue::Bytes(vec![0x00, 0xFF, 0x42]);
        let bytes = bincode::serialize(&val).unwrap();
        let restored: SerializedValue = bincode::deserialize(&bytes).unwrap();
        assert_eq!(val, restored);
    }

    #[test]
    fn snapshot_empty_frames() {
        let snap = Snapshot::new(
            vec![],
            HeapSnapshot { objects: vec![] },
            InstructionPointer {
                cell_index: 0,
                pc: 0,
            },
            sample_metadata(),
        );
        let bytes = snap.serialize().unwrap();
        let restored = Snapshot::deserialize(&bytes).unwrap();
        assert!(restored.frames.is_empty());
        assert!(restored.heap.objects.is_empty());
    }

    // =====================================================================
    // Compression tests
    // =====================================================================

    #[test]
    fn compress_decompress_round_trip() {
        let data = b"Hello, Lumen! This is test data for compression.";
        let compressed = compress(data).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn compress_empty_data() {
        let data = b"";
        let compressed = compress(data).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, data.to_vec());
    }

    #[test]
    fn compress_reduces_size_for_repetitive_data() {
        // Highly repetitive data should compress well
        let data = vec![0xABu8; 10_000];
        let compressed = compress(&data).unwrap();
        assert!(
            compressed.len() < data.len(),
            "compressed size ({}) should be smaller than original ({})",
            compressed.len(),
            data.len()
        );
    }

    #[test]
    fn snapshot_compressed_round_trip() {
        let snap = sample_snapshot();
        let compressed = snap.serialize_compressed().unwrap();
        let restored = Snapshot::deserialize_compressed(&compressed).unwrap();
        assert_eq!(snap.id, restored.id);
        assert_eq!(snap.version, restored.version);
        assert_eq!(snap.frames.len(), restored.frames.len());
        assert_eq!(snap.frames[0].registers, restored.frames[0].registers);
        assert_eq!(snap.heap, restored.heap);
        assert_eq!(snap.ip, restored.ip);
        assert_eq!(snap.metadata, restored.metadata);
    }

    #[test]
    fn compressed_snapshot_is_smaller() {
        // Create a snapshot with enough data that compression helps
        let mut frames = Vec::new();
        for i in 0..50 {
            frames.push(StackFrame {
                cell_index: i,
                pc: i * 10,
                registers: vec![
                    SerializedValue::Int(i as i64),
                    SerializedValue::String(format!("register_{}", i)),
                    SerializedValue::Null,
                ],
                return_address: None,
            });
        }
        let snap = Snapshot::new(
            frames,
            HeapSnapshot { objects: vec![] },
            InstructionPointer {
                cell_index: 0,
                pc: 0,
            },
            sample_metadata(),
        );

        let raw = snap.serialize().unwrap();
        let compressed = snap.serialize_compressed().unwrap();
        assert!(
            compressed.len() < raw.len(),
            "compressed ({}) should be smaller than raw ({})",
            compressed.len(),
            raw.len()
        );
    }

    #[test]
    fn decompress_invalid_data_fails() {
        let bad_data = vec![0x00, 0x01, 0x02, 0x03];
        assert!(decompress(&bad_data).is_err());
    }

    // =====================================================================
    // SnapshotPruner tests
    // =====================================================================

    #[test]
    fn pruner_keeps_max_retained() {
        let pruner = SnapshotPruner::new(2);
        assert_eq!(pruner.max_retained(), 2);

        let mut snapshots: Vec<Snapshot> = (0..5).map(|_| sample_snapshot()).collect();
        let last_two_ids: Vec<SnapshotId> = {
            let mut sorted = snapshots.clone();
            sorted.sort_by_key(|s| s.id);
            sorted.iter().rev().take(2).map(|s| s.id).collect()
        };

        let removed = pruner.prune_in_memory(&mut snapshots);
        assert_eq!(removed.len(), 3);
        assert_eq!(snapshots.len(), 2);

        // The two remaining should be the ones with the highest IDs
        for snap in &snapshots {
            assert!(last_two_ids.contains(&snap.id));
        }
    }

    #[test]
    fn pruner_noop_when_under_limit() {
        let pruner = SnapshotPruner::new(10);
        let mut snapshots: Vec<Snapshot> = (0..3).map(|_| sample_snapshot()).collect();
        let removed = pruner.prune_in_memory(&mut snapshots);
        assert!(removed.is_empty());
        assert_eq!(snapshots.len(), 3);
    }

    #[test]
    fn pruner_exact_limit() {
        let pruner = SnapshotPruner::new(3);
        let mut snapshots: Vec<Snapshot> = (0..3).map(|_| sample_snapshot()).collect();
        let removed = pruner.prune_in_memory(&mut snapshots);
        assert!(removed.is_empty());
        assert_eq!(snapshots.len(), 3);
    }

    #[test]
    #[should_panic(expected = "max_retained must be at least 1")]
    fn pruner_zero_panics() {
        SnapshotPruner::new(0);
    }

    #[test]
    fn pruner_single_snapshot() {
        let pruner = SnapshotPruner::new(1);
        let mut snapshots: Vec<Snapshot> = (0..5).map(|_| sample_snapshot()).collect();
        let last_id = {
            let mut sorted = snapshots.clone();
            sorted.sort_by_key(|s| s.id);
            sorted.last().unwrap().id
        };
        let removed = pruner.prune_in_memory(&mut snapshots);
        assert_eq!(removed.len(), 4);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].id, last_id);
    }

    #[test]
    fn pruner_store_integration() {
        use crate::services::checkpoint::{CheckpointStore, FileCheckpointStore};

        let dir = std::env::temp_dir().join(format!("lumen-pruner-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = FileCheckpointStore::new(&dir).unwrap();

        // Save 5 snapshots
        let mut ids = Vec::new();
        for _ in 0..5 {
            let snap = sample_snapshot();
            let bytes = snap.serialize().unwrap();
            store.save(snap.id, &bytes).unwrap();
            ids.push(snap.id);
        }
        ids.sort();

        // Prune to keep 2
        let pruner = SnapshotPruner::new(2);
        let deleted = pruner.prune_store(&store).unwrap();
        assert_eq!(deleted, 3);

        let remaining = store.list().unwrap();
        assert_eq!(remaining.len(), 2);

        // Remaining should be the 2 highest IDs
        for id in &remaining {
            assert!(ids[3..].contains(id));
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
