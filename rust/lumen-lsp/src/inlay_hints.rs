//! Inlay hints showing inferred types

use lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, InlayHintParams, Position};
use lumen_compiler::compiler::ast::{Expr, Item, Stmt};
use lumen_compiler::compiler::resolve::SymbolTable;

pub fn build_inlay_hints(
    _params: InlayHintParams,
    program: Option<&lumen_compiler::compiler::ast::Program>,
    symbols: Option<&SymbolTable>,
) -> Vec<InlayHint> {
    let mut hints = Vec::new();

    if let Some(prog) = program {
        for item in &prog.items {
            if let Item::Cell(cell) = item {
                for stmt in &cell.body {
                    extract_hints_from_stmt(stmt, &mut hints, symbols);
                }
            }
        }
    }

    hints
}

#[allow(clippy::only_used_in_recursion)]
fn extract_hints_from_stmt(stmt: &Stmt, hints: &mut Vec<InlayHint>, symbols: Option<&SymbolTable>) {
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
        }
        Stmt::If(if_stmt) => {
            for s in &if_stmt.then_body {
                extract_hints_from_stmt(s, hints, symbols);
            }
            if let Some(else_stmts) = &if_stmt.else_body {
                for s in else_stmts {
                    extract_hints_from_stmt(s, hints, symbols);
                }
            }
        }
        Stmt::While(while_stmt) => {
            for s in &while_stmt.body {
                extract_hints_from_stmt(s, hints, symbols);
            }
        }
        Stmt::Loop(loop_stmt) => {
            for s in &loop_stmt.body {
                extract_hints_from_stmt(s, hints, symbols);
            }
        }
        Stmt::For(for_stmt) => {
            for s in &for_stmt.body {
                extract_hints_from_stmt(s, hints, symbols);
            }
        }
        Stmt::Match(match_stmt) => {
            for arm in &match_stmt.arms {
                for s in &arm.body {
                    extract_hints_from_stmt(s, hints, symbols);
                }
            }
        }
        _ => {}
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
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
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
        _ => "<unknown>".to_string(),
    }
}
