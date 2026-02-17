//! Tests for T179: Docs-as-tests — extraction and validation of code blocks from documentation.

use lumen_compiler::compiler::docs_as_tests::*;

// ════════════════════════════════════════════════════════════════════
// §1  DocDirective parsing
// ════════════════════════════════════════════════════════════════════

#[test]
fn docs_directive_empty_is_compile_ok() {
    assert_eq!(DocExtractor::parse_directive(""), DocDirective::CompileOk);
}

#[test]
fn docs_directive_compile_ok_explicit() {
    assert_eq!(
        DocExtractor::parse_directive("compile-ok"),
        DocDirective::CompileOk
    );
}

#[test]
fn docs_directive_compile_error_with_msg() {
    assert_eq!(
        DocExtractor::parse_directive("compile-error(TypeMismatch)"),
        DocDirective::CompileError("TypeMismatch".to_string())
    );
}

#[test]
fn docs_directive_compile_error_no_parens() {
    assert_eq!(
        DocExtractor::parse_directive("compile-error"),
        DocDirective::CompileError(String::new())
    );
}

#[test]
fn docs_directive_run_ok() {
    assert_eq!(DocExtractor::parse_directive("run-ok"), DocDirective::RunOk);
}

#[test]
fn docs_directive_skip() {
    assert_eq!(DocExtractor::parse_directive("skip"), DocDirective::Skip);
}

#[test]
fn docs_directive_no_test() {
    assert_eq!(
        DocExtractor::parse_directive("no-test"),
        DocDirective::NoTest
    );
}

#[test]
fn docs_directive_unknown_defaults_to_compile_ok() {
    assert_eq!(
        DocExtractor::parse_directive("unknown-thing"),
        DocDirective::CompileOk
    );
}

// ════════════════════════════════════════════════════════════════════
// §2  DocExtractor — block extraction
// ════════════════════════════════════════════════════════════════════

#[test]
fn docs_extract_single_lumen_block() {
    let md = r#"# Test

```lumen
cell main() -> Int
  return 42
end
```
"#;
    let blocks = DocExtractor::extract_blocks(md, "test.md");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].language, "lumen");
    assert_eq!(blocks[0].directive, DocDirective::CompileOk);
    assert!(blocks[0].source.contains("cell main"));
    assert_eq!(blocks[0].file, "test.md");
}

#[test]
fn docs_extract_lm_language_tag() {
    let md = r#"```lm
cell test() -> Int
  return 1
end
```
"#;
    let blocks = DocExtractor::extract_blocks(md, "test.md");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].language, "lm");
}

#[test]
fn docs_extract_multiple_blocks() {
    let md = r#"# Docs

```lumen
cell a() -> Int
  return 1
end
```

Some prose.

```lumen
cell b() -> Int
  return 2
end
```
"#;
    let blocks = DocExtractor::extract_blocks(md, "test.md");
    assert_eq!(blocks.len(), 2);
    assert!(blocks[0].source.contains("cell a"));
    assert!(blocks[1].source.contains("cell b"));
}

#[test]
fn docs_extract_ignores_non_lumen_blocks() {
    let md = r#"```python
print("hello")
```

```lumen
cell test() -> Int
  return 42
end
```

```rust
fn main() {}
```
"#;
    let blocks = DocExtractor::extract_blocks(md, "test.md");
    assert_eq!(blocks.len(), 1);
    assert!(blocks[0].source.contains("cell test"));
}

#[test]
fn docs_extract_directive_in_info_string() {
    let md = r#"```lumen compile-error(TypeMismatch)
cell bad() -> Int
  return "not an int"
end
```
"#;
    let blocks = DocExtractor::extract_blocks(md, "test.md");
    assert_eq!(blocks.len(), 1);
    assert_eq!(
        blocks[0].directive,
        DocDirective::CompileError("TypeMismatch".to_string())
    );
}

#[test]
fn docs_extract_skip_directive() {
    let md = r#"```lumen skip
cell incomplete()
  # work in progress
end
```
"#;
    let blocks = DocExtractor::extract_blocks(md, "test.md");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].directive, DocDirective::Skip);
}

#[test]
fn docs_extract_no_test_directive() {
    let md = r#"```lumen no-test
# This is pseudo-code
do_something_magical()
```
"#;
    let blocks = DocExtractor::extract_blocks(md, "test.md");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].directive, DocDirective::NoTest);
}

#[test]
fn docs_extract_run_ok_directive() {
    let md = r#"```lumen run-ok
cell main() -> Int
  return 42
end
```
"#;
    let blocks = DocExtractor::extract_blocks(md, "test.md");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].directive, DocDirective::RunOk);
}

#[test]
fn docs_extract_empty_code_block() {
    let md = "```lumen\n```\n";
    let blocks = DocExtractor::extract_blocks(md, "test.md");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].source, "");
}

#[test]
fn docs_extract_line_numbers() {
    let md = r#"Line 1
Line 2
Line 3

```lumen
cell test() -> Int
  return 42
end
```
"#;
    let blocks = DocExtractor::extract_blocks(md, "test.md");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].line_number, 6); // code starts on line 6
}

#[test]
fn docs_extract_no_blocks() {
    let md = "# Just a heading\n\nSome text.\n";
    let blocks = DocExtractor::extract_blocks(md, "test.md");
    assert!(blocks.is_empty());
}

#[test]
fn docs_extract_quad_backtick_fence() {
    let md = "````lumen\ncell test() -> Int\n  return 42\nend\n````\n";
    let blocks = DocExtractor::extract_blocks(md, "test.md");
    assert_eq!(blocks.len(), 1);
    assert!(blocks[0].source.contains("cell test"));
}

// ════════════════════════════════════════════════════════════════════
// §3  DocTestRunner — run_block
// ════════════════════════════════════════════════════════════════════

#[test]
fn docs_run_block_compile_ok_success() {
    let block = DocCodeBlock {
        source: "cell main() -> Int\n  return 42\nend".to_string(),
        language: "lumen".to_string(),
        line_number: 1,
        file: "test.md".to_string(),
        directive: DocDirective::CompileOk,
    };
    let result = DocTestRunner::run_block(&block);
    assert!(result.passed, "Should pass: {}", result.message);
}

#[test]
fn docs_run_block_compile_ok_failure() {
    let block = DocCodeBlock {
        source: "cell main( -> Int\n  return 42\nend".to_string(),
        language: "lumen".to_string(),
        line_number: 1,
        file: "test.md".to_string(),
        directive: DocDirective::CompileOk,
    };
    let result = DocTestRunner::run_block(&block);
    assert!(!result.passed, "Should fail on parse error");
}

#[test]
fn docs_run_block_compile_error_expected_match() {
    let block = DocCodeBlock {
        source: "cell main( -> Int\n  return 42\nend".to_string(),
        language: "lumen".to_string(),
        line_number: 1,
        file: "test.md".to_string(),
        directive: DocDirective::CompileError("parse".to_string()),
    };
    let result = DocTestRunner::run_block(&block);
    assert!(
        result.passed,
        "Should pass when expected error found: {}",
        result.message
    );
}

#[test]
fn docs_run_block_compile_error_unexpected_success() {
    let block = DocCodeBlock {
        source: "cell main() -> Int\n  return 42\nend".to_string(),
        language: "lumen".to_string(),
        line_number: 1,
        file: "test.md".to_string(),
        directive: DocDirective::CompileError("TypeMismatch".to_string()),
    };
    let result = DocTestRunner::run_block(&block);
    assert!(
        !result.passed,
        "Should fail when compile succeeds but error was expected"
    );
}

#[test]
fn docs_run_block_compile_error_wrong_message() {
    let block = DocCodeBlock {
        source: "cell main( -> Int\n  return 42\nend".to_string(),
        language: "lumen".to_string(),
        line_number: 1,
        file: "test.md".to_string(),
        directive: DocDirective::CompileError("ZZZ_NONEXISTENT_ERROR_ZZZ".to_string()),
    };
    let result = DocTestRunner::run_block(&block);
    assert!(
        !result.passed,
        "Should fail when error message doesn't match"
    );
}

#[test]
fn docs_run_block_compile_error_any() {
    let block = DocCodeBlock {
        source: "cell main( -> Int\n  return 42\nend".to_string(),
        language: "lumen".to_string(),
        line_number: 1,
        file: "test.md".to_string(),
        directive: DocDirective::CompileError(String::new()),
    };
    let result = DocTestRunner::run_block(&block);
    assert!(
        result.passed,
        "Empty error match should accept any error: {}",
        result.message
    );
}

#[test]
fn docs_run_block_skip() {
    let block = DocCodeBlock {
        source: "this is not valid code at all!!!".to_string(),
        language: "lumen".to_string(),
        line_number: 1,
        file: "test.md".to_string(),
        directive: DocDirective::Skip,
    };
    let result = DocTestRunner::run_block(&block);
    assert!(result.passed, "Skip blocks should always pass");
    assert_eq!(result.message, "Skipped");
}

#[test]
fn docs_run_block_no_test() {
    let block = DocCodeBlock {
        source: "pseudo code here".to_string(),
        language: "lumen".to_string(),
        line_number: 1,
        file: "test.md".to_string(),
        directive: DocDirective::NoTest,
    };
    let result = DocTestRunner::run_block(&block);
    assert!(result.passed, "NoTest blocks should always pass");
}

#[test]
fn docs_run_block_run_ok_success() {
    let block = DocCodeBlock {
        source: "cell main() -> Int\n  return 42\nend".to_string(),
        language: "lumen".to_string(),
        line_number: 1,
        file: "test.md".to_string(),
        directive: DocDirective::RunOk,
    };
    let result = DocTestRunner::run_block(&block);
    assert!(
        result.passed,
        "RunOk with valid code should pass: {}",
        result.message
    );
}

// ════════════════════════════════════════════════════════════════════
// §4  DocTestRunner — run_all and summaries
// ════════════════════════════════════════════════════════════════════

#[test]
fn docs_run_all_mixed_results() {
    let blocks = vec![
        DocCodeBlock {
            source: "cell a() -> Int\n  return 1\nend".to_string(),
            language: "lumen".to_string(),
            line_number: 1,
            file: "test.md".to_string(),
            directive: DocDirective::CompileOk,
        },
        DocCodeBlock {
            source: "cell b( -> Int\n  return 2\nend".to_string(),
            language: "lumen".to_string(),
            line_number: 5,
            file: "test.md".to_string(),
            directive: DocDirective::CompileOk,
        },
        DocCodeBlock {
            source: "skipped code".to_string(),
            language: "lumen".to_string(),
            line_number: 10,
            file: "test.md".to_string(),
            directive: DocDirective::Skip,
        },
    ];

    let summary = DocTestRunner::run_all(&blocks);
    assert_eq!(summary.total, 3);
    assert_eq!(summary.passed, 1);
    assert_eq!(summary.failed, 1);
    assert_eq!(summary.skipped, 1);
    assert_eq!(summary.results.len(), 3);
}

#[test]
fn docs_run_all_empty() {
    let summary = DocTestRunner::run_all(&[]);
    assert_eq!(summary.total, 0);
    assert_eq!(summary.passed, 0);
    assert_eq!(summary.failed, 0);
    assert_eq!(summary.skipped, 0);
}

#[test]
fn docs_format_summary() {
    let summary = DocTestSummary {
        total: 10,
        passed: 7,
        failed: 2,
        skipped: 1,
        results: vec![],
    };
    let text = DocTestRunner::format_summary(&summary);
    assert!(text.contains("10 total"));
    assert!(text.contains("7 passed"));
    assert!(text.contains("2 failed"));
    assert!(text.contains("1 skipped"));
}

#[test]
fn docs_format_failures_includes_failing() {
    let blocks = vec![DocCodeBlock {
        source: "cell bad( -> Int\n  return 1\nend".to_string(),
        language: "lumen".to_string(),
        line_number: 5,
        file: "docs.md".to_string(),
        directive: DocDirective::CompileOk,
    }];
    let summary = DocTestRunner::run_all(&blocks);
    let text = DocTestRunner::format_failures(&summary);
    assert!(text.contains("FAIL"), "Should contain FAIL marker");
    assert!(text.contains("docs.md:5"), "Should show file and line");
}

#[test]
fn docs_format_failures_empty_when_all_pass() {
    let blocks = vec![DocCodeBlock {
        source: "cell ok() -> Int\n  return 1\nend".to_string(),
        language: "lumen".to_string(),
        line_number: 1,
        file: "test.md".to_string(),
        directive: DocDirective::CompileOk,
    }];
    let summary = DocTestRunner::run_all(&blocks);
    let text = DocTestRunner::format_failures(&summary);
    assert!(text.is_empty(), "No failures should produce empty output");
}

// ════════════════════════════════════════════════════════════════════
// §5  Integration: extract then run
// ════════════════════════════════════════════════════════════════════

#[test]
fn docs_end_to_end_compile_ok() {
    let md = r#"# API Guide

```lumen
cell greet(name: String) -> String
  return "Hello, " + name
end
```
"#;
    let blocks = DocExtractor::extract_blocks(md, "guide.md");
    let summary = DocTestRunner::run_all(&blocks);
    assert_eq!(summary.passed, 1);
    assert_eq!(summary.failed, 0);
}

#[test]
fn docs_end_to_end_compile_error() {
    let md = r#"# Error Example

```lumen compile-error(parse)
cell bad( -> Int
  return 1
end
```
"#;
    let blocks = DocExtractor::extract_blocks(md, "errors.md");
    assert_eq!(blocks.len(), 1);
    assert_eq!(
        blocks[0].directive,
        DocDirective::CompileError("parse".to_string())
    );
    let summary = DocTestRunner::run_all(&blocks);
    assert_eq!(summary.passed, 1);
    assert_eq!(summary.failed, 0);
}

#[test]
fn docs_end_to_end_mixed() {
    let md = r#"# Mixed Doc

Valid code:

```lumen
cell add(a: Int, b: Int) -> Int
  return a + b
end
```

Skip this:

```lumen skip
incomplete_stuff()
```

Error expected:

```lumen compile-error(parse)
cell broken( -> Int
end
```
"#;
    let blocks = DocExtractor::extract_blocks(md, "mixed.md");
    assert_eq!(blocks.len(), 3);
    let summary = DocTestRunner::run_all(&blocks);
    assert_eq!(summary.passed, 2, "Two compilable blocks should pass");
    assert_eq!(summary.skipped, 1, "One skip block");
    assert_eq!(summary.failed, 0, "No failures");
}

#[test]
fn docs_end_to_end_all_skipped() {
    let md = r#"```lumen skip
code1
```

```lumen no-test
code2
```
"#;
    let blocks = DocExtractor::extract_blocks(md, "test.md");
    let summary = DocTestRunner::run_all(&blocks);
    assert_eq!(summary.total, 2);
    assert_eq!(summary.skipped, 2);
    assert_eq!(summary.passed, 0);
    assert_eq!(summary.failed, 0);
}

#[test]
fn docs_duration_is_recorded() {
    let block = DocCodeBlock {
        source: "cell test() -> Int\n  return 1\nend".to_string(),
        language: "lumen".to_string(),
        line_number: 1,
        file: "test.md".to_string(),
        directive: DocDirective::CompileOk,
    };
    let result = DocTestRunner::run_block(&block);
    // Duration should be non-negative (may be 0 for fast compiles)
    assert!(result.duration_ms < 30000, "Should not take 30+ seconds");
}

#[test]
fn docs_block_clone_and_debug() {
    let block = DocCodeBlock {
        source: "test".to_string(),
        language: "lumen".to_string(),
        line_number: 1,
        file: "test.md".to_string(),
        directive: DocDirective::CompileOk,
    };
    let cloned = block.clone();
    assert_eq!(cloned.source, block.source);
    let debug = format!("{:?}", block);
    assert!(debug.contains("DocCodeBlock"));
}

#[test]
fn docs_result_clone_and_debug() {
    let result = DocTestResult {
        block: DocCodeBlock {
            source: "test".to_string(),
            language: "lumen".to_string(),
            line_number: 1,
            file: "test.md".to_string(),
            directive: DocDirective::CompileOk,
        },
        passed: true,
        message: "ok".to_string(),
        duration_ms: 5,
    };
    let cloned = result.clone();
    assert_eq!(cloned.passed, result.passed);
    let debug = format!("{:?}", result);
    assert!(debug.contains("DocTestResult"));
}

#[test]
fn docs_summary_clone_and_debug() {
    let summary = DocTestSummary {
        total: 1,
        passed: 1,
        failed: 0,
        skipped: 0,
        results: vec![],
    };
    let cloned = summary.clone();
    assert_eq!(cloned.total, 1);
    let debug = format!("{:?}", summary);
    assert!(debug.contains("DocTestSummary"));
}
