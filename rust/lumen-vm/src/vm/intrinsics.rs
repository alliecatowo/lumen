//! Builtin function dispatch, intrinsic opcodes, and closure calls for the VM.

use super::*;
use lumen_compiler::compile_raw;
use num_bigint::BigInt;
use num_traits::{Signed, ToPrimitive};
use std::collections::{BTreeMap, BTreeSet};
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
                    let val = &self.registers[base + a + 1 + i];
                    parts.push(val.display_pretty());
                }
                let output = parts.join(" ");
                println!("{}", output);
                self.output.push(output);
                Ok(Value::Null)
            }
            "len" | "length" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::String(StringRef::Owned(s)) => Value::Int(s.len() as i64),
                    Value::List(l) => Value::Int(l.len() as i64),
                    Value::Map(m) => Value::Int(m.len() as i64),
                    Value::Tuple(t) => Value::Int(t.len() as i64),
                    Value::Set(s) => Value::Int(s.len() as i64),
                    Value::Bytes(b) => Value::Int(b.len() as i64),
                    _ => Value::Int(0),
                })
            }
            "append" => {
                let list = self.registers[base + a + 1].clone();
                let elem = self.registers[base + a + 2].clone();
                if let Value::List(mut l) = list {
                    Arc::make_mut(&mut l).push(elem);
                    Ok(Value::List(l))
                } else {
                    Ok(Value::new_list(vec![elem]))
                }
            }
            "to_string" | "str" | "string" => {
                let arg = &self.registers[base + a + 1];
                Ok(Value::String(StringRef::Owned(arg.display_pretty())))
            }
            "to_int" | "int" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Int(n) => Value::Int(*n),
                    Value::BigInt(n) => Value::BigInt(n.clone()),
                    Value::Float(f) => Value::Int(*f as i64),
                    Value::String(StringRef::Owned(s)) => {
                        if let Ok(i) = s.parse::<i64>() {
                            Value::Int(i)
                        } else if let Ok(bi) = s.parse::<BigInt>() {
                            Value::BigInt(bi)
                        } else {
                            Value::Null
                        }
                    }
                    Value::Bool(b) => Value::Int(if *b { 1 } else { 0 }),
                    _ => Value::Null,
                })
            }
            "to_float" | "float" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(*f),
                    Value::Int(n) => Value::Float(*n as f64),
                    Value::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::NAN)),
                    Value::String(StringRef::Owned(s)) => {
                        s.parse::<f64>().map(Value::Float).unwrap_or(Value::Null)
                    }
                    _ => Value::Null,
                })
            }
            "type_of" | "type" => {
                let arg = &self.registers[base + a + 1];
                Ok(Value::String(StringRef::Owned(arg.type_name().to_string())))
            }
            "keys" => {
                let arg = &self.registers[base + a + 1];
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
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Map(m) => Value::new_list(m.values().cloned().collect()),
                    Value::Record(r) => Value::new_list(r.fields.values().cloned().collect()),
                    _ => Value::new_list(vec![]),
                })
            }
            "contains" | "has" => {
                let collection = &self.registers[base + a + 1];
                let needle = &self.registers[base + a + 2];
                let result = match collection {
                    Value::List(l) => l.iter().any(|v| v == needle),
                    Value::Set(s) => s.iter().any(|v| v == needle),
                    Value::Map(m) => m.contains_key(&needle.as_string()),
                    Value::String(StringRef::Owned(s)) => s.contains(&needle.as_string()),
                    _ => false,
                };
                Ok(Value::Bool(result))
            }
            "join" => {
                let list = &self.registers[base + a + 1];
                let sep = if nargs > 1 {
                    self.registers[base + a + 2].as_string()
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
                let s = self.registers[base + a + 1].as_string();
                let sep = if nargs > 1 {
                    self.registers[base + a + 2].as_string()
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
                let s = self.registers[base + a + 1].as_string();
                Ok(Value::String(StringRef::Owned(s.trim().to_string())))
            }
            "upper" => {
                let s = self.registers[base + a + 1].as_string();
                Ok(Value::String(StringRef::Owned(s.to_uppercase())))
            }
            "lower" => {
                let s = self.registers[base + a + 1].as_string();
                Ok(Value::String(StringRef::Owned(s.to_lowercase())))
            }
            "replace" => {
                let s = self.registers[base + a + 1].as_string();
                let from = self.registers[base + a + 2].as_string();
                let to = self.registers[base + a + 3].as_string();
                Ok(Value::String(StringRef::Owned(s.replace(&from, &to))))
            }
            "abs" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Int(n) => Value::Int(n.abs()),
                    Value::BigInt(n) => Value::BigInt(n.abs()),
                    Value::Float(f) => Value::Float(f.abs()),
                    _ => arg.clone(),
                })
            }
            "min" => {
                let lhs = &self.registers[base + a + 1];
                let rhs = &self.registers[base + a + 2];
                Ok(match (lhs, rhs) {
                    (Value::Int(x), Value::Int(y)) => Value::Int(*x.min(y)),
                    (Value::Float(x), Value::Float(y)) => Value::Float(x.min(*y)),
                    _ => lhs.clone(),
                })
            }
            "max" => {
                let lhs = &self.registers[base + a + 1];
                let rhs = &self.registers[base + a + 2];
                Ok(match (lhs, rhs) {
                    (Value::Int(x), Value::Int(y)) => Value::Int(*x.max(y)),
                    (Value::Float(x), Value::Float(y)) => Value::Float(x.max(*y)),
                    _ => lhs.clone(),
                })
            }
            "range" => {
                let start = self.registers[base + a + 1].as_int().unwrap_or(0);
                let end = self.registers[base + a + 2].as_int().unwrap_or(0);
                let list: Vec<Value> = (start..end).map(Value::Int).collect();
                Ok(Value::new_list(list))
            }
            "spawn" => {
                if nargs == 0 {
                    return Err(VmError::TypeError(
                        "spawn requires a callable argument".to_string(),
                    ));
                }
                let callee = self.registers[base + a + 1].clone();
                let args: Vec<Value> = (1..nargs)
                    .map(|i| self.registers[base + a + 1 + i].clone())
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
                let arg = &self.registers[base + a + 1];
                match arg {
                    Value::Future(f) => match self.future_states.get(&f.id) {
                        Some(FutureState::Completed(v)) => Ok(v.clone()),
                        Some(FutureState::Pending) => Ok(Value::Null),
                        Some(FutureState::Error(msg)) => {
                            Err(VmError::Runtime(format!("timeout target failed: {}", msg)))
                        }
                        None => Ok(Value::Null),
                    },
                    other => Ok(other.clone()),
                }
            }
            "hash" | "sha256" => {
                use sha2::{Digest, Sha256};
                let s = self.registers[base + a + 1].as_string();
                let h = format!("sha256:{:x}", Sha256::digest(s.as_bytes()));
                Ok(Value::String(StringRef::Owned(h)))
            }
            // Collection ops
            "sort" => {
                let arg = self.registers[base + a + 1].clone();
                if let Value::List(mut l) = arg {
                    Arc::make_mut(&mut l).sort();
                    Ok(Value::List(l))
                } else {
                    Ok(arg)
                }
            }
            "reverse" => {
                let arg = self.registers[base + a + 1].clone();
                if let Value::List(mut l) = arg {
                    Arc::make_mut(&mut l).reverse();
                    Ok(Value::List(l))
                } else {
                    Ok(arg)
                }
            }
            "flatten" => {
                let arg = &self.registers[base + a + 1];
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
                    Ok(arg.clone())
                }
            }
            "unique" => {
                let arg = &self.registers[base + a + 1];
                if let Value::List(l) = arg {
                    let mut result = Vec::new();
                    for item in l.iter() {
                        if !result.contains(item) {
                            result.push(item.clone());
                        }
                    }
                    Ok(Value::new_list(result))
                } else {
                    Ok(arg.clone())
                }
            }
            "take" => {
                let arg = &self.registers[base + a + 1];
                let n = self.registers[base + a + 2].as_int().unwrap_or(0) as usize;
                if let Value::List(l) = arg {
                    Ok(Value::new_list(l.iter().take(n).cloned().collect()))
                } else {
                    Ok(arg.clone())
                }
            }
            "drop" => {
                let arg = &self.registers[base + a + 1];
                let n = self.registers[base + a + 2].as_int().unwrap_or(0) as usize;
                if let Value::List(l) = arg {
                    Ok(Value::new_list(l.iter().skip(n).cloned().collect()))
                } else {
                    Ok(arg.clone())
                }
            }
            "first" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::List(l) => l.first().cloned().unwrap_or(Value::Null),
                    Value::Tuple(t) => t.first().cloned().unwrap_or(Value::Null),
                    _ => Value::Null,
                })
            }
            "last" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::List(l) => l.last().cloned().unwrap_or(Value::Null),
                    Value::Tuple(t) => t.last().cloned().unwrap_or(Value::Null),
                    _ => Value::Null,
                })
            }
            "is_empty" => {
                let arg = &self.registers[base + a + 1];
                Ok(Value::Bool(match arg {
                    Value::List(l) => l.is_empty(),
                    Value::Map(m) => m.is_empty(),
                    Value::Set(s) => s.is_empty(),
                    Value::String(StringRef::Owned(s)) => s.is_empty(),
                    Value::Null => true,
                    _ => false,
                }))
            }
            "chars" => {
                let s = self.registers[base + a + 1].as_string();
                Ok(Value::new_list(
                    s.chars()
                        .map(|c| Value::String(StringRef::Owned(c.to_string())))
                        .collect(),
                ))
            }
            "starts_with" => {
                let s = self.registers[base + a + 1].as_string();
                let prefix = self.registers[base + a + 2].as_string();
                Ok(Value::Bool(s.starts_with(&prefix)))
            }
            "ends_with" => {
                let s = self.registers[base + a + 1].as_string();
                let suffix = self.registers[base + a + 2].as_string();
                Ok(Value::Bool(s.ends_with(&suffix)))
            }
            "index_of" => {
                let s = self.registers[base + a + 1].as_string();
                let needle = self.registers[base + a + 2].as_string();
                Ok(match s.find(&needle) {
                    Some(i) => Value::Int(i as i64),
                    None => Value::Int(-1),
                })
            }
            "pad_left" => {
                let s = self.registers[base + a + 1].as_string();
                let width = self.registers[base + a + 2].as_int().unwrap_or(0) as usize;
                let pad = if nargs > 2 {
                    self.registers[base + a + 3].as_string()
                } else {
                    " ".to_string()
                };
                let pad_char = pad.chars().next().unwrap_or(' ');
                if s.len() < width {
                    let padding: String = std::iter::repeat_n(pad_char, width - s.len()).collect();
                    Ok(Value::String(StringRef::Owned(format!("{}{}", padding, s))))
                } else {
                    Ok(Value::String(StringRef::Owned(s)))
                }
            }
            "pad_right" => {
                let s = self.registers[base + a + 1].as_string();
                let width = self.registers[base + a + 2].as_int().unwrap_or(0) as usize;
                let pad = if nargs > 2 {
                    self.registers[base + a + 3].as_string()
                } else {
                    " ".to_string()
                };
                let pad_char = pad.chars().next().unwrap_or(' ');
                if s.len() < width {
                    let padding: String = std::iter::repeat_n(pad_char, width - s.len()).collect();
                    Ok(Value::String(StringRef::Owned(format!("{}{}", s, padding))))
                } else {
                    Ok(Value::String(StringRef::Owned(s)))
                }
            }
            // Math
            "round" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.round()),
                    Value::BigInt(_) => arg.clone(), // Integers are already rounded
                    Value::Int(_) => arg.clone(),
                    _ => arg.clone(),
                })
            }
            "ceil" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.ceil()),
                    Value::BigInt(_) => arg.clone(),
                    Value::Int(_) => arg.clone(),
                    _ => arg.clone(),
                })
            }
            "floor" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.floor()),
                    Value::BigInt(_) => arg.clone(),
                    Value::Int(_) => arg.clone(),
                    _ => arg.clone(),
                })
            }
            "sqrt" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.sqrt()),
                    Value::Int(n) => Value::Float((*n as f64).sqrt()),
                    Value::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::NAN).sqrt()),
                    _ => Value::Null,
                })
            }
            "pow" => {
                let b_val = &self.registers[base + a + 1];
                let e_val = &self.registers[base + a + 2];
                Ok(match (b_val, e_val) {
                    (Value::Int(x), Value::Int(y)) => {
                        if *y >= 0 {
                            if let Some(res) = x.checked_pow(*y as u32) {
                                Value::Int(res)
                            } else {
                                Value::BigInt(BigInt::from(*x).pow(*y as u32))
                            }
                        } else {
                            Value::Float((*x as f64).powf(*y as f64))
                        }
                    }
                    (Value::BigInt(x), Value::Int(y)) => {
                        if *y >= 0 {
                            Value::BigInt(x.pow(*y as u32))
                        } else {
                            Value::Float(x.to_f64().unwrap_or(f64::NAN).powf(*y as f64))
                        }
                    }
                    (Value::Int(x), Value::BigInt(y)) => {
                        // Huge exponent?
                        // If y fits in u32, we can pow. Else it's too big.
                        if let Some(exp) = y.to_u32() {
                            Value::BigInt(BigInt::from(*x).pow(exp))
                        } else {
                            // Too big. Infinity or zero?
                            // x ^ huge
                            Value::Float(f64::INFINITY) // Approximation
                        }
                    }
                    (Value::BigInt(x), Value::BigInt(y)) => {
                        if let Some(exp) = y.to_u32() {
                            Value::BigInt(x.pow(exp))
                        } else {
                            Value::Float(f64::INFINITY)
                        }
                    }
                    (Value::Float(x), Value::Float(y)) => Value::Float(x.powf(*y)),
                    (Value::Int(x), Value::Float(y)) => Value::Float((*x as f64).powf(*y)),
                    (Value::Float(x), Value::Int(y)) => Value::Float(x.powf(*y as f64)),
                    (Value::BigInt(x), Value::Float(y)) => {
                        Value::Float(x.to_f64().unwrap_or(f64::NAN).powf(*y))
                    }
                    (Value::Float(x), Value::BigInt(y)) => {
                        Value::Float(x.powf(y.to_f64().unwrap_or(f64::NAN)))
                    }
                    _ => Value::Null,
                })
            }
            "log" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.ln()),
                    Value::Int(n) => Value::Float((*n as f64).ln()),
                    Value::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::NAN).ln()),
                    _ => Value::Null,
                })
            }
            "sin" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.sin()),
                    Value::Int(n) => Value::Float((*n as f64).sin()),
                    Value::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::NAN).sin()),
                    _ => Value::Null,
                })
            }
            "cos" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.cos()),
                    Value::Int(n) => Value::Float((*n as f64).cos()),
                    Value::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::NAN).cos()),
                    _ => Value::Null,
                })
            }
            "clamp" => {
                let val = &self.registers[base + a + 1];
                let lo = &self.registers[base + a + 2];
                let hi = &self.registers[base + a + 3];
                Ok(match (val, lo, hi) {
                    (Value::Int(v), Value::Int(l), Value::Int(h)) => Value::Int(*v.max(l).min(h)),
                    (Value::BigInt(v), Value::BigInt(l), Value::BigInt(h)) => {
                        Value::BigInt(v.max(l).min(h).clone())
                    }
                    (Value::Float(v), Value::Float(l), Value::Float(h)) => {
                        Value::Float(v.max(*l).min(*h))
                    }
                    _ => val.clone(), // Mixed types in clamp? For now ignore.
                })
            }
            // Result type operations
            "is_ok" => {
                let arg = &self.registers[base + a + 1];
                Ok(Value::Bool(matches!(arg, Value::Union(u) if u.tag == "ok")))
            }
            "is_err" => {
                let arg = &self.registers[base + a + 1];
                Ok(Value::Bool(
                    matches!(arg, Value::Union(u) if u.tag == "err"),
                ))
            }
            "unwrap" => {
                let arg = &self.registers[base + a + 1];
                match arg {
                    Value::Union(u) if u.tag == "ok" => Ok(*u.payload.clone()),
                    Value::Union(u) if u.tag == "err" => {
                        Err(VmError::Runtime(format!("unwrap on err: {}", u.payload)))
                    }
                    _ => Ok(arg.clone()),
                }
            }
            "unwrap_or" => {
                let arg = &self.registers[base + a + 1];
                let default = self.registers[base + a + 2].clone();
                match arg {
                    Value::Union(u) if u.tag == "ok" => Ok(*u.payload.clone()),
                    _ => Ok(default),
                }
            }
            // Crypto
            "sha512" => {
                use sha2::{Digest, Sha512};
                let s = self.registers[base + a + 1].as_string();
                let h = format!("sha512:{:x}", Sha512::digest(s.as_bytes()));
                Ok(Value::String(StringRef::Owned(h)))
            }
            "uuid" | "uuid_v4" => {
                let id = uuid::Uuid::new_v4().to_string();
                Ok(Value::String(StringRef::Owned(id)))
            }
            "timestamp" => {
                let dur = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                Ok(Value::Float(dur.as_secs_f64()))
            }
            // Encoding
            "base64_encode" => {
                // Simple base64 implementation
                let s = self.registers[base + a + 1].as_string();
                Ok(Value::String(StringRef::Owned(simple_base64_encode(
                    s.as_bytes(),
                ))))
            }
            "base64_decode" => {
                let s = self.registers[base + a + 1].as_string();
                match simple_base64_decode(&s) {
                    Some(bytes) => Ok(Value::String(StringRef::Owned(
                        String::from_utf8_lossy(&bytes).to_string(),
                    ))),
                    None => Ok(Value::Null),
                }
            }
            "hex_encode" => {
                let s = self.registers[base + a + 1].as_string();
                let hex: String = s.bytes().map(|b| format!("{:02x}", b)).collect();
                Ok(Value::String(StringRef::Owned(hex)))
            }
            "hex_decode" => {
                let s = self.registers[base + a + 1].as_string();
                if !s.is_ascii() || !s.len().is_multiple_of(2) {
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
            "url_encode" => {
                let s = self.registers[base + a + 1].as_string();
                let mut encoded = String::new();
                for byte in s.bytes() {
                    if byte.is_ascii_alphanumeric()
                        || byte == b'-'
                        || byte == b'_'
                        || byte == b'.'
                        || byte == b'~'
                    {
                        encoded.push(byte as char);
                    } else {
                        encoded.push_str(&format!("%{:02X}", byte));
                    }
                }
                Ok(Value::String(StringRef::Owned(encoded)))
            }
            "url_decode" => {
                let s = self.registers[base + a + 1].as_string();
                let mut result = String::new();
                let mut chars = s.chars();
                while let Some(c) = chars.next() {
                    if c == '%' {
                        let hi = chars.next().unwrap_or('0');
                        let lo = chars.next().unwrap_or('0');
                        let hex = format!("{}{}", hi, lo);
                        if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                            result.push(byte as char);
                        }
                    } else if c == '+' {
                        result.push(' ');
                    } else {
                        result.push(c);
                    }
                }
                Ok(Value::String(StringRef::Owned(result)))
            }
            // JSON
            "json_parse" | "parse_json" => {
                let s = self.registers[base + a + 1].as_string();
                match serde_json::from_str::<serde_json::Value>(&s) {
                    Ok(v) => Ok(json_to_value(&v)),
                    Err(_) => Ok(Value::Null),
                }
            }
            "json_encode" | "to_json" => {
                let val = &self.registers[base + a + 1];
                let j = value_to_json(val);
                Ok(Value::String(StringRef::Owned(j.to_string())))
            }
            "json_pretty" => {
                let val = &self.registers[base + a + 1];
                let j = value_to_json(val);
                let pretty = serde_json::to_string_pretty(&j)
                    .map_err(|e| VmError::Runtime(format!("json_pretty failed: {}", e)))?;
                Ok(Value::String(StringRef::Owned(pretty)))
            }
            // String case transforms (std.string)
            "capitalize" => {
                let s = self.registers[base + a + 1].as_string();
                let mut c = s.chars();
                let result = match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().to_string() + &c.as_str().to_lowercase(),
                };
                Ok(Value::String(StringRef::Owned(result)))
            }
            "title_case" => {
                let s = self.registers[base + a + 1].as_string();
                let result: String = s
                    .split_whitespace()
                    .map(|word| {
                        let mut c = word.chars();
                        match c.next() {
                            None => String::new(),
                            Some(f) => f.to_uppercase().to_string() + &c.as_str().to_lowercase(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                Ok(Value::String(StringRef::Owned(result)))
            }
            "snake_case" => {
                let s = self.registers[base + a + 1].as_string();
                let mut result = String::new();
                for (i, ch) in s.chars().enumerate() {
                    if ch.is_uppercase() && i > 0 {
                        result.push('_');
                    }
                    result.push(ch.to_lowercase().next().unwrap_or(ch));
                }
                Ok(Value::String(StringRef::Owned(
                    result.replace(' ', "_").replace("__", "_"),
                )))
            }
            "camel_case" => {
                let s = self.registers[base + a + 1].as_string();
                let result: String = s
                    .split(['_', ' ', '-'])
                    .enumerate()
                    .map(|(i, word)| {
                        if i == 0 {
                            word.to_lowercase()
                        } else {
                            let mut c = word.chars();
                            match c.next() {
                                None => String::new(),
                                Some(f) => {
                                    f.to_uppercase().to_string() + &c.as_str().to_lowercase()
                                }
                            }
                        }
                    })
                    .collect();
                Ok(Value::String(StringRef::Owned(result)))
            }
            // Test assertions
            "assert" => {
                let arg = &self.registers[base + a + 1];
                if !arg.is_truthy() {
                    let msg = if nargs > 1 {
                        self.registers[base + a + 2].as_string()
                    } else {
                        "assertion failed".to_string()
                    };
                    return Err(VmError::Runtime(msg));
                }
                Ok(Value::Null)
            }
            "assert_eq" => {
                let lhs = &self.registers[base + a + 1];
                let rhs = &self.registers[base + a + 2];
                if lhs != rhs {
                    return Err(VmError::Runtime(format!(
                        "assert_eq failed: {} != {}",
                        lhs, rhs
                    )));
                }
                Ok(Value::Null)
            }
            "assert_ne" => {
                let lhs = &self.registers[base + a + 1];
                let rhs = &self.registers[base + a + 2];
                if lhs == rhs {
                    return Err(VmError::Runtime(format!(
                        "assert_ne failed: {} == {}",
                        lhs, rhs
                    )));
                }
                Ok(Value::Null)
            }
            "assert_contains" => {
                let collection = &self.registers[base + a + 1];
                let needle = &self.registers[base + a + 2];
                let found = match collection {
                    Value::List(l) => l.contains(needle),
                    Value::String(StringRef::Owned(s)) => s.contains(&needle.as_string()),
                    _ => false,
                };
                if !found {
                    return Err(VmError::Runtime(format!(
                        "assert_contains failed: {} not in {}",
                        needle, collection
                    )));
                }
                Ok(Value::Null)
            }
            // Emit/debug
            "emit" => {
                let val = self.registers[base + a + 1].display_pretty();
                println!("{}", val);
                self.output.push(val);
                Ok(Value::Null)
            }
            "debug" => {
                let val = &self.registers[base + a + 1];
                let output = format!("[debug] {:?}", val);
                eprintln!("{}", output);
                self.output.push(output);
                Ok(Value::Null)
            }
            "clone" => Ok(self.registers[base + a + 1].clone()),
            "sizeof" => {
                let val = &self.registers[base + a + 1];
                Ok(Value::Int(std::mem::size_of_val(val) as i64))
            }
            "enumerate" => {
                let arg = &self.registers[base + a + 1];
                if let Value::List(l) = arg {
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
            "zip" => {
                let a_list = &self.registers[base + a + 1];
                let b_list = &self.registers[base + a + 2];
                if let (Value::List(la), Value::List(lb)) = (a_list, b_list) {
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
            "chunk" => {
                let arg = &self.registers[base + a + 1];
                let size = self.registers[base + a + 2].as_int().unwrap_or(1) as usize;
                if let Value::List(l) = arg {
                    let result: Vec<Value> = l
                        .chunks(size.max(1))
                        .map(|chunk| Value::new_list(chunk.to_vec()))
                        .collect();
                    Ok(Value::new_list(result))
                } else {
                    Ok(Value::new_list(vec![]))
                }
            }
            "window" => {
                let arg = &self.registers[base + a + 1];
                let n = self.registers[base + a + 2].as_int().unwrap_or(1) as usize;
                if let Value::List(l) = arg {
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
            // Higher-order functions called by name
            "map" => {
                let list = self.registers[base + a + 1].clone();
                let closure_val = self.registers[base + a + 2].clone();
                if let (Value::List(l), Value::Closure(cv)) = (list, closure_val) {
                    let mut result = Vec::with_capacity(l.len());
                    for item in l.iter() {
                        let val = self.call_closure_sync(&cv, std::slice::from_ref(item))?;
                        result.push(val);
                    }
                    Ok(Value::new_list(result))
                } else {
                    Ok(Value::new_list(vec![]))
                }
            }
            "filter" => {
                let list = self.registers[base + a + 1].clone();
                let closure_val = self.registers[base + a + 2].clone();
                if let (Value::List(l), Value::Closure(cv)) = (list, closure_val) {
                    let mut result = Vec::new();
                    for item in l.iter() {
                        let val = self.call_closure_sync(&cv, std::slice::from_ref(item))?;
                        if val.is_truthy() {
                            result.push(item.clone());
                        }
                    }
                    Ok(Value::new_list(result))
                } else {
                    Ok(Value::new_list(vec![]))
                }
            }
            "reduce" => {
                let list = self.registers[base + a + 1].clone();
                let closure_val = self.registers[base + a + 2].clone();
                let init = self.registers[base + a + 3].clone();
                if let (Value::List(l), Value::Closure(cv)) = (list, closure_val) {
                    let mut acc = init;
                    for item in l.iter() {
                        acc = self.call_closure_sync(&cv, &[acc, item.clone()])?;
                    }
                    Ok(acc)
                } else {
                    Ok(init)
                }
            }
            "flat_map" => {
                let list = self.registers[base + a + 1].clone();
                let closure_val = self.registers[base + a + 2].clone();
                if let (Value::List(l), Value::Closure(cv)) = (list, closure_val) {
                    let mut result = Vec::new();
                    for item in l.iter() {
                        let val = self.call_closure_sync(&cv, std::slice::from_ref(item))?;
                        if let Value::List(inner) = val {
                            result.extend(inner.iter().cloned());
                        } else {
                            result.push(val);
                        }
                    }
                    Ok(Value::new_list(result))
                } else {
                    Ok(Value::new_list(vec![]))
                }
            }
            "any" => {
                let list = self.registers[base + a + 1].clone();
                let closure_val = self.registers[base + a + 2].clone();
                if let (Value::List(l), Value::Closure(cv)) = (list, closure_val) {
                    for item in l.iter() {
                        let val = self.call_closure_sync(&cv, std::slice::from_ref(item))?;
                        if val.is_truthy() {
                            return Ok(Value::Bool(true));
                        }
                    }
                    Ok(Value::Bool(false))
                } else {
                    Ok(Value::Bool(false))
                }
            }
            "all" => {
                let list = self.registers[base + a + 1].clone();
                let closure_val = self.registers[base + a + 2].clone();
                if let (Value::List(l), Value::Closure(cv)) = (list, closure_val) {
                    for item in l.iter() {
                        let val = self.call_closure_sync(&cv, std::slice::from_ref(item))?;
                        if !val.is_truthy() {
                            return Ok(Value::Bool(false));
                        }
                    }
                    Ok(Value::Bool(true))
                } else {
                    Ok(Value::Bool(true))
                }
            }
            "find" => {
                let list = self.registers[base + a + 1].clone();
                let closure_val = self.registers[base + a + 2].clone();
                if let (Value::List(l), Value::Closure(cv)) = (list, closure_val) {
                    for item in l.iter() {
                        let val = self.call_closure_sync(&cv, std::slice::from_ref(item))?;
                        if val.is_truthy() {
                            return Ok(item.clone());
                        }
                    }
                    Ok(Value::Null)
                } else {
                    Ok(Value::Null)
                }
            }
            "position" => {
                let list = self.registers[base + a + 1].clone();
                let closure_val = self.registers[base + a + 2].clone();
                if let (Value::List(l), Value::Closure(cv)) = (list, closure_val) {
                    for (i, item) in l.iter().enumerate() {
                        let val = self.call_closure_sync(&cv, std::slice::from_ref(item))?;
                        if val.is_truthy() {
                            return Ok(Value::Int(i as i64));
                        }
                    }
                    Ok(Value::Int(-1))
                } else {
                    Ok(Value::Int(-1))
                }
            }
            "group_by" => {
                let list = self.registers[base + a + 1].clone();
                let closure_val = self.registers[base + a + 2].clone();
                if let (Value::List(l), Value::Closure(cv)) = (list, closure_val) {
                    let mut groups: BTreeMap<String, Value> = BTreeMap::new();
                    for item in l.iter() {
                        let key = self.call_closure_sync(&cv, std::slice::from_ref(item))?;
                        let key_str = key.as_string();
                        match groups.get_mut(&key_str) {
                            Some(Value::List(ref mut list)) => {
                                Arc::make_mut(list).push(item.clone())
                            }
                            _ => {
                                groups.insert(key_str, Value::new_list(vec![item.clone()]));
                            }
                        }
                    }
                    Ok(Value::new_map(groups))
                } else {
                    Ok(Value::new_map(BTreeMap::new()))
                }
            }
            // Filesystem
            "read_file" => {
                let path = self.registers[base + a + 1].as_string();
                match std::fs::read_to_string(&path) {
                    Ok(contents) => Ok(Value::String(StringRef::Owned(contents))),
                    Err(e) => Err(VmError::Runtime(format!("read_file failed: {}", e))),
                }
            }
            "write_file" => {
                let path = self.registers[base + a + 1].as_string();
                let content = self.registers[base + a + 2].as_string();
                match std::fs::write(&path, &content) {
                    Ok(()) => Ok(Value::Null),
                    Err(e) => Err(VmError::Runtime(format!("write_file failed: {}", e))),
                }
            }
            // Random
            "random" => {
                use std::cell::Cell;
                thread_local! {
                    static RNG_STATE: Cell<u64> = const { Cell::new(0) };
                }
                RNG_STATE.with(|state| {
                    let mut s = state.get();
                    if s == 0 {
                        s = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos() as u64;
                        if s == 0 {
                            s = 1;
                        }
                    }
                    s ^= s << 13;
                    s ^= s >> 7;
                    s ^= s << 17;
                    state.set(s);
                    Ok(Value::Float((s >> 11) as f64 / ((1u64 << 53) as f64)))
                })
            }
            // Environment
            "get_env" => {
                let name = self.registers[base + a + 1].as_string();
                match std::env::var(&name) {
                    Ok(val) => Ok(Value::String(StringRef::Owned(val))),
                    Err(_) => Ok(Value::Null),
                }
            }
            "freeze" => Ok(self.registers[base + a + 1].clone()),
            "format" => {
                let template = self.registers[base + a + 1].as_string();
                let mut result = String::new();
                let mut arg_idx = 0;
                let mut chars = template.chars().peekable();
                while let Some(ch) = chars.next() {
                    if ch == '{' && chars.peek() == Some(&'}') {
                        chars.next();
                        if arg_idx < nargs - 1 {
                            result
                                .push_str(&self.registers[base + a + 2 + arg_idx].display_pretty());
                            arg_idx += 1;
                        } else {
                            result.push_str("{}");
                        }
                    } else {
                        result.push(ch);
                    }
                }
                Ok(Value::String(StringRef::Owned(result)))
            }
            "partition" => {
                let list = self.registers[base + a].clone();
                let predicate = self.registers[base + a + 1].clone();

                let closure_opt = match predicate {
                    Value::Closure(cv) => Some(cv),
                    Value::String(ref s) => {
                        let name_str = match s {
                            StringRef::Owned(s) => s.as_str(),
                            StringRef::Interned(id) => self.strings.resolve(*id).unwrap_or(""),
                        };
                        let module = self.module.as_ref().ok_or(VmError::NoModule)?;
                        module
                            .cells
                            .iter()
                            .position(|c| c.name == name_str)
                            .map(|idx| ClosureValue {
                                cell_idx: idx,
                                captures: vec![],
                            })
                    }
                    _ => None,
                };

                if let (Value::List(l), Some(cv)) = (list, closure_opt) {
                    let mut matching = Vec::new();
                    let mut non_matching = Vec::new();
                    for item in l.iter() {
                        let val = self.call_closure_sync(&cv, std::slice::from_ref(item))?;
                        if val.is_truthy() {
                            matching.push(item.clone());
                        } else {
                            non_matching.push(item.clone());
                        }
                    }
                    Ok(Value::new_tuple(vec![
                        Value::new_list(matching),
                        Value::new_list(non_matching),
                    ]))
                } else {
                    Ok(Value::new_tuple(vec![
                        Value::new_list(vec![]),
                        Value::new_list(vec![]),
                    ]))
                }
            }
            "read_dir" => {
                let path = self.registers[base + a].as_string();
                match std::fs::read_dir(&path) {
                    Ok(entries) => {
                        let mut result = Vec::new();
                        for e in entries.flatten() {
                            result.push(Value::String(StringRef::Owned(
                                e.file_name().to_string_lossy().to_string(),
                            )));
                        }
                        Ok(Value::new_list(result))
                    }
                    Err(e) => Err(VmError::Runtime(format!("read_dir failed: {}", e))),
                }
            }
            "exists" => {
                let path = self.registers[base + a].as_string();
                Ok(Value::Bool(std::path::Path::new(&path).exists()))
            }
            "mkdir" => {
                let path = self.registers[base + a].as_string();
                match std::fs::create_dir_all(&path) {
                    Ok(()) => Ok(Value::Null),
                    Err(e) => Err(VmError::Runtime(format!("mkdir failed: {}", e))),
                }
            }
            "exit" => {
                let code = self.registers[base + a].as_int().unwrap_or(0);
                std::process::exit(code as i32);
            }
            _ => Err(VmError::UndefinedCell(name.to_string())),
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
        let callee_cell = module.cells.get(cv.cell_idx).ok_or_else(|| {
            VmError::Runtime(format!("closure cell index {} out of bounds", cv.cell_idx))
        })?;
        let num_regs = callee_cell.registers as usize;
        let params: Vec<LirParam> = callee_cell.params.clone();
        let cell_regs = callee_cell.registers;
        let new_base = self.registers.len();
        self.registers
            .resize(new_base + num_regs.max(256), Value::Null);
        // Copy captures into frame registers
        for (i, cap) in cv.captures.iter().enumerate() {
            self.check_register(i, cell_regs)?;
            self.registers[new_base + i] = cap.clone();
        }
        // Copy arguments into parameter registers (after captures)
        let cap_count = cv.captures.len();
        let variadic_idx = params[cap_count..]
            .iter()
            .position(|p| p.variadic)
            .map(|i| i + cap_count);
        if let Some(vi) = variadic_idx {
            let fixed_count = vi - cap_count;
            for (i, arg) in args.iter().enumerate().take(fixed_count) {
                let dst = params[cap_count + i].register as usize;
                self.check_register(dst, cell_regs)?;
                self.registers[new_base + dst] = arg.clone();
            }
            let variadic_args: Vec<Value> = args[fixed_count..].to_vec();
            let dst = params[vi].register as usize;
            self.check_register(dst, cell_regs)?;
            self.registers[new_base + dst] = Value::new_list(variadic_args);
        } else {
            for (i, arg) in args.iter().enumerate() {
                if cap_count + i < params.len() {
                    let dst = params[cap_count + i].register as usize;
                    self.check_register(dst, cell_regs)?;
                    self.registers[new_base + dst] = arg.clone();
                }
            }
        }
        // Push a call frame with a sentinel return_register
        self.frames.push(CallFrame {
            cell_idx: cv.cell_idx,
            base_register: new_base,
            ip: 0,
            return_register: new_base, // result will be written here
            future_id: None,
        });
        // Run the VM until this frame returns
        self.run_until(self.frames.len().saturating_sub(1))?;
        Ok(self.registers[new_base].clone())
    }

    /// Execute an intrinsic function by ID.
    pub(crate) fn exec_intrinsic(
        &mut self,
        base: usize,
        _a: usize,
        func_id: usize,
        arg_reg: usize,
    ) -> Result<Value, VmError> {
        let arg = &self.registers[base + arg_reg];
        match func_id {
            0 => {
                // LENGTH (Unicode-aware for strings)
                Ok(match arg {
                    Value::String(StringRef::Owned(s)) => Value::Int(s.chars().count() as i64),
                    Value::String(StringRef::Interned(id)) => {
                        let s = self.strings.resolve(*id).unwrap_or("");
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
                // COUNT (Unicode-aware for strings)
                Ok(match arg {
                    Value::List(l) => Value::Int(l.len() as i64),
                    Value::Map(m) => Value::Int(m.len() as i64),
                    Value::String(StringRef::Owned(s)) => Value::Int(s.chars().count() as i64),
                    _ => Value::Int(0),
                })
            }
            2 => {
                // MATCHES
                Ok(match arg {
                    Value::Bool(b) => Value::Bool(*b),
                    Value::String(_) => Value::Bool(!arg.as_string().is_empty()),
                    _ => Value::Bool(false),
                })
            }
            3 => {
                // HASH
                use sha2::{Digest, Sha256};
                let hash = format!("{:x}", Sha256::digest(arg.as_string().as_bytes()));
                Ok(Value::String(StringRef::Owned(format!("sha256:{}", hash))))
            }
            4 => {
                // DIFF
                let other = &self.registers[base + arg_reg + 1];
                Ok(self.diff_values(arg, other))
            }
            5 => {
                // PATCH
                let patches = &self.registers[base + arg_reg + 1];
                Ok(self.patch_value(arg, patches))
            }
            6 => {
                // REDACT
                let fields = &self.registers[base + arg_reg + 1];
                Ok(self.redact_value(arg, fields))
            }
            7 => {
                // VALIDATE
                Ok(Value::Bool(true)) // full validation deferred to schema opcode
            }
            8 => {
                // TRACEREF
                Ok(Value::TraceRef(self.next_trace_ref()))
            }
            9 => {
                // PRINT
                let output = arg.display_pretty();
                println!("{}", output);
                self.output.push(output);
                Ok(Value::Null)
            }
            10 => Ok(Value::String(StringRef::Owned(arg.display_pretty()))), // TOSTRING
            11 => {
                // TOINT
                Ok(match arg {
                    Value::Int(n) => Value::Int(*n),
                    Value::Float(f) => Value::Int(*f as i64),
                    Value::String(StringRef::Owned(s)) => {
                        s.parse::<i64>().map(Value::Int).unwrap_or(Value::Null)
                    }
                    Value::Bool(b) => Value::Int(if *b { 1 } else { 0 }),
                    _ => Value::Null,
                })
            }
            12 => {
                // TOFLOAT
                Ok(match arg {
                    Value::Float(f) => Value::Float(*f),
                    Value::Int(n) => Value::Float(*n as f64),
                    Value::String(StringRef::Owned(s)) => {
                        s.parse::<f64>().map(Value::Float).unwrap_or(Value::Null)
                    }
                    _ => Value::Null,
                })
            }
            13 => Ok(Value::String(StringRef::Owned(arg.type_name().to_string()))), // TYPEOF
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
                let item = &self.registers[base + arg_reg + 1];
                Ok(match arg {
                    Value::List(l) => Value::Bool(l.contains(item)),
                    Value::Set(s) => Value::Bool(s.contains(item)),
                    Value::Map(m) => Value::Bool(m.contains_key(&item.as_string())),
                    Value::String(StringRef::Owned(s)) => {
                        Value::Bool(s.contains(&item.as_string()))
                    }
                    _ => Value::Bool(false),
                })
            }
            17 => {
                // JOIN
                let sep = self.registers[base + arg_reg + 1].as_string();
                Ok(match arg {
                    Value::List(l) => {
                        let s = l
                            .iter()
                            .map(|v| v.as_string())
                            .collect::<Vec<_>>()
                            .join(&sep);
                        Value::String(StringRef::Owned(s))
                    }
                    _ => Value::String(StringRef::Owned("".into())),
                })
            }
            18 => {
                // SPLIT
                let sep = self.registers[base + arg_reg + 1].as_string();
                Ok(match arg {
                    Value::String(StringRef::Owned(s)) => {
                        let parts: Vec<Value> = s
                            .split(&sep)
                            .map(|p| Value::String(StringRef::Owned(p.to_string())))
                            .collect();
                        Value::new_list(parts)
                    }
                    _ => Value::new_list(vec![]),
                })
            }
            19 => Ok(match arg {
                Value::String(StringRef::Owned(s)) => {
                    Value::String(StringRef::Owned(s.trim().to_string()))
                }
                _ => arg.clone(),
            }), // TRIM
            20 => Ok(match arg {
                Value::String(StringRef::Owned(s)) => {
                    Value::String(StringRef::Owned(s.to_uppercase()))
                }
                _ => arg.clone(),
            }), // UPPER
            21 => Ok(match arg {
                Value::String(StringRef::Owned(s)) => {
                    Value::String(StringRef::Owned(s.to_lowercase()))
                }
                _ => arg.clone(),
            }), // LOWER
            22 => {
                // REPLACE
                let pat = self.registers[base + arg_reg + 1].as_string();
                let with = self.registers[base + arg_reg + 2].as_string();
                Ok(match arg {
                    Value::String(StringRef::Owned(s)) => {
                        Value::String(StringRef::Owned(s.replace(&pat, &with)))
                    }
                    _ => arg.clone(),
                })
            }
            23 => {
                // SLICE
                let start_val = &self.registers[base + arg_reg + 1];
                let end_val = &self.registers[base + arg_reg + 2];
                let start = start_val.as_int().unwrap_or(0);
                let end = end_val.as_int().unwrap_or(0);
                Ok(match arg {
                    Value::List(l) => {
                        let start = start.max(0) as usize;
                        let end = if end <= 0 {
                            l.len()
                        } else {
                            (end as usize).min(l.len())
                        };
                        if start < end {
                            Value::new_list(l[start..end].to_vec())
                        } else {
                            Value::new_list(vec![])
                        }
                    }
                    Value::String(StringRef::Owned(s)) => {
                        let char_count = s.chars().count();
                        let start = start.max(0) as usize;
                        let end = if end <= 0 {
                            char_count
                        } else {
                            (end as usize).min(char_count)
                        };
                        if start < end {
                            Value::String(StringRef::Owned(
                                s.chars().skip(start).take(end - start).collect::<String>(),
                            ))
                        } else {
                            Value::String(StringRef::Owned("".into()))
                        }
                    }
                    _ => Value::Null,
                })
            }
            24 => {
                // APPEND
                let item = self.registers[base + arg_reg + 1].clone();
                Ok(match arg {
                    Value::List(l) => {
                        let mut new_l: Vec<Value> = (**l).clone();
                        new_l.push(item);
                        Value::new_list(new_l)
                    }
                    _ => Value::Null,
                })
            }
            25 => {
                // RANGE
                let end = self.registers[base + arg_reg + 1].as_int().unwrap_or(0);
                let start = arg.as_int().unwrap_or(0);
                let list: Vec<Value> = (start..end).map(Value::Int).collect();
                Ok(Value::new_list(list))
            }
            26 => Ok(match arg {
                Value::Int(n) => Value::Int(n.abs()),
                Value::Float(f) => Value::Float(f.abs()),
                _ => Value::Null,
            }), // ABS
            27 => {
                // MIN
                let other = &self.registers[base + arg_reg + 1];
                Ok(match (arg, other) {
                    (Value::Int(a), Value::Int(b)) => Value::Int(*a.min(b)),
                    (Value::Float(a), Value::Float(b)) => Value::Float(a.min(*b)),
                    _ => arg.clone(),
                })
            }
            28 => {
                // MAX
                let other = &self.registers[base + arg_reg + 1];
                Ok(match (arg, other) {
                    (Value::Int(a), Value::Int(b)) => Value::Int(*a.max(b)),
                    (Value::Float(a), Value::Float(b)) => Value::Float(a.max(*b)),
                    _ => arg.clone(),
                })
            }
            // Extended intrinsics (29+)
            29 => {
                // SORT
                if let Value::List(l) = arg {
                    let mut s: Vec<Value> = (**l).clone();
                    s.sort();
                    Ok(Value::new_list(s))
                } else {
                    Ok(arg.clone())
                }
            }
            30 => {
                // REVERSE
                if let Value::List(l) = arg {
                    let mut r: Vec<Value> = (**l).clone();
                    r.reverse();
                    Ok(Value::new_list(r))
                } else {
                    Ok(arg.clone())
                }
            }
            31 => {
                // MAP: apply closure to each element
                let closure_val = self.registers[base + arg_reg + 1].clone();
                if let (Value::List(l), Value::Closure(cv)) = (arg.clone(), closure_val) {
                    let mut result = Vec::with_capacity(l.len());
                    for item in l.iter() {
                        let val = self.call_closure_sync(&cv, std::slice::from_ref(item))?;
                        result.push(val);
                    }
                    Ok(Value::new_list(result))
                } else {
                    Ok(arg.clone())
                }
            }
            32 => {
                // FILTER: keep elements where closure returns truthy
                let closure_val = self.registers[base + arg_reg + 1].clone();
                if let (Value::List(l), Value::Closure(cv)) = (arg.clone(), closure_val) {
                    let mut result = Vec::new();
                    for item in l.iter() {
                        let val = self.call_closure_sync(&cv, std::slice::from_ref(item))?;
                        if val.is_truthy() {
                            result.push(item.clone());
                        }
                    }
                    Ok(Value::new_list(result))
                } else {
                    Ok(arg.clone())
                }
            }
            33 => {
                // REDUCE: fold with accumulator
                let closure_val = self.registers[base + arg_reg + 1].clone();
                let init = self.registers[base + arg_reg + 2].clone();
                if let (Value::List(l), Value::Closure(cv)) = (arg.clone(), closure_val) {
                    let mut acc = init;
                    for item in l.iter() {
                        acc = self.call_closure_sync(&cv, &[acc, item.clone()])?;
                    }
                    Ok(acc)
                } else {
                    Ok(init)
                }
            }
            34 => {
                // FLAT_MAP: map then flatten one level
                let closure_val = self.registers[base + arg_reg + 1].clone();
                if let (Value::List(l), Value::Closure(cv)) = (arg.clone(), closure_val) {
                    let mut result = Vec::new();
                    for item in l.iter() {
                        let val = self.call_closure_sync(&cv, std::slice::from_ref(item))?;
                        if let Value::List(inner) = val {
                            result.extend(inner.iter().cloned());
                        } else {
                            result.push(val);
                        }
                    }
                    Ok(Value::new_list(result))
                } else {
                    Ok(arg.clone())
                }
            }
            35 => {
                // ZIP: pair elements from two lists
                let other = self.registers[base + arg_reg + 1].clone();
                if let (Value::List(la), Value::List(lb)) = (arg, &other) {
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
                // ENUMERATE: list of [index, element] tuples
                if let Value::List(l) = arg {
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
            37 => {
                // ANY: true if any element satisfies closure
                let closure_val = self.registers[base + arg_reg + 1].clone();
                if let (Value::List(l), Value::Closure(cv)) = (arg.clone(), closure_val) {
                    for item in l.iter() {
                        let val = self.call_closure_sync(&cv, std::slice::from_ref(item))?;
                        if val.is_truthy() {
                            return Ok(Value::Bool(true));
                        }
                    }
                    Ok(Value::Bool(false))
                } else {
                    Ok(Value::Bool(false))
                }
            }
            38 => {
                // ALL: true if all elements satisfy closure
                let closure_val = self.registers[base + arg_reg + 1].clone();
                if let (Value::List(l), Value::Closure(cv)) = (arg.clone(), closure_val) {
                    for item in l.iter() {
                        let val = self.call_closure_sync(&cv, std::slice::from_ref(item))?;
                        if !val.is_truthy() {
                            return Ok(Value::Bool(false));
                        }
                    }
                    Ok(Value::Bool(true))
                } else {
                    Ok(Value::Bool(true))
                }
            }
            39 => {
                // FIND: first element satisfying closure, or Null
                let closure_val = self.registers[base + arg_reg + 1].clone();
                if let (Value::List(l), Value::Closure(cv)) = (arg.clone(), closure_val) {
                    for item in l.iter() {
                        let val = self.call_closure_sync(&cv, std::slice::from_ref(item))?;
                        if val.is_truthy() {
                            return Ok(item.clone());
                        }
                    }
                    Ok(Value::Null)
                } else {
                    Ok(Value::Null)
                }
            }
            40 => {
                // POSITION: index of first element satisfying closure, or -1
                let closure_val = self.registers[base + arg_reg + 1].clone();
                if let (Value::List(l), Value::Closure(cv)) = (arg.clone(), closure_val) {
                    for (i, item) in l.iter().enumerate() {
                        let val = self.call_closure_sync(&cv, std::slice::from_ref(item))?;
                        if val.is_truthy() {
                            return Ok(Value::Int(i as i64));
                        }
                    }
                    Ok(Value::Int(-1))
                } else {
                    Ok(Value::Int(-1))
                }
            }
            41 => {
                // GROUP_BY: group elements by closure result into a map
                let closure_val = self.registers[base + arg_reg + 1].clone();
                if let (Value::List(l), Value::Closure(cv)) = (arg.clone(), closure_val) {
                    let mut groups: BTreeMap<String, Value> = BTreeMap::new();
                    for item in l.iter() {
                        let key = self.call_closure_sync(&cv, std::slice::from_ref(item))?;
                        let key_str = key.as_string();
                        match groups.get_mut(&key_str) {
                            Some(Value::List(ref mut list)) => {
                                Arc::make_mut(list).push(item.clone())
                            }
                            _ => {
                                groups.insert(key_str, Value::new_list(vec![item.clone()]));
                            }
                        }
                    }
                    Ok(Value::new_map(groups))
                } else {
                    Ok(Value::new_map(BTreeMap::new()))
                }
            }
            42 => {
                // CHUNK: split list into chunks of size N
                let n = self.registers[base + arg_reg + 1].as_int().unwrap_or(1) as usize;
                if let Value::List(l) = arg {
                    let result: Vec<Value> = l
                        .chunks(n.max(1))
                        .map(|chunk| Value::new_list(chunk.to_vec()))
                        .collect();
                    Ok(Value::new_list(result))
                } else {
                    Ok(Value::new_list(vec![]))
                }
            }
            43 => {
                // WINDOW: sliding window of size N
                let n = self.registers[base + arg_reg + 1].as_int().unwrap_or(1) as usize;
                if let Value::List(l) = arg {
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
            44 => {
                // FLATTEN
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
                    Ok(arg.clone())
                }
            }
            45 => {
                // UNIQUE
                if let Value::List(l) = arg {
                    let mut result = Vec::new();
                    for item in l.iter() {
                        if !result.contains(item) {
                            result.push(item.clone());
                        }
                    }
                    Ok(Value::new_list(result))
                } else {
                    Ok(arg.clone())
                }
            }
            46 => {
                // TAKE
                let n = self.registers[base + arg_reg + 1].as_int().unwrap_or(0) as usize;
                if let Value::List(l) = arg {
                    Ok(Value::new_list(l.iter().take(n).cloned().collect()))
                } else {
                    Ok(arg.clone())
                }
            }
            47 => {
                // DROP
                let n = self.registers[base + arg_reg + 1].as_int().unwrap_or(0) as usize;
                if let Value::List(l) = arg {
                    Ok(Value::new_list(l.iter().skip(n).cloned().collect()))
                } else {
                    Ok(arg.clone())
                }
            }
            48 => Ok(match arg {
                Value::List(l) => l.first().cloned().unwrap_or(Value::Null),
                _ => Value::Null,
            }), // FIRST
            49 => Ok(match arg {
                Value::List(l) => l.last().cloned().unwrap_or(Value::Null),
                _ => Value::Null,
            }), // LAST
            50 => Ok(Value::Bool(match arg {
                Value::List(l) => l.is_empty(),
                Value::Map(m) => m.is_empty(),
                Value::String(StringRef::Owned(s)) => s.is_empty(),
                _ => true,
            })), // ISEMPTY
            51 => {
                // CHARS
                let s = arg.as_string();
                Ok(Value::new_list(
                    s.chars()
                        .map(|c| Value::String(StringRef::Owned(c.to_string())))
                        .collect(),
                ))
            }
            52 => {
                // STARTSWITH
                let prefix = self.registers[base + arg_reg + 1].as_string();
                Ok(Value::Bool(arg.as_string().starts_with(&prefix)))
            }
            53 => {
                // ENDSWITH
                let suffix = self.registers[base + arg_reg + 1].as_string();
                Ok(Value::Bool(arg.as_string().ends_with(&suffix)))
            }
            54 => {
                // INDEXOF (Unicode-aware: returns character index, not byte index)
                let needle = self.registers[base + arg_reg + 1].as_string();
                let haystack = arg.as_string();
                Ok(match haystack.find(&needle) {
                    Some(byte_idx) => {
                        // Convert byte index to character index
                        let char_idx = haystack[..byte_idx].chars().count();
                        Value::Int(char_idx as i64)
                    }
                    None => Value::Int(-1),
                })
            }
            55 => {
                // PADLEFT (Unicode-aware)
                let width = self.registers[base + arg_reg + 1].as_int().unwrap_or(0) as usize;
                let s = arg.as_string();
                let char_count = s.chars().count();
                if char_count < width {
                    let padding = " ".repeat(width - char_count);
                    Ok(Value::String(StringRef::Owned(format!("{}{}", padding, s))))
                } else {
                    Ok(Value::String(StringRef::Owned(s)))
                }
            }
            56 => {
                // PADRIGHT (Unicode-aware)
                let width = self.registers[base + arg_reg + 1].as_int().unwrap_or(0) as usize;
                let s = arg.as_string();
                let char_count = s.chars().count();
                if char_count < width {
                    let padding = " ".repeat(width - char_count);
                    Ok(Value::String(StringRef::Owned(format!("{}{}", s, padding))))
                } else {
                    Ok(Value::String(StringRef::Owned(s)))
                }
            }
            57 => Ok(match arg {
                Value::Float(f) => Value::Float(f.round()),
                _ => arg.clone(),
            }), // ROUND
            58 => Ok(match arg {
                Value::Float(f) => Value::Float(f.ceil()),
                _ => arg.clone(),
            }), // CEIL
            59 => Ok(match arg {
                Value::Float(f) => Value::Float(f.floor()),
                _ => arg.clone(),
            }), // FLOOR
            60 => Ok(match arg {
                Value::Float(f) => Value::Float(f.sqrt()),
                Value::Int(n) => Value::Float((*n as f64).sqrt()),
                Value::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::INFINITY).sqrt()),
                _ => Value::Null,
            }), // SQRT
            61 => {
                // POW
                let exp = &self.registers[base + arg_reg + 1];
                Ok(match (arg, exp) {
                    (Value::Int(x), Value::Int(y)) => {
                        if *y >= 0 {
                            if let Ok(y_u32) = std::convert::TryFrom::try_from(*y) {
                                if let Some(res) = x.checked_pow(y_u32) {
                                    Value::Int(res)
                                } else {
                                    Value::BigInt(BigInt::from(*x).pow(y_u32))
                                }
                            } else {
                                Value::Null // Exponent too large
                            }
                        } else {
                            Value::Float((*x as f64).powf(*y as f64))
                        }
                    }
                    (Value::BigInt(x), Value::Int(y)) => {
                        if *y >= 0 {
                            if let Ok(y_u32) = std::convert::TryFrom::try_from(*y) {
                                Value::BigInt(x.pow(y_u32))
                            } else {
                                Value::Null
                            }
                        } else {
                            Value::Float(x.to_f64().unwrap_or(f64::INFINITY).powf(*y as f64))
                        }
                    }
                    (Value::Int(x), Value::BigInt(y)) => {
                        if let Some(y_u32) = y.to_u32() {
                            Value::BigInt(BigInt::from(*x).pow(y_u32))
                        } else if y.sign() == num_bigint::Sign::Minus {
                            Value::Float((*x as f64).powf(y.to_f64().unwrap_or(f64::NEG_INFINITY)))
                        } else {
                            Value::Null
                        }
                    }
                    (Value::BigInt(x), Value::BigInt(y)) => {
                        if let Some(y_u32) = y.to_u32() {
                            Value::BigInt(x.pow(y_u32))
                        } else if y.sign() == num_bigint::Sign::Minus {
                            Value::Float(
                                x.to_f64()
                                    .unwrap_or(f64::INFINITY)
                                    .powf(y.to_f64().unwrap_or(f64::NEG_INFINITY)),
                            )
                        } else {
                            Value::Null
                        }
                    }
                    (Value::Float(x), Value::Float(y)) => Value::Float(x.powf(*y)),
                    (Value::Int(x), Value::Float(y)) => Value::Float((*x as f64).powf(*y)),
                    (Value::Float(x), Value::Int(y)) => Value::Float(x.powf(*y as f64)),
                    (Value::BigInt(x), Value::Float(y)) => {
                        Value::Float(x.to_f64().unwrap_or(f64::INFINITY).powf(*y))
                    }
                    (Value::Float(x), Value::BigInt(y)) => {
                        Value::Float(x.powf(y.to_f64().unwrap_or(f64::INFINITY)))
                    }
                    _ => Value::Null,
                })
            }
            62 => Ok(match arg {
                Value::Float(f) => Value::Float(f.ln()),
                Value::Int(n) => Value::Float((*n as f64).ln()),
                Value::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::INFINITY).ln()),
                _ => Value::Null,
            }), // LOG
            63 => Ok(match arg {
                Value::Float(f) => Value::Float(f.sin()),
                Value::Int(n) => Value::Float((*n as f64).sin()),
                Value::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::INFINITY).sin()),
                _ => Value::Null,
            }), // SIN
            64 => Ok(match arg {
                Value::Float(f) => Value::Float(f.cos()),
                Value::Int(n) => Value::Float((*n as f64).cos()),
                Value::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::INFINITY).cos()),
                _ => Value::Null,
            }), // COS
            65 => {
                // CLAMP
                let lo = &self.registers[base + arg_reg + 1];
                let hi = &self.registers[base + arg_reg + 2];
                Ok(match (arg, lo, hi) {
                    (Value::Int(v), Value::Int(l), Value::Int(h)) => Value::Int(*v.max(l).min(h)),
                    (Value::Float(v), Value::Float(l), Value::Float(h)) => {
                        Value::Float(v.max(*l).min(*h))
                    }
                    _ => arg.clone(),
                })
            }
            66 => Ok(arg.clone()),                                   // CLONE
            67 => Ok(Value::Int(std::mem::size_of_val(arg) as i64)), // SIZEOF
            68 => {
                // DEBUG
                let output = format!("[debug] {:?}", arg);
                eprintln!("{}", output);
                self.output.push(output);
                Ok(Value::Null)
            }
            69 => {
                // TOSET - convert list to set (deduplicating elements)
                Ok(match arg {
                    Value::List(l) => {
                        Value::new_set(l.iter().cloned().collect::<BTreeSet<Value>>())
                    }
                    Value::Set(_) => arg.clone(), // already a set
                    _ => Value::new_set(BTreeSet::new()),
                })
            }
            70 => {
                // HAS_KEY - check if map has a key
                let key = self.registers[base + arg_reg + 1].as_string();
                Ok(match arg {
                    Value::Map(m) => Value::Bool(m.contains_key(&key)),
                    Value::Record(r) => Value::Bool(r.fields.contains_key(&key)),
                    _ => Value::Bool(false),
                })
            }
            71 => {
                // MERGE - merge two maps (second overwrites first)
                let other = &self.registers[base + arg_reg + 1];
                Ok(match (arg, other) {
                    (Value::Map(m1), Value::Map(m2)) => {
                        let mut result: BTreeMap<String, Value> = (**m1).clone();
                        result.extend(m2.iter().map(|(k, v)| (k.clone(), v.clone())));
                        Value::new_map(result)
                    }
                    _ => arg.clone(),
                })
            }
            72 => {
                // SIZE - alias for length/count
                Ok(match arg {
                    Value::String(StringRef::Owned(s)) => Value::Int(s.chars().count() as i64),
                    Value::String(StringRef::Interned(id)) => {
                        let s = self.strings.resolve(*id).unwrap_or("");
                        Value::Int(s.chars().count() as i64)
                    }
                    Value::List(l) => Value::Int(l.len() as i64),
                    Value::Map(m) => Value::Int(m.len() as i64),
                    Value::Set(s) => Value::Int(s.len() as i64),
                    Value::Tuple(t) => Value::Int(t.len() as i64),
                    Value::Bytes(b) => Value::Int(b.len() as i64),
                    _ => Value::Int(0),
                })
            }
            73 => {
                // ADD - add element to set (returns new set)
                let item = self.registers[base + arg_reg + 1].clone();
                Ok(match arg {
                    Value::Set(s) => {
                        let mut new_set: BTreeSet<Value> = (**s).clone();
                        new_set.insert(item);
                        Value::new_set(new_set)
                    }
                    _ => {
                        let mut new_set = BTreeSet::new();
                        new_set.insert(item);
                        Value::new_set(new_set)
                    }
                })
            }
            74 => {
                // REMOVE - remove element from set or key from map (returns new collection)
                let item = self.registers[base + arg_reg + 1].clone();
                Ok(match arg {
                    Value::Set(s) => {
                        let mut new_set: BTreeSet<Value> = (**s).clone();
                        new_set.remove(&item);
                        Value::new_set(new_set)
                    }
                    Value::Map(m) => {
                        let key = item.as_string();
                        let mut new_map: BTreeMap<String, Value> = (**m).clone();
                        new_map.remove(&key);
                        Value::new_map(new_map)
                    }
                    _ => arg.clone(),
                })
            }
            75 => {
                // ENTRIES - list of [key, value] tuples for maps
                Ok(match arg {
                    Value::Map(m) => {
                        let entries: Vec<Value> = m
                            .iter()
                            .map(|(k, v)| {
                                Value::new_tuple(vec![
                                    Value::String(StringRef::Owned(k.clone())),
                                    v.clone(),
                                ])
                            })
                            .collect();
                        Value::new_list(entries)
                    }
                    Value::Record(r) => {
                        let entries: Vec<Value> = r
                            .fields
                            .iter()
                            .map(|(k, v)| {
                                Value::new_tuple(vec![
                                    Value::String(StringRef::Owned(k.clone())),
                                    v.clone(),
                                ])
                            })
                            .collect();
                        Value::new_list(entries)
                    }
                    _ => Value::new_list(vec![]),
                })
            }
            77 => {
                // FORMAT: format string with {} placeholders
                let template = arg.as_string();
                let mut result = String::new();
                let mut placeholder_idx = 0;
                let mut chars = template.chars().peekable();
                while let Some(ch) = chars.next() {
                    if ch == '{' && chars.peek() == Some(&'}') {
                        chars.next();
                        let val = &self.registers[base + arg_reg + 1 + placeholder_idx];
                        result.push_str(&val.display_pretty());
                        placeholder_idx += 1;
                    } else {
                        result.push(ch);
                    }
                }
                Ok(Value::String(StringRef::Owned(result)))
            }
            78 => {
                // PARTITION: split list by predicate into (matching, non_matching)
                let closure_val = self.registers[base + arg_reg + 1].clone();
                println!(
                    "DEBUG: exec_intrinsic partition. arg_reg={}, closure_val={:?}",
                    arg_reg, closure_val
                );

                let closure_opt = match closure_val {
                    Value::Closure(cv) => Some(cv),
                    Value::String(ref s) => {
                        let name_str = match s {
                            StringRef::Owned(s) => s.as_str(),
                            StringRef::Interned(id) => self.strings.resolve(*id).unwrap_or(""),
                        };

                        let module = self.module.as_ref().ok_or(VmError::NoModule)?;

                        module
                            .cells
                            .iter()
                            .position(|c| c.name == name_str)
                            .map(|idx| ClosureValue {
                                cell_idx: idx,
                                captures: vec![],
                            })
                    }
                    _ => None,
                };

                if let (Value::List(l), Some(cv)) = (arg.clone(), closure_opt) {
                    let mut matching = Vec::new();
                    let mut non_matching = Vec::new();
                    for item in l.iter() {
                        let val = self.call_closure_sync(&cv, std::slice::from_ref(item))?;
                        if val.is_truthy() {
                            matching.push(item.clone());
                        } else {
                            non_matching.push(item.clone());
                        }
                    }
                    Ok(Value::new_tuple(vec![
                        Value::new_list(matching),
                        Value::new_list(non_matching),
                    ]))
                } else {
                    Ok(Value::new_tuple(vec![
                        Value::new_list(vec![]),
                        Value::new_list(vec![]),
                    ]))
                }
            }
            79 => {
                // READ_DIR: list directory entries
                let path = arg.as_string();
                match std::fs::read_dir(&path) {
                    Ok(entries) => {
                        let mut result = Vec::new();
                        for e in entries.flatten() {
                            result.push(Value::String(StringRef::Owned(
                                e.file_name().to_string_lossy().to_string(),
                            )));
                        }
                        Ok(Value::new_list(result))
                    }
                    Err(e) => Err(VmError::Runtime(format!("read_dir failed: {}", e))),
                }
            }
            80 => {
                // EXISTS: check if path exists
                let path = arg.as_string();
                Ok(Value::Bool(std::path::Path::new(&path).exists()))
            }
            81 => {
                // MKDIR: create directory (and parents)
                let path = arg.as_string();
                match std::fs::create_dir_all(&path) {
                    Ok(()) => Ok(Value::Null),
                    Err(e) => Err(VmError::Runtime(format!("mkdir failed: {}", e))),
                }
            }
            82 => {
                // EVAL: compile and execute string code
                let source = arg.as_string();
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                let cell_name = format!("__eval_{}", now);
                let wrapped_src = format!("cell {}() -> Any\n  {}\nend", cell_name, source);

                match compile_raw(&wrapped_src) {
                    Ok(new_module) => {
                        if let Some(current_mod) = self.module.as_mut() {
                            current_mod.merge(&new_module);
                            self.call_cell_sync(&cell_name, vec![])
                        } else {
                            Err(VmError::Runtime("VM has no module loaded for eval".into()))
                        }
                    }
                    Err(e) => Err(VmError::Runtime(format!("eval compilation failed: {}", e))),
                }
            }
            83 => {
                // GUARDRAIL: value, schema
                let _val = &self.registers[base + arg_reg];
                let _schema = &self.registers[base + arg_reg + 1];
                // semantic placeholder for now
                Ok(Value::Bool(true))
            }
            84 => {
                // PATTERN: value, pattern_def
                let _val = &self.registers[base + arg_reg];
                let _pattern = &self.registers[base + arg_reg + 1];
                // semantic placeholder for now
                Ok(Value::Null)
            }
            85 => {
                // EXIT: exit process with code
                let code = arg.as_int().unwrap_or(0);
                std::process::exit(code as i32);
            }
            _ => Err(VmError::Runtime(format!(
                "Unknown intrinsic ID {} - this is a compiler/VM mismatch bug",
                func_id
            ))),
        }
    }
}
