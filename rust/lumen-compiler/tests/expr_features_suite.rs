//! Tests for is/as expressions and implicit returns.

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

fn assert_compiles(src: &str) {
    let mut lexer = Lexer::new(src, 1, 0);
    let tokens = lexer.tokenize().expect("lex failed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program(vec![]).expect("parse failed");
    let symbols = resolve(&program).expect("resolve failed");
    typecheck(&program, &symbols).expect("typecheck failed");
}

// ============================================================================
// Feature 1: is type test expression
// ============================================================================

#[test]
fn is_type_compiles_and_returns_bool() {
    let src = "cell check(x: Int) -> Bool\n  return x is Int\nend";
    let module = compile_to_lir(src);
    let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
    assert!(
        ops.contains(&OpCode::Is),
        "should emit Is opcode for 'is' expression"
    );
}

#[test]
fn is_type_with_string() {
    assert_compiles("cell check(x: String) -> Bool\n  return x is String\nend");
}

#[test]
fn is_type_with_any_value() {
    assert_compiles("cell check() -> Bool\n  let x = 42\n  return x is Int\nend");
}

#[test]
fn is_type_loads_type_string_constant() {
    let src = "cell check(x: Int) -> Bool\n  return x is Int\nend";
    let module = compile_to_lir(src);
    // Should have "Int" in constants
    let has_int_str = module.cells[0]
        .constants
        .iter()
        .any(|c| matches!(c, lumen_compiler::compiler::lir::Constant::String(s) if s == "Int"));
    assert!(
        has_int_str,
        "should have 'Int' string constant for is type check"
    );
}

#[test]
fn is_type_in_if_condition() {
    assert_compiles(
        "cell check(x: Int) -> String\n  if x is Int\n    return \"yes\"\n  end\n  return \"no\"\nend",
    );
}

// ============================================================================
// Feature 2: as type cast expression
// ============================================================================

#[test]
fn as_int_compiles() {
    let src = "cell convert(x: Float) -> Int\n  return x as Int\nend";
    let module = compile_to_lir(src);
    let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
    assert!(
        ops.contains(&OpCode::Intrinsic),
        "should emit Intrinsic opcode for 'as Int' cast"
    );
    let intr = module.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .expect("should have Intrinsic");
    assert_eq!(
        intr.b,
        IntrinsicId::ToInt as u8,
        "should use ToInt intrinsic"
    );
}

#[test]
fn as_float_compiles() {
    let src = "cell convert(x: Int) -> Float\n  return x as Float\nend";
    let module = compile_to_lir(src);
    let intr = module.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .expect("should have Intrinsic for as Float");
    assert_eq!(
        intr.b,
        IntrinsicId::ToFloat as u8,
        "should use ToFloat intrinsic"
    );
}

#[test]
fn as_string_compiles() {
    let src = "cell convert(x: Int) -> String\n  return x as String\nend";
    let module = compile_to_lir(src);
    let intr = module.cells[0]
        .instructions
        .iter()
        .find(|i| i.op == OpCode::Intrinsic)
        .expect("should have Intrinsic for as String");
    assert_eq!(
        intr.b,
        IntrinsicId::ToString as u8,
        "should use ToString intrinsic"
    );
}

#[test]
fn as_bool_compiles() {
    // Bool cast uses double-not for truthiness check
    let src = "cell convert(x: Int) -> Bool\n  return x as Bool\nend";
    let module = compile_to_lir(src);
    let not_count = module.cells[0]
        .instructions
        .iter()
        .filter(|i| i.op == OpCode::Not)
        .count();
    assert!(
        not_count >= 2,
        "should emit double Not for Bool truthiness cast"
    );
}

#[test]
fn as_float_pi() {
    assert_compiles("cell main() -> Int\n  return 3.14 as Int\nend");
}

// ============================================================================
// Feature 3: Implicit returns
// ============================================================================

#[test]
fn implicit_return_simple() {
    let src = "cell add(a: Int, b: Int) -> Int\n  a + b\nend";
    let module = compile_to_lir(src);
    let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
    assert!(
        ops.contains(&OpCode::Return),
        "should emit Return for implicit return"
    );
    assert!(ops.contains(&OpCode::Add), "should emit Add for a + b");
}

#[test]
fn implicit_return_preserves_explicit() {
    // When there's an explicit return, it should still work
    let src = "cell add(a: Int, b: Int) -> Int\n  return a + b\nend";
    let module = compile_to_lir(src);
    let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
    assert!(ops.contains(&OpCode::Return));
}

#[test]
fn implicit_return_with_let_then_expr() {
    let src = "cell calc(x: Int) -> Int\n  let y = x * 2\n  y + 1\nend";
    let module = compile_to_lir(src);
    let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
    assert!(
        ops.contains(&OpCode::Return),
        "should emit Return for implicit return"
    );
}

#[test]
fn no_implicit_return_without_return_type() {
    // Cell without return type should not use implicit return
    let src = "cell greet()\n  let x = 42\nend";
    let module = compile_to_lir(src);
    // Should end with LoadNil + Return (the default)
    let instrs = &module.cells[0].instructions;
    let last_two: Vec<_> = instrs.iter().rev().take(2).map(|i| i.op).collect();
    assert_eq!(last_two, vec![OpCode::Return, OpCode::LoadNil]);
}

#[test]
fn implicit_return_string_literal() {
    let src = "cell hello() -> String\n  \"hello world\"\nend";
    assert_compiles(src);
    let module = compile_to_lir(src);
    let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
    assert!(ops.contains(&OpCode::Return));
}

// ============================================================================
// Combined features
// ============================================================================

#[test]
fn is_and_as_combined() {
    assert_compiles(
        "cell process(x: Int) -> String\n  if x is Int\n    return x as String\n  end\n  return \"unknown\"\nend",
    );
}
