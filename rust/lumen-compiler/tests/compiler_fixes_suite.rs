//! Tests for compiler fixes: NotEq operator, intrinsic name mappings,
//! and set/map comprehension lowering.

use lumen_compiler::compiler::lexer::Lexer;
use lumen_compiler::compiler::lir::{IntrinsicId, OpCode};
use lumen_compiler::compiler::lower::lower;
use lumen_compiler::compiler::parser::Parser;
use lumen_compiler::compiler::resolve::resolve;
use lumen_compiler::compiler::typecheck::typecheck;

fn compile_to_lir(src: &str) -> lumen_compiler::compiler::lir::LirModule {
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

    let eq_idx = ops.iter().position(|o| *o == OpCode::Eq).expect("should emit Eq");
    assert_eq!(ops[eq_idx + 1], OpCode::Not, "Not should follow Eq for != operator");
}

#[test]
fn noteq_with_variables() {
    let src = "cell neq(a: Int, b: Int) -> Bool\n  return a != b\nend";
    let module = compile_to_lir(src);
    let instrs = &module.cells[0].instructions;

    let eq_instr = instrs.iter().find(|i| i.op == OpCode::Eq).expect("should emit Eq");
    let not_instr = instrs.iter().find(|i| i.op == OpCode::Not).expect("should emit Not");

    // Not should write to the same register as Eq
    assert_eq!(eq_instr.a, not_instr.a, "Eq and Not should target the same register");
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
    let src = "cell main(xs: list[Int], f: fn(Int) -> Bool) -> list[Int]\n  return filter(xs, f)\nend";
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
    let src = "cell main(xs: list[Int], f: fn(Int) -> String) -> list[String]\n  return map(xs, f)\nend";
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

    let id1 = m1.cells[0].instructions.iter().find(|i| i.op == OpCode::Intrinsic).unwrap().b;
    let id2 = m2.cells[0].instructions.iter().find(|i| i.op == OpCode::Intrinsic).unwrap().b;
    let id3 = m3.cells[0].instructions.iter().find(|i| i.op == OpCode::Intrinsic).unwrap().b;

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

    let id1 = m1.cells[0].instructions.iter().find(|i| i.op == OpCode::Intrinsic).unwrap().b;
    let id2 = m2.cells[0].instructions.iter().find(|i| i.op == OpCode::Intrinsic).unwrap().b;

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

    let id1 = m1.cells[0].instructions.iter().find(|i| i.op == OpCode::Intrinsic).unwrap().b;
    let id2 = m2.cells[0].instructions.iter().find(|i| i.op == OpCode::Intrinsic).unwrap().b;

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

    assert!(ops.contains(&OpCode::Append), "set comprehension should use Append during iteration");

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

    assert!(ops.contains(&OpCode::Append), "set comprehension with condition should use Append");
    assert!(ops.contains(&OpCode::Lt), "condition x > 2 should use Lt (swapped)");

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

    assert!(ops.contains(&OpCode::NewMap), "map comprehension should create NewMap");
    assert!(ops.contains(&OpCode::SetIndex), "map comprehension should use SetIndex for key-value pairs");
}

// ============================================================================
// List comprehension tests (ensure no regression)
// ============================================================================

#[test]
fn list_comprehension_uses_newlist_and_append() {
    let src = "cell main() -> list[Int]\n  let xs = [1, 2, 3]\n  return [x * 2 for x in xs]\nend";
    let module = compile_to_lir(src);
    let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();

    assert!(ops.contains(&OpCode::NewList), "list comprehension should create NewList");
    assert!(ops.contains(&OpCode::Append), "list comprehension should use Append");
}
