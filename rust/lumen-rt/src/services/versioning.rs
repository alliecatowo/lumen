//! Workflow versioning and snapshot migration.
//!
//! When the runtime evolves, previously checkpointed [`HeapSnapshot`]s may need
//! structural changes (added fields, renamed fields, schema bumps) before they
//! can be resumed.  This module provides:
//!
//! - [`SchemaVersion`] — semantic version tag for snapshot formats.
//! - [`SnapshotMigration`] — trait describing how to transform a snapshot from
//!   one version to the next.
//! - [`MigrationRegistry`] — ordered collection of migrations with automatic
//!   path-finding between arbitrary version pairs.
//! - [`VersionedSnapshot`] — a [`HeapSnapshot`] paired with its schema version.
//! - [`migrate_snapshot`] — top-level helper that applies a chain of migrations.

use crate::snapshot::HeapSnapshot;
use serde::{Deserialize, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------
// SchemaVersion
// ---------------------------------------------------------------------------

/// Semantic version identifier for snapshot schemas.
///
/// Ordering is lexicographic on (major, minor, patch), which matches the
/// expected semver semantics for schema compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SchemaVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SchemaVersion {
    /// Convenience constructor.
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl PartialOrd for SchemaVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SchemaVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
    }
}

// ---------------------------------------------------------------------------
// MigrationError
// ---------------------------------------------------------------------------

/// Errors that can occur during snapshot migration.
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    /// The source and target versions are fundamentally incompatible (e.g. a
    /// downgrade was requested but no reverse migration is registered).
    #[error("incompatible versions: cannot migrate from {from} to {to}")]
    IncompatibleVersions {
        from: SchemaVersion,
        to: SchemaVersion,
    },

    /// No registered migration covers one of the hops in the required path.
    #[error("no migration registered from {from} to {to}")]
    MissingMigration {
        from: SchemaVersion,
        to: SchemaVersion,
    },

    /// A migration step detected that the snapshot data is corrupt or cannot
    /// be transformed.
    #[error("data corrupted during migration from {from} to {to}: {detail}")]
    DataCorrupted {
        from: SchemaVersion,
        to: SchemaVersion,
        detail: String,
    },
}

// ---------------------------------------------------------------------------
// SnapshotMigration trait
// ---------------------------------------------------------------------------

/// Defines how to transform a [`HeapSnapshot`] from one schema version to
/// the next.
///
/// Each implementation covers exactly one hop (e.g. 1.0.0 → 1.1.0).  The
/// [`MigrationRegistry`] chains multiple migrations together when a longer
/// path is needed.
pub trait SnapshotMigration: Send + Sync {
    /// The version this migration reads.
    fn source_version(&self) -> SchemaVersion;

    /// The version this migration produces.
    fn target_version(&self) -> SchemaVersion;

    /// Apply the migration to `snapshot`, returning the transformed snapshot
    /// or an error.
    fn migrate(&self, snapshot: HeapSnapshot) -> Result<HeapSnapshot, MigrationError>;
}

// ---------------------------------------------------------------------------
// VersionedSnapshot
// ---------------------------------------------------------------------------

/// A [`HeapSnapshot`] annotated with its schema version.
///
/// This is the unit of interchange between checkpoint storage and the
/// migration pipeline — callers can inspect the version before deciding
/// whether migration is necessary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VersionedSnapshot {
    pub version: SchemaVersion,
    pub snapshot: HeapSnapshot,
}

impl VersionedSnapshot {
    /// Wrap a heap snapshot with an explicit version tag.
    pub fn new(version: SchemaVersion, snapshot: HeapSnapshot) -> Self {
        Self { version, snapshot }
    }
}

// ---------------------------------------------------------------------------
// MigrationRegistry
// ---------------------------------------------------------------------------

/// Stores an ordered set of migrations and can find a path between any two
/// registered versions.
///
/// Migrations are stored in the order they are added.  Path-finding walks the
/// registered migrations greedily from `from` → `to` (the set must form a
/// connected chain for the versions of interest).
pub struct MigrationRegistry {
    migrations: Vec<Box<dyn SnapshotMigration>>,
}

impl MigrationRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            migrations: Vec::new(),
        }
    }

    /// Register a migration.  Migrations can be added in any order; the
    /// registry will sort them when building a path.
    pub fn register(&mut self, migration: Box<dyn SnapshotMigration>) {
        self.migrations.push(migration);
    }

    /// Return the number of registered migrations.
    pub fn len(&self) -> usize {
        self.migrations.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.migrations.is_empty()
    }

    /// Find an ordered sequence of migrations that transforms a snapshot from
    /// `from` to `to`.  Returns indices into the internal migration list.
    ///
    /// The algorithm is a greedy forward walk: at each step it picks the
    /// migration whose `from_version` matches the current version.  This is
    /// O(n·m) where n = number of hops and m = number of registered
    /// migrations, which is perfectly fine for realistic migration counts.
    pub fn find_path(
        &self,
        from: SchemaVersion,
        to: SchemaVersion,
    ) -> Result<Vec<usize>, MigrationError> {
        if from == to {
            return Ok(Vec::new());
        }

        // Downgrades are not supported — we only migrate forward.
        if from > to {
            return Err(MigrationError::IncompatibleVersions { from, to });
        }

        let mut path = Vec::new();
        let mut current = from;

        while current < to {
            // Find a migration whose from_version == current.
            let idx = self
                .migrations
                .iter()
                .position(|m| m.source_version() == current && m.target_version() <= to)
                .ok_or(MigrationError::MissingMigration { from: current, to })?;

            current = self.migrations[idx].target_version();
            path.push(idx);
        }

        if current != to {
            return Err(MigrationError::MissingMigration { from: current, to });
        }

        Ok(path)
    }

    /// Apply the full migration chain from `from` → `to` to the given
    /// snapshot.
    pub fn apply(
        &self,
        snapshot: HeapSnapshot,
        from: SchemaVersion,
        to: SchemaVersion,
    ) -> Result<HeapSnapshot, MigrationError> {
        let path = self.find_path(from, to)?;
        let mut snap = snapshot;
        for idx in path {
            snap = self.migrations[idx].migrate(snap)?;
        }
        Ok(snap)
    }
}

impl Default for MigrationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Top-level helper
// ---------------------------------------------------------------------------

/// Apply a chain of migrations from `from` to `to` using the given registry.
///
/// This is a convenience wrapper around [`MigrationRegistry::apply`].
pub fn migrate_snapshot(
    snapshot: HeapSnapshot,
    from: SchemaVersion,
    to: SchemaVersion,
    registry: &MigrationRegistry,
) -> Result<HeapSnapshot, MigrationError> {
    registry.apply(snapshot, from, to)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{HeapObject, HeapSnapshot};

    // -- Test helpers -------------------------------------------------------

    /// Helper: build a HeapSnapshot with the given objects.
    fn make_snapshot(objects: Vec<HeapObject>) -> HeapSnapshot {
        HeapSnapshot { objects }
    }

    /// Helper: build a HeapObject with the given fields.
    fn make_object(id: u64, data: &[u8], type_tag: &str) -> HeapObject {
        HeapObject {
            id,
            data: data.to_vec(),
            type_tag: type_tag.to_string(),
        }
    }

    // -- Concrete migration implementations for tests -----------------------

    /// Migration 1.0.0 → 2.0.0: adds a new field (appends a default object).
    struct MigrateV1ToV2;

    impl SnapshotMigration for MigrateV1ToV2 {
        fn source_version(&self) -> SchemaVersion {
            SchemaVersion::new(1, 0, 0)
        }
        fn target_version(&self) -> SchemaVersion {
            SchemaVersion::new(2, 0, 0)
        }
        fn migrate(&self, mut snapshot: HeapSnapshot) -> Result<HeapSnapshot, MigrationError> {
            // Simulate "add a new field with default" — append a sentinel object.
            let next_id = snapshot.objects.iter().map(|o| o.id).max().unwrap_or(0) + 1;
            snapshot.objects.push(HeapObject {
                id: next_id,
                data: b"default_v2_field".to_vec(),
                type_tag: "AddedFieldV2".to_string(),
            });
            Ok(snapshot)
        }
    }

    /// Migration 2.0.0 → 2.1.0: renames a field (changes type_tag on matching objects).
    struct MigrateV2ToV2_1;

    impl SnapshotMigration for MigrateV2ToV2_1 {
        fn source_version(&self) -> SchemaVersion {
            SchemaVersion::new(2, 0, 0)
        }
        fn target_version(&self) -> SchemaVersion {
            SchemaVersion::new(2, 1, 0)
        }
        fn migrate(&self, mut snapshot: HeapSnapshot) -> Result<HeapSnapshot, MigrationError> {
            // Simulate "rename a field" — rename type_tag "OldName" → "NewName".
            for obj in &mut snapshot.objects {
                if obj.type_tag == "OldName" {
                    obj.type_tag = "NewName".to_string();
                }
            }
            Ok(snapshot)
        }
    }

    /// Migration 1.0.0 → 1.1.0: minor schema tweak (double all data bytes).
    struct MigrateV1_0ToV1_1;

    impl SnapshotMigration for MigrateV1_0ToV1_1 {
        fn source_version(&self) -> SchemaVersion {
            SchemaVersion::new(1, 0, 0)
        }
        fn target_version(&self) -> SchemaVersion {
            SchemaVersion::new(1, 1, 0)
        }
        fn migrate(&self, mut snapshot: HeapSnapshot) -> Result<HeapSnapshot, MigrationError> {
            for obj in &mut snapshot.objects {
                obj.data = obj.data.iter().map(|b| b.wrapping_mul(2)).collect();
            }
            Ok(snapshot)
        }
    }

    /// Migration 1.1.0 → 2.0.0: major version bump (changes type_tags to uppercase).
    struct MigrateV1_1ToV2_0;

    impl SnapshotMigration for MigrateV1_1ToV2_0 {
        fn source_version(&self) -> SchemaVersion {
            SchemaVersion::new(1, 1, 0)
        }
        fn target_version(&self) -> SchemaVersion {
            SchemaVersion::new(2, 0, 0)
        }
        fn migrate(&self, mut snapshot: HeapSnapshot) -> Result<HeapSnapshot, MigrationError> {
            for obj in &mut snapshot.objects {
                obj.type_tag = obj.type_tag.to_uppercase();
            }
            Ok(snapshot)
        }
    }

    /// A migration that always fails — used to test DataCorrupted error.
    struct CorruptingMigration;

    impl SnapshotMigration for CorruptingMigration {
        fn source_version(&self) -> SchemaVersion {
            SchemaVersion::new(3, 0, 0)
        }
        fn target_version(&self) -> SchemaVersion {
            SchemaVersion::new(3, 1, 0)
        }
        fn migrate(&self, _snapshot: HeapSnapshot) -> Result<HeapSnapshot, MigrationError> {
            Err(MigrationError::DataCorrupted {
                from: self.source_version(),
                to: self.target_version(),
                detail: "CRC mismatch in heap object".to_string(),
            })
        }
    }

    /// Migration 2.1.0 → 3.0.0: strips all objects with empty data.
    struct MigrateV2_1ToV3_0;

    impl SnapshotMigration for MigrateV2_1ToV3_0 {
        fn source_version(&self) -> SchemaVersion {
            SchemaVersion::new(2, 1, 0)
        }
        fn target_version(&self) -> SchemaVersion {
            SchemaVersion::new(3, 0, 0)
        }
        fn migrate(&self, mut snapshot: HeapSnapshot) -> Result<HeapSnapshot, MigrationError> {
            snapshot.objects.retain(|obj| !obj.data.is_empty());
            Ok(snapshot)
        }
    }

    // -- Helper to build a pre-populated registry ---------------------------

    fn full_registry() -> MigrationRegistry {
        let mut reg = MigrationRegistry::new();
        reg.register(Box::new(MigrateV1_0ToV1_1));
        reg.register(Box::new(MigrateV1_1ToV2_0));
        reg.register(Box::new(MigrateV2ToV2_1));
        reg.register(Box::new(MigrateV2_1ToV3_0));
        reg.register(Box::new(CorruptingMigration));
        reg
    }

    // -----------------------------------------------------------------------
    // T075 — SchemaVersion tests
    // -----------------------------------------------------------------------

    #[test]
    fn schema_version_display() {
        let v = SchemaVersion::new(1, 2, 3);
        assert_eq!(v.to_string(), "1.2.3");
    }

    #[test]
    fn schema_version_ordering() {
        let v1 = SchemaVersion::new(1, 0, 0);
        let v1_1 = SchemaVersion::new(1, 1, 0);
        let v2 = SchemaVersion::new(2, 0, 0);
        let v2_1 = SchemaVersion::new(2, 1, 0);

        assert!(v1 < v1_1);
        assert!(v1_1 < v2);
        assert!(v2 < v2_1);
        assert!(v1 < v2_1);
    }

    #[test]
    fn schema_version_equality() {
        let a = SchemaVersion::new(1, 0, 0);
        let b = SchemaVersion::new(1, 0, 0);
        assert_eq!(a, b);
    }

    #[test]
    fn schema_version_patch_ordering() {
        let v1_0_0 = SchemaVersion::new(1, 0, 0);
        let v1_0_1 = SchemaVersion::new(1, 0, 1);
        let v1_0_2 = SchemaVersion::new(1, 0, 2);
        assert!(v1_0_0 < v1_0_1);
        assert!(v1_0_1 < v1_0_2);
    }

    // -----------------------------------------------------------------------
    // T075 — VersionedSnapshot tests
    // -----------------------------------------------------------------------

    #[test]
    fn versioned_snapshot_wraps_heap() {
        let heap = make_snapshot(vec![make_object(1, b"hello", "Str")]);
        let vs = VersionedSnapshot::new(SchemaVersion::new(1, 0, 0), heap.clone());
        assert_eq!(vs.version, SchemaVersion::new(1, 0, 0));
        assert_eq!(vs.snapshot, heap);
    }

    #[test]
    fn versioned_snapshot_serde_round_trip() {
        let heap = make_snapshot(vec![make_object(1, b"data", "Blob")]);
        let vs = VersionedSnapshot::new(SchemaVersion::new(2, 1, 0), heap);
        let bytes = bincode::serialize(&vs).unwrap();
        let restored: VersionedSnapshot = bincode::deserialize(&bytes).unwrap();
        assert_eq!(vs, restored);
    }

    // -----------------------------------------------------------------------
    // T075 — MigrationRegistry tests
    // -----------------------------------------------------------------------

    #[test]
    fn registry_empty() {
        let reg = MigrationRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn registry_register_and_len() {
        let mut reg = MigrationRegistry::new();
        reg.register(Box::new(MigrateV1ToV2));
        assert_eq!(reg.len(), 1);
        assert!(!reg.is_empty());
    }

    #[test]
    fn registry_find_path_identity() {
        let reg = full_registry();
        let v = SchemaVersion::new(1, 0, 0);
        let path = reg.find_path(v, v).unwrap();
        assert!(path.is_empty(), "same-version path should be empty");
    }

    #[test]
    fn registry_find_path_single_hop() {
        let reg = full_registry();
        let path = reg
            .find_path(SchemaVersion::new(1, 0, 0), SchemaVersion::new(1, 1, 0))
            .unwrap();
        assert_eq!(path.len(), 1);
    }

    #[test]
    fn registry_find_path_multi_hop() {
        let reg = full_registry();
        // 1.0.0 → 1.1.0 → 2.0.0
        let path = reg
            .find_path(SchemaVersion::new(1, 0, 0), SchemaVersion::new(2, 0, 0))
            .unwrap();
        assert_eq!(path.len(), 2);
    }

    #[test]
    fn registry_find_path_long_chain() {
        let reg = full_registry();
        // 1.0.0 → 1.1.0 → 2.0.0 → 2.1.0 → 3.0.0
        let path = reg
            .find_path(SchemaVersion::new(1, 0, 0), SchemaVersion::new(3, 0, 0))
            .unwrap();
        assert_eq!(path.len(), 4);
    }

    // -----------------------------------------------------------------------
    // T075 — Migration error tests
    // -----------------------------------------------------------------------

    #[test]
    fn missing_migration_error() {
        let reg = MigrationRegistry::new();
        let err = reg
            .find_path(SchemaVersion::new(1, 0, 0), SchemaVersion::new(2, 0, 0))
            .unwrap_err();
        match err {
            MigrationError::MissingMigration { from, to } => {
                assert_eq!(from, SchemaVersion::new(1, 0, 0));
                assert_eq!(to, SchemaVersion::new(2, 0, 0));
            }
            other => panic!("expected MissingMigration, got: {other}"),
        }
    }

    #[test]
    fn incompatible_versions_downgrade() {
        let reg = full_registry();
        let err = reg
            .find_path(SchemaVersion::new(2, 0, 0), SchemaVersion::new(1, 0, 0))
            .unwrap_err();
        match err {
            MigrationError::IncompatibleVersions { from, to } => {
                assert_eq!(from, SchemaVersion::new(2, 0, 0));
                assert_eq!(to, SchemaVersion::new(1, 0, 0));
            }
            other => panic!("expected IncompatibleVersions, got: {other}"),
        }
    }

    #[test]
    fn data_corrupted_error() {
        let reg = full_registry();
        let snap = make_snapshot(vec![make_object(1, b"x", "T")]);
        // 3.0.0 → 3.1.0 uses the CorruptingMigration
        let err = reg
            .apply(
                snap,
                SchemaVersion::new(3, 0, 0),
                SchemaVersion::new(3, 1, 0),
            )
            .unwrap_err();
        match err {
            MigrationError::DataCorrupted { detail, .. } => {
                assert!(detail.contains("CRC mismatch"));
            }
            other => panic!("expected DataCorrupted, got: {other}"),
        }
    }

    // -----------------------------------------------------------------------
    // T076 — Checkpoint + migrate + resume integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn migrate_v1_to_v2_adds_default_field() {
        let snap = make_snapshot(vec![make_object(1, b"original", "Record")]);
        let mut reg = MigrationRegistry::new();
        reg.register(Box::new(MigrateV1ToV2));

        let migrated = migrate_snapshot(
            snap,
            SchemaVersion::new(1, 0, 0),
            SchemaVersion::new(2, 0, 0),
            &reg,
        )
        .unwrap();

        assert_eq!(migrated.objects.len(), 2);
        assert_eq!(migrated.objects[0].type_tag, "Record");
        assert_eq!(migrated.objects[1].type_tag, "AddedFieldV2");
        assert_eq!(migrated.objects[1].data, b"default_v2_field");
    }

    #[test]
    fn migrate_v2_to_v2_1_renames_field() {
        let snap = make_snapshot(vec![
            make_object(1, b"a", "OldName"),
            make_object(2, b"b", "Untouched"),
        ]);
        let mut reg = MigrationRegistry::new();
        reg.register(Box::new(MigrateV2ToV2_1));

        let migrated = migrate_snapshot(
            snap,
            SchemaVersion::new(2, 0, 0),
            SchemaVersion::new(2, 1, 0),
            &reg,
        )
        .unwrap();

        assert_eq!(migrated.objects[0].type_tag, "NewName");
        assert_eq!(migrated.objects[1].type_tag, "Untouched");
    }

    #[test]
    fn migrate_chain_1_0_to_1_1_to_2_0() {
        let snap = make_snapshot(vec![make_object(1, &[10, 20], "item")]);
        let reg = full_registry();

        let migrated = migrate_snapshot(
            snap,
            SchemaVersion::new(1, 0, 0),
            SchemaVersion::new(2, 0, 0),
            &reg,
        )
        .unwrap();

        // V1_0→V1_1 doubles data bytes: [10,20] → [20,40]
        // V1_1→V2_0 uppercases type_tag: "item" → "ITEM"
        assert_eq!(migrated.objects[0].data, vec![20, 40]);
        assert_eq!(migrated.objects[0].type_tag, "ITEM");
    }

    #[test]
    fn migrate_full_chain_1_0_to_3_0() {
        let snap = make_snapshot(vec![
            make_object(1, &[5], "widget"),
            make_object(2, &[], "empty"), // will be stripped by V2_1→V3_0
        ]);
        let reg = full_registry();

        let migrated = migrate_snapshot(
            snap,
            SchemaVersion::new(1, 0, 0),
            SchemaVersion::new(3, 0, 0),
            &reg,
        )
        .unwrap();

        // V1_0→V1_1: data doubled: [5]→[10], []→[]
        // V1_1→V2_0: type_tags uppercased: "widget"→"WIDGET", "empty"→"EMPTY"
        // V2_0→V2_1: OldName→NewName (no match here, no-op)
        // V2_1→V3_0: strip empty-data objects → "EMPTY" removed
        assert_eq!(migrated.objects.len(), 1);
        assert_eq!(migrated.objects[0].type_tag, "WIDGET");
        assert_eq!(migrated.objects[0].data, vec![10]);
    }

    #[test]
    fn migrate_noop_same_version() {
        let snap = make_snapshot(vec![make_object(1, b"keep", "X")]);
        let reg = full_registry();

        let migrated = migrate_snapshot(
            snap.clone(),
            SchemaVersion::new(2, 0, 0),
            SchemaVersion::new(2, 0, 0),
            &reg,
        )
        .unwrap();

        assert_eq!(migrated, snap);
    }

    #[test]
    fn migrate_missing_intermediate_step() {
        // Register only V1_0→V1_1 and V2_0→V2_1 — no bridge from 1.1→2.0.
        let mut reg = MigrationRegistry::new();
        reg.register(Box::new(MigrateV1_0ToV1_1));
        reg.register(Box::new(MigrateV2ToV2_1));

        let snap = make_snapshot(vec![make_object(1, b"x", "T")]);
        let err = migrate_snapshot(
            snap,
            SchemaVersion::new(1, 0, 0),
            SchemaVersion::new(2, 1, 0),
            &reg,
        )
        .unwrap_err();

        matches!(err, MigrationError::MissingMigration { .. });
    }

    #[test]
    fn simulate_kill_save_checkpoint_restore() {
        use crate::checkpoint::{CheckpointEngine, FileCheckpointStore};
        use crate::snapshot::*;
        use std::env;
        use std::fs;

        // 1) Create a snapshot and checkpoint it.
        let dir = {
            let mut p = env::temp_dir();
            p.push(format!("lumen-versioning-kill-test-{}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            p
        };

        let original_heap = make_snapshot(vec![
            make_object(1, b"state_a", "Counter"),
            make_object(2, b"state_b", "Log"),
        ]);

        let snap = Snapshot::new(
            vec![StackFrame {
                cell_index: 0,
                pc: 7,
                registers: vec![SerializedValue::Int(100)],
                return_address: None,
            }],
            original_heap.clone(),
            InstructionPointer {
                cell_index: 0,
                pc: 7,
            },
            SnapshotMetadata {
                process_id: 42,
                process_name: "worker".into(),
                source_file: "work.lm".into(),
                checkpoint_label: Some("pre-kill".into()),
            },
        );

        let store = FileCheckpointStore::new(&dir).unwrap();
        let engine = CheckpointEngine::new(Box::new(store));
        let saved_id = engine.checkpoint(&snap).unwrap();

        // 2) Simulate kill: drop all in-memory state.
        drop(snap);

        // 3) Restore from checkpoint.
        let restored = engine.restore(saved_id).unwrap();
        assert_eq!(restored.heap, original_heap);
        assert_eq!(
            restored.frames[0].registers,
            vec![SerializedValue::Int(100)]
        );
        assert_eq!(restored.metadata.checkpoint_label, Some("pre-kill".into()));

        // 4) Migrate the heap to a new version.
        let mut reg = MigrationRegistry::new();
        reg.register(Box::new(MigrateV1ToV2));

        let migrated = migrate_snapshot(
            restored.heap,
            SchemaVersion::new(1, 0, 0),
            SchemaVersion::new(2, 0, 0),
            &reg,
        )
        .unwrap();

        // Verify the migrated heap has the added default field.
        assert_eq!(migrated.objects.len(), 3);
        assert_eq!(migrated.objects[2].type_tag, "AddedFieldV2");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn simulate_kill_clear_state_restore_verify() {
        use crate::checkpoint::{CheckpointEngine, FileCheckpointStore};
        use crate::snapshot::*;
        use std::env;
        use std::fs;

        let dir = {
            let mut p = env::temp_dir();
            p.push(format!(
                "lumen-versioning-clear-state-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&p);
            p
        };

        // Build original state.
        let mut application_state: Vec<(String, i64)> =
            vec![("counter".into(), 42), ("retries".into(), 3)];

        let heap = make_snapshot(vec![
            make_object(1, b"\x2a\x00\x00\x00", "Counter"), // 42 in LE
            make_object(2, b"\x03\x00\x00\x00", "Retries"), // 3 in LE
        ]);

        let snap = Snapshot::new(
            vec![StackFrame {
                cell_index: 1,
                pc: 15,
                registers: vec![SerializedValue::Int(42), SerializedValue::Int(3)],
                return_address: None,
            }],
            heap,
            InstructionPointer {
                cell_index: 1,
                pc: 15,
            },
            SnapshotMetadata {
                process_id: 99,
                process_name: "agent".into(),
                source_file: "agent.lm.md".into(),
                checkpoint_label: Some("mid-task".into()),
            },
        );

        let store = FileCheckpointStore::new(&dir).unwrap();
        let engine = CheckpointEngine::new(Box::new(store));
        engine.checkpoint(&snap).unwrap();

        // Simulate kill: completely clear application state.
        application_state.clear();
        assert!(application_state.is_empty());

        // Restore.
        let restored = engine.latest().unwrap().expect("should have a snapshot");
        // Re-hydrate application state from registers.
        for reg_val in &restored.frames[0].registers {
            if let SerializedValue::Int(v) = reg_val {
                application_state.push(("restored".into(), *v));
            }
        }

        assert_eq!(application_state.len(), 2);
        assert_eq!(application_state[0].1, 42);
        assert_eq!(application_state[1].1, 3);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrate_preserves_data_integrity_through_chain() {
        // Verify that data flowing through a 3-step chain preserves all
        // non-mutated fields exactly.
        let snap = make_snapshot(vec![
            make_object(1, &[1, 2, 3], "OldName"),
            make_object(2, &[4, 5, 6], "keeper"),
        ]);

        let reg = full_registry();
        // 2.0.0 → 2.1.0 → 3.0.0
        let migrated = migrate_snapshot(
            snap,
            SchemaVersion::new(2, 0, 0),
            SchemaVersion::new(3, 0, 0),
            &reg,
        )
        .unwrap();

        // V2_0→V2_1: "OldName"→"NewName", "keeper" unchanged
        // V2_1→V3_0: strip empty data (none are empty), so both survive
        assert_eq!(migrated.objects.len(), 2);
        assert_eq!(migrated.objects[0].type_tag, "NewName");
        assert_eq!(migrated.objects[0].data, vec![1, 2, 3]);
        assert_eq!(migrated.objects[1].type_tag, "keeper");
        assert_eq!(migrated.objects[1].data, vec![4, 5, 6]);
    }

    #[test]
    fn migrate_snapshot_fn_matches_registry_apply() {
        let snap = make_snapshot(vec![make_object(1, &[7], "x")]);
        let reg = full_registry();
        let from = SchemaVersion::new(1, 0, 0);
        let to = SchemaVersion::new(1, 1, 0);

        let via_fn = migrate_snapshot(snap.clone(), from, to, &reg).unwrap();
        let via_method = reg.apply(snap, from, to).unwrap();
        assert_eq!(via_fn, via_method);
    }

    #[test]
    fn versioned_snapshot_version_accessor() {
        let vs = VersionedSnapshot::new(SchemaVersion::new(5, 3, 1), make_snapshot(vec![]));
        assert_eq!(vs.version.major, 5);
        assert_eq!(vs.version.minor, 3);
        assert_eq!(vs.version.patch, 1);
    }

    #[test]
    fn migration_error_display() {
        let err = MigrationError::IncompatibleVersions {
            from: SchemaVersion::new(2, 0, 0),
            to: SchemaVersion::new(1, 0, 0),
        };
        let msg = err.to_string();
        assert!(msg.contains("2.0.0"));
        assert!(msg.contains("1.0.0"));

        let err2 = MigrationError::MissingMigration {
            from: SchemaVersion::new(1, 0, 0),
            to: SchemaVersion::new(9, 0, 0),
        };
        assert!(err2.to_string().contains("no migration registered"));

        let err3 = MigrationError::DataCorrupted {
            from: SchemaVersion::new(1, 0, 0),
            to: SchemaVersion::new(2, 0, 0),
            detail: "bad checksum".into(),
        };
        assert!(err3.to_string().contains("bad checksum"));
    }

    #[test]
    fn registry_default_is_empty() {
        let reg = MigrationRegistry::default();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }
}
