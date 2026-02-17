//! Wave 20 — T052: Fuzz type checker with constraints
//!
//! Manual fuzzer that generates random programs with `where` clauses and
//! feeds them to the compiler, asserting no panics. Since proptest is not
//! available as a dev dependency, we use a simple deterministic pseudo-random
//! generator (LCG) to produce varied inputs.

use lumen_compiler::compile;
use lumen_compiler::compiler::verification::constraints::{
    lower_expr_to_constraint, ArithOp, CmpOp, Constraint,
};
use lumen_compiler::compiler::verification::solver::{Solver, ToyConstraintSolver};
use lumen_compiler::compiler::verification::{
    collect_constraints, verify, verify_cell_contracts, VerificationContext,
};

fn markdown_from_code(source: &str) -> String {
    format!("# fuzz-test\n\n```lumen\n{}\n```\n", source.trim())
}

/// Simple deterministic LCG pseudo-random number generator.
struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        // LCG parameters from Numerical Recipes
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    #[allow(dead_code)]
    fn next_u32(&mut self) -> u32 {
        (self.next() >> 32) as u32
    }

    fn next_range(&mut self, lo: i64, hi: i64) -> i64 {
        let range = (hi - lo + 1) as u64;
        lo + (self.next() % range) as i64
    }

    fn next_bool(&mut self) -> bool {
        self.next() & 1 == 1
    }

    #[allow(dead_code)]
    fn choose<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        let idx = (self.next() as usize) % items.len();
        &items[idx]
    }
}

// ── Program generators ──────────────────────────────────────────────

fn random_type(rng: &mut Rng) -> &'static str {
    let types = ["Int", "Float", "String", "Bool"];
    types[(rng.next() as usize) % types.len()]
}

fn random_comparison_op(rng: &mut Rng) -> &'static str {
    let ops = [">", ">=", "<", "<=", "==", "!="];
    ops[(rng.next() as usize) % ops.len()]
}

fn random_logical_op(rng: &mut Rng) -> &'static str {
    let ops = ["and", "or"];
    ops[(rng.next() as usize) % ops.len()]
}

fn random_int_literal(rng: &mut Rng) -> i64 {
    rng.next_range(-100, 100)
}

fn random_float_literal(rng: &mut Rng) -> f64 {
    let int_part = rng.next_range(-100, 100);
    let frac = (rng.next() % 100) as f64 / 100.0;
    int_part as f64 + frac
}

fn generate_where_clause(rng: &mut Rng, param_name: &str, ty: &str) -> String {
    let op = random_comparison_op(rng);
    match ty {
        "Int" => {
            let val = random_int_literal(rng);
            if rng.next_bool() {
                // Simple comparison
                format!("{} {} {}", param_name, op, val)
            } else {
                // Compound comparison
                let op2 = random_comparison_op(rng);
                let val2 = random_int_literal(rng);
                let logical = random_logical_op(rng);
                format!(
                    "{} {} {} {} {} {} {}",
                    param_name, op, val, logical, param_name, op2, val2
                )
            }
        }
        "Float" => {
            let val = random_float_literal(rng);
            format!("{} {} {:.2}", param_name, op, val)
        }
        _ => {
            // For non-numeric types, just use a trivial constraint
            format!("{} != {}", param_name, param_name)
        }
    }
}

fn generate_random_record(rng: &mut Rng, idx: usize) -> String {
    let name = format!("FuzzRec{}", idx);
    let num_fields = (rng.next() as usize % 3) + 1;
    let mut fields = Vec::new();
    let mut where_parts = Vec::new();

    for i in 0..num_fields {
        let field_name = format!("f{}", i);
        let ty = random_type(rng);
        fields.push(format!("  {}: {}", field_name, ty));

        // Some fields get where clauses (50% chance)
        if rng.next_bool() && (ty == "Int" || ty == "Float") {
            where_parts.push(generate_where_clause(rng, &field_name, ty));
        }
    }

    let fields_str = fields.join(",\n");
    if where_parts.is_empty() {
        format!("record {}(\n{}\n) end", name, fields_str)
    } else {
        let where_str = where_parts.join(" and ");
        format!(
            "record {}(\n{}\n) where {} end",
            name, fields_str, where_str
        )
    }
}

fn generate_random_cell(rng: &mut Rng, idx: usize) -> String {
    let name = format!("fuzz_cell_{}", idx);
    let num_params = (rng.next() as usize % 3) + 1;
    let mut params = Vec::new();
    let mut where_parts = Vec::new();
    let mut param_info = Vec::new();

    for i in 0..num_params {
        let param_name = format!("p{}", i);
        let ty = random_type(rng);
        params.push(format!("{}: {}", param_name, ty));
        param_info.push((param_name.clone(), ty.to_string()));

        // Some params get where clauses (40% chance)
        if rng.next() % 5 < 2 && (ty == "Int" || ty == "Float") {
            where_parts.push(generate_where_clause(rng, &param_name, ty));
        }
    }

    let return_type = random_type(rng);
    let params_str = params.join(", ");

    let body = match return_type {
        "Int" => "  0".to_string(),
        "Float" => "  0.0".to_string(),
        "String" => "  \"\"".to_string(),
        "Bool" => "  true".to_string(),
        _ => "  0".to_string(),
    };

    if where_parts.is_empty() {
        format!(
            "cell {}({}) -> {}\n{}\nend",
            name, params_str, return_type, body
        )
    } else {
        let where_str = where_parts.join(" and ");
        format!(
            "cell {}({}) -> {} where {}\n{}\nend",
            name, params_str, return_type, where_str, body
        )
    }
}

fn generate_random_program(rng: &mut Rng) -> String {
    let num_records = rng.next() as usize % 4;
    let num_cells = (rng.next() as usize % 4) + 1; // at least 1 cell

    let mut parts = Vec::new();

    for i in 0..num_records {
        parts.push(generate_random_record(rng, i));
    }
    for i in 0..num_cells {
        parts.push(generate_random_cell(rng, i));
    }

    // Always add a main cell
    parts.push("cell main() -> Int\n  0\nend".to_string());

    parts.join("\n\n")
}

// ── Fuzz tests: compile() should never panic ────────────────────────

#[test]
fn fuzz_random_programs_no_panic() {
    let mut rng = Rng::new(42);

    for i in 0..50 {
        let source = generate_random_program(&mut rng);
        let md = markdown_from_code(&source);

        // The key assertion: compile should not panic, even if it returns an error.
        let result = std::panic::catch_unwind(|| compile(&md));
        assert!(
            result.is_ok(),
            "compile panicked on fuzz program #{}\n--- source ---\n{}",
            i,
            source
        );
    }
}

#[test]
fn fuzz_random_programs_different_seeds() {
    for seed in [0, 1, 7, 13, 42, 99, 137, 255, 1000, 9999] {
        let mut rng = Rng::new(seed);
        let source = generate_random_program(&mut rng);
        let md = markdown_from_code(&source);

        let result = std::panic::catch_unwind(|| compile(&md));
        assert!(
            result.is_ok(),
            "compile panicked on seed {}\n--- source ---\n{}",
            seed,
            source
        );
    }
}

// ── Fuzz tests: verification pipeline should never panic ────────────

#[test]
fn fuzz_verification_no_panic() {
    use lumen_compiler::compiler::ast::*;
    use lumen_compiler::compiler::resolve::SymbolTable;
    use lumen_compiler::compiler::tokens::Span;

    let span = Span {
        start: 0,
        end: 0,
        line: 1,
        col: 1,
    };

    fn make_ident(name: &str, span: Span) -> Expr {
        Expr::Ident(name.to_string(), span)
    }
    fn make_int(v: i64, span: Span) -> Expr {
        Expr::IntLit(v, span)
    }
    fn make_binop(lhs: Expr, op: BinOp, rhs: Expr, span: Span) -> Expr {
        Expr::BinOp(Box::new(lhs), op, Box::new(rhs), span)
    }

    let symbols = SymbolTable {
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
    };

    let ops = [
        BinOp::Gt,
        BinOp::GtEq,
        BinOp::Lt,
        BinOp::LtEq,
        BinOp::Eq,
        BinOp::NotEq,
    ];
    let values: Vec<i64> = vec![-100, -1, 0, 1, 100, i64::MIN, i64::MAX];

    let mut rng = Rng::new(12345);

    for i in 0..100 {
        let op_idx = (rng.next() as usize) % ops.len();
        let val_idx = (rng.next() as usize) % values.len();
        let op = ops[op_idx];
        let val = values[val_idx];

        // Create a record with a random constraint
        let constraint = make_binop(make_ident("value", span), op, make_int(val, span), span);

        // Optionally wrap in And/Or/Not
        let final_constraint = match rng.next() % 4 {
            0 => constraint, // plain
            1 => {
                // And with another comparison
                let val2 = values[(rng.next() as usize) % values.len()];
                let op2 = ops[(rng.next() as usize) % ops.len()];
                make_binop(
                    constraint,
                    BinOp::And,
                    make_binop(make_ident("value", span), op2, make_int(val2, span), span),
                    span,
                )
            }
            2 => {
                // Or with another comparison
                let val2 = values[(rng.next() as usize) % values.len()];
                let op2 = ops[(rng.next() as usize) % ops.len()];
                make_binop(
                    constraint,
                    BinOp::Or,
                    make_binop(make_ident("value", span), op2, make_int(val2, span), span),
                    span,
                )
            }
            _ => {
                // Not
                Expr::UnaryOp(UnaryOp::Not, Box::new(constraint), span)
            }
        };

        let program = Program {
            directives: vec![],
            items: vec![Item::Record(RecordDef {
                name: format!("FuzzRec{}", i),
                generic_params: vec![],
                fields: vec![FieldDef {
                    name: "value".to_string(),
                    ty: TypeExpr::Named("Int".to_string(), span),
                    default_value: None,
                    constraint: Some(final_constraint),
                    span,
                }],
                is_pub: false,
                span,
                doc: None,
            })],
            span,
        };

        // Should never panic
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = collect_constraints(&program);
            let _ = verify(&program, &symbols);
        }));
        assert!(
            result.is_ok(),
            "verification panicked on fuzz iteration {}",
            i
        );
    }
}

// ── Fuzz tests: constraint lowering should never panic ──────────────

#[test]
fn fuzz_constraint_lowering_no_panic() {
    use lumen_compiler::compiler::ast::*;
    use lumen_compiler::compiler::tokens::Span;

    let span = Span {
        start: 0,
        end: 0,
        line: 1,
        col: 1,
    };

    let mut rng = Rng::new(54321);

    let ops = [
        BinOp::Gt,
        BinOp::GtEq,
        BinOp::Lt,
        BinOp::LtEq,
        BinOp::Eq,
        BinOp::NotEq,
        BinOp::And,
        BinOp::Or,
        BinOp::Add,
        BinOp::Sub,
        BinOp::Mul,
    ];

    for _ in 0..200 {
        let op_idx = (rng.next() as usize) % ops.len();
        let op = ops[op_idx];

        let lhs = match rng.next() % 3 {
            0 => Expr::Ident(format!("v{}", rng.next() % 5), span),
            1 => Expr::IntLit(rng.next_range(-1000, 1000), span),
            _ => Expr::BoolLit(rng.next_bool(), span),
        };

        let rhs = match rng.next() % 3 {
            0 => Expr::Ident(format!("v{}", rng.next() % 5), span),
            1 => Expr::IntLit(rng.next_range(-1000, 1000), span),
            _ => Expr::FloatLit(rng.next_range(-100, 100) as f64 + 0.5, span),
        };

        let expr = Expr::BinOp(Box::new(lhs), op, Box::new(rhs), span);

        // Should never panic, even if it returns an error.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = lower_expr_to_constraint(&expr);
        }));
        assert!(result.is_ok(), "lower_expr_to_constraint panicked");
    }
}

// ── Fuzz tests: solver should never panic ───────────────────────────

#[test]
fn fuzz_solver_no_panic() {
    let mut rng = Rng::new(77777);

    let cmp_ops = [
        CmpOp::Gt,
        CmpOp::GtEq,
        CmpOp::Lt,
        CmpOp::LtEq,
        CmpOp::Eq,
        CmpOp::NotEq,
    ];
    let arith_ops = [ArithOp::Add, ArithOp::Sub, ArithOp::Mul];

    for _ in 0..100 {
        let mut solver = ToyConstraintSolver::new();

        let num_constraints = (rng.next() as usize % 5) + 1;
        for _ in 0..num_constraints {
            let var = format!("x{}", rng.next() % 3);
            let val = rng.next_range(-1000, 1000);
            let op = cmp_ops[(rng.next() as usize) % cmp_ops.len()];

            let constraint = match rng.next() % 4 {
                0 => Constraint::IntComparison {
                    var: var.clone(),
                    op,
                    value: val,
                },
                1 => Constraint::BoolConst(rng.next_bool()),
                2 => {
                    let arith_op = arith_ops[(rng.next() as usize) % arith_ops.len()];
                    Constraint::Arithmetic {
                        var: var.clone(),
                        arith_op,
                        arith_const: rng.next_range(-10, 10),
                        cmp_op: op,
                        cmp_value: val,
                    }
                }
                _ => Constraint::VarComparison {
                    left: var.clone(),
                    op,
                    right: format!("y{}", rng.next() % 3),
                },
            };

            solver.assert_constraint(&constraint);
        }

        // Should never panic
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = solver.check_sat();
        }));
        assert!(result.is_ok(), "solver.check_sat() panicked");
    }
}

// ── Fuzz tests: VerificationContext should never panic ───────────────

#[test]
fn fuzz_verification_context_no_panic() {
    let mut rng = Rng::new(99999);

    let cmp_ops = [
        CmpOp::Gt,
        CmpOp::GtEq,
        CmpOp::Lt,
        CmpOp::LtEq,
        CmpOp::Eq,
        CmpOp::NotEq,
    ];

    for _ in 0..50 {
        let mut ctx = VerificationContext::new();
        let var = format!("x{}", rng.next() % 5);
        let op = cmp_ops[(rng.next() as usize) % cmp_ops.len()];
        let val = rng.next_range(-1000, 1000);

        let c = Constraint::IntComparison {
            var: var.clone(),
            op,
            value: val,
        };

        // Wrap in various forms
        let final_c = match rng.next() % 5 {
            0 => c.clone(),
            1 => Constraint::Not(Box::new(c.clone())),
            2 => Constraint::And(vec![c.clone(), Constraint::BoolConst(true)]),
            3 => Constraint::Or(vec![c.clone(), Constraint::BoolConst(false)]),
            _ => {
                let op2 = cmp_ops[(rng.next() as usize) % cmp_ops.len()];
                let val2 = rng.next_range(-1000, 1000);
                Constraint::And(vec![
                    c.clone(),
                    Constraint::IntComparison {
                        var: var.clone(),
                        op: op2,
                        value: val2,
                    },
                ])
            }
        };

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = ctx.verify_constraint(&final_c);
        }));
        assert!(result.is_ok(), "verify_constraint panicked");
    }
}

// ── Fuzz tests: cell contract verification should never panic ───────

#[test]
fn fuzz_cell_contracts_no_panic() {
    use lumen_compiler::compiler::ast::*;
    use lumen_compiler::compiler::tokens::Span;

    let span = Span {
        start: 0,
        end: 0,
        line: 1,
        col: 1,
    };

    let mut rng = Rng::new(31415);

    let ops = [
        BinOp::Gt,
        BinOp::GtEq,
        BinOp::Lt,
        BinOp::LtEq,
        BinOp::Eq,
        BinOp::NotEq,
    ];

    for i in 0..50 {
        let op = ops[(rng.next() as usize) % ops.len()];
        let val = rng.next_range(-100, 100);
        let call_val = rng.next_range(-100, 100);

        let where_clause = Expr::BinOp(
            Box::new(Expr::Ident("n".to_string(), span)),
            op,
            Box::new(Expr::IntLit(val, span)),
            span,
        );

        let callee = CellDef {
            name: format!("callee_{}", i),
            generic_params: vec![],
            params: vec![Param {
                name: "n".to_string(),
                ty: TypeExpr::Named("Int".to_string(), span),
                default_value: None,
                variadic: false,
                span,
            }],
            return_type: None,
            effects: vec![],
            body: vec![],
            is_pub: false,
            is_async: false,
            is_extern: false,
            must_use: false,
            where_clauses: vec![where_clause],
            span,
            doc: None,
        };

        let caller = CellDef {
            name: format!("caller_{}", i),
            generic_params: vec![],
            params: vec![],
            return_type: None,
            effects: vec![],
            body: vec![Stmt::Expr(ExprStmt {
                expr: Expr::Call(
                    Box::new(Expr::Ident(format!("callee_{}", i), span)),
                    vec![CallArg::Positional(Expr::IntLit(call_val, span))],
                    span,
                ),
                span,
            })],
            is_pub: false,
            is_async: false,
            is_extern: false,
            must_use: false,
            where_clauses: vec![],
            span,
            doc: None,
        };

        let program = Program {
            directives: vec![],
            items: vec![Item::Cell(callee), Item::Cell(caller)],
            span,
        };

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = verify_cell_contracts(&program);
        }));
        assert!(
            result.is_ok(),
            "verify_cell_contracts panicked on fuzz iteration {}",
            i
        );
    }
}

// ── Fuzz: substitution should never panic ───────────────────────────

#[test]
fn fuzz_substitution_no_panic() {
    let mut rng = Rng::new(27182);

    let cmp_ops = [
        CmpOp::Gt,
        CmpOp::GtEq,
        CmpOp::Lt,
        CmpOp::LtEq,
        CmpOp::Eq,
        CmpOp::NotEq,
    ];
    let arith_ops = [ArithOp::Add, ArithOp::Sub, ArithOp::Mul];

    let extreme_values: Vec<i64> = vec![i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX];

    for _ in 0..100 {
        let op = cmp_ops[(rng.next() as usize) % cmp_ops.len()];
        let val = extreme_values[(rng.next() as usize) % extreme_values.len()];
        let sub_val = extreme_values[(rng.next() as usize) % extreme_values.len()];

        let constraint = match rng.next() % 3 {
            0 => Constraint::IntComparison {
                var: "x".to_string(),
                op,
                value: val,
            },
            1 => {
                let arith_op = arith_ops[(rng.next() as usize) % arith_ops.len()];
                Constraint::Arithmetic {
                    var: "x".to_string(),
                    arith_op,
                    arith_const: val,
                    cmp_op: op,
                    cmp_value: extreme_values[(rng.next() as usize) % extreme_values.len()],
                }
            }
            _ => Constraint::VarComparison {
                left: "x".to_string(),
                op,
                right: "y".to_string(),
            },
        };

        // Should never panic, even with extreme values
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = constraint.substitute_int("x", sub_val);
        }));
        assert!(result.is_ok(), "substitute_int panicked");
    }
}

// ── Fuzz: RefinementContext should never panic ──────────────────────

#[test]
fn fuzz_refinement_context_no_panic() {
    use lumen_compiler::compiler::verification::refinement::RefinementContext;

    let mut rng = Rng::new(16180);

    let cmp_ops = [
        CmpOp::Gt,
        CmpOp::GtEq,
        CmpOp::Lt,
        CmpOp::LtEq,
        CmpOp::Eq,
        CmpOp::NotEq,
    ];

    for _ in 0..50 {
        let mut ctx = RefinementContext::new();

        // Add random facts
        let num_facts = (rng.next() as usize % 5) + 1;
        for _ in 0..num_facts {
            let var = format!("v{}", rng.next() % 4);
            let op = cmp_ops[(rng.next() as usize) % cmp_ops.len()];
            let val = rng.next_range(-1000, 1000);

            let fact = Constraint::IntComparison {
                var: var.clone(),
                op,
                value: val,
            };
            ctx.refine_from_condition(&fact);
        }

        // Check implication
        let conclusion_var = format!("v{}", rng.next() % 4);
        let conclusion_op = cmp_ops[(rng.next() as usize) % cmp_ops.len()];
        let conclusion_val = rng.next_range(-1000, 1000);
        let conclusion = Constraint::IntComparison {
            var: conclusion_var,
            op: conclusion_op,
            value: conclusion_val,
        };

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = ctx.implies(&conclusion);
        }));
        assert!(result.is_ok(), "implies() panicked");

        // Test merge
        let mut ctx2 = RefinementContext::new();
        let fact2_var = format!("v{}", rng.next() % 4);
        let fact2_op = cmp_ops[(rng.next() as usize) % cmp_ops.len()];
        let fact2 = Constraint::IntComparison {
            var: fact2_var.clone(),
            op: fact2_op,
            value: rng.next_range(-1000, 1000),
        };
        ctx2.refine_from_condition(&fact2);

        let merge_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = RefinementContext::merge(&ctx, &ctx2);
        }));
        assert!(merge_result.is_ok(), "merge() panicked");
    }
}
