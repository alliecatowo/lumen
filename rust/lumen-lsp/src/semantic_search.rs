//! Semantic search index for workspace symbol lookup.
//!
//! Provides a searchable symbol database with trigram indexing, fuzzy matching,
//! and structured query parsing for the Lumen LSP.

use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A searchable index of symbols extracted from the workspace.
pub struct SymbolIndex {
    symbols: Vec<IndexedSymbol>,
    trigram_index: HashMap<String, Vec<usize>>,
}

/// A symbol stored in the index.
#[derive(Debug, Clone)]
pub struct IndexedSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub location: SymbolLocation,
    pub signature: Option<String>,
    pub doc: Option<String>,
    pub parent: Option<String>,
}

/// The kind of a symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Cell,
    Record,
    Enum,
    Variant,
    Field,
    Process,
    Effect,
    TypeAlias,
    Module,
    Constant,
}

/// Source location of a symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

/// Result of a search query.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub symbol: IndexedSymbol,
    pub score: f64,
    pub match_kind: MatchKind,
}

/// How the query matched the symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    Exact,
    Prefix,
    Substring,
    Fuzzy,
    Trigram,
}

/// A parsed search query with optional filters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub text: String,
    pub kind_filter: Option<SymbolKind>,
    pub file_filter: Option<String>,
    pub case_sensitive: bool,
}

// ---------------------------------------------------------------------------
// SymbolIndex implementation
// ---------------------------------------------------------------------------

impl SymbolIndex {
    /// Create an empty index.
    pub fn new() -> Self {
        Self {
            symbols: Vec::new(),
            trigram_index: HashMap::new(),
        }
    }

    /// Add a symbol and update the trigram index incrementally.
    pub fn add_symbol(&mut self, symbol: IndexedSymbol) {
        let idx = self.symbols.len();
        let trigrams = extract_trigrams(&symbol.name.to_lowercase());
        self.symbols.push(symbol);
        for tri in trigrams {
            self.trigram_index.entry(tri).or_default().push(idx);
        }
    }

    /// Rebuild the entire trigram index from scratch.
    pub fn rebuild_trigram_index(&mut self) {
        self.trigram_index.clear();
        for (idx, sym) in self.symbols.iter().enumerate() {
            for tri in extract_trigrams(&sym.name.to_lowercase()) {
                self.trigram_index.entry(tri).or_default().push(idx);
            }
        }
    }

    /// Remove all symbols and clear the index.
    pub fn clear(&mut self) {
        self.symbols.clear();
        self.trigram_index.clear();
    }

    // -- Search ---------------------------------------------------------------

    /// Search using the best strategy available — tries exact, prefix,
    /// substring, boundary, trigram, and fuzzy in order, then merges and ranks.
    pub fn search(&self, query: &str, max_results: usize) -> Vec<SearchResult> {
        if query.is_empty() {
            return Vec::new();
        }
        let parsed = SearchQuery::parse(query);
        self.execute_search(&parsed, max_results)
    }

    /// Explicitly fuzzy-only search.
    pub fn fuzzy_search(&self, query: &str, max_results: usize) -> Vec<SearchResult> {
        if query.is_empty() {
            return Vec::new();
        }
        let query_lower = query.to_lowercase();
        let mut results: Vec<SearchResult> = self
            .symbols
            .iter()
            .filter_map(|sym| {
                let score = fuzzy_score(&query_lower, &sym.name.to_lowercase());
                if score > 0.0 {
                    Some(SearchResult {
                        symbol: sym.clone(),
                        score,
                        match_kind: classify_match(&query_lower, &sym.name.to_lowercase()),
                    })
                } else {
                    None
                }
            })
            .collect();
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(max_results);
        results
    }

    /// Search filtered to a specific `SymbolKind`.
    pub fn search_by_kind(
        &self,
        query: &str,
        kind: SymbolKind,
        max_results: usize,
    ) -> Vec<SearchResult> {
        if query.is_empty() {
            return Vec::new();
        }
        let query_lower = query.to_lowercase();
        let mut results: Vec<SearchResult> = self
            .symbols
            .iter()
            .filter(|s| s.kind == kind)
            .filter_map(|sym| {
                let score = fuzzy_score(&query_lower, &sym.name.to_lowercase());
                if score > 0.0 {
                    Some(SearchResult {
                        symbol: sym.clone(),
                        score,
                        match_kind: classify_match(&query_lower, &sym.name.to_lowercase()),
                    })
                } else {
                    None
                }
            })
            .collect();
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(max_results);
        results
    }

    /// Exact name lookup (case-sensitive).
    pub fn search_exact(&self, name: &str) -> Vec<&IndexedSymbol> {
        self.symbols.iter().filter(|s| s.name == name).collect()
    }

    // -- Filtering ------------------------------------------------------------

    /// Return all symbols defined in a given file path.
    pub fn symbols_in_file(&self, file: &str) -> Vec<&IndexedSymbol> {
        self.symbols
            .iter()
            .filter(|s| s.location.file == file)
            .collect()
    }

    /// Return all symbols of a given kind.
    pub fn symbols_by_kind(&self, kind: SymbolKind) -> Vec<&IndexedSymbol> {
        self.symbols.iter().filter(|s| s.kind == kind).collect()
    }

    // -- Stats ----------------------------------------------------------------

    /// Total number of indexed symbols.
    pub fn symbol_count(&self) -> usize {
        self.symbols.len()
    }

    /// Number of distinct files containing symbols.
    pub fn file_count(&self) -> usize {
        let files: HashSet<&str> = self
            .symbols
            .iter()
            .map(|s| s.location.file.as_str())
            .collect();
        files.len()
    }

    // -- Internal search engine -----------------------------------------------

    fn execute_search(&self, query: &SearchQuery, max_results: usize) -> Vec<SearchResult> {
        let query_lower = query.text.to_lowercase();

        // Collect candidates — use trigram index to narrow when query is long enough.
        let candidate_indices: Vec<usize> = if query_lower.len() >= 3 {
            self.trigram_candidates(&query_lower)
        } else {
            (0..self.symbols.len()).collect()
        };

        let mut results: Vec<SearchResult> = candidate_indices
            .into_iter()
            .filter_map(|idx| {
                let sym = &self.symbols[idx];

                // Apply kind filter.
                if let Some(kind) = query.kind_filter {
                    if sym.kind != kind {
                        return None;
                    }
                }
                // Apply file filter.
                if let Some(ref file) = query.file_filter {
                    if !sym.location.file.contains(file.as_str()) {
                        return None;
                    }
                }

                let name_lower = sym.name.to_lowercase();

                let score = if query.case_sensitive {
                    fuzzy_score_case_sensitive(&query.text, &sym.name)
                } else {
                    fuzzy_score(&query_lower, &name_lower)
                };

                if score > 0.0 {
                    Some(SearchResult {
                        symbol: sym.clone(),
                        score,
                        match_kind: if query.case_sensitive {
                            classify_match(&query.text, &sym.name)
                        } else {
                            classify_match(&query_lower, &name_lower)
                        },
                    })
                } else {
                    None
                }
            })
            .collect();

        // Also scan all symbols for short queries to catch boundary matches
        // that the trigram index might miss.
        if query_lower.len() < 3 {
            // Already scanning everything — no extra work needed.
        } else {
            // Check remaining symbols not in trigram candidates for boundary match.
            let trigram_set: HashSet<usize> = {
                let tris = extract_trigrams(&query_lower);
                let mut set = HashSet::new();
                for tri in &tris {
                    if let Some(indices) = self.trigram_index.get(tri) {
                        set.extend(indices.iter().copied());
                    }
                }
                set
            };
            for (idx, sym) in self.symbols.iter().enumerate() {
                if trigram_set.contains(&idx) {
                    continue;
                }
                if let Some(kind) = query.kind_filter {
                    if sym.kind != kind {
                        continue;
                    }
                }
                if let Some(ref file) = query.file_filter {
                    if !sym.location.file.contains(file.as_str()) {
                        continue;
                    }
                }
                let name_lower = sym.name.to_lowercase();
                let boundary_sc = boundary_match_score(&query_lower, &sym.name);
                if boundary_sc > 0.0 {
                    let mk = classify_match(&query_lower, &name_lower);
                    results.push(SearchResult {
                        symbol: sym.clone(),
                        score: boundary_sc,
                        match_kind: mk,
                    });
                }
            }
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Deduplicate by symbol name+file+line (keep highest score).
        let mut seen = HashSet::new();
        results.retain(|r| {
            let key = (
                r.symbol.name.clone(),
                r.symbol.location.file.clone(),
                r.symbol.location.line,
            );
            seen.insert(key)
        });

        results.truncate(max_results);
        results
    }

    /// Use the trigram index to find candidate symbol indices.
    fn trigram_candidates(&self, query_lower: &str) -> Vec<usize> {
        let query_trigrams = extract_trigrams(query_lower);
        if query_trigrams.is_empty() {
            return (0..self.symbols.len()).collect();
        }

        // Count how many query trigrams each symbol matches.
        let mut counts: HashMap<usize, usize> = HashMap::new();
        for tri in &query_trigrams {
            if let Some(indices) = self.trigram_index.get(tri) {
                for &idx in indices {
                    *counts.entry(idx).or_insert(0) += 1;
                }
            }
        }

        // Accept symbols matching at least one trigram.
        let mut candidates: Vec<usize> = counts.into_keys().collect();
        candidates.sort_unstable();
        candidates
    }
}

impl Default for SymbolIndex {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// SearchQuery parser
// ---------------------------------------------------------------------------

impl SearchQuery {
    /// Parse a search query string.
    ///
    /// Supported syntax:
    /// - `"cell:parse"` — filter to cells named `parse`
    /// - `"record:Point"` — filter to records named `Point`
    /// - `"file:main.lm foo"` — filter to file containing `main.lm`, search `foo`
    /// - `"CS:Name"` — case-sensitive search for `Name`
    /// - Raw text with no prefix searches all symbols.
    pub fn parse(input: &str) -> Self {
        let mut kind_filter = None;
        let mut file_filter = None;
        let mut case_sensitive = false;
        let mut remaining = input.to_string();

        // Process prefixes.
        loop {
            let trimmed = remaining.trim_start();
            if let Some(rest) = try_strip_kind_prefix(trimmed) {
                kind_filter = rest.0;
                remaining = rest.1.to_string();
                continue;
            }
            if let Some(rest) = try_strip_prefix(trimmed, "file:") {
                let (token, tail) = split_first_token(rest);
                file_filter = Some(token.to_string());
                remaining = tail.to_string();
                continue;
            }
            if let Some(rest) = try_strip_prefix(trimmed, "CS:") {
                case_sensitive = true;
                remaining = rest.to_string();
                continue;
            }
            break;
        }

        Self {
            text: remaining.trim().to_string(),
            kind_filter,
            file_filter,
            case_sensitive,
        }
    }
}

fn try_strip_prefix<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    input.strip_prefix(prefix)
}

fn try_strip_kind_prefix(input: &str) -> Option<(Option<SymbolKind>, &str)> {
    let kind_prefixes: &[(&str, SymbolKind)] = &[
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

    for &(prefix, kind) in kind_prefixes {
        if let Some(rest) = try_strip_prefix(input, prefix) {
            return Some((Some(kind), rest));
        }
    }
    None
}

fn split_first_token(input: &str) -> (&str, &str) {
    let trimmed = input.trim_start();
    match trimmed.find(char::is_whitespace) {
        Some(pos) => (&trimmed[..pos], &trimmed[pos..]),
        None => (trimmed, ""),
    }
}

// ---------------------------------------------------------------------------
// Trigram extraction
// ---------------------------------------------------------------------------

fn extract_trigrams(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 3 {
        return Vec::new();
    }
    let mut tris = Vec::with_capacity(chars.len() - 2);
    for window in chars.windows(3) {
        tris.push(window.iter().collect::<String>());
    }
    tris
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

/// Compute a relevance score for `query` against `name`.
/// Both should already be lowercased for case-insensitive matching.
fn fuzzy_score(query: &str, name: &str) -> f64 {
    if query.is_empty() {
        return 0.0;
    }

    // Exact match.
    if query == name {
        return 1.0;
    }

    // Prefix match.
    if name.starts_with(query) {
        return 0.9;
    }

    // Substring match.
    if name.contains(query) {
        return 0.7;
    }

    // CamelCase / snake_case boundary match.
    let boundary_sc = boundary_match_score(query, name);
    if boundary_sc > 0.0 {
        return boundary_sc;
    }

    // Trigram Jaccard similarity.
    let tri_score = trigram_similarity(query, name);
    if tri_score > 0.05 {
        // Scale trigram score into 0.0..0.5 range.
        return tri_score * 0.5;
    }

    // Subsequence match (classic fuzzy).
    if is_subsequence(query, name) {
        // Score based on how compact the match is.
        let ratio = query.len() as f64 / name.len() as f64;
        let base = 0.2;
        return base + ratio * 0.2;
    }

    0.0
}

/// Case-sensitive fuzzy score.
fn fuzzy_score_case_sensitive(query: &str, name: &str) -> f64 {
    if query.is_empty() {
        return 0.0;
    }
    if query == name {
        return 1.0;
    }
    if name.starts_with(query) {
        return 0.9;
    }
    if name.contains(query) {
        return 0.7;
    }

    let boundary_sc = boundary_match_score_sensitive(query, name);
    if boundary_sc > 0.0 {
        return boundary_sc;
    }

    let query_lower = query.to_lowercase();
    let name_lower = name.to_lowercase();
    let tri_score = trigram_similarity(&query_lower, &name_lower);
    if tri_score > 0.05 {
        return tri_score * 0.5;
    }

    if is_subsequence_sensitive(query, name) {
        let ratio = query.len() as f64 / name.len() as f64;
        return 0.2 + ratio * 0.2;
    }

    0.0
}

/// Classify a match into its kind (for display purposes).
fn classify_match(query: &str, name: &str) -> MatchKind {
    if query == name {
        MatchKind::Exact
    } else if name.starts_with(query) {
        MatchKind::Prefix
    } else if name.contains(query) {
        MatchKind::Substring
    } else {
        let tri_score = trigram_similarity(query, name);
        if tri_score > 0.1 {
            MatchKind::Trigram
        } else {
            MatchKind::Fuzzy
        }
    }
}

// ---------------------------------------------------------------------------
// Boundary matching (CamelCase / snake_case)
// ---------------------------------------------------------------------------

/// Score for CamelCase or snake_case boundary matching.
/// `query` is lowercase, `original_name` preserves original casing.
fn boundary_match_score(query: &str, original_name: &str) -> f64 {
    let boundaries = extract_boundaries(original_name);
    if boundaries.is_empty() {
        return 0.0;
    }

    // Build a lowercase string from boundary chars.
    let boundary_str: String = boundaries
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();

    let query_lower = query.to_lowercase();

    if boundary_str == query_lower {
        return 0.85;
    }
    if boundary_str.starts_with(&query_lower) {
        return 0.8;
    }

    // Also try matching against boundary-delimited segments.
    let segments = extract_segments(original_name);
    let segments_lower: Vec<String> = segments.iter().map(|s| s.to_lowercase()).collect();
    let joined = segments_lower.join("");

    if joined.starts_with(&query_lower) {
        return 0.75;
    }
    if joined.contains(&query_lower) {
        return 0.6;
    }

    // Check if query chars match first chars of consecutive segments.
    if segments_lower.len() >= 2 && query_lower.len() >= 2 {
        let query_chars: Vec<char> = query_lower.chars().collect();
        let mut qi = 0;
        for seg in &segments_lower {
            if qi >= query_chars.len() {
                break;
            }
            if seg.starts_with(query_chars[qi]) {
                qi += 1;
            }
        }
        if qi == query_chars.len() {
            return 0.65;
        }
    }

    0.0
}

/// Case-sensitive boundary matching.
fn boundary_match_score_sensitive(query: &str, original_name: &str) -> f64 {
    let boundaries = extract_boundaries(original_name);
    if boundaries.is_empty() {
        return 0.0;
    }

    let boundary_str: String = boundaries.iter().collect();

    if boundary_str == query {
        return 0.85;
    }
    if boundary_str.starts_with(query) {
        return 0.8;
    }

    0.0
}

/// Extract boundary characters from a name:
/// - Uppercase letters in CamelCase
/// - First char after `_` in snake_case
/// - Always includes the first character
fn extract_boundaries(name: &str) -> Vec<char> {
    let chars: Vec<char> = name.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }

    let mut boundaries = vec![chars[0]];
    for i in 1..chars.len() {
        let c = chars[i];
        let prev = chars[i - 1];

        if (c.is_uppercase() && prev.is_lowercase()) || (prev == '_' && c != '_') {
            boundaries.push(c);
        }
    }

    boundaries
}

/// Split a name into CamelCase / snake_case segments.
fn extract_segments(name: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();

    for (i, c) in name.chars().enumerate() {
        if c == '_' {
            if !current.is_empty() {
                segments.push(current.clone());
                current.clear();
            }
            continue;
        }
        if i > 0 && c.is_uppercase() {
            let prev = name.chars().nth(i - 1).unwrap_or('_');
            if prev.is_lowercase() && !current.is_empty() {
                segments.push(current.clone());
                current.clear();
            }
        }
        current.push(c);
    }

    if !current.is_empty() {
        segments.push(current);
    }

    segments
}

// ---------------------------------------------------------------------------
// Trigram Jaccard similarity
// ---------------------------------------------------------------------------

fn trigram_similarity(a: &str, b: &str) -> f64 {
    let tris_a: HashSet<String> = extract_trigrams(a).into_iter().collect();
    let tris_b: HashSet<String> = extract_trigrams(b).into_iter().collect();

    if tris_a.is_empty() || tris_b.is_empty() {
        return 0.0;
    }

    let intersection = tris_a.intersection(&tris_b).count();
    let union = tris_a.union(&tris_b).count();

    if union == 0 {
        return 0.0;
    }

    intersection as f64 / union as f64
}

// ---------------------------------------------------------------------------
// Subsequence matching
// ---------------------------------------------------------------------------

fn is_subsequence(query: &str, name: &str) -> bool {
    let mut name_chars = name.chars();
    for qc in query.chars() {
        let found = name_chars.any(|nc| nc == qc);
        if !found {
            return false;
        }
    }
    true
}

fn is_subsequence_sensitive(query: &str, name: &str) -> bool {
    let mut name_chars = name.chars();
    for qc in query.chars() {
        let found = name_chars.any(|nc| nc == qc);
        if !found {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Display helpers
// ---------------------------------------------------------------------------

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            SymbolKind::Cell => "cell",
            SymbolKind::Record => "record",
            SymbolKind::Enum => "enum",
            SymbolKind::Variant => "variant",
            SymbolKind::Field => "field",
            SymbolKind::Process => "process",
            SymbolKind::Effect => "effect",
            SymbolKind::TypeAlias => "type",
            SymbolKind::Module => "module",
            SymbolKind::Constant => "constant",
        };
        write!(f, "{}", s)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_exact_match() {
        let idx = build_sample_index();
        let results = idx.search("main", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].symbol.name, "main");
        assert_eq!(results[0].match_kind, MatchKind::Exact);
        assert!((results[0].score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_prefix_match() {
        let idx = build_sample_index();
        let results = idx.search("pars", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].symbol.name, "parse_json");
        assert_eq!(results[0].match_kind, MatchKind::Prefix);
        assert!((results[0].score - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn test_substring_match() {
        let idx = build_sample_index();
        let results = idx.search("json", 10);
        assert!(!results.is_empty());
        let found = results.iter().any(|r| r.symbol.name == "parse_json");
        assert!(found, "parse_json should appear in results");
    }

    #[test]
    fn test_empty_query_returns_nothing() {
        let idx = build_sample_index();
        assert!(idx.search("", 10).is_empty());
        assert!(idx.fuzzy_search("", 10).is_empty());
    }

    #[test]
    fn test_search_by_kind() {
        let idx = build_sample_index();
        let results = idx.search_by_kind("a", SymbolKind::Cell, 10);
        for r in &results {
            assert_eq!(r.symbol.kind, SymbolKind::Cell);
        }
    }

    #[test]
    fn test_search_exact() {
        let idx = build_sample_index();
        let results = idx.search_exact("Point");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Point");
    }

    #[test]
    fn test_search_exact_missing() {
        let idx = build_sample_index();
        let results = idx.search_exact("nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn test_symbols_in_file() {
        let idx = build_sample_index();
        let results = idx.symbols_in_file("utils.lm");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_symbols_in_file_empty() {
        let idx = build_sample_index();
        let results = idx.symbols_in_file("nonexistent.lm");
        assert!(results.is_empty());
    }

    #[test]
    fn test_symbols_by_kind() {
        let idx = build_sample_index();
        let variants = idx.symbols_by_kind(SymbolKind::Variant);
        assert_eq!(variants.len(), 2);
    }

    #[test]
    fn test_symbol_count() {
        let idx = build_sample_index();
        assert_eq!(idx.symbol_count(), 12);
    }

    #[test]
    fn test_file_count() {
        let idx = build_sample_index();
        assert_eq!(idx.file_count(), 2);
    }

    #[test]
    fn test_clear() {
        let mut idx = build_sample_index();
        idx.clear();
        assert_eq!(idx.symbol_count(), 0);
        assert_eq!(idx.file_count(), 0);
        assert!(idx.search("main", 10).is_empty());
    }

    #[test]
    fn test_rebuild_trigram_index() {
        let mut idx = build_sample_index();
        idx.rebuild_trigram_index();
        // Should still find things.
        let results = idx.search("parse_json", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].symbol.name, "parse_json");
    }

    #[test]
    fn test_max_results_limits_output() {
        let idx = build_sample_index();
        let results = idx.fuzzy_search("a", 2);
        assert!(results.len() <= 2);
    }

    #[test]
    fn test_case_insensitive_by_default() {
        let idx = build_sample_index();
        let results = idx.search("MAIN", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].symbol.name, "main");
    }

    #[test]
    fn test_trigram_extraction() {
        let tris = extract_trigrams("hello");
        assert_eq!(tris, vec!["hel", "ell", "llo"]);
    }

    #[test]
    fn test_trigram_extraction_short() {
        assert!(extract_trigrams("ab").is_empty());
        assert!(extract_trigrams("").is_empty());
    }

    #[test]
    fn test_trigram_similarity_identical() {
        let score = trigram_similarity("hello", "hello");
        assert!((score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_trigram_similarity_disjoint() {
        let score = trigram_similarity("abc", "xyz");
        assert!(score.abs() < f64::EPSILON);
    }

    #[test]
    fn test_boundary_camel_case() {
        let idx = build_sample_index();
        let results = idx.search("BS", 10);
        let found = results.iter().any(|r| r.symbol.name == "BinarySearch");
        assert!(found, "BS should match BinarySearch via boundary matching");
    }

    #[test]
    fn test_boundary_snake_case() {
        let idx = build_sample_index();
        let results = idx.search("pj", 10);
        let found = results.iter().any(|r| r.symbol.name == "parse_json");
        assert!(found, "pj should match parse_json via boundary matching");
    }

    #[test]
    fn test_fuzzy_subsequence() {
        let idx = build_sample_index();
        let results = idx.fuzzy_search("bnsr", 10);
        let found = results.iter().any(|r| r.symbol.name == "BinarySearch");
        assert!(found, "bnsr should match BinarySearch via subsequence");
    }

    #[test]
    fn test_results_sorted_by_score() {
        let idx = build_sample_index();
        let results = idx.search("main", 10);
        for window in results.windows(2) {
            assert!(window[0].score >= window[1].score);
        }
    }

    #[test]
    fn test_search_query_parse_plain() {
        let q = SearchQuery::parse("foo");
        assert_eq!(q.text, "foo");
        assert_eq!(q.kind_filter, None);
        assert_eq!(q.file_filter, None);
        assert!(!q.case_sensitive);
    }

    #[test]
    fn test_search_query_parse_kind_prefix() {
        let q = SearchQuery::parse("cell:parse");
        assert_eq!(q.text, "parse");
        assert_eq!(q.kind_filter, Some(SymbolKind::Cell));
    }

    #[test]
    fn test_search_query_parse_record_prefix() {
        let q = SearchQuery::parse("record:Point");
        assert_eq!(q.text, "Point");
        assert_eq!(q.kind_filter, Some(SymbolKind::Record));
    }

    #[test]
    fn test_search_query_parse_file_filter() {
        let q = SearchQuery::parse("file:utils.lm helper");
        assert_eq!(q.text, "helper");
        assert_eq!(q.file_filter, Some("utils.lm".to_string()));
    }

    #[test]
    fn test_search_query_parse_case_sensitive() {
        let q = SearchQuery::parse("CS:MyType");
        assert_eq!(q.text, "MyType");
        assert!(q.case_sensitive);
    }

    #[test]
    fn test_search_with_kind_filter() {
        let idx = build_sample_index();
        let results = idx.search("cell:main", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].symbol.name, "main");
        assert_eq!(results[0].symbol.kind, SymbolKind::Cell);
    }

    #[test]
    fn test_search_with_file_filter() {
        let idx = build_sample_index();
        let results = idx.search("file:utils.lm helper", 10);
        assert!(!results.is_empty());
        for r in &results {
            assert!(r.symbol.location.file.contains("utils.lm"));
        }
    }

    #[test]
    fn test_search_case_sensitive_rejects_wrong_case() {
        let mut idx = SymbolIndex::new();
        idx.add_symbol(sym("MyType", SymbolKind::Record));
        idx.add_symbol(sym("mytype", SymbolKind::Record));

        let results = idx.search("CS:MyType", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].symbol.name, "MyType");
        assert!((results[0].score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_empty_index() {
        let idx = SymbolIndex::new();
        assert_eq!(idx.symbol_count(), 0);
        assert_eq!(idx.file_count(), 0);
        assert!(idx.search("foo", 10).is_empty());
    }

    #[test]
    fn test_add_symbol_updates_trigrams() {
        let mut idx = SymbolIndex::new();
        idx.add_symbol(sym("transform", SymbolKind::Cell));
        // Trigram "tra" should be in the index.
        assert!(idx.trigram_index.contains_key("tra"));
    }

    #[test]
    fn test_extract_boundaries_camel() {
        let boundaries = extract_boundaries("BinarySearch");
        assert_eq!(boundaries, vec!['B', 'S']);
    }

    #[test]
    fn test_extract_boundaries_snake() {
        let boundaries = extract_boundaries("parse_json");
        assert_eq!(boundaries, vec!['p', 'j']);
    }

    #[test]
    fn test_extract_boundaries_single_word() {
        let boundaries = extract_boundaries("main");
        assert_eq!(boundaries, vec!['m']);
    }

    #[test]
    fn test_extract_segments_camel() {
        let segments = extract_segments("BinarySearch");
        assert_eq!(segments, vec!["Binary", "Search"]);
    }

    #[test]
    fn test_extract_segments_snake() {
        let segments = extract_segments("parse_json");
        assert_eq!(segments, vec!["parse", "json"]);
    }

    #[test]
    fn test_is_subsequence() {
        assert!(is_subsequence("abc", "aXbXc"));
        assert!(!is_subsequence("abc", "aXc"));
    }

    #[test]
    fn test_symbol_kind_display() {
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

    #[test]
    fn test_indexed_symbol_with_metadata() {
        let mut idx = SymbolIndex::new();
        idx.add_symbol(IndexedSymbol {
            name: "add".to_string(),
            kind: SymbolKind::Cell,
            location: loc("math.lm", 5),
            signature: Some("(x: Int, y: Int) -> Int".to_string()),
            doc: Some("Add two integers".to_string()),
            parent: Some("MathModule".to_string()),
        });

        let results = idx.search_exact("add");
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].signature.as_deref(),
            Some("(x: Int, y: Int) -> Int")
        );
        assert_eq!(results[0].doc.as_deref(), Some("Add two integers"));
        assert_eq!(results[0].parent.as_deref(), Some("MathModule"));
    }

    #[test]
    fn test_default_impl() {
        let idx = SymbolIndex::default();
        assert_eq!(idx.symbol_count(), 0);
    }

    #[test]
    fn test_search_query_all_kind_prefixes() {
        let kinds = vec![
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
        for (prefix, expected_kind) in kinds {
            let q = SearchQuery::parse(&format!("{}foo", prefix));
            assert_eq!(
                q.kind_filter,
                Some(expected_kind),
                "prefix {prefix} should produce {expected_kind}"
            );
            assert_eq!(q.text, "foo");
        }
    }

    #[test]
    fn test_multiple_symbols_same_name_different_files() {
        let mut idx = SymbolIndex::new();
        idx.add_symbol(sym_in("init", SymbolKind::Cell, "a.lm", 1));
        idx.add_symbol(sym_in("init", SymbolKind::Cell, "b.lm", 1));
        let results = idx.search_exact("init");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_fuzzy_search_no_match() {
        let idx = build_sample_index();
        let results = idx.fuzzy_search("zzzzz", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_score_ordering_exact_prefix_substring() {
        let mut idx = SymbolIndex::new();
        idx.add_symbol(sym("foo", SymbolKind::Cell));
        idx.add_symbol(sym("foobar", SymbolKind::Cell));
        idx.add_symbol(sym("xfoo", SymbolKind::Cell));

        let results = idx.search("foo", 10);
        assert!(results.len() >= 3);
        // Exact match first.
        assert_eq!(results[0].symbol.name, "foo");
        assert_eq!(results[0].match_kind, MatchKind::Exact);
        // Prefix second.
        assert_eq!(results[1].symbol.name, "foobar");
        assert_eq!(results[1].match_kind, MatchKind::Prefix);
        // Substring third.
        assert_eq!(results[2].symbol.name, "xfoo");
        assert_eq!(results[2].match_kind, MatchKind::Substring);
    }

    #[test]
    fn test_search_query_parse_empty() {
        let q = SearchQuery::parse("");
        assert_eq!(q.text, "");
        assert_eq!(q.kind_filter, None);
    }
}
