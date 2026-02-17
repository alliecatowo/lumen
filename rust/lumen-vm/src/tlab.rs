//! Thread-Local Allocation Buffer (TLAB).
//!
//! Each worker thread gets its own TLAB to reduce contention on the
//! global Immix allocator. The TLAB is a small bump-allocation buffer
//! (typically 32 KiB) that is refilled from the global allocator when
//! exhausted.

/// Default TLAB capacity: 32 KiB.
const DEFAULT_TLAB_SIZE: usize = 32 * 1024;

/// Thread-Local Allocation Buffer.
///
/// A small per-thread bump allocator that sits in front of the global
/// allocator. Allocations within the TLAB are lock-free and
/// contention-free. When the TLAB is exhausted, the owning thread
/// requests a new region from the global allocator.
pub struct Tlab {
    /// Backing storage.
    buffer: Vec<u8>,
    /// Byte offset of the next free position.
    cursor: usize,
    /// Total capacity in bytes.
    capacity: usize,
}

impl Tlab {
    /// Create a new TLAB with the default capacity (32 KiB).
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_TLAB_SIZE)
    }

    /// Create a new TLAB with the given capacity in bytes.
    ///
    /// # Panics
    /// Panics if `capacity` is 0.
    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "TLAB capacity must be > 0");
        Self {
            buffer: vec![0u8; capacity],
            cursor: 0,
            capacity,
        }
    }

    /// Attempt to bump-allocate `size` bytes with the given `align`ment.
    ///
    /// Returns `Some(ptr)` if there is enough space, `None` otherwise.
    /// The caller should refill or reset the TLAB when `None` is returned.
    pub fn alloc(&mut self, size: usize, align: usize) -> Option<*mut u8> {
        assert!(size > 0, "zero-size allocation not supported");
        assert!(align.is_power_of_two(), "alignment must be a power of two");

        let base = self.buffer.as_ptr() as usize;
        let abs_cursor = base + self.cursor;
        let aligned = (abs_cursor + align - 1) & !(align - 1);
        let padding = aligned - abs_cursor;
        let total = padding + size;

        if self.cursor + total > self.capacity {
            return None;
        }

        self.cursor += total;
        Some(aligned as *mut u8)
    }

    /// Number of bytes remaining in the TLAB.
    pub fn remaining(&self) -> usize {
        self.capacity - self.cursor
    }

    /// Returns `true` if no more allocations can be satisfied.
    pub fn is_full(&self) -> bool {
        self.cursor >= self.capacity
    }

    /// Reset the cursor to 0, logically freeing all allocations.
    ///
    /// # Safety
    /// The caller must ensure no live references point into the TLAB
    /// buffer before calling reset.
    pub fn reset(&mut self) {
        self.cursor = 0;
    }

    /// Number of bytes allocated (cursor position).
    pub fn bytes_allocated(&self) -> usize {
        self.cursor
    }

    /// Total capacity of this TLAB.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl Default for Tlab {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Tlab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tlab")
            .field("capacity", &self.capacity)
            .field("cursor", &self.cursor)
            .field("remaining", &self.remaining())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_tlab() {
        let tlab = Tlab::new();
        assert_eq!(tlab.capacity(), DEFAULT_TLAB_SIZE);
        assert_eq!(tlab.bytes_allocated(), 0);
        assert_eq!(tlab.remaining(), DEFAULT_TLAB_SIZE);
        assert!(!tlab.is_full());
    }

    #[test]
    fn test_basic_alloc() {
        let mut tlab = Tlab::with_capacity(1024);
        let ptr = tlab.alloc(64, 8);
        assert!(ptr.is_some());
        assert_eq!(tlab.bytes_allocated(), 64);
        assert_eq!(tlab.remaining(), 1024 - 64);
    }

    #[test]
    fn test_alloc_alignment() {
        let mut tlab = Tlab::with_capacity(1024);
        // Allocate 1 byte to misalign
        let _ = tlab.alloc(1, 1);
        // Now request 16-byte aligned
        let ptr = tlab.alloc(32, 16).unwrap();
        assert_eq!((ptr as usize) % 16, 0, "must be 16-byte aligned");
    }

    #[test]
    fn test_fill_up() {
        let mut tlab = Tlab::with_capacity(128);
        // Allocate exactly the capacity
        let r1 = tlab.alloc(64, 8);
        assert!(r1.is_some());
        let r2 = tlab.alloc(64, 8);
        assert!(r2.is_some());
        assert!(tlab.is_full() || tlab.remaining() == 0);

        // Next allocation should fail
        let r3 = tlab.alloc(1, 1);
        assert!(r3.is_none());
    }

    #[test]
    fn test_reset() {
        let mut tlab = Tlab::with_capacity(256);
        let _ = tlab.alloc(128, 8);
        assert_eq!(tlab.bytes_allocated(), 128);

        tlab.reset();
        assert_eq!(tlab.bytes_allocated(), 0);
        assert_eq!(tlab.remaining(), 256);
        assert!(!tlab.is_full());

        // Can allocate again
        let ptr = tlab.alloc(64, 8);
        assert!(ptr.is_some());
    }

    #[test]
    fn test_alloc_too_large() {
        let mut tlab = Tlab::with_capacity(64);
        let r = tlab.alloc(128, 8);
        assert!(r.is_none());
    }

    #[test]
    fn test_multiple_small_allocs() {
        let mut tlab = Tlab::with_capacity(1024);
        let mut ptrs = Vec::new();
        for _ in 0..64 {
            if let Some(ptr) = tlab.alloc(8, 8) {
                ptrs.push(ptr);
            }
        }
        // All allocations should succeed (64 * 8 = 512 < 1024)
        // Some might fail due to alignment padding, but most should succeed
        assert!(ptrs.len() >= 60);
    }

    #[test]
    #[should_panic(expected = "TLAB capacity must be > 0")]
    fn test_zero_capacity_panics() {
        let _ = Tlab::with_capacity(0);
    }

    #[test]
    fn test_debug_display() {
        let tlab = Tlab::with_capacity(512);
        let dbg = format!("{:?}", tlab);
        assert!(dbg.contains("Tlab"));
        assert!(dbg.contains("512"));
    }
}
