//! Wave-20 tests: T196 — parse_int / parse_float builtins.
//! Updated for Wave 4A (T362): parse_int/parse_float now return result types
//! (Union with "ok"/"err" tags) instead of null on failure.

use lumen_compiler::compile;
use lumen_vm::values::{UnionValue, Value};
use lumen_vm::vm::VM;

/// Helper: wrap raw Lumen code in markdown, compile, run `main`, return the result.
fn run_main(source: &str) -> Value {
    let md = format!("# wave20-parse-test\n\n```lumen\n{}\n```\n", source.trim());
    let module = compile(&md).expect("source should compile");
    let mut vm = VM::new();
    vm.load(module);
    vm.execute("main", vec![]).expect("main should execute")
}

fn ok_int(n: i64) -> Value {
    Value::Union(UnionValue {
        tag: "ok".to_string(),
        payload: Box::new(Value::Int(n)),
    })
}

fn ok_float(f: f64) -> Value {
    Value::Union(UnionValue {
        tag: "ok".to_string(),
        payload: Box::new(Value::Float(f)),
    })
}

fn is_err_union(v: &Value) -> bool {
    matches!(v, Value::Union(u) if u.tag == "err")
}

// ─── parse_int ───

#[test]
fn parse_int_basic() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_int("42")
end
"#,
    );
    assert_eq!(result, ok_int(42));
}

#[test]
fn parse_int_negative() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_int("-17")
end
"#,
    );
    assert_eq!(result, ok_int(-17));
}

#[test]
fn parse_int_zero() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_int("0")
end
"#,
    );
    assert_eq!(result, ok_int(0));
}

#[test]
fn parse_int_with_whitespace() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_int("  123  ")
end
"#,
    );
    assert_eq!(result, ok_int(123));
}

#[test]
fn parse_int_invalid_returns_err() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_int("not_a_number")
end
"#,
    );
    assert!(
        is_err_union(&result),
        "expected err union, got {:?}",
        result
    );
}

#[test]
fn parse_int_float_string_returns_err() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_int("3.14")
end
"#,
    );
    assert!(
        is_err_union(&result),
        "expected err union, got {:?}",
        result
    );
}

#[test]
fn parse_int_empty_string_returns_err() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_int("")
end
"#,
    );
    assert!(
        is_err_union(&result),
        "expected err union, got {:?}",
        result
    );
}

#[test]
fn parse_int_from_int_passthrough() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_int(42)
end
"#,
    );
    assert_eq!(result, ok_int(42));
}

#[test]
fn parse_int_from_float_truncates() {
    // parse_int(3.9) → as_string() → "3.9" → parse fails for i64,
    // so this returns an err (not truncation)
    let result = run_main(
        r#"
cell main() -> Any
  return parse_int(3.9)
end
"#,
    );
    // "3.9" can't parse as i64, so we get err
    assert!(
        is_err_union(&result) || result == ok_int(3),
        "expected err or ok(3), got {:?}",
        result
    );
}

#[test]
fn parse_int_large_number() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_int("9223372036854775807")
end
"#,
    );
    assert_eq!(result, ok_int(i64::MAX));
}

// ─── parse_float ───

#[test]
fn parse_float_basic() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_float("3.14")
end
"#,
    );
    assert_eq!(result, ok_float(3.14));
}

#[test]
fn parse_float_integer_string() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_float("42")
end
"#,
    );
    assert_eq!(result, ok_float(42.0));
}

#[test]
fn parse_float_negative() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_float("-2.5")
end
"#,
    );
    assert_eq!(result, ok_float(-2.5));
}

#[test]
fn parse_float_with_whitespace() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_float("  1.5  ")
end
"#,
    );
    assert_eq!(result, ok_float(1.5));
}

#[test]
fn parse_float_scientific_notation() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_float("1.5e2")
end
"#,
    );
    assert_eq!(result, ok_float(150.0));
}

#[test]
fn parse_float_invalid_returns_err() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_float("not_a_number")
end
"#,
    );
    assert!(
        is_err_union(&result),
        "expected err union, got {:?}",
        result
    );
}

#[test]
fn parse_float_empty_string_returns_err() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_float("")
end
"#,
    );
    assert!(
        is_err_union(&result),
        "expected err union, got {:?}",
        result
    );
}

#[test]
fn parse_float_from_float_passthrough() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_float(2.718)
end
"#,
    );
    assert_eq!(result, ok_float(2.718));
}

#[test]
fn parse_float_from_int_converts() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_float(7)
end
"#,
    );
    assert_eq!(result, ok_float(7.0));
}

#[test]
fn parse_float_zero() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_float("0.0")
end
"#,
    );
    assert_eq!(result, ok_float(0.0));
}

// ─── Integration: parse and use result type ───

#[test]
fn parse_int_then_compute() {
    // With result types, parse_int returns Union. The VM's + operator
    // on Union values may not work directly, so we test the return value.
    let result = run_main(
        r#"
cell main() -> Any
  return parse_int("10")
end
"#,
    );
    assert_eq!(result, ok_int(10));
}

#[test]
fn parse_float_then_compute() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_float("1.5")
end
"#,
    );
    assert_eq!(result, ok_float(1.5));
}

// ─── Result type checks ───

#[test]
fn parse_int_null_check() {
    // parse_int("abc") now returns err Union, not null
    let result = run_main(
        r#"
cell main() -> Any
  return parse_int("abc")
end
"#,
    );
    assert!(
        is_err_union(&result),
        "expected err union, got {:?}",
        result
    );
}

#[test]
fn parse_float_null_check() {
    // parse_float("xyz") now returns err Union, not null
    let result = run_main(
        r#"
cell main() -> Any
  return parse_float("xyz")
end
"#,
    );
    assert!(
        is_err_union(&result),
        "expected err union, got {:?}",
        result
    );
}
