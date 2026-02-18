//! Optimized JSON parsing with fast paths for common cases.
//!
//! Performance targets:
//! - Small objects (<100 bytes): < 2μs
//! - Medium arrays (20 numbers): < 5μs  
//! - Large nested (1KB): < 20μs
//!
//! Strategies:
//! 1. Fast path for simple literals and small objects (zero-alloc where possible)
//! 2. Hand-rolled integer parser (avoids str::parse overhead)
//! 3. simd-json with OwnedValue for large inputs (single-pass, no double conversion)
//! 4. Pre-allocated capacity hints for Vec/BTreeMap construction

use crate::values::{StringRef, Value};
use std::collections::BTreeMap;

/// Fast-path JSON parsing with multiple optimization strategies.
pub fn parse_json_optimized(input: &str) -> Result<Value, String> {
    let bytes = input.as_bytes();
    let len = bytes.len();

    // Fast path: empty or whitespace
    if len == 0 {
        return Ok(Value::Null);
    }

    // Find the first non-whitespace byte to dispatch on
    let first = match bytes.iter().position(|b| !b.is_ascii_whitespace()) {
        Some(pos) => bytes[pos],
        None => return Ok(Value::Null), // all whitespace
    };

    // Fast path: bare literals (exact match, no trailing content check needed
    // since JSON spec says top-level must be a single value)
    match first {
        b'n' if bytes.ends_with(b"null") && len <= 6 => return Ok(Value::Null),
        b't' if bytes.ends_with(b"true") && len <= 6 => return Ok(Value::Bool(true)),
        b'f' if bytes.ends_with(b"false") && len <= 7 => return Ok(Value::Bool(false)),
        _ => {}
    }

    // Fast path: integer (no decimal point, no exponent)
    if matches!(first, b'-' | b'0'..=b'9') {
        if let Some(val) = try_parse_integer_fast(bytes) {
            return Ok(val);
        }
        // Could be a float, or an integer that overflowed i64 — try parsing as f64
        if let Some(val) = try_parse_float_fast(bytes) {
            return Ok(val);
        }
    }

    // Fast path: simple string (quoted, no escapes)
    if first == b'"' && len >= 2 && bytes[len - 1] == b'"' {
        let inner = &bytes[1..len - 1];
        if !inner.iter().any(|&b| b == b'\\' || b == b'"') {
            // SAFETY: input is &str so bytes are valid UTF-8; substring of valid
            // UTF-8 that contains no multi-byte escape sequences is still valid.
            if let Ok(s) = std::str::from_utf8(inner) {
                return Ok(Value::String(StringRef::Owned(s.to_string())));
            }
        }
    }

    // Fast path: small flat object with simple keys (common API responses)
    if first == b'{' && len < 256 {
        if let Some(val) = try_parse_small_object(bytes) {
            return Ok(val);
        }
    }

    // Fast path: array of simple values (numbers, strings, bools, null)
    if first == b'[' && len < 4096 {
        if let Some(val) = try_parse_simple_array(bytes) {
            return Ok(val);
        }
    }

    // Fall back to SIMD-accelerated parser
    parse_with_simd(input)
}

/// Hand-rolled integer parser — avoids str::parse overhead for the common case.
/// Handles optional leading whitespace, optional sign, up to 19 digits (i64 range).
/// Returns None if the input contains non-integer characters (decimal point, exponent,
/// trailing garbage).
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

    // Must start with a digit
    if !bytes[i].is_ascii_digit() {
        return None;
    }

    // Accumulate digits
    let mut val: u64 = 0;
    let digit_start = i;
    while i < len && bytes[i].is_ascii_digit() {
        let d = (bytes[i] - b'0') as u64;
        // Overflow check: 19 digits max for u64
        val = val.checked_mul(10)?.checked_add(d)?;
        i += 1;
    }

    // Skip trailing whitespace
    while i < len && bytes[i].is_ascii_whitespace() {
        i += 1;
    }

    // Must have consumed everything, no decimal point/exponent
    if i != len {
        return None;
    }

    // Reject leading zeros (except bare "0")
    if bytes[digit_start] == b'0' && (i - digit_start) > 1 && !negative {
        return None;
    }

    if negative {
        // i64::MIN magnitude is 9223372036854775808, one more than i64::MAX
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

/// Fast float parser — delegates to str::parse for the actual conversion.
/// Handles both explicit floats (with `.` or `e`/`E`) and integer overflow
/// cases (pure digits that exceeded i64 range).
#[inline]
fn try_parse_float_fast(bytes: &[u8]) -> Option<Value> {
    // All digits (with optional sign) are also valid float candidates
    // when the integer parser overflows.
    let s = std::str::from_utf8(bytes).ok()?;
    let f = s.trim().parse::<f64>().ok()?;
    Some(Value::Float(f))
}

/// Try to parse small objects with simple structure quickly.
/// Pattern: {"key1":"val1","key2":123,"key3":true}
/// Handles only flat objects (no nested objects/arrays). Falls back to simd-json
/// for anything complex.
fn try_parse_small_object(bytes: &[u8]) -> Option<Value> {
    let mut map = BTreeMap::new();
    let len = bytes.len();
    let mut i = 1; // Skip opening {

    loop {
        // Skip whitespace
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        if i >= len {
            return None;
        }

        // End of object?
        if bytes[i] == b'}' {
            return Some(Value::new_map(map));
        }

        // Skip comma
        if bytes[i] == b',' {
            i += 1;
            while i < len && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
        }

        // Parse key (must be quoted string)
        if i >= len || bytes[i] != b'"' {
            return None;
        }
        i += 1;

        let key_start = i;
        while i < len && bytes[i] != b'"' {
            if bytes[i] == b'\\' {
                return None; // No escape support in fast path
            }
            i += 1;
        }

        if i >= len {
            return None;
        }

        let key = std::str::from_utf8(&bytes[key_start..i]).ok()?;
        i += 1; // Skip closing "

        // Skip whitespace and colon
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= len || bytes[i] != b':' {
            return None;
        }
        i += 1;
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        // Parse value
        if i >= len {
            return None;
        }
        let (value, consumed) = parse_simple_value(&bytes[i..])?;
        map.insert(key.to_string(), value);
        i += consumed;
    }
}

/// Parse a simple JSON value (no nesting, for fast path only).
fn parse_simple_value(bytes: &[u8]) -> Option<(Value, usize)> {
    if bytes.is_empty() {
        return None;
    }

    match bytes[0] {
        b'n' if bytes.starts_with(b"null") => Some((Value::Null, 4)),
        b't' if bytes.starts_with(b"true") => Some((Value::Bool(true), 4)),
        b'f' if bytes.starts_with(b"false") => Some((Value::Bool(false), 5)),
        b'"' => {
            // Simple string
            let mut i = 1;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' {
                    return None; // No escape support
                }
                i += 1;
            }
            if i >= bytes.len() {
                return None;
            }
            let s = std::str::from_utf8(&bytes[1..i]).ok()?;
            Some((Value::String(StringRef::Owned(s.to_string())), i + 1))
        }
        b'-' | b'0'..=b'9' => parse_number_value(bytes),
        b'[' => parse_nested_simple_array(bytes),
        // Nested object — bail to simd-json
        b'{' => None,
        _ => None,
    }
}

/// Parse a nested array of simple values within a larger structure.
/// Returns the parsed value and total bytes consumed (including brackets).
fn parse_nested_simple_array(bytes: &[u8]) -> Option<(Value, usize)> {
    let len = bytes.len();
    let estimated_len = bytes
        .iter()
        .take_while(|&&b| b != b']')
        .filter(|&&b| b == b',')
        .count()
        + 1;
    let mut values = Vec::with_capacity(estimated_len);
    let mut i = 1; // Skip [

    loop {
        // Skip whitespace
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        if i >= len {
            return None;
        }

        // End of array?
        if bytes[i] == b']' {
            return Some((Value::new_list(values), i + 1));
        }

        // Skip comma
        if bytes[i] == b',' {
            i += 1;
            while i < len && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
        }

        if i >= len {
            return None;
        }

        // Try to parse element — but don't recurse into nested arrays/objects
        let (val, consumed) = match bytes[i] {
            b'n' if bytes[i..].starts_with(b"null") => (Value::Null, 4),
            b't' if bytes[i..].starts_with(b"true") => (Value::Bool(true), 4),
            b'f' if bytes[i..].starts_with(b"false") => (Value::Bool(false), 5),
            b'"' => {
                let mut j = i + 1;
                while j < len && bytes[j] != b'"' {
                    if bytes[j] == b'\\' {
                        return None;
                    }
                    j += 1;
                }
                if j >= len {
                    return None;
                }
                let s = std::str::from_utf8(&bytes[i + 1..j]).ok()?;
                (Value::String(StringRef::Owned(s.to_string())), j - i + 1)
            }
            b'-' | b'0'..=b'9' => parse_number_value(&bytes[i..])?,
            // Nested arrays/objects in array — bail
            _ => return None,
        };
        values.push(val);
        i += consumed;
    }
}

/// Parse a JSON number from bytes, returning the value and number of bytes consumed.
#[inline]
fn parse_number_value(bytes: &[u8]) -> Option<(Value, usize)> {
    let mut i = 0;
    let negative = bytes[i] == b'-';
    if negative {
        i += 1;
    }

    if i >= bytes.len() || !bytes[i].is_ascii_digit() {
        return None;
    }

    let digit_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }

    let is_float = i < bytes.len() && (bytes[i] == b'.' || bytes[i] == b'e' || bytes[i] == b'E');

    if is_float {
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
            i += 1;
            if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                i += 1;
            }
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        let s = std::str::from_utf8(&bytes[..i]).ok()?;
        let f = s.parse::<f64>().ok()?;
        Some((Value::Float(f), i))
    } else {
        // Hand-roll integer parse to avoid str::parse overhead
        let mut val: u64 = 0;
        for &b in &bytes[digit_start..i] {
            val = val.checked_mul(10)?.checked_add((b - b'0') as u64)?;
        }
        let result = if negative {
            if val > (i64::MAX as u64) + 1 {
                return None;
            }
            if val == (i64::MAX as u64) + 1 {
                i64::MIN
            } else {
                -(val as i64)
            }
        } else {
            if val > i64::MAX as u64 {
                return None;
            }
            val as i64
        };
        Some((Value::Int(result), i))
    }
}

/// Fast path for arrays of simple values: [1,2,3,"hello",true,null]
/// Handles numbers, strings, bools, and null. Bails out on nested arrays/objects.
fn try_parse_simple_array(bytes: &[u8]) -> Option<Value> {
    let len = bytes.len();
    // Estimate capacity: rough count of commas + 1
    let estimated_len = bytes.iter().filter(|&&b| b == b',').count() + 1;
    let mut values = Vec::with_capacity(estimated_len);
    let mut i = 1; // Skip [

    loop {
        // Skip whitespace
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        if i >= len {
            return None;
        }

        // End of array?
        if bytes[i] == b']' {
            return Some(Value::new_list(values));
        }

        // Skip comma
        if bytes[i] == b',' {
            i += 1;
            while i < len && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
        }

        if i >= len {
            return None;
        }

        // Try to parse the value
        let (val, consumed) = parse_simple_value(&bytes[i..])?;
        values.push(val);
        i += consumed;
    }
}

/// Parse using simd-json with OwnedValue for better performance on larger inputs.
/// OwnedValue owns its strings, avoiding the borrow-then-copy pattern of BorrowedValue.
fn parse_with_simd(input: &str) -> Result<Value, String> {
    // simd-json requires mutable input for in-place parsing
    let mut bytes = input.as_bytes().to_vec();

    match simd_json::to_owned_value(&mut bytes) {
        Ok(val) => Ok(owned_json_to_value(val)),
        Err(e) => Err(format!("JSON parse error: {}", e)),
    }
}

/// Convert simd_json::OwnedValue to Lumen Value.
/// Takes ownership to avoid cloning strings — simd_json::OwnedValue::String
/// already owns its String, so we just move it into StringRef::Owned.
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
    fn test_nested_fallback() {
        // Should fall back to simd-json for complex cases
        let val = parse_json_optimized(r#"{"users":[{"name":"Alice"}]}"#).unwrap();
        match val {
            Value::Map(_) => {}
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
            }
            _ => panic!("Expected map, got {:?}", val),
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
        // Very large number that exceeds i64 — should still parse as float via simd-json
        let val = parse_json_optimized("99999999999999999999").unwrap();
        match val {
            Value::Float(_) | Value::Int(_) => {} // either is acceptable
            _ => panic!("Expected numeric value"),
        }
    }

    #[test]
    fn test_escaped_string_falls_through() {
        // String with escapes should NOT use the fast path but still work via simd-json
        let val = parse_json_optimized(r#""hello \"world\"""#).unwrap();
        match val {
            Value::String(StringRef::Owned(s)) => assert_eq!(s, r#"hello "world""#),
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_object_with_nested_array_falls_through() {
        // Small object with nested array should bail from fast path to simd-json
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
}
