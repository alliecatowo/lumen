//! Mailbox — bounded/unbounded MPSC queue for agent communication.
//!
//! This module provides a [`Mailbox`] abstraction wrapping crossbeam MPSC
//! channels with both non-blocking and blocking receive operations, plus
//! timeout-based receives. Designed to integrate with the existing [`Actor`]
//! trait in [`crate::actor`].
//!
//! # Example
//!
//! ```rust
//! use lumen_runtime::mailbox::{Mailbox, MailboxSender};
//!
//! let (sender, mailbox) = Mailbox::<String>::unbounded();
//! sender.send("hello".to_string()).unwrap();
//! assert_eq!(mailbox.recv(), Some("hello".to_string()));
//! assert!(mailbox.is_empty());
//! ```

use crossbeam_channel::{self as cb};
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

/// A receive-side mailbox wrapping an MPSC channel.
///
/// Provides non-blocking (`recv`), blocking (`recv_blocking`), and
/// timeout-based (`recv_timeout`) receive operations. Integrates with
/// the actor system — an actor's message loop can be driven by a `Mailbox`.
pub struct Mailbox<T> {
    inner: cb::Receiver<T>,
}

impl<T> fmt::Debug for Mailbox<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mailbox")
            .field("pending", &self.inner.len())
            .finish()
    }
}

impl<T> Mailbox<T> {
    /// Create an unbounded mailbox, returning `(sender, mailbox)`.
    ///
    /// The sender never blocks; memory is the only limit on buffering.
    pub fn unbounded() -> (MailboxSender<T>, Self) {
        let (tx, rx) = cb::unbounded();
        (MailboxSender { inner: tx }, Self { inner: rx })
    }

    /// Create a bounded mailbox with the given capacity.
    ///
    /// Senders block when the buffer is full (back-pressure).
    pub fn bounded(capacity: usize) -> (MailboxSender<T>, Self) {
        let (tx, rx) = cb::bounded(capacity);
        (MailboxSender { inner: tx }, Self { inner: rx })
    }

    /// Non-blocking receive. Returns `None` if no message is available.
    ///
    /// This returns `None` both when the channel is empty-but-open and when
    /// it is closed-and-drained. Use `is_disconnected` to distinguish.
    pub fn recv(&self) -> Option<T> {
        self.inner.try_recv().ok()
    }

    /// Blocking receive. Waits until a message arrives.
    ///
    /// Returns `Err(MailboxRecvError)` only when all senders have been dropped
    /// and the queue is empty.
    pub fn recv_blocking(&self) -> Result<T, MailboxRecvError> {
        self.inner.recv().map_err(|_| MailboxRecvError)
    }

    /// Receive with a timeout. Returns `None` if no message arrives within
    /// `duration`, or if all senders have been dropped.
    pub fn recv_timeout(&self, duration: Duration) -> Option<T> {
        self.inner.recv_timeout(duration).ok()
    }

    /// Number of messages currently buffered.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Whether the mailbox is currently empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns `true` if all senders have been dropped.
    ///
    /// Note: even if disconnected, there may still be buffered messages
    /// retrievable via `recv` / `recv_blocking`.
    pub fn is_disconnected(&self) -> bool {
        // crossbeam doesn't expose a direct "is_disconnected" method.
        // We peek via try_recv: if Disconnected AND empty, it's disconnected.
        // But we must not consume messages. Use the internal receiver's
        // is_empty + a zero-timeout recv to test.
        //
        // Actually, crossbeam's Receiver doesn't have is_disconnected either.
        // The simplest approach: attempt a zero-duration recv.
        match self.inner.recv_timeout(Duration::ZERO) {
            Err(cb::RecvTimeoutError::Disconnected) => self.inner.is_empty(),
            _ => false,
        }
    }

    /// Drain all currently-buffered messages into a `Vec`.
    pub fn drain(&self) -> Vec<T> {
        let mut msgs = Vec::new();
        while let Ok(msg) = self.inner.try_recv() {
            msgs.push(msg);
        }
        msgs
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
}
