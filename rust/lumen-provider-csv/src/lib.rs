//! CSV provider for Lumen tool dispatch.
//!
//! Implements the `ToolProvider` trait to expose CSV operations as tools:
//! - `csv.parse` — Parse CSV to list of rows
//! - `csv.encode` — Encode rows to CSV

use csv::{ReaderBuilder, WriterBuilder};
use lumen_rt::services::tools::{ToolError, ToolProvider, ToolSchema};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CsvOp {
    Parse,
    Encode,
}

impl CsvOp {
    fn tool_name(&self) -> &'static str {
        match self {
            CsvOp::Parse => "csv.parse",
            CsvOp::Encode => "csv.encode",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            CsvOp::Parse => "Parse a CSV string into a list of string rows",
            CsvOp::Encode => "Encode a list of string rows into CSV",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ParseRequest {
    input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EncodeRequest {
    rows: Vec<Vec<String>>,
}

pub struct CsvProvider {
    op: CsvOp,
    schema: ToolSchema,
}

impl CsvProvider {
    fn new(op: CsvOp) -> Self {
        let (input_schema, output_schema) = match op {
            CsvOp::Parse => (
                json!({
                    "type": "object",
                    "required": ["input"],
                    "properties": {"input": {"type": "string"}}
                }),
                json!({
                    "type": "array",
                    "items": {"type": "array", "items": {"type": "string"}}
                }),
            ),
            CsvOp::Encode => (
                json!({
                    "type": "object",
                    "required": ["rows"],
                    "properties": {
                        "rows": {"type": "array", "items": {"type": "array", "items": {"type": "string"}}}
                    }
                }),
                json!({"type": "string"}),
            ),
        };

        let schema = ToolSchema {
            name: op.tool_name().to_string(),
            description: op.description().to_string(),
            input_schema,
            output_schema,
            effects: vec!["format".to_string()],
        };

        Self { op, schema }
    }

    pub fn parse() -> Self {
        Self::new(CsvOp::Parse)
    }

    pub fn encode() -> Self {
        Self::new(CsvOp::Encode)
    }

    fn execute(&self, input: Value) -> Result<Value, ToolError> {
        match self.op {
            CsvOp::Parse => {
                let req: ParseRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let mut reader = ReaderBuilder::new()
                    .has_headers(false)
                    .from_reader(req.input.as_bytes());
                let mut rows: Vec<Vec<String>> = Vec::new();
                for record in reader.records() {
                    let record = record.map_err(|e| {
                        ToolError::InvocationFailed(format!("csv parse failed: {}", e))
                    })?;
                    rows.push(record.iter().map(|s| s.to_string()).collect());
                }
                Ok(json!(rows))
            }
            CsvOp::Encode => {
                let req: EncodeRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let mut writer = WriterBuilder::new().from_writer(Vec::new());
                for row in req.rows {
                    writer.write_record(row).map_err(|e| {
                        ToolError::InvocationFailed(format!("csv encode failed: {}", e))
                    })?;
                }
                let out = writer.into_inner().map_err(|e| {
                    ToolError::InvocationFailed(format!("csv encode failed: {}", e))
                })?;
                Ok(json!(String::from_utf8_lossy(&out).to_string()))
            }
        }
    }
}

impl ToolProvider for CsvProvider {
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
