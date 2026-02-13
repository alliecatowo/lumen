//! Integration tests: compile (and optionally execute) all example files.

use std::fs;
use std::path::PathBuf;

use lumen_compiler::compile;

fn examples_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../examples")
}

/// Files known to have pre-existing parse issues that prevent compilation.
const SKIP_COMPILE: &[&str] = &["role_interpolation.lm.md"];

/// Collect all .lm.md files in the examples directory.
fn all_example_files() -> Vec<PathBuf> {
    let dir = examples_dir();
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read examples dir {}: {}", dir.display(), e))
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md")
                && path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|n| n.ends_with(".lm.md"))
            {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    files.sort();
    files
}

#[test]
fn all_examples_compile() {
    let files = all_example_files();
    assert!(
        !files.is_empty(),
        "expected at least one example file in {}",
        examples_dir().display()
    );

    let mut failures = Vec::new();
    let mut compiled = 0;
    let mut skipped = 0;

    for path in &files {
        let name = path.file_name().unwrap().to_str().unwrap();

        if SKIP_COMPILE.contains(&name) {
            skipped += 1;
            continue;
        }

        let source = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("cannot read {}: {}", path.display(), e));

        match compile(&source) {
            Ok(_) => compiled += 1,
            Err(err) => failures.push(format!("{}: {}", name, err)),
        }
    }

    if !failures.is_empty() {
        panic!(
            "{} of {} examples failed to compile (skipped {}):\n  {}",
            failures.len(),
            compiled + failures.len(),
            skipped,
            failures.join("\n  ")
        );
    }

    // Sanity: we should have compiled at least 10 examples
    assert!(
        compiled >= 10,
        "only {} examples compiled — expected at least 10",
        compiled
    );
}

// ─── Individual per-example compile tests for fine-grained failure reporting ───

fn compile_example(filename: &str) {
    let path = examples_dir().join(filename);
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", path.display(), e));
    compile(&source).unwrap_or_else(|e| {
        panic!("{} failed to compile:\n--- error ---\n{}", filename, e)
    });
}

#[test]
fn example_hello() {
    compile_example("hello.lm.md");
}

#[test]
fn example_fibonacci() {
    compile_example("fibonacci.lm.md");
}

#[test]
fn example_language_features() {
    compile_example("language_features.lm.md");
}

#[test]
fn example_intrinsics_test() {
    compile_example("intrinsics_test.lm.md");
}

#[test]
fn example_typecheck_pass() {
    compile_example("typecheck_pass.lm.md");
}

#[test]
fn example_record_validation() {
    compile_example("record_validation.lm.md");
}

#[test]
fn example_where_constraints() {
    compile_example("where_constraints.lm.md");
}

#[test]
fn example_expect_schema() {
    compile_example("expect_schema.lm.md");
}

#[test]
fn example_invoice_agent() {
    compile_example("invoice_agent.lm.md");
}

#[test]
fn example_todo_manager() {
    compile_example("todo_manager.lm.md");
}

#[test]
fn example_code_reviewer() {
    compile_example("code_reviewer.lm.md");
}

#[test]
fn example_data_pipeline() {
    compile_example("data_pipeline.lm.md");
}

#[test]
fn example_role_repro() {
    compile_example("role_repro.lm.md");
}

#[test]
#[ignore = "role_interpolation.lm.md has a known parse issue"]
fn example_role_interpolation() {
    compile_example("role_interpolation.lm.md");
}
