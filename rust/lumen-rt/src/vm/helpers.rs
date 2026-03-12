//! Free helper functions used by the VM (not methods on VM).

use super::*;
use std::collections::BTreeMap;

pub(crate) fn process_instance_id(value: Option<&Value>) -> Option<u64> {
    let Value::Record(r) = value? else {
        return None;
    };
    let Value::Int(id) = r.fields.get("__instance_id")? else {
        return None;
    };
    if *id < 0 {
        return None;
    }
    Some(*id as u64)
}

pub(crate) fn merged_policy_for_tool(module: &LirModule, alias: &str) -> serde_json::Value {
    let mut merged = serde_json::Map::new();
    for policy in &module.policies {
        if policy.tool_alias != alias {
            continue;
        }
        if let serde_json::Value::Object(obj) = &policy.grants {
            for (k, v) in obj {
                merged.insert(k.clone(), v.clone());
            }
        }
    }
    serde_json::Value::Object(merged)
}

pub(crate) fn validate_tool_policy(
    policy: &serde_json::Value,
    args: &serde_json::Value,
) -> Result<(), String> {
    let serde_json::Value::Object(policy_obj) = policy else {
        return Ok(());
    };
    let serde_json::Value::Object(args_obj) = args else {
        return Ok(());
    };

    for (key, constraint) in policy_obj {
        match key.as_str() {
            "domain" => {
                let pattern = constraint
                    .as_str()
                    .ok_or_else(|| "domain constraint must be a string".to_string())?;
                let url = args_obj
                    .get("url")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "domain policy requires string 'url' argument".to_string())?;
                if !domain_matches(pattern, url) {
                    return Err(format!("domain '{}' does not allow '{}'", pattern, url));
                }
            }
            "timeout_ms" => {
                let max_timeout = constraint
                    .as_i64()
                    .ok_or_else(|| "timeout_ms constraint must be an integer".to_string())?;
                if let Some(actual) = args_obj.get("timeout_ms").and_then(|v| v.as_i64()) {
                    if actual > max_timeout {
                        return Err(format!(
                            "timeout_ms {} exceeds allowed {}",
                            actual, max_timeout
                        ));
                    }
                }
            }
            _ if key.starts_with("max_") => {
                let limit = constraint
                    .as_i64()
                    .ok_or_else(|| format!("{} constraint must be an integer", key))?;
                if let Some(actual) = args_obj.get(key).and_then(|v| v.as_i64()) {
                    if actual > limit {
                        return Err(format!("{} {} exceeds allowed {}", key, actual, limit));
                    }
                }
            }
            _ => {
                if let Some(actual) = args_obj.get(key) {
                    if actual != constraint {
                        return Err(format!(
                            "argument '{}' value {} violates required {}",
                            key, actual, constraint
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

pub(crate) fn domain_matches(pattern: &str, url: &str) -> bool {
    let host = extract_host(url);
    if host.is_empty() {
        return false;
    }

    // Compare case-insensitively without allocating new strings
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return host.eq_ignore_ascii_case(suffix)
            || (host.len() > suffix.len() + 1
                && host.as_bytes()[host.len() - suffix.len() - 1] == b'.'
                && host[host.len() - suffix.len()..].eq_ignore_ascii_case(suffix));
    }
    host.eq_ignore_ascii_case(pattern)
}

/// Extract host from a URL, returning a borrowed slice to avoid allocation.
pub(crate) fn extract_host(url: &str) -> &str {
    let without_scheme = if let Some((_, rest)) = url.split_once("://") {
        rest
    } else {
        url
    };
    without_scheme
        .split('/')
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
}

pub(crate) fn parse_machine_expr_json(value: &serde_json::Value) -> Option<MachineExpr> {
    let kind = value.get("kind")?.as_str()?;
    match kind {
        "int" => Some(MachineExpr::Int(value.get("value")?.as_i64()?)),
        "float" => Some(MachineExpr::Float(value.get("value")?.as_f64()?)),
        "string" => Some(MachineExpr::String(
            value.get("value")?.as_str()?.to_string(),
        )),
        "bool" => Some(MachineExpr::Bool(value.get("value")?.as_bool()?)),
        "null" => Some(MachineExpr::Null),
        "ident" => Some(MachineExpr::Ident(
            value.get("value")?.as_str()?.to_string(),
        )),
        "unary" => Some(MachineExpr::Unary {
            op: value.get("op")?.as_str()?.to_string(),
            expr: Box::new(parse_machine_expr_json(value.get("expr")?)?),
        }),
        "bin" => Some(MachineExpr::Bin {
            op: value.get("op")?.as_str()?.to_string(),
            lhs: Box::new(parse_machine_expr_json(value.get("lhs")?)?),
            rhs: Box::new(parse_machine_expr_json(value.get("rhs")?)?),
        }),
        _ => None,
    }
}

pub(crate) fn future_schedule_from_addons(addons: &[LirAddon]) -> FutureSchedule {
    for addon in addons {
        if addon.kind != "directive" {
            continue;
        }
        let Some(raw) = addon.name.as_deref() else {
            continue;
        };
        let (name, raw_value) = match raw.split_once('=') {
            Some((k, v)) => (k.trim(), Some(v.trim())),
            None => (raw.trim(), None),
        };
        let key = name.trim_start_matches('@').to_ascii_lowercase();
        if key != "deterministic" {
            continue;
        }
        let parsed = raw_value
            .map(strip_quote_wrappers)
            .and_then(parse_bool_like)
            .unwrap_or(true);
        return if parsed {
            FutureSchedule::DeferredFifo
        } else {
            FutureSchedule::Eager
        };
    }
    FutureSchedule::Eager
}

pub(crate) fn strip_quote_wrappers(s: &str) -> &str {
    let trimmed = s.trim();
    if let Some(inner) = trimmed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        return inner.trim();
    }
    if let Some(inner) = trimmed
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
    {
        return inner.trim();
    }
    trimmed
}

pub(crate) fn parse_bool_like(raw: &str) -> Option<bool> {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("true")
        || trimmed.eq_ignore_ascii_case("yes")
        || trimmed.eq_ignore_ascii_case("on")
        || trimmed == "1"
    {
        Some(true)
    } else if trimmed.eq_ignore_ascii_case("false")
        || trimmed.eq_ignore_ascii_case("no")
        || trimmed.eq_ignore_ascii_case("off")
        || trimmed == "0"
    {
        Some(false)
    } else {
        None
    }
}

/// Convert a Lumen Value to a serde_json Value.
pub(crate) fn value_to_json(
    val: &Value,
    strings: &lumen_core::strings::StringTable,
) -> serde_json::Value {
    match val {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(n) => serde_json::json!(*n),
        Value::Float(f) => serde_json::json!(*f),
        Value::String(StringRef::Owned(s)) => serde_json::Value::String(s.clone()),
        Value::String(StringRef::Interned(_)) => serde_json::Value::String(val.as_string()),
        Value::List(l) => {
            serde_json::Value::Array(l.iter().map(|v| value_to_json(v, strings)).collect())
        }
        Value::Tuple(t) => {
            serde_json::Value::Array(t.iter().map(|v| value_to_json(v, strings)).collect())
        }
        Value::Set(s) => {
            serde_json::Value::Array(s.iter().map(|v| value_to_json(v, strings)).collect())
        }
        Value::Map(m) => {
            let obj: serde_json::Map<String, serde_json::Value> = m
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v, strings)))
                .collect();
            serde_json::Value::Object(obj)
        }
        Value::Record(r) => {
            let mut obj = serde_json::Map::new();
            obj.insert(
                "__type".to_string(),
                serde_json::Value::String(r.type_name.clone()),
            );
            for (k, v) in &r.fields {
                obj.insert(k.clone(), value_to_json(v, strings));
            }
            serde_json::Value::Object(obj)
        }
        Value::Union(u) => {
            let mut obj = serde_json::Map::new();
            let tag_str = strings.resolve(u.tag).unwrap_or("?").to_string();
            obj.insert("__tag".to_string(), serde_json::Value::String(tag_str));
            obj.insert("__payload".to_string(), value_to_json(&u.payload, strings));
            serde_json::Value::Object(obj)
        }
        _ => serde_json::Value::Null,
    }
}

/// Convert a serde_json Value to a Lumen Value.
pub(crate) fn json_to_value(val: &serde_json::Value) -> Value {
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
        serde_json::Value::Array(arr) => Value::new_list(arr.iter().map(json_to_value).collect()),
        serde_json::Value::Object(obj) => {
            let map: BTreeMap<String, Value> = obj
                .iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect();
            Value::new_map(map)
        }
    }
}

/// Simple base64 encode (no external dependency).
pub(crate) fn simple_base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let encoded_len = 4 * data.len().div_ceil(3);
    let mut result = String::with_capacity(encoded_len);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

/// Simple base64 decode.
pub(crate) fn simple_base64_decode(s: &str) -> Option<Vec<u8>> {
    // Lookup table: maps ASCII byte value -> base64 index (0-63), 255 = invalid
    const INVALID: u8 = 255;
    const DECODE_TABLE: [u8; 256] = {
        let mut table = [INVALID; 256];
        let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut i = 0;
        while i < 64 {
            table[chars[i] as usize] = i as u8;
            i += 1;
        }
        table
    };

    let mut result = Vec::with_capacity(s.len() * 3 / 4);
    let bytes: Vec<u8> = s.bytes().filter(|&b| b != b'\n' && b != b'\r').collect();
    for chunk in bytes.chunks(4) {
        if chunk.len() < 4 {
            break;
        }
        let mut vals = [0u8; 4];
        for (i, &b) in chunk.iter().enumerate() {
            if b == b'=' {
                vals[i] = 0;
            } else {
                let v = DECODE_TABLE[b as usize];
                if v == INVALID {
                    return None;
                }
                vals[i] = v;
            }
        }
        let triple = ((vals[0] as u32) << 18)
            | ((vals[1] as u32) << 12)
            | ((vals[2] as u32) << 6)
            | (vals[3] as u32);
        result.push(((triple >> 16) & 0xFF) as u8);
        if chunk[2] != b'=' {
            result.push(((triple >> 8) & 0xFF) as u8);
        }
        if chunk[3] != b'=' {
            result.push((triple & 0xFF) as u8);
        }
    }
    Some(result)
}

/// Borrow a `&str` from a `Value` when possible, or produce an owned
/// conversion via `Cow`. For `StringRef::Owned` this is zero-copy; for
/// `StringRef::Interned` it resolves via `StringTable`; for non-string
/// types it falls back to `as_string()`.
pub(crate) fn value_to_str_cow<'a>(
    val: &'a Value,
    strings: &'a lumen_core::strings::StringTable,
) -> std::borrow::Cow<'a, str> {
    match val {
        Value::String(StringRef::Owned(s)) => std::borrow::Cow::Borrowed(s.as_str()),
        Value::String(StringRef::Interned(id)) => {
            std::borrow::Cow::Borrowed(strings.resolve(*id).unwrap_or(""))
        }
        _ => std::borrow::Cow::Owned(val.as_string()),
    }
}
