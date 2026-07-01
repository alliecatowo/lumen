//! Arithmetic, diff, patch, and redact operations for the VM.

use super::*;
use std::collections::BTreeMap;

use crate::vm::VM;
use lumen_core::heap_value::HeapValue;
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

/// Checked integer arithmetic — returns None on overflow or division by zero.
#[inline(always)]
fn int_op(op: BinaryOp, x: i64, y: i64) -> Option<i64> {
    match op {
        BinaryOp::Add => x.checked_add(y),
        BinaryOp::Sub => x.checked_sub(y),
        BinaryOp::Mul => x.checked_mul(y),
        BinaryOp::Div => {
            if y == 0 {
                None
            } else {
                x.checked_div(y)
            }
        }
        BinaryOp::FloorDiv => {
            if y == 0 {
                None
            } else {
                Some(x.div_euclid(y))
            }
        }
        BinaryOp::Mod => {
            if y == 0 {
                None
            } else {
                Some(x.rem_euclid(y))
            }
        }
        BinaryOp::Rem => {
            if y == 0 {
                None
            } else {
                Some(x % y)
            }
        }
        BinaryOp::Pow => {
            if y < 0 || y > u32::MAX as i64 {
                None
            } else {
                x.checked_pow(y as u32)
            }
        }
    }
}

/// IEEE 754 float arithmetic — overflow produces infinity, not an error.
#[inline(always)]
fn float_op(op: BinaryOp, x: f64, y: f64) -> f64 {
    match op {
        BinaryOp::Add => x + y,
        BinaryOp::Sub => x - y,
        BinaryOp::Mul => x * y,
        BinaryOp::Div => x / y,
        BinaryOp::FloorDiv => (x / y).floor(),
        BinaryOp::Mod => x.rem_euclid(y),
        BinaryOp::Rem => x % y,
        BinaryOp::Pow => x.powf(y),
    }
}

/// Descriptive name for operation — only used in error messages.
#[cold]
#[inline(never)]
fn op_name(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "addition",
        BinaryOp::Sub => "subtraction",
        BinaryOp::Mul => "multiplication",
        BinaryOp::Div => "division",
        BinaryOp::FloorDiv => "floor division",
        BinaryOp::Mod => "modulo",
        BinaryOp::Rem => "remainder",
        BinaryOp::Pow => "exponentiation",
    }
}

/// BigInt arithmetic — only used on the cold/rare path.
#[cold]
#[inline(never)]
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
                Err(VmError::Runtime("exponent out of range".to_string()))
            }
        }
    }
}

/// Handle BigInt and mixed BigInt/Float/Int slow path.
/// Separated out so the compiler doesn't pollute the hot path's code layout.
#[cold]
#[inline(never)]
fn arith_op_slow(op: BinaryOp, lhs: NbValue, rhs: NbValue) -> Result<NbValue, VmError> {
    match (lhs.as_heap_ref(), rhs.as_heap_ref()) {
        (Some(HeapValue::BigInt(x)), Some(HeapValue::BigInt(y))) => {
            Ok(NbValue::new_bigint(bigint_op(op, x, y)?))
        }
        (Some(HeapValue::BigInt(x)), _) => match (rhs.as_int(), rhs.as_float()) {
            (Some(i), _) => Ok(NbValue::new_bigint(bigint_op(op, x, &BigInt::from(i))?)),
            (_, Some(f)) => Ok(NbValue::new_float(float_op(
                op,
                x.to_f64().unwrap_or(f64::NAN),
                f,
            ))),
            _ => Err(VmError::TypeError(format!(
                "arithmetic on non-numeric types: {} ({}) and {} ({})",
                lhs.display(),
                lhs.type_name(),
                rhs.display(),
                rhs.type_name()
            ))),
        },
        (_, Some(HeapValue::BigInt(y))) => match (lhs.as_int(), lhs.as_float()) {
            (Some(i), _) => Ok(NbValue::new_bigint(bigint_op(op, &BigInt::from(i), y)?)),
            (_, Some(f)) => Ok(NbValue::new_float(float_op(
                op,
                f,
                y.to_f64().unwrap_or(f64::NAN),
            ))),
            _ => Err(VmError::TypeError(format!(
                "arithmetic on non-numeric types: {} ({}) and {} ({})",
                lhs.display(),
                lhs.type_name(),
                rhs.display(),
                rhs.type_name()
            ))),
        },
        _ => Err(VmError::TypeError(format!(
            "arithmetic on non-numeric types: {} ({}) and {} ({})",
            lhs.display(),
            lhs.type_name(),
            rhs.display(),
            rhs.type_name()
        ))),
    }
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
    pub(crate) fn patch_value(&self, val: NbValue, patches: NbValue) -> NbValue {
        match (val.as_heap_ref(), patches.as_heap_ref()) {
            (Some(HeapValue::Record(r)), Some(HeapValue::List(patch_list))) => {
                let mut result = (**r).clone();
                for patch_nb in patch_list.iter() {
                    if let Some(HeapValue::Map(m)) = patch_nb.as_heap_ref() {
                        if let Some(field_nb) = m.get("field") {
                            let field = field_nb.display();
                            if let Some(to_nb) = m.get("to") {
                                result.fields.insert(field, *to_nb);
                            } else if m.contains_key("removed") {
                                result.fields.remove(&field);
                            } else if let Some(added) = m.get("added") {
                                result.fields.insert(field, *added);
                            }
                        }
                    }
                }
                NbValue::new_record(&result.type_name, result.fields)
            }
            (Some(HeapValue::Map(map)), Some(HeapValue::List(patch_list))) => {
                let mut result = (**map).clone();
                for patch_nb in patch_list.iter() {
                    if let Some(HeapValue::Map(m)) = patch_nb.as_heap_ref() {
                        if let Some(key_nb) = m.get("key") {
                            let key = key_nb.display();
                            if let Some(to_nb) = m.get("to") {
                                result.insert(key, *to_nb);
                            } else if m.contains_key("removed") {
                                result.remove(&key);
                            } else if let Some(added) = m.get("added") {
                                result.insert(key, *added);
                            }
                        }
                    }
                }
                NbValue::new_map(result)
            }
            _ => val,
        }
    }

    /// Redact specified fields from a value (set to null).
    pub(crate) fn redact_value(&self, val: NbValue, field_list: NbValue) -> NbValue {
        let fields: Vec<String> = match field_list.as_heap_ref() {
            Some(HeapValue::List(l)) => l.iter().map(|v| v.display()).collect(),
            Some(HeapValue::Str(s)) => vec![s.to_string()],
            _ => return val,
        };
        match val.as_heap_ref() {
            Some(HeapValue::Record(r)) => {
                let mut result = (**r).clone();
                for field in &fields {
                    if result.fields.contains_key(field.as_str()) {
                        result.fields.insert(field.clone(), NbValue::new_null());
                    }
                }
                NbValue::new_record(&result.type_name, result.fields)
            }
            Some(HeapValue::Map(m)) => {
                let mut result = (**m).clone();
                for field in &fields {
                    if result.contains_key(field.as_str()) {
                        result.insert(field.clone(), NbValue::new_null());
                    }
                }
                NbValue::new_map(result)
            }
            _ => val,
        }
    }

    /// Core arithmetic dispatch. Inlined into the main VM dispatch loop for performance.
    /// The Int-Int fast path is first and avoids any heap allocation or cloning.
    #[inline(always)]
    pub(crate) fn arith_op(
        &mut self,
        base: usize,
        a: usize,
        b: usize,
        c: usize,
        op: BinaryOp,
    ) -> Result<(), VmError> {
        // Fast path: borrow registers and extract Copy types (Int, Float) directly.
        // This avoids any cloning or heap allocation for the 99% case.
        // Get values as NbValue first, then convert patterns
        let lhs_val = self.registers[base + b];
        let rhs_val = self.registers[base + c];

        // HOT PATH: Int op Int — the vast majority of arithmetic in numeric code.
        if let (Some(x), Some(y)) = (lhs_val.as_int(), rhs_val.as_int()) {
            if let Some(res) = int_op(op, x, y) {
                self.set_reg_nb(base + a, NbValue::new_int(res));
                return Ok(());
            } else {
                return Err(VmError::ArithmeticOverflow(op_name(op).to_string()));
            }
        }

        // WARM PATH: Float op Float
        if let (Some(x), Some(y)) = (lhs_val.as_float(), rhs_val.as_float()) {
            self.set_reg_nb(base + a, NbValue::new_float(float_op(op, x, y)));
            return Ok(());
        }

        // WARM PATH: Mixed Int/Float promotion
        if let (Some(x), Some(y)) = (lhs_val.as_int(), rhs_val.as_float()) {
            self.set_reg_nb(base + a, NbValue::new_float(float_op(op, x as f64, y)));
            return Ok(());
        }
        if let (Some(x), Some(y)) = (lhs_val.as_float(), rhs_val.as_int()) {
            self.set_reg_nb(base + a, NbValue::new_float(float_op(op, x, y as f64)));
            return Ok(());
        }

        // String concatenation for Add
        if op == BinaryOp::Add {
            if let (Some(HeapValue::Str(l)), Some(HeapValue::Str(r))) =
                (lhs_val.as_heap_ref(), rhs_val.as_heap_ref())
            {
                let mut s = String::with_capacity(l.len() + r.len());
                s.push_str(l);
                s.push_str(r);
                self.set_reg_nb(base + a, NbValue::new_str(&s));
                return Ok(());
            }
        }

        // COLD PATH: BigInt and error cases — delegated to a separate non-inlined function
        // so the compiler doesn't bloat the hot path's instruction cache footprint.
        let result = arith_op_slow(op, lhs_val, rhs_val)?;
        self.set_reg_nb(base + a, result);
        Ok(())
    }
}
