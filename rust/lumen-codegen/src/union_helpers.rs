// ---------------------------------------------------------------------------
// Union (Enum) runtime helpers for JIT
// ---------------------------------------------------------------------------

use lumen_core::values::{UnionValue, Value};
use std::sync::Arc;

const NAN_BOX_NULL: i64 = 0x7ff8_0000_0000_0001;

/// Create a new union value (enum variant).
/// `tag_ptr` and `tag_len` describe a UTF-8 string for the variant tag.
/// `payload_ptr` is a pointer to a boxed Value (or 0 for unit variants).
/// Returns a new `*mut Value` containing the Union.
///
/// # Safety
/// `tag_ptr` must point to valid UTF-8 bytes of length `tag_len`.
/// `payload_ptr` must be a valid `*mut Value` pointer or 0.
#[no_mangle]
pub extern "C" fn jit_rt_union_new(tag_ptr: *const u8, tag_len: usize, payload_ptr: i64) -> i64 {
    let tag_str =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(tag_ptr, tag_len)) };

    // Create a local StringTable to intern the tag.
    // Note: This is a limitation - each union creation gets a fresh intern ID.
    // For proper implementation, we'd need to thread a global StringTable through.
    // For now, we store the tag as a simple hash of the string.
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    tag_str.hash(&mut hasher);
    let tag = hasher.finish() as u32;

    let payload = if payload_ptr == 0 || payload_ptr == NAN_BOX_NULL {
        Arc::new(Value::Null)
    } else {
        Arc::new(unsafe { (*(payload_ptr as *const Value)).clone() })
    };

    let union_val = Value::Union(UnionValue { tag, payload });
    Box::into_raw(Box::new(union_val)) as i64
}

/// Check if a union value has a specific variant tag.
/// Returns 1 if the union has the given tag, 0 otherwise.
///
/// # Safety
/// `union_ptr` must be a valid `*mut Value` pointer.
/// `tag_ptr` must point to valid UTF-8 bytes of length `tag_len`.
#[no_mangle]
pub extern "C" fn jit_rt_union_is_variant(
    union_ptr: i64,
    tag_ptr: *const u8,
    tag_len: usize,
) -> i64 {
    if union_ptr == 0 || union_ptr == NAN_BOX_NULL {
        return 0;
    }

    let value = unsafe { &*(union_ptr as *const Value) };
    let tag_str =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(tag_ptr, tag_len)) };

    // Hash the tag string to compare with the union's tag
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    tag_str.hash(&mut hasher);
    let tag = hasher.finish() as u32;

    match value {
        Value::Union(u) => {
            if u.tag == tag {
                1
            } else {
                0
            }
        }
        _ => 0,
    }
}

/// Extract the payload from a union value.
/// Returns a new `*mut Value` containing the payload (cloned).
/// Returns null (0) if the input is not a union.
///
/// # Safety
/// `union_ptr` must be a valid `*mut Value` pointer.
#[no_mangle]
pub extern "C" fn jit_rt_union_unbox(union_ptr: i64) -> i64 {
    if union_ptr == 0 || union_ptr == NAN_BOX_NULL {
        return 0;
    }

    let value = unsafe { &*(union_ptr as *const Value) };
    match value {
        Value::Union(u) => Box::into_raw(Box::new((*u.payload).clone())) as i64,
        _ => 0,
    }
}
