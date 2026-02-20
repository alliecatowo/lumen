//! Time provider for Lumen tool dispatch.
//!
//! Implements `ToolProvider` for:
//! - `time.format` — format an epoch timestamp with a format string

use chrono::{TimeZone, Utc};
use lumen_rt::services::tools::{ToolError, ToolProvider, ToolSchema};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimeOp {
    Format,
}

impl TimeOp {
    fn tool_name(&self) -> &'static str {
        match self {
            TimeOp::Format => "time.format",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            TimeOp::Format => "Format an epoch timestamp with a format string",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FormatRequest {
    timestamp: i64,
    format: String,
}

pub struct TimeProvider {
    op: TimeOp,
    schema: ToolSchema,
}

impl TimeProvider {
    fn new(op: TimeOp) -> Self {
        let schema = ToolSchema {
            name: op.tool_name().to_string(),
            description: op.description().to_string(),
            input_schema: json!({
                "type": "object",
                "required": ["timestamp", "format"],
                "properties": {
                    "timestamp": {"type": "number"},
                    "format": {"type": "string"}
                }
            }),
            output_schema: json!({"type": "string"}),
            effects: vec!["time".to_string()],
        };

        Self { op, schema }
    }

    pub fn format() -> Self {
        Self::new(TimeOp::Format)
    }

    fn execute(&self, input: Value) -> Result<Value, ToolError> {
        match self.op {
            TimeOp::Format => {
                let req: FormatRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let dt = Utc
                    .timestamp_opt(req.timestamp, 0)
                    .single()
                    .ok_or_else(|| ToolError::InvocationFailed("invalid timestamp".to_string()))?;
                Ok(json!(dt.format(&req.format).to_string()))
            }
        }
    }
}

impl ToolProvider for TimeProvider {
    fn name(&self) -> &str {
        self.op.tool_name()
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn schema(&self) -> &ToolSchema {
        &self.schema
    }

    fn call(&self, input: Value) -> Result<Value, ToolError> {
        self.execute(input)
    }
}
