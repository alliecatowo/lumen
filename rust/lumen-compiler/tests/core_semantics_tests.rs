//! Comprehensive tests for core language semantics:
//! - Type inference (let bindings, literals, binary ops)
//! - Pattern matching (nested patterns, exhaustiveness)
//! - Effect inference (call chains)
//! - Comprehension lowering (set/map opcodes)
//! - Closure captures (single and nested)

use lumen_compiler::compiler::lexer::Lexer;
use lumen_compiler::compiler::lower::lower;
use lumen_compiler::compiler::lir::OpCode;
use lumen_compiler::compiler::parser::Parser;
use lumen_compiler::compiler::resolve::resolve;
use lumen_compiler::compiler::typecheck::typecheck;

fn compile_and_typecheck(src: &str) -> Result<lumen_compiler::compiler::lir::LirModule, String> {
    let mut lexer = Lexer::new(src, 1, 0);
    let tokens = lexer.tokenize().map_err(|e| format!("{:?}", e))?;
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program(vec![]).map_err(|e| format!("{:?}", e))?;
    let symbols = resolve(&program).map_err(|e| format!("{:?}", e))?;
    typecheck(&program, &symbols).map_err(|e| format!("{:?}", e))?;
    Ok(lower(&program, &symbols, src))
}

fn compile_expect_error(src: &str) -> String {
    let mut lexer = Lexer::new(src, 1, 0);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program(vec![]).unwrap();
    let symbols = resolve(&program).unwrap();
    match typecheck(&program, &symbols) {
        Ok(_) => panic!("Expected error but compilation succeeded"),
        Err(e) => format!("{:?}", e),
    }
}

// ============================================================================
// TYPE INFERENCE TESTS
// ============================================================================

#[test]
fn test_type_inference_int_literal() {
    let src = r#"
cell test() -> Int
  let x = 5
  return x
end
"#;
    compile_and_typecheck(src).unwrap();
}

#[test]
fn test_type_inference_string_literal() {
    let src = r#"
cell test() -> String
  let x = "hello"
  return x
end
"#;
    compile_and_typecheck(src).unwrap();
}

#[test]
fn test_type_inference_bool_literal() {
    let src = r#"
cell test() -> Bool
  let x = true
  return x
end
"#;
    compile_and_typecheck(src).unwrap();
}

#[test]
fn test_type_inference_float_literal() {
    let src = r#"
cell test() -> Float
  let x = 3.14
  return x
end
"#;
    compile_and_typecheck(src).unwrap();
}

#[test]
fn test_type_inference_list_literal() {
    let src = r#"
cell test() -> list[Int]
  let x = [1, 2, 3]
  return x
end
"#;
    compile_and_typecheck(src).unwrap();
}

#[test]
fn test_type_inference_empty_list() {
    let src = r#"
cell test() -> list[String]
  let x: list[String] = []
  return x
end
"#;
    compile_and_typecheck(src).unwrap();
}

#[test]
fn test_binop_int_plus_int() {
    let src = r#"
cell test() -> Int
  let x = 1 + 2
  return x
end
"#;
    compile_and_typecheck(src).unwrap();
}

#[test]
fn test_binop_string_concat() {
    let src = r#"
cell test() -> String
  let x = "hello" + " world"
  return x
end
"#;
    compile_and_typecheck(src).unwrap();
}

#[test]
fn test_comparison_returns_bool() {
    let src = r#"
cell test() -> Bool
  let x = 5 > 3
  return x
end
"#;
    compile_and_typecheck(src).unwrap();
}

#[test]
fn test_type_mismatch_int_to_string() {
    let src = r#"
cell test() -> String
  let x = 42
  return x
end
"#;
    let err = compile_expect_error(src);
    assert!(err.contains("Mismatch") || err.contains("expected"));
}

// ============================================================================
// PATTERN MATCHING TESTS
// ============================================================================

#[test]
fn test_nested_pattern_some_ok() {
    let src = r#"
enum Option[T]
  some(T)
  none
end

cell test(x: result[Option[Int], String]) -> Int
  match x
    ok(some(v)) -> return v
    ok(none) -> return 0
    err(_) -> return -1
  end
end
"#;
    compile_and_typecheck(src).unwrap();
}

#[test]
fn test_nested_pattern_deeper_some_ok() {
    let src = r#"
enum Option[T]
  some(T)
  none
end

cell test(x: result[Option[Option[Int]], String]) -> Int
  match x
    ok(some(some(v))) -> return v
    ok(some(none)) -> return 1
    ok(none) -> return 0
    err(_) -> return -1
  end
end
"#;
    compile_and_typecheck(src).unwrap();
}

#[test]
fn test_pattern_binding_types() {
    let src = r#"
enum Option[T]
  some(T)
  none
end

cell test(x: Option[Int]) -> Int
  match x
    some(v) -> return v
    none -> return 0
  end
end
"#;
    compile_and_typecheck(src).unwrap();
}

#[test]
fn test_wildcard_pattern() {
    let src = r#"
enum Color
  red
  green
  blue
end

cell test(c: Color) -> Int
  match c
    red -> return 1
    _ -> return 0
  end
end
"#;
    compile_and_typecheck(src).unwrap();
}

#[test]
fn test_exhaustiveness_error_reports_missing_variants() {
    let src = r#"
enum Color
  red
  green
  blue
end

cell test(c: Color) -> Int
  match c
    red -> return 1
    green -> return 2
  end
end
"#;
    let err = compile_expect_error(src);
    assert!(err.contains("IncompleteMatch"), "Should report incomplete match");
    assert!(err.contains("blue") || err.contains("missing"), "Should mention missing variant 'blue'");
}

#[test]
fn test_list_destructure_pattern() {
    let src = r#"
cell test(xs: list[Int]) -> Int
  match xs
    [first, ...rest] -> return first
    [] -> return 0
  end
end
"#;
    compile_and_typecheck(src).unwrap();
}

#[test]
fn test_tuple_destructure_pattern() {
    let src = r#"
cell test(t: (Int, String)) -> Int
  match t
    (x, _) -> return x
  end
end
"#;
    compile_and_typecheck(src).unwrap();
}

// ============================================================================
// EFFECT INFERENCE TESTS
// ============================================================================

#[test]
fn test_effect_inference_call_chain() {
    let src = r#"
use tool HttpGet
grant HttpGet {}
bind effect http to HttpGet

cell fetch_data() -> String / {http}
  return HttpGet(url: "https://example.com")
end

cell process() -> String / {http}
  return fetch_data()
end
"#;
    compile_and_typecheck(src).unwrap();
}

#[test]
fn test_effect_violation_missing_declaration() {
    let src = r#"
use tool HttpGet

cell fetch_data() -> String / {http}
  return HttpGet(url: "https://example.com")
end

cell process() -> String
  return fetch_data()
end
"#;
    let err_str = format!("{:?}", resolve_only(src).unwrap_err());
    assert!(
        err_str.contains("EffectContractViolation") || err_str.contains("http"),
        "Should report effect violation: {}",
        err_str
    );
}

#[test]
fn test_effect_call_chain_with_context() {
    // This tests that error messages include the call chain
    let src = r#"
use tool HttpGet

cell fetch() -> String / {http}
  return HttpGet(url: "https://example.com")
end

cell process() -> String / {http}
  return fetch()
end

cell main() -> String
  return process()
end
"#;
    let err_str = format!("{:?}", resolve_only(src).unwrap_err());
    assert!(
        err_str.contains("process") && err_str.contains("http"),
        "Error should mention 'process' and 'http': {}",
        err_str
    );
}

// Helper for effect tests
fn resolve_only(src: &str) -> Result<(), Vec<lumen_compiler::compiler::resolve::ResolveError>> {
    let mut lexer = Lexer::new(src, 1, 0);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program(vec![]).unwrap();
    resolve(&program).map(|_| ())
}

// ============================================================================
// COMPREHENSION LOWERING TESTS
// ============================================================================

#[test]
fn test_set_comprehension_emits_toset() {
    let src = r#"
cell test() -> set[Int]
  return {x * 2 for x in [1, 2, 3]}
end
"#;
    let module = compile_and_typecheck(src).unwrap();
    let test_cell = module.cells.iter().find(|c| c.name == "test").expect("Should have test cell");
    let ops: Vec<_> = test_cell.instructions.iter().map(|i| i.op).collect();

    // Set comprehensions should:
    // 1. Build as list (NewList, Append)
    // 2. Convert to set using ToSet intrinsic
    assert!(
        ops.contains(&OpCode::NewList),
        "Set comprehension should build list first"
    );
    assert!(
        ops.contains(&OpCode::Intrinsic),
        "Set comprehension should use Intrinsic for ToSet conversion"
    );
}

#[test]
fn test_list_comprehension_returns_list() {
    let src = r#"
cell test() -> list[Int]
  return [x * 2 for x in [1, 2, 3]]
end
"#;
    let module = compile_and_typecheck(src).unwrap();
    let test_cell = module.cells.iter().find(|c| c.name == "test").expect("Should have test cell");
    let ops: Vec<_> = test_cell.instructions.iter().map(|i| i.op).collect();

    assert!(
        ops.contains(&OpCode::NewList),
        "List comprehension should emit NewList"
    );
    assert!(
        ops.contains(&OpCode::Append),
        "List comprehension should emit Append"
    );
}

#[test]
fn test_map_comprehension_emits_newmap() {
    let src = r#"
cell test() -> map[String, Int]
  return {(str(x), x) for x in [1, 2, 3]}
end
"#;
    let module = compile_and_typecheck(src).unwrap();
    let test_cell = module.cells.iter().find(|c| c.name == "test").expect("Should have test cell");
    let ops: Vec<_> = test_cell.instructions.iter().map(|i| i.op).collect();

    assert!(
        ops.contains(&OpCode::NewMap),
        "Map comprehension should emit NewMap: {:?}",
        ops
    );
    assert!(
        ops.contains(&OpCode::SetIndex),
        "Map comprehension should emit SetIndex"
    );
}

#[test]
fn test_set_literal_type_inference() {
    let src = r#"
cell test() -> set[Int]
  let s = {1, 2, 3}
  return s
end
"#;
    compile_and_typecheck(src).unwrap();
}

#[test]
fn test_map_literal_type_inference() {
    let src = r#"
cell test() -> map[String, Int]
  let m = {"a": 1, "b": 2}
  return m
end
"#;
    compile_and_typecheck(src).unwrap();
}

// ============================================================================
// CLOSURE CAPTURE TESTS
// ============================================================================

#[test]
fn test_simple_closure_capture() {
    let src = r#"
cell test() -> Int
  let x = 10
  let f = fn() -> Int => x end
  return f()
end
"#;
    let module = compile_and_typecheck(src).unwrap();

    // Outer cell should emit Closure and SetUpval
    let test_cell = module.cells.iter().find(|c| c.name == "test").expect("Should have test cell");
    let outer_ops: Vec<_> = test_cell.instructions.iter().map(|i| i.op).collect();
    assert!(
        outer_ops.contains(&OpCode::Closure),
        "Outer cell should emit Closure"
    );
    assert!(
        outer_ops.contains(&OpCode::SetUpval),
        "Outer cell should emit SetUpval for captured variable"
    );

    // Lambda cell should have GetUpval for the capture
    // Find the lambda cell (name starts with "<lambda/")
    let lambda = module.cells.iter().find(|c| c.name.starts_with("<lambda/")).expect("Should have lambda cell");
    let lambda_ops: Vec<_> = lambda.instructions.iter().map(|i| i.op).collect();
    assert!(
        lambda_ops.contains(&OpCode::GetUpval),
        "Lambda should emit GetUpval for captured variable"
    );
}

#[test]
fn test_nested_closure_capture() {
    let src = r#"
cell test() -> Int
  let x = 1
  let f = fn() -> Int
    let g = fn() -> Int => x end
    return g()
  end
  return f()
end
"#;
    let module = compile_and_typecheck(src).unwrap();

    // Should have 2 lambda cells (f and g)
    let lambda_cells: Vec<_> = module.cells.iter().filter(|c| c.name.starts_with("<lambda/")).collect();
    assert_eq!(lambda_cells.len(), 2, "Should have 2 lambda cells");

    // At least one of the lambdas should capture x
    let has_getupval = lambda_cells.iter().any(|cell| {
        cell.instructions.iter().any(|i| i.op == OpCode::GetUpval)
    });
    assert!(
        has_getupval,
        "At least one lambda should use GetUpval for captured variable"
    );
}

#[test]
fn test_closure_with_params_and_captures() {
    let src = r#"
cell test() -> Int
  let x = 10
  let f = fn(y: Int) -> Int => x + y end
  return f(5)
end
"#;
    let module = compile_and_typecheck(src).unwrap();

    // Lambda should have both captures and params
    let lambda = module.cells.iter().find(|c| c.name.starts_with("<lambda/")).expect("Should have lambda cell");
    // First param is __capture_x, second is y
    assert_eq!(
        lambda.params.len(),
        2,
        "Lambda should have 2 params (1 capture + 1 actual param)"
    );
    assert!(
        lambda.params[0].name.contains("capture"),
        "First param should be capture: {}",
        lambda.params[0].name
    );
    assert_eq!(lambda.params[1].name, "y", "Second param should be 'y'");
}

#[test]
fn test_multiple_captures() {
    let src = r#"
cell test() -> Int
  let x = 10
  let y = 20
  let f = fn() -> Int => x + y end
  return f()
end
"#;
    let module = compile_and_typecheck(src).unwrap();

    // Count SetUpval instructions in the test cell (should be 2)
    let test_cell = module.cells.iter().find(|c| c.name == "test").expect("Should have test cell");
    let outer_ops: Vec<_> = test_cell.instructions.iter().map(|i| i.op).collect();
    let setupval_count = outer_ops.iter().filter(|&&op| op == OpCode::SetUpval).count();
    assert_eq!(
        setupval_count, 2,
        "Should emit 2 SetUpval instructions for 2 captures"
    );

    // Lambda should have 2 GetUpval instructions
    let lambda = module.cells.iter().find(|c| c.name.starts_with("<lambda/")).expect("Should have lambda cell");
    let getupval_count = lambda
        .instructions
        .iter()
        .filter(|i| i.op == OpCode::GetUpval)
        .count();
    assert_eq!(
        getupval_count, 2,
        "Lambda should load both captures via GetUpval"
    );
}

#[test]
fn test_lambda_no_captures_only_params() {
    let src = r#"
cell test() -> Int
  let f = fn(x: Int) -> Int => x * 2 end
  return f(5)
end
"#;
    let module = compile_and_typecheck(src).unwrap();

    let lambda = module.cells.iter().find(|c| c.name.starts_with("<lambda/")).expect("Should have lambda cell");
    // Should only have the actual parameter, no captures
    assert_eq!(
        lambda.params.len(),
        1,
        "Lambda using only its own params should have 1 param, not captures"
    );
    assert_eq!(lambda.params[0].name, "x");

    // Should not emit GetUpval
    let ops: Vec<_> = lambda.instructions.iter().map(|i| i.op).collect();
    assert!(
        !ops.contains(&OpCode::GetUpval),
        "Lambda without captures should not emit GetUpval"
    );
}

// ============================================================================
// INTEGRATION TESTS
// ============================================================================

#[test]
fn test_complex_nested_types_with_inference() {
    let src = r#"
enum Option[T]
  some(T)
  none
end

cell test() -> list[Option[Int]]
  let x = [some(1), some(2), none]
  return x
end
"#;
    compile_and_typecheck(src).unwrap();
}

#[test]
fn test_result_type_with_pattern_matching() {
    let src = r#"
cell divide(a: Int, b: Int) -> result[Int, String]
  match b
    0 -> return err("division by zero")
    _ -> return ok(a / b)
  end
end

cell safe_divide(a: Int, b: Int) -> Int
  match divide(a, b)
    ok(v) -> return v
    err(_) -> return 0
  end
end
"#;
    compile_and_typecheck(src).unwrap();
}
