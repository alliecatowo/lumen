//! Wave 20 Agent C — T188: Source mapping for string interpolation spans.
//!
//! Tests for:
//! - `InterpolationSegmentSpan` struct and its fields
//! - `map_interpolation_spans()`: parsing interpolated strings into segments
//! - `interpolation_expr_col_range()`: locating specific expression segments

use lumen_compiler::diagnostics::{
    interpolation_expr_col_range, map_interpolation_spans, InterpolationSegmentSpan,
};

// ============================================================================
// map_interpolation_spans — basic cases
// ============================================================================

#[test]
fn interp_span_simple_literal_only() {
    // "hello world" — no interpolation
    let line = r#"let x = "hello world""#;
    let spans = map_interpolation_spans(line, 9); // col 9 is the opening quote
    assert_eq!(spans.len(), 1, "spans: {:?}", spans);
    assert_eq!(spans[0].is_expr, false);
    assert_eq!(spans[0].length, 11); // "hello world" = 11 chars
}

#[test]
fn interp_span_single_expr() {
    // "hello {name}" — one literal, one expr
    let line = r#"let x = "hello {name}""#;
    let spans = map_interpolation_spans(line, 9);
    assert_eq!(spans.len(), 2, "spans: {:?}", spans);

    // "hello " — literal
    assert_eq!(spans[0].is_expr, false);
    assert_eq!(spans[0].length, 6); // "hello "

    // "{name}" — expr
    assert_eq!(spans[1].is_expr, true);
    assert_eq!(spans[1].length, 6); // "{name}"
}

#[test]
fn interp_span_expr_at_start() {
    // "{x} is the value" — expr at start
    let line = r#"let s = "{x} is the value""#;
    let spans = map_interpolation_spans(line, 9);
    assert_eq!(spans.len(), 2, "spans: {:?}", spans);

    // "{x}" — expr
    assert_eq!(spans[0].is_expr, true);
    assert_eq!(spans[0].length, 3); // "{x}"

    // " is the value" — literal
    assert_eq!(spans[1].is_expr, false);
    assert_eq!(spans[1].length, 13);
}

#[test]
fn interp_span_expr_at_end() {
    // "result: {val}" — expr at end
    let line = r#"let s = "result: {val}""#;
    let spans = map_interpolation_spans(line, 9);
    assert_eq!(spans.len(), 2, "spans: {:?}", spans);

    assert_eq!(spans[0].is_expr, false);
    assert_eq!(spans[0].length, 8); // "result: "

    assert_eq!(spans[1].is_expr, true);
    assert_eq!(spans[1].length, 5); // "{val}"
}

#[test]
fn interp_span_multiple_exprs() {
    // "{a} and {b} then {c}"
    let line = r#"let s = "{a} and {b} then {c}""#;
    let spans = map_interpolation_spans(line, 9);
    assert_eq!(spans.len(), 5, "spans: {:?}", spans);

    assert_eq!(spans[0].is_expr, true); // {a}
    assert_eq!(spans[0].length, 3);
    assert_eq!(spans[1].is_expr, false); // " and "
    assert_eq!(spans[1].length, 5);
    assert_eq!(spans[2].is_expr, true); // {b}
    assert_eq!(spans[2].length, 3);
    assert_eq!(spans[3].is_expr, false); // " then "
    assert_eq!(spans[3].length, 6);
    assert_eq!(spans[4].is_expr, true); // {c}
    assert_eq!(spans[4].length, 3);
}

#[test]
fn interp_span_adjacent_exprs() {
    // "{a}{b}" — two adjacent expressions, no literal between
    let line = r#"let s = "{a}{b}""#;
    let spans = map_interpolation_spans(line, 9);
    assert_eq!(spans.len(), 2, "spans: {:?}", spans);

    assert_eq!(spans[0].is_expr, true); // {a}
    assert_eq!(spans[0].length, 3);
    assert_eq!(spans[1].is_expr, true); // {b}
    assert_eq!(spans[1].length, 3);
}

#[test]
fn interp_span_empty_string() {
    // "" — empty string
    let line = r#"let s = """#;
    let spans = map_interpolation_spans(line, 9);
    assert_eq!(spans.len(), 0, "spans: {:?}", spans);
}

// ============================================================================
// map_interpolation_spans — escape sequences
// ============================================================================

#[test]
fn interp_span_escaped_brace() {
    // "hello \{not_expr}" — escaped brace should be literal
    let line = r#"let s = "hello \{not_expr}""#;
    let spans = map_interpolation_spans(line, 9);
    // The backslash escapes the char after it, so \{ is part of the literal
    // But the } is not escaped, so this depends on exact handling.
    // The function treats \ as escape-next-char, so \{ means { is escaped,
    // then not_expr} would be literal text until closing quote.
    // There should be no expr segments.
    for span in &spans {
        assert_eq!(span.is_expr, false, "no expr expected, spans: {:?}", spans);
    }
}

#[test]
fn interp_span_escape_sequence_in_literal() {
    // "line1\nline2 {x}" — \n is an escape in the literal part
    let line = r#"let s = "line1\nline2 {x}""#;
    let spans = map_interpolation_spans(line, 9);
    // Should have a literal and an expr
    let expr_count = spans.iter().filter(|s| s.is_expr).count();
    assert_eq!(expr_count, 1, "spans: {:?}", spans);
}

// ============================================================================
// map_interpolation_spans — nested braces
// ============================================================================

#[test]
fn interp_span_nested_braces() {
    // "val: {f(a, {b: 1})}" — nested braces inside expression
    let line = r#"let s = "val: {f(a, {b: 1})}""#;
    let spans = map_interpolation_spans(line, 9);

    let expr_spans: Vec<_> = spans.iter().filter(|s| s.is_expr).collect();
    assert_eq!(expr_spans.len(), 1, "spans: {:?}", spans);
    // The entire {f(a, {b: 1})} should be one expression
    assert!(
        expr_spans[0].length > 10,
        "nested expr should be wide: {:?}",
        expr_spans[0]
    );
}

// ============================================================================
// map_interpolation_spans — offset correctness
// ============================================================================

#[test]
fn interp_span_offsets_are_from_quote() {
    // Offset should be relative to the opening quote
    let line = r#"let s = "ab{cd}ef""#;
    let spans = map_interpolation_spans(line, 9);
    assert_eq!(spans.len(), 3, "spans: {:?}", spans);

    // "ab" — literal, offset 1 (after the opening quote)
    assert_eq!(spans[0].offset, 1);
    assert_eq!(spans[0].length, 2);
    assert_eq!(spans[0].is_expr, false);

    // "{cd}" — expr, offset 3
    assert_eq!(spans[1].offset, 3);
    assert_eq!(spans[1].length, 4);
    assert_eq!(spans[1].is_expr, true);

    // "ef" — literal, offset 7
    assert_eq!(spans[2].offset, 7);
    assert_eq!(spans[2].length, 2);
    assert_eq!(spans[2].is_expr, false);
}

// ============================================================================
// map_interpolation_spans — edge / error cases
// ============================================================================

#[test]
fn interp_span_col_out_of_bounds() {
    let line = r#"short"#;
    let spans = map_interpolation_spans(line, 100);
    assert_eq!(spans.len(), 0);
}

#[test]
fn interp_span_col_not_quote() {
    let line = r#"let x = hello"#;
    // Column 9 is 'h', not '"'
    let spans = map_interpolation_spans(line, 9);
    assert_eq!(spans.len(), 0);
}

#[test]
fn interp_span_col_zero_handled() {
    // string_start_col is 1-based, 0 triggers saturating_sub edge case
    let line = r#""hello {x}""#;
    let spans = map_interpolation_spans(line, 0);
    // 0 saturating_sub 1 = 0, but col 0 check: chars[0] should be '"'
    // Actually 0.saturating_sub(1) = 0, and chars[0] = '"' → should parse
    assert!(!spans.is_empty(), "col 0 (edge): spans: {:?}", spans);
}

#[test]
fn interp_span_col_one_is_first_char() {
    // Col 1 = first character
    let line = r#""hello {x}""#;
    let spans = map_interpolation_spans(line, 1);
    assert!(!spans.is_empty(), "col 1: spans: {:?}", spans);
    let expr_count = spans.iter().filter(|s| s.is_expr).count();
    assert_eq!(expr_count, 1);
}

// ============================================================================
// interpolation_expr_col_range — basic usage
// ============================================================================

#[test]
fn expr_col_range_single_expr() {
    // "hello {name}" — col 9 is the quote
    let line = r#"let x = "hello {name}""#;
    let range = interpolation_expr_col_range(line, 9, 0);
    assert!(range.is_some(), "should find expr 0");
    let (start, end) = range.unwrap();
    // {name} starts at offset 7 from quote, col = 9 + 7 = 16
    // length = 6, end = 16 + 6 = 22
    assert_eq!(start, 16, "start col");
    assert_eq!(end, 22, "end col");
}

#[test]
fn expr_col_range_multiple_exprs() {
    // "{a} and {b}" — col 9 is the quote
    let line = r#"let x = "{a} and {b}""#;

    // First expr: {a}
    let range0 = interpolation_expr_col_range(line, 9, 0);
    assert!(range0.is_some());
    let (_s0, e0) = range0.unwrap();

    // Second expr: {b}
    let range1 = interpolation_expr_col_range(line, 9, 1);
    assert!(range1.is_some());
    let (s1, _e1) = range1.unwrap();

    // Second expr should start after first
    assert!(s1 > e0, "second expr starts after first: {} > {}", s1, e0);
}

#[test]
fn expr_col_range_out_of_bounds_index() {
    let line = r#"let x = "hello {name}""#;
    // Only one expr, so index 1 should be None
    let range = interpolation_expr_col_range(line, 9, 1);
    assert!(range.is_none());
}

#[test]
fn expr_col_range_no_exprs() {
    let line = r#"let x = "hello world""#;
    let range = interpolation_expr_col_range(line, 9, 0);
    assert!(range.is_none());
}

#[test]
fn expr_col_range_invalid_start() {
    let line = r#"let x = "hello {name}""#;
    // col 100 is out of bounds
    let range = interpolation_expr_col_range(line, 100, 0);
    assert!(range.is_none());
}

// ============================================================================
// InterpolationSegmentSpan — struct equality
// ============================================================================

#[test]
fn segment_span_equality() {
    let a = InterpolationSegmentSpan {
        offset: 1,
        length: 5,
        is_expr: true,
    };
    let b = InterpolationSegmentSpan {
        offset: 1,
        length: 5,
        is_expr: true,
    };
    let c = InterpolationSegmentSpan {
        offset: 1,
        length: 5,
        is_expr: false,
    };
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn segment_span_clone() {
    let a = InterpolationSegmentSpan {
        offset: 3,
        length: 7,
        is_expr: true,
    };
    let b = a.clone();
    assert_eq!(a, b);
}

// ============================================================================
// map_interpolation_spans — complex expression content
// ============================================================================

#[test]
fn interp_span_expr_with_function_call() {
    // "{to_string(x + 1)}" — expression with function call
    let line = r#"let s = "{to_string(x + 1)}""#;
    let spans = map_interpolation_spans(line, 9);
    let expr_spans: Vec<_> = spans.iter().filter(|s| s.is_expr).collect();
    assert_eq!(expr_spans.len(), 1, "spans: {:?}", spans);
}

#[test]
fn interp_span_expr_with_method_chain() {
    // "result: {items |> filter() |> count()}" — pipe expression
    let line = r#"let s = "result: {items |> filter() |> count()}""#;
    let spans = map_interpolation_spans(line, 9);
    let expr_spans: Vec<_> = spans.iter().filter(|s| s.is_expr).collect();
    assert_eq!(expr_spans.len(), 1, "spans: {:?}", spans);
}

// ============================================================================
// Verify segment offsets reconstruct original string content
// ============================================================================

#[test]
fn interp_span_offsets_reconstruct_content() {
    let inner = "ab{cd}ef";
    let line = format!(r#"let s = "{}""#, inner);
    let spans = map_interpolation_spans(&line, 9);

    // Each segment's offset+length should tile the content inside the quotes
    let mut expected_offset = 1; // after opening quote
    for span in &spans {
        assert_eq!(
            span.offset, expected_offset,
            "segment {:?} should start at {}, spans: {:?}",
            span, expected_offset, spans
        );
        expected_offset = span.offset + span.length;
    }
    // Final offset should be right before the closing quote
    // inner length is 8 chars, so expected_offset should be 1 + 8 = 9
    assert_eq!(
        expected_offset,
        1 + inner.len(),
        "spans should tile the string content"
    );
}
