//! Bump-pointer arena allocator for process-local allocations.
//!
//! All allocations within an arena share the arena's lifetime.
//! The arena frees all memory at once when dropped, or can be
//! `reset()` to reuse the underlying chunk storage.

use std::alloc::Layout;
use std::fmt;
use std::marker::PhantomData;

/// Default chunk size: 64 KiB.
const DEFAULT_CHUNK_SIZE: usize = 64 * 1024;

/// A simple bump-pointer arena for process-local allocations.
///
/// All allocations are freed at once when the arena is dropped.
/// The arena is `!Send` and `!Sync` — it must stay on the thread
/// that created it.
pub struct Arena {
    /// Backing chunks of memory.
    chunks: Vec<Box<[u8]>>,
    /// Pointer to the next free byte in the current chunk.
    current: *mut u8,
    /// Remaining bytes in the current chunk.
    remaining: usize,
    /// Total bytes actually handed out to callers.
    total_allocated: usize,
    /// Size used for new chunk allocations.
    chunk_size: usize,
    /// Ensure !Send + !Sync.
    _not_send_sync: PhantomData<*mut u8>,
}

impl Arena {
    /// Create a new arena with the default chunk size (64 KiB).
    pub fn new() -> Self {
        Self::with_chunk_size(DEFAULT_CHUNK_SIZE)
    }

    /// Create a new arena with a custom chunk size.
    ///
    /// # Panics
    /// Panics if `chunk_size` is 0.
    pub fn with_chunk_size(chunk_size: usize) -> Self {
        assert!(chunk_size > 0, "chunk_size must be > 0");
        Self {
            chunks: Vec::new(),
            current: std::ptr::null_mut(),
            remaining: 0,
            total_allocated: 0,
            chunk_size,
            _not_send_sync: PhantomData,
        }
    }

    /// Allocate `layout.size()` bytes with `layout.align()` alignment.
    ///
    /// Returns a pointer to the allocated region.
    ///
    /// # Panics
    /// Panics if `layout.size()` is 0.
    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        assert!(size > 0, "zero-size allocations are not supported");

        // Try to bump-allocate in the current chunk.
        if let Some(ptr) = self.try_alloc_in_current(size, align) {
            return ptr;
        }

        // Need a new chunk. If the request is larger than chunk_size, allocate
        // an oversize chunk exactly for this allocation.
        let needed = size + align - 1; // worst-case alignment padding
        let alloc_size = if needed > self.chunk_size {
            needed
        } else {
            self.chunk_size
        };

        self.add_chunk(alloc_size);
        self.try_alloc_in_current(size, align)
            .expect("fresh chunk should satisfy allocation")
    }

    /// Allocate and write a value of type `T`, returning a mutable reference.
    ///
    /// # Safety note
    /// The returned reference is valid until the arena is reset or dropped.
    /// `T`'s `Drop` impl will **not** be called.
    pub fn alloc_value<T>(&mut self, val: T) -> &mut T {
        let layout = Layout::new::<T>();
        // For ZSTs, return a dangling aligned pointer.
        if layout.size() == 0 {
            // SAFETY: ZST requires no actual memory.
            return unsafe { &mut *std::ptr::NonNull::dangling().as_ptr() };
        }
        let ptr = self.alloc(layout) as *mut T;
        unsafe {
            ptr.write(val);
            &mut *ptr
        }
    }

    /// Reset the arena: reclaim all allocations but keep the underlying
    /// chunks for reuse. The cursor is rewound to the start of the first chunk.
    ///
    /// # Safety
    /// All previously returned pointers become invalid after this call.
    /// The caller must ensure nothing references arena memory.
    pub fn reset(&mut self) {
        self.total_allocated = 0;
        if let Some(first) = self.chunks.first_mut() {
            self.current = first.as_mut_ptr();
            self.remaining = first.len();
        } else {
            self.current = std::ptr::null_mut();
            self.remaining = 0;
        }
        // Keep only the first chunk to reduce fragmentation on repeated use.
        // The others are dropped, freeing their memory.
        self.chunks.truncate(1);
    }

    /// Total bytes handed out to callers (excluding alignment padding).
    pub fn bytes_allocated(&self) -> usize {
        self.total_allocated
    }

    /// Total bytes reserved across all chunks.
    pub fn bytes_reserved(&self) -> usize {
        self.chunks.iter().map(|c| c.len()).sum()
    }

    /// Number of backing chunks.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    // --- internal helpers ---

    /// Try to bump-allocate within the current chunk.
    fn try_alloc_in_current(&mut self, size: usize, align: usize) -> Option<*mut u8> {
        if self.current.is_null() {
            return None;
        }

        let current = self.current as usize;
        let aligned = (current + align - 1) & !(align - 1);
        let padding = aligned - current;
        let total = padding + size;

        if total > self.remaining {
            return None;
        }

        let ptr = aligned as *mut u8;
        // SAFETY: ptr + size is within the current chunk bounds.
        self.current = unsafe { ptr.add(size) };
        self.remaining -= total;
        self.total_allocated += size;

        Some(ptr)
    }

    /// Allocate a new chunk of at least `min_size` bytes and make it current.
    fn add_chunk(&mut self, min_size: usize) {
        let chunk: Box<[u8]> = vec![0u8; min_size].into_boxed_slice();
        let ptr = chunk.as_ptr() as *mut u8;
        let len = chunk.len();

        self.chunks.push(chunk);
        self.current = ptr;
        self.remaining = len;
    }
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Arena {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Arena")
            .field("chunks", &self.chunk_count())
            .field("bytes_allocated", &self.bytes_allocated())
            .field("bytes_reserved", &self.bytes_reserved())
            .field("remaining", &self.remaining)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_allocation() {
        let mut arena = Arena::new();
        let layout = Layout::from_size_align(64, 8).unwrap();
        let ptr = arena.alloc(layout);
        assert!(!ptr.is_null());
        assert_eq!(arena.bytes_allocated(), 64);
    }

    #[test]
    fn test_alloc_value() {
        let mut arena = Arena::new();
        let val = arena.alloc_value(42u64);
        assert_eq!(*val, 42u64);
        *val = 99;
        assert_eq!(*val, 99);
    }

    #[test]
    fn test_alloc_value_struct() {
        #[derive(Debug, PartialEq)]
        struct Point {
            x: f64,
            y: f64,
        }
        let mut arena = Arena::new();
        let p = arena.alloc_value(Point { x: 1.0, y: 2.0 });
        assert_eq!(p.x, 1.0);
        assert_eq!(p.y, 2.0);
    }

    #[test]
    fn test_alignment() {
        let mut arena = Arena::new();

        // Allocate 1 byte with 1-byte alignment (misalign the cursor)
        let _ = arena.alloc(Layout::from_size_align(1, 1).unwrap());

        // Now request 8-byte alignment
        let layout = Layout::from_size_align(16, 8).unwrap();
        let ptr = arena.alloc(layout);
        assert_eq!((ptr as usize) % 8, 0, "pointer must be 8-byte aligned");

        // 16-byte alignment
        let layout = Layout::from_size_align(32, 16).unwrap();
        let ptr = arena.alloc(layout);
        assert_eq!((ptr as usize) % 16, 0, "pointer must be 16-byte aligned");
    }

    #[test]
    fn test_multiple_chunks() {
        // Use small chunks to force multiple chunk allocation.
        let mut arena = Arena::with_chunk_size(128);
        for _ in 0..20 {
            let _ = arena.alloc(Layout::from_size_align(64, 8).unwrap());
        }
        assert!(
            arena.chunk_count() > 1,
            "should have allocated multiple chunks"
        );
    }

    #[test]
    fn test_oversize_allocation() {
        let mut arena = Arena::with_chunk_size(128);
        let layout = Layout::from_size_align(256, 8).unwrap();
        let ptr = arena.alloc(layout);
        assert!(!ptr.is_null());
        assert!(arena.bytes_allocated() >= 256);
    }

    #[test]
    fn test_reset() {
        let mut arena = Arena::with_chunk_size(256);
        let _ = arena.alloc(Layout::from_size_align(64, 8).unwrap());
        let _ = arena.alloc(Layout::from_size_align(64, 8).unwrap());
        assert!(arena.bytes_allocated() >= 128);

        arena.reset();
        assert_eq!(arena.bytes_allocated(), 0);
        // Chunks are kept for reuse.
        assert!(arena.chunk_count() <= 1);

        // Can allocate again after reset.
        let ptr = arena.alloc(Layout::from_size_align(32, 8).unwrap());
        assert!(!ptr.is_null());
        assert_eq!(arena.bytes_allocated(), 32);
    }

    #[test]
    fn test_reuse_after_reset() {
        let mut arena = Arena::with_chunk_size(256);
        let _ = arena.alloc(Layout::from_size_align(128, 8).unwrap());
        let reserved_before = arena.bytes_reserved();

        arena.reset();
        let _ = arena.alloc(Layout::from_size_align(128, 8).unwrap());

        // After reset, the first chunk is reused so reserved shouldn't grow.
        assert_eq!(arena.bytes_reserved(), reserved_before);
    }

    #[test]
    fn test_bytes_reserved() {
        let mut arena = Arena::with_chunk_size(1024);
        assert_eq!(arena.bytes_reserved(), 0);
        let _ = arena.alloc(Layout::from_size_align(8, 8).unwrap());
        assert!(arena.bytes_reserved() >= 1024);
    }

    #[test]
    fn test_empty_arena() {
        let arena = Arena::new();
        assert_eq!(arena.bytes_allocated(), 0);
        assert_eq!(arena.bytes_reserved(), 0);
        assert_eq!(arena.chunk_count(), 0);
    }

    #[test]
    #[should_panic(expected = "chunk_size must be > 0")]
    fn test_zero_chunk_size_panics() {
        let _ = Arena::with_chunk_size(0);
    }
}
