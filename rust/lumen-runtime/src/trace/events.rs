//! Trace event types for the Lumen trace system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub seq: u64,
    pub kind: TraceEventKind,
    pub prev_hash: String,
    pub hash: String,
    pub timestamp: DateTime<Utc>,
    pub doc_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outputs_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TraceEventKind {
    RunStart,
    CellStart,
    CellEnd,
    ToolCall,
    SchemaValidate,
    Error,
    RunEnd,
}
