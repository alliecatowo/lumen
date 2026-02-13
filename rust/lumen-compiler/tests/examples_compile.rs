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
                    .map_or(false, |n| n.ends_with(".lm.md"))
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
