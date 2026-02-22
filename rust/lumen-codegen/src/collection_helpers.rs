// ---------------------------------------------------------------------------
// Collection runtime helpers for JIT (List and Map construction)
// ---------------------------------------------------------------------------

use crate::vm_context::VmContext;
use lumen_core::values::{RecordValue, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

// NbValue encoding constants (match lumen-core/src/values.rs NbValue exactly):
//   NAN_MASK  = 0x7FF8_0000_0000_0000
//   TAG_PTR=0, TAG_INT=1, TAG_ATOM=2, TAG_BOOL=3, TAG_NULL=4, TAG_FIBER=5
const NAN_MASK_U: u64 = 0x7FF8_0000_0000_0000;
const PAYLOAD_MASK_U: u64 = 0x0000_FFFF_FFFF_FFFF;
const NAN_BOX_NULL_U: u64 = 0x7FFC_0000_0000_0000;

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

#[inline]
fn singleton_list_value(element: Value) -> i64 {
    wrap_nanbox_value(Value::new_list(vec![element]))
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

/// Merge two maps (or records) into one: `merge(a, b)` → new map with b's entries overlaid on a.
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
    let a_val = if (u & NAN_MASK_U) == NAN_MASK_U
        && ((u >> 48) & 0x7) == 0
        && (u & PAYLOAD_MASK_U) > 1
    {
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
}
