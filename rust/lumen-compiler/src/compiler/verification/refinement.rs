//! Path-sensitive refinement context for verification.
//!
//! Tracks known facts (constraints) per variable through different execution
//! paths. Supports refining from branch conditions, merging at join points,
//! and checking implication of constraints against accumulated facts.

use std::collections::HashMap;

use super::constraints::{CmpOp, Constraint};
use super::solver::{SatResult, Solver, ToyConstraintSolver};

// ── RefinementContext ───────────────────────────────────────────────

/// A set of known facts about variables in scope, used for path-sensitive
/// verification. Each variable can have multiple constraints (conjunction).
#[derive(Debug, Clone)]
pub struct RefinementContext {
    /// Known facts per variable name. All facts for a variable are implicitly
    /// conjoined — they must all hold simultaneously.
    facts: HashMap<String, Vec<Constraint>>,
}

impl RefinementContext {
    /// Create an empty refinement context.
    pub fn new() -> Self {
        Self {
            facts: HashMap::new(),
        }
    }

    /// Add a fact about a variable.
    pub fn add_fact(&mut self, var: &str, constraint: Constraint) {
        self.facts
            .entry(var.to_string())
            .or_default()
            .push(constraint);
    }

    /// Refine the context from a branch condition expression.
    ///
    /// Given a constraint representing a branch condition (e.g. `x > 0`),
    /// extract the variable and add the constraint as a known fact.
    /// Returns `true` if a fact was extracted, `false` if the condition
    /// could not be decomposed into per-variable facts.
    pub fn refine_from_condition(&mut self, condition: &Constraint) -> bool {
        match condition {
            Constraint::IntComparison { var, .. } => {
                self.add_fact(var, condition.clone());
                true
            }
            Constraint::FloatComparison { var, .. } => {
                self.add_fact(var, condition.clone());
                true
            }
            Constraint::VarComparison { left, right, .. } => {
                // Add the constraint as a fact for both variables involved.
                self.add_fact(left, condition.clone());
                self.add_fact(right, condition.clone());
                true
            }
            Constraint::Arithmetic { var, .. } => {
                self.add_fact(var, condition.clone());
                true
            }
            Constraint::BoolVar(name) => {
                self.add_fact(name, Constraint::BoolConst(true));
                true
            }
            Constraint::BoolConst(_) => {
                // No variable to refine on.
                false
            }
            Constraint::And(parts) => {
                let mut any = false;
                for part in parts {
                    if self.refine_from_condition(part) {
                        any = true;
                    }
                }
                any
            }
            Constraint::Not(inner) => {
                // Refine with the negated version of the inner constraint.
                match inner.as_ref() {
                    Constraint::IntComparison { var, op, value } => {
                        let negated_op = negate_cmp(*op);
                        self.add_fact(
                            var,
                            Constraint::IntComparison {
                                var: var.clone(),
                                op: negated_op,
                                value: *value,
                            },
                        );
                        true
                    }
                    Constraint::BoolVar(name) => {
                        self.add_fact(name, Constraint::BoolConst(false));
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// Merge two refinement contexts at a join point (e.g. after if/else).
    ///
    /// The result is conservative: only facts that hold in *both* branches
    /// are preserved. A fact is preserved if it appears in both contexts
    /// for the same variable with the same constraint.
    pub fn merge(ctx_a: &RefinementContext, ctx_b: &RefinementContext) -> RefinementContext {
        let mut merged = RefinementContext::new();

        for (var, facts_a) in &ctx_a.facts {
            if let Some(facts_b) = ctx_b.facts.get(var) {
                // Keep only facts that appear in both branches.
                for fact in facts_a {
                    if facts_b.contains(fact) {
                        merged.add_fact(var, fact.clone());
                    }
                }
            }
            // If a variable only appears in one branch, we cannot
            // assume any facts about it after the join point.
        }

        merged
    }

    /// Return all known facts as a flat list of constraints.
    pub fn known_facts(&self) -> Vec<Constraint> {
        let mut all = Vec::new();
        for facts in self.facts.values() {
            all.extend(facts.iter().cloned());
        }
        all
    }

    /// Return facts for a specific variable.
    pub fn facts_for(&self, var: &str) -> &[Constraint] {
        self.facts.get(var).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Check whether the accumulated facts imply a given conclusion.
    ///
    /// Uses the solver to check: (all_facts) ∧ ¬conclusion → Unsat means
    /// the facts imply the conclusion.
    pub fn implies(&self, conclusion: &Constraint) -> SatResult {
        let all_facts = self.known_facts();
        if all_facts.is_empty() {
            // No facts — cannot imply anything.
            return SatResult::Unknown;
        }

        let premise = if all_facts.len() == 1 {
            all_facts.into_iter().next().unwrap()
        } else {
            Constraint::And(all_facts)
        };

        let mut solver = ToyConstraintSolver::new();
        solver.check_implication(&premise, conclusion)
    }

    /// Check if the context has any facts.
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }

    /// Get the number of distinct variables with facts.
    pub fn var_count(&self) -> usize {
        self.facts.len()
    }
}

impl Default for RefinementContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Negate a comparison operator.
fn negate_cmp(op: CmpOp) -> CmpOp {
    match op {
        CmpOp::Gt => CmpOp::LtEq,
        CmpOp::GtEq => CmpOp::Lt,
        CmpOp::Lt => CmpOp::GtEq,
        CmpOp::LtEq => CmpOp::Gt,
        CmpOp::Eq => CmpOp::NotEq,
        CmpOp::NotEq => CmpOp::Eq,
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::verification::constraints::CmpOp;

    fn int_cmp(var: &str, op: CmpOp, value: i64) -> Constraint {
        Constraint::IntComparison {
            var: var.to_string(),
            op,
            value,
        }
    }

    #[test]
    fn empty_context() {
        let ctx = RefinementContext::new();
        assert!(ctx.is_empty());
        assert_eq!(ctx.var_count(), 0);
        assert!(ctx.known_facts().is_empty());
    }

    #[test]
    fn add_fact_and_retrieve() {
        let mut ctx = RefinementContext::new();
        ctx.add_fact("x", int_cmp("x", CmpOp::Gt, 0));
        assert_eq!(ctx.var_count(), 1);
        assert_eq!(ctx.facts_for("x").len(), 1);
        assert_eq!(ctx.facts_for("y").len(), 0);
    }

    #[test]
    fn refine_from_int_comparison() {
        let mut ctx = RefinementContext::new();
        let cond = int_cmp("x", CmpOp::Gt, 5);
        assert!(ctx.refine_from_condition(&cond));
        assert_eq!(ctx.facts_for("x").len(), 1);
    }

    #[test]
    fn refine_from_conjunction() {
        let mut ctx = RefinementContext::new();
        let cond = Constraint::And(vec![
            int_cmp("x", CmpOp::Gt, 0),
            int_cmp("y", CmpOp::Lt, 10),
        ]);
        assert!(ctx.refine_from_condition(&cond));
        assert_eq!(ctx.facts_for("x").len(), 1);
        assert_eq!(ctx.facts_for("y").len(), 1);
    }

    #[test]
    fn refine_from_not_comparison() {
        // not(x > 5) → x <= 5
        let mut ctx = RefinementContext::new();
        let cond = Constraint::Not(Box::new(int_cmp("x", CmpOp::Gt, 5)));
        assert!(ctx.refine_from_condition(&cond));
        let facts = ctx.facts_for("x");
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0], int_cmp("x", CmpOp::LtEq, 5));
    }

    #[test]
    fn refine_from_bool_const_returns_false() {
        let mut ctx = RefinementContext::new();
        assert!(!ctx.refine_from_condition(&Constraint::BoolConst(true)));
        assert!(ctx.is_empty());
    }

    #[test]
    fn refine_from_bool_var() {
        let mut ctx = RefinementContext::new();
        let cond = Constraint::BoolVar("flag".to_string());
        assert!(ctx.refine_from_condition(&cond));
        let facts = ctx.facts_for("flag");
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0], Constraint::BoolConst(true));
    }

    #[test]
    fn refine_from_var_comparison() {
        let mut ctx = RefinementContext::new();
        let cond = Constraint::VarComparison {
            left: "x".to_string(),
            op: CmpOp::Gt,
            right: "y".to_string(),
        };
        assert!(ctx.refine_from_condition(&cond));
        // Fact added to both variables
        assert_eq!(ctx.facts_for("x").len(), 1);
        assert_eq!(ctx.facts_for("y").len(), 1);
    }

    #[test]
    fn merge_preserves_common_facts() {
        let mut ctx_a = RefinementContext::new();
        let mut ctx_b = RefinementContext::new();

        // Both branches know x > 0.
        ctx_a.add_fact("x", int_cmp("x", CmpOp::Gt, 0));
        ctx_b.add_fact("x", int_cmp("x", CmpOp::Gt, 0));

        // Only branch A knows y < 10.
        ctx_a.add_fact("y", int_cmp("y", CmpOp::Lt, 10));

        let merged = RefinementContext::merge(&ctx_a, &ctx_b);
        assert_eq!(merged.facts_for("x").len(), 1); // preserved
        assert_eq!(merged.facts_for("y").len(), 0); // dropped (only in A)
    }

    #[test]
    fn merge_drops_divergent_facts() {
        let mut ctx_a = RefinementContext::new();
        let mut ctx_b = RefinementContext::new();

        // A says x > 0, B says x > 5. Different constraints → not preserved.
        ctx_a.add_fact("x", int_cmp("x", CmpOp::Gt, 0));
        ctx_b.add_fact("x", int_cmp("x", CmpOp::Gt, 5));

        let merged = RefinementContext::merge(&ctx_a, &ctx_b);
        assert_eq!(merged.facts_for("x").len(), 0);
    }

    #[test]
    fn merge_nested_if_else() {
        // Simulate: if x > 0 then (if x < 100 then ...) else ...
        // Both branches know x > 0 from outer context.
        let mut outer = RefinementContext::new();
        outer.add_fact("x", int_cmp("x", CmpOp::Gt, 0));

        let mut then_ctx = outer.clone();
        then_ctx.add_fact("x", int_cmp("x", CmpOp::Lt, 100));

        let else_ctx = outer.clone();
        // else_ctx only has x > 0.

        let merged = RefinementContext::merge(&then_ctx, &else_ctx);
        // Only x > 0 is common.
        assert_eq!(merged.facts_for("x").len(), 1);
        assert_eq!(merged.facts_for("x")[0], int_cmp("x", CmpOp::Gt, 0));
    }

    #[test]
    fn implies_with_known_facts() {
        // If we know x > 5, then x > 0 should be implied.
        let mut ctx = RefinementContext::new();
        ctx.add_fact("x", int_cmp("x", CmpOp::Gt, 5));
        let conclusion = int_cmp("x", CmpOp::Gt, 0);
        assert_eq!(ctx.implies(&conclusion), SatResult::Unsat); // Unsat = valid implication
    }

    #[test]
    fn implies_fails_when_not_implied() {
        // If we know x > 0, x > 5 is NOT necessarily true.
        let mut ctx = RefinementContext::new();
        ctx.add_fact("x", int_cmp("x", CmpOp::Gt, 0));
        let conclusion = int_cmp("x", CmpOp::Gt, 5);
        assert_eq!(ctx.implies(&conclusion), SatResult::Sat); // Sat = counterexample exists
    }

    #[test]
    fn implies_empty_context_is_unknown() {
        let ctx = RefinementContext::new();
        let conclusion = int_cmp("x", CmpOp::Gt, 0);
        assert_eq!(ctx.implies(&conclusion), SatResult::Unknown);
    }

    #[test]
    fn implies_conjunction_of_facts() {
        // Know: x > 0 AND x < 100. Implies x >= 0.
        let mut ctx = RefinementContext::new();
        ctx.add_fact("x", int_cmp("x", CmpOp::Gt, 0));
        ctx.add_fact("x", int_cmp("x", CmpOp::Lt, 100));
        let conclusion = int_cmp("x", CmpOp::GtEq, 0);
        assert_eq!(ctx.implies(&conclusion), SatResult::Unsat); // valid
    }

    #[test]
    fn refine_from_arithmetic_constraint() {
        let mut ctx = RefinementContext::new();
        let cond = Constraint::Arithmetic {
            var: "x".to_string(),
            arith_op: super::super::constraints::ArithOp::Add,
            arith_const: 1,
            cmp_op: CmpOp::Gt,
            cmp_value: 0,
        };
        assert!(ctx.refine_from_condition(&cond));
        assert_eq!(ctx.facts_for("x").len(), 1);
    }
}
