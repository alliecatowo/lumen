//! Arithmetic, diff, patch, and redact operations for the VM.

use super::*;
use std::collections::BTreeMap;

use crate::values::Value;
use crate::vm::VM;
use num_bigint::BigInt;
use num_traits::ToPrimitive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
    #[allow(dead_code)]
    Rem,
}

impl VM {
    /// Structural diff of two values.
    pub(crate) fn diff_values(&self, a: &Value, b: &Value) -> Value {
        if a == b {
            return Value::new_list(vec![]);
        }
        match (a, b) {
            (Value::Record(ra), Value::Record(rb)) if ra.type_name == rb.type_name => {
                let mut diffs = Vec::new();
                for (key, va) in &ra.fields {
                    match rb.fields.get(key) {
                        Some(vb) if va != vb => {
                            let mut change = BTreeMap::new();
                            change.insert(
                                "field".to_string(),
                                Value::String(StringRef::Owned(key.clone())),
                            );
                            change.insert("from".to_string(), va.clone());
                            change.insert("to".to_string(), vb.clone());
                            diffs.push(Value::new_map(change));
                        }
                        None => {
                            let mut change = BTreeMap::new();
                            change.insert(
                                "field".to_string(),
                                Value::String(StringRef::Owned(key.clone())),
                            );
                            change.insert("removed".to_string(), va.clone());
                            diffs.push(Value::new_map(change));
                        }
                        Some(_) => {}
                    }
                }
                for (key, vb) in &rb.fields {
                    if !ra.fields.contains_key(key) {
                        let mut change = BTreeMap::new();
                        change.insert(
                            "field".to_string(),
                            Value::String(StringRef::Owned(key.clone())),
                        );
                        change.insert("added".to_string(), vb.clone());
                        diffs.push(Value::new_map(change));
                    }
                }
                Value::new_list(diffs)
            }
            (Value::Map(ma), Value::Map(mb)) => {
                let mut diffs = Vec::new();
                for (key, va) in ma.iter() {
                    match mb.get(key) {
                        Some(vb) if va != vb => {
                            let mut change = BTreeMap::new();
                            change.insert(
                                "key".to_string(),
                                Value::String(StringRef::Owned(key.clone())),
                            );
                            change.insert("from".to_string(), va.clone());
                            change.insert("to".to_string(), vb.clone());
                            diffs.push(Value::new_map(change));
                        }
                        None => {
                            let mut change = BTreeMap::new();
                            change.insert(
                                "key".to_string(),
                                Value::String(StringRef::Owned(key.clone())),
                            );
                            change.insert("removed".to_string(), va.clone());
                            diffs.push(Value::new_map(change));
                        }
                        Some(_) => {}
                    }
                }
                for (key, vb) in mb.iter() {
                    if !ma.contains_key(key) {
                        let mut change = BTreeMap::new();
                        change.insert(
                            "key".to_string(),
                            Value::String(StringRef::Owned(key.clone())),
                        );
                        change.insert("added".to_string(), vb.clone());
                        diffs.push(Value::new_map(change));
                    }
                }
                Value::new_list(diffs)
            }
            _ => {
                let mut change = BTreeMap::new();
                change.insert("from".to_string(), a.clone());
                change.insert("to".to_string(), b.clone());
                Value::new_list(vec![Value::new_map(change)])
            }
        }
    }

    /// Apply patches to a value.
    pub(crate) fn patch_value(&self, val: &Value, patches: &Value) -> Value {
        match (val, patches) {
            (Value::Record(r), Value::List(patch_list)) => {
                let mut result: RecordValue = (**r).clone();
                for patch in patch_list.iter() {
                    if let Value::Map(m) = patch {
                        if let Some(Value::String(StringRef::Owned(field))) = m.get("field") {
                            if let Some(to) = m.get("to") {
                                result.fields.insert(field.clone(), to.clone());
                            } else if m.contains_key("removed") {
                                result.fields.remove(field);
                            } else if let Some(added) = m.get("added") {
                                result.fields.insert(field.clone(), added.clone());
                            }
                        }
                    }
                }
                Value::new_record(result)
            }
            (Value::Map(map), Value::List(patch_list)) => {
                let mut result: BTreeMap<String, Value> = (**map).clone();
                for patch in patch_list.iter() {
                    if let Value::Map(m) = patch {
                        if let Some(Value::String(StringRef::Owned(key))) = m.get("key") {
                            if let Some(to) = m.get("to") {
                                result.insert(key.clone(), to.clone());
                            } else if m.contains_key("removed") {
                                result.remove(key);
                            } else if let Some(added) = m.get("added") {
                                result.insert(key.clone(), added.clone());
                            }
                        }
                    }
                }
                Value::new_map(result)
            }
            _ => val.clone(),
        }
    }

    /// Redact specified fields from a value (set to null).
    pub(crate) fn redact_value(&self, val: &Value, field_list: &Value) -> Value {
        let fields_to_redact: Vec<String> = match field_list {
            Value::List(l) => l.iter().map(|v| v.as_string()).collect(),
            Value::String(StringRef::Owned(s)) => vec![s.clone()],
            _ => return val.clone(),
        };
        match val {
            Value::Record(r) => {
                let mut result: RecordValue = (**r).clone();
                for field in &fields_to_redact {
                    if result.fields.contains_key(field) {
                        result.fields.insert(field.clone(), Value::Null);
                    }
                }
                Value::new_record(result)
            }
            Value::Map(m) => {
                let mut result: BTreeMap<String, Value> = (**m).clone();
                for field in &fields_to_redact {
                    if result.contains_key(field) {
                        result.insert(field.clone(), Value::Null);
                    }
                }
                Value::new_map(result)
            }
            _ => val.clone(),
        }
    }

    pub(crate) fn arith_op(
        &mut self,
        base: usize,
        a: usize,
        b: usize,
        c: usize,
        op: BinaryOp,
    ) -> Result<(), VmError> {
        let lhs = self.registers[base + b].clone();
        let rhs = self.registers[base + c].clone();

        // Helper for checked int ops
        fn int_op(op: BinaryOp, x: i64, y: i64) -> Option<i64> {
            match op {
                BinaryOp::Add => x.checked_add(y),
                BinaryOp::Sub => x.checked_sub(y),
                BinaryOp::Mul => x.checked_mul(y),
                BinaryOp::Div => if y == 0 { None } else { Some(x / y) },
                BinaryOp::FloorDiv => if y == 0 { None } else { Some(x.div_euclid(y)) },
                BinaryOp::Mod => if y == 0 { None } else { Some(x.rem_euclid(y)) },
                BinaryOp::Rem => if y == 0 { None } else { Some(x % y) },
                BinaryOp::Pow => {
                    if y < 0 { None }
                    else if y > u32::MAX as i64 { None }
                    else { x.checked_pow(y as u32) }
                }
            }
        }

        // Helper for float ops that checks for overflow (infinite results)
        fn float_op_checked(op: BinaryOp, x: f64, y: f64) -> Option<f64> {
            let result = match op {
                BinaryOp::Add => x + y,
                BinaryOp::Sub => x - y,
                BinaryOp::Mul => x * y,
                BinaryOp::Div => x / y,
                BinaryOp::FloorDiv => (x / y).floor(),
                BinaryOp::Mod => x.rem_euclid(y),
                BinaryOp::Rem => x % y,
                BinaryOp::Pow => x.powf(y),
            };
            // Check for overflow - if result is infinite but inputs were finite, it's overflow
            if result.is_infinite() && x.is_finite() && y.is_finite() {
                None
            } else {
                Some(result)
            }
        }

        // Helper for BigInt ops
        fn bigint_op(op: BinaryOp, x: &BigInt, y: &BigInt) -> Result<BigInt, VmError> {
            match op {
                BinaryOp::Add => Ok(x + y),
                BinaryOp::Sub => Ok(x - y),
                BinaryOp::Mul => Ok(x * y),
                BinaryOp::Div => Ok(x / y),
                BinaryOp::FloorDiv => Ok(x / y),
                BinaryOp::Mod => Ok(x % y),
                BinaryOp::Rem => Ok(x % y),
                BinaryOp::Pow => {
                    if let Some(exp) = y.to_u32() {
                        Ok(x.pow(exp))
                    } else {
                        // Exponent too large - return error for out of range
                        Err(VmError::Runtime("exponent out of range".to_string()))
                    }
                }
            }
        }

        self.registers[base + a] = match (&lhs, &rhs) {
            (Value::Int(x), Value::Int(y)) => {
                if let Some(res) = int_op(op, *x, *y) {
                    Value::Int(res)
                } else {
                    // Integer overflow - return ArithmeticOverflow error
                    return Err(VmError::ArithmeticOverflow);
                }
            }
            (Value::BigInt(x), Value::BigInt(y)) => {
                Value::BigInt(bigint_op(op, x, y)?)
            }
            (Value::Int(x), Value::BigInt(y)) => {
                Value::BigInt(bigint_op(op, &BigInt::from(*x), y)?)
            }
            (Value::BigInt(x), Value::Int(y)) => {
                Value::BigInt(bigint_op(op, x, &BigInt::from(*y))?)
            }
            (Value::Float(x), Value::Float(y)) => {
                if let Some(res) = float_op_checked(op, *x, *y) {
                    Value::Float(res)
                } else {
                    return Err(VmError::ArithmeticOverflow);
                }
            }
            (Value::Int(x), Value::Float(y)) => {
                let xf = *x as f64;
                if let Some(res) = float_op_checked(op, xf, *y) {
                    Value::Float(res)
                } else {
                    return Err(VmError::ArithmeticOverflow);
                }
            }
            (Value::Float(x), Value::Int(y)) => {
                let yf = *y as f64;
                if let Some(res) = float_op_checked(op, *x, yf) {
                    Value::Float(res)
                } else {
                    return Err(VmError::ArithmeticOverflow);
                }
            }
            (Value::BigInt(x), Value::Float(y)) => {
                let xf = x.to_f64().unwrap_or(f64::NAN);
                if let Some(res) = float_op_checked(op, xf, *y) {
                    Value::Float(res)
                } else {
                    return Err(VmError::ArithmeticOverflow);
                }
            }
            (Value::Float(x), Value::BigInt(y)) => {
                let yf = y.to_f64().unwrap_or(f64::NAN);
                if let Some(res) = float_op_checked(op, *x, yf) {
                    Value::Float(res)
                } else {
                    return Err(VmError::ArithmeticOverflow);
                }
            }
            _ => {
                return Err(VmError::TypeError(format!(
                    "arithmetic on non-numeric types: {} ({}) and {} ({})",
                    lhs.display_pretty(),
                    lhs.type_name(),
                    rhs.display_pretty(),
                    rhs.type_name()
                )))
            }
        };
        Ok(())
    }
}
