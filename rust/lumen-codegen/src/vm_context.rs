//! VM context pointer passed to JIT-compiled code.
//!
//! This module re-exports the `VmContext` and associated types from `lumen-core`.
//! All JIT-generated code and runtime helpers use this context structure.
//!
//! See `lumen_core::vm_context` for full documentation.

// Re-export from lumen-core
pub use lumen_core::vm_context::{
    EffectScope, ProviderRegistry, Scheduler, TraceContext, VmContext,
};
