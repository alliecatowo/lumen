//! Builtin function dispatch, intrinsic opcodes, and closure calls for the VM.

use super::*;
use crate::json_parser::parse_json_optimized;
use lumen_core::heap_value::HeapValue;
use lumen_core::values::UnionPayload;
use num_bigint::BigInt;
use num_traits::{Signed, ToPrimitive};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

impl VM {
    /// Display formatting that matches Value::display_pretty without forcing
    /// a full NbValue -> Value bridge on common scalar paths.
    #[inline]
    fn nb_display_pretty(&self, nb: NbValue) -> String {
        nb.display()
    }

    /// String conversion that mirrors Value::as_string_resolved semantics:
    /// interned strings resolve through the table, and floats keep one
    /// decimal when they are integral.
    #[inline]
    fn nb_to_string_as_resolved_value(&self, nb: NbValue) -> String {
        if let Some(hv) = nb.as_heap_ref() {
            return hv.display();
        }
        if nb.is_int() {
            return nb.as_int().unwrap_or(0).to_string();
        }
        if nb.is_float() {
            let f = f64::from_bits(nb.0);
            if f == f.floor() && f.abs() < 1e15 {
                return format!("{:.1}", f);
            }
            return format!("{}", f);
        }
        if nb.is_bool() {
            return nb.as_bool().unwrap_or(false).to_string();
        }
        if nb.is_null() {
            return "null".to_string();
        }
        "null".to_string()
    }

    /// Extract a string from an NbValue using TAG_PTR borrow-through when possible.
    /// Avoids deep-cloning the Value just to get at the string inside.
    #[inline]
    fn nb_to_string_resolved(&self, nb: NbValue) -> String {
        if let Some(hv) = nb.as_heap_ref() {
            return hv.display();
        }
        if nb.is_int() {
            return nb.as_int().unwrap_or(0).to_string();
        }
        if nb.is_float() {
            return format!("{}", f64::from_bits(nb.0));
        }
        if nb.is_bool() {
            return nb.as_bool().unwrap_or(false).to_string();
        }
        if nb.is_null() {
            return "null".to_string();
        }
        "null".to_string()
    }

    /// Extract an int from an NbValue, using TAG_PTR borrow-through as fallback.
    #[inline]
    fn nb_to_int(&self, nb: NbValue) -> Option<i64> {
        if nb.is_int() {
            return nb.as_int();
        }
        if nb.is_float() || nb.is_bool() || nb.is_null() {
            return None;
        }
        if let Some(HeapValue::BigInt(n)) = nb.as_heap_ref() {
            return n.to_i64();
        }
        None
    }

    /// Convert an NbValue into a legacy Value by walking HeapValue recursively.
    /// This is a compatibility shim for builtins that still return Value.
    fn nb_to_value_deep(&mut self, nb: NbValue) -> Value {
        if let Some(i) = nb.as_int() {
            return Value::Int(i);
        }
        if let Some(f) = nb.as_float() {
            return Value::Float(f);
        }
        if let Some(b) = nb.as_bool() {
            return Value::Bool(b);
        }
        if nb.is_null() {
            return Value::Null;
        }
        if let Some(hv) = nb.as_heap_ref() {
            return match hv {
                HeapValue::Str(s) => Value::String(StringRef::Owned(s.to_string())),
                HeapValue::Bytes(b) => Value::Bytes(b.as_ref().to_vec()),
                HeapValue::BigInt(n) => Value::BigInt((**n).clone()),
                HeapValue::List(l) => Value::List(Arc::clone(l)),
                HeapValue::Tuple(t) => Value::Tuple(Arc::clone(t)),
                HeapValue::Set(s) => {
                    let converted: BTreeSet<Value> =
                        s.iter().map(|v| self.nb_to_value_deep(*v)).collect();
                    Value::Set(Arc::new(converted))
                }
                HeapValue::Map(m) => {
                    let converted: BTreeMap<String, Value> = m
                        .iter()
                        .map(|(k, v)| (k.clone(), self.nb_to_value_deep(*v)))
                        .collect();
                    Value::Map(Arc::new(converted))
                }
                HeapValue::Record(r) => {
                    let fields = r
                        .fields
                        .iter()
                        .map(|(k, v)| (k.clone(), self.nb_to_value_deep(*v)))
                        .collect();
                    Value::Record(Arc::new(RecordValue {
                        type_name: r.type_name.to_string(),
                        fields,
                    }))
                }
                HeapValue::Union(u) => {
                    let tag_id = self.strings.intern(&u.tag);
                    Value::Union(UnionValue {
                        tag: tag_id,
                        payload: UnionPayload::from_value(self.nb_to_value_deep(u.payload)),
                    })
                }
                HeapValue::Closure(c) => Value::Closure(ClosureValue {
                    cell_idx: c.cell_idx,
                    captures: c
                        .captures
                        .iter()
                        .map(|v| self.nb_to_value_deep(*v))
                        .collect(),
                }),
                HeapValue::Future(f) => Value::Future(FutureValue {
                    id: f.id,
                    state: match &f.status {
                        lumen_core::heap_value::FutureStatus::Pending => FutureStatus::Pending,
                        lumen_core::heap_value::FutureStatus::Completed(_) => {
                            FutureStatus::Completed
                        }
                        lumen_core::heap_value::FutureStatus::Error(_) => FutureStatus::Error,
                    },
                }),
                HeapValue::TraceRef(id) => Value::TraceRef(TraceRefValue {
                    trace_id: self.resolve_trace_id(),
                    seq: *id,
                }),
            };
        }
        Value::Null
    }

    /// Convert a legacy Value into an NbValue without using removed bridges.
    fn nb_from_value(&mut self, value: Value) -> NbValue {
        match value {
            Value::Null => NbValue::new_null(),
            Value::Bool(b) => NbValue::new_bool(b),
            Value::Int(n) => {
                if (NbValue::MIN_INT48..=NbValue::MAX_INT48).contains(&n) {
                    NbValue::new_int(n)
                } else {
                    NbValue::new_bigint(BigInt::from(n))
                }
            }
            Value::BigInt(n) => NbValue::new_bigint(n),
            Value::Float(f) => NbValue::new_float(f),
            Value::String(sr) => {
                let s = match sr {
                    StringRef::Owned(s) => s,
                    StringRef::Interned(id) => self.strings.resolve(id).unwrap_or("").to_string(),
                };
                NbValue::new_str(&s)
            }
            Value::Bytes(b) => NbValue::new_bytes(Arc::from(b.into_boxed_slice())),
            Value::List(l) => NbValue::new_heap(HeapValue::List(l)),
            Value::Tuple(t) => NbValue::new_heap(HeapValue::Tuple(t)),
            Value::Set(s) => {
                let converted: BTreeSet<NbValue> =
                    s.iter().cloned().map(|v| self.nb_from_value(v)).collect();
                NbValue::new_set(converted)
            }
            Value::Map(m) => {
                let converted: BTreeMap<String, NbValue> = m
                    .iter()
                    .map(|(k, v)| (k.clone(), self.nb_from_value(v.clone())))
                    .collect();
                NbValue::new_map(converted)
            }
            Value::Record(r) => {
                let fields = r
                    .fields
                    .iter()
                    .map(|(k, v)| (k.clone(), self.nb_from_value(v.clone())))
                    .collect();
                NbValue::new_record(&r.type_name, fields)
            }
            Value::Union(u) => {
                let tag = self.strings.resolve(u.tag).unwrap_or("").to_string();
                let payload = match &u.payload {
                    UnionPayload::Null => NbValue::new_null(),
                    UnionPayload::Bool(b) => NbValue::new_bool(*b),
                    UnionPayload::Int(n) => {
                        if (NbValue::MIN_INT48..=NbValue::MAX_INT48).contains(n) {
                            NbValue::new_int(*n)
                        } else {
                            NbValue::new_bigint(BigInt::from(*n))
                        }
                    }
                    UnionPayload::Float(f) => NbValue::new_float(*f),
                    UnionPayload::Heap(v) => self.nb_from_value((**v).clone()),
                };
                NbValue::new_union(&tag, payload)
            }
            Value::Closure(c) => NbValue::new_closure(
                c.cell_idx,
                c.captures
                    .into_iter()
                    .map(|v| self.nb_from_value(v))
                    .collect(),
            ),
            Value::Future(f) => NbValue::new_heap(HeapValue::Future(Arc::new(
                lumen_core::heap_value::FutureData {
                    id: f.id,
                    status: match f.state {
                        FutureStatus::Pending => lumen_core::heap_value::FutureStatus::Pending,
                        FutureStatus::Completed => {
                            lumen_core::heap_value::FutureStatus::Completed(NbValue::new_null())
                        }
                        FutureStatus::Error => {
                            lumen_core::heap_value::FutureStatus::Error("error".to_string())
                        }
                    },
                    schedule: lumen_core::heap_value::FutureSchedule::Eager,
                },
            ))),
            Value::TraceRef(t) => NbValue::new_heap(HeapValue::TraceRef(t.seq)),
        }
    }

    /// Execute a built-in function by name.
    pub(crate) fn call_builtin(
        &mut self,
        name: &str,
        base: usize,
        a: usize,
        nargs: usize,
    ) -> Result<Value, VmError> {
        if let Some(result) = self.try_call_process_builtin(name, base, a, nargs) {
            return result.map(|nb| self.nb_to_value_deep(nb));
        }
        match name {
            "print" => {
                let mut parts = Vec::new();
                for i in 0..nargs {
                    let nb = self.registers[base + a + 1 + i];
                    let s = self.nb_display_pretty(nb);
                    parts.push(s);
                }
                let output = parts.join(" ");
                println!("{}", output);
                self.output.push(output);
                Ok(Value::Null)
            }
            "len" | "length" => {
                // NbValue fast-path: scalars (Int, Float, Bool, Null) have no meaningful
                // length — return 0 without touching the heap at all.
                let nb = self.registers[base + a + 1];
                if nb.is_int() || nb.is_float() || nb.is_bool() || nb.is_null() {
                    return Ok(Value::Int(0));
                }
                // HeapValue borrow-through: read len without cloning the collection
                if let Some(hv) = nb.as_heap_ref() {
                    let len = match hv {
                        HeapValue::Str(s) => s.len() as i64,
                        HeapValue::List(l) => l.len() as i64,
                        HeapValue::Map(m) => m.len() as i64,
                        HeapValue::Tuple(t) => t.len() as i64,
                        HeapValue::Set(s) => s.len() as i64,
                        HeapValue::Bytes(b) => b.len() as i64,
                        _ => 0,
                    };
                    return Ok(Value::Int(len));
                }
                Ok(Value::Int(0))
            }
            "append" => {
                let list = self.reg_take(base + a + 1);
                let elem = self.reg_take(base + a + 2);
                if let Value::List(mut l) = list {
                    Arc::make_mut(&mut l).push(self.nb_from_value(elem));
                    Ok(Value::List(l))
                } else {
                    Ok(Value::List(Arc::new(vec![self.nb_from_value(elem)])))
                }
            }
            "to_string" | "str" | "string" => {
                let nb = self.registers[base + a + 1];
                Ok(Value::String(StringRef::Owned(self.nb_display_pretty(nb))))
            }
            "to_int" | "int" => {
                let nb = self.registers[base + a + 1];
                // NbValue fast-paths: int identity, float truncate, bool 0/1.
                if nb.is_int() {
                    return Ok(Value::Int(nb.as_int().unwrap_or(0)));
                }
                if nb.is_float() {
                    return Ok(Value::Int(f64::from_bits(nb.0) as i64));
                }
                if nb.is_bool() {
                    return Ok(Value::Int(if nb.as_bool().unwrap_or(false) {
                        1
                    } else {
                        0
                    }));
                }
                if nb.is_null() {
                    return Ok(Value::Null);
                }
                if let Some(hv) = nb.as_heap_ref() {
                    return Ok(match hv {
                        HeapValue::BigInt(n) => Value::BigInt((**n).clone()),
                        HeapValue::Str(s) => {
                            if let Ok(i) = s.parse::<i64>() {
                                Value::Int(i)
                            } else if let Ok(bi) = s.parse::<BigInt>() {
                                Value::BigInt(bi)
                            } else {
                                Value::Null
                            }
                        }
                        _ => Value::Null,
                    });
                }
                let arg = self.nb_to_value_deep(nb);
                if let Value::List(l) = arg {
                    let mut result = Vec::new();
                    for item in l.iter() {
                        if let Some(HeapValue::List(inner)) = item.as_heap_ref() {
                            result.extend(inner.iter().cloned());
                        } else {
                            result.push(*item);
                        }
                    }
                    Ok(Value::List(Arc::new(result)))
                } else {
                    Ok(arg)
                }
            }
            "to_float" | "float" => {
                let nb = self.registers[base + a + 1];
                // NbValue fast-paths: float identity, int promotion.
                if nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(nb.0)));
                }
                if nb.is_int() {
                    return Ok(Value::Float(nb.as_int().unwrap_or(0) as f64));
                }
                if nb.is_null() {
                    return Ok(Value::Null);
                }
                if let Some(hv) = nb.as_heap_ref() {
                    return Ok(match hv {
                        HeapValue::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::NAN)),
                        HeapValue::Str(s) => {
                            s.parse::<f64>().map(Value::Float).unwrap_or(Value::Null)
                        }
                        _ => Value::Null,
                    });
                }
                let arg = self.nb_to_value_deep(nb);
                if let Value::List(l) = arg {
                    let mut result = Vec::new();
                    for item in l.iter() {
                        if !result.contains(item) {
                            result.push(*item);
                        }
                    }
                    Ok(Value::List(Arc::new(result)))
                } else {
                    Ok(arg)
                }
            }
            "type_of" | "type" => {
                let nb = self.registers[base + a + 1];
                let name = if nb.is_int() {
                    "Int"
                } else if nb.is_float() {
                    "Float"
                } else if nb.is_bool() {
                    "Bool"
                } else if nb.is_null() {
                    "Null"
                } else if let Some(hv) = nb.as_heap_ref() {
                    hv.type_name()
                } else {
                    return Ok(Value::String(StringRef::Owned(nb.type_name().to_string())));
                };
                Ok(Value::String(StringRef::Owned(name.to_string())))
            }
            "keys" => {
                let nb = self.registers[base + a + 1];
                if let Some(hv) = nb.as_heap_ref() {
                    return Ok(match hv {
                        HeapValue::Map(m) => {
                            Value::List(Arc::new(m.keys().map(|k| NbValue::new_str(k)).collect()))
                        }
                        HeapValue::Record(r) => Value::List(Arc::new(
                            r.fields.keys().map(|k| NbValue::new_str(k)).collect(),
                        )),
                        _ => Value::List(Arc::new(Vec::new())),
                    });
                }
                Ok(Value::List(Arc::new(Vec::new())))
            }
            "values" => {
                let nb = self.registers[base + a + 1];
                if let Some(hv) = nb.as_heap_ref() {
                    return Ok(match hv {
                        HeapValue::Map(m) => Value::List(Arc::new(m.values().cloned().collect())),
                        HeapValue::Record(r) => {
                            Value::List(Arc::new(r.fields.values().cloned().collect()))
                        }
                        _ => Value::List(Arc::new(Vec::new())),
                    });
                }
                Ok(Value::List(Arc::new(Vec::new())))
            }
            "contains" | "has" => {
                // TAG_PTR borrow-through fast-path: avoid cloning the collection
                let coll_nb = self.registers[base + a + 1];
                let needle_nb = self.registers[base + a + 2];
                if let Some(coll_ref) = coll_nb.as_heap_ref() {
                    // Fast-path: int needle in list (primes sieve pattern)
                    if needle_nb.is_int() {
                        let needle_val = needle_nb.as_int().unwrap_or(0);
                        let result = match coll_ref {
                            HeapValue::List(l) => l.iter().any(|v| v.as_int() == Some(needle_val)),
                            HeapValue::Set(s) => s.iter().any(|v| v.as_int() == Some(needle_val)),
                            _ => false,
                        };
                        return Ok(Value::Bool(result));
                    }
                }
                let result = if let Some(coll_ref) = coll_nb.as_heap_ref() {
                    match coll_ref {
                        HeapValue::List(l) => l.iter().any(|v| *v == needle_nb),
                        HeapValue::Set(s) => s.contains(&needle_nb),
                        HeapValue::Map(m) => {
                            let needle_str = self.nb_to_string_resolved(needle_nb);
                            m.contains_key(&needle_str)
                        }
                        HeapValue::Str(s) => {
                            let needle_str = self.nb_to_string_resolved(needle_nb);
                            s.contains(&needle_str)
                        }
                        _ => false,
                    }
                } else {
                    false
                };
                Ok(Value::Bool(result))
            }
            "join" => {
                let list_nb = self.registers[base + a + 1];
                let sep = if nargs > 1 {
                    self.nb_to_string_resolved(self.registers[base + a + 2])
                } else {
                    ", ".to_string()
                };
                // HeapValue borrow-through for the list
                if let Some(hv) = list_nb.as_heap_ref() {
                    if let HeapValue::List(l) = hv {
                        let joined = l.iter().map(|v| v.display()).collect::<Vec<_>>().join(&sep);
                        return Ok(Value::String(StringRef::Owned(joined)));
                    }
                    return Ok(Value::String(StringRef::Owned(hv.display())));
                }
                Ok(Value::String(StringRef::Owned(
                    self.nb_display_pretty(list_nb),
                )))
            }
            "split" => {
                let s = self.nb_to_string_resolved(self.registers[base + a + 1]);
                let sep = if nargs > 1 {
                    self.nb_to_string_resolved(self.registers[base + a + 2])
                } else {
                    " ".to_string()
                };
                let parts: Vec<NbValue> = s.split(&sep).map(NbValue::new_str).collect();
                Ok(Value::List(Arc::new(parts)))
            }
            "trim" => {
                let s = self.nb_to_string_resolved(self.registers[base + a + 1]);
                Ok(Value::String(StringRef::Owned(s.trim().to_string())))
            }
            "upper" => {
                let s = self.nb_to_string_resolved(self.registers[base + a + 1]);
                Ok(Value::String(StringRef::Owned(s.to_uppercase())))
            }
            "lower" => {
                let s = self.nb_to_string_resolved(self.registers[base + a + 1]);
                Ok(Value::String(StringRef::Owned(s.to_lowercase())))
            }
            "replace" => {
                let s = self.nb_to_string_resolved(self.registers[base + a + 1]);
                let from = self.nb_to_string_resolved(self.registers[base + a + 2]);
                let to = self.nb_to_string_resolved(self.registers[base + a + 3]);
                Ok(Value::String(StringRef::Owned(s.replace(&from, &to))))
            }
            "abs" => {
                let nb = self.registers[base + a + 1];
                // NbValue fast-paths: most common cases need no heap access.
                if nb.is_int() {
                    return Ok(Value::Int(nb.as_int().unwrap_or(0).abs()));
                }
                if nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(nb.0).abs()));
                }
                Ok(self.nb_to_value_deep(nb))
            }
            "min" => {
                let lhs_nb = self.registers[base + a + 1];
                let rhs_nb = self.registers[base + a + 2];
                // NbValue fast-paths: int and float comparisons need no heap access.
                if lhs_nb.is_int() && rhs_nb.is_int() {
                    let x = lhs_nb.as_int().unwrap_or(0);
                    let y = rhs_nb.as_int().unwrap_or(0);
                    return Ok(Value::Int(x.min(y)));
                }
                if lhs_nb.is_float() && rhs_nb.is_float() {
                    let x = f64::from_bits(lhs_nb.0);
                    let y = f64::from_bits(rhs_nb.0);
                    return Ok(Value::Float(x.min(y)));
                }
                if lhs_nb.is_int() && rhs_nb.is_float() {
                    let x = lhs_nb.as_int().unwrap_or(0) as f64;
                    let y = f64::from_bits(rhs_nb.0);
                    return Ok(Value::Float(x.min(y)));
                }
                if lhs_nb.is_float() && rhs_nb.is_int() {
                    let x = f64::from_bits(lhs_nb.0);
                    let y = rhs_nb.as_int().unwrap_or(0) as f64;
                    return Ok(Value::Float(x.min(y)));
                }
                // Cold path: strings etc. — return the smaller of the two.
                Ok(self.nb_to_value_deep(lhs_nb))
            }
            "max" => {
                let lhs_nb = self.registers[base + a + 1];
                let rhs_nb = self.registers[base + a + 2];
                // NbValue fast-paths: int and float comparisons need no heap access.
                if lhs_nb.is_int() && rhs_nb.is_int() {
                    let x = lhs_nb.as_int().unwrap_or(0);
                    let y = rhs_nb.as_int().unwrap_or(0);
                    return Ok(Value::Int(x.max(y)));
                }
                if lhs_nb.is_float() && rhs_nb.is_float() {
                    let x = f64::from_bits(lhs_nb.0);
                    let y = f64::from_bits(rhs_nb.0);
                    return Ok(Value::Float(x.max(y)));
                }
                if lhs_nb.is_int() && rhs_nb.is_float() {
                    let x = lhs_nb.as_int().unwrap_or(0) as f64;
                    let y = f64::from_bits(rhs_nb.0);
                    return Ok(Value::Float(x.max(y)));
                }
                if lhs_nb.is_float() && rhs_nb.is_int() {
                    let x = f64::from_bits(lhs_nb.0);
                    let y = rhs_nb.as_int().unwrap_or(0) as f64;
                    return Ok(Value::Float(x.max(y)));
                }
                // Cold path: strings etc. — return the larger of the two.
                Ok(self.nb_to_value_deep(lhs_nb))
            }
            "range" => {
                // NbValue fast-path: extract ints without peek_legacy.
                let start_nb = self.registers[base + a + 1];
                let end_nb = self.registers[base + a + 2];
                let start = self.nb_to_int(start_nb).unwrap_or(0);
                let end = self.nb_to_int(end_nb).unwrap_or(0);
                let list: Vec<NbValue> = (start..end).map(NbValue::new_int).collect();
                Ok(Value::List(Arc::new(list)))
            }
            "spawn" => {
                if nargs == 0 {
                    return Err(VmError::TypeError(
                        "spawn requires a callable argument".to_string(),
                    ));
                }
                let callee_nb = self.registers[base + a + 1];
                let callee = self.nb_to_value_deep(callee_nb);
                let args: Vec<Value> = (1..nargs)
                    .map(|i| {
                        let arg_nb = self.registers[base + a + 1 + i];
                        self.nb_to_value_deep(arg_nb)
                    })
                    .collect();
                match callee {
                    Value::Closure(cv) => self.spawn_future(FutureTarget::Closure(cv), args),
                    Value::String(sr) => {
                        let name = match sr {
                            StringRef::Owned(s) => s,
                            StringRef::Interned(id) => self
                                .strings
                                .resolve(id)
                                .ok_or_else(|| {
                                    VmError::Runtime(format!(
                                        "unknown interned string id {} for spawn target",
                                        id
                                    ))
                                })?
                                .to_string(),
                        };
                        let module = self.module.as_ref().ok_or(VmError::NoModule)?;
                        let cell_idx = module
                            .cells
                            .iter()
                            .position(|c| c.name == name)
                            .ok_or_else(|| {
                                VmError::TypeError(format!(
                                    "spawn target '{}' is not a cell or closure",
                                    name
                                ))
                            })?;
                        self.spawn_future(FutureTarget::Cell(cell_idx), args)
                    }
                    other => Err(VmError::TypeError(format!(
                        "spawn expects a callable, got {}",
                        other
                    ))),
                }
            }
            "parallel" => {
                let args = self.orchestration_args(base, a, nargs);
                let mut out: Vec<NbValue> = Vec::with_capacity(args.len());
                for arg in args {
                    if let Some(lumen_core::heap_value::HeapValue::Future(f)) = arg.as_heap_ref() {
                        match self.future_states.get(&f.id) {
                            Some(FutureState::Completed(v)) => {
                                out.push(self.nb_from_value(v.clone()))
                            }
                            Some(FutureState::Pending) => out.push(arg),
                            Some(FutureState::Error(_)) | None => out.push(NbValue::new_null()),
                        }
                    } else {
                        out.push(arg);
                    }
                }
                Ok(Value::List(Arc::new(out)))
            }
            "race" => {
                let mut first_pending: Option<NbValue> = None;
                for arg in self.orchestration_args(base, a, nargs) {
                    if let Some(lumen_core::heap_value::HeapValue::Future(f)) = arg.as_heap_ref() {
                        match self.future_states.get(&f.id) {
                            Some(FutureState::Completed(v)) => return Ok(v.clone()),
                            Some(FutureState::Pending) => {
                                if first_pending.is_none() {
                                    first_pending = Some(arg);
                                }
                            }
                            Some(FutureState::Error(_)) | None => {}
                        }
                    } else {
                        return Ok(self.nb_to_value_deep(arg));
                    }
                }
                Ok(first_pending
                    .map(|nb| self.nb_to_value_deep(nb))
                    .unwrap_or(Value::Null))
            }
            "select" => {
                let mut first_pending: Option<NbValue> = None;
                for arg in self.orchestration_args(base, a, nargs) {
                    if let Some(lumen_core::heap_value::HeapValue::Future(f)) = arg.as_heap_ref() {
                        let candidate = match self.future_states.get(&f.id) {
                            Some(FutureState::Completed(v)) => Some(v.clone()),
                            Some(FutureState::Pending) => {
                                if first_pending.is_none() {
                                    first_pending = Some(arg);
                                }
                                None
                            }
                            _ => None,
                        };
                        if let Some(value) = candidate {
                            if !matches!(value, Value::Null) {
                                return Ok(value);
                            }
                        }
                    } else if !arg.is_null() {
                        return Ok(self.nb_to_value_deep(arg));
                    }
                }
                Ok(first_pending
                    .map(|nb| self.nb_to_value_deep(nb))
                    .unwrap_or(Value::Null))
            }
            "vote" => {
                let mut candidates: Vec<Value> = Vec::new();
                let mut first_pending: Option<NbValue> = None;
                for arg in self.orchestration_args(base, a, nargs) {
                    if let Some(lumen_core::heap_value::HeapValue::Future(f)) = arg.as_heap_ref() {
                        match self.future_states.get(&f.id) {
                            Some(FutureState::Completed(v)) => candidates.push(v.clone()),
                            Some(FutureState::Pending) => {
                                if first_pending.is_none() {
                                    first_pending = Some(arg);
                                }
                            }
                            _ => {}
                        }
                    } else {
                        candidates.push(self.nb_to_value_deep(arg));
                    }
                }
                if candidates.is_empty() {
                    return Ok(first_pending
                        .map(|nb| self.nb_to_value_deep(nb))
                        .unwrap_or(Value::Null));
                }
                // Find mode: value with highest frequency (earliest on tie)
                let mut best: Option<Value> = None;
                let mut best_count = 0usize;
                for candidate in &candidates {
                    let count = candidates.iter().filter(|v| *v == candidate).count();
                    if count > best_count {
                        best_count = count;
                        best = Some(candidate.clone());
                    }
                }
                Ok(best.unwrap_or(Value::Null))
            }
            "timeout" => {
                if nargs == 0 {
                    return Ok(Value::Null);
                }
                let arg_nb = self.registers[base + a + 1];
                if let Some(hv) = arg_nb.as_heap_ref() {
                    match hv {
                        HeapValue::Future(f) => match self.future_states.get(&f.id) {
                            Some(FutureState::Completed(v)) => Ok(v.clone()),
                            Some(FutureState::Pending) => Ok(Value::Null),
                            Some(FutureState::Error(msg)) => {
                                Err(VmError::Runtime(format!("timeout target failed: {}", msg)))
                            }
                            None => Ok(Value::Null),
                        },
                        _ => Ok(self.nb_to_value_deep(arg_nb)),
                    }
                } else {
                    Ok(self.nb_to_value_deep(arg_nb))
                }
            }
            "hash" | "sha256" => {
                use sha2::{Digest, Sha256};
                let s = self.nb_to_string_resolved(self.registers[base + a + 1]);
                let h = format!("sha256:{:x}", Sha256::digest(s.as_bytes()));
                Ok(Value::String(StringRef::Owned(h)))
            }
            "sort" => {
                let arg = self.reg_take(base + a + 1);
                if let Value::List(mut l) = arg {
                    sort_list_homogeneous(Arc::make_mut(&mut l));
                    Ok(Value::List(l))
                } else {
                    Ok(arg)
                }
            }
            "reverse" => {
                let arg = self.reg_take(base + a + 1);
                if let Value::List(mut l) = arg {
                    Arc::make_mut(&mut l).reverse();
                    Ok(Value::List(l))
                } else {
                    Ok(arg)
                }
            }
            "flatten" => {
                let nb = self.registers[base + a + 1];
                if let Some(HeapValue::List(l)) = nb.as_heap_ref() {
                    let mut result = Vec::new();
                    for item in l.iter() {
                        let value = item;
                        if let Some(HeapValue::List(inner)) = value.as_heap_ref() {
                            result.extend(inner.iter().cloned());
                        } else {
                            result.push(*value);
                        }
                    }
                    return Ok(Value::List(Arc::new(result)));
                }
                Ok(Value::Null)
            }
            "unique" => {
                let nb = self.registers[base + a + 1];
                if let Some(HeapValue::List(l)) = nb.as_heap_ref() {
                    let mut result = Vec::new();
                    for item in l.iter() {
                        if !result.contains(item) {
                            result.push(*item);
                        }
                    }
                    return Ok(Value::List(Arc::new(result)));
                }
                Ok(Value::Null)
            }
            "take" => {
                let nb = self.registers[base + a + 1];
                let n = self.nb_to_int(self.registers[base + a + 2]).unwrap_or(0) as usize;
                if let Some(HeapValue::List(l)) = nb.as_heap_ref() {
                    return Ok(Value::List(Arc::new(l.iter().take(n).cloned().collect())));
                }
                let arg = self.nb_to_value_deep(nb);
                if let Value::List(l) = arg {
                    Ok(Value::List(Arc::new(l.iter().take(n).cloned().collect())))
                } else {
                    Ok(arg)
                }
            }
            "drop" => {
                let nb = self.registers[base + a + 1];
                let n = self.nb_to_int(self.registers[base + a + 2]).unwrap_or(0) as usize;
                if let Some(HeapValue::List(l)) = nb.as_heap_ref() {
                    return Ok(Value::List(Arc::new(l.iter().skip(n).cloned().collect())));
                }
                let arg = self.nb_to_value_deep(nb);
                if let Value::List(l) = arg {
                    Ok(Value::List(Arc::new(l.iter().skip(n).cloned().collect())))
                } else {
                    Ok(arg)
                }
            }
            "first" | "head" => {
                let nb = self.registers[base + a + 1];
                if let Some(hv) = nb.as_heap_ref() {
                    return Ok(match hv {
                        HeapValue::List(l) => l
                            .first()
                            .map(|v| self.nb_to_value_deep(*v))
                            .unwrap_or(Value::Null),
                        HeapValue::Tuple(t) => t
                            .first()
                            .map(|v| self.nb_to_value_deep(*v))
                            .unwrap_or(Value::Null),
                        _ => Value::Null,
                    });
                }
                Ok(Value::Null)
            }
            "last" | "tail" => {
                let nb = self.registers[base + a + 1];
                if let Some(hv) = nb.as_heap_ref() {
                    return Ok(match hv {
                        HeapValue::List(l) => l
                            .last()
                            .map(|v| self.nb_to_value_deep(*v))
                            .unwrap_or(Value::Null),
                        HeapValue::Tuple(t) => t
                            .last()
                            .map(|v| self.nb_to_value_deep(*v))
                            .unwrap_or(Value::Null),
                        _ => Value::Null,
                    });
                }
                Ok(Value::Null)
            }
            "is_empty" | "empty" => {
                let nb = self.registers[base + a + 1];
                if nb.is_null() {
                    return Ok(Value::Bool(true));
                }
                if nb.is_int() || nb.is_float() || nb.is_bool() {
                    return Ok(Value::Bool(false));
                }
                if let Some(hv) = nb.as_heap_ref() {
                    let empty = match hv {
                        HeapValue::List(l) => l.is_empty(),
                        HeapValue::Map(m) => m.is_empty(),
                        HeapValue::Set(s) => s.is_empty(),
                        HeapValue::Tuple(t) => t.is_empty(),
                        HeapValue::Str(s) => s.is_empty(),
                        _ => false,
                    };
                    return Ok(Value::Bool(empty));
                }
                let arg = self.nb_to_value_deep(nb);
                let empty = match &arg {
                    Value::List(l) => l.is_empty(),
                    Value::Map(m) => m.is_empty(),
                    Value::Set(s) => s.is_empty(),
                    Value::String(_) => arg.as_string_resolved(&self.strings).is_empty(),
                    Value::Null => true,
                    _ => false,
                };
                Ok(Value::Bool(empty))
            }
            "chars" => {
                let s = self.nb_to_string_resolved(self.registers[base + a + 1]);
                let chars: Vec<NbValue> = s
                    .chars()
                    .map(|c| NbValue::new_str(&c.to_string()))
                    .collect();
                Ok(Value::List(Arc::new(chars)))
            }
            "starts_with" => {
                let s = self.nb_to_string_resolved(self.registers[base + a + 1]);
                let prefix = self.nb_to_string_resolved(self.registers[base + a + 2]);
                Ok(Value::Bool(s.starts_with(&prefix)))
            }
            "ends_with" => {
                let s = self.nb_to_string_resolved(self.registers[base + a + 1]);
                let suffix = self.nb_to_string_resolved(self.registers[base + a + 2]);
                Ok(Value::Bool(s.ends_with(&suffix)))
            }
            "index_of" => {
                let s = self.nb_to_string_resolved(self.registers[base + a + 1]);
                let needle = self.nb_to_string_resolved(self.registers[base + a + 2]);
                Ok(match s.find(&needle) {
                    Some(i) => {
                        let char_idx = s[..i].chars().count();
                        Value::Int(char_idx as i64)
                    }
                    None => Value::Int(-1),
                })
            }
            "pad_left" => {
                let s = self.nb_to_string_resolved(self.registers[base + a + 1]);
                let width = self.nb_to_int(self.registers[base + a + 2]).unwrap_or(0) as usize;
                let char_count = s.chars().count();
                if char_count < width {
                    let padding = " ".repeat(width - char_count);
                    Ok(Value::String(StringRef::Owned(format!("{}{}", padding, s))))
                } else {
                    Ok(Value::String(StringRef::Owned(s)))
                }
            }
            "pad_right" => {
                let s = self.nb_to_string_resolved(self.registers[base + a + 1]);
                let width = self.nb_to_int(self.registers[base + a + 2]).unwrap_or(0) as usize;
                let char_count = s.chars().count();
                if char_count < width {
                    let padding = " ".repeat(width - char_count);
                    Ok(Value::String(StringRef::Owned(format!("{}{}", s, padding))))
                } else {
                    Ok(Value::String(StringRef::Owned(s)))
                }
            }
            "round" => {
                let nb = self.registers[base + a + 1];
                if nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(nb.0).round()));
                }
                if nb.is_int() {
                    return Ok(Value::Int(nb.as_int().unwrap_or(0)));
                }
                Ok(Value::Null)
            }
            "ceil" => {
                let nb = self.registers[base + a + 1];
                if nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(nb.0).ceil()));
                }
                if nb.is_int() {
                    return Ok(Value::Int(nb.as_int().unwrap_or(0)));
                }
                Ok(Value::Null)
            }
            "floor" => {
                let nb = self.registers[base + a + 1];
                if nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(nb.0).floor()));
                }
                if nb.is_int() {
                    return Ok(Value::Int(nb.as_int().unwrap_or(0)));
                }
                Ok(Value::Null)
            }
            "sqrt" => {
                let nb = self.registers[base + a + 1];
                // NbValue fast-paths — no heap touch for float/int.
                if nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(nb.0).sqrt()));
                }
                if nb.is_int() {
                    return Ok(Value::Float((nb.as_int().unwrap_or(0) as f64).sqrt()));
                }
                if let Some(hv) = nb.as_heap_ref() {
                    return Ok(match hv {
                        HeapValue::BigInt(n) => {
                            Value::Float(n.to_f64().unwrap_or(f64::INFINITY).sqrt())
                        }
                        _ => Value::Null,
                    });
                }
                Ok(Value::Null)
            }
            "pow" => {
                let base_nb = self.registers[base + a + 1];
                let exp_nb = self.registers[base + a + 2];
                // NbValue fast-path: int**int and float**float.
                if base_nb.is_int() && exp_nb.is_int() {
                    let x = base_nb.as_int().unwrap_or(0);
                    let y = exp_nb.as_int().unwrap_or(0);
                    if y >= 0 {
                        if let Ok(y_u32) = u32::try_from(y) {
                            if let Some(res) = x.checked_pow(y_u32) {
                                return Ok(Value::Int(res));
                            } else {
                                return Ok(Value::BigInt(BigInt::from(x).pow(y_u32)));
                            }
                        }
                    } else {
                        return Ok(Value::Float((x as f64).powf(y as f64)));
                    }
                }
                if base_nb.is_float() && exp_nb.is_float() {
                    return Ok(Value::Float(
                        f64::from_bits(base_nb.0).powf(f64::from_bits(exp_nb.0)),
                    ));
                }
                if base_nb.is_float() && exp_nb.is_int() {
                    return Ok(Value::Float(
                        f64::from_bits(base_nb.0).powf(exp_nb.as_int().unwrap_or(0) as f64),
                    ));
                }
                Ok(Value::Null)
            }
            "log" => {
                let nb = self.registers[base + a + 1];
                if nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(nb.0).ln()));
                }
                if nb.is_int() {
                    return Ok(Value::Float((nb.as_int().unwrap_or(0) as f64).ln()));
                }
                Ok(Value::Null)
            }
            "sin" => {
                let nb = self.registers[base + a + 1];
                if nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(nb.0).sin()));
                }
                if nb.is_int() {
                    return Ok(Value::Float((nb.as_int().unwrap_or(0) as f64).sin()));
                }
                Ok(Value::Null)
            }
            "cos" => {
                let nb = self.registers[base + a + 1];
                if nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(nb.0).cos()));
                }
                if nb.is_int() {
                    return Ok(Value::Float((nb.as_int().unwrap_or(0) as f64).cos()));
                }
                Ok(Value::Null)
            }
            "tan" => {
                let nb = self.registers[base + a + 1];
                if nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(nb.0).tan()));
                }
                if nb.is_int() {
                    return Ok(Value::Float((nb.as_int().unwrap_or(0) as f64).tan()));
                }
                Ok(Value::Null)
            }
            "exp" => {
                let nb = self.registers[base + a + 1];
                if nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(nb.0).exp()));
                }
                if nb.is_int() {
                    return Ok(Value::Float((nb.as_int().unwrap_or(0) as f64).exp()));
                }
                Ok(Value::Null)
            }
            "clamp" => {
                let val_nb = self.registers[base + a + 1];
                let lo_nb = self.registers[base + a + 2];
                let hi_nb = self.registers[base + a + 3];
                // NbValue fast-paths: int clamp and float clamp.
                if val_nb.is_int() && lo_nb.is_int() && hi_nb.is_int() {
                    let v = val_nb.as_int().unwrap_or(0);
                    let l = lo_nb.as_int().unwrap_or(0);
                    let h = hi_nb.as_int().unwrap_or(0);
                    return Ok(Value::Int(v.max(l).min(h)));
                }
                if val_nb.is_float() && lo_nb.is_float() && hi_nb.is_float() {
                    let v = f64::from_bits(val_nb.0);
                    let l = f64::from_bits(lo_nb.0);
                    let h = f64::from_bits(hi_nb.0);
                    return Ok(Value::Float(v.max(l).min(h)));
                }
                // Cold path: return val unchanged (no clamp for non-numeric types).
                Ok(self.nb_to_value_deep(val_nb))
            }
            "read_file" => {
                let path = self.nb_to_string_resolved(self.registers[base + a + 1]);
                match std::fs::read_to_string(path) {
                    Ok(contents) => Ok(Value::String(StringRef::Owned(contents))),
                    Err(e) => Err(VmError::Runtime(format!("read_file failed: {}", e))),
                }
            }
            "write_file" => {
                let path = self.nb_to_string_resolved(self.registers[base + a + 1]);
                let content_str = self.nb_to_string_resolved(self.registers[base + a + 2]);
                match std::fs::write(path, content_str.as_bytes()) {
                    Ok(()) => Ok(Value::Null),
                    Err(e) => Err(VmError::Runtime(format!("write_file failed: {}", e))),
                }
            }
            "get_env" => {
                let name = self.nb_to_string_resolved(self.registers[base + a + 1]);
                match std::env::var(name) {
                    Ok(val) => Ok(Value::String(StringRef::Owned(val))),
                    Err(_) => Ok(Value::Null),
                }
            }
            "random" => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                Ok(Value::Float((now % 1000) as f64 / 1000.0))
            }
            "now" | "timestamp" => {
                let dur = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default();
                Ok(Value::Float(dur.as_secs_f64()))
            }
            "uuid" => {
                let id = uuid::Uuid::new_v4().to_string();
                Ok(Value::String(StringRef::Owned(id)))
            }
            "random_int" => {
                let min = self.nb_to_int(self.registers[base + a + 1]).unwrap_or(0);
                let max = self
                    .nb_to_int(self.registers[base + a + 2])
                    .unwrap_or(i64::MAX);
                if min > max {
                    return Err(VmError::Runtime(format!(
                        "random_int: min ({}) must be less than or equal to max ({})",
                        min, max
                    )));
                }
                if min == max {
                    return Ok(Value::Int(min));
                }
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as u64;
                let range = (max - min) as u64;
                let result = (now % range) as i64 + min;
                Ok(Value::Int(result))
            }
            "panic" => {
                let msg = if nargs > 0 {
                    self.nb_to_string_resolved(self.registers[base + a + 1])
                } else {
                    "panic called".to_string()
                };
                Err(VmError::Runtime(msg))
            }
            "trace" => {
                let frames = self.capture_stack_trace();
                for (i, frame) in frames.iter().enumerate() {
                    println!("  #{}: {} (ip={})", i, frame.cell_name, frame.ip);
                }
                Ok(Value::Null)
            }
            "hex_decode" => {
                let s = self.nb_to_string_resolved(self.registers[base + a + 1]);
                if !s.is_ascii() || s.len() % 2 != 0 {
                    return Ok(Value::Null);
                }
                let mut bytes = Vec::with_capacity(s.len() / 2);
                for chunk in s.as_bytes().chunks_exact(2) {
                    let pair = match std::str::from_utf8(chunk) {
                        Ok(pair) => pair,
                        Err(_) => return Ok(Value::Null),
                    };
                    match u8::from_str_radix(pair, 16) {
                        Ok(byte) => bytes.push(byte),
                        Err(_) => return Ok(Value::Null),
                    }
                }
                Ok(Value::String(StringRef::Owned(
                    String::from_utf8_lossy(&bytes).to_string(),
                )))
            }
            "trim_start" => {
                let s = self.nb_to_string_resolved(self.registers[base + a + 1]);
                Ok(Value::String(StringRef::Owned(s.trim_start().to_string())))
            }
            "trim_end" => {
                let s = self.nb_to_string_resolved(self.registers[base + a + 1]);
                Ok(Value::String(StringRef::Owned(s.trim_end().to_string())))
            }
            "bytes_from_ascii" => {
                // bytes_from_ascii(s: String) -> Bytes
                // Convert an ASCII/UTF-8 string to a Bytes value (Vec<u8>).
                let nb = self.registers[base + a + 1];
                let s = self.nb_to_string_resolved(nb);
                Ok(Value::Bytes(s.into_bytes()))
            }
            "bytes_to_ascii" => {
                // bytes_to_ascii(b: Bytes) -> String
                // Convert a Bytes value back to a String. Returns Null on non-Bytes input.
                let nb = self.registers[base + a + 1];
                if let Some(HeapValue::Bytes(b)) = nb.as_heap_ref() {
                    return Ok(Value::String(StringRef::Owned(
                        String::from_utf8_lossy(b).to_string(),
                    )));
                }
                Ok(Value::Null)
            }
            "bytes_len" => {
                // bytes_len(b: Bytes) -> Int
                // Return the number of bytes in a Bytes value. Returns 0 for non-Bytes.
                let nb = self.registers[base + a + 1];
                if let Some(HeapValue::Bytes(b)) = nb.as_heap_ref() {
                    return Ok(Value::Int(b.len() as i64));
                }
                Ok(Value::Int(0))
            }
            "bytes_slice" => {
                // bytes_slice(b: Bytes, start: Int, end: Int) -> Bytes
                // Return a sub-slice of bytes from start (inclusive) to end (exclusive).
                // If end <= 0, slice to the end of the bytes.
                // Returns Null for non-Bytes input.
                let nb = self.registers[base + a + 1];
                let start_nb = self.registers[base + a + 2];
                let end_nb = self.registers[base + a + 3];
                let start = self.nb_to_int(start_nb).unwrap_or(0) as usize;
                let end_raw = self.nb_to_int(end_nb).unwrap_or(0);
                let get_slice = |b: &[u8]| -> Value {
                    let len = b.len();
                    let end = if end_raw <= 0 {
                        len
                    } else {
                        (end_raw as usize).min(len)
                    };
                    let start = start.min(len);
                    let end = end.max(start);
                    Value::Bytes(b[start..end].to_vec())
                };
                if let Some(HeapValue::Bytes(b)) = nb.as_heap_ref() {
                    return Ok(get_slice(b));
                }
                Ok(Value::Null)
            }
            "bytes_concat" => {
                // bytes_concat(a: Bytes, b: Bytes) -> Bytes
                // Concatenate two Bytes values. Returns Null if either argument is not Bytes.
                let nb_a = self.registers[base + a + 1];
                let nb_b = self.registers[base + a + 2];
                let bytes_a = if let Some(hv) = nb_a.as_heap_ref() {
                    match hv {
                        HeapValue::Bytes(b) => Some(b.as_ref().to_vec()),
                        _ => None,
                    }
                } else {
                    None
                };
                let bytes_b = if let Some(hv) = nb_b.as_heap_ref() {
                    match hv {
                        HeapValue::Bytes(b) => Some(b.as_ref().to_vec()),
                        _ => None,
                    }
                } else {
                    None
                };
                Ok(match (bytes_a, bytes_b) {
                    (Some(mut a_vec), Some(b_vec)) => {
                        a_vec.extend_from_slice(&b_vec);
                        Value::Bytes(a_vec)
                    }
                    _ => Value::Null,
                })
            }
            _ => Err(VmError::Runtime(format!("unknown builtin: {}", name))),
        }
    }

    /// Execute an intrinsic function by ID.
    pub(crate) fn exec_intrinsic(
        &mut self,
        base: usize,
        a: usize,
        func_id: usize,
        arg_reg: usize,
    ) -> Result<Value, VmError> {
        let arg_nb = self.reg_take_nb(base + arg_reg);
        let arg = self.nb_to_value_deep(arg_nb);
        match func_id {
            140 => {
                // JSON_PARSE
                let s = self.nb_to_string_resolved(arg_nb);
                let parsed = match parse_json_optimized(&s) {
                    Ok(v) => v,
                    Err(_) => NbValue::new_null(),
                };
                self.set_reg_nb(base + a, parsed);
                return Ok(Value::Null);
            }
            141 => {
                // JSON_ENCODE
                let encoded = nb_value_to_json_string(arg_nb)
                    .map_err(|e| VmError::Runtime(format!("json_encode failed: {e}")))?;
                self.set_reg_nb(base + a, NbValue::new_str(&encoded));
                return Ok(Value::Null);
            }
            142 => {
                // JSON_PRETTY
                let encoded = nb_value_to_json_pretty_string(arg_nb)
                    .map_err(|e| VmError::Runtime(format!("json_pretty failed: {e}")))?;
                self.set_reg_nb(base + a, NbValue::new_str(&encoded));
                return Ok(Value::Null);
            }
            0 => {
                // LENGTH
                if arg_nb.is_int() || arg_nb.is_float() || arg_nb.is_bool() || arg_nb.is_null() {
                    return Ok(Value::Int(0));
                }
                if let Some(hv) = arg_nb.as_heap_ref() {
                    let len = match hv {
                        HeapValue::Str(s) => s.chars().count() as i64,
                        HeapValue::List(l) => l.len() as i64,
                        HeapValue::Map(m) => m.len() as i64,
                        HeapValue::Tuple(t) => t.len() as i64,
                        HeapValue::Set(s) => s.len() as i64,
                        HeapValue::Bytes(b) => b.len() as i64,
                        _ => 0,
                    };
                    return Ok(Value::Int(len));
                }
                let out = match arg {
                    Value::String(StringRef::Owned(s)) => Value::Int(s.chars().count() as i64),
                    Value::String(StringRef::Interned(id)) => {
                        let s = self.strings.resolve(id).unwrap_or("");
                        Value::Int(s.chars().count() as i64)
                    }
                    Value::List(l) => Value::Int(l.len() as i64),
                    Value::Map(m) => Value::Int(m.len() as i64),
                    Value::Tuple(t) => Value::Int(t.len() as i64),
                    Value::Set(s) => Value::Int(s.len() as i64),
                    Value::Bytes(b) => Value::Int(b.len() as i64),
                    _ => Value::Int(0),
                };
                return Ok(out);
            }
            1 => {
                // COUNT
                if arg_nb.is_int() || arg_nb.is_float() || arg_nb.is_bool() || arg_nb.is_null() {
                    return Ok(Value::Int(0));
                }
                if let Some(hv) = arg_nb.as_heap_ref() {
                    let count = match hv {
                        HeapValue::List(l) => l.len() as i64,
                        HeapValue::Map(m) => m.len() as i64,
                        HeapValue::Str(s) => s.chars().count() as i64,
                        _ => 0,
                    };
                    return Ok(Value::Int(count));
                }
                return Ok(Value::Int(0));
            }
            2 => {
                // MATCHES
                if arg_nb.is_bool() {
                    return Ok(Value::Bool(arg_nb.as_bool().unwrap_or(false)));
                }
                if arg_nb.is_int() {
                    return Ok(Value::Bool(arg_nb.as_int().unwrap_or(0) != 0));
                }
                if arg_nb.is_null() {
                    return Ok(Value::Bool(false));
                }
                if arg_nb.is_float() {
                    return Ok(Value::Bool(f64::from_bits(arg_nb.0) != 0.0));
                }
                if let Some(hv) = arg_nb.as_heap_ref() {
                    return Ok(Value::Bool(hv.is_truthy()));
                }
                return Ok(Value::Bool(self.nb_to_value_deep(arg_nb).is_truthy()));
            }
            3 => {
                // HASH
                use sha2::{Digest, Sha256};
                let s = self.nb_display_pretty(arg_nb);
                let hash = format!("{:x}", Sha256::digest(s.as_bytes()));
                return Ok(Value::String(StringRef::Owned(format!("sha256:{}", hash))));
            }
            67 => {
                // SIZEOF
                if arg_nb.is_int() || arg_nb.is_float() || arg_nb.is_bool() || arg_nb.is_null() {
                    return Ok(Value::Int(8));
                }
                if let Some(hv) = arg_nb.as_heap_ref() {
                    let size_bytes = match hv {
                        HeapValue::BigInt(_) => 8,
                        HeapValue::Str(s) => s.len() as i64,
                        HeapValue::Bytes(b) => b.len() as i64,
                        HeapValue::List(l) | HeapValue::Tuple(l) => (l.len() as i64) * 8,
                        HeapValue::Set(s) => (s.len() as i64) * 8,
                        HeapValue::Map(m) => (m.len() as i64) * 16,
                        HeapValue::Record(r) => (r.fields.len() as i64) * 16,
                        HeapValue::Union(_) => 16,
                        HeapValue::Closure(c) => (c.captures.len() as i64) * 8 + 16,
                        HeapValue::TraceRef(_) => 16,
                        HeapValue::Future(_) => 16,
                    };
                    return Ok(Value::Int(size_bytes));
                }
                let size_bytes = match arg {
                    Value::Null
                    | Value::Bool(_)
                    | Value::Int(_)
                    | Value::BigInt(_)
                    | Value::Float(_) => 8,
                    Value::String(StringRef::Owned(s)) => s.len() as i64,
                    Value::String(StringRef::Interned(id)) => self
                        .strings
                        .resolve(id)
                        .map(|s| s.len() as i64)
                        .unwrap_or(0),
                    Value::Bytes(b) => b.len() as i64,
                    Value::List(l) | Value::Tuple(l) => (l.len() as i64) * 8,
                    Value::Set(s) => (s.len() as i64) * 8,
                    Value::Map(m) => (m.len() as i64) * 16,
                    Value::Record(r) => (r.fields.len() as i64) * 16,
                    Value::Union(_) => 16,
                    Value::Closure(c) => (c.captures.len() as i64) * 8 + 16,
                    Value::TraceRef(_) => 16,
                    Value::Future(_) => 16,
                };
                return Ok(Value::Int(size_bytes));
            }
            9 => {
                // PRINT
                let output = self.nb_display_pretty(arg_nb);
                println!("{}", output);
                self.output.push(output);
                return Ok(Value::Null);
            }
            10 => {
                // TO_STRING
                return Ok(Value::String(StringRef::Owned(
                    self.nb_display_pretty(arg_nb),
                )));
            }
            11 => {
                // TO_INT
                if arg_nb.is_int() {
                    return Ok(Value::Int(arg_nb.as_int().unwrap_or(0)));
                }
                if arg_nb.is_float() {
                    return Ok(Value::Int(f64::from_bits(arg_nb.0) as i64));
                }
                if arg_nb.is_bool() {
                    return Ok(Value::Int(if arg_nb.as_bool().unwrap_or(false) {
                        1
                    } else {
                        0
                    }));
                }
                if arg_nb.is_null() {
                    return Ok(Value::Null);
                }
                if let Some(hv) = arg_nb.as_heap_ref() {
                    let out = match hv {
                        HeapValue::BigInt(n) => Value::Int(n.to_i64().unwrap_or(0)),
                        HeapValue::Str(s) => {
                            s.parse::<i64>().map(Value::Int).unwrap_or(Value::Null)
                        }
                        _ => Value::Null,
                    };
                    return Ok(out);
                }
                return Ok(match arg {
                    Value::Int(n) => Value::Int(n),
                    Value::Float(f) => Value::Int(f as i64),
                    Value::String(sr) => {
                        let s = match sr {
                            StringRef::Owned(s) => s,
                            StringRef::Interned(id) => {
                                self.strings.resolve(id).unwrap_or("").to_string()
                            }
                        };
                        s.parse::<i64>().map(Value::Int).unwrap_or(Value::Null)
                    }
                    Value::Bool(b) => Value::Int(if b { 1 } else { 0 }),
                    _ => Value::Null,
                });
            }
            12 => {
                // TO_FLOAT
                if arg_nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(arg_nb.0)));
                }
                if arg_nb.is_int() {
                    return Ok(Value::Float(arg_nb.as_int().unwrap_or(0) as f64));
                }
                if arg_nb.is_null() || arg_nb.is_bool() {
                    return Ok(Value::Null);
                }
                if let Some(hv) = arg_nb.as_heap_ref() {
                    let out = match hv {
                        HeapValue::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::NAN)),
                        HeapValue::Str(s) => {
                            s.parse::<f64>().map(Value::Float).unwrap_or(Value::Null)
                        }
                        _ => Value::Null,
                    };
                    return Ok(out);
                }
                return Ok(match arg {
                    Value::Float(f) => Value::Float(f),
                    Value::Int(n) => Value::Float(n as f64),
                    Value::String(sr) => {
                        let s = match sr {
                            StringRef::Owned(s) => s,
                            StringRef::Interned(id) => {
                                self.strings.resolve(id).unwrap_or("").to_string()
                            }
                        };
                        s.parse::<f64>().map(Value::Float).unwrap_or(Value::Null)
                    }
                    _ => Value::Null,
                });
            }
            13 => {
                // TYPE_OF
                if arg_nb.is_int() {
                    return Ok(Value::String(StringRef::Owned("Int".to_string())));
                }
                if arg_nb.is_float() {
                    return Ok(Value::String(StringRef::Owned("Float".to_string())));
                }
                if arg_nb.is_bool() {
                    return Ok(Value::String(StringRef::Owned("Bool".to_string())));
                }
                if arg_nb.is_null() {
                    return Ok(Value::String(StringRef::Owned("Null".to_string())));
                }
                if let Some(hv) = arg_nb.as_heap_ref() {
                    return Ok(Value::String(StringRef::Owned(hv.type_name().to_string())));
                }
                return Ok(Value::String(StringRef::Owned(
                    arg_nb.type_name().to_string(),
                )));
            }
            16 => {
                // CONTAINS
                let needle_nb = self.registers[base + arg_reg + 1];
                let needle = needle_nb;
                if let Some(collection) = arg_nb.as_heap_ref() {
                    let result = match collection {
                        HeapValue::List(l) => l.iter().any(|v| v == &needle),
                        HeapValue::Set(s) => s.contains(&needle),
                        HeapValue::Map(m) => {
                            let needle_str = self.nb_to_string_resolved(needle);
                            m.contains_key(&needle_str)
                        }
                        HeapValue::Str(s) => {
                            let needle_str = self.nb_to_string_resolved(needle);
                            s.contains(&needle_str)
                        }
                        _ => false,
                    };
                    return Ok(Value::Bool(result));
                }
                if arg_nb.is_int() || arg_nb.is_float() || arg_nb.is_bool() || arg_nb.is_null() {
                    return Ok(Value::Bool(false));
                }
                return Ok(Value::Bool(false));
            }
            17 => {
                // JOIN
                let sep = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                if let Some(HeapValue::List(l)) = arg_nb.as_heap_ref() {
                    let joined = l.iter().map(|v| v.display()).collect::<Vec<_>>().join(&sep);
                    return Ok(Value::String(StringRef::Owned(joined)));
                }
                return Ok(Value::String(StringRef::Owned(String::new())));
            }
            18 => {
                // SPLIT
                let sep = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                let s = self.nb_to_string_as_resolved_value(arg_nb);
                let parts: Vec<NbValue> = s.split(&sep).map(NbValue::new_str).collect();
                return Ok(Value::List(Arc::new(parts)));
            }
            19 => {
                // TRIM
                let s = self.nb_to_string_as_resolved_value(arg_nb);
                return Ok(Value::String(StringRef::Owned(s.trim().to_string())));
            }
            20 => {
                // UPPER
                let s = self.nb_to_string_as_resolved_value(arg_nb);
                return Ok(Value::String(StringRef::Owned(s.to_uppercase())));
            }
            21 => {
                // LOWER
                let s = self.nb_to_string_as_resolved_value(arg_nb);
                return Ok(Value::String(StringRef::Owned(s.to_lowercase())));
            }
            22 => {
                // REPLACE
                let from = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                let to = self.nb_to_string_resolved(self.registers[base + arg_reg + 2]);
                let s = self.nb_to_string_as_resolved_value(arg_nb);
                return Ok(Value::String(StringRef::Owned(s.replace(&from, &to))));
            }
            23 => {
                // SLICE
                let start = self
                    .nb_to_int(self.registers[base + arg_reg + 1])
                    .unwrap_or(0) as usize;
                let end = self
                    .nb_to_int(self.registers[base + arg_reg + 2])
                    .unwrap_or(0) as usize;
                if let Some(hv) = arg_nb.as_heap_ref() {
                    let out = match hv {
                        HeapValue::List(l) => {
                            let end = end.min(l.len());
                            let start = start.min(end);
                            Value::List(Arc::new(l[start..end].to_vec()))
                        }
                        HeapValue::Str(s) => {
                            let chars: Vec<char> = s.chars().collect();
                            let end = end.min(chars.len());
                            let start = start.min(end);
                            Value::String(StringRef::Owned(chars[start..end].iter().collect()))
                        }
                        _ => Value::Null,
                    };
                    return Ok(out);
                }
                if arg_nb.is_int() || arg_nb.is_float() || arg_nb.is_bool() || arg_nb.is_null() {
                    return Ok(Value::Null);
                }
                return Ok(match arg {
                    Value::List(l) => {
                        let end = end.min(l.len());
                        let start = start.min(end);
                        Value::List(Arc::new(l[start..end].to_vec()))
                    }
                    Value::String(sr) => {
                        let s = match sr {
                            StringRef::Owned(s) => s,
                            StringRef::Interned(id) => {
                                self.strings.resolve(id).unwrap_or("").to_string()
                            }
                        };
                        let chars: Vec<char> = s.chars().collect();
                        let end = end.min(chars.len());
                        let start = start.min(end);
                        Value::String(StringRef::Owned(chars[start..end].iter().collect()))
                    }
                    _ => Value::Null,
                });
            }
            25 => {
                // RANGE
                let start = self.nb_to_int(arg_nb).unwrap_or(0);
                let end = self
                    .nb_to_int(self.registers[base + arg_reg + 1])
                    .unwrap_or(0);
                let list: Vec<NbValue> = (start..end).map(NbValue::new_int).collect();
                return Ok(Value::List(Arc::new(list)));
            }
            26 => {
                // ABS
                if arg_nb.is_int() {
                    return Ok(Value::Int(arg_nb.as_int().unwrap_or(0).abs()));
                }
                if arg_nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(arg_nb.0).abs()));
                }
                if arg_nb.is_bool() || arg_nb.is_null() {
                    return Ok(Value::Null);
                }
                if let Some(hv) = arg_nb.as_heap_ref() {
                    return Ok(match hv {
                        HeapValue::BigInt(n) => Value::BigInt(n.abs()),
                        _ => Value::Null,
                    });
                }
                return Ok(match arg {
                    Value::Int(n) => Value::Int(n.abs()),
                    Value::Float(f) => Value::Float(f.abs()),
                    Value::BigInt(ref n) => Value::BigInt(n.abs()),
                    _ => Value::Null,
                });
            }
            27 => {
                // MIN
                let other_nb = self.registers[base + arg_reg + 1];
                if arg_nb.is_int() {
                    let a = arg_nb.as_int().unwrap_or(0);
                    if other_nb.is_int() {
                        return Ok(Value::Int(a.min(other_nb.as_int().unwrap_or(0))));
                    }
                    if other_nb.is_float() {
                        return Ok(Value::Float((a as f64).min(f64::from_bits(other_nb.0))));
                    }
                    return Ok(Value::Int(a));
                }
                if arg_nb.is_float() {
                    let a = f64::from_bits(arg_nb.0);
                    if other_nb.is_float() {
                        return Ok(Value::Float(a.min(f64::from_bits(other_nb.0))));
                    }
                    if other_nb.is_int() {
                        return Ok(Value::Float(a.min(other_nb.as_int().unwrap_or(0) as f64)));
                    }
                    return Ok(Value::Float(a));
                }
                if arg_nb.is_bool() {
                    return Ok(Value::Bool(arg_nb.as_bool().unwrap_or(false)));
                }
                if arg_nb.is_null() {
                    return Ok(Value::Null);
                }
                let arg = self.nb_to_value_deep(arg_nb);
                if let Value::Int(a) = &arg {
                    if other_nb.is_int() {
                        return Ok(Value::Int((*a).min(other_nb.as_int().unwrap_or(0))));
                    }
                    if other_nb.is_float() {
                        return Ok(Value::Float((*a as f64).min(f64::from_bits(other_nb.0))));
                    }
                }
                if let Value::Float(a) = &arg {
                    if other_nb.is_float() {
                        return Ok(Value::Float(a.min(f64::from_bits(other_nb.0))));
                    }
                    if other_nb.is_int() {
                        return Ok(Value::Float(a.min(other_nb.as_int().unwrap_or(0) as f64)));
                    }
                }
                return Ok(arg);
            }
            28 => {
                // MAX
                let other_nb = self.registers[base + arg_reg + 1];
                if arg_nb.is_int() {
                    let a = arg_nb.as_int().unwrap_or(0);
                    if other_nb.is_int() {
                        return Ok(Value::Int(a.max(other_nb.as_int().unwrap_or(0))));
                    }
                    if other_nb.is_float() {
                        return Ok(Value::Float((a as f64).max(f64::from_bits(other_nb.0))));
                    }
                    return Ok(Value::Int(a));
                }
                if arg_nb.is_float() {
                    let a = f64::from_bits(arg_nb.0);
                    if other_nb.is_float() {
                        return Ok(Value::Float(a.max(f64::from_bits(other_nb.0))));
                    }
                    if other_nb.is_int() {
                        return Ok(Value::Float(a.max(other_nb.as_int().unwrap_or(0) as f64)));
                    }
                    return Ok(Value::Float(a));
                }
                if arg_nb.is_bool() {
                    return Ok(Value::Bool(arg_nb.as_bool().unwrap_or(false)));
                }
                if arg_nb.is_null() {
                    return Ok(Value::Null);
                }
                let arg = self.nb_to_value_deep(arg_nb);
                if let Value::Int(a) = &arg {
                    if other_nb.is_int() {
                        return Ok(Value::Int((*a).max(other_nb.as_int().unwrap_or(0))));
                    }
                    if other_nb.is_float() {
                        return Ok(Value::Float((*a as f64).max(f64::from_bits(other_nb.0))));
                    }
                }
                if let Value::Float(a) = &arg {
                    if other_nb.is_float() {
                        return Ok(Value::Float(a.max(f64::from_bits(other_nb.0))));
                    }
                    if other_nb.is_int() {
                        return Ok(Value::Float(a.max(other_nb.as_int().unwrap_or(0) as f64)));
                    }
                }
                return Ok(arg);
            }
            50 => {
                // IS_EMPTY
                if arg_nb.is_null() {
                    return Ok(Value::Bool(true));
                }
                if arg_nb.is_int() || arg_nb.is_float() || arg_nb.is_bool() {
                    return Ok(Value::Bool(false));
                }
                if let Some(hv) = arg_nb.as_heap_ref() {
                    let empty = match hv {
                        HeapValue::List(l) => l.is_empty(),
                        HeapValue::Map(m) => m.is_empty(),
                        HeapValue::Str(s) => s.is_empty(),
                        HeapValue::Set(s) => s.is_empty(),
                        _ => false,
                    };
                    return Ok(Value::Bool(empty));
                }
                return Ok(Value::Bool(false));
            }
            51 => {
                // CHARS
                let s = self.nb_to_string_as_resolved_value(arg_nb);
                let chars: Vec<NbValue> = s
                    .chars()
                    .map(|c| NbValue::new_str(&c.to_string()))
                    .collect();
                return Ok(Value::List(Arc::new(chars)));
            }
            52 => {
                // STARTS_WITH
                let s = self.nb_to_string_as_resolved_value(arg_nb);
                let prefix = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                return Ok(Value::Bool(s.starts_with(&prefix)));
            }
            53 => {
                // ENDS_WITH
                let s = self.nb_to_string_as_resolved_value(arg_nb);
                let suffix = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                return Ok(Value::Bool(s.ends_with(&suffix)));
            }
            54 => {
                // INDEX_OF
                let s = self.nb_to_string_as_resolved_value(arg_nb);
                let needle = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                return Ok(match s.find(&needle) {
                    Some(i) => {
                        let char_idx = s[..i].chars().count();
                        Value::Int(char_idx as i64)
                    }
                    None => Value::Int(-1),
                });
            }
            55 => {
                // PAD_LEFT
                let s = self.nb_to_string_as_resolved_value(arg_nb);
                let len = match self.nb_to_int(self.registers[base + arg_reg + 1]) {
                    Some(n) => n as usize,
                    None => return Ok(Value::Null),
                };
                if s.len() >= len {
                    return Ok(Value::String(StringRef::Owned(s)));
                }
                let pad = " ".repeat(len - s.len());
                return Ok(Value::String(StringRef::Owned(pad + &s)));
            }
            56 => {
                // PAD_RIGHT
                let s = self.nb_to_string_as_resolved_value(arg_nb);
                let len = match self.nb_to_int(self.registers[base + arg_reg + 1]) {
                    Some(n) => n as usize,
                    None => return Ok(Value::Null),
                };
                if s.len() >= len {
                    return Ok(Value::String(StringRef::Owned(s)));
                }
                let pad = " ".repeat(len - s.len());
                return Ok(Value::String(StringRef::Owned(s + &pad)));
            }
            57 => {
                // ROUND
                if arg_nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(arg_nb.0).round()));
                }
                if arg_nb.is_int() {
                    return Ok(Value::Int(arg_nb.as_int().unwrap_or(0)));
                }
                if arg_nb.is_bool() || arg_nb.is_null() {
                    return Ok(Value::Null);
                }
                if arg_nb.as_heap_ref().is_some() {
                    return Ok(Value::Null);
                }
                return Ok(match arg {
                    Value::Float(f) => Value::Float(f.round()),
                    Value::Int(n) => Value::Int(n),
                    _ => Value::Null,
                });
            }
            58 => {
                // CEIL
                if arg_nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(arg_nb.0).ceil()));
                }
                if arg_nb.is_int() {
                    return Ok(Value::Int(arg_nb.as_int().unwrap_or(0)));
                }
                if arg_nb.is_bool() || arg_nb.is_null() {
                    return Ok(Value::Null);
                }
                if arg_nb.as_heap_ref().is_some() {
                    return Ok(Value::Null);
                }
                return Ok(match arg {
                    Value::Float(f) => Value::Float(f.ceil()),
                    Value::Int(n) => Value::Int(n),
                    _ => Value::Null,
                });
            }
            59 => {
                // FLOOR
                if arg_nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(arg_nb.0).floor()));
                }
                if arg_nb.is_int() {
                    return Ok(Value::Int(arg_nb.as_int().unwrap_or(0)));
                }
                if arg_nb.is_bool() || arg_nb.is_null() {
                    return Ok(Value::Null);
                }
                if arg_nb.as_heap_ref().is_some() {
                    return Ok(Value::Null);
                }
                return Ok(match arg {
                    Value::Float(f) => Value::Float(f.floor()),
                    Value::Int(n) => Value::Int(n),
                    _ => Value::Null,
                });
            }
            60 => {
                // SQRT
                if arg_nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(arg_nb.0).sqrt()));
                }
                if arg_nb.is_int() {
                    return Ok(Value::Float((arg_nb.as_int().unwrap_or(0) as f64).sqrt()));
                }
                if arg_nb.is_bool() || arg_nb.is_null() {
                    return Ok(Value::Null);
                }
                if let Some(hv) = arg_nb.as_heap_ref() {
                    return Ok(match hv {
                        HeapValue::BigInt(n) => {
                            Value::Float(n.to_f64().unwrap_or(f64::INFINITY).sqrt())
                        }
                        _ => Value::Null,
                    });
                }
                return Ok(match arg {
                    Value::Float(f) => Value::Float(f.sqrt()),
                    Value::Int(n) => Value::Float((n as f64).sqrt()),
                    _ => Value::Null,
                });
            }
            61 => {
                // POW
                let exp_nb = self.registers[base + arg_reg + 1];
                let int_pow = |x: i64, y: i64| {
                    if y >= 0 {
                        if let Ok(y_u32) = u32::try_from(y) {
                            if let Some(res) = x.checked_pow(y_u32) {
                                Value::Int(res)
                            } else {
                                Value::BigInt(BigInt::from(x).pow(y_u32))
                            }
                        } else {
                            Value::Null
                        }
                    } else {
                        Value::Float((x as f64).powf(y as f64))
                    }
                };
                if arg_nb.is_int() {
                    if exp_nb.is_int() {
                        return Ok(int_pow(
                            arg_nb.as_int().unwrap_or(0),
                            exp_nb.as_int().unwrap_or(0),
                        ));
                    }
                    return Ok(Value::Null);
                }
                if arg_nb.is_float() {
                    if exp_nb.is_float() {
                        return Ok(Value::Float(
                            f64::from_bits(arg_nb.0).powf(f64::from_bits(exp_nb.0)),
                        ));
                    }
                    return Ok(Value::Null);
                }
                let arg = self.nb_to_value_deep(arg_nb);
                let exp = if exp_nb.is_int() {
                    Value::Int(exp_nb.as_int().unwrap_or(0))
                } else if exp_nb.is_float() {
                    Value::Float(f64::from_bits(exp_nb.0))
                } else {
                    self.nb_to_value_deep(exp_nb)
                };
                return Ok(match (arg, exp) {
                    (Value::Int(x), Value::Int(y)) => int_pow(x, y),
                    (Value::Float(x), Value::Float(y)) => Value::Float(x.powf(y)),
                    _ => Value::Null,
                });
            }
            62 => {
                // LOG
                if arg_nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(arg_nb.0).ln()));
                }
                if arg_nb.is_int() {
                    return Ok(Value::Float((arg_nb.as_int().unwrap_or(0) as f64).ln()));
                }
                if arg_nb.is_bool() || arg_nb.is_null() {
                    return Ok(Value::Null);
                }
                return Ok(match arg {
                    Value::Float(f) => Value::Float(f.ln()),
                    Value::Int(n) => Value::Float((n as f64).ln()),
                    _ => Value::Null,
                });
            }
            63 => {
                // SIN
                if arg_nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(arg_nb.0).sin()));
                }
                if arg_nb.is_int() {
                    return Ok(Value::Float((arg_nb.as_int().unwrap_or(0) as f64).sin()));
                }
                if arg_nb.is_bool() || arg_nb.is_null() {
                    return Ok(Value::Null);
                }
                let arg = self.nb_to_value_deep(arg_nb);
                return Ok(match arg {
                    Value::Float(f) => Value::Float(f.sin()),
                    Value::Int(n) => Value::Float((n as f64).sin()),
                    _ => Value::Null,
                });
            }
            64 => {
                // COS
                if arg_nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(arg_nb.0).cos()));
                }
                if arg_nb.is_int() {
                    return Ok(Value::Float((arg_nb.as_int().unwrap_or(0) as f64).cos()));
                }
                if arg_nb.is_bool() || arg_nb.is_null() {
                    return Ok(Value::Null);
                }
                let arg = self.nb_to_value_deep(arg_nb);
                return Ok(match arg {
                    Value::Float(f) => Value::Float(f.cos()),
                    Value::Int(n) => Value::Float((n as f64).cos()),
                    _ => Value::Null,
                });
            }
            65 => {
                // CLAMP
                let lo_nb = self.registers[base + arg_reg + 1];
                let hi_nb = self.registers[base + arg_reg + 2];
                if arg_nb.is_int() {
                    let v = arg_nb.as_int().unwrap_or(0);
                    if lo_nb.is_int() && hi_nb.is_int() {
                        let l = lo_nb.as_int().unwrap_or(0);
                        let h = hi_nb.as_int().unwrap_or(0);
                        return Ok(Value::Int(v.max(l).min(h)));
                    }
                    let lo = if lo_nb.is_int() {
                        Value::Int(lo_nb.as_int().unwrap_or(0))
                    } else if lo_nb.is_float() {
                        Value::Float(f64::from_bits(lo_nb.0))
                    } else if lo_nb.is_bool() {
                        Value::Bool(lo_nb.as_bool().unwrap_or(false))
                    } else if lo_nb.is_null() {
                        Value::Null
                    } else {
                        self.nb_to_value_deep(lo_nb)
                    };
                    let hi = if hi_nb.is_int() {
                        Value::Int(hi_nb.as_int().unwrap_or(0))
                    } else if hi_nb.is_float() {
                        Value::Float(f64::from_bits(hi_nb.0))
                    } else if hi_nb.is_bool() {
                        Value::Bool(hi_nb.as_bool().unwrap_or(false))
                    } else if hi_nb.is_null() {
                        Value::Null
                    } else {
                        self.nb_to_value_deep(hi_nb)
                    };
                    return Ok(match (lo, hi) {
                        (Value::Int(l), Value::Int(h)) => Value::Int(v.max(l).min(h)),
                        _ => Value::Int(v),
                    });
                }
                if arg_nb.is_float() {
                    let v = f64::from_bits(arg_nb.0);
                    if lo_nb.is_float() && hi_nb.is_float() {
                        let l = f64::from_bits(lo_nb.0);
                        let h = f64::from_bits(hi_nb.0);
                        return Ok(Value::Float(v.max(l).min(h)));
                    }
                    let lo = if lo_nb.is_float() {
                        Value::Float(f64::from_bits(lo_nb.0))
                    } else if lo_nb.is_int() {
                        Value::Int(lo_nb.as_int().unwrap_or(0))
                    } else if lo_nb.is_bool() {
                        Value::Bool(lo_nb.as_bool().unwrap_or(false))
                    } else if lo_nb.is_null() {
                        Value::Null
                    } else {
                        self.nb_to_value_deep(lo_nb)
                    };
                    let hi = if hi_nb.is_float() {
                        Value::Float(f64::from_bits(hi_nb.0))
                    } else if hi_nb.is_int() {
                        Value::Int(hi_nb.as_int().unwrap_or(0))
                    } else if hi_nb.is_bool() {
                        Value::Bool(hi_nb.as_bool().unwrap_or(false))
                    } else if hi_nb.is_null() {
                        Value::Null
                    } else {
                        self.nb_to_value_deep(hi_nb)
                    };
                    return Ok(match (lo, hi) {
                        (Value::Float(l), Value::Float(h)) => Value::Float(v.max(l).min(h)),
                        _ => Value::Float(v),
                    });
                }
                if arg_nb.is_bool() {
                    return Ok(Value::Bool(arg_nb.as_bool().unwrap_or(false)));
                }
                if arg_nb.is_null() {
                    return Ok(Value::Null);
                }
                let arg = self.nb_to_value_deep(arg_nb);
                let lo = self.nb_to_value_deep(lo_nb);
                let hi = self.nb_to_value_deep(hi_nb);
                return Ok(match (arg, lo, hi) {
                    (Value::Int(v), Value::Int(l), Value::Int(h)) => Value::Int(v.max(l).min(h)),
                    (Value::Float(v), Value::Float(l), Value::Float(h)) => {
                        Value::Float(v.max(l).min(h))
                    }
                    (v, _, _) => v,
                });
            }
            138 => {
                // TAN
                if arg_nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(arg_nb.0).tan()));
                }
                if arg_nb.is_int() {
                    return Ok(Value::Float((arg_nb.as_int().unwrap_or(0) as f64).tan()));
                }
                if arg_nb.is_bool() || arg_nb.is_null() {
                    return Ok(Value::Null);
                }
                let arg = self.nb_to_value_deep(arg_nb);
                return Ok(match arg {
                    Value::Float(f) => Value::Float(f.tan()),
                    Value::Int(n) => Value::Float((n as f64).tan()),
                    _ => Value::Null,
                });
            }
            139 => {
                // TRUNC
                if arg_nb.is_float() {
                    return Ok(Value::Float(f64::from_bits(arg_nb.0).trunc()));
                }
                if arg_nb.is_int() {
                    return Ok(Value::Int(arg_nb.as_int().unwrap_or(0)));
                }
                if arg_nb.is_bool() || arg_nb.is_null() {
                    return Ok(Value::Null);
                }
                let arg = self.nb_to_value_deep(arg_nb);
                return Ok(match arg {
                    Value::Float(f) => Value::Float(f.trunc()),
                    Value::Int(n) => Value::Int(n),
                    _ => Value::Null,
                });
            }
            _ => {}
        }
        if arg_nb.as_heap_ref().is_some() {
            let arg_ref = self.nb_to_value_deep(arg_nb);
            match func_id {
                4 => {
                    // DIFF
                    let other = self.nb_to_value_deep(self.registers[base + arg_reg + 1]);
                    return Ok(self.diff_values(&arg_ref, &other));
                }
                5 => {
                    // PATCH
                    let patches = self.registers[base + arg_reg + 1];
                    let arg_nb = self.nb_from_value(arg_ref.clone());
                    let patched = self.patch_value(arg_nb, patches);
                    return Ok(self.nb_to_value_deep(patched));
                }
                6 => {
                    // REDACT
                    let fields = self.registers[base + arg_reg + 1];
                    let arg_nb = self.nb_from_value(arg_ref.clone());
                    let redacted = self.redact_value(arg_nb, fields);
                    return Ok(self.nb_to_value_deep(redacted));
                }
                7 => {
                    // VALIDATE
                    let nargs = if arg_reg == 0 { 0 } else { 1 };
                    if nargs < 1 {
                        return Ok(Value::Bool(!matches!(arg_ref, Value::Null)));
                    }
                    let schema_val = self.nb_to_value_deep(self.registers[base + arg_reg + 1]);
                    return Ok(Value::Bool(validate_value_against_schema(
                        &arg_ref,
                        &schema_val,
                        &self.strings,
                    )));
                }
                35 => {
                    // ZIP
                    let b_list = self.nb_to_value_deep(self.registers[base + arg_reg + 1]);
                    if let (Value::List(la), Value::List(lb)) = (&arg_ref, &b_list) {
                        let result: Vec<NbValue> = la
                            .iter()
                            .zip(lb.iter())
                            .map(|(x, y)| NbValue::new_tuple(vec![*x, *y]))
                            .collect();
                        return Ok(Value::List(Arc::new(result)));
                    }
                    return Ok(Value::List(Arc::new(Vec::new())));
                }
                36 => {
                    // ENUMERATE
                    if let Value::List(l) = &arg_ref {
                        let result: Vec<NbValue> = l
                            .iter()
                            .enumerate()
                            .map(|(i, v)| NbValue::new_tuple(vec![NbValue::new_int(i as i64), *v]))
                            .collect();
                        return Ok(Value::List(Arc::new(result)));
                    }
                    return Ok(Value::List(Arc::new(Vec::new())));
                }
                42 => {
                    // CHUNK
                    let size = self
                        .nb_to_int(self.registers[base + arg_reg + 1])
                        .unwrap_or(1) as usize;
                    if let Value::List(l) = &arg_ref {
                        let result: Vec<NbValue> = l
                            .chunks(size.max(1))
                            .map(|chunk| NbValue::new_list(chunk.to_vec()))
                            .collect();
                        return Ok(Value::List(Arc::new(result)));
                    }
                    return Ok(Value::List(Arc::new(Vec::new())));
                }
                43 => {
                    // WINDOW
                    let n = self
                        .nb_to_int(self.registers[base + arg_reg + 1])
                        .unwrap_or(1) as usize;
                    if let Value::List(l) = arg_ref {
                        if n == 0 || n > l.len() {
                            return Ok(Value::List(Arc::new(Vec::new())));
                        }
                        let result: Vec<NbValue> = l
                            .windows(n)
                            .map(|w| NbValue::new_list(w.to_vec()))
                            .collect();
                        return Ok(Value::List(Arc::new(result)));
                    }
                    return Ok(Value::List(Arc::new(Vec::new())));
                }
                48 => {
                    // FIRST
                    return Ok(match &arg_ref {
                        Value::List(l) => l
                            .first()
                            .map(|v| self.nb_to_value_deep(*v))
                            .unwrap_or(Value::Null),
                        _ => Value::Null,
                    });
                }
                49 => {
                    // LAST
                    return Ok(match &arg_ref {
                        Value::List(l) => l
                            .last()
                            .map(|v| self.nb_to_value_deep(*v))
                            .unwrap_or(Value::Null),
                        _ => Value::Null,
                    });
                }
                50 => {
                    // IS_EMPTY
                    let empty = match &arg_ref {
                        Value::List(l) => l.is_empty(),
                        Value::Map(m) => m.is_empty(),
                        Value::String(StringRef::Owned(s)) => s.is_empty(),
                        Value::String(StringRef::Interned(id)) => {
                            self.strings.resolve(*id).unwrap_or("").is_empty()
                        }
                        Value::Set(s) => s.is_empty(),
                        Value::Null => true,
                        _ => false,
                    };
                    return Ok(Value::Bool(empty));
                }
                55 => {
                    // PAD_LEFT
                    let s = arg_ref.as_string_resolved(&self.strings);
                    let len = match self.nb_to_int(self.registers[base + arg_reg + 1]) {
                        Some(n) => n as usize,
                        None => return Ok(Value::Null),
                    };
                    if s.len() >= len {
                        return Ok(Value::String(StringRef::Owned(s)));
                    }
                    let pad = " ".repeat(len - s.len());
                    return Ok(Value::String(StringRef::Owned(pad + &s)));
                }
                56 => {
                    // PAD_RIGHT
                    let s = arg_ref.as_string_resolved(&self.strings);
                    let len = match self.nb_to_int(self.registers[base + arg_reg + 1]) {
                        Some(n) => n as usize,
                        None => return Ok(Value::Null),
                    };
                    if s.len() >= len {
                        return Ok(Value::String(StringRef::Owned(s)));
                    }
                    let pad = " ".repeat(len - s.len());
                    return Ok(Value::String(StringRef::Owned(s + &pad)));
                }
                70 => {
                    // HAS_KEY
                    let key = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                    return Ok(Value::Bool(match &arg_ref {
                        Value::Map(m) => m.contains_key(&key),
                        Value::Record(r) => r.fields.contains_key(&key),
                        _ => false,
                    }));
                }
                106 => {
                    // STRING_CONCAT
                    let other_nb = self.registers[base + arg_reg + 1];
                    let other = self.nb_to_value_deep(other_nb);
                    return Ok(match (arg_ref, other) {
                        (Value::String(StringRef::Owned(left)), rhs) => {
                            let rhs_str = rhs.as_string_resolved(&self.strings);
                            let mut left = left.clone();
                            left.push_str(&rhs_str);
                            Value::String(StringRef::Owned(left))
                        }
                        (left, rhs) => {
                            let left_str = left.as_string_resolved(&self.strings);
                            let rhs_str = rhs.as_string_resolved(&self.strings);
                            Value::String(StringRef::Owned(format!("{}{}", left_str, rhs_str)))
                        }
                    });
                }
                107 => {
                    // HTTP_GET
                    let url = arg_ref.as_string_resolved(&self.strings);
                    return Ok(http_builtin_get(&url));
                }
                108 => {
                    // HTTP_POST
                    let url = arg_ref.as_string_resolved(&self.strings);
                    let body = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                    return Ok(http_builtin_post(&url, &body));
                }
                109 => {
                    // HTTP_PUT
                    let url = arg_ref.as_string_resolved(&self.strings);
                    let body = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                    return Ok(http_builtin_put(&url, &body));
                }
                110 => {
                    // HTTP_DELETE
                    let url = arg_ref.as_string_resolved(&self.strings);
                    return Ok(http_builtin_delete(&url));
                }
                111 => {
                    // HTTP_REQUEST
                    let method = arg_ref.as_string_resolved(&self.strings);
                    let url = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                    let body = self.nb_to_string_resolved(self.registers[base + arg_reg + 2]);
                    let headers = {
                        let headers_val = self.nb_to_value_deep(self.registers[base + arg_reg + 3]);
                        extract_headers_map(&headers_val)
                    };
                    return Ok(http_builtin_request(&method, &url, &body, &headers));
                }
                _ => {}
            }
        }
        let arg = self.nb_to_value_deep(arg_nb);
        match func_id {
            0 => {
                // LENGTH
                Ok(match arg {
                    Value::String(StringRef::Owned(s)) => Value::Int(s.chars().count() as i64),
                    Value::String(StringRef::Interned(id)) => {
                        let s = self.strings.resolve(id).unwrap_or("");
                        Value::Int(s.chars().count() as i64)
                    }
                    Value::List(l) => Value::Int(l.len() as i64),
                    Value::Map(m) => Value::Int(m.len() as i64),
                    Value::Tuple(t) => Value::Int(t.len() as i64),
                    Value::Set(s) => Value::Int(s.len() as i64),
                    Value::Bytes(b) => Value::Int(b.len() as i64),
                    _ => Value::Int(0),
                })
            }
            1 => {
                // COUNT
                Ok(match arg {
                    Value::List(l) => Value::Int(l.len() as i64),
                    Value::Map(m) => Value::Int(m.len() as i64),
                    Value::String(StringRef::Owned(s)) => Value::Int(s.chars().count() as i64),
                    _ => Value::Int(0),
                })
            }
            2 => {
                // MATCHES
                Ok(Value::Bool(arg.is_truthy()))
            }
            3 => {
                // HASH
                use sha2::{Digest, Sha256};
                let s = arg.display_pretty();
                let hash = format!("{:x}", Sha256::digest(s.as_bytes()));
                Ok(Value::String(StringRef::Owned(format!("sha256:{}", hash))))
            }
            4 => {
                // DIFF
                let other = self.nb_to_value_deep(self.registers[base + arg_reg + 1]);
                Ok(self.diff_values(&arg, &other))
            }
            5 => {
                // PATCH
                let patches = self.registers[base + arg_reg + 1];
                let arg_nb = self.nb_from_value(arg);
                Ok(self.nb_to_value_deep(self.patch_value(arg_nb, patches)))
            }
            6 => {
                // REDACT
                let fields = self.registers[base + arg_reg + 1];
                let arg_nb = self.nb_from_value(arg);
                Ok(self.nb_to_value_deep(self.redact_value(arg_nb, fields)))
            }
            7 => {
                // VALIDATE
                let nargs = if arg_reg == 0 { 0 } else { 1 }; // Simplified arity detection for intrinsic
                if nargs < 1 {
                    Ok(Value::Bool(!matches!(arg, Value::Null)))
                } else {
                    let schema_val = self.nb_to_value_deep(self.registers[base + arg_reg + 1]);
                    Ok(Value::Bool(validate_value_against_schema(
                        &arg,
                        &schema_val,
                        &self.strings,
                    )))
                }
            }
            8 => {
                // TRACEREF
                Ok(Value::TraceRef(self.next_trace_ref()))
            }
            35 => {
                // ZIP
                let b_list = self.nb_to_value_deep(self.registers[base + arg_reg + 1]);
                if let (Value::List(la), Value::List(lb)) = (&arg, &b_list) {
                    let result: Vec<NbValue> = la
                        .iter()
                        .zip(lb.iter())
                        .map(|(x, y)| NbValue::new_tuple(vec![*x, *y]))
                        .collect();
                    Ok(Value::List(Arc::new(result)))
                } else {
                    Ok(Value::List(Arc::new(Vec::new())))
                }
            }
            36 => {
                // ENUMERATE
                if let Value::List(l) = &arg {
                    let result: Vec<NbValue> = l
                        .iter()
                        .enumerate()
                        .map(|(i, v)| NbValue::new_tuple(vec![NbValue::new_int(i as i64), *v]))
                        .collect();
                    Ok(Value::List(Arc::new(result)))
                } else {
                    Ok(Value::List(Arc::new(Vec::new())))
                }
            }
            42 => {
                // CHUNK
                let size = self
                    .nb_to_int(self.registers[base + arg_reg + 1])
                    .unwrap_or(1) as usize;
                if let Value::List(l) = &arg {
                    let result: Vec<NbValue> = l
                        .chunks(size.max(1))
                        .map(|chunk| NbValue::new_list(chunk.to_vec()))
                        .collect();
                    Ok(Value::List(Arc::new(result)))
                } else {
                    Ok(Value::List(Arc::new(Vec::new())))
                }
            }
            43 => {
                // WINDOW
                let n = self
                    .nb_to_int(self.registers[base + arg_reg + 1])
                    .unwrap_or(1) as usize;
                if let Value::List(l) = &arg {
                    if n == 0 || n > l.len() {
                        Ok(Value::List(Arc::new(Vec::new())))
                    } else {
                        let result: Vec<NbValue> = l
                            .windows(n)
                            .map(|w| NbValue::new_list(w.to_vec()))
                            .collect();
                        Ok(Value::List(Arc::new(result)))
                    }
                } else {
                    Ok(Value::List(Arc::new(Vec::new())))
                }
            }
            46 => {
                // TAKE
                let n = self
                    .nb_to_int(self.registers[base + arg_reg + 1])
                    .unwrap_or(0) as usize;
                if let Value::List(l) = &arg {
                    Ok(Value::List(Arc::new(l.iter().take(n).cloned().collect())))
                } else {
                    Ok(arg)
                }
            }
            47 => {
                // DROP
                let n = self
                    .nb_to_int(self.registers[base + arg_reg + 1])
                    .unwrap_or(0) as usize;
                if let Value::List(l) = &arg {
                    Ok(Value::List(Arc::new(l.iter().skip(n).cloned().collect())))
                } else {
                    Ok(arg)
                }
            }
            51 => {
                // CHARS
                let s = arg.as_string_resolved(&self.strings);
                let chars: Vec<NbValue> = s
                    .chars()
                    .map(|c| NbValue::new_str(&c.to_string()))
                    .collect();
                Ok(Value::List(Arc::new(chars)))
            }
            52 => {
                // STARTS_WITH
                let s = arg.as_string_resolved(&self.strings);
                let prefix = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                Ok(Value::Bool(s.starts_with(&prefix)))
            }
            53 => {
                // ENDS_WITH
                let s = arg.as_string_resolved(&self.strings);
                let suffix = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                Ok(Value::Bool(s.ends_with(&suffix)))
            }
            54 => {
                // INDEX_OF
                let s = arg.as_string_resolved(&self.strings);
                let needle = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                Ok(match s.find(&needle) {
                    Some(i) => {
                        let char_idx = s[..i].chars().count();
                        Value::Int(char_idx as i64)
                    }
                    None => Value::Int(-1),
                })
            }
            66 => {
                // CLONE
                Ok(arg.clone())
            }
            69 => {
                // TO_SET
                if let Value::List(l) = arg {
                    // SHIM: delete when P2 complete
                    Ok(Value::new_set_from_vec(
                        l.iter().map(|v| self.nb_to_value_deep(*v)).collect(),
                    ))
                } else {
                    Ok(Value::new_set_from_vec(vec![]))
                }
            }
            70 => {
                // HAS_KEY
                let key = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                Ok(Value::Bool(match arg {
                    Value::Map(m) => m.contains_key(&key),
                    Value::Record(r) => r.fields.contains_key(&key),
                    _ => false,
                }))
            }
            55 => {
                // PAD_LEFT
                let s = arg.as_string_resolved(&self.strings);
                let len = match self.nb_to_int(self.registers[base + arg_reg + 1]) {
                    Some(n) => n as usize,
                    None => return Ok(Value::Null),
                };
                if s.len() >= len {
                    return Ok(Value::String(StringRef::Owned(s)));
                }
                let pad = " ".repeat(len - s.len());
                Ok(Value::String(StringRef::Owned(pad + &s)))
            }
            56 => {
                // PAD_RIGHT
                let s = arg.as_string_resolved(&self.strings);
                let len = match self.nb_to_int(self.registers[base + arg_reg + 1]) {
                    Some(n) => n as usize,
                    None => return Ok(Value::Null),
                };
                if s.len() >= len {
                    return Ok(Value::String(StringRef::Owned(s)));
                }
                let pad = " ".repeat(len - s.len());
                Ok(Value::String(StringRef::Owned(s + &pad)))
            }
            57 => {
                // ROUND
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.round()),
                    Value::Int(n) => Value::Int(n),
                    _ => Value::Null,
                })
            }
            58 => {
                // CEIL
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.ceil()),
                    Value::Int(n) => Value::Int(n),
                    _ => Value::Null,
                })
            }
            59 => {
                // FLOOR
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.floor()),
                    Value::Int(n) => Value::Int(n),
                    _ => Value::Null,
                })
            }
            60 => {
                // SQRT
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.sqrt()),
                    Value::Int(n) => Value::Float((n as f64).sqrt()),
                    _ => Value::Null,
                })
            }
            61 => {
                // POW
                let exp_nb = self.registers[base + arg_reg + 1];
                let exp = if exp_nb.is_int() {
                    Value::Int(exp_nb.as_int().unwrap_or(0))
                } else if exp_nb.is_float() {
                    Value::Float(f64::from_bits(exp_nb.0))
                } else {
                    self.nb_to_value_deep(exp_nb)
                };
                Ok(match (arg, exp) {
                    (Value::Int(x), Value::Int(y)) => {
                        if y >= 0 {
                            if let Ok(y_u32) = u32::try_from(y) {
                                if let Some(res) = x.checked_pow(y_u32) {
                                    Value::Int(res)
                                } else {
                                    Value::BigInt(BigInt::from(x).pow(y_u32))
                                }
                            } else {
                                Value::Null
                            }
                        } else {
                            Value::Float((x as f64).powf(y as f64))
                        }
                    }
                    (Value::Float(x), Value::Float(y)) => Value::Float(x.powf(y)),
                    _ => Value::Null,
                })
            }
            62 => {
                // LOG
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.ln()),
                    Value::Int(n) => Value::Float((n as f64).ln()),
                    _ => Value::Null,
                })
            }
            63 => {
                // SIN
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.sin()),
                    Value::Int(n) => Value::Float((n as f64).sin()),
                    _ => Value::Null,
                })
            }
            64 => {
                // COS
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.cos()),
                    Value::Int(n) => Value::Float((n as f64).cos()),
                    _ => Value::Null,
                })
            }
            65 => {
                // CLAMP
                let lo_nb = self.registers[base + arg_reg + 1];
                let hi_nb = self.registers[base + arg_reg + 2];
                // NbValue fast-path for int clamp
                if let (Value::Int(v), true, true) = (&arg, lo_nb.is_int(), hi_nb.is_int()) {
                    let l = lo_nb.as_int().unwrap_or(0);
                    let h = hi_nb.as_int().unwrap_or(0);
                    return Ok(Value::Int((*v).max(l).min(h)));
                }
                let lo = self.nb_to_value_deep(lo_nb);
                let hi = self.nb_to_value_deep(hi_nb);
                Ok(match (arg, lo, hi) {
                    (Value::Int(v), Value::Int(l), Value::Int(h)) => Value::Int(v.max(l).min(h)),
                    (Value::Float(v), Value::Float(l), Value::Float(h)) => {
                        Value::Float(v.max(l).min(h))
                    }
                    (v, _, _) => v,
                })
            }
            106 => {
                // STRING_CONCAT
                let other = self.nb_to_value_deep(self.registers[base + arg_reg + 1]);
                return Ok(match (arg, other) {
                    (Value::String(StringRef::Owned(mut left)), rhs) => {
                        let rhs_str = rhs.as_string_resolved(&self.strings);
                        left.push_str(&rhs_str);
                        Value::String(StringRef::Owned(left))
                    }
                    (left, rhs) => {
                        let left_str = left.as_string_resolved(&self.strings);
                        let rhs_str = rhs.as_string_resolved(&self.strings);
                        Value::String(StringRef::Owned(format!("{}{}", left_str, rhs_str)))
                    }
                });
            }
            107 => {
                // HTTP_GET
                let url = arg.as_string_resolved(&self.strings);
                return Ok(http_builtin_get(&url));
            }
            108 => {
                // HTTP_POST
                let url = arg.as_string_resolved(&self.strings);
                let body = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                return Ok(http_builtin_post(&url, &body));
            }
            109 => {
                // HTTP_PUT
                let url = arg.as_string_resolved(&self.strings);
                let body = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                return Ok(http_builtin_put(&url, &body));
            }
            110 => {
                // HTTP_DELETE
                let url = arg.as_string_resolved(&self.strings);
                return Ok(http_builtin_delete(&url));
            }
            111 => {
                // HTTP_REQUEST
                let method = arg.as_string_resolved(&self.strings);
                let url = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                let body = self.nb_to_string_resolved(self.registers[base + arg_reg + 2]);
                let headers = {
                    let headers_val = self.nb_to_value_deep(self.registers[base + arg_reg + 3]);
                    extract_headers_map(&headers_val)
                };
                return Ok(http_builtin_request(&method, &url, &body, &headers));
            }
            // 106 = StringConcat, 107 = HttpGet — handled in the fast-path match above
            138 => {
                // TAN
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.tan()),
                    Value::Int(n) => Value::Float((n as f64).tan()),
                    _ => Value::Null,
                })
            }
            139 => {
                // TRUNC
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.trunc()),
                    Value::Int(n) => Value::Int(n),
                    _ => Value::Null,
                })
            }

            // ── Stdlib intrinsics (IDs 9–50+) ─────────────────────────────
            // These map to IntrinsicId enum values in lumen-core/src/lir.rs.
            9 => {
                // PRINT
                let output = arg.display_pretty();
                println!("{}", output);
                self.output.push(output);
                Ok(Value::Null)
            }
            10 => {
                // TO_STRING
                Ok(Value::String(StringRef::Owned(arg.display_pretty())))
            }
            11 => {
                // TO_INT
                Ok(match arg {
                    Value::Int(n) => Value::Int(n),
                    Value::Float(f) => Value::Int(f as i64),
                    Value::String(sr) => {
                        let s = match sr {
                            StringRef::Owned(s) => s,
                            StringRef::Interned(id) => {
                                self.strings.resolve(id).unwrap_or("").to_string()
                            }
                        };
                        s.parse::<i64>().map(Value::Int).unwrap_or(Value::Null)
                    }
                    Value::Bool(b) => Value::Int(if b { 1 } else { 0 }),
                    _ => Value::Null,
                })
            }
            12 => {
                // TO_FLOAT
                Ok(match arg {
                    Value::Float(f) => Value::Float(f),
                    Value::Int(n) => Value::Float(n as f64),
                    Value::String(sr) => {
                        let s = match sr {
                            StringRef::Owned(s) => s,
                            StringRef::Interned(id) => {
                                self.strings.resolve(id).unwrap_or("").to_string()
                            }
                        };
                        s.parse::<f64>().map(Value::Float).unwrap_or(Value::Null)
                    }
                    _ => Value::Null,
                })
            }
            13 => {
                // TYPE_OF
                Ok(Value::String(StringRef::Owned(arg.type_name().to_string())))
            }
            14 => {
                // KEYS
                Ok(match arg {
                    Value::Map(m) => {
                        Value::List(Arc::new(m.keys().map(|k| NbValue::new_str(k)).collect()))
                    }
                    Value::Record(r) => Value::List(Arc::new(
                        r.fields.keys().map(|k| NbValue::new_str(k)).collect(),
                    )),
                    _ => Value::List(Arc::new(Vec::new())),
                })
            }
            15 => {
                // VALUES
                Ok(match arg {
                    Value::Map(m) => Value::List(Arc::new(
                        m.values()
                            .cloned()
                            .map(|v| NbValue::new_heap(HeapValue::from(v)))
                            .collect(),
                    )),
                    Value::Record(r) => Value::List(Arc::new(
                        r.fields
                            .values()
                            .cloned()
                            .map(|v| NbValue::new_heap(HeapValue::from(v)))
                            .collect(),
                    )),
                    _ => Value::List(Arc::new(Vec::new())),
                })
            }
            16 => {
                // CONTAINS
                let needle_nb = self.registers[base + arg_reg + 1];
                // Fast-path: int needle (common in numeric code)
                let result = match arg {
                    Value::List(l) => l.iter().any(|v| *v == needle_nb),
                    Value::Set(s) => {
                        let needle_value = self.nb_to_value_deep(needle_nb);
                        s.iter().any(|v| v == &needle_value)
                    }
                    Value::Map(m) => {
                        let needle_str = self.nb_to_string_resolved(needle_nb);
                        m.contains_key(&needle_str)
                    }
                    Value::String(sr) => {
                        let s = match sr {
                            StringRef::Owned(s) => s,
                            StringRef::Interned(id) => {
                                self.strings.resolve(id).unwrap_or("").to_string()
                            }
                        };
                        let needle_str = self.nb_to_string_resolved(needle_nb);
                        s.contains(&needle_str)
                    }
                    _ => false,
                };
                Ok(Value::Bool(result))
            }
            17 => {
                // JOIN
                let sep = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                Ok(match arg {
                    Value::List(l) => {
                        let joined = l.iter().map(|v| v.display()).collect::<Vec<_>>().join(&sep);
                        Value::String(StringRef::Owned(joined))
                    }
                    _ => Value::String(StringRef::Owned(String::new())),
                })
            }
            18 => {
                // SPLIT
                let sep = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                let s = arg.as_string_resolved(&self.strings);
                let parts: Vec<NbValue> = s.split(&sep).map(NbValue::new_str).collect();
                Ok(Value::List(Arc::new(parts)))
            }
            19 => {
                // TRIM
                let s = arg.as_string_resolved(&self.strings);
                Ok(Value::String(StringRef::Owned(s.trim().to_string())))
            }
            20 => {
                // UPPER
                let s = arg.as_string_resolved(&self.strings);
                Ok(Value::String(StringRef::Owned(s.to_uppercase())))
            }
            21 => {
                // LOWER
                let s = arg.as_string_resolved(&self.strings);
                Ok(Value::String(StringRef::Owned(s.to_lowercase())))
            }
            22 => {
                // REPLACE
                let from = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                let to = self.nb_to_string_resolved(self.registers[base + arg_reg + 2]);
                let s = arg.as_string_resolved(&self.strings);
                Ok(Value::String(StringRef::Owned(s.replace(&from, &to))))
            }
            23 => {
                // SLICE
                let start = self
                    .nb_to_int(self.registers[base + arg_reg + 1])
                    .unwrap_or(0) as usize;
                let end = self
                    .nb_to_int(self.registers[base + arg_reg + 2])
                    .unwrap_or(0) as usize;
                Ok(match arg {
                    Value::List(l) => {
                        let end = end.min(l.len());
                        let start = start.min(end);
                        Value::List(Arc::new(l[start..end].to_vec()))
                    }
                    Value::String(sr) => {
                        let s = match sr {
                            StringRef::Owned(s) => s,
                            StringRef::Interned(id) => {
                                self.strings.resolve(id).unwrap_or("").to_string()
                            }
                        };
                        let chars: Vec<char> = s.chars().collect();
                        let end = end.min(chars.len());
                        let start = start.min(end);
                        Value::String(StringRef::Owned(chars[start..end].iter().collect()))
                    }
                    _ => Value::Null,
                })
            }
            24 => {
                // APPEND
                let list = self.reg_take(base + arg_reg);
                let elem = self.reg_take(base + arg_reg + 1);
                if let Value::List(mut l) = list {
                    Arc::make_mut(&mut l).push(self.nb_from_value(elem));
                    Ok(Value::List(l))
                } else {
                    Ok(Value::List(Arc::new(vec![self.nb_from_value(elem)])))
                }
            }
            25 => {
                // RANGE
                let start = arg.as_int().unwrap_or(0);
                let end = self
                    .nb_to_int(self.registers[base + arg_reg + 1])
                    .unwrap_or(0);
                let list: Vec<NbValue> = (start..end).map(NbValue::new_int).collect();
                Ok(Value::List(Arc::new(list)))
            }
            26 => {
                // ABS
                Ok(match arg {
                    Value::Int(n) => Value::Int(n.abs()),
                    Value::Float(f) => Value::Float(f.abs()),
                    Value::BigInt(ref n) => Value::BigInt(n.abs()),
                    _ => Value::Null,
                })
            }
            27 => {
                // MIN — NbValue fast-path for int/float secondary arg
                let other_nb = self.registers[base + arg_reg + 1];
                if let Value::Int(a) = &arg {
                    if other_nb.is_int() {
                        return Ok(Value::Int((*a).min(other_nb.as_int().unwrap_or(0))));
                    }
                    if other_nb.is_float() {
                        return Ok(Value::Float((*a as f64).min(f64::from_bits(other_nb.0))));
                    }
                }
                if let Value::Float(a) = &arg {
                    if other_nb.is_float() {
                        return Ok(Value::Float(a.min(f64::from_bits(other_nb.0))));
                    }
                    if other_nb.is_int() {
                        return Ok(Value::Float(a.min(other_nb.as_int().unwrap_or(0) as f64)));
                    }
                }
                Ok(arg)
            }
            28 => {
                // MAX — NbValue fast-path for int/float secondary arg
                let other_nb = self.registers[base + arg_reg + 1];
                if let Value::Int(a) = &arg {
                    if other_nb.is_int() {
                        return Ok(Value::Int((*a).max(other_nb.as_int().unwrap_or(0))));
                    }
                    if other_nb.is_float() {
                        return Ok(Value::Float((*a as f64).max(f64::from_bits(other_nb.0))));
                    }
                }
                if let Value::Float(a) = &arg {
                    if other_nb.is_float() {
                        return Ok(Value::Float(a.max(f64::from_bits(other_nb.0))));
                    }
                    if other_nb.is_int() {
                        return Ok(Value::Float(a.max(other_nb.as_int().unwrap_or(0) as f64)));
                    }
                }
                Ok(arg)
            }
            29 => {
                // SORT — ownership-first path.
                // We always consume the intrinsic arg register so unique list values
                // can be sorted in-place (Arc::make_mut without full rebuild). When
                // the value is aliased, Arc::make_mut preserves copy-on-write semantics.
                let arg = self.reg_take(base + arg_reg);
                if let Value::List(mut l) = arg {
                    sort_list_homogeneous(Arc::make_mut(&mut l));
                    Ok(Value::List(l))
                } else {
                    Ok(arg)
                }
            }
            30 => {
                // REVERSE
                let arg = self.reg_take(base + arg_reg);
                if let Value::List(mut l) = arg {
                    Arc::make_mut(&mut l).reverse();
                    Ok(Value::List(l))
                } else {
                    Ok(arg)
                }
            }
            44 => {
                // FLATTEN
                let arg = self.reg_take(base + arg_reg);
                if let Value::List(l) = arg {
                    let mut flat = Vec::new();
                    for item in l.iter() {
                        if let Some(HeapValue::List(inner)) = item.as_heap_ref() {
                            flat.extend(inner.iter().cloned());
                        } else {
                            flat.push(*item);
                        }
                    }
                    Ok(Value::List(Arc::new(flat)))
                } else {
                    Ok(arg)
                }
            }
            45 => {
                // UNIQUE
                let arg = self.reg_take(base + arg_reg);
                if let Value::List(l) = arg {
                    let mut seen = Vec::new();
                    for item in l.iter() {
                        if !seen.contains(item) {
                            seen.push(*item);
                        }
                    }
                    Ok(Value::List(Arc::new(seen)))
                } else {
                    Ok(arg)
                }
            }
            48 => {
                // FIRST
                Ok(match arg {
                    Value::List(l) => l
                        .first()
                        .map(|v| self.nb_to_value_deep(*v))
                        .unwrap_or(Value::Null),
                    _ => Value::Null,
                })
            }
            49 => {
                // LAST
                Ok(match arg {
                    Value::List(l) => l
                        .last()
                        .map(|v| self.nb_to_value_deep(*v))
                        .unwrap_or(Value::Null),
                    _ => Value::Null,
                })
            }
            50 => {
                // IS_EMPTY
                Ok(Value::Bool(match arg {
                    Value::List(l) => l.is_empty(),
                    Value::Map(m) => m.is_empty(),
                    Value::String(StringRef::Owned(s)) => s.is_empty(),
                    Value::String(StringRef::Interned(id)) => {
                        self.strings.resolve(id).unwrap_or("").is_empty()
                    }
                    Value::Set(s) => s.is_empty(),
                    Value::Null => true,
                    _ => false,
                }))
            }
            71 => {
                // MERGE: merge(map1, map2) → map
                let other_nb = self.registers[base + arg_reg + 1];

                if let (Some(HeapValue::Map(map_a)), Some(HeapValue::Map(map_b))) =
                    (arg_nb.as_heap_ref(), other_nb.as_heap_ref())
                {
                    let mut merged = map_a.clone();
                    Arc::make_mut(&mut merged).extend(map_b.iter().map(|(k, v)| (k.clone(), *v)));
                    let converted: BTreeMap<String, Value> = merged
                        .iter()
                        .map(|(k, v)| (k.clone(), self.nb_to_value_deep(*v)))
                        .collect();
                    return Ok(Value::Map(Arc::new(converted)));
                }

                if let (Some(HeapValue::Record(rec_a)), Some(HeapValue::Record(rec_b))) =
                    (arg_nb.as_heap_ref(), other_nb.as_heap_ref())
                {
                    let mut fields = rec_a.fields.clone();
                    fields.extend(rec_b.fields.iter().map(|(k, v)| (k.clone(), *v)));
                    return Ok(Value::Record(Arc::new(lumen_core::values::RecordValue {
                        type_name: rec_a.type_name.to_string(),
                        fields: fields
                            .into_iter()
                            .map(|(k, v)| (k, self.nb_to_value_deep(v)))
                            .collect(),
                    })));
                }

                let other = self.nb_to_value_deep(other_nb);
                Ok(match (arg, other) {
                    (Value::Map(mut m1), Value::Map(m2)) => {
                        let merged = Arc::make_mut(&mut m1);
                        for (k, v) in m2.iter() {
                            merged.insert(k.clone(), v.clone());
                        }
                        Value::Map(m1)
                    }
                    (Value::Record(r1), Value::Record(r2)) => {
                        let mut fields = r1.fields.clone();
                        for (k, v) in &r2.fields {
                            fields.insert(k.clone(), v.clone());
                        }
                        Value::Record(std::sync::Arc::new(lumen_core::values::RecordValue {
                            type_name: r1.type_name.clone(),
                            fields,
                        }))
                    }
                    (first, _) => first,
                })
            }
            _ => Err(VmError::Runtime(format!(
                "unknown intrinsic ID: {}",
                func_id
            ))),
        }
    }

    /// Synchronously call a closure with the given arguments, returning its result.
    /// Used by HOF intrinsics (map, filter, reduce, etc.).
    pub(crate) fn call_closure_sync(
        &mut self,
        closure: &ClosureValue,
        args: &[Value],
    ) -> Result<Value, VmError> {
        if self.frames.len() >= MAX_CALL_DEPTH {
            return Err(VmError::StackOverflow(MAX_CALL_DEPTH));
        }
        let cv = closure.clone();
        let module = self.module.as_ref().ok_or(VmError::NoModule)?;
        if cv.cell_idx >= module.cells.len() {
            return Err(VmError::Runtime(format!(
                "closure cell index {} out of bounds",
                cv.cell_idx
            )));
        }
        let callee_cell = &module.cells[cv.cell_idx];
        let num_regs = callee_cell.registers as usize;
        let params = callee_cell.params.clone();
        let cell_regs = callee_cell.registers;
        let new_base = self.grow_registers(num_regs.max(16));

        // Copy captures
        for (i, cap) in cv.captures.iter().enumerate() {
            self.check_register(i, cell_regs)?;
            self.set_reg(new_base + i, cap.clone());
        }

        // Copy args
        let cap_count = cv.captures.len();
        for (i, arg) in args.iter().enumerate() {
            if cap_count + i < params.len() {
                let dst = params[cap_count + i].register as usize;
                self.check_register(dst, cell_regs)?;
                self.set_reg(new_base + dst, arg.clone());
            }
        }

        self.frames.push(CallFrame {
            cell_idx: cv.cell_idx,
            base_register: new_base,
            ip: 0,
            return_register: new_base,
            future_id: None,
            osr_points: 0,
        });

        self.run_until(self.frames.len().saturating_sub(1))?;
        Ok(self.reg_take(new_base))
    }

    /// FFI trampoline to call a Lumen closure from JIT helper code.
    ///
    /// # Safety
    /// `ctx` must be a valid pointer to a live `VmContext` whose `stack_pool`
    /// points to the owning `VM`.
    #[no_mangle]
    pub extern "C" fn jit_rt_call_closure(
        ctx: *mut VmContext,
        closure_nb: i64,
        args_ptr: *const i64,
        arg_count: i64,
    ) -> i64 {
        if ctx.is_null() {
            return NbValue::NAN_BOX_NULL as i64;
        }
        let vm_ptr = unsafe { (*ctx).stack_pool } as *mut VM;
        if vm_ptr.is_null() {
            return NbValue::NAN_BOX_NULL as i64;
        }
        let vm = unsafe { &mut *vm_ptr };
        let closure_value = vm.nb_to_value_deep(NbValue(closure_nb as u64));
        let closure = match closure_value {
            Value::Closure(closure) => closure,
            _ => return NbValue::NAN_BOX_NULL as i64,
        };
        let argc = arg_count.max(0) as usize;
        let mut args = Vec::with_capacity(argc);
        for i in 0..argc {
            let nb_raw = unsafe { *args_ptr.add(i) };
            let nb = NbValue(nb_raw as u64);
            nb.inc_ref();
            args.push(vm.nb_to_value_deep(nb));
            nb.drop_heap();
        }
        match vm.call_closure_sync(&closure, &args) {
            Ok(result) => {
                let nb = vm.nb_from_value(result);
                nb.0 as i64
            }
            Err(_) => NbValue::NAN_BOX_NULL as i64,
        }
    }
}

pub(crate) mod json_encode {
    use super::JsonEncodeError;
    use lumen_core::heap_value::HeapValue;
    use lumen_core::nb_value::NbValue;

    pub(crate) fn encode_json_compact(value: NbValue) -> Result<String, JsonEncodeError> {
        let mut out = String::new();
        write_value(&mut out, value, 0, false)?;
        Ok(out)
    }

    pub(crate) fn encode_json_pretty(value: NbValue) -> Result<String, JsonEncodeError> {
        let mut out = String::new();
        write_value(&mut out, value, 0, true)?;
        Ok(out)
    }

    fn write_value(
        out: &mut String,
        value: NbValue,
        indent: usize,
        pretty: bool,
    ) -> Result<(), JsonEncodeError> {
        if value.is_null() {
            out.push_str("null");
            return Ok(());
        }
        if let Some(b) = value.as_bool() {
            if b {
                out.push_str("true");
            } else {
                out.push_str("false");
            }
            return Ok(());
        }
        if let Some(i) = value.as_int() {
            out.push_str(&i.to_string());
            return Ok(());
        }
        if let Some(f) = value.as_float() {
            if !f.is_finite() {
                return Err(JsonEncodeError::InvalidType("Float"));
            }
            out.push_str(&format_float(f));
            return Ok(());
        }
        let Some(hv) = value.as_heap_ref() else {
            return Err(JsonEncodeError::InvalidType("Unknown"));
        };
        match hv {
            HeapValue::Str(s) => {
                write_string(out, s);
                Ok(())
            }
            HeapValue::BigInt(n) => {
                out.push_str(&n.to_string());
                Ok(())
            }
            HeapValue::List(list) => write_list(out, list, indent, pretty),
            HeapValue::Tuple(tuple) => write_list(out, tuple, indent, pretty),
            HeapValue::Set(set) => {
                out.push('[');
                if !set.is_empty() {
                    let mut first = true;
                    for item in set.iter() {
                        if !first {
                            out.push(',');
                        }
                        if pretty {
                            out.push('\n');
                            indent_to(out, indent + 2);
                        }
                        write_value(out, *item, indent + 2, pretty)?;
                        first = false;
                    }
                    if pretty {
                        out.push('\n');
                        indent_to(out, indent);
                    }
                }
                out.push(']');
                Ok(())
            }
            HeapValue::Map(map) => write_map(out, map, indent, pretty),
            _ => Err(JsonEncodeError::InvalidType(hv.type_name())),
        }
    }

    fn write_list(
        out: &mut String,
        list: &[NbValue],
        indent: usize,
        pretty: bool,
    ) -> Result<(), JsonEncodeError> {
        out.push('[');
        if !list.is_empty() {
            let mut first = true;
            for item in list.iter() {
                if !first {
                    out.push(',');
                }
                if pretty {
                    out.push('\n');
                    indent_to(out, indent + 2);
                }
                write_value(out, *item, indent + 2, pretty)?;
                first = false;
            }
            if pretty {
                out.push('\n');
                indent_to(out, indent);
            }
        }
        out.push(']');
        Ok(())
    }

    fn write_map(
        out: &mut String,
        map: &std::collections::BTreeMap<String, NbValue>,
        indent: usize,
        pretty: bool,
    ) -> Result<(), JsonEncodeError> {
        out.push('{');
        if !map.is_empty() {
            let mut first = true;
            for (key, value) in map.iter() {
                if !first {
                    out.push(',');
                }
                if pretty {
                    out.push('\n');
                    indent_to(out, indent + 2);
                }
                write_string(out, key);
                out.push(':');
                if pretty {
                    out.push(' ');
                }
                write_value(out, *value, indent + 2, pretty)?;
                first = false;
            }
            if pretty {
                out.push('\n');
                indent_to(out, indent);
            }
        }
        out.push('}');
        Ok(())
    }

    fn indent_to(out: &mut String, indent: usize) {
        for _ in 0..indent {
            out.push(' ');
        }
    }

    fn format_float(value: f64) -> String {
        if value == value.floor() && value.abs() < 1e15 {
            format!("{:.1}", value)
        } else {
            format!("{}", value)
        }
    }

    fn write_string(out: &mut String, value: &str) {
        out.push('"');
        for ch in value.chars() {
            match ch {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{08}' => out.push_str("\\b"),
                '\u{0C}' => out.push_str("\\f"),
                c if (c as u32) <= 0x1F => {
                    use std::fmt::Write;
                    let _ = write!(out, "\\u{:04X}", c as u32);
                }
                c => out.push(c),
            }
        }
        out.push('"');
    }
}

#[derive(Debug)]
pub(crate) enum JsonEncodeError {
    InvalidType(&'static str),
}

impl std::fmt::Display for JsonEncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonEncodeError::InvalidType(ty) => {
                write!(f, "invalid JSON value of type '{ty}'")
            }
        }
    }
}

impl std::error::Error for JsonEncodeError {}

// ── Helper functions for intrinsics ──

// ===========================================================================
// HTTP client builtins (backed by ureq)
// ===========================================================================

/// Build a response map from a successful ureq response.
fn http_response_to_value(resp: ureq::Response) -> Value {
    let status = resp.status() as i64;
    let ok = (200..300).contains(&(status as u16));
    let body = resp.into_string().unwrap_or_default();

    let mut map = BTreeMap::new();
    map.insert("ok".to_string(), Value::Bool(ok));
    map.insert("status".to_string(), Value::Int(status));
    map.insert("body".to_string(), Value::String(StringRef::Owned(body)));
    Value::new_map(map)
}

/// Build an error response map from a ureq error.
fn http_error_to_value(err: ureq::Error) -> Value {
    let mut map = BTreeMap::new();
    map.insert("ok".to_string(), Value::Bool(false));
    match err {
        ureq::Error::Status(code, resp) => {
            map.insert("status".to_string(), Value::Int(code as i64));
            let body = resp.into_string().unwrap_or_default();
            map.insert("body".to_string(), Value::String(StringRef::Owned(body)));
        }
        ureq::Error::Transport(transport) => {
            map.insert("status".to_string(), Value::Int(0));
            map.insert(
                "error".to_string(),
                Value::String(StringRef::Owned(transport.to_string())),
            );
            map.insert(
                "body".to_string(),
                Value::String(StringRef::Owned(String::new())),
            );
        }
    }
    Value::new_map(map)
}

fn http_builtin_get(url: &str) -> Value {
    match ureq::get(url).call() {
        Ok(resp) => http_response_to_value(resp),
        Err(err) => http_error_to_value(err),
    }
}

fn http_builtin_post(url: &str, body: &str) -> Value {
    match ureq::post(url)
        .set("Content-Type", "application/json")
        .send_string(body)
    {
        Ok(resp) => http_response_to_value(resp),
        Err(err) => http_error_to_value(err),
    }
}

fn http_builtin_put(url: &str, body: &str) -> Value {
    match ureq::put(url)
        .set("Content-Type", "application/json")
        .send_string(body)
    {
        Ok(resp) => http_response_to_value(resp),
        Err(err) => http_error_to_value(err),
    }
}

fn http_builtin_delete(url: &str) -> Value {
    match ureq::delete(url).call() {
        Ok(resp) => http_response_to_value(resp),
        Err(err) => http_error_to_value(err),
    }
}

fn http_builtin_request(
    method: &str,
    url: &str,
    body: &str,
    headers: &[(String, String)],
) -> Value {
    let mut req = match method.to_uppercase().as_str() {
        "GET" => ureq::get(url),
        "POST" => ureq::post(url),
        "PUT" => ureq::put(url),
        "DELETE" => ureq::delete(url),
        "PATCH" => ureq::patch(url),
        "HEAD" => ureq::head(url),
        _ => {
            let mut map = BTreeMap::new();
            map.insert("ok".to_string(), Value::Bool(false));
            map.insert("status".to_string(), Value::Int(0));
            map.insert(
                "error".to_string(),
                Value::String(StringRef::Owned(format!(
                    "unsupported HTTP method: {}",
                    method
                ))),
            );
            map.insert(
                "body".to_string(),
                Value::String(StringRef::Owned(String::new())),
            );
            return Value::new_map(map);
        }
    };

    for (name, value) in headers {
        req = req.set(name, value);
    }

    let result = if body.is_empty() {
        req.call()
    } else {
        req.send_string(body)
    };

    match result {
        Ok(resp) => http_response_to_value(resp),
        Err(err) => http_error_to_value(err),
    }
}

/// Extract headers from a Value::Map into a Vec of (name, value) pairs.
fn extract_headers_map(val: &Value) -> Vec<(String, String)> {
    match val {
        Value::Map(m) => m.iter().map(|(k, v)| (k.clone(), v.as_string())).collect(),
        _ => Vec::new(),
    }
}

fn sort_list_homogeneous(items: &mut Vec<NbValue>) {
    if items.len() <= 1 {
        return;
    }
    if items.iter().all(|v| v.is_int()) {
        items
            .sort_unstable_by(|lhs, rhs| lhs.as_int().unwrap_or(0).cmp(&rhs.as_int().unwrap_or(0)));
        return;
    }
    if items.iter().all(|v| v.is_float()) {
        items.sort_unstable_by(|lhs, rhs| f64::from_bits(lhs.0).total_cmp(&f64::from_bits(rhs.0)));
        return;
    }
    items.sort_by(|lhs, rhs| lhs.cmp(rhs));
}

#[inline]

fn validate_value_against_schema(
    val: &Value,
    schema: &Value,
    strings: &lumen_core::strings::StringTable,
) -> bool {
    match schema {
        Value::String(_) => {
            let type_name = schema.as_string_resolved(strings);
            match type_name.as_str() {
                "Any" => true,
                "Int" => matches!(val, Value::Int(_) | Value::BigInt(_)),
                "Float" => matches!(val, Value::Float(_)),
                "String" => matches!(val, Value::String(_)),
                "Bool" => matches!(val, Value::Bool(_)),
                "Null" => matches!(val, Value::Null),
                _ => false,
            }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_union_scalar_payload_matches_value_ord() {
        let mut actual: Vec<NbValue> = vec![
            NbValue::new_union("tag42", NbValue::new_int(4)),
            NbValue::new_union("tag42", NbValue::new_int(-7)),
            NbValue::new_union("tag42", NbValue::new_int(0)),
        ];
        let mut expected = actual.clone();
        expected.sort();

        sort_list_homogeneous(&mut actual);

        assert_eq!(actual, expected);
    }

    #[test]
    fn sort_record_scalar_shape_matches_value_ord() {
        let mut actual: Vec<NbValue> = vec![
            NbValue::new_record(
                "Node",
                BTreeMap::from([
                    ("age".to_string(), NbValue::new_int(3)),
                    ("alive".to_string(), NbValue::new_bool(true)),
                    ("score".to_string(), NbValue::new_float(8.0)),
                ]),
            ),
            NbValue::new_record(
                "Node",
                BTreeMap::from([
                    ("age".to_string(), NbValue::new_int(3)),
                    ("alive".to_string(), NbValue::new_bool(false)),
                    ("score".to_string(), NbValue::new_float(9.0)),
                ]),
            ),
            NbValue::new_record(
                "Node",
                BTreeMap::from([
                    ("age".to_string(), NbValue::new_int(1)),
                    ("alive".to_string(), NbValue::new_bool(true)),
                    ("score".to_string(), NbValue::new_float(7.5)),
                ]),
            ),
        ];
        let mut expected = actual.clone();
        expected.sort();

        sort_list_homogeneous(&mut actual);

        assert_eq!(actual, expected);
    }

    #[test]
    fn sort_record_shape_mismatch_falls_back_to_value_ord() {
        let mut actual: Vec<NbValue> = vec![
            NbValue::new_record(
                "Node",
                BTreeMap::from([("left".to_string(), NbValue::new_int(1))]),
            ),
            NbValue::new_record(
                "Node",
                BTreeMap::from([("right".to_string(), NbValue::new_int(0))]),
            ),
            NbValue::new_record(
                "Other",
                BTreeMap::from([("left".to_string(), NbValue::new_int(2))]),
            ),
        ];
        let mut expected = actual.clone();
        expected.sort();

        sort_list_homogeneous(&mut actual);

        assert_eq!(actual, expected);
    }

    #[test]
    fn json_parse_encode_roundtrip_nbvalue() {
        let mut vm = VM::new();
        vm.registers.resize(4, NbValue::new_null());
        let json_str = "{\"name\":\"Alice\",\"age\":30}";
        vm.set_reg_nb(1, NbValue::new_str(json_str));
        let result = vm.exec_intrinsic(0, 0, 140, 1);
        assert!(result.is_ok());

        let parsed = vm.reg_nb(0);
        if let Some(HeapValue::Map(m)) = parsed.as_heap_ref() {
            assert_eq!(m.get("name"), Some(&NbValue::new_str("Alice")));
            assert_eq!(m.get("age"), Some(&NbValue::new_int(30)));
        } else {
            panic!("expected map from json parse");
        }

        vm.set_reg_nb(1, parsed);
        let result = vm.exec_intrinsic(0, 0, 141, 1);
        assert!(result.is_ok());
        let encoded = vm.reg_nb(0);
        if let Some(HeapValue::Str(s)) = encoded.as_heap_ref() {
            assert!(s.contains("\"name\""));
            assert!(s.contains("\"Alice\""));
        } else {
            panic!("expected json string output");
        }
    }
}
