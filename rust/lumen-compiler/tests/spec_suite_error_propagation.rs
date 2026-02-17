//! Spec suite: error propagation tests.
//!
//! Tests for:
//! - Error propagation operator `?`
//! - Try/else expression (`try expr else |err| fallback`)

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
// T121: Error propagation operator `?`
// ============================================================================

#[test]
fn t121_basic_question_mark_on_result() {
    assert_ok(
        "t121_basic_question_mark",
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
fn t121_chained_question_mark() {
    assert_ok(
        "t121_chained_question_mark",
        r#"
cell step_a() -> result[Int, String]
  Ok(10)
end

cell step_b(x: Int) -> result[Int, String]
  Ok(x * 2)
end

cell main() -> result[Int, String]
  let a = step_a()?
  let b = step_b(a)?
  Ok(b + 1)
end
"#,
    );
}

#[test]
fn t121_question_mark_unwraps_ok_type() {
    // ? on result[Int, E] should produce Int
    assert_ok(
        "t121_unwraps_ok_type",
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

#[test]
fn t121_question_mark_with_string_result() {
    assert_ok(
        "t121_string_result",
        r#"
cell get_name() -> result[String, String]
  Ok("Alice")
end

cell main() -> result[String, String]
  let name = get_name()?
  Ok(name)
end
"#,
    );
}

#[test]
fn t121_try_prefix_form() {
    // `try expr` (prefix form) is equivalent to `expr?`
    assert_ok(
        "t121_try_prefix",
        r#"
cell get_value() -> result[Int, String]
  Ok(42)
end

cell main() -> result[Int, String]
  let val = try get_value()
  Ok(val + 1)
end
"#,
    );
}

// ============================================================================
// T122: Try/else expression
// ============================================================================

#[test]
fn t122_basic_try_else() {
    assert_ok(
        "t122_basic_try_else",
        r#"
cell might_fail() -> result[Int, String]
  Ok(42)
end

cell main() -> Int
  let val = try might_fail() else |err| 0
  return val
end
"#,
    );
}

#[test]
fn t122_try_else_with_fallback_value() {
    assert_ok(
        "t122_try_else_fallback",
        r#"
cell risky() -> result[String, String]
  Err("oops")
end

cell main() -> String
  let s = try risky() else |e| "default"
  return s
end
"#,
    );
}

#[test]
fn t122_try_else_no_pipe_binding() {
    // Without |err| binding, the error binding defaults to _err
    assert_ok(
        "t122_no_pipe_binding",
        r#"
cell might_fail() -> result[Int, String]
  Ok(10)
end

cell main() -> Int
  let val = try might_fail() else 0
  return val
end
"#,
    );
}

#[test]
fn t122_try_else_chained() {
    assert_ok(
        "t122_chained",
        r#"
cell first() -> result[Int, String]
  Err("fail")
end

cell second() -> result[Int, String]
  Ok(99)
end

cell main() -> Int
  let a = try first() else |e1| 0
  let b = try second() else |e2| 0
  return a + b
end
"#,
    );
}

#[test]
fn t122_try_else_handler_uses_error_binding() {
    // The handler should be able to reference the error binding
    assert_ok(
        "t122_uses_error_binding",
        r#"
cell might_fail() -> result[Int, String]
  Err("bad input")
end

cell main() -> String
  let val = try might_fail() else |err| err
  return val
end
"#,
    );
}

#[test]
fn t122_try_else_with_result_returning_function() {
    assert_ok(
        "t122_result_function",
        r#"
cell parse_int(s: String) -> result[Int, String]
  Ok(42)
end

cell main() -> Int
  let n = try parse_int("42") else |e| -1
  return n
end
"#,
    );
}

#[test]
fn t122_try_else_nested() {
    assert_ok(
        "t122_nested",
        r#"
cell inner() -> result[Int, String]
  Ok(5)
end

cell outer() -> result[Int, String]
  Ok(10)
end

cell main() -> Int
  let a = try outer() else |e| try inner() else |e2| 0
  return a
end
"#,
    );
}

#[test]
fn t122_try_else_with_complex_handler() {
    assert_ok(
        "t122_complex_handler",
        r#"
cell get_data() -> result[Int, String]
  Err("not found")
end

cell main() -> Int
  let val = try get_data() else |err| 42 + 1
  return val
end
"#,
    );
}

#[test]
fn t122_try_else_on_non_result_type() {
    // When try/else is used on a non-result type, the handler is still parsed
    // but the expression just returns the value directly
    assert_ok(
        "t122_non_result_type",
        r#"
cell get_int() -> Int
  42
end

cell main() -> Int
  let val = try get_int() else |err| 0
  return val
end
"#,
    );
}

#[test]
fn t122_try_else_in_expression_position() {
    // try/else should work as an expression in a larger expression context
    assert_ok(
        "t122_expr_position",
        r#"
cell might_fail() -> result[Int, String]
  Ok(10)
end

cell main() -> Int
  let val = (try might_fail() else |e| 0) + 5
  return val
end
"#,
    );
}

#[test]
fn t122_try_else_with_bool_result() {
    assert_ok(
        "t122_bool_result",
        r#"
cell check() -> result[Bool, String]
  Ok(true)
end

cell main() -> Bool
  let ok = try check() else |e| false
  return ok
end
"#,
    );
}

#[test]
fn t122_try_else_multiple_bindings_in_scope() {
    // Multiple try/else expressions each with their own error binding
    assert_ok(
        "t122_multiple_bindings",
        r#"
cell a() -> result[Int, String]
  Ok(1)
end

cell b() -> result[Int, String]
  Ok(2)
end

cell c() -> result[Int, String]
  Ok(3)
end

cell main() -> Int
  let x = try a() else |e1| 0
  let y = try b() else |e2| 0
  let z = try c() else |e3| 0
  return x + y + z
end
"#,
    );
}

#[test]
fn t122_try_else_with_string_fallback() {
    assert_ok(
        "t122_string_fallback",
        r#"
cell fetch_name() -> result[String, String]
  Err("not found")
end

cell main() -> String
  let name = try fetch_name() else |e| "anonymous"
  return name
end
"#,
    );
}

// ============================================================================
// T121 + T122 combined
// ============================================================================

#[test]
fn t121_t122_mixed_usage() {
    // Mix ? operator and try/else in the same function
    assert_ok(
        "t121_t122_mixed",
        r#"
cell step1() -> result[Int, String]
  Ok(10)
end

cell step2(x: Int) -> result[Int, String]
  Ok(x + 5)
end

cell main() -> result[Int, String]
  let a = try step1() else |e| 0
  let b = step2(a)?
  Ok(b)
end
"#,
    );
}
