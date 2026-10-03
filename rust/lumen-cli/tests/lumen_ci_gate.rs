//! `lumen ci` is a quality gate: strict lint findings must fail it.

mod common;

use common::TempDir;
use std::process::Command;

#[test]
fn ci_fails_when_strict_lint_reports_violations() {
    let tmp = TempDir::new("cigate");
    let file = tmp.path().join("redundant.lm");
    // `return` as the last statement of a cell is a redundant-return finding.
    std::fs::write(&file, "cell main() -> Int\n  return 1\nend\n").unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_lumen"))
        .arg("ci")
        .arg(&file)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "ci passed despite lint findings:\n{stdout}\n{stderr}"
    );
    assert!(!stdout.contains("lint passed"), "{stdout}");
    assert!(stderr.contains("lint failed"), "{stderr}");
}
