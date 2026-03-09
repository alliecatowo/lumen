//! Integration hardening tests (ALLIE-157).
//!
//! Three targeted integration tests that verify the full pipeline is robust:
//!
//! 1. Parsing round-trip: valid source → parse → AST → no crash
//! 2. VM execution: compile a .lm.md source string → execute in VM → correct result
//! 3. CLI exit codes: `lumen check` exits 1 on invalid input, 0 on valid input
//!
//! These tests are intentionally minimal and self-contained so they catch
//! regressions without requiring external files.

// ── Test 1: Parse round-trip ─────────────────────────────────────────────────

/// Verify that a valid Lumen program can be lexed, parsed to an AST,
/// type-checked, and lowered to LIR without any crash or error.
///
/// This is the "no crash" contract: as long as the source is syntactically and
/// semantically valid, the entire compile pipeline must complete successfully.
#[test]
fn parse_roundtrip_no_crash() {
    let source = r#"
cell add(a: Int, b: Int) -> Int
  return a + b
end

cell main() -> Int
  let result = add(3, 4)
  return result
end
"#;

    // Wrap in markdown (the canonical .lm.md format).
    let md = format!("# parse-roundtrip-test\n\n```lumen\n{}\n```\n", source.trim());

    let module = lumen_compiler::compile(&md)
        .expect("valid source should compile without errors");

    // The compiled module must contain both declared cells.
    let cell_names: Vec<&str> = module.cells.iter().map(|c| c.name.as_str()).collect();
    assert!(
        cell_names.contains(&"main"),
        "compiled module should contain 'main' cell, got: {:?}",
        cell_names
    );
    assert!(
        cell_names.contains(&"add"),
        "compiled module should contain 'add' cell, got: {:?}",
        cell_names
    );
}

/// Verify that a parse error is returned as a `CompileError` rather than a panic.
/// The compiler must never panic on malformed input.
#[test]
fn parse_invalid_source_returns_error_not_panic() {
    // Deliberately invalid Lumen — missing `end` keyword.
    let source = "# bad\n\n```lumen\ncell main() -> Int\n  return 1\n```\n";

    let result = lumen_compiler::compile(source);
    // Must be an Err — and crucially must NOT panic.
    assert!(
        result.is_err(),
        "invalid source should produce a CompileError, not Ok"
    );
}

// ── Test 2: VM execution ─────────────────────────────────────────────────────

/// Verify end-to-end execution: compile a simple .lm.md source string and
/// run it in the VM, checking that the result is correct.
///
/// This covers the full pipeline: markdown extraction → lex → parse → lower →
/// VM load → VM execute.
#[test]
fn vm_executes_simple_lmmd_file() {
    let source = r#"# vm-execution-test

```lumen
cell fibonacci(n: Int) -> Int
  if n <= 1
    return n
  end
  return fibonacci(n - 1) + fibonacci(n - 2)
end

cell main() -> Int
  return fibonacci(10)
end
```
"#;

    let module = lumen_compiler::compile(source)
        .expect("fibonacci source should compile");

    let mut vm = lumen_vm::vm::VM::new();
    vm.load(module);

    let result = vm.execute("main", vec![])
        .expect("main should execute without VM error");

    assert_eq!(
        result,
        lumen_vm::values::Value::Int(55),
        "fibonacci(10) should be 55, got {:?}",
        result
    );
}

/// Verify that the VM returns an error (not a panic) when asked to execute
/// a cell that does not exist in the module.
#[test]
fn vm_missing_cell_returns_error_not_panic() {
    let source = "# missing-cell-test\n\n```lumen\ncell main() -> Int\n  return 42\nend\n```\n";

    let module = lumen_compiler::compile(source)
        .expect("source should compile");

    let mut vm = lumen_vm::vm::VM::new();
    vm.load(module);

    let result = vm.execute("nonexistent_cell", vec![]);
    assert!(
        result.is_err(),
        "executing a missing cell should return an error, not panic"
    );
}

// ── Test 3: CLI exit codes ────────────────────────────────────────────────────

/// Verify that `lumen check <valid-file>` exits with code 0.
///
/// Skipped if the `lumen` binary is not found in the build output (e.g. in
/// a plain `cargo test -p lumen-compiler` run without a prior `cargo build`).
#[test]
fn cli_exits_0_on_valid_input() {
    let Some(lumen_bin) = find_lumen_binary() else {
        eprintln!("SKIP: lumen binary not found — run `cargo build` first");
        return;
    };

    // Write a valid Lumen source file to a temp location.
    let tmp = std::env::temp_dir().join("lumen_ci_test_valid.lm.md");
    std::fs::write(
        &tmp,
        "# valid\n\n```lumen\ncell main() -> Int\n  return 0\nend\n```\n",
    )
    .expect("should be able to write temp file");

    let status = std::process::Command::new(&lumen_bin)
        .args(["check", tmp.to_str().unwrap()])
        .status()
        .expect("failed to spawn lumen");

    let _ = std::fs::remove_file(&tmp);

    assert_eq!(
        status.code(),
        Some(0),
        "lumen check on valid input should exit 0"
    );
}

/// Verify that `lumen check <invalid-file>` exits with code 1.
///
/// Same skip logic as above.
#[test]
fn cli_exits_1_on_invalid_input() {
    let Some(lumen_bin) = find_lumen_binary() else {
        eprintln!("SKIP: lumen binary not found — run `cargo build` first");
        return;
    };

    // Write intentionally invalid Lumen source (missing `end`).
    let tmp = std::env::temp_dir().join("lumen_ci_test_invalid.lm.md");
    std::fs::write(
        &tmp,
        "# invalid\n\n```lumen\ncell main() -> Int\n  return 1\n```\n",
    )
    .expect("should be able to write temp file");

    let status = std::process::Command::new(&lumen_bin)
        .args(["check", tmp.to_str().unwrap()])
        .status()
        .expect("failed to spawn lumen");

    let _ = std::fs::remove_file(&tmp);

    assert_eq!(
        status.code(),
        Some(1),
        "lumen check on invalid input should exit 1"
    );
}

/// Verify that `lumen check <nonexistent-file>` exits with code 1.
#[test]
fn cli_exits_1_on_missing_file() {
    let Some(lumen_bin) = find_lumen_binary() else {
        eprintln!("SKIP: lumen binary not found — run `cargo build` first");
        return;
    };

    let status = std::process::Command::new(&lumen_bin)
        .args(["check", "/tmp/this_file_does_not_exist_lumen_ci.lm.md"])
        .status()
        .expect("failed to spawn lumen");

    assert_eq!(
        status.code(),
        Some(1),
        "lumen check on missing file should exit 1"
    );
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Locate the `lumen` binary from the Cargo build output directory.
///
/// Checks both debug and release profiles. Returns `None` if neither exists
/// so callers can skip instead of fail.
fn find_lumen_binary() -> Option<std::path::PathBuf> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Workspace root is three levels up from lumen-compiler/
    let workspace_root = manifest_dir
        .parent()  // rust/
        .and_then(|p| p.parent())?;  // workspace root

    let bin_name = if cfg!(windows) { "lumen.exe" } else { "lumen" };

    for profile in &["debug", "release"] {
        let candidate = workspace_root
            .join("target")
            .join(profile)
            .join(bin_name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}
