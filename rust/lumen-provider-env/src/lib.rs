//! Environment provider for Lumen tool dispatch.
//!
//! Implements the `ToolProvider` trait to expose environment operations as tools:
//! - `env.get` — get environment variable
//! - `env.set` — set environment variable
//! - `env.list` — list all environment variables
//! - `env.has` — check if environment variable exists
//! - `env.cwd` — get current working directory
//! - `env.home` — get home directory
//! - `env.platform` — get platform string
//! - `env.args` — get command line arguments
//!
//! All tools return JSON values compatible with Lumen's type system.

use lumen_runtime::tools::{ToolError, ToolProvider, ToolSchema};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;

// ---------------------------------------------------------------------------
// EnvTool enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnvTool {
    Get,
    Set,
    List,
    Has,
    Cwd,
    Home,
    Platform,
    Args,
}

impl EnvTool {
    fn tool_name(&self) -> &'static str {
        match self {
            EnvTool::Get => "env.get",
            EnvTool::Set => "env.set",
            EnvTool::List => "env.list",
            EnvTool::Has => "env.has",
            EnvTool::Cwd => "env.cwd",
            EnvTool::Home => "env.home",
            EnvTool::Platform => "env.platform",
            EnvTool::Args => "env.args",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            EnvTool::Get => {
                "Get the value of an environment variable (returns empty string if not set)"
            }
            EnvTool::Set => "Set an environment variable for the current process",
            EnvTool::List => "List all environment variables as key-value pairs",
            EnvTool::Has => "Check if an environment variable exists",
            EnvTool::Cwd => "Get the current working directory",
            EnvTool::Home => "Get the user's home directory",
            EnvTool::Platform => "Get the platform string (linux, macos, windows)",
            EnvTool::Args => "Get command line arguments",
        }
    }
}

// ---------------------------------------------------------------------------
// EnvProvider implementation
// ---------------------------------------------------------------------------

/// Environment provider implementing the `ToolProvider` trait.
pub struct EnvProvider {
    tool: EnvTool,
    schema: ToolSchema,
}

impl EnvProvider {
    /// Create a new environment provider for the given tool.
    fn new(tool: EnvTool) -> Self {
        let (input_schema, output_schema) = match tool {
            EnvTool::Get => (
                json!({
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Name of the environment variable"
                        }
                    }
                }),
                json!({
                    "type": "string",
                    "description": "Value of the environment variable (empty if not set)"
                }),
            ),
            EnvTool::Set => (
                json!({
                    "type": "object",
                    "required": ["name", "value"],
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Name of the environment variable"
                        },
                        "value": {
                            "type": "string",
                            "description": "Value to set"
                        }
                    }
                }),
                json!({
                    "type": "boolean",
                    "description": "Always true (operation succeeded)"
                }),
            ),
            EnvTool::List => (
                json!({
                    "type": "object",
                    "properties": {}
                }),
                json!({
                    "type": "object",
                    "description": "Map of all environment variables",
                    "additionalProperties": {"type": "string"}
                }),
            ),
            EnvTool::Has => (
                json!({
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Name of the environment variable"
                        }
                    }
                }),
                json!({
                    "type": "boolean",
                    "description": "True if the environment variable exists"
                }),
            ),
            EnvTool::Cwd => (
                json!({
                    "type": "object",
                    "properties": {}
                }),
                json!({
                    "type": "string",
                    "description": "Current working directory path"
                }),
            ),
            EnvTool::Home => (
                json!({
                    "type": "object",
                    "properties": {}
                }),
                json!({
                    "type": "string",
                    "description": "Home directory path"
                }),
            ),
            EnvTool::Platform => (
                json!({
                    "type": "object",
                    "properties": {}
                }),
                json!({
                    "type": "string",
                    "description": "Platform identifier (linux, macos, windows)"
                }),
            ),
            EnvTool::Args => (
                json!({
                    "type": "object",
                    "properties": {}
                }),
                json!({
                    "type": "array",
                    "description": "Command line arguments",
                    "items": {"type": "string"}
                }),
            ),
        };

        let schema = ToolSchema {
            name: tool.tool_name().to_string(),
            description: tool.description().to_string(),
            input_schema,
            output_schema,
            effects: vec!["env".to_string()],
        };

        Self { tool, schema }
    }

    /// Create a GET provider.
    pub fn get() -> Self {
        Self::new(EnvTool::Get)
    }

    /// Create a SET provider.
    pub fn set() -> Self {
        Self::new(EnvTool::Set)
    }

    /// Create a LIST provider.
    pub fn list() -> Self {
        Self::new(EnvTool::List)
    }

    /// Create a HAS provider.
    pub fn has() -> Self {
        Self::new(EnvTool::Has)
    }

    /// Create a CWD provider.
    pub fn cwd() -> Self {
        Self::new(EnvTool::Cwd)
    }

    /// Create a HOME provider.
    pub fn home() -> Self {
        Self::new(EnvTool::Home)
    }

    /// Create a PLATFORM provider.
    pub fn platform() -> Self {
        Self::new(EnvTool::Platform)
    }

    /// Create an ARGS provider.
    pub fn args() -> Self {
        Self::new(EnvTool::Args)
    }

    /// Execute the tool operation.
    fn execute(&self, input: Value) -> Result<Value, ToolError> {
        match self.tool {
            EnvTool::Get => {
                #[derive(Deserialize)]
                struct GetInput {
                    name: String,
                }
                let input: GetInput = serde_json::from_value(input).map_err(|e| {
                    ToolError::InvocationFailed(format!("Invalid input format: {}", e))
                })?;
                let value = env::var(&input.name).unwrap_or_default();
                Ok(json!(value))
            }
            EnvTool::Set => {
                #[derive(Deserialize)]
                struct SetInput {
                    name: String,
                    value: String,
                }
                let input: SetInput = serde_json::from_value(input).map_err(|e| {
                    ToolError::InvocationFailed(format!("Invalid input format: {}", e))
                })?;
                env::set_var(&input.name, &input.value);
                Ok(json!(true))
            }
            EnvTool::List => {
                let vars: HashMap<String, String> = env::vars().collect();
                Ok(serde_json::to_value(vars).unwrap())
            }
            EnvTool::Has => {
                #[derive(Deserialize)]
                struct HasInput {
                    name: String,
                }
                let input: HasInput = serde_json::from_value(input).map_err(|e| {
                    ToolError::InvocationFailed(format!("Invalid input format: {}", e))
                })?;
                Ok(json!(env::var(&input.name).is_ok()))
            }
            EnvTool::Cwd => {
                let cwd = env::current_dir().map_err(|e| {
                    ToolError::InvocationFailed(format!("Failed to get current directory: {}", e))
                })?;
                Ok(json!(cwd.to_string_lossy().to_string()))
            }
            EnvTool::Home => {
                let home = env::var("HOME")
                    .or_else(|_| env::var("USERPROFILE"))
                    .map_err(|_| {
                        ToolError::InvocationFailed("Failed to determine home directory".into())
                    })?;
                Ok(json!(home))
            }
            EnvTool::Platform => {
                let platform = if cfg!(target_os = "linux") {
                    "linux"
                } else if cfg!(target_os = "macos") {
                    "macos"
                } else if cfg!(target_os = "windows") {
                    "windows"
                } else {
                    "unknown"
                };
                Ok(json!(platform))
            }
            EnvTool::Args => {
                let args: Vec<String> = env::args().collect();
                Ok(serde_json::to_value(args).unwrap())
            }
        }
    }
}

impl ToolProvider for EnvProvider {
    fn name(&self) -> &str {
        &self.schema.name
    }

    fn version(&self) -> &str {
        "1.0.0"
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
    use serde_json::json;

    #[test]
    fn provider_metadata() {
        let provider = EnvProvider::get();
        assert_eq!(provider.name(), "env.get");
        assert_eq!(provider.version(), "1.0.0");
        assert_eq!(provider.schema().name, "env.get");
        assert_eq!(provider.schema().effects, vec!["env"]);
    }

    #[test]
    fn all_tools_have_correct_metadata() {
        let providers = vec![
            (EnvProvider::get(), "env.get"),
            (EnvProvider::set(), "env.set"),
            (EnvProvider::list(), "env.list"),
            (EnvProvider::has(), "env.has"),
            (EnvProvider::cwd(), "env.cwd"),
            (EnvProvider::home(), "env.home"),
            (EnvProvider::platform(), "env.platform"),
            (EnvProvider::args(), "env.args"),
        ];

        for (provider, expected_name) in providers {
            assert_eq!(provider.name(), expected_name);
            assert_eq!(provider.version(), "1.0.0");
            assert_eq!(provider.schema().effects, vec!["env"]);
        }
    }

    #[test]
    fn env_get_existing_var() {
        env::set_var("TEST_VAR_GET", "test_value");
        let provider = EnvProvider::get();
        let input = json!({"name": "TEST_VAR_GET"});
        let result = provider.call(input).unwrap();
        assert_eq!(result, json!("test_value"));
        env::remove_var("TEST_VAR_GET");
    }

    #[test]
    fn env_get_missing_var() {
        env::remove_var("NONEXISTENT_VAR");
        let provider = EnvProvider::get();
        let input = json!({"name": "NONEXISTENT_VAR"});
        let result = provider.call(input).unwrap();
        assert_eq!(result, json!(""));
    }

    #[test]
    fn env_set() {
        env::remove_var("TEST_VAR_SET");
        let provider = EnvProvider::set();
        let input = json!({"name": "TEST_VAR_SET", "value": "new_value"});
        let result = provider.call(input).unwrap();
        assert_eq!(result, json!(true));
        assert_eq!(env::var("TEST_VAR_SET").unwrap(), "new_value");
        env::remove_var("TEST_VAR_SET");
    }

    #[test]
    fn env_has_existing() {
        env::set_var("TEST_VAR_HAS", "value");
        let provider = EnvProvider::has();
        let input = json!({"name": "TEST_VAR_HAS"});
        let result = provider.call(input).unwrap();
        assert_eq!(result, json!(true));
        env::remove_var("TEST_VAR_HAS");
    }

    #[test]
    fn env_has_missing() {
        env::remove_var("NONEXISTENT_VAR_HAS");
        let provider = EnvProvider::has();
        let input = json!({"name": "NONEXISTENT_VAR_HAS"});
        let result = provider.call(input).unwrap();
        assert_eq!(result, json!(false));
    }

    #[test]
    fn env_list() {
        let provider = EnvProvider::list();
        let input = json!({});
        let result = provider.call(input).unwrap();
        assert!(result.is_object());
        let vars = result.as_object().unwrap();
        assert!(!vars.is_empty());
    }

    #[test]
    fn env_cwd() {
        let provider = EnvProvider::cwd();
        let input = json!({});
        let result = provider.call(input).unwrap();
        assert!(result.is_string());
        let cwd = result.as_str().unwrap();
        assert!(!cwd.is_empty());
    }

    #[test]
    fn env_home() {
        let provider = EnvProvider::home();
        let input = json!({});
        let result = provider.call(input).unwrap();
        assert!(result.is_string());
        let home = result.as_str().unwrap();
        assert!(!home.is_empty());
    }

    #[test]
    fn env_platform() {
        let provider = EnvProvider::platform();
        let input = json!({});
        let result = provider.call(input).unwrap();
        assert!(result.is_string());
        let platform = result.as_str().unwrap();
        assert!(["linux", "macos", "windows", "unknown"].contains(&platform));
    }

    #[test]
    fn env_args() {
        let provider = EnvProvider::args();
        let input = json!({});
        let result = provider.call(input).unwrap();
        assert!(result.is_array());
        let args = result.as_array().unwrap();
        assert!(!args.is_empty());
    }
}
