//! Tier 1 stencil execution support.

use std::collections::HashSet;

use lumen_core::lir::{LirCell, LirModule, OpCode};
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

    pub fn disabled() -> Self {
        Self::new(StencilTierConfig {
            enabled: false,
            ..Default::default()
        })
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
        self.call_counts[cell_idx] == self.config.hot_threshold + 1
    }

    pub fn try_compile(&mut self, cell_idx: usize, module: &LirModule) -> bool {
        let cell = match module.cells.get(cell_idx) {
            Some(cell) => cell,
            None => {
                self.stats.compile_failures += 1;
                return false;
            }
        };

        if !cell_is_supported(cell) {
            self.stats.compile_failures += 1;
            return false;
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
                Ok(code) => code.code_len,
                Err(_) => {
                    self.stats.compile_failures += 1;
                    return false;
                }
            }
        };

        let (start_offset, library) = match self.stitcher.as_ref() {
            Some(stitcher) => {
                let start_offset = stitcher.code_used().saturating_sub(code_len);
                let library = stitcher.library().clone();
                (start_offset, library)
            }
            None => {
                self.stats.compile_failures += 1;
                return false;
            }
        };
        let mut offsets = Vec::with_capacity(cell.instructions.len());
        let mut offset = start_offset;
        for instr in &cell.instructions {
            offsets.push(offset);
            if let Some(stencil) = library.get(instr.op as u8) {
                offset += stencil.code.len();
            }
            if matches!(instr.op, OpCode::Return | OpCode::Halt) {
                break;
            }
        }

        let stitcher = match self.stitcher.as_mut() {
            Some(stitcher) => stitcher,
            None => {
                self.stats.compile_failures += 1;
                return false;
            }
        };

        if stitcher
            .patch_runtime_funcs(cell, start_offset, &offsets, resolve_runtime_helper)
            .is_err()
        {
            self.stats.compile_failures += 1;
            return false;
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
        let regs_ptr = unsafe { vm.registers.as_mut_ptr().add(base) };
        let ctx_ptr = vm.vm_ctx.as_ptr();
        unsafe {
            (*ctx_ptr).stack_pool = vm as *mut VM as *mut ();
        }
        unsafe {
            call_stitched(code.code_ptr, regs_ptr, ctx_ptr);
        }
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
fn cell_is_supported(cell: &LirCell) -> bool {
    cell.instructions.iter().all(|instr| match instr.op {
        OpCode::Nop
        | OpCode::LoadK
        | OpCode::LoadNil
        | OpCode::LoadBool
        | OpCode::LoadInt
        | OpCode::Move
        | OpCode::MoveOwn
        | OpCode::NewList
        | OpCode::NewListStack
        | OpCode::NewMap
        | OpCode::NewRecord
        | OpCode::NewTuple
        | OpCode::NewTupleStack
        | OpCode::NewSet
        | OpCode::GetField
        | OpCode::SetField
        | OpCode::GetIndex
        | OpCode::SetIndex
        | OpCode::Add
        | OpCode::Sub
        | OpCode::Mul
        | OpCode::Div
        | OpCode::Mod
        | OpCode::Neg
        | OpCode::Eq
        | OpCode::Lt
        | OpCode::Le
        | OpCode::Not
        | OpCode::Jmp
        | OpCode::Break
        | OpCode::Continue
        | OpCode::Test
        | OpCode::Call
        | OpCode::TailCall
        | OpCode::Return
        | OpCode::Halt
        | OpCode::Intrinsic => true,
        _ => false,
    })
}

#[cfg(not(feature = "jit"))]
fn cell_is_supported(_cell: &LirCell) -> bool {
    false
}

#[cfg(feature = "jit")]
#[cfg(target_arch = "x86_64")]
unsafe fn call_stitched(code_ptr: *const u8, regs: *mut NbValue, ctx: *mut VmContext) {
    std::arch::asm!(
        "push r14",
        "push r15",
        "mov r14, {regs}",
        "mov r15, {ctx}",
        "call {code}",
        "pop r15",
        "pop r14",
        regs = in(reg) regs,
        ctx = in(reg) ctx,
        code = in(reg) code_ptr,
        clobber_abi("C"),
    );
}

#[cfg(feature = "jit")]
#[cfg(not(target_arch = "x86_64"))]
unsafe fn call_stitched(_code_ptr: *const u8, _regs: *mut NbValue, _ctx: *mut VmContext) {}
