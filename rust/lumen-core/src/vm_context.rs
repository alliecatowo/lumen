//! VM context pointer passed to JIT-compiled code and runtime helpers.
//!
//! This module defines the `VmContext` structure that serves as a bridge between
//! JIT-compiled functions and the Lumen runtime. All JIT functions and runtime
//! helpers follow the calling convention:
//!
//! ```ignore
//! fn(vm_ctx: *mut VmContext, arg0: i64, arg1: i64, ...) -> i64
//! ```
//!
//! The VM context pointer is passed as a hidden first parameter to ALL functions
//! called from JIT code. This follows the V8 architecture approach and provides:
//!
//! - **Thread safety**: No global state; all context is local to the function call
//! - **Re-entrancy**: JIT-compiled code can call helpers which can call other JIT code
//! - **Extensibility**: Easy to add new VM services without changing signatures
//! - **ABI consistency**: All calling conventions unified to `(ctx: *mut VmContext, ...)`
//!
//! # Safety
//!
//! The VM context pointer must:
//! 1. Be valid and properly aligned for `VmContext`
//! 2. Point to an initialized `VmContext` structure
//! 3. Remain valid for the duration of the function call
//! 4. Not be modified concurrently from multiple threads

use crate::strings::StringTable;

/// Opaque effect handler stack type.
/// Holds the stack of active effect handlers for the current execution context.
#[repr(C)]
pub struct EffectScope {
    _private: [u8; 0],
}

/// Opaque scheduler type.
/// Provides work-stealing task scheduling for concurrent operations.
#[repr(C)]
pub struct Scheduler {
    _private: [u8; 0],
}

/// Opaque provider registry type.
/// Maps tool aliases to their runtime implementations.
#[repr(C)]
pub struct ProviderRegistry {
    _private: [u8; 0],
}

/// Opaque trace context type.
/// Collects structured trace events during execution.
#[repr(C)]
pub struct TraceContext {
    _private: [u8; 0],
}

/// VM context passed to all JIT-compiled functions and runtime helpers.
///
/// This is the single parameter that gives JIT code access to all VM services.
/// All functions called from JIT code must accept this as the first parameter.
///
/// # Calling Convention
///
/// ```ignore
/// // Rust FFI function
/// #[no_mangle]
/// pub extern "C" fn lumen_rt_my_helper(
///     vm_ctx: *mut VmContext,
///     arg0: i64,
///     arg1: i64,
/// ) -> i64 {
///     unsafe {
///         let ctx = &*vm_ctx;
///         // Access VM services via ctx.effect_stack, ctx.scheduler, etc.
///     }
/// }
///
/// // JIT-generated code (Cranelift)
/// // Implicit convention: first param is always VM context
/// call_ext vm_ctx, arg0, arg1 -> lumen_rt_my_helper
/// ```
#[repr(C)]
pub struct VmContext {
    /// Effect handler stack for algebraic effects handling.
    /// Enables `perform` / `handle` / `resume` semantics.
    pub effect_stack: *mut EffectScope,

    /// Scheduler for concurrent task management.
    /// Used by `parallel`, `race`, `vote`, `select`, and futures.
    pub scheduler: *mut Scheduler,

    /// Tool provider registry.
    /// Maps effect operations to tool implementations at runtime.
    pub registry: *mut ProviderRegistry,

    /// Trace context for structured event collection.
    /// Records tool calls, performance metrics, and debug information.
    pub trace_ctx: *mut TraceContext,

    /// String interning table shared with the interpreter.
    /// JIT runtime helpers use this to intern union variant tags, ensuring
    /// the same u32 IDs as the interpreter's `StringTable`.
    pub string_table: *mut StringTable,

    /// Current fiber (opaque pointer).
    /// Points to the active fiber in the fiber-based execution model.
    /// Used by the scheduler and continuation system for cooperative multitasking.
    pub current_fiber: *mut (),

    /// Fiber pool for recycling (opaque pointer).
    /// Points to the pool of reusable fiber stacks, reducing allocation overhead
    /// when spawning and completing fibers.
    pub stack_pool: *mut (),
}

impl VmContext {
    /// Creates a new empty VM context (typically initialized by the runtime).
    pub fn new() -> Self {
        Self {
            effect_stack: std::ptr::null_mut(),
            scheduler: std::ptr::null_mut(),
            registry: std::ptr::null_mut(),
            trace_ctx: std::ptr::null_mut(),
            string_table: std::ptr::null_mut(),
            current_fiber: std::ptr::null_mut(),
            stack_pool: std::ptr::null_mut(),
        }
    }

    /// Record a runtime error from JIT-compiled code.
    ///
    /// Currently a no-op stub — the VM checks for errors after JIT dispatch
    /// via other mechanisms. This method exists so that JIT runtime helpers
    /// (e.g. `jit_rt_trap_divzero`) can compile without feature-gating.
    pub fn set_error(&mut self, _msg: String) {
        // TODO: store the error message so the VM can convert it into a
        // proper runtime error after JIT returns.
    }
}

impl Default for VmContext {
    fn default() -> Self {
        Self::new()
    }
}

// Safety: VmContext is just a collection of raw pointers, which is Send/Sync
// The actual safety depends on the runtime ensuring proper pointer validity.
unsafe impl Send for VmContext {}
unsafe impl Sync for VmContext {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_context_default() {
        let ctx = VmContext::default();
        assert!(ctx.effect_stack.is_null());
        assert!(ctx.scheduler.is_null());
        assert!(ctx.registry.is_null());
        assert!(ctx.trace_ctx.is_null());
        assert!(ctx.string_table.is_null());
        assert!(ctx.current_fiber.is_null());
        assert!(ctx.stack_pool.is_null());
    }

    #[test]
    fn test_vm_context_size() {
        // Ensure VmContext has the expected memory layout
        use std::mem;
        assert_eq!(
            mem::size_of::<VmContext>(),
            mem::size_of::<*mut u8>() * 7,
            "VmContext should be exactly 7 pointers"
        );
    }
}
