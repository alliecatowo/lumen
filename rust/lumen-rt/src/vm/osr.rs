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
use thiserror::Error;

#[cfg(feature = "jit")]
use lumen_codegen::jit::JitEngine;

pub mod osr_check {
    use super::*;

    /// Threshold for triggering OSR tier-up
    pub const OSR_HOT_THRESHOLD: u64 = 1000;

    /// Per-cell OSR state tracked during execution
    #[derive(Debug, Clone, Copy)]
    pub struct OsrState {
        pub cell_idx: usize,
        pub loop_count: u64,
        pub compiled: bool,
        pub compiled_fn: Option<*const ()>,
    }

    impl OsrState {
        pub fn new(cell_idx: usize) -> Self {
            Self {
                cell_idx,
                loop_count: 0,
                compiled: false,
                compiled_fn: None,
            }
        }

        /// Check if we should trigger tier-up
        pub fn should_tier_up(&self) -> bool {
            self.loop_count >= OSR_HOT_THRESHOLD && !self.compiled
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
        #[cfg(feature = "jit")]
        jit_engine: Option<JitEngine>,
    }

    impl OsrRuntime {
        pub fn new(num_cells: usize) -> Self {
            let mut states = Vec::with_capacity(num_cells);
            for idx in 0..num_cells {
                states.push(OsrState::new(idx));
            }
            Self {
                states,
                #[cfg(feature = "jit")]
                jit_engine: None,
            }
        }

        #[cfg(feature = "jit")]
        pub fn init_jit(&mut self) {
            use lumen_codegen::jit::CodegenSettings;
            let settings = CodegenSettings::default();
            self.jit_engine = Some(JitEngine::new(settings, 0));
        }

        pub fn get_state(&mut self, cell_idx: usize) -> &mut OsrState {
            if cell_idx >= self.states.len() {
                self.states.resize(cell_idx + 1, OsrState::new(cell_idx));
            }
            &mut self.states[cell_idx]
        }

        /// Record an OSR check for a cell, returns true if tier-up needed
        pub fn record_and_check(&mut self, cell_idx: usize) -> bool {
            let state = self.get_state(cell_idx);
            state.record_check()
        }

        /// Attempt to compile a cell for OSR
        #[cfg(feature = "jit")]
        pub fn try_compile(&mut self, cell_idx: usize, module: &LirModule) -> bool {
            if cell_idx >= module.cells.len() {
                return false;
            }

            let cell_name = module.cells[cell_idx].name.clone();

            if let Some(ref mut engine) = self.jit_engine {
                match engine.compile_hot(&cell_name, module) {
                    Ok(()) => {
                        if engine.is_compiled(&cell_name) {
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

        /// Get the compiled function pointer for a cell
        pub fn get_compiled_fn(&self, cell_idx: usize) -> Option<*const ()> {
            if cell_idx < self.states.len() {
                self.states[cell_idx].compiled_fn
            } else {
                None
            }
        }
    }

    /// OSR check function called from stencil JIT.
    /// Returns 0 if no tier-up needed, or a non-zero function pointer to jump to.
    #[no_mangle]
    pub unsafe extern "C" fn lm_rt_osr_check(
        vm_ctx: &mut VM,
        cell_idx: usize,
        _current_ip: usize,
    ) -> *const () {
        // Get or create the OSR runtime state
        let osr_runtime = &mut vm_ctx.osr_runtime;

        // Record the check and see if we should tier up
        if osr_runtime.record_and_check(cell_idx) {
            // Need to compile - try to compile the cell
            if let Some(ref module) = vm_ctx.module {
                if osr_runtime.try_compile(cell_idx, module) {
                    // Return the compiled function pointer
                    return osr_runtime
                        .get_compiled_fn(cell_idx)
                        .unwrap_or(std::ptr::null());
                }
            }
        }

        // Check if we already have compiled code to jump to
        osr_runtime
            .get_compiled_fn(cell_idx)
            .unwrap_or(std::ptr::null())
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
}

/// Capture a snapshot of the interpreter's register state.
pub fn capture_register_state(vm: &VM, base: usize, register_count: usize) -> Vec<u64> {
    (0..register_count)
        .map(|idx| vm.reg_nb(base + idx).0)
        .collect()
}

/// Attempt to perform an OSR transition to compiled code.
///
/// Currently a stub that reports missing OSR points.
pub unsafe fn perform_osr_transition(
    _vm: &mut VM,
    cell: &lumen_core::lir::LirCell,
    ip: usize,
) -> Result<(), OsrError> {
    if cell.osr_points.iter().any(|p| p.ip == ip) {
        return Err(OsrError::Unavailable(
            "OSR transition not yet implemented".to_string(),
        ));
    }
    Err(OsrError::OsrPointNotFound {
        cell: cell.name.clone(),
        ip,
    })
}

/// Reconstruct interpreter state from an OSR snapshot.
pub fn reconstruct_interpreter_state(_vm: &mut VM) -> Result<(), OsrError> {
    Err(OsrError::Unavailable(
        "OSR reconstruction not yet implemented".to_string(),
    ))
}
