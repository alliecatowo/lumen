//! WebAssembly code generation backend.

pub mod backend;
pub mod control;

// Re-export main API from backend
pub use backend::{compile_to_wasm, WasmCodegen, WasmTarget};
