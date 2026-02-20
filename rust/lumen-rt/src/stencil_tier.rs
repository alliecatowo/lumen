//! Tier 1 stencil execution support.

use std::collections::HashSet;

use lumen_core::lir::{LirModule, OpCode};
use lumen_core::nb_value::NbValue;
use lumen_core::vm_context::VmContext;

#[cfg(feature = "jit")]
use crate::vm::{VmError, VM};
#[cfg(not(feature = "jit"))]
use crate::vm::{VmError, VM};

#[derive(Debug, Clone)]
pub struct StencilTierConfig {
    pub hot_threshold: u64,
    pub enabled: bool,
}

impl Default for StencilTierConfig {
    fn default() -> Self {
        Self {
            hot_threshold: 0,
            enabled: true,
        }
    }
}

impl StencilTierConfig {
    pub fn from_threshold(hot_threshold: u64) -> Self {
        Self {
            hot_threshold,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct StencilTierStats {
    pub cells_compiled: u64,
    pub stencil_executions: u64,
    pub compile_failures: u64,
    pub total_calls_tracked: u64,
}

#[cfg(feature = "jit")]
pub struct StencilTier {
    call_counts: Vec<u64>,
    compiled: HashSet<usize>,
    config: StencilTierConfig,
    stitcher: Option<lumen_codegen::stitcher::Stitcher>,
    stats: StencilTierStats,
}

#[cfg(not(feature = "jit"))]
pub struct StencilTier {
    stats: StencilTierStats,
}

/// Pre-growth headroom (in registers) added before calling into stitched code.
///
/// Stencil code pins `r14 = &registers[base]`. If `registers` reallocates
/// during a runtime callback (e.g. `dispatch_call_from_stencil`), `r14`
/// becomes a dangling pointer. We pre-grow the register vector by this amount
/// before entering stitched code to prevent any reallocation during execution.
#[cfg(feature = "jit")]
// Keep this large enough that nested stencil->stencil call chains do not need
// to resize `vm.registers` inside runtime helpers. Resizing during a helper can
// move the backing allocation while stitched code still holds r14 pinned to the
// old base address.
const STENCIL_HEADROOM: usize = 64 * 1024;
/// Extra capacity slack to avoid backing-buffer moves while nested stencil
/// callbacks temporarily grow `registers` length.
const STENCIL_CAPACITY_SLACK: usize = 1 << 20;

#[cfg(feature = "jit")]
impl StencilTier {
    pub fn new(config: StencilTierConfig) -> Self {
        let lib = lumen_codegen::stencils::build_stencil_library();
        let stitcher = lumen_codegen::stitcher::Stitcher::new(lib).ok();
        Self {
            call_counts: Vec::new(),
            compiled: HashSet::new(),
            config,
            stitcher,
            stats: StencilTierStats::default(),
        }
    }

    /// Zero-cost disabled shell — does NOT build the stencil library.
    /// Used as a temporary placeholder during borrow-split execution.
    pub fn disabled() -> Self {
        Self {
            call_counts: Vec::new(),
            compiled: HashSet::new(),
            config: StencilTierConfig {
                enabled: false,
                hot_threshold: 0,
            },
            stitcher: None,
            stats: StencilTierStats::default(),
        }
    }

    pub fn init_for_module(&mut self, num_cells: usize) {
        self.call_counts.resize(num_cells, 0);
        self.compiled.clear();
        self.stats = StencilTierStats::default();
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn is_compiled(&self, cell_idx: usize) -> bool {
        self.compiled.contains(&cell_idx)
    }

    pub fn record_call(&mut self, cell_idx: usize) -> bool {
        if !self.config.enabled {
            return false;
        }
        if cell_idx >= self.call_counts.len() {
            return false;
        }
        if self.compiled.contains(&cell_idx) {
            return false;
        }
        self.call_counts[cell_idx] += 1;
        self.stats.total_calls_tracked += 1;
        let call_count = self.call_counts[cell_idx];
        // Always try stencil compile on first call. Keep the configured
        // threshold trigger as a retry point for later calls.
        call_count == 1 || call_count == self.config.hot_threshold.saturating_add(1)
    }

    pub fn try_compile(&mut self, cell_idx: usize, module: &LirModule) -> bool {
        let cell = match module.cells.get(cell_idx) {
            Some(cell) => cell,
            None => {
                self.stats.compile_failures += 1;
                return false;
            }
        };

        // Crash-mitigation: skip Tier1 stencil compilation for cells that
        // contain dynamic call opcodes. These cells fall back to interpreter
        // or Tier2 JIT paths, which are safer for nested call behavior.
        if cell
            .instructions
            .iter()
            .any(|instr| matches!(instr.op, OpCode::Call | OpCode::TailCall))
        {
            return false;
        }

        // Semantic guard: multi-step cells are only stencil-safe for a narrow
        // subset of opcode mixes. Keep Tier1 enabled for pure arithmetic chains,
        // but block the known unsafe parameter-fed arithmetic pattern.
        let effective_len = cell
            .instructions
            .iter()
            .position(|instr| matches!(instr.op, OpCode::Return | OpCode::Halt))
            .map(|idx| idx + 1)
            .unwrap_or(cell.instructions.len());

        let multi_step = effective_len > 2;
        if multi_step {
            let body = &cell.instructions[..effective_len];
            if body.iter().any(|instr| !is_pure_arith(instr.op)) {
                return false;
            }
            if has_known_unsafe_multi_step_arith_pattern(cell, body) {
                return false;
            }
        }

        let code_len = {
            let stitcher = match self.stitcher.as_mut() {
                Some(stitcher) => stitcher,
                None => {
                    self.stats.compile_failures += 1;
                    return false;
                }
            };

            match stitcher.compile(cell, module, cell_idx) {
                Ok(code) if code.code_len > 0 => code.code_len,
                Ok(_) => {
                    // Empty stencils (placeholder platform or unsupported opcodes).
                    // Do NOT mark as compiled — code_ptr is NULL/invalid.
                    self.stats.compile_failures += 1;
                    return false;
                }
                Err(_) => {
                    self.stats.compile_failures += 1;
                    return false;
                }
            }
        };

        // Post-patch RuntimeFuncAddr holes with real function addresses.
        // The stitcher emits 0 for RuntimeFuncAddr holes; we now fill them in.
        {
            let stitcher = match self.stitcher.as_mut() {
                Some(s) => s,
                None => {
                    self.stats.compile_failures += 1;
                    return false;
                }
            };

            let (code_ptr, buf_base) = {
                let code = stitcher.get_compiled(cell_idx).unwrap();
                (code.code_ptr, stitcher.code_buffer_ptr())
            };

            // Byte offset of this cell's code from the start of the code buffer.
            let cell_start_buf_offset = code_ptr as usize - buf_base as usize;

            let library = stitcher.library().clone();

            let mut byte_offset = 0usize; // offset within this cell's code
            for instr in &cell.instructions {
                let op_byte = instr.op as u8;
                if let Some(stencil) = library.get(op_byte) {
                    // Check all holes in this stencil for RuntimeFuncAddr.
                    for hole in &stencil.holes {
                        if hole.hole_type
                            == lumen_codegen::stencil_format::HoleType::RuntimeFuncAddr
                        {
                            let func_name = opcode_to_runtime_func(instr.op);
                            if let Some(addr) = resolve_runtime_helper(func_name) {
                                let abs_offset =
                                    cell_start_buf_offset + byte_offset + hole.offset as usize;
                                stitcher.patch_u64_at(abs_offset, addr);
                            }
                        }
                    }
                    byte_offset += stencil.code.len();
                }
                if matches!(instr.op, OpCode::Return | OpCode::Halt) {
                    break;
                }
            }

            let _ = code_len; // suppress unused warning
        }

        self.compiled.insert(cell_idx);
        self.stats.cells_compiled += 1;
        true
    }

    pub fn execute(&mut self, vm: &mut VM, cell_idx: usize, base: usize) -> Result<(), VmError> {
        let stitcher = match self.stitcher.as_ref() {
            Some(stitcher) => stitcher,
            None => return Err(VmError::Runtime("stencil tier unavailable".into())),
        };
        let code = stitcher
            .get_compiled(cell_idx)
            .ok_or_else(|| VmError::Runtime("stencil not compiled".into()))?;
        if code.code_ptr.is_null() || code.code_len == 0 {
            return Err(VmError::Runtime(
                "stencil has empty code (placeholder platform)".into(),
            ));
        }

        // Pre-grow registers to prevent reallocation during runtime callbacks.
        // Stencil code pins r14 = &registers[base]. A reallocation would make
        // r14 dangle, causing memory corruption.
        let needed = base + STENCIL_HEADROOM;
        let reserve_target = needed.saturating_add(STENCIL_CAPACITY_SLACK);
        if reserve_target > vm.registers.capacity() {
            vm.registers
                .reserve(reserve_target - vm.registers.capacity());
        }
        if needed > vm.registers.len() {
            vm.registers.resize(needed, NbValue::new_null());
        }

        // Record the stencil frame base so that runtime helpers (return, intrinsic,
        // call) can compute absolute register addresses. Save/restore is required
        // for nested stencil calls.
        let prev_stencil_base = vm.stencil_base;
        vm.stencil_base = base;

        let run_result = (|| -> Result<(), VmError> {
            let regs_ptr = unsafe { vm.registers.as_mut_ptr().add(base) };
            let ctx_ptr = vm.vm_ctx.as_ptr();
            unsafe {
                (*ctx_ptr).stack_pool = vm as *mut VM as *mut ();
            }
            unsafe {
                call_stitched(code.code_ptr, regs_ptr, ctx_ptr);
            }
            Ok(())
        })();
        vm.stencil_base = prev_stencil_base;
        run_result?;
        self.stats.stencil_executions += 1;
        Ok(())
    }

pub fn stats(&self) -> StencilTierStats {
        self.stats.clone()
    }
}

#[cfg(not(feature = "jit"))]
impl StencilTier {
    pub fn new(_config: StencilTierConfig) -> Self {
        Self {
            stats: StencilTierStats::default(),
        }
    }

    pub fn disabled() -> Self {
        Self::new(StencilTierConfig {
            enabled: false,
            ..Default::default()
        })
    }

    pub fn init_for_module(&mut self, _num_cells: usize) {}

    pub fn is_enabled(&self) -> bool {
        false
    }

    pub fn is_compiled(&self, _cell_idx: usize) -> bool {
        false
    }

    pub fn record_call(&mut self, _cell_idx: usize) -> bool {
        false
    }

    pub fn try_compile(&mut self, _cell_idx: usize, _module: &LirModule) -> bool {
        false
    }

    pub fn execute(&mut self, _vm: &mut VM, _cell_idx: usize, _base: usize) -> Result<(), VmError> {
        Err(VmError::Runtime("stencil tier disabled".into()))
    }

    pub fn stats(&self) -> StencilTierStats {
        self.stats.clone()
    }
}

fn is_pure_arith(op: OpCode) -> bool {
    matches!(
        op,
        OpCode::LoadK
            | OpCode::LoadInt
            | OpCode::LoadNil
            | OpCode::LoadBool
            | OpCode::Add
            | OpCode::Sub
            | OpCode::Mul
            | OpCode::Div
            | OpCode::Mod
            | OpCode::FloorDiv
            | OpCode::Neg
            | OpCode::Return
            | OpCode::Halt
    )
}

#[cfg(feature = "jit")]
fn has_known_unsafe_multi_step_arith_pattern(
    cell: &lumen_core::lir::LirCell,
    body: &[lumen_core::lir::Instruction],
) -> bool {
    // Known bad shape: chained arithmetic that reads parameter registers
    // directly can produce wrong results when invoked through call helpers.
    if cell.params.is_empty() {
        return false;
    }

    let reads_param = |reg: u16| cell.params.iter().any(|p| p.register == reg);
    let mut arith_steps = 0usize;
    let mut uses_param_input = false;

    for instr in body {
        match instr.op {
            OpCode::Add
            | OpCode::Sub
            | OpCode::Mul
            | OpCode::Div
            | OpCode::Mod
            | OpCode::FloorDiv => {
                arith_steps += 1;
                uses_param_input |= reads_param(instr.b) || reads_param(instr.c);
            }
            OpCode::Neg => {
                arith_steps += 1;
                uses_param_input |= reads_param(instr.b);
            }
            _ => {}
        }
    }

    arith_steps >= 2 && uses_param_input
}

/// Map an opcode to the runtime helper function name it needs for its
/// `RuntimeFuncAddr` hole.
#[cfg(feature = "jit")]
fn opcode_to_runtime_func(op: OpCode) -> &'static str {
    match op {
        OpCode::Return => "lm_rt_return",
        OpCode::Halt => "lm_rt_halt",
        OpCode::Call => "lm_rt_call",
        OpCode::TailCall => "lm_rt_tailcall",
        OpCode::Intrinsic => "lm_rt_intrinsic",
        OpCode::Perform => "lm_rt_perform",
        OpCode::HandlePush => "lm_rt_handle_push",
        OpCode::HandlePop => "lm_rt_handle_pop",
        OpCode::Resume => "lm_rt_resume",
        OpCode::OsrCheck => "lm_rt_osr_check",
        OpCode::NewList
        | OpCode::NewListStack
        | OpCode::NewMap
        | OpCode::NewRecord
        | OpCode::NewTuple
        | OpCode::NewTupleStack
        | OpCode::NewSet
        | OpCode::GetField
        | OpCode::SetField
        | OpCode::GetIndex
        | OpCode::SetIndex => "lm_rt_stencil_runtime",
        // Any other opcode that has a RuntimeFuncAddr hole falls back to the
        // stencil runtime dispatcher.
        _ => "lm_rt_stencil_runtime",
    }
}

#[cfg(feature = "jit")]
fn resolve_runtime_helper(name: &str) -> Option<u64> {
    match name {
        "lm_rt_return" => Some(crate::stencil_runtime::lm_rt_return as usize as u64),
        "lm_rt_halt" => Some(crate::stencil_runtime::lm_rt_halt as usize as u64),
        "lm_rt_call" => Some(crate::stencil_runtime::lm_rt_call as usize as u64),
        "lm_rt_tailcall" => Some(crate::stencil_runtime::lm_rt_tailcall as usize as u64),
        "lm_rt_intrinsic" => Some(crate::stencil_runtime::lm_rt_intrinsic as usize as u64),
        "lm_rt_perform" => Some(crate::vm::fiber_effects::lm_rt_perform as usize as u64),
        "lm_rt_handle_push" => Some(crate::vm::fiber_effects::lm_rt_handle_push as usize as u64),
        "lm_rt_handle_pop" => Some(crate::vm::fiber_effects::lm_rt_handle_pop as usize as u64),
        "lm_rt_resume" => Some(crate::vm::fiber_effects::lm_rt_resume as usize as u64),
        "lm_rt_stencil_runtime" => {
            Some(crate::stencil_runtime::lm_rt_stencil_runtime as usize as u64)
        }
        "lm_rt_osr_check" => Some(crate::vm::osr::osr_check::lm_rt_osr_check as usize as u64),
        _ => None,
    }
}

#[cfg(feature = "jit")]
#[cfg(target_arch = "x86_64")]
unsafe fn call_stitched(code_ptr: *const u8, regs: *mut NbValue, ctx: *mut VmContext) {
    // The System V x86-64 ABI requires RSP to be 16-byte aligned immediately
    // before a `call` instruction.  Rust's function prologue may leave RSP at
    // an unknown alignment inside an inline-asm block.  We align explicitly.
    //
    // We pass inputs through caller-saved registers so they're available at the
    // top of the asm (before we push callee-saved regs that could clobber them).
    //   rcx = code_ptr  (caller-saved, fine to use as scratch)
    //   rdi = regs      (caller-saved)
    //   rsi = ctx       (caller-saved)
    // Then we save rbx (callee-saved) as our RSP anchor, save r14/r15, align.
    std::arch::asm!(
        "push rbx",           // save rbx; callee-saved — used as rsp anchor
        "push r14",           // save old r14 on stack (at original_rsp - 16)
        "push r15",           // save old r15 on stack (at original_rsp - 24)
        "mov rbx, rsp",       // rbx = current rsp (original_rsp - 24)
        "and rsp, -16",       // align rsp to 16-byte boundary
        "mov r14, rdi",       // stencil ABI: r14 = register file base (was regs in rdi)
        "mov r15, rsi",       // stencil ABI: r15 = VmContext* (was ctx in rsi)
        "call rcx",           // call stitched code (code_ptr in rcx); rsp 16-aligned ✓
        // Restore callee-saved registers and rsp.
        "mov rsp, rbx",       // rsp = original_rsp - 24
        "pop r15",            // restore old r15; rsp = original_rsp - 16
        "pop r14",            // restore old r14; rsp = original_rsp - 8
        "pop rbx",            // restore old rbx; rsp = original_rsp
        in("rcx") code_ptr,
        in("rdi") regs,
        in("rsi") ctx,
        clobber_abi("C"),
    );
}

#[cfg(feature = "jit")]
#[cfg(not(target_arch = "x86_64"))]
unsafe fn call_stitched(_code_ptr: *const u8, _regs: *mut NbValue, _ctx: *mut VmContext) {}
