//! Time-travel debugger state management.
//!
//! This module provides the core data structures and logic for a step-forward /
//! step-backward debugging experience built on deterministic replay.
//!
//! [`DebugSession`] manages a replay session:
//! - The user can set and remove [`Breakpoint`]s (line-based or event-based).
//! - Each step captures a [`DebugState`] snapshot (registers, stack depth,
//!   instruction pointer, variable bindings).
//! - [`StepHistory`] stores a configurable ring buffer of past states so that
//!   stepping backward is O(1).
//! - [`DebugCommand`] enumerates the commands the future CLI will dispatch.
//!
//! The actual CLI front-end will live in `lumen-cli`; this module only provides
//! the session / state management layer.

use crate::services::snapshot::{InstructionPointer, SerializedValue};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

// ---------------------------------------------------------------------------
// Breakpoints
// ---------------------------------------------------------------------------

/// Unique identifier for a breakpoint within a session.
pub type BreakpointId = u64;

/// A breakpoint that halts execution when its condition is met.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Breakpoint {
    /// Stop when the instruction pointer reaches a specific source line.
    Line {
        /// Breakpoint identifier (assigned by the session).
        id: BreakpointId,
        /// Source file path (may be empty for single-file programs).
        file: String,
        /// 1-indexed line number.
        line: usize,
        /// Whether the breakpoint is currently active.
        enabled: bool,
    },
    /// Stop when a specific replay event kind is encountered.
    Event {
        id: BreakpointId,
        /// One of: "Timestamp", "Random", "IoResult", "ToolResponse", "Uuid".
        event_kind: String,
        enabled: bool,
    },
    /// Stop when a specific cell (function) is entered.
    CellEntry {
        id: BreakpointId,
        /// Cell name to break on.
        cell_name: String,
        enabled: bool,
    },
}

impl Breakpoint {
    /// The unique ID of this breakpoint.
    pub fn id(&self) -> BreakpointId {
        match self {
            Breakpoint::Line { id, .. }
            | Breakpoint::Event { id, .. }
            | Breakpoint::CellEntry { id, .. } => *id,
        }
    }

    /// Whether the breakpoint is currently enabled.
    pub fn is_enabled(&self) -> bool {
        match self {
            Breakpoint::Line { enabled, .. }
            | Breakpoint::Event { enabled, .. }
            | Breakpoint::CellEntry { enabled, .. } => *enabled,
        }
    }

    /// Enable or disable this breakpoint.
    pub fn set_enabled(&mut self, value: bool) {
        match self {
            Breakpoint::Line { enabled, .. }
            | Breakpoint::Event { enabled, .. }
            | Breakpoint::CellEntry { enabled, .. } => *enabled = value,
        }
    }
}

// ---------------------------------------------------------------------------
// Debug state
// ---------------------------------------------------------------------------

/// A snapshot of VM state captured at a single debug step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DebugState {
    /// Step index (monotonically increasing within a session).
    pub step: u64,
    /// Current instruction pointer.
    pub ip: InstructionPointer,
    /// Call-stack depth at this point.
    pub stack_depth: usize,
    /// Name of the current cell being executed (if known).
    pub current_cell: Option<String>,
    /// Source line number corresponding to the instruction (if available).
    pub source_line: Option<usize>,
    /// Register contents (register index → serialized value).
    pub registers: Vec<SerializedValue>,
    /// Named variable bindings visible in the current scope.
    pub variables: BTreeMap<String, SerializedValue>,
}

// ---------------------------------------------------------------------------
// Debug commands
// ---------------------------------------------------------------------------

/// Commands that the future CLI will translate user input into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugCommand {
    /// Execute one instruction forward.
    StepForward,
    /// Go back one step in history.
    StepBackward,
    /// Continue execution until a breakpoint or program end.
    Continue,
    /// Run backward until a breakpoint or the start of history.
    ContinueBackward,
    /// Set a breakpoint and return its ID.
    SetBreakpoint(Breakpoint),
    /// Remove a breakpoint by ID.
    RemoveBreakpoint(BreakpointId),
    /// Enable or disable a breakpoint.
    ToggleBreakpoint(BreakpointId),
    /// Inspect a variable by name in the current scope.
    Inspect(String),
    /// Print the current call stack.
    PrintStack,
    /// Print all breakpoints.
    ListBreakpoints,
    /// Quit the debug session.
    Quit,
}

// ---------------------------------------------------------------------------
// Step history (ring buffer)
// ---------------------------------------------------------------------------

/// A bounded ring buffer of [`DebugState`] snapshots for backward navigation.
///
/// When the buffer is full, the oldest entry is overwritten.  This bounds
/// memory usage for long-running debug sessions.
pub struct StepHistory {
    buffer: Vec<Option<DebugState>>,
    /// Points to the next slot to write into.
    write_pos: usize,
    /// How many valid entries are stored (≤ capacity).
    count: usize,
    /// Maximum number of states to retain.
    capacity: usize,
}

impl StepHistory {
    /// Create a new step history with the given maximum depth.
    ///
    /// # Panics
    /// Panics if `capacity` is 0.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "StepHistory capacity must be > 0");
        StepHistory {
            buffer: (0..capacity).map(|_| None).collect(),
            write_pos: 0,
            count: 0,
            capacity,
        }
    }

    /// Push a new state into the history.  Overwrites the oldest entry if full.
    pub fn push(&mut self, state: DebugState) {
        self.buffer[self.write_pos] = Some(state);
        self.write_pos = (self.write_pos + 1) % self.capacity;
        if self.count < self.capacity {
            self.count += 1;
        }
    }

    /// Pop and return the most recently pushed state, or `None` if empty.
    pub fn pop(&mut self) -> Option<DebugState> {
        if self.count == 0 {
            return None;
        }
        // Move write_pos back one slot.
        self.write_pos = if self.write_pos == 0 {
            self.capacity - 1
        } else {
            self.write_pos - 1
        };
        self.count -= 1;
        self.buffer[self.write_pos].take()
    }

    /// Peek at the most recently pushed state without removing it.
    pub fn peek(&self) -> Option<&DebugState> {
        if self.count == 0 {
            return None;
        }
        let idx = if self.write_pos == 0 {
            self.capacity - 1
        } else {
            self.write_pos - 1
        };
        self.buffer[idx].as_ref()
    }

    /// Number of states currently stored.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether the history is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Maximum number of states this history can hold.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Clear all history entries.
    pub fn clear(&mut self) {
        for slot in &mut self.buffer {
            *slot = None;
        }
        self.write_pos = 0;
        self.count = 0;
    }

    /// Iterate over states from oldest to newest.
    pub fn iter(&self) -> impl Iterator<Item = &DebugState> {
        let start = if self.count < self.capacity {
            0
        } else {
            self.write_pos
        };
        let cap = self.capacity;
        let count = self.count;
        let buffer = &self.buffer;
        (0..count).map(move |i| {
            let idx = (start + i) % cap;
            buffer[idx].as_ref().unwrap()
        })
    }
}

// ---------------------------------------------------------------------------
// Debug session
// ---------------------------------------------------------------------------

/// The result of a debug command execution.
#[derive(Debug, Clone)]
pub enum DebugResponse {
    /// A step was taken; here is the new state.
    Stepped(DebugState),
    /// Execution continued and hit a breakpoint.
    BreakpointHit {
        breakpoint_id: BreakpointId,
        state: DebugState,
    },
    /// Execution completed (no more instructions).
    Finished,
    /// The beginning of recorded history was reached (can't go further back).
    HistoryStart,
    /// Variable inspection result.
    InspectResult {
        name: String,
        value: Option<SerializedValue>,
    },
    /// Call stack snapshot.
    StackTrace(Vec<StackEntry>),
    /// The session was quit.
    Quit,
    /// List of all breakpoints.
    Breakpoints(Vec<Breakpoint>),
    /// A breakpoint was set.
    BreakpointSet(BreakpointId),
    /// A breakpoint was removed.
    BreakpointRemoved(BreakpointId),
    /// A breakpoint was toggled.
    BreakpointToggled(BreakpointId, bool),
}

/// A single entry in a call stack trace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StackEntry {
    /// Frame depth (0 = outermost).
    pub depth: usize,
    /// Name of the cell at this frame.
    pub cell_name: Option<String>,
    /// Instruction pointer within that cell.
    pub ip: InstructionPointer,
    /// Source line (if mapped).
    pub source_line: Option<usize>,
}

/// Manages a time-travel debugging session.
///
/// The session is the state-management core.  It tracks breakpoints, captures
/// [`DebugState`] at each step, and stores them in a [`StepHistory`] ring
/// buffer for backward navigation.
///
/// The session does NOT own or drive the VM.  Instead the host (future CLI)
/// calls `record_step` after each VM step, and the session provides queries
/// and navigation.
pub struct DebugSession {
    /// All registered breakpoints, keyed by ID.
    breakpoints: HashMap<BreakpointId, Breakpoint>,
    /// Next breakpoint ID to assign.
    next_bp_id: BreakpointId,
    /// Ring buffer of captured states.
    history: StepHistory,
    /// The state we are currently viewing (may be in the past if we stepped backward).
    current_state: Option<DebugState>,
    /// Total number of steps recorded.
    total_steps: u64,
    /// Whether the session is still active.
    active: bool,
    /// Stack frames for the current position (updated by the host).
    stack: Vec<StackEntry>,
}

impl DebugSession {
    /// Create a new debug session with the given history depth.
    pub fn new(history_capacity: usize) -> Self {
        DebugSession {
            breakpoints: HashMap::new(),
            next_bp_id: 1,
            history: StepHistory::new(history_capacity),
            current_state: None,
            total_steps: 0,
            active: true,
            stack: Vec::new(),
        }
    }

    /// Whether the session is still active (not quit).
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Total number of steps recorded.
    pub fn total_steps(&self) -> u64 {
        self.total_steps
    }

    /// Read-only access to the current debug state.
    pub fn current_state(&self) -> Option<&DebugState> {
        self.current_state.as_ref()
    }

    /// Read-only access to the step history.
    pub fn history(&self) -> &StepHistory {
        &self.history
    }

    // -- Breakpoints --------------------------------------------------------

    /// Add a breakpoint and return its assigned ID.
    pub fn add_breakpoint(&mut self, mut bp: Breakpoint) -> BreakpointId {
        let id = self.next_bp_id;
        self.next_bp_id += 1;
        // Patch the ID into the breakpoint.
        match &mut bp {
            Breakpoint::Line { id: bp_id, .. }
            | Breakpoint::Event { id: bp_id, .. }
            | Breakpoint::CellEntry { id: bp_id, .. } => *bp_id = id,
        }
        self.breakpoints.insert(id, bp);
        id
    }

    /// Remove a breakpoint by ID.  Returns `true` if it existed.
    pub fn remove_breakpoint(&mut self, id: BreakpointId) -> bool {
        self.breakpoints.remove(&id).is_some()
    }

    /// Toggle a breakpoint's enabled state. Returns the new state, or `None`
    /// if the ID was not found.
    pub fn toggle_breakpoint(&mut self, id: BreakpointId) -> Option<bool> {
        if let Some(bp) = self.breakpoints.get_mut(&id) {
            let new_state = !bp.is_enabled();
            bp.set_enabled(new_state);
            Some(new_state)
        } else {
            None
        }
    }

    /// Get a breakpoint by ID.
    pub fn get_breakpoint(&self, id: BreakpointId) -> Option<&Breakpoint> {
        self.breakpoints.get(&id)
    }

    /// List all breakpoints.
    pub fn breakpoints(&self) -> Vec<Breakpoint> {
        let mut bps: Vec<_> = self.breakpoints.values().cloned().collect();
        bps.sort_by_key(|b| b.id());
        bps
    }

    /// Check if a given state hits any enabled breakpoint.
    pub fn check_breakpoints(&self, state: &DebugState) -> Option<BreakpointId> {
        for bp in self.breakpoints.values() {
            if !bp.is_enabled() {
                continue;
            }
            match bp {
                Breakpoint::Line { line, file, id, .. } => {
                    if let Some(sl) = state.source_line {
                        if sl == *line {
                            // If file is empty, match any file.
                            if file.is_empty() {
                                return Some(*id);
                            }
                            // If the state has a current_cell, use it as a proxy
                            // (actual file matching will be done by the CLI).
                            return Some(*id);
                        }
                    }
                }
                Breakpoint::CellEntry { cell_name, id, .. } => {
                    if let Some(ref cc) = state.current_cell {
                        if cc == cell_name && state.ip.pc == 0 {
                            return Some(*id);
                        }
                    }
                }
                // Event breakpoints are checked separately via check_event_breakpoint
                Breakpoint::Event { .. } => {}
            }
        }
        None
    }

    /// Check if a replay event kind matches any enabled event breakpoint.
    pub fn check_event_breakpoint(&self, event_kind: &str) -> Option<BreakpointId> {
        for bp in self.breakpoints.values() {
            if !bp.is_enabled() {
                continue;
            }
            if let Breakpoint::Event {
                event_kind: ek, id, ..
            } = bp
            {
                if ek == event_kind {
                    return Some(*id);
                }
            }
        }
        None
    }

    // -- Step recording -----------------------------------------------------

    /// Record a new debug state from the host (VM).
    ///
    /// This pushes the previous `current_state` into the history ring buffer
    /// and sets the new state as current.
    pub fn record_step(&mut self, state: DebugState) {
        if let Some(prev) = self.current_state.take() {
            self.history.push(prev);
        }
        self.total_steps += 1;
        self.current_state = Some(state);
    }

    /// Update the call stack (called by the host after each step).
    pub fn update_stack(&mut self, stack: Vec<StackEntry>) {
        self.stack = stack;
    }

    // -- Navigation ---------------------------------------------------------

    /// Step backward one state in the history.
    ///
    /// The current state is discarded (it was already in history when it was
    /// the previous step's "current").  Returns the restored state, or `None`
    /// if we're at the beginning of history.
    pub fn step_backward(&mut self) -> Option<DebugState> {
        if let Some(prev) = self.history.pop() {
            self.current_state = Some(prev.clone());
            Some(prev)
        } else {
            None
        }
    }

    // -- Command dispatch ---------------------------------------------------

    /// Execute a debug command and return the response.
    ///
    /// Note: `StepForward` and `Continue` require the host to actually advance
    /// the VM and call `record_step`.  This method handles the bookkeeping
    /// commands (breakpoints, inspect, stack, quit).
    pub fn execute(&mut self, cmd: DebugCommand) -> DebugResponse {
        match cmd {
            DebugCommand::StepBackward => match self.step_backward() {
                Some(state) => DebugResponse::Stepped(state),
                None => DebugResponse::HistoryStart,
            },
            DebugCommand::SetBreakpoint(bp) => {
                let id = self.add_breakpoint(bp);
                DebugResponse::BreakpointSet(id)
            }
            DebugCommand::RemoveBreakpoint(id) => {
                self.remove_breakpoint(id);
                DebugResponse::BreakpointRemoved(id)
            }
            DebugCommand::ToggleBreakpoint(id) => {
                let new_state = self.toggle_breakpoint(id).unwrap_or(false);
                DebugResponse::BreakpointToggled(id, new_state)
            }
            DebugCommand::Inspect(name) => {
                let value = self
                    .current_state
                    .as_ref()
                    .and_then(|s| s.variables.get(&name).cloned());
                DebugResponse::InspectResult { name, value }
            }
            DebugCommand::PrintStack => DebugResponse::StackTrace(self.stack.clone()),
            DebugCommand::ListBreakpoints => DebugResponse::Breakpoints(self.breakpoints()),
            DebugCommand::Quit => {
                self.active = false;
                DebugResponse::Quit
            }
            // StepForward, Continue, ContinueBackward require VM interaction;
            // the host must drive these and call record_step.
            DebugCommand::StepForward | DebugCommand::Continue | DebugCommand::ContinueBackward => {
                // Return current state if available, otherwise Finished.
                match &self.current_state {
                    Some(s) => DebugResponse::Stepped(s.clone()),
                    None => DebugResponse::Finished,
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::snapshot::SerializedValue;

    fn make_state(step: u64, pc: usize, line: Option<usize>) -> DebugState {
        let mut vars = BTreeMap::new();
        vars.insert("x".into(), SerializedValue::Int(step as i64));
        DebugState {
            step,
            ip: InstructionPointer { cell_index: 0, pc },
            stack_depth: 1,
            current_cell: Some("main".into()),
            source_line: line,
            registers: vec![SerializedValue::Int(step as i64)],
            variables: vars,
        }
    }

    // -- StepHistory tests --------------------------------------------------

    #[test]
    fn history_push_and_pop() {
        let mut h = StepHistory::new(5);
        assert!(h.is_empty());
        h.push(make_state(1, 0, Some(1)));
        h.push(make_state(2, 1, Some(2)));
        assert_eq!(h.len(), 2);

        let s = h.pop().unwrap();
        assert_eq!(s.step, 2);
        let s = h.pop().unwrap();
        assert_eq!(s.step, 1);
        assert!(h.pop().is_none());
    }

    #[test]
    fn history_ring_buffer_overflow() {
        let mut h = StepHistory::new(3);
        for i in 1..=5 {
            h.push(make_state(i, i as usize, Some(i as usize)));
        }
        // Capacity is 3, so only 3 entries survive.
        assert_eq!(h.len(), 3);
        // Most recent pop should be step 5
        assert_eq!(h.pop().unwrap().step, 5);
        assert_eq!(h.pop().unwrap().step, 4);
        assert_eq!(h.pop().unwrap().step, 3);
        assert!(h.pop().is_none());
    }

    #[test]
    fn history_peek() {
        let mut h = StepHistory::new(10);
        assert!(h.peek().is_none());
        h.push(make_state(1, 0, None));
        h.push(make_state(2, 1, None));
        assert_eq!(h.peek().unwrap().step, 2);
        assert_eq!(h.len(), 2); // peek doesn't remove
    }

    #[test]
    fn history_clear() {
        let mut h = StepHistory::new(5);
        h.push(make_state(1, 0, None));
        h.push(make_state(2, 1, None));
        h.clear();
        assert!(h.is_empty());
        assert_eq!(h.len(), 0);
        assert!(h.pop().is_none());
    }

    #[test]
    fn history_iter_order() {
        let mut h = StepHistory::new(4);
        h.push(make_state(10, 0, None));
        h.push(make_state(20, 1, None));
        h.push(make_state(30, 2, None));
        let steps: Vec<u64> = h.iter().map(|s| s.step).collect();
        assert_eq!(steps, vec![10, 20, 30]);
    }

    #[test]
    fn history_iter_after_overflow() {
        let mut h = StepHistory::new(3);
        for i in 1..=5 {
            h.push(make_state(i, 0, None));
        }
        // Should contain 3, 4, 5 in order
        let steps: Vec<u64> = h.iter().map(|s| s.step).collect();
        assert_eq!(steps, vec![3, 4, 5]);
    }

    #[test]
    fn history_capacity() {
        let h = StepHistory::new(42);
        assert_eq!(h.capacity(), 42);
    }

    // -- Breakpoint tests ---------------------------------------------------

    #[test]
    fn breakpoint_enable_disable() {
        let mut bp = Breakpoint::Line {
            id: 0,
            file: "test.lm".into(),
            line: 10,
            enabled: true,
        };
        assert!(bp.is_enabled());
        bp.set_enabled(false);
        assert!(!bp.is_enabled());
        bp.set_enabled(true);
        assert!(bp.is_enabled());
    }

    #[test]
    fn breakpoint_event_variant() {
        let bp = Breakpoint::Event {
            id: 1,
            event_kind: "ToolResponse".into(),
            enabled: true,
        };
        assert_eq!(bp.id(), 1);
        assert!(bp.is_enabled());
    }

    #[test]
    fn breakpoint_cell_entry_variant() {
        let bp = Breakpoint::CellEntry {
            id: 2,
            cell_name: "process_data".into(),
            enabled: false,
        };
        assert_eq!(bp.id(), 2);
        assert!(!bp.is_enabled());
    }

    // -- DebugSession tests -------------------------------------------------

    #[test]
    fn session_add_and_remove_breakpoint() {
        let mut session = DebugSession::new(100);
        let id = session.add_breakpoint(Breakpoint::Line {
            id: 0,
            file: "main.lm".into(),
            line: 5,
            enabled: true,
        });
        assert!(session.get_breakpoint(id).is_some());
        assert!(session.remove_breakpoint(id));
        assert!(session.get_breakpoint(id).is_none());
        // Removing again returns false
        assert!(!session.remove_breakpoint(id));
    }

    #[test]
    fn session_toggle_breakpoint() {
        let mut session = DebugSession::new(100);
        let id = session.add_breakpoint(Breakpoint::Line {
            id: 0,
            file: "".into(),
            line: 10,
            enabled: true,
        });
        // Initially enabled, toggle should return false
        let new_state = session.toggle_breakpoint(id).unwrap();
        assert!(!new_state);
        // Toggle again
        let new_state = session.toggle_breakpoint(id).unwrap();
        assert!(new_state);
        // Non-existent ID
        assert!(session.toggle_breakpoint(999).is_none());
    }

    #[test]
    fn session_record_and_step_backward() {
        let mut session = DebugSession::new(100);
        session.record_step(make_state(1, 0, Some(1)));
        session.record_step(make_state(2, 1, Some(2)));
        session.record_step(make_state(3, 2, Some(3)));

        assert_eq!(session.total_steps(), 3);
        assert_eq!(session.current_state().unwrap().step, 3);

        // Step backward to state 2
        let prev = session.step_backward().unwrap();
        assert_eq!(prev.step, 2);
        assert_eq!(session.current_state().unwrap().step, 2);

        // Step backward to state 1
        let prev = session.step_backward().unwrap();
        assert_eq!(prev.step, 1);

        // No more history
        assert!(session.step_backward().is_none());
    }

    #[test]
    fn session_check_line_breakpoint() {
        let mut session = DebugSession::new(10);
        let bp_id = session.add_breakpoint(Breakpoint::Line {
            id: 0,
            file: "".into(),
            line: 5,
            enabled: true,
        });

        let state = make_state(1, 0, Some(5));
        assert_eq!(session.check_breakpoints(&state), Some(bp_id));

        let state_no_hit = make_state(2, 0, Some(10));
        assert!(session.check_breakpoints(&state_no_hit).is_none());
    }

    #[test]
    fn session_check_cell_entry_breakpoint() {
        let mut session = DebugSession::new(10);
        let bp_id = session.add_breakpoint(Breakpoint::CellEntry {
            id: 0,
            cell_name: "main".into(),
            enabled: true,
        });

        // pc == 0 means cell entry
        let state = make_state(1, 0, None);
        assert_eq!(session.check_breakpoints(&state), Some(bp_id));

        // pc != 0, shouldn't hit
        let state_mid = make_state(2, 5, None);
        assert!(session.check_breakpoints(&state_mid).is_none());
    }

    #[test]
    fn session_check_event_breakpoint() {
        let mut session = DebugSession::new(10);
        let bp_id = session.add_breakpoint(Breakpoint::Event {
            id: 0,
            event_kind: "ToolResponse".into(),
            enabled: true,
        });

        assert_eq!(session.check_event_breakpoint("ToolResponse"), Some(bp_id));
        assert!(session.check_event_breakpoint("Timestamp").is_none());
    }

    #[test]
    fn session_disabled_breakpoint_does_not_fire() {
        let mut session = DebugSession::new(10);
        let bp_id = session.add_breakpoint(Breakpoint::Line {
            id: 0,
            file: "".into(),
            line: 5,
            enabled: false,
        });
        let state = make_state(1, 0, Some(5));
        assert!(session.check_breakpoints(&state).is_none());
        // The breakpoint exists but is disabled
        assert!(session.get_breakpoint(bp_id).is_some());
    }

    #[test]
    fn session_execute_inspect() {
        let mut session = DebugSession::new(10);
        session.record_step(make_state(1, 0, None));

        match session.execute(DebugCommand::Inspect("x".into())) {
            DebugResponse::InspectResult { name, value } => {
                assert_eq!(name, "x");
                assert_eq!(value, Some(SerializedValue::Int(1)));
            }
            other => panic!("expected InspectResult, got {:?}", other),
        }

        // Non-existent variable
        match session.execute(DebugCommand::Inspect("y".into())) {
            DebugResponse::InspectResult { name, value } => {
                assert_eq!(name, "y");
                assert!(value.is_none());
            }
            other => panic!("expected InspectResult, got {:?}", other),
        }
    }

    #[test]
    fn session_execute_print_stack() {
        let mut session = DebugSession::new(10);
        let entry = StackEntry {
            depth: 0,
            cell_name: Some("main".into()),
            ip: InstructionPointer {
                cell_index: 0,
                pc: 3,
            },
            source_line: Some(10),
        };
        session.update_stack(vec![entry.clone()]);

        match session.execute(DebugCommand::PrintStack) {
            DebugResponse::StackTrace(frames) => {
                assert_eq!(frames.len(), 1);
                assert_eq!(frames[0], entry);
            }
            other => panic!("expected StackTrace, got {:?}", other),
        }
    }

    #[test]
    fn session_execute_quit() {
        let mut session = DebugSession::new(10);
        assert!(session.is_active());
        let resp = session.execute(DebugCommand::Quit);
        assert!(matches!(resp, DebugResponse::Quit));
        assert!(!session.is_active());
    }

    #[test]
    fn session_execute_step_backward_empty() {
        let mut session = DebugSession::new(10);
        let resp = session.execute(DebugCommand::StepBackward);
        assert!(matches!(resp, DebugResponse::HistoryStart));
    }

    #[test]
    fn session_list_breakpoints_sorted() {
        let mut session = DebugSession::new(10);
        session.add_breakpoint(Breakpoint::Line {
            id: 0,
            file: "a.lm".into(),
            line: 10,
            enabled: true,
        });
        session.add_breakpoint(Breakpoint::Event {
            id: 0,
            event_kind: "Random".into(),
            enabled: true,
        });
        session.add_breakpoint(Breakpoint::CellEntry {
            id: 0,
            cell_name: "foo".into(),
            enabled: true,
        });

        match session.execute(DebugCommand::ListBreakpoints) {
            DebugResponse::Breakpoints(bps) => {
                assert_eq!(bps.len(), 3);
                // IDs should be monotonically increasing
                assert!(bps[0].id() < bps[1].id());
                assert!(bps[1].id() < bps[2].id());
            }
            other => panic!("expected Breakpoints, got {:?}", other),
        }
    }

    #[test]
    fn session_execute_set_and_remove_breakpoint() {
        let mut session = DebugSession::new(10);

        let resp = session.execute(DebugCommand::SetBreakpoint(Breakpoint::Line {
            id: 0,
            file: "f.lm".into(),
            line: 1,
            enabled: true,
        }));
        let bp_id = match resp {
            DebugResponse::BreakpointSet(id) => id,
            other => panic!("expected BreakpointSet, got {:?}", other),
        };

        let resp = session.execute(DebugCommand::RemoveBreakpoint(bp_id));
        assert!(matches!(resp, DebugResponse::BreakpointRemoved(id) if id == bp_id));
    }

    #[test]
    fn session_history_respects_capacity() {
        let mut session = DebugSession::new(3);
        for i in 1..=10 {
            session.record_step(make_state(i, 0, None));
        }
        // Current is step 10, history has 3 entries (7, 8, 9)
        assert_eq!(session.current_state().unwrap().step, 10);
        assert_eq!(session.history().len(), 3);

        let s = session.step_backward().unwrap();
        assert_eq!(s.step, 9);
        let s = session.step_backward().unwrap();
        assert_eq!(s.step, 8);
        let s = session.step_backward().unwrap();
        assert_eq!(s.step, 7);
        assert!(session.step_backward().is_none());
    }
}
