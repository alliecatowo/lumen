//! SMT solver abstraction and toy implementation.
//!
//! Defines a `Solver` trait that future backends (Z3, CVC5) will implement.
//! Ships with a `ToyConstraintSolver` that handles the subset of constraints
//! that can be decided without a full SMT engine: simple numeric interval
//! checks, boolean constant propagation, conjunction/disjunction of
//! such constraints, variable-to-variable comparisons via transitivity,
//! and arithmetic constraints.

use super::constraints::{ArithOp, CmpOp, Constraint};

// ── Solver trait ────────────────────────────────────────────────────

/// Result of a satisfiability check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SatResult {
    /// The asserted constraints have at least one satisfying assignment.
    Sat,
    /// The asserted constraints are unsatisfiable.
    Unsat,
    /// The solver could not determine satisfiability.
    Unknown,
}

/// Abstract solver interface.
///
/// Designed after the SMT-LIB push/pop model.  Implementations should
/// maintain an assertion stack that `push`/`pop` save and restore.
pub trait Solver {
    /// Assert a constraint in the current scope.
    fn assert_constraint(&mut self, constraint: &Constraint);
    /// Check satisfiability of all asserted constraints.
    fn check_sat(&self) -> SatResult;
    /// Return a human-readable model (variable assignments) if Sat.
    fn get_model(&self) -> Option<String>;
    /// Push a new assertion scope.
    fn push(&mut self);
    /// Pop the most recent assertion scope.
    fn pop(&mut self);
    /// Reset the solver to a clean state.
    fn reset(&mut self);

    /// Check whether `premise => conclusion` is valid.
    ///
    /// Strategy: assert `premise AND NOT(conclusion)` and check for
    /// unsatisfiability.  If Unsat, the implication holds universally.
    /// If Sat, there exists a counterexample where premise holds but
    /// conclusion does not.
    fn check_implication(&mut self, premise: &Constraint, conclusion: &Constraint) -> SatResult {
        self.push();
        self.assert_constraint(premise);
        self.assert_constraint(&Constraint::Not(Box::new(conclusion.clone())));
        let result = self.check_sat();
        self.pop();
        result
    }
}

// ── ToyConstraintSolver ─────────────────────────────────────────────

/// A lightweight solver that handles simple interval constraints without
/// requiring an external SMT backend.
///
/// Capabilities:
/// - Integer comparisons on a single variable (tracks lower/upper bounds)
/// - Boolean constants
/// - Conjunction (`And`) of supported constraints
/// - Disjunction (`Or`) of supported constraints (any-branch-sat)
/// - Negation of supported constraints
/// - Variable-to-variable comparisons (via transitivity reasoning)
/// - Arithmetic constraints (`var + const <op> value`)
/// - Effect budget constraints
///
/// Returns `SatResult::Unknown` for anything outside its capabilities.
#[derive(Debug, Default)]
pub struct ToyConstraintSolver {
    /// Stack of assertion-set snapshots (lengths into `constraints`).
    scope_stack: Vec<usize>,
    /// All asserted constraints.
    constraints: Vec<Constraint>,
}

impl ToyConstraintSolver {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Solver for ToyConstraintSolver {
    fn assert_constraint(&mut self, constraint: &Constraint) {
        self.constraints.push(constraint.clone());
    }

    fn check_sat(&self) -> SatResult {
        // Treat the full constraint set as a conjunction.
        if self.constraints.is_empty() {
            return SatResult::Sat; // vacuously satisfiable
        }
        let conj = if self.constraints.len() == 1 {
            self.constraints[0].clone()
        } else {
            Constraint::And(self.constraints.clone())
        };
        evaluate_constraint(&conj)
    }

    fn get_model(&self) -> Option<String> {
        // The toy solver doesn't produce witness assignments.
        None
    }

    fn push(&mut self) {
        self.scope_stack.push(self.constraints.len());
    }

    fn pop(&mut self) {
        if let Some(len) = self.scope_stack.pop() {
            self.constraints.truncate(len);
        }
    }

    fn reset(&mut self) {
        self.constraints.clear();
        self.scope_stack.clear();
    }
}

// ── Evaluation engine ───────────────────────────────────────────────

/// Negate a comparison operator.
fn negate_cmp_op(op: CmpOp) -> CmpOp {
    match op {
        CmpOp::Gt => CmpOp::LtEq,
        CmpOp::GtEq => CmpOp::Lt,
        CmpOp::Lt => CmpOp::GtEq,
        CmpOp::LtEq => CmpOp::Gt,
        CmpOp::Eq => CmpOp::NotEq,
        CmpOp::NotEq => CmpOp::Eq,
    }
}

/// Tracks integer bounds per variable for interval reasoning.
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

    fn apply(&mut self, op: CmpOp, value: i64) {
        match op {
            CmpOp::Gt => {
                self.lower = Some(match self.lower {
                    Some(prev) => prev.max(value),
                    None => value,
                });
            }
            CmpOp::GtEq => {
                self.lower_eq = Some(match self.lower_eq {
                    Some(prev) => prev.max(value),
                    None => value,
                });
            }
            CmpOp::Lt => {
                self.upper = Some(match self.upper {
                    Some(prev) => prev.min(value),
                    None => value,
                });
            }
            CmpOp::LtEq => {
                self.upper_eq = Some(match self.upper_eq {
                    Some(prev) => prev.min(value),
                    None => value,
                });
            }
            CmpOp::Eq => {
                self.eq = Some(value);
            }
            CmpOp::NotEq => {
                self.neq.push(value);
            }
        }
    }

    /// Return the effective inclusive lower bound, if any.
    fn effective_lower(&self) -> Option<i64> {
        match (self.lower, self.lower_eq) {
            (Some(gt), Some(ge)) => Some((gt + 1).max(ge)),
            (Some(gt), None) => Some(gt + 1),
            (None, Some(ge)) => Some(ge),
            (None, None) => None,
        }
    }

    /// Return the effective inclusive upper bound, if any.
    fn effective_upper(&self) -> Option<i64> {
        match (self.upper, self.upper_eq) {
            (Some(lt), Some(le)) => Some((lt - 1).min(le)),
            (Some(lt), None) => Some(lt - 1),
            (None, Some(le)) => Some(le),
            (None, None) => None,
        }
    }

    /// Check if the accumulated bounds have any satisfying integer.
    fn is_satisfiable(&self) -> SatResult {
        let lo = self.effective_lower();
        let hi = self.effective_upper();

        // If there's an equality constraint, the value must be in bounds
        if let Some(eq_val) = self.eq {
            if let Some(lo) = lo {
                if eq_val < lo {
                    return SatResult::Unsat;
                }
            }
            if let Some(hi) = hi {
                if eq_val > hi {
                    return SatResult::Unsat;
                }
            }
            if self.neq.contains(&eq_val) {
                return SatResult::Unsat;
            }
            return SatResult::Sat;
        }

        // Check if range is non-empty
        match (lo, hi) {
            (Some(lo), Some(hi)) => {
                if lo > hi {
                    return SatResult::Unsat;
                }
                // Check if all integers in [lo, hi] are forbidden
                let range_size = hi - lo + 1;
                if range_size <= self.neq.len() as i64 {
                    // Small range — check exhaustively
                    let all_forbidden = (lo..=hi).all(|v| self.neq.contains(&v));
                    if all_forbidden {
                        return SatResult::Unsat;
                    }
                }
                SatResult::Sat
            }
            _ => SatResult::Sat, // unbounded on at least one side
        }
    }
}

/// Evaluate a single constraint.
fn evaluate_constraint(c: &Constraint) -> SatResult {
    match c {
        Constraint::BoolConst(true) => SatResult::Sat,
        Constraint::BoolConst(false) => SatResult::Unsat,
        Constraint::BoolVar(_) => SatResult::Unknown,
        Constraint::Var(_) => SatResult::Unknown,

        Constraint::IntComparison { .. } => {
            // A single comparison on one variable is always satisfiable
            // (the variable can take any integer value).
            SatResult::Sat
        }

        Constraint::FloatComparison { .. } => {
            // Similar — single comparison is always sat.
            SatResult::Sat
        }

        Constraint::VarComparison { left, op, right } => {
            if left == right {
                // x <op> x — decidable
                match op {
                    CmpOp::Eq | CmpOp::LtEq | CmpOp::GtEq => SatResult::Sat,
                    CmpOp::NotEq | CmpOp::Lt | CmpOp::Gt => SatResult::Unsat,
                }
            } else {
                // Two different variables — satisfiable (pick appropriate values)
                SatResult::Sat
            }
        }

        Constraint::Arithmetic {
            var: _,
            arith_op,
            arith_const,
            cmp_op,
            cmp_value,
        } => {
            // A single arithmetic constraint is satisfiable if there exists
            // an integer x such that (x arith_op arith_const) cmp_op cmp_value.
            // For add/sub, this is always satisfiable (integers are unbounded).
            // For mul by 0, check if 0 cmp_op cmp_value.
            match arith_op {
                ArithOp::Mul if *arith_const == 0 => {
                    let result = 0_i64;
                    let holds = match cmp_op {
                        CmpOp::Gt => result > *cmp_value,
                        CmpOp::GtEq => result >= *cmp_value,
                        CmpOp::Lt => result < *cmp_value,
                        CmpOp::LtEq => result <= *cmp_value,
                        CmpOp::Eq => result == *cmp_value,
                        CmpOp::NotEq => result != *cmp_value,
                    };
                    if holds {
                        SatResult::Sat
                    } else {
                        SatResult::Unsat
                    }
                }
                _ => SatResult::Sat,
            }
        }

        Constraint::EffectBudget {
            max_calls,
            actual_calls,
            ..
        } => {
            if *actual_calls <= *max_calls {
                SatResult::Sat
            } else {
                SatResult::Unsat
            }
        }

        Constraint::Not(inner) => match evaluate_constraint(inner) {
            SatResult::Sat => {
                // not(sat) might still be sat if the inner is not a tautology.
                // Exception: not(true) = false, not(false) = true.
                match inner.as_ref() {
                    Constraint::BoolConst(true) => SatResult::Unsat,
                    Constraint::BoolConst(false) => SatResult::Sat,
                    // Not of an effect budget: invert the check
                    Constraint::EffectBudget {
                        max_calls,
                        actual_calls,
                        ..
                    } => {
                        if *actual_calls > *max_calls {
                            SatResult::Sat
                        } else {
                            SatResult::Unsat
                        }
                    }
                    // Not of a VarComparison where left == right
                    Constraint::VarComparison { left, op, right } if left == right => match op {
                        CmpOp::Eq | CmpOp::LtEq | CmpOp::GtEq => SatResult::Unsat,
                        CmpOp::NotEq | CmpOp::Lt | CmpOp::Gt => SatResult::Sat,
                    },
                    _ => SatResult::Unknown,
                }
            }
            SatResult::Unsat => SatResult::Sat,
            SatResult::Unknown => SatResult::Unknown,
        },

        Constraint::And(parts) => evaluate_conjunction(parts),

        Constraint::Or(parts) => {
            // If any branch is Sat, the disjunction is Sat.
            // If all are Unsat, it's Unsat.
            // Otherwise Unknown.
            let mut any_sat = false;
            let mut all_unsat = true;
            for part in parts {
                match evaluate_constraint(part) {
                    SatResult::Sat => {
                        any_sat = true;
                        all_unsat = false;
                    }
                    SatResult::Unknown => {
                        all_unsat = false;
                    }
                    SatResult::Unsat => {}
                }
            }
            if any_sat {
                SatResult::Sat
            } else if all_unsat {
                SatResult::Unsat
            } else {
                SatResult::Unknown
            }
        }
    }
}

/// Evaluate a conjunction by collecting integer bounds per variable,
/// with support for arithmetic constraints and transitivity.
fn evaluate_conjunction(parts: &[Constraint]) -> SatResult {
    use std::collections::HashMap;

    let mut bounds: HashMap<String, IntBounds> = HashMap::new();
    let mut has_unknown = false;
    // Collect variable ordering facts for transitivity reasoning.
    let mut var_relations: Vec<(String, CmpOp, String)> = Vec::new();

    for part in parts {
        match part {
            Constraint::BoolConst(false) => return SatResult::Unsat,
            Constraint::BoolConst(true) => {} // no-op

            Constraint::IntComparison { var, op, value } => {
                bounds
                    .entry(var.clone())
                    .or_insert_with(IntBounds::new)
                    .apply(*op, *value);
            }

            Constraint::Arithmetic {
                var,
                arith_op,
                arith_const,
                cmp_op,
                cmp_value,
            } => {
                // Reduce to an IntComparison by solving for var.
                // (var + c) cmp v  ↔  var cmp (v - c)
                // (var - c) cmp v  ↔  var cmp (v + c)
                // (var * c) cmp v  → harder, handle simple cases
                match arith_op {
                    ArithOp::Add => {
                        bounds
                            .entry(var.clone())
                            .or_insert_with(IntBounds::new)
                            .apply(*cmp_op, *cmp_value - *arith_const);
                    }
                    ArithOp::Sub => {
                        bounds
                            .entry(var.clone())
                            .or_insert_with(IntBounds::new)
                            .apply(*cmp_op, *cmp_value + *arith_const);
                    }
                    ArithOp::Mul => {
                        if *arith_const > 0 {
                            if *cmp_value % *arith_const == 0 {
                                bounds
                                    .entry(var.clone())
                                    .or_insert_with(IntBounds::new)
                                    .apply(*cmp_op, *cmp_value / *arith_const);
                            } else {
                                has_unknown = true;
                            }
                        } else if *arith_const < 0 {
                            if *cmp_value % arith_const.abs() == 0 {
                                let flipped = match cmp_op {
                                    CmpOp::Gt => CmpOp::Lt,
                                    CmpOp::GtEq => CmpOp::LtEq,
                                    CmpOp::Lt => CmpOp::Gt,
                                    CmpOp::LtEq => CmpOp::GtEq,
                                    CmpOp::Eq => CmpOp::Eq,
                                    CmpOp::NotEq => CmpOp::NotEq,
                                };
                                bounds
                                    .entry(var.clone())
                                    .or_insert_with(IntBounds::new)
                                    .apply(flipped, *cmp_value / *arith_const);
                            } else {
                                has_unknown = true;
                            }
                        } else {
                            // Multiply by 0: 0 <cmp_op> cmp_value
                            let holds = match cmp_op {
                                CmpOp::Gt => 0 > *cmp_value,
                                CmpOp::GtEq => 0 >= *cmp_value,
                                CmpOp::Lt => 0 < *cmp_value,
                                CmpOp::LtEq => 0 <= *cmp_value,
                                CmpOp::Eq => 0 == *cmp_value,
                                CmpOp::NotEq => 0 != *cmp_value,
                            };
                            if !holds {
                                return SatResult::Unsat;
                            }
                        }
                    }
                }
            }

            Constraint::VarComparison { left, op, right } => {
                if left == right {
                    // x <op> x
                    match op {
                        CmpOp::Lt | CmpOp::Gt | CmpOp::NotEq => return SatResult::Unsat,
                        _ => {} // tautology
                    }
                } else {
                    var_relations.push((left.clone(), *op, right.clone()));
                }
            }

            Constraint::EffectBudget {
                max_calls,
                actual_calls,
                ..
            } => {
                if *actual_calls > *max_calls {
                    return SatResult::Unsat;
                }
            }

            // For nested And, flatten the contents into this conjunction.
            Constraint::And(inner) => {
                // Recursively process inner parts as if they were at this level.
                for inner_part in inner {
                    // We create a single-element slice to process through the
                    // same match logic. But it's simpler to just flatten:
                    match inner_part {
                        Constraint::BoolConst(false) => return SatResult::Unsat,
                        Constraint::BoolConst(true) => {}
                        Constraint::IntComparison { var, op, value } => {
                            bounds
                                .entry(var.clone())
                                .or_insert_with(IntBounds::new)
                                .apply(*op, *value);
                        }
                        Constraint::VarComparison { left, op, right } => {
                            if left == right {
                                match op {
                                    CmpOp::Lt | CmpOp::Gt | CmpOp::NotEq => {
                                        return SatResult::Unsat
                                    }
                                    _ => {}
                                }
                            } else {
                                var_relations.push((left.clone(), *op, right.clone()));
                            }
                        }
                        Constraint::EffectBudget {
                            max_calls,
                            actual_calls,
                            ..
                        } => {
                            if *actual_calls > *max_calls {
                                return SatResult::Unsat;
                            }
                        }
                        // For complex inner parts, recurse.
                        other => match evaluate_constraint(other) {
                            SatResult::Unsat => return SatResult::Unsat,
                            SatResult::Unknown => has_unknown = true,
                            SatResult::Sat => {}
                        },
                    }
                }
            }

            // A disjunction inside a conjunction — evaluate it separately.
            Constraint::Or(_) => match evaluate_constraint(part) {
                SatResult::Unsat => return SatResult::Unsat,
                SatResult::Unknown => has_unknown = true,
                SatResult::Sat => {}
            },

            // Negation — handle decidable cases, mark unknown otherwise.
            Constraint::Not(inner) => match inner.as_ref() {
                Constraint::BoolConst(true) => return SatResult::Unsat,
                Constraint::BoolConst(false) => {} // not(false) = true, no-op
                Constraint::IntComparison { var, op, value } => {
                    // not(x > 0) → x <= 0
                    let negated_op = negate_cmp_op(*op);
                    bounds
                        .entry(var.clone())
                        .or_insert_with(IntBounds::new)
                        .apply(negated_op, *value);
                }
                Constraint::VarComparison { left, op, right } => {
                    if left == right {
                        let negated = negate_cmp_op(*op);
                        match negated {
                            CmpOp::Lt | CmpOp::Gt | CmpOp::NotEq => return SatResult::Unsat,
                            _ => {} // tautology
                        }
                    } else {
                        let negated = negate_cmp_op(*op);
                        var_relations.push((left.clone(), negated, right.clone()));
                    }
                }
                Constraint::Arithmetic {
                    var,
                    arith_op,
                    arith_const,
                    cmp_op,
                    cmp_value,
                } => {
                    // Negate the comparison part, then reduce like normal Arithmetic.
                    let negated_cmp = negate_cmp_op(*cmp_op);
                    let negated = Constraint::Arithmetic {
                        var: var.clone(),
                        arith_op: *arith_op,
                        arith_const: *arith_const,
                        cmp_op: negated_cmp,
                        cmp_value: *cmp_value,
                    };
                    // Re-evaluate as if it were a top-level conjunction member.
                    // We recurse through evaluate_conjunction with a single element.
                    match evaluate_conjunction(&[negated]) {
                        SatResult::Unsat => return SatResult::Unsat,
                        SatResult::Unknown => has_unknown = true,
                        SatResult::Sat => {}
                    }
                }
                Constraint::EffectBudget {
                    max_calls,
                    actual_calls,
                    ..
                } => {
                    // not(actual <= max) → actual > max
                    if *actual_calls <= *max_calls {
                        return SatResult::Unsat;
                    }
                }
                _ => has_unknown = true,
            },

            // Float / bool var / Var — we can't decide.
            _ => has_unknown = true,
        }
    }

    // Check per-variable satisfiability
    for b in bounds.values() {
        match b.is_satisfiable() {
            SatResult::Unsat => return SatResult::Unsat,
            SatResult::Unknown => has_unknown = true,
            SatResult::Sat => {}
        }
    }

    // Transitivity reasoning on variable relations.
    // If we know x > y (or x >= y) and have bounds on y, propagate to x.
    for (left, op, right) in &var_relations {
        if let (Some(left_bounds), Some(right_bounds)) = (bounds.get(left), bounds.get(right)) {
            match op {
                CmpOp::Gt | CmpOp::GtEq => {
                    // left > right: left's upper must accommodate right's lower
                    if let (Some(left_hi), Some(right_lo)) = (
                        left_bounds.effective_upper(),
                        right_bounds.effective_lower(),
                    ) {
                        let needed = match op {
                            CmpOp::Gt => right_lo + 1,
                            _ => right_lo,
                        };
                        if left_hi < needed {
                            return SatResult::Unsat;
                        }
                    }
                }
                CmpOp::Lt | CmpOp::LtEq => {
                    // left < right: left's lower must be below right's upper
                    if let (Some(left_lo), Some(right_hi)) = (
                        left_bounds.effective_lower(),
                        right_bounds.effective_upper(),
                    ) {
                        let needed = match op {
                            CmpOp::Lt => right_hi - 1,
                            _ => right_hi,
                        };
                        if left_lo > needed {
                            return SatResult::Unsat;
                        }
                    }
                }
                CmpOp::Eq => {
                    // Ranges must overlap.
                    let l_lo = left_bounds.effective_lower();
                    let l_hi = left_bounds.effective_upper();
                    let r_lo = right_bounds.effective_lower();
                    let r_hi = right_bounds.effective_upper();
                    let combined_lo = match (l_lo, r_lo) {
                        (Some(a), Some(b)) => Some(a.max(b)),
                        (Some(a), None) | (None, Some(a)) => Some(a),
                        (None, None) => None,
                    };
                    let combined_hi = match (l_hi, r_hi) {
                        (Some(a), Some(b)) => Some(a.min(b)),
                        (Some(a), None) | (None, Some(a)) => Some(a),
                        (None, None) => None,
                    };
                    if let (Some(lo), Some(hi)) = (combined_lo, combined_hi) {
                        if lo > hi {
                            return SatResult::Unsat;
                        }
                    }
                }
                CmpOp::NotEq => {
                    // Always satisfiable unless both are forced to the same value.
                    if let (Some(l_eq), Some(r_eq)) = (left_bounds.eq, right_bounds.eq) {
                        if l_eq == r_eq {
                            return SatResult::Unsat;
                        }
                    }
                }
            }
        }
    }

    if has_unknown {
        SatResult::Unknown
    } else {
        SatResult::Sat
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn int_cmp(var: &str, op: CmpOp, value: i64) -> Constraint {
        Constraint::IntComparison {
            var: var.to_string(),
            op,
            value,
        }
    }

    #[test]
    fn empty_solver_is_sat() {
        let solver = ToyConstraintSolver::new();
        assert_eq!(solver.check_sat(), SatResult::Sat);
    }

    #[test]
    fn single_constraint_sat() {
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&int_cmp("x", CmpOp::Gt, 0));
        assert_eq!(solver.check_sat(), SatResult::Sat);
    }

    #[test]
    fn satisfiable_range() {
        // x > 0 and x < 10  →  Sat (e.g. x = 5)
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&int_cmp("x", CmpOp::Gt, 0));
        solver.assert_constraint(&int_cmp("x", CmpOp::Lt, 10));
        assert_eq!(solver.check_sat(), SatResult::Sat);
    }

    #[test]
    fn unsatisfiable_range() {
        // x > 10 and x < 5  →  Unsat
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&int_cmp("x", CmpOp::Gt, 10));
        solver.assert_constraint(&int_cmp("x", CmpOp::Lt, 5));
        assert_eq!(solver.check_sat(), SatResult::Unsat);
    }

    #[test]
    fn boundary_satisfiable() {
        // x >= 5 and x <= 5  →  Sat (x = 5)
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&int_cmp("x", CmpOp::GtEq, 5));
        solver.assert_constraint(&int_cmp("x", CmpOp::LtEq, 5));
        assert_eq!(solver.check_sat(), SatResult::Sat);
    }

    #[test]
    fn boundary_unsatisfiable() {
        // x > 5 and x < 6  →  Unsat (no integer in (5, 6))
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&int_cmp("x", CmpOp::Gt, 5));
        solver.assert_constraint(&int_cmp("x", CmpOp::Lt, 6));
        assert_eq!(solver.check_sat(), SatResult::Unsat);
    }

    #[test]
    fn equality_in_range() {
        // x == 5 and x > 0 and x < 10  →  Sat
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&int_cmp("x", CmpOp::Eq, 5));
        solver.assert_constraint(&int_cmp("x", CmpOp::Gt, 0));
        solver.assert_constraint(&int_cmp("x", CmpOp::Lt, 10));
        assert_eq!(solver.check_sat(), SatResult::Sat);
    }

    #[test]
    fn equality_out_of_range() {
        // x == 15 and x < 10  →  Unsat
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&int_cmp("x", CmpOp::Eq, 15));
        solver.assert_constraint(&int_cmp("x", CmpOp::Lt, 10));
        assert_eq!(solver.check_sat(), SatResult::Unsat);
    }

    #[test]
    fn neq_eliminates_only_option() {
        // x >= 5 and x <= 5 and x != 5  →  Unsat
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&int_cmp("x", CmpOp::GtEq, 5));
        solver.assert_constraint(&int_cmp("x", CmpOp::LtEq, 5));
        solver.assert_constraint(&int_cmp("x", CmpOp::NotEq, 5));
        assert_eq!(solver.check_sat(), SatResult::Unsat);
    }

    #[test]
    fn bool_false_is_unsat() {
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&Constraint::BoolConst(false));
        assert_eq!(solver.check_sat(), SatResult::Unsat);
    }

    #[test]
    fn bool_true_is_sat() {
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&Constraint::BoolConst(true));
        assert_eq!(solver.check_sat(), SatResult::Sat);
    }

    #[test]
    fn push_pop_restores_state() {
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&int_cmp("x", CmpOp::Gt, 0));
        solver.push();
        solver.assert_constraint(&int_cmp("x", CmpOp::Lt, 0)); // makes it unsat
        assert_eq!(solver.check_sat(), SatResult::Unsat);
        solver.pop();
        assert_eq!(solver.check_sat(), SatResult::Sat);
    }

    #[test]
    fn reset_clears_all() {
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&Constraint::BoolConst(false));
        assert_eq!(solver.check_sat(), SatResult::Unsat);
        solver.reset();
        assert_eq!(solver.check_sat(), SatResult::Sat);
    }

    #[test]
    fn multiple_variables() {
        // x > 0 and x < 10 and y > 100 and y < 50  →  Unsat (y is impossible)
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&int_cmp("x", CmpOp::Gt, 0));
        solver.assert_constraint(&int_cmp("x", CmpOp::Lt, 10));
        solver.assert_constraint(&int_cmp("y", CmpOp::Gt, 100));
        solver.assert_constraint(&int_cmp("y", CmpOp::Lt, 50));
        assert_eq!(solver.check_sat(), SatResult::Unsat);
    }

    #[test]
    fn or_one_branch_sat() {
        // (x > 100) or (x < 5)  →  Sat
        let c = Constraint::Or(vec![
            int_cmp("x", CmpOp::Gt, 100),
            int_cmp("x", CmpOp::Lt, 5),
        ]);
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&c);
        assert_eq!(solver.check_sat(), SatResult::Sat);
    }

    #[test]
    fn or_all_unsat() {
        // (false) or (false)  →  Unsat
        let c = Constraint::Or(vec![
            Constraint::BoolConst(false),
            Constraint::BoolConst(false),
        ]);
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&c);
        assert_eq!(solver.check_sat(), SatResult::Unsat);
    }

    #[test]
    fn not_false_is_sat() {
        let c = Constraint::Not(Box::new(Constraint::BoolConst(false)));
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&c);
        assert_eq!(solver.check_sat(), SatResult::Sat);
    }

    #[test]
    fn not_true_is_unsat() {
        let c = Constraint::Not(Box::new(Constraint::BoolConst(true)));
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&c);
        assert_eq!(solver.check_sat(), SatResult::Unsat);
    }

    #[test]
    fn bool_var_is_unknown() {
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&Constraint::BoolVar("flag".to_string()));
        assert_eq!(solver.check_sat(), SatResult::Unknown);
    }

    // ── Implication tests ───────────────────────────────────────

    #[test]
    fn implication_x_gt5_implies_x_gt0() {
        // x > 5 → x > 0 is valid.
        // Assert x > 5 AND NOT(x > 0).
        // x > 5 AND x <= 0 → Unsat → implication holds.
        let mut solver = ToyConstraintSolver::new();
        let premise = int_cmp("x", CmpOp::Gt, 5);
        let conclusion = int_cmp("x", CmpOp::Gt, 0);
        let result = solver.check_implication(&premise, &conclusion);
        assert_eq!(result, SatResult::Unsat); // Unsat means implication is valid
    }

    #[test]
    fn implication_x_gt0_does_not_imply_x_gt5() {
        // x > 0 → x > 5 is NOT valid (e.g. x = 3).
        let mut solver = ToyConstraintSolver::new();
        let premise = int_cmp("x", CmpOp::Gt, 0);
        let conclusion = int_cmp("x", CmpOp::Gt, 5);
        let result = solver.check_implication(&premise, &conclusion);
        assert_eq!(result, SatResult::Sat); // Sat means counterexample exists
    }

    #[test]
    fn implication_conjunction_implies_weaker() {
        // (x > 0 AND x < 10) → x >= 0 is valid.
        let mut solver = ToyConstraintSolver::new();
        let premise = Constraint::And(vec![
            int_cmp("x", CmpOp::Gt, 0),
            int_cmp("x", CmpOp::Lt, 10),
        ]);
        let conclusion = int_cmp("x", CmpOp::GtEq, 0);
        let result = solver.check_implication(&premise, &conclusion);
        assert_eq!(result, SatResult::Unsat); // valid
    }

    // ── Variable comparison tests ───────────────────────────────

    #[test]
    fn var_comparison_same_var_eq() {
        // x == x is always true (tautology)
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&Constraint::VarComparison {
            left: "x".to_string(),
            op: CmpOp::Eq,
            right: "x".to_string(),
        });
        assert_eq!(solver.check_sat(), SatResult::Sat);
    }

    #[test]
    fn var_comparison_same_var_lt() {
        // x < x is always false
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&Constraint::VarComparison {
            left: "x".to_string(),
            op: CmpOp::Lt,
            right: "x".to_string(),
        });
        assert_eq!(solver.check_sat(), SatResult::Unsat);
    }

    #[test]
    fn var_comparison_transitivity_contradiction() {
        // x > y AND x < 5 AND y > 10 → Unsat
        // Because x > y > 10 implies x > 10, but x < 5 contradicts.
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&Constraint::VarComparison {
            left: "x".to_string(),
            op: CmpOp::Gt,
            right: "y".to_string(),
        });
        solver.assert_constraint(&int_cmp("x", CmpOp::Lt, 5));
        solver.assert_constraint(&int_cmp("y", CmpOp::Gt, 10));
        assert_eq!(solver.check_sat(), SatResult::Unsat);
    }

    // ── Arithmetic constraint tests ─────────────────────────────

    #[test]
    fn arithmetic_add_satisfiable() {
        // (x + 1) > 0 → satisfiable (e.g. x = 0)
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&Constraint::Arithmetic {
            var: "x".to_string(),
            arith_op: ArithOp::Add,
            arith_const: 1,
            cmp_op: CmpOp::Gt,
            cmp_value: 0,
        });
        assert_eq!(solver.check_sat(), SatResult::Sat);
    }

    #[test]
    fn arithmetic_add_with_bounds_unsat() {
        // (x + 1) > 5 AND x < 3 → x > 4 AND x < 3 → Unsat
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&Constraint::Arithmetic {
            var: "x".to_string(),
            arith_op: ArithOp::Add,
            arith_const: 1,
            cmp_op: CmpOp::Gt,
            cmp_value: 5,
        });
        solver.assert_constraint(&int_cmp("x", CmpOp::Lt, 3));
        assert_eq!(solver.check_sat(), SatResult::Unsat);
    }

    // ── Effect budget tests ─────────────────────────────────────

    #[test]
    fn effect_budget_within_limit() {
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&Constraint::EffectBudget {
            effect_name: "network".to_string(),
            max_calls: 3,
            actual_calls: 2,
        });
        assert_eq!(solver.check_sat(), SatResult::Sat);
    }

    #[test]
    fn effect_budget_exceeded() {
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&Constraint::EffectBudget {
            effect_name: "network".to_string(),
            max_calls: 3,
            actual_calls: 4,
        });
        assert_eq!(solver.check_sat(), SatResult::Unsat);
    }

    #[test]
    fn effect_budget_exact_limit() {
        let mut solver = ToyConstraintSolver::new();
        solver.assert_constraint(&Constraint::EffectBudget {
            effect_name: "network".to_string(),
            max_calls: 3,
            actual_calls: 3,
        });
        assert_eq!(solver.check_sat(), SatResult::Sat);
    }
}
