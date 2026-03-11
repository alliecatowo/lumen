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
use lumen_core::lir::LirModule;
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
    /// Roots that have crossed the hot threshold and must remain in the
    /// rebuilt engine when new hot cells are compiled.
    hot_roots: HashSet<usize>,
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
            hot_roots: HashSet::new(),
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
        self.hot_roots.clear();
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

    /// Attempt to compile a hot cell. Returns `true` on success.
    ///
    /// On the `jit` feature, this creates/updates the Cranelift JIT engine and
    /// compiles all cells from the module. Cells with unsupported opcodes will
    /// cause compilation to fail gracefully, falling back to the interpreter.
    ///
    /// On no-jit builds, this is a no-op that returns `false`.
    pub fn try_compile(&mut self, _cell_idx: usize, module: &LirModule) -> bool {
        #[cfg(feature = "jit")]
        {
            if module.cells.is_empty() {
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

            // Create a new engine each time (Cranelift JITModule doesn't support
            // incremental addition of functions after finalize_definitions).
            let mut engine = JitEngine::new(settings, 0);
            let Some(_) = module.cells.get(_cell_idx) else {
                self.stats.compile_failures += 1;
                return false;
            };

            let mut hot_roots = self.hot_roots.iter().copied().collect::<Vec<_>>();
            hot_roots.push(_cell_idx);
            hot_roots.sort_unstable();
            hot_roots.dedup();
            let hot_root_names = hot_roots
                .iter()
                .map(|idx| module.cells[*idx].name.clone())
                .collect::<Vec<_>>();

            match engine.compile_hot_roots(&hot_root_names, module) {
                Ok(()) => {
                    self.hot_roots = hot_roots.into_iter().collect();
                    self.compiled.clear();
                    // Only mark cells that were actually compiled for the hot
                    // cell's dependency closure.
                    for (idx, cell) in module.cells.iter().enumerate() {
                        if engine.is_compiled(&cell.name) {
                            self.compiled.insert(idx);
                        }
                    }
                    self.stats.cells_compiled = self.compiled.len() as u64;
                    self.engine = Some(engine);
                    true
                }
                Err(_e) => {
                    // Compilation failed — mark the target cell as not eligible
                    // so we don't retry it.
                    if _cell_idx < self.eligibility.len() {
                        self.eligibility[_cell_idx] = CellEligibility::NotEligible;
                    }
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

/// Wrapper for `lumen_codegen::jit::jit_take_string` that compiles away when
/// the `jit` feature is disabled — prevents wasm32 build errors.
#[cfg(feature = "jit")]
pub(crate) unsafe fn take_jit_string(ptr: i64) -> String {
    lumen_codegen::jit::jit_take_string(ptr)
}

#[cfg(not(feature = "jit"))]
pub(crate) unsafe fn take_jit_string(_ptr: i64) -> String {
    unreachable!("jit feature is not enabled")
}

#[cfg(all(test, feature = "jit", target_arch = "x86_64"))]
mod tests {
    use super::{JitOptLevel, JitTier, JitTierConfig};
    use lumen_core::lir::{Constant, Instruction, LirCell, LirModule, LirParam, OpCode};

    fn make_module_with_cells(cells: Vec<LirCell>) -> LirModule {
        LirModule {
            version: "1.0.0".to_string(),
            doc_hash: "test".to_string(),
            strings: Vec::new(),
            types: Vec::new(),
            cells,
            tools: Vec::new(),
            policies: Vec::new(),
            agents: Vec::new(),
            addons: Vec::new(),
            effects: Vec::new(),
            effect_binds: Vec::new(),
            handlers: Vec::new(),
        }
    }

    #[test]
    fn try_compile_marks_only_hot_cell_dependency_closure() {
        let root = LirCell {
            name: "root".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![Constant::String("child".to_string()), Constant::Int(21)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Call, 0, 1, 1),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };
        let child = LirCell {
            name: "child".to_string(),
            params: vec![LirParam {
                name: "x".to_string(),
                ty: "Int".to_string(),
                register: 0,
                variadic: false,
            }],
            returns: Some("Int".to_string()),
            registers: 3,
            constants: vec![Constant::Int(2)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 1, 0),
                Instruction::abc(OpCode::Mul, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };
        let unrelated = LirCell {
            name: "unrelated".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 2,
            constants: vec![Constant::Int(99)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };
        let module = make_module_with_cells(vec![root, child, unrelated]);

        let mut tier = JitTier::new(JitTierConfig {
            hot_threshold: 0,
            opt_level: JitOptLevel::Speed,
            enabled: true,
        });
        tier.init_for_module(module.cells.len());

        assert!(tier.try_compile(0, &module));
        assert!(tier.is_compiled(0));
        assert!(tier.is_compiled(1));
        assert!(
            !tier.is_compiled(2),
            "runtime tier should not mark unrelated cells as compiled"
        );

        let stats = tier.tier_stats();
        assert_eq!(stats.cells_compiled, 2);
        assert_eq!(stats.compile_failures, 0);
    }

    #[test]
    fn try_compile_preserves_existing_hot_roots_across_engine_rebuilds() {
        let left_root = LirCell {
            name: "left_root".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 3,
            constants: vec![Constant::String("left_leaf".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Call, 0, 0, 1),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };
        let left_leaf = LirCell {
            name: "left_leaf".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 2,
            constants: vec![Constant::Int(11)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };
        let right_root = LirCell {
            name: "right_root".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 3,
            constants: vec![Constant::String("right_leaf".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Call, 0, 0, 1),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };
        let right_leaf = LirCell {
            name: "right_leaf".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 2,
            constants: vec![Constant::Int(29)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };
        let module = make_module_with_cells(vec![left_root, left_leaf, right_root, right_leaf]);

        let mut tier = JitTier::new(JitTierConfig {
            hot_threshold: 0,
            opt_level: JitOptLevel::Speed,
            enabled: true,
        });
        tier.init_for_module(module.cells.len());

        assert!(tier.try_compile(0, &module));
        assert_eq!(tier.execute("left_root", &[]), Some(11));

        assert!(tier.try_compile(2, &module));
        assert_eq!(tier.execute("left_root", &[]), Some(11));
        assert_eq!(tier.execute("right_root", &[]), Some(29));
        assert!(tier.is_compiled(0));
        assert!(tier.is_compiled(1));
        assert!(tier.is_compiled(2));
        assert!(tier.is_compiled(3));
        assert_eq!(tier.tier_stats().cells_compiled, 4);
    }
}
