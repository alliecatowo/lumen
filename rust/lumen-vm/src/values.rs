//! Tagged value representation for the Lumen VM.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// Runtime values in the Lumen VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(StringRef),
    Bytes(Vec<u8>),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
    Record(RecordValue),
    Union(UnionValue),
    TraceRef(TraceRefValue),
}

/// A string reference (interned ID or owned)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StringRef {
    Interned(u32),
    Owned(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordValue {
    pub type_name: String,
    pub fields: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnionValue {
    pub tag: String,
    pub payload: Box<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceRefValue {
    pub trace_id: String,
    pub seq: u64,
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            Value::Float(f) => *f != 0.0,
            Value::String(StringRef::Owned(s)) => !s.is_empty(),
            Value::String(StringRef::Interned(_)) => true,
            Value::List(l) => !l.is_empty(),
            _ => true,
        }
    }

    pub fn as_string(&self) -> String {
        match self {
            Value::Null => "null".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(n) => n.to_string(),
            Value::Float(f) => format_float(*f),
            Value::String(StringRef::Owned(s)) => s.clone(),
            Value::String(StringRef::Interned(id)) => format!("<interned:{}>", id),
            Value::Bytes(b) => format!("<bytes:{}>", b.len()),
            Value::List(l) => format!("[{}]", l.iter().map(|v| v.display_pretty()).collect::<Vec<_>>().join(", ")),
            Value::Map(m) => format!("{{{}}}", m.iter().map(|(k, v)| format!("{}: {}", k, v.display_pretty())).collect::<Vec<_>>().join(", ")),
            Value::Record(r) => {
                let fields = r.fields.iter().map(|(k, v)| format!("{}: {}", k, v.display_pretty())).collect::<Vec<_>>().join(", ");
                format!("{}({})", r.type_name, fields)
            }
            Value::Union(u) => {
                if matches!(*u.payload, Value::Null) {
                    u.tag.clone()
                } else {
                    format!("{}({})", u.tag, u.payload.display_pretty())
                }
            }
            Value::TraceRef(t) => format!("<trace:{}:{}>", t.trace_id, t.seq),
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self { Value::Int(n) => Some(*n), _ => None }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self { Value::Float(f) => Some(*f), Value::Int(n) => Some(*n as f64), _ => None }
    }

    pub fn as_list(&self) -> Option<&Vec<Value>> {
        match self { Value::List(l) => Some(l), _ => None }
    }

    pub fn as_record(&self) -> Option<&RecordValue> {
        match self { Value::Record(r) => Some(r), _ => None }
    }

    pub fn as_map(&self) -> Option<&BTreeMap<String, Value>> {
        match self { Value::Map(m) => Some(m), _ => None }
    }

    /// Pretty display for user-facing output
    pub fn display_pretty(&self) -> String {
        match self {
            Value::String(StringRef::Owned(s)) => s.clone(),
            Value::String(StringRef::Interned(_)) => self.as_string(),
            Value::Null => "null".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(n) => n.to_string(),
            Value::Float(f) => format_float(*f),
            Value::List(l) => {
                let items: Vec<String> = l.iter().map(|v| v.display_quoted()).collect();
                format!("[{}]", items.join(", "))
            }
            Value::Map(m) => {
                let entries: Vec<String> = m.iter().map(|(k, v)| format!("\"{}\": {}", k, v.display_quoted())).collect();
                format!("{{{}}}", entries.join(", "))
            }
            Value::Record(r) => {
                if r.fields.is_empty() {
                    format!("{}()", r.type_name)
                } else {
                    let fields: Vec<String> = r.fields.iter().map(|(k, v)| format!("{}: {}", k, v.display_quoted())).collect();
                    format!("{}({})", r.type_name, fields.join(", "))
                }
            }
            Value::Union(u) => {
                if matches!(*u.payload, Value::Null) {
                    u.tag.clone()
                } else {
                    format!("{}({})", u.tag, u.payload.display_pretty())
                }
            }
            _ => self.as_string(),
        }
    }

    /// Display with quotes for strings (used inside containers)
    fn display_quoted(&self) -> String {
        match self {
            Value::String(StringRef::Owned(s)) => format!("\"{}\"", s),
            _ => self.display_pretty(),
        }
    }
}

/// Format a float nicely (avoid unnecessary trailing zeros but keep at least one decimal)
fn format_float(f: f64) -> String {
    if f == f.floor() && f.abs() < 1e15 {
        format!("{:.1}", f)
    } else {
        format!("{}", f)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_pretty())
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::String(StringRef::Owned(a)), Value::String(StringRef::Owned(b))) => a == b,
            (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
            (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
            (Value::List(a), Value::List(b)) => a == b,
            (Value::Map(a), Value::Map(b)) => a == b,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_pretty_string() {
        let v = Value::String(StringRef::Owned("hello".into()));
        assert_eq!(v.display_pretty(), "hello");
    }

    #[test]
    fn test_display_pretty_list() {
        let v = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(v.display_pretty(), "[1, 2, 3]");
    }

    #[test]
    fn test_display_pretty_record() {
        let mut fields = BTreeMap::new();
        fields.insert("name".to_string(), Value::String(StringRef::Owned("Alice".into())));
        fields.insert("age".to_string(), Value::Int(30));
        let v = Value::Record(RecordValue { type_name: "Person".into(), fields });
        assert_eq!(v.display_pretty(), "Person(age: 30, name: \"Alice\")");
    }

    #[test]
    fn test_truthiness() {
        assert!(!Value::Null.is_truthy());
        assert!(!Value::Bool(false).is_truthy());
        assert!(Value::Bool(true).is_truthy());
        assert!(!Value::Int(0).is_truthy());
        assert!(Value::Int(1).is_truthy());
        assert!(!Value::String(StringRef::Owned("".into())).is_truthy());
        assert!(Value::String(StringRef::Owned("hello".into())).is_truthy());
    }

    #[test]
    fn test_as_helpers() {
        assert_eq!(Value::Int(42).as_int(), Some(42));
        assert_eq!(Value::Float(3.14).as_float(), Some(3.14));
        assert_eq!(Value::Int(42).as_float(), Some(42.0));
        assert!(Value::List(vec![]).as_list().is_some());
        assert!(Value::Null.as_list().is_none());
    }
}
