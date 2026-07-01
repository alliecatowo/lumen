//! NaN-boxed 64-bit value representation (NbValue).
//!
//! This module implements a compact, efficient value representation used by the VM register file
//! and JIT stencils. NaN boxing allows us to store multiple value types (integers, booleans,
//! null, pointers) within the IEEE 754 NaN space, while leaving normal f64 values as raw bits.
//!
//! # Bit Layout (64-bit IEEE 754 NaN Boxing)
//!
//! ```text
//!  63 62 61 60 59 58 57 56 55 54 53 52 51 50 49 48 47...0
//!  |--|--|--|--|--|--|--|--|--|--|--|--|--|--|--|--------|
//!   S  Q  Q  Q  Q  Q  Q  Q  Q  Q  Q  Q  T  T  T  T  PAYLOAD
//!
//! Legend:
//! - S (bit 63): Sign bit (ignored for NaN values)
//! - Q (bits 52-62): Quiet NaN bits - all set to 1 (0x7FF << 52)
//! - T (bits 48-51): 4-bit Type Tag (0-15)
//! - PAYLOAD (bits 0-47): 48-bit payload (pointer, integer, or boolean value)
//!
//! # Type Tags
//!
//! | Tag | Name  | Payload Meaning                          |
//! |-----|-------|------------------------------------------|
//! | 0   | PTR   | Pointer to heap-allocated HeapValue (Arc) |
//! | 1   | INT   | 48-bit signed integer (two's complement) |
//! | 2   | ATOM  | Atom ID (interned symbol)                |
//! | 3   | BOOL  | Boolean (0 = false, 1 = true)            |
//! | 4   | NULL  | Null value (payload ignored)             |
//! | 5   | FIBER | Fiber/pointer type for VM continuations  |
//!
//! # Special Values
//!
//! - Normal f64 floats: stored as raw bits (if not matching NAN_MASK)
//! - NaN floats: stored as TAG_PTR with payload = 1 (reserved sentinel)
//! - Null pointer: stored as TAG_PTR with payload = 0
//!
//! # Integer Range
//!
//! 48-bit signed integers can represent values from -140,737,488,355,328 to
//! +140,737,488,355,327 (±140 trillion). Values outside this range must be
//! heap-allocated as `HeapValue::BigInt`.
//!
//! # Safety Invariants
//!
//! - Pointer values MUST be 48-bit addressable (x86_64 guarantees this in user space)
//! - TAG_PTR values with payload > 1 are assumed to be valid `Arc<HeapValue>` pointers
//! - The implementation uses `Arc` for heap types to maintain reference counting
//!
//! # JIT Compatibility
//!
//! The encoding here must stay in lockstep with:
//! - `lumen-codegen/src/ir.rs`
//! - `lumen-codegen/src/union_helpers.rs`
//! - `lumen-codegen/src/stencils.rs`

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::hash::Hash;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::heap_value::{ClosureData, HeapValue, RecordData, UnionData};
use num_bigint::BigInt;

/// NaN-boxed 64-bit value used by the VM register file and JIT.
///
/// This is a newtype wrapper around `u64` that encodes various value types
/// using IEEE 754 NaN boxing. The representation is optimized for:
/// - Fast type checking (bitmask operations)
/// - Inline storage of small integers and booleans
/// - Direct use of f64 without conversion
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct NbValue(pub u64);

impl PartialEq for NbValue {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.cmp_total(*other) == Ordering::Equal
    }
}

impl Eq for NbValue {}

impl PartialOrd for NbValue {
    #[inline(always)]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp_total(*other))
    }
}

impl Ord for NbValue {
    #[inline(always)]
    fn cmp(&self, other: &Self) -> Ordering {
        self.cmp_total(*other)
    }
}

impl Hash for NbValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if self.is_int() {
            self.as_int().unwrap_or(0).hash(state);
            return;
        }
        if !self.is_nan_boxed() {
            self.0.hash(state);
            return;
        }
        if self.is_bool() {
            self.as_bool().hash(state);
            return;
        }
        if self.is_null() {
            0u8.hash(state);
            return;
        }
        if let Some(hv) = self.as_heap_ref() {
            std::mem::discriminant(hv).hash(state);
            hv.hash(state);
            return;
        }
        self.0.hash(state);
    }
}

impl NbValue {
    // ═════════════════════════════════════════════════════════════════════════
    // CONSTANTS - Bit masks and tag values
    // ═════════════════════════════════════════════════════════════════════════

    /// Quiet-NaN mask — bits 51-62 all set (0xFFF << 51).
    /// This identifies values that use NaN-boxed encoding.
    pub const NAN_MASK: u64 = 0x7FF8_0000_0000_0000;

    /// 48-bit payload mask — bits 0-47.
    pub const PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;

    /// Flag bit for arena-allocated heap pointers.
    /// When set in the payload, the pointer is arena-owned (no Arc refcount).
    pub const PTR_ARENA_FLAG: u64 = 1;

    /// Bit shift for tag storage (bit 48).
    pub const TAG_SHIFT: u64 = 48;

    /// 4-bit tag mask after shifting.
    pub const TAG_MASK: u64 = 0xF;

    // ═════════════════════════════════════════════════════════════════════════
    // TYPE TAGS (bits 48-51)
    // ═════════════════════════════════════════════════════════════════════════

    /// Tag for heap pointers (payload is raw pointer bits).
    /// Pointer values are `Arc<HeapValue>` allocations.
    pub const TAG_PTR: u64 = 0;

    /// Tag for 48-bit signed integers.
    /// Integers are stored as two's complement in the payload.
    pub const TAG_INT: u64 = 1;

    /// Tag for atoms (interned symbols).
    /// Payload is the atom ID.
    pub const TAG_ATOM: u64 = 2;

    /// Tag for booleans.
    /// Payload 0 = false, payload 1 = true.
    pub const TAG_BOOL: u64 = 3;

    /// Tag for null value.
    /// Payload is ignored (typically 0).
    pub const TAG_NULL: u64 = 4;

    // ═════════════════════════════════════════════════════════════════════════
    // INTEGER RANGE LIMITS
    // ═════════════════════════════════════════════════════════════════════════

    /// Minimum signed 48-bit integer: -(2^47) = -140,737,488,355,328
    pub const MIN_INT48: i64 = -(1i64 << 47);

    /// Maximum signed 48-bit integer: 2^47 - 1 = 140,737,488,355,327
    pub const MAX_INT48: i64 = (1i64 << 47) - 1;

    // ═════════════════════════════════════════════════════════════════════════
    // PRE-BUILT CONSTANTS (for fast access)
    // ═════════════════════════════════════════════════════════════════════════

    /// NaN-boxed null value.
    pub const NAN_BOX_NULL: u64 = Self::NAN_MASK | (Self::TAG_NULL << Self::TAG_SHIFT);

    /// NaN-boxed true value.
    pub const NAN_BOX_TRUE: u64 = Self::NAN_MASK | (Self::TAG_BOOL << Self::TAG_SHIFT) | 1;

    /// NaN-boxed false value.
    pub const NAN_BOX_FALSE: u64 = Self::NAN_MASK | (Self::TAG_BOOL << Self::TAG_SHIFT);

    /// NaN float sentinel (TAG_PTR with payload = 1).
    /// Used to distinguish actual NaN floats from NaN-boxed values.
    pub const NAN_FLOAT_SENTINEL: u64 = Self::NAN_MASK | 1;

    // ═════════════════════════════════════════════════════════════════════════
    // CONSTRUCTORS
    // ═════════════════════════════════════════════════════════════════════════

    /// Create a NaN-boxed null value.
    ///
    /// # Examples
    /// ```
    /// use lumen_core::nb_value::NbValue;
    ///
    /// let null = NbValue::new_null();
    /// assert!(null.is_null());
    /// assert!(!null.is_truthy());
    /// ```
    #[inline(always)]
    pub fn new_null() -> Self {
        NbValue(Self::NAN_BOX_NULL)
    }

    /// Create a NaN-boxed boolean value.
    ///
    /// # Examples
    /// ```
    /// use lumen_core::nb_value::NbValue;
    ///
    /// let t = NbValue::new_bool(true);
    /// let f = NbValue::new_bool(false);
    ///
    /// assert!(t.is_bool());
    /// assert!(t.is_truthy());
    /// assert!(!f.is_truthy());
    /// ```
    #[inline(always)]
    pub fn new_bool(value: bool) -> Self {
        if value {
            NbValue(Self::NAN_BOX_TRUE)
        } else {
            NbValue(Self::NAN_BOX_FALSE)
        }
    }

    /// Create a NaN-boxed 48-bit signed integer.
    ///
    /// # Panics
    ///
    /// Panics in debug mode if `value` is outside the 48-bit range
    /// (`MIN_INT48` to `MAX_INT48`). In release mode, the value is
    /// silently truncated (wraps around).
    ///
    /// For values outside this range, heap-box the value as BigInt.
    ///
    /// # Examples
    /// ```
    /// use lumen_core::nb_value::NbValue;
    ///
    /// let nb = NbValue::new_int(42);
    /// assert!(nb.is_int());
    /// assert_eq!(nb.as_int(), Some(42));
    /// ```
    #[inline(always)]
    pub fn new_int(value: i64) -> Self {
        debug_assert!(
            value >= Self::MIN_INT48 && value <= Self::MAX_INT48,
            "NbValue::new_int: value {} is outside 48-bit range ({} to {})",
            value,
            Self::MIN_INT48,
            Self::MAX_INT48
        );
        let payload = (value as u64) & Self::PAYLOAD_MASK;
        NbValue(Self::NAN_MASK | (Self::TAG_INT << Self::TAG_SHIFT) | payload)
    }

    /// Create a NaN-boxed BigInt value.
    #[inline(always)]
    pub fn new_bigint(value: BigInt) -> Self {
        NbValue::new_heap(HeapValue::BigInt(Arc::new(value)))
    }

    /// Create a NaN-boxed float value.
    ///
    /// Normal f64 values are stored as raw bits. If the value is a NaN
    /// (which would collide with our NaN-boxing scheme), it is stored
    /// as a special TAG_PTR sentinel with payload = 1.
    ///
    /// # Examples
    /// ```
    /// use lumen_core::nb_value::NbValue;
    ///
    /// let nb = NbValue::new_float(3.14159);
    /// assert!(nb.is_float());
    /// assert_eq!(nb.as_float(), Some(3.14159));
    /// ```
    #[inline(always)]
    pub fn new_float(value: f64) -> Self {
        let bits = value.to_bits();
        if (bits & Self::NAN_MASK) == Self::NAN_MASK {
            // This is a NaN float - store as reserved sentinel
            NbValue(Self::NAN_FLOAT_SENTINEL)
        } else {
            // Normal float - store raw bits
            NbValue(bits)
        }
    }

    /// Create a NaN-boxed pointer to a heap-allocated `HeapValue`.
    ///
    /// The value is wrapped in an `Arc` for reference counting.
    #[inline(always)]
    pub fn new_heap(value: HeapValue) -> Self {
        let ptr = Arc::into_raw(Arc::new(value));
        let addr = ptr as u64;
        debug_assert!(
            addr & !Self::PAYLOAD_MASK == 0,
            "NbValue::new_heap: pointer {:p} is not 48-bit addressable",
            ptr
        );
        NbValue(Self::NAN_MASK | (addr & Self::PAYLOAD_MASK))
    }

    /// Create a NaN-boxed string value.
    #[inline(always)]
    pub fn new_str(s: &str) -> Self {
        NbValue::new_heap(HeapValue::Str(Arc::from(s)))
    }

    /// Create a NaN-boxed bytes value.
    #[inline(always)]
    pub fn new_bytes(bytes: Arc<[u8]>) -> Self {
        NbValue::new_heap(HeapValue::Bytes(bytes))
    }

    /// Create a NaN-boxed set value.
    #[inline(always)]
    pub fn new_set(set: std::collections::BTreeSet<NbValue>) -> Self {
        NbValue::new_heap(HeapValue::Set(Arc::new(set)))
    }

    /// Create a NaN-boxed future value.
    #[inline(always)]
    pub fn new_future(future: crate::heap_value::FutureData) -> Self {
        NbValue::new_heap(HeapValue::Future(Arc::new(future)))
    }

    /// Create a NaN-boxed list value.
    #[inline(always)]
    pub fn new_list(elems: Vec<NbValue>) -> Self {
        NbValue::new_heap(HeapValue::List(Arc::new(elems)))
    }

    /// Create a NaN-boxed tuple value.
    #[inline(always)]
    pub fn new_tuple(elems: Vec<NbValue>) -> Self {
        NbValue::new_heap(HeapValue::Tuple(Arc::new(elems)))
    }

    /// Create a NaN-boxed map value.
    #[inline(always)]
    pub fn new_map(map: BTreeMap<String, NbValue>) -> Self {
        NbValue::new_heap(HeapValue::Map(Arc::new(map)))
    }

    /// Create a NaN-boxed record value.
    #[inline(always)]
    pub fn new_record(type_name: &str, fields: BTreeMap<String, NbValue>) -> Self {
        NbValue::new_heap(HeapValue::Record(Arc::new(RecordData {
            type_name: Arc::from(type_name),
            fields,
        })))
    }

    /// Create a NaN-boxed union value.
    #[inline(always)]
    pub fn new_union(tag: &str, payload: NbValue) -> Self {
        NbValue::new_union_arc(Arc::from(tag), payload)
    }

    /// Create a NaN-boxed union value from an existing Arc tag.
    ///
    /// This avoids allocating a new tag string when the caller already
    /// owns an `Arc<str>` (e.g., when reusing a tag from a string literal
    /// loaded into an NbValue register).
    #[inline(always)]
    pub fn new_union_arc(tag: Arc<str>, payload: NbValue) -> Self {
        NbValue::new_heap(HeapValue::Union(UnionData { tag, payload }))
    }

    /// Create a NaN-boxed closure value.
    #[inline(always)]
    pub fn new_closure(cell_idx: usize, captures: Vec<NbValue>) -> Self {
        NbValue::new_heap(HeapValue::Closure(Arc::new(ClosureData {
            cell_idx,
            captures,
        })))
    }

    // ═════════════════════════════════════════════════════════════════════════
    // TYPE CHECKERS (inline for speed)
    // ═════════════════════════════════════════════════════════════════════════

    /// Returns `true` if this value uses NaN-boxed encoding.
    ///
    /// A value is NaN-boxed if bits 51-62 all match the quiet-NaN pattern.
    #[inline(always)]
    pub fn is_nan_boxed(self) -> bool {
        (self.0 & Self::NAN_MASK) == Self::NAN_MASK
    }

    /// Returns `true` if this is an actual IEEE 754 float (not NaN-boxed).
    ///
    /// Note: This returns `false` for NaN-boxed values, even if they
    /// represent a logical float stored on the heap.
    #[inline(always)]
    pub fn is_float(self) -> bool {
        !self.is_nan_boxed()
    }

    /// Returns `true` if this is a NaN-boxed integer.
    #[inline(always)]
    pub fn is_int(self) -> bool {
        self.is_nan_boxed() && self.tag() == Self::TAG_INT
    }

    /// Returns `true` if this is a NaN-boxed boolean.
    #[inline(always)]
    pub fn is_bool(self) -> bool {
        self.is_nan_boxed() && self.tag() == Self::TAG_BOOL
    }

    /// Returns `true` if this is a NaN-boxed null value.
    #[inline(always)]
    pub fn is_null(self) -> bool {
        self.is_nan_boxed() && self.tag() == Self::TAG_NULL
    }

    /// Returns `true` if this is a NaN-boxed pointer.
    #[inline(always)]
    pub fn is_ptr(self) -> bool {
        self.is_nan_boxed() && self.tag() == Self::TAG_PTR
    }

    /// Returns `true` if this value is heap-allocated.
    ///
    /// This includes TAG_PTR values with payload > 1 (actual heap pointers).
    /// Null and NaN float sentinel are not considered heap-allocated.
    #[inline(always)]
    pub fn is_heap_allocated(self) -> bool {
        self.is_ptr() && self.payload() > 1 && (self.payload() & Self::PTR_ARENA_FLAG == 0)
    }

    /// Returns `true` if this value is arena-allocated.
    #[inline(always)]
    pub fn is_arena_allocated(self) -> bool {
        self.is_ptr() && self.payload() > 1 && (self.payload() & Self::PTR_ARENA_FLAG != 0)
    }

    // ═════════════════════════════════════════════════════════════════════════
    // EXTRACTORS
    // ═════════════════════════════════════════════════════════════════════════

    /// Return the 3-bit tag value.
    ///
    /// For non-NaN-boxed values, returns 0 (which coincides with TAG_PTR).
    /// Note: We only use 3 tag bits (48-50) because bit 51 is part of the
    /// quiet-NaN pattern. This gives us 8 possible tag values (0-7).
    #[inline(always)]
    pub fn tag(self) -> u64 {
        // Extract bits 50-48 as the tag
        (self.0 >> Self::TAG_SHIFT) & 0x7
    }

    /// Return the 48-bit payload.
    #[inline(always)]
    pub fn payload(self) -> u64 {
        self.0 & Self::PAYLOAD_MASK
    }

    /// Extract a NaN-boxed integer as `i64`.
    ///
    /// Sign-extends the 48-bit two's complement payload to 64 bits.
    /// Returns `None` if this is not an integer.
    #[inline(always)]
    pub fn as_int(self) -> Option<i64> {
        if !self.is_int() {
            return None;
        }
        let raw = self.payload();
        // Sign-extend from 48 bits to 64 bits
        Some(if raw & (1 << 47) != 0 {
            (raw | !Self::PAYLOAD_MASK) as i64
        } else {
            raw as i64
        })
    }

    /// Extract a NaN-boxed boolean.
    ///
    /// Returns `None` if this is not a boolean.
    #[inline(always)]
    pub fn as_bool(self) -> Option<bool> {
        if !self.is_bool() {
            return None;
        }
        Some(self.payload() != 0)
    }

    /// Extract a NaN-boxed float.
    ///
    /// Returns `None` if this is not a raw float (i.e., if it's NaN-boxed).
    /// Note: Heap-allocated floats return `None`.
    #[inline(always)]
    pub fn as_float(self) -> Option<f64> {
        if self.is_nan_boxed() {
            return None;
        }
        Some(f64::from_bits(self.0))
    }

    /// Extract a NaN-boxed pointer.
    ///
    /// Returns `None` if this is not a pointer (TAG_PTR).
    ///
    /// # Safety
    ///
    /// The returned pointer may be invalid if the payload is 0 or 1 (sentinels).
    /// Only use this when you've verified `is_heap_allocated()` is true.
    #[inline(always)]
    pub fn as_pointer<T>(self) -> Option<*const T> {
        if !self.is_ptr() {
            return None;
        }
        let payload = self.payload();
        if payload <= 1 {
            return None;
        }
        Some((payload & !Self::PTR_ARENA_FLAG) as *const T)
    }

    // ═════════════════════════════════════════════════════════════════════════
    // UTILITY METHODS
    // ═════════════════════════════════════════════════════════════════════════

    /// Drop a heap allocation if this is a TAG_PTR value with payload > 1.
    ///
    /// # Safety
    ///
    /// This should only be called when you're done with the NbValue and
    /// want to release the underlying Arc. After calling this, the NbValue
    /// should not be used.
    pub fn drop_heap(self) {
        if !self.is_heap_allocated() {
            return;
        }
        let payload = self.payload();
        unsafe {
            let ptr = (payload & !Self::PTR_ARENA_FLAG) as *const HeapValue;
            drop(Arc::from_raw(ptr));
        }
    }

    /// Increment the Arc reference count for heap-allocated values.
    /// No-op for inline types (int, bool, null, float).
    /// This is used when copying an NbValue to a new location (e.g., register copy).
    #[inline(always)]
    pub fn inc_ref(self) {
        if self.is_heap_allocated() {
            unsafe {
                let ptr = (self.payload() & !Self::PTR_ARENA_FLAG) as *const HeapValue;
                Arc::increment_strong_count(ptr);
            }
        }
    }

    /// Get a reference to the heap-allocated HeapValue without cloning or touching refcounts.
    /// Returns None for inline types (int, bool, null, float) and special pointer values.
    ///
    /// # Safety
    /// The returned reference is valid as long as this NbValue (or its register) is alive.
    /// Do not call drop_heap() or overwrite the register while holding this reference.
    #[inline(always)]
    pub fn as_heap_ref(&self) -> Option<&HeapValue> {
        if !self.is_nan_boxed() {
            return None; // Raw float
        }
        if self.tag() != Self::TAG_PTR {
            return None; // Int, Bool, Null, Atom, Fiber
        }
        let payload = self.payload();
        if payload <= 1 {
            return None; // Null pointer or NaN sentinel
        }
        unsafe {
            let ptr = (payload & !Self::PTR_ARENA_FLAG) as *const HeapValue;
            Some(&*ptr)
        }
    }

    /// Get an owned Arc for the heap-allocated value.
    /// Returns None for inline types (int, bool, null, float) and special pointer values.
    #[inline(always)]
    pub fn as_heap_mut(&self) -> Option<Arc<HeapValue>> {
        if !self.is_nan_boxed() {
            return None; // Raw float
        }
        if self.tag() != Self::TAG_PTR {
            return None; // Int, Bool, Null, Atom, Fiber
        }
        let payload = self.payload();
        if payload <= 1 {
            return None; // Null pointer or NaN sentinel
        }
        unsafe {
            let ptr = (payload & !Self::PTR_ARENA_FLAG) as *const HeapValue;
            Arc::increment_strong_count(ptr);
            Some(Arc::from_raw(ptr))
        }
    }

    /// Returns `true` if this value is truthy.
    pub fn is_truthy(self) -> bool {
        // Handle raw floats first
        if !self.is_nan_boxed() {
            let f = f64::from_bits(self.0);
            return f != 0.0 && !f.is_nan();
        }

        match self.tag() {
            Self::TAG_NULL => false,
            Self::TAG_BOOL => self.payload() != 0,
            Self::TAG_INT => self.as_int().map_or(true, |n| n != 0),
            Self::TAG_PTR => match self.payload() {
                0 => false,
                1 => false,
                _ => self.as_heap_ref().map_or(true, |hv| hv.is_truthy()),
            },
            _ => true,
        }
    }

    /// Return the type name as a static string.
    ///
    /// This is useful for debugging and error messages.
    pub fn type_name(self) -> &'static str {
        if !self.is_nan_boxed() {
            return "Float";
        }
        match self.tag() {
            Self::TAG_PTR => match self.payload() {
                0 => "Null",
                1 => "Float", // NaN sentinel
                _ => self.as_heap_ref().map_or("Heap", |hv| hv.type_name()),
            },
            Self::TAG_INT => "Int",
            Self::TAG_BOOL => "Bool",
            Self::TAG_NULL => "Null",
            _ => "Unknown",
        }
    }

    /// Display this value without converting to legacy Value.
    pub fn display(self) -> String {
        if self.is_int() {
            return self.as_int().unwrap_or(0).to_string();
        }
        if !self.is_nan_boxed() {
            return format!("{}", f64::from_bits(self.0));
        }
        if self.is_bool() {
            return if self.payload() != 0 {
                "true".to_string()
            } else {
                "false".to_string()
            };
        }
        if self.is_null() {
            return "null".to_string();
        }
        if let Some(hv) = self.as_heap_ref() {
            return hv.display();
        }
        "null".to_string()
    }

    /// Extract a string from heap if this is a Str variant.
    pub fn as_str_ref(self) -> Option<Arc<str>> {
        self.as_heap_ref().and_then(|hv| match hv {
            HeapValue::Str(s) => Some(Arc::clone(s)),
            _ => None,
        })
    }

    /// Extract list reference from heap if this is a List variant.
    pub fn as_list_ref(self) -> Option<Arc<Vec<NbValue>>> {
        self.as_heap_ref().and_then(|hv| match hv {
            HeapValue::List(l) => Some(Arc::clone(l)),
            _ => None,
        })
    }

    /// Extract map reference from heap if this is a Map variant.
    pub fn as_map_ref(self) -> Option<Arc<BTreeMap<String, NbValue>>> {
        self.as_heap_ref().and_then(|hv| match hv {
            HeapValue::Map(m) => Some(Arc::clone(m)),
            _ => None,
        })
    }

    /// Extract record reference from heap if this is a Record variant.
    pub fn as_record_ref(self) -> Option<Arc<RecordData>> {
        self.as_heap_ref().and_then(|hv| match hv {
            HeapValue::Record(r) => Some(Arc::clone(r)),
            _ => None,
        })
    }

    /// Extract union data from heap if this is a Union variant.
    pub fn as_union_ref(self) -> Option<UnionData> {
        self.as_heap_ref().and_then(|hv| match hv {
            HeapValue::Union(u) => Some(u.clone()),
            _ => None,
        })
    }

    /// Get the raw u64 bits.
    #[inline(always)]
    pub fn to_bits(self) -> u64 {
        self.0
    }

    /// Create from raw u64 bits.
    ///
    /// # Safety
    ///
    /// The bits must form a valid NbValue encoding. Invalid bit patterns
    /// may cause panics or undefined behavior when the value is used.
    #[inline(always)]
    pub const fn from_bits(bits: u64) -> Self {
        NbValue(bits)
    }

    /// Compare to another NbValue for total ordering.
    #[inline(always)]
    fn cmp_total(self, other: NbValue) -> Ordering {
        if self.is_null() && other.is_null() {
            return Ordering::Equal;
        }
        if self.is_null() {
            return Ordering::Less;
        }
        if other.is_null() {
            return Ordering::Greater;
        }

        if self.is_int() && other.is_int() {
            return self.as_int().unwrap_or(0).cmp(&other.as_int().unwrap_or(0));
        }
        if !self.is_nan_boxed() && !other.is_nan_boxed() {
            return self.0.cmp(&other.0);
        }
        if self.is_bool() && other.is_bool() {
            return self.as_bool().cmp(&other.as_bool());
        }

        // Heap values: compare by discriminant then by HeapValue::cmp
        match (self.as_heap_ref(), other.as_heap_ref()) {
            (Some(a), Some(b)) => a.cmp(b),
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            (None, None) => self.0.cmp(&other.0),
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// TRAIT IMPLEMENTATIONS
// ═════════════════════════════════════════════════════════════════════════════

impl Default for NbValue {
    fn default() -> Self {
        Self::new_null()
    }
}

impl Serialize for NbValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.display())
    }
}

impl<'de> Deserialize<'de> for NbValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(NbValue::new_str(&value))
    }
}

impl std::fmt::Debug for NbValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.is_nan_boxed() {
            return f
                .debug_tuple("NbValue::Float")
                .field(&f64::from_bits(self.0))
                .finish();
        }
        match self.tag() {
            Self::TAG_INT => f.debug_tuple("NbValue::Int").field(&self.as_int()).finish(),
            Self::TAG_BOOL => f
                .debug_tuple("NbValue::Bool")
                .field(&(self.payload() != 0))
                .finish(),
            Self::TAG_NULL => f.debug_tuple("NbValue::Null").finish(),
            Self::TAG_PTR => {
                let payload = self.payload();
                match payload {
                    0 => f.debug_tuple("NbValue::Null").finish(),
                    1 => f.debug_tuple("NbValue::Float(NaN)").finish(),
                    _ => f
                        .debug_tuple("NbValue::Ptr")
                        .field(&format!("0x{:012x}", payload))
                        .finish(),
                }
            }
            tag => f
                .debug_tuple(&format!("NbValue::Unknown({})", tag))
                .field(&self.0)
                .finish(),
        }
    }
}

impl std::fmt::Display for NbValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// UNIT TESTS
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod heap_value_tests {
    use super::NbValue;

    #[test]
    fn test_heap_display_and_type_name() {
        let s = NbValue::new_str("hello");
        assert_eq!(s.display(), "hello");
        assert_eq!(s.type_name(), "String");
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// REGISTER FILE — refcount-safe Vec<NbValue> wrapper
// ═════════════════════════════════════════════════════════════════════════════

/// A refcount-safe wrapper around `Vec<NbValue>`.
///
/// Raw `Vec<NbValue>` is unsound for heap-allocated NbValues because:
/// - `Vec::clone()` copies raw u64 bits without incrementing Arc refcounts
/// - `Vec::drop()` deallocates the buffer without decrementing Arc refcounts
/// - Assignment (`vec = other_vec`) drops the old Vec without cleanup
///
/// `RegisterFile` fixes all three by implementing custom `Clone` and `Drop`
/// that properly manage Arc refcounts for heap-allocated NbValues (TAG_PTR
/// with payload > 1).
///
/// It implements `Deref<Target=[NbValue]>` and `DerefMut` so all existing
/// `self.registers[idx]`, `.len()`, `.iter()`, slice operations, etc. work
/// transparently without changing call sites.
pub struct RegisterFile {
    inner: Vec<NbValue>,
}

impl RegisterFile {
    /// Create an empty register file.
    #[inline]
    pub fn new() -> Self {
        RegisterFile { inner: Vec::new() }
    }

    /// Create an empty register file with pre-allocated capacity.
    #[inline]
    pub fn with_capacity(cap: usize) -> Self {
        RegisterFile {
            inner: Vec::with_capacity(cap),
        }
    }

    /// Resize the register file, initializing new slots with the given value.
    /// Properly drops heap allocations in truncated slots.
    #[inline]
    pub fn resize(&mut self, new_len: usize, value: NbValue) {
        if new_len < self.inner.len() {
            // Shrinking: drop heap allocations in removed slots
            for r in &self.inner[new_len..] {
                r.drop_heap();
            }
        }
        self.inner.resize(new_len, value);
    }

    /// Reserve additional capacity without changing length.
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.inner.reserve(additional);
    }

    /// Return the capacity of the underlying Vec.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    /// Return a raw mutable pointer to the underlying buffer.
    /// # Safety
    /// Caller must not violate refcount invariants.
    #[inline]
    pub unsafe fn as_mut_ptr(&mut self) -> *mut NbValue {
        self.inner.as_mut_ptr()
    }

    /// Replace the register file contents, properly dropping old heap allocations
    /// and incrementing refcounts for the new values.
    ///
    /// This is the safe equivalent of `self.registers = other_vec` that was
    /// previously causing refcount leaks.
    pub fn replace_from_vec(&mut self, new_regs: Vec<NbValue>) {
        // Drop old heap allocations
        for r in &self.inner {
            r.drop_heap();
        }
        // Inc ref on new values
        for r in &new_regs {
            r.inc_ref();
        }
        self.inner = new_regs;
    }

    /// Take the register file contents, leaving an empty file.
    /// The returned Vec owns the heap allocations (no refcount changes).
    /// The caller is responsible for eventually calling drop_heap on each element
    /// or passing it to `replace_from_raw_vec`.
    pub fn take_raw(&mut self) -> Vec<NbValue> {
        std::mem::take(&mut self.inner)
    }

    /// Replace registers from a raw Vec without touching refcounts.
    /// The caller guarantees that the Vec's NbValues already have correct refcounts.
    /// This is used for paired take_raw/replace_from_raw_vec save/restore patterns.
    pub fn replace_from_raw_vec(&mut self, raw: Vec<NbValue>) {
        // Drop old heap allocations
        for r in &self.inner {
            r.drop_heap();
        }
        self.inner = raw;
    }
}

impl Default for RegisterFile {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for RegisterFile {
    fn clone(&self) -> Self {
        // Clone: inc_ref on every heap-allocated NbValue so the clone
        // has its own ownership stake in each Arc.
        for nb in &self.inner {
            nb.inc_ref();
        }
        RegisterFile {
            inner: self.inner.clone(),
        }
    }
}

impl Drop for RegisterFile {
    fn drop(&mut self) {
        for nb in &self.inner {
            nb.drop_heap();
        }
    }
}

impl std::ops::Deref for RegisterFile {
    type Target = [NbValue];
    #[inline(always)]
    fn deref(&self) -> &[NbValue] {
        &self.inner
    }
}

impl std::ops::DerefMut for RegisterFile {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut [NbValue] {
        &mut self.inner
    }
}

impl std::ops::Index<usize> for RegisterFile {
    type Output = NbValue;
    #[inline(always)]
    fn index(&self, idx: usize) -> &NbValue {
        &self.inner[idx]
    }
}

impl std::ops::IndexMut<usize> for RegisterFile {
    #[inline(always)]
    fn index_mut(&mut self, idx: usize) -> &mut NbValue {
        &mut self.inner[idx]
    }
}

impl std::ops::Index<std::ops::Range<usize>> for RegisterFile {
    type Output = [NbValue];
    #[inline(always)]
    fn index(&self, range: std::ops::Range<usize>) -> &[NbValue] {
        &self.inner[range]
    }
}

impl std::ops::IndexMut<std::ops::Range<usize>> for RegisterFile {
    #[inline(always)]
    fn index_mut(&mut self, range: std::ops::Range<usize>) -> &mut [NbValue] {
        &mut self.inner[range]
    }
}

impl std::ops::Index<std::ops::RangeFrom<usize>> for RegisterFile {
    type Output = [NbValue];
    #[inline(always)]
    fn index(&self, range: std::ops::RangeFrom<usize>) -> &[NbValue] {
        &self.inner[range]
    }
}

impl std::fmt::Debug for RegisterFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegisterFile")
            .field("len", &self.inner.len())
            .field("capacity", &self.inner.capacity())
            .finish()
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// UNIT TESTS
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::NbValue;

    // ═════════════════════════════════════════════════════════════════════════
    // Constructor/Extractor Round-trip Tests
    // ═════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_null_roundtrip() {
        let null = NbValue::new_null();
        assert!(null.is_null());
        assert!(null.is_nan_boxed());
        assert_eq!(null.tag(), NbValue::TAG_NULL);
        assert!(!null.is_truthy());
    }

    #[test]
    fn test_bool_roundtrip() {
        let t = NbValue::new_bool(true);
        let f = NbValue::new_bool(false);

        assert!(t.is_bool());
        assert!(f.is_bool());
        assert!(!t.is_null());
        assert!(!f.is_null());

        assert_eq!(t.as_bool(), Some(true));
        assert_eq!(f.as_bool(), Some(false));

        assert!(t.is_truthy());
        assert!(!f.is_truthy());
    }

    #[test]
    fn test_int_roundtrip() {
        // Test zero
        let zero = NbValue::new_int(0);
        assert!(zero.is_int());
        assert_eq!(zero.as_int(), Some(0));
        assert!(!zero.is_truthy());

        // Test positive
        let pos = NbValue::new_int(42);
        assert!(pos.is_int());
        assert_eq!(pos.as_int(), Some(42));
        assert!(pos.is_truthy());

        // Test negative
        let neg = NbValue::new_int(-100);
        assert!(neg.is_int());
        assert_eq!(neg.as_int(), Some(-100));
        assert!(neg.is_truthy());

        // Test max value
        let max = NbValue::new_int(NbValue::MAX_INT48);
        assert_eq!(max.as_int(), Some(NbValue::MAX_INT48));

        // Test min value
        let min = NbValue::new_int(NbValue::MIN_INT48);
        assert_eq!(min.as_int(), Some(NbValue::MIN_INT48));
    }

    #[test]
    fn test_float_roundtrip() {
        // Normal float
        let f = NbValue::new_float(3.14159);
        assert!(f.is_float());
        assert!(!f.is_nan_boxed());
        assert_eq!(f.as_float(), Some(3.14159));

        // Zero
        let zero = NbValue::new_float(0.0);
        assert!(zero.is_float());
        assert!(!zero.is_truthy());

        // Negative zero
        let neg_zero = NbValue::new_float(-0.0);
        assert!(neg_zero.is_float());
        assert!(!neg_zero.is_truthy());

        // NaN is boxed
        let nan = NbValue::new_float(f64::NAN);
        assert!(!nan.is_float()); // It's NaN-boxed
        assert!(nan.is_nan_boxed());
        assert!(nan.is_ptr()); // Stored as TAG_PTR sentinel
        assert_eq!(nan.payload(), 1);

        // Infinity
        let inf = NbValue::new_float(f64::INFINITY);
        assert!(inf.is_float());
        assert!(inf.is_truthy());
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Edge Case Tests
    // ═════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_type_names() {
        assert_eq!(NbValue::new_null().type_name(), "Null");
        assert_eq!(NbValue::new_bool(true).type_name(), "Bool");
        assert_eq!(NbValue::new_int(42).type_name(), "Int");
        assert_eq!(NbValue::new_float(1.0).type_name(), "Float");

        let ptr = NbValue::new_str("hi");
        assert_eq!(ptr.type_name(), "String");
    }

    #[test]
    fn test_truthiness_edge_cases() {
        // Integers
        assert!(!NbValue::new_int(0).is_truthy());
        assert!(NbValue::new_int(1).is_truthy());
        assert!(NbValue::new_int(-1).is_truthy());

        // Floats
        assert!(!NbValue::new_float(0.0).is_truthy());
        assert!(!NbValue::new_float(-0.0).is_truthy());
        assert!(NbValue::new_float(1.0).is_truthy());
        assert!(NbValue::new_float(-1.0).is_truthy());

        // NaN is not truthy
        let nan = NbValue::new_float(f64::NAN);
        assert!(!nan.is_truthy());

        // Null is not truthy
        assert!(!NbValue::new_null().is_truthy());

        // Booleans
        assert!(!NbValue::new_bool(false).is_truthy());
        assert!(NbValue::new_bool(true).is_truthy());

        // Heap pointer is truthy
        let ptr = NbValue::new_str("truthy");
        assert!(ptr.is_truthy());
    }

    #[test]
    fn test_default_is_null() {
        let nb: NbValue = Default::default();
        assert!(nb.is_null());
    }

    #[test]
    fn test_display_formatting() {
        assert_eq!(format!("{}", NbValue::new_null()), "null");
        assert_eq!(format!("{}", NbValue::new_bool(true)), "true");
        assert_eq!(format!("{}", NbValue::new_bool(false)), "false");
        assert_eq!(format!("{}", NbValue::new_int(42)), "42");
    }

    #[test]
    fn test_debug_formatting() {
        // Test that Debug doesn't panic
        let _ = format!("{:?}", NbValue::new_null());
        let _ = format!("{:?}", NbValue::new_bool(true));
        let _ = format!("{:?}", NbValue::new_int(42));
        let _ = format!("{:?}", NbValue::new_float(3.14));
    }

    #[test]
    fn test_eq_trait() {
        let a = NbValue::new_int(42);
        let b = NbValue::new_int(42);
        let c = NbValue::new_int(43);

        assert_eq!(a, b);
        assert_ne!(a, c);

        // Same bits, different types
        let null1 = NbValue::new_null();
        let null2 = NbValue::new_null();
        assert_eq!(null1, null2);
    }

    #[test]
    fn test_from_bits_to_bits() {
        let original = NbValue::new_int(12345);
        let bits = original.to_bits();
        let reconstructed = NbValue::from_bits(bits);
        assert_eq!(original, reconstructed);
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Property-based sanity checks
    // ═════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_int_range_properties() {
        // Verify the 48-bit range constants
        assert_eq!(NbValue::MAX_INT48, 140_737_488_355_327);
        assert_eq!(NbValue::MIN_INT48, -140_737_488_355_328);

        // Verify 2^47 relationship
        assert_eq!(NbValue::MAX_INT48, (1i64 << 47) - 1);
        assert_eq!(NbValue::MIN_INT48, -(1i64 << 47));
    }

    #[test]
    fn test_all_tags_are_different() {
        let tags = [
            NbValue::TAG_PTR,
            NbValue::TAG_INT,
            NbValue::TAG_ATOM,
            NbValue::TAG_BOOL,
            NbValue::TAG_NULL,
        ];
        let unique: std::collections::HashSet<_> = tags.iter().collect();
        assert_eq!(tags.len(), unique.len(), "All tag values must be unique");
    }

    #[test]
    fn test_nan_mask_properties() {
        // Verify NAN_MASK has bits 51-62 set (quiet NaN pattern)
        assert_eq!(NbValue::NAN_MASK, 0x7FF8_0000_0000_0000);

        // A value with NAN_MASK bits set AND some payload should be NaN-boxed
        // Note: We need payload != 0 to distinguish from infinity
        let nan_boxed = NbValue(NbValue::NAN_MASK | 1);
        assert!(nan_boxed.is_nan_boxed());
    }
}
