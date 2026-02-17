//! AST-to-constraint lowering.
//!
//! Converts Lumen `where`-clause expressions (AST `Expr` nodes) into a
//! solver-independent `Constraint` IR.  Only the subset of expressions that
//! is meaningful for SMT solving is handled; everything else produces a
//! `LoweringError` so the caller can mark the constraint as `Unverifiable`.

use crate::compiler::ast::{BinOp, Expr, UnaryOp};
use thiserror::Error;

// ── Constraint IR ───────────────────────────────────────────────────

/// Comparison operators for numeric constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

impl std::fmt::Display for CmpOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CmpOp::Eq => write!(f, "=="),
            CmpOp::NotEq => write!(f, "!="),
            CmpOp::Lt => write!(f, "<"),
            CmpOp::LtEq => write!(f, "<="),
            CmpOp::Gt => write!(f, ">"),
            CmpOp::GtEq => write!(f, ">="),
        }
    }
}

/// Solver-independent constraint representation.
///
/// Intentionally kept simple — this is the IR that the toy solver and
/// future Z3 backend both consume.
#[derive(Debug, Clone, PartialEq)]
pub enum Constraint {
    /// `var <op> value`  (e.g. `x > 0`)
    IntComparison { var: String, op: CmpOp, value: i64 },
    /// `var <op> value` for floats (e.g. `score >= 0.0`)
    FloatComparison { var: String, op: CmpOp, value: f64 },
    /// Constant truth value.
    BoolConst(bool),
    /// Logical conjunction.
    And(Vec<Constraint>),
    /// Logical disjunction.
    Or(Vec<Constraint>),
    /// Logical negation.
    Not(Box<Constraint>),
    /// A symbolic boolean variable (e.g. `is_valid`).
    BoolVar(String),
}

impl std::fmt::Display for Constraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Constraint::IntComparison { var, op, value } => {
                write!(f, "{} {} {}", var, op, value)
            }
            Constraint::FloatComparison { var, op, value } => {
                write!(f, "{} {} {}", var, op, value)
            }
            Constraint::BoolConst(b) => write!(f, "{}", b),
            Constraint::And(cs) => {
                let parts: Vec<_> = cs.iter().map(|c| format!("{}", c)).collect();
                write!(f, "({})", parts.join(" and "))
            }
            Constraint::Or(cs) => {
                let parts: Vec<_> = cs.iter().map(|c| format!("{}", c)).collect();
                write!(f, "({})", parts.join(" or "))
            }
            Constraint::Not(c) => write!(f, "not({})", c),
            Constraint::BoolVar(name) => write!(f, "{}", name),
        }
    }
}

// ── Errors ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum LoweringError {
    #[error("unsupported expression kind in constraint: {0}")]
    UnsupportedExpr(String),
    #[error("unsupported binary operator in constraint: {0}")]
    UnsupportedOp(String),
    #[error("comparison requires one identifier and one literal")]
    NonCanonicalComparison,
}

// ── Public API ──────────────────────────────────────────────────────

/// Lower an AST expression (from a `where` clause) into a `Constraint`.
pub fn lower_expr_to_constraint(expr: &Expr) -> Result<Constraint, LoweringError> {
    match expr {
        // ── Boolean literals ────────────────────────────────────
        Expr::BoolLit(val, _) => Ok(Constraint::BoolConst(*val)),

        // ── Identifiers (treated as boolean variables) ─────────
        Expr::Ident(name, _) => Ok(Constraint::BoolVar(name.clone())),

        // ── Unary not ──────────────────────────────────────────
        Expr::UnaryOp(UnaryOp::Not, inner, _) => {
            let c = lower_expr_to_constraint(inner)?;
            Ok(Constraint::Not(Box::new(c)))
        }

        // ── Binary operations ──────────────────────────────────
        Expr::BinOp(lhs, op, rhs, _) => lower_binop(lhs, *op, rhs),

        // ── Everything else is unsupported ──────────────────────
        other => Err(LoweringError::UnsupportedExpr(format!(
            "{:?}",
            std::mem::discriminant(other)
        ))),
    }
}

// ── Internal helpers ────────────────────────────────────────────────

fn lower_binop(lhs: &Expr, op: BinOp, rhs: &Expr) -> Result<Constraint, LoweringError> {
    match op {
        // Logical connectives
        BinOp::And => {
            let l = lower_expr_to_constraint(lhs)?;
            let r = lower_expr_to_constraint(rhs)?;
            // Flatten nested And
            let mut parts = Vec::new();
            flatten_and(l, &mut parts);
            flatten_and(r, &mut parts);
            Ok(Constraint::And(parts))
        }
        BinOp::Or => {
            let l = lower_expr_to_constraint(lhs)?;
            let r = lower_expr_to_constraint(rhs)?;
            let mut parts = Vec::new();
            flatten_or(l, &mut parts);
            flatten_or(r, &mut parts);
            Ok(Constraint::Or(parts))
        }

        // Comparison operators
        BinOp::Eq => lower_comparison(lhs, CmpOp::Eq, rhs),
        BinOp::NotEq => lower_comparison(lhs, CmpOp::NotEq, rhs),
        BinOp::Lt => lower_comparison(lhs, CmpOp::Lt, rhs),
        BinOp::LtEq => lower_comparison(lhs, CmpOp::LtEq, rhs),
        BinOp::Gt => lower_comparison(lhs, CmpOp::Gt, rhs),
        BinOp::GtEq => lower_comparison(lhs, CmpOp::GtEq, rhs),

        // Unsupported operators
        other => Err(LoweringError::UnsupportedOp(format!("{}", other))),
    }
}

/// Lower a comparison expression.  We require exactly one side to be an
/// identifier and the other to be a numeric literal.  If the literal is on
/// the left, we flip the operator.
fn lower_comparison(lhs: &Expr, op: CmpOp, rhs: &Expr) -> Result<Constraint, LoweringError> {
    match (extract_ident(lhs), extract_ident(rhs)) {
        // ident <op> literal
        (Some(name), None) => match extract_number(rhs) {
            Some(NumericValue::Int(val)) => Ok(Constraint::IntComparison {
                var: name.to_string(),
                op,
                value: val,
            }),
            Some(NumericValue::Float(val)) => Ok(Constraint::FloatComparison {
                var: name.to_string(),
                op,
                value: val,
            }),
            None => Err(LoweringError::NonCanonicalComparison),
        },
        // literal <op> ident  →  ident <flipped_op> literal
        (None, Some(name)) => {
            let flipped = flip_cmp(op);
            match extract_number(lhs) {
                Some(NumericValue::Int(val)) => Ok(Constraint::IntComparison {
                    var: name.to_string(),
                    op: flipped,
                    value: val,
                }),
                Some(NumericValue::Float(val)) => Ok(Constraint::FloatComparison {
                    var: name.to_string(),
                    op: flipped,
                    value: val,
                }),
                None => Err(LoweringError::NonCanonicalComparison),
            }
        }
        _ => Err(LoweringError::NonCanonicalComparison),
    }
}

enum NumericValue {
    Int(i64),
    Float(f64),
}

fn extract_ident(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Ident(name, _) => Some(name.as_str()),
        _ => None,
    }
}

fn extract_number(expr: &Expr) -> Option<NumericValue> {
    match expr {
        Expr::IntLit(v, _) => Some(NumericValue::Int(*v)),
        Expr::FloatLit(v, _) => Some(NumericValue::Float(*v)),
        // Support negated literals: -(42)
        Expr::UnaryOp(UnaryOp::Neg, inner, _) => match inner.as_ref() {
            Expr::IntLit(v, _) => Some(NumericValue::Int(-v)),
            Expr::FloatLit(v, _) => Some(NumericValue::Float(-v)),
            _ => None,
        },
        _ => None,
    }
}

fn flip_cmp(op: CmpOp) -> CmpOp {
    match op {
        CmpOp::Lt => CmpOp::Gt,
        CmpOp::LtEq => CmpOp::GtEq,
        CmpOp::Gt => CmpOp::Lt,
        CmpOp::GtEq => CmpOp::LtEq,
        CmpOp::Eq => CmpOp::Eq,
        CmpOp::NotEq => CmpOp::NotEq,
    }
}

fn flatten_and(c: Constraint, out: &mut Vec<Constraint>) {
    match c {
        Constraint::And(parts) => {
            for p in parts {
                flatten_and(p, out);
            }
        }
        other => out.push(other),
    }
}

fn flatten_or(c: Constraint, out: &mut Vec<Constraint>) {
    match c {
        Constraint::Or(parts) => {
            for p in parts {
                flatten_or(p, out);
            }
        }
        other => out.push(other),
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::tokens::Span;

    fn span() -> Span {
        Span {
            start: 0,
            end: 0,
            line: 1,
            col: 1,
        }
    }

    fn ident(name: &str) -> Expr {
        Expr::Ident(name.to_string(), span())
    }

    fn int_lit(v: i64) -> Expr {
        Expr::IntLit(v, span())
    }

    fn float_lit(v: f64) -> Expr {
        Expr::FloatLit(v, span())
    }

    fn binop(lhs: Expr, op: BinOp, rhs: Expr) -> Expr {
        Expr::BinOp(Box::new(lhs), op, Box::new(rhs), span())
    }

    #[test]
    fn lower_simple_gt() {
        // x > 0
        let expr = binop(ident("x"), BinOp::Gt, int_lit(0));
        let c = lower_expr_to_constraint(&expr).unwrap();
        assert_eq!(
            c,
            Constraint::IntComparison {
                var: "x".to_string(),
                op: CmpOp::Gt,
                value: 0,
            },
        );
    }

    #[test]
    fn lower_flipped_comparison() {
        // 10 > x  →  x < 10
        let expr = binop(int_lit(10), BinOp::Gt, ident("x"));
        let c = lower_expr_to_constraint(&expr).unwrap();
        assert_eq!(
            c,
            Constraint::IntComparison {
                var: "x".to_string(),
                op: CmpOp::Lt,
                value: 10,
            },
        );
    }

    #[test]
    fn lower_float_comparison() {
        // score >= 0.0
        let expr = binop(ident("score"), BinOp::GtEq, float_lit(0.0));
        let c = lower_expr_to_constraint(&expr).unwrap();
        assert_eq!(
            c,
            Constraint::FloatComparison {
                var: "score".to_string(),
                op: CmpOp::GtEq,
                value: 0.0,
            },
        );
    }

    #[test]
    fn lower_and() {
        // x > 0 and x < 100
        let left = binop(ident("x"), BinOp::Gt, int_lit(0));
        let right = binop(ident("x"), BinOp::Lt, int_lit(100));
        let expr = binop(left, BinOp::And, right);
        let c = lower_expr_to_constraint(&expr).unwrap();
        match c {
            Constraint::And(parts) => assert_eq!(parts.len(), 2),
            other => panic!("expected And, got {:?}", other),
        }
    }

    #[test]
    fn lower_or() {
        // x == 0 or x == 1
        let left = binop(ident("x"), BinOp::Eq, int_lit(0));
        let right = binop(ident("x"), BinOp::Eq, int_lit(1));
        let expr = binop(left, BinOp::Or, right);
        let c = lower_expr_to_constraint(&expr).unwrap();
        match c {
            Constraint::Or(parts) => assert_eq!(parts.len(), 2),
            other => panic!("expected Or, got {:?}", other),
        }
    }

    #[test]
    fn lower_not() {
        // not(x > 0)
        let inner = binop(ident("x"), BinOp::Gt, int_lit(0));
        let expr = Expr::UnaryOp(UnaryOp::Not, Box::new(inner), span());
        let c = lower_expr_to_constraint(&expr).unwrap();
        match c {
            Constraint::Not(_) => {}
            other => panic!("expected Not, got {:?}", other),
        }
    }

    #[test]
    fn lower_bool_literal() {
        let expr = Expr::BoolLit(true, span());
        let c = lower_expr_to_constraint(&expr).unwrap();
        assert_eq!(c, Constraint::BoolConst(true));
    }

    #[test]
    fn lower_ident_as_bool_var() {
        let expr = ident("is_valid");
        let c = lower_expr_to_constraint(&expr).unwrap();
        assert_eq!(c, Constraint::BoolVar("is_valid".to_string()));
    }

    #[test]
    fn unsupported_operator() {
        // x + 1 (arithmetic, not a constraint)
        let expr = binop(ident("x"), BinOp::Add, int_lit(1));
        assert!(lower_expr_to_constraint(&expr).is_err());
    }

    #[test]
    fn nested_and_flattens() {
        // (x > 0 and x < 10) and x != 5
        let a = binop(ident("x"), BinOp::Gt, int_lit(0));
        let b = binop(ident("x"), BinOp::Lt, int_lit(10));
        let ab = binop(a, BinOp::And, b);
        let c = binop(ident("x"), BinOp::NotEq, int_lit(5));
        let expr = binop(ab, BinOp::And, c);
        let result = lower_expr_to_constraint(&expr).unwrap();
        match result {
            Constraint::And(parts) => assert_eq!(parts.len(), 3),
            other => panic!("expected flat And with 3 parts, got {:?}", other),
        }
    }

    #[test]
    fn display_constraint() {
        let c = Constraint::IntComparison {
            var: "age".to_string(),
            op: CmpOp::GtEq,
            value: 0,
        };
        assert_eq!(format!("{}", c), "age >= 0");
    }
}
