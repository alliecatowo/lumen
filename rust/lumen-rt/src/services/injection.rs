//! Thread-safe global injection queue.
//!
//! The [`InjectionQueue`] is a FIFO queue protected by a [`Mutex`] that serves
//! as the entry point for newly spawned tasks. Workers drain it periodically
//! when their local queues are empty (before attempting to steal from peers).
//!
//! This is intentionally a simple `Mutex<VecDeque<T>>` rather than a lock-free
//! structure. The injection queue is contended only during spawn and drain
//! operations — the hot path (local deque pop) is lock-free. A lock-free
//! queue can be swapped in later if profiling shows contention.

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex};

/// A thread-safe FIFO queue for injecting work into the scheduler.
///
/// Cloning an `InjectionQueue` produces a new handle to the *same* underlying
/// queue (backed by `Arc`).
pub struct InjectionQueue<T> {
    inner: Arc<Mutex<VecDeque<T>>>,
}

impl<T> Clone for InjectionQueue<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T> Default for InjectionQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> InjectionQueue<T> {
    /// Create a new, empty injection queue.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Push a value to the back of the queue.
    pub fn push(&self, value: T) {
        self.inner.lock().unwrap().push_back(value);
    }

    /// Pop a value from the front of the queue.
    ///
    /// Returns `None` if the queue is empty.
    pub fn pop(&self) -> Option<T> {
        self.inner.lock().unwrap().pop_front()
    }

    /// Drain up to `max` items from the front of the queue into the provided
    /// vector.
    ///
    /// Returns the number of items drained.
    pub fn drain_into(&self, dest: &mut Vec<T>, max: usize) -> usize {
        let mut guard = self.inner.lock().unwrap();
        let n = max.min(guard.len());
        dest.reserve(n);
        for _ in 0..n {
            if let Some(item) = guard.pop_front() {
                dest.push(item);
            }
        }
        n
    }

    /// Drain all items from the queue into the provided vector.
    ///
    /// Returns the number of items drained.
    pub fn drain_all(&self, dest: &mut Vec<T>) -> usize {
        let mut guard = self.inner.lock().unwrap();
        let n = guard.len();
        dest.reserve(n);
        dest.extend(guard.drain(..));
        n
    }

    /// Return the number of items currently in the queue.
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    /// Return `true` if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }
}

impl<T: fmt::Debug> fmt::Debug for InjectionQueue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let guard = self.inner.lock().unwrap();
        f.debug_struct("InjectionQueue")
            .field("len", &guard.len())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn push_and_pop_fifo() {
        let q = InjectionQueue::new();
        q.push(1);
        q.push(2);
        q.push(3);
        assert_eq!(q.pop(), Some(1));
        assert_eq!(q.pop(), Some(2));
        assert_eq!(q.pop(), Some(3));
        assert_eq!(q.pop(), None);
    }

    #[test]
    fn len_and_is_empty() {
        let q = InjectionQueue::new();
        assert!(q.is_empty());
        assert_eq!(q.len(), 0);

        q.push(42);
        assert!(!q.is_empty());
        assert_eq!(q.len(), 1);

        q.pop();
        assert!(q.is_empty());
    }

    #[test]
    fn drain_into_respects_max() {
        let q = InjectionQueue::new();
        for i in 0..10 {
            q.push(i);
        }

        let mut buf = Vec::new();
        let drained = q.drain_into(&mut buf, 3);
        assert_eq!(drained, 3);
        assert_eq!(buf, vec![0, 1, 2]);
        assert_eq!(q.len(), 7);
    }

    #[test]
    fn drain_all_empties_queue() {
        let q = InjectionQueue::new();
        for i in 0..5 {
            q.push(i);
        }

        let mut buf = Vec::new();
        let drained = q.drain_all(&mut buf);
        assert_eq!(drained, 5);
        assert_eq!(buf, vec![0, 1, 2, 3, 4]);
        assert!(q.is_empty());
    }

    #[test]
    fn clone_shares_queue() {
        let q1 = InjectionQueue::new();
        let q2 = q1.clone();

        q1.push(10);
        assert_eq!(q2.pop(), Some(10));
    }

    #[test]
    fn default_is_empty() {
        let q: InjectionQueue<i32> = InjectionQueue::default();
        assert!(q.is_empty());
    }

    #[test]
    fn debug_format() {
        let q = InjectionQueue::new();
        q.push(1);
        q.push(2);
        let dbg = format!("{:?}", q);
        assert!(dbg.contains("InjectionQueue"));
        assert!(dbg.contains("len: 2"));
    }

    #[test]
    fn concurrent_push_pop() {
        let q = InjectionQueue::new();
        let q_arc = Arc::new(q);
        let mut handles = Vec::new();

        // 8 threads, each pushing 1000 items.
        for t in 0..8 {
            let q = Arc::clone(&q_arc);
            handles.push(thread::spawn(move || {
                for i in 0..1000 {
                    q.push(t * 1000 + i);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(q_arc.len(), 8000);

        // Drain all.
        let mut buf = Vec::new();
        q_arc.drain_all(&mut buf);
        assert_eq!(buf.len(), 8000);
        assert!(q_arc.is_empty());
    }
}
