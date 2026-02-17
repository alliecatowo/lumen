//! Supervisor for managed process groups.
//!
//! A [`Supervisor`] monitors a set of child processes and automatically
//! restarts them according to a configurable [`RestartStrategy`] and
//! per-child [`RestartPolicy`].
//!
//! The design follows the Erlang/OTP supervisor model:
//!
//! | Strategy     | On failure …                                         |
//! |-------------|------------------------------------------------------|
//! | OneForOne   | Restart only the failed child.                       |
//! | OneForAll   | Restart every child in the group.                    |
//! | RestForOne  | Restart the failed child and all children added after it. |
//!
//! Restart frequency is throttled: if more than `max_restarts` occur within
//! `max_seconds`, the supervisor itself reports an error (the caller can
//! decide whether to escalate).
//!
//! # Current status
//!
//! This module provides the data structures and restart logic.  Actual
//! integration with the scheduler (spawning OS-thread or green-thread work)
//! will be wired up in a subsequent phase once the VM task model is finalised.

use std::fmt;
use std::time::Instant;

/// Actions returned by [`Supervisor::handle_exit`]: a list of (child id, work closure) pairs.
pub type RestartActions = Vec<(ChildId, Box<dyn FnOnce() + Send + 'static>)>;

// ---------------------------------------------------------------------------
// Restart strategy
// ---------------------------------------------------------------------------

/// Determines which children are restarted when one child fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartStrategy {
    /// Restart only the failed child.
    OneForOne,
    /// Restart all children in the group.
    OneForAll,
    /// Restart the failed child and every child that was added after it.
    RestForOne,
}

// ---------------------------------------------------------------------------
// Restart policy (per child)
// ---------------------------------------------------------------------------

/// Per-child restart policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartPolicy {
    /// Always restart this child, regardless of exit reason.
    Permanent,
    /// Restart only on abnormal exit (i.e. error / crash).
    Transient,
    /// Never restart — the child is expected to run once.
    Temporary,
}

// ---------------------------------------------------------------------------
// Exit reason
// ---------------------------------------------------------------------------

/// Why a child process exited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitReason {
    /// Normal, successful completion.
    Normal,
    /// The child encountered an error.
    Error(String),
    /// The child was explicitly killed / shut down.
    Killed,
}

impl ExitReason {
    /// Returns `true` for non-normal exits.
    pub fn is_abnormal(&self) -> bool {
        !matches!(self, ExitReason::Normal)
    }
}

impl fmt::Display for ExitReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExitReason::Normal => write!(f, "normal"),
            ExitReason::Error(msg) => write!(f, "error: {}", msg),
            ExitReason::Killed => write!(f, "killed"),
        }
    }
}

// ---------------------------------------------------------------------------
// ChildSpec
// ---------------------------------------------------------------------------

/// Opaque child ID (index into the children vector).
pub type ChildId = usize;

/// Specification for a supervised child.
///
/// The `start_fn` is a factory that produces a new instance of the child's
/// work closure each time it needs to be (re)started.
pub struct ChildSpec {
    /// Human-readable name for logging and diagnostics.
    pub name: String,
    /// Factory that creates the child's work closure.
    ///
    /// Must be `Fn` (not `FnOnce`) because it may be called multiple times
    /// when the supervisor restarts the child.
    start_fn: Box<dyn Fn() -> Box<dyn FnOnce() + Send + 'static> + Send + 'static>,
    /// Per-child restart policy.
    pub restart_policy: RestartPolicy,
}

impl ChildSpec {
    /// Create a new child specification.
    pub fn new<F, G>(name: impl Into<String>, restart_policy: RestartPolicy, start_fn: F) -> Self
    where
        F: Fn() -> G + Send + 'static,
        G: FnOnce() + Send + 'static,
    {
        Self {
            name: name.into(),
            start_fn: Box::new(move || Box::new(start_fn())),
            restart_policy,
        }
    }

    /// Invoke the start factory to create a fresh work closure.
    fn make_work(&self) -> Box<dyn FnOnce() + Send + 'static> {
        (self.start_fn)()
    }
}

impl fmt::Debug for ChildSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChildSpec")
            .field("name", &self.name)
            .field("restart_policy", &self.restart_policy)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// ChildState
// ---------------------------------------------------------------------------

/// Runtime state for a supervised child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildState {
    /// The child has been started (or restarted) and is presumed running.
    Running,
    /// The child has stopped (exited) and has not been restarted.
    Stopped,
}

// ---------------------------------------------------------------------------
// Supervisor
// ---------------------------------------------------------------------------

/// Supervisor manages a group of child processes, restarting them according
/// to the configured [`RestartStrategy`].
pub struct Supervisor {
    /// Which children to restart when one fails.
    strategy: RestartStrategy,
    /// Child specifications (in insertion order).
    children: Vec<ChildSpec>,
    /// Per-child runtime state (parallel to `children`).
    states: Vec<ChildState>,
    /// Maximum restarts allowed within the time window.
    max_restarts: u32,
    /// Time window (in seconds) for restart counting.
    max_seconds: u32,
    /// Timestamps of recent restarts (for throttle calculation).
    restart_timestamps: Vec<Instant>,
    /// Count of `start_all` calls and restarts performed.
    start_count: usize,
}

/// Errors that can occur during supervisor operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorError {
    /// The restart frequency threshold has been exceeded.
    MaxRestartsExceeded { restarts: u32, window_seconds: u32 },
    /// The specified child ID is out of bounds.
    InvalidChildId(ChildId),
}

impl fmt::Display for SupervisorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SupervisorError::MaxRestartsExceeded {
                restarts,
                window_seconds,
            } => {
                write!(
                    f,
                    "max restarts exceeded: {} restarts in {} seconds",
                    restarts, window_seconds
                )
            }
            SupervisorError::InvalidChildId(id) => {
                write!(f, "invalid child id: {}", id)
            }
        }
    }
}

impl std::error::Error for SupervisorError {}

impl Supervisor {
    /// Create a new supervisor with the given strategy.
    ///
    /// Defaults: `max_restarts = 3`, `max_seconds = 5`.
    pub fn new(strategy: RestartStrategy) -> Self {
        Self {
            strategy,
            children: Vec::new(),
            states: Vec::new(),
            max_restarts: 3,
            max_seconds: 5,
            restart_timestamps: Vec::new(),
            start_count: 0,
        }
    }

    /// Set the maximum number of restarts within the time window.
    pub fn max_restarts(mut self, n: u32) -> Self {
        self.max_restarts = n;
        self
    }

    /// Set the time window (in seconds) for restart counting.
    pub fn max_seconds(mut self, s: u32) -> Self {
        self.max_seconds = s;
        self
    }

    /// Add a child specification. Returns the child's index (ID).
    pub fn add_child(&mut self, spec: ChildSpec) -> ChildId {
        let id = self.children.len();
        self.children.push(spec);
        self.states.push(ChildState::Stopped);
        id
    }

    /// Return the number of children.
    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    /// Return the restart strategy.
    pub fn strategy(&self) -> RestartStrategy {
        self.strategy
    }

    /// Return the state of a child.
    pub fn child_state(&self, id: ChildId) -> Option<ChildState> {
        self.states.get(id).copied()
    }

    /// Return the total number of start/restart invocations.
    pub fn start_count(&self) -> usize {
        self.start_count
    }

    // -- lifecycle --------------------------------------------------------

    /// Start all children in order.
    ///
    /// Returns a `Vec` of the work closures produced by each child's start
    /// factory. The caller is responsible for actually executing them (e.g.
    /// by submitting to the scheduler).
    pub fn start_all(&mut self) -> Vec<Box<dyn FnOnce() + Send + 'static>> {
        let mut work = Vec::with_capacity(self.children.len());
        for (i, spec) in self.children.iter().enumerate() {
            work.push(spec.make_work());
            self.states[i] = ChildState::Running;
            self.start_count += 1;
        }
        work
    }

    /// Handle the exit of a child process.
    ///
    /// Returns `Ok(restarts)` with a list of `(ChildId, work_closure)` pairs
    /// for children that should be restarted, or `Err` if the restart
    /// frequency threshold has been exceeded.
    pub fn handle_exit(
        &mut self,
        child_id: ChildId,
        reason: ExitReason,
    ) -> Result<RestartActions, SupervisorError> {
        if child_id >= self.children.len() {
            return Err(SupervisorError::InvalidChildId(child_id));
        }

        // Mark the child as stopped.
        self.states[child_id] = ChildState::Stopped;

        // Determine whether this child should be restarted.
        let should_restart = match self.children[child_id].restart_policy {
            RestartPolicy::Permanent => true,
            RestartPolicy::Transient => reason.is_abnormal(),
            RestartPolicy::Temporary => false,
        };

        if !should_restart {
            return Ok(Vec::new());
        }

        // Check restart frequency throttle.
        self.record_restart()?;

        // Determine which children to restart based on strategy.
        let restart_ids: Vec<ChildId> = match self.strategy {
            RestartStrategy::OneForOne => vec![child_id],
            RestartStrategy::OneForAll => (0..self.children.len()).collect(),
            RestartStrategy::RestForOne => (child_id..self.children.len()).collect(),
        };

        // For OneForAll and RestForOne, stop the affected children first.
        for &id in &restart_ids {
            self.states[id] = ChildState::Stopped;
        }

        // Restart the affected children.
        let mut restarts = Vec::with_capacity(restart_ids.len());
        for &id in &restart_ids {
            let work = self.children[id].make_work();
            self.states[id] = ChildState::Running;
            self.start_count += 1;
            restarts.push((id, work));
        }

        Ok(restarts)
    }

    // -- restart throttle -------------------------------------------------

    /// Record a restart event and check whether the throttle is exceeded.
    fn record_restart(&mut self) -> Result<(), SupervisorError> {
        let now = Instant::now();

        // Prune timestamps outside the window.
        let window = std::time::Duration::from_secs(self.max_seconds as u64);
        self.restart_timestamps
            .retain(|&t| now.duration_since(t) < window);

        // Check before recording.
        if self.restart_timestamps.len() as u32 >= self.max_restarts {
            return Err(SupervisorError::MaxRestartsExceeded {
                restarts: self.max_restarts,
                window_seconds: self.max_seconds,
            });
        }

        self.restart_timestamps.push(now);
        Ok(())
    }
}

impl fmt::Debug for Supervisor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Supervisor")
            .field("strategy", &self.strategy)
            .field("child_count", &self.children.len())
            .field("states", &self.states)
            .field("max_restarts", &self.max_restarts)
            .field("max_seconds", &self.max_seconds)
            .field("start_count", &self.start_count)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Helper: create a ChildSpec whose start_fn increments a counter.
    fn counting_child(name: &str, policy: RestartPolicy, counter: Arc<AtomicUsize>) -> ChildSpec {
        ChildSpec::new(name, policy, move || {
            let ctr = Arc::clone(&counter);
            move || {
                ctr.fetch_add(1, Ordering::Relaxed);
            }
        })
    }

    // -- basic lifecycle --------------------------------------------------

    #[test]
    fn start_all_produces_work_closures() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut sup = Supervisor::new(RestartStrategy::OneForOne);
        sup.add_child(counting_child(
            "a",
            RestartPolicy::Permanent,
            Arc::clone(&counter),
        ));
        sup.add_child(counting_child(
            "b",
            RestartPolicy::Permanent,
            Arc::clone(&counter),
        ));

        let work = sup.start_all();
        assert_eq!(work.len(), 2);

        // Execute the closures.
        for w in work {
            w();
        }
        assert_eq!(counter.load(Ordering::Relaxed), 2);

        // All children should be Running.
        assert_eq!(sup.child_state(0), Some(ChildState::Running));
        assert_eq!(sup.child_state(1), Some(ChildState::Running));
        assert_eq!(sup.start_count(), 2);
    }

    // -- OneForOne --------------------------------------------------------

    #[test]
    fn one_for_one_restarts_only_failed() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut sup = Supervisor::new(RestartStrategy::OneForOne);
        sup.add_child(counting_child(
            "a",
            RestartPolicy::Permanent,
            Arc::clone(&counter),
        ));
        sup.add_child(counting_child(
            "b",
            RestartPolicy::Permanent,
            Arc::clone(&counter),
        ));
        let _ = sup.start_all();

        // Child 0 fails.
        let restarts = sup
            .handle_exit(0, ExitReason::Error("crash".into()))
            .unwrap();
        assert_eq!(restarts.len(), 1);
        assert_eq!(restarts[0].0, 0);

        // Execute the restart closure.
        (restarts.into_iter().next().unwrap().1)();
        // start_all produced 2, handle_exit produced 1 restart.
        assert_eq!(sup.start_count(), 3);

        // Child 1 should still be Running (untouched).
        assert_eq!(sup.child_state(1), Some(ChildState::Running));
    }

    // -- OneForAll --------------------------------------------------------

    #[test]
    fn one_for_all_restarts_everyone() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut sup = Supervisor::new(RestartStrategy::OneForAll);
        sup.add_child(counting_child(
            "a",
            RestartPolicy::Permanent,
            Arc::clone(&counter),
        ));
        sup.add_child(counting_child(
            "b",
            RestartPolicy::Permanent,
            Arc::clone(&counter),
        ));
        sup.add_child(counting_child(
            "c",
            RestartPolicy::Permanent,
            Arc::clone(&counter),
        ));
        let _ = sup.start_all();

        let restarts = sup
            .handle_exit(1, ExitReason::Error("crash".into()))
            .unwrap();
        assert_eq!(restarts.len(), 3);
        let ids: Vec<ChildId> = restarts.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![0, 1, 2]);
    }

    // -- RestForOne -------------------------------------------------------

    #[test]
    fn rest_for_one_restarts_failed_and_later() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut sup = Supervisor::new(RestartStrategy::RestForOne);
        sup.add_child(counting_child(
            "a",
            RestartPolicy::Permanent,
            Arc::clone(&counter),
        ));
        sup.add_child(counting_child(
            "b",
            RestartPolicy::Permanent,
            Arc::clone(&counter),
        ));
        sup.add_child(counting_child(
            "c",
            RestartPolicy::Permanent,
            Arc::clone(&counter),
        ));
        let _ = sup.start_all();

        // Child 1 fails → restart 1 and 2, but NOT 0.
        let restarts = sup
            .handle_exit(1, ExitReason::Error("crash".into()))
            .unwrap();
        assert_eq!(restarts.len(), 2);
        let ids: Vec<ChildId> = restarts.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![1, 2]);

        // Child 0 untouched.
        assert_eq!(sup.child_state(0), Some(ChildState::Running));
    }

    // -- Restart policies -------------------------------------------------

    #[test]
    fn transient_only_restarts_on_abnormal() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut sup = Supervisor::new(RestartStrategy::OneForOne);
        sup.add_child(counting_child(
            "t",
            RestartPolicy::Transient,
            Arc::clone(&counter),
        ));
        let _ = sup.start_all();

        // Normal exit → no restart.
        let restarts = sup.handle_exit(0, ExitReason::Normal).unwrap();
        assert!(restarts.is_empty());
        assert_eq!(sup.child_state(0), Some(ChildState::Stopped));
    }

    #[test]
    fn transient_restarts_on_error() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut sup = Supervisor::new(RestartStrategy::OneForOne);
        sup.add_child(counting_child(
            "t",
            RestartPolicy::Transient,
            Arc::clone(&counter),
        ));
        let _ = sup.start_all();

        // Error exit → restart.
        let restarts = sup
            .handle_exit(0, ExitReason::Error("oops".into()))
            .unwrap();
        assert_eq!(restarts.len(), 1);
    }

    #[test]
    fn temporary_never_restarts() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut sup = Supervisor::new(RestartStrategy::OneForOne);
        sup.add_child(counting_child(
            "tmp",
            RestartPolicy::Temporary,
            Arc::clone(&counter),
        ));
        let _ = sup.start_all();

        // Even on error — no restart.
        let restarts = sup
            .handle_exit(0, ExitReason::Error("boom".into()))
            .unwrap();
        assert!(restarts.is_empty());
    }

    // -- Throttle ---------------------------------------------------------

    #[test]
    fn max_restarts_throttle() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut sup = Supervisor::new(RestartStrategy::OneForOne)
            .max_restarts(2)
            .max_seconds(60);
        sup.add_child(counting_child(
            "p",
            RestartPolicy::Permanent,
            Arc::clone(&counter),
        ));
        let _ = sup.start_all();

        // First two restarts should succeed.
        let r1 = sup.handle_exit(0, ExitReason::Error("1".into()));
        assert!(r1.is_ok());
        let r2 = sup.handle_exit(0, ExitReason::Error("2".into()));
        assert!(r2.is_ok());

        // Third restart within the window should fail.
        let r3 = sup.handle_exit(0, ExitReason::Error("3".into()));
        assert!(r3.is_err());
        match r3 {
            Err(SupervisorError::MaxRestartsExceeded { restarts, .. }) => {
                assert_eq!(restarts, 2);
            }
            Err(other) => panic!("unexpected error: {:?}", other),
            Ok(_) => panic!("expected error but got Ok"),
        }
    }

    // -- Edge cases -------------------------------------------------------

    #[test]
    fn invalid_child_id() {
        let mut sup = Supervisor::new(RestartStrategy::OneForOne);
        let result = sup.handle_exit(42, ExitReason::Normal);
        assert!(result.is_err());
        match result {
            Err(SupervisorError::InvalidChildId(42)) => {}
            Err(other) => panic!("unexpected error: {:?}", other),
            Ok(_) => panic!("expected error but got Ok"),
        }
    }

    #[test]
    fn empty_supervisor() {
        let mut sup = Supervisor::new(RestartStrategy::OneForAll);
        let work = sup.start_all();
        assert!(work.is_empty());
        assert_eq!(sup.child_count(), 0);
    }

    #[test]
    fn exit_reason_display() {
        assert_eq!(ExitReason::Normal.to_string(), "normal");
        assert_eq!(ExitReason::Error("oops".into()).to_string(), "error: oops");
        assert_eq!(ExitReason::Killed.to_string(), "killed");
    }

    #[test]
    fn exit_reason_is_abnormal() {
        assert!(!ExitReason::Normal.is_abnormal());
        assert!(ExitReason::Error("x".into()).is_abnormal());
        assert!(ExitReason::Killed.is_abnormal());
    }

    #[test]
    fn supervisor_debug_format() {
        let mut sup = Supervisor::new(RestartStrategy::OneForOne);
        let counter = Arc::new(AtomicUsize::new(0));
        sup.add_child(counting_child("x", RestartPolicy::Permanent, counter));
        let dbg = format!("{:?}", sup);
        assert!(dbg.contains("Supervisor"));
        assert!(dbg.contains("OneForOne"));
        assert!(dbg.contains("child_count: 1"));
    }

    #[test]
    fn child_spec_debug_format() {
        let counter = Arc::new(AtomicUsize::new(0));
        let spec = counting_child("my-worker", RestartPolicy::Transient, counter);
        let dbg = format!("{:?}", spec);
        assert!(dbg.contains("my-worker"));
        assert!(dbg.contains("Transient"));
    }
}
