//! Unix-specific stack handling with SIGSEGV recovery

use crate::platform::GrowStackResult;
use crate::vm::fiber::{Fiber, DEFAULT_MAX_STACK_SIZE};
use libc::{sigaction, sigemptyset, SA_ONSTACK, SA_SIGINFO, SIGSEGV};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

/// Global flag: are we in a stack growth operation?
static GROWING_STACK: AtomicBool = AtomicBool::new(false);
static CURRENT_FIBER: AtomicPtr<Fiber> = AtomicPtr::new(std::ptr::null_mut());

const DEFAULT_PAGE_SIZE: usize = 4096;

pub fn page_size() -> usize {
    unsafe {
        let size = libc::sysconf(libc::_SC_PAGESIZE);
        if size <= 0 {
            DEFAULT_PAGE_SIZE
        } else {
            size as usize
        }
    }
}

/// Install SIGSEGV handler for stack growth
pub unsafe fn install_stack_overflow_handler() -> bool {
    let mut sa: sigaction = std::mem::zeroed();
    sa.sa_sigaction = stack_overflow_handler as usize;
    sigemptyset(&mut sa.sa_mask);
    sa.sa_flags = SA_ONSTACK | SA_SIGINFO; // Use alternate signal stack

    sigaction(SIGSEGV, &sa, std::ptr::null_mut()) == 0
}

/// Update the current fiber pointer for the calling thread.
pub fn set_current_fiber(fiber: *mut Fiber) {
    CURRENT_FIBER.store(fiber, Ordering::Release);
}

extern "C" fn stack_overflow_handler(
    _sig: i32,
    info: *mut libc::siginfo_t,
    ctx: *mut libc::c_void,
) {
    if info.is_null() || ctx.is_null() {
        unsafe { libc::signal(SIGSEGV, libc::SIG_DFL) };
        return;
    }

    // Prevent re-entrancy.
    if GROWING_STACK.swap(true, Ordering::AcqRel) {
        unsafe { libc::signal(SIGSEGV, libc::SIG_DFL) };
        return;
    }

    let fault_addr = unsafe { (*info).si_addr() as *mut u8 };
    let fiber = CURRENT_FIBER.load(Ordering::Acquire);

    let mut handled = false;
    if !fiber.is_null() {
        let fiber_ref = unsafe { &mut *fiber };
        let guard_page_end = unsafe { fiber_ref.stack_bottom.add(page_size()) };
        if fault_addr >= fiber_ref.stack_bottom && fault_addr < guard_page_end {
            handled = unsafe { try_grow_stack(fiber_ref, ctx) };
        }
    }

    GROWING_STACK.store(false, Ordering::Release);

    if !handled {
        unsafe { libc::signal(SIGSEGV, libc::SIG_DFL) };
    }
}

/// Try to grow the fiber stack
///
/// # Safety
/// Called from signal handler. Must be async-signal-safe.
unsafe fn try_grow_stack(fiber: &mut Fiber, ctx: *mut libc::c_void) -> bool {
    let context = &mut *(ctx as *mut libc::ucontext_t);

    #[cfg(target_arch = "x86_64")]
    let (current_rsp, current_rbp) = {
        let rsp = context.uc_mcontext.gregs[libc::REG_RSP as usize] as *mut u8;
        let rbp = context.uc_mcontext.gregs[libc::REG_RBP as usize] as *mut u8;
        (rsp, rbp)
    };

    #[cfg(not(target_arch = "x86_64"))]
    let (current_rsp, current_rbp) = (std::ptr::null_mut(), std::ptr::null_mut());

    if current_rsp.is_null() || current_rbp.is_null() {
        return false;
    }

    if !fiber.can_grow() {
        return false;
    }

    let new_size = fiber.stack_capacity.saturating_mul(2);
    let Some(result) = grow_stack_copy(
        fiber.stack_bottom,
        fiber.stack_top,
        fiber.stack_capacity,
        new_size,
        current_rsp,
        current_rbp,
    ) else {
        return false;
    };

    // Update fiber metadata and saved registers.
    fiber.stack_bottom = result.new_bottom;
    fiber.stack_top = result.new_top;
    fiber.stack_size = result.new_size;
    fiber.stack_capacity = result.new_size;
    fiber.stack_growth_count = fiber.stack_growth_count.saturating_add(1);

    if fiber.saved_rsp != 0 {
        fiber.saved_rsp = result.new_rsp as u64;
    }
    if fiber.saved_rbp != 0 {
        fiber.saved_rbp = result.new_rbp as u64;
    }

    // Patch signal context to continue execution on the new stack.
    #[cfg(target_arch = "x86_64")]
    {
        context.uc_mcontext.gregs[libc::REG_RSP as usize] = result.new_rsp as libc::greg_t;
        context.uc_mcontext.gregs[libc::REG_RBP as usize] = result.new_rbp as libc::greg_t;
    }

    true
}

pub fn allocate_stack(size: usize) -> (*mut u8, usize) {
    let size = round_up(size, page_size());
    unsafe {
        let ptr = libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_ANONYMOUS | libc::MAP_PRIVATE | libc::MAP_STACK,
            -1,
            0,
        );
        if ptr == libc::MAP_FAILED {
            return (std::ptr::null_mut(), 0);
        }
        let bottom = ptr as *mut u8;
        let ret = libc::mprotect(bottom as *mut libc::c_void, page_size(), libc::PROT_NONE);
        if ret != 0 {
            libc::munmap(bottom as *mut libc::c_void, size);
            return (std::ptr::null_mut(), 0);
        }
        (bottom, size)
    }
}

pub unsafe fn free_stack(base: *mut u8, size: usize) {
    if base.is_null() || size == 0 {
        return;
    }
    libc::munmap(base as *mut libc::c_void, size);
}

pub unsafe fn protect_guard_page(base: *mut u8) -> bool {
    if base.is_null() {
        return false;
    }
    libc::mprotect(base as *mut libc::c_void, page_size(), libc::PROT_NONE) == 0
}

pub unsafe fn unprotect_guard_page(base: *mut u8) -> bool {
    if base.is_null() {
        return false;
    }
    libc::mprotect(
        base as *mut libc::c_void,
        page_size(),
        libc::PROT_READ | libc::PROT_WRITE,
    ) == 0
}

pub fn current_stack_pointer() -> *mut u8 {
    let sp: usize;
    unsafe {
        std::arch::asm!("mov {}, rsp", out(reg) sp, options(nomem, nostack, preserves_flags));
    }
    sp as *mut u8
}

/// Relocate the frame pointer (RBP) chain after stack copy.
///
/// This is critical for proper stack unwinding. The x86_64 System V AMD64 ABI
/// uses RBP as a frame pointer that forms a linked list:
///   current RBP -> saved RBP (previous frame) -> ... -> null
///
/// When we copy the stack, all saved RBP values in the stack frames point to
/// the OLD stack. We need to adjust them to point to the NEW stack.
///
/// # Safety
/// - Must be called with valid old_top and new_top pointers
/// - The RBP chain must be valid (no frame pointer omission)
unsafe fn relocate_frame_pointers(
    old_top: *mut u8,
    new_top: *mut u8,
    new_rsp: *mut u8,
    current_rbp: *mut u8,
) -> *mut u8 {
    // Calculate the offset between old and new stacks
    let stack_offset = new_top as isize - old_top as isize;
    
    // Start with the current RBP
    let mut old_frame_ptr = current_rbp;
    let mut new_frame_ptr = current_rbp.offset(stack_offset);
    
    // Walk the frame pointer chain and fix each saved RBP
    // Safety: We limit the walk to prevent infinite loops on corrupted stacks
    let max_frames = 10000;
    let mut frame_count = 0;
    
    while !old_frame_ptr.is_null() 
        && frame_count < max_frames
        && old_frame_ptr >= old_top.sub(DEFAULT_MAX_STACK_SIZE) as *mut u8
    {
        // The saved RBP is stored at [old_frame_ptr]
        // Read the old saved RBP from the old stack
        let saved_rbp = *(old_frame_ptr as *mut *mut u8);
        
        if saved_rbp.is_null() {
            // End of chain - write null to new stack
            *(new_frame_ptr as *mut *mut u8) = std::ptr::null_mut();
            break;
        }
        
        // Calculate the new saved RBP position
        let new_saved_rbp = saved_rbp.offset(stack_offset);
        *(new_frame_ptr as *mut *mut u8) = new_saved_rbp;
        
        // Move to next frame
        old_frame_ptr = saved_rbp;
        new_frame_ptr = new_saved_rbp;
        frame_count += 1;
    }
    
    // Return the relocated RBP for the current frame
    current_rbp.offset(stack_offset)
}

/// Copy stack contents to a new larger stack and relocate frame pointers.
///
/// # Arguments
/// * `old_bottom` - Bottom of the old stack (lowest address)
/// * `old_top` - Top of the old stack (highest address, initial RSP)
/// * `old_size` - Size of the old stack allocation
/// * `new_size` - Desired size of the new stack
/// * `current_rsp` - Current stack pointer (RSP) at the time of growth
/// * `current_rbp` - Current frame pointer (RBP) at the time of growth
///
/// # Returns
/// Some(GrowStackResult) on success, None on failure
///
/// # Safety
/// This function is async-signal-safe: it only uses mmap/munmap which are
/// safe to call from signal handlers on Linux.
pub(crate) unsafe fn grow_stack_copy(
    old_bottom: *mut u8,
    old_top: *mut u8,
    old_size: usize,
    new_size: usize,
    current_rsp: *mut u8,
    current_rbp: *mut u8,
) -> Option<GrowStackResult> {
    if old_bottom.is_null() || old_top.is_null() || old_size == 0 || new_size <= old_size {
        return None;
    }

    // Allocate new stack
    let (new_bottom, new_size) = allocate_stack(new_size);
    if new_bottom.is_null() || new_size == 0 {
        return None;
    }
    let new_top = new_bottom.add(new_size);

    // Calculate how much of the stack is actually used
    let used = old_top as usize - current_rsp as usize;
    if used > old_size {
        // Stack pointer is corrupted or outside expected range
        free_stack(new_bottom, new_size);
        return None;
    }

    // Copy the used portion to the new stack (at corresponding offset from top)
    let new_rsp = new_top.sub(used);
    std::ptr::copy_nonoverlapping(current_rsp, new_rsp, used);

    // Relocate frame pointers - CRITICAL for stack unwinding
    let new_rbp = relocate_frame_pointers(old_top, new_top, new_rsp, current_rbp);

    // Free the old stack
    free_stack(old_bottom, old_size);

    Some(GrowStackResult {
        new_bottom,
        new_top,
        new_size,
        new_rsp,
        new_rbp,
    })
}

#[inline]
fn round_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}
