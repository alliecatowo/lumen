//! Code actions (quick fixes) for LSP diagnostics

use lsp_types::{CodeAction, CodeActionKind, Diagnostic, TextEdit, Uri, WorkspaceEdit};

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
    }

    actions
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
    let line_count = lines.len().saturating_sub(1).max(0) as u32;
    let last_line = lines.last().unwrap_or(&"");
    let character = last_line.len() as u32;

    Some(TextEdit {
        range: lsp_types::Range {
            start: lsp_types::Position {
                line: line_count,
                character,
            },
            end: lsp_types::Position {
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
}
