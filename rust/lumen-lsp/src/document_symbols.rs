//! Document symbol provider — returns outline symbols for the document

use lsp_types::{DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse, Range, SymbolKind};
use lumen_compiler::compiler::ast::{Item, Program};

use crate::hover::type_expr_to_string;

pub fn build_document_symbols(
    _params: DocumentSymbolParams,
    text: &str,
    program: Option<&Program>,
) -> Option<DocumentSymbolResponse> {
    let prog = program?;
    let mut symbols = Vec::new();

    for item in &prog.items {
        match item {
            Item::Cell(cell) => {
                let detail = cell
                    .return_type
                    .as_ref()
                    .map(|t| format!("-> {}", type_expr_to_string(t)));

                let range = span_to_range(&cell.span, text);
                let selection_range = span_to_selection_range(&cell.span);

                #[allow(deprecated)]
                symbols.push(DocumentSymbol {
                    name: cell.name.clone(),
                    detail,
                    kind: SymbolKind::FUNCTION,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range,
                    children: None,
                });
            }
            Item::Record(record) => {
                let range = span_to_range(&record.span, text);
                let selection_range = span_to_selection_range(&record.span);

                let children: Vec<DocumentSymbol> = record
                    .fields
                    .iter()
                    .map(|f| {
                        let field_range = span_to_selection_range(&f.span);
                        #[allow(deprecated)]
                        DocumentSymbol {
                            name: f.name.clone(),
                            detail: Some(type_expr_to_string(&f.ty)),
                            kind: SymbolKind::FIELD,
                            tags: None,
                            deprecated: None,
                            range: field_range,
                            selection_range: field_range,
                            children: None,
                        }
                    })
                    .collect();

                #[allow(deprecated)]
                symbols.push(DocumentSymbol {
                    name: record.name.clone(),
                    detail: None,
                    kind: SymbolKind::STRUCT,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range,
                    children: if children.is_empty() {
                        None
                    } else {
                        Some(children)
                    },
                });
            }
            Item::Enum(enum_def) => {
                let range = span_to_range(&enum_def.span, text);
                let selection_range = span_to_selection_range(&enum_def.span);

                let children: Vec<DocumentSymbol> = enum_def
                    .variants
                    .iter()
                    .map(|v| {
                        let detail = v.payload.as_ref().map(type_expr_to_string);
                        let variant_range = span_to_selection_range(&v.span);
                        #[allow(deprecated)]
                        DocumentSymbol {
                            name: v.name.clone(),
                            detail,
                            kind: SymbolKind::ENUM_MEMBER,
                            tags: None,
                            deprecated: None,
                            range: variant_range,
                            selection_range: variant_range,
                            children: None,
                        }
                    })
                    .collect();

                #[allow(deprecated)]
                symbols.push(DocumentSymbol {
                    name: enum_def.name.clone(),
                    detail: None,
                    kind: SymbolKind::ENUM,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range,
                    children: if children.is_empty() {
                        None
                    } else {
                        Some(children)
                    },
                });
            }
            Item::TypeAlias(alias) => {
                let range = span_to_range(&alias.span, text);
                let selection_range = span_to_selection_range(&alias.span);

                #[allow(deprecated)]
                symbols.push(DocumentSymbol {
                    name: alias.name.clone(),
                    detail: Some(type_expr_to_string(&alias.type_expr)),
                    kind: SymbolKind::TYPE_PARAMETER,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range,
                    children: None,
                });
            }
            Item::Process(process) => {
                let range = span_to_range(&process.span, text);
                let selection_range = span_to_selection_range(&process.span);

                #[allow(deprecated)]
                symbols.push(DocumentSymbol {
                    name: process.name.clone(),
                    detail: Some(process.kind.clone()),
                    kind: SymbolKind::CLASS,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range,
                    children: None,
                });
            }
            Item::Effect(effect) => {
                let range = span_to_range(&effect.span, text);
                let selection_range = span_to_selection_range(&effect.span);

                #[allow(deprecated)]
                symbols.push(DocumentSymbol {
                    name: effect.name.clone(),
                    detail: None,
                    kind: SymbolKind::INTERFACE,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range,
                    children: None,
                });
            }
            Item::Handler(handler) => {
                let range = span_to_range(&handler.span, text);
                let selection_range = span_to_selection_range(&handler.span);

                #[allow(deprecated)]
                symbols.push(DocumentSymbol {
                    name: handler.name.clone(),
                    detail: None,
                    kind: SymbolKind::OBJECT,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range,
                    children: None,
                });
            }
            _ => {}
        }
    }

    if symbols.is_empty() {
        None
    } else {
        Some(DocumentSymbolResponse::Nested(symbols))
    }
}

/// Convert a compiler Span to an LSP Range using byte offsets to compute the end line.
/// The span's `line` field is 1-based; LSP uses 0-based.
fn span_to_range(span: &lumen_compiler::compiler::tokens::Span, text: &str) -> Range {
    let start_line = span.line.saturating_sub(1) as u32;
    let start_char = span.col.saturating_sub(1) as u32;

    let end_line = byte_offset_to_line(text, span.end).unwrap_or(start_line);
    let end_char = byte_offset_to_col(text, span.end).unwrap_or(0);

    Range {
        start: lsp_types::Position {
            line: start_line,
            character: start_char,
        },
        end: lsp_types::Position {
            line: end_line,
            character: end_char,
        },
    }
}

/// Selection range covers just the name on the start line.
fn span_to_selection_range(span: &lumen_compiler::compiler::tokens::Span) -> Range {
    let line = span.line.saturating_sub(1) as u32;
    let start_char = span.col.saturating_sub(1) as u32;

    Range {
        start: lsp_types::Position {
            line,
            character: start_char,
        },
        end: lsp_types::Position {
            line,
            character: start_char + 1,
        },
    }
}

/// Compute the 0-based line number for a byte offset in the source text.
pub fn byte_offset_to_line(text: &str, offset: usize) -> Option<u32> {
    let clamped = offset.min(text.len());
    let line = text[..clamped].matches('\n').count() as u32;
    Some(line)
}

/// Compute the 0-based column for a byte offset in the source text.
fn byte_offset_to_col(text: &str, offset: usize) -> Option<u32> {
    let clamped = offset.min(text.len());
    let last_newline = text[..clamped].rfind('\n');
    let col = match last_newline {
        Some(nl_pos) => clamped - nl_pos - 1,
        None => clamped,
    };
    Some(col as u32)
}
