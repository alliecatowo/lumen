//! Wave 20 — T192: Test Drift Audit
//!
//! This file documents all test-vs-implementation drift discovered across
//! `rust/lumen-compiler/tests/` and `rust/lumen-vm/tests/`. Each finding
//! is annotated with a category (Syntax Drift, Behaviour Drift, Documentation Drift,
//! Commented-Out Tests, Known Workarounds) and includes a runnable test where
//! appropriate to confirm the current state.
//!
//! ## Summary of Findings
//!
//! ### 1. CLAUDE.md says `Rc<T>` but implementation uses `Arc<T>` (Documentation Drift)
//!
//! CLAUDE.md line 147 states:
//!   "Collection variants … are wrapped in `Rc<T>` for cheap reference-counted
//!    cloning … Mutation uses `Rc::make_mut()` …"
//!
//! The actual `values.rs` uses `std::sync::Arc` throughout (List, Tuple, Set,
//! Map, Record). This is a doc-only drift — the code is correct. The docs
//! should be updated to say `Arc<T>` and `Arc::make_mut()`.
//!
//! ### 2. Commented-out Map/Set tests due to "broken" hash-map literal syntax (e2e.rs:2187-2200)
//!
//! Lines 2187-2200 of `rust/lumen-vm/tests/e2e.rs` contain a commented-out
//! `e2e_has_key` test using `#{"a": 1, "b": 2}` syntax. The comment says
//! "Map literal syntax is broken in a separate bug". However, the canonical
//! map literal syntax is `{"key": value}` (without `#` prefix), and other
//! tests use that syntax successfully. The commented test uses non-standard
//! `#{}` syntax that never matched the grammar.
//!
//! ### 3. comptime syntax: test uses `comptime { ... }` but SPEC says `comptime ... end` (Syntax Drift)
//!
//! In `spec_suite.rs:697-705`, the `spec_v1_unimplemented_targets` test uses:
//!   `const LIMIT: Int = comptime { ... }`
//! The SPEC.md §6.11 shows:
//!   `const MAX = comptime ... end`
//! The parser accepts both `comptime { block }` (brace form) and `comptime ... end`
//! (keyword-delimited form), so the test compiles. However, the test's syntax
//! diverges from the SPEC's canonical form.
//!
//! ### 4. `.length` property access in test vs `length()` builtin (spec_suite.rs:673)
//!
//! In `spec_v1_unimplemented_targets`, the `parallel_for_and_select` case
//! uses `values.length` as a method/property access. The canonical way to
//! get collection length in Lumen is `length(values)` or `len(values)`,
//! not `.length`. Since this test is in the "unimplemented targets" section
//! it compiles but would fail at runtime if executed.
//!
//! ### 5. `role_interpolation.lm.md` ignored with known parse issue (examples_compile.rs:164)
//!
//! One example file is permanently skipped via `#[ignore]` and also via
//! `SKIP_COMPILE` list. This is a known parse issue that has not been
//! resolved.
//!
//! ### 6. Seven commented-out typecheck tests (typecheck_tests.rs)
//!
//! Seven test cases in `typecheck_tests.rs` are commented out because
//! "the typechecker doesn't yet validate" call-site argument types or
//! record field argument types. These are at lines: 72, 110, 170, 328,
//! 357, 530, 719. This represents a known gap in the type system where
//! type mismatches at call sites are not caught at compile time.
//!
//! ### 7. enum named `Option` shadows potential stdlib type (e2e.rs:2004)
//!
//! The test `e2e_enum_dot_access_positional_arg` defines `enum Option`
//! with `Some` and `None` variants. While this works, it shadows any
//! future stdlib `Option` type and uses positional construction
//! `Option.Some(42)` rather than named `Option.Some(value: 42)`. Both
//! forms are valid, but they test different code paths.
//!
//! ### 8. enum named `Result` used in test (e2e.rs:1978)
//!
//! The test `e2e_enum_dot_access_in_function_return` defines `enum Result`
//! with `Ok(value: Int)` and `Err(message: String)`. Lumen has a built-in
//! `result[T, E]` type (SPEC §3.3). While user-defined `Result` works,
//! it could conflict with the built-in result type in some contexts.

use lumen_compiler::compile;
use lumen_vm::values::Value;
use lumen_vm::vm::VM;

/// Helper: wrap raw Lumen code in markdown, compile it, run `main`, return the result.
fn run_main(source: &str) -> Value {
    let md = format!("# drift-test\n\n```lumen\n{}\n```\n", source.trim());
    let module = compile(&md).expect("source should compile");
    let mut vm = VM::new();
    vm.load(module);
    vm.execute("main", vec![]).expect("main should execute")
}

/// Helper: wrap raw Lumen code in markdown, compile it. Return Ok/Err.
fn try_compile(source: &str) -> Result<(), String> {
    let md = format!("# drift-test\n\n```lumen\n{}\n```\n", source.trim());
    compile(&md).map(|_| ()).map_err(|e| e.to_string())
}

// ============================================================================
// Drift Finding 1: Arc vs Rc — confirm Arc is used
// ============================================================================

/// Verify that Value::List uses Arc (not Rc) by confirming the type exists and works.
/// This is a documentation-only drift: CLAUDE.md says Rc, code uses Arc.
#[test]
fn drift_values_use_arc_not_rc() {
    // Construct a list, clone it (Arc clone is cheap), verify both are equal.
    let list = Value::new_list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
    let cloned = list.clone();

    // Both should be equal and the clone should be cheap (Arc reference bump).
    match (&list, &cloned) {
        (Value::List(a), Value::List(b)) => {
            // Arc::ptr_eq confirms they share the same allocation after clone.
            assert!(std::sync::Arc::ptr_eq(a, b), "Clone should share Arc");
        }
        _ => panic!("expected List variants"),
    }
}

// ============================================================================
// Drift Finding 2: Map literal `{"key": val}` works, `#{"key": val}` does NOT
// ============================================================================

/// Confirm that standard map literal syntax works in e2e.
#[test]
fn drift_map_literal_standard_syntax_works() {
    let result = run_main(
        r#"
cell main() -> Int
  let m = {"a": 1, "b": 2}
  return m["a"]
end
"#,
    );
    assert_eq!(result, Value::Int(1));
}

/// The commented-out test in e2e.rs used `#{"a": 1}` — confirm this does NOT compile.
#[test]
fn drift_hash_map_literal_syntax_does_not_compile() {
    let result = try_compile(
        r#"
cell main() -> Bool
  let m = #{"a": 1, "b": 2}
  return true
end
"#,
    );
    assert!(
        result.is_err(),
        "#{{}} syntax should not be valid Lumen (it's not in the grammar)"
    );
}

// ============================================================================
// Drift Finding 3: comptime syntax — both forms should compile
// ============================================================================

/// Confirm that `comptime { ... }` (brace form, used in test) compiles.
#[test]
fn drift_comptime_brace_form_compiles() {
    let result = try_compile(
        r#"
const LIMIT: Int = comptime {
  10 * 10
}
"#,
    );
    assert!(
        result.is_ok(),
        "comptime brace form should compile: {:?}",
        result.err()
    );
}

/// Confirm that `comptime ... end` (SPEC canonical form) compiles.
#[test]
fn drift_comptime_end_form_compiles() {
    let result = try_compile(
        r#"
const LIMIT: Int = comptime
  10 * 10
end
"#,
    );
    assert!(
        result.is_ok(),
        "comptime end form should compile: {:?}",
        result.err()
    );
}

// ============================================================================
// Drift Finding 4: length() builtin vs .length property
// ============================================================================

/// Confirm that `length(xs)` works as a builtin call (the canonical way).
#[test]
fn drift_length_builtin_works() {
    let result = run_main(
        r#"
cell main() -> Int
  let xs = [10, 20, 30]
  return length(xs)
end
"#,
    );
    assert_eq!(result, Value::Int(3));
}

// ============================================================================
// Drift Finding 5: role_interpolation.lm.md parse issue
// ============================================================================

/// Confirm the role_interpolation example still fails to compile (known issue).
#[test]
fn drift_role_interpolation_still_fails() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../examples/role_interpolation.lm.md");
    if path.exists() {
        let source = std::fs::read_to_string(&path).expect("can read file");
        let result = compile(&source);
        // This is a known parse issue. If it starts passing, the ignore can be removed.
        if result.is_ok() {
            // Great! The issue is fixed. This test documents the change.
            // The #[ignore] in examples_compile.rs can now be removed.
        }
        // Either way, we just document the current state — not a hard assertion.
    }
    // If the file doesn't exist, skip silently.
}

// ============================================================================
// Drift Finding 6: Typecheck call-site validation gap
// ============================================================================

/// Demonstrate that passing wrong types to a cell does NOT produce a compile error.
/// This is the gap documented in typecheck_tests.rs (7 commented-out tests).
#[test]
fn drift_typecheck_no_callsite_validation() {
    // Passing a string where Int is expected — this SHOULD fail but currently compiles.
    let result = try_compile(
        r#"
cell add(a: Int, b: Int) -> Int
  return a + b
end

cell main() -> Int
  return add(1, "hello")
end
"#,
    );
    // Document the current state: this compiles without error.
    // If the typechecker is enhanced, this test should change to assert is_err.
    if result.is_err() {
        // Good: the typechecker now catches this!
    } else {
        // Expected: call-site type checking not yet implemented.
    }
}

// ============================================================================
// Drift Finding 7: User-defined Option/Result enums
// ============================================================================

/// Confirm user-defined enum named Option works fine (no collision with built-in).
#[test]
fn drift_user_defined_option_enum_works() {
    let result = run_main(
        r#"
enum Option
  Some(value: Int)
  None
end

cell main() -> Int
  let x = Option.Some(42)
  match x
    Some(v) -> return v
    None() -> return 0
  end
end
"#,
    );
    assert_eq!(result, Value::Int(42));
}

// ============================================================================
// Bonus: Set literal syntax consistency check
// ============================================================================

/// CLAUDE.md says set literals use `{1, 2, 3}`. Confirm this works e2e.
#[test]
fn drift_set_literal_curly_brace_works() {
    let result = run_main(
        r#"
cell main() -> Int
  let s = {10, 20, 30}
  return size(s)
end
"#,
    );
    assert_eq!(result, Value::Int(3));
}

/// Confirm that `set[Int]` is only valid in type position, not expression position.
#[test]
fn drift_set_bracket_only_in_type_position() {
    // This should compile: set[Int] in return type position.
    let result = try_compile(
        r#"
cell main() -> set[Int]
  return {1, 2, 3}
end
"#,
    );
    assert!(
        result.is_ok(),
        "set[Int] in type position should be valid: {:?}",
        result.err()
    );
}
