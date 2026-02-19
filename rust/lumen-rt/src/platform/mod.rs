//! Platform-specific stack and signal handling

#[cfg(unix)]
pub mod unix;
#[cfg(windows)]
pub mod windows;

#[cfg(unix)]
pub use unix::*;
#[cfg(windows)]
pub use windows::*;

/// Allocate a fiber stack with guard page
pub fn allocate_stack(size: usize) -> (*mut u8, usize) {
    #[cfg(unix)]
    {
        unix::allocate_stack(size)
    }
    #[cfg(windows)]
    {
        windows::allocate_stack(size)
    }
}

/// Free a fiber stack
pub unsafe fn free_stack(base: *mut u8, size: usize) {
    #[cfg(unix)]
    {
        unix::free_stack(base, size)
    }
    #[cfg(windows)]
    {
        windows::free_stack(base, size)
    }
}

/// Protect/unprotect guard page
pub unsafe fn protect_guard_page(base: *mut u8) -> bool {
    #[cfg(unix)]
    {
        unix::protect_guard_page(base)
    }
    #[cfg(windows)]
    {
        windows::protect_guard_page(base)
    }
}

pub unsafe fn unprotect_guard_page(base: *mut u8) -> bool {
    #[cfg(unix)]
    {
        unix::unprotect_guard_page(base)
    }
    #[cfg(windows)]
    {
        windows::unprotect_guard_page(base)
    }
}

/// Get current stack pointer
pub fn current_stack_pointer() -> *mut u8 {
    #[cfg(unix)]
    {
        unix::current_stack_pointer()
    }
    #[cfg(windows)]
    {
        windows::current_stack_pointer()
    }
}

/// Platform page size in bytes.
pub fn page_size() -> usize {
    #[cfg(unix)]
    {
        unix::page_size()
    }
    #[cfg(windows)]
    {
        windows::page_size()
    }
}

/// Update the per-thread current fiber pointer for stack overflow handling.
pub fn set_current_fiber(fiber: *mut crate::vm::fiber::Fiber) {
    #[cfg(unix)]
    {
        unix::set_current_fiber(fiber)
    }
    #[cfg(windows)]
    {
        windows::set_current_fiber(fiber)
    }
}

/// Install the SIGSEGV signal handler for automatic stack growth.
///
/// This should be called once at VM initialization. The handler:
/// 1. Detects when a fiber overflows into its guard page
/// 2. Attempts to grow the stack if possible
/// 3. Aborts if growth fails or if it's a real segfault
///
/// # Safety
/// This installs a process-wide signal handler. It should only be called
/// once and before any fibers are created.
pub unsafe fn install_stack_growth_handler() -> bool {
    #[cfg(unix)]
    {
        unix::install_stack_overflow_handler()
    }
    #[cfg(windows)]
    {
        // Windows uses structured exception handling (SEH) instead of signals
        // This would need a different implementation
        false
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct GrowStackResult {
    pub new_bottom: *mut u8,
    pub new_top: *mut u8,
    pub new_size: usize,
    pub new_rsp: *mut u8,
    pub new_rbp: *mut u8,
}

pub(crate) unsafe fn grow_stack_copy(
    old_bottom: *mut u8,
    old_top: *mut u8,
    old_size: usize,
    new_size: usize,
    current_rsp: *mut u8,
    current_rbp: *mut u8,
) -> Option<GrowStackResult> {
    #[cfg(unix)]
    {
        unix::grow_stack_copy(
            old_bottom,
            old_top,
            old_size,
            new_size,
            current_rsp,
            current_rbp,
        )
    }
    #[cfg(windows)]
    {
        windows::grow_stack_copy(
            old_bottom,
            old_top,
            old_size,
            new_size,
            current_rsp,
            current_rbp,
        )
    }
}
