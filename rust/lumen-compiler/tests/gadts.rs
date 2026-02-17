//! Wave 21 — T117: GADT (Generalized Algebraic Data Types) tests
//!
//! Exercises the GADT module:
//! - Validation: enum name, arity, concrete types, specialization detection
//! - Type refinement: per-variant type-parameter narrowing
//! - Exhaustiveness: missing-variant detection

use lumen_compiler::compiler::gadts::*;
use lumen_compiler::compiler::tokens::Span;

fn span() -> Span {
    Span::dummy()
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Build the canonical `Expr[T]` GADT used in most tests.
fn make_expr_gadt() -> GadtDef {
    GadtDef {
        enum_name: "Expr".into(),
        generic_params: vec!["T".into()],
        variants: vec![
            GadtVariantInfo {
                name: "IntLit".into(),
                payload_types: vec!["Int".into()],
                return_constraint: Some(GadtVariantConstraint {
                    variant_name: "Expr".into(),
                    type_args: vec![GadtTypeArg::Concrete("Int".into())],
                    span: span(),
                }),
            },
            GadtVariantInfo {
                name: "BoolLit".into(),
                payload_types: vec!["Bool".into()],
                return_constraint: Some(GadtVariantConstraint {
                    variant_name: "Expr".into(),
                    type_args: vec![GadtTypeArg::Concrete("Bool".into())],
                    span: span(),
                }),
            },
            GadtVariantInfo {
                name: "Add".into(),
                payload_types: vec!["Expr[Int]".into(), "Expr[Int]".into()],
                return_constraint: Some(GadtVariantConstraint {
                    variant_name: "Expr".into(),
                    type_args: vec![GadtTypeArg::Concrete("Int".into())],
                    span: span(),
                }),
            },
            GadtVariantInfo {
                name: "Eq".into(),
                payload_types: vec!["Expr[Int]".into(), "Expr[Int]".into()],
                return_constraint: Some(GadtVariantConstraint {
                    variant_name: "Expr".into(),
                    type_args: vec![GadtTypeArg::Concrete("Bool".into())],
                    span: span(),
                }),
            },
            GadtVariantInfo {
                name: "If".into(),
                payload_types: vec!["Expr[Bool]".into(), "Expr[T]".into(), "Expr[T]".into()],
                return_constraint: Some(GadtVariantConstraint {
                    variant_name: "Expr".into(),
                    type_args: vec![GadtTypeArg::Param("T".into())],
                    span: span(),
                }),
            },
        ],
        span: span(),
    }
}

/// Build a two-parameter GADT `Pair[A, B]`.
fn make_pair_gadt() -> GadtDef {
    GadtDef {
        enum_name: "Pair".into(),
        generic_params: vec!["A".into(), "B".into()],
        variants: vec![
            GadtVariantInfo {
                name: "IntStr".into(),
                payload_types: vec!["Int".into(), "String".into()],
                return_constraint: Some(GadtVariantConstraint {
                    variant_name: "Pair".into(),
                    type_args: vec![
                        GadtTypeArg::Concrete("Int".into()),
                        GadtTypeArg::Concrete("String".into()),
                    ],
                    span: span(),
                }),
            },
            GadtVariantInfo {
                name: "BoolFloat".into(),
                payload_types: vec!["Bool".into(), "Float".into()],
                return_constraint: Some(GadtVariantConstraint {
                    variant_name: "Pair".into(),
                    type_args: vec![
                        GadtTypeArg::Concrete("Bool".into()),
                        GadtTypeArg::Concrete("Float".into()),
                    ],
                    span: span(),
                }),
            },
            GadtVariantInfo {
                name: "Generic".into(),
                payload_types: vec!["A".into(), "B".into()],
                return_constraint: Some(GadtVariantConstraint {
                    variant_name: "Pair".into(),
                    type_args: vec![
                        GadtTypeArg::Param("A".into()),
                        GadtTypeArg::Param("B".into()),
                    ],
                    span: span(),
                }),
            },
        ],
        span: span(),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Validation tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn gadt_validate_valid_expr() {
    let gadt = make_expr_gadt();
    assert!(
        validate_gadt(&gadt).is_ok(),
        "canonical Expr[T] GADT should validate"
    );
}

#[test]
fn gadt_validate_valid_pair() {
    let gadt = make_pair_gadt();
    assert!(
        validate_gadt(&gadt).is_ok(),
        "Pair[A, B] GADT should validate"
    );
}

#[test]
fn gadt_validate_rejects_wrong_enum_name() {
    let gadt = GadtDef {
        enum_name: "Expr".into(),
        generic_params: vec!["T".into()],
        variants: vec![GadtVariantInfo {
            name: "Bad".into(),
            payload_types: vec![],
            return_constraint: Some(GadtVariantConstraint {
                variant_name: "WrongName".into(),
                type_args: vec![GadtTypeArg::Concrete("Int".into())],
                span: span(),
            }),
        }],
        span: span(),
    };
    let errs = validate_gadt(&gadt).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, GadtError::WrongEnumName { .. })));
}

#[test]
fn gadt_validate_rejects_arity_mismatch() {
    let gadt = GadtDef {
        enum_name: "Expr".into(),
        generic_params: vec!["T".into()],
        variants: vec![GadtVariantInfo {
            name: "Bad".into(),
            payload_types: vec![],
            return_constraint: Some(GadtVariantConstraint {
                variant_name: "Expr".into(),
                type_args: vec![
                    GadtTypeArg::Concrete("Int".into()),
                    GadtTypeArg::Concrete("Bool".into()),
                ],
                span: span(),
            }),
        }],
        span: span(),
    };
    let errs = validate_gadt(&gadt).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, GadtError::ArityMismatch { .. })));
}

#[test]
fn gadt_validate_rejects_invalid_concrete_type() {
    let gadt = GadtDef {
        enum_name: "Expr".into(),
        generic_params: vec!["T".into()],
        variants: vec![GadtVariantInfo {
            name: "Bad".into(),
            payload_types: vec![],
            return_constraint: Some(GadtVariantConstraint {
                variant_name: "Expr".into(),
                type_args: vec![GadtTypeArg::Concrete("FooBar".into())],
                span: span(),
            }),
        }],
        span: span(),
    };
    let errs = validate_gadt(&gadt).unwrap_err();
    assert!(errs.iter().any(
        |e| matches!(e, GadtError::InvalidConcreteType { type_name, .. } if type_name == "FooBar")
    ));
}

#[test]
fn gadt_validate_rejects_no_concrete_specialization() {
    let gadt = GadtDef {
        enum_name: "Box".into(),
        generic_params: vec!["T".into()],
        variants: vec![
            GadtVariantInfo {
                name: "Wrap".into(),
                payload_types: vec!["T".into()],
                return_constraint: Some(GadtVariantConstraint {
                    variant_name: "Box".into(),
                    type_args: vec![GadtTypeArg::Param("T".into())],
                    span: span(),
                }),
            },
            GadtVariantInfo {
                name: "Empty".into(),
                payload_types: vec![],
                return_constraint: None,
            },
        ],
        span: span(),
    };
    let errs = validate_gadt(&gadt).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, GadtError::NoConcreteSpecialization { .. })));
}

#[test]
fn gadt_validate_multiple_errors_at_once() {
    let gadt = GadtDef {
        enum_name: "Expr".into(),
        generic_params: vec!["T".into()],
        variants: vec![
            GadtVariantInfo {
                name: "BadName".into(),
                payload_types: vec![],
                return_constraint: Some(GadtVariantConstraint {
                    variant_name: "Wrong".into(),
                    type_args: vec![GadtTypeArg::Concrete("Oops".into())],
                    span: span(),
                }),
            },
            GadtVariantInfo {
                name: "BadArity".into(),
                payload_types: vec![],
                return_constraint: Some(GadtVariantConstraint {
                    variant_name: "Expr".into(),
                    type_args: vec![
                        GadtTypeArg::Concrete("Int".into()),
                        GadtTypeArg::Concrete("Bool".into()),
                    ],
                    span: span(),
                }),
            },
        ],
        span: span(),
    };
    let errs = validate_gadt(&gadt).unwrap_err();
    // Should detect wrong name, invalid concrete type, arity mismatch,
    // and no-concrete (since the only Concrete that *would* count belongs
    // to the wrong-enum-name variant — it's still concrete though).
    assert!(errs.len() >= 3, "expected ≥3 errors, got {}", errs.len());
}

#[test]
fn gadt_validate_variant_without_constraint_ok() {
    // A variant with no return constraint among valid GADT variants is fine.
    let gadt = GadtDef {
        enum_name: "Maybe".into(),
        generic_params: vec!["T".into()],
        variants: vec![
            GadtVariantInfo {
                name: "Just".into(),
                payload_types: vec!["T".into()],
                return_constraint: Some(GadtVariantConstraint {
                    variant_name: "Maybe".into(),
                    type_args: vec![GadtTypeArg::Concrete("Int".into())],
                    span: span(),
                }),
            },
            GadtVariantInfo {
                name: "Nothing".into(),
                payload_types: vec![],
                return_constraint: None,
            },
        ],
        span: span(),
    };
    assert!(validate_gadt(&gadt).is_ok());
}

// ═══════════════════════════════════════════════════════════════════════
// Type refinement tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn gadt_refine_intlit_gives_t_int() {
    let gadt = make_expr_gadt();
    let r = refine_type_in_branch(&gadt, "IntLit", &["T".into()]);
    assert_eq!(r.get("T"), Some(&"Int".to_string()));
}

#[test]
fn gadt_refine_boollit_gives_t_bool() {
    let gadt = make_expr_gadt();
    let r = refine_type_in_branch(&gadt, "BoolLit", &["T".into()]);
    assert_eq!(r.get("T"), Some(&"Bool".to_string()));
}

#[test]
fn gadt_refine_add_gives_t_int() {
    let gadt = make_expr_gadt();
    let r = refine_type_in_branch(&gadt, "Add", &["T".into()]);
    assert_eq!(r.get("T"), Some(&"Int".to_string()));
}

#[test]
fn gadt_refine_eq_gives_t_bool() {
    let gadt = make_expr_gadt();
    let r = refine_type_in_branch(&gadt, "Eq", &["T".into()]);
    assert_eq!(r.get("T"), Some(&"Bool".to_string()));
}

#[test]
fn gadt_refine_if_gives_no_refinement() {
    let gadt = make_expr_gadt();
    let r = refine_type_in_branch(&gadt, "If", &["T".into()]);
    assert!(r.is_empty(), "If variant should not refine T, got: {r:?}");
}

#[test]
fn gadt_refine_unknown_variant_gives_empty() {
    let gadt = make_expr_gadt();
    let r = refine_type_in_branch(&gadt, "DoesNotExist", &["T".into()]);
    assert!(r.is_empty());
}

#[test]
fn gadt_refine_variant_without_constraint_gives_empty() {
    let gadt = GadtDef {
        enum_name: "Box".into(),
        generic_params: vec!["T".into()],
        variants: vec![GadtVariantInfo {
            name: "Empty".into(),
            payload_types: vec![],
            return_constraint: None,
        }],
        span: span(),
    };
    let r = refine_type_in_branch(&gadt, "Empty", &["T".into()]);
    assert!(r.is_empty());
}

#[test]
fn gadt_refine_multi_param_intstr() {
    let gadt = make_pair_gadt();
    let r = refine_type_in_branch(&gadt, "IntStr", &["A".into(), "B".into()]);
    assert_eq!(r.get("A"), Some(&"Int".to_string()));
    assert_eq!(r.get("B"), Some(&"String".to_string()));
}

#[test]
fn gadt_refine_multi_param_boolfloat() {
    let gadt = make_pair_gadt();
    let r = refine_type_in_branch(&gadt, "BoolFloat", &["A".into(), "B".into()]);
    assert_eq!(r.get("A"), Some(&"Bool".to_string()));
    assert_eq!(r.get("B"), Some(&"Float".to_string()));
}

#[test]
fn gadt_refine_multi_param_generic_no_refinement() {
    let gadt = make_pair_gadt();
    let r = refine_type_in_branch(&gadt, "Generic", &["A".into(), "B".into()]);
    assert!(r.is_empty());
}

#[test]
fn gadt_refine_partial_specialization() {
    // Only A is specialized, B stays generic.
    let gadt = GadtDef {
        enum_name: "Half".into(),
        generic_params: vec!["A".into(), "B".into()],
        variants: vec![GadtVariantInfo {
            name: "IntAny".into(),
            payload_types: vec!["Int".into(), "B".into()],
            return_constraint: Some(GadtVariantConstraint {
                variant_name: "Half".into(),
                type_args: vec![
                    GadtTypeArg::Concrete("Int".into()),
                    GadtTypeArg::Param("B".into()),
                ],
                span: span(),
            }),
        }],
        span: span(),
    };
    let r = refine_type_in_branch(&gadt, "IntAny", &["A".into(), "B".into()]);
    assert_eq!(r.get("A"), Some(&"Int".to_string()));
    assert!(r.get("B").is_none(), "B should not be refined");
}

#[test]
fn gadt_refine_complex_type_arg() {
    let gadt = GadtDef {
        enum_name: "Container".into(),
        generic_params: vec!["T".into()],
        variants: vec![GadtVariantInfo {
            name: "ListVariant".into(),
            payload_types: vec![],
            return_constraint: Some(GadtVariantConstraint {
                variant_name: "Container".into(),
                type_args: vec![GadtTypeArg::Complex("List[Int]".into())],
                span: span(),
            }),
        }],
        span: span(),
    };
    let r = refine_type_in_branch(&gadt, "ListVariant", &["T".into()]);
    assert_eq!(r.get("T"), Some(&"List[Int]".to_string()));
}

// ═══════════════════════════════════════════════════════════════════════
// Exhaustiveness tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn gadt_exhaustiveness_all_covered() {
    let gadt = make_expr_gadt();
    let matched: Vec<String> = vec![
        "IntLit".into(),
        "BoolLit".into(),
        "Add".into(),
        "Eq".into(),
        "If".into(),
    ];
    let missing = check_gadt_exhaustiveness(&gadt, &matched);
    assert!(missing.is_empty(), "all variants matched: {missing:?}");
}

#[test]
fn gadt_exhaustiveness_missing_variants() {
    let gadt = make_expr_gadt();
    let matched: Vec<String> = vec!["IntLit".into(), "Add".into()];
    let missing = check_gadt_exhaustiveness(&gadt, &matched);
    assert_eq!(missing.len(), 3);
    assert!(missing.contains(&"BoolLit".to_string()));
    assert!(missing.contains(&"Eq".to_string()));
    assert!(missing.contains(&"If".to_string()));
}

#[test]
fn gadt_exhaustiveness_none_matched() {
    let gadt = make_expr_gadt();
    let missing = check_gadt_exhaustiveness(&gadt, &[]);
    assert_eq!(missing.len(), 5);
}

#[test]
fn gadt_exhaustiveness_pair_partial() {
    let gadt = make_pair_gadt();
    let matched: Vec<String> = vec!["IntStr".into()];
    let missing = check_gadt_exhaustiveness(&gadt, &matched);
    assert_eq!(missing.len(), 2);
    assert!(missing.contains(&"BoolFloat".to_string()));
    assert!(missing.contains(&"Generic".to_string()));
}

// ═══════════════════════════════════════════════════════════════════════
// Error display tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn gadt_error_display_wrong_name() {
    let err = GadtError::WrongEnumName {
        variant_name: "V".into(),
        found: "X".into(),
        expected: "Y".into(),
        span: span(),
    };
    let s = err.to_string();
    assert!(s.contains("V") && s.contains("X") && s.contains("Y"));
}

#[test]
fn gadt_error_display_arity() {
    let err = GadtError::ArityMismatch {
        variant_name: "V".into(),
        found: 2,
        expected: 1,
        span: span(),
    };
    let s = err.to_string();
    assert!(s.contains('2') && s.contains('1'));
}

#[test]
fn gadt_error_display_invalid_type() {
    let err = GadtError::InvalidConcreteType {
        variant_name: "V".into(),
        type_name: "Bogus".into(),
        span: span(),
    };
    assert!(err.to_string().contains("Bogus"));
}

#[test]
fn gadt_error_display_no_specialization() {
    let err = GadtError::NoConcreteSpecialization {
        enum_name: "Foo".into(),
        span: span(),
    };
    assert!(err.to_string().contains("Foo"));
}
