//! Wave 20 — T040: Parse `where` clauses into AST
//!
//! Verifies that record field `where` clauses and cell-level `where` clauses
//! are available in the AST for downstream verification.
//! Uses end-to-end `compile()` tests for record field where-clauses (parsed
//! by the parser) and direct AST construction for cell where-clauses (not yet
//! parsed but present in the AST definition).

use lumen_compiler::compile;
use lumen_compiler::compiler::verification::{collect_constraints, verify, VerificationResult};

fn markdown_from_code(source: &str) -> String {
    format!("# wave20-where\n\n```lumen\n{}\n```\n", source.trim())
}

fn assert_compile_ok(id: &str, source: &str) {
    let md = markdown_from_code(source);
    if let Err(err) = compile(&md) {
        panic!(
            "case '{}' failed to compile\n--- source ---\n{}\n--- error ---\n{}",
            id, source, err
        );
    }
}

// ── Record field where-clause parsing (indent-based syntax) ─────────

#[test]
fn record_field_where_gt_zero() {
    assert_compile_ok(
        "record-field-gt-zero",
        r#"
record Positive
  value: Int where value > 0
end
cell main() -> Int
  let p = Positive(value: 1)
  p.value
end
"#,
    );
}

#[test]
fn record_field_where_range() {
    assert_compile_ok(
        "record-field-range",
        r#"
record Percentage
  value: Float where value >= 0.0 and value <= 100.0
end
cell main() -> Float
  let p = Percentage(value: 50.0)
  p.value
end
"#,
    );
}

#[test]
fn record_field_where_not_eq() {
    assert_compile_ok(
        "record-field-not-eq",
        r#"
record NonZero
  value: Int where value != 0
end
cell main() -> Int
  let n = NonZero(value: 42)
  n.value
end
"#,
    );
}

#[test]
fn record_multiple_fields_with_constraints() {
    assert_compile_ok(
        "record-multi-field-constraints",
        r#"
record BoundedPair
  lo: Int where lo >= 0
  hi: Int where hi >= 0
end
cell main() -> Int
  let bp = BoundedPair(lo: 1, hi: 10)
  bp.hi
end
"#,
    );
}

#[test]
fn record_field_where_complex_and_or() {
    assert_compile_ok(
        "record-field-complex-and-or",
        r#"
record Score
  value: Int where value >= 0 and value <= 100
end
cell main() -> Int
  let s = Score(value: 85)
  s.value
end
"#,
    );
}

// ── Cell where-clause tests (AST-based) ─────────────────────────────
// Note: The parser does not yet support `cell ... where` syntax, but the
// CellDef AST node has a `where_clauses` field. These tests construct ASTs
// directly to verify the verification pipeline handles them correctly.

#[test]
fn cell_where_clause_nonzero_divisor_via_ast() {
    use lumen_compiler::compiler::ast::*;
    use lumen_compiler::compiler::tokens::Span;

    let span = Span {
        start: 0,
        end: 0,
        line: 1,
        col: 1,
    };

    let program = Program {
        directives: vec![],
        items: vec![Item::Cell(CellDef {
            name: "divide".to_string(),
            generic_params: vec![],
            params: vec![
                Param {
                    name: "a".to_string(),
                    ty: TypeExpr::Named("Int".to_string(), span),
                    default_value: None,
                    variadic: false,
                    span,
                },
                Param {
                    name: "b".to_string(),
                    ty: TypeExpr::Named("Int".to_string(), span),
                    default_value: None,
                    variadic: false,
                    span,
                },
            ],
            return_type: Some(TypeExpr::Named("Int".to_string(), span)),
            effects: vec![],
            body: vec![],
            is_pub: false,
            is_async: false,
            is_extern: false,
            must_use: false,
            where_clauses: vec![Expr::BinOp(
                Box::new(Expr::Ident("b".to_string(), span)),
                BinOp::NotEq,
                Box::new(Expr::IntLit(0, span)),
                span,
            )],
            span,
            doc: None,
        })],
        span,
    };

    let collected = collect_constraints(&program);
    assert_eq!(collected.len(), 1);
    assert!(collected[0].origin.contains("divide"));
    assert!(collected[0].lowered.is_ok());
}

#[test]
fn cell_where_clause_positive_param_via_ast() {
    use lumen_compiler::compiler::ast::*;
    use lumen_compiler::compiler::tokens::Span;

    let span = Span {
        start: 0,
        end: 0,
        line: 1,
        col: 1,
    };

    let program = Program {
        directives: vec![],
        items: vec![Item::Cell(CellDef {
            name: "sqrt_int".to_string(),
            generic_params: vec![],
            params: vec![Param {
                name: "n".to_string(),
                ty: TypeExpr::Named("Int".to_string(), span),
                default_value: None,
                variadic: false,
                span,
            }],
            return_type: Some(TypeExpr::Named("Int".to_string(), span)),
            effects: vec![],
            body: vec![],
            is_pub: false,
            is_async: false,
            is_extern: false,
            must_use: false,
            where_clauses: vec![Expr::BinOp(
                Box::new(Expr::Ident("n".to_string(), span)),
                BinOp::GtEq,
                Box::new(Expr::IntLit(0, span)),
                span,
            )],
            span,
            doc: None,
        })],
        span,
    };

    let collected = collect_constraints(&program);
    assert_eq!(collected.len(), 1);
    assert!(collected[0].origin.contains("sqrt_int"));
    assert!(collected[0].lowered.is_ok());
}

#[test]
fn cell_where_clause_multiple_via_ast() {
    use lumen_compiler::compiler::ast::*;
    use lumen_compiler::compiler::tokens::Span;

    let span = Span {
        start: 0,
        end: 0,
        line: 1,
        col: 1,
    };

    let program = Program {
        directives: vec![],
        items: vec![Item::Cell(CellDef {
            name: "clamp".to_string(),
            generic_params: vec![],
            params: vec![
                Param {
                    name: "val".to_string(),
                    ty: TypeExpr::Named("Int".to_string(), span),
                    default_value: None,
                    variadic: false,
                    span,
                },
                Param {
                    name: "lo".to_string(),
                    ty: TypeExpr::Named("Int".to_string(), span),
                    default_value: None,
                    variadic: false,
                    span,
                },
                Param {
                    name: "hi".to_string(),
                    ty: TypeExpr::Named("Int".to_string(), span),
                    default_value: None,
                    variadic: false,
                    span,
                },
            ],
            return_type: Some(TypeExpr::Named("Int".to_string(), span)),
            effects: vec![],
            body: vec![],
            is_pub: false,
            is_async: false,
            is_extern: false,
            must_use: false,
            where_clauses: vec![
                Expr::BinOp(
                    Box::new(Expr::Ident("lo".to_string(), span)),
                    BinOp::GtEq,
                    Box::new(Expr::IntLit(0, span)),
                    span,
                ),
                Expr::BinOp(
                    Box::new(Expr::Ident("hi".to_string(), span)),
                    BinOp::GtEq,
                    Box::new(Expr::IntLit(0, span)),
                    span,
                ),
            ],
            span,
            doc: None,
        })],
        span,
    };

    let collected = collect_constraints(&program);
    assert_eq!(collected.len(), 2);
    assert!(collected[0].origin.contains("clamp"));
    assert!(collected[1].origin.contains("clamp"));
    assert!(collected[0].lowered.is_ok());
    assert!(collected[1].lowered.is_ok());
}

// ── Constraint collection from parsed AST ───────────────────────────

#[test]
fn collect_record_field_constraint_via_compile() {
    // Parse a program with record field where-clauses (indent-based syntax)
    // and verify the verification module can extract constraints from the AST.
    let source = r#"
record Positive
  value: Int where value > 0
end
cell main() -> Int
  let p = Positive(value: 1)
  p.value
end
"#;
    let md = markdown_from_code(source);

    // Use the internal pipeline to get the AST
    let extracted = lumen_compiler::markdown::extract::extract_blocks(&md);
    let mut full_code = String::new();
    let mut current_line = 1;
    for block in extracted.code_blocks.iter() {
        while current_line < block.code_start_line {
            full_code.push('\n');
            current_line += 1;
        }
        full_code.push_str(&block.code);
        let lines_in_block = block.code.chars().filter(|&c| c == '\n').count();
        current_line += lines_in_block;
    }

    let directives: Vec<lumen_compiler::compiler::ast::Directive> = extracted
        .directives
        .iter()
        .map(|d| lumen_compiler::compiler::ast::Directive {
            name: d.name.clone(),
            value: d.value.clone(),
            span: d.span,
        })
        .collect();

    let mut lexer = lumen_compiler::compiler::lexer::Lexer::new(&full_code, 1, 0);
    let tokens = lexer.tokenize().expect("lex failed");
    let mut parser = lumen_compiler::compiler::parser::Parser::new(tokens);
    let (program, parse_errors) = parser.parse_program_with_recovery(directives);
    assert!(
        parse_errors.is_empty(),
        "unexpected parse errors: {:?}",
        parse_errors
    );

    // Collect constraints from the AST
    let collected = collect_constraints(&program);
    assert!(
        !collected.is_empty(),
        "expected at least one constraint from record field where-clause"
    );
    assert!(
        collected[0].origin.contains("Positive"),
        "origin should reference 'Positive', got: {}",
        collected[0].origin
    );
    assert!(
        collected[0].lowered.is_ok(),
        "constraint lowering should succeed"
    );
}

#[test]
fn collect_cell_where_clause_via_ast() {
    // Cell where-clauses are not yet parsed, so we construct the AST directly
    // to test constraint collection from the CellDef.where_clauses field.
    use lumen_compiler::compiler::ast::*;
    use lumen_compiler::compiler::tokens::Span;

    let span = Span {
        start: 0,
        end: 0,
        line: 1,
        col: 1,
    };

    let program = Program {
        directives: vec![],
        items: vec![Item::Cell(CellDef {
            name: "divide".to_string(),
            generic_params: vec![],
            params: vec![
                Param {
                    name: "a".to_string(),
                    ty: TypeExpr::Named("Int".to_string(), span),
                    default_value: None,
                    variadic: false,
                    span,
                },
                Param {
                    name: "b".to_string(),
                    ty: TypeExpr::Named("Int".to_string(), span),
                    default_value: None,
                    variadic: false,
                    span,
                },
            ],
            return_type: Some(TypeExpr::Named("Int".to_string(), span)),
            effects: vec![],
            body: vec![],
            is_pub: false,
            is_async: false,
            is_extern: false,
            must_use: false,
            where_clauses: vec![Expr::BinOp(
                Box::new(Expr::Ident("b".to_string(), span)),
                BinOp::NotEq,
                Box::new(Expr::IntLit(0, span)),
                span,
            )],
            span,
            doc: None,
        })],
        span,
    };

    let collected = collect_constraints(&program);
    assert!(
        !collected.is_empty(),
        "expected at least one constraint from cell where-clause"
    );
    assert!(
        collected[0].origin.contains("divide"),
        "origin should reference 'divide', got: {}",
        collected[0].origin
    );
    assert!(
        collected[0].lowered.is_ok(),
        "constraint lowering should succeed"
    );
}

#[test]
fn verify_record_field_constraint_always_true() {
    // A constraint like `true` on a field should verify.
    use lumen_compiler::compiler::ast::*;
    use lumen_compiler::compiler::resolve::SymbolTable;
    use lumen_compiler::compiler::tokens::Span;

    let span = Span {
        start: 0,
        end: 0,
        line: 1,
        col: 1,
    };

    let program = Program {
        directives: vec![],
        items: vec![Item::Record(RecordDef {
            name: "AlwaysOk".to_string(),
            generic_params: vec![],
            fields: vec![FieldDef {
                name: "x".to_string(),
                ty: TypeExpr::Named("Int".to_string(), span),
                default_value: None,
                constraint: Some(Expr::BoolLit(true, span)),
                span,
            }],
            is_pub: false,
            span,
            doc: None,
        })],
        span,
    };

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

    let results = verify(&program, &symbols);
    assert_eq!(results.len(), 1);
    assert!(
        matches!(&results[0], VerificationResult::Verified { .. }),
        "true constraint should be verified, got: {:?}",
        results[0]
    );
}

#[test]
fn collect_constraints_from_multiple_records() {
    use lumen_compiler::compiler::ast::*;
    use lumen_compiler::compiler::tokens::Span;

    let span = Span {
        start: 0,
        end: 0,
        line: 1,
        col: 1,
    };

    fn ident(name: &str, span: Span) -> Expr {
        Expr::Ident(name.to_string(), span)
    }
    fn int_lit(v: i64, span: Span) -> Expr {
        Expr::IntLit(v, span)
    }

    let program = Program {
        directives: vec![],
        items: vec![
            Item::Record(RecordDef {
                name: "Positive".to_string(),
                generic_params: vec![],
                fields: vec![FieldDef {
                    name: "value".to_string(),
                    ty: TypeExpr::Named("Int".to_string(), span),
                    default_value: None,
                    constraint: Some(Expr::BinOp(
                        Box::new(ident("value", span)),
                        BinOp::Gt,
                        Box::new(int_lit(0, span)),
                        span,
                    )),
                    span,
                }],
                is_pub: false,
                span,
                doc: None,
            }),
            Item::Record(RecordDef {
                name: "Bounded".to_string(),
                generic_params: vec![],
                fields: vec![FieldDef {
                    name: "x".to_string(),
                    ty: TypeExpr::Named("Int".to_string(), span),
                    default_value: None,
                    constraint: Some(Expr::BinOp(
                        Box::new(ident("x", span)),
                        BinOp::LtEq,
                        Box::new(int_lit(100, span)),
                        span,
                    )),
                    span,
                }],
                is_pub: false,
                span,
                doc: None,
            }),
        ],
        span,
    };

    let collected = collect_constraints(&program);
    assert_eq!(collected.len(), 2, "expected 2 constraints from 2 records");
    assert!(collected[0].origin.contains("Positive"));
    assert!(collected[1].origin.contains("Bounded"));
    assert!(collected[0].lowered.is_ok());
    assert!(collected[1].lowered.is_ok());
}

#[test]
fn collect_no_constraints_from_unconstrained_record() {
    use lumen_compiler::compiler::ast::*;
    use lumen_compiler::compiler::tokens::Span;

    let span = Span {
        start: 0,
        end: 0,
        line: 1,
        col: 1,
    };

    let program = Program {
        directives: vec![],
        items: vec![Item::Record(RecordDef {
            name: "Plain".to_string(),
            generic_params: vec![],
            fields: vec![FieldDef {
                name: "x".to_string(),
                ty: TypeExpr::Named("Int".to_string(), span),
                default_value: None,
                constraint: None,
                span,
            }],
            is_pub: false,
            span,
            doc: None,
        })],
        span,
    };

    let collected = collect_constraints(&program);
    assert!(
        collected.is_empty(),
        "unconstrained record should produce no constraints"
    );
}

// ── Cell where-clause constraint lowering ───────────────────────────

#[test]
fn cell_where_clause_lowers_to_constraint() {
    use lumen_compiler::compiler::ast::*;
    use lumen_compiler::compiler::tokens::Span;
    use lumen_compiler::compiler::verification::constraints::{CmpOp, Constraint};

    let span = Span {
        start: 0,
        end: 0,
        line: 1,
        col: 1,
    };

    let program = Program {
        directives: vec![],
        items: vec![Item::Cell(CellDef {
            name: "safe_div".to_string(),
            generic_params: vec![],
            params: vec![
                Param {
                    name: "a".to_string(),
                    ty: TypeExpr::Named("Int".to_string(), span),
                    default_value: None,
                    variadic: false,
                    span,
                },
                Param {
                    name: "b".to_string(),
                    ty: TypeExpr::Named("Int".to_string(), span),
                    default_value: None,
                    variadic: false,
                    span,
                },
            ],
            return_type: Some(TypeExpr::Named("Int".to_string(), span)),
            effects: vec![],
            body: vec![],
            is_pub: false,
            is_async: false,
            is_extern: false,
            must_use: false,
            where_clauses: vec![Expr::BinOp(
                Box::new(Expr::Ident("b".to_string(), span)),
                BinOp::NotEq,
                Box::new(Expr::IntLit(0, span)),
                span,
            )],
            span,
            doc: None,
        })],
        span,
    };

    let collected = collect_constraints(&program);
    assert_eq!(collected.len(), 1);
    assert!(collected[0].origin.contains("safe_div"));

    let constraint = collected[0]
        .lowered
        .as_ref()
        .expect("lowering should succeed");
    assert_eq!(
        *constraint,
        Constraint::IntComparison {
            var: "b".to_string(),
            op: CmpOp::NotEq,
            value: 0,
        }
    );
}
