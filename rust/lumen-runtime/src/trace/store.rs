//! JSONL trace file writer with hash-chaining.

use crate::trace::events::{TraceEvent, TraceEventKind};
use crate::trace::hasher::{canonical_json, sha256_hash};
use chrono::Utc;
use serde_json::json;
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

const TRACE_GENESIS_HASH: &str = "sha256:genesis";

impl TraceStore {
    pub fn new(base_dir: &Path) -> Self {
        let trace_dir = base_dir.join("trace");
        fs::create_dir_all(&trace_dir).ok();
        Self {
            trace_dir,
            current_run_id: String::new(),
            current_file: None,
            seq: 0,
            prev_hash: TRACE_GENESIS_HASH.to_string(),
            doc_hash: String::new(),
        }
    }

    pub fn start_run(&mut self, doc_hash: &str) -> String {
        let run_id = uuid::Uuid::new_v4().to_string();
        self.current_run_id = run_id.clone();
        self.doc_hash = doc_hash.to_string();
        self.seq = 0;
        self.prev_hash = TRACE_GENESIS_HASH.to_string();

        let path = self.trace_dir.join(format!("{}.jsonl", &run_id));
        self.current_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)
            .ok();

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

    pub fn call_enter(&mut self, cell_name: &str) {
        self.emit_event(TraceEventKind::CallEnter, Some(cell_name.to_string()), None);
    }

    pub fn call_exit(&mut self, cell_name: &str, result_type: &str) {
        let mut event = self.make_event(TraceEventKind::CallExit);
        event.cell = Some(cell_name.to_string());
        event.details = Some(json!({ "result_type": result_type }));
        self.write_event(&mut event);
    }

    pub fn vm_step(&mut self, cell: &str, ip: usize, opcode: &str) {
        let mut event = self.make_event(TraceEventKind::VmStep);
        event.cell = Some(cell.to_string());
        event.details = Some(json!({ "ip": ip, "opcode": opcode }));
        self.write_event(&mut event);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn tool_call(
        &mut self,
        cell: &str,
        tool_id: &str,
        tool_version: &str,
        latency_ms: u64,
        cached: bool,
        success: bool,
        message: Option<&str>,
    ) {
        let mut event = self.make_event(TraceEventKind::ToolCall);
        event.cell = Some(cell.to_string());
        event.tool_id = Some(tool_id.to_string());
        event.tool_version = Some(tool_version.to_string());
        event.latency_ms = Some(latency_ms);
        event.cached = Some(cached);
        event.details = Some(json!({ "success": success }));
        event.message = message.map(ToString::to_string);
        self.write_event(&mut event);
    }

    pub fn schema_validate(&mut self, cell: &str, schema: &str, valid: bool) {
        let mut event = self.make_event(TraceEventKind::SchemaValidate);
        event.cell = Some(cell.to_string());
        event.details = Some(json!({ "schema": schema, "valid": valid }));
        self.write_event(&mut event);
    }

    pub fn error(&mut self, cell: Option<&str>, message: &str) {
        let mut event = self.make_event(TraceEventKind::Error);
        event.cell = cell.map(|c| c.to_string());
        event.message = Some(message.to_string());
        self.write_event(&mut event);
    }

    pub fn run_id(&self) -> &str {
        &self.current_run_id
    }

    fn emit_event(&mut self, kind: TraceEventKind, cell: Option<String>, message: Option<String>) {
        let mut event = self.make_event(kind);
        event.cell = cell;
        event.message = message;
        self.write_event(&mut event);
    }

    fn make_event(&mut self, kind: TraceEventKind) -> TraceEvent {
        self.seq += 1;

        TraceEvent {
            seq: self.seq,
            kind,
            prev_hash: self.prev_hash.clone(),
            hash: String::new(),
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
            details: None,
            message: None,
        }
    }

    fn write_event(&mut self, event: &mut TraceEvent) {
        event.hash = compute_event_hash(event);
        self.prev_hash = event.hash.clone();
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
        TraceEventKind::CallEnter => "call_enter",
        TraceEventKind::CallExit => "call_exit",
        TraceEventKind::VmStep => "vm_step",
        TraceEventKind::ToolCall => "tool_call",
        TraceEventKind::SchemaValidate => "schema_validate",
        TraceEventKind::Error => "error",
        TraceEventKind::RunEnd => "run_end",
    }
}

fn event_payload(event: &TraceEvent) -> serde_json::Value {
    json!({
        "seq": event.seq,
        "kind": kind_str(&event.kind),
        "prev_hash": &event.prev_hash,
        "doc_hash": &event.doc_hash,
        "cell": &event.cell,
        "tool_id": &event.tool_id,
        "tool_version": &event.tool_version,
        "inputs_hash": &event.inputs_hash,
        "outputs_hash": &event.outputs_hash,
        "policy_hash": &event.policy_hash,
        "latency_ms": event.latency_ms,
        "cached": event.cached,
        "details": &event.details,
        "message": &event.message,
    })
}

pub fn compute_event_hash(event: &TraceEvent) -> String {
    let canonical = canonical_json(&event_payload(event));
    sha256_hash(&canonical)
}

pub fn verify_event_chain(events: &[TraceEvent]) -> Result<(), String> {
    let mut expected_seq = 1_u64;
    let mut expected_prev = TRACE_GENESIS_HASH.to_string();

    for event in events {
        if event.seq != expected_seq {
            return Err(format!(
                "trace sequence mismatch at seq {} (expected {})",
                event.seq, expected_seq
            ));
        }
        if event.prev_hash != expected_prev {
            return Err(format!(
                "trace hash chain mismatch at seq {} (expected prev '{}', got '{}')",
                event.seq, expected_prev, event.prev_hash
            ));
        }
        let expected_hash = compute_event_hash(event);
        if event.hash != expected_hash {
            return Err(format!(
                "trace event hash mismatch at seq {} (expected '{}', got '{}')",
                event.seq, expected_hash, event.hash
            ));
        }
        expected_seq += 1;
        expected_prev = event.hash.clone();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn trace_store_emits_structured_vm_events() {
        let base_dir =
            std::env::temp_dir().join(format!("lumen-trace-store-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&base_dir).expect("test temp dir should be created");

        let mut store = TraceStore::new(&base_dir);
        let run_id = store.start_run("doc-123");
        store.cell_start("main");
        store.call_enter("main");
        store.vm_step("main", 7, "ToolCall");
        store.tool_call("main", "http.get", "1.0.0", 12, false, true, None);
        store.schema_validate("main", "String", true);
        store.call_exit("main", "String");
        store.cell_end("main");
        store.end_run();

        let path = base_dir.join("trace").join(format!("{}.jsonl", run_id));
        let content = fs::read_to_string(&path).expect("trace file should be readable");
        let events: Vec<TraceEvent> = content
            .lines()
            .map(|line| serde_json::from_str(line).expect("trace event should deserialize"))
            .collect();

        let kinds: Vec<TraceEventKind> = events.iter().map(|event| event.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                TraceEventKind::RunStart,
                TraceEventKind::CellStart,
                TraceEventKind::CallEnter,
                TraceEventKind::VmStep,
                TraceEventKind::ToolCall,
                TraceEventKind::SchemaValidate,
                TraceEventKind::CallExit,
                TraceEventKind::CellEnd,
                TraceEventKind::RunEnd,
            ]
        );

        let step = events
            .iter()
            .find(|event| event.kind == TraceEventKind::VmStep)
            .expect("vm_step event should exist");
        assert_eq!(step.cell.as_deref(), Some("main"));
        assert_eq!(
            step.details
                .as_ref()
                .and_then(|d| d.get("ip"))
                .and_then(|v| v.as_u64()),
            Some(7)
        );
        assert_eq!(
            step.details
                .as_ref()
                .and_then(|d| d.get("opcode"))
                .and_then(|v| v.as_str()),
            Some("ToolCall")
        );

        let tool = events
            .iter()
            .find(|event| event.kind == TraceEventKind::ToolCall)
            .expect("tool_call event should exist");
        assert_eq!(tool.tool_id.as_deref(), Some("http.get"));
        assert_eq!(tool.tool_version.as_deref(), Some("1.0.0"));
        assert_eq!(tool.latency_ms, Some(12));
        assert_eq!(tool.cached, Some(false));
        assert_eq!(
            tool.details
                .as_ref()
                .and_then(|d| d.get("success"))
                .and_then(|v| v.as_bool()),
            Some(true)
        );

        let schema = events
            .iter()
            .find(|event| event.kind == TraceEventKind::SchemaValidate)
            .expect("schema_validate event should exist");
        assert_eq!(
            schema
                .details
                .as_ref()
                .and_then(|d| d.get("schema"))
                .and_then(|v| v.as_str()),
            Some("String")
        );
        assert_eq!(
            schema
                .details
                .as_ref()
                .and_then(|d| d.get("valid"))
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        verify_event_chain(&events).expect("generated trace should pass chain verification");

        fs::remove_dir_all(&base_dir).expect("test temp dir should be removed");
    }

    #[test]
    fn compute_event_hash_is_stable_across_timestamp_but_sensitive_to_payload() {
        let mut event = TraceEvent {
            seq: 2,
            kind: TraceEventKind::VmStep,
            prev_hash: "sha256:prev".to_string(),
            hash: String::new(),
            timestamp: Utc
                .timestamp_opt(1_700_000_000, 0)
                .single()
                .expect("timestamp should be valid"),
            doc_hash: "doc-123".to_string(),
            cell: Some("main".to_string()),
            tool_id: None,
            tool_version: None,
            inputs_hash: None,
            outputs_hash: None,
            policy_hash: None,
            latency_ms: None,
            cached: None,
            details: Some(json!({"ip": 1, "opcode": "LoadK"})),
            message: None,
        };

        let hash_a = compute_event_hash(&event);
        event.timestamp = Utc
            .timestamp_opt(1_700_000_001, 0)
            .single()
            .expect("timestamp should be valid");
        let hash_b = compute_event_hash(&event);
        assert_eq!(hash_a, hash_b, "hash should ignore wall-clock timestamp");

        event.message = Some("tampered".to_string());
        let hash_c = compute_event_hash(&event);
        assert_ne!(hash_a, hash_c, "hash should change when payload changes");
    }

    #[test]
    fn verify_event_chain_rejects_tampered_event_payload() {
        let base_dir = std::env::temp_dir().join(format!(
            "lumen-trace-store-verify-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&base_dir).expect("test temp dir should be created");

        let mut store = TraceStore::new(&base_dir);
        let run_id = store.start_run("doc-123");
        store.cell_start("main");
        store.vm_step("main", 1, "LoadK");
        store.end_run();

        let path = base_dir.join("trace").join(format!("{}.jsonl", run_id));
        let content = fs::read_to_string(&path).expect("trace file should be readable");
        let mut events: Vec<TraceEvent> = content
            .lines()
            .map(|line| serde_json::from_str(line).expect("trace event should deserialize"))
            .collect();

        verify_event_chain(&events).expect("fresh events should pass verification");

        let step = events
            .iter_mut()
            .find(|event| event.kind == TraceEventKind::VmStep)
            .expect("vm step event should exist");
        step.details = Some(json!({"ip": 999, "opcode": "LoadK"}));

        let err = verify_event_chain(&events).expect_err("tampered payload should be rejected");
        assert!(
            err.contains("trace event hash mismatch"),
            "unexpected error: {}",
            err
        );

        fs::remove_dir_all(&base_dir).expect("test temp dir should be removed");
    }
}
