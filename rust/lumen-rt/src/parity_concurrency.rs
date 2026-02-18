//! Concurrency safety parity checklist for the Lumen language.
//!
//! Tracks how Lumen's concurrency features compare to those found in
//! Go, Tokio/Rust, Erlang/OTP, Python Trio, and other major runtimes.

use std::fmt;

// ---------------------------------------------------------------------------
// Category
// ---------------------------------------------------------------------------

/// Broad category for a concurrency-parity checklist item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConcurrencyCategory {
    TaskScheduling,
    WorkStealing,
    ChannelCommunication,
    ActorModel,
    SupervisorTrees,
    NurseryScoping,
    FutureExecution,
    ParallelPrimitives,
    LockFreeStructures,
    DataRaceProtection,
    DeadlockPrevention,
    ResourceOrdering,
    CancellationSafety,
    StructuredConcurrency,
    BackpressureHandling,
}

impl fmt::Display for ConcurrencyCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::TaskScheduling => "Task Scheduling",
            Self::WorkStealing => "Work Stealing",
            Self::ChannelCommunication => "Channel Communication",
            Self::ActorModel => "Actor Model",
            Self::SupervisorTrees => "Supervisor Trees",
            Self::NurseryScoping => "Nursery Scoping",
            Self::FutureExecution => "Future Execution",
            Self::ParallelPrimitives => "Parallel Primitives",
            Self::LockFreeStructures => "Lock-Free Structures",
            Self::DataRaceProtection => "Data Race Protection",
            Self::DeadlockPrevention => "Deadlock Prevention",
            Self::ResourceOrdering => "Resource Ordering",
            Self::CancellationSafety => "Cancellation Safety",
            Self::StructuredConcurrency => "Structured Concurrency",
            Self::BackpressureHandling => "Backpressure Handling",
        };
        f.write_str(label)
    }
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

/// Implementation status of a concurrency parity item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConcParityStatus {
    /// Fully implemented and tested.
    Implemented,
    /// Partially implemented; the `String` describes what is missing.
    Partial(String),
    /// Designed but not yet implemented.
    Designed,
    /// Not applicable to Lumen; the `String` explains why.
    NotApplicable(String),
}

impl fmt::Display for ConcParityStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Implemented => f.write_str("Implemented"),
            Self::Partial(note) => write!(f, "Partial — {}", note),
            Self::Designed => f.write_str("Designed"),
            Self::NotApplicable(reason) => write!(f, "N/A — {}", reason),
        }
    }
}

impl ConcParityStatus {
    /// Returns `true` when the feature is fully implemented.
    pub fn is_implemented(&self) -> bool {
        matches!(self, Self::Implemented)
    }

    /// Returns `true` when the feature represents a gap (not fully done and
    /// not explicitly inapplicable).
    pub fn is_gap(&self) -> bool {
        matches!(self, Self::Partial(_) | Self::Designed)
    }
}

// ---------------------------------------------------------------------------
// Item
// ---------------------------------------------------------------------------

/// A single entry in the concurrency parity checklist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcurrencyParityItem {
    /// Unique identifier, e.g. `"CONC-001"`.
    pub id: String,
    /// Concurrency sub-area.
    pub category: ConcurrencyCategory,
    /// Short feature name.
    pub feature: String,
    /// Longer description.
    pub description: String,
    /// Current implementation status.
    pub status: ConcParityStatus,
    /// What language/runtime feature this is comparable to.
    pub comparable_to: String,
    /// How Lumen implements (or plans to implement) this.
    pub lumen_approach: String,
}

impl fmt::Display for ConcurrencyParityItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} — {} ({})",
            self.id, self.feature, self.status, self.category
        )
    }
}

// ---------------------------------------------------------------------------
// Checklist
// ---------------------------------------------------------------------------

/// The full concurrency-safety parity checklist.
#[derive(Debug, Clone)]
pub struct ConcurrencyParityChecklist {
    pub items: Vec<ConcurrencyParityItem>,
}

impl ConcurrencyParityChecklist {
    /// Build the complete checklist (>= 30 items).
    pub fn full_checklist() -> Self {
        let items = vec![
            // ---------------------------------------------------------------
            // Task Scheduling
            // ---------------------------------------------------------------
            ConcurrencyParityItem {
                id: "CONC-001".into(),
                category: ConcurrencyCategory::TaskScheduling,
                feature: "M:N green-thread scheduler".into(),
                description: "Multiplexes many lightweight tasks onto a smaller pool of OS threads".into(),
                status: ConcParityStatus::Designed,
                comparable_to: "Go goroutines, Tokio task scheduler".into(),
                lumen_approach: "Lumen VM runs cells on a single-threaded interpreter; future work adds M:N scheduling".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-002".into(),
                category: ConcurrencyCategory::TaskScheduling,
                feature: "Cooperative yielding".into(),
                description: "Tasks voluntarily yield control at await/effect points".into(),
                status: ConcParityStatus::Implemented,
                comparable_to: "Tokio cooperative budget, Go runtime.Gosched".into(),
                lumen_approach: "Effect perform and future await are yield points in the VM dispatch loop".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-003".into(),
                category: ConcurrencyCategory::TaskScheduling,
                feature: "Priority-based task scheduling".into(),
                description: "Higher-priority tasks are dispatched before lower-priority ones".into(),
                status: ConcParityStatus::Designed,
                comparable_to: "Erlang process priorities, OS thread priorities".into(),
                lumen_approach: "Planned via priority annotations on cells/processes".into(),
            },

            // ---------------------------------------------------------------
            // Work Stealing
            // ---------------------------------------------------------------
            ConcurrencyParityItem {
                id: "CONC-004".into(),
                category: ConcurrencyCategory::WorkStealing,
                feature: "Work-stealing task queues".into(),
                description: "Idle worker threads steal tasks from busy workers' local queues".into(),
                status: ConcParityStatus::Designed,
                comparable_to: "Tokio work-stealing scheduler, Go P/M/G model".into(),
                lumen_approach: "Planned for multi-threaded VM backend; current VM is single-threaded".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-005".into(),
                category: ConcurrencyCategory::WorkStealing,
                feature: "LIFO slot for cache locality".into(),
                description: "Newly spawned tasks run on the local thread first for cache friendliness".into(),
                status: ConcParityStatus::Designed,
                comparable_to: "Tokio LIFO slot, Go local run queue".into(),
                lumen_approach: "Planned alongside work-stealing implementation".into(),
            },

            // ---------------------------------------------------------------
            // Channel Communication
            // ---------------------------------------------------------------
            ConcurrencyParityItem {
                id: "CONC-006".into(),
                category: ConcurrencyCategory::ChannelCommunication,
                feature: "Typed bounded channels".into(),
                description: "Fixed-capacity channels with typed messages; sends block when full".into(),
                status: ConcParityStatus::Designed,
                comparable_to: "Go buffered channels, Rust mpsc, Tokio bounded channels".into(),
                lumen_approach: "Effect-based channel primitives with compile-time type checking".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-007".into(),
                category: ConcurrencyCategory::ChannelCommunication,
                feature: "Typed unbounded channels".into(),
                description: "Unlimited-capacity channels with typed messages".into(),
                status: ConcParityStatus::Designed,
                comparable_to: "Tokio unbounded channels, Erlang process mailboxes".into(),
                lumen_approach: "Effect-based channel primitives backed by growable queues".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-008".into(),
                category: ConcurrencyCategory::ChannelCommunication,
                feature: "Select / multi-channel receive".into(),
                description: "Wait on multiple channels simultaneously, process first available".into(),
                status: ConcParityStatus::Implemented,
                comparable_to: "Go select, Tokio tokio::select!, Erlang receive".into(),
                lumen_approach: "Built-in `select` combinator in the VM orchestration builtins".into(),
            },

            // ---------------------------------------------------------------
            // Actor Model
            // ---------------------------------------------------------------
            ConcurrencyParityItem {
                id: "CONC-009".into(),
                category: ConcurrencyCategory::ActorModel,
                feature: "Typed actor mailboxes".into(),
                description: "Each process has a typed mailbox accepting only declared message types".into(),
                status: ConcParityStatus::Partial("Memory processes accept any value; typed mailbox enforcement planned".into()),
                comparable_to: "Erlang process mailbox, Akka typed actors".into(),
                lumen_approach: "Memory processes have append/recall; full typed mailbox is a future enhancement".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-010".into(),
                category: ConcurrencyCategory::ActorModel,
                feature: "Process isolation".into(),
                description: "Processes cannot directly access each other's state".into(),
                status: ConcParityStatus::Implemented,
                comparable_to: "Erlang process isolation, Lunatic actors".into(),
                lumen_approach: "Each process (memory/machine/pipeline) has its own runtime state keyed by instance ID".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-011".into(),
                category: ConcurrencyCategory::ActorModel,
                feature: "Spawn semantics for actors".into(),
                description: "Lightweight actor spawn with message-passing interface".into(),
                status: ConcParityStatus::Partial("Processes are created declaratively; dynamic spawn is planned".into()),
                comparable_to: "Erlang spawn/3, Go go func()".into(),
                lumen_approach: "Processes declared at module level; dynamic spawn under design".into(),
            },

            // ---------------------------------------------------------------
            // Supervisor Trees
            // ---------------------------------------------------------------
            ConcurrencyParityItem {
                id: "CONC-012".into(),
                category: ConcurrencyCategory::SupervisorTrees,
                feature: "Supervisor restart strategies".into(),
                description: "Automatic restart of failed child processes with configurable strategies".into(),
                status: ConcParityStatus::Designed,
                comparable_to: "Erlang/OTP supervisor (one_for_one, one_for_all, rest_for_one)".into(),
                lumen_approach: "Planned via orchestration processes with built-in restart policies".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-013".into(),
                category: ConcurrencyCategory::SupervisorTrees,
                feature: "Process linking / monitoring".into(),
                description: "A process is notified when a linked/monitored process terminates".into(),
                status: ConcParityStatus::Designed,
                comparable_to: "Erlang link/monitor, Akka DeathWatch".into(),
                lumen_approach: "Planned via effect-based lifecycle notifications".into(),
            },

            // ---------------------------------------------------------------
            // Nursery Scoping
            // ---------------------------------------------------------------
            ConcurrencyParityItem {
                id: "CONC-014".into(),
                category: ConcurrencyCategory::NurseryScoping,
                feature: "Nursery-scoped task groups".into(),
                description: "Spawned tasks are bound to a scope; scope waits for all tasks".into(),
                status: ConcParityStatus::Partial("parallel combinator provides grouping; explicit nursery API planned".into()),
                comparable_to: "Python Trio nurseries, Swift structured concurrency TaskGroup".into(),
                lumen_approach: "parallel/race combinators enforce scope; full nursery with cancel-on-error planned".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-015".into(),
                category: ConcurrencyCategory::NurseryScoping,
                feature: "Automatic scope cleanup on error".into(),
                description: "When one task in a scope fails, sibling tasks are cancelled".into(),
                status: ConcParityStatus::Designed,
                comparable_to: "Trio nursery cancel scope, Kotlin coroutineScope".into(),
                lumen_approach: "Planned integration with algebraic effect handlers for error propagation".into(),
            },

            // ---------------------------------------------------------------
            // Future Execution
            // ---------------------------------------------------------------
            ConcurrencyParityItem {
                id: "CONC-016".into(),
                category: ConcurrencyCategory::FutureExecution,
                feature: "Eager future scheduling".into(),
                description: "Futures begin execution immediately when created".into(),
                status: ConcParityStatus::Implemented,
                comparable_to: "JavaScript promises, Tokio spawn".into(),
                lumen_approach: "Default FutureSchedule::Eager in the VM; futures execute on creation".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-017".into(),
                category: ConcurrencyCategory::FutureExecution,
                feature: "Deferred FIFO future scheduling".into(),
                description: "Futures are queued and executed in order, not started eagerly".into(),
                status: ConcParityStatus::Implemented,
                comparable_to: "Rust lazy futures (poll-based), deterministic testing schedulers".into(),
                lumen_approach: "FutureSchedule::DeferredFifo enabled by @deterministic true directive".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-018".into(),
                category: ConcurrencyCategory::FutureExecution,
                feature: "Future state tracking".into(),
                description: "Track whether a future is pending, completed, or errored".into(),
                status: ConcParityStatus::Implemented,
                comparable_to: "JavaScript Promise states, Rust Poll::Ready/Pending".into(),
                lumen_approach: "FutureState enum (Pending, Completed, Error) in the VM".into(),
            },

            // ---------------------------------------------------------------
            // Parallel Primitives
            // ---------------------------------------------------------------
            ConcurrencyParityItem {
                id: "CONC-019".into(),
                category: ConcurrencyCategory::ParallelPrimitives,
                feature: "parallel combinator".into(),
                description: "Run multiple futures concurrently and collect all results".into(),
                status: ConcParityStatus::Implemented,
                comparable_to: "Promise.all (JS), tokio::join!, asyncio.gather".into(),
                lumen_approach: "Built-in `parallel` orchestration builtin with deterministic arg-order semantics".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-020".into(),
                category: ConcurrencyCategory::ParallelPrimitives,
                feature: "race combinator".into(),
                description: "Run multiple futures, return the first to complete".into(),
                status: ConcParityStatus::Implemented,
                comparable_to: "Promise.race (JS), tokio::select!, asyncio.wait FIRST_COMPLETED".into(),
                lumen_approach: "Built-in `race` orchestration builtin".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-021".into(),
                category: ConcurrencyCategory::ParallelPrimitives,
                feature: "vote combinator".into(),
                description: "Run multiple futures and return the majority/consensus result".into(),
                status: ConcParityStatus::Implemented,
                comparable_to: "Custom quorum patterns in distributed systems".into(),
                lumen_approach: "Built-in `vote` orchestration builtin for consensus-style execution".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-022".into(),
                category: ConcurrencyCategory::ParallelPrimitives,
                feature: "timeout combinator".into(),
                description: "Wrap a future with a maximum execution time".into(),
                status: ConcParityStatus::Implemented,
                comparable_to: "tokio::time::timeout, context.WithTimeout (Go)".into(),
                lumen_approach: "Built-in `timeout` orchestration builtin".into(),
            },

            // ---------------------------------------------------------------
            // Lock-Free Structures
            // ---------------------------------------------------------------
            ConcurrencyParityItem {
                id: "CONC-023".into(),
                category: ConcurrencyCategory::LockFreeStructures,
                feature: "Lock-free concurrent data structures".into(),
                description: "Concurrent queues, maps, or stacks that avoid mutex contention".into(),
                status: ConcParityStatus::NotApplicable(
                    "Lumen uses value semantics with Rc/COW; no shared mutable state between tasks".into(),
                ),
                comparable_to: "crossbeam (Rust), java.util.concurrent, Go sync.Map".into(),
                lumen_approach: "Value types with Rc<T> and copy-on-write; no shared mutable references exist".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-024".into(),
                category: ConcurrencyCategory::LockFreeStructures,
                feature: "Atomic reference counting".into(),
                description: "Thread-safe reference counting for shared ownership".into(),
                status: ConcParityStatus::NotApplicable(
                    "Single-threaded VM uses Rc; Arc not needed until multi-threaded backend".into(),
                ),
                comparable_to: "Arc (Rust), shared_ptr (C++)".into(),
                lumen_approach: "Rc<T> for collection values; will migrate to Arc when multi-threaded".into(),
            },

            // ---------------------------------------------------------------
            // Data Race Protection
            // ---------------------------------------------------------------
            ConcurrencyParityItem {
                id: "CONC-025".into(),
                category: ConcurrencyCategory::DataRaceProtection,
                feature: "Compile-time data race prevention".into(),
                description: "Prevent concurrent mutable access at compile time".into(),
                status: ConcParityStatus::Implemented,
                comparable_to: "Rust borrow checker, Go race detector (runtime)".into(),
                lumen_approach: "Lumen has no shared mutable state; value semantics + effect system prevent data races by construction".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-026".into(),
                category: ConcurrencyCategory::DataRaceProtection,
                feature: "Effect-based concurrency control".into(),
                description: "Algebraic effects track side-effects; handler scope limits concurrency hazards".into(),
                status: ConcParityStatus::Implemented,
                comparable_to: "Koka effect system, OCaml 5 effect handlers".into(),
                lumen_approach: "Effect rows on cells declare what side-effects are possible; handlers scope their execution".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-027".into(),
                category: ConcurrencyCategory::DataRaceProtection,
                feature: "Deterministic scheduling mode".into(),
                description: "Opt-in mode that makes all execution order deterministic for testing".into(),
                status: ConcParityStatus::Implemented,
                comparable_to: "Loom (Rust), Shuttle, deterministic simulation testing".into(),
                lumen_approach: "@deterministic true directive: rejects nondeterministic ops, uses DeferredFifo scheduling".into(),
            },

            // ---------------------------------------------------------------
            // Deadlock Prevention
            // ---------------------------------------------------------------
            ConcurrencyParityItem {
                id: "CONC-028".into(),
                category: ConcurrencyCategory::DeadlockPrevention,
                feature: "Deadlock detection".into(),
                description: "Runtime or compile-time detection of circular blocking dependencies".into(),
                status: ConcParityStatus::Partial("No explicit deadlock detector; effect system limits cycles but doesn't detect all".into()),
                comparable_to: "Go runtime deadlock detector, database lock graph analysis".into(),
                lumen_approach: "One-shot continuations prevent re-entrant blocking; full cycle detection planned".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-029".into(),
                category: ConcurrencyCategory::DeadlockPrevention,
                feature: "Timeout-based livelock prevention".into(),
                description: "Operations that could block indefinitely have timeout limits".into(),
                status: ConcParityStatus::Implemented,
                comparable_to: "Go context deadlines, Tokio timeout".into(),
                lumen_approach: "timeout combinator and tool-policy timeout_ms constraint at runtime".into(),
            },

            // ---------------------------------------------------------------
            // Resource Ordering
            // ---------------------------------------------------------------
            ConcurrencyParityItem {
                id: "CONC-030".into(),
                category: ConcurrencyCategory::ResourceOrdering,
                feature: "Grant-based resource acquisition ordering".into(),
                description: "Tool/resource access controlled by grant policies, preventing unordered acquisition".into(),
                status: ConcParityStatus::Implemented,
                comparable_to: "Java synchronized ordering conventions, Rust lock ordering via type system".into(),
                lumen_approach: "Grant policies enforce tool access order; validate_tool_policy checks constraints at dispatch".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-031".into(),
                category: ConcurrencyCategory::ResourceOrdering,
                feature: "Scoped resource cleanup (defer)".into(),
                description: "Resources released in reverse acquisition order when scope exits".into(),
                status: ConcParityStatus::Implemented,
                comparable_to: "Go defer, Rust Drop, Python context managers".into(),
                lumen_approach: "defer blocks execute in LIFO order on scope exit".into(),
            },

            // ---------------------------------------------------------------
            // Cancellation Safety
            // ---------------------------------------------------------------
            ConcurrencyParityItem {
                id: "CONC-032".into(),
                category: ConcurrencyCategory::CancellationSafety,
                feature: "Cancellation-safe futures".into(),
                description: "Dropping/cancelling a future does not leave resources in an inconsistent state".into(),
                status: ConcParityStatus::Partial("Future cancellation exists but drop-safety auditing is manual".into()),
                comparable_to: "Tokio cancellation safety, C# CancellationToken".into(),
                lumen_approach: "Future error state plus defer cleanup; formal cancel-safety analysis planned".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-033".into(),
                category: ConcurrencyCategory::CancellationSafety,
                feature: "Cooperative cancellation tokens".into(),
                description: "Pass a cancellation signal that tasks check periodically".into(),
                status: ConcParityStatus::Designed,
                comparable_to: "Go context.Context cancellation, C# CancellationToken".into(),
                lumen_approach: "Planned via effect-based cancellation signal propagation".into(),
            },

            // ---------------------------------------------------------------
            // Structured Concurrency
            // ---------------------------------------------------------------
            ConcurrencyParityItem {
                id: "CONC-034".into(),
                category: ConcurrencyCategory::StructuredConcurrency,
                feature: "Structured concurrency enforcement".into(),
                description: "All concurrent tasks have a well-defined parent scope that outlives them".into(),
                status: ConcParityStatus::Partial("Combinators enforce structure; user code can escape via tool calls".into()),
                comparable_to: "Swift structured concurrency, Kotlin coroutineScope, Trio nurseries".into(),
                lumen_approach: "parallel/race/vote/select enforce parent-child; grant policies limit escape hatches".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-035".into(),
                category: ConcurrencyCategory::StructuredConcurrency,
                feature: "Error propagation across task boundaries".into(),
                description: "Errors in child tasks propagate to the parent scope automatically".into(),
                status: ConcParityStatus::Implemented,
                comparable_to: "Kotlin structured concurrency exceptions, Trio exception groups".into(),
                lumen_approach: "Future error states bubble up through combinators; effect handlers catch and propagate".into(),
            },

            // ---------------------------------------------------------------
            // Backpressure Handling
            // ---------------------------------------------------------------
            ConcurrencyParityItem {
                id: "CONC-036".into(),
                category: ConcurrencyCategory::BackpressureHandling,
                feature: "Bounded-channel backpressure".into(),
                description: "Producers block when channel is full, preventing unbounded memory growth".into(),
                status: ConcParityStatus::Designed,
                comparable_to: "Go buffered channels, Tokio bounded mpsc".into(),
                lumen_approach: "Planned via bounded effect-based channels with send-blocking semantics".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-037".into(),
                category: ConcurrencyCategory::BackpressureHandling,
                feature: "Rate limiting at tool dispatch".into(),
                description: "Tool calls are throttled by policy to prevent overloading providers".into(),
                status: ConcParityStatus::Implemented,
                comparable_to: "API rate limiters, Tokio rate limiting middleware".into(),
                lumen_approach: "RetryPolicy with backoff in tool dispatch; RateLimit error type with retry_after_ms".into(),
            },
            ConcurrencyParityItem {
                id: "CONC-038".into(),
                category: ConcurrencyCategory::BackpressureHandling,
                feature: "Pipeline stage backpressure".into(),
                description: "Pipeline stages slow down when downstream cannot keep up".into(),
                status: ConcParityStatus::Partial("Pipeline stages execute synchronously; async backpressure planned".into()),
                comparable_to: "Reactive Streams backpressure, Akka Streams".into(),
                lumen_approach: "Pipeline stages chain synchronously; async pipeline with bounded buffers planned".into(),
            },
        ];

        Self { items }
    }

    /// Filter items by category.
    pub fn by_category(&self, cat: ConcurrencyCategory) -> Vec<&ConcurrencyParityItem> {
        self.items
            .iter()
            .filter(|item| item.category == cat)
            .collect()
    }

    /// Number of fully-implemented items.
    pub fn implemented_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status.is_implemented())
            .count()
    }

    /// Total number of items in the checklist.
    pub fn total_count(&self) -> usize {
        self.items.len()
    }

    /// Implementation coverage as a percentage (0.0–100.0).
    pub fn coverage_percent(&self) -> f64 {
        if self.items.is_empty() {
            return 0.0;
        }
        (self.implemented_count() as f64 / self.total_count() as f64) * 100.0
    }

    /// Items that represent gaps (partial or designed, not N/A).
    pub fn gaps(&self) -> Vec<&ConcurrencyParityItem> {
        self.items
            .iter()
            .filter(|item| item.status.is_gap())
            .collect()
    }

    /// Render the checklist as a Markdown table.
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str("# Concurrency Safety Parity Checklist\n\n");
        md.push_str(&format!(
            "**Coverage:** {}/{} implemented ({:.1}%)\n\n",
            self.implemented_count(),
            self.total_count(),
            self.coverage_percent(),
        ));
        md.push_str("| ID | Category | Feature | Status | Comparable To | Lumen Approach |\n");
        md.push_str("|---|---|---|---|---|---|\n");
        for item in &self.items {
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} |\n",
                item.id,
                item.category,
                item.feature,
                item.status,
                item.comparable_to,
                item.lumen_approach,
            ));
        }
        md
    }

    /// Short human-readable summary string.
    pub fn summary(&self) -> String {
        let implemented = self.implemented_count();
        let total = self.total_count();
        let gaps = self.gaps().len();
        let na_count = self
            .items
            .iter()
            .filter(|i| matches!(i.status, ConcParityStatus::NotApplicable(_)))
            .count();
        format!(
            "Concurrency parity: {}/{} implemented ({:.1}%), {} gaps, {} N/A",
            implemented,
            total,
            self.coverage_percent(),
            gaps,
            na_count,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_checklist_has_at_least_30_items() {
        let cl = ConcurrencyParityChecklist::full_checklist();
        assert!(
            cl.total_count() >= 30,
            "expected >= 30 items, got {}",
            cl.total_count()
        );
    }

    #[test]
    fn all_ids_are_unique() {
        let cl = ConcurrencyParityChecklist::full_checklist();
        let mut seen = std::collections::HashSet::new();
        for item in &cl.items {
            assert!(seen.insert(&item.id), "duplicate id: {}", item.id);
        }
    }

    #[test]
    fn coverage_percent_in_range() {
        let cl = ConcurrencyParityChecklist::full_checklist();
        let pct = cl.coverage_percent();
        assert!((0.0..=100.0).contains(&pct), "coverage {pct} out of range");
    }

    #[test]
    fn implemented_count_le_total() {
        let cl = ConcurrencyParityChecklist::full_checklist();
        assert!(cl.implemented_count() <= cl.total_count());
    }
}
