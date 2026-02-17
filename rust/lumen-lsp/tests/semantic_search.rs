//! Integration tests for semantic search (wave24).

use lumen_lsp::semantic_search::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn loc(file: &str, line: u32) -> SymbolLocation {
    SymbolLocation {
        file: file.to_string(),
        line,
        column: 0,
        end_line: line,
        end_column: 0,
    }
}

fn sym(name: &str, kind: SymbolKind) -> IndexedSymbol {
    IndexedSymbol {
        name: name.to_string(),
        kind,
        location: loc("test.lm", 1),
        signature: None,
        doc: None,
        parent: None,
    }
}

fn sym_in(name: &str, kind: SymbolKind, file: &str, line: u32) -> IndexedSymbol {
    IndexedSymbol {
        name: name.to_string(),
        kind,
        location: loc(file, line),
        signature: None,
        doc: None,
        parent: None,
    }
}

fn build_sample_index() -> SymbolIndex {
    let mut idx = SymbolIndex::new();
    idx.add_symbol(sym("main", SymbolKind::Cell));
    idx.add_symbol(sym("parse_json", SymbolKind::Cell));
    idx.add_symbol(sym("BinarySearch", SymbolKind::Cell));
    idx.add_symbol(sym("Point", SymbolKind::Record));
    idx.add_symbol(sym("Color", SymbolKind::Enum));
    idx.add_symbol(sym("Red", SymbolKind::Variant));
    idx.add_symbol(sym("Green", SymbolKind::Variant));
    idx.add_symbol(sym("x", SymbolKind::Field));
    idx.add_symbol(sym("Logger", SymbolKind::Process));
    idx.add_symbol(sym("http", SymbolKind::Effect));
    idx.add_symbol(sym_in("helper", SymbolKind::Cell, "utils.lm", 10));
    idx.add_symbol(sym_in("transform", SymbolKind::Cell, "utils.lm", 20));
    idx
}

// ===========================================================================
// 1. Basic construction
// ===========================================================================

#[test]
fn semantic_search_new_index_is_empty() {
    let idx = SymbolIndex::new();
    assert_eq!(idx.symbol_count(), 0);
    assert_eq!(idx.file_count(), 0);
}

#[test]
fn semantic_search_default_is_empty() {
    let idx = SymbolIndex::default();
    assert_eq!(idx.symbol_count(), 0);
}

#[test]
fn semantic_search_add_increments_count() {
    let mut idx = SymbolIndex::new();
    idx.add_symbol(sym("foo", SymbolKind::Cell));
    assert_eq!(idx.symbol_count(), 1);
    idx.add_symbol(sym("bar", SymbolKind::Cell));
    assert_eq!(idx.symbol_count(), 2);
}

// ===========================================================================
// 2. Exact search
// ===========================================================================

#[test]
fn semantic_search_exact_finds_symbol() {
    let idx = build_sample_index();
    let results = idx.search_exact("Point");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Point");
}

#[test]
fn semantic_search_exact_missing_returns_empty() {
    let idx = build_sample_index();
    assert!(idx.search_exact("nonexistent").is_empty());
}

#[test]
fn semantic_search_exact_is_case_sensitive() {
    let idx = build_sample_index();
    assert!(idx.search_exact("point").is_empty());
    assert_eq!(idx.search_exact("Point").len(), 1);
}

// ===========================================================================
// 3. Ranked search — exact / prefix / substring
// ===========================================================================

#[test]
fn semantic_search_exact_match_scores_1() {
    let idx = build_sample_index();
    let results = idx.search("main", 10);
    assert!(!results.is_empty());
    assert_eq!(results[0].symbol.name, "main");
    assert!((results[0].score - 1.0).abs() < f64::EPSILON);
    assert_eq!(results[0].match_kind, MatchKind::Exact);
}

#[test]
fn semantic_search_prefix_match_scores_0_9() {
    let idx = build_sample_index();
    let results = idx.search("pars", 10);
    assert!(!results.is_empty());
    assert_eq!(results[0].symbol.name, "parse_json");
    assert!((results[0].score - 0.9).abs() < f64::EPSILON);
    assert_eq!(results[0].match_kind, MatchKind::Prefix);
}

#[test]
fn semantic_search_substring_match_found() {
    let idx = build_sample_index();
    let results = idx.search("json", 10);
    let found = results.iter().any(|r| r.symbol.name == "parse_json");
    assert!(found);
}

#[test]
fn semantic_search_ordering_exact_gt_prefix_gt_substring() {
    let mut idx = SymbolIndex::new();
    idx.add_symbol(sym("foo", SymbolKind::Cell));
    idx.add_symbol(sym("foobar", SymbolKind::Cell));
    idx.add_symbol(sym("xfoo", SymbolKind::Cell));

    let results = idx.search("foo", 10);
    assert!(results.len() >= 3);
    assert_eq!(results[0].symbol.name, "foo");
    assert_eq!(results[1].symbol.name, "foobar");
    assert_eq!(results[2].symbol.name, "xfoo");
}

// ===========================================================================
// 4. Case insensitivity
// ===========================================================================

#[test]
fn semantic_search_case_insensitive_by_default() {
    let idx = build_sample_index();
    let results = idx.search("MAIN", 10);
    assert!(!results.is_empty());
    assert_eq!(results[0].symbol.name, "main");
}

#[test]
fn semantic_search_case_sensitive_prefix() {
    let mut idx = SymbolIndex::new();
    idx.add_symbol(sym("MyType", SymbolKind::Record));
    idx.add_symbol(sym("mytype", SymbolKind::Record));
    let results = idx.search("CS:MyType", 10);
    assert!(!results.is_empty());
    assert_eq!(results[0].symbol.name, "MyType");
    assert!((results[0].score - 1.0).abs() < f64::EPSILON);
}

// ===========================================================================
// 5. Boundary matching (CamelCase / snake_case)
// ===========================================================================

#[test]
fn semantic_search_camel_case_boundary() {
    let idx = build_sample_index();
    let results = idx.search("BS", 10);
    let found = results.iter().any(|r| r.symbol.name == "BinarySearch");
    assert!(found, "BS should match BinarySearch");
}

#[test]
fn semantic_search_snake_case_boundary() {
    let idx = build_sample_index();
    let results = idx.search("pj", 10);
    let found = results.iter().any(|r| r.symbol.name == "parse_json");
    assert!(found, "pj should match parse_json");
}

// ===========================================================================
// 6. Fuzzy / subsequence
// ===========================================================================

#[test]
fn semantic_search_fuzzy_subsequence() {
    let idx = build_sample_index();
    let results = idx.fuzzy_search("bnsr", 10);
    let found = results.iter().any(|r| r.symbol.name == "BinarySearch");
    assert!(found, "bnsr should match BinarySearch");
}

#[test]
fn semantic_search_fuzzy_no_match() {
    let idx = build_sample_index();
    let results = idx.fuzzy_search("zzzzz", 10);
    assert!(results.is_empty());
}

// ===========================================================================
// 7. Trigram index
// ===========================================================================

#[test]
fn semantic_search_trigram_in_index_after_add() {
    let mut idx = SymbolIndex::new();
    idx.add_symbol(sym("transform", SymbolKind::Cell));
    // "tra" should exist.
    let results = idx.search("tra", 10);
    assert!(!results.is_empty());
}

#[test]
fn semantic_search_rebuild_trigram_preserves_results() {
    let mut idx = build_sample_index();
    idx.rebuild_trigram_index();
    let results = idx.search("parse_json", 10);
    assert!(!results.is_empty());
    assert_eq!(results[0].symbol.name, "parse_json");
}

// ===========================================================================
// 8. Kind filter via search_by_kind
// ===========================================================================

#[test]
fn semantic_search_by_kind_cells_only() {
    let idx = build_sample_index();
    let results = idx.search_by_kind("a", SymbolKind::Cell, 20);
    for r in &results {
        assert_eq!(r.symbol.kind, SymbolKind::Cell);
    }
}

#[test]
fn semantic_search_by_kind_no_match() {
    let idx = build_sample_index();
    let results = idx.search_by_kind("Point", SymbolKind::Cell, 10);
    // Point is a Record, not a Cell.
    assert!(results.is_empty());
}

// ===========================================================================
// 9. File filtering
// ===========================================================================

#[test]
fn semantic_search_symbols_in_file() {
    let idx = build_sample_index();
    let results = idx.symbols_in_file("utils.lm");
    assert_eq!(results.len(), 2);
}

#[test]
fn semantic_search_symbols_in_nonexistent_file() {
    let idx = build_sample_index();
    assert!(idx.symbols_in_file("nope.lm").is_empty());
}

// ===========================================================================
// 10. Kind listing
// ===========================================================================

#[test]
fn semantic_search_symbols_by_kind_variant() {
    let idx = build_sample_index();
    let variants = idx.symbols_by_kind(SymbolKind::Variant);
    assert_eq!(variants.len(), 2);
}

// ===========================================================================
// 11. Stats
// ===========================================================================

#[test]
fn semantic_search_file_count() {
    let idx = build_sample_index();
    assert_eq!(idx.file_count(), 2); // test.lm and utils.lm
}

#[test]
fn semantic_search_symbol_count() {
    let idx = build_sample_index();
    assert_eq!(idx.symbol_count(), 12);
}

// ===========================================================================
// 12. Clear
// ===========================================================================

#[test]
fn semantic_search_clear_empties_everything() {
    let mut idx = build_sample_index();
    idx.clear();
    assert_eq!(idx.symbol_count(), 0);
    assert_eq!(idx.file_count(), 0);
    assert!(idx.search("main", 10).is_empty());
}

// ===========================================================================
// 13. Max results
// ===========================================================================

#[test]
fn semantic_search_max_results_respected() {
    let idx = build_sample_index();
    let results = idx.fuzzy_search("a", 2);
    assert!(results.len() <= 2);
}

// ===========================================================================
// 14. Empty query
// ===========================================================================

#[test]
fn semantic_search_empty_query_returns_nothing() {
    let idx = build_sample_index();
    assert!(idx.search("", 10).is_empty());
    assert!(idx.fuzzy_search("", 10).is_empty());
}

// ===========================================================================
// 15. SearchQuery parsing
// ===========================================================================

#[test]
fn semantic_search_query_parse_plain() {
    let q = SearchQuery::parse("foo");
    assert_eq!(q.text, "foo");
    assert_eq!(q.kind_filter, None);
    assert_eq!(q.file_filter, None);
    assert!(!q.case_sensitive);
}

#[test]
fn semantic_search_query_parse_cell() {
    let q = SearchQuery::parse("cell:main");
    assert_eq!(q.text, "main");
    assert_eq!(q.kind_filter, Some(SymbolKind::Cell));
}

#[test]
fn semantic_search_query_parse_record() {
    let q = SearchQuery::parse("record:Point");
    assert_eq!(q.text, "Point");
    assert_eq!(q.kind_filter, Some(SymbolKind::Record));
}

#[test]
fn semantic_search_query_parse_file_filter() {
    let q = SearchQuery::parse("file:utils.lm helper");
    assert_eq!(q.file_filter, Some("utils.lm".to_string()));
    assert_eq!(q.text, "helper");
}

#[test]
fn semantic_search_query_parse_case_sensitive() {
    let q = SearchQuery::parse("CS:Name");
    assert!(q.case_sensitive);
    assert_eq!(q.text, "Name");
}

#[test]
fn semantic_search_query_parse_empty() {
    let q = SearchQuery::parse("");
    assert_eq!(q.text, "");
    assert_eq!(q.kind_filter, None);
}

#[test]
fn semantic_search_query_all_kind_prefixes() {
    let cases = vec![
        ("cell:", SymbolKind::Cell),
        ("record:", SymbolKind::Record),
        ("enum:", SymbolKind::Enum),
        ("variant:", SymbolKind::Variant),
        ("field:", SymbolKind::Field),
        ("process:", SymbolKind::Process),
        ("effect:", SymbolKind::Effect),
        ("type:", SymbolKind::TypeAlias),
        ("module:", SymbolKind::Module),
        ("constant:", SymbolKind::Constant),
    ];
    for (prefix, expected) in cases {
        let q = SearchQuery::parse(&format!("{prefix}test"));
        assert_eq!(q.kind_filter, Some(expected), "prefix {prefix}");
        assert_eq!(q.text, "test");
    }
}

// ===========================================================================
// 16. Integrated query: kind filter via search()
// ===========================================================================

#[test]
fn semantic_search_integrated_kind_filter() {
    let idx = build_sample_index();
    let results = idx.search("cell:main", 10);
    assert!(!results.is_empty());
    assert_eq!(results[0].symbol.name, "main");
    assert_eq!(results[0].symbol.kind, SymbolKind::Cell);
}

// ===========================================================================
// 17. Integrated query: file filter via search()
// ===========================================================================

#[test]
fn semantic_search_integrated_file_filter() {
    let idx = build_sample_index();
    let results = idx.search("file:utils.lm helper", 10);
    assert!(!results.is_empty());
    for r in &results {
        assert!(r.symbol.location.file.contains("utils.lm"));
    }
}

// ===========================================================================
// 18. Metadata round-trip
// ===========================================================================

#[test]
fn semantic_search_symbol_metadata() {
    let mut idx = SymbolIndex::new();
    idx.add_symbol(IndexedSymbol {
        name: "add".to_string(),
        kind: SymbolKind::Cell,
        location: loc("math.lm", 5),
        signature: Some("(x: Int, y: Int) -> Int".to_string()),
        doc: Some("Add two numbers".to_string()),
        parent: Some("MathMod".to_string()),
    });
    let results = idx.search_exact("add");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].signature.as_deref(),
        Some("(x: Int, y: Int) -> Int")
    );
    assert_eq!(results[0].doc.as_deref(), Some("Add two numbers"));
    assert_eq!(results[0].parent.as_deref(), Some("MathMod"));
}

// ===========================================================================
// 19. Duplicate names across files
// ===========================================================================

#[test]
fn semantic_search_duplicate_names_different_files() {
    let mut idx = SymbolIndex::new();
    idx.add_symbol(sym_in("init", SymbolKind::Cell, "a.lm", 1));
    idx.add_symbol(sym_in("init", SymbolKind::Cell, "b.lm", 1));
    let results = idx.search_exact("init");
    assert_eq!(results.len(), 2);
}

// ===========================================================================
// 20. SymbolKind Display
// ===========================================================================

#[test]
fn semantic_search_symbol_kind_display() {
    assert_eq!(format!("{}", SymbolKind::Cell), "cell");
    assert_eq!(format!("{}", SymbolKind::Record), "record");
    assert_eq!(format!("{}", SymbolKind::Enum), "enum");
    assert_eq!(format!("{}", SymbolKind::Variant), "variant");
    assert_eq!(format!("{}", SymbolKind::Field), "field");
    assert_eq!(format!("{}", SymbolKind::Process), "process");
    assert_eq!(format!("{}", SymbolKind::Effect), "effect");
    assert_eq!(format!("{}", SymbolKind::TypeAlias), "type");
    assert_eq!(format!("{}", SymbolKind::Module), "module");
    assert_eq!(format!("{}", SymbolKind::Constant), "constant");
}

// ===========================================================================
// 21. Results sorted by score
// ===========================================================================

#[test]
fn semantic_search_results_sorted_descending() {
    let idx = build_sample_index();
    let results = idx.search("main", 10);
    for w in results.windows(2) {
        assert!(w[0].score >= w[1].score);
    }
}
