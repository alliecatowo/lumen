//! Wave 26 — verification parity checklist tests.
//!
//! Validates that the `parity_verification` module produces a correct,
//! internally-consistent checklist with the expected categories, statuses,
//! and item counts.

use lumen_compiler::compiler::verification::parity_verification::{
    VerifParityStatus, VerificationCategory, VerificationParityChecklist,
};

// ── Helpers ────────────────────────────────────────────────────────

fn checklist() -> VerificationParityChecklist {
    VerificationParityChecklist::build()
}

// ── Build & basic invariants ───────────────────────────────────────

#[test]
fn wave26_parity_verification_build_succeeds() {
    let cl = checklist();
    assert!(!cl.is_empty());
}

#[test]
fn wave26_parity_verification_at_least_35_items() {
    let cl = checklist();
    assert!(cl.len() >= 35, "expected >= 35 items, got {}", cl.len());
}

#[test]
fn wave26_parity_verification_exactly_42_items() {
    let cl = checklist();
    assert_eq!(cl.len(), 42, "expected 42 items, got {}", cl.len());
}

#[test]
fn wave26_parity_verification_validation_passes() {
    let cl = checklist();
    let errors = cl.validate();
    assert!(errors.is_empty(), "validation errors: {:?}", errors);
}

#[test]
fn wave26_parity_verification_default_equals_build() {
    let default = VerificationParityChecklist::default();
    let built = checklist();
    assert_eq!(default.len(), built.len());
}

// ── ID format and uniqueness ───────────────────────────────────────

#[test]
fn wave26_parity_verification_all_ids_start_with_vp() {
    let cl = checklist();
    for item in &cl.items {
        assert!(
            item.id.starts_with("VP-"),
            "ID '{}' does not start with VP-",
            item.id
        );
    }
}

#[test]
fn wave26_parity_verification_all_ids_unique() {
    let cl = checklist();
    let mut seen = std::collections::HashSet::new();
    for item in &cl.items {
        assert!(seen.insert(&item.id), "Duplicate ID: {}", item.id);
    }
}

#[test]
fn wave26_parity_verification_ids_sequential() {
    let cl = checklist();
    for (i, item) in cl.items.iter().enumerate() {
        let expected = format!("VP-{:03}", i + 1);
        assert_eq!(
            item.id, expected,
            "Item {} has id '{}', expected '{}'",
            i, item.id, expected
        );
    }
}

// ── Non-empty field checks ─────────────────────────────────────────

#[test]
fn wave26_parity_verification_no_empty_features() {
    let cl = checklist();
    for item in &cl.items {
        assert!(!item.feature.is_empty(), "{}: empty feature name", item.id);
    }
}

#[test]
fn wave26_parity_verification_no_empty_descriptions() {
    let cl = checklist();
    for item in &cl.items {
        assert!(
            !item.description.is_empty(),
            "{}: empty description",
            item.id
        );
    }
}

#[test]
fn wave26_parity_verification_no_empty_comparable_to() {
    let cl = checklist();
    for item in &cl.items {
        assert!(
            !item.comparable_to.is_empty(),
            "{}: empty comparable_to",
            item.id
        );
    }
}

#[test]
fn wave26_parity_verification_no_empty_lumen_approach() {
    let cl = checklist();
    for item in &cl.items {
        assert!(
            !item.lumen_approach.is_empty(),
            "{}: empty lumen_approach",
            item.id
        );
    }
}

// ── Category coverage ──────────────────────────────────────────────

#[test]
fn wave26_parity_verification_all_18_categories_covered() {
    let cl = checklist();
    let categories: std::collections::HashSet<_> = cl.items.iter().map(|i| i.category).collect();
    assert_eq!(
        categories.len(),
        18,
        "expected 18 categories, got {}",
        categories.len()
    );
}

#[test]
fn wave26_parity_verification_type_safety_items() {
    let cl = checklist();
    let items = cl.items_by_category(VerificationCategory::TypeSafety);
    assert!(
        items.len() >= 3,
        "expected >= 3 TypeSafety items, got {}",
        items.len()
    );
}

#[test]
fn wave26_parity_verification_refinement_types_items() {
    let cl = checklist();
    let items = cl.items_by_category(VerificationCategory::RefinementTypes);
    assert!(
        items.len() >= 3,
        "expected >= 3 RefinementTypes items, got {}",
        items.len()
    );
}

#[test]
fn wave26_parity_verification_effect_tracking_items() {
    let cl = checklist();
    let items = cl.items_by_category(VerificationCategory::EffectTracking);
    assert!(
        items.len() >= 3,
        "expected >= 3 EffectTracking items, got {}",
        items.len()
    );
}

#[test]
fn wave26_parity_verification_smt_integration_items() {
    let cl = checklist();
    let items = cl.items_by_category(VerificationCategory::SmtIntegration);
    assert!(
        items.len() >= 4,
        "expected >= 4 SmtIntegration items, got {}",
        items.len()
    );
}

#[test]
fn wave26_parity_verification_dependent_types_item() {
    let cl = checklist();
    let items = cl.items_by_category(VerificationCategory::DependentTypes);
    assert_eq!(items.len(), 1, "expected exactly 1 DependentTypes item");
}

// ── Status distribution ────────────────────────────────────────────

#[test]
fn wave26_parity_verification_has_implemented_items() {
    let cl = checklist();
    let impl_items = cl.implemented_items();
    assert!(
        impl_items.len() >= 20,
        "expected >= 20 implemented items, got {}",
        impl_items.len()
    );
}

#[test]
fn wave26_parity_verification_has_partial_items() {
    let cl = checklist();
    let partial = cl
        .items
        .iter()
        .filter(|i| matches!(i.status, VerifParityStatus::Partial(_)))
        .count();
    assert!(
        partial >= 1,
        "expected at least 1 partial item, got {}",
        partial
    );
}

#[test]
fn wave26_parity_verification_has_designed_items() {
    let cl = checklist();
    let designed = cl
        .items
        .iter()
        .filter(|i| i.status == VerifParityStatus::Designed)
        .count();
    assert!(
        designed >= 1,
        "expected at least 1 designed item, got {}",
        designed
    );
}

#[test]
fn wave26_parity_verification_has_not_applicable_items() {
    let cl = checklist();
    let na = cl.not_applicable_items();
    assert!(
        na.len() >= 1,
        "expected at least 1 N/A item, got {}",
        na.len()
    );
}

#[test]
fn wave26_parity_verification_pending_items() {
    let cl = checklist();
    let pending = cl.pending_items();
    // Pending = Partial + Designed
    let partial = cl
        .items
        .iter()
        .filter(|i| matches!(i.status, VerifParityStatus::Partial(_)))
        .count();
    let designed = cl
        .items
        .iter()
        .filter(|i| i.status == VerifParityStatus::Designed)
        .count();
    assert_eq!(
        pending.len(),
        partial + designed,
        "pending_items should equal Partial + Designed"
    );
}

#[test]
fn wave26_parity_verification_status_counts_sum_to_total() {
    let cl = checklist();
    let summary = cl.summary();
    let sum = summary.implemented + summary.partial + summary.designed + summary.not_applicable;
    assert_eq!(sum, summary.total, "status counts must sum to total");
}

// ── find_by_id ─────────────────────────────────────────────────────

#[test]
fn wave26_parity_verification_find_by_id_vp001() {
    let cl = checklist();
    let item = cl.find_by_id("VP-001");
    assert!(item.is_some(), "VP-001 should exist");
    let item = item.unwrap();
    assert_eq!(item.category, VerificationCategory::TypeSafety);
    assert!(item.feature.contains("Hindley-Milner"));
}

#[test]
fn wave26_parity_verification_find_by_id_vp042() {
    let cl = checklist();
    let item = cl.find_by_id("VP-042");
    assert!(item.is_some(), "VP-042 should exist");
    let item = item.unwrap();
    assert_eq!(item.category, VerificationCategory::ResourceAccounting);
}

#[test]
fn wave26_parity_verification_find_by_id_missing() {
    let cl = checklist();
    assert!(cl.find_by_id("VP-999").is_none());
}

#[test]
fn wave26_parity_verification_find_by_id_empty() {
    let cl = checklist();
    assert!(cl.find_by_id("").is_none());
}

// ── Specific item content checks ───────────────────────────────────

#[test]
fn wave26_parity_verification_vp008_is_not_applicable() {
    let cl = checklist();
    let item = cl.find_by_id("VP-008").unwrap();
    assert!(
        matches!(item.status, VerifParityStatus::NotApplicable(_)),
        "VP-008 (dependent types) should be N/A"
    );
}

#[test]
fn wave26_parity_verification_vp020_is_implemented() {
    let cl = checklist();
    let item = cl.find_by_id("VP-020").unwrap();
    assert_eq!(
        item.status,
        VerifParityStatus::Implemented,
        "VP-020 (row-polymorphic effects) should be implemented"
    );
    assert_eq!(item.category, VerificationCategory::EffectTracking);
}

#[test]
fn wave26_parity_verification_vp031_z3_backend() {
    let cl = checklist();
    let item = cl.find_by_id("VP-031").unwrap();
    assert!(item.feature.contains("Z3"), "VP-031 should reference Z3");
    assert_eq!(item.category, VerificationCategory::SmtIntegration);
    assert_eq!(item.status, VerifParityStatus::Implemented);
}

#[test]
fn wave26_parity_verification_vp013_postcondition_designed() {
    let cl = checklist();
    let item = cl.find_by_id("VP-013").unwrap();
    assert_eq!(
        item.status,
        VerifParityStatus::Designed,
        "VP-013 postconditions should be Designed"
    );
}

#[test]
fn wave26_parity_verification_vp027_model_checking_partial() {
    let cl = checklist();
    let item = cl.find_by_id("VP-027").unwrap();
    assert!(
        matches!(item.status, VerifParityStatus::Partial(_)),
        "VP-027 (state-machine reachability) should be Partial"
    );
}

// ── Summary ────────────────────────────────────────────────────────

#[test]
fn wave26_parity_verification_summary_total() {
    let cl = checklist();
    let summary = cl.summary();
    assert_eq!(summary.total, 42);
}

#[test]
fn wave26_parity_verification_summary_category_count() {
    let cl = checklist();
    let summary = cl.summary();
    assert_eq!(
        summary.category_count, 18,
        "expected 18 categories in summary"
    );
}

#[test]
fn wave26_parity_verification_summary_has_comparable_systems() {
    let cl = checklist();
    let summary = cl.summary();
    assert!(
        summary.comparable_system_count >= 10,
        "expected >= 10 comparable systems, got {}",
        summary.comparable_system_count
    );
}

#[test]
fn wave26_parity_verification_summary_display() {
    let cl = checklist();
    let summary = cl.summary();
    let display = format!("{}", summary);
    assert!(display.contains("Verification Parity"));
    assert!(display.contains("implemented"));
}

// ── Display impls ──────────────────────────────────────────────────

#[test]
fn wave26_parity_verification_category_display() {
    assert_eq!(
        format!("{}", VerificationCategory::TypeSafety),
        "Type Safety"
    );
    assert_eq!(
        format!("{}", VerificationCategory::SmtIntegration),
        "SMT Integration"
    );
    assert_eq!(
        format!("{}", VerificationCategory::PropertyBasedTesting),
        "Property-Based Testing"
    );
}

#[test]
fn wave26_parity_verification_status_display_implemented() {
    assert_eq!(format!("{}", VerifParityStatus::Implemented), "Implemented");
}

#[test]
fn wave26_parity_verification_status_display_partial() {
    let status = VerifParityStatus::Partial("missing X".into());
    let display = format!("{}", status);
    assert!(display.contains("Partial"));
    assert!(display.contains("missing X"));
}

#[test]
fn wave26_parity_verification_status_display_designed() {
    assert_eq!(format!("{}", VerifParityStatus::Designed), "Designed");
}

#[test]
fn wave26_parity_verification_status_display_not_applicable() {
    let status = VerifParityStatus::NotApplicable("out of scope".into());
    let display = format!("{}", status);
    assert!(display.contains("N/A"));
    assert!(display.contains("out of scope"));
}

#[test]
fn wave26_parity_verification_item_display() {
    let cl = checklist();
    let item = cl.find_by_id("VP-001").unwrap();
    let display = format!("{}", item);
    assert!(display.contains("[VP-001]"));
    assert!(display.contains("Hindley-Milner"));
}

// ── items_by_status ────────────────────────────────────────────────

#[test]
fn wave26_parity_verification_items_by_status_implemented() {
    let cl = checklist();
    let items = cl.items_by_status(&VerifParityStatus::Implemented);
    assert!(items.len() >= 20);
    for item in &items {
        assert_eq!(item.status, VerifParityStatus::Implemented);
    }
}

#[test]
fn wave26_parity_verification_items_by_status_designed() {
    let cl = checklist();
    let items = cl.items_by_status(&VerifParityStatus::Designed);
    assert!(items.len() >= 1);
    for item in &items {
        assert_eq!(item.status, VerifParityStatus::Designed);
    }
}
