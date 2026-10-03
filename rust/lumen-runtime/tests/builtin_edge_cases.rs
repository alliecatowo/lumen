//! Negative / oversized arguments to list and string builtins must not panic,
//! and the by-name and by-id dispatch paths of `pad_left`/`pad_right` agree.

use lumen_runtime::vm::vm::VM;

fn eval(expr: &str) -> String {
    let src = format!("cell main() -> Any\n  return {expr}\nend\n");
    let module = lumen_compiler::compile_raw(&src).unwrap_or_else(|e| panic!("{expr}: {e}"));
    let mut vm = VM::new();
    vm.load(module);
    match vm.execute("main", vec![]) {
        Ok(v) => format!("{v}"),
        Err(e) => format!("ERR {e}"),
    }
}

#[test]
fn negative_widths_and_counts_are_clamped() {
    assert_eq!(eval("pad_left(\"a\", -1)"), "a");
    assert_eq!(eval("pad_right(\"a\", -5)"), "a");
    assert_eq!(eval("pad_left(\"a\", -1, \"*\")"), "a");
    assert_eq!(eval("take([1, 2, 3], -1)"), "[]");
    assert_eq!(eval("drop([1, 2, 3], -1)"), "[1, 2, 3]");
    assert_eq!(eval("chunk([1, 2, 3], -2)"), "[[1], [2], [3]]");
    assert_eq!(eval("window([1, 2, 3], -1)"), "[]");
}

#[test]
fn pad_paths_agree_and_count_characters() {
    // two-argument form (intrinsic id) and three-argument form (by name)
    assert_eq!(eval("pad_left(\"ab\", 4)"), "  ab");
    assert_eq!(eval("pad_left(\"ab\", 4, \"*\")"), "**ab");
    assert_eq!(eval("pad_right(\"ab\", 4, \"*\")"), "ab**");
    // width counts characters, not bytes
    assert_eq!(eval("pad_left(\"é\", 3)"), "  é");
    assert_eq!(eval("pad_left(\"é\", 3, \"-\")"), "--é");
    assert_eq!(eval("pad_right(\"日本\", 4, \".\")"), "日本..");
}

#[test]
fn absurd_pad_width_is_an_error_not_an_abort() {
    let r = eval("pad_left(\"a\", 9000000000000)");
    assert!(r.starts_with("ERR") && r.contains("pad width"), "{r}");
}
