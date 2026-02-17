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

/// Arithmetic operators for constraints like `x + 1 > 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
}

impl std::fmt::Display for ArithOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArithOp::Add => write!(f, "+"),
            ArithOp::Sub => write!(f, "-"),
            ArithOp::Mul => write!(f, "*"),
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

    // ── New variants (T042-T048) ────────────────────────────────
    /// A named variable reference (non-boolean, for use in constraint
    /// substitution, e.g. representing "the value of x").
    Var(String),

    /// Comparison between two variables: `left <op> right`.
    VarComparison {
        left: String,
        op: CmpOp,
        right: String,
    },

    /// Arithmetic constraint: `(var <arith_op> constant) <cmp_op> value`.
    /// Example: `x + 1 > 0` → Arithmetic { var: "x", arith_op: Add, arith_const: 1, cmp_op: Gt, cmp_value: 0 }
    Arithmetic {
        var: String,
        arith_op: ArithOp,
        arith_const: i64,
        cmp_op: CmpOp,
        cmp_value: i64,
    },

    /// Effect budget constraint: limits the number of calls to a given
    /// effect/tool within a cell body.
    EffectBudget {
        effect_name: String,
        max_calls: u32,
        actual_calls: u32,
    },
}

impl Constraint {
    /// Substitute all occurrences of `var_name` with a constant integer value.
    /// This is used to check preconditions when arguments are known literals.
    pub fn substitute_int(&self, var_name: &str, value: i64) -> Constraint {
        match self {
            Constraint::IntComparison { var, op, value: v } if var == var_name => {
                // After substitution, this becomes a concrete check.
                let holds = match op {
                    CmpOp::Gt => value > *v,
                    CmpOp::GtEq => value >= *v,
                    CmpOp::Lt => value < *v,
                    CmpOp::LtEq => value <= *v,
                    CmpOp::Eq => value == *v,
                    CmpOp::NotEq => value != *v,
                };
                Constraint::BoolConst(holds)
            }
            Constraint::IntComparison { .. } => self.clone(),
            Constraint::FloatComparison { .. } => self.clone(),
            Constraint::BoolConst(_) => self.clone(),
            Constraint::BoolVar(name) if name == var_name => {
                // Treat non-zero as true for boolean substitution.
                Constraint::BoolConst(value != 0)
            }
            Constraint::BoolVar(_) => self.clone(),
            Constraint::Var(name) if name == var_name => {
                // Replace with a bool-const for now (var becomes concrete).
                Constraint::BoolConst(true)
            }
            Constraint::Var(_) => self.clone(),
            Constraint::And(parts) => Constraint::And(
                parts
                    .iter()
                    .map(|p| p.substitute_int(var_name, value))
                    .collect(),
            ),
            Constraint::Or(parts) => Constraint::Or(
                parts
                    .iter()
                    .map(|p| p.substitute_int(var_name, value))
                    .collect(),
            ),
            Constraint::Not(inner) => {
                Constraint::Not(Box::new(inner.substitute_int(var_name, value)))
            }
            Constraint::VarComparison { left, op, right } => {
                if left == var_name && right == var_name {
                    // Both sides are the same variable being substituted.
                    let holds = match op {
                        CmpOp::Eq => true,
                        CmpOp::NotEq => false,
                        CmpOp::Lt => false,
                        CmpOp::LtEq => true,
                        CmpOp::Gt => false,
                        CmpOp::GtEq => true,
                    };
                    Constraint::BoolConst(holds)
                } else if left == var_name {
                    Constraint::IntComparison {
                        var: right.clone(),
                        op: flip_cmp(*op),
                        value,
                    }
                } else if right == var_name {
                    Constraint::IntComparison {
                        var: left.clone(),
                        op: *op,
                        value,
                    }
                } else {
                    self.clone()
                }
            }
            Constraint::Arithmetic {
                var,
                arith_op,
                arith_const,
                cmp_op,
                cmp_value,
            } if var == var_name => {
                let computed = match arith_op {
                    ArithOp::Add => value.saturating_add(*arith_const),
                    ArithOp::Sub => value.saturating_sub(*arith_const),
                    ArithOp::Mul => value.saturating_mul(*arith_const),
                };
                let holds = match cmp_op {
                    CmpOp::Gt => computed > *cmp_value,
                    CmpOp::GtEq => computed >= *cmp_value,
                    CmpOp::Lt => computed < *cmp_value,
                    CmpOp::LtEq => computed <= *cmp_value,
                    CmpOp::Eq => computed == *cmp_value,
                    CmpOp::NotEq => computed != *cmp_value,
                };
                Constraint::BoolConst(holds)
            }
            Constraint::Arithmetic { .. } => self.clone(),
            Constraint::EffectBudget { .. } => self.clone(),
        }
    }

    /// Rename a variable in the constraint (used to map callee parameter
    /// names to caller argument names).
    pub fn rename_var(&self, from: &str, to: &str) -> Constraint {
        match self {
            Constraint::IntComparison { var, op, value } => {
                let var = if var == from {
                    to.to_string()
                } else {
                    var.clone()
                };
                Constraint::IntComparison {
                    var,
                    op: *op,
                    value: *value,
                }
            }
            Constraint::FloatComparison { var, op, value } => {
                let var = if var == from {
                    to.to_string()
                } else {
                    var.clone()
                };
                Constraint::FloatComparison {
                    var,
                    op: *op,
                    value: *value,
                }
            }
            Constraint::BoolConst(_) => self.clone(),
            Constraint::BoolVar(name) => {
                if name == from {
                    Constraint::BoolVar(to.to_string())
                } else {
                    self.clone()
                }
            }
            Constraint::Var(name) => {
                if name == from {
                    Constraint::Var(to.to_string())
                } else {
                    self.clone()
                }
            }
            Constraint::And(parts) => {
                Constraint::And(parts.iter().map(|p| p.rename_var(from, to)).collect())
            }
            Constraint::Or(parts) => {
                Constraint::Or(parts.iter().map(|p| p.rename_var(from, to)).collect())
            }
            Constraint::Not(inner) => Constraint::Not(Box::new(inner.rename_var(from, to))),
            Constraint::VarComparison { left, op, right } => {
                let left = if left == from {
                    to.to_string()
                } else {
                    left.clone()
                };
                let right = if right == from {
                    to.to_string()
                } else {
                    right.clone()
                };
                Constraint::VarComparison {
                    left,
                    op: *op,
                    right,
                }
            }
            Constraint::Arithmetic {
                var,
                arith_op,
                arith_const,
                cmp_op,
                cmp_value,
            } => {
                let var = if var == from {
                    to.to_string()
                } else {
                    var.clone()
                };
                Constraint::Arithmetic {
                    var,
                    arith_op: *arith_op,
                    arith_const: *arith_const,
                    cmp_op: *cmp_op,
                    cmp_value: *cmp_value,
                }
            }
            Constraint::EffectBudget { .. } => self.clone(),
        }
    }
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
            Constraint::Var(name) => write!(f, "${}", name),
            Constraint::VarComparison { left, op, right } => {
                write!(f, "{} {} {}", left, op, right)
            }
            Constraint::Arithmetic {
                var,
                arith_op,
                arith_const,
                cmp_op,
                cmp_value,
            } => {
                write!(
                    f,
                    "({} {} {}) {} {}",
                    var, arith_op, arith_const, cmp_op, cmp_value
                )
            }
            Constraint::EffectBudget {
                effect_name,
                max_calls,
                actual_calls,
            } => {
                write!(
                    f,
                    "effect_budget({}, max={}, actual={})",
                    effect_name, max_calls, actual_calls
                )
            }
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

/// Lower a comparison expression.  We handle three cases:
/// 1. `ident <op> literal`  → IntComparison / FloatComparison
/// 2. `literal <op> ident`  → flip the operator
/// 3. `ident <op> ident`    → VarComparison (NEW)
///
/// Additionally, if one side is `ident <arith> literal` and the other
/// side is a literal, we produce an Arithmetic constraint.
fn lower_comparison(lhs: &Expr, op: CmpOp, rhs: &Expr) -> Result<Constraint, LoweringError> {
    // Try: (ident arith_op const) cmp_op literal
    if let Some(c) = try_lower_arithmetic(lhs, op, rhs) {
        return Ok(c);
    }
    // Try: literal cmp_op (ident arith_op const)
    if let Some(c) = try_lower_arithmetic(rhs, flip_cmp(op), lhs) {
        return Ok(c);
    }

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
        // ident <op> ident → VarComparison
        (Some(left), Some(right)) => Ok(Constraint::VarComparison {
            left: left.to_string(),
            op,
            right: right.to_string(),
        }),
        _ => Err(LoweringError::NonCanonicalComparison),
    }
}

/// Try to lower `(ident arith_op const) cmp_op literal`.
/// Returns `None` if lhs is not an arithmetic expression.
fn try_lower_arithmetic(lhs: &Expr, cmp_op: CmpOp, rhs: &Expr) -> Option<Constraint> {
    // lhs must be BinOp(ident, arith, int_lit) and rhs must be int_lit
    if let Expr::BinOp(inner_lhs, inner_op, inner_rhs, _) = lhs {
        let arith_op = match inner_op {
            BinOp::Add => ArithOp::Add,
            BinOp::Sub => ArithOp::Sub,
            BinOp::Mul => ArithOp::Mul,
            _ => return None,
        };
        if let (Some(var_name), Some(NumericValue::Int(arith_const))) =
            (extract_ident(inner_lhs), extract_number(inner_rhs))
        {
            if let Some(NumericValue::Int(cmp_value)) = extract_number(rhs) {
                return Some(Constraint::Arithmetic {
                    var: var_name.to_string(),
                    arith_op,
                    arith_const,
                    cmp_op,
                    cmp_value,
                });
            }
        }
    }
    None
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

pub fn flip_cmp(op: CmpOp) -> CmpOp {
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
        // x + 1 (arithmetic, not a constraint — no comparison wrapping it)
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

    // ── New constraint variant tests ────────────────────────────

    #[test]
    fn lower_var_comparison() {
        // x > y
        let expr = binop(ident("x"), BinOp::Gt, ident("y"));
        let c = lower_expr_to_constraint(&expr).unwrap();
        assert_eq!(
            c,
            Constraint::VarComparison {
                left: "x".to_string(),
                op: CmpOp::Gt,
                right: "y".to_string(),
            },
        );
    }

    #[test]
    fn lower_arithmetic_constraint() {
        // (x + 1) > 0
        let arith = binop(ident("x"), BinOp::Add, int_lit(1));
        let expr = binop(arith, BinOp::Gt, int_lit(0));
        let c = lower_expr_to_constraint(&expr).unwrap();
        assert_eq!(
            c,
            Constraint::Arithmetic {
                var: "x".to_string(),
                arith_op: ArithOp::Add,
                arith_const: 1,
                cmp_op: CmpOp::Gt,
                cmp_value: 0,
            },
        );
    }

    #[test]
    fn substitute_int_simple() {
        // x > 0, substitute x = 5 → true
        let c = Constraint::IntComparison {
            var: "x".to_string(),
            op: CmpOp::Gt,
            value: 0,
        };
        assert_eq!(c.substitute_int("x", 5), Constraint::BoolConst(true));
    }

    #[test]
    fn substitute_int_fails() {
        // x > 0, substitute x = -1 → false
        let c = Constraint::IntComparison {
            var: "x".to_string(),
            op: CmpOp::Gt,
            value: 0,
        };
        assert_eq!(c.substitute_int("x", -1), Constraint::BoolConst(false));
    }

    #[test]
    fn rename_var_in_constraint() {
        let c = Constraint::IntComparison {
            var: "param_x".to_string(),
            op: CmpOp::Gt,
            value: 0,
        };
        let renamed = c.rename_var("param_x", "arg_val");
        assert_eq!(
            renamed,
            Constraint::IntComparison {
                var: "arg_val".to_string(),
                op: CmpOp::Gt,
                value: 0,
            },
        );
    }

    #[test]
    fn display_var_comparison() {
        let c = Constraint::VarComparison {
            left: "x".to_string(),
            op: CmpOp::Gt,
            right: "y".to_string(),
        };
        assert_eq!(format!("{}", c), "x > y");
    }

    #[test]
    fn display_arithmetic() {
        let c = Constraint::Arithmetic {
            var: "x".to_string(),
            arith_op: ArithOp::Add,
            arith_const: 1,
            cmp_op: CmpOp::Gt,
            cmp_value: 0,
        };
        assert_eq!(format!("{}", c), "(x + 1) > 0");
    }

    #[test]
    fn display_effect_budget() {
        let c = Constraint::EffectBudget {
            effect_name: "network".to_string(),
            max_calls: 3,
            actual_calls: 2,
        };
        assert_eq!(format!("{}", c), "effect_budget(network, max=3, actual=2)");
    }

    #[test]
    fn substitute_arithmetic_constraint() {
        // (x + 1) > 0, substitute x = 5 → (6 > 0) → true
        let c = Constraint::Arithmetic {
            var: "x".to_string(),
            arith_op: ArithOp::Add,
            arith_const: 1,
            cmp_op: CmpOp::Gt,
            cmp_value: 0,
        };
        assert_eq!(c.substitute_int("x", 5), Constraint::BoolConst(true));
    }

    #[test]
    fn substitute_var_comparison_left() {
        // x > y, substitute x = 10 → y < 10
        let c = Constraint::VarComparison {
            left: "x".to_string(),
            op: CmpOp::Gt,
            right: "y".to_string(),
        };
        let result = c.substitute_int("x", 10);
        assert_eq!(
            result,
            Constraint::IntComparison {
                var: "y".to_string(),
                op: CmpOp::Lt,
                value: 10,
            },
        );
    }
}
