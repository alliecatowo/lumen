//! Actor trait and process interface for the Lumen runtime.
//!
//! This module provides a standard actor abstraction — a mailbox + state +
//! handler interface — that integrates with the existing process, channel, and
//! scheduler infrastructure.
//!
//! # Design
//!
//! Each actor runs on its own OS thread with a dedicated mailbox (crossbeam
//! channel receiver). Messages are processed sequentially — there is no
//! concurrent access to actor state. An [`ActorRef`] is the send-side handle
//! and is `Clone`-able so multiple producers can send messages.
//!
//! Graceful shutdown occurs when:
//! - The handler returns [`ActorResult::Stop`] or [`ActorResult::StopWithError`].
//! - All [`ActorRef`] handles are dropped (channel disconnects).
//! - [`ActorRef::stop`] is called (sends an internal stop signal).
//!
//! Each spawned actor is assigned a unique [`ProcessId`] for integration with
//! the existing process management system.

use crate::services::process::ProcessId;

use crossbeam_channel::{self as cb};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

// ---------------------------------------------------------------------------
// ActorResult
// ---------------------------------------------------------------------------

/// The result of handling a message.
///
/// Determines whether the actor continues processing or stops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActorResult<S> {
    /// Continue processing with the updated state.
    Continue(S),
    /// Stop the actor gracefully with the final state.
    Stop(S),
    /// Stop the actor with an error message and the final state.
    StopWithError(S, String),
}

// ---------------------------------------------------------------------------
// Actor trait
// ---------------------------------------------------------------------------

/// Core actor trait — implement to create an actor.
///
/// The associated types define the message and state types. All three must be
/// `Send + 'static` to allow the actor to run on its own thread.
pub trait Actor: Send + 'static {
    /// The type of messages this actor can receive.
    type Message: Send + 'static;
    /// The type of the actor's internal state.
    type State: Send + 'static;

    /// Initialize the actor's state. Called once when the actor is spawned.
    fn init(&self) -> Self::State;

    /// Handle a single message, returning an updated state (or a stop signal).
    fn handle(&self, msg: Self::Message, state: Self::State) -> ActorResult<Self::State>;

    /// Called when the actor is stopping. Override for cleanup logic.
    fn on_stop(&self, _state: &Self::State) {}
}

// ---------------------------------------------------------------------------
// ActorError
// ---------------------------------------------------------------------------

/// Errors that can occur when interacting with an actor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActorError {
    /// The actor has stopped and can no longer accept messages.
    Stopped,
    /// The actor stopped with an error.
    ActorFailed(String),
    /// The actor panicked.
    Panicked(String),
}

impl fmt::Display for ActorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActorError::Stopped => write!(f, "actor has stopped"),
            ActorError::ActorFailed(msg) => write!(f, "actor failed: {}", msg),
            ActorError::Panicked(msg) => write!(f, "actor panicked: {}", msg),
        }
    }
}

impl std::error::Error for ActorError {}

// ---------------------------------------------------------------------------
// Envelope — internal message wrapper
// ---------------------------------------------------------------------------

/// Internal envelope that wraps user messages and control signals.
enum Envelope<M> {
    /// A user-supplied message.
    Msg(M),
    /// Request the actor to stop gracefully.
    Stop,
}

// ---------------------------------------------------------------------------
// ActorRef
// ---------------------------------------------------------------------------

/// A handle to a running actor — used to send messages.
///
/// `ActorRef` is cheaply cloneable. When all clones are dropped (and no
/// explicit stop was sent), the actor's mailbox disconnects and the actor
/// will stop after processing any remaining buffered messages.
pub struct ActorRef<M: Send + 'static> {
    sender: cb::Sender<Envelope<M>>,
    id: ProcessId,
    stopped: Arc<AtomicBool>,
}

impl<M: Send + 'static> Clone for ActorRef<M> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            id: self.id,
            stopped: Arc::clone(&self.stopped),
        }
    }
}

impl<M: Send + 'static> fmt::Debug for ActorRef<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActorRef")
            .field("id", &self.id)
            .field("stopped", &self.stopped.load(Ordering::Acquire))
            .finish()
    }
}

impl<M: Send + 'static> ActorRef<M> {
    /// Send a message to the actor.
    ///
    /// Returns `Err(ActorError::Stopped)` if the actor has already stopped.
    pub fn send(&self, msg: M) -> Result<(), ActorError> {
        self.sender
            .send(Envelope::Msg(msg))
            .map_err(|_| ActorError::Stopped)
    }

    /// Request the actor to stop gracefully.
    ///
    /// The actor will finish processing its current message, call `on_stop`,
    /// and then exit. Returns `Err` if the actor has already stopped.
    pub fn stop(&self) -> Result<(), ActorError> {
        self.sender
            .send(Envelope::Stop)
            .map_err(|_| ActorError::Stopped)
    }

    /// Return this actor's [`ProcessId`].
    pub fn id(&self) -> ProcessId {
        self.id
    }

    /// Return `true` if the actor has stopped.
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }
}

// ---------------------------------------------------------------------------
// spawn_actor
// ---------------------------------------------------------------------------

/// Spawn an actor on a new OS thread, returning an [`ActorRef`] handle.
///
/// The actor's `init` method is called on the new thread, and then the actor
/// enters a message-processing loop. Returns a join handle alongside the
/// `ActorRef` so callers can wait for the actor to finish.
pub fn spawn_actor<A: Actor>(
    actor: A,
) -> (ActorRef<A::Message>, JoinHandle<Result<(), ActorError>>) {
    let (tx, rx) = cb::unbounded::<Envelope<A::Message>>();
    let id = ProcessId::next();
    let stopped = Arc::new(AtomicBool::new(false));
    let stopped_flag = Arc::clone(&stopped);

    let handle = thread::Builder::new()
        .name(format!("actor-{}", id))
        .spawn(move || {
            let mut state = actor.init();

            loop {
                match rx.recv() {
                    Ok(Envelope::Msg(msg)) => match actor.handle(msg, state) {
                        ActorResult::Continue(s) => {
                            state = s;
                        }
                        ActorResult::Stop(s) => {
                            actor.on_stop(&s);
                            stopped_flag.store(true, Ordering::Release);
                            return Ok(());
                        }
                        ActorResult::StopWithError(s, err) => {
                            actor.on_stop(&s);
                            stopped_flag.store(true, Ordering::Release);
                            return Err(ActorError::ActorFailed(err));
                        }
                    },
                    Ok(Envelope::Stop) => {
                        actor.on_stop(&state);
                        stopped_flag.store(true, Ordering::Release);
                        return Ok(());
                    }
                    Err(_) => {
                        // All senders dropped — graceful shutdown.
                        actor.on_stop(&state);
                        stopped_flag.store(true, Ordering::Release);
                        return Ok(());
                    }
                }
            }
        })
        .expect("failed to spawn actor thread");

    let actor_ref = ActorRef {
        sender: tx,
        id,
        stopped,
    };

    (actor_ref, handle)
}

// ---------------------------------------------------------------------------
// ActorHandle — trait-object interface for ActorSystem
// ---------------------------------------------------------------------------

/// Type-erased handle to a running actor, used by [`ActorSystem`].
///
/// This trait allows the system to manage actors with different message types
/// in a single collection.
pub trait ActorHandle: Send {
    /// Return the actor's [`ProcessId`].
    fn id(&self) -> ProcessId;

    /// Return `true` if the actor has stopped.
    fn is_stopped(&self) -> bool;

    /// Request the actor to stop gracefully.
    fn request_stop(&self) -> Result<(), ActorError>;

    /// Wait for the actor thread to finish and return its result.
    fn join(self: Box<Self>) -> Result<(), ActorError>;
}

/// Concrete implementation of [`ActorHandle`] for a specific message type.
struct TypedActorHandle<M: Send + 'static> {
    actor_ref: ActorRef<M>,
    join_handle: Option<JoinHandle<Result<(), ActorError>>>,
}

impl<M: Send + 'static> ActorHandle for TypedActorHandle<M> {
    fn id(&self) -> ProcessId {
        self.actor_ref.id()
    }

    fn is_stopped(&self) -> bool {
        self.actor_ref.is_stopped()
    }

    fn request_stop(&self) -> Result<(), ActorError> {
        self.actor_ref.stop()
    }

    fn join(mut self: Box<Self>) -> Result<(), ActorError> {
        // Drop the ActorRef sender so the actor sees a disconnect if stop
        // wasn't explicitly sent.
        drop(self.actor_ref);
        if let Some(jh) = self.join_handle.take() {
            match jh.join() {
                Ok(result) => result,
                Err(panic_payload) => {
                    let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "unknown panic".to_string()
                    };
                    Err(ActorError::Panicked(msg))
                }
            }
        } else {
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// ActorSystem
// ---------------------------------------------------------------------------

/// An actor system that manages multiple actors.
///
/// Actors are spawned into the system and tracked by their [`ProcessId`].
/// The system provides bulk lifecycle operations (stop all, wait for all).
pub struct ActorSystem {
    actors: Vec<Box<dyn ActorHandle>>,
}

impl ActorSystem {
    /// Create a new, empty actor system.
    pub fn new() -> Self {
        Self { actors: Vec::new() }
    }

    /// Spawn an actor into this system, returning its [`ActorRef`].
    pub fn spawn<A: Actor>(&mut self, actor: A) -> ActorRef<A::Message> {
        let (actor_ref, join_handle) = spawn_actor(actor);
        let handle = TypedActorHandle {
            actor_ref: actor_ref.clone(),
            join_handle: Some(join_handle),
        };
        self.actors.push(Box::new(handle));
        actor_ref
    }

    /// Return the number of actors managed by this system.
    pub fn actor_count(&self) -> usize {
        self.actors.len()
    }

    /// Return the number of actors that are still running.
    pub fn running_count(&self) -> usize {
        self.actors.iter().filter(|a| !a.is_stopped()).count()
    }

    /// Request all actors to stop gracefully.
    ///
    /// Sends a stop signal to each actor. Actors that have already stopped
    /// are silently skipped.
    pub fn stop_all(&self) {
        for actor in &self.actors {
            let _ = actor.request_stop();
        }
    }

    /// Stop all actors and wait for them to finish.
    ///
    /// Returns a list of errors from actors that failed. An empty vec means
    /// all actors stopped cleanly.
    pub fn shutdown(self) -> Vec<ActorError> {
        let mut errors = Vec::new();
        for actor in self.actors {
            let _ = actor.request_stop();
            if let Err(e) = actor.join() {
                errors.push(e);
            }
        }
        errors
    }

    /// Check if a specific actor (by [`ProcessId`]) is still running.
    pub fn is_running(&self, id: ProcessId) -> Option<bool> {
        self.actors
            .iter()
            .find(|a| a.id() == id)
            .map(|a| !a.is_stopped())
    }
}

impl Default for ActorSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for ActorSystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActorSystem")
            .field("actor_count", &self.actors.len())
            .field("running_count", &self.running_count())
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

    // -- Test helpers: a simple counter actor ------------------------------

    /// A simple actor that accumulates a running sum.
    struct CounterActor {
        initial: i64,
    }

    impl Actor for CounterActor {
        type Message = i64;
        type State = i64;

        fn init(&self) -> Self::State {
            self.initial
        }

        fn handle(&self, msg: Self::Message, state: Self::State) -> ActorResult<Self::State> {
            if msg < 0 {
                ActorResult::Stop(state)
            } else {
                ActorResult::Continue(state + msg)
            }
        }
    }

    /// An actor that tracks on_stop calls via an Arc<AtomicBool>.
    struct LifecycleActor {
        stopped_flag: Arc<AtomicBool>,
    }

    impl Actor for LifecycleActor {
        type Message = String;
        type State = Vec<String>;

        fn init(&self) -> Self::State {
            Vec::new()
        }

        fn handle(&self, msg: Self::Message, mut state: Self::State) -> ActorResult<Self::State> {
            if msg == "stop" {
                ActorResult::Stop(state)
            } else if msg == "fail" {
                ActorResult::StopWithError(state, "deliberate failure".to_string())
            } else {
                state.push(msg);
                ActorResult::Continue(state)
            }
        }

        fn on_stop(&self, _state: &Self::State) {
            self.stopped_flag.store(true, Ordering::Release);
        }
    }

    /// An actor that echoes the total number of messages received, used
    /// for concurrency tests.
    struct EchoCountActor {
        counter: Arc<AtomicUsize>,
    }

    impl Actor for EchoCountActor {
        type Message = ();
        type State = usize;

        fn init(&self) -> Self::State {
            0
        }

        fn handle(&self, _msg: Self::Message, state: Self::State) -> ActorResult<Self::State> {
            let new_state = state + 1;
            self.counter.store(new_state, AtomicOrdering::Release);
            ActorResult::Continue(new_state)
        }
    }

    // =====================================================================
    // 1. Basic spawn and message send
    // =====================================================================
    #[test]
    fn basic_spawn_and_send() {
        let (actor_ref, handle) = spawn_actor(CounterActor { initial: 0 });
        actor_ref.send(10).unwrap();
        actor_ref.send(20).unwrap();
        actor_ref.send(-1).unwrap(); // triggers Stop
        let result = handle.join().unwrap();
        assert!(result.is_ok());
    }

    // =====================================================================
    // 2. Actor processes messages sequentially
    // =====================================================================
    #[test]
    fn sequential_message_processing() {
        let order = Arc::new(std::sync::Mutex::new(Vec::new()));

        struct OrderActor {
            order: Arc<std::sync::Mutex<Vec<u32>>>,
        }
        impl Actor for OrderActor {
            type Message = u32;
            type State = ();
            fn init(&self) -> Self::State {}
            fn handle(&self, msg: Self::Message, _state: Self::State) -> ActorResult<Self::State> {
                self.order.lock().unwrap().push(msg);
                ActorResult::Continue(())
            }
        }

        let (actor_ref, handle) = spawn_actor(OrderActor {
            order: Arc::clone(&order),
        });
        for i in 0..100 {
            actor_ref.send(i).unwrap();
        }
        drop(actor_ref);
        handle.join().unwrap().unwrap();

        let recorded = order.lock().unwrap();
        let expected: Vec<u32> = (0..100).collect();
        assert_eq!(*recorded, expected);
    }

    // =====================================================================
    // 3. ActorRef is cloneable — multiple senders
    // =====================================================================
    #[test]
    fn actor_ref_clone_multiple_senders() {
        let counter = Arc::new(AtomicUsize::new(0));
        let (actor_ref, handle) = spawn_actor(EchoCountActor {
            counter: Arc::clone(&counter),
        });

        let mut join_handles = vec![];
        for _ in 0..5 {
            let r = actor_ref.clone();
            join_handles.push(thread::spawn(move || {
                for _ in 0..20 {
                    r.send(()).unwrap();
                }
            }));
        }

        for jh in join_handles {
            jh.join().unwrap();
        }
        drop(actor_ref);
        handle.join().unwrap().unwrap();

        assert_eq!(counter.load(AtomicOrdering::Acquire), 100);
    }

    // =====================================================================
    // 4. Graceful stop via ActorRef::stop()
    // =====================================================================
    #[test]
    fn graceful_stop_via_ref() {
        let stopped = Arc::new(AtomicBool::new(false));
        let (actor_ref, handle) = spawn_actor(LifecycleActor {
            stopped_flag: Arc::clone(&stopped),
        });

        actor_ref.send("hello".to_string()).unwrap();
        actor_ref.stop().unwrap();
        handle.join().unwrap().unwrap();

        assert!(stopped.load(Ordering::Acquire));
    }

    // =====================================================================
    // 5. Graceful shutdown on all refs dropped
    // =====================================================================
    #[test]
    fn shutdown_on_all_refs_dropped() {
        let stopped = Arc::new(AtomicBool::new(false));
        let (actor_ref, handle) = spawn_actor(LifecycleActor {
            stopped_flag: Arc::clone(&stopped),
        });

        actor_ref.send("msg1".to_string()).unwrap();
        drop(actor_ref); // last ref dropped → channel disconnects
        handle.join().unwrap().unwrap();

        assert!(stopped.load(Ordering::Acquire));
    }

    // =====================================================================
    // 6. Stop via message handler returning ActorResult::Stop
    // =====================================================================
    #[test]
    fn stop_from_handler() {
        let stopped = Arc::new(AtomicBool::new(false));
        let (actor_ref, handle) = spawn_actor(LifecycleActor {
            stopped_flag: Arc::clone(&stopped),
        });

        actor_ref.send("stop".to_string()).unwrap();
        handle.join().unwrap().unwrap();

        assert!(stopped.load(Ordering::Acquire));
    }

    // =====================================================================
    // 7. StopWithError propagates error
    // =====================================================================
    #[test]
    fn stop_with_error_propagates() {
        let stopped = Arc::new(AtomicBool::new(false));
        let (actor_ref, handle) = spawn_actor(LifecycleActor {
            stopped_flag: Arc::clone(&stopped),
        });

        actor_ref.send("fail".to_string()).unwrap();
        let result = handle.join().unwrap();
        assert!(result.is_err());
        match result.unwrap_err() {
            ActorError::ActorFailed(msg) => {
                assert_eq!(msg, "deliberate failure");
            }
            other => panic!("expected ActorFailed, got {:?}", other),
        }

        assert!(stopped.load(Ordering::Acquire));
    }

    // =====================================================================
    // 8. Sending to a stopped actor returns error
    // =====================================================================
    #[test]
    fn send_to_stopped_actor_errors() {
        let (actor_ref, handle) = spawn_actor(CounterActor { initial: 0 });
        actor_ref.send(-1).unwrap(); // stop
        handle.join().unwrap().unwrap();

        // Actor has stopped — further sends should fail.
        // Give the stopped flag a moment to propagate.
        thread::sleep(Duration::from_millis(10));
        let result = actor_ref.send(42);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ActorError::Stopped);
    }

    // =====================================================================
    // 9. ProcessId integration
    // =====================================================================
    #[test]
    fn actor_has_unique_process_id() {
        let (ref1, h1) = spawn_actor(CounterActor { initial: 0 });
        let (ref2, h2) = spawn_actor(CounterActor { initial: 0 });

        assert_ne!(ref1.id(), ref2.id());
        assert!(ref1.id().as_u64() > 0);
        assert!(ref2.id().as_u64() > 0);

        drop(ref1);
        drop(ref2);
        h1.join().unwrap().unwrap();
        h2.join().unwrap().unwrap();
    }

    // =====================================================================
    // 10. is_stopped reflects actor state
    // =====================================================================
    #[test]
    fn is_stopped_reflects_state() {
        let (actor_ref, handle) = spawn_actor(CounterActor { initial: 0 });
        assert!(!actor_ref.is_stopped());

        actor_ref.send(-1).unwrap(); // stop
        handle.join().unwrap().unwrap();

        assert!(actor_ref.is_stopped());
    }

    // =====================================================================
    // 11. ActorRef Debug format
    // =====================================================================
    #[test]
    fn actor_ref_debug_format() {
        let (actor_ref, handle) = spawn_actor(CounterActor { initial: 0 });
        let dbg = format!("{:?}", actor_ref);
        assert!(dbg.contains("ActorRef"));
        assert!(dbg.contains("stopped: false"));

        drop(actor_ref);
        handle.join().unwrap().unwrap();
    }

    // =====================================================================
    // 12. ActorError Display
    // =====================================================================
    #[test]
    fn actor_error_display() {
        let e1 = ActorError::Stopped;
        assert!(e1.to_string().contains("stopped"));

        let e2 = ActorError::ActorFailed("boom".to_string());
        assert!(e2.to_string().contains("boom"));

        let e3 = ActorError::Panicked("oops".to_string());
        assert!(e3.to_string().contains("oops"));
    }

    // =====================================================================
    // 13. ActorSystem — spawn and manage
    // =====================================================================
    #[test]
    fn actor_system_spawn_and_manage() {
        let mut system = ActorSystem::new();
        assert_eq!(system.actor_count(), 0);

        let ref1 = system.spawn(CounterActor { initial: 0 });
        let ref2 = system.spawn(CounterActor { initial: 10 });

        assert_eq!(system.actor_count(), 2);

        ref1.send(5).unwrap();
        ref2.send(5).unwrap();

        // Stop actors
        ref1.send(-1).unwrap();
        ref2.send(-1).unwrap();

        // Brief wait for actors to process
        thread::sleep(Duration::from_millis(50));

        let errors = system.shutdown();
        assert!(errors.is_empty());
    }

    // =====================================================================
    // 14. ActorSystem — stop_all
    // =====================================================================
    #[test]
    fn actor_system_stop_all() {
        let mut system = ActorSystem::new();

        let _ref1 = system.spawn(CounterActor { initial: 0 });
        let _ref2 = system.spawn(CounterActor { initial: 0 });

        system.stop_all();
        thread::sleep(Duration::from_millis(50));

        let errors = system.shutdown();
        assert!(errors.is_empty());
    }

    // =====================================================================
    // 15. ActorSystem — running_count
    // =====================================================================
    #[test]
    fn actor_system_running_count() {
        let mut system = ActorSystem::new();

        let ref1 = system.spawn(CounterActor { initial: 0 });
        let _ref2 = system.spawn(CounterActor { initial: 0 });

        // Both should be running initially
        // (brief sleep to ensure threads have started)
        thread::sleep(Duration::from_millis(10));
        assert_eq!(system.running_count(), 2);

        // Stop first actor
        ref1.send(-1).unwrap();
        thread::sleep(Duration::from_millis(50));

        assert_eq!(system.running_count(), 1);

        let errors = system.shutdown();
        assert!(errors.is_empty());
    }

    // =====================================================================
    // 16. ActorSystem — is_running by ProcessId
    // =====================================================================
    #[test]
    fn actor_system_is_running_by_id() {
        let mut system = ActorSystem::new();
        let ref1 = system.spawn(CounterActor { initial: 0 });
        let pid = ref1.id();

        assert_eq!(system.is_running(pid), Some(true));

        ref1.send(-1).unwrap();
        thread::sleep(Duration::from_millis(50));

        assert_eq!(system.is_running(pid), Some(false));

        // Unknown PID
        let fake_pid = ProcessId::next();
        assert_eq!(system.is_running(fake_pid), None);

        system.shutdown();
    }

    // =====================================================================
    // 17. ActorSystem — Default trait
    // =====================================================================
    #[test]
    fn actor_system_default() {
        let system = ActorSystem::default();
        assert_eq!(system.actor_count(), 0);
        assert_eq!(system.running_count(), 0);
    }

    // =====================================================================
    // 18. ActorSystem — Debug format
    // =====================================================================
    #[test]
    fn actor_system_debug_format() {
        let mut system = ActorSystem::new();
        let _ref1 = system.spawn(CounterActor { initial: 0 });

        let dbg = format!("{:?}", system);
        assert!(dbg.contains("ActorSystem"));
        assert!(dbg.contains("actor_count: 1"));

        system.shutdown();
    }

    // =====================================================================
    // 19. High-throughput: many messages from many senders
    // =====================================================================
    #[test]
    fn high_throughput_many_messages() {
        let counter = Arc::new(AtomicUsize::new(0));
        let (actor_ref, handle) = spawn_actor(EchoCountActor {
            counter: Arc::clone(&counter),
        });

        let num_senders = 10;
        let msgs_per_sender = 100;
        let mut join_handles = vec![];

        for _ in 0..num_senders {
            let r = actor_ref.clone();
            join_handles.push(thread::spawn(move || {
                for _ in 0..msgs_per_sender {
                    r.send(()).unwrap();
                }
            }));
        }

        for jh in join_handles {
            jh.join().unwrap();
        }
        drop(actor_ref);
        handle.join().unwrap().unwrap();

        assert_eq!(
            counter.load(AtomicOrdering::Acquire),
            num_senders * msgs_per_sender
        );
    }

    // =====================================================================
    // 20. on_stop is called exactly once
    // =====================================================================
    #[test]
    fn on_stop_called_once() {
        let stop_count = Arc::new(AtomicUsize::new(0));

        struct OnStopCounter {
            count: Arc<AtomicUsize>,
        }

        impl Actor for OnStopCounter {
            type Message = ();
            type State = ();
            fn init(&self) -> Self::State {}
            fn handle(&self, _msg: Self::Message, state: Self::State) -> ActorResult<Self::State> {
                ActorResult::Continue(state)
            }
            fn on_stop(&self, _state: &Self::State) {
                self.count.fetch_add(1, AtomicOrdering::Relaxed);
            }
        }

        let (actor_ref, handle) = spawn_actor(OnStopCounter {
            count: Arc::clone(&stop_count),
        });

        actor_ref.send(()).unwrap();
        actor_ref.send(()).unwrap();
        actor_ref.stop().unwrap();
        handle.join().unwrap().unwrap();

        assert_eq!(stop_count.load(AtomicOrdering::Acquire), 1);
    }

    // =====================================================================
    // 21. ActorResult variants
    // =====================================================================
    #[test]
    fn actor_result_variants() {
        let cont: ActorResult<i32> = ActorResult::Continue(42);
        let stop: ActorResult<i32> = ActorResult::Stop(42);
        let err: ActorResult<i32> = ActorResult::StopWithError(42, "oops".to_string());

        assert_eq!(cont, ActorResult::Continue(42));
        assert_eq!(stop, ActorResult::Stop(42));
        assert_eq!(err, ActorResult::StopWithError(42, "oops".to_string()));

        // Debug
        let dbg = format!("{:?}", cont);
        assert!(dbg.contains("Continue"));
    }

    // =====================================================================
    // 22. Actor with complex state
    // =====================================================================
    #[test]
    fn actor_with_complex_state() {
        struct Aggregator;

        #[derive(Debug)]
        enum AggMsg {
            Add(String),
            Done,
        }

        impl Actor for Aggregator {
            type Message = AggMsg;
            type State = Vec<String>;

            fn init(&self) -> Self::State {
                Vec::new()
            }

            fn handle(
                &self,
                msg: Self::Message,
                mut state: Self::State,
            ) -> ActorResult<Self::State> {
                match msg {
                    AggMsg::Add(s) => {
                        state.push(s);
                        ActorResult::Continue(state)
                    }
                    AggMsg::Done => ActorResult::Stop(state),
                }
            }
        }

        let (actor_ref, handle) = spawn_actor(Aggregator);
        actor_ref.send(AggMsg::Add("alpha".to_string())).unwrap();
        actor_ref.send(AggMsg::Add("beta".to_string())).unwrap();
        actor_ref.send(AggMsg::Add("gamma".to_string())).unwrap();
        actor_ref.send(AggMsg::Done).unwrap();

        let result = handle.join().unwrap();
        assert!(result.is_ok());
    }

    // =====================================================================
    // 23. Multiple actors in system with different types
    // =====================================================================
    #[test]
    fn system_heterogeneous_actors() {
        struct StringActor;
        impl Actor for StringActor {
            type Message = String;
            type State = String;
            fn init(&self) -> Self::State {
                String::new()
            }
            fn handle(
                &self,
                msg: Self::Message,
                mut state: Self::State,
            ) -> ActorResult<Self::State> {
                state.push_str(&msg);
                ActorResult::Continue(state)
            }
        }

        let mut system = ActorSystem::new();

        // Spawn actors with different message types
        let int_ref = system.spawn(CounterActor { initial: 0 });
        let str_ref = system.spawn(StringActor);

        int_ref.send(42).unwrap();
        str_ref.send("hello".to_string()).unwrap();

        system.stop_all();
        let errors = system.shutdown();
        assert!(errors.is_empty());
    }

    // =====================================================================
    // 24. ActorSystem shutdown collects errors
    // =====================================================================
    #[test]
    fn system_shutdown_collects_errors() {
        let mut system = ActorSystem::new();

        let stopped = Arc::new(AtomicBool::new(false));
        let ref1 = system.spawn(LifecycleActor {
            stopped_flag: Arc::clone(&stopped),
        });

        // Trigger a failure
        ref1.send("fail".to_string()).unwrap();
        thread::sleep(Duration::from_millis(50));

        let errors = system.shutdown();
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            ActorError::ActorFailed(msg) => {
                assert!(msg.contains("deliberate failure"));
            }
            other => panic!("expected ActorFailed, got {:?}", other),
        }
    }
}
