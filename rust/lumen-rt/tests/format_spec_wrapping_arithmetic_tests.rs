//! Format specifier, wrapping arithmetic, and clippy compliance tests.

use lumen_compiler::compile;
use lumen_vm::values::{StringRef, Value};
use lumen_vm::vm::VM;

/// Helper: wrap raw Lumen code in markdown, compile, run `main`, return the result.
fn run_main(source: &str) -> Value {
    let md = format!("# wave19-test\n\n```lumen\n{}\n```\n", source.trim());
    let module = compile(&md).expect("source should compile");
    let mut vm = VM::new();
    vm.load(module);
    vm.execute("main", vec![]).expect("main should execute")
}

/// Helper: run and expect a runtime error.
fn run_main_err(source: &str) -> String {
    let md = format!("# wave19-test\n\n```lumen\n{}\n```\n", source.trim());
    let module = compile(&md).expect("source should compile");
    let mut vm = VM::new();
    vm.load(module);
    match vm.execute("main", vec![]) {
        Err(e) => format!("{}", e),
        Ok(v) => panic!("expected error, got {:?}", v),
    }
}

// ─── T216: __format_spec tests ───

#[test]
fn format_spec_float_precision_2f() {
    let result = run_main(
        r#"
cell main() -> String
  return __format_spec(3.14159, ".2f")
end
"#,
    );
    assert_eq!(result, Value::String(StringRef::Owned("3.14".to_string())));
}

#[test]
fn format_spec_float_precision_0f() {
    let result = run_main(
        r#"
cell main() -> String
  return __format_spec(3.14159, ".0f")
end
"#,
    );
    assert_eq!(result, Value::String(StringRef::Owned("3".to_string())));
}

#[test]
fn format_spec_hex() {
    let result = run_main(
        r##"
cell main() -> String
  return __format_spec(255, "#x")
end
"##,
    );
    assert_eq!(result, Value::String(StringRef::Owned("0xff".to_string())));
}

#[test]
fn format_spec_octal() {
    let result = run_main(
        r##"
cell main() -> String
  return __format_spec(255, "#o")
end
"##,
    );
    assert_eq!(result, Value::String(StringRef::Owned("0o377".to_string())));
}

#[test]
fn format_spec_binary() {
    let result = run_main(
        r##"
cell main() -> String
  return __format_spec(10, "#b")
end
"##,
    );
    assert_eq!(
        result,
        Value::String(StringRef::Owned("0b1010".to_string()))
    );
}

#[test]
fn format_spec_right_align() {
    let result = run_main(
        r#"
cell main() -> String
  return __format_spec(42, ">10")
end
"#,
    );
    assert_eq!(
        result,
        Value::String(StringRef::Owned("        42".to_string()))
    );
}

#[test]
fn format_spec_left_align() {
    let result = run_main(
        r#"
cell main() -> String
  return __format_spec(42, "<10")
end
"#,
    );
    assert_eq!(
        result,
        Value::String(StringRef::Owned("42        ".to_string()))
    );
}

#[test]
fn format_spec_center_align() {
    let result = run_main(
        r#"
cell main() -> String
  return __format_spec(42, "^10")
end
"#,
    );
    assert_eq!(
        result,
        Value::String(StringRef::Owned("    42    ".to_string()))
    );
}

#[test]
fn format_spec_zero_pad() {
    let result = run_main(
        r#"
cell main() -> String
  return __format_spec(42, "08")
end
"#,
    );
    assert_eq!(
        result,
        Value::String(StringRef::Owned("00000042".to_string()))
    );
}

#[test]
fn format_spec_sign_plus() {
    let result = run_main(
        r#"
cell main() -> String
  return __format_spec(42, "+")
end
"#,
    );
    assert_eq!(result, Value::String(StringRef::Owned("+42".to_string())));
}

#[test]
fn format_spec_sign_plus_negative() {
    let result = run_main(
        r#"
cell main() -> String
  return __format_spec(0 - 42, "+")
end
"#,
    );
    assert_eq!(result, Value::String(StringRef::Owned("-42".to_string())));
}

// ─── T123: Wrapping arithmetic tests ───

#[test]
fn wrapping_add_basic() {
    let result = run_main(
        r#"
cell main() -> Int
  return wrapping_add(10, 20)
end
"#,
    );
    assert_eq!(result, Value::Int(30));
}

#[test]
fn wrapping_add_overflow() {
    let result = run_main(
        r#"
cell main() -> Int
  return wrapping_add(9223372036854775807, 1)
end
"#,
    );
    // i64::MAX + 1 wraps to i64::MIN
    assert_eq!(result, Value::Int(i64::MIN));
}

#[test]
fn wrapping_sub_basic() {
    let result = run_main(
        r#"
cell main() -> Int
  return wrapping_sub(30, 10)
end
"#,
    );
    assert_eq!(result, Value::Int(20));
}

#[test]
fn wrapping_sub_underflow() {
    let result = run_main(
        r#"
cell main() -> Int
  return wrapping_sub(-9223372036854775808, 1)
end
"#,
    );
    // i64::MIN - 1 wraps to i64::MAX
    assert_eq!(result, Value::Int(i64::MAX));
}

#[test]
fn wrapping_mul_basic() {
    let result = run_main(
        r#"
cell main() -> Int
  return wrapping_mul(6, 7)
end
"#,
    );
    assert_eq!(result, Value::Int(42));
}

#[test]
fn wrapping_mul_overflow() {
    let result = run_main(
        r#"
cell main() -> Int
  return wrapping_mul(9223372036854775807, 2)
end
"#,
    );
    // i64::MAX * 2 wraps
    assert_eq!(result, Value::Int(i64::MAX.wrapping_mul(2)));
}

// ─── T123: Checked arithmetic overflow detection ───

#[test]
fn checked_add_overflow_detected() {
    let err = run_main_err(
        r#"
cell main() -> Int
  let x: Int = 9223372036854775807
  return x + 1
end
"#,
    );
    assert!(
        err.contains("overflow") || err.contains("Overflow") || err.contains("arithmetic"),
        "expected overflow error, got: {}",
        err,
    );
}

#[test]
fn checked_mul_overflow_detected() {
    let err = run_main_err(
        r#"
cell main() -> Int
  let x: Int = 9223372036854775807
  return x * 2
end
"#,
    );
    assert!(
        err.contains("overflow") || err.contains("Overflow") || err.contains("arithmetic"),
        "expected overflow error, got: {}",
        err,
    );
}
