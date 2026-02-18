//! Lumen native code generation via Cranelift.
//!
//! Lowers LIR bytecode modules to native machine code.

pub mod aot;
pub mod bench_programs;
pub mod collection_helpers;
pub mod context;
pub mod emit;
pub mod ffi;
pub mod ir;
pub mod jit;
pub mod opcode_def;
pub mod opt;
pub mod types;
pub mod union_helpers;
pub mod wasm;
pub mod wit;

// Re-export commonly used wasm types
pub use wasm::{compile_to_wasm, WasmCodegen, WasmTarget};

// Re-export opcode definition system
pub use opcode_def::{OpContext, OpcodeDef, OpcodeRegistry};
