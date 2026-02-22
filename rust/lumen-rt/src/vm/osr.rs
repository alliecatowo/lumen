//! On-Stack Replacement (OSR) support for tier transitions.
//!
//! OSR enables transitioning from interpreted/stencil code to optimized Cranelift
//! code mid-execution, particularly useful for hot loops.
//!
//! ## Flow
//!
//! 1. During execution, `OsrCheck` stencil calls `lm_rt_osr_check`
//! 2. The OSR runtime checks if the loop is hot (crossed threshold)
//! 3. If hot and not yet compiled, triggers Cranelift compilation
//! 4. If compiled code exists, returns a function pointer to jump to
//! 5. The stencil code jumps to the optimized code with captured state

use crate::vm::VM;
use lumen_core::lir::LirModule;
use lumen_core::nb_value::NbValue;
use lumen_core::values::Value;
use lumen_core::vm_context::VmContext;
use thiserror::Error;

#[cfg(feature = "jit")]
use lumen_codegen::jit::JitEngine;
#[cfg(feature = "jit")]
use lumen_core::lir::{Instruction, LirCell, LirOsrPoint, LirParam, OpCode};
#[cfg(feature = "jit")]
use std::collections::HashMap;

pub mod osr_check {
    use super::*;

    /// Threshold for triggering OSR tier-up
    pub const OSR_HOT_THRESHOLD: u64 = 50;

    /// Per-cell OSR state tracked during execution
    #[derive(Debug, Clone, Copy)]
    pub struct OsrState {
        pub cell_idx: usize,
        pub loop_count: u64,
        pub compiled: bool,
        pub compile_attempted: bool,
        pub compiled_fn: Option<*const ()>,
    }

    impl OsrState {
        pub fn new(cell_idx: usize) -> Self {
            Self {
                cell_idx,
                loop_count: 0,
                compiled: false,
                compile_attempted: false,
                compiled_fn: None,
            }
        }

        /// Check if we should trigger tier-up
        pub fn should_tier_up(&self) -> bool {
            self.loop_count >= OSR_HOT_THRESHOLD && !self.compiled && !self.compile_attempted
        }

        /// Record an OSR check and return whether we should tier up
        pub fn record_check(&mut self) -> bool {
            self.loop_count += 1;
            self.should_tier_up()
        }
    }

    /// Runtime state for OSR - indexed by cell_idx
    pub struct OsrRuntime {
        states: Vec<OsrState>,
        transition_count: u64,
        entry_transition_count: u64,
        restart_fallback_transition_count: u64,
        #[cfg(feature = "jit")]
        jit_engine: Option<JitEngine>,
        /// Reference to the module for compilation
        #[cfg(feature = "jit")]
        module: Option<LirModule>,
        /// Mapping from (cell_idx, osr_ip) to compiled synthetic entry cells.
        #[cfg(feature = "jit")]
        osr_entries: HashMap<(usize, usize), OsrEntry>,
        /// Whether synthetic OSR entry cells have been compiled into the engine.
        #[cfg(feature = "jit")]
        osr_entries_compiled: bool,
    }

    impl OsrRuntime {
        pub fn new(num_cells: usize) -> Self {
            let mut states = Vec::with_capacity(num_cells);
            for idx in 0..num_cells {
                states.push(OsrState::new(idx));
            }
            Self {
                states,
                transition_count: 0,
                entry_transition_count: 0,
                restart_fallback_transition_count: 0,
                #[cfg(feature = "jit")]
                jit_engine: None,
                #[cfg(feature = "jit")]
                module: None,
                #[cfg(feature = "jit")]
                osr_entries: HashMap::new(),
                #[cfg(feature = "jit")]
                osr_entries_compiled: false,
            }
        }

        #[cfg(feature = "jit")]
        pub fn init_jit(&mut self, module: LirModule) {
            // If the JIT engine is already initialized, skip reinitialization
            // to preserve pre-compiled cells from load().
            if self.jit_engine.is_some() {
                return;
            }
            use lumen_codegen::jit::CodegenSettings;
            let settings = CodegenSettings::default();
            self.jit_engine = Some(JitEngine::new(settings, 0));
            self.module = Some(module);
        }

        pub fn get_state(&mut self, cell_idx: usize) -> &mut OsrState {
            if cell_idx >= self.states.len() {
                // Expand the states vector if needed
                let current_len = self.states.len();
                self.states.resize(cell_idx + 1, OsrState::new(cell_idx));
                // Fix up indices for newly created states
                for i in current_len..self.states.len() {
                    self.states[i] = OsrState::new(i);
                }
            }
            &mut self.states[cell_idx]
        }

        pub fn is_compiled(&self, cell_idx: usize) -> bool {
            self.states
                .get(cell_idx)
                .map(|state| state.compiled)
                .unwrap_or(false)
        }

        pub fn record_transition(&mut self) {
            self.transition_count += 1;
        }

        pub fn record_entry_transition(&mut self) {
            self.transition_count += 1;
            self.entry_transition_count += 1;
        }

        pub fn record_restart_fallback_transition(&mut self) {
            self.transition_count += 1;
            self.restart_fallback_transition_count += 1;
        }

        pub fn transition_count(&self) -> u64 {
            self.transition_count
        }

        pub fn entry_transition_count(&self) -> u64 {
            self.entry_transition_count
        }

        pub fn restart_fallback_transition_count(&self) -> u64 {
            self.restart_fallback_transition_count
        }

        /// Record an OSR check for a cell, returns true if tier-up needed
        pub fn record_and_check(&mut self, cell_idx: usize) -> bool {
            let state = self.get_state(cell_idx);
            state.record_check()
        }

        /// Attempt to compile a cell for OSR
        #[cfg(feature = "jit")]
        pub fn try_compile(&mut self, cell_idx: usize) -> bool {
            // Mark attempted before anything else to prevent infinite retry
            // if compilation panics and catch_unwind swallows it.
            let state = self.get_state(cell_idx);
            state.compile_attempted = true;

            let module = match &self.module {
                Some(m) => m,
                None => return false,
            };

            if cell_idx >= module.cells.len() {
                return false;
            }

            {
                use lumen_core::lir::OpCode;
                let cell = &module.cells[cell_idx];

                // Skip cells with NewRecord/NewSet which have known codegen
                // bugs (SIGSEGV in generated code). NewMap and string
                // arithmetic are fully supported.
                if cell
                    .instructions
                    .iter()
                    .any(|i| matches!(i.op, OpCode::NewRecord | OpCode::NewSet))
                {
                    return false;
                }
            }

            let cell_name = module.cells[cell_idx].name.clone();

            let mut compiled_entries: Option<HashMap<(usize, usize), OsrEntry>> = None;
            let mut module_for_compile: Option<LirModule> = None;
            if !self.osr_entries_compiled {
                let (entry_module, entries) = build_module_with_osr_entries(module);
                module_for_compile = Some(entry_module);
                compiled_entries = Some(entries);
            }

            if let Some(ref mut engine) = self.jit_engine {
                let compile_result = if let Some(ref entry_module) = module_for_compile {
                    engine.compile_module(entry_module)
                } else if engine.is_compiled(&cell_name) {
                    Ok(())
                } else {
                    engine.compile_hot(&cell_name, module)
                };

                match compile_result {
                    Ok(()) => {
                        if let Some(entries) = compiled_entries {
                            self.osr_entries = entries
                                .into_iter()
                                .filter(|(_, entry)| engine.is_compiled(&entry.cell_name))
                                .collect();
                            self.osr_entries_compiled = true;
                        }

                        if engine.is_compiled(&cell_name) {
                            // Get the function pointer (needs immutable borrow)
                            if let Some(fn_ptr) = engine.get_compiled_fn(&cell_name) {
                                let state = self.get_state(cell_idx);
                                state.compiled = true;
                                state.compiled_fn = Some(fn_ptr);
                                return true;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("OSR compile failed for {}: {}", cell_name, e);
                    }
                }
            }
            false
        }

        #[cfg(not(feature = "jit"))]
        pub fn try_compile(&mut self, _cell_idx: usize) -> bool {
            false
        }

        /// Get the compiled function pointer for a cell
        pub fn get_compiled_fn(&self, cell_idx: usize) -> Option<*const ()> {
            if cell_idx < self.states.len() {
                self.states[cell_idx].compiled_fn
            } else {
                None
            }
        }

        #[cfg(feature = "jit")]
        pub fn return_type(&self, cell_name: &str) -> Option<lumen_codegen::jit::JitVarType> {
            self.jit_engine
                .as_ref()
                .and_then(|engine| engine.return_type(cell_name))
        }

        /// Get a mutable reference to the JIT engine (for executing compiled code).
        #[cfg(feature = "jit")]
        pub fn jit_engine_mut(&mut self) -> Option<&mut JitEngine> {
            self.jit_engine.as_mut()
        }

        /// Lookup an OSR entry cell for a specific `(cell_idx, ip)` safepoint.
        #[cfg(feature = "jit")]
        pub fn osr_entry(&self, cell_idx: usize, ip: usize) -> Option<&OsrEntry> {
            self.osr_entries.get(&(cell_idx, ip))
        }
    }

    /// OSR check — interpreter fast path (no catch_unwind overhead).
    ///
    /// Called directly from the interpreter dispatch loop; safe to call
    /// since we are still in Rust code and panics will propagate normally.
    /// Returns `null` if no compiled code is available, or the compiled
    /// function pointer if tier-up has occurred.
    #[inline]
    pub fn osr_check_interp(vm: &mut VM, cell_idx: usize) -> *const () {
        let osr_runtime = &mut vm.osr_runtime;

        // Fast path: already compiled — return immediately without incrementing.
        if let Some(ptr) = osr_runtime.get_compiled_fn(cell_idx) {
            return ptr;
        }

        // Record the check and maybe trigger compilation.
        if osr_runtime.record_and_check(cell_idx) {
            if osr_runtime.try_compile(cell_idx) {
                return osr_runtime
                    .get_compiled_fn(cell_idx)
                    .unwrap_or(std::ptr::null());
            }
        }

        std::ptr::null()
    }

    /// OSR check function called from stencil JIT (extern "C").
    ///
    /// The stencil calling convention passes `r15 = *mut VmContext` as the
    /// first argument to ALL runtime callbacks (matching `lm_rt_call`,
    /// `lm_rt_return`, etc.). We extract the VM pointer from
    /// `(*ctx).stack_pool` exactly as `stencil_runtime.rs` does via
    /// `vm_from_ctx`.
    ///
    /// Wraps `osr_check_interp` in catch_unwind to prevent panics crossing
    /// the FFI boundary. The interpreter should prefer `osr_check_interp`.
    #[no_mangle]
    pub unsafe extern "C" fn lm_rt_osr_check(
        ctx: *mut VmContext,
        cell_idx: usize,
        _current_ip: usize,
    ) -> *const () {
        debug_assert!(!ctx.is_null(), "lm_rt_osr_check: null VmContext");
        let vm: &mut VM = {
            let ptr = (*ctx).stack_pool as *mut VM;
            debug_assert!(!ptr.is_null(), "lm_rt_osr_check: null VM pointer");
            &mut *ptr
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            osr_check_interp(vm, cell_idx)
        }));
        result.unwrap_or(std::ptr::null())
    }
}

/// Errors returned by OSR transition attempts.
#[derive(Debug, Error)]
pub enum OsrError {
    #[error("OSR point not found for cell '{cell}' at ip {ip}")]
    OsrPointNotFound { cell: String, ip: usize },
    #[error("cell not found for OSR: {0}")]
    CellNotFound(String),
    #[error("OSR transition is not available: {0}")]
    Unavailable(String),
}

/// Descriptor for an OSR entry point in compiled code.
#[derive(Debug, Clone)]
pub struct OsrEntry {
    pub cell_name: String,
    pub ip: usize,
    pub arg_count: usize,
}

#[cfg(feature = "jit")]
fn build_module_with_osr_entries(
    module: &LirModule,
) -> (LirModule, HashMap<(usize, usize), OsrEntry>) {
    let mut combined = module.clone();
    let mut entries = HashMap::new();
    let mut synthetic_cells = Vec::new();

    for (cell_idx, cell) in module.cells.iter().enumerate() {
        for point in &cell.osr_points {
            if let Some((entry, synthetic)) = build_osr_entry_cell(cell_idx, cell, point) {
                entries.insert((cell_idx, point.ip), entry);
                synthetic_cells.push(synthetic);
            }
        }
    }

    combined.cells.extend(synthetic_cells);
    (combined, entries)
}

#[cfg(feature = "jit")]
fn build_osr_entry_cell(
    cell_idx: usize,
    cell: &LirCell,
    point: &LirOsrPoint,
) -> Option<(OsrEntry, LirCell)> {
    let entry_ip = point.ip;
    if entry_ip >= cell.instructions.len() {
        return None;
    }
    let entry_starts_with_osr = cell.instructions[entry_ip].op == OpCode::OsrCheck;
    let entry_prev_is_osr = entry_ip > 0 && cell.instructions[entry_ip - 1].op == OpCode::OsrCheck;

    let arg_count = point
        .live_registers
        .iter()
        .copied()
        .max()
        .map(|max_reg| max_reg as usize + 1)
        .unwrap_or(0);

    let mut instructions = Vec::with_capacity(cell.instructions.len() - entry_ip);
    for (local_ip, instr) in cell.instructions[entry_ip..].iter().enumerate() {
        let orig_ip = entry_ip + local_ip;
        let Some(remapped) = remap_osr_entry_instruction(
            *instr,
            orig_ip,
            local_ip,
            entry_ip,
            cell.instructions.len(),
            entry_starts_with_osr,
            entry_prev_is_osr,
        ) else {
            return None;
        };
        instructions.push(remapped);
    }

    let entry_name = format!("__osr_entry_{}_{}", cell_idx, entry_ip);
    let params = (0..arg_count)
        .map(|reg| LirParam {
            name: format!("r{reg}"),
            ty: cell
                .params
                .get(reg)
                .map(|p| p.ty.clone())
                .unwrap_or_else(|| "Int".to_string()),
            register: reg as u16,
            variadic: false,
        })
        .collect();

    let synthetic = LirCell {
        name: entry_name.clone(),
        params,
        returns: cell.returns.clone(),
        registers: cell.registers.max(arg_count as u16),
        constants: cell.constants.clone(),
        instructions,
        effect_handler_metas: cell.effect_handler_metas.clone(),
        osr_points: Vec::new(),
    };
    let entry = OsrEntry {
        cell_name: entry_name,
        ip: entry_ip,
        arg_count,
    };

    Some((entry, synthetic))
}

#[cfg(feature = "jit")]
fn remap_osr_entry_instruction(
    instr: Instruction,
    orig_ip: usize,
    local_ip: usize,
    entry_start_ip: usize,
    total_instructions: usize,
    entry_starts_with_osr: bool,
    entry_prev_is_osr: bool,
) -> Option<Instruction> {
    let source_next = orig_ip as i64 + 1;
    let new_next = local_ip as i64 + 1;
    let entry_start = entry_start_ip as i64;
    let end = total_instructions as i64;
    let in_entry_range = |target: i64| target >= entry_start && target < end;

    match instr.op {
        OpCode::OsrCheck => Some(Instruction::abc(OpCode::Nop, 0, 0, 0)),
        OpCode::Jmp | OpCode::Break | OpCode::Continue => {
            let mut target = source_next + instr.sax_val();
            if entry_starts_with_osr && target == entry_start {
                target += 1;
            } else if entry_prev_is_osr && target == entry_start - 1 {
                target += 1;
            }
            if !in_entry_range(target) {
                return None;
            }
            let new_target = target - entry_start;
            Some(Instruction::sax(instr.op, new_target - new_next))
        }
        OpCode::Loop => {
            let mut target = source_next + instr.sbx() as i64;
            if entry_starts_with_osr && target == entry_start {
                target += 1;
            } else if entry_prev_is_osr && target == entry_start - 1 {
                target += 1;
            }
            if !in_entry_range(target) {
                return None;
            }
            let new_target = target - entry_start;
            let sb = new_target - new_next;
            if sb < i32::MIN as i64 || sb > i32::MAX as i64 {
                return None;
            }
            Some(Instruction::abx(instr.op, instr.a, sb as i32 as u32))
        }
        OpCode::ForPrep => {
            let mut target = source_next + instr.bx() as i64;
            if entry_starts_with_osr && target == entry_start {
                target += 1;
            } else if entry_prev_is_osr && target == entry_start - 1 {
                target += 1;
            }
            if !in_entry_range(target) {
                return None;
            }
            let new_target = target - entry_start;
            let bx = new_target - new_next;
            if bx < 0 || bx > u32::MAX as i64 {
                return None;
            }
            Some(Instruction::abx(instr.op, instr.a, bx as u32))
        }
        OpCode::ForLoop => {
            let mut target = source_next - instr.bx() as i64;
            if entry_starts_with_osr && target == entry_start {
                target += 1;
            } else if entry_prev_is_osr && target == entry_start - 1 {
                target += 1;
            }
            if !in_entry_range(target) {
                return None;
            }
            let new_target = target - entry_start;
            let bx = new_next - new_target;
            if bx < 0 || bx > u32::MAX as i64 {
                return None;
            }
            Some(Instruction::abx(instr.op, instr.a, bx as u32))
        }
        OpCode::HandlePush => {
            let mut target = orig_ip as i64 + instr.bx() as i64;
            if entry_starts_with_osr && target == entry_start {
                target += 1;
            } else if entry_prev_is_osr && target == entry_start - 1 {
                target += 1;
            }
            if !in_entry_range(target) {
                return None;
            }
            let new_target = target - entry_start;
            let bx = new_target - local_ip as i64;
            if bx < 0 || bx > u32::MAX as i64 {
                return None;
            }
            Some(Instruction::abx(instr.op, instr.a, bx as u32))
        }
        _ => Some(instr),
    }
}

#[cfg(feature = "jit")]
fn execute_compiled_cell(vm: &mut VM, cell_name: &str, args: &[i64]) -> Result<Value, OsrError> {
    use lumen_codegen::jit::JitVarType;
    use lumen_core::values::StringRef;

    let engine = vm
        .osr_runtime
        .jit_engine_mut()
        .ok_or_else(|| OsrError::Unavailable("no JIT engine in OSR runtime".into()))?;

    let raw_nb = engine
        .execute_jit(&vm.vm_ctx.inner, cell_name, args)
        .map_err(|e| OsrError::Unavailable(format!("JIT execution failed: {e}")))?;

    // execute_jit returns NaN-boxed values. Unbox based on the return type
    // before converting to Value.
    let ret_ty = engine.return_type(cell_name).unwrap_or(JitVarType::Int);
    let raw = lumen_codegen::jit::nan_unbox_typed_pub(raw_nb, ret_ty);

    let result = match ret_ty {
        JitVarType::Str => {
            let s = unsafe { lumen_codegen::jit::jit_take_string(raw) };
            Value::String(StringRef::Owned(s))
        }
        JitVarType::Float => Value::Float(f64::from_bits(raw as u64)),
        JitVarType::Bool => Value::Bool(raw != 0),
        JitVarType::Int => Value::Int(raw),
        JitVarType::Ptr => {
            if raw == 0 || raw == 0x7FF8_0000_0000_0000_u64 as i64 {
                Value::Null
            } else {
                let boxed = unsafe { Box::from_raw(raw as *mut Value) };
                *boxed
            }
        }
        JitVarType::RawInt => Value::Int(raw),
    };

    Ok(result)
}

/// Capture a snapshot of the interpreter's register state as NbValue slice.
/// Properly increments Arc refcounts for heap-allocated NbValues so the
/// returned Vec owns its references independently of the live register file.
pub fn capture_register_state(vm: &VM, base: usize, register_count: usize) -> Vec<NbValue> {
    let snapshot = vm.registers[base..base + register_count].to_vec();
    for nb in &snapshot {
        nb.inc_ref();
    }
    snapshot
}

/// Copy interpreter register state to compiled code's stack frame.
///
/// This implements the "transplant" logic: taking the interpreter's register
/// file (Vec<NbValue>) and copying it into the format expected by the
/// Cranelift-compiled code at the OSR entry point.
///
/// The layout follows the OSR calling convention from stackmap.rs:
/// - Registers 0-5: passed in rdi, rsi, rdx, rcx, r8, r9
/// - Registers 6+: passed on stack after return address and callee-saved regs
pub fn transplant_registers(regs: &[NbValue], frame_pointer: *mut u8) {
    use lumen_codegen::stackmap::osr_calling_convention::{
        ARG_REGISTERS, CALLEE_SAVED, MAX_REG_ARGS,
    };

    // Copy each LIR register to its designated location
    for (i, nbval) in regs.iter().enumerate() {
        let i = i as u16;

        if (i as usize) < MAX_REG_ARGS && (i as usize) < ARG_REGISTERS.len() {
            // Move to argument register
            // Note: This requires inline assembly or runtime helper
            // For now, we'll use the stack-based approach
        }

        // Calculate stack offset:
        // Stack layout: [ret_addr] [rbx] [r12] [r13] [r14] [r15] [reg0] [reg1] ...
        let stack_offset = 8 + // return address
            (CALLEE_SAVED.len() * 8) + // saved callee-saved regs
            (i as usize * 8); // register location

        // Write the NbValue to the stack frame
        let slot_ptr = unsafe { frame_pointer.add(stack_offset) };
        unsafe {
            std::ptr::write(slot_ptr as *mut NbValue, *nbval);
        }
    }
}

/// Attempt to perform an OSR transition to compiled code.
///
/// Prefers one-way transfer into a synthetic OSR entry cell keyed by
/// `(cell_idx, osr_ip)`. If no valid entry is available, falls back to
/// re-executing the compiled full cell from its declared parameters.
pub fn perform_osr_transition(
    vm: &mut VM,
    cell: &lumen_core::lir::LirCell,
    ip: usize,
) -> Result<Value, OsrError> {
    #[cfg(feature = "jit")]
    {
        use lumen_codegen::jit::EXECUTE_JIT_MAX_ARITY;

        let (cell_idx, base) = vm
            .frames
            .last()
            .map(|f| (f.cell_idx, f.base_register))
            .ok_or_else(|| {
                OsrError::Unavailable("OSR transition requested with no frame".into())
            })?;

        // Prefer one-way entry transfer for the exact safepoint. If the
        // safepoint instruction is OsrCheck and metadata uses the first
        // post-check instruction, allow `(ip + 1)` as a compatible entry.
        let entry = vm.osr_runtime.osr_entry(cell_idx, ip).cloned().or_else(|| {
            if ip < cell.instructions.len()
                && cell.instructions[ip].op == OpCode::OsrCheck
                && ip + 1 < cell.instructions.len()
            {
                vm.osr_runtime.osr_entry(cell_idx, ip + 1).cloned()
            } else {
                None
            }
        });
        if let Some(entry) = entry {
            let entry_is_valid = entry.arg_count <= EXECUTE_JIT_MAX_ARITY
                && base.saturating_add(entry.arg_count) <= vm.registers.len()
                && vm.osr_runtime.return_type(&entry.cell_name).is_some();
            if entry_is_valid {
                let args: Vec<i64> = (0..entry.arg_count)
                    .map(|reg| vm.registers[base + reg].0 as i64)
                    .collect();
                let result = execute_compiled_cell(vm, &entry.cell_name, &args)?;
                vm.osr_runtime.record_entry_transition();
                return Ok(result);
            }
        }

        // Safe fallback: restart the full cell from its declared parameters.
        let args: Vec<i64> = (0..cell.params.len())
            .map(|i| vm.registers[base + i].0 as i64)
            .collect();
        let result = execute_compiled_cell(vm, &cell.name, &args)?;
        vm.osr_runtime.record_restart_fallback_transition();
        Ok(result)
    }

    #[cfg(not(feature = "jit"))]
    {
        let _ = (cell, ip);
        Err(OsrError::Unavailable(
            "OSR transition requires jit feature".to_string(),
        ))
    }
}

/// Reconstruct interpreter state from an OSR snapshot.
pub fn reconstruct_interpreter_state(_vm: &mut VM) -> Result<(), OsrError> {
    Err(OsrError::Unavailable(
        "OSR reconstruction not yet implemented".to_string(),
    ))
}
