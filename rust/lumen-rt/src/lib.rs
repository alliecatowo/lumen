//! Lumen RT — unified runtime combining the register-based virtual machine and runtime services.
//!
//! Provides VM execution, memory management, trace collection, tool dispatch,
//! process management, scheduling, and inter-process communication infrastructure.
#![warn(clippy::all)]

pub mod arena;
pub mod gc;
pub mod immix;
pub mod jit_tier;
pub mod parity_concurrency;
pub mod tagged;
pub mod tlab;
pub mod vm;
pub mod services;

// Re-export core types from lumen-core for backward compatibility
pub use lumen_core::{strings, types, values};
