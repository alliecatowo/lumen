//! Lumen VM — register-based virtual machine for executing LIR bytecode.
#![warn(clippy::all)]

pub mod arena;
pub mod gc;
pub mod immix;
pub mod jit_tier;
pub mod parity_concurrency;
pub mod strings;
pub mod tagged;
pub mod tlab;
pub mod types;
pub mod values;
pub mod vm;
