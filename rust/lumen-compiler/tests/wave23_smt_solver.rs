//! Comprehensive tests for the SMT solver abstraction layer (T037).

use lumen_compiler::compiler::verification::constraints::{ArithOp, CmpOp, Constraint};
use lumen_compiler::compiler::verification::smt_solver::*;

// ════════════════════════════════════════════════════════════════════
// §1  SmtExpr construction and SMT-LIB2 display
// ════════════════════════════════════════════════════════════════════

#[test]
fn wave23_smt_expr_int_const_display() {
    assert_eq!(SmtExpr::IntConst(42).to_smtlib2(), "42");
    assert_eq!(SmtExpr::IntConst(0).to_smtlib2(), "0");
    assert_eq!(SmtExpr::IntConst(-7).to_smtlib2(), "(- 7)");
}

#[test]
fn wave23_smt_expr_bool_const_display() {
    assert_eq!(SmtExpr::BoolConst(true).to_smtlib2(), "true");
    assert_eq!(SmtExpr::BoolConst(false).to_smtlib2(), "false");
}

#[test]
fn wave23_smt_expr_float_const_display() {
    let s = SmtExpr::FloatConst(3.14).to_smtlib2();
    assert!(s.contains("3.14"), "got: {}", s);
    let neg = SmtExpr::FloatConst(-2.5).to_smtlib2();
    assert!(neg.starts_with("(- "), "got: {}", neg);
}

#[test]
fn wave23_smt_expr_string_const_display() {
    let s = SmtExpr::StringConst("hello".to_string()).to_smtlib2();
    assert_eq!(s, "\"hello\"");
}

#[test]
fn wave23_smt_expr_var_display() {
    let v = SmtExpr::Var("x".to_string(), SmtSort::Int);
    assert_eq!(v.to_smtlib2(), "x");
}

#[test]
fn wave23_smt_expr_arithmetic_display() {
    let x = || Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int));
    let one = || Box::new(SmtExpr::IntConst(1));

    assert_eq!(SmtExpr::Add(x(), one()).to_smtlib2(), "(+ x 1)");
    assert_eq!(SmtExpr::Sub(x(), one()).to_smtlib2(), "(- x 1)");
    assert_eq!(SmtExpr::Mul(x(), one()).to_smtlib2(), "(* x 1)");
    assert_eq!(SmtExpr::Div(x(), one()).to_smtlib2(), "(div x 1)");
    assert_eq!(SmtExpr::Mod(x(), one()).to_smtlib2(), "(mod x 1)");
    assert_eq!(SmtExpr::Neg(x()).to_smtlib2(), "(- x)");
}

#[test]
fn wave23_smt_expr_comparison_display() {
    let x = || Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int));
    let five = || Box::new(SmtExpr::IntConst(5));

    assert_eq!(SmtExpr::Eq(x(), five()).to_smtlib2(), "(= x 5)");
    assert_eq!(SmtExpr::Ne(x(), five()).to_smtlib2(), "(not (= x 5))");
    assert_eq!(SmtExpr::Lt(x(), five()).to_smtlib2(), "(< x 5)");
    assert_eq!(SmtExpr::Le(x(), five()).to_smtlib2(), "(<= x 5)");
    assert_eq!(SmtExpr::Gt(x(), five()).to_smtlib2(), "(> x 5)");
    assert_eq!(SmtExpr::Ge(x(), five()).to_smtlib2(), "(>= x 5)");
}

#[test]
fn wave23_smt_expr_logical_display() {
    let t = SmtExpr::BoolConst(true);
    let f = SmtExpr::BoolConst(false);

    assert_eq!(
        SmtExpr::And(vec![t.clone(), f.clone()]).to_smtlib2(),
        "(and true false)"
    );
    assert_eq!(
        SmtExpr::Or(vec![t.clone(), f.clone()]).to_smtlib2(),
        "(or true false)"
    );
    assert_eq!(SmtExpr::Not(Box::new(t.clone())).to_smtlib2(), "(not true)");
    assert_eq!(
        SmtExpr::Implies(Box::new(t.clone()), Box::new(f.clone())).to_smtlib2(),
        "(=> true false)"
    );
    assert_eq!(
        SmtExpr::Iff(Box::new(t.clone()), Box::new(f.clone())).to_smtlib2(),
        "(= true false)"
    );
}

#[test]
fn wave23_smt_expr_empty_and_or() {
    assert_eq!(SmtExpr::And(vec![]).to_smtlib2(), "true");
    assert_eq!(SmtExpr::Or(vec![]).to_smtlib2(), "false");
}

#[test]
fn wave23_smt_expr_singleton_and_or() {
    let x = SmtExpr::Var("x".to_string(), SmtSort::Bool);
    assert_eq!(SmtExpr::And(vec![x.clone()]).to_smtlib2(), "x");
    assert_eq!(SmtExpr::Or(vec![x.clone()]).to_smtlib2(), "x");
}

#[test]
fn wave23_smt_expr_quantifier_display() {
    let body = SmtExpr::Gt(
        Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
        Box::new(SmtExpr::IntConst(0)),
    );
    let forall = SmtExpr::ForAll(
        vec![("x".to_string(), SmtSort::Int)],
        Box::new(body.clone()),
    );
    assert_eq!(forall.to_smtlib2(), "(forall ((x Int)) (> x 0))");

    let exists = SmtExpr::Exists(vec![("x".to_string(), SmtSort::Int)], Box::new(body));
    assert_eq!(exists.to_smtlib2(), "(exists ((x Int)) (> x 0))");
}

#[test]
fn wave23_smt_expr_array_display() {
    let arr = SmtExpr::Var(
        "a".to_string(),
        SmtSort::Array(Box::new(SmtSort::Int), Box::new(SmtSort::Int)),
    );
    let idx = SmtExpr::IntConst(3);
    let val = SmtExpr::IntConst(42);

    let sel = SmtExpr::ArraySelect(Box::new(arr.clone()), Box::new(idx.clone()));
    assert_eq!(sel.to_smtlib2(), "(select a 3)");

    let store = SmtExpr::ArrayStore(Box::new(arr), Box::new(idx), Box::new(val));
    assert_eq!(store.to_smtlib2(), "(store a 3 42)");
}

#[test]
fn wave23_smt_expr_bv_display() {
    let a = Box::new(SmtExpr::Var("a".to_string(), SmtSort::BitVec(8)));
    let b = Box::new(SmtExpr::Var("b".to_string(), SmtSort::BitVec(8)));

    assert_eq!(
        SmtExpr::BvAnd(a.clone(), b.clone()).to_smtlib2(),
        "(bvand a b)"
    );
    assert_eq!(
        SmtExpr::BvOr(a.clone(), b.clone()).to_smtlib2(),
        "(bvor a b)"
    );
    assert_eq!(
        SmtExpr::BvShiftLeft(a.clone(), b.clone()).to_smtlib2(),
        "(bvshl a b)"
    );
    assert_eq!(
        SmtExpr::BvShiftRight(a.clone(), b.clone()).to_smtlib2(),
        "(bvlshr a b)"
    );
}

#[test]
fn wave23_smt_expr_ite_display() {
    let ite = SmtExpr::Ite(
        Box::new(SmtExpr::BoolConst(true)),
        Box::new(SmtExpr::IntConst(1)),
        Box::new(SmtExpr::IntConst(0)),
    );
    assert_eq!(ite.to_smtlib2(), "(ite true 1 0)");
}

#[test]
fn wave23_smt_expr_apply_display() {
    let app = SmtExpr::Apply(
        "f".to_string(),
        vec![SmtExpr::IntConst(1), SmtExpr::IntConst(2)],
    );
    assert_eq!(app.to_smtlib2(), "(f 1 2)");

    let no_args = SmtExpr::Apply("g".to_string(), vec![]);
    assert_eq!(no_args.to_smtlib2(), "g");
}

// ════════════════════════════════════════════════════════════════════
// §2  SmtSort display
// ════════════════════════════════════════════════════════════════════

#[test]
fn wave23_smt_sort_display() {
    assert_eq!(format!("{}", SmtSort::Bool), "Bool");
    assert_eq!(format!("{}", SmtSort::Int), "Int");
    assert_eq!(format!("{}", SmtSort::Float), "Real");
    assert_eq!(format!("{}", SmtSort::String), "String");
    assert_eq!(format!("{}", SmtSort::BitVec(32)), "(_ BitVec 32)");
    assert_eq!(
        format!(
            "{}",
            SmtSort::Array(Box::new(SmtSort::Int), Box::new(SmtSort::Bool))
        ),
        "(Array Int Bool)"
    );
    assert_eq!(
        format!("{}", SmtSort::Uninterpreted("MyType".to_string())),
        "MyType"
    );
}

// ════════════════════════════════════════════════════════════════════
// §3  SmtTheory display
// ════════════════════════════════════════════════════════════════════

#[test]
fn wave23_smt_theory_display() {
    assert_eq!(format!("{}", SmtTheory::QfLia), "QF_LIA");
    assert_eq!(format!("{}", SmtTheory::QfBv), "QF_BV");
    assert_eq!(format!("{}", SmtTheory::Strings), "STRINGS");
}

// ════════════════════════════════════════════════════════════════════
// §4  BuiltinSmtSolver: satisfiable formulas
// ════════════════════════════════════════════════════════════════════

#[test]
fn wave23_smt_builtin_empty_is_sat() {
    let solver = BuiltinSmtSolver::new();
    assert!(solver.check_sat(&[]).is_sat());
}

#[test]
fn wave23_smt_builtin_true_is_sat() {
    let solver = BuiltinSmtSolver::new();
    assert!(solver.check_sat(&[SmtExpr::BoolConst(true)]).is_sat());
}

#[test]
fn wave23_smt_builtin_simple_comparison_sat() {
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![SmtExpr::Gt(
        Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
        Box::new(SmtExpr::IntConst(0)),
    )];
    assert!(solver.check_sat(&assertions).is_sat());
}

#[test]
fn wave23_smt_builtin_satisfiable_range() {
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![
        SmtExpr::Gt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(0)),
        ),
        SmtExpr::Lt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(10)),
        ),
    ];
    assert!(solver.check_sat(&assertions).is_sat());
}

#[test]
fn wave23_smt_builtin_equality_sat() {
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![
        SmtExpr::Eq(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(5)),
        ),
        SmtExpr::Gt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(0)),
        ),
    ];
    assert!(solver.check_sat(&assertions).is_sat());
}

// ════════════════════════════════════════════════════════════════════
// §5  BuiltinSmtSolver: unsatisfiable formulas
// ════════════════════════════════════════════════════════════════════

#[test]
fn wave23_smt_builtin_false_is_unsat() {
    let solver = BuiltinSmtSolver::new();
    assert!(solver.check_sat(&[SmtExpr::BoolConst(false)]).is_unsat());
}

#[test]
fn wave23_smt_builtin_contradictory_range_unsat() {
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![
        SmtExpr::Gt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(10)),
        ),
        SmtExpr::Lt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(5)),
        ),
    ];
    assert!(solver.check_sat(&assertions).is_unsat());
}

#[test]
fn wave23_smt_builtin_no_integer_in_gap_unsat() {
    // x > 5 and x < 6 — no integer in (5,6)
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![
        SmtExpr::Gt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(5)),
        ),
        SmtExpr::Lt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(6)),
        ),
    ];
    assert!(solver.check_sat(&assertions).is_unsat());
}

#[test]
fn wave23_smt_builtin_equality_out_of_range_unsat() {
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![
        SmtExpr::Eq(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(15)),
        ),
        SmtExpr::Lt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(10)),
        ),
    ];
    assert!(solver.check_sat(&assertions).is_unsat());
}

#[test]
fn wave23_smt_builtin_neq_eliminates_only_option() {
    // x >= 5, x <= 5, x != 5 → Unsat
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![
        SmtExpr::Ge(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(5)),
        ),
        SmtExpr::Le(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(5)),
        ),
        SmtExpr::Ne(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(5)),
        ),
    ];
    assert!(solver.check_sat(&assertions).is_unsat());
}

#[test]
fn wave23_smt_builtin_not_true_unsat() {
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![SmtExpr::Not(Box::new(SmtExpr::BoolConst(true)))];
    assert!(solver.check_sat(&assertions).is_unsat());
}

// ════════════════════════════════════════════════════════════════════
// §6  BuiltinSmtSolver: model extraction
// ════════════════════════════════════════════════════════════════════

#[test]
fn wave23_smt_builtin_model_for_equality() {
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![SmtExpr::Eq(
        Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
        Box::new(SmtExpr::IntConst(42)),
    )];
    let (result, model) = solver.check_sat_with_model(&assertions);
    assert!(result.is_sat());
    let model = model.expect("should have a model");
    match model.get("x") {
        Some(SmtValue::Int(42)) => {} // expected
        other => panic!("expected x = 42, got {:?}", other),
    }
}

#[test]
fn wave23_smt_builtin_model_for_range() {
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![
        SmtExpr::Gt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(0)),
        ),
        SmtExpr::Lt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(10)),
        ),
    ];
    let (result, model) = solver.check_sat_with_model(&assertions);
    assert!(result.is_sat());
    let model = model.expect("should have a model");
    if let Some(SmtValue::Int(v)) = model.get("x") {
        assert!(*v > 0 && *v < 10, "x should be in (0,10), got {}", v);
    } else {
        panic!("expected integer value for x");
    }
}

#[test]
fn wave23_smt_builtin_no_model_for_unsat() {
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![SmtExpr::BoolConst(false)];
    let (result, model) = solver.check_sat_with_model(&assertions);
    assert!(result.is_unsat());
    assert!(model.is_none());
}

// ════════════════════════════════════════════════════════════════════
// §7  BuiltinSmtSolver: push/pop scope management
// ════════════════════════════════════════════════════════════════════

#[test]
fn wave23_smt_builtin_push_pop_basic() {
    let mut solver = BuiltinSmtSolver::new();
    // Add a contradictory assertion behind a push
    solver.push();
    let inner_assertions = vec![SmtExpr::BoolConst(false)];
    // The internal assertions field is separate; push/pop manages it
    assert!(solver.check_sat(&inner_assertions).is_unsat());
    solver.pop();
    // After pop, no persistent assertions
    assert!(solver.check_sat(&[]).is_sat());
}

#[test]
fn wave23_smt_builtin_nested_push_pop() {
    let mut solver = BuiltinSmtSolver::new();
    solver.push();
    solver.push();
    solver.pop();
    solver.pop();
    // Should still work after double push/pop
    assert!(solver.check_sat(&[SmtExpr::BoolConst(true)]).is_sat());
}

#[test]
fn wave23_smt_builtin_reset_clears() {
    let mut solver = BuiltinSmtSolver::new();
    solver.push();
    solver.reset();
    // After reset, everything is clean
    assert!(solver.check_sat(&[SmtExpr::BoolConst(true)]).is_sat());
}

// ════════════════════════════════════════════════════════════════════
// §8  SmtSolverFactory
// ════════════════════════════════════════════════════════════════════

#[test]
fn wave23_smt_factory_create_builtin_works() {
    let solver = SmtSolverFactory::create_builtin();
    assert_eq!(solver.solver_name(), "builtin");
}

#[test]
fn wave23_smt_factory_available_includes_builtin() {
    let available = SmtSolverFactory::available_solvers();
    assert!(available.contains(&"builtin".to_string()));
}

#[test]
fn wave23_smt_factory_best_available_exists() {
    let solver = SmtSolverFactory::create_best_available();
    // Should always return something (at minimum, builtin)
    let name = solver.solver_name();
    assert!(
        name == "builtin" || name == "z3" || name == "cvc5",
        "unexpected solver name: {}",
        name
    );
}

// ════════════════════════════════════════════════════════════════════
// §9  ConstraintTranslator
// ════════════════════════════════════════════════════════════════════

#[test]
fn wave23_smt_translate_bool_const() {
    let c = Constraint::BoolConst(true);
    let expr = ConstraintTranslator::translate(&c);
    assert_eq!(expr, SmtExpr::BoolConst(true));
}

#[test]
fn wave23_smt_translate_int_comparison() {
    let c = Constraint::IntComparison {
        var: "x".to_string(),
        op: CmpOp::Gt,
        value: 0,
    };
    let expr = ConstraintTranslator::translate(&c);
    assert_eq!(
        expr,
        SmtExpr::Gt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(0)),
        )
    );
}

#[test]
fn wave23_smt_translate_float_comparison() {
    let c = Constraint::FloatComparison {
        var: "score".to_string(),
        op: CmpOp::GtEq,
        value: 0.0,
    };
    let expr = ConstraintTranslator::translate(&c);
    assert_eq!(
        expr,
        SmtExpr::Ge(
            Box::new(SmtExpr::Var("score".to_string(), SmtSort::Float)),
            Box::new(SmtExpr::FloatConst(0.0)),
        )
    );
}

#[test]
fn wave23_smt_translate_var_comparison() {
    let c = Constraint::VarComparison {
        left: "x".to_string(),
        op: CmpOp::Lt,
        right: "y".to_string(),
    };
    let expr = ConstraintTranslator::translate(&c);
    assert_eq!(
        expr,
        SmtExpr::Lt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::Var("y".to_string(), SmtSort::Int)),
        )
    );
}

#[test]
fn wave23_smt_translate_and() {
    let c = Constraint::And(vec![
        Constraint::BoolConst(true),
        Constraint::BoolConst(false),
    ]);
    let expr = ConstraintTranslator::translate(&c);
    assert_eq!(
        expr,
        SmtExpr::And(vec![SmtExpr::BoolConst(true), SmtExpr::BoolConst(false)])
    );
}

#[test]
fn wave23_smt_translate_or() {
    let c = Constraint::Or(vec![
        Constraint::BoolConst(true),
        Constraint::BoolConst(false),
    ]);
    let expr = ConstraintTranslator::translate(&c);
    assert_eq!(
        expr,
        SmtExpr::Or(vec![SmtExpr::BoolConst(true), SmtExpr::BoolConst(false)])
    );
}

#[test]
fn wave23_smt_translate_not() {
    let c = Constraint::Not(Box::new(Constraint::BoolConst(true)));
    let expr = ConstraintTranslator::translate(&c);
    assert_eq!(expr, SmtExpr::Not(Box::new(SmtExpr::BoolConst(true))));
}

#[test]
fn wave23_smt_translate_arithmetic() {
    let c = Constraint::Arithmetic {
        var: "x".to_string(),
        arith_op: ArithOp::Add,
        arith_const: 1,
        cmp_op: CmpOp::Gt,
        cmp_value: 0,
    };
    let expr = ConstraintTranslator::translate(&c);
    assert_eq!(
        expr,
        SmtExpr::Gt(
            Box::new(SmtExpr::Add(
                Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
                Box::new(SmtExpr::IntConst(1)),
            )),
            Box::new(SmtExpr::IntConst(0)),
        )
    );
}

#[test]
fn wave23_smt_translate_effect_budget() {
    let c = Constraint::EffectBudget {
        effect_name: "network".to_string(),
        max_calls: 3,
        actual_calls: 2,
    };
    let expr = ConstraintTranslator::translate(&c);
    // actual_calls <= max_calls → 2 <= 3
    assert_eq!(
        expr,
        SmtExpr::Le(
            Box::new(SmtExpr::IntConst(2)),
            Box::new(SmtExpr::IntConst(3)),
        )
    );
}

#[test]
fn wave23_smt_translate_bool_var() {
    let c = Constraint::BoolVar("is_valid".to_string());
    let expr = ConstraintTranslator::translate(&c);
    assert_eq!(expr, SmtExpr::Var("is_valid".to_string(), SmtSort::Bool));
}

#[test]
fn wave23_smt_translate_all() {
    let constraints = vec![
        Constraint::BoolConst(true),
        Constraint::IntComparison {
            var: "x".to_string(),
            op: CmpOp::Gt,
            value: 0,
        },
    ];
    let exprs = ConstraintTranslator::translate_all(&constraints);
    assert_eq!(exprs.len(), 2);
    assert_eq!(exprs[0], SmtExpr::BoolConst(true));
}

// ════════════════════════════════════════════════════════════════════
// §10  SMT-LIB2 serialization (full script)
// ════════════════════════════════════════════════════════════════════

#[test]
fn wave23_smt_smtlib2_script_generation() {
    let assertions = vec![
        SmtExpr::Gt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(0)),
        ),
        SmtExpr::Lt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(100)),
        ),
    ];
    let script = generate_smtlib2_script(&assertions);
    assert!(script.contains("(set-logic ALL)"));
    assert!(script.contains("(declare-const x Int)"));
    assert!(script.contains("(assert (> x 0))"));
    assert!(script.contains("(assert (< x 100))"));
    assert!(script.contains("(check-sat)"));
}

#[test]
fn wave23_smt_smtlib2_multiple_vars() {
    let assertions = vec![SmtExpr::And(vec![
        SmtExpr::Gt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::Var("y".to_string(), SmtSort::Int)),
        ),
        SmtExpr::Lt(
            Box::new(SmtExpr::Var("y".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(10)),
        ),
    ])];
    let script = generate_smtlib2_script(&assertions);
    assert!(script.contains("(declare-const x Int)"));
    assert!(script.contains("(declare-const y Int)"));
}

// ════════════════════════════════════════════════════════════════════
// §11  Complex nested expressions
// ════════════════════════════════════════════════════════════════════

#[test]
fn wave23_smt_builtin_nested_and_sat() {
    let solver = BuiltinSmtSolver::new();
    let inner_and = SmtExpr::And(vec![
        SmtExpr::Gt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(0)),
        ),
        SmtExpr::Lt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(10)),
        ),
    ]);
    let assertions = vec![inner_and, SmtExpr::BoolConst(true)];
    assert!(solver.check_sat(&assertions).is_sat());
}

#[test]
fn wave23_smt_builtin_or_with_one_sat_branch() {
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![SmtExpr::Or(vec![
        SmtExpr::BoolConst(false),
        SmtExpr::BoolConst(true),
    ])];
    assert!(solver.check_sat(&assertions).is_sat());
}

#[test]
fn wave23_smt_builtin_or_all_unsat() {
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![SmtExpr::Or(vec![
        SmtExpr::BoolConst(false),
        SmtExpr::BoolConst(false),
    ])];
    assert!(solver.check_sat(&assertions).is_unsat());
}

#[test]
fn wave23_smt_builtin_not_false_is_sat() {
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![SmtExpr::Not(Box::new(SmtExpr::BoolConst(false)))];
    assert!(solver.check_sat(&assertions).is_sat());
}

#[test]
fn wave23_smt_builtin_negated_comparison_in_conjunction() {
    // not(x > 5) and x >= 0 → x <= 5 and x >= 0 → Sat
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![
        SmtExpr::Not(Box::new(SmtExpr::Gt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(5)),
        ))),
        SmtExpr::Ge(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(0)),
        ),
    ];
    assert!(solver.check_sat(&assertions).is_sat());
}

#[test]
fn wave23_smt_builtin_negated_comparison_contradiction() {
    // not(x <= 5) and x < 3 → x > 5 and x < 3 → Unsat
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![
        SmtExpr::Not(Box::new(SmtExpr::Le(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(5)),
        ))),
        SmtExpr::Lt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(3)),
        ),
    ];
    assert!(solver.check_sat(&assertions).is_unsat());
}

// ════════════════════════════════════════════════════════════════════
// §12  Theory support queries
// ════════════════════════════════════════════════════════════════════

#[test]
fn wave23_smt_builtin_theory_support() {
    let solver = BuiltinSmtSolver::new();
    assert!(solver.supports_theory(SmtTheory::QfLia));
    assert!(solver.supports_theory(SmtTheory::QfLra));
    assert!(!solver.supports_theory(SmtTheory::QfBv));
    assert!(!solver.supports_theory(SmtTheory::QfAx));
    assert!(!solver.supports_theory(SmtTheory::QfNia));
    assert!(!solver.supports_theory(SmtTheory::Lia));
    assert!(!solver.supports_theory(SmtTheory::Arrays));
    assert!(!solver.supports_theory(SmtTheory::Strings));
}

// ════════════════════════════════════════════════════════════════════
// §13  Edge cases and unknown handling
// ════════════════════════════════════════════════════════════════════

#[test]
fn wave23_smt_builtin_quantifier_returns_unknown() {
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![SmtExpr::ForAll(
        vec![("x".to_string(), SmtSort::Int)],
        Box::new(SmtExpr::Gt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(0)),
        )),
    )];
    let result = solver.check_sat(&assertions);
    assert!(matches!(result, SmtResult::Unknown(_)));
}

#[test]
fn wave23_smt_builtin_bitvector_returns_unknown() {
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![SmtExpr::BvAnd(
        Box::new(SmtExpr::Var("a".to_string(), SmtSort::BitVec(8))),
        Box::new(SmtExpr::Var("b".to_string(), SmtSort::BitVec(8))),
    )];
    let result = solver.check_sat(&assertions);
    assert!(matches!(result, SmtResult::Unknown(_)));
}

#[test]
fn wave23_smt_builtin_array_returns_unknown() {
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![SmtExpr::ArraySelect(
        Box::new(SmtExpr::Var(
            "a".to_string(),
            SmtSort::Array(Box::new(SmtSort::Int), Box::new(SmtSort::Int)),
        )),
        Box::new(SmtExpr::IntConst(0)),
    )];
    let result = solver.check_sat(&assertions);
    assert!(matches!(result, SmtResult::Unknown(_)));
}

#[test]
fn wave23_smt_builtin_constant_comparison_sat() {
    let solver = BuiltinSmtSolver::new();
    // 5 > 3 → Sat
    let assertions = vec![SmtExpr::Gt(
        Box::new(SmtExpr::IntConst(5)),
        Box::new(SmtExpr::IntConst(3)),
    )];
    assert!(solver.check_sat(&assertions).is_sat());
}

#[test]
fn wave23_smt_builtin_constant_comparison_unsat() {
    let solver = BuiltinSmtSolver::new();
    // 3 > 5 → Unsat
    let assertions = vec![SmtExpr::Gt(
        Box::new(SmtExpr::IntConst(3)),
        Box::new(SmtExpr::IntConst(5)),
    )];
    assert!(solver.check_sat(&assertions).is_unsat());
}

// ════════════════════════════════════════════════════════════════════
// §14  SmtResult and SmtValue utilities
// ════════════════════════════════════════════════════════════════════

#[test]
fn wave23_smt_result_helpers() {
    assert!(SmtResult::Sat.is_sat());
    assert!(!SmtResult::Sat.is_unsat());
    assert!(SmtResult::Unsat.is_unsat());
    assert!(!SmtResult::Unsat.is_sat());
    assert!(!SmtResult::Unknown("test".to_string()).is_sat());
    assert!(!SmtResult::Timeout.is_sat());
    assert!(!SmtResult::Error("err".to_string()).is_sat());
}

#[test]
fn wave23_smt_value_display() {
    assert_eq!(format!("{}", SmtValue::Int(42)), "42");
    assert_eq!(format!("{}", SmtValue::Bool(true)), "true");
    assert_eq!(format!("{}", SmtValue::Float(3.14)), "3.14");
    assert_eq!(format!("{}", SmtValue::String("hi".to_string())), "\"hi\"");
}

#[test]
fn wave23_smt_model_operations() {
    let mut model = SmtModel::new();
    assert!(model.get("x").is_none());
    model.assignments.insert("x".to_string(), SmtValue::Int(5));
    assert_eq!(model.get("x"), Some(&SmtValue::Int(5)));
}

// ════════════════════════════════════════════════════════════════════
// §15  Variable collection from expressions
// ════════════════════════════════════════════════════════════════════

#[test]
fn wave23_smt_collect_vars() {
    let expr = SmtExpr::And(vec![
        SmtExpr::Gt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(0)),
        ),
        SmtExpr::Lt(
            Box::new(SmtExpr::Var("y".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
        ),
    ]);
    let vars = expr.collect_vars();
    assert_eq!(vars.len(), 2); // x and y (x appears twice but deduped)
    let names: Vec<_> = vars.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"x"));
    assert!(names.contains(&"y"));
}

#[test]
fn wave23_smt_collect_vars_no_vars() {
    let expr = SmtExpr::And(vec![SmtExpr::BoolConst(true), SmtExpr::IntConst(5)]);
    let vars = expr.collect_vars();
    assert!(vars.is_empty());
}

// ════════════════════════════════════════════════════════════════════
// §16  Builtin solver with linear arithmetic
// ════════════════════════════════════════════════════════════════════

#[test]
fn wave23_smt_builtin_linear_arith_sat() {
    // (x + 1) > 0 and x < 100 → Sat
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![
        SmtExpr::Gt(
            Box::new(SmtExpr::Add(
                Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
                Box::new(SmtExpr::IntConst(1)),
            )),
            Box::new(SmtExpr::IntConst(0)),
        ),
        SmtExpr::Lt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(100)),
        ),
    ];
    assert!(solver.check_sat(&assertions).is_sat());
}

#[test]
fn wave23_smt_builtin_linear_arith_unsat() {
    // (x + 1) > 10 and x < 5 → x > 9 and x < 5 → Unsat
    let solver = BuiltinSmtSolver::new();
    let assertions = vec![
        SmtExpr::Gt(
            Box::new(SmtExpr::Add(
                Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
                Box::new(SmtExpr::IntConst(1)),
            )),
            Box::new(SmtExpr::IntConst(10)),
        ),
        SmtExpr::Lt(
            Box::new(SmtExpr::Var("x".to_string(), SmtSort::Int)),
            Box::new(SmtExpr::IntConst(5)),
        ),
    ];
    assert!(solver.check_sat(&assertions).is_unsat());
}

// ════════════════════════════════════════════════════════════════════
// §17  Z3/CVC5 graceful handling when not installed
// ════════════════════════════════════════════════════════════════════

#[test]
fn wave23_smt_z3_availability_check() {
    // This test always passes; it just exercises the availability check
    let _available = Z3ProcessSolver::is_available();
}

#[test]
fn wave23_smt_cvc5_availability_check() {
    let _available = Cvc5ProcessSolver::is_available();
}

// ════════════════════════════════════════════════════════════════════
// §18  Translate then solve round-trip
// ════════════════════════════════════════════════════════════════════

#[test]
fn wave23_smt_translate_and_solve_sat() {
    let constraint = Constraint::IntComparison {
        var: "x".to_string(),
        op: CmpOp::Gt,
        value: 0,
    };
    let expr = ConstraintTranslator::translate(&constraint);
    let solver = BuiltinSmtSolver::new();
    assert!(solver.check_sat(&[expr]).is_sat());
}

#[test]
fn wave23_smt_translate_and_solve_unsat() {
    let constraint = Constraint::And(vec![
        Constraint::IntComparison {
            var: "x".to_string(),
            op: CmpOp::Gt,
            value: 10,
        },
        Constraint::IntComparison {
            var: "x".to_string(),
            op: CmpOp::Lt,
            value: 5,
        },
    ]);
    let expr = ConstraintTranslator::translate(&constraint);
    let solver = BuiltinSmtSolver::new();
    assert!(solver.check_sat(&[expr]).is_unsat());
}

#[test]
fn wave23_smt_translate_effect_budget_and_solve() {
    let within = Constraint::EffectBudget {
        effect_name: "net".to_string(),
        max_calls: 3,
        actual_calls: 2,
    };
    let exceeded = Constraint::EffectBudget {
        effect_name: "net".to_string(),
        max_calls: 1,
        actual_calls: 5,
    };
    let solver = BuiltinSmtSolver::new();
    assert!(solver
        .check_sat(&[ConstraintTranslator::translate(&within)])
        .is_sat());
    assert!(solver
        .check_sat(&[ConstraintTranslator::translate(&exceeded)])
        .is_unsat());
}
