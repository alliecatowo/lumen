//! HeapValue: the canonical heap-allocated value type for Lumen.
//!
//! NbValue::TAG_PTR points to Arc<HeapValue>. This is the ONLY heap type
//! in the Lumen runtime. There is no bridge to Value.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use num_bigint::BigInt;

use crate::nb_value::NbValue;

/// Heap-allocated Lumen values. NbValue::TAG_PTR points to Arc<HeapValue>.
/// All variants are reference-counted for cheap sharing.
#[derive(Debug, Clone)]
pub enum HeapValue {
    /// UTF-8 string
    Str(Arc<str>),
    /// Raw bytes
    Bytes(Arc<[u8]>),
    /// Arbitrary precision integer (rare, only when > 48 bits)
    BigInt(Arc<BigInt>),
    /// Ordered list of NbValues
    List(Arc<Vec<NbValue>>),
    /// Fixed-length tuple of NbValues
    Tuple(Arc<Vec<NbValue>>),
    /// Unordered unique set
    Set(Arc<BTreeSet<NbValue>>),
    /// String-keyed map
    Map(Arc<BTreeMap<String, NbValue>>),
    /// Named record with typed fields
    Record(Arc<RecordData>),
    /// Tagged union variant (enum payload)
    Union(UnionData),
    /// Captured closure
    Closure(Arc<ClosureData>),
    /// Async future
    Future(Arc<FutureData>),
    /// Trace/span reference
    TraceRef(u64),
}

#[derive(Debug, Clone)]
pub struct RecordData {
    pub type_name: Arc<str>,
    pub fields: BTreeMap<String, NbValue>,
}

#[derive(Debug, Clone)]
pub struct UnionData {
    pub tag: Arc<str>,
    pub payload: NbValue,
}

#[derive(Debug, Clone)]
pub struct ClosureData {
    pub cell_idx: usize,
    pub captures: Vec<NbValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FutureStatus {
    Pending,
    Completed(NbValue),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct FutureData {
    pub id: u64,
    pub status: FutureStatus,
    pub schedule: FutureSchedule,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FutureSchedule {
    Eager,
    DeferredFifo,
}

impl From<crate::values::Value> for HeapValue {
    fn from(value: crate::values::Value) -> Self {
        use crate::values::StringRef;
        match value {
            crate::values::Value::String(StringRef::Owned(s)) => HeapValue::Str(Arc::from(s)),
            crate::values::Value::String(StringRef::Interned(id)) => {
                HeapValue::Str(Arc::from(format!("<interned:{}>", id)))
            }
            crate::values::Value::Bytes(b) => HeapValue::Bytes(Arc::from(b.into_boxed_slice())),
            crate::values::Value::BigInt(n) => HeapValue::BigInt(Arc::new(n)),
            crate::values::Value::List(list) => HeapValue::List(list),
            crate::values::Value::Tuple(tuple) => HeapValue::Tuple(tuple),
            crate::values::Value::Set(set) => {
                let converted: BTreeSet<NbValue> = set
                    .iter()
                    .cloned()
                    .map(|v| NbValue::new_heap(HeapValue::from(v)))
                    .collect();
                HeapValue::Set(Arc::new(converted))
            }
            crate::values::Value::Map(map) => {
                let converted: BTreeMap<String, NbValue> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), NbValue::new_heap(HeapValue::from(v.clone()))))
                    .collect();
                HeapValue::Map(Arc::new(converted))
            }
            crate::values::Value::Record(record) => {
                let fields = record
                    .fields
                    .iter()
                    .map(|(k, v)| (k.clone(), NbValue::new_heap(HeapValue::from(v.clone()))))
                    .collect();
                HeapValue::Record(Arc::new(RecordData {
                    type_name: Arc::from(record.type_name.as_str()),
                    fields,
                }))
            }
            crate::values::Value::Union(union) => {
                let payload = match union.payload {
                    crate::values::UnionPayload::Null => NbValue::new_null(),
                    crate::values::UnionPayload::Bool(b) => NbValue::new_bool(b),
                    crate::values::UnionPayload::Int(n) => {
                        if n >= NbValue::MIN_INT48 && n <= NbValue::MAX_INT48 {
                            NbValue::new_int(n)
                        } else {
                            NbValue::new_bigint(BigInt::from(n))
                        }
                    }
                    crate::values::UnionPayload::Float(f) => NbValue::new_float(f),
                    crate::values::UnionPayload::Heap(v) => {
                        NbValue::new_heap(HeapValue::from((*v).clone()))
                    }
                };
                HeapValue::Union(UnionData {
                    tag: Arc::from(union.tag.to_string()),
                    payload,
                })
            }
            crate::values::Value::Closure(c) => HeapValue::Closure(Arc::new(ClosureData {
                cell_idx: c.cell_idx,
                captures: c
                    .captures
                    .into_iter()
                    .map(|v| NbValue::new_heap(HeapValue::from(v)))
                    .collect(),
            })),
            crate::values::Value::Future(f) => HeapValue::Future(Arc::new(FutureData {
                id: f.id,
                status: FutureStatus::Pending,
                schedule: FutureSchedule::Eager,
            })),
            crate::values::Value::TraceRef(t) => HeapValue::TraceRef(t.seq),
            other => {
                // Scalars should never be here, but fall back to string display.
                HeapValue::Str(Arc::from(other.as_string()))
            }
        }
    }
}

impl HeapValue {
    /// Convert an NbValue into a HeapValue (scalar values become strings).
    pub fn from_nbvalue(value: NbValue) -> HeapValue {
        if let Some(hv) = value.as_heap_ref() {
            return hv.clone();
        }
        HeapValue::Str(Arc::from(value.display()))
    }
}

impl HeapValue {
    /// Get string content if this is a Str variant.
    pub fn as_str(&self) -> Option<&str> {
        if let HeapValue::Str(s) = self {
            Some(s)
        } else {
            None
        }
    }

    /// Display as a string (for print/debug builtins).
    pub fn display(&self) -> String {
        match self {
            HeapValue::Str(s) => s.to_string(),
            HeapValue::Bytes(b) => format!("<bytes:{}>", b.len()),
            HeapValue::BigInt(n) => n.to_string(),
            HeapValue::List(l) => {
                let inner: Vec<String> = l.iter().map(|v| v.display()).collect();
                format!("[{}]", inner.join(", "))
            }
            HeapValue::Tuple(t) => {
                let inner: Vec<String> = t.iter().map(|v| v.display()).collect();
                format!("({})", inner.join(", "))
            }
            HeapValue::Set(s) => {
                let inner: Vec<String> = s.iter().map(|v| v.display()).collect();
                format!("{{{}}}", inner.join(", "))
            }
            HeapValue::Map(m) => {
                let inner: Vec<String> = m
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.display()))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
            HeapValue::Record(r) => {
                let fields: Vec<String> = r
                    .fields
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.display()))
                    .collect();
                format!("{}({})", r.type_name, fields.join(", "))
            }
            HeapValue::Union(u) => format!("{}({})", u.tag, u.payload.display()),
            HeapValue::Closure(_) => "<closure>".to_string(),
            HeapValue::Future(f) => format!("<future:{}>", f.id),
            HeapValue::TraceRef(id) => format!("<trace:{}>", id),
        }
    }

    /// Type name string for type_of() builtin.
    pub fn type_name(&self) -> &'static str {
        match self {
            HeapValue::Str(_) => "String",
            HeapValue::Bytes(_) => "Bytes",
            HeapValue::BigInt(_) => "BigInt",
            HeapValue::List(_) => "List",
            HeapValue::Tuple(_) => "Tuple",
            HeapValue::Set(_) => "Set",
            HeapValue::Map(_) => "Map",
            HeapValue::Record(_) => "Record",
            HeapValue::Union(_) => "Union",
            HeapValue::Closure(_) => "Closure",
            HeapValue::Future(_) => "Future",
            HeapValue::TraceRef(_) => "TraceRef",
        }
    }

    /// Is this value truthy?
    pub fn is_truthy(&self) -> bool {
        match self {
            HeapValue::Str(s) => !s.is_empty(),
            HeapValue::List(l) => !l.is_empty(),
            HeapValue::Tuple(t) => !t.is_empty(),
            HeapValue::Set(s) => !s.is_empty(),
            HeapValue::Map(m) => !m.is_empty(),
            _ => true,
        }
    }
}

impl fmt::Display for HeapValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display())
    }
}

// NbValue needs to compare HeapValues for Eq/Ord (used in BTreeSet<NbValue>).
// We implement these on HeapValue so BTreeSet<NbValue> can work.
impl PartialEq for HeapValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (HeapValue::Str(a), HeapValue::Str(b)) => a == b,
            (HeapValue::BigInt(a), HeapValue::BigInt(b)) => a == b,
            (HeapValue::List(a), HeapValue::List(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x == y)
            }
            (HeapValue::Tuple(a), HeapValue::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x == y)
            }
            (HeapValue::Map(a), HeapValue::Map(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|((k1, v1), (k2, v2))| k1 == k2 && v1 == v2)
            }
            (HeapValue::Record(a), HeapValue::Record(b)) => {
                a.type_name == b.type_name
                    && a.fields.len() == b.fields.len()
                    && a.fields
                        .iter()
                        .zip(b.fields.iter())
                        .all(|((k1, v1), (k2, v2))| k1 == k2 && v1 == v2)
            }
            (HeapValue::Union(a), HeapValue::Union(b)) => a.tag == b.tag && a.payload == b.payload,
            _ => false,
        }
    }
}

impl Eq for HeapValue {}

impl PartialOrd for HeapValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::hash::Hash for HeapValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            HeapValue::Str(s) => s.hash(state),
            HeapValue::BigInt(n) => n.hash(state),
            HeapValue::List(l) => {
                for v in l.iter() {
                    v.hash(state);
                }
            }
            _ => {}
        }
    }
}

impl Ord for HeapValue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        // Type ordering by discriminant first
        let da = std::mem::discriminant(self);
        let db = std::mem::discriminant(other);
        if da != db {
            return format!("{:?}", da).cmp(&format!("{:?}", db));
        }
        match (self, other) {
            (HeapValue::Str(a), HeapValue::Str(b)) => a.cmp(b),
            (HeapValue::BigInt(a), HeapValue::BigInt(b)) => a.cmp(b),
            (HeapValue::List(a), HeapValue::List(b)) => a
                .iter()
                .zip(b.iter())
                .find_map(|(x, y)| match x.cmp(y) {
                    Ordering::Equal => None,
                    o => Some(o),
                })
                .unwrap_or_else(|| a.len().cmp(&b.len())),
            _ => Ordering::Equal,
        }
    }
}
