//! Spec suite: exhaustiveness checking, trailing lambda, and result sugar tests.
//!
//! Tests for:
//! - Exhaustiveness checking for integer refinement ranges
//! - Trailing lambda / DSL blocks (`do...end` after calls)
//! - Result/optional syntactic sugar (`expr!` and `??` on results)

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
// T049: Exhaustiveness checking for integer refinement ranges
// ============================================================================

#[test]
fn t049_int_match_exhaustive_with_wildcard() {
    // Wildcard makes any integer match exhaustive — should compile OK
    assert_ok(
        "t049_int_match_exhaustive_with_wildcard",
        r#"
cell check(x: Int) -> String
  match x
    1 -> return "one"
    2 -> return "two"
    _ -> return "other"
  end
  return ""
end
"#,
    );
}

#[test]
fn t049_int_match_exhaustive_small_range() {
    // All values 1, 2, 3 are covered — should compile OK
    assert_ok(
        "t049_int_match_exhaustive_small_range",
        r#"
cell check(x: Int) -> String
  match x
    1 -> return "one"
    2 -> return "two"
    3 -> return "three"
  end
  return ""
end
"#,
    );
}

#[test]
fn t049_int_match_incomplete_small_range() {
    // Missing value 2 in the range 1..3
    assert_err(
        "t049_int_match_incomplete_small_range",
        r#"
cell check(x: Int) -> String
  match x
    1 -> return "one"
    3 -> return "three"
  end
  return ""
end
"#,
        "IncompleteMatch",
    );
}

#[test]
fn t049_int_match_range_pattern_exhaustive() {
    // Range pattern covers all values
    assert_ok(
        "t049_int_match_range_pattern_exhaustive",
        r#"
cell check(x: Int) -> String
  match x
    1..=5 -> return "small"
  end
  return ""
end
"#,
    );
}

#[test]
fn t049_int_match_range_with_gap() {
    // Ranges with a gap: 1..=3 and 5..=7 miss 4
    assert_err(
        "t049_int_match_range_with_gap",
        r#"
cell check(x: Int) -> String
  match x
    1..=3 -> return "low"
    5..=7 -> return "high"
  end
  return ""
end
"#,
        "IncompleteMatch",
    );
}

#[test]
fn t049_int_match_mixed_literal_and_range() {
    // Literal 4 fills the gap between ranges
    assert_ok(
        "t049_int_match_mixed_literal_and_range",
        r#"
cell check(x: Int) -> String
  match x
    1..=3 -> return "low"
    4 -> return "mid"
    5..=7 -> return "high"
  end
  return ""
end
"#,
    );
}

#[test]
fn t049_int_match_large_range_requires_wildcard() {
    // Range > 256 without wildcard should fail
    assert_err(
        "t049_int_match_large_range_requires_wildcard",
        r#"
cell check(x: Int) -> String
  match x
    0..=300 -> return "big"
  end
  return ""
end
"#,
        "wildcard required",
    );
}

#[test]
fn t049_int_match_ident_catchall() {
    // Identifier binding acts as catchall
    assert_ok(
        "t049_int_match_ident_catchall",
        r#"
cell check(x: Int) -> String
  match x
    1 -> return "one"
    n -> return "other"
  end
  return ""
end
"#,
    );
}

// ============================================================================
// T120: Trailing lambda / DSL blocks
// ============================================================================

#[test]
fn t120_trailing_do_block_no_params() {
    // Simple trailing do block with no parameters
    assert_ok(
        "t120_trailing_do_block_no_params",
        r#"
cell apply(f: fn() -> Int) -> Int
  return f()
end

cell main() -> Int
  return apply() do
    return 42
  end
end
"#,
    );
}

#[test]
fn t120_trailing_do_block_with_params() {
    // Trailing do block with pipe-delimited parameters
    assert_ok(
        "t120_trailing_do_block_with_params",
        r#"
cell apply(x: Int, f: fn(Int) -> Int) -> Int
  return f(x)
end

cell main() -> Int
  return apply(10) do |n|
    return n + 1
  end
end
"#,
    );
}

#[test]
fn t120_trailing_do_block_with_typed_params() {
    // Trailing do block with typed parameters
    assert_ok(
        "t120_trailing_do_block_with_typed_params",
        r#"
cell transform(x: Int, f: fn(Int) -> String) -> String
  return f(x)
end

cell main() -> String
  return transform(42) do |n: Int|
    return "value"
  end
end
"#,
    );
}

#[test]
fn t120_trailing_do_block_with_existing_args() {
    // Trailing do block appended to existing positional args
    assert_ok(
        "t120_trailing_do_block_with_existing_args",
        r#"
cell with_both(a: Int, b: Int, f: fn() -> Int) -> Int
  return a + b + f()
end

cell main() -> Int
  return with_both(1, 2) do
    return 10
  end
end
"#,
    );
}

#[test]
fn t120_trailing_do_block_multiline_body() {
    // Trailing do block with multiple statements
    assert_ok(
        "t120_trailing_do_block_multiline_body",
        r#"
cell run(f: fn() -> Int) -> Int
  return f()
end

cell main() -> Int
  return run() do
    let x = 10
    let y = 20
    return x + y
  end
end
"#,
    );
}

// ============================================================================
// T209: Result/optional syntactic sugar
// ============================================================================

#[test]
fn t209_null_assert_still_works() {
    // Original null assert behavior should still work
    assert_ok(
        "t209_null_assert_still_works",
        r#"
cell get_value() -> Int?
  return 42
end

cell main() -> Int
  let v = get_value()
  return v!
end
"#,
    );
}

#[test]
fn t209_result_unwrap_with_bang() {
    // expr! on result[T, E] should unwrap to T
    assert_ok(
        "t209_result_unwrap_with_bang",
        r#"
cell try_parse(s: String) -> result[Int, String]
  return ok(42)
end

cell main() -> Int
  let r = try_parse("hello")
  return r!
end
"#,
    );
}

#[test]
fn t209_null_coalesce_still_works() {
    // Original ?? behavior for nullable types
    assert_ok(
        "t209_null_coalesce_still_works",
        r#"
cell maybe_val() -> Int?
  return null
end

cell main() -> Int
  let v = maybe_val()
  return v ?? 0
end
"#,
    );
}

#[test]
fn t209_result_coalesce() {
    // ?? on result[T, E] should unwrap ok or use default on err
    assert_ok(
        "t209_result_coalesce",
        r#"
cell try_parse(s: String) -> result[Int, String]
  return ok(42)
end

cell main() -> Int
  let r = try_parse("hello")
  return r ?? 0
end
"#,
    );
}

#[test]
fn t209_result_unwrap_type_inference() {
    // Type inference: expr! on result[Int, String] should produce Int
    assert_ok(
        "t209_result_unwrap_type_inference",
        r#"
cell fallible() -> result[String, Int]
  return ok("hello")
end

cell use_result() -> String
  let r = fallible()
  let s: String = r!
  return s
end
"#,
    );
}

#[test]
fn t209_result_coalesce_type_inference() {
    // Type inference: result[Int, E] ?? default should produce Int
    assert_ok(
        "t209_result_coalesce_type_inference",
        r#"
cell fallible() -> result[Int, String]
  return ok(42)
end

cell use_result() -> Int
  let r = fallible()
  let v: Int = r ?? 0
  return v
end
"#,
    );
}
