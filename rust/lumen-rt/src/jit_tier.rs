//! Tiered JIT compilation integration for the Lumen VM.
//!
//! Provides the `JitTier` abstraction that sits between the interpreter and the
//! Cranelift JIT engine. During interpretation, every cell call is tracked. When
//! a cell's call count crosses a configurable threshold it is compiled to native
//! code via Cranelift and subsequent calls are dispatched directly as native
//! function pointers — bypassing the interpreter entirely.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────┐   cold    ┌─────────────┐
//! │ Interpreter  │──────────▶│ call_count++ │
//! │  dispatch    │           └──────┬───────┘
//! └──────┬───────┘                  │ count > threshold?
//!        │ hot                      │
//!        ▼                          ▼
//! ┌─────────────┐           ┌──────────────┐
//! │  JIT native  │◀──────────│  Cranelift    │
//! │  fn pointer  │  compile  │  JIT compile  │
//! └─────────────┘           └──────────────┘
//! ```
//!
//! All cells are eligible for JIT compilation attempt. If a cell contains
//! unsupported opcodes, compilation fails gracefully and the cell falls back
//! to the interpreter.

#[cfg(feature = "jit")]
use lumen_codegen::jit::{CodegenSettings, JitEngine, JitStats, OptLevel};
use lumen_core::lir::{Constant, LirCell, LirModule, OpCode};
use std::collections::HashSet;

/// Configuration for the tiered JIT.
#[derive(Debug, Clone)]
pub struct JitTierConfig {
    /// Number of calls before a cell is considered "hot" and compiled.
    pub hot_threshold: u64,
    /// Optimisation level for JIT compilation.
    pub opt_level: JitOptLevel,
    /// Whether JIT is enabled at all.
    pub enabled: bool,
}

/// Mirror of codegen OptLevel so the VM crate doesn't leak codegen types
/// when the jit feature is disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JitOptLevel {
    None,
    Speed,
    SpeedAndSize,
}

impl Default for JitTierConfig {
    fn default() -> Self {
        Self {
            hot_threshold: 10,
            opt_level: JitOptLevel::Speed,
            enabled: true,
        }
    }
}

impl JitTierConfig {
    /// Create a config with the given hot threshold.
    pub fn from_threshold(hot_threshold: u64) -> Self {
        Self {
            hot_threshold,
            ..Default::default()
        }
    }
}

/// The tiered JIT state held by the VM.
///
/// When the `jit` feature is disabled this is a zero-size struct with no-op
/// methods, so there is zero overhead.
pub struct JitTier {
    /// Per-cell call counts (indexed by cell_idx for O(1) lookup).
    call_counts: Vec<u64>,
    /// Set of cell indices that have been compiled.
    compiled: HashSet<usize>,
    /// Configuration.
    config: JitTierConfig,
    /// The actual Cranelift JIT engine (only present when feature = "jit").
    #[cfg(feature = "jit")]
    engine: Option<JitEngine>,
    /// Statistics.
    pub stats: JitTierStats,
}

/// Public statistics about tiered JIT activity.
#[derive(Debug, Clone, Default)]
pub struct JitTierStats {
    /// Total number of JIT-compiled cells.
    pub cells_compiled: u64,
    /// Total number of native JIT executions (calls that bypassed the interpreter).
    pub jit_executions: u64,
    /// Total number of compilation attempts that failed.
    pub compile_failures: u64,
    /// Total number of calls tracked.
    pub total_calls_tracked: u64,
}

impl JitTier {
    /// Create a new JIT tier with the given configuration.
    pub fn new(config: JitTierConfig) -> Self {
        Self {
            call_counts: Vec::new(),
            compiled: HashSet::new(),
            config,
            #[cfg(feature = "jit")]
            engine: None,
            stats: JitTierStats::default(),
        }
    }

    /// Create a disabled JIT tier (no-op).
    pub fn disabled() -> Self {
        Self::new(JitTierConfig {
            enabled: false,
            ..Default::default()
        })
    }

    /// Initialise internal vectors to match the number of cells in the module.
    /// Must be called after `VM::load()`.
    pub fn init_for_module(&mut self, num_cells: usize) {
        self.call_counts.resize(num_cells, 0);
        self.compiled.clear();
        self.stats = JitTierStats::default();
    }

    /// Check whether JIT is enabled.
    #[inline(always)]
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Check if a cell has been JIT-compiled.
    #[inline(always)]
    pub fn is_compiled(&self, cell_idx: usize) -> bool {
        self.compiled.contains(&cell_idx)
    }

    /// Record a call to `cell_idx`. Returns `true` if the cell *just* crossed
    /// the hot threshold and should be compiled NOW.
    #[inline]
    pub fn record_call(&mut self, cell_idx: usize) -> bool {
        if !self.config.enabled {
            return false;
        }
        if cell_idx >= self.call_counts.len() {
            return false;
        }
        // Already compiled — no need to track further.
        if self.compiled.contains(&cell_idx) {
            return false;
        }
        self.call_counts[cell_idx] += 1;
        self.stats.total_calls_tracked += 1;
        self.call_counts[cell_idx] == self.config.hot_threshold + 1
    }

    /// Attempt to compile a hot cell. Returns `true` on success.
    ///
    /// On the `jit` feature, this creates/updates the Cranelift JIT engine and
    /// compiles only the specified hot cell — not the entire module. Cells with
    /// unsupported opcodes will have deopt stubs inserted; if compilation fails
    /// entirely the cell falls back to the interpreter.
    ///
    /// On no-jit builds, this is a no-op that returns `false`.
    pub fn try_compile(&mut self, cell_idx: usize, module: &LirModule) -> bool {
        #[cfg(feature = "jit")]
        {
            if module.cells.is_empty() || cell_idx >= module.cells.len() {
                self.stats.compile_failures += 1;
                return false;
            }

            let cell = &module.cells[cell_idx];
            let cell_name = &cell.name;

            // Skip JIT for cells that perform string arithmetic — the JIT's
            // Add/Concat lowering only handles numeric types.  If a string
            // constant flows into an Add or Concat operand, the JIT would
            // miscompile it as numeric addition.  String constants used ONLY
            // as Call callee names or NewUnion/IsVariant tags are fine.
            if has_string_arithmetic(cell) {
                return false;
            }

            // Skip JIT for cells with complex collection constructors that
            // have known codegen bugs (SIGSEGV in generated code).
            if cell
                .instructions
                .iter()
                .any(|i| matches!(i.op, OpCode::NewMap | OpCode::NewRecord | OpCode::NewSet))
            {
                return false;
            }

            let opt = match self.config.opt_level {
                JitOptLevel::None => OptLevel::None,
                JitOptLevel::Speed => OptLevel::Speed,
                JitOptLevel::SpeedAndSize => OptLevel::SpeedAndSize,
            };
            let settings = CodegenSettings {
                opt_level: opt,
                target: None,
            };

            // Create a new engine each time (Cranelift JITModule doesn't support
            // incremental addition of functions after finalize_definitions).
            let mut engine = JitEngine::new(settings, 0);
            match engine.compile_hot(cell_name, module) {
                Ok(()) => {
                    // Mark only the hot cell as compiled.
                    if engine.is_compiled(cell_name) {
                        self.compiled.insert(cell_idx);
                        self.stats.cells_compiled += 1;
                    }
                    self.engine = Some(engine);
                    true
                }
                Err(_e) => {
                    self.stats.compile_failures += 1;
                    false
                }
            }
        }

        #[cfg(not(feature = "jit"))]
        {
            let _ = module;
            false
        }
    }

    /// Execute a JIT-compiled cell with the given i64 arguments.
    /// Returns `Some(result)` on success, `None` if not compiled or execution fails.
    #[inline]
    pub fn execute(
        &mut self,
        cell_name: &str,
        args: &[i64],
        vm_ctx: &lumen_codegen::vm_context::VmContext,
    ) -> Option<i64> {
        #[cfg(feature = "jit")]
        {
            if let Some(ref mut engine) = self.engine {
                match engine.execute_jit(vm_ctx, cell_name, args) {
                    Ok(result) => {
                        self.stats.jit_executions += 1;
                        Some(result)
                    }
                    Err(_) => None,
                }
            } else {
                None
            }
        }

        #[cfg(not(feature = "jit"))]
        {
            let _ = (cell_name, args);
            None
        }
    }

    /// Check if a compiled cell returns a heap-allocated string pointer.
    /// When true, the i64 result from `execute` is a `*mut String` that must
    /// be consumed via `lumen_codegen::jit::jit_take_string`.
    pub fn returns_string(&self, cell_name: &str) -> bool {
        #[cfg(feature = "jit")]
        {
            if let Some(ref engine) = self.engine {
                engine.returns_string(cell_name)
            } else {
                false
            }
        }

        #[cfg(not(feature = "jit"))]
        {
            let _ = cell_name;
            false
        }
    }

    /// Get the NaN-boxing return type for a compiled cell.
    /// Returns `None` if the cell is not compiled or the JIT feature is disabled.
    #[cfg(feature = "jit")]
    pub fn return_type(&self, cell_name: &str) -> Option<lumen_codegen::jit::JitVarType> {
        self.engine.as_ref().and_then(|e| e.return_type(cell_name))
    }

    /// Get a snapshot of JIT tier statistics.
    pub fn tier_stats(&self) -> JitTierStats {
        self.stats.clone()
    }

    /// Get the underlying codegen JIT stats (if available).
    #[cfg(feature = "jit")]
    pub fn codegen_stats(&self) -> Option<JitStats> {
        self.engine.as_ref().map(|e| e.stats())
    }

    /// Get the hot threshold.
    pub fn hot_threshold(&self) -> u64 {
        self.config.hot_threshold
    }
}

/// Check if a cell uses string constants as arithmetic operands (Add/Concat).
///
/// Flow-sensitive: tracks which registers currently hold string data, removing
/// them when overwritten by non-string sources (e.g. Call results, arithmetic).
/// String constants used only as Call callee names or NewUnion tags are fine.
fn has_string_arithmetic(cell: &LirCell) -> bool {
    let mut string_regs: HashSet<u16> = HashSet::new();
    for inst in &cell.instructions {
        match inst.op {
            // LoadK of a string constant → register becomes string
            OpCode::LoadK => {
                let const_idx = inst.bx() as usize;
                if const_idx < cell.constants.len()
                    && matches!(cell.constants[const_idx], Constant::String(_))
                {
                    string_regs.insert(inst.a);
                } else {
                    // Loading a non-string constant → register is no longer string
                    string_regs.remove(&inst.a);
                }
            }
            // Move/MoveOwn propagate string status
            OpCode::Move | OpCode::MoveOwn => {
                if string_regs.contains(&inst.b) {
                    string_regs.insert(inst.a);
                } else {
                    string_regs.remove(&inst.a);
                }
            }
            // Add/Concat with a string operand → cell has string arithmetic
            OpCode::Add | OpCode::Concat => {
                if string_regs.contains(&inst.b) || string_regs.contains(&inst.c) {
                    return true;
                }
                // Result of Add is numeric (or wrong), remove string status
                string_regs.remove(&inst.a);
            }
            // Call/TailCall/Intrinsic overwrite the result register with a
            // non-string value (the return value).  The callee name in the
            // base register is consumed and replaced.
            OpCode::Call | OpCode::TailCall => {
                string_regs.remove(&inst.a);
            }
            OpCode::Intrinsic => {
                string_regs.remove(&inst.a);
            }
            // Any other opcode that writes to register A overwrites it
            _ => {
                // Most opcodes write their result to register A.  If the
                // result is not a string, clear the string flag.
                string_regs.remove(&inst.a);
            }
        }
    }
    false
}
