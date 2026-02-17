//! Inlay hints showing inferred types and parameter names
//!
//! Provides two kinds of inlay hints:
//! - **Type hints**: Show inferred types for `let` bindings without explicit annotation
//! - **Parameter hints**: Show parameter names at call sites

use lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, InlayHintParams, Position};
use lumen_compiler::compiler::ast::{CallArg, CellDef, Expr, Item, Stmt};
use lumen_compiler::compiler::resolve::SymbolTable;

pub fn build_inlay_hints(
    _params: InlayHintParams,
    program: Option<&lumen_compiler::compiler::ast::Program>,
    symbols: Option<&SymbolTable>,
) -> Vec<InlayHint> {
    let mut hints = Vec::new();

    if let Some(prog) = program {
        // Collect cell definitions for parameter name lookup
        let cell_defs: Vec<&CellDef> = prog
            .items
            .iter()
            .filter_map(|item| {
                if let Item::Cell(cell) = item {
                    Some(cell)
                } else {
                    None
                }
            })
            .collect();

        for item in &prog.items {
            if let Item::Cell(cell) = item {
                for stmt in &cell.body {
                    extract_hints_from_stmt(stmt, &mut hints, symbols, &cell_defs);
                }
            }
        }
    }

    hints
}

fn extract_hints_from_stmt(
    stmt: &Stmt,
    hints: &mut Vec<InlayHint>,
    symbols: Option<&SymbolTable>,
    cell_defs: &[&CellDef],
) {
    match stmt {
        Stmt::Let(let_stmt) => {
            // Only show hints for bindings without explicit type annotation
            if let_stmt.ty.is_none() {
                // Infer the type from the initializer
                let inferred_type = infer_type_from_expr(&let_stmt.value);
                let line = if let_stmt.span.line > 0 {
                    (let_stmt.span.line - 1) as u32
                } else {
                    0
                };

                // Position after the variable name
                let position = Position {
                    line,
                    character: (let_stmt.span.start + let_stmt.name.len()) as u32,
                };

                hints.push(InlayHint {
                    position,
                    label: InlayHintLabel::String(format!(": {}", inferred_type)),
                    kind: Some(InlayHintKind::TYPE),
                    text_edits: None,
                    tooltip: None,
                    padding_left: None,
                    padding_right: Some(true),
                    data: None,
                });
            }

            // Also check the initializer expression for call-site param hints
            extract_param_hints_from_expr(&let_stmt.value, hints, symbols, cell_defs);
        }
        Stmt::If(if_stmt) => {
            extract_param_hints_from_expr(&if_stmt.condition, hints, symbols, cell_defs);
            for s in &if_stmt.then_body {
                extract_hints_from_stmt(s, hints, symbols, cell_defs);
            }
            if let Some(else_stmts) = &if_stmt.else_body {
                for s in else_stmts {
                    extract_hints_from_stmt(s, hints, symbols, cell_defs);
                }
            }
        }
        Stmt::While(while_stmt) => {
            extract_param_hints_from_expr(&while_stmt.condition, hints, symbols, cell_defs);
            for s in &while_stmt.body {
                extract_hints_from_stmt(s, hints, symbols, cell_defs);
            }
        }
        Stmt::Loop(loop_stmt) => {
            for s in &loop_stmt.body {
                extract_hints_from_stmt(s, hints, symbols, cell_defs);
            }
        }
        Stmt::For(for_stmt) => {
            extract_param_hints_from_expr(&for_stmt.iter, hints, symbols, cell_defs);
            for s in &for_stmt.body {
                extract_hints_from_stmt(s, hints, symbols, cell_defs);
            }
        }
        Stmt::Match(match_stmt) => {
            extract_param_hints_from_expr(&match_stmt.subject, hints, symbols, cell_defs);
            for arm in &match_stmt.arms {
                for s in &arm.body {
                    extract_hints_from_stmt(s, hints, symbols, cell_defs);
                }
            }
        }
        Stmt::Return(ret) => {
            extract_param_hints_from_expr(&ret.value, hints, symbols, cell_defs);
        }
        Stmt::Expr(expr_stmt) => {
            extract_param_hints_from_expr(&expr_stmt.expr, hints, symbols, cell_defs);
        }
        Stmt::Assign(assign) => {
            extract_param_hints_from_expr(&assign.value, hints, symbols, cell_defs);
        }
        _ => {}
    }
}

/// Extract parameter name hints from expressions, specifically at call sites.
fn extract_param_hints_from_expr(
    expr: &Expr,
    hints: &mut Vec<InlayHint>,
    symbols: Option<&SymbolTable>,
    cell_defs: &[&CellDef],
) {
    match expr {
        Expr::Call(func, args, _) => {
            // Try to resolve the function name to get parameter names
            if let Expr::Ident(func_name, _) = &**func {
                let param_names = resolve_param_names(func_name, symbols, cell_defs);

                for (i, arg) in args.iter().enumerate() {
                    if let CallArg::Positional(arg_expr) = arg {
                        if let Some(param_name) = param_names.get(i) {
                            // Don't show hint if the argument is already a simple
                            // identifier with the same name as the parameter
                            if is_trivial_arg(arg_expr, param_name) {
                                continue;
                            }

                            let span = arg_expr.span();
                            let line = if span.line > 0 {
                                (span.line - 1) as u32
                            } else {
                                0
                            };
                            let col = if span.col > 0 {
                                (span.col - 1) as u32
                            } else {
                                0
                            };

                            hints.push(InlayHint {
                                position: Position {
                                    line,
                                    character: col,
                                },
                                label: InlayHintLabel::String(format!("{}:", param_name)),
                                kind: Some(InlayHintKind::PARAMETER),
                                text_edits: None,
                                tooltip: None,
                                padding_left: None,
                                padding_right: Some(true),
                                data: None,
                            });
                        }
                    }
                    // Recurse into argument expressions
                    match arg {
                        CallArg::Positional(e) => {
                            extract_param_hints_from_expr(e, hints, symbols, cell_defs);
                        }
                        CallArg::Named(_, e, _) => {
                            extract_param_hints_from_expr(e, hints, symbols, cell_defs);
                        }
                        CallArg::Role(_, e, _) => {
                            extract_param_hints_from_expr(e, hints, symbols, cell_defs);
                        }
                    }
                }
            } else {
                // Recurse into non-ident function expression
                extract_param_hints_from_expr(func, hints, symbols, cell_defs);
                for arg in args {
                    match arg {
                        CallArg::Positional(e) => {
                            extract_param_hints_from_expr(e, hints, symbols, cell_defs);
                        }
                        CallArg::Named(_, e, _) => {
                            extract_param_hints_from_expr(e, hints, symbols, cell_defs);
                        }
                        CallArg::Role(_, e, _) => {
                            extract_param_hints_from_expr(e, hints, symbols, cell_defs);
                        }
                    }
                }
            }
        }
        Expr::BinOp(left, _, right, _) => {
            extract_param_hints_from_expr(left, hints, symbols, cell_defs);
            extract_param_hints_from_expr(right, hints, symbols, cell_defs);
        }
        Expr::UnaryOp(_, inner, _) => {
            extract_param_hints_from_expr(inner, hints, symbols, cell_defs);
        }
        Expr::ListLit(items, _) => {
            for item in items {
                extract_param_hints_from_expr(item, hints, symbols, cell_defs);
            }
        }
        Expr::DotAccess(inner, _, _) => {
            extract_param_hints_from_expr(inner, hints, symbols, cell_defs);
        }
        Expr::Pipe { left, right, .. } => {
            extract_param_hints_from_expr(left, hints, symbols, cell_defs);
            extract_param_hints_from_expr(right, hints, symbols, cell_defs);
        }
        _ => {}
    }
}

/// Resolve parameter names for a function call.
/// Checks the symbol table first, then falls back to AST cell definitions,
/// and finally checks builtins.
fn resolve_param_names(
    func_name: &str,
    symbols: Option<&SymbolTable>,
    cell_defs: &[&CellDef],
) -> Vec<String> {
    // Check symbol table
    if let Some(syms) = symbols {
        if let Some(cell_info) = syms.cells.get(func_name) {
            return cell_info
                .params
                .iter()
                .map(|(name, _, _)| name.clone())
                .collect();
        }
    }

    // Check AST cell definitions
    for cell in cell_defs {
        if cell.name == func_name {
            return cell.params.iter().map(|p| p.name.clone()).collect();
        }
    }

    // Check builtins
    builtin_param_names(func_name)
}

/// Returns parameter names for well-known builtin functions.
fn builtin_param_names(name: &str) -> Vec<String> {
    match name {
        "print" => vec!["value".to_string()],
        "len" => vec!["collection".to_string()],
        "append" => vec!["list".to_string(), "item".to_string()],
        "sort" => vec!["list".to_string()],
        "map" => vec!["list".to_string(), "fn".to_string()],
        "filter" => vec!["list".to_string(), "predicate".to_string()],
        "reduce" => vec!["list".to_string(), "initial".to_string(), "fn".to_string()],
        "join" => vec!["list".to_string(), "separator".to_string()],
        "split" => vec!["string".to_string(), "separator".to_string()],
        "trim" => vec!["string".to_string()],
        "parse_int" => vec!["string".to_string()],
        "parse_float" => vec!["string".to_string()],
        "to_string" => vec!["value".to_string()],
        "contains" => vec!["collection".to_string(), "item".to_string()],
        "keys" => vec!["map".to_string()],
        "values" => vec!["map".to_string()],
        "parallel" => vec!["futures".to_string()],
        "race" => vec!["futures".to_string()],
        "timeout" => vec!["future".to_string(), "ms".to_string()],
        "get_env" => vec!["key".to_string()],
        "read_file" => vec!["path".to_string()],
        "write_file" => vec!["path".to_string(), "content".to_string()],
        _ => vec![],
    }
}

/// Check whether the argument is "trivial" — i.e., an identifier with the same
/// name as the parameter, so showing a hint would be redundant.
fn is_trivial_arg(expr: &Expr, param_name: &str) -> bool {
    if let Expr::Ident(name, _) = expr {
        name == param_name
    } else {
        false
    }
}

fn infer_type_from_expr(expr: &Expr) -> String {
    match expr {
        Expr::IntLit(_, _) => "Int".to_string(),
        Expr::FloatLit(_, _) => "Float".to_string(),
        Expr::StringLit(_, _) => "String".to_string(),
        Expr::BoolLit(_, _) => "Bool".to_string(),
        Expr::ListLit(items, _) => {
            if items.is_empty() {
                "list[_]".to_string()
            } else {
                let elem_type = infer_type_from_expr(&items[0]);
                format!("list[{}]", elem_type)
            }
        }
        Expr::MapLit(entries, _) => {
            if entries.is_empty() {
                "map[_, _]".to_string()
            } else {
                let key_type = infer_type_from_expr(&entries[0].0);
                let val_type = infer_type_from_expr(&entries[0].1);
                format!("map[{}, {}]", key_type, val_type)
            }
        }
        Expr::RecordLit(type_name, _, _) => type_name.clone(),
        Expr::Call(func_expr, _, _) => {
            // For builtins, we can infer some types
            if let Expr::Ident(name, _) = &**func_expr {
                match name.as_str() {
                    "print" => "Void".to_string(),
                    "len" => "Int".to_string(),
                    "to_string" => "String".to_string(),
                    "parse_int" => "result[Int, String]".to_string(),
                    "parse_float" => "result[Float, String]".to_string(),
                    _ => "<unknown>".to_string(),
                }
            } else {
                "<unknown>".to_string()
            }
        }
        Expr::BinOp(left, op, right, _) => {
            use lumen_compiler::compiler::ast::BinOp;
            match op {
                BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Div
                | BinOp::FloorDiv
                | BinOp::Mod => {
                    // Numeric operations
                    let left_type = infer_type_from_expr(left);
                    if left_type == "Float" || infer_type_from_expr(right) == "Float" {
                        "Float".to_string()
                    } else {
                        "Int".to_string()
                    }
                }
                BinOp::Eq
                | BinOp::NotEq
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::LtEq
                | BinOp::GtEq
                | BinOp::And
                | BinOp::Or => "Bool".to_string(),
                _ => "<unknown>".to_string(),
            }
        }
        Expr::TupleLit(items, _) => {
            let types: Vec<String> = items.iter().map(infer_type_from_expr).collect();
            format!("({})", types.join(", "))
        }
        Expr::NullLit(_) => "Null".to_string(),
        Expr::StringInterp(_, _) => "String".to_string(),
        _ => "<unknown>".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_program(source: &str) -> Option<lumen_compiler::compiler::ast::Program> {
        let mut lexer = lumen_compiler::compiler::lexer::Lexer::new(source, 1, 0);
        let tokens = lexer.tokenize().ok()?;
        let mut parser = lumen_compiler::compiler::parser::Parser::new(tokens);
        parser.parse_program(vec![]).ok()
    }

    fn resolve_symbols(prog: &lumen_compiler::compiler::ast::Program) -> Option<SymbolTable> {
        lumen_compiler::compiler::resolve::resolve(prog).ok()
    }

    #[test]
    fn test_type_hint_for_int_literal() {
        let source = "cell main() -> Int\n  let x = 42\n  return x\nend";
        let program = parse_program(source);
        let symbols = program.as_ref().and_then(resolve_symbols);

        let params = InlayHintParams {
            work_done_progress_params: Default::default(),
            text_document: lsp_types::TextDocumentIdentifier {
                uri: "file:///test.lm".parse().unwrap(),
            },
            range: lsp_types::Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 10,
                    character: 0,
                },
            },
        };

        let hints = build_inlay_hints(params, program.as_ref(), symbols.as_ref());

        // Should have at least one type hint for `let x = 42`
        let type_hints: Vec<_> = hints
            .iter()
            .filter(|h| h.kind == Some(InlayHintKind::TYPE))
            .collect();
        assert!(!type_hints.is_empty(), "Should have type hints");

        // The type hint should say ": Int"
        let has_int = type_hints.iter().any(|h| {
            if let InlayHintLabel::String(s) = &h.label {
                s.contains("Int")
            } else {
                false
            }
        });
        assert!(has_int, "Should infer Int for integer literal");
    }

    #[test]
    fn test_type_hint_for_string_literal() {
        let source = "cell main() -> String\n  let msg = \"hello\"\n  return msg\nend";
        let program = parse_program(source);
        let symbols = program.as_ref().and_then(resolve_symbols);

        let params = InlayHintParams {
            work_done_progress_params: Default::default(),
            text_document: lsp_types::TextDocumentIdentifier {
                uri: "file:///test.lm".parse().unwrap(),
            },
            range: lsp_types::Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 10,
                    character: 0,
                },
            },
        };

        let hints = build_inlay_hints(params, program.as_ref(), symbols.as_ref());

        let type_hints: Vec<_> = hints
            .iter()
            .filter(|h| h.kind == Some(InlayHintKind::TYPE))
            .collect();

        let has_string = type_hints.iter().any(|h| {
            if let InlayHintLabel::String(s) = &h.label {
                s.contains("String")
            } else {
                false
            }
        });
        assert!(has_string, "Should infer String for string literal");
    }

    #[test]
    fn test_no_type_hint_when_annotated() {
        let source = "cell main() -> Int\n  let x: Int = 42\n  return x\nend";
        let program = parse_program(source);
        let symbols = program.as_ref().and_then(resolve_symbols);

        let params = InlayHintParams {
            work_done_progress_params: Default::default(),
            text_document: lsp_types::TextDocumentIdentifier {
                uri: "file:///test.lm".parse().unwrap(),
            },
            range: lsp_types::Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 10,
                    character: 0,
                },
            },
        };

        let hints = build_inlay_hints(params, program.as_ref(), symbols.as_ref());

        let type_hints: Vec<_> = hints
            .iter()
            .filter(|h| h.kind == Some(InlayHintKind::TYPE))
            .collect();

        // When a let has an explicit type, there should be no type hint for it
        assert!(
            type_hints.is_empty(),
            "Should not show type hint for explicitly annotated binding"
        );
    }

    #[test]
    fn test_param_hint_at_call_site() {
        let source =
            "cell greet(name: String, count: Int) -> String\n  return name\nend\n\ncell main() -> Int\n  let r = greet(\"world\", 5)\n  return 0\nend";
        let program = parse_program(source);
        let symbols = program.as_ref().and_then(resolve_symbols);

        let params = InlayHintParams {
            work_done_progress_params: Default::default(),
            text_document: lsp_types::TextDocumentIdentifier {
                uri: "file:///test.lm".parse().unwrap(),
            },
            range: lsp_types::Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 10,
                    character: 0,
                },
            },
        };

        let hints = build_inlay_hints(params, program.as_ref(), symbols.as_ref());

        let param_hints: Vec<_> = hints
            .iter()
            .filter(|h| h.kind == Some(InlayHintKind::PARAMETER))
            .collect();

        assert!(
            !param_hints.is_empty(),
            "Should have parameter hints at call site"
        );

        let labels: Vec<String> = param_hints
            .iter()
            .filter_map(|h| {
                if let InlayHintLabel::String(s) = &h.label {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .collect();

        assert!(
            labels.iter().any(|l| l.contains("name")),
            "Should have 'name' parameter hint, got: {:?}",
            labels
        );
        assert!(
            labels.iter().any(|l| l.contains("count")),
            "Should have 'count' parameter hint, got: {:?}",
            labels
        );
    }

    #[test]
    fn test_param_hint_skipped_when_arg_matches_param_name() {
        let source =
            "cell greet(name: String) -> String\n  return name\nend\n\ncell main() -> Int\n  let name = \"world\"\n  let r = greet(name)\n  return 0\nend";
        let program = parse_program(source);
        let symbols = program.as_ref().and_then(resolve_symbols);

        let params = InlayHintParams {
            work_done_progress_params: Default::default(),
            text_document: lsp_types::TextDocumentIdentifier {
                uri: "file:///test.lm".parse().unwrap(),
            },
            range: lsp_types::Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 10,
                    character: 0,
                },
            },
        };

        let hints = build_inlay_hints(params, program.as_ref(), symbols.as_ref());

        let param_hints: Vec<_> = hints
            .iter()
            .filter(|h| h.kind == Some(InlayHintKind::PARAMETER))
            .collect();

        // When calling greet(name) where arg name matches param name, no hint
        assert!(
            param_hints.is_empty(),
            "Should not show param hint when arg name matches param name, got: {:?}",
            param_hints.iter().map(|h| &h.label).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_builtin_param_names() {
        assert_eq!(builtin_param_names("print"), vec!["value"]);
        assert_eq!(builtin_param_names("join"), vec!["list", "separator"]);
        assert!(builtin_param_names("nonexistent").is_empty());
    }

    #[test]
    fn test_infer_type_tuple() {
        let source = "cell main() -> Int\n  let pair = (1, \"hello\")\n  return 0\nend";
        let program = parse_program(source);
        let symbols = program.as_ref().and_then(resolve_symbols);

        let params = InlayHintParams {
            work_done_progress_params: Default::default(),
            text_document: lsp_types::TextDocumentIdentifier {
                uri: "file:///test.lm".parse().unwrap(),
            },
            range: lsp_types::Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 10,
                    character: 0,
                },
            },
        };

        let hints = build_inlay_hints(params, program.as_ref(), symbols.as_ref());

        let type_hints: Vec<_> = hints
            .iter()
            .filter(|h| h.kind == Some(InlayHintKind::TYPE))
            .collect();

        let has_tuple = type_hints.iter().any(|h| {
            if let InlayHintLabel::String(s) = &h.label {
                s.contains("Int") && s.contains("String")
            } else {
                false
            }
        });
        assert!(has_tuple, "Should infer tuple type (Int, String)");
    }
}
