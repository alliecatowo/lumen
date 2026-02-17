//! LSP diagnostics from compiler errors

use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use lumen_compiler::compiler::constraints::ConstraintError;
use lumen_compiler::compiler::lexer::LexError;
use lumen_compiler::compiler::ownership::OwnershipError;
use lumen_compiler::compiler::parser::ParseError;
use lumen_compiler::compiler::resolve::ResolveError;
use lumen_compiler::compiler::typecheck::TypeError;
use lumen_compiler::CompileError;

/// Convert a compile error into LSP diagnostics
pub fn compile_error_to_diagnostics(error: &CompileError, _source: &str) -> Vec<Diagnostic> {
    match error {
        CompileError::Lex(e) => vec![lex_error_to_diagnostic(e)],
        CompileError::Parse(errors) => errors.iter().map(parse_error_to_diagnostic).collect(),
        CompileError::Resolve(errors) => errors.iter().map(resolve_error_to_diagnostic).collect(),
        CompileError::Type(errors) => errors.iter().map(type_error_to_diagnostic).collect(),
        CompileError::Constraint(errors) => {
            errors.iter().map(constraint_error_to_diagnostic).collect()
        }
        CompileError::Ownership(errors) => {
            errors.iter().map(ownership_error_to_diagnostic).collect()
        }
        CompileError::Multiple(errors) => errors
            .iter()
            .flat_map(|e| compile_error_to_diagnostics(e, _source))
            .collect(),
    }
}

fn lex_error_to_diagnostic(error: &LexError) -> Diagnostic {
    match error {
        LexError::UnexpectedChar { ch, line, col } => {
            let line_zero = line.saturating_sub(1) as u32;
            let col_zero = col.saturating_sub(1) as u32;

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: col_zero,
                    },
                    end: Position {
                        line: line_zero,
                        character: col_zero + 1,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E001".to_string())),
                source: Some("lumen".to_string()),
                message: format!("unexpected character '{}'", ch),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        LexError::UnterminatedString { line, col } => {
            let line_zero = line.saturating_sub(1) as u32;
            let col_zero = col.saturating_sub(1) as u32;

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: col_zero,
                    },
                    end: Position {
                        line: line_zero,
                        character: col_zero + 10, // Highlight a reasonable amount
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E002".to_string())),
                source: Some("lumen".to_string()),
                message: "unterminated string literal".to_string(),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        LexError::InconsistentIndent { line } => {
            let line_zero = line.saturating_sub(1) as u32;

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: 0,
                    },
                    end: Position {
                        line: line_zero,
                        character: u32::MAX,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E003".to_string())),
                source: Some("lumen".to_string()),
                message: "inconsistent indentation".to_string(),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        LexError::InvalidNumber { line, col } => {
            let line_zero = line.saturating_sub(1) as u32;
            let col_zero = col.saturating_sub(1) as u32;

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: col_zero,
                    },
                    end: Position {
                        line: line_zero,
                        character: col_zero + 5, // Approximate
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E004".to_string())),
                source: Some("lumen".to_string()),
                message: "invalid number literal".to_string(),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        LexError::InvalidBytesLiteral { line, col } => {
            let line_zero = line.saturating_sub(1) as u32;
            let col_zero = col.saturating_sub(1) as u32;

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: col_zero,
                    },
                    end: Position {
                        line: line_zero,
                        character: col_zero + 5,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E005".to_string())),
                source: Some("lumen".to_string()),
                message: "invalid bytes literal (must be hex: b\"48656c6c6f\")".to_string(),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        LexError::InvalidUnicodeEscape { line, col } => {
            let line_zero = line.saturating_sub(1) as u32;
            let col_zero = col.saturating_sub(1) as u32;

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: col_zero,
                    },
                    end: Position {
                        line: line_zero,
                        character: col_zero + 5,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E006".to_string())),
                source: Some("lumen".to_string()),
                message: "invalid unicode escape sequence (use \\u{XXXX})".to_string(),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        LexError::UnterminatedMarkdownBlock { line, col } => {
            let line_zero = line.saturating_sub(1) as u32;
            let col_zero = col.saturating_sub(1) as u32;

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: col_zero,
                    },
                    end: Position {
                        line: line_zero,
                        character: col_zero + 3,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E007".to_string())),
                source: Some("lumen".to_string()),
                message: "unterminated markdown block (add closing ```)".to_string(),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
    }
}

fn parse_error_to_diagnostic(error: &ParseError) -> Diagnostic {
    match error {
        ParseError::Unexpected {
            found,
            expected,
            line,
            col,
        } => {
            let line_zero = line.saturating_sub(1) as u32;
            let col_zero = col.saturating_sub(1) as u32;
            let len = found.len().max(1) as u32;

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: col_zero,
                    },
                    end: Position {
                        line: line_zero,
                        character: col_zero + len,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E010".to_string())),
                source: Some("lumen".to_string()),
                message: format!("unexpected token '{}', expected {}", found, expected),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        ParseError::UnexpectedEof => Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 0,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(lsp_types::NumberOrString::String("E011".to_string())),
            source: Some("lumen".to_string()),
            message: "unexpected end of input (check for missing 'end' keywords)".to_string(),
            related_information: None,
            tags: None,
            code_description: None,
            data: None,
        },
        ParseError::UnclosedBracket {
            bracket,
            open_line,
            open_col,
            current_line,
            current_col,
        } => {
            let line_zero = current_line.saturating_sub(1) as u32;
            let col_zero = current_col.saturating_sub(1) as u32;
            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: col_zero,
                    },
                    end: Position {
                        line: line_zero,
                        character: col_zero + 1,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E012".to_string())),
                source: Some("lumen".to_string()),
                message: format!(
                    "unclosed '{}' opened at line {}, col {}",
                    bracket, open_line, open_col
                ),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        ParseError::MissingEnd {
            construct,
            open_line,
            open_col,
            current_line,
            current_col,
        } => {
            let line_zero = current_line.saturating_sub(1) as u32;
            let col_zero = current_col.saturating_sub(1) as u32;
            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: col_zero,
                    },
                    end: Position {
                        line: line_zero,
                        character: col_zero + 3,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E013".to_string())),
                source: Some("lumen".to_string()),
                message: format!(
                    "expected 'end' to close '{}' at line {}, col {}",
                    construct, open_line, open_col
                ),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        ParseError::MissingType { line, col } => {
            let line_zero = line.saturating_sub(1) as u32;
            let col_zero = col.saturating_sub(1) as u32;
            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: col_zero,
                    },
                    end: Position {
                        line: line_zero,
                        character: col_zero + 1,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E014".to_string())),
                source: Some("lumen".to_string()),
                message: "expected type after ':'".to_string(),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        ParseError::IncompleteExpression { line, col, context } => {
            let line_zero = line.saturating_sub(1) as u32;
            let col_zero = col.saturating_sub(1) as u32;
            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: col_zero,
                    },
                    end: Position {
                        line: line_zero,
                        character: col_zero + 1,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E015".to_string())),
                source: Some("lumen".to_string()),
                message: format!("incomplete expression: {}", context),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        ParseError::MalformedConstruct {
            construct,
            reason,
            line,
            col,
        } => {
            let line_zero = line.saturating_sub(1) as u32;
            let col_zero = col.saturating_sub(1) as u32;
            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: col_zero,
                    },
                    end: Position {
                        line: line_zero,
                        character: col_zero + construct.len() as u32,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E016".to_string())),
                source: Some("lumen".to_string()),
                message: format!("malformed {}: {}", construct, reason),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
    }
}

fn resolve_error_to_diagnostic(error: &ResolveError) -> Diagnostic {
    match error {
        ResolveError::UndefinedType {
            name,
            line,
            suggestions,
        } => {
            let line_zero = line.saturating_sub(1) as u32;
            let mut message = format!("undefined type '{}'", name);
            if !suggestions.is_empty() {
                message.push_str(&format!(
                    " (did you mean {}?)",
                    suggestions
                        .iter()
                        .map(|s| format!("'{}'", s))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: 0,
                    },
                    end: Position {
                        line: line_zero,
                        character: u32::MAX,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E020".to_string())),
                source: Some("lumen".to_string()),
                message,
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        ResolveError::UndefinedCell {
            name,
            line,
            suggestions,
        } => {
            let line_zero = line.saturating_sub(1) as u32;
            let mut message = format!("undefined cell '{}'", name);
            if !suggestions.is_empty() {
                message.push_str(&format!(
                    " (did you mean {}?)",
                    suggestions
                        .iter()
                        .map(|s| format!("'{}'", s))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: 0,
                    },
                    end: Position {
                        line: line_zero,
                        character: u32::MAX,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E021".to_string())),
                source: Some("lumen".to_string()),
                message,
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        ResolveError::UndeclaredEffect {
            cell,
            effect,
            line,
            cause,
        } => {
            let line_zero = line.saturating_sub(1) as u32;
            let mut message = format!(
                "cell '{}' performs effect '{}' but it is not declared in its effect row",
                cell, effect
            );
            if !cause.is_empty() {
                message.push_str(&format!(" ({})", cause));
            }

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: 0,
                    },
                    end: Position {
                        line: line_zero,
                        character: u32::MAX,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E030".to_string())),
                source: Some("lumen".to_string()),
                message,
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        _ => {
            // Fallback for other resolve errors
            Diagnostic {
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: u32::MAX,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E099".to_string())),
                source: Some("lumen".to_string()),
                message: error.to_string(),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
    }
}

fn type_error_to_diagnostic(error: &TypeError) -> Diagnostic {
    match error {
        TypeError::Mismatch {
            expected,
            actual,
            line,
        } => {
            let line_zero = line.saturating_sub(1) as u32;

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: 0,
                    },
                    end: Position {
                        line: line_zero,
                        character: u32::MAX,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E040".to_string())),
                source: Some("lumen".to_string()),
                message: format!("type mismatch: expected {}, got {}", expected, actual),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        TypeError::UndefinedVar { name, line } => {
            let line_zero = line.saturating_sub(1) as u32;

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: 0,
                    },
                    end: Position {
                        line: line_zero,
                        character: u32::MAX,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E041".to_string())),
                source: Some("lumen".to_string()),
                message: format!("undefined variable '{}'", name),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        TypeError::UnknownField {
            field,
            ty,
            line,
            suggestions,
        } => {
            let line_zero = line.saturating_sub(1) as u32;
            let mut message = format!("unknown field '{}' on type '{}'", field, ty);
            if !suggestions.is_empty() {
                message.push_str(&format!(
                    " (did you mean {}?)",
                    suggestions
                        .iter()
                        .map(|s| format!("'{}'", s))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: 0,
                    },
                    end: Position {
                        line: line_zero,
                        character: u32::MAX,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E042".to_string())),
                source: Some("lumen".to_string()),
                message,
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        TypeError::IncompleteMatch {
            enum_name,
            missing,
            line,
        } => {
            let line_zero = line.saturating_sub(1) as u32;
            let missing_list = missing.join(", ");

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: 0,
                    },
                    end: Position {
                        line: line_zero,
                        character: u32::MAX,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E043".to_string())),
                source: Some("lumen".to_string()),
                message: format!(
                    "incomplete match on enum '{}': missing variants [{}]",
                    enum_name, missing_list
                ),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        _ => {
            // Fallback for other type errors
            let line = match error {
                TypeError::NotCallable { line }
                | TypeError::ArgCount { line, .. }
                | TypeError::MissingReturn { line, .. }
                | TypeError::ImmutableAssign { line, .. }
                | TypeError::UndefinedType { line, .. } => *line,
                _ => 1,
            };

            let line_zero = line.saturating_sub(1) as u32;

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: 0,
                    },
                    end: Position {
                        line: line_zero,
                        character: u32::MAX,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E049".to_string())),
                source: Some("lumen".to_string()),
                message: error.to_string(),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
    }
}

fn constraint_error_to_diagnostic(error: &ConstraintError) -> Diagnostic {
    match error {
        ConstraintError::Invalid {
            field,
            line,
            message,
        } => {
            let line_zero = line.saturating_sub(1) as u32;

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: 0,
                    },
                    end: Position {
                        line: line_zero,
                        character: u32::MAX,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E050".to_string())),
                source: Some("lumen".to_string()),
                message: format!("invalid constraint on field '{}': {}", field, message),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
    }
}

fn ownership_error_to_diagnostic(error: &OwnershipError) -> Diagnostic {
    match error {
        OwnershipError::UseAfterMove {
            variable,
            moved_at,
            used_at,
        } => {
            let line_zero = used_at.line.saturating_sub(1) as u32;
            let col_zero = used_at.col.saturating_sub(1) as u32;

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: col_zero,
                    },
                    end: Position {
                        line: line_zero,
                        character: col_zero + variable.len() as u32,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E060".to_string())),
                source: Some("lumen".to_string()),
                message: format!(
                    "use of moved variable '{}' (moved at line {})",
                    variable, moved_at.line
                ),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        OwnershipError::NotConsumed {
            variable,
            declared_at,
        } => {
            let line_zero = declared_at.line.saturating_sub(1) as u32;
            let col_zero = declared_at.col.saturating_sub(1) as u32;

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: col_zero,
                    },
                    end: Position {
                        line: line_zero,
                        character: col_zero + variable.len() as u32,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E061".to_string())),
                source: Some("lumen".to_string()),
                message: format!("owned variable '{}' was never consumed", variable),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        OwnershipError::AlreadyBorrowed {
            variable,
            first_borrow,
            second_borrow,
        } => {
            let line_zero = second_borrow.line.saturating_sub(1) as u32;
            let col_zero = second_borrow.col.saturating_sub(1) as u32;

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: col_zero,
                    },
                    end: Position {
                        line: line_zero,
                        character: col_zero + variable.len() as u32,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E062".to_string())),
                source: Some("lumen".to_string()),
                message: format!(
                    "variable '{}' already borrowed at line {}",
                    variable, first_borrow.line
                ),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
        OwnershipError::MoveWhileBorrowed {
            variable,
            borrow_at,
            move_at,
        } => {
            let line_zero = move_at.line.saturating_sub(1) as u32;
            let col_zero = move_at.col.saturating_sub(1) as u32;

            Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: col_zero,
                    },
                    end: Position {
                        line: line_zero,
                        character: col_zero + variable.len() as u32,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("E063".to_string())),
                source: Some("lumen".to_string()),
                message: format!(
                    "cannot move '{}' while it is borrowed (borrowed at line {})",
                    variable, borrow_at.line
                ),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        }
    }
}
