//! TOML provider for Lumen tool dispatch.
//!
//! Implements the `ToolProvider` trait to expose TOML operations as tools:
//! - `toml.parse` — Parse TOML to JSON value
//! - `toml.encode` — Encode JSON value to TOML

use lumen_rt::services::tools::{ToolError, ToolProvider, ToolSchema};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TomlOp {
    Parse,
    Encode,
}

impl TomlOp {
    fn tool_name(&self) -> &'static str {
        match self {
            TomlOp::Parse => "toml.parse",
            TomlOp::Encode => "toml.encode",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            TomlOp::Parse => "Parse TOML to a JSON-compatible value",
            TomlOp::Encode => "Encode a JSON-compatible value to TOML",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ParseRequest {
    input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EncodeRequest {
    value: Value,
}

pub struct TomlProvider {
    op: TomlOp,
    schema: ToolSchema,
}

impl TomlProvider {
    fn new(op: TomlOp) -> Self {
        let (input_schema, output_schema) = match op {
            TomlOp::Parse => (
                json!({
                    "type": "object",
                    "required": ["input"],
                    "properties": {"input": {"type": "string"}}
                }),
                json!({"type": "object"}),
            ),
            TomlOp::Encode => (
                json!({
                    "type": "object",
                    "required": ["value"],
                    "properties": {"value": {"type": ["object", "array", "string", "number", "boolean", "null"]}}
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
        Self::new(TomlOp::Parse)
    }

    pub fn encode() -> Self {
        Self::new(TomlOp::Encode)
    }

    fn execute(&self, input: Value) -> Result<Value, ToolError> {
        match self.op {
            TomlOp::Parse => {
                let req: ParseRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let value: toml::Value = toml::from_str(&req.input).map_err(|e| {
                    ToolError::InvocationFailed(format!("toml parse failed: {}", e))
                })?;
                let json_value = serde_json::to_value(value).map_err(|e| {
                    ToolError::InvocationFailed(format!("toml parse failed: {}", e))
                })?;
                Ok(json_value)
            }
            TomlOp::Encode => {
                let req: EncodeRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let toml_value: toml::Value = serde_json::from_value(req.value).map_err(|e| {
                    ToolError::InvocationFailed(format!("toml encode failed: {}", e))
                })?;
                let out = toml::to_string(&toml_value).map_err(|e| {
                    ToolError::InvocationFailed(format!("toml encode failed: {}", e))
                })?;
                Ok(json!(out))
            }
        }
    }
}

impl ToolProvider for TomlProvider {
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
