use std::fs;
use std::path::PathBuf;

use lumen_compiler::{compile_raw_with_imports, compile_with_imports};

fn std_testing_module_source() -> String {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let testing_path = manifest_dir.join("../../stdlib/std/testing.lm.md");
    fs::read_to_string(&testing_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", testing_path.display(), e))
}

fn resolver(module: &str, testing_source: &str) -> Option<String> {
    if module == "std.testing" {
        Some(testing_source.to_string())
    } else {
        None
    }
}

#[test]
fn testing_helpers_compile_in_markdown_program() {
    let testing_source = std_testing_module_source();
    let source = r#"
# testing-import-md

```lumen
import std.testing: create_test_suite, add_test, assert_eq, assert_true, assert_contains, assert_not_contains, assert_length, summarize_tests, all_passed

cell main() -> Bool
  let tests = create_test_suite()
  tests = add_test(tests, assert_eq(2 + 2, 4, "2 + 2 == 4"))
  tests = add_test(tests, assert_true(3 < 5, "3 < 5"))
  tests = add_test(tests, assert_contains([1, 2, 3], 2, "contains list value"))
  tests = add_test(tests, assert_not_contains([1, 2, 3], 9, "value is absent"))
  tests = add_test(tests, assert_length([10, 20, 30], 3, "list length is 3"))
  let summary = summarize_tests(tests)
  return all_passed(tests) and summary.all_passed
end
```
"#;

    let module = compile_with_imports(source, &|module| resolver(module, &testing_source))
        .expect("markdown program should compile with std.testing imports");

    assert!(
        module.cells.iter().any(|c| c.name == "main"),
        "expected main cell in compiled module"
    );
    assert!(
        module.cells.iter().any(|c| c.name == "assert_not_contains"),
        "expected imported std.testing helper cells in compiled module"
    );
}

#[test]
fn testing_helpers_compile_in_raw_program() {
    let testing_source = std_testing_module_source();
    let source = r#"
import std.testing: create_test_suite, add_test, assert_false, assert_not_empty, assert_empty, summarize_tests

cell main() -> Bool
  let tests = create_test_suite()
  tests = add_test(tests, assert_false(false, "false is false"))
  tests = add_test(tests, assert_not_empty([1], "list is not empty"))
  tests = add_test(tests, assert_empty([], "empty list is empty"))
  let summary = summarize_tests(tests)
  return summary.all_passed and summary.failed == 0
end
"#;

    let module = compile_raw_with_imports(source, &|module| resolver(module, &testing_source))
        .expect("raw program should compile with std.testing imports");

    assert!(
        module.cells.iter().any(|c| c.name == "main"),
        "expected main cell in compiled module"
    );
    assert!(
        module.cells.iter().any(|c| c.name == "summarize_tests"),
        "expected imported summary helpers in compiled module"
    );
}
