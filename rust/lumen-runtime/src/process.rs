//! Process Control Block (PCB) for the Lumen runtime scheduler.
//!
//! Each lightweight process (task) in the Lumen runtime is represented by a
//! [`ProcessControlBlock`]. It tracks identity, scheduling state, priority,
//! an optional human-readable name, and a simple mailbox for inter-process
//! messaging.

use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

// ---------------------------------------------------------------------------
// ProcessId
// ---------------------------------------------------------------------------

/// Monotonically increasing counter used to mint unique [`ProcessId`]s.
static NEXT_PROCESS_ID: AtomicU64 = AtomicU64::new(1);

/// A unique, opaque identifier for a runtime process.
///
/// Process IDs are assigned sequentially from a global atomic counter and are
/// guaranteed to be unique for the lifetime of the program.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessId(u64);

impl ProcessId {
    /// Allocate the next unique process ID.
    pub fn next() -> Self {
        Self(NEXT_PROCESS_ID.fetch_add(1, Ordering::Relaxed))
    }

    /// Return the raw numeric value (useful for logging / tracing).
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

impl fmt::Debug for ProcessId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ProcessId({})", self.0)
    }
}

impl fmt::Display for ProcessId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "pid:{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// ProcessStatus
// ---------------------------------------------------------------------------

/// The lifecycle state of a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessStatus {
    /// Eligible to be scheduled on a worker.
    Ready,
    /// Currently executing on a worker thread.
    Running,
    /// Voluntarily yielded or waiting on I/O / a message.
    Suspended,
    /// Finished successfully.
    Completed,
    /// Terminated with an error.
    Failed,
}

impl fmt::Display for ProcessStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProcessStatus::Ready => write!(f, "Ready"),
            ProcessStatus::Running => write!(f, "Running"),
            ProcessStatus::Suspended => write!(f, "Suspended"),
            ProcessStatus::Completed => write!(f, "Completed"),
            ProcessStatus::Failed => write!(f, "Failed"),
        }
    }
}

// ---------------------------------------------------------------------------
// Message
// ---------------------------------------------------------------------------

/// A simple message type for inter-process communication.
///
/// For now this wraps an opaque JSON value; future iterations will use a
/// zero-copy representation aligned with the VM's `Value` type.
#[derive(Debug, Clone)]
pub struct Message {
    pub payload: serde_json::Value,
}

impl Message {
    pub fn new(payload: serde_json::Value) -> Self {
        Self { payload }
    }
}

// ---------------------------------------------------------------------------
// ProcessControlBlock
// ---------------------------------------------------------------------------

/// Internal mutable state protected by a [`Mutex`].
struct ProcessInner {
    status: ProcessStatus,
    mailbox: VecDeque<Message>,
}

/// The Process Control Block tracks identity, scheduling metadata, and a
/// mailbox for a single lightweight process.
///
/// Interior mutability is provided via `std::sync::Mutex` so that the PCB
/// can be shared across worker threads without external synchronization.
pub struct ProcessControlBlock {
    /// Unique process identifier.
    id: ProcessId,
    /// Scheduling priority (0 = highest, 255 = lowest).
    priority: u8,
    /// Optional human-readable label for debugging.
    name: Option<String>,
    /// Timestamp of process creation (monotonic).
    created_at: Instant,
    /// Mutable state guarded by a mutex.
    inner: Mutex<ProcessInner>,
}

impl ProcessControlBlock {
    /// Create a new PCB in the [`Ready`](ProcessStatus::Ready) state.
    ///
    /// `priority` follows the convention 0 = highest priority, 255 = lowest.
    pub fn new(priority: u8, name: Option<String>) -> Self {
        Self {
            id: ProcessId::next(),
            priority,
            name,
            created_at: Instant::now(),
            inner: Mutex::new(ProcessInner {
                status: ProcessStatus::Ready,
                mailbox: VecDeque::new(),
            }),
        }
    }

    /// Return this process's unique ID.
    pub fn id(&self) -> ProcessId {
        self.id
    }

    /// Return the scheduling priority.
    pub fn priority(&self) -> u8 {
        self.priority
    }

    /// Return the optional human-readable name.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Return the creation timestamp.
    pub fn created_at(&self) -> Instant {
        self.created_at
    }

    // -- status -----------------------------------------------------------

    /// Read the current status.
    pub fn status(&self) -> ProcessStatus {
        self.inner.lock().unwrap().status
    }

    /// Transition to a new status.
    pub fn set_status(&self, status: ProcessStatus) {
        self.inner.lock().unwrap().status = status;
    }

    // -- mailbox ----------------------------------------------------------

    /// Enqueue a message into this process's mailbox.
    pub fn send_message(&self, msg: Message) {
        self.inner.lock().unwrap().mailbox.push_back(msg);
    }

    /// Dequeue the oldest message, if any.
    pub fn receive_message(&self) -> Option<Message> {
        self.inner.lock().unwrap().mailbox.pop_front()
    }

    /// Number of pending messages in the mailbox.
    pub fn mailbox_len(&self) -> usize {
        self.inner.lock().unwrap().mailbox.len()
    }
}

impl fmt::Debug for ProcessControlBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.inner.lock().unwrap();
        f.debug_struct("ProcessControlBlock")
            .field("id", &self.id)
            .field("status", &inner.status)
            .field("priority", &self.priority)
            .field("name", &self.name)
            .field("mailbox_len", &inner.mailbox.len())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn process_ids_are_unique() {
        let a = ProcessId::next();
        let b = ProcessId::next();
        let c = ProcessId::next();
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert!(a.as_u64() < b.as_u64());
        assert!(b.as_u64() < c.as_u64());
    }

    #[test]
    fn process_id_display_and_debug() {
        let id = ProcessId::next();
        let display = format!("{}", id);
        assert!(display.starts_with("pid:"));
        let debug = format!("{:?}", id);
        assert!(debug.starts_with("ProcessId("));
    }

    #[test]
    fn pcb_default_state() {
        let pcb = ProcessControlBlock::new(10, Some("worker".to_string()));
        assert_eq!(pcb.status(), ProcessStatus::Ready);
        assert_eq!(pcb.priority(), 10);
        assert_eq!(pcb.name(), Some("worker"));
        assert_eq!(pcb.mailbox_len(), 0);
        assert!(pcb.receive_message().is_none());
    }

    #[test]
    fn pcb_status_transitions() {
        let pcb = ProcessControlBlock::new(0, None);
        assert_eq!(pcb.status(), ProcessStatus::Ready);

        pcb.set_status(ProcessStatus::Running);
        assert_eq!(pcb.status(), ProcessStatus::Running);

        pcb.set_status(ProcessStatus::Suspended);
        assert_eq!(pcb.status(), ProcessStatus::Suspended);

        pcb.set_status(ProcessStatus::Completed);
        assert_eq!(pcb.status(), ProcessStatus::Completed);
    }

    #[test]
    fn pcb_mailbox_fifo() {
        let pcb = ProcessControlBlock::new(5, None);

        pcb.send_message(Message::new(json!(1)));
        pcb.send_message(Message::new(json!(2)));
        pcb.send_message(Message::new(json!(3)));

        assert_eq!(pcb.mailbox_len(), 3);

        let m1 = pcb.receive_message().unwrap();
        assert_eq!(m1.payload, json!(1));

        let m2 = pcb.receive_message().unwrap();
        assert_eq!(m2.payload, json!(2));

        let m3 = pcb.receive_message().unwrap();
        assert_eq!(m3.payload, json!(3));

        assert!(pcb.receive_message().is_none());
        assert_eq!(pcb.mailbox_len(), 0);
    }

    #[test]
    fn pcb_debug_format() {
        let pcb = ProcessControlBlock::new(7, Some("test-proc".to_string()));
        pcb.send_message(Message::new(json!("hello")));
        let dbg = format!("{:?}", pcb);
        assert!(dbg.contains("test-proc"));
        assert!(dbg.contains("mailbox_len: 1"));
    }

    #[test]
    fn pcb_created_at_is_recent() {
        let before = Instant::now();
        let pcb = ProcessControlBlock::new(0, None);
        let after = Instant::now();
        assert!(pcb.created_at() >= before);
        assert!(pcb.created_at() <= after);
    }

    #[test]
    fn pcb_unnamed_process() {
        let pcb = ProcessControlBlock::new(128, None);
        assert!(pcb.name().is_none());
    }

    #[test]
    fn process_status_display() {
        assert_eq!(ProcessStatus::Ready.to_string(), "Ready");
        assert_eq!(ProcessStatus::Running.to_string(), "Running");
        assert_eq!(ProcessStatus::Suspended.to_string(), "Suspended");
        assert_eq!(ProcessStatus::Completed.to_string(), "Completed");
        assert_eq!(ProcessStatus::Failed.to_string(), "Failed");
    }

    #[test]
    fn concurrent_mailbox_access() {
        use std::sync::Arc;
        use std::thread;

        let pcb = Arc::new(ProcessControlBlock::new(0, Some("concurrent".into())));
        let mut handles = vec![];

        // Spawn 10 threads, each sending 100 messages.
        for t in 0..10 {
            let pcb = Arc::clone(&pcb);
            handles.push(thread::spawn(move || {
                for i in 0..100 {
                    pcb.send_message(Message::new(json!({ "thread": t, "seq": i })));
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(pcb.mailbox_len(), 1000);

        // Drain and verify count.
        let mut count = 0;
        while pcb.receive_message().is_some() {
            count += 1;
        }
        assert_eq!(count, 1000);
    }
}
