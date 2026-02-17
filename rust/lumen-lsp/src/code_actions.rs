//! Code actions (quick fixes) for LSP diagnostics
//!
//! Provides code actions including:
//! - Add missing 'end' keyword
//! - Add closing markdown fence
//! - Fix typos ("Did you mean X?")
//! - Add missing match arm (when match is non-exhaustive)
//! - Add import (when an unresolved name matches a known symbol)

use lsp_types::{
    CodeAction, CodeActionKind, Diagnostic, Position, Range, TextEdit, Uri, WorkspaceEdit,
};

/// Build code actions for the given diagnostics and document.
pub fn build_code_actions(
    uri: &Uri,
    text: &str,
    context_diagnostics: &[Diagnostic],
) -> Vec<CodeAction> {
    let mut actions = Vec::new();

    for diag in context_diagnostics {
        if diag.source.as_deref() != Some("lumen") {
            continue;
        }

        // "unexpected end of input (check for missing 'end' keywords)" -> Add end
        if diag.message.contains("missing 'end'")
            || diag.message.contains("unexpected end of input")
        {
            if let Some(edit) = insert_at_end(text, "end\n") {
                actions.push(CodeAction {
                    title: "Add missing 'end'".to_string(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![diag.clone()]),
                    edit: Some(WorkspaceEdit {
                        changes: Some([(uri.clone(), vec![edit])].into_iter().collect()),
                        document_changes: None,
                        change_annotations: None,
                    }),
                    command: None,
                    is_preferred: Some(true),
                    disabled: None,
                    data: None,
                });
            }
        }

        // "unterminated markdown block (add closing ```)" -> Add closing ```
        if diag.message.contains("unterminated markdown block") {
            if let Some(edit) = insert_at_end(text, "```\n") {
                actions.push(CodeAction {
                    title: "Add closing ```".to_string(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![diag.clone()]),
                    edit: Some(WorkspaceEdit {
                        changes: Some([(uri.clone(), vec![edit])].into_iter().collect()),
                        document_changes: None,
                        change_annotations: None,
                    }),
                    command: None,
                    is_preferred: Some(true),
                    disabled: None,
                    data: None,
                });
            }
        }

        // "Did you mean `X`?" typos
        if let Some(suggestion) = parse_did_you_mean(&diag.message) {
            // We need to find the range to replace. The diagnostic range covers the wrong word.
            actions.push(CodeAction {
                title: format!("Change to `{}`", suggestion),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![diag.clone()]),
                edit: Some(WorkspaceEdit {
                    changes: Some(
                        [(
                            uri.clone(),
                            vec![TextEdit {
                                range: diag.range,
                                new_text: suggestion.to_string(),
                            }],
                        )]
                        .into_iter()
                        .collect(),
                    ),
                    document_changes: None,
                    change_annotations: None,
                }),
                command: None,
                is_preferred: Some(true),
                disabled: None,
                data: None,
            });
        }

        // "Incomplete match" / missing match arms
        if let Some(action) = build_add_missing_match_arm(uri, text, diag) {
            actions.push(action);
        }

        // "Undefined symbol" / unresolved name → suggest adding an import
        if let Some(action) = build_add_import(uri, text, diag) {
            actions.push(action);
        }
    }

    actions
}

/// Build a "Add missing match arm" code action.
///
/// Triggers on diagnostics that mention incomplete match or missing variants.
/// Parses the diagnostic message to extract missing variant names and inserts
/// a wildcard or specific variant arm after the last existing arm.
fn build_add_missing_match_arm(uri: &Uri, text: &str, diag: &Diagnostic) -> Option<CodeAction> {
    // Match diagnostics like:
    //   "Incomplete match: missing variants: Foo, Bar"
    //   "Non-exhaustive match: missing variant `Baz`"
    let missing_variants = parse_missing_variants(&diag.message)?;
    if missing_variants.is_empty() {
        return None;
    }

    // Build the arm text for each missing variant
    let arms_text: String = missing_variants
        .iter()
        .map(|v| format!("  {} -> halt \"TODO: handle {}\"\n", v, v))
        .collect();

    // Find where to insert: look for the match's `end` keyword.
    // The diagnostic range points to the match statement. We look for the
    // `end` keyword on or after the diagnostic line.
    let insert_position = find_match_end_insert_position(text, diag.range.start.line)?;

    let edit = TextEdit {
        range: Range {
            start: insert_position,
            end: insert_position,
        },
        new_text: arms_text,
    };

    Some(CodeAction {
        title: format!(
            "Add missing match arm{}",
            if missing_variants.len() > 1 { "s" } else { "" }
        ),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diag.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some([(uri.clone(), vec![edit])].into_iter().collect()),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    })
}

/// Build an "Add import" code action.
///
/// Triggers on diagnostics about undefined/unresolved symbols when the symbol
/// name looks like it could be imported (starts with uppercase = type/module).
fn build_add_import(uri: &Uri, text: &str, diag: &Diagnostic) -> Option<CodeAction> {
    let unresolved_name = parse_unresolved_name(&diag.message)?;

    // Only suggest imports for names that look like types or modules (capitalized)
    if unresolved_name.is_empty() {
        return None;
    }

    // Build the import statement. We guess a module name from the symbol name
    // by converting CamelCase to snake_case as a heuristic.
    let module_name = camel_to_snake(&unresolved_name);

    let import_line = format!("import {}: {}\n", module_name, unresolved_name);

    // Insert at the top of the file (after any existing imports or directives)
    let insert_line = find_import_insert_line(text);

    let edit = TextEdit {
        range: Range {
            start: Position {
                line: insert_line,
                character: 0,
            },
            end: Position {
                line: insert_line,
                character: 0,
            },
        },
        new_text: import_line.clone(),
    };

    Some(CodeAction {
        title: format!("Add `{}`", import_line.trim()),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diag.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some([(uri.clone(), vec![edit])].into_iter().collect()),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    })
}

/// Parse missing variant names from diagnostic messages.
///
/// Handles formats like:
/// - "Incomplete match: missing variants: Foo, Bar"
/// - "Incomplete match on `Color`: missing variants `Red`, `Blue`"
/// - "Non-exhaustive match: missing variant `Baz`"
fn parse_missing_variants(message: &str) -> Option<Vec<String>> {
    // Pattern 1: "missing variants: Foo, Bar"
    if let Some(idx) = message.find("missing variants:") {
        let after = &message[idx + "missing variants:".len()..];
        let variants: Vec<String> = after
            .split(',')
            .map(|s| s.trim().trim_matches('`').trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !variants.is_empty() {
            return Some(variants);
        }
    }

    // Pattern 2: "missing variants `Foo`, `Bar`"
    if let Some(idx) = message.find("missing variant") {
        let after = &message[idx..];
        let mut variants = Vec::new();
        let mut search = after;
        while let Some(start) = search.find('`') {
            let rest = &search[start + 1..];
            if let Some(end) = rest.find('`') {
                let name = rest[..end].trim().to_string();
                if !name.is_empty() {
                    variants.push(name);
                }
                search = &rest[end + 1..];
            } else {
                break;
            }
        }
        if !variants.is_empty() {
            return Some(variants);
        }
    }

    // Pattern 3: "IncompleteMatch" error code with backtick-quoted names
    if message.contains("IncompleteMatch") || message.contains("incomplete match") {
        let mut variants = Vec::new();
        let bytes = message.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'`' {
                if let Some(end) = message[i + 1..].find('`') {
                    let name = &message[i + 1..i + 1 + end];
                    if !name.is_empty() && name.chars().next().is_some_and(|c| c.is_uppercase()) {
                        variants.push(name.to_string());
                    }
                    i = i + 1 + end + 1;
                    continue;
                }
            }
            i += 1;
        }
        if !variants.is_empty() {
            return Some(variants);
        }
    }

    None
}

/// Parse an unresolved symbol name from a diagnostic message.
///
/// Handles formats like:
/// - "Undefined symbol `Foo`"
/// - "Unknown type `Bar`"
/// - "Unresolved name: `Baz`"
/// - "undefined name 'Qux'"
fn parse_unresolved_name(message: &str) -> Option<String> {
    let patterns = [
        "Undefined symbol `",
        "Unknown type `",
        "Unresolved name: `",
        "Unresolved name `",
        "undefined name `",
        "unknown type `",
        "Undefined name `",
        "Undefined type `",
    ];

    for pattern in &patterns {
        if let Some(idx) = message.find(pattern) {
            let after = &message[idx + pattern.len()..];
            if let Some(end) = after.find('`') {
                let name = after[..end].trim().to_string();
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
    }

    // Also check single-quote variants
    let sq_patterns = ["Undefined symbol '", "Unknown type '", "undefined name '"];
    for pattern in &sq_patterns {
        if let Some(idx) = message.find(pattern) {
            let after = &message[idx + pattern.len()..];
            if let Some(end) = after.find('\'') {
                let name = after[..end].trim().to_string();
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
    }

    None
}

/// Find the line just before the match `end` keyword, where we should insert arms.
fn find_match_end_insert_position(text: &str, match_start_line: u32) -> Option<Position> {
    let lines: Vec<&str> = text.lines().collect();
    let start = match_start_line as usize;

    // Search forward from the match statement for the `end` keyword
    for (i, line) in lines.iter().enumerate().skip(start) {
        let trimmed = line.trim();
        if trimmed == "end" || trimmed.starts_with("end ") || trimmed.starts_with("end\t") {
            // Insert just before the `end` line
            return Some(Position {
                line: i as u32,
                character: 0,
            });
        }
    }
    None
}

/// Find the line where a new import should be inserted.
/// Returns the line number after the last existing import or directive, or 0.
fn find_import_insert_line(text: &str) -> u32 {
    let mut last_import_or_directive: Option<u32> = None;

    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") || trimmed.starts_with("@") {
            last_import_or_directive = Some(i as u32);
        } else if !trimmed.is_empty()
            && !trimmed.starts_with('#')
            && !trimmed.starts_with("//")
            && last_import_or_directive.is_some()
        {
            // Once we hit non-import, non-directive content, stop
            break;
        }
    }

    match last_import_or_directive {
        Some(line) => line + 1,
        None => 0,
    }
}

/// Convert CamelCase to snake_case for module name guessing.
fn camel_to_snake(name: &str) -> String {
    let mut result = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(ch.to_lowercase().next().unwrap_or(ch));
    }
    result
}

fn parse_did_you_mean(message: &str) -> Option<&str> {
    // Format: "... Hint: Did you mean `suggestion`?"
    // or "... Did you mean `suggestion`?"
    if let Some(idx) = message.find("Did you mean `") {
        let after = &message[idx + "Did you mean `".len()..];
        if let Some(end) = after.find('`') {
            return Some(&after[..end]);
        }
    }
    None
}

fn insert_at_end(text: &str, to_insert: &str) -> Option<TextEdit> {
    let lines: Vec<&str> = text.split('\n').collect();
    let line_count = lines.len().saturating_sub(1) as u32;
    let last_line = lines.last().unwrap_or(&"");
    let character = last_line.len() as u32;

    Some(TextEdit {
        range: Range {
            start: Position {
                line: line_count,
                character,
            },
            end: Position {
                line: line_count,
                character,
            },
        },
        new_text: to_insert.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_uri() -> Uri {
        "file:///test.lm".parse().unwrap()
    }

    fn make_diagnostic(message: &str) -> Diagnostic {
        Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 10,
                },
            },
            severity: None,
            code: None,
            code_description: None,
            source: Some("lumen".to_string()),
            message: message.to_string(),
            related_information: None,
            tags: None,
            data: None,
        }
    }

    #[test]
    fn test_parse_did_you_mean() {
        assert_eq!(
            parse_did_you_mean("Some error. Hint: Did you mean `foo`?"),
            Some("foo")
        );
        assert_eq!(
            parse_did_you_mean("Error: Did you mean `bar`?"),
            Some("bar")
        );
        assert_eq!(parse_did_you_mean("No hint here."), None);
        assert_eq!(parse_did_you_mean("Did you mean `broken"), None);
    }

    #[test]
    fn test_parse_missing_variants_colon_format() {
        let msg = "Incomplete match: missing variants: Red, Green, Blue";
        let variants = parse_missing_variants(msg).unwrap();
        assert_eq!(variants, vec!["Red", "Green", "Blue"]);
    }

    #[test]
    fn test_parse_missing_variants_backtick_format() {
        let msg = "Non-exhaustive match: missing variant `None`";
        let variants = parse_missing_variants(msg).unwrap();
        assert_eq!(variants, vec!["None"]);
    }

    #[test]
    fn test_parse_missing_variants_multiple_backtick() {
        let msg = "Incomplete match on `Shape`: missing variants `Circle`, `Triangle`";
        let variants = parse_missing_variants(msg).unwrap();
        assert!(variants.contains(&"Circle".to_string()));
        assert!(variants.contains(&"Triangle".to_string()));
    }

    #[test]
    fn test_parse_missing_variants_no_match() {
        assert!(parse_missing_variants("type error: Int vs String").is_none());
    }

    #[test]
    fn test_add_missing_match_arm_action() {
        let text = "cell main() -> Int\n  match color\n    Red -> 1\n  end\n  return 0\nend";
        let uri = make_uri();
        let diag = Diagnostic {
            range: Range {
                start: Position {
                    line: 1,
                    character: 2,
                },
                end: Position {
                    line: 1,
                    character: 14,
                },
            },
            source: Some("lumen".to_string()),
            message: "Incomplete match: missing variants: Green, Blue".to_string(),
            ..make_diagnostic("")
        };

        let actions = build_code_actions(&uri, text, &[diag]);
        let match_action = actions
            .iter()
            .find(|a| a.title.contains("missing match arm"));
        assert!(
            match_action.is_some(),
            "Should produce a missing match arm action"
        );

        let action = match_action.unwrap();
        let ws_edit = action.edit.as_ref().unwrap();
        let edits = ws_edit.changes.as_ref().unwrap().get(&uri).unwrap();
        assert_eq!(edits.len(), 1);
        let edit_text = &edits[0].new_text;
        assert!(edit_text.contains("Green"), "Edit should mention Green");
        assert!(edit_text.contains("Blue"), "Edit should mention Blue");
    }

    #[test]
    fn test_parse_unresolved_name() {
        assert_eq!(
            parse_unresolved_name("Undefined symbol `MyType`"),
            Some("MyType".to_string())
        );
        assert_eq!(
            parse_unresolved_name("Unknown type `HttpClient`"),
            Some("HttpClient".to_string())
        );
        assert_eq!(parse_unresolved_name("some other error"), None);
    }

    #[test]
    fn test_add_import_action() {
        let text = "cell main() -> Int\n  let x = HttpClient()\n  return 0\nend";
        let uri = make_uri();
        let diag = make_diagnostic("Undefined symbol `HttpClient`");

        let actions = build_code_actions(&uri, text, &[diag]);
        let import_action = actions.iter().find(|a| a.title.contains("import"));
        assert!(import_action.is_some(), "Should produce an import action");

        let action = import_action.unwrap();
        let ws_edit = action.edit.as_ref().unwrap();
        let edits = ws_edit.changes.as_ref().unwrap().get(&uri).unwrap();
        assert_eq!(edits.len(), 1);
        assert!(
            edits[0].new_text.contains("HttpClient"),
            "Import should reference HttpClient"
        );
        assert!(
            edits[0].new_text.contains("import"),
            "Should contain import keyword"
        );
    }

    #[test]
    fn test_camel_to_snake() {
        assert_eq!(camel_to_snake("HttpClient"), "http_client");
        assert_eq!(camel_to_snake("Foo"), "foo");
        assert_eq!(camel_to_snake("FooBarBaz"), "foo_bar_baz");
        assert_eq!(camel_to_snake("X"), "x");
    }

    #[test]
    fn test_find_import_insert_line() {
        let text = "import foo: Bar\nimport baz: Qux\n\ncell main() -> Int\nend";
        assert_eq!(find_import_insert_line(text), 2);

        let text_no_imports = "cell main() -> Int\nend";
        assert_eq!(find_import_insert_line(text_no_imports), 0);
    }

    #[test]
    fn test_non_lumen_diagnostic_ignored() {
        let uri = make_uri();
        let diag = Diagnostic {
            source: Some("other-tool".to_string()),
            message: "Undefined symbol `Foo`".to_string(),
            ..make_diagnostic("")
        };
        let actions = build_code_actions(&uri, "cell main() -> Int\nend", &[diag]);
        assert!(
            actions.is_empty(),
            "Non-lumen diagnostics should be ignored"
        );
    }
}
