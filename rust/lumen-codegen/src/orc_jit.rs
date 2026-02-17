//! OrcJIT engine enhancement (T035).
//!
//! Provides an enhanced JIT engine with module management, function
//! registration, profiling-based hot-function detection, and compilation
//! status tracking.  This complements the Cranelift-backed [`super::jit`]
//! module with a higher-level abstraction suitable for incremental and
//! multi-module compilation workflows.

use std::collections::HashMap;
use std::fmt;

// ---------------------------------------------------------------------------
// Optimisation level
// ---------------------------------------------------------------------------

/// Optimisation level for ORC-style JIT compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JitOptLevel {
    /// No optimisation (fastest compile).
    None,
    /// Optimise for execution speed.
    Speed,
    /// Optimise for both speed and code size.
    SpeedAndSize,
}

// ---------------------------------------------------------------------------
// JIT types
// ---------------------------------------------------------------------------

/// Simplified type representation for JIT functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JitType {
    Void,
    Int64,
    Float64,
    Bool,
    Pointer,
}

impl fmt::Display for JitType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JitType::Void => write!(f, "void"),
            JitType::Int64 => write!(f, "i64"),
            JitType::Float64 => write!(f, "f64"),
            JitType::Bool => write!(f, "bool"),
            JitType::Pointer => write!(f, "ptr"),
        }
    }
}

// ---------------------------------------------------------------------------
// Module status
// ---------------------------------------------------------------------------

/// Status of a JIT module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleStatus {
    /// Module is loaded but no functions have been compiled.
    Loaded,
    /// All functions have been compiled.
    Compiled,
    /// Module has been optimised after compilation.
    Optimized,
    /// Compilation or optimisation failed.
    Failed(String),
}

impl fmt::Display for ModuleStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModuleStatus::Loaded => write!(f, "Loaded"),
            ModuleStatus::Compiled => write!(f, "Compiled"),
            ModuleStatus::Optimized => write!(f, "Optimized"),
            ModuleStatus::Failed(msg) => write!(f, "Failed({msg})"),
        }
    }
}

// ---------------------------------------------------------------------------
// JIT errors
// ---------------------------------------------------------------------------

/// Errors produced by the ORC JIT engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JitError {
    /// A module index was out of range.
    ModuleNotFound(usize),
    /// A function name was not found in any module.
    FunctionNotFound(String),
    /// Compilation of a module failed.
    CompilationFailed(String),
    /// A symbol was already registered under a different module.
    DuplicateSymbol(String),
    /// The module is in an invalid state for the requested operation.
    InvalidModule(String),
}

impl fmt::Display for JitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JitError::ModuleNotFound(idx) => write!(f, "module not found: index {idx}"),
            JitError::FunctionNotFound(name) => write!(f, "function not found: {name}"),
            JitError::CompilationFailed(msg) => write!(f, "compilation failed: {msg}"),
            JitError::DuplicateSymbol(name) => write!(f, "duplicate symbol: {name}"),
            JitError::InvalidModule(msg) => write!(f, "invalid module: {msg}"),
        }
    }
}

impl std::error::Error for JitError {}

// ---------------------------------------------------------------------------
// JIT function
// ---------------------------------------------------------------------------

/// Metadata for a single JIT-managed function.
#[derive(Debug, Clone)]
pub struct JitFunction {
    /// Function name (must be unique across all modules).
    pub name: String,
    /// Number of parameters.
    pub param_count: usize,
    /// Return type.
    pub return_type: JitType,
    /// Whether the function has been compiled to native code.
    pub is_compiled: bool,
    /// Number of times this function has been called (for profiling).
    pub call_count: u64,
}

impl JitFunction {
    /// Create a new uncompiled function with zero calls.
    pub fn new(name: &str, param_count: usize, return_type: JitType) -> Self {
        Self {
            name: name.to_string(),
            param_count,
            return_type,
            is_compiled: false,
            call_count: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// JIT module
// ---------------------------------------------------------------------------

/// A named collection of JIT functions.
#[derive(Debug, Clone)]
pub struct JitModule {
    /// Module name.
    pub name: String,
    /// Functions belonging to this module.
    pub functions: Vec<JitFunction>,
    /// Current status.
    pub status: ModuleStatus,
}

impl JitModule {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            functions: Vec::new(),
            status: ModuleStatus::Loaded,
        }
    }
}

// ---------------------------------------------------------------------------
// OrcJitEngine
// ---------------------------------------------------------------------------

/// Enhanced JIT engine with module management, symbol tracking, and profiling.
pub struct OrcJitEngine {
    modules: Vec<Option<JitModule>>,
    symbols: HashMap<String, usize>,
    opt_level: JitOptLevel,
    compiled_count: usize,
}

impl OrcJitEngine {
    /// Create a new engine with the given optimisation level.
    pub fn new(opt_level: JitOptLevel) -> Self {
        Self {
            modules: Vec::new(),
            symbols: HashMap::new(),
            opt_level,
            compiled_count: 0,
        }
    }

    /// Return the optimisation level.
    pub fn opt_level(&self) -> JitOptLevel {
        self.opt_level
    }

    // -- Module management -------------------------------------------------

    /// Add a new module and return its index.
    pub fn add_module(&mut self, name: &str) -> usize {
        let idx = self.modules.len();
        self.modules.push(Some(JitModule::new(name)));
        idx
    }

    /// Remove a module by index, returning `true` if it existed.
    /// All symbols belonging to the module are also removed.
    pub fn remove_module(&mut self, index: usize) -> bool {
        if index >= self.modules.len() {
            return false;
        }
        if self.modules[index].is_none() {
            return false;
        }

        // Remove symbols that belong to this module.
        let module = self.modules[index].take().unwrap();
        for func in &module.functions {
            if func.is_compiled {
                self.compiled_count = self.compiled_count.saturating_sub(1);
            }
            self.symbols.remove(&func.name);
        }
        true
    }

    /// Get an immutable reference to a module by index.
    pub fn get_module(&self, index: usize) -> Option<&JitModule> {
        self.modules.get(index).and_then(|m| m.as_ref())
    }

    // -- Function registration ---------------------------------------------

    /// Register a function in the given module.
    pub fn register_function(&mut self, module: usize, func: JitFunction) -> Result<(), JitError> {
        // Check module exists.
        if module >= self.modules.len() || self.modules[module].is_none() {
            return Err(JitError::ModuleNotFound(module));
        }

        // Check for duplicate symbol.
        if let Some(&existing_mod) = self.symbols.get(&func.name) {
            if existing_mod != module {
                return Err(JitError::DuplicateSymbol(func.name.clone()));
            }
            // Same module — check if already present.
            let m = self.modules[module].as_ref().unwrap();
            if m.functions.iter().any(|f| f.name == func.name) {
                return Err(JitError::DuplicateSymbol(func.name.clone()));
            }
        }

        self.symbols.insert(func.name.clone(), module);
        if func.is_compiled {
            self.compiled_count = self.compiled_count.saturating_add(1);
        }
        let m = self.modules[module].as_mut().unwrap();
        m.functions.push(func);
        Ok(())
    }

    /// Look up a function by name across all modules.
    pub fn lookup_function(&self, name: &str) -> Option<(&JitModule, &JitFunction)> {
        let &mod_idx = self.symbols.get(name)?;
        let module = self.modules[mod_idx].as_ref()?;
        let func = module.functions.iter().find(|f| f.name == name)?;
        Some((module, func))
    }

    // -- Compilation -------------------------------------------------------

    /// Compile all functions in a module.
    pub fn compile_module(&mut self, index: usize) -> Result<(), JitError> {
        let module = self
            .modules
            .get(index)
            .and_then(|m| m.as_ref())
            .ok_or(JitError::ModuleNotFound(index))?;

        if matches!(module.status, ModuleStatus::Failed(_)) {
            return Err(JitError::InvalidModule(format!(
                "module '{}' is in a failed state",
                module.name
            )));
        }

        let opt_level = self.opt_level;

        let module = self.modules[index].as_mut().unwrap();
        let mut newly_compiled = 0usize;
        for func in &mut module.functions {
            if !func.is_compiled {
                func.is_compiled = true;
                newly_compiled = newly_compiled.saturating_add(1);
            }
        }
        self.compiled_count = self.compiled_count.saturating_add(newly_compiled);

        let module = self.modules[index].as_mut().unwrap();
        module.status = match opt_level {
            JitOptLevel::None => ModuleStatus::Compiled,
            _ => ModuleStatus::Optimized,
        };

        Ok(())
    }

    /// Compile all modules. Returns one result per module.
    pub fn compile_all(&mut self) -> Vec<Result<(), JitError>> {
        let indices: Vec<usize> = (0..self.modules.len())
            .filter(|&i| self.modules[i].is_some())
            .collect();

        let mut results = Vec::with_capacity(indices.len());
        for idx in indices {
            results.push(self.compile_module(idx));
        }
        results
    }

    // -- Profiling ---------------------------------------------------------

    /// Record a call to a function, incrementing its call count.
    pub fn record_call(&mut self, name: &str) {
        if let Some(&mod_idx) = self.symbols.get(name) {
            if let Some(module) = self.modules[mod_idx].as_mut() {
                if let Some(func) = module.functions.iter_mut().find(|f| f.name == name) {
                    func.call_count = func.call_count.saturating_add(1);
                }
            }
        }
    }

    /// Return functions whose call count exceeds the given threshold.
    pub fn hot_functions(&self, threshold: u64) -> Vec<&JitFunction> {
        self.modules
            .iter()
            .filter_map(|m| m.as_ref())
            .flat_map(|m| m.functions.iter())
            .filter(|f| f.call_count > threshold)
            .collect()
    }

    // -- Statistics --------------------------------------------------------

    /// Total number of registered functions across all modules.
    pub fn total_functions(&self) -> usize {
        self.modules
            .iter()
            .filter_map(|m| m.as_ref())
            .map(|m| m.functions.len())
            .sum()
    }

    /// Number of compiled functions.
    pub fn compiled_functions(&self) -> usize {
        self.compiled_count
    }

    /// Number of live (non-removed) modules.
    pub fn module_count(&self) -> usize {
        self.modules.iter().filter(|m| m.is_some()).count()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_new_defaults() {
        let engine = OrcJitEngine::new(JitOptLevel::Speed);
        assert_eq!(engine.opt_level(), JitOptLevel::Speed);
        assert_eq!(engine.module_count(), 0);
        assert_eq!(engine.total_functions(), 0);
        assert_eq!(engine.compiled_functions(), 0);
    }

    #[test]
    fn add_and_get_module() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let idx = engine.add_module("test_mod");
        let m = engine.get_module(idx).unwrap();
        assert_eq!(m.name, "test_mod");
        assert_eq!(m.status, ModuleStatus::Loaded);
        assert!(m.functions.is_empty());
    }

    #[test]
    fn remove_module() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let idx = engine.add_module("m");
        assert!(engine.remove_module(idx));
        assert!(engine.get_module(idx).is_none());
        assert_eq!(engine.module_count(), 0);
    }

    #[test]
    fn remove_nonexistent_module() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        assert!(!engine.remove_module(99));
    }

    #[test]
    fn remove_already_removed_module() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let idx = engine.add_module("m");
        assert!(engine.remove_module(idx));
        assert!(!engine.remove_module(idx));
    }

    #[test]
    fn register_function_success() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let idx = engine.add_module("mod");
        let func = JitFunction::new("foo", 2, JitType::Int64);
        assert!(engine.register_function(idx, func).is_ok());
        assert_eq!(engine.total_functions(), 1);
    }

    #[test]
    fn register_duplicate_symbol_different_module() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let m1 = engine.add_module("m1");
        let m2 = engine.add_module("m2");
        engine
            .register_function(m1, JitFunction::new("foo", 0, JitType::Void))
            .unwrap();
        let err = engine
            .register_function(m2, JitFunction::new("foo", 0, JitType::Void))
            .unwrap_err();
        assert_eq!(err, JitError::DuplicateSymbol("foo".into()));
    }

    #[test]
    fn register_duplicate_symbol_same_module() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let m = engine.add_module("m");
        engine
            .register_function(m, JitFunction::new("foo", 0, JitType::Void))
            .unwrap();
        let err = engine
            .register_function(m, JitFunction::new("foo", 0, JitType::Void))
            .unwrap_err();
        assert_eq!(err, JitError::DuplicateSymbol("foo".into()));
    }

    #[test]
    fn register_function_invalid_module() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let err = engine
            .register_function(42, JitFunction::new("f", 0, JitType::Void))
            .unwrap_err();
        assert_eq!(err, JitError::ModuleNotFound(42));
    }

    #[test]
    fn lookup_function_found() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let m = engine.add_module("mod");
        engine
            .register_function(m, JitFunction::new("bar", 1, JitType::Float64))
            .unwrap();
        let (module, func) = engine.lookup_function("bar").unwrap();
        assert_eq!(module.name, "mod");
        assert_eq!(func.name, "bar");
        assert_eq!(func.param_count, 1);
        assert_eq!(func.return_type, JitType::Float64);
    }

    #[test]
    fn lookup_function_not_found() {
        let engine = OrcJitEngine::new(JitOptLevel::None);
        assert!(engine.lookup_function("nonexistent").is_none());
    }

    #[test]
    fn compile_module_marks_functions_compiled() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let m = engine.add_module("m");
        engine
            .register_function(m, JitFunction::new("a", 0, JitType::Void))
            .unwrap();
        engine
            .register_function(m, JitFunction::new("b", 1, JitType::Int64))
            .unwrap();

        engine.compile_module(m).unwrap();

        let module = engine.get_module(m).unwrap();
        assert_eq!(module.status, ModuleStatus::Compiled);
        assert!(module.functions.iter().all(|f| f.is_compiled));
        assert_eq!(engine.compiled_functions(), 2);
    }

    #[test]
    fn compile_module_optimized_when_opt_level_speed() {
        let mut engine = OrcJitEngine::new(JitOptLevel::Speed);
        let m = engine.add_module("m");
        engine
            .register_function(m, JitFunction::new("f", 0, JitType::Void))
            .unwrap();
        engine.compile_module(m).unwrap();
        assert_eq!(
            engine.get_module(m).unwrap().status,
            ModuleStatus::Optimized
        );
    }

    #[test]
    fn compile_module_not_found() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        assert_eq!(
            engine.compile_module(999).unwrap_err(),
            JitError::ModuleNotFound(999)
        );
    }

    #[test]
    fn compile_failed_module_errors() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let m = engine.add_module("bad");
        // Directly set the module status to Failed via the internal vec.
        engine.modules[m].as_mut().unwrap().status = ModuleStatus::Failed("broken".into());
        let err = engine.compile_module(m).unwrap_err();
        assert!(matches!(err, JitError::InvalidModule(_)));
    }

    #[test]
    fn compile_all_multiple_modules() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let m1 = engine.add_module("m1");
        let m2 = engine.add_module("m2");
        engine
            .register_function(m1, JitFunction::new("f1", 0, JitType::Void))
            .unwrap();
        engine
            .register_function(m2, JitFunction::new("f2", 0, JitType::Void))
            .unwrap();

        let results = engine.compile_all();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.is_ok()));
        assert_eq!(engine.compiled_functions(), 2);
    }

    #[test]
    fn record_call_increments_count() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let m = engine.add_module("m");
        engine
            .register_function(m, JitFunction::new("fn1", 0, JitType::Void))
            .unwrap();
        engine.record_call("fn1");
        engine.record_call("fn1");
        engine.record_call("fn1");
        let (_, func) = engine.lookup_function("fn1").unwrap();
        assert_eq!(func.call_count, 3);
    }

    #[test]
    fn record_call_unknown_function_no_op() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        engine.record_call("nonexistent"); // should not panic
    }

    #[test]
    fn hot_functions_threshold() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let m = engine.add_module("m");
        engine
            .register_function(m, JitFunction::new("cold", 0, JitType::Void))
            .unwrap();
        engine
            .register_function(m, JitFunction::new("hot", 0, JitType::Void))
            .unwrap();
        for _ in 0..10 {
            engine.record_call("hot");
        }
        engine.record_call("cold");

        let hot = engine.hot_functions(5);
        assert_eq!(hot.len(), 1);
        assert_eq!(hot[0].name, "hot");
    }

    #[test]
    fn hot_functions_empty_when_none_hot() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let m = engine.add_module("m");
        engine
            .register_function(m, JitFunction::new("f", 0, JitType::Void))
            .unwrap();
        assert!(engine.hot_functions(100).is_empty());
    }

    #[test]
    fn total_functions_across_modules() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let m1 = engine.add_module("m1");
        let m2 = engine.add_module("m2");
        engine
            .register_function(m1, JitFunction::new("a", 0, JitType::Void))
            .unwrap();
        engine
            .register_function(m1, JitFunction::new("b", 0, JitType::Void))
            .unwrap();
        engine
            .register_function(m2, JitFunction::new("c", 0, JitType::Void))
            .unwrap();
        assert_eq!(engine.total_functions(), 3);
    }

    #[test]
    fn module_count_reflects_removals() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        engine.add_module("a");
        let b = engine.add_module("b");
        engine.add_module("c");
        assert_eq!(engine.module_count(), 3);
        engine.remove_module(b);
        assert_eq!(engine.module_count(), 2);
    }

    #[test]
    fn compiled_functions_decrements_on_module_remove() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let m = engine.add_module("m");
        engine
            .register_function(m, JitFunction::new("f", 0, JitType::Void))
            .unwrap();
        engine.compile_module(m).unwrap();
        assert_eq!(engine.compiled_functions(), 1);
        engine.remove_module(m);
        assert_eq!(engine.compiled_functions(), 0);
    }

    #[test]
    fn jit_type_display() {
        assert_eq!(format!("{}", JitType::Void), "void");
        assert_eq!(format!("{}", JitType::Int64), "i64");
        assert_eq!(format!("{}", JitType::Float64), "f64");
        assert_eq!(format!("{}", JitType::Bool), "bool");
        assert_eq!(format!("{}", JitType::Pointer), "ptr");
    }

    #[test]
    fn module_status_display() {
        assert_eq!(format!("{}", ModuleStatus::Loaded), "Loaded");
        assert_eq!(format!("{}", ModuleStatus::Compiled), "Compiled");
        assert_eq!(format!("{}", ModuleStatus::Optimized), "Optimized");
        assert_eq!(
            format!("{}", ModuleStatus::Failed("oops".into())),
            "Failed(oops)"
        );
    }

    #[test]
    fn jit_error_display() {
        assert_eq!(
            format!("{}", JitError::ModuleNotFound(3)),
            "module not found: index 3"
        );
        assert_eq!(
            format!("{}", JitError::FunctionNotFound("foo".into())),
            "function not found: foo"
        );
        assert_eq!(
            format!("{}", JitError::CompilationFailed("bad ir".into())),
            "compilation failed: bad ir"
        );
        assert_eq!(
            format!("{}", JitError::DuplicateSymbol("dup".into())),
            "duplicate symbol: dup"
        );
        assert_eq!(
            format!("{}", JitError::InvalidModule("wrong state".into())),
            "invalid module: wrong state"
        );
    }

    #[test]
    fn jit_function_new_defaults() {
        let f = JitFunction::new("test", 3, JitType::Int64);
        assert_eq!(f.name, "test");
        assert_eq!(f.param_count, 3);
        assert_eq!(f.return_type, JitType::Int64);
        assert!(!f.is_compiled);
        assert_eq!(f.call_count, 0);
    }

    #[test]
    fn pre_compiled_function_counted() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let m = engine.add_module("m");
        let mut f = JitFunction::new("f", 0, JitType::Void);
        f.is_compiled = true;
        engine.register_function(m, f).unwrap();
        assert_eq!(engine.compiled_functions(), 1);
    }

    #[test]
    fn compile_idempotent() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let m = engine.add_module("m");
        engine
            .register_function(m, JitFunction::new("f", 0, JitType::Void))
            .unwrap();
        engine.compile_module(m).unwrap();
        engine.compile_module(m).unwrap(); // second compile should not double-count
        assert_eq!(engine.compiled_functions(), 1);
    }

    #[test]
    fn lookup_after_remove_returns_none() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let m = engine.add_module("m");
        engine
            .register_function(m, JitFunction::new("f", 0, JitType::Void))
            .unwrap();
        engine.remove_module(m);
        assert!(engine.lookup_function("f").is_none());
    }

    #[test]
    fn opt_level_variants() {
        assert_ne!(JitOptLevel::None, JitOptLevel::Speed);
        assert_ne!(JitOptLevel::Speed, JitOptLevel::SpeedAndSize);
        assert_ne!(JitOptLevel::None, JitOptLevel::SpeedAndSize);
    }

    #[test]
    fn multiple_modules_independent_compilation() {
        let mut engine = OrcJitEngine::new(JitOptLevel::Speed);
        let m1 = engine.add_module("alpha");
        let m2 = engine.add_module("beta");

        engine
            .register_function(m1, JitFunction::new("a1", 0, JitType::Void))
            .unwrap();
        engine
            .register_function(m2, JitFunction::new("b1", 0, JitType::Void))
            .unwrap();

        engine.compile_module(m1).unwrap();
        assert_eq!(
            engine.get_module(m1).unwrap().status,
            ModuleStatus::Optimized
        );
        assert_eq!(engine.get_module(m2).unwrap().status, ModuleStatus::Loaded);
    }

    #[test]
    fn register_in_removed_module_fails() {
        let mut engine = OrcJitEngine::new(JitOptLevel::None);
        let m = engine.add_module("m");
        engine.remove_module(m);
        let err = engine
            .register_function(m, JitFunction::new("f", 0, JitType::Void))
            .unwrap_err();
        assert_eq!(err, JitError::ModuleNotFound(m));
    }
}
