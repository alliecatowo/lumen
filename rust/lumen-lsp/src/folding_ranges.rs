//! Folding range provider — returns foldable regions for cells, records, enums, etc.

use lsp_types::{FoldingRange, FoldingRangeKind, FoldingRangeParams};
use lumen_compiler::compiler::ast::{Item, Program};

use crate::document_symbols::byte_offset_to_line;

pub fn build_folding_ranges(
    _params: FoldingRangeParams,
    text: &str,
    program: Option<&Program>,
) -> Vec<FoldingRange> {
    let prog = match program {
        Some(p) => p,
        None => return vec![],
    };

    let mut ranges = Vec::new();

    for item in &prog.items {
        match item {
            Item::Cell(cell) => {
                if let Some(range) = make_folding_range(
                    &cell.span,
                    text,
                    FoldingRangeKind::Region,
                ) {
                    ranges.push(range);
                }
            }
            Item::Record(record) => {
                if let Some(range) = make_folding_range(
                    &record.span,
                    text,
                    FoldingRangeKind::Region,
                ) {
                    ranges.push(range);
                }
            }
            Item::Enum(enum_def) => {
                if let Some(range) = make_folding_range(
                    &enum_def.span,
                    text,
                    FoldingRangeKind::Region,
                ) {
                    ranges.push(range);
                }
            }
            Item::Process(process) => {
                if let Some(range) = make_folding_range(
                    &process.span,
                    text,
                    FoldingRangeKind::Region,
                ) {
                    ranges.push(range);
                }
            }
            Item::Effect(effect) => {
                if let Some(range) = make_folding_range(
                    &effect.span,
                    text,
                    FoldingRangeKind::Region,
                ) {
                    ranges.push(range);
                }
            }
            Item::Handler(handler) => {
                if let Some(range) = make_folding_range(
                    &handler.span,
                    text,
                    FoldingRangeKind::Region,
                ) {
                    ranges.push(range);
                }
            }
            Item::TypeAlias(alias) => {
                if let Some(range) = make_folding_range(
                    &alias.span,
                    text,
                    FoldingRangeKind::Region,
                ) {
                    ranges.push(range);
                }
            }
            _ => {}
        }
    }

    ranges
}

fn make_folding_range(
    span: &lumen_compiler::compiler::tokens::Span,
    text: &str,
    kind: FoldingRangeKind,
) -> Option<FoldingRange> {
    let start_line = span.line.saturating_sub(1) as u32;
    let end_line = byte_offset_to_line(text, span.end)?;

    if end_line <= start_line {
        return None;
    }

    Some(FoldingRange {
        start_line,
        start_character: None,
        end_line,
        end_character: None,
        kind: Some(kind),
        collapsed_text: None,
    })
}
