//! Rich error diagnostics with source snippets, colors, and suggestions.

use crate::compiler::constraints::ConstraintError;
use crate::compiler::lexer::LexError;
use crate::compiler::parser::ParseError;
use crate::compiler::resolve::ResolveError;
use crate::compiler::typecheck::TypeError;
use crate::CompileError;

/// Severity level for diagnostics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

/// A rendered diagnostic with source context
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Option<String>,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub col: Option<usize>,
    pub source_line: Option<String>,
    pub underline: Option<String>,
    pub suggestions: Vec<String>,
}

impl Diagnostic {
    /// Render with ANSI colors for terminal
    pub fn render_ansi(&self) -> String {
        let mut out = String::new();

        // Header: error[E001]: message
        let severity_label = match self.severity {
            Severity::Error => red("error"),
            Severity::Warning => yellow("warning"),
            Severity::Note => cyan("note"),
        };

        if let Some(ref code) = self.code {
            out.push_str(&format!("{}[{}]: ", severity_label, bold(code)));
        } else {
            out.push_str(&format!("{}: ", severity_label));
        }
        out.push_str(&bold(&self.message));
        out.push('\n');

        // Location: --> file:line:col
        if let (Some(ref file), Some(line), Some(col)) = (&self.file, self.line, self.col) {
            out.push_str(&format!("  {} {}:{}:{}\n", cyan("-->"), file, line, col));
        } else if let (Some(ref file), Some(line)) = (&self.file, self.line) {
            out.push_str(&format!("  {} {}:{}\n", cyan("-->"), file, line));
        }

        // Source line with underline
        if let (Some(line_num), Some(ref line_text), Some(ref underline)) =
            (self.line, &self.source_line, &self.underline)
        {
            out.push_str(&format!("   {}\n", cyan("|")));
            out.push_str(&format!(
                "{:>3} {} {}\n",
                cyan(&line_num.to_string()),
                cyan("|"),
                line_text
            ));
            out.push_str(&format!("   {} {}\n", cyan("|"), red(underline)));
        }

        // Suggestions
        if !self.suggestions.is_empty() {
            out.push_str(&format!("   {}\n", cyan("|")));
            for suggestion in &self.suggestions {
                out.push_str(&format!("   {} {}: {}\n", cyan("="), cyan("help"), suggestion));
            }
        }

        out
    }

    /// Render without colors (for LSP, tests)
    pub fn render_plain(&self) -> String {
        let mut out = String::new();

        // Header
        let severity_label = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        };

        if let Some(ref code) = self.code {
            out.push_str(&format!("{}[{}]: ", severity_label, code));
        } else {
            out.push_str(&format!("{}: ", severity_label));
        }
        out.push_str(&self.message);
        out.push('\n');

        // Location
        if let (Some(ref file), Some(line), Some(col)) = (&self.file, self.line, self.col) {
            out.push_str(&format!("  --> {}:{}:{}\n", file, line, col));
        } else if let (Some(ref file), Some(line)) = (&self.file, self.line) {
            out.push_str(&format!("  --> {}:{}\n", file, line));
        }

        // Source line with underline
        if let (Some(line_num), Some(ref line_text), Some(ref underline)) =
            (self.line, &self.source_line, &self.underline)
        {
            out.push_str("   |\n");
            out.push_str(&format!("{:>3} | {}\n", line_num, line_text));
            out.push_str(&format!("   | {}\n", underline));
        }

        // Suggestions
        if !self.suggestions.is_empty() {
            out.push_str("   |\n");
            for suggestion in &self.suggestions {
                out.push_str(&format!("   = help: {}\n", suggestion));
            }
        }

        out
    }
}

// ANSI color helpers
fn red(s: &str) -> String {
    format!("\x1b[31m{}\x1b[0m", s)
}

fn yellow(s: &str) -> String {
    format!("\x1b[33m{}\x1b[0m", s)
}

fn cyan(s: &str) -> String {
    format!("\x1b[36m{}\x1b[0m", s)
}

fn bold(s: &str) -> String {
    format!("\x1b[1m{}\x1b[0m", s)
}

// Source line extraction
fn get_source_line(source: &str, line: usize) -> Option<String> {
    source
        .lines()
        .nth(line.saturating_sub(1))
        .map(|s| s.to_string())
}

fn make_underline(col: usize, len: usize) -> String {
    format!(
        "{}{}",
        " ".repeat(col.saturating_sub(1)),
        "^".repeat(len.max(1))
    )
}

// Edit distance for suggestions
fn edit_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut matrix = vec![vec![0; b_len + 1]; a_len + 1];

    #[allow(clippy::needless_range_loop)]
    for i in 0..=a_len {
        matrix[i][0] = i;
    }
    #[allow(clippy::needless_range_loop)]
    for j in 0..=b_len {
        matrix[0][j] = j;
    }

    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[a_len][b_len]
}

fn suggest_similar(name: &str, candidates: &[&str], max_distance: usize) -> Vec<String> {
    let mut matches: Vec<(usize, String)> = candidates
        .iter()
        .filter_map(|c| {
            let d = edit_distance(name, c);
            if d <= max_distance {
                Some((d, c.to_string()))
            } else {
                None
            }
        })
        .collect();

    matches.sort_by_key(|(d, _)| *d);
    matches.into_iter().map(|(_, s)| s).take(3).collect()
}

// Lumen keywords for suggestions
const KEYWORDS: &[&str] = &[
    "record", "enum", "cell", "let", "if", "else", "for", "in", "match", "return", "halt", "end",
    "use", "tool", "as", "grant", "expect", "schema", "role", "where", "and", "or", "not", "null",
    "result", "ok", "err", "list", "map", "while", "loop", "break", "continue", "mut", "const",
    "pub", "import", "from", "async", "await", "parallel", "fn", "trait", "impl", "type", "set",
    "tuple", "emit", "yield", "mod", "self", "with", "try", "union", "step", "comptime", "macro",
    "extern", "then", "when", "bool", "int", "float", "string", "bytes", "json",
];

// Builtin functions for suggestions
const BUILTINS: &[&str] = &[
    "print", "len", "length", "append", "range", "to_string", "str", "to_int", "int", "to_float",
    "float", "type_of", "keys", "values", "contains", "join", "split", "trim", "upper", "lower",
    "replace", "abs", "min", "max", "hash", "not", "count", "matches", "slice", "sort", "reverse",
    "map", "filter", "reduce", "parallel", "race", "vote", "select", "timeout", "spawn", "resume",
];

/// Convert a CompileError + source text into a list of Diagnostics
pub fn format_compile_error(
    error: &CompileError,
    source: &str,
    filename: &str,
) -> Vec<Diagnostic> {
    match error {
        CompileError::Lex(e) => vec![format_lex_error(e, source, filename)],
        CompileError::Parse(e) => vec![format_parse_error(e, source, filename)],
        CompileError::Resolve(errors) => errors
            .iter()
            .map(|e| format_resolve_error(e, source, filename))
            .collect(),
        CompileError::Type(errors) => errors
            .iter()
            .map(|e| format_type_error(e, source, filename))
            .collect(),
        CompileError::Constraint(errors) => errors
            .iter()
            .map(|e| format_constraint_error(e, source, filename))
            .collect(),
    }
}

fn format_lex_error(error: &LexError, source: &str, filename: &str) -> Diagnostic {
    match error {
        LexError::UnexpectedChar { ch, line, col } => {
            let source_line = get_source_line(source, *line);
            let underline = source_line.as_ref().map(|_| make_underline(*col, 1));

            Diagnostic {
                severity: Severity::Error,
                code: Some("E001".to_string()),
                message: format!("unexpected character '{}'", ch),
                file: Some(filename.to_string()),
                line: Some(*line),
                col: Some(*col),
                source_line,
                underline,
                suggestions: vec![],
            }
        }
        LexError::UnterminatedString { line, col } => {
            let source_line = get_source_line(source, *line);
            let underline = source_line
                .as_ref()
                .map(|l| make_underline(*col, l.len() - col + 1));

            Diagnostic {
                severity: Severity::Error,
                code: Some("E002".to_string()),
                message: "unterminated string literal".to_string(),
                file: Some(filename.to_string()),
                line: Some(*line),
                col: Some(*col),
                source_line,
                underline,
                suggestions: vec!["add a closing quote".to_string()],
            }
        }
        LexError::InconsistentIndent { line } => {
            let source_line = get_source_line(source, *line);
            let underline = source_line.as_ref().map(|l| {
                let indent = l.chars().take_while(|c| c.is_whitespace()).count();
                make_underline(1, indent.max(1))
            });

            Diagnostic {
                severity: Severity::Error,
                code: Some("E003".to_string()),
                message: "inconsistent indentation".to_string(),
                file: Some(filename.to_string()),
                line: Some(*line),
                col: Some(1),
                source_line,
                underline,
                suggestions: vec![
                    "ensure all indentation uses the same number of spaces".to_string()
                ],
            }
        }
        LexError::InvalidNumber { line, col } => {
            let source_line = get_source_line(source, *line);
            let underline = source_line.as_ref().map(|_| make_underline(*col, 1));

            Diagnostic {
                severity: Severity::Error,
                code: Some("E004".to_string()),
                message: "invalid number literal".to_string(),
                file: Some(filename.to_string()),
                line: Some(*line),
                col: Some(*col),
                source_line,
                underline,
                suggestions: vec![],
            }
        }
        LexError::InvalidBytesLiteral { line, col } => {
            let source_line = get_source_line(source, *line);
            let underline = source_line.as_ref().map(|_| make_underline(*col, 1));

            Diagnostic {
                severity: Severity::Error,
                code: Some("E005".to_string()),
                message: "invalid bytes literal".to_string(),
                file: Some(filename.to_string()),
                line: Some(*line),
                col: Some(*col),
                source_line,
                underline,
                suggestions: vec!["bytes literals must be hex: b\"48656c6c6f\"".to_string()],
            }
        }
        LexError::InvalidUnicodeEscape { line, col } => {
            let source_line = get_source_line(source, *line);
            let underline = source_line.as_ref().map(|_| make_underline(*col, 1));

            Diagnostic {
                severity: Severity::Error,
                code: Some("E006".to_string()),
                message: "invalid unicode escape sequence".to_string(),
                file: Some(filename.to_string()),
                line: Some(*line),
                col: Some(*col),
                source_line,
                underline,
                suggestions: vec!["use \\u{XXXX} format for unicode escapes".to_string()],
            }
        }
    }
}

fn format_parse_error(error: &ParseError, source: &str, filename: &str) -> Diagnostic {
    match error {
        ParseError::Unexpected {
            found,
            expected,
            line,
            col,
        } => {
            let source_line = get_source_line(source, *line);
            let underline = source_line.as_ref().map(|_| make_underline(*col, 1));

            Diagnostic {
                severity: Severity::Error,
                code: Some("E010".to_string()),
                message: format!("unexpected token '{}', expected {}", found, expected),
                file: Some(filename.to_string()),
                line: Some(*line),
                col: Some(*col),
                source_line,
                underline,
                suggestions: vec![],
            }
        }
        ParseError::UnexpectedEof => Diagnostic {
            severity: Severity::Error,
            code: Some("E011".to_string()),
            message: "unexpected end of input".to_string(),
            file: Some(filename.to_string()),
            line: None,
            col: None,
            source_line: None,
            underline: None,
            suggestions: vec!["check for missing 'end' keywords".to_string()],
        },
    }
}

fn format_resolve_error(error: &ResolveError, source: &str, filename: &str) -> Diagnostic {
    match error {
        ResolveError::UndefinedType {
            name,
            line,
            suggestions: error_suggestions,
        } => {
            let source_line = get_source_line(source, *line);
            let underline = source_line.as_ref().map(|l| {
                if let Some(pos) = l.find(name) {
                    make_underline(pos + 1, name.len())
                } else {
                    make_underline(1, 1)
                }
            });

            let help = if !error_suggestions.is_empty() {
                error_suggestions
                    .iter()
                    .map(|s| format!("did you mean '{}'?", s))
                    .collect()
            } else {
                vec![]
            };

            Diagnostic {
                severity: Severity::Error,
                code: Some("E020".to_string()),
                message: format!("undefined type '{}'", name),
                file: Some(filename.to_string()),
                line: Some(*line),
                col: None,
                source_line,
                underline,
                suggestions: help,
            }
        }
        ResolveError::UndefinedCell {
            name,
            line,
            suggestions: error_suggestions,
        } => {
            let source_line = get_source_line(source, *line);
            let underline = source_line.as_ref().map(|l| {
                if let Some(pos) = l.find(name) {
                    make_underline(pos + 1, name.len())
                } else {
                    make_underline(1, 1)
                }
            });

            let help = if !error_suggestions.is_empty() {
                error_suggestions
                    .iter()
                    .map(|s| format!("did you mean '{}'?", s))
                    .collect()
            } else {
                vec![]
            };

            Diagnostic {
                severity: Severity::Error,
                code: Some("E021".to_string()),
                message: format!("undefined cell '{}'", name),
                file: Some(filename.to_string()),
                line: Some(*line),
                col: None,
                source_line,
                underline,
                suggestions: help,
            }
        }
        ResolveError::UndefinedTool { name, line } => {
            let source_line = get_source_line(source, *line);
            let underline = source_line.as_ref().map(|l| {
                if let Some(pos) = l.find(name) {
                    make_underline(pos + 1, name.len())
                } else {
                    make_underline(1, 1)
                }
            });

            Diagnostic {
                severity: Severity::Error,
                code: Some("E022".to_string()),
                message: format!("undefined tool alias '{}'", name),
                file: Some(filename.to_string()),
                line: Some(*line),
                col: None,
                source_line,
                underline,
                suggestions: vec!["ensure the tool is declared with 'use tool'".to_string()],
            }
        }
        ResolveError::Duplicate { name, line } => {
            let source_line = get_source_line(source, *line);
            let underline = source_line.as_ref().map(|l| {
                if let Some(pos) = l.find(name) {
                    make_underline(pos + 1, name.len())
                } else {
                    make_underline(1, 1)
                }
            });

            Diagnostic {
                severity: Severity::Error,
                code: Some("E023".to_string()),
                message: format!("duplicate definition '{}'", name),
                file: Some(filename.to_string()),
                line: Some(*line),
                col: None,
                source_line,
                underline,
                suggestions: vec![],
            }
        }
        ResolveError::UndeclaredEffect {
            cell,
            effect,
            line,
            cause,
        } => {
            let source_line = get_source_line(source, *line);
            let underline = source_line.as_ref().map(|_| make_underline(1, 1));

            let mut suggestions = vec![format!(
                "add '{}' to the effect row of cell '{}'",
                effect, cell
            )];
            if !cause.is_empty() {
                suggestions.push(format!("caused by: {}", cause));
            }

            Diagnostic {
                severity: Severity::Error,
                code: Some("E030".to_string()),
                message: format!(
                    "cell '{}' performs effect '{}' but it is not declared in its effect row",
                    cell, effect
                ),
                file: Some(filename.to_string()),
                line: Some(*line),
                col: None,
                source_line,
                underline,
                suggestions,
            }
        }
        _ => {
            // Fallback for other resolve errors
            Diagnostic {
                severity: Severity::Error,
                code: Some("E099".to_string()),
                message: error.to_string(),
                file: Some(filename.to_string()),
                line: None,
                col: None,
                source_line: None,
                underline: None,
                suggestions: vec![],
            }
        }
    }
}

fn format_type_error(error: &TypeError, source: &str, filename: &str) -> Diagnostic {
    match error {
        TypeError::Mismatch {
            expected,
            actual,
            line,
        } => {
            let source_line = get_source_line(source, *line);
            let underline = source_line.as_ref().map(|_| make_underline(1, 1));

            Diagnostic {
                severity: Severity::Error,
                code: Some("E040".to_string()),
                message: format!("type mismatch: expected {}, got {}", expected, actual),
                file: Some(filename.to_string()),
                line: Some(*line),
                col: None,
                source_line,
                underline,
                suggestions: vec![],
            }
        }
        TypeError::UndefinedVar { name, line } => {
            let source_line = get_source_line(source, *line);
            let underline = source_line.as_ref().map(|l| {
                if let Some(pos) = l.find(name) {
                    make_underline(pos + 1, name.len())
                } else {
                    make_underline(1, 1)
                }
            });

            let mut candidates: Vec<&str> = KEYWORDS.to_vec();
            candidates.extend(BUILTINS.iter().copied());
            let suggestions = suggest_similar(name, &candidates, 2);
            let help = if !suggestions.is_empty() {
                suggestions
                    .into_iter()
                    .map(|s| format!("did you mean '{}'?", s))
                    .collect()
            } else {
                vec![]
            };

            Diagnostic {
                severity: Severity::Error,
                code: Some("E041".to_string()),
                message: format!("undefined variable '{}'", name),
                file: Some(filename.to_string()),
                line: Some(*line),
                col: None,
                source_line,
                underline,
                suggestions: help,
            }
        }
        TypeError::UnknownField {
            field,
            ty,
            line,
            suggestions: error_suggestions,
        } => {
            let source_line = get_source_line(source, *line);
            let underline = source_line.as_ref().map(|l| {
                if let Some(pos) = l.find(field) {
                    make_underline(pos + 1, field.len())
                } else {
                    make_underline(1, 1)
                }
            });

            let help = if !error_suggestions.is_empty() {
                error_suggestions
                    .iter()
                    .map(|s| format!("did you mean '{}'?", s))
                    .collect()
            } else {
                vec![]
            };

            Diagnostic {
                severity: Severity::Error,
                code: Some("E042".to_string()),
                message: format!("unknown field '{}' on type '{}'", field, ty),
                file: Some(filename.to_string()),
                line: Some(*line),
                col: None,
                source_line,
                underline,
                suggestions: help,
            }
        }
        TypeError::IncompleteMatch {
            enum_name,
            missing,
            line,
        } => {
            let source_line = get_source_line(source, *line);
            let underline = source_line.as_ref().map(|_| make_underline(1, 1));

            let missing_list = missing.join(", ");
            let suggestions = vec![format!(
                "add patterns for missing variants: {}",
                missing_list
            )];

            Diagnostic {
                severity: Severity::Error,
                code: Some("E043".to_string()),
                message: format!(
                    "incomplete match on enum '{}': missing variants [{}]",
                    enum_name, missing_list
                ),
                file: Some(filename.to_string()),
                line: Some(*line),
                col: None,
                source_line,
                underline,
                suggestions,
            }
        }
        _ => {
            // Fallback for other type errors
            let line = match error {
                TypeError::NotCallable { line }
                | TypeError::ArgCount { line, .. }
                | TypeError::Mismatch { line, .. }
                | TypeError::UndefinedVar { line, .. }
                | TypeError::UnknownField { line, .. }
                | TypeError::IncompleteMatch { line, .. } => Some(*line),
                _ => None,
            };

            let source_line = line.and_then(|l| get_source_line(source, l));
            let underline = source_line.as_ref().map(|_| make_underline(1, 1));

            Diagnostic {
                severity: Severity::Error,
                code: Some("E049".to_string()),
                message: error.to_string(),
                file: Some(filename.to_string()),
                line,
                col: None,
                source_line,
                underline,
                suggestions: vec![],
            }
        }
    }
}

fn format_constraint_error(error: &ConstraintError, source: &str, filename: &str) -> Diagnostic {
    match error {
        ConstraintError::Invalid {
            field,
            line,
            message,
        } => {
            let source_line = get_source_line(source, *line);
            let underline = source_line.as_ref().map(|_| make_underline(1, 1));

            Diagnostic {
                severity: Severity::Error,
                code: Some("E050".to_string()),
                message: format!("invalid constraint on field '{}': {}", field, message),
                file: Some(filename.to_string()),
                line: Some(*line),
                col: None,
                source_line,
                underline,
                suggestions: vec![],
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_source_line() {
        let source = "line 1\nline 2\nline 3\n";
        assert_eq!(get_source_line(source, 1), Some("line 1".to_string()));
        assert_eq!(get_source_line(source, 2), Some("line 2".to_string()));
        assert_eq!(get_source_line(source, 3), Some("line 3".to_string()));
        assert_eq!(get_source_line(source, 4), None);
    }

    #[test]
    fn test_make_underline() {
        assert_eq!(make_underline(1, 3), "^^^");
        assert_eq!(make_underline(5, 2), "    ^^");
        assert_eq!(make_underline(10, 1), "         ^");
    }

    #[test]
    fn test_edit_distance() {
        assert_eq!(edit_distance("", ""), 0);
        assert_eq!(edit_distance("a", ""), 1);
        assert_eq!(edit_distance("", "a"), 1);
        assert_eq!(edit_distance("abc", "abc"), 0);
        assert_eq!(edit_distance("abc", "abd"), 1);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn test_suggest_similar() {
        let candidates = &["for", "from", "foo", "bar"];
        let suggestions = suggest_similar("fr", candidates, 2);
        assert!(suggestions.contains(&"for".to_string()));
        assert!(suggestions.len() <= 3);

        let suggestions = suggest_similar("xyz", candidates, 1);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_format_parse_error() {
        let error = ParseError::Unexpected {
            found: "if".to_string(),
            expected: "end".to_string(),
            line: 5,
            col: 10,
        };
        let source = "line 1\nline 2\nline 3\nline 4\nline 5 with if\n";
        let diag = format_parse_error(&error, source, "test.lm.md");

        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.code, Some("E010".to_string()));
        assert!(diag.message.contains("unexpected token"));
        assert_eq!(diag.line, Some(5));
    }

    #[test]
    fn test_format_type_error_undefined_var() {
        let error = TypeError::UndefinedVar {
            name: "fo".to_string(),
            line: 3,
        };
        let source = "line 1\nline 2\nlet x = fo\n";
        let diag = format_type_error(&error, source, "test.lm.md");

        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.code, Some("E041".to_string()));
        assert!(diag.message.contains("undefined variable"));
        assert!(!diag.suggestions.is_empty());
        // Should suggest "for" since edit distance is 1
        assert!(diag
            .suggestions
            .iter()
            .any(|s| s.contains("for") || s.contains("to")));
    }

    #[test]
    fn test_render_plain() {
        let diag = Diagnostic {
            severity: Severity::Error,
            code: Some("E041".to_string()),
            message: "undefined variable 'foo'".to_string(),
            file: Some("test.lm.md".to_string()),
            line: Some(10),
            col: Some(5),
            source_line: Some("  let x = foo".to_string()),
            underline: Some("         ^^^".to_string()),
            suggestions: vec!["did you mean 'for'?".to_string()],
        };

        let output = diag.render_plain();
        assert!(output.contains("error[E041]"));
        assert!(output.contains("undefined variable"));
        assert!(output.contains("test.lm.md:10:5"));
        assert!(output.contains("let x = foo"));
        assert!(output.contains("^^^"));
        assert!(output.contains("did you mean 'for'?"));
    }

    #[test]
    fn test_render_ansi() {
        let diag = Diagnostic {
            severity: Severity::Error,
            code: Some("E041".to_string()),
            message: "undefined variable 'foo'".to_string(),
            file: Some("test.lm.md".to_string()),
            line: Some(10),
            col: Some(5),
            source_line: Some("  let x = foo".to_string()),
            underline: Some("         ^^^".to_string()),
            suggestions: vec!["did you mean 'for'?".to_string()],
        };

        let output = diag.render_ansi();
        // Check that ANSI codes are present
        assert!(output.contains("\x1b["));
        assert!(output.contains("E041"));
        assert!(output.contains("undefined variable"));
    }
}
