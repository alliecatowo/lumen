//! Synchronous single-threaded scheduler for deterministic stepping.
//!
//! The [`SyncScheduler`] provides the same logical model as the threaded
//! [`Scheduler`](crate::scheduler::Scheduler) — a global injection queue,
//! per-worker local queues, and work-stealing — but executes everything on the
//! calling thread. This makes it suitable for:
//!
//! * **VM integration**: the VM dispatch loop can call [`tick()`] once per
//!   scheduling quantum, keeping execution deterministic and single-threaded.
//! * **Testing**: behaviour is reproducible because there is no thread
//!   interleaving.
//!
//! # Scheduling algorithm (per tick)
//!
//! 1. Drain the global injection queue round-robin into worker local queues.
//! 2. For each worker, pop one task from the local queue and run it.
//!    If the local queue is empty, attempt to steal from a peer.
//! 3. Return a [`TickResult`] indicating whether work was performed.

use crate::injection::InjectionQueue;
use crate::process::{ProcessControlBlock, ProcessId, ProcessStatus};
use crate::scheduler::Task;

use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// TickResult
// ---------------------------------------------------------------------------

/// Outcome of a single [`SyncScheduler::tick`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickResult {
    /// At least one task was executed during this tick.
    Progress,
    /// No tasks were available — all queues were empty.
    Idle,
}

// ---------------------------------------------------------------------------
// SpawnEntry — what sits in the injection queue
// ---------------------------------------------------------------------------

/// A spawned process waiting to be assigned to a worker.
///
/// Pairs a [`ProcessControlBlock`] with a [`Task`] closure so the scheduler
/// can both track the process metadata and execute the work.
struct SpawnEntry {
    pcb: Arc<ProcessControlBlock>,
    task: Task,
}

// ---------------------------------------------------------------------------
// SyncScheduler
// ---------------------------------------------------------------------------

/// A synchronous, single-threaded work-stealing scheduler.
///
/// All execution happens on the calling thread — no OS worker threads are
/// spawned. This provides a deterministic integration point for the VM.
pub struct SyncScheduler {
    /// Number of logical workers.
    num_workers: usize,
    /// Per-worker local task queues.
    local_queues: Vec<VecDeque<Task>>,
    /// Global injection queue where `spawn_process` deposits new work.
    injection: InjectionQueue<SpawnEntry>,
    /// Registry of all known PCBs, keyed by insertion order.
    /// The VM can look up a process by its [`ProcessId`] if needed.
    processes: Vec<Arc<ProcessControlBlock>>,
    /// Round-robin index for distributing injected tasks to workers.
    rr_index: usize,
    /// Total number of tasks executed.
    completed_count: usize,
}

impl SyncScheduler {
    /// Create a new synchronous scheduler with `num_workers` logical workers.
    ///
    /// Passing `0` defaults to 1 (at least one worker is required).
    pub fn new(num_workers: usize) -> Self {
        let num_workers = num_workers.max(1);
        let local_queues = (0..num_workers).map(|_| VecDeque::new()).collect();
        Self {
            num_workers,
            local_queues,
            injection: InjectionQueue::new(),
            processes: Vec::new(),
            rr_index: 0,
            completed_count: 0,
        }
    }

    /// Return the number of logical workers.
    pub fn num_workers(&self) -> usize {
        self.num_workers
    }

    /// Return the total number of tasks completed.
    pub fn completed_count(&self) -> usize {
        self.completed_count
    }

    /// Return the number of tracked processes.
    pub fn process_count(&self) -> usize {
        self.processes.len()
    }

    /// Look up a process by its [`ProcessId`].
    pub fn get_process(&self, pid: ProcessId) -> Option<Arc<ProcessControlBlock>> {
        self.processes.iter().find(|p| p.id() == pid).cloned()
    }

    /// Return the total number of tasks across all local queues.
    pub fn pending_local_tasks(&self) -> usize {
        self.local_queues.iter().map(|q| q.len()).sum()
    }

    /// Return the number of tasks in the injection queue.
    pub fn pending_injected_tasks(&self) -> usize {
        self.injection.len()
    }

    // -- spawn ------------------------------------------------------------

    /// Spawn a new process into the scheduler.
    ///
    /// Creates a [`ProcessControlBlock`] with the given priority and name,
    /// wraps the provided closure into a [`Task`], and pushes it into the
    /// global injection queue. Returns the [`ProcessId`] of the new process.
    ///
    /// The task will not execute until [`tick()`] or [`run_until_idle()`] is
    /// called.
    pub fn spawn_process<F>(&mut self, priority: u8, name: Option<String>, work: F) -> ProcessId
    where
        F: FnOnce() + Send + 'static,
    {
        let pcb = Arc::new(ProcessControlBlock::new(priority, name));
        let pid = pcb.id();
        let task = Task::new(pid, work);
        self.injection.push(SpawnEntry {
            pcb: Arc::clone(&pcb),
            task,
        });
        self.processes.push(pcb);
        pid
    }

    /// Spawn a process with default priority (128) and no name.
    pub fn spawn_process_fn<F>(&mut self, work: F) -> ProcessId
    where
        F: FnOnce() + Send + 'static,
    {
        self.spawn_process(128, None, work)
    }

    // -- scheduling -------------------------------------------------------

    /// Drain the injection queue into worker local queues using round-robin
    /// distribution.
    fn drain_injection(&mut self) {
        let mut buf = Vec::new();
        self.injection.drain_all(&mut buf);
        for entry in buf {
            // Mark the process as Ready (it already is, but be explicit).
            entry.pcb.set_status(ProcessStatus::Ready).ok();
            let worker_idx = self.rr_index % self.num_workers;
            self.local_queues[worker_idx].push_back(entry.task);
            self.rr_index = self.rr_index.wrapping_add(1);
        }
    }

    /// Attempt to steal a task from a peer worker's queue.
    ///
    /// Tries each peer in order starting from `(worker_idx + 1)`. Steals
    /// half the peer's queue (minimum 1) to amortise the cost.
    fn try_steal(&mut self, worker_idx: usize) -> Option<Task> {
        for offset in 1..self.num_workers {
            let peer = (worker_idx + offset) % self.num_workers;
            let peer_len = self.local_queues[peer].len();
            if peer_len > 0 {
                // Steal half of the peer's tasks (at least 1).
                let steal_count = (peer_len / 2).max(1);
                // Pop from the front of the peer queue.
                let mut stolen: Vec<Task> = Vec::with_capacity(steal_count);
                for _ in 0..steal_count {
                    if let Some(task) = self.local_queues[peer].pop_front() {
                        stolen.push(task);
                    }
                }
                // The first stolen task is returned for immediate execution.
                // The rest go into the worker's local queue.
                let first = if stolen.is_empty() {
                    None
                } else {
                    let first = stolen.remove(0);
                    for t in stolen {
                        self.local_queues[worker_idx].push_back(t);
                    }
                    Some(first)
                };
                if first.is_some() {
                    return first;
                }
            }
        }
        None
    }

    /// Execute one scheduling round.
    ///
    /// 1. Drain the injection queue into worker local queues (round-robin).
    /// 2. For each worker, pop one task and execute it. If the local queue
    ///    is empty, attempt to steal from a peer.
    /// 3. Return [`TickResult::Progress`] if any task ran, otherwise
    ///    [`TickResult::Idle`].
    pub fn tick(&mut self) -> TickResult {
        // Step 1: drain injection queue.
        self.drain_injection();

        let mut did_work = false;

        // Step 2: each worker runs one task.
        for worker_idx in 0..self.num_workers {
            // Try local queue first.
            let task = self.local_queues[worker_idx].pop_front();

            if let Some(mut task) = task {
                // Mark the process as Running.
                if let Some(pcb) = self.processes.iter().find(|p| p.id() == task.process_id) {
                    let _ = pcb.set_status(ProcessStatus::Running);
                }
                task.run();
                // Mark the process as Completed.
                if let Some(pcb) = self.processes.iter().find(|p| p.id() == task.process_id) {
                    let _ = pcb.set_status(ProcessStatus::Completed);
                }
                self.completed_count += 1;
                did_work = true;
            } else {
                // Try stealing from a peer.
                if let Some(mut stolen_task) = self.try_steal(worker_idx) {
                    if let Some(pcb) = self
                        .processes
                        .iter()
                        .find(|p| p.id() == stolen_task.process_id)
                    {
                        let _ = pcb.set_status(ProcessStatus::Running);
                    }
                    stolen_task.run();
                    if let Some(pcb) = self
                        .processes
                        .iter()
                        .find(|p| p.id() == stolen_task.process_id)
                    {
                        let _ = pcb.set_status(ProcessStatus::Completed);
                    }
                    self.completed_count += 1;
                    did_work = true;
                }
            }
        }

        if did_work {
            TickResult::Progress
        } else {
            TickResult::Idle
        }
    }

    /// Run ticks until all queues (injection + local) are empty and no more
    /// work can be done.
    ///
    /// Returns the total number of tasks executed during this call.
    pub fn run_until_idle(&mut self) -> usize {
        let start = self.completed_count;
        loop {
            match self.tick() {
                TickResult::Progress => continue,
                TickResult::Idle => {
                    // Double-check: injection queue might have been populated
                    // by a task that just ran (self-spawning).
                    if self.injection.is_empty() {
                        break;
                    }
                }
            }
        }
        self.completed_count - start
    }
}

impl fmt::Debug for SyncScheduler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncScheduler")
            .field("num_workers", &self.num_workers)
            .field("completed_count", &self.completed_count)
            .field("process_count", &self.processes.len())
            .field("pending_injected", &self.injection.len())
            .field("pending_local", &self.pending_local_tasks())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Integration with the threaded Scheduler
// ---------------------------------------------------------------------------

// The threaded `Scheduler` already has spawn() and spawn_fn(). We add
// `spawn_process()` there via extension in the scheduler module itself
// (see scheduler.rs additions below). This module focuses on the sync path.

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::ProcessStatus;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // -- T057: Injection queue integration --------------------------------

    #[test]
    fn spawn_into_injection_queue_and_drain() {
        let mut sched = SyncScheduler::new(2);
        let counter = Arc::new(AtomicUsize::new(0));

        let ctr = Arc::clone(&counter);
        sched.spawn_process_fn(move || {
            ctr.fetch_add(1, Ordering::Relaxed);
        });

        // Task is in the injection queue, not yet in a local queue.
        assert_eq!(sched.pending_injected_tasks(), 1);
        assert_eq!(sched.pending_local_tasks(), 0);

        // One tick drains injection and runs the task.
        let result = sched.tick();
        assert_eq!(result, TickResult::Progress);
        assert_eq!(counter.load(Ordering::Relaxed), 1);
        assert_eq!(sched.completed_count(), 1);
    }

    #[test]
    fn multiple_spawns_distributed_round_robin() {
        let mut sched = SyncScheduler::new(3);
        let counter = Arc::new(AtomicUsize::new(0));

        // Spawn 6 tasks — should be distributed 2 per worker.
        for _ in 0..6 {
            let ctr = Arc::clone(&counter);
            sched.spawn_process_fn(move || {
                ctr.fetch_add(1, Ordering::Relaxed);
            });
        }

        assert_eq!(sched.pending_injected_tasks(), 6);

        // Drain injection manually by calling tick. First tick drains all 6
        // into local queues and runs one per worker (3 total).
        let result = sched.tick();
        assert_eq!(result, TickResult::Progress);
        // 3 workers each ran 1 task.
        assert_eq!(counter.load(Ordering::Relaxed), 3);

        // Second tick runs the remaining 3.
        let result = sched.tick();
        assert_eq!(result, TickResult::Progress);
        assert_eq!(counter.load(Ordering::Relaxed), 6);

        // Third tick — no work left.
        let result = sched.tick();
        assert_eq!(result, TickResult::Idle);
    }

    #[test]
    fn workers_steal_from_overloaded_peer() {
        let mut sched = SyncScheduler::new(2);
        let counter = Arc::new(AtomicUsize::new(0));

        // Spawn 5 tasks — round-robin puts 3 on worker 0, 2 on worker 1.
        for _ in 0..5 {
            let ctr = Arc::clone(&counter);
            sched.spawn_process_fn(move || {
                ctr.fetch_add(1, Ordering::Relaxed);
            });
        }

        // Run to completion.
        let executed = sched.run_until_idle();
        assert_eq!(executed, 5);
        assert_eq!(counter.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn run_until_idle_terminates_when_empty() {
        let mut sched = SyncScheduler::new(2);

        // No tasks spawned — should return immediately.
        let executed = sched.run_until_idle();
        assert_eq!(executed, 0);
        assert_eq!(sched.completed_count(), 0);
    }

    #[test]
    fn run_until_idle_completes_all_tasks() {
        let mut sched = SyncScheduler::new(4);
        let counter = Arc::new(AtomicUsize::new(0));

        for _ in 0..100 {
            let ctr = Arc::clone(&counter);
            sched.spawn_process_fn(move || {
                ctr.fetch_add(1, Ordering::Relaxed);
            });
        }

        let executed = sched.run_until_idle();
        assert_eq!(executed, 100);
        assert_eq!(counter.load(Ordering::Relaxed), 100);
    }

    #[test]
    fn spawn_process_returns_unique_pids() {
        let mut sched = SyncScheduler::new(1);
        let pid1 = sched.spawn_process_fn(|| {});
        let pid2 = sched.spawn_process_fn(|| {});
        let pid3 = sched.spawn_process_fn(|| {});

        assert_ne!(pid1, pid2);
        assert_ne!(pid2, pid3);
        assert_ne!(pid1, pid3);
    }

    #[test]
    fn process_status_transitions() {
        let mut sched = SyncScheduler::new(1);

        let pid = sched.spawn_process(0, Some("test".into()), || {});

        // Before tick: process should be Ready.
        let pcb = sched.get_process(pid).unwrap();
        assert_eq!(pcb.status().unwrap(), ProcessStatus::Ready);

        // After tick: process should be Completed.
        sched.tick();
        assert_eq!(pcb.status().unwrap(), ProcessStatus::Completed);
    }

    #[test]
    fn spawn_process_with_priority_and_name() {
        let mut sched = SyncScheduler::new(1);

        let pid = sched.spawn_process(42, Some("my-proc".into()), || {});
        let pcb = sched.get_process(pid).unwrap();

        assert_eq!(pcb.priority(), 42);
        assert_eq!(pcb.name(), Some("my-proc"));
    }

    #[test]
    fn process_count_tracks_spawns() {
        let mut sched = SyncScheduler::new(1);
        assert_eq!(sched.process_count(), 0);

        sched.spawn_process_fn(|| {});
        assert_eq!(sched.process_count(), 1);

        sched.spawn_process_fn(|| {});
        assert_eq!(sched.process_count(), 2);
    }

    #[test]
    fn tick_returns_idle_when_no_work() {
        let mut sched = SyncScheduler::new(2);
        assert_eq!(sched.tick(), TickResult::Idle);
        assert_eq!(sched.tick(), TickResult::Idle);
    }

    #[test]
    fn debug_format() {
        let mut sched = SyncScheduler::new(2);
        sched.spawn_process_fn(|| {});
        let dbg = format!("{:?}", sched);
        assert!(dbg.contains("SyncScheduler"));
        assert!(dbg.contains("num_workers: 2"));
        assert!(dbg.contains("process_count: 1"));
    }

    #[test]
    fn single_worker_executes_all_tasks() {
        let mut sched = SyncScheduler::new(1);
        let counter = Arc::new(AtomicUsize::new(0));

        for _ in 0..50 {
            let ctr = Arc::clone(&counter);
            sched.spawn_process_fn(move || {
                ctr.fetch_add(1, Ordering::Relaxed);
            });
        }

        let executed = sched.run_until_idle();
        assert_eq!(executed, 50);
        assert_eq!(counter.load(Ordering::Relaxed), 50);
    }

    #[test]
    fn many_workers_few_tasks() {
        // More workers than tasks — should still work fine.
        let mut sched = SyncScheduler::new(8);
        let counter = Arc::new(AtomicUsize::new(0));

        for _ in 0..3 {
            let ctr = Arc::clone(&counter);
            sched.spawn_process_fn(move || {
                ctr.fetch_add(1, Ordering::Relaxed);
            });
        }

        let executed = sched.run_until_idle();
        assert_eq!(executed, 3);
        assert_eq!(counter.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn work_stealing_with_uneven_distribution() {
        // Spawn tasks, drain to workers, then add more tasks only to worker 0.
        let mut sched = SyncScheduler::new(2);
        let counter = Arc::new(AtomicUsize::new(0));

        // Spawn 10 tasks — 5 per worker after drain.
        for _ in 0..10 {
            let ctr = Arc::clone(&counter);
            sched.spawn_process_fn(move || {
                ctr.fetch_add(1, Ordering::Relaxed);
            });
        }

        // Run one tick to drain injection and execute 1 per worker (2 total).
        sched.tick();
        let completed_after_tick1 = counter.load(Ordering::Relaxed);
        assert_eq!(completed_after_tick1, 2);

        // Continue until idle.
        sched.run_until_idle();
        assert_eq!(counter.load(Ordering::Relaxed), 10);
    }
}
