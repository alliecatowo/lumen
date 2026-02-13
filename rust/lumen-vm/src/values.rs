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
            Value::Float(f) => f.to_string(),
            Value::String(StringRef::Owned(s)) => s.clone(),
            Value::String(StringRef::Interned(id)) => format!("<interned:{}>", id),
            Value::Bytes(b) => format!("<bytes:{}>", b.len()),
            Value::List(l) => format!("[{} items]", l.len()),
            Value::Map(m) => format!("{{{} entries}}", m.len()),
            Value::Record(r) => format!("{}{{...}}", r.type_name),
            Value::Union(u) => format!("{}(...)", u.tag),
            Value::TraceRef(t) => format!("<trace:{}:{}>", t.trace_id, t.seq),
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self { Value::Int(n) => Some(*n), _ => None }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self { Value::Float(f) => Some(*f), Value::Int(n) => Some(*n as f64), _ => None }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_string())
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
            _ => false,
        }
    }
}
