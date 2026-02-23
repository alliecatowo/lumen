//! Per-frame bump-pointer arena for JIT-allocated Values.
//!
//! Allocated once per call frame, freed en masse on frame exit.
//! NOT thread-safe — each fiber/thread has its own.

use std::mem;

use crate::values::Value;

/// Size of each arena chunk (64 KiB).
pub const ARENA_CHUNK_SIZE: usize = 64 * 1024;

/// Per-frame bump-pointer arena for JIT-allocated Values.
///
/// Values allocated in the arena live until `reset()` or `clear()` is called.
///
/// # Safety
///
/// This arena is **not** thread-safe. It must remain on the same thread/fiber
/// where it was created.
pub struct ValueArena {
    /// Backing chunks of memory (current chunk is the last one).
    chunks: Vec<Box<[u8]>>,
    /// Current bump position within the current chunk.
    bump: usize,
    /// Capacity of the current chunk.
    capacity: usize,
    /// Pointers to Values allocated in this arena (for drop on reset/clear).
    values: Vec<*mut Value>,
}

impl ValueArena {
    /// Create a new empty arena.
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            bump: 0,
            capacity: 0,
            values: Vec::new(),
        }
    }

    /// Allocate `size` bytes, aligned to `align` bytes.
    ///
    /// Never returns null — grows by adding a new chunk if needed.
    ///
    /// # Panics
    ///
    /// Panics if `align` is not a power of two.
    pub fn alloc_aligned(&mut self, size: usize, align: usize) -> *mut u8 {
        debug_assert!(align.is_power_of_two(), "align must be power of two");
        if size == 0 {
            return std::ptr::NonNull::<u8>::dangling().as_ptr();
        }

        let aligned = (self.bump + align - 1) & !(align - 1);
        if aligned + size <= self.capacity {
            self.bump = aligned + size;
            return unsafe { self.current_chunk_ptr().add(aligned) };
        }

        self.grow(size, align)
    }

    /// Allocate `size` bytes, aligned to 8 bytes.
    pub fn alloc(&mut self, size: usize) -> *mut u8 {
        self.alloc_aligned(size, 8)
    }

    /// Allocate a Value in the arena.
    ///
    /// Returns a raw pointer valid for the arena's lifetime.
    pub fn alloc_value(&mut self, v: Value) -> *mut Value {
        let size = mem::size_of::<Value>();
        let align = mem::align_of::<Value>();
        let ptr = self.alloc_aligned(size, align) as *mut Value;
        unsafe {
            ptr.write(v);
        }
        self.values.push(ptr);
        ptr
    }

    /// Reset the arena (free all allocations).
    /// Called on frame exit.
    pub fn reset(&mut self) {
        unsafe {
            for ptr in self.values.drain(..) {
                ptr.drop_in_place();
            }
        }
        self.bump = 0;
        if let Some(last) = self.chunks.last() {
            self.capacity = last.len();
        } else {
            self.capacity = 0;
        }
    }

    /// Returns true if the pointer lies within any arena chunk.
    pub fn contains_ptr<T>(&self, ptr: *const T) -> bool {
        let addr = ptr as usize;
        for chunk in &self.chunks {
            let start = chunk.as_ptr() as usize;
            let end = start + chunk.len();
            if addr >= start && addr < end {
                return true;
            }
        }
        false
    }

    /// Free all chunks.
    pub fn clear(&mut self) {
        unsafe {
            for ptr in self.values.drain(..) {
                ptr.drop_in_place();
            }
        }
        self.chunks.clear();
        self.bump = 0;
        self.capacity = 0;
    }

    fn current_chunk_ptr(&mut self) -> *mut u8 {
        let Some(chunk) = self.chunks.last_mut() else {
            return std::ptr::null_mut();
        };
        chunk.as_mut_ptr()
    }

    fn grow(&mut self, size: usize, align: usize) -> *mut u8 {
        let needed = size.saturating_add(align - 1);
        let chunk_size = if needed > ARENA_CHUNK_SIZE {
            needed
        } else {
            ARENA_CHUNK_SIZE
        };
        let chunk: Box<[u8]> = vec![0u8; chunk_size].into_boxed_slice();
        self.capacity = chunk.len();
        self.bump = 0;
        self.chunks.push(chunk);

        let aligned = (self.bump + align - 1) & !(align - 1);
        self.bump = aligned + size;
        unsafe { self.current_chunk_ptr().add(aligned) }
    }
}

impl Default for ValueArena {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ValueArena {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn arena_alloc_value_drops_on_reset() {
        let mut arena = ValueArena::new();
        let shared = Arc::new(vec![Value::Int(1)]);
        let keep = shared.clone();
        let value = Value::List(shared);
        let _ptr = arena.alloc_value(value);
        assert_eq!(Arc::strong_count(&keep), 2);
        arena.reset();
        assert_eq!(Arc::strong_count(&keep), 1);
    }
}
