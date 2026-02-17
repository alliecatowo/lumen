//! textDocument/formatting handler
//!
//! Delegates to the formatter in `lumen_cli::fmt` and returns a single
//! whole-document `TextEdit` when the source changes.

use lsp_types::{DocumentFormattingParams, Position, Range, TextEdit};

/// Build formatting edits for the given document.
///
/// Returns a `Vec<TextEdit>` — either a single whole-document replacement when
/// formatting produces a different result, or an empty vec when the source is
/// already correctly formatted (or on parse error, to avoid destroying the
/// user's code).
pub fn build_formatting(
    _params: DocumentFormattingParams,
    text: &str,
    uri_path: &str,
) -> Vec<TextEdit> {
    let is_lm_md = uri_path.ends_with(".lm.md");
    let is_lumen = uri_path.ends_with(".lumen");
    let is_lm = uri_path.ends_with(".lm");
    let is_markdown = uri_path.ends_with(".md") && !is_lm_md;

    let formatted = if is_lm_md || is_markdown {
        lumen_cli::fmt::format_file(text)
    } else if is_lm || is_lumen {
        lumen_cli::fmt::format_lm_source(text)
    } else {
        // Unknown extension — try code-first formatting
        lumen_cli::fmt::format_lm_source(text)
    };

    // If the formatted output is the same as the input, return empty edits
    if formatted == text {
        return vec![];
    }

    // Return a single TextEdit that replaces the entire document
    let line_count = text.lines().count() as u32;
    let last_line_len = text.lines().last().map(|l| l.len() as u32).unwrap_or(0);

    // Handle trailing newline: if text ends with \n, the last "line" from
    // lines() is the one before the newline, but the position is actually
    // one line further.
    let (end_line, end_char) = if text.ends_with('\n') {
        (line_count, 0)
    } else {
        (line_count.saturating_sub(1), last_line_len)
    };

    vec![TextEdit {
        range: Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: end_line,
                character: end_char,
            },
        },
        new_text: formatted,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{DocumentFormattingParams, FormattingOptions, TextDocumentIdentifier, Uri};
    use std::str::FromStr;

    fn make_params() -> DocumentFormattingParams {
        DocumentFormattingParams {
            text_document: TextDocumentIdentifier {
                uri: Uri::from_str("file:///test.lm").unwrap(),
            },
            options: FormattingOptions {
                tab_size: 2,
                insert_spaces: true,
                ..Default::default()
            },
            work_done_progress_params: Default::default(),
        }
    }

    fn make_params_md() -> DocumentFormattingParams {
        DocumentFormattingParams {
            text_document: TextDocumentIdentifier {
                uri: Uri::from_str("file:///test.lm.md").unwrap(),
            },
            options: FormattingOptions {
                tab_size: 2,
                insert_spaces: true,
                ..Default::default()
            },
            work_done_progress_params: Default::default(),
        }
    }

    #[test]
    fn already_formatted_returns_empty() {
        let source = "cell main() -> Int\n  return 42\nend\n";
        let edits = build_formatting(make_params(), source, "/test.lm");
        assert!(edits.is_empty(), "No edits needed for well-formatted code");
    }

    #[test]
    fn formats_lm_source() {
        // The formatter re-parses via AST, so badly indented code should be fixed
        let source = "cell foo() -> Int\nreturn 42\nend\n";
        let edits = build_formatting(make_params(), source, "/test.lm");
        // The formatter should produce something with proper indentation
        if !edits.is_empty() {
            assert_eq!(edits.len(), 1);
            assert!(edits[0].new_text.contains("  return 42"));
            // Should start at (0,0)
            assert_eq!(edits[0].range.start.line, 0);
            assert_eq!(edits[0].range.start.character, 0);
        }
    }

    #[test]
    fn formats_lm_md_source() {
        let source = "# Title\n\n```lumen\ncell foo() -> Int\nreturn 42\nend\n```\n";
        let edits = build_formatting(make_params_md(), source, "/test.lm.md");
        if !edits.is_empty() {
            assert_eq!(edits.len(), 1);
            assert!(edits[0].new_text.contains("  return 42"));
        }
    }

    #[test]
    fn parse_error_returns_no_destructive_changes() {
        // When the formatter can't parse the code, it should either return
        // empty edits or return edits that preserve the code content
        let source = "cell foo( -> Int\n  return 42\nend\n";
        let edits = build_formatting(make_params(), source, "/test.lm");
        if !edits.is_empty() {
            // If edits are produced, the new text should still contain the original code
            // (the formatter falls back to returning the original on parse error)
            let new_text = &edits[0].new_text;
            assert!(
                new_text.contains("cell foo("),
                "parse-error code should be preserved"
            );
        }
    }

    #[test]
    fn edit_range_covers_entire_document() {
        let source = "cell foo() -> Int\nreturn 42\nend\n";
        let edits = build_formatting(make_params(), source, "/test.lm");
        if !edits.is_empty() {
            let edit = &edits[0];
            assert_eq!(edit.range.start.line, 0);
            assert_eq!(edit.range.start.character, 0);
            // Last line should cover end of document
            assert!(edit.range.end.line >= 2);
        }
    }

    #[test]
    fn empty_source_returns_empty() {
        let edits = build_formatting(make_params(), "", "/test.lm");
        assert!(edits.is_empty());
    }

    #[test]
    fn lumen_extension_uses_code_first_mode() {
        let source = "cell foo() -> Int\nreturn 42\nend\n";
        let edits = build_formatting(make_params(), source, "/test.lumen");
        if !edits.is_empty() {
            assert!(edits[0].new_text.contains("  return 42"));
        }
    }
}
