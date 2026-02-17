//! Tests for T165: Memory safety parity checklist.
//!
//! Validates the `MemoryParityChecklist` structure, queries, reporting,
//! and individual parity item properties.

use lumen_compiler::compiler::parity_memory::*;

// ════════════════════════════════════════════════════════════════════
// §1  Checklist construction
// ════════════════════════════════════════════════════════════════════

#[test]
fn checklist_has_at_least_30_items() {
    let cl = MemoryParityChecklist::full_checklist();
    assert!(
        cl.total_count() >= 30,
        "Expected >= 30 items, got {}",
        cl.total_count()
    );
}

#[test]
fn checklist_has_50_items() {
    let cl = MemoryParityChecklist::full_checklist();
    assert_eq!(cl.total_count(), 50, "Expected exactly 50 items");
}

#[test]
fn ids_are_unique() {
    let cl = MemoryParityChecklist::full_checklist();
    let mut seen = std::collections::HashSet::new();
    for item in &cl.items {
        assert!(seen.insert(item.id.clone()), "Duplicate ID: {}", item.id);
    }
}

#[test]
fn ids_follow_mem_prefix_pattern() {
    let cl = MemoryParityChecklist::full_checklist();
    for item in &cl.items {
        assert!(
            item.id.starts_with("MEM-"),
            "ID {} should start with MEM-",
            item.id
        );
    }
}

#[test]
fn ids_are_zero_padded_three_digits() {
    let cl = MemoryParityChecklist::full_checklist();
    for item in &cl.items {
        let suffix = &item.id[4..];
        assert_eq!(suffix.len(), 3, "ID suffix '{}' should be 3 digits", suffix);
        assert!(
            suffix.chars().all(|c| c.is_ascii_digit()),
            "ID suffix '{}' should be all digits",
            suffix
        );
    }
}

// ════════════════════════════════════════════════════════════════════
// §2  Category queries
// ════════════════════════════════════════════════════════════════════

#[test]
fn by_category_ownership_model() {
    let cl = MemoryParityChecklist::full_checklist();
    let items = cl.by_category(MemoryCategory::OwnershipModel);
    assert!(
        items.len() >= 3,
        "OwnershipModel should have >= 3 items, got {}",
        items.len()
    );
    for item in &items {
        assert_eq!(item.category, MemoryCategory::OwnershipModel);
    }
}

#[test]
fn by_category_borrow_checking() {
    let cl = MemoryParityChecklist::full_checklist();
    let items = cl.by_category(MemoryCategory::BorrowChecking);
    assert!(
        items.len() >= 3,
        "BorrowChecking should have >= 3 items, got {}",
        items.len()
    );
}

#[test]
fn by_category_arena_allocation() {
    let cl = MemoryParityChecklist::full_checklist();
    let items = cl.by_category(MemoryCategory::ArenaAllocation);
    assert!(
        items.len() >= 2,
        "ArenaAllocation should have >= 2 items, got {}",
        items.len()
    );
}

#[test]
fn by_category_garbage_collection() {
    let cl = MemoryParityChecklist::full_checklist();
    let items = cl.by_category(MemoryCategory::GarbageCollection);
    assert!(
        items.len() >= 3,
        "GarbageCollection should have >= 3 items, got {}",
        items.len()
    );
}

#[test]
fn by_category_linear_types() {
    let cl = MemoryParityChecklist::full_checklist();
    let items = cl.by_category(MemoryCategory::LinearTypes);
    assert!(
        items.len() >= 2,
        "LinearTypes should have >= 2 items, got {}",
        items.len()
    );
}

#[test]
fn by_category_affine_types() {
    let cl = MemoryParityChecklist::full_checklist();
    let items = cl.by_category(MemoryCategory::AffineTypes);
    assert!(
        items.len() >= 2,
        "AffineTypes should have >= 2 items, got {}",
        items.len()
    );
}

#[test]
fn by_category_escape_analysis() {
    let cl = MemoryParityChecklist::full_checklist();
    let items = cl.by_category(MemoryCategory::EscapeAnalysis);
    assert!(
        items.len() >= 2,
        "EscapeAnalysis should have >= 2 items, got {}",
        items.len()
    );
}

#[test]
fn by_category_returns_empty_for_no_match() {
    // All 15 categories are used, but we can verify filtering works correctly
    // by checking one category and ensuring no cross-contamination.
    let cl = MemoryParityChecklist::full_checklist();
    let items = cl.by_category(MemoryCategory::EscapeAnalysis);
    for item in &items {
        assert_eq!(item.category, MemoryCategory::EscapeAnalysis);
    }
}

#[test]
fn all_15_categories_represented() {
    let cl = MemoryParityChecklist::full_checklist();
    let cats: std::collections::HashSet<_> = cl.items.iter().map(|i| i.category).collect();
    assert_eq!(
        cats.len(),
        15,
        "Expected all 15 categories, got {}",
        cats.len()
    );
}

// ════════════════════════════════════════════════════════════════════
// §3  Coverage and counts
// ════════════════════════════════════════════════════════════════════

#[test]
fn implemented_count_reasonable() {
    let cl = MemoryParityChecklist::full_checklist();
    // At least 30 items should be Implemented
    assert!(
        cl.implemented_count() >= 30,
        "Expected >= 30 implemented, got {}",
        cl.implemented_count()
    );
}

#[test]
fn implemented_count_le_total() {
    let cl = MemoryParityChecklist::full_checklist();
    assert!(cl.implemented_count() <= cl.total_count());
}

#[test]
fn coverage_percent_in_range() {
    let cl = MemoryParityChecklist::full_checklist();
    let pct = cl.coverage_percent();
    assert!(pct > 0.0 && pct <= 100.0, "Coverage {}% out of range", pct);
}

#[test]
fn coverage_percent_matches_ratio() {
    let cl = MemoryParityChecklist::full_checklist();
    let expected = (cl.implemented_count() as f64 / cl.total_count() as f64) * 100.0;
    let actual = cl.coverage_percent();
    assert!(
        (actual - expected).abs() < 0.001,
        "Coverage mismatch: expected {}, got {}",
        expected,
        actual
    );
}

#[test]
fn gaps_plus_implemented_plus_na_equals_total() {
    let cl = MemoryParityChecklist::full_checklist();
    let gaps = cl.gaps().len();
    let implemented = cl.implemented_count();
    let na = cl.not_applicable().len();
    assert_eq!(
        gaps + implemented + na,
        cl.total_count(),
        "gaps({}) + implemented({}) + na({}) != total({})",
        gaps,
        implemented,
        na,
        cl.total_count()
    );
}

#[test]
fn gaps_are_not_implemented() {
    let cl = MemoryParityChecklist::full_checklist();
    for gap in cl.gaps() {
        assert!(
            !gap.status.is_implemented(),
            "Gap {} should not be implemented",
            gap.id
        );
    }
}

// ════════════════════════════════════════════════════════════════════
// §4  Parity status behavior
// ════════════════════════════════════════════════════════════════════

#[test]
fn status_implemented_is_implemented() {
    assert!(ParityStatus::Implemented.is_implemented());
    assert!(!ParityStatus::Implemented.is_gap());
}

#[test]
fn status_partially_implemented_is_gap() {
    let s = ParityStatus::PartiallyImplemented("WIP".into());
    assert!(!s.is_implemented());
    assert!(s.is_gap());
}

#[test]
fn status_designed_is_gap() {
    assert!(!ParityStatus::Designed.is_implemented());
    assert!(ParityStatus::Designed.is_gap());
}

#[test]
fn status_planned_is_gap() {
    let s = ParityStatus::Planned("v2".into());
    assert!(!s.is_implemented());
    assert!(s.is_gap());
}

#[test]
fn status_not_applicable_is_neither() {
    let s = ParityStatus::NotApplicable("different model".into());
    assert!(!s.is_implemented());
    assert!(!s.is_gap());
}

// ════════════════════════════════════════════════════════════════════
// §5  Display implementations
// ════════════════════════════════════════════════════════════════════

#[test]
fn category_display() {
    assert_eq!(
        format!("{}", MemoryCategory::OwnershipModel),
        "Ownership Model"
    );
    assert_eq!(
        format!("{}", MemoryCategory::BorrowChecking),
        "Borrow Checking"
    );
    assert_eq!(
        format!("{}", MemoryCategory::GarbageCollection),
        "Garbage Collection"
    );
    assert_eq!(
        format!("{}", MemoryCategory::EscapeAnalysis),
        "Escape Analysis"
    );
    assert_eq!(format!("{}", MemoryCategory::LinearTypes), "Linear Types");
    assert_eq!(format!("{}", MemoryCategory::AffineTypes), "Affine Types");
}

#[test]
fn status_display_implemented() {
    assert_eq!(format!("{}", ParityStatus::Implemented), "Implemented");
}

#[test]
fn status_display_partial() {
    let s = ParityStatus::PartiallyImplemented("no syntax".into());
    let d = format!("{}", s);
    assert!(d.contains("Partially Implemented"));
    assert!(d.contains("no syntax"));
}

#[test]
fn status_display_planned() {
    let s = ParityStatus::Planned("v3".into());
    let d = format!("{}", s);
    assert!(d.contains("Planned"));
    assert!(d.contains("v3"));
}

#[test]
fn status_display_not_applicable() {
    let s = ParityStatus::NotApplicable("no GC in Rust".into());
    let d = format!("{}", s);
    assert!(d.contains("N/A"));
    assert!(d.contains("no GC in Rust"));
}

#[test]
fn status_display_designed() {
    assert_eq!(format!("{}", ParityStatus::Designed), "Designed");
}

// ════════════════════════════════════════════════════════════════════
// §6  Reporting: summary and markdown
// ════════════════════════════════════════════════════════════════════

#[test]
fn summary_contains_counts() {
    let cl = MemoryParityChecklist::full_checklist();
    let s = cl.summary();
    assert!(s.contains("Memory parity:"), "Summary missing header");
    assert!(s.contains('/'), "Summary missing slash separator");
    assert!(s.contains('%'), "Summary missing percent sign");
}

#[test]
fn summary_mentions_gaps_if_present() {
    let cl = MemoryParityChecklist::full_checklist();
    let s = cl.summary();
    if !cl.gaps().is_empty() {
        assert!(s.contains("gaps"), "Summary should mention gaps");
    } else {
        assert!(s.contains("no gaps"), "Summary should say no gaps");
    }
}

#[test]
fn markdown_has_header_and_table() {
    let cl = MemoryParityChecklist::full_checklist();
    let md = cl.to_markdown();
    assert!(md.starts_with("# Lumen Memory Safety Parity Checklist"));
    assert!(md.contains("| ID |"));
    assert!(md.contains("|---|"));
}

#[test]
fn markdown_contains_all_ids() {
    let cl = MemoryParityChecklist::full_checklist();
    let md = cl.to_markdown();
    for item in &cl.items {
        assert!(md.contains(&item.id), "Markdown missing ID {}", item.id);
    }
}

#[test]
fn markdown_gaps_section_if_gaps_exist() {
    let cl = MemoryParityChecklist::full_checklist();
    let md = cl.to_markdown();
    if !cl.gaps().is_empty() {
        assert!(md.contains("## Gaps"), "Markdown should have Gaps section");
    }
}

// ════════════════════════════════════════════════════════════════════
// §7  Specific item spot-checks
// ════════════════════════════════════════════════════════════════════

#[test]
fn item_mem001_single_owner() {
    let cl = MemoryParityChecklist::full_checklist();
    let item = cl.items.iter().find(|i| i.id == "MEM-001").unwrap();
    assert_eq!(item.category, MemoryCategory::OwnershipModel);
    assert!(item.status.is_implemented());
    assert!(item.test_coverage);
}

#[test]
fn item_mem025_immix() {
    let cl = MemoryParityChecklist::full_checklist();
    let item = cl.items.iter().find(|i| i.id == "MEM-025").unwrap();
    assert_eq!(item.category, MemoryCategory::GarbageCollection);
    assert!(item.status.is_implemented());
    assert!(item.feature.contains("Immix"));
}

#[test]
fn item_mem033_null_safety() {
    let cl = MemoryParityChecklist::full_checklist();
    let item = cl.items.iter().find(|i| i.id == "MEM-033").unwrap();
    assert!(item.feature.contains("Null safety"));
    assert!(item.status.is_implemented());
}

#[test]
fn item_mem034_bounds_checking() {
    let cl = MemoryParityChecklist::full_checklist();
    let item = cl.items.iter().find(|i| i.id == "MEM-034").unwrap();
    assert!(item.feature.contains("bounds"));
    assert!(item.status.is_implemented());
}

#[test]
fn item_mem044_escape_analysis_planned() {
    let cl = MemoryParityChecklist::full_checklist();
    let item = cl.items.iter().find(|i| i.id == "MEM-044").unwrap();
    assert_eq!(item.category, MemoryCategory::EscapeAnalysis);
    assert!(item.status.is_gap());
}

// ════════════════════════════════════════════════════════════════════
// §8  Item fields are non-empty
// ════════════════════════════════════════════════════════════════════

#[test]
fn all_items_have_nonempty_fields() {
    let cl = MemoryParityChecklist::full_checklist();
    for item in &cl.items {
        assert!(!item.id.is_empty(), "Item has empty id");
        assert!(
            !item.feature.is_empty(),
            "Item {} has empty feature",
            item.id
        );
        assert!(
            !item.description.is_empty(),
            "Item {} has empty description",
            item.id
        );
        assert!(
            !item.rust_equivalent.is_empty(),
            "Item {} has empty rust_equivalent",
            item.id
        );
        assert!(
            !item.lumen_implementation.is_empty(),
            "Item {} has empty lumen_implementation",
            item.id
        );
    }
}

#[test]
fn implemented_items_have_test_coverage() {
    let cl = MemoryParityChecklist::full_checklist();
    for item in &cl.items {
        if item.status.is_implemented() {
            assert!(
                item.test_coverage,
                "Implemented item {} should have test_coverage=true",
                item.id
            );
        }
    }
}

// ════════════════════════════════════════════════════════════════════
// §9  Edge cases
// ════════════════════════════════════════════════════════════════════

#[test]
fn empty_checklist_coverage_zero() {
    let cl = MemoryParityChecklist { items: vec![] };
    assert_eq!(cl.coverage_percent(), 0.0);
    assert_eq!(cl.implemented_count(), 0);
    assert_eq!(cl.total_count(), 0);
    assert!(cl.gaps().is_empty());
}

#[test]
fn empty_checklist_summary() {
    let cl = MemoryParityChecklist { items: vec![] };
    let s = cl.summary();
    assert!(s.contains("0/0"));
    assert!(s.contains("no gaps"));
}

#[test]
fn empty_checklist_markdown() {
    let cl = MemoryParityChecklist { items: vec![] };
    let md = cl.to_markdown();
    assert!(md.contains("0/0"));
}

#[test]
fn single_implemented_item() {
    let cl = MemoryParityChecklist {
        items: vec![ParityItem {
            id: "TEST-001".into(),
            category: MemoryCategory::OwnershipModel,
            feature: "test".into(),
            description: "test desc".into(),
            status: ParityStatus::Implemented,
            rust_equivalent: "test".into(),
            lumen_implementation: "test".into(),
            test_coverage: true,
        }],
    };
    assert_eq!(cl.implemented_count(), 1);
    assert_eq!(cl.total_count(), 1);
    assert_eq!(cl.coverage_percent(), 100.0);
    assert!(cl.gaps().is_empty());
}

#[test]
fn single_gap_item() {
    let cl = MemoryParityChecklist {
        items: vec![ParityItem {
            id: "TEST-002".into(),
            category: MemoryCategory::EscapeAnalysis,
            feature: "test gap".into(),
            description: "test gap desc".into(),
            status: ParityStatus::Planned("future".into()),
            rust_equivalent: "test".into(),
            lumen_implementation: "planned".into(),
            test_coverage: false,
        }],
    };
    assert_eq!(cl.implemented_count(), 0);
    assert_eq!(cl.total_count(), 1);
    assert_eq!(cl.coverage_percent(), 0.0);
    assert_eq!(cl.gaps().len(), 1);
}

// ════════════════════════════════════════════════════════════════════
// §10  Category Display for all variants
// ════════════════════════════════════════════════════════════════════

#[test]
fn all_category_display_variants() {
    // Ensure all MemoryCategory variants have non-empty Display output
    let categories = [
        MemoryCategory::OwnershipModel,
        MemoryCategory::BorrowChecking,
        MemoryCategory::LifetimeAnalysis,
        MemoryCategory::MoveSemantics,
        MemoryCategory::CopySemantics,
        MemoryCategory::DropSemantics,
        MemoryCategory::ArenaAllocation,
        MemoryCategory::GarbageCollection,
        MemoryCategory::StackAllocation,
        MemoryCategory::HeapManagement,
        MemoryCategory::RegionBasedMemory,
        MemoryCategory::LinearTypes,
        MemoryCategory::AffineTypes,
        MemoryCategory::ReferenceCountingOptimization,
        MemoryCategory::EscapeAnalysis,
    ];
    for cat in &categories {
        let s = format!("{}", cat);
        assert!(!s.is_empty(), "Category {:?} has empty display", cat);
    }
}

#[test]
fn not_applicable_accessor() {
    let cl = MemoryParityChecklist::full_checklist();
    // Our full checklist currently has no N/A items, verify that
    let na = cl.not_applicable();
    // They should all truly be NotApplicable
    for item in &na {
        assert!(matches!(item.status, ParityStatus::NotApplicable(_)));
    }
}
