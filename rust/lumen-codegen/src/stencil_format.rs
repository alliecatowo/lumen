//! Copy-and-patch stencil binary format.
//!
//! A **stencil** is a small, pre-compiled machine code fragment that implements
//! one LIR opcode.  The stitcher concatenates copies of stencils into a mutable
//! code buffer and **patches** register offsets, constants and jump targets into
//! the designated holes before making the buffer executable.
//!
//! ## Binary layout (conceptual)
//!
//! ```text
//! StencilLibrary
//!   stencils: HashMap<u8 (opcode), StencilDef>
//!     StencilDef
//!       opcode:  u8
//!       name:    String
//!       code:    Vec<u8>       ← raw machine code; holes are zeros
//!       holes:   Vec<HoleDef>  ← patch sites within `code`
//!     HoleDef
//!       offset:    u32         ← byte offset into `code`
//!       hole_type: HoleType    ← what value to write
//!       size:      u8          ← bytes to write (1 / 2 / 4 / 8)
//! ```
//!
//! ## Calling convention assumed by stencils
//!
//! - `r14` = base pointer of the `NbValue` register file (`*mut NbValue`)
//! - `r15` = pointer to `VmContext`
//! - Each `NbValue` is 8 bytes → `regs[n]` lives at `[r14 + n*8]`
//! - Scratch registers: `rax`, `rcx`, `rdx`, `r8`–`r11` (caller-saved per SysV)
//! - Stencils **fall through** to the next stitched fragment; no explicit jump.
//!
//! ## NaN-boxing scheme (must match `lumen_core::values::NbValue`)
//!
//! ```text
//! NAN_MASK     = 0x7FF8_0000_0000_0000
//! TAG_SHIFT    = 48
//! PAYLOAD_MASK = 0x0000_FFFF_FFFF_FFFF
//!
//! TAG_PTR  = 0  →  NAN_MASK | 0                        (heap pointer, 48-bit)
//! TAG_INT  = 1  →  NAN_MASK | (1 << 48) | payload48   (0x7FF9_…)
//! TAG_BOOL = 3  →  NAN_MASK | (3 << 48) | 0/1         (0x7FFB_0000_0000_000{0,1})
//! TAG_NULL = 4  →  NAN_MASK | (4 << 48)               (0x7FFC_0000_0000_0000)
//! ```

use std::collections::HashMap;

/// A collection of pre-compiled stencils, one per LIR opcode.
///
/// Loaded once at VM startup (or synthesised at build time).  The stitcher
/// clones stencil code into a writable-executable buffer and patches holes.
#[derive(Debug, Clone)]
pub struct StencilLibrary {
    /// Map from `OpCode as u8` → stencil definition.
    pub stencils: HashMap<u8, StencilDef>,
}

impl StencilLibrary {
    /// Create an empty library.
    pub fn new() -> Self {
        Self {
            stencils: HashMap::new(),
        }
    }

    /// Register a stencil definition.
    pub fn insert(&mut self, def: StencilDef) {
        self.stencils.insert(def.opcode, def);
    }

    /// Look up the stencil for a given opcode byte.
    pub fn get(&self, opcode: u8) -> Option<&StencilDef> {
        self.stencils.get(&opcode)
    }

    /// Number of registered stencils.
    pub fn len(&self) -> usize {
        self.stencils.len()
    }

    /// Returns `true` when the library contains no stencils.
    pub fn is_empty(&self) -> bool {
        self.stencils.is_empty()
    }

    // ------------------------------------------------------------------
    // Serialisation (simple binary format, not reliant on serde)
    // ------------------------------------------------------------------

    /// Serialise the library to a compact binary blob.
    ///
    /// Layout:
    /// ```text
    /// [u32 LE]  num_stencils
    /// for each stencil (sorted by opcode for determinism):
    ///   [u8]      opcode
    ///   [u16 LE]  name_len
    ///   [u8 * name_len]   name bytes (UTF-8)
    ///   [u32 LE]  code_len
    ///   [u8 * code_len]   machine code bytes
    ///   [u8]      num_holes
    ///   for each hole:
    ///     [u32 LE]  offset
    ///     [u8]      hole_type (HoleType as u8)
    ///     [u8]      size
    /// ```
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();

        // Collect and sort by opcode for deterministic output.
        let mut entries: Vec<&StencilDef> = self.stencils.values().collect();
        entries.sort_by_key(|s| s.opcode);

        out.extend_from_slice(&(entries.len() as u32).to_le_bytes());

        for def in entries {
            out.push(def.opcode);

            let name_bytes = def.name.as_bytes();
            out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            out.extend_from_slice(name_bytes);

            out.extend_from_slice(&(def.code.len() as u32).to_le_bytes());
            out.extend_from_slice(&def.code);

            out.push(def.holes.len() as u8);
            for hole in &def.holes {
                out.extend_from_slice(&hole.offset.to_le_bytes());
                out.push(hole.hole_type as u8);
                out.push(hole.size);
            }
        }

        out
    }

    /// Deserialise from a binary blob produced by [`to_bytes`].
    pub fn from_bytes(data: &[u8]) -> Result<Self, StencilFormatError> {
        let mut pos = 0;

        let num_stencils = read_u32(data, &mut pos)? as usize;
        let mut lib = StencilLibrary::new();

        for _ in 0..num_stencils {
            let opcode = read_u8(data, &mut pos)?;

            let name_len = read_u16(data, &mut pos)? as usize;
            let name = std::str::from_utf8(read_bytes(data, &mut pos, name_len)?)
                .map_err(|_| StencilFormatError::InvalidUtf8)?
                .to_string();

            let code_len = read_u32(data, &mut pos)? as usize;
            let code = read_bytes(data, &mut pos, code_len)?.to_vec();

            let num_holes = read_u8(data, &mut pos)? as usize;
            let mut holes = Vec::with_capacity(num_holes);
            for _ in 0..num_holes {
                let offset = read_u32(data, &mut pos)?;
                let ht_byte = read_u8(data, &mut pos)?;
                let hole_type = HoleType::from_u8(ht_byte)
                    .ok_or(StencilFormatError::UnknownHoleType(ht_byte))?;
                let size = read_u8(data, &mut pos)?;
                holes.push(HoleDef {
                    offset,
                    hole_type,
                    size,
                });
            }

            lib.insert(StencilDef {
                opcode,
                name,
                code,
                holes,
            });
        }

        Ok(lib)
    }
}

impl Default for StencilLibrary {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// StencilDef
// ---------------------------------------------------------------------------

/// A single stencil — pre-compiled machine code for one LIR opcode.
#[derive(Debug, Clone)]
pub struct StencilDef {
    /// Opcode byte (matches `lumen_core::lir::OpCode as u8`).
    pub opcode: u8,

    /// Human-readable name (e.g. `"Move"`, `"Add"`).
    pub name: String,

    /// Machine code bytes.
    ///
    /// Holes are represented as **zero bytes** in their respective positions;
    /// the stitcher overwrites them with the patched values.
    pub code: Vec<u8>,

    /// Patch sites within `code`.
    pub holes: Vec<HoleDef>,
}

impl StencilDef {
    /// Create a new stencil definition.
    pub fn new(opcode: u8, name: impl Into<String>, code: Vec<u8>, holes: Vec<HoleDef>) -> Self {
        Self {
            opcode,
            name: name.into(),
            code,
            holes,
        }
    }

    /// Returns the total byte count of this stencil's machine code.
    pub fn code_len(&self) -> usize {
        self.code.len()
    }
}

// ---------------------------------------------------------------------------
// HoleDef
// ---------------------------------------------------------------------------

/// A single patch site within a stencil's machine code.
///
/// The stitcher reads `hole_type` to determine what value to compute, then
/// writes `size` bytes of that value at byte `offset` within the code copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoleDef {
    /// Byte offset within [`StencilDef::code`] where this hole begins.
    pub offset: u32,

    /// How to compute the patch value from the LIR instruction operands.
    pub hole_type: HoleType,

    /// Width of the hole in bytes: 1, 2, 4, or 8.
    pub size: u8,
}

impl HoleDef {
    /// Convenience constructor.
    pub fn new(offset: u32, hole_type: HoleType, size: u8) -> Self {
        Self {
            offset,
            hole_type,
            size,
        }
    }
}

// ---------------------------------------------------------------------------
// HoleType
// ---------------------------------------------------------------------------

/// Specifies how to compute the patch value for a hole.
///
/// **Register holes** (RegA / RegB / RegC) produce a byte offset into the
/// `NbValue` register array: `(reg_index as i32) * 8`.  This is an `i32`
/// displacement suitable for use in x86-64 ModRM `disp32` fields.
///
/// **Constant holes** ask the stitcher to read the LIR constant pool and
/// convert the constant to a 64-bit `NbValue` at stitch time — eliminating
/// the constant-pool indirection from the hot path.
///
/// **Jump holes** are patched with a PC-relative 32-bit signed offset to
/// the stitched target stencil.  The stitcher knows the layout of the
/// code buffer and can compute this at stitch time.
///
/// **RuntimeFuncAddr** holes are patched with the 64-bit absolute address
/// of a named runtime helper function (looked up via the `lm_rt_*` symbol
/// table at stitch time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HoleType {
    /// Patch with `(instr.a as i32) * 8` — disp32 into the NbValue register file.
    RegA = 0,

    /// Patch with `(instr.b as i32) * 8` — disp32 into the NbValue register file.
    RegB = 1,

    /// Patch with `(instr.c as i32) * 8` — disp32 into the NbValue register file.
    RegC = 2,

    /// Patch with `instr.a as u8` — raw register index (not scaled).
    RegAIndex = 3,

    /// Patch with `instr.b as u8` — raw register index (not scaled).
    RegBIndex = 4,

    /// Patch with `instr.c as u8` — raw register index (not scaled).
    RegCIndex = 5,

    /// Patch with `instr.bx as u16` — raw 16-bit constant pool index.
    ConstBx = 6,

    /// Patch 8 bytes with the `NbValue` representation of `constants[instr.bx]`.
    ///
    /// The stitcher reads the LIR constant pool at stitch time, converts the
    /// `Constant` to an `NbValue` (64-bit NaN-boxed word), and writes it here.
    /// Eliminates the constant-pool indirection on the hot path.
    Constant64 = 7,

    /// Patch 4 bytes with a signed PC-relative jump offset to the target stencil.
    ///
    /// `target_addr - (hole_addr + 4)` — standard x86-64 `rel32` encoding.
    JumpOffset32 = 8,

    /// Patch 8 bytes with the 64-bit absolute address of a runtime helper.
    ///
    /// The stitcher resolves the symbol name (embedded in the stencil metadata)
    /// from the runtime function table at stitch time.
    RuntimeFuncAddr = 9,

    /// Patch 8 bytes with the 64-bit absolute address of the `VmContext`.
    ///
    /// Used by stencils that need to call back into the VM (e.g. effect handlers).
    VmContextAddr = 10,

    /// Patch 2 bytes with `instr.sbx as i16` — signed Bx field.
    SBx = 11,

    /// Patch 4 bytes with the full 32-bit LIR instruction word.
    ///
    /// Used by runtime-call stencils that pass the raw instruction to helpers.
    InstructionWord = 12,
}

impl HoleType {
    /// Convert from the serialised `u8` tag.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::RegA),
            1 => Some(Self::RegB),
            2 => Some(Self::RegC),
            3 => Some(Self::RegAIndex),
            4 => Some(Self::RegBIndex),
            5 => Some(Self::RegCIndex),
            6 => Some(Self::ConstBx),
            7 => Some(Self::Constant64),
            8 => Some(Self::JumpOffset32),
            9 => Some(Self::RuntimeFuncAddr),
            10 => Some(Self::VmContextAddr),
            11 => Some(Self::SBx),
            12 => Some(Self::InstructionWord),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur when deserialising a stencil library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StencilFormatError {
    /// The input buffer ended unexpectedly.
    UnexpectedEof,
    /// A name field contained invalid UTF-8.
    InvalidUtf8,
    /// A `HoleType` byte was not recognised.
    UnknownHoleType(u8),
}

impl std::fmt::Display for StencilFormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "stencil binary: unexpected end of data"),
            Self::InvalidUtf8 => write!(f, "stencil binary: invalid UTF-8 in stencil name"),
            Self::UnknownHoleType(b) => write!(f, "stencil binary: unknown HoleType byte {b:#04x}"),
        }
    }
}

impl std::error::Error for StencilFormatError {}

// ---------------------------------------------------------------------------
// Internal decode helpers
// ---------------------------------------------------------------------------

fn read_u8(data: &[u8], pos: &mut usize) -> Result<u8, StencilFormatError> {
    if *pos >= data.len() {
        return Err(StencilFormatError::UnexpectedEof);
    }
    let v = data[*pos];
    *pos += 1;
    Ok(v)
}

fn read_u16(data: &[u8], pos: &mut usize) -> Result<u16, StencilFormatError> {
    let b = read_bytes(data, pos, 2)?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}

fn read_u32(data: &[u8], pos: &mut usize) -> Result<u32, StencilFormatError> {
    let b = read_bytes(data, pos, 4)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn read_bytes<'a>(
    data: &'a [u8],
    pos: &mut usize,
    len: usize,
) -> Result<&'a [u8], StencilFormatError> {
    let end = pos
        .checked_add(len)
        .ok_or(StencilFormatError::UnexpectedEof)?;
    if end > data.len() {
        return Err(StencilFormatError::UnexpectedEof);
    }
    let slice = &data[*pos..end];
    *pos = end;
    Ok(slice)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_move_stencil() -> StencilDef {
        // mov rax, [r14 + B*8]  →  49 8B 86 <disp32>  (7 bytes, hole at 3)
        // mov [r14 + A*8], rax  →  49 89 86 <disp32>  (7 bytes, hole at 10)
        StencilDef::new(
            0x05, // OpCode::Move
            "Move",
            vec![
                0x49, 0x8B, 0x86, 0x00, 0x00, 0x00, 0x00, // mov rax, [r14+B*8]
                0x49, 0x89, 0x86, 0x00, 0x00, 0x00, 0x00, // mov [r14+A*8], rax
            ],
            vec![
                HoleDef::new(3, HoleType::RegB, 4),
                HoleDef::new(10, HoleType::RegA, 4),
            ],
        )
    }

    #[test]
    fn test_stencil_def_creation() {
        let s = make_move_stencil();
        assert_eq!(s.opcode, 0x05);
        assert_eq!(s.name, "Move");
        assert_eq!(s.code_len(), 14);
        assert_eq!(s.holes.len(), 2);
    }

    #[test]
    fn test_library_insert_and_get() {
        let mut lib = StencilLibrary::new();
        assert!(lib.is_empty());

        lib.insert(make_move_stencil());
        assert_eq!(lib.len(), 1);
        assert!(lib.get(0x05).is_some());
        assert!(lib.get(0xFF).is_none());
    }

    #[test]
    fn test_round_trip_serialisation() {
        let mut lib = StencilLibrary::new();
        lib.insert(make_move_stencil());

        // Also add a LoadNull stencil.
        lib.insert(StencilDef::new(
            0x02, // LoadNil
            "LoadNil",
            vec![
                // movabs rax, 0x7FFC_0000_0000_0000  (17 bytes)
                0x48, 0xB8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFC, 0x7F,
                // mov [r14+A*8], rax
                0x49, 0x89, 0x86, 0x00, 0x00, 0x00, 0x00,
            ],
            vec![HoleDef::new(13, HoleType::RegA, 4)],
        ));

        let bytes = lib.to_bytes();
        let lib2 = StencilLibrary::from_bytes(&bytes).expect("round-trip failed");

        assert_eq!(lib2.len(), 2);

        let mv = lib2.get(0x05).unwrap();
        assert_eq!(mv.name, "Move");
        assert_eq!(mv.code_len(), 14);
        assert_eq!(mv.holes.len(), 2);
        assert_eq!(mv.holes[0].hole_type, HoleType::RegB);
        assert_eq!(mv.holes[1].hole_type, HoleType::RegA);
    }

    #[test]
    fn test_hole_type_round_trip() {
        let types = [
            HoleType::RegA,
            HoleType::RegB,
            HoleType::RegC,
            HoleType::Constant64,
            HoleType::JumpOffset32,
            HoleType::RuntimeFuncAddr,
            HoleType::InstructionWord,
        ];
        for ht in types {
            assert_eq!(HoleType::from_u8(ht as u8), Some(ht));
        }
        assert_eq!(HoleType::from_u8(0xFF), None);
    }

    #[test]
    fn test_serialise_empty_library() {
        let lib = StencilLibrary::new();
        let bytes = lib.to_bytes();
        // Just a u32 zero.
        assert_eq!(bytes, vec![0, 0, 0, 0]);
        let lib2 = StencilLibrary::from_bytes(&bytes).unwrap();
        assert!(lib2.is_empty());
    }
}
