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
//! Eligible cells must have all-Int parameters and an Int return type (the
//! current Cranelift backend operates on i64 values only). Non-eligible cells
//! remain interpreted forever.

#[cfg(feature = "jit")]
use lumen_codegen::jit::{CodegenSettings, JitEngine, JitStats, OptLevel};
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

/// Eligibility status for a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CellEligibility {
    /// Not yet checked.
    Unknown,
    /// Eligible for JIT compilation (all-Int params, Int return).
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
            eligibility: Vec::new(),
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
        self.eligibility.resize(num_cells, CellEligibility::Unknown);
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

    /// Check and cache JIT eligibility for a cell.
    /// A cell is eligible if ALL params have type "Int" and return type is "Int".
    pub fn check_eligibility(&mut self, cell_idx: usize, module: &LirModule) -> bool {
        if cell_idx >= self.eligibility.len() {
            return false;
        }
        match self.eligibility[cell_idx] {
            CellEligibility::Eligible => true,
            CellEligibility::NotEligible => false,
            CellEligibility::Unknown => {
                let cell = &module.cells[cell_idx];
                let all_int_params = cell.params.iter().all(|p| p.ty == "Int");
                let returns_int = cell.returns.as_deref() == Some("Int");
                let eligible = all_int_params && returns_int;
                self.eligibility[cell_idx] = if eligible {
                    CellEligibility::Eligible
                } else {
                    CellEligibility::NotEligible
                };
                eligible
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

    /// Attempt to compile a hot cell. Returns `true` on success.
    ///
    /// On the `jit` feature, this creates/updates the Cranelift JIT engine and
    /// compiles all Int-eligible cells from the module. Non-Int cells are
    /// filtered out so mixed-type modules can still benefit from JIT for their
    /// Int-only hot paths.
    ///
    /// On no-jit builds, this is a no-op that returns `false`.
    pub fn try_compile(&mut self, _cell_idx: usize, module: &LirModule) -> bool {
        #[cfg(feature = "jit")]
        {
            // Build a filtered module containing only Int-eligible cells.
            // This allows JIT compilation of hot Int-only cells even when the
            // module contains cells using strings, records, etc.
            let int_cells: Vec<_> = module
                .cells
                .iter()
                .filter(|c| {
                    let params_ok = c.params.iter().all(|p| p.ty == "Int");
                    let ret_ok = c.returns.as_deref() == Some("Int");
                    params_ok && ret_ok
                })
                .cloned()
                .collect();

            if int_cells.is_empty() {
                self.stats.compile_failures += 1;
                return false;
            }

            // Check that the target cell is in the filtered set.
            let target_name = &module.cells[_cell_idx].name;
            if !int_cells.iter().any(|c| c.name == *target_name) {
                if _cell_idx < self.eligibility.len() {
                    self.eligibility[_cell_idx] = CellEligibility::NotEligible;
                }
                self.stats.compile_failures += 1;
                return false;
            }

            // Create a minimal LirModule with only Int-eligible cells.
            let filtered_module = LirModule {
                version: module.version.clone(),
                cells: int_cells,
                strings: module.strings.clone(),
                types: module.types.clone(),
                tools: Vec::new(),
                policies: Vec::new(),
                agents: Vec::new(),
                handlers: Vec::new(),
                addons: Vec::new(),
                effects: Vec::new(),
                effect_binds: Vec::new(),
                doc_hash: module.doc_hash.clone(),
            };

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
            match engine.compile_module(&filtered_module) {
                Ok(()) => {
                    // Mark ALL Int-eligible cells as compiled. We need to map
                    // back to the original module indices.
                    for (idx, cell) in module.cells.iter().enumerate() {
                        let params_ok = cell.params.iter().all(|p| p.ty == "Int");
                        let ret_ok = cell.returns.as_deref() == Some("Int");
                        if params_ok && ret_ok {
                            self.compiled.insert(idx);
                            self.stats.cells_compiled += 1;
                        }
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
    pub fn execute(&mut self, cell_name: &str, args: &[i64]) -> Option<i64> {
        #[cfg(feature = "jit")]
        {
            if let Some(ref mut engine) = self.engine {
                match engine.execute_jit(cell_name, args) {
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
