//! Unified LIR-to-Cranelift IR lowering module.
//!
//! Provides a generic `lower_cell` function that works with both JITModule
//! and ObjectModule, abstracting over the `cranelift_module::Module` trait.
//!
//! This implementation supports all opcodes from the JIT path, including:
//! - Type-aware arithmetic (Int, Float, String)
//! - String runtime helpers (alloc, concat, clone, eq, cmp, drop)
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
    /// Heap-allocated refcounted string, represented as a `*mut JitString` cast to i64.
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
pub fn lower_cell<M: Module>(
    ctx: &mut Context,
    fb_ctx: &mut FunctionBuilderContext,
    cell: &LirCell,
    module: &mut M,
    pointer_type: ClifType,
    func_id: FuncId,
    func_ids: &HashMap<String, FuncId>,
    string_table: &[String],
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
    let value_clone_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_value_clone",
        &[types::I64],
        &[types::I64],
    )?;
    let value_drop_ref =
        declare_helper_func(module, &mut func, "jit_rt_value_drop", &[types::I64], &[])?;

    // Declare intrinsic runtime helper functions (for JIT builtin support).
    let intrinsic_print_int_ref =
        declare_helper_func(module, &mut func, "jit_rt_print_int", &[types::I64], &[])?;
    let intrinsic_print_float_ref =
        declare_helper_func(module, &mut func, "jit_rt_print_float", &[types::F64], &[])?;
    let intrinsic_print_bool_ref =
        declare_helper_func(module, &mut func, "jit_rt_print_bool", &[types::I64], &[])?;
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
    // Absolute value helpers
    let intrinsic_abs_float_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_abs_float",
        &[types::F64],
        &[types::F64],
    )?;
    let intrinsic_abs_int_ref = declare_helper_func(
        module,
        &mut func,
        "jit_rt_abs_int",
        &[types::I64],
        &[types::I64],
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

    // Suppress unused-variable warnings for helpers not yet used in all paths.
    let _ = value_clone_ref;
    let _ = value_drop_ref;
    let _ = intrinsic_abs_float_ref;
    let _ = intrinsic_abs_int_ref;
    let _ = intrinsic_tan_ref;
    let _ = intrinsic_print_bool_ref;

    let mut builder = FunctionBuilder::new(&mut func, fb_ctx);

    let num_regs = (cell.registers as usize)
        .max(cell.params.len())
        .clamp(1, MAX_REGS);
    let mut vars: Vec<Variable> = Vec::with_capacity(num_regs);

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
                    57 | 58 | 59 | 60 | 62 | 63 | 64 | 123 | 124 | 127 | 128 => {
                        // Round, Ceil, Floor, Sqrt, Log, Sin, Cos, Log2, Log10, MathPi, MathE
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

    // All Cranelift variables are declared as I64 (both ints and string pointers
    // are I64; only floats are F64). The semantic distinction is in var_types.
    for i in 0..num_regs {
        let var = Variable::from_u32(i as u32);
        let (var_ty, clif_ty) = if i < cell.params.len() {
            let param_ty_str = &cell.params[i].ty;
            if param_ty_str == "Float" {
                (JitVarType::Float, types::F64)
            } else if param_ty_str == "String" {
                (JitVarType::Str, types::I64)
            } else {
                (JitVarType::Int, types::I64)
            }
        } else if float_regs.contains(&(i as u16)) {
            (JitVarType::Float, types::F64)
        } else {
            // Both int and string regs use I64 at the Cranelift level.
            // The semantic type for string regs is set later when LoadK executes.
            (JitVarType::Int, types::I64)
        };
        builder.declare_var(var, clif_ty);
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

    // Initialize remaining registers to zero.
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
                    // Clone via inline refcount increment (3 instructions,
                    // no function call). JitString layout: refcount is at
                    // offset 0. We load it, add 1, store back. The result
                    // is the same pointer — both source and dest share the
                    // underlying string data.
                    let src = use_var(&mut builder, &vars, inst.b);
                    let flags = MemFlags::new();
                    let rc = builder.ins().load(types::I64, flags, src, 0);
                    let rc_plus_one = builder.ins().iadd_imm(rc, 1);
                    builder.ins().store(flags, rc_plus_one, src, 0);
                    src
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
                            Some(use_var(&mut builder, &vars, inst.a))
                        } else {
                            None
                        };

                        // Build a stack slot holding the array of string pointers.
                        let slot_size = (leaves.len() * 8) as u32;
                        let slot_data =
                            StackSlotData::new(StackSlotKind::ExplicitSlot, slot_size, 3);
                        let slot = builder.create_sized_stack_slot(slot_data);

                        for (i, &leaf_reg) in leaves.iter().enumerate() {
                            let val = use_var(&mut builder, &vars, leaf_reg);
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
                        def_var(&mut builder, &vars, inst.a, result);
                    } else {
                        // String concatenation (non-chain) — safe to read operands now.
                        let lhs = use_var(&mut builder, &vars, inst.b);
                        let rhs = use_var(&mut builder, &vars, inst.c);
                        let dest_ty = var_types
                            .get(&(inst.a as u32))
                            .copied()
                            .unwrap_or(JitVarType::Int);

                        // Optimization: if dest == lhs (a = a + c), use in-place mutation
                        if dest_ty == JitVarType::Str && inst.a == inst.b {
                            // In-place: a = a + c
                            // Use jit_rt_string_concat_mut which takes ownership of lhs
                            let call = builder.ins().call(str_concat_mut_ref, &[lhs, rhs]);
                            let result = builder.inst_results(call)[0];
                            var_types.insert(inst.a as u32, JitVarType::Str);
                            def_var(&mut builder, &vars, inst.a, result);
                            // Note: lhs is consumed by concat_mut, no need to drop
                        } else {
                            // Standard case: create new string
                            let old_dest = if dest_ty == JitVarType::Str
                                && inst.a != inst.b
                                && inst.a != inst.c
                            {
                                Some(use_var(&mut builder, &vars, inst.a))
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
                            def_var(&mut builder, &vars, inst.a, result);
                        }
                    }
                } else if lhs_ty == JitVarType::Float || rhs_ty == JitVarType::Float {
                    let lhs = use_var(&mut builder, &vars, inst.b);
                    let rhs = use_var(&mut builder, &vars, inst.c);
                    let res = builder.ins().fadd(lhs, rhs);
                    var_types.insert(inst.a as u32, JitVarType::Float);
                    def_var(&mut builder, &vars, inst.a, res);
                } else {
                    let lhs = use_var(&mut builder, &vars, inst.b);
                    let rhs = use_var(&mut builder, &vars, inst.c);
                    let res = builder.ins().iadd(lhs, rhs);
                    var_types.insert(inst.a as u32, JitVarType::Int);
                    def_var(&mut builder, &vars, inst.a, res);
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
                        Some(use_var(&mut builder, &vars, inst.a))
                    } else {
                        None
                    };

                    // Build a stack slot holding the array of string pointers.
                    let slot_size = (leaves.len() * 8) as u32;
                    let slot_data = StackSlotData::new(StackSlotKind::ExplicitSlot, slot_size, 3);
                    let slot = builder.create_sized_stack_slot(slot_data);

                    for (i, &leaf_reg) in leaves.iter().enumerate() {
                        let val = use_var(&mut builder, &vars, leaf_reg);
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
                    def_var(&mut builder, &vars, inst.a, result);
                } else {
                    // Non-chain: regular concat
                    let lhs = use_var(&mut builder, &vars, inst.b);
                    let rhs = use_var(&mut builder, &vars, inst.c);
                    let dest_ty = var_types
                        .get(&(inst.a as u32))
                        .copied()
                        .unwrap_or(JitVarType::Int);

                    // Optimization: if dest == lhs (a = a ++ c), use in-place mutation
                    if dest_ty == JitVarType::Str && inst.a == inst.b {
                        let call = builder.ins().call(str_concat_mut_ref, &[lhs, rhs]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Str);
                        def_var(&mut builder, &vars, inst.a, result);
                    } else {
                        // Drop old destination string if it exists and differs from operands.
                        let old_dest =
                            if dest_ty == JitVarType::Str && inst.a != inst.b && inst.a != inst.c {
                                Some(use_var(&mut builder, &vars, inst.a))
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
                        def_var(&mut builder, &vars, inst.a, result);
                    }
                }
            }

            OpCode::Sub => {
                let lhs = use_var(&mut builder, &vars, inst.b);
                let rhs = use_var(&mut builder, &vars, inst.c);
                let is_float = is_float_op(&var_types, inst.b, inst.c);
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
                let is_float = is_float_op(&var_types, inst.b, inst.c);
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
                let is_float = is_float_op(&var_types, inst.b, inst.c);
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
                let is_float = is_float_op(&var_types, inst.b, inst.c);
                let res = if is_float {
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
                let is_float = is_float_op(&var_types, inst.b, inst.c);
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

            // Comparison (type-aware)
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
                let target_pc = (pc as isize + 1 + offset as isize) as usize;
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
                // Drop all live string registers except the return value.
                let ret_reg = inst.a;
                for (&reg_id, &ty) in &var_types {
                    if ty == JitVarType::Str
                        && reg_id != ret_reg as u32
                        && (reg_id as usize) < vars.len()
                    {
                        let v = use_var(&mut builder, &vars, reg_id as u16);
                        builder.ins().call(str_drop_ref, &[v]);
                    }
                }
                let val = use_var(&mut builder, &vars, ret_reg);
                // ABI always returns I64. If the register holds a float, bitcast
                // the F64 bits into I64 so the caller can reconstruct via from_bits.
                let ret_val = if var_types.get(&(ret_reg as u32)) == Some(&JitVarType::Float) {
                    builder.ins().bitcast(types::I64, MemFlags::new(), val)
                } else {
                    val
                };
                builder.ins().return_(&[ret_val]);
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
                    let old = use_var(&mut builder, &vars, base);
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

                // Drop string-typed argument registers after the call.
                for arg_reg in str_arg_regs {
                    if (arg_reg as usize) < vars.len() {
                        let v = use_var(&mut builder, &vars, arg_reg as u16);
                        builder.ins().call(str_drop_ref, &[v]);
                    }
                    var_types.remove(&arg_reg);
                }

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

                // Drop the callee name string in base.
                if var_types.get(&(base as u32)) == Some(&JitVarType::Str) {
                    let old = use_var(&mut builder, &vars, base);
                    builder.ins().call(str_drop_ref, &[old]);
                    var_types.remove(&(base as u32));
                }

                // Drop any string-typed argument registers.
                for i in 0..num_args {
                    let arg_reg = (base + 1 + i as u16) as u32;
                    if var_types.get(&arg_reg) == Some(&JitVarType::Str) {
                        if (arg_reg as usize) < vars.len() {
                            let v = use_var(&mut builder, &vars, arg_reg as u16);
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
                                let arg_reg = base + 1 + i as u16;
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

            // Record field access
            OpCode::GetField => {
                // A = B.field[C]
                // B is the record register (pointer to Value containing a Record)
                // C is the field name index in the string table
                let record_ptr = use_var(&mut builder, &vars, inst.b);

                // Get the field name from the string table
                let field_name = if (inst.c as usize) < string_table.len() {
                    &string_table[inst.c as usize]
                } else {
                    ""
                };

                // Pass field name as raw pointer and length to the runtime helper
                let field_name_bytes = field_name.as_bytes();
                let field_name_ptr = builder
                    .ins()
                    .iconst(types::I64, field_name_bytes.as_ptr() as i64);
                let field_name_len = builder
                    .ins()
                    .iconst(types::I64, field_name_bytes.len() as i64);

                // Call jit_rt_record_get_field(record_ptr, field_name_ptr, field_name_len)
                let call = builder.ins().call(
                    record_get_field_ref,
                    &[record_ptr, field_name_ptr, field_name_len],
                );
                let result = builder.inst_results(call)[0];

                // The result is a boxed Value, store it
                var_types.insert(inst.a as u32, JitVarType::Int); // Represents a Value pointer
                def_var(&mut builder, &vars, inst.a, result);
            }
            OpCode::SetField => {
                // A.field[B] = C
                // A is the record register (pointer to Value containing a Record)
                // B is the field name index in the string table
                // C is the value to set (register containing the new value)
                let record_ptr = use_var(&mut builder, &vars, inst.a);
                let value_ptr = use_var(&mut builder, &vars, inst.c);

                // Get the field name from the string table
                let field_name = if (inst.b as usize) < string_table.len() {
                    &string_table[inst.b as usize]
                } else {
                    ""
                };

                // Pass field name as raw pointer and length to the runtime helper
                let field_name_bytes = field_name.as_bytes();
                let field_name_ptr = builder
                    .ins()
                    .iconst(types::I64, field_name_bytes.as_ptr() as i64);
                let field_name_len = builder
                    .ins()
                    .iconst(types::I64, field_name_bytes.len() as i64);

                // Call jit_rt_record_set_field(record_ptr, field_name_ptr, field_name_len, value_ptr)
                let call = builder.ins().call(
                    record_set_field_ref,
                    &[record_ptr, field_name_ptr, field_name_len, value_ptr],
                );
                let result = builder.inst_results(call)[0];

                // Update the record register with the new record (COW semantics)
                var_types.insert(inst.a as u32, JitVarType::Int); // Represents a Value pointer
                def_var(&mut builder, &vars, inst.a, result);
            }

            // Intrinsic (builtin function call)
            OpCode::Intrinsic => {
                // Instruction format: Intrinsic A, B, C
                // A = destination register for result
                // B = intrinsic ID (from IntrinsicId enum, 0-137)
                // C = base register for arguments

                let intrinsic_id = inst.b as u32;
                let arg_base = inst.c;

                // For now, implement a subset of common intrinsics.
                // Full implementation requires complex Value type support.
                match intrinsic_id {
                    // -------------------------------------------------------
                    // 0 / 1 / 72: Length / Count / Size
                    // -------------------------------------------------------
                    0 | 1 | 72 => {
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Str) {
                            let str_ptr = use_var(&mut builder, &vars, arg_base);
                            let call = builder.ins().call(intrinsic_string_len_ref, &[str_ptr]);
                            let result = builder.inst_results(call)[0];
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &vars, inst.a, result);
                        } else {
                            // For non-string types, return 0 (stub — collections not yet in JIT)
                            let zero = builder.ins().iconst(types::I64, 0);
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &vars, inst.a, zero);
                        }
                    }

                    // -------------------------------------------------------
                    // 9: Print
                    // -------------------------------------------------------
                    9 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        match arg_ty {
                            Some(JitVarType::Str) => {
                                let v = use_var(&mut builder, &vars, arg_base);
                                builder.ins().call(intrinsic_print_str_ref, &[v]);
                            }
                            Some(JitVarType::Float) => {
                                let v = use_var(&mut builder, &vars, arg_base);
                                builder.ins().call(intrinsic_print_float_ref, &[v]);
                            }
                            _ => {
                                // Int or unknown — print as int
                                let v = use_var(&mut builder, &vars, arg_base);
                                builder.ins().call(intrinsic_print_int_ref, &[v]);
                            }
                        }
                        let zero = builder.ins().iconst(types::I64, 0);
                        var_types.insert(inst.a as u32, JitVarType::Int);
                        def_var(&mut builder, &vars, inst.a, zero);
                    }

                    // -------------------------------------------------------
                    // 10: ToString
                    // -------------------------------------------------------
                    10 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        match arg_ty {
                            Some(JitVarType::Str) => {
                                // Already a string — clone it
                                let v = use_var(&mut builder, &vars, arg_base);
                                let call = builder.ins().call(str_clone_ref, &[v]);
                                let result = builder.inst_results(call)[0];
                                var_types.insert(inst.a as u32, JitVarType::Str);
                                def_var(&mut builder, &vars, inst.a, result);
                            }
                            Some(JitVarType::Float) => {
                                let v = use_var(&mut builder, &vars, arg_base);
                                let call = builder.ins().call(intrinsic_to_string_float_ref, &[v]);
                                let result = builder.inst_results(call)[0];
                                var_types.insert(inst.a as u32, JitVarType::Str);
                                def_var(&mut builder, &vars, inst.a, result);
                            }
                            _ => {
                                let v = use_var(&mut builder, &vars, arg_base);
                                let call = builder.ins().call(intrinsic_to_string_int_ref, &[v]);
                                let result = builder.inst_results(call)[0];
                                var_types.insert(inst.a as u32, JitVarType::Str);
                                def_var(&mut builder, &vars, inst.a, result);
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
                                let v = use_var(&mut builder, &vars, arg_base);
                                let call =
                                    builder.ins().call(intrinsic_to_int_from_float_ref, &[v]);
                                let result = builder.inst_results(call)[0];
                                var_types.insert(inst.a as u32, JitVarType::Int);
                                def_var(&mut builder, &vars, inst.a, result);
                            }
                            Some(JitVarType::Str) => {
                                let v = use_var(&mut builder, &vars, arg_base);
                                let call =
                                    builder.ins().call(intrinsic_to_int_from_string_ref, &[v]);
                                let result = builder.inst_results(call)[0];
                                var_types.insert(inst.a as u32, JitVarType::Int);
                                def_var(&mut builder, &vars, inst.a, result);
                            }
                            _ => {
                                // Already Int — pass through
                                let v = use_var(&mut builder, &vars, arg_base);
                                var_types.insert(inst.a as u32, JitVarType::Int);
                                def_var(&mut builder, &vars, inst.a, v);
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
                                let v = use_var(&mut builder, &vars, arg_base);
                                let call =
                                    builder.ins().call(intrinsic_to_float_from_int_ref, &[v]);
                                let result = builder.inst_results(call)[0];
                                var_types.insert(inst.a as u32, JitVarType::Float);
                                def_var(&mut builder, &vars, inst.a, result);
                            }
                            Some(JitVarType::Str) => {
                                let v = use_var(&mut builder, &vars, arg_base);
                                let call =
                                    builder.ins().call(intrinsic_to_float_from_string_ref, &[v]);
                                let result = builder.inst_results(call)[0];
                                var_types.insert(inst.a as u32, JitVarType::Float);
                                def_var(&mut builder, &vars, inst.a, result);
                            }
                            _ => {
                                // Already Float — pass through
                                let v = use_var(&mut builder, &vars, arg_base);
                                var_types.insert(inst.a as u32, JitVarType::Float);
                                def_var(&mut builder, &vars, inst.a, v);
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
                            Some(JitVarType::Str) => b"String",
                            None => b"Unknown",
                        };
                        // Create a string from the static type name
                        let ptr_val = builder.ins().iconst(types::I64, type_str.as_ptr() as i64);
                        let len_val = builder.ins().iconst(types::I64, type_str.len() as i64);
                        let call = builder.ins().call(str_alloc_ref, &[ptr_val, len_val]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Str);
                        def_var(&mut builder, &vars, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 26: Abs — pure Cranelift IR (no runtime helper)
                    // -------------------------------------------------------
                    26 => {
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            let v = use_var(&mut builder, &vars, arg_base);
                            let result = builder.ins().fabs(v);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &vars, inst.a, result);
                        } else {
                            // Integer abs: neg = ineg(x); cmp = icmp(slt, x, 0); select(cmp, neg, x)
                            let v = use_var(&mut builder, &vars, arg_base);
                            let neg = builder.ins().ineg(v);
                            let zero = builder.ins().iconst(types::I64, 0);
                            let is_neg = builder.ins().icmp(IntCC::SignedLessThan, v, zero);
                            let result = builder.ins().select(is_neg, neg, v);
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &vars, inst.a, result);
                        }
                    }

                    // -------------------------------------------------------
                    // 27: Min
                    // -------------------------------------------------------
                    27 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        if arg_ty == Some(JitVarType::Float) {
                            let a = use_var(&mut builder, &vars, arg_base);
                            let b = use_var(&mut builder, &vars, arg_base + 1);
                            let result = builder.ins().fmin(a, b);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &vars, inst.a, result);
                        } else {
                            let a = use_var(&mut builder, &vars, arg_base);
                            let b = use_var(&mut builder, &vars, arg_base + 1);
                            let result = builder.ins().smin(a, b);
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &vars, inst.a, result);
                        }
                    }

                    // -------------------------------------------------------
                    // 28: Max
                    // -------------------------------------------------------
                    28 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        if arg_ty == Some(JitVarType::Float) {
                            let a = use_var(&mut builder, &vars, arg_base);
                            let b = use_var(&mut builder, &vars, arg_base + 1);
                            let result = builder.ins().fmax(a, b);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &vars, inst.a, result);
                        } else {
                            let a = use_var(&mut builder, &vars, arg_base);
                            let b = use_var(&mut builder, &vars, arg_base + 1);
                            let result = builder.ins().smax(a, b);
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &vars, inst.a, result);
                        }
                    }

                    // -------------------------------------------------------
                    // 50: IsEmpty — string only for now
                    // -------------------------------------------------------
                    50 => {
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Str) {
                            let str_ptr = use_var(&mut builder, &vars, arg_base);
                            let call = builder.ins().call(intrinsic_string_len_ref, &[str_ptr]);
                            let len = builder.inst_results(call)[0];
                            let zero = builder.ins().iconst(types::I64, 0);
                            let is_empty = builder.ins().icmp(IntCC::Equal, len, zero);
                            let result = builder.ins().uextend(types::I64, is_empty);
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &vars, inst.a, result);
                        } else {
                            // Non-string: stub returns false (0)
                            let zero = builder.ins().iconst(types::I64, 0);
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &vars, inst.a, zero);
                        }
                    }

                    // -------------------------------------------------------
                    // 57: Round (nearest even) — pure Cranelift IR
                    // -------------------------------------------------------
                    57 => {
                        let v = use_var(&mut builder, &vars, arg_base);
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            let result = builder.ins().nearest(v);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &vars, inst.a, result);
                        } else {
                            // Int round is a no-op
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &vars, inst.a, v);
                        }
                    }

                    // -------------------------------------------------------
                    // 58: Ceil — pure Cranelift IR
                    // -------------------------------------------------------
                    58 => {
                        let v = use_var(&mut builder, &vars, arg_base);
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            let result = builder.ins().ceil(v);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &vars, inst.a, result);
                        } else {
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &vars, inst.a, v);
                        }
                    }

                    // -------------------------------------------------------
                    // 59: Floor — pure Cranelift IR
                    // -------------------------------------------------------
                    59 => {
                        let v = use_var(&mut builder, &vars, arg_base);
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            let result = builder.ins().floor(v);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &vars, inst.a, result);
                        } else {
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &vars, inst.a, v);
                        }
                    }

                    // -------------------------------------------------------
                    // 60: Sqrt — pure Cranelift IR
                    // -------------------------------------------------------
                    60 => {
                        let v = use_var(&mut builder, &vars, arg_base);
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            let result = builder.ins().sqrt(v);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &vars, inst.a, result);
                        } else {
                            // Convert int to float first, then sqrt
                            let fv = builder.ins().fcvt_from_sint(types::F64, v);
                            let result = builder.ins().sqrt(fv);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &vars, inst.a, result);
                        }
                    }

                    // -------------------------------------------------------
                    // 61: Pow — runtime helper (no Cranelift native)
                    // -------------------------------------------------------
                    61 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        if arg_ty == Some(JitVarType::Float) {
                            let base = use_var(&mut builder, &vars, arg_base);
                            let exp = use_var(&mut builder, &vars, arg_base + 1);
                            let call = builder.ins().call(intrinsic_pow_float_ref, &[base, exp]);
                            let result = builder.inst_results(call)[0];
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &vars, inst.a, result);
                        } else {
                            let base = use_var(&mut builder, &vars, arg_base);
                            let exp = use_var(&mut builder, &vars, arg_base + 1);
                            let call = builder.ins().call(intrinsic_pow_int_ref, &[base, exp]);
                            let result = builder.inst_results(call)[0];
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &vars, inst.a, result);
                        }
                    }

                    // -------------------------------------------------------
                    // 62: Log (natural) — runtime helper
                    // -------------------------------------------------------
                    62 => {
                        let v = use_var(&mut builder, &vars, arg_base);
                        let fv = if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            v
                        } else {
                            builder.ins().fcvt_from_sint(types::F64, v)
                        };
                        let call = builder.ins().call(intrinsic_log_ref, &[fv]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Float);
                        def_var(&mut builder, &vars, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 63: Sin — runtime helper
                    // -------------------------------------------------------
                    63 => {
                        let v = use_var(&mut builder, &vars, arg_base);
                        let fv = if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            v
                        } else {
                            builder.ins().fcvt_from_sint(types::F64, v)
                        };
                        let call = builder.ins().call(intrinsic_sin_ref, &[fv]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Float);
                        def_var(&mut builder, &vars, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 64: Cos — runtime helper
                    // -------------------------------------------------------
                    64 => {
                        let v = use_var(&mut builder, &vars, arg_base);
                        let fv = if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            v
                        } else {
                            builder.ins().fcvt_from_sint(types::F64, v)
                        };
                        let call = builder.ins().call(intrinsic_cos_ref, &[fv]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Float);
                        def_var(&mut builder, &vars, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 65: Clamp(val, lo, hi)
                    // -------------------------------------------------------
                    65 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        let val = use_var(&mut builder, &vars, arg_base);
                        let lo = use_var(&mut builder, &vars, arg_base + 1);
                        let hi = use_var(&mut builder, &vars, arg_base + 2);
                        if arg_ty == Some(JitVarType::Float) {
                            let clamped_lo = builder.ins().fmax(val, lo);
                            let result = builder.ins().fmin(clamped_lo, hi);
                            var_types.insert(inst.a as u32, JitVarType::Float);
                            def_var(&mut builder, &vars, inst.a, result);
                        } else {
                            let clamped_lo = builder.ins().smax(val, lo);
                            let result = builder.ins().smin(clamped_lo, hi);
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &vars, inst.a, result);
                        }
                    }

                    // -------------------------------------------------------
                    // 68 / 96 / 97: Debug / Eprint / Eprintln — same as print for now
                    // -------------------------------------------------------
                    68 | 96 | 97 => {
                        let arg_ty = var_types.get(&(arg_base as u32)).copied();
                        match arg_ty {
                            Some(JitVarType::Str) => {
                                let v = use_var(&mut builder, &vars, arg_base);
                                builder.ins().call(intrinsic_print_str_ref, &[v]);
                            }
                            Some(JitVarType::Float) => {
                                let v = use_var(&mut builder, &vars, arg_base);
                                builder.ins().call(intrinsic_print_float_ref, &[v]);
                            }
                            _ => {
                                let v = use_var(&mut builder, &vars, arg_base);
                                builder.ins().call(intrinsic_print_int_ref, &[v]);
                            }
                        }
                        let zero = builder.ins().iconst(types::I64, 0);
                        var_types.insert(inst.a as u32, JitVarType::Int);
                        def_var(&mut builder, &vars, inst.a, zero);
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
                        let v = use_var(&mut builder, &vars, arg_base);
                        let fv = if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            v
                        } else {
                            builder.ins().fcvt_from_sint(types::F64, v)
                        };
                        let call = builder.ins().call(intrinsic_log2_ref, &[fv]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Float);
                        def_var(&mut builder, &vars, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 124: Log10 — runtime helper
                    // -------------------------------------------------------
                    124 => {
                        let v = use_var(&mut builder, &vars, arg_base);
                        let fv = if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            v
                        } else {
                            builder.ins().fcvt_from_sint(types::F64, v)
                        };
                        let call = builder.ins().call(intrinsic_log10_ref, &[fv]);
                        let result = builder.inst_results(call)[0];
                        var_types.insert(inst.a as u32, JitVarType::Float);
                        def_var(&mut builder, &vars, inst.a, result);
                    }

                    // -------------------------------------------------------
                    // 125: IsNan — pure Cranelift IR
                    // fcmp Unordered(v, v) — NaN is unordered with itself
                    // -------------------------------------------------------
                    125 => {
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            let v = use_var(&mut builder, &vars, arg_base);
                            let is_nan = builder.ins().fcmp(FloatCC::Unordered, v, v);
                            let result = builder.ins().uextend(types::I64, is_nan);
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &vars, inst.a, result);
                        } else {
                            // Integers are never NaN
                            let zero = builder.ins().iconst(types::I64, 0);
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &vars, inst.a, zero);
                        }
                    }

                    // -------------------------------------------------------
                    // 126: IsInfinite — pure Cranelift IR
                    // fabs(v) == +inf
                    // -------------------------------------------------------
                    126 => {
                        if var_types.get(&(arg_base as u32)) == Some(&JitVarType::Float) {
                            let v = use_var(&mut builder, &vars, arg_base);
                            let abs_v = builder.ins().fabs(v);
                            let inf = builder.ins().f64const(f64::INFINITY);
                            let is_inf = builder.ins().fcmp(FloatCC::Equal, abs_v, inf);
                            let result = builder.ins().uextend(types::I64, is_inf);
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &vars, inst.a, result);
                        } else {
                            // Integers are never infinite
                            let zero = builder.ins().iconst(types::I64, 0);
                            var_types.insert(inst.a as u32, JitVarType::Int);
                            def_var(&mut builder, &vars, inst.a, zero);
                        }
                    }

                    // -------------------------------------------------------
                    // 127: MathPi — f64const(π)
                    // -------------------------------------------------------
                    127 => {
                        let pi = builder.ins().f64const(std::f64::consts::PI);
                        var_types.insert(inst.a as u32, JitVarType::Float);
                        def_var(&mut builder, &vars, inst.a, pi);
                    }

                    // -------------------------------------------------------
                    // 128: MathE — f64const(e)
                    // -------------------------------------------------------
                    128 => {
                        let e = builder.ins().f64const(std::f64::consts::E);
                        var_types.insert(inst.a as u32, JitVarType::Float);
                        def_var(&mut builder, &vars, inst.a, e);
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
                                let v = use_var(&mut builder, &vars, arg_base);
                                let flags = MemFlags::new();
                                let rc = builder.ins().load(types::I64, flags, v, 0);
                                let rc1 = builder.ins().iadd_imm(rc, 1);
                                builder.ins().store(flags, rc1, v, 0);
                                var_types.insert(inst.a as u32, JitVarType::Str);
                                def_var(&mut builder, &vars, inst.a, v);
                            }
                            Some(JitVarType::Float) => {
                                let v = use_var(&mut builder, &vars, arg_base);
                                let call = builder.ins().call(intrinsic_to_string_float_ref, &[v]);
                                let result = builder.inst_results(call)[0];
                                var_types.insert(inst.a as u32, JitVarType::Str);
                                def_var(&mut builder, &vars, inst.a, result);
                            }
                            _ => {
                                // Int or unknown — convert to string
                                let v = use_var(&mut builder, &vars, arg_base);
                                let call = builder.ins().call(intrinsic_to_string_int_ref, &[v]);
                                let result = builder.inst_results(call)[0];
                                var_types.insert(inst.a as u32, JitVarType::Str);
                                def_var(&mut builder, &vars, inst.a, result);
                            }
                        }
                    }

                    // -------------------------------------------------------
                    // Unsupported intrinsic — return 0 stub
                    // -------------------------------------------------------
                    _ => {
                        let zero = builder.ins().iconst(types::I64, 0);
                        var_types.insert(inst.a as u32, JitVarType::Int);
                        def_var(&mut builder, &vars, inst.a, zero);
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
    vars: &[Variable],
    reg: u16,
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
    reg: u16,
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
