//! Copy-and-patch stitcher — Tier 1 JIT compiler.
//!
//! The stitcher concatenates pre-compiled machine code fragments (stencils) into
//! executable memory and patches register offsets, constants, and jump targets.
//! This produces native code for a LIR cell without running a full compiler
//! pipeline — the tradeoff is that the code is un-optimized but compiles in
//! microseconds.
//!
//! ## Usage
//!
//! ```ignore
//! let lib = build_stencil_library();
//! let mut stitcher = Stitcher::new(lib)?;
//! let code = stitcher.compile(&module.cells[0], &module)?;
//! // code.code_ptr is now a function pointer to native x86-64 code
//! ```

use std::collections::HashMap;

use lumen_core::lir::{Constant, Instruction, LirCell, LirModule, OpCode};
use lumen_core::nb_value::NbValue;

use crate::stencil_format::{HoleType, StencilLibrary};

// ---------------------------------------------------------------------------
// StitchedCode — a compiled cell's native code
// ---------------------------------------------------------------------------

/// A compiled cell — a contiguous block of native machine code.
#[derive(Debug)]
pub struct StitchedCode {
    /// Pointer into the executable memory region.
    pub code_ptr: *const u8,
    /// Length of the stitched code in bytes.
    pub code_len: usize,
}

// SAFETY: The code pointer points into a long-lived mmap region owned by the
// Stitcher. It is safe to send across threads as long as the Stitcher outlives
// all StitchedCode references (which it does, since the Stitcher owns the
// memory and lives for the entire program lifetime).
unsafe impl Send for StitchedCode {}
unsafe impl Sync for StitchedCode {}

// ---------------------------------------------------------------------------
// Stitcher
// ---------------------------------------------------------------------------

/// Copy-and-patch JIT compiler.
///
/// Holds a library of pre-compiled stencils and an executable memory region
/// into which stitched code is emitted.
pub struct Stitcher {
    /// Pre-compiled stencils indexed by opcode.
    library: StencilLibrary,

    /// Executable memory region (mmap RWX on Linux).
    ///
    /// We write stencil copies here and then make it executable.
    /// On Linux this is allocated with `mmap(PROT_READ|PROT_WRITE|PROT_EXEC)`.
    code_buffer: *mut u8,

    /// Total capacity of the code buffer in bytes.
    code_capacity: usize,

    /// Current write position (next free byte) in the code buffer.
    code_offset: usize,

    /// Compiled cells: `cell_index → StitchedCode`.
    compiled: HashMap<usize, StitchedCode>,
}

// SAFETY: The Stitcher owns its mmap region exclusively.
unsafe impl Send for Stitcher {}

/// Errors from the stitcher.
#[derive(Debug, Clone)]
pub enum StitchError {
    /// No stencil registered for this opcode.
    MissingStencil(u8, String),
    /// Code buffer exhausted.
    OutOfMemory { needed: usize, available: usize },
    /// Platform error during mmap.
    MmapFailed(String),
    /// Constant pool index out of bounds.
    ConstantOutOfBounds(u32),
}

impl std::fmt::Display for StitchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingStencil(op, name) => {
                write!(f, "no stencil for opcode {op:#04x} ({name})")
            }
            Self::OutOfMemory { needed, available } => {
                write!(
                    f,
                    "code buffer exhausted: need {needed} bytes, {available} available"
                )
            }
            Self::MmapFailed(msg) => write!(f, "mmap failed: {msg}"),
            Self::ConstantOutOfBounds(idx) => {
                write!(f, "constant pool index {idx} out of bounds")
            }
        }
    }
}

impl std::error::Error for StitchError {}

/// Default code buffer size: 4 MiB.
const DEFAULT_CODE_SIZE: usize = 4 * 1024 * 1024;

impl Stitcher {
    /// Create a new stitcher with the given stencil library.
    ///
    /// Allocates an executable memory region of `DEFAULT_CODE_SIZE` bytes.
    pub fn new(library: StencilLibrary) -> Result<Self, StitchError> {
        Self::with_capacity(library, DEFAULT_CODE_SIZE)
    }

    /// Create a new stitcher with a custom code buffer size.
    pub fn with_capacity(library: StencilLibrary, capacity: usize) -> Result<Self, StitchError> {
        let code_buffer = alloc_executable(capacity)?;
        Ok(Self {
            library,
            code_buffer,
            code_capacity: capacity,
            code_offset: 0,
            compiled: HashMap::new(),
        })
    }

    /// Returns the number of bytes used in the code buffer.
    pub fn code_used(&self) -> usize {
        self.code_offset
    }

    /// Returns the number of bytes remaining in the code buffer.
    pub fn code_available(&self) -> usize {
        self.code_capacity - self.code_offset
    }

    /// Returns the stencil library.
    pub fn library(&self) -> &StencilLibrary {
        &self.library
    }

    /// Look up previously compiled code for a cell.
    pub fn get_compiled(&self, cell_index: usize) -> Option<&StitchedCode> {
        self.compiled.get(&cell_index)
    }

    /// Compile a single LIR cell into native code.
    ///
    /// For each instruction in the cell:
    /// 1. Find the stencil for the opcode
    /// 2. Copy the stencil bytes into the code buffer
    /// 3. Patch holes with actual register indices, constants, and jump offsets
    ///
    /// Returns the `StitchedCode` for the compiled cell.
    pub fn compile(
        &mut self,
        cell: &LirCell,
        module: &LirModule,
        cell_index: usize,
    ) -> Result<&StitchedCode, StitchError> {
        let start_offset = self.code_offset;

        // First pass: compute the byte offset of each instruction's stencil
        // in the output buffer. We need this for jump patching.
        let mut instruction_offsets: Vec<usize> = Vec::with_capacity(cell.instructions.len());
        {
            let mut offset = start_offset;
            for instr in &cell.instructions {
                instruction_offsets.push(offset);
                let op = instr.op as u8;
                let stencil = if instr.op == OpCode::Intrinsic {
                    self.library
                        .get_intrinsic(instr.b as u8)
                        .or_else(|| self.library.get(op))
                } else {
                    self.library.get(op)
                };
                if let Some(stencil) = stencil {
                    offset += stencil.code.len();
                } else {
                    // Unknown opcodes get a fallback size of 0 (will error in second pass)
                    // but we still need to record the offset for subsequent instructions.
                }
            }
        }

        // Check we have enough space for all stencils.
        let total_size: usize = cell
            .instructions
            .iter()
            .map(|i| {
                if i.op == OpCode::Intrinsic {
                    self.library
                        .get_intrinsic(i.b as u8)
                        .or_else(|| self.library.get(i.op as u8))
                        .map(|s| s.code.len())
                        .unwrap_or(0)
                } else {
                    self.library
                        .get(i.op as u8)
                        .map(|s| s.code.len())
                        .unwrap_or(0)
                }
            })
            .sum();

        if total_size > self.code_available() {
            return Err(StitchError::OutOfMemory {
                needed: total_size,
                available: self.code_available(),
            });
        }

        // Second pass: copy stencils and patch holes.
        //
        // We collect stencil references (index into library) up front so we
        // don't hold an immutable borrow on `self.library` while mutating
        // `self.code_buffer` through `emit_bytes` / `patch_*`.
        for (pc, instr) in cell.instructions.iter().enumerate() {
            let stencil = if instr.op == OpCode::Intrinsic {
                let intrinsic_id = instr.b as u8;
                let stencil = self
                    .library
                    .get_intrinsic(intrinsic_id)
                    .or_else(|| self.library.get(instr.op as u8));
                let Some(stencil) = stencil else {
                    return Err(StitchError::MissingStencil(
                        instr.op as u8,
                        format!("Intrinsic({intrinsic_id})"),
                    ));
                };
                stencil
            } else {
                let op_byte = instr.op as u8;
                let stencil = self.library.get(op_byte).ok_or_else(|| {
                    StitchError::MissingStencil(op_byte, format!("{:?}", instr.op))
                })?;
                stencil
            };

            // Copy stencil code and holes out of the library to release the
            // immutable borrow on `self` before we call `emit_bytes`.
            let code_bytes = stencil.code.clone();
            let holes = stencil.holes.clone();

            let stencil_start = self.code_offset;

            // Copy stencil bytes into the code buffer.
            self.emit_bytes(&code_bytes);

            // Patch each hole.
            for hole in &holes {
                let patch_addr = stencil_start + hole.offset as usize;

                match hole.hole_type {
                    HoleType::RegA => {
                        // Register A displacement: a * 8 (bytes offset into NbValue array)
                        let disp = (instr.a as i32) * 8;
                        self.patch_i32(patch_addr, disp);
                    }
                    HoleType::RegB => {
                        let disp = if instr.op == OpCode::Intrinsic {
                            match instr.b as u8 {
                                24 | 25 => (instr.c as i32 + 1) * 8,
                                _ => (instr.b as i32) * 8,
                            }
                        } else {
                            (instr.b as i32) * 8
                        };
                        self.patch_i32(patch_addr, disp);
                    }
                    HoleType::RegC => {
                        let disp = (instr.c as i32) * 8;
                        self.patch_i32(patch_addr, disp);
                    }
                    HoleType::RegAIndex => {
                        self.patch_u8(patch_addr, instr.a as u8);
                    }
                    HoleType::RegBIndex => {
                        self.patch_u8(patch_addr, instr.b as u8);
                    }
                    HoleType::RegCIndex => {
                        self.patch_u8(patch_addr, instr.c as u8);
                    }
                    HoleType::ConstBx => {
                        self.patch_u16(patch_addr, instr.bx() as u16);
                    }
                    HoleType::Constant64 => {
                        let nb = constant_to_nb(instr, cell, module)?;
                        self.patch_u64(patch_addr, nb.0);
                    }
                    HoleType::JumpOffset32 => {
                        // Compute the stitched jump target in instruction units.
                        //
                        // VM dispatch increments `ip` before executing the opcode.
                        // For Jmp/Break/Continue this means target = (pc + 1) + sAx.
                        //
                        // Test/IsVariant are "skip next instruction" branches, so
                        // they always target pc + 2 when the branch is taken.
                        let target_pc = match instr.op {
                            OpCode::IsVariant | OpCode::Test => Some(pc + 2),
                            OpCode::Jmp | OpCode::Break | OpCode::Continue => {
                                Some((pc as i64 + 1 + instr.sax_val()) as usize)
                            }
                            // Other stencils may use JumpOffset32 for internal local
                            // branches; leave their rel32 as emitted by the stencil.
                            _ => None,
                        };

                        let Some(target_pc) = target_pc else {
                            continue;
                        };

                        let target_addr = if target_pc < instruction_offsets.len() {
                            instruction_offsets[target_pc]
                        } else if matches!(instr.op, OpCode::IsVariant | OpCode::Test) {
                            // Skip-next at end of stream should jump to the final `ret`.
                            start_offset + total_size
                        } else {
                            // Leave the default zero rel32 for out-of-bounds jumps.
                            continue;
                        };
                        let rel = target_addr as i64 - (patch_addr as i64 + 4);
                        self.patch_i32(patch_addr, rel as i32);
                    }
                    HoleType::RuntimeFuncAddr => {
                        // Runtime function addresses are resolved at integration time.
                        // For now, patch with a placeholder (0) — the integrator in
                        // jit_tier.rs will fill these in.
                        // TODO: Wire up lm_rt_* function lookup table.
                        self.patch_u64(patch_addr, 0);
                    }
                    HoleType::VmContextAddr => {
                        // VmContext address patched at integration time.
                        self.patch_u64(patch_addr, 0);
                    }
                    HoleType::SBx => {
                        self.patch_i16(patch_addr, instr.sbx() as i16);
                    }
                    HoleType::InstructionWord => {
                        // Embed the full 8-byte Instruction struct as a u64 in the
                        // movabs immediate. The runtime helper transmutes the u64 back
                        // to an Instruction to decode op/a/b/c fields.
                        let word: u64 = unsafe { std::mem::transmute(*instr) };
                        self.patch_u64(patch_addr, word);
                    }
                }
            }
        }

        // Terminate stitched function with `ret` (0xC3) so that after
        // lm_rt_return returns, control flows back to call_stitched.
        self.emit_bytes(&[0xC3u8]);

        let code_len = self.code_offset - start_offset;
        let code = StitchedCode {
            code_ptr: unsafe { self.code_buffer.add(start_offset) },
            code_len,
        };

        self.compiled.insert(cell_index, code);
        Ok(self.compiled.get(&cell_index).unwrap())
    }

    // ── Emit / patch helpers ─────────────────────────────────────────

    /// Copy bytes into the code buffer at the current offset.
    #[inline]
    fn emit_bytes(&mut self, bytes: &[u8]) {
        debug_assert!(
            self.code_offset + bytes.len() <= self.code_capacity,
            "emit_bytes: buffer overflow"
        );
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                self.code_buffer.add(self.code_offset),
                bytes.len(),
            );
        }
        self.code_offset += bytes.len();
    }

    /// Patch a single byte at the given absolute offset in the code buffer.
    #[inline]
    fn patch_u8(&self, offset: usize, value: u8) {
        debug_assert!(offset < self.code_capacity);
        unsafe {
            *self.code_buffer.add(offset) = value;
        }
    }

    /// Patch a 16-bit little-endian value.
    #[inline]
    fn patch_u16(&self, offset: usize, value: u16) {
        debug_assert!(offset + 2 <= self.code_capacity);
        let bytes = value.to_le_bytes();
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.code_buffer.add(offset), 2);
        }
    }

    /// Patch a 16-bit signed little-endian value.
    #[inline]
    fn patch_i16(&self, offset: usize, value: i16) {
        self.patch_u16(offset, value as u16);
    }

    /// Patch a 32-bit little-endian value.
    #[inline]
    fn patch_u32(&self, offset: usize, value: u32) {
        debug_assert!(offset + 4 <= self.code_capacity);
        let bytes = value.to_le_bytes();
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.code_buffer.add(offset), 4);
        }
    }

    /// Patch a 32-bit signed little-endian value (for `rel32` jumps).
    #[inline]
    fn patch_i32(&self, offset: usize, value: i32) {
        self.patch_u32(offset, value as u32);
    }

    /// Patch a 64-bit little-endian value.
    #[inline]
    fn patch_u64(&self, offset: usize, value: u64) {
        debug_assert!(offset + 8 <= self.code_capacity);
        let bytes = value.to_le_bytes();
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.code_buffer.add(offset), 8);
        }
    }

    /// Public version of `patch_u64` used by the stencil tier to post-patch
    /// `RuntimeFuncAddr` holes after compilation.
    ///
    /// `byte_offset` is an absolute byte offset from the start of the code buffer.
    pub fn patch_u64_at(&self, byte_offset: usize, value: u64) {
        debug_assert!(
            byte_offset + 8 <= self.code_capacity,
            "patch_u64_at: byte_offset {byte_offset} out of bounds"
        );
        self.patch_u64(byte_offset, value);
    }

    /// Return the raw base pointer of the code buffer.
    ///
    /// Used by `stencil_tier` to compute absolute byte offsets for patching.
    pub fn code_buffer_ptr(&self) -> *const u8 {
        self.code_buffer as *const u8
    }
}

impl Drop for Stitcher {
    fn drop(&mut self) {
        if !self.code_buffer.is_null() {
            free_executable(self.code_buffer, self.code_capacity);
            self.code_buffer = std::ptr::null_mut();
        }
    }
}

// ---------------------------------------------------------------------------
// Constant → NbValue conversion
// ---------------------------------------------------------------------------

/// Convert a LIR constant to a 64-bit NbValue for patching into stencil holes.
///
/// For `Constant64` holes, the stitcher reads the LIR constant pool at
/// `instr.bx()` and converts to an NbValue. For `LoadBool` and `LoadInt`,
/// the value is derived from instruction fields instead.
fn constant_to_nb(
    instr: &Instruction,
    cell: &LirCell,
    _module: &LirModule,
) -> Result<NbValue, StitchError> {
    let op = instr.op;

    // LoadBool: value is in instr.b (0 = false, nonzero = true)
    if op == OpCode::LoadBool {
        return Ok(NbValue::new_bool(instr.b != 0));
    }

    // LoadInt: value is in sbx (sign-extended 32-bit)
    if op == OpCode::LoadInt {
        return Ok(NbValue::new_int(instr.sbx() as i64));
    }

    // LoadK and others: read from constant pool at bx()
    let idx = instr.bx() as usize;
    if idx >= cell.constants.len() {
        return Err(StitchError::ConstantOutOfBounds(instr.bx()));
    }

    let nb = match &cell.constants[idx] {
        Constant::Null => NbValue::new_null(),
        Constant::Bool(b) => NbValue::new_bool(*b),
        Constant::Int(n) => {
            if *n >= NbValue::MIN_INT48 && *n <= NbValue::MAX_INT48 {
                NbValue::new_int(*n)
            } else {
                // Large int: box on the heap. For the stitcher this is a cold path.
                NbValue::new_ptr(std::sync::Arc::into_raw(std::sync::Arc::new(
                    lumen_core::values::Value::BigInt(num_bigint::BigInt::from(*n)),
                )))
            }
        }
        Constant::BigInt(n) => {
            // Always heap-boxed.
            NbValue::new_ptr(std::sync::Arc::into_raw(std::sync::Arc::new(
                lumen_core::values::Value::BigInt(n.clone()),
            )))
        }
        Constant::Float(f) => NbValue::new_float(*f),
        Constant::String(s) => {
            // Strings are heap-boxed.
            NbValue::new_ptr(std::sync::Arc::into_raw(std::sync::Arc::new(
                lumen_core::values::Value::String(lumen_core::values::StringRef::Owned(s.clone())),
            )))
        }
        Constant::NbValue(bits) => NbValue(*bits),
    };

    Ok(nb)
}

// ---------------------------------------------------------------------------
// Platform-specific executable memory allocation
// ---------------------------------------------------------------------------

/// Allocate a read-write-execute memory region.
#[cfg(unix)]
fn alloc_executable(size: usize) -> Result<*mut u8, StitchError> {
    use std::ptr;

    let ptr = unsafe {
        libc::mmap(
            ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };

    if ptr == libc::MAP_FAILED {
        Err(StitchError::MmapFailed(format!(
            "mmap({size} bytes) failed: {}",
            std::io::Error::last_os_error()
        )))
    } else {
        Ok(ptr as *mut u8)
    }
}

/// Free a previously allocated executable region.
#[cfg(unix)]
fn free_executable(ptr: *mut u8, size: usize) {
    unsafe {
        libc::munmap(ptr as *mut libc::c_void, size);
    }
}

/// Fallback for non-Unix platforms: use a regular Vec<u8>.
///
/// WARNING: Code in this buffer is NOT actually executable. This is a
/// compilation stub so the crate can build on non-Unix (e.g. WASM targets).
/// Attempting to call into StitchedCode on these platforms will segfault.
#[cfg(not(unix))]
fn alloc_executable(size: usize) -> Result<*mut u8, StitchError> {
    let mut buf = vec![0u8; size];
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    Ok(ptr)
}

#[cfg(not(unix))]
fn free_executable(ptr: *mut u8, size: usize) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr, 0, size);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stencils::build_stencil_library;

    fn empty_module() -> LirModule {
        LirModule {
            version: "1.0".into(),
            doc_hash: String::new(),
            strings: vec![],
            types: vec![],
            cells: vec![],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            handlers: vec![],
            effect_binds: vec![],
        }
    }

    #[test]
    fn test_stitcher_creation() {
        let lib = build_stencil_library();
        let stitcher = Stitcher::new(lib).unwrap();
        assert_eq!(stitcher.code_used(), 0);
        assert!(stitcher.code_available() > 0);
    }

    #[test]
    fn test_compile_simple_cell() {
        let lib = build_stencil_library();
        let mut stitcher = Stitcher::new(lib).unwrap();

        // A cell that loads an int constant and returns it:
        // LoadInt r0, 42
        // Return r0, 1, 0
        let cell = LirCell {
            name: "test_cell".into(),
            params: vec![],
            returns: Some("Int".into()),
            registers: 1,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 0, 42),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: vec![],
            osr_points: vec![],
        };

        let module = empty_module();
        let code = stitcher.compile(&cell, &module, 0).unwrap();
        assert!(code.code_len > 0);
        assert!(!code.code_ptr.is_null());
    }

    #[test]
    fn test_compile_arithmetic_cell() {
        let lib = build_stencil_library();
        let mut stitcher = Stitcher::new(lib).unwrap();

        // r0 = LoadInt 10
        // r1 = LoadInt 20
        // r2 = Add r0, r1
        // Return r2
        let cell = LirCell {
            name: "add_test".into(),
            params: vec![],
            returns: Some("Int".into()),
            registers: 3,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 0, 10),
                Instruction::abx(OpCode::LoadInt, 1, 20),
                Instruction::abc(OpCode::Add, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: vec![],
            osr_points: vec![],
        };

        let module = empty_module();
        let code = stitcher.compile(&cell, &module, 0).unwrap();

        // Verify the code is non-trivial (LoadInt=17 bytes each, Add≈101 bytes, Return=20 bytes)
        assert!(code.code_len > 50, "code_len={}", code.code_len);
    }

    #[test]
    fn test_compile_with_constants() {
        let lib = build_stencil_library();
        let mut stitcher = Stitcher::new(lib).unwrap();

        // r0 = LoadK constants[0]  (where constants[0] = Float(3.14))
        // Return r0
        let cell = LirCell {
            name: "const_test".into(),
            params: vec![],
            returns: Some("Float".into()),
            registers: 1,
            constants: vec![Constant::Float(3.14)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: vec![],
            osr_points: vec![],
        };

        let module = empty_module();
        let code = stitcher.compile(&cell, &module, 0).unwrap();
        assert!(code.code_len > 0);
    }

    #[test]
    fn test_compile_jump() {
        let lib = build_stencil_library();
        let mut stitcher = Stitcher::new(lib).unwrap();

        // r0 = LoadInt 1
        // Jmp +1          (skip the next instruction)
        // r0 = LoadInt 2  (should be skipped)
        // Return r0
        let cell = LirCell {
            name: "jmp_test".into(),
            params: vec![],
            returns: Some("Int".into()),
            registers: 1,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::LoadInt, 0, 1),
                Instruction::sax(OpCode::Jmp, 1),
                Instruction::abx(OpCode::LoadInt, 0, 2),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: vec![],
            osr_points: vec![],
        };

        let module = empty_module();
        let code = stitcher.compile(&cell, &module, 0).unwrap();
        assert!(code.code_len > 0);
    }

    #[test]
    fn test_missing_stencil_error() {
        // Create an empty library (no stencils).
        let lib = StencilLibrary::new();
        let mut stitcher = Stitcher::with_capacity(lib, 4096).unwrap();

        let cell = LirCell {
            name: "fail".into(),
            params: vec![],
            returns: None,
            registers: 1,
            constants: vec![],
            instructions: vec![Instruction::abc(OpCode::Nop, 0, 0, 0)],
            effect_handler_metas: vec![],
            osr_points: vec![],
        };

        let module = empty_module();
        let result = stitcher.compile(&cell, &module, 0);
        assert!(result.is_err());
        if let Err(StitchError::MissingStencil(op, _)) = result {
            assert_eq!(op, OpCode::Nop as u8);
        }
    }

    #[test]
    fn test_code_caching() {
        let lib = build_stencil_library();
        let mut stitcher = Stitcher::new(lib).unwrap();

        let cell = LirCell {
            name: "cached".into(),
            params: vec![],
            returns: None,
            registers: 0,
            constants: vec![],
            instructions: vec![Instruction::abc(OpCode::Nop, 0, 0, 0)],
            effect_handler_metas: vec![],
            osr_points: vec![],
        };

        let module = empty_module();
        let _ = stitcher.compile(&cell, &module, 42).unwrap();

        // Should be retrievable from cache.
        assert!(stitcher.get_compiled(42).is_some());
        assert!(stitcher.get_compiled(99).is_none());
    }

    #[test]
    fn test_patch_register_displacement() {
        // Verify that register holes are patched with correct byte offsets.
        // Register 5 should produce displacement 5 * 8 = 40 = 0x28.
        let lib = build_stencil_library();
        let mut stitcher = Stitcher::new(lib).unwrap();

        let cell = LirCell {
            name: "reg_patch".into(),
            params: vec![],
            returns: None,
            registers: 10,
            constants: vec![],
            instructions: vec![
                // Move r5 = r3
                Instruction::abc(OpCode::Move, 5, 3, 0),
            ],
            effect_handler_metas: vec![],
            osr_points: vec![],
        };

        let module = empty_module();
        let code = stitcher.compile(&cell, &module, 0).unwrap();

        // The Move stencil is 14 bytes, plus the stitcher appends a trailing `ret` (1 byte):
        //   bytes 0-6:  mov rax, [r14 + RegB*8]  → hole RegB at 3 (4 bytes)
        //   bytes 7-13: mov [r14 + RegA*8], rax  → hole RegA at 10 (4 bytes)
        //   byte 14:    ret (0xC3) — added by Stitcher::compile
        //
        // RegB = 3: displacement = 3 * 8 = 24 = 0x18
        // RegA = 5: displacement = 5 * 8 = 40 = 0x28
        assert_eq!(code.code_len, 15); // 14 bytes stencil + 1 byte trailing ret

        let code_bytes = unsafe { std::slice::from_raw_parts(code.code_ptr, code.code_len) };

        // Check RegB displacement at offset 3 (4 bytes LE)
        let reg_b_disp =
            i32::from_le_bytes([code_bytes[3], code_bytes[4], code_bytes[5], code_bytes[6]]);
        assert_eq!(reg_b_disp, 24, "RegB displacement should be 3*8=24");

        // Check RegA displacement at offset 10 (4 bytes LE)
        let reg_a_disp = i32::from_le_bytes([
            code_bytes[10],
            code_bytes[11],
            code_bytes[12],
            code_bytes[13],
        ]);
        assert_eq!(reg_a_disp, 40, "RegA displacement should be 5*8=40");
    }

    #[test]
    fn test_is_variant_jump_patches_to_skip_next_stencil() {
        let lib = build_stencil_library();
        let mut stitcher = Stitcher::new(lib).unwrap();

        let is_variant = stitcher
            .library()
            .get(OpCode::IsVariant as u8)
            .expect("IsVariant stencil missing")
            .clone();
        let nop = stitcher
            .library()
            .get(OpCode::Nop as u8)
            .expect("Nop stencil missing")
            .clone();
        let jump_hole = is_variant
            .holes
            .iter()
            .find(|h| h.hole_type == HoleType::JumpOffset32)
            .expect("IsVariant missing JumpOffset32 hole");

        // IsVariant should skip exactly one instruction (the Nop) and land on Return.
        let cell = LirCell {
            name: "is_variant_skip".into(),
            params: vec![],
            returns: None,
            registers: 1,
            constants: vec![],
            instructions: vec![
                Instruction::abx(OpCode::IsVariant, 0, 0),
                Instruction::abc(OpCode::Nop, 0, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: vec![],
            osr_points: vec![],
        };

        let module = empty_module();
        let code = stitcher.compile(&cell, &module, 0).unwrap();
        let code_bytes = unsafe { std::slice::from_raw_parts(code.code_ptr, code.code_len) };

        let rel = i32::from_le_bytes([
            code_bytes[jump_hole.offset as usize],
            code_bytes[jump_hole.offset as usize + 1],
            code_bytes[jump_hole.offset as usize + 2],
            code_bytes[jump_hole.offset as usize + 3],
        ]);

        let target_addr = is_variant.code.len() + nop.code.len();
        let expected_rel = target_addr as i32 - (jump_hole.offset as i32 + 4);
        assert_eq!(
            rel, expected_rel,
            "IsVariant jump should land on the instruction after next"
        );
    }
}
