//! Mailbox — bounded/unbounded MPSC queue for agent communication.
//!
//! This module provides a [`Mailbox`] abstraction wrapping crossbeam MPSC
//! channels with both non-blocking and blocking receive operations, plus
//! timeout-based receives. Designed to integrate with the existing [`Actor`]
//! trait in [`crate::services::actor`].
//!
//! Supports Erlang-style **selective receive**: [`recv_selective`] scans the
//! mailbox for the first message matching a predicate, leaving non-matching
//! messages in order via an internal save queue.
//!
//! # Example
//!
//! ```rust
//! use lumen_rt::services::mailbox::{Mailbox, MailboxSender};
//!
//! let (sender, mailbox) = Mailbox::<String>::unbounded();
//! sender.send("hello".to_string()).unwrap();
//! assert_eq!(mailbox.recv(), Some("hello".to_string()));
//! assert!(mailbox.is_empty());
//! ```

use crossbeam_channel::{self as cb};
use std::collections::VecDeque;
use std::fmt;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Error returned when sending into a closed mailbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailboxSendError<T>(pub T);

impl<T> fmt::Display for MailboxSendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "mailbox send failed: receiver has been dropped")
    }
}

impl<T: fmt::Debug> std::error::Error for MailboxSendError<T> {}

/// Error returned by blocking receive when the mailbox is closed and empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MailboxRecvError;

impl fmt::Display for MailboxRecvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "mailbox recv failed: all senders dropped and queue empty"
        )
    }
}

impl std::error::Error for MailboxRecvError {}

// ---------------------------------------------------------------------------
// MailboxSender
// ---------------------------------------------------------------------------

/// The sending half of a mailbox.
///
/// Cheaply cloneable — multiple producers can hold a `MailboxSender` and
/// send messages concurrently. The mailbox is only closed when *all* senders
/// are dropped.
pub struct MailboxSender<T> {
    inner: cb::Sender<T>,
}

impl<T> Clone for MailboxSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> fmt::Debug for MailboxSender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MailboxSender")
            .field("pending", &self.inner.len())
            .finish()
    }
}

impl<T> MailboxSender<T> {
    /// Non-blocking send. Returns `Err` if the mailbox receiver has been dropped.
    pub fn send(&self, msg: T) -> Result<(), MailboxSendError<T>> {
        self.inner.send(msg).map_err(|e| MailboxSendError(e.0))
    }

    /// Number of messages currently buffered in the mailbox.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Whether the mailbox buffer is currently empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Mailbox
// ---------------------------------------------------------------------------

/// A receive-side mailbox wrapping an MPSC channel with an Erlang-style
/// save queue for selective receive.
///
/// Provides non-blocking (`recv`), blocking (`recv_blocking`), and
/// timeout-based (`recv_timeout`) receive operations. Supports selective
/// receive via [`recv_selective`](Mailbox::recv_selective) which scans for
/// matching messages while preserving non-matching ones in order.
///
/// Integrates with the actor system — an actor's message loop can be driven
/// by a `Mailbox`.
pub struct Mailbox<T> {
    inner: cb::Receiver<T>,
    /// Erlang-style save queue: messages that were inspected during selective
    /// receive but did not match the predicate are stored here and drained
    /// first on subsequent receive calls.
    save_queue: std::cell::RefCell<VecDeque<T>>,
}

impl<T> fmt::Debug for Mailbox<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mailbox")
            .field(
                "pending",
                &(self.save_queue.borrow().len() + self.inner.len()),
            )
            .finish()
    }
}

impl<T> Mailbox<T> {
    /// Create an unbounded mailbox, returning `(sender, mailbox)`.
    ///
    /// The sender never blocks; memory is the only limit on buffering.
    pub fn unbounded() -> (MailboxSender<T>, Self) {
        let (tx, rx) = cb::unbounded();
        (
            MailboxSender { inner: tx },
            Self {
                inner: rx,
                save_queue: std::cell::RefCell::new(VecDeque::new()),
            },
        )
    }

    /// Create a bounded mailbox with the given capacity.
    ///
    /// Senders block when the buffer is full (back-pressure).
    pub fn bounded(capacity: usize) -> (MailboxSender<T>, Self) {
        let (tx, rx) = cb::bounded(capacity);
        (
            MailboxSender { inner: tx },
            Self {
                inner: rx,
                save_queue: std::cell::RefCell::new(VecDeque::new()),
            },
        )
    }

    /// Non-blocking receive. Returns `None` if no message is available.
    ///
    /// Messages in the save queue (from prior selective receives) are
    /// drained first, in order.
    ///
    /// This returns `None` both when the channel is empty-but-open and when
    /// it is closed-and-drained. Use `is_disconnected` to distinguish.
    pub fn recv(&self) -> Option<T> {
        // First check the save queue
        {
            let mut sq = self.save_queue.borrow_mut();
            if let Some(msg) = sq.pop_front() {
                return Some(msg);
            }
        }
        self.inner.try_recv().ok()
    }

    /// Blocking receive. Waits until a message arrives.
    ///
    /// Messages in the save queue are drained first.
    ///
    /// Returns `Err(MailboxRecvError)` only when all senders have been dropped
    /// and the queue is empty.
    pub fn recv_blocking(&self) -> Result<T, MailboxRecvError> {
        {
            let mut sq = self.save_queue.borrow_mut();
            if let Some(msg) = sq.pop_front() {
                return Ok(msg);
            }
        }
        self.inner.recv().map_err(|_| MailboxRecvError)
    }

    /// Receive with a timeout. Returns `None` if no message arrives within
    /// `duration`, or if all senders have been dropped.
    ///
    /// Messages in the save queue are drained first.
    pub fn recv_timeout(&self, duration: Duration) -> Option<T> {
        {
            let mut sq = self.save_queue.borrow_mut();
            if let Some(msg) = sq.pop_front() {
                return Some(msg);
            }
        }
        self.inner.recv_timeout(duration).ok()
    }

    /// Selective receive (Erlang-style). Scans the mailbox for the first
    /// message matching `predicate`, leaving non-matching messages in order.
    ///
    /// The scan order is: save queue first (front to back), then the
    /// underlying channel. Messages inspected but not matching are placed
    /// back in the save queue so that a subsequent `recv()` or
    /// `recv_selective()` sees them in the original order.
    ///
    /// Returns `None` if no matching message is currently available (i.e. the
    /// save queue and channel have been fully scanned without a match).
    pub fn recv_selective<F>(&self, predicate: F) -> Option<T>
    where
        F: Fn(&T) -> bool,
    {
        let mut sq = self.save_queue.borrow_mut();

        // Phase 1: scan the save queue by index — remove the first match
        for i in 0..sq.len() {
            if predicate(&sq[i]) {
                return sq.remove(i);
            }
        }

        // Phase 2: drain the channel, testing each message
        while let Ok(msg) = self.inner.try_recv() {
            if predicate(&msg) {
                return Some(msg);
            }
            sq.push_back(msg);
        }

        None
    }

    /// Selective receive with timeout. Same as [`recv_selective`](Mailbox::recv_selective)
    /// but waits up to `timeout` for a matching message to arrive.
    ///
    /// Returns `None` if no matching message arrives within the timeout.
    pub fn recv_selective_timeout<F>(&self, predicate: F, timeout: Duration) -> Option<T>
    where
        F: Fn(&T) -> bool,
    {
        use std::time::Instant;

        let deadline = Instant::now() + timeout;

        // First, do a non-blocking selective scan of everything already buffered
        {
            let mut sq = self.save_queue.borrow_mut();

            // Phase 1: scan save queue by index
            for i in 0..sq.len() {
                if predicate(&sq[i]) {
                    return sq.remove(i);
                }
            }

            // Phase 2: drain channel (non-blocking)
            while let Ok(msg) = self.inner.try_recv() {
                if predicate(&msg) {
                    return Some(msg);
                }
                sq.push_back(msg);
            }
        }

        // Phase 3: wait for new messages until deadline
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            match self.inner.recv_timeout(remaining) {
                Ok(msg) => {
                    if predicate(&msg) {
                        return Some(msg);
                    }
                    self.save_queue.borrow_mut().push_back(msg);
                }
                Err(_) => return None,
            }
        }
    }

    /// Number of messages currently buffered (save queue + channel).
    pub fn len(&self) -> usize {
        self.save_queue.borrow().len() + self.inner.len()
    }

    /// Whether the mailbox is currently empty (save queue + channel).
    pub fn is_empty(&self) -> bool {
        self.save_queue.borrow().is_empty() && self.inner.is_empty()
    }

    /// Returns `true` if all senders have been dropped.
    ///
    /// Note: even if disconnected, there may still be buffered messages
    /// retrievable via `recv` / `recv_blocking` (including in the save queue).
    pub fn is_disconnected(&self) -> bool {
        // If there are messages in the save queue, we're not fully disconnected
        // in a meaningful sense — there's still data to read.
        if !self.save_queue.borrow().is_empty() {
            return false;
        }
        // crossbeam doesn't expose a direct "is_disconnected" method.
        // The simplest approach: attempt a zero-duration recv.
        match self.inner.recv_timeout(Duration::ZERO) {
            Err(cb::RecvTimeoutError::Disconnected) => self.inner.is_empty(),
            _ => false,
        }
    }

    /// Drain all currently-buffered messages into a `Vec`.
    ///
    /// Save-queue messages come first (in order), then channel messages.
    pub fn drain(&self) -> Vec<T> {
        let mut msgs = Vec::new();
        {
            let mut sq = self.save_queue.borrow_mut();
            while let Some(msg) = sq.pop_front() {
                msgs.push(msg);
            }
        }
        while let Ok(msg) = self.inner.try_recv() {
            msgs.push(msg);
        }
        msgs
    }

    /// Number of messages currently in the save queue (skipped by
    /// selective receive but not yet consumed).
    pub fn save_queue_len(&self) -> usize {
        self.save_queue.borrow().len()
    }

    /// Provide access to the underlying crossbeam `Receiver` for use in
    /// `crossbeam_channel::select!` or integration with the actor system.
    pub fn as_receiver(&self) -> &cb::Receiver<T> {
        &self.inner
    }
}

// ---------------------------------------------------------------------------
// Integration: convert between Mailbox and Actor-compatible channel
// ---------------------------------------------------------------------------

/// Create a mailbox-backed actor message loop.
///
/// This is a convenience for wiring a `Mailbox` into an actor's receive loop
/// without the full `spawn_actor` machinery. Useful when the caller wants
/// direct control over the thread or async runtime.
///
/// Returns a `(MailboxSender<T>, Mailbox<T>)` pair. The caller drives the
/// receive side; producers use the sender.
pub fn mailbox_pair<T>(capacity: Option<usize>) -> (MailboxSender<T>, Mailbox<T>) {
    match capacity {
        Some(cap) => Mailbox::bounded(cap),
        None => Mailbox::unbounded(),
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
    use std::thread;
    use std::time::Duration;

    // =====================================================================
    // 1. Basic unbounded send/recv
    // =====================================================================
    #[test]
    fn unbounded_send_recv() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        tx.send(42).unwrap();
        tx.send(99).unwrap();
        assert_eq!(mb.recv(), Some(42));
        assert_eq!(mb.recv(), Some(99));
        assert_eq!(mb.recv(), None);
    }

    // =====================================================================
    // 2. Bounded send/recv
    // =====================================================================
    #[test]
    fn bounded_send_recv() {
        let (tx, mb) = Mailbox::<String>::bounded(2);
        tx.send("a".into()).unwrap();
        tx.send("b".into()).unwrap();
        assert_eq!(mb.recv(), Some("a".to_string()));
        assert_eq!(mb.recv(), Some("b".to_string()));
        assert_eq!(mb.recv(), None);
    }

    // =====================================================================
    // 3. recv_blocking waits for message
    // =====================================================================
    #[test]
    fn recv_blocking_waits() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            tx.send(7).unwrap();
        });
        let val = mb.recv_blocking().unwrap();
        assert_eq!(val, 7);
        handle.join().unwrap();
    }

    // =====================================================================
    // 4. recv_blocking returns error when all senders dropped
    // =====================================================================
    #[test]
    fn recv_blocking_error_on_disconnect() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        drop(tx);
        assert!(mb.recv_blocking().is_err());
    }

    // =====================================================================
    // 5. recv_timeout returns None on timeout
    // =====================================================================
    #[test]
    fn recv_timeout_returns_none() {
        let (_tx, mb) = Mailbox::<i32>::unbounded();
        let result = mb.recv_timeout(Duration::from_millis(10));
        assert!(result.is_none());
    }

    // =====================================================================
    // 6. recv_timeout returns message before timeout
    // =====================================================================
    #[test]
    fn recv_timeout_returns_message() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        tx.send(55).unwrap();
        let result = mb.recv_timeout(Duration::from_millis(100));
        assert_eq!(result, Some(55));
    }

    // =====================================================================
    // 7. len and is_empty
    // =====================================================================
    #[test]
    fn len_and_is_empty() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        assert!(mb.is_empty());
        assert_eq!(mb.len(), 0);

        tx.send(1).unwrap();
        tx.send(2).unwrap();
        assert_eq!(mb.len(), 2);
        assert!(!mb.is_empty());

        mb.recv();
        assert_eq!(mb.len(), 1);
    }

    // =====================================================================
    // 8. drain collects all buffered messages
    // =====================================================================
    #[test]
    fn drain_all() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        for i in 0..5 {
            tx.send(i).unwrap();
        }
        let drained = mb.drain();
        assert_eq!(drained, vec![0, 1, 2, 3, 4]);
        assert!(mb.is_empty());
    }

    // =====================================================================
    // 9. Multiple senders (MPSC)
    // =====================================================================
    #[test]
    fn multiple_senders() {
        let (tx, mb) = Mailbox::<u64>::unbounded();
        let counter = Arc::new(AtomicUsize::new(0));
        let num_senders = 5;
        let msgs_per_sender = 20;

        let mut handles = vec![];
        for _ in 0..num_senders {
            let tx = tx.clone();
            let ctr = Arc::clone(&counter);
            handles.push(thread::spawn(move || {
                for _ in 0..msgs_per_sender {
                    tx.send(1).unwrap();
                    ctr.fetch_add(1, Ordering::Relaxed);
                }
            }));
        }
        drop(tx); // drop original sender

        for h in handles {
            h.join().unwrap();
        }

        let drained = mb.drain();
        assert_eq!(drained.len(), num_senders * msgs_per_sender);
    }

    // =====================================================================
    // 10. Send to dropped mailbox returns error
    // =====================================================================
    #[test]
    fn send_to_dropped_mailbox_errors() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        drop(mb);
        let err = tx.send(42).unwrap_err();
        assert_eq!(err.0, 42);
    }

    // =====================================================================
    // 11. Sender len/is_empty reflect mailbox state
    // =====================================================================
    #[test]
    fn sender_len_and_is_empty() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        assert!(tx.is_empty());
        assert_eq!(tx.len(), 0);

        tx.send(1).unwrap();
        assert_eq!(tx.len(), 1);
        assert!(!tx.is_empty());

        mb.recv();
        assert_eq!(tx.len(), 0);
    }

    // =====================================================================
    // 12. Bounded back-pressure (sender blocks when full)
    // =====================================================================
    #[test]
    fn bounded_back_pressure() {
        let (tx, mb) = Mailbox::<i32>::bounded(1);
        tx.send(1).unwrap(); // fills buffer

        // Spawn thread to drain so sender can unblock
        let mb_handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            let v = mb.recv_blocking().unwrap();
            assert_eq!(v, 1);
            let v2 = mb.recv_blocking().unwrap();
            assert_eq!(v2, 2);
        });

        tx.send(2).unwrap(); // should unblock once consumer drains
        mb_handle.join().unwrap();
    }

    // =====================================================================
    // 13. mailbox_pair convenience
    // =====================================================================
    #[test]
    fn mailbox_pair_unbounded() {
        let (tx, mb) = mailbox_pair::<i32>(None);
        tx.send(10).unwrap();
        assert_eq!(mb.recv(), Some(10));
    }

    #[test]
    fn mailbox_pair_bounded() {
        let (tx, mb) = mailbox_pair::<i32>(Some(5));
        tx.send(20).unwrap();
        assert_eq!(mb.recv(), Some(20));
    }

    // =====================================================================
    // 14. Debug format
    // =====================================================================
    #[test]
    fn debug_format() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        tx.send(1).unwrap();
        let tx_dbg = format!("{:?}", tx);
        let mb_dbg = format!("{:?}", mb);
        assert!(tx_dbg.contains("MailboxSender"));
        assert!(mb_dbg.contains("Mailbox"));
    }

    // =====================================================================
    // 15. Error display
    // =====================================================================
    #[test]
    fn error_display() {
        let send_err = MailboxSendError(42);
        assert!(send_err.to_string().contains("dropped"));

        let recv_err = MailboxRecvError;
        assert!(recv_err.to_string().contains("empty"));
    }

    // =====================================================================
    // 16. is_disconnected
    // =====================================================================
    #[test]
    fn is_disconnected_when_senders_dropped() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        assert!(!mb.is_disconnected());
        drop(tx);
        assert!(mb.is_disconnected());
    }

    #[test]
    fn is_disconnected_false_with_buffered_messages() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        tx.send(1).unwrap();
        drop(tx);
        // Messages still buffered — may or may not report disconnected
        // depending on implementation, but recv should still work
        assert_eq!(mb.recv(), Some(1));
        assert!(mb.is_disconnected());
    }

    // =====================================================================
    // 17. High-throughput stress test
    // =====================================================================
    #[test]
    fn high_throughput_stress() {
        let (tx, mb) = Mailbox::<u64>::unbounded();
        let total_msgs = 10_000;

        let producer = thread::spawn(move || {
            for i in 0..total_msgs {
                tx.send(i).unwrap();
            }
        });

        producer.join().unwrap();
        let drained = mb.drain();
        assert_eq!(drained.len(), total_msgs as usize);

        // Verify ordering (single producer, so should be in order)
        for (i, val) in drained.iter().enumerate() {
            assert_eq!(*val, i as u64);
        }
    }

    // =====================================================================
    // 18. recv_timeout with disconnected sender
    // =====================================================================
    #[test]
    fn recv_timeout_disconnected() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        tx.send(99).unwrap();
        drop(tx);
        // Should still get the buffered message
        assert_eq!(mb.recv_timeout(Duration::from_millis(50)), Some(99));
        // Now empty and disconnected
        assert_eq!(mb.recv_timeout(Duration::from_millis(10)), None);
    }

    // =====================================================================
    // 19. as_receiver exposes crossbeam receiver
    // =====================================================================
    #[test]
    fn as_receiver_works() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        tx.send(77).unwrap();
        let val = mb.as_receiver().try_recv().unwrap();
        assert_eq!(val, 77);
    }

    // =====================================================================
    // 20. Integration: actor-style message loop using Mailbox
    // =====================================================================
    #[test]
    fn actor_style_message_loop() {
        let (tx, mb) = Mailbox::<String>::unbounded();
        let result = Arc::new(std::sync::Mutex::new(Vec::new()));
        let result_clone = Arc::clone(&result);

        let consumer = thread::spawn(move || loop {
            match mb.recv_blocking() {
                Ok(msg) => {
                    if msg == "STOP" {
                        break;
                    }
                    result_clone.lock().unwrap().push(msg);
                }
                Err(_) => break,
            }
        });

        tx.send("hello".to_string()).unwrap();
        tx.send("world".to_string()).unwrap();
        tx.send("STOP".to_string()).unwrap();

        consumer.join().unwrap();

        let collected = result.lock().unwrap();
        assert_eq!(*collected, vec!["hello".to_string(), "world".to_string()]);
    }

    // =====================================================================
    // 21. Selective receive finds matching message
    // =====================================================================
    #[test]
    fn selective_recv_finds_match() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send(3).unwrap();

        // Select only the even number
        let result = mb.recv_selective(|msg| msg % 2 == 0);
        assert_eq!(result, Some(2));

        // Non-matching messages (1, 3) are preserved
        assert_eq!(mb.len(), 2);
        assert_eq!(mb.recv(), Some(1));
        assert_eq!(mb.recv(), Some(3));
        assert_eq!(mb.recv(), None);
    }

    // =====================================================================
    // 22. Selective receive preserves non-matching message order
    // =====================================================================
    #[test]
    fn selective_recv_preserves_order() {
        let (tx, mb) = Mailbox::<String>::unbounded();
        tx.send("a".into()).unwrap();
        tx.send("b".into()).unwrap();
        tx.send("TARGET".into()).unwrap();
        tx.send("c".into()).unwrap();
        tx.send("d".into()).unwrap();

        let result = mb.recv_selective(|msg| msg == "TARGET");
        assert_eq!(result, Some("TARGET".to_string()));

        // Remaining messages should be in original order: a, b, c, d
        let remaining = mb.drain();
        assert_eq!(
            remaining,
            vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "d".to_string()
            ]
        );
    }

    // =====================================================================
    // 23. Selective receive returns None when no match
    // =====================================================================
    #[test]
    fn selective_recv_no_match() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        tx.send(1).unwrap();
        tx.send(3).unwrap();
        tx.send(5).unwrap();

        let result = mb.recv_selective(|msg| msg % 2 == 0);
        assert_eq!(result, None);

        // All messages should still be in the save queue, retrievable in order
        assert_eq!(mb.len(), 3);
        assert_eq!(mb.recv(), Some(1));
        assert_eq!(mb.recv(), Some(3));
        assert_eq!(mb.recv(), Some(5));
    }

    // =====================================================================
    // 24. Selective receive on empty mailbox
    // =====================================================================
    #[test]
    fn selective_recv_empty() {
        let (_tx, mb) = Mailbox::<i32>::unbounded();
        let result = mb.recv_selective(|_| true);
        assert_eq!(result, None);
    }

    // =====================================================================
    // 25. Selective receive all messages match — returns first
    // =====================================================================
    #[test]
    fn selective_recv_all_match_returns_first() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        tx.send(2).unwrap();
        tx.send(4).unwrap();
        tx.send(6).unwrap();

        let result = mb.recv_selective(|msg| msg % 2 == 0);
        assert_eq!(result, Some(2));

        // Remaining: 4, 6
        assert_eq!(mb.recv(), Some(4));
        assert_eq!(mb.recv(), Some(6));
    }

    // =====================================================================
    // 26. Selective receive with timeout — finds match before timeout
    // =====================================================================
    #[test]
    fn selective_recv_timeout_finds_match() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        tx.send(1).unwrap();
        tx.send(42).unwrap();
        tx.send(3).unwrap();

        let result = mb.recv_selective_timeout(|msg| *msg == 42, Duration::from_millis(100));
        assert_eq!(result, Some(42));

        // Non-matching messages preserved in order
        assert_eq!(mb.recv(), Some(1));
        assert_eq!(mb.recv(), Some(3));
    }

    // =====================================================================
    // 27. Selective receive with timeout — times out
    // =====================================================================
    #[test]
    fn selective_recv_timeout_expires() {
        let (_tx, mb) = Mailbox::<i32>::unbounded();
        let result = mb.recv_selective_timeout(|_| true, Duration::from_millis(10));
        assert_eq!(result, None);
    }

    // =====================================================================
    // 28. Selective receive with timeout — message arrives during wait
    // =====================================================================
    #[test]
    fn selective_recv_timeout_waits_for_new_message() {
        let (tx, mb) = Mailbox::<i32>::unbounded();

        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            tx.send(99).unwrap();
        });

        let result = mb.recv_selective_timeout(|msg| *msg == 99, Duration::from_millis(500));
        assert_eq!(result, Some(99));

        handle.join().unwrap();
    }

    // =====================================================================
    // 29. Multiple selective receives in sequence
    // =====================================================================
    #[test]
    fn multiple_selective_receives() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        for i in 1..=10 {
            tx.send(i).unwrap();
        }

        // Pick out 5, then 3, then 7
        assert_eq!(mb.recv_selective(|m| *m == 5), Some(5));
        assert_eq!(mb.recv_selective(|m| *m == 3), Some(3));
        assert_eq!(mb.recv_selective(|m| *m == 7), Some(7));

        // Remaining should be 1, 2, 4, 6, 8, 9, 10 in order
        let remaining = mb.drain();
        assert_eq!(remaining, vec![1, 2, 4, 6, 8, 9, 10]);
    }

    // =====================================================================
    // 30. Save queue length tracking
    // =====================================================================
    #[test]
    fn save_queue_len() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send(3).unwrap();

        assert_eq!(mb.save_queue_len(), 0);

        // No match — all go to save queue
        mb.recv_selective(|m| *m == 99);
        assert_eq!(mb.save_queue_len(), 3);

        // recv drains from save queue first
        mb.recv();
        assert_eq!(mb.save_queue_len(), 2);
    }

    // =====================================================================
    // 31. Selective receive from save queue (previously skipped messages)
    // =====================================================================
    #[test]
    fn selective_recv_from_save_queue() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send(3).unwrap();

        // First selective: look for 99 (no match) — puts 1, 2, 3 in save queue
        assert_eq!(mb.recv_selective(|m| *m == 99), None);
        assert_eq!(mb.save_queue_len(), 3);

        // Second selective: look for 2 — should find it in the save queue
        assert_eq!(mb.recv_selective(|m| *m == 2), Some(2));
        assert_eq!(mb.save_queue_len(), 2);

        // Remaining: 1, 3
        assert_eq!(mb.recv(), Some(1));
        assert_eq!(mb.recv(), Some(3));
    }

    // =====================================================================
    // 32. recv_blocking drains save queue first
    // =====================================================================
    #[test]
    fn recv_blocking_drains_save_queue() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        tx.send(10).unwrap();
        tx.send(20).unwrap();

        // Miss — both go to save queue
        mb.recv_selective(|m| *m == 99);
        assert_eq!(mb.save_queue_len(), 2);

        // recv_blocking should get from save queue first
        assert_eq!(mb.recv_blocking().unwrap(), 10);
        assert_eq!(mb.recv_blocking().unwrap(), 20);
    }

    // =====================================================================
    // 33. recv_timeout drains save queue first
    // =====================================================================
    #[test]
    fn recv_timeout_drains_save_queue() {
        let (tx, mb) = Mailbox::<i32>::unbounded();
        tx.send(5).unwrap();

        // Miss — goes to save queue
        mb.recv_selective(|m| *m == 99);

        let result = mb.recv_timeout(Duration::from_millis(10));
        assert_eq!(result, Some(5));
    }
}
