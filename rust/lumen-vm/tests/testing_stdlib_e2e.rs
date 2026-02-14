use std::fs;
use std::path::PathBuf;

use lumen_compiler::{compile_raw_with_imports, compile_with_imports};
use lumen_vm::values::Value;
use lumen_vm::vm::VM;

fn std_testing_module_source() -> String {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let testing_path = manifest_dir.join("../../stdlib/std/testing.lm.md");
    fs::read_to_string(&testing_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", testing_path.display(), e))
}

fn resolve_std_testing(module: &str, testing_source: &str) -> Option<String> {
    if module == "std.testing" {
        Some(testing_source.to_string())
    } else {
        None
    }
}

fn run_markdown_main_with_std_imports(source: &str) -> Value {
    let testing_source = std_testing_module_source();
    let module = compile_with_imports(source, &|module| resolve_std_testing(module, &testing_source))
        .expect("markdown source should compile with std.testing");
    let mut vm = VM::new();
    vm.load(module);
    vm.execute("main", vec![]).expect("main should execute")
}

fn run_raw_main_with_std_imports(source: &str) -> Value {
    let testing_source = std_testing_module_source();
    let module =
        compile_raw_with_imports(source, &|module| resolve_std_testing(module, &testing_source))
            .expect("raw source should compile with std.testing");
    let mut vm = VM::new();
    vm.load(module);
    vm.execute("main", vec![]).expect("main should execute")
}

#[test]
fn e2e_testing_helpers_pass_in_markdown_program() {
    let source = r#"
# std-testing-md

```lumen
import std.testing: create_test_suite, add_test, assert_eq, assert_true, assert_contains, assert_not_contains, run_tests

cell main() -> Bool / {emit}
  let tests = create_test_suite()
  tests = add_test(tests, assert_eq(10 + 5, 15, "math works"))
  tests = add_test(tests, assert_true(3 < 7, "comparison works"))
  tests = add_test(tests, assert_contains([5, 6, 7], 6, "value exists"))
  tests = add_test(tests, assert_not_contains([1, 2, 3], 9, "value not present"))
  let summary = run_tests(tests)
  return summary.all_passed and summary.total == 4
end
```
"#;

    let result = run_markdown_main_with_std_imports(source);
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn e2e_testing_helpers_report_failure_in_raw_program() {
    let source = r#"
import std.testing: create_test_suite, add_test, assert_eq, summarize_tests

cell main() -> Bool
  let tests = create_test_suite()
  tests = add_test(tests, assert_eq(1, 2, "intentional failure"))
  let summary = summarize_tests(tests)
  return summary.all_passed
end
"#;

    let result = run_raw_main_with_std_imports(source);
    assert_eq!(result, Value::Bool(false));
}
