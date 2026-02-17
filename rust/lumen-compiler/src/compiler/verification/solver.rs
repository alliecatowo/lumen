//! SMT solver abstraction and toy implementation.
//!
//! Defines a `Solver` trait that future backends (Z3, CVC5) will implement.
//! Ships with a `ToyConstraintSolver` that handles the subset of constraints
//! that can be decided without a full SMT engine: simple numeric interval
//! checks, boolean constant propagation, and conjunction/disjunction of
//! such constraints.

use super::constraints::{CmpOp, Constraint};

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

    /// Check if the accumulated bounds have any satisfying integer.
    fn is_satisfiable(&self) -> SatResult {
        // Effective lower bound (inclusive)
        let lo = match (self.lower, self.lower_eq) {
            (Some(gt), Some(ge)) => Some((gt + 1).max(ge)),
            (Some(gt), None) => Some(gt + 1),
            (None, Some(ge)) => Some(ge),
            (None, None) => None,
        };
        // Effective upper bound (inclusive)
        let hi = match (self.upper, self.upper_eq) {
            (Some(lt), Some(le)) => Some((lt - 1).min(le)),
            (Some(lt), None) => Some(lt - 1),
            (None, Some(le)) => Some(le),
            (None, None) => None,
        };

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

        Constraint::IntComparison { .. } => {
            // A single comparison on one variable is always satisfiable
            // (the variable can take any integer value).
            SatResult::Sat
        }

        Constraint::FloatComparison { .. } => {
            // Similar — single comparison is always sat.
            SatResult::Sat
        }

        Constraint::Not(inner) => match evaluate_constraint(inner) {
            SatResult::Sat => {
                // not(sat) might still be sat if the inner is not a tautology.
                // We can't decide in general, so Unknown.
                // Exception: not(true) = false, not(false) = true.
                match inner.as_ref() {
                    Constraint::BoolConst(true) => SatResult::Unsat,
                    Constraint::BoolConst(false) => SatResult::Sat,
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

/// Evaluate a conjunction by collecting integer bounds per variable.
fn evaluate_conjunction(parts: &[Constraint]) -> SatResult {
    use std::collections::HashMap;

    let mut bounds: HashMap<String, IntBounds> = HashMap::new();
    let mut has_unknown = false;

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

            // For nested And, flatten into the same bounds set.
            Constraint::And(inner) => match evaluate_conjunction(inner) {
                SatResult::Unsat => return SatResult::Unsat,
                SatResult::Unknown => has_unknown = true,
                SatResult::Sat => {}
            },

            // A disjunction inside a conjunction — evaluate it separately.
            Constraint::Or(_) => match evaluate_constraint(part) {
                SatResult::Unsat => return SatResult::Unsat,
                SatResult::Unknown => has_unknown = true,
                SatResult::Sat => {}
            },

            // Not / float / bool var — we can't decide.
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
}
