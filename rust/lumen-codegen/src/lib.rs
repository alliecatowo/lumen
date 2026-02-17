//! Lumen native code generation via Cranelift.
//!
//! Lowers LIR bytecode modules to native machine code.

pub mod context;
pub mod emit;
pub mod lower;
pub mod types;
