//! Wave-20 tests: T195 — Bytes literals and bytes builtins.

use lumen_compiler::compile;
use lumen_rt::values::{StringRef, Value};
use lumen_rt::vm::VM;

/// Helper: wrap raw Lumen code in markdown, compile, run `main`, return the result.
fn run_main(source: &str) -> Value {
    let md = format!("# wave20-bytes-test\n\n```lumen\n{}\n```\n", source.trim());
    let module = compile(&md).expect("source should compile");
    let mut vm = VM::new();
    vm.load(module);
    vm.execute("main", vec![]).expect("main should execute")
}

// ─── bytes_from_ascii: verify via roundtrip to string ───

#[test]
fn bytes_from_ascii_basic_roundtrip() {
    let result = run_main(
        r#"
cell main() -> String
  let b = bytes_from_ascii("hello")
  return bytes_to_ascii(b)
end
"#,
    );
    assert_eq!(result, Value::String(StringRef::Owned("hello".to_string())));
}

#[test]
fn bytes_from_ascii_empty_roundtrip() {
    let result = run_main(
        r#"
cell main() -> String
  let b = bytes_from_ascii("")
  return bytes_to_ascii(b)
end
"#,
    );
    assert_eq!(result, Value::String(StringRef::Owned("".to_string())));
}

#[test]
fn bytes_from_ascii_special_chars_roundtrip() {
    let result = run_main(
        r#"
cell main() -> String
  let b = bytes_from_ascii("abc123")
  return bytes_to_ascii(b)
end
"#,
    );
    assert_eq!(
        result,
        Value::String(StringRef::Owned("abc123".to_string()))
    );
}

#[test]
fn bytes_from_ascii_returns_bytes_type() {
    let result = run_main(
        r#"
cell main() -> String
  let b = bytes_from_ascii("hello")
  return type_of(b)
end
"#,
    );
    assert_eq!(result, Value::String(StringRef::Owned("Bytes".to_string())));
}

// ─── bytes_to_ascii ───

#[test]
fn bytes_to_ascii_roundtrip() {
    let result = run_main(
        r#"
cell main() -> String
  let b = bytes_from_ascii("hello world")
  return bytes_to_ascii(b)
end
"#,
    );
    assert_eq!(
        result,
        Value::String(StringRef::Owned("hello world".to_string()))
    );
}

#[test]
fn bytes_to_ascii_empty() {
    let result = run_main(
        r#"
cell main() -> String
  let b = bytes_from_ascii("")
  return bytes_to_ascii(b)
end
"#,
    );
    assert_eq!(result, Value::String(StringRef::Owned("".to_string())));
}

#[test]
fn bytes_to_ascii_non_bytes_returns_null() {
    let result = run_main(
        r#"
cell main() -> Bool
  let s = bytes_to_ascii(42)
  return s == null
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

// ─── bytes_len ───

#[test]
fn bytes_len_basic() {
    let result = run_main(
        r#"
cell main() -> Int
  let b = bytes_from_ascii("hello")
  return bytes_len(b)
end
"#,
    );
    assert_eq!(result, Value::Int(5));
}

#[test]
fn bytes_len_empty() {
    let result = run_main(
        r#"
cell main() -> Int
  let b = bytes_from_ascii("")
  return bytes_len(b)
end
"#,
    );
    assert_eq!(result, Value::Int(0));
}

#[test]
fn bytes_len_longer_string() {
    let result = run_main(
        r#"
cell main() -> Int
  let b = bytes_from_ascii("hello world!")
  return bytes_len(b)
end
"#,
    );
    assert_eq!(result, Value::Int(12));
}

// ─── bytes_slice ───

#[test]
fn bytes_slice_basic() {
    let result = run_main(
        r#"
cell main() -> String
  let b = bytes_from_ascii("hello world")
  let s = bytes_slice(b, 0, 5)
  return bytes_to_ascii(s)
end
"#,
    );
    assert_eq!(result, Value::String(StringRef::Owned("hello".to_string())));
}

#[test]
fn bytes_slice_middle() {
    let result = run_main(
        r#"
cell main() -> String
  let b = bytes_from_ascii("hello world")
  let s = bytes_slice(b, 6, 11)
  return bytes_to_ascii(s)
end
"#,
    );
    assert_eq!(result, Value::String(StringRef::Owned("world".to_string())));
}

#[test]
fn bytes_slice_empty_range() {
    let result = run_main(
        r#"
cell main() -> Int
  let b = bytes_from_ascii("hello")
  let s = bytes_slice(b, 2, 2)
  return bytes_len(s)
end
"#,
    );
    assert_eq!(result, Value::Int(0));
}

#[test]
fn bytes_slice_to_end() {
    let result = run_main(
        r#"
cell main() -> String
  let b = bytes_from_ascii("abcdef")
  let s = bytes_slice(b, 3, 0)
  return bytes_to_ascii(s)
end
"#,
    );
    // end=0 means end of bytes (see implementation: end<=0 means b.len())
    assert_eq!(result, Value::String(StringRef::Owned("def".to_string())));
}

// ─── bytes_concat ───

#[test]
fn bytes_concat_basic() {
    let result = run_main(
        r#"
cell main() -> String
  let a = bytes_from_ascii("hello")
  let b = bytes_from_ascii(" world")
  let c = bytes_concat(a, b)
  return bytes_to_ascii(c)
end
"#,
    );
    assert_eq!(
        result,
        Value::String(StringRef::Owned("hello world".to_string()))
    );
}

#[test]
fn bytes_concat_empty_left() {
    let result = run_main(
        r#"
cell main() -> String
  let a = bytes_from_ascii("")
  let b = bytes_from_ascii("world")
  let c = bytes_concat(a, b)
  return bytes_to_ascii(c)
end
"#,
    );
    assert_eq!(result, Value::String(StringRef::Owned("world".to_string())));
}

#[test]
fn bytes_concat_empty_right() {
    let result = run_main(
        r#"
cell main() -> String
  let a = bytes_from_ascii("hello")
  let b = bytes_from_ascii("")
  let c = bytes_concat(a, b)
  return bytes_to_ascii(c)
end
"#,
    );
    assert_eq!(result, Value::String(StringRef::Owned("hello".to_string())));
}

#[test]
fn bytes_concat_non_bytes_returns_null() {
    let result = run_main(
        r#"
cell main() -> Bool
  let r = bytes_concat(42, "not bytes")
  return r == null
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

// ─── len() builtin also works for Bytes ───

#[test]
fn len_on_bytes() {
    let result = run_main(
        r#"
cell main() -> Int
  let b = bytes_from_ascii("testing")
  return len(b)
end
"#,
    );
    assert_eq!(result, Value::Int(7));
}

// ─── Integration: full pipeline ───

#[test]
fn bytes_full_pipeline() {
    let result = run_main(
        r#"
cell main() -> String
  let original = "Hello, Lumen!"
  let b = bytes_from_ascii(original)
  let first_five = bytes_slice(b, 0, 5)
  let exclaim = bytes_from_ascii("!")
  let result = bytes_concat(first_five, exclaim)
  return bytes_to_ascii(result)
end
"#,
    );
    assert_eq!(
        result,
        Value::String(StringRef::Owned("Hello!".to_string()))
    );
}

#[test]
fn bytes_concat_then_len() {
    let result = run_main(
        r#"
cell main() -> Int
  let a = bytes_from_ascii("abc")
  let b = bytes_from_ascii("defgh")
  let c = bytes_concat(a, b)
  return bytes_len(c)
end
"#,
    );
    assert_eq!(result, Value::Int(8));
}
