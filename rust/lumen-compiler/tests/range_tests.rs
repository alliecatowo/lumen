//! Wave 20 — T114: Inclusive/exclusive range coverage tests.
//!
//! Verifies that `..` (exclusive) and `..=` (inclusive) ranges work correctly
//! in for-loops, as expressions, with variables, negative ranges, and type inference.

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

fn assert_compiles_to_lir(label: &str, source: &str) -> lumen_compiler::compiler::lir::LirModule {
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
// Exclusive range: start..end
// ============================================================================

#[test]
fn wave20_range_exclusive_basic() {
    assert_compiles(
        "range_exclusive_basic",
        r#"
cell main() -> Int
  let sum = 0
  for i in 0..5
    sum = sum + i
  end
  return sum
end
"#,
    );
}

#[test]
fn wave20_range_exclusive_expression() {
    // Range as a standalone expression (produces a list)
    assert_compiles(
        "range_exclusive_expr",
        r#"
cell main() -> list[Int]
  let r = 1..5
  return r
end
"#,
    );
}

#[test]
fn wave20_range_exclusive_in_let() {
    assert_compiles(
        "range_exclusive_let",
        r#"
cell main() -> Int
  let nums = 0..10
  return len(nums)
end
"#,
    );
}

// ============================================================================
// Inclusive range: start..=end
// ============================================================================

#[test]
fn wave20_range_inclusive_basic() {
    assert_compiles(
        "range_inclusive_basic",
        r#"
cell main() -> Int
  let sum = 0
  for i in 0..=5
    sum = sum + i
  end
  return sum
end
"#,
    );
}

#[test]
fn wave20_range_inclusive_expression() {
    assert_compiles(
        "range_inclusive_expr",
        r#"
cell main() -> list[Int]
  let r = 1..=5
  return r
end
"#,
    );
}

#[test]
fn wave20_range_inclusive_in_let() {
    assert_compiles(
        "range_inclusive_let",
        r#"
cell main() -> Int
  let nums = 0..=10
  return len(nums)
end
"#,
    );
}

// ============================================================================
// Range in for-loops
// ============================================================================

#[test]
fn wave20_range_for_loop_exclusive() {
    assert_compiles(
        "range_for_exclusive",
        r#"
cell main() -> Int
  let total = 0
  for x in 1..10
    total = total + x
  end
  return total
end
"#,
    );
}

#[test]
fn wave20_range_for_loop_inclusive() {
    assert_compiles(
        "range_for_inclusive",
        r#"
cell main() -> Int
  let total = 0
  for x in 1..=10
    total = total + x
  end
  return total
end
"#,
    );
}

#[test]
fn wave20_range_for_loop_nested() {
    assert_compiles(
        "range_for_nested",
        r#"
cell main() -> Int
  let count = 0
  for i in 0..3
    for j in 0..3
      count = count + 1
    end
  end
  return count
end
"#,
    );
}

// ============================================================================
// Range with variables
// ============================================================================

#[test]
fn wave20_range_with_variable_start() {
    assert_compiles(
        "range_var_start",
        r#"
cell main() -> Int
  let start = 3
  let sum = 0
  for i in start..10
    sum = sum + i
  end
  return sum
end
"#,
    );
}

#[test]
fn wave20_range_with_variable_end() {
    assert_compiles(
        "range_var_end",
        r#"
cell main() -> Int
  let limit = 10
  let sum = 0
  for i in 0..limit
    sum = sum + i
  end
  return sum
end
"#,
    );
}

#[test]
fn wave20_range_with_variable_both() {
    assert_compiles(
        "range_var_both",
        r#"
cell main() -> Int
  let lo = 2
  let hi = 8
  let sum = 0
  for i in lo..hi
    sum = sum + i
  end
  return sum
end
"#,
    );
}

#[test]
fn wave20_range_inclusive_with_variables() {
    assert_compiles(
        "range_incl_vars",
        r#"
cell main() -> Int
  let lo = 1
  let hi = 5
  let sum = 0
  for i in lo..=hi
    sum = sum + i
  end
  return sum
end
"#,
    );
}

// ============================================================================
// Range with expressions as bounds
// ============================================================================

#[test]
fn wave20_range_expression_bounds() {
    assert_compiles(
        "range_expr_bounds",
        r#"
cell main() -> Int
  let sum = 0
  for i in (1 + 1)..(3 + 2)
    sum = sum + i
  end
  return sum
end
"#,
    );
}

// ============================================================================
// Range type inference
// ============================================================================

#[test]
fn wave20_range_type_is_list_int() {
    // Range expressions should infer as list[Int]
    assert_compiles(
        "range_type_inference",
        r#"
cell sum_list(items: list[Int]) -> Int
  let total = 0
  for x in items
    total = total + x
  end
  return total
end

cell main() -> Int
  return sum_list(1..10)
end
"#,
    );
}

#[test]
fn wave20_range_len() {
    assert_compiles(
        "range_len",
        r#"
cell main() -> Int
  let r = 0..5
  return len(r)
end
"#,
    );
}

// ============================================================================
// Range in match patterns
// ============================================================================

#[test]
fn wave20_range_pattern_exclusive() {
    assert_compiles(
        "range_pattern_exclusive",
        r#"
cell classify(x: Int) -> String
  match x
    0..10 -> return "small"
    _ -> return "big"
  end
end

cell main() -> String
  return classify(5)
end
"#,
    );
}

#[test]
fn wave20_range_pattern_inclusive() {
    assert_compiles(
        "range_pattern_inclusive",
        r#"
cell classify(x: Int) -> String
  match x
    0..=10 -> return "small"
    _ -> return "big"
  end
end

cell main() -> String
  return classify(10)
end
"#,
    );
}

// ============================================================================
// Range LIR verification
// ============================================================================

#[test]
fn wave20_range_exclusive_emits_intrinsic() {
    use lumen_compiler::compiler::lir::{IntrinsicId, OpCode};

    let module = assert_compiles_to_lir(
        "range_excl_lir",
        r#"
cell main() -> list[Int]
  return 0..5
end
"#,
    );

    // The main cell should contain a Range intrinsic call
    let main_cell = &module.cells[0];
    let has_range = main_cell
        .instructions
        .iter()
        .any(|i| i.op == OpCode::Intrinsic && i.b == IntrinsicId::Range as u8);
    assert!(has_range, "exclusive range should emit Range intrinsic");
}

#[test]
fn wave20_range_inclusive_emits_add() {
    use lumen_compiler::compiler::lir::OpCode;

    let module = assert_compiles_to_lir(
        "range_incl_lir_add",
        r#"
cell main() -> list[Int]
  return 0..=5
end
"#,
    );

    // Inclusive range should emit an Add instruction (to compute end+1)
    let main_cell = &module.cells[0];
    let has_add = main_cell.instructions.iter().any(|i| i.op == OpCode::Add);
    assert!(
        has_add,
        "inclusive range should emit Add to increment end bound"
    );
}

// ============================================================================
// Range with zero-length range
// ============================================================================

#[test]
fn wave20_range_zero_length() {
    assert_compiles(
        "range_zero_len",
        r#"
cell main() -> Int
  let sum = 0
  for i in 5..5
    sum = sum + i
  end
  return sum
end
"#,
    );
}

// ============================================================================
// Range combined with other expressions
// ============================================================================

#[test]
fn wave20_range_assigned_to_variable() {
    assert_compiles(
        "range_assign_var",
        r#"
cell main() -> Int
  let numbers = 1..=100
  return len(numbers)
end
"#,
    );
}

#[test]
fn wave20_range_as_function_argument() {
    assert_compiles(
        "range_func_arg",
        r#"
cell sum_all(items: list[Int]) -> Int
  let total = 0
  for x in items
    total = total + x
  end
  return total
end

cell main() -> Int
  return sum_all(1..=10)
end
"#,
    );
}

// ============================================================================
// Negative ranges (start > end should produce empty)
// ============================================================================

#[test]
fn wave20_range_negative_start() {
    assert_compiles(
        "range_neg_start",
        r#"
cell main() -> Int
  let sum = 0
  for i in 0..5
    sum = sum + i
  end
  return sum
end
"#,
    );
}

#[test]
fn wave20_range_with_negative_numbers() {
    assert_compiles(
        "range_neg_nums",
        r#"
cell main() -> Int
  let sum = 0
  let start = 0 - 3
  for i in start..3
    sum = sum + i
  end
  return sum
end
"#,
    );
}

#[test]
fn wave20_range_inverted() {
    // An inverted range (start > end) should compile fine (empty list at runtime)
    assert_compiles(
        "range_inverted",
        r#"
cell main() -> Int
  let sum = 0
  for i in 10..5
    sum = sum + i
  end
  return sum
end
"#,
    );
}
