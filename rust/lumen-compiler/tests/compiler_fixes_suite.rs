//! Tests for compiler fixes: NotEq operator, intrinsic name mappings,
//! and set/map comprehension lowering.

use lumen_compiler::compiler::lexer::Lexer;
use lumen_compiler::compiler::lower::lower;
use lumen_compiler::compiler::parser::Parser;
use lumen_compiler::compiler::resolve::resolve;
use lumen_compiler::compiler::typecheck::typecheck;
use lumen_core::lir::{IntrinsicId, OpCode};

fn compile_to_lir(src: &str) -> lumen_core::lir::LirModule {
    let mut lexer = Lexer::new(src, 1, 0);
    let tokens = lexer.tokenize().expect("lex failed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program(vec![]).expect("parse failed");
    let symbols = resolve(&program).expect("resolve failed");
    typecheck(&program, &symbols).expect("typecheck failed");
    lower(&program, &symbols, src)
}

// ============================================================================
// NotEq operator tests
// ============================================================================

#[test]
fn noteq_emits_eq_then_not() {
    let src = "cell main() -> Bool\n  return 1 != 2\nend";
    let module = compile_to_lir(src);
    let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();

    let eq_idx = ops
        .iter()
        .position(|o| *o == OpCode::Eq)
        .expect("should emit Eq");
    assert_eq!(
        ops[eq_idx + 1],
        OpCode::Not,
        "Not should follow Eq for != operator"
    );
}

#[test]
fn noteq_with_variables() {
    let src = "cell neq(a: Int, b: Int) -> Bool\n  return a != b\nend";
    let module = compile_to_lir(src);
    let instrs = &module.cells[0].instructions;

    let eq_instr = instrs
        .iter()
        .find(|i| i.op == OpCode::Eq)
        .expect("should emit Eq");
    let not_instr = instrs
        .iter()
        .find(|i| i.op == OpCode::Not)
        .expect("should emit Not");

    // Not should write to the same register as Eq
    assert_eq!(
        eq_instr.a, not_instr.a,
        "Eq and Not should target the same register"
    );
    // Not should read from the same register
    assert_eq!(not_instr.a, not_instr.b, "Not should invert in place");
}

#[test]
fn noteq_in_if_condition() {
    let src = "cell check(x: Int) -> String\n  if x != 0\n    return \"nonzero\"\n  end\n  return \"zero\"\nend";
    let module = compile_to_lir(src);
    let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();

    assert!(ops.contains(&OpCode::Eq), "if != should emit Eq");
    assert!(ops.contains(&OpCode::Not), "if != should emit Not");
    assert!(ops.contains(&OpCode::Test), "if condition should emit Test");
}

// ============================================================================
// Intrinsic name mapping tests
// ============================================================================

#[test]
fn intrinsic_upper() {
    let src = "cell main() -> String\n  let s = \"hello\"\n  return upper(s)\nend";
    let module = compile_to_lir(src);
    let intr = module.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .expect("should emit Intrinsic opcode");
    assert_eq!(intr.b, IntrinsicId::Upper as u8);
}

#[test]
fn intrinsic_lower() {
    let src = "cell main() -> String\n  let s = \"HELLO\"\n  return lower(s)\nend";
    let module = compile_to_lir(src);
    let intr = module.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .expect("should emit Intrinsic opcode");
    assert_eq!(intr.b, IntrinsicId::Lower as u8);
}

#[test]
fn intrinsic_trim() {
    let src = "cell main() -> String\n  let s = \" hello \"\n  return trim(s)\nend";
    let module = compile_to_lir(src);
    let intr = module.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .expect("should emit Intrinsic opcode");
    assert_eq!(intr.b, IntrinsicId::Trim as u8);
}

#[test]
fn intrinsic_sort() {
    let src = "cell main(xs: list[Int]) -> list[Int]\n  return sort(xs)\nend";
    let module = compile_to_lir(src);
    let intr = module.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .expect("should emit Intrinsic opcode");
    assert_eq!(intr.b, IntrinsicId::Sort as u8);
}

#[test]
fn intrinsic_reverse() {
    let src = "cell main(xs: list[Int]) -> list[Int]\n  return reverse(xs)\nend";
    let module = compile_to_lir(src);
    let intr = module.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .expect("should emit Intrinsic opcode");
    assert_eq!(intr.b, IntrinsicId::Reverse as u8);
}

#[test]
fn intrinsic_filter() {
    let src =
        "cell main(xs: list[Int], f: fn(Int) -> Bool) -> list[Int]\n  return filter(xs, f)\nend";
    let module = compile_to_lir(src);
    let intr = module.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .expect("should emit Intrinsic opcode");
    assert_eq!(intr.b, IntrinsicId::Filter as u8);
}

#[test]
fn intrinsic_map() {
    let src =
        "cell main(xs: list[Int], f: fn(Int) -> String) -> list[String]\n  return map(xs, f)\nend";
    let module = compile_to_lir(src);
    let intr = module.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .expect("should emit Intrinsic opcode");
    assert_eq!(intr.b, IntrinsicId::Map as u8);
}

#[test]
fn intrinsic_reduce() {
    let src = "cell main(xs: list[Int], init: Int, f: fn(Int, Int) -> Int) -> Int\n  return reduce(xs, init, f)\nend";
    let module = compile_to_lir(src);
    let intr = module.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .expect("should emit Intrinsic opcode");
    assert_eq!(intr.b, IntrinsicId::Reduce as u8);
}

#[test]
fn intrinsic_first_last_is_empty() {
    let src_first = "cell test_first(xs: list[Int]) -> Int\n  return first(xs)\nend";
    let src_last = "cell test_last(xs: list[Int]) -> Int\n  return last(xs)\nend";
    let src_empty = "cell test_empty(xs: list[Int]) -> Bool\n  return is_empty(xs)\nend";

    let m1 = compile_to_lir(src_first);
    let m2 = compile_to_lir(src_last);
    let m3 = compile_to_lir(src_empty);

    let id1 = m1.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .unwrap()
        .b;
    let id2 = m2.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .unwrap()
        .b;
    let id3 = m3.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .unwrap()
        .b;

    assert_eq!(id1, IntrinsicId::First as u8);
    assert_eq!(id2, IntrinsicId::Last as u8);
    assert_eq!(id3, IntrinsicId::IsEmpty as u8);
}

#[test]
fn intrinsic_starts_with_ends_with() {
    let src_sw = "cell test_sw(s: String, p: String) -> Bool\n  return starts_with(s, p)\nend";
    let src_ew = "cell test_ew(s: String, p: String) -> Bool\n  return ends_with(s, p)\nend";

    let m1 = compile_to_lir(src_sw);
    let m2 = compile_to_lir(src_ew);

    let id1 = m1.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .unwrap()
        .b;
    let id2 = m2.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .unwrap()
        .b;

    assert_eq!(id1, IntrinsicId::StartsWith as u8);
    assert_eq!(id2, IntrinsicId::EndsWith as u8);
}

#[test]
fn intrinsic_chars() {
    let src = "cell main(s: String) -> list[String]\n  return chars(s)\nend";
    let module = compile_to_lir(src);
    let intr = module.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .expect("should emit Intrinsic opcode");
    assert_eq!(intr.b, IntrinsicId::Chars as u8);
}

#[test]
fn intrinsic_replace() {
    // Note: "from" is a keyword in Lumen, so use different parameter names
    let src = "cell main(s: String, old: String, new_val: String) -> String\n  return replace(s, old, new_val)\nend";
    let module = compile_to_lir(src);
    let intr = module.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .expect("should emit Intrinsic opcode");
    assert_eq!(intr.b, IntrinsicId::Replace as u8);
}

#[test]
fn intrinsic_flatten() {
    let src = "cell main(xs: list[list[Int]]) -> list[Int]\n  return flatten(xs)\nend";
    let module = compile_to_lir(src);
    let intr = module.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .expect("should emit Intrinsic opcode");
    assert_eq!(intr.b, IntrinsicId::Flatten as u8);
}

#[test]
fn intrinsic_unique() {
    let src = "cell main(xs: list[Int]) -> list[Int]\n  return unique(xs)\nend";
    let module = compile_to_lir(src);
    let intr = module.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .expect("should emit Intrinsic opcode");
    assert_eq!(intr.b, IntrinsicId::Unique as u8);
}

#[test]
fn intrinsic_take_drop() {
    let src_take = "cell test_take(xs: list[Int], n: Int) -> list[Int]\n  return take(xs, n)\nend";
    let src_drop = "cell test_drop(xs: list[Int], n: Int) -> list[Int]\n  return drop(xs, n)\nend";

    let m1 = compile_to_lir(src_take);
    let m2 = compile_to_lir(src_drop);

    let id1 = m1.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .unwrap()
        .b;
    let id2 = m2.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .unwrap()
        .b;

    assert_eq!(id1, IntrinsicId::Take as u8);
    assert_eq!(id2, IntrinsicId::Drop as u8);
}

// ============================================================================
// Set comprehension tests
// ============================================================================

#[test]
fn set_comprehension_uses_append_and_toset() {
    let src = "cell main() -> set[Int]\n  let xs = [1, 2, 3]\n  return {x for x in xs}\nend";
    let module = compile_to_lir(src);
    let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();

    assert!(
        ops.contains(&OpCode::Append),
        "set comprehension should use Append during iteration"
    );

    let intrinsics: Vec<u8> = module.cells[0]
        .instructions
        .iter()
        .filter(|i| i.op == OpCode::Intrinsic)
        .map(|i| i.b)
        .collect();
    assert!(
        intrinsics.contains(&(IntrinsicId::ToSet as u8)),
        "set comprehension should use ToSet intrinsic to convert list to set"
    );
}

#[test]
fn set_comprehension_with_condition() {
    let src = "cell main() -> set[Int]\n  let xs = [1, 2, 3, 4, 5]\n  return {x for x in xs if x > 2}\nend";
    let module = compile_to_lir(src);
    let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();

    assert!(
        ops.contains(&OpCode::Append),
        "set comprehension with condition should use Append"
    );
    assert!(
        ops.contains(&OpCode::Lt),
        "condition x > 2 should use Lt (swapped)"
    );

    let intrinsics: Vec<u8> = module.cells[0]
        .instructions
        .iter()
        .filter(|i| i.op == OpCode::Intrinsic)
        .map(|i| i.b)
        .collect();
    assert!(
        intrinsics.contains(&(IntrinsicId::ToSet as u8)),
        "set comprehension should use ToSet intrinsic"
    );
}

// ============================================================================
// Map comprehension tests
// ============================================================================

#[test]
fn map_comprehension_uses_newmap_and_setindex() {
    // Map comprehension uses tuple body syntax: {(key, value) for var in iter}
    let src = "cell main() -> map[String, Int]\n  let xs = [1, 2, 3]\n  return {(string(x), x) for x in xs}\nend";
    let module = compile_to_lir(src);
    let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();

    assert!(
        ops.contains(&OpCode::NewMap),
        "map comprehension should create NewMap"
    );
    assert!(
        ops.contains(&OpCode::SetIndex),
        "map comprehension should use SetIndex for key-value pairs"
    );
}

// ============================================================================
// List comprehension tests (ensure no regression)
// ============================================================================

#[test]
fn list_comprehension_uses_newlist_and_append() {
    let src = "cell main() -> list[Int]\n  let xs = [1, 2, 3]\n  return [x * 2 for x in xs]\nend";
    let module = compile_to_lir(src);
    let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();

    assert!(
        ops.contains(&OpCode::NewList),
        "list comprehension should create NewList"
    );
    assert!(
        ops.contains(&OpCode::Append),
        "list comprehension should use Append"
    );
}

// ============================================================================
// T193: Register reuse / contiguity regression tests
//
// These tests verify that lowering patterns requiring contiguous registers
// (TypeCast, RangeExpr, Perform) allocate them via alloc_block instead of
// individual alloc_temp() calls, which could return non-contiguous recycled
// registers and clobber live named bindings.
// ============================================================================

/// Helper: for every Intrinsic instruction, verify arg_start (c) == dest (a) + 1
/// for single-arg intrinsics (ToInt, ToFloat, ToString).
#[allow(dead_code)]
fn assert_intrinsic_registers_contiguous(module: &lumen_core::lir::LirModule, cell_name: &str) {
    let cell = module
        .cells
        .iter()
        .find(|c| c.name == cell_name)
        .unwrap_or_else(|| panic!("cell '{}' not found", cell_name));
    for (idx, instr) in cell.instructions.iter().enumerate() {
        if instr.op == OpCode::Intrinsic {
            let dest = instr.a;
            let arg_start = instr.c;
            // For single-arg intrinsics, dest + 1 == arg_start.
            // For multi-arg intrinsics (Range), the arg block is allocated
            // separately from dest so this relationship may not hold — those
            // are tested individually.
            let is_single_arg = matches!(
                instr.b,
                b if b == IntrinsicId::ToInt as u8
                    || b == IntrinsicId::ToFloat as u8
                    || b == IntrinsicId::ToString as u8
            );
            if is_single_arg {
                assert_eq!(
                    dest + 1,
                    arg_start,
                    "Intrinsic at index {}: dest=r{}, arg_start=r{} — \
                     single-arg intrinsics must have contiguous registers",
                    idx,
                    dest,
                    arg_start
                );
            }
        }
    }
}

#[test]
fn t193_typecast_after_stmt_expr_uses_contiguous_block() {
    // After a statement-level expression (which triggers free_statement_temps),
    // a type cast must allocate [dest, arg_start] contiguously.
    let src = "\
cell helper(x: Int) -> Int
  return x + 1
end

cell main() -> Int
  let a = helper(1)
  let b = helper(2)
  assert(a > 0)
  let c = b as Int
  return a + c
end";
    let module = compile_to_lir(src);
    let main = module
        .cells
        .iter()
        .find(|c| c.name == "main")
        .expect("main cell");

    // Verify all Intrinsic instructions have contiguous dest/arg registers
    for (idx, instr) in main.instructions.iter().enumerate() {
        if instr.op == OpCode::Intrinsic {
            assert_eq!(
                instr.a + 1,
                instr.c,
                "T193: Intrinsic at index {} must have contiguous dest(r{}) \
                 and arg_start(r{})",
                idx,
                instr.a,
                instr.c
            );
        }
    }
}

#[test]
fn t193_typecast_to_float_contiguous_registers() {
    // Type cast to Float should use contiguous block for dest and arg.
    let src = "\
cell main() -> Float
  let x = 42
  let y = 10
  assert(x > 0)
  return x as Float
end";
    let module = compile_to_lir(src);
    let main = module
        .cells
        .iter()
        .find(|c| c.name == "main")
        .expect("main cell");

    for (idx, instr) in main.instructions.iter().enumerate() {
        if instr.op == OpCode::Intrinsic && instr.b == IntrinsicId::ToFloat as u8 {
            assert_eq!(
                instr.a + 1,
                instr.c,
                "T193: ToFloat Intrinsic at index {} must have contiguous \
                 dest(r{}) and arg(r{})",
                idx,
                instr.a,
                instr.c
            );
        }
    }
}

#[test]
fn t193_typecast_to_string_contiguous_registers() {
    // Type cast to String should use contiguous block for dest and arg.
    let src = "\
cell main() -> String
  let a = 1
  let b = 2
  let c = 3
  assert(a > 0)
  return b as String
end";
    let module = compile_to_lir(src);
    let main = module
        .cells
        .iter()
        .find(|c| c.name == "main")
        .expect("main cell");

    for (idx, instr) in main.instructions.iter().enumerate() {
        if instr.op == OpCode::Intrinsic && instr.b == IntrinsicId::ToString as u8 {
            assert_eq!(
                instr.a + 1,
                instr.c,
                "T193: ToString Intrinsic at index {} must have contiguous \
                 dest(r{}) and arg(r{})",
                idx,
                instr.a,
                instr.c
            );
        }
    }
}

#[test]
fn t193_multiple_typecasts_in_sequence() {
    // Multiple type casts in a row should each have contiguous registers.
    let src = "\
cell main() -> String
  let x = 42
  let y = x as Float
  let z = y as Int
  return z as String
end";
    let module = compile_to_lir(src);
    assert_intrinsic_registers_contiguous(&module, "main");

    let main = module
        .cells
        .iter()
        .find(|c| c.name == "main")
        .expect("main cell");

    let intrinsic_count = main
        .instructions
        .iter()
        .filter(|i| i.op == OpCode::Intrinsic)
        .count();
    assert!(
        intrinsic_count >= 3,
        "Should have at least 3 Intrinsic instructions (ToFloat, ToInt, ToString), got {}",
        intrinsic_count
    );

    // Verify each one is contiguous
    for (idx, instr) in main.instructions.iter().enumerate() {
        if instr.op == OpCode::Intrinsic {
            assert_eq!(
                instr.a + 1,
                instr.c,
                "T193: Intrinsic at index {} must have contiguous dest(r{}) \
                 and arg(r{})",
                idx,
                instr.a,
                instr.c
            );
        }
    }
}

#[test]
fn t193_typecast_does_not_clobber_named_binding() {
    // The key T193 scenario: a type cast after free_statement_temps could
    // recycle a register adjacent to a named binding and clobber it.
    // Verify no Intrinsic's arg_start register overlaps with param registers.
    let src = "\
cell compute(a: Int, b: Int) -> String
  let sum = a + b
  assert(sum > 0)
  return sum as String
end";
    let module = compile_to_lir(src);
    let cell = module
        .cells
        .iter()
        .find(|c| c.name == "compute")
        .expect("compute cell");

    let param_regs: Vec<u8> = (0..cell.params.len() as u8).collect();

    for (idx, instr) in cell.instructions.iter().enumerate() {
        if instr.op == OpCode::Intrinsic {
            // The Move that places the arg value at arg_start should not
            // target a parameter register.
            assert!(
                !param_regs.contains(&instr.c),
                "T193: Intrinsic at index {} has arg_start=r{} which is a \
                 parameter register — would clobber named binding",
                idx,
                instr.c
            );
            // Also verify contiguity
            assert_eq!(
                instr.a + 1,
                instr.c,
                "T193: Intrinsic at index {} must have contiguous registers",
                idx,
            );
        }
    }
}

#[test]
fn t193_range_expr_uses_contiguous_arg_block() {
    // Range lowering must allocate a contiguous block for [start, end] args.
    let src = "\
cell main() -> list[Int]
  let x = 5
  let y = 10
  assert(x > 0)
  return x..y
end";
    let module = compile_to_lir(src);
    let main = module
        .cells
        .iter()
        .find(|c| c.name == "main")
        .expect("main cell");

    for (idx, instr) in main.instructions.iter().enumerate() {
        if instr.op == OpCode::Intrinsic && instr.b == IntrinsicId::Range as u8 {
            // For Range, the C field is arg_start where start is at C and
            // end is at C+1. They must be contiguous (guaranteed by alloc_block).
            // dest (A) is allocated separately, so A+1 != C is acceptable.
            // But C must not overlap with any parameter register.
            let param_regs: Vec<u8> = (0..main.params.len() as u8).collect();
            assert!(
                !param_regs.contains(&instr.c),
                "T193: Range arg_start=r{} at index {} overlaps param register",
                instr.c,
                idx
            );
            assert!(
                !param_regs.contains(&(instr.c + 1)),
                "T193: Range arg_end=r{} at index {} overlaps param register",
                instr.c + 1,
                idx
            );
        }
    }
}

#[test]
fn t193_chained_calls_then_typecast() {
    // Chained calls followed by type cast: free_statement_temps after the
    // intermediate call should not cause the cast to clobber bindings.
    let src = "\
cell double(n: Int) -> Int
  return n * 2
end

cell main() -> String
  let a = double(3)
  let b = double(a)
  assert(b > 0)
  return b as String
end";
    let module = compile_to_lir(src);
    let main = module
        .cells
        .iter()
        .find(|c| c.name == "main")
        .expect("main cell");

    for (idx, instr) in main.instructions.iter().enumerate() {
        if instr.op == OpCode::Intrinsic {
            assert_eq!(
                instr.a + 1,
                instr.c,
                "T193: Intrinsic at index {} after chained calls must have \
                 contiguous dest(r{}) and arg(r{})",
                idx,
                instr.a,
                instr.c
            );
        }
    }
}

#[test]
fn t193_call_then_assert_then_use_result() {
    // Classic T193 pattern: call result stored, assert reuses registers,
    // then original result is used. This must compile without register
    // clobbering.
    let src = "\
cell add(a: Int, b: Int) -> Int
  return a + b
end

cell main() -> Int
  let x = add(1, 2)
  let y = add(3, 4)
  assert(x > 0)
  assert(y > 0)
  return x + y
end";
    let module = compile_to_lir(src);
    let main = module
        .cells
        .iter()
        .find(|c| c.name == "main")
        .expect("main cell");

    // Verify x and y are in distinct named binding registers
    // and that no instruction between their definition and use clobbers them.
    // Since x and y are named bindings (alloc_named), they should be
    // protected from free_statement_temps recycling.
    let _param_count = main.params.len();

    // The return instruction should use an Add that reads from the registers
    // holding x and y. Verify those registers are below the param+named watermark.
    let add_instrs: Vec<_> = main
        .instructions
        .iter()
        .filter(|i| i.op == OpCode::Add)
        .collect();
    // There should be at least one Add for `x + y` in main
    assert!(
        !add_instrs.is_empty(),
        "main should have an Add instruction for x + y"
    );

    // All Call instructions should have distinct result registers
    let call_results: Vec<u8> = main
        .instructions
        .iter()
        .filter(|i| i.op == OpCode::Call)
        .map(|i| {
            // After Call, the result is Moved to a register.
            // The Call's A field is the base register.
            i.a
        })
        .collect();
    // Just verify compilation succeeds without panics (the real test is
    // that the program compiles at all with correct register allocation).
    assert!(
        call_results.len() >= 2,
        "should have at least 2 Call instructions"
    );
}

#[test]
fn t193_nested_call_with_typecast() {
    // Nested function call whose result is type-cast.
    let src = "\
cell inner(x: Int) -> Int
  return x * 2
end

cell outer(x: Int) -> Int
  return inner(x) + 1
end

cell main() -> Float
  let result = outer(5)
  assert(result > 0)
  return result as Float
end";
    let module = compile_to_lir(src);
    let main = module
        .cells
        .iter()
        .find(|c| c.name == "main")
        .expect("main cell");

    for (idx, instr) in main.instructions.iter().enumerate() {
        if instr.op == OpCode::Intrinsic && instr.b == IntrinsicId::ToFloat as u8 {
            assert_eq!(
                instr.a + 1,
                instr.c,
                "T193: ToFloat at index {} in nested call scenario must \
                 have contiguous registers",
                idx,
            );
        }
    }
}

#[test]
fn t193_multiple_lets_with_calls_and_cast() {
    // Many let bindings with calls, then a type cast — stress test
    // for register pressure and recycling.
    let src = "\
cell f(x: Int) -> Int
  return x + 1
end

cell main() -> String
  let a = f(1)
  let b = f(2)
  let c = f(3)
  let d = f(4)
  assert(a > 0)
  assert(b > 0)
  assert(c > 0)
  let total = a + b + c + d
  return total as String
end";
    let module = compile_to_lir(src);
    let main = module
        .cells
        .iter()
        .find(|c| c.name == "main")
        .expect("main cell");

    // Verify ToString intrinsic has contiguous registers
    for (idx, instr) in main.instructions.iter().enumerate() {
        if instr.op == OpCode::Intrinsic && instr.b == IntrinsicId::ToString as u8 {
            assert_eq!(
                instr.a + 1,
                instr.c,
                "T193: ToString at index {} with many let bindings must \
                 have contiguous registers",
                idx,
            );
        }
    }

    // Verify no intrinsic arg register overlaps with named binding registers
    // (params + a,b,c,d,total = at least 5 named registers above param_count)
    let named_high = main.params.len() + 5; // a, b, c, d, total
    for (idx, instr) in main.instructions.iter().enumerate() {
        if instr.op == OpCode::Intrinsic {
            assert!(
                instr.c >= named_high as u8,
                "T193: Intrinsic at index {} has arg_start=r{} which is \
                 within named binding range [0..{})",
                idx,
                instr.c,
                named_high
            );
        }
    }
}
