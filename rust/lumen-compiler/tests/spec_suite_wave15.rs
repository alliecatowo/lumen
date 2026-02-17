//! Wave 15 test suite.
//!
//! Tests for:
//! - T119: String interpolation with format specifiers
//! - T163: Complete variadic parameters

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
// T119: String interpolation with format specifiers
// ============================================================================

#[test]
fn t119_basic_format_spec_decimal() {
    // Integer with decimal format spec
    assert_ok(
        "format_spec_decimal",
        r#"
cell main() -> String
  let x = 42
  return "Value: {x:d}"
end
"#,
    );
}

#[test]
fn t119_format_spec_hex() {
    // Integer with hex format spec
    assert_ok(
        "format_spec_hex",
        r#"
cell main() -> String
  let n = 255
  return "Hex: {n:x}"
end
"#,
    );
}

#[test]
fn t119_format_spec_hex_upper() {
    // Integer with uppercase hex format spec
    assert_ok(
        "format_spec_hex_upper",
        r#"
cell main() -> String
  let n = 255
  return "Hex: {n:X}"
end
"#,
    );
}

#[test]
fn t119_format_spec_octal() {
    // Integer with octal format spec
    assert_ok(
        "format_spec_octal",
        r#"
cell main() -> String
  let n = 8
  return "Oct: {n:o}"
end
"#,
    );
}

#[test]
fn t119_format_spec_binary() {
    // Integer with binary format spec
    assert_ok(
        "format_spec_binary",
        r#"
cell main() -> String
  let n = 10
  return "Bin: {n:b}"
end
"#,
    );
}

#[test]
fn t119_format_spec_float_fixed() {
    // Float with fixed-point format spec
    assert_ok(
        "format_spec_float_fixed",
        r#"
cell main() -> String
  let pi = 3.14159
  return "Pi: {pi:.2f}"
end
"#,
    );
}

#[test]
fn t119_format_spec_scientific() {
    // Float with scientific notation
    assert_ok(
        "format_spec_scientific",
        r#"
cell main() -> String
  let big = 123456.789
  return "Sci: {big:.3e}"
end
"#,
    );
}

#[test]
fn t119_format_spec_scientific_upper() {
    // Float with uppercase scientific notation
    assert_ok(
        "format_spec_scientific_upper",
        r#"
cell main() -> String
  let big = 123456.789
  return "Sci: {big:.3E}"
end
"#,
    );
}

#[test]
fn t119_format_spec_width_padding() {
    // Right-aligned with width
    assert_ok(
        "format_spec_width_padding",
        r#"
cell main() -> String
  let s = "hi"
  return "Padded: {s:>10}"
end
"#,
    );
}

#[test]
fn t119_format_spec_left_align() {
    // Left-aligned with width
    assert_ok(
        "format_spec_left_align",
        r#"
cell main() -> String
  let s = "hi"
  return "Left: {s:<10}"
end
"#,
    );
}

#[test]
fn t119_format_spec_center_align() {
    // Center-aligned with width
    assert_ok(
        "format_spec_center_align",
        r#"
cell main() -> String
  let s = "hi"
  return "Center: {s:^10}"
end
"#,
    );
}

#[test]
fn t119_format_spec_fill_char() {
    // Fill character with alignment
    assert_ok(
        "format_spec_fill_char",
        r#"
cell main() -> String
  let x = 42
  return "Filled: {x:*>10d}"
end
"#,
    );
}

#[test]
fn t119_format_spec_alternate_hex() {
    // Alternate form for hex (0x prefix)
    assert_ok(
        "format_spec_alternate_hex",
        r#"
cell main() -> String
  let n = 255
  return "Hex: {n:#x}"
end
"#,
    );
}

#[test]
fn t119_format_spec_zero_pad() {
    // Zero-padded integer
    assert_ok(
        "format_spec_zero_pad",
        r#"
cell main() -> String
  let n = 42
  return "Padded: {n:05d}"
end
"#,
    );
}

#[test]
fn t119_format_spec_sign_plus() {
    // Sign always shown
    assert_ok(
        "format_spec_sign_plus",
        r#"
cell main() -> String
  let n = 42
  return "Signed: {n:+d}"
end
"#,
    );
}

#[test]
fn t119_format_spec_mixed_segments() {
    // Mix of plain interpolation and formatted interpolation
    assert_ok(
        "format_spec_mixed_segments",
        r#"
cell main() -> String
  let name = "Alice"
  let score = 95.5
  return "Hello {name}, your score is {score:.1f}!"
end
"#,
    );
}

#[test]
fn t119_format_spec_multiple_formatted() {
    // Multiple format specs in one string
    assert_ok(
        "format_spec_multiple_formatted",
        r#"
cell main() -> String
  let x = 255
  let y = 3.14
  return "Hex: {x:#x}, Pi: {y:.2f}"
end
"#,
    );
}

#[test]
fn t119_format_spec_string_type() {
    // Explicit string format type
    assert_ok(
        "format_spec_string_type",
        r#"
cell main() -> String
  let s = "hello"
  return "Str: {s:>10s}"
end
"#,
    );
}

#[test]
fn t119_format_spec_int_with_float_spec_error() {
    // Using hex format on a string should error
    assert_err(
        "format_spec_type_mismatch_hex_on_string",
        r#"
cell main() -> String
  let s = "hello"
  return "Bad: {s:x}"
end
"#,
        "mismatch",
    );
}

#[test]
fn t119_format_spec_float_on_bool_error() {
    // Using float format on a bool should error
    assert_err(
        "format_spec_type_mismatch_float_on_bool",
        r#"
cell main() -> String
  let b = true
  return "Bad: {b:.2f}"
end
"#,
        "mismatch",
    );
}

#[test]
fn t119_format_spec_expression_in_interp() {
    // Expression (not just variable) with format spec
    assert_ok(
        "format_spec_expression",
        r#"
cell main() -> String
  let x = 10
  let y = 20
  return "Sum: {x + y:05d}"
end
"#,
    );
}

// ============================================================================
// T163: Complete variadic parameters
// ============================================================================

#[test]
fn t163_basic_variadic_call() {
    // Cell with variadic param called with multiple args
    assert_ok(
        "variadic_basic",
        r#"
cell sum(...nums: Int) -> Int
  let total = 0
  for n in nums
    total = total + n
  end
  return total
end

cell main() -> Int
  return sum(1, 2, 3)
end
"#,
    );
}

#[test]
fn t163_variadic_empty() {
    // Call a variadic cell with no variadic args
    assert_ok(
        "variadic_empty",
        r#"
cell collect(...items: String) -> list[String]
  return items
end

cell main() -> list[String]
  return collect()
end
"#,
    );
}

#[test]
fn t163_variadic_single_arg() {
    // Call a variadic cell with a single variadic arg
    assert_ok(
        "variadic_single",
        r#"
cell wrap(...items: Int) -> list[Int]
  return items
end

cell main() -> list[Int]
  return wrap(42)
end
"#,
    );
}

#[test]
fn t163_variadic_mixed_params() {
    // Cell with fixed + variadic params
    assert_ok(
        "variadic_mixed",
        r#"
cell format_msg(prefix: String, ...values: Int) -> String
  let result = prefix
  for v in values
    result = result
  end
  return result
end

cell main() -> String
  return format_msg("total:", 1, 2, 3)
end
"#,
    );
}

#[test]
fn t163_variadic_type_in_body() {
    // Variadic param is typed as list inside the body
    assert_ok(
        "variadic_type_list",
        r#"
cell count(...items: String) -> Int
  return len(items)
end

cell main() -> Int
  return count("a", "b", "c")
end
"#,
    );
}

#[test]
fn t163_variadic_no_extra_args() {
    // Mixed params: all fixed args provided, no variadic args
    assert_ok(
        "variadic_no_extra",
        r#"
cell greet(name: String, ...titles: String) -> String
  return name
end

cell main() -> String
  return greet("Alice")
end
"#,
    );
}

#[test]
fn t163_variadic_preserves_param_in_lir() {
    // Verify that the variadic flag is preserved in lowered LIR
    let src = r#"
cell take_many(...xs: Int) -> Int
  return len(xs)
end

cell main() -> Int
  return take_many(1, 2, 3)
end
"#;
    let md = markdown(src);
    let module = compile(&md).expect("should compile");
    // Find the take_many cell and check its variadic param
    let cell = module
        .cells
        .iter()
        .find(|c| c.name == "take_many")
        .expect("take_many cell should exist");
    assert_eq!(cell.params.len(), 1, "should have 1 param");
    assert!(cell.params[0].variadic, "param should be variadic");
}
