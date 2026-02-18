//! Tests for T197: i64::MIN literal parsing and lowering.
//!
//! The literal -9223372036854775808 (i64::MIN) previously caused a "cannot negate"
//! runtime error because the parser treated `-` as unary negation of the positive
//! literal 9223372036854775808, which overflows i64 and becomes a BigInt. The VM's
//! Neg opcode didn't handle BigInt values.
//!
//! The fix constant-folds `-<integer>` in the parser, with a special case for the
//! BigInt value 9223372036854775808 (i64::MAX + 1) that folds to IntLit(i64::MIN).

use lumen_compiler::compiler::ast::{Expr, Item, Stmt, UnaryOp};
use lumen_compiler::compiler::lexer::Lexer;
use lumen_compiler::compiler::lower::lower;
use lumen_compiler::compiler::parser::Parser;
use lumen_compiler::compiler::resolve::resolve;
use lumen_compiler::compiler::typecheck::typecheck;

fn parse(src: &str) -> lumen_compiler::compiler::ast::Program {
    let mut lexer = Lexer::new(src, 1, 0);
    let tokens = lexer.tokenize().expect("lex failed");
    let mut parser = Parser::new(tokens);
    parser.parse_program(vec![]).expect("parse failed")
}

fn compile_to_lir(src: &str) -> lumen_core::lir::LirModule {
    let mut lexer = Lexer::new(src, 1, 0);
    let tokens = lexer.tokenize().expect("lex failed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program(vec![]).expect("parse failed");
    let symbols = resolve(&program).expect("resolve failed");
    typecheck(&program, &symbols).expect("typecheck failed");
    lower(&program, &symbols, src)
}

/// Extract the first expression from a `let` binding in the first cell's body.
fn first_let_expr(program: &lumen_compiler::compiler::ast::Program) -> &Expr {
    let cell = match &program.items[0] {
        Item::Cell(c) => c,
        other => panic!("expected Cell, got {:?}", other),
    };
    for stmt in &cell.body {
        if let Stmt::Let(let_stmt) = stmt {
            return &let_stmt.value;
        }
    }
    panic!("no let statement found in cell body");
}

// ============================================================================
// Test 1: i64::MIN literal (-9223372036854775808) parses as IntLit(i64::MIN)
// ============================================================================

#[test]
fn i64_min_parses_as_int_literal() {
    let src = "cell main() -> Int\n  let x = -9223372036854775808\n  x\nend";
    let program = parse(src);
    let expr = first_let_expr(&program);
    match expr {
        Expr::IntLit(n, _) => {
            assert_eq!(*n, i64::MIN, "should fold to i64::MIN");
        }
        other => panic!(
            "expected IntLit(i64::MIN), got {:?}",
            std::mem::discriminant(other)
        ),
    }
}

// ============================================================================
// Test 2: -1 still works normally and folds to IntLit(-1)
// ============================================================================

#[test]
fn neg_one_parses_as_int_literal() {
    let src = "cell main() -> Int\n  let x = -1\n  x\nend";
    let program = parse(src);
    let expr = first_let_expr(&program);
    match expr {
        Expr::IntLit(n, _) => {
            assert_eq!(*n, -1, "should fold -1 to IntLit(-1)");
        }
        other => panic!(
            "expected IntLit(-1), got {:?}",
            std::mem::discriminant(other)
        ),
    }
}

// ============================================================================
// Test 3: i64::MAX (9223372036854775807) works as a positive literal
// ============================================================================

#[test]
fn i64_max_parses_as_int_literal() {
    let src = "cell main() -> Int\n  let x = 9223372036854775807\n  x\nend";
    let program = parse(src);
    let expr = first_let_expr(&program);
    match expr {
        Expr::IntLit(n, _) => {
            assert_eq!(*n, i64::MAX, "should be i64::MAX");
        }
        other => panic!(
            "expected IntLit(i64::MAX), got {:?}",
            std::mem::discriminant(other)
        ),
    }
}

// ============================================================================
// Test 4: -9223372036854775807 (-(i64::MAX)) works as normal negation
// ============================================================================

#[test]
fn neg_i64_max_parses_as_int_literal() {
    let src = "cell main() -> Int\n  let x = -9223372036854775807\n  x\nend";
    let program = parse(src);
    let expr = first_let_expr(&program);
    match expr {
        Expr::IntLit(n, _) => {
            assert_eq!(*n, -9223372036854775807i64, "should fold to -(i64::MAX)");
        }
        other => panic!(
            "expected IntLit(-9223372036854775807), got {:?}",
            std::mem::discriminant(other)
        ),
    }
}

// ============================================================================
// Test 5: Large positive beyond i64::MAX becomes BigIntLit
// ============================================================================

#[test]
fn large_positive_beyond_i64_max_is_bigint() {
    let src = "cell main() -> Int\n  let x = 9223372036854775808\n  x\nend";
    let program = parse(src);
    let expr = first_let_expr(&program);
    match expr {
        Expr::BigIntLit(_, _) => { /* expected */ }
        other => panic!(
            "expected BigIntLit for value > i64::MAX, got {:?}",
            std::mem::discriminant(other)
        ),
    }
}

// ============================================================================
// Test 6: i64::MIN compiles and lowers to LIR without errors
// ============================================================================

#[test]
fn i64_min_compiles_to_lir() {
    let src = "cell main() -> Int\n  let x = -9223372036854775808\n  x\nend";
    let module = compile_to_lir(src);
    // Should have at least one cell
    assert!(
        !module.cells.is_empty(),
        "module should have at least one cell"
    );
    // The constant pool should contain i64::MIN
    let has_min = module.cells[0]
        .constants
        .iter()
        .any(|c| matches!(c, lumen_core::lir::Constant::Int(n) if *n == i64::MIN));
    assert!(
        has_min,
        "constant pool should contain i64::MIN (-9223372036854775808)"
    );
}

// ============================================================================
// Test 7: Negative zero still works
// ============================================================================

#[test]
fn neg_zero_parses_as_int_literal() {
    let src = "cell main() -> Int\n  let x = -0\n  x\nend";
    let program = parse(src);
    let expr = first_let_expr(&program);
    match expr {
        Expr::IntLit(n, _) => {
            assert_eq!(*n, 0, "-0 should fold to 0 for integers");
        }
        other => panic!(
            "expected IntLit(0), got {:?}",
            std::mem::discriminant(other)
        ),
    }
}

// ============================================================================
// Test 8: Negation of non-literal expressions is still UnaryOp
// ============================================================================

#[test]
fn neg_variable_remains_unary_op() {
    let src = "cell main(a: Int) -> Int\n  let x = -a\n  x\nend";
    let program = parse(src);
    let expr = first_let_expr(&program);
    match expr {
        Expr::UnaryOp(UnaryOp::Neg, inner, _) => match inner.as_ref() {
            Expr::Ident(name, _) => assert_eq!(name, "a"),
            other => panic!("expected Ident(a), got {:?}", std::mem::discriminant(other)),
        },
        other => panic!(
            "expected UnaryOp(Neg, Ident(a)), got {:?}",
            std::mem::discriminant(other)
        ),
    }
}

// ============================================================================
// Test 9: Very large negative beyond i64::MIN stays as UnaryOp(Neg, BigIntLit)
// ============================================================================

#[test]
fn very_large_negative_stays_unary_bigint() {
    // -99999999999999999999 is well beyond i64 range
    let src = "cell main() -> Int\n  let x = -99999999999999999999\n  x\nend";
    let program = parse(src);
    let expr = first_let_expr(&program);
    match expr {
        Expr::UnaryOp(UnaryOp::Neg, inner, _) => {
            match inner.as_ref() {
                Expr::BigIntLit(_, _) => { /* expected */ }
                other => panic!(
                    "expected BigIntLit operand, got {:?}",
                    std::mem::discriminant(other)
                ),
            }
        }
        other => panic!(
            "expected UnaryOp(Neg, BigIntLit), got {:?}",
            std::mem::discriminant(other)
        ),
    }
}
