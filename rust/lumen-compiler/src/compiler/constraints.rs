//! Constraint validation for `where` clauses on record fields.

use crate::compiler::ast::*;
use crate::compiler::tokens::Span;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConstraintError {
    #[error("invalid constraint on field '{field}' at line {line}: {message}")]
    Invalid { field: String, line: usize, message: String },
}

/// Validate that all `where` constraints are well-formed.
/// Constraints can use: length(), count(), matches(), comparisons, and/or/not.
pub fn validate_constraints(program: &Program) -> Result<(), Vec<ConstraintError>> {
    let mut errors = Vec::new();

    for item in &program.items {
        if let Item::Record(r) = item {
            for field in &r.fields {
                if let Some(ref constraint) = field.constraint {
                    validate_constraint_expr(constraint, &field.name, &mut errors);
                }
            }
        }
    }

    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

fn validate_constraint_expr(expr: &Expr, field: &str, errors: &mut Vec<ConstraintError>) {
    match expr {
        Expr::BinOp(lhs, op, rhs, _) => {
            match op {
                BinOp::And | BinOp::Or | BinOp::Eq | BinOp::NotEq
                | BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq
                | BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                    validate_constraint_expr(lhs, field, errors);
                    validate_constraint_expr(rhs, field, errors);
                }
                _ => {
                    errors.push(ConstraintError::Invalid {
                        field: field.to_string(), line: expr.span().line,
                        message: format!("unsupported operator '{}' in constraint", op),
                    });
                }
            }
        }
        Expr::UnaryOp(UnaryOp::Not, inner, _) => {
            validate_constraint_expr(inner, field, errors);
        }
        Expr::Call(callee, args, span) => {
            // Only allow known constraint functions: length, count, matches
            if let Expr::Ident(name, _) = callee.as_ref() {
                match name.as_str() {
                    "length" | "count" | "matches" => {}
                    _ => {
                        errors.push(ConstraintError::Invalid {
                            field: field.to_string(), line: span.line,
                            message: format!("unknown constraint function '{}'", name),
                        });
                    }
                }
            }
        }
        Expr::Ident(_, _) | Expr::IntLit(_, _) | Expr::FloatLit(_, _)
        | Expr::StringLit(_, _) | Expr::BoolLit(_, _) | Expr::ListLit(_, _) => {}
        _ => {
            errors.push(ConstraintError::Invalid {
                field: field.to_string(), line: expr.span().line,
                message: "unsupported expression in constraint".to_string(),
            });
        }
    }
}
