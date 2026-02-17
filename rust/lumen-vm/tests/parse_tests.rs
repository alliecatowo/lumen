//! Wave-20 tests: T196 — parse_int / parse_float builtins.

use lumen_compiler::compile;
use lumen_vm::values::Value;
use lumen_vm::vm::VM;

/// Helper: wrap raw Lumen code in markdown, compile, run `main`, return the result.
fn run_main(source: &str) -> Value {
    let md = format!("# wave20-parse-test\n\n```lumen\n{}\n```\n", source.trim());
    let module = compile(&md).expect("source should compile");
    let mut vm = VM::new();
    vm.load(module);
    vm.execute("main", vec![]).expect("main should execute")
}

// ─── parse_int ───

#[test]
fn parse_int_basic() {
    let result = run_main(
        r#"
cell main() -> Int
  return parse_int("42")
end
"#,
    );
    assert_eq!(result, Value::Int(42));
}

#[test]
fn parse_int_negative() {
    let result = run_main(
        r#"
cell main() -> Int
  return parse_int("-17")
end
"#,
    );
    assert_eq!(result, Value::Int(-17));
}

#[test]
fn parse_int_zero() {
    let result = run_main(
        r#"
cell main() -> Int
  return parse_int("0")
end
"#,
    );
    assert_eq!(result, Value::Int(0));
}

#[test]
fn parse_int_with_whitespace() {
    let result = run_main(
        r#"
cell main() -> Int
  return parse_int("  123  ")
end
"#,
    );
    assert_eq!(result, Value::Int(123));
}

#[test]
fn parse_int_invalid_returns_null() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_int("not_a_number")
end
"#,
    );
    assert_eq!(result, Value::Null);
}

#[test]
fn parse_int_float_string_returns_null() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_int("3.14")
end
"#,
    );
    assert_eq!(result, Value::Null);
}

#[test]
fn parse_int_empty_string_returns_null() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_int("")
end
"#,
    );
    assert_eq!(result, Value::Null);
}

#[test]
fn parse_int_from_int_passthrough() {
    let result = run_main(
        r#"
cell main() -> Int
  return parse_int(42)
end
"#,
    );
    assert_eq!(result, Value::Int(42));
}

#[test]
fn parse_int_from_float_truncates() {
    let result = run_main(
        r#"
cell main() -> Int
  return parse_int(3.9)
end
"#,
    );
    assert_eq!(result, Value::Int(3));
}

#[test]
fn parse_int_large_number() {
    let result = run_main(
        r#"
cell main() -> Int
  return parse_int("9223372036854775807")
end
"#,
    );
    assert_eq!(result, Value::Int(i64::MAX));
}

// ─── parse_float ───

#[test]
fn parse_float_basic() {
    let result = run_main(
        r#"
cell main() -> Float
  return parse_float("3.14")
end
"#,
    );
    assert_eq!(result, Value::Float(3.14));
}

#[test]
fn parse_float_integer_string() {
    let result = run_main(
        r#"
cell main() -> Float
  return parse_float("42")
end
"#,
    );
    assert_eq!(result, Value::Float(42.0));
}

#[test]
fn parse_float_negative() {
    let result = run_main(
        r#"
cell main() -> Float
  return parse_float("-2.5")
end
"#,
    );
    assert_eq!(result, Value::Float(-2.5));
}

#[test]
fn parse_float_with_whitespace() {
    let result = run_main(
        r#"
cell main() -> Float
  return parse_float("  1.5  ")
end
"#,
    );
    assert_eq!(result, Value::Float(1.5));
}

#[test]
fn parse_float_scientific_notation() {
    let result = run_main(
        r#"
cell main() -> Float
  return parse_float("1.5e2")
end
"#,
    );
    assert_eq!(result, Value::Float(150.0));
}

#[test]
fn parse_float_invalid_returns_null() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_float("not_a_number")
end
"#,
    );
    assert_eq!(result, Value::Null);
}

#[test]
fn parse_float_empty_string_returns_null() {
    let result = run_main(
        r#"
cell main() -> Any
  return parse_float("")
end
"#,
    );
    assert_eq!(result, Value::Null);
}

#[test]
fn parse_float_from_float_passthrough() {
    let result = run_main(
        r#"
cell main() -> Float
  return parse_float(2.718)
end
"#,
    );
    assert_eq!(result, Value::Float(2.718));
}

#[test]
fn parse_float_from_int_converts() {
    let result = run_main(
        r#"
cell main() -> Float
  return parse_float(7)
end
"#,
    );
    assert_eq!(result, Value::Float(7.0));
}

#[test]
fn parse_float_zero() {
    let result = run_main(
        r#"
cell main() -> Float
  return parse_float("0.0")
end
"#,
    );
    assert_eq!(result, Value::Float(0.0));
}

// ─── Integration: parse and compute ───

#[test]
fn parse_int_then_compute() {
    let result = run_main(
        r#"
cell main() -> Int
  let a = parse_int("10")
  let b = parse_int("20")
  return a + b
end
"#,
    );
    assert_eq!(result, Value::Int(30));
}

#[test]
fn parse_float_then_compute() {
    let result = run_main(
        r#"
cell main() -> Float
  let a = parse_float("1.5")
  let b = parse_float("2.5")
  return a + b
end
"#,
    );
    assert_eq!(result, Value::Float(4.0));
}

// ─── Null safety: parse returns null on bad input ───

#[test]
fn parse_int_null_check() {
    let result = run_main(
        r#"
cell main() -> Bool
  let val = parse_int("abc")
  return val == null
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn parse_float_null_check() {
    let result = run_main(
        r#"
cell main() -> Bool
  let val = parse_float("xyz")
  return val == null
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}
