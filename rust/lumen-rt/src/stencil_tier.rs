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

    /// Return an OSR entry point for a stitched cell at the given LIR IP.
    pub fn get_osr_entry(&self, cell_idx: usize, lir_ip: usize) -> Option<OsrStencilEntry> {
        let stitcher = self.stitcher.as_ref()?;
        let code = stitcher.get_compiled(cell_idx)?;
        if lir_ip >= code.instruction_offsets.len() {
            return None;
        }
        let offset = code.instruction_offsets[lir_ip];
        let code_ptr = unsafe { code.code_ptr.add(offset) };
        Some(OsrStencilEntry {
            code_ptr,
            reg_count: code.register_count,
        })
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

    pub fn try_compile(
        &mut self,
        cell_idx: usize,
        module: &LirModule,
        jit_tier: &mut crate::jit_tier::JitTier,
    ) -> bool {
        let cell = match module.cells.get(cell_idx) {
            Some(cell) => cell,
            None => {
                self.stats.compile_failures += 1;
                return false;
            }
        };

        // Don't stencil-compile cells with Call/TailCall opcodes.
        // Stencil's lm_rt_call re-enters the interpreter on the main Rust stack.
        // For recursive or cross-cell calls, this causes deep Rust stack nesting
        // that overflows the stack (each LIR call ≈ 4–5 Rust frames deep).
        // Cells with calls are better served by the Cranelift JIT (tier 2).
        use lumen_core::lir::OpCode;
        let has_call = cell
            .instructions
            .iter()
            .any(|i| matches!(i.op, OpCode::Call | OpCode::TailCall));
        if has_call {
            self.stats.compile_failures += 1;
            return jit_tier.try_compile(cell_idx, module);
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
                Err(lumen_codegen::stitcher::StitchError::MissingStencil(_, _)) => {
                    self.stats.compile_failures += 1;
                    return jit_tier.try_compile(cell_idx, module);
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
                let stencil = if instr.op == OpCode::Intrinsic {
                    library
                        .get_intrinsic(instr.b as u8)
                        .or_else(|| library.get(op_byte))
                } else {
                    library.get(op_byte)
                };
                if let Some(stencil) = stencil {
                    // Check all holes in this stencil for RuntimeFuncAddr.
                    for hole in &stencil.holes {
                        if hole.hole_type
                            == lumen_codegen::stencil_format::HoleType::RuntimeFuncAddr
                        {
                            let func_name = if instr.op == OpCode::Intrinsic {
                                intrinsic_runtime_helper(instr.b as u8)
                                    .unwrap_or_else(|| opcode_to_runtime_func(instr.op))
                            } else {
                                opcode_to_runtime_func(instr.op)
                            };
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

#[cfg(feature = "jit")]
#[derive(Debug, Clone, Copy)]
pub struct OsrStencilEntry {
    pub code_ptr: *const u8,
    pub reg_count: usize,
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

    pub fn get_osr_entry(&self, _cell_idx: usize, _lir_ip: usize) -> Option<OsrStencilEntry> {
        None
    }

    pub fn record_call(&mut self, _cell_idx: usize) -> bool {
        false
    }

    pub fn try_compile(
        &mut self,
        _cell_idx: usize,
        _module: &LirModule,
        _jit_tier: &mut crate::jit_tier::JitTier,
    ) -> bool {
        false
    }

    pub fn execute(&mut self, _vm: &mut VM, _cell_idx: usize, _base: usize) -> Result<(), VmError> {
        Err(VmError::Runtime("stencil tier disabled".into()))
    }

    pub fn stats(&self) -> StencilTierStats {
        self.stats.clone()
    }
}

#[cfg(not(feature = "jit"))]
#[derive(Debug, Clone, Copy)]
pub struct OsrStencilEntry {
    pub code_ptr: *const u8,
    pub reg_count: usize,
}

#[cfg(feature = "jit")]
fn intrinsic_runtime_helper(func_id: u8) -> Option<&'static str> {
    match func_id {
        // Append
        24 => Some("jit_rt_list_append"),
        // Range
        25 => Some("jit_rt_range"),
        // Sort / SortAsc
        29 | 129 => Some("jit_rt_sort"),
        // Length / Count / Size
        0 | 1 | 72 => Some("jit_rt_collection_len"),
        // Keys / Values
        14 => Some("jit_rt_map_keys"),
        15 => Some("jit_rt_map_values"),
        _ => None,
    }
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
    // Cast via *const () to satisfy Rust's "direct cast of fn item to integer" lint.
    macro_rules! fn_addr {
        ($f:expr) => {
            Some($f as *const () as usize as u64)
        };
    }
    match name {
        "lm_rt_return" => fn_addr!(crate::stencil_runtime::lm_rt_return),
        "lm_rt_halt" => fn_addr!(crate::stencil_runtime::lm_rt_halt),
        "lm_rt_call" => fn_addr!(crate::stencil_runtime::lm_rt_call),
        "lm_rt_tailcall" => fn_addr!(crate::stencil_runtime::lm_rt_tailcall),
        "lm_rt_intrinsic" => fn_addr!(crate::stencil_runtime::lm_rt_intrinsic),
        "lm_rt_perform" => fn_addr!(crate::vm::fiber_effects::lm_rt_perform),
        "lm_rt_handle_push" => fn_addr!(crate::vm::fiber_effects::lm_rt_handle_push),
        "lm_rt_handle_pop" => fn_addr!(crate::vm::fiber_effects::lm_rt_handle_pop),
        "lm_rt_resume" => fn_addr!(crate::vm::fiber_effects::lm_rt_resume),
        "lm_rt_stencil_runtime" => fn_addr!(crate::stencil_runtime::lm_rt_stencil_runtime),
        "lm_rt_osr_check" => fn_addr!(crate::vm::osr::osr_check::lm_rt_osr_check),
        "jit_rt_list_append" => fn_addr!(lumen_codegen::collection_helpers::jit_rt_list_append),
        "jit_rt_range" => fn_addr!(lumen_codegen::collection_helpers::jit_rt_range),
        "jit_rt_sort" => fn_addr!(lumen_codegen::collection_helpers::jit_rt_sort),
        "jit_rt_collection_len" => {
            fn_addr!(lumen_codegen::collection_helpers::jit_rt_collection_len)
        }
        "jit_rt_map_keys" => fn_addr!(lumen_codegen::collection_helpers::jit_rt_map_keys),
        "jit_rt_map_values" => fn_addr!(lumen_codegen::collection_helpers::jit_rt_map_values),
        _ => None,
    }
}

#[cfg(feature = "jit")]
#[cfg(target_arch = "x86_64")]
pub(crate) unsafe fn call_stitched(code_ptr: *const u8, regs: *mut NbValue, ctx: *mut VmContext) {
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
pub(crate) unsafe fn call_stitched(
    _code_ptr: *const u8,
    _regs: *mut NbValue,
    _ctx: *mut VmContext,
) {
}
