//! JSONL trace file writer with hash-chaining.

use crate::trace::events::{TraceEvent, TraceEventKind};
use crate::trace::hasher::sha256_hash;
use chrono::Utc;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct TraceStore {
    trace_dir: PathBuf,
    current_run_id: String,
    current_file: Option<File>,
    seq: u64,
    prev_hash: String,
    doc_hash: String,
}

impl TraceStore {
    pub fn new(base_dir: &Path) -> Self {
        let trace_dir = base_dir.join("trace");
        fs::create_dir_all(&trace_dir).ok();
        Self {
            trace_dir,
            current_run_id: String::new(),
            current_file: None,
            seq: 0,
            prev_hash: "sha256:genesis".to_string(),
            doc_hash: String::new(),
        }
    }

    pub fn start_run(&mut self, doc_hash: &str) -> String {
        let run_id = uuid::Uuid::new_v4().to_string();
        self.current_run_id = run_id.clone();
        self.doc_hash = doc_hash.to_string();
        self.seq = 0;
        self.prev_hash = "sha256:genesis".to_string();

        let path = self.trace_dir.join(format!("{}.jsonl", &run_id));
        self.current_file = OpenOptions::new().create(true).write(true).open(path).ok();

        self.emit_event(TraceEventKind::RunStart, None, None);
        run_id
    }

    pub fn end_run(&mut self) {
        self.emit_event(TraceEventKind::RunEnd, None, None);
        self.current_file = None;
    }

    pub fn cell_start(&mut self, cell_name: &str) {
        self.emit_event(TraceEventKind::CellStart, Some(cell_name.to_string()), None);
    }

    pub fn cell_end(&mut self, cell_name: &str) {
        self.emit_event(TraceEventKind::CellEnd, Some(cell_name.to_string()), None);
    }

    pub fn tool_call(&mut self, cell: &str, tool_id: &str, latency_ms: u64, cached: bool) {
        let mut event = self.make_event(TraceEventKind::ToolCall);
        event.cell = Some(cell.to_string());
        event.tool_id = Some(tool_id.to_string());
        event.latency_ms = Some(latency_ms);
        event.cached = Some(cached);
        self.write_event(&event);
    }

    pub fn error(&mut self, cell: Option<&str>, message: &str) {
        let mut event = self.make_event(TraceEventKind::Error);
        event.cell = cell.map(|c| c.to_string());
        event.message = Some(message.to_string());
        self.write_event(&event);
    }

    pub fn run_id(&self) -> &str {
        &self.current_run_id
    }

    fn emit_event(&mut self, kind: TraceEventKind, cell: Option<String>, message: Option<String>) {
        let mut event = self.make_event(kind);
        event.cell = cell;
        event.message = message;
        self.write_event(&event);
    }

    fn make_event(&mut self, kind: TraceEventKind) -> TraceEvent {
        self.seq += 1;
        let content = format!("{}:{}:{}", self.seq, kind_str(&kind), &self.prev_hash);
        let hash = sha256_hash(&content);

        let event = TraceEvent {
            seq: self.seq,
            kind,
            prev_hash: self.prev_hash.clone(),
            hash: hash.clone(),
            timestamp: Utc::now(),
            doc_hash: self.doc_hash.clone(),
            cell: None,
            tool_id: None,
            tool_version: None,
            inputs_hash: None,
            outputs_hash: None,
            policy_hash: None,
            latency_ms: None,
            cached: None,
            message: None,
        };
        self.prev_hash = hash;
        event
    }

    fn write_event(&mut self, event: &TraceEvent) {
        if let Some(ref mut file) = self.current_file {
            if let Ok(json) = serde_json::to_string(event) {
                writeln!(file, "{}", json).ok();
            }
        }
    }
}

fn kind_str(kind: &TraceEventKind) -> &'static str {
    match kind {
        TraceEventKind::RunStart => "run_start",
        TraceEventKind::CellStart => "cell_start",
        TraceEventKind::CellEnd => "cell_end",
        TraceEventKind::ToolCall => "tool_call",
        TraceEventKind::SchemaValidate => "schema_validate",
        TraceEventKind::Error => "error",
        TraceEventKind::RunEnd => "run_end",
    }
}
