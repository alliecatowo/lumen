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
//! | 0   | PTR   | Pointer to heap-allocated Value (Arc)    |
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
//! heap-allocated as `Value::BigInt`.
//!
//! # Safety Invariants
//!
//! - Pointer values MUST be 48-bit addressable (x86_64 guarantees this in user space)
//! - TAG_PTR values with payload > 1 are assumed to be valid `Arc<Value>` pointers
//! - The implementation uses `Arc` for heap types to maintain reference counting
//!
//! # JIT Compatibility
//!
//! The encoding here must stay in lockstep with:
//! - `lumen-codegen/src/ir.rs`
//! - `lumen-codegen/src/union_helpers.rs`
//! - `lumen-codegen/src/stencils.rs`

use std::sync::Arc;

use crate::values::Value;

/// NaN-boxed 64-bit value used by the VM register file and JIT.
///
/// This is a newtype wrapper around `u64` that encodes various value types
/// using IEEE 754 NaN boxing. The representation is optimized for:
/// - Fast type checking (bitmask operations)
/// - Inline storage of small integers and booleans
/// - Direct use of f64 without conversion
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct NbValue(pub u64);

impl NbValue {
    // ═════════════════════════════════════════════════════════════════════════
    // CONSTANTS - Bit masks and tag values
    // ═════════════════════════════════════════════════════════════════════════

    /// Quiet-NaN mask — bits 51-62 all set (0xFFF << 51).
    /// This identifies values that use NaN-boxed encoding.
    pub const NAN_MASK: u64 = 0x7FF8_0000_0000_0000;

    /// 48-bit payload mask — bits 0-47.
    pub const PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;

    /// Bit shift for tag storage (bit 48).
    pub const TAG_SHIFT: u64 = 48;

    /// 4-bit tag mask after shifting.
    pub const TAG_MASK: u64 = 0xF;

    // ═════════════════════════════════════════════════════════════════════════
    // TYPE TAGS (bits 48-51)
    // ═════════════════════════════════════════════════════════════════════════

    /// Tag for heap pointers (payload is raw pointer bits).
    /// Pointer values are `Arc<Value>` allocations.
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
    /// For values outside this range, use `from_legacy()` with `Value::BigInt`.
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

    /// Create a NaN-boxed pointer to a heap-allocated `Value`.
    ///
    /// The pointer is wrapped in an `Arc` for reference counting.
    /// This consumes the pointer - the caller should not use it afterward.
    ///
    /// # Safety
    ///
    /// The pointer must be a valid, non-null pointer obtained from `Arc::into_raw()`.
    /// It must be 48-bit addressable (upper 16 bits must be zero).
    ///
    /// # Panics
    ///
    /// Panics in debug mode if the pointer has bits set in positions 48-63.
    #[inline(always)]
    pub fn new_ptr(ptr: *const Value) -> Self {
        let addr = ptr as u64;
        debug_assert!(
            addr & !Self::PAYLOAD_MASK == 0,
            "NbValue::new_ptr: pointer {:p} is not 48-bit addressable",
            ptr
        );
        NbValue(Self::NAN_MASK | (addr & Self::PAYLOAD_MASK))
    }

    /// Create a NaN-boxed value from a raw pointer to any type.
    ///
    /// This is a convenience wrapper around `new_ptr` for generic pointers.
    /// The pointer will be cast to `*const Value`.
    ///
    /// # Safety
    ///
    /// See `new_ptr` for safety requirements.
    #[inline(always)]
    pub fn from_pointer<T>(ptr: *const T) -> Self {
        Self::new_ptr(ptr as *const Value)
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
        self.is_ptr() && self.payload() > 1
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
    /// Note: Heap-allocated floats (TAG_PTR to Value::Float) return `None`;
    /// use `to_legacy()` to access those.
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
        Some(self.payload() as *const T)
    }

    // ═════════════════════════════════════════════════════════════════════════
    // CONVERSION TO/FROM LEGACY Value ENUM
    // ═════════════════════════════════════════════════════════════════════════

    /// Convert a legacy `Value` to `NbValue`, heap-boxing when necessary.
    ///
    /// This is the main entry point for transitioning from the old Value
    /// representation to the new NaN-boxed representation.
    ///
    /// # Conversion Table
    /// | Value variant | NbValue representation |
    /// |---------------|------------------------|
    /// | Null          | TAG_NULL               |
    /// | Bool(b)       | TAG_BOOL               |
    /// | Int(i)        | TAG_INT (if in range)  |
    /// | Int(i)        | TAG_PTR → Value::Int   |
    /// | BigInt        | TAG_PTR → Value::BigInt|
    /// | Float(f)      | Raw bits (or sentinel) |
    /// | Other         | TAG_PTR → Value        |
    ///
    /// # Examples
    /// ```
    /// use lumen_core::nb_value::NbValue;
    /// use lumen_core::values::Value;
    ///
    /// let nb = NbValue::from_legacy(Value::Int(42));
    /// assert!(nb.is_int());
    /// assert_eq!(nb.as_int(), Some(42));
    /// ```
    pub fn from_legacy(value: Value) -> Self {
        match value {
            Value::Null => NbValue::new_null(),
            Value::Bool(b) => NbValue::new_bool(b),
            Value::Int(n) => {
                if n >= Self::MIN_INT48 && n <= Self::MAX_INT48 {
                    NbValue::new_int(n)
                } else {
                    // Large integer - heap box it
                    NbValue::new_ptr(Arc::into_raw(Arc::new(Value::Int(n))))
                }
            }
            Value::Float(f) => NbValue::new_float(f),
            // All other types are heap-allocated
            other => NbValue::new_ptr(Arc::into_raw(Arc::new(other))),
        }
    }

    /// Convert `NbValue` to legacy `Value`, consuming heap allocations.
    ///
    /// This is the inverse of `from_legacy()`. For TAG_PTR values with
    /// payload > 1, this consumes the Arc (decrements reference count).
    ///
    /// To peek at the value without consuming, use `peek_legacy()`.
    pub fn to_legacy(self) -> Value {
        if !self.is_nan_boxed() {
            // Raw float bits
            return Value::Float(f64::from_bits(self.0));
        }

        match self.tag() {
            Self::TAG_INT => Value::Int(self.as_int().unwrap_or(0)),
            Self::TAG_BOOL => Value::Bool(self.payload() != 0),
            Self::TAG_NULL => Value::Null,
            Self::TAG_PTR => match self.payload() {
                0 => Value::Null,
                1 => Value::Float(f64::NAN),
                payload => unsafe {
                    let ptr = payload as *const Value;
                    let arc = Arc::from_raw(ptr);
                    // Try to unwrap; if shared, clone and drop our ref
                    match Arc::try_unwrap(arc) {
                        Ok(value) => value,
                        Err(arc) => {
                            let value = (*arc).clone();
                            drop(arc);
                            value
                        }
                    }
                },
            },
            _ => Value::Null, // Unknown tag
        }
    }

    /// Convert `NbValue` to legacy `Value` without consuming heap allocations.
    ///
    /// This clones the underlying value for heap-allocated types,
    /// keeping the original Arc alive.
    pub fn peek_legacy(self) -> Value {
        if !self.is_nan_boxed() {
            return Value::Float(f64::from_bits(self.0));
        }

        match self.tag() {
            Self::TAG_INT => Value::Int(self.as_int().unwrap_or(0)),
            Self::TAG_BOOL => Value::Bool(self.payload() != 0),
            Self::TAG_NULL => Value::Null,
            Self::TAG_PTR => match self.payload() {
                0 => Value::Null,
                1 => Value::Float(f64::NAN),
                payload => unsafe {
                    let ptr = payload as *const Value;
                    // Increment ref count, then decrement via Arc::from_raw
                    Arc::increment_strong_count(ptr);
                    let arc = Arc::from_raw(ptr);
                    let value = (*arc).clone();
                    drop(arc);
                    value
                },
            },
            _ => Value::Null,
        }
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
            let ptr = payload as *const Value;
            drop(Arc::from_raw(ptr));
        }
    }

    /// Returns `true` if this value is truthy.
    ///
    /// Truthiness rules:
    /// - Null: false
    /// - Bool(b): b
    /// - Int(n): n != 0
    /// - Float(f): f != 0.0 && !f.is_nan()
    /// - Everything else: true (including empty collections)
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
            Self::TAG_PTR => {
                // NaN sentinel (payload = 1) is not truthy
                // Null sentinel (payload = 0) is not truthy
                // Other pointers are truthy
                self.payload() > 1
            }
            _ => true, // Atoms, fibers, and other types are truthy
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
            Self::TAG_PTR => {
                match self.payload() {
                    0 => "Null",
                    1 => "Float", // NaN sentinel
                    _ => "Pointer",
                }
            }
            Self::TAG_INT => "Int",
            Self::TAG_BOOL => "Bool",
            Self::TAG_NULL => "Null",
            _ => "Unknown",
        }
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
}

// ═════════════════════════════════════════════════════════════════════════════
// TRAIT IMPLEMENTATIONS
// ═════════════════════════════════════════════════════════════════════════════

impl Default for NbValue {
    fn default() -> Self {
        Self::new_null()
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
        match self.tag() {
            _ if !self.is_nan_boxed() => write!(f, "{}", f64::from_bits(self.0)),
            Self::TAG_INT => write!(f, "{}", self.as_int().unwrap_or(0)),
            Self::TAG_BOOL => write!(f, "{}", self.payload() != 0),
            Self::TAG_NULL => write!(f, "null"),
            Self::TAG_PTR => match self.payload() {
                0 => write!(f, "null"),
                1 => write!(f, "NaN"),
                _ => write!(f, "<ptr:{:012x}>", self.payload()),
            },
            _ => write!(f, "<unknown:{:016x}>", self.0),
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// UNIT TESTS
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::NbValue;
    use crate::values::{StringRef, Value};

    // ═════════════════════════════════════════════════════════════════════════
    // Constructor/Extractor Round-trip Tests
    // ═════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_null_roundtrip() {
        let null = NbValue::new_null();
        assert!(null.is_null());
        assert!(null.is_nan_boxed());
        assert_eq!(null.tag(), NbValue::TAG_NULL);
        assert_eq!(null.to_legacy(), Value::Null);
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

        assert_eq!(t.to_legacy(), Value::Bool(true));
        assert_eq!(f.to_legacy(), Value::Bool(false));

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
        assert_eq!(f.to_legacy(), Value::Float(3.14159));

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
        assert_eq!(nan.to_legacy(), Value::Float(f64::NAN));

        // Infinity
        let inf = NbValue::new_float(f64::INFINITY);
        assert!(inf.is_float());
        assert!(inf.is_truthy());
    }

    #[test]
    fn test_pointer_roundtrip() {
        let value = Value::String(StringRef::Owned("hello".to_string()));
        let nb = NbValue::from_legacy(value.clone());

        assert!(nb.is_ptr());
        assert!(nb.is_nan_boxed());
        assert!(nb.is_heap_allocated());
        assert!(nb.payload() > 1);

        // Peek doesn't consume
        let peeked = nb.peek_legacy();
        assert_eq!(peeked, value);

        // Second peek should work
        let peeked2 = nb.peek_legacy();
        assert_eq!(peeked2, value);

        // To consume
        let consumed = nb.to_legacy();
        assert_eq!(consumed, value);
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Legacy Value Conversion Tests
    // ═════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_from_legacy_null() {
        let nb = NbValue::from_legacy(Value::Null);
        assert!(nb.is_null());
    }

    #[test]
    fn test_from_legacy_bool() {
        let nb_true = NbValue::from_legacy(Value::Bool(true));
        let nb_false = NbValue::from_legacy(Value::Bool(false));

        assert!(nb_true.is_bool());
        assert!(nb_false.is_bool());
        assert_eq!(nb_true.as_bool(), Some(true));
        assert_eq!(nb_false.as_bool(), Some(false));
    }

    #[test]
    fn test_from_legacy_int_in_range() {
        // Within 48-bit range -> inline
        let nb = NbValue::from_legacy(Value::Int(42));
        assert!(nb.is_int());
        assert_eq!(nb.as_int(), Some(42));
    }

    #[test]
    fn test_from_legacy_int_out_of_range() {
        // Outside 48-bit range -> heap boxed
        let big = NbValue::MAX_INT48 + 1;
        let nb = NbValue::from_legacy(Value::Int(big));
        assert!(nb.is_ptr());
        assert!(nb.is_heap_allocated());
        assert_eq!(nb.to_legacy(), Value::Int(big));
    }

    #[test]
    fn test_from_legacy_float() {
        let nb = NbValue::from_legacy(Value::Float(2.71828));
        assert!(nb.is_float());
        assert_eq!(nb.as_float(), Some(2.71828));
    }

    #[test]
    fn test_from_legacy_string() {
        let s = Value::String(StringRef::Owned("test".to_string()));
        let nb = NbValue::from_legacy(s.clone());

        assert!(nb.is_ptr());
        assert!(nb.is_heap_allocated());
        assert_eq!(nb.to_legacy(), s);
    }

    #[test]
    fn test_from_legacy_list() {
        let list = Value::new_list(vec![Value::Int(1), Value::Int(2)]);
        let nb = NbValue::from_legacy(list.clone());

        assert!(nb.is_ptr());
        assert!(nb.is_heap_allocated());
        assert_eq!(nb.to_legacy(), list);
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

        let ptr = NbValue::from_pointer(&Value::Null);
        assert_eq!(ptr.type_name(), "Pointer");
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

        // Pointers are truthy
        let ptr = NbValue::from_legacy(Value::Int(999));
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
        let _ = format!("{:?}", NbValue::from_legacy(Value::Null));
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
