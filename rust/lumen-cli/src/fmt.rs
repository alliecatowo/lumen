//! Lumen code formatter
//!
//! Formats `.lm.md` files by preserving markdown structure and reformatting
//! code inside ```lumen ... ``` fenced blocks with consistent indentation,
//! spacing, and line breaks.

use lumen_compiler::compiler::lexer::Lexer;
use lumen_compiler::compiler::tokens::{Token, TokenKind};
use std::path::PathBuf;

/// Format a complete .lm.md file
pub fn format_file(content: &str) -> String {
    let mut output = String::new();
    let mut in_code_block = false;
    let mut code_block = String::new();

    for line in content.lines() {
        let trimmed = line.trim_start();

        if !in_code_block && trimmed.starts_with("```lumen") {
            // Start of lumen code block
            in_code_block = true;
            output.push_str(line);
            output.push('\n');
            code_block.clear();
        } else if in_code_block && trimmed.starts_with("```") {
            // End of code block — format and emit
            let formatted = format_lumen_code(&code_block);
            output.push_str(&formatted);
            if !formatted.is_empty() && !formatted.ends_with('\n') {
                output.push('\n');
            }
            output.push_str(line);
            output.push('\n');
            in_code_block = false;
        } else if in_code_block {
            // Accumulate code
            code_block.push_str(line);
            code_block.push('\n');
        } else {
            // Regular markdown - preserve as-is
            output.push_str(line);
            output.push('\n');
        }
    }

    output
}

/// Format Lumen code using token-based formatting
pub fn format_lumen_code(code: &str) -> String {
    if code.trim().is_empty() {
        return String::new();
    }

    // Tokenize
    let mut lexer = Lexer::new(code, 1, 0);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(_) => {
            // If lexing fails, return code as-is
            return code.to_string();
        }
    };

    // Group tokens into logical lines
    let mut lines = Vec::new();
    let mut current_line = Vec::new();

    for tok in &tokens {
        match &tok.kind {
            TokenKind::Newline => {
                if !current_line.is_empty() {
                    lines.push(std::mem::take(&mut current_line));
                }
            }
            TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof => {
                // Skip these meta-tokens
            }
            _ => {
                current_line.push(tok.clone());
            }
        }
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }

    // Format each line
    let mut output = String::new();
    let mut indent_level = 0;
    let mut prev_was_blank = false;
    let mut last_top_level_line = 0;

    for (line_idx, line_tokens) in lines.iter().enumerate() {
        if line_tokens.is_empty() {
            continue;
        }

        // Check if line starts with a dedent keyword
        let first_kind = &line_tokens[0].kind;
        let is_dedent_keyword = matches!(
            first_kind,
            TokenKind::End | TokenKind::Else
        );

        if is_dedent_keyword && indent_level > 0 {
            indent_level -= 1;
        }

        // Check if we should add blank line before top-level declarations
        let is_top_level_decl = indent_level == 0 && matches!(
            first_kind,
            TokenKind::Cell | TokenKind::Record | TokenKind::Enum |
            TokenKind::Use | TokenKind::Grant | TokenKind::Tool |
            TokenKind::Role | TokenKind::Schema
        );

        if is_top_level_decl && line_idx > 0 && !prev_was_blank && (line_idx - last_top_level_line) > 0 {
            output.push('\n');
        }

        if is_top_level_decl {
            last_top_level_line = line_idx;
        }

        // Emit indentation
        for _ in 0..indent_level {
            output.push_str("  ");
        }

        // Emit tokens for this line with appropriate spacing
        format_line(&mut output, line_tokens);
        output.push('\n');

        prev_was_blank = false;

        // Determine if this line should increase indent for the next line
        // Look for keywords that start blocks
        let starts_block = line_tokens.iter().any(|t| matches!(
            &t.kind,
            TokenKind::Cell | TokenKind::If | TokenKind::Else |
            TokenKind::For | TokenKind::While | TokenKind::Loop |
            TokenKind::Match | TokenKind::Try | TokenKind::Fn |
            TokenKind::Record | TokenKind::Enum
        ));

        // Also check for do keyword or opening brace
        let has_do = line_tokens.iter().any(|t| matches!(&t.kind, TokenKind::Ident(s) if s == "do"));
        let has_open_brace = line_tokens.iter().any(|t| matches!(&t.kind, TokenKind::LBrace));

        // Don't indent after single-line declarations (those with 'end' on same line)
        let has_end_same_line = line_tokens.iter().any(|t| matches!(&t.kind, TokenKind::End));

        if (starts_block || has_do || has_open_brace) && !has_end_same_line {
            indent_level += 1;
        }

        // Check for closing brace on this line that should dedent next line
        let has_close_brace = line_tokens.iter().any(|t| matches!(&t.kind, TokenKind::RBrace));
        if has_close_brace && indent_level > 0 {
            indent_level -= 1;
        }
    }

    // Trim trailing whitespace from each line and ensure single final newline
    let trimmed_lines: Vec<_> = output
        .lines()
        .map(|l| l.trim_end())
        .collect();

    let mut result = trimmed_lines.join("\n");
    if !result.is_empty() && !result.ends_with('\n') {
        result.push('\n');
    }

    result
}

/// Format a single line of tokens with proper spacing
fn format_line(output: &mut String, tokens: &[Token]) {
    for (i, tok) in tokens.iter().enumerate() {
        let next = tokens.get(i + 1);
        let prev = if i > 0 { tokens.get(i - 1) } else { None };

        // Add space before token if needed
        if i > 0 && should_space_before(tok, prev) {
            output.push(' ');
        }

        // Add token text
        match &tok.kind {
            TokenKind::IntLit(n) => output.push_str(&n.to_string()),
            TokenKind::FloatLit(f) => output.push_str(&f.to_string()),
            TokenKind::StringLit(s) => {
                output.push('"');
                output.push_str(&escape_string(s));
                output.push('"');
            }
            TokenKind::StringInterpLit(segments) => {
                // Reconstruct interpolated string
                output.push('"');
                for (is_expr, text) in segments {
                    if *is_expr {
                        output.push('{');
                        output.push_str(text);
                        output.push('}');
                    } else {
                        output.push_str(&escape_string(text));
                    }
                }
                output.push('"');
            }
            TokenKind::BoolLit(b) => output.push_str(&b.to_string()),
            TokenKind::RawStringLit(s) => {
                output.push_str("r\"");
                output.push_str(s);
                output.push('"');
            }
            TokenKind::BytesLit(_) => {
                output.push_str("b\"...\"");
            }
            TokenKind::NullLit => output.push_str("null"),
            TokenKind::Ident(s) => output.push_str(s),
            _ => output.push_str(&tok.kind.to_string()),
        }

        // Add spacing after token
        let needs_space = should_space_after(tok, next);
        if needs_space {
            output.push(' ');
        }
    }
}

/// Determine if we should add a space before this token
fn should_space_before(tok: &Token, prev: Option<&Token>) -> bool {
    let prev_kind = match prev {
        Some(t) => &t.kind,
        None => return false,
    };

    // Space before binary operators (unless after opening delim)
    if matches!(
        &tok.kind,
        TokenKind::Plus | TokenKind::Minus | TokenKind::Star | TokenKind::Slash |
        TokenKind::Percent | TokenKind::Eq | TokenKind::NotEq |
        TokenKind::Lt | TokenKind::LtEq | TokenKind::Gt | TokenKind::GtEq |
        TokenKind::Assign |
        TokenKind::PlusAssign | TokenKind::MinusAssign | TokenKind::StarAssign | TokenKind::SlashAssign |
        TokenKind::StarStar | TokenKind::DotDot | TokenKind::DotDotEq |
        TokenKind::PipeForward | TokenKind::Compose | TokenKind::QuestionQuestion |
        TokenKind::FatArrow | TokenKind::PlusPlus |
        TokenKind::Ampersand | TokenKind::Caret | TokenKind::Pipe |
        TokenKind::Arrow | TokenKind::And | TokenKind::Or
    ) {
        return !matches!(
            prev_kind,
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace
        );
    }

    false
}

/// Determine if we should add a space after this token
fn should_space_after(tok: &Token, next: Option<&Token>) -> bool {
    let next_kind = match next {
        Some(t) => &t.kind,
        None => return false,
    };

    // No space before closing delimiters or punctuation
    if matches!(
        next_kind,
        TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace |
        TokenKind::Comma | TokenKind::Semicolon |
        TokenKind::Dot | TokenKind::QuestionDot
    ) {
        return false;
    }

    match &tok.kind {
        // Space after binary operators
        TokenKind::Plus | TokenKind::Minus | TokenKind::Star | TokenKind::Slash |
        TokenKind::Percent | TokenKind::Eq | TokenKind::NotEq |
        TokenKind::Lt | TokenKind::LtEq | TokenKind::Gt | TokenKind::GtEq |
        TokenKind::Assign |
        TokenKind::PlusAssign | TokenKind::MinusAssign | TokenKind::StarAssign | TokenKind::SlashAssign |
        TokenKind::StarStar | TokenKind::DotDot | TokenKind::DotDotEq |
        TokenKind::PipeForward | TokenKind::Compose | TokenKind::QuestionQuestion |
        TokenKind::FatArrow | TokenKind::PlusPlus |
        TokenKind::Ampersand | TokenKind::Caret | TokenKind::Pipe => true,

        // Space after arrow
        TokenKind::Arrow => true,

        // Space after comma
        TokenKind::Comma => true,

        // Space after colon (for type annotations)
        TokenKind::Colon => !matches!(next_kind, TokenKind::Colon),

        // Space after keywords
        TokenKind::Cell | TokenKind::Record | TokenKind::Enum |
        TokenKind::Let | TokenKind::If | TokenKind::Else |
        TokenKind::For | TokenKind::In | TokenKind::Match |
        TokenKind::Return | TokenKind::Halt | TokenKind::Use |
        TokenKind::Tool | TokenKind::As | TokenKind::Grant |
        TokenKind::Expect | TokenKind::Schema | TokenKind::Role |
        TokenKind::Where | TokenKind::And | TokenKind::Or | TokenKind::Not |
        TokenKind::While | TokenKind::Loop | TokenKind::Break | TokenKind::Continue |
        TokenKind::Mut | TokenKind::Const | TokenKind::Pub |
        TokenKind::Import | TokenKind::From | TokenKind::Async | TokenKind::Await |
        TokenKind::Parallel | TokenKind::Fn | TokenKind::Trait | TokenKind::Impl |
        TokenKind::Type | TokenKind::Emit | TokenKind::Yield | TokenKind::Mod |
        TokenKind::With | TokenKind::Try | TokenKind::Union | TokenKind::Step |
        TokenKind::Then | TokenKind::When => true,

        // Space after identifiers (unless followed by paren/bracket/dot/operators that add their own space before)
        TokenKind::Ident(_) => {
            !matches!(
                next_kind,
                TokenKind::LParen | TokenKind::LBracket | TokenKind::Dot | TokenKind::Colon |
                // Operators add space before themselves
                TokenKind::Plus | TokenKind::Minus | TokenKind::Star | TokenKind::Slash |
                TokenKind::Percent | TokenKind::Eq | TokenKind::NotEq |
                TokenKind::Lt | TokenKind::LtEq | TokenKind::Gt | TokenKind::GtEq |
                TokenKind::Assign |
                TokenKind::PlusAssign | TokenKind::MinusAssign | TokenKind::StarAssign | TokenKind::SlashAssign |
                TokenKind::StarStar | TokenKind::DotDot | TokenKind::DotDotEq |
                TokenKind::PipeForward | TokenKind::Compose | TokenKind::QuestionQuestion |
                TokenKind::FatArrow | TokenKind::PlusPlus |
                TokenKind::Ampersand | TokenKind::Caret | TokenKind::Pipe |
                TokenKind::Arrow | TokenKind::And | TokenKind::Or
            )
        }

        // Don't add space after closing paren before arrow (arrow adds its own space before)
        TokenKind::RParen => {
            !matches!(next_kind, TokenKind::Arrow | TokenKind::LParen | TokenKind::LBracket | TokenKind::Dot)
        }

        // No space after opening delimiters
        TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => false,

        // No space after dots
        TokenKind::Dot | TokenKind::QuestionDot => false,

        // No space after prefix operators
        TokenKind::Bang | TokenKind::Tilde => false,

        _ => false,
    }
}

/// Escape special characters in a string literal
fn escape_string(s: &str) -> String {
    let mut result = String::new();
    for ch in s.chars() {
        match ch {
            '\n' => result.push_str("\\n"),
            '\t' => result.push_str("\\t"),
            '\r' => result.push_str("\\r"),
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            '\0' => result.push_str("\\0"),
            c => result.push(c),
        }
    }
    result
}

/// Format files in place or check if they need formatting
pub fn format_files(files: &[PathBuf], check_mode: bool) -> Result<bool, String> {
    let mut needs_formatting = false;

    for file in files {
        let content = std::fs::read_to_string(file)
            .map_err(|e| format!("error reading '{}': {}", file.display(), e))?;

        let formatted = format_file(&content);

        if content != formatted {
            needs_formatting = true;
            if check_mode {
                println!("{} — would reformat", file.display());
            } else {
                std::fs::write(file, &formatted)
                    .map_err(|e| format!("error writing '{}': {}", file.display(), e))?;
                println!("{} — formatted", file.display());
            }
        } else if !check_mode {
            println!("{} — already formatted", file.display());
        }
    }

    Ok(needs_formatting)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_indentation() {
        let input = r#"cell foo() -> Int
	return 42
end"#;
        let output = format_lumen_code(input);
        assert!(output.contains("  return 42"));
        assert!(!output.contains('\t'));
    }

    #[test]
    fn test_keyword_block_indentation() {
        let input = r#"cell main() -> Int
if true
return 1
else
return 2
end
end"#;
        let output = format_lumen_code(input);
        let lines: Vec<_> = output.lines().collect();

        assert_eq!(lines[0], "cell main() -> Int");
        assert!(lines[1].starts_with("  if"));
        assert!(lines[2].starts_with("    return 1"));
        assert!(lines[3].starts_with("  else"));
        assert!(lines[4].starts_with("    return 2"));
        assert!(lines[5].starts_with("  end"));
    }

    #[test]
    fn test_operator_spacing() {
        let input = "let x=1+2*3";
        let output = format_lumen_code(input);
        assert!(output.contains("x = 1 + 2 * 3"));
    }

    #[test]
    fn test_trailing_whitespace_removal() {
        let input = "cell foo() -> Int   \n  return 42  \nend  ";
        let output = format_lumen_code(input);
        for line in output.lines() {
            assert_eq!(line, line.trim_end());
        }
    }

    #[test]
    fn test_markdown_preservation() {
        let input = r#"# Hello

Some text here.

```lumen
cell greet() -> String
return "hello"
end
```

More text.
"#;
        let output = format_file(input);
        assert!(output.contains("# Hello"));
        assert!(output.contains("Some text here."));
        assert!(output.contains("More text."));
        assert!(output.contains("```lumen"));
        assert!(output.contains("  return \"hello\""));
    }

    #[test]
    fn test_check_mode() {
        let input = "cell foo()->Int\nreturn 42\nend\n";
        let formatted = format_lumen_code(input);
        assert_ne!(input, formatted);
        assert!(formatted.contains("cell foo() -> Int"));
    }

    #[test]
    fn test_blank_lines_between_top_level() {
        let input = r#"cell foo() -> Int
  return 1
end
cell bar() -> Int
  return 2
end"#;
        let output = format_lumen_code(input);
        assert!(output.contains("end\n\ncell bar"));
    }
}
