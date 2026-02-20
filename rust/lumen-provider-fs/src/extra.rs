//! Extended filesystem provider operations.
//!
//! Provides additional tools:
//! - fs.read_lines
//! - fs.walk
//! - fs.glob

use lumen_rt::services::tools::{ToolError, ToolProvider, ToolSchema};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExtraOp {
    ReadLines,
    Walk,
    Glob,
}

impl ExtraOp {
    fn tool_name(&self) -> &'static str {
        match self {
            ExtraOp::ReadLines => "fs.read_lines",
            ExtraOp::Walk => "fs.walk",
            ExtraOp::Glob => "fs.glob",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            ExtraOp::ReadLines => "Read a file and return lines as a list of strings",
            ExtraOp::Walk => "Recursively list all file paths in a directory",
            ExtraOp::Glob => "Find files matching a glob pattern",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PathRequest {
    path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GlobRequest {
    pattern: String,
}

pub struct FsExtraProvider {
    op: ExtraOp,
    schema: ToolSchema,
}

impl FsExtraProvider {
    fn new(op: ExtraOp) -> Self {
        let (input_schema, output_schema) = match op {
            ExtraOp::ReadLines | ExtraOp::Walk => (
                json!({
                    "type": "object",
                    "required": ["path"],
                    "properties": {"path": {"type": "string"}}
                }),
                json!({"type": "array", "items": {"type": "string"}}),
            ),
            ExtraOp::Glob => (
                json!({
                    "type": "object",
                    "required": ["pattern"],
                    "properties": {"pattern": {"type": "string"}}
                }),
                json!({"type": "array", "items": {"type": "string"}}),
            ),
        };

        let schema = ToolSchema {
            name: op.tool_name().to_string(),
            description: op.description().to_string(),
            input_schema,
            output_schema,
            effects: vec!["fs".to_string()],
        };

        Self { op, schema }
    }

    pub fn read_lines() -> Self {
        Self::new(ExtraOp::ReadLines)
    }

    pub fn walk() -> Self {
        Self::new(ExtraOp::Walk)
    }

    pub fn glob() -> Self {
        Self::new(ExtraOp::Glob)
    }

    fn execute(&self, input: Value) -> Result<Value, ToolError> {
        match self.op {
            ExtraOp::ReadLines => {
                let req: PathRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let content = std::fs::read_to_string(&req.path)
                    .map_err(|e| ToolError::InvocationFailed(format!("read failed: {}", e)))?;
                let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                Ok(json!(lines))
            }
            ExtraOp::Walk => {
                let req: PathRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let mut results = Vec::new();
                let root = Path::new(&req.path);
                if root.exists() {
                    let mut stack = vec![root.to_path_buf()];
                    while let Some(path) = stack.pop() {
                        if path.is_dir() {
                            let entries = std::fs::read_dir(&path).map_err(|e| {
                                ToolError::InvocationFailed(format!("walk failed: {}", e))
                            })?;
                            for entry in entries.flatten() {
                                stack.push(entry.path());
                            }
                        } else {
                            results.push(path.to_string_lossy().to_string());
                        }
                    }
                }
                Ok(json!(results))
            }
            ExtraOp::Glob => {
                let req: GlobRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let mut results = Vec::new();
                for entry in glob::glob(&req.pattern)
                    .map_err(|e| ToolError::InvocationFailed(format!("glob failed: {}", e)))?
                {
                    if let Ok(path) = entry {
                        results.push(path.to_string_lossy().to_string());
                    }
                }
                Ok(json!(results))
            }
        }
    }
}

impl ToolProvider for FsExtraProvider {
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
