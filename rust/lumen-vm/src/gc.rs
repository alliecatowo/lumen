//! GC header and object tagging for heap-managed objects.
//!
//! Every GC-managed heap object is prepended with an 8-byte `GcHeader`
//! encoding color (tri-color marking), pin state, mark/forward bits,
//! a type tag, and the object size.

use std::fmt;

/// Header prepended to all GC-managed heap objects.
///
/// Layout (8 bytes total):
/// ```text
/// flags (u32):
///   [1:0]  color     — GcColor (White/Gray/Black)
///   [2]    pinned    — object is pinned (cannot be moved)
///   [3]    marked    — object is marked (live)
///   [4]    forwarded — object has been forwarded (moved during compaction)
///   [7:5]  unused
///   [15:8] type_tag  — TypeTag discriminant
///   [31:16] unused
/// size (u32):
///   object body size in bytes (max ~4 GiB)
/// ```
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GcHeader {
    flags: u32,
    size: u32,
}

// Bit masks for the flags field
const COLOR_MASK: u32 = 0b11;
const COLOR_SHIFT: u32 = 0;
const PINNED_BIT: u32 = 1 << 2;
const MARKED_BIT: u32 = 1 << 3;
const FORWARDED_BIT: u32 = 1 << 4;
const TYPE_TAG_MASK: u32 = 0xFF;
const TYPE_TAG_SHIFT: u32 = 8;

/// Tri-color abstraction for concurrent/incremental tracing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcColor {
    /// Not yet traced — candidate for collection.
    White = 0,
    /// Queued for tracing — reachable but children not yet scanned.
    Gray = 1,
    /// Fully traced — object and all its children are reachable.
    Black = 2,
}

impl GcColor {
    fn from_bits(bits: u32) -> Self {
        match bits & 0b11 {
            0 => GcColor::White,
            1 => GcColor::Gray,
            2 => GcColor::Black,
            _ => GcColor::White, // reserved → treat as White
        }
    }
}

/// Discriminant for the kind of heap object following the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeTag {
    List = 0,
    Tuple = 1,
    Map = 2,
    Set = 3,
    Record = 4,
    String = 5,
    Bytes = 6,
    Closure = 7,
    Future = 8,
}

impl TypeTag {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(TypeTag::List),
            1 => Some(TypeTag::Tuple),
            2 => Some(TypeTag::Map),
            3 => Some(TypeTag::Set),
            4 => Some(TypeTag::Record),
            5 => Some(TypeTag::String),
            6 => Some(TypeTag::Bytes),
            7 => Some(TypeTag::Closure),
            8 => Some(TypeTag::Future),
            _ => None,
        }
    }
}

impl GcHeader {
    /// Size of the header itself in bytes.
    pub const SIZE: usize = std::mem::size_of::<Self>();

    /// Create a new header with the given type tag and body size.
    /// Initializes as White, not pinned, not marked, not forwarded.
    pub fn new(tag: TypeTag, size: u32) -> Self {
        let flags = ((tag as u32) << TYPE_TAG_SHIFT) | (GcColor::White as u32);
        Self { flags, size }
    }

    // --- Color ---

    /// Get the current tri-color value.
    pub fn color(&self) -> GcColor {
        GcColor::from_bits((self.flags >> COLOR_SHIFT) & COLOR_MASK)
    }

    /// Set the tri-color value.
    pub fn set_color(&mut self, color: GcColor) {
        self.flags = (self.flags & !(COLOR_MASK << COLOR_SHIFT)) | ((color as u32) << COLOR_SHIFT);
    }

    // --- Pinned ---

    /// Returns `true` if the object is pinned (cannot be relocated).
    pub fn is_pinned(&self) -> bool {
        self.flags & PINNED_BIT != 0
    }

    /// Pin this object so it will not be moved during compaction.
    pub fn pin(&mut self) {
        self.flags |= PINNED_BIT;
    }

    /// Unpin this object, allowing it to be relocated.
    pub fn unpin(&mut self) {
        self.flags &= !PINNED_BIT;
    }

    // --- Marked ---

    /// Returns `true` if the object has been marked (live).
    pub fn is_marked(&self) -> bool {
        self.flags & MARKED_BIT != 0
    }

    /// Mark the object as live.
    pub fn mark(&mut self) {
        self.flags |= MARKED_BIT;
    }

    /// Clear the mark bit.
    pub fn unmark(&mut self) {
        self.flags &= !MARKED_BIT;
    }

    // --- Forwarded ---

    /// Returns `true` if the object has been forwarded (moved during compaction).
    pub fn is_forwarded(&self) -> bool {
        self.flags & FORWARDED_BIT != 0
    }

    /// Set the forwarded flag.
    pub fn set_forwarded(&mut self) {
        self.flags |= FORWARDED_BIT;
    }

    // --- Type tag ---

    /// Get the type tag identifying the kind of heap object.
    pub fn type_tag(&self) -> TypeTag {
        let raw = ((self.flags >> TYPE_TAG_SHIFT) & TYPE_TAG_MASK) as u8;
        TypeTag::from_u8(raw).expect("invalid type tag in GcHeader")
    }

    // --- Size ---

    /// Get the size of the object body in bytes (excluding the header).
    pub fn object_size(&self) -> u32 {
        self.size
    }

    /// Get the total allocation size: header + body.
    pub fn total_size(&self) -> usize {
        Self::SIZE + self.size as usize
    }
}

impl fmt::Debug for GcHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GcHeader")
            .field("color", &self.color())
            .field("pinned", &self.is_pinned())
            .field("marked", &self.is_marked())
            .field("forwarded", &self.is_forwarded())
            .field("type_tag", &self.type_tag())
            .field("object_size", &self.object_size())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_size_is_8_bytes() {
        assert_eq!(GcHeader::SIZE, 8);
    }

    #[test]
    fn test_new_header_defaults() {
        let h = GcHeader::new(TypeTag::List, 128);
        assert_eq!(h.color(), GcColor::White);
        assert!(!h.is_pinned());
        assert!(!h.is_marked());
        assert!(!h.is_forwarded());
        assert_eq!(h.type_tag(), TypeTag::List);
        assert_eq!(h.object_size(), 128);
        assert_eq!(h.total_size(), 8 + 128);
    }

    #[test]
    fn test_color_set_get() {
        let mut h = GcHeader::new(TypeTag::Tuple, 64);
        assert_eq!(h.color(), GcColor::White);

        h.set_color(GcColor::Gray);
        assert_eq!(h.color(), GcColor::Gray);

        h.set_color(GcColor::Black);
        assert_eq!(h.color(), GcColor::Black);

        h.set_color(GcColor::White);
        assert_eq!(h.color(), GcColor::White);
    }

    #[test]
    fn test_pin_unpin() {
        let mut h = GcHeader::new(TypeTag::Map, 256);
        assert!(!h.is_pinned());
        h.pin();
        assert!(h.is_pinned());
        h.unpin();
        assert!(!h.is_pinned());
    }

    #[test]
    fn test_mark_unmark() {
        let mut h = GcHeader::new(TypeTag::Set, 32);
        assert!(!h.is_marked());
        h.mark();
        assert!(h.is_marked());
        h.unmark();
        assert!(!h.is_marked());
    }

    #[test]
    fn test_forwarded() {
        let mut h = GcHeader::new(TypeTag::Record, 100);
        assert!(!h.is_forwarded());
        h.set_forwarded();
        assert!(h.is_forwarded());
    }

    #[test]
    fn test_all_type_tags() {
        let tags = [
            (TypeTag::List, 0),
            (TypeTag::Tuple, 1),
            (TypeTag::Map, 2),
            (TypeTag::Set, 3),
            (TypeTag::Record, 4),
            (TypeTag::String, 5),
            (TypeTag::Bytes, 6),
            (TypeTag::Closure, 7),
            (TypeTag::Future, 8),
        ];
        for (tag, _expected) in &tags {
            let h = GcHeader::new(*tag, 16);
            assert_eq!(h.type_tag(), *tag);
        }
    }

    #[test]
    fn test_flags_dont_interfere() {
        let mut h = GcHeader::new(TypeTag::Closure, 512);
        h.set_color(GcColor::Gray);
        h.pin();
        h.mark();
        h.set_forwarded();

        // All flags should be independently readable
        assert_eq!(h.color(), GcColor::Gray);
        assert!(h.is_pinned());
        assert!(h.is_marked());
        assert!(h.is_forwarded());
        assert_eq!(h.type_tag(), TypeTag::Closure);
        assert_eq!(h.object_size(), 512);
    }

    #[test]
    fn test_zero_size_object() {
        let h = GcHeader::new(TypeTag::Tuple, 0);
        assert_eq!(h.object_size(), 0);
        assert_eq!(h.total_size(), GcHeader::SIZE);
    }

    #[test]
    fn test_max_size_object() {
        let h = GcHeader::new(TypeTag::Bytes, u32::MAX);
        assert_eq!(h.object_size(), u32::MAX);
        assert_eq!(h.total_size(), GcHeader::SIZE + u32::MAX as usize);
    }

    #[test]
    fn test_debug_format() {
        let h = GcHeader::new(TypeTag::String, 42);
        let dbg = format!("{:?}", h);
        assert!(dbg.contains("GcHeader"));
        assert!(dbg.contains("White"));
        assert!(dbg.contains("String"));
    }
}
