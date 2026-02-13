//! Markdown → code block extraction with source location tracking

use crate::compiler::tokens::Span;

/// A code block extracted from a Markdown file
#[derive(Debug, Clone)]
pub struct CodeBlock {
    /// The raw source code inside the fenced block
    pub code: String,
    /// Language tag (should be "lumen")
    pub language: String,
    /// Span covering the entire fenced block in the original file
    pub span: Span,
    /// Byte offset where the code content starts (after the opening fence)
    pub code_offset: usize,
    /// Line number where the code content starts (1-based)
    pub code_start_line: usize,
}

/// A top-level directive line (starts with @)
#[derive(Debug, Clone)]
pub struct DirectiveLine {
    pub name: String,
    pub value: Option<String>,
    pub span: Span,
}

/// Result of extracting blocks from Markdown
#[derive(Debug, Clone)]
pub struct ExtractResult {
    pub code_blocks: Vec<CodeBlock>,
    pub directives: Vec<DirectiveLine>,
}

/// Extract Lumen code blocks and directives from a Markdown file.
///
/// Code blocks are fenced with triple backticks and tagged `lumen`.
/// Directives are lines starting with `@` outside of code blocks.
pub fn extract_blocks(source: &str) -> ExtractResult {
    let mut code_blocks = Vec::new();
    let mut directives = Vec::new();

    let mut in_fence = false;
    let mut fence_lang = String::new();
    let mut fence_code = String::new();
    let mut fence_start_offset: usize = 0;
    let mut fence_start_line: usize = 0;
    let mut code_start_line: usize = 0;
    let mut code_start_offset: usize = 0;
    let mut fence_backtick_count: usize = 0;

    let mut byte_offset: usize = 0;

    // Normalize line endings (handle CRLF)
    let normalized = source.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();

    for (line_idx, line) in lines.iter().enumerate() {
        let line_num = line_idx + 1; // 1-based
        let trimmed = line.trim();

        if !in_fence {
            // Check for opening fence: ```lumen (or ````lumen, etc.)
            if let Some(backtick_count) = count_leading_backticks(trimmed) {
                if backtick_count >= 3 {
                    // Extract language tag after backticks, trimming whitespace
                    let rest = &trimmed[backtick_count..];
                    let lang = rest.trim().to_lowercase();
                    // Accept "lumen", "lm", or empty (treated as lumen if it's the first block)
                    if lang == "lumen" || lang == "lm" {
                        in_fence = true;
                        fence_lang = lang;
                        fence_code.clear();
                        fence_start_offset = byte_offset;
                        fence_start_line = line_num;
                        code_start_line = line_num + 1;
                        code_start_offset = byte_offset + line.len() + 1; // +1 for newline
                        fence_backtick_count = backtick_count;
                    }
                }
            } else if let Some(stripped) = trimmed.strip_prefix('@') {
                // Parse directive
                let directive_text = stripped.trim();
                let (name, value) =
                    if let Some(space_idx) = directive_text.find(|c: char| c.is_whitespace()) {
                        let n = directive_text[..space_idx].to_string();
                        let v = directive_text[space_idx..]
                            .trim()
                            .trim_matches('"')
                            .to_string();
                        (n, Some(v))
                    } else {
                        (directive_text.to_string(), None)
                    };
                directives.push(DirectiveLine {
                    name,
                    value,
                    span: Span::new(byte_offset, byte_offset + line.len(), line_num, 1),
                });
            }
        } else {
            // Check for closing fence (must match opening backtick count or more)
            if let Some(backtick_count) = count_leading_backticks(trimmed) {
                let rest = &trimmed[backtick_count..];
                if backtick_count >= fence_backtick_count && rest.trim().is_empty() {
                    // Closing fence found
                    in_fence = false;
                    code_blocks.push(CodeBlock {
                        code: fence_code.clone(),
                        language: fence_lang.clone(),
                        span: Span::new(
                            fence_start_offset,
                            byte_offset + line.len(),
                            fence_start_line,
                            1,
                        ),
                        code_offset: code_start_offset,
                        code_start_line,
                    });
                    fence_code.clear();
                    continue;
                }
            }
            // Not a closing fence, add line to code
            if !fence_code.is_empty() {
                fence_code.push('\n');
            }
            fence_code.push_str(line);
        }

        byte_offset += line.len() + 1; // +1 for newline
    }

    ExtractResult {
        code_blocks,
        directives,
    }
}

/// Count leading backticks in a trimmed line, returning None if doesn't start with backticks
fn count_leading_backticks(trimmed: &str) -> Option<usize> {
    let count = trimmed.chars().take_while(|&c| c == '`').count();
    if count > 0 {
        Some(count)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple() {
        let src = r#"@lumen 1
@package "test"

# Hello

```lumen
record Foo
  x: Int
end
```

Some prose here.

```lumen
cell main() -> Int
  return 42
end
```
"#;
        let result = extract_blocks(src);
        assert_eq!(result.directives.len(), 2);
        assert_eq!(result.directives[0].name, "lumen");
        assert_eq!(result.directives[0].value, Some("1".to_string()));
        assert_eq!(result.directives[1].name, "package");
        assert_eq!(result.directives[1].value, Some("test".to_string()));

        assert_eq!(result.code_blocks.len(), 2);
        assert!(result.code_blocks[0].code.contains("record Foo"));
        assert!(result.code_blocks[1].code.contains("cell main"));
    }

    #[test]
    fn test_extract_non_lumen_blocks_ignored() {
        let src = r#"
```python
print("hello")
```

```lumen
cell greet() -> String
  return "hello"
end
```
"#;
        let result = extract_blocks(src);
        assert_eq!(result.code_blocks.len(), 1);
        assert!(result.code_blocks[0].code.contains("cell greet"));
    }

    #[test]
    fn test_nested_code_fences() {
        let src = r#"
````lumen
record Example
  code: String
end

cell demo() -> String
  let x = "```lumen\ncell foo()\nend\n```"
  return x
end
````
"#;
        let result = extract_blocks(src);
        assert_eq!(result.code_blocks.len(), 1);
        assert!(result.code_blocks[0].code.contains("```lumen"));
        assert!(result.code_blocks[0].code.contains("cell foo"));
    }

    #[test]
    fn test_language_alias_lm() {
        let src = r#"
```lm
cell test() -> Int
  42
end
```
"#;
        let result = extract_blocks(src);
        assert_eq!(result.code_blocks.len(), 1);
        assert_eq!(result.code_blocks[0].language, "lm");
        assert!(result.code_blocks[0].code.contains("cell test"));
    }

    #[test]
    fn test_case_insensitive_language() {
        let src = r#"
```Lumen
cell test() -> Int
  42
end
```

```LUMEN
cell test2() -> Int
  84
end
```
"#;
        let result = extract_blocks(src);
        assert_eq!(result.code_blocks.len(), 2);
        assert!(result.code_blocks[0].code.contains("cell test"));
        assert!(result.code_blocks[1].code.contains("cell test2"));
    }

    #[test]
    fn test_empty_code_block() {
        let src = r#"
```lumen
```
"#;
        let result = extract_blocks(src);
        assert_eq!(result.code_blocks.len(), 1);
        assert_eq!(result.code_blocks[0].code, "");
    }

    #[test]
    fn test_trailing_whitespace_on_fence() {
        let src = r#"
```lumen
cell test() -> Int
  42
end
```
"#;
        let result = extract_blocks(src);
        assert_eq!(result.code_blocks.len(), 1);
        assert!(result.code_blocks[0].code.contains("cell test"));
    }

    #[test]
    fn test_windows_line_endings() {
        let src = "```lumen\r\ncell test() -> Int\r\n  42\r\nend\r\n```\r\n";
        let result = extract_blocks(src);
        assert_eq!(result.code_blocks.len(), 1);
        assert!(result.code_blocks[0].code.contains("cell test"));
        assert!(result.code_blocks[0].code.contains("42"));
    }

    #[test]
    fn test_no_final_newline() {
        let src = "```lumen\ncell test() -> Int\n  42\nend\n```";
        let result = extract_blocks(src);
        assert_eq!(result.code_blocks.len(), 1);
        assert!(result.code_blocks[0].code.contains("cell test"));
    }

    #[test]
    fn test_multiple_blocks_line_tracking() {
        let src = r#"First line

```lumen
cell first() -> Int
  1
end
```

Middle prose here.

```lumen
cell second() -> Int
  2
end
```
"#;
        let result = extract_blocks(src);
        assert_eq!(result.code_blocks.len(), 2);
        // First block starts on line 3 (after blank line and "First line")
        assert_eq!(result.code_blocks[0].code_start_line, 4);
        // Second block starts after first block + prose
        assert!(result.code_blocks[1].code_start_line > result.code_blocks[0].code_start_line);
    }

    #[test]
    fn test_indented_code_blocks_ignored() {
        let src = r#"
Regular text.

    This is an indented code block
    It should be ignored

```lumen
cell test() -> Int
  42
end
```
"#;
        let result = extract_blocks(src);
        // Only the fenced block should be extracted
        assert_eq!(result.code_blocks.len(), 1);
        assert!(result.code_blocks[0].code.contains("cell test"));
        assert!(!result.code_blocks[0].code.contains("indented code block"));
    }

    #[test]
    fn test_backticks_inside_code() {
        let src = r#"
```lumen
cell demo() -> String
  let msg = "Use ``` for code fences"
  return msg
end
```
"#;
        let result = extract_blocks(src);
        assert_eq!(result.code_blocks.len(), 1);
        assert!(result.code_blocks[0]
            .code
            .contains("Use ``` for code fences"));
    }
}
