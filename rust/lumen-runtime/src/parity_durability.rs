//! Durability parity checklist — tracks feature coverage relative to industry
//! standards (Temporal, Durable Objects, event sourcing frameworks, etc.).
//!
//! [`DurabilityParityChecklist`] enumerates durability capabilities that
//! production-grade durable execution runtimes provide, and rates Lumen's
//! current coverage for each one.  This is used for internal planning, gap
//! analysis, and progress reporting.

use std::fmt;

// ---------------------------------------------------------------------------
// Category enum
// ---------------------------------------------------------------------------

/// High-level category of a durability feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DurabilityCategory {
    Checkpointing,
    Replay,
    EventSourcing,
    Snapshotting,
    WriteAheadLog,
    TimeTravelDebug,
    VersionedState,
    SchemaEvolution,
    CrashRecovery,
    ExactlyOnceSemantics,
    IdempotencyKeys,
    DurableTimers,
    SagaPattern,
    CompensatingTransactions,
    AuditLogging,
}

impl fmt::Display for DurabilityCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DurabilityCategory::Checkpointing => "Checkpointing",
            DurabilityCategory::Replay => "Replay",
            DurabilityCategory::EventSourcing => "Event Sourcing",
            DurabilityCategory::Snapshotting => "Snapshotting",
            DurabilityCategory::WriteAheadLog => "Write-Ahead Log",
            DurabilityCategory::TimeTravelDebug => "Time-Travel Debug",
            DurabilityCategory::VersionedState => "Versioned State",
            DurabilityCategory::SchemaEvolution => "Schema Evolution",
            DurabilityCategory::CrashRecovery => "Crash Recovery",
            DurabilityCategory::ExactlyOnceSemantics => "Exactly-Once Semantics",
            DurabilityCategory::IdempotencyKeys => "Idempotency Keys",
            DurabilityCategory::DurableTimers => "Durable Timers",
            DurabilityCategory::SagaPattern => "Saga Pattern",
            DurabilityCategory::CompensatingTransactions => "Compensating Transactions",
            DurabilityCategory::AuditLogging => "Audit Logging",
        };
        write!(f, "{}", s)
    }
}

// ---------------------------------------------------------------------------
// Status enum
// ---------------------------------------------------------------------------

/// Implementation status of a durability parity item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DurabParityStatus {
    /// Fully implemented and tested.
    Implemented,
    /// Partially implemented — the `String` describes what's missing.
    Partial(String),
    /// Designed but not yet implemented.
    Designed,
    /// Not applicable to Lumen's execution model.
    NotApplicable(String),
}

impl fmt::Display for DurabParityStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DurabParityStatus::Implemented => write!(f, "Implemented"),
            DurabParityStatus::Partial(detail) => write!(f, "Partial: {}", detail),
            DurabParityStatus::Designed => write!(f, "Designed"),
            DurabParityStatus::NotApplicable(reason) => {
                write!(f, "N/A: {}", reason)
            }
        }
    }
}

impl DurabParityStatus {
    /// Whether this status counts as "implemented" (fully or partially).
    pub fn is_implemented(&self) -> bool {
        matches!(self, DurabParityStatus::Implemented)
    }

    /// Whether this status represents a gap (not fully implemented and not N/A).
    pub fn is_gap(&self) -> bool {
        matches!(
            self,
            DurabParityStatus::Partial(_) | DurabParityStatus::Designed
        )
    }
}

// ---------------------------------------------------------------------------
// Parity item
// ---------------------------------------------------------------------------

/// A single item in the durability parity checklist.
#[derive(Debug, Clone)]
pub struct DurabilityParityItem {
    /// Unique identifier (e.g. "DUR-001").
    pub id: String,
    /// Category this item belongs to.
    pub category: DurabilityCategory,
    /// Short feature name.
    pub feature: String,
    /// Longer description of the feature.
    pub description: String,
    /// Current implementation status.
    pub status: DurabParityStatus,
    /// What industry system this is comparable to.
    pub comparable_to: String,
    /// How Lumen approaches / will approach this feature.
    pub lumen_approach: String,
}

// ---------------------------------------------------------------------------
// Checklist
// ---------------------------------------------------------------------------

/// The full durability parity checklist for the Lumen runtime.
#[derive(Debug, Clone)]
pub struct DurabilityParityChecklist {
    /// All checklist items.
    pub items: Vec<DurabilityParityItem>,
}

impl DurabilityParityChecklist {
    /// Build the complete durability parity checklist with all items.
    pub fn full_checklist() -> Self {
        let items = vec![
            // ---- Checkpointing ----
            DurabilityParityItem {
                id: "DUR-001".into(),
                category: DurabilityCategory::Checkpointing,
                feature: "VM state serialization".into(),
                description: "Serialize full VM state (stack frames, registers, heap) to bytes for checkpoint persistence.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Temporal workflow checkpointing".into(),
                lumen_approach: "Snapshot module with bincode serialization of StackFrame, HeapSnapshot, InstructionPointer.".into(),
            },
            DurabilityParityItem {
                id: "DUR-002".into(),
                category: DurabilityCategory::Checkpointing,
                feature: "Filesystem checkpoint store".into(),
                description: "Persist checkpoints to disk with atomic write-to-tmp + rename for crash safety.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Temporal persistence layer".into(),
                lumen_approach: "FileCheckpointStore with atomic tmp-file writes and CheckpointEngine for higher-level ops.".into(),
            },
            DurabilityParityItem {
                id: "DUR-003".into(),
                category: DurabilityCategory::Checkpointing,
                feature: "Checkpoint compression".into(),
                description: "Compress checkpoint data with gzip to reduce storage footprint.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Durable Objects compressed state".into(),
                lumen_approach: "CheckpointEngine::new_compressed with flate2 gzip compression/decompression.".into(),
            },
            DurabilityParityItem {
                id: "DUR-004".into(),
                category: DurabilityCategory::Checkpointing,
                feature: "Checkpoint pruning".into(),
                description: "Automatically prune old checkpoints, retaining only the N most recent.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Temporal history trimming".into(),
                lumen_approach: "SnapshotPruner with configurable max_retained; works in-memory and on-disk.".into(),
            },

            // ---- Replay ----
            DurabilityParityItem {
                id: "DUR-005".into(),
                category: DurabilityCategory::Replay,
                feature: "Deterministic replay recording".into(),
                description: "Record all nondeterministic events (timestamps, random values, I/O, tool calls, UUIDs) during execution.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Temporal deterministic replay".into(),
                lumen_approach: "ReplayRecorder captures ReplayEvents into a ReplayLog; supports JSON serialization.".into(),
            },
            DurabilityParityItem {
                id: "DUR-006".into(),
                category: DurabilityCategory::Replay,
                feature: "Deterministic replay playback".into(),
                description: "Replay previously recorded events to reproduce execution deterministically.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Temporal replay worker".into(),
                lumen_approach: "ReplayPlayer consumes a ReplayLog in order, supplying recorded values for nondeterministic ops.".into(),
            },
            DurabilityParityItem {
                id: "DUR-007".into(),
                category: DurabilityCategory::Replay,
                feature: "Replay log persistence".into(),
                description: "Save and load replay logs to/from files for offline analysis and regression testing.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Temporal event history export".into(),
                lumen_approach: "ReplayLog::save_to_file / load_from_file with JSON format.".into(),
            },

            // ---- Event sourcing ----
            DurabilityParityItem {
                id: "DUR-008".into(),
                category: DurabilityCategory::EventSourcing,
                feature: "Append-only durable event log".into(),
                description: "Record every nondeterministic operation as an append-only log entry with immediate flush.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Event sourcing append-only stores".into(),
                lumen_approach: "DurableLog with JSON-lines encoding, optional writer backend, and flush-on-append semantics.".into(),
            },
            DurabilityParityItem {
                id: "DUR-009".into(),
                category: DurabilityCategory::EventSourcing,
                feature: "Event log replay reconstruction".into(),
                description: "Reconstruct runtime state by replaying a durable log from the beginning.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Event sourcing state reconstruction".into(),
                lumen_approach: "DurableLog::replay_from and DurableLog::load_from_file to hydrate from stored events.".into(),
            },
            DurabilityParityItem {
                id: "DUR-010".into(),
                category: DurabilityCategory::EventSourcing,
                feature: "Event log compaction".into(),
                description: "Compact the event log by collapsing old events into a snapshot plus recent events.".into(),
                status: DurabParityStatus::Designed,
                comparable_to: "Kafka log compaction, event store snapshots".into(),
                lumen_approach: "Combine checkpoint (snapshot) with DurableLog tail; not yet implemented.".into(),
            },

            // ---- Snapshotting ----
            DurabilityParityItem {
                id: "DUR-011".into(),
                category: DurabilityCategory::Snapshotting,
                feature: "Versioned snapshot format".into(),
                description: "Snapshot format includes a version tag for forward-compatible deserialization.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Durable Objects versioned state".into(),
                lumen_approach: "Snapshot struct with SNAPSHOT_VERSION constant; deserialization rejects mismatched versions.".into(),
            },
            DurabilityParityItem {
                id: "DUR-012".into(),
                category: DurabilityCategory::Snapshotting,
                feature: "Monotonic snapshot IDs".into(),
                description: "Each snapshot receives a globally unique, monotonically increasing ID.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Database LSN / sequence numbers".into(),
                lumen_approach: "SnapshotId with AtomicU64 counter; IDs are ordered for latest-snapshot queries.".into(),
            },
            DurabilityParityItem {
                id: "DUR-013".into(),
                category: DurabilityCategory::Snapshotting,
                feature: "Heap object serialization".into(),
                description: "Serialize all reachable heap objects into a portable format (bincode).".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "JVM heap dumps, V8 heap snapshots".into(),
                lumen_approach: "SerializedValue enum mirrors VM Value; HeapSnapshot collects HeapObjects with type tags.".into(),
            },

            // ---- Write-ahead log ----
            DurabilityParityItem {
                id: "DUR-014".into(),
                category: DurabilityCategory::WriteAheadLog,
                feature: "Write-ahead logging for tool calls".into(),
                description: "Log tool call intent before execution; result appended after completion.".into(),
                status: DurabParityStatus::Partial("DurableLog records results; pre-execution intent logging not yet separated.".into()),
                comparable_to: "Database WAL, Temporal activity heartbeat".into(),
                lumen_approach: "DurableLog append with flush-on-write; intent vs result separation planned.".into(),
            },
            DurabilityParityItem {
                id: "DUR-015".into(),
                category: DurabilityCategory::WriteAheadLog,
                feature: "WAL fsync guarantees".into(),
                description: "Ensure WAL entries survive OS crashes via fsync before acknowledging writes.".into(),
                status: DurabParityStatus::Partial("BufWriter flush is called but not explicit fsync.".into()),
                comparable_to: "PostgreSQL WAL fsync, SQLite WAL".into(),
                lumen_approach: "DurableLog uses BufWriter::flush; OS-level fsync not yet enforced.".into(),
            },

            // ---- Time-travel debug ----
            DurabilityParityItem {
                id: "DUR-016".into(),
                category: DurabilityCategory::TimeTravelDebug,
                feature: "Step-forward debugging".into(),
                description: "Step through execution one instruction at a time, inspecting state at each step.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Temporal web UI replay, rr debugger".into(),
                lumen_approach: "DebugSession with StepHistory capturing DebugState at each step; StepForward command.".into(),
            },
            DurabilityParityItem {
                id: "DUR-017".into(),
                category: DurabilityCategory::TimeTravelDebug,
                feature: "Step-backward debugging".into(),
                description: "Step backward through execution by restoring previously captured states.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "rr reverse execution, Temporal replay scrubbing".into(),
                lumen_approach: "StepHistory ring buffer stores past DebugStates; StepBackward pops previous state.".into(),
            },
            DurabilityParityItem {
                id: "DUR-018".into(),
                category: DurabilityCategory::TimeTravelDebug,
                feature: "Breakpoints".into(),
                description: "Set line-based, event-based, and cell-entry breakpoints during debugging.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "GDB/LLDB breakpoints, IDE breakpoints".into(),
                lumen_approach: "Breakpoint enum (Line, Event, CellEntry) managed by DebugSession with enable/disable.".into(),
            },

            // ---- Versioned state ----
            DurabilityParityItem {
                id: "DUR-019".into(),
                category: DurabilityCategory::VersionedState,
                feature: "Snapshot migration framework".into(),
                description: "Migrate snapshots between schema versions via a chain of registered migrations.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Database schema migrations, Flyway/Liquibase".into(),
                lumen_approach: "SnapshotMigration trait + MigrationRegistry with greedy path-finding; migrate_snapshot helper.".into(),
            },
            DurabilityParityItem {
                id: "DUR-020".into(),
                category: DurabilityCategory::VersionedState,
                feature: "Semantic versioning for schemas".into(),
                description: "Schema versions use semver (major.minor.patch) with ordering for migration planning.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Protobuf schema versions, Avro schema evolution".into(),
                lumen_approach: "SchemaVersion with Ord impl; MigrationRegistry enforces forward-only migration.".into(),
            },

            // ---- Schema evolution ----
            DurabilityParityItem {
                id: "DUR-021".into(),
                category: DurabilityCategory::SchemaEvolution,
                feature: "Schema drift detection".into(),
                description: "Detect when actual data diverges from the declared schema (type mismatches, missing/extra fields).".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Protobuf compatibility checks, Avro schema resolution".into(),
                lumen_approach: "detect_drift recursive comparison with DriftKind classification and severity levels.".into(),
            },
            DurabilityParityItem {
                id: "DUR-022".into(),
                category: DurabilityCategory::SchemaEvolution,
                feature: "Drift history tracking".into(),
                description: "Accumulate drift reports over time for trend analysis and regression detection.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Schema registry compatibility history".into(),
                lumen_approach: "DriftHistory with capacity-limited storage, breaking_drifts queries, and drift_trend per field.".into(),
            },

            // ---- Crash recovery ----
            DurabilityParityItem {
                id: "DUR-023".into(),
                category: DurabilityCategory::CrashRecovery,
                feature: "Resume from last checkpoint".into(),
                description: "After a crash, resume execution from the most recent checkpoint.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Temporal workflow resume, Durable Objects alarm recovery".into(),
                lumen_approach: "CheckpointEngine::latest() + Snapshot::deserialize to restore VM state after crash.".into(),
            },
            DurabilityParityItem {
                id: "DUR-024".into(),
                category: DurabilityCategory::CrashRecovery,
                feature: "Atomic checkpoint writes".into(),
                description: "Checkpoint writes are atomic (write-tmp + rename) so partial writes never corrupt state.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Database crash-safe writes, journaling filesystems".into(),
                lumen_approach: "FileCheckpointStore writes to .snap.tmp then renames to .snap atomically.".into(),
            },
            DurabilityParityItem {
                id: "DUR-025".into(),
                category: DurabilityCategory::CrashRecovery,
                feature: "Process state persistence".into(),
                description: "Persist the state of Lumen processes (memory, machine, pipeline) across restarts.".into(),
                status: DurabParityStatus::Partial("VM-level snapshots exist; process-level constructors not yet persisted.".into()),
                comparable_to: "Durable Objects persistent state".into(),
                lumen_approach: "Snapshot captures VM state; process-specific state (memory entries, machine state) needs dedicated serialization.".into(),
            },

            // ---- Exactly-once semantics ----
            DurabilityParityItem {
                id: "DUR-026".into(),
                category: DurabilityCategory::ExactlyOnceSemantics,
                feature: "Idempotent side-effect execution".into(),
                description: "Ensure side effects execute exactly once even during replay, using cached results.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Temporal activity deduplication".into(),
                lumen_approach: "IdempotencyStore::check_or_execute returns cached results on duplicate keys.".into(),
            },
            DurabilityParityItem {
                id: "DUR-027".into(),
                category: DurabilityCategory::ExactlyOnceSemantics,
                feature: "Deduplication across restarts".into(),
                description: "Idempotency keys survive process restarts via persistent storage.".into(),
                status: DurabParityStatus::Partial("In-memory IdempotencyStore; no disk persistence yet.".into()),
                comparable_to: "Temporal deduplication persistence".into(),
                lumen_approach: "IdempotencyStore is in-memory; planned integration with DurableLog for persistence.".into(),
            },

            // ---- Idempotency keys ----
            DurabilityParityItem {
                id: "DUR-028".into(),
                category: DurabilityCategory::IdempotencyKeys,
                feature: "Idempotency key store".into(),
                description: "Map idempotency keys to cached serialized results for deduplication.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Stripe idempotency keys, Temporal activity IDs".into(),
                lumen_approach: "IdempotencyStore with check_or_execute, invalidate, insert_raw, and get_raw.".into(),
            },
            DurabilityParityItem {
                id: "DUR-029".into(),
                category: DurabilityCategory::IdempotencyKeys,
                feature: "Idempotency key invalidation".into(),
                description: "Selectively invalidate cached results to allow re-execution of specific operations.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Cache invalidation in idempotency layers".into(),
                lumen_approach: "IdempotencyStore::invalidate removes a single key; clear() wipes all.".into(),
            },

            // ---- Durable timers ----
            DurabilityParityItem {
                id: "DUR-030".into(),
                category: DurabilityCategory::DurableTimers,
                feature: "Durable scheduled execution".into(),
                description: "Schedule future execution that survives process restarts (durable timers / alarms).".into(),
                status: DurabParityStatus::Designed,
                comparable_to: "Temporal timers, Durable Objects alarms, Step Functions wait states".into(),
                lumen_approach: "Planned: persist timer state in DurableLog; on recovery, reschedule pending timers.".into(),
            },
            DurabilityParityItem {
                id: "DUR-031".into(),
                category: DurabilityCategory::DurableTimers,
                feature: "Timer replay determinism".into(),
                description: "During replay, timers fire at recorded timestamps rather than wall-clock time.".into(),
                status: DurabParityStatus::Partial("ReplayPlayer supplies recorded timestamps; timer scheduling not yet integrated.".into()),
                comparable_to: "Temporal timer replay".into(),
                lumen_approach: "ReplayPlayer::next_timestamp provides deterministic time; timer integration pending.".into(),
            },

            // ---- Saga pattern ----
            DurabilityParityItem {
                id: "DUR-032".into(),
                category: DurabilityCategory::SagaPattern,
                feature: "Saga orchestration".into(),
                description: "Orchestrate multi-step workflows where each step has a compensating rollback action.".into(),
                status: DurabParityStatus::Designed,
                comparable_to: "Temporal saga pattern, AWS Step Functions".into(),
                lumen_approach: "Planned: model sagas as machine processes with compensation transitions on failure.".into(),
            },

            // ---- Compensating transactions ----
            DurabilityParityItem {
                id: "DUR-033".into(),
                category: DurabilityCategory::CompensatingTransactions,
                feature: "Compensation action registration".into(),
                description: "Register compensating actions for each forward step; on failure, execute in reverse order.".into(),
                status: DurabParityStatus::Designed,
                comparable_to: "Temporal compensation, saga rollback".into(),
                lumen_approach: "Planned: defer-style compensation stack with DurableLog persistence for crash recovery.".into(),
            },
            DurabilityParityItem {
                id: "DUR-034".into(),
                category: DurabilityCategory::CompensatingTransactions,
                feature: "Compensation idempotency".into(),
                description: "Compensating actions are idempotent so they can be safely retried after crashes.".into(),
                status: DurabParityStatus::Designed,
                comparable_to: "Temporal activity retry with idempotency".into(),
                lumen_approach: "Planned: combine IdempotencyStore with compensation actions for at-most-once rollback.".into(),
            },

            // ---- Audit logging ----
            DurabilityParityItem {
                id: "DUR-035".into(),
                category: DurabilityCategory::AuditLogging,
                feature: "Structured audit trail".into(),
                description: "Record security-relevant and workflow-relevant events in a structured, queryable audit log.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Temporal visibility, audit log systems".into(),
                lumen_approach: "CLI audit module provides structured audit logging; runtime trace events record tool calls and state changes.".into(),
            },
            DurabilityParityItem {
                id: "DUR-036".into(),
                category: DurabilityCategory::AuditLogging,
                feature: "Trace recording and replay".into(),
                description: "Record execution traces including tool calls, state transitions, and timing for post-hoc analysis.".into(),
                status: DurabParityStatus::Implemented,
                comparable_to: "Temporal trace export, OpenTelemetry tracing".into(),
                lumen_approach: "VM trace module with --trace-dir flag; trace show command for display; ReplayLog for nondeterministic event capture.".into(),
            },
        ];

        DurabilityParityChecklist { items }
    }

    /// Filter items by category.
    pub fn by_category(&self, cat: DurabilityCategory) -> Vec<&DurabilityParityItem> {
        self.items.iter().filter(|i| i.category == cat).collect()
    }

    /// Count of fully implemented items.
    pub fn implemented_count(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.status.is_implemented())
            .count()
    }

    /// Total number of items.
    pub fn total_count(&self) -> usize {
        self.items.len()
    }

    /// Coverage percentage (implemented / total * 100).
    pub fn coverage_percent(&self) -> f64 {
        if self.items.is_empty() {
            return 0.0;
        }
        (self.implemented_count() as f64 / self.total_count() as f64) * 100.0
    }

    /// Items that represent gaps (Partial or Designed status).
    pub fn gaps(&self) -> Vec<&DurabilityParityItem> {
        self.items.iter().filter(|i| i.status.is_gap()).collect()
    }

    /// Render the checklist as a Markdown table.
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str("# Durability Parity Checklist\n\n");
        md.push_str(&format!(
            "**Coverage**: {}/{} ({:.1}%)\n\n",
            self.implemented_count(),
            self.total_count(),
            self.coverage_percent()
        ));
        md.push_str("| ID | Category | Feature | Status | Comparable To |\n");
        md.push_str("|---|---|---|---|---|\n");
        for item in &self.items {
            let status_str = match &item.status {
                DurabParityStatus::Implemented => "Implemented".to_string(),
                DurabParityStatus::Partial(d) => format!("Partial: {}", d),
                DurabParityStatus::Designed => "Designed".to_string(),
                DurabParityStatus::NotApplicable(r) => format!("N/A: {}", r),
            };
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                item.id, item.category, item.feature, status_str, item.comparable_to
            ));
        }
        md
    }

    /// A brief summary string with counts by status.
    pub fn summary(&self) -> String {
        let implemented = self.implemented_count();
        let partial = self
            .items
            .iter()
            .filter(|i| matches!(i.status, DurabParityStatus::Partial(_)))
            .count();
        let designed = self
            .items
            .iter()
            .filter(|i| matches!(i.status, DurabParityStatus::Designed))
            .count();
        let na = self
            .items
            .iter()
            .filter(|i| matches!(i.status, DurabParityStatus::NotApplicable(_)))
            .count();
        let total = self.total_count();

        format!(
            "Durability Parity: {}/{} implemented ({:.1}%) | {} partial | {} designed | {} N/A",
            implemented,
            total,
            self.coverage_percent(),
            partial,
            designed,
            na
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_checklist_has_at_least_30_items() {
        let cl = DurabilityParityChecklist::full_checklist();
        assert!(
            cl.total_count() >= 30,
            "expected >= 30 items, got {}",
            cl.total_count()
        );
    }

    #[test]
    fn all_ids_are_unique() {
        let cl = DurabilityParityChecklist::full_checklist();
        let mut ids: Vec<&str> = cl.items.iter().map(|i| i.id.as_str()).collect();
        let before = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(before, ids.len(), "duplicate IDs found");
    }

    #[test]
    fn all_items_have_nonempty_fields() {
        let cl = DurabilityParityChecklist::full_checklist();
        for item in &cl.items {
            assert!(!item.id.is_empty(), "item has empty id");
            assert!(
                !item.feature.is_empty(),
                "item {} has empty feature",
                item.id
            );
            assert!(
                !item.description.is_empty(),
                "item {} has empty description",
                item.id
            );
            assert!(
                !item.comparable_to.is_empty(),
                "item {} has empty comparable_to",
                item.id
            );
            assert!(
                !item.lumen_approach.is_empty(),
                "item {} has empty lumen_approach",
                item.id
            );
        }
    }

    #[test]
    fn implemented_count_positive() {
        let cl = DurabilityParityChecklist::full_checklist();
        assert!(
            cl.implemented_count() > 0,
            "should have some implemented items"
        );
    }

    #[test]
    fn coverage_percent_in_range() {
        let cl = DurabilityParityChecklist::full_checklist();
        let pct = cl.coverage_percent();
        assert!(pct >= 0.0 && pct <= 100.0, "coverage {pct} out of range");
    }

    #[test]
    fn coverage_percent_matches_counts() {
        let cl = DurabilityParityChecklist::full_checklist();
        let expected = (cl.implemented_count() as f64 / cl.total_count() as f64) * 100.0;
        assert!(
            (cl.coverage_percent() - expected).abs() < f64::EPSILON,
            "coverage mismatch"
        );
    }

    #[test]
    fn gaps_returns_non_implemented_items() {
        let cl = DurabilityParityChecklist::full_checklist();
        let gaps = cl.gaps();
        for gap in &gaps {
            assert!(
                gap.status.is_gap(),
                "item {} should be a gap but is {:?}",
                gap.id,
                gap.status
            );
        }
    }

    #[test]
    fn gaps_plus_implemented_plus_na_equals_total() {
        let cl = DurabilityParityChecklist::full_checklist();
        let gaps = cl.gaps().len();
        let implemented = cl.implemented_count();
        let na = cl
            .items
            .iter()
            .filter(|i| matches!(i.status, DurabParityStatus::NotApplicable(_)))
            .count();
        assert_eq!(
            gaps + implemented + na,
            cl.total_count(),
            "gap + implemented + N/A should equal total"
        );
    }

    #[test]
    fn by_category_checkpointing() {
        let cl = DurabilityParityChecklist::full_checklist();
        let items = cl.by_category(DurabilityCategory::Checkpointing);
        assert!(!items.is_empty());
        for item in &items {
            assert_eq!(item.category, DurabilityCategory::Checkpointing);
        }
    }

    #[test]
    fn by_category_replay() {
        let cl = DurabilityParityChecklist::full_checklist();
        let items = cl.by_category(DurabilityCategory::Replay);
        assert!(!items.is_empty());
        for item in &items {
            assert_eq!(item.category, DurabilityCategory::Replay);
        }
    }

    #[test]
    fn by_category_event_sourcing() {
        let cl = DurabilityParityChecklist::full_checklist();
        let items = cl.by_category(DurabilityCategory::EventSourcing);
        assert!(!items.is_empty());
    }

    #[test]
    fn by_category_time_travel_debug() {
        let cl = DurabilityParityChecklist::full_checklist();
        let items = cl.by_category(DurabilityCategory::TimeTravelDebug);
        assert!(!items.is_empty());
    }

    #[test]
    fn by_category_crash_recovery() {
        let cl = DurabilityParityChecklist::full_checklist();
        let items = cl.by_category(DurabilityCategory::CrashRecovery);
        assert!(!items.is_empty());
    }

    #[test]
    fn by_category_exactly_once() {
        let cl = DurabilityParityChecklist::full_checklist();
        let items = cl.by_category(DurabilityCategory::ExactlyOnceSemantics);
        assert!(!items.is_empty());
    }

    #[test]
    fn by_category_idempotency_keys() {
        let cl = DurabilityParityChecklist::full_checklist();
        let items = cl.by_category(DurabilityCategory::IdempotencyKeys);
        assert!(!items.is_empty());
    }

    #[test]
    fn to_markdown_contains_header() {
        let cl = DurabilityParityChecklist::full_checklist();
        let md = cl.to_markdown();
        assert!(md.contains("# Durability Parity Checklist"));
        assert!(md.contains("**Coverage**"));
    }

    #[test]
    fn to_markdown_contains_all_ids() {
        let cl = DurabilityParityChecklist::full_checklist();
        let md = cl.to_markdown();
        for item in &cl.items {
            assert!(md.contains(&item.id), "markdown missing id {}", item.id);
        }
    }

    #[test]
    fn to_markdown_contains_table_header() {
        let cl = DurabilityParityChecklist::full_checklist();
        let md = cl.to_markdown();
        assert!(md.contains("| ID |"));
        assert!(md.contains("| Category |"));
        assert!(md.contains("| Feature |"));
        assert!(md.contains("| Status |"));
    }

    #[test]
    fn summary_contains_counts() {
        let cl = DurabilityParityChecklist::full_checklist();
        let summary = cl.summary();
        assert!(summary.contains("Durability Parity:"));
        assert!(summary.contains("implemented"));
        assert!(summary.contains("partial"));
        assert!(summary.contains("designed"));
    }

    #[test]
    fn summary_contains_percentage() {
        let cl = DurabilityParityChecklist::full_checklist();
        let summary = cl.summary();
        assert!(summary.contains('%'), "summary should contain percentage");
    }

    #[test]
    fn status_display_implemented() {
        assert_eq!(DurabParityStatus::Implemented.to_string(), "Implemented");
    }

    #[test]
    fn status_display_partial() {
        let s = DurabParityStatus::Partial("half done".into());
        assert_eq!(s.to_string(), "Partial: half done");
    }

    #[test]
    fn status_display_designed() {
        assert_eq!(DurabParityStatus::Designed.to_string(), "Designed");
    }

    #[test]
    fn status_display_not_applicable() {
        let s = DurabParityStatus::NotApplicable("no need".into());
        assert_eq!(s.to_string(), "N/A: no need");
    }

    #[test]
    fn status_is_implemented_true() {
        assert!(DurabParityStatus::Implemented.is_implemented());
    }

    #[test]
    fn status_is_implemented_false_for_partial() {
        assert!(!DurabParityStatus::Partial("x".into()).is_implemented());
    }

    #[test]
    fn status_is_implemented_false_for_designed() {
        assert!(!DurabParityStatus::Designed.is_implemented());
    }

    #[test]
    fn status_is_gap_partial() {
        assert!(DurabParityStatus::Partial("x".into()).is_gap());
    }

    #[test]
    fn status_is_gap_designed() {
        assert!(DurabParityStatus::Designed.is_gap());
    }

    #[test]
    fn status_is_gap_false_for_implemented() {
        assert!(!DurabParityStatus::Implemented.is_gap());
    }

    #[test]
    fn status_is_gap_false_for_na() {
        assert!(!DurabParityStatus::NotApplicable("reason".into()).is_gap());
    }

    #[test]
    fn category_display_all_variants() {
        // Verify every variant has a non-empty display string.
        let categories = [
            DurabilityCategory::Checkpointing,
            DurabilityCategory::Replay,
            DurabilityCategory::EventSourcing,
            DurabilityCategory::Snapshotting,
            DurabilityCategory::WriteAheadLog,
            DurabilityCategory::TimeTravelDebug,
            DurabilityCategory::VersionedState,
            DurabilityCategory::SchemaEvolution,
            DurabilityCategory::CrashRecovery,
            DurabilityCategory::ExactlyOnceSemantics,
            DurabilityCategory::IdempotencyKeys,
            DurabilityCategory::DurableTimers,
            DurabilityCategory::SagaPattern,
            DurabilityCategory::CompensatingTransactions,
            DurabilityCategory::AuditLogging,
        ];
        for cat in &categories {
            let s = cat.to_string();
            assert!(!s.is_empty(), "{:?} has empty display", cat);
        }
    }

    #[test]
    fn category_equality() {
        assert_eq!(
            DurabilityCategory::Checkpointing,
            DurabilityCategory::Checkpointing
        );
        assert_ne!(
            DurabilityCategory::Checkpointing,
            DurabilityCategory::Replay
        );
    }

    #[test]
    fn empty_checklist_coverage_is_zero() {
        let cl = DurabilityParityChecklist { items: vec![] };
        assert!((cl.coverage_percent() - 0.0).abs() < f64::EPSILON);
        assert_eq!(cl.implemented_count(), 0);
        assert_eq!(cl.total_count(), 0);
    }

    #[test]
    fn empty_checklist_gaps_empty() {
        let cl = DurabilityParityChecklist { items: vec![] };
        assert!(cl.gaps().is_empty());
    }

    #[test]
    fn empty_checklist_by_category_empty() {
        let cl = DurabilityParityChecklist { items: vec![] };
        assert!(cl.by_category(DurabilityCategory::Replay).is_empty());
    }

    #[test]
    fn by_category_saga_pattern() {
        let cl = DurabilityParityChecklist::full_checklist();
        let items = cl.by_category(DurabilityCategory::SagaPattern);
        assert!(!items.is_empty());
    }

    #[test]
    fn by_category_compensating_transactions() {
        let cl = DurabilityParityChecklist::full_checklist();
        let items = cl.by_category(DurabilityCategory::CompensatingTransactions);
        assert!(!items.is_empty());
    }

    #[test]
    fn by_category_audit_logging() {
        let cl = DurabilityParityChecklist::full_checklist();
        let items = cl.by_category(DurabilityCategory::AuditLogging);
        assert!(!items.is_empty());
    }

    #[test]
    fn by_category_durable_timers() {
        let cl = DurabilityParityChecklist::full_checklist();
        let items = cl.by_category(DurabilityCategory::DurableTimers);
        assert!(!items.is_empty());
    }

    #[test]
    fn by_category_snapshotting() {
        let cl = DurabilityParityChecklist::full_checklist();
        let items = cl.by_category(DurabilityCategory::Snapshotting);
        assert!(!items.is_empty());
    }

    #[test]
    fn by_category_write_ahead_log() {
        let cl = DurabilityParityChecklist::full_checklist();
        let items = cl.by_category(DurabilityCategory::WriteAheadLog);
        assert!(!items.is_empty());
    }

    #[test]
    fn by_category_versioned_state() {
        let cl = DurabilityParityChecklist::full_checklist();
        let items = cl.by_category(DurabilityCategory::VersionedState);
        assert!(!items.is_empty());
    }

    #[test]
    fn by_category_schema_evolution() {
        let cl = DurabilityParityChecklist::full_checklist();
        let items = cl.by_category(DurabilityCategory::SchemaEvolution);
        assert!(!items.is_empty());
    }

    #[test]
    fn all_15_categories_covered() {
        let cl = DurabilityParityChecklist::full_checklist();
        let all_cats = [
            DurabilityCategory::Checkpointing,
            DurabilityCategory::Replay,
            DurabilityCategory::EventSourcing,
            DurabilityCategory::Snapshotting,
            DurabilityCategory::WriteAheadLog,
            DurabilityCategory::TimeTravelDebug,
            DurabilityCategory::VersionedState,
            DurabilityCategory::SchemaEvolution,
            DurabilityCategory::CrashRecovery,
            DurabilityCategory::ExactlyOnceSemantics,
            DurabilityCategory::IdempotencyKeys,
            DurabilityCategory::DurableTimers,
            DurabilityCategory::SagaPattern,
            DurabilityCategory::CompensatingTransactions,
            DurabilityCategory::AuditLogging,
        ];
        for cat in &all_cats {
            let items = cl.by_category(*cat);
            assert!(
                !items.is_empty(),
                "category {:?} has no items in checklist",
                cat
            );
        }
    }
}
