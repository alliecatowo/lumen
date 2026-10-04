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

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::atomic::{AtomicI64, Ordering};

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{types, AbiParam, InstBuilder, MemFlags, Type as ClifType};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};

use crate::compiler::lir::{Constant, Instruction, LirCell, LirModule, OpCode};

use crate::codegen::emit::CodegenError;
use crate::codegen::jit_verify::{self, CellPlan};
use crate::codegen::types::lir_type_str_to_cl_type;

/// Maximum number of virtual registers we support per cell.
const MAX_REGS: usize = 256;

// ---------------------------------------------------------------------------
// JIT variable type tracking
// ---------------------------------------------------------------------------

/// Tracks the semantic type of each JIT variable/register.
/// At the Cranelift IR level, both Int and Str are I64, but we need to
/// distinguish them so that operations like Add dispatch to the correct
/// implementation (iadd vs string concatenation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JitVarType {
    /// 64-bit signed integer.
    Int,
    /// 64-bit IEEE 754 floating point.
    Float,
    /// Heap-allocated string, represented as a `*mut String` cast to i64.
    /// The pointer is created by `jit_rt_string_alloc` or `jit_rt_string_concat`
    /// and must be freed via `jit_rt_string_drop` when no longer needed.
    Str,
}

impl JitVarType {
    /// Return the Cranelift IR type for this variable type.
    #[allow(dead_code)]
    fn clif_type(self) -> ClifType {
        match self {
            JitVarType::Int => types::I64,
            JitVarType::Float => types::F64,
            // String pointers are i64 on 64-bit targets.
            JitVarType::Str => types::I64,
        }
    }
}

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

/// Integer exponentiation with the interpreter's exact semantics (`checked_pow`,
/// exponent in `0..=u32::MAX`). On failure it raises the trap flag in the
/// engine's [`JitState`] (address passed as `state`) and returns 0.
extern "C" fn jit_rt_ipow(x: i64, y: i64, state: i64) -> i64 {
    let r = if y < 0 || y > u32::MAX as i64 {
        None
    } else {
        x.checked_pow(y as u32)
    };
    match r {
        Some(v) => v,
        None => {
            // SAFETY: `state` is the address of the engine's boxed state, which
            // outlives all compiled code.
            unsafe {
                (*(state as *const JitState))
                    .trap
                    .store(1, Ordering::Relaxed)
            };
            0
        }
    }
}

/// Register all JIT string runtime helper symbols with a JITBuilder.
fn register_string_helpers(builder: &mut JITBuilder) {
    builder.symbol("jit_rt_ipow", jit_rt_ipow as *const u8);
    builder.symbol("jit_rt_string_alloc", jit_rt_string_alloc as *const u8);
    builder.symbol("jit_rt_string_concat", jit_rt_string_concat as *const u8);
    builder.symbol("jit_rt_string_clone", jit_rt_string_clone as *const u8);
    builder.symbol("jit_rt_string_eq", jit_rt_string_eq as *const u8);
    builder.symbol("jit_rt_string_cmp", jit_rt_string_cmp as *const u8);
    builder.symbol("jit_rt_string_drop", jit_rt_string_drop as *const u8);
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
    /// Native code hit a condition the interpreter must decide (integer
    /// overflow, division by zero, bad shift, stack exhaustion). Compiled cells
    /// are pure, so the caller can safely re-run the call in the interpreter.
    Trap,
}

impl std::fmt::Display for JitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JitError::CompileError(e) => write!(f, "JIT compile error: {e}"),
            JitError::CellNotFound(name) => write!(f, "cell not found: {name}"),
            JitError::ModuleError(msg) => write!(f, "JIT module error: {msg}"),
            JitError::Trap => write!(f, "JIT trap (deferred to interpreter)"),
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

/// How the raw `i64` returned by compiled code must be interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JitReturn {
    /// A 64-bit signed integer.
    Int,
    /// A boolean encoded as 0 / 1.
    Bool,
    /// A heap `*mut String` (experimental tier only).
    Str,
}

/// Mutable state shared between a [`JitEngine`] and the native code it owns.
/// Its address is baked into every compiled function, so it lives in a `Box`.
#[repr(C)]
pub(crate) struct JitState {
    /// Lowest stack address compiled code may use; below it the prologue traps.
    stack_limit: AtomicI64,
    /// Non-zero once native code has requested an interpreter fallback.
    trap: AtomicI64,
}

const STATE_OFF_STACK_LIMIT: i32 = 0;
const STATE_OFF_TRAP: i32 = 8;

/// Native stack budget for one top-level JIT call. Windows' main thread only
/// has 1 MiB, so be more conservative there.
#[cfg(windows)]
const JIT_STACK_BUDGET: i64 = 256 * 1024;
#[cfg(not(windows))]
const JIT_STACK_BUDGET: i64 = 1024 * 1024;

/// Metadata for a JIT-compiled function.
struct CompiledFunction {
    /// Raw function pointer to the compiled native code.
    fn_ptr: *const u8,
    /// Number of parameters the function expects.
    param_count: usize,
    /// How to interpret the returned `i64`.
    ret: JitReturn,
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
    /// Shared trap / stack-limit state read and written by compiled code.
    state: Box<JitState>,
    /// When false (the default) only the strict Int/Bool tier is compiled.
    experimental_types: bool,
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
            state: Box::new(JitState {
                stack_limit: AtomicI64::new(0),
                trap: AtomicI64::new(0),
            }),
            experimental_types: false,
        }
    }

    /// Opt in to the experimental String/Float lowering. It is **not** verified
    /// to match the interpreter (float typing, string ownership on error paths)
    /// and the VM never enables it; it exists for engine-level experiments.
    pub fn with_experimental_types(mut self, enabled: bool) -> Self {
        self.experimental_types = enabled;
        self
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

        let mut jit_module = JITModule::new(builder);
        let pointer_type = jit_module.isa().pointer_type();

        // Lower all cells into the JIT module.
        let state_addr = &*self.state as *const JitState as i64;
        let lowered = lower_module_jit(
            &mut jit_module,
            module,
            pointer_type,
            !self.experimental_types,
            state_addr,
        )?;

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
                    ret: func.ret,
                },
            );
            self.stats.cells_compiled += 1;
        }
        self.stats.cache_size = self.cache.len();

        // Store the JIT module so its memory stays alive.
        self.jit_module = Some(jit_module);

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

    /// Run compiled code for `cell_name`. Resets the trap flag, arms the
    /// stack guard, and converts a raised trap into [`JitError::Trap`].
    fn run(&mut self, cell_name: &str, args: &[i64]) -> Result<i64, JitError> {
        let compiled = self
            .cache
            .get(cell_name)
            .ok_or_else(|| JitError::CellNotFound(cell_name.to_string()))?;
        if compiled.param_count != args.len() {
            return Err(JitError::ModuleError(format!(
                "cell '{cell_name}' expects {} arguments, got {}",
                compiled.param_count,
                args.len()
            )));
        }
        let fn_ptr = compiled.fn_ptr;
        self.stats.executions += 1;

        let marker = 0u8;
        let sp = &marker as *const u8 as i64;
        self.state
            .stack_limit
            .store(sp.saturating_sub(JIT_STACK_BUDGET), Ordering::Relaxed);
        self.state.trap.store(0, Ordering::Relaxed);

        // SAFETY: `fn_ptr` was produced by Cranelift for a function whose
        // signature is `(i64 x param_count) -> i64`; the JITModule that owns the
        // code lives as long as `self`.
        let result = unsafe {
            match args.len() {
                0 => std::mem::transmute::<*const u8, extern "C" fn() -> i64>(fn_ptr)(),
                1 => std::mem::transmute::<*const u8, extern "C" fn(i64) -> i64>(fn_ptr)(args[0]),
                2 => std::mem::transmute::<*const u8, extern "C" fn(i64, i64) -> i64>(fn_ptr)(
                    args[0], args[1],
                ),
                3 => std::mem::transmute::<*const u8, extern "C" fn(i64, i64, i64) -> i64>(fn_ptr)(
                    args[0], args[1], args[2],
                ),
                4 => std::mem::transmute::<*const u8, extern "C" fn(i64, i64, i64, i64) -> i64>(
                    fn_ptr,
                )(args[0], args[1], args[2], args[3]),
                5 => {
                    std::mem::transmute::<*const u8, extern "C" fn(i64, i64, i64, i64, i64) -> i64>(
                        fn_ptr,
                    )(args[0], args[1], args[2], args[3], args[4])
                }
                6 => std::mem::transmute::<
                    *const u8,
                    extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64,
                >(fn_ptr)(args[0], args[1], args[2], args[3], args[4], args[5]),
                n => {
                    return Err(JitError::ModuleError(format!(
                        "unsupported arity {n} for JIT execution (max 6)"
                    )))
                }
            }
        };
        if self.state.trap.swap(0, Ordering::Relaxed) != 0 {
            return Err(JitError::Trap);
        }
        Ok(result)
    }

    /// Execute a JIT-compiled function with no arguments.
    pub fn execute_jit_nullary(&mut self, cell_name: &str) -> Result<i64, JitError> {
        self.run(cell_name, &[])
    }

    /// Execute a JIT-compiled function with one i64 argument.
    pub fn execute_jit_unary(&mut self, cell_name: &str, arg: i64) -> Result<i64, JitError> {
        self.run(cell_name, &[arg])
    }

    /// Execute a JIT-compiled function with two i64 arguments.
    pub fn execute_jit_binary(
        &mut self,
        cell_name: &str,
        arg1: i64,
        arg2: i64,
    ) -> Result<i64, JitError> {
        self.run(cell_name, &[arg1, arg2])
    }

    /// Execute a JIT-compiled function with three i64 arguments.
    pub fn execute_jit_ternary(
        &mut self,
        cell_name: &str,
        arg1: i64,
        arg2: i64,
        arg3: i64,
    ) -> Result<i64, JitError> {
        self.run(cell_name, &[arg1, arg2, arg3])
    }

    /// Generic JIT execution dispatching on arity. Supports 0..=6 i64
    /// arguments. Returns [`JitError::Trap`] if the native code asked for an
    /// interpreter fallback.
    pub fn execute_jit(&mut self, cell_name: &str, args: &[i64]) -> Result<i64, JitError> {
        self.run(cell_name, args)
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
        self.return_kind(cell_name) == Some(JitReturn::Str)
    }

    /// How the `i64` returned by `cell_name` must be interpreted.
    pub fn return_kind(&self, cell_name: &str) -> Option<JitReturn> {
        self.cache.get(cell_name).map(|c| c.ret)
    }
}

// ---------------------------------------------------------------------------
// Pre-scan: check if a cell only uses JIT-supported opcodes
// ---------------------------------------------------------------------------

/// Returns `true` if every instruction in the cell uses an opcode the JIT can
/// compile. Cells containing unsupported opcodes (e.g. Intrinsic, ToolCall,
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
        )
    })
}

// ---------------------------------------------------------------------------
// JIT-specific lowering (mirrors lower.rs but targets JITModule)
// ---------------------------------------------------------------------------

/// Result of lowering an entire LIR module into the JIT.
struct JitLoweredModule {
    functions: Vec<JitLoweredFunction>,
}

struct JitLoweredFunction {
    name: String,
    func_id: FuncId,
    param_count: usize,
    ret: JitReturn,
}

/// Lower an entire LIR module into Cranelift IR inside the given `JITModule`.
/// Cells containing unsupported opcodes are silently skipped — they will
/// remain interpreted.
fn lower_module_jit(
    module: &mut JITModule,
    lir: &LirModule,
    pointer_type: ClifType,
    strict: bool,
    state_addr: i64,
) -> Result<JitLoweredModule, CodegenError> {
    let mut fb_ctx = FunctionBuilderContext::new();

    if strict {
        return lower_module_strict(module, lir, &mut fb_ctx, state_addr);
    }

    // Filter to only JIT-compilable cells.
    let compilable_cells: Vec<&LirCell> = lir
        .cells
        .iter()
        .filter(|c| is_cell_jit_compilable(c))
        .collect();

    if compilable_cells.is_empty() {
        return Ok(JitLoweredModule {
            functions: Vec::new(),
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
    let mut lowered = JitLoweredModule {
        functions: Vec::with_capacity(compilable_cells.len()),
    };

    for cell in &compilable_cells {
        let func_id = func_ids[&cell.name];
        lower_cell_jit(module, cell, &mut fb_ctx, pointer_type, func_id, &func_ids)?;
        let ret_is_string = cell
            .returns
            .as_deref()
            .map(|s| s == "String")
            .unwrap_or(false);
        lowered.functions.push(JitLoweredFunction {
            name: cell.name.clone(),
            func_id,
            param_count: cell.params.len(),
            ret: if ret_is_string {
                JitReturn::Str
            } else {
                JitReturn::Int
            },
        });
    }

    Ok(lowered)
}

// ---------------------------------------------------------------------------
// Pre-scan: identify basic-block boundaries
// ---------------------------------------------------------------------------

fn collect_block_starts(instructions: &[Instruction]) -> BTreeSet<usize> {
    let mut targets = BTreeSet::new();

    for (pc, inst) in instructions.iter().enumerate() {
        match inst.op {
            OpCode::Jmp | OpCode::Break | OpCode::Continue => {
                let offset = inst.sax_val();
                let target = (pc as i32 + 1 + offset) as usize;
                targets.insert(target);
                if pc + 1 < instructions.len() {
                    targets.insert(pc + 1);
                }
            }
            OpCode::Return | OpCode::Halt | OpCode::TailCall if pc + 1 < instructions.len() => {
                targets.insert(pc + 1);
            }
            _ => {}
        }
    }

    targets
}

fn has_self_tail_call(cell: &LirCell) -> bool {
    for (pc, inst) in cell.instructions.iter().enumerate() {
        if inst.op == OpCode::TailCall {
            let base = inst.a;
            if let Some(ref name) = find_callee_name(cell, &cell.instructions, pc, base) {
                if name == &cell.name {
                    return true;
                }
            }
        }
    }
    false
}

fn find_callee_name(
    cell: &LirCell,
    instructions: &[Instruction],
    call_pc: usize,
    base_reg: u8,
) -> Option<String> {
    for i in (0..call_pc).rev() {
        let inst = &instructions[i];
        match inst.op {
            OpCode::LoadK if inst.a == base_reg => {
                let bx = inst.bx() as usize;
                if let Some(Constant::String(name)) = cell.constants.get(bx) {
                    return Some(name.clone());
                }
            }
            OpCode::Move if inst.a == base_reg => {
                return find_callee_name(cell, instructions, i, inst.b);
            }
            OpCode::MoveOwn if inst.a == base_reg => {
                return find_callee_name(cell, instructions, i, inst.b);
            }
            _ => {}
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Per-cell lowering (JIT variant)
// ---------------------------------------------------------------------------

fn lower_cell_jit(
    module: &mut JITModule,
    cell: &LirCell,
    fb_ctx: &mut FunctionBuilderContext,
    pointer_type: ClifType,
    func_id: FuncId,
    func_ids: &HashMap<String, FuncId>,
) -> Result<(), CodegenError> {
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
    let abi_ret = if ret_ty == types::I8 {
        types::I64
    } else {
        ret_ty
    };
    sig.returns.push(AbiParam::new(abi_ret));

    let mut func = cranelift_codegen::ir::Function::with_name_signature(
        cranelift_codegen::ir::UserFuncName::user(0, func_id.as_u32()),
        sig,
    );

    let mut callee_refs: HashMap<FuncId, cranelift_codegen::ir::FuncRef> = HashMap::new();
    for &callee_id in func_ids.values() {
        let func_ref = module.declare_func_in_func(callee_id, &mut func);
        callee_refs.insert(callee_id, func_ref);
    }

    // Declare string runtime helper functions in this function's scope.
    let str_concat_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_concat",
        &[types::I64, types::I64],
        &[types::I64],
    )?;
    let str_alloc_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_alloc",
        &[types::I64, types::I64], // ptr, len
        &[types::I64],
    )?;
    let str_clone_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_clone",
        &[types::I64],
        &[types::I64],
    )?;
    let str_eq_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_eq",
        &[types::I64, types::I64],
        &[types::I64],
    )?;
    let str_cmp_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_cmp",
        &[types::I64, types::I64],
        &[types::I64],
    )?;
    let str_drop_ref =
        declare_helper_func(module, &mut func, "jit_rt_string_drop", &[types::I64], &[])?;

    // Suppress unused-variable warnings for helpers not yet used in all paths.
    let _ = str_clone_ref;

    let mut builder = FunctionBuilder::new(&mut func, fb_ctx);

    let num_regs = (cell.registers as usize)
        .max(cell.params.len())
        .clamp(1, MAX_REGS);
    let mut vars: Vec<Variable> = Vec::with_capacity(num_regs);

    // Track the semantic type of each variable for type-aware code generation.
    let mut var_types: HashMap<u32, JitVarType> = HashMap::new();

    // Pre-scan constants to determine which registers receive float/string values.
    let mut float_regs: std::collections::HashSet<u8> = std::collections::HashSet::new();
    let mut string_regs: std::collections::HashSet<u8> = std::collections::HashSet::new();
    for inst in &cell.instructions {
        if inst.op == OpCode::LoadK {
            let bx = inst.bx() as usize;
            match cell.constants.get(bx) {
                Some(Constant::Float(_)) => {
                    float_regs.insert(inst.a);
                }
                Some(Constant::String(_)) => {
                    string_regs.insert(inst.a);
                }
                _ => {}
            }
        }
    }

    // -----------------------------------------------------------------------
    // Pre-scan: identify registers that hold string constants used ONLY as
    // Call/TailCall callee names in a straight-line code sequence (no branches
    // between the LoadK and the Call). For these registers we skip the heap
    // string allocation entirely, eliminating millions of alloc/free cycles in
    // recursive workloads like fibonacci.
    //
    // For each LoadK String at register R, we walk forward looking for the
    // pattern: LoadK R -> optional Moves -> Call/TailCall with R (or Move-dest)
    // as the base register. If we encounter any branch, return, or other use
    // of R before finding the Call, we conservatively bail out.
    // -----------------------------------------------------------------------
    let call_name_regs: HashSet<u8> = {
        let mut result: HashSet<u8> = HashSet::new();
        let instructions = &cell.instructions;

        for (loadk_pc, loadk_inst) in instructions.iter().enumerate() {
            if loadk_inst.op != OpCode::LoadK {
                continue;
            }
            let bx = loadk_inst.bx() as usize;
            if !matches!(cell.constants.get(bx), Some(Constant::String(_))) {
                continue;
            }

            let origin_reg = loadk_inst.a;
            let mut aliases: HashSet<u8> = HashSet::new();
            aliases.insert(origin_reg);
            let mut found_call_use = false;
            let mut invalidated = false;

            // Walk forward from the instruction after LoadK.
            for inst in &instructions[(loadk_pc + 1)..] {
                match inst.op {
                    OpCode::Call | OpCode::TailCall => {
                        let base = inst.a;
                        let num_args = inst.b as usize;
                        // Check if any alias is used as an argument (not callee name).
                        for i in 0..num_args {
                            let arg_reg = base + 1 + i as u8;
                            if aliases.contains(&arg_reg) {
                                invalidated = true;
                                break;
                            }
                        }
                        if invalidated {
                            break;
                        }
                        // If the base register is an alias, this is a callee-name use.
                        if aliases.contains(&base) {
                            found_call_use = true;
                            aliases.remove(&base);
                        }
                    }
                    OpCode::Move | OpCode::MoveOwn => {
                        let dest = inst.a;
                        let src = inst.b;
                        if aliases.contains(&src) {
                            aliases.insert(dest);
                        } else if aliases.contains(&dest) {
                            // dest is being overwritten from a non-alias source.
                            aliases.remove(&dest);
                        }
                    }
                    OpCode::LoadK | OpCode::LoadBool | OpCode::LoadInt | OpCode::LoadNil => {
                        // Redefines inst.a — kill the alias if present.
                        aliases.remove(&inst.a);
                    }
                    OpCode::Jmp | OpCode::Break | OpCode::Continue => {
                        // Branch instruction — we can't safely track aliases
                        // across control flow. If any alias is still live,
                        // conservatively bail out.
                        if !aliases.is_empty() {
                            invalidated = true;
                        }
                        break;
                    }
                    OpCode::Halt
                    | OpCode::Nop
                    | OpCode::Loop
                    | OpCode::ForPrep
                    | OpCode::ForLoop
                    | OpCode::ForIn => {
                        // No register reads/writes of concern.
                    }
                    OpCode::Test => {
                        if aliases.contains(&inst.a) {
                            invalidated = true;
                            break;
                        }
                    }
                    OpCode::Return => {
                        if aliases.contains(&inst.a) {
                            invalidated = true;
                        }
                        break;
                    }
                    _ => {
                        // Arithmetic, etc. — read b/c, write a.
                        if aliases.contains(&inst.b) || aliases.contains(&inst.c) {
                            invalidated = true;
                            break;
                        }
                        aliases.remove(&inst.a);
                    }
                }

                if aliases.is_empty() {
                    break;
                }
            }

            if found_call_use && !invalidated {
                result.insert(origin_reg);
                // Also include Move destinations of this origin in the result,
                // so the Move handler skips string clone for them too.
                let mut propagated: HashSet<u8> = HashSet::new();
                propagated.insert(origin_reg);
                for inst in &instructions[(loadk_pc + 1)..] {
                    if (inst.op == OpCode::Move || inst.op == OpCode::MoveOwn)
                        && propagated.contains(&inst.b)
                    {
                        propagated.insert(inst.a);
                    }
                    // Stop propagating through redefinitions.
                    if matches!(
                        inst.op,
                        OpCode::LoadK | OpCode::LoadBool | OpCode::LoadInt | OpCode::LoadNil
                    ) {
                        propagated.remove(&inst.a);
                    }
                    if matches!(inst.op, OpCode::Call | OpCode::TailCall)
                        && propagated.contains(&inst.a)
                    {
                        propagated.remove(&inst.a);
                    }
                    // Stop at branches.
                    if matches!(inst.op, OpCode::Jmp | OpCode::Break | OpCode::Continue) {
                        break;
                    }
                }
                result.extend(propagated);
            }
        }

        result
    };

    // All Cranelift variables are declared as I64 (both ints and string pointers
    // are I64; only floats are F64). The semantic distinction is in var_types.
    for i in 0..num_regs {
        let (var_ty, clif_ty) = if i < cell.params.len() {
            let param_ty_str = &cell.params[i].ty;
            if param_ty_str == "Float" {
                (JitVarType::Float, types::F64)
            } else if param_ty_str == "String" {
                (JitVarType::Str, types::I64)
            } else {
                (JitVarType::Int, types::I64)
            }
        } else if float_regs.contains(&(i as u8)) {
            (JitVarType::Float, types::F64)
        } else {
            // Both int and string regs use I64 at the Cranelift level.
            // The semantic type for string regs is set later when LoadK executes.
            (JitVarType::Int, types::I64)
        };
        let var = builder.declare_var(clif_ty);
        var_types.insert(i as u32, var_ty);
        vars.push(var);
    }

    let self_tco = has_self_tail_call(cell);
    let block_starts = collect_block_starts(&cell.instructions);

    let entry_block = builder.create_block();
    builder.append_block_params_for_function_params(entry_block);
    builder.switch_to_block(entry_block);

    for (i, _param) in cell.params.iter().enumerate() {
        if i < vars.len() {
            let val = builder.block_params(entry_block)[i];
            builder.def_var(vars[i], val);
        }
    }

    {
        for (i, var) in vars
            .iter()
            .enumerate()
            .take(num_regs)
            .skip(cell.params.len())
        {
            let vty = var_types
                .get(&(i as u32))
                .copied()
                .unwrap_or(JitVarType::Int);
            if vty == JitVarType::Float {
                let zero = builder.ins().f64const(0.0);
                builder.def_var(*var, zero);
            } else {
                // Both Int and Str use I64; strings start as null pointers (0).
                let zero = builder.ins().iconst(types::I64, 0);
                builder.def_var(*var, zero);
            }
        }
    }

    let tco_loop_block = if self_tco {
        let loop_block = builder.create_block();
        builder.ins().jump(loop_block, &[]);
        builder.switch_to_block(loop_block);
        Some(loop_block)
    } else {
        None
    };

    let mut block_map: HashMap<usize, cranelift_codegen::ir::Block> = HashMap::new();
    for &pc in &block_starts {
        let blk = builder.create_block();
        block_map.insert(pc, blk);
    }

    let mut terminated = false;
    let mut pending_test: Option<u8> = None;

    for (pc, inst) in cell.instructions.iter().enumerate() {
        if let Some(&target_block) = block_map.get(&pc) {
            if !terminated {
                builder.ins().jump(target_block, &[]);
            }
            builder.switch_to_block(target_block);
            terminated = false;
        }

        if terminated {
            continue;
        }

        match inst.op {
            OpCode::LoadK => {
                let a = inst.a;
                let bx = inst.bx() as usize;
                if let Some(constant) = cell.constants.get(bx) {
                    match constant {
                        Constant::String(s) => {
                            if call_name_regs.contains(&a) {
                                // This register is only used as a Call/TailCall
                                // base (callee name). The Call handler resolves
                                // the name at compile time, so we can skip the
                                // heap string allocation entirely.
                                let dummy = builder.ins().iconst(types::I64, 0);
                                var_types.insert(a as u32, JitVarType::Int);
                                def_var(&mut builder, &vars, a, dummy);
                            } else {
                                // Drop the old string value if the dest register held one.
                                if var_types.get(&(a as u32)) == Some(&JitVarType::Str) {
                                    let old = use_var(&mut builder, &vars, a);
                                    builder.ins().call(str_drop_ref, &[old]);
                                }
                                // Allocate the string on the heap via runtime helper.
                                let str_bytes = s.as_bytes();
                                let ptr_val =
                                    builder.ins().iconst(types::I64, str_bytes.as_ptr() as i64);
                                let len_val =
                                    builder.ins().iconst(types::I64, str_bytes.len() as i64);
                                let call = builder.ins().call(str_alloc_ref, &[ptr_val, len_val]);
                                let result = builder.inst_results(call)[0];
                                var_types.insert(a as u32, JitVarType::Str);
                                def_var(&mut builder, &vars, a, result);
                            }
                        }
                        Constant::Float(_) => {
                            let val = lower_constant(&mut builder, cell, bx)?;
                            var_types.insert(a as u32, JitVarType::Float);
                            def_var(&mut builder, &vars, a, val);
                        }
                        _ => {
                            let val = lower_constant(&mut builder, cell, bx)?;
                            var_types.insert(a as u32, JitVarType::Int);
                            def_var(&mut builder, &vars, a, val);
                        }
                    }
                } else {
                    let val = lower_constant(&mut builder, cell, bx)?;
                    var_types.insert(a as u32, JitVarType::Int);
                    def_var(&mut builder, &vars, a, val);
                }
            }
            OpCode::LoadBool => {
                let a = inst.a;
                let b_val = inst.b;
                let val = builder.ins().iconst(types::I64, b_val as i64);
                def_var(&mut builder, &vars, a, val);
            }
            OpCode::LoadInt => {
                let a = inst.a;
                let imm = inst.sbx() as i64;
                let val = builder.ins().iconst(types::I64, imm);
                def_var(&mut builder, &vars, a, val);
            }
            OpCode::LoadNil => {
                let a = inst.a;
                let count = inst.b as usize;
                let zero = builder.ins().iconst(types::I64, 0);
                for i in 0..=count {
                    let r = a as usize + i;
                    if r < vars.len() {
                        builder.def_var(vars[r], zero);
                    }
                }
            }
            OpCode::Move => {
                let src_ty = var_types
                    .get(&(inst.b as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let val = if src_ty == JitVarType::Str && !call_name_regs.contains(&inst.b) {
                    // Drop old destination string if it held one and differs from source.
                    if var_types.get(&(inst.a as u32)) == Some(&JitVarType::Str) && inst.a != inst.b
                    {
                        let old = use_var(&mut builder, &vars, inst.a);
                        builder.ins().call(str_drop_ref, &[old]);
                    }
                    // Clone the string so both source and dest own independent copies.
                    let src = use_var(&mut builder, &vars, inst.b);
                    let call = builder.ins().call(str_clone_ref, &[src]);
                    builder.inst_results(call)[0]
                } else {
                    use_var(&mut builder, &vars, inst.b)
                };
                let actual_ty = if call_name_regs.contains(&inst.b) {
                    JitVarType::Int
                } else {
                    src_ty
                };
                var_types.insert(inst.a as u32, actual_ty);
                def_var(&mut builder, &vars, inst.a, val);
            }
            OpCode::MoveOwn => {
                // MoveOwn transfers ownership — no clone needed even for strings.
                let val = use_var(&mut builder, &vars, inst.b);
                if let Some(&src_ty) = var_types.get(&(inst.b as u32)) {
                    var_types.insert(inst.a as u32, src_ty);
                    // For strings, null out the source register so the
                    // Return-time cleanup doesn't double-free the pointer.
                    if src_ty == JitVarType::Str && inst.a != inst.b {
                        let null = builder.ins().iconst(types::I64, 0);
                        def_var(&mut builder, &vars, inst.b, null);
                    }
                }
                def_var(&mut builder, &vars, inst.a, val);
            }

            // Arithmetic (type-aware: Int uses iadd/isub/etc., Float uses fadd/fsub/etc., Str uses concat)
            OpCode::Add => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let lhs_ty = var_types
                    .get(&(inst.b as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let rhs_ty = var_types
                    .get(&(inst.c as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                if lhs_ty == JitVarType::Str || rhs_ty == JitVarType::Str {
                    // Read the old destination value before overwriting, if it
                    // was a string that is distinct from both inputs. We'll
                    // drop it after the concat to prevent a leak.
                    let dest_ty = var_types
                        .get(&(inst.a as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    let old_dest =
                        if dest_ty == JitVarType::Str && inst.a != inst.b && inst.a != inst.c {
                            Some(use_var(&mut builder, &vars, inst.a))
                        } else {
                            None
                        };

                    // String concatenation via runtime helper.
                    let call = builder.ins().call(str_concat_ref, &[lhs, rhs]);
                    let result = builder.inst_results(call)[0];

                    // Drop the old destination string if it was distinct from
                    // the inputs. For a == b (self-assign like s = s + "x"),
                    // the old value is the `lhs` input — drop it instead.
                    if let Some(old) = old_dest {
                        builder.ins().call(str_drop_ref, &[old]);
                    } else if dest_ty == JitVarType::Str && inst.a == inst.b {
                        // self-assign: old value was `lhs`, safe to drop after concat
                        builder.ins().call(str_drop_ref, &[lhs]);
                    } else if dest_ty == JitVarType::Str && inst.a == inst.c {
                        builder.ins().call(str_drop_ref, &[rhs]);
                    }

                    var_types.insert(inst.a as u32, JitVarType::Str);
                    def_var(&mut builder, &vars, inst.a, result);
                } else if lhs_ty == JitVarType::Float || rhs_ty == JitVarType::Float {
                    let res = builder.ins().fadd(lhs, rhs);
                    var_types.insert(inst.a as u32, JitVarType::Float);
                    def_var(&mut builder, &vars, inst.a, res);
                } else {
                    let res = builder.ins().iadd(lhs, rhs);
                    var_types.insert(inst.a as u32, JitVarType::Int);
                    def_var(&mut builder, &vars, inst.a, res);
                }
            }
            OpCode::Sub => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let is_float = is_float_op_jvt(&var_types, inst.b, inst.c);
                let res = if is_float {
                    builder.ins().fsub(lhs, rhs)
                } else {
                    builder.ins().isub(lhs, rhs)
                };
                var_types.insert(
                    inst.a as u32,
                    if is_float {
                        JitVarType::Float
                    } else {
                        JitVarType::Int
                    },
                );
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::Mul => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let is_float = is_float_op_jvt(&var_types, inst.b, inst.c);
                let res = if is_float {
                    builder.ins().fmul(lhs, rhs)
                } else {
                    builder.ins().imul(lhs, rhs)
                };
                var_types.insert(
                    inst.a as u32,
                    if is_float {
                        JitVarType::Float
                    } else {
                        JitVarType::Int
                    },
                );
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::Div => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let is_float = is_float_op_jvt(&var_types, inst.b, inst.c);
                let res = if is_float {
                    builder.ins().fdiv(lhs, rhs)
                } else {
                    builder.ins().sdiv(lhs, rhs)
                };
                var_types.insert(
                    inst.a as u32,
                    if is_float {
                        JitVarType::Float
                    } else {
                        JitVarType::Int
                    },
                );
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::Mod => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                // Cranelift doesn't have a direct float modulo; keep integer path.
                let res = builder.ins().srem(lhs, rhs);
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::Neg => {
                let operand = use_var(&mut builder, &vars, inst.b);
                let is_float = var_types.get(&(inst.b as u32)).copied() == Some(JitVarType::Float);
                let res = if is_float {
                    builder.ins().fneg(operand)
                } else {
                    builder.ins().ineg(operand)
                };
                var_types.insert(
                    inst.a as u32,
                    if is_float {
                        JitVarType::Float
                    } else {
                        JitVarType::Int
                    },
                );
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::FloorDiv => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let is_float = is_float_op_jvt(&var_types, inst.b, inst.c);
                let res = if is_float {
                    // floor(a / b)
                    let div = builder.ins().fdiv(lhs, rhs);
                    builder.ins().floor(div)
                } else {
                    builder.ins().sdiv(lhs, rhs)
                };
                var_types.insert(
                    inst.a as u32,
                    if is_float {
                        JitVarType::Float
                    } else {
                        JitVarType::Int
                    },
                );
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::Pow => {
                let is_float = is_float_op_jvt(&var_types, inst.b, inst.c);
                if is_float {
                    let zero = builder.ins().f64const(0.0);
                    var_types.insert(inst.a as u32, JitVarType::Float);
                    def_var(&mut builder, &vars, inst.a, zero);
                } else {
                    let zero = builder.ins().iconst(types::I64, 0);
                    var_types.insert(inst.a as u32, JitVarType::Int);
                    def_var(&mut builder, &vars, inst.a, zero);
                }
            }

            // Bitwise
            OpCode::BitOr => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let res = builder.ins().bor(lhs, rhs);
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::BitAnd => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let res = builder.ins().band(lhs, rhs);
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::BitXor => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let res = builder.ins().bxor(lhs, rhs);
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::BitNot => {
                let operand = use_var(&mut builder, &vars, inst.b);
                let res = builder.ins().bnot(operand);
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::Shl => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let res = builder.ins().ishl(lhs, rhs);
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::Shr => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let res = builder.ins().sshr(lhs, rhs);
                def_var(&mut builder, &vars, inst.a, res);
            }

            // Comparison (type-aware: Float uses fcmp, Str uses runtime helpers)
            OpCode::Eq => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let lhs_ty = var_types
                    .get(&(inst.b as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let rhs_ty = var_types
                    .get(&(inst.c as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let res = if lhs_ty == JitVarType::Str || rhs_ty == JitVarType::Str {
                    let call = builder.ins().call(str_eq_ref, &[lhs, rhs]);
                    builder.inst_results(call)[0]
                } else if lhs_ty == JitVarType::Float || rhs_ty == JitVarType::Float {
                    let cmp = builder.ins().fcmp(FloatCC::Equal, lhs, rhs);
                    builder.ins().uextend(types::I64, cmp)
                } else {
                    let cmp = builder.ins().icmp(IntCC::Equal, lhs, rhs);
                    builder.ins().uextend(types::I64, cmp)
                };
                var_types.insert(inst.a as u32, JitVarType::Int);
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::Lt => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let lhs_ty = var_types
                    .get(&(inst.b as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let rhs_ty = var_types
                    .get(&(inst.c as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let res = if lhs_ty == JitVarType::Str || rhs_ty == JitVarType::Str {
                    // str_cmp returns -1/0/1; Lt means cmp < 0.
                    let call = builder.ins().call(str_cmp_ref, &[lhs, rhs]);
                    let cmp_result = builder.inst_results(call)[0];
                    let zero = builder.ins().iconst(types::I64, 0);
                    let lt = builder.ins().icmp(IntCC::SignedLessThan, cmp_result, zero);
                    builder.ins().uextend(types::I64, lt)
                } else if lhs_ty == JitVarType::Float || rhs_ty == JitVarType::Float {
                    let cmp = builder.ins().fcmp(FloatCC::LessThan, lhs, rhs);
                    builder.ins().uextend(types::I64, cmp)
                } else {
                    let cmp = builder.ins().icmp(IntCC::SignedLessThan, lhs, rhs);
                    builder.ins().uextend(types::I64, cmp)
                };
                var_types.insert(inst.a as u32, JitVarType::Int);
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::Le => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let lhs_ty = var_types
                    .get(&(inst.b as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let rhs_ty = var_types
                    .get(&(inst.c as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let res = if lhs_ty == JitVarType::Str || rhs_ty == JitVarType::Str {
                    // str_cmp returns -1/0/1; Le means cmp <= 0.
                    let call = builder.ins().call(str_cmp_ref, &[lhs, rhs]);
                    let cmp_result = builder.inst_results(call)[0];
                    let zero = builder.ins().iconst(types::I64, 0);
                    let le = builder
                        .ins()
                        .icmp(IntCC::SignedLessThanOrEqual, cmp_result, zero);
                    builder.ins().uextend(types::I64, le)
                } else if lhs_ty == JitVarType::Float || rhs_ty == JitVarType::Float {
                    let cmp = builder.ins().fcmp(FloatCC::LessThanOrEqual, lhs, rhs);
                    builder.ins().uextend(types::I64, cmp)
                } else {
                    let cmp = builder.ins().icmp(IntCC::SignedLessThanOrEqual, lhs, rhs);
                    builder.ins().uextend(types::I64, cmp)
                };
                var_types.insert(inst.a as u32, JitVarType::Int);
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::Not => {
                let operand = use_var(&mut builder, &vars, inst.b);
                let zero = builder.ins().iconst(types::I64, 0);
                let cmp = builder.ins().icmp(IntCC::Equal, operand, zero);
                let res = builder.ins().uextend(types::I64, cmp);
                def_var(&mut builder, &vars, inst.a, res);
            }

            // Logic
            OpCode::And => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let res = builder.ins().band(lhs, rhs);
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::Or => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let res = builder.ins().bor(lhs, rhs);
                def_var(&mut builder, &vars, inst.a, res);
            }

            // Test
            OpCode::Test => {
                pending_test = Some(inst.a);
            }

            // Control flow
            OpCode::Jmp | OpCode::Break | OpCode::Continue => {
                let offset = inst.sax_val();
                let target_pc = (pc as i32 + 1 + offset) as usize;
                let fallthrough_pc = pc + 1;

                let target_block = get_or_create_block(&mut builder, &mut block_map, target_pc);
                let fallthrough_block =
                    get_or_create_block(&mut builder, &mut block_map, fallthrough_pc);

                if let Some(test_reg) = pending_test.take() {
                    let cond = use_var(&mut builder, &vars, test_reg);
                    let zero = builder.ins().iconst(types::I64, 0);
                    let is_truthy = builder.ins().icmp(IntCC::NotEqual, cond, zero);
                    builder
                        .ins()
                        .brif(is_truthy, fallthrough_block, &[], target_block, &[]);
                } else {
                    builder.ins().jump(target_block, &[]);
                }
                terminated = true;
            }

            // Return / Halt
            OpCode::Return => {
                // Drop all live string registers except the return value to
                // prevent memory leaks. The return value ownership transfers
                // to the caller (VM converts it back via jit_take_string).
                let ret_reg = inst.a;
                for (&reg_id, &ty) in &var_types {
                    if ty == JitVarType::Str
                        && reg_id != ret_reg as u32
                        && (reg_id as usize) < vars.len()
                    {
                        let v = use_var(&mut builder, &vars, reg_id as u8);
                        builder.ins().call(str_drop_ref, &[v]);
                    }
                }
                let val = use_var(&mut builder, &vars, ret_reg);
                builder.ins().return_(&[val]);
                terminated = true;
            }
            OpCode::Halt => {
                builder
                    .ins()
                    .trap(cranelift_codegen::ir::TrapCode::unwrap_user(1));
                terminated = true;
            }

            // Function calls
            OpCode::Call => {
                let base = inst.a;
                let num_args = inst.b as usize;
                let callee_name = find_callee_name(cell, &cell.instructions, pc, base);

                // Drop the old string in the base register before overwriting
                // with the call result (base typically holds the callee name).
                if var_types.get(&(base as u32)) == Some(&JitVarType::Str) {
                    let old = use_var(&mut builder, &vars, base);
                    builder.ins().call(str_drop_ref, &[old]);
                }

                // Collect which argument registers are string-typed so we can
                // drop them AFTER the call instruction (not before, to avoid
                // use-after-free when the call reads the argument values).
                let mut str_arg_regs: Vec<u32> = Vec::new();
                for i in 0..num_args {
                    let arg_reg = (base + 1 + i as u8) as u32;
                    if var_types.get(&arg_reg) == Some(&JitVarType::Str) {
                        str_arg_regs.push(arg_reg);
                    }
                }

                if let Some(ref name) = callee_name {
                    if let Some(&callee_func_id) = func_ids.get(name.as_str()) {
                        if let Some(&func_ref) = callee_refs.get(&callee_func_id) {
                            let mut args: Vec<cranelift_codegen::ir::Value> =
                                Vec::with_capacity(num_args);
                            for i in 0..num_args {
                                let arg_reg = base + 1 + i as u8;
                                args.push(use_var(&mut builder, &vars, arg_reg));
                            }
                            let call = builder.ins().call(func_ref, &args);
                            let result = builder.inst_results(call)[0];
                            def_var(&mut builder, &vars, base, result);
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            def_var(&mut builder, &vars, base, zero);
                        }
                    } else {
                        let zero = builder.ins().iconst(types::I64, 0);
                        def_var(&mut builder, &vars, base, zero);
                    }
                } else {
                    let zero = builder.ins().iconst(types::I64, 0);
                    def_var(&mut builder, &vars, base, zero);
                }

                // Now drop string-typed argument registers AFTER the call
                // has read them, and remove from var_types so Return cleanup
                // doesn't double-free.
                for arg_reg in str_arg_regs {
                    if (arg_reg as usize) < vars.len() {
                        let v = use_var(&mut builder, &vars, arg_reg as u8);
                        builder.ins().call(str_drop_ref, &[v]);
                    }
                    var_types.remove(&arg_reg);
                }

                // The call result is an integer (or float), not a string.
                // Update var_types so Return cleanup doesn't try to drop it.
                var_types.insert(base as u32, JitVarType::Int);
            }
            OpCode::TailCall => {
                let base = inst.a;
                let num_args = inst.b as usize;
                let callee_name = find_callee_name(cell, &cell.instructions, pc, base);

                let is_self_call = callee_name
                    .as_ref()
                    .map(|n| n == &cell.name)
                    .unwrap_or(false);

                // Drop the callee name string in base before overwriting.
                if var_types.get(&(base as u32)) == Some(&JitVarType::Str) {
                    let old = use_var(&mut builder, &vars, base);
                    builder.ins().call(str_drop_ref, &[old]);
                    var_types.remove(&(base as u32));
                }

                // Drop any string-typed argument registers consumed by the call.
                for i in 0..num_args {
                    let arg_reg = (base + 1 + i as u8) as u32;
                    if var_types.get(&arg_reg) == Some(&JitVarType::Str) {
                        if (arg_reg as usize) < vars.len() {
                            let v = use_var(&mut builder, &vars, arg_reg as u8);
                            builder.ins().call(str_drop_ref, &[v]);
                        }
                        var_types.remove(&arg_reg);
                    }
                }

                if is_self_call && self_tco {
                    if let Some(loop_block) = tco_loop_block {
                        let mut new_args: Vec<cranelift_codegen::ir::Value> =
                            Vec::with_capacity(num_args);
                        for i in 0..num_args {
                            let arg_reg = base + 1 + i as u8;
                            new_args.push(use_var(&mut builder, &vars, arg_reg));
                        }
                        for (i, &val) in new_args.iter().enumerate() {
                            if i < vars.len() {
                                builder.def_var(vars[i], val);
                            }
                        }
                        builder.ins().jump(loop_block, &[]);
                        terminated = true;
                    }
                } else if let Some(ref name) = callee_name {
                    if let Some(&callee_func_id) = func_ids.get(name.as_str()) {
                        if let Some(&func_ref) = callee_refs.get(&callee_func_id) {
                            let mut args: Vec<cranelift_codegen::ir::Value> =
                                Vec::with_capacity(num_args);
                            for i in 0..num_args {
                                let arg_reg = base + 1 + i as u8;
                                args.push(use_var(&mut builder, &vars, arg_reg));
                            }
                            let call = builder.ins().call(func_ref, &args);
                            let result = builder.inst_results(call)[0];
                            builder.ins().return_(&[result]);
                            terminated = true;
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            builder.ins().return_(&[zero]);
                            terminated = true;
                        }
                    } else {
                        let zero = builder.ins().iconst(types::I64, 0);
                        builder.ins().return_(&[zero]);
                        terminated = true;
                    }
                } else {
                    let zero = builder.ins().iconst(types::I64, 0);
                    builder.ins().return_(&[zero]);
                    terminated = true;
                }
            }

            // Legacy loop opcodes
            OpCode::Loop | OpCode::ForPrep | OpCode::ForLoop | OpCode::ForIn => {}

            OpCode::Nop => {}

            // Everything else -> error (should be unreachable due to pre-scan).
            _ => {
                return Err(CodegenError::LoweringError(format!(
                    "unsupported opcode {:?} in cell '{}' (should have been filtered by pre-scan)",
                    inst.op, cell.name
                )));
            }
        }
    }

    if !terminated {
        let zero = builder.ins().iconst(types::I64, 0);
        builder.ins().return_(&[zero]);
    }

    builder.seal_all_blocks();
    builder.finalize();

    let mut ctx = Context::for_function(func);
    module
        .define_function(func_id, &mut ctx)
        .map_err(|e| CodegenError::LoweringError(format!("define_function({}): {e}", cell.name)))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Strict tier: provably interpreter-equivalent Int/Bool cells
// ---------------------------------------------------------------------------

/// Lower every cell accepted by [`jit_verify::eligible_cells`].
fn lower_module_strict(
    module: &mut JITModule,
    lir: &LirModule,
    fb_ctx: &mut FunctionBuilderContext,
    state_addr: i64,
) -> Result<JitLoweredModule, CodegenError> {
    let eligible = jit_verify::eligible_cells(&lir.cells);
    let cells: Vec<&LirCell> = lir
        .cells
        .iter()
        .filter(|c| eligible.contains_key(&c.name))
        .collect();
    let mut lowered = JitLoweredModule {
        functions: Vec::with_capacity(cells.len()),
    };
    if cells.is_empty() {
        return Ok(lowered);
    }

    let mut func_ids: HashMap<String, FuncId> = HashMap::new();
    for cell in &cells {
        let mut sig = module.make_signature();
        for _ in &cell.params {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I64));
        let id = module
            .declare_function(&cell.name, Linkage::Export, &sig)
            .map_err(|e| {
                CodegenError::LoweringError(format!("declare_function({}): {e}", cell.name))
            })?;
        func_ids.insert(cell.name.clone(), id);
    }

    for cell in &cells {
        let (sig, plan) = &eligible[&cell.name];
        lower_cell_strict(
            module,
            cell,
            plan,
            fb_ctx,
            func_ids[&cell.name],
            &func_ids,
            state_addr,
        )?;
        lowered.functions.push(JitLoweredFunction {
            name: cell.name.clone(),
            func_id: func_ids[&cell.name],
            param_count: cell.params.len(),
            ret: match sig.ret {
                jit_verify::ScalarTy::Int => JitReturn::Int,
                jit_verify::ScalarTy::Bool => JitReturn::Bool,
            },
        });
    }
    Ok(lowered)
}

/// Branch to `trap_blk` when `cond` (an i8 boolean) is non-zero, otherwise
/// continue in a fresh block.
fn trap_if(
    builder: &mut FunctionBuilder,
    cond: cranelift_codegen::ir::Value,
    trap_blk: cranelift_codegen::ir::Block,
) {
    let cont = builder.create_block();
    builder.ins().brif(cond, trap_blk, &[], cont, &[]);
    builder.switch_to_block(cont);
}

/// Trap if the engine's trap flag has been raised (after a call or helper).
fn trap_if_flagged(
    builder: &mut FunctionBuilder,
    state_addr: i64,
    trap_blk: cranelift_codegen::ir::Block,
) {
    let st = builder.ins().iconst(types::I64, state_addr);
    let flag = builder
        .ins()
        .load(types::I64, MemFlags::trusted(), st, STATE_OFF_TRAP);
    let set = builder.ins().icmp_imm(IntCC::NotEqual, flag, 0);
    trap_if(builder, set, trap_blk);
}

fn lower_cell_strict(
    module: &mut JITModule,
    cell: &LirCell,
    plan: &CellPlan,
    fb_ctx: &mut FunctionBuilderContext,
    func_id: FuncId,
    func_ids: &HashMap<String, FuncId>,
    state_addr: i64,
) -> Result<(), CodegenError> {
    use cranelift_codegen::ir::Block;

    let mut sig = module.make_signature();
    for _ in &cell.params {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));
    let mut func = cranelift_codegen::ir::Function::with_name_signature(
        cranelift_codegen::ir::UserFuncName::user(0, func_id.as_u32()),
        sig,
    );

    let mut callee_refs: HashMap<String, cranelift_codegen::ir::FuncRef> = HashMap::new();
    for name in plan.callees.values() {
        if !callee_refs.contains_key(name) {
            let r = module.declare_func_in_func(func_ids[name], &mut func);
            callee_refs.insert(name.clone(), r);
        }
    }
    let pow_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_ipow",
        &[types::I64, types::I64, types::I64],
        &[types::I64],
    )?;

    let mut builder = FunctionBuilder::new(&mut func, fb_ctx);
    let nregs = (cell.registers as usize).max(cell.params.len()).max(1);
    let vars: Vec<Variable> = (0..nregs)
        .map(|_| builder.declare_var(types::I64))
        .collect();

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    for (i, var) in vars.iter().enumerate() {
        let v = if i < cell.params.len() {
            builder.block_params(entry)[i]
        } else {
            builder.ins().iconst(types::I64, 0)
        };
        builder.def_var(*var, v);
    }

    let trap_blk = builder.create_block();

    // Prologue: refuse to run if the native stack budget is exhausted.
    let st = builder.ins().iconst(types::I64, state_addr);
    let sp = builder.ins().get_stack_pointer(types::I64);
    let lim = builder
        .ins()
        .load(types::I64, MemFlags::trusted(), st, STATE_OFF_STACK_LIMIT);
    let over = builder.ins().icmp(IntCC::UnsignedLessThan, sp, lim);
    trap_if(&mut builder, over, trap_blk);

    let has_self_tail = plan
        .callees
        .iter()
        .any(|(pc, name)| cell.instructions[*pc].op == OpCode::TailCall && name == &cell.name);
    let loop_blk: Option<Block> = if has_self_tail {
        let b = builder.create_block();
        builder.ins().jump(b, &[]);
        builder.switch_to_block(b);
        Some(b)
    } else {
        None
    };

    // Basic-block leaders among reachable instructions.
    let mut leaders: BTreeSet<usize> = BTreeSet::new();
    for (pc, inst) in cell.instructions.iter().enumerate() {
        if !plan.reachable[pc] {
            continue;
        }
        match inst.op {
            OpCode::Jmp | OpCode::Break | OpCode::Continue => {
                leaders.insert((pc as i64 + 1 + inst.sax_val() as i64) as usize);
            }
            OpCode::Test => {
                leaders.insert(pc + 1);
                leaders.insert(pc + 2);
            }
            OpCode::LoadBool if inst.c != 0 => {
                leaders.insert(pc + 2);
            }
            _ => {}
        }
    }
    let blocks: HashMap<usize, Block> = leaders
        .iter()
        .map(|&pc| (pc, builder.create_block()))
        .collect();

    let mut terminated = false;
    for (pc, inst) in cell.instructions.iter().enumerate() {
        if !plan.reachable[pc] {
            continue;
        }
        if let Some(&blk) = blocks.get(&pc) {
            if !terminated {
                builder.ins().jump(blk, &[]);
            }
            builder.switch_to_block(blk);
            terminated = false;
        }
        debug_assert!(
            !terminated,
            "reachable pc {pc} follows a terminator without a block"
        );

        let use_v = |b: &mut FunctionBuilder, r: u8| b.use_var(vars[r as usize]);
        match inst.op {
            OpCode::Nop => {}
            OpCode::LoadK => {
                let v = match &cell.constants[inst.bx() as usize] {
                    Constant::Int(n) => *n,
                    Constant::Bool(x) => *x as i64,
                    // Callee-name placeholder; never read as a value.
                    _ => 0,
                };
                let c = builder.ins().iconst(types::I64, v);
                builder.def_var(vars[inst.a as usize], c);
            }
            OpCode::LoadBool => {
                let c = builder.ins().iconst(types::I64, (inst.b != 0) as i64);
                builder.def_var(vars[inst.a as usize], c);
                if inst.c != 0 {
                    builder.ins().jump(blocks[&(pc + 2)], &[]);
                    terminated = true;
                }
            }
            OpCode::LoadInt => {
                let c = builder.ins().iconst(types::I64, inst.sbx() as i64);
                builder.def_var(vars[inst.a as usize], c);
            }
            OpCode::Move | OpCode::MoveOwn => {
                let v = use_v(&mut builder, inst.b);
                builder.def_var(vars[inst.a as usize], v);
            }
            OpCode::Add | OpCode::Sub | OpCode::Mul => {
                let l = use_v(&mut builder, inst.b);
                let r = use_v(&mut builder, inst.c);
                let (res, of) = match inst.op {
                    OpCode::Add => builder.ins().sadd_overflow(l, r),
                    OpCode::Sub => builder.ins().ssub_overflow(l, r),
                    _ => builder.ins().smul_overflow(l, r),
                };
                trap_if(&mut builder, of, trap_blk);
                builder.def_var(vars[inst.a as usize], res);
            }
            OpCode::Neg => {
                let x = use_v(&mut builder, inst.b);
                let zero = builder.ins().iconst(types::I64, 0);
                let (res, of) = builder.ins().ssub_overflow(zero, x);
                trap_if(&mut builder, of, trap_blk);
                builder.def_var(vars[inst.a as usize], res);
            }
            OpCode::Div | OpCode::Mod | OpCode::FloorDiv => {
                let l = use_v(&mut builder, inst.b);
                let r = use_v(&mut builder, inst.c);
                // Division by zero and i64::MIN / -1 are decided by the interpreter.
                let is_zero = builder.ins().icmp_imm(IntCC::Equal, r, 0);
                let is_m1 = builder.ins().icmp_imm(IntCC::Equal, r, -1);
                let is_min = builder.ins().icmp_imm(IntCC::Equal, l, i64::MIN);
                let ovf = builder.ins().band(is_m1, is_min);
                let bad = builder.ins().bor(is_zero, ovf);
                trap_if(&mut builder, bad, trap_blk);
                let res = match inst.op {
                    // Truncating division, like `checked_div`.
                    OpCode::Div => builder.ins().sdiv(l, r),
                    // `rem_euclid`: remainder is always >= 0.
                    OpCode::Mod => {
                        let rem = builder.ins().srem(l, r);
                        let rem_neg = builder.ins().icmp_imm(IntCC::SignedLessThan, rem, 0);
                        let r_neg = builder.ins().icmp_imm(IntCC::SignedLessThan, r, 0);
                        let up = builder.ins().iadd(rem, r);
                        let down = builder.ins().isub(rem, r);
                        let fixed = builder.ins().select(r_neg, down, up);
                        builder.ins().select(rem_neg, fixed, rem)
                    }
                    // `div_euclid`.
                    _ => {
                        let q = builder.ins().sdiv(l, r);
                        let rem = builder.ins().srem(l, r);
                        let rem_neg = builder.ins().icmp_imm(IntCC::SignedLessThan, rem, 0);
                        let r_neg = builder.ins().icmp_imm(IntCC::SignedLessThan, r, 0);
                        let plus = builder.ins().iadd_imm(q, 1);
                        let minus = builder.ins().iadd_imm(q, -1);
                        let fixed = builder.ins().select(r_neg, plus, minus);
                        builder.ins().select(rem_neg, fixed, q)
                    }
                };
                builder.def_var(vars[inst.a as usize], res);
            }
            OpCode::Pow => {
                let l = use_v(&mut builder, inst.b);
                let r = use_v(&mut builder, inst.c);
                let st = builder.ins().iconst(types::I64, state_addr);
                let call = builder.ins().call(pow_ref, &[l, r, st]);
                let res = builder.inst_results(call)[0];
                trap_if_flagged(&mut builder, state_addr, trap_blk);
                builder.def_var(vars[inst.a as usize], res);
            }
            OpCode::BitOr | OpCode::BitAnd | OpCode::BitXor => {
                let l = use_v(&mut builder, inst.b);
                let r = use_v(&mut builder, inst.c);
                let res = match inst.op {
                    OpCode::BitOr => builder.ins().bor(l, r),
                    OpCode::BitAnd => builder.ins().band(l, r),
                    _ => builder.ins().bxor(l, r),
                };
                builder.def_var(vars[inst.a as usize], res);
            }
            OpCode::BitNot => {
                let x = use_v(&mut builder, inst.b);
                let res = builder.ins().bnot(x);
                builder.def_var(vars[inst.a as usize], res);
            }
            OpCode::Shl | OpCode::Shr => {
                let l = use_v(&mut builder, inst.b);
                let r = use_v(&mut builder, inst.c);
                // Amount must be in 0..=63 (unsigned compare also catches negatives).
                let bad = builder.ins().icmp_imm(IntCC::UnsignedGreaterThan, r, 63);
                trap_if(&mut builder, bad, trap_blk);
                let res = if inst.op == OpCode::Shl {
                    builder.ins().ishl(l, r)
                } else {
                    builder.ins().sshr(l, r)
                };
                builder.def_var(vars[inst.a as usize], res);
            }
            OpCode::Eq | OpCode::Lt | OpCode::Le => {
                let l = use_v(&mut builder, inst.b);
                let r = use_v(&mut builder, inst.c);
                let cc = match inst.op {
                    OpCode::Eq => IntCC::Equal,
                    OpCode::Lt => IntCC::SignedLessThan,
                    _ => IntCC::SignedLessThanOrEqual,
                };
                let cmp = builder.ins().icmp(cc, l, r);
                let res = builder.ins().uextend(types::I64, cmp);
                builder.def_var(vars[inst.a as usize], res);
            }
            OpCode::Not => {
                let x = use_v(&mut builder, inst.b);
                let z = builder.ins().icmp_imm(IntCC::Equal, x, 0);
                let res = builder.ins().uextend(types::I64, z);
                builder.def_var(vars[inst.a as usize], res);
            }
            OpCode::And | OpCode::Or => {
                let l = use_v(&mut builder, inst.b);
                let r = use_v(&mut builder, inst.c);
                let lt = builder.ins().icmp_imm(IntCC::NotEqual, l, 0);
                let rt = builder.ins().icmp_imm(IntCC::NotEqual, r, 0);
                let both = if inst.op == OpCode::And {
                    builder.ins().band(lt, rt)
                } else {
                    builder.ins().bor(lt, rt)
                };
                let res = builder.ins().uextend(types::I64, both);
                builder.def_var(vars[inst.a as usize], res);
            }
            OpCode::Test => {
                // Skip the next instruction when truthiness != (c != 0).
                let x = use_v(&mut builder, inst.a);
                let truthy = builder.ins().icmp_imm(IntCC::NotEqual, x, 0);
                let (next, skip) = (blocks[&(pc + 1)], blocks[&(pc + 2)]);
                if inst.c != 0 {
                    builder.ins().brif(truthy, next, &[], skip, &[]);
                } else {
                    builder.ins().brif(truthy, skip, &[], next, &[]);
                }
                terminated = true;
            }
            OpCode::Jmp | OpCode::Break | OpCode::Continue => {
                let target = (pc as i64 + 1 + inst.sax_val() as i64) as usize;
                builder.ins().jump(blocks[&target], &[]);
                terminated = true;
            }
            OpCode::Return => {
                let v = use_v(&mut builder, inst.a);
                builder.ins().return_(&[v]);
                terminated = true;
            }
            OpCode::Call | OpCode::TailCall => {
                let name = &plan.callees[&pc];
                let base = inst.a;
                let nargs = inst.b as usize;
                let mut args: Vec<cranelift_codegen::ir::Value> = Vec::with_capacity(nargs);
                for i in 0..nargs {
                    args.push(use_v(&mut builder, base + 1 + i as u8));
                }
                if inst.op == OpCode::TailCall && name == &cell.name {
                    // Self tail call: rebind the parameters and loop.
                    for (i, v) in args.iter().enumerate() {
                        builder.def_var(vars[i], *v);
                    }
                    builder.ins().jump(loop_blk.expect("loop block"), &[]);
                    terminated = true;
                } else {
                    let call = builder.ins().call(callee_refs[name], &args);
                    let res = builder.inst_results(call)[0];
                    trap_if_flagged(&mut builder, state_addr, trap_blk);
                    if inst.op == OpCode::TailCall {
                        builder.ins().return_(&[res]);
                        terminated = true;
                    } else {
                        builder.def_var(vars[base as usize], res);
                    }
                }
            }
            other => {
                return Err(CodegenError::LoweringError(format!(
                    "strict JIT: unexpected opcode {other:?} in '{}'",
                    cell.name
                )));
            }
        }
    }
    debug_assert!(
        terminated,
        "strict JIT: cell '{}' falls off the end",
        cell.name
    );

    // Shared trap exit: raise the flag and return 0; callers check the flag.
    builder.switch_to_block(trap_blk);
    let st = builder.ins().iconst(types::I64, state_addr);
    let one = builder.ins().iconst(types::I64, 1);
    builder
        .ins()
        .store(MemFlags::trusted(), one, st, STATE_OFF_TRAP);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().return_(&[zero]);

    builder.seal_all_blocks();
    builder.finalize();

    let mut ctx = Context::for_function(func);
    module
        .define_function(func_id, &mut ctx)
        .map_err(|e| CodegenError::LoweringError(format!("define_function({}): {e}", cell.name)))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Variable helpers
// ---------------------------------------------------------------------------

/// Declare an external helper function in both the JIT module and the current
/// Cranelift function, returning a `FuncRef` that can be used with `builder.ins().call()`.
fn declare_helper_func(
    module: &mut JITModule,
    func: &mut cranelift_codegen::ir::Function,
    name: &str,
    params: &[ClifType],
    returns: &[ClifType],
) -> Result<cranelift_codegen::ir::FuncRef, CodegenError> {
    let mut sig = module.make_signature();
    for &p in params {
        sig.params.push(AbiParam::new(p));
    }
    for &r in returns {
        sig.returns.push(AbiParam::new(r));
    }
    let func_id = module
        .declare_function(name, Linkage::Import, &sig)
        .map_err(|e| CodegenError::LoweringError(format!("declare_function({name}): {e}")))?;
    Ok(module.declare_func_in_func(func_id, func))
}

/// Returns `true` if either operand register is typed as Float, indicating a
/// float operation.
fn is_float_op_jvt(var_types: &HashMap<u32, JitVarType>, lhs_reg: u8, rhs_reg: u8) -> bool {
    var_types.get(&(lhs_reg as u32)).copied() == Some(JitVarType::Float)
        || var_types.get(&(rhs_reg as u32)).copied() == Some(JitVarType::Float)
}

fn use_var(
    builder: &mut FunctionBuilder,
    vars: &[Variable],
    reg: u8,
) -> cranelift_codegen::ir::Value {
    let idx = reg as usize;
    if idx < vars.len() {
        builder.use_var(vars[idx])
    } else {
        builder.ins().iconst(types::I64, 0)
    }
}

fn def_var(
    builder: &mut FunctionBuilder,
    vars: &[Variable],
    reg: u8,
    val: cranelift_codegen::ir::Value,
) {
    let idx = reg as usize;
    if idx < vars.len() {
        builder.def_var(vars[idx], val);
    }
}

fn get_or_create_block(
    builder: &mut FunctionBuilder,
    block_map: &mut HashMap<usize, cranelift_codegen::ir::Block>,
    pc: usize,
) -> cranelift_codegen::ir::Block {
    *block_map
        .entry(pc)
        .or_insert_with(|| builder.create_block())
}

// ---------------------------------------------------------------------------
// Constant lowering
// ---------------------------------------------------------------------------

fn lower_constant(
    builder: &mut FunctionBuilder,
    cell: &LirCell,
    index: usize,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    let constant = cell.constants.get(index).ok_or_else(|| {
        CodegenError::LoweringError(format!(
            "constant index {index} out of range (cell has {})",
            cell.constants.len()
        ))
    })?;

    let val = match constant {
        Constant::Int(n) => builder.ins().iconst(types::I64, *n),
        Constant::Float(f) => builder.ins().f64const(*f),
        Constant::Bool(b) => builder.ins().iconst(types::I64, *b as i64),
        Constant::Null => builder.ins().iconst(types::I64, 0),
        Constant::String(_) => builder.ins().iconst(types::I64, 0),
        Constant::BigInt(_) => builder.ins().iconst(types::I64, 0),
    };

    Ok(val)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, target_arch = "x86_64"))]
mod tests {
    use super::*;
    use crate::compiler::lir::{Constant, Instruction, LirCell, LirModule, LirParam, OpCode};

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
        //  6: LoadInt   r3, -56        -- NOTE: LoadInt uses i8, so we use small vals
        //  7: Return    r3
        //
        // LoadInt stores b as u8 interpreted as i8 for the value.
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
        let mut engine = JitEngine::new(settings, 0).with_experimental_types(true);
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
        let mut engine = JitEngine::new(settings, 0).with_experimental_types(true);
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
        let mut engine = JitEngine::new(settings, 0).with_experimental_types(true);
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
        let mut engine = JitEngine::new(settings, 0).with_experimental_types(true);
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
        let mut engine = JitEngine::new(settings, 0).with_experimental_types(true);
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
        let mut engine = JitEngine::new(settings, 0).with_experimental_types(true);
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
        let mut engine = JitEngine::new(settings, 0).with_experimental_types(true);
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
        let mut engine = JitEngine::new(settings, 0).with_experimental_types(true);
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
        let mut engine = JitEngine::new(settings, 0).with_experimental_types(true);
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
        let mut engine = JitEngine::new(settings, 0).with_experimental_types(true);
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
        let mut engine = JitEngine::new(settings, 0).with_experimental_types(true);
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
        let mut engine = JitEngine::new(settings, 0).with_experimental_types(true);
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
        let mut engine = JitEngine::new(settings, 0).with_experimental_types(true);
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
        let mut engine = JitEngine::new(settings, 0).with_experimental_types(true);
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
        let mut engine = JitEngine::new(settings, 0).with_experimental_types(true);
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
        let mut engine = JitEngine::new(settings, 0).with_experimental_types(true);
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
        let mut engine = JitEngine::new(settings, 0).with_experimental_types(true);
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
        let mut engine = JitEngine::new(settings, 0).with_experimental_types(true);
        engine.compile_module(&lir).expect("compile");

        let raw = engine
            .execute_jit_nullary("overwrite_concat")
            .expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "hello world");
    }

    // --- Strict tier: interpreter-equivalence guards -----------------------

    fn int_cell(
        name: &str,
        nparams: usize,
        regs: u16,
        consts: Vec<Constant>,
        ins: Vec<Instruction>,
    ) -> LirCell {
        LirCell {
            name: name.to_string(),
            params: (0..nparams)
                .map(|i| crate::compiler::lir::LirParam {
                    name: format!("p{i}"),
                    ty: "Int".to_string(),
                    register: i as u8,
                    variadic: false,
                })
                .collect(),
            returns: Some("Int".to_string()),
            registers: regs,
            constants: consts,
            instructions: ins,
            effect_handler_metas: Vec::new(),
        }
    }

    fn binop_cell(op: OpCode) -> LirCell {
        int_cell(
            "f",
            2,
            3,
            vec![],
            vec![
                Instruction::abc(op, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
        )
    }

    #[test]
    fn strict_int_ops_trap_instead_of_wrapping_or_crashing() {
        for (op, a, b) in [
            (OpCode::Add, i64::MAX, 1),
            (OpCode::Sub, i64::MIN, 1),
            (OpCode::Mul, i64::MAX, 2),
            (OpCode::Div, 7, 0),
            (OpCode::Div, i64::MIN, -1),
            (OpCode::Mod, 7, 0),
            (OpCode::Mod, i64::MIN, -1),
            (OpCode::FloorDiv, 7, 0),
            (OpCode::Pow, 2, 70),
            (OpCode::Pow, 2, -1),
            (OpCode::Shl, 1, 64),
            (OpCode::Shr, 1, -1),
        ] {
            let lir = make_module_with_cells(vec![binop_cell(op)]);
            let mut engine = JitEngine::new(CodegenSettings::default(), 0);
            engine.compile_module(&lir).unwrap();
            assert!(engine.is_compiled("f"), "{op:?} should compile");
            assert!(
                matches!(engine.execute_jit("f", &[a, b]), Err(JitError::Trap)),
                "{op:?}({a}, {b}) must trap so the interpreter decides"
            );
            // The engine stays usable after a trap (the flag is reset per call).
            assert!(engine.execute_jit("f", &[2, 1]).is_ok(), "{op:?}");
        }
    }

    #[test]
    fn strict_mod_and_floordiv_match_euclidean_semantics() {
        for (op, a, b, want) in [
            (OpCode::Mod, -7, 2, 1),
            (OpCode::Mod, 7, -2, 1),
            (OpCode::Mod, -7, -2, 1),
            (OpCode::Mod, 7, 2, 1),
            (OpCode::FloorDiv, -7, 2, -4),
            (OpCode::FloorDiv, 7, -2, -3),
            (OpCode::FloorDiv, -7, -2, 4),
            (OpCode::FloorDiv, 7, 2, 3),
            (OpCode::Div, -7, 2, -3),
        ] {
            let lir = make_module_with_cells(vec![binop_cell(op)]);
            let mut engine = JitEngine::new(CodegenSettings::default(), 0);
            engine.compile_module(&lir).unwrap();
            assert_eq!(
                engine.execute_jit("f", &[a, b]).unwrap(),
                want,
                "{op:?}({a},{b})"
            );
        }
    }

    #[test]
    fn strict_tier_refuses_cells_with_uncompiled_callees() {
        // caller() calls a name that is not a compiled cell; the old JIT returned 0.
        let caller = int_cell(
            "caller",
            0,
            2,
            vec![Constant::String("print".to_string())],
            vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Call, 0, 0, 1),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
        );
        let lir = make_module_with_cells(vec![caller]);
        let mut engine = JitEngine::new(CodegenSettings::default(), 0);
        engine.compile_module(&lir).unwrap();
        assert!(!engine.is_compiled("caller"));
    }

    #[test]
    fn strict_tier_refuses_float_and_untyped_cells() {
        let mut fl = int_cell(
            "fl",
            0,
            2,
            vec![Constant::Float(1.5)],
            vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
        );
        fl.returns = Some("Float".to_string());
        let mut untyped = binop_cell(OpCode::Add);
        untyped.name = "untyped".to_string();
        untyped.returns = None;
        let lir = make_module_with_cells(vec![fl, untyped]);
        let mut engine = JitEngine::new(CodegenSettings::default(), 0);
        engine.compile_module(&lir).unwrap();
        assert!(!engine.is_compiled("fl"));
        assert!(!engine.is_compiled("untyped"));
    }

    #[test]
    fn strict_tier_bool_return_kind() {
        let mut p = int_cell(
            "lt",
            2,
            3,
            vec![],
            vec![
                Instruction::abc(OpCode::Lt, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
        );
        p.returns = Some("Bool".to_string());
        let lir = make_module_with_cells(vec![p]);
        let mut engine = JitEngine::new(CodegenSettings::default(), 0);
        engine.compile_module(&lir).unwrap();
        assert_eq!(engine.return_kind("lt"), Some(JitReturn::Bool));
        assert_eq!(engine.execute_jit("lt", &[1, 2]).unwrap(), 1);
        assert_eq!(engine.execute_jit("lt", &[2, 1]).unwrap(), 0);
    }

    #[test]
    fn strict_tier_stack_guard_traps_runaway_recursion() {
        // f(n) = 1 + f(n + 1)  -- never terminates; must trap, not overflow the stack.
        let f = int_cell(
            "f",
            1,
            6,
            vec![Constant::String("f".to_string()), Constant::Int(1)],
            vec![
                Instruction::abx(OpCode::LoadK, 1, 0),
                Instruction::abx(OpCode::LoadK, 3, 1),
                Instruction::abc(OpCode::Add, 2, 0, 3),
                Instruction::abc(OpCode::Move, 2, 2, 0),
                Instruction::abc(OpCode::Call, 1, 1, 1),
                Instruction::abx(OpCode::LoadK, 4, 1),
                Instruction::abc(OpCode::Add, 5, 4, 1),
                Instruction::abc(OpCode::Return, 5, 1, 0),
            ],
        );
        let lir = make_module_with_cells(vec![f]);
        let mut engine = JitEngine::new(CodegenSettings::default(), 0);
        engine.compile_module(&lir).unwrap();
        assert!(engine.is_compiled("f"));
        assert!(matches!(engine.execute_jit("f", &[0]), Err(JitError::Trap)));
    }
}
