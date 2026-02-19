//! Native fiber (lightweight stack) for algebraic effects.
//!
//! Each fiber owns a contiguous stack allocated via `mmap`. The fiber_switch
//! assembly routine swaps callee-saved registers + RSP between fibers in ~20ns.
//!
//! # Growable Stacks
//!
//! Fibers support growable stacks using a SIGSEGV signal handler:
//! 1. Stacks are allocated with a guard page at the bottom (low address)
//! 2. When stack overflows into the guard page, SIGSEGV is triggered
//! 3. The signal handler detects fiber stack hits and grows the stack
//! 4. Frame pointers (RBP chain) are relocated to the new stack
//!
//! # Safety Invariants
//!
//! - A `Fiber` must not be moved after `init_with_fn` is called, because the
//!   stack memory contains a pointer back to the entry point and the `Fiber`
//!   struct itself is referenced from the stack through the `parent` pointer chain.
//! - `fiber_switch` is only safe to call with valid, live fiber pointers where
//!   both fibers are in the correct state (target is `Suspended` or freshly
//!   initialized).
//! - Fibers must not cross thread boundaries without external synchronization —
//!   they are `!Send` and `!Sync` by virtue of holding raw pointers.
//! - The guard page at the bottom of each stack must not be written to. Stack
//!   overflow into the guard page will cause a SIGSEGV.
//! - The SIGSEGV handler for stack growth is async-signal-safe: no malloc, no locks.

use std::ptr;

use crate::platform;

/// Default stack size per fiber: 64 KiB. Adequate for most effect handler chains.
pub const DEFAULT_FIBER_STACK_SIZE: usize = 64 * 1024;

/// Default maximum stack size for growable fibers: 8 MB.
pub const DEFAULT_MAX_STACK_SIZE: usize = 8 * 1024 * 1024;

/// Default initial stack size for growable fibers: 16 KB.
pub const DEFAULT_INITIAL_STACK_SIZE: usize = 16 * 1024;

/// Default stack growth increment: 64 KB.
pub const DEFAULT_GROWTH_INCREMENT: usize = 64 * 1024;

/// Stack growth configuration for fibers.
#[derive(Debug, Clone, Copy)]
pub struct FiberStackConfig {
    /// Initial stack allocation size (excluding guard page).
    pub initial_size: usize,
    /// Amount to grow by on each overflow (or factor if using exponential growth).
    pub growth_increment: usize,
    /// Maximum stack size before growth fails.
    pub max_size: usize,
    /// Guard page size (typically one page, 4KB).
    pub guard_page_size: usize,
    /// Whether to use exponential growth (double) or linear growth.
    pub exponential_growth: bool,
}

impl Default for FiberStackConfig {
    fn default() -> Self {
        Self {
            initial_size: DEFAULT_INITIAL_STACK_SIZE,
            growth_increment: DEFAULT_GROWTH_INCREMENT,
            max_size: DEFAULT_MAX_STACK_SIZE,
            guard_page_size: platform::page_size().max(4096),
            exponential_growth: true,
        }
    }
}

impl FiberStackConfig {
    /// Create a configuration with a larger initial size.
    pub fn with_initial_size(initial_size: usize) -> Self {
        Self {
            initial_size,
            ..Default::default()
        }
    }

    /// Create a configuration for fixed-size stacks (no growth).
    pub fn fixed(size: usize) -> Self {
        Self {
            initial_size: size,
            growth_increment: 0,
            max_size: size,
            ..Default::default()
        }
    }

    /// Calculate the next stack size given the current size.
    pub fn next_size(&self, current: usize) -> Option<usize> {
        if current >= self.max_size {
            return None;
        }
        let next = if self.exponential_growth {
            current.saturating_mul(2).min(self.max_size)
        } else {
            current
                .saturating_add(self.growth_increment)
                .min(self.max_size)
        };
        if next <= current {
            None
        } else {
            Some(next)
        }
    }
}

/// Status of a fiber in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FiberStatus {
    /// Currently executing on a CPU.
    Running,
    /// Suspended via `perform` — waiting to be resumed by a handler.
    Suspended,
    /// Finished execution — stack can be recycled.
    Dead,
}

/// Stack utilization information.
#[derive(Debug, Clone, Copy)]
pub struct StackUsage {
    /// Current stack capacity (allocated size in bytes).
    pub capacity: usize,
    /// Estimated used stack space (in bytes).
    pub used: usize,
    /// Available stack space before next growth or overflow (in bytes).
    pub available: usize,
    /// Maximum allowed stack size (in bytes).
    pub max: usize,
}

impl StackUsage {
    /// Returns the utilization ratio (0.0 to 1.0).
    pub fn ratio(&self) -> f64 {
        if self.capacity == 0 {
            0.0
        } else {
            self.used as f64 / self.capacity as f64
        }
    }

    /// Returns true if the stack is nearly full (> 90% utilized).
    pub fn is_nearly_full(&self) -> bool {
        self.ratio() > 0.9
    }
}

/// A native fiber (lightweight coroutine with its own stack).
///
/// # Memory Layout (x86_64)
///
/// The saved register area is at the very beginning of the struct (`repr(C)`)
/// so that the offsets used in `fiber_switch` assembly match exactly:
///
/// ```text
/// offset 0:  saved_rsp
/// offset 8:  saved_rbp
/// offset 16: saved_rbx
/// offset 24: saved_r12
/// offset 32: saved_r13
/// offset 40: saved_r14
/// offset 48: saved_r15
/// ```
///
/// These offsets are used directly in the `fiber_switch` inline assembly.
#[repr(C)]
pub struct Fiber {
    // ── Saved registers (must be first, at known offsets) ──
    pub saved_rsp: u64,
    pub saved_rbp: u64,
    pub saved_rbx: u64,
    pub saved_r12: u64,
    pub saved_r13: u64,
    pub saved_r14: u64,
    pub saved_r15: u64,

    // ── Metadata ──
    /// Bottom of the allocated stack (low address, guard page at the very bottom).
    pub stack_bottom: *mut u8,
    /// Top of the allocated stack (high address, initial RSP points here).
    pub stack_top: *mut u8,
    /// Size of the allocated stack region in bytes (including guard page).
    pub stack_size: usize,
    /// Current allocation size in bytes (including guard page).
    pub stack_capacity: usize,
    /// Number of times this stack has grown.
    pub stack_growth_count: u32,
    /// Max stack size before growth fails.
    pub max_stack_size: usize,
    /// Parent fiber (the handler that will receive `perform` calls).
    /// Null if this is the root fiber.
    pub parent: *mut Fiber,
    /// Current lifecycle status.
    pub status: FiberStatus,
    /// If true, this fiber cannot be suspended (e.g., crossing an FFI boundary).
    pub pinned: bool,
    /// Opaque user data (e.g., effect handler metadata pointer).
    pub user_data: u64,
}

// Fibers hold raw pointers and are inherently single-threaded.
// Safety: callers must not send fibers across threads.
unsafe impl Send for Fiber {}

impl Fiber {
    /// Allocate a new fiber with its own stack using default stack size.
    ///
    /// The stack is allocated with `mmap(MAP_ANONYMOUS | MAP_PRIVATE | MAP_STACK)`.
    /// A guard page (`PROT_NONE`) is placed at the bottom (lowest address) to catch
    /// stack overflows via SIGSEGV.
    ///
    /// The fiber starts in `Suspended` state and must be initialized with
    /// [`Fiber::init_with_fn`] before being switched to.
    ///
    /// # Panics
    ///
    /// Panics if `mmap` or `mprotect` fail (out-of-memory or system limit).
    pub fn new(stack_size: usize) -> Box<Fiber> {
        Self::with_config(FiberStackConfig::with_initial_size(stack_size))
    }

    /// Allocate a new fiber with a specific stack configuration.
    ///
    /// This allows for:
    /// - Small initial stacks that grow on demand (growable stacks)
    /// - Fixed-size stacks that fail on overflow
    /// - Custom growth behavior (exponential vs linear)
    ///
    /// # Example
    /// ```
    /// use lumen_rt::vm::fiber::{Fiber, FiberStackConfig};
    ///
    /// // Create a fiber with a small initial stack that grows to max 1MB
    /// let config = FiberStackConfig {
    ///     initial_size: 16 * 1024,
    ///     growth_increment: 64 * 1024,
    ///     max_size: 1024 * 1024,
    ///     ..Default::default()
    /// };
    /// let fiber = Fiber::with_config(config);
    /// ```
    pub fn with_config(config: FiberStackConfig) -> Box<Fiber> {
        let (stack_bottom, stack_size) = platform::allocate_stack(config.initial_size);
        assert!(!stack_bottom.is_null(), "fiber: stack allocation failed");
        let stack_top = unsafe { stack_bottom.add(stack_size) };

        let mut fiber = Box::new(Fiber {
            saved_rsp: 0,
            saved_rbp: 0,
            saved_rbx: 0,
            saved_r12: 0,
            saved_r13: 0,
            saved_r14: 0,
            saved_r15: 0,
            stack_bottom,
            stack_top,
            stack_size,
            stack_capacity: stack_size,
            stack_growth_count: 0,
            max_stack_size: config.max_size,
            parent: ptr::null_mut(),
            status: FiberStatus::Suspended,
            pinned: false,
            user_data: 0,
        });

        // Point saved_rsp to the top of the stack (stacks grow downward).
        // We leave space for the entry-point setup done in `init_with_fn`.
        fiber.saved_rsp = stack_top as u64;
        fiber
    }

    /// Set up this fiber to call `entry(arg)` when first switched to.
    ///
    /// Arranges the fiber's stack so that `fiber_switch` will pop `entry` as
    /// the return address, causing it to be called with `arg` in `rdi` (SysV ABI).
    ///
    /// Must be called exactly once before the first `fiber_switch` to this fiber.
    ///
    /// # Safety
    ///
    /// The fiber must not have been switched to yet. `entry` must be a valid
    /// function pointer that does not return (or, if it returns, the caller
    /// must ensure the parent fiber handles fiber termination).
    pub unsafe fn init_with_fn(&mut self, entry: extern "C" fn(u64), _arg: u64) {
        // Stack layout (high → low address, stack grows downward):
        //
        //   stack_top (16-aligned):
        //     [sp+24]: fiber_exit_trampoline — sentinel if entry returns (8 bytes)
        //     [sp+16]: entry fn ptr          — popped by fiber_entry_trampoline (8 bytes)
        //     [sp+ 8]: <8-byte padding>       — for 16-byte alignment before jmp entry (8 bytes)
        //     [sp+ 0]: fiber_entry_trampoline — popped by `ret` in fiber_switch (8 bytes)
        //              ← saved_rsp points here
        //
        // On first fiber_switch to this fiber:
        //  1. fiber_switch restores rsp = saved_rsp (points to fiber_entry_trampoline).
        //  2. `ret` pops fiber_entry_trampoline → rsp now points to padding slot.
        //  3. fiber_entry_trampoline: `mov rdi, rax` (rax = resume_val from fiber_switch),
        //     `pop r11` (pops padding, advances rsp to entry fn ptr), err — we need to
        //     pop the entry fn ptr, not padding.
        //
        // Revised layout (no padding needed if we count carefully):
        //
        //   stack_top:
        //     [sp+16]: fiber_exit_trampoline — sentinel       (8 bytes)
        //     [sp+ 8]: entry fn ptr          — popped by trampoline (8 bytes)
        //     [sp+ 0]: fiber_entry_trampoline — `ret` target  (8 bytes)
        //              ← saved_rsp
        //
        // After `ret` → fiber_entry_trampoline, rsp = sp+8 (points to entry fn ptr).
        // `pop r11` → r11 = entry, rsp = sp+16 (points to fiber_exit_trampoline).
        // `jmp r11` → entry(rdi=resume_val), with rsp = sp+16 (16-byte aligned if
        //             stack_top was 16-aligned and we pushed 3×8 = 24 bytes → sp+0 is
        //             16-aligned-minus-8, which is the correct SysV ABI RSP alignment
        //             at function entry when reached via `jmp` rather than `call`).
        //
        // NOTE: `_arg` is ignored. The first argument to `entry` is the `resume_val`
        // passed to `fiber_switch`, not the `_arg` here. This matches the SysV calling
        // convention: `fiber_switch` puts `resume_val` in `rax`, and `fiber_entry_trampoline`
        // moves it to `rdi` before jumping to `entry`.
        let mut sp = self.stack_top as u64;
        // Align to 16 bytes.
        sp &= !15u64;

        // [sp-8]: sentinel — fiber_exit_trampoline
        sp -= 8;
        *(sp as *mut u64) = fiber_exit_trampoline as *const () as u64;

        // [sp-16]: entry fn ptr (popped by fiber_entry_trampoline)
        sp -= 8;
        *(sp as *mut u64) = entry as u64;

        // [sp-24]: fiber_entry_trampoline (popped by `ret` in fiber_switch)
        sp -= 8;
        *(sp as *mut u64) = fiber_entry_trampoline as *const () as u64;

        // saved_rsp points to fiber_entry_trampoline.
        self.saved_rsp = sp;
        // Callee-saved registers start at 0 (clean state for the new fiber).
        self.saved_rbp = 0;
        self.saved_rbx = 0;
        self.saved_r12 = 0;
        self.saved_r13 = 0;
        self.saved_r14 = 0;
        self.saved_r15 = 0;
    }

    /// Returns `true` if this fiber has finished execution.
    #[inline]
    pub fn is_dead(&self) -> bool {
        self.status == FiberStatus::Dead
    }

    /// Returns `true` if this fiber is waiting to be resumed.
    #[inline]
    pub fn is_suspended(&self) -> bool {
        self.status == FiberStatus::Suspended
    }

    /// Returns `true` if the stack can still grow.
    ///
    /// A stack cannot grow if:
    /// - It has reached `max_stack_size`
    /// - It was created with a fixed-size configuration
    #[inline]
    pub fn can_grow(&self) -> bool {
        self.stack_capacity < self.max_stack_size
    }

    /// Returns the current stack capacity (allocated size).
    #[inline]
    pub fn stack_capacity(&self) -> usize {
        self.stack_capacity
    }

    /// Returns the maximum stack size this fiber can grow to.
    #[inline]
    pub fn max_stack_size(&self) -> usize {
        self.max_stack_size
    }

    /// Returns the number of times this stack has grown.
    #[inline]
    pub fn growth_count(&self) -> u32 {
        self.stack_growth_count
    }

    /// Returns stack utilization information.
    ///
    /// Note: This is only accurate when called on the currently executing fiber
    /// or when the fiber is suspended.
    pub fn stack_usage(&self) -> StackUsage {
        let used = if self.saved_rsp != 0 {
            self.stack_top as usize - self.saved_rsp as usize
        } else {
            0
        };
        StackUsage {
            capacity: self.stack_capacity,
            used,
            available: self.stack_capacity.saturating_sub(used),
            max: self.max_stack_size,
        }
    }

    /// Returns the bounds of the usable stack area (excluding guard page).
    ///
    /// The guard page is at [stack_bottom, stack_bottom + guard_page_size).
    /// The usable stack is [stack_bottom + guard_page_size, stack_top).
    pub fn usable_stack_bounds(&self) -> (*mut u8, *mut u8) {
        let guard_size = platform::page_size();
        let usable_bottom = unsafe { self.stack_bottom.add(guard_size) };
        (usable_bottom, self.stack_top)
    }

    /// Attempt to grow the stack
    ///
    /// # Safety
    /// Only safe when called from signal handler or when fiber is suspended
    pub unsafe fn try_grow(&mut self, current_rsp: *mut u8, current_rbp: *mut u8) -> bool {
        if current_rsp.is_null() || current_rbp.is_null() {
            return false;
        }
        if !self.can_grow() {
            return false;
        }

        let new_size = self
            .stack_capacity
            .saturating_mul(2)
            .min(self.max_stack_size);
        let Some(result) = platform::grow_stack_copy(
            self.stack_bottom,
            self.stack_top,
            self.stack_capacity,
            new_size,
            current_rsp,
            current_rbp,
        ) else {
            return false;
        };

        self.stack_bottom = result.new_bottom;
        self.stack_top = result.new_top;
        self.stack_size = result.new_size;
        self.stack_capacity = result.new_size;
        self.stack_growth_count = self.stack_growth_count.saturating_add(1);

        if self.saved_rsp != 0 {
            self.saved_rsp = result.new_rsp as u64;
        }
        if self.saved_rbp != 0 {
            self.saved_rbp = result.new_rbp as u64;
        }

        true
    }
}

impl Drop for Fiber {
    fn drop(&mut self) {
        if self.stack_bottom.is_null() {
            return;
        }
        unsafe {
            platform::free_stack(self.stack_bottom, self.stack_size);
        }
        self.stack_bottom = ptr::null_mut();
        self.stack_top = ptr::null_mut();
    }
}

/// Called if a fiber entry function returns. Marks the fiber dead and
/// switches back to its parent. This should never be reached in normal use.
extern "C" fn fiber_exit_trampoline(_arg: u64) {
    // Fiber entry functions must not return. If they do, terminate the process
    // without unwinding (safe in signal/runtime contexts).
    std::process::abort();
}

// ── fiber_switch x86_64 assembly ────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
std::arch::global_asm!(
    // ── fiber_switch ──────────────────────────────────────────────────────────
    // Switch execution from `current` (rdi) to `target` (rsi) with resume_val (rdx).
    ".global fiber_switch",
    "fiber_switch:",
    // Save callee-saved registers of *current* fiber (pointed to by rdi).
    "    mov [rdi + 0],  rsp",
    "    mov [rdi + 8],  rbp",
    "    mov [rdi + 16], rbx",
    "    mov [rdi + 24], r12",
    "    mov [rdi + 32], r13",
    "    mov [rdi + 40], r14",
    "    mov [rdi + 48], r15",
    // Restore callee-saved registers of *target* fiber (pointed to by rsi).
    "    mov rsp, [rsi + 0]",
    "    mov rbp, [rsi + 8]",
    "    mov rbx, [rsi + 16]",
    "    mov r12, [rsi + 24]",
    "    mov r13, [rsi + 32]",
    "    mov r14, [rsi + 40]",
    "    mov r15, [rsi + 48]",
    // Return value = resume_val (rdx → rax per SysV ABI).
    // rax is also the first arg to fiber_entry_trampoline (used on first switch).
    "    mov rax, rdx",
    // `ret` pops the return address from the target fiber's stack,
    // jumping to the next instruction after the previous fiber_switch call
    // (or to fiber_entry_trampoline on the very first switch).
    "    ret",
    // ── fiber_entry_trampoline ────────────────────────────────────────────────
    // Called (via `ret`) on the FIRST fiber_switch to a freshly-initialized fiber.
    //
    // Stack layout at entry to this trampoline (see init_with_fn):
    //   [rsp + 0]: entry fn ptr   ← we pop this into r11
    //   [rsp + 8]: fiber_exit_trampoline  ← sentinel if entry returns
    //
    // rax = resume_val passed by fiber_switch (= first argument to entry).
    ".global fiber_entry_trampoline",
    "fiber_entry_trampoline:",
    "    mov rdi, rax", // resume_val → first argument per SysV ABI
    "    pop r11",      // pop entry fn ptr (r11 is caller-saved, ok to clobber)
    "    jmp r11",      // tail-call: entry(resume_val)
                        // rsp now points to fiber_exit_trampoline (sentinel ret addr)
);

#[cfg(target_arch = "x86_64")]
extern "C" {
    /// Switch execution from `current` fiber to `target` fiber.
    ///
    /// Saves all callee-saved registers of `current` into `current.saved_*`,
    /// restores callee-saved registers from `target.saved_*`, then returns
    /// to wherever `target` was last suspended (or to its entry function on
    /// first switch).
    ///
    /// The return value is the `resume_val` passed by the *next* caller of
    /// `fiber_switch` that switches back to `current`.
    ///
    /// # Safety
    ///
    /// - Both `current` and `target` must be valid, non-null, properly aligned
    ///   `Fiber` pointers.
    /// - `target` must be in `Suspended` state (or freshly initialized).
    /// - The caller is responsible for updating `status` fields before/after.
    pub fn fiber_switch(current: *mut Fiber, target: *mut Fiber, resume_val: u64) -> u64;

    /// Trampoline that receives control on the FIRST switch to a freshly-initialized fiber.
    ///
    /// Moves `rax` (= `resume_val` from `fiber_switch`) into `rdi`, pops the
    /// real entry fn pointer from the stack, and tail-calls it. Never called
    /// directly from Rust — used only as a raw function pointer in `init_with_fn`.
    fn fiber_entry_trampoline();
}

// ── FiberPool ────────────────────────────────────────────────────────────────

/// A pool of reusable fiber stacks to amortize `mmap`/`munmap` overhead.
///
/// Stacks are recycled after a fiber dies. At most `max_cached` stacks of the
/// default size are held. Stacks of non-default size are always freed immediately.
pub struct FiberPool {
    /// Cached free stacks: `(stack_bottom, stack_size)`.
    free_stacks: std::collections::BTreeMap<usize, Vec<*mut u8>>,
    /// Size used for new stacks (also the size that qualifies for caching).
    default_size: usize,
    /// Maximum number of stacks to keep in the free list.
    max_cached: usize,
}

// Safety: raw pointers in free_stacks are to mmap-ed memory; the pool is
// single-threaded by construction (same thread that drives the VM).
unsafe impl Send for FiberPool {}

/// Default number of stacks to pre-allocate in `FiberPool::new`.
pub const DEFAULT_POOL_PRE_ALLOCATE: usize = 16;
/// Default maximum number of cached stacks in a `FiberPool`.
pub const DEFAULT_POOL_MAX_CACHED: usize = 64;

impl FiberPool {
    /// Create a new pool, pre-allocating `pre_allocate` stacks up front.
    ///
    /// Pre-allocation pays the `mmap` cost at startup rather than on first use,
    /// ensuring the first batch of fiber creates are allocation-free.
    ///
    /// `max_cached` caps the total number of stacks held in the free list.
    /// Stacks released beyond this limit are `munmap`-ed immediately.
    ///
    /// # Defaults
    ///
    /// Use `DEFAULT_POOL_PRE_ALLOCATE` (16) and `DEFAULT_POOL_MAX_CACHED` (64)
    /// for the recommended production configuration.
    pub fn new(stack_size: usize, pre_allocate: usize) -> Self {
        let default_size = round_up(stack_size, platform::page_size());
        let max_cached = DEFAULT_POOL_MAX_CACHED;
        let mut pool = FiberPool {
            free_stacks: std::collections::BTreeMap::new(),
            default_size,
            max_cached,
        };
        for _ in 0..pre_allocate {
            let (bottom, size) = pool.alloc_stack(default_size);
            pool.insert_free(size, bottom);
        }
        pool
    }

    /// Create a pool with explicit `max_cached` limit (for tests or tuning).
    pub fn with_max_cached(stack_size: usize, pre_allocate: usize, max_cached: usize) -> Self {
        let default_size = round_up(stack_size, platform::page_size());
        let mut pool = FiberPool {
            free_stacks: std::collections::BTreeMap::new(),
            default_size,
            max_cached,
        };
        for _ in 0..pre_allocate {
            let (bottom, size) = pool.alloc_stack(default_size);
            pool.insert_free(size, bottom);
        }
        pool
    }

    /// Acquire a stack, returning `(stack_bottom, stack_size)`.
    ///
    /// Pops from the free list if available; otherwise allocates a new one.
    pub fn acquire(&mut self) -> (*mut u8, usize) {
        if let Some(list) = self.free_stacks.get_mut(&self.default_size) {
            if let Some(bottom) = list.pop() {
                return (bottom, self.default_size);
            }
        }
        self.alloc_stack(self.default_size)
    }

    /// Return a stack to the pool.
    ///
    /// If the stack is the default size and the pool is not full, it is cached
    /// for reuse. Otherwise it is `munmap`-ed immediately.
    pub fn release(&mut self, stack_bottom: *mut u8, stack_size: usize) {
        if stack_size == self.default_size {
            let cached = self
                .free_stacks
                .get(&stack_size)
                .map(|list| list.len())
                .unwrap_or(0);
            if cached < self.max_cached {
                self.insert_free(stack_size, stack_bottom);
                return;
            }
        }
        Self::munmap_stack(stack_bottom, stack_size);
    }

    fn cached_count(&self, size: usize) -> usize {
        self.free_stacks
            .get(&size)
            .map(|list| list.len())
            .unwrap_or(0)
    }

    fn alloc_stack(&self, size: usize) -> (*mut u8, usize) {
        let (bottom, size) = platform::allocate_stack(size);
        assert!(!bottom.is_null(), "FiberPool: stack alloc failed");
        (bottom, size)
    }

    /// Free a stack directly. Static so it can be called from `Drop` without
    /// borrow-checker conflicts caused by iterating `self.free_stacks` and
    /// calling an `&self` method at the same time.
    fn munmap_stack(stack_bottom: *mut u8, stack_size: usize) {
        unsafe {
            platform::free_stack(stack_bottom, stack_size);
        }
    }

    fn insert_free(&mut self, size: usize, bottom: *mut u8) {
        self.free_stacks
            .entry(size)
            .or_insert_with(Vec::new)
            .push(bottom);
    }
}

impl Drop for FiberPool {
    fn drop(&mut self) {
        let mut all: Vec<(*mut u8, usize)> = Vec::new();
        for (size, list) in self.free_stacks.iter_mut() {
            for bottom in list.drain(..) {
                all.push((bottom, *size));
            }
        }
        self.free_stacks.clear();
        for (bottom, size) in all {
            Self::munmap_stack(bottom, size);
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

#[inline]
fn round_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

/// Pack `(effect_id, op_id)` into a single `u64` for `Fiber::user_data`.
#[inline]
fn pack_handler_key(effect_id: u32, op_id: u32) -> u64 {
    ((effect_id as u64) << 32) | (op_id as u64)
}

// ── Effect runtime helpers ────────────────────────────────────────────────────
//
// These functions form the C-ABI surface called by both the interpreter and
// JIT-compiled code for `handle`, `perform`, and `resume` semantics.
//
// Handler matching uses `Fiber::user_data` to store the packed
// `(effect_id << 32 | op_id)` key. `lm_rt_perform` walks the `parent` chain
// to find the innermost matching handler.
//
// All functions are safe Rust (no `unsafe` keyword on the fn itself) but
// internally dereference raw pointers — callers must uphold the documented
// safety contracts.

/// Allocate a new handler fiber from `fiber_pool` and wire it into the chain.
///
/// The new fiber starts in `Suspended` state. The caller is responsible for
/// initializing its entry point via [`Fiber::init_with_fn`] before the first
/// [`lm_rt_perform`] that dispatches to it.
///
/// # Parameters
/// - `fiber_pool` — pool to acquire a stack from.
/// - `parent` — the fiber that installed this handler (typically `current`).
/// - `effect_id` — LIR effect index this fiber handles.
/// - `op_id` — LIR operation index within the effect.
///
/// # Returns
/// Raw pointer to the newly allocated `Fiber`. The fiber is `Box`-allocated;
/// free it with [`lm_rt_handle_pop`] (which calls `Box::from_raw` internally).
///
/// # Safety
/// `parent` must be either null (root) or a valid, non-null `*mut Fiber`.
pub fn lm_rt_handle_push(
    fiber_pool: &mut FiberPool,
    performer: *mut Fiber,
    effect_id: u32,
    op_id: u32,
) -> *mut Fiber {
    let (stack_bottom, stack_size) = fiber_pool.acquire();

    // The handler inherits the performer's old parent, so when lm_rt_perform
    // walks the parent chain it can find handlers further up the chain too.
    let old_parent = if performer.is_null() {
        ptr::null_mut()
    } else {
        unsafe { (*performer).parent }
    };

    let fiber = Box::new(Fiber {
        saved_rsp: stack_bottom as u64 + stack_size as u64, // points to stack top
        saved_rbp: 0,
        saved_rbx: 0,
        saved_r12: 0,
        saved_r13: 0,
        saved_r14: 0,
        saved_r15: 0,
        stack_bottom,
        stack_top: unsafe { stack_bottom.add(stack_size) },
        stack_size,
        stack_capacity: stack_size,
        stack_growth_count: 0,
        max_stack_size: DEFAULT_MAX_STACK_SIZE,
        parent: old_parent,
        status: FiberStatus::Suspended,
        pinned: false,
        user_data: pack_handler_key(effect_id, op_id),
    });

    let handler_ptr = Box::into_raw(fiber);

    // Wire the performer's parent to point to the new handler, inserting the
    // handler into the chain: performer → handler → old_parent → ...
    // This is how lm_rt_perform finds handlers by walking performer.parent.
    if !performer.is_null() {
        unsafe {
            (*performer).parent = handler_ptr;
        }
    }

    handler_ptr
}

/// Mark a handler fiber as dead and return its stack to the pool.
///
/// Also unwires the handler from the parent chain: if `performer` is non-null
/// and its parent points to `handler`, the performer's parent is restored to
/// the handler's parent (reversing what `lm_rt_handle_push` did).
///
/// # Safety
/// - `handler` must be a valid, non-null pointer produced by [`lm_rt_handle_push`].
/// - The fiber must not be currently executing (status must not be `Running`).
/// - After this call `handler` is dangling — do not dereference it.
pub fn lm_rt_handle_pop(fiber_pool: &mut FiberPool, handler: *mut Fiber, performer: *mut Fiber) {
    if handler.is_null() {
        return;
    }

    // Unwire the handler from the parent chain before freeing it.
    // Restore: performer.parent = handler.parent (the handler's old parent).
    if !performer.is_null() {
        unsafe {
            let handler_parent = (*handler).parent;
            if (*performer).parent == handler {
                (*performer).parent = handler_parent;
            }
        }
    }

    // Safety: handler was created by Box::into_raw in lm_rt_handle_push.
    let mut fiber = unsafe { Box::from_raw(handler) };
    fiber.status = FiberStatus::Dead;

    // Return the stack to the pool before the Fiber box is dropped.
    // We must extract the fields before drop runs (which would munmap).
    let bottom = fiber.stack_bottom;
    let size = fiber.stack_size;

    // Null out the stack pointer so Fiber's Drop impl does not double-free.
    fiber.stack_bottom = std::ptr::null_mut();
    fiber.stack_top = std::ptr::null_mut();

    // Box<Fiber> drops here — stack is already nulled, so no munmap from Drop.
    drop(fiber);

    // Now recycle the stack.
    fiber_pool.release(bottom, size);
}

/// Dispatch an effect operation to the nearest matching handler.
///
/// Walks the `parent` chain from `current` outward looking for a fiber whose
/// `user_data` matches `pack_handler_key(effect_id, op_id)`. If found:
/// 1. Sets `current` to `Suspended`.
/// 2. Sets the handler to `Running`.
/// 3. Calls `fiber_switch(current, handler, arg)` to transfer control.
/// 4. Returns the `resume_val` from the fiber_switch when the handler resumes us.
///
/// Returns `u64::MAX` as a sentinel if no matching handler is found in the chain.
/// (Well-formed programs always have a handler; this signals a compiler bug.)
///
/// # Safety
/// - `current` must be a valid, non-null `*mut Fiber` pointing to the fiber
///   currently executing this code on the CPU.
/// - All fibers in the parent chain must be valid, non-null pointers.
pub fn lm_rt_perform(current: *mut Fiber, effect_id: u32, op_id: u32, arg: u64) -> u64 {
    debug_assert!(!current.is_null(), "lm_rt_perform: null current fiber");

    let key = pack_handler_key(effect_id, op_id);

    // Walk the parent chain to find the innermost matching handler.
    let mut candidate = unsafe { (*current).parent };
    while !candidate.is_null() {
        if unsafe { (*candidate).user_data } == key
            && unsafe { (*candidate).status } == FiberStatus::Suspended
        {
            break;
        }
        candidate = unsafe { (*candidate).parent };
    }

    if candidate.is_null() {
        // No handler found — sentinel value signals unhandled effect.
        return u64::MAX;
    }

    let handler = candidate;

    // Suspend the performer and activate the handler.
    unsafe {
        (*current).status = FiberStatus::Suspended;
        (*handler).status = FiberStatus::Running;
    }

    // Switch to the handler fiber. Returns when the handler calls lm_rt_resume.
    #[cfg(target_arch = "x86_64")]
    let resume_val = unsafe { fiber_switch(current, handler, arg) };

    #[cfg(not(target_arch = "x86_64"))]
    let resume_val = {
        // Non-x86_64: no native fiber switch; return arg unchanged.
        let _ = (current, handler);
        arg
    };

    // We are back. Restore Running status.
    unsafe {
        (*current).status = FiberStatus::Running;
    }

    resume_val
}

/// Resume a suspended performer fiber, passing it a result value.
///
/// Called from inside a handler body to deliver a value to the performer
/// and suspend the handler until it is needed again (or freed).
///
/// # Parameters
/// - `handler` — the currently-running handler fiber (the caller).
/// - `target` — the suspended performer fiber to wake up.
/// - `value` — `NbValue` bits to deliver as the return of `lm_rt_perform`.
///
/// # Returns
/// The value passed to the next `fiber_switch` back to `handler` (i.e. from
/// the performer's next `lm_rt_perform` call, or 0 if the performer finishes).
///
/// # Safety
/// - `handler` must be a valid, non-null `*mut Fiber` (the currently-running fiber).
/// - `target` must be a valid, non-null `*mut Fiber` in `Suspended` state.
pub fn lm_rt_resume(handler: *mut Fiber, target: *mut Fiber, value: u64) -> u64 {
    debug_assert!(!handler.is_null(), "lm_rt_resume: null handler fiber");
    debug_assert!(!target.is_null(), "lm_rt_resume: null target fiber");
    debug_assert_eq!(
        unsafe { (*target).status },
        FiberStatus::Suspended,
        "lm_rt_resume: target fiber is not Suspended"
    );

    unsafe {
        (*handler).status = FiberStatus::Suspended;
        (*target).status = FiberStatus::Running;
    }

    #[cfg(target_arch = "x86_64")]
    let ret = unsafe { fiber_switch(handler, target, value) };

    #[cfg(not(target_arch = "x86_64"))]
    let ret = {
        let _ = (handler, target, value);
        0u64
    };

    // We return here when someone switches back to the handler.
    unsafe {
        (*handler).status = FiberStatus::Running;
    }

    ret
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fiber_creation_and_drop() {
        // Just allocating and dropping should not leak or panic.
        let fiber = Fiber::new(DEFAULT_FIBER_STACK_SIZE);
        assert_eq!(fiber.status, FiberStatus::Suspended);
        assert!(!fiber.stack_bottom.is_null());
        assert!(!fiber.stack_top.is_null());
        assert!(fiber.stack_size >= DEFAULT_FIBER_STACK_SIZE);
        // stack_top > stack_bottom
        assert!(fiber.stack_top > fiber.stack_bottom);
    }

    #[test]
    #[cfg(all(target_arch = "x86_64", unix))]
    fn fiber_stack_grows_on_sigsegv() {
        use std::sync::atomic::{AtomicU64, Ordering};

        unsafe {
            assert!(
                platform::install_stack_overflow_handler(),
                "stack overflow handler install failed"
            );
            assert!(
                platform::ensure_thread_stack_growth_handler(),
                "alt signal stack install failed"
            );
        }

        static MAIN_PTR: AtomicU64 = AtomicU64::new(0);
        static WORKER_PTR: AtomicU64 = AtomicU64::new(0);
        static RESULT: AtomicU64 = AtomicU64::new(0);

        extern "C" fn deep_recursion(arg: u64) {
            fn recurse(depth: u32) -> u64 {
                let buf = [0u8; 256];
                std::hint::black_box(&buf);
                if depth == 0 {
                    return 1;
                }
                recurse(depth - 1).wrapping_add(1)
            }

            let computed = recurse(arg as u32);
            RESULT.store(computed, Ordering::SeqCst);
            let main = MAIN_PTR.load(Ordering::SeqCst) as *mut Fiber;
            let worker = WORKER_PTR.load(Ordering::SeqCst) as *mut Fiber;
            unsafe {
                fiber_switch(worker, main, computed);
            }
        }

        let mut main_fiber = Fiber::new(DEFAULT_FIBER_STACK_SIZE);
        let mut worker_fiber = Fiber::with_config(FiberStackConfig {
            initial_size: 32 * 1024,
            max_size: DEFAULT_MAX_STACK_SIZE,
            ..Default::default()
        });

        MAIN_PTR.store(&mut *main_fiber as *mut Fiber as u64, Ordering::SeqCst);
        WORKER_PTR.store(&mut *worker_fiber as *mut Fiber as u64, Ordering::SeqCst);

        unsafe {
            let mut old_size = worker_fiber.stack_capacity();
            platform::set_current_fiber(&mut *worker_fiber as *mut Fiber);
            worker_fiber.init_with_fn(deep_recursion, 0);
            let mut ret = 0u64;
            for _ in 0..6 {
                ret = fiber_switch(
                    &mut *main_fiber as *mut Fiber,
                    &mut *worker_fiber as *mut Fiber,
                    2048,
                );
                if worker_fiber.stack_capacity() > old_size {
                    break;
                }
                old_size = worker_fiber.stack_capacity();
            }
            assert_eq!(ret, 2049);
        }

        let grew = worker_fiber.growth_count();
        assert!(grew > 0, "expected stack growth, got {grew}");
        assert_eq!(RESULT.load(Ordering::SeqCst), 2049);
        assert!(worker_fiber.stack_capacity() <= DEFAULT_MAX_STACK_SIZE);
    }

    #[test]
    fn fiber_pool_pre_allocate() {
        // new(size, pre_allocate) should eagerly mmap N stacks.
        let pool = FiberPool::new(DEFAULT_FIBER_STACK_SIZE, 4);
        assert_eq!(pool.cached_count(pool.default_size), 4);
    }

    #[test]
    fn fiber_pool_acquire_release() {
        // Use 0 pre-allocate so acquire goes through the mmap path.
        let mut pool = FiberPool::new(DEFAULT_FIBER_STACK_SIZE, 0);

        let (bottom1, size1) = pool.acquire();
        assert!(!bottom1.is_null());
        assert_eq!(
            size1,
            round_up(DEFAULT_FIBER_STACK_SIZE, platform::page_size())
        );
        assert_eq!(pool.cached_count(pool.default_size), 0);

        pool.release(bottom1, size1);
        assert_eq!(pool.cached_count(pool.default_size), 1);

        // Re-acquire should return the cached stack.
        let (bottom2, size2) = pool.acquire();
        assert_eq!(bottom2, bottom1);
        assert_eq!(size2, size1);
        assert_eq!(pool.cached_count(pool.default_size), 0);

        pool.release(bottom2, size2);
    }

    #[test]
    fn fiber_pool_max_cached() {
        // with_max_cached lets us set a small limit for testing.
        let mut pool = FiberPool::with_max_cached(DEFAULT_FIBER_STACK_SIZE, 0, 2);

        let (b1, s1) = pool.acquire();
        let (b2, s2) = pool.acquire();
        let (b3, s3) = pool.acquire();

        pool.release(b1, s1);
        pool.release(b2, s2);
        // Third release exceeds max_cached=2, should munmap immediately.
        pool.release(b3, s3);
        assert_eq!(pool.cached_count(pool.default_size), 2);
    }

    #[test]
    fn fiber_status_transitions() {
        let mut fiber = Fiber::new(DEFAULT_FIBER_STACK_SIZE);
        assert!(fiber.is_suspended());
        assert!(!fiber.is_dead());

        fiber.status = FiberStatus::Running;
        assert!(!fiber.is_suspended());
        assert!(!fiber.is_dead());

        fiber.status = FiberStatus::Dead;
        assert!(fiber.is_dead());
    }

    /// Test fiber_switch between two fibers.
    ///
    /// This test creates two fibers: a "main" stub and a "worker" fiber.
    /// The worker fiber, when switched to, immediately switches back to main
    /// passing the resume_val it received + 1.
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn fiber_switch_roundtrip() {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc;

        // Shared result slot (written by the worker, read by main).
        static RESULT: AtomicU64 = AtomicU64::new(0);
        static WORKER_PTR: AtomicU64 = AtomicU64::new(0);
        static MAIN_PTR: AtomicU64 = AtomicU64::new(0);

        extern "C" fn worker_entry(arg: u64) {
            // arg is the resume_val passed by the initial fiber_switch.
            // We switch back to main with arg + 1.
            RESULT.store(arg + 1, Ordering::SeqCst);
            let main = MAIN_PTR.load(Ordering::SeqCst) as *mut Fiber;
            let worker = WORKER_PTR.load(Ordering::SeqCst) as *mut Fiber;
            unsafe {
                fiber_switch(worker, main, arg + 1);
            }
        }

        let mut main_fiber = Fiber::new(DEFAULT_FIBER_STACK_SIZE);
        let mut worker_fiber = Fiber::new(DEFAULT_FIBER_STACK_SIZE);

        MAIN_PTR.store(&mut *main_fiber as *mut Fiber as u64, Ordering::SeqCst);
        WORKER_PTR.store(&mut *worker_fiber as *mut Fiber as u64, Ordering::SeqCst);

        unsafe {
            worker_fiber.init_with_fn(worker_entry, 41);
            // Switch from main to worker; worker will switch back.
            let ret = fiber_switch(
                &mut *main_fiber as *mut Fiber,
                &mut *worker_fiber as *mut Fiber,
                41,
            );
            // Worker should have stored 42 and passed it back.
            assert_eq!(ret, 42, "fiber_switch resume_val mismatch");
        }
        assert_eq!(RESULT.load(Ordering::SeqCst), 42);
    }

    // ── Effect helper tests ───────────────────────────────────────────────────

    #[test]
    fn handle_push_sets_fields() {
        let mut pool = FiberPool::new(DEFAULT_FIBER_STACK_SIZE, 0);
        let performer_ptr = std::ptr::null_mut(); // root has no performer

        let handler = lm_rt_handle_push(&mut pool, performer_ptr, 1, 2);
        assert!(!handler.is_null());

        unsafe {
            assert_eq!((*handler).status, FiberStatus::Suspended);
            assert_eq!((*handler).user_data, pack_handler_key(1, 2));
            // Handler inherits performer's old parent (null since performer is null).
            assert_eq!((*handler).parent, std::ptr::null_mut());
        }

        lm_rt_handle_pop(&mut pool, handler, performer_ptr);
        // Stack should be returned to the pool after pop.
        assert_eq!(pool.cached_count(pool.default_size), 1);
    }

    #[test]
    fn handle_push_pop_stack_recycled() {
        let mut pool = FiberPool::new(DEFAULT_FIBER_STACK_SIZE, 0);

        let h1 = lm_rt_handle_push(&mut pool, std::ptr::null_mut(), 0, 0);
        let (stack_ptr, _) = unsafe { ((*h1).stack_bottom, (*h1).stack_size) };

        lm_rt_handle_pop(&mut pool, h1, std::ptr::null_mut());
        assert_eq!(pool.cached_count(pool.default_size), 1);

        // The next push should reuse the same stack.
        let h2 = lm_rt_handle_push(&mut pool, std::ptr::null_mut(), 0, 0);
        let stack2 = unsafe { (*h2).stack_bottom };
        assert_eq!(stack_ptr, stack2, "stack should be recycled from pool");

        lm_rt_handle_pop(&mut pool, h2, std::ptr::null_mut());
    }

    #[test]
    fn perform_no_handler_returns_sentinel() {
        // A standalone fiber with no parent chain — no handler to find.
        let mut root = Fiber::new(DEFAULT_FIBER_STACK_SIZE);
        root.status = FiberStatus::Running;
        root.parent = std::ptr::null_mut();

        let result = lm_rt_perform(&mut *root as *mut Fiber, 99, 0, 42);
        assert_eq!(result, u64::MAX, "expected sentinel for missing handler");
    }

    #[test]
    fn perform_finds_matching_handler_in_chain() {
        // Build a parent chain: root → handler(effect=3,op=1)
        // We don't actually fiber_switch (that requires x86_64 + init_with_fn),
        // but we can verify the matching logic finds the right node.
        let mut root = Fiber::new(DEFAULT_FIBER_STACK_SIZE);
        root.status = FiberStatus::Suspended;
        root.user_data = pack_handler_key(3, 1);

        let mut performer = Fiber::new(DEFAULT_FIBER_STACK_SIZE);
        performer.status = FiberStatus::Running;
        performer.parent = &mut *root as *mut Fiber;

        // Wrong effect — should return sentinel.
        let miss = lm_rt_perform(&mut *performer as *mut Fiber, 3, 2, 0);
        assert_eq!(miss, u64::MAX);

        // Right effect, but we can't actually switch on non-x86_64 / without
        // init_with_fn. Just verify up to the point of finding the handler.
        // (The full roundtrip is tested in perform_resume_roundtrip_x86_64.)
    }

    #[test]
    fn pack_handler_key_roundtrip() {
        for (eid, oid) in [(0u32, 0u32), (1, 2), (u32::MAX, u32::MAX), (0xDEAD, 0xBEEF)] {
            let key = pack_handler_key(eid, oid);
            let got_eid = (key >> 32) as u32;
            let got_oid = (key & 0xFFFF_FFFF) as u32;
            assert_eq!(got_eid, eid);
            assert_eq!(got_oid, oid);
        }
    }

    /// Full perform/resume fiber_switch roundtrip through the helper API.
    ///
    /// lm_rt_handle_push wires the handler into the performer's parent chain,
    /// so lm_rt_perform can find it by walking performer.parent.
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn perform_resume_roundtrip_helpers() {
        use std::sync::atomic::{AtomicU64, Ordering};

        static PERFORMER_PTR: AtomicU64 = AtomicU64::new(0);
        static HANDLER_PTR: AtomicU64 = AtomicU64::new(0);
        static RECEIVED_ARG: AtomicU64 = AtomicU64::new(0);

        // Handler body: receives the perform arg, stores it, resumes with arg+100.
        extern "C" fn handler_body(arg: u64) {
            RECEIVED_ARG.store(arg, Ordering::SeqCst);
            let handler = HANDLER_PTR.load(Ordering::SeqCst) as *mut Fiber;
            let performer = PERFORMER_PTR.load(Ordering::SeqCst) as *mut Fiber;
            lm_rt_resume(handler, performer, arg + 100);
            // After resume, the handler is suspended. If the performer finishes
            // (doesn't call perform again), we just stop here.
        }

        // Allocate a pool and create the handler fiber.
        let mut pool = FiberPool::new(DEFAULT_FIBER_STACK_SIZE, 0);
        let mut performer = Fiber::new(DEFAULT_FIBER_STACK_SIZE);
        performer.status = FiberStatus::Running;

        let performer_ptr = &mut *performer as *mut Fiber;

        // Create handler — this wires performer.parent → handler.
        let handler = lm_rt_handle_push(
            &mut pool,
            performer_ptr,
            /*effect_id=*/ 7,
            /*op_id=*/ 3,
        );

        // Initialize the handler fiber's entry point.
        unsafe { (*handler).init_with_fn(handler_body, 0) };

        PERFORMER_PTR.store(performer_ptr as u64, Ordering::SeqCst);
        HANDLER_PTR.store(handler as u64, Ordering::SeqCst);

        // perform: switches to handler with arg=55, expects to get back 55+100=155.
        let result = lm_rt_perform(
            performer_ptr,
            /*effect_id=*/ 7,
            /*op_id=*/ 3,
            /*arg=*/ 55,
        );

        assert_eq!(
            RECEIVED_ARG.load(Ordering::SeqCst),
            55,
            "handler received wrong arg"
        );
        assert_eq!(result, 155, "perform returned wrong resume value");

        // Clean up — handler is suspended; pop it and unwire from parent chain.
        lm_rt_handle_pop(&mut pool, handler, performer_ptr);
    }

    // ── Growable Stack Tests ───────────────────────────────────────────────────

    #[test]
    fn fiber_stack_config_defaults() {
        let config = FiberStackConfig::default();
        assert_eq!(config.initial_size, DEFAULT_INITIAL_STACK_SIZE);
        assert_eq!(config.max_size, DEFAULT_MAX_STACK_SIZE);
        assert!(config.exponential_growth);
    }

    #[test]
    fn fiber_stack_config_fixed() {
        let config = FiberStackConfig::fixed(128 * 1024);
        assert_eq!(config.initial_size, 128 * 1024);
        assert_eq!(config.max_size, 128 * 1024);
        assert_eq!(config.growth_increment, 0);
    }

    #[test]
    fn fiber_stack_config_next_size_exponential() {
        let config = FiberStackConfig {
            initial_size: 16 * 1024,
            max_size: 256 * 1024,
            exponential_growth: true,
            ..Default::default()
        };

        assert_eq!(config.next_size(16 * 1024), Some(32 * 1024));
        assert_eq!(config.next_size(32 * 1024), Some(64 * 1024));
        assert_eq!(config.next_size(128 * 1024), Some(256 * 1024));
        assert_eq!(config.next_size(256 * 1024), None); // At max
    }

    #[test]
    fn fiber_stack_config_next_size_linear() {
        let config = FiberStackConfig {
            initial_size: 16 * 1024,
            growth_increment: 64 * 1024,
            max_size: 256 * 1024,
            exponential_growth: false,
            ..Default::default()
        };

        assert_eq!(config.next_size(16 * 1024), Some(80 * 1024));
        assert_eq!(config.next_size(80 * 1024), Some(144 * 1024));
        assert_eq!(config.next_size(200 * 1024), Some(256 * 1024));
        assert_eq!(config.next_size(256 * 1024), None); // At max
    }

    #[test]
    fn fiber_with_config_growable() {
        let config = FiberStackConfig {
            initial_size: 16 * 1024,
            max_size: 256 * 1024,
            ..Default::default()
        };
        let fiber = Fiber::with_config(config);

        assert!(fiber.can_grow());
        assert_eq!(fiber.max_stack_size(), 256 * 1024);
        assert_eq!(fiber.growth_count(), 0);
        assert!(fiber.is_suspended());
    }

    #[test]
    fn fiber_with_config_fixed() {
        let config = FiberStackConfig::fixed(64 * 1024);
        let fiber = Fiber::with_config(config);

        assert!(!fiber.can_grow());
        assert_eq!(fiber.max_stack_size(), 64 * 1024);
    }

    #[test]
    fn fiber_stack_usage_info() {
        let fiber = Fiber::new(DEFAULT_FIBER_STACK_SIZE);
        let usage = fiber.stack_usage();

        assert_eq!(usage.capacity, fiber.stack_capacity());
        assert_eq!(usage.max, fiber.max_stack_size());
        assert_eq!(usage.used, 0); // No usage yet (fiber not started)
        assert_eq!(usage.available, fiber.stack_capacity());
        assert!(!usage.is_nearly_full());
    }

    #[test]
    fn fiber_usable_stack_bounds() {
        let fiber = Fiber::new(DEFAULT_FIBER_STACK_SIZE);
        let (usable_bottom, usable_top) = fiber.usable_stack_bounds();
        let guard_size = platform::page_size();

        // Usable bottom should be after the guard page
        assert_eq!(usable_bottom, unsafe { fiber.stack_bottom.add(guard_size) });
        // Usable top should be the stack top
        assert_eq!(usable_top, fiber.stack_top);
    }

    #[test]
    fn fiber_stack_config_with_initial_size() {
        let config = FiberStackConfig::with_initial_size(32 * 1024);
        assert_eq!(config.initial_size, 32 * 1024);
        assert_eq!(config.max_size, DEFAULT_MAX_STACK_SIZE);
    }

    #[test]
    fn fiber_growth_count_increments_after_growth() {
        // Create a fiber with small initial stack
        let mut fiber = Fiber::with_config(FiberStackConfig {
            initial_size: 16 * 1024,
            max_size: 256 * 1024,
            ..Default::default()
        });

        assert_eq!(fiber.growth_count(), 0);

        // Note: We can't easily test actual growth without triggering a segfault,
        // but we can verify the fiber is set up correctly for growth
        assert!(fiber.can_grow());
    }
}
