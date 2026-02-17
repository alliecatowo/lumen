//! Multi-shot continuation support for algebraic effects.
//!
//! This module provides the infrastructure for multi-shot delimited continuations,
//! extending Lumen's existing one-shot algebraic effect system. Multi-shot
//! continuations allow a `resume` to be called multiple times within an effect
//! handler, enabling patterns like backtracking search, nondeterministic
//! computation, and ambiguity operators.
//!
//! # Architecture
//!
//! The core idea is a **serializable snapshot** of VM state at the point of
//! suspension (`ContinuationSnapshot`). For one-shot semantics the snapshot is
//! consumed on resume; for multi-shot semantics it is deep-cloned so each
//! resumption starts from an independent copy of the captured state.
//!
//! A [`MultiShotScheduler`] manages the queue of pending resumptions and
//! collects results, providing the building block for handlers that resume
//! a continuation over a collection of values (e.g. `Choose.choose`).

use std::fmt;

// ---------------------------------------------------------------------------
// SavedValue — serializable value representation for snapshots
// ---------------------------------------------------------------------------

/// A serializable, deep-clonable representation of a runtime value.
///
/// `SavedValue` mirrors the subset of [`crate::values::Value`] variants that
/// are needed to faithfully snapshot registers across resumptions. The `Opaque`
/// variant covers values that cannot be structurally serialized (closures,
/// futures, process references, etc.) — they are stored as a debug description.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum SavedValue {
    /// 64-bit signed integer.
    Int(i64),
    /// 64-bit floating-point number.
    Float(f64),
    /// Boolean value.
    Bool(bool),
    /// Owned string value.
    Str(String),
    /// The null / unit value.
    Null,
    /// Ordered list of saved values.
    List(Vec<SavedValue>),
    /// String-keyed map of saved values.
    Map(Vec<(String, SavedValue)>),
    /// Opaque value that cannot be structurally cloned — stored as a debug
    /// description string. This preserves presence in the register file while
    /// acknowledging the value cannot be faithfully duplicated.
    Opaque(String),
}

impl fmt::Display for SavedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SavedValue::Int(n) => write!(f, "{}", n),
            SavedValue::Float(n) => write!(f, "{}", n),
            SavedValue::Bool(b) => write!(f, "{}", b),
            SavedValue::Str(s) => write!(f, "\"{}\"", s),
            SavedValue::Null => write!(f, "null"),
            SavedValue::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            SavedValue::Map(entries) => {
                write!(f, "{{")?;
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "\"{}\": {}", k, v)?;
                }
                write!(f, "}}")
            }
            SavedValue::Opaque(desc) => write!(f, "<opaque: {}>", desc),
        }
    }
}

// ---------------------------------------------------------------------------
// SavedFrame — one call frame in a snapshot
// ---------------------------------------------------------------------------

/// A saved call frame representing one entry on the VM call stack at the
/// point a continuation was captured.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub struct SavedFrame {
    /// Index of the cell (function) being executed.
    pub cell_index: usize,
    /// Instruction pointer within the cell.
    pub ip: usize,
    /// Base register offset for this frame.
    pub base_reg: usize,
    /// Local variable bindings captured at suspension time.
    pub locals: Vec<(String, SavedValue)>,
}

// ---------------------------------------------------------------------------
// SavedRegister — one register in a snapshot
// ---------------------------------------------------------------------------

/// A saved register entry: an index paired with its value at capture time.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub struct SavedRegister {
    /// The register index (0–255 in the current VM encoding).
    pub index: u8,
    /// The saved value held in this register.
    pub value: SavedValue,
}

// ---------------------------------------------------------------------------
// ContinuationSnapshot — deep-clonable VM state
// ---------------------------------------------------------------------------

/// A deep-clonable snapshot of the VM state captured at a suspension point.
///
/// This is the core data structure that enables multi-shot continuations.
/// Unlike the existing [`super::SuspendedContinuation`] which references
/// live `Value` objects (and is therefore consumed on resume), a
/// `ContinuationSnapshot` stores only [`SavedValue`]s and can be cheaply
/// and independently cloned for each resumption.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub struct ContinuationSnapshot {
    /// Saved call frames (bottom of stack first).
    pub frames: Vec<SavedFrame>,
    /// Saved register file entries that were live at suspension.
    pub registers: Vec<SavedRegister>,
    /// Depth of the effect handler stack at the point of suspension, so that
    /// the handler stack can be restored to the correct level on resume.
    pub handler_stack_depth: usize,
    /// Instruction pointer to resume execution at.
    pub resume_point_ip: usize,
    /// Cell (function) index where execution should resume.
    pub resume_point_cell: usize,
}

#[allow(dead_code)]
impl ContinuationSnapshot {
    /// Produce an independent deep clone suitable for a multi-shot resume.
    ///
    /// Because all fields are `Clone` and contain only owned data
    /// (`SavedValue` rather than `Rc`-wrapped runtime values), the standard
    /// `Clone` implementation already provides a full deep copy. This method
    /// exists as an explicit API entry-point that documents intent.
    pub fn clone_for_resume(&self) -> ContinuationSnapshot {
        self.clone()
    }
}

// ---------------------------------------------------------------------------
// ContinuationMode — one-shot vs. multi-shot
// ---------------------------------------------------------------------------

/// Determines how many times a captured continuation may be resumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ContinuationMode {
    /// The continuation may be resumed at most once. Attempting a second
    /// resume produces [`ContinuationError::AlreadyResumed`].
    OneShot,
    /// The continuation may be resumed an arbitrary number of times (or up
    /// to a configured `max_resumes` limit).
    MultiShot,
}

// ---------------------------------------------------------------------------
// ContinuationError
// ---------------------------------------------------------------------------

/// Errors that can occur when working with continuations.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ContinuationError {
    /// Attempted to resume a one-shot continuation that was already resumed.
    AlreadyResumed,
    /// Attempted to resume a multi-shot continuation beyond its configured
    /// maximum number of resumes.
    MaxResumesExceeded(u64),
    /// The continuation is in an invalid state for the requested operation.
    InvalidState(String),
}

impl fmt::Display for ContinuationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContinuationError::AlreadyResumed => {
                write!(f, "continuation already resumed (one-shot)")
            }
            ContinuationError::MaxResumesExceeded(max) => {
                write!(f, "continuation exceeded maximum resume count of {}", max)
            }
            ContinuationError::InvalidState(msg) => {
                write!(f, "continuation in invalid state: {}", msg)
            }
        }
    }
}

impl std::error::Error for ContinuationError {}

// ---------------------------------------------------------------------------
// ContinuationState — tracks resume count and semantics
// ---------------------------------------------------------------------------

/// Tracks the state of a captured continuation, including its snapshot,
/// resumption mode, and how many times it has been resumed.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ContinuationState {
    /// The captured VM state snapshot.
    snapshot: Option<ContinuationSnapshot>,
    /// Whether this is a one-shot or multi-shot continuation.
    mode: ContinuationMode,
    /// Number of times this continuation has been resumed so far.
    resume_count: u64,
    /// Optional upper bound on the number of resumes (for multi-shot).
    /// `None` means unlimited.
    max_resumes: Option<u64>,
}

#[allow(dead_code)]
impl ContinuationState {
    /// Create a new continuation state wrapping the given snapshot.
    pub fn new(snapshot: ContinuationSnapshot, mode: ContinuationMode) -> Self {
        Self {
            snapshot: Some(snapshot),
            mode,
            resume_count: 0,
            max_resumes: None,
        }
    }

    /// Create a new multi-shot continuation with an explicit resume limit.
    pub fn with_max_resumes(snapshot: ContinuationSnapshot, max_resumes: u64) -> Self {
        Self {
            snapshot: Some(snapshot),
            mode: ContinuationMode::MultiShot,
            resume_count: 0,
            max_resumes: Some(max_resumes),
        }
    }

    /// Returns `true` if the continuation can be resumed at least one more
    /// time given its mode and resume count.
    pub fn can_resume(&self) -> bool {
        if self.snapshot.is_none() {
            return false;
        }
        match self.mode {
            ContinuationMode::OneShot => self.resume_count == 0,
            ContinuationMode::MultiShot => {
                if let Some(max) = self.max_resumes {
                    self.resume_count < max
                } else {
                    true
                }
            }
        }
    }

    /// Prepare a snapshot for resumption.
    ///
    /// - **OneShot**: consumes the snapshot (moves it out), so subsequent
    ///   calls will fail with [`ContinuationError::AlreadyResumed`].
    /// - **MultiShot**: returns a deep clone of the snapshot, leaving the
    ///   original in place for future resumes.
    pub fn prepare_resume(&mut self) -> Result<ContinuationSnapshot, ContinuationError> {
        match self.mode {
            ContinuationMode::OneShot => {
                if self.resume_count > 0 {
                    return Err(ContinuationError::AlreadyResumed);
                }
                let snap = self
                    .snapshot
                    .take()
                    .ok_or(ContinuationError::AlreadyResumed)?;
                self.resume_count = 1;
                Ok(snap)
            }
            ContinuationMode::MultiShot => {
                if let Some(max) = self.max_resumes {
                    if self.resume_count >= max {
                        return Err(ContinuationError::MaxResumesExceeded(max));
                    }
                }
                let snap = self
                    .snapshot
                    .as_ref()
                    .ok_or_else(|| ContinuationError::InvalidState("snapshot missing".to_string()))?
                    .clone_for_resume();
                self.resume_count += 1;
                Ok(snap)
            }
        }
    }

    /// How many times this continuation has been resumed.
    pub fn resume_count(&self) -> u64 {
        self.resume_count
    }

    /// The resumption mode of this continuation.
    pub fn mode(&self) -> ContinuationMode {
        self.mode
    }

    /// The optional maximum number of resumes (only meaningful for multi-shot).
    pub fn max_resumes(&self) -> Option<u64> {
        self.max_resumes
    }
}

// ---------------------------------------------------------------------------
// MultiShotScheduler — manages multiple resumption paths
// ---------------------------------------------------------------------------

/// A scheduler that manages multiple pending resumptions for a multi-shot
/// continuation handler.
///
/// When an effect handler needs to resume a continuation with each element
/// of a collection (e.g. `Choose.choose`), it enqueues a `(snapshot, value)`
/// pair for each element. The VM (or a driver loop) then dequeues and
/// executes each resumption, collecting results.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MultiShotScheduler {
    /// FIFO queue of pending resumptions: `(snapshot_to_restore, value_to_inject)`.
    pending: Vec<(ContinuationSnapshot, SavedValue)>,
    /// Collected results from completed resumptions.
    results: Vec<SavedValue>,
}

#[allow(dead_code)]
impl MultiShotScheduler {
    /// Create a new empty scheduler.
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            results: Vec::new(),
        }
    }

    /// Enqueue a resumption: the continuation will be restored from `snapshot`
    /// and the `value` will be injected into the resume register.
    pub fn enqueue(&mut self, snapshot: ContinuationSnapshot, value: SavedValue) {
        self.pending.push((snapshot, value));
    }

    /// Dequeue the next pending resumption (FIFO order).
    ///
    /// Returns `None` when there are no more pending resumptions.
    pub fn dequeue(&mut self) -> Option<(ContinuationSnapshot, SavedValue)> {
        if self.pending.is_empty() {
            None
        } else {
            Some(self.pending.remove(0))
        }
    }

    /// Record a result from a completed resumption.
    pub fn add_result(&mut self, value: SavedValue) {
        self.results.push(value);
    }

    /// Returns `true` when all enqueued resumptions have been dequeued
    /// (i.e. the pending queue is empty).
    pub fn is_complete(&self) -> bool {
        self.pending.is_empty()
    }

    /// Access the collected results from completed resumptions.
    pub fn results(&self) -> &[SavedValue] {
        &self.results
    }

    /// Number of resumptions still pending.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Number of results collected so far.
    pub fn result_count(&self) -> usize {
        self.results.len()
    }
}

impl Default for MultiShotScheduler {
    fn default() -> Self {
        Self::new()
    }
}
