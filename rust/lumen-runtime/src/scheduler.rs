//! M:N work-stealing scheduler for the Lumen runtime.
//!
//! The scheduler maintains a pool of OS worker threads, each with a local
//! work-stealing deque. New tasks enter via a global injection queue and are
//! distributed to workers. When a worker's local queue is empty it attempts
//! to steal from peers before falling back to the global queue.
//!
//! # Work-stealing algorithm
//!
//! Each worker thread runs a loop with the following priority:
//! 1. Pop from local FIFO deque (cheapest — no contention).
//! 2. Steal a batch from the global [`Injector`] queue into the local deque.
//! 3. Steal from a random peer worker's [`Stealer`].
//! 4. Park briefly (1 ms) to avoid busy-spinning, then retry.
//!
//! Task completion is tracked via a shared [`AtomicUsize`] counter so callers
//! can wait for a known number of tasks to finish.

use crate::process::{ProcessControlBlock, ProcessId, ProcessStatus};
use crossbeam_deque::{Injector, Steal, Stealer, Worker};
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Task
// ---------------------------------------------------------------------------

/// A schedulable unit of work.
///
/// Each task is associated with a [`ProcessId`] (so it can be correlated back
/// to the owning PCB) and carries a boxed closure that the worker will invoke.
/// The closure signature `FnOnce()` is intentionally simple — the real
/// execution path will call into the VM dispatch loop via an opaque handle.
pub struct Task {
    /// The process that owns this task.
    pub process_id: ProcessId,
    /// The work to execute. `Option` so we can `.take()` to run it exactly once.
    work: Option<Box<dyn FnOnce() + Send + 'static>>,
}

impl Task {
    /// Create a new task for the given process.
    pub fn new<F>(process_id: ProcessId, f: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        Self {
            process_id,
            work: Some(Box::new(f)),
        }
    }

    /// Execute the task's work closure, consuming it.
    ///
    /// Returns `true` if the closure was present and executed, `false` if the
    /// task had already been consumed.
    pub fn run(&mut self) -> bool {
        if let Some(f) = self.work.take() {
            f();
            true
        } else {
            false
        }
    }
}

impl fmt::Debug for Task {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Task")
            .field("process_id", &self.process_id)
            .field("has_work", &self.work.is_some())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// WorkerHandle (per-thread bookkeeping exposed to the Scheduler)
// ---------------------------------------------------------------------------

/// Per-worker metadata visible to the [`Scheduler`].
///
/// Each OS thread owns a [`Worker`] deque (push/pop from the owning thread)
/// and publishes a [`Stealer`] that peers can use to steal tasks.
struct WorkerHandle {
    /// A stealer handle that other workers can use.
    ///
    /// Retained here so the scheduler can expose per-worker diagnostics in
    /// the future (e.g. queue depth). The actual work-stealing uses the
    /// `Arc<Vec<Stealer>>` shared across all threads.
    _stealer: Stealer<Task>,
    /// The join handle for the OS thread.
    join_handle: Option<thread::JoinHandle<()>>,
}

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

/// An M:N work-stealing task scheduler.
///
/// The scheduler owns a pool of worker threads and a global injection queue.
/// Tasks are spawned into the global queue and picked up by workers.
pub struct Scheduler {
    /// Global injection queue — new tasks land here.
    global_queue: Arc<Injector<Task>>,
    /// Per-worker metadata (stealers + join handles).
    workers: Vec<WorkerHandle>,
    /// Signal used to request graceful shutdown.
    shutdown: Arc<AtomicBool>,
    /// Number of worker threads.
    worker_count: usize,
    /// Number of tasks that have been completed across all workers.
    completed_count: Arc<AtomicUsize>,
    /// Registry of spawned process control blocks, keyed by [`ProcessId`].
    process_registry: Arc<Mutex<HashMap<ProcessId, Arc<ProcessControlBlock>>>>,
}

impl Scheduler {
    /// Create a new scheduler with `num_workers` OS threads.
    ///
    /// Passing `0` will default to the number of available CPUs.
    pub fn new(num_workers: usize) -> Self {
        let num_workers = if num_workers == 0 {
            num_cpus::get().max(1)
        } else {
            num_workers
        };

        let global_queue = Arc::new(Injector::<Task>::new());
        let shutdown = Arc::new(AtomicBool::new(false));
        let completed_count = Arc::new(AtomicUsize::new(0));

        // Phase 1: create all Worker deques and collect stealers.
        let mut local_workers: Vec<Worker<Task>> = Vec::with_capacity(num_workers);
        let mut stealers: Vec<Stealer<Task>> = Vec::with_capacity(num_workers);

        for _ in 0..num_workers {
            let w = Worker::new_fifo();
            stealers.push(w.stealer());
            local_workers.push(w);
        }

        // Wrap stealers in an Arc so every thread can access all of them.
        let stealers = Arc::new(stealers);

        // Phase 2: spawn OS threads.
        let mut handles: Vec<WorkerHandle> = Vec::with_capacity(num_workers);

        for (idx, local) in local_workers.into_iter().enumerate() {
            let global = Arc::clone(&global_queue);
            let shutdown_flag = Arc::clone(&shutdown);
            let peer_stealers = Arc::clone(&stealers);
            let completed = Arc::clone(&completed_count);

            let jh = thread::Builder::new()
                .name(format!("lumen-worker-{}", idx))
                .spawn(move || {
                    Self::worker_loop(idx, local, global, peer_stealers, shutdown_flag, completed);
                })
                .expect("failed to spawn worker thread");

            handles.push(WorkerHandle {
                _stealer: stealers[idx].clone(),
                join_handle: Some(jh),
            });
        }

        Self {
            global_queue,
            workers: handles,
            shutdown,
            worker_count: num_workers,
            completed_count,
            process_registry: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Return the number of worker threads.
    pub fn worker_count(&self) -> usize {
        self.worker_count
    }

    /// Return the number of tasks completed so far.
    pub fn completed_count(&self) -> usize {
        self.completed_count.load(Ordering::Acquire)
    }

    /// Spawn a new task onto the global injection queue.
    pub fn spawn(&self, task: Task) {
        self.global_queue.push(task);
    }

    /// Convenience: wrap a closure in a [`Task`] with a fresh [`ProcessId`]
    /// and push it to the global queue.
    ///
    /// This is useful when you don't need to correlate the task back to an
    /// existing process control block.
    pub fn spawn_fn<F: FnOnce() + Send + 'static>(&self, f: F) {
        let pid = ProcessId::next();
        self.global_queue.push(Task::new(pid, f));
    }

    /// Spawn a new process with a [`ProcessControlBlock`].
    ///
    /// Creates a PCB with the given priority and optional name, wraps the
    /// closure so that the process status is updated on completion, registers
    /// the PCB in the process registry, and pushes the task to the global
    /// injection queue. Returns the [`ProcessId`] of the new process.
    pub fn spawn_process<F>(&self, priority: u8, name: Option<String>, work: F) -> ProcessId
    where
        F: FnOnce() + Send + 'static,
    {
        let pcb = Arc::new(ProcessControlBlock::new(priority, name));
        let pid = pcb.id();

        // Register the PCB before pushing the task so lookups are valid
        // immediately after this call returns.
        self.process_registry
            .lock()
            .unwrap()
            .insert(pid, Arc::clone(&pcb));

        // Wrap the user's closure so we transition the PCB status.
        let pcb_inner = Arc::clone(&pcb);
        let task = Task::new(pid, move || {
            pcb_inner.set_status(ProcessStatus::Running);
            work();
            pcb_inner.set_status(ProcessStatus::Completed);
        });

        self.global_queue.push(task);
        pid
    }

    /// Look up a process by its [`ProcessId`].
    pub fn get_process(&self, pid: ProcessId) -> Option<Arc<ProcessControlBlock>> {
        self.process_registry.lock().unwrap().get(&pid).cloned()
    }

    /// Return the number of processes in the registry.
    pub fn process_count(&self) -> usize {
        self.process_registry.lock().unwrap().len()
    }

    /// Block until at least `expected` tasks have completed, or `timeout`
    /// elapses.
    ///
    /// Returns the actual completed count at the time the wait ended.
    /// If the completed count is less than `expected`, the timeout was reached.
    pub fn wait_for_completion(&self, expected: usize, timeout: Duration) -> usize {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let current = self.completed_count.load(Ordering::Acquire);
            if current >= expected {
                return current;
            }
            if std::time::Instant::now() >= deadline {
                return current;
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    /// Request a graceful shutdown and wait for all workers to finish.
    ///
    /// Any tasks still in queues when workers notice the shutdown signal will
    /// be abandoned (not executed).
    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        for w in &mut self.workers {
            if let Some(jh) = w.join_handle.take() {
                let _ = jh.join();
            }
        }
    }

    /// Return `true` if shutdown has been requested.
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }

    // -- internal worker loop ---------------------------------------------

    /// Simple deterministic pseudo-random number generator (xorshift32).
    ///
    /// We avoid pulling in the `rand` crate for this single use case.
    /// Each worker has its own state so there is no contention.
    fn xorshift32(state: &mut u32) -> u32 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *state = x;
        x
    }

    /// The main loop executed by each worker thread.
    ///
    /// Priority order:
    /// 1. Pop from the local deque (cheapest).
    /// 2. Steal a batch from the global injection queue.
    /// 3. Steal from a random peer worker.
    /// 4. Park briefly (1 ms) to avoid busy-spinning.
    fn worker_loop(
        idx: usize,
        local: Worker<Task>,
        global: Arc<Injector<Task>>,
        stealers: Arc<Vec<Stealer<Task>>>,
        shutdown: Arc<AtomicBool>,
        completed: Arc<AtomicUsize>,
    ) {
        // Seed the per-worker PRNG. Avoid zero (xorshift32 fixpoint).
        let mut rng_state: u32 = (idx as u32).wrapping_mul(2654435761).max(1);

        loop {
            if shutdown.load(Ordering::Acquire) {
                return;
            }

            // 1. Try local deque.
            if let Some(mut task) = local.pop() {
                task.run();
                completed.fetch_add(1, Ordering::Release);
                continue;
            }

            // 2. Try global queue (steal a batch into local).
            match global.steal_batch_and_pop(&local) {
                Steal::Success(mut task) => {
                    task.run();
                    completed.fetch_add(1, Ordering::Release);
                    continue;
                }
                Steal::Retry => {
                    // Contention — try again next iteration.
                    thread::yield_now();
                    continue;
                }
                Steal::Empty => {}
            }

            // 3. Try stealing from a random peer.
            let num_peers = stealers.len();
            if num_peers > 0 {
                let start = Self::xorshift32(&mut rng_state) as usize % num_peers;
                let mut stolen = false;
                for offset in 0..num_peers {
                    let peer_idx = (start + offset) % num_peers;
                    // Skip our own stealer — stealing from ourselves is a no-op
                    // on FIFO deques (and the Worker handle is on this thread).
                    if peer_idx == idx {
                        continue;
                    }
                    match stealers[peer_idx].steal_batch_and_pop(&local) {
                        Steal::Success(mut task) => {
                            task.run();
                            completed.fetch_add(1, Ordering::Release);
                            stolen = true;
                            break;
                        }
                        Steal::Retry => {
                            // Will try other peers or next loop iteration.
                        }
                        Steal::Empty => {}
                    }
                }
                if stolen {
                    continue;
                }
            }

            // 4. Nothing to do — brief sleep to avoid busy-spinning.
            //    A production scheduler would use a condition variable /
            //    eventfd here, but this is adequate for the current phase.
            thread::park_timeout(Duration::from_millis(1));
        }
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        if !self.is_shutdown() {
            self.shutdown();
        }
    }
}

impl fmt::Debug for Scheduler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scheduler")
            .field("worker_count", &self.worker_count)
            .field("completed_count", &self.completed_count())
            .field("shutdown", &self.is_shutdown())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    #[test]
    fn scheduler_creates_requested_workers() {
        let mut sched = Scheduler::new(2);
        assert_eq!(sched.worker_count(), 2);
        sched.shutdown();
    }

    #[test]
    fn scheduler_default_workers_nonzero() {
        let mut sched = Scheduler::new(0);
        assert!(sched.worker_count() >= 1);
        sched.shutdown();
    }

    #[test]
    fn scheduler_spawn_and_execute() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut sched = Scheduler::new(2);

        let n = 100;
        for _ in 0..n {
            let ctr = Arc::clone(&counter);
            let pid = ProcessId::next();
            sched.spawn(Task::new(pid, move || {
                ctr.fetch_add(1, Ordering::Relaxed);
            }));
        }

        // Give workers time to drain the queue.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while counter.load(Ordering::Relaxed) < n {
            if std::time::Instant::now() > deadline {
                break;
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }

        sched.shutdown();
        assert_eq!(counter.load(Ordering::Relaxed), n);
    }

    #[test]
    fn scheduler_shutdown_is_idempotent() {
        let mut sched = Scheduler::new(1);
        sched.shutdown();
        assert!(sched.is_shutdown());
        // Second call should not panic.
        sched.shutdown();
        assert!(sched.is_shutdown());
    }

    #[test]
    fn task_run_consumes_work() {
        let flag = Arc::new(AtomicBool::new(false));
        let flag2 = Arc::clone(&flag);
        let pid = ProcessId::next();
        let mut task = Task::new(pid, move || {
            flag2.store(true, Ordering::Relaxed);
        });

        assert!(task.run());
        assert!(flag.load(Ordering::Relaxed));
        // Second run returns false — already consumed.
        assert!(!task.run());
    }

    #[test]
    fn task_debug_format() {
        let pid = ProcessId::next();
        let task = Task::new(pid, || {});
        let dbg = format!("{:?}", task);
        assert!(dbg.contains("Task"));
        assert!(dbg.contains("has_work: true"));
    }

    #[test]
    fn scheduler_debug_format() {
        let mut sched = Scheduler::new(1);
        let dbg = format!("{:?}", sched);
        assert!(dbg.contains("Scheduler"));
        assert!(dbg.contains("worker_count: 1"));
        sched.shutdown();
    }

    #[test]
    fn work_stealing_distributes_tasks() {
        // Spawn a large number of tasks and verify they all complete, proving
        // that work-stealing (or at minimum the global queue) distributes work.
        let counter = Arc::new(AtomicUsize::new(0));
        let mut sched = Scheduler::new(4);

        let n = 1_000;
        for _ in 0..n {
            let ctr = Arc::clone(&counter);
            let pid = ProcessId::next();
            sched.spawn(Task::new(pid, move || {
                ctr.fetch_add(1, Ordering::Relaxed);
            }));
        }

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while counter.load(Ordering::Relaxed) < n {
            if std::time::Instant::now() > deadline {
                break;
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }

        sched.shutdown();
        assert_eq!(counter.load(Ordering::Relaxed), n);
    }

    #[test]
    fn spawn_fn_executes_closures() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut sched = Scheduler::new(2);

        let n = 100usize;
        for _ in 0..n {
            let ctr = Arc::clone(&counter);
            sched.spawn_fn(move || {
                ctr.fetch_add(1, Ordering::Relaxed);
            });
        }

        let completed = sched.wait_for_completion(n, Duration::from_secs(5));
        sched.shutdown();
        assert_eq!(completed, n);
        assert_eq!(counter.load(Ordering::Relaxed), n);
    }

    #[test]
    fn completed_count_tracks_tasks() {
        let mut sched = Scheduler::new(2);

        let n = 50usize;
        for _ in 0..n {
            sched.spawn_fn(|| {
                // no-op task
            });
        }

        let completed = sched.wait_for_completion(n, Duration::from_secs(5));
        assert_eq!(completed, n);

        sched.shutdown();
        // After shutdown, completed_count should still be accessible.
        assert!(sched.completed_count() >= n);
    }

    #[test]
    fn wait_for_completion_returns_on_timeout() {
        let mut sched = Scheduler::new(1);

        // Don't spawn any tasks — wait should time out.
        let completed = sched.wait_for_completion(100, Duration::from_millis(50));
        assert_eq!(completed, 0);

        sched.shutdown();
    }

    #[test]
    fn work_stealing_1000_tasks_4_workers() {
        // Verify all 1000 tasks complete with 4-worker work-stealing.
        let mut sched = Scheduler::new(4);

        let counter = Arc::new(AtomicUsize::new(0));
        let n = 1_000usize;
        for _ in 0..n {
            let ctr = Arc::clone(&counter);
            sched.spawn_fn(move || {
                ctr.fetch_add(1, Ordering::Relaxed);
            });
        }

        let completed = sched.wait_for_completion(n, Duration::from_secs(10));
        sched.shutdown();
        assert_eq!(completed, n);
        assert_eq!(counter.load(Ordering::Relaxed), n);
    }

    // -- T058: spawn_process with PCB tracking ----------------------------

    #[test]
    fn spawn_process_creates_pcb_and_executes() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut sched = Scheduler::new(2);

        let ctr = Arc::clone(&counter);
        let pid = sched.spawn_process(10, Some("worker".into()), move || {
            ctr.fetch_add(1, Ordering::Relaxed);
        });

        // PCB should be registered immediately.
        let pcb = sched.get_process(pid).expect("PCB should be registered");
        assert_eq!(pcb.priority(), 10);
        assert_eq!(pcb.name(), Some("worker"));

        // Wait for execution.
        let _ = sched.wait_for_completion(1, Duration::from_secs(5));
        sched.shutdown();

        assert_eq!(counter.load(Ordering::Relaxed), 1);
        assert_eq!(pcb.status(), ProcessStatus::Completed);
    }

    #[test]
    fn spawn_process_tracks_multiple_processes() {
        let mut sched = Scheduler::new(2);

        let n = 20usize;
        let mut pids = Vec::new();
        for i in 0..n {
            let pid = sched.spawn_process(128, Some(format!("proc-{}", i)), || {});
            pids.push(pid);
        }

        assert_eq!(sched.process_count(), n);

        // All PIDs should be unique.
        let unique: std::collections::HashSet<_> = pids.iter().copied().collect();
        assert_eq!(unique.len(), n);

        let _ = sched.wait_for_completion(n, Duration::from_secs(5));
        sched.shutdown();
    }

    #[test]
    fn spawn_process_status_transitions_to_completed() {
        let mut sched = Scheduler::new(1);

        let pid = sched.spawn_process(0, None, || {
            // simulate a tiny bit of work
            std::thread::yield_now();
        });

        let pcb = sched.get_process(pid).unwrap();
        // Status should be Ready before execution.
        assert_eq!(pcb.status(), ProcessStatus::Ready);

        let _ = sched.wait_for_completion(1, Duration::from_secs(5));
        sched.shutdown();

        assert_eq!(pcb.status(), ProcessStatus::Completed);
    }

    #[test]
    fn get_process_returns_none_for_unknown_pid() {
        let mut sched = Scheduler::new(1);
        let fake_pid = ProcessId::next();
        assert!(sched.get_process(fake_pid).is_none());
        sched.shutdown();
    }
}
