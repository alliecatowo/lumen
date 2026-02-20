//! Regex provider for Lumen tool dispatch.
//!
//! Implements the `ToolProvider` trait to expose regex operations as tools:
//! - `regex.match` — First match capture groups
//! - `regex.replace` — Replace all matches
//! - `regex.find_all` — Return all matches

use lumen_rt::services::tools::{ToolError, ToolProvider, ToolSchema};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegexOp {
    Match,
    Replace,
    FindAll,
}

impl RegexOp {
    fn tool_name(&self) -> &'static str {
        match self {
            RegexOp::Match => "regex.match",
            RegexOp::Replace => "regex.replace",
            RegexOp::FindAll => "regex.find_all",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            RegexOp::Match => "Return capture groups for the first regex match",
            RegexOp::Replace => "Replace all regex matches in a string",
            RegexOp::FindAll => "Return all regex matches as a list of strings",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MatchRequest {
    pattern: String,
    text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReplaceRequest {
    pattern: String,
    text: String,
    replacement: String,
}

pub struct RegexProvider {
    op: RegexOp,
    schema: ToolSchema,
}

impl RegexProvider {
    fn new(op: RegexOp) -> Self {
        let (input_schema, output_schema) = match op {
            RegexOp::Match => (
                json!({
                    "type": "object",
                    "required": ["pattern", "text"],
                    "properties": {
                        "pattern": {"type": "string"},
                        "text": {"type": "string"}
                    }
                }),
                json!({"type": "array", "items": {"type": "string"}}),
            ),
            RegexOp::Replace => (
                json!({
                    "type": "object",
                    "required": ["pattern", "text", "replacement"],
                    "properties": {
                        "pattern": {"type": "string"},
                        "text": {"type": "string"},
                        "replacement": {"type": "string"}
                    }
                }),
                json!({"type": "string"}),
            ),
            RegexOp::FindAll => (
                json!({
                    "type": "object",
                    "required": ["pattern", "text"],
                    "properties": {
                        "pattern": {"type": "string"},
                        "text": {"type": "string"}
                    }
                }),
                json!({"type": "array", "items": {"type": "string"}}),
            ),
        };

        let schema = ToolSchema {
            name: op.tool_name().to_string(),
            description: op.description().to_string(),
            input_schema,
            output_schema,
            effects: vec!["regex".to_string()],
        };

        Self { op, schema }
    }

    pub fn regex_match() -> Self {
        Self::new(RegexOp::Match)
    }

    pub fn regex_replace() -> Self {
        Self::new(RegexOp::Replace)
    }

    pub fn regex_find_all() -> Self {
        Self::new(RegexOp::FindAll)
    }

    fn execute(&self, input: Value) -> Result<Value, ToolError> {
        match self.op {
            RegexOp::Match => {
                let req: MatchRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let re = Regex::new(&req.pattern)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid regex: {}", e)))?;
                let caps = match re.captures(&req.text) {
                    Some(caps) => caps,
                    None => return Ok(json!([])),
                };
                let mut out = Vec::new();
                for i in 0..caps.len() {
                    out.push(
                        caps.get(i)
                            .map(|m| m.as_str().to_string())
                            .unwrap_or_default(),
                    );
                }
                Ok(json!(out))
            }
            RegexOp::Replace => {
                let req: ReplaceRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let re = Regex::new(&req.pattern)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid regex: {}", e)))?;
                Ok(json!(re
                    .replace_all(&req.text, req.replacement.as_str())
                    .to_string()))
            }
            RegexOp::FindAll => {
                let req: MatchRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let re = Regex::new(&req.pattern)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid regex: {}", e)))?;
                let matches: Vec<String> = re
                    .find_iter(&req.text)
                    .map(|m| m.as_str().to_string())
                    .collect();
                Ok(json!(matches))
            }
        }
    }
}

impl ToolProvider for RegexProvider {
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
