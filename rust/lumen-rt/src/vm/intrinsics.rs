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
                let list = std::mem::take(&mut self.registers[base + a + 1]);
                let elem = std::mem::take(&mut self.registers[base + a + 2]);
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
                    Value::Map(m) => {
                        let needle_str = value_to_str_cow(needle, &self.strings);
                        m.contains_key(needle_str.as_ref())
                    }
                    Value::String(StringRef::Owned(s)) => {
                        let needle_str = value_to_str_cow(needle, &self.strings);
                        s.contains(needle_str.as_ref())
                    }
                    _ => false,
                };
                Ok(Value::Bool(result))
            }
            "join" => {
                let list = &self.registers[base + a + 1];
                let sep = if nargs > 1 {
                    value_to_str_cow(&self.registers[base + a + 2], &self.strings)
                } else {
                    std::borrow::Cow::Borrowed(", ")
                };
                if let Value::List(l) = list {
                    let joined = l
                        .iter()
                        .map(|v| v.display_pretty())
                        .collect::<Vec<_>>()
                        .join(sep.as_ref());
                    Ok(Value::String(StringRef::Owned(joined)))
                } else {
                    Ok(Value::String(StringRef::Owned(list.display_pretty())))
                }
            }
            "split" => {
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                let sep = if nargs > 1 {
                    value_to_str_cow(&self.registers[base + a + 2], &self.strings)
                } else {
                    std::borrow::Cow::Borrowed(" ")
                };
                let parts: Vec<Value> = s
                    .split(sep.as_ref())
                    .map(|p| Value::String(StringRef::Owned(p.to_string())))
                    .collect();
                Ok(Value::new_list(parts))
            }
            "trim" => {
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                Ok(Value::String(StringRef::Owned(s.trim().to_string())))
            }
            "upper" => {
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                Ok(Value::String(StringRef::Owned(s.to_uppercase())))
            }
            "lower" => {
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                Ok(Value::String(StringRef::Owned(s.to_lowercase())))
            }
            "replace" => {
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                let from = value_to_str_cow(&self.registers[base + a + 2], &self.strings);
                let to = value_to_str_cow(&self.registers[base + a + 3], &self.strings);
                Ok(Value::String(StringRef::Owned(
                    s.replace(from.as_ref(), to.as_ref()),
                )))
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
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                let h = format!("sha256:{:x}", Sha256::digest(s.as_bytes()));
                Ok(Value::String(StringRef::Owned(h)))
            }
            // Collection ops
            "sort" => {
                let arg = std::mem::take(&mut self.registers[base + a + 1]);
                if let Value::List(mut l) = arg {
                    Arc::make_mut(&mut l).sort();
                    Ok(Value::List(l))
                } else {
                    Ok(arg)
                }
            }
            "reverse" => {
                let arg = std::mem::take(&mut self.registers[base + a + 1]);
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
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                Ok(Value::new_list(
                    s.chars()
                        .map(|c| Value::String(StringRef::Owned(c.to_string())))
                        .collect(),
                ))
            }
            "starts_with" => {
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                let prefix = value_to_str_cow(&self.registers[base + a + 2], &self.strings);
                Ok(Value::Bool(s.starts_with(prefix.as_ref())))
            }
            "ends_with" => {
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                let suffix = value_to_str_cow(&self.registers[base + a + 2], &self.strings);
                Ok(Value::Bool(s.ends_with(suffix.as_ref())))
            }
            "index_of" => {
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                let needle = value_to_str_cow(&self.registers[base + a + 2], &self.strings);
                Ok(match s.find(needle.as_ref()) {
                    Some(i) => Value::Int(i as i64),
                    None => Value::Int(-1),
                })
            }
            "pad_left" => {
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                let width = self.registers[base + a + 2].as_int().unwrap_or(0) as usize;
                let pad = if nargs > 2 {
                    value_to_str_cow(&self.registers[base + a + 3], &self.strings)
                } else {
                    std::borrow::Cow::Borrowed(" ")
                };
                let pad_char = pad.chars().next().unwrap_or(' ');
                if s.len() < width {
                    let padding: String = std::iter::repeat_n(pad_char, width - s.len()).collect();
                    Ok(Value::String(StringRef::Owned(format!("{}{}", padding, s))))
                } else {
                    Ok(Value::String(StringRef::Owned(s.into_owned())))
                }
            }
            "pad_right" => {
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                let width = self.registers[base + a + 2].as_int().unwrap_or(0) as usize;
                let pad = if nargs > 2 {
                    value_to_str_cow(&self.registers[base + a + 3], &self.strings)
                } else {
                    std::borrow::Cow::Borrowed(" ")
                };
                let pad_char = pad.chars().next().unwrap_or(' ');
                if s.len() < width {
                    let padding: String = std::iter::repeat_n(pad_char, width - s.len()).collect();
                    Ok(Value::String(StringRef::Owned(format!("{}{}", s, padding))))
                } else {
                    Ok(Value::String(StringRef::Owned(s.into_owned())))
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
                let tag_ok = self.tag_ok;
                Ok(Value::Bool(
                    matches!(arg, Value::Union(u) if u.tag == tag_ok),
                ))
            }
            "is_err" => {
                let arg = &self.registers[base + a + 1];
                let tag_err = self.tag_err;
                Ok(Value::Bool(
                    matches!(arg, Value::Union(u) if u.tag == tag_err),
                ))
            }
            "unwrap" => {
                let arg = &self.registers[base + a + 1];
                let tag_ok = self.tag_ok;
                let tag_err = self.tag_err;
                match arg {
                    Value::Union(u) if u.tag == tag_ok => Ok((*u.payload).clone()),
                    Value::Union(u) if u.tag == tag_err => {
                        Err(VmError::Runtime(format!("unwrap on err: {}", u.payload)))
                    }
                    _ => Ok(arg.clone()),
                }
            }
            "unwrap_or" => {
                let arg = &self.registers[base + a + 1];
                let default = self.registers[base + a + 2].clone();
                let tag_ok = self.tag_ok;
                match arg {
                    Value::Union(u) if u.tag == tag_ok => Ok((*u.payload).clone()),
                    _ => Ok(default),
                }
            }
            // Crypto
            "sha512" => {
                use sha2::{Digest, Sha512};
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
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
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                Ok(Value::String(StringRef::Owned(simple_base64_encode(
                    s.as_bytes(),
                ))))
            }
            "base64_decode" => {
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                match simple_base64_decode(&s) {
                    Some(bytes) => Ok(Value::String(StringRef::Owned(
                        String::from_utf8_lossy(&bytes).to_string(),
                    ))),
                    None => Ok(Value::Null),
                }
            }
            "hex_encode" => {
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                let hex: String = s.bytes().map(|b| format!("{:02x}", b)).collect();
                Ok(Value::String(StringRef::Owned(hex)))
            }
            "hex_decode" => {
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
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
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
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
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
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
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                match serde_json::from_str::<serde_json::Value>(&s) {
                    Ok(v) => Ok(json_to_value(&v)),
                    Err(_) => Ok(Value::Null),
                }
            }
            "json_encode" | "to_json" => {
                let val = &self.registers[base + a + 1];
                let j = value_to_json(val, &self.strings);
                Ok(Value::String(StringRef::Owned(j.to_string())))
            }
            "json_pretty" => {
                let val = &self.registers[base + a + 1];
                let j = value_to_json(val, &self.strings);
                let pretty = serde_json::to_string_pretty(&j)
                    .map_err(|e| VmError::Runtime(format!("json_pretty failed: {}", e)))?;
                Ok(Value::String(StringRef::Owned(pretty)))
            }
            // String case transforms (std.string)
            "capitalize" => {
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                let mut c = s.chars();
                let result = match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().to_string() + &c.as_str().to_lowercase(),
                };
                Ok(Value::String(StringRef::Owned(result)))
            }
            "title_case" => {
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
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
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
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
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
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
                        value_to_str_cow(&self.registers[base + a + 2], &self.strings).into_owned()
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
                    Value::String(StringRef::Owned(s)) => {
                        let needle_str = value_to_str_cow(needle, &self.strings);
                        s.contains(needle_str.as_ref())
                    }
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
                        let key_str = value_to_str_cow(&key, &self.strings).into_owned();
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
                let path = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                match std::fs::read_to_string(path.as_ref()) {
                    Ok(contents) => Ok(Value::String(StringRef::Owned(contents))),
                    Err(e) => Err(VmError::Runtime(format!("read_file failed: {}", e))),
                }
            }
            "write_file" => {
                let path = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                let content = value_to_str_cow(&self.registers[base + a + 2], &self.strings);
                match std::fs::write(path.as_ref(), content.as_bytes()) {
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
                let name = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                match std::env::var(name.as_ref()) {
                    Ok(val) => Ok(Value::String(StringRef::Owned(val))),
                    Err(_) => Ok(Value::Null),
                }
            }
            "freeze" => Ok(self.registers[base + a + 1].clone()),
            "format" => {
                let template = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
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
                let path = value_to_str_cow(&self.registers[base + a], &self.strings).into_owned();
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
                let path = value_to_str_cow(&self.registers[base + a], &self.strings).into_owned();
                Ok(Value::Bool(std::path::Path::new(&path).exists()))
            }
            "mkdir" => {
                let path = value_to_str_cow(&self.registers[base + a], &self.strings).into_owned();
                match std::fs::create_dir_all(&path) {
                    Ok(()) => Ok(Value::Null),
                    Err(e) => Err(VmError::Runtime(format!("mkdir failed: {}", e))),
                }
            }
            "exit" => {
                let code = self.registers[base + a].as_int().unwrap_or(0);
                std::process::exit(code as i32);
            }
            // String trimming variants
            "trim_start" => {
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                Ok(Value::String(StringRef::Owned(s.trim_start().to_string())))
            }
            "trim_end" => {
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                Ok(Value::String(StringRef::Owned(s.trim_end().to_string())))
            }
            // Math: exponential
            "exp" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.exp()),
                    Value::Int(n) => Value::Float((*n as f64).exp()),
                    Value::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::NAN).exp()),
                    _ => Value::Null,
                })
            }
            // Math: tangent
            "tan" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.tan()),
                    Value::Int(n) => Value::Float((*n as f64).tan()),
                    Value::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::NAN).tan()),
                    _ => Value::Null,
                })
            }
            // Math: truncate toward zero
            "trunc" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.trunc()),
                    Value::Int(_) => arg.clone(),
                    Value::BigInt(_) => arg.clone(),
                    _ => Value::Null,
                })
            }
            // Random integer in range [min, max]
            "random_int" => {
                let min_val = match &self.registers[base + a + 1] {
                    Value::Int(n) => *n,
                    other => {
                        return Err(VmError::Runtime(format!(
                            "random_int: min must be Int, got {}",
                            other.type_name()
                        )));
                    }
                };
                let max_val = match &self.registers[base + a + 2] {
                    Value::Int(n) => *n,
                    other => {
                        return Err(VmError::Runtime(format!(
                            "random_int: max must be Int, got {}",
                            other.type_name()
                        )));
                    }
                };
                if min_val > max_val {
                    return Err(VmError::Runtime(format!(
                        "random_int: min ({}) must be <= max ({})",
                        min_val, max_val
                    )));
                }
                use std::cell::Cell;
                thread_local! {
                    static RNG_STATE_INT: Cell<u64> = const { Cell::new(0) };
                }
                RNG_STATE_INT.with(|state| {
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
                    let range = (max_val - min_val + 1) as u64;
                    let result = min_val + (s % range) as i64;
                    Ok(Value::Int(result))
                })
            }
            "to_list" => {
                let arg = self.registers[base + a + 1].clone();
                match arg {
                    Value::List(_) => Ok(arg),
                    Value::Set(s) => {
                        let sorted: Vec<Value> = s.iter().cloned().collect();
                        Ok(Value::new_list(sorted))
                    }
                    Value::Map(m) => {
                        let pairs: Vec<Value> = m
                            .iter()
                            .map(|(k, v)| {
                                Value::new_list(vec![
                                    Value::String(StringRef::Owned(k.clone())),
                                    v.clone(),
                                ])
                            })
                            .collect();
                        Ok(Value::new_list(pairs))
                    }
                    Value::Tuple(t) => Ok(Value::new_list(t.to_vec())),
                    Value::String(ref sr) => {
                        let s = match sr {
                            StringRef::Owned(s) => s.as_str(),
                            StringRef::Interned(id) => self.strings.resolve(*id).unwrap_or(""),
                        };
                        let chars: Vec<Value> = s
                            .chars()
                            .map(|c| Value::String(StringRef::Owned(c.to_string())))
                            .collect();
                        Ok(Value::new_list(chars))
                    }
                    other => Err(VmError::Runtime(format!(
                        "to_list: cannot convert {} to list",
                        other.type_name()
                    ))),
                }
            }
            // ── T216: Format specifier builtin ──
            "__format_spec" => {
                if nargs != 2 {
                    return Err(VmError::Runtime(
                        "__format_spec requires 2 arguments".into(),
                    ));
                }
                let value = self.registers[base + a + 1].clone();
                let fmt_str = match &self.registers[base + a + 2] {
                    Value::String(StringRef::Owned(s)) => s.clone(),
                    Value::String(StringRef::Interned(id)) => {
                        self.strings.resolve(*id).unwrap_or("").to_string()
                    }
                    _ => {
                        return Err(VmError::Runtime(
                            "__format_spec: format spec must be a string".into(),
                        ))
                    }
                };
                let result = format_value_with_spec(&value, &fmt_str)?;
                Ok(Value::String(StringRef::Owned(result)))
            }

            // ── T123: Wrapping arithmetic builtins ──
            "wrapping_add" => {
                if nargs != 2 {
                    return Err(VmError::Runtime("wrapping_add requires 2 arguments".into()));
                }
                match (&self.registers[base + a + 1], &self.registers[base + a + 2]) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_add(*b))),
                    _ => Err(VmError::Runtime(
                        "wrapping_add requires two integers".into(),
                    )),
                }
            }
            "wrapping_sub" => {
                if nargs != 2 {
                    return Err(VmError::Runtime("wrapping_sub requires 2 arguments".into()));
                }
                match (&self.registers[base + a + 1], &self.registers[base + a + 2]) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_sub(*b))),
                    _ => Err(VmError::Runtime(
                        "wrapping_sub requires two integers".into(),
                    )),
                }
            }
            "wrapping_mul" => {
                if nargs != 2 {
                    return Err(VmError::Runtime("wrapping_mul requires 2 arguments".into()));
                }
                match (&self.registers[base + a + 1], &self.registers[base + a + 2]) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_mul(*b))),
                    _ => Err(VmError::Runtime(
                        "wrapping_mul requires two integers".into(),
                    )),
                }
            }

            // ── List mutation: push alias for append ──
            "push" => {
                let item = self.registers[base + a + 2].clone();
                let list = &mut self.registers[base + a + 1];
                match list {
                    Value::List(l) => {
                        let l = std::sync::Arc::make_mut(l);
                        l.push(item);
                        Ok(Value::Null)
                    }
                    _ => Err(VmError::Runtime(
                        "push requires a list as first argument".into(),
                    )),
                }
            }

            // ── String-to-number parsing: parse_int / parse_float ──
            "parse_int" => {
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                let tag_ok = self.tag_ok;
                let tag_err = self.tag_err;
                match s.trim().parse::<i64>() {
                    Ok(n) => Ok(Value::Union(UnionValue {
                        tag: tag_ok,
                        payload: Arc::new(Value::Int(n)),
                    })),
                    Err(_) => match s.trim().parse::<BigInt>() {
                        Ok(n) => Ok(Value::Union(UnionValue {
                            tag: tag_ok,
                            payload: Arc::new(Value::BigInt(n)),
                        })),
                        Err(_) => Ok(Value::Union(UnionValue {
                            tag: tag_err,
                            payload: Arc::new(Value::String(StringRef::Owned(format!(
                                "invalid integer: {}",
                                s
                            )))),
                        })),
                    },
                }
            }
            "parse_float" => {
                let s = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                let tag_ok = self.tag_ok;
                let tag_err = self.tag_err;
                match s.trim().parse::<f64>() {
                    Ok(f) => Ok(Value::Union(UnionValue {
                        tag: tag_ok,
                        payload: Arc::new(Value::Float(f)),
                    })),
                    Err(_) => Ok(Value::Union(UnionValue {
                        tag: tag_err,
                        payload: Arc::new(Value::String(StringRef::Owned(format!(
                            "invalid float: {}",
                            s
                        )))),
                    })),
                }
            }

            // ── Bytes builtins: ASCII/hex conversion and slicing ──
            "bytes_from_ascii" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::String(sref) => {
                        let s = match sref {
                            StringRef::Owned(s) => s.clone(),
                            StringRef::Interned(id) => {
                                self.strings.resolve(*id).unwrap_or("").to_string()
                            }
                        };
                        Value::Bytes(s.into_bytes())
                    }
                    _ => Value::Null,
                })
            }
            "bytes_to_ascii" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Bytes(b) => match String::from_utf8(b.clone()) {
                        Ok(s) => Value::String(StringRef::Owned(s)),
                        Err(_) => Value::Null,
                    },
                    _ => Value::Null,
                })
            }
            "bytes_len" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Bytes(b) => Value::Int(b.len() as i64),
                    _ => Value::Null,
                })
            }
            "bytes_slice" => {
                let arg = &self.registers[base + a + 1];
                let start = &self.registers[base + a + 2];
                let end = &self.registers[base + a + 3];
                Ok(match (arg, start, end) {
                    (Value::Bytes(b), Value::Int(s), Value::Int(e)) => {
                        let s = *s as usize;
                        let e = if *e <= 0 { b.len() } else { *e as usize };
                        if s <= e && e <= b.len() {
                            Value::Bytes(b[s..e].to_vec())
                        } else {
                            Value::Bytes(vec![])
                        }
                    }
                    _ => Value::Null,
                })
            }
            "bytes_concat" => {
                let arg1 = &self.registers[base + a + 1];
                let arg2 = &self.registers[base + a + 2];
                Ok(match (arg1, arg2) {
                    (Value::Bytes(a_bytes), Value::Bytes(b_bytes)) => {
                        let mut result = a_bytes.clone();
                        result.extend_from_slice(b_bytes);
                        Value::Bytes(result)
                    }
                    _ => Value::Null,
                })
            }

            // ── Schema validation builtin ──
            "validate" => {
                if nargs < 2 {
                    let val = &self.registers[base + a + 1];
                    Ok(Value::Bool(!matches!(val, Value::Null)))
                } else {
                    let val = &self.registers[base + a + 1];
                    let schema_val = &self.registers[base + a + 2];
                    Ok(Value::Bool(validate_value_against_schema(
                        val,
                        schema_val,
                        &self.strings,
                    )))
                }
            }

            // ── Filesystem: read_lines, walk_dir, glob ──
            "read_lines" => {
                let path =
                    value_to_str_cow(&self.registers[base + a + 1], &self.strings).into_owned();
                match std::fs::read_to_string(&path) {
                    Ok(contents) => {
                        let lines: Vec<Value> = contents
                            .lines()
                            .map(|l| Value::String(StringRef::Owned(l.to_string())))
                            .collect();
                        Ok(Value::new_list(lines))
                    }
                    Err(e) => Err(VmError::Runtime(format!("read_lines failed: {}", e))),
                }
            }
            "walk_dir" => {
                let path =
                    value_to_str_cow(&self.registers[base + a + 1], &self.strings).into_owned();
                fn walk_recursive_cb(
                    dir: &std::path::Path,
                    result: &mut Vec<Value>,
                ) -> Result<(), VmError> {
                    let entries = std::fs::read_dir(dir)
                        .map_err(|e| VmError::Runtime(format!("walk_dir failed: {}", e)))?;
                    for entry in entries.flatten() {
                        let p = entry.path();
                        result.push(Value::String(StringRef::Owned(
                            p.to_string_lossy().to_string(),
                        )));
                        if p.is_dir() {
                            walk_recursive_cb(&p, result)?;
                        }
                    }
                    Ok(())
                }
                let mut result = Vec::new();
                walk_recursive_cb(std::path::Path::new(&path), &mut result)?;
                Ok(Value::new_list(result))
            }
            "glob" => {
                let pattern =
                    value_to_str_cow(&self.registers[base + a + 1], &self.strings).into_owned();
                let mut result = Vec::new();
                glob_walk(std::path::Path::new("."), &pattern, &mut result)?;
                Ok(Value::new_list(result))
            }

            // ── Path operations ──
            "path_join" => {
                let a_str =
                    value_to_str_cow(&self.registers[base + a + 1], &self.strings).into_owned();
                let b_str =
                    value_to_str_cow(&self.registers[base + a + 2], &self.strings).into_owned();
                let joined = std::path::Path::new(&a_str)
                    .join(&b_str)
                    .to_string_lossy()
                    .to_string();
                Ok(Value::String(StringRef::Owned(joined)))
            }
            "path_parent" => {
                let p_str =
                    value_to_str_cow(&self.registers[base + a + 1], &self.strings).into_owned();
                let parent = std::path::Path::new(&p_str)
                    .parent()
                    .map(|pp| pp.to_string_lossy().to_string())
                    .unwrap_or_default();
                Ok(Value::String(StringRef::Owned(parent)))
            }
            "path_extension" => {
                let p_str =
                    value_to_str_cow(&self.registers[base + a + 1], &self.strings).into_owned();
                let ext = std::path::Path::new(&p_str)
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default();
                Ok(Value::String(StringRef::Owned(ext)))
            }
            "path_filename" => {
                let p_str =
                    value_to_str_cow(&self.registers[base + a + 1], &self.strings).into_owned();
                let name = std::path::Path::new(&p_str)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                Ok(Value::String(StringRef::Owned(name)))
            }
            "path_stem" => {
                let p_str =
                    value_to_str_cow(&self.registers[base + a + 1], &self.strings).into_owned();
                let stem = std::path::Path::new(&p_str)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                Ok(Value::String(StringRef::Owned(stem)))
            }

            // ── Process execution ──
            "exec" => {
                let cmd =
                    value_to_str_cow(&self.registers[base + a + 1], &self.strings).into_owned();
                match std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                        let status = output.status.code().unwrap_or(-1) as i64;
                        let mut map = BTreeMap::new();
                        map.insert(
                            "stdout".to_string(),
                            Value::String(StringRef::Owned(stdout)),
                        );
                        map.insert(
                            "stderr".to_string(),
                            Value::String(StringRef::Owned(stderr)),
                        );
                        map.insert("status".to_string(), Value::Int(status));
                        Ok(Value::new_map(map))
                    }
                    Err(e) => Err(VmError::Runtime(format!("exec failed: {}", e))),
                }
            }

            // ── Stdin reading ──
            "read_stdin" => {
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin()
                    .lock()
                    .read_to_string(&mut buf)
                    .map_err(|e| VmError::Runtime(format!("read_stdin failed: {}", e)))?;
                Ok(Value::String(StringRef::Owned(buf)))
            }
            "read_line" => {
                use std::io::BufRead;
                let mut line = String::new();
                std::io::stdin()
                    .lock()
                    .read_line(&mut line)
                    .map_err(|e| VmError::Runtime(format!("read_line failed: {}", e)))?;
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                Ok(Value::String(StringRef::Owned(line)))
            }

            // ── Stderr output ──
            "eprint" => {
                let msg = self.registers[base + a + 1].display_pretty();
                eprint!("{}", msg);
                Ok(Value::Null)
            }
            "eprintln" => {
                let msg = self.registers[base + a + 1].display_pretty();
                eprintln!("{}", msg);
                Ok(Value::Null)
            }

            // ── CSV ──
            "csv_parse" => {
                let text = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                Ok(csv_parse_text(&text))
            }
            "csv_encode" => {
                let val = &self.registers[base + a + 1];
                Ok(Value::String(StringRef::Owned(csv_encode_value(val))))
            }

            // ── TOML ──
            "toml_parse" => {
                let text = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                Ok(toml_parse_text(&text))
            }
            "toml_encode" => {
                let val = &self.registers[base + a + 1];
                Ok(Value::String(StringRef::Owned(toml_encode_value(val, ""))))
            }

            // ── Regex ──
            "regex_match" => {
                let pattern = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                let text = value_to_str_cow(&self.registers[base + a + 2], &self.strings);
                match regex::Regex::new(&pattern) {
                    Ok(re) => {
                        if let Some(caps) = re.captures(&text) {
                            let groups: Vec<Value> = caps
                                .iter()
                                .map(|m| match m {
                                    Some(m) => {
                                        Value::String(StringRef::Owned(m.as_str().to_string()))
                                    }
                                    None => Value::Null,
                                })
                                .collect();
                            Ok(Value::new_list(groups))
                        } else {
                            Ok(Value::new_list(vec![]))
                        }
                    }
                    Err(e) => Err(VmError::Runtime(format!(
                        "regex_match: invalid pattern: {}",
                        e
                    ))),
                }
            }
            "regex_replace" => {
                let pattern = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                let text = value_to_str_cow(&self.registers[base + a + 2], &self.strings);
                let replacement = value_to_str_cow(&self.registers[base + a + 3], &self.strings);
                match regex::Regex::new(&pattern) {
                    Ok(re) => {
                        let result = re.replace_all(&text, &*replacement);
                        Ok(Value::String(StringRef::Owned(result.to_string())))
                    }
                    Err(e) => Err(VmError::Runtime(format!(
                        "regex_replace: invalid pattern: {}",
                        e
                    ))),
                }
            }
            "regex_find_all" => {
                let pattern = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                let text = value_to_str_cow(&self.registers[base + a + 2], &self.strings);
                match regex::Regex::new(&pattern) {
                    Ok(re) => {
                        let matches: Vec<Value> = re
                            .find_iter(&text)
                            .map(|m| Value::String(StringRef::Owned(m.as_str().to_string())))
                            .collect();
                        Ok(Value::new_list(matches))
                    }
                    Err(e) => Err(VmError::Runtime(format!(
                        "regex_find_all: invalid pattern: {}",
                        e
                    ))),
                }
            }

            // ── String concat ──
            "string_concat" => {
                let arg = &self.registers[base + a + 1];
                if let Value::List(l) = arg {
                    let mut buf = String::new();
                    for item in l.iter() {
                        buf.push_str(&item.display_pretty());
                    }
                    Ok(Value::String(StringRef::Owned(buf)))
                } else {
                    Ok(Value::String(StringRef::Owned(arg.display_pretty())))
                }
            }

            // ----- HTTP client builtins -----
            "http_get" => {
                let url = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                Ok(http_builtin_get(&url))
            }
            "http_post" => {
                let url = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                let body = if nargs > 1 {
                    value_to_str_cow(&self.registers[base + a + 2], &self.strings)
                } else {
                    std::borrow::Cow::Borrowed("")
                };
                Ok(http_builtin_post(&url, &body))
            }
            "http_put" => {
                let url = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                let body = if nargs > 1 {
                    value_to_str_cow(&self.registers[base + a + 2], &self.strings)
                } else {
                    std::borrow::Cow::Borrowed("")
                };
                Ok(http_builtin_put(&url, &body))
            }
            "http_delete" => {
                let url = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                Ok(http_builtin_delete(&url))
            }
            "http_request" => {
                let method = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                let url = value_to_str_cow(&self.registers[base + a + 2], &self.strings);
                let body = if nargs > 2 {
                    value_to_str_cow(&self.registers[base + a + 3], &self.strings)
                } else {
                    std::borrow::Cow::Borrowed("")
                };
                let headers = if nargs > 3 {
                    extract_headers_map(&self.registers[base + a + 4])
                } else {
                    Vec::new()
                };
                Ok(http_builtin_request(&method, &url, &body, &headers))
            }

            // ----- TCP/UDP networking builtins -----
            "tcp_connect" => {
                let addr = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                Ok(net_tcp_connect(&addr))
            }
            "tcp_listen" => {
                let addr = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                Ok(net_tcp_listen(&addr))
            }
            "tcp_send" => {
                let handle = self.registers[base + a + 1].as_int().unwrap_or(-1);
                let data = value_to_str_cow(&self.registers[base + a + 2], &self.strings);
                Ok(net_tcp_send(handle, &data))
            }
            "tcp_recv" => {
                let handle = self.registers[base + a + 1].as_int().unwrap_or(-1);
                let max_bytes = if nargs > 1 {
                    self.registers[base + a + 2].as_int().unwrap_or(4096)
                } else {
                    4096
                };
                Ok(net_tcp_recv(handle, max_bytes))
            }
            "tcp_close" => {
                let handle = self.registers[base + a + 1].as_int().unwrap_or(-1);
                net_tcp_close(handle);
                Ok(Value::Null)
            }
            "udp_bind" => {
                let addr = value_to_str_cow(&self.registers[base + a + 1], &self.strings);
                Ok(net_udp_bind(&addr))
            }
            "udp_send" => {
                let handle = self.registers[base + a + 1].as_int().unwrap_or(-1);
                let addr = value_to_str_cow(&self.registers[base + a + 2], &self.strings);
                let data = value_to_str_cow(&self.registers[base + a + 3], &self.strings);
                Ok(net_udp_send(handle, &addr, &data))
            }
            "udp_recv" => {
                let handle = self.registers[base + a + 1].as_int().unwrap_or(-1);
                let max_bytes = if nargs > 1 {
                    self.registers[base + a + 2].as_int().unwrap_or(4096)
                } else {
                    4096
                };
                Ok(net_udp_recv(handle, max_bytes))
            }

            // Wave 4A: stdlib completeness (T361-T370)
            "map_sorted_keys" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Map(m) => Value::new_list(
                        m.keys()
                            .map(|k| Value::String(StringRef::Owned(k.clone())))
                            .collect(),
                    ),
                    _ => Value::new_list(vec![]),
                })
            }
            "log2" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.log2()),
                    Value::Int(n) => Value::Float((*n as f64).log2()),
                    Value::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::INFINITY).log2()),
                    _ => Value::Null,
                })
            }
            "log10" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.log10()),
                    Value::Int(n) => Value::Float((*n as f64).log10()),
                    Value::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::INFINITY).log10()),
                    _ => Value::Null,
                })
            }
            "is_nan" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Bool(f.is_nan()),
                    _ => Value::Bool(false),
                })
            }
            "is_infinite" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Bool(f.is_infinite()),
                    _ => Value::Bool(false),
                })
            }
            "math_pi" => Ok(Value::Float(std::f64::consts::PI)),
            "math_e" => Ok(Value::Float(std::f64::consts::E)),
            "sort_asc" => {
                let arg = &self.registers[base + a + 1];
                if let Value::List(l) = arg {
                    let mut s: Vec<Value> = (**l).clone();
                    s.sort();
                    Ok(Value::new_list(s))
                } else {
                    Ok(arg.clone())
                }
            }
            "sort_desc" => {
                let arg = &self.registers[base + a + 1];
                if let Value::List(l) = arg {
                    let mut s: Vec<Value> = (**l).clone();
                    s.sort();
                    s.reverse();
                    Ok(Value::new_list(s))
                } else {
                    Ok(arg.clone())
                }
            }
            "sort_by" => {
                let arg = self.registers[base + a + 1].clone();
                let closure_val = self.registers[base + a + 2].clone();
                if let (Value::List(l), Value::Closure(cv)) = (arg.clone(), closure_val) {
                    let mut items: Vec<Value> = (**l).to_vec();
                    let len = items.len();
                    for i in 1..len {
                        let mut j = i;
                        while j > 0 {
                            let cmp_result = self.call_closure_sync(
                                &cv,
                                &[items[j - 1].clone(), items[j].clone()],
                            )?;
                            let cmp_val = cmp_result.as_int().unwrap_or(0);
                            if cmp_val > 0 {
                                items.swap(j - 1, j);
                                j -= 1;
                            } else {
                                break;
                            }
                        }
                    }
                    Ok(Value::new_list(items))
                } else {
                    Ok(arg)
                }
            }
            "binary_search" => {
                let arg = &self.registers[base + a + 1];
                let target = self.registers[base + a + 2].clone();
                let tag_ok = self.tag_ok;
                let tag_err = self.tag_err;
                if let Value::List(l) = arg {
                    match l.binary_search(&target) {
                        Ok(idx) => Ok(Value::Union(UnionValue {
                            tag: tag_ok,
                            payload: Arc::new(Value::Int(idx as i64)),
                        })),
                        Err(idx) => Ok(Value::Union(UnionValue {
                            tag: tag_err,
                            payload: Arc::new(Value::Int(idx as i64)),
                        })),
                    }
                } else {
                    Ok(Value::Union(UnionValue {
                        tag: tag_err,
                        payload: Arc::new(Value::Int(0)),
                    }))
                }
            }
            "hrtime" => {
                use std::sync::OnceLock;
                use std::time::Instant;
                static EPOCH: OnceLock<Instant> = OnceLock::new();
                let epoch = EPOCH.get_or_init(Instant::now);
                let elapsed = epoch.elapsed();
                Ok(Value::Int(elapsed.as_nanos() as i64))
            }
            "format_time" => {
                let timestamp_secs = match &self.registers[base + a + 1] {
                    Value::Float(f) => *f,
                    Value::Int(n) => *n as f64,
                    _ => 0.0,
                };
                let format_str = value_to_str_cow(&self.registers[base + a + 2], &self.strings);
                let total_secs = timestamp_secs as i64;
                let frac = timestamp_secs - total_secs as f64;
                let (year, month, day, hour, minute, second) = epoch_to_datetime(total_secs);
                let result = if format_str == "iso8601"
                    || format_str == "ISO8601"
                    || format_str.is_empty()
                {
                    if frac.abs() < 0.001 {
                        format!(
                            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                            year, month, day, hour, minute, second
                        )
                    } else {
                        format!(
                            "{:04}-{:02}-{:02}T{:02}:{:02}:{:06.3}Z",
                            year,
                            month,
                            day,
                            hour,
                            minute,
                            second as f64 + frac
                        )
                    }
                } else {
                    format_str
                        .replace("%Y", &format!("{:04}", year))
                        .replace("%m", &format!("{:02}", month))
                        .replace("%d", &format!("{:02}", day))
                        .replace("%H", &format!("{:02}", hour))
                        .replace("%M", &format!("{:02}", minute))
                        .replace("%S", &format!("{:02}", second))
                };
                Ok(Value::String(StringRef::Owned(result)))
            }
            "args" => {
                let args: Vec<Value> = std::env::args()
                    .map(|a| Value::String(StringRef::Owned(a)))
                    .collect();
                Ok(Value::new_list(args))
            }
            "set_env" => {
                let key =
                    value_to_str_cow(&self.registers[base + a + 1], &self.strings).into_owned();
                let value =
                    value_to_str_cow(&self.registers[base + a + 2], &self.strings).into_owned();
                #[allow(unused_unsafe)]
                unsafe {
                    std::env::set_var(&key, &value);
                }
                Ok(Value::Null)
            }
            "env_vars" => {
                let mut map = BTreeMap::new();
                for (key, value) in std::env::vars() {
                    map.insert(key, Value::String(StringRef::Owned(value)));
                }
                Ok(Value::new_map(map))
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
            .resize(new_base + num_regs.max(16), Value::Null);
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
                    Value::String(_) => {
                        Value::Bool(!value_to_str_cow(arg, &self.strings).is_empty())
                    }
                    _ => Value::Bool(false),
                })
            }
            3 => {
                // HASH
                use sha2::{Digest, Sha256};
                let hash = format!(
                    "{:x}",
                    Sha256::digest(value_to_str_cow(arg, &self.strings).as_bytes())
                );
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
                // VALIDATE — 1-arg: not-null check; 2-arg: schema validation
                let nargs = _a.saturating_sub(arg_reg);
                if nargs < 2 {
                    // Single arg: validate(value) => not null
                    Ok(Value::Bool(!matches!(arg, Value::Null)))
                } else {
                    // Two args: validate(value, schema)
                    let schema_val = &self.registers[base + arg_reg + 1];
                    Ok(Value::Bool(validate_value_against_schema(
                        arg,
                        schema_val,
                        &self.strings,
                    )))
                }
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
                    Value::Map(m) => {
                        Value::Bool(m.contains_key(&*value_to_str_cow(item, &self.strings)))
                    }
                    Value::String(StringRef::Owned(s)) => {
                        Value::Bool(s.contains(&*value_to_str_cow(item, &self.strings)))
                    }
                    _ => Value::Bool(false),
                })
            }
            17 => {
                // JOIN
                let sep = value_to_str_cow(&self.registers[base + arg_reg + 1], &self.strings);
                Ok(match arg {
                    Value::List(l) => {
                        let s = l
                            .iter()
                            .map(|v| value_to_str_cow(v, &self.strings).into_owned())
                            .collect::<Vec<_>>()
                            .join(&sep);
                        Value::String(StringRef::Owned(s))
                    }
                    _ => Value::String(StringRef::Owned("".into())),
                })
            }
            18 => {
                // SPLIT
                let sep = value_to_str_cow(&self.registers[base + arg_reg + 1], &self.strings);
                Ok(match arg {
                    Value::String(StringRef::Owned(s)) => {
                        let parts: Vec<Value> = s
                            .split(&*sep)
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
                let pat = value_to_str_cow(&self.registers[base + arg_reg + 1], &self.strings);
                let with = value_to_str_cow(&self.registers[base + arg_reg + 2], &self.strings);
                Ok(match arg {
                    Value::String(StringRef::Owned(s)) => {
                        Value::String(StringRef::Owned(s.replace(pat.as_ref(), with.as_ref())))
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
                // APPEND — O(1) amortized via take + Arc::make_mut
                let item = std::mem::take(&mut self.registers[base + arg_reg + 1]);
                let taken = std::mem::take(&mut self.registers[base + arg_reg]);
                Ok(match taken {
                    Value::List(mut l) => {
                        Arc::make_mut(&mut l).push(item);
                        Value::List(l)
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
                        let key_str = value_to_str_cow(&key, &self.strings).into_owned();
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
                let s = value_to_str_cow(arg, &self.strings);
                Ok(Value::new_list(
                    s.chars()
                        .map(|c| Value::String(StringRef::Owned(c.to_string())))
                        .collect(),
                ))
            }
            52 => {
                // STARTSWITH
                let prefix = value_to_str_cow(&self.registers[base + arg_reg + 1], &self.strings);
                Ok(Value::Bool(
                    value_to_str_cow(arg, &self.strings).starts_with(&*prefix),
                ))
            }
            53 => {
                // ENDSWITH
                let suffix = value_to_str_cow(&self.registers[base + arg_reg + 1], &self.strings);
                Ok(Value::Bool(
                    value_to_str_cow(arg, &self.strings).ends_with(&*suffix),
                ))
            }
            54 => {
                // INDEXOF (Unicode-aware: returns character index, not byte index)
                let needle = value_to_str_cow(&self.registers[base + arg_reg + 1], &self.strings);
                let haystack = value_to_str_cow(arg, &self.strings);
                Ok(match haystack.find(&*needle) {
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
                let s = value_to_str_cow(arg, &self.strings);
                let char_count = s.chars().count();
                if char_count < width {
                    let padding = " ".repeat(width - char_count);
                    Ok(Value::String(StringRef::Owned(format!("{}{}", padding, s))))
                } else {
                    Ok(Value::String(StringRef::Owned(s.into_owned())))
                }
            }
            56 => {
                // PADRIGHT (Unicode-aware)
                let width = self.registers[base + arg_reg + 1].as_int().unwrap_or(0) as usize;
                let s = value_to_str_cow(arg, &self.strings);
                let char_count = s.chars().count();
                if char_count < width {
                    let padding = " ".repeat(width - char_count);
                    Ok(Value::String(StringRef::Owned(format!("{}{}", s, padding))))
                } else {
                    Ok(Value::String(StringRef::Owned(s.into_owned())))
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
                let key = value_to_str_cow(&self.registers[base + arg_reg + 1], &self.strings)
                    .into_owned();
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
                        let key = value_to_str_cow(&item, &self.strings).into_owned();
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
                let template = value_to_str_cow(arg, &self.strings);
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
                let path = value_to_str_cow(arg, &self.strings).into_owned();
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
                let path = value_to_str_cow(arg, &self.strings).into_owned();
                Ok(Value::Bool(std::path::Path::new(&path).exists()))
            }
            81 => {
                // MKDIR: create directory (and parents)
                let path = value_to_str_cow(arg, &self.strings).into_owned();
                match std::fs::create_dir_all(&path) {
                    Ok(()) => Ok(Value::Null),
                    Err(e) => Err(VmError::Runtime(format!("mkdir failed: {}", e))),
                }
            }
            82 => {
                // EVAL: compile and execute string code
                let source = value_to_str_cow(arg, &self.strings).into_owned();
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
            86 => {
                // READ_LINES: read file and split by newline
                let path = value_to_str_cow(arg, &self.strings).into_owned();
                match std::fs::read_to_string(&path) {
                    Ok(contents) => {
                        let lines: Vec<Value> = contents
                            .lines()
                            .map(|l| Value::String(StringRef::Owned(l.to_string())))
                            .collect();
                        Ok(Value::new_list(lines))
                    }
                    Err(e) => Err(VmError::Runtime(format!("read_lines failed: {}", e))),
                }
            }
            87 => {
                // WALK_DIR: recursively list all file paths
                let path = value_to_str_cow(arg, &self.strings).into_owned();
                fn walk_recursive(
                    dir: &std::path::Path,
                    result: &mut Vec<Value>,
                ) -> Result<(), VmError> {
                    let entries = std::fs::read_dir(dir)
                        .map_err(|e| VmError::Runtime(format!("walk_dir failed: {}", e)))?;
                    for entry in entries.flatten() {
                        let p = entry.path();
                        result.push(Value::String(StringRef::Owned(
                            p.to_string_lossy().to_string(),
                        )));
                        if p.is_dir() {
                            walk_recursive(&p, result)?;
                        }
                    }
                    Ok(())
                }
                let mut result = Vec::new();
                walk_recursive(std::path::Path::new(&path), &mut result)?;
                Ok(Value::new_list(result))
            }
            88 => {
                // GLOB: simple glob matching over current directory
                let pattern = value_to_str_cow(arg, &self.strings).into_owned();
                let mut result = Vec::new();
                glob_walk(std::path::Path::new("."), &pattern, &mut result)?;
                Ok(Value::new_list(result))
            }
            89 => {
                // PATH_JOIN: join two path components
                let other = value_to_str_cow(&self.registers[base + arg_reg + 1], &self.strings)
                    .into_owned();
                let joined =
                    std::path::Path::new(&value_to_str_cow(arg, &self.strings).into_owned())
                        .join(&other)
                        .to_string_lossy()
                        .to_string();
                Ok(Value::String(StringRef::Owned(joined)))
            }
            90 => {
                // PATH_PARENT
                let s = value_to_str_cow(arg, &self.strings).into_owned();
                let p = std::path::Path::new(&s);
                let parent = p
                    .parent()
                    .map(|pp| pp.to_string_lossy().to_string())
                    .unwrap_or_default();
                Ok(Value::String(StringRef::Owned(parent)))
            }
            91 => {
                // PATH_EXTENSION
                let s = value_to_str_cow(arg, &self.strings).into_owned();
                let p = std::path::Path::new(&s);
                let ext = p
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default();
                Ok(Value::String(StringRef::Owned(ext)))
            }
            92 => {
                // PATH_FILENAME
                let s = value_to_str_cow(arg, &self.strings).into_owned();
                let p = std::path::Path::new(&s);
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                Ok(Value::String(StringRef::Owned(name)))
            }
            93 => {
                // PATH_STEM
                let s = value_to_str_cow(arg, &self.strings).into_owned();
                let p = std::path::Path::new(&s);
                let stem = p
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                Ok(Value::String(StringRef::Owned(stem)))
            }
            94 => {
                // EXEC: run shell command
                let cmd = value_to_str_cow(arg, &self.strings).into_owned();
                match std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                        let status = output.status.code().unwrap_or(-1) as i64;
                        let mut map = BTreeMap::new();
                        map.insert(
                            "stdout".to_string(),
                            Value::String(StringRef::Owned(stdout)),
                        );
                        map.insert(
                            "stderr".to_string(),
                            Value::String(StringRef::Owned(stderr)),
                        );
                        map.insert("status".to_string(), Value::Int(status));
                        Ok(Value::new_map(map))
                    }
                    Err(e) => Err(VmError::Runtime(format!("exec failed: {}", e))),
                }
            }
            95 => {
                // READ_STDIN: read all of stdin to string
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin()
                    .lock()
                    .read_to_string(&mut buf)
                    .map_err(|e| VmError::Runtime(format!("read_stdin failed: {}", e)))?;
                Ok(Value::String(StringRef::Owned(buf)))
            }
            96 => {
                // EPRINT
                eprint!("{}", arg.display_pretty());
                Ok(Value::Null)
            }
            97 => {
                // EPRINTLN
                eprintln!("{}", arg.display_pretty());
                Ok(Value::Null)
            }
            98 => {
                // CSV_PARSE: parse CSV text into list of lists of strings
                let text = value_to_str_cow(arg, &self.strings);
                let rows = csv_parse_text(&text);
                Ok(rows)
            }
            99 => {
                // CSV_ENCODE: convert list of lists to CSV string
                let encoded = csv_encode_value(arg);
                Ok(Value::String(StringRef::Owned(encoded)))
            }
            100 => {
                // TOML_PARSE: parse TOML text into map
                let text = value_to_str_cow(arg, &self.strings);
                Ok(toml_parse_text(&text))
            }
            101 => {
                // TOML_ENCODE: convert map to TOML string
                let encoded = toml_encode_value(arg, "");
                Ok(Value::String(StringRef::Owned(encoded)))
            }
            102 => {
                // REGEX_MATCH: return capture groups for first match
                let pattern = value_to_str_cow(arg, &self.strings);
                let text = value_to_str_cow(&self.registers[base + arg_reg + 1], &self.strings);
                match regex::Regex::new(&pattern) {
                    Ok(re) => {
                        if let Some(caps) = re.captures(&text) {
                            let groups: Vec<Value> = caps
                                .iter()
                                .map(|m| match m {
                                    Some(m) => {
                                        Value::String(StringRef::Owned(m.as_str().to_string()))
                                    }
                                    None => Value::Null,
                                })
                                .collect();
                            Ok(Value::new_list(groups))
                        } else {
                            Ok(Value::new_list(vec![]))
                        }
                    }
                    Err(e) => Err(VmError::Runtime(format!(
                        "regex_match: invalid pattern: {}",
                        e
                    ))),
                }
            }
            103 => {
                // REGEX_REPLACE: replace all matches
                let pattern = value_to_str_cow(arg, &self.strings);
                let text = value_to_str_cow(&self.registers[base + arg_reg + 1], &self.strings);
                let replacement =
                    value_to_str_cow(&self.registers[base + arg_reg + 2], &self.strings);
                match regex::Regex::new(&pattern) {
                    Ok(re) => {
                        let result = re.replace_all(&text, &*replacement);
                        Ok(Value::String(StringRef::Owned(result.to_string())))
                    }
                    Err(e) => Err(VmError::Runtime(format!(
                        "regex_replace: invalid pattern: {}",
                        e
                    ))),
                }
            }
            104 => {
                // REGEX_FIND_ALL: return all matches
                let pattern = value_to_str_cow(arg, &self.strings);
                let text = value_to_str_cow(&self.registers[base + arg_reg + 1], &self.strings);
                match regex::Regex::new(&pattern) {
                    Ok(re) => {
                        let matches: Vec<Value> = re
                            .find_iter(&text)
                            .map(|m| Value::String(StringRef::Owned(m.as_str().to_string())))
                            .collect();
                        Ok(Value::new_list(matches))
                    }
                    Err(e) => Err(VmError::Runtime(format!(
                        "regex_find_all: invalid pattern: {}",
                        e
                    ))),
                }
            }
            105 => {
                // READ_LINE: read a single line from stdin
                use std::io::BufRead;
                let mut line = String::new();
                std::io::stdin()
                    .lock()
                    .read_line(&mut line)
                    .map_err(|e| VmError::Runtime(format!("read_line failed: {}", e)))?;
                // Trim trailing newline
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                Ok(Value::String(StringRef::Owned(line)))
            }
            106 => {
                // STRING_CONCAT: concatenate a list of values into a string
                if let Value::List(l) = arg {
                    let mut buf = String::new();
                    for item in l.iter() {
                        buf.push_str(&item.display_pretty());
                    }
                    Ok(Value::String(StringRef::Owned(buf)))
                } else {
                    Ok(Value::String(StringRef::Owned(arg.display_pretty())))
                }
            }
            107 => {
                // HTTP_GET
                let url = value_to_str_cow(arg, &self.strings);
                Ok(http_builtin_get(&url))
            }
            108 => {
                // HTTP_POST
                let url = value_to_str_cow(arg, &self.strings);
                let body = value_to_str_cow(&self.registers[base + arg_reg + 1], &self.strings);
                Ok(http_builtin_post(&url, &body))
            }
            109 => {
                // HTTP_PUT
                let url = value_to_str_cow(arg, &self.strings);
                let body = value_to_str_cow(&self.registers[base + arg_reg + 1], &self.strings);
                Ok(http_builtin_put(&url, &body))
            }
            110 => {
                // HTTP_DELETE
                let url = value_to_str_cow(arg, &self.strings);
                Ok(http_builtin_delete(&url))
            }
            111 => {
                // HTTP_REQUEST
                let method = value_to_str_cow(arg, &self.strings);
                let url = value_to_str_cow(&self.registers[base + arg_reg + 1], &self.strings);
                let body = value_to_str_cow(&self.registers[base + arg_reg + 2], &self.strings);
                let headers = extract_headers_map(&self.registers[base + arg_reg + 3]);
                Ok(http_builtin_request(&method, &url, &body, &headers))
            }
            112 => {
                // TCP_CONNECT
                let addr = value_to_str_cow(arg, &self.strings);
                Ok(net_tcp_connect(&addr))
            }
            113 => {
                // TCP_LISTEN
                let addr = value_to_str_cow(arg, &self.strings);
                Ok(net_tcp_listen(&addr))
            }
            114 => {
                // TCP_SEND
                let handle = arg.as_int().unwrap_or(-1);
                let data = value_to_str_cow(&self.registers[base + arg_reg + 1], &self.strings);
                Ok(net_tcp_send(handle, &data))
            }
            115 => {
                // TCP_RECV
                let handle = arg.as_int().unwrap_or(-1);
                let max_bytes = self.registers[base + arg_reg + 1].as_int().unwrap_or(4096);
                Ok(net_tcp_recv(handle, max_bytes))
            }
            116 => {
                // UDP_BIND
                let addr = value_to_str_cow(arg, &self.strings);
                Ok(net_udp_bind(&addr))
            }
            117 => {
                // UDP_SEND
                let handle = arg.as_int().unwrap_or(-1);
                let addr = value_to_str_cow(&self.registers[base + arg_reg + 1], &self.strings);
                let data = value_to_str_cow(&self.registers[base + arg_reg + 2], &self.strings);
                Ok(net_udp_send(handle, &addr, &data))
            }
            118 => {
                // UDP_RECV
                let handle = arg.as_int().unwrap_or(-1);
                let max_bytes = self.registers[base + arg_reg + 1].as_int().unwrap_or(4096);
                Ok(net_udp_recv(handle, max_bytes))
            }
            119 => {
                // TCP_CLOSE
                let handle = arg.as_int().unwrap_or(-1);
                net_tcp_close(handle);
                Ok(Value::Null)
            }
            // Wave 4A: stdlib completeness (T361-T370)
            120 => {
                // MAP_SORTED_KEYS: return map keys in sorted order
                Ok(match arg {
                    Value::Map(m) => Value::new_list(
                        m.keys()
                            .map(|k| Value::String(StringRef::Owned(k.clone())))
                            .collect(),
                    ),
                    _ => Value::new_list(vec![]),
                })
            }
            121 => {
                // PARSE_INT: parse string to int, return result type
                let s = value_to_str_cow(arg, &self.strings);
                let tag_ok = self.tag_ok;
                let tag_err = self.tag_err;
                match s.trim().parse::<i64>() {
                    Ok(n) => Ok(Value::Union(UnionValue {
                        tag: tag_ok,
                        payload: Arc::new(Value::Int(n)),
                    })),
                    Err(_) => {
                        // Try BigInt
                        match s.trim().parse::<BigInt>() {
                            Ok(n) => Ok(Value::Union(UnionValue {
                                tag: tag_ok,
                                payload: Arc::new(Value::BigInt(n)),
                            })),
                            Err(_) => Ok(Value::Union(UnionValue {
                                tag: tag_err,
                                payload: Arc::new(Value::String(StringRef::Owned(format!(
                                    "invalid integer: {}",
                                    s
                                )))),
                            })),
                        }
                    }
                }
            }
            122 => {
                // PARSE_FLOAT: parse string to float, return result type
                let s = value_to_str_cow(arg, &self.strings);
                let tag_ok = self.tag_ok;
                let tag_err = self.tag_err;
                match s.trim().parse::<f64>() {
                    Ok(f) => Ok(Value::Union(UnionValue {
                        tag: tag_ok,
                        payload: Arc::new(Value::Float(f)),
                    })),
                    Err(_) => Ok(Value::Union(UnionValue {
                        tag: tag_err,
                        payload: Arc::new(Value::String(StringRef::Owned(format!(
                            "invalid float: {}",
                            s
                        )))),
                    })),
                }
            }
            123 => {
                // LOG2
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.log2()),
                    Value::Int(n) => Value::Float((*n as f64).log2()),
                    Value::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::INFINITY).log2()),
                    _ => Value::Null,
                })
            }
            124 => {
                // LOG10
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.log10()),
                    Value::Int(n) => Value::Float((*n as f64).log10()),
                    Value::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::INFINITY).log10()),
                    _ => Value::Null,
                })
            }
            125 => {
                // IS_NAN
                Ok(match arg {
                    Value::Float(f) => Value::Bool(f.is_nan()),
                    _ => Value::Bool(false),
                })
            }
            126 => {
                // IS_INFINITE
                Ok(match arg {
                    Value::Float(f) => Value::Bool(f.is_infinite()),
                    _ => Value::Bool(false),
                })
            }
            127 => {
                // MATH_PI
                Ok(Value::Float(std::f64::consts::PI))
            }
            128 => {
                // MATH_E
                Ok(Value::Float(std::f64::consts::E))
            }
            129 => {
                // SORT_ASC: sort list in ascending order
                if let Value::List(l) = arg {
                    let mut s: Vec<Value> = (**l).clone();
                    s.sort();
                    Ok(Value::new_list(s))
                } else {
                    Ok(arg.clone())
                }
            }
            130 => {
                // SORT_DESC: sort list in descending order
                if let Value::List(l) = arg {
                    let mut s: Vec<Value> = (**l).clone();
                    s.sort();
                    s.reverse();
                    Ok(Value::new_list(s))
                } else {
                    Ok(arg.clone())
                }
            }
            131 => {
                // SORT_BY: sort using a comparator closure (a, b) -> Int
                let closure_val = self.registers[base + arg_reg + 1].clone();
                if let (Value::List(l), Value::Closure(cv)) = (arg.clone(), closure_val) {
                    let mut items: Vec<Value> = l.as_ref().clone();
                    // Use a simple stable insertion sort to avoid issues with closures in sort
                    let len = items.len();
                    for i in 1..len {
                        let mut j = i;
                        while j > 0 {
                            let cmp_result = self.call_closure_sync(
                                &cv,
                                &[items[j - 1].clone(), items[j].clone()],
                            )?;
                            let cmp_val = cmp_result.as_int().unwrap_or(0);
                            if cmp_val > 0 {
                                items.swap(j - 1, j);
                                j -= 1;
                            } else {
                                break;
                            }
                        }
                    }
                    Ok(Value::new_list(items))
                } else {
                    Ok(arg.clone())
                }
            }
            132 => {
                // BINARY_SEARCH: binary search sorted list, return result[Int, Int]
                let target = self.registers[base + arg_reg + 1].clone();
                let tag_ok = self.tag_ok;
                let tag_err = self.tag_err;
                if let Value::List(l) = arg {
                    match l.binary_search(&target) {
                        Ok(idx) => Ok(Value::Union(UnionValue {
                            tag: tag_ok,
                            payload: Arc::new(Value::Int(idx as i64)),
                        })),
                        Err(idx) => Ok(Value::Union(UnionValue {
                            tag: tag_err,
                            payload: Arc::new(Value::Int(idx as i64)),
                        })),
                    }
                } else {
                    Ok(Value::Union(UnionValue {
                        tag: tag_err,
                        payload: Arc::new(Value::Int(0)),
                    }))
                }
            }
            133 => {
                // HRTIME: high-resolution monotonic timer in nanoseconds
                use std::sync::OnceLock;
                use std::time::Instant;
                static EPOCH: OnceLock<Instant> = OnceLock::new();
                let epoch = EPOCH.get_or_init(Instant::now);
                let elapsed = epoch.elapsed();
                Ok(Value::Int(elapsed.as_nanos() as i64))
            }
            134 => {
                // FORMAT_TIME: format epoch timestamp with format string
                let timestamp_secs = match arg {
                    Value::Float(f) => *f,
                    Value::Int(n) => *n as f64,
                    _ => 0.0,
                };
                let format_str =
                    value_to_str_cow(&self.registers[base + arg_reg + 1], &self.strings)
                        .into_owned();

                // Manual ISO 8601 formatting from epoch seconds
                let total_secs = timestamp_secs as i64;
                let frac = timestamp_secs - total_secs as f64;

                // Convert epoch seconds to date/time components
                let (year, month, day, hour, minute, second) = epoch_to_datetime(total_secs);

                let result = if format_str == "iso8601"
                    || format_str == "ISO8601"
                    || format_str.is_empty()
                {
                    if frac.abs() < 0.001 {
                        format!(
                            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                            year, month, day, hour, minute, second
                        )
                    } else {
                        format!(
                            "{:04}-{:02}-{:02}T{:02}:{:02}:{:06.3}Z",
                            year,
                            month,
                            day,
                            hour,
                            minute,
                            second as f64 + frac
                        )
                    }
                } else {
                    // Simple substitution: %Y, %m, %d, %H, %M, %S
                    format_str
                        .replace("%Y", &format!("{:04}", year))
                        .replace("%m", &format!("{:02}", month))
                        .replace("%d", &format!("{:02}", day))
                        .replace("%H", &format!("{:02}", hour))
                        .replace("%M", &format!("{:02}", minute))
                        .replace("%S", &format!("{:02}", second))
                };
                Ok(Value::String(StringRef::Owned(result)))
            }
            135 => {
                // ARGS: return command-line arguments
                let args: Vec<Value> = std::env::args()
                    .map(|a| Value::String(StringRef::Owned(a)))
                    .collect();
                Ok(Value::new_list(args))
            }
            136 => {
                // SET_ENV: set environment variable
                let key = value_to_str_cow(arg, &self.strings).into_owned();
                let value = value_to_str_cow(&self.registers[base + arg_reg + 1], &self.strings)
                    .into_owned();
                // SAFETY: This is intentional; Lumen programs run single-threaded.
                #[allow(unused_unsafe)]
                unsafe {
                    std::env::set_var(&key, &value);
                }
                Ok(Value::Null)
            }
            137 => {
                // ENV_VARS: return all environment variables as a map
                let mut map = BTreeMap::new();
                for (key, value) in std::env::vars() {
                    map.insert(key, Value::String(StringRef::Owned(value)));
                }
                Ok(Value::new_map(map))
            }
            138 => {
                // TAN
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.tan()),
                    Value::Int(n) => Value::Float((*n as f64).tan()),
                    Value::BigInt(n) => Value::Float(n.to_f64().unwrap_or(f64::NAN).tan()),
                    _ => Value::Null,
                })
            }
            139 => {
                // TRUNC
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.trunc()),
                    Value::Int(_) => arg.clone(),
                    Value::BigInt(_) => arg.clone(),
                    _ => Value::Null,
                })
            }
            _ => Err(VmError::Runtime(format!(
                "Unknown intrinsic ID {} - this is a compiler/VM mismatch bug",
                func_id
            ))),
        }
    }
}

/// Format a Value according to a format specifier string.
///
/// Supported specifiers (Python-style):
/// - `<fill><align><width>`  — fill char + alignment + width (e.g., `*^10`)
/// - `>N`   — right-align in width N (fill defaults to space)
/// - `<N`   — left-align in width N
/// - `^N`   — center-align in width N
/// - `.Nf`  — float with N decimal places (e.g., ".2f")
/// - `#x`   — hexadecimal (lowercase)
/// - `#o`   — octal
/// - `#b`   — binary
/// - `0N`   — zero-pad to width N
/// - `+`    — always show sign for numbers
fn format_value_with_spec(value: &Value, spec: &str) -> Result<String, VmError> {
    if spec.is_empty() {
        return Ok(value.display_pretty());
    }

    // Parse the spec into components
    let mut sign_plus = false;
    let mut zero_pad: Option<usize> = None;
    let mut fill_char: char = ' ';
    let mut align: Option<(char, usize)> = None; // (alignment_char, width)
    let mut precision: Option<usize> = None;
    let mut radix: Option<char> = None; // 'x', 'o', 'b'

    let chars: Vec<char> = spec.chars().collect();
    let mut i = 0;

    // Check for fill+align at the start: if chars[1] is an alignment char,
    // then chars[0] is the fill character.
    if chars.len() >= 2 && matches!(chars[1], '>' | '<' | '^') {
        fill_char = chars[0];
        i = 1; // skip the fill char, let the alignment arm handle chars[1]
    }

    while i < chars.len() {
        match chars[i] {
            '+' => {
                sign_plus = true;
                i += 1;
            }
            '#' if i + 1 < chars.len() => {
                radix = Some(chars[i + 1]);
                i += 2;
            }
            '.' => {
                // Parse .Nf
                i += 1;
                let start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if i > start {
                    if let Ok(n) = spec[start..i].parse::<usize>() {
                        precision = Some(n);
                    }
                }
                // Skip trailing 'f' if present
                if i < chars.len() && chars[i] == 'f' {
                    i += 1;
                }
            }
            '>' | '<' | '^' => {
                let align_char = chars[i];
                i += 1;
                let start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if i > start {
                    if let Ok(width) = spec[start..i].parse::<usize>() {
                        align = Some((align_char, width));
                    }
                }
            }
            '0' if i + 1 < chars.len() && chars[i + 1].is_ascii_digit() => {
                i += 1; // skip the '0'
                let start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if i > start {
                    if let Ok(width) = spec[start..i].parse::<usize>() {
                        zero_pad = Some(width);
                    }
                }
            }
            _ => {
                i += 1; // skip unrecognized
            }
        }
    }

    // Format the base string
    let mut formatted = if let Some(r) = radix {
        match (r, value) {
            ('x', Value::Int(n)) => format!("{:#x}", n),
            ('o', Value::Int(n)) => format!("{:#o}", n),
            ('b', Value::Int(n)) => format!("{:#b}", n),
            ('x', _) => {
                return Err(VmError::Runtime(
                    "__format_spec: #x requires an integer value".into(),
                ))
            }
            ('o', _) => {
                return Err(VmError::Runtime(
                    "__format_spec: #o requires an integer value".into(),
                ))
            }
            ('b', _) => {
                return Err(VmError::Runtime(
                    "__format_spec: #b requires an integer value".into(),
                ))
            }
            _ => {
                return Err(VmError::Runtime(format!(
                    "__format_spec: unknown radix specifier '#{}' ",
                    r
                )))
            }
        }
    } else if let Some(prec) = precision {
        match value {
            Value::Float(f) => format!("{:.prec$}", f, prec = prec),
            Value::Int(n) => format!("{:.prec$}", *n as f64, prec = prec),
            _ => {
                return Err(VmError::Runtime(
                    "__format_spec: precision format requires a numeric value".into(),
                ))
            }
        }
    } else {
        value.display_pretty()
    };

    // Apply sign
    if sign_plus {
        match value {
            Value::Int(n) if *n >= 0 && radix.is_none() => {
                formatted = format!("+{}", formatted);
            }
            Value::Float(f) if *f >= 0.0 && radix.is_none() => {
                formatted = format!("+{}", formatted);
            }
            _ => {} // negative numbers already have sign, or non-numeric
        }
    }

    // Apply zero-padding
    if let Some(width) = zero_pad {
        if formatted.len() < width {
            let is_negative = formatted.starts_with('-');
            let is_positive_sign = formatted.starts_with('+');
            if is_negative || is_positive_sign {
                let sign = &formatted[..1];
                let rest = &formatted[1..];
                let pad_len = width.saturating_sub(formatted.len());
                formatted = format!("{}{}{}", sign, "0".repeat(pad_len), rest);
            } else {
                let pad_len = width.saturating_sub(formatted.len());
                formatted = format!("{}{}", "0".repeat(pad_len), formatted);
            }
        }
    }

    // Apply alignment with fill character
    if let Some((align_char, width)) = align {
        if formatted.len() < width {
            let pad_len = width - formatted.len();
            match align_char {
                '>' => {
                    let padding: String = std::iter::repeat_n(fill_char, pad_len).collect();
                    formatted = format!("{}{}", padding, formatted);
                }
                '<' => {
                    let padding: String = std::iter::repeat_n(fill_char, pad_len).collect();
                    formatted = format!("{}{}", formatted, padding);
                }
                '^' => {
                    let left_pad = pad_len / 2;
                    let right_pad = pad_len - left_pad;
                    let left: String = std::iter::repeat_n(fill_char, left_pad).collect();
                    let right: String = std::iter::repeat_n(fill_char, right_pad).collect();
                    formatted = format!("{}{}{}", left, formatted, right);
                }
                _ => {}
            }
        }
    }

    Ok(formatted)
}

// ── Schema validation helper functions ──

/// Check if a runtime Value matches a type name string.
fn validate_value_against_type_name(val: &Value, type_name: &str) -> bool {
    match type_name {
        "Any" => true,
        "Int" | "int" => matches!(val, Value::Int(_)),
        "Float" | "float" => matches!(val, Value::Float(_)),
        "String" | "string" => matches!(val, Value::String(_)),
        "Bool" | "bool" => matches!(val, Value::Bool(_)),
        "Null" | "null" => matches!(val, Value::Null),
        "List" | "list" => matches!(val, Value::List(_)),
        "Map" | "map" => matches!(val, Value::Map(_)),
        "Tuple" | "tuple" => matches!(val, Value::Tuple(_)),
        "Set" | "set" => matches!(val, Value::Set(_)),
        "Bytes" | "bytes" => matches!(val, Value::Bytes(_)),
        _ => false,
    }
}

/// Validate a value against a schema. The schema can be:
/// - A string type name like "Int", "String", etc.
/// - A map of field names to type name strings (for record/map validation)
fn validate_value_against_schema(
    val: &Value,
    schema: &Value,
    strings: &lumen_core::strings::StringTable,
) -> bool {
    match schema {
        Value::String(sref) => {
            let type_name = match sref {
                StringRef::Owned(s) => s.as_str(),
                StringRef::Interned(id) => strings.resolve(*id).unwrap_or(""),
            };
            validate_value_against_type_name(val, type_name)
        }
        Value::Map(schema_map) => {
            // Schema is a map: validate that val is a map with matching field types
            if let Value::Map(val_map) = val {
                for (key, expected_type) in schema_map.iter() {
                    match val_map.get(key) {
                        Some(field_val) => {
                            if !validate_value_against_schema(field_val, expected_type, strings) {
                                return false;
                            }
                        }
                        None => return false,
                    }
                }
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

// ── Glob matching helper ──

/// Convert Unix epoch seconds to (year, month, day, hour, minute, second).
/// Handles dates from 1970 onwards. Leap years are accounted for.
fn epoch_to_datetime(epoch_secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let secs_per_day: i64 = 86400;
    let mut days = epoch_secs / secs_per_day;
    let mut time_of_day = (epoch_secs % secs_per_day) as u32;
    if epoch_secs < 0 && time_of_day != 0 {
        days -= 1;
        time_of_day = (secs_per_day as u32) - (((-epoch_secs) % secs_per_day) as u32);
    }
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;

    // Days since 1970-01-01
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // month index [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    (y, m, d, hour, minute, second)
}

/// Walk a directory tree and collect paths matching a simple glob pattern.
/// Supports `*` (match any segment component) and `**` (match zero or more directories).
fn glob_walk(
    base: &std::path::Path,
    pattern: &str,
    result: &mut Vec<Value>,
) -> Result<(), VmError> {
    let segments: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    glob_walk_recursive(base, &segments, result)
}

fn glob_walk_recursive(
    dir: &std::path::Path,
    segments: &[&str],
    result: &mut Vec<Value>,
) -> Result<(), VmError> {
    if segments.is_empty() {
        return Ok(());
    }

    let seg = segments[0];
    let rest = &segments[1..];

    if seg == "**" {
        // Match zero directories (skip **)
        glob_walk_recursive(dir, rest, result)?;
        // Match one or more directories
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Try matching rest in this subdirectory
                glob_walk_recursive(&path, rest, result)?;
                // Continue recursing with ** still active
                glob_walk_recursive(&path, segments, result)?;
            } else if rest.is_empty() {
                result.push(Value::String(StringRef::Owned(
                    path.to_string_lossy().to_string(),
                )));
            }
        }
    } else {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if glob_match_segment(seg, &name) {
                if rest.is_empty() {
                    result.push(Value::String(StringRef::Owned(
                        path.to_string_lossy().to_string(),
                    )));
                } else if path.is_dir() {
                    glob_walk_recursive(&path, rest, result)?;
                }
            }
        }
    }
    Ok(())
}

/// Match a single glob segment against a filename. Supports `*` as a wildcard
/// that matches any sequence of characters, and `?` matching a single character.
fn glob_match_segment(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    glob_match_chars(&p, &n)
}

fn glob_match_chars(pattern: &[char], name: &[char]) -> bool {
    let mut pi = 0;
    let mut ni = 0;
    let mut star_pi = usize::MAX;
    let mut star_ni = 0;

    while ni < name.len() {
        if pi < pattern.len() && (pattern[pi] == '?' || pattern[pi] == name[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < pattern.len() && pattern[pi] == '*' {
            star_pi = pi;
            star_ni = ni;
            pi += 1;
        } else if star_pi != usize::MAX {
            pi = star_pi + 1;
            star_ni += 1;
            ni = star_ni;
        } else {
            return false;
        }
    }
    while pi < pattern.len() && pattern[pi] == '*' {
        pi += 1;
    }
    pi == pattern.len()
}

// ── CSV parsing helpers ──

/// Parse CSV text into a Value::List of Value::List of Value::String.
/// Handles quoted fields with escaped quotes ("").
fn csv_parse_text(text: &str) -> Value {
    let mut rows: Vec<Value> = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let fields = csv_parse_line(line);
        let row: Vec<Value> = fields
            .into_iter()
            .map(|f| Value::String(StringRef::Owned(f)))
            .collect();
        rows.push(Value::new_list(row));
    }
    Value::new_list(rows)
}

fn csv_parse_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    // Escaped quote
                    chars.next();
                    current.push('"');
                } else {
                    // End of quoted field
                    in_quotes = false;
                }
            } else {
                current.push(c);
            }
        } else {
            match c {
                ',' => {
                    fields.push(current.clone());
                    current.clear();
                }
                '"' => {
                    in_quotes = true;
                }
                _ => {
                    current.push(c);
                }
            }
        }
    }
    fields.push(current);
    fields
}

/// Encode a list of lists into a CSV string with proper quoting.
fn csv_encode_value(val: &Value) -> String {
    let mut output = String::new();
    if let Value::List(rows) = val {
        for row in rows.iter() {
            if let Value::List(fields) = row {
                let line: Vec<String> = fields
                    .iter()
                    .map(|f| {
                        let s = f.display_pretty();
                        if s.contains(',') || s.contains('"') || s.contains('\n') {
                            format!("\"{}\"", s.replace('"', "\"\""))
                        } else {
                            s
                        }
                    })
                    .collect();
                output.push_str(&line.join(","));
            }
            output.push('\n');
        }
    }
    output
}

// ── TOML parsing helpers ──

/// Parse a basic TOML string into a Value::Map.
/// Supports key=value pairs, [sections], nested tables,
/// quoted strings, integers, floats, booleans, and arrays.
fn toml_parse_text(text: &str) -> Value {
    let mut root: BTreeMap<String, Value> = BTreeMap::new();
    let mut current_section: Vec<String> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Section header
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let inner = trimmed[1..trimmed.len() - 1].trim();
            current_section = inner.split('.').map(|s| s.trim().to_string()).collect();
            // Ensure the section exists
            ensure_section(&mut root, &current_section);
            continue;
        }
        // Key = value
        if let Some(eq_pos) = trimmed.find('=') {
            let key = trimmed[..eq_pos].trim().to_string();
            let val_str = trimmed[eq_pos + 1..].trim();
            let value = toml_parse_value(val_str);

            if current_section.is_empty() {
                root.insert(key, value);
            } else {
                insert_at_section(&mut root, &current_section, &key, value);
            }
        }
    }

    Value::new_map(root)
}

fn ensure_section(root: &mut BTreeMap<String, Value>, path: &[String]) {
    let mut current = root;
    for seg in path {
        let entry = current
            .entry(seg.clone())
            .or_insert_with(|| Value::new_map(BTreeMap::new()));
        if let Value::Map(ref mut m) = entry {
            current = Arc::make_mut(m);
        } else {
            return;
        }
    }
}

fn insert_at_section(root: &mut BTreeMap<String, Value>, path: &[String], key: &str, value: Value) {
    let mut current = root;
    for seg in path {
        let entry = current
            .entry(seg.clone())
            .or_insert_with(|| Value::new_map(BTreeMap::new()));
        if let Value::Map(ref mut m) = entry {
            current = Arc::make_mut(m);
        } else {
            return;
        }
    }
    current.insert(key.to_string(), value);
}

fn toml_parse_value(s: &str) -> Value {
    let s = s.trim();

    // Boolean
    if s == "true" {
        return Value::Bool(true);
    }
    if s == "false" {
        return Value::Bool(false);
    }

    // Quoted string (double quotes)
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        let inner = &s[1..s.len() - 1];
        let unescaped = inner
            .replace("\\n", "\n")
            .replace("\\t", "\t")
            .replace("\\\\", "\\")
            .replace("\\\"", "\"");
        return Value::String(StringRef::Owned(unescaped));
    }

    // Single-quoted string (literal)
    if s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2 {
        return Value::String(StringRef::Owned(s[1..s.len() - 1].to_string()));
    }

    // Array
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        if inner.trim().is_empty() {
            return Value::new_list(vec![]);
        }
        let items: Vec<Value> = split_toml_array(inner)
            .iter()
            .map(|item| toml_parse_value(item.trim()))
            .collect();
        return Value::new_list(items);
    }

    // Integer
    if let Ok(n) = s.parse::<i64>() {
        return Value::Int(n);
    }

    // Float
    if let Ok(f) = s.parse::<f64>() {
        return Value::Float(f);
    }

    // Bare string fallback
    Value::String(StringRef::Owned(s.to_string()))
}

/// Split a TOML array string by commas, respecting nested brackets and quotes.
fn split_toml_array(s: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_quotes = false;
    let mut quote_char = '"';

    for c in s.chars() {
        if in_quotes {
            current.push(c);
            if c == quote_char {
                in_quotes = false;
            }
        } else {
            match c {
                '"' | '\'' => {
                    in_quotes = true;
                    quote_char = c;
                    current.push(c);
                }
                '[' => {
                    depth += 1;
                    current.push(c);
                }
                ']' => {
                    depth -= 1;
                    current.push(c);
                }
                ',' if depth == 0 => {
                    let trimmed = current.trim().to_string();
                    if !trimmed.is_empty() {
                        items.push(trimmed);
                    }
                    current.clear();
                }
                _ => {
                    current.push(c);
                }
            }
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        items.push(trimmed);
    }
    items
}

/// Encode a Value (typically a map) to TOML string format.
fn toml_encode_value(val: &Value, prefix: &str) -> String {
    let mut output = String::new();
    match val {
        Value::Map(m) => {
            // First pass: simple key-value pairs (non-map values)
            for (k, v) in m.iter() {
                if !matches!(v, Value::Map(_)) {
                    output.push_str(&format!("{} = {}\n", k, toml_format_scalar(v)));
                }
            }
            // Second pass: nested tables
            for (k, v) in m.iter() {
                if let Value::Map(_) = v {
                    let section = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{}.{}", prefix, k)
                    };
                    output.push_str(&format!("\n[{}]\n", section));
                    output.push_str(&toml_encode_value(v, &section));
                }
            }
        }
        _ => {
            output.push_str(&toml_format_scalar(val));
            output.push('\n');
        }
    }
    output
}

fn toml_format_scalar(val: &Value) -> String {
    match val {
        Value::String(StringRef::Owned(s)) => {
            format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
        }
        Value::String(StringRef::Interned(_)) => {
            format!(
                "\"{}\"",
                val.as_string().replace('\\', "\\\\").replace('"', "\\\"")
            )
        }
        Value::Int(n) => n.to_string(),
        Value::Float(f) => {
            let s = f.to_string();
            if s.contains('.') {
                s
            } else {
                format!("{}.0", s)
            }
        }
        Value::Bool(b) => b.to_string(),
        Value::Null => "\"\"".to_string(),
        Value::List(l) => {
            let items: Vec<String> = l.iter().map(toml_format_scalar).collect();
            format!("[{}]", items.join(", "))
        }
        _ => format!("\"{}\"", val.display_pretty().replace('"', "\\\"")),
    }
}

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

// ===========================================================================
// TCP/UDP networking builtins (backed by std::net)
// ===========================================================================

use once_cell::sync::Lazy;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::Mutex;

/// Enum that can hold a TCP stream or a UDP socket.
enum NetHandle {
    TcpStream(TcpStream),
    UdpSocket(UdpSocket),
}

/// Global registry of network handles, keyed by monotonic integer IDs.
struct HandleRegistry {
    handles: HashMap<i64, NetHandle>,
    next_id: i64,
}

impl HandleRegistry {
    fn new() -> Self {
        Self {
            handles: HashMap::new(),
            next_id: 1,
        }
    }

    fn insert(&mut self, handle: NetHandle) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.handles.insert(id, handle);
        id
    }

    fn remove(&mut self, id: i64) -> Option<NetHandle> {
        self.handles.remove(&id)
    }
}

static NET_HANDLES: Lazy<Mutex<HandleRegistry>> = Lazy::new(|| Mutex::new(HandleRegistry::new()));

fn net_tcp_connect(addr: &str) -> Value {
    match TcpStream::connect(addr) {
        Ok(stream) => {
            let id = NET_HANDLES
                .lock()
                .expect("NET_HANDLES lock poisoned")
                .insert(NetHandle::TcpStream(stream));
            let mut map = BTreeMap::new();
            map.insert("ok".to_string(), Value::Bool(true));
            map.insert("handle".to_string(), Value::Int(id));
            Value::new_map(map)
        }
        Err(e) => {
            let mut map = BTreeMap::new();
            map.insert("ok".to_string(), Value::Bool(false));
            map.insert(
                "error".to_string(),
                Value::String(StringRef::Owned(e.to_string())),
            );
            Value::new_map(map)
        }
    }
}

fn net_tcp_listen(addr: &str) -> Value {
    match TcpListener::bind(addr) {
        Ok(listener) => {
            // Accept one incoming connection (blocking).
            match listener.accept() {
                Ok((stream, peer_addr)) => {
                    let id = NET_HANDLES
                        .lock()
                        .expect("NET_HANDLES lock poisoned")
                        .insert(NetHandle::TcpStream(stream));
                    let mut map = BTreeMap::new();
                    map.insert("ok".to_string(), Value::Bool(true));
                    map.insert("handle".to_string(), Value::Int(id));
                    map.insert(
                        "peer".to_string(),
                        Value::String(StringRef::Owned(peer_addr.to_string())),
                    );
                    Value::new_map(map)
                }
                Err(e) => {
                    let mut map = BTreeMap::new();
                    map.insert("ok".to_string(), Value::Bool(false));
                    map.insert(
                        "error".to_string(),
                        Value::String(StringRef::Owned(format!("accept failed: {}", e))),
                    );
                    Value::new_map(map)
                }
            }
        }
        Err(e) => {
            let mut map = BTreeMap::new();
            map.insert("ok".to_string(), Value::Bool(false));
            map.insert(
                "error".to_string(),
                Value::String(StringRef::Owned(e.to_string())),
            );
            Value::new_map(map)
        }
    }
}

fn net_tcp_send(handle: i64, data: &str) -> Value {
    let mut registry = NET_HANDLES.lock().expect("NET_HANDLES lock poisoned");
    match registry.handles.get_mut(&handle) {
        Some(NetHandle::TcpStream(ref mut stream)) => match stream.write_all(data.as_bytes()) {
            Ok(()) => Value::Int(data.len() as i64),
            Err(e) => {
                let mut map = BTreeMap::new();
                map.insert("ok".to_string(), Value::Bool(false));
                map.insert(
                    "error".to_string(),
                    Value::String(StringRef::Owned(e.to_string())),
                );
                Value::new_map(map)
            }
        },
        _ => {
            let mut map = BTreeMap::new();
            map.insert("ok".to_string(), Value::Bool(false));
            map.insert(
                "error".to_string(),
                Value::String(StringRef::Owned(format!(
                    "invalid TCP stream handle: {}",
                    handle
                ))),
            );
            Value::new_map(map)
        }
    }
}

fn net_tcp_recv(handle: i64, max_bytes: i64) -> Value {
    let mut registry = NET_HANDLES.lock().expect("NET_HANDLES lock poisoned");
    match registry.handles.get_mut(&handle) {
        Some(NetHandle::TcpStream(ref mut stream)) => {
            let buf_size = max_bytes.clamp(1, 1_048_576) as usize;
            let mut buf = vec![0u8; buf_size];
            match stream.read(&mut buf) {
                Ok(n) => {
                    buf.truncate(n);
                    let data = String::from_utf8_lossy(&buf).to_string();
                    let mut map = BTreeMap::new();
                    map.insert("ok".to_string(), Value::Bool(true));
                    map.insert("data".to_string(), Value::String(StringRef::Owned(data)));
                    map.insert("bytes_read".to_string(), Value::Int(n as i64));
                    Value::new_map(map)
                }
                Err(e) => {
                    let mut map = BTreeMap::new();
                    map.insert("ok".to_string(), Value::Bool(false));
                    map.insert(
                        "error".to_string(),
                        Value::String(StringRef::Owned(e.to_string())),
                    );
                    Value::new_map(map)
                }
            }
        }
        _ => {
            let mut map = BTreeMap::new();
            map.insert("ok".to_string(), Value::Bool(false));
            map.insert(
                "error".to_string(),
                Value::String(StringRef::Owned(format!(
                    "invalid TCP stream handle: {}",
                    handle
                ))),
            );
            Value::new_map(map)
        }
    }
}

fn net_tcp_close(handle: i64) {
    let mut registry = NET_HANDLES.lock().expect("NET_HANDLES lock poisoned");
    // Dropping the handle closes the underlying socket.
    registry.remove(handle);
}

fn net_udp_bind(addr: &str) -> Value {
    match UdpSocket::bind(addr) {
        Ok(socket) => {
            let id = NET_HANDLES
                .lock()
                .expect("NET_HANDLES lock poisoned")
                .insert(NetHandle::UdpSocket(socket));
            let mut map = BTreeMap::new();
            map.insert("ok".to_string(), Value::Bool(true));
            map.insert("handle".to_string(), Value::Int(id));
            Value::new_map(map)
        }
        Err(e) => {
            let mut map = BTreeMap::new();
            map.insert("ok".to_string(), Value::Bool(false));
            map.insert(
                "error".to_string(),
                Value::String(StringRef::Owned(e.to_string())),
            );
            Value::new_map(map)
        }
    }
}

fn net_udp_send(handle: i64, addr: &str, data: &str) -> Value {
    let registry = NET_HANDLES.lock().expect("NET_HANDLES lock poisoned");
    match registry.handles.get(&handle) {
        Some(NetHandle::UdpSocket(ref socket)) => match socket.send_to(data.as_bytes(), addr) {
            Ok(n) => Value::Int(n as i64),
            Err(e) => {
                let mut map = BTreeMap::new();
                map.insert("ok".to_string(), Value::Bool(false));
                map.insert(
                    "error".to_string(),
                    Value::String(StringRef::Owned(e.to_string())),
                );
                Value::new_map(map)
            }
        },
        _ => {
            let mut map = BTreeMap::new();
            map.insert("ok".to_string(), Value::Bool(false));
            map.insert(
                "error".to_string(),
                Value::String(StringRef::Owned(format!(
                    "invalid UDP socket handle: {}",
                    handle
                ))),
            );
            Value::new_map(map)
        }
    }
}

fn net_udp_recv(handle: i64, max_bytes: i64) -> Value {
    let registry = NET_HANDLES.lock().expect("NET_HANDLES lock poisoned");
    match registry.handles.get(&handle) {
        Some(NetHandle::UdpSocket(ref socket)) => {
            let buf_size = max_bytes.clamp(1, 1_048_576) as usize;
            let mut buf = vec![0u8; buf_size];
            match socket.recv_from(&mut buf) {
                Ok((n, from_addr)) => {
                    buf.truncate(n);
                    let data = String::from_utf8_lossy(&buf).to_string();
                    let mut map = BTreeMap::new();
                    map.insert("ok".to_string(), Value::Bool(true));
                    map.insert("data".to_string(), Value::String(StringRef::Owned(data)));
                    map.insert(
                        "from".to_string(),
                        Value::String(StringRef::Owned(from_addr.to_string())),
                    );
                    map.insert("bytes_read".to_string(), Value::Int(n as i64));
                    Value::new_map(map)
                }
                Err(e) => {
                    let mut map = BTreeMap::new();
                    map.insert("ok".to_string(), Value::Bool(false));
                    map.insert(
                        "error".to_string(),
                        Value::String(StringRef::Owned(e.to_string())),
                    );
                    Value::new_map(map)
                }
            }
        }
        _ => {
            let mut map = BTreeMap::new();
            map.insert("ok".to_string(), Value::Bool(false));
            map.insert(
                "error".to_string(),
                Value::String(StringRef::Owned(format!(
                    "invalid UDP socket handle: {}",
                    handle
                ))),
            );
            Value::new_map(map)
        }
    }
}
