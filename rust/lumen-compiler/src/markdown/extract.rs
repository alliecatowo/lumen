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

    let mut byte_offset: usize = 0;

    for (line_idx, line) in source.lines().enumerate() {
        let line_num = line_idx + 1; // 1-based
        let trimmed = line.trim();

        if !in_fence {
            // Check for opening fence: ```lumen
            if trimmed.starts_with("```") && trimmed.len() > 3 {
                let lang = trimmed[3..].trim().to_string();
                if lang == "lumen" {
                    in_fence = true;
                    fence_lang = lang;
                    fence_code.clear();
                    fence_start_offset = byte_offset;
                    fence_start_line = line_num;
                    code_start_line = line_num + 1;
                    code_start_offset = byte_offset + line.len() + 1; // +1 for newline
                }
            } else if trimmed.starts_with('@') {
                // Parse directive
                let directive_text = trimmed[1..].trim();
                let (name, value) = if let Some(space_idx) = directive_text.find(|c: char| c.is_whitespace()) {
                    let n = directive_text[..space_idx].to_string();
                    let v = directive_text[space_idx..].trim().trim_matches('"').to_string();
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
            // Check for closing fence
            if trimmed == "```" {
                in_fence = false;
                code_blocks.push(CodeBlock {
                    code: fence_code.clone(),
                    language: fence_lang.clone(),
                    span: Span::new(fence_start_offset, byte_offset + line.len(), fence_start_line, 1),
                    code_offset: code_start_offset,
                    code_start_line,
                });
                fence_code.clear();
            } else {
                if !fence_code.is_empty() {
                    fence_code.push('\n');
                }
                fence_code.push_str(line);
            }
        }

        byte_offset += line.len() + 1; // +1 for newline
    }

    ExtractResult { code_blocks, directives }
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
}
