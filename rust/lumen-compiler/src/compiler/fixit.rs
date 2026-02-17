//! Fix-it hints: given a `CompileError` and the source text, suggest
//! concrete replacement/insertion/deletion edits the user can apply.

use crate::compiler::constraints::ConstraintError;
use crate::compiler::lexer::LexError;
use crate::compiler::ownership::OwnershipError;
use crate::compiler::parser::ParseError;
use crate::compiler::resolve::ResolveError;
use crate::compiler::tokens::Span;
use crate::compiler::typecheck::TypeError;
use crate::CompileError;

// ── Public types ───────────────────────────────────────────────────

/// The kind of source edit a fix-it represents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixitKind {
    /// Replace the text at `span` with `replacement`.
    Replace,
    /// Insert `replacement` *before* the position indicated by `span`.
    Insert,
    /// Delete the text at `span` (replacement is empty).
    Delete,
}

/// A concrete, machine-applicable fix-it hint attached to a span.
#[derive(Debug, Clone)]
pub struct FixitHint {
    /// Human-readable explanation, e.g. "Did you mean 'Foo'?"
    pub message: String,
    /// Location in the original source the hint applies to.
    pub span: Span,
    /// The replacement text (empty for `Delete`).
    pub replacement: String,
    /// Whether this is a replace, insert, or delete.
    pub kind: FixitKind,
}

// ── Levenshtein distance ───────────────────────────────────────────

/// Classic Levenshtein (edit) distance between two strings.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

// ── Name collection helpers ────────────────────────────────────────

/// Scan source for identifier-like tokens to use as candidates for
/// fuzzy matching.  This is intentionally lightweight: we split on
/// whitespace/punctuation and keep tokens that look like identifiers.
fn collect_names(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut buf = String::new();
    for ch in source.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            buf.push(ch);
        } else {
            if buf.len() >= 2 {
                names.push(buf.clone());
            }
            buf.clear();
        }
    }
    if buf.len() >= 2 {
        names.push(buf);
    }
    // Deduplicate while preserving first-occurrence order.
    let mut seen = std::collections::HashSet::new();
    names.retain(|n| seen.insert(n.clone()));
    names
}

/// Find the best fuzzy match for `name` among `candidates` with edit
/// distance ≤ `max_dist`.  Returns `None` if nothing is close enough.
fn best_match<'a>(name: &str, candidates: &'a [String], max_dist: usize) -> Option<&'a str> {
    candidates
        .iter()
        .filter(|c| c.as_str() != name)
        .filter_map(|c| {
            let d = levenshtein(name, c);
            if d > 0 && d <= max_dist {
                Some((d, c.as_str()))
            } else {
                None
            }
        })
        .min_by_key(|(d, _)| *d)
        .map(|(_, s)| s)
}

/// Check for a case-only mismatch (e.g. "foo" vs "Foo") among candidates.
fn case_match<'a>(name: &str, candidates: &'a [String]) -> Option<&'a str> {
    let lower = name.to_lowercase();
    candidates
        .iter()
        .find(|c| c.to_lowercase() == lower && c.as_str() != name)
        .map(|c| c.as_str())
}

/// Build a `Span` covering a given 1-based `line` and `col` with `len`
/// bytes.  `start`/`end` offsets are approximate for display purposes.
fn span_at(line: usize, col: usize, len: usize) -> Span {
    Span {
        start: 0,
        end: len,
        line,
        col,
    }
}

// ── Core suggestion engine ─────────────────────────────────────────

/// Given a compile error and the full source text, produce zero or more
/// fix-it hints.
pub fn suggest_fixit(error: &CompileError, source: &str) -> Vec<FixitHint> {
    match error {
        CompileError::Lex(e) => suggest_lex(e),
        CompileError::Parse(errors) => errors.iter().flat_map(suggest_parse).collect(),
        CompileError::Resolve(errors) => errors
            .iter()
            .flat_map(|e| suggest_resolve(e, source))
            .collect(),
        CompileError::Type(errors) => errors
            .iter()
            .flat_map(|e| suggest_type(e, source))
            .collect(),
        CompileError::Constraint(errors) => errors.iter().flat_map(suggest_constraint).collect(),
        CompileError::Ownership(errors) => errors.iter().flat_map(suggest_ownership).collect(),
        CompileError::Lower(_) => vec![],
        CompileError::Multiple(errors) => errors
            .iter()
            .flat_map(|e| suggest_fixit(e, source))
            .collect(),
        CompileError::Typestate(_) | CompileError::Session(_) => vec![],
    }
}

// ── Per-phase suggestion generators ────────────────────────────────

fn suggest_lex(error: &LexError) -> Vec<FixitHint> {
    match error {
        LexError::UnterminatedString { line, col } => vec![FixitHint {
            message: "Add a closing '\"' to terminate the string".to_string(),
            span: span_at(*line, *col, 1),
            replacement: "\"".to_string(),
            kind: FixitKind::Insert,
        }],
        _ => vec![],
    }
}

fn suggest_parse(error: &ParseError) -> Vec<FixitHint> {
    match error {
        ParseError::MissingEnd {
            construct,
            current_line,
            current_col,
            ..
        } => vec![FixitHint {
            message: format!("Add 'end' to close '{}'", construct),
            span: span_at(*current_line, *current_col, 0),
            replacement: "end".to_string(),
            kind: FixitKind::Insert,
        }],
        ParseError::UnclosedBracket {
            bracket,
            current_line,
            current_col,
            ..
        } => {
            let close = match bracket {
                '(' => ')',
                '[' => ']',
                '{' => '}',
                other => *other,
            };
            vec![FixitHint {
                message: format!("Add closing '{}'", close),
                span: span_at(*current_line, *current_col, 0),
                replacement: close.to_string(),
                kind: FixitKind::Insert,
            }]
        }
        _ => vec![],
    }
}

fn suggest_resolve(error: &ResolveError, source: &str) -> Vec<FixitHint> {
    let names = collect_names(source);

    match error {
        ResolveError::UndefinedType {
            name,
            line,
            suggestions,
            ..
        } => {
            // Prefer the resolver's own suggestions first.
            if let Some(s) = suggestions.first() {
                return vec![FixitHint {
                    message: format!("Did you mean '{}'?", s),
                    span: span_at(*line, 1, name.len()),
                    replacement: s.clone(),
                    kind: FixitKind::Replace,
                }];
            }
            // Case mismatch.
            if let Some(m) = case_match(name, &names) {
                return vec![FixitHint {
                    message: format!("Did you mean '{}' (case mismatch)?", m),
                    span: span_at(*line, 1, name.len()),
                    replacement: m.to_string(),
                    kind: FixitKind::Replace,
                }];
            }
            // Fuzzy.
            if let Some(m) = best_match(name, &names, 2) {
                return vec![FixitHint {
                    message: format!("Did you mean '{}'?", m),
                    span: span_at(*line, 1, name.len()),
                    replacement: m.to_string(),
                    kind: FixitKind::Replace,
                }];
            }
            vec![]
        }

        ResolveError::UndefinedCell {
            name,
            line,
            suggestions,
            ..
        } => {
            if let Some(s) = suggestions.first() {
                return vec![FixitHint {
                    message: format!("Did you mean '{}'?", s),
                    span: span_at(*line, 1, name.len()),
                    replacement: s.clone(),
                    kind: FixitKind::Replace,
                }];
            }
            if let Some(m) = case_match(name, &names) {
                return vec![FixitHint {
                    message: format!("Did you mean '{}' (case mismatch)?", m),
                    span: span_at(*line, 1, name.len()),
                    replacement: m.to_string(),
                    kind: FixitKind::Replace,
                }];
            }
            if let Some(m) = best_match(name, &names, 2) {
                return vec![FixitHint {
                    message: format!("Did you mean '{}'?", m),
                    span: span_at(*line, 1, name.len()),
                    replacement: m.to_string(),
                    kind: FixitKind::Replace,
                }];
            }
            vec![]
        }

        ResolveError::UndeclaredEffect {
            cell, effect, line, ..
        } => vec![FixitHint {
            message: format!(
                "Add effect to cell signature: `cell {}(...) -> T / {{{}}}`",
                cell, effect
            ),
            span: span_at(*line, 1, 0),
            replacement: format!(" / {{{}}}", effect),
            kind: FixitKind::Insert,
        }],

        _ => vec![],
    }
}

fn suggest_type(error: &TypeError, source: &str) -> Vec<FixitHint> {
    let names = collect_names(source);

    match error {
        TypeError::Mismatch {
            expected,
            actual,
            line,
        } => {
            // Suggest conversion builtins for common type mismatches.
            let conversion = match (expected.as_str(), actual.as_str()) {
                ("Int", "String") | ("Int", "string") => Some("to_int()"),
                ("Float", "String") | ("Float", "string") => Some("to_float()"),
                ("String", "Int") | ("String", "int") => Some("to_string()"),
                ("String", "Float") | ("String", "float") => Some("to_string()"),
                ("String", "Bool") | ("String", "bool") => Some("to_string()"),
                ("Int", "Float") | ("Int", "float") => Some("to_int()"),
                ("Float", "Int") | ("Float", "int") => Some("to_float()"),
                _ => None,
            };
            if let Some(func) = conversion {
                return vec![FixitHint {
                    message: format!(
                        "Consider using '{}' to convert from {} to {}",
                        func, actual, expected
                    ),
                    span: span_at(*line, 1, 0),
                    replacement: func.to_string(),
                    kind: FixitKind::Insert,
                }];
            }
            vec![]
        }

        TypeError::UndefinedVar { name, line } => {
            // Case mismatch.
            if let Some(m) = case_match(name, &names) {
                return vec![FixitHint {
                    message: format!("Did you mean '{}' (case mismatch)?", m),
                    span: span_at(*line, 1, name.len()),
                    replacement: m.to_string(),
                    kind: FixitKind::Replace,
                }];
            }
            // Fuzzy.
            if let Some(m) = best_match(name, &names, 2) {
                return vec![FixitHint {
                    message: format!("Did you mean '{}'?", m),
                    span: span_at(*line, 1, name.len()),
                    replacement: m.to_string(),
                    kind: FixitKind::Replace,
                }];
            }
            vec![]
        }

        TypeError::UnknownField {
            field,
            line,
            suggestions,
            ..
        } => {
            if let Some(s) = suggestions.first() {
                return vec![FixitHint {
                    message: format!("Did you mean '{}'?", s),
                    span: span_at(*line, 1, field.len()),
                    replacement: s.clone(),
                    kind: FixitKind::Replace,
                }];
            }
            vec![]
        }

        TypeError::IncompleteMatch { missing, line, .. } => {
            let arms = missing
                .iter()
                .map(|v| format!("  {} -> ...", v))
                .collect::<Vec<_>>()
                .join("\n");
            vec![FixitHint {
                message: format!(
                    "Add missing match arm{}: `{}`",
                    if missing.len() > 1 { "s" } else { "" },
                    missing.join(", ")
                ),
                span: span_at(*line, 1, 0),
                replacement: arms,
                kind: FixitKind::Insert,
            }]
        }

        TypeError::ImmutableAssign { name, line } => vec![FixitHint {
            message: format!("Declare '{}' as mutable: `let mut {}`", name, name),
            span: span_at(*line, 1, 0),
            replacement: format!("let mut {}", name),
            kind: FixitKind::Replace,
        }],

        _ => vec![],
    }
}

fn suggest_constraint(_error: &ConstraintError) -> Vec<FixitHint> {
    // Constraint errors are too domain-specific for generic suggestions.
    vec![]
}

fn suggest_ownership(error: &OwnershipError) -> Vec<FixitHint> {
    match error {
        OwnershipError::UseAfterMove {
            variable, used_at, ..
        } => vec![FixitHint {
            message: format!("Consider cloning '{}' before the move", variable),
            span: *used_at,
            replacement: format!("clone({})", variable),
            kind: FixitKind::Replace,
        }],
        _ => vec![],
    }
}

// ── Integration: render hints as text lines ────────────────────────

/// Render a list of fix-it hints as human-readable "Hint: ..." lines
/// suitable for appending to diagnostic output.
pub fn render_hints(hints: &[FixitHint]) -> Vec<String> {
    hints
        .iter()
        .map(|h| format!("Hint: {}", h.message))
        .collect()
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Levenshtein ────────────────────────────────────────────────

    #[test]
    fn test_levenshtein_identical() {
        assert_eq!(levenshtein("hello", "hello"), 0);
    }

    #[test]
    fn test_levenshtein_empty() {
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", ""), 0);
    }

    #[test]
    fn test_levenshtein_basic() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("abc", "abd"), 1);
        assert_eq!(levenshtein("abc", "abcd"), 1);
    }

    // ── collect_names ──────────────────────────────────────────────

    #[test]
    fn test_collect_names_dedup() {
        let src = "cell foo() -> Int\n  let bar = foo()\nend";
        let names = collect_names(src);
        assert!(names.contains(&"foo".to_string()));
        assert!(names.contains(&"bar".to_string()));
        // foo should appear only once despite two occurrences in source
        assert_eq!(names.iter().filter(|n| *n == "foo").count(), 1);
    }

    // ── case_match / best_match ────────────────────────────────────

    #[test]
    fn test_case_match_found() {
        let candidates = vec!["Foo".to_string(), "bar".to_string()];
        assert_eq!(case_match("foo", &candidates), Some("Foo"));
    }

    #[test]
    fn test_case_match_no_match() {
        let candidates = vec!["baz".to_string()];
        assert_eq!(case_match("foo", &candidates), None);
    }

    #[test]
    fn test_best_match_within_distance() {
        let candidates = vec!["bar".to_string(), "baz".to_string(), "xyz".to_string()];
        assert_eq!(best_match("bat", &candidates, 2), Some("bar"));
    }

    #[test]
    fn test_best_match_too_far() {
        let candidates = vec!["abcdef".to_string()];
        assert_eq!(best_match("z", &candidates, 2), None);
    }

    // ── suggest_fixit on UndefinedVar with case mismatch ───────────

    #[test]
    fn test_fixit_undefined_var_case_mismatch() {
        let src = "cell main() -> Int\n  let Foo = 1\n  return foo\nend";
        let err = CompileError::Type(vec![TypeError::UndefinedVar {
            name: "foo".into(),
            line: 3,
        }]);
        let hints = suggest_fixit(&err, src);
        assert!(!hints.is_empty(), "expected at least one fixit hint");
        assert!(hints[0].message.contains("Foo"));
        assert_eq!(hints[0].kind, FixitKind::Replace);
    }

    // ── suggest_fixit on UndefinedVar with fuzzy match ─────────────

    #[test]
    fn test_fixit_undefined_var_fuzzy() {
        let src = "cell main() -> Int\n  let count = 1\n  return cunt\nend";
        let err = CompileError::Type(vec![TypeError::UndefinedVar {
            name: "cunt".into(),
            line: 3,
        }]);
        let hints = suggest_fixit(&err, src);
        assert!(!hints.is_empty(), "expected a fuzzy match hint");
        assert!(hints[0].message.contains("count"), "should suggest 'count'");
    }

    // ── suggest_fixit on TypeMismatch ──────────────────────────────

    #[test]
    fn test_fixit_type_mismatch_int_string() {
        let err = CompileError::Type(vec![TypeError::Mismatch {
            expected: "Int".into(),
            actual: "String".into(),
            line: 5,
        }]);
        let hints = suggest_fixit(&err, "");
        assert!(!hints.is_empty());
        assert!(hints[0].message.contains("to_int()"));
    }

    #[test]
    fn test_fixit_type_mismatch_string_int() {
        let err = CompileError::Type(vec![TypeError::Mismatch {
            expected: "String".into(),
            actual: "Int".into(),
            line: 5,
        }]);
        let hints = suggest_fixit(&err, "");
        assert!(!hints.is_empty());
        assert!(hints[0].message.contains("to_string()"));
    }

    // ── suggest_fixit on IncompleteMatch ───────────────────────────

    #[test]
    fn test_fixit_incomplete_match() {
        let err = CompileError::Type(vec![TypeError::IncompleteMatch {
            enum_name: "Color".into(),
            missing: vec!["Red".into(), "Blue".into()],
            line: 10,
        }]);
        let hints = suggest_fixit(&err, "");
        assert!(!hints.is_empty());
        assert!(hints[0].message.contains("Red"));
        assert!(hints[0].message.contains("Blue"));
        assert_eq!(hints[0].kind, FixitKind::Insert);
    }

    // ── suggest_fixit on UndeclaredEffect ──────────────────────────

    #[test]
    fn test_fixit_undeclared_effect() {
        let err = CompileError::Resolve(vec![ResolveError::UndeclaredEffect {
            cell: "fetch_data".into(),
            effect: "http".into(),
            line: 3,
            cause: "tool call".into(),
        }]);
        let hints = suggest_fixit(&err, "");
        assert!(!hints.is_empty());
        assert!(hints[0].message.contains("http"));
        assert!(hints[0].message.contains("cell signature"));
    }

    // ── render_hints ───────────────────────────────────────────────

    #[test]
    fn test_render_hints_empty() {
        assert!(render_hints(&[]).is_empty());
    }

    #[test]
    fn test_render_hints_format() {
        let hints = vec![FixitHint {
            message: "Did you mean 'Foo'?".into(),
            span: span_at(1, 1, 3),
            replacement: "Foo".into(),
            kind: FixitKind::Replace,
        }];
        let lines = render_hints(&hints);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("Hint: "));
        assert!(lines[0].contains("Foo"));
    }

    // ── suggest_fixit on ImmutableAssign ───────────────────────────

    #[test]
    fn test_fixit_immutable_assign() {
        let err = CompileError::Type(vec![TypeError::ImmutableAssign {
            name: "counter".into(),
            line: 7,
        }]);
        let hints = suggest_fixit(&err, "");
        assert!(!hints.is_empty());
        assert!(hints[0].message.contains("let mut"));
    }

    // ── suggest_fixit on ownership UseAfterMove ────────────────────

    #[test]
    fn test_fixit_use_after_move() {
        let err = CompileError::Ownership(vec![OwnershipError::UseAfterMove {
            variable: "data".into(),
            moved_at: Span::new(0, 4, 2, 1),
            used_at: Span::new(10, 14, 5, 1),
        }]);
        let hints = suggest_fixit(&err, "");
        assert!(!hints.is_empty());
        assert!(
            hints[0].message.contains("cloning"),
            "expected 'cloning' in message: {}",
            hints[0].message
        );
    }

    // ── suggest_fixit returns empty for Lower ──────────────────────

    #[test]
    fn test_fixit_lower_no_hints() {
        let err = CompileError::Lower("internal".into());
        let hints = suggest_fixit(&err, "");
        assert!(hints.is_empty());
    }

    // ── Multiple errors produce combined hints ─────────────────────

    #[test]
    fn test_fixit_multiple_errors() {
        let err = CompileError::Multiple(vec![
            CompileError::Type(vec![TypeError::Mismatch {
                expected: "Int".into(),
                actual: "String".into(),
                line: 1,
            }]),
            CompileError::Type(vec![TypeError::Mismatch {
                expected: "String".into(),
                actual: "Float".into(),
                line: 2,
            }]),
        ]);
        let hints = suggest_fixit(&err, "");
        assert_eq!(hints.len(), 2);
    }
}
