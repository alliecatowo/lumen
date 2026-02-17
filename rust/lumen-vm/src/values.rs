//! Tagged value representation for the Lumen VM.

use crate::strings::StringTable;
use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::rc::Rc;

/// Runtime values in the Lumen VM.
///
/// Collection variants (List, Tuple, Set, Map, Record) are wrapped in `Rc` for
/// cheap cloning via reference counting. Mutation uses `Rc::make_mut()` which
/// provides copy-on-write semantics — the inner data is only cloned when the
/// reference count is greater than one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    BigInt(BigInt),
    Float(f64),
    String(StringRef),
    Bytes(Vec<u8>),
    List(Rc<Vec<Value>>),
    Tuple(Rc<Vec<Value>>),
    Set(Rc<BTreeSet<Value>>),
    Map(Rc<BTreeMap<String, Value>>),
    Record(Rc<RecordValue>),
    Union(UnionValue),
    Closure(ClosureValue),
    TraceRef(TraceRefValue),
    Future(FutureValue),
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
pub struct ClosureValue {
    pub cell_idx: usize,
    pub captures: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceRefValue {
    pub trace_id: String,
    pub seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FutureValue {
    pub id: u64,
    pub state: FutureStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FutureStatus {
    Pending,
    Completed,
    Error,
}

impl Value {
    // -- Constructors (wrap inner data in Rc) --

    pub fn new_list(v: Vec<Value>) -> Self {
        Value::List(Rc::new(v))
    }

    pub fn new_tuple(v: Vec<Value>) -> Self {
        Value::Tuple(Rc::new(v))
    }

    pub fn new_set(s: BTreeSet<Value>) -> Self {
        Value::Set(Rc::new(s))
    }

    pub fn new_set_from_vec(v: Vec<Value>) -> Self {
        Value::Set(Rc::new(v.into_iter().collect()))
    }

    pub fn new_map(m: BTreeMap<String, Value>) -> Self {
        Value::Map(Rc::new(m))
    }

    pub fn new_record(r: RecordValue) -> Self {
        Value::Record(Rc::new(r))
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            Value::BigInt(n) => !n.is_zero(),
            Value::Float(f) => *f != 0.0,
            Value::String(StringRef::Owned(s)) => !s.is_empty(),
            Value::String(StringRef::Interned(_)) => true,
            Value::List(l) => !l.is_empty(),
            Value::Tuple(t) => !t.is_empty(),
            Value::Set(s) => !s.is_empty(),
            Value::Future(_) => true,
            _ => true,
        }
    }

    pub fn as_string(&self) -> String {
        match self {
            Value::Null => "null".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(n) => n.to_string(),
            Value::BigInt(n) => n.to_string(),
            Value::Float(f) => format_float(*f),
            Value::String(StringRef::Owned(s)) => s.clone(),
            Value::String(StringRef::Interned(id)) => format!("<interned:{}>", id),
            Value::Bytes(b) => format!("<bytes:{}>", b.len()),
            Value::List(l) => format!(
                "[{}]",
                l.iter()
                    .map(|v| v.display_pretty())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Value::Tuple(t) => format!(
                "({})",
                t.iter()
                    .map(|v| v.display_pretty())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Value::Set(s) => format!(
                "set[{}]",
                s.iter()
                    .map(|v| v.display_pretty())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Value::Map(m) => format!(
                "{{{}}}",
                m.iter()
                    .map(|(k, v)| format!("{}: {}", k, v.display_pretty()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Value::Record(r) => {
                let fields = r
                    .fields
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.display_pretty()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({})", r.type_name, fields)
            }
            Value::Union(u) => {
                if matches!(*u.payload, Value::Null) {
                    u.tag.clone()
                } else {
                    format!("{}({})", u.tag, u.payload.display_pretty())
                }
            }
            Value::Closure(c) => format!(
                "<closure:cell={},captures={}>",
                c.cell_idx,
                c.captures.len()
            ),
            Value::TraceRef(t) => format!("<trace:{}:{}>", t.trace_id, t.seq),
            Value::Future(f) => format!("<future:{}:{}>", f.id, future_status_name(f.state)),
        }
    }

    /// Convert to string, resolving interned strings using the provided table.
    /// For non-string values, returns the same as `as_string()`.
    pub fn as_string_resolved(&self, strings: &crate::strings::StringTable) -> String {
        match self {
            Value::String(StringRef::Interned(id)) => {
                strings.resolve(*id).unwrap_or("").to_string()
            }
            _ => self.as_string(),
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Int(n) => Some(*n as f64),
            Value::BigInt(n) => n.to_f64(),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&Vec<Value>> {
        match self {
            Value::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_record(&self) -> Option<&RecordValue> {
        match self {
            Value::Record(r) => Some(r),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }

    /// Return a numeric discriminant for type ordering.
    /// Order: Null < Bool < Int < Float < String < Bytes < List < Tuple < Set < Map < Record < Union < Closure < TraceRef
    fn type_order(&self) -> u8 {
        match self {
            Value::Null => 0,
            Value::Bool(_) => 1,
            Value::Int(_) | Value::BigInt(_) | Value::Float(_) => 2,
            Value::String(_) => 5,
            Value::Bytes(_) => 5,
            Value::List(_) => 6,
            Value::Tuple(_) => 7,
            Value::Set(_) => 8,
            Value::Map(_) => 9,
            Value::Record(_) => 10,
            Value::Union(_) => 11,
            Value::Closure(_) => 12,
            Value::TraceRef(_) => 13,
            Value::Future(_) => 14,
        }
    }

    /// Return the variant index for ordering different variants with the same type_order.
    /// This follows the enum declaration order.
    fn variant_index(&self) -> u8 {
        match self {
            Value::Null => 0,
            Value::Bool(_) => 1,
            Value::Int(_) => 2,
            Value::BigInt(_) => 3,
            Value::Float(_) => 4,
            Value::String(_) => 5,
            Value::Bytes(_) => 6,
            Value::List(_) => 7,
            Value::Tuple(_) => 8,
            Value::Set(_) => 9,
            Value::Map(_) => 10,
            Value::Record(_) => 11,
            Value::Union(_) => 12,
            Value::Closure(_) => 13,
            Value::TraceRef(_) => 14,
            Value::Future(_) => 15,
        }
    }

    /// Return the type name as a string (for the `is` operator).
    pub fn type_name(&self) -> &str {
        match self {
            Value::Null => "Null",
            Value::Bool(_) => "Bool",
            Value::Int(_) => "Int",
            Value::BigInt(_) => "Int", // BigInt reports as "Int" to user
            Value::Float(_) => "Float",
            Value::String(_) => "String",
            Value::Bytes(_) => "Bytes",
            Value::List(_) => "List",
            Value::Tuple(_) => "Tuple",
            Value::Set(_) => "Set",
            Value::Map(_) => "Map",
            Value::Record(r) => &r.type_name,
            Value::Union(u) => &u.tag,
            Value::Closure(_) => "Closure",
            Value::TraceRef(_) => "TraceRef",
            Value::Future(_) => "Future",
        }
    }

    /// Pretty display for user-facing output
    pub fn display_pretty(&self) -> String {
        match self {
            Value::String(StringRef::Owned(s)) => s.clone(),
            Value::String(StringRef::Interned(_)) => self.as_string(),
            Value::Null => "null".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(n) => n.to_string(),
            Value::BigInt(n) => n.to_string(),
            Value::Float(f) => format_float(*f),
            Value::List(l) => {
                let items: Vec<String> = l.iter().map(|v| v.display_quoted()).collect();
                format!("[{}]", items.join(", "))
            }
            Value::Tuple(t) => {
                let items: Vec<String> = t.iter().map(|v| v.display_quoted()).collect();
                format!("({})", items.join(", "))
            }
            Value::Set(s) => {
                let items: Vec<String> = s.iter().map(|v| v.display_quoted()).collect();
                format!("set[{}]", items.join(", "))
            }
            Value::Map(m) => {
                let entries: Vec<String> = m
                    .iter()
                    .map(|(k, v)| format!("\"{}\": {}", k, v.display_quoted()))
                    .collect();
                format!("{{{}}}", entries.join(", "))
            }
            Value::Record(r) => {
                if r.fields.is_empty() {
                    format!("{}()", r.type_name)
                } else {
                    let fields: Vec<String> = r
                        .fields
                        .iter()
                        .map(|(k, v)| format!("{}: {}", k, v.display_quoted()))
                        .collect();
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
            Value::Closure(c) => format!("<closure:cell={}>", c.cell_idx),
            Value::Future(f) => format!("<future:{}:{}>", f.id, future_status_name(f.state)),
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
            (Value::BigInt(a), Value::BigInt(b)) => a == b,
            (Value::Int(a), Value::BigInt(b)) => BigInt::from(*a) == *b,
            (Value::BigInt(a), Value::Int(b)) => *a == BigInt::from(*b),
            // Compare floats by bit pattern so Eq stays reflexive for NaN and
            // preserves sign/payload distinctions (e.g. -0.0 vs +0.0, NaN payloads).
            (Value::Float(a), Value::Float(b)) => a.to_bits() == b.to_bits(),
            (Value::BigInt(a), Value::Float(b)) => a
                .to_f64()
                .map(|f| f.to_bits() == b.to_bits())
                .unwrap_or(false),
            (Value::Float(a), Value::BigInt(b)) => b
                .to_f64()
                .map(|f| f.to_bits() == a.to_bits())
                .unwrap_or(false),
            (Value::String(StringRef::Owned(a)), Value::String(StringRef::Owned(b))) => a == b,
            // At Value-layer (without StringTable), interned equality is by id only.
            (Value::String(StringRef::Interned(a)), Value::String(StringRef::Interned(b))) => {
                a == b
            }
            // Cross representation string equality requires StringTable resolution (handled in VM).
            (Value::String(StringRef::Owned(_)), Value::String(StringRef::Interned(_))) => false,
            (Value::String(StringRef::Interned(_)), Value::String(StringRef::Owned(_))) => false,
            (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
            (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
            (Value::List(a), Value::List(b)) => **a == **b,
            (Value::Tuple(a), Value::Tuple(b)) => **a == **b,
            (Value::Set(a), Value::Set(b)) => **a == **b,
            (Value::Map(a), Value::Map(b)) => **a == **b,
            (Value::Record(a), Value::Record(b)) => {
                a.type_name == b.type_name && a.fields == b.fields
            }
            (Value::Union(a), Value::Union(b)) => a.tag == b.tag && a.payload == b.payload,
            (Value::Closure(a), Value::Closure(b)) => {
                a.cell_idx == b.cell_idx && a.captures == b.captures
            }
            (Value::TraceRef(a), Value::TraceRef(b)) => a.trace_id == b.trace_id && a.seq == b.seq,
            (Value::Future(a), Value::Future(b)) => a.id == b.id && a.state == b.state,
            _ => false,
        }
    }
}

impl Eq for Value {}

/// Compare two values for equality, resolving interned strings via the provided
/// `StringTable`. This enables correct cross-representation string equality
/// (interned vs owned) at all nesting depths (lists, maps, records, etc.).
pub fn values_equal(a: &Value, b: &Value, strings: &StringTable) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::BigInt(x), Value::BigInt(y)) => x == y,
        (Value::Int(x), Value::BigInt(y)) => BigInt::from(*x) == *y,
        (Value::BigInt(x), Value::Int(y)) => *x == BigInt::from(*y),
        (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits(),
        (Value::BigInt(x), Value::Float(y)) => x
            .to_f64()
            .map(|f| f.to_bits() == y.to_bits())
            .unwrap_or(false),
        (Value::Float(x), Value::BigInt(y)) => y
            .to_f64()
            .map(|f| f.to_bits() == x.to_bits())
            .unwrap_or(false),
        (Value::String(sa), Value::String(sb)) => {
            let left = match sa {
                StringRef::Owned(s) => s.as_str(),
                StringRef::Interned(id) => strings.resolve(*id).unwrap_or(""),
            };
            let right = match sb {
                StringRef::Owned(s) => s.as_str(),
                StringRef::Interned(id) => strings.resolve(*id).unwrap_or(""),
            };
            left == right
        }
        (Value::Int(x), Value::Float(y)) => (*x as f64) == *y,
        (Value::Float(x), Value::Int(y)) => *x == (*y as f64),
        (Value::List(x), Value::List(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|(a, b)| values_equal(a, b, strings))
        }
        (Value::Tuple(x), Value::Tuple(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|(a, b)| values_equal(a, b, strings))
        }
        (Value::Set(x), Value::Set(y)) => {
            // For sets, element-wise comparison with string resolution.
            // Since sets are BTreeSet<Value> ordered by Value::Ord (which doesn't
            // resolve strings), we fall back to pairwise comparison.
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|(a, b)| values_equal(a, b, strings))
        }
        (Value::Map(x), Value::Map(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, va)| y.get(k).is_some_and(|vb| values_equal(va, vb, strings)))
        }
        (Value::Record(x), Value::Record(y)) => {
            x.type_name == y.type_name
                && x.fields.len() == y.fields.len()
                && x.fields.iter().all(|(k, va)| {
                    y.fields
                        .get(k)
                        .is_some_and(|vb| values_equal(va, vb, strings))
                })
        }
        (Value::Union(x), Value::Union(y)) => {
            x.tag == y.tag && values_equal(&x.payload, &y.payload, strings)
        }
        (Value::Closure(x), Value::Closure(y)) => {
            x.cell_idx == y.cell_idx && x.captures == y.captures
        }
        (Value::TraceRef(x), Value::TraceRef(y)) => x.trace_id == y.trace_id && x.seq == y.seq,
        (Value::Future(x), Value::Future(y)) => x.id == y.id && x.state == y.state,
        _ => false,
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Value {
    fn cmp(&self, other: &Self) -> Ordering {
        let type_a = self.type_order();
        let type_b = other.type_order();
        if type_a != type_b {
            return type_a.cmp(&type_b);
        }
        // Same type category
        match (self, other) {
            (Value::Null, Value::Null) => Ordering::Equal,
            (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
            (Value::Int(a), Value::Int(b)) => a.cmp(b),
            (Value::BigInt(a), Value::BigInt(b)) => a.cmp(b),
            (Value::Float(a), Value::Float(b)) => a.total_cmp(b),
            (Value::String(a), Value::String(b)) => match (a, b) {
                (StringRef::Owned(sa), StringRef::Owned(sb)) => sa.cmp(sb),
                (StringRef::Interned(ida), StringRef::Interned(idb)) => ida.cmp(idb),
                // Keep interned-vs-owned ordering deterministic without StringTable access.
                (StringRef::Interned(_), StringRef::Owned(_)) => Ordering::Less,
                (StringRef::Owned(_), StringRef::Interned(_)) => Ordering::Greater,
            },
            (Value::Bytes(a), Value::Bytes(b)) => a.cmp(b),
            (Value::List(a), Value::List(b)) => (**a).cmp(&**b),
            (Value::Tuple(a), Value::Tuple(b)) => (**a).cmp(&**b),
            (Value::Set(a), Value::Set(b)) => a.len().cmp(&b.len()).then_with(|| (**a).cmp(&**b)),
            (Value::Map(a), Value::Map(b)) => {
                let ak: Vec<_> = a.keys().collect();
                let bk: Vec<_> = b.keys().collect();
                ak.cmp(&bk).then_with(|| {
                    for key in ak {
                        if let (Some(va), Some(vb)) = (a.get(key), b.get(key)) {
                            let c = va.cmp(vb);
                            if c != Ordering::Equal {
                                return c;
                            }
                        }
                    }
                    Ordering::Equal
                })
            }
            (Value::Record(a), Value::Record(b)) => a.type_name.cmp(&b.type_name).then_with(|| {
                let ak: Vec<_> = a.fields.keys().collect();
                let bk: Vec<_> = b.fields.keys().collect();
                ak.cmp(&bk).then_with(|| {
                    for key in ak {
                        if let (Some(va), Some(vb)) = (a.fields.get(key), b.fields.get(key)) {
                            let c = va.cmp(vb);
                            if c != Ordering::Equal {
                                return c;
                            }
                        }
                    }
                    Ordering::Equal
                })
            }),
            (Value::Union(a), Value::Union(b)) => {
                a.tag.cmp(&b.tag).then_with(|| a.payload.cmp(&b.payload))
            }
            (Value::Closure(a), Value::Closure(b)) => a
                .cell_idx
                .cmp(&b.cell_idx)
                .then_with(|| a.captures.cmp(&b.captures)),
            (Value::TraceRef(a), Value::TraceRef(b)) => {
                a.trace_id.cmp(&b.trace_id).then_with(|| a.seq.cmp(&b.seq))
            }
            (Value::Future(a), Value::Future(b)) => {
                a.id.cmp(&b.id)
                    .then_with(|| future_status_ord(a.state).cmp(&future_status_ord(b.state)))
            }
            _ => {
                // Same type_order but different variants - order by variant index
                // This ensures deterministic ordering for types like Int vs Float
                // (both have type_order 2, but Int < Float by enum declaration order)
                self.variant_index().cmp(&other.variant_index())
            }
        }
    }
}

fn future_status_name(state: FutureStatus) -> &'static str {
    match state {
        FutureStatus::Pending => "pending",
        FutureStatus::Completed => "completed",
        FutureStatus::Error => "error",
    }
}

fn future_status_ord(state: FutureStatus) -> u8 {
    match state {
        FutureStatus::Pending => 0,
        FutureStatus::Completed => 1,
        FutureStatus::Error => 2,
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
        let v = Value::new_list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(v.display_pretty(), "[1, 2, 3]");
    }

    #[test]
    fn test_display_pretty_tuple() {
        let v = Value::new_tuple(vec![
            Value::Int(1),
            Value::String(StringRef::Owned("a".into())),
        ]);
        assert_eq!(v.display_pretty(), "(1, \"a\")");
    }

    #[test]
    fn test_display_pretty_set() {
        let v = Value::new_set_from_vec(vec![Value::Int(1), Value::Int(2)]);
        assert_eq!(v.display_pretty(), "set[1, 2]");
    }

    #[test]
    fn test_display_pretty_record() {
        let mut fields = BTreeMap::new();
        fields.insert(
            "name".to_string(),
            Value::String(StringRef::Owned("Alice".into())),
        );
        fields.insert("age".to_string(), Value::Int(30));
        let v = Value::new_record(RecordValue {
            type_name: "Person".into(),
            fields,
        });
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
        assert_eq!(Value::Float(2.5).as_float(), Some(2.5));
        assert_eq!(Value::Int(42).as_float(), Some(42.0));
        assert!(Value::new_list(vec![]).as_list().is_some());
        assert!(Value::Null.as_list().is_none());
    }

    #[test]
    fn test_value_ordering() {
        assert!(Value::Null < Value::Bool(false));
        assert!(Value::Bool(false) < Value::Int(0));
        assert!(Value::Int(0) < Value::Float(0.0));
        assert!(Value::Int(1) < Value::Int(2));
        assert!(Value::Float(1.0) < Value::Float(2.0));
    }

    #[test]
    fn test_set_equality() {
        let a = Value::new_set_from_vec(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let b = Value::new_set_from_vec(vec![Value::Int(3), Value::Int(1), Value::Int(2)]);
        assert_eq!(a, b);
    }

    #[test]
    fn test_closure_display() {
        let c = Value::Closure(ClosureValue {
            cell_idx: 0,
            captures: vec![Value::Int(42)],
        });
        assert_eq!(c.display_pretty(), "<closure:cell=0>");
    }

    #[test]
    fn test_nan_equality() {
        let nan1 = Value::Float(f64::NAN);
        let nan2 = Value::Float(f64::NAN);
        // Bitwise equality makes NaN reflexive at the Value layer.
        assert_eq!(nan1, nan2);
        assert_eq!(nan1.cmp(&nan2), Ordering::Equal);
    }

    #[test]
    fn test_nan_ordering_is_total_and_stable() {
        // Construct distinct quiet-NaNs explicitly by payload bit pattern.
        let nan_a = Value::Float(f64::from_bits(0x7ff8_0000_0000_0001));
        let nan_b = Value::Float(f64::from_bits(0x7ff8_0000_0000_0002));
        assert_ne!(nan_a, nan_b);
        assert_eq!(nan_a.cmp(&nan_b), Ordering::Less);
        assert_eq!(nan_b.cmp(&nan_a), Ordering::Greater);
    }

    #[test]
    fn test_float_equality_normal() {
        assert_eq!(Value::Float(1.5), Value::Float(1.5));
        assert_ne!(Value::Float(1.5), Value::Float(2.5));
    }

    #[test]
    fn test_is_truthy_interned_always_true_without_table() {
        // Without string table resolution, interned strings are truthy
        // (the VM's value_is_truthy method handles resolution)
        assert!(Value::String(StringRef::Interned(0)).is_truthy());
        assert!(Value::String(StringRef::Interned(99)).is_truthy());
    }

    #[test]
    fn test_is_truthy_comprehensive() {
        // Null
        assert!(!Value::Null.is_truthy());
        // Bool
        assert!(!Value::Bool(false).is_truthy());
        assert!(Value::Bool(true).is_truthy());
        // Int
        assert!(!Value::Int(0).is_truthy());
        assert!(Value::Int(1).is_truthy());
        assert!(Value::Int(-1).is_truthy());
        // Float
        assert!(!Value::Float(0.0).is_truthy());
        assert!(Value::Float(1.0).is_truthy());
        assert!(Value::Float(-0.5).is_truthy());
        // String
        assert!(!Value::String(StringRef::Owned("".into())).is_truthy());
        assert!(Value::String(StringRef::Owned("hello".into())).is_truthy());
        // List
        assert!(!Value::new_list(vec![]).is_truthy());
        assert!(Value::new_list(vec![Value::Null]).is_truthy());
        // Tuple
        assert!(!Value::new_tuple(vec![]).is_truthy());
        assert!(Value::new_tuple(vec![Value::Int(1)]).is_truthy());
        // Set
        assert!(!Value::new_set_from_vec(vec![]).is_truthy());
        assert!(Value::new_set_from_vec(vec![Value::Int(1)]).is_truthy());
    }

    #[test]
    fn test_interned_string_ordering() {
        // Interned strings should compare by ID
        let a = Value::String(StringRef::Interned(1));
        let b = Value::String(StringRef::Interned(2));
        assert!(a < b);

        // Same interned ID should be equal for both Eq and Ord.
        let c = Value::String(StringRef::Interned(1));
        assert_eq!(a, c);
        assert_eq!(a.cmp(&c), Ordering::Equal);

        // Interned and owned are distinct representations at Value layer.
        let owned = Value::String(StringRef::Owned("test".into()));
        let interned = Value::String(StringRef::Interned(0));
        assert_ne!(interned, owned);
        assert!(interned < owned);

        // Interned-vs-owned ordering is stable regardless of owned string contents.
        let owned_aaa = Value::String(StringRef::Owned("aaa".into()));
        let owned_zzz = Value::String(StringRef::Owned("zzz".into()));
        assert!(interned < owned_aaa);
        assert!(interned < owned_zzz);
    }

    #[test]
    fn test_values_equal_cross_representation_strings() {
        let mut table = StringTable::new();
        let id = table.intern("hello");

        let interned = Value::String(StringRef::Interned(id));
        let owned = Value::String(StringRef::Owned("hello".into()));

        // Value::PartialEq returns false for cross-representation (no table access)
        assert_ne!(interned, owned);

        // values_equal resolves via StringTable and compares correctly
        assert!(values_equal(&interned, &owned, &table));
        assert!(values_equal(&owned, &interned, &table));
    }

    #[test]
    fn test_values_equal_cross_representation_different_content() {
        let mut table = StringTable::new();
        let id = table.intern("hello");

        let interned = Value::String(StringRef::Interned(id));
        let owned = Value::String(StringRef::Owned("world".into()));

        assert!(!values_equal(&interned, &owned, &table));
    }

    #[test]
    fn test_values_equal_nested_lists_with_mixed_strings() {
        let mut table = StringTable::new();
        let id = table.intern("item");

        let list_a = Value::new_list(vec![Value::Int(1), Value::String(StringRef::Interned(id))]);
        let list_b = Value::new_list(vec![
            Value::Int(1),
            Value::String(StringRef::Owned("item".into())),
        ]);

        // Value::PartialEq would fail for nested cross-representation strings
        assert_ne!(list_a, list_b);

        // values_equal handles nested comparisons correctly
        assert!(values_equal(&list_a, &list_b, &table));
    }

    #[test]
    fn test_values_equal_nested_maps_with_mixed_strings() {
        let mut table = StringTable::new();
        let id = table.intern("val");

        let mut map_a = BTreeMap::new();
        map_a.insert("key".to_string(), Value::String(StringRef::Interned(id)));
        let mut map_b = BTreeMap::new();
        map_b.insert(
            "key".to_string(),
            Value::String(StringRef::Owned("val".into())),
        );

        let va = Value::new_map(map_a);
        let vb = Value::new_map(map_b);

        assert!(values_equal(&va, &vb, &table));
    }

    #[test]
    fn test_values_equal_same_representation() {
        let table = StringTable::new();

        // Same representation should still work
        assert!(values_equal(&Value::Int(42), &Value::Int(42), &table));
        assert!(!values_equal(&Value::Int(42), &Value::Int(43), &table));
        assert!(values_equal(
            &Value::String(StringRef::Owned("hi".into())),
            &Value::String(StringRef::Owned("hi".into())),
            &table
        ));
    }
}
