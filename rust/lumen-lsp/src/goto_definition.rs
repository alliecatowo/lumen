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
                            start: Position {
                                line,
                                character: 0,
                            },
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
                            start: Position {
                                line,
                                character: 0,
                            },
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
                            start: Position {
                                line,
                                character: 0,
                            },
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
                            start: Position {
                                line,
                                character: 0,
                            },
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
                            start: Position {
                                line,
                                character: 0,
                            },
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
                            start: Position {
                                line,
                                character: 0,
                            },
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
                                    start: Position {
                                        line,
                                        character: 0,
                                    },
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
    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(position.line as usize)?;
    let char_pos = position.character as usize;

    if char_pos > line.len() {
        return None;
    }

    let start = line[..char_pos]
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);

    let end = line[char_pos..]
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| char_pos + i)
        .unwrap_or(line.len());

    if start >= end {
        return None;
    }

    Some(line[start..end].to_string())
}
