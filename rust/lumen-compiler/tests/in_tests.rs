//! Wave 20 — T115: Membership operator `in` tests.
//!
//! Verifies that `x in collection` works as a standalone boolean expression
//! for lists, sets, maps (key check), and strings (substring check).

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

fn assert_compiles_to_lir(label: &str, source: &str) -> lumen_core::lir::LirModule {
    let md = md(source);
    match compile(&md) {
        Ok(module) => module,
        Err(err) => panic!(
            "[{}] failed to compile\n--- source ---\n{}\n--- error ---\n{}",
            label, source, err
        ),
    }
}

// ============================================================================
// In with list
// ============================================================================

#[test]
fn in_list_literal() {
    assert_compiles(
        "in_list_literal",
        r#"
cell main() -> Bool
  return 3 in [1, 2, 3, 4, 5]
end
"#,
    );
}

#[test]
fn in_list_variable() {
    assert_compiles(
        "in_list_var",
        r#"
cell main() -> Bool
  let items = [10, 20, 30]
  return 20 in items
end
"#,
    );
}

#[test]
fn in_list_not_found() {
    assert_compiles(
        "in_list_not_found",
        r#"
cell main() -> Bool
  return 99 in [1, 2, 3]
end
"#,
    );
}

#[test]
fn in_list_string_elements() {
    assert_compiles(
        "in_list_strings",
        r#"
cell main() -> Bool
  let names = ["alice", "bob", "charlie"]
  return "bob" in names
end
"#,
    );
}

#[test]
fn in_empty_list() {
    assert_compiles(
        "in_empty_list",
        r#"
cell main() -> Bool
  let items: list[Int] = []
  return 1 in items
end
"#,
    );
}

// ============================================================================
// In with set
// ============================================================================

#[test]
fn in_set_literal() {
    assert_compiles(
        "in_set_literal",
        r#"
cell main() -> Bool
  return 3 in {1, 2, 3, 4, 5}
end
"#,
    );
}

#[test]
fn in_set_variable() {
    assert_compiles(
        "in_set_var",
        r#"
cell main() -> Bool
  let items = {10, 20, 30}
  return 20 in items
end
"#,
    );
}

// ============================================================================
// In with map (key check)
// ============================================================================

#[test]
fn in_map_key_check() {
    assert_compiles(
        "in_map_key",
        r#"
cell main() -> Bool
  let m = {"name": "Alice", "age": "30"}
  return "name" in m
end
"#,
    );
}

#[test]
fn in_map_missing_key() {
    assert_compiles(
        "in_map_missing",
        r#"
cell main() -> Bool
  let m = {"a": 1, "b": 2}
  return "c" in m
end
"#,
    );
}

// ============================================================================
// In with string (substring check)
// ============================================================================

#[test]
fn in_string_substring() {
    assert_compiles(
        "in_string_substr",
        r#"
cell main() -> Bool
  return "ell" in "hello"
end
"#,
    );
}

#[test]
fn in_string_not_found() {
    assert_compiles(
        "in_string_not_found",
        r#"
cell main() -> Bool
  return "xyz" in "hello"
end
"#,
    );
}

#[test]
fn in_string_with_variables() {
    assert_compiles(
        "in_string_vars",
        r#"
cell main() -> Bool
  let haystack = "The quick brown fox"
  let needle = "quick"
  return needle in haystack
end
"#,
    );
}

// ============================================================================
// In returns Bool type
// ============================================================================

#[test]
fn in_returns_bool_type() {
    // Verify that `in` expression has Bool type (usable in if conditions)
    assert_compiles(
        "in_returns_bool",
        r#"
cell main() -> String
  let items = [1, 2, 3]
  if 2 in items
    return "found"
  end
  return "not found"
end
"#,
    );
}

#[test]
fn in_in_let_binding() {
    assert_compiles(
        "in_let_binding",
        r#"
cell main() -> Bool
  let items = [1, 2, 3]
  let found = 2 in items
  return found
end
"#,
    );
}

// ============================================================================
// In with negation (not ... in ...)
// ============================================================================

#[test]
fn in_with_not() {
    assert_compiles(
        "in_with_not",
        r#"
cell main() -> Bool
  let items = [1, 2, 3]
  return not (4 in items)
end
"#,
    );
}

// ============================================================================
// In combined with and/or
// ============================================================================

#[test]
fn in_combined_with_and() {
    assert_compiles(
        "in_with_and",
        r#"
cell main() -> Bool
  let a = [1, 2, 3]
  let b = [4, 5, 6]
  return 2 in a and 5 in b
end
"#,
    );
}

#[test]
fn in_combined_with_or() {
    assert_compiles(
        "in_with_or",
        r#"
cell main() -> Bool
  let items = [1, 2, 3]
  return 2 in items or 99 in items
end
"#,
    );
}

// ============================================================================
// In emits OpCode::In
// ============================================================================

#[test]
fn in_emits_in_opcode() {
    use lumen_core::lir::OpCode;

    let module = assert_compiles_to_lir(
        "in_opcode",
        r#"
cell main() -> Bool
  return 3 in [1, 2, 3]
end
"#,
    );

    let main_cell = &module.cells[0];
    let has_in = main_cell.instructions.iter().any(|i| i.op == OpCode::In);
    assert!(has_in, "should emit In opcode for 'in' expression");
}

// ============================================================================
// In with function call result
// ============================================================================

#[test]
fn in_with_function_result() {
    assert_compiles(
        "in_func_result",
        r#"
cell get_items() -> list[Int]
  return [1, 2, 3, 4, 5]
end

cell main() -> Bool
  return 3 in get_items()
end
"#,
    );
}

// ============================================================================
// In as function argument
// ============================================================================

#[test]
fn in_as_argument() {
    assert_compiles(
        "in_as_arg",
        r#"
cell check(b: Bool) -> String
  if b
    return "yes"
  end
  return "no"
end

cell main() -> String
  return check(3 in [1, 2, 3])
end
"#,
    );
}

// ============================================================================
// In in while condition
// ============================================================================

#[test]
fn in_while_condition() {
    assert_compiles(
        "in_while_cond",
        r#"
cell main() -> Int
  let mut items = [1, 2, 3, 4, 5]
  let target = 3
  let count = 0
  while target in items
    items = [1, 2]
    count = count + 1
  end
  return count
end
"#,
    );
}

// ============================================================================
// In with nested collections
// ============================================================================

#[test]
fn in_nested_expression() {
    assert_compiles(
        "in_nested",
        r#"
cell main() -> Bool
  let x = 5
  return (x * 2) in [8, 10, 12]
end
"#,
    );
}

// ============================================================================
// In for-loop context (existing usage, regression test)
// ============================================================================

#[test]
fn in_for_loop_regression() {
    // The `in` keyword is used in for-loops; ensure it still works there
    assert_compiles(
        "in_for_loop_regression",
        r#"
cell main() -> Int
  let sum = 0
  for x in [1, 2, 3]
    sum = sum + x
  end
  return sum
end
"#,
    );
}

#[test]
fn in_for_loop_with_range() {
    assert_compiles(
        "in_for_range",
        r#"
cell main() -> Int
  let sum = 0
  for x in 1..10
    sum = sum + x
  end
  return sum
end
"#,
    );
}

// ============================================================================
// In with match expressions
// ============================================================================

#[test]
fn in_match_guard() {
    assert_compiles(
        "in_match_guard",
        r#"
cell classify(x: Int) -> String
  let specials = [1, 3, 5, 7]
  match x
    n if n in specials -> return "special"
    _ -> return "normal"
  end
end

cell main() -> String
  return classify(3)
end
"#,
    );
}

// ============================================================================
// Multiple in expressions in same scope
// ============================================================================

#[test]
fn in_multiple_same_scope() {
    assert_compiles(
        "in_multi_scope",
        r#"
cell main() -> Int
  let a = [1, 2, 3]
  let b = [4, 5, 6]
  let count = 0
  if 2 in a
    count = count + 1
  end
  if 5 in b
    count = count + 1
  end
  return count
end
"#,
    );
}
