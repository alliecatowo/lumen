//! Wave 19 Agent C test suite.
//!
//! Tests for three falsely-completed compiler features:
//! - T201: Nested list comprehension scoping
//! - T205: Let destructuring with type patterns
//! - T209: Result/optional sugar completion (expr!, ??)

use lumen_compiler::compile;

fn markdown(code: &str) -> String {
    format!("# test\n\n```lumen\n{}\n```\n", code.trim())
}

fn assert_ok(id: &str, code: &str) {
    let md = markdown(code);
    if let Err(err) = compile(&md) {
        panic!("case '{}' failed to compile:\n{}", id, err);
    }
}

fn assert_err(id: &str, code: &str, expect: &str) {
    let md = markdown(code);
    match compile(&md) {
        Ok(_) => panic!("case '{}' unexpectedly compiled", id),
        Err(err) => {
            let msg = err.to_string().to_lowercase();
            assert!(
                msg.contains(&expect.to_lowercase()),
                "case '{}' error mismatch\nexpected substring: {}\nactual: {}",
                id,
                expect,
                err
            );
        }
    }
}

// ============================================================================
// T201: Nested list comprehension scoping
// ============================================================================

#[test]
fn t201_nested_comprehension_two_vars() {
    // Both x and y should be in scope for the output expression
    assert_ok(
        "t201_nested_comprehension_two_vars",
        r#"
cell main() -> list[Int]
  [x + y for x in [1, 2] for y in [10, 20]]
end
"#,
    );
}

#[test]
fn t201_nested_comprehension_three_vars() {
    // Three nested for-clauses: x, y, z all visible in body
    assert_ok(
        "t201_nested_comprehension_three_vars",
        r#"
cell main() -> list[Int]
  [x + y + z for x in [1, 2] for y in [10, 20] for z in [100, 200]]
end
"#,
    );
}

#[test]
fn t201_nested_comprehension_with_condition() {
    // Nested comprehension with filter condition referencing both vars
    assert_ok(
        "t201_nested_comprehension_with_condition",
        r#"
cell main() -> list[Int]
  [x + y for x in [1, 2, 3] for y in [10, 20, 30] if x + y > 12]
end
"#,
    );
}

#[test]
fn t201_nested_comprehension_string_interp() {
    // Nested comprehension producing strings from both variables
    assert_ok(
        "t201_nested_comprehension_string_interp",
        r#"
cell main() -> list[String]
  ["{x}-{y}" for x in [1, 2] for y in ["a", "b"]]
end
"#,
    );
}

#[test]
fn t201_comprehension_vars_dont_leak() {
    // Comprehension variables must not be visible after the comprehension
    assert_err(
        "t201_comprehension_vars_dont_leak",
        r#"
cell main() -> Int
  let result = [x + y for x in [1, 2] for y in [10, 20]]
  x
end
"#,
        "undefined",
    );
}

#[test]
fn t201_comprehension_inner_var_doesnt_leak() {
    // Even the inner (extra clause) variable must not leak
    assert_err(
        "t201_comprehension_inner_var_doesnt_leak",
        r#"
cell main() -> Int
  let result = [x + y for x in [1, 2] for y in [10, 20]]
  y
end
"#,
        "undefined",
    );
}

#[test]
fn t201_set_comprehension_nested() {
    // Nested set comprehension (set literals use { } syntax)
    assert_ok(
        "t201_set_comprehension_nested",
        r#"
cell main() -> set[Int]
  {x * y for x in {1, 2, 3} for y in {10, 20}}
end
"#,
    );
}

#[test]
fn t201_comprehension_outer_var_visible() {
    // Variables defined before comprehension should still be accessible
    // both inside and after the comprehension
    assert_ok(
        "t201_comprehension_outer_var_visible",
        r#"
cell main() -> list[Int]
  let offset = 100
  let result = [x + offset for x in [1, 2, 3]]
  let total = offset + 1
  result
end
"#,
    );
}

// ============================================================================
// T205: Let destructuring with type patterns
// ============================================================================

#[test]
fn t205_tuple_destructure_basic() {
    // Basic tuple destructuring: let (a, b) = (1, 2)
    assert_ok(
        "t205_tuple_destructure_basic",
        r#"
cell main() -> Int
  let (a, b) = (1, 2)
  a + b
end
"#,
    );
}

#[test]
fn t205_tuple_destructure_three_elements() {
    // Tuple destructuring with three elements
    assert_ok(
        "t205_tuple_destructure_three_elements",
        r#"
cell main() -> Int
  let (a, b, c) = (1, 2, 3)
  a + b + c
end
"#,
    );
}

#[test]
fn t205_record_destructure_basic() {
    // Record destructuring: let Point(x:, y:) = point
    assert_ok(
        "t205_record_destructure_basic",
        r#"
record Point
  x: Int
  y: Int
end

cell main() -> Int
  let p = Point(x: 10, y: 20)
  let Point(x:, y:) = p
  x + y
end
"#,
    );
}

#[test]
fn t205_typed_tuple_destructure() {
    // Typed tuple destructuring: let (n: Int, s: String) = expr
    assert_ok(
        "t205_typed_tuple_destructure",
        r#"
cell main() -> Int
  let (n: Int, s: String) = (42, "hello")
  n
end
"#,
    );
}

#[test]
fn t205_list_destructure() {
    // List destructuring: let [a, b] = [1, 2]
    assert_ok(
        "t205_list_destructure",
        r#"
cell main() -> Int
  let [a, b] = [1, 2]
  a + b
end
"#,
    );
}

#[test]
fn t205_nested_tuple_destructure() {
    // Nested tuple destructuring
    assert_ok(
        "t205_nested_tuple_destructure",
        r#"
cell main() -> Int
  let (a, (b, c)) = (1, (2, 3))
  a + b + c
end
"#,
    );
}

#[test]
fn t205_record_destructure_with_pattern() {
    // Record destructuring with a sub-pattern on a field
    assert_ok(
        "t205_record_destructure_with_pattern",
        r#"
record Pair
  first: Int
  second: Int
end

cell main() -> Int
  let p = Pair(first: 5, second: 10)
  let Pair(first: a, second: b) = p
  a + b
end
"#,
    );
}

// ============================================================================
// T209: Result/optional sugar completion
// ============================================================================

#[test]
fn t209_null_assert_optional() {
    // expr! on T? should unwrap to T
    assert_ok(
        "t209_null_assert_optional",
        r#"
cell main() -> Int
  let x: Int? = 42
  x!
end
"#,
    );
}

#[test]
fn t209_null_coalesce_optional() {
    // expr ?? default on T? should return T
    assert_ok(
        "t209_null_coalesce_optional",
        r#"
cell main() -> Int
  let x: Int? = null
  x ?? 0
end
"#,
    );
}

#[test]
fn t209_force_unwrap_result() {
    // expr! on result[T, E] should return T
    assert_ok(
        "t209_force_unwrap_result",
        r#"
cell main() -> Int
  let r: result[Int, String] = ok(42)
  r!
end
"#,
    );
}

#[test]
fn t209_coalesce_result() {
    // expr ?? default on result[T, E] should return T
    assert_ok(
        "t209_coalesce_result",
        r#"
cell main() -> Int
  let r: result[Int, String] = err("fail")
  r ?? 0
end
"#,
    );
}

#[test]
fn t209_chained_null_coalesce() {
    // Chained ?? operators
    assert_ok(
        "t209_chained_null_coalesce",
        r#"
cell main() -> Int
  let a: Int? = null
  let b: Int? = null
  let c: Int? = 42
  a ?? b ?? c ?? 0
end
"#,
    );
}

#[test]
fn t209_force_unwrap_in_expression() {
    // Using ! in a larger expression
    assert_ok(
        "t209_force_unwrap_in_expression",
        r#"
cell main() -> Int
  let x: Int? = 10
  let y: Int? = 20
  x! + y!
end
"#,
    );
}

#[test]
fn t209_null_coalesce_type_inference() {
    // ?? should produce the non-null type
    assert_ok(
        "t209_null_coalesce_type_inference",
        r#"
cell add(a: Int, b: Int) -> Int
  a + b
end

cell main() -> Int
  let x: Int? = 5
  add(x ?? 0, 10)
end
"#,
    );
}
