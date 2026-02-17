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
}
