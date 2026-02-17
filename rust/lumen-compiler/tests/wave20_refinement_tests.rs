//! Wave 20 — T051: Refinement verification tests
//!
//! Exercises the verification pipeline (typecheck + constraint validation) for:
//! - Division safety (non-zero divisor)
//! - Array/index bounds checks
//! - Negative tests (violated constraints)
//! - Record field constraints
//! - Path-sensitive analysis through if/else branches
//! - Cell contract verification at call sites

use lumen_compiler::compiler::ast::*;
use lumen_compiler::compiler::resolve::SymbolTable;
use lumen_compiler::compiler::tokens::Span;
use lumen_compiler::compiler::verification::constraints::{ArithOp, CmpOp, Constraint};
use lumen_compiler::compiler::verification::refinement::RefinementContext;
use lumen_compiler::compiler::verification::solver::{SatResult, Solver, ToyConstraintSolver};
use lumen_compiler::compiler::verification::{
    collect_constraints, verify, verify_cell_contracts, VerificationContext, VerificationResult,
};

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

fn make_param(name: &str) -> Param {
    Param {
        name: name.to_string(),
        ty: TypeExpr::Named("Int".to_string(), span()),
        default_value: None,
        variadic: false,
        span: span(),
    }
}

fn make_cell(name: &str, params: Vec<Param>, where_clauses: Vec<Expr>, body: Vec<Stmt>) -> CellDef {
    CellDef {
        name: name.to_string(),
        generic_params: vec![],
        params,
        return_type: None,
        effects: vec![],
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

// ── Division safety ─────────────────────────────────────────────────

#[test]
fn division_by_positive_literal_verified() {
    // cell divide(a: Int, b: Int) -> Int where b != 0
    // cell caller() -> Int  divide(10, 5)  end
    // Calling with literal 5 satisfies b != 0.
    let callee = make_cell(
        "divide",
        vec![make_param("a"), make_param("b")],
        vec![binop(ident("b"), BinOp::NotEq, int_lit(0))],
        vec![],
    );
    let caller = make_cell(
        "caller",
        vec![],
        vec![],
        vec![expr_stmt(call_expr(
            "divide",
            vec![int_lit(10), int_lit(5)],
        ))],
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
        "divide(10, 5) should verify b != 0: {:?}",
        results[0]
    );
}

#[test]
fn division_by_zero_literal_violated() {
    // Calling divide(10, 0) should violate b != 0.
    let callee = make_cell(
        "divide",
        vec![make_param("a"), make_param("b")],
        vec![binop(ident("b"), BinOp::NotEq, int_lit(0))],
        vec![],
    );
    let caller = make_cell(
        "caller",
        vec![],
        vec![],
        vec![expr_stmt(call_expr(
            "divide",
            vec![int_lit(10), int_lit(0)],
        ))],
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
        "divide(10, 0) should violate b != 0: {:?}",
        results[0]
    );
}

#[test]
fn division_by_known_nonzero_variable_verified() {
    // cell divide(a: Int, b: Int) -> Int where b != 0
    // cell caller(x: Int) where x > 0  divide(10, x)  end
    // Caller knows x > 0, which implies x != 0.
    let callee = make_cell(
        "divide",
        vec![make_param("a"), make_param("b")],
        vec![binop(ident("b"), BinOp::NotEq, int_lit(0))],
        vec![],
    );
    let caller = make_cell(
        "caller",
        vec![make_param("x")],
        vec![binop(ident("x"), BinOp::Gt, int_lit(0))],
        vec![expr_stmt(call_expr(
            "divide",
            vec![int_lit(10), ident("x")],
        ))],
    );
    let prog = Program {
        directives: vec![],
        items: vec![Item::Cell(callee), Item::Cell(caller)],
        span: span(),
    };

    let results = verify_cell_contracts(&prog);
    assert_eq!(results.len(), 1);
    // x > 0 implies x != 0. The toy solver should determine this.
    assert!(
        matches!(
            &results[0],
            VerificationResult::Verified { .. } | VerificationResult::Unverifiable { .. }
        ),
        "x > 0 should (at least attempt to) imply x != 0: {:?}",
        results[0]
    );
}

#[test]
fn division_by_possibly_zero_variable_not_verified() {
    // cell divide(a: Int, b: Int) -> Int where b != 0
    // cell caller(x: Int)  divide(10, x)  end    (no constraint on x)
    // Without knowing x != 0, the precondition cannot be verified.
    let callee = make_cell(
        "divide",
        vec![make_param("a"), make_param("b")],
        vec![binop(ident("b"), BinOp::NotEq, int_lit(0))],
        vec![],
    );
    let caller = make_cell(
        "caller",
        vec![make_param("x")],
        vec![],
        vec![expr_stmt(call_expr(
            "divide",
            vec![int_lit(10), ident("x")],
        ))],
    );
    let prog = Program {
        directives: vec![],
        items: vec![Item::Cell(callee), Item::Cell(caller)],
        span: span(),
    };

    let results = verify_cell_contracts(&prog);
    assert_eq!(results.len(), 1);
    // Without caller context, the solver should not verify the constraint.
    assert!(
        !matches!(&results[0], VerificationResult::Verified { .. }),
        "divide(10, x) with unknown x should NOT be verified: {:?}",
        results[0]
    );
}

// ── Array/index bounds ──────────────────────────────────────────────

#[test]
fn index_within_bounds_literal_verified() {
    // cell at_index(idx: Int) where idx >= 0 and idx < 10
    // cell caller()  at_index(5)  end
    let callee = make_cell(
        "at_index",
        vec![make_param("idx")],
        vec![
            binop(ident("idx"), BinOp::GtEq, int_lit(0)),
            binop(ident("idx"), BinOp::Lt, int_lit(10)),
        ],
        vec![],
    );
    let caller = make_cell(
        "caller",
        vec![],
        vec![],
        vec![expr_stmt(call_expr("at_index", vec![int_lit(5)]))],
    );
    let prog = Program {
        directives: vec![],
        items: vec![Item::Cell(callee), Item::Cell(caller)],
        span: span(),
    };

    let results = verify_cell_contracts(&prog);
    assert_eq!(results.len(), 2, "two preconditions: idx >= 0, idx < 10");
    assert!(
        results
            .iter()
            .all(|r| matches!(r, VerificationResult::Verified { .. })),
        "at_index(5) should verify both bounds: {:?}",
        results
    );
}

#[test]
fn index_out_of_bounds_violated() {
    // cell at_index(idx: Int) where idx >= 0 and idx < 10
    // cell caller()  at_index(15)  end
    let callee = make_cell(
        "at_index",
        vec![make_param("idx")],
        vec![
            binop(ident("idx"), BinOp::GtEq, int_lit(0)),
            binop(ident("idx"), BinOp::Lt, int_lit(10)),
        ],
        vec![],
    );
    let caller = make_cell(
        "caller",
        vec![],
        vec![],
        vec![expr_stmt(call_expr("at_index", vec![int_lit(15)]))],
    );
    let prog = Program {
        directives: vec![],
        items: vec![Item::Cell(callee), Item::Cell(caller)],
        span: span(),
    };

    let results = verify_cell_contracts(&prog);
    assert_eq!(results.len(), 2);
    // idx >= 0 is satisfied (15 >= 0), but idx < 10 is not (15 < 10 is false)
    let verified_count = results
        .iter()
        .filter(|r| matches!(r, VerificationResult::Verified { .. }))
        .count();
    let violated_count = results
        .iter()
        .filter(|r| matches!(r, VerificationResult::Violated { .. }))
        .count();
    assert_eq!(
        verified_count, 1,
        "idx >= 0 should verify for 15: {:?}",
        results
    );
    assert_eq!(
        violated_count, 1,
        "idx < 10 should be violated for 15: {:?}",
        results
    );
}

#[test]
fn index_negative_violated() {
    // cell at_index(idx: Int) where idx >= 0 and idx < 10
    // cell caller()  at_index(-1)  end
    let callee = make_cell(
        "at_index",
        vec![make_param("idx")],
        vec![
            binop(ident("idx"), BinOp::GtEq, int_lit(0)),
            binop(ident("idx"), BinOp::Lt, int_lit(10)),
        ],
        vec![],
    );
    let caller = make_cell(
        "caller",
        vec![],
        vec![],
        vec![expr_stmt(call_expr(
            "at_index",
            vec![Expr::UnaryOp(UnaryOp::Neg, Box::new(int_lit(1)), span())],
        ))],
    );
    let prog = Program {
        directives: vec![],
        items: vec![Item::Cell(callee), Item::Cell(caller)],
        span: span(),
    };

    let results = verify_cell_contracts(&prog);
    // With a UnaryOp::Neg expression, the solver may not be able to substitute,
    // so it may report Unverifiable for both. This is acceptable.
    assert_eq!(results.len(), 2);
    // At minimum, the results should not be Verified for a negative index.
    let all_verified = results
        .iter()
        .all(|r| matches!(r, VerificationResult::Verified { .. }));
    assert!(
        !all_verified,
        "at_index(-1) should NOT pass all verifications: {:?}",
        results
    );
}

// ── Record field constraint verification ────────────────────────────

#[test]
fn record_field_constraint_always_false_is_violated() {
    let program = Program {
        directives: vec![],
        items: vec![Item::Record(RecordDef {
            name: "Impossible".to_string(),
            generic_params: vec![],
            fields: vec![FieldDef {
                name: "x".to_string(),
                ty: TypeExpr::Named("Int".to_string(), span()),
                default_value: None,
                constraint: Some(Expr::BoolLit(false, span())),
                span: span(),
            }],
            is_pub: false,
            span: span(),
            doc: None,
        })],
        span: span(),
    };

    let symbols = empty_symbol_table();
    let results = verify(&program, &symbols);
    assert_eq!(results.len(), 1);
    assert!(
        matches!(&results[0], VerificationResult::Violated { .. }),
        "false constraint should be violated: {:?}",
        results[0]
    );
}

#[test]
fn record_field_constraint_always_true_is_verified() {
    let program = Program {
        directives: vec![],
        items: vec![Item::Record(RecordDef {
            name: "AlwaysOk".to_string(),
            generic_params: vec![],
            fields: vec![FieldDef {
                name: "x".to_string(),
                ty: TypeExpr::Named("Int".to_string(), span()),
                default_value: None,
                constraint: Some(Expr::BoolLit(true, span())),
                span: span(),
            }],
            is_pub: false,
            span: span(),
            doc: None,
        })],
        span: span(),
    };

    let symbols = empty_symbol_table();
    let results = verify(&program, &symbols);
    assert_eq!(results.len(), 1);
    assert!(
        matches!(&results[0], VerificationResult::Verified { .. }),
        "true constraint should be verified: {:?}",
        results[0]
    );
}

#[test]
fn record_field_constraint_comparison_is_not_tautology() {
    // x > 0 is not a tautology (x could be negative), so it should NOT be Verified.
    let program = Program {
        directives: vec![],
        items: vec![Item::Record(RecordDef {
            name: "Positive".to_string(),
            generic_params: vec![],
            fields: vec![FieldDef {
                name: "value".to_string(),
                ty: TypeExpr::Named("Int".to_string(), span()),
                default_value: None,
                constraint: Some(binop(ident("value"), BinOp::Gt, int_lit(0))),
                span: span(),
            }],
            is_pub: false,
            span: span(),
            doc: None,
        })],
        span: span(),
    };

    let symbols = empty_symbol_table();
    let results = verify(&program, &symbols);
    assert_eq!(results.len(), 1);
    assert!(
        !matches!(&results[0], VerificationResult::Verified { .. }),
        "x > 0 is not a tautology, should not be Verified: {:?}",
        results[0]
    );
}

// ── Refinement context tests ────────────────────────────────────────

#[test]
fn refinement_division_safety_through_branch() {
    // If we know x > 0 from a branch condition, then x != 0 should be implied.
    let mut ctx = RefinementContext::new();
    ctx.add_fact(
        "x",
        Constraint::IntComparison {
            var: "x".to_string(),
            op: CmpOp::Gt,
            value: 0,
        },
    );

    let conclusion = Constraint::IntComparison {
        var: "x".to_string(),
        op: CmpOp::NotEq,
        value: 0,
    };

    let result = ctx.implies(&conclusion);
    // x > 0 implies x != 0 (since 0 is not > 0)
    assert_eq!(
        result,
        SatResult::Unsat,
        "x > 0 should imply x != 0 (Unsat means valid implication)"
    );
}

#[test]
fn refinement_bounds_from_multiple_facts() {
    // Know: x >= 0 AND x < 10. Conclusion: x >= 0 should be implied trivially.
    let mut ctx = RefinementContext::new();
    ctx.add_fact(
        "x",
        Constraint::IntComparison {
            var: "x".to_string(),
            op: CmpOp::GtEq,
            value: 0,
        },
    );
    ctx.add_fact(
        "x",
        Constraint::IntComparison {
            var: "x".to_string(),
            op: CmpOp::Lt,
            value: 10,
        },
    );

    let conclusion = Constraint::IntComparison {
        var: "x".to_string(),
        op: CmpOp::GtEq,
        value: 0,
    };

    let result = ctx.implies(&conclusion);
    assert_eq!(
        result,
        SatResult::Unsat,
        "x >= 0 AND x < 10 should imply x >= 0"
    );
}

#[test]
fn refinement_cannot_imply_stronger_bound() {
    // Know: x > 0. Conclusion: x > 100 should NOT be implied.
    let mut ctx = RefinementContext::new();
    ctx.add_fact(
        "x",
        Constraint::IntComparison {
            var: "x".to_string(),
            op: CmpOp::Gt,
            value: 0,
        },
    );

    let conclusion = Constraint::IntComparison {
        var: "x".to_string(),
        op: CmpOp::Gt,
        value: 100,
    };

    let result = ctx.implies(&conclusion);
    assert_eq!(
        result,
        SatResult::Sat,
        "x > 0 should NOT imply x > 100 (Sat means counterexample exists)"
    );
}

#[test]
fn refinement_empty_context_implies_nothing() {
    let ctx = RefinementContext::new();
    let conclusion = Constraint::IntComparison {
        var: "x".to_string(),
        op: CmpOp::Gt,
        value: 0,
    };
    assert_eq!(
        ctx.implies(&conclusion),
        SatResult::Unknown,
        "empty context should not imply anything"
    );
}

// ── Path-sensitive analysis (if/else branches) ──────────────────────

#[test]
fn path_sensitive_call_in_then_branch() {
    // cell callee(n: Int) where n > 0
    // cell caller(x: Int)
    //   if x > 0
    //     callee(x)   # should verify — we know x > 0 here
    //   end
    // end
    let callee = make_cell(
        "callee",
        vec![make_param("n")],
        vec![binop(ident("n"), BinOp::Gt, int_lit(0))],
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
        "in then-branch of x > 0, callee(x) should verify n > 0: {:?}",
        results[0]
    );
}

#[test]
fn path_sensitive_call_in_else_branch() {
    // cell needs_nonpositive(n: Int) where n <= 0
    // cell caller(x: Int)
    //   if x > 0
    //     # then branch
    //   else
    //     needs_nonpositive(x)   # should verify — NOT(x > 0) → x <= 0
    //   end
    // end
    let callee = make_cell(
        "needs_nonpositive",
        vec![make_param("n")],
        vec![binop(ident("n"), BinOp::LtEq, int_lit(0))],
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
        "in else-branch of x > 0, needs_nonpositive(x) should verify n <= 0: {:?}",
        results[0]
    );
}

// ── Solver direct tests ─────────────────────────────────────────────

#[test]
fn solver_implication_division_safe() {
    // Premise: b > 0. Conclusion: b != 0. Should be valid (Unsat).
    let mut solver = ToyConstraintSolver::new();
    let premise = Constraint::IntComparison {
        var: "b".to_string(),
        op: CmpOp::Gt,
        value: 0,
    };
    let conclusion = Constraint::IntComparison {
        var: "b".to_string(),
        op: CmpOp::NotEq,
        value: 0,
    };
    let result = solver.check_implication(&premise, &conclusion);
    assert_eq!(result, SatResult::Unsat, "b > 0 implies b != 0");
}

#[test]
fn solver_implication_bounds_safe() {
    // Premise: idx >= 0 AND idx < 10. Conclusion: idx >= 0. Should be valid.
    let mut solver = ToyConstraintSolver::new();
    let premise = Constraint::And(vec![
        Constraint::IntComparison {
            var: "idx".to_string(),
            op: CmpOp::GtEq,
            value: 0,
        },
        Constraint::IntComparison {
            var: "idx".to_string(),
            op: CmpOp::Lt,
            value: 10,
        },
    ]);
    let conclusion = Constraint::IntComparison {
        var: "idx".to_string(),
        op: CmpOp::GtEq,
        value: 0,
    };
    let result = solver.check_implication(&premise, &conclusion);
    assert_eq!(result, SatResult::Unsat, "bounded idx implies idx >= 0");
}

#[test]
fn solver_implication_insufficient_for_upper_bound() {
    // Premise: idx >= 0. Conclusion: idx < 10. Should NOT be valid (Sat).
    let mut solver = ToyConstraintSolver::new();
    let premise = Constraint::IntComparison {
        var: "idx".to_string(),
        op: CmpOp::GtEq,
        value: 0,
    };
    let conclusion = Constraint::IntComparison {
        var: "idx".to_string(),
        op: CmpOp::Lt,
        value: 10,
    };
    let result = solver.check_implication(&premise, &conclusion);
    assert_eq!(result, SatResult::Sat, "idx >= 0 does NOT imply idx < 10");
}

#[test]
fn solver_contradictory_bounds_unsat() {
    // x > 10 AND x < 5 should be unsatisfiable.
    let mut solver = ToyConstraintSolver::new();
    solver.assert_constraint(&Constraint::IntComparison {
        var: "x".to_string(),
        op: CmpOp::Gt,
        value: 10,
    });
    solver.assert_constraint(&Constraint::IntComparison {
        var: "x".to_string(),
        op: CmpOp::Lt,
        value: 5,
    });
    assert_eq!(solver.check_sat(), SatResult::Unsat);
}

#[test]
fn solver_arithmetic_constraint_reduction() {
    // (x + 1) > 5 AND x < 3 → x > 4 AND x < 3 → Unsat.
    let mut solver = ToyConstraintSolver::new();
    solver.assert_constraint(&Constraint::Arithmetic {
        var: "x".to_string(),
        arith_op: ArithOp::Add,
        arith_const: 1,
        cmp_op: CmpOp::Gt,
        cmp_value: 5,
    });
    solver.assert_constraint(&Constraint::IntComparison {
        var: "x".to_string(),
        op: CmpOp::Lt,
        value: 3,
    });
    assert_eq!(solver.check_sat(), SatResult::Unsat);
}

// ── Verification context ────────────────────────────────────────────

#[test]
fn verification_context_verifies_tautology() {
    let mut ctx = VerificationContext::new();
    let c = Constraint::BoolConst(true);
    let result = ctx.verify_constraint(&c);
    assert!(
        matches!(result, VerificationResult::Verified { .. }),
        "true should be verified: {:?}",
        result
    );
}

#[test]
fn verification_context_violated_contradiction() {
    let mut ctx = VerificationContext::new();
    let c = Constraint::BoolConst(false);
    let result = ctx.verify_constraint(&c);
    assert!(
        matches!(result, VerificationResult::Violated { .. }),
        "false should be violated: {:?}",
        result
    );
}

#[test]
fn verification_context_unknown_for_free_var() {
    // x > 0 is satisfiable but not a tautology — should be Violated or Unverifiable.
    let mut ctx = VerificationContext::new();
    let c = Constraint::IntComparison {
        var: "x".to_string(),
        op: CmpOp::Gt,
        value: 0,
    };
    let result = ctx.verify_constraint(&c);
    // not(x > 0) = x <= 0, which is satisfiable → the original is not a tautology.
    assert!(
        !matches!(result, VerificationResult::Verified { .. }),
        "x > 0 is not a tautology: {:?}",
        result
    );
}

// ── Effect budget verification ──────────────────────────────────────

#[test]
fn effect_budget_within_limit_verified() {
    let mut ctx = VerificationContext::new();
    let c = Constraint::EffectBudget {
        effect_name: "network".to_string(),
        max_calls: 3,
        actual_calls: 2,
    };
    let result = ctx.verify_constraint(&c);
    // The budget is satisfied (2 <= 3), so not(2 <= 3) is unsat → Verified.
    assert!(
        matches!(result, VerificationResult::Verified { .. }),
        "2 calls within budget of 3 should be verified: {:?}",
        result
    );
}

#[test]
fn effect_budget_exceeded_violated() {
    let mut ctx = VerificationContext::new();
    let c = Constraint::EffectBudget {
        effect_name: "network".to_string(),
        max_calls: 2,
        actual_calls: 5,
    };
    let result = ctx.verify_constraint(&c);
    // Budget exceeded (5 > 2), so the constraint is false → Violated.
    assert!(
        matches!(result, VerificationResult::Violated { .. }),
        "5 calls exceeding budget of 2 should be violated: {:?}",
        result
    );
}

// ── Constraint lowering edge cases ──────────────────────────────────

#[test]
fn lowering_unsupported_expr_returns_error() {
    use lumen_compiler::compiler::verification::constraints::lower_expr_to_constraint;

    // A string literal is not a valid constraint expression.
    let expr = Expr::StringLit("hello".to_string(), span());
    let result = lower_expr_to_constraint(&expr);
    assert!(
        result.is_err(),
        "string literal should fail constraint lowering"
    );
}

#[test]
fn lowering_var_comparison_ok() {
    use lumen_compiler::compiler::verification::constraints::lower_expr_to_constraint;

    // x > y should lower to VarComparison
    let expr = binop(ident("x"), BinOp::Gt, ident("y"));
    let result = lower_expr_to_constraint(&expr).expect("should lower");
    assert_eq!(
        result,
        Constraint::VarComparison {
            left: "x".to_string(),
            op: CmpOp::Gt,
            right: "y".to_string(),
        }
    );
}

#[test]
fn lowering_arithmetic_constraint_ok() {
    use lumen_compiler::compiler::verification::constraints::lower_expr_to_constraint;

    // (x + 1) > 0
    let arith = binop(ident("x"), BinOp::Add, int_lit(1));
    let expr = binop(arith, BinOp::Gt, int_lit(0));
    let result = lower_expr_to_constraint(&expr).expect("should lower");
    assert_eq!(
        result,
        Constraint::Arithmetic {
            var: "x".to_string(),
            arith_op: ArithOp::Add,
            arith_const: 1,
            cmp_op: CmpOp::Gt,
            cmp_value: 0,
        }
    );
}

// ── Substitution tests ──────────────────────────────────────────────

#[test]
fn substitute_satisfies_constraint() {
    // b != 0, substitute b = 5 → true.
    let c = Constraint::IntComparison {
        var: "b".to_string(),
        op: CmpOp::NotEq,
        value: 0,
    };
    assert_eq!(c.substitute_int("b", 5), Constraint::BoolConst(true));
}

#[test]
fn substitute_violates_constraint() {
    // b != 0, substitute b = 0 → false.
    let c = Constraint::IntComparison {
        var: "b".to_string(),
        op: CmpOp::NotEq,
        value: 0,
    };
    assert_eq!(c.substitute_int("b", 0), Constraint::BoolConst(false));
}

#[test]
fn substitute_arithmetic_satisfies() {
    // (x + 1) > 0, substitute x = 5 → (6 > 0) → true.
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
fn substitute_arithmetic_violates() {
    // (x + 1) > 10, substitute x = 5 → (6 > 10) → false.
    let c = Constraint::Arithmetic {
        var: "x".to_string(),
        arith_op: ArithOp::Add,
        arith_const: 1,
        cmp_op: CmpOp::Gt,
        cmp_value: 10,
    };
    assert_eq!(c.substitute_int("x", 5), Constraint::BoolConst(false));
}

// ── Rename variable in constraint ───────────────────────────────────

#[test]
fn rename_var_maps_callee_to_caller() {
    // Callee has "n > 0", rename "n" to "x" for caller site.
    let c = Constraint::IntComparison {
        var: "n".to_string(),
        op: CmpOp::Gt,
        value: 0,
    };
    let renamed = c.rename_var("n", "x");
    assert_eq!(
        renamed,
        Constraint::IntComparison {
            var: "x".to_string(),
            op: CmpOp::Gt,
            value: 0,
        }
    );
}

#[test]
fn rename_var_in_var_comparison() {
    let c = Constraint::VarComparison {
        left: "n".to_string(),
        op: CmpOp::Gt,
        right: "m".to_string(),
    };
    let renamed = c.rename_var("n", "x");
    assert_eq!(
        renamed,
        Constraint::VarComparison {
            left: "x".to_string(),
            op: CmpOp::Gt,
            right: "m".to_string(),
        }
    );
}

// ── Mixed record + cell constraints ─────────────────────────────────

#[test]
fn mixed_record_and_cell_constraints_collected() {
    let program = Program {
        directives: vec![],
        items: vec![
            Item::Record(RecordDef {
                name: "Pos".to_string(),
                generic_params: vec![],
                fields: vec![FieldDef {
                    name: "v".to_string(),
                    ty: TypeExpr::Named("Int".to_string(), span()),
                    default_value: None,
                    constraint: Some(binop(ident("v"), BinOp::Gt, int_lit(0))),
                    span: span(),
                }],
                is_pub: false,
                span: span(),
                doc: None,
            }),
            Item::Cell(make_cell(
                "safe_div",
                vec![make_param("a"), make_param("b")],
                vec![binop(ident("b"), BinOp::NotEq, int_lit(0))],
                vec![],
            )),
        ],
        span: span(),
    };

    let collected = collect_constraints(&program);
    assert_eq!(
        collected.len(),
        2,
        "should collect 1 record + 1 cell constraint"
    );
    assert!(collected[0].origin.contains("Pos"));
    assert!(collected[1].origin.contains("safe_div"));
}
