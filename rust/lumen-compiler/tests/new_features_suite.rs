//! Tests for new syntax features: spaceship operator (T113), membership `in` (T115),
//! error propagation `?` (T121), and `@must_use` attribute (T164).

use lumen_compiler::compile;

fn markdown_from_code(source: &str) -> String {
    format!("# new-features-test\n\n```lumen\n{}\n```\n", source.trim())
}

fn assert_compiles(source: &str) {
    let md = markdown_from_code(source);
    if let Err(err) = compile(&md) {
        panic!(
            "expected source to compile, but got error:\n{}\nsource:\n{}",
            err, source
        );
    }
}

fn assert_compile_error(source: &str, expected_fragment: &str) {
    let md = markdown_from_code(source);
    match compile(&md) {
        Ok(_) => panic!(
            "expected compile error with '{}', but source compiled successfully:\n{}",
            expected_fragment, source
        ),
        Err(err) => {
            let msg = err.to_string().to_lowercase();
            let expect = expected_fragment.to_lowercase();
            assert!(
                msg.contains(&expect),
                "expected error containing '{}', got:\n{}",
                expected_fragment,
                err
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// T113: Spaceship operator <=>
// ═══════════════════════════════════════════════════════════════════

#[test]
fn spaceship_int_comparison() {
    assert_compiles(
        r#"
cell main() -> Int
  3 <=> 5
end
"#,
    );
}

#[test]
fn spaceship_returns_int() {
    assert_compiles(
        r#"
cell main() -> Int
  let result: Int = 3 <=> 5
  result
end
"#,
    );
}

#[test]
fn spaceship_with_variables() {
    assert_compiles(
        r#"
cell compare(a: Int, b: Int) -> Int
  a <=> b
end

cell main() -> Int
  compare(10, 20)
end
"#,
    );
}

#[test]
fn spaceship_in_expression() {
    assert_compiles(
        r#"
cell main() -> Int
  let a = 3 <=> 5
  let b = 5 <=> 3
  let c = 5 <=> 5
  a + b + c
end
"#,
    );
}

#[test]
fn spaceship_float_comparison() {
    assert_compiles(
        r#"
cell main() -> Int
  1.5 <=> 2.5
end
"#,
    );
}

#[test]
fn spaceship_string_comparison() {
    assert_compiles(
        r#"
cell main() -> Int
  "abc" <=> "def"
end
"#,
    );
}

// ═══════════════════════════════════════════════════════════════════
// T115: Membership operator `in`
// ═══════════════════════════════════════════════════════════════════

#[test]
fn in_operator_list_membership() {
    assert_compiles(
        r#"
cell main() -> Bool
  let items = [1, 2, 3]
  2 in items
end
"#,
    );
}

#[test]
fn in_operator_with_string_list() {
    assert_compiles(
        r#"
cell main() -> Bool
  let names = ["alice", "bob"]
  "alice" in names
end
"#,
    );
}

#[test]
fn in_operator_returns_bool() {
    assert_compiles(
        r#"
cell main() -> Bool
  let result: Bool = 5 in [1, 2, 3, 4, 5]
  result
end
"#,
    );
}

#[test]
fn in_operator_in_if_condition() {
    assert_compiles(
        r#"
cell main() -> String
  let items = [1, 2, 3]
  if 2 in items
    "found"
  else
    "not found"
  end
end
"#,
    );
}

// ═══════════════════════════════════════════════════════════════════
// T121: Error propagation `?`
// ═══════════════════════════════════════════════════════════════════

#[test]
fn try_operator_on_result() {
    assert_compiles(
        r#"
cell get_value() -> result[Int, String]
  Ok(42)
end

cell main() -> result[Int, String]
  let val = get_value()?
  Ok(val + 1)
end
"#,
    );
}

#[test]
fn try_operator_propagates_error() {
    assert_compiles(
        r#"
cell may_fail(x: Int) -> result[Int, String]
  if x > 0
    Ok(x)
  else
    Err("negative")
  end
end

cell main() -> result[Int, String]
  let a = may_fail(10)?
  let b = may_fail(20)?
  Ok(a + b)
end
"#,
    );
}

#[test]
fn try_operator_extracts_ok_type() {
    // The ? operator should unwrap result[Int, E] to Int
    assert_compiles(
        r#"
cell get_num() -> result[Int, String]
  Ok(42)
end

cell main() -> result[Int, String]
  let n: Int = get_num()?
  Ok(n * 2)
end
"#,
    );
}

// ═══════════════════════════════════════════════════════════════════
// T164: @must_use attribute
// ═══════════════════════════════════════════════════════════════════

#[test]
fn must_use_no_error_when_result_used() {
    assert_compiles(
        r#"
@must_use
cell compute(x: Int) -> Int
  x * 2
end

cell main() -> Int
  let result = compute(5)
  result
end
"#,
    );
}

#[test]
fn must_use_error_when_result_discarded() {
    assert_compile_error(
        r#"
@must_use
cell compute(x: Int) -> Int
  x * 2
end

cell main() -> Int
  compute(5)
  42
end
"#,
        "mustuseignored",
    );
}

#[test]
fn must_use_no_error_on_regular_cell() {
    // Without @must_use, discarding result should be fine
    assert_compiles(
        r#"
cell compute(x: Int) -> Int
  x * 2
end

cell main() -> Int
  compute(5)
  42
end
"#,
    );
}

#[test]
fn must_use_with_return_value() {
    // Using the result as the return value is fine
    assert_compiles(
        r#"
@must_use
cell compute(x: Int) -> Int
  x * 2
end

cell main() -> Int
  compute(5)
end
"#,
    );
}

#[test]
fn must_use_pub_cell() {
    assert_compiles(
        r#"
@must_use
pub cell compute(x: Int) -> Int
  x * 2
end

cell main() -> Int
  let r = compute(5)
  r
end
"#,
    );
}
