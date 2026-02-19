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
    let tag = (u >> 48) & 0xF;
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

/// Create a new union value (enum variant).
/// `tag_ptr` and `tag_len` describe a UTF-8 string for the variant tag.
/// `payload` is a NaN-boxed value (integer, bool, null, or heap pointer).
/// Returns a raw `*mut Value` cast to i64.
///
/// # Safety
/// `tag_ptr` must point to valid UTF-8 bytes of length `tag_len`.
/// If `payload` is a heap pointer, it must be a valid `*const Value`.
#[no_mangle]
pub extern "C" fn jit_rt_union_new(
    ctx: *mut VmContext,
    tag_ptr: *const u8,
    tag_len: usize,
    payload: i64,
) -> i64 {
    let tag_str =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(tag_ptr, tag_len)) };

    // Use the shared StringTable to intern the tag, producing the same u32 ID
    // as the interpreter's `NewUnion` opcode handler.
    let tag = unsafe {
        let st = (*ctx).string_table;
        assert!(
            !st.is_null(),
            "jit_rt_union_new: VmContext.string_table is null"
        );
        (*st).intern(tag_str)
    };

    let payload_value = unsafe { nanbox_to_value(payload) };

    let union_val = Value::Union(UnionValue {
        tag,
        payload: Arc::new(payload_value),
    });
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
    ctx: *mut VmContext,
    union_ptr: i64,
    tag_ptr: *const u8,
    tag_len: usize,
) -> i64 {
    let u = union_ptr as u64;
    if (u & NAN_MASK_U) != NAN_MASK_U || ((u >> 48) & 0xF) != 0 || (u & PAYLOAD_MASK_U) <= 1 {
        return 0;
    }
    let value = unsafe { &*(( u & PAYLOAD_MASK_U) as *const Value) };
    let tag_str =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(tag_ptr, tag_len)) };

    // Use the shared StringTable to intern the tag — same IDs as the interpreter.
    let tag = unsafe {
        let st = (*ctx).string_table;
        assert!(
            !st.is_null(),
            "jit_rt_union_is_variant: VmContext.string_table is null"
        );
        (*st).intern(tag_str)
    };

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

/// Re-encode a `Value` into its NbValue-compatible i64 representation so the
/// result can be used directly by JIT-compiled code.
///
/// - `Int(n)` → `NAN_MASK | (TAG_INT << 48) | (n & PAYLOAD_MASK)`
/// - `Bool(true)` → `NAN_BOX_TRUE`
/// - `Bool(false)` → `NAN_BOX_FALSE`
/// - `Null` → `NAN_BOX_NULL`
/// - `Float(f)` → raw f64 bits (NOT NaN-boxed)
/// - Everything else → heap-allocate a `Box<Value>` and return as TAG_PTR.
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
            let ptr = Box::into_raw(Box::new(other.clone())) as u64;
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
    if (u & NAN_MASK_U) != NAN_MASK_U || ((u >> 48) & 0xF) != 0 || (u & PAYLOAD_MASK_U) <= 1 {
        return NAN_BOX_NULL;
    }
    let value = unsafe { &*((u & PAYLOAD_MASK_U) as *const Value) };
    match value {
        Value::Union(u) => value_to_nanbox(&u.payload),
        _ => NAN_BOX_NULL,
    }
}
