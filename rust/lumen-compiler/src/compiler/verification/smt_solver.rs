//! SMT solver abstraction layer with Z3/CVC5 process bindings and builtin fallback.
//!
//! This module provides a unified `SmtSolver` trait for satisfiability checking,
//! with three backends:
//!
//! - **`BuiltinSmtSolver`** — Enhanced fallback solver handling QF_LIA, boolean
//!   logic, and simple comparisons. Always available.
//! - **`Z3ProcessSolver`** — Communicates with Z3 via SMT-LIB2 over stdin/stdout.
//! - **`Cvc5ProcessSolver`** — Communicates with CVC5 via SMT-LIB2 over stdin/stdout.
//!
//! Use `SmtSolverFactory::create_best_available()` to get the most capable
//! solver that is installed on the current system.

use std::collections::HashMap;
use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use super::constraints::{ArithOp, CmpOp, Constraint};

// ── SMT Expression Types ────────────────────────────────────────────

/// Sort (type) in the SMT universe.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SmtSort {
    Bool,
    Int,
    Float,
    String,
    BitVec(u32),
    Array(Box<SmtSort>, Box<SmtSort>),
    Uninterpreted(std::string::String),
}

impl fmt::Display for SmtSort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SmtSort::Bool => write!(f, "Bool"),
            SmtSort::Int => write!(f, "Int"),
            SmtSort::Float => write!(f, "Real"),
            SmtSort::String => write!(f, "String"),
            SmtSort::BitVec(w) => write!(f, "(_ BitVec {})", w),
            SmtSort::Array(idx, elem) => write!(f, "(Array {} {})", idx, elem),
            SmtSort::Uninterpreted(name) => write!(f, "{}", name),
        }
    }
}

/// SMT-LIB2-compatible expression AST.
#[derive(Debug, Clone, PartialEq)]
pub enum SmtExpr {
    // Constants
    IntConst(i64),
    BoolConst(bool),
    FloatConst(f64),
    StringConst(std::string::String),

    // Variables
    Var(std::string::String, SmtSort),

    // Arithmetic
    Add(Box<SmtExpr>, Box<SmtExpr>),
    Sub(Box<SmtExpr>, Box<SmtExpr>),
    Mul(Box<SmtExpr>, Box<SmtExpr>),
    Div(Box<SmtExpr>, Box<SmtExpr>),
    Mod(Box<SmtExpr>, Box<SmtExpr>),
    Neg(Box<SmtExpr>),

    // Comparison
    Eq(Box<SmtExpr>, Box<SmtExpr>),
    Ne(Box<SmtExpr>, Box<SmtExpr>),
    Lt(Box<SmtExpr>, Box<SmtExpr>),
    Le(Box<SmtExpr>, Box<SmtExpr>),
    Gt(Box<SmtExpr>, Box<SmtExpr>),
    Ge(Box<SmtExpr>, Box<SmtExpr>),

    // Logical
    And(Vec<SmtExpr>),
    Or(Vec<SmtExpr>),
    Not(Box<SmtExpr>),
    Implies(Box<SmtExpr>, Box<SmtExpr>),
    Iff(Box<SmtExpr>, Box<SmtExpr>),

    // Quantifiers
    ForAll(Vec<(std::string::String, SmtSort)>, Box<SmtExpr>),
    Exists(Vec<(std::string::String, SmtSort)>, Box<SmtExpr>),

    // Array theory
    ArraySelect(Box<SmtExpr>, Box<SmtExpr>),
    ArrayStore(Box<SmtExpr>, Box<SmtExpr>, Box<SmtExpr>),

    // Bitvector theory
    BvAnd(Box<SmtExpr>, Box<SmtExpr>),
    BvOr(Box<SmtExpr>, Box<SmtExpr>),
    BvShiftLeft(Box<SmtExpr>, Box<SmtExpr>),
    BvShiftRight(Box<SmtExpr>, Box<SmtExpr>),

    // If-then-else
    Ite(Box<SmtExpr>, Box<SmtExpr>, Box<SmtExpr>),

    // Function application
    Apply(std::string::String, Vec<SmtExpr>),
}

impl fmt::Display for SmtExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_smtlib2())
    }
}

impl SmtExpr {
    /// Convert this expression to an SMT-LIB2 string.
    pub fn to_smtlib2(&self) -> std::string::String {
        match self {
            SmtExpr::IntConst(v) => {
                if *v < 0 {
                    format!("(- {})", v.saturating_neg())
                } else {
                    format!("{}", v)
                }
            }
            SmtExpr::BoolConst(b) => {
                if *b {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            SmtExpr::FloatConst(v) => {
                if *v < 0.0 {
                    format!("(- {})", -v)
                } else if v.fract() == 0.0 {
                    format!("{}.0", v)
                } else {
                    format!("{}", v)
                }
            }
            SmtExpr::StringConst(s) => {
                format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
            }
            SmtExpr::Var(name, _sort) => name.clone(),
            SmtExpr::Add(a, b) => format!("(+ {} {})", a.to_smtlib2(), b.to_smtlib2()),
            SmtExpr::Sub(a, b) => format!("(- {} {})", a.to_smtlib2(), b.to_smtlib2()),
            SmtExpr::Mul(a, b) => format!("(* {} {})", a.to_smtlib2(), b.to_smtlib2()),
            SmtExpr::Div(a, b) => format!("(div {} {})", a.to_smtlib2(), b.to_smtlib2()),
            SmtExpr::Mod(a, b) => format!("(mod {} {})", a.to_smtlib2(), b.to_smtlib2()),
            SmtExpr::Neg(a) => format!("(- {})", a.to_smtlib2()),
            SmtExpr::Eq(a, b) => format!("(= {} {})", a.to_smtlib2(), b.to_smtlib2()),
            SmtExpr::Ne(a, b) => format!("(not (= {} {}))", a.to_smtlib2(), b.to_smtlib2()),
            SmtExpr::Lt(a, b) => format!("(< {} {})", a.to_smtlib2(), b.to_smtlib2()),
            SmtExpr::Le(a, b) => format!("(<= {} {})", a.to_smtlib2(), b.to_smtlib2()),
            SmtExpr::Gt(a, b) => format!("(> {} {})", a.to_smtlib2(), b.to_smtlib2()),
            SmtExpr::Ge(a, b) => format!("(>= {} {})", a.to_smtlib2(), b.to_smtlib2()),
            SmtExpr::And(parts) => {
                if parts.is_empty() {
                    "true".to_string()
                } else if parts.len() == 1 {
                    parts[0].to_smtlib2()
                } else {
                    let inner: Vec<_> = parts.iter().map(|p| p.to_smtlib2()).collect();
                    format!("(and {})", inner.join(" "))
                }
            }
            SmtExpr::Or(parts) => {
                if parts.is_empty() {
                    "false".to_string()
                } else if parts.len() == 1 {
                    parts[0].to_smtlib2()
                } else {
                    let inner: Vec<_> = parts.iter().map(|p| p.to_smtlib2()).collect();
                    format!("(or {})", inner.join(" "))
                }
            }
            SmtExpr::Not(a) => format!("(not {})", a.to_smtlib2()),
            SmtExpr::Implies(a, b) => format!("(=> {} {})", a.to_smtlib2(), b.to_smtlib2()),
            SmtExpr::Iff(a, b) => format!("(= {} {})", a.to_smtlib2(), b.to_smtlib2()),
            SmtExpr::ForAll(vars, body) => {
                let bindings: Vec<_> = vars
                    .iter()
                    .map(|(name, sort)| format!("({} {})", name, sort))
                    .collect();
                format!("(forall ({}) {})", bindings.join(" "), body.to_smtlib2())
            }
            SmtExpr::Exists(vars, body) => {
                let bindings: Vec<_> = vars
                    .iter()
                    .map(|(name, sort)| format!("({} {})", name, sort))
                    .collect();
                format!("(exists ({}) {})", bindings.join(" "), body.to_smtlib2())
            }
            SmtExpr::ArraySelect(arr, idx) => {
                format!("(select {} {})", arr.to_smtlib2(), idx.to_smtlib2())
            }
            SmtExpr::ArrayStore(arr, idx, val) => format!(
                "(store {} {} {})",
                arr.to_smtlib2(),
                idx.to_smtlib2(),
                val.to_smtlib2()
            ),
            SmtExpr::BvAnd(a, b) => format!("(bvand {} {})", a.to_smtlib2(), b.to_smtlib2()),
            SmtExpr::BvOr(a, b) => format!("(bvor {} {})", a.to_smtlib2(), b.to_smtlib2()),
            SmtExpr::BvShiftLeft(a, b) => {
                format!("(bvshl {} {})", a.to_smtlib2(), b.to_smtlib2())
            }
            SmtExpr::BvShiftRight(a, b) => {
                format!("(bvlshr {} {})", a.to_smtlib2(), b.to_smtlib2())
            }
            SmtExpr::Ite(cond, then_val, else_val) => format!(
                "(ite {} {} {})",
                cond.to_smtlib2(),
                then_val.to_smtlib2(),
                else_val.to_smtlib2()
            ),
            SmtExpr::Apply(func, args) => {
                if args.is_empty() {
                    func.clone()
                } else {
                    let arg_strs: Vec<_> = args.iter().map(|a| a.to_smtlib2()).collect();
                    format!("({} {})", func, arg_strs.join(" "))
                }
            }
        }
    }

    /// Collect all free variables (Var nodes) from this expression.
    pub fn collect_vars(&self) -> Vec<(std::string::String, SmtSort)> {
        let mut vars = Vec::new();
        let mut seen = std::collections::HashSet::new();
        self.collect_vars_inner(&mut vars, &mut seen);
        vars
    }

    fn collect_vars_inner(
        &self,
        vars: &mut Vec<(std::string::String, SmtSort)>,
        seen: &mut std::collections::HashSet<std::string::String>,
    ) {
        match self {
            SmtExpr::Var(name, sort) => {
                if seen.insert(name.clone()) {
                    vars.push((name.clone(), sort.clone()));
                }
            }
            SmtExpr::Add(a, b)
            | SmtExpr::Sub(a, b)
            | SmtExpr::Mul(a, b)
            | SmtExpr::Div(a, b)
            | SmtExpr::Mod(a, b)
            | SmtExpr::Eq(a, b)
            | SmtExpr::Ne(a, b)
            | SmtExpr::Lt(a, b)
            | SmtExpr::Le(a, b)
            | SmtExpr::Gt(a, b)
            | SmtExpr::Ge(a, b)
            | SmtExpr::Implies(a, b)
            | SmtExpr::Iff(a, b)
            | SmtExpr::BvAnd(a, b)
            | SmtExpr::BvOr(a, b)
            | SmtExpr::BvShiftLeft(a, b)
            | SmtExpr::BvShiftRight(a, b)
            | SmtExpr::ArraySelect(a, b) => {
                a.collect_vars_inner(vars, seen);
                b.collect_vars_inner(vars, seen);
            }
            SmtExpr::ArrayStore(a, b, c) | SmtExpr::Ite(a, b, c) => {
                a.collect_vars_inner(vars, seen);
                b.collect_vars_inner(vars, seen);
                c.collect_vars_inner(vars, seen);
            }
            SmtExpr::Neg(a) | SmtExpr::Not(a) => {
                a.collect_vars_inner(vars, seen);
            }
            SmtExpr::And(parts) | SmtExpr::Or(parts) => {
                for p in parts {
                    p.collect_vars_inner(vars, seen);
                }
            }
            SmtExpr::ForAll(_, body) | SmtExpr::Exists(_, body) => {
                body.collect_vars_inner(vars, seen);
            }
            SmtExpr::Apply(_, args) => {
                for a in args {
                    a.collect_vars_inner(vars, seen);
                }
            }
            // Leaf constants have no vars
            SmtExpr::IntConst(_)
            | SmtExpr::BoolConst(_)
            | SmtExpr::FloatConst(_)
            | SmtExpr::StringConst(_) => {}
        }
    }
}

// ── Result and Model types ──────────────────────────────────────────

/// Result of a satisfiability check.
#[derive(Debug, Clone, PartialEq)]
pub enum SmtResult {
    Sat,
    Unsat,
    Unknown(std::string::String),
    Timeout,
    Error(std::string::String),
}

impl SmtResult {
    /// Returns true if the result is `Sat`.
    pub fn is_sat(&self) -> bool {
        matches!(self, SmtResult::Sat)
    }

    /// Returns true if the result is `Unsat`.
    pub fn is_unsat(&self) -> bool {
        matches!(self, SmtResult::Unsat)
    }
}

/// A value assigned to a variable in a satisfying model.
#[derive(Debug, Clone, PartialEq)]
pub enum SmtValue {
    Int(i64),
    Bool(bool),
    Float(f64),
    String(std::string::String),
    BitVec(Vec<u8>),
}

impl fmt::Display for SmtValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SmtValue::Int(v) => write!(f, "{}", v),
            SmtValue::Bool(b) => write!(f, "{}", b),
            SmtValue::Float(v) => write!(f, "{}", v),
            SmtValue::String(s) => write!(f, "\"{}\"", s),
            SmtValue::BitVec(bv) => {
                write!(f, "#b")?;
                for byte in bv {
                    write!(f, "{:08b}", byte)?;
                }
                Ok(())
            }
        }
    }
}

/// A model (satisfying assignment) returned by the solver.
#[derive(Debug, Clone, PartialEq)]
pub struct SmtModel {
    pub assignments: HashMap<std::string::String, SmtValue>,
}

impl SmtModel {
    /// Create a new empty model.
    pub fn new() -> Self {
        Self {
            assignments: HashMap::new(),
        }
    }

    /// Get the value assigned to a variable.
    pub fn get(&self, var: &str) -> Option<&SmtValue> {
        self.assignments.get(var)
    }
}

impl Default for SmtModel {
    fn default() -> Self {
        Self::new()
    }
}

// ── SMT Theory enum ─────────────────────────────────────────────────

/// SMT-LIB theories that a solver may support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SmtTheory {
    /// Quantifier-free linear integer arithmetic
    QfLia,
    /// Quantifier-free linear real arithmetic
    QfLra,
    /// Quantifier-free bitvectors
    QfBv,
    /// Quantifier-free arrays
    QfAx,
    /// Quantifier-free nonlinear integer arithmetic
    QfNia,
    /// Linear integer arithmetic with quantifiers
    Lia,
    /// Array theory
    Arrays,
    /// String theory
    Strings,
}

impl fmt::Display for SmtTheory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SmtTheory::QfLia => write!(f, "QF_LIA"),
            SmtTheory::QfLra => write!(f, "QF_LRA"),
            SmtTheory::QfBv => write!(f, "QF_BV"),
            SmtTheory::QfAx => write!(f, "QF_AX"),
            SmtTheory::QfNia => write!(f, "QF_NIA"),
            SmtTheory::Lia => write!(f, "LIA"),
            SmtTheory::Arrays => write!(f, "ARRAYS"),
            SmtTheory::Strings => write!(f, "STRINGS"),
        }
    }
}

// ── SmtSolver trait ─────────────────────────────────────────────────

/// Unified SMT solver interface.
pub trait SmtSolver: Send + Sync {
    /// Check satisfiability of the given assertions.
    fn check_sat(&self, assertions: &[SmtExpr]) -> SmtResult;

    /// Check satisfiability and return a model if sat.
    fn check_sat_with_model(&self, assertions: &[SmtExpr]) -> (SmtResult, Option<SmtModel>);

    /// Push a new assertion scope (for incremental solving).
    fn push(&mut self);

    /// Pop the most recent assertion scope.
    fn pop(&mut self);

    /// Reset the solver to a clean state.
    fn reset(&mut self);

    /// Return the solver's name.
    fn solver_name(&self) -> &str;

    /// Check whether this solver supports a given theory.
    fn supports_theory(&self, theory: SmtTheory) -> bool;
}

// ── BuiltinSmtSolver ────────────────────────────────────────────────

/// Enhanced fallback solver that handles quantifier-free linear integer
/// arithmetic and boolean combinations without external dependencies.
///
/// Capabilities:
/// - Linear integer arithmetic (add, sub, mul by constants)
/// - Boolean combinations (and, or, not, implies)
/// - Simple equality and comparison
/// - Basic quantifier-free satisfiability
///
/// Returns `Unknown` for bitvectors, arrays, quantifiers, nonlinear, strings.
#[derive(Debug)]
pub struct BuiltinSmtSolver {
    /// Stack of assertion-set snapshots.
    scope_stack: Vec<usize>,
    /// Persistent assertions across push/pop.
    assertions: Vec<SmtExpr>,
}

impl BuiltinSmtSolver {
    pub fn new() -> Self {
        Self {
            scope_stack: Vec::new(),
            assertions: Vec::new(),
        }
    }

    /// Evaluate a single SmtExpr for satisfiability.
    fn evaluate_expr(&self, expr: &SmtExpr) -> SmtResult {
        match expr {
            SmtExpr::BoolConst(true) => SmtResult::Sat,
            SmtExpr::BoolConst(false) => SmtResult::Unsat,

            // A single variable is satisfiable (assign it true or any value)
            SmtExpr::Var(_, SmtSort::Bool) => SmtResult::Sat,
            SmtExpr::Var(_, _) => SmtResult::Sat,

            // Integer/float constants are not boolean — treat as unknown
            SmtExpr::IntConst(_) | SmtExpr::FloatConst(_) | SmtExpr::StringConst(_) => {
                SmtResult::Unknown("non-boolean constant".to_string())
            }

            SmtExpr::Not(inner) => match self.evaluate_expr(inner) {
                SmtResult::Sat => {
                    // not(sat) — might still be sat if inner is not a tautology
                    match inner.as_ref() {
                        SmtExpr::BoolConst(true) => SmtResult::Unsat,
                        SmtExpr::BoolConst(false) => SmtResult::Sat,
                        _ => {
                            // Try: if inner is definitely a tautology, return Unsat
                            // Otherwise, it's often Unknown or Sat
                            SmtResult::Unknown("negation of satisfiable formula".to_string())
                        }
                    }
                }
                SmtResult::Unsat => SmtResult::Sat,
                other => other,
            },

            SmtExpr::And(parts) => {
                if parts.is_empty() {
                    return SmtResult::Sat;
                }
                self.evaluate_conjunction(parts)
            }

            SmtExpr::Or(parts) => {
                if parts.is_empty() {
                    return SmtResult::Unsat;
                }
                let mut any_sat = false;
                let mut all_unsat = true;
                for p in parts {
                    match self.evaluate_expr(p) {
                        SmtResult::Sat => {
                            any_sat = true;
                            all_unsat = false;
                        }
                        SmtResult::Unsat => {}
                        _ => {
                            all_unsat = false;
                        }
                    }
                }
                if any_sat {
                    SmtResult::Sat
                } else if all_unsat {
                    SmtResult::Unsat
                } else {
                    SmtResult::Unknown("disjunction with unknown branches".to_string())
                }
            }

            SmtExpr::Implies(a, b) => {
                // a => b is equivalent to (not a) or b
                let rewritten = SmtExpr::Or(vec![SmtExpr::Not(a.clone()), *b.clone()]);
                self.evaluate_expr(&rewritten)
            }

            SmtExpr::Iff(a, b) => {
                // a <=> b is equivalent to (a => b) and (b => a)
                let rewritten = SmtExpr::And(vec![
                    SmtExpr::Implies(a.clone(), b.clone()),
                    SmtExpr::Implies(b.clone(), a.clone()),
                ]);
                self.evaluate_expr(&rewritten)
            }

            // Single comparison: always satisfiable (free variables)
            SmtExpr::Eq(_, _)
            | SmtExpr::Ne(_, _)
            | SmtExpr::Lt(_, _)
            | SmtExpr::Le(_, _)
            | SmtExpr::Gt(_, _)
            | SmtExpr::Ge(_, _) => {
                // Check if both sides are constants — can decide
                if let Some(result) = self.try_eval_comparison(expr) {
                    return result;
                }
                SmtResult::Sat
            }

            SmtExpr::Ite(cond, then_val, else_val) => {
                // ITE: if condition is decidable, evaluate the right branch
                match self.evaluate_expr(cond) {
                    SmtResult::Sat => self.evaluate_expr(then_val),
                    SmtResult::Unsat => self.evaluate_expr(else_val),
                    _ => SmtResult::Unknown("ite with unknown condition".to_string()),
                }
            }

            // Unsupported theories
            SmtExpr::ForAll(_, _) | SmtExpr::Exists(_, _) => {
                SmtResult::Unknown("quantifiers not supported by builtin solver".to_string())
            }
            SmtExpr::ArraySelect(_, _) | SmtExpr::ArrayStore(_, _, _) => {
                SmtResult::Unknown("array theory not supported by builtin solver".to_string())
            }
            SmtExpr::BvAnd(_, _)
            | SmtExpr::BvOr(_, _)
            | SmtExpr::BvShiftLeft(_, _)
            | SmtExpr::BvShiftRight(_, _) => {
                SmtResult::Unknown("bitvector theory not supported by builtin solver".to_string())
            }
            SmtExpr::Add(_, _)
            | SmtExpr::Sub(_, _)
            | SmtExpr::Mul(_, _)
            | SmtExpr::Div(_, _)
            | SmtExpr::Mod(_, _)
            | SmtExpr::Neg(_) => {
                // Arithmetic expressions that aren't part of a comparison
                SmtResult::Unknown("bare arithmetic expression".to_string())
            }
            SmtExpr::Apply(_, _) => SmtResult::Unknown(
                "uninterpreted functions not supported by builtin solver".to_string(),
            ),
        }
    }

    /// Try to evaluate a comparison where both sides are constants.
    fn try_eval_comparison(&self, expr: &SmtExpr) -> Option<SmtResult> {
        match expr {
            SmtExpr::Eq(a, b) => {
                self.try_const_cmp(a, b, |l, r| l == r, |l, r| (l - r).abs() < f64::EPSILON)
            }
            SmtExpr::Ne(a, b) => {
                self.try_const_cmp(a, b, |l, r| l != r, |l, r| (l - r).abs() >= f64::EPSILON)
            }
            SmtExpr::Lt(a, b) => self.try_const_cmp(a, b, |l, r| l < r, |l, r| l < r),
            SmtExpr::Le(a, b) => self.try_const_cmp(a, b, |l, r| l <= r, |l, r| l <= r),
            SmtExpr::Gt(a, b) => self.try_const_cmp(a, b, |l, r| l > r, |l, r| l > r),
            SmtExpr::Ge(a, b) => self.try_const_cmp(a, b, |l, r| l >= r, |l, r| l >= r),
            _ => None,
        }
    }

    fn try_const_cmp(
        &self,
        a: &SmtExpr,
        b: &SmtExpr,
        int_cmp: impl Fn(i64, i64) -> bool,
        float_cmp: impl Fn(f64, f64) -> bool,
    ) -> Option<SmtResult> {
        match (a, b) {
            (SmtExpr::IntConst(l), SmtExpr::IntConst(r)) => {
                if int_cmp(*l, *r) {
                    Some(SmtResult::Sat)
                } else {
                    Some(SmtResult::Unsat)
                }
            }
            (SmtExpr::FloatConst(l), SmtExpr::FloatConst(r)) => {
                if float_cmp(*l, *r) {
                    Some(SmtResult::Sat)
                } else {
                    Some(SmtResult::Unsat)
                }
            }
            (SmtExpr::BoolConst(l), SmtExpr::BoolConst(r)) => {
                if int_cmp(*l as i64, *r as i64) {
                    Some(SmtResult::Sat)
                } else {
                    Some(SmtResult::Unsat)
                }
            }
            _ => None,
        }
    }

    /// Evaluate a conjunction of expressions using interval reasoning.
    fn evaluate_conjunction(&self, parts: &[SmtExpr]) -> SmtResult {
        let mut bounds: HashMap<std::string::String, IntBounds> = HashMap::new();
        let mut has_unknown = false;

        for part in parts {
            match part {
                SmtExpr::BoolConst(false) => return SmtResult::Unsat,
                SmtExpr::BoolConst(true) => {} // no-op

                // var > const, var < const, etc.
                SmtExpr::Gt(a, b) => {
                    if let Some((name, val)) = self.extract_var_int(a, b) {
                        bounds
                            .entry(name)
                            .or_insert_with(IntBounds::new)
                            .apply_gt(val);
                    } else if let Some((name, val)) = self.extract_var_int(b, a) {
                        // b > a ← flipped: name < val
                        bounds
                            .entry(name)
                            .or_insert_with(IntBounds::new)
                            .apply_lt(val);
                    } else if let Some(r) = self.try_eval_comparison(part) {
                        match r {
                            SmtResult::Unsat => return SmtResult::Unsat,
                            SmtResult::Unknown(_) => has_unknown = true,
                            _ => {}
                        }
                    } else {
                        has_unknown = true;
                    }
                }
                SmtExpr::Ge(a, b) => {
                    if let Some((name, val)) = self.extract_var_int(a, b) {
                        bounds
                            .entry(name)
                            .or_insert_with(IntBounds::new)
                            .apply_ge(val);
                    } else if let Some((name, val)) = self.extract_var_int(b, a) {
                        bounds
                            .entry(name)
                            .or_insert_with(IntBounds::new)
                            .apply_le(val);
                    } else if let Some(r) = self.try_eval_comparison(part) {
                        match r {
                            SmtResult::Unsat => return SmtResult::Unsat,
                            SmtResult::Unknown(_) => has_unknown = true,
                            _ => {}
                        }
                    } else {
                        has_unknown = true;
                    }
                }
                SmtExpr::Lt(a, b) => {
                    if let Some((name, val)) = self.extract_var_int(a, b) {
                        bounds
                            .entry(name)
                            .or_insert_with(IntBounds::new)
                            .apply_lt(val);
                    } else if let Some((name, val)) = self.extract_var_int(b, a) {
                        bounds
                            .entry(name)
                            .or_insert_with(IntBounds::new)
                            .apply_gt(val);
                    } else if let Some(r) = self.try_eval_comparison(part) {
                        match r {
                            SmtResult::Unsat => return SmtResult::Unsat,
                            SmtResult::Unknown(_) => has_unknown = true,
                            _ => {}
                        }
                    } else {
                        has_unknown = true;
                    }
                }
                SmtExpr::Le(a, b) => {
                    if let Some((name, val)) = self.extract_var_int(a, b) {
                        bounds
                            .entry(name)
                            .or_insert_with(IntBounds::new)
                            .apply_le(val);
                    } else if let Some((name, val)) = self.extract_var_int(b, a) {
                        bounds
                            .entry(name)
                            .or_insert_with(IntBounds::new)
                            .apply_ge(val);
                    } else if let Some(r) = self.try_eval_comparison(part) {
                        match r {
                            SmtResult::Unsat => return SmtResult::Unsat,
                            SmtResult::Unknown(_) => has_unknown = true,
                            _ => {}
                        }
                    } else {
                        has_unknown = true;
                    }
                }
                SmtExpr::Eq(a, b) => {
                    if let Some((name, val)) = self.extract_var_int(a, b) {
                        bounds
                            .entry(name)
                            .or_insert_with(IntBounds::new)
                            .apply_eq(val);
                    } else if let Some((name, val)) = self.extract_var_int(b, a) {
                        bounds
                            .entry(name)
                            .or_insert_with(IntBounds::new)
                            .apply_eq(val);
                    } else if let Some(r) = self.try_eval_comparison(part) {
                        match r {
                            SmtResult::Unsat => return SmtResult::Unsat,
                            SmtResult::Unknown(_) => has_unknown = true,
                            _ => {}
                        }
                    } else {
                        has_unknown = true;
                    }
                }
                SmtExpr::Ne(a, b) => {
                    if let Some((name, val)) = self.extract_var_int(a, b) {
                        bounds
                            .entry(name)
                            .or_insert_with(IntBounds::new)
                            .apply_neq(val);
                    } else if let Some((name, val)) = self.extract_var_int(b, a) {
                        bounds
                            .entry(name)
                            .or_insert_with(IntBounds::new)
                            .apply_neq(val);
                    } else if let Some(r) = self.try_eval_comparison(part) {
                        match r {
                            SmtResult::Unsat => return SmtResult::Unsat,
                            SmtResult::Unknown(_) => has_unknown = true,
                            _ => {}
                        }
                    } else {
                        has_unknown = true;
                    }
                }
                SmtExpr::Not(inner) => match inner.as_ref() {
                    SmtExpr::BoolConst(true) => return SmtResult::Unsat,
                    SmtExpr::BoolConst(false) => {} // not(false) = true
                    SmtExpr::Gt(a, b) => {
                        // not(a > b) = a <= b
                        if let Some((name, val)) = self.extract_var_int(a, b) {
                            bounds
                                .entry(name)
                                .or_insert_with(IntBounds::new)
                                .apply_le(val);
                        } else if let Some((name, val)) = self.extract_var_int(b, a) {
                            bounds
                                .entry(name)
                                .or_insert_with(IntBounds::new)
                                .apply_ge(val);
                        } else {
                            has_unknown = true;
                        }
                    }
                    SmtExpr::Ge(a, b) => {
                        if let Some((name, val)) = self.extract_var_int(a, b) {
                            bounds
                                .entry(name)
                                .or_insert_with(IntBounds::new)
                                .apply_lt(val);
                        } else if let Some((name, val)) = self.extract_var_int(b, a) {
                            bounds
                                .entry(name)
                                .or_insert_with(IntBounds::new)
                                .apply_gt(val);
                        } else {
                            has_unknown = true;
                        }
                    }
                    SmtExpr::Lt(a, b) => {
                        if let Some((name, val)) = self.extract_var_int(a, b) {
                            bounds
                                .entry(name)
                                .or_insert_with(IntBounds::new)
                                .apply_ge(val);
                        } else if let Some((name, val)) = self.extract_var_int(b, a) {
                            bounds
                                .entry(name)
                                .or_insert_with(IntBounds::new)
                                .apply_le(val);
                        } else {
                            has_unknown = true;
                        }
                    }
                    SmtExpr::Le(a, b) => {
                        if let Some((name, val)) = self.extract_var_int(a, b) {
                            bounds
                                .entry(name)
                                .or_insert_with(IntBounds::new)
                                .apply_gt(val);
                        } else if let Some((name, val)) = self.extract_var_int(b, a) {
                            bounds
                                .entry(name)
                                .or_insert_with(IntBounds::new)
                                .apply_lt(val);
                        } else {
                            has_unknown = true;
                        }
                    }
                    SmtExpr::Eq(a, b) => {
                        if let Some((name, val)) = self.extract_var_int(a, b) {
                            bounds
                                .entry(name)
                                .or_insert_with(IntBounds::new)
                                .apply_neq(val);
                        } else if let Some((name, val)) = self.extract_var_int(b, a) {
                            bounds
                                .entry(name)
                                .or_insert_with(IntBounds::new)
                                .apply_neq(val);
                        } else {
                            has_unknown = true;
                        }
                    }
                    _ => has_unknown = true,
                },

                // Nested And — flatten
                SmtExpr::And(inner) => match self.evaluate_conjunction(inner) {
                    SmtResult::Unsat => return SmtResult::Unsat,
                    SmtResult::Unknown(_) => has_unknown = true,
                    _ => {}
                },

                // Nested Or
                SmtExpr::Or(_) => match self.evaluate_expr(part) {
                    SmtResult::Unsat => return SmtResult::Unsat,
                    SmtResult::Unknown(_) => has_unknown = true,
                    _ => {}
                },

                // Variables in conjunction are satisfiable
                SmtExpr::Var(_, SmtSort::Bool) => {} // bool var is satisfiable
                SmtExpr::Var(_, _) => has_unknown = true,

                _ => match self.evaluate_expr(part) {
                    SmtResult::Unsat => return SmtResult::Unsat,
                    SmtResult::Unknown(_) => has_unknown = true,
                    _ => {}
                },
            }
        }

        // Check per-variable satisfiability
        for b in bounds.values() {
            if !b.is_satisfiable() {
                return SmtResult::Unsat;
            }
        }

        if has_unknown {
            SmtResult::Unknown("some sub-expressions not decidable".to_string())
        } else {
            SmtResult::Sat
        }
    }

    /// Extract (var_name, int_value) from patterns like (Var(...), IntConst(...)).
    fn extract_var_int(
        &self,
        var_side: &SmtExpr,
        const_side: &SmtExpr,
    ) -> Option<(std::string::String, i64)> {
        // Also handle Add/Sub with constants for linear arithmetic
        match (var_side, const_side) {
            (SmtExpr::Var(name, SmtSort::Int), SmtExpr::IntConst(val)) => {
                Some((name.clone(), *val))
            }
            // (x + c) cmp v  →  x cmp (v - c)
            (SmtExpr::Add(a, b), SmtExpr::IntConst(val)) => {
                if let SmtExpr::Var(name, SmtSort::Int) = a.as_ref() {
                    if let SmtExpr::IntConst(c) = b.as_ref() {
                        return Some((name.clone(), val.saturating_sub(*c)));
                    }
                }
                None
            }
            // (x - c) cmp v  →  x cmp (v + c)
            (SmtExpr::Sub(a, b), SmtExpr::IntConst(val)) => {
                if let SmtExpr::Var(name, SmtSort::Int) = a.as_ref() {
                    if let SmtExpr::IntConst(c) = b.as_ref() {
                        return Some((name.clone(), val.saturating_add(*c)));
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Try to build a model from current bounds for sat cases.
    fn try_build_model(&self, assertions: &[SmtExpr]) -> Option<SmtModel> {
        let mut bounds: HashMap<std::string::String, IntBounds> = HashMap::new();
        let mut bool_vars: HashMap<std::string::String, Option<bool>> = HashMap::new();

        // Collect all variables and their bounds
        for assertion in assertions {
            self.collect_bounds(assertion, &mut bounds, &mut bool_vars);
        }

        let mut model = SmtModel::new();

        // Assign integer variables
        for (name, b) in &bounds {
            let val = b.pick_value();
            model.assignments.insert(name.clone(), SmtValue::Int(val));
        }

        // Assign boolean variables
        for (name, val) in &bool_vars {
            model
                .assignments
                .insert(name.clone(), SmtValue::Bool(val.unwrap_or(true)));
        }

        // Collect vars from expressions to ensure all are in the model
        for assertion in assertions {
            for (name, sort) in assertion.collect_vars() {
                model.assignments.entry(name).or_insert_with(|| match sort {
                    SmtSort::Int => SmtValue::Int(0),
                    SmtSort::Bool => SmtValue::Bool(true),
                    SmtSort::Float => SmtValue::Float(0.0),
                    SmtSort::String => SmtValue::String(std::string::String::new()),
                    _ => SmtValue::Int(0),
                });
            }
        }

        if model.assignments.is_empty() {
            None
        } else {
            Some(model)
        }
    }

    fn collect_bounds(
        &self,
        expr: &SmtExpr,
        bounds: &mut HashMap<std::string::String, IntBounds>,
        bool_vars: &mut HashMap<std::string::String, Option<bool>>,
    ) {
        match expr {
            SmtExpr::Gt(a, b) => {
                if let Some((name, val)) = self.extract_var_int(a, b) {
                    bounds
                        .entry(name)
                        .or_insert_with(IntBounds::new)
                        .apply_gt(val);
                }
            }
            SmtExpr::Ge(a, b) => {
                if let Some((name, val)) = self.extract_var_int(a, b) {
                    bounds
                        .entry(name)
                        .or_insert_with(IntBounds::new)
                        .apply_ge(val);
                }
            }
            SmtExpr::Lt(a, b) => {
                if let Some((name, val)) = self.extract_var_int(a, b) {
                    bounds
                        .entry(name)
                        .or_insert_with(IntBounds::new)
                        .apply_lt(val);
                }
            }
            SmtExpr::Le(a, b) => {
                if let Some((name, val)) = self.extract_var_int(a, b) {
                    bounds
                        .entry(name)
                        .or_insert_with(IntBounds::new)
                        .apply_le(val);
                }
            }
            SmtExpr::Eq(a, b) => {
                if let Some((name, val)) = self.extract_var_int(a, b) {
                    bounds
                        .entry(name)
                        .or_insert_with(IntBounds::new)
                        .apply_eq(val);
                }
            }
            SmtExpr::Var(name, SmtSort::Bool) => {
                bool_vars.entry(name.clone()).or_insert(None);
            }
            SmtExpr::And(parts) => {
                for p in parts {
                    self.collect_bounds(p, bounds, bool_vars);
                }
            }
            _ => {}
        }
    }
}

impl Default for BuiltinSmtSolver {
    fn default() -> Self {
        Self::new()
    }
}

impl SmtSolver for BuiltinSmtSolver {
    fn check_sat(&self, assertions: &[SmtExpr]) -> SmtResult {
        if assertions.is_empty() {
            return SmtResult::Sat;
        }

        let all: Vec<SmtExpr> = self
            .assertions
            .iter()
            .chain(assertions.iter())
            .cloned()
            .collect();

        if all.len() == 1 {
            self.evaluate_expr(&all[0])
        } else {
            self.evaluate_conjunction(&all)
        }
    }

    fn check_sat_with_model(&self, assertions: &[SmtExpr]) -> (SmtResult, Option<SmtModel>) {
        let result = self.check_sat(assertions);
        match &result {
            SmtResult::Sat => {
                let all: Vec<SmtExpr> = self
                    .assertions
                    .iter()
                    .chain(assertions.iter())
                    .cloned()
                    .collect();
                let model = self.try_build_model(&all);
                (result, model)
            }
            _ => (result, None),
        }
    }

    fn push(&mut self) {
        self.scope_stack.push(self.assertions.len());
    }

    fn pop(&mut self) {
        if let Some(len) = self.scope_stack.pop() {
            self.assertions.truncate(len);
        }
    }

    fn reset(&mut self) {
        self.assertions.clear();
        self.scope_stack.clear();
    }

    fn solver_name(&self) -> &str {
        "builtin"
    }

    fn supports_theory(&self, theory: SmtTheory) -> bool {
        matches!(theory, SmtTheory::QfLia | SmtTheory::QfLra)
    }
}

/// Internal integer bounds tracker for the builtin solver.
#[derive(Debug, Clone)]
struct IntBounds {
    /// Exclusive lower bound: var > lower.
    lower: Option<i64>,
    /// Inclusive lower bound: var >= lower_eq.
    lower_eq: Option<i64>,
    /// Exclusive upper bound: var < upper.
    upper: Option<i64>,
    /// Inclusive upper bound: var <= upper_eq.
    upper_eq: Option<i64>,
    /// Required equality value.
    eq: Option<i64>,
    /// Forbidden values.
    neq: Vec<i64>,
}

impl IntBounds {
    fn new() -> Self {
        Self {
            lower: None,
            lower_eq: None,
            upper: None,
            upper_eq: None,
            eq: None,
            neq: Vec::new(),
        }
    }

    fn apply_gt(&mut self, value: i64) {
        self.lower = Some(match self.lower {
            Some(prev) => prev.max(value),
            None => value,
        });
    }

    fn apply_ge(&mut self, value: i64) {
        self.lower_eq = Some(match self.lower_eq {
            Some(prev) => prev.max(value),
            None => value,
        });
    }

    fn apply_lt(&mut self, value: i64) {
        self.upper = Some(match self.upper {
            Some(prev) => prev.min(value),
            None => value,
        });
    }

    fn apply_le(&mut self, value: i64) {
        self.upper_eq = Some(match self.upper_eq {
            Some(prev) => prev.min(value),
            None => value,
        });
    }

    fn apply_eq(&mut self, value: i64) {
        self.eq = Some(value);
    }

    fn apply_neq(&mut self, value: i64) {
        self.neq.push(value);
    }

    /// Return the effective inclusive lower bound.
    fn effective_lower(&self) -> Option<i64> {
        match (self.lower, self.lower_eq) {
            (Some(gt), Some(ge)) => Some(gt.saturating_add(1).max(ge)),
            (Some(gt), None) => Some(gt.saturating_add(1)),
            (None, Some(ge)) => Some(ge),
            (None, None) => None,
        }
    }

    /// Return the effective inclusive upper bound.
    fn effective_upper(&self) -> Option<i64> {
        match (self.upper, self.upper_eq) {
            (Some(lt), Some(le)) => Some(lt.saturating_sub(1).min(le)),
            (Some(lt), None) => Some(lt.saturating_sub(1)),
            (None, Some(le)) => Some(le),
            (None, None) => None,
        }
    }

    fn is_satisfiable(&self) -> bool {
        let lo = self.effective_lower();
        let hi = self.effective_upper();

        if let Some(eq_val) = self.eq {
            if let Some(lo) = lo {
                if eq_val < lo {
                    return false;
                }
            }
            if let Some(hi) = hi {
                if eq_val > hi {
                    return false;
                }
            }
            if self.neq.contains(&eq_val) {
                return false;
            }
            return true;
        }

        match (lo, hi) {
            (Some(lo), Some(hi)) => {
                if lo > hi {
                    return false;
                }
                let range_size = (hi as i128) - (lo as i128) + 1;
                if range_size <= self.neq.len() as i128 {
                    let all_forbidden = (lo..=hi).all(|v| self.neq.contains(&v));
                    if all_forbidden {
                        return false;
                    }
                }
                true
            }
            _ => true,
        }
    }

    /// Pick a satisfying value for model construction.
    fn pick_value(&self) -> i64 {
        if let Some(eq_val) = self.eq {
            return eq_val;
        }
        let lo = self.effective_lower().unwrap_or(0);
        let hi = self.effective_upper().unwrap_or(lo.saturating_add(100));
        // Pick a value in range that's not forbidden
        for v in lo..=hi {
            if !self.neq.contains(&v) {
                return v;
            }
        }
        lo
    }
}

// ── Z3ProcessSolver ─────────────────────────────────────────────────

/// SMT solver that communicates with Z3 via SMT-LIB2 over stdin/stdout.
pub struct Z3ProcessSolver {
    /// Timeout for Z3 queries in milliseconds.
    timeout_ms: u64,
    /// Scope depth for push/pop.
    scope_depth: u32,
}

impl Z3ProcessSolver {
    /// Create a new Z3 process solver with a default 5-second timeout.
    pub fn new() -> Option<Self> {
        if Self::is_available() {
            Some(Self {
                timeout_ms: 5000,
                scope_depth: 0,
            })
        } else {
            None
        }
    }

    /// Create with a custom timeout.
    pub fn with_timeout(timeout_ms: u64) -> Option<Self> {
        if Self::is_available() {
            Some(Self {
                timeout_ms,
                scope_depth: 0,
            })
        } else {
            None
        }
    }

    /// Check if Z3 is available on the system.
    pub fn is_available() -> bool {
        Command::new("z3")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
    }

    /// Build a complete SMT-LIB2 script from assertions.
    fn build_script(&self, assertions: &[SmtExpr], get_model: bool) -> std::string::String {
        let mut script = std::string::String::new();
        script.push_str("(set-option :produce-models true)\n");
        script.push_str(&format!("(set-option :timeout {})\n", self.timeout_ms));
        script.push_str("(set-logic ALL)\n");

        // Collect and declare all variables
        let mut all_vars = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for assertion in assertions {
            for (name, sort) in assertion.collect_vars() {
                if seen.insert(name.clone()) {
                    all_vars.push((name, sort));
                }
            }
        }
        for (name, sort) in &all_vars {
            script.push_str(&format!("(declare-const {} {})\n", name, sort));
        }

        // Assert all expressions
        for assertion in assertions {
            script.push_str(&format!("(assert {})\n", assertion.to_smtlib2()));
        }

        script.push_str("(check-sat)\n");
        if get_model {
            script.push_str("(get-model)\n");
        }
        script.push_str("(exit)\n");
        script
    }

    /// Run Z3 with the given script and parse the result.
    fn run_z3(&self, script: &str) -> (SmtResult, std::string::String) {
        let timeout = Duration::from_millis(self.timeout_ms);
        let child = Command::new("z3")
            .arg("-in")
            .arg("-smt2")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                return (
                    SmtResult::Error(format!("failed to spawn z3: {}", e)),
                    std::string::String::new(),
                );
            }
        };

        // Write script to stdin
        if let Some(ref mut stdin) = child.stdin {
            if let Err(e) = stdin.write_all(script.as_bytes()) {
                return (
                    SmtResult::Error(format!("failed to write to z3 stdin: {}", e)),
                    std::string::String::new(),
                );
            }
        }
        drop(child.stdin.take());

        // Wait for result with timeout
        match child.wait_timeout(timeout) {
            Ok(Some(_status)) => {
                let stdout = child
                    .stdout
                    .map(|s| {
                        let reader = BufReader::new(s);
                        reader
                            .lines()
                            .map_while(Result::ok)
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();

                let result = self.parse_result(&stdout);
                (result, stdout)
            }
            Ok(None) => {
                // Timeout — kill the process
                let _ = child.kill();
                let _ = child.wait();
                (SmtResult::Timeout, std::string::String::new())
            }
            Err(e) => (
                SmtResult::Error(format!("failed to wait for z3: {}", e)),
                std::string::String::new(),
            ),
        }
    }

    /// Parse Z3's output into an SmtResult.
    fn parse_result(&self, output: &str) -> SmtResult {
        let first_line = output.lines().next().unwrap_or("").trim();
        match first_line {
            "sat" => SmtResult::Sat,
            "unsat" => SmtResult::Unsat,
            "unknown" => SmtResult::Unknown("z3 returned unknown".to_string()),
            "timeout" => SmtResult::Timeout,
            _ => {
                if first_line.starts_with("(error") {
                    SmtResult::Error(first_line.to_string())
                } else {
                    SmtResult::Error(format!("unexpected z3 output: {}", first_line))
                }
            }
        }
    }

    /// Parse a model from Z3 output.
    fn parse_model(&self, output: &str) -> Option<SmtModel> {
        // Look for the model section after "sat"
        let model_start = output.find("(model")?;
        let model_text = &output[model_start..];

        let mut model = SmtModel::new();

        // Simple parser for (define-fun name () Type value)
        for line in model_text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("(define-fun ") {
                if let Some(assignment) = self.parse_define_fun(trimmed) {
                    model.assignments.insert(assignment.0, assignment.1);
                }
            }
        }

        if model.assignments.is_empty() {
            None
        } else {
            Some(model)
        }
    }

    fn parse_define_fun(&self, line: &str) -> Option<(std::string::String, SmtValue)> {
        // Format: (define-fun name () Type value)
        let inner = line.strip_prefix("(define-fun ")?.strip_suffix(')')?;
        let parts: Vec<&str> = inner.splitn(4, ' ').collect();
        if parts.len() < 4 {
            return None;
        }
        let name = parts[0].to_string();
        let value_str = parts[3].trim().trim_end_matches(')');

        // Try to parse as integer
        if let Ok(v) = value_str.parse::<i64>() {
            return Some((name, SmtValue::Int(v)));
        }
        // Try negative: (- N)
        if value_str.starts_with("(- ") {
            let num_str = value_str.strip_prefix("(- ")?.strip_suffix(')')?;
            if let Ok(v) = num_str.parse::<i64>() {
                return Some((name, SmtValue::Int(-v)));
            }
        }
        // Boolean
        match value_str {
            "true" => return Some((name, SmtValue::Bool(true))),
            "false" => return Some((name, SmtValue::Bool(false))),
            _ => {}
        }

        None
    }
}

impl Default for Z3ProcessSolver {
    fn default() -> Self {
        Self {
            timeout_ms: 5000,
            scope_depth: 0,
        }
    }
}

/// Extension trait for `std::process::Child` to support timeouts.
trait ChildExt {
    fn wait_timeout(
        &mut self,
        timeout: Duration,
    ) -> std::io::Result<Option<std::process::ExitStatus>>;
}

impl ChildExt for std::process::Child {
    fn wait_timeout(
        &mut self,
        timeout: Duration,
    ) -> std::io::Result<Option<std::process::ExitStatus>> {
        let start = std::time::Instant::now();
        loop {
            match self.try_wait()? {
                Some(status) => return Ok(Some(status)),
                None => {
                    if start.elapsed() >= timeout {
                        return Ok(None);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }
}

impl SmtSolver for Z3ProcessSolver {
    fn check_sat(&self, assertions: &[SmtExpr]) -> SmtResult {
        let script = self.build_script(assertions, false);
        let (result, _) = self.run_z3(&script);
        result
    }

    fn check_sat_with_model(&self, assertions: &[SmtExpr]) -> (SmtResult, Option<SmtModel>) {
        let script = self.build_script(assertions, true);
        let (result, output) = self.run_z3(&script);
        let model = if result.is_sat() {
            self.parse_model(&output)
        } else {
            None
        };
        (result, model)
    }

    fn push(&mut self) {
        self.scope_depth = self.scope_depth.saturating_add(1);
    }

    fn pop(&mut self) {
        self.scope_depth = self.scope_depth.saturating_sub(1);
    }

    fn reset(&mut self) {
        self.scope_depth = 0;
    }

    fn solver_name(&self) -> &str {
        "z3"
    }

    fn supports_theory(&self, _theory: SmtTheory) -> bool {
        true // Z3 supports all theories
    }
}

// ── Cvc5ProcessSolver ───────────────────────────────────────────────

/// SMT solver that communicates with CVC5 via SMT-LIB2 over stdin/stdout.
pub struct Cvc5ProcessSolver {
    /// Timeout for CVC5 queries in milliseconds.
    timeout_ms: u64,
    /// Scope depth for push/pop.
    scope_depth: u32,
}

impl Cvc5ProcessSolver {
    /// Create a new CVC5 process solver with a default 5-second timeout.
    pub fn new() -> Option<Self> {
        if Self::is_available() {
            Some(Self {
                timeout_ms: 5000,
                scope_depth: 0,
            })
        } else {
            None
        }
    }

    /// Create with a custom timeout.
    pub fn with_timeout(timeout_ms: u64) -> Option<Self> {
        if Self::is_available() {
            Some(Self {
                timeout_ms,
                scope_depth: 0,
            })
        } else {
            None
        }
    }

    /// Check if CVC5 is available on the system.
    pub fn is_available() -> bool {
        Command::new("cvc5")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
    }

    /// Build a complete SMT-LIB2 script.
    fn build_script(&self, assertions: &[SmtExpr], get_model: bool) -> std::string::String {
        let mut script = std::string::String::new();
        script.push_str("(set-option :produce-models true)\n");
        script.push_str("(set-logic ALL)\n");

        // Collect and declare all variables
        let mut all_vars = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for assertion in assertions {
            for (name, sort) in assertion.collect_vars() {
                if seen.insert(name.clone()) {
                    all_vars.push((name, sort));
                }
            }
        }
        for (name, sort) in &all_vars {
            script.push_str(&format!("(declare-const {} {})\n", name, sort));
        }

        for assertion in assertions {
            script.push_str(&format!("(assert {})\n", assertion.to_smtlib2()));
        }

        script.push_str("(check-sat)\n");
        if get_model {
            script.push_str("(get-model)\n");
        }
        script.push_str("(exit)\n");
        script
    }

    /// Run CVC5 with the given script and parse the result.
    fn run_cvc5(&self, script: &str) -> (SmtResult, std::string::String) {
        let timeout = Duration::from_millis(self.timeout_ms);
        let timeout_secs = (self.timeout_ms / 1000).max(1);
        let child = Command::new("cvc5")
            .arg("--lang=smt2")
            .arg("--incremental")
            .arg(format!("--tlimit={}", self.timeout_ms))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                return (
                    SmtResult::Error(format!("failed to spawn cvc5: {}", e)),
                    std::string::String::new(),
                );
            }
        };

        if let Some(ref mut stdin) = child.stdin {
            if let Err(e) = stdin.write_all(script.as_bytes()) {
                return (
                    SmtResult::Error(format!("failed to write to cvc5 stdin: {}", e)),
                    std::string::String::new(),
                );
            }
        }
        drop(child.stdin.take());

        let _ = timeout_secs; // Used in timeout arg
        match child.wait_timeout(timeout) {
            Ok(Some(_status)) => {
                let stdout = child
                    .stdout
                    .map(|s| {
                        let reader = BufReader::new(s);
                        reader
                            .lines()
                            .map_while(Result::ok)
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();

                let result = self.parse_result(&stdout);
                (result, stdout)
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                (SmtResult::Timeout, std::string::String::new())
            }
            Err(e) => (
                SmtResult::Error(format!("failed to wait for cvc5: {}", e)),
                std::string::String::new(),
            ),
        }
    }

    fn parse_result(&self, output: &str) -> SmtResult {
        let first_line = output.lines().next().unwrap_or("").trim();
        match first_line {
            "sat" => SmtResult::Sat,
            "unsat" => SmtResult::Unsat,
            "unknown" => SmtResult::Unknown("cvc5 returned unknown".to_string()),
            _ => {
                if first_line.starts_with("(error") {
                    SmtResult::Error(first_line.to_string())
                } else {
                    SmtResult::Error(format!("unexpected cvc5 output: {}", first_line))
                }
            }
        }
    }
}

impl Default for Cvc5ProcessSolver {
    fn default() -> Self {
        Self {
            timeout_ms: 5000,
            scope_depth: 0,
        }
    }
}

impl SmtSolver for Cvc5ProcessSolver {
    fn check_sat(&self, assertions: &[SmtExpr]) -> SmtResult {
        let script = self.build_script(assertions, false);
        let (result, _) = self.run_cvc5(&script);
        result
    }

    fn check_sat_with_model(&self, assertions: &[SmtExpr]) -> (SmtResult, Option<SmtModel>) {
        let script = self.build_script(assertions, true);
        let (result, _output) = self.run_cvc5(&script);
        // CVC5 model parsing is similar to Z3 but we keep it simple for now
        (result, None)
    }

    fn push(&mut self) {
        self.scope_depth = self.scope_depth.saturating_add(1);
    }

    fn pop(&mut self) {
        self.scope_depth = self.scope_depth.saturating_sub(1);
    }

    fn reset(&mut self) {
        self.scope_depth = 0;
    }

    fn solver_name(&self) -> &str {
        "cvc5"
    }

    fn supports_theory(&self, _theory: SmtTheory) -> bool {
        true // CVC5 supports all theories
    }
}

// ── SmtSolverFactory ────────────────────────────────────────────────

/// Factory for creating SMT solver instances.
///
/// Tries Z3 first, then CVC5, then falls back to the builtin solver.
pub struct SmtSolverFactory;

impl SmtSolverFactory {
    /// Create the best available solver on this system.
    ///
    /// Priority: Z3 > CVC5 > Builtin.
    pub fn create_best_available() -> Box<dyn SmtSolver> {
        if let Some(z3) = Self::create_z3() {
            return z3;
        }
        if let Some(cvc5) = Self::create_cvc5() {
            return cvc5;
        }
        Self::create_builtin()
    }

    /// Create a Z3 solver, or `None` if Z3 is not installed.
    pub fn create_z3() -> Option<Box<dyn SmtSolver>> {
        Z3ProcessSolver::new().map(|s| Box::new(s) as Box<dyn SmtSolver>)
    }

    /// Create a CVC5 solver, or `None` if CVC5 is not installed.
    pub fn create_cvc5() -> Option<Box<dyn SmtSolver>> {
        Cvc5ProcessSolver::new().map(|s| Box::new(s) as Box<dyn SmtSolver>)
    }

    /// Create the builtin solver (always available).
    pub fn create_builtin() -> Box<dyn SmtSolver> {
        Box::new(BuiltinSmtSolver::new())
    }

    /// List all available solver backends.
    pub fn available_solvers() -> Vec<std::string::String> {
        let mut solvers = vec!["builtin".to_string()];
        if Z3ProcessSolver::is_available() {
            solvers.push("z3".to_string());
        }
        if Cvc5ProcessSolver::is_available() {
            solvers.push("cvc5".to_string());
        }
        solvers
    }
}

// ── ConstraintTranslator ────────────────────────────────────────────

/// Translates between the existing `Constraint` IR (from `constraints.rs`)
/// and the new `SmtExpr` representation.
pub struct ConstraintTranslator;

impl ConstraintTranslator {
    /// Translate a single constraint into an SMT expression.
    pub fn translate(constraint: &Constraint) -> SmtExpr {
        match constraint {
            Constraint::BoolConst(b) => SmtExpr::BoolConst(*b),

            Constraint::BoolVar(name) => SmtExpr::Var(name.clone(), SmtSort::Bool),

            Constraint::Var(name) => SmtExpr::Var(name.clone(), SmtSort::Int),

            Constraint::IntComparison { var, op, value } => {
                let var_expr = SmtExpr::Var(var.clone(), SmtSort::Int);
                let val_expr = SmtExpr::IntConst(*value);
                Self::make_comparison(&var_expr, *op, &val_expr)
            }

            Constraint::FloatComparison { var, op, value } => {
                let var_expr = SmtExpr::Var(var.clone(), SmtSort::Float);
                let val_expr = SmtExpr::FloatConst(*value);
                Self::make_comparison(&var_expr, *op, &val_expr)
            }

            Constraint::VarComparison { left, op, right } => {
                let left_expr = SmtExpr::Var(left.clone(), SmtSort::Int);
                let right_expr = SmtExpr::Var(right.clone(), SmtSort::Int);
                Self::make_comparison(&left_expr, *op, &right_expr)
            }

            Constraint::And(parts) => {
                let translated: Vec<SmtExpr> = parts.iter().map(Self::translate).collect();
                SmtExpr::And(translated)
            }

            Constraint::Or(parts) => {
                let translated: Vec<SmtExpr> = parts.iter().map(Self::translate).collect();
                SmtExpr::Or(translated)
            }

            Constraint::Not(inner) => SmtExpr::Not(Box::new(Self::translate(inner))),

            Constraint::Arithmetic {
                var,
                arith_op,
                arith_const,
                cmp_op,
                cmp_value,
            } => {
                let var_expr = SmtExpr::Var(var.clone(), SmtSort::Int);
                let const_expr = SmtExpr::IntConst(*arith_const);
                let arith_expr = match arith_op {
                    ArithOp::Add => SmtExpr::Add(Box::new(var_expr), Box::new(const_expr)),
                    ArithOp::Sub => SmtExpr::Sub(Box::new(var_expr), Box::new(const_expr)),
                    ArithOp::Mul => SmtExpr::Mul(Box::new(var_expr), Box::new(const_expr)),
                };
                let cmp_val_expr = SmtExpr::IntConst(*cmp_value);
                Self::make_comparison(&arith_expr, *cmp_op, &cmp_val_expr)
            }

            Constraint::EffectBudget {
                actual_calls,
                max_calls,
                ..
            } => {
                // actual_calls <= max_calls
                SmtExpr::Le(
                    Box::new(SmtExpr::IntConst(*actual_calls as i64)),
                    Box::new(SmtExpr::IntConst(*max_calls as i64)),
                )
            }
        }
    }

    /// Translate a slice of constraints.
    pub fn translate_all(constraints: &[Constraint]) -> Vec<SmtExpr> {
        constraints.iter().map(Self::translate).collect()
    }

    /// Build a comparison SmtExpr from a CmpOp.
    fn make_comparison(left: &SmtExpr, op: CmpOp, right: &SmtExpr) -> SmtExpr {
        match op {
            CmpOp::Eq => SmtExpr::Eq(Box::new(left.clone()), Box::new(right.clone())),
            CmpOp::NotEq => SmtExpr::Ne(Box::new(left.clone()), Box::new(right.clone())),
            CmpOp::Lt => SmtExpr::Lt(Box::new(left.clone()), Box::new(right.clone())),
            CmpOp::LtEq => SmtExpr::Le(Box::new(left.clone()), Box::new(right.clone())),
            CmpOp::Gt => SmtExpr::Gt(Box::new(left.clone()), Box::new(right.clone())),
            CmpOp::GtEq => SmtExpr::Ge(Box::new(left.clone()), Box::new(right.clone())),
        }
    }
}

// ── Generate SMT-LIB2 declarations for testing ─────────────────────

/// Generate a complete SMT-LIB2 script string from assertions.
/// Useful for testing serialization even without Z3/CVC5 installed.
pub fn generate_smtlib2_script(assertions: &[SmtExpr]) -> std::string::String {
    let mut script = std::string::String::new();
    script.push_str("(set-logic ALL)\n");

    // Collect and declare variables
    let mut all_vars = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for assertion in assertions {
        for (name, sort) in assertion.collect_vars() {
            if seen.insert(name.clone()) {
                all_vars.push((name, sort));
            }
        }
    }
    for (name, sort) in &all_vars {
        script.push_str(&format!("(declare-const {} {})\n", name, sort));
    }

    for assertion in assertions {
        script.push_str(&format!("(assert {})\n", assertion.to_smtlib2()));
    }

    script.push_str("(check-sat)\n");
    script
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smtlib2_int_const() {
        assert_eq!(SmtExpr::IntConst(42).to_smtlib2(), "42");
        assert_eq!(SmtExpr::IntConst(-5).to_smtlib2(), "(- 5)");
        assert_eq!(SmtExpr::IntConst(0).to_smtlib2(), "0");
    }

    #[test]
    fn smtlib2_bool_const() {
        assert_eq!(SmtExpr::BoolConst(true).to_smtlib2(), "true");
        assert_eq!(SmtExpr::BoolConst(false).to_smtlib2(), "false");
    }

    #[test]
    fn smtlib2_var() {
        let v = SmtExpr::Var("x".to_string(), SmtSort::Int);
        assert_eq!(v.to_smtlib2(), "x");
    }

    #[test]
    fn smtlib2_add() {
        let expr = SmtExpr::Add(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(1)),
        );
        assert_eq!(expr.to_smtlib2(), "(+ x 1)");
    }

    #[test]
    fn smtlib2_nested_and() {
        let expr = SmtExpr::And(vec![
            SmtExpr::Gt(
                Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
                Box::new(SmtExpr::IntConst(0)),
            ),
            SmtExpr::Lt(
                Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
                Box::new(SmtExpr::IntConst(10)),
            ),
        ]);
        assert_eq!(expr.to_smtlib2(), "(and (> x 0) (< x 10))");
    }

    #[test]
    fn builtin_solver_name() {
        let solver = BuiltinSmtSolver::new();
        assert_eq!(solver.solver_name(), "builtin");
    }

    #[test]
    fn builtin_supports_qflia() {
        let solver = BuiltinSmtSolver::new();
        assert!(solver.supports_theory(SmtTheory::QfLia));
        assert!(!solver.supports_theory(SmtTheory::QfBv));
    }

    #[test]
    fn factory_builtin_always_works() {
        let solver = SmtSolverFactory::create_builtin();
        assert_eq!(solver.solver_name(), "builtin");
    }

    #[test]
    fn factory_available_includes_builtin() {
        let solvers = SmtSolverFactory::available_solvers();
        assert!(solvers.contains(&"builtin".to_string()));
    }
}
