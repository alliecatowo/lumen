//! Fiber-based algebraic effect runtime helpers (C-ABI).
//!
//! These are the low-level runtime functions called by both the interpreter
//! and JIT-compiled code to implement `handle`, `perform`, and `resume`.
//!
//! # Architecture
//!
//! Each `handle` block installs a **handler fiber** onto the `FiberEffectStack`
//! stored in the `VmContext`. When `perform` is called, the runtime:
//! 1. Walks the stack to find a matching handler by `(effect_id, op_id)`.
//! 2. Records the performer's fiber as a `SuspendedPerformer`.
//! 3. Switches to the handler fiber, passing the argument value.
//!
//! The handler fiber may later call `resume`, which switches back to the
//! performer fiber with a result value.
//!
//! # Calling Convention
//!
//! All functions are `extern "C"` so they can be called from JIT code via
//! indirect calls. The first argument is always `*mut VmContext` (following
//! the established Lumen JIT ABI), though the current implementations below
//! operate on a standalone `FiberEffectStack` pointer for clarity during the
//! transition period while the full VM context wiring is in progress.
//!
//! # Safety
//!
//! All public functions are `unsafe` because they dereference raw pointers.
//! Callers must ensure:
//! - All pointers are valid, non-null, and properly aligned.
//! - The `FiberEffectStack` outlives all fibers registered with it.
//! - `fiber_switch` is only called with fibers in valid states.

use super::fiber::{Fiber, FiberPool, FiberStatus, DEFAULT_FIBER_STACK_SIZE};
use crate::platform;
use lumen_core::nb_value::NbValue;
use lumen_core::vm_context::VmContext;

// ── Effect handler entry record ───────────────────────────────────────────────

/// An installed effect handler on the effect stack.
///
/// Created by `lm_rt_handle_push` and consumed by `lm_rt_handle_pop`.
pub struct HandlerEntry {
    /// Unique ID of the effect being handled (matches LIR effect index).
    pub effect_id: u32,
    /// Unique ID of the operation within that effect.
    pub op_id: u32,
    /// The fiber running the handler body.
    /// When `perform` dispatches to this handler, it fiber-switches here.
    pub handler_fiber: *mut Fiber,
    /// The fiber that was running before this handler was installed.
    /// `lm_rt_handle_pop` switches back to this fiber.
    pub parent_fiber: *mut Fiber,
}

// ── Suspended performer record ────────────────────────────────────────────────

/// Records a fiber that has called `perform` and is waiting to be resumed.
///
/// Stored in the `FiberEffectStack` while the handler is executing.
/// Released (and the fiber switched back to) by `lm_rt_resume`.
pub struct SuspendedPerformer {
    /// The fiber that called `perform` (suspended, waiting for resume).
    pub fiber: *mut Fiber,
    /// The register slot in the performer fiber where the resume value
    /// should be stored. Not used by the fiber helpers directly — the
    /// caller (VM or JIT) is responsible for placing the value into the
    /// correct register after `lm_rt_resume` returns.
    pub result_slot: u32,
}

// ── FiberEffectStack ──────────────────────────────────────────────────────────

/// Stack of active effect handlers for a single execution context.
///
/// This is the fiber-native replacement for the interpreter's `Vec<EffectScope>`.
/// It is heap-allocated and its pointer is stored in `VmContext::effect_stack`
/// (cast through the opaque `*mut EffectScope` type).
///
/// # Layout
///
/// ```text
///   handler_stack:  [ entry_0, entry_1, ... entry_N ]   ← top = entry_N
///   suspended:      None | Some(SuspendedPerformer)
///   pool:           FiberPool  (shared stack allocator)
/// ```
pub struct FiberEffectStack {
    /// Active handler entries, innermost (most recently pushed) last.
    handler_stack: Vec<HandlerEntry>,
    /// The currently-suspended performer (if a `perform` is in flight).
    /// Only one performer can be in flight per stack (one-shot semantics).
    suspended: Option<SuspendedPerformer>,
    /// Stack allocator for handler fibers.
    pool: FiberPool,
    /// Handler fibers pending free after we switch off their stacks.
    pending_handler_frees: Vec<*mut Fiber>,
}

impl FiberEffectStack {
    /// Create a new empty effect stack.
    pub fn new() -> Self {
        FiberEffectStack {
            handler_stack: Vec::new(),
            suspended: None,
            pool: FiberPool::new(DEFAULT_FIBER_STACK_SIZE, 0),
            pending_handler_frees: Vec::new(),
        }
    }

    /// Create a new effect stack with pre-allocated stacks.
    pub fn with_pool(pre_allocate: usize) -> Self {
        FiberEffectStack {
            handler_stack: Vec::new(),
            suspended: None,
            pool: FiberPool::new(DEFAULT_FIBER_STACK_SIZE, pre_allocate),
            pending_handler_frees: Vec::new(),
        }
    }

    /// Find the innermost handler for `(effect_id, op_id)`, searching from top.
    fn find_handler(&self, effect_id: u32, op_id: u32) -> Option<usize> {
        self.handler_stack
            .iter()
            .rposition(|e| e.effect_id == effect_id && e.op_id == op_id)
    }
}

impl Default for FiberEffectStack {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for FiberEffectStack {
    fn drop(&mut self) {
        // Free all handler fibers still on the stack (e.g., if we're unwinding).
        for entry in self.handler_stack.drain(..) {
            if !entry.handler_fiber.is_null() {
                // Safety: handler_fiber was allocated by Box::into_raw.
                unsafe {
                    let _ = Box::from_raw(entry.handler_fiber);
                }
            }
        }
        for handler in self.pending_handler_frees.drain(..) {
            if !handler.is_null() {
                unsafe {
                    let _ = Box::from_raw(handler);
                }
            }
        }
        // The FiberPool drops itself, munmapping all cached stacks.
    }
}

// ── Helper: cast VmContext::effect_stack to FiberEffectStack ─────────────────

/// Cast the opaque `VmContext::effect_stack` pointer to `&mut FiberEffectStack`.
///
/// # Safety
/// The pointer must have been set by `lm_rt_effect_stack_init`.
#[inline(always)]
unsafe fn effect_stack_of(ctx: *mut VmContext) -> &'static mut FiberEffectStack {
    debug_assert!(!ctx.is_null(), "lm_rt: null VmContext");
    let ctx_ref = &mut *ctx;
    debug_assert!(
        !ctx_ref.effect_stack.is_null(),
        "lm_rt: effect_stack not initialized"
    );
    &mut *(ctx_ref.effect_stack as *mut FiberEffectStack)
}

/// Cast the opaque `VmContext::current_fiber` to `*mut Fiber`.
#[inline(always)]
unsafe fn current_fiber_of(ctx: *mut VmContext) -> *mut Fiber {
    (*ctx).current_fiber as *mut Fiber
}

/// Write `fiber` into `VmContext::current_fiber`.
#[inline(always)]
unsafe fn set_current_fiber(ctx: *mut VmContext, fiber: *mut Fiber) {
    (*ctx).current_fiber = fiber as *mut ();
    platform::set_current_fiber(fiber);
}

#[inline(always)]
unsafe fn queue_handler_free(stack: &mut FiberEffectStack, handler: *mut Fiber) {
    if handler.is_null() {
        return;
    }
    if stack
        .pending_handler_frees
        .iter()
        .any(|&queued| queued == handler)
    {
        return;
    }
    stack.pending_handler_frees.push(handler);
}

#[inline(always)]
unsafe fn drain_pending_handler_frees(ctx: *mut VmContext, stack: &mut FiberEffectStack) {
    if stack.pending_handler_frees.is_empty() {
        return;
    }

    let current = current_fiber_of(ctx);
    let mut idx = 0;
    while idx < stack.pending_handler_frees.len() {
        let handler = stack.pending_handler_frees[idx];
        if handler.is_null() || handler == current {
            idx += 1;
            continue;
        }

        stack.pending_handler_frees.swap_remove(idx);
        let _ = Box::from_raw(handler);
    }
}

// ── Public C-ABI runtime helpers ─────────────────────────────────────────────

/// Initialize the `FiberEffectStack` in a `VmContext`.
///
/// Must be called once before any other `lm_rt_*` function.
/// The stack is heap-allocated and its ownership is transferred to the context.
/// Call `lm_rt_effect_stack_free` when the context is torn down.
///
/// # Safety
/// `ctx` must be a valid, non-null pointer to an initialized `VmContext`.
#[no_mangle]
pub unsafe extern "C" fn lm_rt_effect_stack_init(ctx: *mut VmContext) {
    let stack = Box::new(FiberEffectStack::new());
    (*ctx).effect_stack = Box::into_raw(stack) as *mut lumen_core::vm_context::EffectScope;
    platform::set_current_fiber(std::ptr::null_mut());
    #[cfg(unix)]
    {
        let _ = platform::ensure_thread_stack_growth_handler();
    }
}

/// Free the `FiberEffectStack` owned by `ctx`.
///
/// After this call, `ctx.effect_stack` is null.
///
/// # Safety
/// `ctx` must be a valid, non-null pointer. The effect stack must have been
/// initialized with `lm_rt_effect_stack_init`.
#[no_mangle]
pub unsafe extern "C" fn lm_rt_effect_stack_free(ctx: *mut VmContext) {
    if (*ctx).effect_stack.is_null() {
        return;
    }
    let _ = Box::from_raw((*ctx).effect_stack as *mut FiberEffectStack);
    (*ctx).effect_stack = std::ptr::null_mut();
}

// ── lm_rt_handle_push ────────────────────────────────────────────────────────

/// Install a new effect handler on the effect stack.
///
/// Allocates a handler fiber from the pool. The fiber is not yet switched to —
/// that happens when the first matching `perform` is dispatched.
///
/// # Parameters
/// - `ctx` — VM context. `ctx.effect_stack` must be initialized.
/// - `effect_id` — LIR effect index of the effect being handled.
/// - `op_id` — LIR operation index within the effect.
/// - `handler_entry` — `extern "C" fn(u64) -> !` that is the handler body.
///                     Receives the performed argument as its parameter.
///                     Must not return (it resumes or terminates the fiber).
///
/// # Safety
/// All pointers must be valid. `handler_entry` must be a valid function pointer
/// that does not return normally.
#[no_mangle]
pub unsafe extern "C" fn lm_rt_handle_push(
    ctx: *mut VmContext,
    effect_id: u32,
    op_id: u32,
    handler_entry: extern "C" fn(u64),
) {
    let stack = effect_stack_of(ctx);
    drain_pending_handler_frees(ctx, stack);
    let parent_fiber = current_fiber_of(ctx);

    // Allocate a new handler fiber and initialize it with the entry function.
    // The arg (u64) will be the NbValue bits of the performed argument.
    let mut fiber = Fiber::new(DEFAULT_FIBER_STACK_SIZE);
    fiber.init_with_fn(handler_entry, 0); // arg is set at perform-time via resume_val
    let fiber_ptr = Box::into_raw(fiber);

    stack.handler_stack.push(HandlerEntry {
        effect_id,
        op_id,
        handler_fiber: fiber_ptr,
        parent_fiber,
    });
}

// ── lm_rt_handle_pop ─────────────────────────────────────────────────────────

/// Remove the innermost handler from the effect stack.
///
/// This is called after a `handle` block exits normally (no perform was
/// dispatched to this handler, or the handler ran to completion).
///
/// If called from the handler fiber itself, the free is deferred until we are
/// running on a different fiber, because freeing the currently-executing stack
/// is unsafe.
///
/// # Safety
/// `ctx` must be valid and `ctx.effect_stack` must be initialized.
#[no_mangle]
pub unsafe extern "C" fn lm_rt_handle_pop(ctx: *mut VmContext) {
    let stack = effect_stack_of(ctx);
    drain_pending_handler_frees(ctx, stack);

    let entry = match stack.handler_stack.pop() {
        Some(e) => e,
        None => return, // no handler to pop (should not happen in well-formed code)
    };

    if !entry.handler_fiber.is_null() {
        (*entry.handler_fiber).status = FiberStatus::Dead;
    }
    let cur = current_fiber_of(ctx);
    queue_handler_free(stack, entry.handler_fiber);
    if cur == entry.handler_fiber {
        // We are currently executing ON the handler fiber, so we must defer free
        // and switch off this stack first. Any deferred free is drained next time
        // runtime helpers run on a different current fiber.
        if !entry.parent_fiber.is_null() {
            set_current_fiber(ctx, entry.parent_fiber);
            super::fiber::fiber_switch(entry.handler_fiber, entry.parent_fiber, 0);
            // If this frame is ever resumed, re-drain now that we're off the popped stack.
            let stack = effect_stack_of(ctx);
            drain_pending_handler_frees(ctx, stack);
        }
        return;
    }

    // Safe to free immediately when we're not running on that stack.
    drain_pending_handler_frees(ctx, stack);
}

// ── lm_rt_perform ────────────────────────────────────────────────────────────

/// Dispatch an effect operation to the nearest matching handler.
///
/// Searches the effect stack from innermost outward for a handler matching
/// `(effect_id, op_id)`. If found:
/// 1. Records the current fiber as `SuspendedPerformer`.
/// 2. Switches to the handler fiber, passing `arg` as the resume value.
/// 3. Returns whatever value the handler eventually passes to `lm_rt_resume`.
///
/// # Returns
/// The `NbValue` passed by the handler to `lm_rt_resume`, encoded as `u64`.
///
/// # Errors (returned as NbValue)
/// Returns `NbValue::new_null()` (0-payload TAG_NULL) if no handler is found.
/// Well-formed Lumen programs always have a handler; missing handlers are a
/// compiler invariant violation.
///
/// # Safety
/// All pointers must be valid. `ctx.current_fiber` must point to the fiber
/// currently executing this code.
#[no_mangle]
pub unsafe extern "C" fn lm_rt_perform(
    ctx: *mut VmContext,
    effect_id: u32,
    op_id: u32,
    arg: u64, // NbValue bits
) -> u64 {
    let stack = effect_stack_of(ctx);
    drain_pending_handler_frees(ctx, stack);

    let handler_idx = match stack.find_handler(effect_id, op_id) {
        Some(idx) => idx,
        None => {
            // No handler found — return null (caller should check).
            return NbValue::new_null().0;
        }
    };

    let performer_fiber = current_fiber_of(ctx);
    debug_assert!(
        !performer_fiber.is_null(),
        "lm_rt_perform: no current fiber"
    );

    // Pinned fibers cannot be suspended (e.g., FFI/native code on stack).
    if (*performer_fiber).pinned {
        return NbValue::new_null().0;
    }

    // Mark the performer as suspended.
    (*performer_fiber).status = FiberStatus::Suspended;

    // Record the suspended performer so the handler can resume it.
    // One-shot: only one performer may be suspended at a time per stack.
    stack.suspended = Some(SuspendedPerformer {
        fiber: performer_fiber,
        result_slot: 0, // The interpreter/JIT sets this before calling perform.
    });

    // Switch to the handler fiber.
    let handler_fiber = stack.handler_stack[handler_idx].handler_fiber;
    debug_assert!(
        !handler_fiber.is_null(),
        "lm_rt_perform: null handler fiber"
    );
    (*handler_fiber).status = FiberStatus::Running;
    set_current_fiber(ctx, handler_fiber);

    // fiber_switch saves performer registers and restores handler registers.
    // The handler will return here (via lm_rt_resume → fiber_switch) with
    // the resume value in `resume_val` (rax after the switch).
    #[cfg(target_arch = "x86_64")]
    let resume_val = super::fiber::fiber_switch(performer_fiber, handler_fiber, arg);

    #[cfg(not(target_arch = "x86_64"))]
    let resume_val = {
        // Fallback for non-x86_64: no native fiber switch available.
        // Return the arg unchanged (behavior: no suspension, inline handler).
        let _ = (performer_fiber, handler_fiber);
        arg
    };

    // When we get here, the handler has called lm_rt_resume and switched
    // back to us. Restore our status.
    (*performer_fiber).status = FiberStatus::Running;
    set_current_fiber(ctx, performer_fiber);

    resume_val
}

// ── lm_rt_fiber_set_pinned ────────────────────────────────────────────────────

/// Pin or unpin the current fiber.
///
/// When pinned, `lm_rt_perform` returns null instead of switching fibers.
/// Use this at FFI boundaries to prevent stack corruption when native C code
/// is on the stack.
///
/// # Safety
/// `ctx` must point to a valid VmContext with a valid `current_fiber`.
#[no_mangle]
pub unsafe extern "C" fn lm_rt_fiber_set_pinned(ctx: *mut VmContext, pinned: bool) {
    let fiber = current_fiber_of(ctx);
    if !fiber.is_null() {
        (*fiber).pinned = pinned;
    }
}

// ── lm_rt_resume ─────────────────────────────────────────────────────────────

/// Resume a suspended performer fiber with a result value.
///
/// Called from inside a handler body to pass a value back to the performer
/// and yield control. After this call, the handler fiber is suspended and
/// the performer continues executing.
///
/// # Parameters
/// - `ctx` — VM context.
/// - `value` — The `NbValue` (as `u64`) to deliver to the performer.
///
/// # Returns
/// Currently always returns `NbValue::new_null()` (handlers do not receive
/// a value back from resume in one-shot semantics).
///
/// # Panics
/// Panics in debug builds if there is no suspended performer.
///
/// # Safety
/// Must be called from within a handler fiber (i.e., after `lm_rt_perform`
/// has switched to this fiber).
#[no_mangle]
pub unsafe extern "C" fn lm_rt_resume(ctx: *mut VmContext, value: u64) -> u64 {
    let stack = effect_stack_of(ctx);
    drain_pending_handler_frees(ctx, stack);

    let performer = match stack.suspended.take() {
        Some(p) => p,
        None => {
            debug_assert!(false, "lm_rt_resume: no suspended performer");
            return NbValue::new_null().0;
        }
    };

    let handler_fiber = current_fiber_of(ctx);
    debug_assert!(!handler_fiber.is_null(), "lm_rt_resume: no current fiber");

    let performer_fiber = performer.fiber;
    debug_assert!(
        !performer_fiber.is_null(),
        "lm_rt_resume: null performer fiber"
    );

    // Mark handler as suspended (it will be resumed if perform is called again,
    // or freed when the handle block exits via lm_rt_handle_pop).
    (*handler_fiber).status = FiberStatus::Suspended;
    (*performer_fiber).status = FiberStatus::Running;
    set_current_fiber(ctx, performer_fiber);

    // Switch back to the performer, passing `value` as the resume result.
    // The performer's lm_rt_perform call will return this value.
    #[cfg(target_arch = "x86_64")]
    {
        super::fiber::fiber_switch(handler_fiber, performer_fiber, value);
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (handler_fiber, performer_fiber, value);
    }

    // Handler fiber continues here after the performer calls perform again,
    // or when the performer finishes and the handle block pops the handler.
    NbValue::new_null().0
}

// ── lm_rt_resume_explicit: resume a specific fiber by pointer ────────────────

/// Resume a specific suspended fiber (explicit pointer variant).
///
/// This variant is for JIT code that holds a direct `*mut Fiber` (e.g.,
/// multi-shot continuations or explicit continuation objects).
///
/// # Parameters
/// - `ctx` — VM context.
/// - `fiber` — The suspended performer fiber to resume.
/// - `value` — The `NbValue` (as `u64`) to deliver.
///
/// # Returns
/// The value returned by the resumed fiber (from its next `lm_rt_perform`
/// call or from its normal return).
///
/// # Safety
/// `fiber` must be a valid, non-null pointer to a `Fiber` in `Suspended` state.
#[no_mangle]
pub unsafe extern "C" fn lm_rt_resume_explicit(
    ctx: *mut VmContext,
    fiber: *mut Fiber,
    value: u64,
) -> u64 {
    debug_assert!(!fiber.is_null(), "lm_rt_resume_explicit: null fiber");
    debug_assert_eq!(
        (*fiber).status,
        FiberStatus::Suspended,
        "lm_rt_resume_explicit: fiber is not Suspended"
    );

    let handler_fiber = current_fiber_of(ctx);
    (*handler_fiber).status = FiberStatus::Suspended;
    (*fiber).status = FiberStatus::Running;
    set_current_fiber(ctx, fiber);

    #[cfg(target_arch = "x86_64")]
    let ret = super::fiber::fiber_switch(handler_fiber, fiber, value);

    #[cfg(not(target_arch = "x86_64"))]
    let ret = {
        let _ = (handler_fiber, fiber, value);
        NbValue::new_null().0
    };

    ret
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lumen_core::vm_context::VmContext;

    fn make_ctx_with_stack() -> (*mut VmContext, Box<VmContext>) {
        let mut ctx = Box::new(VmContext::new());
        let ptr = &mut *ctx as *mut VmContext;
        unsafe { lm_rt_effect_stack_init(ptr) };
        (ptr, ctx)
    }

    #[test]
    fn effect_stack_init_and_free() {
        let (ptr, mut ctx) = make_ctx_with_stack();
        assert!(!ctx.effect_stack.is_null());
        unsafe { lm_rt_effect_stack_free(ptr) };
        assert!(ctx.effect_stack.is_null());
    }

    #[test]
    fn handle_push_pop_no_fiber_switch() {
        let (ptr, _ctx) = make_ctx_with_stack();

        extern "C" fn dummy_handler(_arg: u64) {
            // In a real handler this would call lm_rt_resume.
            // For this test the handler is never switched to.
        }

        unsafe {
            // Push a handler for effect 1, op 0.
            lm_rt_handle_push(ptr, 1, 0, dummy_handler);

            let stack = effect_stack_of(ptr);
            assert_eq!(stack.handler_stack.len(), 1);
            assert_eq!(stack.handler_stack[0].effect_id, 1);
            assert_eq!(stack.handler_stack[0].op_id, 0);

            // Pop it — should not panic even though we never switched.
            lm_rt_handle_pop(ptr);
            let stack = effect_stack_of(ptr);
            assert_eq!(stack.handler_stack.len(), 0);

            lm_rt_effect_stack_free(ptr);
        }
    }

    #[test]
    fn handle_pop_defers_free_for_current_handler() {
        let (ptr, _ctx) = make_ctx_with_stack();

        extern "C" fn dummy_handler(_arg: u64) {}

        unsafe {
            lm_rt_handle_push(ptr, 1, 0, dummy_handler);
            let handler = {
                let stack = effect_stack_of(ptr);
                stack.handler_stack[0].handler_fiber
            };

            // Simulate "currently executing on handler fiber".
            (*ptr).current_fiber = handler as *mut ();
            lm_rt_handle_pop(ptr);

            let stack = effect_stack_of(ptr);
            assert!(stack.handler_stack.is_empty());
            assert_eq!(stack.pending_handler_frees.len(), 1);
            assert_eq!(stack.pending_handler_frees[0], handler);

            // Move back to another fiber and trigger a drain.
            let mut main_fiber = Fiber::new(DEFAULT_FIBER_STACK_SIZE);
            (*ptr).current_fiber = &mut *main_fiber as *mut Fiber as *mut ();
            lm_rt_handle_push(ptr, 2, 0, dummy_handler);

            let stack = effect_stack_of(ptr);
            assert!(stack.pending_handler_frees.is_empty());

            lm_rt_handle_pop(ptr);
            lm_rt_effect_stack_free(ptr);
        }
    }

    #[test]
    fn find_handler_innermost() {
        let (ptr, _ctx) = make_ctx_with_stack();

        extern "C" fn h1(_: u64) {}
        extern "C" fn h2(_: u64) {}

        unsafe {
            lm_rt_handle_push(ptr, 1, 0, h1);
            lm_rt_handle_push(ptr, 1, 0, h2); // same effect/op, inner handler

            let stack = effect_stack_of(ptr);
            // find_handler should return the innermost (index 1).
            let idx = stack.find_handler(1, 0).expect("handler not found");
            assert_eq!(idx, 1);

            // Different op_id should not match.
            assert!(stack.find_handler(1, 1).is_none());

            lm_rt_handle_pop(ptr);
            lm_rt_handle_pop(ptr);
            lm_rt_effect_stack_free(ptr);
        }
    }

    #[test]
    fn perform_no_handler_returns_null() {
        let (ptr, _ctx) = make_ctx_with_stack();

        unsafe {
            // No handler installed for effect 99, op 0.
            // Set a dummy current_fiber (non-null but we won't actually switch).
            let mut fiber = Fiber::new(DEFAULT_FIBER_STACK_SIZE);
            (*ptr).current_fiber = &mut *fiber as *mut Fiber as *mut ();

            let result = lm_rt_perform(ptr, 99, 0, NbValue::new_int(42).0);
            assert_eq!(result, NbValue::new_null().0);

            lm_rt_effect_stack_free(ptr);
        }
    }

    /// Integration test: fiber_switch-based perform/resume roundtrip.
    ///
    /// The test sets up:
    ///   - A "main" fiber (simulated by current thread's stack, represented as a Fiber).
    ///   - A handler fiber that receives the perform arg, adds 10, and resumes.
    ///   - Calls lm_rt_perform from "main" and checks the resume value is arg+10.
    #[test]
    #[cfg(all(target_arch = "x86_64", unix))]
    fn perform_resume_roundtrip() {
        use std::sync::atomic::{AtomicU64, Ordering};

        static CTX_PTR: AtomicU64 = AtomicU64::new(0);
        static HANDLER_RECEIVED_ARG: AtomicU64 = AtomicU64::new(0);

        extern "C" fn handler_body(arg: u64) {
            // Record what we received.
            HANDLER_RECEIVED_ARG.store(arg, Ordering::SeqCst);
            // Resume the performer with arg + 10.
            let ctx = CTX_PTR.load(Ordering::SeqCst) as *mut VmContext;
            unsafe {
                lm_rt_resume(ctx, arg + 10);
            }
            // After resume, the handler fiber is suspended.
            // When the test completes and handle_pop frees us, we stop here.
        }

        unsafe {
            // Build a VmContext with an effect stack.
            let mut ctx_box = Box::new(VmContext::new());
            let ctx = &mut *ctx_box as *mut VmContext;
            lm_rt_effect_stack_init(ctx);
            CTX_PTR.store(ctx as u64, Ordering::SeqCst);

            assert!(
                platform::install_stack_growth_handler(),
                "stack overflow handler install failed"
            );
            assert!(
                platform::ensure_thread_stack_growth_handler(),
                "alt signal stack install failed"
            );

            // Create a "main" fiber to represent the current execution context.
            // We don't actually allocate a stack for it — it IS the current stack.
            // We just need a Fiber struct with saved registers.
            let mut main_fiber = Fiber::new(DEFAULT_FIBER_STACK_SIZE);
            main_fiber.status = FiberStatus::Running;
            (*ctx).current_fiber = &mut *main_fiber as *mut Fiber as *mut ();

            // Install a handler for effect 0, op 0.
            lm_rt_handle_push(ctx, 0, 0, handler_body);

            // Perform: passes NbValue::new_int(5) to the handler.
            // The handler resumes with 5 + 10 = 15.
            let arg = NbValue::new_int(5).0;
            let result = lm_rt_perform(ctx, 0, 0, arg);

            assert_eq!(
                HANDLER_RECEIVED_ARG.load(Ordering::SeqCst),
                arg,
                "handler received wrong arg"
            );
            assert_eq!(
                result,
                NbValue::new_int(5).0 + 10,
                "perform returned wrong resume value"
            );

            lm_rt_handle_pop(ctx);
            lm_rt_effect_stack_free(ctx);
        }
    }
}
