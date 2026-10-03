//! `lumen fmt` must never change what a program means: over every example it has
//! to be idempotent and keep type-checking programs type-checking.

use std::path::Path;

#[test]
fn formatting_the_examples_is_idempotent_and_keeps_them_valid() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("examples dir") {
        let path = entry.unwrap().path();
        if !path.to_string_lossy().ends_with(".lm.md") {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        if lumen_compiler::compile(&source).is_err() {
            continue; // only programs that were valid to begin with
        }
        let once = lumen_cli::fmt::format_file(&source);
        let twice = lumen_cli::fmt::format_file(&once);
        assert_eq!(once, twice, "{} is not idempotent", path.display());
        assert!(
            lumen_compiler::compile(&once).is_ok(),
            "formatting broke {}",
            path.display()
        );
        checked += 1;
    }
    assert!(checked >= 20, "only {checked} examples were checked");
}
