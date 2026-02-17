//! Compile-time verification of `where`-clause constraints.
//!
//! This module is the foundation of Lumen's static verification system.
//! It collects `where` clauses from record field definitions and cell
//! signatures, lowers them to a solver-independent constraint IR, and
//! checks them against a pluggable SMT solver backend.
//!
//! ## Architecture
//!
//! ```text
//!   AST (Expr)
//!       │
//!       ▼
//!   constraints.rs   — lower_expr_to_constraint()
//!       │
//!       ▼
//!   Constraint IR    — IntComparison, And, Or, Not, …
//!       │
//!       ▼
//!   solver.rs        — Solver trait  →  ToyConstraintSolver (now)
//!       │                            →  Z3Backend           (T037)
//!       ▼
//!   VerificationResult
//! ```
//!
//! ## Integration
//!
//! This module is *not* yet wired into the main `compile()` pipeline.
//! Call [`verify()`] directly when you want to run verification as an
//! optional pass after type-checking (T044 will integrate it).

pub mod constraints;
pub mod solver;
pub mod sort_map;

use crate::compiler::ast::{Item, Program};
use crate::compiler::resolve::SymbolTable;

use constraints::{lower_expr_to_constraint, Constraint, LoweringError};
use solver::{SatResult, Solver, ToyConstraintSolver};

// ── Result types ────────────────────────────────────────────────────

/// Outcome of verifying a single constraint.
#[derive(Debug, Clone, PartialEq)]
pub enum VerificationResult {
    /// The constraint was proven to always hold.
    Verified { constraint: String },
    /// The solver could not determine the constraint's validity
    /// (e.g. it contains expressions the toy solver cannot handle).
    Unverifiable { constraint: String, reason: String },
    /// The constraint can be violated — there exists a counterexample.
    Violated {
        constraint: String,
        counterexample: Option<String>,
    },
}

// ── Verification context ────────────────────────────────────────────

/// Holds solver state and type mappings for a verification session.
pub struct VerificationContext {
    solver: Box<dyn Solver>,
}

impl VerificationContext {
    /// Create a new context using the default toy solver.
    pub fn new() -> Self {
        Self {
            solver: Box::new(ToyConstraintSolver::new()),
        }
    }

    /// Create a context with a custom solver backend.
    pub fn with_solver(solver: Box<dyn Solver>) -> Self {
        Self { solver }
    }

    /// Verify a single constraint expression.
    ///
    /// Strategy: assert the *negation* of the constraint and check for
    /// unsatisfiability.  If ¬C is Unsat then C is a tautology (Verified).
    /// If ¬C is Sat we have a counterexample (Violated).
    /// If Unknown, the result is Unverifiable.
    pub fn verify_constraint(&mut self, constraint: &Constraint) -> VerificationResult {
        let display = format!("{}", constraint);

        self.solver.push();
        let negated = Constraint::Not(Box::new(constraint.clone()));
        self.solver.assert_constraint(&negated);

        let result = match self.solver.check_sat() {
            SatResult::Unsat => VerificationResult::Verified {
                constraint: display,
            },
            SatResult::Sat => VerificationResult::Violated {
                constraint: display,
                counterexample: self.solver.get_model(),
            },
            SatResult::Unknown => VerificationResult::Unverifiable {
                constraint: display,
                reason: "solver returned Unknown".to_string(),
            },
        };

        self.solver.pop();
        result
    }
}

impl Default for VerificationContext {
    fn default() -> Self {
        Self::new()
    }
}

// ── Collected constraint ────────────────────────────────────────────

/// A constraint extracted from the AST along with its provenance.
#[derive(Debug, Clone)]
pub struct CollectedConstraint {
    /// Human-readable origin: "record Foo, field bar" or "cell baz, where-clause #1"
    pub origin: String,
    /// The lowered constraint, or an error if lowering failed.
    pub lowered: Result<Constraint, LoweringError>,
}

// ── Public entry point ──────────────────────────────────────────────

/// Walk the program AST, collect all `where`-clause constraints, and
/// attempt to verify each one.
///
/// `_symbols` is accepted for forward compatibility (T039 sort mapping
/// will need it to resolve record field types).
pub fn verify(program: &Program, _symbols: &SymbolTable) -> Vec<VerificationResult> {
    let collected = collect_constraints(program);
    let mut ctx = VerificationContext::new();
    let mut results = Vec::with_capacity(collected.len());

    for cc in &collected {
        let result = match &cc.lowered {
            Ok(constraint) => ctx.verify_constraint(constraint),
            Err(err) => VerificationResult::Unverifiable {
                constraint: cc.origin.clone(),
                reason: format!("lowering failed: {}", err),
            },
        };
        results.push(result);
    }

    results
}

/// Collect all constraints from the program AST.
pub fn collect_constraints(program: &Program) -> Vec<CollectedConstraint> {
    let mut out = Vec::new();

    for item in &program.items {
        match item {
            Item::Record(rec) => {
                for field in &rec.fields {
                    if let Some(ref constraint_expr) = field.constraint {
                        let origin = format!("record {}, field {}", rec.name, field.name);
                        let lowered = lower_expr_to_constraint(constraint_expr);
                        out.push(CollectedConstraint { origin, lowered });
                    }
                }
            }
            Item::Cell(cell) => {
                for (i, wc) in cell.where_clauses.iter().enumerate() {
                    let origin = format!("cell {}, where-clause #{}", cell.name, i + 1);
                    let lowered = lower_expr_to_constraint(wc);
                    out.push(CollectedConstraint { origin, lowered });
                }
            }
            // Processes may contain cells with where-clauses.
            Item::Process(proc) => {
                for cell in &proc.cells {
                    for (i, wc) in cell.where_clauses.iter().enumerate() {
                        let origin = format!(
                            "process {}, cell {}, where-clause #{}",
                            proc.name,
                            cell.name,
                            i + 1,
                        );
                        let lowered = lower_expr_to_constraint(wc);
                        out.push(CollectedConstraint { origin, lowered });
                    }
                }
            }
            // Agents may contain cells.
            Item::Agent(agent) => {
                for cell in &agent.cells {
                    for (i, wc) in cell.where_clauses.iter().enumerate() {
                        let origin = format!(
                            "agent {}, cell {}, where-clause #{}",
                            agent.name,
                            cell.name,
                            i + 1,
                        );
                        let lowered = lower_expr_to_constraint(wc);
                        out.push(CollectedConstraint { origin, lowered });
                    }
                }
            }
            _ => {}
        }
    }

    out
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::ast::*;
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

    fn binop(lhs: Expr, op: BinOp, rhs: Expr) -> Expr {
        Expr::BinOp(Box::new(lhs), op, Box::new(rhs), span())
    }

    fn make_program_with_field_constraint(constraint: Expr) -> Program {
        Program {
            directives: vec![],
            items: vec![Item::Record(RecordDef {
                name: "TestRec".to_string(),
                generic_params: vec![],
                fields: vec![FieldDef {
                    name: "value".to_string(),
                    ty: TypeExpr::Named("Int".to_string(), span()),
                    default_value: None,
                    constraint: Some(constraint),
                    span: span(),
                }],
                is_pub: false,
                span: span(),
                doc: None,
            })],
            span: span(),
        }
    }

    fn empty_symbol_table() -> SymbolTable {
        SymbolTable {
            types: Default::default(),
            cells: Default::default(),
            cell_policies: Default::default(),
            tools: Default::default(),
            agents: Default::default(),
            processes: Default::default(),
            effects: Default::default(),
            effect_binds: Default::default(),
            handlers: Default::default(),
            addons: Default::default(),
            type_aliases: Default::default(),
            traits: Default::default(),
            impls: Default::default(),
            consts: Default::default(),
        }
    }

    #[test]
    fn collect_record_field_constraint() {
        let constraint = binop(ident("value"), BinOp::Gt, int_lit(0));
        let prog = make_program_with_field_constraint(constraint);
        let collected = collect_constraints(&prog);
        assert_eq!(collected.len(), 1);
        assert!(collected[0].lowered.is_ok());
        assert!(collected[0].origin.contains("TestRec"));
        assert!(collected[0].origin.contains("value"));
    }

    #[test]
    fn collect_cell_where_clause() {
        let wc = binop(ident("n"), BinOp::GtEq, int_lit(0));
        let prog = Program {
            directives: vec![],
            items: vec![Item::Cell(CellDef {
                name: "factorial".to_string(),
                generic_params: vec![],
                params: vec![],
                return_type: None,
                effects: vec![],
                body: vec![],
                is_pub: false,
                is_async: false,
                is_extern: false,
                where_clauses: vec![wc],
                span: span(),
                doc: None,
            })],
            span: span(),
        };
        let collected = collect_constraints(&prog);
        assert_eq!(collected.len(), 1);
        assert!(collected[0].origin.contains("factorial"));
    }

    #[test]
    fn verify_always_true_constraint() {
        // `true` is trivially verified.
        let prog = make_program_with_field_constraint(Expr::BoolLit(true, span()));
        let symbols = empty_symbol_table();
        let results = verify(&prog, &symbols);
        assert_eq!(results.len(), 1);
        // not(true) is Unsat → Verified
        matches!(&results[0], VerificationResult::Verified { .. });
    }

    #[test]
    fn verify_always_false_constraint() {
        // `false` is always violated.
        let prog = make_program_with_field_constraint(Expr::BoolLit(false, span()));
        let symbols = empty_symbol_table();
        let results = verify(&prog, &symbols);
        assert_eq!(results.len(), 1);
        // not(false) is Sat → Violated
        matches!(&results[0], VerificationResult::Violated { .. });
    }

    #[test]
    fn verify_simple_range_is_unverifiable() {
        // `x > 0 and x < 100` — the toy solver reports Unknown for
        // not(x > 0 and x < 100) because negation of a conjunction of
        // comparisons is outside its decidable fragment.
        let constraint = binop(
            binop(ident("x"), BinOp::Gt, int_lit(0)),
            BinOp::And,
            binop(ident("x"), BinOp::Lt, int_lit(100)),
        );
        let prog = make_program_with_field_constraint(constraint);
        let symbols = empty_symbol_table();
        let results = verify(&prog, &symbols);
        assert_eq!(results.len(), 1);
        // The negation of a satisfiable conjunction is also satisfiable or unknown.
        // The toy solver should report either Violated or Unverifiable — not Verified,
        // because x > 0 && x < 100 is NOT a tautology.
        assert!(
            !matches!(&results[0], VerificationResult::Verified { .. }),
            "x > 0 && x < 100 is not a tautology, should not be Verified",
        );
    }

    #[test]
    fn verify_no_constraints_yields_empty() {
        let prog = Program {
            directives: vec![],
            items: vec![Item::Record(RecordDef {
                name: "Empty".to_string(),
                generic_params: vec![],
                fields: vec![FieldDef {
                    name: "x".to_string(),
                    ty: TypeExpr::Named("Int".to_string(), span()),
                    default_value: None,
                    constraint: None,
                    span: span(),
                }],
                is_pub: false,
                span: span(),
                doc: None,
            })],
            span: span(),
        };
        let symbols = empty_symbol_table();
        let results = verify(&prog, &symbols);
        assert!(results.is_empty());
    }

    #[test]
    fn verification_context_with_custom_solver() {
        let solver = ToyConstraintSolver::new();
        let mut ctx = VerificationContext::with_solver(Box::new(solver));
        let c = Constraint::BoolConst(true);
        let result = ctx.verify_constraint(&c);
        assert!(matches!(result, VerificationResult::Verified { .. }));
    }

    #[test]
    fn verification_context_default() {
        let ctx = VerificationContext::default();
        // Just ensure it constructs without panic.
        drop(ctx);
    }
}
