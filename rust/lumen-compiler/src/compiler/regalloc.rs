//! Simple linear-scan register allocator.
//! Assigns registers to cell params, let bindings, and temporaries.

use std::collections::HashMap;

/// Register allocation state for a single cell
#[derive(Debug)]
pub struct RegAlloc {
    next_reg: u8,
    bindings: HashMap<String, u8>,
}

impl Default for RegAlloc {
    fn default() -> Self {
        Self::new()
    }
}

impl RegAlloc {
    pub fn new() -> Self {
        Self {
            next_reg: 0,
            bindings: HashMap::new(),
        }
    }

    /// Allocate a named register for a parameter or let binding
    pub fn alloc_named(&mut self, name: &str) -> u8 {
        let reg = self.next_reg;
        self.bindings.insert(name.to_string(), reg);
        self.next_reg += 1;
        reg
    }

    /// Allocate a temporary register
    pub fn alloc_temp(&mut self) -> u8 {
        let reg = self.next_reg;
        self.next_reg += 1;
        reg
    }

    /// Look up a named binding
    pub fn lookup(&self, name: &str) -> Option<u8> {
        self.bindings.get(name).copied()
    }

    /// Get the maximum register count used
    pub fn max_regs(&self) -> u8 {
        self.next_reg
    }

    /// Manually bind a name to an existing register (for temporary shadowing)
    pub fn bind(&mut self, name: &str, reg: u8) {
        self.bindings.insert(name.to_string(), reg);
    }

    /// Unbind a name (for temporary shadowing)
    pub fn unbind(&mut self, name: &str) {
        self.bindings.remove(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regalloc_basic() {
        let mut ra = RegAlloc::new();
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
}
