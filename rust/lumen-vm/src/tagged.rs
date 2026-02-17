//! Tagged pointer / small-value optimization.
//!
//! `TaggedValue` is a 64-bit value that can represent small immediates
//! (integers, booleans, null) inline without heap allocation, or a
//! tagged pointer to a heap object.
//!
//! Encoding scheme:
//! - Bit 0 = 0: heap pointer (aligned to 8 bytes, so low 3 bits are free)
//! - Bit 0 = 1: immediate value; bits [2:1] encode the sub-type:
//!   - `0b_001` = small integer (i61 in bits [63:3])
//!   - `0b_011` = boolean (bit 3: 0=false, 1=true)
//!   - `0b_101` = null
//!   - `0b_111` = reserved (future: small float, etc.)

use std::fmt;

/// Immediate type tags (low 3 bits, bit 0 always set).
const TAG_INT: u64 = 0b001;
const TAG_BOOL: u64 = 0b011;
const TAG_NULL: u64 = 0b101;
#[allow(dead_code)]
const TAG_RESERVED: u64 = 0b111;

/// Mask for the 3-bit tag.
const TAG_MASK: u64 = 0b111;

/// Maximum value for a small integer (i61): 2^60 - 1.
const SMALL_INT_MAX: i64 = (1i64 << 60) - 1;
/// Minimum value for a small integer (i61): -(2^60).
const SMALL_INT_MIN: i64 = -(1i64 << 60);

/// A 64-bit tagged value that can represent small immediates without
/// heap allocation.
#[derive(Clone, Copy)]
pub struct TaggedValue(u64);

impl TaggedValue {
    /// Pack a small integer. Returns `None` if the value does not fit
    /// in 61 bits (signed).
    pub fn from_int(i: i64) -> Option<Self> {
        if !(SMALL_INT_MIN..=SMALL_INT_MAX).contains(&i) {
            return None;
        }
        // Shift left 3, then set the int tag.
        // For negative numbers, the arithmetic works because we mask
        // after the shift.
        let bits = ((i as u64) << 3) | TAG_INT;
        Some(Self(bits))
    }

    /// Pack a boolean.
    pub fn from_bool(b: bool) -> Self {
        let bits = ((b as u64) << 3) | TAG_BOOL;
        Self(bits)
    }

    /// Create the null sentinel.
    pub fn from_null() -> Self {
        Self(TAG_NULL)
    }

    /// Tag a heap pointer. The pointer must be aligned to at least 8 bytes
    /// (low 3 bits zero).
    ///
    /// # Panics
    /// Panics if the pointer is not 8-byte aligned or is null.
    pub fn from_ptr(ptr: *const u8) -> Self {
        let addr = ptr as u64;
        assert!(addr != 0, "cannot tag a null pointer — use from_null()");
        assert!(
            addr & TAG_MASK == 0,
            "pointer must be 8-byte aligned (got {addr:#x})"
        );
        Self(addr)
    }

    // --- Queries ---

    /// Returns `true` if this is an immediate (non-pointer) value.
    pub fn is_immediate(&self) -> bool {
        self.0 & 1 == 1
    }

    /// Returns `true` if this is a heap pointer.
    pub fn is_pointer(&self) -> bool {
        self.0 & 1 == 0 && self.0 != 0
    }

    /// Returns `true` if this is a small integer.
    pub fn is_int(&self) -> bool {
        self.0 & TAG_MASK == TAG_INT
    }

    /// Returns `true` if this is a boolean.
    pub fn is_bool(&self) -> bool {
        self.0 & TAG_MASK == TAG_BOOL
    }

    /// Returns `true` if this is null.
    pub fn is_null(&self) -> bool {
        self.0 & TAG_MASK == TAG_NULL
    }

    // --- Unpacking ---

    /// Unpack a small integer.
    pub fn as_int(&self) -> Option<i64> {
        if !self.is_int() {
            return None;
        }
        // Arithmetic right-shift to sign-extend.
        Some((self.0 as i64) >> 3)
    }

    /// Unpack a boolean.
    pub fn as_bool(&self) -> Option<bool> {
        if !self.is_bool() {
            return None;
        }
        Some((self.0 >> 3) & 1 == 1)
    }

    /// Unpack a heap pointer.
    pub fn as_ptr(&self) -> Option<*const u8> {
        if !self.is_pointer() {
            return None;
        }
        Some(self.0 as *const u8)
    }

    /// Get the raw bits (for debugging / serialization).
    pub fn raw_bits(&self) -> u64 {
        self.0
    }
}

impl PartialEq for TaggedValue {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for TaggedValue {}

impl fmt::Debug for TaggedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_null() {
            write!(f, "TaggedValue(null)")
        } else if self.is_int() {
            write!(f, "TaggedValue(int: {})", self.as_int().unwrap())
        } else if self.is_bool() {
            write!(f, "TaggedValue(bool: {})", self.as_bool().unwrap())
        } else if self.is_pointer() {
            write!(f, "TaggedValue(ptr: {:#x})", self.0)
        } else {
            write!(f, "TaggedValue(raw: {:#018x})", self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_int_zero() {
        let v = TaggedValue::from_int(0).unwrap();
        assert!(v.is_int());
        assert!(v.is_immediate());
        assert!(!v.is_pointer());
        assert!(!v.is_bool());
        assert!(!v.is_null());
        assert_eq!(v.as_int(), Some(0));
    }

    #[test]
    fn test_small_int_positive() {
        let v = TaggedValue::from_int(42).unwrap();
        assert!(v.is_int());
        assert_eq!(v.as_int(), Some(42));
    }

    #[test]
    fn test_small_int_negative() {
        let v = TaggedValue::from_int(-7).unwrap();
        assert!(v.is_int());
        assert_eq!(v.as_int(), Some(-7));
    }

    #[test]
    fn test_small_int_max() {
        let v = TaggedValue::from_int(SMALL_INT_MAX).unwrap();
        assert_eq!(v.as_int(), Some(SMALL_INT_MAX));
    }

    #[test]
    fn test_small_int_min() {
        let v = TaggedValue::from_int(SMALL_INT_MIN).unwrap();
        assert_eq!(v.as_int(), Some(SMALL_INT_MIN));
    }

    #[test]
    fn test_small_int_overflow() {
        assert!(TaggedValue::from_int(SMALL_INT_MAX + 1).is_none());
        assert!(TaggedValue::from_int(SMALL_INT_MIN - 1).is_none());
    }

    #[test]
    fn test_bool_true() {
        let v = TaggedValue::from_bool(true);
        assert!(v.is_bool());
        assert!(v.is_immediate());
        assert!(!v.is_int());
        assert_eq!(v.as_bool(), Some(true));
    }

    #[test]
    fn test_bool_false() {
        let v = TaggedValue::from_bool(false);
        assert!(v.is_bool());
        assert_eq!(v.as_bool(), Some(false));
    }

    #[test]
    fn test_null() {
        let v = TaggedValue::from_null();
        assert!(v.is_null());
        assert!(v.is_immediate());
        assert!(!v.is_pointer());
        assert!(!v.is_int());
        assert!(!v.is_bool());
        assert_eq!(v.as_int(), None);
        assert_eq!(v.as_bool(), None);
        assert_eq!(v.as_ptr(), None);
    }

    #[test]
    fn test_pointer() {
        // Create an 8-byte aligned address.
        let aligned_addr: u64 = 0x7FFE_0000_1000;
        let ptr = aligned_addr as *const u8;
        let v = TaggedValue::from_ptr(ptr);
        assert!(v.is_pointer());
        assert!(!v.is_immediate());
        assert_eq!(v.as_ptr(), Some(ptr));
    }

    #[test]
    #[should_panic(expected = "8-byte aligned")]
    fn test_unaligned_pointer_panics() {
        let unaligned = 0x7FFE_0000_1001 as *const u8;
        TaggedValue::from_ptr(unaligned);
    }

    #[test]
    #[should_panic(expected = "null pointer")]
    fn test_null_pointer_panics() {
        TaggedValue::from_ptr(std::ptr::null());
    }

    #[test]
    fn test_equality() {
        assert_eq!(TaggedValue::from_int(5), TaggedValue::from_int(5));
        assert_ne!(TaggedValue::from_int(5), TaggedValue::from_int(6));
        assert_eq!(TaggedValue::from_bool(true), TaggedValue::from_bool(true));
        assert_ne!(TaggedValue::from_bool(true), TaggedValue::from_bool(false));
        assert_eq!(TaggedValue::from_null(), TaggedValue::from_null());
    }

    #[test]
    fn test_round_trip_int() {
        for val in [-1000, -1, 0, 1, 1000, SMALL_INT_MIN, SMALL_INT_MAX] {
            let tv = TaggedValue::from_int(val).unwrap();
            assert_eq!(tv.as_int(), Some(val), "round-trip failed for {val}");
        }
    }

    #[test]
    fn test_debug_format() {
        let i = TaggedValue::from_int(99).unwrap();
        assert!(format!("{:?}", i).contains("int: 99"));

        let b = TaggedValue::from_bool(false);
        assert!(format!("{:?}", b).contains("bool: false"));

        let n = TaggedValue::from_null();
        assert!(format!("{:?}", n).contains("null"));
    }

    #[test]
    fn test_type_discrimination() {
        // Ensure each type only matches its own predicate
        let int_val = TaggedValue::from_int(1).unwrap();
        let bool_val = TaggedValue::from_bool(true);
        let null_val = TaggedValue::from_null();

        assert!(int_val.is_int() && !int_val.is_bool() && !int_val.is_null());
        assert!(!bool_val.is_int() && bool_val.is_bool() && !bool_val.is_null());
        assert!(!null_val.is_int() && !null_val.is_bool() && null_val.is_null());
    }
}
