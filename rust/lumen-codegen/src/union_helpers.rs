// ---------------------------------------------------------------------------
// Union (Enum) runtime helpers for JIT
// ---------------------------------------------------------------------------

use crate::vm_context::VmContext;
use lumen_core::values::{UnionValue, Value};
use std::sync::Arc;

// NaN-boxing constants — must match ir.rs and NbValue in lumen-core exactly.
const NAN_BOX_NULL: i64 = 0x7FFC_0000_0000_0000_u64 as i64; // NAN_MASK | (TAG_NULL=4 << 48)
const NAN_BOX_TRUE: i64 = 0x7FFB_0000_0000_0001_u64 as i64; // NAN_MASK | (TAG_BOOL=3 << 48) | 1
const NAN_BOX_FALSE: i64 = 0x7FFB_0000_0000_0000_u64 as i64; // NAN_MASK | (TAG_BOOL=3 << 48) | 0

const NAN_MASK_U: u64 = 0x7FF8_0000_0000_0000;
const PAYLOAD_MASK_U: u64 = 0x0000_FFFF_FFFF_FFFF;

/// Decode a NbValue-encoded i64 from the JIT into a `Value`.
///
/// Encoding scheme matches NbValue in lumen-core:
///   - Non-NaN bits: raw f64 float
///   - TAG_PTR=0: heap pointer to a `Value` (or special: 0=Null, 1=NaN float)
///   - TAG_INT=1: 48-bit signed integer
///   - TAG_BOOL=3: boolean (payload 0=false, 1=true)
///   - TAG_NULL=4: null value
///
/// # Safety
/// If `val` is a TAG_PTR with payload > 1, it must be a valid `*const Value`.
unsafe fn nanbox_to_value(val: i64) -> Value {
    let u = val as u64;
    if (u & NAN_MASK_U) != NAN_MASK_U {
        // Not NaN-boxed: raw f64 float bits.
        return Value::Float(f64::from_bits(u));
    }
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
        4 => Value::Null, // TAG_NULL
        _ => Value::Null, // Unknown tag
    }
}

/// Decode a NaN-boxed payload directly into an `Arc<Value>` for use as a
/// union payload.  For heap pointers (TAG_PTR), takes ownership of the
/// existing Arc via `Arc::from_raw` — **no deep clone**.  For inline types
/// (Int, Bool, Null, Float), wraps in a fresh Arc.
///
/// # Safety
/// For TAG_PTR payloads with payload > 1, the raw pointer must have been
/// produced by `Arc::into_raw` and must not be used again after this call.
#[inline]
unsafe fn nanbox_to_payload_arc(val: i64) -> Arc<Value> {
    let u = val as u64;
    if (u & NAN_MASK_U) != NAN_MASK_U {
        return Arc::new(Value::Float(f64::from_bits(u)));
    }
    let tag = (u >> 48) & 0x7;
    let payload = u & PAYLOAD_MASK_U;
    match tag {
        0 => {
            if payload == 0 {
                Arc::new(Value::Null)
            } else if payload == 1 {
                Arc::new(Value::Float(f64::NAN))
            } else {
                // Take ownership of the existing Arc — no clone needed.
                // The JIT register holding this pointer is consumed (dead after
                // NewUnion writes its result to a different register).
                Arc::from_raw(payload as *const Value)
            }
        }
        1 => {
            let signed = if payload & (1 << 47) != 0 {
                (payload | !PAYLOAD_MASK_U) as i64
            } else {
                payload as i64
            };
            Arc::new(Value::Int(signed))
        }
        3 => Arc::new(Value::Bool(payload != 0)),
        4 => Arc::new(Value::Null),
        _ => Arc::new(Value::Null),
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

    // Take ownership of the payload Arc directly — avoids deep clone + extra
    // Arc allocation that was the #1 bottleneck in build_tree (262K nodes).
    let payload_arc = unsafe { nanbox_to_payload_arc(payload) };

    let union_val = Value::Union(UnionValue {
        tag,
        payload: payload_arc,
    });
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
            // Resolve the union's tag ID to a string via Vec lookup (O(1)),
            // then compare strings directly.
            let st = unsafe { &*(*ctx).string_table };
            if let Some(resolved) = st.resolve(uv.tag) {
                if resolved == tag_str { 1 } else { 0 }
            } else {
                0
            }
        }
        _ => 0,
    }
}

/// Re-encode a `Value` into its NbValue-compatible i64 representation so the
/// result can be used directly by JIT-compiled code.
///
/// - `Int(n)` → `NAN_MASK | (TAG_INT << 48) | (n & PAYLOAD_MASK)`
/// - `Bool(true)` → `NAN_BOX_TRUE`
/// - `Bool(false)` → `NAN_BOX_FALSE`
/// - `Null` → `NAN_BOX_NULL`
/// - `Float(f)` → raw f64 bits (NOT NaN-boxed)
/// - Everything else → heap-allocate via `Arc<Value>` and return as TAG_PTR.
fn value_to_nanbox(v: &Value) -> i64 {
    match v {
        Value::Int(n) => {
            let payload = (*n as u64) & PAYLOAD_MASK_U;
            (NAN_MASK_U | (1u64 << 48) | payload) as i64
        }
        Value::Bool(true) => NAN_BOX_TRUE,
        Value::Bool(false) => NAN_BOX_FALSE,
        Value::Null => NAN_BOX_NULL,
        Value::Float(f) => f.to_bits() as i64,
        other => {
            // Must use Arc to match NbValue's heap convention (peek_legacy /
            // drop_heap use Arc reference counting).
            let ptr = Arc::into_raw(Arc::new(other.clone())) as u64;
            (NAN_MASK_U | (ptr & PAYLOAD_MASK_U)) as i64
        }
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
        Value::Union(uv) => {
            // Fast path for inline types — return NaN-boxed directly.
            match &*uv.payload {
                Value::Int(n) => {
                    let payload = (*n as u64) & PAYLOAD_MASK_U;
                    (NAN_MASK_U | (1u64 << 48) | payload) as i64
                }
                Value::Bool(true) => NAN_BOX_TRUE,
                Value::Bool(false) => NAN_BOX_FALSE,
                Value::Null => NAN_BOX_NULL,
                Value::Float(f) => f.to_bits() as i64,
                _ => {
                    // Reuse existing payload Arc — just bump refcount instead
                    // of cloning the Value and wrapping in a redundant new Arc.
                    let ptr = Arc::into_raw(Arc::clone(&uv.payload)) as u64;
                    (NAN_MASK_U | (ptr & PAYLOAD_MASK_U)) as i64
                }
            }
        }
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
/// This eliminates a separate Unbox extern "C" call after every IsVariant check,
/// saving ~524K function calls for the tree benchmark.
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
                // Tag matches — return the payload NaN-boxed.
                match &*uv.payload {
                    Value::Int(n) => {
                        let payload = (*n as u64) & PAYLOAD_MASK_U;
                        (NAN_MASK_U | (1u64 << 48) | payload) as i64
                    }
                    Value::Bool(true) => NAN_BOX_TRUE,
                    Value::Bool(false) => NAN_BOX_FALSE,
                    Value::Null => NAN_BOX_NULL,
                    Value::Float(f) => f.to_bits() as i64,
                    _ => {
                        let ptr = Arc::into_raw(Arc::clone(&uv.payload)) as u64;
                        (NAN_MASK_U | (ptr & PAYLOAD_MASK_U)) as i64
                    }
                }
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
            if uv.tag == tag_id { 1 } else { 0 }
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
            // Tag matches — return the payload NaN-boxed.
            match &*uv.payload {
                Value::Int(n) => {
                    let payload = (*n as u64) & PAYLOAD_MASK_U;
                    (NAN_MASK_U | (1u64 << 48) | payload) as i64
                }
                Value::Bool(true) => NAN_BOX_TRUE,
                Value::Bool(false) => NAN_BOX_FALSE,
                Value::Null => NAN_BOX_NULL,
                Value::Float(f) => f.to_bits() as i64,
                _ => {
                    let ptr = Arc::into_raw(Arc::clone(&uv.payload)) as u64;
                    (NAN_MASK_U | (ptr & PAYLOAD_MASK_U)) as i64
                }
            }
        }
        _ => UNION_NO_MATCH,
    }
}
