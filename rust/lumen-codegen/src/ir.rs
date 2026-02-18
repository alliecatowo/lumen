//! Unified LIR-to-Cranelift IR lowering module.
//!
//! Provides a generic `lower_cell` function that works with both JITModule
//! and ObjectModule, abstracting over the `cranelift_module::Module` trait.
//!
//! This implementation supports all opcodes from the JIT path, including:
//! - Type-aware arithmetic (Int, Float, String)
//! - String runtime helpers (inline malloc alloc, concat, clone, eq, cmp, drop)
//! - Float operations (fadd, fsub, fmul, fdiv, fneg, floor)
//! - Type-aware comparisons (icmp, fcmp, string_cmp)
//! - Tail-call optimization for self-recursive calls
//! - String memory management (drop on overwrite, return cleanup)

use std::collections::{BTreeSet, HashMap};

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::stackslot::{StackSlotData, StackSlotKind};
use cranelift_codegen::ir::{types, AbiParam, InstBuilder, MemFlags, Type as ClifType};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{FuncId, Linkage, Module};

use lumen_core::lir::{Constant, Instruction, LirCell, OpCode};

use crate::emit::CodegenError;
use crate::types::lir_type_str_to_cl_type;

/// Maximum number of virtual registers we support per cell.
/// With 64-bit instructions, register fields are 16 bits wide (up to 65,536).
const MAX_REGS: usize = 65_536;

// ---------------------------------------------------------------------------
// NaN-boxing constants and helpers
//
// All values are stored as 64-bit integers in registers.  The encoding:
//   - **Integers**:  `(val << 1) | 1`  (tagged as odd numbers)
//   - **Floats**:    raw IEEE 754 f64 bits reinterpreted as i64 (bitcast)
//   - **Null**:      canonical quiet NaN  `0x7FF8_0000_0000_0000`
//   - **True**:      quiet NaN payload 1  `0x7FF8_0000_0000_0001`
//   - **False**:     quiet NaN payload 2  `0x7FF8_0000_0000_0002`
//   - **Pointers** (Strings, Records): raw pointers stored unchanged
// ---------------------------------------------------------------------------

/// NaN-boxed representation of `null`.
pub const NAN_BOX_NULL: i64 = 0x7FF8_0000_0000_0000_u64 as i64;

/// NaN-boxed representation of `true`.
pub const NAN_BOX_TRUE: i64 = 0x7FF8_0000_0000_0001_u64 as i64;

/// NaN-boxed representation of `false`.
pub const NAN_BOX_FALSE: i64 = 0x7FF8_0000_0000_0002_u64 as i64;

/// Emit IR to NaN-box an integer: `(val << 1) | 1`.
fn emit_box_int(
    builder: &mut FunctionBuilder,
    val: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let shifted = builder.ins().ishl_imm(val, 1);
    builder.ins().bor_imm(shifted, 1)
}

/// Emit IR to unbox a NaN-boxed integer: arithmetic right shift by 1.
/// Only valid when the value is known to be a NaN-boxed int (low bit = 1,
/// not a boolean sentinel).
fn emit_unbox_int(
    builder: &mut FunctionBuilder,
    val: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    builder.ins().sshr_imm(val, 1)
}

/// Emit IR to NaN-box a float: bitcast F64 → I64.
fn emit_box_float(
    builder: &mut FunctionBuilder,
    val: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    builder.ins().bitcast(types::I64, MemFlags::new(), val)
}

/// Emit IR to unbox a NaN-boxed float: bitcast I64 → F64.
fn emit_unbox_float(
    builder: &mut FunctionBuilder,
    val: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    builder.ins().bitcast(types::F64, MemFlags::new(), val)
}

/// Pure Rust: NaN-box an integer value (for test assertions).
pub fn nan_box_int(v: i64) -> i64 {
    (v << 1) | 1
}

/// Pure Rust: unbox a NaN-boxed integer (for test assertions).
pub fn nan_unbox_int(v: i64) -> i64 {
    v >> 1
}

/// Pure Rust: NaN-box a float value (for test assertions).
pub fn nan_box_float(v: f64) -> i64 {
    v.to_bits() as i64
}

/// Pure Rust: unbox a NaN-boxed float (for test assertions).
pub fn nan_unbox_float(v: i64) -> f64 {
    f64::from_bits(v as u64)
}

/// Pure Rust: unbox a NaN-boxed JIT result to a plain i64 suitable for test
/// assertions.  Handles integers (tagged with low bit = 1), booleans
/// (`NAN_BOX_TRUE` → 1, `NAN_BOX_FALSE` → 0), null (`NAN_BOX_NULL` → 0),
/// and pass-through for anything else (float bits, pointers).
///
/// # Deprecated
/// This function uses a heuristic `(v & 1) == 1` test that incorrectly
/// matches some float bit patterns (e.g. `f64::consts::E`). Use
/// `nan_unbox_typed()` in `jit.rs` instead, which uses compile-time return
/// type information for correct unboxing.
#[deprecated(note = "use nan_unbox_typed() with JitVarType for correct type-aware unboxing")]
pub fn nan_unbox_jit_result(raw: i64) -> i64 {
    match raw {
        NAN_BOX_TRUE => 1,
        NAN_BOX_FALSE => 0,
        NAN_BOX_NULL => 0,
        v if (v & 1) == 1 => v >> 1, // NaN-boxed integer
        other => other,              // float bits or pointer
    }
}

/// Hybrid SSA register state.
///
/// Registers that are only written within a single basic block use pure SSA
/// `Value` tracking (no Cranelift `Variable` overhead). Registers written
/// across multiple blocks (or that need phi-merging at control flow joins)
/// use Cranelift `Variable` for automatic SSA construction.
struct HybridRegs {
    /// Registers that need Cranelift `Variable` (written in multiple blocks,
    /// used as function parameters, or involved in TCO loops).
    vars: HashMap<u16, Variable>,
    /// Pure SSA values for registers written within a single block.
    /// These are cheap: just a direct `Value` lookup with no SSA bookkeeping.
    ssa_vals: HashMap<u16, cranelift_codegen::ir::Value>,
    /// Total number of registers in the cell.
    num_regs: usize,
}

impl HybridRegs {
    fn new(num_regs: usize) -> Self {
        Self {
            vars: HashMap::new(),
            ssa_vals: HashMap::new(),
            num_regs,
        }
    }

    /// Check if a register uses a Cranelift `Variable` (multi-block).
    #[allow(dead_code)]
    fn has_var(&self, reg: u16) -> bool {
        self.vars.contains_key(&reg)
    }
}

/// Classify registers as needing Cranelift `Variable` (multi-block) vs pure SSA.
///
/// A register needs `Variable` if:
/// 1. It's a function parameter (defined in entry block, potentially re-defined in TCO)
/// 2. It's written (def'd) in more than one basic block
/// 3. It's written in a block that differs from all blocks where it's read
///    (i.e., the value crosses a block boundary)
///
/// Everything else uses pure SSA `Value` tracking.
fn classify_multi_block_regs(
    cell: &LirCell,
    block_starts: &BTreeSet<usize>,
    _self_tco: bool,
) -> std::collections::HashSet<u16> {
    use std::collections::HashSet;

    let mut multi_block: HashSet<u16> = HashSet::new();

    // Rule 1: All function parameters need Variable (entry block definition,
    // may be re-written in TCO loop, and may be read in any block).
    for i in 0..cell.params.len() {
        multi_block.insert(i as u16);
    }

    // Build a mapping: instruction PC → block index.
    // Block index changes at each PC in block_starts.
    let sorted_starts: Vec<usize> = block_starts.iter().copied().collect();

    let block_of = |pc: usize| -> usize {
        // Binary search for the largest block_start <= pc.
        // Block 0 is the entry block (before any block_start).
        match sorted_starts.binary_search(&pc) {
            Ok(i) => i + 1, // PC is exactly a block start → that block
            Err(0) => 0,    // Before all block starts → entry block
            Err(i) => i,    // Between block_starts[i-1] and block_starts[i]
        }
    };

    // Registers written in 2+ distinct blocks (NOT counting zero-init).
    // We skip zero-init from the analysis because SSA-only registers get
    // a zero default in use_var when no SSA value has been set yet.
    let mut def_blocks_no_init: HashMap<u16, HashSet<usize>> = HashMap::new();
    for (pc, inst) in cell.instructions.iter().enumerate() {
        let blk = block_of(pc);
        match inst.op {
            OpCode::LoadK
            | OpCode::LoadBool
            | OpCode::LoadInt
            | OpCode::Move
            | OpCode::MoveOwn
            | OpCode::Add
            | OpCode::Sub
            | OpCode::Mul
            | OpCode::Div
            | OpCode::FloorDiv
            | OpCode::Mod
            | OpCode::Pow
            | OpCode::Neg
            | OpCode::Eq
            | OpCode::Lt
            | OpCode::Le
            | OpCode::Not
            | OpCode::And
            | OpCode::Or
            | OpCode::BitOr
            | OpCode::BitAnd
            | OpCode::BitXor
            | OpCode::BitNot
            | OpCode::Shl
            | OpCode::Shr
            | OpCode::Concat
            | OpCode::NullCo
            | OpCode::Intrinsic
            | OpCode::GetField
            | OpCode::SetField
            | OpCode::GetIndex
            | OpCode::SetIndex
            | OpCode::Call => {
                def_blocks_no_init.entry(inst.a).or_default().insert(blk);
            }
            OpCode::LoadNil => {
                let count = inst.b as usize;
                for i in 0..=count {
                    let r = inst.a as usize + i;
                    if r < MAX_REGS {
                        def_blocks_no_init.entry(r as u16).or_default().insert(blk);
                    }
                }
            }
            _ => {}
        }
        if inst.op == OpCode::MoveOwn {
            def_blocks_no_init.entry(inst.b).or_default().insert(blk);
        }
    }

    for (&reg, blocks) in &def_blocks_no_init {
        if blocks.len() > 1 {
            multi_block.insert(reg);
        }
    }

    // Also: any register that is READ in a block where it was NOT written
    // (cross-block read) needs Variable. This handles cases where a value
    // is defined in one block and used in a successor.
    let mut read_blocks: HashMap<u16, HashSet<usize>> = HashMap::new();
    for (pc, inst) in cell.instructions.iter().enumerate() {
        let blk = block_of(pc);
        // Collect source registers (reads)
        match inst.op {
            OpCode::Add
            | OpCode::Sub
            | OpCode::Mul
            | OpCode::Div
            | OpCode::FloorDiv
            | OpCode::Mod
            | OpCode::Pow
            | OpCode::Eq
            | OpCode::Lt
            | OpCode::Le
            | OpCode::BitOr
            | OpCode::BitAnd
            | OpCode::BitXor
            | OpCode::Shl
            | OpCode::Shr
            | OpCode::Concat
            | OpCode::NullCo => {
                read_blocks.entry(inst.b).or_default().insert(blk);
                read_blocks.entry(inst.c).or_default().insert(blk);
            }
            OpCode::Move
            | OpCode::MoveOwn
            | OpCode::Neg
            | OpCode::BitNot
            | OpCode::Not
            | OpCode::Test => {
                read_blocks.entry(inst.b).or_default().insert(blk);
            }
            OpCode::Return => {
                read_blocks.entry(inst.a).or_default().insert(blk);
            }
            OpCode::Call | OpCode::TailCall => {
                // Reads the callee name from base, and args from base+1..
                // But base is also the destination for Call, so it's both read AND written.
                // The callee name read happens before the write.
                let num_args = inst.b as usize;
                for i in 0..num_args {
                    let arg_reg = inst.a + 1 + i as u16;
                    read_blocks.entry(arg_reg).or_default().insert(blk);
                }
                // Base register is read (callee name) before being overwritten
                read_blocks.entry(inst.a).or_default().insert(blk);
            }
            OpCode::Intrinsic => {
                let arg_base = inst.c;
                // The intrinsic may read arg_base and beyond
                read_blocks.entry(arg_base).or_default().insert(blk);
            }
            OpCode::GetField => {
                read_blocks.entry(inst.b).or_default().insert(blk);
            }
            OpCode::SetField => {
                read_blocks.entry(inst.a).or_default().insert(blk);
                read_blocks.entry(inst.c).or_default().insert(blk);
            }
            OpCode::GetIndex => {
                read_blocks.entry(inst.b).or_default().insert(blk);
                read_blocks.entry(inst.c).or_default().insert(blk);
            }
            OpCode::SetIndex => {
                read_blocks.entry(inst.a).or_default().insert(blk);
                read_blocks.entry(inst.b).or_default().insert(blk);
                read_blocks.entry(inst.c).or_default().insert(blk);
            }
            _ => {}
        }
    }

    // If a register is read in any block where it doesn't have a def in
    // that SAME block (from def_blocks_no_init), it crosses a block boundary.
    for (&reg, r_blocks) in &read_blocks {
        let d_blocks = def_blocks_no_init.get(&reg);
        for &rblk in r_blocks {
            let defined_in_same_block = d_blocks.map(|dbs| dbs.contains(&rblk)).unwrap_or(false);
            if !defined_in_same_block {
                // Read in a block where it's not defined → crosses block boundary
                multi_block.insert(reg);
                break;
            }
        }
    }

    multi_block
}

/// Tracks the semantic type of each JIT variable/register.
/// At the Cranelift IR level, both Int and Str are I64, but we need to
/// distinguish them so that operations like Add dispatch to the correct
/// implementation (iadd vs string concatenation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JitVarType {
    /// 64-bit signed integer.
    Int,
    /// 64-bit IEEE 754 floating point.
    Float,
    /// Heap-allocated refcounted string, represented as a `*mut JitString` cast to i64.
    /// The pointer is created by inline `jit_rt_malloc` + field stores or `jit_rt_string_concat`
    /// and must be freed via `jit_rt_string_drop` when no longer needed.
    Str,
    /// Boolean value, NaN-boxed as sentinel values (NAN_BOX_TRUE / NAN_BOX_FALSE).
    Bool,
}

impl JitVarType {
    /// Return the Cranelift IR type for this variable type.
    ///
    /// With NaN-boxing, all types are uniformly I64 in registers.
    #[allow(dead_code)]
    fn clif_type(self) -> ClifType {
        // NaN-boxing: everything is I64
        types::I64
    }

    /// Infer JitVarType from a LIR return-type string (e.g. "Int", "Float", "String", "Bool").
    pub(crate) fn from_lir_return_type(s: &str) -> Self {
        match s {
            "Float" => JitVarType::Float,
            "String" => JitVarType::Str,
            "Bool" => JitVarType::Bool,
            _ => JitVarType::Int,
        }
    }
}

/// Unified cell lowering function that works with any Module implementation.
///
/// This function lowers a single LIR cell into Cranelift IR, handling all
/// opcodes supported by the JIT backend including type-aware arithmetic,
/// string operations, float operations, and tail-call optimization.
///
/// # Type parameters
///
/// * `M` - The Cranelift module type (JITModule or ObjectModule)
///
/// # Arguments
///
/// * `ctx` - Cranelift compilation context
/// * `builder` - Function builder context for SSA construction
/// * `cell` - The LIR cell to lower
/// * `module` - The Cranelift module to define the function in
///
/// # Returns
///
/// Ok(()) on success, or a CodegenError describing what went wrong.
pub(crate) fn lower_cell<M: Module>(
    ctx: &mut Context,
    fb_ctx: &mut FunctionBuilderContext,
    cell: &LirCell,
    module: &mut M,
    pointer_type: ClifType,
    func_id: FuncId,
    func_ids: &HashMap<String, FuncId>,
    string_table: &[String],
    cell_return_types: &HashMap<String, JitVarType>,
) -> Result<(), CodegenError> {
    // Build signature
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
    // Always return I64 at the ABI level. Float results are bitcast to I64
    // before returning so that execute_jit_nullary/unary (which transmute the
    // fn ptr to `fn(...) -> i64`) work uniformly. Callers that expect a float
    // result use `f64::from_bits(result as u64)`.
    sig.returns.push(AbiParam::new(types::I64));

    let mut func = cranelift_codegen::ir::Function::with_name_signature(
        cranelift_codegen::ir::UserFuncName::user(0, func_id.as_u32()),
        sig,
    );

    // Pre-declare all callable functions in this function's namespace.
    let mut callee_refs: HashMap<FuncId, cranelift_codegen::ir::FuncRef> = HashMap::new();
    for (_name, &callee_id) in func_ids.iter() {
        let func_ref = module.declare_func_in_func(callee_id, &mut func);
        callee_refs.insert(callee_id, func_ref);
    }

    // Declare string runtime helper functions (for JIT string support).
    let str_concat_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_concat",
        &[types::I64, types::I64],
        &[types::I64],
    )?;
    let str_concat_mut_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_concat_mut",
        &[types::I64, types::I64],
        &[types::I64],
    )?;
    let str_concat_multi_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_concat_multi",
        &[types::I64, types::I64], // ptr to array, count
        &[types::I64],
    )?;
    let malloc_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_malloc",
        &[types::I64], // size
        &[types::I64],
    )?;
    let alloc_bytes_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_alloc_bytes",
        &[types::I64], // size
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
    let memcpy_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_memcpy",
        &[types::I64, types::I64, types::I64], // dst, src, len
        &[],
    )?;

    // Declare record runtime helper functions (for JIT record support).
    let record_get_field_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_record_get_field",
        &[types::I64, types::I64, types::I64], // record_ptr, field_name_ptr, field_name_len
        &[types::I64],
    )?;
    let record_set_field_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_record_set_field",
        &[types::I64, types::I64, types::I64, types::I64], // record_ptr, field_name_ptr, field_name_len, value_ptr
        &[types::I64],
    )?;
    // Declare collection index runtime helper functions (for JIT list/map indexing).
    let get_index_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_get_index",
        &[types::I64, types::I64], // collection_ptr, index_ptr
        &[types::I64],
    )?;
    let set_index_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_set_index",
        &[types::I64, types::I64, types::I64], // collection_ptr, index_ptr, value_ptr
        &[types::I64],
    )?;
    // Declare collection runtime helper functions (for JIT List/Map construction).
    let new_list_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_new_list",
        &[types::I64, types::I64], // values_ptr, count
        &[types::I64],
    )?;
    let new_map_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_new_map",
        &[types::I64, types::I64], // kvpairs_ptr, count
        &[types::I64],
    )?;
    let collection_len_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_collection_len",
        &[types::I64], // value_ptr
        &[types::I64],
    )?;
    // Declare union runtime helper functions (for JIT enum support).
    let union_new_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_union_new",
        &[types::I64, types::I64, types::I64], // tag_ptr, tag_len, payload_ptr
        &[types::I64],
    )?;
    let union_is_variant_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_union_is_variant",
        &[types::I64, types::I64, types::I64], // union_ptr, tag_ptr, tag_len
        &[types::I64],
    )?;
    let union_unbox_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_union_unbox",
        &[types::I64],
        &[types::I64],
    )?;
    // Declare intrinsic runtime helper functions (for JIT builtin support).
    let intrinsic_print_int_ref =
        declare_helper_func(module, &mut func, "jit_rt_print_int", &[types::I64], &[])?;
    let intrinsic_print_float_ref =
        declare_helper_func(module, &mut func, "jit_rt_print_float", &[types::F64], &[])?;
    let intrinsic_print_str_ref =
        declare_helper_func(module, &mut func, "jit_rt_print_str", &[types::I64], &[])?;
    let intrinsic_string_len_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_len",
        &[types::I64],
        &[types::I64],
    )?;
    // Math transcendental helpers (Cranelift can't do these natively)
    let intrinsic_sin_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_sin",
        &[types::F64],
        &[types::F64],
    )?;
    let intrinsic_cos_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_cos",
        &[types::F64],
        &[types::F64],
    )?;
    let intrinsic_tan_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_tan",
        &[types::F64],
        &[types::F64],
    )?;
    let intrinsic_log_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_log",
        &[types::F64],
        &[types::F64],
    )?;
    let intrinsic_log2_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_log2",
        &[types::F64],
        &[types::F64],
    )?;
    let intrinsic_log10_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_log10",
        &[types::F64],
        &[types::F64],
    )?;
    let intrinsic_pow_float_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_pow_float",
        &[types::F64, types::F64],
        &[types::F64],
    )?;
    let intrinsic_pow_int_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_pow_int",
        &[types::I64, types::I64],
        &[types::I64],
    )?;
    let intrinsic_fmod_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_fmod",
        &[types::F64, types::F64],
        &[types::F64],
    )?;
    // Conversion helpers
    let intrinsic_to_string_int_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_to_string_int",
        &[types::I64],
        &[types::I64],
    )?;
    let intrinsic_to_string_float_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_to_string_float",
        &[types::F64],
        &[types::I64],
    )?;
    let intrinsic_to_int_from_float_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_to_int_from_float",
        &[types::F64],
        &[types::I64],
    )?;
    let intrinsic_to_int_from_string_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_to_int_from_string",
        &[types::I64],
        &[types::I64],
    )?;
    let intrinsic_to_float_from_int_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_to_float_from_int",
        &[types::I64],
        &[types::F64],
    )?;
    let intrinsic_to_float_from_string_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_to_float_from_string",
        &[types::I64],
        &[types::F64],
    )?;

    // String operation helpers
    let intrinsic_string_upper_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_upper",
        &[types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_lower_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_lower",
        &[types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_trim_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_trim",
        &[types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_contains_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_contains",
        &[types::I64, types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_starts_with_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_starts_with",
        &[types::I64, types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_ends_with_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_ends_with",
        &[types::I64, types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_replace_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_replace",
        &[types::I64, types::I64, types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_index_of_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_index_of",
        &[types::I64, types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_slice_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_slice",
        &[types::I64, types::I64, types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_pad_left_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_pad_left",
        &[types::I64, types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_pad_right_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_pad_right",
        &[types::I64, types::I64],
        &[types::I64],
    )?;
    let intrinsic_hrtime_ref =
        declare_helper_func(module, &mut func, "jit_rt_hrtime", &[], &[types::I64])?;
    let intrinsic_string_hash_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_hash",
        &[types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_split_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_split",
        &[types::I64, types::I64],
        &[types::I64],
    )?;
    let _intrinsic_string_join_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_join",
        &[types::I64, types::I64],
        &[types::I64],
    )?;

    let mut builder = FunctionBuilder::new(&mut func, fb_ctx);

    let num_regs = (cell.registers as usize)
        .max(cell.params.len())
        .clamp(1, MAX_REGS);
    let mut regs = HybridRegs::new(num_regs);

    // Track the semantic type of each variable for type-aware code generation.
    let mut var_types: HashMap<u32, JitVarType> = HashMap::new();

    // Pre-scan constants and intrinsics to determine which registers receive float/string values.
    // This is a forward pass so that intrinsic type-propagation works correctly
    // (e.g., abs(float_reg) -> float_reg, sqrt(int_reg) -> float_reg).
    let mut float_regs: std::collections::HashSet<u16> = std::collections::HashSet::new();
    let mut string_regs: std::collections::HashSet<u16> = std::collections::HashSet::new();
    // Seed float_regs from parameters
    for (i, p) in cell.params.iter().enumerate() {
        if p.ty == "Float" {
            float_regs.insert(i as u16);
        }
    }
    for inst in &cell.instructions {
        match inst.op {
            OpCode::LoadK => {
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
            OpCode::Intrinsic => {
                let intrinsic_id = inst.b as u32;
                let arg_base = inst.c;
                match intrinsic_id {
                    // Intrinsics that ALWAYS produce float results
                    57 | 58 | 59 | 60 | 62 | 63 | 64 | 123 | 124 | 127 | 128 | 138 | 139 => {
                        // Round, Ceil, Floor, Sqrt, Log, Sin, Cos, Log2, Log10, MathPi, MathE, Tan, Trunc
                        float_regs.insert(inst.a);
                    }
                    // Intrinsics that produce float when input is float
                    26 | 27 | 28 | 65 => {
                        // Abs, Min, Max, Clamp
                        if float_regs.contains(&arg_base) {
                            float_regs.insert(inst.a);
                        }
                    }
                    // Pow: float if base is float
                    61 => {
                        if float_regs.contains(&arg_base) {
                            float_regs.insert(inst.a);
                        }
                    }
                    // ToFloat / ParseFloat always produce float
                    12 | 122 => {
                        float_regs.insert(inst.a);
                    }
                    // String-producing intrinsics: ToString, StringConcat,
                    // Upper, Lower, Trim, Replace, Slice, PadLeft, PadRight, TypeOf
                    10 | 106 | 13 | 19 | 20 | 21 | 22 | 23 | 55 | 56 => {
                        string_regs.insert(inst.a);
                    }
                    _ => {}
                }
            }
            // Arithmetic ops propagate float: if either operand is float, result is float
            OpCode::Add
            | OpCode::Sub
            | OpCode::Mul
            | OpCode::Div
            | OpCode::FloorDiv
            | OpCode::Mod
            | OpCode::Pow
            | OpCode::Neg => {
                if inst.op == OpCode::Neg {
                    if float_regs.contains(&inst.b) {
                        float_regs.insert(inst.a);
                    }
                } else if float_regs.contains(&inst.b) || float_regs.contains(&inst.c) {
                    float_regs.insert(inst.a);
                }
            }
            // Move/MoveOwn propagate float type
            OpCode::Move | OpCode::MoveOwn => {
                if float_regs.contains(&inst.b) {
                    float_regs.insert(inst.a);
                }
            }
            // NullCo propagates float/string from either operand
            OpCode::NullCo => {
                if float_regs.contains(&inst.b) || float_regs.contains(&inst.c) {
                    float_regs.insert(inst.a);
                }
                if string_regs.contains(&inst.b) || string_regs.contains(&inst.c) {
                    string_regs.insert(inst.a);
                }
            }
            _ => {}
        }
    }

    // Pre-scan: identify registers that hold string constants used ONLY as
    // Call/TailCall callee names. For these we skip heap string allocation.
    let call_name_regs = identify_call_name_registers(cell);

    // Pre-scan: identify chains of string Add/Concat that can be batched into
    // a single jit_rt_string_concat_multi call.
    let string_param_regs: std::collections::HashSet<u16> = cell
        .params
        .iter()
        .enumerate()
        .filter(|(_i, p)| p.ty == "String")
        .map(|(i, _p)| i as u16)
        .collect();
    let concat_chains = identify_concat_chains(cell, &string_param_regs);

    let self_tco = has_self_tail_call(cell);
    let block_starts = collect_block_starts(&cell.instructions);

    // Classify which registers need Cranelift Variable (multi-block SSA) vs
    // pure SSA Value tracking (single-block). This avoids declaring up to
    // 65,536 Variables when most registers are local to one basic block.
    let multi_block_regs = classify_multi_block_regs(cell, &block_starts, self_tco);

    // Declare Cranelift Variables ONLY for multi-block registers.
    // Single-block registers use pure SSA Values tracked in regs.ssa_vals.
    for i in 0..num_regs {
        let var_ty = if i < cell.params.len() {
            let param_ty_str = &cell.params[i].ty;
            if param_ty_str == "Float" {
                JitVarType::Float
            } else if param_ty_str == "String" {
                JitVarType::Str
            } else {
                JitVarType::Int
            }
        } else if float_regs.contains(&(i as u16)) {
            JitVarType::Float
        } else {
            // Both int and string regs use I64 at the Cranelift level.
            // The semantic type for string regs is set later when LoadK executes.
            JitVarType::Int
        };
        // NaN-boxing: all registers are I64 regardless of semantic type
        let clif_ty = types::I64;
        var_types.insert(i as u32, var_ty);

        if multi_block_regs.contains(&(i as u16)) {
            let var = Variable::from_u32(i as u32);
            builder.declare_var(var, clif_ty);
            regs.vars.insert(i as u16, var);
        }
    }

    let entry_block = builder.create_block();
    builder.append_block_params_for_function_params(entry_block);
    builder.switch_to_block(entry_block);

    // Initialize function parameters from block params.
    // Callers (execute_jit_*) NaN-box Int parameters before calling, so
    // the values arrive already in NaN-boxed form. We just store them
    // directly into their registers.
    for (i, _param) in cell.params.iter().enumerate() {
        let val = builder.block_params(entry_block)[i];
        if let Some(&var) = regs.vars.get(&(i as u16)) {
            builder.def_var(var, val);
        }
        // Also set SSA value for single-block registers that don't have a Variable
        if !regs.vars.contains_key(&(i as u16)) {
            regs.ssa_vals.insert(i as u16, val);
        }
    }

    // Zero-initialize only multi-block (Variable) registers.
    // SSA-only registers get a lazy zero default in use_var when no value is set.
    for i in cell.params.len()..num_regs {
        if let Some(&var) = regs.vars.get(&(i as u16)) {
            let vty = var_types
                .get(&(i as u32))
                .copied()
                .unwrap_or(JitVarType::Int);
            // NaN-boxing: all registers are I64
            let zero = match vty {
                JitVarType::Float => {
                    // f64 0.0 has bits = 0i64, which is the NaN-boxed form
                    builder.ins().iconst(types::I64, 0)
                }
                JitVarType::Int => {
                    // NaN-boxed integer 0 = (0 << 1) | 1 = 1
                    builder.ins().iconst(types::I64, nan_box_int(0))
                }
                JitVarType::Bool => {
                    // NaN-boxed false sentinel
                    builder.ins().iconst(types::I64, NAN_BOX_FALSE)
                }
                JitVarType::Str => {
                    // Null/zero pointer
                    builder.ins().iconst(types::I64, 0)
                }
            };
            builder.def_var(var, zero);
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
    let mut pending_test: Option<u16> = None;

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

        // Skip intermediate Add/Concat instructions that are part of a
        // batch-concat chain. Their work is handled at the chain tail.
        if concat_chains.skip_pcs.contains(&pc) {
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
                                // base (callee name). Skip heap string allocation.
                                let dummy = builder.ins().iconst(types::I64, 0);
                                var_types.insert(a as u32, JitVarType::Int);
                                def_var(&mut builder, &mut regs, a, dummy);
                            } else {
                                // Drop the old string value if the dest register held one.
                                if var_types.get(&(a as u32)) == Some(&JitVarType::Str) {
                                    let old = use_var(&mut builder, &regs, &var_types, a);
                                    builder.ins().call(str_drop_ref, &[old]);
                                }
                                // Inline JitString allocation. Cranelift sees the
                                // individual malloc + field stores, enabling constant
                                // folding of struct fields and dead-store elimination.
                                //
                                // JitString layout (40 bytes, repr(C)):
                                //   offset  0: refcount   (i64) = 1
                                //   offset  8: len        (i64) - byte length
                                //   offset 16: char_count (i64) - Unicode character count
                                //   offset 24: cap        (i64) = len
                                //   offset 32: ptr        (*mut u8)
                                let str_bytes = s.as_bytes();
                                let len = str_bytes.len() as i64;
                                let char_count = s.chars().count() as i64;
                                let flags = MemFlags::new();

                                // 1. Allocate the 40-byte JitString struct.
                                let struct_size = builder.ins().iconst(types::I64, 40);
                                let struct_call = builder.ins().call(malloc_ref, &[struct_size]);
                                let struct_ptr = builder.inst_results(struct_call)[0];

                                // 2. Store struct fields (all known at compile time).
                                let rc_one = builder.ins().iconst(types::I64, 1);
                                builder.ins().store(flags, rc_one, struct_ptr, 0); // refcount
                                let len_val = builder.ins().iconst(types::I64, len);
                                builder.ins().store(flags, len_val, struct_ptr, 8); // len
                                let char_count_val = builder.ins().iconst(types::I64, char_count);
                                builder.ins().store(flags, char_count_val, struct_ptr, 16); // char_count
                                builder.ins().store(flags, len_val, struct_ptr, 24); // cap = len

                                // 3. Allocate data buffer and copy string bytes.
                                //    Uses alloc_bytes_ref (Vec-compatible) so that
                                //    JitString::drop_ref can free via Vec::from_raw_parts.
                                if len > 0 {
                                    let data_call = builder.ins().call(alloc_bytes_ref, &[len_val]);
                                    let data_ptr = builder.inst_results(data_call)[0];
                                    let src_ptr =
                                        builder.ins().iconst(types::I64, str_bytes.as_ptr() as i64);
                                    builder
                                        .ins()
                                        .call(memcpy_ref, &[data_ptr, src_ptr, len_val]);
                                    builder.ins().store(flags, data_ptr, struct_ptr, 32);
                                // ptr
                                } else {
                                    let null_ptr = builder.ins().iconst(types::I64, 0);
                                    builder.ins().store(flags, null_ptr, struct_ptr, 32);
                                    // ptr = null
                                }

                                var_types.insert(a as u32, JitVarType::Str);
                                def_var(&mut builder, &mut regs, a, struct_ptr);
                            }
                        }
                        Constant::Float(_) => {
                            let val = lower_constant(&mut builder, cell, bx)?;
                            var_types.insert(a as u32, JitVarType::Float);
                            def_var(&mut builder, &mut regs, a, val);
                        }
                        _ => {
                            let val = lower_constant(&mut builder, cell, bx)?;
                            var_types.insert(a as u32, JitVarType::Int);
                            def_var(&mut builder, &mut regs, a, val);
                        }
                    }
                } else {
                    let val = lower_constant(&mut builder, cell, bx)?;
                    var_types.insert(a as u32, JitVarType::Int);
                    def_var(&mut builder, &mut regs, a, val);
                }
            }
            OpCode::LoadBool => {
                let a = inst.a;
                let b_val = inst.b;
                // NaN-box the boolean: true → NAN_BOX_TRUE, false → NAN_BOX_FALSE
                let nan_boxed = if b_val != 0 {
                    NAN_BOX_TRUE
                } else {
                    NAN_BOX_FALSE
                };
                let val = builder.ins().iconst(types::I64, nan_boxed);
                def_var(&mut builder, &mut regs, a, val);
            }
            OpCode::LoadInt => {
                let a = inst.a;
                let imm = inst.sbx() as i64;
                // NaN-box the integer: (imm << 1) | 1
                let val = builder.ins().iconst(types::I64, nan_box_int(imm));
                def_var(&mut builder, &mut regs, a, val);
            }
            OpCode::LoadNil => {
                let a = inst.a;
                let count = inst.b as usize;
                // NaN-box null: canonical quiet NaN sentinel
                let null_val = builder.ins().iconst(types::I64, NAN_BOX_NULL);
                for i in 0..=count {
                    let r = a as usize + i;
                    if r < regs.num_regs {
                        def_var(&mut builder, &mut regs, r as u16, null_val);
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
                        let old = use_var(&mut builder, &regs, &var_types, inst.a);
                        builder.ins().call(str_drop_ref, &[old]);
                    }
                    // Clone via inline refcount increment (3 instructions,
                    // no function call). JitString layout: refcount is at
                    // offset 0. We load it, add 1, store back. The result
                    // is the same pointer — both source and dest share the
                    // underlying string data.
                    let src = use_var(&mut builder, &regs, &var_types, inst.b);
                    let flags = MemFlags::new();
                    let rc = builder.ins().load(types::I64, flags, src, 0);
                    let rc_plus_one = builder.ins().iadd_imm(rc, 1);
                    builder.ins().store(flags, rc_plus_one, src, 0);
                    src
                } else {
                    use_var(&mut builder, &regs, &var_types, inst.b)
                };
                let actual_ty = if call_name_regs.contains(&inst.b) {
                    JitVarType::Int
                } else {
                    src_ty
                };
                var_types.insert(inst.a as u32, actual_ty);
                def_var(&mut builder, &mut regs, inst.a, val);
            }
            OpCode::MoveOwn => {
                // MoveOwn transfers ownership — no clone needed even for strings.
                let val = use_var(&mut builder, &regs, &var_types, inst.b);
                if let Some(&src_ty) = var_types.get(&(inst.b as u32)) {
                    var_types.insert(inst.a as u32, src_ty);
                    // For strings, null out the source register so the
                    // Return-time cleanup doesn't double-free the pointer.
                    if src_ty == JitVarType::Str && inst.a != inst.b {
                        let null = builder.ins().iconst(types::I64, 0);
                        def_var(&mut builder, &mut regs, inst.b, null);
                    }
                }
                def_var(&mut builder, &mut regs, inst.a, val);
            }

            // Arithmetic (type-aware)
            OpCode::Add => {
                let lhs_ty = var_types
                    .get(&(inst.b as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let rhs_ty = var_types
                    .get(&(inst.c as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                if lhs_ty == JitVarType::Str || rhs_ty == JitVarType::Str {
                    // Check if this is the tail of a batch-concat chain.
                    if let Some(leaves) = concat_chains.chains.get(&pc) {
                        let dest_ty = var_types
                            .get(&(inst.a as u32))
                            .copied()
                            .unwrap_or(JitVarType::Int);
                        // Drop old destination string if it exists and differs
                        // from all leaf operands.
                        let old_dest = if dest_ty == JitVarType::Str && !leaves.contains(&inst.a) {
                            Some(use_var(&mut builder, &regs, &var_types, inst.a))
                        } else {
                            None
                        };

                        // Build a stack slot holding the array of string pointers.
                        let slot_size = (leaves.len() * 8) as u32;
                        let slot_data =
                            StackSlotData::new(StackSlotKind::ExplicitSlot, slot_size, 3);
                        let slot = builder.create_sized_stack_slot(slot_data);

                        for (i, &leaf_reg) in leaves.iter().enumerate() {
                            let val = use_var(&mut builder, &regs, &var_types, leaf_reg);
                            builder.ins().stack_store(val, slot, (i * 8) as i32);
                        }

                        let addr = builder.ins().stack_addr(types::I64, slot, 0);
                        let count_val = builder.ins().iconst(types::I64, leaves.len() as i64);
                        let call = builder.ins().call(str_concat_multi_ref, &[addr, count_val]);
                        let result = builder.inst_results(call)[0];

                        if let Some(old) = old_dest {
                            builder.ins().call(str_drop_ref, &[old]);
                        }

                        var_types.insert(inst.a as u32, JitVarType::Str);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    } else {
                        // String concatenation (non-chain) — safe to read operands now.
                        let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                        let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                        let dest_ty = var_types
                            .get(&(inst.a as u32))
                            .copied()
                            .unwrap_or(JitVarType::Int);

                        // Optimization: if dest == lhs (a = a + c), use in-place mutation
                        if dest_ty == JitVarType::Str && inst.a == inst.b {
                            // In-place: a = a + c
                            // Inline fast path: if refcount==1 && cap>=len_a+len_b,
                            // do memcpy inline and skip the runtime call entirely.
                            let flags = MemFlags::new();

                            // Load JitString fields for lhs (a)
                            let refcount_a = builder.ins().load(types::I64, flags, lhs, 0);
                            let len_a = builder.ins().load(types::I64, flags, lhs, 8);
                            let cap_a = builder.ins().load(types::I64, flags, lhs, 16);

                            // Load len for rhs (b)
                            let len_b = builder.ins().load(types::I64, flags, rhs, 8);

                            // Compute total_len = len_a + len_b
                            let total_len = builder.ins().iadd(len_a, len_b);

                            // Check refcount == 1 (exclusive ownership)
                            let one = builder.ins().iconst(types::I64, 1);
                            let rc_ok = builder.ins().icmp(IntCC::Equal, refcount_a, one);

                            // Check cap >= total_len (capacity suffices)
                            let cap_ok = builder.ins().icmp(
                                IntCC::SignedGreaterThanOrEqual,
                                cap_a,
                                total_len,
                            );

                            // Both conditions must hold
                            let both_ok = builder.ins().band(rc_ok, cap_ok);

                            // Create blocks for inline fast path, slow path, and merge
                            let inline_block = builder.create_block();
                            let slow_block = builder.create_block();
                            let merge_block = builder.create_block();
                            builder.append_block_param(merge_block, types::I64);

                            builder
                                .ins()
                                .brif(both_ok, inline_block, &[], slow_block, &[]);

                            // --- Inline fast path block ---
                            builder.switch_to_block(inline_block);
                            let ptr_a = builder.ins().load(types::I64, flags, lhs, 24);
                            let ptr_b = builder.ins().load(types::I64, flags, rhs, 24);
                            let dst = builder.ins().iadd(ptr_a, len_a);
                            builder.ins().call(memcpy_ref, &[dst, ptr_b, len_b]);
                            // Update len field in-place
                            builder.ins().store(flags, total_len, lhs, 8);
                            builder.ins().jump(merge_block, &[lhs]);

                            // --- Slow path block ---
                            builder.switch_to_block(slow_block);
                            let call = builder.ins().call(str_concat_mut_ref, &[lhs, rhs]);
                            let slow_result = builder.inst_results(call)[0];
                            builder.ins().jump(merge_block, &[slow_result]);

                            // --- Merge block ---
                            builder.switch_to_block(merge_block);
                            let result = builder.block_params(merge_block)[0];
                            var_types.insert(inst.a as u32, JitVarType::Str);
                            def_var(&mut builder, &mut regs, inst.a, result);
                            // Note: lhs is consumed by concat_mut, no need to drop
                        } else {
                            // Standard case: create new string
                            let old_dest = if dest_ty == JitVarType::Str
                                && inst.a != inst.b
                                && inst.a != inst.c
                            {
                                Some(use_var(&mut builder, &regs, &var_types, inst.a))
                            } else {
                                None
                            };

                            let call = builder.ins().call(str_concat_ref, &[lhs, rhs]);
                            let result = builder.inst_results(call)[0];

                            if let Some(old) = old_dest {
                                builder.ins().call(str_drop_ref, &[old]);
                            } else if dest_ty == JitVarType::Str && inst.a == inst.c {
                                // a = b + a: drop the old value of a (which is rhs)
                                builder.ins().call(str_drop_ref, &[rhs]);
                            }

                            var_types.insert(inst.a as u32, JitVarType::Str);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        }
                    }
                } else if lhs_ty == JitVarType::Float || rhs_ty == JitVarType::Float {
                    let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                    let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                    // NaN-boxing: unbox floats (bitcast I64→F64), add, rebox
                    let lhs_f = emit_unbox_float(&mut builder, lhs);
                    let rhs_f = emit_unbox_float(&mut builder, rhs);
                    let res_f = builder.ins().fadd(lhs_f, rhs_f);
                    let res = emit_box_float(&mut builder, res_f);
                    var_types.insert(inst.a as u32, JitVarType::Float);
                    def_var(&mut builder, &mut regs, inst.a, res);
                } else {
                    let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                    let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                    // NaN-boxing: unbox ints (sshr 1), add, rebox
                    let lhs_i = emit_unbox_int(&mut builder, lhs);
                    let rhs_i = emit_unbox_int(&mut builder, rhs);
                    let res_i = builder.ins().iadd(lhs_i, rhs_i);
                    let res = emit_box_int(&mut builder, res_i);
                    var_types.insert(inst.a as u32, JitVarType::Int);
                    def_var(&mut builder, &mut regs, inst.a, res);
                }
            }
            // Explicit string/list concatenation operator (++).
            // In the JIT we only support string concatenation (lists are not
            // JIT-compiled). The logic mirrors the string branch of Add.
            OpCode::Concat => {
                // Check if this is the tail of a batch-concat chain.
                if let Some(leaves) = concat_chains.chains.get(&pc) {
                    let dest_ty = var_types
                        .get(&(inst.a as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    // Drop old destination string if it exists and differs
                    // from all leaf operands.
                    let old_dest = if dest_ty == JitVarType::Str && !leaves.contains(&inst.a) {
                        Some(use_var(&mut builder, &regs, &var_types, inst.a))
                    } else {
                        None
                    };

                    // Build a stack slot holding the array of string pointers.
                    let slot_size = (leaves.len() * 8) as u32;
                    let slot_data = StackSlotData::new(StackSlotKind::ExplicitSlot, slot_size, 3);
                    let slot = builder.create_sized_stack_slot(slot_data);

                    for (i, &leaf_reg) in leaves.iter().enumerate() {
                        let val = use_var(&mut builder, &regs, &var_types, leaf_reg);
                        builder.ins().stack_store(val, slot, (i * 8) as i32);
                    }

                    let addr = builder.ins().stack_addr(types::I64, slot, 0);
                    let count_val = builder.ins().iconst(types::I64, leaves.len() as i64);
                    let call = builder.ins().call(str_concat_multi_ref, &[addr, count_val]);
                    let result = builder.inst_results(call)[0];

                    if let Some(old) = old_dest {
                        builder.ins().call(str_drop_ref, &[old]);
                    }

                    var_types.insert(inst.a as u32, JitVarType::Str);
                    def_var(&mut builder, &mut regs, inst.a, result);
                } else {
                    // Non-chain: regular concat
                    let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                    let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                    let dest_ty = var_types
                        .get(&(inst.a as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);

                    // Optimization: if dest == lhs (a = a ++ c), use in-place mutation
                    if dest_ty == JitVarType::Str && inst.a == inst.b {
                        // Inline fast path: if refcount==1 && cap>=len_a+len_b,
                        // do memcpy inline and skip the runtime call entirely.
                        let flags = MemFlags::new();

                        // Load JitString fields for lhs (a)
                        let refcount_a = builder.ins().load(types::I64, flags, lhs, 0);
                        let len_a = builder.ins().load(types::I64, flags, lhs, 8);
                        let cap_a = builder.ins().load(types::I64, flags, lhs, 16);

                        // Load len for rhs (b)
                        let len_b = builder.ins().load(types::I64, flags, rhs, 8);

                        // Compute total_len = len_a + len_b
                        let total_len = builder.ins().iadd(len_a, len_b);

                        // Check refcount == 1 (exclusive ownership)
                        let one = builder.ins().iconst(types::I64, 1);
                        let rc_ok = builder.ins().icmp(IntCC::Equal, refcount_a, one);

                        // Check cap >= total_len (capacity suffices)
                        let cap_ok =
                            builder
                                .ins()
                                .icmp(IntCC::SignedGreaterThanOrEqual, cap_a, total_len);

                        // Both conditions must hold
                        let both_ok = builder.ins().band(rc_ok, cap_ok);

                        // Create blocks for inline fast path, slow path, and merge
                        let inline_block = builder.create_block();
                        let slow_block = builder.create_block();
                        let merge_block = builder.create_block();
                        builder.append_block_param(merge_block, types::I64);

                        builder
                            .ins()
                            .brif(both_ok, inline_block, &[], slow_block, &[]);

                        // --- Inline fast path block ---
                        builder.switch_to_block(inline_block);
                        let ptr_a = builder.ins().load(types::I64, flags, lhs, 24);
                        let ptr_b = builder.ins().load(types::I64, flags, rhs, 24);
                        let dst = builder.ins().iadd(ptr_a, len_a);
                        builder.ins().call(memcpy_ref, &[dst, ptr_b, len_b]);
                        // Update len field in-place
                        builder.ins().store(flags, total_len, lhs, 8);
                        builder.ins().jump(merge_block, &[lhs]);

                        // --- Slow path block ---
                        builder.switch_to_block(slow_block);
                        let call = builder.ins().call(str_concat_mut_ref, &[lhs, rhs]);
                        let slow_result = builder.inst_results(call)[0];
                        builder.ins().jump(merge_block, &[slow_result]);

                        // --- Merge block ---
                        builder.switch_to_block(merge_block);
                        let result = builder.block_params(merge_block)[0];
                        var_types.insert(inst.a as u32, JitVarType::Str);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    } else {
                        // Drop old destination string if it exists and differs from operands.
                        let old_dest =
                            if dest_ty == JitVarType::Str && inst.a != inst.b && inst.a != inst.c {
                                Some(use_var(&mut builder, &regs, &var_types, inst.a))
                            } else {
                                None
                            };

                        let call = builder.ins().call(str_concat_ref, &[lhs, rhs]);
                        let result = builder.inst_results(call)[0];

                        if let Some(old) = old_dest {
                            builder.ins().call(str_drop_ref, &[old]);
                        } else if dest_ty == JitVarType::Str && inst.a == inst.c {
                            // a = b ++ a: drop the old value of a (which is rhs)
                            builder.ins().call(str_drop_ref, &[rhs]);
                        }

                        var_types.insert(inst.a as u32, JitVarType::Str);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }
                }
            }

            OpCode::Sub => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let is_float = is_float_op(&var_types, inst.b, inst.c);
                let res = if is_float {
                    // NaN-boxing: unbox floats, sub, rebox
                    let lhs_f = emit_unbox_float(&mut builder, lhs);
                    let rhs_f = emit_unbox_float(&mut builder, rhs);
                    let r = builder.ins().fsub(lhs_f, rhs_f);
                    emit_box_float(&mut builder, r)
                } else {
                    // NaN-boxing: unbox ints, sub, rebox
                    let lhs_i = emit_unbox_int(&mut builder, lhs);
                    let rhs_i = emit_unbox_int(&mut builder, rhs);
                    let r = builder.ins().isub(lhs_i, rhs_i);
                    emit_box_int(&mut builder, r)
                };
                var_types.insert(
                    inst.a as u32,
                    if is_float {
                        JitVarType::Float
                    } else {
                        JitVarType::Int
                    },
                );
                def_var(&mut builder, &mut regs, inst.a, res);
            }
            OpCode::Mul => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let is_float = is_float_op(&var_types, inst.b, inst.c);
                let res = if is_float {
                    let lhs_f = emit_unbox_float(&mut builder, lhs);
                    let rhs_f = emit_unbox_float(&mut builder, rhs);
                    let r = builder.ins().fmul(lhs_f, rhs_f);
                    emit_box_float(&mut builder, r)
                } else {
                    let lhs_i = emit_unbox_int(&mut builder, lhs);
                    let rhs_i = emit_unbox_int(&mut builder, rhs);
                    let r = builder.ins().imul(lhs_i, rhs_i);
                    emit_box_int(&mut builder, r)
                };
                var_types.insert(
                    inst.a as u32,
                    if is_float {
                        JitVarType::Float
                    } else {
                        JitVarType::Int
                    },
                );
                def_var(&mut builder, &mut regs, inst.a, res);
            }
            OpCode::Div => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let is_float = is_float_op(&var_types, inst.b, inst.c);
                let res = if is_float {
                    let lhs_f = emit_unbox_float(&mut builder, lhs);
                    let rhs_f = emit_unbox_float(&mut builder, rhs);
                    let r = builder.ins().fdiv(lhs_f, rhs_f);
                    emit_box_float(&mut builder, r)
                } else {
                    let lhs_i = emit_unbox_int(&mut builder, lhs);
                    let rhs_i = emit_unbox_int(&mut builder, rhs);
                    let r = builder.ins().sdiv(lhs_i, rhs_i);
                    emit_box_int(&mut builder, r)
                };
                var_types.insert(
                    inst.a as u32,
                    if is_float {
                        JitVarType::Float
                    } else {
                        JitVarType::Int
                    },
                );
                def_var(&mut builder, &mut regs, inst.a, res);
            }
            OpCode::Mod => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let is_float = is_float_op(&var_types, inst.b, inst.c);
                if is_float {
                    // NaN-boxing: unbox floats, call fmod, rebox
                    let lhs_f = emit_unbox_float(&mut builder, lhs);
                    let rhs_f = emit_unbox_float(&mut builder, rhs);
                    let call = builder.ins().call(intrinsic_fmod_ref, &[lhs_f, rhs_f]);
                    let result_f = builder.inst_results(call)[0];
                    let result = emit_box_float(&mut builder, result_f);
                    var_types.insert(inst.a as u32, JitVarType::Float);
                    def_var(&mut builder, &mut regs, inst.a, result);
                } else {
                    // NaN-boxing: unbox ints, srem, rebox
                    let lhs_i = emit_unbox_int(&mut builder, lhs);
                    let rhs_i = emit_unbox_int(&mut builder, rhs);
                    let res_i = builder.ins().srem(lhs_i, rhs_i);
                    let res = emit_box_int(&mut builder, res_i);
                    var_types.insert(inst.a as u32, JitVarType::Int);
                    def_var(&mut builder, &mut regs, inst.a, res);
                }
            }
            OpCode::Neg => {
                let operand = use_var(&mut builder, &regs, &var_types, inst.b);
                let is_float = var_types.get(&(inst.b as u32)).copied() == Some(JitVarType::Float);
                let res = if is_float {
                    let f = emit_unbox_float(&mut builder, operand);
                    let r = builder.ins().fneg(f);
                    emit_box_float(&mut builder, r)
                } else {
                    let i = emit_unbox_int(&mut builder, operand);
                    let r = builder.ins().ineg(i);
                    emit_box_int(&mut builder, r)
                };
                var_types.insert(
                    inst.a as u32,
                    if is_float {
                        JitVarType::Float
                    } else {
                        JitVarType::Int
                    },
                );
                def_var(&mut builder, &mut regs, inst.a, res);
            }
            OpCode::FloorDiv => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let is_float = is_float_op(&var_types, inst.b, inst.c);
                let res = if is_float {
                    let lhs_f = emit_unbox_float(&mut builder, lhs);
                    let rhs_f = emit_unbox_float(&mut builder, rhs);
                    let div = builder.ins().fdiv(lhs_f, rhs_f);
                    let floored = builder.ins().floor(div);
                    emit_box_float(&mut builder, floored)
                } else {
                    let lhs_i = emit_unbox_int(&mut builder, lhs);
                    let rhs_i = emit_unbox_int(&mut builder, rhs);
                    let r = builder.ins().sdiv(lhs_i, rhs_i);
                    emit_box_int(&mut builder, r)
                };
                var_types.insert(
                    inst.a as u32,
                    if is_float {
                        JitVarType::Float
                    } else {
                        JitVarType::Int
                    },
                );
                def_var(&mut builder, &mut regs, inst.a, res);
            }
            OpCode::Pow => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let is_float = is_float_op(&var_types, inst.b, inst.c);
                if is_float {
                    // NaN-boxing: unbox floats, call pow, rebox
                    let lhs_f = emit_unbox_float(&mut builder, lhs);
                    let rhs_f = emit_unbox_float(&mut builder, rhs);
                    let call = builder.ins().call(intrinsic_pow_float_ref, &[lhs_f, rhs_f]);
                    let result_f = builder.inst_results(call)[0];
                    let result = emit_box_float(&mut builder, result_f);
                    var_types.insert(inst.a as u32, JitVarType::Float);
                    def_var(&mut builder, &mut regs, inst.a, result);
                } else {
                    // NaN-boxing: unbox ints, call pow, rebox
                    let lhs_i = emit_unbox_int(&mut builder, lhs);
                    let rhs_i = emit_unbox_int(&mut builder, rhs);
                    let call = builder.ins().call(intrinsic_pow_int_ref, &[lhs_i, rhs_i]);
                    let result_i = builder.inst_results(call)[0];
                    let result = emit_box_int(&mut builder, result_i);
                    var_types.insert(inst.a as u32, JitVarType::Int);
                    def_var(&mut builder, &mut regs, inst.a, result);
                }
            }

            // Bitwise (NaN-boxing: unbox ints, operate, rebox)
            OpCode::BitOr => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let lhs_i = emit_unbox_int(&mut builder, lhs);
                let rhs_i = emit_unbox_int(&mut builder, rhs);
                let res_i = builder.ins().bor(lhs_i, rhs_i);
                let res = emit_box_int(&mut builder, res_i);
                def_var(&mut builder, &mut regs, inst.a, res);
            }
            OpCode::BitAnd => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let lhs_i = emit_unbox_int(&mut builder, lhs);
                let rhs_i = emit_unbox_int(&mut builder, rhs);
                let res_i = builder.ins().band(lhs_i, rhs_i);
                let res = emit_box_int(&mut builder, res_i);
                def_var(&mut builder, &mut regs, inst.a, res);
            }
            OpCode::BitXor => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let lhs_i = emit_unbox_int(&mut builder, lhs);
                let rhs_i = emit_unbox_int(&mut builder, rhs);
                let res_i = builder.ins().bxor(lhs_i, rhs_i);
                let res = emit_box_int(&mut builder, res_i);
                def_var(&mut builder, &mut regs, inst.a, res);
            }
            OpCode::BitNot => {
                let operand = use_var(&mut builder, &regs, &var_types, inst.b);
                let i = emit_unbox_int(&mut builder, operand);
                let res_i = builder.ins().bnot(i);
                let res = emit_box_int(&mut builder, res_i);
                def_var(&mut builder, &mut regs, inst.a, res);
            }
            OpCode::Shl => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let lhs_i = emit_unbox_int(&mut builder, lhs);
                let rhs_i = emit_unbox_int(&mut builder, rhs);
                let res_i = builder.ins().ishl(lhs_i, rhs_i);
                let res = emit_box_int(&mut builder, res_i);
                def_var(&mut builder, &mut regs, inst.a, res);
            }
            OpCode::Shr => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let lhs_i = emit_unbox_int(&mut builder, lhs);
                let rhs_i = emit_unbox_int(&mut builder, rhs);
                let res_i = builder.ins().sshr(lhs_i, rhs_i);
                let res = emit_box_int(&mut builder, res_i);
                def_var(&mut builder, &mut regs, inst.a, res);
            }

            // Comparison (type-aware)
            // NaN-boxing: comparison results are NaN-boxed ints (0 or 1)
            OpCode::Eq => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let lhs_ty = var_types
                    .get(&(inst.b as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let rhs_ty = var_types
                    .get(&(inst.c as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let cmp_i1 = if lhs_ty == JitVarType::Str || rhs_ty == JitVarType::Str {
                    // String equality returns raw 0/1 — need to NaN-box
                    let call = builder.ins().call(str_eq_ref, &[lhs, rhs]);
                    let raw = builder.inst_results(call)[0];
                    // raw is already 0 or 1, box it
                    let res = emit_box_int(&mut builder, raw);
                    var_types.insert(inst.a as u32, JitVarType::Int);
                    def_var(&mut builder, &mut regs, inst.a, res);
                    continue;
                } else if lhs_ty == JitVarType::Float || rhs_ty == JitVarType::Float {
                    let lhs_f = emit_unbox_float(&mut builder, lhs);
                    let rhs_f = emit_unbox_float(&mut builder, rhs);
                    builder.ins().fcmp(FloatCC::Equal, lhs_f, rhs_f)
                } else {
                    // Int: can compare NaN-boxed values directly (monotonic)
                    builder.ins().icmp(IntCC::Equal, lhs, rhs)
                };
                let raw = builder.ins().uextend(types::I64, cmp_i1);
                let res = emit_box_int(&mut builder, raw);
                var_types.insert(inst.a as u32, JitVarType::Int);
                def_var(&mut builder, &mut regs, inst.a, res);
            }
            OpCode::Lt => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let lhs_ty = var_types
                    .get(&(inst.b as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let rhs_ty = var_types
                    .get(&(inst.c as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let cmp_i1 = if lhs_ty == JitVarType::Str || rhs_ty == JitVarType::Str {
                    let call = builder.ins().call(str_cmp_ref, &[lhs, rhs]);
                    let cmp_result = builder.inst_results(call)[0];
                    let zero = builder.ins().iconst(types::I64, 0);
                    builder.ins().icmp(IntCC::SignedLessThan, cmp_result, zero)
                } else if lhs_ty == JitVarType::Float || rhs_ty == JitVarType::Float {
                    let lhs_f = emit_unbox_float(&mut builder, lhs);
                    let rhs_f = emit_unbox_float(&mut builder, rhs);
                    builder.ins().fcmp(FloatCC::LessThan, lhs_f, rhs_f)
                } else {
                    // Int: NaN-boxed ordering is preserved (2a+1 < 2b+1 iff a < b)
                    builder.ins().icmp(IntCC::SignedLessThan, lhs, rhs)
                };
                let raw = builder.ins().uextend(types::I64, cmp_i1);
                let res = emit_box_int(&mut builder, raw);
                var_types.insert(inst.a as u32, JitVarType::Int);
                def_var(&mut builder, &mut regs, inst.a, res);
            }
            OpCode::Le => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let lhs_ty = var_types
                    .get(&(inst.b as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let rhs_ty = var_types
                    .get(&(inst.c as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let cmp_i1 = if lhs_ty == JitVarType::Str || rhs_ty == JitVarType::Str {
                    let call = builder.ins().call(str_cmp_ref, &[lhs, rhs]);
                    let cmp_result = builder.inst_results(call)[0];
                    let zero = builder.ins().iconst(types::I64, 0);
                    builder
                        .ins()
                        .icmp(IntCC::SignedLessThanOrEqual, cmp_result, zero)
                } else if lhs_ty == JitVarType::Float || rhs_ty == JitVarType::Float {
                    let lhs_f = emit_unbox_float(&mut builder, lhs);
                    let rhs_f = emit_unbox_float(&mut builder, rhs);
                    builder.ins().fcmp(FloatCC::LessThanOrEqual, lhs_f, rhs_f)
                } else {
                    builder.ins().icmp(IntCC::SignedLessThanOrEqual, lhs, rhs)
                };
                let raw = builder.ins().uextend(types::I64, cmp_i1);
                let res = emit_box_int(&mut builder, raw);
                var_types.insert(inst.a as u32, JitVarType::Int);
                def_var(&mut builder, &mut regs, inst.a, res);
            }
            OpCode::Not => {
                // Logical NOT: produce NAN_BOX_TRUE if operand is falsy, NAN_BOX_FALSE otherwise.
                // Falsy: int 0 (NaN-boxed = 1), false (NAN_BOX_FALSE), null (NAN_BOX_NULL), float 0.0 (bits = 0)
                let operand = use_var(&mut builder, &regs, &var_types, inst.b);
                let b_ty = var_types
                    .get(&(inst.b as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let falsy_val = match b_ty {
                    JitVarType::Int => builder.ins().iconst(types::I64, nan_box_int(0)),
                    JitVarType::Float => builder.ins().iconst(types::I64, 0i64), // f64 0.0 bits
                    JitVarType::Bool => builder.ins().iconst(types::I64, NAN_BOX_FALSE),
                    JitVarType::Str => builder.ins().iconst(types::I64, 0i64), // null ptr
                };
                let is_falsy = builder.ins().icmp(IntCC::Equal, operand, falsy_val);
                let t = builder.ins().iconst(types::I64, NAN_BOX_TRUE);
                let f = builder.ins().iconst(types::I64, NAN_BOX_FALSE);
                let res = builder.ins().select(is_falsy, t, f);
                var_types.insert(inst.a as u32, JitVarType::Bool);
                def_var(&mut builder, &mut regs, inst.a, res);
            }

            // Logic — short-circuit And/Or with NaN-boxing
            OpCode::And => {
                // Short-circuit AND: if LHS is falsy, result = LHS; else result = RHS.
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let b_ty = var_types
                    .get(&(inst.b as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let falsy_val = match b_ty {
                    JitVarType::Int => builder.ins().iconst(types::I64, nan_box_int(0)),
                    JitVarType::Float => builder.ins().iconst(types::I64, 0i64),
                    JitVarType::Bool => builder.ins().iconst(types::I64, NAN_BOX_FALSE),
                    JitVarType::Str => builder.ins().iconst(types::I64, 0i64),
                };
                let is_falsy = builder.ins().icmp(IntCC::Equal, lhs, falsy_val);
                let res = builder.ins().select(is_falsy, lhs, rhs);
                // Inherit type from RHS (the "truthy" branch value)
                let rhs_ty = var_types
                    .get(&(inst.c as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                var_types.insert(inst.a as u32, rhs_ty);
                def_var(&mut builder, &mut regs, inst.a, res);
            }
            OpCode::Or => {
                // Short-circuit OR: if LHS is truthy, result = LHS; else result = RHS.
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let b_ty = var_types
                    .get(&(inst.b as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let falsy_val = match b_ty {
                    JitVarType::Int => builder.ins().iconst(types::I64, nan_box_int(0)),
                    JitVarType::Float => builder.ins().iconst(types::I64, 0i64),
                    JitVarType::Bool => builder.ins().iconst(types::I64, NAN_BOX_FALSE),
                    JitVarType::Str => builder.ins().iconst(types::I64, 0i64),
                };
                let is_falsy = builder.ins().icmp(IntCC::Equal, lhs, falsy_val);
                let res = builder.ins().select(is_falsy, rhs, lhs);
                // Inherit type from LHS (the "truthy" branch value)
                var_types.insert(inst.a as u32, b_ty);
                def_var(&mut builder, &mut regs, inst.a, res);
            }

            // Null coalescing
            OpCode::NullCo => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let null_sentinel = builder.ins().iconst(types::I64, NAN_BOX_NULL);
                let is_null = builder.ins().icmp(IntCC::Equal, lhs, null_sentinel);
                let res = builder.ins().select(is_null, rhs, lhs);
                let rhs_ty = var_types
                    .get(&(inst.c as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                var_types.insert(inst.a as u32, rhs_ty);
                def_var(&mut builder, &mut regs, inst.a, res);
            }

            // Test
            OpCode::Test => {
                pending_test = Some(inst.a);
            }

            // Control flow
            OpCode::Jmp | OpCode::Break | OpCode::Continue => {
                let offset = inst.sax_val();
                let target_pc = (pc as isize + 1 + offset as isize) as usize;
                let fallthrough_pc = pc + 1;

                let target_block = get_or_create_block(&mut builder, &mut block_map, target_pc);
                let fallthrough_block =
                    get_or_create_block(&mut builder, &mut block_map, fallthrough_pc);

                if let Some(test_reg) = pending_test.take() {
                    let cond = use_var(&mut builder, &regs, &var_types, test_reg);
                    // NaN-boxing truthiness: check against type-specific falsy value
                    let test_ty = var_types
                        .get(&(test_reg as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    let falsy_val = match test_ty {
                        JitVarType::Int => builder.ins().iconst(types::I64, nan_box_int(0)),
                        JitVarType::Float => builder.ins().iconst(types::I64, 0i64),
                        JitVarType::Bool => builder.ins().iconst(types::I64, NAN_BOX_FALSE),
                        JitVarType::Str => builder.ins().iconst(types::I64, 0i64),
                    };
                    let is_truthy = builder.ins().icmp(IntCC::NotEqual, cond, falsy_val);
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
                // Drop all live string registers except the return value.
                let ret_reg = inst.a;
                for (&reg_id, &ty) in &var_types {
                    if ty == JitVarType::Str
                        && reg_id != ret_reg as u32
                        && (reg_id as usize) < regs.num_regs
                    {
                        let v = use_var(&mut builder, &regs, &var_types, reg_id as u16);
                        builder.ins().call(str_drop_ref, &[v]);
                    }
                }
                let val = use_var(&mut builder, &regs, &var_types, ret_reg);
                // NaN-boxing: all values are I64, no bitcast needed.
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

                // Drop the old string in the base register before overwriting.
                if var_types.get(&(base as u32)) == Some(&JitVarType::Str) {
                    let old = use_var(&mut builder, &regs, &var_types, base);
                    builder.ins().call(str_drop_ref, &[old]);
                }

                // Collect string-typed argument registers for cleanup.
                let mut str_arg_regs: Vec<u32> = Vec::new();
                for i in 0..num_args {
                    let arg_reg = (base + 1 + i as u16) as u32;
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
                                let arg_reg = base + 1 + i as u16;
                                args.push(use_var(&mut builder, &regs, &var_types, arg_reg));
                            }
                            let call = builder.ins().call(func_ref, &args);
                            let result = builder.inst_results(call)[0];
                            def_var(&mut builder, &mut regs, base, result);
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            def_var(&mut builder, &mut regs, base, zero);
                        }
                    } else {
                        let zero = builder.ins().iconst(types::I64, 0);
                        def_var(&mut builder, &mut regs, base, zero);
                    }
                } else {
                    let zero = builder.ins().iconst(types::I64, 0);
                    def_var(&mut builder, &mut regs, base, zero);
                }

                // Drop string-typed argument registers after the call.
                for arg_reg in str_arg_regs {
                    if (arg_reg as usize) < regs.num_regs {
                        let v = use_var(&mut builder, &regs, &var_types, arg_reg as u16);
                        builder.ins().call(str_drop_ref, &[v]);
                    }
                    var_types.remove(&arg_reg);
                }

                // Infer return type from callee's declared return type.
                let ret_ty = callee_name
                    .as_ref()
                    .and_then(|n| cell_return_types.get(n.as_str()))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                // Only set Str; Float results stay as Int (I64 bits).
                let effective_ty = if ret_ty == JitVarType::Str {
                    JitVarType::Str
                } else {
                    JitVarType::Int
                };
                var_types.insert(base as u32, effective_ty);
            }
            OpCode::TailCall => {
                let base = inst.a;
                let num_args = inst.b as usize;
                let callee_name = find_callee_name(cell, &cell.instructions, pc, base);

                let is_self_call = callee_name
                    .as_ref()
                    .map(|n| n == &cell.name)
                    .unwrap_or(false);

                // Drop the callee name string in base.
                if var_types.get(&(base as u32)) == Some(&JitVarType::Str) {
                    let old = use_var(&mut builder, &regs, &var_types, base);
                    builder.ins().call(str_drop_ref, &[old]);
                    var_types.remove(&(base as u32));
                }

                // Drop any string-typed argument registers.
                for i in 0..num_args {
                    let arg_reg = (base + 1 + i as u16) as u32;
                    if var_types.get(&arg_reg) == Some(&JitVarType::Str) {
                        if (arg_reg as usize) < regs.num_regs {
                            let v = use_var(&mut builder, &regs, &var_types, arg_reg as u16);
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
                            let arg_reg = base + 1 + i as u16;
                            new_args.push(use_var(&mut builder, &regs, &var_types, arg_reg));
                        }
                        for (i, &val) in new_args.iter().enumerate() {
                            if i < regs.num_regs {
                                def_var(&mut builder, &mut regs, i as u16, val);
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
                                let arg_reg = base + 1 + i as u16;
                                args.push(use_var(&mut builder, &regs, &var_types, arg_reg));
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

            // Record field access
            OpCode::GetField => {
                let record_ptr = use_var(&mut builder, &regs, &var_types, inst.b);
                let field_name = if (inst.c as usize) < string_table.len() {
                    &string_table[inst.c as usize]
                } else {
                    ""
                };
                let field_name_bytes = field_name.as_bytes();
                let field_name_ptr = builder
                    .ins()
                    .iconst(types::I64, field_name_bytes.as_ptr() as i64);
                let field_name_len = builder
                    .ins()
                    .iconst(types::I64, field_name_bytes.len() as i64);
                let call = builder.ins().call(
                    record_get_field_ref,
                    &[record_ptr, field_name_ptr, field_name_len],
                );
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Int);
                def_var(&mut builder, &mut regs, inst.a, result);
            }
            OpCode::SetField => {
                let record_ptr = use_var(&mut builder, &regs, &var_types, inst.a);
                let value_ptr = use_var(&mut builder, &regs, &var_types, inst.c);
                let field_name = if (inst.b as usize) < string_table.len() {
                    &string_table[inst.b as usize]
                } else {
                    ""
                };
                let field_name_bytes = field_name.as_bytes();
                let field_name_ptr = builder
                    .ins()
                    .iconst(types::I64, field_name_bytes.as_ptr() as i64);
                let field_name_len = builder
                    .ins()
                    .iconst(types::I64, field_name_bytes.len() as i64);
                let call = builder.ins().call(
                    record_set_field_ref,
                    &[record_ptr, field_name_ptr, field_name_len, value_ptr],
                );
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Int);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // GetIndex: Get element from list/map by index/key
            // r[a] = r[b][r[c]]
            OpCode::GetIndex => {
                let collection_ptr = use_var(&mut builder, &regs, &var_types, inst.b);
                let index_ptr = use_var(&mut builder, &regs, &var_types, inst.c);
                let call = builder
                    .ins()
                    .call(get_index_ref, &[collection_ptr, index_ptr]);
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Int);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // SetIndex: Set element in list/map by index/key
            // r[a][r[b]] = r[c]
            OpCode::SetIndex => {
                let collection_ptr = use_var(&mut builder, &regs, &var_types, inst.a);
                let index_ptr = use_var(&mut builder, &regs, &var_types, inst.b);
                let value_ptr = use_var(&mut builder, &regs, &var_types, inst.c);
                let call = builder
                    .ins()
                    .call(set_index_ref, &[collection_ptr, index_ptr, value_ptr]);
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Int);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // NewList: Create a list from register values
            // r[a] = List([r[a+1], ..., r[a+b]])
            OpCode::NewList => {
                let count = inst.b as usize;

                // Allocate stack space for the array of value pointers
                let slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    (count * 8) as u32,
                    8,
                ));
                let array_ptr = builder.ins().stack_addr(types::I64, slot, 0);

                // Fill the array with pointers from registers r[a+1] through r[a+b]
                for i in 0..count {
                    let value = use_var(&mut builder, &regs, &var_types, inst.a + 1 + i as u16);
                    let offset = (i * 8) as i32;
                    builder
                        .ins()
                        .store(MemFlags::new(), value, array_ptr, offset);
                }

                // Call jit_rt_new_list(array_ptr, count)
                let count_val = builder.ins().iconst(types::I64, count as i64);
                let call = builder.ins().call(new_list_ref, &[array_ptr, count_val]);
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Int);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // NewMap: Create a map from key-value pairs
            // r[a] = Map({r[a+1]: r[a+2], r[a+3]: r[a+4], ...})
            OpCode::NewMap => {
                let count = inst.b as usize; // number of key-value pairs

                // Allocate stack space for the array of key-value pointers (count * 2 * 8 bytes)
                let slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    (count * 2 * 8) as u32,
                    8,
                ));
                let array_ptr = builder.ins().stack_addr(types::I64, slot, 0);

                // Fill the array with alternating key and value pointers
                for i in 0..count {
                    let key = use_var(&mut builder, &regs, &var_types, inst.a + 1 + (i * 2) as u16);
                    let value =
                        use_var(&mut builder, &regs, &var_types, inst.a + 2 + (i * 2) as u16);

                    let key_offset = (i * 2 * 8) as i32;
                    let value_offset = ((i * 2 + 1) * 8) as i32;

                    builder
                        .ins()
                        .store(MemFlags::new(), key, array_ptr, key_offset);
                    builder
                        .ins()
                        .store(MemFlags::new(), value, array_ptr, value_offset);
                }

                // Call jit_rt_new_map(array_ptr, count)
                let count_val = builder.ins().iconst(types::I64, count as i64);
                let call = builder.ins().call(new_map_ref, &[array_ptr, count_val]);
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Int);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // NewUnion: Create a union (enum) value
            // r[a] = Union(tag=strings[b], payload=r[c])
            OpCode::NewUnion => {
                let tag_str = if (inst.b as usize) < string_table.len() {
                    &string_table[inst.b as usize]
                } else {
                    ""
                };
                let tag_bytes = tag_str.as_bytes();
                let tag_ptr = builder.ins().iconst(types::I64, tag_bytes.as_ptr() as i64);
                let tag_len = builder.ins().iconst(types::I64, tag_bytes.len() as i64);
                let payload_ptr = use_var(&mut builder, &regs, &var_types, inst.c);
                let call = builder
                    .ins()
                    .call(union_new_ref, &[tag_ptr, tag_len, payload_ptr]);
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Int);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // IsVariant: Check if union has a specific variant tag
            // Skip next instruction if r[a] is Union with tag=strings[bx]
            OpCode::IsVariant => {
                let union_ptr = use_var(&mut builder, &regs, &var_types, inst.a);
                let tag_idx = inst.bx() as usize;
                let tag_str = if tag_idx < string_table.len() {
                    &string_table[tag_idx]
                } else {
                    ""
                };
                let tag_bytes = tag_str.as_bytes();
                let tag_ptr = builder.ins().iconst(types::I64, tag_bytes.as_ptr() as i64);
                let tag_len = builder.ins().iconst(types::I64, tag_bytes.len() as i64);
                let call = builder
                    .ins()
                    .call(union_is_variant_ref, &[union_ptr, tag_ptr, tag_len]);
                let result = builder.inst_results(call)[0];
                // IsVariant doesn't store result in a register, it's a control flow instruction
                // For now, we'll skip implementing the conditional jump logic in JIT
                // This requires more complex control flow handling
                // TODO: Implement proper conditional branching for IsVariant
                let _ = result; // silence unused warning
            }

            // Unbox: Extract payload from union
            // r[a] = payload of Union in r[b]
            OpCode::Unbox => {
                let union_ptr = use_var(&mut builder, &regs, &var_types, inst.b);
                let call = builder.ins().call(union_unbox_ref, &[union_ptr]);
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Int);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // Intrinsic (builtin function call)
            OpCode::Intrinsic => {
                let intrinsic_id = inst.b as u32;
                let arg_base = inst.c;

                match intrinsic_id {
                    // -------------------------------------------------------
                    // 0 / 1 / 72: Length / Count / Size
                    // -------------------------------------------------------
                    0 | 1 | 72 => {
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Str) {
                            let str_ptr = use_var(&mut builder, &regs, &var_types, arg_base);
                            let call = builder.ins().call(intrinsic_string_len_ref, &[str_ptr]);
                            let raw_len = builder.inst_results(call)[0];
                            // string_len returns raw i64 — NaN-box it
                            let result = emit_box_int(&mut builder, raw_len);
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        } else {
                            let zero = builder.ins().iconst(types::I64, nan_box_int(0));
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &mut regs, inst.a, zero);
                        }
                    }

                    // -------------------------------------------------------
                    // 9: Print
                    // -------------------------------------------------------
                    9 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        match arg_ty {
                            Some(JitVarType::Str) => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                builder.ins().call(intrinsic_print_str_ref, &[v]);
                            }
                            Some(JitVarType::Float) => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                // Unbox float: bitcast I64 → F64 for the C function
                                let fv = emit_unbox_float(&mut builder, v);
                                builder.ins().call(intrinsic_print_float_ref, &[fv]);
                            }
                            _ => {
                                // Int or unknown — unbox int before printing
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let unboxed = emit_unbox_int(&mut builder, v);
                                builder.ins().call(intrinsic_print_int_ref, &[unboxed]);
                            }
                        }
                        // print returns null
                        let null_val = builder.ins().iconst(types::I64, NAN_BOX_NULL);
                        var_types.insert(inst.a as u32, JitVarType::Int);
                        def_var(&mut builder, &mut regs, inst.a, null_val);
                    }

                    // -------------------------------------------------------
                    // 10: ToString
                    // -------------------------------------------------------
                    10 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        match arg_ty {
                            Some(JitVarType::Str) => {
                                // Already a string — inline refcount increment
                                let src = use_var(&mut builder, &regs, &var_types, arg_base);
                                let flags = MemFlags::new();
                                let rc = builder.ins().load(types::I64, flags, src, 0);
                                let rc_plus_one = builder.ins().iadd_imm(rc, 1);
                                builder.ins().store(flags, rc_plus_one, src, 0);
                                var_types.insert(inst.a as u32, JitVarType::Str);
                                def_var(&mut builder, &mut regs, inst.a, src);
                            }
                            Some(JitVarType::Float) => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let fv = emit_unbox_float(&mut builder, v);
                                let call = builder.ins().call(intrinsic_to_string_float_ref, &[fv]);
                                let result = builder.inst_results(call)[0];
                                var_types.insert(inst.a as u32, JitVarType::Str);
                                def_var(&mut builder, &mut regs, inst.a, result);
                            }
                            _ => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let unboxed = emit_unbox_int(&mut builder, v);
                                let call =
                                    builder.ins().call(intrinsic_to_string_int_ref, &[unboxed]);
                                let result = builder.inst_results(call)[0];
                                var_types.insert(inst.a as u32, JitVarType::Str);
                                def_var(&mut builder, &mut regs, inst.a, result);
                            }
                        }
                    }

                    // -------------------------------------------------------
                    // 11 / 121: ToInt / ParseInt
                    // -------------------------------------------------------
                    11 | 121 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        match arg_ty {
                            Some(JitVarType::Float) => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let fv = emit_unbox_float(&mut builder, v);
                                let call =
                                    builder.ins().call(intrinsic_to_int_from_float_ref, &[fv]);
                                let raw_result = builder.inst_results(call)[0];
                                // to_int_from_float returns raw i64, NaN-box it
                                let result = emit_box_int(&mut builder, raw_result);
                                var_types.insert(inst.a as u32, JitVarType::Int);
                                def_var(&mut builder, &mut regs, inst.a, result);
                            }
                            Some(JitVarType::Str) => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let call =
                                    builder.ins().call(intrinsic_to_int_from_string_ref, &[v]);
                                let raw_result = builder.inst_results(call)[0];
                                // to_int_from_string returns raw i64, NaN-box it
                                let result = emit_box_int(&mut builder, raw_result);
                                var_types.insert(inst.a as u32, JitVarType::Int);
                                def_var(&mut builder, &mut regs, inst.a, result);
                            }
                            _ => {
                                // Already Int — already NaN-boxed, pass through
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                var_types.insert(inst.a as u32, JitVarType::Int);
                                def_var(&mut builder, &mut regs, inst.a, v);
                            }
                        }
                    }

                    // -------------------------------------------------------
                    // 12 / 122: ToFloat / ParseFloat
                    // -------------------------------------------------------
                    12 | 122 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        match arg_ty {
                            Some(JitVarType::Int) => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let unboxed = emit_unbox_int(&mut builder, v);
                                let call = builder
                                    .ins()
                                    .call(intrinsic_to_float_from_int_ref, &[unboxed]);
                                let f64_result = builder.inst_results(call)[0];
                                let result = emit_box_float(&mut builder, f64_result);
                                var_types.insert(inst.a as u32, JitVarType::Float);
                                def_var(&mut builder, &mut regs, inst.a, result);
                            }
                            Some(JitVarType::Str) => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let call =
                                    builder.ins().call(intrinsic_to_float_from_string_ref, &[v]);
                                let f64_result = builder.inst_results(call)[0];
                                let result = emit_box_float(&mut builder, f64_result);
                                var_types.insert(inst.a as u32, JitVarType::Float);
                                def_var(&mut builder, &mut regs, inst.a, result);
                            }
                            _ => {
                                // Already Float — already NaN-boxed, pass through
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                var_types.insert(inst.a as u32, JitVarType::Float);
                                def_var(&mut builder, &mut regs, inst.a, v);
                            }
                        }
                    }

                    // -------------------------------------------------------
                    // 13: TypeOf — returns a string representation of the type
                    // -------------------------------------------------------
                    13 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        let type_str: &[u8] = match arg_ty {
                            Some(JitVarType::Int) => b"Int",
                            Some(JitVarType::Float) => b"Float",
                            Some(JitVarType::Bool) => b"Bool",
                            Some(JitVarType::Str) => b"String",
                            None => b"Unknown",
                        };
                        // Inline JitString allocation for the type name.
                        let len = type_str.len() as i64;
                        let char_count = type_str.len() as i64; // ASCII type names
                        let flags = MemFlags::new();

                        let struct_size = builder.ins().iconst(types::I64, 40);
                        let struct_call = builder.ins().call(malloc_ref, &[struct_size]);
                        let struct_ptr = builder.inst_results(struct_call)[0];

                        let rc_one = builder.ins().iconst(types::I64, 1);
                        builder.ins().store(flags, rc_one, struct_ptr, 0); // refcount
                        let len_val = builder.ins().iconst(types::I64, len);
                        builder.ins().store(flags, len_val, struct_ptr, 8); // len
                        let char_count_val = builder.ins().iconst(types::I64, char_count);
                        builder.ins().store(flags, char_count_val, struct_ptr, 16); // char_count
                        builder.ins().store(flags, len_val, struct_ptr, 24); // cap = len

                        // Type names are always non-empty (3-7 bytes).
                        // Uses alloc_bytes_ref (Vec-compatible) so that
                        // JitString::drop_ref can free via Vec::from_raw_parts.
                        let data_call = builder.ins().call(alloc_bytes_ref, &[len_val]);
                        let data_ptr = builder.inst_results(data_call)[0];
                        let src_ptr = builder.ins().iconst(types::I64, type_str.as_ptr() as i64);
                        builder
                            .ins()
                            .call(memcpy_ref, &[data_ptr, src_ptr, len_val]);
                        builder.ins().store(flags, data_ptr, struct_ptr, 32); // ptr

                        var_types.insert(inst.a as u32, JitVarType::Str);
                        def_var(&mut builder, &mut regs, inst.a, struct_ptr);
                    }

                    // -------------------------------------------------------
                    // 26: Abs — pure Cranelift IR (no runtime helper)
                    // -------------------------------------------------------
                    26 => {
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            let v = use_var(&mut builder, &regs, &var_types, arg_base);
                            let fv = emit_unbox_float(&mut builder, v);
                            let abs_fv = builder.ins().fabs(fv);
                            let result = emit_box_float(&mut builder, abs_fv);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        } else {
                            // Integer abs: unbox → neg → cmp → select → rebox
                            let v = use_var(&mut builder, &regs, &var_types, arg_base);
                            let raw = emit_unbox_int(&mut builder, v);
                            let neg = builder.ins().ineg(raw);
                            let zero = builder.ins().iconst(types::I64, 0);
                            let is_neg = builder.ins().icmp(IntCC::SignedLessThan, raw, zero);
                            let abs_raw = builder.ins().select(is_neg, neg, raw);
                            let result = emit_box_int(&mut builder, abs_raw);
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        }
                    }

                    // -------------------------------------------------------
                    // 27: Min
                    // -------------------------------------------------------
                    27 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        if arg_ty == Some(JitVarType::Float) {
                            let a = use_var(&mut builder, &regs, &var_types, arg_base);
                            let b = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                            let fa = emit_unbox_float(&mut builder, a);
                            let fb = emit_unbox_float(&mut builder, b);
                            let min_f = builder.ins().fmin(fa, fb);
                            let result = emit_box_float(&mut builder, min_f);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        } else {
                            let a = use_var(&mut builder, &regs, &var_types, arg_base);
                            let b = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                            let ra = emit_unbox_int(&mut builder, a);
                            let rb = emit_unbox_int(&mut builder, b);
                            let min_i = builder.ins().smin(ra, rb);
                            let result = emit_box_int(&mut builder, min_i);
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        }
                    }

                    // -------------------------------------------------------
                    // 28: Max
                    // -------------------------------------------------------
                    28 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        if arg_ty == Some(JitVarType::Float) {
                            let a = use_var(&mut builder, &regs, &var_types, arg_base);
                            let b = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                            let fa = emit_unbox_float(&mut builder, a);
                            let fb = emit_unbox_float(&mut builder, b);
                            let max_f = builder.ins().fmax(fa, fb);
                            let result = emit_box_float(&mut builder, max_f);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        } else {
                            let a = use_var(&mut builder, &regs, &var_types, arg_base);
                            let b = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                            let ra = emit_unbox_int(&mut builder, a);
                            let rb = emit_unbox_int(&mut builder, b);
                            let max_i = builder.ins().smax(ra, rb);
                            let result = emit_box_int(&mut builder, max_i);
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        }
                    }

                    // -------------------------------------------------------
                    // 50: IsEmpty — string only for now
                    // -------------------------------------------------------
                    50 => {
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Str) {
                            let str_ptr = use_var(&mut builder, &regs, &var_types, arg_base);
                            let call = builder.ins().call(intrinsic_string_len_ref, &[str_ptr]);
                            let len = builder.inst_results(call)[0];
                            let zero = builder.ins().iconst(types::I64, 0);
                            let is_empty = builder.ins().icmp(IntCC::Equal, len, zero);
                            let true_val = builder.ins().iconst(types::I64, NAN_BOX_TRUE);
                            let false_val = builder.ins().iconst(types::I64, NAN_BOX_FALSE);
                            let result = builder.ins().select(is_empty, true_val, false_val);
                            var_types.insert(inst.a as u32, JitVarType::Bool);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        } else {
                            // Non-string: stub returns false
                            let result = builder.ins().iconst(types::I64, NAN_BOX_FALSE);
                            var_types.insert(inst.a as u32, JitVarType::Bool);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        }
                    }

                    // -------------------------------------------------------
                    // 57: Round (nearest even) — pure Cranelift IR
                    // -------------------------------------------------------
                    57 => {
                        let v = use_var(&mut builder, &regs, &var_types, arg_base);
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            let fv = emit_unbox_float(&mut builder, v);
                            let rounded = builder.ins().nearest(fv);
                            let result = emit_box_float(&mut builder, rounded);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        } else {
                            // Int round is a no-op
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &mut regs, inst.a, v);
                        }
                    }

                    // -------------------------------------------------------
                    // 58: Ceil — pure Cranelift IR
                    // -------------------------------------------------------
                    58 => {
                        let v = use_var(&mut builder, &regs, &var_types, arg_base);
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            let fv = emit_unbox_float(&mut builder, v);
                            let ceiled = builder.ins().ceil(fv);
                            let result = emit_box_float(&mut builder, ceiled);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        } else {
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &mut regs, inst.a, v);
                        }
                    }

                    // -------------------------------------------------------
                    // 59: Floor — pure Cranelift IR
                    // -------------------------------------------------------
                    59 => {
                        let v = use_var(&mut builder, &regs, &var_types, arg_base);
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            let fv = emit_unbox_float(&mut builder, v);
                            let floored = builder.ins().floor(fv);
                            let result = emit_box_float(&mut builder, floored);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        } else {
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &mut regs, inst.a, v);
                        }
                    }

                    // -------------------------------------------------------
                    // 60: Sqrt — pure Cranelift IR
                    // -------------------------------------------------------
                    60 => {
                        let v = use_var(&mut builder, &regs, &var_types, arg_base);
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            let fv = emit_unbox_float(&mut builder, v);
                            let sqrtv = builder.ins().sqrt(fv);
                            let result = emit_box_float(&mut builder, sqrtv);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        } else {
                            // Convert NaN-boxed int to float, then sqrt, rebox as float
                            let raw_int = emit_unbox_int(&mut builder, v);
                            let fv = builder.ins().fcvt_from_sint(types::F64, raw_int);
                            let sqrtv = builder.ins().sqrt(fv);
                            let result = emit_box_float(&mut builder, sqrtv);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        }
                    }

                    // -------------------------------------------------------
                    // 61: Pow — runtime helper (no Cranelift native)
                    // NaN-boxing: unbox operands before calling helpers,
                    // rebox result after.
                    // -------------------------------------------------------
                    61 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        if arg_ty == Some(JitVarType::Float) {
                            let base_raw = use_var(&mut builder, &regs, &var_types, arg_base);
                            let exp_raw = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                            // Unbox NaN-boxed floats: bitcast I64 -> F64
                            let base_f = emit_unbox_float(&mut builder, base_raw);
                            let exp_f = emit_unbox_float(&mut builder, exp_raw);
                            let call = builder
                                .ins()
                                .call(intrinsic_pow_float_ref, &[base_f, exp_f]);
                            let result_f = builder.inst_results(call)[0];
                            // Rebox: bitcast F64 -> I64
                            let result = emit_box_float(&mut builder, result_f);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        } else {
                            let base_raw = use_var(&mut builder, &regs, &var_types, arg_base);
                            let exp_raw = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                            // Unbox NaN-boxed ints: arithmetic right shift by 1
                            let base_i = emit_unbox_int(&mut builder, base_raw);
                            let exp_i = emit_unbox_int(&mut builder, exp_raw);
                            let call = builder.ins().call(intrinsic_pow_int_ref, &[base_i, exp_i]);
                            let result_i = builder.inst_results(call)[0];
                            // Rebox: (val << 1) | 1
                            let result = emit_box_int(&mut builder, result_i);
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        }
                    }

                    // -------------------------------------------------------
                    // 62: Log (natural) — runtime helper
                    // -------------------------------------------------------
                    62 => {
                        let v = use_var(&mut builder, &regs, &var_types, arg_base);
                        let fv = if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            emit_unbox_float(&mut builder, v)
                        } else {
                            let raw = emit_unbox_int(&mut builder, v);
                            builder.ins().fcvt_from_sint(types::F64, raw)
                        };
                        let call = builder.ins().call(intrinsic_log_ref, &[fv]);
                        let f64_result = builder.inst_results(call)[0];
                        let result = emit_box_float(&mut builder, f64_result);
                        var_types.insert(inst.a as u32, JitVarType::Float);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 63: Sin — runtime helper
                    // -------------------------------------------------------
                    63 => {
                        let v = use_var(&mut builder, &regs, &var_types, arg_base);
                        let fv = if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            emit_unbox_float(&mut builder, v)
                        } else {
                            let raw = emit_unbox_int(&mut builder, v);
                            builder.ins().fcvt_from_sint(types::F64, raw)
                        };
                        let call = builder.ins().call(intrinsic_sin_ref, &[fv]);
                        let f64_result = builder.inst_results(call)[0];
                        let result = emit_box_float(&mut builder, f64_result);
                        var_types.insert(inst.a as u32, JitVarType::Float);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 64: Cos — runtime helper
                    // -------------------------------------------------------
                    64 => {
                        let v = use_var(&mut builder, &regs, &var_types, arg_base);
                        let fv = if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            emit_unbox_float(&mut builder, v)
                        } else {
                            let raw = emit_unbox_int(&mut builder, v);
                            builder.ins().fcvt_from_sint(types::F64, raw)
                        };
                        let call = builder.ins().call(intrinsic_cos_ref, &[fv]);
                        let f64_result = builder.inst_results(call)[0];
                        let result = emit_box_float(&mut builder, f64_result);
                        var_types.insert(inst.a as u32, JitVarType::Float);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 138: Tan — runtime helper
                    // -------------------------------------------------------
                    138 => {
                        let v = use_var(&mut builder, &regs, &var_types, arg_base);
                        let fv = if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            emit_unbox_float(&mut builder, v)
                        } else {
                            let raw = emit_unbox_int(&mut builder, v);
                            builder.ins().fcvt_from_sint(types::F64, raw)
                        };
                        let call = builder.ins().call(intrinsic_tan_ref, &[fv]);
                        let f64_result = builder.inst_results(call)[0];
                        let result = emit_box_float(&mut builder, f64_result);
                        var_types.insert(inst.a as u32, JitVarType::Float);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 139: Trunc — pure Cranelift IR
                    // -------------------------------------------------------
                    139 => {
                        let v = use_var(&mut builder, &regs, &var_types, arg_base);
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            let fv = emit_unbox_float(&mut builder, v);
                            let truncated = builder.ins().trunc(fv);
                            let result = emit_box_float(&mut builder, truncated);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        } else {
                            // Int input: already a whole number, pass through
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &mut regs, inst.a, v);
                        }
                    }

                    // -------------------------------------------------------
                    // 65: Clamp(val, lo, hi)
                    // -------------------------------------------------------
                    65 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        let val = use_var(&mut builder, &regs, &var_types, arg_base);
                        let lo = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let hi = use_var(&mut builder, &regs, &var_types, arg_base + 2);
                        if arg_ty == Some(JitVarType::Float) {
                            let fval = emit_unbox_float(&mut builder, val);
                            let flo = emit_unbox_float(&mut builder, lo);
                            let fhi = emit_unbox_float(&mut builder, hi);
                            let clamped_lo = builder.ins().fmax(fval, flo);
                            let clamped = builder.ins().fmin(clamped_lo, fhi);
                            let result = emit_box_float(&mut builder, clamped);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        } else {
                            let ival = emit_unbox_int(&mut builder, val);
                            let ilo = emit_unbox_int(&mut builder, lo);
                            let ihi = emit_unbox_int(&mut builder, hi);
                            let clamped_lo = builder.ins().smax(ival, ilo);
                            let clamped = builder.ins().smin(clamped_lo, ihi);
                            let result = emit_box_int(&mut builder, clamped);
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        }
                    }

                    // -------------------------------------------------------
                    // 68 / 96 / 97: Debug / Eprint / Eprintln — same as print for now
                    // -------------------------------------------------------
                    68 | 96 | 97 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        match arg_ty {
                            Some(JitVarType::Str) => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                builder.ins().call(intrinsic_print_str_ref, &[v]);
                            }
                            Some(JitVarType::Float) => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let fv = emit_unbox_float(&mut builder, v);
                                builder.ins().call(intrinsic_print_float_ref, &[fv]);
                            }
                            _ => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let unboxed = emit_unbox_int(&mut builder, v);
                                builder.ins().call(intrinsic_print_int_ref, &[unboxed]);
                            }
                        }
                        let null_val = builder.ins().iconst(types::I64, NAN_BOX_NULL);
                        var_types.insert(inst.a as u32, JitVarType::Int);
                        def_var(&mut builder, &mut regs, inst.a, null_val);
                    }

                    // -------------------------------------------------------
                    // 85: Exit — trap to terminate
                    // -------------------------------------------------------
                    85 => {
                        builder
                            .ins()
                            .trap(cranelift_codegen::ir::TrapCode::unwrap_user(3));
                        terminated = true;
                    }

                    // -------------------------------------------------------
                    // 123: Log2 — runtime helper
                    // -------------------------------------------------------
                    123 => {
                        let v = use_var(&mut builder, &regs, &var_types, arg_base);
                        let fv = if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            emit_unbox_float(&mut builder, v)
                        } else {
                            let raw = emit_unbox_int(&mut builder, v);
                            builder.ins().fcvt_from_sint(types::F64, raw)
                        };
                        let call = builder.ins().call(intrinsic_log2_ref, &[fv]);
                        let f64_result = builder.inst_results(call)[0];
                        let result = emit_box_float(&mut builder, f64_result);
                        var_types.insert(inst.a as u32, JitVarType::Float);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 124: Log10 — runtime helper
                    // -------------------------------------------------------
                    124 => {
                        let v = use_var(&mut builder, &regs, &var_types, arg_base);
                        let fv = if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            emit_unbox_float(&mut builder, v)
                        } else {
                            let raw = emit_unbox_int(&mut builder, v);
                            builder.ins().fcvt_from_sint(types::F64, raw)
                        };
                        let call = builder.ins().call(intrinsic_log10_ref, &[fv]);
                        let f64_result = builder.inst_results(call)[0];
                        let result = emit_box_float(&mut builder, f64_result);
                        var_types.insert(inst.a as u32, JitVarType::Float);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 125: IsNan — pure Cranelift IR
                    // fcmp Unordered(v, v) — NaN is unordered with itself
                    // -------------------------------------------------------
                    125 => {
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            let v = use_var(&mut builder, &regs, &var_types, arg_base);
                            let fv = emit_unbox_float(&mut builder, v);
                            let is_nan = builder.ins().fcmp(FloatCC::Unordered, fv, fv);
                            let true_val = builder.ins().iconst(types::I64, NAN_BOX_TRUE);
                            let false_val = builder.ins().iconst(types::I64, NAN_BOX_FALSE);
                            let result = builder.ins().select(is_nan, true_val, false_val);
                            var_types.insert(inst.a as u32, JitVarType::Bool);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        } else {
                            // Integers are never NaN
                            let result = builder.ins().iconst(types::I64, NAN_BOX_FALSE);
                            var_types.insert(inst.a as u32, JitVarType::Bool);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        }
                    }

                    // -------------------------------------------------------
                    // 126: IsInfinite — pure Cranelift IR
                    // fabs(v) == +inf
                    // -------------------------------------------------------
                    126 => {
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            let v = use_var(&mut builder, &regs, &var_types, arg_base);
                            let fv = emit_unbox_float(&mut builder, v);
                            let abs_v = builder.ins().fabs(fv);
                            let inf = builder.ins().f64const(f64::INFINITY);
                            let is_inf = builder.ins().fcmp(FloatCC::Equal, abs_v, inf);
                            let true_val = builder.ins().iconst(types::I64, NAN_BOX_TRUE);
                            let false_val = builder.ins().iconst(types::I64, NAN_BOX_FALSE);
                            let result = builder.ins().select(is_inf, true_val, false_val);
                            var_types.insert(inst.a as u32, JitVarType::Bool);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        } else {
                            // Integers are never infinite
                            let result = builder.ins().iconst(types::I64, NAN_BOX_FALSE);
                            var_types.insert(inst.a as u32, JitVarType::Bool);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        }
                    }

                    // -------------------------------------------------------
                    // 127: MathPi — f64const(π)
                    // -------------------------------------------------------
                    127 => {
                        let pi = builder.ins().f64const(std::f64::consts::PI);
                        let result = emit_box_float(&mut builder, pi);
                        var_types.insert(inst.a as u32, JitVarType::Float);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 128: MathE — f64const(e)
                    // -------------------------------------------------------
                    128 => {
                        let e = builder.ins().f64const(std::f64::consts::E);
                        let result = emit_box_float(&mut builder, e);
                        var_types.insert(inst.a as u32, JitVarType::Float);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 106: StringConcat — convert arg to string
                    // VM semantics: if arg is a List, join all elements;
                    // otherwise convert the single value to a string.
                    // The JIT doesn't support Lists yet, so we handle the
                    // scalar case (Str passthrough, Int/Float → to_string).
                    // -------------------------------------------------------
                    106 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        match arg_ty {
                            Some(JitVarType::Str) => {
                                // Already a string — clone it
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let flags = MemFlags::new();
                                let rc = builder.ins().load(types::I64, flags, v, 0);
                                let rc1 = builder.ins().iadd_imm(rc, 1);
                                builder.ins().store(flags, rc1, v, 0);
                                var_types.insert(inst.a as u32, JitVarType::Str);
                                def_var(&mut builder, &mut regs, inst.a, v);
                            }
                            Some(JitVarType::Float) => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let fv = emit_unbox_float(&mut builder, v);
                                let call = builder.ins().call(intrinsic_to_string_float_ref, &[fv]);
                                let result = builder.inst_results(call)[0];
                                var_types.insert(inst.a as u32, JitVarType::Str);
                                def_var(&mut builder, &mut regs, inst.a, result);
                            }
                            _ => {
                                // Int or unknown — unbox int, convert to string
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let unboxed = emit_unbox_int(&mut builder, v);
                                let call =
                                    builder.ins().call(intrinsic_to_string_int_ref, &[unboxed]);
                                let result = builder.inst_results(call)[0];
                                var_types.insert(inst.a as u32, JitVarType::Str);
                                def_var(&mut builder, &mut regs, inst.a, result);
                            }
                        }
                    }

                    // -------------------------------------------------------
                    // 16: Contains(str, substr) -> Bool
                    // -------------------------------------------------------
                    16 => {
                        let a = use_var(&mut builder, &regs, &var_types, arg_base);
                        let b = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder.ins().call(intrinsic_string_contains_ref, &[a, b]);
                        let raw = builder.inst_results(call)[0];
                        // raw is 0 or 1 — NaN-box as bool
                        let zero = builder.ins().iconst(types::I64, 0);
                        let is_true = builder.ins().icmp(IntCC::NotEqual, raw, zero);
                        let true_val = builder.ins().iconst(types::I64, NAN_BOX_TRUE);
                        let false_val = builder.ins().iconst(types::I64, NAN_BOX_FALSE);
                        let result = builder.ins().select(is_true, true_val, false_val);
                        var_types.insert(inst.a as u32, JitVarType::Bool);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 19: Trim(str) -> String
                    // -------------------------------------------------------
                    19 => {
                        let v = use_var(&mut builder, &regs, &var_types, arg_base);
                        let call = builder.ins().call(intrinsic_string_trim_ref, &[v]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Str);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 20: Upper(str) -> String
                    // -------------------------------------------------------
                    20 => {
                        let v = use_var(&mut builder, &regs, &var_types, arg_base);
                        let call = builder.ins().call(intrinsic_string_upper_ref, &[v]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Str);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 21: Lower(str) -> String
                    // -------------------------------------------------------
                    21 => {
                        let v = use_var(&mut builder, &regs, &var_types, arg_base);
                        let call = builder.ins().call(intrinsic_string_lower_ref, &[v]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Str);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 22: Replace(str, old, new) -> String
                    // -------------------------------------------------------
                    22 => {
                        let a = use_var(&mut builder, &regs, &var_types, arg_base);
                        let b = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let c = use_var(&mut builder, &regs, &var_types, arg_base + 2);
                        let call = builder.ins().call(intrinsic_string_replace_ref, &[a, b, c]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Str);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 23: Slice(str, start, end) -> String
                    // -------------------------------------------------------
                    23 => {
                        let a = use_var(&mut builder, &regs, &var_types, arg_base);
                        let b_raw = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let c_raw = use_var(&mut builder, &regs, &var_types, arg_base + 2);
                        // start and end are NaN-boxed ints — unbox before calling runtime
                        let b = emit_unbox_int(&mut builder, b_raw);
                        let c = emit_unbox_int(&mut builder, c_raw);
                        let call = builder.ins().call(intrinsic_string_slice_ref, &[a, b, c]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Str);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 52: StartsWith(str, prefix) -> Bool
                    // -------------------------------------------------------
                    52 => {
                        let a = use_var(&mut builder, &regs, &var_types, arg_base);
                        let b = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder
                            .ins()
                            .call(intrinsic_string_starts_with_ref, &[a, b]);
                        let raw = builder.inst_results(call)[0];
                        let zero = builder.ins().iconst(types::I64, 0);
                        let is_true = builder.ins().icmp(IntCC::NotEqual, raw, zero);
                        let true_val = builder.ins().iconst(types::I64, NAN_BOX_TRUE);
                        let false_val = builder.ins().iconst(types::I64, NAN_BOX_FALSE);
                        let result = builder.ins().select(is_true, true_val, false_val);
                        var_types.insert(inst.a as u32, JitVarType::Bool);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 53: EndsWith(str, suffix) -> Bool
                    // -------------------------------------------------------
                    53 => {
                        let a = use_var(&mut builder, &regs, &var_types, arg_base);
                        let b = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder.ins().call(intrinsic_string_ends_with_ref, &[a, b]);
                        let raw = builder.inst_results(call)[0];
                        let zero = builder.ins().iconst(types::I64, 0);
                        let is_true = builder.ins().icmp(IntCC::NotEqual, raw, zero);
                        let true_val = builder.ins().iconst(types::I64, NAN_BOX_TRUE);
                        let false_val = builder.ins().iconst(types::I64, NAN_BOX_FALSE);
                        let result = builder.ins().select(is_true, true_val, false_val);
                        var_types.insert(inst.a as u32, JitVarType::Bool);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 54: IndexOf(str, substr) -> Int
                    // -------------------------------------------------------
                    54 => {
                        let a = use_var(&mut builder, &regs, &var_types, arg_base);
                        let b = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder.ins().call(intrinsic_string_index_of_ref, &[a, b]);
                        let raw = builder.inst_results(call)[0];
                        let result = emit_box_int(&mut builder, raw);
                        var_types.insert(inst.a as u32, JitVarType::Int);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 55: PadLeft(str, width) -> String
                    // -------------------------------------------------------
                    55 => {
                        let a = use_var(&mut builder, &regs, &var_types, arg_base);
                        let b_raw = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        // width is a NaN-boxed int — unbox before calling runtime
                        let b = emit_unbox_int(&mut builder, b_raw);
                        let call = builder.ins().call(intrinsic_string_pad_left_ref, &[a, b]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Str);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 56: PadRight(str, width) -> String
                    // -------------------------------------------------------
                    56 => {
                        let a = use_var(&mut builder, &regs, &var_types, arg_base);
                        let b_raw = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        // width is a NaN-boxed int — unbox before calling runtime
                        let b = emit_unbox_int(&mut builder, b_raw);
                        let call = builder.ins().call(intrinsic_string_pad_right_ref, &[a, b]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Str);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 2: Matches — truthiness test
                    // -------------------------------------------------------
                    2 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        let result = match arg_ty {
                            Some(JitVarType::Bool) => {
                                // Already a NaN-boxed bool — pass through
                                use_var(&mut builder, &regs, &var_types, arg_base)
                            }
                            Some(JitVarType::Float) => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let fv = emit_unbox_float(&mut builder, v);
                                let zero = builder.ins().f64const(0.0);
                                let is_nonzero = builder.ins().fcmp(FloatCC::NotEqual, fv, zero);
                                let true_val = builder.ins().iconst(types::I64, NAN_BOX_TRUE);
                                let false_val = builder.ins().iconst(types::I64, NAN_BOX_FALSE);
                                builder.ins().select(is_nonzero, true_val, false_val)
                            }
                            Some(JitVarType::Str) => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let call = builder.ins().call(intrinsic_string_len_ref, &[v]);
                                let len = builder.inst_results(call)[0];
                                let zero = builder.ins().iconst(types::I64, 0);
                                let is_nonempty = builder.ins().icmp(IntCC::NotEqual, len, zero);
                                let true_val = builder.ins().iconst(types::I64, NAN_BOX_TRUE);
                                let false_val = builder.ins().iconst(types::I64, NAN_BOX_FALSE);
                                builder.ins().select(is_nonempty, true_val, false_val)
                            }
                            _ => {
                                // Int: nonzero → true
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let raw = emit_unbox_int(&mut builder, v);
                                let zero = builder.ins().iconst(types::I64, 0);
                                let is_nonzero = builder.ins().icmp(IntCC::NotEqual, raw, zero);
                                let true_val = builder.ins().iconst(types::I64, NAN_BOX_TRUE);
                                let false_val = builder.ins().iconst(types::I64, NAN_BOX_FALSE);
                                builder.ins().select(is_nonzero, true_val, false_val)
                            }
                        };
                        var_types.insert(inst.a as u32, JitVarType::Bool);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 3: Hash — hash value to sha256-prefixed hex string
                    // -------------------------------------------------------
                    3 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        // Convert non-strings to string first, then hash
                        let str_val = match arg_ty {
                            Some(JitVarType::Str) => {
                                use_var(&mut builder, &regs, &var_types, arg_base)
                            }
                            Some(JitVarType::Float) => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let fv = emit_unbox_float(&mut builder, v);
                                let call = builder.ins().call(intrinsic_to_string_float_ref, &[fv]);
                                builder.inst_results(call)[0]
                            }
                            _ => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let unboxed = emit_unbox_int(&mut builder, v);
                                let call =
                                    builder.ins().call(intrinsic_to_string_int_ref, &[unboxed]);
                                builder.inst_results(call)[0]
                            }
                        };
                        let call = builder.ins().call(intrinsic_string_hash_ref, &[str_val]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Str);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 18: Split(str, sep) -> String (stub: returns comma-joined)
                    // -------------------------------------------------------
                    18 => {
                        let a = use_var(&mut builder, &regs, &var_types, arg_base);
                        let b = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder.ins().call(intrinsic_string_split_ref, &[a, b]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Str);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 66: Clone — copy value (refcount increment for strings)
                    // -------------------------------------------------------
                    66 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        let v = use_var(&mut builder, &regs, &var_types, arg_base);
                        match arg_ty {
                            Some(JitVarType::Str) => {
                                // Increment refcount
                                let flags = MemFlags::new();
                                let rc = builder.ins().load(types::I64, flags, v, 0);
                                let rc1 = builder.ins().iadd_imm(rc, 1);
                                builder.ins().store(flags, rc1, v, 0);
                                var_types.insert(inst.a as u32, JitVarType::Str);
                                def_var(&mut builder, &mut regs, inst.a, v);
                            }
                            _ => {
                                // Scalar — just copy the NaN-boxed value
                                let ty = arg_ty.unwrap_or(JitVarType::Int);
                                var_types.insert(inst.a as u32, ty);
                                def_var(&mut builder, &mut regs, inst.a, v);
                            }
                        }
                    }

                    // -------------------------------------------------------
                    // 67: Sizeof — size in bytes
                    // -------------------------------------------------------
                    67 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        let result = if arg_ty == Some(JitVarType::Str) {
                            // Return the byte length of the string content
                            let v = use_var(&mut builder, &regs, &var_types, arg_base);
                            let call = builder.ins().call(intrinsic_string_len_ref, &[v]);
                            let raw_len = builder.inst_results(call)[0];
                            emit_box_int(&mut builder, raw_len)
                        } else {
                            // All scalars are 8 bytes (i64 NaN-boxed)
                            builder.ins().iconst(types::I64, nan_box_int(8))
                        };
                        var_types.insert(inst.a as u32, JitVarType::Int);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 77: Format — same as ToString for scalar types
                    // -------------------------------------------------------
                    77 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        match arg_ty {
                            Some(JitVarType::Str) => {
                                let src = use_var(&mut builder, &regs, &var_types, arg_base);
                                let flags = MemFlags::new();
                                let rc = builder.ins().load(types::I64, flags, src, 0);
                                let rc_plus_one = builder.ins().iadd_imm(rc, 1);
                                builder.ins().store(flags, rc_plus_one, src, 0);
                                var_types.insert(inst.a as u32, JitVarType::Str);
                                def_var(&mut builder, &mut regs, inst.a, src);
                            }
                            Some(JitVarType::Float) => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let fv = emit_unbox_float(&mut builder, v);
                                let call = builder.ins().call(intrinsic_to_string_float_ref, &[fv]);
                                let result = builder.inst_results(call)[0];
                                var_types.insert(inst.a as u32, JitVarType::Str);
                                def_var(&mut builder, &mut regs, inst.a, result);
                            }
                            _ => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let unboxed = emit_unbox_int(&mut builder, v);
                                let call =
                                    builder.ins().call(intrinsic_to_string_int_ref, &[unboxed]);
                                let result = builder.inst_results(call)[0];
                                var_types.insert(inst.a as u32, JitVarType::Str);
                                def_var(&mut builder, &mut regs, inst.a, result);
                            }
                        }
                    }

                    // -------------------------------------------------------
                    // 133: Hrtime — high-resolution timer (nanoseconds)
                    // -------------------------------------------------------
                    133 => {
                        let call = builder.ins().call(intrinsic_hrtime_ref, &[]);
                        let raw = builder.inst_results(call)[0];
                        let result = emit_box_int(&mut builder, raw);
                        var_types.insert(inst.a as u32, JitVarType::Int);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // Unsupported intrinsic — return 0 stub
                    // -------------------------------------------------------
                    _ => {
                        let zero = builder.ins().iconst(types::I64, nan_box_int(0));
                        var_types.insert(inst.a as u32, JitVarType::Int);
                        def_var(&mut builder, &mut regs, inst.a, zero);
                    }
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

    *ctx = Context::for_function(func);
    module
        .define_function(func_id, ctx)
        .map_err(|e| CodegenError::LoweringError(format!("define_function({}): {e}", cell.name)))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Declare an external helper function in both the module and the current
/// Cranelift function, returning a `FuncRef` that can be used with `builder.ins().call()`.
fn declare_helper_func<M: Module>(
    module: &mut M,
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

fn use_var(
    builder: &mut FunctionBuilder,
    regs: &HybridRegs,
    var_types: &HashMap<u32, JitVarType>,
    reg: u16,
) -> cranelift_codegen::ir::Value {
    let idx = reg as usize;
    if idx >= regs.num_regs {
        return builder.ins().iconst(types::I64, 0);
    }
    // Multi-block registers use Cranelift Variable.
    if let Some(&var) = regs.vars.get(&reg) {
        return builder.use_var(var);
    }
    // Single-block registers use pure SSA Value.
    if let Some(&val) = regs.ssa_vals.get(&reg) {
        return val;
    }
    // No value set yet — return NaN-boxed zero default.
    let vty = var_types
        .get(&(reg as u32))
        .copied()
        .unwrap_or(JitVarType::Int);
    match vty {
        JitVarType::Float => {
            // f64 0.0 has bits = 0i64
            builder.ins().iconst(types::I64, 0)
        }
        JitVarType::Int => {
            // NaN-boxed integer 0 = (0 << 1) | 1 = 1
            builder.ins().iconst(types::I64, nan_box_int(0))
        }
        JitVarType::Bool => {
            // NaN-boxed false
            builder.ins().iconst(types::I64, NAN_BOX_FALSE)
        }
        JitVarType::Str => {
            // Null pointer
            builder.ins().iconst(types::I64, 0)
        }
    }
}

fn def_var(
    builder: &mut FunctionBuilder,
    regs: &mut HybridRegs,
    reg: u16,
    val: cranelift_codegen::ir::Value,
) {
    let idx = reg as usize;
    if idx >= regs.num_regs {
        return;
    }
    // Multi-block registers use Cranelift Variable.
    if let Some(&var) = regs.vars.get(&reg) {
        builder.def_var(var, val);
    } else {
        // Single-block: store in SSA map.
        regs.ssa_vals.insert(reg, val);
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
        // NaN-box integers: (n << 1) | 1
        Constant::Int(n) => builder.ins().iconst(types::I64, nan_box_int(*n)),
        // NaN-box floats: raw f64 bits as i64
        Constant::Float(f) => builder.ins().iconst(types::I64, nan_box_float(*f)),
        // NaN-box booleans: quiet NaN payloads
        Constant::Bool(b) => {
            let sentinel = if *b { NAN_BOX_TRUE } else { NAN_BOX_FALSE };
            builder.ins().iconst(types::I64, sentinel)
        }
        // NaN-box null: canonical quiet NaN sentinel
        Constant::Null => builder.ins().iconst(types::I64, NAN_BOX_NULL),
        Constant::String(_) => builder.ins().iconst(types::I64, 0),
        Constant::BigInt(_) => builder.ins().iconst(types::I64, 0),
    };

    Ok(val)
}

fn collect_block_starts(instructions: &[Instruction]) -> BTreeSet<usize> {
    let mut targets = BTreeSet::new();

    for (pc, inst) in instructions.iter().enumerate() {
        match inst.op {
            OpCode::Jmp | OpCode::Break | OpCode::Continue => {
                let offset = inst.sax_val();
                let target = (pc as isize + 1 + offset as isize) as usize;
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
    base_reg: u16,
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

fn is_float_op(var_types: &HashMap<u32, JitVarType>, lhs_reg: u16, rhs_reg: u16) -> bool {
    var_types.get(&(lhs_reg as u32)).copied() == Some(JitVarType::Float)
        || var_types.get(&(rhs_reg as u32)).copied() == Some(JitVarType::Float)
}

/// Identify registers that hold string constants used ONLY as Call/TailCall
/// callee names in straight-line code. For these we skip heap string allocation.
fn identify_call_name_registers(cell: &LirCell) -> std::collections::HashSet<u16> {
    use std::collections::HashSet;

    let mut result: HashSet<u16> = HashSet::new();
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
        let mut aliases: HashSet<u16> = HashSet::new();
        aliases.insert(origin_reg);
        let mut found_call_use = false;
        let mut invalidated = false;

        for inst in &instructions[(loadk_pc + 1)..] {
            match inst.op {
                OpCode::Call | OpCode::TailCall => {
                    let base = inst.a;
                    let num_args = inst.b as usize;
                    for i in 0..num_args {
                        let arg_reg = base + 1 + i as u16;
                        if aliases.contains(&arg_reg) {
                            invalidated = true;
                            break;
                        }
                    }
                    if invalidated {
                        break;
                    }
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
                        aliases.remove(&dest);
                    }
                }
                OpCode::LoadK | OpCode::LoadBool | OpCode::LoadInt | OpCode::LoadNil => {
                    aliases.remove(&inst.a);
                }
                OpCode::Jmp | OpCode::Break | OpCode::Continue => {
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
                | OpCode::ForIn => {}
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
            let mut propagated: HashSet<u16> = HashSet::new();
            propagated.insert(origin_reg);
            for inst in &instructions[(loadk_pc + 1)..] {
                if (inst.op == OpCode::Move || inst.op == OpCode::MoveOwn)
                    && propagated.contains(&inst.b)
                {
                    propagated.insert(inst.a);
                }
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
                if matches!(inst.op, OpCode::Jmp | OpCode::Break | OpCode::Continue) {
                    break;
                }
            }
            result.extend(propagated);
        }
    }

    result
}

/// Result of concat chain analysis: identifies chains of string Add/Concat
/// instructions that can be replaced with a single `jit_rt_string_concat_multi`
/// call.
struct ConcatChains {
    /// Maps the PC of the final instruction in a chain to the ordered list of
    /// leaf register indices (the original string operands to concatenate).
    chains: HashMap<usize, Vec<u16>>,
    /// PCs of intermediate Add/Concat instructions that are consumed by a chain
    /// and should be skipped during lowering.
    skip_pcs: std::collections::HashSet<usize>,
}

/// Pre-scan the instruction stream to find chains of string Add/Concat
/// operations whose intermediate results are used only once (as the LHS of
/// the next concatenation in the chain). Chains of 3+ leaf strings are
/// candidates for batch concatenation via `jit_rt_string_concat_multi`.
///
/// A typical chain looks like:
/// ```text
///   pc0: r2 = r0 + r1     (string)
///   pc1: r3 = r2 + r4     (r2 single-use → chain continues)
///   pc2: r5 = r3 + r6     (r3 single-use → chain continues)
/// ```
/// This produces `chains[pc2] = [r0, r1, r4, r6]` and `skip_pcs = {pc0, pc1}`.
///
/// Only operates on straight-line code — jumps, branches, and control flow
/// boundaries conservatively break chains.
fn identify_concat_chains(
    cell: &LirCell,
    string_param_regs: &std::collections::HashSet<u16>,
) -> ConcatChains {
    use std::collections::{HashMap, HashSet};

    let instructions = &cell.instructions;

    // Step 1: Forward-propagate string types to identify which registers are
    // strings. We start from LoadK(String) and string parameters, then
    // propagate through Add/Concat/Move instructions.
    let mut is_string_reg: HashSet<u16> = HashSet::new();
    is_string_reg.extend(string_param_regs);

    // Also seed from LoadK string constants.
    for inst in instructions {
        if inst.op == OpCode::LoadK {
            let bx = inst.bx() as usize;
            if matches!(cell.constants.get(bx), Some(Constant::String(_))) {
                is_string_reg.insert(inst.a);
            }
        }
    }

    // Propagate: Add/Concat with a string operand produces a string.
    // Move/MoveOwn from a string register produces a string.
    // We iterate until stable (usually 1-2 passes for straight-line code).
    loop {
        let mut changed = false;
        for inst in instructions {
            match inst.op {
                OpCode::Add | OpCode::Concat => {
                    if (is_string_reg.contains(&inst.b) || is_string_reg.contains(&inst.c))
                        && is_string_reg.insert(inst.a)
                    {
                        changed = true;
                    }
                }
                OpCode::Move | OpCode::MoveOwn => {
                    if is_string_reg.contains(&inst.b) && is_string_reg.insert(inst.a) {
                        changed = true;
                    }
                }
                _ => {}
            }
        }
        if !changed {
            break;
        }
    }

    // Step 2: Count how many times each register is used as a source operand
    // (inst.b or inst.c) across ALL instructions. A register is "single-use"
    // if it appears as a source exactly once.
    let mut use_count: HashMap<u16, usize> = HashMap::new();
    for inst in instructions {
        match inst.op {
            // Two-source opcodes: both b and c are used.
            OpCode::Add
            | OpCode::Concat
            | OpCode::Sub
            | OpCode::Mul
            | OpCode::Div
            | OpCode::Mod
            | OpCode::Pow
            | OpCode::FloorDiv
            | OpCode::Eq
            | OpCode::Lt
            | OpCode::Le
            | OpCode::BitOr
            | OpCode::BitAnd
            | OpCode::BitXor
            | OpCode::Shl
            | OpCode::Shr => {
                *use_count.entry(inst.b).or_insert(0) += 1;
                *use_count.entry(inst.c).or_insert(0) += 1;
            }
            // One-source opcodes: only b is used.
            OpCode::Move
            | OpCode::MoveOwn
            | OpCode::Neg
            | OpCode::BitNot
            | OpCode::Test
            | OpCode::Not => {
                *use_count.entry(inst.b).or_insert(0) += 1;
            }
            // Return uses a.
            OpCode::Return => {
                *use_count.entry(inst.a).or_insert(0) += 1;
            }
            // Call uses base reg (a) plus arguments.
            OpCode::Call | OpCode::TailCall => {
                let base = inst.a;
                let num_args = inst.b as usize;
                *use_count.entry(base).or_insert(0) += 1;
                for i in 0..num_args {
                    *use_count.entry(base + 1 + i as u16).or_insert(0) += 1;
                }
            }
            // Intrinsic uses base reg arguments.
            OpCode::Intrinsic => {
                let base = inst.a;
                let num_args = inst.c as usize;
                for i in 0..num_args {
                    *use_count.entry(base + 1 + i as u16).or_insert(0) += 1;
                }
            }
            _ => {}
        }
    }

    // Step 3: Identify which PCs are string Add/Concat instructions and
    // build a map from destination register to the PC that produced it.
    let mut string_concat_pcs: HashSet<usize> = HashSet::new();
    let mut producer_pc: HashMap<u16, usize> = HashMap::new();
    for (pc, inst) in instructions.iter().enumerate() {
        if matches!(inst.op, OpCode::Add | OpCode::Concat)
            && (is_string_reg.contains(&inst.b) || is_string_reg.contains(&inst.c))
        {
            string_concat_pcs.insert(pc);
            producer_pc.insert(inst.a, pc);
        }
    }

    // Step 4: Build chains. For each string concat instruction, walk backward
    // through LHS operands to find chains of single-use intermediates.
    //
    // We process instructions in reverse order so that we find the longest
    // chain tail first and mark intermediates before shorter overlapping
    // chains can claim them.
    let mut intermediate_pcs: HashSet<usize> = HashSet::new();
    let mut chains: HashMap<usize, Vec<u16>> = HashMap::new();

    // Iterate in reverse so longer tails are found first.
    for pc in (0..instructions.len()).rev() {
        if !string_concat_pcs.contains(&pc) {
            continue;
        }
        // Skip if this pc is already consumed as an intermediate of a
        // longer chain found earlier (in reverse iteration).
        if intermediate_pcs.contains(&pc) {
            continue;
        }

        // Collect the chain by walking backward through LHS operands.
        let mut leaves: Vec<u16> = Vec::new();
        let mut current_pc = pc;
        let mut found_intermediates: Vec<usize> = Vec::new();

        loop {
            let current_inst = &instructions[current_pc];
            let lhs_reg = current_inst.b;
            let rhs_reg = current_inst.c;

            // RHS is always a leaf (rightmost operand at each step).
            leaves.push(rhs_reg);

            // Check if LHS is a single-use result of another string concat.
            if let Some(&lhs_producer_pc) = producer_pc.get(&lhs_reg) {
                let lhs_uses = use_count.get(&lhs_reg).copied().unwrap_or(0);
                if lhs_uses == 1
                    && string_concat_pcs.contains(&lhs_producer_pc)
                    && lhs_producer_pc < current_pc
                    && !intermediate_pcs.contains(&lhs_producer_pc)
                {
                    // LHS is a single-use intermediate — extend the chain.
                    // Only mark current_pc as an intermediate if it's not
                    // the chain tail (pc). The tail is where concat_multi
                    // is emitted, so it must NOT be skipped.
                    if current_pc != pc {
                        found_intermediates.push(current_pc);
                    }
                    current_pc = lhs_producer_pc;
                    continue;
                }
            }

            // LHS is a leaf (not a single-use intermediate).
            leaves.push(lhs_reg);
            break;
        }

        // Leaves were collected in reverse order (RHS first, walking backward).
        // Reverse to get left-to-right order.
        leaves.reverse();

        // Only optimize chains with 3+ leaf strings (2 leaves = regular concat).
        if leaves.len() >= 3 {
            // current_pc is the earliest instruction in the chain (the "head").
            // pc is the last instruction (the "tail") that produces the final result.
            // Mark the head and all intermediates as skipped.
            intermediate_pcs.insert(current_pc);
            for &ipc in &found_intermediates {
                intermediate_pcs.insert(ipc);
            }
            chains.insert(pc, leaves);
        }
    }

    ConcatChains {
        chains,
        skip_pcs: intermediate_pcs,
    }
}
