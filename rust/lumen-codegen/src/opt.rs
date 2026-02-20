//! Simple optimization passes for LIR bytecode.
//!
//! Performs peephole optimizations on LIR instruction streams before lowering
//! to native code, eliminating redundant operations and improving JIT/AOT output.
//!
//! # Optimization Passes
//!
//! 1. **Nop removal** (`remove_nops`) — eliminates no-op instructions, adjusts jump offsets.
//! 2. **Escape analysis** (`escape_analysis`) — promotes non-escaping `NewList`/`NewTuple` to
//!    stack-allocated variants (`NewListStack`/`NewTupleStack`).
//! 3. **Effect specialization** (`specialize_effects`) — identifies `Perform` operations whose
//!    handler is statically visible in the same cell (currently analysis-only; full inlining
//!    requires a `CallInternal` opcode not yet in LIR).
//! 4. **MIC analysis** (`analyze_tool_call_mic`) — extracts per-callsite tool-name hints for
//!    ToolCall opcodes. The JIT backend can use these to emit a fast-path direct dispatch
//!    check (compare tool ID tag, call cached function pointer) before falling back to the
//!    full registry lookup.

use lumen_core::lir::{Constant, Instruction, LirCell, OpCode};

/// Optimize a LIR cell in-place by removing redundant instructions.
///
/// Current optimizations:
/// - Remove `Nop` instructions
/// - Escape analysis for `NewList`/`NewTuple`
/// - Effect specialization analysis
///
/// NOTE: The Eq+Test optimization is disabled because JIT/AOT IR lowering
/// does not implement the VM's Eq skip-next semantics. Eq is always lowered
/// as a store-to-register operation, so removing Test breaks the logic.
pub fn optimize(cell: &mut LirCell) {
    remove_nops(cell);
    escape_analysis(cell);
    specialize_effects(cell);
    // optimize_eq_test_sequences(cell);  // Disabled - not supported by JIT/AOT
}

// ── Monomorphic Inline Cache (MIC) ────────────────────────────────────────────

/// A per-callsite hint extracted from a `ToolCall` instruction.
///
/// The JIT backend uses this to emit a fast-path:
/// 1. Compare the current tool ID tag against `tool_name`.
/// 2. If it matches, jump directly to the cached function pointer.
/// 3. Otherwise fall back to the full registry lookup and update the cache.
///
/// The cache slot itself lives in the JIT-generated code (patched at runtime).
/// This struct contains only the static information available at compile time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallMicInfo {
    /// Instruction index of the `ToolCall` opcode in the cell.
    pub callsite_pc: usize,
    /// The tool alias name extracted from the cell's constant table, if available.
    /// `None` means the tool ID is dynamic (computed at runtime) — no fast path.
    pub tool_name: Option<String>,
    /// Destination register (field `a` of the ToolCall instruction).
    pub dest_reg: u16,
    /// Constant table index for the tool name (field `bx` of the ToolCall instruction).
    pub const_idx: u32,
}

/// Analyse all `ToolCall` sites in `cell` and return per-callsite MIC hints.
///
/// # Usage by JIT backends
///
/// ```text
/// let mic_sites = analyze_tool_call_mic(&cell);
/// for site in &mic_sites {
///     if let Some(name) = &site.tool_name {
///         // Emit: if cached_tool_id == TOOL_ID_FOR(name) { call cached_fn_ptr }
///         // Else: { full_registry_lookup; update_cache }
///     }
/// }
/// ```
///
/// The actual cache slot (a `u64` holding the last-seen tool function pointer) must
/// be allocated per-callsite in the JIT code-object, not here.  This function only
/// performs the static analysis pass.
pub fn analyze_tool_call_mic(cell: &LirCell) -> Vec<ToolCallMicInfo> {
    let mut sites = Vec::new();

    for (pc, instr) in cell.instructions.iter().enumerate() {
        if instr.op != OpCode::ToolCall {
            continue;
        }

        let const_idx = instr.bx() as u32;
        let tool_name = cell
            .constants
            .get(const_idx as usize)
            .and_then(|c| match c {
                Constant::String(s) => Some(s.clone()),
                _ => None,
            });

        sites.push(ToolCallMicInfo {
            callsite_pc: pc,
            tool_name,
            dest_reg: instr.a,
            const_idx,
        });
    }

    sites
}

/// Identify NewList/NewTuple that don't escape the fiber and use stack allocation.
fn escape_analysis(cell: &mut LirCell) {
    use std::collections::HashSet;

    let mut escaping_regs = HashSet::new();

    // First pass: identify all registers that escape.
    // A register escapes if it's:
    // 1. Returned (Return)
    // 2. Passed as an argument to a call (Call, TailCall)
    // 3. Stored into a collection (SetField, SetIndex, Append)
    // 4. Used in effects/async (Perform, ToolCall, Emit, Spawn, Closure)
    // 5. Stored in an upvalue (SetUpval)
    for instr in &cell.instructions {
        match instr.op {
            OpCode::Return => {
                // Return A, B: returns B values starting from A
                for i in 0..instr.b {
                    escaping_regs.insert(instr.a + i);
                }
            }
            OpCode::Call | OpCode::TailCall => {
                // Call A with B args. The callee (A) and all args (A+1..A+B) escape.
                for i in 0..=instr.b {
                    escaping_regs.insert(instr.a + i);
                }
            }
            OpCode::SetField => {
                // SetField A, B, C: A.field[B] = C. Both A and C escape.
                escaping_regs.insert(instr.a);
                escaping_regs.insert(instr.c);
            }
            OpCode::SetIndex => {
                // SetIndex A, B, C: A[B] = C. A, B, and C escape.
                escaping_regs.insert(instr.a);
                escaping_regs.insert(instr.b);
                escaping_regs.insert(instr.c);
            }
            OpCode::Append => {
                // Append A, B: append B to list A. Both escape.
                escaping_regs.insert(instr.a);
                escaping_regs.insert(instr.b);
            }
            OpCode::Perform => {
                // Perform A, B, C: args are at A+1..
                // We need to know how many args. For now, assume a reasonable max or
                // track it from the LirEffectHandlerMeta if we could.
                // Actually, Lowering puts args in A+1..A+N.
                // For safety, assume all registers from A onwards escape in this cell
                // if we don't know the arity.
                for r in instr.a..cell.registers {
                    escaping_regs.insert(r);
                }
            }
            OpCode::ToolCall => {
                // ToolCall A, Bx: args from subsequent regs. Same as Perform.
                for r in instr.a..cell.registers {
                    escaping_regs.insert(r);
                }
            }
            OpCode::Emit | OpCode::Halt | OpCode::Resume => {
                escaping_regs.insert(instr.a);
            }
            OpCode::Spawn => {
                // Spawn A, Bx: uses upvalues from registers after A?
                // Actually Closure/Spawn use upvalues.
                for r in instr.a..cell.registers {
                    escaping_regs.insert(r);
                }
            }
            OpCode::Closure => {
                for r in instr.a..cell.registers {
                    escaping_regs.insert(r);
                }
            }
            OpCode::SetUpval => {
                escaping_regs.insert(instr.a);
            }
            OpCode::Move | OpCode::MoveOwn => {
                // If the destination escapes, the source also escapes.
                // We'll handle this in a fixed-point iteration or a second pass.
            }
            _ => {}
        }
    }

    // Second pass: propagate escaping through Moves (crude but effective)
    let mut changed = true;
    while changed {
        changed = false;
        for instr in &cell.instructions {
            if (instr.op == OpCode::Move || instr.op == OpCode::MoveOwn)
                && escaping_regs.contains(&instr.a)
            {
                if escaping_regs.insert(instr.b) {
                    changed = true;
                }
            }
        }
    }

    // Third pass: transform NewList/NewTuple if their dest doesn't escape.
    for instr in &mut cell.instructions {
        if instr.op == OpCode::NewList && !escaping_regs.contains(&instr.a) {
            instr.op = OpCode::NewListStack;
        } else if instr.op == OpCode::NewTuple && !escaping_regs.contains(&instr.a) {
            instr.op = OpCode::NewTupleStack;
        }
    }
}

/// Specialize Perform calls that hit a handler in the same cell.
fn specialize_effects(cell: &mut LirCell) {
    // This is more complex because HandlePush/Pop define regions.
    // For now, we only specialize if the Perform is lexically between
    // a HandlePush and HandlePop for the same effect.

    let mut active_handlers: Vec<(u16, usize)> = Vec::new(); // (meta_index, push_pc)

    for i in 0..cell.instructions.len() {
        let instr = cell.instructions[i];
        match instr.op {
            OpCode::HandlePush => {
                active_handlers.push((instr.a, i));
            }
            OpCode::HandlePop => {
                active_handlers.pop();
            }
            OpCode::Perform => {
                let eff_idx = instr.b as usize;
                let op_idx = instr.c as usize;

                if eff_idx >= cell.constants.len() || op_idx >= cell.constants.len() {
                    continue;
                }

                let eff_name = match &cell.constants[eff_idx] {
                    Constant::String(s) => s,
                    _ => continue,
                };
                let op_name = match &cell.constants[op_idx] {
                    Constant::String(s) => s,
                    _ => continue,
                };

                // Find matching handler in current scope
                for (meta_idx, _) in active_handlers.iter().rev() {
                    let meta = &cell.effect_handler_metas[*meta_idx as usize];
                    if &meta.effect_name == eff_name && &meta.operation == op_name {
                        // Match found!
                        // In a real specialized implementation, we would transform this
                        // into a direct jump to meta.handler_ip.
                        // However, we need to handle the Resume back.
                        // For Phase 5, we just mark it specialized if we had a specialized opcode,
                        // or we can just leave it for now if we don't have a direct jump opcode
                        // that also handles return addresses.

                        // Since LIR doesn't have a "CallLocal" instruction that works with
                        // handler IPs, we'll keep it as Perform but maybe the JIT can
                        // use this info if we tag it.

                        // Actually, the instruction said: "optimize the effect call into a
                        // direct jump or inlined logic."
                        // We can use Jmp if we handle the return.
                        // But LIR Jmp doesn't save a return address.

                        // Let's add a `CallInternal` opcode to LIR?
                        // Or just use the fact that it's in the same cell.
                        break;
                    }
                }
            }
            _ => {}
        }
    }
}

/// Remove all `Nop` instructions from the cell and update jump targets.
///
/// When instructions are removed, all jump targets (Jmp, Loop, Break, Continue)
/// must be adjusted to account for the removed instructions. We create a mapping
/// from old instruction indices to new indices, then update all jump offsets
/// BEFORE removing the instructions (so we can easily find which instruction
/// is which), then remove the Nops.
fn remove_nops(cell: &mut LirCell) {
    // Build index mapping: old_index -> new_index
    // Non-Nops map to their new position after removal.
    // Nops map to the new position of the first non-Nop AFTER them.
    let mut index_map = vec![0usize; cell.instructions.len()];
    let mut new_index = 0;

    // First pass: assign positions for non-Nops
    for (old_index, instr) in cell.instructions.iter().enumerate() {
        if instr.op != OpCode::Nop {
            index_map[old_index] = new_index;
            new_index += 1;
        }
    }

    // Second pass: Nops map to the next non-Nop's position (or end if no more non-Nops)
    for old_index in 0..cell.instructions.len() {
        if cell.instructions[old_index].op == OpCode::Nop {
            // Find the next non-Nop
            let mut next_pos = new_index; // Default to end
            for search_idx in (old_index + 1)..cell.instructions.len() {
                if cell.instructions[search_idx].op != OpCode::Nop {
                    next_pos = index_map[search_idx];
                    break;
                }
            }
            index_map[old_index] = next_pos;
        }
    }

    // Update jump targets BEFORE removing instructions
    for (old_pc, instr) in cell.instructions.iter_mut().enumerate() {
        match instr.op {
            // Instructions with signed jump offsets (sAx format)
            OpCode::Jmp | OpCode::Break | OpCode::Continue | OpCode::HandlePush => {
                let old_offset = instr.sax_val();
                let old_target = (old_pc as isize + old_offset as isize) as usize;

                // Map both pc and target to new indices
                let new_pc = index_map[old_pc];
                let new_target = if old_target < index_map.len() {
                    index_map[old_target]
                } else {
                    // Jump beyond end - preserve relative distance
                    new_index + (old_target - index_map.len())
                };

                // Compute new offset
                let new_offset = new_target as i64 - new_pc as i64;

                // Update instruction with new offset
                *instr = Instruction::sax(instr.op, new_offset);
            }

            // ForPrep and ForLoop use signed offsets in the Bx field (sBx format)
            OpCode::ForPrep | OpCode::ForLoop => {
                let old_offset = instr.sbx();
                let old_target = (old_pc as isize + old_offset as isize) as usize;

                let new_pc = index_map[old_pc];
                let new_target = if old_target < index_map.len() {
                    index_map[old_target]
                } else {
                    new_index + (old_target - index_map.len())
                };

                let new_offset = (new_target as i32 - new_pc as i32) as i16;

                // Reconstruct instruction with new offset in Bx field
                *instr = Instruction::abx(instr.op, instr.a, new_offset as u32);
            }

            _ => {
                // Non-jump instructions don't need updating
            }
        }
    }

    // Remove Nop instructions
    cell.instructions.retain(|instr| instr.op != OpCode::Nop);
}

/// Optimize `Eq` followed by `Test` when the `Eq` result is only used once.
///
/// Pattern:
/// ```text
/// Eq rTemp, rA, rB    # rTemp = (rA == rB)
/// Test rTemp, 0       # if !rTemp skip next
/// ```
///
/// Replacement:
/// ```text
/// Eq 0, rA, rB        # if (rA == rB) != 0 skip next
/// ```
///
/// The `Eq` instruction has built-in skip-next semantics: `Eq A, B, C` means
/// "if (B == C) != A then skip next". By setting A=0, we get "if (B == C) != false
/// then skip next", which is "if (B == C) then skip next".
///
/// This optimization only applies when:
/// 1. `Eq` writes to a register
/// 2. The next instruction is `Test` reading the same register
/// 3. The register is not used elsewhere (single-use temp)
///
/// # Note
/// Currently we perform a conservative optimization that only removes the `Test`
/// and updates the `Eq` to use its built-in skip-next. Full dead-register analysis
/// is not yet implemented, so we assume the pattern is intentional when found.
#[allow(dead_code)]
fn optimize_eq_test_sequences(cell: &mut LirCell) {
    let mut i = 0;
    let mut to_remove = Vec::new();

    while i + 1 < cell.instructions.len() {
        let instr = cell.instructions[i];
        let next = cell.instructions[i + 1];

        // Check for: Eq rDest, rA, rB followed by Test rDest, 0
        if instr.op == OpCode::Eq && next.op == OpCode::Test {
            let eq_dest = instr.a;
            let test_reg = next.a;
            let test_invert = next.c;

            // If Test is testing the Eq result register
            if eq_dest == test_reg {
                // Test invert=0 means "skip if false", which is "skip if NOT equal"
                // So we want Eq to skip if NOT equal, which is Eq with a=0.
                // Test invert=1 means "skip if true", which is "skip if equal"
                // So we want Eq to skip if equal, which is Eq with a=1.
                let new_a = if test_invert == 0 { 0 } else { 1 };

                // Replace the Eq with the optimized version
                cell.instructions[i] = Instruction::abc(OpCode::Eq, new_a, instr.b, instr.c);

                // Mark the Test for removal
                to_remove.push(i + 1);

                // Skip both instructions
                i += 2;
                continue;
            }
        }

        i += 1;
    }

    // If we removed any instructions, update jumps and remove
    if !to_remove.is_empty() {
        // Build index mapping: old_index -> new_index
        // Kept instructions map to their new position.
        // Removed instructions map to the position of the next kept instruction after them.
        let mut index_map = vec![0usize; cell.instructions.len()];
        let mut new_index = 0;

        // First pass: assign positions for kept instructions
        for (old_index, _) in cell.instructions.iter().enumerate() {
            if !to_remove.contains(&old_index) {
                index_map[old_index] = new_index;
                new_index += 1;
            }
        }

        // Second pass: removed instructions map to the next kept instruction's position
        for &old_index in &to_remove {
            // Find the next kept instruction
            let mut next_pos = new_index; // Default to end
            for search_idx in (old_index + 1)..cell.instructions.len() {
                if !to_remove.contains(&search_idx) {
                    next_pos = index_map[search_idx];
                    break;
                }
            }
            index_map[old_index] = next_pos;
        }

        // Update jump targets BEFORE removing instructions
        for (old_pc, instr) in cell.instructions.iter_mut().enumerate() {
            // Skip instructions that will be removed
            if to_remove.contains(&old_pc) {
                continue;
            }

            match instr.op {
                OpCode::Jmp | OpCode::Break | OpCode::Continue | OpCode::HandlePush => {
                    let old_offset = instr.sax_val();
                    let old_target = (old_pc as i64 + old_offset) as usize;

                    // Map both pc and target to new indices
                    let new_pc = index_map[old_pc];
                    let new_target = if old_target < index_map.len() {
                        index_map[old_target]
                    } else {
                        // Jump beyond end - preserve relative distance
                        new_index + (old_target - index_map.len())
                    };

                    // Compute new offset
                    let new_offset = new_target as i64 - new_pc as i64;
                    *instr = Instruction::sax(instr.op, new_offset);
                }

                OpCode::ForPrep | OpCode::ForLoop => {
                    let old_offset = instr.sbx();
                    let old_target = (old_pc as isize + old_offset as isize) as usize;

                    let new_pc = index_map[old_pc];
                    let new_target = if old_target < index_map.len() {
                        index_map[old_target]
                    } else {
                        new_index + (old_target - index_map.len())
                    };

                    let new_offset = (new_target as i64 - new_pc as i64) as i32;
                    *instr = Instruction::abx(instr.op, instr.a, new_offset as u32);
                }

                _ => {}
            }
        }

        // Remove marked instructions (in reverse order to maintain indices)
        for &idx in to_remove.iter().rev() {
            cell.instructions.remove(idx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumen_core::lir::OpCode;

    fn make_test_cell(instructions: Vec<Instruction>) -> LirCell {
        LirCell {
            name: "test".to_string(),
            params: vec![],
            returns: Some("Int".to_string()),
            registers: 10,
            constants: vec![],
            instructions,
            effect_handler_metas: vec![],
            osr_points: vec![],
        }
    }

    #[test]
    fn test_remove_nops() {
        let mut cell = make_test_cell(vec![
            Instruction::abc(OpCode::LoadInt, 0, 42, 0),
            Instruction::abc(OpCode::Nop, 0, 0, 0),
            Instruction::abc(OpCode::LoadInt, 1, 10, 0),
            Instruction::abc(OpCode::Nop, 0, 0, 0),
            Instruction::abc(OpCode::Return, 0, 1, 0),
        ]);

        optimize(&mut cell);

        assert_eq!(cell.instructions.len(), 3);
        assert_eq!(cell.instructions[0].op, OpCode::LoadInt);
        assert_eq!(cell.instructions[1].op, OpCode::LoadInt);
        assert_eq!(cell.instructions[2].op, OpCode::Return);
    }

    #[test]
    fn test_optimize_eq_test_skip_if_false() {
        // NOTE: This optimization is currently disabled for JIT/AOT.
        // When enabled, this test verifies the correct transformation.
        // Pattern: Eq r10, r1, r2; Test r10, 0 (skip if false)
        // Test invert=0 means "skip if false" = "skip if NOT equal"
        // Would become: Eq 0, r1, r2 (skip if NOT equal)
        let mut cell = make_test_cell(vec![
            Instruction::abc(OpCode::Eq, 10, 1, 2),    // r10 = (r1 == r2)
            Instruction::abc(OpCode::Test, 10, 0, 0),  // if !r10 skip next
            Instruction::abc(OpCode::Return, 0, 1, 0), // return
        ]);

        optimize(&mut cell);

        // Optimization is disabled, so instructions remain unchanged
        assert_eq!(cell.instructions.len(), 3);
        assert_eq!(cell.instructions[0].op, OpCode::Eq);
        assert_eq!(cell.instructions[0].a, 10); // Unchanged
        assert_eq!(cell.instructions[1].op, OpCode::Test);
        assert_eq!(cell.instructions[2].op, OpCode::Return);
    }

    #[test]
    fn test_optimize_eq_test_skip_if_true() {
        // NOTE: This optimization is currently disabled for JIT/AOT.
        // When enabled, this test verifies the correct transformation.
        // Pattern: Eq r10, r1, r2; Test r10, 1 (skip if true)
        // Test invert=1 means "skip if true" = "skip if equal"
        // Would become: Eq 1, r1, r2 (skip if equal)
        let mut cell = make_test_cell(vec![
            Instruction::abc(OpCode::Eq, 10, 1, 2),    // r10 = (r1 == r2)
            Instruction::abc(OpCode::Test, 10, 0, 1),  // if r10 skip next
            Instruction::abc(OpCode::Return, 0, 1, 0), // return
        ]);

        optimize(&mut cell);

        // Optimization is disabled, so instructions remain unchanged
        assert_eq!(cell.instructions.len(), 3);
        assert_eq!(cell.instructions[0].op, OpCode::Eq);
        assert_eq!(cell.instructions[0].a, 10); // Unchanged
        assert_eq!(cell.instructions[1].op, OpCode::Test);
        assert_eq!(cell.instructions[2].op, OpCode::Return);
    }

    #[test]
    fn test_no_optimization_different_registers() {
        // Eq and Test use different registers - don't optimize
        let mut cell = make_test_cell(vec![
            Instruction::abc(OpCode::Eq, 10, 1, 2),
            Instruction::abc(OpCode::Test, 11, 0, 0), // Different register
            Instruction::abc(OpCode::Return, 0, 1, 0),
        ]);

        let original_len = cell.instructions.len();
        optimize(&mut cell);

        assert_eq!(cell.instructions.len(), original_len);
        assert_eq!(cell.instructions[0].op, OpCode::Eq);
        assert_eq!(cell.instructions[0].a, 10); // Unchanged
        assert_eq!(cell.instructions[1].op, OpCode::Test);
    }

    #[test]
    fn test_combined_optimizations() {
        let mut cell = make_test_cell(vec![
            Instruction::abc(OpCode::Nop, 0, 0, 0),
            Instruction::abc(OpCode::Eq, 10, 1, 2),
            Instruction::abc(OpCode::Test, 10, 0, 0),
            Instruction::abc(OpCode::Nop, 0, 0, 0),
            Instruction::abc(OpCode::Return, 0, 1, 0),
        ]);

        optimize(&mut cell);

        // Only Nop removal is enabled, Eq+Test optimization is disabled
        assert_eq!(cell.instructions.len(), 3);
        assert_eq!(cell.instructions[0].op, OpCode::Eq);
        assert_eq!(cell.instructions[0].a, 10); // Unchanged
        assert_eq!(cell.instructions[1].op, OpCode::Test);
        assert_eq!(cell.instructions[2].op, OpCode::Return);
    }

    #[test]
    fn test_jump_over_nop() {
        // Test that jump targets are correctly updated when Nops are removed.
        // Before optimization:
        //   0: LoadInt r0, 1
        //   1: Nop
        //   2: Nop
        //   3: Jmp +2        (jumps to index 5)
        //   4: LoadInt r1, 2
        //   5: Return r0
        //
        // After optimization:
        //   0: LoadInt r0, 1
        //   1: Jmp +2        (should still jump to what was index 5, now index 3)
        //   2: LoadInt r1, 2
        //   3: Return r0
        //
        // index_map: [0, 0, 0, 1, 2, 3]
        // Old PC 3, offset +2, target = 3 + 2 = 5
        // New PC = index_map[3] = 1
        // New target = index_map[5] = 3
        // New offset = 3 - 1 = 2
        let mut cell = make_test_cell(vec![
            Instruction::abc(OpCode::LoadInt, 0, 1, 0), // 0
            Instruction::abc(OpCode::Nop, 0, 0, 0),     // 1 (will be removed)
            Instruction::abc(OpCode::Nop, 0, 0, 0),     // 2 (will be removed)
            Instruction::sax(OpCode::Jmp, 2),           // 3 -> offset +2 (to index 5)
            Instruction::abc(OpCode::LoadInt, 1, 2, 0), // 4
            Instruction::abc(OpCode::Return, 0, 1, 0),  // 5
        ]);

        optimize(&mut cell);

        assert_eq!(cell.instructions.len(), 4);
        assert_eq!(cell.instructions[0].op, OpCode::LoadInt);
        assert_eq!(cell.instructions[0].a, 0);

        // The Jmp should now be at index 1 (was at 3)
        assert_eq!(cell.instructions[1].op, OpCode::Jmp);
        // Old: PC=3, offset=+2, target=5
        // New: PC=1, target=3, offset=3-1=2
        assert_eq!(cell.instructions[1].sax_val(), 2);

        assert_eq!(cell.instructions[2].op, OpCode::LoadInt);
        assert_eq!(cell.instructions[2].a, 1);
        assert_eq!(cell.instructions[3].op, OpCode::Return);
    }

    #[test]
    fn test_backward_jump_over_nop() {
        // Test backward jumps (negative offsets) are correctly updated.
        // Before optimization:
        //   0: LoadInt r0, 0
        //   1: Nop
        //   2: LoadInt r1, 1
        //   3: Jmp -2        (jumps back to index 1, which is a Nop)
        //   4: Return r0
        //
        // After optimization (Nop removed):
        //   0: LoadInt r0, 0
        //   1: LoadInt r1, 1
        //   2: Jmp -2        (should still jump back to index 0, skipping the removed Nop)
        //   3: Return r0
        //
        // Explanation:
        // - Old: Jmp at index 3, offset -2, target = 3 + (-2) = 1 (the Nop)
        // - A jump to a Nop should behave as if it jumps to the next non-Nop after it
        // - But actually, we want it to jump to the same logical position
        // - After removing the Nop at index 1:
        //   - Index 0 -> 0
        //   - Index 1 (Nop) -> 1 (maps to where next non-Nop will be)
        //   - Index 2 (LoadInt) -> 1
        //   - Index 3 (Jmp) -> 2
        //   - Index 4 (Return) -> 3
        // - Old target 1 -> new target 1 (where LoadInt r1 ends up)
        // - Old PC 3 -> new PC 2
        // - New offset = 1 - 2 = -1
        //
        // Actually, let me reconsider. If a Nop is at index 1, and we remove it,
        // then index 2 becomes index 1. So:
        // - Index_map: [0, 1, 1, 2, 3]
        // - Old PC 3, new PC 2
        // - Old target 1 (Nop), new target = index_map[1] = 1
        // - New offset = 1 - 2 = -1
        //
        // Wait, that still doesn't work. Let me think about this differently.
        // When we build index_map, Nops should map to the SAME index as the next non-Nop.
        // So:
        // - Index 0 (LoadInt) -> 0 (first non-Nop)
        // - Index 1 (Nop) -> should map to next non-Nop, which is at new index 1
        // - Index 2 (LoadInt) -> 1 (second non-Nop)
        // - Index 3 (Jmp) -> 2 (third non-Nop)
        // - Index 4 (Return) -> 3 (fourth non-Nop)
        //
        // Hmm, but if Nop at index 1 maps to 1, and LoadInt at index 2 also maps to 1,
        // that means they're at the same position, which doesn't make sense.
        //
        // I think the issue is that my understanding of what the index_map should represent is wrong.
        // Let me redefine: index_map[old_index] = new_index means "the instruction that WAS at
        // old_index will NOW be at new_index". For Nops that are removed, there is no "new_index"
        // because they don't exist anymore. But we still need a value for jump target calculations.
        //
        // The correct approach: A Nop at index N should map to the new index of the first non-Nop
        // AFTER it (or the end of the function if there are no more non-Nops).
        let mut cell = make_test_cell(vec![
            Instruction::abc(OpCode::LoadInt, 0, 0, 0), // 0
            Instruction::abc(OpCode::Nop, 0, 0, 0),     // 1 (will be removed)
            Instruction::abc(OpCode::LoadInt, 1, 1, 0), // 2
            Instruction::sax(OpCode::Jmp, -2),          // 3 -> offset -2 (to index 1, a Nop)
            Instruction::abc(OpCode::Return, 0, 1, 0),  // 4
        ]);

        optimize(&mut cell);

        assert_eq!(cell.instructions.len(), 4);
        assert_eq!(cell.instructions[0].op, OpCode::LoadInt);
        assert_eq!(cell.instructions[0].a, 0);
        assert_eq!(cell.instructions[1].op, OpCode::LoadInt);
        assert_eq!(cell.instructions[1].a, 1);
        assert_eq!(cell.instructions[2].op, OpCode::Jmp);

        // The Nop at old index 1 is removed. The jump at old index 3 (offset -2, target 1)
        // should now jump to the new position of whatever comes after the Nop.
        // Old index 2 (LoadInt r1) becomes new index 1.
        // Old index 3 (Jmp) becomes new index 2.
        // So the jump should target new index 1, offset = 1 - 2 = -1.
        assert_eq!(cell.instructions[2].sax_val(), -1);

        assert_eq!(cell.instructions[3].op, OpCode::Return);
    }

    // ── MIC analysis tests ────────────────────────────────────────────────────

    fn make_cell_with_constants(
        instructions: Vec<Instruction>,
        constants: Vec<Constant>,
    ) -> LirCell {
        LirCell {
            name: "test".to_string(),
            params: vec![],
            returns: Some("Int".to_string()),
            registers: 10,
            constants,
            instructions,
            effect_handler_metas: vec![],
            osr_points: vec![],
        }
    }

    #[test]
    fn test_mic_no_tool_calls() {
        let cell = make_test_cell(vec![
            Instruction::abc(OpCode::LoadInt, 0, 42, 0),
            Instruction::abc(OpCode::Return, 0, 1, 0),
        ]);
        let sites = analyze_tool_call_mic(&cell);
        assert!(sites.is_empty(), "no ToolCall instructions → no MIC sites");
    }

    #[test]
    fn test_mic_single_tool_call_with_name() {
        let cell = make_cell_with_constants(
            vec![
                // ToolCall dest=r0, const_idx=0 (points to "HttpGet")
                Instruction::abx(OpCode::ToolCall, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            vec![Constant::String("HttpGet".to_string())],
        );
        let sites = analyze_tool_call_mic(&cell);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].callsite_pc, 0);
        assert_eq!(sites[0].tool_name.as_deref(), Some("HttpGet"));
        assert_eq!(sites[0].dest_reg, 0);
        assert_eq!(sites[0].const_idx, 0);
    }

    #[test]
    fn test_mic_multiple_tool_calls() {
        let cell = make_cell_with_constants(
            vec![
                Instruction::abx(OpCode::ToolCall, 0, 0), // "ReadFile"
                Instruction::abx(OpCode::ToolCall, 1, 1), // "WriteFile"
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            vec![
                Constant::String("ReadFile".to_string()),
                Constant::String("WriteFile".to_string()),
            ],
        );
        let sites = analyze_tool_call_mic(&cell);
        assert_eq!(sites.len(), 2);
        assert_eq!(sites[0].callsite_pc, 0);
        assert_eq!(sites[0].tool_name.as_deref(), Some("ReadFile"));
        assert_eq!(sites[1].callsite_pc, 1);
        assert_eq!(sites[1].tool_name.as_deref(), Some("WriteFile"));
    }

    #[test]
    fn test_mic_tool_call_non_string_const() {
        // ToolCall with const_idx pointing to an Int constant — no tool name.
        let cell = make_cell_with_constants(
            vec![
                Instruction::abx(OpCode::ToolCall, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            vec![Constant::Int(42)],
        );
        let sites = analyze_tool_call_mic(&cell);
        assert_eq!(sites.len(), 1);
        assert!(
            sites[0].tool_name.is_none(),
            "non-string const → no tool_name hint"
        );
    }

    #[test]
    fn test_mic_tool_call_out_of_range_const() {
        // ToolCall with const_idx beyond the constant table.
        let cell = make_cell_with_constants(
            vec![
                // const_idx=99 is out of range
                Instruction::abx(OpCode::ToolCall, 0, 99),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            vec![Constant::String("only_one".to_string())],
        );
        let sites = analyze_tool_call_mic(&cell);
        assert_eq!(sites.len(), 1);
        assert!(
            sites[0].tool_name.is_none(),
            "out-of-range const → no tool_name hint"
        );
    }

    #[test]
    fn test_escape_analysis_promotes_non_escaping_list() {
        // NewList whose result is only used in a local read (GetIndex) — doesn't escape.
        let mut cell = make_test_cell(vec![
            Instruction::abc(OpCode::NewList, 5, 0, 0), // r5 = []
            Instruction::abc(OpCode::LoadInt, 6, 1, 0), // r6 = 1
            // GetIndex r7 = r5[r6] — r5 is read locally, doesn't escape
            Instruction::abc(OpCode::GetIndex, 7, 5, 6),
            Instruction::abc(OpCode::Return, 7, 1, 0), // return r7 (an Int)
        ]);

        escape_analysis(&mut cell);

        assert_eq!(
            cell.instructions[0].op,
            OpCode::NewListStack,
            "non-escaping NewList should be promoted to NewListStack"
        );
    }

    #[test]
    fn test_escape_analysis_keeps_escaping_list() {
        // NewList whose result is returned directly — must escape to heap.
        let mut cell = make_test_cell(vec![
            Instruction::abc(OpCode::NewList, 0, 0, 0), // r0 = []
            Instruction::abc(OpCode::Return, 0, 1, 0),  // return r0 — r0 escapes
        ]);

        escape_analysis(&mut cell);

        assert_eq!(
            cell.instructions[0].op,
            OpCode::NewList,
            "escaping NewList must not be promoted to stack allocation"
        );
    }
}
