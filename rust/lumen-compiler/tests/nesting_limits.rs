//! Deeply nested or very long expressions must produce a clean compile error
//! (not abort the process with a stack overflow), and ordinary deep-but-sane
//! programs must compile even when the caller's thread has a small stack.

use lumen_compiler::compile_raw;

fn program(expr: &str) -> String {
    format!("cell main() -> Any\n  return {expr}\nend\n")
}

fn err_text(src: &str) -> String {
    match compile_raw(src) {
        Ok(_) => panic!("expected a compile error"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn deeply_nested_parentheses_are_a_clean_error() {
    let src = program(&format!("{}1{}", "(".repeat(5000), ")".repeat(5000)));
    let e = err_text(&src);
    assert!(e.contains("too deep"), "{e}");
}

#[test]
fn deeply_nested_lists_are_a_clean_error() {
    let src = program(&format!("{}1{}", "[".repeat(5000), "]".repeat(5000)));
    assert!(err_text(&src).contains("too deep"));
}

#[test]
fn very_long_operator_chain_is_a_clean_error() {
    let chain = vec!["1"; 4000].join(" + ");
    assert!(err_text(&program(&chain)).contains("too"));
}

#[test]
fn deeply_nested_blocks_and_types_are_a_clean_error() {
    let mut src = String::from("cell main() -> Int\n");
    for i in 0..400 {
        src.push_str(&format!("{}if true\n", "  ".repeat(i + 1)));
    }
    src.push_str(&format!("{}return 1\n", "  ".repeat(401)));
    for i in (0..400).rev() {
        src.push_str(&format!("{}end\n", "  ".repeat(i + 1)));
    }
    src.push_str("end\n");
    let e = err_text(&src);
    assert!(e.contains("too deep"), "{}", &e[..e.len().min(600)]);

    let ty = format!("{}Int{}", "list[".repeat(600), "]".repeat(600));
    let src = format!("cell f(x: {ty}) -> Int\n  return 1\nend\n");
    assert!(err_text(&src).contains("too deep"));
}

#[test]
fn reasonable_nesting_compiles_on_a_small_stack() {
    // Runs the compiler from a 256 KiB thread: the entry point moves the work
    // onto its own large stack.
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let src = program(&format!("{}1{}", "(".repeat(120), ")".repeat(120)));
            compile_raw(&src).map(|_| ()).map_err(|e| e.to_string())
        })
        .unwrap();
    handle.join().unwrap().expect("120 nested parens compile");
}
