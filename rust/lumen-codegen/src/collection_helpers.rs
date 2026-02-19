// ---------------------------------------------------------------------------
// Collection runtime helpers for JIT (List and Map construction)
// ---------------------------------------------------------------------------

use crate::vm_context::VmContext;
use lumen_core::values::Value;
use std::collections::BTreeMap;

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
    let tag = (u >> 48) & 0xF;
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
    Box::into_raw(Box::new(list_value)) as i64
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
    Box::into_raw(Box::new(map_value)) as i64
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
    Box::into_raw(Box::new(tuple_value)) as i64
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
    if (u & NAN_MASK_U) != NAN_MASK_U || ((u >> 48) & 0xF) != 0 || (u & PAYLOAD_MASK_U) <= 1 {
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
