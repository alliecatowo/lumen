//! Comprehensive tests for array bounds propagation (T178).

use lumen_compiler::compiler::verification::bounds::*;

// ── BoundsContext basics ────────────────────────────────────────────

#[test]
fn context_new_starts_empty() {
    let ctx = BoundsContext::new();
    assert!(ctx.known_bounds.is_empty());
    assert!(ctx.conditions.is_empty());
    assert_eq!(ctx.scope_depth, 0);
}

#[test]
fn context_default_is_same_as_new() {
    let ctx = BoundsContext::default();
    assert!(ctx.known_bounds.is_empty());
    assert!(ctx.conditions.is_empty());
}

#[test]
fn context_set_length_and_retrieval() {
    let mut ctx = BoundsContext::new();
    ctx.set_length("items", 5);
    let info = ctx.known_bounds.get("items").unwrap();
    assert_eq!(info.exact_length, Some(5));
    assert_eq!(info.lower, Some(5));
    assert_eq!(info.upper, Some(5));
}

#[test]
fn context_set_bounds_and_retrieval() {
    let mut ctx = BoundsContext::new();
    ctx.set_bounds("x", Some(0), Some(99));
    let info = ctx.known_bounds.get("x").unwrap();
    assert_eq!(info.lower, Some(0));
    assert_eq!(info.upper, Some(99));
    assert_eq!(info.exact_length, None);
}

#[test]
fn context_set_bounds_tightens_existing() {
    let mut ctx = BoundsContext::new();
    ctx.set_bounds("x", Some(0), Some(100));
    ctx.set_bounds("x", Some(5), Some(50));
    let info = ctx.known_bounds.get("x").unwrap();
    // Lower should be max(0, 5) = 5
    assert_eq!(info.lower, Some(5));
    // Upper should be min(100, 50) = 50
    assert_eq!(info.upper, Some(50));
}

#[test]
fn context_push_condition_and_pop_condition() {
    let mut ctx = BoundsContext::new();
    assert!(ctx.conditions.is_empty());

    ctx.push_condition(ActiveCondition {
        variable: "x".to_string(),
        op: BoundsOp::Gt,
        value: 0,
        negated: false,
    });
    assert_eq!(ctx.conditions.len(), 1);

    ctx.push_condition(ActiveCondition {
        variable: "y".to_string(),
        op: BoundsOp::Lt,
        value: 10,
        negated: false,
    });
    assert_eq!(ctx.conditions.len(), 2);

    ctx.pop_condition();
    assert_eq!(ctx.conditions.len(), 1);
    assert_eq!(ctx.conditions[0].variable, "x");

    ctx.pop_condition();
    assert!(ctx.conditions.is_empty());
}

// ── infer_from_condition ────────────────────────────────────────────

#[test]
fn infer_gt_refines_lower_bound() {
    let mut ctx = BoundsContext::new();
    ctx.infer_from_condition("x", &BoundsOp::Gt, 3);
    let info = ctx.known_bounds.get("x").unwrap();
    // x > 3  =>  x >= 4
    assert_eq!(info.lower, Some(4));
    assert_eq!(info.upper, None);
}

#[test]
fn infer_lt_refines_upper_bound() {
    let mut ctx = BoundsContext::new();
    ctx.infer_from_condition("x", &BoundsOp::Lt, 10);
    let info = ctx.known_bounds.get("x").unwrap();
    // x < 10  =>  x <= 9
    assert_eq!(info.upper, Some(9));
    assert_eq!(info.lower, None);
}

#[test]
fn infer_ge_refines_lower_bound() {
    let mut ctx = BoundsContext::new();
    ctx.infer_from_condition("x", &BoundsOp::Ge, 5);
    let info = ctx.known_bounds.get("x").unwrap();
    assert_eq!(info.lower, Some(5));
}

#[test]
fn infer_le_refines_upper_bound() {
    let mut ctx = BoundsContext::new();
    ctx.infer_from_condition("x", &BoundsOp::Le, 20);
    let info = ctx.known_bounds.get("x").unwrap();
    assert_eq!(info.upper, Some(20));
}

#[test]
fn infer_eq_sets_both_bounds() {
    let mut ctx = BoundsContext::new();
    ctx.infer_from_condition("x", &BoundsOp::Eq, 7);
    let info = ctx.known_bounds.get("x").unwrap();
    assert_eq!(info.lower, Some(7));
    assert_eq!(info.upper, Some(7));
}

#[test]
fn infer_ne_does_not_tighten() {
    let mut ctx = BoundsContext::new();
    ctx.set_bounds("x", Some(0), Some(10));
    ctx.infer_from_condition("x", &BoundsOp::Ne, 5);
    let info = ctx.known_bounds.get("x").unwrap();
    // Ne doesn't tighten bounds
    assert_eq!(info.lower, Some(0));
    assert_eq!(info.upper, Some(10));
}

// ── check_index_access ──────────────────────────────────────────────

#[test]
fn check_index_known_length_in_bounds() {
    let mut ctx = BoundsContext::new();
    ctx.set_length("items", 5);
    assert_eq!(check_index_access(&ctx, "items", 0), BoundsResult::Safe);
    assert_eq!(check_index_access(&ctx, "items", 4), BoundsResult::Safe);
}

#[test]
fn check_index_known_length_out_of_bounds() {
    let mut ctx = BoundsContext::new();
    ctx.set_length("items", 5);
    match check_index_access(&ctx, "items", 5) {
        BoundsResult::Unsafe { reason } => {
            assert!(reason.contains("out of bounds"));
            assert!(reason.contains("items"));
        }
        other => panic!("expected Unsafe, got {:?}", other),
    }
    match check_index_access(&ctx, "items", 100) {
        BoundsResult::Unsafe { .. } => {}
        other => panic!("expected Unsafe, got {:?}", other),
    }
}

#[test]
fn check_index_negative_index_in_bounds() {
    let mut ctx = BoundsContext::new();
    ctx.set_length("items", 5);
    // Python-style: -1 is the last element
    assert_eq!(check_index_access(&ctx, "items", -1), BoundsResult::Safe);
    assert_eq!(check_index_access(&ctx, "items", -5), BoundsResult::Safe);
}

#[test]
fn check_index_negative_index_out_of_bounds() {
    let mut ctx = BoundsContext::new();
    ctx.set_length("items", 5);
    match check_index_access(&ctx, "items", -6) {
        BoundsResult::Unsafe { reason } => {
            assert!(reason.contains("out of bounds"));
        }
        other => panic!("expected Unsafe, got {:?}", other),
    }
}

#[test]
fn check_index_no_length_info_returns_unknown() {
    let ctx = BoundsContext::new();
    assert_eq!(check_index_access(&ctx, "items", 0), BoundsResult::Unknown);
}

#[test]
fn check_index_with_lower_bound_only() {
    let mut ctx = BoundsContext::new();
    // We know items has at least 3 elements (e.g. from len(items) >= 3)
    ctx.set_bounds("items", Some(3), None);
    assert_eq!(check_index_access(&ctx, "items", 0), BoundsResult::Safe);
    assert_eq!(check_index_access(&ctx, "items", 2), BoundsResult::Safe);
    // index 3 is unknown since we only know min length is 3
    assert_eq!(check_index_access(&ctx, "items", 3), BoundsResult::Unknown);
}

#[test]
fn check_index_with_upper_bound_exceeded() {
    let mut ctx = BoundsContext::new();
    ctx.set_bounds("items", None, Some(5));
    match check_index_access(&ctx, "items", 5) {
        BoundsResult::Unsafe { reason } => {
            assert!(reason.contains("at most"));
        }
        other => panic!("expected Unsafe, got {:?}", other),
    }
}

// ── check_dynamic_index ─────────────────────────────────────────────

#[test]
fn check_dynamic_index_both_bounds_safe() {
    let mut ctx = BoundsContext::new();
    ctx.set_length("items", 10);
    ctx.set_bounds("i", Some(0), Some(9));
    assert_eq!(check_dynamic_index(&ctx, "items", "i"), BoundsResult::Safe);
}

#[test]
fn check_dynamic_index_potentially_too_large() {
    let mut ctx = BoundsContext::new();
    ctx.set_length("items", 5);
    ctx.set_bounds("i", Some(0), Some(10));
    match check_dynamic_index(&ctx, "items", "i") {
        BoundsResult::Unsafe { reason } => {
            assert!(reason.contains("out of bounds"));
        }
        other => panic!("expected Unsafe, got {:?}", other),
    }
}

#[test]
fn check_dynamic_index_negative_lower_bound() {
    let mut ctx = BoundsContext::new();
    ctx.set_length("items", 5);
    ctx.set_bounds("i", Some(-1), Some(3));
    match check_dynamic_index(&ctx, "items", "i") {
        BoundsResult::Unsafe { reason } => {
            assert!(reason.contains("out of bounds"));
        }
        other => panic!("expected Unsafe, got {:?}", other),
    }
}

#[test]
fn check_dynamic_index_unknown_bounds() {
    let ctx = BoundsContext::new();
    assert_eq!(
        check_dynamic_index(&ctx, "items", "i"),
        BoundsResult::Unknown
    );
}

#[test]
fn check_dynamic_index_partial_info() {
    let mut ctx = BoundsContext::new();
    ctx.set_length("items", 10);
    // Only lower bound on index, no upper
    ctx.set_bounds("i", Some(0), None);
    assert_eq!(
        check_dynamic_index(&ctx, "items", "i"),
        BoundsResult::Unknown
    );
}

// ── infer_length_from_condition ─────────────────────────────────────

#[test]
fn infer_length_gt_zero() {
    // len(x) > 0  =>  min length 1
    let info = infer_length_from_condition("x", &BoundsOp::Gt, 0).unwrap();
    assert_eq!(info.lower, Some(1));
    assert_eq!(info.upper, None);
    assert_eq!(info.exact_length, None);
}

#[test]
fn infer_length_eq_three() {
    // len(x) == 3  =>  exact length 3
    let info = infer_length_from_condition("x", &BoundsOp::Eq, 3).unwrap();
    assert_eq!(info.exact_length, Some(3));
    assert_eq!(info.lower, Some(3));
    assert_eq!(info.upper, Some(3));
}

#[test]
fn infer_length_ge_five() {
    // len(x) >= 5  =>  min length 5
    let info = infer_length_from_condition("x", &BoundsOp::Ge, 5).unwrap();
    assert_eq!(info.lower, Some(5));
    assert_eq!(info.upper, None);
}

#[test]
fn infer_length_lt_ten() {
    // len(x) < 10  =>  max length 9
    let info = infer_length_from_condition("x", &BoundsOp::Lt, 10).unwrap();
    assert_eq!(info.upper, Some(9));
    assert_eq!(info.lower, None);
}

#[test]
fn infer_length_le_seven() {
    // len(x) <= 7  =>  max length 7
    let info = infer_length_from_condition("x", &BoundsOp::Le, 7).unwrap();
    assert_eq!(info.upper, Some(7));
}

#[test]
fn infer_length_ne_returns_none() {
    // len(x) != 5  =>  no useful bounds
    assert!(infer_length_from_condition("x", &BoundsOp::Ne, 5).is_none());
}

#[test]
fn infer_length_negative_value_returns_none() {
    // len(x) > -1 is trivially true for any collection, not useful
    // len(x) < 0 is impossible
    assert!(infer_length_from_condition("x", &BoundsOp::Lt, 0).is_none());
    assert!(infer_length_from_condition("x", &BoundsOp::Eq, -1).is_none());
}

// ── generate_diagnostic ─────────────────────────────────────────────

#[test]
fn diagnostic_safe_returns_none() {
    let check = IndexCheck {
        collection: "items".to_string(),
        index_expr: "0".to_string(),
        index_value: Some(0),
        collection_length: None,
        result: BoundsResult::Safe,
    };
    assert!(generate_diagnostic(&check, 10).is_none());
}

#[test]
fn diagnostic_unsafe_returns_some_with_error() {
    let check = IndexCheck {
        collection: "items".to_string(),
        index_expr: "5".to_string(),
        index_value: Some(5),
        collection_length: Some(BoundsInfo::with_exact_length("items", 3)),
        result: BoundsResult::Unsafe {
            reason: "index 5 out of bounds for length 3".to_string(),
        },
    };
    let diag = generate_diagnostic(&check, 42).unwrap();
    assert_eq!(diag.collection, "items");
    assert_eq!(diag.index, "5");
    assert_eq!(diag.line, 42);
    assert!(diag.suggestion.is_some());
    assert!(diag.suggestion.as_ref().unwrap().contains("out of bounds"));
}

#[test]
fn diagnostic_unknown_returns_some_with_warning() {
    let check = IndexCheck {
        collection: "data".to_string(),
        index_expr: "i".to_string(),
        index_value: None,
        collection_length: None,
        result: BoundsResult::Unknown,
    };
    let diag = generate_diagnostic(&check, 7).unwrap();
    assert_eq!(diag.collection, "data");
    assert_eq!(diag.line, 7);
    assert!(diag.suggestion.as_ref().unwrap().contains("cannot prove"));
    assert!(diag.suggestion.as_ref().unwrap().contains("length check"));
}

#[test]
fn diagnostic_conditional_safe_returns_info() {
    let check = IndexCheck {
        collection: "items".to_string(),
        index_expr: "0".to_string(),
        index_value: Some(0),
        collection_length: None,
        result: BoundsResult::ConditionalSafe {
            condition: "len(items) > 0".to_string(),
        },
    };
    let diag = generate_diagnostic(&check, 15).unwrap();
    assert!(diag.suggestion.as_ref().unwrap().contains("safe only when"));
}

// ── BoundsResult variants ───────────────────────────────────────────

#[test]
fn bounds_result_safe_equality() {
    assert_eq!(BoundsResult::Safe, BoundsResult::Safe);
}

#[test]
fn bounds_result_unsafe_contains_reason() {
    let r = BoundsResult::Unsafe {
        reason: "test reason".to_string(),
    };
    if let BoundsResult::Unsafe { reason } = &r {
        assert_eq!(reason, "test reason");
    } else {
        panic!("expected Unsafe");
    }
}

#[test]
fn bounds_result_unknown_equality() {
    assert_eq!(BoundsResult::Unknown, BoundsResult::Unknown);
}

#[test]
fn bounds_result_conditional_safe() {
    let r = BoundsResult::ConditionalSafe {
        condition: "x > 0".to_string(),
    };
    if let BoundsResult::ConditionalSafe { condition } = &r {
        assert_eq!(condition, "x > 0");
    } else {
        panic!("expected ConditionalSafe");
    }
}

// ── Push condition with negation ────────────────────────────────────

#[test]
fn push_negated_condition_flips_operator() {
    let mut ctx = BoundsContext::new();
    // Negated x > 5 means x <= 5
    ctx.push_condition(ActiveCondition {
        variable: "x".to_string(),
        op: BoundsOp::Gt,
        value: 5,
        negated: true,
    });
    let info = ctx.known_bounds.get("x").unwrap();
    // NOT (x > 5) => x <= 5
    assert_eq!(info.upper, Some(5));
}

// ── Compound scenario ───────────────────────────────────────────────

#[test]
fn compound_scenario_if_len_then_access() {
    // Simulate: if len(items) > 0 then items[0]
    let mut ctx = BoundsContext::new();

    // We learn from the condition len(items) > 0
    let inferred = infer_length_from_condition("items", &BoundsOp::Gt, 0).unwrap();
    // Apply the inferred info to the context
    if let Some(lower) = inferred.lower {
        ctx.set_bounds("items", Some(lower), None);
    }

    // Now check items[0]
    let result = check_index_access(&ctx, "items", 0);
    assert_eq!(result, BoundsResult::Safe);
}

#[test]
fn compound_scenario_no_guard_unknown() {
    // Without any guard, items[0] is unknown
    let ctx = BoundsContext::new();
    let result = check_index_access(&ctx, "items", 0);
    assert_eq!(result, BoundsResult::Unknown);
}

// ── BoundsInfo construction ─────────────────────────────────────────

#[test]
fn bounds_info_with_bounds_constructor() {
    let info = BoundsInfo::with_bounds("x", Some(1), Some(10));
    assert_eq!(info.variable, "x");
    assert_eq!(info.lower, Some(1));
    assert_eq!(info.upper, Some(10));
    assert_eq!(info.exact_length, None);
}

// ── IndexCheck struct ───────────────────────────────────────────────

#[test]
fn index_check_construction() {
    let check = IndexCheck {
        collection: "data".to_string(),
        index_expr: "idx".to_string(),
        index_value: Some(3),
        collection_length: Some(BoundsInfo::with_exact_length("data", 10)),
        result: BoundsResult::Safe,
    };
    assert_eq!(check.collection, "data");
    assert_eq!(check.index_value, Some(3));
    assert!(check.collection_length.is_some());
}
