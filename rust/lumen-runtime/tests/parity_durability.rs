//! Integration tests for `lumen_runtime::parity_durability` (T168: Durability parity checklist).
//!
//! Covers DurabilityParityChecklist, DurabilityParityItem, DurabilityCategory,
//! DurabParityStatus, filtering, counting, markdown output, and summary.

use lumen_runtime::parity_durability::*;

// ===========================================================================
// DurabilityParityChecklist — full_checklist construction
// ===========================================================================

#[test]
fn wave26_parity_full_checklist_at_least_30_items() {
    let cl = DurabilityParityChecklist::full_checklist();
    assert!(
        cl.total_count() >= 30,
        "expected >= 30 items, got {}",
        cl.total_count()
    );
}

#[test]
fn wave26_parity_all_ids_unique() {
    let cl = DurabilityParityChecklist::full_checklist();
    let mut ids: Vec<&str> = cl.items.iter().map(|i| i.id.as_str()).collect();
    let before = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(before, ids.len(), "duplicate IDs found in checklist");
}

#[test]
fn wave26_parity_all_fields_nonempty() {
    let cl = DurabilityParityChecklist::full_checklist();
    for item in &cl.items {
        assert!(!item.id.is_empty());
        assert!(!item.feature.is_empty());
        assert!(!item.description.is_empty());
        assert!(!item.comparable_to.is_empty());
        assert!(!item.lumen_approach.is_empty());
    }
}

#[test]
fn wave26_parity_ids_follow_dur_prefix() {
    let cl = DurabilityParityChecklist::full_checklist();
    for item in &cl.items {
        assert!(
            item.id.starts_with("DUR-"),
            "id {} should start with DUR-",
            item.id
        );
    }
}

// ===========================================================================
// DurabilityParityChecklist — counting
// ===========================================================================

#[test]
fn wave26_parity_implemented_count_positive() {
    let cl = DurabilityParityChecklist::full_checklist();
    assert!(cl.implemented_count() > 0);
}

#[test]
fn wave26_parity_total_count_equals_items_len() {
    let cl = DurabilityParityChecklist::full_checklist();
    assert_eq!(cl.total_count(), cl.items.len());
}

#[test]
fn wave26_parity_coverage_percent_in_range() {
    let cl = DurabilityParityChecklist::full_checklist();
    let pct = cl.coverage_percent();
    assert!(pct > 0.0 && pct <= 100.0, "coverage {pct} out of range");
}

#[test]
fn wave26_parity_coverage_formula_correct() {
    let cl = DurabilityParityChecklist::full_checklist();
    let expected = (cl.implemented_count() as f64 / cl.total_count() as f64) * 100.0;
    assert!(
        (cl.coverage_percent() - expected).abs() < f64::EPSILON,
        "coverage percent does not match formula"
    );
}

// ===========================================================================
// DurabilityParityChecklist — gaps
// ===========================================================================

#[test]
fn wave26_parity_gaps_all_are_gaps() {
    let cl = DurabilityParityChecklist::full_checklist();
    for gap in cl.gaps() {
        assert!(gap.status.is_gap(), "{} should be a gap", gap.id);
    }
}

#[test]
fn wave26_parity_gaps_plus_impl_plus_na_equals_total() {
    let cl = DurabilityParityChecklist::full_checklist();
    let gaps = cl.gaps().len();
    let implemented = cl.implemented_count();
    let na = cl
        .items
        .iter()
        .filter(|i| matches!(i.status, DurabParityStatus::NotApplicable(_)))
        .count();
    assert_eq!(gaps + implemented + na, cl.total_count());
}

#[test]
fn wave26_parity_has_some_gaps() {
    let cl = DurabilityParityChecklist::full_checklist();
    assert!(
        !cl.gaps().is_empty(),
        "expected some gap items in checklist"
    );
}

// ===========================================================================
// DurabilityParityChecklist — by_category
// ===========================================================================

#[test]
fn wave26_parity_by_category_checkpointing_nonempty() {
    let cl = DurabilityParityChecklist::full_checklist();
    let items = cl.by_category(DurabilityCategory::Checkpointing);
    assert!(!items.is_empty());
    for item in &items {
        assert_eq!(item.category, DurabilityCategory::Checkpointing);
    }
}

#[test]
fn wave26_parity_by_category_replay_nonempty() {
    let cl = DurabilityParityChecklist::full_checklist();
    assert!(!cl.by_category(DurabilityCategory::Replay).is_empty());
}

#[test]
fn wave26_parity_by_category_event_sourcing_nonempty() {
    let cl = DurabilityParityChecklist::full_checklist();
    assert!(!cl.by_category(DurabilityCategory::EventSourcing).is_empty());
}

#[test]
fn wave26_parity_by_category_time_travel_debug_nonempty() {
    let cl = DurabilityParityChecklist::full_checklist();
    assert!(!cl
        .by_category(DurabilityCategory::TimeTravelDebug)
        .is_empty());
}

#[test]
fn wave26_parity_by_category_crash_recovery_nonempty() {
    let cl = DurabilityParityChecklist::full_checklist();
    assert!(!cl.by_category(DurabilityCategory::CrashRecovery).is_empty());
}

#[test]
fn wave26_parity_by_category_saga_pattern_nonempty() {
    let cl = DurabilityParityChecklist::full_checklist();
    assert!(!cl.by_category(DurabilityCategory::SagaPattern).is_empty());
}

#[test]
fn wave26_parity_all_15_categories_covered() {
    let cl = DurabilityParityChecklist::full_checklist();
    let all_cats = [
        DurabilityCategory::Checkpointing,
        DurabilityCategory::Replay,
        DurabilityCategory::EventSourcing,
        DurabilityCategory::Snapshotting,
        DurabilityCategory::WriteAheadLog,
        DurabilityCategory::TimeTravelDebug,
        DurabilityCategory::VersionedState,
        DurabilityCategory::SchemaEvolution,
        DurabilityCategory::CrashRecovery,
        DurabilityCategory::ExactlyOnceSemantics,
        DurabilityCategory::IdempotencyKeys,
        DurabilityCategory::DurableTimers,
        DurabilityCategory::SagaPattern,
        DurabilityCategory::CompensatingTransactions,
        DurabilityCategory::AuditLogging,
    ];
    for cat in &all_cats {
        assert!(
            !cl.by_category(*cat).is_empty(),
            "category {:?} should have at least one item",
            cat
        );
    }
}

#[test]
fn wave26_parity_by_category_nonexistent_returns_empty_on_filtered_list() {
    // Test with an empty checklist to show by_category returns empty.
    let cl = DurabilityParityChecklist { items: vec![] };
    assert!(cl.by_category(DurabilityCategory::SagaPattern).is_empty());
}

// ===========================================================================
// DurabilityParityChecklist — markdown output
// ===========================================================================

#[test]
fn wave26_parity_to_markdown_contains_header() {
    let cl = DurabilityParityChecklist::full_checklist();
    let md = cl.to_markdown();
    assert!(md.contains("# Durability Parity Checklist"));
}

#[test]
fn wave26_parity_to_markdown_contains_coverage() {
    let cl = DurabilityParityChecklist::full_checklist();
    let md = cl.to_markdown();
    assert!(md.contains("**Coverage**"));
    assert!(md.contains('%'));
}

#[test]
fn wave26_parity_to_markdown_contains_table_headers() {
    let cl = DurabilityParityChecklist::full_checklist();
    let md = cl.to_markdown();
    assert!(md.contains("| ID |"));
    assert!(md.contains("| Category |"));
    assert!(md.contains("| Feature |"));
    assert!(md.contains("| Status |"));
    assert!(md.contains("| Comparable To |"));
}

#[test]
fn wave26_parity_to_markdown_contains_all_ids() {
    let cl = DurabilityParityChecklist::full_checklist();
    let md = cl.to_markdown();
    for item in &cl.items {
        assert!(md.contains(&item.id), "markdown missing id {}", item.id);
    }
}

// ===========================================================================
// DurabilityParityChecklist — summary
// ===========================================================================

#[test]
fn wave26_parity_summary_contains_key_terms() {
    let cl = DurabilityParityChecklist::full_checklist();
    let s = cl.summary();
    assert!(s.contains("Durability Parity:"));
    assert!(s.contains("implemented"));
    assert!(s.contains("partial"));
    assert!(s.contains("designed"));
    assert!(s.contains("N/A"));
}

#[test]
fn wave26_parity_summary_contains_percentage() {
    let cl = DurabilityParityChecklist::full_checklist();
    let s = cl.summary();
    assert!(s.contains('%'));
}

// ===========================================================================
// DurabParityStatus — Display and predicates
// ===========================================================================

#[test]
fn wave26_parity_status_display_implemented() {
    assert_eq!(DurabParityStatus::Implemented.to_string(), "Implemented");
}

#[test]
fn wave26_parity_status_display_partial() {
    let s = DurabParityStatus::Partial("half done".into());
    assert_eq!(s.to_string(), "Partial: half done");
}

#[test]
fn wave26_parity_status_display_designed() {
    assert_eq!(DurabParityStatus::Designed.to_string(), "Designed");
}

#[test]
fn wave26_parity_status_display_na() {
    let s = DurabParityStatus::NotApplicable("not needed".into());
    assert_eq!(s.to_string(), "N/A: not needed");
}

#[test]
fn wave26_parity_status_is_implemented() {
    assert!(DurabParityStatus::Implemented.is_implemented());
    assert!(!DurabParityStatus::Partial("x".into()).is_implemented());
    assert!(!DurabParityStatus::Designed.is_implemented());
    assert!(!DurabParityStatus::NotApplicable("y".into()).is_implemented());
}

#[test]
fn wave26_parity_status_is_gap() {
    assert!(!DurabParityStatus::Implemented.is_gap());
    assert!(DurabParityStatus::Partial("x".into()).is_gap());
    assert!(DurabParityStatus::Designed.is_gap());
    assert!(!DurabParityStatus::NotApplicable("y".into()).is_gap());
}

// ===========================================================================
// DurabilityCategory — Display
// ===========================================================================

#[test]
fn wave26_parity_category_display_all_nonempty() {
    let categories = [
        DurabilityCategory::Checkpointing,
        DurabilityCategory::Replay,
        DurabilityCategory::EventSourcing,
        DurabilityCategory::Snapshotting,
        DurabilityCategory::WriteAheadLog,
        DurabilityCategory::TimeTravelDebug,
        DurabilityCategory::VersionedState,
        DurabilityCategory::SchemaEvolution,
        DurabilityCategory::CrashRecovery,
        DurabilityCategory::ExactlyOnceSemantics,
        DurabilityCategory::IdempotencyKeys,
        DurabilityCategory::DurableTimers,
        DurabilityCategory::SagaPattern,
        DurabilityCategory::CompensatingTransactions,
        DurabilityCategory::AuditLogging,
    ];
    for cat in &categories {
        assert!(!cat.to_string().is_empty(), "{:?} has empty display", cat);
    }
}

#[test]
fn wave26_parity_category_equality() {
    assert_eq!(
        DurabilityCategory::Checkpointing,
        DurabilityCategory::Checkpointing
    );
    assert_ne!(
        DurabilityCategory::Checkpointing,
        DurabilityCategory::Replay
    );
}

// ===========================================================================
// Edge cases — empty checklist
// ===========================================================================

#[test]
fn wave26_parity_empty_checklist_coverage_zero() {
    let cl = DurabilityParityChecklist { items: vec![] };
    assert!((cl.coverage_percent() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn wave26_parity_empty_checklist_counts_zero() {
    let cl = DurabilityParityChecklist { items: vec![] };
    assert_eq!(cl.implemented_count(), 0);
    assert_eq!(cl.total_count(), 0);
    assert!(cl.gaps().is_empty());
}
