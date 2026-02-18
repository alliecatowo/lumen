//! JIT hot-path detection and in-process native code execution.
//!
//! Provides execution profiling to identify frequently-called cells and a
//! `JitEngine` that compiles LIR to native machine code via Cranelift's JIT
//! backend, then executes the compiled functions directly as native function
//! pointers.
//!
//! The engine observes call counts through `ExecutionProfile` and triggers
//! compilation once a cell crosses the configurable threshold. Compiled
//! functions are cached as callable function pointers — subsequent calls
//! bypass the interpreter entirely.

use cranelift_codegen::ir::{types, AbiParam, Type as ClifType};
use cranelift_codegen::Context;
use cranelift_frontend::FunctionBuilderContext;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use std::collections::HashMap;

use lumen_core::lir::{LirCell, LirModule, OpCode};

use crate::emit::CodegenError;
use crate::types::lir_type_str_to_cl_type;

// ---------------------------------------------------------------------------
// String runtime helpers (extern "C" functions callable from JIT code)
// ---------------------------------------------------------------------------

/// Allocate a new heap `String` from a raw UTF-8 byte pointer and length.
/// Returns a `*mut String` as i64.
///
/// # Safety
/// `ptr` must point to valid UTF-8 bytes of at least `len` bytes.
extern "C" fn jit_rt_string_alloc(ptr: *const u8, len: usize) -> i64 {
    let s = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(ptr, len)) };
    let boxed = Box::new(s.to_string());
    Box::into_raw(boxed) as i64
}

/// Concatenate two heap strings. Both inputs are `*mut String` as i64.
/// Returns a new `*mut String` as i64 owning the concatenated result.
/// The input strings are NOT freed (callers manage lifetimes).
///
/// # Safety
/// Both `a` and `b` must be valid `*mut String` pointers.
extern "C" fn jit_rt_string_concat(a: i64, b: i64) -> i64 {
    let sa = unsafe { &*(a as *const String) };
    let sb = unsafe { &*(b as *const String) };
    let mut result = String::with_capacity(sa.len() + sb.len());
    result.push_str(sa);
    result.push_str(sb);
    let boxed = Box::new(result);
    Box::into_raw(boxed) as i64
}

/// Concatenate two heap strings with in-place optimization.
/// Takes ownership of `a`, appends `b` to it, and returns the modified `a`.
/// The string `a` is consumed and must not be used afterward.
/// The string `b` is borrowed (not consumed).
///
/// This is used for the pattern `a = a + b` where we can reuse `a`'s allocation.
///
/// # Safety
/// Both `a` and `b` must be valid `*mut String` pointers.
/// After this call, `a` is consumed and the returned pointer should be used instead.
extern "C" fn jit_rt_string_concat_mut(a: i64, b: i64) -> i64 {
    let mut boxed_a = unsafe { Box::from_raw(a as *mut String) };
    let sb = unsafe { &*(b as *const String) };

    // Append b to a in-place (will reallocate if needed, but may reuse capacity)
    boxed_a.push_str(sb);

    Box::into_raw(boxed_a) as i64
}

/// Clone a heap string. Input is `*mut String` as i64.
/// Returns a new `*mut String` as i64.
///
/// # Safety
/// `s` must be a valid `*mut String` pointer.
extern "C" fn jit_rt_string_clone(s: i64) -> i64 {
    let original = unsafe { &*(s as *const String) };
    let boxed = Box::new(original.clone());
    Box::into_raw(boxed) as i64
}

/// Compare two heap strings for equality. Returns 1 if equal, 0 if not.
///
/// # Safety
/// Both `a` and `b` must be valid `*mut String` pointers.
extern "C" fn jit_rt_string_eq(a: i64, b: i64) -> i64 {
    let sa = unsafe { &*(a as *const String) };
    let sb = unsafe { &*(b as *const String) };
    if sa == sb {
        1
    } else {
        0
    }
}

/// Compare two heap strings, returning -1/0/1 for less/equal/greater.
///
/// # Safety
/// Both `a` and `b` must be valid `*mut String` pointers.
extern "C" fn jit_rt_string_cmp(a: i64, b: i64) -> i64 {
    let sa = unsafe { &*(a as *const String) };
    let sb = unsafe { &*(b as *const String) };
    match sa.cmp(sb) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// Free a heap string. Input is `*mut String` as i64.
/// Call this when a string value is no longer needed.
///
/// # Safety
/// `s` must be a valid `*mut String` pointer that was created by one of the
/// `jit_rt_string_*` functions. Must not be called twice on the same pointer.
extern "C" fn jit_rt_string_drop(s: i64) {
    if s != 0 {
        unsafe {
            let _ = Box::from_raw(s as *mut String);
        }
    }
}

/// Reconstruct a `String` from a JIT-returned raw pointer.
///
/// # Safety
/// `ptr` must be a valid `*mut String` pointer created by `jit_rt_string_alloc`,
/// `jit_rt_string_concat`, or `jit_rt_string_clone`. After this call the pointer
/// is consumed and must not be used again.
pub unsafe fn jit_take_string(ptr: i64) -> String {
    if ptr == 0 {
        String::new()
    } else {
        *Box::from_raw(ptr as *mut String)
    }
}

/// Register all JIT string runtime helper symbols with a JITBuilder.
fn register_string_helpers(builder: &mut JITBuilder) {
    builder.symbol("jit_rt_string_alloc", jit_rt_string_alloc as *const u8);
    builder.symbol("jit_rt_string_concat", jit_rt_string_concat as *const u8);
    builder.symbol(
        "jit_rt_string_concat_mut",
        jit_rt_string_concat_mut as *const u8,
    );
    builder.symbol("jit_rt_string_clone", jit_rt_string_clone as *const u8);
    builder.symbol("jit_rt_string_eq", jit_rt_string_eq as *const u8);
    builder.symbol("jit_rt_string_cmp", jit_rt_string_cmp as *const u8);
    builder.symbol("jit_rt_string_drop", jit_rt_string_drop as *const u8);
}

// ---------------------------------------------------------------------------
// Record runtime helpers (extern "C" functions callable from JIT code)
// ---------------------------------------------------------------------------

use lumen_core::values::{RecordValue, Value};

/// Get a field from a Record by field name.
/// Returns a `*mut Value` as i64 (boxed Value).
/// If the record is null or the field doesn't exist, returns a boxed Value::Null.
///
/// # Safety
/// `record_ptr` must be a valid `*mut Value` pointer pointing to a `Value::Record`.
/// `field_name_ptr` must be a valid `*const u8` pointer to UTF-8 bytes.
extern "C" fn jit_rt_record_get_field(
    record_ptr: i64,
    field_name_ptr: *const u8,
    field_name_len: usize,
) -> i64 {
    if record_ptr == 0 {
        // Null record, return boxed null
        return Box::into_raw(Box::new(Value::Null)) as i64;
    }

    let value = unsafe { &*(record_ptr as *const Value) };
    let field_name = unsafe {
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(field_name_ptr, field_name_len))
    };

    let result = match value {
        Value::Record(r) => r.fields.get(field_name).cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    };

    Box::into_raw(Box::new(result)) as i64
}

/// Set a field in a Record by field name.
/// Creates a new Record with the updated field (copy-on-write).
/// Returns a `*mut Value` as i64 (boxed Value::Record).
///
/// # Safety
/// `record_ptr` must be a valid `*mut Value` pointer pointing to a `Value::Record`.
/// `field_name_ptr` must be a valid `*const u8` pointer to UTF-8 bytes.
/// `value_ptr` must be a valid `*mut Value` pointer.
extern "C" fn jit_rt_record_set_field(
    record_ptr: i64,
    field_name_ptr: *const u8,
    field_name_len: usize,
    value_ptr: i64,
) -> i64 {
    if record_ptr == 0 {
        // Can't set field on null, return null
        return Box::into_raw(Box::new(Value::Null)) as i64;
    }

    let value = unsafe { &*(record_ptr as *const Value) };
    let field_name = unsafe {
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(field_name_ptr, field_name_len))
    };
    let new_value = if value_ptr == 0 {
        Value::Null
    } else {
        unsafe { (*(value_ptr as *const Value)).clone() }
    };

    let result = match value {
        Value::Record(r) => {
            // Clone the record and update the field
            let mut new_fields = r.fields.clone();
            new_fields.insert(field_name.to_string(), new_value);
            Value::new_record(RecordValue {
                type_name: r.type_name.clone(),
                fields: new_fields,
            })
        }
        _ => Value::Null,
    };

    Box::into_raw(Box::new(result)) as i64
}

/// Clone a Value (for record field access results).
/// Returns a new `*mut Value` as i64.
///
/// # Safety
/// `value_ptr` must be a valid `*mut Value` pointer.
extern "C" fn jit_rt_value_clone(value_ptr: i64) -> i64 {
    if value_ptr == 0 {
        return 0;
    }
    let value = unsafe { &*(value_ptr as *const Value) };
    Box::into_raw(Box::new(value.clone())) as i64
}

/// Free a boxed Value.
///
/// # Safety
/// `value_ptr` must be a valid `*mut Value` pointer that was created by one of the
/// JIT runtime functions. Must not be called twice on the same pointer.
extern "C" fn jit_rt_value_drop(value_ptr: i64) {
    if value_ptr != 0 {
        unsafe {
            let _ = Box::from_raw(value_ptr as *mut Value);
        }
    }
}

/// Register all JIT record runtime helper symbols with a JITBuilder.
fn register_record_helpers(builder: &mut JITBuilder) {
    builder.symbol(
        "jit_rt_record_get_field",
        jit_rt_record_get_field as *const u8,
    );
    builder.symbol(
        "jit_rt_record_set_field",
        jit_rt_record_set_field as *const u8,
    );
    builder.symbol("jit_rt_value_clone", jit_rt_value_clone as *const u8);
    builder.symbol("jit_rt_value_drop", jit_rt_value_drop as *const u8);
}

// ---------------------------------------------------------------------------
// JIT Intrinsic Runtime Helpers
// ---------------------------------------------------------------------------

/// Print an integer to stdout (intrinsic #2: PRINT)
/// For JIT-compiled code, simplified to print just integers.
///
/// # Safety
/// None - operates on a simple i64 value.
extern "C" fn jit_rt_print_int(value: i64) {
    println!("{}", value);
}

/// Print a string to stdout (intrinsic #2: PRINT)
/// For JIT-compiled code, prints a single string argument.
///
/// # Safety
/// `s` must be a valid `*mut String` pointer.
extern "C" fn jit_rt_print_str(s: i64) {
    if s != 0 {
        let string = unsafe { &*(s as *const String) };
        println!("{}", string);
    }
}

/// Get the length of a string (intrinsic #0: LENGTH)
/// Returns the number of Unicode characters (not bytes).
///
/// # Safety
/// `s` must be a valid `*mut String` pointer.
extern "C" fn jit_rt_string_len(s: i64) -> i64 {
    if s == 0 {
        return 0;
    }
    let string = unsafe { &*(s as *const String) };
    string.chars().count() as i64
}

/// Absolute value of an integer (intrinsic #16: ABS)
///
/// # Safety
/// None - operates on a simple i64 value.
extern "C" fn jit_rt_abs_int(value: i64) -> i64 {
    value.abs()
}

/// Absolute value of a float (intrinsic #16: ABS)
///
/// # Safety
/// None - operates on a simple f64 value.
extern "C" fn jit_rt_abs_float(value: f64) -> f64 {
    value.abs()
}

/// Register all JIT intrinsic runtime helper symbols with a JITBuilder.
fn register_intrinsic_helpers(builder: &mut JITBuilder) {
    builder.symbol("jit_rt_print_int", jit_rt_print_int as *const u8);
    builder.symbol("jit_rt_print_str", jit_rt_print_str as *const u8);
    builder.symbol("jit_rt_string_len", jit_rt_string_len as *const u8);
    builder.symbol("jit_rt_abs_int", jit_rt_abs_int as *const u8);
    builder.symbol("jit_rt_abs_float", jit_rt_abs_float as *const u8);
}

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
    /// Number of JIT executions performed.
    pub executions: u64,
}

// ---------------------------------------------------------------------------
// JIT Error
// ---------------------------------------------------------------------------

/// Errors specific to JIT compilation and execution.
#[derive(Debug)]
pub enum JitError {
    /// Compilation failed.
    CompileError(CodegenError),
    /// The requested cell was not found in the module.
    CellNotFound(String),
    /// JIT module creation failed.
    ModuleError(String),
}

impl std::fmt::Display for JitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JitError::CompileError(e) => write!(f, "JIT compile error: {e}"),
            JitError::CellNotFound(name) => write!(f, "cell not found: {name}"),
            JitError::ModuleError(msg) => write!(f, "JIT module error: {msg}"),
        }
    }
}

impl std::error::Error for JitError {}

impl From<CodegenError> for JitError {
    fn from(e: CodegenError) -> Self {
        JitError::CompileError(e)
    }
}

// ---------------------------------------------------------------------------
// Cached compiled function
// ---------------------------------------------------------------------------

/// Metadata for a JIT-compiled function.
struct CompiledFunction {
    /// Raw function pointer to the compiled native code.
    fn_ptr: *const u8,
    /// Number of parameters the function expects.
    param_count: usize,
    /// True if the function returns a heap-allocated string pointer.
    returns_string: bool,
}

// Safety: The function pointers are valid for the lifetime of the JITModule
// that produced them. We ensure the JITModule lives as long as the JitEngine.
unsafe impl Send for CompiledFunction {}

// ---------------------------------------------------------------------------
// JIT Engine
// ---------------------------------------------------------------------------

/// Manages JIT-compiled function caching and on-demand compilation with
/// real in-process native code execution.
///
/// Typical lifecycle:
/// 1. Interpreter calls `record_and_check("cell_name")` on every cell entry.
/// 2. When the function returns `true` (just became hot), the runtime calls
///    `compile_hot("cell_name", &module)` to compile it.
/// 3. Subsequent invocations call `execute_jit("cell_name", &args)` to run
///    the native code directly, bypassing the interpreter.
pub struct JitEngine {
    profile: ExecutionProfile,
    /// The Cranelift JIT module. Owns the compiled code memory.
    jit_module: Option<JITModule>,
    /// Cached compiled function pointers keyed by cell name.
    cache: HashMap<String, CompiledFunction>,
    /// Settings for on-demand compilation.
    #[allow(dead_code)]
    codegen_settings: CodegenSettings,
    /// Compilation statistics.
    stats: JitStats,
    /// Retained optimized cells whose string constant data is referenced by
    /// raw pointers baked into the JIT machine code. Must live as long as
    /// `jit_module`.
    _retained_cells: Vec<LirCell>,
}

impl JitEngine {
    /// Create a new JIT engine. The `threshold` is forwarded to the internal
    /// `ExecutionProfile`.
    pub fn new(settings: CodegenSettings, threshold: u64) -> Self {
        Self {
            profile: ExecutionProfile::new(threshold),
            jit_module: None,
            cache: HashMap::new(),
            codegen_settings: settings,
            stats: JitStats::default(),
            _retained_cells: Vec::new(),
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

    /// Compile all cells from the given `LirModule` via Cranelift JIT.
    /// Compiled function pointers are stored in the cache.
    ///
    /// If a cell is already cached, the cache entry is preserved (with a
    /// cache-hit bump).
    pub fn compile_module(&mut self, module: &LirModule) -> Result<(), JitError> {
        // Create a new JIT module for this compilation batch.
        // Enable Cranelift's `speed` optimization level so the generated
        // native code is competitive with ahead-of-time compilers. Without
        // this, Cranelift defaults to `none` (no optimizations), resulting
        // in 20-50x slower code for compute-heavy workloads like fibonacci.
        let mut builder = JITBuilder::with_flags(
            &[("opt_level", "speed")],
            cranelift_module::default_libcall_names(),
        )
        .map_err(|e| JitError::ModuleError(format!("JITBuilder creation failed: {e}")))?;

        // Register string runtime helper symbols so JIT code can call them.
        register_string_helpers(&mut builder);

        // Register record runtime helper symbols so JIT code can call them.
        register_record_helpers(&mut builder);

        // Register intrinsic runtime helper symbols so JIT code can call builtins.
        register_intrinsic_helpers(&mut builder);

        let mut jit_module = JITModule::new(builder);
        let pointer_type = jit_module.isa().pointer_type();

        // Lower all cells into the JIT module.
        let lowered = lower_module_jit(&mut jit_module, module, pointer_type)?;

        // Finalize all definitions so we can retrieve function pointers.
        jit_module
            .finalize_definitions()
            .map_err(|e| JitError::ModuleError(format!("finalize_definitions failed: {e}")))?;

        // Retrieve and cache function pointers.
        for func in &lowered.functions {
            let fn_ptr = jit_module.get_finalized_function(func.func_id);
            self.cache.insert(
                func.name.clone(),
                CompiledFunction {
                    fn_ptr,
                    param_count: func.param_count,
                    returns_string: func.returns_string,
                },
            );
            self.stats.cells_compiled += 1;
        }
        self.stats.cache_size = self.cache.len();

        // Store the JIT module so its memory stays alive.
        self.jit_module = Some(jit_module);

        // Retain optimized cells so string constant pointers stay valid.
        self._retained_cells = lowered._retained_cells;

        Ok(())
    }

    /// Compile a single cell from the given `LirModule` to native code via
    /// Cranelift JIT. The compiled function pointer is stored in the cache.
    ///
    /// If the cell is already cached, returns Ok immediately (with a
    /// cache-hit bump).
    pub fn compile_hot(&mut self, cell_name: &str, module: &LirModule) -> Result<(), JitError> {
        // Return early if already cached.
        if self.cache.contains_key(cell_name) {
            self.stats.cache_hits += 1;
            return Ok(());
        }

        // Compile the entire module (all cells) since cross-cell calls need
        // all functions present.
        self.compile_module(module)?;

        if !self.cache.contains_key(cell_name) {
            return Err(JitError::CellNotFound(cell_name.to_string()));
        }

        // Reset the profile counter so we don't re-trigger immediately.
        self.profile.reset(cell_name);

        Ok(())
    }

    /// Execute a JIT-compiled function with no arguments.
    /// Returns the i64 result.
    ///
    /// # Safety
    /// The caller must ensure that the function was compiled with the
    /// correct signature (no params, returns i64).
    pub fn execute_jit_nullary(&mut self, cell_name: &str) -> Result<i64, JitError> {
        let compiled = self
            .cache
            .get(cell_name)
            .ok_or_else(|| JitError::CellNotFound(cell_name.to_string()))?;

        let fn_ptr = compiled.fn_ptr;
        self.stats.executions += 1;

        // SAFETY: The function pointer was produced by Cranelift JIT and is
        // valid for the lifetime of the JITModule (which we own). The
        // caller guarantees the signature matches.
        let result = unsafe {
            let code_fn: fn() -> i64 = std::mem::transmute(fn_ptr);
            code_fn()
        };
        Ok(result)
    }

    /// Execute a JIT-compiled function with one i64 argument.
    /// Returns the i64 result.
    pub fn execute_jit_unary(&mut self, cell_name: &str, arg: i64) -> Result<i64, JitError> {
        let compiled = self
            .cache
            .get(cell_name)
            .ok_or_else(|| JitError::CellNotFound(cell_name.to_string()))?;

        let fn_ptr = compiled.fn_ptr;
        self.stats.executions += 1;

        let result = unsafe {
            let code_fn: fn(i64) -> i64 = std::mem::transmute(fn_ptr);
            code_fn(arg)
        };
        Ok(result)
    }

    /// Execute a JIT-compiled function with two i64 arguments.
    /// Returns the i64 result.
    pub fn execute_jit_binary(
        &mut self,
        cell_name: &str,
        arg1: i64,
        arg2: i64,
    ) -> Result<i64, JitError> {
        let compiled = self
            .cache
            .get(cell_name)
            .ok_or_else(|| JitError::CellNotFound(cell_name.to_string()))?;

        let fn_ptr = compiled.fn_ptr;
        self.stats.executions += 1;

        let result = unsafe {
            let code_fn: fn(i64, i64) -> i64 = std::mem::transmute(fn_ptr);
            code_fn(arg1, arg2)
        };
        Ok(result)
    }

    /// Execute a JIT-compiled function with three i64 arguments.
    /// Returns the i64 result.
    pub fn execute_jit_ternary(
        &mut self,
        cell_name: &str,
        arg1: i64,
        arg2: i64,
        arg3: i64,
    ) -> Result<i64, JitError> {
        let compiled = self
            .cache
            .get(cell_name)
            .ok_or_else(|| JitError::CellNotFound(cell_name.to_string()))?;

        let fn_ptr = compiled.fn_ptr;
        self.stats.executions += 1;

        let result = unsafe {
            let code_fn: fn(i64, i64, i64) -> i64 = std::mem::transmute(fn_ptr);
            code_fn(arg1, arg2, arg3)
        };
        Ok(result)
    }

    /// Generic JIT execution dispatching on arity. Supports 0..=3 i64
    /// arguments.
    pub fn execute_jit(&mut self, cell_name: &str, args: &[i64]) -> Result<i64, JitError> {
        match args.len() {
            0 => self.execute_jit_nullary(cell_name),
            1 => self.execute_jit_unary(cell_name, args[0]),
            2 => self.execute_jit_binary(cell_name, args[0], args[1]),
            3 => self.execute_jit_ternary(cell_name, args[0], args[1], args[2]),
            n => Err(JitError::ModuleError(format!(
                "unsupported arity {n} for JIT execution (max 3)"
            ))),
        }
    }

    /// Compile a cell if not already compiled, then execute it.
    /// Convenience method that combines `compile_hot` and `execute_jit`.
    pub fn compile_and_execute(
        &mut self,
        cell_name: &str,
        module: &LirModule,
        args: &[i64],
    ) -> Result<i64, JitError> {
        self.compile_hot(cell_name, module)?;
        self.execute_jit(cell_name, args)
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

    /// Get the number of parameters for a compiled cell.
    pub fn compiled_param_count(&self, cell_name: &str) -> Option<usize> {
        self.cache.get(cell_name).map(|c| c.param_count)
    }

    /// Check if a compiled cell returns a heap-allocated string pointer.
    pub fn returns_string(&self, cell_name: &str) -> bool {
        self.cache
            .get(cell_name)
            .map(|c| c.returns_string)
            .unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// Pre-scan: check if a cell only uses JIT-supported opcodes
// ---------------------------------------------------------------------------

/// Returns `true` if every instruction in the cell uses an opcode the JIT can
/// compile. Cells containing unsupported opcodes (e.g. ToolCall,
/// NewList, etc.) are filtered out before compilation so we never emit traps
/// for unsupported operations.
fn is_cell_jit_compilable(cell: &LirCell) -> bool {
    cell.instructions.iter().all(|instr| {
        matches!(
            instr.op,
            OpCode::LoadK
                | OpCode::LoadBool
                | OpCode::LoadInt
                | OpCode::LoadNil
                | OpCode::Move
                | OpCode::MoveOwn
                | OpCode::Add
                | OpCode::Sub
                | OpCode::Mul
                | OpCode::Div
                | OpCode::Mod
                | OpCode::Neg
                | OpCode::FloorDiv
                | OpCode::Pow
                | OpCode::Eq
                | OpCode::Lt
                | OpCode::Le
                | OpCode::Not
                | OpCode::And
                | OpCode::Or
                | OpCode::Test
                | OpCode::Jmp
                | OpCode::Break
                | OpCode::Continue
                | OpCode::Return
                | OpCode::Halt
                | OpCode::Call
                | OpCode::TailCall
                | OpCode::Intrinsic
                | OpCode::Nop
                | OpCode::Loop
                | OpCode::ForPrep
                | OpCode::ForLoop
                | OpCode::ForIn
                | OpCode::BitOr
                | OpCode::BitAnd
                | OpCode::BitXor
                | OpCode::BitNot
                | OpCode::Shl
                | OpCode::Shr
                | OpCode::GetField
                | OpCode::SetField
        )
    })
}

// ---------------------------------------------------------------------------
// JIT-specific lowering (mirrors lower.rs but targets JITModule)
// ---------------------------------------------------------------------------

/// Result of lowering an entire LIR module into the JIT.
struct JitLoweredModule {
    functions: Vec<JitLoweredFunction>,
    /// Retain optimized cells so that string constant data (whose raw pointers
    /// are baked into the generated machine code as immediates for
    /// `jit_rt_string_alloc` calls) stays alive for the lifetime of the JIT
    /// code.
    _retained_cells: Vec<LirCell>,
}

struct JitLoweredFunction {
    name: String,
    func_id: FuncId,
    param_count: usize,
    returns_string: bool,
}

/// Lower an entire LIR module into Cranelift IR inside the given `JITModule`.
/// Cells containing unsupported opcodes are silently skipped — they will
/// remain interpreted.
fn lower_module_jit(
    module: &mut JITModule,
    lir: &LirModule,
    pointer_type: ClifType,
) -> Result<JitLoweredModule, CodegenError> {
    let mut fb_ctx = FunctionBuilderContext::new();

    // Filter to only JIT-compilable cells.
    let compilable_cells: Vec<&LirCell> = lir
        .cells
        .iter()
        .filter(|c| is_cell_jit_compilable(c))
        .collect();

    if compilable_cells.is_empty() {
        return Ok(JitLoweredModule {
            functions: Vec::new(),
            _retained_cells: Vec::new(),
        });
    }

    // First pass: declare all compilable cell signatures.
    let mut func_ids: HashMap<String, FuncId> = HashMap::new();
    for cell in &compilable_cells {
        let mut sig = module.make_signature();
        for param in &cell.params {
            let param_ty = lir_type_str_to_cl_type(&param.ty, pointer_type);
            // Cranelift ABI requires I8 to be extended; use I64 for Bool params.
            let abi_ty = if param_ty == types::I8 {
                types::I64
            } else {
                param_ty
            };
            sig.params.push(AbiParam::new(abi_ty));
        }
        let ret_ty = cell
            .returns
            .as_deref()
            .map(|s| lir_type_str_to_cl_type(s, pointer_type))
            .unwrap_or(pointer_type);
        // Same for return: use I64 for Bool.
        let abi_ret = if ret_ty == types::I8 {
            types::I64
        } else {
            ret_ty
        };
        sig.returns.push(AbiParam::new(abi_ret));
        let func_id = module
            .declare_function(&cell.name, Linkage::Export, &sig)
            .map_err(|e| {
                CodegenError::LoweringError(format!("declare_function({}): {e}", cell.name))
            })?;
        func_ids.insert(cell.name.clone(), func_id);
    }

    // Second pass: lower each cell body.
    // We collect optimized cells so their constant string data (whose raw
    // pointers are baked into the machine code) stays alive as long as the
    // JIT module.
    let mut retained_cells: Vec<LirCell> = Vec::with_capacity(compilable_cells.len());
    let mut lowered = JitLoweredModule {
        functions: Vec::with_capacity(compilable_cells.len()),
        _retained_cells: Vec::new(), // filled after the loop
    };

    for cell in &compilable_cells {
        let func_id = func_ids[&cell.name];
        let mut ctx = Context::new();

        // Optimize before lowering to IR
        let mut optimized_cell = (*cell).clone();
        crate::opt::optimize(&mut optimized_cell);

        lower_cell_jit(
            &mut ctx,
            module,
            &optimized_cell,
            &mut fb_ctx,
            pointer_type,
            func_id,
            &func_ids,
            &lir.strings,
        )?;

        // Keep the cell alive so string constant pointers remain valid.
        retained_cells.push(optimized_cell);

        let ret_is_string = cell
            .returns
            .as_deref()
            .map(|s| s == "String")
            .unwrap_or(false);
        lowered.functions.push(JitLoweredFunction {
            name: cell.name.clone(),
            func_id,
            param_count: cell.params.len(),
            returns_string: ret_is_string,
        });
    }

    lowered._retained_cells = retained_cells;
    Ok(lowered)
}

// ---------------------------------------------------------------------------
// Per-cell lowering (JIT variant)
// ---------------------------------------------------------------------------

fn lower_cell_jit(
    ctx: &mut Context,
    module: &mut JITModule,
    cell: &LirCell,
    fb_ctx: &mut FunctionBuilderContext,
    pointer_type: ClifType,
    func_id: FuncId,
    func_ids: &HashMap<String, FuncId>,
    string_table: &[String],
) -> Result<(), CodegenError> {
    // Delegate to the unified IR builder
    crate::ir::lower_cell(
        ctx,
        fb_ctx,
        cell,
        module,
        pointer_type,
        func_id,
        func_ids,
        string_table,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, target_arch = "x86_64"))]
mod tests {
    use super::*;
    use lumen_core::lir::{Constant, Instruction, LirCell, LirModule, LirParam, OpCode};

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
        assert!(!profile.is_hot("fn_a"));

        profile.record_call("fn_a");
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
        profile.record_call("gamma");

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

    // --- JitEngine record_and_check tests ---------------------------------

    #[test]
    fn engine_record_and_check() {
        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 3);

        assert!(!engine.record_and_check("fn_x"));
        assert!(!engine.record_and_check("fn_x"));
        assert!(!engine.record_and_check("fn_x"));
        assert!(engine.record_and_check("fn_x"));
        assert!(!engine.record_and_check("fn_x"));
    }

    // --- JIT compile and execute: REAL native code execution tests ----------

    #[test]
    fn jit_execute_constant_42() {
        // cell answer() -> Int = 42
        let lir = simple_lir_module();
        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        let result = engine
            .compile_and_execute("answer", &lir, &[])
            .expect("JIT compile and execute should succeed");
        assert_eq!(result, 42, "JIT-compiled answer() should return 42");
    }

    #[test]
    fn jit_execute_addition() {
        // cell add_two() -> Int = 10 + 32
        let lir = make_module_with_cells(vec![LirCell {
            name: "add_two".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![Constant::Int(10), Constant::Int(32)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Add, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        let result = engine
            .compile_and_execute("add_two", &lir, &[])
            .expect("JIT add should succeed");
        assert_eq!(result, 42, "10 + 32 = 42");
    }

    #[test]
    fn jit_execute_with_parameter() {
        // cell double(x: Int) -> Int = x + x
        let lir = make_module_with_cells(vec![LirCell {
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
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        engine
            .compile_module(&lir)
            .expect("JIT compile should succeed");

        assert_eq!(engine.execute_jit_unary("double", 21).unwrap(), 42);
        assert_eq!(engine.execute_jit_unary("double", 0).unwrap(), 0);
        assert_eq!(engine.execute_jit_unary("double", -5).unwrap(), -10);
    }

    #[test]
    fn jit_execute_binary_params() {
        // cell add(a: Int, b: Int) -> Int = a + b
        let lir = make_module_with_cells(vec![LirCell {
            name: "add".to_string(),
            params: vec![
                LirParam {
                    name: "a".to_string(),
                    ty: "Int".to_string(),
                    register: 0,
                    variadic: false,
                },
                LirParam {
                    name: "b".to_string(),
                    ty: "Int".to_string(),
                    register: 1,
                    variadic: false,
                },
            ],
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![],
            instructions: vec![
                Instruction::abc(OpCode::Add, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        engine
            .compile_module(&lir)
            .expect("JIT compile should succeed");

        assert_eq!(engine.execute_jit_binary("add", 10, 32).unwrap(), 42);
        assert_eq!(engine.execute_jit_binary("add", -3, 3).unwrap(), 0);
        assert_eq!(engine.execute_jit_binary("add", 100, 200).unwrap(), 300);
    }

    #[test]
    fn jit_execute_factorial_loop() {
        // Iterative factorial via while loop:
        //   cell factorial(n: Int) -> Int
        //     r1 = 1 (result)
        //     r2 = 1 (counter constant)
        //     while n > 0: r1 = r1 * n; n = n - r2
        //     return r1
        //
        //  0: LoadInt  r1, 1          (result = 1)
        //  1: LoadInt  r2, 1          (decrement constant)
        //  2: LoadInt  r3, 0          (zero for comparison)
        //  3: Lt       r4, r3, r0     (0 < n?)  -- loop header
        //  4: Test     r4, 0, 0
        //  5: Jmp      +3             (-> 9: exit loop)
        //  6: Mul      r1, r1, r0     (result *= n)
        //  7: Sub      r0, r0, r2     (n -= 1)
        //  8: Jmp      -6             (-> 3: loop header)
        //  9: Return   r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "factorial".to_string(),
            params: vec![LirParam {
                name: "n".to_string(),
                ty: "Int".to_string(),
                register: 0,
                variadic: false,
            }],
            returns: Some("Int".to_string()),
            registers: 5,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 1, 1),   // 0: r1 = 1
                Instruction::abx(OpCode::LoadInt, 2, 1),   // 1: r2 = 1
                Instruction::abx(OpCode::LoadInt, 3, 0),   // 2: r3 = 0
                Instruction::abc(OpCode::Lt, 4, 3, 0),     // 3: r4 = 0 < n
                Instruction::abc(OpCode::Test, 4, 0, 0),   // 4: test
                Instruction::sax(OpCode::Jmp, 3),          // 5: -> 9 (exit)
                Instruction::abc(OpCode::Mul, 1, 1, 0),    // 6: r1 *= n
                Instruction::abc(OpCode::Sub, 0, 0, 2),    // 7: n -= 1
                Instruction::sax(OpCode::Jmp, -6),         // 8: -> 3 (loop)
                Instruction::abc(OpCode::Return, 1, 1, 0), // 9: return r1
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        engine
            .compile_module(&lir)
            .expect("JIT compile should succeed");

        assert_eq!(engine.execute_jit_unary("factorial", 0).unwrap(), 1);
        assert_eq!(engine.execute_jit_unary("factorial", 1).unwrap(), 1);
        assert_eq!(engine.execute_jit_unary("factorial", 5).unwrap(), 120);
        assert_eq!(engine.execute_jit_unary("factorial", 10).unwrap(), 3628800);
    }

    #[test]
    fn jit_execute_fibonacci_tco() {
        // Tail-recursive fibonacci accumulator:
        //   cell fib_acc(n: Int, a: Int, b: Int) -> Int
        //     if n <= 0 then return a end
        //     fib_acc(n - 1, b, a + b)
        //   end
        //
        //  0: LoadInt   r3, 0
        //  1: Le        r4, r0, r3      (n <= 0?)
        //  2: Test      r4, 0, 0
        //  3: Jmp       +1              (-> 5: not done)
        //  4: Return    r1              (return a)
        //  5: LoadK     r5, 0           ("fib_acc")
        //  6: LoadInt   r8, 1
        //  7: Sub       r6, r0, r8      (n - 1)
        //  8: Move      r7, r2          (b)
        //  9: Add       r8, r1, r2      (a + b)
        // 10: TailCall  r5, 3, 1        (fib_acc(r6, r7, r8))
        let lir = make_module_with_cells(vec![LirCell {
            name: "fib_acc".to_string(),
            params: vec![
                LirParam {
                    name: "n".to_string(),
                    ty: "Int".to_string(),
                    register: 0,
                    variadic: false,
                },
                LirParam {
                    name: "a".to_string(),
                    ty: "Int".to_string(),
                    register: 1,
                    variadic: false,
                },
                LirParam {
                    name: "b".to_string(),
                    ty: "Int".to_string(),
                    register: 2,
                    variadic: false,
                },
            ],
            returns: Some("Int".to_string()),
            registers: 9,
            constants: vec![Constant::String("fib_acc".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 3, 0),     // 0: r3 = 0
                Instruction::abc(OpCode::Le, 4, 0, 3),       // 1: r4 = n <= 0
                Instruction::abc(OpCode::Test, 4, 0, 0),     // 2: test
                Instruction::sax(OpCode::Jmp, 1),            // 3: -> 5
                Instruction::abc(OpCode::Return, 1, 1, 0),   // 4: return a
                Instruction::abx(OpCode::LoadK, 5, 0),       // 5: r5 = "fib_acc"
                Instruction::abx(OpCode::LoadInt, 8, 1),     // 6: r8 = 1
                Instruction::abc(OpCode::Sub, 6, 0, 8),      // 7: r6 = n - 1
                Instruction::abc(OpCode::Move, 7, 2, 0),     // 8: r7 = b
                Instruction::abc(OpCode::Add, 8, 1, 2),      // 9: r8 = a + b
                Instruction::abc(OpCode::TailCall, 5, 3, 1), // 10: tail-call
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        engine
            .compile_module(&lir)
            .expect("JIT compile should succeed");

        // fib_acc(n, 0, 1) computes fib(n)
        assert_eq!(engine.execute_jit_ternary("fib_acc", 0, 0, 1).unwrap(), 0);
        assert_eq!(engine.execute_jit_ternary("fib_acc", 1, 0, 1).unwrap(), 1);
        assert_eq!(engine.execute_jit_ternary("fib_acc", 5, 0, 1).unwrap(), 5);
        assert_eq!(engine.execute_jit_ternary("fib_acc", 10, 0, 1).unwrap(), 55);
        assert_eq!(
            engine.execute_jit_ternary("fib_acc", 20, 0, 1).unwrap(),
            6765
        );
    }

    #[test]
    fn jit_execute_cross_cell_call() {
        // Two cells: double(x) = x + x, main() = double(21)
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
                Instruction::abx(OpCode::LoadK, 0, 0), // r0 = "double"
                Instruction::abx(OpCode::LoadK, 1, 1), // r1 = 21
                Instruction::abc(OpCode::Call, 0, 1, 1),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };

        let lir = make_module_with_cells(vec![double_cell, main_cell]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        let result = engine
            .compile_and_execute("main", &lir, &[])
            .expect("cross-cell JIT should succeed");
        assert_eq!(result, 42, "main() -> double(21) = 42");
    }

    #[test]
    fn jit_hot_path_triggers_compilation() {
        let lir = simple_lir_module();
        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 3);

        // Not hot yet.
        assert!(!engine.is_compiled("answer"));
        assert!(!engine.record_and_check("answer"));
        assert!(!engine.record_and_check("answer"));
        assert!(!engine.record_and_check("answer"));

        // 4th call: crosses threshold.
        assert!(engine.record_and_check("answer"));

        // Now compile and execute.
        engine
            .compile_hot("answer", &lir)
            .expect("compile_hot should succeed");
        assert!(engine.is_compiled("answer"));

        let result = engine
            .execute_jit_nullary("answer")
            .expect("execute should succeed");
        assert_eq!(result, 42);
    }

    #[test]
    fn jit_cache_and_stats() {
        let lir = simple_lir_module();
        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        let s0 = engine.stats();
        assert_eq!(s0.cells_compiled, 0);
        assert_eq!(s0.cache_hits, 0);
        assert_eq!(s0.executions, 0);

        engine.compile_hot("answer", &lir).expect("first compile");
        let s1 = engine.stats();
        assert_eq!(s1.cells_compiled, 1);
        assert!(s1.cache_size >= 1);

        // Second compile_hot should be a cache hit.
        engine.compile_hot("answer", &lir).expect("cached compile");
        let s2 = engine.stats();
        assert_eq!(s2.cache_hits, 1);

        engine.execute_jit_nullary("answer").expect("execute");
        let s3 = engine.stats();
        assert_eq!(s3.executions, 1);
    }

    #[test]
    fn jit_invalidate() {
        let lir = simple_lir_module();
        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        engine.compile_hot("answer", &lir).expect("compile");
        assert!(engine.is_compiled("answer"));

        engine.invalidate("answer");
        assert!(!engine.is_compiled("answer"));
        assert_eq!(engine.stats().cache_size, 0);
    }

    #[test]
    fn jit_execute_if_else() {
        // cell choose(x: Int) -> Int
        //   if x > 0 then 100 else 200 end
        //
        //  0: LoadInt   r1, 0
        //  1: Lt        r2, r1, r0     (0 < x => x > 0)
        //  2: Test      r2, 0, 0
        //  3: Jmp       +2             (-> 6: else)
        //  4: LoadInt   r3, 100
        //  5: Jmp       +1             (-> 7: end)
        //  6: LoadInt   r3, 50         -- LoadInt uses sbx (signed 32-bit) for the value
        //  7: Return    r3
        //
        // LoadInt stores the value in the Bx field (signed 32-bit via sbx()).
        // 100 fits in i8 (0x64). For the else branch let's use 50.
        let lir = make_module_with_cells(vec![LirCell {
            name: "choose".to_string(),
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
                Instruction::abx(OpCode::LoadInt, 1, 0),   // 0: r1 = 0
                Instruction::abc(OpCode::Lt, 2, 1, 0),     // 1: r2 = 0 < x
                Instruction::abc(OpCode::Test, 2, 0, 0),   // 2: test
                Instruction::sax(OpCode::Jmp, 2),          // 3: -> 6 (else)
                Instruction::abx(OpCode::LoadInt, 3, 100), // 4: r3 = 100
                Instruction::sax(OpCode::Jmp, 1),          // 5: -> 7 (end)
                Instruction::abx(OpCode::LoadInt, 3, 50),  // 6: r3 = 50
                Instruction::abc(OpCode::Return, 3, 1, 0), // 7: return r3
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        assert_eq!(engine.execute_jit_unary("choose", 5).unwrap(), 100);
        assert_eq!(engine.execute_jit_unary("choose", -1).unwrap(), 50);
        assert_eq!(engine.execute_jit_unary("choose", 0).unwrap(), 50);
    }

    #[test]
    fn jit_execute_generic_dispatch() {
        // Test the generic execute_jit() dispatch with varying arities.
        let add_cell = LirCell {
            name: "add".to_string(),
            params: vec![
                LirParam {
                    name: "a".to_string(),
                    ty: "Int".to_string(),
                    register: 0,
                    variadic: false,
                },
                LirParam {
                    name: "b".to_string(),
                    ty: "Int".to_string(),
                    register: 1,
                    variadic: false,
                },
            ],
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![],
            instructions: vec![
                Instruction::abc(OpCode::Add, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };

        let answer_cell = LirCell {
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
        };

        let lir = make_module_with_cells(vec![add_cell, answer_cell]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        // Nullary dispatch.
        assert_eq!(engine.execute_jit("answer", &[]).unwrap(), 42);

        // Binary dispatch.
        assert_eq!(engine.execute_jit("add", &[10, 32]).unwrap(), 42);

        // Unsupported arity.
        assert!(engine.execute_jit("add", &[1, 2, 3, 4]).is_err());
    }

    #[test]
    fn jit_compilable_includes_record_ops() {
        // Verify GetField and SetField are in the whitelist
        let get_field_cell = LirCell {
            name: "get_field".to_string(),
            params: vec![],
            returns: Some("Int".to_string()),
            registers: 3,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 0, 0), // r0 = 0 (record ptr stub)
                Instruction::abc(OpCode::GetField, 1, 0, 0), // r1 = r0.field[0]
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };

        let set_field_cell = LirCell {
            name: "set_field".to_string(),
            params: vec![],
            returns: Some("Int".to_string()),
            registers: 3,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 0, 0), // r0 = 0 (record ptr stub)
                Instruction::abx(OpCode::LoadInt, 1, 42), // r1 = 42
                Instruction::abc(OpCode::SetField, 0, 0, 1), // r0.field[0] = r1
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };

        assert!(
            is_cell_jit_compilable(&get_field_cell),
            "GetField should be JIT-compilable"
        );
        assert!(
            is_cell_jit_compilable(&set_field_cell),
            "SetField should be JIT-compilable"
        );
    }

    #[test]
    fn jit_compile_record_field_access() {
        // Test that cells with GetField/SetField compile and execute without errors.
        // GetField on a null record returns a boxed Value::Null (non-zero pointer).
        // SetField on a null record returns a boxed Value::Null (non-zero pointer).
        let lir = make_module_with_cells(vec![
            LirCell {
                name: "access_field".to_string(),
                params: vec![],
                returns: Some("Int".to_string()),
                registers: 3,
                constants: vec![],
                instructions: vec![
                    Instruction::abx(OpCode::LoadInt, 0, 0), // r0 = 0 (null record ptr)
                    Instruction::abc(OpCode::GetField, 1, 0, 0), // r1 = r0.field[0]
                    Instruction::abc(OpCode::Return, 1, 1, 0), // return r1
                ],
                effect_handler_metas: Vec::new(),
            },
            LirCell {
                name: "set_field".to_string(),
                params: vec![],
                returns: Some("Int".to_string()),
                registers: 3,
                constants: vec![],
                instructions: vec![
                    Instruction::abx(OpCode::LoadInt, 0, 0), // r0 = 0 (null record ptr)
                    Instruction::abx(OpCode::LoadInt, 1, 42), // r1 = 42
                    Instruction::abc(OpCode::SetField, 0, 0, 1), // r0.field[0] = r1 (updates r0)
                    Instruction::abc(OpCode::Return, 1, 1, 0), // return r1
                ],
                effect_handler_metas: Vec::new(),
            },
        ]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        // Should compile successfully
        engine
            .compile_module(&lir)
            .expect("Record field access cells should compile");

        // Verify both cells are compiled
        assert!(
            engine.is_compiled("access_field"),
            "access_field should be compiled"
        );
        assert!(
            engine.is_compiled("set_field"),
            "set_field should be compiled"
        );

        // Execute to ensure no runtime traps
        // GetField on null record returns a boxed Value::Null (non-zero pointer)
        let result = engine
            .execute_jit_nullary("access_field")
            .expect("GetField should execute");
        assert_ne!(
            result, 0,
            "GetField on null returns boxed Value::Null pointer"
        );

        let result2 = engine
            .execute_jit_nullary("set_field")
            .expect("SetField should execute");
        assert_eq!(result2, 42, "SetField returns the value register unchanged");
    }

    #[test]
    fn opt_level_variants() {
        let _none = OptLevel::None;
        let _speed = OptLevel::Speed;
        let _both = OptLevel::SpeedAndSize;
        assert_ne!(OptLevel::None, OptLevel::Speed);
        assert_ne!(OptLevel::Speed, OptLevel::SpeedAndSize);
    }

    // --- JIT string operation tests ----------------------------------------

    #[test]
    fn jit_string_constant_load_and_return() {
        // cell greeting() -> String
        //   return "hello"
        //
        // 0: LoadK   r0, 0   ("hello")
        // 1: Return  r0
        let lir = make_module_with_cells(vec![LirCell {
            name: "greeting".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 2,
            constants: vec![Constant::String("hello".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        assert!(
            engine.returns_string("greeting"),
            "greeting should be marked as returning a string"
        );

        let raw = engine
            .execute_jit_nullary("greeting")
            .expect("execute greeting");
        assert_ne!(raw, 0, "string pointer should be non-null");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "hello");
    }

    #[test]
    fn jit_string_concatenation() {
        // cell concat() -> String
        //   r0 = "hello, "
        //   r1 = "world"
        //   r2 = r0 + r1
        //   return r2
        //
        // 0: LoadK  r0, 0   ("hello, ")
        // 1: LoadK  r1, 1   ("world")
        // 2: Add    r2, r0, r1
        // 3: Return r2
        let lir = make_module_with_cells(vec![LirCell {
            name: "concat".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 4,
            constants: vec![
                Constant::String("hello, ".to_string()),
                Constant::String("world".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Add, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("concat").expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "hello, world");
    }

    #[test]
    fn jit_string_self_concat() {
        // cell double_str() -> String
        //   r0 = "ab"
        //   r0 = r0 + r0   (self-assign concat: a == b)
        //   return r0
        //
        // 0: LoadK  r0, 0   ("ab")
        // 1: Add    r0, r0, r0
        // 2: Return r0
        let lir = make_module_with_cells(vec![LirCell {
            name: "double_str".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 2,
            constants: vec![Constant::String("ab".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Add, 0, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("double_str").expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "abab");
    }

    #[test]
    fn jit_string_equality() {
        // cell eq_test() -> Int
        //   r0 = "abc"
        //   r1 = "abc"
        //   r2 = (r0 == r1)   -> should be 1
        //   return r2
        //
        // 0: LoadK  r0, 0   ("abc")
        // 1: LoadK  r1, 1   ("abc")
        // 2: Eq     r2, r0, r1
        // 3: Return r2
        let lir = make_module_with_cells(vec![LirCell {
            name: "eq_test".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![
                Constant::String("abc".to_string()),
                Constant::String("abc".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Eq, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("eq_test").expect("execute");
        assert_eq!(result, 1, "equal strings should return 1");
    }

    #[test]
    fn jit_string_inequality() {
        // cell neq_test() -> Int
        //   r0 = "abc"
        //   r1 = "xyz"
        //   r2 = (r0 == r1)   -> should be 0
        //   return r2
        let lir = make_module_with_cells(vec![LirCell {
            name: "neq_test".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![
                Constant::String("abc".to_string()),
                Constant::String("xyz".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Eq, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("neq_test").expect("execute");
        assert_eq!(result, 0, "different strings should return 0");
    }

    #[test]
    fn jit_string_less_than() {
        // cell lt_test() -> Int
        //   r0 = "apple"
        //   r1 = "banana"
        //   r2 = (r0 < r1)   -> should be 1 (lexicographic)
        //   return r2
        let lir = make_module_with_cells(vec![LirCell {
            name: "lt_test".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![
                Constant::String("apple".to_string()),
                Constant::String("banana".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Lt, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("lt_test").expect("execute");
        assert_eq!(result, 1, "\"apple\" < \"banana\" should be 1");
    }

    #[test]
    fn jit_string_less_than_reverse() {
        // "banana" < "apple" -> 0
        let lir = make_module_with_cells(vec![LirCell {
            name: "lt_rev".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![
                Constant::String("banana".to_string()),
                Constant::String("apple".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Lt, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("lt_rev").expect("execute");
        assert_eq!(result, 0, "\"banana\" < \"apple\" should be 0");
    }

    #[test]
    fn jit_string_less_equal() {
        // "abc" <= "abc" -> 1
        let lir = make_module_with_cells(vec![LirCell {
            name: "le_eq".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![
                Constant::String("abc".to_string()),
                Constant::String("abc".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Le, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("le_eq").expect("execute");
        assert_eq!(result, 1, "\"abc\" <= \"abc\" should be 1");
    }

    #[test]
    fn jit_string_move_clone() {
        // cell clone_str() -> String
        //   r0 = "original"
        //   r1 = r0         (Move: clone string)
        //   return r1
        //
        // 0: LoadK  r0, 0   ("original")
        // 1: Move   r1, r0
        // 2: Return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "clone_str".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 3,
            constants: vec![Constant::String("original".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Move, 1, 0, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("clone_str").expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "original");
    }

    #[test]
    fn jit_string_overwrite_drops_old() {
        // Verify that overwriting a string register with a new LoadK drops
        // the old value (no leak). We can't directly observe the drop, but
        // we confirm the final value is correct and no crash occurs.
        //
        // cell overwrite() -> String
        //   r0 = "first"
        //   r0 = "second"    (should drop "first" internally)
        //   return r0
        //
        // 0: LoadK  r0, 0   ("first")
        // 1: LoadK  r0, 1   ("second")
        // 2: Return r0
        let lir = make_module_with_cells(vec![LirCell {
            name: "overwrite".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 2,
            constants: vec![
                Constant::String("first".to_string()),
                Constant::String("second".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 0, 1),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("overwrite").expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "second");
    }

    #[test]
    fn jit_string_concat_in_loop() {
        // Build a string by concatenating in a loop (tests memory management
        // under repeated allocation/deallocation).
        //
        // cell build() -> String
        //   r0 = ""           (accumulator)
        //   r1 = "x"          (append constant)
        //   r2 = 3            (counter)
        //   r3 = 0            (zero)
        //   r4 = 1            (decrement)
        //   loop:
        //     if 0 < counter goto body else goto end
        //     body:
        //       r0 = r0 + r1    (self-assign concat)
        //       r2 = r2 - r4
        //       goto loop
        //   end:
        //     return r0
        //
        //  0: LoadK   r0, 0   ("")
        //  1: LoadK   r1, 1   ("x")
        //  2: LoadInt  r2, 3
        //  3: LoadInt  r3, 0
        //  4: LoadInt  r4, 1
        //  5: Lt       r5, r3, r2   (0 < counter? -> truthy means continue)
        //  6: Test     r5, 0, 0
        //  7: Jmp      +3           (-> 11: end, taken when r5 is falsy)
        //  8: Add      r0, r0, r1   (accum += "x")
        //  9: Sub      r2, r2, r4   (counter -= 1)
        // 10: Jmp      -6           (-> 5: loop)
        // 11: Return   r0
        let lir = make_module_with_cells(vec![LirCell {
            name: "build".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 7,
            constants: vec![
                Constant::String("".to_string()),
                Constant::String("x".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),     // 0: r0 = ""
                Instruction::abx(OpCode::LoadK, 1, 1),     // 1: r1 = "x"
                Instruction::abx(OpCode::LoadInt, 2, 3),   // 2: r2 = 3
                Instruction::abx(OpCode::LoadInt, 3, 0),   // 3: r3 = 0
                Instruction::abx(OpCode::LoadInt, 4, 1),   // 4: r4 = 1
                Instruction::abc(OpCode::Lt, 5, 3, 2),     // 5: r5 = 0 < counter
                Instruction::abc(OpCode::Test, 5, 0, 0),   // 6: test
                Instruction::sax(OpCode::Jmp, 3),          // 7: -> 11 (end)
                Instruction::abc(OpCode::Add, 0, 0, 1),    // 8: r0 = r0 + r1
                Instruction::abc(OpCode::Sub, 2, 2, 4),    // 9: r2 -= 1
                Instruction::sax(OpCode::Jmp, -6),         // 10: -> 5 (loop)
                Instruction::abc(OpCode::Return, 0, 1, 0), // 11: return r0
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("build").expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "xxx", "loop should concatenate 'x' three times");
    }

    #[test]
    fn jit_string_conditional_branch() {
        // cell pick(x: Int) -> String
        //   if x > 0 then "positive" else "non-positive" end
        //
        //  0: LoadInt  r1, 0
        //  1: Lt       r2, r1, r0      (0 < x => x > 0?)
        //  2: Test     r2, 0, 0
        //  3: Jmp      +2              (-> 6: else)
        //  4: LoadK    r3, 0           ("positive")
        //  5: Jmp      +1              (-> 7: end)
        //  6: LoadK    r3, 1           ("non-positive")
        //  7: Return   r3
        let lir = make_module_with_cells(vec![LirCell {
            name: "pick".to_string(),
            params: vec![LirParam {
                name: "x".to_string(),
                ty: "Int".to_string(),
                register: 0,
                variadic: false,
            }],
            returns: Some("String".to_string()),
            registers: 5,
            constants: vec![
                Constant::String("positive".to_string()),
                Constant::String("non-positive".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 1, 0),   // 0: r1 = 0
                Instruction::abc(OpCode::Lt, 2, 1, 0),     // 1: r2 = 0 < x
                Instruction::abc(OpCode::Test, 2, 0, 0),   // 2: test
                Instruction::sax(OpCode::Jmp, 2),          // 3: -> 6 (else)
                Instruction::abx(OpCode::LoadK, 3, 0),     // 4: r3 = "positive"
                Instruction::sax(OpCode::Jmp, 1),          // 5: -> 7 (end)
                Instruction::abx(OpCode::LoadK, 3, 1),     // 6: r3 = "non-positive"
                Instruction::abc(OpCode::Return, 3, 1, 0), // 7: return r3
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        assert!(engine.returns_string("pick"));

        let raw_pos = engine.execute_jit_unary("pick", 5).expect("positive");
        let s_pos = unsafe { jit_take_string(raw_pos) };
        assert_eq!(s_pos, "positive");

        let raw_neg = engine.execute_jit_unary("pick", -1).expect("negative");
        let s_neg = unsafe { jit_take_string(raw_neg) };
        assert_eq!(s_neg, "non-positive");

        let raw_zero = engine.execute_jit_unary("pick", 0).expect("zero");
        let s_zero = unsafe { jit_take_string(raw_zero) };
        assert_eq!(s_zero, "non-positive");
    }

    #[test]
    fn jit_string_empty_string() {
        // Verify empty string allocation and return.
        let lir = make_module_with_cells(vec![LirCell {
            name: "empty".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 2,
            constants: vec![Constant::String("".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("empty").expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "");
    }

    #[test]
    fn jit_string_multiple_concats() {
        // cell three_way() -> String
        //   r0 = "a"
        //   r1 = "b"
        //   r2 = "c"
        //   r3 = r0 + r1    ("ab")
        //   r4 = r3 + r2    ("abc")
        //   return r4
        let lir = make_module_with_cells(vec![LirCell {
            name: "three_way".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 6,
            constants: vec![
                Constant::String("a".to_string()),
                Constant::String("b".to_string()),
                Constant::String("c".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),     // r0 = "a"
                Instruction::abx(OpCode::LoadK, 1, 1),     // r1 = "b"
                Instruction::abx(OpCode::LoadK, 2, 2),     // r2 = "c"
                Instruction::abc(OpCode::Add, 3, 0, 1),    // r3 = "a" + "b"
                Instruction::abc(OpCode::Add, 4, 3, 2),    // r4 = "ab" + "c"
                Instruction::abc(OpCode::Return, 4, 1, 0), // return "abc"
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("three_way").expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "abc");
    }

    #[test]
    fn jit_string_eq_used_in_branch() {
        // cell is_hello() -> Int
        //   r0 = "hello"
        //   r1 = "hello"
        //   r2 = (r0 == r1)
        //   if r2 then return 100 else return 200
        //
        //  0: LoadK   r0, 0   ("hello")
        //  1: LoadK   r1, 1   ("hello")
        //  2: Eq      r2, r0, r1
        //  3: Test    r2, 0, 0
        //  4: Jmp     +2      (-> 7: else)
        //  5: LoadInt r3, 100
        //  6: Jmp     +1      (-> 8: end)
        //  7: LoadInt r3, 50
        //  8: Return  r3
        let lir = make_module_with_cells(vec![LirCell {
            name: "is_hello".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 5,
            constants: vec![
                Constant::String("hello".to_string()),
                Constant::String("hello".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Eq, 2, 0, 1),
                Instruction::abc(OpCode::Test, 2, 0, 0),
                Instruction::sax(OpCode::Jmp, 2),
                Instruction::abx(OpCode::LoadInt, 3, 100),
                Instruction::sax(OpCode::Jmp, 1),
                Instruction::abx(OpCode::LoadInt, 3, 50),
                Instruction::abc(OpCode::Return, 3, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("is_hello").expect("execute");
        assert_eq!(result, 100, "equal strings should take the then-branch");
    }

    #[test]
    fn jit_string_returns_string_flag() {
        // Verify that cells returning String have returns_string=true,
        // and cells returning Int have returns_string=false.
        let lir = make_module_with_cells(vec![
            LirCell {
                name: "str_cell".to_string(),
                params: Vec::new(),
                returns: Some("String".to_string()),
                registers: 2,
                constants: vec![Constant::String("hi".to_string())],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
                effect_handler_metas: Vec::new(),
            },
            LirCell {
                name: "int_cell".to_string(),
                params: Vec::new(),
                returns: Some("Int".to_string()),
                registers: 2,
                constants: vec![Constant::Int(42)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
                effect_handler_metas: Vec::new(),
            },
        ]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        assert!(engine.returns_string("str_cell"));
        assert!(!engine.returns_string("int_cell"));
    }

    #[test]
    fn jit_string_move_own_transfer() {
        // cell transfer() -> String
        //   r0 = "transferred"
        //   MoveOwn r1, r0    (ownership transfer, no clone)
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "transfer".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 3,
            constants: vec![Constant::String("transferred".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::MoveOwn, 1, 0, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("transfer").expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "transferred");
    }

    #[test]
    fn jit_string_concat_dest_overwrites_distinct() {
        // Test where Add dest (r0) already holds a string different from both
        // operands (r1, r2). The old r0 value should be dropped.
        //
        // cell overwrite_concat() -> String
        //   r0 = "old"
        //   r1 = "hello"
        //   r2 = " world"
        //   r0 = r1 + r2    (overwrites "old" in r0 with "hello world")
        //   return r0
        let lir = make_module_with_cells(vec![LirCell {
            name: "overwrite_concat".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 4,
            constants: vec![
                Constant::String("old".to_string()),
                Constant::String("hello".to_string()),
                Constant::String(" world".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),     // r0 = "old"
                Instruction::abx(OpCode::LoadK, 1, 1),     // r1 = "hello"
                Instruction::abx(OpCode::LoadK, 2, 2),     // r2 = " world"
                Instruction::abc(OpCode::Add, 0, 1, 2),    // r0 = r1 + r2
                Instruction::abc(OpCode::Return, 0, 1, 0), // return r0
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine
            .execute_jit_nullary("overwrite_concat")
            .expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "hello world");
    }

    #[test]
    fn jit_string_concat_in_place_optimization() {
        // Test the in-place optimization for `a = a + b` pattern.
        // This should use jit_rt_string_concat_mut which reuses the allocation
        // from r0 instead of creating a new string.
        //
        // cell concat_test() -> String
        //   r0 = ""
        //   r1 = "x"
        //   r0 = r0 + r1    (in-place)
        //   r0 = r0 + r1    (in-place)
        //   r0 = r0 + r1    (in-place)
        //   return r0
        let lir = make_module_with_cells(vec![LirCell {
            name: "concat_test".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 3,
            constants: vec![
                Constant::String("".to_string()),
                Constant::String("x".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),     // r0 = ""
                Instruction::abx(OpCode::LoadK, 1, 1),     // r1 = "x"
                Instruction::abc(OpCode::Add, 0, 0, 1),    // r0 = r0 + r1 (in-place!)
                Instruction::abc(OpCode::Add, 0, 0, 1),    // r0 = r0 + r1 (in-place!)
                Instruction::abc(OpCode::Add, 0, 0, 1),    // r0 = r0 + r1 (in-place!)
                Instruction::abc(OpCode::Return, 0, 1, 0), // return r0
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("concat_test").expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "xxx");
    }

    #[test]
    fn jit_intrinsic_abs_int() {
        // Test abs() intrinsic with integer argument
        // cell test_abs() -> Int
        //   r0 = -10
        //   r1 = abs(r0)   # Intrinsic(1, 26, 0) - IntrinsicId::Abs = 26
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_abs".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 2,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 0, (-10i32) as u32), // r0 = -10
                Instruction::abc(OpCode::Intrinsic, 1, 26, 0),         // r1 = abs(r0)
                Instruction::abc(OpCode::Return, 1, 1, 0),             // return r1
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("test_abs").expect("execute");
        assert_eq!(result, 10); // abs(-10) = 10
    }

    #[test]
    fn jit_intrinsic_print_int() {
        // Test print() intrinsic with integer argument
        // cell test_print() -> Int
        //   r0 = 42
        //   r1 = print(r0)  # Intrinsic(1, 9, 0) - IntrinsicId::Print = 9
        //   r2 = 0
        //   return r2
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_print".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 3,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 0, 42),     // r0 = 42
                Instruction::abc(OpCode::Intrinsic, 1, 9, 0), // r1 = print(r0)
                Instruction::abx(OpCode::LoadInt, 2, 0),      // r2 = 0
                Instruction::abc(OpCode::Return, 2, 1, 0),    // return r2
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        // Just verify it compiles and executes without crashing
        // (print goes to stdout, we don't capture it here)
        let result = engine.execute_jit_nullary("test_print").expect("execute");
        assert_eq!(result, 0);
    }

    #[test]
    fn jit_intrinsic_len_string() {
        // Test len() intrinsic with string argument
        // cell test_len() -> Int
        //   r0 = "hello"
        //   r1 = len(r0)     # Intrinsic(1, 0, 0) - IntrinsicId::Length = 0
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_len".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 2,
            constants: vec![Constant::String("hello".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),        // r0 = "hello"
                Instruction::abc(OpCode::Intrinsic, 1, 0, 0), // r1 = len(r0)
                Instruction::abc(OpCode::Return, 1, 1, 0),    // return r1
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("test_len").expect("execute");
        assert_eq!(result, 5); // len("hello") = 5
    }
}
