// ---------------------------------------------------------------------------
// Collection runtime helpers for JIT (List and Map construction)
// ---------------------------------------------------------------------------

use crate::vm_context::VmContext;
use lumen_core::strings::StringTable;
use lumen_core::values::{RecordValue, StringRef, Value};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

// NbValue encoding constants (match lumen-core/src/values.rs NbValue exactly):
//   NAN_MASK  = 0x7FF8_0000_0000_0000
//   TAG_PTR=0, TAG_INT=1, TAG_ATOM=2, TAG_BOOL=3, TAG_NULL=4, TAG_FIBER=5
const NAN_MASK_U: u64 = 0x7FF8_0000_0000_0000;
const PAYLOAD_MASK_U: u64 = 0x0000_FFFF_FFFF_FFFF;
const NAN_BOX_NULL_U: u64 = 0x7FFC_0000_0000_0000;
const NAN_BOX_TRUE_U: u64 = NAN_MASK_U | (3u64 << 48) | 1;
const NAN_BOX_FALSE_U: u64 = NAN_MASK_U | (3u64 << 48);

/// Decode a NbValue-encoded i64 to a heap-allocated Value.
/// Returns a `*mut Value` that must be freed by the caller.
///
/// Encoding scheme matches NbValue in lumen-core:
///   - Non-NaN bits: raw f64 float
///   - TAG_PTR=0: heap pointer to a `Value` (or special: 0=Null, 1=NaN float)
///   - TAG_INT=1: 48-bit signed integer
///   - TAG_BOOL=3: boolean (payload 0=false, 1=true)
///   - TAG_NULL=4: null value
///
/// # Safety
/// If `val` is a TAG_PTR with payload > 1, it must be a valid `*const Value` pointer.
unsafe fn nanbox_to_value(val: i64) -> *mut Value {
    Box::into_raw(Box::new(nb_decode(val)))
}

/// Decode a NbValue-encoded i64 directly to a stack-allocated Value.
/// Unlike `nanbox_to_value`, this avoids heap allocation for inline types
/// (int, float, bool, null).
///
/// # Safety
/// If `val` is a TAG_PTR with payload > 1, it must be a valid `*const Value` pointer.
#[inline]
unsafe fn nb_decode(val: i64) -> Value {
    let u = val as u64;
    if (u & NAN_MASK_U) != NAN_MASK_U {
        // Not NaN-boxed: raw f64 float bits.
        return Value::Float(f64::from_bits(u));
    }
    // NbValue uses 3-bit tags at bits 48-50 (bit 51 is the quiet-NaN bit, not part of tag).
    // Must use 0x7 mask (not 0xF) to extract the correct 3-bit tag value.
    let tag = (u >> 48) & 0x7;
    let payload = u & PAYLOAD_MASK_U;
    match tag {
        0 => {
            // TAG_PTR: heap pointer or special sentinel.
            if payload == 0 {
                Value::Null
            } else if payload == 1 {
                Value::Float(f64::NAN)
            } else {
                (*(payload as *const Value)).clone()
            }
        }
        1 => {
            // TAG_INT: 48-bit two's-complement integer.
            let signed = if payload & (1 << 47) != 0 {
                (payload | !PAYLOAD_MASK_U) as i64
            } else {
                payload as i64
            };
            Value::Int(signed)
        }
        3 => {
            // TAG_BOOL: payload 0=false, 1=true.
            Value::Bool(payload != 0)
        }
        4 => {
            // TAG_NULL
            Value::Null
        }
        _ => {
            // Unknown tag — treat as null.
            Value::Null
        }
    }
}

#[inline]
fn wrap_nanbox_ptr(ptr: *const Value) -> i64 {
    let raw = ptr as u64;
    (NAN_MASK_U | (raw & PAYLOAD_MASK_U)) as i64
}

#[inline]
fn wrap_nanbox_value(value: Value) -> i64 {
    wrap_nanbox_ptr(Arc::into_raw(Arc::new(value)))
}

/// Public wrapper for jit.rs to decode NbValue i64 → Value without an Arc leak.
///
/// # Safety
/// `val` must be a valid NaN-boxed i64 produced by the JIT/VM.
#[inline]
pub unsafe fn nb_decode_pub(val: i64) -> Value {
    nb_decode(val)
}

/// Public wrapper for jit.rs to encode a Value as a NaN-boxed i64 (Arc-backed).
#[inline]
pub fn wrap_nanbox_value_pub(value: Value) -> i64 {
    wrap_nanbox_value(value)
}

#[inline]
fn singleton_list_value(element: Value) -> i64 {
    wrap_nanbox_value(Value::new_list(vec![element]))
}

#[inline]
fn value_is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        Value::Float(f) => *f != 0.0,
        Value::String(StringRef::Owned(s)) => !s.is_empty(),
        Value::String(StringRef::Interned(_)) => true,
        Value::List(l) => !l.is_empty(),
        Value::Tuple(t) => !t.is_empty(),
        Value::Set(s) => !s.is_empty(),
        Value::Map(m) => !m.is_empty(),
        _ => true,
    }
}

#[inline]
fn value_to_nanbox(value: &Value) -> i64 {
    match value {
        Value::Int(n) => {
            let payload = (*n as u64) & PAYLOAD_MASK_U;
            (NAN_MASK_U | (1u64 << 48) | payload) as i64
        }
        Value::Bool(true) => NAN_BOX_TRUE_U as i64,
        Value::Bool(false) => NAN_BOX_FALSE_U as i64,
        Value::Null => NAN_BOX_NULL_U as i64,
        Value::Float(f) => f.to_bits() as i64,
        other => wrap_nanbox_value(other.clone()),
    }
}

#[inline]
fn nb_to_int(value_nb: i64) -> Option<i64> {
    match unsafe { nb_decode(value_nb) } {
        Value::Int(i) => Some(i),
        _ => None,
    }
}

#[inline]
fn cmp_value_natural(lhs: &Value, rhs: &Value) -> Ordering {
    match (lhs, rhs) {
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        (Value::Float(a), Value::Float(b)) => a.total_cmp(b),
        (Value::Int(a), Value::Float(b)) => (*a as f64).total_cmp(b),
        (Value::Float(a), Value::Int(b)) => a.total_cmp(&(*b as f64)),
        (Value::String(a), Value::String(b)) => match (a, b) {
            (
                lumen_core::values::StringRef::Owned(lhs),
                lumen_core::values::StringRef::Owned(rhs),
            ) => lhs.cmp(rhs),
            (
                lumen_core::values::StringRef::Interned(lhs),
                lumen_core::values::StringRef::Interned(rhs),
            ) => lhs.cmp(rhs),
            (
                lumen_core::values::StringRef::Interned(_),
                lumen_core::values::StringRef::Owned(_),
            ) => Ordering::Less,
            (
                lumen_core::values::StringRef::Owned(_),
                lumen_core::values::StringRef::Interned(_),
            ) => Ordering::Greater,
        },
        (Value::String(_), other) => lhs.as_string().cmp(&other.as_string()),
        (other, Value::String(_)) => other.as_string().cmp(&rhs.as_string()),
        _ => lhs.cmp(rhs),
    }
}

#[inline]
fn decode_value_ptr(list_ptr: i64) -> Option<*const Value> {
    let u = list_ptr as u64;
    if u == 0 || u == NAN_BOX_NULL_U {
        return None;
    }

    // NaN-boxed TAG_PTR.
    if (u & NAN_MASK_U) == NAN_MASK_U && ((u >> 48) & 0x7) == 0 {
        let payload = u & PAYLOAD_MASK_U;
        return if payload > 1 {
            Some(payload as *const Value)
        } else {
            None
        };
    }

    // Legacy/raw pointer form used by some helper paths.
    if u > 1 && u < (1u64 << 48) {
        return Some(u as *const Value);
    }

    None
}

/// Call a Lumen closure from JIT helper code.
/// `closure_nb` is the NaN-boxed closure value.
/// `args_ptr` points to an array of NaN-boxed i64 arguments.
/// `arg_count` is the number of arguments.
/// Returns a NaN-boxed i64 result.
pub extern "C" fn jit_rt_call_closure(
    ctx: *mut VmContext,
    closure_nb: i64,
    args_ptr: *const i64,
    arg_count: i64,
) -> i64 {
    if !ctx.is_null() {
        let trampoline = unsafe { (*ctx).call_closure };
        if let Some(call) = trampoline {
            return call(ctx, closure_nb, args_ptr, arg_count);
        }
    }
    NAN_BOX_NULL_U as i64
}

// Call a unary closure (one arg) via ctx->call_closure trampoline.
// The VM's call_closure already handles Option B: JIT fn_ptr if compiled, else interpreter.
#[inline]
fn call_hof_unary(ctx: *mut VmContext, closure_nb: i64, arg: &Value) -> Value {
    let arg_nb = value_to_nanbox(arg);
    let args = [arg_nb];
    let result_nb = jit_rt_call_closure(ctx, closure_nb, args.as_ptr(), 1);
    unsafe { nb_decode(result_nb) }
}

// Call a binary closure (two args) via ctx->call_closure trampoline.
#[inline]
fn call_hof_binary(ctx: *mut VmContext, closure_nb: i64, left: &Value, right: &Value) -> Value {
    let left_nb = value_to_nanbox(left);
    let right_nb = value_to_nanbox(right);
    let args = [left_nb, right_nb];
    let result_nb = jit_rt_call_closure(ctx, closure_nb, args.as_ptr(), 2);
    unsafe { nb_decode(result_nb) }
}

#[inline]
fn append_to_list_value(list_ptr: i64, element_value: Value) -> i64 {
    let Some(raw_ptr) = decode_value_ptr(list_ptr) else {
        return singleton_list_value(element_value);
    };

    // Take ownership via Arc — the caller overwrites the source register.
    let mut arc_list = unsafe { Arc::from_raw(raw_ptr) };
    if let Value::List(inner_arc) = Arc::make_mut(&mut arc_list) {
        Arc::make_mut(inner_arc).push(element_value);
    }
    wrap_nanbox_ptr(Arc::into_raw(arc_list))
}

#[inline]
fn resolve_string(ctx: *mut VmContext, value: &Value) -> String {
    let table: Option<&StringTable> = if ctx.is_null() {
        None
    } else {
        let table_ptr = unsafe { (*ctx).string_table };
        if table_ptr.is_null() {
            None
        } else {
            Some(unsafe { &*table_ptr })
        }
    };

    match table {
        Some(table) => value.as_string_resolved(table),
        None => value.as_string(),
    }
}

/// Create a new List value from an array of boxed Values.
/// `values_ptr` points to an array of `i64` representing NaN-boxed values or pointers.
/// `count` is the number of elements in the array.
/// Returns a new `*mut Value` containing the List.
///
/// # Safety
/// `values_ptr` must point to a valid array of `count` i64 values.
#[no_mangle]
pub extern "C" fn jit_rt_new_list(_ctx: *mut VmContext, values_ptr: *const i64, count: i64) -> i64 {
    let count = count as usize;
    let mut list = Vec::with_capacity(count);

    for i in 0..count {
        let val_i64 = unsafe { *values_ptr.add(i) };
        let value = unsafe { nb_decode(val_i64) };
        list.push(value);
    }

    let list_value = Value::new_list(list);
    let ptr = Arc::into_raw(Arc::new(list_value)) as u64;
    (NAN_MASK_U | (ptr & PAYLOAD_MASK_U)) as i64
}

/// Create a new Map value from an array of key-value pairs.
/// `kvpairs_ptr` points to an array of `i64` values alternating between:
///   - key: NaN-boxed value or pointer that will be converted to string
///   - value: NaN-boxed value or pointer
/// `count` is the number of key-value PAIRS (so the array has `count * 2` elements).
/// Returns a new `*mut Value` containing the Map.
///
/// # Safety
/// `kvpairs_ptr` must point to a valid array of `count * 2` i64 values.
#[no_mangle]
pub extern "C" fn jit_rt_new_map(_ctx: *mut VmContext, kvpairs_ptr: *const i64, count: i64) -> i64 {
    let count = count as usize;
    let mut map = BTreeMap::new();

    for i in 0..count {
        // Key is at index i*2, value is at index i*2+1
        let key_i64 = unsafe { *kvpairs_ptr.add(i * 2) };
        let value_i64 = unsafe { *kvpairs_ptr.add(i * 2 + 1) };

        let key_value = unsafe { nb_decode(key_i64) };
        let key_str = key_value.as_string();
        let value = unsafe { nb_decode(value_i64) };

        map.insert(key_str, value);
    }

    let map_value = Value::new_map(map);
    let ptr = Arc::into_raw(Arc::new(map_value)) as u64;
    (NAN_MASK_U | (ptr & PAYLOAD_MASK_U)) as i64
}

/// Create a new Tuple value from an array of boxed Values.
/// `values_ptr` points to an array of `i64` representing NaN-boxed values or pointers.
/// `count` is the number of elements in the tuple.
/// Returns a new `*mut Value` containing the Tuple.
///
/// # Safety
/// `values_ptr` must point to a valid array of `count` i64 values.
#[no_mangle]
pub extern "C" fn jit_rt_new_tuple(
    _ctx: *mut VmContext,
    values_ptr: *const i64,
    count: i64,
) -> i64 {
    let count = count as usize;
    let mut elements = Vec::with_capacity(count);

    for i in 0..count {
        let val_i64 = unsafe { *values_ptr.add(i) };
        let value = unsafe { nb_decode(val_i64) };
        elements.push(value);
    }

    let tuple_value = Value::new_tuple(elements);
    let ptr = Arc::into_raw(Arc::new(tuple_value)) as u64;
    (NAN_MASK_U | (ptr & PAYLOAD_MASK_U)) as i64
}

/// Get the length of a collection (List, Map, Set, Tuple, or String).
/// Returns the count as i64, or 0 if the value is not a collection.
///
/// # Safety
/// `value_ptr` must be a valid `*const Value` pointer.
#[no_mangle]
pub extern "C" fn jit_rt_collection_len(_ctx: *mut VmContext, value_ptr: i64) -> i64 {
    let u = value_ptr as u64;
    // Must be a TAG_PTR (NAN_MASK set, tag bits == 0) with non-special payload.
    if (u & NAN_MASK_U) != NAN_MASK_U || ((u >> 48) & 0x7) != 0 || (u & PAYLOAD_MASK_U) <= 1 {
        return 0;
    }
    let ptr = (u & PAYLOAD_MASK_U) as *const Value;
    let value = unsafe { &*ptr };
    match value {
        Value::List(l) => l.len() as i64,
        Value::Map(m) => m.len() as i64,
        Value::Set(s) => s.len() as i64,
        Value::Tuple(t) => t.len() as i64,
        Value::String(lumen_core::values::StringRef::Owned(s)) => s.chars().count() as i64,
        Value::String(lumen_core::values::StringRef::Interned(_)) => {
            // For interned strings, we'd need the StringTable to resolve
            // For now, return 0 (this is a limitation)
            0
        }
        _ => 0,
    }
}

/// Create a new Set value from an array of boxed Values.
/// `values_ptr` points to an array of `i64` representing NaN-boxed values or pointers.
/// `count` is the number of elements in the set.
/// Returns a new `*mut Value` containing the Set (with duplicates removed).
///
/// # Safety
/// `values_ptr` must point to a valid array of `count` i64 values.
#[no_mangle]
pub extern "C" fn jit_rt_new_set(_ctx: *mut VmContext, values_ptr: *const i64, count: i64) -> i64 {
    let count = count as usize;
    let mut set = BTreeSet::new();

    for i in 0..count {
        let val_i64 = unsafe { *values_ptr.add(i) };
        let value = unsafe { nb_decode(val_i64) };
        set.insert(value);
    }

    let set_value = Value::Set(Arc::new(set));
    let ptr = Arc::into_raw(Arc::new(set_value)) as u64;
    (NAN_MASK_U | (ptr & PAYLOAD_MASK_U)) as i64
}

/// Create a new Record value with the given type name and an empty field map.
///
/// The JIT lowers `NewRecord` by calling this helper to create the record shell.
/// Fields are then populated by `SetField` / `jit_rt_record_set_field` calls.
///
/// # Parameters
/// - `_ctx`: VM context (unused, reserved for future use)
/// - `type_name_ptr`: Pointer to the type name byte string
/// - `type_name_len`: Length of the type name in bytes
///
/// # Returns
/// Raw `*mut Value` (heap-boxed `Value::Record`) cast to i64.
///
/// # Safety
/// `type_name_ptr` must point to a valid UTF-8 byte sequence of length `type_name_len`.
#[no_mangle]
pub extern "C" fn jit_rt_new_record(
    _ctx: *mut VmContext,
    type_name_ptr: *const u8,
    type_name_len: i64,
) -> i64 {
    let type_name = if type_name_ptr.is_null() || type_name_len == 0 {
        "Unknown".to_string()
    } else {
        let bytes = unsafe { std::slice::from_raw_parts(type_name_ptr, type_name_len as usize) };
        std::str::from_utf8(bytes).unwrap_or("Unknown").to_string()
    };

    let record_value = Value::new_record(RecordValue {
        type_name,
        fields: BTreeMap::new(),
    });
    let ptr = Arc::into_raw(Arc::new(record_value)) as u64;
    (NAN_MASK_U | (ptr & PAYLOAD_MASK_U)) as i64
}

/// Apply a closure to each element in a list, returning a new list.
#[no_mangle]
pub extern "C" fn jit_rt_hof_map(ctx: *mut VmContext, list_nb: i64, closure_nb: i64) -> i64 {
    let list_val = unsafe { nb_decode(list_nb) };
    let items = match list_val {
        Value::List(list) => list,
        _ => return wrap_nanbox_value(Value::new_list(Vec::new())),
    };
    let mut out = Vec::with_capacity(items.len());
    for elem in items.iter() {
        out.push(call_hof_unary(ctx, closure_nb, elem));
    }
    wrap_nanbox_value(Value::new_list(out))
}

/// Filter list elements using a predicate closure.
#[no_mangle]
pub extern "C" fn jit_rt_hof_filter(ctx: *mut VmContext, list_nb: i64, closure_nb: i64) -> i64 {
    let list_val = unsafe { nb_decode(list_nb) };
    let items = match list_val {
        Value::List(list) => list,
        _ => return wrap_nanbox_value(Value::new_list(Vec::new())),
    };
    let mut out = Vec::new();
    for elem in items.iter() {
        let keep = call_hof_unary(ctx, closure_nb, elem);
        if value_is_truthy(&keep) {
            out.push(elem.clone());
        }
    }
    wrap_nanbox_value(Value::new_list(out))
}

/// Reduce a list using a binary closure and initial accumulator.
#[no_mangle]
pub extern "C" fn jit_rt_hof_reduce(
    ctx: *mut VmContext,
    list_nb: i64,
    closure_nb: i64,
    init_nb: i64,
) -> i64 {
    let list_val = unsafe { nb_decode(list_nb) };
    let items = match list_val {
        Value::List(list) => list,
        _ => return init_nb,
    };
    let mut acc = unsafe { nb_decode(init_nb) };
    for elem in items.iter() {
        acc = call_hof_binary(ctx, closure_nb, &acc, elem);
    }
    value_to_nanbox(&acc)
}

/// Map then flatten one level.
#[no_mangle]
pub extern "C" fn jit_rt_hof_flat_map(ctx: *mut VmContext, list_nb: i64, closure_nb: i64) -> i64 {
    let list_val = unsafe { nb_decode(list_nb) };
    let items = match list_val {
        Value::List(list) => list,
        _ => return wrap_nanbox_value(Value::new_list(Vec::new())),
    };
    let mut out = Vec::new();
    for elem in items.iter() {
        let mapped = call_hof_unary(ctx, closure_nb, elem);
        match mapped {
            Value::List(inner) => out.extend(inner.iter().cloned()),
            other => out.push(other),
        }
    }
    wrap_nanbox_value(Value::new_list(out))
}

/// True if any element matches predicate.
#[no_mangle]
pub extern "C" fn jit_rt_hof_any(ctx: *mut VmContext, list_nb: i64, closure_nb: i64) -> i64 {
    let list_val = unsafe { nb_decode(list_nb) };
    let items = match list_val {
        Value::List(list) => list,
        _ => return NAN_BOX_FALSE_U as i64,
    };
    for elem in items.iter() {
        let pred = call_hof_unary(ctx, closure_nb, elem);
        if value_is_truthy(&pred) {
            return NAN_BOX_TRUE_U as i64;
        }
    }
    NAN_BOX_FALSE_U as i64
}

/// True if all elements match predicate.
#[no_mangle]
pub extern "C" fn jit_rt_hof_all(ctx: *mut VmContext, list_nb: i64, closure_nb: i64) -> i64 {
    let list_val = unsafe { nb_decode(list_nb) };
    let items = match list_val {
        Value::List(list) => list,
        _ => return NAN_BOX_FALSE_U as i64,
    };
    for elem in items.iter() {
        let pred = call_hof_unary(ctx, closure_nb, elem);
        if !value_is_truthy(&pred) {
            return NAN_BOX_FALSE_U as i64;
        }
    }
    NAN_BOX_TRUE_U as i64
}

/// Return the first element that matches predicate or null.
#[no_mangle]
pub extern "C" fn jit_rt_hof_find(ctx: *mut VmContext, list_nb: i64, closure_nb: i64) -> i64 {
    let list_val = unsafe { nb_decode(list_nb) };
    let items = match list_val {
        Value::List(list) => list,
        _ => return NAN_BOX_NULL_U as i64,
    };
    for elem in items.iter() {
        let pred = call_hof_unary(ctx, closure_nb, elem);
        if value_is_truthy(&pred) {
            return value_to_nanbox(elem);
        }
    }
    NAN_BOX_NULL_U as i64
}

/// Return index of first match, or -1.
#[no_mangle]
pub extern "C" fn jit_rt_hof_position(ctx: *mut VmContext, list_nb: i64, closure_nb: i64) -> i64 {
    let list_val = unsafe { nb_decode(list_nb) };
    let items = match list_val {
        Value::List(list) => list,
        _ => return value_to_nanbox(&Value::Int(-1)),
    };
    for (idx, elem) in items.iter().enumerate() {
        let pred = call_hof_unary(ctx, closure_nb, elem);
        if value_is_truthy(&pred) {
            return value_to_nanbox(&Value::Int(idx as i64));
        }
    }
    value_to_nanbox(&Value::Int(-1))
}

/// Group elements by key function; keys are stringified.
#[no_mangle]
pub extern "C" fn jit_rt_hof_group_by(ctx: *mut VmContext, list_nb: i64, closure_nb: i64) -> i64 {
    let list_val = unsafe { nb_decode(list_nb) };
    let items = match list_val {
        Value::List(list) => list,
        _ => return wrap_nanbox_value(Value::new_map(BTreeMap::new())),
    };
    let mut groups: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for elem in items.iter() {
        let key_value = call_hof_unary(ctx, closure_nb, elem);
        let key = resolve_string(ctx, &key_value);
        groups.entry(key).or_default().push(elem.clone());
    }
    let mut map = BTreeMap::new();
    for (key, values) in groups {
        map.insert(key, Value::new_list(values));
    }
    wrap_nanbox_value(Value::new_map(map))
}

/// Sort a list using comparator closure returning Int.
#[no_mangle]
pub extern "C" fn jit_rt_hof_sort_by(ctx: *mut VmContext, list_nb: i64, closure_nb: i64) -> i64 {
    let list_val = unsafe { nb_decode(list_nb) };
    let items = match list_val {
        Value::List(list) => list,
        _ => return wrap_nanbox_value(Value::new_list(Vec::new())),
    };
    let mut values: Vec<Value> = items.iter().cloned().collect();
    values.sort_by(|lhs, rhs| {
        let cmp_val = call_hof_binary(ctx, closure_nb, lhs, rhs);
        match cmp_val {
            Value::Int(n) => n.cmp(&0),
            Value::Float(f) => f.total_cmp(&0.0),
            Value::Bool(true) => Ordering::Greater,
            Value::Bool(false) => Ordering::Less,
            _ => Ordering::Equal,
        }
    });
    wrap_nanbox_value(Value::new_list(values))
}

/// Append a value to a List.
/// `list_ptr` is a raw `*mut Value` pointer to a Value::List (JIT Ptr encoding,
/// same convention as jit_rt_set_index / jit_rt_new_list).
/// `element` is a NaN-boxed i64 representing the element to append.
/// Takes ownership of `list_ptr` (the JIT register is overwritten after this call).
/// Returns a raw `*mut Value` as i64 with the element appended.
///
/// # Safety
/// `list_ptr` must be a valid `*mut Value` produced by a JIT collection helper,
/// or 0 / NAN_BOX_NULL for empty list fallback.
#[no_mangle]
pub extern "C" fn jit_rt_list_append(_ctx: *mut VmContext, list_ptr: i64, element: i64) -> i64 {
    append_to_list_value(list_ptr, unsafe { nb_decode(element) })
}

/// Append a raw i64 integer value to a List.
/// `element` is an unboxed integer (not NbValue-encoded).
#[no_mangle]
pub extern "C" fn jit_rt_list_append_int(_ctx: *mut VmContext, list_ptr: i64, element: i64) -> i64 {
    append_to_list_value(list_ptr, Value::Int(element))
}

/// Append a raw f64-bit-pattern value to a List.
/// `element_bits` is interpreted as IEEE-754 `f64` bits.
#[no_mangle]
pub extern "C" fn jit_rt_list_append_float(
    _ctx: *mut VmContext,
    list_ptr: i64,
    element_bits: i64,
) -> i64 {
    append_to_list_value(list_ptr, Value::Float(f64::from_bits(element_bits as u64)))
}

/// Create a list of integers for a range [start, end).
#[no_mangle]
pub extern "C" fn jit_rt_range(_ctx: *mut VmContext, start_nb: i64, end_nb: i64) -> i64 {
    let start = nb_to_int(start_nb).unwrap_or(0);
    let end = nb_to_int(end_nb).unwrap_or(0);
    if end <= start {
        return wrap_nanbox_value(Value::new_list(Vec::new()));
    }
    let mut list = Vec::with_capacity((end - start) as usize);
    for value in start..end {
        list.push(Value::Int(value));
    }
    wrap_nanbox_value(Value::new_list(list))
}

/// Sort a list in natural ascending order.
#[no_mangle]
pub extern "C" fn jit_rt_sort(_ctx: *mut VmContext, list_nb: i64) -> i64 {
    let Some(raw_ptr) = decode_value_ptr(list_nb) else {
        return list_nb;
    };

    let mut arc_value = unsafe { Arc::from_raw(raw_ptr) };
    if let Value::List(list) = Arc::make_mut(&mut arc_value) {
        Arc::make_mut(list).sort_by(cmp_value_natural);
    }

    wrap_nanbox_ptr(Arc::into_raw(arc_value))
}

/// Sort a list in natural descending order.
#[no_mangle]
pub extern "C" fn jit_rt_sort_desc(_ctx: *mut VmContext, list_nb: i64) -> i64 {
    let Some(raw_ptr) = decode_value_ptr(list_nb) else {
        return list_nb;
    };

    let mut arc_value = unsafe { Arc::from_raw(raw_ptr) };
    if let Value::List(list) = Arc::make_mut(&mut arc_value) {
        let list = Arc::make_mut(list);
        list.sort_by(cmp_value_natural);
        list.reverse();
    }

    wrap_nanbox_ptr(Arc::into_raw(arc_value))
}

/// Reverse a List in-place (copy-on-write) and return the list.
#[no_mangle]
pub extern "C" fn jit_rt_list_reverse(_ctx: *mut VmContext, list_nb: i64) -> i64 {
    let Some(raw_ptr) = decode_value_ptr(list_nb) else {
        return list_nb;
    };

    let mut arc_value = unsafe { Arc::from_raw(raw_ptr) };
    if let Value::List(list) = Arc::make_mut(&mut arc_value) {
        Arc::make_mut(list).reverse();
    }

    wrap_nanbox_ptr(Arc::into_raw(arc_value))
}

/// Flatten a List by one level.
#[no_mangle]
pub extern "C" fn jit_rt_list_flatten(_ctx: *mut VmContext, list_nb: i64) -> i64 {
    let Some(raw_ptr) = decode_value_ptr(list_nb) else {
        return list_nb;
    };

    let arc_value = unsafe { Arc::from_raw(raw_ptr) };
    let Value::List(list) = arc_value.as_ref() else {
        return wrap_nanbox_ptr(Arc::into_raw(arc_value));
    };

    let mut flat = Vec::new();
    for item in list.iter() {
        if let Value::List(inner) = item {
            flat.extend(inner.iter().cloned());
        } else {
            flat.push(item.clone());
        }
    }

    wrap_nanbox_value(Value::new_list(flat))
}

/// Remove duplicate elements from a List (first-occurrence order).
#[no_mangle]
pub extern "C" fn jit_rt_list_unique(_ctx: *mut VmContext, list_nb: i64) -> i64 {
    let Some(raw_ptr) = decode_value_ptr(list_nb) else {
        return list_nb;
    };

    let arc_value = unsafe { Arc::from_raw(raw_ptr) };
    let Value::List(list) = arc_value.as_ref() else {
        return wrap_nanbox_ptr(Arc::into_raw(arc_value));
    };

    let mut seen: Vec<Value> = Vec::new();
    for item in list.iter() {
        if !seen.contains(item) {
            seen.push(item.clone());
        }
    }

    wrap_nanbox_value(Value::new_list(seen))
}

/// Take the first N elements from a List.
#[no_mangle]
pub extern "C" fn jit_rt_list_take(_ctx: *mut VmContext, list_nb: i64, n_nb: i64) -> i64 {
    let n = nb_to_int(n_nb).unwrap_or(0) as usize;
    let Some(raw_ptr) = decode_value_ptr(list_nb) else {
        return list_nb;
    };

    let arc_value = unsafe { Arc::from_raw(raw_ptr) };
    let Value::List(list) = arc_value.as_ref() else {
        return wrap_nanbox_ptr(Arc::into_raw(arc_value));
    };

    let taken: Vec<Value> = list.iter().take(n).cloned().collect();
    wrap_nanbox_value(Value::new_list(taken))
}

/// Drop the first N elements from a List.
#[no_mangle]
pub extern "C" fn jit_rt_list_drop(_ctx: *mut VmContext, list_nb: i64, n_nb: i64) -> i64 {
    let n = nb_to_int(n_nb).unwrap_or(0) as usize;
    let Some(raw_ptr) = decode_value_ptr(list_nb) else {
        return list_nb;
    };

    let arc_value = unsafe { Arc::from_raw(raw_ptr) };
    let Value::List(list) = arc_value.as_ref() else {
        return wrap_nanbox_ptr(Arc::into_raw(arc_value));
    };

    let dropped: Vec<Value> = list.iter().skip(n).cloned().collect();
    wrap_nanbox_value(Value::new_list(dropped))
}

/// Return the first element of a List or null.
#[no_mangle]
pub extern "C" fn jit_rt_list_first(_ctx: *mut VmContext, list_nb: i64) -> i64 {
    let Some(raw_ptr) = decode_value_ptr(list_nb) else {
        return NAN_BOX_NULL_U as i64;
    };

    let arc_value = unsafe { Arc::from_raw(raw_ptr) };
    let Value::List(list) = arc_value.as_ref() else {
        return NAN_BOX_NULL_U as i64;
    };

    list.first()
        .map(value_to_nanbox)
        .unwrap_or(NAN_BOX_NULL_U as i64)
}

/// Return the last element of a List or null.
#[no_mangle]
pub extern "C" fn jit_rt_list_last(_ctx: *mut VmContext, list_nb: i64) -> i64 {
    let Some(raw_ptr) = decode_value_ptr(list_nb) else {
        return NAN_BOX_NULL_U as i64;
    };

    let arc_value = unsafe { Arc::from_raw(raw_ptr) };
    let Value::List(list) = arc_value.as_ref() else {
        return NAN_BOX_NULL_U as i64;
    };

    list.last()
        .map(value_to_nanbox)
        .unwrap_or(NAN_BOX_NULL_U as i64)
}

/// Merge two maps (or records) into one: `merge(a, b)` → new map with b's entries overlaid on a.
// ---------------------------------------------------------------------------
// Phase 1e: Set/collection ops — ToSet, Add, Chars, Join, Zip, Enumerate,
//           Chunk, Window
// ---------------------------------------------------------------------------

/// Convert a list to a set (removing duplicates).
/// Returns a NaN-boxed TAG_PTR to a Value::Set.
#[no_mangle]
pub extern "C" fn jit_rt_to_set(_ctx: *mut VmContext, list_nb: i64) -> i64 {
    let val = unsafe { nb_decode(list_nb) };
    let set: BTreeSet<Value> = match val {
        Value::List(l) => l.iter().cloned().collect(),
        Value::Set(_s) => return list_nb, // already a set — return as-is
        _ => BTreeSet::new(),
    };
    wrap_nanbox_value(Value::Set(Arc::new(set)))
}

/// Add an element to a set (returns new set).
/// Returns a NaN-boxed TAG_PTR to a new Value::Set.
#[no_mangle]
pub extern "C" fn jit_rt_set_add(_ctx: *mut VmContext, set_nb: i64, elem_nb: i64) -> i64 {
    let elem = unsafe { nb_decode(elem_nb) };
    let set_val = unsafe { nb_decode(set_nb) };
    let mut set: BTreeSet<Value> = match set_val {
        Value::Set(s) => (*s).clone(),
        Value::List(l) => l.iter().cloned().collect(),
        _ => BTreeSet::new(),
    };
    set.insert(elem);
    wrap_nanbox_value(Value::Set(Arc::new(set)))
}

/// Split a string into a list of single-character strings.
/// Returns a NaN-boxed TAG_PTR to a Value::List.
#[no_mangle]
pub extern "C" fn jit_rt_chars(ctx: *mut VmContext, str_nb: i64) -> i64 {
    let val = unsafe { nb_decode(str_nb) };
    let s = resolve_string(ctx, &val);
    let chars: Vec<Value> = s
        .chars()
        .map(|c| Value::String(lumen_core::values::StringRef::Owned(c.to_string())))
        .collect();
    wrap_nanbox_value(Value::new_list(chars))
}

/// Join a list of strings with a separator.
/// `list_nb` is a NaN-boxed list of strings.
/// `sep_nb` is a NaN-boxed string separator.
/// Returns a NaN-boxed TAG_PTR (Str) of the joined string.
#[no_mangle]
pub extern "C" fn jit_rt_join(ctx: *mut VmContext, list_nb: i64, sep_nb: i64) -> i64 {
    let list_val = unsafe { nb_decode(list_nb) };
    let sep_val = unsafe { nb_decode(sep_nb) };
    let sep = resolve_string(ctx, &sep_val);
    let items: Vec<String> = match list_val {
        Value::List(l) => l.iter().map(|v| resolve_string(ctx, v)).collect(),
        _ => vec![],
    };
    let joined = items.join(&sep);
    wrap_nanbox_value(Value::String(lumen_core::values::StringRef::Owned(joined)))
}

/// Zip two lists into a list of 2-tuples.
/// Returns a NaN-boxed TAG_PTR to a Value::List of Value::Tuple pairs.
#[no_mangle]
pub extern "C" fn jit_rt_zip(_ctx: *mut VmContext, a_nb: i64, b_nb: i64) -> i64 {
    let a_val = unsafe { nb_decode(a_nb) };
    let b_val = unsafe { nb_decode(b_nb) };
    let a_list: &[Value] = match &a_val {
        Value::List(l) => unsafe { &*(l.as_slice() as *const [Value]) },
        _ => &[],
    };
    let b_list: &[Value] = match &b_val {
        Value::List(l) => unsafe { &*(l.as_slice() as *const [Value]) },
        _ => &[],
    };
    let pairs: Vec<Value> = a_list
        .iter()
        .zip(b_list.iter())
        .map(|(a, b)| Value::new_tuple(vec![a.clone(), b.clone()]))
        .collect();
    wrap_nanbox_value(Value::new_list(pairs))
}

/// Enumerate a list into a list of (index, element) 2-tuples.
/// Returns a NaN-boxed TAG_PTR to a Value::List of Value::Tuple pairs.
#[no_mangle]
pub extern "C" fn jit_rt_enumerate(_ctx: *mut VmContext, list_nb: i64) -> i64 {
    let val = unsafe { nb_decode(list_nb) };
    let pairs: Vec<Value> = match val {
        Value::List(l) => l
            .iter()
            .enumerate()
            .map(|(i, v)| Value::new_tuple(vec![Value::Int(i as i64), v.clone()]))
            .collect(),
        _ => vec![],
    };
    wrap_nanbox_value(Value::new_list(pairs))
}

/// Split a list into fixed-size chunks.
/// `n_nb` is the NaN-boxed chunk size (Int).
/// Returns a NaN-boxed TAG_PTR to a Value::List of Value::List chunks.
#[no_mangle]
pub extern "C" fn jit_rt_chunk(_ctx: *mut VmContext, list_nb: i64, n_nb: i64) -> i64 {
    let val = unsafe { nb_decode(list_nb) };
    let n = nb_to_int(n_nb).unwrap_or(1).max(1) as usize;
    let chunks: Vec<Value> = match val {
        Value::List(l) => l
            .chunks(n)
            .map(|chunk| Value::new_list(chunk.to_vec()))
            .collect(),
        _ => vec![],
    };
    wrap_nanbox_value(Value::new_list(chunks))
}

/// Produce a list of sliding windows of size n.
/// `n_nb` is the NaN-boxed window size (Int).
/// Returns a NaN-boxed TAG_PTR to a Value::List of Value::List windows.
#[no_mangle]
pub extern "C" fn jit_rt_window(_ctx: *mut VmContext, list_nb: i64, n_nb: i64) -> i64 {
    let val = unsafe { nb_decode(list_nb) };
    let n = nb_to_int(n_nb).unwrap_or(1).max(1) as usize;
    let windows: Vec<Value> = match val {
        Value::List(l) => {
            if n > l.len() {
                vec![]
            } else {
                l.windows(n).map(|w| Value::new_list(w.to_vec())).collect()
            }
        }
        _ => vec![],
    };
    wrap_nanbox_value(Value::new_list(windows))
}

/// Both `a_ptr` and `b_ptr` are NaN-boxed TAG_PTR values pointing to `Arc<Value>`.
/// Returns a new NaN-boxed TAG_PTR to the merged map.
///
/// # Safety
/// Both pointers must be valid NaN-boxed TAG_PTR to `Arc<Value>` (Map or Record).
#[no_mangle]
pub extern "C" fn jit_rt_merge(_ctx: *mut VmContext, a_ptr: i64, b_ptr: i64) -> i64 {
    let a_val = unsafe { nb_decode(a_ptr) };
    let b_val = unsafe { nb_decode(b_ptr) };

    let result = match (a_val, b_val) {
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
            Value::Record(Arc::new(RecordValue {
                type_name: r1.type_name.clone(),
                fields,
            }))
        }
        (first, _) => first,
    };
    let ptr = Arc::into_raw(Arc::new(result)) as u64;
    (NAN_MASK_U | (ptr & PAYLOAD_MASK_U)) as i64
}

/// Merge two maps/records, taking ownership of the first argument's Arc.
///
/// Unlike `jit_rt_merge`, this function takes *ownership* of `a_ptr`'s Arc via
/// `Arc::from_raw` + `Arc::try_unwrap`. Because only one Rust `Arc` handle exists
/// (refcount == 1), `Arc::make_mut` can mutate the inner `BTreeMap` in-place
/// (O(log n) insert) instead of deep-copying the entire map (O(n)).
///
/// Use this when the caller guarantees `a_ptr` will not be accessed after the call
/// (i.e., the source register came from a `MoveOwn` instruction).
///
/// # Safety
/// `a_ptr` must be a valid NaN-boxed `Arc<Value>` pointer whose Arc refcount is 1,
/// OR an inline scalar (TAG_INT/BOOL/NULL), in which case we fall back to `nb_decode`.
#[no_mangle]
pub extern "C" fn jit_rt_merge_take_a(_ctx: *mut VmContext, a_nb: i64, b_nb: i64) -> i64 {
    // Take ownership of a's Arc without incrementing refcount, so Arc::make_mut
    // can mutate in-place (count stays 1 after from_raw + try_unwrap).
    let u = a_nb as u64;
    let a_val =
        if (u & NAN_MASK_U) == NAN_MASK_U && ((u >> 48) & 0x7) == 0 && (u & PAYLOAD_MASK_U) > 1 {
            // TAG_PTR: consume the Arc — count stays 1, no clone of BTreeMap.
            let ptr = (u & PAYLOAD_MASK_U) as *const Value;
            let arc = unsafe { Arc::<Value>::from_raw(ptr) };
            match Arc::try_unwrap(arc) {
                Ok(val) => val,
                Err(shared) => (*shared).clone(), // fallback: someone else holds a ref
            }
        } else {
            // Inline scalar — decode normally.
            unsafe { nb_decode(a_nb) }
        };

    let b_val = unsafe { nb_decode(b_nb) };

    let result = match (a_val, b_val) {
        (Value::Map(mut m1), Value::Map(m2)) => {
            // Arc::make_mut sees count=1 → mutates in place, no BTreeMap copy.
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
            Value::Record(Arc::new(RecordValue {
                type_name: r1.type_name.clone(),
                fields,
            }))
        }
        (first, _) => first,
    };
    let ptr = Arc::into_raw(Arc::new(result)) as u64;
    (NAN_MASK_U | (ptr & PAYLOAD_MASK_U)) as i64
}

/// Return a list of keys from a map/record.
#[no_mangle]
pub extern "C" fn jit_rt_map_keys(_ctx: *mut VmContext, map_nb: i64) -> i64 {
    let map_val = unsafe { nb_decode(map_nb) };
    let keys = match map_val {
        Value::Map(m) => m
            .keys()
            .map(|k| Value::String(StringRef::Owned(k.clone())))
            .collect(),
        Value::Record(r) => r
            .fields
            .keys()
            .map(|k| Value::String(StringRef::Owned(k.clone())))
            .collect(),
        _ => Vec::new(),
    };
    wrap_nanbox_value(Value::new_list(keys))
}

/// Return a list of values from a map/record.
#[no_mangle]
pub extern "C" fn jit_rt_map_values(_ctx: *mut VmContext, map_nb: i64) -> i64 {
    let map_val = unsafe { nb_decode(map_nb) };
    let values = match map_val {
        Value::Map(m) => m.values().cloned().collect(),
        Value::Record(r) => r.fields.values().cloned().collect(),
        _ => Vec::new(),
    };
    wrap_nanbox_value(Value::new_list(values))
}

/// Return a list of (key, value) tuples from a map/record.
#[no_mangle]
pub extern "C" fn jit_rt_map_entries(_ctx: *mut VmContext, map_nb: i64) -> i64 {
    let map_val = unsafe { nb_decode(map_nb) };
    let entries = match map_val {
        Value::Map(m) => m
            .iter()
            .map(|(k, v)| {
                Value::new_tuple(vec![Value::String(StringRef::Owned(k.clone())), v.clone()])
            })
            .collect(),
        Value::Record(r) => r
            .fields
            .iter()
            .map(|(k, v)| {
                Value::new_tuple(vec![Value::String(StringRef::Owned(k.clone())), v.clone()])
            })
            .collect(),
        _ => Vec::new(),
    };
    wrap_nanbox_value(Value::new_list(entries))
}

/// Check if a map/record has a key.
#[no_mangle]
pub extern "C" fn jit_rt_map_has_key(ctx: *mut VmContext, map_nb: i64, key_nb: i64) -> i64 {
    let map_val = unsafe { nb_decode(map_nb) };
    let key_val = unsafe { nb_decode(key_nb) };
    let key = resolve_string(ctx, &key_val);
    let has_key = match map_val {
        Value::Map(m) => m.contains_key(&key),
        Value::Record(r) => r.fields.contains_key(&key),
        _ => false,
    };
    if has_key {
        NAN_BOX_TRUE_U as i64
    } else {
        NAN_BOX_FALSE_U as i64
    }
}

/// Remove a key from a map/record (returns a new map/record).
#[no_mangle]
pub extern "C" fn jit_rt_map_remove(ctx: *mut VmContext, map_nb: i64, key_nb: i64) -> i64 {
    let map_val = unsafe { nb_decode(map_nb) };
    let key_val = unsafe { nb_decode(key_nb) };
    let key = resolve_string(ctx, &key_val);

    let result = match map_val {
        Value::Map(mut m) => {
            Arc::make_mut(&mut m).remove(&key);
            Value::Map(m)
        }
        Value::Record(r) => {
            let mut fields = r.fields.clone();
            fields.remove(&key);
            Value::Record(Arc::new(RecordValue {
                type_name: r.type_name.clone(),
                fields,
            }))
        }
        other => other,
    };

    wrap_nanbox_value(result)
}

/// Return keys from a map in sorted order (BTreeMap order).
#[no_mangle]
pub extern "C" fn jit_rt_map_sorted_keys(_ctx: *mut VmContext, map_nb: i64) -> i64 {
    let map_val = unsafe { nb_decode(map_nb) };
    let keys = match map_val {
        Value::Map(m) => m
            .keys()
            .map(|k| Value::String(StringRef::Owned(k.clone())))
            .collect(),
        _ => Vec::new(),
    };
    wrap_nanbox_value(Value::new_list(keys))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_result_arc(nb: i64) -> Arc<Value> {
        let ptr = decode_value_ptr(nb).expect("expected encoded Value pointer");
        // The helper contract returns an Arc::into_raw pointer in NbValue TAG_PTR format.
        unsafe { Arc::from_raw(ptr) }
    }

    fn extract_list(value: &Value) -> Arc<Vec<Value>> {
        match value {
            Value::List(list) => list.clone(),
            other => panic!("expected Value::List, got {other:?}"),
        }
    }

    #[test]
    fn list_append_int_creates_list_from_null() {
        let out = jit_rt_list_append_int(std::ptr::null_mut(), NAN_BOX_NULL_U as i64, 42);
        let out_arc = decode_result_arc(out);
        let list = extract_list(&out_arc);
        assert_eq!(&*list, &[Value::Int(42)]);
    }

    #[test]
    fn list_append_float_creates_list_from_null() {
        let out = jit_rt_list_append_float(
            std::ptr::null_mut(),
            NAN_BOX_NULL_U as i64,
            1.25f64.to_bits() as i64,
        );
        let out_arc = decode_result_arc(out);
        let list = extract_list(&out_arc);
        assert_eq!(&*list, &[Value::Float(1.25)]);
    }

    #[test]
    fn list_append_int_preserves_cow_on_shared_input() {
        let shared_src = Arc::new(Value::new_list(vec![Value::Int(1)]));
        let shared_observer = shared_src.clone();
        let input_nb = wrap_nanbox_ptr(Arc::into_raw(shared_src));

        let out = jit_rt_list_append_int(std::ptr::null_mut(), input_nb, 2);
        let out_arc = decode_result_arc(out);
        let out_list = extract_list(&out_arc);

        assert_eq!(&*out_list, &[Value::Int(1), Value::Int(2)]);
        let observed_list = extract_list(&shared_observer);
        assert_eq!(&*observed_list, &[Value::Int(1)]);
    }

    #[test]
    fn range_builds_int_list() {
        let out = jit_rt_range(
            std::ptr::null_mut(),
            value_to_nanbox(&Value::Int(1)),
            value_to_nanbox(&Value::Int(4)),
        );
        let out_arc = decode_result_arc(out);
        let list = extract_list(&out_arc);
        assert_eq!(&*list, &[Value::Int(1), Value::Int(2), Value::Int(3)]);
    }

    #[test]
    fn sort_orders_list_ascending() {
        let list = Value::new_list(vec![Value::Int(3), Value::Int(1), Value::Int(2)]);
        let input_nb = wrap_nanbox_value(list);
        let out = jit_rt_sort(std::ptr::null_mut(), input_nb);
        let out_arc = decode_result_arc(out);
        let list = extract_list(&out_arc);
        assert_eq!(&*list, &[Value::Int(1), Value::Int(2), Value::Int(3)]);
    }

    #[test]
    fn sort_orders_list_descending() {
        let list = Value::new_list(vec![Value::Int(2), Value::Int(3), Value::Int(1)]);
        let input_nb = wrap_nanbox_value(list);
        let out = jit_rt_sort_desc(std::ptr::null_mut(), input_nb);
        let out_arc = decode_result_arc(out);
        let list = extract_list(&out_arc);
        assert_eq!(&*list, &[Value::Int(3), Value::Int(2), Value::Int(1)]);
    }

    #[test]
    fn map_helpers_basic_flow() {
        let mut map = BTreeMap::new();
        map.insert("b".to_string(), Value::Int(2));
        map.insert("a".to_string(), Value::Int(1));
        let map_nb = wrap_nanbox_value(Value::new_map(map));

        let keys_nb = jit_rt_map_keys(std::ptr::null_mut(), map_nb);
        let keys_arc = decode_result_arc(keys_nb);
        let keys = extract_list(&keys_arc);
        assert_eq!(
            &*keys,
            &[
                Value::String(StringRef::Owned("a".to_string())),
                Value::String(StringRef::Owned("b".to_string())),
            ]
        );

        let values_nb = jit_rt_map_values(std::ptr::null_mut(), map_nb);
        let values_arc = decode_result_arc(values_nb);
        let values = extract_list(&values_arc);
        assert_eq!(&*values, &[Value::Int(1), Value::Int(2)]);

        let entries_nb = jit_rt_map_entries(std::ptr::null_mut(), map_nb);
        let entries_arc = decode_result_arc(entries_nb);
        let entries = extract_list(&entries_arc);
        assert_eq!(
            &*entries,
            &[
                Value::new_tuple(vec![
                    Value::String(StringRef::Owned("a".to_string())),
                    Value::Int(1),
                ]),
                Value::new_tuple(vec![
                    Value::String(StringRef::Owned("b".to_string())),
                    Value::Int(2),
                ]),
            ]
        );

        let key_nb = wrap_nanbox_value(Value::String(StringRef::Owned("a".to_string())));
        let has_key = jit_rt_map_has_key(std::ptr::null_mut(), map_nb, key_nb);
        assert_eq!(has_key, NAN_BOX_TRUE_U as i64);

        let missing_key = wrap_nanbox_value(Value::String(StringRef::Owned("z".to_string())));
        let missing = jit_rt_map_has_key(std::ptr::null_mut(), map_nb, missing_key);
        assert_eq!(missing, NAN_BOX_FALSE_U as i64);

        let removed_nb = jit_rt_map_remove(std::ptr::null_mut(), map_nb, key_nb);
        let removed_arc = decode_result_arc(removed_nb);
        if let Value::Map(m) = &*removed_arc {
            assert_eq!(m.len(), 1);
            assert_eq!(m.get("b"), Some(&Value::Int(2)));
        } else {
            panic!("expected Value::Map after remove");
        }

        let sorted_nb = jit_rt_map_sorted_keys(std::ptr::null_mut(), map_nb);
        let sorted_arc = decode_result_arc(sorted_nb);
        let sorted = extract_list(&sorted_arc);
        assert_eq!(
            &*sorted,
            &[
                Value::String(StringRef::Owned("a".to_string())),
                Value::String(StringRef::Owned("b".to_string())),
            ]
        );
    }
}
