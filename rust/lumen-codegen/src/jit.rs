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

use std::collections::{BTreeSet, HashMap};

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{types, AbiParam, InstBuilder, Type as ClifType};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};

use lumen_compiler::compiler::lir::{Constant, Instruction, LirCell, LirModule, OpCode};

use crate::emit::CodegenError;
use crate::types::lir_type_str_to_cl_type;

/// Maximum number of virtual registers we support per cell.
const MAX_REGS: usize = 256;

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
        let builder = JITBuilder::new(cranelift_module::default_libcall_names())
            .map_err(|e| JitError::ModuleError(format!("JITBuilder creation failed: {e}")))?;

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
}

/// Lower an entire LIR module into Cranelift IR inside the given `JITModule`.
fn lower_module_jit(
    module: &mut JITModule,
    lir: &LirModule,
    pointer_type: ClifType,
) -> Result<JitLoweredModule, CodegenError> {
    let mut fb_ctx = FunctionBuilderContext::new();

    // First pass: declare all cell signatures.
    let mut func_ids: HashMap<String, FuncId> = HashMap::new();
    for cell in &lir.cells {
        let mut sig = module.make_signature();
        for _param in &cell.params {
            sig.params.push(AbiParam::new(pointer_type));
        }
        let ret_ty = cell
            .returns
            .as_deref()
            .map(|s| lir_type_str_to_cl_type(s, pointer_type))
            .unwrap_or(pointer_type);
        sig.returns.push(AbiParam::new(ret_ty));
        let func_id = module
            .declare_function(&cell.name, Linkage::Export, &sig)
            .map_err(|e| {
                CodegenError::LoweringError(format!("declare_function({}): {e}", cell.name))
            })?;
        func_ids.insert(cell.name.clone(), func_id);
    }

    // Second pass: lower each cell body.
    let mut lowered = JitLoweredModule {
        functions: Vec::with_capacity(lir.cells.len()),
    };

    for cell in &lir.cells {
        let func_id = func_ids[&cell.name];
        lower_cell_jit(module, cell, &mut fb_ctx, pointer_type, func_id, &func_ids)?;
        lowered.functions.push(JitLoweredFunction {
            name: cell.name.clone(),
            func_id,
            param_count: cell.params.len(),
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
            OpCode::Return | OpCode::Halt | OpCode::TailCall => {
                if pc + 1 < instructions.len() {
                    targets.insert(pc + 1);
                }
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
    for _param in &cell.params {
        sig.params.push(AbiParam::new(pointer_type));
    }
    let ret_ty = cell
        .returns
        .as_deref()
        .map(|s| lir_type_str_to_cl_type(s, pointer_type))
        .unwrap_or(pointer_type);
    sig.returns.push(AbiParam::new(ret_ty));

    let mut func = cranelift_codegen::ir::Function::with_name_signature(
        cranelift_codegen::ir::UserFuncName::user(0, func_id.as_u32()),
        sig,
    );

    let mut callee_refs: HashMap<FuncId, cranelift_codegen::ir::FuncRef> = HashMap::new();
    for (_name, &callee_id) in func_ids.iter() {
        let func_ref = module.declare_func_in_func(callee_id, &mut func);
        callee_refs.insert(callee_id, func_ref);
    }

    let mut builder = FunctionBuilder::new(&mut func, fb_ctx);

    let num_regs = (cell.registers as usize)
        .max(cell.params.len())
        .clamp(1, MAX_REGS);
    let mut vars: Vec<Variable> = Vec::with_capacity(num_regs);
    for i in 0..num_regs {
        let var = Variable::from_u32(i as u32);
        builder.declare_var(var, types::I64);
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
        let zero = builder.ins().iconst(types::I64, 0);
        for var in vars.iter().take(num_regs).skip(cell.params.len()) {
            builder.def_var(*var, zero);
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
                let val = lower_constant(&mut builder, cell, bx)?;
                def_var(&mut builder, &vars, a, val);
            }
            OpCode::LoadBool => {
                let a = inst.a;
                let b_val = inst.b;
                let val = builder.ins().iconst(types::I64, b_val as i64);
                def_var(&mut builder, &vars, a, val);
            }
            OpCode::LoadInt => {
                let a = inst.a;
                let imm = inst.b as i8 as i64;
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
                let val = use_var(&mut builder, &vars, inst.b);
                def_var(&mut builder, &vars, inst.a, val);
            }

            // Arithmetic
            OpCode::Add => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let res = builder.ins().iadd(lhs, rhs);
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::Sub => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let res = builder.ins().isub(lhs, rhs);
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::Mul => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let res = builder.ins().imul(lhs, rhs);
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::Div => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let res = builder.ins().sdiv(lhs, rhs);
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::Mod => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let res = builder.ins().srem(lhs, rhs);
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::Neg => {
                let operand = use_var(&mut builder, &vars, inst.b);
                let res = builder.ins().ineg(operand);
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::FloorDiv => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let res = builder.ins().sdiv(lhs, rhs);
                def_var(&mut builder, &vars, inst.a, res);
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

            // Comparison
            OpCode::Eq => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let cmp = builder.ins().icmp(IntCC::Equal, lhs, rhs);
                let res = builder.ins().uextend(types::I64, cmp);
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::Lt => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let cmp = builder.ins().icmp(IntCC::SignedLessThan, lhs, rhs);
                let res = builder.ins().uextend(types::I64, cmp);
                def_var(&mut builder, &vars, inst.a, res);
            }
            OpCode::Le => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let cmp = builder.ins().icmp(IntCC::SignedLessThanOrEqual, lhs, rhs);
                let res = builder.ins().uextend(types::I64, cmp);
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
                let val = use_var(&mut builder, &vars, inst.a);
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
            }
            OpCode::TailCall => {
                let base = inst.a;
                let num_args = inst.b as usize;
                let callee_name = find_callee_name(cell, &cell.instructions, pc, base);

                let is_self_call = callee_name
                    .as_ref()
                    .map(|n| n == &cell.name)
                    .unwrap_or(false);

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

            // Everything else -> trap
            _ => {
                builder
                    .ins()
                    .trap(cranelift_codegen::ir::TrapCode::unwrap_user(2));
                terminated = true;
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
// Variable helpers
// ---------------------------------------------------------------------------

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
                Instruction::abc(OpCode::LoadInt, 1, 1, 0), // 0: r1 = 1
                Instruction::abc(OpCode::LoadInt, 2, 1, 0), // 1: r2 = 1
                Instruction::abc(OpCode::LoadInt, 3, 0, 0), // 2: r3 = 0
                Instruction::abc(OpCode::Lt, 4, 3, 0),      // 3: r4 = 0 < n
                Instruction::abc(OpCode::Test, 4, 0, 0),    // 4: test
                Instruction::sax(OpCode::Jmp, 3),           // 5: -> 9 (exit)
                Instruction::abc(OpCode::Mul, 1, 1, 0),     // 6: r1 *= n
                Instruction::abc(OpCode::Sub, 0, 0, 2),     // 7: n -= 1
                Instruction::sax(OpCode::Jmp, -6),          // 8: -> 3 (loop)
                Instruction::abc(OpCode::Return, 1, 1, 0),  // 9: return r1
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
                Instruction::abc(OpCode::LoadInt, 3, 0, 0),  // 0: r3 = 0
                Instruction::abc(OpCode::Le, 4, 0, 3),       // 1: r4 = n <= 0
                Instruction::abc(OpCode::Test, 4, 0, 0),     // 2: test
                Instruction::sax(OpCode::Jmp, 1),            // 3: -> 5
                Instruction::abc(OpCode::Return, 1, 1, 0),   // 4: return a
                Instruction::abx(OpCode::LoadK, 5, 0),       // 5: r5 = "fib_acc"
                Instruction::abc(OpCode::LoadInt, 8, 1, 0),  // 6: r8 = 1
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
                Instruction::abc(OpCode::LoadInt, 1, 0, 0),   // 0: r1 = 0
                Instruction::abc(OpCode::Lt, 2, 1, 0),        // 1: r2 = 0 < x
                Instruction::abc(OpCode::Test, 2, 0, 0),      // 2: test
                Instruction::sax(OpCode::Jmp, 2),             // 3: -> 6 (else)
                Instruction::abc(OpCode::LoadInt, 3, 100, 0), // 4: r3 = 100
                Instruction::sax(OpCode::Jmp, 1),             // 5: -> 7 (end)
                Instruction::abc(OpCode::LoadInt, 3, 50, 0),  // 6: r3 = 50
                Instruction::abc(OpCode::Return, 3, 1, 0),    // 7: return r3
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
}
