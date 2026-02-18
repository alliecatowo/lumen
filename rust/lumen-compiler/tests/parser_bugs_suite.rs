//! Tests for parser bug fixes:
//! 1. For-loop filter conditions were silently discarded
//! 2. Labeled break/continue labels were silently discarded
//! 3. Variadic parameter flags were silently discarded

use lumen_compiler::compiler::ast::{Expr, Stmt};
use lumen_compiler::compiler::lexer::Lexer;
use lumen_compiler::compiler::lower::lower;
use lumen_compiler::compiler::parser::Parser;
use lumen_compiler::compiler::resolve::resolve;
use lumen_compiler::compiler::typecheck::typecheck;
use lumen_core::lir::OpCode;

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

// ============================================================================
// Bug 1: For-loop filter conditions
// ============================================================================

#[test]
fn for_filter_parsed_into_ast() {
    let src = "cell main() -> Int\n  let total = 0\n  for x in [1, 2, 3, 4, 5] if x > 2\n    total += x\n  end\n  return total\nend";
    let program = parse(src);
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };
    // Find the for statement
    let for_stmt = cell
        .body
        .iter()
        .find_map(|s| match s {
            Stmt::For(fs) => Some(fs),
            _ => None,
        })
        .expect("should have a for statement");

    assert!(
        for_stmt.filter.is_some(),
        "for-loop filter should be stored in the AST"
    );

    // Check the filter is a BinOp (x > 2)
    match for_stmt.filter.as_ref().unwrap() {
        Expr::BinOp(_, op, _, _) => {
            assert_eq!(
                *op,
                lumen_compiler::compiler::ast::BinOp::Gt,
                "filter should be a > comparison"
            );
        }
        other => panic!("expected BinOp for filter, got {:?}", other),
    }
}

#[test]
fn for_filter_compiles_to_lir() {
    let src = "cell main() -> Int\n  let total = 0\n  for x in [1, 2, 3, 4, 5] if x > 2\n    total += x\n  end\n  return total\nend";
    let module = compile_to_lir(src);
    let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();

    // The filter should generate a comparison (Gt or Lt) and a Test+Jmp to skip the body
    // Without the filter fix, the filter would be discarded and no comparison would appear
    // for the filter condition
    // Gt is lowered as Lt with swapped operands, so look for Lt
    let gt_count = ops
        .iter()
        .filter(|o| **o == OpCode::Lt || **o == OpCode::Le)
        .count();
    assert!(
        gt_count >= 1,
        "for-loop filter should emit a comparison instruction, got opcodes: {:?}",
        ops
    );

    // Should have Test instructions (one for loop bounds, one for filter)
    let test_count = ops.iter().filter(|o| **o == OpCode::Test).count();
    assert!(
        test_count >= 2,
        "for-loop with filter should emit at least 2 Test instructions (bounds + filter), got {}",
        test_count
    );
}

#[test]
fn for_without_filter_still_works() {
    let src = "cell main() -> Int\n  let total = 0\n  for x in [1, 2, 3]\n    total += x\n  end\n  return total\nend";
    let program = parse(src);
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };
    let for_stmt = cell
        .body
        .iter()
        .find_map(|s| match s {
            Stmt::For(fs) => Some(fs),
            _ => None,
        })
        .expect("should have a for statement");

    assert!(
        for_stmt.filter.is_none(),
        "for-loop without filter should have None filter"
    );
}

#[test]
fn for_filter_with_pattern_destructure() {
    let src = "cell main() -> Int\n  let total = 0\n  let items = [(1, 2), (3, 4), (5, 6)]\n  for (a, b) in items if a > 2\n    total += b\n  end\n  return total\nend";
    let program = parse(src);
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };
    let for_stmt = cell
        .body
        .iter()
        .find_map(|s| match s {
            Stmt::For(fs) => Some(fs),
            _ => None,
        })
        .expect("should have a for statement");

    assert!(
        for_stmt.filter.is_some(),
        "for-loop with pattern and filter should store the filter"
    );
    assert!(
        for_stmt.pattern.is_some(),
        "for-loop should also preserve the pattern"
    );
}

// ============================================================================
// Bug 2: Labeled break/continue
// ============================================================================

#[test]
fn labeled_break_parsed_into_ast() {
    let src =
        "cell main() -> Int\n  loop\n    loop\n      break @outer\n    end\n  end\n  return 0\nend";
    let program = parse(src);
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };
    // Navigate to the inner loop's break statement
    let outer_loop = match &cell.body[0] {
        Stmt::Loop(ls) => ls,
        _ => panic!("expected outer loop"),
    };
    let inner_loop = match &outer_loop.body[0] {
        Stmt::Loop(ls) => ls,
        _ => panic!("expected inner loop"),
    };
    let break_stmt = match &inner_loop.body[0] {
        Stmt::Break(bs) => bs,
        _ => panic!("expected break statement"),
    };

    assert_eq!(
        break_stmt.label.as_deref(),
        Some("outer"),
        "break should have label 'outer'"
    );
}

#[test]
fn labeled_continue_parsed_into_ast() {
    let src = "cell main() -> Int\n  loop\n    loop\n      continue @outer\n    end\n  end\n  return 0\nend";
    let program = parse(src);
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };
    let outer_loop = match &cell.body[0] {
        Stmt::Loop(ls) => ls,
        _ => panic!("expected outer loop"),
    };
    let inner_loop = match &outer_loop.body[0] {
        Stmt::Loop(ls) => ls,
        _ => panic!("expected inner loop"),
    };
    let continue_stmt = match &inner_loop.body[0] {
        Stmt::Continue(cs) => cs,
        _ => panic!("expected continue statement"),
    };

    assert_eq!(
        continue_stmt.label.as_deref(),
        Some("outer"),
        "continue should have label 'outer'"
    );
}

#[test]
fn unlabeled_break_has_none_label() {
    let src = "cell main() -> Int\n  loop\n    break\n  end\n  return 0\nend";
    let program = parse(src);
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };
    let loop_stmt = match &cell.body[0] {
        Stmt::Loop(ls) => ls,
        _ => panic!("expected loop"),
    };
    let break_stmt = match &loop_stmt.body[0] {
        Stmt::Break(bs) => bs,
        _ => panic!("expected break"),
    };

    assert!(
        break_stmt.label.is_none(),
        "unlabeled break should have None label"
    );
}

#[test]
fn unlabeled_continue_has_none_label() {
    let src = "cell main() -> Int\n  let i = 0\n  loop\n    i += 1\n    if i > 5\n      break\n    end\n    continue\n  end\n  return i\nend";
    let program = parse(src);
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };
    // Find any continue in the loop body
    fn find_continue(stmts: &[Stmt]) -> bool {
        for s in stmts {
            match s {
                Stmt::Continue(cs) => {
                    assert!(
                        cs.label.is_none(),
                        "unlabeled continue should have None label"
                    );
                    return true;
                }
                Stmt::Loop(ls) => {
                    if find_continue(&ls.body) {
                        return true;
                    }
                }
                Stmt::If(ifs) => {
                    if find_continue(&ifs.then_body) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }
    assert!(
        find_continue(&cell.body),
        "should find a continue statement"
    );
}

// ============================================================================
// Bug 3: Variadic parameters
// ============================================================================

#[test]
fn variadic_param_flag_stored_in_ast() {
    let src = "cell sum(...args: Int) -> Int\n  return 0\nend";
    let program = parse(src);
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };

    assert_eq!(cell.params.len(), 1, "should have one param");
    assert!(cell.params[0].variadic, "param should be marked variadic");
    assert_eq!(cell.params[0].name, "args", "param name should be 'args'");
}

#[test]
fn non_variadic_param_flag_is_false() {
    let src = "cell add(a: Int, b: Int) -> Int\n  return a + b\nend";
    let program = parse(src);
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };

    assert_eq!(cell.params.len(), 2, "should have two params");
    assert!(
        !cell.params[0].variadic,
        "first param should not be variadic"
    );
    assert!(
        !cell.params[1].variadic,
        "second param should not be variadic"
    );
}

#[test]
fn mixed_regular_and_variadic_params() {
    let src = "cell format(template: String, ...values) -> String\n  return template\nend";
    let program = parse(src);
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };

    assert_eq!(cell.params.len(), 2, "should have two params");
    assert!(!cell.params[0].variadic, "template should not be variadic");
    assert!(cell.params[1].variadic, "values should be variadic");
    assert_eq!(cell.params[1].name, "values");
}

#[test]
fn variadic_param_with_dotdot() {
    // Test that .. (2 dots) also works as variadic marker
    let src = "cell sum(..args: Int) -> Int\n  return 0\nend";
    let program = parse(src);
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };

    assert!(
        cell.params[0].variadic,
        "param with .. should also be marked variadic"
    );
}
