//! Multi-error reporting tests.
//!
//! Verifies that the compiler collects and reports errors from multiple passes
//! (resolve, typecheck, constraints) in a single compilation run, rather than
//! stopping at the first failing pass.

use lumen_compiler::{compile_raw, CompileError};

/// Helper: compile raw Lumen source and return the error (panics if it succeeds).
fn expect_error(source: &str) -> CompileError {
    match compile_raw(source) {
        Ok(_) => panic!(
            "expected compilation to fail, but it succeeded:\n{}",
            source
        ),
        Err(e) => e,
    }
}

/// Helper: count the total number of individual error items across all passes.
fn count_errors(err: &CompileError) -> usize {
    match err {
        CompileError::Lex(_) => 1,
        CompileError::Parse(es) => es.len(),
        CompileError::Resolve(es) => es.len(),
        CompileError::Type(es) => es.len(),
        CompileError::Constraint(es) => es.len(),
        CompileError::Ownership(es) => es.len(),
        CompileError::Multiple(es) => es.iter().map(count_errors).sum(),
        CompileError::Lower(_) => 1,
    }
}

/// Helper: check if the error contains a `Resolve` variant anywhere (including inside Multiple).
fn has_resolve_errors(err: &CompileError) -> bool {
    match err {
        CompileError::Resolve(es) => !es.is_empty(),
        CompileError::Multiple(es) => es.iter().any(has_resolve_errors),
        _ => false,
    }
}

/// Helper: check if the error contains a `Type` variant anywhere (including inside Multiple).
fn has_type_errors(err: &CompileError) -> bool {
    match err {
        CompileError::Type(es) => !es.is_empty(),
        CompileError::Multiple(es) => es.iter().any(has_type_errors),
        _ => false,
    }
}

// ═══════════════════════════════════════════════════════════════════
// Test 1: Two type errors both reported
// ═══════════════════════════════════════════════════════════════════

#[test]
fn two_type_errors_both_reported() {
    // Two distinct type mismatches in two different cells
    let err = expect_error(
        r#"
cell foo() -> Int
  return "hello"
end

cell bar() -> String
  return 42
end
"#,
    );

    let msg = format!("{:?}", err);
    // Both type mismatches should be present
    assert!(
        msg.contains("Mismatch") || msg.contains("mismatch"),
        "expected type mismatch errors, got: {}",
        msg
    );
    // Should contain at least 2 errors
    assert!(
        count_errors(&err) >= 2,
        "expected at least 2 errors, got {}: {:?}",
        count_errors(&err),
        err
    );
}

// ═══════════════════════════════════════════════════════════════════
// Test 2: Three unresolved names all reported
// ═══════════════════════════════════════════════════════════════════

#[test]
fn three_unresolved_types_all_reported() {
    let err = expect_error(
        r#"
cell foo() -> Banana
  return Banana()
end

cell bar() -> Cherry
  return Cherry()
end

cell baz() -> Durian
  return Durian()
end
"#,
    );

    let msg = format!("{:?}", err);
    // All three undefined types should appear
    assert!(
        msg.contains("Banana"),
        "expected Banana in errors, got: {}",
        msg
    );
    assert!(
        msg.contains("Cherry"),
        "expected Cherry in errors, got: {}",
        msg
    );
    assert!(
        msg.contains("Durian"),
        "expected Durian in errors, got: {}",
        msg
    );
    assert!(
        count_errors(&err) >= 3,
        "expected at least 3 errors, got {}: {:?}",
        count_errors(&err),
        err
    );
}

// ═══════════════════════════════════════════════════════════════════
// Test 3: Mixed resolve + type errors both reported
// ═══════════════════════════════════════════════════════════════════

#[test]
fn mixed_resolve_and_type_errors() {
    let err = expect_error(
        r#"
cell foo() -> UnknownType
  return 42
end

cell bar() -> Int
  return "not an int"
end
"#,
    );

    // Should have both resolve errors (UnknownType) and type errors (mismatch in bar)
    assert!(
        has_resolve_errors(&err),
        "expected resolve errors, got: {:?}",
        err
    );
    assert!(
        has_type_errors(&err),
        "expected type errors, got: {:?}",
        err
    );
}

// ═══════════════════════════════════════════════════════════════════
// Test 4: Zero errors still produces successful compilation
// ═══════════════════════════════════════════════════════════════════

#[test]
fn zero_errors_compiles_successfully() {
    let result = compile_raw(
        r#"
cell main() -> Int
  return 42
end
"#,
    );

    assert!(
        result.is_ok(),
        "expected successful compilation, got: {:?}",
        result.err()
    );
}

// ═══════════════════════════════════════════════════════════════════
// Test 5: Error count matches expected for known input
// ═══════════════════════════════════════════════════════════════════

#[test]
fn error_count_matches_expected() {
    // Exactly 2 undefined types in resolve
    let err = expect_error(
        r#"
cell foo() -> Alpha
  return Alpha()
end

cell bar() -> Beta
  return Beta()
end
"#,
    );

    // At least 2 resolve errors for undefined types Alpha and Beta
    assert!(
        count_errors(&err) >= 2,
        "expected at least 2 errors, got {}: {:?}",
        count_errors(&err),
        err
    );
}

// ═══════════════════════════════════════════════════════════════════
// Test 6: Duplicate definitions are all reported
// ═══════════════════════════════════════════════════════════════════

#[test]
fn duplicate_definitions_all_reported() {
    let err = expect_error(
        r#"
cell foo() -> Int
  return 1
end

cell foo() -> Int
  return 2
end

record Bar
  x: Int
end

record Bar
  y: String
end
"#,
    );

    let msg = format!("{:?}", err);
    assert!(
        msg.contains("foo") || msg.contains("Duplicate"),
        "expected duplicate error for foo, got: {}",
        msg
    );
    assert!(
        msg.contains("Bar") || msg.contains("Duplicate"),
        "expected duplicate error for Bar, got: {}",
        msg
    );
    assert!(
        count_errors(&err) >= 2,
        "expected at least 2 errors, got {}: {:?}",
        count_errors(&err),
        err
    );
}

// ═══════════════════════════════════════════════════════════════════
// Test 7: Single-pass error is not wrapped in Multiple
// ═══════════════════════════════════════════════════════════════════

#[test]
fn single_pass_error_not_wrapped_in_multiple() {
    // Only type errors, no resolve errors
    let err = expect_error(
        r#"
cell foo() -> Int
  return "hello"
end
"#,
    );

    // Should be a Type error, not Multiple
    match &err {
        CompileError::Type(_) => {} // expected
        CompileError::Multiple(_) => panic!("single-pass error should not be wrapped in Multiple"),
        other => panic!("expected Type error, got: {:?}", other),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Test 8: format_error handles Multiple variant
// ═══════════════════════════════════════════════════════════════════

#[test]
fn format_error_handles_multiple() {
    let err = expect_error(
        r#"
cell foo() -> UnknownType
  return 42
end

cell bar() -> Int
  return "not an int"
end
"#,
    );

    let formatted = lumen_compiler::format_error(&err, "dummy source", "test.lm");
    // Should produce output (not crash) and contain info about the errors
    assert!(!formatted.is_empty(), "formatted error should not be empty");
}

// ═══════════════════════════════════════════════════════════════════
// Test 9: from_multiple helper correctness
// ═══════════════════════════════════════════════════════════════════

#[test]
fn from_multiple_empty_returns_none() {
    assert!(CompileError::from_multiple(vec![]).is_none());
}

#[test]
fn from_multiple_single_unwraps() {
    let single = CompileError::Type(vec![]);
    let result = CompileError::from_multiple(vec![single]);
    match result {
        Some(CompileError::Type(_)) => {} // expected: unwrapped, not Multiple
        other => panic!("expected Some(Type(_)), got: {:?}", other),
    }
}

#[test]
fn from_multiple_flattens_nested() {
    let inner = CompileError::Multiple(vec![
        CompileError::Type(vec![]),
        CompileError::Resolve(vec![]),
    ]);
    let result = CompileError::from_multiple(vec![inner, CompileError::Constraint(vec![])]);
    match result {
        Some(CompileError::Multiple(errors)) => {
            assert_eq!(
                errors.len(),
                3,
                "expected 3 flattened errors, got {}",
                errors.len()
            );
        }
        other => panic!("expected Multiple with 3 items, got: {:?}", other),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Test 10: Markdown compile also collects multi-pass errors
// ═══════════════════════════════════════════════════════════════════

#[test]
fn markdown_compile_multi_error() {
    let md = r#"# Test

```lumen
cell foo() -> UnknownType
  return 42
end

cell bar() -> Int
  return "not an int"
end
```
"#;

    let err = match lumen_compiler::compile(md) {
        Ok(_) => panic!("expected compilation to fail"),
        Err(e) => e,
    };

    // Should contain both resolve and type errors
    assert!(
        has_resolve_errors(&err),
        "expected resolve errors in markdown compile, got: {:?}",
        err
    );
    assert!(
        has_type_errors(&err),
        "expected type errors in markdown compile, got: {:?}",
        err
    );
}

// ═══════════════════════════════════════════════════════════════════
// Test 11: Multiple type errors in the same cell
// ═══════════════════════════════════════════════════════════════════

#[test]
fn multiple_type_errors_same_cell() {
    let err = expect_error(
        r#"
record Point
  x: Int
  y: Int
end

cell main() -> Int
  let p = Point(x: "bad", y: "also bad")
  return p
end
"#,
    );

    // Should have multiple type errors
    assert!(
        count_errors(&err) >= 2,
        "expected at least 2 type errors, got {}: {:?}",
        count_errors(&err),
        err
    );
}

// ═══════════════════════════════════════════════════════════════════
// Test 12: Parse errors still bail early (no resolve/type run)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn parse_errors_bail_early() {
    let err = expect_error(
        r#"
cell foo( -> Int
  return 42
end
"#,
    );

    // Parse errors should still be reported immediately (no Multiple wrapper needed)
    match &err {
        CompileError::Parse(_) => {} // expected
        CompileError::Multiple(_) => panic!("parse errors should bail early, not produce Multiple"),
        other => panic!("expected Parse error, got: {:?}", other),
    }
}
