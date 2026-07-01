// ---------------------------------------------------------------------------
// Collection runtime helpers for JIT (NbValue-native)
// ---------------------------------------------------------------------------

use crate::vm_context::VmContext;
use lumen_core::arena::ValueArena;
use lumen_core::heap_value::{HeapValue, RecordData};
use lumen_core::nb_value::NbValue;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

// NbValue encoding constants (match lumen-core/src/nb_value.rs exactly):
//   NAN_MASK  = 0x7FF8_0000_0000_0000
//   TAG_PTR=0, TAG_INT=1, TAG_ATOM=2, TAG_BOOL=3, TAG_NULL=4
const NAN_MASK_U: u64 = 0x7FF8_0000_0000_0000;
const PAYLOAD_MASK_U: u64 = 0x0000_FFFF_FFFF_FFFF;
const PTR_ARENA_FLAG_U: u64 = 1;
const NAN_BOX_TRUE_U: u64 = NAN_MASK_U | (3u64 << 48) | 1;
const NAN_BOX_FALSE_U: u64 = NAN_MASK_U | (3u64 << 48);
const NAN_BOX_NULL_U: u64 = 0x7FFC_0000_0000_0000;

// ---------------------------------------------------------------------------
// NbValue helpers
// ---------------------------------------------------------------------------

#[inline]
fn nb_from_bits(bits: i64) -> NbValue {
    NbValue::from_bits(bits as u64)
}

#[inline]
fn nb_to_bits(nb: NbValue) -> i64 {
    nb.to_bits() as i64
}

#[inline]
fn nb_to_int(value_nb: i64) -> Option<i64> {
    nb_from_bits(value_nb).as_int()
}

#[inline]
fn wrap_nanbox_ptr(ptr: *const HeapValue) -> i64 {
    let raw = ptr as u64;
    (NAN_MASK_U | (raw & PAYLOAD_MASK_U)) as i64
}

#[inline]
fn wrap_nanbox_ptr_arena(ptr: *const HeapValue) -> i64 {
    let raw = (ptr as u64) & PAYLOAD_MASK_U;
    (NAN_MASK_U | (raw | PTR_ARENA_FLAG_U)) as i64
}

#[inline]
fn wrap_nanbox_heap(value: HeapValue) -> i64 {
    let ptr = Arc::into_raw(Arc::new(value));
    wrap_nanbox_ptr(ptr)
}

#[inline]
fn wrap_nanbox_heap_with_ctx(ctx: *mut VmContext, value: HeapValue) -> i64 {
    if !ctx.is_null() {
        let arena = unsafe { (*ctx).arena } as *mut ValueArena;
        if !arena.is_null() {
            let ptr = unsafe { (*arena).alloc_value(value) } as *mut HeapValue;
            return wrap_nanbox_ptr_arena(ptr as *const HeapValue);
        }
    }
    wrap_nanbox_heap(value)
}

#[inline]
fn resolve_string(ctx: *mut VmContext, nb: NbValue) -> String {
    if let Some(HeapValue::Str(s)) = nb.as_heap_ref() {
        return s.to_string();
    }
    let _ = ctx; // reserved for future string table resolution
    nb.display()
}

#[inline]
fn value_is_truthy(nb: NbValue) -> bool {
    nb.is_truthy()
}

#[inline]
fn sort_list_fast(items: &mut Vec<NbValue>) {
    if items.len() <= 1 {
        return;
    }
    if items.iter().all(|v| v.is_int()) {
        items.sort_unstable_by(|a, b| a.as_int().unwrap_or(0).cmp(&b.as_int().unwrap_or(0)));
        return;
    }
    if items.iter().all(|v| v.is_float()) {
        items.sort_unstable_by(|a, b| {
            f64::from_bits(a.to_bits()).total_cmp(&f64::from_bits(b.to_bits()))
        });
        return;
    }
    items.sort();
}

#[inline]
fn cmp_nb_value_natural(lhs: &NbValue, rhs: &NbValue) -> Ordering {
    lhs.cmp(rhs)
}

#[inline]
fn decode_heap_ptr(list_ptr: i64) -> Option<*const HeapValue> {
    let u = list_ptr as u64;
    if u == 0 || u == NAN_BOX_NULL_U {
        return None;
    }
    // NaN-boxed TAG_PTR.
    if (u & NAN_MASK_U) == NAN_MASK_U && ((u >> 48) & 0x7) == 0 {
        let payload = u & PAYLOAD_MASK_U;
        return if payload > 1 {
            Some((payload & !PTR_ARENA_FLAG_U) as *const HeapValue)
        } else {
            None
        };
    }
    // Raw pointer form used by some helper paths.
    if u > 1 && u < (1u64 << 48) {
        return Some(u as *const HeapValue);
    }
    None
}

#[inline]
fn nb_is_arena_ptr(nb_bits: i64) -> bool {
    let u = nb_bits as u64;
    if (u & NAN_MASK_U) == NAN_MASK_U && ((u >> 48) & 0x7) == 0 {
        let payload = u & PAYLOAD_MASK_U;
        return payload > 1 && (payload & PTR_ARENA_FLAG_U) != 0;
    }
    false
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
#[inline]
fn call_hof_unary(ctx: *mut VmContext, closure_nb: i64, arg: NbValue) -> NbValue {
    let arg_nb = nb_to_bits(arg);
    let args = [arg_nb];
    let result_nb = jit_rt_call_closure(ctx, closure_nb, args.as_ptr(), 1);
    nb_from_bits(result_nb)
}

// Call a binary closure (two args) via ctx->call_closure trampoline.
#[inline]
fn call_hof_binary(ctx: *mut VmContext, closure_nb: i64, left: NbValue, right: NbValue) -> NbValue {
    let left_nb = nb_to_bits(left);
    let right_nb = nb_to_bits(right);
    let args = [left_nb, right_nb];
    let result_nb = jit_rt_call_closure(ctx, closure_nb, args.as_ptr(), 2);
    nb_from_bits(result_nb)
}

#[inline]
fn append_to_list_value(ctx: *mut VmContext, list_ptr: i64, element_nb: NbValue) -> i64 {
    let Some(raw_ptr) = decode_heap_ptr(list_ptr) else {
        return wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(vec![element_nb])));
    };

    let arena_flagged = nb_is_arena_ptr(list_ptr);
    let is_arena_ptr = arena_flagged
        || if !ctx.is_null() {
            let arena = unsafe { (*ctx).arena };
            if arena.is_null() {
                false
            } else {
                unsafe { (*arena).contains_ptr(raw_ptr) }
            }
        } else {
            false
        };

    if is_arena_ptr {
        unsafe {
            if let HeapValue::List(inner_arc) = &mut *(raw_ptr as *mut HeapValue) {
                Arc::make_mut(inner_arc).push(element_nb);
            }
        }
        return wrap_nanbox_ptr_arena(raw_ptr);
    }

    // Take ownership via Arc — the caller overwrites the source register.
    let mut arc_list = unsafe { Arc::from_raw(raw_ptr) };
    if let HeapValue::List(inner_arc) = Arc::make_mut(&mut arc_list) {
        Arc::make_mut(inner_arc).push(element_nb);
    }
    wrap_nanbox_ptr(Arc::into_raw(arc_list))
}

// ---------------------------------------------------------------------------
// Collection constructors
// ---------------------------------------------------------------------------

/// Create a new List value from an array of NbValue bits.
#[no_mangle]
pub extern "C" fn jit_rt_new_list(_ctx: *mut VmContext, values_ptr: *const i64, count: i64) -> i64 {
    let count = count as usize;
    let mut list = Vec::with_capacity(count);
    for i in 0..count {
        let val_i64 = unsafe { *values_ptr.add(i) };
        list.push(nb_from_bits(val_i64));
    }
    wrap_nanbox_heap(HeapValue::List(Arc::new(list)))
}

/// Create a new Map value from an array of key-value pairs.
#[no_mangle]
pub extern "C" fn jit_rt_new_map(ctx: *mut VmContext, kvpairs_ptr: *const i64, count: i64) -> i64 {
    let count = count as usize;
    let mut map = BTreeMap::new();
    for i in 0..count {
        let key_i64 = unsafe { *kvpairs_ptr.add(i * 2) };
        let value_i64 = unsafe { *kvpairs_ptr.add(i * 2 + 1) };
        let key_nb = nb_from_bits(key_i64);
        let key_str = resolve_string(ctx, key_nb);
        let value = nb_from_bits(value_i64);
        map.insert(key_str, value);
    }
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::Map(Arc::new(map)))
}

/// Create a new Tuple value from an array of NbValue bits.
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
        elements.push(nb_from_bits(val_i64));
    }
    wrap_nanbox_heap(HeapValue::Tuple(Arc::new(elements)))
}

/// Get the length of a collection (List, Map, Set, Tuple, or String).
#[no_mangle]
pub extern "C" fn jit_rt_collection_len(_ctx: *mut VmContext, value_ptr: i64) -> i64 {
    let nb = nb_from_bits(value_ptr);
    match nb.as_heap_ref() {
        Some(HeapValue::List(l)) => l.len() as i64,
        Some(HeapValue::Map(m)) => m.len() as i64,
        Some(HeapValue::Set(s)) => s.len() as i64,
        Some(HeapValue::Tuple(t)) => t.len() as i64,
        Some(HeapValue::Str(s)) => s.chars().count() as i64,
        _ => 0,
    }
}

/// Create a new Set value from an array of NbValue bits.
#[no_mangle]
pub extern "C" fn jit_rt_new_set(_ctx: *mut VmContext, values_ptr: *const i64, count: i64) -> i64 {
    let count = count as usize;
    let mut set = BTreeSet::new();
    for i in 0..count {
        let val_i64 = unsafe { *values_ptr.add(i) };
        let value = nb_from_bits(val_i64);
        set.insert(value);
    }
    wrap_nanbox_heap(HeapValue::Set(Arc::new(set)))
}

/// Create a new Record value with the given type name and an empty field map.
#[no_mangle]
pub extern "C" fn jit_rt_new_record(
    ctx: *mut VmContext,
    type_name_ptr: *const u8,
    type_name_len: i64,
) -> i64 {
    let type_name = if type_name_ptr.is_null() || type_name_len == 0 {
        "Unknown".to_string()
    } else {
        let bytes = unsafe { std::slice::from_raw_parts(type_name_ptr, type_name_len as usize) };
        std::str::from_utf8(bytes).unwrap_or("Unknown").to_string()
    };

    let record = HeapValue::Record(Arc::new(RecordData {
        type_name: Arc::from(type_name.as_str()),
        fields: BTreeMap::new(),
    }));
    wrap_nanbox_heap_with_ctx(ctx, record)
}

// ---------------------------------------------------------------------------
// Higher-order helpers
// ---------------------------------------------------------------------------

/// Apply a closure to each element in a list, returning a new list.
#[no_mangle]
pub extern "C" fn jit_rt_hof_map(ctx: *mut VmContext, list_nb: i64, closure_nb: i64) -> i64 {
    let list_val = nb_from_bits(list_nb);
    let items = match list_val.as_heap_ref() {
        Some(HeapValue::List(list)) => list,
        _ => return wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(Vec::new()))),
    };
    let mut out = Vec::with_capacity(items.len());
    for elem in items.iter() {
        out.push(call_hof_unary(ctx, closure_nb, *elem));
    }
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(out)))
}

/// Filter list elements using a predicate closure.
#[no_mangle]
pub extern "C" fn jit_rt_hof_filter(ctx: *mut VmContext, list_nb: i64, closure_nb: i64) -> i64 {
    let list_val = nb_from_bits(list_nb);
    let items = match list_val.as_heap_ref() {
        Some(HeapValue::List(list)) => list,
        _ => return wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(Vec::new()))),
    };
    let mut out = Vec::new();
    for elem in items.iter() {
        let keep = call_hof_unary(ctx, closure_nb, *elem);
        if value_is_truthy(keep) {
            out.push(*elem);
        }
    }
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(out)))
}

/// Reduce a list using a binary closure and initial accumulator.
#[no_mangle]
pub extern "C" fn jit_rt_hof_reduce(
    ctx: *mut VmContext,
    list_nb: i64,
    closure_nb: i64,
    init_nb: i64,
) -> i64 {
    let list_val = nb_from_bits(list_nb);
    let items = match list_val.as_heap_ref() {
        Some(HeapValue::List(list)) => list,
        _ => return init_nb,
    };
    let mut acc = nb_from_bits(init_nb);
    for elem in items.iter() {
        acc = call_hof_binary(ctx, closure_nb, acc, *elem);
    }
    nb_to_bits(acc)
}

/// Map then flatten one level.
#[no_mangle]
pub extern "C" fn jit_rt_hof_flat_map(ctx: *mut VmContext, list_nb: i64, closure_nb: i64) -> i64 {
    let list_val = nb_from_bits(list_nb);
    let items = match list_val.as_heap_ref() {
        Some(HeapValue::List(list)) => list,
        _ => return wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(Vec::new()))),
    };
    let mut out = Vec::new();
    for elem in items.iter() {
        let mapped = call_hof_unary(ctx, closure_nb, *elem);
        match mapped.as_heap_ref() {
            Some(HeapValue::List(inner)) => out.extend(inner.iter().copied()),
            _ => out.push(mapped),
        }
    }
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(out)))
}

/// True if any element matches predicate.
#[no_mangle]
pub extern "C" fn jit_rt_hof_any(ctx: *mut VmContext, list_nb: i64, closure_nb: i64) -> i64 {
    let list_val = nb_from_bits(list_nb);
    let items = match list_val.as_heap_ref() {
        Some(HeapValue::List(list)) => list,
        _ => return NAN_BOX_FALSE_U as i64,
    };
    for elem in items.iter() {
        let pred = call_hof_unary(ctx, closure_nb, *elem);
        if value_is_truthy(pred) {
            return NAN_BOX_TRUE_U as i64;
        }
    }
    NAN_BOX_FALSE_U as i64
}

/// True if all elements match predicate.
#[no_mangle]
pub extern "C" fn jit_rt_hof_all(ctx: *mut VmContext, list_nb: i64, closure_nb: i64) -> i64 {
    let list_val = nb_from_bits(list_nb);
    let items = match list_val.as_heap_ref() {
        Some(HeapValue::List(list)) => list,
        _ => return NAN_BOX_FALSE_U as i64,
    };
    for elem in items.iter() {
        let pred = call_hof_unary(ctx, closure_nb, *elem);
        if !value_is_truthy(pred) {
            return NAN_BOX_FALSE_U as i64;
        }
    }
    NAN_BOX_TRUE_U as i64
}

/// Return the first element that matches predicate or null.
#[no_mangle]
pub extern "C" fn jit_rt_hof_find(ctx: *mut VmContext, list_nb: i64, closure_nb: i64) -> i64 {
    let list_val = nb_from_bits(list_nb);
    let items = match list_val.as_heap_ref() {
        Some(HeapValue::List(list)) => list,
        _ => return NAN_BOX_NULL_U as i64,
    };
    for elem in items.iter() {
        let pred = call_hof_unary(ctx, closure_nb, *elem);
        if value_is_truthy(pred) {
            return nb_to_bits(*elem);
        }
    }
    NAN_BOX_NULL_U as i64
}

/// Return index of first match, or -1.
#[no_mangle]
pub extern "C" fn jit_rt_hof_position(ctx: *mut VmContext, list_nb: i64, closure_nb: i64) -> i64 {
    let list_val = nb_from_bits(list_nb);
    let items = match list_val.as_heap_ref() {
        Some(HeapValue::List(list)) => list,
        _ => return nb_to_bits(NbValue::new_int(-1)),
    };
    for (idx, elem) in items.iter().enumerate() {
        let pred = call_hof_unary(ctx, closure_nb, *elem);
        if value_is_truthy(pred) {
            return nb_to_bits(NbValue::new_int(idx as i64));
        }
    }
    nb_to_bits(NbValue::new_int(-1))
}

/// Group elements by key function; keys are stringified.
#[no_mangle]
pub extern "C" fn jit_rt_hof_group_by(ctx: *mut VmContext, list_nb: i64, closure_nb: i64) -> i64 {
    let list_val = nb_from_bits(list_nb);
    let items = match list_val.as_heap_ref() {
        Some(HeapValue::List(list)) => list,
        _ => return wrap_nanbox_heap_with_ctx(ctx, HeapValue::Map(Arc::new(BTreeMap::new()))),
    };
    let mut groups: BTreeMap<String, Vec<NbValue>> = BTreeMap::new();
    for elem in items.iter() {
        let key_value = call_hof_unary(ctx, closure_nb, *elem);
        let key = resolve_string(ctx, key_value);
        groups.entry(key).or_default().push(*elem);
    }
    let mut map = BTreeMap::new();
    for (key, values) in groups {
        map.insert(key, NbValue::new_list(values));
    }
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::Map(Arc::new(map)))
}

/// Sort a list using comparator closure returning Int.
#[no_mangle]
pub extern "C" fn jit_rt_hof_sort_by(ctx: *mut VmContext, list_nb: i64, closure_nb: i64) -> i64 {
    let list_val = nb_from_bits(list_nb);
    let items = match list_val.as_heap_ref() {
        Some(HeapValue::List(list)) => list,
        _ => return wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(Vec::new()))),
    };
    let mut values: Vec<NbValue> = items.iter().copied().collect();
    values.sort_by(|lhs, rhs| {
        let cmp_val = call_hof_binary(ctx, closure_nb, *lhs, *rhs);
        if let Some(n) = cmp_val.as_int() {
            n.cmp(&0)
        } else if cmp_val.is_float() {
            let f = f64::from_bits(cmp_val.to_bits());
            f.total_cmp(&0.0)
        } else if let Some(b) = cmp_val.as_bool() {
            if b {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        } else {
            Ordering::Equal
        }
    });
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(values)))
}

// ---------------------------------------------------------------------------
// List operations
// ---------------------------------------------------------------------------

/// Append a value to a List.
#[no_mangle]
pub extern "C" fn jit_rt_list_append(ctx: *mut VmContext, list_ptr: i64, element: i64) -> i64 {
    append_to_list_value(ctx, list_ptr, nb_from_bits(element))
}

/// Append a raw i64 integer value to a List.
#[no_mangle]
pub extern "C" fn jit_rt_list_append_int(ctx: *mut VmContext, list_ptr: i64, element: i64) -> i64 {
    append_to_list_value(ctx, list_ptr, NbValue::new_int(element))
}

/// Append a raw f64-bit-pattern value to a List.
#[no_mangle]
pub extern "C" fn jit_rt_list_append_float(
    ctx: *mut VmContext,
    list_ptr: i64,
    element_bits: i64,
) -> i64 {
    let nb = NbValue::new_float(f64::from_bits(element_bits as u64));
    append_to_list_value(ctx, list_ptr, nb)
}

/// Create a list of integers for a range [start, end).
#[no_mangle]
pub extern "C" fn jit_rt_range(ctx: *mut VmContext, start_nb: i64, end_nb: i64) -> i64 {
    let start = nb_to_int(start_nb).unwrap_or(0);
    let end = nb_to_int(end_nb).unwrap_or(0);
    if end <= start {
        return wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(Vec::new())));
    }
    let mut list = Vec::with_capacity((end - start) as usize);
    for value in start..end {
        list.push(NbValue::new_int(value));
    }
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(list)))
}

/// Sort a list in natural ascending order.
#[no_mangle]
pub extern "C" fn jit_rt_sort(ctx: *mut VmContext, list_nb: i64) -> i64 {
    let u = list_nb as u64;
    if (u & NAN_MASK_U) == NAN_MASK_U && ((u >> 48) & 0x7) == 0 {
        let payload = u & PAYLOAD_MASK_U;
        if payload > 1 {
            let raw_ptr = (payload & !PTR_ARENA_FLAG_U) as *mut HeapValue;
            unsafe {
                if let HeapValue::List(list) = &mut *raw_ptr {
                    sort_list_fast(Arc::make_mut(list));
                }
            }
            return if (payload & PTR_ARENA_FLAG_U) != 0 {
                wrap_nanbox_ptr_arena(raw_ptr as *const HeapValue)
            } else {
                wrap_nanbox_ptr(raw_ptr as *const HeapValue)
            };
        }
    }

    let Some(raw_ptr) = decode_heap_ptr(list_nb) else {
        return list_nb;
    };

    let arena_flagged = nb_is_arena_ptr(list_nb);
    let is_arena_ptr = arena_flagged
        || if !ctx.is_null() {
            let arena = unsafe { (*ctx).arena };
            if arena.is_null() {
                false
            } else {
                unsafe { (*arena).contains_ptr(raw_ptr) }
            }
        } else {
            false
        };

    if is_arena_ptr {
        unsafe {
            if let HeapValue::List(list) = &mut *(raw_ptr as *mut HeapValue) {
                sort_list_fast(Arc::make_mut(list));
            }
        }
        return wrap_nanbox_ptr_arena(raw_ptr);
    }

    let mut value_arc = unsafe { Arc::from_raw(raw_ptr) };
    if let HeapValue::List(list) = Arc::make_mut(&mut value_arc) {
        sort_list_fast(Arc::make_mut(list));
    }

    wrap_nanbox_ptr(Arc::into_raw(value_arc))
}

/// Sort a list in natural descending order.
#[no_mangle]
pub extern "C" fn jit_rt_sort_desc(ctx: *mut VmContext, list_nb: i64) -> i64 {
    let Some(raw_ptr) = decode_heap_ptr(list_nb) else {
        return list_nb;
    };

    let arena_flagged = nb_is_arena_ptr(list_nb);
    let is_arena_ptr = arena_flagged
        || if !ctx.is_null() {
            let arena = unsafe { (*ctx).arena };
            if arena.is_null() {
                false
            } else {
                unsafe { (*arena).contains_ptr(raw_ptr) }
            }
        } else {
            false
        };

    if is_arena_ptr {
        unsafe {
            if let HeapValue::List(list) = &mut *(raw_ptr as *mut HeapValue) {
                let list = Arc::make_mut(list);
                list.sort_by(cmp_nb_value_natural);
                list.reverse();
            }
        }
        return wrap_nanbox_ptr_arena(raw_ptr);
    }

    let mut value_arc = unsafe { Arc::from_raw(raw_ptr) };
    if let HeapValue::List(list) = Arc::make_mut(&mut value_arc) {
        let list = Arc::make_mut(list);
        list.sort_by(cmp_nb_value_natural);
        list.reverse();
    }

    wrap_nanbox_ptr(Arc::into_raw(value_arc))
}

/// Reverse a List in-place (copy-on-write) and return the list.
#[no_mangle]
pub extern "C" fn jit_rt_list_reverse(ctx: *mut VmContext, list_nb: i64) -> i64 {
    let Some(raw_ptr) = decode_heap_ptr(list_nb) else {
        return list_nb;
    };

    let arena_flagged = nb_is_arena_ptr(list_nb);
    let is_arena_ptr = arena_flagged
        || if !ctx.is_null() {
            let arena = unsafe { (*ctx).arena };
            if arena.is_null() {
                false
            } else {
                unsafe { (*arena).contains_ptr(raw_ptr) }
            }
        } else {
            false
        };

    if is_arena_ptr {
        unsafe {
            if let HeapValue::List(list) = &mut *(raw_ptr as *mut HeapValue) {
                Arc::make_mut(list).reverse();
            }
        }
        return wrap_nanbox_ptr_arena(raw_ptr);
    }

    let mut value_arc = unsafe { Arc::from_raw(raw_ptr) };
    if let HeapValue::List(list) = Arc::make_mut(&mut value_arc) {
        Arc::make_mut(list).reverse();
    }

    wrap_nanbox_ptr(Arc::into_raw(value_arc))
}

/// Flatten a List by one level.
#[no_mangle]
pub extern "C" fn jit_rt_list_flatten(ctx: *mut VmContext, list_nb: i64) -> i64 {
    let Some(raw_ptr) = decode_heap_ptr(list_nb) else {
        return list_nb;
    };

    let arena_flagged = nb_is_arena_ptr(list_nb);
    let is_arena_ptr = arena_flagged
        || if !ctx.is_null() {
            let arena = unsafe { (*ctx).arena };
            if arena.is_null() {
                false
            } else {
                unsafe { (*arena).contains_ptr(raw_ptr) }
            }
        } else {
            false
        };

    let items: Vec<NbValue> = match unsafe { &*(raw_ptr as *const HeapValue) } {
        HeapValue::List(list) => list.iter().copied().collect(),
        _ => {
            return if is_arena_ptr {
                wrap_nanbox_ptr_arena(raw_ptr)
            } else {
                wrap_nanbox_ptr(raw_ptr)
            };
        }
    };

    let mut flat = Vec::new();
    for item in items {
        if let Some(HeapValue::List(inner)) = item.as_heap_ref() {
            flat.extend(inner.iter().copied());
        } else {
            flat.push(item);
        }
    }

    wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(flat)))
}

/// Remove duplicate elements from a List (first-occurrence order).
#[no_mangle]
pub extern "C" fn jit_rt_list_unique(ctx: *mut VmContext, list_nb: i64) -> i64 {
    let Some(raw_ptr) = decode_heap_ptr(list_nb) else {
        return list_nb;
    };

    let arena_flagged = nb_is_arena_ptr(list_nb);
    let is_arena_ptr = arena_flagged
        || if !ctx.is_null() {
            let arena = unsafe { (*ctx).arena };
            if arena.is_null() {
                false
            } else {
                unsafe { (*arena).contains_ptr(raw_ptr) }
            }
        } else {
            false
        };

    let items: Vec<NbValue> = match unsafe { &*(raw_ptr as *const HeapValue) } {
        HeapValue::List(list) => list.iter().copied().collect(),
        _ => {
            return if is_arena_ptr {
                wrap_nanbox_ptr_arena(raw_ptr)
            } else {
                wrap_nanbox_ptr(raw_ptr)
            };
        }
    };

    let mut seen: Vec<NbValue> = Vec::new();
    for item in items {
        if !seen.contains(&item) {
            seen.push(item);
        }
    }

    wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(seen)))
}

/// Take the first N elements from a List.
#[no_mangle]
pub extern "C" fn jit_rt_list_take(ctx: *mut VmContext, list_nb: i64, n_nb: i64) -> i64 {
    let n = nb_to_int(n_nb).unwrap_or(0) as usize;
    let Some(raw_ptr) = decode_heap_ptr(list_nb) else {
        return list_nb;
    };

    let arena_flagged = nb_is_arena_ptr(list_nb);
    let is_arena_ptr = arena_flagged
        || if !ctx.is_null() {
            let arena = unsafe { (*ctx).arena };
            if arena.is_null() {
                false
            } else {
                unsafe { (*arena).contains_ptr(raw_ptr) }
            }
        } else {
            false
        };

    let taken: Vec<NbValue> = match unsafe { &*(raw_ptr as *const HeapValue) } {
        HeapValue::List(list) => list.iter().take(n).copied().collect(),
        _ => {
            return if is_arena_ptr {
                wrap_nanbox_ptr_arena(raw_ptr)
            } else {
                wrap_nanbox_ptr(raw_ptr)
            };
        }
    };
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(taken)))
}

/// Drop the first N elements from a List.
#[no_mangle]
pub extern "C" fn jit_rt_list_drop(ctx: *mut VmContext, list_nb: i64, n_nb: i64) -> i64 {
    let n = nb_to_int(n_nb).unwrap_or(0) as usize;
    let Some(raw_ptr) = decode_heap_ptr(list_nb) else {
        return list_nb;
    };

    let arena_flagged = nb_is_arena_ptr(list_nb);
    let is_arena_ptr = arena_flagged
        || if !ctx.is_null() {
            let arena = unsafe { (*ctx).arena };
            if arena.is_null() {
                false
            } else {
                unsafe { (*arena).contains_ptr(raw_ptr) }
            }
        } else {
            false
        };

    let dropped: Vec<NbValue> = match unsafe { &*(raw_ptr as *const HeapValue) } {
        HeapValue::List(list) => list.iter().skip(n).copied().collect(),
        _ => {
            return if is_arena_ptr {
                wrap_nanbox_ptr_arena(raw_ptr)
            } else {
                wrap_nanbox_ptr(raw_ptr)
            };
        }
    };
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(dropped)))
}

/// Return the first element of a List or null.
#[no_mangle]
pub extern "C" fn jit_rt_list_first(_ctx: *mut VmContext, list_nb: i64) -> i64 {
    let list_val = nb_from_bits(list_nb);
    match list_val.as_heap_ref() {
        Some(HeapValue::List(list)) => list
            .first()
            .copied()
            .map(nb_to_bits)
            .unwrap_or(NAN_BOX_NULL_U as i64),
        _ => NAN_BOX_NULL_U as i64,
    }
}

/// Return the last element of a List or null.
#[no_mangle]
pub extern "C" fn jit_rt_list_last(_ctx: *mut VmContext, list_nb: i64) -> i64 {
    let list_val = nb_from_bits(list_nb);
    match list_val.as_heap_ref() {
        Some(HeapValue::List(list)) => list
            .last()
            .copied()
            .map(nb_to_bits)
            .unwrap_or(NAN_BOX_NULL_U as i64),
        _ => NAN_BOX_NULL_U as i64,
    }
}

// ---------------------------------------------------------------------------
// Set/collection ops
// ---------------------------------------------------------------------------

/// Convert a list to a set (removing duplicates).
#[no_mangle]
pub extern "C" fn jit_rt_to_set(ctx: *mut VmContext, list_nb: i64) -> i64 {
    let val = nb_from_bits(list_nb);
    let set: BTreeSet<NbValue> = match val.as_heap_ref() {
        Some(HeapValue::List(l)) => l.iter().copied().collect(),
        Some(HeapValue::Set(_)) => return list_nb,
        _ => BTreeSet::new(),
    };
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::Set(Arc::new(set)))
}

/// Add an element to a set (returns new set).
#[no_mangle]
pub extern "C" fn jit_rt_set_add(ctx: *mut VmContext, set_nb: i64, elem_nb: i64) -> i64 {
    let elem = nb_from_bits(elem_nb);
    let set_val = nb_from_bits(set_nb);
    let mut set: BTreeSet<NbValue> = match set_val.as_heap_ref() {
        Some(HeapValue::Set(s)) => (**s).clone(),
        Some(HeapValue::List(l)) => l.iter().copied().collect(),
        _ => BTreeSet::new(),
    };
    set.insert(elem);
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::Set(Arc::new(set)))
}

/// Split a string into a list of single-character strings.
#[no_mangle]
pub extern "C" fn jit_rt_chars(ctx: *mut VmContext, str_nb: i64) -> i64 {
    let val = nb_from_bits(str_nb);
    let s = resolve_string(ctx, val);
    let chars: Vec<NbValue> = s
        .chars()
        .map(|c| NbValue::new_str(&c.to_string()))
        .collect();
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(chars)))
}

/// Join a list of strings with a separator.
#[no_mangle]
pub extern "C" fn jit_rt_join(ctx: *mut VmContext, list_nb: i64, sep_nb: i64) -> i64 {
    let list_val = nb_from_bits(list_nb);
    let sep_val = nb_from_bits(sep_nb);
    let sep = resolve_string(ctx, sep_val);
    let items: Vec<String> = match list_val.as_heap_ref() {
        Some(HeapValue::List(l)) => l.iter().map(|v| resolve_string(ctx, *v)).collect(),
        _ => vec![],
    };
    let joined = items.join(&sep);
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::Str(Arc::from(joined.as_str())))
}

/// Zip two lists into a list of 2-tuples.
#[no_mangle]
pub extern "C" fn jit_rt_zip(ctx: *mut VmContext, a_nb: i64, b_nb: i64) -> i64 {
    let a_val = nb_from_bits(a_nb);
    let b_val = nb_from_bits(b_nb);
    match (a_val.as_heap_ref(), b_val.as_heap_ref()) {
        (Some(HeapValue::List(a_list)), Some(HeapValue::List(b_list))) => {
            let pairs: Vec<NbValue> = a_list
                .iter()
                .zip(b_list.iter())
                .map(|(a, b)| NbValue::new_tuple(vec![*a, *b]))
                .collect();
            wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(pairs)))
        }
        _ => wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(Vec::new()))),
    }
}

/// Enumerate a list into a list of (index, element) 2-tuples.
#[no_mangle]
pub extern "C" fn jit_rt_enumerate(ctx: *mut VmContext, list_nb: i64) -> i64 {
    let val = nb_from_bits(list_nb);
    let pairs: Vec<NbValue> = match val.as_heap_ref() {
        Some(HeapValue::List(l)) => l
            .iter()
            .enumerate()
            .map(|(i, v)| NbValue::new_tuple(vec![NbValue::new_int(i as i64), *v]))
            .collect(),
        _ => vec![],
    };
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(pairs)))
}

/// Split a list into fixed-size chunks.
#[no_mangle]
pub extern "C" fn jit_rt_chunk(ctx: *mut VmContext, list_nb: i64, n_nb: i64) -> i64 {
    let val = nb_from_bits(list_nb);
    let n = nb_to_int(n_nb).unwrap_or(1).max(1) as usize;
    let chunks: Vec<NbValue> = match val.as_heap_ref() {
        Some(HeapValue::List(l)) => l
            .chunks(n)
            .map(|chunk| NbValue::new_list(chunk.to_vec()))
            .collect(),
        _ => vec![],
    };
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(chunks)))
}

/// Produce a list of sliding windows of size n.
#[no_mangle]
pub extern "C" fn jit_rt_window(ctx: *mut VmContext, list_nb: i64, n_nb: i64) -> i64 {
    let val = nb_from_bits(list_nb);
    let n = nb_to_int(n_nb).unwrap_or(1).max(1) as usize;
    let windows: Vec<NbValue> = match val.as_heap_ref() {
        Some(HeapValue::List(l)) => {
            if n > l.len() {
                vec![]
            } else {
                l.windows(n)
                    .map(|w| NbValue::new_list(w.to_vec()))
                    .collect()
            }
        }
        _ => vec![],
    };
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(windows)))
}

// ---------------------------------------------------------------------------
// Map helpers
// ---------------------------------------------------------------------------

/// Merge two maps (or records) into one: `merge(a, b)` → new map with b's entries overlaid on a.
#[no_mangle]
pub extern "C" fn jit_rt_merge(ctx: *mut VmContext, a_ptr: i64, b_ptr: i64) -> i64 {
    let a_val = nb_from_bits(a_ptr);
    let b_val = nb_from_bits(b_ptr);

    let result = match (a_val.as_heap_ref(), b_val.as_heap_ref()) {
        (Some(HeapValue::Map(m1)), Some(HeapValue::Map(m2))) => {
            let mut merged = (**m1).clone();
            merged.extend(m2.iter().map(|(k, v)| (k.clone(), *v)));
            HeapValue::Map(Arc::new(merged))
        }
        (Some(HeapValue::Record(r1)), Some(HeapValue::Record(r2))) => {
            let mut fields = r1.fields.clone();
            for (k, v) in &r2.fields {
                fields.insert(k.clone(), *v);
            }
            HeapValue::Record(Arc::new(RecordData {
                type_name: r1.type_name.clone(),
                fields,
            }))
        }
        (Some(hv), _) => hv.clone(),
        _ => HeapValue::Map(Arc::new(BTreeMap::new())),
    };
    wrap_nanbox_heap_with_ctx(ctx, result)
}

/// Merge two maps/records, taking ownership of the first argument's Arc.
#[no_mangle]
pub extern "C" fn jit_rt_merge_take_a(ctx: *mut VmContext, a_nb: i64, b_nb: i64) -> i64 {
    let a_val = nb_from_bits(a_nb);
    let b_val = nb_from_bits(b_nb);

    if !a_val.is_heap_allocated() {
        return jit_rt_merge(ctx, a_nb, b_nb);
    }

    let arc = match a_val.as_heap_mut() {
        Some(arc) => arc,
        None => return jit_rt_merge(ctx, a_nb, b_nb),
    };

    let b_ref = b_val.as_heap_ref();
    match (arc.as_ref(), b_ref) {
        (HeapValue::Map(map_a), Some(HeapValue::Map(map_b))) => {
            let mut out = map_a.as_ref().clone();
            out.extend(map_b.iter().map(|(k, v)| (k.clone(), *v)));
            wrap_nanbox_heap_with_ctx(ctx, HeapValue::Map(Arc::new(out)))
        }
        (HeapValue::Record(rec_a), Some(HeapValue::Record(rec_b))) => {
            let mut fields = rec_a.fields.clone();
            fields.extend(rec_b.fields.iter().map(|(k, v)| (k.clone(), *v)));
            wrap_nanbox_heap_with_ctx(
                ctx,
                HeapValue::Record(Arc::new(RecordData {
                    type_name: rec_a.type_name.clone(),
                    fields,
                })),
            )
        }
        _ => jit_rt_merge(ctx, a_nb, b_nb),
    }
}

/// Return a list of keys from a map/record.
#[no_mangle]
pub extern "C" fn jit_rt_map_keys(ctx: *mut VmContext, map_nb: i64) -> i64 {
    let map_val = nb_from_bits(map_nb);
    let keys: Vec<NbValue> = match map_val.as_heap_ref() {
        Some(HeapValue::Map(m)) => m.keys().map(|k| NbValue::new_str(k)).collect(),
        Some(HeapValue::Record(r)) => r.fields.keys().map(|k| NbValue::new_str(k)).collect(),
        _ => Vec::new(),
    };
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(keys)))
}

/// Return a list of values from a map/record.
#[no_mangle]
pub extern "C" fn jit_rt_map_values(ctx: *mut VmContext, map_nb: i64) -> i64 {
    let map_val = nb_from_bits(map_nb);
    let values: Vec<NbValue> = match map_val.as_heap_ref() {
        Some(HeapValue::Map(m)) => m.values().copied().collect(),
        Some(HeapValue::Record(r)) => r.fields.values().copied().collect(),
        _ => Vec::new(),
    };
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(values)))
}

/// Return a list of (key, value) tuples from a map/record.
#[no_mangle]
pub extern "C" fn jit_rt_map_entries(ctx: *mut VmContext, map_nb: i64) -> i64 {
    let map_val = nb_from_bits(map_nb);
    let entries: Vec<NbValue> = match map_val.as_heap_ref() {
        Some(HeapValue::Map(m)) => m
            .iter()
            .map(|(k, v)| NbValue::new_tuple(vec![NbValue::new_str(k), *v]))
            .collect(),
        Some(HeapValue::Record(r)) => r
            .fields
            .iter()
            .map(|(k, v)| NbValue::new_tuple(vec![NbValue::new_str(k), *v]))
            .collect(),
        _ => Vec::new(),
    };
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(entries)))
}

/// Check if a map/record has a key.
#[no_mangle]
pub extern "C" fn jit_rt_map_has_key(ctx: *mut VmContext, map_nb: i64, key_nb: i64) -> i64 {
    let map_val = nb_from_bits(map_nb);
    let key_val = nb_from_bits(key_nb);
    let key = resolve_string(ctx, key_val);
    let has_key = match map_val.as_heap_ref() {
        Some(HeapValue::Map(m)) => m.contains_key(&key),
        Some(HeapValue::Record(r)) => r.fields.contains_key(&key),
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
    let map_val = nb_from_bits(map_nb);
    let key_val = nb_from_bits(key_nb);
    let key = resolve_string(ctx, key_val);

    let result = match map_val.as_heap_ref() {
        Some(HeapValue::Map(m)) => {
            let mut new_map = (**m).clone();
            new_map.remove(&key);
            HeapValue::Map(Arc::new(new_map))
        }
        Some(HeapValue::Record(r)) => {
            let mut fields = r.fields.clone();
            fields.remove(&key);
            HeapValue::Record(Arc::new(RecordData {
                type_name: r.type_name.clone(),
                fields,
            }))
        }
        Some(hv) => hv.clone(),
        None => HeapValue::Map(Arc::new(BTreeMap::new())),
    };

    wrap_nanbox_heap_with_ctx(ctx, result)
}

/// Return keys from a map in sorted order (BTreeMap order).
#[no_mangle]
pub extern "C" fn jit_rt_map_sorted_keys(ctx: *mut VmContext, map_nb: i64) -> i64 {
    let map_val = nb_from_bits(map_nb);
    let keys: Vec<NbValue> = match map_val.as_heap_ref() {
        Some(HeapValue::Map(m)) => m.keys().map(|k| NbValue::new_str(k)).collect(),
        _ => Vec::new(),
    };
    wrap_nanbox_heap_with_ctx(ctx, HeapValue::List(Arc::new(keys)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_result_arc(nb: i64) -> Arc<HeapValue> {
        let u = nb as u64;
        if (u & NAN_MASK_U) == NAN_MASK_U && ((u >> 48) & 0x7) == 0 {
            let payload = u & PAYLOAD_MASK_U;
            if payload > 1 && (payload & PTR_ARENA_FLAG_U) != 0 {
                let ptr = (payload & !PTR_ARENA_FLAG_U) as *const HeapValue;
                let value = unsafe { &*ptr };
                return Arc::new(value.clone());
            }
        }
        let ptr = decode_heap_ptr(nb).expect("expected encoded HeapValue pointer");
        unsafe { Arc::from_raw(ptr) }
    }

    fn decode_arc_from_nb(nb: i64) -> Arc<HeapValue> {
        let ptr = decode_heap_ptr(nb).expect("expected encoded HeapValue pointer");
        unsafe { Arc::from_raw(ptr) }
    }

    fn extract_list(value: &HeapValue) -> Vec<NbValue> {
        match value {
            HeapValue::List(list) => list.iter().copied().collect(),
            other => panic!("expected HeapValue::List, got {other:?}"),
        }
    }

    #[test]
    fn list_append_int_creates_list_from_null() {
        let out = jit_rt_list_append_int(std::ptr::null_mut(), NAN_BOX_NULL_U as i64, 42);
        let out_arc = decode_result_arc(out);
        let list = extract_list(&out_arc);
        assert_eq!(list, vec![NbValue::new_int(42)]);
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
        assert_eq!(list, vec![NbValue::new_float(1.25)]);
    }

    #[test]
    fn list_append_int_preserves_cow_on_shared_input() {
        let shared_src = Arc::new(HeapValue::List(Arc::new(vec![NbValue::new_int(1)])));
        let shared_observer = shared_src.clone();
        let input_nb = wrap_nanbox_ptr(Arc::into_raw(shared_src));

        let out = jit_rt_list_append_int(std::ptr::null_mut(), input_nb, 2);
        let out_arc = decode_result_arc(out);
        let out_list = extract_list(&out_arc);

        assert_eq!(out_list, vec![NbValue::new_int(1), NbValue::new_int(2)]);
        let observed_list = extract_list(&shared_observer);
        assert_eq!(observed_list, vec![NbValue::new_int(1)]);
    }

    #[test]
    fn range_builds_int_list() {
        let out = jit_rt_range(
            std::ptr::null_mut(),
            nb_to_bits(NbValue::new_int(1)),
            nb_to_bits(NbValue::new_int(4)),
        );
        let out_arc = decode_result_arc(out);
        let list = extract_list(&out_arc);
        assert_eq!(
            list,
            vec![
                NbValue::new_int(1),
                NbValue::new_int(2),
                NbValue::new_int(3)
            ]
        );
    }

    #[test]
    fn sort_orders_list_ascending() {
        let list = HeapValue::List(Arc::new(vec![
            NbValue::new_int(3),
            NbValue::new_int(1),
            NbValue::new_int(2),
        ]));
        let input_nb = wrap_nanbox_heap(list);
        let out = jit_rt_sort(std::ptr::null_mut(), input_nb);
        let out_arc = decode_result_arc(out);
        let list = extract_list(&out_arc);
        assert_eq!(
            list,
            vec![
                NbValue::new_int(1),
                NbValue::new_int(2),
                NbValue::new_int(3)
            ]
        );
    }

    #[test]
    fn sort_orders_list_descending() {
        let list = HeapValue::List(Arc::new(vec![
            NbValue::new_int(2),
            NbValue::new_int(3),
            NbValue::new_int(1),
        ]));
        let input_nb = wrap_nanbox_heap(list);
        let out = jit_rt_sort_desc(std::ptr::null_mut(), input_nb);
        let out_arc = decode_result_arc(out);
        let list = extract_list(&out_arc);
        assert_eq!(
            list,
            vec![
                NbValue::new_int(3),
                NbValue::new_int(2),
                NbValue::new_int(1)
            ]
        );
    }

    #[test]
    fn map_helpers_basic_flow() {
        let mut map = BTreeMap::new();
        map.insert("b".to_string(), NbValue::new_int(2));
        map.insert("a".to_string(), NbValue::new_int(1));
        let map_arc = Arc::new(map);

        let map_nb_keys = wrap_nanbox_heap(HeapValue::Map(Arc::clone(&map_arc)));
        let keys_nb = jit_rt_map_keys(std::ptr::null_mut(), map_nb_keys);
        let keys_arc = decode_result_arc(keys_nb);
        let keys_ref = keys_arc.as_ref();
        let keys = match keys_ref {
            HeapValue::List(list) => list.clone(),
            other => panic!("expected HeapValue::List, got {other:?}"),
        };
        assert_eq!(keys, vec![NbValue::new_str("a"), NbValue::new_str("b")]);

        let map_nb_values = wrap_nanbox_heap(HeapValue::Map(Arc::clone(&map_arc)));
        let values_nb = jit_rt_map_values(std::ptr::null_mut(), map_nb_values);
        let values_arc = decode_result_arc(values_nb);
        let values_ref = values_arc.as_ref();
        let values = match values_ref {
            HeapValue::List(list) => list.clone(),
            other => panic!("expected HeapValue::List, got {other:?}"),
        };
        assert_eq!(values, vec![NbValue::new_int(1), NbValue::new_int(2)]);

        let map_nb_entries = wrap_nanbox_heap(HeapValue::Map(Arc::clone(&map_arc)));
        let entries_nb = jit_rt_map_entries(std::ptr::null_mut(), map_nb_entries);
        let entries_arc = decode_result_arc(entries_nb);
        let entries_ref = entries_arc.as_ref();
        let entries = match entries_ref {
            HeapValue::List(list) => list.clone(),
            other => panic!("expected HeapValue::List, got {other:?}"),
        };
        assert_eq!(
            entries,
            vec![
                NbValue::new_tuple(vec![NbValue::new_str("a"), NbValue::new_int(1)]),
                NbValue::new_tuple(vec![NbValue::new_str("b"), NbValue::new_int(2)]),
            ]
        );

        let key_nb = wrap_nanbox_heap(HeapValue::Str(Arc::from("a")));
        let map_nb_has_key = wrap_nanbox_heap(HeapValue::Map(Arc::clone(&map_arc)));
        let has_key = jit_rt_map_has_key(std::ptr::null_mut(), map_nb_has_key, key_nb);
        assert_eq!(has_key, NAN_BOX_TRUE_U as i64);

        let missing_key = wrap_nanbox_heap(HeapValue::Str(Arc::from("z")));
        let map_nb_missing = wrap_nanbox_heap(HeapValue::Map(Arc::clone(&map_arc)));
        let missing = jit_rt_map_has_key(std::ptr::null_mut(), map_nb_missing, missing_key);
        assert_eq!(missing, NAN_BOX_FALSE_U as i64);

        let _ = decode_arc_from_nb(map_nb_keys);
        let _ = decode_arc_from_nb(map_nb_values);
        let _ = decode_arc_from_nb(map_nb_entries);
        let _ = decode_arc_from_nb(map_nb_has_key);
        let _ = decode_arc_from_nb(map_nb_missing);
    }

    #[test]
    fn merge_take_a_preserves_source_map() {
        let mut map_a = BTreeMap::new();
        map_a.insert("a".to_string(), NbValue::new_int(1));
        let mut map_b = BTreeMap::new();
        map_b.insert("b".to_string(), NbValue::new_int(2));

        let a_nb = wrap_nanbox_heap(HeapValue::Map(Arc::new(map_a)));
        let b_nb = wrap_nanbox_heap(HeapValue::Map(Arc::new(map_b)));

        let out = jit_rt_merge_take_a(std::ptr::null_mut(), a_nb, b_nb);
        let out_arc = decode_result_arc(out);
        let out_map = match out_arc.as_ref() {
            HeapValue::Map(m) => m,
            other => panic!("expected HeapValue::Map, got {other:?}"),
        };
        assert_eq!(out_map.get("a"), Some(&NbValue::new_int(1)));
        assert_eq!(out_map.get("b"), Some(&NbValue::new_int(2)));

        let original_map_owned = decode_arc_from_nb(a_nb);
        match original_map_owned.as_ref() {
            HeapValue::Map(m) => {
                assert_eq!(m.get("a"), Some(&NbValue::new_int(1)));
            }
            other => panic!("expected HeapValue::Map, got {other:?}"),
        }

        let _ = decode_arc_from_nb(b_nb);
    }
}
