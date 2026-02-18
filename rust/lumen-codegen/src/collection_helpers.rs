// ---------------------------------------------------------------------------
// Collection runtime helpers for JIT (List and Map construction)
// ---------------------------------------------------------------------------

use lumen_core::values::Value;
use std::collections::BTreeMap;

const NAN_BOX_NULL: i64 = 0x7ff8_0000_0000_0001;
const NAN_BOX_TRUE: i64 = 0x7ff8_0000_0000_0002;
const NAN_BOX_FALSE: i64 = 0x7ff8_0000_0000_0003;

/// Convert a NaN-boxed or pointer i64 to a heap-allocated Value.
/// Returns a `*mut Value` that must be freed by the caller.
///
/// # Safety
/// If `val` is a pointer, it must be a valid `*const Value` pointer.
unsafe fn nanbox_to_value(val: i64) -> *mut Value {
    // Check for special NaN-boxed values
    if val == NAN_BOX_NULL {
        return Box::into_raw(Box::new(Value::Null));
    }
    if val == NAN_BOX_TRUE {
        return Box::into_raw(Box::new(Value::Bool(true)));
    }
    if val == NAN_BOX_FALSE {
        return Box::into_raw(Box::new(Value::Bool(false)));
    }

    // Check if it's a NaN-boxed integer or float
    let bits = val as u64;
    if (bits & 0x7ff8_0000_0000_0000) == 0x7ff8_0000_0000_0000 {
        // It's a NaN-boxed integer
        let int_val = (val as i32) as i64; // Sign-extend 32-bit int
        return Box::into_raw(Box::new(Value::Int(int_val)));
    }

    // Otherwise, it's either a real float or a pointer
    // If the top 13 bits are NOT all 1s, it's a normal IEEE float
    if (bits >> 51) != 0x1FFF {
        let float_val = f64::from_bits(bits);
        return Box::into_raw(Box::new(Value::Float(float_val)));
    }

    // It's a pointer - clone the Value it points to
    Box::into_raw(Box::new((*(val as *const Value)).clone()))
}

/// Create a new List value from an array of boxed Values.
/// `values_ptr` points to an array of `i64` representing NaN-boxed values or pointers.
/// `count` is the number of elements in the array.
/// Returns a new `*mut Value` containing the List.
///
/// # Safety
/// `values_ptr` must point to a valid array of `count` i64 values.
#[no_mangle]
pub extern "C" fn jit_rt_new_list(values_ptr: *const i64, count: i64) -> i64 {
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
pub extern "C" fn jit_rt_new_map(kvpairs_ptr: *const i64, count: i64) -> i64 {
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

/// Get the length of a collection (List, Map, Set, Tuple, or String).
/// Returns the count as i64, or 0 if the value is not a collection.
///
/// # Safety
/// `value_ptr` must be a valid `*const Value` pointer.
#[no_mangle]
pub extern "C" fn jit_rt_collection_len(value_ptr: i64) -> i64 {
    if value_ptr == 0 || value_ptr == NAN_BOX_NULL {
        return 0;
    }

    let value = unsafe { &*(value_ptr as *const Value) };
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
