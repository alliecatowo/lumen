//! LIR-to-Cranelift IR lowering.
//!
//! Translates each LIR cell into a Cranelift IR function. Arithmetic,
//! comparisons, control flow, and constants are lowered directly. Opcodes
//! that require runtime support (tool calls, effects, collections, etc.)
//! emit a `trap` placeholder for now.
//!
//! ## Control-flow lowering strategy
//!
//! LIR uses a flat instruction stream with relative `Jmp` offsets (signed
//! 24-bit). Cranelift requires explicit basic blocks connected by branch
//! instructions. We bridge the gap in two passes:
//!
//! 1. **Pre-scan**: Walk the instruction list once. For every `Jmp`
//!    (including `Break`/`Continue`) compute the *target* instruction index.
//!    Each unique target index gets a fresh Cranelift `Block`.
//!
//! 2. **Emit**: Walk the instructions again. Whenever the current PC matches
//!    a block-start index we terminate the current block with an
//!    unconditional `jump` and `switch_to_block` to the new one. `Test`
//!    stores a boolean; the immediately-following `Jmp` consumes it as a
//!    `brif`.
//!
//! Function calls look up the callee by name (matching LIR cell names within
//! the module) and emit a Cranelift `call` instruction.

use std::collections::{BTreeSet, HashMap};

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{AbiParam, InstBuilder, Type as ClifType, Value};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::ObjectModule;

use lumen_compiler::compiler::lir::{Constant, Instruction, LirCell, LirModule, OpCode};

use crate::emit::CodegenError;
use crate::types::lir_type_str_to_cl_type;

/// Maximum number of virtual registers we support per cell.
const MAX_REGS: usize = 256;

/// Result of lowering an entire LIR module.
pub struct LoweredModule {
    /// One entry per cell, in the same order as `LirModule::cells`.
    pub functions: Vec<LoweredFunction>,
}

pub struct LoweredFunction {
    pub name: String,
    pub func_id: FuncId,
}

/// Lower an entire LIR module into Cranelift IR inside the given `ObjectModule`.
///
/// Each cell becomes a separate function. After this call the module is ready
/// to be finalised via `emit::emit_object`.
pub fn lower_module(
    module: &mut ObjectModule,
    lir: &LirModule,
    pointer_type: ClifType,
) -> Result<LoweredModule, CodegenError> {
    let mut fb_ctx = FunctionBuilderContext::new();

    // First pass: declare all cell signatures so we can resolve Call targets.
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
    let mut lowered = LoweredModule {
        functions: Vec::with_capacity(lir.cells.len()),
    };

    for cell in &lir.cells {
        let func_id = func_ids[&cell.name];
        lower_cell(module, cell, &mut fb_ctx, pointer_type, func_id, &func_ids)?;
        lowered.functions.push(LoweredFunction {
            name: cell.name.clone(),
            func_id,
        });
    }

    Ok(lowered)
}

// ---------------------------------------------------------------------------
// Pre-scan: identify basic-block boundaries
// ---------------------------------------------------------------------------

/// Collect all instruction indices that are jump targets. Each one will become
/// the start of a new Cranelift basic block.
fn collect_block_starts(instructions: &[Instruction]) -> BTreeSet<usize> {
    let mut targets = BTreeSet::new();

    for (pc, inst) in instructions.iter().enumerate() {
        match inst.op {
            OpCode::Jmp | OpCode::Break | OpCode::Continue => {
                let offset = inst.sax_val();
                let target = (pc as i32 + 1 + offset) as usize;
                targets.insert(target);
                // The instruction *after* the Jmp also starts a new block.
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

/// Detect whether a cell contains any self-recursive tail calls. Scans for
/// `TailCall` instructions whose callee (resolved via `find_callee_name`)
/// matches the cell's own name.
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

// ---------------------------------------------------------------------------
// Per-cell lowering
// ---------------------------------------------------------------------------

fn lower_cell(
    module: &mut ObjectModule,
    cell: &LirCell,
    fb_ctx: &mut FunctionBuilderContext,
    pointer_type: ClifType,
    func_id: FuncId,
    func_ids: &HashMap<String, FuncId>,
) -> Result<(), CodegenError> {
    // Re-build the signature.
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

    // Pre-declare all callable functions in this function's namespace.
    // We do this before creating the FunctionBuilder because
    // `module.declare_func_in_func` needs `&mut module` and `&mut func`.
    let mut callee_refs: HashMap<FuncId, cranelift_codegen::ir::FuncRef> = HashMap::new();
    for (&ref _name, &callee_id) in func_ids.iter() {
        let func_ref = module.declare_func_in_func(callee_id, &mut func);
        callee_refs.insert(callee_id, func_ref);
    }

    let mut builder = FunctionBuilder::new(&mut func, fb_ctx);

    // --- Declare variables for the register file ---
    let num_regs = (cell.registers as usize)
        .max(cell.params.len())
        .max(1)
        .min(MAX_REGS);
    let mut vars: Vec<Variable> = Vec::with_capacity(num_regs);
    for i in 0..num_regs {
        let var = Variable::from_u32(i as u32);
        builder.declare_var(var, types::I64);
        vars.push(var);
    }

    // --- Tail-call optimization: detect self-recursive tail calls ---
    //
    // If the cell contains any TailCall instructions targeting itself, we set
    // up a "loop header" block that the tail calls can jump back to. The ABI
    // entry block receives the actual function parameters, copies them into
    // variables, and falls through to the loop header. Self-recursive tail
    // calls overwrite the parameter variables and jump to the loop header,
    // turning recursion into a loop.
    let self_tco = has_self_tail_call(cell);

    // --- Pre-scan for basic-block boundaries ---
    let block_starts = collect_block_starts(&cell.instructions);

    // Create the entry block.
    let entry_block = builder.create_block();
    builder.append_block_params_for_function_params(entry_block);
    builder.switch_to_block(entry_block);

    // Seed parameter registers.
    for (i, _param) in cell.params.iter().enumerate() {
        if i < vars.len() {
            let val = builder.block_params(entry_block)[i];
            builder.def_var(vars[i], val);
        }
    }

    // Initialize remaining registers to zero so Cranelift's SSA verifier
    // does not complain about undefined uses on branches.
    {
        let zero = builder.ins().iconst(types::I64, 0);
        for i in cell.params.len()..num_regs {
            builder.def_var(vars[i], zero);
        }
    }

    // If self-TCO is active, create a loop header block and jump to it.
    // Self tail calls will jump back here after updating param variables.
    let tco_loop_block = if self_tco {
        let loop_block = builder.create_block();
        builder.ins().jump(loop_block, &[]);
        builder.switch_to_block(loop_block);
        Some(loop_block)
    } else {
        None
    };

    // Pre-create Cranelift blocks for each identified block start.
    let mut block_map: HashMap<usize, cranelift_codegen::ir::Block> = HashMap::new();
    for &pc in &block_starts {
        let blk = builder.create_block();
        block_map.insert(pc, blk);
    }

    // --- Emit instructions ---
    let mut terminated = false;
    // When we see a Test instruction, we stash the register index here so
    // the immediately following Jmp can consume it as a conditional branch.
    let mut pending_test: Option<u8> = None;

    for (pc, inst) in cell.instructions.iter().enumerate() {
        // If this PC is the start of a new block, finalize the current one.
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
            // ---- Constants ---------------------------------------------------
            OpCode::LoadK => {
                let a = inst.a;
                let bx = inst.bx() as usize;
                let val = lower_constant(&mut builder, cell, bx, pointer_type)?;
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

            // ---- Integer arithmetic ------------------------------------------
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

            // ---- Bitwise -----------------------------------------------------
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

            // ---- Comparison --------------------------------------------------
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

            // ---- Logic -------------------------------------------------------
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

            // ---- Test -------------------------------------------------------
            OpCode::Test => {
                // The LIR `Test` instruction semantics:
                //   if (Reg[A] is truthy) != C then skip next instruction.
                // We stash the register so the following Jmp can emit a `brif`.
                pending_test = Some(inst.a);
            }

            // ---- Control flow: Jmp -------------------------------------------
            OpCode::Jmp | OpCode::Break | OpCode::Continue => {
                let offset = inst.sax_val();
                let target_pc = (pc as i32 + 1 + offset) as usize;
                let fallthrough_pc = pc + 1;

                let target_block = get_or_create_block(&mut builder, &mut block_map, target_pc);
                let fallthrough_block =
                    get_or_create_block(&mut builder, &mut block_map, fallthrough_pc);

                if let Some(test_reg) = pending_test.take() {
                    // Conditional branch: truthy → fallthrough, falsy → target.
                    let cond = use_var(&mut builder, &vars, test_reg);
                    let zero = builder.ins().iconst(types::I64, 0);
                    let is_truthy = builder.ins().icmp(IntCC::NotEqual, cond, zero);
                    builder
                        .ins()
                        .brif(is_truthy, fallthrough_block, &[], target_block, &[]);
                } else {
                    // Unconditional jump.
                    builder.ins().jump(target_block, &[]);
                }
                terminated = true;
            }

            // ---- Return / Halt -----------------------------------------------
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

            // ---- Function calls ----------------------------------------------
            OpCode::Call => {
                let base = inst.a;
                let num_args = inst.b as usize;

                let callee_name = find_callee_name(cell, &cell.instructions, pc, base);

                if let Some(ref name) = callee_name {
                    if let Some(&callee_func_id) = func_ids.get(name.as_str()) {
                        if let Some(&func_ref) = callee_refs.get(&callee_func_id) {
                            let mut args: Vec<Value> = Vec::with_capacity(num_args);
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

                // Check for self-recursive tail call — emit a loop-back jump
                // instead of call + return (tail-call optimization).
                let is_self_call = callee_name
                    .as_ref()
                    .map(|n| n == &cell.name)
                    .unwrap_or(false);

                if is_self_call && self_tco {
                    if let Some(loop_block) = tco_loop_block {
                        // Read the new argument values FIRST, before overwriting
                        // any parameter variables (a later arg might read from
                        // a register that is also a parameter slot).
                        let mut new_args: Vec<Value> = Vec::with_capacity(num_args);
                        for i in 0..num_args {
                            let arg_reg = base + 1 + i as u8;
                            new_args.push(use_var(&mut builder, &vars, arg_reg));
                        }

                        // Write the new values into the parameter variables.
                        for (i, &val) in new_args.iter().enumerate() {
                            if i < vars.len() {
                                builder.def_var(vars[i], val);
                            }
                        }

                        // Jump back to the loop header — this is where TCO
                        // actually happens: no new stack frame is created.
                        builder.ins().jump(loop_block, &[]);
                        terminated = true;
                    }
                } else if let Some(ref name) = callee_name {
                    if let Some(&callee_func_id) = func_ids.get(name.as_str()) {
                        if let Some(&func_ref) = callee_refs.get(&callee_func_id) {
                            let mut args: Vec<Value> = Vec::with_capacity(num_args);
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

            // ---- Loop/for (legacy opcodes) -----------------------------------
            OpCode::Loop | OpCode::ForPrep | OpCode::ForLoop | OpCode::ForIn => {}

            // ---- Nop ---------------------------------------------------------
            OpCode::Nop => {}

            // ---- Everything else → trap --------------------------------------
            _ => {
                builder
                    .ins()
                    .trap(cranelift_codegen::ir::TrapCode::unwrap_user(2));
                terminated = true;
            }
        }
    }

    // Ensure the function is terminated.
    if !terminated {
        let zero = builder.ins().iconst(types::I64, 0);
        builder.ins().return_(&[zero]);
    }

    // Seal all blocks. We defer sealing to the end because loop headers
    // have backward-jump predecessors that aren't known until after the
    // entire instruction stream is emitted.
    builder.seal_all_blocks();
    builder.finalize();

    // Compile and define the function in the module.
    let mut ctx = Context::for_function(func);
    module
        .define_function(func_id, &mut ctx)
        .map_err(|e| CodegenError::LoweringError(format!("define_function({}): {e}", cell.name)))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Helper: resolve callee name from constant pool
// ---------------------------------------------------------------------------

/// Scan backwards from a Call instruction to find the LoadK that populated the
/// base register with a function-name string constant.
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
// Variable helpers
// ---------------------------------------------------------------------------

fn use_var(builder: &mut FunctionBuilder, vars: &[Variable], reg: u8) -> Value {
    let idx = reg as usize;
    if idx < vars.len() {
        builder.use_var(vars[idx])
    } else {
        builder.ins().iconst(types::I64, 0)
    }
}

fn def_var(builder: &mut FunctionBuilder, vars: &[Variable], reg: u8, val: Value) {
    let idx = reg as usize;
    if idx < vars.len() {
        builder.def_var(vars[idx], val);
    }
}

/// Get or create a Cranelift block for a given instruction PC.
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
    _pointer_type: ClifType,
) -> Result<Value, CodegenError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CodegenContext;
    use crate::emit::emit_object;
    use lumen_compiler::compiler::lir::{
        Constant, Instruction, LirCell, LirModule, LirParam, OpCode,
    };

    /// Build a minimal LirModule with a single cell for testing.
    fn make_module(
        name: &str,
        constants: Vec<Constant>,
        instructions: Vec<Instruction>,
    ) -> LirModule {
        make_module_with_params(name, Vec::new(), 4, constants, instructions)
    }

    fn make_module_with_params(
        name: &str,
        params: Vec<LirParam>,
        registers: u8,
        constants: Vec<Constant>,
        instructions: Vec<Instruction>,
    ) -> LirModule {
        LirModule {
            version: "1.0.0".to_string(),
            doc_hash: "test".to_string(),
            strings: Vec::new(),
            types: Vec::new(),
            cells: vec![LirCell {
                name: name.to_string(),
                params,
                returns: Some("Int".to_string()),
                registers,
                constants,
                instructions,
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

    fn make_multi_cell_module(cells: Vec<LirCell>) -> LirModule {
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
    fn lower_load_const_add_return() {
        let lir = make_module(
            "add_two",
            vec![Constant::Int(10), Constant::Int(32)],
            vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Add, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
        );

        let mut ctx = CodegenContext::new().expect("host context");
        let ptr_ty = ctx.pointer_type();
        let result = lower_module(&mut ctx.module, &lir, ptr_ty);
        assert!(
            result.is_ok(),
            "lowering should succeed: {:?}",
            result.err()
        );
        let lowered = result.unwrap();
        assert_eq!(lowered.functions.len(), 1);
        assert_eq!(lowered.functions[0].name, "add_two");
    }

    #[test]
    fn lower_bool_constants() {
        let lir = make_module(
            "bool_test",
            vec![],
            vec![
                Instruction::abc(OpCode::LoadBool, 0, 1, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
        );

        let mut ctx = CodegenContext::new().expect("host context");
        let ptr_ty = ctx.pointer_type();
        let result = lower_module(&mut ctx.module, &lir, ptr_ty);
        assert!(
            result.is_ok(),
            "bool lowering should succeed: {:?}",
            result.err()
        );
    }

    #[test]
    fn lower_comparison_ops() {
        let lir = make_module(
            "cmp_test",
            vec![Constant::Int(5), Constant::Int(10)],
            vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Eq, 2, 0, 1),
                Instruction::abc(OpCode::Lt, 3, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
        );

        let mut ctx = CodegenContext::new().expect("host context");
        let ptr_ty = ctx.pointer_type();
        let result = lower_module(&mut ctx.module, &lir, ptr_ty);
        assert!(
            result.is_ok(),
            "comparison lowering should succeed: {:?}",
            result.err()
        );
    }

    // -----------------------------------------------------------------------
    // T029: If/else to basic blocks
    // -----------------------------------------------------------------------

    #[test]
    fn lower_if_else() {
        // Simulates: if r0 then r1 = 10 else r1 = 20; return r1
        //
        // LIR layout:
        //  0: LoadBool r0, 1          (r0 = true)
        //  1: LoadBool r2, 1          (true const for comparison)
        //  2: Eq       r3, r0, r2     (r3 = r0 == true)
        //  3: Test     r3, 0, 0       (if r3 truthy, skip Jmp)
        //  4: Jmp      +2             (→ inst 7: else branch)
        //  5: LoadInt   r1, 10        (then: r1 = 10)
        //  6: Jmp      +1             (→ inst 8: after else)
        //  7: LoadInt   r1, 20        (else: r1 = 20)
        //  8: Return   r1
        let lir = make_module(
            "if_else_test",
            vec![],
            vec![
                Instruction::abc(OpCode::LoadBool, 0, 1, 0), // 0
                Instruction::abc(OpCode::LoadBool, 2, 1, 0), // 1
                Instruction::abc(OpCode::Eq, 3, 0, 2),       // 2
                Instruction::abc(OpCode::Test, 3, 0, 0),     // 3
                Instruction::sax(OpCode::Jmp, 2),            // 4 → 7
                Instruction::abc(OpCode::LoadInt, 1, 10, 0), // 5 (then)
                Instruction::sax(OpCode::Jmp, 1),            // 6 → 8
                Instruction::abc(OpCode::LoadInt, 1, 20, 0), // 7 (else)
                Instruction::abc(OpCode::Return, 1, 1, 0),   // 8
            ],
        );

        let mut ctx = CodegenContext::new().expect("host context");
        let ptr_ty = ctx.pointer_type();
        let result = lower_module(&mut ctx.module, &lir, ptr_ty);
        assert!(
            result.is_ok(),
            "if/else lowering should succeed: {:?}",
            result.err()
        );

        let bytes = emit_object(ctx.module).expect("emission should succeed");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn lower_if_no_else() {
        // if r0 then r1 = 42; return r1
        //
        //  0: LoadBool r0, 1
        //  1: Test     r0, 0, 0
        //  2: Jmp      +1             (→ 4: skip then)
        //  3: LoadInt   r1, 42        (then body)
        //  4: Return   r1
        let lir = make_module(
            "if_no_else",
            vec![],
            vec![
                Instruction::abc(OpCode::LoadBool, 0, 1, 0), // 0
                Instruction::abc(OpCode::Test, 0, 0, 0),     // 1
                Instruction::sax(OpCode::Jmp, 1),            // 2 → 4
                Instruction::abc(OpCode::LoadInt, 1, 42, 0), // 3
                Instruction::abc(OpCode::Return, 1, 1, 0),   // 4
            ],
        );

        let mut ctx = CodegenContext::new().expect("host context");
        let ptr_ty = ctx.pointer_type();
        let result = lower_module(&mut ctx.module, &lir, ptr_ty);
        assert!(
            result.is_ok(),
            "if-no-else lowering should succeed: {:?}",
            result.err()
        );
    }

    // -----------------------------------------------------------------------
    // T030: Loops (while loop with backward jump)
    // -----------------------------------------------------------------------

    #[test]
    fn lower_while_loop() {
        // r0 = 0; r1 = 5; while r0 < r1 do r0 = r0 + 1 end; return r0
        //
        //  0: LoadInt  r0, 0
        //  1: LoadInt  r1, 5
        //  2: LoadInt  r2, 1
        //  3: Lt       r3, r0, r1     (loop header)
        //  4: Test     r3, 0, 0
        //  5: Jmp      +2             (→ 8: exit loop)
        //  6: Add      r0, r0, r2
        //  7: Jmp      -5             (→ 3: loop header)
        //  8: Return   r0
        let lir = make_module(
            "while_loop",
            vec![],
            vec![
                Instruction::abc(OpCode::LoadInt, 0, 0, 0), // 0
                Instruction::abc(OpCode::LoadInt, 1, 5, 0), // 1
                Instruction::abc(OpCode::LoadInt, 2, 1, 0), // 2
                Instruction::abc(OpCode::Lt, 3, 0, 1),      // 3
                Instruction::abc(OpCode::Test, 3, 0, 0),    // 4
                Instruction::sax(OpCode::Jmp, 2),           // 5 → 8
                Instruction::abc(OpCode::Add, 0, 0, 2),     // 6
                Instruction::sax(OpCode::Jmp, -5),          // 7 → 3
                Instruction::abc(OpCode::Return, 0, 1, 0),  // 8
            ],
        );

        let mut ctx = CodegenContext::new().expect("host context");
        let ptr_ty = ctx.pointer_type();
        let result = lower_module(&mut ctx.module, &lir, ptr_ty);
        assert!(
            result.is_ok(),
            "while-loop lowering should succeed: {:?}",
            result.err()
        );

        let bytes = emit_object(ctx.module).expect("emission should succeed");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn lower_loop_with_break() {
        // loop { r0 += 1; if r0 == 5 break }; return r0
        //
        //  0: LoadInt  r0, 0
        //  1: LoadInt  r1, 1
        //  2: LoadInt  r2, 5
        //  3: Add      r0, r0, r1     (loop body)
        //  4: Eq       r3, r0, r2
        //  5: Test     r3, 0, 0
        //  6: Jmp      +2             (→ 9: not equal, skip break)
        //  7: Jmp      +2             (→ 10: break)
        //  8: Nop
        //  9: Jmp      -7             (→ 3: back to loop)
        // 10: Return   r0
        let lir = make_module(
            "loop_break",
            vec![],
            vec![
                Instruction::abc(OpCode::LoadInt, 0, 0, 0), // 0
                Instruction::abc(OpCode::LoadInt, 1, 1, 0), // 1
                Instruction::abc(OpCode::LoadInt, 2, 5, 0), // 2
                Instruction::abc(OpCode::Add, 0, 0, 1),     // 3
                Instruction::abc(OpCode::Eq, 3, 0, 2),      // 4
                Instruction::abc(OpCode::Test, 3, 0, 0),    // 5
                Instruction::sax(OpCode::Jmp, 2),           // 6 → 9
                Instruction::sax(OpCode::Jmp, 2),           // 7 → 10
                Instruction::abc(OpCode::Nop, 0, 0, 0),     // 8
                Instruction::sax(OpCode::Jmp, -7),          // 9 → 3
                Instruction::abc(OpCode::Return, 0, 1, 0),  // 10
            ],
        );

        let mut ctx = CodegenContext::new().expect("host context");
        let ptr_ty = ctx.pointer_type();
        let result = lower_module(&mut ctx.module, &lir, ptr_ty);
        assert!(
            result.is_ok(),
            "loop-break lowering should succeed: {:?}",
            result.err()
        );
    }

    // -----------------------------------------------------------------------
    // T031: Match/switch (chain of brif)
    // -----------------------------------------------------------------------

    #[test]
    fn lower_match_chain() {
        // match r0 { 1 => r1=10, 2 => r1=20, _ => r1=30 }; return r1
        //
        //  0: LoadInt  r0, 2          (subject)
        //  1: LoadInt  r2, 1          (pattern 1)
        //  2: Eq       r3, r0, r2
        //  3: Test     r3, 0, 0
        //  4: Jmp      +2             (→ 7: try next arm)
        //  5: LoadInt  r1, 10         (arm 1 body)
        //  6: Jmp      +7             (→ 14: end)
        //  7: LoadInt  r2, 2          (pattern 2)
        //  8: Eq       r3, r0, r2
        //  9: Test     r3, 0, 0
        // 10: Jmp      +2             (→ 13: wildcard)
        // 11: LoadInt  r1, 20         (arm 2 body)
        // 12: Jmp      +1             (→ 14: end)
        // 13: LoadInt  r1, 30         (wildcard)
        // 14: Return   r1
        let lir = make_module(
            "match_chain",
            vec![],
            vec![
                Instruction::abc(OpCode::LoadInt, 0, 2, 0),  //  0
                Instruction::abc(OpCode::LoadInt, 2, 1, 0),  //  1
                Instruction::abc(OpCode::Eq, 3, 0, 2),       //  2
                Instruction::abc(OpCode::Test, 3, 0, 0),     //  3
                Instruction::sax(OpCode::Jmp, 2),            //  4 → 7
                Instruction::abc(OpCode::LoadInt, 1, 10, 0), //  5
                Instruction::sax(OpCode::Jmp, 7),            //  6 → 14
                Instruction::abc(OpCode::LoadInt, 2, 2, 0),  //  7
                Instruction::abc(OpCode::Eq, 3, 0, 2),       //  8
                Instruction::abc(OpCode::Test, 3, 0, 0),     //  9
                Instruction::sax(OpCode::Jmp, 2),            // 10 → 13
                Instruction::abc(OpCode::LoadInt, 1, 20, 0), // 11
                Instruction::sax(OpCode::Jmp, 1),            // 12 → 14
                Instruction::abc(OpCode::LoadInt, 1, 30, 0), // 13
                Instruction::abc(OpCode::Return, 1, 1, 0),   // 14
            ],
        );

        let mut ctx = CodegenContext::new().expect("host context");
        let ptr_ty = ctx.pointer_type();
        let result = lower_module(&mut ctx.module, &lir, ptr_ty);
        assert!(
            result.is_ok(),
            "match-chain lowering should succeed: {:?}",
            result.err()
        );

        let bytes = emit_object(ctx.module).expect("emission should succeed");
        assert!(!bytes.is_empty());
    }

    // -----------------------------------------------------------------------
    // T032: Function calls (cross-cell)
    // -----------------------------------------------------------------------

    #[test]
    fn lower_function_call() {
        // Two cells: `double(x) = x + x` and `main() = double(21)`
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

        let lir = make_multi_cell_module(vec![double_cell, main_cell]);

        let mut ctx = CodegenContext::new().expect("host context");
        let ptr_ty = ctx.pointer_type();
        let result = lower_module(&mut ctx.module, &lir, ptr_ty);
        assert!(
            result.is_ok(),
            "function-call lowering should succeed: {:?}",
            result.err()
        );

        let lowered = result.unwrap();
        assert_eq!(lowered.functions.len(), 2);
        assert_eq!(lowered.functions[0].name, "double");
        assert_eq!(lowered.functions[1].name, "main");

        let bytes = emit_object(ctx.module).expect("emission should succeed");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn lower_tail_call() {
        let identity_cell = LirCell {
            name: "identity".to_string(),
            params: vec![LirParam {
                name: "x".to_string(),
                ty: "Int".to_string(),
                register: 0,
                variadic: false,
            }],
            returns: Some("Int".to_string()),
            registers: 2,
            constants: vec![],
            instructions: vec![Instruction::abc(OpCode::Return, 0, 1, 0)],
            effect_handler_metas: Vec::new(),
        };

        let main_cell = LirCell {
            name: "main_tail".to_string(),
            params: vec![],
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![Constant::String("identity".to_string()), Constant::Int(42)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::TailCall, 0, 1, 1),
            ],
            effect_handler_metas: Vec::new(),
        };

        let lir = make_multi_cell_module(vec![identity_cell, main_cell]);

        let mut ctx = CodegenContext::new().expect("host context");
        let ptr_ty = ctx.pointer_type();
        let result = lower_module(&mut ctx.module, &lir, ptr_ty);
        assert!(
            result.is_ok(),
            "tail-call lowering should succeed: {:?}",
            result.err()
        );
    }

    // -----------------------------------------------------------------------
    // T027/T028: Function prologue/epilogue with parameters
    // -----------------------------------------------------------------------

    #[test]
    fn lower_function_with_params() {
        let lir = make_module_with_params(
            "add_params",
            vec![
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
            4,
            vec![],
            vec![
                Instruction::abc(OpCode::Add, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
        );

        let mut ctx = CodegenContext::new().expect("host context");
        let ptr_ty = ctx.pointer_type();
        let result = lower_module(&mut ctx.module, &lir, ptr_ty);
        assert!(
            result.is_ok(),
            "parameterized function lowering should succeed: {:?}",
            result.err()
        );

        let bytes = emit_object(ctx.module).expect("emission should succeed");
        assert!(!bytes.is_empty());
    }

    // -----------------------------------------------------------------------
    // Integration tests: compiler → codegen → object bytes
    // -----------------------------------------------------------------------

    #[test]
    fn integration_compile_and_lower() {
        let source = "cell main() -> Int\n  1 + 2\nend\n";
        let lir = lumen_compiler::compile(source).expect("compilation should succeed");

        let mut ctx = CodegenContext::new().expect("host context");
        let ptr_ty = ctx.pointer_type();
        let result = lower_module(&mut ctx.module, &lir, ptr_ty);
        assert!(
            result.is_ok(),
            "lowering compiler output should succeed: {:?}",
            result.err()
        );

        let lowered = result.unwrap();
        assert!(
            !lowered.functions.is_empty(),
            "should have at least one function"
        );

        let bytes = emit_object(ctx.module).expect("emission should succeed");
        assert!(!bytes.is_empty(), "object file should not be empty");
        assert!(bytes.len() > 16, "object file should have reasonable size");
    }

    #[test]
    fn integration_if_else_from_compiler() {
        let source = r#"
cell choose(x: Int) -> Int
  if x > 0
    100
  else
    200
  end
end
"#;
        let lir = lumen_compiler::compile(source).expect("compilation should succeed");

        let mut ctx = CodegenContext::new().expect("host context");
        let ptr_ty = ctx.pointer_type();
        let result = lower_module(&mut ctx.module, &lir, ptr_ty);
        assert!(
            result.is_ok(),
            "if/else integration should succeed: {:?}",
            result.err()
        );

        let bytes = emit_object(ctx.module).expect("emission should succeed");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn integration_function_call_from_compiler() {
        let source = r#"
cell double(x: Int) -> Int
  x + x
end

cell main() -> Int
  double(21)
end
"#;
        let lir = lumen_compiler::compile(source).expect("compilation should succeed");

        let mut ctx = CodegenContext::new().expect("host context");
        let ptr_ty = ctx.pointer_type();
        let result = lower_module(&mut ctx.module, &lir, ptr_ty);
        assert!(
            result.is_ok(),
            "function call integration should succeed: {:?}",
            result.err()
        );

        let lowered = result.unwrap();
        assert!(
            lowered.functions.len() >= 2,
            "should have at least two functions (double + main)"
        );

        let bytes = emit_object(ctx.module).expect("emission should succeed");
        assert!(!bytes.is_empty());
    }

    // -----------------------------------------------------------------------
    // T033: Tail-call optimization (self-recursive → loop)
    // -----------------------------------------------------------------------

    #[test]
    fn tco_self_recursive_tail_call() {
        // Simulates a countdown function:
        //   cell countdown(n: Int) -> Int
        //     if n <= 0
        //       0
        //     else
        //       countdown(n - 1)   # self tail-call → should become jump
        //     end
        //   end
        //
        // LIR layout:
        //  0: LoadInt   r1, 0          (base case value)
        //  1: Le        r2, r0, r1     (n <= 0?)
        //  2: Test      r2, 0, 0
        //  3: Jmp       +2             (→ 6: else)
        //  4: Move      r0, r1         (return 0)
        //  5: Return    r0
        //  6: LoadK     r3, 0          (load "countdown" fn name)
        //  7: LoadInt   r5, 1
        //  8: Sub       r4, r0, r5     (n - 1)
        //  9: TailCall  r3, 1, 1       (tail-call countdown(r4))
        let cell = LirCell {
            name: "countdown".to_string(),
            params: vec![LirParam {
                name: "n".to_string(),
                ty: "Int".to_string(),
                register: 0,
                variadic: false,
            }],
            returns: Some("Int".to_string()),
            registers: 6,
            constants: vec![Constant::String("countdown".to_string())],
            instructions: vec![
                Instruction::abc(OpCode::LoadInt, 1, 0, 0),  // 0: r1 = 0
                Instruction::abc(OpCode::Le, 2, 0, 1),       // 1: r2 = n <= 0
                Instruction::abc(OpCode::Test, 2, 0, 0),     // 2: test r2
                Instruction::sax(OpCode::Jmp, 2),            // 3: → 6 (else)
                Instruction::abc(OpCode::Move, 0, 1, 0),     // 4: r0 = 0
                Instruction::abc(OpCode::Return, 0, 1, 0),   // 5: return r0
                Instruction::abx(OpCode::LoadK, 3, 0),       // 6: r3 = "countdown"
                Instruction::abc(OpCode::LoadInt, 5, 1, 0),  // 7: r5 = 1
                Instruction::abc(OpCode::Sub, 4, 0, 5),      // 8: r4 = n - 1
                Instruction::abc(OpCode::TailCall, 3, 1, 1), // 9: tail-call countdown(r4)
            ],
            effect_handler_metas: Vec::new(),
        };

        // Verify TCO detection
        assert!(
            has_self_tail_call(&cell),
            "should detect self-recursive tail call"
        );

        let lir = make_multi_cell_module(vec![cell]);
        let mut ctx = CodegenContext::new().expect("host context");
        let ptr_ty = ctx.pointer_type();
        let result = lower_module(&mut ctx.module, &lir, ptr_ty);
        assert!(
            result.is_ok(),
            "self-recursive TCO lowering should succeed: {:?}",
            result.err()
        );

        let bytes = emit_object(ctx.module).expect("emission should succeed");
        assert!(!bytes.is_empty(), "object bytes should not be empty");
        assert!(bytes.len() > 16);
    }

    #[test]
    fn tco_non_self_tail_call_no_loop() {
        // A TailCall to a *different* function should NOT trigger TCO.
        // It should still emit call + return.
        let helper = LirCell {
            name: "helper".to_string(),
            params: vec![LirParam {
                name: "x".to_string(),
                ty: "Int".to_string(),
                register: 0,
                variadic: false,
            }],
            returns: Some("Int".to_string()),
            registers: 2,
            constants: vec![],
            instructions: vec![Instruction::abc(OpCode::Return, 0, 1, 0)],
            effect_handler_metas: Vec::new(),
        };

        let caller = LirCell {
            name: "caller".to_string(),
            params: vec![LirParam {
                name: "x".to_string(),
                ty: "Int".to_string(),
                register: 0,
                variadic: false,
            }],
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![Constant::String("helper".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 1, 0),       // r1 = "helper"
                Instruction::abc(OpCode::Move, 2, 0, 0),     // r2 = x
                Instruction::abc(OpCode::TailCall, 1, 1, 1), // tail-call helper(x)
            ],
            effect_handler_metas: Vec::new(),
        };

        // Verify: caller does NOT have self-tail-calls
        assert!(
            !has_self_tail_call(&caller),
            "non-self tail call should not trigger TCO detection"
        );

        let lir = make_multi_cell_module(vec![helper, caller]);
        let mut ctx = CodegenContext::new().expect("host context");
        let ptr_ty = ctx.pointer_type();
        let result = lower_module(&mut ctx.module, &lir, ptr_ty);
        assert!(
            result.is_ok(),
            "non-self tail-call lowering should succeed: {:?}",
            result.err()
        );

        let bytes = emit_object(ctx.module).expect("emission should succeed");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn tco_has_self_tail_call_detection() {
        // Positive case: cell that tail-calls itself
        let self_call = LirCell {
            name: "recur".to_string(),
            params: vec![LirParam {
                name: "n".to_string(),
                ty: "Int".to_string(),
                register: 0,
                variadic: false,
            }],
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![Constant::String("recur".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 1, 0),
                Instruction::abc(OpCode::Move, 2, 0, 0),
                Instruction::abc(OpCode::TailCall, 1, 1, 1),
            ],
            effect_handler_metas: Vec::new(),
        };
        assert!(has_self_tail_call(&self_call));

        // Negative case: cell that tail-calls a different function
        let other_call = LirCell {
            name: "wrapper".to_string(),
            params: vec![LirParam {
                name: "n".to_string(),
                ty: "Int".to_string(),
                register: 0,
                variadic: false,
            }],
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![Constant::String("other_fn".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 1, 0),
                Instruction::abc(OpCode::Move, 2, 0, 0),
                Instruction::abc(OpCode::TailCall, 1, 1, 1),
            ],
            effect_handler_metas: Vec::new(),
        };
        assert!(!has_self_tail_call(&other_call));

        // Negative case: no tail calls at all
        let no_tc = LirCell {
            name: "simple".to_string(),
            params: vec![],
            returns: Some("Int".to_string()),
            registers: 2,
            constants: vec![Constant::Int(42)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };
        assert!(!has_self_tail_call(&no_tc));
    }

    #[test]
    fn tco_fibonacci_self_recursive() {
        // Simulates tail-recursive fibonacci accumulator:
        //   cell fib_acc(n: Int, a: Int, b: Int) -> Int
        //     if n <= 0 then return a end
        //     fib_acc(n - 1, b, a + b)
        //   end
        //
        //  0: LoadInt   r3, 0
        //  1: Le        r4, r0, r3      (n <= 0?)
        //  2: Test      r4, 0, 0
        //  3: Jmp       +1              (→ 5: not done)
        //  4: Return    r1              (return a)
        //  5: LoadK     r5, 0           ("fib_acc")
        //  6: LoadInt   r8, 1
        //  7: Sub       r6, r0, r8      (n - 1)
        //  8: Move      r7, r2          (b)
        //  9: Add       r8, r1, r2      (a + b)
        // 10: TailCall  r5, 3, 1        (fib_acc(r6, r7, r8))
        let cell = LirCell {
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
                Instruction::sax(OpCode::Jmp, 1),            // 3: → 5
                Instruction::abc(OpCode::Return, 1, 1, 0),   // 4: return a
                Instruction::abx(OpCode::LoadK, 5, 0),       // 5: r5 = "fib_acc"
                Instruction::abc(OpCode::LoadInt, 8, 1, 0),  // 6: r8 = 1
                Instruction::abc(OpCode::Sub, 6, 0, 8),      // 7: r6 = n - 1
                Instruction::abc(OpCode::Move, 7, 2, 0),     // 8: r7 = b
                Instruction::abc(OpCode::Add, 8, 1, 2),      // 9: r8 = a + b
                Instruction::abc(OpCode::TailCall, 5, 3, 1), // 10: tail-call fib_acc(r6, r7, r8)
            ],
            effect_handler_metas: Vec::new(),
        };

        assert!(has_self_tail_call(&cell));

        let lir = make_multi_cell_module(vec![cell]);
        let mut ctx = CodegenContext::new().expect("host context");
        let ptr_ty = ctx.pointer_type();
        let result = lower_module(&mut ctx.module, &lir, ptr_ty);
        assert!(
            result.is_ok(),
            "TCO fibonacci should compile: {:?}",
            result.err()
        );

        let bytes = emit_object(ctx.module).expect("emission should succeed");
        assert!(!bytes.is_empty());
    }
}
