//! Register allocator with temporary recycling.
//! Assigns registers to cell params, let bindings, and temporaries.
//! Implements a free-list for temporary registers to enable reuse.

use std::collections::HashMap;

/// Maximum number of registers available per cell (u8::MAX)
pub const MAX_REGISTERS: u8 = 255;

/// Register allocation state for a single cell
#[derive(Debug)]
pub struct RegAlloc {
    /// Next register index for new allocations when free list is empty
    next_reg: u8,
    /// Map of variable names to their permanent registers
    bindings: HashMap<String, u8>,
    /// Cell name for error reporting
    cell_name: String,
    /// Pool of free temporary registers available for reuse
    free_temps: Vec<u8>,
    /// Track the maximum register count ever used (for reporting, not recycling)
    max_reg_ever_used: u8,
    /// Count of how many temps have been recycled (for diagnostics)
    temps_recycled: usize,
    /// High-water mark for named bindings - temps allocated below this
    /// might be used for long-term storage and shouldn't be auto-freed
    named_bindings_high_water: u8,
}

impl Default for RegAlloc {
    fn default() -> Self {
        Self::new("<anonymous>")
    }
}

impl RegAlloc {
    pub fn new(cell_name: &str) -> Self {
        Self {
            next_reg: 0,
            bindings: HashMap::new(),
            cell_name: cell_name.to_string(),
            free_temps: Vec::new(),
            max_reg_ever_used: 0,
            temps_recycled: 0,
            named_bindings_high_water: 0,
        }
    }

    /// Allocate a named register for a parameter or let binding.
    /// Named registers are permanent and are never recycled.
    pub fn alloc_named(&mut self, name: &str) -> u8 {
        let reg = self.next_reg;
        if reg == MAX_REGISTERS {
            panic!(
                "Register allocation error in cell '{}': exceeded maximum of {} registers. \
                 This cell is too complex. Consider breaking it into smaller helper cells.",
                self.cell_name, MAX_REGISTERS
            );
        }
        self.bindings.insert(name.to_string(), reg);
        self.next_reg += 1;
        self.max_reg_ever_used = self.max_reg_ever_used.max(reg + 1);
        // Update high-water mark for named bindings
        self.named_bindings_high_water = self.named_bindings_high_water.max(self.next_reg);
        reg
    }

    /// Allocate a temporary register.
    /// First tries to reuse a free temp from the pool, only allocating
    /// a new register if the pool is empty.
    pub fn alloc_temp(&mut self) -> u8 {
        // First, try to reuse a free temp from the pool
        if let Some(reg) = self.free_temps.pop() {
            self.temps_recycled += 1;
            return reg;
        }

        // No free temps available - allocate a new one
        let reg = self.next_reg;
        if reg == MAX_REGISTERS {
            panic!(
                "Register allocation error in cell '{}': exceeded maximum of {} registers \
                 even with recycling ({} temps were reused). \
                 This cell is too complex. Consider breaking it into smaller helper cells.",
                self.cell_name, MAX_REGISTERS, self.temps_recycled
            );
        }
        self.next_reg += 1;
        self.max_reg_ever_used = self.max_reg_ever_used.max(self.next_reg);
        reg
    }

    /// Allocate a contiguous block of temporary registers.
    /// This bypasses the free pool to ensure contiguity, as required by
    /// some VM opcodes (Call, NewList, etc).
    pub fn alloc_block(&mut self, count: u8) -> u8 {
        if count == 0 {
            return self.next_reg;
        }
        let start = self.next_reg;
        if start as u16 + count as u16 > MAX_REGISTERS as u16 {
            panic!(
                "Register allocation error in cell '{}': block of {} registers exceeds limit.",
                self.cell_name, count
            );
        }
        self.next_reg += count;
        self.max_reg_ever_used = self.max_reg_ever_used.max(self.next_reg);
        start
    }

    /// Mark a temporary register as free for reuse.
    /// This should only be called for temporary registers, not named bindings.
    /// Safe to call multiple times on the same register - duplicates are ignored.
    pub fn free_temp(&mut self, reg: u8) {
        // Don't free registers that are bound to names (permanent allocations)
        if self.bindings.values().any(|&r| r == reg) {
            return;
        }

        // Don't add duplicates to the free list
        if self.free_temps.contains(&reg) {
            return;
        }

        self.free_temps.push(reg);
    }

    /// Free multiple temporary registers at once.
    pub fn free_temps(&mut self, regs: &[u8]) {
        for &reg in regs {
            self.free_temp(reg);
        }
    }

    /// Look up a named binding
    pub fn lookup(&self, name: &str) -> Option<u8> {
        self.bindings.get(name).copied()
    }

    /// Get the maximum register count used.
    /// This returns the high-water mark of register usage, which is
    /// what's needed for the VM to allocate sufficient register space.
    pub fn max_regs(&self) -> u8 {
        self.max_reg_ever_used
    }

    /// Get the total number of unique registers allocated (named + temps in use + free temps).
    /// This represents the actual register file size needed.
    pub fn register_file_size(&self) -> u8 {
        // The register file needs to be large enough for:
        // 1. All named bindings
        // 2. All currently-in-use temps (allocated but not freed)
        // 3. Any other registers allocated up to next_reg
        self.next_reg
    }

    /// Get the number of temps that have been recycled
    pub fn recycle_count(&self) -> usize {
        self.temps_recycled
    }

    /// Get the number of temps currently in the free pool
    pub fn free_temp_count(&self) -> usize {
        self.free_temps.len()
    }

    /// Manually bind a name to an existing register (for temporary shadowing)
    pub fn bind(&mut self, name: &str, reg: u8) {
        self.bindings.insert(name.to_string(), reg);
    }

    /// Unbind a name (for temporary shadowing)
    pub fn unbind(&mut self, name: &str) {
        self.bindings.remove(name);
    }

    /// Get the current next_reg value (for manual tracking)
    pub fn current_reg_count(&self) -> u8 {
        self.next_reg
    }

    /// Free all temporary registers that were allocated above the named bindings high-water mark.
    /// This should be called at the end of statements to recycle temps that are no longer needed.
    pub fn free_statement_temps(&mut self) {
        // Any register index >= named_bindings_high_water that's not already in the free list
        // and not a named binding can be freed
        let named_regs: Vec<u8> = self.bindings.values().copied().collect();
        
        for reg in self.named_bindings_high_water..self.next_reg {
            if !named_regs.contains(&reg) && !self.free_temps.contains(&reg) {
                self.free_temps.push(reg);
            }
        }
    }

    /// Get the high-water mark for named bindings
    pub fn named_bindings_count(&self) -> u8 {
        self.named_bindings_high_water
    }

    /// Free specific temporary registers.
    /// This is used to manually free temps that are known to be dead.
    /// Named registers are never freed.
    pub fn free_specific_temps(&mut self, regs: &[u8]) {
        for &reg in regs {
            self.free_temp(reg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regalloc_basic() {
        let mut ra = RegAlloc::new("test");
        let r0 = ra.alloc_named("x");
        let r1 = ra.alloc_named("y");
        let r2 = ra.alloc_temp();
        assert_eq!(r0, 0);
        assert_eq!(r1, 1);
        assert_eq!(r2, 2);
        assert_eq!(ra.lookup("x"), Some(0));
        assert_eq!(ra.lookup("y"), Some(1));
        assert_eq!(ra.max_regs(), 3);
    }

    #[test]
    fn test_temp_recycling() {
        let mut ra = RegAlloc::new("test");
        
        // Allocate some temps
        let t1 = ra.alloc_temp();
        let t2 = ra.alloc_temp();
        let t3 = ra.alloc_temp();
        
        assert_eq!(t1, 0);
        assert_eq!(t2, 1);
        assert_eq!(t3, 2);
        assert_eq!(ra.max_regs(), 3);
        assert_eq!(ra.recycle_count(), 0);
        
        // Free a temp
        ra.free_temp(t2);
        assert_eq!(ra.free_temp_count(), 1);
        
        // Next allocation should reuse t2
        let t4 = ra.alloc_temp();
        assert_eq!(t4, t2); // Reused!
        assert_eq!(ra.recycle_count(), 1);
        assert_eq!(ra.free_temp_count(), 0);
        
        // max_regs should still reflect the high-water mark
        assert_eq!(ra.max_regs(), 3);
    }

    #[test]
    fn test_temp_recycling_multiple() {
        let mut ra = RegAlloc::new("test");
        
        // Allocate and free several temps
        let t1 = ra.alloc_temp();
        let t2 = ra.alloc_temp();
        let t3 = ra.alloc_temp();
        
        ra.free_temp(t1);
        ra.free_temp(t3);
        
        // Next allocations should reuse in LIFO order
        let t4 = ra.alloc_temp();
        let t5 = ra.alloc_temp();
        
        assert_eq!(t4, t3); // Most recently freed first
        assert_eq!(t5, t1);
        assert_eq!(ra.recycle_count(), 2);
    }

    #[test]
    fn test_named_regs_not_recycled() {
        let mut ra = RegAlloc::new("test");
        
        // Allocate a named register
        let named = ra.alloc_named("x");
        
        // Try to free it (should be a no-op)
        ra.free_temp(named);
        
        // Should still be able to look it up
        assert_eq!(ra.lookup("x"), Some(named));
        
        // Next temp allocation should NOT reuse the named register
        let t1 = ra.alloc_temp();
        assert_ne!(t1, named);
    }

    #[test]
    fn test_no_duplicate_free() {
        let mut ra = RegAlloc::new("test");
        
        let t1 = ra.alloc_temp();
        ra.free_temp(t1);
        ra.free_temp(t1); // Duplicate free - should be ignored
        ra.free_temp(t1); // Another duplicate - should be ignored
        
        // Should only have one entry in free list
        assert_eq!(ra.free_temp_count(), 1);
        
        // Should only recycle once
        let t2 = ra.alloc_temp();
        assert_eq!(t2, t1);
        assert_eq!(ra.recycle_count(), 1);
    }

    #[test]
    fn test_free_temps_batch() {
        let mut ra = RegAlloc::new("test");
        
        let t1 = ra.alloc_temp();
        let t2 = ra.alloc_temp();
        let t3 = ra.alloc_temp();
        
        ra.free_temps(&[t1, t2, t3]);
        
        assert_eq!(ra.free_temp_count(), 3);
        assert_eq!(ra.recycle_count(), 0); // None recycled yet
        
        // Allocate again - should reuse
        let _ = ra.alloc_temp();
        let _ = ra.alloc_temp();
        let _ = ra.alloc_temp();
        
        assert_eq!(ra.recycle_count(), 3);
    }

    #[test]
    fn test_manual_temp_freeing() {
        let mut ra = RegAlloc::new("test");
        
        // Allocate some initial registers
        let _ = ra.alloc_named("x");
        
        // Allocate temps
        let t1 = ra.alloc_temp();
        let t2 = ra.alloc_temp();
        
        assert_eq!(ra.max_regs(), 3); // x, t1, t2
        
        // Free the specific temps manually
        ra.free_specific_temps(&[t1, t2]);
        
        // max_regs should still be 3 (high-water mark)
        assert_eq!(ra.max_regs(), 3);
        
        // Free temps should include t1 and t2
        assert_eq!(ra.free_temp_count(), 2);
        
        // Next allocation should recycle
        let t3 = ra.alloc_temp();
        assert!(t3 == t1 || t3 == t2);
        assert_eq!(ra.recycle_count(), 1);
    }

    #[test]
    fn test_recycling_reduces_register_pressure() {
        let mut ra = RegAlloc::new("test");
        
        // Simulate a complex expression that uses many temps
        for i in 0..100 {
            let t1 = ra.alloc_temp();
            let t2 = ra.alloc_temp();
            let t3 = ra.alloc_temp();
            
            // Free them when done with this iteration
            ra.free_temp(t1);
            ra.free_temp(t2);
            ra.free_temp(t3);
            
            // Without recycling, max_regs would be 300+
            // With recycling, it should stay low
            assert!(
                ra.max_regs() < 10,
                "Register pressure should stay low with recycling, got {} at iteration {}",
                ra.max_regs(),
                i
            );
        }
        
        // Should have recycled many times
        assert!(ra.recycle_count() > 200);
    }

    #[test]
    fn test_register_file_size() {
        let mut ra = RegAlloc::new("test");
        
        // Initially empty
        assert_eq!(ra.register_file_size(), 0);
        
        // Allocate some registers
        let _ = ra.alloc_named("x");
        let _ = ra.alloc_named("y");
        let t1 = ra.alloc_temp();
        let t2 = ra.alloc_temp();
        
        // Register file should account for all allocated
        assert_eq!(ra.register_file_size(), 4);
        
        // Free temps - register file size stays the same (high-water mark)
        ra.free_temp(t1);
        ra.free_temp(t2);
        assert_eq!(ra.register_file_size(), 4);
        
        // Recycle - still the same high-water mark
        let _ = ra.alloc_temp();
        assert_eq!(ra.register_file_size(), 4);
    }
}
