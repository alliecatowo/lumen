//! Control flow compilation for WASM backend.
//!
//! This module implements structured control flow emission for LIR bytecode.
//! It uses a **Switch-Loop** (trampoline) strategy to handle arbitrary jumps.
//!
//! ## Strategy: Switch-Loop Trampoline
//!
//! LIR is a register-based VM with arbitrary jumps (forward and backward).
//! WebAssembly only has structured control flow (if/block/loop).
//!
//! To bridge this gap, we use a trampoline pattern:
//! 1. Wrap the entire function body in a `loop $dispatch`.
//! 2. Use a local variable `$pc` to track the current instruction index.
//! 3. Inside the loop, use `br_table` to jump to the appropriate instruction.
//! 4. Each instruction updates `$pc` and branches back to `$dispatch`.

use lumen_core::lir::{Instruction, LirCell, LirModule, OpCode};
use wasm_encoder::{BlockType, Function, Instruction as WasmInst};

use crate::emit::CodegenError;

/// Emit a function body with control flow using the switch-loop strategy.
///
/// # Arguments
/// * `func` - The WASM function being built
/// * `cell` - The LIR cell to compile
/// * `lir` - The complete LIR module (for looking up function indices)
/// * `num_params` - Number of function parameters
/// * `pc_local` - Index of the PC local variable
pub fn emit_function_with_control_flow(
    func: &mut Function,
    cell: &LirCell,
    lir: &LirModule,
    num_params: usize,
    pc_local: u32,
) -> Result<(), CodegenError> {
    if cell.instructions.is_empty() {
        // Empty function: just return 0 if it has a return type
        if cell.returns.is_some() {
            func.instruction(&WasmInst::I64Const(0));
        }
        func.instruction(&WasmInst::End);
        return Ok(());
    }

    // Check if we need control flow (any jumps, branches, calls, or loops)
    let needs_control_flow = cell.instructions.iter().any(|inst| {
        matches!(
            inst.op,
            OpCode::Jmp
                | OpCode::Test
                | OpCode::Break
                | OpCode::Continue
                | OpCode::Loop
                | OpCode::ForPrep
                | OpCode::ForLoop
                | OpCode::ForIn
        )
    });

    if !needs_control_flow {
        // Simple straight-line code: emit directly without switch-loop overhead
        emit_instructions_linear(func, cell, lir, num_params)?;

        // If the function has a return type but the last instruction wasn't a Return, push a default return value.
        let last_is_return = cell
            .instructions
            .last()
            .map(|i| i.op == OpCode::Return)
            .unwrap_or(false);
        if !last_is_return && cell.returns.is_some() {
            func.instruction(&WasmInst::I64Const(0));
        }

        func.instruction(&WasmInst::End);
        return Ok(());
    }

    // Initialize PC to 0
    func.instruction(&WasmInst::I32Const(0));
    func.instruction(&WasmInst::LocalSet(pc_local));

    // Outer loop: $dispatch (depth 0 from inside)
    func.instruction(&WasmInst::Loop(BlockType::Empty));

    // Emit br_table switch on PC
    // Create nested blocks for each instruction
    let num_insts = cell.instructions.len();

    // Open blocks in reverse order (outermost first)
    for _ in 0..num_insts {
        func.instruction(&WasmInst::Block(BlockType::Empty));
    }

    // Emit br_table: jump to block based on PC value
    func.instruction(&WasmInst::LocalGet(pc_local));

    // Build target list: block 0, 1, 2, ..., default to exit
    let targets: Vec<u32> = (0..num_insts as u32).collect();
    func.instruction(&WasmInst::BrTable(targets.into(), num_insts as u32));

    // Close all the blocks
    for _ in 0..num_insts {
        func.instruction(&WasmInst::End);
    }

    // Now emit each instruction as a separate block
    for (idx, inst) in cell.instructions.iter().enumerate() {
        func.instruction(&WasmInst::Block(BlockType::Empty));

        match inst.op {
            OpCode::Jmp => {
                // Unconditional jump: set PC and branch to $dispatch
                let offset = inst.sax_val();
                let target_pc = (idx as i64 + offset) as i32;
                func.instruction(&WasmInst::I32Const(target_pc));
                func.instruction(&WasmInst::LocalSet(pc_local));
                // Branch to dispatch loop (depth = num_insts + 1 - idx)
                func.instruction(&WasmInst::Br(num_insts as u32 - idx as u32 + 1));
            }
            OpCode::Test => {
                // Test: A = register to test, C = invert flag
                let test_reg = inst.a;
                let invert = inst.c != 0;

                // Load test register
                func.instruction(&WasmInst::LocalGet(test_reg as u32));

                // Convert to boolean (i64 != 0)
                func.instruction(&WasmInst::I64Const(0));
                func.instruction(&WasmInst::I64Ne);

                if invert {
                    // Invert the condition
                    func.instruction(&WasmInst::I32Eqz);
                }

                // If condition is true, skip next instruction
                func.instruction(&WasmInst::If(BlockType::Empty));

                // Set PC to skip next instruction (idx + 2)
                func.instruction(&WasmInst::I32Const((idx + 2) as i32));
                func.instruction(&WasmInst::LocalSet(pc_local));
                func.instruction(&WasmInst::Br(num_insts as u32 - idx as u32 + 2));

                func.instruction(&WasmInst::End); // End if

                // Fall through: set PC to next instruction
                func.instruction(&WasmInst::I32Const((idx + 1) as i32));
                func.instruction(&WasmInst::LocalSet(pc_local));
                func.instruction(&WasmInst::Br(num_insts as u32 - idx as u32 + 1));
            }
            OpCode::Call | OpCode::TailCall => {
                // Call: A = function register/cell index, B = arg count, C = result count
                // For now, we need to map cell names to function indices
                let call_target = inst.a;
                let arg_count = inst.b;
                let result_count = inst.c;

                // Load call target (assumes it's a constant cell index)
                // In a real implementation, we'd need to look this up
                // For now, emit a simple call to function index
                let func_idx = call_target as u32; // This is a simplification

                // Push arguments from registers (A+1 to A+B)
                for i in 0..arg_count {
                    let arg_reg = call_target + 1 + i;
                    func.instruction(&WasmInst::LocalGet(arg_reg as u32));
                }

                // Emit call instruction
                func.instruction(&WasmInst::Call(func_idx));

                // Store results back to registers (A to A+C-1)
                if result_count > 0 {
                    func.instruction(&WasmInst::LocalSet(call_target as u32));
                }

                // Continue to next instruction
                func.instruction(&WasmInst::I32Const((idx + 1) as i32));
                func.instruction(&WasmInst::LocalSet(pc_local));
                func.instruction(&WasmInst::Br(num_insts as u32 - idx as u32 + 1));
            }
            OpCode::Break => {
                // Break: similar to Jmp, exit from loop
                let offset = inst.sax_val();
                let target_pc = (idx as i64 + offset) as i32;
                func.instruction(&WasmInst::I32Const(target_pc));
                func.instruction(&WasmInst::LocalSet(pc_local));
                func.instruction(&WasmInst::Br(num_insts as u32 - idx as u32 + 1));
            }
            OpCode::Continue => {
                // Continue: similar to Jmp, continue to loop start
                let offset = inst.sax_val();
                let target_pc = (idx as i64 + offset) as i32;
                func.instruction(&WasmInst::I32Const(target_pc));
                func.instruction(&WasmInst::LocalSet(pc_local));
                func.instruction(&WasmInst::Br(num_insts as u32 - idx as u32 + 1));
            }
            OpCode::Loop => {
                // Loop: decrement counter, jump if > 0
                // Format: A = counter register, sBx = jump offset
                let counter_reg = inst.a;
                let offset = inst.sbx() as i64;

                // Decrement counter
                func.instruction(&WasmInst::LocalGet(counter_reg as u32));
                func.instruction(&WasmInst::I64Const(1));
                func.instruction(&WasmInst::I64Sub);
                func.instruction(&WasmInst::LocalSet(counter_reg as u32));

                // Test if counter > 0
                func.instruction(&WasmInst::LocalGet(counter_reg as u32));
                func.instruction(&WasmInst::I64Const(0));
                func.instruction(&WasmInst::I64GtS);

                func.instruction(&WasmInst::If(BlockType::Empty));

                // If > 0, jump back
                let target_pc = (idx as i64 + offset) as i32;
                func.instruction(&WasmInst::I32Const(target_pc));
                func.instruction(&WasmInst::LocalSet(pc_local));
                func.instruction(&WasmInst::Br(num_insts as u32 - idx as u32 + 2));

                func.instruction(&WasmInst::End); // End if

                // Fall through to next instruction
                func.instruction(&WasmInst::I32Const((idx + 1) as i32));
                func.instruction(&WasmInst::LocalSet(pc_local));
                func.instruction(&WasmInst::Br(num_insts as u32 - idx as u32 + 1));
            }
            OpCode::ForPrep | OpCode::ForLoop | OpCode::ForIn => {
                // For-loop opcodes: emit unreachable for now (complex to implement)
                func.instruction(&WasmInst::Unreachable);
            }
            OpCode::Return => {
                // Return: don't update PC, just return
                emit_single_instruction(func, inst, cell)?;
            }
            _ => {
                // All other instructions: emit normally and advance PC
                emit_single_instruction(func, inst, cell)?;

                // Advance to next instruction
                func.instruction(&WasmInst::I32Const((idx + 1) as i32));
                func.instruction(&WasmInst::LocalSet(pc_local));
                func.instruction(&WasmInst::Br(num_insts as u32 - idx as u32 + 1));
            }
        }

        func.instruction(&WasmInst::End); // End instruction block
    }

    // End of dispatch loop
    func.instruction(&WasmInst::End);

    // If the function has a return type but the last instruction wasn't a Return, push a default return value.
    let last_is_return = cell
        .instructions
        .last()
        .map(|i| i.op == OpCode::Return)
        .unwrap_or(false);
    if !last_is_return && cell.returns.is_some() {
        func.instruction(&WasmInst::I64Const(0));
    }

    // End of function body
    func.instruction(&WasmInst::End);

    Ok(())
}

/// Emit instructions linearly without control flow (for simple functions).
fn emit_instructions_linear(
    func: &mut Function,
    cell: &LirCell,
    _lir: &LirModule,
    _num_params: usize,
) -> Result<(), CodegenError> {
    for inst in &cell.instructions {
        emit_single_instruction(func, inst, cell)?;
    }
    Ok(())
}

/// Emit a single LIR instruction to WASM.
fn emit_single_instruction(
    func: &mut Function,
    inst: &Instruction,
    cell: &LirCell,
) -> Result<(), CodegenError> {
    match inst.op {
        OpCode::LoadK => {
            let a = inst.a;
            let bx = inst.bx() as usize;
            emit_wasm_load_constant(func, cell, bx, a)?;
        }
        OpCode::LoadInt => {
            let a = inst.a;
            let imm = inst.sbx() as i64;
            func.instruction(&WasmInst::I64Const(imm));
            func.instruction(&WasmInst::LocalSet(a as u32));
        }
        OpCode::LoadBool => {
            let a = inst.a;
            let b_val = inst.b;
            func.instruction(&WasmInst::I64Const(b_val as i64));
            func.instruction(&WasmInst::LocalSet(a as u32));
        }
        OpCode::LoadNil => {
            let a = inst.a;
            let count = inst.b as usize;
            for i in 0..=count {
                let r = a as usize + i;
                func.instruction(&WasmInst::I64Const(0));
                func.instruction(&WasmInst::LocalSet(r as u32));
            }
        }
        OpCode::Move => {
            func.instruction(&WasmInst::LocalGet(inst.b as u32));
            func.instruction(&WasmInst::LocalSet(inst.a as u32));
        }
        OpCode::Add => {
            func.instruction(&WasmInst::LocalGet(inst.b as u32));
            func.instruction(&WasmInst::LocalGet(inst.c as u32));
            func.instruction(&WasmInst::I64Add);
            func.instruction(&WasmInst::LocalSet(inst.a as u32));
        }
        OpCode::Sub => {
            func.instruction(&WasmInst::LocalGet(inst.b as u32));
            func.instruction(&WasmInst::LocalGet(inst.c as u32));
            func.instruction(&WasmInst::I64Sub);
            func.instruction(&WasmInst::LocalSet(inst.a as u32));
        }
        OpCode::Mul => {
            func.instruction(&WasmInst::LocalGet(inst.b as u32));
            func.instruction(&WasmInst::LocalGet(inst.c as u32));
            func.instruction(&WasmInst::I64Mul);
            func.instruction(&WasmInst::LocalSet(inst.a as u32));
        }
        OpCode::Div => {
            func.instruction(&WasmInst::LocalGet(inst.b as u32));
            func.instruction(&WasmInst::LocalGet(inst.c as u32));
            func.instruction(&WasmInst::I64DivS);
            func.instruction(&WasmInst::LocalSet(inst.a as u32));
        }
        OpCode::Mod => {
            func.instruction(&WasmInst::LocalGet(inst.b as u32));
            func.instruction(&WasmInst::LocalGet(inst.c as u32));
            func.instruction(&WasmInst::I64RemS);
            func.instruction(&WasmInst::LocalSet(inst.a as u32));
        }
        OpCode::Neg => {
            func.instruction(&WasmInst::I64Const(0));
            func.instruction(&WasmInst::LocalGet(inst.b as u32));
            func.instruction(&WasmInst::I64Sub);
            func.instruction(&WasmInst::LocalSet(inst.a as u32));
        }
        OpCode::FloorDiv => {
            func.instruction(&WasmInst::LocalGet(inst.b as u32));
            func.instruction(&WasmInst::LocalGet(inst.c as u32));
            func.instruction(&WasmInst::I64DivS);
            func.instruction(&WasmInst::LocalSet(inst.a as u32));
        }
        OpCode::BitAnd => {
            func.instruction(&WasmInst::LocalGet(inst.b as u32));
            func.instruction(&WasmInst::LocalGet(inst.c as u32));
            func.instruction(&WasmInst::I64And);
            func.instruction(&WasmInst::LocalSet(inst.a as u32));
        }
        OpCode::BitOr => {
            func.instruction(&WasmInst::LocalGet(inst.b as u32));
            func.instruction(&WasmInst::LocalGet(inst.c as u32));
            func.instruction(&WasmInst::I64Or);
            func.instruction(&WasmInst::LocalSet(inst.a as u32));
        }
        OpCode::BitXor => {
            func.instruction(&WasmInst::LocalGet(inst.b as u32));
            func.instruction(&WasmInst::LocalGet(inst.c as u32));
            func.instruction(&WasmInst::I64Xor);
            func.instruction(&WasmInst::LocalSet(inst.a as u32));
        }
        OpCode::Shl => {
            func.instruction(&WasmInst::LocalGet(inst.b as u32));
            func.instruction(&WasmInst::LocalGet(inst.c as u32));
            func.instruction(&WasmInst::I64Shl);
            func.instruction(&WasmInst::LocalSet(inst.a as u32));
        }
        OpCode::Shr => {
            func.instruction(&WasmInst::LocalGet(inst.b as u32));
            func.instruction(&WasmInst::LocalGet(inst.c as u32));
            func.instruction(&WasmInst::I64ShrS);
            func.instruction(&WasmInst::LocalSet(inst.a as u32));
        }
        OpCode::Eq => {
            func.instruction(&WasmInst::LocalGet(inst.b as u32));
            func.instruction(&WasmInst::LocalGet(inst.c as u32));
            func.instruction(&WasmInst::I64Eq);
            func.instruction(&WasmInst::I64ExtendI32U);
            func.instruction(&WasmInst::LocalSet(inst.a as u32));
        }
        OpCode::Lt => {
            func.instruction(&WasmInst::LocalGet(inst.b as u32));
            func.instruction(&WasmInst::LocalGet(inst.c as u32));
            func.instruction(&WasmInst::I64LtS);
            func.instruction(&WasmInst::I64ExtendI32U);
            func.instruction(&WasmInst::LocalSet(inst.a as u32));
        }
        OpCode::Le => {
            func.instruction(&WasmInst::LocalGet(inst.b as u32));
            func.instruction(&WasmInst::LocalGet(inst.c as u32));
            func.instruction(&WasmInst::I64LeS);
            func.instruction(&WasmInst::I64ExtendI32U);
            func.instruction(&WasmInst::LocalSet(inst.a as u32));
        }
        OpCode::Not => {
            func.instruction(&WasmInst::LocalGet(inst.b as u32));
            func.instruction(&WasmInst::I64Eqz);
            func.instruction(&WasmInst::I64ExtendI32U);
            func.instruction(&WasmInst::LocalSet(inst.a as u32));
        }
        OpCode::And => {
            func.instruction(&WasmInst::LocalGet(inst.b as u32));
            func.instruction(&WasmInst::LocalGet(inst.c as u32));
            func.instruction(&WasmInst::I64And);
            func.instruction(&WasmInst::LocalSet(inst.a as u32));
        }
        OpCode::Or => {
            func.instruction(&WasmInst::LocalGet(inst.b as u32));
            func.instruction(&WasmInst::LocalGet(inst.c as u32));
            func.instruction(&WasmInst::I64Or);
            func.instruction(&WasmInst::LocalSet(inst.a as u32));
        }
        OpCode::Return => {
            func.instruction(&WasmInst::LocalGet(inst.a as u32));
            func.instruction(&WasmInst::Return);
        }
        OpCode::Nop => {
            // Skip
        }
        OpCode::Halt => {
            func.instruction(&WasmInst::Unreachable);
        }
        _ => {
            // Unsupported: emit unreachable
            func.instruction(&WasmInst::Unreachable);
        }
    }

    Ok(())
}

/// Load a constant from the cell's constant pool.
fn emit_wasm_load_constant(
    func: &mut Function,
    cell: &LirCell,
    const_idx: usize,
    dest_reg: u16,
) -> Result<(), CodegenError> {
    use lumen_core::lir::Constant;

    let constant = cell.constants.get(const_idx).ok_or_else(|| {
        CodegenError::LoweringError(format!(
            "wasm: constant index {const_idx} out of range (cell has {})",
            cell.constants.len()
        ))
    })?;

    match constant {
        Constant::Int(n) => {
            func.instruction(&WasmInst::I64Const(*n));
        }
        Constant::Float(f) => {
            func.instruction(&WasmInst::F64Const(*f));
            func.instruction(&WasmInst::I64ReinterpretF64);
        }
        Constant::Bool(b) => {
            func.instruction(&WasmInst::I64Const(*b as i64));
        }
        Constant::Null => {
            func.instruction(&WasmInst::I64Const(0));
        }
        Constant::String(_) => {
            func.instruction(&WasmInst::I64Const(0));
        }
        Constant::BigInt(_) => {
            func.instruction(&WasmInst::I64Const(0));
        }
    }

    func.instruction(&WasmInst::LocalSet(dest_reg as u32));
    Ok(())
}
