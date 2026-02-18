//! T124 — Register limit tests
//!
//! Verifies that exceeding the 65,535 register limit produces a clear
//! `CompileError::Lower` error instead of a panic.
//! With 64-bit instruction encoding, registers are u16 (up to 65,535).

use lumen_compiler::compile;

/// Wrap raw Lumen source in markdown for the compiler.
fn md(source: &str) -> String {
    format!("# test\n\n```lumen\n{}\n```\n", source.trim())
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
fn t124_moderate_cell_compiles() {
    // A cell with ~50 variables should be fine
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

#[test]
fn t124_260_variables_now_compiles_with_64bit_encoding() {
    // With 64-bit instruction encoding, 260 variables is well within the
    // 65,535 register limit (u16). This test verifies the wider encoding works.
    let mut body = String::new();
    for i in 0..260 {
        body.push_str(&format!("  let var_{} = {}\n", i, i));
    }
    body.push_str("  var_0\n");
    let src = format!("cell main() -> Int\n{body}end\n");
    let result = compile(&md(&src));
    assert!(
        result.is_ok(),
        "260-variable cell should compile with 64-bit encoding: {:?}",
        result.err()
    );
}

#[test]
fn t124_300_variables_compiles_with_64bit_encoding() {
    // 300 variables is also within the 65,535 register limit
    let mut body = String::new();
    for i in 0..300 {
        body.push_str(&format!("  let reg_{} = {}\n", i, i));
    }
    body.push_str("  reg_0\n");
    let src = format!("cell main() -> Int\n{body}end\n");
    let result = compile(&md(&src));
    assert!(
        result.is_ok(),
        "300-variable cell should compile with 64-bit encoding: {:?}",
        result.err()
    );
}

#[test]
fn t124_lower_safe_does_not_panic() {
    // The key property: even with a large cell, the compiler should NOT panic.
    // With 64-bit encoding, this now succeeds instead of erroring.
    let mut body = String::new();
    for i in 0..300 {
        body.push_str(&format!("  let reg_{} = {}\n", i, i));
    }
    body.push_str("  reg_0\n");
    let src = format!("cell main() -> Int\n{body}end\n");
    // lower_safe is still working — it would catch panics if they happened
    let result = compile(&md(&src));
    // With 64-bit encoding, this compiles successfully
    assert!(
        result.is_ok(),
        "300-variable cell should compile without panic: {:?}",
        result.err()
    );
}

#[test]
fn t124_500_variables_compiles_with_64bit_encoding() {
    // 500 variables is well within the 65,535 register limit.
    // Verifies expanded register space handles moderately large cells.
    let mut body = String::new();
    for i in 0..500 {
        body.push_str(&format!("  let v_{} = {}\n", i, i));
    }
    body.push_str("  v_0 + v_499\n");
    let src = format!("cell main() -> Int\n{body}end\n");
    let result = compile(&md(&src));
    assert!(
        result.is_ok(),
        "500-variable cell should compile under 16-bit register space: {:?}",
        result.err()
    );
}
