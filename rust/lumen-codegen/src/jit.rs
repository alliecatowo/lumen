//! JIT hot path detection and compilation infrastructure.
//!
//! Provides execution profiling to identify frequently-called cells and a
//! `JitEngine` that caches compiled native code for hot functions. The engine
//! observes call counts through `ExecutionProfile` and triggers compilation
//! once a cell crosses the configurable threshold.

use std::collections::HashMap;

use crate::context::CodegenContext;
use crate::emit::{self, CodegenError};
use crate::lower;

use lumen_compiler::compiler::lir::LirModule;

// ---------------------------------------------------------------------------
// Execution profiling
// ---------------------------------------------------------------------------

/// Tracks how many times each cell has been called in the current session.
/// When a cell's call count crosses `threshold`, it is considered "hot"
/// and eligible for JIT compilation.
pub struct ExecutionProfile {
    call_counts: HashMap<String, u64>,
    threshold: u64,
}

impl ExecutionProfile {
    /// Create a new profile with the given hot-call threshold.
    pub fn new(threshold: u64) -> Self {
        Self {
            call_counts: HashMap::new(),
            threshold,
        }
    }

    /// Record a single call to `cell_name`. Returns the new count.
    pub fn record_call(&mut self, cell_name: &str) -> u64 {
        let count = self.call_counts.entry(cell_name.to_string()).or_insert(0);
        *count += 1;
        *count
    }

    /// Returns `true` if the cell's call count exceeds the threshold.
    pub fn is_hot(&self, cell_name: &str) -> bool {
        self.call_counts
            .get(cell_name)
            .map(|&c| c > self.threshold)
            .unwrap_or(false)
    }

    /// Return all cell names whose call count exceeds the threshold.
    pub fn hot_cells(&self) -> Vec<&str> {
        self.call_counts
            .iter()
            .filter(|(_, &c)| c > self.threshold)
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// Reset the counter for a specific cell (e.g. after JIT compilation).
    pub fn reset(&mut self, cell_name: &str) {
        self.call_counts.remove(cell_name);
    }

    /// Get the current call count for a cell.
    pub fn call_count(&self, cell_name: &str) -> u64 {
        self.call_counts.get(cell_name).copied().unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Optimisation level
// ---------------------------------------------------------------------------

/// Optimisation level for JIT compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptLevel {
    /// No optimisation (fastest compile, slowest code).
    None,
    /// Optimise for execution speed.
    Speed,
    /// Optimise for both speed and code size.
    SpeedAndSize,
}

// ---------------------------------------------------------------------------
// Codegen settings
// ---------------------------------------------------------------------------

/// Settings controlling how the JIT engine compiles cells.
pub struct CodegenSettings {
    pub opt_level: OptLevel,
    /// Optional target triple (e.g. `"x86_64-unknown-linux-gnu"`).
    /// If `None`, the host platform is used.
    pub target: Option<String>,
}

impl Default for CodegenSettings {
    fn default() -> Self {
        Self {
            opt_level: OptLevel::Speed,
            target: None,
        }
    }
}

// ---------------------------------------------------------------------------
// JIT statistics
// ---------------------------------------------------------------------------

/// Aggregated statistics about JIT compilation activity.
#[derive(Debug, Clone, Default)]
pub struct JitStats {
    /// Number of cells compiled so far.
    pub cells_compiled: u64,
    /// Number of times a pre-compiled cell was served from cache.
    pub cache_hits: u64,
    /// Number of cache entries currently stored.
    pub cache_size: usize,
}

// ---------------------------------------------------------------------------
// JIT Engine
// ---------------------------------------------------------------------------

/// Manages JIT-compiled function caching and on-demand compilation.
///
/// Typical lifecycle:
/// 1. Interpreter calls `record_and_check("cell_name")` on every cell entry.
/// 2. When the function returns `true` (just became hot), the runtime calls
///    `compile_hot("cell_name", &module)` to compile it.
/// 3. Subsequent invocations call `get_compiled("cell_name")` to retrieve the
///    cached native object code.
pub struct JitEngine {
    profile: ExecutionProfile,
    /// Cached compiled object code keyed by cell name.
    cache: HashMap<String, Vec<u8>>,
    /// Settings for on-demand compilation.
    codegen_settings: CodegenSettings,
    /// Compilation statistics.
    stats: JitStats,
}

impl JitEngine {
    /// Create a new JIT engine. The `threshold` is forwarded to the internal
    /// `ExecutionProfile`.
    pub fn new(settings: CodegenSettings, threshold: u64) -> Self {
        Self {
            profile: ExecutionProfile::new(threshold),
            cache: HashMap::new(),
            codegen_settings: settings,
            stats: JitStats::default(),
        }
    }

    /// Record a call to `cell_name` and return `true` if the cell *just*
    /// crossed the hot threshold (i.e., it was not hot before this call
    /// but now is). This is the trigger for the runtime to schedule JIT
    /// compilation.
    pub fn record_and_check(&mut self, cell_name: &str) -> bool {
        let was_hot = self.profile.is_hot(cell_name);
        self.profile.record_call(cell_name);
        !was_hot && self.profile.is_hot(cell_name)
    }

    /// Compile a single cell from the given `LirModule` to native object
    /// code. The compiled bytes are stored in the cache and also returned.
    ///
    /// If the cell is already cached, the cached bytes are returned (with a
    /// cache-hit bump).
    pub fn compile_hot(
        &mut self,
        cell_name: &str,
        module: &LirModule,
    ) -> Result<Vec<u8>, CodegenError> {
        // Return cached version if available.
        if let Some(cached) = self.cache.get(cell_name) {
            self.stats.cache_hits += 1;
            return Ok(cached.clone());
        }

        // Build a mini-module containing only the target cell (plus any
        // cells it references).  For simplicity we compile all cells in
        // the module; Cranelift will only emit the ones that are reachable.
        let mut ctx = match &self.codegen_settings.target {
            Some(triple) => CodegenContext::new_with_target(triple)?,
            None => CodegenContext::new()?,
        };

        let ptr_ty = ctx.pointer_type();
        let _lowered = lower::lower_module(&mut ctx.module, module, ptr_ty)?;
        let bytes = emit::emit_object(ctx.module)?;

        self.cache.insert(cell_name.to_string(), bytes.clone());
        self.stats.cells_compiled += 1;
        self.stats.cache_size = self.cache.len();

        // Reset the profile counter so we don't re-trigger immediately.
        self.profile.reset(cell_name);

        Ok(bytes)
    }

    /// Retrieve previously compiled native code for a cell, if available.
    pub fn get_compiled(&mut self, cell_name: &str) -> Option<&[u8]> {
        let result = self.cache.get(cell_name);
        if result.is_some() {
            self.stats.cache_hits += 1;
        }
        result.map(|v| v.as_slice())
    }

    /// Remove a cached cell (e.g. when source code changes).
    pub fn invalidate(&mut self, cell_name: &str) {
        self.cache.remove(cell_name);
        self.stats.cache_size = self.cache.len();
    }

    /// Return a snapshot of JIT statistics.
    pub fn stats(&self) -> JitStats {
        self.stats.clone()
    }

    /// Expose the internal execution profile (read-only).
    pub fn profile(&self) -> &ExecutionProfile {
        &self.profile
    }

    /// Check if a cell has been compiled and cached.
    pub fn is_compiled(&self, cell_name: &str) -> bool {
        self.cache.contains_key(cell_name)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use lumen_compiler::compiler::lir::{
        Constant, Instruction, LirCell, LirModule, LirParam, OpCode,
    };

    fn simple_lir_module() -> LirModule {
        LirModule {
            version: "1.0.0".to_string(),
            doc_hash: "test".to_string(),
            strings: Vec::new(),
            types: Vec::new(),
            cells: vec![LirCell {
                name: "answer".to_string(),
                params: Vec::new(),
                returns: Some("Int".to_string()),
                registers: 2,
                constants: vec![Constant::Int(42)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
                effect_handler_metas: Vec::new(),
            }],
            tools: Vec::new(),
            policies: Vec::new(),
            agents: Vec::new(),
            addons: Vec::new(),
            effects: Vec::new(),
            effect_binds: Vec::new(),
            handlers: Vec::new(),
        }
    }

    // --- ExecutionProfile tests -------------------------------------------

    #[test]
    fn profile_starts_empty() {
        let profile = ExecutionProfile::new(100);
        assert_eq!(profile.call_count("foo"), 0);
        assert!(!profile.is_hot("foo"));
        assert!(profile.hot_cells().is_empty());
    }

    #[test]
    fn profile_record_increments() {
        let mut profile = ExecutionProfile::new(3);
        assert_eq!(profile.record_call("foo"), 1);
        assert_eq!(profile.record_call("foo"), 2);
        assert_eq!(profile.record_call("bar"), 1);
        assert_eq!(profile.call_count("foo"), 2);
        assert_eq!(profile.call_count("bar"), 1);
    }

    #[test]
    fn profile_hot_threshold() {
        let mut profile = ExecutionProfile::new(3);
        for _ in 0..3 {
            profile.record_call("fn_a");
        }
        // 3 calls, threshold is 3: not hot yet (need > threshold).
        assert!(!profile.is_hot("fn_a"));

        profile.record_call("fn_a");
        // 4 calls > 3 threshold: now hot.
        assert!(profile.is_hot("fn_a"));
        assert!(!profile.is_hot("fn_b"));
    }

    #[test]
    fn profile_hot_cells() {
        let mut profile = ExecutionProfile::new(2);
        for _ in 0..5 {
            profile.record_call("alpha");
        }
        for _ in 0..3 {
            profile.record_call("beta");
        }
        profile.record_call("gamma"); // only 1 call — not hot

        let mut hot = profile.hot_cells();
        hot.sort();
        assert_eq!(hot, vec!["alpha", "beta"]);
    }

    #[test]
    fn profile_reset() {
        let mut profile = ExecutionProfile::new(2);
        for _ in 0..5 {
            profile.record_call("fn_a");
        }
        assert!(profile.is_hot("fn_a"));

        profile.reset("fn_a");
        assert!(!profile.is_hot("fn_a"));
        assert_eq!(profile.call_count("fn_a"), 0);
    }

    // --- JitEngine tests --------------------------------------------------

    #[test]
    fn engine_record_and_check() {
        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 3);

        // Not hot until > 3 calls.
        assert!(!engine.record_and_check("fn_x"));
        assert!(!engine.record_and_check("fn_x"));
        assert!(!engine.record_and_check("fn_x"));
        // 4th call pushes past threshold — returns true ONCE.
        assert!(engine.record_and_check("fn_x"));
        // Subsequent calls: still hot but already crossed — returns false.
        assert!(!engine.record_and_check("fn_x"));
    }

    #[test]
    fn engine_compile_hot_produces_bytes() {
        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0); // threshold 0: always hot

        let lir = simple_lir_module();
        let bytes = engine
            .compile_hot("answer", &lir)
            .expect("compile_hot should succeed");
        assert!(!bytes.is_empty(), "compiled bytes should not be empty");
        assert!(bytes.len() > 16, "should be a real object file");
    }

    #[test]
    fn engine_cache_hit() {
        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        let lir = simple_lir_module();
        let first = engine.compile_hot("answer", &lir).unwrap();
        let second = engine.compile_hot("answer", &lir).unwrap();
        assert_eq!(first, second, "cached bytes should be identical");

        assert_eq!(engine.stats().cells_compiled, 1);
        assert_eq!(engine.stats().cache_hits, 1);
    }

    #[test]
    fn engine_get_compiled() {
        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        assert!(engine.get_compiled("answer").is_none());

        let lir = simple_lir_module();
        engine.compile_hot("answer", &lir).unwrap();
        assert!(engine.get_compiled("answer").is_some());
    }

    #[test]
    fn engine_invalidate() {
        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        let lir = simple_lir_module();
        engine.compile_hot("answer", &lir).unwrap();
        assert!(engine.is_compiled("answer"));

        engine.invalidate("answer");
        assert!(!engine.is_compiled("answer"));
        assert_eq!(engine.stats().cache_size, 0);
    }

    #[test]
    fn engine_stats_tracking() {
        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        let lir = simple_lir_module();
        let s0 = engine.stats();
        assert_eq!(s0.cells_compiled, 0);
        assert_eq!(s0.cache_hits, 0);
        assert_eq!(s0.cache_size, 0);

        engine.compile_hot("answer", &lir).unwrap();
        let s1 = engine.stats();
        assert_eq!(s1.cells_compiled, 1);
        assert_eq!(s1.cache_size, 1);

        engine.compile_hot("answer", &lir).unwrap(); // cache hit
        let s2 = engine.stats();
        assert_eq!(s2.cells_compiled, 1);
        assert_eq!(s2.cache_hits, 1);
    }

    #[test]
    fn engine_multi_cell_module() {
        let double_cell = LirCell {
            name: "double".to_string(),
            params: vec![LirParam {
                name: "x".to_string(),
                ty: "Int".to_string(),
                register: 0,
                variadic: false,
            }],
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![],
            instructions: vec![
                Instruction::abc(OpCode::Add, 1, 0, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };

        let main_cell = LirCell {
            name: "main".to_string(),
            params: vec![],
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![Constant::String("double".to_string()), Constant::Int(21)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Call, 0, 1, 1),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };

        let lir = LirModule {
            version: "1.0.0".to_string(),
            doc_hash: "test".to_string(),
            strings: Vec::new(),
            types: Vec::new(),
            cells: vec![double_cell, main_cell],
            tools: Vec::new(),
            policies: Vec::new(),
            agents: Vec::new(),
            addons: Vec::new(),
            effects: Vec::new(),
            effect_binds: Vec::new(),
            handlers: Vec::new(),
        };

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        let bytes = engine
            .compile_hot("double", &lir)
            .expect("multi-cell compile should succeed");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn opt_level_variants() {
        // Ensure all OptLevel variants are constructable.
        let _none = OptLevel::None;
        let _speed = OptLevel::Speed;
        let _both = OptLevel::SpeedAndSize;
        assert_ne!(OptLevel::None, OptLevel::Speed);
        assert_ne!(OptLevel::Speed, OptLevel::SpeedAndSize);
    }
}
