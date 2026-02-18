//! JIT hot-path detection and in-process native code execution.
//!
//! Provides execution profiling to identify frequently-called cells and a
//! `JitEngine` that compiles LIR to native machine code via Cranelift's JIT
//! backend, then executes the compiled functions directly as native function
//! pointers.
//!
//! The engine observes call counts through `ExecutionProfile` and triggers
//! compilation once a cell crosses the configurable threshold. Compiled
//! functions are cached as callable function pointers — subsequent calls
//! bypass the interpreter entirely.

use cranelift_codegen::ir::{types, AbiParam, Type as ClifType};
use cranelift_codegen::Context;
use cranelift_frontend::FunctionBuilderContext;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use std::collections::HashMap;

use lumen_core::lir::{LirCell, LirModule, OpCode};

use crate::emit::CodegenError;
use crate::types::lir_type_str_to_cl_type;

// ---------------------------------------------------------------------------
// JitString — refcounted string with C-compatible layout
// ---------------------------------------------------------------------------

/// A reference-counted string with a C-compatible memory layout.
///
/// This replaces `Box<String>` for JIT string operations. The key advantage
/// is that cloning is O(1) (refcount increment) instead of O(n) (heap copy),
/// and length queries are O(1) field reads instead of function calls.
///
/// # Layout (32 bytes, all fields i64-aligned)
///
/// | Offset | Field    | Description                              |
/// |--------|----------|------------------------------------------|
/// | 0      | refcount | Reference count (i64, starts at 1)       |
/// | 8      | len      | Byte length of the string (i64)          |
/// | 16     | cap      | Capacity of the data buffer (i64)        |
/// | 24     | ptr      | Pointer to UTF-8 data buffer (*mut u8)   |
///
/// When refcount drops to 0, both the data buffer and the JitString struct
/// are freed.
#[repr(C)]
struct JitString {
    refcount: i64,
    len: i64,
    cap: i64,
    ptr: *mut u8,
}

impl JitString {
    /// Allocate a new JitString from raw UTF-8 bytes.
    fn from_bytes(data: &[u8]) -> *mut JitString {
        let len = data.len();
        // Allocate data buffer
        let data_ptr = if len > 0 {
            let mut buf = Vec::with_capacity(len);
            buf.extend_from_slice(data);
            let ptr = buf.as_mut_ptr();
            std::mem::forget(buf);
            ptr
        } else {
            std::ptr::null_mut()
        };

        let js = Box::new(JitString {
            refcount: 1,
            len: len as i64,
            cap: len as i64,
            ptr: data_ptr,
        });
        Box::into_raw(js)
    }

    /// Allocate a new empty JitString with a given capacity.
    fn with_capacity(cap: usize) -> *mut JitString {
        let data_ptr = if cap > 0 {
            let buf = Vec::<u8>::with_capacity(cap);
            let ptr = buf.as_ptr() as *mut u8;
            std::mem::forget(buf);
            ptr
        } else {
            std::ptr::null_mut()
        };

        let js = Box::new(JitString {
            refcount: 1,
            len: 0,
            cap: cap as i64,
            ptr: data_ptr,
        });
        Box::into_raw(js)
    }

    /// Get the string data as a byte slice.
    ///
    /// # Safety
    /// The JitString must be valid (non-null ptr if len > 0).
    unsafe fn as_bytes(&self) -> &[u8] {
        if self.len == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(self.ptr, self.len as usize)
        }
    }

    /// Get the string data as a &str.
    ///
    /// # Safety
    /// The data must be valid UTF-8.
    unsafe fn as_str(&self) -> &str {
        std::str::from_utf8_unchecked(self.as_bytes())
    }

    /// Increment refcount and return the same pointer.
    unsafe fn clone_ref(ptr: *mut JitString) -> *mut JitString {
        (*ptr).refcount += 1;
        ptr
    }

    /// Decrement refcount. If it reaches 0, free data buffer and struct.
    unsafe fn drop_ref(ptr: *mut JitString) {
        if ptr.is_null() {
            return;
        }
        (*ptr).refcount -= 1;
        if (*ptr).refcount <= 0 {
            // Free the data buffer
            let len = (*ptr).len as usize;
            let cap = (*ptr).cap as usize;
            if cap > 0 && !(*ptr).ptr.is_null() {
                // Reconstruct the Vec to let Rust free it properly
                drop(Vec::from_raw_parts((*ptr).ptr, len, cap));
            }
            // Free the JitString struct
            drop(Box::from_raw(ptr));
        }
    }
}

// ---------------------------------------------------------------------------
// String runtime helpers (extern "C" functions callable from JIT code)
// ---------------------------------------------------------------------------

/// Allocate a new JitString from a raw UTF-8 byte pointer and length.
/// Returns a `*mut JitString` as i64.
///
/// # Safety
/// `ptr` must point to valid UTF-8 bytes of at least `len` bytes.
extern "C" fn jit_rt_string_alloc(ptr: *const u8, len: usize) -> i64 {
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    JitString::from_bytes(bytes) as i64
}

/// Concatenate two JitStrings. Both inputs are `*mut JitString` as i64.
/// Returns a new `*mut JitString` as i64 owning the concatenated result.
/// The input strings are NOT freed (callers manage lifetimes).
///
/// # Safety
/// Both `a` and `b` must be valid `*mut JitString` pointers.
extern "C" fn jit_rt_string_concat(a: i64, b: i64) -> i64 {
    unsafe {
        let sa = &*(a as *const JitString);
        let sb = &*(b as *const JitString);
        let total = sa.len as usize + sb.len as usize;
        let new = JitString::with_capacity(total);
        if sa.len > 0 {
            std::ptr::copy_nonoverlapping(sa.ptr, (*new).ptr, sa.len as usize);
        }
        if sb.len > 0 {
            std::ptr::copy_nonoverlapping(sb.ptr, (*new).ptr.add(sa.len as usize), sb.len as usize);
        }
        (*new).len = total as i64;
        new as i64
    }
}

/// Concatenate two JitStrings with in-place optimization.
/// Takes ownership of `a`, appends `b` to it, and returns the result.
/// If `a` has refcount == 1 and sufficient capacity, appends in-place.
/// Otherwise allocates a new JitString.
///
/// # Safety
/// Both `a` and `b` must be valid `*mut JitString` pointers.
/// After this call, `a` is consumed and the returned pointer should be used instead.
extern "C" fn jit_rt_string_concat_mut(a: i64, b: i64) -> i64 {
    unsafe {
        let pa = a as *mut JitString;
        let sb = &*(b as *const JitString);
        let b_len = sb.len as usize;

        if b_len == 0 {
            return a;
        }

        // Can only mutate in-place if we have exclusive ownership
        if (*pa).refcount == 1 {
            let a_len = (*pa).len as usize;
            let a_cap = (*pa).cap as usize;
            let new_len = a_len + b_len;

            if new_len <= a_cap {
                // Fast path: append in-place, capacity suffices
                std::ptr::copy_nonoverlapping(sb.ptr, (*pa).ptr.add(a_len), b_len);
                (*pa).len = new_len as i64;
                return a;
            }

            // Need to grow: allocate new buffer with 2x growth
            let new_cap = new_len.max(a_cap * 2);
            let mut new_buf = Vec::<u8>::with_capacity(new_cap);
            if a_len > 0 {
                new_buf.extend_from_slice(std::slice::from_raw_parts((*pa).ptr, a_len));
            }
            new_buf.extend_from_slice(std::slice::from_raw_parts(sb.ptr, b_len));
            let new_ptr = new_buf.as_mut_ptr();
            std::mem::forget(new_buf);

            // Free old buffer
            if a_cap > 0 && !(*pa).ptr.is_null() {
                drop(Vec::from_raw_parts((*pa).ptr, a_len, a_cap));
            }

            (*pa).ptr = new_ptr;
            (*pa).len = new_len as i64;
            (*pa).cap = new_cap as i64;
            return a;
        }

        // Shared: must create a new JitString
        let sa = &*(a as *const JitString);
        let total = sa.len as usize + b_len;
        let new = JitString::with_capacity(total);
        if sa.len > 0 {
            std::ptr::copy_nonoverlapping(sa.ptr, (*new).ptr, sa.len as usize);
        }
        std::ptr::copy_nonoverlapping(sb.ptr, (*new).ptr.add(sa.len as usize), b_len);
        (*new).len = total as i64;
        // Drop old reference
        JitString::drop_ref(pa);
        new as i64
    }
}

/// Clone a JitString by incrementing its reference count.
/// Returns the SAME pointer (not a copy). O(1) operation.
///
/// # Safety
/// `s` must be a valid `*mut JitString` pointer.
extern "C" fn jit_rt_string_clone(s: i64) -> i64 {
    if s == 0 {
        return 0;
    }
    unsafe { JitString::clone_ref(s as *mut JitString) as i64 }
}

/// Compare two JitStrings for equality. Returns 1 if equal, 0 if not.
///
/// # Safety
/// Both `a` and `b` must be valid `*mut JitString` pointers.
extern "C" fn jit_rt_string_eq(a: i64, b: i64) -> i64 {
    // Fast path: same pointer means same string
    if a == b {
        return 1;
    }
    unsafe {
        let sa = &*(a as *const JitString);
        let sb = &*(b as *const JitString);
        // Quick length check before comparing bytes
        if sa.len != sb.len {
            return 0;
        }
        if sa.len == 0 {
            return 1;
        }
        let eq = std::slice::from_raw_parts(sa.ptr, sa.len as usize)
            == std::slice::from_raw_parts(sb.ptr, sb.len as usize);
        if eq {
            1
        } else {
            0
        }
    }
}

/// Compare two JitStrings, returning -1/0/1 for less/equal/greater.
///
/// # Safety
/// Both `a` and `b` must be valid `*mut JitString` pointers.
extern "C" fn jit_rt_string_cmp(a: i64, b: i64) -> i64 {
    if a == b {
        return 0;
    }
    unsafe {
        let sa = &*(a as *const JitString);
        let sb = &*(b as *const JitString);
        let a_bytes = sa.as_bytes();
        let b_bytes = sb.as_bytes();
        match a_bytes.cmp(b_bytes) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        }
    }
}

/// Free a JitString by decrementing its reference count.
/// When refcount reaches 0, frees the data buffer and the struct.
///
/// # Safety
/// `s` must be a valid `*mut JitString` pointer that was created by one of the
/// `jit_rt_string_*` functions.
extern "C" fn jit_rt_string_drop(s: i64) {
    if s != 0 {
        unsafe {
            JitString::drop_ref(s as *mut JitString);
        }
    }
}

/// Concatenate multiple JitStrings in a single allocation.
/// Takes a pointer to an array of `*mut JitString` as i64 values and a count.
/// Pre-computes the total length, allocates once, and copies all parts.
/// Returns a new `*mut JitString` as i64.
///
/// # Safety
/// `ptrs` must point to an array of `count` valid `i64` values, each being
/// a valid `*mut JitString` pointer (or 0 for null/empty).
extern "C" fn jit_rt_string_concat_multi(ptrs: *const i64, count: usize) -> i64 {
    if count == 0 {
        return JitString::from_bytes(&[]) as i64;
    }

    let slice = unsafe { std::slice::from_raw_parts(ptrs, count) };

    // First pass: compute total length.
    let mut total_len = 0usize;
    for &ptr in slice {
        if ptr != 0 {
            let s = unsafe { &*(ptr as *const JitString) };
            total_len += s.len as usize;
        }
    }

    // Single allocation with exact capacity.
    let new = JitString::with_capacity(total_len);

    // Second pass: copy all parts.
    unsafe {
        let mut offset = 0usize;
        for &ptr in slice {
            if ptr != 0 {
                let s = &*(ptr as *const JitString);
                let slen = s.len as usize;
                if slen > 0 {
                    std::ptr::copy_nonoverlapping(s.ptr, (*new).ptr.add(offset), slen);
                    offset += slen;
                }
            }
        }
        (*new).len = total_len as i64;
    }

    new as i64
}

/// Reconstruct a `String` from a JIT-returned raw pointer (JitString).
///
/// # Safety
/// `ptr` must be a valid `*mut JitString` pointer created by `jit_rt_string_alloc`,
/// `jit_rt_string_concat`, or `jit_rt_string_clone`. After this call the pointer
/// is consumed (refcount decremented) and must not be used again.
pub unsafe fn jit_take_string(ptr: i64) -> String {
    if ptr == 0 {
        String::new()
    } else {
        let js = &*(ptr as *const JitString);
        let result = js.as_str().to_string();
        JitString::drop_ref(ptr as *mut JitString);
        result
    }
}

/// Register all JIT string runtime helper symbols with a JITBuilder.
fn register_string_helpers(builder: &mut JITBuilder) {
    builder.symbol("jit_rt_string_alloc", jit_rt_string_alloc as *const u8);
    builder.symbol("jit_rt_string_concat", jit_rt_string_concat as *const u8);
    builder.symbol(
        "jit_rt_string_concat_mut",
        jit_rt_string_concat_mut as *const u8,
    );
    builder.symbol(
        "jit_rt_string_concat_multi",
        jit_rt_string_concat_multi as *const u8,
    );
    builder.symbol("jit_rt_string_clone", jit_rt_string_clone as *const u8);
    builder.symbol("jit_rt_string_eq", jit_rt_string_eq as *const u8);
    builder.symbol("jit_rt_string_cmp", jit_rt_string_cmp as *const u8);
    builder.symbol("jit_rt_string_drop", jit_rt_string_drop as *const u8);
}

// ---------------------------------------------------------------------------
// Record runtime helpers (extern "C" functions callable from JIT code)
// ---------------------------------------------------------------------------

use lumen_core::values::{RecordValue, Value};

/// Get a field from a Record by field name.
/// Returns a `*mut Value` as i64 (boxed Value).
/// If the record is null or the field doesn't exist, returns a boxed Value::Null.
///
/// # Safety
/// `record_ptr` must be a valid `*mut Value` pointer pointing to a `Value::Record`.
/// `field_name_ptr` must be a valid `*const u8` pointer to UTF-8 bytes.
extern "C" fn jit_rt_record_get_field(
    record_ptr: i64,
    field_name_ptr: *const u8,
    field_name_len: usize,
) -> i64 {
    if record_ptr == 0 {
        // Null record, return boxed null
        return Box::into_raw(Box::new(Value::Null)) as i64;
    }

    let value = unsafe { &*(record_ptr as *const Value) };
    let field_name = unsafe {
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(field_name_ptr, field_name_len))
    };

    let result = match value {
        Value::Record(r) => r.fields.get(field_name).cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    };

    Box::into_raw(Box::new(result)) as i64
}

/// Set a field in a Record by field name.
/// Creates a new Record with the updated field (copy-on-write).
/// Returns a `*mut Value` as i64 (boxed Value::Record).
///
/// # Safety
/// `record_ptr` must be a valid `*mut Value` pointer pointing to a `Value::Record`.
/// `field_name_ptr` must be a valid `*const u8` pointer to UTF-8 bytes.
/// `value_ptr` must be a valid `*mut Value` pointer.
extern "C" fn jit_rt_record_set_field(
    record_ptr: i64,
    field_name_ptr: *const u8,
    field_name_len: usize,
    value_ptr: i64,
) -> i64 {
    if record_ptr == 0 {
        // Can't set field on null, return null
        return Box::into_raw(Box::new(Value::Null)) as i64;
    }

    let value = unsafe { &*(record_ptr as *const Value) };
    let field_name = unsafe {
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(field_name_ptr, field_name_len))
    };
    let new_value = if value_ptr == 0 {
        Value::Null
    } else {
        unsafe { (*(value_ptr as *const Value)).clone() }
    };

    let result = match value {
        Value::Record(r) => {
            // Clone the record and update the field
            let mut new_fields = r.fields.clone();
            new_fields.insert(field_name.to_string(), new_value);
            Value::new_record(RecordValue {
                type_name: r.type_name.clone(),
                fields: new_fields,
            })
        }
        _ => Value::Null,
    };

    Box::into_raw(Box::new(result)) as i64
}

/// Clone a Value (for record field access results).
/// Returns a new `*mut Value` as i64.
///
/// # Safety
/// `value_ptr` must be a valid `*mut Value` pointer.
extern "C" fn jit_rt_value_clone(value_ptr: i64) -> i64 {
    if value_ptr == 0 {
        return 0;
    }
    let value = unsafe { &*(value_ptr as *const Value) };
    Box::into_raw(Box::new(value.clone())) as i64
}

/// Free a boxed Value.
///
/// # Safety
/// `value_ptr` must be a valid `*mut Value` pointer that was created by one of the
/// JIT runtime functions. Must not be called twice on the same pointer.
extern "C" fn jit_rt_value_drop(value_ptr: i64) {
    if value_ptr != 0 {
        unsafe {
            let _ = Box::from_raw(value_ptr as *mut Value);
        }
    }
}

/// Register all JIT record runtime helper symbols with a JITBuilder.
fn register_record_helpers(builder: &mut JITBuilder) {
    builder.symbol(
        "jit_rt_record_get_field",
        jit_rt_record_get_field as *const u8,
    );
    builder.symbol(
        "jit_rt_record_set_field",
        jit_rt_record_set_field as *const u8,
    );
    builder.symbol("jit_rt_value_clone", jit_rt_value_clone as *const u8);
    builder.symbol("jit_rt_value_drop", jit_rt_value_drop as *const u8);
}

// ---------------------------------------------------------------------------
// JIT Intrinsic Runtime Helpers
// ---------------------------------------------------------------------------

/// Print an integer to stdout (intrinsic #2: PRINT)
/// For JIT-compiled code, simplified to print just integers.
///
/// # Safety
/// None - operates on a simple i64 value.
extern "C" fn jit_rt_print_int(value: i64) {
    println!("{}", value);
}

/// Print a string to stdout (intrinsic #2: PRINT)
/// For JIT-compiled code, prints a single JitString argument.
///
/// # Safety
/// `s` must be a valid `*mut JitString` pointer.
extern "C" fn jit_rt_print_str(s: i64) {
    if s != 0 {
        let string = unsafe { &*(s as *const JitString) };
        let str_data = unsafe { string.as_str() };
        println!("{}", str_data);
    }
}

/// Get the length of a JitString (intrinsic #0: LENGTH)
/// Returns the number of Unicode characters (not bytes).
///
/// # Safety
/// `s` must be a valid `*mut JitString` pointer.
extern "C" fn jit_rt_string_len(s: i64) -> i64 {
    if s == 0 {
        return 0;
    }
    let string = unsafe { &*(s as *const JitString) };
    let str_data = unsafe { string.as_str() };
    str_data.chars().count() as i64
}

// ---------------------------------------------------------------------------
// Math runtime helpers (transcendental functions Cranelift doesn't support)
// ---------------------------------------------------------------------------

/// Sine of a float (radians).
extern "C" fn jit_rt_sin(value: f64) -> f64 {
    value.sin()
}

/// Cosine of a float (radians).
extern "C" fn jit_rt_cos(value: f64) -> f64 {
    value.cos()
}

/// Tangent of a float (radians).
extern "C" fn jit_rt_tan(value: f64) -> f64 {
    value.tan()
}

/// Natural logarithm.
extern "C" fn jit_rt_log(value: f64) -> f64 {
    value.ln()
}

/// Base-2 logarithm.
extern "C" fn jit_rt_log2(value: f64) -> f64 {
    value.log2()
}

/// Base-10 logarithm.
extern "C" fn jit_rt_log10(value: f64) -> f64 {
    value.log10()
}

/// Float modulo (a % b), matching Rust `f64::rem`.
extern "C" fn jit_rt_fmod(a: f64, b: f64) -> f64 {
    a % b
}

/// Power: base^exp.
extern "C" fn jit_rt_pow_float(base: f64, exp: f64) -> f64 {
    base.powf(exp)
}

/// Integer power: base^exp (returns i64).
extern "C" fn jit_rt_pow_int(base: i64, exp: i64) -> i64 {
    if exp < 0 {
        return 0; // integer power with negative exponent → 0 (truncation)
    }
    (base as i128).pow(exp as u32) as i64
}

/// Absolute value of a float.
extern "C" fn jit_rt_abs_float(value: f64) -> f64 {
    value.abs()
}

/// Absolute value of an integer.
extern "C" fn jit_rt_abs_int(value: i64) -> i64 {
    value.wrapping_abs()
}

// ---------------------------------------------------------------------------
// Conversion runtime helpers
// ---------------------------------------------------------------------------

/// Print a float to stdout.
extern "C" fn jit_rt_print_float(value: f64) {
    // Use Display formatting which omits trailing zeros like the interpreter
    println!("{}", value);
}

/// Print a bool to stdout (1=true, 0=false).
extern "C" fn jit_rt_print_bool(value: i64) {
    if value != 0 {
        println!("true");
    } else {
        println!("false");
    }
}

/// Convert an integer to a JitString. Returns `*mut JitString` as i64.
extern "C" fn jit_rt_to_string_int(value: i64) -> i64 {
    let s = value.to_string();
    JitString::from_bytes(s.as_bytes()) as i64
}

/// Convert a float to a JitString. Returns `*mut JitString` as i64.
extern "C" fn jit_rt_to_string_float(value: f64) -> i64 {
    let s = if value.fract() == 0.0 && value.is_finite() {
        format!("{:.1}", value) // e.g. "3.0"
    } else {
        format!("{}", value)
    };
    JitString::from_bytes(s.as_bytes()) as i64
}

/// Convert a float to an integer (truncate toward zero).
extern "C" fn jit_rt_to_int_from_float(value: f64) -> i64 {
    value as i64
}

/// Convert a JitString to an integer. Returns the parsed value, or 0 on failure.
extern "C" fn jit_rt_to_int_from_string(s: i64) -> i64 {
    if s == 0 {
        return 0;
    }
    let js = unsafe { &*(s as *const JitString) };
    let text = unsafe { js.as_str() };
    text.trim().parse::<i64>().unwrap_or(0)
}

/// Convert an integer to a float.
extern "C" fn jit_rt_to_float_from_int(value: i64) -> f64 {
    value as f64
}

/// Convert a JitString to a float. Returns the parsed value, or 0.0 on failure.
extern "C" fn jit_rt_to_float_from_string(s: i64) -> f64 {
    if s == 0 {
        return 0.0;
    }
    let js = unsafe { &*(s as *const JitString) };
    let text = unsafe { js.as_str() };
    text.trim().parse::<f64>().unwrap_or(0.0)
}

/// Register all JIT intrinsic runtime helper symbols with a JITBuilder.
fn register_intrinsic_helpers(builder: &mut JITBuilder) {
    // Print helpers
    builder.symbol("jit_rt_print_int", jit_rt_print_int as *const u8);
    builder.symbol("jit_rt_print_float", jit_rt_print_float as *const u8);
    builder.symbol("jit_rt_print_bool", jit_rt_print_bool as *const u8);
    builder.symbol("jit_rt_print_str", jit_rt_print_str as *const u8);

    // String intrinsics
    builder.symbol("jit_rt_string_len", jit_rt_string_len as *const u8);

    // Math helpers (transcendental functions Cranelift can't do natively)
    builder.symbol("jit_rt_sin", jit_rt_sin as *const u8);
    builder.symbol("jit_rt_cos", jit_rt_cos as *const u8);
    builder.symbol("jit_rt_tan", jit_rt_tan as *const u8);
    builder.symbol("jit_rt_log", jit_rt_log as *const u8);
    builder.symbol("jit_rt_log2", jit_rt_log2 as *const u8);
    builder.symbol("jit_rt_log10", jit_rt_log10 as *const u8);
    builder.symbol("jit_rt_pow_float", jit_rt_pow_float as *const u8);
    builder.symbol("jit_rt_pow_int", jit_rt_pow_int as *const u8);
    builder.symbol("jit_rt_fmod", jit_rt_fmod as *const u8);
    builder.symbol("jit_rt_abs_float", jit_rt_abs_float as *const u8);
    builder.symbol("jit_rt_abs_int", jit_rt_abs_int as *const u8);

    // Conversion helpers
    builder.symbol("jit_rt_to_string_int", jit_rt_to_string_int as *const u8);
    builder.symbol(
        "jit_rt_to_string_float",
        jit_rt_to_string_float as *const u8,
    );
    builder.symbol(
        "jit_rt_to_int_from_float",
        jit_rt_to_int_from_float as *const u8,
    );
    builder.symbol(
        "jit_rt_to_int_from_string",
        jit_rt_to_int_from_string as *const u8,
    );
    builder.symbol(
        "jit_rt_to_float_from_int",
        jit_rt_to_float_from_int as *const u8,
    );
    builder.symbol(
        "jit_rt_to_float_from_string",
        jit_rt_to_float_from_string as *const u8,
    );
}

// ---------------------------------------------------------------------------
// Execution profiling
// ---------------------------------------------------------------------------

/// Tracks how many times each cell has been called in the current session.
/// When a cell's call count crosses `threshold`, it is considered "hot"
/// and eligible for JIT compilation.
pub struct ExecutionProfile {
    call_counts: HashMap<String, u64>,
    threshold: u64,
}

impl ExecutionProfile {
    /// Create a new profile with the given hot-call threshold.
    pub fn new(threshold: u64) -> Self {
        Self {
            call_counts: HashMap::new(),
            threshold,
        }
    }

    /// Record a single call to `cell_name`. Returns the new count.
    pub fn record_call(&mut self, cell_name: &str) -> u64 {
        let count = self.call_counts.entry(cell_name.to_string()).or_insert(0);
        *count += 1;
        *count
    }

    /// Returns `true` if the cell's call count exceeds the threshold.
    pub fn is_hot(&self, cell_name: &str) -> bool {
        self.call_counts
            .get(cell_name)
            .map(|&c| c > self.threshold)
            .unwrap_or(false)
    }

    /// Return all cell names whose call count exceeds the threshold.
    pub fn hot_cells(&self) -> Vec<&str> {
        self.call_counts
            .iter()
            .filter(|(_, &c)| c > self.threshold)
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// Reset the counter for a specific cell (e.g. after JIT compilation).
    pub fn reset(&mut self, cell_name: &str) {
        self.call_counts.remove(cell_name);
    }

    /// Get the current call count for a cell.
    pub fn call_count(&self, cell_name: &str) -> u64 {
        self.call_counts.get(cell_name).copied().unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Optimisation level
// ---------------------------------------------------------------------------

/// Optimisation level for JIT compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptLevel {
    /// No optimisation (fastest compile, slowest code).
    None,
    /// Optimise for execution speed.
    Speed,
    /// Optimise for both speed and code size.
    SpeedAndSize,
}

// ---------------------------------------------------------------------------
// Codegen settings
// ---------------------------------------------------------------------------

/// Settings controlling how the JIT engine compiles cells.
pub struct CodegenSettings {
    pub opt_level: OptLevel,
    /// Optional target triple (e.g. `"x86_64-unknown-linux-gnu"`).
    /// If `None`, the host platform is used.
    pub target: Option<String>,
}

impl Default for CodegenSettings {
    fn default() -> Self {
        Self {
            opt_level: OptLevel::Speed,
            target: None,
        }
    }
}

// ---------------------------------------------------------------------------
// JIT statistics
// ---------------------------------------------------------------------------

/// Aggregated statistics about JIT compilation activity.
#[derive(Debug, Clone, Default)]
pub struct JitStats {
    /// Number of cells compiled so far.
    pub cells_compiled: u64,
    /// Number of times a pre-compiled cell was served from cache.
    pub cache_hits: u64,
    /// Number of cache entries currently stored.
    pub cache_size: usize,
    /// Number of JIT executions performed.
    pub executions: u64,
}

// ---------------------------------------------------------------------------
// JIT Error
// ---------------------------------------------------------------------------

/// Errors specific to JIT compilation and execution.
#[derive(Debug)]
pub enum JitError {
    /// Compilation failed.
    CompileError(CodegenError),
    /// The requested cell was not found in the module.
    CellNotFound(String),
    /// JIT module creation failed.
    ModuleError(String),
}

impl std::fmt::Display for JitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JitError::CompileError(e) => write!(f, "JIT compile error: {e}"),
            JitError::CellNotFound(name) => write!(f, "cell not found: {name}"),
            JitError::ModuleError(msg) => write!(f, "JIT module error: {msg}"),
        }
    }
}

impl std::error::Error for JitError {}

impl From<CodegenError> for JitError {
    fn from(e: CodegenError) -> Self {
        JitError::CompileError(e)
    }
}

// ---------------------------------------------------------------------------
// Cached compiled function
// ---------------------------------------------------------------------------

/// Metadata for a JIT-compiled function.
struct CompiledFunction {
    /// Raw function pointer to the compiled native code.
    fn_ptr: *const u8,
    /// Number of parameters the function expects.
    param_count: usize,
    /// True if the function returns a heap-allocated string pointer.
    returns_string: bool,
}

// Safety: The function pointers are valid for the lifetime of the JITModule
// that produced them. We ensure the JITModule lives as long as the JitEngine.
unsafe impl Send for CompiledFunction {}

// ---------------------------------------------------------------------------
// JIT Engine
// ---------------------------------------------------------------------------

/// Manages JIT-compiled function caching and on-demand compilation with
/// real in-process native code execution.
///
/// Typical lifecycle:
/// 1. Interpreter calls `record_and_check("cell_name")` on every cell entry.
/// 2. When the function returns `true` (just became hot), the runtime calls
///    `compile_hot("cell_name", &module)` to compile it.
/// 3. Subsequent invocations call `execute_jit("cell_name", &args)` to run
///    the native code directly, bypassing the interpreter.
pub struct JitEngine {
    profile: ExecutionProfile,
    /// The Cranelift JIT module. Owns the compiled code memory.
    jit_module: Option<JITModule>,
    /// Cached compiled function pointers keyed by cell name.
    cache: HashMap<String, CompiledFunction>,
    /// Settings for on-demand compilation.
    #[allow(dead_code)]
    codegen_settings: CodegenSettings,
    /// Compilation statistics.
    stats: JitStats,
    /// Retained optimized cells whose string constant data is referenced by
    /// raw pointers baked into the JIT machine code. Must live as long as
    /// `jit_module`.
    _retained_cells: Vec<LirCell>,
}

impl JitEngine {
    /// Create a new JIT engine. The `threshold` is forwarded to the internal
    /// `ExecutionProfile`.
    pub fn new(settings: CodegenSettings, threshold: u64) -> Self {
        Self {
            profile: ExecutionProfile::new(threshold),
            jit_module: None,
            cache: HashMap::new(),
            codegen_settings: settings,
            stats: JitStats::default(),
            _retained_cells: Vec::new(),
        }
    }

    /// Record a call to `cell_name` and return `true` if the cell *just*
    /// crossed the hot threshold (i.e., it was not hot before this call
    /// but now is). This is the trigger for the runtime to schedule JIT
    /// compilation.
    pub fn record_and_check(&mut self, cell_name: &str) -> bool {
        let was_hot = self.profile.is_hot(cell_name);
        self.profile.record_call(cell_name);
        !was_hot && self.profile.is_hot(cell_name)
    }

    /// Compile all cells from the given `LirModule` via Cranelift JIT.
    /// Compiled function pointers are stored in the cache.
    ///
    /// If a cell is already cached, the cache entry is preserved (with a
    /// cache-hit bump).
    pub fn compile_module(&mut self, module: &LirModule) -> Result<(), JitError> {
        // Create a new JIT module for this compilation batch.
        // Enable Cranelift's `speed` optimization level so the generated
        // native code is competitive with ahead-of-time compilers. Without
        // this, Cranelift defaults to `none` (no optimizations), resulting
        // in 20-50x slower code for compute-heavy workloads like fibonacci.
        let cl_opt = match self.codegen_settings.opt_level {
            OptLevel::None => "none",
            OptLevel::Speed => "speed",
            OptLevel::SpeedAndSize => "speed_and_size",
        };
        let mut builder = JITBuilder::with_flags(
            &[("opt_level", cl_opt)],
            cranelift_module::default_libcall_names(),
        )
        .map_err(|e| JitError::ModuleError(format!("JITBuilder creation failed: {e}")))?;

        // Register string runtime helper symbols so JIT code can call them.
        register_string_helpers(&mut builder);

        // Register record runtime helper symbols so JIT code can call them.
        register_record_helpers(&mut builder);

        // Register intrinsic runtime helper symbols so JIT code can call builtins.
        register_intrinsic_helpers(&mut builder);

        let mut jit_module = JITModule::new(builder);
        let pointer_type = jit_module.isa().pointer_type();

        // Lower all cells into the JIT module.
        let lowered = lower_module_jit(&mut jit_module, module, pointer_type)?;

        // Finalize all definitions so we can retrieve function pointers.
        jit_module
            .finalize_definitions()
            .map_err(|e| JitError::ModuleError(format!("finalize_definitions failed: {e}")))?;

        // Retrieve and cache function pointers.
        for func in &lowered.functions {
            let fn_ptr = jit_module.get_finalized_function(func.func_id);
            self.cache.insert(
                func.name.clone(),
                CompiledFunction {
                    fn_ptr,
                    param_count: func.param_count,
                    returns_string: func.returns_string,
                },
            );
            self.stats.cells_compiled += 1;
        }
        self.stats.cache_size = self.cache.len();

        // Store the JIT module so its memory stays alive.
        self.jit_module = Some(jit_module);

        // Retain optimized cells so string constant pointers stay valid.
        self._retained_cells = lowered._retained_cells;

        Ok(())
    }

    /// Compile a single cell from the given `LirModule` to native code via
    /// Cranelift JIT. The compiled function pointer is stored in the cache.
    ///
    /// If the cell is already cached, returns Ok immediately (with a
    /// cache-hit bump).
    pub fn compile_hot(&mut self, cell_name: &str, module: &LirModule) -> Result<(), JitError> {
        // Return early if already cached.
        if self.cache.contains_key(cell_name) {
            self.stats.cache_hits += 1;
            return Ok(());
        }

        // Compile the entire module (all cells) since cross-cell calls need
        // all functions present.
        self.compile_module(module)?;

        if !self.cache.contains_key(cell_name) {
            return Err(JitError::CellNotFound(cell_name.to_string()));
        }

        // Reset the profile counter so we don't re-trigger immediately.
        self.profile.reset(cell_name);

        Ok(())
    }

    /// Execute a JIT-compiled function with no arguments.
    /// Returns the i64 result.
    ///
    /// # Safety
    /// The caller must ensure that the function was compiled with the
    /// correct signature (no params, returns i64).
    pub fn execute_jit_nullary(&mut self, cell_name: &str) -> Result<i64, JitError> {
        let compiled = self
            .cache
            .get(cell_name)
            .ok_or_else(|| JitError::CellNotFound(cell_name.to_string()))?;

        let fn_ptr = compiled.fn_ptr;
        self.stats.executions += 1;

        // SAFETY: The function pointer was produced by Cranelift JIT and is
        // valid for the lifetime of the JITModule (which we own). The
        // caller guarantees the signature matches.
        let result = unsafe {
            let code_fn: fn() -> i64 = std::mem::transmute(fn_ptr);
            code_fn()
        };
        Ok(result)
    }

    /// Execute a JIT-compiled function with one i64 argument.
    /// Returns the i64 result.
    pub fn execute_jit_unary(&mut self, cell_name: &str, arg: i64) -> Result<i64, JitError> {
        let compiled = self
            .cache
            .get(cell_name)
            .ok_or_else(|| JitError::CellNotFound(cell_name.to_string()))?;

        let fn_ptr = compiled.fn_ptr;
        self.stats.executions += 1;

        let result = unsafe {
            let code_fn: fn(i64) -> i64 = std::mem::transmute(fn_ptr);
            code_fn(arg)
        };
        Ok(result)
    }

    /// Execute a JIT-compiled function with two i64 arguments.
    /// Returns the i64 result.
    pub fn execute_jit_binary(
        &mut self,
        cell_name: &str,
        arg1: i64,
        arg2: i64,
    ) -> Result<i64, JitError> {
        let compiled = self
            .cache
            .get(cell_name)
            .ok_or_else(|| JitError::CellNotFound(cell_name.to_string()))?;

        let fn_ptr = compiled.fn_ptr;
        self.stats.executions += 1;

        let result = unsafe {
            let code_fn: fn(i64, i64) -> i64 = std::mem::transmute(fn_ptr);
            code_fn(arg1, arg2)
        };
        Ok(result)
    }

    /// Execute a JIT-compiled function with three i64 arguments.
    /// Returns the i64 result.
    pub fn execute_jit_ternary(
        &mut self,
        cell_name: &str,
        arg1: i64,
        arg2: i64,
        arg3: i64,
    ) -> Result<i64, JitError> {
        let compiled = self
            .cache
            .get(cell_name)
            .ok_or_else(|| JitError::CellNotFound(cell_name.to_string()))?;

        let fn_ptr = compiled.fn_ptr;
        self.stats.executions += 1;

        let result = unsafe {
            let code_fn: fn(i64, i64, i64) -> i64 = std::mem::transmute(fn_ptr);
            code_fn(arg1, arg2, arg3)
        };
        Ok(result)
    }

    /// Generic JIT execution dispatching on arity. Supports 0..=3 i64
    /// arguments.
    pub fn execute_jit(&mut self, cell_name: &str, args: &[i64]) -> Result<i64, JitError> {
        match args.len() {
            0 => self.execute_jit_nullary(cell_name),
            1 => self.execute_jit_unary(cell_name, args[0]),
            2 => self.execute_jit_binary(cell_name, args[0], args[1]),
            3 => self.execute_jit_ternary(cell_name, args[0], args[1], args[2]),
            n => Err(JitError::ModuleError(format!(
                "unsupported arity {n} for JIT execution (max 3)"
            ))),
        }
    }

    /// Compile a cell if not already compiled, then execute it.
    /// Convenience method that combines `compile_hot` and `execute_jit`.
    pub fn compile_and_execute(
        &mut self,
        cell_name: &str,
        module: &LirModule,
        args: &[i64],
    ) -> Result<i64, JitError> {
        self.compile_hot(cell_name, module)?;
        self.execute_jit(cell_name, args)
    }

    /// Remove a cached cell (e.g. when source code changes).
    pub fn invalidate(&mut self, cell_name: &str) {
        self.cache.remove(cell_name);
        self.stats.cache_size = self.cache.len();
    }

    /// Return a snapshot of JIT statistics.
    pub fn stats(&self) -> JitStats {
        self.stats.clone()
    }

    /// Expose the internal execution profile (read-only).
    pub fn profile(&self) -> &ExecutionProfile {
        &self.profile
    }

    /// Check if a cell has been compiled and cached.
    pub fn is_compiled(&self, cell_name: &str) -> bool {
        self.cache.contains_key(cell_name)
    }

    /// Get the number of parameters for a compiled cell.
    pub fn compiled_param_count(&self, cell_name: &str) -> Option<usize> {
        self.cache.get(cell_name).map(|c| c.param_count)
    }

    /// Check if a compiled cell returns a heap-allocated string pointer.
    pub fn returns_string(&self, cell_name: &str) -> bool {
        self.cache
            .get(cell_name)
            .map(|c| c.returns_string)
            .unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// Pre-scan: check if a cell only uses JIT-supported opcodes
// ---------------------------------------------------------------------------

/// Returns `true` if every instruction in the cell uses an opcode the JIT can
/// compile. Cells containing unsupported opcodes (e.g. ToolCall,
/// NewList, etc.) are filtered out before compilation so we never emit traps
/// for unsupported operations.
fn is_cell_jit_compilable(cell: &LirCell) -> bool {
    cell.instructions.iter().all(|instr| {
        matches!(
            instr.op,
            OpCode::LoadK
                | OpCode::LoadBool
                | OpCode::LoadInt
                | OpCode::LoadNil
                | OpCode::Move
                | OpCode::MoveOwn
                | OpCode::Add
                | OpCode::Sub
                | OpCode::Mul
                | OpCode::Div
                | OpCode::Mod
                | OpCode::Neg
                | OpCode::FloorDiv
                | OpCode::Pow
                | OpCode::Eq
                | OpCode::Lt
                | OpCode::Le
                | OpCode::Not
                | OpCode::And
                | OpCode::Or
                | OpCode::Test
                | OpCode::Jmp
                | OpCode::Break
                | OpCode::Continue
                | OpCode::Return
                | OpCode::Halt
                | OpCode::Call
                | OpCode::TailCall
                | OpCode::Intrinsic
                | OpCode::Nop
                | OpCode::Loop
                | OpCode::ForPrep
                | OpCode::ForLoop
                | OpCode::ForIn
                | OpCode::BitOr
                | OpCode::BitAnd
                | OpCode::BitXor
                | OpCode::BitNot
                | OpCode::Shl
                | OpCode::Shr
                | OpCode::Concat
                | OpCode::GetField
                | OpCode::SetField
        )
    })
}

// ---------------------------------------------------------------------------
// JIT-specific lowering (mirrors lower.rs but targets JITModule)
// ---------------------------------------------------------------------------

/// Result of lowering an entire LIR module into the JIT.
struct JitLoweredModule {
    functions: Vec<JitLoweredFunction>,
    /// Retain optimized cells so that string constant data (whose raw pointers
    /// are baked into the generated machine code as immediates for
    /// `jit_rt_string_alloc` calls) stays alive for the lifetime of the JIT
    /// code.
    _retained_cells: Vec<LirCell>,
}

struct JitLoweredFunction {
    name: String,
    func_id: FuncId,
    param_count: usize,
    returns_string: bool,
}

/// Lower an entire LIR module into Cranelift IR inside the given `JITModule`.
/// Cells containing unsupported opcodes are silently skipped — they will
/// remain interpreted.
fn lower_module_jit(
    module: &mut JITModule,
    lir: &LirModule,
    pointer_type: ClifType,
) -> Result<JitLoweredModule, CodegenError> {
    let mut fb_ctx = FunctionBuilderContext::new();

    // Filter to only JIT-compilable cells.
    let compilable_cells: Vec<&LirCell> = lir
        .cells
        .iter()
        .filter(|c| is_cell_jit_compilable(c))
        .collect();

    if compilable_cells.is_empty() {
        return Ok(JitLoweredModule {
            functions: Vec::new(),
            _retained_cells: Vec::new(),
        });
    }

    // First pass: declare all compilable cell signatures.
    let mut func_ids: HashMap<String, FuncId> = HashMap::new();
    for cell in &compilable_cells {
        let mut sig = module.make_signature();
        for param in &cell.params {
            let param_ty = lir_type_str_to_cl_type(&param.ty, pointer_type);
            // Cranelift ABI requires I8 to be extended; use I64 for Bool params.
            let abi_ty = if param_ty == types::I8 {
                types::I64
            } else {
                param_ty
            };
            sig.params.push(AbiParam::new(abi_ty));
        }
        // ABI always returns I64. Float results are bitcast to I64 by the
        // callee's Return handler so that execute_jit_nullary (which
        // transmutes the fn ptr to `fn() -> i64`) works uniformly.
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function(&cell.name, Linkage::Export, &sig)
            .map_err(|e| {
                CodegenError::LoweringError(format!("declare_function({}): {e}", cell.name))
            })?;
        func_ids.insert(cell.name.clone(), func_id);
    }

    // Second pass: lower each cell body.
    // We collect optimized cells so their constant string data (whose raw
    // pointers are baked into the machine code) stays alive as long as the
    // JIT module.
    let mut retained_cells: Vec<LirCell> = Vec::with_capacity(compilable_cells.len());
    let mut lowered = JitLoweredModule {
        functions: Vec::with_capacity(compilable_cells.len()),
        _retained_cells: Vec::new(), // filled after the loop
    };

    // Build a map of cell name → return type for cross-cell call type inference.
    let cell_return_types: HashMap<String, crate::ir::JitVarType> = compilable_cells
        .iter()
        .map(|c| {
            let ret_ty = c
                .returns
                .as_deref()
                .map(crate::ir::JitVarType::from_lir_return_type)
                .unwrap_or(crate::ir::JitVarType::Int);
            (c.name.clone(), ret_ty)
        })
        .collect();

    for cell in &compilable_cells {
        let func_id = func_ids[&cell.name];
        let mut ctx = Context::new();

        // Optimize before lowering to IR
        let mut optimized_cell = (*cell).clone();
        crate::opt::optimize(&mut optimized_cell);

        lower_cell_jit(
            &mut ctx,
            module,
            &optimized_cell,
            &mut fb_ctx,
            pointer_type,
            func_id,
            &func_ids,
            &lir.strings,
            &cell_return_types,
        )?;

        // Keep the cell alive so string constant pointers remain valid.
        retained_cells.push(optimized_cell);

        let ret_is_string = cell
            .returns
            .as_deref()
            .map(|s| s == "String")
            .unwrap_or(false);
        lowered.functions.push(JitLoweredFunction {
            name: cell.name.clone(),
            func_id,
            param_count: cell.params.len(),
            returns_string: ret_is_string,
        });
    }

    lowered._retained_cells = retained_cells;
    Ok(lowered)
}

// ---------------------------------------------------------------------------
// Per-cell lowering (JIT variant)
// ---------------------------------------------------------------------------

fn lower_cell_jit(
    ctx: &mut Context,
    module: &mut JITModule,
    cell: &LirCell,
    fb_ctx: &mut FunctionBuilderContext,
    pointer_type: ClifType,
    func_id: FuncId,
    func_ids: &HashMap<String, FuncId>,
    string_table: &[String],
    cell_return_types: &HashMap<String, crate::ir::JitVarType>,
) -> Result<(), CodegenError> {
    // Delegate to the unified IR builder
    crate::ir::lower_cell(
        ctx,
        fb_ctx,
        cell,
        module,
        pointer_type,
        func_id,
        func_ids,
        string_table,
        cell_return_types,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, target_arch = "x86_64"))]
mod tests {
    use super::*;
    use lumen_core::lir::{Constant, Instruction, LirCell, LirModule, LirParam, OpCode};

    fn simple_lir_module() -> LirModule {
        LirModule {
            version: "1.0.0".to_string(),
            doc_hash: "test".to_string(),
            strings: Vec::new(),
            types: Vec::new(),
            cells: vec![LirCell {
                name: "answer".to_string(),
                params: Vec::new(),
                returns: Some("Int".to_string()),
                registers: 2,
                constants: vec![Constant::Int(42)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
                effect_handler_metas: Vec::new(),
            }],
            tools: Vec::new(),
            policies: Vec::new(),
            agents: Vec::new(),
            addons: Vec::new(),
            effects: Vec::new(),
            effect_binds: Vec::new(),
            handlers: Vec::new(),
        }
    }

    fn make_module_with_cells(cells: Vec<LirCell>) -> LirModule {
        LirModule {
            version: "1.0.0".to_string(),
            doc_hash: "test".to_string(),
            strings: Vec::new(),
            types: Vec::new(),
            cells,
            tools: Vec::new(),
            policies: Vec::new(),
            agents: Vec::new(),
            addons: Vec::new(),
            effects: Vec::new(),
            effect_binds: Vec::new(),
            handlers: Vec::new(),
        }
    }

    // --- ExecutionProfile tests -------------------------------------------

    #[test]
    fn profile_starts_empty() {
        let profile = ExecutionProfile::new(100);
        assert_eq!(profile.call_count("foo"), 0);
        assert!(!profile.is_hot("foo"));
        assert!(profile.hot_cells().is_empty());
    }

    #[test]
    fn profile_record_increments() {
        let mut profile = ExecutionProfile::new(3);
        assert_eq!(profile.record_call("foo"), 1);
        assert_eq!(profile.record_call("foo"), 2);
        assert_eq!(profile.record_call("bar"), 1);
        assert_eq!(profile.call_count("foo"), 2);
        assert_eq!(profile.call_count("bar"), 1);
    }

    #[test]
    fn profile_hot_threshold() {
        let mut profile = ExecutionProfile::new(3);
        for _ in 0..3 {
            profile.record_call("fn_a");
        }
        assert!(!profile.is_hot("fn_a"));

        profile.record_call("fn_a");
        assert!(profile.is_hot("fn_a"));
        assert!(!profile.is_hot("fn_b"));
    }

    #[test]
    fn profile_hot_cells() {
        let mut profile = ExecutionProfile::new(2);
        for _ in 0..5 {
            profile.record_call("alpha");
        }
        for _ in 0..3 {
            profile.record_call("beta");
        }
        profile.record_call("gamma");

        let mut hot = profile.hot_cells();
        hot.sort();
        assert_eq!(hot, vec!["alpha", "beta"]);
    }

    #[test]
    fn profile_reset() {
        let mut profile = ExecutionProfile::new(2);
        for _ in 0..5 {
            profile.record_call("fn_a");
        }
        assert!(profile.is_hot("fn_a"));

        profile.reset("fn_a");
        assert!(!profile.is_hot("fn_a"));
        assert_eq!(profile.call_count("fn_a"), 0);
    }

    // --- JitEngine record_and_check tests ---------------------------------

    #[test]
    fn engine_record_and_check() {
        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 3);

        assert!(!engine.record_and_check("fn_x"));
        assert!(!engine.record_and_check("fn_x"));
        assert!(!engine.record_and_check("fn_x"));
        assert!(engine.record_and_check("fn_x"));
        assert!(!engine.record_and_check("fn_x"));
    }

    // --- JIT compile and execute: REAL native code execution tests ----------

    #[test]
    fn jit_execute_constant_42() {
        // cell answer() -> Int = 42
        let lir = simple_lir_module();
        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        let result = engine
            .compile_and_execute("answer", &lir, &[])
            .expect("JIT compile and execute should succeed");
        assert_eq!(result, 42, "JIT-compiled answer() should return 42");
    }

    #[test]
    fn jit_execute_addition() {
        // cell add_two() -> Int = 10 + 32
        let lir = make_module_with_cells(vec![LirCell {
            name: "add_two".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![Constant::Int(10), Constant::Int(32)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Add, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        let result = engine
            .compile_and_execute("add_two", &lir, &[])
            .expect("JIT add should succeed");
        assert_eq!(result, 42, "10 + 32 = 42");
    }

    #[test]
    fn jit_execute_with_parameter() {
        // cell double(x: Int) -> Int = x + x
        let lir = make_module_with_cells(vec![LirCell {
            name: "double".to_string(),
            params: vec![LirParam {
                name: "x".to_string(),
                ty: "Int".to_string(),
                register: 0,
                variadic: false,
            }],
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![],
            instructions: vec![
                Instruction::abc(OpCode::Add, 1, 0, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        engine
            .compile_module(&lir)
            .expect("JIT compile should succeed");

        assert_eq!(engine.execute_jit_unary("double", 21).unwrap(), 42);
        assert_eq!(engine.execute_jit_unary("double", 0).unwrap(), 0);
        assert_eq!(engine.execute_jit_unary("double", -5).unwrap(), -10);
    }

    #[test]
    fn jit_execute_binary_params() {
        // cell add(a: Int, b: Int) -> Int = a + b
        let lir = make_module_with_cells(vec![LirCell {
            name: "add".to_string(),
            params: vec![
                LirParam {
                    name: "a".to_string(),
                    ty: "Int".to_string(),
                    register: 0,
                    variadic: false,
                },
                LirParam {
                    name: "b".to_string(),
                    ty: "Int".to_string(),
                    register: 1,
                    variadic: false,
                },
            ],
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![],
            instructions: vec![
                Instruction::abc(OpCode::Add, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        engine
            .compile_module(&lir)
            .expect("JIT compile should succeed");

        assert_eq!(engine.execute_jit_binary("add", 10, 32).unwrap(), 42);
        assert_eq!(engine.execute_jit_binary("add", -3, 3).unwrap(), 0);
        assert_eq!(engine.execute_jit_binary("add", 100, 200).unwrap(), 300);
    }

    #[test]
    fn jit_execute_factorial_loop() {
        // Iterative factorial via while loop:
        //   cell factorial(n: Int) -> Int
        //     r1 = 1 (result)
        //     r2 = 1 (counter constant)
        //     while n > 0: r1 = r1 * n; n = n - r2
        //     return r1
        //
        //  0: LoadInt  r1, 1          (result = 1)
        //  1: LoadInt  r2, 1          (decrement constant)
        //  2: LoadInt  r3, 0          (zero for comparison)
        //  3: Lt       r4, r3, r0     (0 < n?)  -- loop header
        //  4: Test     r4, 0, 0
        //  5: Jmp      +3             (-> 9: exit loop)
        //  6: Mul      r1, r1, r0     (result *= n)
        //  7: Sub      r0, r0, r2     (n -= 1)
        //  8: Jmp      -6             (-> 3: loop header)
        //  9: Return   r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "factorial".to_string(),
            params: vec![LirParam {
                name: "n".to_string(),
                ty: "Int".to_string(),
                register: 0,
                variadic: false,
            }],
            returns: Some("Int".to_string()),
            registers: 5,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 1, 1),   // 0: r1 = 1
                Instruction::abx(OpCode::LoadInt, 2, 1),   // 1: r2 = 1
                Instruction::abx(OpCode::LoadInt, 3, 0),   // 2: r3 = 0
                Instruction::abc(OpCode::Lt, 4, 3, 0),     // 3: r4 = 0 < n
                Instruction::abc(OpCode::Test, 4, 0, 0),   // 4: test
                Instruction::sax(OpCode::Jmp, 3),          // 5: -> 9 (exit)
                Instruction::abc(OpCode::Mul, 1, 1, 0),    // 6: r1 *= n
                Instruction::abc(OpCode::Sub, 0, 0, 2),    // 7: n -= 1
                Instruction::sax(OpCode::Jmp, -6),         // 8: -> 3 (loop)
                Instruction::abc(OpCode::Return, 1, 1, 0), // 9: return r1
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        engine
            .compile_module(&lir)
            .expect("JIT compile should succeed");

        assert_eq!(engine.execute_jit_unary("factorial", 0).unwrap(), 1);
        assert_eq!(engine.execute_jit_unary("factorial", 1).unwrap(), 1);
        assert_eq!(engine.execute_jit_unary("factorial", 5).unwrap(), 120);
        assert_eq!(engine.execute_jit_unary("factorial", 10).unwrap(), 3628800);
    }

    #[test]
    fn jit_execute_fibonacci_tco() {
        // Tail-recursive fibonacci accumulator:
        //   cell fib_acc(n: Int, a: Int, b: Int) -> Int
        //     if n <= 0 then return a end
        //     fib_acc(n - 1, b, a + b)
        //   end
        //
        //  0: LoadInt   r3, 0
        //  1: Le        r4, r0, r3      (n <= 0?)
        //  2: Test      r4, 0, 0
        //  3: Jmp       +1              (-> 5: not done)
        //  4: Return    r1              (return a)
        //  5: LoadK     r5, 0           ("fib_acc")
        //  6: LoadInt   r8, 1
        //  7: Sub       r6, r0, r8      (n - 1)
        //  8: Move      r7, r2          (b)
        //  9: Add       r8, r1, r2      (a + b)
        // 10: TailCall  r5, 3, 1        (fib_acc(r6, r7, r8))
        let lir = make_module_with_cells(vec![LirCell {
            name: "fib_acc".to_string(),
            params: vec![
                LirParam {
                    name: "n".to_string(),
                    ty: "Int".to_string(),
                    register: 0,
                    variadic: false,
                },
                LirParam {
                    name: "a".to_string(),
                    ty: "Int".to_string(),
                    register: 1,
                    variadic: false,
                },
                LirParam {
                    name: "b".to_string(),
                    ty: "Int".to_string(),
                    register: 2,
                    variadic: false,
                },
            ],
            returns: Some("Int".to_string()),
            registers: 9,
            constants: vec![Constant::String("fib_acc".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 3, 0),     // 0: r3 = 0
                Instruction::abc(OpCode::Le, 4, 0, 3),       // 1: r4 = n <= 0
                Instruction::abc(OpCode::Test, 4, 0, 0),     // 2: test
                Instruction::sax(OpCode::Jmp, 1),            // 3: -> 5
                Instruction::abc(OpCode::Return, 1, 1, 0),   // 4: return a
                Instruction::abx(OpCode::LoadK, 5, 0),       // 5: r5 = "fib_acc"
                Instruction::abx(OpCode::LoadInt, 8, 1),     // 6: r8 = 1
                Instruction::abc(OpCode::Sub, 6, 0, 8),      // 7: r6 = n - 1
                Instruction::abc(OpCode::Move, 7, 2, 0),     // 8: r7 = b
                Instruction::abc(OpCode::Add, 8, 1, 2),      // 9: r8 = a + b
                Instruction::abc(OpCode::TailCall, 5, 3, 1), // 10: tail-call
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        engine
            .compile_module(&lir)
            .expect("JIT compile should succeed");

        // fib_acc(n, 0, 1) computes fib(n)
        assert_eq!(engine.execute_jit_ternary("fib_acc", 0, 0, 1).unwrap(), 0);
        assert_eq!(engine.execute_jit_ternary("fib_acc", 1, 0, 1).unwrap(), 1);
        assert_eq!(engine.execute_jit_ternary("fib_acc", 5, 0, 1).unwrap(), 5);
        assert_eq!(engine.execute_jit_ternary("fib_acc", 10, 0, 1).unwrap(), 55);
        assert_eq!(
            engine.execute_jit_ternary("fib_acc", 20, 0, 1).unwrap(),
            6765
        );
    }

    #[test]
    fn jit_execute_cross_cell_call() {
        // Two cells: double(x) = x + x, main() = double(21)
        let double_cell = LirCell {
            name: "double".to_string(),
            params: vec![LirParam {
                name: "x".to_string(),
                ty: "Int".to_string(),
                register: 0,
                variadic: false,
            }],
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![],
            instructions: vec![
                Instruction::abc(OpCode::Add, 1, 0, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };

        let main_cell = LirCell {
            name: "main".to_string(),
            params: vec![],
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![Constant::String("double".to_string()), Constant::Int(21)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0), // r0 = "double"
                Instruction::abx(OpCode::LoadK, 1, 1), // r1 = 21
                Instruction::abc(OpCode::Call, 0, 1, 1),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };

        let lir = make_module_with_cells(vec![double_cell, main_cell]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        let result = engine
            .compile_and_execute("main", &lir, &[])
            .expect("cross-cell JIT should succeed");
        assert_eq!(result, 42, "main() -> double(21) = 42");
    }

    #[test]
    fn jit_hot_path_triggers_compilation() {
        let lir = simple_lir_module();
        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 3);

        // Not hot yet.
        assert!(!engine.is_compiled("answer"));
        assert!(!engine.record_and_check("answer"));
        assert!(!engine.record_and_check("answer"));
        assert!(!engine.record_and_check("answer"));

        // 4th call: crosses threshold.
        assert!(engine.record_and_check("answer"));

        // Now compile and execute.
        engine
            .compile_hot("answer", &lir)
            .expect("compile_hot should succeed");
        assert!(engine.is_compiled("answer"));

        let result = engine
            .execute_jit_nullary("answer")
            .expect("execute should succeed");
        assert_eq!(result, 42);
    }

    #[test]
    fn jit_cache_and_stats() {
        let lir = simple_lir_module();
        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        let s0 = engine.stats();
        assert_eq!(s0.cells_compiled, 0);
        assert_eq!(s0.cache_hits, 0);
        assert_eq!(s0.executions, 0);

        engine.compile_hot("answer", &lir).expect("first compile");
        let s1 = engine.stats();
        assert_eq!(s1.cells_compiled, 1);
        assert!(s1.cache_size >= 1);

        // Second compile_hot should be a cache hit.
        engine.compile_hot("answer", &lir).expect("cached compile");
        let s2 = engine.stats();
        assert_eq!(s2.cache_hits, 1);

        engine.execute_jit_nullary("answer").expect("execute");
        let s3 = engine.stats();
        assert_eq!(s3.executions, 1);
    }

    #[test]
    fn jit_invalidate() {
        let lir = simple_lir_module();
        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        engine.compile_hot("answer", &lir).expect("compile");
        assert!(engine.is_compiled("answer"));

        engine.invalidate("answer");
        assert!(!engine.is_compiled("answer"));
        assert_eq!(engine.stats().cache_size, 0);
    }

    #[test]
    fn jit_execute_if_else() {
        // cell choose(x: Int) -> Int
        //   if x > 0 then 100 else 200 end
        //
        //  0: LoadInt   r1, 0
        //  1: Lt        r2, r1, r0     (0 < x => x > 0)
        //  2: Test      r2, 0, 0
        //  3: Jmp       +2             (-> 6: else)
        //  4: LoadInt   r3, 100
        //  5: Jmp       +1             (-> 7: end)
        //  6: LoadInt   r3, 50         -- LoadInt uses sbx (signed 32-bit) for the value
        //  7: Return    r3
        //
        // LoadInt stores the value in the Bx field (signed 32-bit via sbx()).
        // 100 fits in i8 (0x64). For the else branch let's use 50.
        let lir = make_module_with_cells(vec![LirCell {
            name: "choose".to_string(),
            params: vec![LirParam {
                name: "x".to_string(),
                ty: "Int".to_string(),
                register: 0,
                variadic: false,
            }],
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 1, 0),   // 0: r1 = 0
                Instruction::abc(OpCode::Lt, 2, 1, 0),     // 1: r2 = 0 < x
                Instruction::abc(OpCode::Test, 2, 0, 0),   // 2: test
                Instruction::sax(OpCode::Jmp, 2),          // 3: -> 6 (else)
                Instruction::abx(OpCode::LoadInt, 3, 100), // 4: r3 = 100
                Instruction::sax(OpCode::Jmp, 1),          // 5: -> 7 (end)
                Instruction::abx(OpCode::LoadInt, 3, 50),  // 6: r3 = 50
                Instruction::abc(OpCode::Return, 3, 1, 0), // 7: return r3
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        assert_eq!(engine.execute_jit_unary("choose", 5).unwrap(), 100);
        assert_eq!(engine.execute_jit_unary("choose", -1).unwrap(), 50);
        assert_eq!(engine.execute_jit_unary("choose", 0).unwrap(), 50);
    }

    #[test]
    fn jit_execute_generic_dispatch() {
        // Test the generic execute_jit() dispatch with varying arities.
        let add_cell = LirCell {
            name: "add".to_string(),
            params: vec![
                LirParam {
                    name: "a".to_string(),
                    ty: "Int".to_string(),
                    register: 0,
                    variadic: false,
                },
                LirParam {
                    name: "b".to_string(),
                    ty: "Int".to_string(),
                    register: 1,
                    variadic: false,
                },
            ],
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![],
            instructions: vec![
                Instruction::abc(OpCode::Add, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };

        let answer_cell = LirCell {
            name: "answer".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 2,
            constants: vec![Constant::Int(42)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };

        let lir = make_module_with_cells(vec![add_cell, answer_cell]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        // Nullary dispatch.
        assert_eq!(engine.execute_jit("answer", &[]).unwrap(), 42);

        // Binary dispatch.
        assert_eq!(engine.execute_jit("add", &[10, 32]).unwrap(), 42);

        // Unsupported arity.
        assert!(engine.execute_jit("add", &[1, 2, 3, 4]).is_err());
    }

    #[test]
    fn jit_compilable_includes_record_ops() {
        // Verify GetField and SetField are in the whitelist
        let get_field_cell = LirCell {
            name: "get_field".to_string(),
            params: vec![],
            returns: Some("Int".to_string()),
            registers: 3,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 0, 0), // r0 = 0 (record ptr stub)
                Instruction::abc(OpCode::GetField, 1, 0, 0), // r1 = r0.field[0]
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };

        let set_field_cell = LirCell {
            name: "set_field".to_string(),
            params: vec![],
            returns: Some("Int".to_string()),
            registers: 3,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 0, 0), // r0 = 0 (record ptr stub)
                Instruction::abx(OpCode::LoadInt, 1, 42), // r1 = 42
                Instruction::abc(OpCode::SetField, 0, 0, 1), // r0.field[0] = r1
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };

        assert!(
            is_cell_jit_compilable(&get_field_cell),
            "GetField should be JIT-compilable"
        );
        assert!(
            is_cell_jit_compilable(&set_field_cell),
            "SetField should be JIT-compilable"
        );
    }

    #[test]
    fn jit_compile_record_field_access() {
        // Test that cells with GetField/SetField compile and execute without errors.
        // GetField on a null record returns a boxed Value::Null (non-zero pointer).
        // SetField on a null record returns a boxed Value::Null (non-zero pointer).
        let lir = make_module_with_cells(vec![
            LirCell {
                name: "access_field".to_string(),
                params: vec![],
                returns: Some("Int".to_string()),
                registers: 3,
                constants: vec![],
                instructions: vec![
                    Instruction::abx(OpCode::LoadInt, 0, 0), // r0 = 0 (null record ptr)
                    Instruction::abc(OpCode::GetField, 1, 0, 0), // r1 = r0.field[0]
                    Instruction::abc(OpCode::Return, 1, 1, 0), // return r1
                ],
                effect_handler_metas: Vec::new(),
            },
            LirCell {
                name: "set_field".to_string(),
                params: vec![],
                returns: Some("Int".to_string()),
                registers: 3,
                constants: vec![],
                instructions: vec![
                    Instruction::abx(OpCode::LoadInt, 0, 0), // r0 = 0 (null record ptr)
                    Instruction::abx(OpCode::LoadInt, 1, 42), // r1 = 42
                    Instruction::abc(OpCode::SetField, 0, 0, 1), // r0.field[0] = r1 (updates r0)
                    Instruction::abc(OpCode::Return, 1, 1, 0), // return r1
                ],
                effect_handler_metas: Vec::new(),
            },
        ]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);

        // Should compile successfully
        engine
            .compile_module(&lir)
            .expect("Record field access cells should compile");

        // Verify both cells are compiled
        assert!(
            engine.is_compiled("access_field"),
            "access_field should be compiled"
        );
        assert!(
            engine.is_compiled("set_field"),
            "set_field should be compiled"
        );

        // Execute to ensure no runtime traps
        // GetField on null record returns a boxed Value::Null (non-zero pointer)
        let result = engine
            .execute_jit_nullary("access_field")
            .expect("GetField should execute");
        assert_ne!(
            result, 0,
            "GetField on null returns boxed Value::Null pointer"
        );

        let result2 = engine
            .execute_jit_nullary("set_field")
            .expect("SetField should execute");
        assert_eq!(result2, 42, "SetField returns the value register unchanged");
    }

    #[test]
    fn opt_level_variants() {
        let _none = OptLevel::None;
        let _speed = OptLevel::Speed;
        let _both = OptLevel::SpeedAndSize;
        assert_ne!(OptLevel::None, OptLevel::Speed);
        assert_ne!(OptLevel::Speed, OptLevel::SpeedAndSize);
    }

    // --- JIT string operation tests ----------------------------------------

    #[test]
    fn jit_string_constant_load_and_return() {
        // cell greeting() -> String
        //   return "hello"
        //
        // 0: LoadK   r0, 0   ("hello")
        // 1: Return  r0
        let lir = make_module_with_cells(vec![LirCell {
            name: "greeting".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 2,
            constants: vec![Constant::String("hello".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        assert!(
            engine.returns_string("greeting"),
            "greeting should be marked as returning a string"
        );

        let raw = engine
            .execute_jit_nullary("greeting")
            .expect("execute greeting");
        assert_ne!(raw, 0, "string pointer should be non-null");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "hello");
    }

    #[test]
    fn jit_string_concatenation() {
        // cell concat() -> String
        //   r0 = "hello, "
        //   r1 = "world"
        //   r2 = r0 + r1
        //   return r2
        //
        // 0: LoadK  r0, 0   ("hello, ")
        // 1: LoadK  r1, 1   ("world")
        // 2: Add    r2, r0, r1
        // 3: Return r2
        let lir = make_module_with_cells(vec![LirCell {
            name: "concat".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 4,
            constants: vec![
                Constant::String("hello, ".to_string()),
                Constant::String("world".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Add, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("concat").expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "hello, world");
    }

    #[test]
    fn jit_string_self_concat() {
        // cell double_str() -> String
        //   r0 = "ab"
        //   r0 = r0 + r0   (self-assign concat: a == b)
        //   return r0
        //
        // 0: LoadK  r0, 0   ("ab")
        // 1: Add    r0, r0, r0
        // 2: Return r0
        let lir = make_module_with_cells(vec![LirCell {
            name: "double_str".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 2,
            constants: vec![Constant::String("ab".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Add, 0, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("double_str").expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "abab");
    }

    #[test]
    fn jit_string_equality() {
        // cell eq_test() -> Int
        //   r0 = "abc"
        //   r1 = "abc"
        //   r2 = (r0 == r1)   -> should be 1
        //   return r2
        //
        // 0: LoadK  r0, 0   ("abc")
        // 1: LoadK  r1, 1   ("abc")
        // 2: Eq     r2, r0, r1
        // 3: Return r2
        let lir = make_module_with_cells(vec![LirCell {
            name: "eq_test".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![
                Constant::String("abc".to_string()),
                Constant::String("abc".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Eq, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("eq_test").expect("execute");
        assert_eq!(result, 1, "equal strings should return 1");
    }

    #[test]
    fn jit_string_inequality() {
        // cell neq_test() -> Int
        //   r0 = "abc"
        //   r1 = "xyz"
        //   r2 = (r0 == r1)   -> should be 0
        //   return r2
        let lir = make_module_with_cells(vec![LirCell {
            name: "neq_test".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![
                Constant::String("abc".to_string()),
                Constant::String("xyz".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Eq, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("neq_test").expect("execute");
        assert_eq!(result, 0, "different strings should return 0");
    }

    #[test]
    fn jit_string_less_than() {
        // cell lt_test() -> Int
        //   r0 = "apple"
        //   r1 = "banana"
        //   r2 = (r0 < r1)   -> should be 1 (lexicographic)
        //   return r2
        let lir = make_module_with_cells(vec![LirCell {
            name: "lt_test".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![
                Constant::String("apple".to_string()),
                Constant::String("banana".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Lt, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("lt_test").expect("execute");
        assert_eq!(result, 1, "\"apple\" < \"banana\" should be 1");
    }

    #[test]
    fn jit_string_less_than_reverse() {
        // "banana" < "apple" -> 0
        let lir = make_module_with_cells(vec![LirCell {
            name: "lt_rev".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![
                Constant::String("banana".to_string()),
                Constant::String("apple".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Lt, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("lt_rev").expect("execute");
        assert_eq!(result, 0, "\"banana\" < \"apple\" should be 0");
    }

    #[test]
    fn jit_string_less_equal() {
        // "abc" <= "abc" -> 1
        let lir = make_module_with_cells(vec![LirCell {
            name: "le_eq".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![
                Constant::String("abc".to_string()),
                Constant::String("abc".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Le, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("le_eq").expect("execute");
        assert_eq!(result, 1, "\"abc\" <= \"abc\" should be 1");
    }

    #[test]
    fn jit_string_move_clone() {
        // cell clone_str() -> String
        //   r0 = "original"
        //   r1 = r0         (Move: clone string)
        //   return r1
        //
        // 0: LoadK  r0, 0   ("original")
        // 1: Move   r1, r0
        // 2: Return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "clone_str".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 3,
            constants: vec![Constant::String("original".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Move, 1, 0, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("clone_str").expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "original");
    }

    #[test]
    fn jit_string_overwrite_drops_old() {
        // Verify that overwriting a string register with a new LoadK drops
        // the old value (no leak). We can't directly observe the drop, but
        // we confirm the final value is correct and no crash occurs.
        //
        // cell overwrite() -> String
        //   r0 = "first"
        //   r0 = "second"    (should drop "first" internally)
        //   return r0
        //
        // 0: LoadK  r0, 0   ("first")
        // 1: LoadK  r0, 1   ("second")
        // 2: Return r0
        let lir = make_module_with_cells(vec![LirCell {
            name: "overwrite".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 2,
            constants: vec![
                Constant::String("first".to_string()),
                Constant::String("second".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 0, 1),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("overwrite").expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "second");
    }

    #[test]
    fn jit_string_concat_in_loop() {
        // Build a string by concatenating in a loop (tests memory management
        // under repeated allocation/deallocation).
        //
        // cell build() -> String
        //   r0 = ""           (accumulator)
        //   r1 = "x"          (append constant)
        //   r2 = 3            (counter)
        //   r3 = 0            (zero)
        //   r4 = 1            (decrement)
        //   loop:
        //     if 0 < counter goto body else goto end
        //     body:
        //       r0 = r0 + r1    (self-assign concat)
        //       r2 = r2 - r4
        //       goto loop
        //   end:
        //     return r0
        //
        //  0: LoadK   r0, 0   ("")
        //  1: LoadK   r1, 1   ("x")
        //  2: LoadInt  r2, 3
        //  3: LoadInt  r3, 0
        //  4: LoadInt  r4, 1
        //  5: Lt       r5, r3, r2   (0 < counter? -> truthy means continue)
        //  6: Test     r5, 0, 0
        //  7: Jmp      +3           (-> 11: end, taken when r5 is falsy)
        //  8: Add      r0, r0, r1   (accum += "x")
        //  9: Sub      r2, r2, r4   (counter -= 1)
        // 10: Jmp      -6           (-> 5: loop)
        // 11: Return   r0
        let lir = make_module_with_cells(vec![LirCell {
            name: "build".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 7,
            constants: vec![
                Constant::String("".to_string()),
                Constant::String("x".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),     // 0: r0 = ""
                Instruction::abx(OpCode::LoadK, 1, 1),     // 1: r1 = "x"
                Instruction::abx(OpCode::LoadInt, 2, 3),   // 2: r2 = 3
                Instruction::abx(OpCode::LoadInt, 3, 0),   // 3: r3 = 0
                Instruction::abx(OpCode::LoadInt, 4, 1),   // 4: r4 = 1
                Instruction::abc(OpCode::Lt, 5, 3, 2),     // 5: r5 = 0 < counter
                Instruction::abc(OpCode::Test, 5, 0, 0),   // 6: test
                Instruction::sax(OpCode::Jmp, 3),          // 7: -> 11 (end)
                Instruction::abc(OpCode::Add, 0, 0, 1),    // 8: r0 = r0 + r1
                Instruction::abc(OpCode::Sub, 2, 2, 4),    // 9: r2 -= 1
                Instruction::sax(OpCode::Jmp, -6),         // 10: -> 5 (loop)
                Instruction::abc(OpCode::Return, 0, 1, 0), // 11: return r0
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("build").expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "xxx", "loop should concatenate 'x' three times");
    }

    #[test]
    fn jit_string_conditional_branch() {
        // cell pick(x: Int) -> String
        //   if x > 0 then "positive" else "non-positive" end
        //
        //  0: LoadInt  r1, 0
        //  1: Lt       r2, r1, r0      (0 < x => x > 0?)
        //  2: Test     r2, 0, 0
        //  3: Jmp      +2              (-> 6: else)
        //  4: LoadK    r3, 0           ("positive")
        //  5: Jmp      +1              (-> 7: end)
        //  6: LoadK    r3, 1           ("non-positive")
        //  7: Return   r3
        let lir = make_module_with_cells(vec![LirCell {
            name: "pick".to_string(),
            params: vec![LirParam {
                name: "x".to_string(),
                ty: "Int".to_string(),
                register: 0,
                variadic: false,
            }],
            returns: Some("String".to_string()),
            registers: 5,
            constants: vec![
                Constant::String("positive".to_string()),
                Constant::String("non-positive".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 1, 0),   // 0: r1 = 0
                Instruction::abc(OpCode::Lt, 2, 1, 0),     // 1: r2 = 0 < x
                Instruction::abc(OpCode::Test, 2, 0, 0),   // 2: test
                Instruction::sax(OpCode::Jmp, 2),          // 3: -> 6 (else)
                Instruction::abx(OpCode::LoadK, 3, 0),     // 4: r3 = "positive"
                Instruction::sax(OpCode::Jmp, 1),          // 5: -> 7 (end)
                Instruction::abx(OpCode::LoadK, 3, 1),     // 6: r3 = "non-positive"
                Instruction::abc(OpCode::Return, 3, 1, 0), // 7: return r3
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        assert!(engine.returns_string("pick"));

        let raw_pos = engine.execute_jit_unary("pick", 5).expect("positive");
        let s_pos = unsafe { jit_take_string(raw_pos) };
        assert_eq!(s_pos, "positive");

        let raw_neg = engine.execute_jit_unary("pick", -1).expect("negative");
        let s_neg = unsafe { jit_take_string(raw_neg) };
        assert_eq!(s_neg, "non-positive");

        let raw_zero = engine.execute_jit_unary("pick", 0).expect("zero");
        let s_zero = unsafe { jit_take_string(raw_zero) };
        assert_eq!(s_zero, "non-positive");
    }

    #[test]
    fn jit_string_empty_string() {
        // Verify empty string allocation and return.
        let lir = make_module_with_cells(vec![LirCell {
            name: "empty".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 2,
            constants: vec![Constant::String("".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("empty").expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "");
    }

    #[test]
    fn jit_string_multiple_concats() {
        // cell three_way() -> String
        //   r0 = "a"
        //   r1 = "b"
        //   r2 = "c"
        //   r3 = r0 + r1    ("ab")
        //   r4 = r3 + r2    ("abc")
        //   return r4
        let lir = make_module_with_cells(vec![LirCell {
            name: "three_way".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 6,
            constants: vec![
                Constant::String("a".to_string()),
                Constant::String("b".to_string()),
                Constant::String("c".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),     // r0 = "a"
                Instruction::abx(OpCode::LoadK, 1, 1),     // r1 = "b"
                Instruction::abx(OpCode::LoadK, 2, 2),     // r2 = "c"
                Instruction::abc(OpCode::Add, 3, 0, 1),    // r3 = "a" + "b"
                Instruction::abc(OpCode::Add, 4, 3, 2),    // r4 = "ab" + "c"
                Instruction::abc(OpCode::Return, 4, 1, 0), // return "abc"
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("three_way").expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "abc");
    }

    #[test]
    fn jit_string_eq_used_in_branch() {
        // cell is_hello() -> Int
        //   r0 = "hello"
        //   r1 = "hello"
        //   r2 = (r0 == r1)
        //   if r2 then return 100 else return 200
        //
        //  0: LoadK   r0, 0   ("hello")
        //  1: LoadK   r1, 1   ("hello")
        //  2: Eq      r2, r0, r1
        //  3: Test    r2, 0, 0
        //  4: Jmp     +2      (-> 7: else)
        //  5: LoadInt r3, 100
        //  6: Jmp     +1      (-> 8: end)
        //  7: LoadInt r3, 50
        //  8: Return  r3
        let lir = make_module_with_cells(vec![LirCell {
            name: "is_hello".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 5,
            constants: vec![
                Constant::String("hello".to_string()),
                Constant::String("hello".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Eq, 2, 0, 1),
                Instruction::abc(OpCode::Test, 2, 0, 0),
                Instruction::sax(OpCode::Jmp, 2),
                Instruction::abx(OpCode::LoadInt, 3, 100),
                Instruction::sax(OpCode::Jmp, 1),
                Instruction::abx(OpCode::LoadInt, 3, 50),
                Instruction::abc(OpCode::Return, 3, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("is_hello").expect("execute");
        assert_eq!(result, 100, "equal strings should take the then-branch");
    }

    #[test]
    fn jit_string_returns_string_flag() {
        // Verify that cells returning String have returns_string=true,
        // and cells returning Int have returns_string=false.
        let lir = make_module_with_cells(vec![
            LirCell {
                name: "str_cell".to_string(),
                params: Vec::new(),
                returns: Some("String".to_string()),
                registers: 2,
                constants: vec![Constant::String("hi".to_string())],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
                effect_handler_metas: Vec::new(),
            },
            LirCell {
                name: "int_cell".to_string(),
                params: Vec::new(),
                returns: Some("Int".to_string()),
                registers: 2,
                constants: vec![Constant::Int(42)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
                effect_handler_metas: Vec::new(),
            },
        ]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        assert!(engine.returns_string("str_cell"));
        assert!(!engine.returns_string("int_cell"));
    }

    #[test]
    fn jit_string_move_own_transfer() {
        // cell transfer() -> String
        //   r0 = "transferred"
        //   MoveOwn r1, r0    (ownership transfer, no clone)
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "transfer".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 3,
            constants: vec![Constant::String("transferred".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::MoveOwn, 1, 0, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("transfer").expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "transferred");
    }

    #[test]
    fn jit_string_concat_dest_overwrites_distinct() {
        // Test where Add dest (r0) already holds a string different from both
        // operands (r1, r2). The old r0 value should be dropped.
        //
        // cell overwrite_concat() -> String
        //   r0 = "old"
        //   r1 = "hello"
        //   r2 = " world"
        //   r0 = r1 + r2    (overwrites "old" in r0 with "hello world")
        //   return r0
        let lir = make_module_with_cells(vec![LirCell {
            name: "overwrite_concat".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 4,
            constants: vec![
                Constant::String("old".to_string()),
                Constant::String("hello".to_string()),
                Constant::String(" world".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),     // r0 = "old"
                Instruction::abx(OpCode::LoadK, 1, 1),     // r1 = "hello"
                Instruction::abx(OpCode::LoadK, 2, 2),     // r2 = " world"
                Instruction::abc(OpCode::Add, 0, 1, 2),    // r0 = r1 + r2
                Instruction::abc(OpCode::Return, 0, 1, 0), // return r0
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine
            .execute_jit_nullary("overwrite_concat")
            .expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "hello world");
    }

    #[test]
    fn jit_string_concat_in_place_optimization() {
        // Test the in-place optimization for `a = a + b` pattern.
        // This should use jit_rt_string_concat_mut which reuses the allocation
        // from r0 instead of creating a new string.
        //
        // cell concat_test() -> String
        //   r0 = ""
        //   r1 = "x"
        //   r0 = r0 + r1    (in-place)
        //   r0 = r0 + r1    (in-place)
        //   r0 = r0 + r1    (in-place)
        //   return r0
        let lir = make_module_with_cells(vec![LirCell {
            name: "concat_test".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 3,
            constants: vec![
                Constant::String("".to_string()),
                Constant::String("x".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),     // r0 = ""
                Instruction::abx(OpCode::LoadK, 1, 1),     // r1 = "x"
                Instruction::abc(OpCode::Add, 0, 0, 1),    // r0 = r0 + r1 (in-place!)
                Instruction::abc(OpCode::Add, 0, 0, 1),    // r0 = r0 + r1 (in-place!)
                Instruction::abc(OpCode::Add, 0, 0, 1),    // r0 = r0 + r1 (in-place!)
                Instruction::abc(OpCode::Return, 0, 1, 0), // return r0
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("concat_test").expect("execute");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "xxx");
    }

    #[test]
    fn jit_intrinsic_abs_int() {
        // Test abs() intrinsic with integer argument
        // cell test_abs() -> Int
        //   r0 = -10
        //   r1 = abs(r0)   # Intrinsic(1, 26, 0) - IntrinsicId::Abs = 26
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_abs".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 2,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 0, (-10i32) as u32), // r0 = -10
                Instruction::abc(OpCode::Intrinsic, 1, 26, 0),         // r1 = abs(r0)
                Instruction::abc(OpCode::Return, 1, 1, 0),             // return r1
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("test_abs").expect("execute");
        assert_eq!(result, 10); // abs(-10) = 10
    }

    #[test]
    fn jit_intrinsic_print_int() {
        // Test print() intrinsic with integer argument
        // cell test_print() -> Int
        //   r0 = 42
        //   r1 = print(r0)  # Intrinsic(1, 9, 0) - IntrinsicId::Print = 9
        //   r2 = 0
        //   return r2
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_print".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 3,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 0, 42),     // r0 = 42
                Instruction::abc(OpCode::Intrinsic, 1, 9, 0), // r1 = print(r0)
                Instruction::abx(OpCode::LoadInt, 2, 0),      // r2 = 0
                Instruction::abc(OpCode::Return, 2, 1, 0),    // return r2
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        // Just verify it compiles and executes without crashing
        // (print goes to stdout, we don't capture it here)
        let result = engine.execute_jit_nullary("test_print").expect("execute");
        assert_eq!(result, 0);
    }

    #[test]
    fn jit_intrinsic_len_string() {
        // Test len() intrinsic with string argument
        // cell test_len() -> Int
        //   r0 = "hello"
        //   r1 = len(r0)     # Intrinsic(1, 0, 0) - IntrinsicId::Length = 0
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_len".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 2,
            constants: vec![Constant::String("hello".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),        // r0 = "hello"
                Instruction::abc(OpCode::Intrinsic, 1, 0, 0), // r1 = len(r0)
                Instruction::abc(OpCode::Return, 1, 1, 0),    // return r1
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("test_len").expect("execute");
        assert_eq!(result, 5); // len("hello") = 5
    }

    // --- New intrinsic tests (math, conversion, type) ----------------------

    #[test]
    fn jit_intrinsic_abs_float() {
        // cell test_abs_float() -> Float
        //   r0 = -3.5  (via LoadK Float constant)
        //   r1 = abs(r0)  # Intrinsic(1, 26, 0)
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_abs_float".to_string(),
            params: Vec::new(),
            returns: Some("Float".to_string()),
            registers: 2,
            constants: vec![Constant::Float(-3.5)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 26, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result_bits = engine
            .execute_jit_nullary("test_abs_float")
            .expect("execute");
        let result = f64::from_bits(result_bits as u64);
        assert!(
            (result - 3.5).abs() < 1e-10,
            "abs(-3.5) should be 3.5, got {result}"
        );
    }

    #[test]
    fn jit_intrinsic_min_int() {
        // cell test_min() -> Int
        //   r0 = 10
        //   r1 = 3
        //   r2 = min(r0, r1)  # Intrinsic(2, 27, 0) — args at r0, r1
        //   return r2
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_min".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 3,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 0, 10),
                Instruction::abx(OpCode::LoadInt, 1, 3),
                Instruction::abc(OpCode::Intrinsic, 2, 27, 0),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("test_min").expect("execute");
        assert_eq!(result, 3, "min(10, 3) = 3");
    }

    #[test]
    fn jit_intrinsic_max_int() {
        // cell test_max() -> Int
        //   r0 = 10
        //   r1 = 3
        //   r2 = max(r0, r1)  # Intrinsic(2, 28, 0)
        //   return r2
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_max".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 3,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 0, 10),
                Instruction::abx(OpCode::LoadInt, 1, 3),
                Instruction::abc(OpCode::Intrinsic, 2, 28, 0),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("test_max").expect("execute");
        assert_eq!(result, 10, "max(10, 3) = 10");
    }

    #[test]
    fn jit_intrinsic_sqrt_float() {
        // cell test_sqrt() -> Float
        //   r0 = 9.0
        //   r1 = sqrt(r0)  # Intrinsic(1, 60, 0)
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_sqrt".to_string(),
            params: Vec::new(),
            returns: Some("Float".to_string()),
            registers: 2,
            constants: vec![Constant::Float(9.0)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 60, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result_bits = engine.execute_jit_nullary("test_sqrt").expect("execute");
        let result = f64::from_bits(result_bits as u64);
        assert!(
            (result - 3.0).abs() < 1e-10,
            "sqrt(9.0) should be 3.0, got {result}"
        );
    }

    #[test]
    fn jit_intrinsic_floor_float() {
        // cell test_floor() -> Float
        //   r0 = 3.7
        //   r1 = floor(r0)  # Intrinsic(1, 59, 0)
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_floor".to_string(),
            params: Vec::new(),
            returns: Some("Float".to_string()),
            registers: 2,
            constants: vec![Constant::Float(3.7)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 59, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result_bits = engine.execute_jit_nullary("test_floor").expect("execute");
        let result = f64::from_bits(result_bits as u64);
        assert!(
            (result - 3.0).abs() < 1e-10,
            "floor(3.7) should be 3.0, got {result}"
        );
    }

    #[test]
    fn jit_intrinsic_ceil_float() {
        // cell test_ceil() -> Float
        //   r0 = 3.2
        //   r1 = ceil(r0)  # Intrinsic(1, 58, 0)
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_ceil".to_string(),
            params: Vec::new(),
            returns: Some("Float".to_string()),
            registers: 2,
            constants: vec![Constant::Float(3.2)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 58, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result_bits = engine.execute_jit_nullary("test_ceil").expect("execute");
        let result = f64::from_bits(result_bits as u64);
        assert!(
            (result - 4.0).abs() < 1e-10,
            "ceil(3.2) should be 4.0, got {result}"
        );
    }

    #[test]
    fn jit_intrinsic_round_float() {
        // cell test_round() -> Float
        //   r0 = 3.5
        //   r1 = round(r0)  # Intrinsic(1, 57, 0)
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_round".to_string(),
            params: Vec::new(),
            returns: Some("Float".to_string()),
            registers: 2,
            constants: vec![Constant::Float(3.5)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 57, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result_bits = engine.execute_jit_nullary("test_round").expect("execute");
        let result = f64::from_bits(result_bits as u64);
        // Cranelift's `nearest` uses banker's rounding (round half to even)
        // 3.5 rounds to 4.0 (nearest even)
        assert!(
            (result - 4.0).abs() < 1e-10,
            "round(3.5) should be 4.0 (banker's rounding), got {result}"
        );
    }

    #[test]
    fn jit_intrinsic_sin() {
        // cell test_sin() -> Float
        //   r0 = 0.0
        //   r1 = sin(r0)  # Intrinsic(1, 63, 0)
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_sin".to_string(),
            params: Vec::new(),
            returns: Some("Float".to_string()),
            registers: 2,
            constants: vec![Constant::Float(0.0)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 63, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result_bits = engine.execute_jit_nullary("test_sin").expect("execute");
        let result = f64::from_bits(result_bits as u64);
        assert!(
            (result - 0.0).abs() < 1e-10,
            "sin(0.0) should be 0.0, got {result}"
        );
    }

    #[test]
    fn jit_intrinsic_cos() {
        // cell test_cos() -> Float
        //   r0 = 0.0
        //   r1 = cos(r0)  # Intrinsic(1, 64, 0)
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_cos".to_string(),
            params: Vec::new(),
            returns: Some("Float".to_string()),
            registers: 2,
            constants: vec![Constant::Float(0.0)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 64, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result_bits = engine.execute_jit_nullary("test_cos").expect("execute");
        let result = f64::from_bits(result_bits as u64);
        assert!(
            (result - 1.0).abs() < 1e-10,
            "cos(0.0) should be 1.0, got {result}"
        );
    }

    #[test]
    fn jit_intrinsic_log() {
        // cell test_log() -> Float
        //   r0 = 1.0
        //   r1 = log(r0)  # Intrinsic(1, 62, 0) — ln(1.0) = 0.0
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_log".to_string(),
            params: Vec::new(),
            returns: Some("Float".to_string()),
            registers: 2,
            constants: vec![Constant::Float(1.0)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 62, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result_bits = engine.execute_jit_nullary("test_log").expect("execute");
        let result = f64::from_bits(result_bits as u64);
        assert!(
            (result - 0.0).abs() < 1e-10,
            "log(1.0) should be 0.0, got {result}"
        );
    }

    #[test]
    fn jit_intrinsic_pow_int() {
        // cell test_pow() -> Int
        //   r0 = 2
        //   r1 = 10
        //   r2 = pow(r0, r1)  # Intrinsic(2, 61, 0) — 2^10 = 1024
        //   return r2
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_pow".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 3,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 0, 2),
                Instruction::abx(OpCode::LoadInt, 1, 10),
                Instruction::abc(OpCode::Intrinsic, 2, 61, 0),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("test_pow").expect("execute");
        assert_eq!(result, 1024, "pow(2, 10) = 1024");
    }

    #[test]
    fn jit_intrinsic_pow_float() {
        // cell test_pow_f() -> Float
        //   r0 = 2.0
        //   r1 = 3.0
        //   r2 = pow(r0, r1)  # Intrinsic(2, 61, 0) — 2.0^3.0 = 8.0
        //   return r2
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_pow_f".to_string(),
            params: Vec::new(),
            returns: Some("Float".to_string()),
            registers: 3,
            constants: vec![Constant::Float(2.0), Constant::Float(3.0)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Intrinsic, 2, 61, 0),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result_bits = engine.execute_jit_nullary("test_pow_f").expect("execute");
        let result = f64::from_bits(result_bits as u64);
        assert!(
            (result - 8.0).abs() < 1e-10,
            "pow(2.0, 3.0) should be 8.0, got {result}"
        );
    }

    #[test]
    fn jit_intrinsic_clamp_int() {
        // cell test_clamp() -> Int
        //   r0 = 15       (value)
        //   r1 = 0        (lo)
        //   r2 = 10       (hi)
        //   r3 = clamp(r0, r1, r2)  # Intrinsic(3, 65, 0) — clamp(15, 0, 10) = 10
        //   return r3
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_clamp".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 0, 15),
                Instruction::abx(OpCode::LoadInt, 1, 0),
                Instruction::abx(OpCode::LoadInt, 2, 10),
                Instruction::abc(OpCode::Intrinsic, 3, 65, 0),
                Instruction::abc(OpCode::Return, 3, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("test_clamp").expect("execute");
        assert_eq!(result, 10, "clamp(15, 0, 10) = 10");
    }

    #[test]
    fn jit_intrinsic_math_pi() {
        // cell test_pi() -> Float
        //   r0 = math_pi()  # Intrinsic(0, 127, 0) — no args needed
        //   return r0
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_pi".to_string(),
            params: Vec::new(),
            returns: Some("Float".to_string()),
            registers: 1,
            constants: vec![],
            instructions: vec![
                Instruction::abc(OpCode::Intrinsic, 0, 127, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result_bits = engine.execute_jit_nullary("test_pi").expect("execute");
        let result = f64::from_bits(result_bits as u64);
        assert!(
            (result - std::f64::consts::PI).abs() < 1e-10,
            "math_pi should be π, got {result}"
        );
    }

    #[test]
    fn jit_intrinsic_math_e() {
        // cell test_e() -> Float
        //   r0 = math_e()  # Intrinsic(0, 128, 0)
        //   return r0
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_e".to_string(),
            params: Vec::new(),
            returns: Some("Float".to_string()),
            registers: 1,
            constants: vec![],
            instructions: vec![
                Instruction::abc(OpCode::Intrinsic, 0, 128, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result_bits = engine.execute_jit_nullary("test_e").expect("execute");
        let result = f64::from_bits(result_bits as u64);
        assert!(
            (result - std::f64::consts::E).abs() < 1e-10,
            "math_e should be e, got {result}"
        );
    }

    #[test]
    fn jit_intrinsic_is_nan() {
        // cell test_is_nan() -> Int
        //   r0 = NaN (0.0/0.0 via constants)
        //   r1 = is_nan(r0)  # Intrinsic(1, 125, 0)
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_is_nan".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 2,
            constants: vec![Constant::Float(f64::NAN)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 125, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("test_is_nan").expect("execute");
        assert_eq!(result, 1, "is_nan(NaN) should be 1 (true)");
    }

    #[test]
    fn jit_intrinsic_is_nan_false() {
        // cell test_not_nan() -> Int
        //   r0 = 42.0
        //   r1 = is_nan(r0)  # Intrinsic(1, 125, 0)
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_not_nan".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 2,
            constants: vec![Constant::Float(42.0)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 125, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("test_not_nan").expect("execute");
        assert_eq!(result, 0, "is_nan(42.0) should be 0 (false)");
    }

    #[test]
    fn jit_intrinsic_is_infinite() {
        // cell test_is_inf() -> Int
        //   r0 = +inf
        //   r1 = is_infinite(r0)  # Intrinsic(1, 126, 0)
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_is_inf".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 2,
            constants: vec![Constant::Float(f64::INFINITY)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 126, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("test_is_inf").expect("execute");
        assert_eq!(result, 1, "is_infinite(+inf) should be 1 (true)");
    }

    #[test]
    fn jit_intrinsic_is_infinite_neg() {
        // cell test_is_neg_inf() -> Int
        //   r0 = -inf
        //   r1 = is_infinite(r0)  # Intrinsic(1, 126, 0)
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_is_neg_inf".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 2,
            constants: vec![Constant::Float(f64::NEG_INFINITY)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 126, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine
            .execute_jit_nullary("test_is_neg_inf")
            .expect("execute");
        assert_eq!(result, 1, "is_infinite(-inf) should be 1 (true)");
    }

    #[test]
    fn jit_intrinsic_to_int_from_float() {
        // cell test_to_int() -> Int
        //   r0 = 3.7
        //   r1 = to_int(r0)  # Intrinsic(1, 11, 0) — truncates to 3
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_to_int".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 2,
            constants: vec![Constant::Float(3.7)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 11, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("test_to_int").expect("execute");
        assert_eq!(result, 3, "to_int(3.7) should be 3");
    }

    #[test]
    fn jit_intrinsic_to_float_from_int() {
        // cell test_to_float() -> Float
        //   r0 = 42
        //   r1 = to_float(r0)  # Intrinsic(1, 12, 0)
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_to_float".to_string(),
            params: Vec::new(),
            returns: Some("Float".to_string()),
            registers: 2,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 0, 42),
                Instruction::abc(OpCode::Intrinsic, 1, 12, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result_bits = engine
            .execute_jit_nullary("test_to_float")
            .expect("execute");
        let result = f64::from_bits(result_bits as u64);
        assert!(
            (result - 42.0).abs() < 1e-10,
            "to_float(42) should be 42.0, got {result}"
        );
    }

    #[test]
    fn jit_intrinsic_log2() {
        // cell test_log2() -> Float
        //   r0 = 8.0
        //   r1 = log2(r0)  # Intrinsic(1, 123, 0) — log2(8) = 3.0
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_log2".to_string(),
            params: Vec::new(),
            returns: Some("Float".to_string()),
            registers: 2,
            constants: vec![Constant::Float(8.0)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 123, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result_bits = engine.execute_jit_nullary("test_log2").expect("execute");
        let result = f64::from_bits(result_bits as u64);
        assert!(
            (result - 3.0).abs() < 1e-10,
            "log2(8.0) should be 3.0, got {result}"
        );
    }

    #[test]
    fn jit_intrinsic_log10() {
        // cell test_log10() -> Float
        //   r0 = 100.0
        //   r1 = log10(r0)  # Intrinsic(1, 124, 0) — log10(100) = 2.0
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_log10".to_string(),
            params: Vec::new(),
            returns: Some("Float".to_string()),
            registers: 2,
            constants: vec![Constant::Float(100.0)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 124, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result_bits = engine.execute_jit_nullary("test_log10").expect("execute");
        let result = f64::from_bits(result_bits as u64);
        assert!(
            (result - 2.0).abs() < 1e-10,
            "log10(100.0) should be 2.0, got {result}"
        );
    }

    #[test]
    fn jit_intrinsic_is_empty_string() {
        // cell test_is_empty() -> Int
        //   r0 = ""
        //   r1 = is_empty(r0)  # Intrinsic(1, 50, 0)
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_is_empty".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 2,
            constants: vec![Constant::String("".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 50, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine
            .execute_jit_nullary("test_is_empty")
            .expect("execute");
        assert_eq!(result, 1, "is_empty(\"\") should be 1 (true)");
    }

    #[test]
    fn jit_intrinsic_is_empty_nonempty() {
        // cell test_not_empty() -> Int
        //   r0 = "hi"
        //   r1 = is_empty(r0)  # Intrinsic(1, 50, 0)
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_not_empty".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 2,
            constants: vec![Constant::String("hi".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 50, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine
            .execute_jit_nullary("test_not_empty")
            .expect("execute");
        assert_eq!(result, 0, "is_empty(\"hi\") should be 0 (false)");
    }

    #[test]
    fn jit_intrinsic_string_concat_str() {
        // cell test_str_concat() -> String
        //   r0 = "hello"
        //   r1 = string_concat(r0)  # Intrinsic(1, 106, 0) — passthrough
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_str_concat".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 2,
            constants: vec![Constant::String("hello".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 106, 0), // string_concat(r0)
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine
            .execute_jit_nullary("test_str_concat")
            .expect("execute");
        assert_ne!(raw, 0, "string pointer should be non-null");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "hello", "string_concat(\"hello\") should be \"hello\"");
    }

    #[test]
    fn jit_intrinsic_string_concat_int() {
        // cell test_str_concat_int() -> String
        //   r0 = 42
        //   r1 = string_concat(r0)  # Intrinsic(1, 106, 0) — int to string
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_str_concat_int".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 2,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 0, 42),
                Instruction::abc(OpCode::Intrinsic, 1, 106, 0), // string_concat(42)
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine
            .execute_jit_nullary("test_str_concat_int")
            .expect("execute");
        assert_ne!(raw, 0, "string pointer should be non-null");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "42", "string_concat(42) should produce \"42\"");
    }

    #[test]
    fn jit_intrinsic_string_concat_float() {
        // cell test_str_concat_float() -> String
        //   r0 = 3.14
        //   r1 = string_concat(r0)  # Intrinsic(1, 106, 0) — float to string
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_str_concat_float".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 2,
            constants: vec![Constant::Float(3.14)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 106, 0), // string_concat(3.14)
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine
            .execute_jit_nullary("test_str_concat_float")
            .expect("execute");
        assert_ne!(raw, 0, "string pointer should be non-null");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "3.14", "string_concat(3.14) should be \"3.14\"");
    }

    // --- OpCode::Pow (** operator) tests ----------------------------------

    #[test]
    fn jit_opcode_pow_int() {
        // cell test_pow_op() -> Int
        //   r0 = 2
        //   r1 = 10
        //   r2 = r0 ** r1   # OpCode::Pow — 2^10 = 1024
        //   return r2
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_pow_op".to_string(),
            params: Vec::new(),
            returns: Some("Int".to_string()),
            registers: 3,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 0, 2),
                Instruction::abx(OpCode::LoadInt, 1, 10),
                Instruction::abc(OpCode::Pow, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let result = engine.execute_jit_nullary("test_pow_op").expect("execute");
        assert_eq!(result, 1024, "2 ** 10 = 1024");
    }

    #[test]
    fn jit_opcode_pow_float() {
        // cell test_pow_op_f() -> Float
        //   r0 = 3.0
        //   r1 = 2.0
        //   r2 = r0 ** r1   # OpCode::Pow — 3.0^2.0 = 9.0
        //   return r2
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_pow_op_f".to_string(),
            params: Vec::new(),
            returns: Some("Float".to_string()),
            registers: 3,
            constants: vec![Constant::Float(3.0), Constant::Float(2.0)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Pow, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine
            .execute_jit_nullary("test_pow_op_f")
            .expect("execute");
        let result = f64::from_bits(raw as u64);
        assert!(
            (result - 9.0).abs() < 1e-10,
            "3.0 ** 2.0 = 9.0, got {result}"
        );
    }

    // --- OpCode::Mod with floats tests ------------------------------------

    #[test]
    fn jit_opcode_mod_float() {
        // cell test_mod_f() -> Float
        //   r0 = 7.5
        //   r1 = 2.5
        //   r2 = r0 % r1   # OpCode::Mod — 7.5 % 2.5 = 0.0
        //   return r2
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_mod_f".to_string(),
            params: Vec::new(),
            returns: Some("Float".to_string()),
            registers: 3,
            constants: vec![Constant::Float(7.5), Constant::Float(2.5)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Mod, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("test_mod_f").expect("execute");
        let result = f64::from_bits(raw as u64);
        assert!(result.abs() < 1e-10, "7.5 % 2.5 = 0.0, got {result}");
    }

    #[test]
    fn jit_opcode_mod_float_nonzero() {
        // cell test_mod_f2() -> Float
        //   r0 = 10.0
        //   r1 = 3.0
        //   r2 = r0 % r1   # OpCode::Mod — 10.0 % 3.0 = 1.0
        //   return r2
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_mod_f2".to_string(),
            params: Vec::new(),
            returns: Some("Float".to_string()),
            registers: 3,
            constants: vec![Constant::Float(10.0), Constant::Float(3.0)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Mod, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("test_mod_f2").expect("execute");
        let result = f64::from_bits(raw as u64);
        assert!(
            (result - 1.0).abs() < 1e-10,
            "10.0 % 3.0 = 1.0, got {result}"
        );
    }

    // --- ToString inline refcount test ------------------------------------

    #[test]
    fn jit_intrinsic_to_string_passthrough() {
        // cell test_tostr_pass() -> String
        //   r0 = "hello"
        //   r1 = to_string(r0)   # Intrinsic(1, 10, 0) — should inline refcount
        //   return r1
        let lir = make_module_with_cells(vec![LirCell {
            name: "test_tostr_pass".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 2,
            constants: vec![Constant::String("hello".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Intrinsic, 1, 10, 0), // to_string(r0)
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine
            .execute_jit_nullary("test_tostr_pass")
            .expect("execute");
        assert_ne!(raw, 0, "string pointer should be non-null");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "hello", "to_string on a string should pass through");
    }

    // --- Standalone Concat opcode test ------------------------------------

    #[test]
    fn jit_string_concat_opcode() {
        // cell greet() -> String
        //   r0 = "hello"
        //   r1 = " world"
        //   r2 = r0 ++ r1          # Concat(2, 0, 1)
        //   return r2
        let lir = make_module_with_cells(vec![LirCell {
            name: "greet".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 3,
            constants: vec![
                Constant::String("hello".to_string()),
                Constant::String(" world".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Concat, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("greet").expect("execute");
        assert_ne!(raw, 0, "string pointer should be non-null");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(s, "hello world", "Concat opcode should concatenate strings");
    }

    // --- Cross-cell call return type inference tests -----------------------

    #[test]
    fn jit_cross_cell_call_string_return() {
        // Test that when main() calls greet() which returns String,
        // the JIT correctly infers the return type as String (not Int).
        //
        // cell greet() -> String
        //   r0 = "hello"
        //   r1 = " world"
        //   r2 = r0 ++ r1          # Concat(2, 0, 1)
        //   return r2
        //
        // cell main() -> String
        //   r0 = "greet"
        //   call r0, 0 args       # result in r0, should be typed as String
        //   return r0
        let greet_cell = LirCell {
            name: "greet".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 3,
            constants: vec![
                Constant::String("hello".to_string()),
                Constant::String(" world".to_string()),
            ],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),     // r0 = "hello"
                Instruction::abx(OpCode::LoadK, 1, 1),     // r1 = " world"
                Instruction::abc(OpCode::Concat, 2, 0, 1), // r2 = r0 ++ r1
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };

        let main_cell = LirCell {
            name: "main".to_string(),
            params: Vec::new(),
            returns: Some("String".to_string()),
            registers: 2,
            constants: vec![Constant::String("greet".to_string())],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),     // r0 = "greet"
                Instruction::abc(OpCode::Call, 0, 0, 0),   // r0 = greet()
                Instruction::abc(OpCode::Return, 0, 1, 0), // return r0
            ],
            effect_handler_metas: Vec::new(),
        };

        let lir = make_module_with_cells(vec![greet_cell, main_cell]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("main").expect("execute main");
        assert_ne!(raw, 0, "string pointer should be non-null");
        let s = unsafe { jit_take_string(raw) };
        assert_eq!(
            s, "hello world",
            "cross-cell string return should preserve string type"
        );
    }

    #[test]
    fn jit_cross_cell_call_float_return() {
        // Test that when main() calls half() which returns Float,
        // the JIT correctly infers the return type as Float (not Int).
        //
        // cell half(x: Int) -> Float
        //   r1 = to_float(r0)   # Intrinsic(1, 12, 0)
        //   r2 = 2.0
        //   r3 = r1 / r2
        //   return r3
        //
        // cell main() -> Float
        //   r0 = "half"
        //   r1 = 7
        //   call r0, 1 arg       # result in r0, should be typed as Float
        //   return r0
        let half_cell = LirCell {
            name: "half".to_string(),
            params: vec![LirParam {
                name: "x".to_string(),
                ty: "Int".to_string(),
                register: 0,
                variadic: false,
            }],
            returns: Some("Float".to_string()),
            registers: 4,
            constants: vec![Constant::Float(2.0)],
            instructions: vec![
                Instruction::abc(OpCode::Intrinsic, 1, 12, 0), // r1 = to_float(r0)
                Instruction::abx(OpCode::LoadK, 2, 0),         // r2 = 2.0
                Instruction::abc(OpCode::Div, 3, 1, 2),        // r3 = r1 / r2
                Instruction::abc(OpCode::Return, 3, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };

        let main_cell = LirCell {
            name: "main".to_string(),
            params: Vec::new(),
            returns: Some("Float".to_string()),
            registers: 3,
            constants: vec![Constant::String("half".to_string()), Constant::Int(7)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),     // r0 = "half"
                Instruction::abx(OpCode::LoadK, 1, 1),     // r1 = 7
                Instruction::abc(OpCode::Call, 0, 1, 1),   // r0 = half(7)
                Instruction::abc(OpCode::Return, 0, 1, 0), // return r0
            ],
            effect_handler_metas: Vec::new(),
        };

        let lir = make_module_with_cells(vec![half_cell, main_cell]);

        let settings = CodegenSettings::default();
        let mut engine = JitEngine::new(settings, 0);
        engine.compile_module(&lir).expect("compile");

        let raw = engine.execute_jit_nullary("main").expect("execute main");
        let result = f64::from_bits(raw as u64);
        assert!(
            (result - 3.5).abs() < 1e-10,
            "half(7) should return 3.5, got {result}"
        );
    }
}
