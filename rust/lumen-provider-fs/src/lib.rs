//! Filesystem provider for Lumen tool dispatch.
//!
//! Implements the `ToolProvider` trait to expose filesystem operations as tools:
//! - `fs.read` — Read file to string
//! - `fs.write` — Write string to file
//! - `fs.exists` — Check if path exists
//! - `fs.list` — List directory entries
//! - `fs.mkdir` — Create directory (recursive)
//! - `fs.remove` — Remove file or empty directory

use lumen_rt::services::tools::{ToolError, ToolProvider, ToolSchema};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

// ---------------------------------------------------------------------------
// Operation enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FsOp {
    Read,
    Write,
    Exists,
    List,
    Mkdir,
    Remove,
}

impl FsOp {
    fn tool_name(&self) -> &'static str {
        match self {
            FsOp::Read => "fs.read",
            FsOp::Write => "fs.write",
            FsOp::Exists => "fs.exists",
            FsOp::List => "fs.list",
            FsOp::Mkdir => "fs.mkdir",
            FsOp::Remove => "fs.remove",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            FsOp::Read => "Read file contents as a string",
            FsOp::Write => "Write string content to a file",
            FsOp::Exists => "Check if a path exists",
            FsOp::List => "List directory entries",
            FsOp::Mkdir => "Create directory recursively",
            FsOp::Remove => "Remove file or empty directory",
        }
    }
}

// ---------------------------------------------------------------------------
// Request/Response schemas
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReadRequest {
    path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WriteRequest {
    path: String,
    content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PathRequest {
    path: String,
}

// ---------------------------------------------------------------------------
// FsProvider implementation
// ---------------------------------------------------------------------------

/// Filesystem provider implementing the `ToolProvider` trait.
pub struct FsProvider {
    op: FsOp,
    schema: ToolSchema,
}

impl FsProvider {
    /// Create a new filesystem provider for the given operation.
    fn new(op: FsOp) -> Self {
        let schema = match op {
            FsOp::Read => ToolSchema {
                name: op.tool_name().to_string(),
                description: op.description().to_string(),
                input_schema: json!({
                    "type": "object",
                    "required": ["path"],
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to read"
                        }
                    }
                }),
                output_schema: json!({
                    "type": "string",
                    "description": "File content as a string"
                }),
                effects: vec!["fs".to_string()],
            },
            FsOp::Write => ToolSchema {
                name: op.tool_name().to_string(),
                description: op.description().to_string(),
                input_schema: json!({
                    "type": "object",
                    "required": ["path", "content"],
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to write"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to write to the file"
                        }
                    }
                }),
                output_schema: json!({
                    "type": "boolean",
                    "description": "True if write succeeded"
                }),
                effects: vec!["fs".to_string()],
            },
            FsOp::Exists => ToolSchema {
                name: op.tool_name().to_string(),
                description: op.description().to_string(),
                input_schema: json!({
                    "type": "object",
                    "required": ["path"],
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to check for existence"
                        }
                    }
                }),
                output_schema: json!({
                    "type": "boolean",
                    "description": "True if path exists"
                }),
                effects: vec!["fs".to_string()],
            },
            FsOp::List => ToolSchema {
                name: op.tool_name().to_string(),
                description: op.description().to_string(),
                input_schema: json!({
                    "type": "object",
                    "required": ["path"],
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Directory path to list"
                        }
                    }
                }),
                output_schema: json!({
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of file and directory names"
                }),
                effects: vec!["fs".to_string()],
            },
            FsOp::Mkdir => ToolSchema {
                name: op.tool_name().to_string(),
                description: op.description().to_string(),
                input_schema: json!({
                    "type": "object",
                    "required": ["path"],
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Directory path to create (recursive)"
                        }
                    }
                }),
                output_schema: json!({
                    "type": "boolean",
                    "description": "True if directory was created"
                }),
                effects: vec!["fs".to_string()],
            },
            FsOp::Remove => ToolSchema {
                name: op.tool_name().to_string(),
                description: op.description().to_string(),
                input_schema: json!({
                    "type": "object",
                    "required": ["path"],
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File or empty directory to remove"
                        }
                    }
                }),
                output_schema: json!({
                    "type": "boolean",
                    "description": "True if path was removed"
                }),
                effects: vec!["fs".to_string()],
            },
        };

        Self { op, schema }
    }

    /// Factory methods for each operation.
    pub fn read() -> Self {
        Self::new(FsOp::Read)
    }

    pub fn write() -> Self {
        Self::new(FsOp::Write)
    }

    pub fn exists() -> Self {
        Self::new(FsOp::Exists)
    }

    pub fn list() -> Self {
        Self::new(FsOp::List)
    }

    pub fn mkdir() -> Self {
        Self::new(FsOp::Mkdir)
    }

    pub fn remove() -> Self {
        Self::new(FsOp::Remove)
    }

    /// Execute the filesystem operation.
    fn execute(&self, input: Value) -> Result<Value, ToolError> {
        match self.op {
            FsOp::Read => {
                let req: ReadRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;

                let content = std::fs::read_to_string(&req.path)
                    .map_err(|e| ToolError::InvocationFailed(format!("read failed: {}", e)))?;

                Ok(json!(content))
            }
            FsOp::Write => {
                let req: WriteRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;

                std::fs::write(&req.path, &req.content)
                    .map_err(|e| ToolError::InvocationFailed(format!("write failed: {}", e)))?;

                Ok(json!(true))
            }
            FsOp::Exists => {
                let req: PathRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;

                let exists = Path::new(&req.path).exists();
                Ok(json!(exists))
            }
            FsOp::List => {
                let req: PathRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;

                let entries = std::fs::read_dir(&req.path)
                    .map_err(|e| ToolError::InvocationFailed(format!("list failed: {}", e)))?;

                let names: Vec<String> = entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
                    .collect();

                Ok(json!(names))
            }
            FsOp::Mkdir => {
                let req: PathRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;

                std::fs::create_dir_all(&req.path)
                    .map_err(|e| ToolError::InvocationFailed(format!("mkdir failed: {}", e)))?;

                Ok(json!(true))
            }
            FsOp::Remove => {
                let req: PathRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;

                let path = Path::new(&req.path);
                if path.is_dir() {
                    std::fs::remove_dir(&req.path).map_err(|e| {
                        ToolError::InvocationFailed(format!("remove dir failed: {}", e))
                    })?;
                } else {
                    std::fs::remove_file(&req.path).map_err(|e| {
                        ToolError::InvocationFailed(format!("remove file failed: {}", e))
                    })?;
                }

                Ok(json!(true))
            }
        }
    }
}

impl ToolProvider for FsProvider {
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn temp_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("lumen_fs_test_{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn test_write_and_read() {
        let tmp = temp_dir();
        fs::create_dir_all(&tmp).unwrap();
        let file_path = tmp.join("test.txt");

        // Write
        let write_provider = FsProvider::write();
        let write_input = json!({
            "path": file_path.to_str().unwrap(),
            "content": "hello world"
        });
        let write_result = write_provider.call(write_input).unwrap();
        assert_eq!(write_result, json!(true));

        // Read
        let read_provider = FsProvider::read();
        let read_input = json!({
            "path": file_path.to_str().unwrap()
        });
        let read_result = read_provider.call(read_input).unwrap();
        assert_eq!(read_result, json!("hello world"));

        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn test_exists() {
        let tmp = temp_dir();
        fs::create_dir_all(&tmp).unwrap();
        let existing_file = tmp.join("exists.txt");
        let mut file = fs::File::create(&existing_file).unwrap();
        file.write_all(b"test").unwrap();

        let provider = FsProvider::exists();

        // Existing file
        let result = provider
            .call(json!({
                "path": existing_file.to_str().unwrap()
            }))
            .unwrap();
        assert_eq!(result, json!(true));

        // Non-existing file
        let result = provider
            .call(json!({
                "path": tmp.join("nonexistent.txt").to_str().unwrap()
            }))
            .unwrap();
        assert_eq!(result, json!(false));

        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn test_list() {
        let tmp = temp_dir();
        fs::create_dir_all(&tmp).unwrap();
        fs::File::create(tmp.join("file1.txt")).unwrap();
        fs::File::create(tmp.join("file2.txt")).unwrap();
        fs::create_dir_all(tmp.join("subdir")).unwrap();

        let provider = FsProvider::list();
        let result = provider
            .call(json!({
                "path": tmp.to_str().unwrap()
            }))
            .unwrap();

        let entries: Vec<String> = serde_json::from_value(result).unwrap();
        assert_eq!(entries.len(), 3);
        assert!(entries.contains(&"file1.txt".to_string()));
        assert!(entries.contains(&"file2.txt".to_string()));
        assert!(entries.contains(&"subdir".to_string()));

        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn test_mkdir() {
        let tmp = temp_dir();
        let new_dir = tmp.join("nested/deep/dir");

        let provider = FsProvider::mkdir();
        let result = provider
            .call(json!({
                "path": new_dir.to_str().unwrap()
            }))
            .unwrap();
        assert_eq!(result, json!(true));
        assert!(new_dir.exists());
        assert!(new_dir.is_dir());

        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn test_remove_file() {
        let tmp = temp_dir();
        fs::create_dir_all(&tmp).unwrap();
        let file_path = tmp.join("to_remove.txt");
        fs::File::create(&file_path).unwrap();

        assert!(file_path.exists());

        let provider = FsProvider::remove();
        let result = provider
            .call(json!({
                "path": file_path.to_str().unwrap()
            }))
            .unwrap();
        assert_eq!(result, json!(true));
        assert!(!file_path.exists());

        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn test_remove_directory() {
        let tmp = temp_dir();
        let dir_path = tmp.join("empty_dir");
        fs::create_dir_all(&dir_path).unwrap();

        assert!(dir_path.exists());

        let provider = FsProvider::remove();
        let result = provider
            .call(json!({
                "path": dir_path.to_str().unwrap()
            }))
            .unwrap();
        assert_eq!(result, json!(true));
        assert!(!dir_path.exists());

        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn test_read_nonexistent_file() {
        let provider = FsProvider::read();
        let result = provider.call(json!({
            "path": "/tmp/nonexistent_lumen_test_file_12345.txt"
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_schema() {
        let provider = FsProvider::read();
        assert_eq!(provider.name(), "fs.read");
        assert_eq!(provider.version(), "0.1.0");
        assert_eq!(provider.schema().effects, vec!["fs"]);
    }
}
