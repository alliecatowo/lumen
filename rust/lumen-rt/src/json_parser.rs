//! Optimized JSON parsing with single-pass recursive descent.
//!
//! Performance strategy: parse JSON directly into Lumen `Value` in one pass,
//! avoiding the double-conversion overhead of simd-json → OwnedValue → Value.
//!
//! Fast paths for top-level scalars (integers, floats, bools, null, simple strings)
//! avoid entering the recursive parser entirely. For structured JSON (objects, arrays),
//! a hand-rolled recursive descent parser builds Lumen Values directly.
//!
//! Falls back to simd-json only when our parser cannot handle the input (e.g.,
//! unusual numeric formats that our fast integer parser rejects).

use crate::values::{StringRef, Value};
use std::collections::BTreeMap;

/// Main entry point: fast-path JSON parsing with multiple optimization tiers.
pub fn parse_json_optimized(input: &str) -> Result<Value, String> {
    let bytes = input.as_bytes();
    let len = bytes.len();

    // Fast path: empty or whitespace
    if len == 0 {
        return Ok(Value::Null);
    }

    // Find the first non-whitespace byte to dispatch on
    let start = match bytes.iter().position(|b| !b.is_ascii_whitespace()) {
        Some(pos) => pos,
        None => return Ok(Value::Null), // all whitespace
    };
    let first = bytes[start];

    // Fast path: bare literals (exact match for top-level scalars)
    match first {
        b'n' if bytes[start..].starts_with(b"null") => {
            // Verify no trailing non-whitespace
            if bytes[start + 4..].iter().all(|b| b.is_ascii_whitespace()) {
                return Ok(Value::Null);
            }
        }
        b't' if bytes[start..].starts_with(b"true") => {
            if bytes[start + 4..].iter().all(|b| b.is_ascii_whitespace()) {
                return Ok(Value::Bool(true));
            }
        }
        b'f' if bytes[start..].starts_with(b"false") => {
            if bytes[start + 5..].iter().all(|b| b.is_ascii_whitespace()) {
                return Ok(Value::Bool(false));
            }
        }
        _ => {}
    }

    // Fast path: top-level integer
    if matches!(first, b'-' | b'0'..=b'9') {
        if let Some(val) = try_parse_integer_fast(bytes) {
            return Ok(val);
        }
        // Could be a float, or an integer that overflowed i64
        if let Some(val) = try_parse_float_fast(bytes) {
            return Ok(val);
        }
    }

    // Fast path: top-level simple string (no escapes)
    if first == b'"' {
        let inner = &bytes[start + 1..];
        if let Some(end_quote) = inner.iter().position(|&b| b == b'"' || b == b'\\') {
            if inner[end_quote] == b'"' {
                // Check no trailing non-whitespace after closing quote
                let after = start + 1 + end_quote + 1;
                if bytes[after..].iter().all(|b| b.is_ascii_whitespace()) {
                    if let Ok(s) = std::str::from_utf8(&inner[..end_quote]) {
                        return Ok(Value::String(StringRef::Owned(s.to_string())));
                    }
                }
            }
        }
    }

    // Recursive descent parser for all structured JSON
    let mut pos = start;
    match parse_value(bytes, &mut pos) {
        Some(val) => {
            // Verify we consumed everything (minus trailing whitespace)
            while pos < len && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            if pos == len {
                Ok(val)
            } else {
                // Trailing garbage — fall back to simd-json for error reporting
                parse_with_simd(input)
            }
        }
        None => parse_with_simd(input),
    }
}

// ---------------------------------------------------------------------------
// Top-level scalar fast paths
// ---------------------------------------------------------------------------

/// Hand-rolled integer parser for top-level integers.
/// Avoids str::parse overhead. Handles optional sign, up to 19 digits (i64 range).
#[inline]
fn try_parse_integer_fast(bytes: &[u8]) -> Option<Value> {
    let mut i = 0;
    let len = bytes.len();

    // Skip leading whitespace
    while i < len && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= len {
        return None;
    }

    let negative = bytes[i] == b'-';
    if negative {
        i += 1;
        if i >= len {
            return None;
        }
    }

    if !bytes[i].is_ascii_digit() {
        return None;
    }

    let digit_start = i;
    let mut val: u64 = 0;
    while i < len && bytes[i].is_ascii_digit() {
        let d = (bytes[i] - b'0') as u64;
        val = val.checked_mul(10)?.checked_add(d)?;
        i += 1;
    }

    // Skip trailing whitespace
    while i < len && bytes[i].is_ascii_whitespace() {
        i += 1;
    }

    // Must have consumed everything — no decimal point, exponent, or trailing chars
    if i != len {
        return None;
    }

    // Reject leading zeros (except bare "0")
    if bytes[digit_start] == b'0' && (i - digit_start) > 1 && !negative {
        return None;
    }

    if negative {
        if val > (i64::MAX as u64) + 1 {
            return None;
        }
        if val == (i64::MAX as u64) + 1 {
            return Some(Value::Int(i64::MIN));
        }
        Some(Value::Int(-(val as i64)))
    } else {
        if val > i64::MAX as u64 {
            return None;
        }
        Some(Value::Int(val as i64))
    }
}

/// Fast float parser for top-level floats (or integer overflow to f64).
#[inline]
fn try_parse_float_fast(bytes: &[u8]) -> Option<Value> {
    let s = std::str::from_utf8(bytes).ok()?;
    let f = s.trim().parse::<f64>().ok()?;
    Some(Value::Float(f))
}

// ---------------------------------------------------------------------------
// Recursive descent JSON parser — builds Value directly in one pass
// ---------------------------------------------------------------------------

/// Skip whitespace bytes starting at `pos`.
#[inline(always)]
fn skip_ws(bytes: &[u8], pos: &mut usize) {
    let len = bytes.len();
    let mut p = *pos;
    while p < len {
        // JSON only has 4 whitespace characters: space, tab, newline, carriage return
        match unsafe { *bytes.get_unchecked(p) } {
            b' ' | b'\t' | b'\n' | b'\r' => p += 1,
            _ => break,
        }
    }
    *pos = p;
}

/// Parse any JSON value at the current position.
/// Returns None if the input at `pos` is not valid JSON.
fn parse_value(bytes: &[u8], pos: &mut usize) -> Option<Value> {
    skip_ws(bytes, pos);
    if *pos >= bytes.len() {
        return None;
    }

    match bytes[*pos] {
        b'"' => parse_string(bytes, pos),
        b'{' => parse_object(bytes, pos),
        b'[' => parse_array(bytes, pos),
        b't' => parse_true(bytes, pos),
        b'f' => parse_false(bytes, pos),
        b'n' => parse_null(bytes, pos),
        b'-' | b'0'..=b'9' => parse_number(bytes, pos),
        _ => None,
    }
}

/// Parse `true`.
#[inline]
fn parse_true(bytes: &[u8], pos: &mut usize) -> Option<Value> {
    if bytes[*pos..].starts_with(b"true") {
        *pos += 4;
        Some(Value::Bool(true))
    } else {
        None
    }
}

/// Parse `false`.
#[inline]
fn parse_false(bytes: &[u8], pos: &mut usize) -> Option<Value> {
    if bytes[*pos..].starts_with(b"false") {
        *pos += 5;
        Some(Value::Bool(false))
    } else {
        None
    }
}

/// Parse `null`.
#[inline]
fn parse_null(bytes: &[u8], pos: &mut usize) -> Option<Value> {
    if bytes[*pos..].starts_with(b"null") {
        *pos += 4;
        Some(Value::Null)
    } else {
        None
    }
}

/// Parse a JSON number (integer or float) and advance `pos`.
#[inline]
fn parse_number(bytes: &[u8], pos: &mut usize) -> Option<Value> {
    let start = *pos;
    let len = bytes.len();

    let negative = bytes[*pos] == b'-';
    if negative {
        *pos += 1;
        if *pos >= len || !bytes[*pos].is_ascii_digit() {
            *pos = start;
            return None;
        }
    }

    // Integer part
    let digit_start = *pos;
    while *pos < len && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }

    // Check for fractional or exponent part
    let mut is_float = false;
    if *pos < len && bytes[*pos] == b'.' {
        is_float = true;
        *pos += 1;
        while *pos < len && bytes[*pos].is_ascii_digit() {
            *pos += 1;
        }
    }
    if *pos < len && (bytes[*pos] == b'e' || bytes[*pos] == b'E') {
        is_float = true;
        *pos += 1;
        if *pos < len && (bytes[*pos] == b'+' || bytes[*pos] == b'-') {
            *pos += 1;
        }
        while *pos < len && bytes[*pos].is_ascii_digit() {
            *pos += 1;
        }
    }

    if is_float {
        let s = std::str::from_utf8(&bytes[start..*pos]).ok()?;
        let f = s.parse::<f64>().ok()?;
        Some(Value::Float(f))
    } else {
        // Hand-rolled integer parse
        let mut val: u64 = 0;
        for &b in &bytes[digit_start..*pos] {
            val = match val.checked_mul(10) {
                Some(v) => match v.checked_add((b - b'0') as u64) {
                    Some(v2) => v2,
                    None => {
                        // Overflow — parse as float instead
                        let s = std::str::from_utf8(&bytes[start..*pos]).ok()?;
                        let f = s.parse::<f64>().ok()?;
                        return Some(Value::Float(f));
                    }
                },
                None => {
                    let s = std::str::from_utf8(&bytes[start..*pos]).ok()?;
                    let f = s.parse::<f64>().ok()?;
                    return Some(Value::Float(f));
                }
            };
        }

        if negative {
            if val > (i64::MAX as u64) + 1 {
                // Overflow — parse as float
                let s = std::str::from_utf8(&bytes[start..*pos]).ok()?;
                let f = s.parse::<f64>().ok()?;
                return Some(Value::Float(f));
            }
            if val == (i64::MAX as u64) + 1 {
                Some(Value::Int(i64::MIN))
            } else {
                Some(Value::Int(-(val as i64)))
            }
        } else {
            if val > i64::MAX as u64 {
                let s = std::str::from_utf8(&bytes[start..*pos]).ok()?;
                let f = s.parse::<f64>().ok()?;
                return Some(Value::Float(f));
            }
            Some(Value::Int(val as i64))
        }
    }
}

/// Parse a JSON string (with escape support) and advance `pos`.
fn parse_string(bytes: &[u8], pos: &mut usize) -> Option<Value> {
    debug_assert_eq!(bytes[*pos], b'"');
    *pos += 1; // skip opening "

    let start = *pos;
    let len = bytes.len();

    // Fast scan: look for end quote or backslash
    let mut end = start;
    while end < len {
        let b = unsafe { *bytes.get_unchecked(end) };
        if b == b'"' {
            // No escapes — fast path
            let s = unsafe { std::str::from_utf8_unchecked(&bytes[start..end]) };
            *pos = end + 1; // skip closing "
            return Some(Value::String(StringRef::Owned(s.to_string())));
        }
        if b == b'\\' {
            // Has escapes — process them
            let raw = parse_string_raw_escaped(bytes, pos, start, end)?;
            return Some(Value::String(StringRef::Owned(raw)));
        }
        end += 1;
    }

    None // unterminated string
}

/// Parse a JSON object and advance `pos`.
fn parse_object(bytes: &[u8], pos: &mut usize) -> Option<Value> {
    debug_assert_eq!(bytes[*pos], b'{');
    *pos += 1; // skip {

    let len = bytes.len();

    skip_ws(bytes, pos);
    if *pos < len && bytes[*pos] == b'}' {
        *pos += 1;
        return Some(Value::new_map(BTreeMap::new()));
    }

    // Collect key-value pairs into a Vec, then build BTreeMap at the end.
    // This avoids repeated BTreeMap rebalancing during insertion.
    let mut pairs: Vec<(String, Value)> = Vec::with_capacity(8);

    loop {
        skip_ws(bytes, pos);
        if *pos >= len || bytes[*pos] != b'"' {
            return None;
        }

        // Parse key — extract string content directly (avoid Value wrapping)
        let key = parse_string_raw(bytes, pos)?;

        skip_ws(bytes, pos);
        if *pos >= len || bytes[*pos] != b':' {
            return None;
        }
        *pos += 1; // skip :

        let value = parse_value(bytes, pos)?;
        pairs.push((key, value));

        skip_ws(bytes, pos);
        if *pos >= len {
            return None;
        }

        match bytes[*pos] {
            b',' => {
                *pos += 1;
            }
            b'}' => {
                *pos += 1;
                // Build BTreeMap from collected pairs
                let map: BTreeMap<String, Value> = pairs.into_iter().collect();
                return Some(Value::new_map(map));
            }
            _ => return None,
        }
    }
}

/// Parse a JSON string and return the raw String (not wrapped in Value).
/// Used by object key parsing to avoid wrapping/unwrapping.
#[inline]
fn parse_string_raw(bytes: &[u8], pos: &mut usize) -> Option<String> {
    debug_assert_eq!(bytes[*pos], b'"');
    *pos += 1;

    let start = *pos;
    let len = bytes.len();

    // Fast scan for end quote or backslash
    let mut end = start;
    while end < len {
        let b = unsafe { *bytes.get_unchecked(end) };
        if b == b'"' {
            // No escapes — fast path (most common for JSON keys)
            // Safety: JSON keys are valid UTF-8 by spec
            let s = unsafe { std::str::from_utf8_unchecked(&bytes[start..end]) };
            *pos = end + 1;
            return Some(s.to_string());
        }
        if b == b'\\' {
            // Has escapes — use slow path
            return parse_string_raw_escaped(bytes, pos, start, end);
        }
        end += 1;
    }

    None // unterminated string
}

/// Slow path for parse_string_raw when escapes are present.
/// `start` is the position after the opening `"`, `esc_pos` is where the first `\` was found.
fn parse_string_raw_escaped(
    bytes: &[u8],
    pos: &mut usize,
    start: usize,
    esc_pos: usize,
) -> Option<String> {
    let len = bytes.len();
    let mut result = String::with_capacity(32);
    // Copy the already-scanned non-escape prefix (bytes before the first backslash)
    result.push_str(unsafe { std::str::from_utf8_unchecked(&bytes[start..esc_pos]) });
    let mut i = esc_pos;
    while i < len {
        match bytes[i] {
            b'"' => {
                *pos = i + 1;
                return Some(result);
            }
            b'\\' => {
                i += 1;
                if i >= len {
                    return None;
                }
                match bytes[i] {
                    b'"' => result.push('"'),
                    b'\\' => result.push('\\'),
                    b'/' => result.push('/'),
                    b'b' => result.push('\u{0008}'),
                    b'f' => result.push('\u{000C}'),
                    b'n' => result.push('\n'),
                    b'r' => result.push('\r'),
                    b't' => result.push('\t'),
                    b'u' => {
                        if i + 4 >= len {
                            return None;
                        }
                        let hex = std::str::from_utf8(&bytes[i + 1..i + 5]).ok()?;
                        let code = u16::from_str_radix(hex, 16).ok()?;
                        i += 4;
                        if (0xD800..=0xDBFF).contains(&code) {
                            if i + 6 < len && bytes[i + 1] == b'\\' && bytes[i + 2] == b'u' {
                                let hex2 = std::str::from_utf8(&bytes[i + 3..i + 7]).ok()?;
                                let low = u16::from_str_radix(hex2, 16).ok()?;
                                if (0xDC00..=0xDFFF).contains(&low) {
                                    let cp = 0x10000
                                        + ((code as u32 - 0xD800) << 10)
                                        + (low as u32 - 0xDC00);
                                    result.push(char::from_u32(cp)?);
                                    i += 6;
                                } else {
                                    return None;
                                }
                            } else {
                                return None;
                            }
                        } else {
                            result.push(char::from_u32(code as u32)?);
                        }
                    }
                    _ => return None,
                }
                i += 1;
            }
            other => {
                result.push(other as char);
                i += 1;
            }
        }
    }
    None
}

/// Parse a JSON array and advance `pos`.
fn parse_array(bytes: &[u8], pos: &mut usize) -> Option<Value> {
    debug_assert_eq!(bytes[*pos], b'[');
    *pos += 1; // skip [
    let len = bytes.len();

    skip_ws(bytes, pos);
    if *pos < len && bytes[*pos] == b']' {
        *pos += 1;
        return Some(Value::new_list(Vec::new()));
    }

    // Estimate capacity from remaining bytes (rough heuristic: ~4 bytes per element)
    let remaining = len.saturating_sub(*pos);
    let est_cap = (remaining / 4).min(64); // cap at 64 to avoid over-allocation
    let mut values = Vec::with_capacity(est_cap);

    loop {
        let val = parse_value(bytes, pos)?;
        values.push(val);

        skip_ws(bytes, pos);
        if *pos >= len {
            return None;
        }

        match bytes[*pos] {
            b',' => {
                *pos += 1;
            }
            b']' => {
                *pos += 1;
                return Some(Value::new_list(values));
            }
            _ => return None,
        }
    }
}

// ---------------------------------------------------------------------------
// simd-json fallback (for inputs our parser rejects)
// ---------------------------------------------------------------------------

/// Parse using simd-json with OwnedValue for larger/unusual inputs.
fn parse_with_simd(input: &str) -> Result<Value, String> {
    let mut bytes = input.as_bytes().to_vec();

    match simd_json::to_owned_value(&mut bytes) {
        Ok(val) => Ok(owned_json_to_value(val)),
        Err(e) => Err(format!("JSON parse error: {}", e)),
    }
}

/// Convert simd_json::OwnedValue to Lumen Value.
fn owned_json_to_value(val: simd_json::OwnedValue) -> Value {
    use simd_json::OwnedValue;

    match val {
        OwnedValue::Static(s) => match s {
            simd_json::StaticNode::Null => Value::Null,
            simd_json::StaticNode::Bool(b) => Value::Bool(b),
            simd_json::StaticNode::F64(f) => Value::Float(f),
            simd_json::StaticNode::I64(i) => Value::Int(i),
            simd_json::StaticNode::U64(u) => {
                if u <= i64::MAX as u64 {
                    Value::Int(u as i64)
                } else {
                    Value::Float(u as f64)
                }
            }
        },
        OwnedValue::String(s) => Value::String(StringRef::Owned(s)),
        OwnedValue::Array(arr) => {
            let mut values = Vec::with_capacity(arr.len());
            for v in arr {
                values.push(owned_json_to_value(v));
            }
            Value::new_list(values)
        }
        OwnedValue::Object(obj) => {
            let mut map = BTreeMap::new();
            for (k, v) in obj.into_iter() {
                map.insert(k.into(), owned_json_to_value(v));
            }
            Value::new_map(map)
        }
    }
}

/// Fallback to serde_json for compatibility.
pub fn parse_json_serde(input: &str) -> Result<Value, String> {
    match serde_json::from_str::<serde_json::Value>(input) {
        Ok(v) => Ok(serde_json_to_value(&v)),
        Err(e) => Err(format!("JSON parse error: {}", e)),
    }
}

fn serde_json_to_value(val: &serde_json::Value) -> Value {
    match val {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Null
            }
        }
        serde_json::Value::String(s) => Value::String(StringRef::Owned(s.clone())),
        serde_json::Value::Array(arr) => {
            let mut values = Vec::with_capacity(arr.len());
            for v in arr {
                values.push(serde_json_to_value(v));
            }
            Value::new_list(values)
        }
        serde_json::Value::Object(obj) => {
            let map: BTreeMap<String, Value> = obj
                .iter()
                .map(|(k, v)| (k.clone(), serde_json_to_value(v)))
                .collect();
            Value::new_map(map)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fast_path_literals() {
        assert_eq!(parse_json_optimized("null").unwrap(), Value::Null);
        assert_eq!(parse_json_optimized("true").unwrap(), Value::Bool(true));
        assert_eq!(parse_json_optimized("false").unwrap(), Value::Bool(false));
        assert_eq!(parse_json_optimized("42").unwrap(), Value::Int(42));
        assert_eq!(parse_json_optimized("3.14").unwrap(), Value::Float(3.14));
    }

    #[test]
    fn test_fast_path_negative() {
        assert_eq!(parse_json_optimized("-1").unwrap(), Value::Int(-1));
        assert_eq!(parse_json_optimized("-99").unwrap(), Value::Int(-99));
        assert_eq!(
            parse_json_optimized("-9223372036854775808").unwrap(),
            Value::Int(i64::MIN)
        );
    }

    #[test]
    fn test_fast_path_string() {
        let val = parse_json_optimized(r#""hello""#).unwrap();
        match val {
            Value::String(StringRef::Owned(s)) => assert_eq!(s, "hello"),
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_number_array() {
        let val = parse_json_optimized("[1,2,3,4,5]").unwrap();
        match val {
            Value::List(l) => {
                assert_eq!(l.len(), 5);
                assert_eq!(l[0], Value::Int(1));
                assert_eq!(l[4], Value::Int(5));
            }
            _ => panic!("Expected list"),
        }
    }

    #[test]
    fn test_mixed_array() {
        let val = parse_json_optimized(r#"[1,"hello",true,null,3.14]"#).unwrap();
        match val {
            Value::List(l) => {
                assert_eq!(l.len(), 5);
                assert_eq!(l[0], Value::Int(1));
                assert_eq!(l[2], Value::Bool(true));
                assert_eq!(l[3], Value::Null);
            }
            _ => panic!("Expected list"),
        }
    }

    #[test]
    fn test_small_object() {
        let val = parse_json_optimized(r#"{"name":"Alice","age":30}"#).unwrap();
        match val {
            Value::Map(m) => {
                assert_eq!(m.len(), 2);
                assert!(m.contains_key("name"));
                assert!(m.contains_key("age"));
            }
            _ => panic!("Expected map"),
        }
    }

    #[test]
    fn test_nested_object() {
        let val = parse_json_optimized(r#"{"users":[{"name":"Alice"}]}"#).unwrap();
        match &val {
            Value::Map(m) => {
                assert!(m.contains_key("users"));
                match m.get("users") {
                    Some(Value::List(l)) => {
                        assert_eq!(l.len(), 1);
                        match &l[0] {
                            Value::Map(inner) => {
                                assert_eq!(
                                    inner.get("name"),
                                    Some(&Value::String(StringRef::Owned("Alice".to_string())))
                                );
                            }
                            _ => panic!("Expected inner map"),
                        }
                    }
                    other => panic!("Expected list, got {:?}", other),
                }
            }
            _ => panic!("Expected map"),
        }
    }

    #[test]
    fn test_large_nested_object() {
        let input = r#"{"users":[{"id":1,"name":"Alice","email":"alice@example.com","active":true},{"id":2,"name":"Bob","email":"bob@example.com","active":false}],"meta":{"total":2,"page":1}}"#;
        let val = parse_json_optimized(input).unwrap();
        match &val {
            Value::Map(m) => {
                assert!(m.contains_key("users"));
                assert!(m.contains_key("meta"));
                match m.get("users") {
                    Some(Value::List(l)) => assert_eq!(l.len(), 2),
                    other => panic!("Expected list of users, got {:?}", other),
                }
                match m.get("meta") {
                    Some(Value::Map(meta)) => {
                        assert_eq!(meta.get("total"), Some(&Value::Int(2)));
                        assert_eq!(meta.get("page"), Some(&Value::Int(1)));
                    }
                    other => panic!("Expected meta map, got {:?}", other),
                }
            }
            _ => panic!("Expected map, got {:?}", val),
        }
    }

    #[test]
    fn test_deeply_nested() {
        let val = parse_json_optimized(r#"{"a":{"b":{"c":{"d":42}}}}"#).unwrap();
        match &val {
            Value::Map(m) => match m.get("a") {
                Some(Value::Map(m2)) => match m2.get("b") {
                    Some(Value::Map(m3)) => match m3.get("c") {
                        Some(Value::Map(m4)) => {
                            assert_eq!(m4.get("d"), Some(&Value::Int(42)));
                        }
                        other => panic!("Expected c map, got {:?}", other),
                    },
                    other => panic!("Expected b map, got {:?}", other),
                },
                other => panic!("Expected a map, got {:?}", other),
            },
            _ => panic!("Expected map"),
        }
    }

    #[test]
    fn test_array_of_objects() {
        let val = parse_json_optimized(r#"[{"x":1,"y":2},{"x":3,"y":4},{"x":5,"y":6}]"#).unwrap();
        match &val {
            Value::List(l) => {
                assert_eq!(l.len(), 3);
                for item in l.iter() {
                    match item {
                        Value::Map(m) => {
                            assert!(m.contains_key("x"));
                            assert!(m.contains_key("y"));
                        }
                        _ => panic!("Expected map in array"),
                    }
                }
            }
            _ => panic!("Expected list"),
        }
    }

    #[test]
    fn test_whitespace_input() {
        assert_eq!(parse_json_optimized("").unwrap(), Value::Null);
        assert_eq!(parse_json_optimized("   ").unwrap(), Value::Null);
        assert_eq!(parse_json_optimized(" null ").unwrap(), Value::Null);
    }

    #[test]
    fn test_integer_overflow_falls_to_float() {
        let val = parse_json_optimized("99999999999999999999").unwrap();
        match val {
            Value::Float(_) | Value::Int(_) => {}
            _ => panic!("Expected numeric value"),
        }
    }

    #[test]
    fn test_escaped_string() {
        let val = parse_json_optimized(r#""hello \"world\"""#).unwrap();
        match val {
            Value::String(StringRef::Owned(s)) => assert_eq!(s, r#"hello "world""#),
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_escaped_string_in_object() {
        let val = parse_json_optimized(r#"{"msg":"hello\nworld"}"#).unwrap();
        match &val {
            Value::Map(m) => match m.get("msg") {
                Some(Value::String(StringRef::Owned(s))) => assert_eq!(s, "hello\nworld"),
                other => panic!("Expected string, got {:?}", other),
            },
            _ => panic!("Expected map"),
        }
    }

    #[test]
    fn test_unicode_escape() {
        let val = parse_json_optimized(r#""\u0041\u0042\u0043""#).unwrap();
        match val {
            Value::String(StringRef::Owned(s)) => assert_eq!(s, "ABC"),
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_object_with_nested_array() {
        let val = parse_json_optimized(r#"{"data":[1,2,3]}"#).unwrap();
        match val {
            Value::Map(m) => {
                assert!(m.contains_key("data"));
                match m.get("data") {
                    Some(Value::List(l)) => assert_eq!(l.len(), 3),
                    other => panic!("Expected list, got {:?}", other),
                }
            }
            _ => panic!("Expected map"),
        }
    }

    #[test]
    fn test_empty_object() {
        let val = parse_json_optimized("{}").unwrap();
        match val {
            Value::Map(m) => assert!(m.is_empty()),
            _ => panic!("Expected empty map"),
        }
    }

    #[test]
    fn test_empty_array() {
        let val = parse_json_optimized("[]").unwrap();
        match val {
            Value::List(l) => assert!(l.is_empty()),
            _ => panic!("Expected empty list"),
        }
    }

    #[test]
    fn test_nested_empty_structures() {
        let val = parse_json_optimized(r#"{"a":[],"b":{},"c":[{},{}]}"#).unwrap();
        match &val {
            Value::Map(m) => {
                assert_eq!(m.len(), 3);
                match m.get("a") {
                    Some(Value::List(l)) => assert!(l.is_empty()),
                    other => panic!("Expected empty list, got {:?}", other),
                }
                match m.get("b") {
                    Some(Value::Map(m2)) => assert!(m2.is_empty()),
                    other => panic!("Expected empty map, got {:?}", other),
                }
            }
            _ => panic!("Expected map"),
        }
    }

    #[test]
    fn test_benchmark_large_input() {
        // This is the exact input used in the benchmark
        let large_json = r#"{
            "users": [
                {"id":1,"name":"Alice","email":"alice@example.com","active":true},
                {"id":2,"name":"Bob","email":"bob@example.com","active":false},
                {"id":3,"name":"Charlie","email":"charlie@example.com","active":true},
                {"id":4,"name":"Diana","email":"diana@example.com","active":true},
                {"id":5,"name":"Eve","email":"eve@example.com","active":false}
            ],
            "meta": {"total":5,"page":1,"per_page":10}
        }"#;
        let val = parse_json_optimized(large_json).unwrap();
        match &val {
            Value::Map(m) => {
                match m.get("users") {
                    Some(Value::List(l)) => {
                        assert_eq!(l.len(), 5);
                        // Check first user
                        match &l[0] {
                            Value::Map(user) => {
                                assert_eq!(user.get("id"), Some(&Value::Int(1)));
                                assert_eq!(
                                    user.get("name"),
                                    Some(&Value::String(StringRef::Owned("Alice".to_string())))
                                );
                                assert_eq!(user.get("active"), Some(&Value::Bool(true)));
                            }
                            _ => panic!("Expected user map"),
                        }
                    }
                    other => panic!("Expected users list, got {:?}", other),
                }
                match m.get("meta") {
                    Some(Value::Map(meta)) => {
                        assert_eq!(meta.get("total"), Some(&Value::Int(5)));
                        assert_eq!(meta.get("per_page"), Some(&Value::Int(10)));
                    }
                    other => panic!("Expected meta map, got {:?}", other),
                }
            }
            _ => panic!("Expected map"),
        }
    }

    #[test]
    fn test_float_values_in_object() {
        let val = parse_json_optimized(r#"{"pi":3.14159,"e":2.71828}"#).unwrap();
        match &val {
            Value::Map(m) => match m.get("pi") {
                Some(Value::Float(f)) => assert!((*f - 3.14159).abs() < 1e-10),
                other => panic!("Expected float, got {:?}", other),
            },
            _ => panic!("Expected map"),
        }
    }

    #[test]
    fn test_negative_numbers_in_array() {
        let val = parse_json_optimized("[-1,-2,-3,0,1]").unwrap();
        match val {
            Value::List(l) => {
                assert_eq!(l[0], Value::Int(-1));
                assert_eq!(l[1], Value::Int(-2));
                assert_eq!(l[2], Value::Int(-3));
                assert_eq!(l[3], Value::Int(0));
                assert_eq!(l[4], Value::Int(1));
            }
            _ => panic!("Expected list"),
        }
    }

    #[test]
    fn test_scientific_notation() {
        let val = parse_json_optimized("1.5e10").unwrap();
        match val {
            Value::Float(f) => assert!((f - 1.5e10).abs() < 1.0),
            _ => panic!("Expected float"),
        }
    }
}
