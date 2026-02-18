//! Simple optimization passes for LIR bytecode.
//!
//! Performs peephole optimizations on LIR instruction streams before lowering
//! to native code, eliminating redundant operations and improving JIT/AOT output.

use lumen_core::lir::{Instruction, LirCell, OpCode};

/// Optimize a LIR cell in-place by removing redundant instructions.
///
/// Current optimizations:
/// - Remove `Nop` instructions
///
/// NOTE: The Eq+Test optimization is disabled because JIT/AOT IR lowering
/// does not implement the VM's Eq skip-next semantics. Eq is always lowered
/// as a store-to-register operation, so removing Test breaks the logic.
pub fn optimize(cell: &mut LirCell) {
    remove_nops(cell);
    // optimize_eq_test_sequences(cell);  // Disabled - not supported by JIT/AOT
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
}
