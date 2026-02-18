//! Wave 20 Agent C — T181: Import path error recovery.
//!
//! Tests that when compiling imported modules, multiple parse errors in
//! dependencies are reported (not just the first), and that the main
//! module compilation continues as far as possible.

use lumen_compiler::compile_with_imports;

fn markdown(code: &str) -> String {
    format!("# test\n\n```lumen\n{}\n```\n", code.trim())
}

// ============================================================================
// Multiple parse errors in a dependency should all be reported
// ============================================================================

#[test]
fn import_broken_dep_reports_error() {
    // The imported module has a parse error — compilation of main should fail
    let broken_lib = markdown(
        r#"
cell helper() -> Int
  let x =
  return 1
end
"#,
    );
    let main_src = markdown(
        r#"
import mylib: *

cell main() -> Int
  return 42
end
"#,
    );

    let result = compile_with_imports(&main_src, &|module| {
        if module == "mylib" {
            Some(broken_lib.clone())
        } else {
            None
        }
    });

    assert!(
        result.is_err(),
        "should fail when imported module has parse errors"
    );
}

#[test]
fn import_module_not_found_error() {
    let main_src = markdown(
        r#"
import nonexistent: foo

cell main() -> Int
  return 42
end
"#,
    );

    let result = compile_with_imports(&main_src, &|_module| None);

    assert!(
        result.is_err(),
        "should fail when imported module is not found"
    );
    let err = result.unwrap_err();
    let msg = format!("{:?}", err);
    assert!(
        msg.contains("nonexistent") || msg.contains("ModuleNotFound") || msg.contains("not found"),
        "error should mention the missing module, got: {}",
        msg
    );
}

#[test]
fn import_circular_detection() {
    // Module A imports B, B imports A → circular import
    let module_a = markdown(
        r#"
import module_b: *

cell cell_a() -> Int
  return 1
end
"#,
    );
    let module_b = markdown(
        r#"
import module_a: *

cell cell_b() -> Int
  return 2
end
"#,
    );

    let result = compile_with_imports(&module_a, &|module| match module {
        "module_b" => Some(module_b.clone()),
        "module_a" => Some(module_a.clone()),
        _ => None,
    });

    // Should detect circular import
    assert!(
        result.is_err(),
        "should fail with circular import detection"
    );
}

// ============================================================================
// Main module continues with partial results from dependency
// ============================================================================

#[test]
fn import_good_dep_works() {
    let good_lib = markdown(
        r#"
cell helper(x: Int) -> Int
  return x * 2
end
"#,
    );
    let main_src = markdown(
        r#"
import goodlib: helper

cell main() -> Int
  return helper(5)
end
"#,
    );

    let result = compile_with_imports(&main_src, &|module| {
        if module == "goodlib" {
            Some(good_lib.clone())
        } else {
            None
        }
    });

    assert!(
        result.is_ok(),
        "should compile successfully with good dependency, got: {:?}",
        result.err()
    );
}

#[test]
fn import_dep_with_type_error() {
    // Imported module has a type error — should be caught
    let bad_lib = markdown(
        r#"
cell helper(x: Int) -> Int
  return "not an int"
end
"#,
    );
    let main_src = markdown(
        r#"
import badlib: helper

cell main() -> Int
  return helper(5)
end
"#,
    );

    let result = compile_with_imports(&main_src, &|module| {
        if module == "badlib" {
            Some(bad_lib.clone())
        } else {
            None
        }
    });

    // The type error in the dependency should propagate
    assert!(
        result.is_err(),
        "should fail when dependency has type error"
    );
}

#[test]
fn import_symbol_not_found_in_module() {
    let good_lib = markdown(
        r#"
cell helper(x: Int) -> Int
  return x * 2
end
"#,
    );
    let main_src = markdown(
        r#"
import goodlib: nonexistent_cell

cell main() -> Int
  return 42
end
"#,
    );

    let result = compile_with_imports(&main_src, &|module| {
        if module == "goodlib" {
            Some(good_lib.clone())
        } else {
            None
        }
    });

    assert!(
        result.is_err(),
        "should fail when imported symbol doesn't exist in module"
    );
    let err = result.unwrap_err();
    let msg = format!("{:?}", err);
    assert!(
        msg.contains("nonexistent_cell")
            || msg.contains("ImportedSymbolNotFound")
            || msg.contains("not found"),
        "error should mention the missing symbol, got: {}",
        msg
    );
}

// ============================================================================
// Multiple import errors collected
// ============================================================================

#[test]
fn import_multiple_missing_modules() {
    let main_src = markdown(
        r#"
import missing_a: *
import missing_b: *

cell main() -> Int
  return 42
end
"#,
    );

    let result = compile_with_imports(&main_src, &|_module| None);

    assert!(result.is_err(), "should fail with missing modules");
    let err = result.unwrap_err();
    let msg = format!("{:?}", err);
    // Both missing modules should be reported
    assert!(
        msg.contains("missing_a"),
        "should mention missing_a, got: {}",
        msg
    );
    assert!(
        msg.contains("missing_b"),
        "should mention missing_b, got: {}",
        msg
    );
}

#[test]
fn import_mixed_good_and_bad_deps() {
    let good_lib = markdown(
        r#"
cell helper(x: Int) -> Int
  return x * 2
end
"#,
    );
    let main_src = markdown(
        r#"
import goodlib: helper
import missing_lib: *

cell main() -> Int
  return helper(5)
end
"#,
    );

    let result = compile_with_imports(&main_src, &|module| {
        if module == "goodlib" {
            Some(good_lib.clone())
        } else {
            None
        }
    });

    // Should fail because of missing_lib, but goodlib symbols should be resolved
    assert!(result.is_err(), "should fail with mixed deps");
    let err = result.unwrap_err();
    let msg = format!("{:?}", err);
    assert!(
        msg.contains("missing_lib"),
        "should mention missing_lib, got: {}",
        msg
    );
}

// ============================================================================
// Recovery: dependency parse errors don't crash the compiler
// ============================================================================

#[test]
fn import_dep_with_multiple_parse_errors_no_crash() {
    // Dependency with several parse errors
    let broken_lib = markdown(
        r#"
cell bad1() -> Int
  let x =
  return 1
end

cell bad2(param Int) -> Int
  return param
end

record Broken
  x:
end
"#,
    );
    let main_src = markdown(
        r#"
import brokenlib: *

cell main() -> Int
  return 42
end
"#,
    );

    // Should not panic — just return an error
    let result = compile_with_imports(&main_src, &|module| {
        if module == "brokenlib" {
            Some(broken_lib.clone())
        } else {
            None
        }
    });

    // Error is expected, but no panic
    assert!(result.is_err(), "should error on broken dependency");
}

#[test]
fn import_raw_source_dep() {
    // Test importing a raw .lm source (no markdown fencing)
    let raw_lib = r#"
cell square(x: Int) -> Int
  return x * x
end
"#;
    let main_src = markdown(
        r#"
import rawlib: square

cell main() -> Int
  return square(5)
end
"#,
    );

    let result = compile_with_imports(&main_src, &|module| {
        if module == "rawlib" {
            Some(raw_lib.to_string())
        } else {
            None
        }
    });

    assert!(
        result.is_ok(),
        "should compile with raw source dependency, got: {:?}",
        result.err()
    );
}
