//! Tests for labeled loops, floor division, and null-safe index access.

use lumen_compiler::compiler::ast::{BinOp, CompoundOp, Expr, Stmt};
use lumen_compiler::compiler::lexer::Lexer;
use lumen_compiler::compiler::lower::lower;
use lumen_compiler::compiler::parser::Parser;
use lumen_compiler::compiler::resolve::resolve;
use lumen_compiler::compiler::tokens::TokenKind;
use lumen_compiler::compiler::typecheck::typecheck;
use lumen_core::lir::OpCode;

fn compile_to_lir(src: &str) -> lumen_core::lir::LirModule {
    let mut lexer = Lexer::new(src, 1, 0);
    let tokens = lexer.tokenize().expect("lex failed");
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program(vec![]).expect("parse failed");
    let symbols = resolve(&program).expect("resolve failed");
    typecheck(&program, &symbols).expect("typecheck failed");
    lower(&program, &symbols, src)
}

fn parse_program(src: &str) -> lumen_compiler::compiler::ast::Program {
    let mut lexer = Lexer::new(src, 1, 0);
    let tokens = lexer.tokenize().expect("lex failed");
    let mut parser = Parser::new(tokens);
    parser.parse_program(vec![]).expect("parse failed")
}

fn lex(src: &str) -> Vec<TokenKind> {
    let mut lexer = Lexer::new(src, 1, 0);
    lexer
        .tokenize()
        .expect("lex failed")
        .into_iter()
        .map(|t| t.kind)
        .collect()
}

// ============================================================================
// Feature 1: Floor Division //
// ============================================================================

#[test]
fn lex_floor_div() {
    let tokens = lex("a // b");
    assert!(tokens.contains(&TokenKind::FloorDiv));
}

#[test]
fn lex_floor_div_assign() {
    let tokens = lex("a //= b");
    assert!(tokens.contains(&TokenKind::FloorDivAssign));
}

#[test]
fn lex_floor_div_does_not_conflict_with_slash() {
    let tokens = lex("a / b // c");
    assert!(tokens.contains(&TokenKind::Slash));
    assert!(tokens.contains(&TokenKind::FloorDiv));
}

#[test]
fn lex_slash_assign_still_works() {
    let tokens = lex("a /= b");
    assert!(tokens.contains(&TokenKind::SlashAssign));
}

#[test]
fn parse_floor_div_binop() {
    let program = parse_program("cell main() -> Int\n  return 7 // 2\nend");
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };
    if let Stmt::Return(ret) = &cell.body[0] {
        match &ret.value {
            Expr::BinOp(_, op, _, _) => assert_eq!(*op, BinOp::FloorDiv),
            _ => panic!("expected BinOp"),
        }
    } else {
        panic!("expected return");
    }
}

#[test]
fn parse_floor_div_assign() {
    let program = parse_program("cell main() -> Int\n  let mut x = 10\n  x //= 3\n  return x\nend");
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };
    if let Stmt::CompoundAssign(ca) = &cell.body[1] {
        assert!(matches!(ca.op, CompoundOp::FloorDivAssign));
    } else {
        panic!("expected CompoundAssign");
    }
}

#[test]
fn floor_div_emits_floordiv_opcode() {
    let src = "cell main() -> Int\n  return 7 // 2\nend";
    let module = compile_to_lir(src);
    let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
    assert!(
        ops.contains(&OpCode::FloorDiv),
        "should emit FloorDiv opcode, got {:?}",
        ops
    );
}

#[test]
fn floor_div_assign_emits_floordiv_opcode() {
    let src = "cell main() -> Int\n  let mut x = 10\n  x //= 3\n  return x\nend";
    let module = compile_to_lir(src);
    let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
    assert!(
        ops.contains(&OpCode::FloorDiv),
        "should emit FloorDiv opcode for //=, got {:?}",
        ops
    );
}

#[test]
fn floor_div_same_precedence_as_div() {
    // 2 + 10 // 3 should parse as 2 + (10 // 3)
    let program = parse_program("cell main() -> Int\n  return 2 + 10 // 3\nend");
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };
    if let Stmt::Return(ret) = &cell.body[0] {
        match &ret.value {
            Expr::BinOp(_, BinOp::Add, rhs, _) => {
                match rhs.as_ref() {
                    Expr::BinOp(_, BinOp::FloorDiv, _, _) => {} // correct
                    other => panic!("expected FloorDiv on right of Add, got {:?}", other),
                }
            }
            _ => panic!("expected Add BinOp"),
        }
    } else {
        panic!("expected return");
    }
}

// ============================================================================
// Feature 2: Optional Index ?[]
// ============================================================================

#[test]
fn lex_question_bracket() {
    let tokens = lex("a?[0]");
    assert!(tokens.contains(&TokenKind::QuestionBracket));
}

#[test]
fn parse_null_safe_index() {
    let program =
        parse_program("cell main() -> Int\n  let x = [1, 2, 3]\n  let y = x?[0]\n  return 0\nend");
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };
    if let Stmt::Let(ls) = &cell.body[1] {
        match &ls.value {
            Expr::NullSafeIndex(_, _, _) => {} // correct
            other => panic!("expected NullSafeIndex, got {:?}", other),
        }
    } else {
        panic!("expected let");
    }
}

#[test]
fn null_safe_index_emits_null_check() {
    let src = "cell main() -> Int\n  let x = [1, 2, 3]\n  let y = x?[0]\n  return 0\nend";
    let module = compile_to_lir(src);
    let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
    // Should contain: LoadNil (null check), Eq, Test, Jmp, GetIndex
    assert!(
        ops.contains(&OpCode::LoadNil),
        "should emit LoadNil for null check"
    );
    assert!(
        ops.contains(&OpCode::GetIndex),
        "should emit GetIndex for the actual indexing"
    );
}

#[test]
fn question_bracket_does_not_conflict_with_question() {
    // expr? should still work as TryExpr
    let tokens = lex("a? + b");
    assert!(tokens.contains(&TokenKind::Question));
}

// ============================================================================
// Feature 3: Labeled Loops
// ============================================================================

#[test]
fn parse_labeled_loop() {
    let program = parse_program(
        "cell main() -> Int\n  loop @outer\n    break @outer\n  end\n  return 0\nend",
    );
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };
    if let Stmt::Loop(ls) = &cell.body[0] {
        assert_eq!(ls.label.as_deref(), Some("outer"));
    } else {
        panic!("expected loop statement");
    }
}

#[test]
fn parse_labeled_while() {
    let program = parse_program(
        "cell main() -> Int\n  let mut x = 0\n  while @outer x < 10\n    x = x + 1\n    continue @outer\n  end\n  return x\nend",
    );
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };
    if let Stmt::While(ws) = &cell.body[1] {
        assert_eq!(ws.label.as_deref(), Some("outer"));
    } else {
        panic!("expected while statement, got {:?}", cell.body[1]);
    }
}

#[test]
fn parse_labeled_for() {
    let program = parse_program(
        "cell main() -> Int\n  for @items x in [1, 2, 3]\n    break @items\n  end\n  return 0\nend",
    );
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };
    if let Stmt::For(fs) = &cell.body[0] {
        assert_eq!(fs.label.as_deref(), Some("items"));
    } else {
        panic!("expected for statement");
    }
}

#[test]
fn parse_break_with_label() {
    let program = parse_program(
        "cell main() -> Int\n  loop @outer\n    break @outer\n  end\n  return 0\nend",
    );
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };
    if let Stmt::Loop(ls) = &cell.body[0] {
        if let Stmt::Break(bs) = &ls.body[0] {
            assert_eq!(bs.label.as_deref(), Some("outer"));
        } else {
            panic!("expected break statement");
        }
    } else {
        panic!("expected loop statement");
    }
}

#[test]
fn parse_continue_with_label() {
    let program = parse_program(
        "cell main() -> Int\n  let mut x = 0\n  while @outer x < 10\n    x = x + 1\n    continue @outer\n  end\n  return x\nend",
    );
    let cell = match &program.items[0] {
        lumen_compiler::compiler::ast::Item::Cell(c) => c,
        _ => panic!("expected cell"),
    };
    if let Stmt::While(ws) = &cell.body[1] {
        if let Stmt::Continue(cs) = &ws.body[1] {
            assert_eq!(cs.label.as_deref(), Some("outer"));
        } else {
            panic!("expected continue, got {:?}", ws.body[1]);
        }
    } else {
        panic!("expected while statement");
    }
}

#[test]
fn labeled_loop_compiles_to_lir() {
    let src = "cell main() -> Int\n  loop @outer\n    break @outer\n  end\n  return 0\nend";
    let module = compile_to_lir(src);
    let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
    // Should have jump instructions
    assert!(
        ops.contains(&OpCode::Jmp),
        "labeled loop should emit Jmp instructions"
    );
}

#[test]
fn nested_labeled_loops_break_outer() {
    let src = r#"cell main() -> Int
  let mut result = 0
  loop @outer
    loop @inner
      result = 1
      break @outer
    end
    result = 2
  end
  return result
end"#;
    let module = compile_to_lir(src);
    // Should compile successfully - the break @outer should target the outer loop
    assert!(!module.cells.is_empty());
    let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
    // Count Jmp instructions - expect multiple for nested loops with labeled break
    let jmp_count = ops.iter().filter(|&&op| op == OpCode::Jmp).count();
    assert!(
        jmp_count >= 3,
        "nested labeled loops should emit at least 3 Jmp instructions, got {}",
        jmp_count
    );
}

#[test]
fn unlabeled_loop_still_works() {
    let src = "cell main() -> Int\n  let mut x = 0\n  loop\n    x = x + 1\n    if x == 5\n      break\n    end\n  end\n  return x\nend";
    let module = compile_to_lir(src);
    assert!(!module.cells.is_empty());
}

#[test]
fn unlabeled_while_still_works() {
    let src = "cell main() -> Int\n  let mut x = 0\n  while x < 10\n    x = x + 1\n  end\n  return x\nend";
    let module = compile_to_lir(src);
    assert!(!module.cells.is_empty());
}

#[test]
fn unlabeled_for_still_works() {
    let src = "cell main() -> Int\n  let mut sum = 0\n  for x in [1, 2, 3]\n    sum = sum + x\n  end\n  return sum\nend";
    let module = compile_to_lir(src);
    assert!(!module.cells.is_empty());
}

// ============================================================================
// VM integration: Floor Division
// ============================================================================

#[test]
fn vm_floor_div_int() {
    use lumen_vm::vm::VM;
    let module = compile_to_lir("cell main() -> Int\n  return 7 // 2\nend");
    let mut vm = VM::new();
    vm.load(module);
    let result = vm.execute("main", vec![]).expect("vm run failed");
    assert_eq!(result.to_string(), "3");
}

#[test]
fn vm_floor_div_negative() {
    use lumen_vm::vm::VM;
    // Test negative floor division: (-7) // 2 = -4 (rounds toward negative infinity)
    let module = compile_to_lir("cell fdiv(a: Int, b: Int) -> Int\n  return a // b\nend\ncell main() -> Int\n  return fdiv(0 - 7, 2)\nend");
    let mut vm = VM::new();
    vm.load(module);
    let result = vm.execute("main", vec![]).expect("vm run failed");
    assert_eq!(result.to_string(), "-4");
}

#[test]
fn vm_floor_div_float() {
    use lumen_vm::vm::VM;
    let module = compile_to_lir("cell main() -> Float\n  return 7.0 // 2.0\nend");
    let mut vm = VM::new();
    vm.load(module);
    let result = vm.execute("main", vec![]).expect("vm run failed");
    assert_eq!(result.to_string(), "3.0");
}

// ============================================================================
// VM integration: Null-safe Index
// ============================================================================

#[test]
fn vm_null_safe_index_valid() {
    use lumen_vm::vm::VM;
    let module = compile_to_lir(
        "cell main() -> Int\n  let x = [10, 20, 30]\n  let y = x?[1]\n  return y ?? 0\nend",
    );
    let mut vm = VM::new();
    vm.load(module);
    let result = vm.execute("main", vec![]).expect("vm run failed");
    assert_eq!(result.to_string(), "20");
}

#[test]
fn vm_null_safe_index_on_null() {
    use lumen_vm::vm::VM;
    let module = compile_to_lir(
        "cell main() -> Int\n  let x = null\n  let y = x?[0]\n  return y ?? 42\nend",
    );
    let mut vm = VM::new();
    vm.load(module);
    let result = vm.execute("main", vec![]).expect("vm run failed");
    assert_eq!(result.to_string(), "42");
}

// ============================================================================
// VM integration: Labeled Loops
// ============================================================================

#[test]
fn vm_labeled_break_outer() {
    use lumen_vm::vm::VM;
    let src = r#"cell main() -> Int
  let mut result = 0
  loop @outer
    let mut i = 0
    loop @inner
      i = i + 1
      if i == 3
        result = i
        break @outer
      end
    end
    result = 99
  end
  return result
end"#;
    let module = compile_to_lir(src);
    let mut vm = VM::new();
    vm.load(module);
    let result = vm.execute("main", vec![]).expect("vm run failed");
    assert_eq!(result.to_string(), "3");
}

#[test]
fn vm_labeled_continue() {
    use lumen_vm::vm::VM;
    let src = r#"cell main() -> Int
  let mut count = 0
  let mut i = 0
  while @outer i < 5
    i = i + 1
    if i == 3
      continue @outer
    end
    count = count + 1
  end
  return count
end"#;
    let module = compile_to_lir(src);
    let mut vm = VM::new();
    vm.load(module);
    let result = vm.execute("main", vec![]).expect("vm run failed");
    // Skips i==3, so count increments for i=1,2,4,5 = 4
    assert_eq!(result.to_string(), "4");
}

#[test]
fn vm_compose_operator() {
    use lumen_vm::vm::VM;
    let src = r#"cell double(x: Int) -> Int
  return x * 2
end

cell add_one(x: Int) -> Int
  return x + 1
end

cell main() -> Int
  let f = double ~> add_one
  return f(5)
end"#;
    let module = compile_to_lir(src);
    let mut vm = VM::new();
    vm.load(module);
    let result = vm.execute("main", vec![]).expect("vm run failed");
    // double(5) = 10, add_one(10) = 11
    assert_eq!(result.to_string(), "11");
}

// ============================================================================
// Variadic parameter tests
// ============================================================================

#[test]
fn variadic_param_compiles() {
    let src = r#"cell sum_all(...nums: Int) -> Int
  let total = 0
  for n in nums
    total = total + n
  end
  return total
end

cell main() -> Int
  return sum_all(1, 2, 3, 4, 5)
end"#;
    // Should compile without errors
    compile_to_lir(src);
}

#[test]
fn variadic_param_e2e() {
    use lumen_vm::vm::VM;
    let src = r#"cell sum_all(...nums: Int) -> Int
  let total = 0
  for n in nums
    total = total + n
  end
  return total
end

cell main() -> Int
  return sum_all(1, 2, 3, 4, 5)
end"#;
    let module = compile_to_lir(src);
    let mut vm = VM::new();
    vm.load(module);
    let result = vm.execute("main", vec![]).expect("vm run failed");
    assert_eq!(result.to_string(), "15");
}

#[test]
fn variadic_param_empty_args() {
    use lumen_vm::vm::VM;
    let src = r#"cell count(...items: String) -> Int
  let n = 0
  for _ in items
    n = n + 1
  end
  return n
end

cell main() -> Int
  return count()
end"#;
    let module = compile_to_lir(src);
    let mut vm = VM::new();
    vm.load(module);
    let result = vm.execute("main", vec![]).expect("vm run failed");
    assert_eq!(result.to_string(), "0");
}

#[test]
fn variadic_with_fixed_params() {
    use lumen_vm::vm::VM;
    let src = r#"cell format_list(sep: String, ...items: String) -> String
  let result = ""
  let first = true
  for item in items
    if first
      result = item
      first = false
    else
      result = result + sep + item
    end
  end
  return result
end

cell main() -> String
  return format_list(", ", "a", "b", "c")
end"#;
    let module = compile_to_lir(src);
    let mut vm = VM::new();
    vm.load(module);
    let result = vm.execute("main", vec![]).expect("vm run failed");
    assert_eq!(result.to_string(), "a, b, c");
}
