//! Lumen native code generation via Cranelift.
//!
//! Lowers LIR bytecode modules to native machine code.

pub mod aot;
pub mod bench_programs;
pub mod context;
pub mod emit;
pub mod ffi;
pub mod ir;
pub mod jit;
pub mod types;
pub mod wasm;
pub mod wit;
