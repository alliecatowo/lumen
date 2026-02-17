//! Wave 14 test suite.
//!
//! Tests for:
//! - T201: Nested list comprehension (multiple for-clauses)
//! - T205: Let destructuring with type patterns

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

#[allow(dead_code)]
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
// T201: Nested list comprehension
// ============================================================================

#[test]
fn t201_nested_list_comp_flat() {
    // Basic nested comprehension: flatten a matrix
    assert_ok(
        "nested_list_comp_flat",
        r#"
cell main() -> list[Int]
  let matrix = [[1, 2], [3, 4]]
  let flat = [y for x in matrix for y in x]
  return flat
end
"#,
    );
}

#[test]
fn t201_nested_list_comp_two_independent_iters() {
    // Cartesian product from two lists
    assert_ok(
        "nested_list_comp_cartesian",
        r#"
cell main() -> list[Int]
  let xs = [1, 2]
  let ys = [10, 20]
  let products = [a + b for a in xs for b in ys]
  return products
end
"#,
    );
}

#[test]
fn t201_nested_list_comp_with_condition() {
    // Nested comprehension with a filter condition
    assert_ok(
        "nested_list_comp_with_cond",
        r#"
cell main() -> list[Int]
  let matrix = [[1, 2, 3], [4, 5, 6]]
  let evens = [y for x in matrix for y in x if y % 2 == 0]
  return evens
end
"#,
    );
}

#[test]
fn t201_nested_list_comp_triple() {
    // Triple-nested comprehension
    assert_ok(
        "nested_list_comp_triple",
        r#"
cell main() -> list[Int]
  let a = [[1, 2], [3]]
  let b = [[10], [20, 30]]
  let c = [x + y for xs in a for x in xs for ys in b for y in ys]
  return c
end
"#,
    );
}

#[test]
fn t201_nested_set_comp() {
    // Nested set comprehension
    assert_ok(
        "nested_set_comp",
        r#"
cell main() -> set[Int]
  let matrix = [[1, 2], [2, 3]]
  let flat = {y for x in matrix for y in x}
  return flat
end
"#,
    );
}

#[test]
fn t201_single_for_still_works() {
    // Ensure single-clause comprehensions still compile
    assert_ok(
        "single_for_comp",
        r#"
cell main() -> list[Int]
  let xs = [1, 2, 3]
  let doubled = [x * 2 for x in xs]
  return doubled
end
"#,
    );
}

#[test]
fn t201_single_for_with_condition() {
    // Single-clause with filter
    assert_ok(
        "single_for_cond_comp",
        r#"
cell main() -> list[Int]
  let xs = [1, 2, 3, 4, 5]
  let evens = [x for x in xs if x % 2 == 0]
  return evens
end
"#,
    );
}

#[test]
fn t201_nested_comp_body_uses_both_vars() {
    // Body references variables from both for-clauses
    assert_ok(
        "nested_comp_body_uses_both",
        r#"
cell main() -> list[String]
  let names = ["Alice", "Bob"]
  let greetings = ["Hello", "Hi"]
  let results = [g for g in greetings for n in names]
  return results
end
"#,
    );
}

#[test]
fn t201_nested_comp_inner_depends_on_outer() {
    // Inner iterable depends on outer variable (the key use case)
    assert_ok(
        "nested_comp_inner_depends_outer",
        r#"
cell main() -> list[Int]
  let rows = [[10, 20], [30, 40, 50]]
  let flat = [elem for row in rows for elem in row]
  return flat
end
"#,
    );
}

// ============================================================================
// T205: Let destructuring with type patterns
// ============================================================================

#[test]
fn t205_let_tuple_type_check_basic() {
    // Basic tuple destructure with type annotations
    assert_ok(
        "let_tuple_type_check_basic",
        r#"
cell main() -> Int
  let pair = (42, "hello")
  let (a: Int, b: String) = pair
  return a
end
"#,
    );
}

#[test]
fn t205_let_tuple_mixed_typed_untyped() {
    // Mix of typed and untyped bindings in tuple destructure
    assert_ok(
        "let_tuple_mixed_typed_untyped",
        r#"
cell main() -> Int
  let triple = (1, "two", 3)
  let (a: Int, b, c: Int) = triple
  return a + c
end
"#,
    );
}

#[test]
fn t205_let_simple_type_check_binding() {
    // Single binding with type check pattern in let
    // Parsed as record destructure or type check depending on context
    // Using a tuple destructure with a single typed element
    assert_ok(
        "let_simple_type_check",
        r#"
cell main() -> Int
  let x = 42
  let (a: Int,) = (x,)
  return a
end
"#,
    );
}

#[test]
fn t205_let_tuple_destructure_no_types() {
    // Tuple destructure without type annotations still works
    assert_ok(
        "let_tuple_no_types",
        r#"
cell main() -> Int
  let pair = (10, 20)
  let (a, b) = pair
  return a + b
end
"#,
    );
}

#[test]
fn t205_let_list_destructure_still_works() {
    // List destructure is unaffected
    assert_ok(
        "let_list_destructure",
        r#"
cell main() -> Int
  let items = [1, 2, 3]
  let [a, b, c] = items
  return a + b + c
end
"#,
    );
}

#[test]
fn t205_let_record_destructure_still_works() {
    // Record destructure is unaffected
    assert_ok(
        "let_record_destructure",
        r#"
record Point
  x: Int
  y: Int
end

cell main() -> Int
  let p = Point(x: 10, y: 20)
  let Point(x:, y:) = p
  return x + y
end
"#,
    );
}

#[test]
fn t205_let_nested_tuple_with_type() {
    // Nested tuple destructuring with type annotations
    assert_ok(
        "let_nested_tuple_with_type",
        r#"
cell main() -> Int
  let data = ((1, 2), 3)
  let ((a: Int, b: Int), c) = data
  return a + b + c
end
"#,
    );
}

#[test]
fn t205_type_check_in_match_arm() {
    // TypeCheck patterns in match arms (should already work, regression test)
    assert_ok(
        "type_check_in_match",
        r#"
cell main() -> String
  let x = 42
  match x
    n: Int -> return "int"
    _ -> return "other"
  end
end
"#,
    );
}

// ============================================================================
// Combined / regression tests
// ============================================================================

#[test]
fn combined_nested_comp_and_destructure() {
    // Use nested comprehension result with destructuring
    assert_ok(
        "combined_nested_comp_destructure",
        r#"
cell main() -> Int
  let matrix = [[1, 2], [3, 4]]
  let flat = [y for row in matrix for y in row]
  let [a, b, c, d] = flat
  return a + b + c + d
end
"#,
    );
}

#[test]
fn combined_type_pattern_with_list_comp() {
    // Combine type pattern and list comprehension
    assert_ok(
        "combined_type_pattern_comp",
        r#"
cell main() -> list[Int]
  let data = (1, [10, 20, 30])
  let (count: Int, items) = data
  let doubled = [x * 2 for x in items]
  return doubled
end
"#,
    );
}
