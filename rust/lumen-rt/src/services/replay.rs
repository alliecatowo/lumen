//! Deterministic replay recording and playback.
//!
//! During execution the [`ReplayRecorder`] captures every nondeterministic
//! operation — timestamps, random values, I/O results, tool call responses, and
//! UUID generations — into a [`ReplayLog`].  The log can be serialized to JSON
//! and loaded back for deterministic playback via [`ReplayPlayer`].
//!
//! # Modes
//!
//! [`ReplayMode`] selects the runtime behaviour:
//! - **Record** — capture events into a [`ReplayRecorder`].
//! - **Replay** — supply pre-recorded values from a [`ReplayPlayer`].
//! - **Live** — passthrough; nondeterministic operations execute normally.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// Replay events
// ---------------------------------------------------------------------------

/// A single nondeterministic event captured during execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReplayEvent {
    /// A timestamp observation (epoch seconds as f64 for sub-second precision).
    Timestamp(f64),
    /// A random value produced by the runtime RNG.
    Random(f64),
    /// An I/O operation identified by `key` that produced `result`.
    IoResult { key: String, result: String },
    /// A tool call response: tool name → JSON-encoded result value.
    ToolResponse {
        tool_name: String,
        result: serde_json::Value,
    },
    /// A generated UUID string.
    Uuid(String),
}

// ---------------------------------------------------------------------------
// Replay log
// ---------------------------------------------------------------------------

/// A serializable, ordered sequence of replay events.
///
/// The log additionally stores optional metadata (e.g. source file, run label)
/// so that humans can identify what execution produced it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayLog {
    /// Monotonic sequence of events in recording order.
    pub events: Vec<ReplayEvent>,
    /// Free-form metadata attached to the log.
    pub metadata: BTreeMap<String, String>,
}

impl ReplayLog {
    /// Create an empty replay log.
    pub fn new() -> Self {
        ReplayLog {
            events: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    /// Create a replay log pre-populated with events.
    pub fn from_events(events: Vec<ReplayEvent>) -> Self {
        ReplayLog {
            events,
            metadata: BTreeMap::new(),
        }
    }

    /// Serialize the log to a JSON string.
    pub fn to_json(&self) -> Result<String, ReplayError> {
        serde_json::to_string_pretty(self).map_err(|e| ReplayError::Serialize(e.to_string()))
    }

    /// Deserialize a log from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, ReplayError> {
        serde_json::from_str(json).map_err(|e| ReplayError::Deserialize(e.to_string()))
    }

    /// Save the log to a file (pretty-printed JSON).
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<(), ReplayError> {
        let json = self.to_json()?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load a log from a JSON file.
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, ReplayError> {
        let contents = std::fs::read_to_string(path)?;
        Self::from_json(&contents)
    }

    /// Number of events in the log.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether the log contains no events.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

impl Default for ReplayLog {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Replay mode
// ---------------------------------------------------------------------------

/// Selects the runtime's nondeterminism handling strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayMode {
    /// Capture nondeterministic events into a recorder.
    Record,
    /// Replay previously recorded events for deterministic execution.
    Replay,
    /// Passthrough — nondeterministic operations execute normally.
    Live,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors produced by replay recording / playback.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialize(String),
    #[error("deserialization error: {0}")]
    Deserialize(String),
    #[error("replay exhausted: no more events of kind `{expected}`")]
    Exhausted { expected: String },
    #[error("replay mismatch: expected {expected}, found {found}")]
    Mismatch { expected: String, found: String },
}

// ---------------------------------------------------------------------------
// Replay recorder
// ---------------------------------------------------------------------------

/// Records nondeterministic events during a live execution.
///
/// After execution finishes, call [`finish`](ReplayRecorder::finish) to obtain
/// a [`ReplayLog`] that can be saved and later replayed.
pub struct ReplayRecorder {
    log: ReplayLog,
}

impl ReplayRecorder {
    /// Start a new recording session.
    pub fn new() -> Self {
        ReplayRecorder {
            log: ReplayLog::new(),
        }
    }

    /// Record a timestamp event.
    pub fn record_timestamp(&mut self, ts: f64) {
        self.log.events.push(ReplayEvent::Timestamp(ts));
    }

    /// Record a random value event.
    pub fn record_random(&mut self, val: f64) {
        self.log.events.push(ReplayEvent::Random(val));
    }

    /// Record an I/O result.
    pub fn record_io(&mut self, key: String, result: String) {
        self.log.events.push(ReplayEvent::IoResult { key, result });
    }

    /// Record a tool call response.
    pub fn record_tool_response(&mut self, tool_name: String, result: serde_json::Value) {
        self.log
            .events
            .push(ReplayEvent::ToolResponse { tool_name, result });
    }

    /// Record a UUID generation.
    pub fn record_uuid(&mut self, uuid: String) {
        self.log.events.push(ReplayEvent::Uuid(uuid));
    }

    /// Set metadata on the log.
    pub fn set_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.log.metadata.insert(key.into(), value.into());
    }

    /// How many events have been recorded so far.
    pub fn event_count(&self) -> usize {
        self.log.events.len()
    }

    /// Read-only access to the events recorded so far.
    pub fn events(&self) -> &[ReplayEvent] {
        &self.log.events
    }

    /// Consume the recorder and return the finished [`ReplayLog`].
    pub fn finish(self) -> ReplayLog {
        self.log
    }
}

impl Default for ReplayRecorder {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Replay player
// ---------------------------------------------------------------------------

/// Plays back a recorded [`ReplayLog`], supplying deterministic values for
/// nondeterministic operations in the exact order they were recorded.
pub struct ReplayPlayer {
    log: ReplayLog,
    cursor: usize,
}

impl ReplayPlayer {
    /// Create a player from a previously recorded log.
    pub fn new(log: ReplayLog) -> Self {
        ReplayPlayer { log, cursor: 0 }
    }

    /// Load a player from a JSON file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ReplayError> {
        let log = ReplayLog::load_from_file(path)?;
        Ok(Self::new(log))
    }

    /// Current position in the event stream (0-indexed).
    pub fn position(&self) -> usize {
        self.cursor
    }

    /// Total number of events in the underlying log.
    pub fn total_events(&self) -> usize {
        self.log.events.len()
    }

    /// Number of events remaining.
    pub fn remaining(&self) -> usize {
        self.log.events.len().saturating_sub(self.cursor)
    }

    /// Whether all events have been consumed.
    pub fn is_exhausted(&self) -> bool {
        self.cursor >= self.log.events.len()
    }

    /// Peek at the next event without consuming it.
    pub fn peek(&self) -> Option<&ReplayEvent> {
        self.log.events.get(self.cursor)
    }

    /// Consume the next event, returning it regardless of variant.
    pub fn next_event(&mut self) -> Option<ReplayEvent> {
        if self.cursor < self.log.events.len() {
            let event = self.log.events[self.cursor].clone();
            self.cursor += 1;
            Some(event)
        } else {
            None
        }
    }

    /// Consume the next event, asserting it is a `Timestamp`.
    pub fn next_timestamp(&mut self) -> Result<f64, ReplayError> {
        match self.next_event() {
            Some(ReplayEvent::Timestamp(ts)) => Ok(ts),
            Some(other) => Err(ReplayError::Mismatch {
                expected: "Timestamp".into(),
                found: event_kind_name(&other),
            }),
            None => Err(ReplayError::Exhausted {
                expected: "Timestamp".into(),
            }),
        }
    }

    /// Consume the next event, asserting it is a `Random`.
    pub fn next_random(&mut self) -> Result<f64, ReplayError> {
        match self.next_event() {
            Some(ReplayEvent::Random(val)) => Ok(val),
            Some(other) => Err(ReplayError::Mismatch {
                expected: "Random".into(),
                found: event_kind_name(&other),
            }),
            None => Err(ReplayError::Exhausted {
                expected: "Random".into(),
            }),
        }
    }

    /// Consume the next event, asserting it is an `IoResult`.
    pub fn next_io(&mut self) -> Result<(String, String), ReplayError> {
        match self.next_event() {
            Some(ReplayEvent::IoResult { key, result }) => Ok((key, result)),
            Some(other) => Err(ReplayError::Mismatch {
                expected: "IoResult".into(),
                found: event_kind_name(&other),
            }),
            None => Err(ReplayError::Exhausted {
                expected: "IoResult".into(),
            }),
        }
    }

    /// Consume the next event, asserting it is a `ToolResponse`.
    pub fn next_tool_response(&mut self) -> Result<(String, serde_json::Value), ReplayError> {
        match self.next_event() {
            Some(ReplayEvent::ToolResponse { tool_name, result }) => Ok((tool_name, result)),
            Some(other) => Err(ReplayError::Mismatch {
                expected: "ToolResponse".into(),
                found: event_kind_name(&other),
            }),
            None => Err(ReplayError::Exhausted {
                expected: "ToolResponse".into(),
            }),
        }
    }

    /// Consume the next event, asserting it is a `Uuid`.
    pub fn next_uuid(&mut self) -> Result<String, ReplayError> {
        match self.next_event() {
            Some(ReplayEvent::Uuid(u)) => Ok(u),
            Some(other) => Err(ReplayError::Mismatch {
                expected: "Uuid".into(),
                found: event_kind_name(&other),
            }),
            None => Err(ReplayError::Exhausted {
                expected: "Uuid".into(),
            }),
        }
    }

    /// Reset the player to the beginning of the log.
    pub fn reset(&mut self) {
        self.cursor = 0;
    }

    /// Read-only access to the underlying log.
    pub fn log(&self) -> &ReplayLog {
        &self.log
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Human-readable name for an event variant (for error messages).
fn event_kind_name(event: &ReplayEvent) -> String {
    match event {
        ReplayEvent::Timestamp(_) => "Timestamp".into(),
        ReplayEvent::Random(_) => "Random".into(),
        ReplayEvent::IoResult { .. } => "IoResult".into(),
        ReplayEvent::ToolResponse { .. } => "ToolResponse".into(),
        ReplayEvent::Uuid(_) => "Uuid".into(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- ReplayRecorder tests -----------------------------------------------

    #[test]
    fn recorder_records_timestamp() {
        let mut rec = ReplayRecorder::new();
        rec.record_timestamp(1234.5);
        assert_eq!(rec.event_count(), 1);
        assert_eq!(rec.events()[0], ReplayEvent::Timestamp(1234.5));
    }

    #[test]
    fn recorder_records_random() {
        let mut rec = ReplayRecorder::new();
        rec.record_random(0.42);
        let log = rec.finish();
        assert_eq!(log.events[0], ReplayEvent::Random(0.42));
    }

    #[test]
    fn recorder_records_io() {
        let mut rec = ReplayRecorder::new();
        rec.record_io("read_file".into(), "contents".into());
        assert_eq!(
            rec.events()[0],
            ReplayEvent::IoResult {
                key: "read_file".into(),
                result: "contents".into(),
            }
        );
    }

    #[test]
    fn recorder_records_tool_response() {
        let mut rec = ReplayRecorder::new();
        let val = serde_json::json!({"status": 200});
        rec.record_tool_response("http_get".into(), val.clone());
        assert_eq!(
            rec.events()[0],
            ReplayEvent::ToolResponse {
                tool_name: "http_get".into(),
                result: val,
            }
        );
    }

    #[test]
    fn recorder_records_uuid() {
        let mut rec = ReplayRecorder::new();
        rec.record_uuid("550e8400-e29b-41d4-a716-446655440000".into());
        assert_eq!(
            rec.events()[0],
            ReplayEvent::Uuid("550e8400-e29b-41d4-a716-446655440000".into())
        );
    }

    #[test]
    fn recorder_metadata() {
        let mut rec = ReplayRecorder::new();
        rec.set_metadata("source", "main.lm.md");
        rec.set_metadata("run_id", "abc-123");
        let log = rec.finish();
        assert_eq!(log.metadata.get("source").unwrap(), "main.lm.md");
        assert_eq!(log.metadata.get("run_id").unwrap(), "abc-123");
    }

    #[test]
    fn recorder_mixed_events_preserve_order() {
        let mut rec = ReplayRecorder::new();
        rec.record_timestamp(1.0);
        rec.record_random(0.5);
        rec.record_uuid("u1".into());
        rec.record_io("k".into(), "v".into());
        rec.record_tool_response("t".into(), serde_json::json!(null));
        let log = rec.finish();
        assert_eq!(log.len(), 5);
        assert!(matches!(log.events[0], ReplayEvent::Timestamp(_)));
        assert!(matches!(log.events[1], ReplayEvent::Random(_)));
        assert!(matches!(log.events[2], ReplayEvent::Uuid(_)));
        assert!(matches!(log.events[3], ReplayEvent::IoResult { .. }));
        assert!(matches!(log.events[4], ReplayEvent::ToolResponse { .. }));
    }

    // -- ReplayLog tests ----------------------------------------------------

    #[test]
    fn log_json_round_trip() {
        let mut log = ReplayLog::new();
        log.events.push(ReplayEvent::Timestamp(99.9));
        log.events.push(ReplayEvent::Random(0.1));
        log.events.push(ReplayEvent::Uuid("u".into()));
        log.metadata.insert("key".into(), "val".into());

        let json = log.to_json().unwrap();
        let restored = ReplayLog::from_json(&json).unwrap();
        assert_eq!(log, restored);
    }

    #[test]
    fn log_file_round_trip() {
        let path =
            std::env::temp_dir().join(format!("lumen-replay-test-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let mut log = ReplayLog::new();
        log.events.push(ReplayEvent::Timestamp(42.0));
        log.events.push(ReplayEvent::ToolResponse {
            tool_name: "fetch".into(),
            result: serde_json::json!({"ok": true}),
        });
        log.save_to_file(&path).unwrap();

        let loaded = ReplayLog::load_from_file(&path).unwrap();
        assert_eq!(log, loaded);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn log_default_is_empty() {
        let log = ReplayLog::default();
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
    }

    #[test]
    fn log_from_events() {
        let events = vec![ReplayEvent::Random(0.7), ReplayEvent::Timestamp(100.0)];
        let log = ReplayLog::from_events(events.clone());
        assert_eq!(log.events, events);
        assert!(log.metadata.is_empty());
    }

    // -- ReplayPlayer tests -------------------------------------------------

    #[test]
    fn player_next_timestamp() {
        let log = ReplayLog::from_events(vec![ReplayEvent::Timestamp(77.7)]);
        let mut player = ReplayPlayer::new(log);
        assert_eq!(player.remaining(), 1);
        let ts = player.next_timestamp().unwrap();
        assert!((ts - 77.7).abs() < f64::EPSILON);
        assert!(player.is_exhausted());
    }

    #[test]
    fn player_next_random() {
        let log = ReplayLog::from_events(vec![ReplayEvent::Random(0.123)]);
        let mut player = ReplayPlayer::new(log);
        let val = player.next_random().unwrap();
        assert!((val - 0.123).abs() < f64::EPSILON);
    }

    #[test]
    fn player_next_io() {
        let log = ReplayLog::from_events(vec![ReplayEvent::IoResult {
            key: "stdin".into(),
            result: "hello".into(),
        }]);
        let mut player = ReplayPlayer::new(log);
        let (k, v) = player.next_io().unwrap();
        assert_eq!(k, "stdin");
        assert_eq!(v, "hello");
    }

    #[test]
    fn player_next_tool_response() {
        let val = serde_json::json!({"data": [1, 2, 3]});
        let log = ReplayLog::from_events(vec![ReplayEvent::ToolResponse {
            tool_name: "query".into(),
            result: val.clone(),
        }]);
        let mut player = ReplayPlayer::new(log);
        let (name, result) = player.next_tool_response().unwrap();
        assert_eq!(name, "query");
        assert_eq!(result, val);
    }

    #[test]
    fn player_next_uuid() {
        let log = ReplayLog::from_events(vec![ReplayEvent::Uuid("abc-def".into())]);
        let mut player = ReplayPlayer::new(log);
        assert_eq!(player.next_uuid().unwrap(), "abc-def");
    }

    #[test]
    fn player_mismatch_error() {
        let log = ReplayLog::from_events(vec![ReplayEvent::Timestamp(1.0)]);
        let mut player = ReplayPlayer::new(log);
        let err = player.next_random().unwrap_err();
        match err {
            ReplayError::Mismatch { expected, found } => {
                assert_eq!(expected, "Random");
                assert_eq!(found, "Timestamp");
            }
            other => panic!("expected Mismatch, got {:?}", other),
        }
    }

    #[test]
    fn player_exhausted_error() {
        let log = ReplayLog::new();
        let mut player = ReplayPlayer::new(log);
        let err = player.next_timestamp().unwrap_err();
        match err {
            ReplayError::Exhausted { expected } => {
                assert_eq!(expected, "Timestamp");
            }
            other => panic!("expected Exhausted, got {:?}", other),
        }
    }

    #[test]
    fn player_peek_does_not_advance() {
        let log =
            ReplayLog::from_events(vec![ReplayEvent::Timestamp(1.0), ReplayEvent::Random(0.5)]);
        let mut player = ReplayPlayer::new(log);
        assert_eq!(player.position(), 0);
        let peeked = player.peek().unwrap().clone();
        assert_eq!(player.position(), 0); // still at 0
        assert_eq!(peeked, ReplayEvent::Timestamp(1.0));
        // Now consume
        player.next_timestamp().unwrap();
        assert_eq!(player.position(), 1);
    }

    #[test]
    fn player_reset() {
        let log = ReplayLog::from_events(vec![ReplayEvent::Random(0.1), ReplayEvent::Random(0.2)]);
        let mut player = ReplayPlayer::new(log);
        player.next_random().unwrap();
        player.next_random().unwrap();
        assert!(player.is_exhausted());
        player.reset();
        assert_eq!(player.position(), 0);
        assert_eq!(player.remaining(), 2);
        let v = player.next_random().unwrap();
        assert!((v - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn player_full_sequence_replay() {
        // Record a full sequence, then replay it
        let mut rec = ReplayRecorder::new();
        rec.record_timestamp(1000.0);
        rec.record_random(0.42);
        rec.record_uuid("u-1".into());
        rec.record_io("file".into(), "data".into());
        rec.record_tool_response("tool1".into(), serde_json::json!("ok"));
        let log = rec.finish();

        let mut player = ReplayPlayer::new(log);
        assert_eq!(player.total_events(), 5);

        assert!((player.next_timestamp().unwrap() - 1000.0).abs() < f64::EPSILON);
        assert!((player.next_random().unwrap() - 0.42).abs() < f64::EPSILON);
        assert_eq!(player.next_uuid().unwrap(), "u-1");
        let (k, v) = player.next_io().unwrap();
        assert_eq!(k, "file");
        assert_eq!(v, "data");
        let (name, result) = player.next_tool_response().unwrap();
        assert_eq!(name, "tool1");
        assert_eq!(result, serde_json::json!("ok"));

        assert!(player.is_exhausted());
    }

    // -- ReplayMode tests ---------------------------------------------------

    #[test]
    fn replay_mode_equality() {
        assert_eq!(ReplayMode::Record, ReplayMode::Record);
        assert_eq!(ReplayMode::Replay, ReplayMode::Replay);
        assert_eq!(ReplayMode::Live, ReplayMode::Live);
        assert_ne!(ReplayMode::Record, ReplayMode::Replay);
        assert_ne!(ReplayMode::Replay, ReplayMode::Live);
    }
}
