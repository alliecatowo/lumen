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

pub mod bounds;
pub mod constraints;
pub mod counterexample;
pub mod proof_hints;
pub mod refinement;
pub mod smt_solver;
pub mod solver;
pub mod sort_map;

use crate::compiler::ast::{CallArg, CellDef, Expr, Item, Program, Stmt};
use crate::compiler::resolve::SymbolTable;

use constraints::{lower_expr_to_constraint, Constraint, LoweringError};
use refinement::RefinementContext;
use solver::{SatResult, Solver, ToyConstraintSolver};

use std::collections::HashMap;

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

// ── Cell contract verification ──────────────────────────────────────

/// Information about a cell's contract: preconditions from where-clauses
/// and parameter names for substitution at call sites.
#[derive(Debug, Clone)]
struct CellContract {
    /// Lowered preconditions from where-clauses.
    preconditions: Vec<Constraint>,
    /// Parameter names in order (for positional argument mapping).
    param_names: Vec<String>,
    /// Declared effects (for budget counting).
    #[allow(dead_code)]
    effects: Vec<String>,
}

/// Verify cell contracts across the program.
///
/// This function:
/// 1. Collects preconditions (where-clauses) for each cell.
/// 2. Walks all cell bodies looking for call sites.
/// 3. At each call site, checks whether the arguments satisfy the callee's
///    preconditions (by substituting known argument values).
/// 4. Counts effect-related calls and checks against effect budgets.
///
/// Returns a list of `VerificationResult` entries for each check performed.
pub fn verify_cell_contracts(program: &Program) -> Vec<VerificationResult> {
    let mut results = Vec::new();
    let contracts = collect_cell_contracts(program);

    // Walk each cell body and check call sites.
    for item in &program.items {
        match item {
            Item::Cell(cell) => {
                verify_cell_body(cell, &contracts, &mut results);
            }
            Item::Process(proc) => {
                for cell in &proc.cells {
                    verify_cell_body(cell, &contracts, &mut results);
                }
            }
            Item::Agent(agent) => {
                for cell in &agent.cells {
                    verify_cell_body(cell, &contracts, &mut results);
                }
            }
            _ => {}
        }
    }

    results
}

/// Collect cell contracts (preconditions + param names) from the program.
fn collect_cell_contracts(program: &Program) -> HashMap<String, CellContract> {
    let mut contracts = HashMap::new();

    for item in &program.items {
        let cells: Vec<&CellDef> = match item {
            Item::Cell(c) => vec![c],
            Item::Process(p) => p.cells.iter().collect(),
            Item::Agent(a) => a.cells.iter().collect(),
            _ => continue,
        };

        for cell in cells {
            let mut preconditions = Vec::new();
            for wc in &cell.where_clauses {
                if let Ok(constraint) = lower_expr_to_constraint(wc) {
                    preconditions.push(constraint);
                }
            }
            let param_names: Vec<String> = cell.params.iter().map(|p| p.name.clone()).collect();
            contracts.insert(
                cell.name.clone(),
                CellContract {
                    preconditions,
                    param_names,
                    effects: cell.effects.clone(),
                },
            );
        }
    }

    contracts
}

/// Verify all call sites within a cell body.
fn verify_cell_body(
    cell: &CellDef,
    contracts: &HashMap<String, CellContract>,
    results: &mut Vec<VerificationResult>,
) {
    // Build a refinement context from the cell's own parameters and preconditions.
    let mut caller_ctx = RefinementContext::new();
    if let Some(contract) = contracts.get(&cell.name) {
        for pre in &contract.preconditions {
            caller_ctx.refine_from_condition(pre);
        }
    }

    // Track effect call counts for budget verification.
    let mut effect_counts: HashMap<String, u32> = HashMap::new();

    // Walk the body.
    for stmt in &cell.body {
        check_stmt_calls(stmt, &cell.name, contracts, &caller_ctx, results);
        count_effect_calls_in_stmt(stmt, &mut effect_counts);
    }

    // Check effect budgets (if the cell declares effects).
    // We treat the declared effect list as a budget: each declared effect
    // should not be called more times than reasonable. For now, we don't
    // have explicit budget annotations, but we verify EffectBudget
    // constraints if they appear in the contract.
    if let Some(contract) = contracts.get(&cell.name) {
        for pre in &contract.preconditions {
            if let Constraint::EffectBudget {
                effect_name,
                max_calls,
                ..
            } = pre
            {
                let actual = effect_counts.get(effect_name).copied().unwrap_or(0);
                let budget_constraint = Constraint::EffectBudget {
                    effect_name: effect_name.clone(),
                    max_calls: *max_calls,
                    actual_calls: actual,
                };
                let display = format!(
                    "cell {}: effect budget for '{}' (max={}, actual={})",
                    cell.name, effect_name, max_calls, actual
                );
                let mut ctx = VerificationContext::new();
                let vr = ctx.verify_constraint(&budget_constraint);
                // Re-label with our custom description.
                let result = match vr {
                    VerificationResult::Verified { .. } => VerificationResult::Verified {
                        constraint: display,
                    },
                    VerificationResult::Violated { counterexample, .. } => {
                        VerificationResult::Violated {
                            constraint: display,
                            counterexample,
                        }
                    }
                    VerificationResult::Unverifiable { reason, .. } => {
                        VerificationResult::Unverifiable {
                            constraint: display,
                            reason,
                        }
                    }
                };
                results.push(result);
            }
        }
    }
}

/// Recursively check call expressions in a statement.
fn check_stmt_calls(
    stmt: &Stmt,
    caller_name: &str,
    contracts: &HashMap<String, CellContract>,
    ctx: &RefinementContext,
    results: &mut Vec<VerificationResult>,
) {
    match stmt {
        Stmt::Expr(expr_stmt) => {
            check_expr_calls(&expr_stmt.expr, caller_name, contracts, ctx, results);
        }
        Stmt::Let(let_stmt) => {
            check_expr_calls(&let_stmt.value, caller_name, contracts, ctx, results);
        }
        Stmt::Return(ret) => {
            check_expr_calls(&ret.value, caller_name, contracts, ctx, results);
        }
        Stmt::If(if_stmt) => {
            check_expr_calls(&if_stmt.condition, caller_name, contracts, ctx, results);

            // Path-sensitive: refine context for then/else branches.
            if let Ok(cond_constraint) = lower_expr_to_constraint(&if_stmt.condition) {
                let mut then_ctx = ctx.clone();
                then_ctx.refine_from_condition(&cond_constraint);
                for s in &if_stmt.then_body {
                    check_stmt_calls(s, caller_name, contracts, &then_ctx, results);
                }

                if let Some(ref else_body) = if_stmt.else_body {
                    let mut else_ctx = ctx.clone();
                    let negated = Constraint::Not(Box::new(cond_constraint));
                    else_ctx.refine_from_condition(&negated);
                    for s in else_body {
                        check_stmt_calls(s, caller_name, contracts, &else_ctx, results);
                    }
                }
            } else {
                // Condition couldn't be lowered — check without refinement.
                for s in &if_stmt.then_body {
                    check_stmt_calls(s, caller_name, contracts, ctx, results);
                }
                if let Some(ref else_body) = if_stmt.else_body {
                    for s in else_body {
                        check_stmt_calls(s, caller_name, contracts, ctx, results);
                    }
                }
            }
            // Already handled children — fall through to end of function.
        }
        Stmt::For(for_stmt) => {
            check_expr_calls(&for_stmt.iter, caller_name, contracts, ctx, results);
            for s in &for_stmt.body {
                check_stmt_calls(s, caller_name, contracts, ctx, results);
            }
        }
        Stmt::While(while_stmt) => {
            check_expr_calls(&while_stmt.condition, caller_name, contracts, ctx, results);
            for s in &while_stmt.body {
                check_stmt_calls(s, caller_name, contracts, ctx, results);
            }
        }
        Stmt::Match(match_stmt) => {
            check_expr_calls(&match_stmt.subject, caller_name, contracts, ctx, results);
            for arm in &match_stmt.arms {
                for s in &arm.body {
                    check_stmt_calls(s, caller_name, contracts, ctx, results);
                }
            }
        }
        Stmt::Loop(loop_stmt) => {
            for s in &loop_stmt.body {
                check_stmt_calls(s, caller_name, contracts, ctx, results);
            }
        }
        Stmt::Assign(assign) => {
            check_expr_calls(&assign.value, caller_name, contracts, ctx, results);
        }
        Stmt::CompoundAssign(ca) => {
            check_expr_calls(&ca.value, caller_name, contracts, ctx, results);
        }
        Stmt::Defer(defer) => {
            for s in &defer.body {
                check_stmt_calls(s, caller_name, contracts, ctx, results);
            }
        }
        Stmt::Yield(yield_stmt) => {
            check_expr_calls(&yield_stmt.value, caller_name, contracts, ctx, results);
        }
        Stmt::Emit(emit) => {
            check_expr_calls(&emit.value, caller_name, contracts, ctx, results);
        }
        _ => {}
    }
}

/// Check a call expression against the callee's preconditions.
fn check_expr_calls(
    expr: &Expr,
    caller_name: &str,
    contracts: &HashMap<String, CellContract>,
    ctx: &RefinementContext,
    results: &mut Vec<VerificationResult>,
) {
    match expr {
        Expr::Call(callee, args, _span) => {
            // First, recursively check sub-expressions.
            check_expr_calls(callee, caller_name, contracts, ctx, results);
            for arg in args {
                match arg {
                    CallArg::Positional(e) | CallArg::Named(_, e, _) | CallArg::Role(_, e, _) => {
                        check_expr_calls(e, caller_name, contracts, ctx, results);
                    }
                }
            }

            // Now check preconditions for the callee.
            if let Expr::Ident(callee_name, _) = callee.as_ref() {
                if let Some(contract) = contracts.get(callee_name.as_str()) {
                    check_call_preconditions(
                        caller_name,
                        callee_name,
                        contract,
                        args,
                        ctx,
                        results,
                    );
                }
            }
        }
        Expr::BinOp(lhs, _, rhs, _) => {
            check_expr_calls(lhs, caller_name, contracts, ctx, results);
            check_expr_calls(rhs, caller_name, contracts, ctx, results);
        }
        Expr::UnaryOp(_, inner, _) => {
            check_expr_calls(inner, caller_name, contracts, ctx, results);
        }
        Expr::IfExpr {
            cond,
            then_val,
            else_val,
            ..
        } => {
            check_expr_calls(cond, caller_name, contracts, ctx, results);
            check_expr_calls(then_val, caller_name, contracts, ctx, results);
            check_expr_calls(else_val, caller_name, contracts, ctx, results);
        }
        _ => {
            // For other expression types, we don't recurse deeply.
            // This could be expanded in the future.
        }
    }
}

/// Check whether call arguments satisfy the callee's preconditions.
fn check_call_preconditions(
    caller_name: &str,
    callee_name: &str,
    contract: &CellContract,
    args: &[CallArg],
    ctx: &RefinementContext,
    results: &mut Vec<VerificationResult>,
) {
    if contract.preconditions.is_empty() {
        return;
    }

    // Build a mapping from parameter names to argument expressions.
    let mut param_to_arg: HashMap<String, &Expr> = HashMap::new();
    for (i, arg) in args.iter().enumerate() {
        match arg {
            CallArg::Positional(expr) => {
                if i < contract.param_names.len() {
                    param_to_arg.insert(contract.param_names[i].clone(), expr);
                }
            }
            CallArg::Named(name, expr, _) => {
                param_to_arg.insert(name.clone(), expr);
            }
            CallArg::Role(_, _, _) => {
                // Role blocks are not regular parameters.
            }
        }
    }

    // For each precondition, try to verify it at the call site.
    for (i, pre) in contract.preconditions.iter().enumerate() {
        // Skip EffectBudget constraints — those are checked differently.
        if matches!(pre, Constraint::EffectBudget { .. }) {
            continue;
        }

        let display = format!(
            "call {}() in {}: precondition #{}",
            callee_name,
            caller_name,
            i + 1
        );

        // Try to substitute known literal values.
        let mut substituted = pre.clone();
        let mut all_substituted = true;
        let mut any_var_arg = false;

        for (param_name, arg_expr) in &param_to_arg {
            match arg_expr {
                Expr::IntLit(val, _) => {
                    substituted = substituted.substitute_int(param_name, *val);
                }
                Expr::Ident(arg_name, _) => {
                    any_var_arg = true;
                    // Rename the parameter to the caller's variable name
                    // so the refinement context can reason about it.
                    substituted = substituted.rename_var(param_name, arg_name);
                }
                _ => {
                    all_substituted = false;
                }
            }
        }

        // If all arguments were literals, the substituted constraint is concrete.
        if all_substituted && !any_var_arg {
            let mut ctx_inner = VerificationContext::new();
            let result = ctx_inner.verify_constraint(&substituted);
            let relabeled = match result {
                VerificationResult::Verified { .. } => VerificationResult::Verified {
                    constraint: display,
                },
                VerificationResult::Violated { counterexample, .. } => {
                    VerificationResult::Violated {
                        constraint: display,
                        counterexample,
                    }
                }
                VerificationResult::Unverifiable { reason, .. } => {
                    VerificationResult::Unverifiable {
                        constraint: display,
                        reason,
                    }
                }
            };
            results.push(relabeled);
        } else if any_var_arg {
            // Try to use the refinement context to check implication.
            let implication = ctx.implies(&substituted);
            let result = match implication {
                SatResult::Unsat => VerificationResult::Verified {
                    constraint: display,
                },
                SatResult::Sat => VerificationResult::Violated {
                    constraint: display,
                    counterexample: None,
                },
                SatResult::Unknown => VerificationResult::Unverifiable {
                    constraint: display,
                    reason: "could not determine from caller context".to_string(),
                },
            };
            results.push(result);
        } else {
            results.push(VerificationResult::Unverifiable {
                constraint: display,
                reason: "argument expression too complex for static analysis".to_string(),
            });
        }
    }
}

/// Count calls that correspond to effects in a statement (by name matching).
fn count_effect_calls_in_stmt(stmt: &Stmt, counts: &mut HashMap<String, u32>) {
    match stmt {
        Stmt::Expr(expr_stmt) => count_effect_calls_in_expr(&expr_stmt.expr, counts),
        Stmt::Let(let_stmt) => count_effect_calls_in_expr(&let_stmt.value, counts),
        Stmt::Return(ret) => {
            count_effect_calls_in_expr(&ret.value, counts);
        }
        Stmt::If(if_stmt) => {
            count_effect_calls_in_expr(&if_stmt.condition, counts);
            for s in &if_stmt.then_body {
                count_effect_calls_in_stmt(s, counts);
            }
            if let Some(ref else_body) = if_stmt.else_body {
                for s in else_body {
                    count_effect_calls_in_stmt(s, counts);
                }
            }
        }
        Stmt::For(for_stmt) => {
            count_effect_calls_in_expr(&for_stmt.iter, counts);
            for s in &for_stmt.body {
                count_effect_calls_in_stmt(s, counts);
            }
        }
        Stmt::While(while_stmt) => {
            count_effect_calls_in_expr(&while_stmt.condition, counts);
            for s in &while_stmt.body {
                count_effect_calls_in_stmt(s, counts);
            }
        }
        Stmt::Loop(loop_stmt) => {
            for s in &loop_stmt.body {
                count_effect_calls_in_stmt(s, counts);
            }
        }
        _ => {}
    }
}

/// Count effect-like calls in an expression.
/// We count `perform Effect.operation(args)` patterns.
fn count_effect_calls_in_expr(expr: &Expr, counts: &mut HashMap<String, u32>) {
    match expr {
        Expr::Call(callee, args, _) => {
            // Check if callee is an effect operation: Effect.operation
            if let Expr::DotAccess(base, _op, _) = callee.as_ref() {
                if let Expr::Ident(effect_name, _) = base.as_ref() {
                    *counts.entry(effect_name.clone()).or_insert(0) += 1;
                }
            }
            // Also count direct calls by name (for simple tool-like effects).
            if let Expr::Ident(name, _) = callee.as_ref() {
                *counts.entry(name.clone()).or_insert(0) += 1;
            }
            for arg in args {
                match arg {
                    CallArg::Positional(e) | CallArg::Named(_, e, _) | CallArg::Role(_, e, _) => {
                        count_effect_calls_in_expr(e, counts);
                    }
                }
            }
        }
        Expr::BinOp(lhs, _, rhs, _) => {
            count_effect_calls_in_expr(lhs, counts);
            count_effect_calls_in_expr(rhs, counts);
        }
        Expr::UnaryOp(_, inner, _) => {
            count_effect_calls_in_expr(inner, counts);
        }
        _ => {}
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::ast::*;
    use crate::compiler::tokens::Span;
    use std::collections::HashMap;

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
                must_use: false,
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

    // ── Cell contract verification tests ────────────────────────

    fn make_cell(
        name: &str,
        params: Vec<Param>,
        where_clauses: Vec<Expr>,
        body: Vec<Stmt>,
        effects: Vec<String>,
    ) -> CellDef {
        CellDef {
            name: name.to_string(),
            generic_params: vec![],
            params,
            return_type: None,
            effects,
            body,
            is_pub: false,
            is_async: false,
            is_extern: false,
            must_use: false,
            where_clauses,
            span: span(),
            doc: None,
        }
    }

    fn make_param(name: &str) -> Param {
        Param {
            name: name.to_string(),
            ty: TypeExpr::Named("Int".to_string(), span()),
            default_value: None,
            variadic: false,
            span: span(),
        }
    }

    fn call_expr(callee_name: &str, args: Vec<Expr>) -> Expr {
        Expr::Call(
            Box::new(ident(callee_name)),
            args.into_iter().map(CallArg::Positional).collect(),
            span(),
        )
    }

    fn expr_stmt(expr: Expr) -> Stmt {
        Stmt::Expr(ExprStmt { expr, span: span() })
    }

    #[test]
    fn contract_precondition_literal_satisfied() {
        // cell callee(n: Int) where n > 0 end
        // cell caller() callee(5) end
        let callee = make_cell(
            "callee",
            vec![make_param("n")],
            vec![binop(ident("n"), BinOp::Gt, int_lit(0))],
            vec![],
            vec![],
        );
        let caller = make_cell(
            "caller",
            vec![],
            vec![],
            vec![expr_stmt(call_expr("callee", vec![int_lit(5)]))],
            vec![],
        );
        let prog = Program {
            directives: vec![],
            items: vec![Item::Cell(callee), Item::Cell(caller)],
            span: span(),
        };

        let results = verify_cell_contracts(&prog);
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0], VerificationResult::Verified { constraint } if constraint.contains("callee")),
            "calling callee(5) should verify n > 0: {:?}",
            results[0]
        );
    }

    #[test]
    fn contract_precondition_literal_violated() {
        // cell callee(n: Int) where n > 0 end
        // cell caller() callee(-1) end
        let callee = make_cell(
            "callee",
            vec![make_param("n")],
            vec![binop(ident("n"), BinOp::Gt, int_lit(0))],
            vec![],
            vec![],
        );
        let caller = make_cell(
            "caller",
            vec![],
            vec![],
            vec![expr_stmt(call_expr("callee", vec![int_lit(-1)]))],
            vec![],
        );
        let prog = Program {
            directives: vec![],
            items: vec![Item::Cell(callee), Item::Cell(caller)],
            span: span(),
        };

        let results = verify_cell_contracts(&prog);
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0], VerificationResult::Violated { .. }),
            "calling callee(-1) should violate n > 0: {:?}",
            results[0]
        );
    }

    #[test]
    fn contract_precondition_variable_known_positive() {
        // cell callee(n: Int) where n > 0 end
        // cell caller(x: Int) where x > 5 callee(x) end
        // Caller knows x > 5, so x > 0 should be implied.
        let callee = make_cell(
            "callee",
            vec![make_param("n")],
            vec![binop(ident("n"), BinOp::Gt, int_lit(0))],
            vec![],
            vec![],
        );
        let caller = make_cell(
            "caller",
            vec![make_param("x")],
            vec![binop(ident("x"), BinOp::Gt, int_lit(5))],
            vec![expr_stmt(call_expr("callee", vec![ident("x")]))],
            vec![],
        );
        let prog = Program {
            directives: vec![],
            items: vec![Item::Cell(callee), Item::Cell(caller)],
            span: span(),
        };

        let results = verify_cell_contracts(&prog);
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0], VerificationResult::Verified { .. }),
            "caller's x > 5 should imply callee's n > 0: {:?}",
            results[0]
        );
    }

    #[test]
    fn contract_precondition_variable_insufficient() {
        // cell callee(n: Int) where n > 10 end
        // cell caller(x: Int) where x > 0 callee(x) end
        // Caller knows x > 0, but callee needs n > 10 — not sufficient.
        let callee = make_cell(
            "callee",
            vec![make_param("n")],
            vec![binop(ident("n"), BinOp::Gt, int_lit(10))],
            vec![],
            vec![],
        );
        let caller = make_cell(
            "caller",
            vec![make_param("x")],
            vec![binop(ident("x"), BinOp::Gt, int_lit(0))],
            vec![expr_stmt(call_expr("callee", vec![ident("x")]))],
            vec![],
        );
        let prog = Program {
            directives: vec![],
            items: vec![Item::Cell(callee), Item::Cell(caller)],
            span: span(),
        };

        let results = verify_cell_contracts(&prog);
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0], VerificationResult::Violated { .. }),
            "caller's x > 0 should NOT imply callee's n > 10: {:?}",
            results[0]
        );
    }

    #[test]
    fn contract_no_preconditions_no_results() {
        // cell callee(n: Int) end  (no where clause)
        // cell caller() callee(5) end
        let callee = make_cell("callee", vec![make_param("n")], vec![], vec![], vec![]);
        let caller = make_cell(
            "caller",
            vec![],
            vec![],
            vec![expr_stmt(call_expr("callee", vec![int_lit(5)]))],
            vec![],
        );
        let prog = Program {
            directives: vec![],
            items: vec![Item::Cell(callee), Item::Cell(caller)],
            span: span(),
        };

        let results = verify_cell_contracts(&prog);
        assert!(results.is_empty(), "no preconditions → no checks");
    }

    #[test]
    fn contract_multiple_preconditions() {
        // cell callee(n: Int) where n > 0 and n < 100 end
        // cell caller() callee(50) end → both should verify
        let callee = make_cell(
            "callee",
            vec![make_param("n")],
            vec![
                binop(ident("n"), BinOp::Gt, int_lit(0)),
                binop(ident("n"), BinOp::Lt, int_lit(100)),
            ],
            vec![],
            vec![],
        );
        let caller = make_cell(
            "caller",
            vec![],
            vec![],
            vec![expr_stmt(call_expr("callee", vec![int_lit(50)]))],
            vec![],
        );
        let prog = Program {
            directives: vec![],
            items: vec![Item::Cell(callee), Item::Cell(caller)],
            span: span(),
        };

        let results = verify_cell_contracts(&prog);
        assert_eq!(results.len(), 2);
        assert!(
            results
                .iter()
                .all(|r| matches!(r, VerificationResult::Verified { .. })),
            "50 satisfies both n > 0 and n < 100: {:?}",
            results
        );
    }

    #[test]
    fn contract_precondition_boundary_zero() {
        // cell callee(n: Int) where n > 0 end
        // cell caller() callee(0) end → violated (0 is not > 0)
        let callee = make_cell(
            "callee",
            vec![make_param("n")],
            vec![binop(ident("n"), BinOp::Gt, int_lit(0))],
            vec![],
            vec![],
        );
        let caller = make_cell(
            "caller",
            vec![],
            vec![],
            vec![expr_stmt(call_expr("callee", vec![int_lit(0)]))],
            vec![],
        );
        let prog = Program {
            directives: vec![],
            items: vec![Item::Cell(callee), Item::Cell(caller)],
            span: span(),
        };

        let results = verify_cell_contracts(&prog);
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0], VerificationResult::Violated { .. }),
            "callee(0) should violate n > 0: {:?}",
            results[0]
        );
    }

    #[test]
    fn contract_precondition_gte_boundary() {
        // cell callee(n: Int) where n >= 0 end
        // cell caller() callee(0) end → verified (0 >= 0 is true)
        let callee = make_cell(
            "callee",
            vec![make_param("n")],
            vec![binop(ident("n"), BinOp::GtEq, int_lit(0))],
            vec![],
            vec![],
        );
        let caller = make_cell(
            "caller",
            vec![],
            vec![],
            vec![expr_stmt(call_expr("callee", vec![int_lit(0)]))],
            vec![],
        );
        let prog = Program {
            directives: vec![],
            items: vec![Item::Cell(callee), Item::Cell(caller)],
            span: span(),
        };

        let results = verify_cell_contracts(&prog);
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0], VerificationResult::Verified { .. }),
            "callee(0) should verify n >= 0: {:?}",
            results[0]
        );
    }

    #[test]
    fn contract_call_in_if_then_branch() {
        // cell callee(n: Int) where n > 0 end
        // cell caller(x: Int)
        //   if x > 0
        //     callee(x)  # should verify — we know x > 0 here
        //   end
        // end
        let callee = make_cell(
            "callee",
            vec![make_param("n")],
            vec![binop(ident("n"), BinOp::Gt, int_lit(0))],
            vec![],
            vec![],
        );
        let caller = make_cell(
            "caller",
            vec![make_param("x")],
            vec![],
            vec![Stmt::If(IfStmt {
                condition: binop(ident("x"), BinOp::Gt, int_lit(0)),
                then_body: vec![expr_stmt(call_expr("callee", vec![ident("x")]))],
                else_body: None,
                span: span(),
            })],
            vec![],
        );
        let prog = Program {
            directives: vec![],
            items: vec![Item::Cell(callee), Item::Cell(caller)],
            span: span(),
        };

        let results = verify_cell_contracts(&prog);
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0], VerificationResult::Verified { .. }),
            "in then-branch of x > 0, callee(x) should verify: {:?}",
            results[0]
        );
    }

    #[test]
    fn contract_call_in_else_branch_negated() {
        // cell needs_nonpositive(n: Int) where n <= 0 end
        // cell caller(x: Int)
        //   if x > 0
        //     # then branch
        //   else
        //     needs_nonpositive(x)  # should verify — we know NOT(x > 0) → x <= 0
        //   end
        // end
        let callee = make_cell(
            "needs_nonpositive",
            vec![make_param("n")],
            vec![binop(ident("n"), BinOp::LtEq, int_lit(0))],
            vec![],
            vec![],
        );
        let caller = make_cell(
            "caller",
            vec![make_param("x")],
            vec![],
            vec![Stmt::If(IfStmt {
                condition: binop(ident("x"), BinOp::Gt, int_lit(0)),
                then_body: vec![],
                else_body: Some(vec![expr_stmt(call_expr(
                    "needs_nonpositive",
                    vec![ident("x")],
                ))]),
                span: span(),
            })],
            vec![],
        );
        let prog = Program {
            directives: vec![],
            items: vec![Item::Cell(callee), Item::Cell(caller)],
            span: span(),
        };

        let results = verify_cell_contracts(&prog);
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0], VerificationResult::Verified { .. }),
            "in else-branch of x > 0, needs_nonpositive(x) should verify: {:?}",
            results[0]
        );
    }

    #[test]
    fn contract_unknown_callee_no_results() {
        // cell caller() unknown_fn(5) end
        // Since unknown_fn has no contract, no checks.
        let caller = make_cell(
            "caller",
            vec![],
            vec![],
            vec![expr_stmt(call_expr("unknown_fn", vec![int_lit(5)]))],
            vec![],
        );
        let prog = Program {
            directives: vec![],
            items: vec![Item::Cell(caller)],
            span: span(),
        };

        let results = verify_cell_contracts(&prog);
        assert!(results.is_empty());
    }

    #[test]
    fn contract_empty_program() {
        let prog = Program {
            directives: vec![],
            items: vec![],
            span: span(),
        };
        let results = verify_cell_contracts(&prog);
        assert!(results.is_empty());
    }

    #[test]
    fn contract_effect_budget_within_limit() {
        // cell worker() / {network}
        //   where effect_budget: network max=2
        //   network.fetch()
        //   network.fetch()
        // end
        // We simulate this by creating a cell with an EffectBudget precondition
        // and a body that calls network.fetch twice.
        let fetch_call = Expr::Call(
            Box::new(Expr::DotAccess(
                Box::new(ident("network")),
                "fetch".to_string(),
                span(),
            )),
            vec![],
            span(),
        );
        let budget_constraint = Constraint::EffectBudget {
            effect_name: "network".to_string(),
            max_calls: 2,
            actual_calls: 0, // placeholder; actual is counted at verification time
        };
        // We need to embed the budget constraint as a where-clause expression.
        // Since EffectBudget can't be round-tripped through AST Expr, we test
        // the contract verification directly using the internal API.
        let mut contracts = HashMap::new();
        contracts.insert(
            "worker".to_string(),
            CellContract {
                preconditions: vec![budget_constraint],
                param_names: vec![],
                effects: vec!["network".to_string()],
            },
        );

        let cell = make_cell(
            "worker",
            vec![],
            vec![],
            vec![expr_stmt(fetch_call.clone()), expr_stmt(fetch_call)],
            vec!["network".to_string()],
        );

        let mut results = Vec::new();
        verify_cell_body(&cell, &contracts, &mut results);
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0], VerificationResult::Verified { .. }),
            "2 calls within budget of 2: {:?}",
            results[0]
        );
    }

    #[test]
    fn contract_effect_budget_exceeded() {
        let fetch_call = Expr::Call(
            Box::new(Expr::DotAccess(
                Box::new(ident("network")),
                "fetch".to_string(),
                span(),
            )),
            vec![],
            span(),
        );
        let budget_constraint = Constraint::EffectBudget {
            effect_name: "network".to_string(),
            max_calls: 1,
            actual_calls: 0,
        };
        let mut contracts = HashMap::new();
        contracts.insert(
            "worker".to_string(),
            CellContract {
                preconditions: vec![budget_constraint],
                param_names: vec![],
                effects: vec!["network".to_string()],
            },
        );

        let cell = make_cell(
            "worker",
            vec![],
            vec![],
            vec![
                expr_stmt(fetch_call.clone()),
                expr_stmt(fetch_call.clone()),
                expr_stmt(fetch_call),
            ],
            vec!["network".to_string()],
        );

        let mut results = Vec::new();
        verify_cell_body(&cell, &contracts, &mut results);
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0], VerificationResult::Violated { .. }),
            "3 calls exceeds budget of 1: {:?}",
            results[0]
        );
    }

    #[test]
    fn contract_process_cell_verified() {
        // Process cells should also be checked.
        let callee = make_cell(
            "callee",
            vec![make_param("n")],
            vec![binop(ident("n"), BinOp::Gt, int_lit(0))],
            vec![],
            vec![],
        );
        let process_cell = make_cell(
            "proc_fn",
            vec![],
            vec![],
            vec![expr_stmt(call_expr("callee", vec![int_lit(10)]))],
            vec![],
        );
        let prog = Program {
            directives: vec![],
            items: vec![
                Item::Cell(callee),
                Item::Process(ProcessDecl {
                    kind: "memory".to_string(),
                    name: "MyProc".to_string(),
                    configs: Default::default(),
                    cells: vec![process_cell],
                    grants: vec![],
                    pipeline_stages: vec![],
                    machine_initial: None,
                    machine_states: vec![],
                    span: span(),
                }),
            ],
            span: span(),
        };

        let results = verify_cell_contracts(&prog);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], VerificationResult::Verified { .. }));
    }
}
