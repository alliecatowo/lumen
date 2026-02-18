//! Wave-26 tests: T166 — Concurrency safety parity checklist.
//!
//! At least 35 tests covering every public type, method, and invariant
//! exposed by `lumen_rt::parity_concurrency`.

use lumen_rt::parity_concurrency::*;

// ===========================================================================
// ConcurrencyCategory — Display
// ===========================================================================

#[test]
fn parity_category_display_task_scheduling() {
    assert_eq!(
        ConcurrencyCategory::TaskScheduling.to_string(),
        "Task Scheduling"
    );
}

#[test]
fn parity_category_display_work_stealing() {
    assert_eq!(
        ConcurrencyCategory::WorkStealing.to_string(),
        "Work Stealing"
    );
}

#[test]
fn parity_category_display_channel_communication() {
    assert_eq!(
        ConcurrencyCategory::ChannelCommunication.to_string(),
        "Channel Communication"
    );
}

#[test]
fn parity_category_display_actor_model() {
    assert_eq!(ConcurrencyCategory::ActorModel.to_string(), "Actor Model");
}

#[test]
fn parity_category_display_all_variants() {
    // Ensures every variant has a non-empty Display impl.
    let cats = [
        ConcurrencyCategory::TaskScheduling,
        ConcurrencyCategory::WorkStealing,
        ConcurrencyCategory::ChannelCommunication,
        ConcurrencyCategory::ActorModel,
        ConcurrencyCategory::SupervisorTrees,
        ConcurrencyCategory::NurseryScoping,
        ConcurrencyCategory::FutureExecution,
        ConcurrencyCategory::ParallelPrimitives,
        ConcurrencyCategory::LockFreeStructures,
        ConcurrencyCategory::DataRaceProtection,
        ConcurrencyCategory::DeadlockPrevention,
        ConcurrencyCategory::ResourceOrdering,
        ConcurrencyCategory::CancellationSafety,
        ConcurrencyCategory::StructuredConcurrency,
        ConcurrencyCategory::BackpressureHandling,
    ];
    for cat in &cats {
        let s = cat.to_string();
        assert!(!s.is_empty(), "empty display for {:?}", cat);
    }
}

// ===========================================================================
// ConcurrencyCategory — trait impls
// ===========================================================================

#[test]
fn parity_category_clone_eq() {
    let a = ConcurrencyCategory::FutureExecution;
    let b = a;
    assert_eq!(a, b);
    #[allow(clippy::clone_on_copy)]
    let c = a.clone();
    assert_eq!(a, c);
}

#[test]
fn parity_category_debug() {
    let dbg = format!("{:?}", ConcurrencyCategory::DeadlockPrevention);
    assert!(dbg.contains("DeadlockPrevention"));
}

// ===========================================================================
// ConcParityStatus
// ===========================================================================

#[test]
fn parity_status_display_implemented() {
    assert_eq!(ConcParityStatus::Implemented.to_string(), "Implemented");
}

#[test]
fn parity_status_display_partial() {
    let s = ConcParityStatus::Partial("needs XYZ".into());
    assert_eq!(s.to_string(), "Partial — needs XYZ");
}

#[test]
fn parity_status_display_designed() {
    assert_eq!(ConcParityStatus::Designed.to_string(), "Designed");
}

#[test]
fn parity_status_display_not_applicable() {
    let s = ConcParityStatus::NotApplicable("no shared state".into());
    assert_eq!(s.to_string(), "N/A — no shared state");
}

#[test]
fn parity_status_is_implemented() {
    assert!(ConcParityStatus::Implemented.is_implemented());
    assert!(!ConcParityStatus::Designed.is_implemented());
    assert!(!ConcParityStatus::Partial("x".into()).is_implemented());
    assert!(!ConcParityStatus::NotApplicable("y".into()).is_implemented());
}

#[test]
fn parity_status_is_gap() {
    assert!(!ConcParityStatus::Implemented.is_gap());
    assert!(ConcParityStatus::Designed.is_gap());
    assert!(ConcParityStatus::Partial("wip".into()).is_gap());
    assert!(!ConcParityStatus::NotApplicable("n/a".into()).is_gap());
}

#[test]
fn parity_status_clone_eq() {
    let a = ConcParityStatus::Partial("test".into());
    let b = a.clone();
    assert_eq!(a, b);
}

// ===========================================================================
// ConcurrencyParityItem — Display and construction
// ===========================================================================

fn make_item(id: &str, status: ConcParityStatus) -> ConcurrencyParityItem {
    ConcurrencyParityItem {
        id: id.into(),
        category: ConcurrencyCategory::TaskScheduling,
        feature: "test feature".into(),
        description: "test description".into(),
        status,
        comparable_to: "Go".into(),
        lumen_approach: "effect system".into(),
    }
}

#[test]
fn parity_item_display_contains_id_and_feature() {
    let item = make_item("CONC-099", ConcParityStatus::Implemented);
    let s = item.to_string();
    assert!(s.contains("CONC-099"), "missing id in: {s}");
    assert!(s.contains("test feature"), "missing feature in: {s}");
    assert!(s.contains("Implemented"), "missing status in: {s}");
}

#[test]
fn parity_item_clone_eq() {
    let a = make_item("X", ConcParityStatus::Designed);
    let b = a.clone();
    assert_eq!(a, b);
}

#[test]
fn parity_item_debug() {
    let item = make_item("D", ConcParityStatus::Implemented);
    let dbg = format!("{:?}", item);
    assert!(dbg.contains("ConcurrencyParityItem"));
}

// ===========================================================================
// ConcurrencyParityChecklist — full_checklist
// ===========================================================================

#[test]
fn parity_full_checklist_at_least_30() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    assert!(cl.total_count() >= 30, "got {}", cl.total_count());
}

#[test]
fn parity_all_ids_unique() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    let mut seen = std::collections::HashSet::new();
    for item in &cl.items {
        assert!(seen.insert(item.id.clone()), "duplicate: {}", item.id);
    }
}

#[test]
fn parity_all_ids_start_with_conc() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    for item in &cl.items {
        assert!(item.id.starts_with("CONC-"), "bad id prefix: {}", item.id);
    }
}

#[test]
fn parity_no_empty_fields() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    for item in &cl.items {
        assert!(!item.feature.is_empty(), "empty feature on {}", item.id);
        assert!(
            !item.description.is_empty(),
            "empty description on {}",
            item.id
        );
        assert!(
            !item.comparable_to.is_empty(),
            "empty comparable_to on {}",
            item.id
        );
        assert!(
            !item.lumen_approach.is_empty(),
            "empty lumen_approach on {}",
            item.id
        );
    }
}

// ===========================================================================
// by_category
// ===========================================================================

#[test]
fn parity_by_category_task_scheduling() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    let items = cl.by_category(ConcurrencyCategory::TaskScheduling);
    assert!(!items.is_empty());
    for item in &items {
        assert_eq!(item.category, ConcurrencyCategory::TaskScheduling);
    }
}

#[test]
fn parity_by_category_future_execution() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    let items = cl.by_category(ConcurrencyCategory::FutureExecution);
    assert!(
        items.len() >= 3,
        "expected >=3 future items, got {}",
        items.len()
    );
}

#[test]
fn parity_by_category_parallel_primitives() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    let items = cl.by_category(ConcurrencyCategory::ParallelPrimitives);
    assert!(
        items.len() >= 4,
        "expected >=4 parallel items, got {}",
        items.len()
    );
}

#[test]
fn parity_by_category_returns_empty_for_unused() {
    // Build a checklist with a single item in one category.
    let cl = ConcurrencyParityChecklist {
        items: vec![make_item("T", ConcParityStatus::Implemented)],
    };
    let items = cl.by_category(ConcurrencyCategory::BackpressureHandling);
    assert!(items.is_empty());
}

// ===========================================================================
// implemented_count / total_count / coverage_percent
// ===========================================================================

#[test]
fn parity_implemented_count_le_total() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    assert!(cl.implemented_count() <= cl.total_count());
}

#[test]
fn parity_coverage_percent_in_range() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    let pct = cl.coverage_percent();
    assert!((0.0..=100.0).contains(&pct));
}

#[test]
fn parity_coverage_100_when_all_implemented() {
    let cl = ConcurrencyParityChecklist {
        items: vec![
            make_item("A", ConcParityStatus::Implemented),
            make_item("B", ConcParityStatus::Implemented),
        ],
    };
    assert!((cl.coverage_percent() - 100.0).abs() < f64::EPSILON);
}

#[test]
fn parity_coverage_0_when_none_implemented() {
    let cl = ConcurrencyParityChecklist {
        items: vec![
            make_item("A", ConcParityStatus::Designed),
            make_item("B", ConcParityStatus::Partial("wip".into())),
        ],
    };
    assert!(cl.coverage_percent().abs() < f64::EPSILON);
}

#[test]
fn parity_coverage_0_when_empty() {
    let cl = ConcurrencyParityChecklist { items: vec![] };
    assert!(cl.coverage_percent().abs() < f64::EPSILON);
    assert_eq!(cl.implemented_count(), 0);
    assert_eq!(cl.total_count(), 0);
}

// ===========================================================================
// gaps
// ===========================================================================

#[test]
fn parity_gaps_excludes_implemented() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    let gaps = cl.gaps();
    for item in &gaps {
        assert!(
            !item.status.is_implemented(),
            "gap should not be implemented: {}",
            item.id
        );
    }
}

#[test]
fn parity_gaps_excludes_not_applicable() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    let gaps = cl.gaps();
    for item in &gaps {
        assert!(
            !matches!(item.status, ConcParityStatus::NotApplicable(_)),
            "gap should not be N/A: {}",
            item.id,
        );
    }
}

#[test]
fn parity_gaps_plus_implemented_plus_na_equals_total() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    let gap_count = cl.gaps().len();
    let impl_count = cl.implemented_count();
    let na_count = cl
        .items
        .iter()
        .filter(|i| matches!(i.status, ConcParityStatus::NotApplicable(_)))
        .count();
    assert_eq!(gap_count + impl_count + na_count, cl.total_count());
}

// ===========================================================================
// to_markdown
// ===========================================================================

#[test]
fn parity_to_markdown_contains_header() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    let md = cl.to_markdown();
    assert!(md.contains("# Concurrency Safety Parity Checklist"));
}

#[test]
fn parity_to_markdown_contains_table_header() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    let md = cl.to_markdown();
    assert!(md.contains("| ID |"));
    assert!(md.contains("| Category |") || md.contains("Category"));
}

#[test]
fn parity_to_markdown_contains_all_ids() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    let md = cl.to_markdown();
    for item in &cl.items {
        assert!(md.contains(&item.id), "markdown missing id {}", item.id);
    }
}

#[test]
fn parity_to_markdown_contains_coverage_stat() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    let md = cl.to_markdown();
    assert!(
        md.contains("Coverage:"),
        "markdown should contain coverage line"
    );
}

// ===========================================================================
// summary
// ===========================================================================

#[test]
fn parity_summary_contains_counts() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    let s = cl.summary();
    assert!(s.contains("Concurrency parity:"));
    assert!(s.contains("implemented"));
    assert!(s.contains("gaps"));
    assert!(s.contains("N/A"));
}

#[test]
fn parity_summary_matches_numbers() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    let s = cl.summary();
    let impl_str = format!("{}/{}", cl.implemented_count(), cl.total_count());
    assert!(
        s.contains(&impl_str),
        "summary should contain {impl_str}: {s}"
    );
}

// ===========================================================================
// Category coverage — at least one item per category
// ===========================================================================

#[test]
fn parity_every_category_has_items() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    let cats = [
        ConcurrencyCategory::TaskScheduling,
        ConcurrencyCategory::WorkStealing,
        ConcurrencyCategory::ChannelCommunication,
        ConcurrencyCategory::ActorModel,
        ConcurrencyCategory::SupervisorTrees,
        ConcurrencyCategory::NurseryScoping,
        ConcurrencyCategory::FutureExecution,
        ConcurrencyCategory::ParallelPrimitives,
        ConcurrencyCategory::LockFreeStructures,
        ConcurrencyCategory::DataRaceProtection,
        ConcurrencyCategory::DeadlockPrevention,
        ConcurrencyCategory::ResourceOrdering,
        ConcurrencyCategory::CancellationSafety,
        ConcurrencyCategory::StructuredConcurrency,
        ConcurrencyCategory::BackpressureHandling,
    ];
    for cat in &cats {
        let items = cl.by_category(*cat);
        assert!(!items.is_empty(), "no items for category {:?}", cat);
    }
}

// ===========================================================================
// Specific feature presence
// ===========================================================================

#[test]
fn parity_has_parallel_combinator() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    assert!(
        cl.items.iter().any(|i| i.feature.contains("parallel")),
        "missing parallel combinator item"
    );
}

#[test]
fn parity_has_race_combinator() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    assert!(
        cl.items.iter().any(|i| i.feature.contains("race")),
        "missing race combinator item"
    );
}

#[test]
fn parity_has_vote_combinator() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    assert!(
        cl.items.iter().any(|i| i.feature.contains("vote")),
        "missing vote combinator item"
    );
}

#[test]
fn parity_has_timeout_combinator() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    assert!(
        cl.items.iter().any(|i| i.feature.contains("timeout")),
        "missing timeout combinator item"
    );
}

#[test]
fn parity_has_deterministic_scheduling() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    assert!(
        cl.items
            .iter()
            .any(|i| i.feature.to_lowercase().contains("deterministic")),
        "missing deterministic scheduling item"
    );
}

#[test]
fn parity_has_process_isolation() {
    let cl = ConcurrencyParityChecklist::full_checklist();
    assert!(
        cl.items
            .iter()
            .any(|i| i.feature.to_lowercase().contains("isolation")),
        "missing process isolation item"
    );
}
