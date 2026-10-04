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
use lumen_compiler::codegen::jit::{CodegenSettings, JitEngine, JitStats, OptLevel};
use lumen_compiler::compiler::lir::LirModule;
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

/// How the raw `i64` result of a native call maps back to a VM [`Value`].
///
/// [`Value`]: crate::vm::values::Value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JitRet {
    Int,
    Bool,
    /// Heap `*mut String` (experimental tier only; never produced by the VM's
    /// default strict tier).
    Str,
}

/// Returns true when the `LUMEN_JIT` environment variable asks for the JIT
/// to be disabled (`0`, `off`, `false`, `no`).
pub fn jit_disabled_by_env() -> bool {
    match std::env::var("LUMEN_JIT") {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "0" | "off" | "false" | "no"
        ),
        Err(_) => false,
    }
}

/// Eligibility status for a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CellEligibility {
    /// Not yet checked.
    Unknown,
    /// Eligible for JIT compilation.
    Eligible,
    /// Not eligible — will remain interpreted.
    NotEligible,
}

/// The tiered JIT state held by the VM.
///
/// When the `jit` feature is disabled this is a zero-size struct with no-op
/// methods, so there is zero overhead.
pub struct JitTier {
    /// Per-cell call counts (indexed by cell_idx for O(1) lookup).
    call_counts: Vec<u64>,
    /// Per-cell eligibility cache.
    eligibility: Vec<CellEligibility>,
    /// Set of cell indices that have been compiled.
    compiled: HashSet<usize>,
    /// Configuration.
    config: JitTierConfig,
    /// The actual Cranelift JIT engine (only present when feature = "jit").
    #[cfg(feature = "jit")]
    engine: Option<JitEngine>,
    /// True once the module-wide compilation has been attempted.
    engine_attempted: bool,
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
    /// Native calls that trapped (overflow, division by zero, stack budget)
    /// and were re-run by the interpreter.
    pub jit_fallbacks: u64,
}

impl JitTier {
    /// Create a new JIT tier with the given configuration.
    pub fn new(mut config: JitTierConfig) -> Self {
        if jit_disabled_by_env() {
            config.enabled = false;
        }
        Self {
            call_counts: Vec::new(),
            eligibility: Vec::new(),
            compiled: HashSet::new(),
            config,
            #[cfg(feature = "jit")]
            engine: None,
            engine_attempted: false,
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
        self.eligibility.resize(num_cells, CellEligibility::Unknown);
        self.compiled.clear();
        self.engine_attempted = false;
        #[cfg(feature = "jit")]
        {
            self.engine = None;
        }
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

    /// Check and cache JIT eligibility for a cell.
    /// All cells are eligible — if compilation fails for unsupported opcodes,
    /// the cell gracefully falls back to the interpreter.
    pub fn check_eligibility(&mut self, cell_idx: usize, _module: &LirModule) -> bool {
        if cell_idx >= self.eligibility.len() {
            return false;
        }
        match self.eligibility[cell_idx] {
            CellEligibility::Eligible => true,
            CellEligibility::NotEligible => false,
            CellEligibility::Unknown => {
                // All cells are eligible for JIT compilation attempt.
                // If compilation fails (unsupported opcodes), the cell falls back to interpreter.
                self.eligibility[cell_idx] = CellEligibility::Eligible;
                true
            }
        }
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

    /// Compile the module's eligible cells (once) and report whether
    /// `cell_idx` is among them.
    ///
    /// The strict tier decides eligibility for the whole module at once (a cell
    /// is only compiled if every callee is too), so one compilation covers every
    /// cell. A cell that was not compiled is marked not eligible and stays
    /// interpreted. Compiler panics are caught and treated as "not compiled".
    ///
    /// On no-jit builds this is a no-op that returns `false`.
    pub fn try_compile(&mut self, cell_idx: usize, module: &LirModule) -> bool {
        #[cfg(feature = "jit")]
        {
            if !self.engine_attempted {
                self.engine_attempted = true;
                if !module.cells.is_empty() {
                    let opt = match self.config.opt_level {
                        JitOptLevel::None => OptLevel::None,
                        JitOptLevel::Speed => OptLevel::Speed,
                        JitOptLevel::SpeedAndSize => OptLevel::SpeedAndSize,
                    };
                    let settings = CodegenSettings {
                        opt_level: opt,
                        target: None,
                    };
                    let mut engine = JitEngine::new(settings, 0);
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        engine.compile_module(module)
                    }));
                    match outcome {
                        Ok(Ok(())) => {
                            for (idx, cell) in module.cells.iter().enumerate() {
                                if engine.is_compiled(&cell.name) {
                                    self.compiled.insert(idx);
                                    self.stats.cells_compiled += 1;
                                }
                            }
                            self.engine = Some(engine);
                        }
                        _ => {
                            self.stats.compile_failures += 1;
                        }
                    }
                } else {
                    self.stats.compile_failures += 1;
                }
            }
            if self.compiled.contains(&cell_idx) {
                true
            } else {
                if cell_idx < self.eligibility.len() {
                    self.eligibility[cell_idx] = CellEligibility::NotEligible;
                }
                false
            }
        }

        #[cfg(not(feature = "jit"))]
        {
            let _ = (cell_idx, module);
            false
        }
    }

    /// Execute a JIT-compiled cell with the given i64 arguments.
    ///
    /// Returns `Some((raw, kind))` on success. Returns `None` if the cell is
    /// not compiled or the native code trapped; in the trap case the cell is
    /// permanently demoted to the interpreter, which will re-run the call and
    /// produce the exact interpreter behaviour (error or result).
    #[inline]
    pub fn execute(
        &mut self,
        cell_idx: usize,
        cell_name: &str,
        args: &[i64],
    ) -> Option<(i64, JitRet)> {
        #[cfg(feature = "jit")]
        {
            let engine = self.engine.as_mut()?;
            match engine.execute_jit(cell_name, args) {
                Ok(raw) => {
                    self.stats.jit_executions += 1;
                    let kind = match engine.return_kind(cell_name) {
                        Some(lumen_compiler::codegen::jit::JitReturn::Bool) => JitRet::Bool,
                        Some(lumen_compiler::codegen::jit::JitReturn::Str) => JitRet::Str,
                        _ => JitRet::Int,
                    };
                    Some((raw, kind))
                }
                Err(_) => {
                    self.stats.jit_fallbacks += 1;
                    self.compiled.remove(&cell_idx);
                    if cell_idx < self.eligibility.len() {
                        self.eligibility[cell_idx] = CellEligibility::NotEligible;
                    }
                    None
                }
            }
        }

        #[cfg(not(feature = "jit"))]
        {
            let _ = (cell_idx, cell_name, args);
            None
        }
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

/// Take ownership of a heap-allocated `String` that was produced by a JIT stencil
/// via `Box::into_raw`.  Wraps `lumen_compiler::codegen::jit::jit_take_string` so that call
/// sites in `vm/mod.rs` don't need to reference `lumen_codegen` directly (which
/// would fail to compile when the `jit` feature is disabled).
///
/// # Safety
/// The pointer must have been produced by stencil code through `Box::into_raw::<String>`.
#[cfg(feature = "jit")]
pub(crate) unsafe fn take_jit_string(ptr: i64) -> String {
    lumen_compiler::codegen::jit::jit_take_string(ptr)
}

/// No-op stub used when the `jit` feature is disabled.  The code path that calls
/// this function is guarded by `jit_tier.returns_string()` which returns `false`
/// when JIT is disabled, so this is genuinely unreachable at runtime.
#[cfg(not(feature = "jit"))]
pub(crate) unsafe fn take_jit_string(_ptr: i64) -> String {
    unreachable!("jit feature is not enabled")
}
