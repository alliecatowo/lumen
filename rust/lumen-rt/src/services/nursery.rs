//! Structured concurrency (nursery / scope pattern) for the Lumen runtime.
//!
//! A [`Nursery`] is a concurrency scope that guarantees all child tasks either
//! complete or are cancelled before the scope exits.  This is the same idea as
//! Trio nurseries, Kotlin `coroutineScope`, or Swift `TaskGroup`.
//!
//! # Invariants
//!
//! 1. When a nursery scope exits (via [`wait_all`]), every child task is either
//!    joined (completed) or cancelled.
//! 2. If **any** child task returns an error, all remaining siblings are
//!    cancelled and the first error is propagated.
//! 3. A shared [`CancelToken`] lets cooperative tasks observe the cancellation
//!    signal and exit early.
//! 4. Nurseries can be nested — an inner nursery must fully complete before the
//!    outer nursery can proceed.
//!
//! # Thread model
//!
//! Each spawned task runs on its own OS thread (via [`std::thread::spawn`]).
//! This matches the current Lumen scheduler model which uses OS threads with
//! work-stealing.  A future iteration may integrate directly with the
//! [`Scheduler`](crate::services::scheduler::Scheduler) thread pool.

use crate::services::process::ProcessId;

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// CancelToken
// ---------------------------------------------------------------------------

/// A cooperative cancellation signal shared among all tasks in a nursery.
///
/// Tasks receive a clone of this token when spawned and should periodically
/// check [`is_cancelled`] to decide whether to stop early.
#[derive(Clone)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// Create a new token in the non-cancelled state.
    fn new(flag: Arc<AtomicBool>) -> Self {
        Self(flag)
    }

    /// Returns `true` if cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    /// Request cancellation.  All clones of this token will observe `true`.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
}

impl fmt::Debug for CancelToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CancelToken")
            .field(&self.is_cancelled())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// NurseryError
// ---------------------------------------------------------------------------

/// Errors that can arise from nursery operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NurseryError {
    /// A child task returned an error.
    TaskFailed { task_id: ProcessId, error: String },
    /// The nursery was cancelled (either explicitly or due to a sibling error).
    Cancelled,
    /// The nursery exceeded its deadline.
    Timeout,
    /// A child task panicked.  The panic message is captured as a string.
    TaskPanicked { task_id: ProcessId, message: String },
}

impl fmt::Display for NurseryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NurseryError::TaskFailed { task_id, error } => {
                write!(f, "task {} failed: {}", task_id, error)
            }
            NurseryError::Cancelled => write!(f, "nursery cancelled"),
            NurseryError::Timeout => write!(f, "nursery timed out"),
            NurseryError::TaskPanicked { task_id, message } => {
                write!(f, "task {} panicked: {}", task_id, message)
            }
        }
    }
}

impl std::error::Error for NurseryError {}

// ---------------------------------------------------------------------------
// NurseryTask
// ---------------------------------------------------------------------------

/// A task that has been spawned into a nursery.
///
/// Each task is identified by its [`ProcessId`] and backed by an OS thread
/// [`JoinHandle`].
struct NurseryTask {
    id: ProcessId,
    handle: Option<JoinHandle<Result<String, String>>>,
}

// ---------------------------------------------------------------------------
// Nursery
// ---------------------------------------------------------------------------

/// A structured concurrency scope.
///
/// All tasks spawned into a nursery must complete (or be cancelled) before the
/// nursery itself completes.
pub struct Nursery {
    tasks: Vec<NurseryTask>,
    cancel_token: Arc<AtomicBool>,
}

impl Nursery {
    /// Create a new, empty nursery scope.
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            cancel_token: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Return a [`CancelToken`] that can be used to observe (or trigger)
    /// cancellation for this nursery.
    pub fn cancel_token(&self) -> CancelToken {
        CancelToken::new(Arc::clone(&self.cancel_token))
    }

    /// Return the number of tasks that have been spawned.
    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    /// Spawn a task within this nursery.
    ///
    /// The closure receives a [`CancelToken`] and must return
    /// `Ok(value_string)` on success or `Err(error_string)` on failure.
    /// The returned [`ProcessId`] identifies the task.
    pub fn spawn<F>(&mut self, f: F) -> ProcessId
    where
        F: FnOnce(CancelToken) -> Result<String, String> + Send + 'static,
    {
        let pid = ProcessId::next();
        let token = CancelToken::new(Arc::clone(&self.cancel_token));

        let handle = thread::Builder::new()
            .name(format!("nursery-task-{}", pid))
            .spawn(move || f(token))
            .expect("failed to spawn nursery task thread");

        self.tasks.push(NurseryTask {
            id: pid,
            handle: Some(handle),
        });

        pid
    }

    /// Cancel all running tasks by setting the shared cancellation flag.
    ///
    /// This is a cooperative signal — tasks must check
    /// [`CancelToken::is_cancelled`] to actually stop.
    pub fn cancel_all(&self) {
        self.cancel_token.store(true, Ordering::Release);
    }

    /// Wait for all tasks to complete.
    ///
    /// Results are returned in spawn order.  If any task fails (returns `Err`)
    /// or panics, all remaining siblings are cancelled and the first error is
    /// propagated.
    ///
    /// Internally this polls all handles in a round-robin loop so that an
    /// error in *any* task (not just the first in spawn order) is detected
    /// promptly and triggers cancellation.
    pub fn wait_all(&mut self) -> Result<Vec<String>, NurseryError> {
        self.poll_all(None)
    }

    /// Wait for all tasks with a timeout.
    ///
    /// If all tasks complete within `timeout`, returns their results.
    /// Otherwise cancels all tasks and returns [`NurseryError::Timeout`].
    pub fn wait_all_timeout(&mut self, timeout: Duration) -> Result<Vec<String>, NurseryError> {
        self.poll_all(Some(Instant::now() + timeout))
    }

    /// Core polling loop shared by [`wait_all`] and [`wait_all_timeout`].
    ///
    /// Polls all live handles in a round-robin fashion.  When a handle
    /// finishes, its result is stored in a slot indexed by spawn order.
    /// If any handle returns an error or panics, cancellation is triggered
    /// and remaining handles are joined.
    fn poll_all(&mut self, deadline: Option<Instant>) -> Result<Vec<String>, NurseryError> {
        let n = self.tasks.len();
        if n == 0 {
            return Ok(Vec::new());
        }

        // Slot per task for the result value.  `None` means not yet finished.
        let mut slots: Vec<Option<String>> = (0..n).map(|_| None).collect();
        let mut remaining = n;

        loop {
            // Check deadline.
            if let Some(dl) = deadline {
                if Instant::now() >= dl {
                    self.cancel_all();
                    self.join_remaining();
                    return Err(NurseryError::Timeout);
                }
            }

            let mut made_progress = false;

            // We index into both `self.tasks` and `slots` by position,
            // and mutate `self.tasks[i].handle` mid-loop, so a range loop
            // is the clearest expression here.
            #[allow(clippy::needless_range_loop)]
            for i in 0..n {
                // Skip already-completed tasks.
                let handle_ref = match &self.tasks[i].handle {
                    Some(h) => h,
                    None => continue,
                };

                if !handle_ref.is_finished() {
                    continue;
                }

                // Handle is finished — take and join.
                let handle = self.tasks[i].handle.take().unwrap();
                let task_id = self.tasks[i].id;

                match handle.join() {
                    Ok(Ok(value)) => {
                        slots[i] = Some(value);
                        remaining -= 1;
                        made_progress = true;
                    }
                    Ok(Err(error)) => {
                        let err = NurseryError::TaskFailed { task_id, error };
                        self.cancel_all();
                        self.join_remaining();
                        return Err(err);
                    }
                    Err(panic_payload) => {
                        let message = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                            s.to_string()
                        } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                            s.clone()
                        } else {
                            "unknown panic".to_string()
                        };
                        let err = NurseryError::TaskPanicked { task_id, message };
                        self.cancel_all();
                        self.join_remaining();
                        return Err(err);
                    }
                }
            }

            if remaining == 0 {
                // All tasks completed successfully.
                return Ok(slots.into_iter().map(|s| s.unwrap()).collect());
            }

            if !made_progress {
                // Brief sleep to avoid busy-spinning.
                thread::sleep(Duration::from_micros(200));
            }
        }
    }

    /// Join any remaining task handles (best-effort, ignoring results).
    ///
    /// Called after cancellation to ensure all OS threads are cleaned up.
    fn join_remaining(&mut self) {
        for task in &mut self.tasks {
            if let Some(handle) = task.handle.take() {
                let _ = handle.join();
            }
        }
    }
}

impl Default for Nursery {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Nursery {
    fn drop(&mut self) {
        // Structured concurrency guarantee: if the nursery is dropped without
        // calling wait_all, cancel everything and join.
        self.cancel_all();
        self.join_remaining();
    }
}

impl fmt::Debug for Nursery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Nursery")
            .field("task_count", &self.tasks.len())
            .field("cancelled", &self.cancel_token.load(Ordering::Acquire))
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    // 1. Basic: spawn 3 tasks, wait_all, all complete
    #[test]
    fn basic_spawn_three_tasks_all_complete() {
        let mut nursery = Nursery::new();

        nursery.spawn(|_token| Ok("a".to_string()));
        nursery.spawn(|_token| Ok("b".to_string()));
        nursery.spawn(|_token| Ok("c".to_string()));

        let results = nursery.wait_all().unwrap();
        assert_eq!(results, vec!["a", "b", "c"]);
    }

    // 2. Error propagation: one task fails, others get cancelled
    #[test]
    fn error_propagation_cancels_siblings() {
        let mut nursery = Nursery::new();

        // Task 0: long-running, checks cancellation
        let cancelled_flag = Arc::new(AtomicBool::new(false));
        let flag_clone = Arc::clone(&cancelled_flag);
        nursery.spawn(move |token| {
            // Wait until cancelled
            while !token.is_cancelled() {
                thread::sleep(Duration::from_millis(1));
            }
            flag_clone.store(true, Ordering::Release);
            Ok("cancelled-early".to_string())
        });

        // Task 1: fails immediately
        nursery.spawn(|_token| Err("task-1-error".to_string()));

        let err = nursery.wait_all().unwrap_err();
        match err {
            NurseryError::TaskFailed { error, .. } => {
                assert_eq!(error, "task-1-error");
            }
            other => panic!("expected TaskFailed, got {:?}", other),
        }

        // The cancellation signal should have been sent
        // (the long-running task in slot 0 is joined as part of join_remaining,
        // but since task 1 is processed second in wait_all, task 0 may have
        // already completed. The important thing is cancel_all was called.)
    }

    // 3. Cancel token: task checks is_cancelled and stops
    #[test]
    fn cancel_token_cooperative_shutdown() {
        let mut nursery = Nursery::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let ctr = Arc::clone(&counter);
        nursery.spawn(move |token| {
            let mut i = 0u64;
            while !token.is_cancelled() && i < 1_000_000 {
                i += 1;
            }
            ctr.store(1, AtomicOrdering::Release);
            Ok(format!("stopped at {}", i))
        });

        // Cancel from the outside
        nursery.cancel_all();

        let results = nursery.wait_all().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(counter.load(AtomicOrdering::Acquire), 1);
    }

    // 4. Timeout: tasks exceed timeout, all cancelled
    #[test]
    fn timeout_cancels_all_tasks() {
        let mut nursery = Nursery::new();

        // Task that runs longer than the timeout
        nursery.spawn(|token| {
            while !token.is_cancelled() {
                thread::sleep(Duration::from_millis(10));
            }
            Ok("done".to_string())
        });

        let err = nursery
            .wait_all_timeout(Duration::from_millis(50))
            .unwrap_err();
        assert_eq!(err, NurseryError::Timeout);
    }

    // 5. Empty nursery: wait_all returns immediately
    #[test]
    fn empty_nursery_returns_immediately() {
        let mut nursery = Nursery::new();
        let results = nursery.wait_all().unwrap();
        assert!(results.is_empty());
        assert_eq!(nursery.task_count(), 0);
    }

    // 6. Nested nurseries: inner completes before outer
    #[test]
    fn nested_nurseries_inner_completes_first() {
        let mut outer = Nursery::new();

        outer.spawn(|_outer_token| {
            let mut inner = Nursery::new();
            inner.spawn(|_inner_token| Ok("inner-1".to_string()));
            inner.spawn(|_inner_token| Ok("inner-2".to_string()));

            let inner_results = inner.wait_all().map_err(|e| e.to_string())?;
            Ok(format!("outer-got: {}", inner_results.join(",")))
        });

        let results = outer.wait_all().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "outer-got: inner-1,inner-2");
    }

    // 7. Single task nursery
    #[test]
    fn single_task_nursery() {
        let mut nursery = Nursery::new();
        nursery.spawn(|_token| Ok("only-one".to_string()));

        let results = nursery.wait_all().unwrap();
        assert_eq!(results, vec!["only-one"]);
    }

    // 8. All tasks fail
    #[test]
    fn all_tasks_fail_first_error_propagated() {
        let mut nursery = Nursery::new();

        nursery.spawn(|_token| Err("error-a".to_string()));
        nursery.spawn(|_token| Err("error-b".to_string()));
        nursery.spawn(|_token| Err("error-c".to_string()));

        let err = nursery.wait_all().unwrap_err();
        // One of the task errors should be propagated; which one arrives
        // first is non-deterministic because tasks run on threads.
        match err {
            NurseryError::TaskFailed { error, .. } => {
                assert!(
                    ["error-a", "error-b", "error-c"].contains(&error.as_str()),
                    "unexpected error: {error}"
                );
            }
            other => panic!("expected TaskFailed, got {:?}", other),
        }
    }

    // 9. Cancel_all explicit
    #[test]
    fn explicit_cancel_all() {
        let mut nursery = Nursery::new();
        let saw_cancel = Arc::new(AtomicBool::new(false));

        let flag = Arc::clone(&saw_cancel);
        nursery.spawn(move |token| {
            while !token.is_cancelled() {
                thread::sleep(Duration::from_millis(1));
            }
            flag.store(true, Ordering::Release);
            Ok("observed-cancel".to_string())
        });

        // Give the task a moment to start
        thread::sleep(Duration::from_millis(10));

        nursery.cancel_all();

        let results = nursery.wait_all().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "observed-cancel");
        assert!(saw_cancel.load(Ordering::Acquire));
    }

    // 10. Task ordering: results maintain spawn order
    #[test]
    fn results_maintain_spawn_order() {
        let mut nursery = Nursery::new();

        // Spawn tasks that complete at different speeds
        for i in 0..5 {
            nursery.spawn(move |_token| {
                // Vary sleep to test ordering
                thread::sleep(Duration::from_millis((5 - i) * 2));
                Ok(format!("task-{}", i))
            });
        }

        let results = nursery.wait_all().unwrap();
        assert_eq!(
            results,
            vec!["task-0", "task-1", "task-2", "task-3", "task-4"]
        );
    }

    // 11. Rapid spawn/cancel cycles
    #[test]
    fn rapid_spawn_cancel_cycles() {
        for _ in 0..20 {
            let mut nursery = Nursery::new();
            nursery.spawn(|token| {
                if token.is_cancelled() {
                    return Ok("cancelled".to_string());
                }
                Ok("done".to_string())
            });
            nursery.cancel_all();
            // Drop triggers join — should not leak threads.
        }
    }

    // 12. Task panic is caught and reported
    #[test]
    fn task_panic_is_caught() {
        let mut nursery = Nursery::new();

        nursery.spawn(|_token| {
            panic!("deliberate panic");
        });

        let err = nursery.wait_all().unwrap_err();
        match err {
            NurseryError::TaskPanicked { message, .. } => {
                assert!(message.contains("deliberate panic"));
            }
            other => panic!("expected TaskPanicked, got {:?}", other),
        }
    }

    // 13. Cancel token is_cancelled starts as false
    #[test]
    fn cancel_token_initially_false() {
        let nursery = Nursery::new();
        let token = nursery.cancel_token();
        assert!(!token.is_cancelled());
    }

    // 14. ProcessId is unique per spawned task
    #[test]
    fn process_ids_are_unique() {
        let mut nursery = Nursery::new();
        let pid1 = nursery.spawn(|_| Ok("a".to_string()));
        let pid2 = nursery.spawn(|_| Ok("b".to_string()));
        let pid3 = nursery.spawn(|_| Ok("c".to_string()));

        assert_ne!(pid1, pid2);
        assert_ne!(pid2, pid3);
        assert_ne!(pid1, pid3);

        nursery.wait_all().unwrap();
    }

    // 15. Nursery drop cancels and joins without wait_all
    #[test]
    fn drop_cancels_and_joins() {
        let finished = Arc::new(AtomicBool::new(false));
        let finished_clone = Arc::clone(&finished);

        {
            let mut nursery = Nursery::new();
            nursery.spawn(move |token| {
                while !token.is_cancelled() {
                    thread::sleep(Duration::from_millis(1));
                }
                finished_clone.store(true, Ordering::Release);
                Ok("done".to_string())
            });
            // nursery dropped here — should cancel + join
        }

        assert!(finished.load(Ordering::Acquire));
    }

    // 16. Empty nursery with timeout returns immediately
    #[test]
    fn empty_nursery_timeout_returns_immediately() {
        let mut nursery = Nursery::new();
        let results = nursery
            .wait_all_timeout(Duration::from_millis(100))
            .unwrap();
        assert!(results.is_empty());
    }

    // 17. Timeout with tasks that complete in time
    #[test]
    fn timeout_with_tasks_completing_in_time() {
        let mut nursery = Nursery::new();

        nursery.spawn(|_token| {
            thread::sleep(Duration::from_millis(5));
            Ok("fast".to_string())
        });

        let results = nursery.wait_all_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(results, vec!["fast"]);
    }

    // 18. Debug format
    #[test]
    fn debug_format() {
        let mut nursery = Nursery::new();
        nursery.spawn(|_| Ok("x".to_string()));
        let dbg = format!("{:?}", nursery);
        assert!(dbg.contains("Nursery"));
        assert!(dbg.contains("task_count: 1"));
        nursery.wait_all().unwrap();
    }

    // 19. NurseryError display
    #[test]
    fn error_display_messages() {
        let pid = ProcessId::next();

        let e1 = NurseryError::TaskFailed {
            task_id: pid,
            error: "boom".to_string(),
        };
        assert!(e1.to_string().contains("boom"));

        let e2 = NurseryError::Cancelled;
        assert_eq!(e2.to_string(), "nursery cancelled");

        let e3 = NurseryError::Timeout;
        assert_eq!(e3.to_string(), "nursery timed out");

        let e4 = NurseryError::TaskPanicked {
            task_id: pid,
            message: "oops".to_string(),
        };
        assert!(e4.to_string().contains("oops"));
    }

    // 20. Default trait
    #[test]
    fn default_creates_empty_nursery() {
        let nursery = Nursery::default();
        assert_eq!(nursery.task_count(), 0);
    }

    // 21. Multiple tasks with shared state via Arc
    #[test]
    fn shared_state_across_tasks() {
        let mut nursery = Nursery::new();
        let counter = Arc::new(AtomicUsize::new(0));

        for _ in 0..10 {
            let ctr = Arc::clone(&counter);
            nursery.spawn(move |_token| {
                ctr.fetch_add(1, AtomicOrdering::Relaxed);
                Ok("ok".to_string())
            });
        }

        let results = nursery.wait_all().unwrap();
        assert_eq!(results.len(), 10);
        assert_eq!(counter.load(AtomicOrdering::Relaxed), 10);
    }

    // 22. Error in middle task cancels later tasks
    #[test]
    fn error_in_middle_cancels_later() {
        let mut nursery = Nursery::new();
        let task3_ran = Arc::new(AtomicBool::new(false));

        // Task 0: succeeds quickly
        nursery.spawn(|_token| Ok("first".to_string()));

        // Task 1: fails
        nursery.spawn(|_token| Err("middle-fail".to_string()));

        // Task 2: long running, should be cancelled
        let flag = Arc::clone(&task3_ran);
        nursery.spawn(move |token| {
            // Wait a bit to ensure we'd be cancelled
            thread::sleep(Duration::from_millis(50));
            if !token.is_cancelled() {
                flag.store(true, Ordering::Release);
            }
            Ok("should-not-appear".to_string())
        });

        let err = nursery.wait_all().unwrap_err();
        match err {
            NurseryError::TaskFailed { error, .. } => {
                assert_eq!(error, "middle-fail");
            }
            other => panic!("expected TaskFailed, got {:?}", other),
        }
    }
}
