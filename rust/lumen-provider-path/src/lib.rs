//! Path utilities provider for Lumen tool dispatch.
//!
//! Implements the `ToolProvider` trait to expose path operations as tools:
//! - `path.join` — Join two path components
//! - `path.parent` — Parent directory
//! - `path.extension` — File extension
//! - `path.filename` — File name
//! - `path.stem` — File name without extension

use lumen_rt::services::tools::{ToolError, ToolProvider, ToolSchema};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathOp {
    Join,
    Parent,
    Extension,
    Filename,
    Stem,
}

impl PathOp {
    fn tool_name(&self) -> &'static str {
        match self {
            PathOp::Join => "path.join",
            PathOp::Parent => "path.parent",
            PathOp::Extension => "path.extension",
            PathOp::Filename => "path.filename",
            PathOp::Stem => "path.stem",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            PathOp::Join => "Join two path components",
            PathOp::Parent => "Return the parent directory of a path",
            PathOp::Extension => "Return the file extension of a path",
            PathOp::Filename => "Return the filename component of a path",
            PathOp::Stem => "Return the filename without extension",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JoinRequest {
    left: String,
    right: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PathRequest {
    path: String,
}

pub struct PathProvider {
    op: PathOp,
    schema: ToolSchema,
}

impl PathProvider {
    fn new(op: PathOp) -> Self {
        let schema = match op {
            PathOp::Join => ToolSchema {
                name: op.tool_name().to_string(),
                description: op.description().to_string(),
                input_schema: json!({
                    "type": "object",
                    "required": ["left", "right"],
                    "properties": {
                        "left": {"type": "string"},
                        "right": {"type": "string"}
                    }
                }),
                output_schema: json!({"type": "string"}),
                effects: vec!["fs".to_string()],
            },
            _ => ToolSchema {
                name: op.tool_name().to_string(),
                description: op.description().to_string(),
                input_schema: json!({
                    "type": "object",
                    "required": ["path"],
                    "properties": {
                        "path": {"type": "string"}
                    }
                }),
                output_schema: json!({"type": "string"}),
                effects: vec!["fs".to_string()],
            },
        };

        Self { op, schema }
    }

    pub fn join() -> Self {
        Self::new(PathOp::Join)
    }

    pub fn parent() -> Self {
        Self::new(PathOp::Parent)
    }

    pub fn extension() -> Self {
        Self::new(PathOp::Extension)
    }

    pub fn filename() -> Self {
        Self::new(PathOp::Filename)
    }

    pub fn stem() -> Self {
        Self::new(PathOp::Stem)
    }

    fn execute(&self, input: Value) -> Result<Value, ToolError> {
        match self.op {
            PathOp::Join => {
                let req: JoinRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let joined = Path::new(&req.left).join(&req.right);
                Ok(json!(joined.to_string_lossy().to_string()))
            }
            PathOp::Parent => {
                let req: PathRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let parent = Path::new(&req.path)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                Ok(json!(parent))
            }
            PathOp::Extension => {
                let req: PathRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let ext = Path::new(&req.path)
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                Ok(json!(ext))
            }
            PathOp::Filename => {
                let req: PathRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let name = Path::new(&req.path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                Ok(json!(name))
            }
            PathOp::Stem => {
                let req: PathRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let stem = Path::new(&req.path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                Ok(json!(stem))
            }
        }
    }
}

impl ToolProvider for PathProvider {
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
