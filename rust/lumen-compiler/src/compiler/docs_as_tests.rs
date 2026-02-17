//! Docs-as-tests: extract and validate code blocks from documentation.
//!
//! Parses fenced Lumen code blocks from Markdown documentation and runs
//! them through the compiler to verify correctness.

use std::time::Instant;

// ════════════════════════════════════════════════════════════════════
// Types
// ════════════════════════════════════════════════════════════════════

/// Directive controlling how a code block is tested.
#[derive(Debug, Clone, PartialEq)]
pub enum DocDirective {
    /// Must compile without errors (default for `lumen` blocks).
    CompileOk,
    /// Must produce a compile error whose message contains the given string.
    CompileError(String),
    /// Must compile and run successfully.
    RunOk,
    /// Skip this block entirely.
    Skip,
    /// This block is not a test (e.g., pseudo-code).
    NoTest,
}

/// A code block extracted from a documentation file.
#[derive(Debug, Clone)]
pub struct DocCodeBlock {
    /// The source code inside the fenced block.
    pub source: String,
    /// Language tag (e.g., "lumen", "lm").
    pub language: String,
    /// Line number in the original document where the code starts.
    pub line_number: usize,
    /// Source file path.
    pub file: String,
    /// Directive controlling how the block should be tested.
    pub directive: DocDirective,
}

/// Result of running a single doc test block.
#[derive(Debug, Clone)]
pub struct DocTestResult {
    pub block: DocCodeBlock,
    pub passed: bool,
    pub message: String,
    pub duration_ms: u64,
}

/// Summary of running all doc tests.
#[derive(Debug, Clone)]
pub struct DocTestSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub results: Vec<DocTestResult>,
}

// ════════════════════════════════════════════════════════════════════
// DocExtractor
// ════════════════════════════════════════════════════════════════════

/// Extracts testable code blocks from documentation content.
pub struct DocExtractor;

impl DocExtractor {
    /// Extract all testable code blocks from content (auto-detects markdown).
    pub fn extract_blocks(content: &str, file: &str) -> Vec<DocCodeBlock> {
        Self::extract_from_markdown(content, file)
    }

    /// Extract code blocks from Markdown content.
    ///
    /// Finds fenced code blocks with `lumen` or `lm` language tags and
    /// parses directives from the info string.
    pub fn extract_from_markdown(content: &str, file: &str) -> Vec<DocCodeBlock> {
        let mut blocks = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let trimmed = lines[i].trim();
            if let Some(backtick_count) = count_leading_backticks(trimmed) {
                if backtick_count >= 3 {
                    let info_string = trimmed[backtick_count..].trim();
                    let (lang, directive) = Self::parse_info_string(info_string);

                    if lang == "lumen" || lang == "lm" {
                        // Collect block body
                        let code_start_line = i + 2; // 1-based, next line
                        let mut code_lines = Vec::new();
                        i += 1;

                        while i < lines.len() {
                            let inner_trimmed = lines[i].trim();
                            if let Some(close_count) = count_leading_backticks(inner_trimmed) {
                                if close_count >= backtick_count
                                    && inner_trimmed[close_count..].trim().is_empty()
                                {
                                    break;
                                }
                            }
                            code_lines.push(lines[i]);
                            i += 1;
                        }

                        let source = code_lines.join("\n");
                        blocks.push(DocCodeBlock {
                            source,
                            language: lang,
                            line_number: code_start_line,
                            file: file.to_string(),
                            directive,
                        });
                    }
                }
            }
            i += 1;
        }

        blocks
    }

    /// Parse the info string after the opening fence.
    ///
    /// Formats:
    /// - `lumen` → (lumen, CompileOk)
    /// - `lumen compile-ok` → (lumen, CompileOk)
    /// - `lumen compile-error(TypeMismatch)` → (lumen, CompileError("TypeMismatch"))
    /// - `lumen run-ok` → (lumen, RunOk)
    /// - `lumen skip` → (lumen, Skip)
    /// - `lumen no-test` → (lumen, NoTest)
    /// - `lm` → (lm, CompileOk)
    fn parse_info_string(info: &str) -> (String, DocDirective) {
        let parts: Vec<&str> = info.splitn(2, char::is_whitespace).collect();
        let lang = parts[0].to_lowercase();

        if lang != "lumen" && lang != "lm" {
            return (lang, DocDirective::NoTest);
        }

        let directive = if parts.len() > 1 {
            Self::parse_directive(parts[1].trim())
        } else {
            DocDirective::CompileOk
        };

        (lang, directive)
    }

    /// Parse a directive string from the code block info line.
    pub fn parse_directive(info_string: &str) -> DocDirective {
        let trimmed = info_string.trim();
        if trimmed.is_empty() || trimmed == "compile-ok" {
            DocDirective::CompileOk
        } else if let Some(rest) = trimmed.strip_prefix("compile-error") {
            let rest = rest.trim();
            if let Some(inner) = rest.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
                DocDirective::CompileError(inner.to_string())
            } else {
                // compile-error without parens: expect any compile error
                DocDirective::CompileError(String::new())
            }
        } else if trimmed == "run-ok" {
            DocDirective::RunOk
        } else if trimmed == "skip" {
            DocDirective::Skip
        } else if trimmed == "no-test" {
            DocDirective::NoTest
        } else {
            // Unknown directive, default to CompileOk
            DocDirective::CompileOk
        }
    }
}

/// Count leading backticks in a string, returning None if no backticks.
fn count_leading_backticks(s: &str) -> Option<usize> {
    let count = s.chars().take_while(|&c| c == '`').count();
    if count > 0 {
        Some(count)
    } else {
        None
    }
}

// ════════════════════════════════════════════════════════════════════
// DocTestRunner
// ════════════════════════════════════════════════════════════════════

/// Runs extracted doc code blocks through the compiler.
pub struct DocTestRunner;

impl DocTestRunner {
    /// Run a single doc code block and return the result.
    pub fn run_block(block: &DocCodeBlock) -> DocTestResult {
        let start = Instant::now();

        match &block.directive {
            DocDirective::Skip | DocDirective::NoTest => DocTestResult {
                block: block.clone(),
                passed: true,
                message: "Skipped".to_string(),
                duration_ms: 0,
            },
            DocDirective::CompileOk | DocDirective::RunOk => {
                let result = crate::compile_raw(&block.source);
                let elapsed = start.elapsed().as_millis() as u64;
                match result {
                    Ok(_) => DocTestResult {
                        block: block.clone(),
                        passed: true,
                        message: "Compiled successfully".to_string(),
                        duration_ms: elapsed,
                    },
                    Err(e) => DocTestResult {
                        block: block.clone(),
                        passed: false,
                        message: format!("Expected successful compilation, got: {e}"),
                        duration_ms: elapsed,
                    },
                }
            }
            DocDirective::CompileError(expected) => {
                let result = crate::compile_raw(&block.source);
                let elapsed = start.elapsed().as_millis() as u64;
                match result {
                    Ok(_) => DocTestResult {
                        block: block.clone(),
                        passed: false,
                        message: format!(
                            "Expected compile error{}, but compilation succeeded",
                            if expected.is_empty() {
                                String::new()
                            } else {
                                format!(" containing '{expected}'")
                            }
                        ),
                        duration_ms: elapsed,
                    },
                    Err(e) => {
                        let msg = format!("{e}");
                        if expected.is_empty() || msg.contains(expected.as_str()) {
                            DocTestResult {
                                block: block.clone(),
                                passed: true,
                                message: format!("Correctly produced compile error: {msg}"),
                                duration_ms: elapsed,
                            }
                        } else {
                            DocTestResult {
                                block: block.clone(),
                                passed: false,
                                message: format!(
                                    "Expected error containing '{expected}', got: {msg}"
                                ),
                                duration_ms: elapsed,
                            }
                        }
                    }
                }
            }
        }
    }

    /// Run all blocks and produce a summary.
    pub fn run_all(blocks: &[DocCodeBlock]) -> DocTestSummary {
        let mut results = Vec::new();
        let mut passed = 0;
        let mut failed = 0;
        let mut skipped = 0;

        for block in blocks {
            let result = Self::run_block(block);
            match &block.directive {
                DocDirective::Skip | DocDirective::NoTest => skipped += 1,
                _ => {
                    if result.passed {
                        passed += 1;
                    } else {
                        failed += 1;
                    }
                }
            }
            results.push(result);
        }

        DocTestSummary {
            total: blocks.len(),
            passed,
            failed,
            skipped,
            results,
        }
    }

    /// Format a human-readable summary.
    pub fn format_summary(summary: &DocTestSummary) -> String {
        format!(
            "Doc tests: {} total, {} passed, {} failed, {} skipped",
            summary.total, summary.passed, summary.failed, summary.skipped
        )
    }

    /// Format details of failing tests.
    pub fn format_failures(summary: &DocTestSummary) -> String {
        let mut out = String::new();
        for result in &summary.results {
            if !result.passed
                && !matches!(
                    result.block.directive,
                    DocDirective::Skip | DocDirective::NoTest
                )
            {
                out.push_str(&format!(
                    "FAIL {}:{} - {}\n",
                    result.block.file, result.block.line_number, result.message
                ));
                out.push_str("  Source:\n");
                for (i, line) in result.block.source.lines().enumerate() {
                    out.push_str(&format!("    {}: {line}\n", i + 1));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_simple_block() {
        let md = r#"# Docs

```lumen
cell main() -> Int
  return 42
end
```
"#;
        let blocks = DocExtractor::extract_blocks(md, "test.md");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].directive, DocDirective::CompileOk);
        assert!(blocks[0].source.contains("cell main"));
    }

    #[test]
    fn parse_directive_variants() {
        assert_eq!(DocExtractor::parse_directive(""), DocDirective::CompileOk);
        assert_eq!(
            DocExtractor::parse_directive("compile-ok"),
            DocDirective::CompileOk
        );
        assert_eq!(
            DocExtractor::parse_directive("compile-error(TypeMismatch)"),
            DocDirective::CompileError("TypeMismatch".to_string())
        );
        assert_eq!(DocExtractor::parse_directive("run-ok"), DocDirective::RunOk);
        assert_eq!(DocExtractor::parse_directive("skip"), DocDirective::Skip);
        assert_eq!(
            DocExtractor::parse_directive("no-test"),
            DocDirective::NoTest
        );
    }
}
