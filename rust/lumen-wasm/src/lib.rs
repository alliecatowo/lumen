//! WebAssembly bindings for the Lumen compiler and VM.
//!
//! This crate provides a WASM-compatible interface for compiling and executing
//! Lumen programs in browser and WASI environments.

pub mod wasi;

use lumen_vm::vm::VM;
use wasm_bindgen::prelude::*;

/// Result of a compilation or execution operation.
///
/// JSON format:
/// - Success: `{"ok": "result_value"}`
/// - Error: `{"error": "error_message"}`
#[wasm_bindgen]
pub struct LumenResult {
    json: String,
}

#[wasm_bindgen]
impl LumenResult {
    /// Returns true if the operation succeeded.
    pub fn is_ok(&self) -> bool {
        self.json.contains("\"ok\"")
    }

    /// Returns true if the operation failed.
    pub fn is_err(&self) -> bool {
        !self.is_ok()
    }

    /// Returns the result as a JSON string.
    pub fn to_json(&self) -> String {
        self.json.clone()
    }
}

impl LumenResult {
    fn ok(value: String) -> Self {
        let json = serde_json::json!({ "ok": value }).to_string();
        Self { json }
    }

    fn err(error: String) -> Self {
        let json = serde_json::json!({ "error": error }).to_string();
        Self { json }
    }
}

/// Type-check a Lumen source file.
///
/// Returns a LumenResult:
/// - On success: `{"ok": "Type-checked successfully"}`
/// - On error: `{"error": "error message with diagnostics"}`
#[wasm_bindgen]
pub fn check(source: &str) -> LumenResult {
    match lumen_compiler::compile(source) {
        Ok(_) => LumenResult::ok("Type-checked successfully".to_string()),
        Err(err) => {
            let formatted = lumen_compiler::format_error(&err, source, "input.lm");
            LumenResult::err(formatted)
        }
    }
}

/// Compile Lumen source to LIR JSON.
///
/// Returns a LumenResult:
/// - On success: `{"ok": "<LIR JSON>"}`
/// - On error: `{"error": "error message with diagnostics"}`
#[wasm_bindgen]
pub fn compile(source: &str) -> LumenResult {
    match lumen_compiler::compile(source) {
        Ok(module) => match serde_json::to_string_pretty(&module) {
            Ok(json) => LumenResult::ok(json),
            Err(e) => LumenResult::err(format!("Failed to serialize LIR: {}", e)),
        },
        Err(err) => {
            let formatted = lumen_compiler::format_error(&err, source, "input.lm");
            LumenResult::err(formatted)
        }
    }
}

/// Compile and execute Lumen source.
///
/// Returns a LumenResult:
/// - On success: `{"ok": "<output>"}`
/// - On error: `{"error": "error message"}`
///
/// The `cell_name` parameter specifies which cell to execute (default: "main").
#[wasm_bindgen]
pub fn run(source: &str, cell_name: Option<String>) -> LumenResult {
    let cell = cell_name.as_deref().unwrap_or("main");

    // Compile the source
    let module = match lumen_compiler::compile(source) {
        Ok(m) => m,
        Err(err) => {
            let formatted = lumen_compiler::format_error(&err, source, "input.lm");
            return LumenResult::err(formatted);
        }
    };

    // Create VM instance and load module
    let mut vm = VM::new();
    vm.load(module);

    // Execute the specified cell
    match vm.execute(cell, vec![]) {
        Ok(result) => {
            // Format the result value as a string
            let output = format!("{}", result);
            LumenResult::ok(output)
        }
        Err(e) => LumenResult::err(format!("Runtime error: {:?}", e)),
    }
}

/// Get the version of the Lumen compiler.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_valid() {
        let source = "```lumen\ncell main() -> Int\n    42\nend\n```";
        let result = check(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_invalid() {
        // Undefined variable
        let source = "```lumen\ncell main() -> Int\n    undefined_var\nend\n```";
        let result = check(source);
        assert!(result.is_err());
    }

    #[test]
    fn test_compile_valid() {
        let source = "```lumen\ncell main() -> Int\n    42\nend\n```";
        let result = compile(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_simple() {
        let source = "```lumen\ncell main() -> Int\n    42\nend\n```";
        let result = run(source, None);
        if result.is_err() {
            eprintln!("Run error: {}", result.to_json());
        }
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_arithmetic() {
        let source = "```lumen\ncell main() -> Int\n    2 + 3\nend\n```";
        let result = run(source, Some("main".to_string()));
        if result.is_err() {
            eprintln!("Run error: {}", result.to_json());
        }
        assert!(result.is_ok());
    }

    #[test]
    fn test_version() {
        let v = version();
        assert!(!v.is_empty());
        assert!(v.contains('.'));
    }
}
