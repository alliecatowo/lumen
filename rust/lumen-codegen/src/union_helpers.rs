// ---------------------------------------------------------------------------
// Union (Enum) runtime helpers for JIT
// ---------------------------------------------------------------------------

use crate::vm_context::VmContext;
use lumen_core::heap_value::{HeapValue, UnionData};
use lumen_core::nb_value::NbValue;
use lumen_core::strings::StringTable;
use std::sync::Arc;

// NaN-boxing constants — must match ir.rs and NbValue in lumen-core exactly.
const NAN_BOX_NULL: i64 = 0x7FFC_0000_0000_0000_u64 as i64;
#[allow(dead_code)]
const NAN_BOX_TRUE: i64 = 0x7FFB_0000_0000_0001_u64 as i64;
#[allow(dead_code)]
const NAN_BOX_FALSE: i64 = 0x7FFB_0000_0000_0000_u64 as i64;

const NAN_MASK_U: u64 = 0x7FF8_0000_0000_0000;
const PAYLOAD_MASK_U: u64 = 0x0000_FFFF_FFFF_FFFF;

/// Decode a NaN-boxed i64 back to NbValue.
#[inline]
fn nanbox_to_nb(val: i64) -> NbValue {
    NbValue(val as u64)
}

/// Encode an NbValue as i64 (NaN-boxed).
#[inline]
fn nb_to_nanbox(nb: NbValue) -> i64 {
    nb.0 as i64
}

/// Extract the HeapValue::Union from a TAG_PTR NbValue.
/// Returns None if the NbValue is not a TAG_PTR pointing to a HeapValue::Union.
#[inline]
unsafe fn nb_as_union(u: u64) -> Option<&'static HeapValue> {
    if (u & NAN_MASK_U) != NAN_MASK_U || ((u >> 48) & 0x7) != 0 {
        return None;
    }
    let payload = u & PAYLOAD_MASK_U;
    if payload <= 1 {
        return None;
    }
    let ptr = (payload & !NbValue::PTR_ARENA_FLAG) as *const HeapValue;
    Some(&*ptr)
}

/// Create a new union value (enum variant).
/// `tag_ptr` and `tag_len` describe a UTF-8 string for the variant tag.
/// `payload` is a NaN-boxed value (integer, bool, null, or heap pointer).
/// Returns a NaN-boxed TAG_PTR i64.
///
/// # Safety
/// `tag_ptr` must point to valid UTF-8 bytes of length `tag_len`.
#[no_mangle]
pub extern "C" fn jit_rt_union_new(
    _ctx: *mut VmContext,
    tag_ptr: *const u8,
    tag_len: usize,
    payload: i64,
) -> i64 {
    let tag_str =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(tag_ptr, tag_len)) };
    let payload_nb = nanbox_to_nb(payload);
    let tag = if !_ctx.is_null() {
        unsafe {
            let table_ptr = (*_ctx).string_table as *mut StringTable;
            if !table_ptr.is_null() {
                let table = &mut *table_ptr;
                table.get_or_intern_arc(tag_str)
            } else {
                Arc::from(tag_str)
            }
        }
    } else {
        Arc::from(tag_str)
    };
    let union_val = HeapValue::Union(UnionData {
        tag,
        payload: payload_nb,
    });
    let nb = NbValue::new_heap(union_val);
    nb_to_nanbox(nb)
}

/// Check if a union value has a specific variant tag.
/// Returns 1 if the union has the given tag, 0 otherwise.
///
/// # Safety
/// `union_ptr` must be a valid NaN-boxed value.
/// `tag_ptr` must point to valid UTF-8 bytes of length `tag_len`.
#[no_mangle]
pub extern "C" fn jit_rt_union_is_variant(
    _ctx: *mut VmContext,
    _union_ptr: i64,
    tag_ptr: *const u8,
    tag_len: usize,
) -> i64 {
    let u = _union_ptr as u64;
    let heap = unsafe { nb_as_union(u) };
    let tag_str =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(tag_ptr, tag_len)) };
    match heap {
        Some(HeapValue::Union(uv)) => {
            if !_ctx.is_null() {
                unsafe {
                    let table_ptr = (*_ctx).string_table as *mut StringTable;
                    if !table_ptr.is_null() {
                        let table = &*table_ptr;
                        if let (Some(tag_id), Some(union_id)) =
                            (table.get_id(tag_str), table.get_id(uv.tag.as_ref()))
                        {
                            return if tag_id == union_id { 1 } else { 0 };
                        }
                    }
                }
            }
            if uv.tag.as_ref() == tag_str {
                1
            } else {
                0
            }
        }
        _ => 0,
    }
}

/// Extract the payload from a union value and return it NaN-boxed.
/// Returns `NAN_BOX_NULL` if the input is not a union.
///
/// # Safety
/// `union_ptr` must be a valid NaN-boxed value.
#[no_mangle]
pub extern "C" fn jit_rt_union_unbox(_ctx: *mut VmContext, union_ptr: i64) -> i64 {
    let u = union_ptr as u64;
    let heap = unsafe { nb_as_union(u) };
    match heap {
        Some(HeapValue::Union(uv)) => nb_to_nanbox(uv.payload),
        _ => NAN_BOX_NULL,
    }
}

/// Sentinel value returned by `jit_rt_union_match` when the tag does not match.
pub const UNION_NO_MATCH: i64 = -1_i64;

/// Combined IsVariant + Unbox: check if a union has the given tag and, if so,
/// return the payload NaN-boxed. Returns `UNION_NO_MATCH` if no match.
///
/// # Safety
/// `union_ptr` must be a valid NaN-boxed value.
/// `tag_ptr` must point to valid UTF-8 bytes of length `tag_len`.
#[no_mangle]
pub extern "C" fn jit_rt_union_match(
    _ctx: *mut VmContext,
    union_ptr: i64,
    tag_ptr: *const u8,
    tag_len: usize,
) -> i64 {
    let u = union_ptr as u64;
    let heap = unsafe { nb_as_union(u) };
    let tag_str =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(tag_ptr, tag_len)) };
    match heap {
        Some(HeapValue::Union(uv)) => {
            if !_ctx.is_null() {
                unsafe {
                    let table_ptr = (*_ctx).string_table as *mut StringTable;
                    if !table_ptr.is_null() {
                        let table = &*table_ptr;
                        if let (Some(tag_id), Some(union_id)) =
                            (table.get_id(tag_str), table.get_id(uv.tag.as_ref()))
                        {
                            return if tag_id == union_id {
                                nb_to_nanbox(uv.payload)
                            } else {
                                UNION_NO_MATCH
                            };
                        }
                    }
                }
            }
            if uv.tag.as_ref() == tag_str {
                nb_to_nanbox(uv.payload)
            } else {
                UNION_NO_MATCH
            }
        }
        _ => UNION_NO_MATCH,
    }
}

/// Check if a union value has a specific variant tag, using an interned
/// string ID instead of raw string bytes.
///
/// Note: With HeapValue::Union, tags are stored as Arc<str>. This function
/// compares the interned ID by resolving through the string table when available.
/// For now, returns 0 (use jit_rt_union_is_variant for tag-string matching).
///
/// # Safety
/// `union_ptr` must be a valid NaN-boxed value.
#[no_mangle]
pub extern "C" fn jit_rt_union_is_variant_by_id(
    _ctx: *mut VmContext,
    union_ptr: i64,
    _tag_id: u32,
) -> i64 {
    // HeapValue::Union now uses Arc<str> tags, not interned IDs.
    // This path is only hit by Cranelift JIT which is feature-gated.
    let u = union_ptr as u64;
    let heap = unsafe { nb_as_union(u) };
    match heap {
        Some(HeapValue::Union(_)) => 0, // Cannot match by old interned ID — fall back
        _ => 0,
    }
}

/// Combined IsVariant + Unbox using interned string ID.
/// Returns `UNION_NO_MATCH` — use jit_rt_union_match for string-based matching.
///
/// # Safety
/// `union_ptr` must be a valid NaN-boxed value.
#[no_mangle]
pub extern "C" fn jit_rt_union_match_by_id(
    _ctx: *mut VmContext,
    _union_ptr: i64,
    _tag_id: u32,
) -> i64 {
    // HeapValue::Union now uses Arc<str> tags, not interned IDs.
    UNION_NO_MATCH
}
