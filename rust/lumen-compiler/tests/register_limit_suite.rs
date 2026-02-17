//! T124 — Register limit tests
//!
//! Verifies that exceeding the 255 register limit produces a clear
//! `CompileError::Lower` error instead of a panic.

use lumen_compiler::{compile, CompileError};

/// Wrap raw Lumen source in markdown for the compiler.
fn md(source: &str) -> String {
    format!("# test\n\n```lumen\n{}\n```\n", source.trim())
}

#[test]
fn t124_too_many_variables_produces_lower_error() {
    // Generate a cell with 260 distinct let-bindings.
    // Each one consumes a register; this exceeds the 255 limit.
    let mut body = String::new();
    for i in 0..260 {
        body.push_str(&format!("  let var_{} = {}\n", i, i));
    }
    body.push_str("  var_0\n");
    let src = format!("cell main() -> Int\n{body}end\n");
    let result = compile(&md(&src));
    match result {
        Err(CompileError::Lower(msg)) => {
            assert!(
                msg.contains("register") || msg.contains("Register"),
                "Lower error should mention registers, got: {}",
                msg
            );
        }
        Err(other) => {
            // Also acceptable if the error is wrapped differently, but it should NOT be a panic
            let msg = format!("{:?}", other);
            assert!(
                msg.contains("register") || msg.contains("Register") || msg.contains("Lower"),
                "expected register limit error, got: {}",
                msg
            );
        }
        Ok(_) => panic!("expected compilation error for 260 variables, but compilation succeeded"),
    }
}

#[test]
fn t124_normal_compilation_unaffected() {
    // A normal cell should compile fine
    let src = r#"
cell main() -> Int
  let a = 1
  let b = 2
  let c = a + b
  c
end
"#;
    let result = compile(&md(src));
    assert!(
        result.is_ok(),
        "normal cell should compile: {:?}",
        result.err()
    );
}

#[test]
fn t124_error_message_is_descriptive() {
    // The error message should mention the cell name and the register limit
    let mut body = String::new();
    for i in 0..260 {
        body.push_str(&format!("  let v_{} = {}\n", i, i));
    }
    body.push_str("  v_0\n");
    let src = format!("cell my_big_cell() -> Int\n{body}end\n");
    let result = compile(&md(&src));
    match result {
        Err(CompileError::Lower(msg)) => {
            assert!(
                msg.contains("my_big_cell"),
                "error should name the offending cell 'my_big_cell', got: {}",
                msg
            );
            assert!(
                msg.contains("255"),
                "error should mention the 255 register limit, got: {}",
                msg
            );
        }
        Err(other) => {
            let msg = format!("{:?}", other);
            assert!(
                msg.contains("my_big_cell") || msg.contains("255"),
                "expected descriptive error, got: {}",
                msg
            );
        }
        Ok(_) => panic!("expected compilation error, but compilation succeeded"),
    }
}

#[test]
fn t124_lower_safe_does_not_panic() {
    // The key property: even with a huge cell, the compiler should NOT panic.
    // It should return an Err, not crash the process.
    let mut body = String::new();
    for i in 0..300 {
        body.push_str(&format!("  let reg_{} = {}\n", i, i));
    }
    body.push_str("  reg_0\n");
    let src = format!("cell main() -> Int\n{body}end\n");
    // If lower_safe is working, this returns Err, not a panic
    let result = compile(&md(&src));
    assert!(
        result.is_err(),
        "compilation of 300-variable cell should fail, not succeed"
    );
}

#[test]
fn t124_moderate_cell_compiles() {
    // A cell with ~50 variables should be fine (well within 255 limit)
    let mut body = String::new();
    for i in 0..50 {
        body.push_str(&format!("  let x_{} = {}\n", i, i));
    }
    body.push_str("  x_0 + x_49\n");
    let src = format!("cell main() -> Int\n{body}end\n");
    let result = compile(&md(&src));
    assert!(
        result.is_ok(),
        "50-variable cell should compile fine: {:?}",
        result.err()
    );
}
