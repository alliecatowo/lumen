//! Process provider for Lumen tool dispatch.
//!
//! Implements the `ToolProvider` trait to expose process operations as tools:
//! - `process.exec` — Execute a command
//! - `process.exit` — Exit the process
//! - `process.stdin` — Read all stdin
//! - `process.read_line` — Read a single line from stdin

use lumen_rt::services::tools::{ToolError, ToolProvider, ToolSchema};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Read};
use std::process::{Command, Stdio};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessOp {
    Exec,
    Exit,
    Stdin,
    ReadLine,
}

impl ProcessOp {
    fn tool_name(&self) -> &'static str {
        match self {
            ProcessOp::Exec => "process.exec",
            ProcessOp::Exit => "process.exit",
            ProcessOp::Stdin => "process.stdin",
            ProcessOp::ReadLine => "process.read_line",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            ProcessOp::Exec => "Execute a shell command and return stdout, stderr, and status",
            ProcessOp::Exit => "Exit the process with a status code",
            ProcessOp::Stdin => "Read all stdin to a string",
            ProcessOp::ReadLine => "Read a single line from stdin",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExecRequest {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: std::collections::HashMap<String, String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    stdin: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExitRequest {
    code: i32,
}

#[derive(Debug, Error)]
enum ExecError {
    #[error("failed to parse command: {0}")]
    Parse(String),
    #[error("failed to spawn process: {0}")]
    Spawn(String),
    #[error("failed to read output: {0}")]
    Output(String),
}

pub struct ProcessProvider {
    op: ProcessOp,
    schema: ToolSchema,
}

impl ProcessProvider {
    fn new(op: ProcessOp) -> Self {
        let schema = match op {
            ProcessOp::Exec => ToolSchema {
                name: op.tool_name().to_string(),
                description: op.description().to_string(),
                input_schema: json!({
                    "type": "object",
                    "required": ["command"],
                    "properties": {
                        "command": {"type": "string"},
                        "args": {"type": "array", "items": {"type": "string"}},
                        "env": {"type": "object", "additionalProperties": {"type": "string"}},
                        "cwd": {"type": "string"},
                        "stdin": {"type": "string"}
                    }
                }),
                output_schema: json!({
                    "type": "object",
                    "required": ["status", "stdout", "stderr"],
                    "properties": {
                        "status": {"type": "number"},
                        "stdout": {"type": "string"},
                        "stderr": {"type": "string"}
                    }
                }),
                effects: vec!["process".to_string()],
            },
            ProcessOp::Exit => ToolSchema {
                name: op.tool_name().to_string(),
                description: op.description().to_string(),
                input_schema: json!({
                    "type": "object",
                    "required": ["code"],
                    "properties": {"code": {"type": "number"}}
                }),
                output_schema: json!({"type": "null"}),
                effects: vec!["process".to_string()],
            },
            ProcessOp::Stdin | ProcessOp::ReadLine => ToolSchema {
                name: op.tool_name().to_string(),
                description: op.description().to_string(),
                input_schema: json!({"type": "object", "properties": {}}),
                output_schema: json!({"type": "string"}),
                effects: vec!["process".to_string()],
            },
        };

        Self { op, schema }
    }

    pub fn exec() -> Self {
        Self::new(ProcessOp::Exec)
    }

    pub fn exit() -> Self {
        Self::new(ProcessOp::Exit)
    }

    pub fn stdin() -> Self {
        Self::new(ProcessOp::Stdin)
    }

    pub fn read_line() -> Self {
        Self::new(ProcessOp::ReadLine)
    }

    fn run_exec(req: ExecRequest) -> Result<Value, ToolError> {
        let mut command = if req.args.is_empty() {
            let mut parts = shell_words::split(&req.command).map_err(|e| {
                ToolError::InvocationFailed(ExecError::Parse(e.to_string()).to_string())
            })?;
            if parts.is_empty() {
                return Err(ToolError::InvocationFailed(
                    ExecError::Parse("empty command".to_string()).to_string(),
                ));
            }
            let program = parts.remove(0);
            let mut cmd = Command::new(program);
            cmd.args(parts);
            cmd
        } else {
            let mut cmd = Command::new(&req.command);
            cmd.args(&req.args);
            cmd
        };

        if let Some(cwd) = req.cwd {
            command.current_dir(cwd);
        }

        if !req.env.is_empty() {
            command.envs(req.env);
        }

        if let Some(stdin) = req.stdin {
            command.stdin(Stdio::piped());
            let mut child = command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| {
                    ToolError::InvocationFailed(ExecError::Spawn(e.to_string()).to_string())
                })?;
            if let Some(mut handle) = child.stdin.take() {
                use std::io::Write;
                handle.write_all(stdin.as_bytes()).map_err(|e| {
                    ToolError::InvocationFailed(ExecError::Output(e.to_string()).to_string())
                })?;
            }
            let output = child.wait_with_output().map_err(|e| {
                ToolError::InvocationFailed(ExecError::Output(e.to_string()).to_string())
            })?;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let status = output.status.code().unwrap_or(-1);
            Ok(json!({"status": status, "stdout": stdout, "stderr": stderr}))
        } else {
            let output = command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .map_err(|e| {
                    ToolError::InvocationFailed(ExecError::Output(e.to_string()).to_string())
                })?;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let status = output.status.code().unwrap_or(-1);
            Ok(json!({"status": status, "stdout": stdout, "stderr": stderr}))
        }
    }

    fn execute(&self, input: Value) -> Result<Value, ToolError> {
        match self.op {
            ProcessOp::Exec => {
                let req: ExecRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                Self::run_exec(req)
            }
            ProcessOp::Exit => {
                let req: ExitRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                std::process::exit(req.code);
            }
            ProcessOp::Stdin => {
                let mut buf = String::new();
                io::stdin()
                    .read_to_string(&mut buf)
                    .map_err(|e| ToolError::InvocationFailed(e.to_string()))?;
                Ok(json!(buf))
            }
            ProcessOp::ReadLine => {
                let mut buf = String::new();
                let stdin = io::stdin();
                let mut handle = stdin.lock();
                let bytes = handle
                    .read_line(&mut buf)
                    .map_err(|e| ToolError::InvocationFailed(e.to_string()))?;
                if bytes == 0 {
                    Ok(json!(""))
                } else {
                    Ok(json!(buf.trim_end_matches(['\n', '\r']).to_string()))
                }
            }
        }
    }
}

impl ToolProvider for ProcessProvider {
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
