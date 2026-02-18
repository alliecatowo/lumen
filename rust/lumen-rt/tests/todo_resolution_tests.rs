//! Wave 20 — T204: Resolve Test-Suite TODOs
//!
//! This file catalogues all TODO-like items found across the test suites and
//! provides resolution for each one: either a fix test, a documentation
//! explanation of why the workaround is acceptable, or a probe test that
//! captures the current behaviour so regressions are detectable.
//!
//! ## Catalogue of TODOs/Workarounds Found
//!
//! ### TODO-1: Commented-out `has_key` test using wrong map syntax (e2e.rs:2187-2200)
//!
//! **Location**: `rust/lumen-vm/tests/e2e.rs` lines 2187-2200
//! **Issue**: Test uses `#{"a": 1, "b": 2}` syntax. Comment says "Map literal
//!   syntax is broken in a separate bug".
//! **Resolution**: The `#{}` syntax was never part of the Lumen grammar.
//!   Standard `{"a": 1}` map literals work. The workaround is acceptable
//!   because the commented test was testing the wrong syntax. We provide
//!   a working replacement below.
//!
//! ### TODO-2: `role_interpolation.lm.md` skip/ignore (examples_compile.rs:14,164)
//!
//! **Location**: `rust/lumen-compiler/tests/examples_compile.rs`
//! **Issue**: Example file permanently skipped with `SKIP_COMPILE` and `#[ignore]`.
//! **Resolution**: This is a known parse issue in the example file itself
//!   (likely a role/interpolation interaction in the parser). The workaround
//!   of skipping is acceptable until the parser is enhanced to handle the
//!   specific construct. Not fixing here as it requires parser changes
//!   (owned by Agent A).
//!
//! ### TODO-3: Seven commented-out typecheck tests (typecheck_tests.rs)
//!
//! **Location**: `rust/lumen-compiler/tests/typecheck_tests.rs` lines
//!   72, 110, 170, 328, 357, 530, 719
//! **Issue**: Typechecker does not validate:
//!   - Function call argument types at call sites (lines 72, 530)
//!   - Record field argument types at construction (lines 110, 719)
//!   - Enum variant argument types at construction (line 170)
//!   - Heterogeneous list element types (line 328)
//!   - Heterogeneous map value types (line 357)
//! **Resolution**: These are deliberate implementation gaps documented with
//!   NOTE comments. The typechecker currently validates return types and
//!   variable types but not call-site argument types. This is a phased
//!   approach — the current behaviour is consistent and well-documented.
//!   We add probe tests below to track when these become detectable.
//!
//! ### TODO-4: Commented-out map operation tests (e2e.rs:2186-2200)
//!
//! **Location**: `rust/lumen-vm/tests/e2e.rs` lines 2186-2200
//! **Issue**: `has_key`, `merge`, `entries` tests all commented out.
//! **Resolution**: The tests used invalid `#{}` syntax. Standard `{}`
//!   map syntax works. We provide working replacements below.
//!
//! ### TODO-5: CLAUDE.md Rc vs Arc documentation mismatch
//!
//! **Location**: `CLAUDE.md` line 147
//! **Issue**: States `Rc<T>` and `Rc::make_mut()` but code uses `Arc<T>`.
//! **Resolution**: Documentation-only fix needed. The code correctly uses
//!   `Arc` for thread-safety. This is not a code bug. Documented in the
//!   drift notes file.

use lumen_compiler::compile;
use lumen_vm::values::{StringRef, Value};
use lumen_vm::vm::VM;

/// Helper: wrap raw Lumen code in markdown, compile it, run `main`, return the result.
fn run_main(source: &str) -> Value {
    let md = format!("# todo-test\n\n```lumen\n{}\n```\n", source.trim());
    let module = compile(&md).expect("source should compile");
    let mut vm = VM::new();
    vm.load(module);
    vm.execute("main", vec![]).expect("main should execute")
}

/// Helper: compile and run, return the result or error string.
fn try_run_main(source: &str) -> Result<Value, String> {
    let md = format!("# todo-test\n\n```lumen\n{}\n```\n", source.trim());
    let module = compile(&md).map_err(|e| e.to_string())?;
    let mut vm = VM::new();
    vm.load(module);
    vm.execute("main", vec![]).map_err(|e| e.to_string())
}

/// Helper: check if source compiles.
fn compiles_ok(source: &str) -> bool {
    let md = format!("# todo-test\n\n```lumen\n{}\n```\n", source.trim());
    compile(&md).is_ok()
}

// ============================================================================
// TODO-1 Resolution: Replacement tests for commented-out has_key
// ============================================================================

/// Replacement for the commented-out `e2e_has_key` test.
/// Uses standard `{"a": 1}` map literal syntax instead of `#{"a": 1}`.
#[test]
fn todo1_has_key_with_standard_map_syntax() {
    let result = run_main(
        r#"
cell main() -> Bool
  let m = {"a": 1, "b": 2}
  return has_key(m, "a")
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

/// has_key returns false for a missing key.
#[test]
fn todo1_has_key_missing_key() {
    let result = run_main(
        r#"
cell main() -> Bool
  let m = {"a": 1, "b": 2}
  return has_key(m, "c")
end
"#,
    );
    assert_eq!(result, Value::Bool(false));
}

// ============================================================================
// TODO-3 Resolution: Probe tests for typecheck gaps
// ============================================================================

/// Probe: does the typechecker catch wrong argument types at call sites?
/// As of Wave 20, the typechecker NOW catches call-site argument type mismatches.
/// This means the commented-out tests in typecheck_tests.rs can be uncommented.
#[test]
fn todo3_probe_callsite_typecheck_param_type() {
    let compiles = compiles_ok(
        r#"
cell add(a: Int, b: Int) -> Int
  return a + b
end

cell main() -> Int
  return add(1, "hello")
end
"#,
    );
    // Updated: the typechecker now catches call-site argument type mismatches.
    assert!(
        !compiles,
        "Typechecker now catches call-site argument type mismatches (previously a gap)"
    );
}

/// Probe: does the typechecker catch wrong record field types at construction?
#[test]
fn todo3_probe_callsite_typecheck_record_field() {
    let compiles = compiles_ok(
        r#"
record User
  name: String
  age: Int
end

cell main() -> User
  return User(name: "alice", age: "thirty")
end
"#,
    );
    // Updated: the typechecker now catches record field type mismatches at construction.
    assert!(
        !compiles,
        "Typechecker now catches record field type mismatches at construction (previously a gap)"
    );
}

/// Probe: does the typechecker catch wrong enum variant payload types?
#[test]
fn todo3_probe_callsite_typecheck_enum_variant() {
    let compiles = compiles_ok(
        r#"
enum Result
  Ok(value: Int)
  Err(message: String)
end

cell main() -> Result
  return Ok(value: "not an int")
end
"#,
    );
    assert!(
        compiles,
        "Expected: enum variant type checking at construction not yet implemented"
    );
}

/// Probe: does the typechecker catch heterogeneous list elements?
#[test]
fn todo3_probe_callsite_typecheck_list_elements() {
    let compiles = compiles_ok(
        r#"
cell main() -> list[Int]
  return [1, 2, "three", 4]
end
"#,
    );
    assert!(
        compiles,
        "Expected: list element type checking not yet implemented"
    );
}

/// Probe: does the typechecker catch heterogeneous map values?
#[test]
fn todo3_probe_callsite_typecheck_map_values() {
    let compiles = compiles_ok(
        r#"
cell main() -> map[String, Int]
  return {"a": 1, "b": "two", "c": 3}
end
"#,
    );
    assert!(
        compiles,
        "Expected: map value type checking not yet implemented"
    );
}

// ============================================================================
// TODO-4 Resolution: Working map operation tests
// ============================================================================

/// Replacement for commented-out map merge test.
#[test]
fn todo4_map_merge_with_standard_syntax() {
    let result = run_main(
        r#"
cell main() -> Int
  let m1 = {"a": 1, "b": 2}
  let m2 = {"c": 3}
  let merged = merge(m1, m2)
  return merged["a"] + merged["c"]
end
"#,
    );
    assert_eq!(result, Value::Int(4)); // 1 + 3
}

/// Replacement for commented-out map entries test.
#[test]
fn todo4_map_entries_with_standard_syntax() {
    let result = run_main(
        r#"
cell main() -> Int
  let m = {"a": 1}
  let pairs = entries(m)
  return length(pairs)
end
"#,
    );
    assert_eq!(result, Value::Int(1));
}

/// Verify map field access works with standard syntax.
#[test]
fn todo4_map_index_access() {
    let result = run_main(
        r#"
cell main() -> Int
  let m = {"x": 10, "y": 20}
  return m["x"] + m["y"]
end
"#,
    );
    assert_eq!(result, Value::Int(30));
}

// ============================================================================
// Additional resolution: verify effect handler tests match current grammar
// ============================================================================

/// Confirm algebraic effect handle/perform/resume pattern matches SPEC syntax.
/// The SPEC says:
///   `perform Effect.operation(args)`
///   `handle body with Effect.op(params) => resume(value) end`
///
/// DISCOVERY: handler clause parameters (e.g. `msg` in `Logger.log(msg) =>`) are
/// flagged as UndefinedVar by the resolver. This is a known scoping gap — the
/// handler parameter bindings are not yet introduced into scope for the handler body.
/// This test documents the gap: the basic syntax is accepted by the parser, but the
/// handler parameter variable is not resolved. When the scoping gap is fixed, change
/// the assertion to `assert!(compiles, ...)`.
#[test]
fn todo_effect_handler_syntax_matches_spec() {
    // Test 1: basic effect/handle/perform syntax parses and compiles when the
    // handler body does NOT reference the handler parameter.
    let compiles_basic = compiles_ok(
        r#"
effect Logger
  cell log(msg: String) -> String
end

cell main() -> String / {Logger}
  let result = handle
    perform Logger.log("hello")
  with
    Logger.log(msg) =>
      resume("logged")
  end
  return result
end
"#,
    );
    // The basic case (handler parameter declared but not referenced in body)
    // compiles successfully. The syntax is accepted by parser and resolver.
    assert!(
        compiles_basic,
        "Basic effect handler syntax should compile when handler param is unused"
    );

    // Test 2: the handler parameter variable is referenced — fails because
    // the resolver flags `msg` as UndefinedVar (scoping gap).
    let compiles_with_param = compiles_ok(
        r#"
effect Logger
  cell log(msg: String) -> String
end

cell main() -> String / {Logger}
  let result = handle
    perform Logger.log("hello")
  with
    Logger.log(msg) =>
      resume("logged: " + msg)
  end
  return result
end
"#,
    );
    assert!(
        !compiles_with_param,
        "Effect handler parameter scoping gap: `msg` is UndefinedVar. \
         When handler parameter binding is implemented, flip this assertion."
    );
}

/// Confirm that while-loop backward jumps work correctly (regression coverage).
/// This was a critical bug related to signed jump offsets (sax/sax_val).
#[test]
fn todo_regression_backward_jump_still_works() {
    let result = run_main(
        r#"
cell main() -> Int
  let mut sum = 0
  let mut i = 0
  while i < 100
    sum = sum + 1
    i = i + 1
  end
  return sum
end
"#,
    );
    assert_eq!(result, Value::Int(100));
}

/// Confirm match statement register clobber regression is still fixed.
#[test]
fn todo_regression_match_register_clobber_fixed() {
    let result = run_main(
        r#"
cell classify(x: Int) -> String
  match x
    1 -> return "one"
    2 -> return "two"
    3 -> return "three"
    _ -> return "other"
  end
end

cell main() -> String
  return classify(3)
end
"#,
    );
    match &result {
        Value::String(StringRef::Owned(s)) => assert_eq!(s, "three"),
        other => panic!("expected 'three', got {:?}", other),
    }
}

// ============================================================================
// Additional: verify the set literal and set operations work end-to-end
// ============================================================================

/// Verify set operations that were previously only tested at compile level.
#[test]
fn todo_set_operations_e2e() {
    let result = run_main(
        r#"
cell main() -> Int
  let s1 = {1, 2, 3, 4, 5}
  let s2 = add(s1, 6)
  let s3 = remove(s2, 3)
  return size(s3)
end
"#,
    );
    assert_eq!(result, Value::Int(5)); // started 5, added 6 (=6), removed 3 (=5)
}

/// Verify for-in iteration over sets works (BTreeSet = sorted order).
#[test]
fn todo_set_iteration_order() {
    let result = run_main(
        r#"
cell main() -> Int
  let s = {30, 10, 20}
  let mut first = 0
  for x in s
    first = x
    break
  end
  return first
end
"#,
    );
    // BTreeSet iterates in sorted order, so first element is 10.
    assert_eq!(result, Value::Int(10));
}

// ============================================================================
// Additional: runtime error handling tests to cover gaps
// ============================================================================

/// Verify division by zero produces a runtime error (not a crash).
#[test]
fn todo_runtime_division_by_zero() {
    let result = try_run_main(
        r#"
cell main() -> Int
  let x = 10
  let y = 0
  return x / y
end
"#,
    );
    assert!(
        result.is_err(),
        "Division by zero should produce a runtime error"
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("division") || err.contains("zero") || err.contains("divide"),
        "Error should mention division by zero, got: {}",
        err
    );
}

/// Verify out-of-bounds list index produces a runtime error.
#[test]
fn todo_runtime_index_out_of_bounds() {
    let result = try_run_main(
        r#"
cell main() -> Int
  let xs = [1, 2, 3]
  return xs[10]
end
"#,
    );
    assert!(
        result.is_err(),
        "Out-of-bounds index should produce a runtime error"
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("index")
            || err.contains("out of")
            || err.contains("bounds")
            || err.contains("range"),
        "Error should mention index bounds, got: {}",
        err
    );
}
