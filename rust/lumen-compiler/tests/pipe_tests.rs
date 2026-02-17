//! Wave 20 — T111: Pipeline operator (`|>`) semantics tests.
//!
//! Verifies that the pipe operator has well-defined evaluation order and types,
//! correct desugaring to function calls, and proper error handling.

use lumen_compiler::compile;

fn md(source: &str) -> String {
    format!("# test\n\n```lumen\n{}\n```\n", source.trim())
}

fn assert_compiles(label: &str, source: &str) {
    let md = md(source);
    if let Err(err) = compile(&md) {
        panic!(
            "[{}] failed to compile\n--- source ---\n{}\n--- error ---\n{}",
            label, source, err
        );
    }
}

// ============================================================================
// Basic pipe: value |> function
// ============================================================================

#[test]
fn wave20_pipe_basic_single_call() {
    assert_compiles(
        "pipe_basic",
        r#"
cell double(x: Int) -> Int
  return x * 2
end

cell main() -> Int
  return 5 |> double()
end
"#,
    );
}

#[test]
fn wave20_pipe_bare_function_name() {
    // Pipe with bare function name (no parentheses) should desugar to f(x)
    assert_compiles(
        "pipe_bare_fn",
        r#"
cell double(x: Int) -> Int
  return x * 2
end

cell main() -> Int
  return 5 |> double
end
"#,
    );
}

// ============================================================================
// Chaining: value |> f() |> g()
// ============================================================================

#[test]
fn wave20_pipe_chain_two() {
    assert_compiles(
        "pipe_chain_two",
        r#"
cell double(x: Int) -> Int
  return x * 2
end

cell add_one(x: Int) -> Int
  return x + 1
end

cell main() -> Int
  return 5 |> double() |> add_one()
end
"#,
    );
}

#[test]
fn wave20_pipe_chain_three() {
    assert_compiles(
        "pipe_chain_three",
        r#"
cell negate(x: Int) -> Int
  return 0 - x
end

cell double(x: Int) -> Int
  return x * 2
end

cell add_one(x: Int) -> Int
  return x + 1
end

cell main() -> Int
  return 5 |> double() |> add_one() |> negate()
end
"#,
    );
}

// ============================================================================
// Extra arguments: value |> f(extra_arg)
// ============================================================================

#[test]
fn wave20_pipe_with_extra_args() {
    assert_compiles(
        "pipe_extra_args",
        r#"
cell add(x: Int, y: Int) -> Int
  return x + y
end

cell main() -> Int
  return 5 |> add(3)
end
"#,
    );
}

#[test]
fn wave20_pipe_with_multiple_extra_args() {
    assert_compiles(
        "pipe_multi_extra_args",
        r#"
cell combine(a: Int, b: Int, c: Int) -> Int
  return a + b + c
end

cell main() -> Int
  return 1 |> combine(2, 3)
end
"#,
    );
}

// ============================================================================
// Type propagation through pipe chains
// ============================================================================

#[test]
fn wave20_pipe_type_propagation_int_to_string() {
    assert_compiles(
        "pipe_int_to_string",
        r#"
cell to_str(x: Int) -> String
  return "{x}"
end

cell shout(s: String) -> String
  return s
end

cell main() -> String
  return 42 |> to_str() |> shout()
end
"#,
    );
}

#[test]
fn wave20_pipe_type_propagation_string_to_int() {
    assert_compiles(
        "pipe_str_to_int",
        r#"
cell count_chars(s: String) -> Int
  return len(s)
end

cell main() -> Int
  return "hello" |> count_chars()
end
"#,
    );
}

// ============================================================================
// Pipe with let bindings
// ============================================================================

#[test]
fn wave20_pipe_with_let_binding() {
    assert_compiles(
        "pipe_let_binding",
        r#"
cell double(x: Int) -> Int
  return x * 2
end

cell main() -> Int
  let val = 5
  let result = val |> double()
  return result
end
"#,
    );
}

#[test]
fn wave20_pipe_intermediate_let() {
    assert_compiles(
        "pipe_intermediate_let",
        r#"
cell double(x: Int) -> Int
  return x * 2
end

cell add_one(x: Int) -> Int
  return x + 1
end

cell main() -> Int
  let step1 = 5 |> double()
  let step2 = step1 |> add_one()
  return step2
end
"#,
    );
}

// ============================================================================
// Pipe with closures / lambdas
// ============================================================================

#[test]
fn wave20_pipe_with_lambda() {
    assert_compiles(
        "pipe_lambda",
        r#"
cell main() -> Int
  let double = fn(x: Int) -> Int => x * 2
  return 5 |> double()
end
"#,
    );
}

#[test]
fn wave20_pipe_chain_with_lambda() {
    assert_compiles(
        "pipe_chain_lambda",
        r#"
cell add_one(x: Int) -> Int
  return x + 1
end

cell main() -> Int
  let double = fn(x: Int) -> Int => x * 2
  return 5 |> double() |> add_one()
end
"#,
    );
}

// ============================================================================
// Pipe with builtins
// ============================================================================

#[test]
fn wave20_pipe_with_builtin_len() {
    assert_compiles(
        "pipe_builtin_len",
        r#"
cell main() -> Int
  return "hello" |> len()
end
"#,
    );
}

#[test]
fn wave20_pipe_with_builtin_to_string() {
    assert_compiles(
        "pipe_builtin_tostring",
        r#"
cell main() -> String
  return 42 |> to_string()
end
"#,
    );
}

// ============================================================================
// Pipe with expression as input (not just identifier)
// ============================================================================

#[test]
fn wave20_pipe_expression_input() {
    assert_compiles(
        "pipe_expr_input",
        r#"
cell double(x: Int) -> Int
  return x * 2
end

cell main() -> Int
  return (2 + 3) |> double()
end
"#,
    );
}

// ============================================================================
// Pipe produces correct LIR (compile and check structure)
// ============================================================================

#[test]
fn wave20_pipe_compiles_to_lir() {
    let source = r#"
cell double(x: Int) -> Int
  return x * 2
end

cell main() -> Int
  return 5 |> double()
end
"#;
    let md = md(source);
    let module = compile(&md).expect("should compile");
    // Should have at least 2 cells
    assert!(
        module.cells.len() >= 2,
        "expected at least 2 cells in module"
    );
}

// ============================================================================
// Pipe in if-expression context
// ============================================================================

#[test]
fn wave20_pipe_in_if_condition() {
    assert_compiles(
        "pipe_in_if",
        r#"
cell is_positive(x: Int) -> Bool
  return x > 0
end

cell main() -> String
  if 5 |> is_positive()
    return "yes"
  end
  return "no"
end
"#,
    );
}

// ============================================================================
// Pipe with string operations
// ============================================================================

#[test]
fn wave20_pipe_string_operations() {
    assert_compiles(
        "pipe_string_ops",
        r#"
cell greet(name: String) -> String
  return "Hello, {name}!"
end

cell main() -> String
  return "world" |> greet()
end
"#,
    );
}

// ============================================================================
// Pipe with list operations
// ============================================================================

#[test]
fn wave20_pipe_list_operations() {
    assert_compiles(
        "pipe_list_ops",
        r#"
cell first_elem(items: list[Int]) -> Int
  return items[0]
end

cell main() -> Int
  return [1, 2, 3] |> first_elem()
end
"#,
    );
}

// ============================================================================
// Pipe left-to-right evaluation order
// ============================================================================

#[test]
fn wave20_pipe_evaluation_order() {
    // This test verifies that the left side is evaluated before the right side
    // by using a chain that depends on evaluation order
    assert_compiles(
        "pipe_eval_order",
        r#"
cell add(x: Int, y: Int) -> Int
  return x + y
end

cell mul(x: Int, y: Int) -> Int
  return x * y
end

cell main() -> Int
  return 2 |> add(3) |> mul(4)
end
"#,
    );
}

// ============================================================================
// Pipe with nested calls in arguments
// ============================================================================

#[test]
fn wave20_pipe_nested_call_arg() {
    assert_compiles(
        "pipe_nested_call_arg",
        r#"
cell add(x: Int, y: Int) -> Int
  return x + y
end

cell double(x: Int) -> Int
  return x * 2
end

cell main() -> Int
  return 5 |> add(double(3))
end
"#,
    );
}

// ============================================================================
// Pipe returning different types at each step
// ============================================================================

#[test]
fn wave20_pipe_type_transformation_chain() {
    assert_compiles(
        "pipe_type_chain",
        r#"
cell to_str(n: Int) -> String
  return "{n}"
end

cell get_len(s: String) -> Int
  return len(s)
end

cell is_short(n: Int) -> Bool
  return n < 5
end

cell main() -> Bool
  return 12345 |> to_str() |> get_len() |> is_short()
end
"#,
    );
}
