// ---------------------------------------------------------------------------
// Union (Enum) runtime helpers for JIT
// ---------------------------------------------------------------------------

use crate::vm_context::VmContext;
use lumen_core::nb_value::NbValue;
use lumen_core::values::{UnionPayload, UnionValue, Value};
use std::sync::Arc;

// NaN-boxing constants — must match ir.rs and NbValue in lumen-core exactly.
const NAN_BOX_NULL: i64 = 0x7FFC_0000_0000_0000_u64 as i64; // NAN_MASK | (TAG_NULL=4 << 48)
const NAN_BOX_TRUE: i64 = 0x7FFB_0000_0000_0001_u64 as i64; // NAN_MASK | (TAG_BOOL=3 << 48) | 1
const NAN_BOX_FALSE: i64 = 0x7FFB_0000_0000_0000_u64 as i64; // NAN_MASK | (TAG_BOOL=3 << 48) | 0

const NAN_MASK_U: u64 = 0x7FF8_0000_0000_0000;
const PAYLOAD_MASK_U: u64 = 0x0000_FFFF_FFFF_FFFF;

/// Decode a NaN-boxed value into a `UnionPayload`. For scalar types (Int, Bool,
/// Null, Float), returns an inline `UnionPayload` variant with **no Arc
/// allocation**. For heap pointers (TAG_PTR with payload > 1), takes ownership
/// of the existing `Arc<Value>` via `Arc::from_raw` and wraps it in
/// `UnionPayload::Heap` — again, no allocation.
///
/// # Safety
/// For TAG_PTR payloads with payload > 1, the raw pointer must have been
/// produced by `Arc::into_raw` and must not be used again after this call.
#[inline]
unsafe fn nanbox_to_union_payload(val: i64) -> UnionPayload {
    let u = val as u64;
    if (u & NAN_MASK_U) != NAN_MASK_U {
        return UnionPayload::Float(f64::from_bits(u));
    }
    let tag = (u >> 48) & 0x7;
    let payload = u & PAYLOAD_MASK_U;
    match tag {
        0 => {
            if payload == 0 {
                UnionPayload::Null
            } else if payload == 1 {
                UnionPayload::Float(f64::NAN)
            } else {
                // Take ownership of the existing Arc — no clone needed.
                if payload & NbValue::PTR_ARENA_FLAG != 0 {
                    let value = &*((payload & !NbValue::PTR_ARENA_FLAG) as *const Value);
                    UnionPayload::from_value(value.clone())
                } else {
                    let arc = Arc::from_raw((payload & !NbValue::PTR_ARENA_FLAG) as *const Value);
                    UnionPayload::from_arc(arc)
                }
            }
        }
        1 => {
            let signed = if payload & (1 << 47) != 0 {
                (payload | !PAYLOAD_MASK_U) as i64
            } else {
                payload as i64
            };
            UnionPayload::Int(signed)
        }
        3 => UnionPayload::Bool(payload != 0),
        4 => UnionPayload::Null,
        _ => UnionPayload::Null,
    }
}

/// Convert a `UnionPayload` to its NaN-boxed i64 representation. For inline
/// scalars (Int, Bool, Null, Float) this is a direct encoding with no
/// allocation. For `Heap` values, bumps the Arc refcount via `Arc::clone` and
/// returns the raw pointer — **no deep clone**.
#[inline]
fn union_payload_to_nanbox(p: &UnionPayload) -> i64 {
    match p {
        UnionPayload::Int(n) => {
            let payload = (*n as u64) & PAYLOAD_MASK_U;
            (NAN_MASK_U | (1u64 << 48) | payload) as i64
        }
        UnionPayload::Bool(true) => NAN_BOX_TRUE,
        UnionPayload::Bool(false) => NAN_BOX_FALSE,
        UnionPayload::Null => NAN_BOX_NULL,
        UnionPayload::Float(f) => f.to_bits() as i64,
        UnionPayload::Heap(arc) => {
            // Bump refcount and return raw pointer — no deep clone.
            let ptr = Arc::into_raw(Arc::clone(arc)) as u64;
            (NAN_MASK_U | (ptr & PAYLOAD_MASK_U)) as i64
        }
    }
}

/// Create a new union value (enum variant).
/// `tag_ptr` and `tag_len` describe a UTF-8 string for the variant tag.
/// `payload` is a NaN-boxed value (integer, bool, null, or heap pointer).
/// Returns a NaN-boxed TAG_PTR (NAN_MASK | pointer) i64.
///
/// # Safety
/// `tag_ptr` must point to valid UTF-8 bytes of length `tag_len`.
/// If `payload` is a heap pointer (TAG_PTR), this function takes ownership
/// of the Arc — the caller must not use the payload pointer again.
#[no_mangle]
pub extern "C" fn jit_rt_union_new(
    ctx: *mut VmContext,
    tag_ptr: *const u8,
    tag_len: usize,
    payload: i64,
) -> i64 {
    let tag_str =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(tag_ptr, tag_len)) };

    let tag = unsafe {
        let st = &mut *(*ctx).string_table;
        st.intern(tag_str)
    };

    // Decode payload into UnionPayload — inline scalars avoid Arc allocation.
    let payload = unsafe { nanbox_to_union_payload(payload) };

    let union_val = Value::Union(UnionValue { tag, payload });
    let ptr = Arc::into_raw(Arc::new(union_val)) as u64;
    (NAN_MASK_U | (ptr & PAYLOAD_MASK_U)) as i64
}

/// Check if a union value has a specific variant tag.
/// Returns 1 if the union has the given tag, 0 otherwise.
///
/// # Safety
/// `union_ptr` must be a valid `*mut Value` pointer.
/// `tag_ptr` must point to valid UTF-8 bytes of length `tag_len`.
#[no_mangle]
pub extern "C" fn jit_rt_union_is_variant(
    ctx: *mut VmContext,
    union_ptr: i64,
    tag_ptr: *const u8,
    tag_len: usize,
) -> i64 {
    let u = union_ptr as u64;
    if (u & NAN_MASK_U) != NAN_MASK_U || ((u >> 48) & 0x7) != 0 || (u & PAYLOAD_MASK_U) <= 1 {
        return 0;
    }
    let value = unsafe { &*((u & PAYLOAD_MASK_U) as *const Value) };
    let tag_str =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(tag_ptr, tag_len)) };

    match value {
        Value::Union(uv) => {
            let st = unsafe { &*(*ctx).string_table };
            if let Some(resolved) = st.resolve(uv.tag) {
                if resolved == tag_str {
                    1
                } else {
                    0
                }
            } else {
                0
            }
        }
        _ => 0,
    }
}

/// Extract the payload from a union value and return it NaN-boxed so the
/// JIT can use it directly (e.g. in arithmetic, function calls, or further
/// union construction).
///
/// Returns `NAN_BOX_NULL` if the input is not a union.
///
/// # Safety
/// `union_ptr` must be a valid `*const Value` pointer (or 0 / NAN_BOX_NULL).
#[no_mangle]
pub extern "C" fn jit_rt_union_unbox(_ctx: *mut VmContext, union_ptr: i64) -> i64 {
    let u = union_ptr as u64;
    if (u & NAN_MASK_U) != NAN_MASK_U || ((u >> 48) & 0x7) != 0 || (u & PAYLOAD_MASK_U) <= 1 {
        return NAN_BOX_NULL;
    }
    let value = unsafe { &*((u & PAYLOAD_MASK_U) as *const Value) };
    match value {
        Value::Union(uv) => union_payload_to_nanbox(&uv.payload),
        _ => NAN_BOX_NULL,
    }
}

/// Sentinel value returned by `jit_rt_union_match` when the tag does not match.
/// Uses NbValue tag 7 (unused) so it's distinguishable from any valid NaN-boxed value.
pub const UNION_NO_MATCH: i64 = -1_i64; // 0xFFFF_FFFF_FFFF_FFFF

/// Combined IsVariant + Unbox: check if a union has the given tag and, if so,
/// return the payload NaN-boxed.  Returns `UNION_NO_MATCH` if the tag does not
/// match (or the value is not a union).
///
/// # Safety
/// `union_ptr` must be a valid NaN-boxed TAG_PTR to an `Arc<Value>`.
/// `tag_ptr` must point to valid UTF-8 bytes of length `tag_len`.
#[no_mangle]
pub extern "C" fn jit_rt_union_match(
    ctx: *mut VmContext,
    union_ptr: i64,
    tag_ptr: *const u8,
    tag_len: usize,
) -> i64 {
    let u = union_ptr as u64;
    if (u & NAN_MASK_U) != NAN_MASK_U || ((u >> 48) & 0x7) != 0 || (u & PAYLOAD_MASK_U) <= 1 {
        return UNION_NO_MATCH;
    }
    let value = unsafe { &*((u & PAYLOAD_MASK_U) as *const Value) };
    let tag_str =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(tag_ptr, tag_len)) };

    match value {
        Value::Union(uv) => {
            let st = unsafe { &*(*ctx).string_table };
            if let Some(resolved) = st.resolve(uv.tag) {
                if resolved != tag_str {
                    return UNION_NO_MATCH;
                }
                union_payload_to_nanbox(&uv.payload)
            } else {
                UNION_NO_MATCH
            }
        }
        _ => UNION_NO_MATCH,
    }
}

/// Check if a union value has a specific variant tag, using an interned
/// string ID instead of raw string bytes. This is O(1) — a single integer
/// comparison — instead of the string-table resolve + string compare path.
///
/// # Safety
/// `union_ptr` must be a valid NaN-boxed TAG_PTR to an `Arc<Value>`.
#[no_mangle]
pub extern "C" fn jit_rt_union_is_variant_by_id(
    _ctx: *mut VmContext,
    union_ptr: i64,
    tag_id: u32,
) -> i64 {
    let u = union_ptr as u64;
    if (u & NAN_MASK_U) != NAN_MASK_U || ((u >> 48) & 0x7) != 0 || (u & PAYLOAD_MASK_U) <= 1 {
        return 0;
    }
    let value = unsafe { &*((u & PAYLOAD_MASK_U) as *const Value) };
    match value {
        Value::Union(uv) => {
            if uv.tag == tag_id {
                1
            } else {
                0
            }
        }
        _ => 0,
    }
}

/// Combined IsVariant + Unbox using interned string ID. Returns the payload
/// NaN-boxed if the tag matches, or `UNION_NO_MATCH` otherwise.
/// O(1) integer comparison instead of string-table resolve + string compare.
///
/// # Safety
/// `union_ptr` must be a valid NaN-boxed TAG_PTR to an `Arc<Value>`.
#[no_mangle]
pub extern "C" fn jit_rt_union_match_by_id(
    _ctx: *mut VmContext,
    union_ptr: i64,
    tag_id: u32,
) -> i64 {
    let u = union_ptr as u64;
    if (u & NAN_MASK_U) != NAN_MASK_U || ((u >> 48) & 0x7) != 0 || (u & PAYLOAD_MASK_U) <= 1 {
        return UNION_NO_MATCH;
    }
    let value = unsafe { &*((u & PAYLOAD_MASK_U) as *const Value) };
    match value {
        Value::Union(uv) => {
            if uv.tag != tag_id {
                return UNION_NO_MATCH;
            }
            union_payload_to_nanbox(&uv.payload)
        }
        _ => UNION_NO_MATCH,
    }
}
