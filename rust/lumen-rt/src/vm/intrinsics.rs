//! Builtin function dispatch, intrinsic opcodes, and closure calls for the VM.

use super::*;
use crate::json_parser::parse_json_optimized;
use num_bigint::BigInt;
use num_traits::{Signed, ToPrimitive};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

impl VM {
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
                    let val = self.registers[base + a + 1 + i].peek_legacy();
                    parts.push(val.display_pretty());
                }
                let output = parts.join(" ");
                println!("{}", output);
                self.output.push(output);
                Ok(Value::Null)
            }
            "len" | "length" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(match arg {
                    Value::String(StringRef::Owned(s)) => Value::Int(s.len() as i64),
                    Value::String(StringRef::Interned(id)) => {
                        let s = self.strings.resolve(id).unwrap_or("");
                        Value::Int(s.len() as i64)
                    }
                    Value::List(l) => Value::Int(l.len() as i64),
                    Value::Map(m) => Value::Int(m.len() as i64),
                    Value::Tuple(t) => Value::Int(t.len() as i64),
                    Value::Set(s) => Value::Int(s.len() as i64),
                    Value::Bytes(b) => Value::Int(b.len() as i64),
                    _ => Value::Int(0),
                })
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
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(Value::String(StringRef::Owned(arg.display_pretty())))
            }
            "to_int" | "int" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(match arg {
                    Value::Int(n) => Value::Int(n),
                    Value::BigInt(n) => Value::BigInt(n.clone()),
                    Value::Float(f) => Value::Int(f as i64),
                    Value::String(sr) => {
                        let s = match sr {
                            StringRef::Owned(s) => s,
                            StringRef::Interned(id) => {
                                self.strings.resolve(id).unwrap_or("").to_string()
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
                    Value::Bool(b) => Value::Int(if b { 1 } else { 0 }),
                    _ => Value::Null,
                })
            }
            "to_float" | "float" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(match arg {
                    Value::Float(f) => Value::Float(f),
                    Value::Int(n) => Value::Float(n as f64),
                    Value::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::NAN)),
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
            "type_of" | "type" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(Value::String(StringRef::Owned(arg.type_name().to_string())))
            }
            "keys" => {
                let arg = self.registers[base + a + 1].peek_legacy();
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
            "values" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(match arg {
                    Value::Map(m) => Value::new_list(m.values().cloned().collect()),
                    Value::Record(r) => Value::new_list(r.fields.values().cloned().collect()),
                    _ => Value::new_list(vec![]),
                })
            }
            "contains" | "has" => {
                let collection = self.registers[base + a + 1].peek_legacy();
                let needle = self.registers[base + a + 2].peek_legacy();
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
                let list = self.registers[base + a + 1].peek_legacy();
                let sep = if nargs > 1 {
                    self.registers[base + a + 2]
                        .peek_legacy()
                        .as_string_resolved(&self.strings)
                } else {
                    ", ".to_string()
                };
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
                let s = self.registers[base + a + 1]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                let sep = if nargs > 1 {
                    self.registers[base + a + 2]
                        .peek_legacy()
                        .as_string_resolved(&self.strings)
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
                let s = self.registers[base + a + 1]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                Ok(Value::String(StringRef::Owned(s.trim().to_string())))
            }
            "upper" => {
                let s = self.registers[base + a + 1]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                Ok(Value::String(StringRef::Owned(s.to_uppercase())))
            }
            "lower" => {
                let s = self.registers[base + a + 1]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                Ok(Value::String(StringRef::Owned(s.to_lowercase())))
            }
            "replace" => {
                let s = self.registers[base + a + 1]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                let from = self.registers[base + a + 2]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                let to = self.registers[base + a + 3]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                Ok(Value::String(StringRef::Owned(s.replace(&from, &to))))
            }
            "abs" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(match arg {
                    Value::Int(n) => Value::Int(n.abs()),
                    Value::BigInt(n) => Value::BigInt(n.abs()),
                    Value::Float(f) => Value::Float(f.abs()),
                    _ => arg,
                })
            }
            "min" => {
                let lhs = self.registers[base + a + 1].peek_legacy();
                let rhs = self.registers[base + a + 2].peek_legacy();
                Ok(match (&lhs, &rhs) {
                    (Value::Int(x), Value::Int(y)) => Value::Int(*x.min(y)),
                    (Value::Float(x), Value::Float(y)) => Value::Float(x.min(*y)),
                    _ => lhs,
                })
            }
            "max" => {
                let lhs = self.registers[base + a + 1].peek_legacy();
                let rhs = self.registers[base + a + 2].peek_legacy();
                Ok(match (&lhs, &rhs) {
                    (Value::Int(x), Value::Int(y)) => Value::Int(*x.max(y)),
                    (Value::Float(x), Value::Float(y)) => Value::Float(x.max(*y)),
                    _ => lhs,
                })
            }
            "range" => {
                let start = self.registers[base + a + 1]
                    .peek_legacy()
                    .as_int()
                    .unwrap_or(0);
                let end = self.registers[base + a + 2]
                    .peek_legacy()
                    .as_int()
                    .unwrap_or(0);
                let list: Vec<Value> = (start..end).map(Value::Int).collect();
                Ok(Value::new_list(list))
            }
            "spawn" => {
                if nargs == 0 {
                    return Err(VmError::TypeError(
                        "spawn requires a callable argument".to_string(),
                    ));
                }
                let callee = self.registers[base + a + 1].peek_legacy();
                let args: Vec<Value> = (1..nargs)
                    .map(|i| self.registers[base + a + 1 + i].peek_legacy())
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
                let arg = self.registers[base + a + 1].peek_legacy();
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
                let val = self.registers[base + a + 1].peek_legacy();
                let owned = match &val {
                    Value::String(StringRef::Owned(_)) | Value::String(StringRef::Interned(_)) => {
                        None
                    }
                    _ => Some(val.as_string_resolved(&self.strings)),
                };
                let bytes = match &val {
                    Value::String(StringRef::Owned(s)) => s.as_bytes(),
                    Value::String(StringRef::Interned(id)) => {
                        self.strings.resolve(*id).unwrap_or("").as_bytes()
                    }
                    _ => owned.as_ref().unwrap().as_bytes(),
                };
                let h = format!("sha256:{:x}", Sha256::digest(bytes));
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
                let arg = self.registers[base + a + 1].peek_legacy();
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
                let arg = self.registers[base + a + 1].peek_legacy();
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
                let arg = self.registers[base + a + 1].peek_legacy();
                let n = self.registers[base + a + 2]
                    .peek_legacy()
                    .as_int()
                    .unwrap_or(0) as usize;
                if let Value::List(l) = arg {
                    Ok(Value::new_list(l.iter().take(n).cloned().collect()))
                } else {
                    Ok(arg)
                }
            }
            "drop" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                let n = self.registers[base + a + 2]
                    .peek_legacy()
                    .as_int()
                    .unwrap_or(0) as usize;
                if let Value::List(l) = arg {
                    Ok(Value::new_list(l.iter().skip(n).cloned().collect()))
                } else {
                    Ok(arg)
                }
            }
            "first" | "head" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(match arg {
                    Value::List(l) => l.first().cloned().unwrap_or(Value::Null),
                    Value::Tuple(t) => t.first().cloned().unwrap_or(Value::Null),
                    _ => Value::Null,
                })
            }
            "last" | "tail" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(match arg {
                    Value::List(l) => l.last().cloned().unwrap_or(Value::Null),
                    Value::Tuple(t) => t.last().cloned().unwrap_or(Value::Null),
                    _ => Value::Null,
                })
            }
            "is_empty" | "empty" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                let empty = match arg {
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
                let s = self.registers[base + a + 1]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                Ok(Value::new_list(
                    s.chars()
                        .map(|c| Value::String(StringRef::Owned(c.to_string())))
                        .collect(),
                ))
            }
            "starts_with" => {
                let s = self.registers[base + a + 1]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                let prefix = self.registers[base + a + 2]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                Ok(Value::Bool(s.starts_with(&prefix)))
            }
            "ends_with" => {
                let s = self.registers[base + a + 1]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                let suffix = self.registers[base + a + 2]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                Ok(Value::Bool(s.ends_with(&suffix)))
            }
            "index_of" => {
                let s = self.registers[base + a + 1]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                let needle = self.registers[base + a + 2]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                Ok(match s.find(&needle) {
                    Some(i) => {
                        let char_idx = s[..i].chars().count();
                        Value::Int(char_idx as i64)
                    }
                    None => Value::Int(-1),
                })
            }
            "pad_left" => {
                let s = self.registers[base + a + 1]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                let width = self.registers[base + a + 2]
                    .peek_legacy()
                    .as_int()
                    .unwrap_or(0) as usize;
                let char_count = s.chars().count();
                if char_count < width {
                    let padding = " ".repeat(width - char_count);
                    Ok(Value::String(StringRef::Owned(format!("{}{}", padding, s))))
                } else {
                    Ok(Value::String(StringRef::Owned(s)))
                }
            }
            "pad_right" => {
                let s = self.registers[base + a + 1]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                let width = self.registers[base + a + 2]
                    .peek_legacy()
                    .as_int()
                    .unwrap_or(0) as usize;
                let char_count = s.chars().count();
                if char_count < width {
                    let padding = " ".repeat(width - char_count);
                    Ok(Value::String(StringRef::Owned(format!("{}{}", s, padding))))
                } else {
                    Ok(Value::String(StringRef::Owned(s)))
                }
            }
            "round" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.round()),
                    _ => arg,
                })
            }
            "ceil" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.ceil()),
                    _ => arg,
                })
            }
            "floor" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.floor()),
                    _ => arg,
                })
            }
            "sqrt" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.sqrt()),
                    Value::Int(n) => Value::Float((n as f64).sqrt()),
                    Value::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::INFINITY).sqrt()),
                    _ => Value::Null,
                })
            }
            "pow" => {
                let base_val = self.registers[base + a + 1].peek_legacy();
                let exp_val = self.registers[base + a + 2].peek_legacy();
                Ok(match (base_val, exp_val) {
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
            "log" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.ln()),
                    Value::Int(n) => Value::Float((n as f64).ln()),
                    _ => Value::Null,
                })
            }
            "sin" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.sin()),
                    Value::Int(n) => Value::Float((n as f64).sin()),
                    _ => Value::Null,
                })
            }
            "cos" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.cos()),
                    Value::Int(n) => Value::Float((n as f64).cos()),
                    _ => Value::Null,
                })
            }
            "tan" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.tan()),
                    Value::Int(n) => Value::Float((n as f64).tan()),
                    _ => Value::Null,
                })
            }
            "exp" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.exp()),
                    Value::Int(n) => Value::Float((n as f64).exp()),
                    _ => Value::Null,
                })
            }
            "floor" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.floor()),
                    Value::Int(n) => Value::Int(n),
                    _ => Value::Null,
                })
            }
            "ceil" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.ceil()),
                    Value::Int(n) => Value::Int(n),
                    _ => Value::Null,
                })
            }
            "round" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.round()),
                    Value::Int(n) => Value::Int(n),
                    _ => Value::Null,
                })
            }
            "trim" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                let s = arg.as_string_resolved(&self.strings);
                Ok(Value::String(StringRef::Owned(s.trim().to_string())))
            }
            "trim_start" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                let s = arg.as_string_resolved(&self.strings);
                Ok(Value::String(StringRef::Owned(s.trim_start().to_string())))
            }
            "trim_end" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                let s = arg.as_string_resolved(&self.strings);
                Ok(Value::String(StringRef::Owned(s.trim_end().to_string())))
            }
            "hex_decode" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                let s = arg.as_string_resolved(&self.strings);
                if s.len() % 2 != 0 {
                    return Ok(Value::Null);
                }
                if !s.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Ok(Value::Null);
                }
                let bytes: Vec<u8> = (0..s.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap_or(0))
                    .collect();
                Ok(Value::Bytes(bytes.into()))
            }
            "hex_encode" => {
                let arg = self.registers[base + a + 1].peek_legacy();
                let owned = match &arg {
                    Value::String(StringRef::Owned(_)) | Value::String(StringRef::Interned(_)) => {
                        None
                    }
                    _ => Some(arg.as_string_resolved(&self.strings)),
                };
                let bytes = match &arg {
                    Value::String(StringRef::Owned(s)) => s.as_bytes(),
                    Value::String(StringRef::Interned(id)) => {
                        self.strings.resolve(*id).unwrap_or("").as_bytes()
                    }
                    _ => owned.as_ref().unwrap().as_bytes(),
                };
                let hex = bytes
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<String>();
                Ok(Value::String(StringRef::Owned(hex)))
            }
            "ends_with" => {
                let s = self.registers[base + a + 1]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                let suffix = self.registers[base + a + 2]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                Ok(Value::Bool(s.ends_with(&suffix)))
            }
            "pad_left" => {
                let s = self.registers[base + a + 1]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                let len = match self.registers[base + a + 2].peek_legacy() {
                    Value::Int(n) => n as usize,
                    _ => return Ok(Value::Null),
                };
                if s.len() >= len {
                    return Ok(Value::String(StringRef::Owned(s)));
                }
                let pad = " ".repeat(len - s.len());
                Ok(Value::String(StringRef::Owned(pad + &s)))
            }
            "pad_right" => {
                let s = self.registers[base + a + 1]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                let len = match self.registers[base + a + 2].peek_legacy() {
                    Value::Int(n) => n as usize,
                    _ => return Ok(Value::Null),
                };
                if s.len() >= len {
                    return Ok(Value::String(StringRef::Owned(s)));
                }
                let pad = " ".repeat(len - s.len());
                Ok(Value::String(StringRef::Owned(s + &pad)))
            }
            "clamp" => {
                let val = self.registers[base + a + 1].peek_legacy();
                let lo = self.registers[base + a + 2].peek_legacy();
                let hi = self.registers[base + a + 3].peek_legacy();
                Ok(match (val, lo, hi) {
                    (Value::Int(v), Value::Int(l), Value::Int(h)) => Value::Int(v.max(l).min(h)),
                    (Value::Float(v), Value::Float(l), Value::Float(h)) => {
                        Value::Float(v.max(l).min(h))
                    }
                    (v, _, _) => v,
                })
            }
            "json_parse" | "parse_json" => {
                let s = self.registers[base + a + 1]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                match parse_json_optimized(&s) {
                    Ok(v) => Ok(v),
                    Err(_) => Ok(Value::Null),
                }
            }
            "json_encode" | "to_json" => {
                let val = self.registers[base + a + 1].peek_legacy();
                let j = helpers::value_to_json(&val, &self.strings);
                Ok(Value::String(StringRef::Owned(j.to_string())))
            }
            "json_pretty" => {
                let val = self.registers[base + a + 1].peek_legacy();
                let j = helpers::value_to_json(&val, &self.strings);
                let pretty = serde_json::to_string_pretty(&j)
                    .map_err(|e| VmError::Runtime(format!("json_pretty failed: {}", e)))?;
                Ok(Value::String(StringRef::Owned(pretty)))
            }
            "read_file" => {
                let path = self.registers[base + a + 1]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                match std::fs::read_to_string(path) {
                    Ok(contents) => Ok(Value::String(StringRef::Owned(contents))),
                    Err(e) => Err(VmError::Runtime(format!("read_file failed: {}", e))),
                }
            }
            "write_file" => {
                let path = self.registers[base + a + 1]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                let content = self.registers[base + a + 2].peek_legacy();
                let owned = match &content {
                    Value::String(StringRef::Owned(_)) | Value::String(StringRef::Interned(_)) => {
                        None
                    }
                    _ => Some(content.as_string_resolved(&self.strings)),
                };
                let bytes = match &content {
                    Value::String(StringRef::Owned(s)) => s.as_bytes(),
                    Value::String(StringRef::Interned(id)) => {
                        self.strings.resolve(*id).unwrap_or("").as_bytes()
                    }
                    _ => owned.as_ref().unwrap().as_bytes(),
                };
                match std::fs::write(path, bytes) {
                    Ok(()) => Ok(Value::Null),
                    Err(e) => Err(VmError::Runtime(format!("write_file failed: {}", e))),
                }
            }
            "get_env" => {
                let name = self.registers[base + a + 1]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
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
                let min = match self.registers[base + a + 1].peek_legacy() {
                    Value::Int(n) => n,
                    _ => 0,
                };
                let max = match self.registers[base + a + 2].peek_legacy() {
                    Value::Int(n) => n,
                    _ => i64::MAX,
                };
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
                    self.registers[base + a + 1].peek_legacy().display_pretty()
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
        let arg = self.registers[base + arg_reg].peek_legacy();
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
                let other = self.registers[base + arg_reg + 1].peek_legacy();
                Ok(self.diff_values(&arg, &other))
            }
            5 => {
                // PATCH
                let patches = self.registers[base + arg_reg + 1].peek_legacy();
                Ok(self.patch_value(&arg, &patches))
            }
            6 => {
                // REDACT
                let fields = self.registers[base + arg_reg + 1].peek_legacy();
                Ok(self.redact_value(&arg, &fields))
            }
            7 => {
                // VALIDATE
                let nargs = if arg_reg == 0 { 0 } else { 1 }; // Simplified arity detection for intrinsic
                if nargs < 1 {
                    Ok(Value::Bool(!matches!(arg, Value::Null)))
                } else {
                    let schema_val = self.registers[base + arg_reg + 1].peek_legacy();
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
            52 => {
                // ENDS_WITH
                let s = arg.as_string_resolved(&self.strings);
                let suffix = self.registers[base + arg_reg + 1]
                    .peek_legacy()
                    .as_string_resolved(&self.strings);
                Ok(Value::Bool(s.ends_with(&suffix)))
            }
            55 => {
                // PAD_LEFT
                let s = arg.as_string_resolved(&self.strings);
                let len = match self.registers[base + arg_reg + 1].peek_legacy() {
                    Value::Int(n) => n as usize,
                    _ => return Ok(Value::Null),
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
                let len = match self.registers[base + arg_reg + 1].peek_legacy() {
                    Value::Int(n) => n as usize,
                    _ => return Ok(Value::Null),
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
                let exp = self.registers[base + arg_reg + 1].peek_legacy();
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
                let lo = self.registers[base + arg_reg + 1].peek_legacy();
                let hi = self.registers[base + arg_reg + 2].peek_legacy();
                Ok(match (arg, lo, hi) {
                    (Value::Int(v), Value::Int(l), Value::Int(h)) => Value::Int(v.max(l).min(h)),
                    (Value::Float(v), Value::Float(l), Value::Float(h)) => {
                        Value::Float(v.max(l).min(h))
                    }
                    (v, _, _) => v,
                })
            }
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
}

// ── Helper functions for intrinsics ──

fn sort_list_homogeneous(items: &mut Vec<Value>) {
    if items.len() <= 1 {
        return;
    }
    match items[0] {
        Value::Int(_) => {
            if items.iter().all(|v| matches!(v, Value::Int(_))) {
                let mut ints: Vec<i64> = items
                    .iter()
                    .map(|v| match v {
                        Value::Int(n) => *n,
                        _ => unreachable!(),
                    })
                    .collect();
                ints.sort_unstable();
                for (slot, n) in items.iter_mut().zip(ints) {
                    *slot = Value::Int(n);
                }
                return;
            }
        }
        Value::Float(_) => {
            if items.iter().all(|v| matches!(v, Value::Float(_))) {
                let mut floats: Vec<f64> = items
                    .iter()
                    .map(|v| match v {
                        Value::Float(f) => *f,
                        _ => unreachable!(),
                    })
                    .collect();
                floats.sort_unstable_by(f64::total_cmp);
                for (slot, f) in items.iter_mut().zip(floats) {
                    *slot = Value::Float(f);
                }
                return;
            }
        }
        _ => {}
    }
    items.sort();
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
