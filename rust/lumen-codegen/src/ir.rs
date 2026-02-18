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
use cranelift_codegen::ir::{types, AbiParam, InstBuilder, Type as ClifType};
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
    let mut float_regs: std::collections::HashSet<u16> = std::collections::HashSet::new();
    let mut string_regs: std::collections::HashSet<u16> = std::collections::HashSet::new();
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

    // Pre-scan: identify registers that hold string constants used ONLY as
    // Call/TailCall callee names. For these we skip heap string allocation.
    let call_name_regs = identify_call_name_registers(cell);

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

            // Arithmetic (type-aware)
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
                    // String concatenation
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
                            // a = b + a: drop the old value of a (which is rhs)
                            builder.ins().call(str_drop_ref, &[rhs]);
                        }

                        var_types.insert(inst.a as u32, JitVarType::Str);
                        def_var(&mut builder, &vars, inst.a, result);
                    }
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
                // B is the record register (pointer to RecordValue)
                // C is the field name index in the string table
                let record_ptr = use_var(&mut builder, &vars, inst.b);
                let field_idx = builder.ins().iconst(types::I64, inst.c as i64);

                // For now, stub with null - full implementation requires runtime helpers
                let zero = builder.ins().iconst(types::I64, 0);
                var_types.insert(inst.a as u32, JitVarType::Int);
                def_var(&mut builder, &vars, inst.a, zero);

                // Suppress unused variable warnings
                let _ = record_ptr;
                let _ = field_idx;
            }
            OpCode::SetField => {
                // A.field[B] = C
                // A is the record register (pointer to RecordValue)
                // B is the field name index in the string table
                // C is the value to set
                let _record_ptr = use_var(&mut builder, &vars, inst.a);
                let _field_idx = builder.ins().iconst(types::I64, inst.b as i64);
                let _value = use_var(&mut builder, &vars, inst.c);

                // For now, stub - full implementation requires runtime helpers
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
