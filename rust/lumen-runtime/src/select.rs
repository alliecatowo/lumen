//! Channel multiplexing via a builder-pattern `Selector`.
//!
//! `Selector` lets you wait on multiple [`Receiver`]s simultaneously. When any
//! registered channel has a message ready, the corresponding handler runs and
//! its return value becomes the [`SelectResult`].
//!
//! # Fair selection
//!
//! When multiple channels are ready at the same time, the winner is chosen
//! **randomly** (not always the first-registered channel). This is guaranteed
//! by the underlying [`crossbeam_channel::Select`] primitive.
//!
//! # Example
//!
//! ```ignore
//! use lumen_runtime::channel;
//! use lumen_runtime::select::{Selector, SelectResult};
//! use std::time::Duration;
//!
//! let (tx1, rx1) = channel::unbounded::<i32>();
//! let (tx2, rx2) = channel::unbounded::<String>();
//!
//! tx1.send(42).unwrap();
//!
//! let result = Selector::new()
//!     .recv(&rx1, |v| SelectResult::Matched(format!("int: {v}")))
//!     .recv(&rx2, |v| SelectResult::Matched(format!("str: {v}")))
//!     .timeout(Duration::from_secs(1))
//!     .select();
//!
//! assert!(matches!(result, SelectResult::Matched(_)));
//! ```

use crate::channel::Receiver;
use crossbeam_channel::{self as cb};
use std::time::Duration;

/// Type alias for the boxed handler closures stored inside [`Selector`].
type HandlerFn<'a> = Box<dyn FnOnce() -> Option<SelectResult> + 'a>;

// ---------------------------------------------------------------------------
// Result type
// ---------------------------------------------------------------------------

/// Outcome of a [`Selector::select`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectResult {
    /// A handler ran and produced this value.
    Matched(String),
    /// No channel became ready within the configured timeout.
    Timeout,
    /// The default (non-blocking) case executed.
    Default,
    /// Every registered channel is closed (disconnected).
    Closed,
}

// ---------------------------------------------------------------------------
// Selector builder
// ---------------------------------------------------------------------------

/// A builder for multiplexed channel receives.
///
/// Construct with [`Selector::new`], register channels with [`recv`](Selector::recv),
/// optionally set a [`timeout`](Selector::timeout) or [`default_case`](Selector::default_case),
/// then call [`select`](Selector::select) to block until one fires.
pub struct Selector<'a> {
    /// Crossbeam receivers (raw) kept alive for the duration of the select so
    /// we can register them with `cb::Select` which borrows them.
    receivers: Vec<&'a dyn AsRawReceiver>,

    /// Parallel vec of handler closures (one per arm).
    handlers: Vec<HandlerFn<'a>>,

    /// Optional deadline.
    timeout: Option<Duration>,

    /// Optional non-blocking default handler.
    default_handler: Option<Box<dyn FnOnce() -> SelectResult + 'a>>,
}

/// Internal helper trait to erase `T` from `Receiver<T>` so we can store
/// heterogeneous receivers in the same vec.
trait AsRawReceiver {
    /// Register `self` with a `crossbeam_channel::Select` and return the op index.
    fn register<'s, 'sel>(&'s self, sel: &mut cb::Select<'sel>) -> usize
    where
        's: 'sel;
}

impl<T> AsRawReceiver for Receiver<T> {
    fn register<'s, 'sel>(&'s self, sel: &mut cb::Select<'sel>) -> usize
    where
        's: 'sel,
    {
        sel.recv(&self.inner)
    }
}

impl<'a> Selector<'a> {
    /// Create an empty selector.
    pub fn new() -> Self {
        Self {
            receivers: Vec::new(),
            handlers: Vec::new(),
            timeout: None,
            default_handler: None,
        }
    }

    /// Register a channel to listen on.
    ///
    /// When `rx` has a message ready, `handler` will be called with the
    /// received value. The handler returns a [`SelectResult`] (typically
    /// [`SelectResult::Matched`]).
    ///
    /// Channels are checked fairly — if multiple are ready simultaneously,
    /// one is chosen at random.
    pub fn recv<T: 'a, F>(mut self, rx: &'a Receiver<T>, handler: F) -> Self
    where
        F: FnOnce(T) -> SelectResult + 'a,
    {
        self.receivers.push(rx);

        // Capture a try_recv + handler closure that borrows `rx`.
        let try_handler: HandlerFn<'a> = Box::new(move || {
            match rx.inner.try_recv() {
                Ok(val) => Some(handler(val)),
                // Channel disconnected or was drained between `ready` and
                // `try_recv` — treat as "this arm failed".
                Err(_) => None,
            }
        });

        self.handlers.push(try_handler);
        self
    }

    /// Set a timeout for the select operation.
    ///
    /// If no channel becomes ready within `duration`, [`SelectResult::Timeout`]
    /// is returned.
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    /// Set a non-blocking default case.
    ///
    /// If no channel is *immediately* ready, `handler` runs and its return
    /// value is the result. When a default is set, [`select`](Self::select)
    /// never blocks.
    pub fn default_case<F>(mut self, handler: F) -> Self
    where
        F: FnOnce() -> SelectResult + 'a,
    {
        self.default_handler = Some(Box::new(handler));
        self
    }

    /// Execute the select operation.
    ///
    /// This method consumes the `Selector`. It blocks (subject to timeout /
    /// default) until one channel has a message, then invokes the
    /// corresponding handler.
    ///
    /// # Panics
    ///
    /// Panics if no channels have been registered and no default handler is
    /// set.
    pub fn select(self) -> SelectResult {
        let Selector {
            receivers,
            handlers,
            timeout,
            default_handler,
        } = self;

        if receivers.is_empty() {
            // No channels registered — run default if present, otherwise panic.
            if let Some(dh) = default_handler {
                return dh();
            }
            panic!("Selector::select called with no channels and no default handler");
        }

        // Build a crossbeam Select and register every receiver.
        let mut sel = cb::Select::new();
        let mut indices: Vec<usize> = Vec::with_capacity(receivers.len());
        for rx in &receivers {
            let idx = rx.register(&mut sel);
            indices.push(idx);
        }

        // Convert our parallel vecs into an indexable structure.
        // `handlers` is consumed element-by-element so we put it into an
        // `Option` vec.
        let mut handlers: Vec<Option<HandlerFn<'a>>> = handlers.into_iter().map(Some).collect();

        // ---- Attempt loop --------------------------------------------------
        // `ready()` / `try_ready()` / `ready_timeout()` tell us which index
        // is ready but do NOT consume the message. We then call `try_recv()`
        // via the handler closure. If `try_recv` fails (race), we remove
        // that arm and retry.
        //
        // Once all arms are removed we know every channel is closed.

        // If a default handler is set, do exactly one non-blocking check.
        if let Some(dh) = default_handler {
            match sel.try_ready() {
                Ok(ready_idx) => {
                    // Find which of our arms this corresponds to.
                    if let Some(pos) = indices.iter().position(|&i| i == ready_idx) {
                        if let Some(h) = handlers[pos].take() {
                            if let Some(result) = h() {
                                return result;
                            }
                            // Race: channel drained between ready & try_recv.
                            // Fall through to default.
                        }
                    }
                    return dh();
                }
                Err(_) => return dh(),
            }
        }

        // No default handler — blocking path.
        loop {
            // Check if all handlers have been consumed (all channels closed).
            if handlers.iter().all(|h| h.is_none()) {
                return SelectResult::Closed;
            }

            // Rebuild the Select with only live arms.
            let mut sel = cb::Select::new();
            let mut live: Vec<(usize, usize)> = Vec::new(); // (handler_pos, cb_idx)
            for (pos, rx) in receivers.iter().enumerate() {
                if handlers[pos].is_some() {
                    let idx = rx.register(&mut sel);
                    live.push((pos, idx));
                }
            }

            if live.is_empty() {
                return SelectResult::Closed;
            }

            let ready_result = if let Some(dur) = timeout {
                sel.ready_timeout(dur)
            } else {
                Ok(sel.ready())
            };

            match ready_result {
                Err(_) => return SelectResult::Timeout,
                Ok(ready_idx) => {
                    // Map crossbeam index back to our handler position.
                    if let Some(&(pos, _)) = live.iter().find(|(_, ci)| *ci == ready_idx) {
                        if let Some(h) = handlers[pos].take() {
                            if let Some(result) = h() {
                                return result;
                            }
                            // Handler returned None → channel closed mid-race.
                            // Loop and retry with remaining arms.
                            continue;
                        }
                    }
                    // Unknown index — should not happen. Retry.
                    continue;
                }
            }
        }
    }
}

impl<'a> Default for Selector<'a> {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel;
    use std::collections::HashSet;
    use std::thread;
    use std::time::{Duration, Instant};

    // -- basic recv -------------------------------------------------------

    #[test]
    fn select_single_channel_ready() {
        let (tx, rx) = channel::unbounded::<i32>();
        tx.send(42).unwrap();

        let result = Selector::new()
            .recv(&rx, |v| SelectResult::Matched(format!("{v}")))
            .select();

        assert_eq!(result, SelectResult::Matched("42".into()));
    }

    #[test]
    fn select_two_channels_first_ready() {
        let (tx1, rx1) = channel::unbounded::<i32>();
        let (_tx2, rx2) = channel::unbounded::<String>();

        tx1.send(10).unwrap();

        let result = Selector::new()
            .recv(&rx1, |v| SelectResult::Matched(format!("int:{v}")))
            .recv(&rx2, |v| SelectResult::Matched(format!("str:{v}")))
            .select();

        assert_eq!(result, SelectResult::Matched("int:10".into()));
    }

    #[test]
    fn select_two_channels_second_ready() {
        let (_tx1, rx1) = channel::unbounded::<i32>();
        let (tx2, rx2) = channel::unbounded::<String>();

        tx2.send("hello".into()).unwrap();

        let result = Selector::new()
            .recv(&rx1, |v| SelectResult::Matched(format!("int:{v}")))
            .recv(&rx2, |v| SelectResult::Matched(format!("str:{v}")))
            .select();

        assert_eq!(result, SelectResult::Matched("str:hello".into()));
    }

    // -- blocking recv (message arrives from another thread) ---------------

    #[test]
    fn select_blocks_until_message() {
        let (tx, rx) = channel::unbounded::<i32>();

        thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            tx.send(99).unwrap();
        });

        let start = Instant::now();
        let result = Selector::new()
            .recv(&rx, |v| SelectResult::Matched(format!("{v}")))
            .select();

        assert_eq!(result, SelectResult::Matched("99".into()));
        assert!(start.elapsed() >= Duration::from_millis(20));
    }

    // -- timeout ----------------------------------------------------------

    #[test]
    fn select_timeout_fires() {
        let (_tx, rx) = channel::unbounded::<i32>();

        let start = Instant::now();
        let result = Selector::new()
            .recv(&rx, |v| SelectResult::Matched(format!("{v}")))
            .timeout(Duration::from_millis(50))
            .select();

        assert_eq!(result, SelectResult::Timeout);
        assert!(start.elapsed() >= Duration::from_millis(40));
    }

    #[test]
    fn select_timeout_not_needed_when_ready() {
        let (tx, rx) = channel::unbounded::<i32>();
        tx.send(7).unwrap();

        let result = Selector::new()
            .recv(&rx, |v| SelectResult::Matched(format!("{v}")))
            .timeout(Duration::from_secs(10))
            .select();

        assert_eq!(result, SelectResult::Matched("7".into()));
    }

    // -- default case (non-blocking) --------------------------------------

    #[test]
    fn select_default_when_nothing_ready() {
        let (_tx, rx) = channel::unbounded::<i32>();

        let result = Selector::new()
            .recv(&rx, |_v| SelectResult::Matched("nope".into()))
            .default_case(|| SelectResult::Default)
            .select();

        assert_eq!(result, SelectResult::Default);
    }

    #[test]
    fn select_default_not_used_when_ready() {
        let (tx, rx) = channel::unbounded::<i32>();
        tx.send(5).unwrap();

        let result = Selector::new()
            .recv(&rx, |v| SelectResult::Matched(format!("{v}")))
            .default_case(|| SelectResult::Default)
            .select();

        assert_eq!(result, SelectResult::Matched("5".into()));
    }

    #[test]
    fn select_default_with_no_channels() {
        let result = Selector::new()
            .default_case(|| SelectResult::Default)
            .select();

        assert_eq!(result, SelectResult::Default);
    }

    // -- closed detection -------------------------------------------------

    #[test]
    fn select_all_closed() {
        let (tx, rx) = channel::unbounded::<i32>();
        drop(tx); // close the channel

        let result = Selector::new()
            .recv(&rx, |v| SelectResult::Matched(format!("{v}")))
            .select();

        assert_eq!(result, SelectResult::Closed);
    }

    #[test]
    fn select_closed_multiple_channels() {
        let (tx1, rx1) = channel::unbounded::<i32>();
        let (tx2, rx2) = channel::unbounded::<String>();
        drop(tx1);
        drop(tx2);

        let result = Selector::new()
            .recv(&rx1, |v| SelectResult::Matched(format!("{v}")))
            .recv(&rx2, |v| SelectResult::Matched(format!("{v}")))
            .select();

        assert_eq!(result, SelectResult::Closed);
    }

    #[test]
    fn select_partially_closed_receives_from_open() {
        let (tx1, rx1) = channel::unbounded::<i32>();
        let (tx2, rx2) = channel::unbounded::<i32>();
        drop(tx1); // close first channel
        tx2.send(77).unwrap();

        let result = Selector::new()
            .recv(&rx1, |v| SelectResult::Matched(format!("a:{v}")))
            .recv(&rx2, |v| SelectResult::Matched(format!("b:{v}")))
            .select();

        assert_eq!(result, SelectResult::Matched("b:77".into()));
    }

    // -- fair selection ---------------------------------------------------

    #[test]
    fn select_fairness_both_channels_get_picked() {
        // When both channels always have a message, over many iterations
        // both should be selected at least once. This is a probabilistic
        // test — with random selection and 200 iterations the chance of
        // never picking one channel is astronomically low (2^-200).
        let (tx1, rx1) = channel::unbounded::<&str>();
        let (tx2, rx2) = channel::unbounded::<&str>();

        let mut seen = HashSet::new();
        for _ in 0..200 {
            tx1.send("a").unwrap();
            tx2.send("b").unwrap();

            let result = Selector::new()
                .recv(&rx1, |v| SelectResult::Matched(v.to_string()))
                .recv(&rx2, |v| SelectResult::Matched(v.to_string()))
                .select();

            if let SelectResult::Matched(s) = result {
                seen.insert(s);
            }

            // Drain the other channel so we start fresh.
            let _ = rx1.try_recv();
            let _ = rx2.try_recv();
        }

        assert!(
            seen.contains("a") && seen.contains("b"),
            "Expected both channels to be selected at least once, got: {seen:?}"
        );
    }

    // -- bounded channel --------------------------------------------------

    #[test]
    fn select_with_bounded_channel() {
        let (tx, rx) = channel::bounded::<i32>(1);
        tx.send(100).unwrap();

        let result = Selector::new()
            .recv(&rx, |v| SelectResult::Matched(format!("{v}")))
            .select();

        assert_eq!(result, SelectResult::Matched("100".into()));
    }

    // -- multiple messages / drain ----------------------------------------

    #[test]
    fn select_successive_calls_drain_messages() {
        let (tx, rx) = channel::unbounded::<i32>();
        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send(3).unwrap();

        let mut results = Vec::new();
        for _ in 0..3 {
            let r = Selector::new()
                .recv(&rx, |v| SelectResult::Matched(format!("{v}")))
                .timeout(Duration::from_millis(50))
                .select();
            results.push(r);
        }

        assert_eq!(
            results,
            vec![
                SelectResult::Matched("1".into()),
                SelectResult::Matched("2".into()),
                SelectResult::Matched("3".into()),
            ]
        );
    }

    // -- mixed types ------------------------------------------------------

    #[test]
    fn select_heterogeneous_channel_types() {
        let (tx_i, rx_i) = channel::unbounded::<i32>();
        let (tx_s, rx_s) = channel::unbounded::<String>();
        let (tx_b, rx_b) = channel::unbounded::<bool>();

        tx_s.send("yes".into()).unwrap();

        let result = Selector::new()
            .recv(&rx_i, |v| SelectResult::Matched(format!("int:{v}")))
            .recv(&rx_s, |v| SelectResult::Matched(format!("str:{v}")))
            .recv(&rx_b, |v| SelectResult::Matched(format!("bool:{v}")))
            .select();

        assert_eq!(result, SelectResult::Matched("str:yes".into()));

        // Now send on another type
        tx_b.send(true).unwrap();
        let result2 = Selector::new()
            .recv(&rx_i, |v| SelectResult::Matched(format!("int:{v}")))
            .recv(&rx_s, |v| SelectResult::Matched(format!("str:{v}")))
            .recv(&rx_b, |v| SelectResult::Matched(format!("bool:{v}")))
            .select();

        assert_eq!(result2, SelectResult::Matched("bool:true".into()));

        // Clean up unused senders
        drop(tx_i);
    }

    // -- panic on empty selector without default --------------------------

    #[test]
    #[should_panic(expected = "no channels and no default")]
    fn select_panics_with_no_channels_no_default() {
        Selector::new().select();
    }

    // -- default handler runs custom logic --------------------------------

    #[test]
    fn select_default_handler_custom_result() {
        let (_tx, rx) = channel::unbounded::<i32>();

        let result = Selector::new()
            .recv(&rx, |_v| SelectResult::Matched("msg".into()))
            .default_case(|| SelectResult::Matched("fallback".into()))
            .select();

        assert_eq!(result, SelectResult::Matched("fallback".into()));
    }

    // -- closed channel with buffered messages still drains ---------------

    #[test]
    fn select_closed_with_buffered_messages() {
        let (tx, rx) = channel::unbounded::<i32>();
        tx.send(10).unwrap();
        tx.send(20).unwrap();
        drop(tx); // close, but messages are buffered

        let r1 = Selector::new()
            .recv(&rx, |v| SelectResult::Matched(format!("{v}")))
            .select();
        assert_eq!(r1, SelectResult::Matched("10".into()));

        let r2 = Selector::new()
            .recv(&rx, |v| SelectResult::Matched(format!("{v}")))
            .select();
        assert_eq!(r2, SelectResult::Matched("20".into()));

        // Now truly empty and closed
        let r3 = Selector::new()
            .recv(&rx, |v| SelectResult::Matched(format!("{v}")))
            .select();
        assert_eq!(r3, SelectResult::Closed);
    }
}
