//! Comprehensive tests for the counter-example generation module (T149).

use lumen_compiler::compiler::verification::counterexample::*;

// ── parse_simple_constraint tests ───────────────────────────────────

#[test]
fn counterexample_parse_x_gt_0() {
    let parsed = parse_simple_constraint("x > 0").unwrap();
    assert_eq!(
        parsed,
        ParsedConstraint::Comparison {
            left: "x".to_string(),
            op: CompOp::Gt,
            right: "0".to_string(),
        }
    );
}

#[test]
fn counterexample_parse_x_lt_100() {
    let parsed = parse_simple_constraint("x < 100").unwrap();
    assert_eq!(
        parsed,
        ParsedConstraint::Comparison {
            left: "x".to_string(),
            op: CompOp::Lt,
            right: "100".to_string(),
        }
    );
}

#[test]
fn counterexample_parse_x_eq_5() {
    let parsed = parse_simple_constraint("x == 5").unwrap();
    assert_eq!(
        parsed,
        ParsedConstraint::Comparison {
            left: "x".to_string(),
            op: CompOp::Eq,
            right: "5".to_string(),
        }
    );
}

#[test]
fn counterexample_parse_and_constraint() {
    let parsed = parse_simple_constraint("x >= 1 and x <= 10").unwrap();
    match parsed {
        ParsedConstraint::And(l, r) => {
            assert_eq!(
                *l,
                ParsedConstraint::Comparison {
                    left: "x".to_string(),
                    op: CompOp::Ge,
                    right: "1".to_string(),
                }
            );
            assert_eq!(
                *r,
                ParsedConstraint::Comparison {
                    left: "x".to_string(),
                    op: CompOp::Le,
                    right: "10".to_string(),
                }
            );
        }
        other => panic!("expected And, got {:?}", other),
    }
}

#[test]
fn counterexample_parse_complex_returns_none() {
    // "a + b == c" contains arithmetic operators — too complex
    assert!(parse_simple_constraint("a + b == c").is_none());
}

#[test]
fn counterexample_parse_func_call_len() {
    let parsed = parse_simple_constraint("len(s) > 0").unwrap();
    assert_eq!(
        parsed,
        ParsedConstraint::FuncCall {
            name: "len".to_string(),
            arg: "s".to_string(),
            op: CompOp::Gt,
            value: "0".to_string(),
        }
    );
}

#[test]
fn counterexample_parse_or_constraint() {
    let parsed = parse_simple_constraint("x > 10 or x < 0").unwrap();
    match parsed {
        ParsedConstraint::Or(l, r) => {
            assert!(matches!(*l, ParsedConstraint::Comparison { .. }));
            assert!(matches!(*r, ParsedConstraint::Comparison { .. }));
        }
        other => panic!("expected Or, got {:?}", other),
    }
}

#[test]
fn counterexample_parse_ne_constraint() {
    let parsed = parse_simple_constraint("x != 0").unwrap();
    assert_eq!(
        parsed,
        ParsedConstraint::Comparison {
            left: "x".to_string(),
            op: CompOp::Ne,
            right: "0".to_string(),
        }
    );
}

// ── generate_counterexample tests ───────────────────────────────────

#[test]
fn counterexample_generate_x_gt_0() {
    let vars = vec![("x".to_string(), "Int".to_string())];
    let ce = generate_counterexample("x > 0", &vars, true).unwrap();
    assert_eq!(ce.violated_constraint, "x > 0");
    assert_eq!(ce.variables.len(), 1);
    assert_eq!(ce.variables[0].name, "x");
    // x > 0 is violated by x = 0 (boundary)
    assert_eq!(ce.variables[0].value, ConcreteValue::Int(0));
}

#[test]
fn counterexample_generate_x_lt_100() {
    let vars = vec![("x".to_string(), "Int".to_string())];
    let ce = generate_counterexample("x < 100", &vars, true).unwrap();
    assert_eq!(ce.violated_constraint, "x < 100");
    assert_eq!(ce.variables[0].name, "x");
    // x < 100 is violated by x = 100
    assert_eq!(ce.variables[0].value, ConcreteValue::Int(100));
}

#[test]
fn counterexample_generate_x_ge_1_and_x_le_10() {
    let vars = vec![("x".to_string(), "Int".to_string())];
    let ce = generate_counterexample("x >= 1 and x <= 10", &vars, true).unwrap();
    assert_eq!(ce.violated_constraint, "x >= 1 and x <= 10");
    assert_eq!(ce.variables.len(), 1);
    assert_eq!(ce.variables[0].name, "x");
    // Should be x = 0 (below range [1, 10])
    let val = match &ce.variables[0].value {
        ConcreteValue::Int(v) => *v,
        other => panic!("expected Int, got {:?}", other),
    };
    assert!(
        val < 1 || val > 10,
        "expected value outside [1, 10], got {}",
        val
    );
}

#[test]
fn counterexample_generate_x_eq_5() {
    let vars = vec![("x".to_string(), "Int".to_string())];
    let ce = generate_counterexample("x == 5", &vars, true).unwrap();
    let val = match &ce.variables[0].value {
        ConcreteValue::Int(v) => *v,
        other => panic!("expected Int, got {:?}", other),
    };
    assert_ne!(val, 5, "counter-example for x == 5 should not be 5");
}

#[test]
fn counterexample_generate_boolean() {
    let vars = vec![("flag".to_string(), "Bool".to_string())];
    let ce = generate_counterexample("flag == true", &vars, true).unwrap();
    assert_eq!(ce.variables[0].name, "flag");
    assert_eq!(ce.variables[0].value, ConcreteValue::Bool(false));
}

#[test]
fn counterexample_generate_len_s_gt_0() {
    let vars = vec![("s".to_string(), "String".to_string())];
    let ce = generate_counterexample("len(s) > 0", &vars, true).unwrap();
    assert_eq!(ce.variables[0].name, "s");
    // len(s) > 0 violated by empty string
    assert_eq!(ce.variables[0].value, ConcreteValue::Str(String::new()));
}

#[test]
fn counterexample_not_violated_returns_none() {
    let vars = vec![("x".to_string(), "Int".to_string())];
    let result = generate_counterexample("x > 0", &vars, false);
    assert!(result.is_none());
}

#[test]
fn counterexample_generate_x_ge_0() {
    let vars = vec![("x".to_string(), "Int".to_string())];
    let ce = generate_counterexample("x >= 0", &vars, true).unwrap();
    // x >= 0 violated by x = -1
    assert_eq!(ce.variables[0].value, ConcreteValue::Int(-1));
}

#[test]
fn counterexample_generate_x_le_10() {
    let vars = vec![("x".to_string(), "Int".to_string())];
    let ce = generate_counterexample("x <= 10", &vars, true).unwrap();
    // x <= 10 violated by x = 11
    assert_eq!(ce.variables[0].value, ConcreteValue::Int(11));
}

#[test]
fn counterexample_generate_x_ne_0() {
    let vars = vec![("x".to_string(), "Int".to_string())];
    let ce = generate_counterexample("x != 0", &vars, true).unwrap();
    // x != 0 violated by x = 0
    assert_eq!(ce.variables[0].value, ConcreteValue::Int(0));
}

// ── format_counterexample tests ─────────────────────────────────────

#[test]
fn counterexample_format_multiline() {
    let ce = CounterExample {
        variables: vec![VariableBinding {
            name: "x".to_string(),
            value: ConcreteValue::Int(-1),
            constraint_role: ConstraintRole::Input,
        }],
        violated_constraint: "x > 0".to_string(),
        explanation: "When x = -1, the constraint x > 0 is false".to_string(),
        trace: vec![EvalStep {
            expression: "x > 0".to_string(),
            result: ConcreteValue::Bool(false),
            note: None,
        }],
    };
    let formatted = format_counterexample(&ce);
    assert!(formatted.contains("Counter-example for violated constraint: x > 0"));
    assert!(formatted.contains("x = -1 (input)"));
    assert!(formatted.contains("Trace:"));
    assert!(formatted.contains("x > 0 => false"));
}

#[test]
fn counterexample_format_short() {
    let ce = CounterExample {
        variables: vec![VariableBinding {
            name: "x".to_string(),
            value: ConcreteValue::Int(-1),
            constraint_role: ConstraintRole::Input,
        }],
        violated_constraint: "x > 0".to_string(),
        explanation: "When x = -1, the constraint x > 0 is false".to_string(),
        trace: vec![],
    };
    let short = format_counterexample_short(&ce);
    assert_eq!(short, "x = -1 violates x > 0");
}

#[test]
fn counterexample_format_short_multiple_vars() {
    let ce = CounterExample {
        variables: vec![
            VariableBinding {
                name: "x".to_string(),
                value: ConcreteValue::Int(0),
                constraint_role: ConstraintRole::Input,
            },
            VariableBinding {
                name: "y".to_string(),
                value: ConcreteValue::Int(11),
                constraint_role: ConstraintRole::Input,
            },
        ],
        violated_constraint: "x >= 1 and y <= 10".to_string(),
        explanation: "test".to_string(),
        trace: vec![],
    };
    let short = format_counterexample_short(&ce);
    assert_eq!(short, "x = 0, y = 11 violates x >= 1 and y <= 10");
}

// ── Type construction tests ─────────────────────────────────────────

#[test]
fn counterexample_concrete_value_variants() {
    // Verify all ConcreteValue variants construct correctly
    let _ = ConcreteValue::Int(42);
    let _ = ConcreteValue::Float(3.14);
    let _ = ConcreteValue::Bool(true);
    let _ = ConcreteValue::Str("hello".to_string());
    let _ = ConcreteValue::Null;
    let _ = ConcreteValue::List(vec![ConcreteValue::Int(1)]);
    let _ = ConcreteValue::Tuple(vec![ConcreteValue::Int(1), ConcreteValue::Bool(false)]);
    let _ = ConcreteValue::Record(
        "Point".to_string(),
        vec![
            ("x".to_string(), ConcreteValue::Int(1)),
            ("y".to_string(), ConcreteValue::Int(2)),
        ],
    );
}

#[test]
fn counterexample_constraint_role_variants() {
    assert_eq!(format!("{}", ConstraintRole::Input), "input");
    assert_eq!(format!("{}", ConstraintRole::Output), "output");
    assert_eq!(format!("{}", ConstraintRole::Intermediate), "intermediate");
    assert_eq!(format!("{}", ConstraintRole::Bound), "bound");
}

#[test]
fn counterexample_eval_step_construction() {
    let step = EvalStep {
        expression: "x > 0".to_string(),
        result: ConcreteValue::Bool(false),
        note: Some("boundary value".to_string()),
    };
    assert_eq!(step.expression, "x > 0");
    assert_eq!(step.result, ConcreteValue::Bool(false));
    assert_eq!(step.note, Some("boundary value".to_string()));
}

#[test]
fn counterexample_eval_step_no_note() {
    let step = EvalStep {
        expression: "x == 5".to_string(),
        result: ConcreteValue::Bool(true),
        note: None,
    };
    assert!(step.note.is_none());
}

#[test]
fn counterexample_variable_binding_construction() {
    let binding = VariableBinding {
        name: "count".to_string(),
        value: ConcreteValue::Int(0),
        constraint_role: ConstraintRole::Output,
    };
    assert_eq!(binding.name, "count");
    assert_eq!(binding.value, ConcreteValue::Int(0));
    assert_eq!(binding.constraint_role, ConstraintRole::Output);
}

#[test]
fn counterexample_multiple_variables() {
    let ce = CounterExample {
        variables: vec![
            VariableBinding {
                name: "x".to_string(),
                value: ConcreteValue::Int(0),
                constraint_role: ConstraintRole::Input,
            },
            VariableBinding {
                name: "y".to_string(),
                value: ConcreteValue::Int(100),
                constraint_role: ConstraintRole::Input,
            },
            VariableBinding {
                name: "z".to_string(),
                value: ConcreteValue::Str("".to_string()),
                constraint_role: ConstraintRole::Intermediate,
            },
        ],
        violated_constraint: "x > 0 and y < 50 and len(z) > 0".to_string(),
        explanation: "Multiple violations".to_string(),
        trace: vec![],
    };
    assert_eq!(ce.variables.len(), 3);
    assert_eq!(ce.variables[0].name, "x");
    assert_eq!(ce.variables[1].name, "y");
    assert_eq!(ce.variables[2].name, "z");
}

// ── Edge cases ──────────────────────────────────────────────────────

#[test]
fn counterexample_format_with_note_in_trace() {
    let ce = CounterExample {
        variables: vec![VariableBinding {
            name: "x".to_string(),
            value: ConcreteValue::Int(0),
            constraint_role: ConstraintRole::Input,
        }],
        violated_constraint: "x > 0".to_string(),
        explanation: "boundary".to_string(),
        trace: vec![EvalStep {
            expression: "x > 0".to_string(),
            result: ConcreteValue::Bool(false),
            note: Some("x = 0 violates x > 0".to_string()),
        }],
    };
    let formatted = format_counterexample(&ce);
    assert!(formatted.contains("-- x = 0 violates x > 0"));
}

#[test]
fn counterexample_concrete_value_null_display() {
    assert_eq!(format!("{}", ConcreteValue::Null), "null");
}

#[test]
fn counterexample_concrete_value_float_display() {
    let v = ConcreteValue::Float(3.14);
    let s = format!("{}", v);
    assert!(s.starts_with("3.14"));
}

#[test]
fn counterexample_concrete_value_bool_display() {
    assert_eq!(format!("{}", ConcreteValue::Bool(true)), "true");
    assert_eq!(format!("{}", ConcreteValue::Bool(false)), "false");
}

#[test]
fn counterexample_concrete_value_tuple_display() {
    let t = ConcreteValue::Tuple(vec![
        ConcreteValue::Int(1),
        ConcreteValue::Str("a".to_string()),
    ]);
    assert_eq!(format!("{}", t), "(1, \"a\")");
}

#[test]
fn counterexample_comp_op_display() {
    assert_eq!(format!("{}", CompOp::Gt), ">");
    assert_eq!(format!("{}", CompOp::Lt), "<");
    assert_eq!(format!("{}", CompOp::Ge), ">=");
    assert_eq!(format!("{}", CompOp::Le), "<=");
    assert_eq!(format!("{}", CompOp::Eq), "==");
    assert_eq!(format!("{}", CompOp::Ne), "!=");
}

#[test]
fn counterexample_generate_has_explanation() {
    let vars = vec![("x".to_string(), "Int".to_string())];
    let ce = generate_counterexample("x > 0", &vars, true).unwrap();
    assert!(!ce.explanation.is_empty());
    assert!(ce.explanation.contains("x"));
}

#[test]
fn counterexample_generate_has_trace() {
    let vars = vec![("x".to_string(), "Int".to_string())];
    let ce = generate_counterexample("x > 0", &vars, true).unwrap();
    assert!(!ce.trace.is_empty());
    assert_eq!(ce.trace[0].result, ConcreteValue::Bool(false));
}

#[test]
fn counterexample_parse_ge_constraint() {
    let parsed = parse_simple_constraint("y >= 5").unwrap();
    assert_eq!(
        parsed,
        ParsedConstraint::Comparison {
            left: "y".to_string(),
            op: CompOp::Ge,
            right: "5".to_string(),
        }
    );
}

#[test]
fn counterexample_parse_le_constraint() {
    let parsed = parse_simple_constraint("count <= 100").unwrap();
    assert_eq!(
        parsed,
        ParsedConstraint::Comparison {
            left: "count".to_string(),
            op: CompOp::Le,
            right: "100".to_string(),
        }
    );
}

#[test]
fn counterexample_format_empty_trace() {
    let ce = CounterExample {
        variables: vec![VariableBinding {
            name: "x".to_string(),
            value: ConcreteValue::Int(0),
            constraint_role: ConstraintRole::Input,
        }],
        violated_constraint: "x > 0".to_string(),
        explanation: "test".to_string(),
        trace: vec![],
    };
    let formatted = format_counterexample(&ce);
    assert!(formatted.contains("x = 0 (input)"));
    // No "Trace:" section when trace is empty
    assert!(!formatted.contains("Trace:"));
}

#[test]
fn counterexample_generate_boolean_ne_true() {
    let vars = vec![("active".to_string(), "Bool".to_string())];
    let ce = generate_counterexample("active != true", &vars, true).unwrap();
    assert_eq!(ce.variables[0].value, ConcreteValue::Bool(true));
}

#[test]
fn counterexample_role_from_type_string() {
    let vars = vec![
        ("x".to_string(), "input".to_string()),
        ("y".to_string(), "output".to_string()),
        ("z".to_string(), "intermediate".to_string()),
        ("w".to_string(), "bound".to_string()),
    ];
    let ce = generate_counterexample("x > 0", &vars, true).unwrap();
    assert_eq!(ce.variables[0].constraint_role, ConstraintRole::Input);
}
