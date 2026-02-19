//! Windows-specific stack handling (stub)

use crate::platform::GrowStackResult;

pub fn allocate_stack(_size: usize) -> (*mut u8, usize) {
    (std::ptr::null_mut(), 0)
}

pub unsafe fn free_stack(_base: *mut u8, _size: usize) {}

pub unsafe fn protect_guard_page(_base: *mut u8) -> bool {
    false
}

pub unsafe fn unprotect_guard_page(_base: *mut u8) -> bool {
    false
}

pub fn current_stack_pointer() -> *mut u8 {
    std::ptr::null_mut()
}

pub fn page_size() -> usize {
    4096
}

pub fn set_current_fiber(_fiber: *mut crate::vm::fiber::Fiber) {}

pub(crate) unsafe fn grow_stack_copy(
    _old_bottom: *mut u8,
    _old_top: *mut u8,
    _old_size: usize,
    _new_size: usize,
    _current_rsp: *mut u8,
    _current_rbp: *mut u8,
) -> Option<GrowStackResult> {
    None
}
