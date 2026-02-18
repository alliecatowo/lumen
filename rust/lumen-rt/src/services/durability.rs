//! Durable log for recording nondeterministic events during execution.
//!
//! During a run the VM can [`append`](DurableLog::append) entries for every
//! nondeterministic operation (tool calls, timestamps, random values, external
//! input).  On replay the same log is fed back so that the replayed execution
//! is bit-for-bit identical to the original.

use crate::snapshot::SnapshotId;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

// ---------------------------------------------------------------------------
// Log entries
// ---------------------------------------------------------------------------

/// A single event recorded in the durable log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LogEntry {
    /// A checkpoint was taken at this point.
    Checkpoint(SnapshotId),
    /// A tool was invoked with arguments and produced a result.
    ToolCall {
        name: String,
        args: String,
        result: String,
    },
    /// External (non-deterministic) input arrived.
    ExternalInput { source: String, data: Vec<u8> },
    /// A timestamp was observed.
    Timestamp(u64),
    /// A random value was observed.
    Random(u64),
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum DurableLogError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialize(String),
    #[error("deserialization error: {0}")]
    Deserialize(String),
}

// ---------------------------------------------------------------------------
// Durable log
// ---------------------------------------------------------------------------

/// Append-only log for recording nondeterministic events during execution.
///
/// Entries are stored in memory and optionally flushed to a writer (file,
/// network, etc.).  For deterministic replay (T073) the log is replayed back
/// in order.
pub struct DurableLog {
    entries: Vec<LogEntry>,
    writer: Option<Box<dyn Write + Send>>,
}

impl DurableLog {
    /// Create an in-memory-only durable log.
    pub fn new() -> Self {
        DurableLog {
            entries: Vec::new(),
            writer: None,
        }
    }

    /// Create a durable log backed by the given file path.
    /// Each entry is JSON-lines encoded and flushed on append.
    pub fn with_file(path: impl AsRef<Path>) -> Result<Self, DurableLogError> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(DurableLog {
            entries: Vec::new(),
            writer: Some(Box::new(std::io::BufWriter::new(file))),
        })
    }

    /// Create a durable log with a custom writer (useful for testing).
    pub fn with_writer(writer: Box<dyn Write + Send>) -> Self {
        DurableLog {
            entries: Vec::new(),
            writer: Some(writer),
        }
    }

    /// Append an entry to the log.
    ///
    /// If a writer is attached, the entry is serialized as a JSON line and
    /// flushed immediately so that crash recovery never loses committed entries.
    pub fn append(&mut self, entry: LogEntry) -> Result<(), DurableLogError> {
        if let Some(ref mut w) = self.writer {
            let json = serde_json::to_string(&entry)
                .map_err(|e| DurableLogError::Serialize(e.to_string()))?;
            writeln!(w, "{}", json)?;
            w.flush()?;
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Read-only access to all recorded entries.
    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    /// Number of entries recorded so far.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the log is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Construct a log pre-populated with entries for replay.
    /// No writer is attached — replay is read-only.
    pub fn replay_from(entries: Vec<LogEntry>) -> Self {
        DurableLog {
            entries,
            writer: None,
        }
    }

    /// Load a durable log from a JSON-lines file.
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, DurableLogError> {
        let contents = std::fs::read_to_string(path)?;
        let mut entries = Vec::new();
        for line in contents.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let entry: LogEntry = serde_json::from_str(line)
                .map_err(|e| DurableLogError::Deserialize(e.to_string()))?;
            entries.push(entry);
        }
        Ok(DurableLog {
            entries,
            writer: None,
        })
    }
}

impl Default for DurableLog {
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
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    fn temp_path(suffix: &str) -> PathBuf {
        let mut p = env::temp_dir();
        p.push(format!(
            "lumen-durlog-test-{}-{}.jsonl",
            suffix,
            std::process::id()
        ));
        let _ = fs::remove_file(&p);
        p
    }

    #[test]
    fn append_and_read_entries() {
        let mut log = DurableLog::new();
        assert!(log.is_empty());

        log.append(LogEntry::Timestamp(1000)).unwrap();
        log.append(LogEntry::Random(42)).unwrap();
        log.append(LogEntry::ToolCall {
            name: "http_get".into(),
            args: r#"{"url":"https://example.com"}"#.into(),
            result: r#"{"status":200}"#.into(),
        })
        .unwrap();

        assert_eq!(log.len(), 3);
        assert_eq!(log.entries()[0], LogEntry::Timestamp(1000));
        assert_eq!(log.entries()[1], LogEntry::Random(42));
        match &log.entries()[2] {
            LogEntry::ToolCall { name, .. } => assert_eq!(name, "http_get"),
            other => panic!("expected ToolCall, got {:?}", other),
        }
    }

    #[test]
    fn replay_from_entries() {
        let entries = vec![
            LogEntry::Timestamp(500),
            LogEntry::Checkpoint(SnapshotId(1)),
            LogEntry::ExternalInput {
                source: "stdin".into(),
                data: b"hello".to_vec(),
            },
        ];
        let log = DurableLog::replay_from(entries.clone());
        assert_eq!(log.len(), 3);
        assert_eq!(log.entries(), &entries[..]);
    }

    #[test]
    fn file_persistence() {
        let path = temp_path("persist");
        {
            let mut log = DurableLog::with_file(&path).unwrap();
            log.append(LogEntry::Timestamp(111)).unwrap();
            log.append(LogEntry::Random(222)).unwrap();
        }
        // Re-read from file.
        let loaded = DurableLog::load_from_file(&path).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.entries()[0], LogEntry::Timestamp(111));
        assert_eq!(loaded.entries()[1], LogEntry::Random(222));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn file_persistence_tool_call() {
        let path = temp_path("persist-tool");
        {
            let mut log = DurableLog::with_file(&path).unwrap();
            log.append(LogEntry::ToolCall {
                name: "fetch".into(),
                args: "{}".into(),
                result: "ok".into(),
            })
            .unwrap();
        }
        let loaded = DurableLog::load_from_file(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        match &loaded.entries()[0] {
            LogEntry::ToolCall { name, result, .. } => {
                assert_eq!(name, "fetch");
                assert_eq!(result, "ok");
            }
            other => panic!("expected ToolCall, got {:?}", other),
        }
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn custom_writer() {
        let buf: Vec<u8> = Vec::new();
        let cursor = std::io::Cursor::new(buf);
        let mut log = DurableLog::with_writer(Box::new(cursor));
        log.append(LogEntry::Timestamp(999)).unwrap();
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn default_is_empty() {
        let log = DurableLog::default();
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
    }
}
