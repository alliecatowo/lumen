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
use lumen_core::lir::{LirModule, OpCode};
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
            hot_threshold: 3,
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
    /// Cached raw function pointers indexed by cell_idx for O(1) dispatch
    /// (avoids HashMap lookup on every call).
    #[cfg(feature = "jit")]
    fn_ptrs: Vec<Option<*const u8>>,
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
            #[cfg(feature = "jit")]
            fn_ptrs: Vec::new(),
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
        #[cfg(feature = "jit")]
        self.fn_ptrs.resize(num_cells, None);
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

            // Keep only the known-unsafe guard. Collection opcodes such as
            // Append/NewList/NewListStack/GetIndex/SetIndex/NewMap are eligible.
            if cell
                .instructions
                .iter()
                .any(|i| Self::is_known_unsafe_opcode(i.op))
            {
                self.stats.compile_failures += 1;
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
            let make_settings = || CodegenSettings {
                opt_level: settings.opt_level,
                target: settings.target.clone(),
            };

            // Create a new engine each time (Cranelift JITModule doesn't support
            // incremental addition of functions after finalize_definitions).
            let mut engine = JitEngine::new(make_settings(), 0);
            let mut compiled = engine.compile_hot(cell_name, module).is_ok();

            // If full-module lowering fails, retry in isolated mode for cells
            // without direct Call/TailCall. This avoids blanket module failure
            // for collection-heavy leaf cells while preserving call semantics.
            if !compiled && !Self::has_direct_call(cell) {
                let mut isolated_engine = JitEngine::new(make_settings(), 0);
                let isolated_module = Self::single_cell_module(module, cell_idx);
                compiled = isolated_engine
                    .compile_hot(cell_name, &isolated_module)
                    .is_ok();
                if compiled {
                    engine = isolated_engine;
                }
            }

            if !compiled {
                self.stats.compile_failures += 1;
                return false;
            }

            // Mark only the hot cell as compiled.
            if engine.is_compiled(cell_name) {
                self.compiled.insert(cell_idx);
                self.stats.cells_compiled += 1;
                // Cache raw fn pointer for O(1) indexed dispatch.
                if let Some(ptr) = engine.get_compiled_fn(cell_name) {
                    if cell_idx < self.fn_ptrs.len() {
                        self.fn_ptrs[cell_idx] = Some(ptr as *const u8);
                    }
                }
            }
            self.engine = Some(engine);
            true
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

    /// Execute a JIT-compiled cell by index, bypassing the HashMap lookup.
    /// Falls back to `execute()` if no cached fn pointer is available.
    #[inline]
    pub fn execute_by_idx(
        &mut self,
        cell_idx: usize,
        args: &[i64],
        vm_ctx: &lumen_codegen::vm_context::VmContext,
        cell_name: &str,
    ) -> Option<i64> {
        #[cfg(feature = "jit")]
        {
            // Fast path: use cached fn pointer (no HashMap lookup).
            if let Some(Some(ptr)) = self.fn_ptrs.get(cell_idx) {
                let fn_ptr = *ptr;
                let ctx_mut = vm_ctx as *const lumen_codegen::vm_context::VmContext
                    as *mut lumen_codegen::vm_context::VmContext;
                let raw = unsafe {
                    match args.len() {
                        0 => {
                            let f: fn(*mut lumen_codegen::vm_context::VmContext) -> i64 =
                                std::mem::transmute(fn_ptr);
                            f(ctx_mut)
                        }
                        1 => {
                            let f: fn(*mut lumen_codegen::vm_context::VmContext, i64) -> i64 =
                                std::mem::transmute(fn_ptr);
                            f(ctx_mut, args[0])
                        }
                        2 => {
                            let f: fn(*mut lumen_codegen::vm_context::VmContext, i64, i64) -> i64 =
                                std::mem::transmute(fn_ptr);
                            f(ctx_mut, args[0], args[1])
                        }
                        3 => {
                            let f: fn(
                                *mut lumen_codegen::vm_context::VmContext,
                                i64,
                                i64,
                                i64,
                            ) -> i64 = std::mem::transmute(fn_ptr);
                            f(ctx_mut, args[0], args[1], args[2])
                        }
                        4 => {
                            let f: fn(
                                *mut lumen_codegen::vm_context::VmContext,
                                i64,
                                i64,
                                i64,
                                i64,
                            ) -> i64 = std::mem::transmute(fn_ptr);
                            f(ctx_mut, args[0], args[1], args[2], args[3])
                        }
                        5 => {
                            let f: fn(
                                *mut lumen_codegen::vm_context::VmContext,
                                i64,
                                i64,
                                i64,
                                i64,
                                i64,
                            ) -> i64 = std::mem::transmute(fn_ptr);
                            f(ctx_mut, args[0], args[1], args[2], args[3], args[4])
                        }
                        6 => {
                            let f: fn(
                                *mut lumen_codegen::vm_context::VmContext,
                                i64,
                                i64,
                                i64,
                                i64,
                                i64,
                                i64,
                            ) -> i64 = std::mem::transmute(fn_ptr);
                            f(ctx_mut, args[0], args[1], args[2], args[3], args[4], args[5])
                        }
                        _ => return None,
                    }
                };
                if lumen_codegen::jit::jit_check_divzero_trap() {
                    return None;
                }
                self.stats.jit_executions += 1;
                return Some(raw);
            }
            // Slow path: fall back to HashMap-based lookup.
            return self.execute(cell_name, args, vm_ctx);
        }

        #[cfg(not(feature = "jit"))]
        {
            let _ = (cell_idx, args, cell_name);
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

    #[cfg(feature = "jit")]
    #[inline(always)]
    fn is_known_unsafe_opcode(op: OpCode) -> bool {
        matches!(op, OpCode::NewRecord | OpCode::NewSet)
    }

    #[cfg(feature = "jit")]
    #[inline(always)]
    fn has_direct_call(cell: &lumen_core::lir::LirCell) -> bool {
        cell.instructions
            .iter()
            .any(|i| matches!(i.op, OpCode::Call | OpCode::TailCall))
    }

    #[cfg(feature = "jit")]
    fn single_cell_module(module: &LirModule, cell_idx: usize) -> LirModule {
        let mut isolated = module.clone();
        isolated.cells = vec![module.cells[cell_idx].clone()];
        isolated
    }
}
