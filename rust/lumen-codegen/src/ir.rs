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
// All values are stored as 64-bit integers in registers.  The encoding
// matches NbValue in lumen-core exactly:
//
//   NAN_MASK = 0x7FF8_0000_0000_0000 (quiet-NaN bits: exponent all 1, quiet bit set)
//   TAG_SHIFT = 48
//   PAYLOAD_MASK = 0x0000_FFFF_FFFF_FFFF
//
//   Encoding: NAN_MASK | (tag << TAG_SHIFT) | (payload & PAYLOAD_MASK)
//
//   - **Floats**:    raw IEEE 754 f64 bits (NOT NaN-boxed; discriminated by
//                   checking if NAN_MASK bits are absent)
//   - **Integers**:  NAN_MASK | (TAG_INT=1 << 48) | (val & 0x0000_FFFF_FFFF_FFFF)
//                   → 0x7FF9_xxxx_xxxx_xxxx  (48-bit two's complement)
//   - **True**:      NAN_MASK | (TAG_BOOL=3 << 48) | 1  → 0x7FFB_0000_0000_0001
//   - **False**:     NAN_MASK | (TAG_BOOL=3 << 48) | 0  → 0x7FFB_0000_0000_0000
//   - **Null**:      NAN_MASK | (TAG_NULL=4 << 48)       → 0x7FFC_0000_0000_0000
//   - **Pointers**:  NAN_MASK | (TAG_PTR=0 << 48) | addr → 0x7FF8_xxxx_xxxx_xxxx
//                   (Strings, Records: raw pointer in lower 48 bits)
// ---------------------------------------------------------------------------

/// Quiet-NaN mask — bits 52-62 all set.  All NaN-boxed values have these bits set.
const NAN_MASK_U: u64 = 0x7FF8_0000_0000_0000;
/// 48-bit payload mask.
const PAYLOAD_MASK_U: u64 = 0x0000_FFFF_FFFF_FFFF;

/// NaN-boxed representation of `null`  (TAG_NULL=4).
pub const NAN_BOX_NULL: i64 = 0x7FFC_0000_0000_0000_u64 as i64;

/// NaN-boxed representation of `true`  (TAG_BOOL=3, payload=1).
pub const NAN_BOX_TRUE: i64 = 0x7FFB_0000_0000_0001_u64 as i64;

/// NaN-boxed representation of `false` (TAG_BOOL=3, payload=0).
pub const NAN_BOX_FALSE: i64 = 0x7FFB_0000_0000_0000_u64 as i64;

/// Base for NaN-boxed integers: NAN_MASK | (TAG_INT=1 << 48).
const NAN_INT_BASE: i64 = 0x7FF9_0000_0000_0000_u64 as i64;

/// Emit IR to NaN-box an integer using NbValue encoding:
/// `NAN_MASK | (TAG_INT << 48) | (val & PAYLOAD_MASK)`.
fn emit_box_int(
    builder: &mut FunctionBuilder,
    val: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let base = builder.ins().iconst(types::I64, NAN_INT_BASE);
    let masked = builder.ins().band_imm(val, PAYLOAD_MASK_U as i64);
    builder.ins().bor(base, masked)
}

/// Emit IR to unbox a NaN-boxed integer: extract bits 0-47 and sign-extend
/// from bit 47.
/// Only valid when the value is known to be a NaN-boxed int (tag == TAG_INT=1).
fn emit_unbox_int(
    builder: &mut FunctionBuilder,
    val: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    // Mask to lower 48 bits, then sign-extend from bit 47 via ishl+sshr.
    let masked = builder.ins().band_imm(val, PAYLOAD_MASK_U as i64);
    let shl = builder.ins().ishl_imm(masked, 16);
    builder.ins().sshr_imm(shl, 16)
}

/// Emit IR to NaN-box a boolean: converts i1 (0 or 1) to NAN_BOX_FALSE or NAN_BOX_TRUE.
fn emit_box_bool(
    builder: &mut FunctionBuilder,
    val: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    // val is i1 (0 or 1)
    // false (0) -> NAN_BOX_FALSE (0x7FFB_0000_0000_0000)
    // true (1) -> NAN_BOX_TRUE  (0x7FFB_0000_0000_0001)
    let false_const = builder.ins().iconst(types::I64, NAN_BOX_FALSE);
    let true_const = builder.ins().iconst(types::I64, NAN_BOX_TRUE);
    builder.ins().select(val, true_const, false_const)
}

/// Emit IR to NaN-box a boolean from an integer 0/1.
fn emit_box_bool_from_int(
    builder: &mut FunctionBuilder,
    val: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let zero = builder.ins().iconst(types::I64, 0);
    let is_true = builder.ins().icmp(IntCC::NotEqual, val, zero);
    emit_box_bool(builder, is_true)
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

/// Pure Rust: NaN-box an integer value using NbValue encoding.
/// Matches `NbValue::new_int(v)` exactly.
pub fn nan_box_int(v: i64) -> i64 {
    (NAN_MASK_U | (1u64 << 48) | ((v as u64) & PAYLOAD_MASK_U)) as i64
}

/// Pure Rust: unbox a NaN-boxed integer (extract 48-bit two's complement payload).
/// Matches `NbValue::as_int()` exactly.
pub fn nan_unbox_int(v: i64) -> i64 {
    let raw = (v as u64) & PAYLOAD_MASK_U;
    // Sign-extend from bit 47
    if raw & (1 << 47) != 0 {
        (raw | !PAYLOAD_MASK_U) as i64
    } else {
        raw as i64
    }
}

/// Emit IR to ensure a value is a raw (unboxed) integer.
/// If `ty` is already `RawInt`, returns the value unchanged.
/// If `ty` is `Int` (NaN-boxed), emits `emit_unbox_int`.
fn ensure_raw_int(
    builder: &mut FunctionBuilder,
    val: cranelift_codegen::ir::Value,
    ty: JitVarType,
) -> cranelift_codegen::ir::Value {
    match ty {
        JitVarType::RawInt => val,
        _ => emit_unbox_int(builder, val),
    }
}

/// Emit IR to ensure a value is a NaN-boxed integer.
/// If `ty` is already `Int` (boxed), returns the value unchanged.
/// If `ty` is `RawInt`, emits `emit_box_int`.
fn ensure_boxed_int(
    builder: &mut FunctionBuilder,
    val: cranelift_codegen::ir::Value,
    ty: JitVarType,
) -> cranelift_codegen::ir::Value {
    match ty {
        JitVarType::RawInt => emit_box_int(builder, val),
        _ => val,
    }
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
/// assertions.  Handles integers (NbValue TAG_INT), booleans
/// (`NAN_BOX_TRUE` → 1, `NAN_BOX_FALSE` → 0), null (`NAN_BOX_NULL` → 0),
/// and pass-through for anything else (float bits, pointers).
///
/// # Deprecated
/// Use `nan_unbox_typed()` in `jit.rs` instead, which uses compile-time return
/// type information for correct unboxing.
#[deprecated(note = "use nan_unbox_typed() with JitVarType for correct type-aware unboxing")]
pub fn nan_unbox_jit_result(raw: i64) -> i64 {
    match raw {
        NAN_BOX_TRUE => 1,
        NAN_BOX_FALSE => 0,
        NAN_BOX_NULL => 0,
        v => {
            let u = v as u64;
            if (u & NAN_MASK_U) == NAN_MASK_U {
                let tag = (u >> 48) & 0x7;
                if tag == 1 {
                    // TAG_INT: extract 48-bit payload and sign-extend
                    nan_unbox_int(v)
                } else {
                    v // other NaN-boxed type (bool, ptr, etc.)
                }
            } else {
                v // raw float bits or pointer
            }
        }
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
            | OpCode::Append
            | OpCode::NewUnion
            | OpCode::IsVariant
            | OpCode::Unbox
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
            // Append reads the list from r[a] and element from r[b], writes result to r[a]
            OpCode::Append => {
                read_blocks.entry(inst.a).or_default().insert(blk);
                read_blocks.entry(inst.b).or_default().insert(blk);
            }
            // NewUnion reads payload from r[c]
            OpCode::NewUnion => {
                read_blocks.entry(inst.c).or_default().insert(blk);
            }
            // IsVariant reads the union from r[a]
            OpCode::IsVariant => {
                read_blocks.entry(inst.a).or_default().insert(blk);
            }
            // Unbox reads the union from r[b]
            OpCode::Unbox => {
                read_blocks.entry(inst.b).or_default().insert(blk);
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
    /// 64-bit signed integer (NaN-boxed).
    Int,
    /// Raw (unboxed) 64-bit signed integer. Used internally within JIT-compiled
    /// functions to avoid repeated NaN-box/unbox cycles on arithmetic-heavy code.
    /// Must be reboxed via `emit_box_int` before returning or passing to external calls.
    RawInt,
    /// 64-bit IEEE 754 floating point.
    Float,
    /// Heap-allocated refcounted string, represented as a `*mut JitString` cast to i64.
    /// The pointer is created by inline `jit_rt_malloc` + field stores or `jit_rt_string_concat`
    /// and must be freed via `jit_rt_string_drop` when no longer needed.
    Str,
    /// Boolean value, NaN-boxed as sentinel values (NAN_BOX_TRUE / NAN_BOX_FALSE).
    Bool,
    /// Heap-allocated `*mut Value` pointer (for Union, Tuple, List, Record, etc.).
    /// The raw pointer is 8-byte aligned (low bit 0) and must be treated as an
    /// opaque handle — NOT a NaN-boxed integer.
    Ptr,
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
            "Int" => JitVarType::Int,
            "Float" => JitVarType::Float,
            "String" => JitVarType::Str,
            "Bool" => JitVarType::Bool,
            "Null" => JitVarType::Int, // NaN-boxed sentinel
            _ => JitVarType::Ptr,      // Union, Tuple, Record, List, etc. → heap pointer
        }
    }

    /// Returns true if this type is an integer (boxed or raw).
    fn is_int(self) -> bool {
        matches!(self, JitVarType::Int | JitVarType::RawInt)
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
    sig.params.push(AbiParam::new(pointer_type));
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
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let str_concat_mut_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_concat_mut",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let str_concat_multi_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_concat_multi",
        &[pointer_type, types::I64, types::I64], // ctx, ptr to array, count
        &[types::I64],
    )?;
    let malloc_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_malloc",
        &[pointer_type, types::I64], // ctx, size
        &[types::I64],
    )?;
    let alloc_bytes_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_alloc_bytes",
        &[pointer_type, types::I64], // ctx, size
        &[types::I64],
    )?;
    let str_eq_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_eq",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let str_cmp_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_cmp",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let str_drop_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_drop",
        &[pointer_type, types::I64],
        &[],
    )?;
    let memcpy_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_memcpy",
        &[pointer_type, types::I64, types::I64, types::I64], // ctx, dst, src, len
        &[],
    )?;
    // Trap helper: called by integer Div/FloorDiv/Mod when divisor is zero.
    let trap_divzero_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_trap_divzero",
        &[pointer_type], // ctx
        &[types::I64],   // returns 0 as sentinel
    )?;

    // Declare record runtime helper functions (for JIT record support).
    let new_record_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_new_record",
        &[pointer_type, types::I64, types::I64], // ctx, type_name_ptr, type_name_len
        &[types::I64],
    )?;
    let record_get_field_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_record_get_field",
        &[pointer_type, types::I64, types::I64, types::I64], // ctx, record_ptr, field_name_ptr, field_name_len
        &[types::I64],
    )?;
    let record_set_field_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_record_set_field",
        &[pointer_type, types::I64, types::I64, types::I64, types::I64], // ctx, record_ptr, field_name_ptr, field_name_len, value_ptr
        &[types::I64],
    )?;
    // Declare collection index runtime helper functions (for JIT list/map indexing).
    let get_index_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_get_index",
        &[pointer_type, types::I64, types::I64], // ctx, collection_ptr, index_ptr
        &[types::I64],
    )?;
    let set_index_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_set_index",
        &[pointer_type, types::I64, types::I64, types::I64], // ctx, collection_ptr, index_ptr, value_ptr
        &[types::I64],
    )?;
    // Declare collection runtime helper functions (for JIT List/Map construction).
    let new_list_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_new_list",
        &[pointer_type, types::I64, types::I64], // ctx, values_ptr, count
        &[types::I64],
    )?;
    let new_map_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_new_map",
        &[pointer_type, types::I64, types::I64], // ctx, kvpairs_ptr, count
        &[types::I64],
    )?;
    let new_tuple_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_new_tuple",
        &[pointer_type, types::I64, types::I64], // ctx, values_ptr, count
        &[types::I64],
    )?;
    let new_set_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_new_set",
        &[pointer_type, types::I64, types::I64], // ctx, values_ptr, count
        &[types::I64],
    )?;
    let collection_len_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_collection_len",
        &[pointer_type, types::I64], // ctx, value_ptr
        &[types::I64],
    )?;
    let list_append_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_list_append",
        &[pointer_type, types::I64, types::I64], // ctx, list_ptr, element
        &[types::I64],
    )?;
    let list_append_int_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_list_append_int",
        &[pointer_type, types::I64, types::I64], // ctx, list_ptr, element_raw_i64
        &[types::I64],
    )?;
    let list_append_float_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_list_append_float",
        &[pointer_type, types::I64, types::I64], // ctx, list_ptr, element_f64_bits
        &[types::I64],
    )?;
    let range_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_range",
        &[pointer_type, types::I64, types::I64], // ctx, start_nb, end_nb
        &[types::I64],
    )?;
    let sort_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_sort",
        &[pointer_type, types::I64], // ctx, list_nb
        &[types::I64],
    )?;
    let sort_desc_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_sort_desc",
        &[pointer_type, types::I64], // ctx, list_nb
        &[types::I64],
    )?;
    let list_reverse_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_list_reverse",
        &[pointer_type, types::I64], // ctx, list_ptr
        &[types::I64],
    )?;
    let list_flatten_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_list_flatten",
        &[pointer_type, types::I64], // ctx, list_ptr
        &[types::I64],
    )?;
    let list_unique_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_list_unique",
        &[pointer_type, types::I64], // ctx, list_ptr
        &[types::I64],
    )?;
    let list_take_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_list_take",
        &[pointer_type, types::I64, types::I64], // ctx, list_ptr, n
        &[types::I64],
    )?;
    let list_drop_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_list_drop",
        &[pointer_type, types::I64, types::I64], // ctx, list_ptr, n
        &[types::I64],
    )?;
    let list_first_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_list_first",
        &[pointer_type, types::I64], // ctx, list_ptr
        &[types::I64],
    )?;
    let list_last_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_list_last",
        &[pointer_type, types::I64], // ctx, list_ptr
        &[types::I64],
    )?;
    let _merge_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_merge",
        &[pointer_type, types::I64, types::I64], // ctx, a_ptr, b_ptr
        &[types::I64],
    )?;
    let merge_take_a_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_merge_take_a",
        &[pointer_type, types::I64, types::I64], // ctx, a_owned_nb, b_nb
        &[types::I64],
    )?;
    let new_map_strs_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_new_map_strs",
        &[pointer_type, types::I64, types::I64], // ctx, jit_str_pairs_ptr, count
        &[types::I64],
    )?;
    let map_keys_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_map_keys",
        &[pointer_type, types::I64],
        &[types::I64],
    )?;
    let map_values_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_map_values",
        &[pointer_type, types::I64],
        &[types::I64],
    )?;
    let map_entries_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_map_entries",
        &[pointer_type, types::I64],
        &[types::I64],
    )?;
    let map_has_key_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_map_has_key",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let map_remove_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_map_remove",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let map_sorted_keys_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_map_sorted_keys",
        &[pointer_type, types::I64],
        &[types::I64],
    )?;
    // Phase 1e: set/collection helpers
    let to_set_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_to_set",
        &[pointer_type, types::I64],
        &[types::I64],
    )?;
    let set_add_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_set_add",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let chars_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_chars",
        &[pointer_type, types::I64],
        &[types::I64],
    )?;
    let join_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_join",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let zip_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_zip",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let enumerate_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_enumerate",
        &[pointer_type, types::I64],
        &[types::I64],
    )?;
    let chunk_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_chunk",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let window_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_window",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    // Phase 1f: higher-order intrinsics helpers
    let hof_map_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_hof_map",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let hof_filter_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_hof_filter",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let hof_reduce_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_hof_reduce",
        &[pointer_type, types::I64, types::I64, types::I64],
        &[types::I64],
    )?;
    let hof_flat_map_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_hof_flat_map",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let hof_any_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_hof_any",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let hof_all_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_hof_all",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let hof_find_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_hof_find",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let hof_position_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_hof_position",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let hof_group_by_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_hof_group_by",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let hof_sort_by_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_hof_sort_by",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let union_match_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_union_match",
        &[pointer_type, types::I64, types::I64, types::I64], // ctx, union_ptr, tag_ptr, tag_len
        &[types::I64],
    )?;
    let str_to_nb_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_str_to_nb",
        &[pointer_type, types::I64], // ctx, jitstring_ptr
        &[types::I64],
    )?;
    // Declare union runtime helper functions (for JIT enum support).
    let union_new_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_union_new",
        &[pointer_type, types::I64, types::I64, types::I64], // ctx, tag_ptr, tag_len, payload_ptr
        &[types::I64],
    )?;
    let _union_is_variant_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_union_is_variant",
        &[pointer_type, types::I64, types::I64, types::I64], // ctx, union_ptr, tag_ptr, tag_len
        &[types::I64],
    )?;
    let union_unbox_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_union_unbox",
        &[pointer_type, types::I64],
        &[types::I64],
    )?;
    // Declare Is type-name helper: takes raw name bytes instead of JitString pointer.
    // Used by Is opcode when the type name is a compile-time constant.
    let is_type_name_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_is_type_name",
        &[pointer_type, types::I64, types::I64, types::I64], // ctx, value_ptr, name_ptr, name_len
        &[types::I64],
    )?;
    // Declare intrinsic runtime helper functions (for JIT builtin support).
    let intrinsic_print_int_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_print_int",
        &[pointer_type, types::I64],
        &[],
    )?;
    let intrinsic_print_float_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_print_float",
        &[pointer_type, types::F64],
        &[],
    )?;
    let intrinsic_print_str_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_print_str",
        &[pointer_type, types::I64],
        &[],
    )?;
    let intrinsic_string_len_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_len",
        &[pointer_type, types::I64],
        &[types::I64],
    )?;
    // Math transcendental helpers (Cranelift can't do these natively)
    let intrinsic_sin_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_sin",
        &[pointer_type, types::F64],
        &[types::F64],
    )?;
    let intrinsic_cos_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_cos",
        &[pointer_type, types::F64],
        &[types::F64],
    )?;
    let intrinsic_tan_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_tan",
        &[pointer_type, types::F64],
        &[types::F64],
    )?;
    let intrinsic_log_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_log",
        &[pointer_type, types::F64],
        &[types::F64],
    )?;
    let intrinsic_log2_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_log2",
        &[pointer_type, types::F64],
        &[types::F64],
    )?;
    let intrinsic_log10_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_log10",
        &[pointer_type, types::F64],
        &[types::F64],
    )?;
    let intrinsic_pow_float_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_pow_float",
        &[pointer_type, types::F64, types::F64],
        &[types::F64],
    )?;
    let intrinsic_pow_int_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_pow_int",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let intrinsic_fmod_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_fmod",
        &[pointer_type, types::F64, types::F64],
        &[types::F64],
    )?;
    // Conversion helpers
    let intrinsic_to_string_int_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_to_string_int",
        &[pointer_type, types::I64],
        &[types::I64],
    )?;
    let intrinsic_to_string_float_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_to_string_float",
        &[pointer_type, types::F64],
        &[types::I64],
    )?;
    let intrinsic_to_int_from_float_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_to_int_from_float",
        &[pointer_type, types::F64],
        &[types::I64],
    )?;
    let intrinsic_to_int_from_string_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_to_int_from_string",
        &[pointer_type, types::I64],
        &[types::I64],
    )?;
    let intrinsic_to_float_from_int_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_to_float_from_int",
        &[pointer_type, types::I64],
        &[types::F64],
    )?;
    let intrinsic_to_float_from_string_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_to_float_from_string",
        &[pointer_type, types::I64],
        &[types::F64],
    )?;

    // String operation helpers
    let intrinsic_string_upper_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_upper",
        &[pointer_type, types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_lower_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_lower",
        &[pointer_type, types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_trim_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_trim",
        &[pointer_type, types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_contains_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_contains",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_starts_with_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_starts_with",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_ends_with_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_ends_with",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_replace_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_replace",
        &[pointer_type, types::I64, types::I64, types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_index_of_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_index_of",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_slice_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_slice",
        &[pointer_type, types::I64, types::I64, types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_pad_left_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_pad_left",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_pad_right_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_pad_right",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let intrinsic_hrtime_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_hrtime",
        &[pointer_type],
        &[types::I64],
    )?;
    let intrinsic_string_hash_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_hash",
        &[pointer_type, types::I64],
        &[types::I64],
    )?;
    let intrinsic_string_split_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_split",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;
    let _intrinsic_string_join_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_string_join",
        &[pointer_type, types::I64, types::I64],
        &[types::I64],
    )?;

    // Declare async runtime helper functions (spawn/await).
    let async_spawn_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_spawn",
        &[pointer_type, types::I32, pointer_type, types::I32],
        &[types::I64],
    )?;
    let async_await_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_await",
        &[pointer_type, types::I64],
        &[types::I64],
    )?;

    // Declare tool system runtime helper functions (Phase 0.4).
    let tool_call_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_tool_call",
        &[pointer_type, types::I32, types::I64],
        &[types::I64],
    )?;
    let schema_validate_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_schema_validate",
        &[pointer_type, types::I64, types::I32],
        &[types::I64],
    )?;
    let emit_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_emit",
        &[pointer_type, types::I64],
        &[types::I64],
    )?;
    let trace_ref_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_trace_ref",
        &[pointer_type],
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
    // Map from register → declared param type string (for collection element inference).
    let mut param_type_map: HashMap<u16, &str> = HashMap::new();
    // Seed float_regs from parameters
    for (i, p) in cell.params.iter().enumerate() {
        param_type_map.insert(i as u16, p.ty.as_str());
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
            // GetIndex: if collection param is list[Float], result is Float
            OpCode::GetIndex => {
                let collection_reg = inst.b;
                if let Some(param_ty) = param_type_map.get(&collection_reg) {
                    if param_ty.contains("Float") || *param_ty == "list[Float]" {
                        float_regs.insert(inst.a);
                    }
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

    // Pre-scan: identify LoadK PCs whose strings are only used as `Is` type-name
    // operands. For these we skip JitString allocation (262K allocs saved for tree).
    // Also builds a map from Is-instruction PC → type name string for the Is handler.
    let (is_type_name_regs, is_pc_to_type_name) = identify_is_type_name_registers(cell);

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
            JitVarType::from_lir_return_type(&cell.params[i].ty)
        } else if float_regs.contains(&(i as u16)) {
            JitVarType::Float
        } else if string_regs.contains(&(i as u16)) {
            // Pre-classified string register: initialize as Str so the zero value
            // is a null pointer (0) rather than nan_box_int(0).  Without this,
            // multi-block string registers get nan_box_int(0) = 0x7FF9_0000_0000_0000
            // as their initial Cranelift Variable value, which passes
            // JitString::drop_ref's alignment guard (addr >= 4096 and addr & 7 == 0)
            // and causes a SIGSEGV when generated code tries to drop the "old"
            // register value in a block whose predecessors haven't assigned to it yet.
            JitVarType::Str
        } else {
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
    let vm_ctx_param = builder.block_params(entry_block)[0];
    builder.switch_to_block(entry_block);

    // Initialize function parameters from block params.
    // Callers (execute_jit_*) NaN-box Int parameters before calling, so
    // the values arrive already in NaN-boxed form. We just store them
    // directly into their registers.
    for (i, _param) in cell.params.iter().enumerate() {
        let val = builder.block_params(entry_block)[i + 1];
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
                    // NaN-boxed integer 0 = NAN_MASK | (TAG_INT << 48) | 0 = 0x7FF9_0000_0000_0000
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
                JitVarType::Ptr => {
                    // Null heap pointer sentinel
                    builder.ins().iconst(types::I64, NAN_BOX_NULL)
                }
                JitVarType::RawInt => {
                    // Raw i64 zero
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
    // Direct SSA value for conditions that should NOT be stored in registers.
    // Used by IsVariant which must NOT overwrite r[a] (the union register).
    let mut pending_test_value: Option<cranelift_codegen::ir::Value> = None;

    // Cranelift Variable for the cached IsVariant+Unbox payload.
    // IsVariant stores the combined union_match result here via def_var;
    // Unbox reads it via use_var — works across basic block boundaries.
    let union_match_cache_var = Variable::from_u32(num_regs as u32);
    builder.declare_var(union_match_cache_var, types::I64);
    let init_zero = builder.ins().iconst(types::I64, -1i64); // UNION_NO_MATCH
    builder.def_var(union_match_cache_var, init_zero);

    // Cache for combined IsVariant+Unbox: tracks which union register was matched.
    // IsVariant stores the payload in union_match_cache_var; Unbox reads it
    // instead of making a second extern "C" call.
    let mut last_union_match_reg: Option<u16> = None;
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
                            if call_name_regs.contains(&pc) || is_type_name_regs.contains(&pc) {
                                // This LoadK PC is only used as a Call/TailCall
                                // base (callee name) or an `Is` type-name operand.
                                // Skip heap string allocation — the Is handler uses
                                // raw bytes directly, and Call names are passed as 0.
                                let dummy = builder.ins().iconst(types::I64, 0);
                                var_types.insert(a as u32, JitVarType::Int);
                                def_var(&mut builder, &mut regs, a, dummy);
                            } else {
                                // Drop the old string value if the dest register held one.
                                if var_types.get(&(a as u32)) == Some(&JitVarType::Str) {
                                    let old = use_var(&mut builder, &regs, &var_types, a);
                                    builder.ins().call(str_drop_ref, &[vm_ctx_param, old]);
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
                                let struct_call =
                                    builder.ins().call(malloc_ref, &[vm_ctx_param, struct_size]);
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
                                    let data_call = builder
                                        .ins()
                                        .call(alloc_bytes_ref, &[vm_ctx_param, len_val]);
                                    let data_ptr = builder.inst_results(data_call)[0];
                                    let src_ptr =
                                        builder.ins().iconst(types::I64, str_bytes.as_ptr() as i64);
                                    builder.ins().call(
                                        memcpy_ref,
                                        &[vm_ctx_param, data_ptr, src_ptr, len_val],
                                    );
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
                // Track as Bool so And/Or use the correct falsy sentinel (NAN_BOX_FALSE).
                var_types.insert(a as u32, JitVarType::Bool);
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
                let val = if src_ty == JitVarType::Str {
                    // Drop old destination string if it held one and differs from source.
                    if var_types.get(&(inst.a as u32)) == Some(&JitVarType::Str) && inst.a != inst.b
                    {
                        let old = use_var(&mut builder, &regs, &var_types, inst.a);
                        builder.ins().call(str_drop_ref, &[vm_ctx_param, old]);
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
                let actual_ty = src_ty;
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
                            let raw_val = use_var(&mut builder, &regs, &var_types, leaf_reg);
                            let leaf_ty = var_types
                                .get(&(leaf_reg as u32))
                                .copied()
                                .unwrap_or(JitVarType::Int);
                            // Coerce non-string leaves to JitString before batch concat.
                            let val = if leaf_ty != JitVarType::Str {
                                if leaf_ty == JitVarType::Float {
                                    let fv = emit_unbox_float(&mut builder, raw_val);
                                    let call = builder
                                        .ins()
                                        .call(intrinsic_to_string_float_ref, &[vm_ctx_param, fv]);
                                    builder.inst_results(call)[0]
                                } else {
                                    let iv = emit_unbox_int(&mut builder, raw_val);
                                    let call = builder
                                        .ins()
                                        .call(intrinsic_to_string_int_ref, &[vm_ctx_param, iv]);
                                    builder.inst_results(call)[0]
                                }
                            } else {
                                raw_val
                            };
                            builder.ins().stack_store(val, slot, (i * 8) as i32);
                        }

                        let addr = builder.ins().stack_addr(types::I64, slot, 0);
                        let count_val = builder.ins().iconst(types::I64, leaves.len() as i64);
                        let call = builder
                            .ins()
                            .call(str_concat_multi_ref, &[vm_ctx_param, addr, count_val]);
                        let result = builder.inst_results(call)[0];

                        if let Some(old) = old_dest {
                            builder.ins().call(str_drop_ref, &[vm_ctx_param, old]);
                        }

                        var_types.insert(inst.a as u32, JitVarType::Str);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    } else {
                        // String concatenation (non-chain) — safe to read operands now.
                        let raw_lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                        let raw_rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                        let dest_ty = var_types
                            .get(&(inst.a as u32))
                            .copied()
                            .unwrap_or(JitVarType::Int);

                        // Coerce non-string operands to JitString before concat.
                        let lhs = if lhs_ty != JitVarType::Str {
                            if lhs_ty == JitVarType::Float {
                                let fv = emit_unbox_float(&mut builder, raw_lhs);
                                let call = builder
                                    .ins()
                                    .call(intrinsic_to_string_float_ref, &[vm_ctx_param, fv]);
                                builder.inst_results(call)[0]
                            } else {
                                let iv = emit_unbox_int(&mut builder, raw_lhs);
                                let call = builder
                                    .ins()
                                    .call(intrinsic_to_string_int_ref, &[vm_ctx_param, iv]);
                                builder.inst_results(call)[0]
                            }
                        } else {
                            raw_lhs
                        };
                        let rhs = if rhs_ty != JitVarType::Str {
                            if rhs_ty == JitVarType::Float {
                                let fv = emit_unbox_float(&mut builder, raw_rhs);
                                let call = builder
                                    .ins()
                                    .call(intrinsic_to_string_float_ref, &[vm_ctx_param, fv]);
                                builder.inst_results(call)[0]
                            } else {
                                let iv = emit_unbox_int(&mut builder, raw_rhs);
                                let call = builder
                                    .ins()
                                    .call(intrinsic_to_string_int_ref, &[vm_ctx_param, iv]);
                                builder.inst_results(call)[0]
                            }
                        } else {
                            raw_rhs
                        };

                        // Optimization: if dest == lhs (a = a + c), use in-place mutation
                        if dest_ty == JitVarType::Str
                            && inst.a == inst.b
                            && lhs_ty == JitVarType::Str
                        {
                            // In-place: a = a + c
                            // Inline fast path: if refcount==1 && cap>=len_a+len_b,
                            // do memcpy inline and skip the runtime call entirely.
                            //
                            // JitString layout (offsets):
                            //   0: refcount (i64)
                            //   8: len (i64)
                            //  16: char_count (i64)
                            //  24: cap (i64)
                            //  32: ptr (*mut u8)
                            let flags = MemFlags::new();

                            // Load JitString fields for lhs (a)
                            let refcount_a = builder.ins().load(types::I64, flags, lhs, 0);
                            let len_a = builder.ins().load(types::I64, flags, lhs, 8);
                            let cap_a = builder.ins().load(types::I64, flags, lhs, 24);

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
                            let ptr_a = builder.ins().load(types::I64, flags, lhs, 32);
                            let ptr_b = builder.ins().load(types::I64, flags, rhs, 32);
                            let dst = builder.ins().iadd(ptr_a, len_a);
                            builder
                                .ins()
                                .call(memcpy_ref, &[vm_ctx_param, dst, ptr_b, len_b]);
                            // Update len field in-place
                            builder.ins().store(flags, total_len, lhs, 8);
                            builder.ins().jump(merge_block, &[lhs]);

                            // --- Slow path block ---
                            builder.switch_to_block(slow_block);
                            let call = builder
                                .ins()
                                .call(str_concat_mut_ref, &[vm_ctx_param, lhs, rhs]);
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

                            let call = builder
                                .ins()
                                .call(str_concat_ref, &[vm_ctx_param, lhs, rhs]);
                            let result = builder.inst_results(call)[0];

                            if let Some(old) = old_dest {
                                builder.ins().call(str_drop_ref, &[vm_ctx_param, old]);
                            } else if dest_ty == JitVarType::Str && inst.a == inst.c {
                                // a = b + a: drop the old value of a (which is rhs)
                                builder.ins().call(str_drop_ref, &[vm_ctx_param, rhs]);
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
                    let lhs_ty = var_types
                        .get(&(inst.b as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    let rhs_ty = var_types
                        .get(&(inst.c as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    // RawInt optimization: operate on unboxed ints, defer boxing
                    let lhs_i = ensure_raw_int(&mut builder, lhs, lhs_ty);
                    let rhs_i = ensure_raw_int(&mut builder, rhs, rhs_ty);
                    let res_i = builder.ins().iadd(lhs_i, rhs_i);
                    // If the destination register is a multi-block Variable (used in a loop
                    // phi merge), we must NaN-box the result to keep a consistent
                    // representation across all incoming edges of the phi.
                    // Without boxing, the phi could merge NaN-boxed(first entry) with
                    // RawInt(back-edge), causing GetIndex/jit_rt_get_index to receive
                    // a raw integer (e.g. 0x1) instead of a NaN-boxed one.
                    if regs.vars.contains_key(&inst.a) {
                        let res_boxed = emit_box_int(&mut builder, res_i);
                        var_types.insert(inst.a as u32, JitVarType::Bool);
                        def_var(&mut builder, &mut regs, inst.a, res_boxed);
                    } else {
                        var_types.insert(inst.a as u32, JitVarType::RawInt);
                        def_var(&mut builder, &mut regs, inst.a, res_i);
                    }
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
                        let raw_val = use_var(&mut builder, &regs, &var_types, leaf_reg);
                        let leaf_ty = var_types
                            .get(&(leaf_reg as u32))
                            .copied()
                            .unwrap_or(JitVarType::Int);
                        // Coerce non-string leaves to JitString before batch concat.
                        let val = if leaf_ty != JitVarType::Str {
                            if leaf_ty == JitVarType::Float {
                                let fv = emit_unbox_float(&mut builder, raw_val);
                                let call = builder
                                    .ins()
                                    .call(intrinsic_to_string_float_ref, &[vm_ctx_param, fv]);
                                builder.inst_results(call)[0]
                            } else {
                                let iv = emit_unbox_int(&mut builder, raw_val);
                                let call = builder
                                    .ins()
                                    .call(intrinsic_to_string_int_ref, &[vm_ctx_param, iv]);
                                builder.inst_results(call)[0]
                            }
                        } else {
                            raw_val
                        };
                        builder.ins().stack_store(val, slot, (i * 8) as i32);
                    }

                    let addr = builder.ins().stack_addr(types::I64, slot, 0);
                    let count_val = builder.ins().iconst(types::I64, leaves.len() as i64);
                    let call = builder
                        .ins()
                        .call(str_concat_multi_ref, &[vm_ctx_param, addr, count_val]);
                    let result = builder.inst_results(call)[0];

                    if let Some(old) = old_dest {
                        builder.ins().call(str_drop_ref, &[vm_ctx_param, old]);
                    }

                    var_types.insert(inst.a as u32, JitVarType::Str);
                    def_var(&mut builder, &mut regs, inst.a, result);
                } else {
                    // Non-chain: regular concat
                    let raw_lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                    let raw_rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                    let lhs_ty = var_types
                        .get(&(inst.b as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    let rhs_ty = var_types
                        .get(&(inst.c as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    let dest_ty = var_types
                        .get(&(inst.a as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);

                    // Coerce non-string operands to JitString before concat.
                    let lhs = if lhs_ty != JitVarType::Str {
                        if lhs_ty == JitVarType::Float {
                            let fv = emit_unbox_float(&mut builder, raw_lhs);
                            let call = builder
                                .ins()
                                .call(intrinsic_to_string_float_ref, &[vm_ctx_param, fv]);
                            builder.inst_results(call)[0]
                        } else {
                            let iv = emit_unbox_int(&mut builder, raw_lhs);
                            let call = builder
                                .ins()
                                .call(intrinsic_to_string_int_ref, &[vm_ctx_param, iv]);
                            builder.inst_results(call)[0]
                        }
                    } else {
                        raw_lhs
                    };
                    let rhs = if rhs_ty != JitVarType::Str {
                        if rhs_ty == JitVarType::Float {
                            let fv = emit_unbox_float(&mut builder, raw_rhs);
                            let call = builder
                                .ins()
                                .call(intrinsic_to_string_float_ref, &[vm_ctx_param, fv]);
                            builder.inst_results(call)[0]
                        } else {
                            let iv = emit_unbox_int(&mut builder, raw_rhs);
                            let call = builder
                                .ins()
                                .call(intrinsic_to_string_int_ref, &[vm_ctx_param, iv]);
                            builder.inst_results(call)[0]
                        }
                    } else {
                        raw_rhs
                    };

                    // Optimization: if dest == lhs (a = a ++ c), use in-place mutation
                    if dest_ty == JitVarType::Str && inst.a == inst.b && lhs_ty == JitVarType::Str {
                        // Inline fast path: if refcount==1 && cap>=len_a+len_b,
                        // do memcpy inline and skip the runtime call entirely.
                        //
                        // JitString layout (offsets):
                        //   0: refcount (i64)
                        //   8: len (i64)
                        //  16: char_count (i64)
                        //  24: cap (i64)
                        //  32: ptr (*mut u8)
                        let flags = MemFlags::new();

                        // Load JitString fields for lhs (a)
                        let refcount_a = builder.ins().load(types::I64, flags, lhs, 0);
                        let len_a = builder.ins().load(types::I64, flags, lhs, 8);
                        let cap_a = builder.ins().load(types::I64, flags, lhs, 24);

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
                        let ptr_a = builder.ins().load(types::I64, flags, lhs, 32);
                        let ptr_b = builder.ins().load(types::I64, flags, rhs, 32);
                        let dst = builder.ins().iadd(ptr_a, len_a);
                        builder
                            .ins()
                            .call(memcpy_ref, &[vm_ctx_param, dst, ptr_b, len_b]);
                        // Update len field in-place
                        builder.ins().store(flags, total_len, lhs, 8);
                        builder.ins().jump(merge_block, &[lhs]);

                        // --- Slow path block ---
                        builder.switch_to_block(slow_block);
                        let call = builder
                            .ins()
                            .call(str_concat_mut_ref, &[vm_ctx_param, lhs, rhs]);
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

                        let call = builder
                            .ins()
                            .call(str_concat_ref, &[vm_ctx_param, lhs, rhs]);
                        let result = builder.inst_results(call)[0];

                        if let Some(old) = old_dest {
                            builder.ins().call(str_drop_ref, &[vm_ctx_param, old]);
                        } else if dest_ty == JitVarType::Str && inst.a == inst.c {
                            // a = b ++ a: drop the old value of a (which is rhs)
                            builder.ins().call(str_drop_ref, &[vm_ctx_param, rhs]);
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
                if is_float {
                    // NaN-boxing: unbox floats, sub, rebox
                    let lhs_f = emit_unbox_float(&mut builder, lhs);
                    let rhs_f = emit_unbox_float(&mut builder, rhs);
                    let r = builder.ins().fsub(lhs_f, rhs_f);
                    let res = emit_box_float(&mut builder, r);
                    var_types.insert(inst.a as u32, JitVarType::Float);
                    def_var(&mut builder, &mut regs, inst.a, res);
                } else {
                    // RawInt optimization: operate on unboxed ints, defer boxing
                    let lhs_ty = var_types
                        .get(&(inst.b as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    let rhs_ty = var_types
                        .get(&(inst.c as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    let lhs_i = ensure_raw_int(&mut builder, lhs, lhs_ty);
                    let rhs_i = ensure_raw_int(&mut builder, rhs, rhs_ty);
                    let r = builder.ins().isub(lhs_i, rhs_i);
                    // Box if dest is a multi-block Variable (loop phi merge point).
                    if regs.vars.contains_key(&inst.a) {
                        let res_boxed = emit_box_int(&mut builder, r);
                        var_types.insert(inst.a as u32, JitVarType::Bool);
                        def_var(&mut builder, &mut regs, inst.a, res_boxed);
                    } else {
                        var_types.insert(inst.a as u32, JitVarType::RawInt);
                        def_var(&mut builder, &mut regs, inst.a, r);
                    }
                }
            }
            OpCode::Mul => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let is_float = is_float_op(&var_types, inst.b, inst.c);
                if is_float {
                    let lhs_f = emit_unbox_float(&mut builder, lhs);
                    let rhs_f = emit_unbox_float(&mut builder, rhs);
                    let r = builder.ins().fmul(lhs_f, rhs_f);
                    let res = emit_box_float(&mut builder, r);
                    var_types.insert(inst.a as u32, JitVarType::Float);
                    def_var(&mut builder, &mut regs, inst.a, res);
                } else {
                    // RawInt optimization: operate on unboxed ints, defer boxing
                    let lhs_ty = var_types
                        .get(&(inst.b as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    let rhs_ty = var_types
                        .get(&(inst.c as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    let lhs_i = ensure_raw_int(&mut builder, lhs, lhs_ty);
                    let rhs_i = ensure_raw_int(&mut builder, rhs, rhs_ty);
                    let r = builder.ins().imul(lhs_i, rhs_i);
                    // Box if dest is a multi-block Variable (loop phi merge point).
                    if regs.vars.contains_key(&inst.a) {
                        let res_boxed = emit_box_int(&mut builder, r);
                        var_types.insert(inst.a as u32, JitVarType::Bool);
                        def_var(&mut builder, &mut regs, inst.a, res_boxed);
                    } else {
                        var_types.insert(inst.a as u32, JitVarType::RawInt);
                        def_var(&mut builder, &mut regs, inst.a, r);
                    }
                }
            }
            OpCode::Div => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let is_float = is_float_op(&var_types, inst.b, inst.c);
                if is_float {
                    let rhs_f = emit_unbox_float(&mut builder, rhs);
                    let zero_f = builder.ins().f64const(0.0);
                    let is_zero = builder.ins().fcmp(FloatCC::Equal, rhs_f, zero_f);
                    let trap_block = builder.create_block();
                    let div_block = builder.create_block();
                    let merge_block = builder.create_block();
                    builder.append_block_param(merge_block, types::I64);
                    builder.ins().brif(is_zero, trap_block, &[], div_block, &[]);
                    builder.switch_to_block(trap_block);
                    builder.seal_block(trap_block);
                    let sentinel = builder.ins().call(trap_divzero_ref, &[vm_ctx_param]);
                    let sentinel_val = builder.inst_results(sentinel)[0];
                    let boxed_sentinel = emit_box_int(&mut builder, sentinel_val);
                    builder.ins().jump(merge_block, &[boxed_sentinel]);
                    builder.switch_to_block(div_block);
                    builder.seal_block(div_block);
                    let lhs_f = emit_unbox_float(&mut builder, lhs);
                    let r = builder.ins().fdiv(lhs_f, rhs_f);
                    let boxed = emit_box_float(&mut builder, r);
                    builder.ins().jump(merge_block, &[boxed]);
                    builder.switch_to_block(merge_block);
                    builder.seal_block(merge_block);
                    let res = builder.block_params(merge_block)[0];
                    var_types.insert(inst.a as u32, JitVarType::Float);
                    def_var(&mut builder, &mut regs, inst.a, res);
                } else {
                    // RawInt optimization: operate on unboxed ints, defer boxing
                    let lhs_ty = var_types
                        .get(&(inst.b as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    let rhs_ty = var_types
                        .get(&(inst.c as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    let lhs_i = ensure_raw_int(&mut builder, lhs, lhs_ty);
                    let rhs_i = ensure_raw_int(&mut builder, rhs, rhs_ty);
                    // Div-by-zero guard: branch to trap_block if rhs_i == 0.
                    let zero = builder.ins().iconst(types::I64, 0);
                    let is_zero = builder.ins().icmp(IntCC::Equal, rhs_i, zero);
                    let trap_block = builder.create_block();
                    let div_block = builder.create_block();
                    let merge_block = builder.create_block();
                    builder.append_block_param(merge_block, types::I64);
                    builder.ins().brif(is_zero, trap_block, &[], div_block, &[]);
                    // Trap block: call trap helper, jump to merge with sentinel 0.
                    builder.switch_to_block(trap_block);
                    builder.seal_block(trap_block);
                    let sentinel = builder.ins().call(trap_divzero_ref, &[vm_ctx_param]);
                    let sentinel_val = builder.inst_results(sentinel)[0];
                    builder.ins().jump(merge_block, &[sentinel_val]);
                    // Div block: perform the division, jump to merge with result.
                    builder.switch_to_block(div_block);
                    builder.seal_block(div_block);
                    let r = builder.ins().sdiv(lhs_i, rhs_i);
                    builder.ins().jump(merge_block, &[r]);
                    // Merge block: result is the block param (RawInt).
                    builder.switch_to_block(merge_block);
                    builder.seal_block(merge_block);
                    let res = builder.block_params(merge_block)[0];
                    // Box if dest is a multi-block Variable (loop phi merge point).
                    if regs.vars.contains_key(&inst.a) {
                        let res_boxed = emit_box_int(&mut builder, res);
                        var_types.insert(inst.a as u32, JitVarType::Bool);
                        def_var(&mut builder, &mut regs, inst.a, res_boxed);
                    } else {
                        var_types.insert(inst.a as u32, JitVarType::RawInt);
                        def_var(&mut builder, &mut regs, inst.a, res);
                    }
                }
            }
            OpCode::Mod => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let is_float = is_float_op(&var_types, inst.b, inst.c);
                if is_float {
                    // NaN-boxing: unbox floats, call fmod, rebox
                    let rhs_f = emit_unbox_float(&mut builder, rhs);
                    let zero_f = builder.ins().f64const(0.0);
                    let is_zero = builder.ins().fcmp(FloatCC::Equal, rhs_f, zero_f);
                    let trap_block = builder.create_block();
                    let mod_block = builder.create_block();
                    let merge_block = builder.create_block();
                    builder.append_block_param(merge_block, types::I64);
                    builder.ins().brif(is_zero, trap_block, &[], mod_block, &[]);
                    builder.switch_to_block(trap_block);
                    builder.seal_block(trap_block);
                    let sentinel = builder.ins().call(trap_divzero_ref, &[vm_ctx_param]);
                    let sentinel_val = builder.inst_results(sentinel)[0];
                    let boxed_sentinel = emit_box_int(&mut builder, sentinel_val);
                    builder.ins().jump(merge_block, &[boxed_sentinel]);
                    builder.switch_to_block(mod_block);
                    builder.seal_block(mod_block);
                    let lhs_f = emit_unbox_float(&mut builder, lhs);
                    let call = builder
                        .ins()
                        .call(intrinsic_fmod_ref, &[vm_ctx_param, lhs_f, rhs_f]);
                    let result_f = builder.inst_results(call)[0];
                    let result = emit_box_float(&mut builder, result_f);
                    builder.ins().jump(merge_block, &[result]);
                    builder.switch_to_block(merge_block);
                    builder.seal_block(merge_block);
                    let merged = builder.block_params(merge_block)[0];
                    var_types.insert(inst.a as u32, JitVarType::Float);
                    def_var(&mut builder, &mut regs, inst.a, merged);
                } else {
                    // RawInt optimization: operate on unboxed ints, defer boxing
                    let lhs_ty = var_types
                        .get(&(inst.b as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    let rhs_ty = var_types
                        .get(&(inst.c as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    let lhs_i = ensure_raw_int(&mut builder, lhs, lhs_ty);
                    let rhs_i = ensure_raw_int(&mut builder, rhs, rhs_ty);
                    let zero = builder.ins().iconst(types::I64, 0);
                    let is_zero = builder.ins().icmp(IntCC::Equal, rhs_i, zero);
                    let trap_block = builder.create_block();
                    let rem_block = builder.create_block();
                    let merge_block = builder.create_block();
                    builder.append_block_param(merge_block, types::I64);
                    builder.ins().brif(is_zero, trap_block, &[], rem_block, &[]);
                    builder.switch_to_block(trap_block);
                    builder.seal_block(trap_block);
                    let sentinel = builder.ins().call(trap_divzero_ref, &[vm_ctx_param]);
                    let sentinel_val = builder.inst_results(sentinel)[0];
                    builder.ins().jump(merge_block, &[sentinel_val]);
                    builder.switch_to_block(rem_block);
                    builder.seal_block(rem_block);
                    let res_i = builder.ins().srem(lhs_i, rhs_i);
                    builder.ins().jump(merge_block, &[res_i]);
                    builder.switch_to_block(merge_block);
                    builder.seal_block(merge_block);
                    let result = builder.block_params(merge_block)[0];
                    // Box if dest is a multi-block Variable (loop phi merge point).
                    if regs.vars.contains_key(&inst.a) {
                        let result_boxed = emit_box_int(&mut builder, result);
                        var_types.insert(inst.a as u32, JitVarType::Bool);
                        def_var(&mut builder, &mut regs, inst.a, result_boxed);
                    } else {
                        var_types.insert(inst.a as u32, JitVarType::RawInt);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }
                }
            }
            OpCode::Neg => {
                let operand = use_var(&mut builder, &regs, &var_types, inst.b);
                let op_ty = var_types
                    .get(&(inst.b as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                if op_ty == JitVarType::Float {
                    let f = emit_unbox_float(&mut builder, operand);
                    let r = builder.ins().fneg(f);
                    let res = emit_box_float(&mut builder, r);
                    var_types.insert(inst.a as u32, JitVarType::Float);
                    def_var(&mut builder, &mut regs, inst.a, res);
                } else {
                    // RawInt optimization
                    let i = ensure_raw_int(&mut builder, operand, op_ty);
                    let r = builder.ins().ineg(i);
                    var_types.insert(inst.a as u32, JitVarType::RawInt);
                    def_var(&mut builder, &mut regs, inst.a, r);
                }
            }
            OpCode::FloorDiv => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let is_float = is_float_op(&var_types, inst.b, inst.c);
                if is_float {
                    let rhs_f = emit_unbox_float(&mut builder, rhs);
                    let zero_f = builder.ins().f64const(0.0);
                    let is_zero = builder.ins().fcmp(FloatCC::Equal, rhs_f, zero_f);
                    let trap_block = builder.create_block();
                    let div_block = builder.create_block();
                    let merge_block = builder.create_block();
                    builder.append_block_param(merge_block, types::I64);
                    builder.ins().brif(is_zero, trap_block, &[], div_block, &[]);
                    builder.switch_to_block(trap_block);
                    builder.seal_block(trap_block);
                    let sentinel = builder.ins().call(trap_divzero_ref, &[vm_ctx_param]);
                    let sentinel_val = builder.inst_results(sentinel)[0];
                    let boxed_sentinel = emit_box_int(&mut builder, sentinel_val);
                    builder.ins().jump(merge_block, &[boxed_sentinel]);
                    builder.switch_to_block(div_block);
                    builder.seal_block(div_block);
                    let lhs_f = emit_unbox_float(&mut builder, lhs);
                    let div = builder.ins().fdiv(lhs_f, rhs_f);
                    let floored = builder.ins().floor(div);
                    let boxed = emit_box_float(&mut builder, floored);
                    builder.ins().jump(merge_block, &[boxed]);
                    builder.switch_to_block(merge_block);
                    builder.seal_block(merge_block);
                    let res = builder.block_params(merge_block)[0];
                    def_var(&mut builder, &mut regs, inst.a, res);
                } else {
                    // RawInt optimization: operate on unboxed ints, defer boxing
                    let lhs_ty = var_types
                        .get(&(inst.b as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    let rhs_ty = var_types
                        .get(&(inst.c as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    let lhs_i = ensure_raw_int(&mut builder, lhs, lhs_ty);
                    let rhs_i = ensure_raw_int(&mut builder, rhs, rhs_ty);
                    // Div-by-zero guard for integer floor division.
                    let zero = builder.ins().iconst(types::I64, 0);
                    let is_zero = builder.ins().icmp(IntCC::Equal, rhs_i, zero);
                    let trap_block = builder.create_block();
                    let div_block = builder.create_block();
                    let merge_block = builder.create_block();
                    builder.append_block_param(merge_block, types::I64);
                    builder.ins().brif(is_zero, trap_block, &[], div_block, &[]);
                    builder.switch_to_block(trap_block);
                    builder.seal_block(trap_block);
                    let sentinel = builder.ins().call(trap_divzero_ref, &[vm_ctx_param]);
                    let sentinel_val = builder.inst_results(sentinel)[0];
                    builder.ins().jump(merge_block, &[sentinel_val]);
                    builder.switch_to_block(div_block);
                    builder.seal_block(div_block);
                    let r = builder.ins().sdiv(lhs_i, rhs_i);
                    builder.ins().jump(merge_block, &[r]);
                    builder.switch_to_block(merge_block);
                    builder.seal_block(merge_block);
                    let res = builder.block_params(merge_block)[0];
                    // Box if dest is a multi-block Variable (loop phi merge point).
                    if regs.vars.contains_key(&inst.a) {
                        let res_boxed = emit_box_int(&mut builder, res);
                        var_types.insert(inst.a as u32, JitVarType::Int);
                        def_var(&mut builder, &mut regs, inst.a, res_boxed);
                    } else {
                        var_types.insert(inst.a as u32, JitVarType::RawInt);
                        def_var(&mut builder, &mut regs, inst.a, res);
                    }
                }
            }
            OpCode::Pow => {
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let is_float = is_float_op(&var_types, inst.b, inst.c);
                if is_float {
                    // NaN-boxing: unbox floats, call pow, rebox
                    let lhs_f = emit_unbox_float(&mut builder, lhs);
                    let rhs_f = emit_unbox_float(&mut builder, rhs);
                    let call = builder
                        .ins()
                        .call(intrinsic_pow_float_ref, &[vm_ctx_param, lhs_f, rhs_f]);
                    let result_f = builder.inst_results(call)[0];
                    let result = emit_box_float(&mut builder, result_f);
                    var_types.insert(inst.a as u32, JitVarType::Float);
                    def_var(&mut builder, &mut regs, inst.a, result);
                } else {
                    // NaN-boxing: unbox ints, call pow, rebox
                    let lhs_i = emit_unbox_int(&mut builder, lhs);
                    let rhs_i = emit_unbox_int(&mut builder, rhs);
                    let call = builder
                        .ins()
                        .call(intrinsic_pow_int_ref, &[vm_ctx_param, lhs_i, rhs_i]);
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
            // NaN-boxing: comparison results are NaN-boxed booleans
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
                    // String equality returns raw 0/1 i64, but we need i1 for emit_box_bool
                    let call = builder.ins().call(str_eq_ref, &[vm_ctx_param, lhs, rhs]);
                    let raw = builder.inst_results(call)[0];
                    // raw is i64 (0 or 1), convert to i1
                    let zero = builder.ins().iconst(types::I64, 0);
                    builder.ins().icmp(IntCC::NotEqual, raw, zero)
                } else if lhs_ty == JitVarType::Float || rhs_ty == JitVarType::Float {
                    let lhs_f = emit_unbox_float(&mut builder, lhs);
                    let rhs_f = emit_unbox_float(&mut builder, rhs);
                    builder.ins().fcmp(FloatCC::Equal, lhs_f, rhs_f)
                } else if lhs_ty == JitVarType::RawInt || rhs_ty == JitVarType::RawInt {
                    // RawInt (unboxed) vs Int (NaN-boxed): normalize both to raw before comparing.
                    let lhs_i = ensure_raw_int(&mut builder, lhs, lhs_ty);
                    let rhs_i = ensure_raw_int(&mut builder, rhs, rhs_ty);
                    builder.ins().icmp(IntCC::Equal, lhs_i, rhs_i)
                } else {
                    // Int: can compare NaN-boxed values directly (both are identically encoded)
                    builder.ins().icmp(IntCC::Equal, lhs, rhs)
                };
                let one = builder.ins().iconst(types::I64, 1);
                let zero = builder.ins().iconst(types::I64, 0);
                let cmp_int = builder.ins().select(cmp_i1, one, zero);
                let res = emit_box_bool_from_int(&mut builder, cmp_int);
                var_types.insert(inst.a as u32, JitVarType::Bool);
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
                    let call = builder.ins().call(str_cmp_ref, &[vm_ctx_param, lhs, rhs]);
                    let cmp_result = builder.inst_results(call)[0];
                    let zero = builder.ins().iconst(types::I64, 0);
                    builder.ins().icmp(IntCC::SignedLessThan, cmp_result, zero)
                } else if lhs_ty == JitVarType::Float || rhs_ty == JitVarType::Float {
                    let lhs_f = emit_unbox_float(&mut builder, lhs);
                    let rhs_f = emit_unbox_float(&mut builder, rhs);
                    builder.ins().fcmp(FloatCC::LessThan, lhs_f, rhs_f)
                } else {
                    // Int: must unbox before comparing — NbValue encoding does NOT preserve
                    // signed ordering (negative values have high payload bits set).
                    let lhs_i = emit_unbox_int(&mut builder, lhs);
                    let rhs_i = emit_unbox_int(&mut builder, rhs);
                    builder.ins().icmp(IntCC::SignedLessThan, lhs_i, rhs_i)
                };
                let one = builder.ins().iconst(types::I64, 1);
                let zero = builder.ins().iconst(types::I64, 0);
                let cmp_int = builder.ins().select(cmp_i1, one, zero);
                let res = emit_box_bool_from_int(&mut builder, cmp_int);
                var_types.insert(inst.a as u32, JitVarType::Bool);
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
                    let call = builder.ins().call(str_cmp_ref, &[vm_ctx_param, lhs, rhs]);
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
                    // Int: must unbox before comparing — NbValue encoding does NOT preserve
                    // signed ordering (negative values have high payload bits set).
                    let lhs_i = emit_unbox_int(&mut builder, lhs);
                    let rhs_i = emit_unbox_int(&mut builder, rhs);
                    builder
                        .ins()
                        .icmp(IntCC::SignedLessThanOrEqual, lhs_i, rhs_i)
                };
                let one = builder.ins().iconst(types::I64, 1);
                let zero = builder.ins().iconst(types::I64, 0);
                let cmp_int = builder.ins().select(cmp_i1, one, zero);
                let res = emit_box_bool_from_int(&mut builder, cmp_int);
                var_types.insert(inst.a as u32, JitVarType::Bool);
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
                    JitVarType::Ptr => builder.ins().iconst(types::I64, NAN_BOX_NULL),
                    JitVarType::RawInt => builder.ins().iconst(types::I64, 0i64),
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
                // Short-circuit AND: result is Bool (true iff both operands are truthy).
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let b_ty = var_types
                    .get(&(inst.b as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let c_ty = var_types
                    .get(&(inst.c as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let lhs_falsy_val = match b_ty {
                    JitVarType::Int => builder.ins().iconst(types::I64, nan_box_int(0)),
                    JitVarType::Float => builder.ins().iconst(types::I64, 0i64),
                    JitVarType::Bool => builder.ins().iconst(types::I64, NAN_BOX_FALSE),
                    JitVarType::Str => builder.ins().iconst(types::I64, 0i64),
                    JitVarType::Ptr => builder.ins().iconst(types::I64, NAN_BOX_NULL),
                    JitVarType::RawInt => builder.ins().iconst(types::I64, 0i64),
                };
                let rhs_falsy_val = match c_ty {
                    JitVarType::Int => builder.ins().iconst(types::I64, nan_box_int(0)),
                    JitVarType::Float => builder.ins().iconst(types::I64, 0i64),
                    JitVarType::Bool => builder.ins().iconst(types::I64, NAN_BOX_FALSE),
                    JitVarType::Str => builder.ins().iconst(types::I64, 0i64),
                    JitVarType::Ptr => builder.ins().iconst(types::I64, NAN_BOX_NULL),
                    JitVarType::RawInt => builder.ins().iconst(types::I64, 0i64),
                };
                let lhs_is_falsy = builder.ins().icmp(IntCC::Equal, lhs, lhs_falsy_val);
                let rhs_is_falsy = builder.ins().icmp(IntCC::Equal, rhs, rhs_falsy_val);
                // AND is true iff both are truthy (neither is falsy)
                let either_falsy = builder.ins().bor(lhs_is_falsy, rhs_is_falsy);
                // either_falsy is i8 (i1); select NAN_BOX_FALSE if falsy, else NAN_BOX_TRUE
                let t = builder.ins().iconst(types::I64, NAN_BOX_TRUE);
                let f = builder.ins().iconst(types::I64, NAN_BOX_FALSE);
                let res = builder.ins().select(either_falsy, f, t);
                var_types.insert(inst.a as u32, JitVarType::Bool);
                def_var(&mut builder, &mut regs, inst.a, res);
            }
            OpCode::Or => {
                // Short-circuit OR: result is Bool (true iff at least one operand is truthy).
                let lhs = use_var(&mut builder, &regs, &var_types, inst.b);
                let rhs = use_var(&mut builder, &regs, &var_types, inst.c);
                let b_ty = var_types
                    .get(&(inst.b as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let c_ty = var_types
                    .get(&(inst.c as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let lhs_falsy_val = match b_ty {
                    JitVarType::Int => builder.ins().iconst(types::I64, nan_box_int(0)),
                    JitVarType::Float => builder.ins().iconst(types::I64, 0i64),
                    JitVarType::Bool => builder.ins().iconst(types::I64, NAN_BOX_FALSE),
                    JitVarType::Str => builder.ins().iconst(types::I64, 0i64),
                    JitVarType::Ptr => builder.ins().iconst(types::I64, NAN_BOX_NULL),
                    JitVarType::RawInt => builder.ins().iconst(types::I64, 0i64),
                };
                let rhs_falsy_val = match c_ty {
                    JitVarType::Int => builder.ins().iconst(types::I64, nan_box_int(0)),
                    JitVarType::Float => builder.ins().iconst(types::I64, 0i64),
                    JitVarType::Bool => builder.ins().iconst(types::I64, NAN_BOX_FALSE),
                    JitVarType::Str => builder.ins().iconst(types::I64, 0i64),
                    JitVarType::Ptr => builder.ins().iconst(types::I64, NAN_BOX_NULL),
                    JitVarType::RawInt => builder.ins().iconst(types::I64, 0i64),
                };
                let lhs_is_falsy = builder.ins().icmp(IntCC::Equal, lhs, lhs_falsy_val);
                let rhs_is_falsy = builder.ins().icmp(IntCC::Equal, rhs, rhs_falsy_val);
                // OR is true iff at least one is truthy (not both falsy)
                let both_falsy = builder.ins().band(lhs_is_falsy, rhs_is_falsy);
                // both_falsy is i8 (i1); select NAN_BOX_FALSE if both falsy, else NAN_BOX_TRUE
                let t = builder.ins().iconst(types::I64, NAN_BOX_TRUE);
                let f = builder.ins().iconst(types::I64, NAN_BOX_FALSE);
                let res = builder.ins().select(both_falsy, f, t);
                var_types.insert(inst.a as u32, JitVarType::Bool);
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

                if let Some(direct_cond) = pending_test_value.take() {
                    // Direct SSA condition from IsVariant — already a NaN-boxed Bool.
                    let falsy_val = builder.ins().iconst(types::I64, NAN_BOX_FALSE);
                    let is_truthy = builder.ins().icmp(IntCC::NotEqual, direct_cond, falsy_val);
                    builder
                        .ins()
                        .brif(is_truthy, fallthrough_block, &[], target_block, &[]);
                } else if let Some(test_reg) = pending_test.take() {
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
                        JitVarType::Ptr => builder.ins().iconst(types::I64, NAN_BOX_NULL),
                        JitVarType::RawInt => builder.ins().iconst(types::I64, 0i64),
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
                // IMPORTANT: Only drop registers that are multi-block Variables
                // (which use Cranelift phi nodes and are valid in any block).
                // SSA-only registers may be defined in a non-dominating block
                // and referencing them here would cause a Cranelift verifier error.
                let ret_reg = inst.a;
                for (&reg_id, &ty) in &var_types {
                    if ty == JitVarType::Str
                        && reg_id != ret_reg as u32
                        && (reg_id as usize) < regs.num_regs
                        && regs.vars.contains_key(&(reg_id as u16))
                    {
                        let v = use_var(&mut builder, &regs, &var_types, reg_id as u16);
                        builder.ins().call(str_drop_ref, &[vm_ctx_param, v]);
                    }
                }
                let val = use_var(&mut builder, &regs, &var_types, ret_reg);
                // NaN-boxing: all values are I64, no bitcast needed.
                // If the return value is a RawInt (from the RawInt optimization),
                // box it before returning so callers receive a proper NaN-boxed value.
                let ret_ty = var_types
                    .get(&(ret_reg as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let val = ensure_boxed_int(&mut builder, val, ret_ty);
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
                    builder.ins().call(str_drop_ref, &[vm_ctx_param, old]);
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
                                Vec::with_capacity(num_args + 1);
                            args.push(vm_ctx_param);
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
                        builder.ins().call(str_drop_ref, &[vm_ctx_param, v]);
                    }
                    var_types.remove(&arg_reg);
                }

                // Infer return type from callee's declared return type.
                let ret_ty = callee_name
                    .as_ref()
                    .and_then(|n| cell_return_types.get(n.as_str()))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                // Propagate the actual return type so Ptr/Str/Float/Bool
                // are tracked correctly for downstream instructions.
                // NOTE: For non-Str/non-Ptr types, Int is fine since they
                // are all NaN-boxed I64 anyway. The key distinction is
                // Str (refcounted) and Ptr (heap Value*).
                var_types.insert(base as u32, ret_ty);
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
                    builder.ins().call(str_drop_ref, &[vm_ctx_param, old]);
                    var_types.remove(&(base as u32));
                }

                // Drop any string-typed argument registers.
                for i in 0..num_args {
                    let arg_reg = (base + 1 + i as u16) as u32;
                    if var_types.get(&arg_reg) == Some(&JitVarType::Str) {
                        if (arg_reg as usize) < regs.num_regs {
                            let v = use_var(&mut builder, &regs, &var_types, arg_reg as u16);
                            builder.ins().call(str_drop_ref, &[vm_ctx_param, v]);
                        }
                        var_types.remove(&arg_reg);
                    }
                }

                if is_self_call && self_tco {
                    if let Some(loop_block) = tco_loop_block {
                        let mut new_args: Vec<cranelift_codegen::ir::Value> =
                            Vec::with_capacity(num_args);
                        let mut new_arg_types: Vec<JitVarType> = Vec::with_capacity(num_args);
                        for i in 0..num_args {
                            let arg_reg = base + 1 + i as u16;
                            let arg_ty = var_types
                                .get(&(arg_reg as u32))
                                .copied()
                                .unwrap_or(JitVarType::Int);
                            let val = use_var(&mut builder, &regs, &var_types, arg_reg);
                            // Box RawInt args before storing back to the loop variable,
                            // so parameter registers are always in NaN-boxed form on
                            // the next iteration (matches the initial parameter setup).
                            let val = ensure_boxed_int(&mut builder, val, arg_ty);
                            new_args.push(val);
                            new_arg_types.push(if arg_ty == JitVarType::RawInt {
                                JitVarType::Int
                            } else {
                                arg_ty
                            });
                        }
                        for (i, (&val, &arg_ty)) in
                            new_args.iter().zip(new_arg_types.iter()).enumerate()
                        {
                            if i < regs.num_regs {
                                def_var(&mut builder, &mut regs, i as u16, val);
                                // Reset the type of the parameter register to its boxed form
                                // so subsequent instructions (and Return) see the correct type.
                                var_types.insert(i as u32, arg_ty);
                            }
                        }
                        builder.ins().jump(loop_block, &[]);
                        terminated = true;
                    }
                } else if let Some(ref name) = callee_name {
                    if let Some(&callee_func_id) = func_ids.get(name.as_str()) {
                        if let Some(&func_ref) = callee_refs.get(&callee_func_id) {
                            let mut args: Vec<cranelift_codegen::ir::Value> =
                                Vec::with_capacity(num_args + 1);
                            args.push(vm_ctx_param);
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
                    &[vm_ctx_param, record_ptr, field_name_ptr, field_name_len],
                );
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, result);
            }
            OpCode::SetField => {
                let record_ptr = use_var(&mut builder, &regs, &var_types, inst.a);
                let val_raw = use_var(&mut builder, &regs, &var_types, inst.c);
                let val_ty = var_types
                    .get(&(inst.c as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let value_ptr = match val_ty {
                    JitVarType::RawInt => ensure_boxed_int(&mut builder, val_raw, val_ty),
                    JitVarType::Str => {
                        let call = builder.ins().call(str_to_nb_ref, &[vm_ctx_param, val_raw]);
                        builder.inst_results(call)[0]
                    }
                    _ => val_raw,
                };
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
                    &[
                        vm_ctx_param,
                        record_ptr,
                        field_name_ptr,
                        field_name_len,
                        value_ptr,
                    ],
                );
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // GetIndex: Get element from list/map by index/key
            // r[a] = r[b][r[c]]
            OpCode::GetIndex => {
                let collection_ptr = use_var(&mut builder, &regs, &var_types, inst.b);
                let idx_raw = use_var(&mut builder, &regs, &var_types, inst.c);
                // RawInt loop counters must be NaN-boxed before passing to the runtime
                // helper, which uses decode_nbvalue_index expecting TAG_INT encoding.
                let idx_ty = var_types
                    .get(&(inst.c as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let index_ptr = match idx_ty {
                    JitVarType::RawInt => ensure_boxed_int(&mut builder, idx_raw, idx_ty),
                    JitVarType::Str => {
                        let call = builder.ins().call(str_to_nb_ref, &[vm_ctx_param, idx_raw]);
                        builder.inst_results(call)[0]
                    }
                    _ => idx_raw,
                };
                let call = builder
                    .ins()
                    .call(get_index_ref, &[vm_ctx_param, collection_ptr, index_ptr]);
                let result = builder.inst_results(call)[0];
                // jit_rt_get_index returns NbValue-encoded i64:
                //   Float elements → raw f64 bits (same as JitVarType::Float)
                //   Int elements   → NAN-tagged
                //   Heap values    → NAN-tagged pointer
                // Use the pre-pass float_regs result to set the correct JIT type
                // so that downstream arithmetic can correctly identify float operands.
                let elem_ty = if float_regs.contains(&inst.a) {
                    JitVarType::Float
                } else {
                    JitVarType::Int
                };
                var_types.insert(inst.a as u32, elem_ty);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // SetIndex: Set element in list/map by index/key
            // r[a][r[b]] = r[c]
            OpCode::SetIndex => {
                let collection_ptr = use_var(&mut builder, &regs, &var_types, inst.a);
                let idx_raw = use_var(&mut builder, &regs, &var_types, inst.b);
                // Box RawInt index — same reason as GetIndex.
                let idx_ty = var_types
                    .get(&(inst.b as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                let index_ptr = match idx_ty {
                    JitVarType::RawInt => ensure_boxed_int(&mut builder, idx_raw, idx_ty),
                    JitVarType::Str => {
                        let call = builder.ins().call(str_to_nb_ref, &[vm_ctx_param, idx_raw]);
                        builder.inst_results(call)[0]
                    }
                    _ => idx_raw,
                };
                let val_raw = use_var(&mut builder, &regs, &var_types, inst.c);
                let val_ty = var_types
                    .get(&(inst.c as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);
                // jit_rt_set_index decodes the value via nbval_to_value:
                //   Float bits (JitVarType::Float) → Value::Float  ✓ (no NaN mask)
                //   NbValue int (JitVarType::Int) → Value::Int     ✓ (NAN | TAG_INT)
                //   RawInt → must box to NbValue first
                let value_nbval = match val_ty {
                    JitVarType::RawInt => ensure_boxed_int(&mut builder, val_raw, val_ty),
                    JitVarType::Str => {
                        let call = builder.ins().call(str_to_nb_ref, &[vm_ctx_param, val_raw]);
                        builder.inst_results(call)[0]
                    }
                    _ => val_raw,
                };
                let call = builder.ins().call(
                    set_index_ref,
                    &[vm_ctx_param, collection_ptr, index_ptr, value_nbval],
                );
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // Append: Append element to list
            // r[a] = append(r[a], r[b])
            OpCode::Append => {
                let list_ptr = use_var(&mut builder, &regs, &var_types, inst.a);
                let elem_raw = use_var(&mut builder, &regs, &var_types, inst.b);
                let elem_ty = var_types
                    .get(&(inst.b as u32))
                    .copied()
                    .unwrap_or(JitVarType::Int);

                // Use typed helpers for known scalar element types to avoid
                // generic NbValue decoding on the runtime side.
                let call = match elem_ty {
                    JitVarType::Int | JitVarType::RawInt => {
                        let raw_int = ensure_raw_int(&mut builder, elem_raw, elem_ty);
                        builder
                            .ins()
                            .call(list_append_int_ref, &[vm_ctx_param, list_ptr, raw_int])
                    }
                    JitVarType::Float => builder
                        .ins()
                        .call(list_append_float_ref, &[vm_ctx_param, list_ptr, elem_raw]),
                    _ => {
                        // Fallback generic path expects NbValue encoding.
                        let element_nb = ensure_boxed_int(&mut builder, elem_raw, elem_ty);
                        builder
                            .ins()
                            .call(list_append_ref, &[vm_ctx_param, list_ptr, element_nb])
                    }
                };
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // NewList: Create a list from register values
            // r[a] = List([r[a+1], ..., r[a+b]])
            OpCode::NewList | OpCode::NewListStack => {
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
                    let reg = inst.a + 1 + i as u16;
                    let raw = use_var(&mut builder, &regs, &var_types, reg);
                    let ty = var_types
                        .get(&(reg as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    let value = match ty {
                        JitVarType::RawInt => ensure_boxed_int(&mut builder, raw, ty),
                        JitVarType::Str => {
                            let call = builder.ins().call(str_to_nb_ref, &[vm_ctx_param, raw]);
                            builder.inst_results(call)[0]
                        }
                        _ => raw,
                    };
                    let offset = (i * 8) as i32;
                    builder
                        .ins()
                        .store(MemFlags::new(), value, array_ptr, offset);
                }

                // Call jit_rt_new_list(array_ptr, count)
                let count_val = builder.ins().iconst(types::I64, count as i64);
                let call = builder
                    .ins()
                    .call(new_list_ref, &[vm_ctx_param, array_ptr, count_val]);
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // NewMap: Create a map from key-value pairs
            // r[a] = Map({r[a+1]: r[a+2], r[a+3]: r[a+4], ...})
            OpCode::NewMap => {
                let count = inst.b as usize; // number of key-value pairs

                // Fast path: if ALL keys and values are JitString pointers, use
                // jit_rt_new_map_strs which converts each in one copy (vs. the
                // general path which does str_to_nb_ref → nb_decode = two copies).
                let all_strings = count > 0
                    && (0..count).all(|i| {
                        let key_reg = inst.a + 1 + (i * 2) as u16;
                        let val_reg = inst.a + 2 + (i * 2) as u16;
                        var_types.get(&(key_reg as u32)).copied() == Some(JitVarType::Str)
                            && var_types.get(&(val_reg as u32)).copied() == Some(JitVarType::Str)
                    });

                // Allocate stack space for key-value array (count * 2 * 8 bytes)
                let slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    (count * 2 * 8).max(8) as u32,
                    8,
                ));
                let array_ptr = builder.ins().stack_addr(types::I64, slot, 0);

                if all_strings {
                    // Pass raw JitString pointers directly — no str_to_nb_ref overhead.
                    for i in 0..count {
                        let key_reg = inst.a + 1 + (i * 2) as u16;
                        let val_reg = inst.a + 2 + (i * 2) as u16;
                        let raw_key = use_var(&mut builder, &regs, &var_types, key_reg);
                        let raw_value = use_var(&mut builder, &regs, &var_types, val_reg);
                        builder.ins().store(
                            MemFlags::new(),
                            raw_key,
                            array_ptr,
                            (i * 2 * 8) as i32,
                        );
                        builder.ins().store(
                            MemFlags::new(),
                            raw_value,
                            array_ptr,
                            ((i * 2 + 1) * 8) as i32,
                        );
                    }
                    let count_val = builder.ins().iconst(types::I64, count as i64);
                    let call = builder
                        .ins()
                        .call(new_map_strs_ref, &[vm_ctx_param, array_ptr, count_val]);
                    let result = builder.inst_results(call)[0];
                    var_types.insert(inst.a as u32, JitVarType::Ptr);
                    def_var(&mut builder, &mut regs, inst.a, result);
                } else {
                    // General path: convert JitString to NaN-boxed Value via str_to_nb.
                    for i in 0..count {
                        let key_reg = inst.a + 1 + (i * 2) as u16;
                        let val_reg = inst.a + 2 + (i * 2) as u16;
                        let raw_key = use_var(&mut builder, &regs, &var_types, key_reg);
                        let raw_value = use_var(&mut builder, &regs, &var_types, val_reg);
                        let key_ty = var_types
                            .get(&(key_reg as u32))
                            .copied()
                            .unwrap_or(JitVarType::Int);
                        let val_ty = var_types
                            .get(&(val_reg as u32))
                            .copied()
                            .unwrap_or(JitVarType::Int);

                        let key = if key_ty == JitVarType::Str {
                            let call = builder.ins().call(str_to_nb_ref, &[vm_ctx_param, raw_key]);
                            builder.inst_results(call)[0]
                        } else {
                            raw_key
                        };
                        let value = if val_ty == JitVarType::Str {
                            let call = builder
                                .ins()
                                .call(str_to_nb_ref, &[vm_ctx_param, raw_value]);
                            builder.inst_results(call)[0]
                        } else {
                            raw_value
                        };

                        builder
                            .ins()
                            .store(MemFlags::new(), key, array_ptr, (i * 2 * 8) as i32);
                        builder.ins().store(
                            MemFlags::new(),
                            value,
                            array_ptr,
                            ((i * 2 + 1) * 8) as i32,
                        );
                    }
                    let count_val = builder.ins().iconst(types::I64, count as i64);
                    let call = builder
                        .ins()
                        .call(new_map_ref, &[vm_ctx_param, array_ptr, count_val]);
                    let result = builder.inst_results(call)[0];
                    var_types.insert(inst.a as u32, JitVarType::Ptr);
                    def_var(&mut builder, &mut regs, inst.a, result);
                }
            }

            // NewTuple: Create a tuple from register values
            // r[a] = Tuple([r[a+1], ..., r[a+b]])
            OpCode::NewTuple | OpCode::NewTupleStack => {
                let count = inst.b as usize;

                // Allocate stack space for the array of value pointers
                let slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    (count * 8) as u32,
                    8,
                ));
                let array_ptr = builder.ins().stack_addr(types::I64, slot, 0);

                // Fill the array with values from registers r[a+1] through r[a+b]
                for i in 0..count {
                    let reg = inst.a + 1 + i as u16;
                    let raw = use_var(&mut builder, &regs, &var_types, reg);
                    let ty = var_types
                        .get(&(reg as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    let value = match ty {
                        JitVarType::RawInt => ensure_boxed_int(&mut builder, raw, ty),
                        JitVarType::Str => {
                            let call = builder.ins().call(str_to_nb_ref, &[vm_ctx_param, raw]);
                            builder.inst_results(call)[0]
                        }
                        _ => raw,
                    };
                    let offset = (i * 8) as i32;
                    builder
                        .ins()
                        .store(MemFlags::new(), value, array_ptr, offset);
                }

                // Call jit_rt_new_tuple(array_ptr, count)
                let count_val = builder.ins().iconst(types::I64, count as i64);
                let call = builder
                    .ins()
                    .call(new_tuple_ref, &[vm_ctx_param, array_ptr, count_val]);
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // NewSet: Create a set from register values
            // r[a] = Set({r[a+1], ..., r[a+b]})
            OpCode::NewSet => {
                let count = inst.b as usize;

                // Allocate stack space for the array of values
                let slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    (count * 8) as u32,
                    8,
                ));
                let array_ptr = builder.ins().stack_addr(types::I64, slot, 0);

                // Fill the array with values from registers r[a+1] through r[a+b]
                for i in 0..count {
                    let reg = inst.a + 1 + i as u16;
                    let raw = use_var(&mut builder, &regs, &var_types, reg);
                    let ty = var_types
                        .get(&(reg as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);
                    let value = match ty {
                        JitVarType::RawInt => ensure_boxed_int(&mut builder, raw, ty),
                        JitVarType::Str => {
                            let call = builder.ins().call(str_to_nb_ref, &[vm_ctx_param, raw]);
                            builder.inst_results(call)[0]
                        }
                        _ => raw,
                    };
                    let offset = (i * 8) as i32;
                    builder
                        .ins()
                        .store(MemFlags::new(), value, array_ptr, offset);
                }

                // Call jit_rt_new_set(ctx, values_ptr, count)
                let count_val = builder.ins().iconst(types::I64, count as i64);
                let call = builder
                    .ins()
                    .call(new_set_ref, &[vm_ctx_param, array_ptr, count_val]);
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // NewRecord: Create a record (struct) with an empty field map
            // Instruction format: ABx (a=dest_reg, bx=type_name_string_index)
            // r[a] = Record { type_name: strings[bx], fields: {} }
            OpCode::NewRecord => {
                let bx = inst.bx() as usize;
                let type_name_str = if bx < string_table.len() {
                    &string_table[bx]
                } else {
                    "Unknown"
                };
                let type_name_bytes = type_name_str.as_bytes();
                let type_name_ptr = builder
                    .ins()
                    .iconst(types::I64, type_name_bytes.as_ptr() as i64);
                let type_name_len = builder
                    .ins()
                    .iconst(types::I64, type_name_bytes.len() as i64);
                let call = builder.ins().call(
                    new_record_ref,
                    &[vm_ctx_param, type_name_ptr, type_name_len],
                );
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // NewUnion: Create a union (enum) value
            // r[a] = Union(tag=strings[b], payload=r[c])
            OpCode::NewUnion => {
                // r[b] is a REGISTER containing a JitString pointer (loaded via LoadK).
                // Extract the string data pointer and length from the JitString struct.
                // JitString layout: offset 8 = len (i64), offset 32 = ptr (*mut u8).
                let jit_str_ptr = use_var(&mut builder, &regs, &var_types, inst.b);
                let flags = MemFlags::new();
                let tag_len = builder.ins().load(types::I64, flags, jit_str_ptr, 8); // len
                let tag_ptr = builder.ins().load(types::I64, flags, jit_str_ptr, 32); // ptr
                let payload_ptr = use_var(&mut builder, &regs, &var_types, inst.c);
                let call = builder.ins().call(
                    union_new_ref,
                    &[vm_ctx_param, tag_ptr, tag_len, payload_ptr],
                );
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // IsVariant: Check if union has a specific variant tag
            // VM semantics: skip next instruction if r[a] is Union with tag=strings[bx].
            // The skipped instruction is typically a Jmp to the "not-matched" branch.
            // IMPORTANT: IsVariant must NOT write to r[a] — it preserves the union
            // value so that subsequent Unbox instructions can read it. The interpreter
            // uses instruction-skip semantics (ip += 1 if matched), while the JIT
            // stores the boolean condition directly in pending_test_value for the
            // immediately following Jmp to consume.
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
                let call = builder.ins().call(
                    union_match_ref,
                    &[vm_ctx_param, union_ptr, tag_ptr, tag_len],
                );
                let match_result = builder.inst_results(call)[0]; // payload or -1
                                                                  // Check if matched: UNION_NO_MATCH = -1 (0xFFFF_FFFF_FFFF_FFFF)
                let no_match_val = builder.ins().iconst(types::I64, -1i64);
                let is_match = builder
                    .ins()
                    .icmp(IntCC::NotEqual, match_result, no_match_val);
                let nan_true = builder.ins().iconst(types::I64, NAN_BOX_TRUE);
                let nan_false = builder.ins().iconst(types::I64, NAN_BOX_FALSE);
                let result = builder.ins().select(is_match, nan_true, nan_false);
                // Store the condition directly — do NOT write to r[a].
                pending_test_value = Some(result);
                // Cache the payload in a Cranelift Variable (survives across
                // basic block boundaries created by the Jmp that follows).
                builder.def_var(union_match_cache_var, match_result);
                last_union_match_reg = Some(inst.a);
            }

            // Unbox: Extract payload from union
            // r[a] = payload of Union in r[b]
            OpCode::Unbox => {
                // If the preceding IsVariant already extracted the payload via
                // the combined union_match helper, reuse it — no second call.
                let result = if last_union_match_reg == Some(inst.b) {
                    last_union_match_reg = None;
                    builder.use_var(union_match_cache_var)
                } else {
                    let union_ptr = use_var(&mut builder, &regs, &var_types, inst.b);
                    let call = builder
                        .ins()
                        .call(union_unbox_ref, &[vm_ctx_param, union_ptr]);
                    builder.inst_results(call)[0]
                };
                // Unboxed payload is NaN-boxed by value_to_nanbox: could be
                // Int, Bool, Null, or a heap pointer (Tuple, Union, etc.).
                // Use Ptr as a conservative type — truthiness check against
                // NAN_BOX_NULL works for all NaN-boxed representations.
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // Intrinsic (builtin function call)
            OpCode::Intrinsic => {
                let intrinsic_id = inst.b as u32;
                let arg_base = inst.c;

                match intrinsic_id {
                    // -------------------------------------------------------
                    // 24: Append(list, elem)
                    // -------------------------------------------------------
                    24 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let elem_raw = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let elem_ty = var_types
                            .get(&((arg_base + 1) as u32))
                            .copied()
                            .unwrap_or(JitVarType::Int);
                        let call = match elem_ty {
                            JitVarType::Int | JitVarType::RawInt => {
                                let raw_int = ensure_raw_int(&mut builder, elem_raw, elem_ty);
                                builder
                                    .ins()
                                    .call(list_append_int_ref, &[vm_ctx_param, list, raw_int])
                            }
                            JitVarType::Float => builder
                                .ins()
                                .call(list_append_float_ref, &[vm_ctx_param, list, elem_raw]),
                            _ => {
                                let elem_nb = ensure_boxed_int(&mut builder, elem_raw, elem_ty);
                                builder
                                    .ins()
                                    .call(list_append_ref, &[vm_ctx_param, list, elem_nb])
                            }
                        };
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 25: Range(start, end)
                    // -------------------------------------------------------
                    25 => {
                        let start_raw = use_var(&mut builder, &regs, &var_types, arg_base);
                        let end_raw = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let start_ty = var_types
                            .get(&(arg_base as u32))
                            .copied()
                            .unwrap_or(JitVarType::Int);
                        let end_ty = var_types
                            .get(&((arg_base + 1) as u32))
                            .copied()
                            .unwrap_or(JitVarType::Int);
                        let start_nb = ensure_boxed_int(&mut builder, start_raw, start_ty);
                        let end_nb = ensure_boxed_int(&mut builder, end_raw, end_ty);
                        let call = builder
                            .ins()
                            .call(range_ref, &[vm_ctx_param, start_nb, end_nb]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 29 / 129: Sort / SortAsc
                    // -------------------------------------------------------
                    29 | 129 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let call = builder.ins().call(sort_ref, &[vm_ctx_param, list]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 130: SortDesc
                    // -------------------------------------------------------
                    130 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let call = builder.ins().call(sort_desc_ref, &[vm_ctx_param, list]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 30: Reverse
                    // -------------------------------------------------------
                    30 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let call = builder.ins().call(list_reverse_ref, &[vm_ctx_param, list]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 31: Map(list, fn)
                    // -------------------------------------------------------
                    31 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let closure = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder
                            .ins()
                            .call(hof_map_ref, &[vm_ctx_param, list, closure]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 32: Filter(list, fn)
                    // -------------------------------------------------------
                    32 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let closure = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder
                            .ins()
                            .call(hof_filter_ref, &[vm_ctx_param, list, closure]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 33: Reduce(list, fn, init)
                    // -------------------------------------------------------
                    33 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let closure = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let init = use_var(&mut builder, &regs, &var_types, arg_base + 2);
                        let call = builder
                            .ins()
                            .call(hof_reduce_ref, &[vm_ctx_param, list, closure, init]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 34: FlatMap(list, fn)
                    // -------------------------------------------------------
                    34 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let closure = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder
                            .ins()
                            .call(hof_flat_map_ref, &[vm_ctx_param, list, closure]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 37: Any(list, fn)
                    // -------------------------------------------------------
                    37 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let closure = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder
                            .ins()
                            .call(hof_any_ref, &[vm_ctx_param, list, closure]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Bool);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 38: All(list, fn)
                    // -------------------------------------------------------
                    38 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let closure = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder
                            .ins()
                            .call(hof_all_ref, &[vm_ctx_param, list, closure]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Bool);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 39: Find(list, fn)
                    // -------------------------------------------------------
                    39 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let closure = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder
                            .ins()
                            .call(hof_find_ref, &[vm_ctx_param, list, closure]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 40: Position(list, fn)
                    // -------------------------------------------------------
                    40 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let closure = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder
                            .ins()
                            .call(hof_position_ref, &[vm_ctx_param, list, closure]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Int);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 41: GroupBy(list, fn)
                    // -------------------------------------------------------
                    41 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let closure = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder
                            .ins()
                            .call(hof_group_by_ref, &[vm_ctx_param, list, closure]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 131: SortBy(list, fn)
                    // -------------------------------------------------------
                    131 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let closure = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder
                            .ins()
                            .call(hof_sort_by_ref, &[vm_ctx_param, list, closure]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 44: Flatten
                    // -------------------------------------------------------
                    44 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let call = builder.ins().call(list_flatten_ref, &[vm_ctx_param, list]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 45: Unique
                    // -------------------------------------------------------
                    45 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let call = builder.ins().call(list_unique_ref, &[vm_ctx_param, list]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 46: Take
                    // -------------------------------------------------------
                    46 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let n_raw = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let n_ty = var_types
                            .get(&((arg_base + 1) as u32))
                            .copied()
                            .unwrap_or(JitVarType::Int);
                        let n_nb = ensure_boxed_int(&mut builder, n_raw, n_ty);
                        let call = builder
                            .ins()
                            .call(list_take_ref, &[vm_ctx_param, list, n_nb]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 47: Drop
                    // -------------------------------------------------------
                    47 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let n_raw = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let n_ty = var_types
                            .get(&((arg_base + 1) as u32))
                            .copied()
                            .unwrap_or(JitVarType::Int);
                        let n_nb = ensure_boxed_int(&mut builder, n_raw, n_ty);
                        let call = builder
                            .ins()
                            .call(list_drop_ref, &[vm_ctx_param, list, n_nb]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 48: First
                    // -------------------------------------------------------
                    48 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let call = builder.ins().call(list_first_ref, &[vm_ctx_param, list]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 49: Last
                    // -------------------------------------------------------
                    49 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let call = builder.ins().call(list_last_ref, &[vm_ctx_param, list]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 0 / 1 / 72: Length / Count / Size
                    // -------------------------------------------------------
                    0 | 1 | 72 => {
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Str) {
                            // String type - use string_len
                            let str_ptr = use_var(&mut builder, &regs, &var_types, arg_base);
                            let call = builder
                                .ins()
                                .call(intrinsic_string_len_ref, &[vm_ctx_param, str_ptr]);
                            let raw_len = builder.inst_results(call)[0];
                            // string_len returns raw i64 — NaN-box it
                            let result = emit_box_int(&mut builder, raw_len);
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &mut regs, inst.a, result);
                        } else {
                            // Collection type (List/Map/Set/Tuple) - use collection_len
                            let value_ptr = use_var(&mut builder, &regs, &var_types, arg_base);
                            let call = builder
                                .ins()
                                .call(collection_len_ref, &[vm_ctx_param, value_ptr]);
                            let raw_len = builder.inst_results(call)[0];
                            // collection_len returns raw i64 — NaN-box it
                            let result = emit_box_int(&mut builder, raw_len);
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &mut regs, inst.a, result);
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
                                builder
                                    .ins()
                                    .call(intrinsic_print_str_ref, &[vm_ctx_param, v]);
                            }
                            Some(JitVarType::Float) => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                // Unbox float: bitcast I64 → F64 for the C function
                                let fv = emit_unbox_float(&mut builder, v);
                                builder
                                    .ins()
                                    .call(intrinsic_print_float_ref, &[vm_ctx_param, fv]);
                            }
                            _ => {
                                // Int or unknown — unbox int before printing
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let unboxed = emit_unbox_int(&mut builder, v);
                                builder
                                    .ins()
                                    .call(intrinsic_print_int_ref, &[vm_ctx_param, unboxed]);
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
                                let call = builder
                                    .ins()
                                    .call(intrinsic_to_string_float_ref, &[vm_ctx_param, fv]);
                                let result = builder.inst_results(call)[0];
                                var_types.insert(inst.a as u32, JitVarType::Str);
                                def_var(&mut builder, &mut regs, inst.a, result);
                            }
                            _ => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let unboxed = emit_unbox_int(&mut builder, v);
                                let call = builder
                                    .ins()
                                    .call(intrinsic_to_string_int_ref, &[vm_ctx_param, unboxed]);
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
                                let call = builder
                                    .ins()
                                    .call(intrinsic_to_int_from_float_ref, &[vm_ctx_param, fv]);
                                let raw_result = builder.inst_results(call)[0];
                                // to_int_from_float returns raw i64, NaN-box it
                                let result = emit_box_int(&mut builder, raw_result);
                                var_types.insert(inst.a as u32, JitVarType::Int);
                                def_var(&mut builder, &mut regs, inst.a, result);
                            }
                            Some(JitVarType::Str) => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let call = builder
                                    .ins()
                                    .call(intrinsic_to_int_from_string_ref, &[vm_ctx_param, v]);
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
                                let call = builder.ins().call(
                                    intrinsic_to_float_from_int_ref,
                                    &[vm_ctx_param, unboxed],
                                );
                                let f64_result = builder.inst_results(call)[0];
                                let result = emit_box_float(&mut builder, f64_result);
                                var_types.insert(inst.a as u32, JitVarType::Float);
                                def_var(&mut builder, &mut regs, inst.a, result);
                            }
                            Some(JitVarType::Str) => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let call = builder
                                    .ins()
                                    .call(intrinsic_to_float_from_string_ref, &[vm_ctx_param, v]);
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
                            Some(JitVarType::Ptr) => b"Object",
                            Some(JitVarType::RawInt) => b"Int",
                            None => b"Unknown",
                        };
                        // Inline JitString allocation for the type name.
                        let len = type_str.len() as i64;
                        let char_count = type_str.len() as i64; // ASCII type names
                        let flags = MemFlags::new();

                        let struct_size = builder.ins().iconst(types::I64, 40);
                        let struct_call =
                            builder.ins().call(malloc_ref, &[vm_ctx_param, struct_size]);
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
                        let data_call = builder
                            .ins()
                            .call(alloc_bytes_ref, &[vm_ctx_param, len_val]);
                        let data_ptr = builder.inst_results(data_call)[0];
                        let src_ptr = builder.ins().iconst(types::I64, type_str.as_ptr() as i64);
                        builder
                            .ins()
                            .call(memcpy_ref, &[vm_ctx_param, data_ptr, src_ptr, len_val]);
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
                            let call = builder
                                .ins()
                                .call(intrinsic_string_len_ref, &[vm_ctx_param, str_ptr]);
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
                                .call(intrinsic_pow_float_ref, &[vm_ctx_param, base_f, exp_f]);
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
                            let call = builder
                                .ins()
                                .call(intrinsic_pow_int_ref, &[vm_ctx_param, base_i, exp_i]);
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
                        let call = builder.ins().call(intrinsic_log_ref, &[vm_ctx_param, fv]);
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
                        let call = builder.ins().call(intrinsic_sin_ref, &[vm_ctx_param, fv]);
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
                        let call = builder.ins().call(intrinsic_cos_ref, &[vm_ctx_param, fv]);
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
                        let call = builder.ins().call(intrinsic_tan_ref, &[vm_ctx_param, fv]);
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
                                builder
                                    .ins()
                                    .call(intrinsic_print_str_ref, &[vm_ctx_param, v]);
                            }
                            Some(JitVarType::Float) => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let fv = emit_unbox_float(&mut builder, v);
                                builder
                                    .ins()
                                    .call(intrinsic_print_float_ref, &[vm_ctx_param, fv]);
                            }
                            _ => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let unboxed = emit_unbox_int(&mut builder, v);
                                builder
                                    .ins()
                                    .call(intrinsic_print_int_ref, &[vm_ctx_param, unboxed]);
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
                        let call = builder.ins().call(intrinsic_log2_ref, &[vm_ctx_param, fv]);
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
                        let call = builder.ins().call(intrinsic_log10_ref, &[vm_ctx_param, fv]);
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
                                let call = builder
                                    .ins()
                                    .call(intrinsic_to_string_float_ref, &[vm_ctx_param, fv]);
                                let result = builder.inst_results(call)[0];
                                var_types.insert(inst.a as u32, JitVarType::Str);
                                def_var(&mut builder, &mut regs, inst.a, result);
                            }
                            _ => {
                                // Int or unknown — unbox int, convert to string
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let unboxed = emit_unbox_int(&mut builder, v);
                                let call = builder
                                    .ins()
                                    .call(intrinsic_to_string_int_ref, &[vm_ctx_param, unboxed]);
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
                        let call = builder
                            .ins()
                            .call(intrinsic_string_contains_ref, &[vm_ctx_param, a, b]);
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
                        let call = builder
                            .ins()
                            .call(intrinsic_string_trim_ref, &[vm_ctx_param, v]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Str);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 20: Upper(str) -> String
                    // -------------------------------------------------------
                    20 => {
                        let v = use_var(&mut builder, &regs, &var_types, arg_base);
                        let call = builder
                            .ins()
                            .call(intrinsic_string_upper_ref, &[vm_ctx_param, v]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Str);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 21: Lower(str) -> String
                    // -------------------------------------------------------
                    21 => {
                        let v = use_var(&mut builder, &regs, &var_types, arg_base);
                        let call = builder
                            .ins()
                            .call(intrinsic_string_lower_ref, &[vm_ctx_param, v]);
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
                        let call = builder
                            .ins()
                            .call(intrinsic_string_replace_ref, &[vm_ctx_param, a, b, c]);
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
                        let call = builder
                            .ins()
                            .call(intrinsic_string_slice_ref, &[vm_ctx_param, a, b, c]);
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
                            .call(intrinsic_string_starts_with_ref, &[vm_ctx_param, a, b]);
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
                        let call = builder
                            .ins()
                            .call(intrinsic_string_ends_with_ref, &[vm_ctx_param, a, b]);
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
                        let call = builder
                            .ins()
                            .call(intrinsic_string_index_of_ref, &[vm_ctx_param, a, b]);
                        let raw = builder.inst_results(call)[0];
                        let result = emit_box_int(&mut builder, raw);
                        var_types.insert(inst.a as u32, JitVarType::Bool);
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
                        let call = builder
                            .ins()
                            .call(intrinsic_string_pad_left_ref, &[vm_ctx_param, a, b]);
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
                        let call = builder
                            .ins()
                            .call(intrinsic_string_pad_right_ref, &[vm_ctx_param, a, b]);
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
                                let call = builder
                                    .ins()
                                    .call(intrinsic_string_len_ref, &[vm_ctx_param, v]);
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
                                let call = builder
                                    .ins()
                                    .call(intrinsic_to_string_float_ref, &[vm_ctx_param, fv]);
                                builder.inst_results(call)[0]
                            }
                            _ => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let unboxed = emit_unbox_int(&mut builder, v);
                                let call = builder
                                    .ins()
                                    .call(intrinsic_to_string_int_ref, &[vm_ctx_param, unboxed]);
                                builder.inst_results(call)[0]
                            }
                        };
                        let call = builder
                            .ins()
                            .call(intrinsic_string_hash_ref, &[vm_ctx_param, str_val]);
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
                        let call = builder
                            .ins()
                            .call(intrinsic_string_split_ref, &[vm_ctx_param, a, b]);
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
                            let call = builder
                                .ins()
                                .call(intrinsic_string_len_ref, &[vm_ctx_param, v]);
                            let raw_len = builder.inst_results(call)[0];
                            emit_box_int(&mut builder, raw_len)
                        } else {
                            // All scalars are 8 bytes (i64 NaN-boxed)
                            builder.ins().iconst(types::I64, nan_box_int(8))
                        };
                        var_types.insert(inst.a as u32, JitVarType::Bool);
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
                                let call = builder
                                    .ins()
                                    .call(intrinsic_to_string_float_ref, &[vm_ctx_param, fv]);
                                let result = builder.inst_results(call)[0];
                                var_types.insert(inst.a as u32, JitVarType::Str);
                                def_var(&mut builder, &mut regs, inst.a, result);
                            }
                            _ => {
                                let v = use_var(&mut builder, &regs, &var_types, arg_base);
                                let unboxed = emit_unbox_int(&mut builder, v);
                                let call = builder
                                    .ins()
                                    .call(intrinsic_to_string_int_ref, &[vm_ctx_param, unboxed]);
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
                        let call = builder.ins().call(intrinsic_hrtime_ref, &[vm_ctx_param]);
                        let raw = builder.inst_results(call)[0];
                        let result = emit_box_int(&mut builder, raw);
                        var_types.insert(inst.a as u32, JitVarType::Bool);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 71: Merge — merge two maps/records.
                    // Uses merge_take_a which takes ownership of a's Arc so
                    // Arc::make_mut can mutate in-place (O(log n)) instead of
                    // deep-copying the BTreeMap (O(n)) on every call.
                    // -------------------------------------------------------
                    71 => {
                        let a = use_var(&mut builder, &regs, &var_types, arg_base);
                        let b = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder.ins().call(merge_take_a_ref, &[vm_ctx_param, a, b]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 14: Keys — return list of keys
                    // -------------------------------------------------------
                    14 => {
                        let map = use_var(&mut builder, &regs, &var_types, arg_base);
                        let call = builder.ins().call(map_keys_ref, &[vm_ctx_param, map]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 15: Values — return list of values
                    // -------------------------------------------------------
                    15 => {
                        let map = use_var(&mut builder, &regs, &var_types, arg_base);
                        let call = builder.ins().call(map_values_ref, &[vm_ctx_param, map]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 70: HasKey — return Bool
                    // -------------------------------------------------------
                    70 => {
                        let map = use_var(&mut builder, &regs, &var_types, arg_base);
                        let key_raw = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let key_ty = var_types
                            .get(&((arg_base + 1) as u32))
                            .copied()
                            .unwrap_or(JitVarType::Int);
                        let key_nb = if key_ty == JitVarType::Str {
                            let call = builder.ins().call(str_to_nb_ref, &[vm_ctx_param, key_raw]);
                            builder.inst_results(call)[0]
                        } else {
                            ensure_boxed_int(&mut builder, key_raw, key_ty)
                        };
                        let call = builder
                            .ins()
                            .call(map_has_key_ref, &[vm_ctx_param, map, key_nb]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Bool);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 74: Remove — remove key from map
                    // -------------------------------------------------------
                    74 => {
                        let map = use_var(&mut builder, &regs, &var_types, arg_base);
                        let key_raw = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let key_ty = var_types
                            .get(&((arg_base + 1) as u32))
                            .copied()
                            .unwrap_or(JitVarType::Int);
                        let key_nb = if key_ty == JitVarType::Str {
                            let call = builder.ins().call(str_to_nb_ref, &[vm_ctx_param, key_raw]);
                            builder.inst_results(call)[0]
                        } else {
                            ensure_boxed_int(&mut builder, key_raw, key_ty)
                        };
                        let call = builder
                            .ins()
                            .call(map_remove_ref, &[vm_ctx_param, map, key_nb]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 75: Entries — list of key/value tuples
                    // -------------------------------------------------------
                    75 => {
                        let map = use_var(&mut builder, &regs, &var_types, arg_base);
                        let call = builder.ins().call(map_entries_ref, &[vm_ctx_param, map]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 120: MapSortedKeys — return sorted key list
                    // -------------------------------------------------------
                    120 => {
                        let map = use_var(&mut builder, &regs, &var_types, arg_base);
                        let call = builder
                            .ins()
                            .call(map_sorted_keys_ref, &[vm_ctx_param, map]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // Phase 1e: set/collection ops
                    // -------------------------------------------------------

                    // 69: ToSet(list) -> set
                    69 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let call = builder.ins().call(to_set_ref, &[vm_ctx_param, list]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // 73: Add(set, elem) -> set
                    73 => {
                        let set = use_var(&mut builder, &regs, &var_types, arg_base);
                        let elem = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder.ins().call(set_add_ref, &[vm_ctx_param, set, elem]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // 51: Chars(str) -> list[String]
                    51 => {
                        let s = use_var(&mut builder, &regs, &var_types, arg_base);
                        let call = builder.ins().call(chars_ref, &[vm_ctx_param, s]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // 17: Join(list, sep) -> String
                    17 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let sep = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder.ins().call(join_ref, &[vm_ctx_param, list, sep]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // 35: Zip(a, b) -> list[tuple[T, U]]
                    35 => {
                        let a = use_var(&mut builder, &regs, &var_types, arg_base);
                        let b = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder.ins().call(zip_ref, &[vm_ctx_param, a, b]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // 36: Enumerate(list) -> list[tuple[Int, T]]
                    36 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let call = builder.ins().call(enumerate_ref, &[vm_ctx_param, list]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // 42: Chunk(list, n) -> list[list[T]]
                    42 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let n = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder.ins().call(chunk_ref, &[vm_ctx_param, list, n]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
                        def_var(&mut builder, &mut regs, inst.a, result);
                    }

                    // 43: Window(list, n) -> list[list[T]]
                    43 => {
                        let list = use_var(&mut builder, &regs, &var_types, arg_base);
                        let n = use_var(&mut builder, &regs, &var_types, arg_base + 1);
                        let call = builder.ins().call(window_ref, &[vm_ctx_param, list, n]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Ptr);
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

            // Async / concurrency opcodes
            OpCode::Spawn => {
                // Spawn async task from cell proto index.
                // Instruction format: ABx (a=dest, bx=cell index)
                // Runtime helper signature:
                //   jit_rt_spawn(vm_ctx, cell_idx, args_ptr, arg_count) -> i64
                //
                let vm_ctx = vm_ctx_param;
                let cell_idx = builder.ins().iconst(types::I32, inst.bx() as i64);
                let args_ptr = builder.ins().iconst(pointer_type, 0);
                let arg_count = builder.ins().iconst(types::I32, 0);
                let call = builder
                    .ins()
                    .call(async_spawn_ref, &[vm_ctx, cell_idx, args_ptr, arg_count]);
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, result);
            }
            OpCode::Await => {
                // Await a future value.
                // Instruction format: ABC (a=dest, b=future_reg)
                // Runtime helper signature:
                //   jit_rt_await(vm_ctx, future_handle) -> i64
                //
                let vm_ctx = vm_ctx_param;
                let future_handle = use_var(&mut builder, &regs, &var_types, inst.b);
                let call = builder
                    .ins()
                    .call(async_await_ref, &[vm_ctx, future_handle]);
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // ---------------------------------------------------------------
            // Effect System Opcodes (Phase 0.2)
            // ---------------------------------------------------------------
            OpCode::Perform => {
                // Perform effect_name.operation(args)
                // Instruction format: Perform dest, effect_const_idx, operation_const_idx
                // Runtime helper signature:
                //   jit_rt_perform(vm_ctx, effect_id, operation_id, args_ptr, arg_count) -> i64
                //
                // We pass effect/op as string-table IDs. The helper will resolve the
                // names and dispatch through the fiber-based effect runtime.
                let perform_ref = declare_helper_func(
                    module,
                    &mut builder.func,
                    "jit_rt_perform",
                    &[
                        pointer_type,
                        types::I32,
                        types::I32,
                        pointer_type,
                        pointer_type,
                    ],
                    &[types::I64],
                )?;

                let vm_ctx = vm_ctx_param;
                let effect_id =
                    if let Some(Constant::String(s)) = cell.constants.get(inst.b as usize) {
                        let id = string_table.iter().position(|item| item == s).unwrap_or(0) as i64;
                        builder.ins().iconst(types::I32, id)
                    } else {
                        builder.ins().iconst(types::I32, 0)
                    };
                let operation_id =
                    if let Some(Constant::String(s)) = cell.constants.get(inst.c as usize) {
                        let id = string_table.iter().position(|item| item == s).unwrap_or(0) as i64;
                        builder.ins().iconst(types::I32, id)
                    } else {
                        builder.ins().iconst(types::I32, 0)
                    };

                let arg_ptr = if (inst.a + 1) < cell.registers {
                    let arg_reg = inst.a + 1;
                    let arg_val = use_var(&mut builder, &regs, &var_types, arg_reg);
                    let arg_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        8,
                        8,
                    ));
                    let arg_ptr = builder.ins().stack_addr(pointer_type, arg_slot, 0);
                    builder.ins().store(MemFlags::new(), arg_val, arg_ptr, 0);
                    arg_ptr
                } else {
                    builder.ins().iconst(pointer_type, 0)
                };
                let arg_count = if (inst.a + 1) < cell.registers {
                    builder.ins().iconst(pointer_type, 1)
                } else {
                    builder.ins().iconst(pointer_type, 0)
                };

                let call = builder.ins().call(
                    perform_ref,
                    &[vm_ctx, effect_id, operation_id, arg_ptr, arg_count],
                );
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            OpCode::HandlePush => {
                // HandlePush meta_idx, offset
                // Instruction format: ABX (a=meta_idx, bx=offset to handler code)
                // Runtime helper signature:
                //   jit_rt_handle_push(vm_ctx, effect_id, operation_id)
                let handle_push_ref = declare_helper_func(
                    module,
                    &mut builder.func,
                    "jit_rt_handle_push",
                    &[pointer_type, types::I32, types::I32],
                    &[],
                )?;

                let vm_ctx = vm_ctx_param;
                let meta_idx = inst.a as usize;
                let (effect_id, operation_id) = if meta_idx < cell.effect_handler_metas.len() {
                    let meta = &cell.effect_handler_metas[meta_idx];
                    let eff_id = string_table
                        .iter()
                        .position(|item| item == &meta.effect_name)
                        .unwrap_or(0) as i64;
                    let op_id = string_table
                        .iter()
                        .position(|item| item == &meta.operation)
                        .unwrap_or(0) as i64;
                    (
                        builder.ins().iconst(types::I32, eff_id),
                        builder.ins().iconst(types::I32, op_id),
                    )
                } else {
                    (
                        builder.ins().iconst(types::I32, 0),
                        builder.ins().iconst(types::I32, 0),
                    )
                };

                builder
                    .ins()
                    .call(handle_push_ref, &[vm_ctx, effect_id, operation_id]);
            }

            OpCode::HandlePop => {
                // HandlePop
                // Runtime helper signature:
                //   jit_rt_handle_pop(vm_ctx)
                let handle_pop_ref = declare_helper_func(
                    module,
                    &mut builder.func,
                    "jit_rt_handle_pop",
                    &[pointer_type],
                    &[],
                )?;

                let vm_ctx = vm_ctx_param;
                builder.ins().call(handle_pop_ref, &[vm_ctx]);
            }

            OpCode::Resume => {
                // Resume dest, value_reg
                // Instruction format: ABC (a=dest, b=value_reg)
                // Runtime helper signature:
                //   jit_rt_resume(vm_ctx, value) -> i64
                let resume_ref = declare_helper_func(
                    module,
                    &mut builder.func,
                    "jit_rt_resume",
                    &[pointer_type, types::I64],
                    &[types::I64],
                )?;

                let vm_ctx = vm_ctx_param;
                let value = use_var(&mut builder, &regs, &var_types, inst.b);
                let call = builder.ins().call(resume_ref, &[vm_ctx, value]);
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // ---------------------------------------------------------------
            // Iteration Opcodes (Phase 0.5)
            // ---------------------------------------------------------------
            OpCode::Loop => {
                // Loop counter_reg, decrement_imm, jump_offset
                // Instruction format: A = counter register, sB = signed jump offset
                // VM semantics: decrement R[A] by 1, jump back if R[A] > 0
                //
                // Native Cranelift: unbox counter, subtract 1, re-box, compare > 0, branch.
                // Eliminates jit_rt_loop runtime call per iteration.
                let counter_boxed = use_var(&mut builder, &regs, &var_types, inst.a);
                let raw_counter = emit_unbox_int(&mut builder, counter_boxed);
                let one = builder.ins().iconst(types::I64, 1);
                let new_raw = builder.ins().isub(raw_counter, one);

                // Re-box and store the new counter
                let new_boxed = emit_box_int(&mut builder, new_raw);
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, new_boxed);

                // Test if new counter > 0
                let zero = builder.ins().iconst(types::I64, 0);
                let is_positive = builder.ins().icmp(IntCC::SignedGreaterThan, new_raw, zero);

                // Compute jump target
                let offset = inst.sbx() as i32;
                let target_pc = (pc as i32 + 1 + offset) as usize;
                let fallthrough_pc = pc + 1;

                let target_block = get_or_create_block(&mut builder, &mut block_map, target_pc);
                let fallthrough_block =
                    get_or_create_block(&mut builder, &mut block_map, fallthrough_pc);

                builder
                    .ins()
                    .brif(is_positive, target_block, &[], fallthrough_block, &[]);
                terminated = true;
            }

            OpCode::ForPrep => {
                // ForPrep base_reg, skip_offset
                // Instruction format: A = base register, sB = signed jump offset
                // JIT semantics (numeric for-loop layout):
                //   - R[A]   = init
                //   - R[A+1] = limit
                //   - R[A+2] = step
                //   - Store init as counter in R[A]
                //   - If loop is already exhausted, jump forward by sB
                //
                // Native Cranelift: no runtime call needed.
                // jit_rt_for_prep just returned init unchanged.
                let init_boxed = use_var(&mut builder, &regs, &var_types, inst.a);
                let limit_boxed = use_var(&mut builder, &regs, &var_types, inst.a + 1);
                let step_boxed = use_var(&mut builder, &regs, &var_types, inst.a + 2);
                let init = emit_unbox_int(&mut builder, init_boxed);
                let limit = emit_unbox_int(&mut builder, limit_boxed);
                let step = emit_unbox_int(&mut builder, step_boxed);

                // ForPrep: initial counter = init (no runtime call needed)
                let initial_counter = init;

                // Store initial counter at A (NaN-boxed)
                let boxed_counter = emit_box_int(&mut builder, initial_counter);
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, boxed_counter);

                // Skip loop if already exhausted:
                // step == 0 => skip
                // step > 0  => skip if init >= limit
                // step < 0  => skip if init <= limit
                let zero = builder.ins().iconst(types::I64, 0);
                let step_is_zero = builder.ins().icmp(IntCC::Equal, step, zero);
                let step_is_pos = builder.ins().icmp(IntCC::SignedGreaterThan, step, zero);
                let step_is_neg = builder.ins().icmp(IntCC::SignedLessThan, step, zero);
                let init_ge_limit =
                    builder
                        .ins()
                        .icmp(IntCC::SignedGreaterThanOrEqual, init, limit);
                let init_le_limit = builder
                    .ins()
                    .icmp(IntCC::SignedLessThanOrEqual, init, limit);
                let skip_pos = builder.ins().band(step_is_pos, init_ge_limit);
                let skip_neg = builder.ins().band(step_is_neg, init_le_limit);
                let skip_temp = builder.ins().bor(skip_pos, skip_neg);
                let should_skip = builder.ins().bor(step_is_zero, skip_temp);

                let skip_offset = inst.sbx() as i32;
                let skip_pc = (pc as i32 + 1 + skip_offset) as usize;
                let continue_pc = pc + 1;

                let skip_block = get_or_create_block(&mut builder, &mut block_map, skip_pc);
                let continue_block = get_or_create_block(&mut builder, &mut block_map, continue_pc);

                builder
                    .ins()
                    .brif(should_skip, skip_block, &[], continue_block, &[]);
                terminated = true;
            }

            OpCode::ForLoop => {
                // ForLoop base_reg, back_jump_offset
                // Instruction format: A = base register, sB = signed jump offset
                // JIT semantics (numeric for-loop layout):
                //   - R[A]   = counter
                //   - R[A+1] = limit
                //   - R[A+2] = step
                //   - Increment counter by step; if past limit, exit loop
                //
                // Native Cranelift: counter += step, compare to limit, branch.
                // Eliminates jit_rt_for_loop runtime call per iteration.
                let counter_boxed = use_var(&mut builder, &regs, &var_types, inst.a);
                let limit_boxed = use_var(&mut builder, &regs, &var_types, inst.a + 1);
                let step_boxed = use_var(&mut builder, &regs, &var_types, inst.a + 2);
                let counter = emit_unbox_int(&mut builder, counter_boxed);
                let limit = emit_unbox_int(&mut builder, limit_boxed);
                let step = emit_unbox_int(&mut builder, step_boxed);

                // next = counter + step
                let next = builder.ins().iadd(counter, step);

                // Exhaustion check:
                // For positive step: exhausted if next >= limit
                // For negative step: exhausted if next <= limit
                // For zero step: always exhausted
                // Since most for loops have positive step (step=1), we optimize:
                // We replicate the runtime helper logic exactly:
                //   is_exhausted = (step > 0 && next >= limit)
                //                  || (step < 0 && next <= limit)
                //                  || (step == 0)
                let zero = builder.ins().iconst(types::I64, 0);
                let step_is_pos = builder.ins().icmp(IntCC::SignedGreaterThan, step, zero);
                let step_is_neg = builder.ins().icmp(IntCC::SignedLessThan, step, zero);
                let step_is_zero = builder.ins().icmp(IntCC::Equal, step, zero);
                let next_ge_limit =
                    builder
                        .ins()
                        .icmp(IntCC::SignedGreaterThanOrEqual, next, limit);
                let next_le_limit = builder
                    .ins()
                    .icmp(IntCC::SignedLessThanOrEqual, next, limit);
                let exhaust_pos = builder.ins().band(step_is_pos, next_ge_limit);
                let exhaust_neg = builder.ins().band(step_is_neg, next_le_limit);
                let exhaust_temp = builder.ins().bor(exhaust_pos, exhaust_neg);
                let is_exhausted = builder.ins().bor(step_is_zero, exhaust_temp);

                // Store updated counter (if exhausted, keep previous counter)
                let selected = builder.ins().select(is_exhausted, counter, next);
                let boxed_result = emit_box_int(&mut builder, selected);
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, boxed_result);

                // Compute jump target
                let offset = inst.sbx() as i32;
                let loop_pc = (pc as i32 + 1 + offset) as usize;
                let exit_pc = pc + 1;

                let loop_block = get_or_create_block(&mut builder, &mut block_map, loop_pc);
                let exit_block = get_or_create_block(&mut builder, &mut block_map, exit_pc);

                builder
                    .ins()
                    .brif(is_exhausted, exit_block, &[], loop_block, &[]);
                terminated = true;
            }

            OpCode::ForIn => {
                // ForIn base, iterator_reg, element_dest
                // Instruction format: ABC (a=base, b=iterator_reg, c=element_dest)
                // VM semantics:
                //   - R[A+1] = current index
                //   - R[B] = iterator (collection)
                //   - R[C] = extracted element (output)
                //   - R[A] = Bool(has_more)
                //
                // Runtime helper signature:
                //   jit_rt_for_in(vm_ctx, iterator_ptr, index) -> i64 (element or NAN_BOX_NULL)
                let for_in_sig = module.make_signature();
                let for_in_fn = module
                    .declare_function("jit_rt_for_in", Linkage::Import, &{
                        let mut sig = for_in_sig;
                        sig.params.push(AbiParam::new(pointer_type)); // vm_ctx
                        sig.params.push(AbiParam::new(types::I64)); // iterator (NaN-boxed ptr)
                        sig.params.push(AbiParam::new(types::I64)); // index (raw i64)
                        sig.returns.push(AbiParam::new(types::I64)); // element (NaN-boxed) or NAN_BOX_NULL
                        sig
                    })
                    .map_err(|e| CodegenError::LoweringError(e.to_string()))?;
                let for_in_ref = module.declare_func_in_func(for_in_fn, &mut builder.func);

                let vm_ctx = vm_ctx_param;
                let iterator = use_var(&mut builder, &regs, &var_types, inst.b);
                let index_boxed = use_var(&mut builder, &regs, &var_types, inst.a + 1);
                let index = emit_unbox_int(&mut builder, index_boxed);

                let call = builder.ins().call(for_in_ref, &[vm_ctx, iterator, index]);
                let element = builder.inst_results(call)[0];

                // Store element at dest register C
                var_types.insert(inst.c as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.c, element);

                // has_more = element != NAN_BOX_NULL
                let nan_box_null = builder.ins().iconst(types::I64, NAN_BOX_NULL);
                let has_more_raw = builder.ins().icmp(IntCC::NotEqual, element, nan_box_null);
                let true_val = builder.ins().iconst(types::I64, NAN_BOX_TRUE);
                let false_val = builder.ins().iconst(types::I64, NAN_BOX_FALSE);
                let has_more = builder.ins().select(has_more_raw, true_val, false_val);
                var_types.insert(inst.a as u32, JitVarType::Bool);
                def_var(&mut builder, &mut regs, inst.a, has_more);

                // Increment index at A+1
                let one = builder.ins().iconst(types::I64, 1);
                let new_index = builder.ins().iadd(index, one);
                let boxed_new_index = emit_box_int(&mut builder, new_index);
                var_types.insert((inst.a + 1) as u32, JitVarType::Int);
                def_var(&mut builder, &mut regs, inst.a + 1, boxed_new_index);
            }

            // ---------------------------------------------------------------
            // Type and Membership Opcodes (Phase 0.6)
            // ---------------------------------------------------------------
            OpCode::In => {
                // In dest, needle_reg, haystack_reg
                // Instruction format: ABC (a=dest, b=needle_reg, c=haystack_reg)
                // Runtime helper signature:
                //   jit_rt_in(vm_ctx, value_ptr, collection_ptr) -> i64
                //
                // Returns NAN_BOX_TRUE if needle is in haystack, NAN_BOX_FALSE otherwise.
                let in_sig = module.make_signature();
                let in_fn = module
                    .declare_function("jit_rt_in", Linkage::Import, &{
                        let mut sig = in_sig;
                        sig.params.push(AbiParam::new(pointer_type)); // vm_ctx
                        sig.params.push(AbiParam::new(types::I64)); // value (NaN-boxed needle)
                        sig.params.push(AbiParam::new(types::I64)); // collection (NaN-boxed haystack)
                        sig.returns.push(AbiParam::new(types::I64)); // result (NAN_BOX_TRUE/FALSE)
                        sig
                    })
                    .map_err(|e| CodegenError::LoweringError(e.to_string()))?;
                let in_ref = module.declare_func_in_func(in_fn, &mut builder.func);

                let vm_ctx = vm_ctx_param;
                let needle = use_var(&mut builder, &regs, &var_types, inst.b);
                let haystack = use_var(&mut builder, &regs, &var_types, inst.c);
                let call = builder.ins().call(in_ref, &[vm_ctx, needle, haystack]);
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            OpCode::Is => {
                // Is dest, val_reg, type_name_reg
                // Instruction format: ABC (a=dest, b=val_reg, c=type_name_reg)
                //
                // Fast path: if the type name is a compile-time constant (the common
                // case — type names are always literals like "Tuple", "List", etc.),
                // pass raw bytes directly to jit_rt_is_type_name instead of using a
                // JitString pointer. This saves one malloc + string-drop per call.
                let vm_ctx = vm_ctx_param;
                let value = use_var(&mut builder, &regs, &var_types, inst.b);

                let result = if let Some(type_name) = is_pc_to_type_name.get(&pc) {
                    // Compile-time type name: embed raw bytes as constants.
                    let name_bytes = type_name.as_bytes();
                    let name_ptr = builder.ins().iconst(types::I64, name_bytes.as_ptr() as i64);
                    let name_len = builder.ins().iconst(types::I64, name_bytes.len() as i64);
                    let call = builder
                        .ins()
                        .call(is_type_name_ref, &[vm_ctx, value, name_ptr, name_len]);
                    builder.inst_results(call)[0]
                } else {
                    // Fallback: use JitString pointer (type name came from a variable).
                    let is_sig = module.make_signature();
                    let is_fn = module
                        .declare_function("jit_rt_is", Linkage::Import, &{
                            let mut sig = is_sig;
                            sig.params.push(AbiParam::new(pointer_type)); // vm_ctx
                            sig.params.push(AbiParam::new(types::I64)); // value (NaN-boxed)
                            sig.params.push(AbiParam::new(types::I64)); // type_id (JitString ptr)
                            sig.returns.push(AbiParam::new(types::I64)); // result
                            sig
                        })
                        .map_err(|e| CodegenError::LoweringError(e.to_string()))?;
                    let is_ref = module.declare_func_in_func(is_fn, &mut builder.func);
                    let type_id = use_var(&mut builder, &regs, &var_types, inst.c);
                    let call = builder.ins().call(is_ref, &[vm_ctx, value, type_id]);
                    builder.inst_results(call)[0]
                };
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            // ---------------------------------------------------------------
            // Tool System Opcodes (Phase 0.4)
            // ---------------------------------------------------------------
            OpCode::ToolCall => {
                // ToolCall dest, tool_id_const_idx
                // Instruction format: ABx (a=dest, bx=tool constant index)
                // Runtime helper signature:
                //   jit_rt_tool_call(vm_ctx, tool_id, args_map_ptr) -> i64
                //
                // The ToolCall lowering constructs an args map in the destination
                // register (dest) via NewMap immediately before ToolCall. The
                // runtime helper expects the args map pointer in that register.
                let vm_ctx = vm_ctx_param;
                let tool_id = builder.ins().iconst(types::I32, inst.bx() as i64);
                let args_map = use_var(&mut builder, &regs, &var_types, inst.a);
                let call = builder
                    .ins()
                    .call(tool_call_ref, &[vm_ctx, tool_id, args_map]);
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            OpCode::Schema => {
                // Schema value_reg, schema_id
                // Instruction format: ABx (a=value_reg, bx=schema_id)
                // Runtime helper signature:
                //   jit_rt_schema_validate(vm_ctx, value_ptr, schema_id) -> i64 (bool)
                let vm_ctx = vm_ctx_param;
                let value = use_var(&mut builder, &regs, &var_types, inst.a);
                let schema_id = builder.ins().iconst(types::I32, inst.bx() as i64);
                builder
                    .ins()
                    .call(schema_validate_ref, &[vm_ctx, value, schema_id]);
            }

            OpCode::Emit => {
                // Emit value_reg
                // Instruction format: A (a=value_reg to emit)
                // Runtime helper signature:
                //   jit_rt_emit(vm_ctx, value_ptr) -> i64 (returns null)
                // No result register needed; this is a side effect.
                let vm_ctx = vm_ctx_param;
                let value = use_var(&mut builder, &regs, &var_types, inst.a);
                builder.ins().call(emit_ref, &[vm_ctx, value]);
                // Emit has no destination register; it's a pure side effect.
            }

            OpCode::TraceRef => {
                // TraceRef dest
                // Instruction format: A (a=dest_reg)
                // Runtime helper signature:
                //   jit_rt_trace_ref(vm_ctx) -> i64
                let vm_ctx = vm_ctx_param;
                let call = builder.ins().call(trace_ref_ref, &[vm_ctx]);
                let result = builder.inst_results(call)[0];
                var_types.insert(inst.a as u32, JitVarType::Ptr);
                def_var(&mut builder, &mut regs, inst.a, result);
            }

            OpCode::Nop => {}

            // OsrCheck is a no-op in JIT-compiled code — the cell is already
            // compiled, so there's nothing to tier-up to.
            OpCode::OsrCheck => {}

            // All opcodes are expected to be handled above in Phase 0.7.
            _ => {
                return Err(CodegenError::LoweringError(format!(
                    "unhandled opcode in JIT lowering: {:?}",
                    inst.op
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
            // NaN-boxed integer 0 = NAN_MASK | (TAG_INT << 48) | 0 = 0x7FF9_0000_0000_0000
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
        JitVarType::Ptr => {
            // Null heap pointer sentinel
            builder.ins().iconst(types::I64, NAN_BOX_NULL)
        }
        JitVarType::RawInt => {
            // Raw i64 zero
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
        // NaN-box integers: NAN_MASK | (TAG_INT << 48) | (n & PAYLOAD_MASK)
        Constant::Int(n) => builder.ins().iconst(types::I64, nan_box_int(*n)),
        // Floats: raw f64 bits as i64 (NOT NaN-boxed)
        Constant::Float(f) => builder.ins().iconst(types::I64, nan_box_float(*f)),
        // NaN-box booleans: TAG_BOOL sentinels
        Constant::Bool(b) => {
            let sentinel = if *b { NAN_BOX_TRUE } else { NAN_BOX_FALSE };
            builder.ins().iconst(types::I64, sentinel)
        }
        // Pre-boxed NaN-boxed value stored directly.
        Constant::NbValue(raw) => builder.ins().iconst(types::I64, *raw as i64),
        // NaN-box null: TAG_NULL sentinel
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
/// Returns a set of **LoadK PCs** (not register numbers) whose string constants
/// are used exclusively as Call/TailCall callee names. The JIT can skip heap
/// allocation for these strings since they're only used for function dispatch.
fn identify_call_name_registers(cell: &LirCell) -> std::collections::HashSet<usize> {
    use std::collections::HashSet;

    let mut result: HashSet<usize> = HashSet::new();
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
            // Track by LoadK PC, not register number, so reused registers
            // don't incorrectly skip allocation for non-call string loads.
            result.insert(loadk_pc);
        }
    }

    result
}

/// Identify LoadK PCs whose string constants are only used as `Is` type-name
/// operands (register c of `Is` instructions). For those LoadKs we skip
/// JitString heap allocation — the type name is embedded as raw bytes in the
/// generated code. Returns both:
///   - `HashSet<usize>`: LoadK PCs to skip JitString allocation for
///   - `HashMap<usize, String>`: Is-instruction PCs → type name string
fn identify_is_type_name_registers(
    cell: &LirCell,
) -> (
    std::collections::HashSet<usize>,
    std::collections::HashMap<usize, String>,
) {
    use std::collections::{HashMap, HashSet};

    let mut skip_alloc: HashSet<usize> = HashSet::new();
    let mut is_type_map: HashMap<usize, String> = HashMap::new();
    let instructions = &cell.instructions;

    // First pass: find all `Is` instructions and the register they use as type_name (inst.c).
    // Trace each back through Move chains to find the defining LoadK.
    for (is_pc, is_inst) in instructions.iter().enumerate() {
        if is_inst.op != OpCode::Is {
            continue;
        }
        let type_name_reg = is_inst.c;

        // Walk backward from is_pc to find the LoadK that last defined type_name_reg.
        // Simple backward scan — stops at the most recent definition.
        let mut found_loadk_pc: Option<usize> = None;
        let mut current_reg = type_name_reg;

        'outer: for (scan_pc, scan_inst) in instructions[..is_pc].iter().enumerate().rev() {
            if scan_inst.a == current_reg {
                match scan_inst.op {
                    OpCode::LoadK => {
                        let bx = scan_inst.bx() as usize;
                        if let Some(Constant::String(s)) = cell.constants.get(bx) {
                            found_loadk_pc = Some(scan_pc);
                            is_type_map.insert(is_pc, s.clone());
                        }
                        break 'outer;
                    }
                    OpCode::Move | OpCode::MoveOwn => {
                        // Follow the move chain backward.
                        current_reg = scan_inst.b;
                    }
                    _ => break 'outer,
                }
            }
        }

        if let Some(loadk_pc) = found_loadk_pc {
            // Verify this LoadK's register is ONLY used as an Is type-name operand
            // and is never passed to string-consuming instructions.
            let origin_reg = instructions[loadk_pc].a;
            let mut aliases: HashSet<u16> = HashSet::new();
            aliases.insert(origin_reg);
            let mut only_is_use = false;
            let mut invalidated = false;

            for inst in &instructions[(loadk_pc + 1)..=is_pc] {
                match inst.op {
                    OpCode::Is => {
                        if aliases.contains(&inst.c) {
                            only_is_use = true;
                            aliases.remove(&inst.c);
                        }
                        // inst.b is the value operand — not a string use.
                    }
                    OpCode::Move | OpCode::MoveOwn => {
                        if aliases.contains(&inst.b) {
                            aliases.insert(inst.a);
                        } else if aliases.contains(&inst.a) {
                            aliases.remove(&inst.a);
                        }
                    }
                    OpCode::LoadK | OpCode::LoadBool | OpCode::LoadInt | OpCode::LoadNil => {
                        aliases.remove(&inst.a);
                    }
                    OpCode::Return => {
                        if aliases.contains(&inst.a) {
                            invalidated = true;
                        }
                        break;
                    }
                    _ => {
                        // Any other use of the string register invalidates the optimization.
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

            if only_is_use && !invalidated {
                skip_alloc.insert(loadk_pc);
            } else {
                // If invalidated, remove from is_type_map so we use the fallback path.
                is_type_map.remove(&is_pc);
            }
        }
    }

    (skip_alloc, is_type_map)
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
