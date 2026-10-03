//! Go-to-definition support

use lsp_types::{GotoDefinitionParams, GotoDefinitionResponse, Location, Position, Range, Uri};
use lumen_compiler::compiler::ast::{Item, Program};

pub fn build_goto_definition(
    params: GotoDefinitionParams,
    text: &str,
    program: Option<&Program>,
    uri: &Uri,
) -> Option<GotoDefinitionResponse> {
    let position = params.text_document_position_params.position;
    let word = extract_word_at_position(text, position)?;

    if let Some(prog) = program {
        for item in &prog.items {
            match item {
                Item::Cell(cell) if cell.name == word => {
                    let line = if cell.span.line > 0 {
                        (cell.span.line - 1) as u32
                    } else {
                        0
                    };

                    return Some(GotoDefinitionResponse::Scalar(Location {
                        uri: uri.clone(),
                        range: Range {
                            start: Position { line, character: 0 },
                            end: Position {
                                line,
                                character: u32::MAX,
                            },
                        },
                    }));
                }
                Item::Record(record) if record.name == word => {
                    let line = if record.span.line > 0 {
                        (record.span.line - 1) as u32
                    } else {
                        0
                    };

                    return Some(GotoDefinitionResponse::Scalar(Location {
                        uri: uri.clone(),
                        range: Range {
                            start: Position { line, character: 0 },
                            end: Position {
                                line,
                                character: u32::MAX,
                            },
                        },
                    }));
                }
                Item::Enum(enum_def) if enum_def.name == word => {
                    let line = if enum_def.span.line > 0 {
                        (enum_def.span.line - 1) as u32
                    } else {
                        0
                    };

                    return Some(GotoDefinitionResponse::Scalar(Location {
                        uri: uri.clone(),
                        range: Range {
                            start: Position { line, character: 0 },
                            end: Position {
                                line,
                                character: u32::MAX,
                            },
                        },
                    }));
                }
                Item::TypeAlias(alias) if alias.name == word => {
                    let line = if alias.span.line > 0 {
                        (alias.span.line - 1) as u32
                    } else {
                        0
                    };

                    return Some(GotoDefinitionResponse::Scalar(Location {
                        uri: uri.clone(),
                        range: Range {
                            start: Position { line, character: 0 },
                            end: Position {
                                line,
                                character: u32::MAX,
                            },
                        },
                    }));
                }
                Item::Process(process) if process.name == word => {
                    let line = if process.span.line > 0 {
                        (process.span.line - 1) as u32
                    } else {
                        0
                    };

                    return Some(GotoDefinitionResponse::Scalar(Location {
                        uri: uri.clone(),
                        range: Range {
                            start: Position { line, character: 0 },
                            end: Position {
                                line,
                                character: u32::MAX,
                            },
                        },
                    }));
                }
                Item::Effect(effect) if effect.name == word => {
                    let line = if effect.span.line > 0 {
                        (effect.span.line - 1) as u32
                    } else {
                        0
                    };

                    return Some(GotoDefinitionResponse::Scalar(Location {
                        uri: uri.clone(),
                        range: Range {
                            start: Position { line, character: 0 },
                            end: Position {
                                line,
                                character: u32::MAX,
                            },
                        },
                    }));
                }
                // Check enum variants
                Item::Enum(enum_def) => {
                    for variant in &enum_def.variants {
                        if variant.name == word {
                            let line = if enum_def.span.line > 0 {
                                (enum_def.span.line - 1) as u32
                            } else {
                                0
                            };

                            return Some(GotoDefinitionResponse::Scalar(Location {
                                uri: uri.clone(),
                                range: Range {
                                    start: Position { line, character: 0 },
                                    end: Position {
                                        line,
                                        character: u32::MAX,
                                    },
                                },
                            }));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    None
}

fn extract_word_at_position(text: &str, position: Position) -> Option<String> {
    crate::position::word_at(text, position)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::testing::*;
    use lsp_types::{TextDocumentIdentifier, TextDocumentPositionParams, WorkDoneProgressParams};

    fn definition_at(
        text: &str,
        program: Option<&Program>,
        line: u32,
        character: u32,
    ) -> Option<GotoDefinitionResponse> {
        let uri: Uri = "file:///t.lm".parse().unwrap();
        build_goto_definition(
            GotoDefinitionParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                    position: Position { line, character },
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: Default::default(),
            },
            text,
            program,
            &uri,
        )
    }

    #[test]
    fn definition_after_non_ascii_text_on_the_same_line() {
        let program = parse_program(UNICODE_DOC).expect("parses");
        let col = col_of(UNICODE_DOC, 6, "greet", 1);
        let found =
            definition_at(UNICODE_DOC, Some(&program), 6, col).expect("definition of greet");
        let GotoDefinitionResponse::Scalar(loc) = found else {
            panic!("expected a single location")
        };
        assert_eq!(loc.range.start.line, 0);
    }

    #[test]
    fn definition_never_panics_on_any_column_of_a_non_ascii_line() {
        let program = parse_program(UNICODE_DOC).expect("parses");
        for line in 0..UNICODE_DOC.lines().count() as u32 + 1 {
            for col in 0..60 {
                let _ = definition_at(UNICODE_DOC, Some(&program), line, col);
            }
        }
    }
}
