//! Immix-style block/line allocator skeleton.
//!
//! Implements the block and line abstraction from the Immix collector
//! (Blackburn & McKinley, 2008). Blocks are 32 KiB and divided into
//! 128-byte lines. The allocator bump-allocates within lines and
//! recycles partially-free blocks by finding holes (contiguous
//! unmarked lines).

/// Block size in bytes (32 KiB).
pub const BLOCK_SIZE: usize = 32 * 1024;
/// Line size in bytes (128 bytes).
pub const LINE_SIZE: usize = 128;
/// Number of lines per block.
pub const LINES_PER_BLOCK: usize = BLOCK_SIZE / LINE_SIZE;

/// A single Immix block: a 32 KiB region divided into 128-byte lines.
pub struct Block {
    /// Raw storage.
    data: Box<[u8; BLOCK_SIZE]>,
    /// Per-line mark bitmap. `true` = line contains at least one live object.
    line_marks: [bool; LINES_PER_BLOCK],
    /// Cached count of holes (contiguous runs of unmarked lines).
    hole_count: u32,
}

impl Block {
    /// Create a new zeroed block with all lines unmarked.
    pub fn new() -> Self {
        Self {
            data: Box::new([0u8; BLOCK_SIZE]),
            line_marks: [false; LINES_PER_BLOCK],
            hole_count: 0,
        }
    }

    /// Mark a line as containing live data.
    ///
    /// # Panics
    /// Panics if `line_idx >= LINES_PER_BLOCK`.
    pub fn mark_line(&mut self, line_idx: usize) {
        assert!(
            line_idx < LINES_PER_BLOCK,
            "line index {line_idx} out of range (max {})",
            LINES_PER_BLOCK - 1
        );
        self.line_marks[line_idx] = true;
    }

    /// Clear the mark for a line.
    ///
    /// # Panics
    /// Panics if `line_idx >= LINES_PER_BLOCK`.
    pub fn unmark_line(&mut self, line_idx: usize) {
        assert!(
            line_idx < LINES_PER_BLOCK,
            "line index {line_idx} out of range (max {})",
            LINES_PER_BLOCK - 1
        );
        self.line_marks[line_idx] = false;
    }

    /// Check whether a line is marked.
    ///
    /// # Panics
    /// Panics if `line_idx >= LINES_PER_BLOCK`.
    pub fn is_line_marked(&self, line_idx: usize) -> bool {
        assert!(
            line_idx < LINES_PER_BLOCK,
            "line index {line_idx} out of range (max {})",
            LINES_PER_BLOCK - 1
        );
        self.line_marks[line_idx]
    }

    /// Find the next hole (contiguous run of unmarked lines) starting
    /// at or after `start_line`.
    ///
    /// Returns `Some((start, len))` where `start` is the first unmarked
    /// line index and `len` is the number of contiguous unmarked lines.
    /// Returns `None` if no hole exists from `start_line` onward.
    pub fn find_hole(&self, start_line: usize) -> Option<(usize, usize)> {
        if start_line >= LINES_PER_BLOCK {
            return None;
        }

        // Skip marked lines to find the start of a hole.
        let mut i = start_line;
        while i < LINES_PER_BLOCK && self.line_marks[i] {
            i += 1;
        }
        if i >= LINES_PER_BLOCK {
            return None;
        }

        let hole_start = i;
        while i < LINES_PER_BLOCK && !self.line_marks[i] {
            i += 1;
        }

        Some((hole_start, i - hole_start))
    }

    /// Count the number of holes (contiguous runs of unmarked lines).
    pub fn count_holes(&self) -> u32 {
        let mut count = 0u32;
        let mut in_hole = false;
        for &marked in &self.line_marks {
            if !marked {
                if !in_hole {
                    count += 1;
                    in_hole = true;
                }
            } else {
                in_hole = false;
            }
        }
        count
    }

    /// Recalculate and cache the hole count. Called during sweep.
    pub fn update_hole_count(&mut self) {
        self.hole_count = self.count_holes();
    }

    /// Get the cached hole count.
    pub fn hole_count(&self) -> u32 {
        self.hole_count
    }

    /// Returns `true` if all lines are unmarked (block is completely free).
    pub fn is_empty(&self) -> bool {
        self.line_marks.iter().all(|&m| !m)
    }

    /// Returns `true` if all lines are marked (no free space).
    pub fn is_full(&self) -> bool {
        self.line_marks.iter().all(|&m| m)
    }

    /// Get a raw pointer to the start of a given line.
    ///
    /// # Panics
    /// Panics if `line_idx >= LINES_PER_BLOCK`.
    pub fn line_ptr(&mut self, line_idx: usize) -> *mut u8 {
        assert!(
            line_idx < LINES_PER_BLOCK,
            "line index {line_idx} out of range (max {})",
            LINES_PER_BLOCK - 1
        );
        unsafe { self.data.as_mut_ptr().add(line_idx * LINE_SIZE) }
    }

    /// Clear all line marks (used when recycling a block).
    pub fn clear_marks(&mut self) {
        self.line_marks = [false; LINES_PER_BLOCK];
        self.hole_count = 0;
    }
}

impl Default for Block {
    fn default() -> Self {
        Self::new()
    }
}

/// Immix-style allocator managing a set of blocks.
///
/// Allocation bump-allocates within lines of the current block.
/// When a line fills up, it advances to the next line. When a block
/// fills up, it takes a new block (from the free list, recyclable
/// list, or by allocating fresh).
pub struct ImmixAllocator {
    /// Blocks currently owned by this allocator.
    blocks: Vec<Block>,
    /// Index of the current block in `blocks`.
    current_block: usize,
    /// Current line within the current block.
    current_line: usize,
    /// Byte cursor within the current line (0..LINE_SIZE).
    cursor: usize,
    /// Fully free blocks available for immediate use.
    free_blocks: Vec<Block>,
    /// Partially-occupied blocks (have holes) available for recycling.
    recyclable_blocks: Vec<Block>,
}

impl ImmixAllocator {
    /// Create a new allocator with one initial block.
    pub fn new() -> Self {
        Self {
            blocks: vec![Block::new()],
            current_block: 0,
            current_line: 0,
            cursor: 0,
            free_blocks: Vec::new(),
            recyclable_blocks: Vec::new(),
        }
    }

    /// Attempt to allocate `size` bytes with the given `align`ment.
    ///
    /// Returns `Some(ptr)` on success, `None` if no space is available
    /// (caller should trigger GC or allocate a new block).
    pub fn alloc(&mut self, size: usize, align: usize) -> Option<*mut u8> {
        assert!(size > 0, "zero-size allocation not supported");
        assert!(align.is_power_of_two(), "alignment must be a power of two");

        // Fast path: try current line in current block.
        if let Some(ptr) = self.try_alloc_in_current_line(size, align) {
            return Some(ptr);
        }

        // Overflow: advance to the next line or block.
        self.advance_line();
        if let Some(ptr) = self.try_alloc_in_current_line(size, align) {
            return Some(ptr);
        }

        // Current block is full. Try to get a new block.
        if self.advance_block() {
            return self.try_alloc_in_current_line(size, align);
        }

        None
    }

    /// Allocate and add a fresh block, making it the current block.
    pub fn alloc_new_block(&mut self) {
        self.blocks.push(Block::new());
        self.current_block = self.blocks.len() - 1;
        self.current_line = 0;
        self.cursor = 0;
    }

    /// Run the sweep phase: categorize blocks into free, recyclable,
    /// and fully occupied. Blocks with no live lines are moved to
    /// the free list; partially live blocks go to the recyclable list.
    pub fn sweep(&mut self) {
        let mut kept = Vec::new();

        for mut block in self.blocks.drain(..) {
            block.update_hole_count();
            if block.is_empty() {
                block.clear_marks();
                self.free_blocks.push(block);
            } else if !block.is_full() {
                self.recyclable_blocks.push(block);
            } else {
                kept.push(block);
            }
        }

        self.blocks = kept;
        self.current_block = 0;
        self.current_line = 0;
        self.cursor = 0;
    }

    /// Total number of active blocks (not counting free/recyclable).
    pub fn active_block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Number of blocks in the free list.
    pub fn free_block_count(&self) -> usize {
        self.free_blocks.len()
    }

    /// Number of blocks in the recyclable list.
    pub fn recyclable_block_count(&self) -> usize {
        self.recyclable_blocks.len()
    }

    // --- internal helpers ---

    /// Try to bump-allocate within the current line of the current block.
    fn try_alloc_in_current_line(&mut self, size: usize, align: usize) -> Option<*mut u8> {
        if self.blocks.is_empty() {
            return None;
        }
        let block = &mut self.blocks[self.current_block];
        let line_start = block.line_ptr(self.current_line) as usize;
        let abs_cursor = line_start + self.cursor;
        let aligned = (abs_cursor + align - 1) & !(align - 1);
        let padding = aligned - abs_cursor;
        let needed = padding + size;

        let remaining_in_line = LINE_SIZE - self.cursor;
        if needed > remaining_in_line {
            return None;
        }

        self.cursor += needed;
        Some(aligned as *mut u8)
    }

    /// Advance to the next unmarked line in the current block.
    fn advance_line(&mut self) {
        if self.blocks.is_empty() {
            return;
        }
        self.current_line += 1;
        self.cursor = 0;

        // Skip over marked lines.
        while self.current_line < LINES_PER_BLOCK {
            if !self.blocks[self.current_block].is_line_marked(self.current_line) {
                return;
            }
            self.current_line += 1;
        }
    }

    /// Try to advance to the next available block.
    /// Returns `true` if a block was found, `false` otherwise.
    fn advance_block(&mut self) -> bool {
        // Try recyclable blocks first (they have holes).
        if let Some(block) = self.recyclable_blocks.pop() {
            // Find the first hole.
            let first_hole = block.find_hole(0);
            self.blocks.push(block);
            self.current_block = self.blocks.len() - 1;
            if let Some((start, _)) = first_hole {
                self.current_line = start;
            } else {
                self.current_line = 0;
            }
            self.cursor = 0;
            return true;
        }

        // Then free blocks.
        if let Some(block) = self.free_blocks.pop() {
            self.blocks.push(block);
            self.current_block = self.blocks.len() - 1;
            self.current_line = 0;
            self.cursor = 0;
            return true;
        }

        false
    }
}

impl Default for ImmixAllocator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Block tests ---

    #[test]
    fn test_block_new_is_empty() {
        let block = Block::new();
        assert!(block.is_empty());
        assert!(!block.is_full());
    }

    #[test]
    fn test_block_mark_unmark() {
        let mut block = Block::new();
        assert!(!block.is_line_marked(0));
        block.mark_line(0);
        assert!(block.is_line_marked(0));
        block.unmark_line(0);
        assert!(!block.is_line_marked(0));
    }

    #[test]
    fn test_block_is_full() {
        let mut block = Block::new();
        for i in 0..LINES_PER_BLOCK {
            block.mark_line(i);
        }
        assert!(block.is_full());
        assert!(!block.is_empty());
    }

    #[test]
    fn test_block_find_hole_entire_block() {
        let block = Block::new();
        let hole = block.find_hole(0);
        assert_eq!(hole, Some((0, LINES_PER_BLOCK)));
    }

    #[test]
    fn test_block_find_hole_after_marks() {
        let mut block = Block::new();
        // Mark lines 0..5
        for i in 0..5 {
            block.mark_line(i);
        }
        let hole = block.find_hole(0);
        assert_eq!(hole, Some((5, LINES_PER_BLOCK - 5)));
    }

    #[test]
    fn test_block_find_hole_in_middle() {
        let mut block = Block::new();
        // Mark lines 0..3 and 6..10
        for i in 0..3 {
            block.mark_line(i);
        }
        for i in 6..10 {
            block.mark_line(i);
        }
        let hole = block.find_hole(0);
        assert_eq!(hole, Some((3, 3))); // lines 3,4,5
    }

    #[test]
    fn test_block_find_hole_none_when_full() {
        let mut block = Block::new();
        for i in 0..LINES_PER_BLOCK {
            block.mark_line(i);
        }
        assert_eq!(block.find_hole(0), None);
    }

    #[test]
    fn test_block_count_holes() {
        let mut block = Block::new();
        // Pattern: [marked, unmarked, unmarked, marked, unmarked]...
        // Mark lines 0, 3
        block.mark_line(0);
        block.mark_line(3);
        // Holes: lines 1-2, and lines 4..LINES_PER_BLOCK
        assert_eq!(block.count_holes(), 2);
    }

    #[test]
    fn test_block_clear_marks() {
        let mut block = Block::new();
        block.mark_line(0);
        block.mark_line(5);
        block.clear_marks();
        assert!(block.is_empty());
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn test_block_mark_out_of_range() {
        let mut block = Block::new();
        block.mark_line(LINES_PER_BLOCK);
    }

    // --- ImmixAllocator tests ---

    #[test]
    fn test_alloc_basic() {
        let mut alloc = ImmixAllocator::new();
        let ptr = alloc.alloc(16, 8);
        assert!(ptr.is_some());
    }

    #[test]
    fn test_alloc_alignment() {
        let mut alloc = ImmixAllocator::new();
        // Misalign first
        let _ = alloc.alloc(1, 1);
        let ptr = alloc.alloc(16, 16).unwrap();
        assert_eq!((ptr as usize) % 16, 0);
    }

    #[test]
    fn test_alloc_multiple_lines() {
        let mut alloc = ImmixAllocator::new();
        // Fill more than one line (128 bytes per line)
        let mut ptrs = Vec::new();
        for _ in 0..20 {
            if let Some(ptr) = alloc.alloc(64, 8) {
                ptrs.push(ptr);
            } else {
                alloc.alloc_new_block();
                ptrs.push(alloc.alloc(64, 8).unwrap());
            }
        }
        assert!(ptrs.len() == 20);
    }

    #[test]
    fn test_alloc_new_block() {
        let mut alloc = ImmixAllocator::new();
        assert_eq!(alloc.active_block_count(), 1);
        alloc.alloc_new_block();
        assert_eq!(alloc.active_block_count(), 2);
    }

    #[test]
    fn test_sweep_empty_blocks() {
        let mut alloc = ImmixAllocator::new();
        // No marks → sweep should move blocks to free list
        alloc.sweep();
        assert_eq!(alloc.free_block_count(), 1);
        assert_eq!(alloc.active_block_count(), 0);
    }

    #[test]
    fn test_sweep_partial_block() {
        let mut alloc = ImmixAllocator::new();
        // Mark some lines
        alloc.blocks[0].mark_line(0);
        alloc.blocks[0].mark_line(1);
        alloc.sweep();
        assert_eq!(alloc.recyclable_block_count(), 1);
        assert_eq!(alloc.free_block_count(), 0);
    }

    #[test]
    fn test_sweep_full_block() {
        let mut alloc = ImmixAllocator::new();
        for i in 0..LINES_PER_BLOCK {
            alloc.blocks[0].mark_line(i);
        }
        alloc.sweep();
        // Full block stays in active set
        assert_eq!(alloc.active_block_count(), 1);
        assert_eq!(alloc.free_block_count(), 0);
        assert_eq!(alloc.recyclable_block_count(), 0);
    }

    #[test]
    fn test_constants() {
        assert_eq!(BLOCK_SIZE, 32 * 1024);
        assert_eq!(LINE_SIZE, 128);
        assert_eq!(LINES_PER_BLOCK, 256);
    }
}
