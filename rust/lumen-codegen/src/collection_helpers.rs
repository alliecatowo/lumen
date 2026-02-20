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
    let u = val as u64;
    if (u & NAN_MASK_U) != NAN_MASK_U {
        // Not NaN-boxed: raw f64 float bits.
        return Box::into_raw(Box::new(Value::Float(f64::from_bits(u))));
    }
    // NbValue uses 3-bit tags at bits 48-50 (bit 51 is the quiet-NaN bit, not part of tag).
    // Must use 0x7 mask (not 0xF) to extract the correct 3-bit tag value.
    let tag = (u >> 48) & 0x7;
    let payload = u & PAYLOAD_MASK_U;
    match tag {
        0 => {
            // TAG_PTR: heap pointer or special sentinel.
            if payload == 0 {
                Box::into_raw(Box::new(Value::Null))
            } else if payload == 1 {
                Box::into_raw(Box::new(Value::Float(f64::NAN)))
            } else {
                Box::into_raw(Box::new((*(payload as *const Value)).clone()))
            }
        }
        1 => {
            // TAG_INT: 48-bit two's-complement integer.
            let signed = if payload & (1 << 47) != 0 {
                (payload | !PAYLOAD_MASK_U) as i64
            } else {
                payload as i64
            };
            Box::into_raw(Box::new(Value::Int(signed)))
        }
        3 => {
            // TAG_BOOL: payload 0=false, 1=true.
            Box::into_raw(Box::new(Value::Bool(payload != 0)))
        }
        4 => {
            // TAG_NULL
            Box::into_raw(Box::new(Value::Null))
        }
        _ => {
            // Unknown tag — treat as null.
            Box::into_raw(Box::new(Value::Null))
        }
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
        let value_ptr = unsafe { nanbox_to_value(val_i64) };
        let value = unsafe { (*value_ptr).clone() };
        unsafe { drop(Box::from_raw(value_ptr)) }; // Free the temporary Value
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

        // Convert key to Value and then to string
        let key_ptr = unsafe { nanbox_to_value(key_i64) };
        let key_str = unsafe { (*key_ptr).as_string() };
        unsafe { drop(Box::from_raw(key_ptr)) }; // Free the temporary Value

        // Convert value to Value
        let value_ptr = unsafe { nanbox_to_value(value_i64) };
        let value = unsafe { (*value_ptr).clone() };
        unsafe { drop(Box::from_raw(value_ptr)) }; // Free the temporary Value

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
        let value_ptr = unsafe { nanbox_to_value(val_i64) };
        let value = unsafe { (*value_ptr).clone() };
        unsafe { drop(Box::from_raw(value_ptr)) }; // Free the temporary Value
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
        let value_ptr = unsafe { nanbox_to_value(val_i64) };
        let value = unsafe { (*value_ptr).clone() };
        unsafe { drop(Box::from_raw(value_ptr)) };
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
    // Decode the element to append from NaN-boxed encoding.
    let element_value = unsafe {
        let eptr = nanbox_to_value(element);
        let v = (*eptr).clone();
        drop(Box::from_raw(eptr));
        v
    };

    // Extract raw pointer from NaN-boxed or raw form.
    let raw_ptr = {
        let u = list_ptr as u64;
        if u == 0 || u == 0x7FFC_0000_0000_0000 {
            // Null — create a fresh single-element list.
            let result = Value::new_list(vec![element_value]);
            let ptr = Arc::into_raw(Arc::new(result)) as u64;
            return (NAN_MASK_U | (ptr & PAYLOAD_MASK_U)) as i64;
        }
        if (u & NAN_MASK_U) == NAN_MASK_U && ((u >> 48) & 0x7) == 0 {
            (u & PAYLOAD_MASK_U) as *const Value
        } else if u > 1 && u < (1u64 << 48) {
            u as *const Value
        } else {
            let result = Value::new_list(vec![element_value]);
            let ptr = Arc::into_raw(Arc::new(result)) as u64;
            return (NAN_MASK_U | (ptr & PAYLOAD_MASK_U)) as i64;
        }
    };

    // Take ownership via Arc — JIT register is consumed after this call.
    // NbValue uses Arc for heap allocations, so we must use Arc::from_raw here.
    let mut arc_list = unsafe { Arc::from_raw(raw_ptr) };
    if let Value::List(inner_arc) = Arc::make_mut(&mut arc_list) {
        Arc::make_mut(inner_arc).push(element_value);
    }
    let ptr = Arc::into_raw(arc_list) as u64;
    (NAN_MASK_U | (ptr & PAYLOAD_MASK_U)) as i64
}
