//! Stack map for OSR (On-Stack Replacement) and deoptimization.
//!
//! This module provides data structures for mapping between:
//! - LIR virtual registers (r0, r1, ..., rN)
//! - Cranelift SSA values and stack slots
//! - Native x86-64 registers and stack positions
//!
//! ## OSR Flow
//!
//! 1. Interpreter/stencil JIT executes hot loop, hits `OsrCheck`
//! 2. `lm_rt_osr_check` determines threshold crossed, triggers Tier 2 compilation
//! 3. Cranelift compiles the loop with OSR entry point at loop header
//! 4. StackMap records where each LIR register lives in the compiled frame
//! 5. Transplant copies `Vec<NbValue>` register file into Cranelift frame
//! 6. Execution jumps to OSR entry point and continues in optimized code

use cranelift_codegen::ir::{StackSlot, Value as CraneliftValue};
use std::collections::HashMap;

/// Type of a live value at a safepoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueType {
    Int,
    Float,
    Bool,
    Ptr,
    Any,
}

/// Location of a live value at a safepoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueLocation {
    /// Value is in a Cranelift SSA value (register-allocated)
    SsaValue(u32),
    /// Value is spilled to a Cranelift stack slot
    StackSlot(u32),
    /// Value is in a specific native register (for OSR entry)
    NativeRegister(u16),
    /// Value is at a specific stack offset from frame pointer
    StackOffset(i32),
}

/// A single live value entry in a stack map.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LiveValue {
    /// The LIR virtual register index
    pub lir_reg: u16,
    /// The type of the value
    pub ty: ValueType,
    /// Where the value is located
    pub location: ValueLocation,
}

/// Stack map for a single safepoint (OSR entry point).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackMap {
    /// Unique identifier for this safepoint
    pub safepoint_id: usize,
    /// The LIR instruction pointer where this map applies
    pub lir_ip: usize,
    /// Live values at this safepoint
    pub live_values: Vec<LiveValue>,
    /// Total number of LIR registers in the frame
    pub total_registers: u16,
    /// Stack frame size in bytes (for OSR entry setup)
    pub frame_size: u32,
}

impl StackMap {
    /// Create a new empty stack map
    pub fn new(safepoint_id: usize, lir_ip: usize) -> Self {
        Self {
            safepoint_id,
            lir_ip,
            live_values: Vec::new(),
            total_registers: 0,
            frame_size: 0,
        }
    }

    /// Add a live value mapping
    pub fn add_live_value(&mut self, lir_reg: u16, ty: ValueType, location: ValueLocation) {
        self.live_values.push(LiveValue {
            lir_reg,
            ty,
            location,
        });
        self.total_registers = self.total_registers.max(lir_reg + 1);
    }

    /// Look up the location of a specific LIR register
    pub fn find_location(&self, lir_reg: u16) -> Option<&LiveValue> {
        self.live_values.iter().find(|lv| lv.lir_reg == lir_reg)
    }
}

/// Registry of stack maps for all OSR entry points in a compiled function.
#[derive(Debug, Clone, Default)]
pub struct StackMapRegistry {
    /// Stack maps indexed by safepoint ID
    maps: HashMap<usize, StackMap>,
    /// Stack maps indexed by LIR instruction pointer
    by_ip: HashMap<usize, usize>,
}

impl StackMapRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            maps: HashMap::new(),
            by_ip: HashMap::new(),
        }
    }

    /// Register a stack map
    pub fn register(&mut self, map: StackMap) {
        let safepoint_id = map.safepoint_id;
        let lir_ip = map.lir_ip;
        self.by_ip.insert(lir_ip, safepoint_id);
        self.maps.insert(safepoint_id, map);
    }

    /// Look up a stack map by safepoint ID
    pub fn get(&self, safepoint_id: usize) -> Option<&StackMap> {
        self.maps.get(&safepoint_id)
    }

    /// Look up a stack map by LIR instruction pointer
    pub fn get_by_ip(&self, lir_ip: usize) -> Option<&StackMap> {
        self.by_ip.get(&lir_ip).and_then(|id| self.maps.get(id))
    }

    /// Get all stack maps
    pub fn all_maps(&self) -> impl Iterator<Item = &StackMap> {
        self.maps.values()
    }
}

/// Builder for constructing stack maps during Cranelift codegen.
#[derive(Debug, Default)]
pub struct StackMapBuilder {
    registry: StackMapRegistry,
    current_map: Option<StackMap>,
    next_safepoint_id: usize,
}

impl StackMapBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            registry: StackMapRegistry::new(),
            current_map: None,
            next_safepoint_id: 0,
        }
    }

    /// Start a new stack map at the given LIR IP
    pub fn begin_map(&mut self, lir_ip: usize) -> usize {
        let safepoint_id = self.next_safepoint_id;
        self.next_safepoint_id += 1;
        self.current_map = Some(StackMap::new(safepoint_id, lir_ip));
        safepoint_id
    }

    /// Add a live value to the current map
    pub fn add_live_value(&mut self, lir_reg: u16, ty: ValueType, location: ValueLocation) {
        if let Some(ref mut map) = self.current_map {
            map.add_live_value(lir_reg, ty, location);
        }
    }

    /// Add a Cranelift SSA value mapping
    pub fn add_ssa_value(&mut self, lir_reg: u16, ty: ValueType, ssa: CraneliftValue) {
        self.add_live_value(lir_reg, ty, ValueLocation::SsaValue(ssa.as_u32()));
    }

    /// Add a stack slot mapping
    pub fn add_stack_slot(&mut self, lir_reg: u16, ty: ValueType, slot: StackSlot) {
        self.add_live_value(lir_reg, ty, ValueLocation::StackSlot(slot.as_u32()));
    }

    /// Set the frame size for the current map
    pub fn set_frame_size(&mut self, size: u32) {
        if let Some(ref mut map) = self.current_map {
            map.frame_size = size;
        }
    }

    /// Finish the current map and register it
    pub fn end_map(&mut self) -> Option<usize> {
        let map = self.current_map.take()?;
        let safepoint_id = map.safepoint_id;
        self.registry.register(map);
        Some(safepoint_id)
    }

    /// Build and return the registry
    pub fn build(self) -> StackMapRegistry {
        self.registry
    }
}

/// OSR entry point descriptor for a compiled function.
#[derive(Debug, Clone)]
pub struct OsrEntryPoint {
    /// The native code address of the OSR entry point
    pub entry_address: *const u8,
    /// Stack map for this entry point
    pub stack_map: StackMap,
    /// Number of LIR registers expected
    pub register_count: u16,
}

impl OsrEntryPoint {
    /// Create a new OSR entry point
    pub fn new(entry_address: *const u8, stack_map: StackMap) -> Self {
        let register_count = stack_map.total_registers;
        Self {
            entry_address,
            stack_map,
            register_count,
        }
    }
}

/// Native register assignments for OSR entry (x86-64 SysV ABI).
///
/// The first few LIR registers are passed in native registers for speed.
/// Remaining registers are passed on the stack.
#[cfg(target_arch = "x86_64")]
pub mod osr_calling_convention {
    use super::ValueLocation;

    /// Argument registers in order (SysV ABI)
    pub const ARG_REGISTERS: &[u16] = &[
        /* rdi */ 7, /* rsi */ 6, /* rdx */ 2, /* rcx */ 1, /* r8 */ 8,
        /* r9 */ 9,
    ];

    /// Maximum arguments passed in registers
    pub const MAX_REG_ARGS: usize = 6;

    /// Callee-saved registers we can use for LIR registers
    pub const CALLEE_SAVED: &[u16] = &[
        /* rbx */ 3, /* r12 */ 12, /* r13 */ 13, /* r14 */ 14,
        /* r15 */ 15,
    ];

    /// Map an LIR register index to its location for OSR entry.
    ///
    /// Returns:
    /// - `ValueLocation::NativeRegister(reg)` for first N registers
    /// - `ValueLocation::StackOffset(offset)` for remaining registers
    pub fn lir_reg_to_location(lir_reg: u16, _total_regs: u16) -> ValueLocation {
        let reg_idx = lir_reg as usize;

        // First few registers go in argument registers
        if reg_idx < ARG_REGISTERS.len() && reg_idx < MAX_REG_ARGS {
            ValueLocation::NativeRegister(ARG_REGISTERS[reg_idx])
        } else {
            // Remaining registers are on stack after return address and saved registers
            // Stack layout: [ret_addr] [rbx] [r12] [r13] [r14] [r15] [reg6] [reg7] ...
            let stack_offset = 8i32 + // return address
                (CALLEE_SAVED.len() * 8) as i32 + // saved callee-saved regs
                ((reg_idx - MAX_REG_ARGS) * 8) as i32; // remaining regs
            ValueLocation::StackOffset(stack_offset)
        }
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub mod osr_calling_convention {
    pub const MAX_REG_ARGS: usize = 0;

    pub fn lir_reg_to_location(lir_reg: u16, _total_regs: u16) -> ValueLocation {
        // On non-x86_64, all registers are on stack
        ValueLocation::StackOffset((lir_reg as i32 + 1) * 8)
    }
}

/// Emit a placeholder stack map for the given safepoint.
pub fn emit_stackmap_at_safepoint(safepoint_id: usize) -> StackMap {
    StackMap::new(safepoint_id, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_map_builder() {
        let mut builder = StackMapBuilder::new();

        let id = builder.begin_map(100);
        builder.add_live_value(0, ValueType::Int, ValueLocation::NativeRegister(7));
        builder.add_live_value(1, ValueType::Int, ValueLocation::NativeRegister(6));
        builder.set_frame_size(128);
        builder.end_map();

        let registry = builder.build();
        let map = registry.get(id).unwrap();

        assert_eq!(map.lir_ip, 100);
        assert_eq!(map.live_values.len(), 2);
        assert_eq!(map.frame_size, 128);
    }

    #[test]
    fn test_find_location() {
        let mut map = StackMap::new(0, 0);
        map.add_live_value(0, ValueType::Int, ValueLocation::NativeRegister(7));
        map.add_live_value(2, ValueType::Ptr, ValueLocation::StackOffset(16));

        let loc0 = map.find_location(0).unwrap();
        assert_eq!(loc0.ty, ValueType::Int);

        let loc2 = map.find_location(2).unwrap();
        assert_eq!(loc2.location, ValueLocation::StackOffset(16));

        assert!(map.find_location(1).is_none());
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn test_osr_calling_convention() {
        use osr_calling_convention::*;

        // First register goes in rdi
        let loc0 = lir_reg_to_location(0, 10);
        assert_eq!(loc0, ValueLocation::NativeRegister(7)); // rdi

        // Second register goes in rsi
        let loc1 = lir_reg_to_location(1, 10);
        assert_eq!(loc1, ValueLocation::NativeRegister(6)); // rsi

        // Seventh register goes on stack
        let loc6 = lir_reg_to_location(6, 10);
        match loc6 {
            ValueLocation::StackOffset(offset) => {
                assert!(offset > 0);
            }
            _ => panic!("Expected stack offset for register 6"),
        }
    }
}
