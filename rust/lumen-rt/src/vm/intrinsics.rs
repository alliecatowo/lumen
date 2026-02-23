//! Builtin function dispatch, intrinsic opcodes, and closure calls for the VM.

use super::*;
use crate::json_parser::parse_json_optimized;
use lumen_core::values::UnionPayload;
use num_bigint::BigInt;
use num_traits::{Signed, ToPrimitive};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

impl VM {
    /// Convert NbValue to Value without using peek_legacy/to_legacy.
    #[inline]
    fn nb_to_value(nb: NbValue) -> Value {
        if nb.is_int() {
            return Value::Int(nb.as_int().unwrap_or(0));
        }
        if !nb.is_nan_boxed() {
            return Value::Float(f64::from_bits(nb.0));
        }
        if nb.is_bool() {
            return Value::Bool(nb.as_bool().unwrap_or(false));
        }
        if nb.is_null() {
            return Value::Null;
        }
        if let Some(v) = nb.as_heap_ref() {
            return v.clone();
        }
        Value::Null
    }

    /// Borrow-through helper for TAG_PTR values (payload > 1).
    #[inline]
    fn nb_borrow_value(&self, nb: NbValue) -> Option<&Value> {
        if !nb.is_ptr() {
            return None;
        }
        let payload = nb.payload();
        if payload <= 1 {
            return None;
        }
        Some(unsafe { &*(payload as *const Value) })
    }

    /// Display formatting that matches Value::display_pretty without forcing
    /// a full NbValue -> Value bridge on common scalar paths.
    #[inline]
    fn nb_display_pretty(&self, nb: NbValue) -> String {
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
        if let Some(val_ref) = self.nb_borrow_value(nb) {
            return val_ref.display_pretty();
        }
        "null".to_string()
    }

    /// String conversion that mirrors Value::as_string_resolved semantics:
    /// interned strings resolve through the table, and floats keep one
    /// decimal when they are integral.
    #[inline]
    fn nb_to_string_as_resolved_value(&self, nb: NbValue) -> String {
        if let Some(val_ref) = self.nb_borrow_value(nb) {
            return match val_ref {
                Value::String(StringRef::Owned(s)) => s.clone(),
                Value::String(StringRef::Interned(id)) => {
                    self.strings.resolve(*id).unwrap_or("").to_string()
                }
                Value::Int(i) => i.to_string(),
                Value::Float(f) => {
                    if *f == f.floor() && f.abs() < 1e15 {
                        format!("{:.1}", f)
                    } else {
                        format!("{}", f)
                    }
                }
                Value::Bool(b) => b.to_string(),
                Value::Null => "null".to_string(),
                other => other.as_string_resolved(&self.strings),
            };
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
        if let Some(val_ref) = self.nb_borrow_value(nb) {
            return match val_ref {
                Value::String(StringRef::Owned(s)) => s.clone(),
                Value::String(StringRef::Interned(id)) => {
                    self.strings.resolve(*id).unwrap_or("").to_string()
                }
                Value::Int(i) => i.to_string(),
                Value::Float(f) => format!("{}", f),
                Value::Bool(b) => b.to_string(),
                Value::Null => "null".to_string(),
                other => other.display_pretty(),
            };
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
        if let Some(val_ref) = self.nb_borrow_value(nb) {
            if let Value::Int(i) = val_ref {
                return Some(*i);
            }
        }
        None
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
            return result;
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
                // TAG_PTR borrow-through: read len without cloning the collection
                if nb.is_ptr() {
                    let payload = nb.payload();
                    if payload > 1 {
                        let val_ref = unsafe { &*(payload as *const Value) };
                        let len = match val_ref {
                            Value::String(StringRef::Owned(s)) => s.len() as i64,
                            Value::String(StringRef::Interned(id)) => {
                                self.strings.resolve(*id).map(|s| s.len()).unwrap_or(0) as i64
                            }
                            Value::List(l) => l.len() as i64,
                            Value::Map(m) => m.len() as i64,
                            Value::Tuple(t) => t.len() as i64,
                            Value::Set(s) => s.len() as i64,
                            Value::Bytes(b) => b.len() as i64,
                            _ => 0,
                        };
                        return Ok(Value::Int(len));
                    }
                }
                Ok(Value::Int(0))
            }
            "append" => {
                let list = self.reg_take(base + a + 1);
                let elem = self.reg_take(base + a + 2);
                if let Value::List(mut l) = list {
                    Arc::make_mut(&mut l).push(elem);
                    Ok(Value::List(l))
                } else {
                    Ok(Value::new_list(vec![elem]))
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
                if let Some(val_ref) = self.nb_borrow_value(nb) {
                    return Ok(match val_ref {
                        Value::BigInt(n) => Value::BigInt(n.clone()),
                        Value::String(sr) => {
                            let s = match sr {
                                StringRef::Owned(s) => s.clone(),
                                StringRef::Interned(id) => {
                                    self.strings.resolve(*id).unwrap_or("").to_string()
                                }
                            };
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
                Ok(Value::Null)
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
                if let Some(val_ref) = self.nb_borrow_value(nb) {
                    return Ok(match val_ref {
                        Value::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::NAN)),
                        Value::String(sr) => {
                            let s = match sr {
                                StringRef::Owned(s) => s.clone(),
                                StringRef::Interned(id) => {
                                    self.strings.resolve(*id).unwrap_or("").to_string()
                                }
                            };
                            s.parse::<f64>().map(Value::Float).unwrap_or(Value::Null)
                        }
                        _ => Value::Null,
                    });
                }
                Ok(Value::Null)
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
                } else if nb.is_ptr() {
                    let payload = nb.payload();
                    if payload > 1 {
                        let val_ref = unsafe { &*(payload as *const Value) };
                        val_ref.type_name()
                    } else {
                        "Null"
                    }
                } else {
                    let arg = Self::nb_to_value(nb);
                    return Ok(Value::String(StringRef::Owned(arg.type_name().to_string())));
                };
                Ok(Value::String(StringRef::Owned(name.to_string())))
            }
            "keys" => {
                let nb = self.registers[base + a + 1];
                if nb.is_ptr() {
                    let payload = nb.payload();
                    if payload > 1 {
                        let val_ref = unsafe { &*(payload as *const Value) };
                        return Ok(match val_ref {
                            Value::Map(m) => Value::new_list(
                                m.keys()
                                    .map(|k| Value::String(StringRef::Owned(k.clone())))
                                    .collect(),
                            ),
                            Value::Record(r) => Value::new_list(
                                r.fields
                                    .keys()
                                    .map(|k| Value::String(StringRef::Owned(k.clone())))
                                    .collect(),
                            ),
                            _ => Value::new_list(vec![]),
                        });
                    }
                }
                Ok(Value::new_list(vec![]))
            }
            "values" => {
                let nb = self.registers[base + a + 1];
                if nb.is_ptr() {
                    let payload = nb.payload();
                    if payload > 1 {
                        let val_ref = unsafe { &*(payload as *const Value) };
                        return Ok(match val_ref {
                            Value::Map(m) => Value::new_list(m.values().cloned().collect()),
                            Value::Record(r) => {
                                Value::new_list(r.fields.values().cloned().collect())
                            }
                            _ => Value::new_list(vec![]),
                        });
                    }
                }
                Ok(Value::new_list(vec![]))
            }
            "contains" | "has" => {
                // TAG_PTR borrow-through fast-path: avoid cloning the collection
                let coll_nb = self.registers[base + a + 1];
                let needle_nb = self.registers[base + a + 2];
                if coll_nb.is_ptr() && coll_nb.payload() > 1 {
                    let coll_ref = unsafe {
                        &*((coll_nb.payload() & !NbValue::PTR_ARENA_FLAG) as *const Value)
                    };
                    // Fast-path: int needle in list (primes sieve pattern)
                    if needle_nb.is_int() {
                        let needle_val = Value::Int(needle_nb.as_int().unwrap_or(0));
                        let result = match coll_ref {
                            Value::List(l) => l.iter().any(|v| v == &needle_val),
                            Value::Set(s) => s.iter().any(|v| v == &needle_val),
                            _ => false,
                        };
                        return Ok(Value::Bool(result));
                    }
                }
                let collection = Self::nb_to_value(coll_nb);
                let needle = Self::nb_to_value(needle_nb);
                let result = match collection {
                    Value::List(l) => l.iter().any(|v| v == &needle),
                    Value::Set(s) => s.iter().any(|v| v == &needle),
                    Value::Map(m) => {
                        let needle_str = needle.as_string_resolved(&self.strings);
                        m.contains_key(&needle_str)
                    }
                    Value::String(sr) => {
                        let s = match sr {
                            StringRef::Owned(s) => s,
                            StringRef::Interned(id) => {
                                self.strings.resolve(id).unwrap_or("").to_string()
                            }
                        };
                        let needle_str = needle.as_string_resolved(&self.strings);
                        s.contains(&needle_str)
                    }
                    _ => false,
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
                // TAG_PTR borrow-through for the list
                if list_nb.is_ptr() && list_nb.payload() > 1 {
                    let val_ref = unsafe {
                        &*((list_nb.payload() & !NbValue::PTR_ARENA_FLAG) as *const Value)
                    };
                    if let Value::List(l) = val_ref {
                        let joined = l
                            .iter()
                            .map(|v| v.display_pretty())
                            .collect::<Vec<_>>()
                            .join(&sep);
                        return Ok(Value::String(StringRef::Owned(joined)));
                    }
                }
                let list = Self::nb_to_value(list_nb);
                if let Value::List(l) = list {
                    let joined = l
                        .iter()
                        .map(|v| v.display_pretty())
                        .collect::<Vec<_>>()
                        .join(&sep);
                    Ok(Value::String(StringRef::Owned(joined)))
                } else {
                    Ok(Value::String(StringRef::Owned(list.display_pretty())))
                }
            }
            "split" => {
                let s = self.nb_to_string_resolved(self.registers[base + a + 1]);
                let sep = if nargs > 1 {
                    self.nb_to_string_resolved(self.registers[base + a + 2])
                } else {
                    " ".to_string()
                };
                let parts: Vec<Value> = s
                    .split(&sep)
                    .map(|p| Value::String(StringRef::Owned(p.to_string())))
                    .collect();
                Ok(Value::new_list(parts))
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
                if let Some(val_ref) = self.nb_borrow_value(nb) {
                    return Ok(match val_ref {
                        Value::BigInt(n) => Value::BigInt(n.abs()),
                        _ => val_ref.clone(),
                    });
                }
                Ok(Value::Null)
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
                Ok(Self::nb_to_value(lhs_nb))
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
                Ok(Self::nb_to_value(lhs_nb))
            }
            "range" => {
                // NbValue fast-path: extract ints without peek_legacy.
                let start_nb = self.registers[base + a + 1];
                let end_nb = self.registers[base + a + 2];
                let start = self.nb_to_int(start_nb).unwrap_or(0);
                let end = self.nb_to_int(end_nb).unwrap_or(0);
                let list: Vec<Value> = (start..end).map(Value::Int).collect();
                Ok(Value::new_list(list))
            }
            "spawn" => {
                if nargs == 0 {
                    return Err(VmError::TypeError(
                        "spawn requires a callable argument".to_string(),
                    ));
                }
                let callee = Self::nb_to_value(self.registers[base + a + 1]);
                let args: Vec<Value> = (1..nargs)
                    .map(|i| Self::nb_to_value(self.registers[base + a + 1 + i]))
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
                let mut out = Vec::with_capacity(args.len());
                for arg in args {
                    match arg {
                        Value::Future(ref f) => match self.future_states.get(&f.id) {
                            Some(FutureState::Completed(v)) => out.push(v.clone()),
                            Some(FutureState::Pending) => out.push(arg.clone()),
                            Some(FutureState::Error(_)) | None => {
                                out.push(Value::Future(FutureValue {
                                    id: f.id,
                                    state: FutureStatus::Error,
                                }));
                            }
                        },
                        other => out.push(other),
                    }
                }
                Ok(Value::new_list(out))
            }
            "race" => {
                let mut first_pending: Option<Value> = None;
                for arg in self.orchestration_args(base, a, nargs) {
                    match arg {
                        Value::Future(ref f) => match self.future_states.get(&f.id) {
                            Some(FutureState::Completed(v)) => return Ok(v.clone()),
                            Some(FutureState::Pending) => {
                                if first_pending.is_none() {
                                    first_pending = Some(arg.clone());
                                }
                            }
                            Some(FutureState::Error(_)) | None => {}
                        },
                        other => return Ok(other),
                    }
                }
                Ok(first_pending.unwrap_or(Value::Null))
            }
            "select" => {
                let mut first_pending: Option<Value> = None;
                for arg in self.orchestration_args(base, a, nargs) {
                    let candidate = match arg {
                        Value::Future(ref f) => match self.future_states.get(&f.id) {
                            Some(FutureState::Completed(v)) => Some(v.clone()),
                            Some(FutureState::Pending) => {
                                if first_pending.is_none() {
                                    first_pending = Some(Value::Future(FutureValue {
                                        id: f.id,
                                        state: FutureStatus::Pending,
                                    }));
                                }
                                None
                            }
                            _ => None,
                        },
                        other => Some(other),
                    };
                    if let Some(value) = candidate {
                        if !matches!(value, Value::Null) {
                            return Ok(value);
                        }
                    }
                }
                Ok(first_pending.unwrap_or(Value::Null))
            }
            "vote" => {
                let mut counts: BTreeMap<Value, (usize, usize)> = BTreeMap::new();
                let mut first_pending: Option<Value> = None;
                for (i, arg) in self
                    .orchestration_args(base, a, nargs)
                    .into_iter()
                    .enumerate()
                {
                    let value = match arg {
                        Value::Future(ref f) => match self.future_states.get(&f.id) {
                            Some(FutureState::Completed(v)) => Some(v.clone()),
                            Some(FutureState::Pending) => {
                                if first_pending.is_none() {
                                    first_pending = Some(Value::Future(FutureValue {
                                        id: f.id,
                                        state: FutureStatus::Pending,
                                    }));
                                }
                                None
                            }
                            _ => None,
                        },
                        other => Some(other),
                    };
                    if let Some(value) = value {
                        let entry = counts.entry(value).or_insert((0, i));
                        entry.0 += 1;
                    }
                }
                if counts.is_empty() {
                    return Ok(first_pending.unwrap_or(Value::Null));
                }
                let mut best: Option<(Value, usize, usize)> = None;
                for (value, (count, first_idx)) in counts {
                    match &best {
                        None => best = Some((value, count, first_idx)),
                        Some((_, best_count, best_idx)) => {
                            if count > *best_count
                                || (count == *best_count && first_idx < *best_idx)
                            {
                                best = Some((value, count, first_idx));
                            }
                        }
                    }
                }
                Ok(best.map(|(value, _, _)| value).unwrap_or(Value::Null))
            }
            "timeout" => {
                if nargs == 0 {
                    return Ok(Value::Null);
                }
                let arg = Self::nb_to_value(self.registers[base + a + 1]);
                match arg {
                    Value::Future(f) => match self.future_states.get(&f.id) {
                        Some(FutureState::Completed(v)) => Ok(v.clone()),
                        Some(FutureState::Pending) => Ok(Value::Null),
                        Some(FutureState::Error(msg)) => {
                            Err(VmError::Runtime(format!("timeout target failed: {}", msg)))
                        }
                        None => Ok(Value::Null),
                    },
                    other => Ok(other),
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
                if nb.is_ptr() && nb.payload() > 1 {
                    let val_ref =
                        unsafe { &*((nb.payload() & !NbValue::PTR_ARENA_FLAG) as *const Value) };
                    if let Value::List(l) = val_ref {
                        let mut result = Vec::new();
                        for item in l.iter() {
                            if let Value::List(inner) = item {
                                result.extend(inner.iter().cloned());
                            } else {
                                result.push(item.clone());
                            }
                        }
                        return Ok(Value::new_list(result));
                    }
                }
                let arg = Self::nb_to_value(nb);
                if let Value::List(l) = arg {
                    let mut result = Vec::new();
                    for item in l.iter() {
                        if let Value::List(inner) = item {
                            result.extend(inner.iter().cloned());
                        } else {
                            result.push(item.clone());
                        }
                    }
                    Ok(Value::new_list(result))
                } else {
                    Ok(arg)
                }
            }
            "unique" => {
                let nb = self.registers[base + a + 1];
                if nb.is_ptr() && nb.payload() > 1 {
                    let val_ref =
                        unsafe { &*((nb.payload() & !NbValue::PTR_ARENA_FLAG) as *const Value) };
                    if let Value::List(l) = val_ref {
                        let mut result = Vec::new();
                        for item in l.iter() {
                            if !result.contains(item) {
                                result.push(item.clone());
                            }
                        }
                        return Ok(Value::new_list(result));
                    }
                }
                let arg = Self::nb_to_value(nb);
                if let Value::List(l) = arg {
                    let mut result = Vec::new();
                    for item in l.iter() {
                        if !result.contains(item) {
                            result.push(item.clone());
                        }
                    }
                    Ok(Value::new_list(result))
                } else {
                    Ok(arg)
                }
            }
            "take" => {
                let nb = self.registers[base + a + 1];
                let n = self.nb_to_int(self.registers[base + a + 2]).unwrap_or(0) as usize;
                if nb.is_ptr() && nb.payload() > 1 {
                    let val_ref =
                        unsafe { &*((nb.payload() & !NbValue::PTR_ARENA_FLAG) as *const Value) };
                    if let Value::List(l) = val_ref {
                        return Ok(Value::new_list(l.iter().take(n).cloned().collect()));
                    }
                }
                let arg = Self::nb_to_value(nb);
                if let Value::List(l) = arg {
                    Ok(Value::new_list(l.iter().take(n).cloned().collect()))
                } else {
                    Ok(arg)
                }
            }
            "drop" => {
                let nb = self.registers[base + a + 1];
                let n = self.nb_to_int(self.registers[base + a + 2]).unwrap_or(0) as usize;
                if nb.is_ptr() && nb.payload() > 1 {
                    let val_ref =
                        unsafe { &*((nb.payload() & !NbValue::PTR_ARENA_FLAG) as *const Value) };
                    if let Value::List(l) = val_ref {
                        return Ok(Value::new_list(l.iter().skip(n).cloned().collect()));
                    }
                }
                let arg = Self::nb_to_value(nb);
                if let Value::List(l) = arg {
                    Ok(Value::new_list(l.iter().skip(n).cloned().collect()))
                } else {
                    Ok(arg)
                }
            }
            "first" | "head" => {
                let nb = self.registers[base + a + 1];
                if nb.is_ptr() {
                    let payload = nb.payload();
                    if payload > 1 {
                        let val_ref = unsafe { &*(payload as *const Value) };
                        return Ok(match val_ref {
                            Value::List(l) => l.first().cloned().unwrap_or(Value::Null),
                            Value::Tuple(t) => t.first().cloned().unwrap_or(Value::Null),
                            _ => Value::Null,
                        });
                    }
                }
                Ok(Value::Null)
            }
            "last" | "tail" => {
                let nb = self.registers[base + a + 1];
                if nb.is_ptr() {
                    let payload = nb.payload();
                    if payload > 1 {
                        let val_ref = unsafe { &*(payload as *const Value) };
                        return Ok(match val_ref {
                            Value::List(l) => l.last().cloned().unwrap_or(Value::Null),
                            Value::Tuple(t) => t.last().cloned().unwrap_or(Value::Null),
                            _ => Value::Null,
                        });
                    }
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
                if nb.is_ptr() {
                    let payload = nb.payload();
                    if payload > 1 {
                        let val_ref = unsafe { &*(payload as *const Value) };
                        let empty = match val_ref {
                            Value::List(l) => l.is_empty(),
                            Value::Map(m) => m.is_empty(),
                            Value::Set(s) => s.is_empty(),
                            Value::Tuple(t) => t.is_empty(),
                            Value::String(StringRef::Owned(s)) => s.is_empty(),
                            Value::String(StringRef::Interned(id)) => self
                                .strings
                                .resolve(*id)
                                .map(|s| s.is_empty())
                                .unwrap_or(true),
                            Value::Null => true,
                            _ => false,
                        };
                        return Ok(Value::Bool(empty));
                    }
                }
                let arg = Self::nb_to_value(nb);
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
                Ok(Value::new_list(
                    s.chars()
                        .map(|c| Value::String(StringRef::Owned(c.to_string())))
                        .collect(),
                ))
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
                if let Some(val_ref) = self.nb_borrow_value(nb) {
                    return Ok(match val_ref {
                        Value::BigInt(n) => {
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
                if let Some(val_ref) = self.nb_borrow_value(val_nb) {
                    Ok(val_ref.clone())
                } else {
                    Ok(Value::Null)
                }
            }
            "json_parse" | "parse_json" => {
                let s = self.nb_to_string_resolved(self.registers[base + a + 1]);
                match parse_json_optimized(&s) {
                    Ok(v) => Ok(v),
                    Err(_) => Ok(Value::Null),
                }
            }
            "json_encode" | "to_json" => {
                let nb = self.registers[base + a + 1];
                if nb.is_ptr() && nb.payload() > 1 {
                    let val_ref =
                        unsafe { &*((nb.payload() & !NbValue::PTR_ARENA_FLAG) as *const Value) };
                    let j = helpers::value_to_json(val_ref, &self.strings);
                    return Ok(Value::String(StringRef::Owned(j.to_string())));
                }
                let val = Self::nb_to_value(nb);
                let j = helpers::value_to_json(&val, &self.strings);
                Ok(Value::String(StringRef::Owned(j.to_string())))
            }
            "json_pretty" => {
                let nb = self.registers[base + a + 1];
                if nb.is_ptr() && nb.payload() > 1 {
                    let val_ref =
                        unsafe { &*((nb.payload() & !NbValue::PTR_ARENA_FLAG) as *const Value) };
                    let j = helpers::value_to_json(val_ref, &self.strings);
                    let pretty = serde_json::to_string_pretty(&j)
                        .map_err(|e| VmError::Runtime(format!("json_pretty failed: {}", e)))?;
                    return Ok(Value::String(StringRef::Owned(pretty)));
                }
                let val = Self::nb_to_value(nb);
                let j = helpers::value_to_json(&val, &self.strings);
                let pretty = serde_json::to_string_pretty(&j)
                    .map_err(|e| VmError::Runtime(format!("json_pretty failed: {}", e)))?;
                Ok(Value::String(StringRef::Owned(pretty)))
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
                if nb.is_ptr() && nb.payload() > 1 {
                    let val_ref =
                        unsafe { &*((nb.payload() & !NbValue::PTR_ARENA_FLAG) as *const Value) };
                    return Ok(match val_ref {
                        Value::Bytes(b) => {
                            Value::String(StringRef::Owned(String::from_utf8_lossy(b).to_string()))
                        }
                        _ => Value::Null,
                    });
                }
                Ok(Value::Null)
            }
            "bytes_len" => {
                // bytes_len(b: Bytes) -> Int
                // Return the number of bytes in a Bytes value. Returns 0 for non-Bytes.
                let nb = self.registers[base + a + 1];
                if nb.is_ptr() && nb.payload() > 1 {
                    let val_ref =
                        unsafe { &*((nb.payload() & !NbValue::PTR_ARENA_FLAG) as *const Value) };
                    return Ok(match val_ref {
                        Value::Bytes(b) => Value::Int(b.len() as i64),
                        _ => Value::Int(0),
                    });
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
                let get_slice = |b: &Vec<u8>| -> Value {
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
                if nb.is_ptr() && nb.payload() > 1 {
                    let val_ref =
                        unsafe { &*((nb.payload() & !NbValue::PTR_ARENA_FLAG) as *const Value) };
                    return Ok(match val_ref {
                        Value::Bytes(b) => get_slice(b),
                        _ => Value::Null,
                    });
                }
                let arg = Self::nb_to_value(nb);
                Ok(match arg {
                    Value::Bytes(ref b) => get_slice(b),
                    _ => Value::Null,
                })
            }
            "bytes_concat" => {
                // bytes_concat(a: Bytes, b: Bytes) -> Bytes
                // Concatenate two Bytes values. Returns Null if either argument is not Bytes.
                let nb_a = self.registers[base + a + 1];
                let nb_b = self.registers[base + a + 2];
                let bytes_a = if nb_a.is_ptr() && nb_a.payload() > 1 {
                    let val_ref =
                        unsafe { &*((nb_a.payload() & !NbValue::PTR_ARENA_FLAG) as *const Value) };
                    match val_ref {
                        Value::Bytes(b) => Some(b.clone()),
                        _ => None,
                    }
                } else {
                    match Self::nb_to_value(nb_a) {
                        Value::Bytes(b) => Some(b.clone()),
                        _ => None,
                    }
                };
                let bytes_b = if nb_b.is_ptr() && nb_b.payload() > 1 {
                    let val_ref =
                        unsafe { &*((nb_b.payload() & !NbValue::PTR_ARENA_FLAG) as *const Value) };
                    match val_ref {
                        Value::Bytes(b) => Some(b.clone()),
                        _ => None,
                    }
                } else {
                    match Self::nb_to_value(nb_b) {
                        Value::Bytes(b) => Some(b.clone()),
                        _ => None,
                    }
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
        _a: usize,
        func_id: usize,
        arg_reg: usize,
    ) -> Result<Value, VmError> {
        let arg_nb = self.registers[base + arg_reg];
        match func_id {
            0 => {
                // LENGTH
                if arg_nb.is_int() || arg_nb.is_float() || arg_nb.is_bool() || arg_nb.is_null() {
                    return Ok(Value::Int(0));
                }
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    let len = match val_ref {
                        Value::String(StringRef::Owned(s)) => s.chars().count() as i64,
                        Value::String(StringRef::Interned(id)) => {
                            self.strings
                                .resolve(*id)
                                .map(|s| s.chars().count())
                                .unwrap_or(0) as i64
                        }
                        Value::List(l) => l.len() as i64,
                        Value::Map(m) => m.len() as i64,
                        Value::Tuple(t) => t.len() as i64,
                        Value::Set(s) => s.len() as i64,
                        Value::Bytes(b) => b.len() as i64,
                        _ => 0,
                    };
                    return Ok(Value::Int(len));
                }
                let arg = Self::nb_to_value(arg_nb);
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    let count = match val_ref {
                        Value::List(l) => l.len() as i64,
                        Value::Map(m) => m.len() as i64,
                        Value::String(StringRef::Owned(s)) => s.chars().count() as i64,
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    return Ok(Value::Bool(val_ref.is_truthy()));
                }
                return Ok(Value::Bool(Self::nb_to_value(arg_nb).is_truthy()));
            }
            3 => {
                // HASH
                use sha2::{Digest, Sha256};
                let s = self.nb_display_pretty(arg_nb);
                let hash = format!("{:x}", Sha256::digest(s.as_bytes()));
                return Ok(Value::String(StringRef::Owned(format!("sha256:{}", hash))));
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    let out = match val_ref {
                        Value::Int(n) => Value::Int(*n),
                        Value::Float(f) => Value::Int(*f as i64),
                        Value::String(sr) => {
                            let s = match sr {
                                StringRef::Owned(s) => s.as_str(),
                                StringRef::Interned(id) => self.strings.resolve(*id).unwrap_or(""),
                            };
                            s.parse::<i64>().map(Value::Int).unwrap_or(Value::Null)
                        }
                        Value::Bool(b) => Value::Int(if *b { 1 } else { 0 }),
                        _ => Value::Null,
                    };
                    return Ok(out);
                }
                let arg = Self::nb_to_value(arg_nb);
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    let out = match val_ref {
                        Value::Float(f) => Value::Float(*f),
                        Value::Int(n) => Value::Float(*n as f64),
                        Value::String(sr) => {
                            let s = match sr {
                                StringRef::Owned(s) => s.as_str(),
                                StringRef::Interned(id) => self.strings.resolve(*id).unwrap_or(""),
                            };
                            s.parse::<f64>().map(Value::Float).unwrap_or(Value::Null)
                        }
                        _ => Value::Null,
                    };
                    return Ok(out);
                }
                let arg = Self::nb_to_value(arg_nb);
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    return Ok(Value::String(StringRef::Owned(
                        val_ref.type_name().to_string(),
                    )));
                }
                return Ok(Value::String(StringRef::Owned(
                    Self::nb_to_value(arg_nb).type_name().to_string(),
                )));
            }
            16 => {
                // CONTAINS
                let needle_nb = self.registers[base + arg_reg + 1];
                let needle = if needle_nb.is_int() {
                    Value::Int(needle_nb.as_int().unwrap_or(0))
                } else if needle_nb.is_float() {
                    Value::Float(f64::from_bits(needle_nb.0))
                } else if needle_nb.is_bool() {
                    Value::Bool(needle_nb.as_bool().unwrap_or(false))
                } else if needle_nb.is_null() {
                    Value::Null
                } else {
                    Self::nb_to_value(needle_nb)
                };
                if let Some(collection) = self.nb_borrow_value(arg_nb) {
                    let result = match collection {
                        Value::List(l) => l.iter().any(|v| v == &needle),
                        Value::Set(s) => s.iter().any(|v| v == &needle),
                        Value::Map(m) => {
                            let needle_str = needle.as_string_resolved(&self.strings);
                            m.contains_key(&needle_str)
                        }
                        Value::String(sr) => {
                            let s = match sr {
                                StringRef::Owned(s) => s.as_str(),
                                StringRef::Interned(id) => self.strings.resolve(*id).unwrap_or(""),
                            };
                            let needle_str = needle.as_string_resolved(&self.strings);
                            s.contains(&needle_str)
                        }
                        _ => false,
                    };
                    return Ok(Value::Bool(result));
                }
                if arg_nb.is_int() || arg_nb.is_float() || arg_nb.is_bool() || arg_nb.is_null() {
                    return Ok(Value::Bool(false));
                }
                let collection = Self::nb_to_value(arg_nb);
                let result = match collection {
                    Value::List(l) => l.iter().any(|v| v == &needle),
                    Value::Set(s) => s.iter().any(|v| v == &needle),
                    Value::Map(m) => {
                        let needle_str = needle.as_string_resolved(&self.strings);
                        m.contains_key(&needle_str)
                    }
                    Value::String(sr) => {
                        let s = match sr {
                            StringRef::Owned(s) => s,
                            StringRef::Interned(id) => {
                                self.strings.resolve(id).unwrap_or("").to_string()
                            }
                        };
                        let needle_str = needle.as_string_resolved(&self.strings);
                        s.contains(&needle_str)
                    }
                    _ => false,
                };
                return Ok(Value::Bool(result));
            }
            17 => {
                // JOIN
                let sep = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                if let Some(Value::List(l)) = self.nb_borrow_value(arg_nb) {
                    let joined = l
                        .iter()
                        .map(|v| v.display_pretty())
                        .collect::<Vec<_>>()
                        .join(&sep);
                    return Ok(Value::String(StringRef::Owned(joined)));
                }
                return Ok(Value::String(StringRef::Owned(String::new())));
            }
            18 => {
                // SPLIT
                let sep = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                let s = self.nb_to_string_as_resolved_value(arg_nb);
                let parts: Vec<Value> = s
                    .split(&sep)
                    .map(|p| Value::String(StringRef::Owned(p.to_string())))
                    .collect();
                return Ok(Value::new_list(parts));
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    let out = match val_ref {
                        Value::List(l) => {
                            let end = end.min(l.len());
                            let start = start.min(end);
                            Value::new_list(l[start..end].to_vec())
                        }
                        Value::String(sr) => {
                            let s = match sr {
                                StringRef::Owned(s) => s.clone(),
                                StringRef::Interned(id) => {
                                    self.strings.resolve(*id).unwrap_or("").to_string()
                                }
                            };
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
                let arg = Self::nb_to_value(arg_nb);
                return Ok(match arg {
                    Value::List(l) => {
                        let end = end.min(l.len());
                        let start = start.min(end);
                        Value::new_list(l[start..end].to_vec())
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
                let list: Vec<Value> = (start..end).map(Value::Int).collect();
                return Ok(Value::new_list(list));
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    return Ok(match val_ref {
                        Value::Int(n) => Value::Int(n.abs()),
                        Value::Float(f) => Value::Float(f.abs()),
                        Value::BigInt(n) => Value::BigInt(n.abs()),
                        _ => Value::Null,
                    });
                }
                let arg = Self::nb_to_value(arg_nb);
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    return match val_ref {
                        Value::Int(a) => {
                            if other_nb.is_int() {
                                Ok(Value::Int((*a).min(other_nb.as_int().unwrap_or(0))))
                            } else if other_nb.is_float() {
                                Ok(Value::Float((*a as f64).min(f64::from_bits(other_nb.0))))
                            } else {
                                Ok(Value::Int(*a))
                            }
                        }
                        Value::Float(a) => {
                            if other_nb.is_float() {
                                Ok(Value::Float(a.min(f64::from_bits(other_nb.0))))
                            } else if other_nb.is_int() {
                                Ok(Value::Float(a.min(other_nb.as_int().unwrap_or(0) as f64)))
                            } else {
                                Ok(Value::Float(*a))
                            }
                        }
                        _ => Ok(val_ref.clone()),
                    };
                }
                let arg = Self::nb_to_value(arg_nb);
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    return match val_ref {
                        Value::Int(a) => {
                            if other_nb.is_int() {
                                Ok(Value::Int((*a).max(other_nb.as_int().unwrap_or(0))))
                            } else if other_nb.is_float() {
                                Ok(Value::Float((*a as f64).max(f64::from_bits(other_nb.0))))
                            } else {
                                Ok(Value::Int(*a))
                            }
                        }
                        Value::Float(a) => {
                            if other_nb.is_float() {
                                Ok(Value::Float(a.max(f64::from_bits(other_nb.0))))
                            } else if other_nb.is_int() {
                                Ok(Value::Float(a.max(other_nb.as_int().unwrap_or(0) as f64)))
                            } else {
                                Ok(Value::Float(*a))
                            }
                        }
                        _ => Ok(val_ref.clone()),
                    };
                }
                let arg = Self::nb_to_value(arg_nb);
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    let empty = match val_ref {
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
                return Ok(Value::Bool(false));
            }
            51 => {
                // CHARS
                let s = self.nb_to_string_as_resolved_value(arg_nb);
                return Ok(Value::new_list(
                    s.chars()
                        .map(|c| Value::String(StringRef::Owned(c.to_string())))
                        .collect(),
                ));
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    return Ok(match val_ref {
                        Value::Float(f) => Value::Float(f.round()),
                        Value::Int(n) => Value::Int(*n),
                        _ => Value::Null,
                    });
                }
                let arg = Self::nb_to_value(arg_nb);
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    return Ok(match val_ref {
                        Value::Float(f) => Value::Float(f.ceil()),
                        Value::Int(n) => Value::Int(*n),
                        _ => Value::Null,
                    });
                }
                let arg = Self::nb_to_value(arg_nb);
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    return Ok(match val_ref {
                        Value::Float(f) => Value::Float(f.floor()),
                        Value::Int(n) => Value::Int(*n),
                        _ => Value::Null,
                    });
                }
                let arg = Self::nb_to_value(arg_nb);
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    return Ok(match val_ref {
                        Value::Float(f) => Value::Float(f.sqrt()),
                        Value::Int(n) => Value::Float((*n as f64).sqrt()),
                        _ => Value::Null,
                    });
                }
                let arg = Self::nb_to_value(arg_nb);
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    return Ok(match val_ref {
                        Value::Int(x) => {
                            if exp_nb.is_int() {
                                int_pow(*x, exp_nb.as_int().unwrap_or(0))
                            } else {
                                Value::Null
                            }
                        }
                        Value::Float(x) => {
                            if exp_nb.is_float() {
                                Value::Float(x.powf(f64::from_bits(exp_nb.0)))
                            } else {
                                Value::Null
                            }
                        }
                        _ => Value::Null,
                    });
                }
                let arg = Self::nb_to_value(arg_nb);
                let exp = if exp_nb.is_int() {
                    Value::Int(exp_nb.as_int().unwrap_or(0))
                } else if exp_nb.is_float() {
                    Value::Float(f64::from_bits(exp_nb.0))
                } else {
                    Self::nb_to_value(exp_nb)
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    return Ok(match val_ref {
                        Value::Float(f) => Value::Float(f.ln()),
                        Value::Int(n) => Value::Float((*n as f64).ln()),
                        _ => Value::Null,
                    });
                }
                let arg = Self::nb_to_value(arg_nb);
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    return Ok(match val_ref {
                        Value::Float(f) => Value::Float(f.sin()),
                        Value::Int(n) => Value::Float((*n as f64).sin()),
                        _ => Value::Null,
                    });
                }
                let arg = Self::nb_to_value(arg_nb);
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    return Ok(match val_ref {
                        Value::Float(f) => Value::Float(f.cos()),
                        Value::Int(n) => Value::Float((*n as f64).cos()),
                        _ => Value::Null,
                    });
                }
                let arg = Self::nb_to_value(arg_nb);
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
                        Self::nb_to_value(lo_nb)
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
                        Self::nb_to_value(hi_nb)
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
                        Self::nb_to_value(lo_nb)
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
                        Self::nb_to_value(hi_nb)
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    return Ok(match val_ref {
                        Value::Int(v) => {
                            let lo = Self::nb_to_value(lo_nb);
                            let hi = Self::nb_to_value(hi_nb);
                            match (lo, hi) {
                                (Value::Int(l), Value::Int(h)) => Value::Int((*v).max(l).min(h)),
                                _ => Value::Int(*v),
                            }
                        }
                        Value::Float(v) => {
                            let lo = Self::nb_to_value(lo_nb);
                            let hi = Self::nb_to_value(hi_nb);
                            match (lo, hi) {
                                (Value::Float(l), Value::Float(h)) => {
                                    Value::Float((*v).max(l).min(h))
                                }
                                _ => Value::Float(*v),
                            }
                        }
                        _ => val_ref.clone(),
                    });
                }
                let arg = Self::nb_to_value(arg_nb);
                let lo = Self::nb_to_value(lo_nb);
                let hi = Self::nb_to_value(hi_nb);
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    return Ok(match val_ref {
                        Value::Float(f) => Value::Float(f.tan()),
                        Value::Int(n) => Value::Float((*n as f64).tan()),
                        _ => Value::Null,
                    });
                }
                let arg = Self::nb_to_value(arg_nb);
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
                if let Some(val_ref) = self.nb_borrow_value(arg_nb) {
                    return Ok(match val_ref {
                        Value::Float(f) => Value::Float(f.trunc()),
                        Value::Int(n) => Value::Int(*n),
                        _ => Value::Null,
                    });
                }
                let arg = Self::nb_to_value(arg_nb);
                return Ok(match arg {
                    Value::Float(f) => Value::Float(f.trunc()),
                    Value::Int(n) => Value::Int(n),
                    _ => Value::Null,
                });
            }
            _ => {}
        }
        if let Some(arg_ref) = self.nb_borrow_value(arg_nb) {
            match func_id {
                4 => {
                    // DIFF
                    let other = Self::nb_to_value(self.registers[base + arg_reg + 1]);
                    return Ok(self.diff_values(arg_ref, &other));
                }
                5 => {
                    // PATCH
                    let patches = Self::nb_to_value(self.registers[base + arg_reg + 1]);
                    return Ok(self.patch_value(arg_ref, &patches));
                }
                6 => {
                    // REDACT
                    let fields = Self::nb_to_value(self.registers[base + arg_reg + 1]);
                    return Ok(self.redact_value(arg_ref, &fields));
                }
                7 => {
                    // VALIDATE
                    let nargs = if arg_reg == 0 { 0 } else { 1 };
                    if nargs < 1 {
                        return Ok(Value::Bool(!matches!(arg_ref, Value::Null)));
                    }
                    let schema_val = Self::nb_to_value(self.registers[base + arg_reg + 1]);
                    return Ok(Value::Bool(validate_value_against_schema(
                        arg_ref,
                        &schema_val,
                        &self.strings,
                    )));
                }
                35 => {
                    // ZIP
                    let b_list = Self::nb_to_value(self.registers[base + arg_reg + 1]);
                    if let (Value::List(la), Value::List(lb)) = (arg_ref, &b_list) {
                        let result: Vec<Value> = la
                            .iter()
                            .zip(lb.iter())
                            .map(|(x, y)| Value::new_tuple(vec![x.clone(), y.clone()]))
                            .collect();
                        return Ok(Value::new_list(result));
                    }
                    return Ok(Value::new_list(vec![]));
                }
                36 => {
                    // ENUMERATE
                    if let Value::List(l) = arg_ref {
                        let result: Vec<Value> = l
                            .iter()
                            .enumerate()
                            .map(|(i, v)| Value::new_tuple(vec![Value::Int(i as i64), v.clone()]))
                            .collect();
                        return Ok(Value::new_list(result));
                    }
                    return Ok(Value::new_list(vec![]));
                }
                42 => {
                    // CHUNK
                    let size = self
                        .nb_to_int(self.registers[base + arg_reg + 1])
                        .unwrap_or(1) as usize;
                    if let Value::List(l) = arg_ref {
                        let result: Vec<Value> = l
                            .chunks(size.max(1))
                            .map(|chunk| Value::new_list(chunk.to_vec()))
                            .collect();
                        return Ok(Value::new_list(result));
                    }
                    return Ok(Value::new_list(vec![]));
                }
                43 => {
                    // WINDOW
                    let n = self
                        .nb_to_int(self.registers[base + arg_reg + 1])
                        .unwrap_or(1) as usize;
                    if let Value::List(l) = arg_ref {
                        if n == 0 || n > l.len() {
                            return Ok(Value::new_list(vec![]));
                        }
                        let result: Vec<Value> =
                            l.windows(n).map(|w| Value::new_list(w.to_vec())).collect();
                        return Ok(Value::new_list(result));
                    }
                    return Ok(Value::new_list(vec![]));
                }
                48 => {
                    // FIRST
                    return Ok(match arg_ref {
                        Value::List(l) => l.first().cloned().unwrap_or(Value::Null),
                        _ => Value::Null,
                    });
                }
                49 => {
                    // LAST
                    return Ok(match arg_ref {
                        Value::List(l) => l.last().cloned().unwrap_or(Value::Null),
                        _ => Value::Null,
                    });
                }
                50 => {
                    // IS_EMPTY
                    let empty = match arg_ref {
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
                    return Ok(Value::Bool(match arg_ref {
                        Value::Map(m) => m.contains_key(&key),
                        Value::Record(r) => r.fields.contains_key(&key),
                        _ => false,
                    }));
                }
                106 => {
                    // STRING_CONCAT
                    let other_nb = self.registers[base + arg_reg + 1];
                    let other = if let Some(val_ref) = self.nb_borrow_value(other_nb) {
                        val_ref.clone()
                    } else {
                        Self::nb_to_value(other_nb)
                    };
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
                        let headers_val = Self::nb_to_value(self.registers[base + arg_reg + 3]);
                        extract_headers_map(&headers_val)
                    };
                    return Ok(http_builtin_request(&method, &url, &body, &headers));
                }
                _ => {}
            }
        }
        let arg = Self::nb_to_value(arg_nb);
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
                let other = Self::nb_to_value(self.registers[base + arg_reg + 1]);
                Ok(self.diff_values(&arg, &other))
            }
            5 => {
                // PATCH
                let patches = Self::nb_to_value(self.registers[base + arg_reg + 1]);
                Ok(self.patch_value(&arg, &patches))
            }
            6 => {
                // REDACT
                let fields = Self::nb_to_value(self.registers[base + arg_reg + 1]);
                Ok(self.redact_value(&arg, &fields))
            }
            7 => {
                // VALIDATE
                let nargs = if arg_reg == 0 { 0 } else { 1 }; // Simplified arity detection for intrinsic
                if nargs < 1 {
                    Ok(Value::Bool(!matches!(arg, Value::Null)))
                } else {
                    let schema_val = Self::nb_to_value(self.registers[base + arg_reg + 1]);
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
                let b_list = Self::nb_to_value(self.registers[base + arg_reg + 1]);
                if let (Value::List(la), Value::List(lb)) = (&arg, &b_list) {
                    let result: Vec<Value> = la
                        .iter()
                        .zip(lb.iter())
                        .map(|(x, y)| Value::new_tuple(vec![x.clone(), y.clone()]))
                        .collect();
                    Ok(Value::new_list(result))
                } else {
                    Ok(Value::new_list(vec![]))
                }
            }
            36 => {
                // ENUMERATE
                if let Value::List(l) = &arg {
                    let result: Vec<Value> = l
                        .iter()
                        .enumerate()
                        .map(|(i, v)| Value::new_tuple(vec![Value::Int(i as i64), v.clone()]))
                        .collect();
                    Ok(Value::new_list(result))
                } else {
                    Ok(Value::new_list(vec![]))
                }
            }
            42 => {
                // CHUNK
                let size = self
                    .nb_to_int(self.registers[base + arg_reg + 1])
                    .unwrap_or(1) as usize;
                if let Value::List(l) = &arg {
                    let result: Vec<Value> = l
                        .chunks(size.max(1))
                        .map(|chunk| Value::new_list(chunk.to_vec()))
                        .collect();
                    Ok(Value::new_list(result))
                } else {
                    Ok(Value::new_list(vec![]))
                }
            }
            43 => {
                // WINDOW
                let n = self
                    .nb_to_int(self.registers[base + arg_reg + 1])
                    .unwrap_or(1) as usize;
                if let Value::List(l) = &arg {
                    if n == 0 || n > l.len() {
                        Ok(Value::new_list(vec![]))
                    } else {
                        let result: Vec<Value> =
                            l.windows(n).map(|w| Value::new_list(w.to_vec())).collect();
                        Ok(Value::new_list(result))
                    }
                } else {
                    Ok(Value::new_list(vec![]))
                }
            }
            46 => {
                // TAKE
                let n = self
                    .nb_to_int(self.registers[base + arg_reg + 1])
                    .unwrap_or(0) as usize;
                if let Value::List(l) = &arg {
                    Ok(Value::new_list(l.iter().take(n).cloned().collect()))
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
                    Ok(Value::new_list(l.iter().skip(n).cloned().collect()))
                } else {
                    Ok(arg)
                }
            }
            51 => {
                // CHARS
                let s = arg.as_string_resolved(&self.strings);
                Ok(Value::new_list(
                    s.chars()
                        .map(|c| Value::String(StringRef::Owned(c.to_string())))
                        .collect(),
                ))
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
                    Ok(Value::new_set_from_vec(l.to_vec()))
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
                    Self::nb_to_value(exp_nb)
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
                let lo = Self::nb_to_value(lo_nb);
                let hi = Self::nb_to_value(hi_nb);
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
                let other = Self::nb_to_value(self.registers[base + arg_reg + 1]);
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
                    let headers_val = Self::nb_to_value(self.registers[base + arg_reg + 3]);
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
                    Value::Map(m) => Value::new_list(
                        m.keys()
                            .map(|k| Value::String(StringRef::Owned(k.clone())))
                            .collect(),
                    ),
                    Value::Record(r) => Value::new_list(
                        r.fields
                            .keys()
                            .map(|k| Value::String(StringRef::Owned(k.clone())))
                            .collect(),
                    ),
                    _ => Value::new_list(vec![]),
                })
            }
            15 => {
                // VALUES
                Ok(match arg {
                    Value::Map(m) => Value::new_list(m.values().cloned().collect()),
                    Value::Record(r) => Value::new_list(r.fields.values().cloned().collect()),
                    _ => Value::new_list(vec![]),
                })
            }
            16 => {
                // CONTAINS
                let needle_nb = self.registers[base + arg_reg + 1];
                // Fast-path: int needle (common in numeric code)
                if needle_nb.is_int() {
                    let needle_val = Value::Int(needle_nb.as_int().unwrap_or(0));
                    let result = match &arg {
                        Value::List(l) => l.iter().any(|v| v == &needle_val),
                        Value::Set(s) => s.iter().any(|v| v == &needle_val),
                        _ => false,
                    };
                    return Ok(Value::Bool(result));
                }
                let needle = Self::nb_to_value(needle_nb);
                let result = match arg {
                    Value::List(l) => l.iter().any(|v| v == &needle),
                    Value::Set(s) => s.iter().any(|v| v == &needle),
                    Value::Map(m) => {
                        let needle_str = needle.as_string_resolved(&self.strings);
                        m.contains_key(&needle_str)
                    }
                    Value::String(sr) => {
                        let s = match sr {
                            StringRef::Owned(s) => s,
                            StringRef::Interned(id) => {
                                self.strings.resolve(id).unwrap_or("").to_string()
                            }
                        };
                        let needle_str = needle.as_string_resolved(&self.strings);
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
                        let joined = l
                            .iter()
                            .map(|v| v.display_pretty())
                            .collect::<Vec<_>>()
                            .join(&sep);
                        Value::String(StringRef::Owned(joined))
                    }
                    _ => Value::String(StringRef::Owned(String::new())),
                })
            }
            18 => {
                // SPLIT
                let sep = self.nb_to_string_resolved(self.registers[base + arg_reg + 1]);
                let s = arg.as_string_resolved(&self.strings);
                let parts: Vec<Value> = s
                    .split(&sep)
                    .map(|p| Value::String(StringRef::Owned(p.to_string())))
                    .collect();
                Ok(Value::new_list(parts))
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
                        Value::new_list(l[start..end].to_vec())
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
                    Arc::make_mut(&mut l).push(elem);
                    Ok(Value::List(l))
                } else {
                    Ok(Value::new_list(vec![elem]))
                }
            }
            25 => {
                // RANGE
                let start = arg.as_int().unwrap_or(0);
                let end = self
                    .nb_to_int(self.registers[base + arg_reg + 1])
                    .unwrap_or(0);
                let list: Vec<Value> = (start..end).map(Value::Int).collect();
                Ok(Value::new_list(list))
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
                        if let Value::List(inner) = item {
                            flat.extend(inner.iter().cloned());
                        } else {
                            flat.push(item.clone());
                        }
                    }
                    Ok(Value::new_list(flat))
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
                            seen.push(item.clone());
                        }
                    }
                    Ok(Value::new_list(seen))
                } else {
                    Ok(arg)
                }
            }
            48 => {
                // FIRST
                Ok(match arg {
                    Value::List(l) => l.first().cloned().unwrap_or(Value::Null),
                    _ => Value::Null,
                })
            }
            49 => {
                // LAST
                Ok(match arg {
                    Value::List(l) => l.last().cloned().unwrap_or(Value::Null),
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
                let other = Self::nb_to_value(self.registers[base + arg_reg + 1]);
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
        let closure_value = Self::nb_to_value(NbValue(closure_nb as u64));
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
            args.push(Self::nb_to_value(nb));
            nb.drop_heap();
        }
        match vm.call_closure_sync(&closure, &args) {
            Ok(result) => {
                let nb = value_to_nb(result);
                nb.0 as i64
            }
            Err(_) => NbValue::NAN_BOX_NULL as i64,
        }
    }
}

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

fn sort_list_homogeneous(items: &mut Vec<Value>) {
    if items.len() <= 1 {
        return;
    }
    match &items[0] {
        Value::Int(_) => {
            if items.iter().all(|v| matches!(v, Value::Int(_))) {
                items.sort_unstable_by(|lhs, rhs| match (lhs, rhs) {
                    (Value::Int(a), Value::Int(b)) => a.cmp(b),
                    _ => unreachable!(),
                });
                return;
            }
        }
        Value::Float(_) => {
            if items.iter().all(|v| matches!(v, Value::Float(_))) {
                items.sort_unstable_by(|lhs, rhs| match (lhs, rhs) {
                    (Value::Float(a), Value::Float(b)) => a.total_cmp(b),
                    _ => unreachable!(),
                });
                return;
            }
        }
        Value::Union(_) => {
            if let Some((tag, payload_kind)) = homogeneous_union_scalar_shape(items) {
                items.sort_by(|lhs, rhs| match (lhs, rhs) {
                    (Value::Union(a), Value::Union(b)) => {
                        debug_assert_eq!(a.tag, tag);
                        debug_assert_eq!(b.tag, tag);
                        cmp_union_payload_scalar(&a.payload, &b.payload, payload_kind)
                    }
                    _ => unreachable!(),
                });
                return;
            }
        }
        Value::Record(_) => {
            if let Some(field_kinds) = homogeneous_record_scalar_shape(items) {
                items.sort_by(|lhs, rhs| match (lhs, rhs) {
                    (Value::Record(a), Value::Record(b)) => {
                        cmp_record_scalar_fields(a, b, &field_kinds)
                    }
                    _ => unreachable!(),
                });
                return;
            }
        }
        _ => {}
    }
    items.sort();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScalarSortKind {
    Null,
    Bool,
    Int,
    Float,
}

#[inline]
fn scalar_sort_kind(value: &Value) -> Option<ScalarSortKind> {
    match value {
        Value::Null => Some(ScalarSortKind::Null),
        Value::Bool(_) => Some(ScalarSortKind::Bool),
        Value::Int(_) => Some(ScalarSortKind::Int),
        Value::Float(_) => Some(ScalarSortKind::Float),
        _ => None,
    }
}

#[inline]
fn cmp_scalar_value(lhs: &Value, rhs: &Value, kind: ScalarSortKind) -> Ordering {
    match kind {
        ScalarSortKind::Null => Ordering::Equal,
        ScalarSortKind::Bool => match (lhs, rhs) {
            (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
            _ => unreachable!(),
        },
        ScalarSortKind::Int => match (lhs, rhs) {
            (Value::Int(a), Value::Int(b)) => a.cmp(b),
            _ => unreachable!(),
        },
        ScalarSortKind::Float => match (lhs, rhs) {
            (Value::Float(a), Value::Float(b)) => a.total_cmp(b),
            _ => unreachable!(),
        },
    }
}

#[inline]
fn union_payload_sort_kind(payload: &UnionPayload) -> Option<ScalarSortKind> {
    match payload {
        UnionPayload::Null => Some(ScalarSortKind::Null),
        UnionPayload::Bool(_) => Some(ScalarSortKind::Bool),
        UnionPayload::Int(_) => Some(ScalarSortKind::Int),
        UnionPayload::Float(_) => Some(ScalarSortKind::Float),
        UnionPayload::Heap(_) => None,
    }
}

#[inline]
fn cmp_union_payload_scalar(
    lhs: &UnionPayload,
    rhs: &UnionPayload,
    payload_kind: ScalarSortKind,
) -> Ordering {
    match payload_kind {
        ScalarSortKind::Null => Ordering::Equal,
        ScalarSortKind::Bool => match (lhs, rhs) {
            (UnionPayload::Bool(a), UnionPayload::Bool(b)) => a.cmp(b),
            _ => unreachable!(),
        },
        ScalarSortKind::Int => match (lhs, rhs) {
            (UnionPayload::Int(a), UnionPayload::Int(b)) => a.cmp(b),
            _ => unreachable!(),
        },
        ScalarSortKind::Float => match (lhs, rhs) {
            (UnionPayload::Float(a), UnionPayload::Float(b)) => a.total_cmp(b),
            _ => unreachable!(),
        },
    }
}

fn homogeneous_union_scalar_shape(items: &[Value]) -> Option<(u32, ScalarSortKind)> {
    let first = match items.first()? {
        Value::Union(u) => u,
        _ => return None,
    };
    let payload_kind = union_payload_sort_kind(&first.payload)?;
    for item in items.iter().skip(1) {
        let union = match item {
            Value::Union(u) => u,
            _ => return None,
        };
        if union.tag != first.tag || union_payload_sort_kind(&union.payload) != Some(payload_kind) {
            return None;
        }
    }
    Some((first.tag, payload_kind))
}

fn homogeneous_record_scalar_shape(items: &[Value]) -> Option<Vec<ScalarSortKind>> {
    let first = match items.first()? {
        Value::Record(record) => record,
        _ => return None,
    };
    let mut field_kinds = Vec::with_capacity(first.fields.len());
    for value in first.fields.values() {
        field_kinds.push(scalar_sort_kind(value)?);
    }
    for item in items.iter().skip(1) {
        let record = match item {
            Value::Record(record) => record,
            _ => return None,
        };
        if record.type_name != first.type_name || record.fields.len() != field_kinds.len() {
            return None;
        }
        for (((first_key, _), (record_key, record_val)), expected_kind) in first
            .fields
            .iter()
            .zip(record.fields.iter())
            .zip(field_kinds.iter())
        {
            if first_key != record_key || scalar_sort_kind(record_val) != Some(*expected_kind) {
                return None;
            }
        }
    }
    Some(field_kinds)
}

#[inline]
fn cmp_record_scalar_fields(
    lhs: &lumen_core::values::RecordValue,
    rhs: &lumen_core::values::RecordValue,
    field_kinds: &[ScalarSortKind],
) -> Ordering {
    debug_assert_eq!(lhs.type_name, rhs.type_name);
    debug_assert_eq!(lhs.fields.len(), field_kinds.len());
    debug_assert_eq!(rhs.fields.len(), field_kinds.len());
    for ((lhs_val, rhs_val), field_kind) in lhs
        .fields
        .values()
        .zip(rhs.fields.values())
        .zip(field_kinds.iter())
    {
        let ord = cmp_scalar_value(lhs_val, rhs_val, *field_kind);
        if ord != Ordering::Equal {
            return ord;
        }
    }
    Ordering::Equal
}

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
                "Int" => matches!(val, Value::Int(_)),
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
    use lumen_core::values::{RecordValue, UnionValue};

    #[test]
    fn sort_union_scalar_payload_matches_value_ord() {
        let mut actual = vec![
            Value::Union(UnionValue {
                tag: 42,
                payload: UnionPayload::Int(4),
            }),
            Value::Union(UnionValue {
                tag: 42,
                payload: UnionPayload::Int(-7),
            }),
            Value::Union(UnionValue {
                tag: 42,
                payload: UnionPayload::Int(0),
            }),
        ];
        let mut expected = actual.clone();
        expected.sort();

        sort_list_homogeneous(&mut actual);

        assert_eq!(actual, expected);
    }

    #[test]
    fn sort_record_scalar_shape_matches_value_ord() {
        fn mk_record(age: i64, alive: bool, score: f64) -> Value {
            let mut fields = BTreeMap::new();
            fields.insert("age".to_string(), Value::Int(age));
            fields.insert("alive".to_string(), Value::Bool(alive));
            fields.insert("score".to_string(), Value::Float(score));
            Value::Record(Arc::new(RecordValue {
                type_name: "Node".to_string(),
                fields,
            }))
        }

        let mut actual = vec![
            mk_record(3, true, 8.0),
            mk_record(3, false, 9.0),
            mk_record(1, true, 7.5),
        ];
        let mut expected = actual.clone();
        expected.sort();

        sort_list_homogeneous(&mut actual);

        assert_eq!(actual, expected);
    }

    #[test]
    fn sort_record_shape_mismatch_falls_back_to_value_ord() {
        fn mk_record(type_name: &str, key: &str, value: Value) -> Value {
            let mut fields = BTreeMap::new();
            fields.insert(key.to_string(), value);
            Value::Record(Arc::new(RecordValue {
                type_name: type_name.to_string(),
                fields,
            }))
        }

        let mut actual = vec![
            mk_record("Node", "left", Value::Int(1)),
            mk_record("Node", "right", Value::Int(0)),
            mk_record("Other", "left", Value::Int(2)),
        ];
        let mut expected = actual.clone();
        expected.sort();

        sort_list_homogeneous(&mut actual);

        assert_eq!(actual, expected);
    }
}
