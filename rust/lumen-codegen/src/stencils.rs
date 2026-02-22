//! Pre-built stencil definitions for the copy-and-patch Tier 1 JIT.
//!
//! Each public function returns a [`StencilDef`] containing x86-64 machine
//! code bytes and hole descriptors for one LIR opcode.  Call
//! [`build_stencil_library`] to get a [`StencilLibrary`] with all stencils.
//!
//! ## Machine code conventions
//!
//! Target: **x86-64** (Linux/macOS SysV AMD64 ABI).
//!
//! | Register | Role |
//! |----------|------|
//! | `r14`    | Base of `NbValue` register file (`*mut NbValue`) |
//! | `r15`    | `VmContext*` |
//! | `rax`, `rcx`, `rdx` | Scratch (caller-saved) |
//! | `r8`–`r11`          | Additional scratch (caller-saved) |
//!
//! Each NbValue is 8 bytes, so `regs[n]` lives at `[r14 + n*8]`.
//!
//! ## Common x86-64 encodings used here
//!
//! ```text
//! mov rax, [r14 + disp32]  →  49 8B 86 <disp32:4>
//! mov rcx, [r14 + disp32]  →  49 8B 8E <disp32:4>
//! mov rdx, [r14 + disp32]  →  49 8B 96 <disp32:4>
//! mov [r14 + disp32], rax  →  49 89 86 <disp32:4>
//! mov [r14 + disp32], rcx  →  49 89 8E <disp32:4>
//! movabs rax, imm64        →  48 B8 <imm64:8>
//! movabs rcx, imm64        →  48 B9 <imm64:8>
//! movabs rdx, imm64        →  48 BA <imm64:8>
//! mov rdx, rax             →  48 89 C2
//! mov rax, rcx             →  48 89 C8
//! add rax, rcx             →  48 03 C1
//! sub rax, rcx             →  48 2B C1
//! imul rax, rcx            →  48 0F AF C1
//! neg rax                  →  48 F7 D8
//! and rax, rcx             →  48 23 C1
//! or  rax, rcx             →  48 0B C1
//! xor rax, rcx             →  48 33 C1
//! not rax                  →  48 F7 D0
//! shl rax, imm8            →  48 C1 E0 <imm8>
//! shr rax, imm8            →  48 C1 E8 <imm8>
//! sar rax, imm8            →  48 C1 F8 <imm8>
//! cmp edx, imm32           →  81 FA <imm32:4>
//! je  rel32                →  0F 84 <rel32:4>
//! jne rel32                →  0F 85 <rel32:4>
//! jl  rel32                →  0F 8C <rel32:4>
//! jle rel32                →  0F 8E <rel32:4>
//! jmp rel32                →  E9 <rel32:4>
//! call rax                 →  FF D0
//! ret                      →  C3
//! nop                      →  90
//! test rax, rax            →  48 85 C0
//! cmp rax, rcx             →  48 3B C1
//! sete al                  →  0F 94 C0
//! setne al                 →  0F 95 C0
//! setl al                  →  0F 9C C0
//! setle al                 →  0F 9E C0
//! setg al                  →  0F 9F C0
//! setge al                 →  0F 9D C0
//! movzx rax, al            →  48 0F B6 C0
//! ```
//!
//! ## NaN-boxing constants
//!
//! ```text
//! NAN_MASK     = 0x7FF8_0000_0000_0000
//! TAG_INT_BASE = 0x7FF9_0000_0000_0000  (NAN_MASK | TAG_INT << 48)
//! TAG_BOOL_BASE= 0x7FFB_0000_0000_0000  (NAN_MASK | TAG_BOOL << 48)
//! NULL_VALUE   = 0x7FFC_0000_0000_0000  (NAN_MASK | TAG_NULL << 48)
//! TRUE_VALUE   = 0x7FFB_0000_0000_0001
//! FALSE_VALUE  = 0x7FFB_0000_0000_0000
//! PAYLOAD_MASK = 0x0000_FFFF_FFFF_FFFF
//! TAG_CHECK_MASK = 0x7FFF_0000_0000_0000
//! ```
//!
//! Int tag check (top 16 bits == 0x7FF9):
//! ```text
//! mov rdx, rax         ; 48 89 C2
//! shr rdx, 48          ; 48 C1 EA 30
//! cmp edx, 0x7FF9      ; 81 FA F9 7F 00 00
//! jne <slow_path>      ; 0F 85 <rel32>
//! ```

use lumen_core::lir::OpCode;

use crate::stencil_format::{HoleDef, HoleType, StencilDef, StencilLibrary};

// ---------------------------------------------------------------------------
// Compiled NaN-box constants (little-endian byte arrays)
// ---------------------------------------------------------------------------

/// `NULL_VALUE = 0x7FFC_0000_0000_0000` in little-endian bytes.
const NULL_VALUE_LE: [u8; 8] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFC, 0x7F];
/// `TRUE_VALUE  = 0x7FFB_0000_0000_0001` in little-endian bytes.
/// Exposed for stitcher reference (used when computing `NbValue::new_bool(true)`).
#[allow(dead_code)]
const TRUE_VALUE_LE: [u8; 8] = [0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFB, 0x7F];
/// `FALSE_VALUE = 0x7FFB_0000_0000_0000` in little-endian bytes.
const FALSE_VALUE_LE: [u8; 8] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFB, 0x7F];
/// `TAG_INT_BASE = 0x7FF9_0000_0000_0000` in little-endian bytes.
const TAG_INT_BASE_LE: [u8; 8] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF9, 0x7F];
/// `PAYLOAD_MASK = 0x0000_FFFF_FFFF_FFFF` in little-endian bytes.
const PAYLOAD_MASK_LE: [u8; 8] = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00];

// ---------------------------------------------------------------------------
// Helper macros for building code byte vectors
// ---------------------------------------------------------------------------

/// Concatenate multiple byte-slice expressions into a single `Vec<u8>`.
macro_rules! code {
    ($($chunk:expr),* $(,)?) => {{
        let mut v: Vec<u8> = Vec::new();
        $(v.extend_from_slice(&$chunk[..]);)*
        v
    }};
}

// ---------------------------------------------------------------------------
// Misc
// ---------------------------------------------------------------------------

/// **Nop** — no operation.
///
/// ```text
/// nop     →  90
/// ```
pub fn stencil_nop() -> StencilDef {
    StencilDef::new(
        OpCode::Nop as u8,
        "Nop",
        vec![0x90], // nop
        vec![],
    )
}

// ---------------------------------------------------------------------------
// Register and constant ops
// ---------------------------------------------------------------------------

/// **LoadK** — load constant from pool into register A.
///
/// The constant value (converted to a 64-bit NbValue) is patched in at
/// stitch time; no runtime constant-pool indirection on the hot path.
///
/// ```text
/// movabs rax, <CONST:8>    →  48 B8 [hole: Constant64, 8 bytes]
/// mov [r14+A*8], rax       →  49 89 86 [hole: RegA, 4 bytes]
/// ```
pub fn stencil_loadk() -> StencilDef {
    StencilDef::new(
        OpCode::LoadK as u8,
        "LoadK",
        code!(
            [0x48u8, 0xB8],       // movabs rax, ...
            [0x00u8; 8],          // hole: Constant64 (8 bytes)
            [0x49u8, 0x89, 0x86], // mov [r14+disp32], rax
            [0x00u8; 4],          // hole: RegA (4 bytes)
        ),
        vec![
            HoleDef::new(2, HoleType::Constant64, 8),
            HoleDef::new(13, HoleType::RegA, 4),
        ],
    )
}

/// **LoadNil** — set register A to `Null`.
///
/// `NULL_VALUE = 0x7FFC_0000_0000_0000` (NAN_MASK | TAG_NULL << 48).
///
/// ```text
/// movabs rax, 0x7FFC_0000_0000_0000   →  48 B8 [8 const bytes]
/// mov [r14+A*8], rax                   →  49 89 86 [hole: RegA, 4]
/// ```
pub fn stencil_loadnil() -> StencilDef {
    StencilDef::new(
        OpCode::LoadNil as u8,
        "LoadNil",
        code!(
            [0x48u8, 0xB8], // movabs rax, ...
            NULL_VALUE_LE,  // 0x7FFC_0000_0000_0000
            [0x49u8, 0x89, 0x86],
            [0x00u8; 4], // hole: RegA
        ),
        vec![HoleDef::new(13, HoleType::RegA, 4)],
    )
}

/// **LoadBool** — load a boolean constant into register A.
///
/// The bool value (true or false) is resolved at stitch time from `instr.b`
/// and written as a full NbValue:
/// - `true`  → `0x7FFB_0000_0000_0001`
/// - `false` → `0x7FFB_0000_0000_0000`
///
/// ```text
/// movabs rax, <BOOL_NBVAL:8>    →  48 B8 [hole: Constant64, 8]
/// mov [r14+A*8], rax             →  49 89 86 [hole: RegA, 4]
/// ```
///
/// *Note*: `HoleType::Constant64` is used here because the stitcher must
/// compute `NbValue::new_bool(instr.b != 0)` at stitch time, exactly like it
/// would for a constant-pool entry.
pub fn stencil_loadbool() -> StencilDef {
    StencilDef::new(
        OpCode::LoadBool as u8,
        "LoadBool",
        code!(
            [0x48u8, 0xB8],
            [0x00u8; 8], // hole: Constant64 — stitcher writes NbValue bool
            [0x49u8, 0x89, 0x86],
            [0x00u8; 4], // hole: RegA
        ),
        vec![
            HoleDef::new(2, HoleType::Constant64, 8),
            HoleDef::new(13, HoleType::RegA, 4),
        ],
    )
}

/// **LoadInt** — load a small signed integer immediate into register A.
///
/// `sBx` (sign-extended 16-bit field) is the integer value.  The stitcher
/// computes `NbValue::new_int(instr.sbx() as i64)` and patches it in.
///
/// ```text
/// movabs rax, <INT_NBVAL:8>    →  48 B8 [hole: Constant64, 8]
/// mov [r14+A*8], rax            →  49 89 86 [hole: RegA, 4]
/// ```
pub fn stencil_loadint() -> StencilDef {
    StencilDef::new(
        OpCode::LoadInt as u8,
        "LoadInt",
        code!(
            [0x48u8, 0xB8],
            [0x00u8; 8], // hole: Constant64 — stitcher writes NbValue::new_int(sbx)
            [0x49u8, 0x89, 0x86],
            [0x00u8; 4], // hole: RegA
        ),
        vec![
            HoleDef::new(2, HoleType::Constant64, 8),
            HoleDef::new(13, HoleType::RegA, 4),
        ],
    )
}

/// **Move** — copy register B to register A.
///
/// ```text
/// mov rax, [r14+B*8]   →  49 8B 86 [hole: RegB, 4]
/// mov [r14+A*8], rax   →  49 89 86 [hole: RegA, 4]
/// ```
pub fn stencil_move() -> StencilDef {
    StencilDef::new(
        OpCode::Move as u8,
        "Move",
        code!(
            [0x49u8, 0x8B, 0x86],
            [0x00u8; 4], // hole: RegB (load source)
            [0x49u8, 0x89, 0x86],
            [0x00u8; 4], // hole: RegA (store dest)
        ),
        vec![
            HoleDef::new(3, HoleType::RegB, 4),
            HoleDef::new(10, HoleType::RegA, 4),
        ],
    )
}

/// **MoveOwn** — same encoding as Move (semantics differ only for GC).
pub fn stencil_moveown() -> StencilDef {
    let mut s = stencil_move();
    s.opcode = OpCode::MoveOwn as u8;
    s.name = "MoveOwn".into();
    s
}

// ---------------------------------------------------------------------------
// Arithmetic stencils
// ---------------------------------------------------------------------------
//
// For arithmetic, each stencil implements an **SMI (small integer) fast path**
// inline, followed by a **runtime fallback** that calls `lm_rt_stencil_runtime`
// for non-integer operands (Float, String, etc.).
//
// ## Fast-path machine code structure (Add as example)
//
// ```text
// ;;; Load operands
// mov rax, [r14+B*8]        ; 49 8B 86 <RegB:4>         — 7 bytes, hole at  3
// mov rcx, [r14+C*8]        ; 49 8B 8E <RegC:4>         — 7 bytes, hole at 10
//
// ;;; Int tag check for rax  (top 16 bits == 0x7FF9?)
// mov rdx, rax              ; 48 89 C2                  — 3 bytes
// shr rdx, 48               ; 48 C1 EA 30               — 4 bytes
// cmp edx, 0x7FF9           ; 81 FA F9 7F 00 00         — 6 bytes
// jne .slow                 ; 0F 85 <baked-rel32>       — 6 bytes (offset 28)
//
// ;;; Int tag check for rcx
// mov rdx, rcx              ; 48 89 D2                  — 3 bytes
// shr rdx, 48               ; 48 C1 EA 30               — 4 bytes
// cmp edx, 0x7FF9           ; 81 FA F9 7F 00 00         — 6 bytes
// jne .slow                 ; 0F 85 <baked-rel32>       — 6 bytes (offset 47)
//
// ;;; Sign-extend 48-bit payloads
// shl rax, 16               ; 48 C1 E0 10               — 4 bytes
// sar rax, 16               ; 48 C1 F8 10               — 4 bytes
// shl rcx, 16               ; 48 C1 E1 10               — 4 bytes
// sar rcx, 16               ; 48 C1 F9 10               — 4 bytes
//
// ;;; Compute result
// add rax, rcx              ; 48 03 C1                  — 3 bytes
//
// ;;; Re-box as TAG_INT: result = TAG_INT_BASE | (rax & PAYLOAD_MASK)
// movabs rdx, PAYLOAD_MASK  ; 48 BA <8 bytes>           — 10 bytes
// and rax, rdx              ; 48 23 C2                  — 3 bytes
// movabs rdx, TAG_INT_BASE  ; 48 BA <8 bytes>           — 10 bytes
// or  rax, rdx              ; 48 0B C2                  — 3 bytes
//
// ;;; Store result
// mov [r14+A*8], rax        ; 49 89 86 <RegA:4>        — 7 bytes, hole
//
// ;; fall through to next stencil
//
// .slow:  (offset = 101 + compute_len)
// ;; Runtime fallback: lm_rt_stencil_runtime(ctx, instr_word)
// mov rdi, r15              ; 4C 89 FF — arg1 = VmContext*
// movabs rax, <instr_word>  ; 48 B8 <InstructionWord:8>
// mov rsi, rax              ; 48 89 C6 — arg2 = instr_word
// movabs rax, <func>        ; 48 B8 <RuntimeFuncAddr:8>
// sub rsp, 8                ; 48 83 EC 08 (align RSP)
// call rax                  ; FF D0
// add rsp, 8                ; 48 83 C4 08 (restore RSP)
// ```
//
// The `jne .slow` rel32 offsets are baked into the stencil at build time
// (they point to a fixed offset within the same stencil); they are NOT
// `HoleType::JumpOffset32` holes.

/// Build the int-tag-check sequence for `rax` (check top 16 bits == 0x7FF9).
///
/// Returns bytes and the offset of the `jne rel32` hole within those bytes.
///
/// ```text
/// mov rdx, rax     48 89 C2
/// shr rdx, 48      48 C1 EA 30
/// cmp edx, 0x7FF9  81 FA F9 7F 00 00
/// jne rel32        0F 85 [hole: JumpOffset32, 4]
/// ```
///
/// Total: 19 bytes.  The `jne` hole is at offset 16 (relative to the start
/// of this sequence).
fn tag_check_rax() -> Vec<u8> {
    code!(
        [0x48u8, 0x89, 0xC2],                   // mov rdx, rax
        [0x48u8, 0xC1, 0xEA, 0x30],             // shr rdx, 48
        [0x81u8, 0xFA, 0xF9, 0x7F, 0x00, 0x00], // cmp edx, 0x7FF9
        [0x0Fu8, 0x85, 0x00, 0x00, 0x00, 0x00], // jne rel32
    )
}

/// Same as `tag_check_rax` but for `rcx`.
///
/// ```text
/// mov rdx, rcx     48 89 D2
/// shr rdx, 48      48 C1 EA 30
/// cmp edx, 0x7FF9  81 FA F9 7F 00 00
/// jne rel32        0F 85 [hole: JumpOffset32, 4]
/// ```
fn tag_check_rcx() -> Vec<u8> {
    code!(
        [0x48u8, 0x89, 0xD2],                   // mov rdx, rcx
        [0x48u8, 0xC1, 0xEA, 0x30],             // shr rdx, 48
        [0x81u8, 0xFA, 0xF9, 0x7F, 0x00, 0x00], // cmp edx, 0x7FF9
        [0x0Fu8, 0x85, 0x00, 0x00, 0x00, 0x00], // jne rel32
    )
}

/// Sign-extend 48-bit payload of `rax` to 64 bits via `shl 16 / sar 16`.
///
/// ```text
/// shl rax, 16    48 C1 E0 10
/// sar rax, 16    48 C1 F8 10
/// ```
fn sign_extend_rax() -> Vec<u8> {
    code!(
        [0x48u8, 0xC1, 0xE0, 0x10], // shl rax, 16
        [0x48u8, 0xC1, 0xF8, 0x10], // sar rax, 16
    )
}

/// Sign-extend 48-bit payload of `rcx` to 64 bits via `shl 16 / sar 16`.
///
/// ```text
/// shl rcx, 16    48 C1 E1 10
/// sar rcx, 16    48 C1 F9 10
/// ```
fn sign_extend_rcx() -> Vec<u8> {
    code!(
        [0x48u8, 0xC1, 0xE1, 0x10], // shl rcx, 16
        [0x48u8, 0xC1, 0xF9, 0x10], // sar rcx, 16
    )
}

/// Re-box `rax` (raw signed i64) as a TAG_INT NbValue.
///
/// ```text
/// movabs rdx, PAYLOAD_MASK   48 BA FF FF FF FF FF FF 00 00
/// and rax, rdx               48 23 C2
/// movabs rdx, TAG_INT_BASE   48 BA 00 00 00 00 00 00 F9 7F
/// or rax, rdx                48 0B C2
/// ```
fn rebox_int_rax() -> Vec<u8> {
    code!(
        [0x48u8, 0xBA], // movabs rdx, PAYLOAD_MASK
        PAYLOAD_MASK_LE,
        [0x48u8, 0x23, 0xC2], // and rax, rdx
        [0x48u8, 0xBA],       // movabs rdx, TAG_INT_BASE
        TAG_INT_BASE_LE,
        [0x48u8, 0x0B, 0xC2], // or rax, rdx
    )
}

/// Binary arithmetic stencil template.
///
/// Emits the load / tag-check / sign-extend / op / re-box / store sequence,
/// followed by a slow-path tail that calls `lm_rt_stencil_runtime(ctx, instr)`
/// when either operand is not a TAG_INT.
///
/// `compute_bytes` is the machine code for the actual operation on `rax` (lhs)
/// and `rcx` (rhs), leaving the result in `rax`.
///
/// ## Layout
///
/// ```text
/// ;; Fast path
/// 0:  mov rax, [r14+B*8]   7 bytes  → hole RegB at 3
/// 7:  mov rcx, [r14+C*8]   7 bytes  → hole RegC at 10
/// 14: tag_check_rax        19 bytes (jne baked-in rel32 at 30 → slow path)
/// 33: tag_check_rcx        19 bytes (jne baked-in rel32 at 49 → slow path)
/// 52: sign_extend_rax       8 bytes
/// 60: sign_extend_rcx       8 bytes
/// 68: compute               varies
/// 68+N: rebox_int_rax      26 bytes
/// 94+N: mov [r14+A*8], rax  7 bytes → hole RegA at (94+N-4) = (94+N-4)
///
/// ;; Slow path (offset = 101 + compute_len)
/// +0:  mov rdi, r15         3 bytes   (VmContext* arg1)
/// +3:  movabs rax, instr   10 bytes   hole InstructionWord at (+5)
/// +13: mov rsi, rax         3 bytes   (instr_word arg2)
/// +16: movabs rax, func    10 bytes   hole RuntimeFuncAddr at (+18)
/// +26: sub rsp, 8           4 bytes   (16-byte stack align)
/// +28: call rax             2 bytes
/// +30: add rsp, 8           4 bytes
/// ;; total slow path: 34 bytes
/// ```
fn arith_stencil_abc(opcode: u8, name: &str, compute_bytes: &[u8]) -> StencilDef {
    let compute_len = compute_bytes.len();

    // Fast-path ends at byte: 7+7+19+19+8+8+compute_len+26+7 = 101 + compute_len
    let fast_path_end = 101 + compute_len;

    // Bake the rel32 offsets for the two `jne` instructions.
    //
    // The first tag-check `jne` has rel32 at [29..33), instruction end = 33.
    // The second tag-check `jne` has rel32 at [48..52), instruction end = 52.
    //
    // Both branches must land at `slow_path_start = fast_path_end`.
    let jne1_rel = fast_path_end as i32 - 33;
    let jne2_rel = fast_path_end as i32 - 52;
    let jne1_rel_le = jne1_rel.to_le_bytes();
    let jne2_rel_le = jne2_rel.to_le_bytes();

    // Build tag_check_rax with baked-in slow-path jne rel32.
    let tag_check_rax_baked: Vec<u8> = vec![
        0x48,
        0x89,
        0xC2, // mov rdx, rax
        0x48,
        0xC1,
        0xEA,
        0x30, // shr rdx, 48
        0x81,
        0xFA,
        0xF9,
        0x7F,
        0x00,
        0x00, // cmp edx, 0x7FF9
        0x0F,
        0x85, // jne rel32
        jne1_rel_le[0],
        jne1_rel_le[1],
        jne1_rel_le[2],
        jne1_rel_le[3],
    ];

    // Build tag_check_rcx with baked-in slow-path jne rel32.
    let tag_check_rcx_baked: Vec<u8> = vec![
        0x48,
        0x89,
        0xD2, // mov rdx, rcx
        0x48,
        0xC1,
        0xEA,
        0x30, // shr rdx, 48
        0x81,
        0xFA,
        0xF9,
        0x7F,
        0x00,
        0x00, // cmp edx, 0x7FF9
        0x0F,
        0x85, // jne rel32
        jne2_rel_le[0],
        jne2_rel_le[1],
        jne2_rel_le[2],
        jne2_rel_le[3],
    ];

    let store_offset = 68 + compute_len + 26 + 3;

    // Slow-path layout (relative to slow_path_start = fast_path_end = 101+compute_len):
    //  +0:  4C 89 FF            mov rdi, r15          3 bytes
    //  +3:  48 B8 <8>           movabs rax, instr    10 bytes, InstructionWord hole at +5
    // +13:  48 89 C6            mov rsi, rax          3 bytes
    // +16:  48 B8 <8>           movabs rax, func     10 bytes, RuntimeFuncAddr hole at +18
    // +26:  48 83 EC 08         sub rsp, 8            4 bytes
    // +28: (would be +26 before sub rsp was 4 bytes)
    // Actually: +26 FF D0       call rax              2 bytes
    // +28:  48 83 C4 08         add rsp, 8            4 bytes
    // total slow path = 3+10+3+10+4+2+4 = 36 bytes
    let instr_word_hole_offset = (fast_path_end + 5) as u32;
    let runtime_func_hole_offset = (fast_path_end + 18) as u32;

    let mut code = code!(
        [0x49u8, 0x8B, 0x86],
        [0x00u8; 4], // mov rax, [r14+B*8]
        [0x49u8, 0x8B, 0x8E],
        [0x00u8; 4], // mov rcx, [r14+C*8]
    );
    code.extend_from_slice(&tag_check_rax_baked);
    code.extend_from_slice(&tag_check_rcx_baked);
    code.extend_from_slice(&sign_extend_rax());
    code.extend_from_slice(&sign_extend_rcx());
    code.extend_from_slice(compute_bytes);
    code.extend_from_slice(&rebox_int_rax());
    // mov [r14+A*8], rax
    code.extend_from_slice(&[0x49, 0x89, 0x86]);
    code.extend_from_slice(&[0x00; 4]); // hole: RegA

    // Slow path: call lm_rt_stencil_runtime(ctx, instr_word)
    code.extend_from_slice(&[
        0x4C, 0x89, 0xFF, // mov rdi, r15
        0x48, 0xB8, 0x00, 0x00, 0x00, 0x00, // movabs rax, <instr_word>  (low 8 of imm64)
        0x00, 0x00, 0x00, 0x00, //   (high 4 bytes of imm64)
        0x48, 0x89, 0xC6, // mov rsi, rax
        0x48, 0xB8, 0x00, 0x00, 0x00, 0x00, // movabs rax, <lm_rt_stencil_runtime>
        0x00, 0x00, 0x00, 0x00, //   (high 4 bytes)
        0x48, 0x83, 0xEC, 0x08, // sub rsp, 8
        0xFF, 0xD0, // call rax
        0x48, 0x83, 0xC4, 0x08, // add rsp, 8
    ]);

    let holes = vec![
        HoleDef::new(3, HoleType::RegB, 4),
        HoleDef::new(10, HoleType::RegC, 4),
        // Note: jne rel32 at offsets 30 and 49 are now baked-in constants pointing
        // to the slow-path tail; they are no longer HoleType::JumpOffset32.
        HoleDef::new(store_offset as u32, HoleType::RegA, 4),
        HoleDef::new(instr_word_hole_offset, HoleType::InstructionWord, 8),
        HoleDef::new(runtime_func_hole_offset, HoleType::RuntimeFuncAddr, 8),
    ];

    StencilDef::new(opcode, name, code, holes)
}

/// **Add** — `R[A] = R[B] + R[C]` (SMI fast path, runtime fallback on type mismatch).
///
/// Fast path: both operands are TAG_INT, extract 48-bit signed payloads,
/// add, re-box.  The `jne` holes jump to a slow-path stencil (patched by
/// stitcher; currently a TODO stub).
///
/// ```text
/// add rax, rcx  →  48 03 C1
/// ```
pub fn stencil_add() -> StencilDef {
    // add rax, rcx   48 03 C1
    arith_stencil_abc(OpCode::Add as u8, "Add", &[0x48, 0x03, 0xC1])
}

/// **Sub** — `R[A] = R[B] - R[C]` (SMI fast path).
///
/// ```text
/// sub rax, rcx  →  48 2B C1
/// ```
pub fn stencil_sub() -> StencilDef {
    // sub rax, rcx   48 2B C1
    arith_stencil_abc(OpCode::Sub as u8, "Sub", &[0x48, 0x2B, 0xC1])
}

/// **Mul** — `R[A] = R[B] * R[C]` (SMI fast path).
///
/// ```text
/// imul rax, rcx  →  48 0F AF C1
/// ```
pub fn stencil_mul() -> StencilDef {
    // imul rax, rcx   48 0F AF C1
    arith_stencil_abc(OpCode::Mul as u8, "Mul", &[0x48, 0x0F, 0xAF, 0xC1])
}

/// **Div** — `R[A] = R[B] / R[C]` (integer division, SMI fast path).
///
/// ```text
/// cqo           ; sign-extend rax into rdx:rax   48 99
/// idiv rcx      ; signed divide rdx:rax / rcx    48 F7 F9
/// ; result in rax (quotient), rdx (remainder)
/// ```
pub fn stencil_div() -> StencilDef {
    // cqo (sign-extend rax→rdx:rax), idiv rcx
    arith_stencil_abc(
        OpCode::Div as u8,
        "Div",
        &[
            0x48, 0x99, // cqo
            0x48, 0xF7, 0xF9, // idiv rcx
        ],
    )
}

/// **Mod** — `R[A] = R[B] % R[C]` (integer modulo, SMI fast path).
///
/// Uses `idiv` like Div; remainder is in `rdx`. Copy to `rax` after.
///
/// ```text
/// cqo           48 99
/// idiv rcx      48 F7 F9
/// mov rax, rdx  48 89 D0   ; remainder → rax
/// ```
pub fn stencil_mod() -> StencilDef {
    arith_stencil_abc(
        OpCode::Mod as u8,
        "Mod",
        &[
            0x48, 0x99, // cqo
            0x48, 0xF7, 0xF9, // idiv rcx
            0x48, 0x89, 0xD0, // mov rax, rdx (remainder)
        ],
    )
}

/// **Neg** — `R[A] = -R[B]` (negate, SMI fast path).
///
/// ```text
/// mov rax, [r14+B*8]   49 8B 86 <RegB:4>
/// ; tag check (19 bytes, jne hole at 3+4+16 = 23)
/// ; sign-extend (8 bytes)
/// neg rax              48 F7 D8
/// ; re-box (26 bytes)
/// mov [r14+A*8], rax   49 89 86 <RegA:4>
/// ```
pub fn stencil_neg() -> StencilDef {
    // Offsets:
    // 0:  load B (7 bytes), hole RegB at 3
    // 7:  tag_check_rax (19 bytes), jne hole at 7+16 = 23
    // 26: sign_extend_rax (8 bytes)
    // 34: neg rax (3 bytes)
    // 37: rebox_int_rax (26 bytes)
    // 63: store A (7 bytes), hole RegA at 63+3 = 66
    let mut code = code!(
        [0x49u8, 0x8B, 0x86],
        [0x00u8; 4], // mov rax, [r14+B*8]
    );
    code.extend_from_slice(&tag_check_rax());
    code.extend_from_slice(&sign_extend_rax());
    code.extend_from_slice(&[0x48, 0xF7, 0xD8]); // neg rax
    code.extend_from_slice(&rebox_int_rax());
    code.extend_from_slice(&[0x49, 0x89, 0x86]);
    code.extend_from_slice(&[0x00; 4]);

    StencilDef::new(
        OpCode::Neg as u8,
        "Neg",
        code,
        vec![
            HoleDef::new(3, HoleType::RegB, 4),
            HoleDef::new(23, HoleType::JumpOffset32, 4),
            HoleDef::new(66, HoleType::RegA, 4),
        ],
    )
}

// ---------------------------------------------------------------------------
// Comparison stencils
// ---------------------------------------------------------------------------
//
// Comparisons follow the Lumen LIR convention:
//   Eq A B C:  if (B == C) != (A != 0) then skip next instruction
//   Lt A B C:  if (B < C)  != (A != 0) then skip next
//   Le A B C:  if (B <= C) != (A != 0) then skip next
//
// For the Stitcher, comparisons are simplified: the "skip next" is replaced
// with a conditional `jmp` hole that jumps over the next stencil when the
// condition evaluates to `A == 0` (i.e., skip when condition is true and A=0,
// or when condition is false and A=1).
//
// In practice the stitcher will wire the `jne`/`je` holes to point to the
// correct next stencil address.
//
// For the prototype we emit the SMI fast path for integer comparisons.
// The slow path (type mismatch) is a JumpOffset32 hole.

/// Helper: emit int comparison between `rax` and `rcx`, store bool NbValue in `rax`.
///
/// `setcc_byte` is the single-byte condition code for `setXX al` (e.g. `0x94` for `sete`).
///
/// ```text
/// cmp rax, rcx         48 3B C1
/// setXX al             0F <setcc_byte> C0
/// movzx rax, al        48 0F B6 C0
/// ; rax = 0 or 1 (unboxed)
/// ; re-box as bool:
/// movabs rdx, FALSE_VALUE  48 BA <8 bytes>
/// ; true path:  rax=1 → rdx + 1 → TRUE_VALUE = FALSE_VALUE + 1
/// add rdx, rax         48 03 D0
/// mov rax, rdx         48 89 D0
/// ```
fn cmp_rax_rcx_to_bool_rax(setcc_byte: u8) -> Vec<u8> {
    code!(
        [0x48u8, 0x3B, 0xC1],       // cmp rax, rcx
        [0x0Fu8, setcc_byte, 0xC0], // setXX al
        [0x48u8, 0x0F, 0xB6, 0xC0], // movzx rax, al  (zero-extend to 64 bits)
        // Re-box: FALSE_VALUE + rax → bool NbValue
        // FALSE_VALUE=0x7FFB_0000_0000_0000, TRUE_VALUE=0x7FFB_0000_0000_0001
        [0x48u8, 0xBA], // movabs rdx, FALSE_VALUE
        FALSE_VALUE_LE,
        [0x48u8, 0x03, 0xD0], // add rdx, rax
        [0x48u8, 0x89, 0xD0], // mov rax, rdx
    )
}

/// Binary comparison stencil template (Eq, Lt, Le).
///
/// Stores the boolean result in `R[A]`.
fn cmp_stencil_abc(opcode: u8, name: &str, setcc_byte: u8) -> StencilDef {
    // 0:  load rax = R[B]  (7 bytes, hole RegB at 3)
    // 7:  load rcx = R[C]  (7 bytes, hole RegC at 10)
    // 14: tag_check_rax    (19 bytes, jne hole at 30)
    // 33: tag_check_rcx    (19 bytes, jne hole at 49)
    // 52: sign_extend_rax  (8 bytes)
    // 60: sign_extend_rcx  (8 bytes)
    // 68: cmp_to_bool      (3+3+4+2+8+3+3 = 26 bytes)
    // 94: store R[A]       (7 bytes, hole RegA at 94+3 = 97)

    let cmp_seq = cmp_rax_rcx_to_bool_rax(setcc_byte);
    let cmp_len = cmp_seq.len();
    let store_offset = 68 + cmp_len + 3;

    let mut code = code!(
        [0x49u8, 0x8B, 0x86],
        [0x00u8; 4],
        [0x49u8, 0x8B, 0x8E],
        [0x00u8; 4],
    );
    code.extend_from_slice(&tag_check_rax());
    code.extend_from_slice(&tag_check_rcx());
    code.extend_from_slice(&sign_extend_rax());
    code.extend_from_slice(&sign_extend_rcx());
    code.extend_from_slice(&cmp_seq);
    code.extend_from_slice(&[0x49, 0x89, 0x86]);
    code.extend_from_slice(&[0x00; 4]);

    StencilDef::new(
        opcode,
        name,
        code,
        vec![
            HoleDef::new(3, HoleType::RegB, 4),
            HoleDef::new(10, HoleType::RegC, 4),
            HoleDef::new(30, HoleType::JumpOffset32, 4),
            HoleDef::new(49, HoleType::JumpOffset32, 4),
            HoleDef::new(store_offset as u32, HoleType::RegA, 4),
        ],
    )
}

/// **Eq** — `R[A] = (R[B] == R[C])` as NbValue bool.
pub fn stencil_eq() -> StencilDef {
    cmp_stencil_abc(OpCode::Eq as u8, "Eq", 0x94) // sete
}

/// **Lt** — `R[A] = (R[B] < R[C])` as NbValue bool.
pub fn stencil_lt() -> StencilDef {
    cmp_stencil_abc(OpCode::Lt as u8, "Lt", 0x9C) // setl
}

/// **Le** — `R[A] = (R[B] <= R[C])` as NbValue bool.
pub fn stencil_le() -> StencilDef {
    cmp_stencil_abc(OpCode::Le as u8, "Le", 0x9E) // setle
}

/// **Gt** — `R[A] = (R[B] > R[C])` as NbValue bool.
///
/// Note: the LIR has no dedicated Gt opcode; the compiler lowers `>` to
/// `OpCode::Lt` with operands B and C swapped.  This stencil is therefore
/// not registered in the library and exists only for reference / future use.
/// Raw opcode 0x3A is reserved for a potential future Gt instruction.
#[allow(dead_code)]
pub fn stencil_gt() -> StencilDef {
    // We load rax=R[B], rcx=R[C], then cmp uses setg.
    cmp_stencil_abc(0x3A, "Gt", 0x9F) // setg — raw byte; no OpCode::Gt variant exists
}

/// **Ge** — `R[A] = (R[B] >= R[C])` as NbValue bool.
///
/// Note: the LIR has no dedicated Ge opcode; the compiler lowers `>=` to
/// `OpCode::Le` with operands B and C swapped.  This stencil is therefore
/// not registered in the library and exists only for reference / future use.
/// Raw opcode 0x3B is reserved for a potential future Ge instruction.
#[allow(dead_code)]
pub fn stencil_ge() -> StencilDef {
    cmp_stencil_abc(0x3B, "Ge", 0x9D) // setge — raw byte; no OpCode::Ge variant exists
}

/// **Not** — `R[A] = !R[B]` (logical not of bool NbValue).
///
/// ```text
/// mov rax, [r14+B*8]       ; load R[B]
/// ; rax is FALSE_VALUE (0x7FFB_0000_0000_0000) or TRUE_VALUE (0x7FFB_0000_0000_0001)
/// ; Flip the low bit: rax ^= 1  →  xor rax, 1
/// xor rax, 1               ; 48 83 F0 01
/// ; But we must mask so we don't corrupt higher bits if not bool.
/// ; Simple: assume bool input (the compiler ensures this).
/// mov [r14+A*8], rax       ; store R[A]
/// ```
///
/// *Precondition*: `R[B]` is a NbValue bool.  The low bit is 1 for true, 0 for false.
/// XOR with 1 flips it, toggling between FALSE/TRUE while preserving the tag bits.
pub fn stencil_not() -> StencilDef {
    StencilDef::new(
        OpCode::Not as u8,
        "Not",
        code!(
            [0x49u8, 0x8B, 0x86],
            [0x00u8; 4],                // mov rax, [r14+B*8]  hole RegB at 3
            [0x48u8, 0x83, 0xF0, 0x01], // xor rax, 1
            [0x49u8, 0x89, 0x86],
            [0x00u8; 4], // mov [r14+A*8], rax  hole RegA at 14
        ),
        vec![
            HoleDef::new(3, HoleType::RegB, 4),
            HoleDef::new(14, HoleType::RegA, 4),
        ],
    )
}

// ---------------------------------------------------------------------------
// Control flow stencils
// ---------------------------------------------------------------------------

/// **Jmp** — unconditional jump by signed offset.
///
/// The target address is computed by the stitcher from the LIR `sAx` field
/// and patched into a 32-bit PC-relative jump.
///
/// ```text
/// jmp rel32    →  E9 [hole: JumpOffset32, 4]
/// ```
pub fn stencil_jmp() -> StencilDef {
    StencilDef::new(
        OpCode::Jmp as u8,
        "Jmp",
        code!([0xE9u8], [0x00u8; 4]),
        vec![HoleDef::new(1, HoleType::JumpOffset32, 4)],
    )
}

/// **Break** — same as Jmp but for loop break.
pub fn stencil_break() -> StencilDef {
    let mut s = stencil_jmp();
    s.opcode = OpCode::Break as u8;
    s.name = "Break".into();
    s
}

/// **Continue** — same as Jmp but for loop continue.
pub fn stencil_continue() -> StencilDef {
    let mut s = stencil_jmp();
    s.opcode = OpCode::Continue as u8;
    s.name = "Continue".into();
    s
}

/// **Test** — conditional skip: if `bool(R[A]) != (C != 0)` then skip next.
///
/// Implemented as:
/// - Load `R[A]` into `rax`.
/// - Extract the low bit (bool payload): `and rax, 1`.
/// - Compare with field C (baked in as an immediate at stitch time).
/// - If not equal, jump over the next stencil (JumpOffset32 hole).
///
/// The "skip" semantics require the stitcher to set the jump target to
/// point past the immediately following stencil.
///
/// ```text
/// mov rax, [r14+A*8]    49 8B 86 <RegA:4>
/// and eax, 1            83 E0 01
/// cmp eax, <C:1>        83 F8 <RegCIndex:1>
/// jne rel32             0F 85 <JumpOffset32:4>
/// ```
///
/// Holes:
/// - `RegA` at offset 3 (4 bytes)
/// - `RegCIndex` at offset 12 (1 byte) — raw value of field C (0 or 1)
/// - `JumpOffset32` at offset 15 (4 bytes)
pub fn stencil_test() -> StencilDef {
    StencilDef::new(
        OpCode::Test as u8,
        "Test",
        code!(
            [0x49u8, 0x8B, 0x86],
            [0x00u8; 4],                            // mov rax, [r14+A*8]  hole RegA at 3
            [0x83u8, 0xE0, 0x01],                   // and eax, 1
            [0x83u8, 0xF8, 0x00], // cmp eax, <imm8>     hole RegCIndex at 12 (1 byte)
            [0x0Fu8, 0x85, 0x00, 0x00, 0x00, 0x00], // jne rel32        hole JumpOffset32 at 15
        ),
        vec![
            HoleDef::new(3, HoleType::RegA, 4),
            HoleDef::new(12, HoleType::RegCIndex, 1),
            HoleDef::new(15, HoleType::JumpOffset32, 4),
        ],
    )
}

/// **Return** — return from cell.
///
/// Calls the runtime return handler which restores the caller frame and
/// transfers control back.  The return-value register index is patched in.
///
/// ```text
/// ; Set up call to lm_rt_return(ctx, ret_reg_idx)
/// ; rdi = r15 (VmContext*)
/// ; rsi = A (return register index)
/// mov rdi, r15              4C 89 FF    (actually 49 89 FF for r15 → rdi via REX)
/// movzx esi, <A:1>          0F B6 35 <RegAIndex:1>  — wrong encoding, use simpler:
/// ; simpler: encode A as imm8
/// mov esi, <A:1>            BE <A:1> 00 00 00  (mov esi, imm32, with A baked as byte)
/// movabs rax, <lm_rt_return>  48 B8 <RuntimeFuncAddr:8>
/// call rax                  FF D0
/// ```
///
/// For the stitcher prototype, Return is a runtime call via trampoline.
/// Holes: `RegAIndex` (1 byte) for the return register, `RuntimeFuncAddr`
/// for `lm_rt_return`.
///
/// Encoding:
/// ```text
/// 49 89 FE                       ; mov rsi, r15  ... no: mov r15 to rdi
/// 4C 89 FF                       ; mov rdi, r15  (r15 has REX.B bit)
/// B8 <A:1> 00 00 00              ; mov eax, A   (esi would be: BE <A>...)
///                                ; but want esi for arg2:
/// BE 00 00 00 00                 ; mov esi, imm32 (A baked as 32-bit)
/// 48 B8 <RuntimeFuncAddr:8>      ; movabs rax, func
/// FF D0                          ; call rax
/// ```
pub fn stencil_return() -> StencilDef {
    // Offsets:
    // 0: 4C 89 FF         mov rdi, r15    (3 bytes)
    // 3: BE 00 00 00 00   mov esi, <A>    (5 bytes, hole RegAIndex at 4, size 1)
    //    Actually for a 32-bit imm we write the A value as imm32 but it's just a u8.
    //    Hole: RegAIndex at 4 (1 byte), and the remaining 3 bytes of the imm32 stay 0.
    // 8: 48 B8 <8>        movabs rax, func (10 bytes, hole RuntimeFuncAddr at 10)
    // 18: FF D0            call rax        (2 bytes)
    StencilDef::new(
        OpCode::Return as u8,
        "Return",
        code!(
            [0x4Cu8, 0x89, 0xFF],             // mov rdi, r15
            [0xBEu8, 0x00, 0x00, 0x00, 0x00], // mov esi, <A> (imm32, low byte = A)
            [0x48u8, 0xB8],
            [0x00u8; 8],                // movabs rax, <lm_rt_return>
            [0x48u8, 0x83, 0xEC, 0x08], // sub rsp, 8  (align RSP for call)
            [0xFFu8, 0xD0],             // call rax
            [0x48u8, 0x83, 0xC4, 0x08], // add rsp, 8  (restore RSP)
        ),
        vec![
            HoleDef::new(4, HoleType::RegAIndex, 1),
            HoleDef::new(10, HoleType::RuntimeFuncAddr, 8),
        ],
    )
}

/// **Halt** — stop execution with error.
///
/// Calls `lm_rt_halt(ctx, error_reg_idx)`.
pub fn stencil_halt() -> StencilDef {
    StencilDef::new(
        OpCode::Halt as u8,
        "Halt",
        code!(
            [0x4Cu8, 0x89, 0xFF],             // mov rdi, r15
            [0xBEu8, 0x00, 0x00, 0x00, 0x00], // mov esi, <A>
            [0x48u8, 0xB8],
            [0x00u8; 8],                // movabs rax, <lm_rt_halt>
            [0x48u8, 0x83, 0xEC, 0x08], // sub rsp, 8  (align RSP)
            [0xFFu8, 0xD0],             // call rax
            [0x48u8, 0x83, 0xC4, 0x08], // add rsp, 8  (restore RSP)
        ),
        vec![
            HoleDef::new(4, HoleType::RegAIndex, 1),
            HoleDef::new(10, HoleType::RuntimeFuncAddr, 8),
        ],
    )
}

// ---------------------------------------------------------------------------
// Call stencil
// ---------------------------------------------------------------------------

/// **Call** — call closure/cell `R[A]` with `B` args starting at `R[A+1]`.
///
/// Dispatches to `lm_rt_call(ctx, instr_word)` which handles frame setup,
/// argument passing, and dispatch into the callee's stitched or interpreted code.
///
/// ```text
/// 4C 89 FF                    mov rdi, r15  (VmContext*)
/// 48 B8 <instr:8>             movabs rax, <instr_word:8>    hole: InstructionWord
/// 48 89 C6                    mov rsi, rax  (instr as u64)
/// 48 B8 <addr:8>              movabs rax, <lm_rt_call:8>    hole: RuntimeFuncAddr
/// FF D0                       call rax
/// ```
pub fn stencil_call() -> StencilDef {
    // Offsets:
    // 0:  4C 89 FF              mov rdi, r15       (3 bytes)
    // 3:  48 B8 <8>             movabs rax, instr  (10 bytes, hole InstructionWord at 5)
    // 13: 48 89 C6              mov rsi, rax       (3 bytes)
    // 16: 48 B8 <8>             movabs rax, func   (10 bytes, hole RuntimeFuncAddr at 18)
    // 26: 48 83 EC 08           sub rsp, 8         (4 bytes, align RSP)
    // 30: FF D0                 call rax           (2 bytes)
    // 32: 48 83 C4 08           add rsp, 8         (4 bytes, restore RSP)
    StencilDef::new(
        OpCode::Call as u8,
        "Call",
        code!(
            [0x4Cu8, 0x89, 0xFF], // mov rdi, r15
            [0x48u8, 0xB8],
            [0x00u8; 8],          // movabs rax, <instr_word>  hole at 5
            [0x48u8, 0x89, 0xC6], // mov rsi, rax
            [0x48u8, 0xB8],
            [0x00u8; 8],                // movabs rax, <lm_rt_call>  hole at 18
            [0x48u8, 0x83, 0xEC, 0x08], // sub rsp, 8
            [0xFFu8, 0xD0],             // call rax
            [0x48u8, 0x83, 0xC4, 0x08], // add rsp, 8
        ),
        vec![
            HoleDef::new(5, HoleType::InstructionWord, 8),
            HoleDef::new(18, HoleType::RuntimeFuncAddr, 8),
        ],
    )
}

/// **TailCall** — same shape as Call, different runtime function.
pub fn stencil_tailcall() -> StencilDef {
    let mut s = stencil_call();
    s.opcode = OpCode::TailCall as u8;
    s.name = "TailCall".into();
    s
}

// ---------------------------------------------------------------------------
// Intrinsic stencil
// ---------------------------------------------------------------------------

/// **Intrinsic** — dispatch to a built-in function.
///
/// `lm_rt_intrinsic(ctx, instr_word)` looks up the intrinsic ID from
/// the instruction and dispatches.
pub fn stencil_intrinsic() -> StencilDef {
    StencilDef::new(
        OpCode::Intrinsic as u8,
        "Intrinsic",
        code!(
            [0x4Cu8, 0x89, 0xFF], // mov rdi, r15
            [0x48u8, 0xB8],
            [0x00u8; 8],          // movabs rax, <instr_word>  hole@5
            [0x48u8, 0x89, 0xC6], // mov rsi, rax
            [0x48u8, 0xB8],
            [0x00u8; 8],                // movabs rax, <lm_rt_intrinsic> hole@18
            [0x48u8, 0x83, 0xEC, 0x08], // sub rsp, 8  (align RSP)
            [0xFFu8, 0xD0],             // call rax
            [0x48u8, 0x83, 0xC4, 0x08], // add rsp, 8  (restore RSP)
        ),
        vec![
            HoleDef::new(5, HoleType::InstructionWord, 8),
            HoleDef::new(18, HoleType::RuntimeFuncAddr, 8),
        ],
    )
}

/// **IntrinsicAppend** — `R[A] = append(R[C], R[C+1])` fast path.
///
/// Calls `jit_rt_list_append(ctx, list, elem)` and stores the returned list
/// pointer back into R[A].
pub fn stencil_intrinsic_append() -> StencilDef {
    StencilDef::new(
        OpCode::Intrinsic as u8,
        "IntrinsicAppend",
        code!(
            [0x4Cu8, 0x89, 0xFF], // mov rdi, r15
            [0x49u8, 0x8B, 0x86],
            [0x00u8; 4],          // mov rax, [r14+C*8]  hole RegC at 6
            [0x48u8, 0x89, 0xC6], // mov rsi, rax
            [0x49u8, 0x8B, 0x86],
            [0x00u8; 4],          // mov rax, [r14+B*8]  hole RegB at 17 (patched to C+1)
            [0x48u8, 0x89, 0xC2], // mov rdx, rax
            [0x48u8, 0xB8],
            [0x00u8; 8],                // movabs rax, <jit_rt_list_append>
            [0x48u8, 0x83, 0xEC, 0x08], // sub rsp, 8
            [0xFFu8, 0xD0],             // call rax
            [0x48u8, 0x83, 0xC4, 0x08], // add rsp, 8
            [0x49u8, 0x89, 0x86],
            [0x00u8; 4], // mov [r14+A*8], rax  hole RegA at 46
        ),
        vec![
            HoleDef::new(6, HoleType::RegC, 4),
            HoleDef::new(16, HoleType::RegB, 4),
            HoleDef::new(25, HoleType::RuntimeFuncAddr, 8),
            HoleDef::new(46, HoleType::RegA, 4),
        ],
    )
}

/// **IntrinsicRange** — `R[A] = range(R[C], R[C+1])` fast path.
///
/// Calls `jit_rt_range(ctx, start, end)` and stores the returned list pointer.
pub fn stencil_intrinsic_range() -> StencilDef {
    StencilDef::new(
        OpCode::Intrinsic as u8,
        "IntrinsicRange",
        code!(
            [0x4Cu8, 0x89, 0xFF], // mov rdi, r15
            [0x49u8, 0x8B, 0x86],
            [0x00u8; 4],          // mov rax, [r14+C*8]  hole RegC at 6
            [0x48u8, 0x89, 0xC6], // mov rsi, rax
            [0x49u8, 0x8B, 0x86],
            [0x00u8; 4],          // mov rax, [r14+B*8]  hole RegB at 17 (patched to C+1)
            [0x48u8, 0x89, 0xC2], // mov rdx, rax
            [0x48u8, 0xB8],
            [0x00u8; 8],                // movabs rax, <jit_rt_range>
            [0x48u8, 0x83, 0xEC, 0x08], // sub rsp, 8
            [0xFFu8, 0xD0],             // call rax
            [0x48u8, 0x83, 0xC4, 0x08], // add rsp, 8
            [0x49u8, 0x89, 0x86],
            [0x00u8; 4], // mov [r14+A*8], rax  hole RegA at 46
        ),
        vec![
            HoleDef::new(6, HoleType::RegC, 4),
            HoleDef::new(16, HoleType::RegB, 4),
            HoleDef::new(25, HoleType::RuntimeFuncAddr, 8),
            HoleDef::new(46, HoleType::RegA, 4),
        ],
    )
}

/// **IntrinsicSort** — `R[A] = sort(R[C])` fast path.
///
/// Calls `jit_rt_sort(ctx, list)` and stores the returned list pointer.
pub fn stencil_intrinsic_sort() -> StencilDef {
    StencilDef::new(
        OpCode::Intrinsic as u8,
        "IntrinsicSort",
        code!(
            [0x4Cu8, 0x89, 0xFF], // mov rdi, r15
            [0x49u8, 0x8B, 0x86],
            [0x00u8; 4],          // mov rax, [r14+C*8]  hole RegC at 6
            [0x48u8, 0x89, 0xC6], // mov rsi, rax
            [0x48u8, 0xB8],
            [0x00u8; 8],                // movabs rax, <jit_rt_sort>
            [0x48u8, 0x83, 0xEC, 0x08], // sub rsp, 8
            [0xFFu8, 0xD0],             // call rax
            [0x48u8, 0x83, 0xC4, 0x08], // add rsp, 8
            [0x49u8, 0x89, 0x86],
            [0x00u8; 4], // mov [r14+A*8], rax  hole RegA at 34
        ),
        vec![
            HoleDef::new(6, HoleType::RegC, 4),
            HoleDef::new(15, HoleType::RuntimeFuncAddr, 8),
            HoleDef::new(36, HoleType::RegA, 4),
        ],
    )
}

/// **IntrinsicLength** — `R[A] = length(R[C])` fast path.
///
/// Calls `jit_rt_collection_len(ctx, value)` and boxes the result as TAG_INT.
pub fn stencil_intrinsic_length() -> StencilDef {
    StencilDef::new(
        OpCode::Intrinsic as u8,
        "IntrinsicLength",
        code!(
            [0x4Cu8, 0x89, 0xFF], // mov rdi, r15
            [0x49u8, 0x8B, 0x86],
            [0x00u8; 4],          // mov rax, [r14+C*8]  hole RegC at 6
            [0x48u8, 0x89, 0xC6], // mov rsi, rax
            [0x48u8, 0xB8],
            [0x00u8; 8],                // movabs rax, <jit_rt_collection_len>
            [0x48u8, 0x83, 0xEC, 0x08], // sub rsp, 8
            [0xFFu8, 0xD0],             // call rax
            [0x48u8, 0x83, 0xC4, 0x08], // add rsp, 8
            // Box raw i64 in rax as TAG_INT
            [0x48u8, 0xBA], // movabs rdx, PAYLOAD_MASK
            PAYLOAD_MASK_LE,
            [0x48u8, 0x23, 0xC2], // and rax, rdx
            [0x48u8, 0xBA],       // movabs rdx, TAG_INT_BASE
            TAG_INT_BASE_LE,
            [0x48u8, 0x0B, 0xC2], // or rax, rdx
            [0x49u8, 0x89, 0x86],
            [0x00u8; 4], // mov [r14+A*8], rax  hole RegA at 62
        ),
        vec![
            HoleDef::new(6, HoleType::RegC, 4),
            HoleDef::new(15, HoleType::RuntimeFuncAddr, 8),
            HoleDef::new(62, HoleType::RegA, 4),
        ],
    )
}

/// **IntrinsicKeys** — `R[A] = keys(R[C])` fast path.
pub fn stencil_intrinsic_keys() -> StencilDef {
    StencilDef::new(
        OpCode::Intrinsic as u8,
        "IntrinsicKeys",
        code!(
            [0x4Cu8, 0x89, 0xFF], // mov rdi, r15
            [0x49u8, 0x8B, 0x86],
            [0x00u8; 4],          // mov rax, [r14+C*8]  hole RegC at 6
            [0x48u8, 0x89, 0xC6], // mov rsi, rax
            [0x48u8, 0xB8],
            [0x00u8; 8],                // movabs rax, <jit_rt_map_keys>
            [0x48u8, 0x83, 0xEC, 0x08], // sub rsp, 8
            [0xFFu8, 0xD0],             // call rax
            [0x48u8, 0x83, 0xC4, 0x08], // add rsp, 8
            [0x49u8, 0x89, 0x86],
            [0x00u8; 4], // mov [r14+A*8], rax  hole RegA at 36
        ),
        vec![
            HoleDef::new(6, HoleType::RegC, 4),
            HoleDef::new(15, HoleType::RuntimeFuncAddr, 8),
            HoleDef::new(36, HoleType::RegA, 4),
        ],
    )
}

/// **IntrinsicValues** — `R[A] = values(R[C])` fast path.
pub fn stencil_intrinsic_values() -> StencilDef {
    StencilDef::new(
        OpCode::Intrinsic as u8,
        "IntrinsicValues",
        code!(
            [0x4Cu8, 0x89, 0xFF], // mov rdi, r15
            [0x49u8, 0x8B, 0x86],
            [0x00u8; 4],          // mov rax, [r14+C*8]  hole RegC at 6
            [0x48u8, 0x89, 0xC6], // mov rsi, rax
            [0x48u8, 0xB8],
            [0x00u8; 8],                // movabs rax, <jit_rt_map_values>
            [0x48u8, 0x83, 0xEC, 0x08], // sub rsp, 8
            [0xFFu8, 0xD0],             // call rax
            [0x48u8, 0x83, 0xC4, 0x08], // add rsp, 8
            [0x49u8, 0x89, 0x86],
            [0x00u8; 4], // mov [r14+A*8], rax  hole RegA at 36
        ),
        vec![
            HoleDef::new(6, HoleType::RegC, 4),
            HoleDef::new(15, HoleType::RuntimeFuncAddr, 8),
            HoleDef::new(36, HoleType::RegA, 4),
        ],
    )
}

// ---------------------------------------------------------------------------
// Effect system stencils
// ---------------------------------------------------------------------------

/// Runtime-call stencil template for effect opcodes.
///
/// All effect opcodes (Perform, HandlePush, HandlePop, Resume) call a
/// corresponding `lm_rt_*` function with `(ctx, instr_word)`.
fn effect_stencil(opcode: u8, name: &str) -> StencilDef {
    // Layout (x86-64, System V ABI):
    //  0:  4C 89 FF              mov rdi, r15         (ctx → arg1)
    //  3:  48 B8 [8]             movabs rax, instr    (hole InstructionWord at 5)
    // 13:  48 89 C6              mov rsi, rax         (instr_word → arg2)
    // 16:  48 B8 [8]             movabs rax, func     (hole RuntimeFuncAddr at 18)
    // 26:  48 83 EC 08           sub rsp, 8           (align RSP to 16 for call)
    // 30:  FF D0                 call rax
    // 32:  48 83 C4 08           add rsp, 8           (restore RSP after call)
    //
    // The stencil is entered with RSP ≡ 8 (mod 16) (post-call ABI convention).
    // We subtract 8 before `call rax` so the runtime helper is entered with
    // RSP ≡ 8-8 = 0 before call → callee entry RSP ≡ 8 (ABI-correct) and
    // Rust's codegen can generate movaps without misalignment issues.
    StencilDef::new(
        opcode,
        name,
        code!(
            [0x4Cu8, 0x89, 0xFF], // mov rdi, r15
            [0x48u8, 0xB8],
            [0x00u8; 8],          // movabs rax, <instr_word>  hole@5
            [0x48u8, 0x89, 0xC6], // mov rsi, rax
            [0x48u8, 0xB8],
            [0x00u8; 8],                // movabs rax, <runtime_func> hole@18
            [0x48u8, 0x83, 0xEC, 0x08], // sub rsp, 8  (align)
            [0xFFu8, 0xD0],             // call rax
            [0x48u8, 0x83, 0xC4, 0x08], // add rsp, 8  (restore)
        ),
        vec![
            HoleDef::new(5, HoleType::InstructionWord, 8),
            HoleDef::new(18, HoleType::RuntimeFuncAddr, 8),
        ],
    )
}

/// **Perform** — invoke an effect operation.
pub fn stencil_perform() -> StencilDef {
    effect_stencil(OpCode::Perform as u8, "Perform")
}

/// **HandlePush** — push an effect handler onto the handler stack.
pub fn stencil_handle_push() -> StencilDef {
    effect_stencil(OpCode::HandlePush as u8, "HandlePush")
}

/// **HandlePop** — pop the innermost effect handler.
pub fn stencil_handle_pop() -> StencilDef {
    effect_stencil(OpCode::HandlePop as u8, "HandlePop")
}

/// **Resume** — resume a suspended continuation with a value.
pub fn stencil_resume() -> StencilDef {
    effect_stencil(OpCode::Resume as u8, "Resume")
}

// ---------------------------------------------------------------------------
// Collection construction stencils
// ---------------------------------------------------------------------------

/// **NewList** — create a list from B values at registers A+1..A+B.
pub fn stencil_new_list() -> StencilDef {
    effect_stencil(OpCode::NewList as u8, "NewList")
}

/// **NewListStack** — create a list from B values at registers A+1..A+B.
pub fn stencil_new_list_stack() -> StencilDef {
    effect_stencil(OpCode::NewListStack as u8, "NewListStack")
}

/// **NewRecord** — create a record of type Bx.
pub fn stencil_new_record() -> StencilDef {
    effect_stencil(OpCode::NewRecord as u8, "NewRecord")
}

/// **NewMap** — create a map from B kv pairs.
pub fn stencil_new_map() -> StencilDef {
    effect_stencil(OpCode::NewMap as u8, "NewMap")
}

/// **NewTuple** — create a tuple from B values.
pub fn stencil_new_tuple() -> StencilDef {
    effect_stencil(OpCode::NewTuple as u8, "NewTuple")
}

/// **NewTupleStack** — create a tuple from B values at registers A+1..A+B.
pub fn stencil_new_tuple_stack() -> StencilDef {
    effect_stencil(OpCode::NewTupleStack as u8, "NewTupleStack")
}

/// **NewSet** — create a set from B values.
pub fn stencil_new_set() -> StencilDef {
    effect_stencil(OpCode::NewSet as u8, "NewSet")
}

// ---------------------------------------------------------------------------
// Field/index access stencils
// ---------------------------------------------------------------------------

/// **GetField** — `R[A] = R[B].field[C]`.
///
/// Field access requires runtime dispatch (record field lookup by index).
pub fn stencil_get_field() -> StencilDef {
    effect_stencil(OpCode::GetField as u8, "GetField")
}

/// **SetField** — `R[A].field[B] = R[C]`.
pub fn stencil_set_field() -> StencilDef {
    effect_stencil(OpCode::SetField as u8, "SetField")
}

/// **GetIndex** — `R[A] = R[B][R[C]]` (list/map index).
pub fn stencil_get_index() -> StencilDef {
    effect_stencil(OpCode::GetIndex as u8, "GetIndex")
}

/// **SetIndex** — `R[A][R[B]] = R[C]`.
pub fn stencil_set_index() -> StencilDef {
    effect_stencil(OpCode::SetIndex as u8, "SetIndex")
}

// ---------------------------------------------------------------------------
// Bitwise stencils (inline fast paths — Pattern A)
// ---------------------------------------------------------------------------
//
// Bitwise ops work directly on the 48-bit integer payload stored in NbValue.
// The tag for TAG_INT is 0x7FF9 in the top 16 bits.  We reuse arith_stencil_abc
// which handles tag checks + payload extraction + re-boxing.
//
// For bitwise ops, after sign-extension we apply the bitwise operation
// directly on the signed i64 value. The result is re-boxed as TAG_INT.

/// **BitOr** — `R[A] = R[B] | R[C]` (bitwise OR of integers).
///
/// ```text
/// or rax, rcx   48 0B C1
/// ```
pub fn stencil_bitor() -> StencilDef {
    arith_stencil_abc(OpCode::BitOr as u8, "BitOr", &[0x48, 0x0B, 0xC1])
}

/// **BitAnd** — `R[A] = R[B] & R[C]` (bitwise AND of integers).
///
/// ```text
/// and rax, rcx   48 23 C1
/// ```
pub fn stencil_bitand() -> StencilDef {
    arith_stencil_abc(OpCode::BitAnd as u8, "BitAnd", &[0x48, 0x23, 0xC1])
}

/// **BitXor** — `R[A] = R[B] ^ R[C]` (bitwise XOR of integers).
///
/// ```text
/// xor rax, rcx   48 33 C1
/// ```
pub fn stencil_bitxor() -> StencilDef {
    arith_stencil_abc(OpCode::BitXor as u8, "BitXor", &[0x48, 0x33, 0xC1])
}

/// **BitNot** — `R[A] = ~R[B]` (bitwise NOT of integer).
///
/// Uses same single-operand structure as Neg.
///
/// ```text
/// not rax   48 F7 D0
/// ```
pub fn stencil_bitnot() -> StencilDef {
    // Same layout as stencil_neg() but with `not rax` instead of `neg rax`
    let mut code = code!(
        [0x49u8, 0x8B, 0x86],
        [0x00u8; 4], // mov rax, [r14+B*8]
    );
    code.extend_from_slice(&tag_check_rax());
    code.extend_from_slice(&sign_extend_rax());
    code.extend_from_slice(&[0x48, 0xF7, 0xD0]); // not rax
    code.extend_from_slice(&rebox_int_rax());
    code.extend_from_slice(&[0x49, 0x89, 0x86]);
    code.extend_from_slice(&[0x00; 4]);

    StencilDef::new(
        OpCode::BitNot as u8,
        "BitNot",
        code,
        vec![
            HoleDef::new(3, HoleType::RegB, 4),
            HoleDef::new(23, HoleType::JumpOffset32, 4),
            HoleDef::new(66, HoleType::RegA, 4),
        ],
    )
}

/// **Shl** — `R[A] = R[B] << R[C]` (left shift).
///
/// x86-64 `shl rax, cl` performs `rax << (cl & 63)`.
/// We extract the shift amount from the rcx payload into cl before the shift.
///
/// ```text
/// mov rcx, rcx (already have rcx as shift count)
/// ; rcx holds sign-extended i64 shift amount; need count in cl
/// ; shl rax, cl  → 48 D3 E0
/// ```
pub fn stencil_shl() -> StencilDef {
    // After arith_stencil_abc's sign-extend+compute phase:
    // rax = lhs (sign-extended), rcx = rhs (sign-extended)
    // shl rax, cl  = 48 D3 E0
    arith_stencil_abc(OpCode::Shl as u8, "Shl", &[0x48, 0xD3, 0xE0])
}

/// **Shr** — `R[A] = R[B] >> R[C]` (arithmetic right shift).
///
/// ```text
/// sar rax, cl   48 D3 F8
/// ```
pub fn stencil_shr() -> StencilDef {
    arith_stencil_abc(OpCode::Shr as u8, "Shr", &[0x48, 0xD3, 0xF8])
}

/// **FloorDiv** — `R[A] = R[B] // R[C]` (floor division).
///
/// Floor division differs from truncating division for negative operands.
/// We use idiv (truncating), then correct: if remainder != 0 and signs differ,
/// subtract 1 from quotient.
///
/// ```text
/// cqo               48 99
/// idiv rcx          48 F7 F9
/// ; rax = quotient, rdx = remainder
/// ; floor correction: if rdx != 0 and sign(rax) != sign(rdx), rax -= 1
/// test rdx, rdx     48 85 D2
/// je .done          74 08   (short jump 8 bytes forward — skip correction)
/// xor rdx, rax      48 31 C2   (rdx ^= rax)
/// js .sub_one       78 02   (if sign bit of (remainder^quotient) is set, correct)
/// jmp .done         EB 02
/// .sub_one:
/// sub rax, 1        48 FF C8  (dec rax)
/// .done: (re-box happens after)
/// ```
///
/// Total compute bytes: 2+3+2+3+2+2+3 = 17 bytes
pub fn stencil_floordiv() -> StencilDef {
    let compute: &[u8] = &[
        0x48, 0x99, // cqo
        0x48, 0xF7, 0xF9, // idiv rcx
        // floor correction:
        0x48, 0x85, 0xD2, // test rdx, rdx
        0x74, 0x08, // je .done (short jump, 8 bytes)
        0x48, 0x31, 0xC2, // xor rdx, rax
        0x78, 0x02, // js .sub_one (2 bytes)
        0xEB, 0x03, // jmp .done
        // .sub_one:
        0x48, 0xFF, 0xC8, // dec rax  (sub rax, 1)
              // .done:
    ];
    arith_stencil_abc(OpCode::FloorDiv as u8, "FloorDiv", compute)
}

// ---------------------------------------------------------------------------
// Logical stencils (Pattern A — inline bool operations)
// ---------------------------------------------------------------------------
//
// NbValue bool layout:
//   FALSE_VALUE = 0x7FFB_0000_0000_0000  (low bit = 0)
//   TRUE_VALUE  = 0x7FFB_0000_0000_0001  (low bit = 1)
//
// For And/Or we just operate on the low bits.

/// **And** — `R[A] = R[B] and R[C]` (logical AND of booleans).
///
/// Both booleans have the same high bits; AND-ing the full words keeps tag,
/// and the result low bit is correct: (low_B & low_C).
///
/// ```text
/// mov rax, [r14+B*8]   load B (full NbValue bool)
/// mov rcx, [r14+C*8]   load C
/// and rax, rcx          AND — if both true (low bits both 1), result = TRUE_VALUE
/// mov [r14+A*8], rax   store
/// ```
///
/// Precondition: both operands are NbValue bools (tag 0x7FFB).
/// The high tag bits are identical for TRUE/FALSE so AND is correct.
pub fn stencil_and() -> StencilDef {
    StencilDef::new(
        OpCode::And as u8,
        "And",
        code!(
            [0x49u8, 0x8B, 0x86],
            [0x00u8; 4], // mov rax, [r14+B*8]  hole RegB at 3
            [0x49u8, 0x8B, 0x8E],
            [0x00u8; 4],          // mov rcx, [r14+C*8]  hole RegC at 10
            [0x48u8, 0x23, 0xC1], // and rax, rcx
            [0x49u8, 0x89, 0x86],
            [0x00u8; 4], // mov [r14+A*8], rax  hole RegA at 17
        ),
        vec![
            HoleDef::new(3, HoleType::RegB, 4),
            HoleDef::new(10, HoleType::RegC, 4),
            HoleDef::new(17, HoleType::RegA, 4),
        ],
    )
}

/// **Or** — `R[A] = R[B] or R[C]` (logical OR of booleans).
///
/// ```text
/// mov rax, [r14+B*8]
/// mov rcx, [r14+C*8]
/// or  rax, rcx          OR — if either true (low bit 1), result = TRUE_VALUE
/// mov [r14+A*8], rax
/// ```
pub fn stencil_or() -> StencilDef {
    StencilDef::new(
        OpCode::Or as u8,
        "Or",
        code!(
            [0x49u8, 0x8B, 0x86],
            [0x00u8; 4], // mov rax, [r14+B*8]  hole RegB at 3
            [0x49u8, 0x8B, 0x8E],
            [0x00u8; 4],          // mov rcx, [r14+C*8]  hole RegC at 10
            [0x48u8, 0x0B, 0xC1], // or rax, rcx
            [0x49u8, 0x89, 0x86],
            [0x00u8; 4], // mov [r14+A*8], rax  hole RegA at 17
        ),
        vec![
            HoleDef::new(3, HoleType::RegB, 4),
            HoleDef::new(10, HoleType::RegC, 4),
            HoleDef::new(17, HoleType::RegA, 4),
        ],
    )
}

/// **NullCo** — `R[A] = R[B] ?? R[C]` (null coalescing).
///
/// If R[B] is not null, A = B; otherwise A = C.
/// NULL_VALUE = 0x7FFC_0000_0000_0000.
///
/// ```text
/// mov rax, [r14+B*8]              ; load B
/// mov rcx, [r14+C*8]              ; load C
/// movabs rdx, 0x7FFC_0000_0000_0000  ; NULL_VALUE
/// cmp rax, rdx                    ; is B null?
/// cmove rax, rcx                  ; if B==null, rax = rcx  (48 0F 44 C1)
/// mov [r14+A*8], rax              ; store result
/// ```
pub fn stencil_nullco() -> StencilDef {
    StencilDef::new(
        OpCode::NullCo as u8,
        "NullCo",
        code!(
            [0x49u8, 0x8B, 0x86],
            [0x00u8; 4], // mov rax, [r14+B*8]   hole RegB at 3
            [0x49u8, 0x8B, 0x8E],
            [0x00u8; 4],                // mov rcx, [r14+C*8]   hole RegC at 10
            [0x48u8, 0xBA],             // movabs rdx, NULL_VALUE
            NULL_VALUE_LE,              // 8 bytes: 0x7FFC_0000_0000_0000
            [0x48u8, 0x3B, 0xC2],       // cmp rax, rdx
            [0x48u8, 0x0F, 0x44, 0xC1], // cmove rax, rcx  (conditional move if equal/zero)
            [0x49u8, 0x89, 0x86],
            [0x00u8; 4], // mov [r14+A*8], rax   hole RegA at 36
        ),
        vec![
            HoleDef::new(3, HoleType::RegB, 4),
            HoleDef::new(10, HoleType::RegC, 4),
            HoleDef::new(36, HoleType::RegA, 4),
        ],
    )
}

// ---------------------------------------------------------------------------
// Effect/runtime-dispatch stencils (Pattern B — calls lm_rt_stencil_runtime)
// ---------------------------------------------------------------------------

/// **Pow** — `R[A] = R[B] ** R[C]` (power / exponentiation).
///
/// Requires runtime dispatch (floating point or bigint for large values).
pub fn stencil_pow() -> StencilDef {
    effect_stencil(OpCode::Pow as u8, "Pow")
}

/// **Concat** — `R[A] = R[B] ++ R[C]` (string/list concatenation).
pub fn stencil_concat() -> StencilDef {
    effect_stencil(OpCode::Concat as u8, "Concat")
}

/// **In** — `R[A] = R[B] in R[C]` (membership test).
pub fn stencil_in() -> StencilDef {
    effect_stencil(OpCode::In as u8, "In")
}

/// **Is** — `R[A] = typeof(R[B]) == type(C)` (type check).
pub fn stencil_is() -> StencilDef {
    effect_stencil(OpCode::Is as u8, "Is")
}

/// **GetTuple** — `R[A] = R[B].elements[C]` (tuple element access by constant index).
pub fn stencil_get_tuple() -> StencilDef {
    effect_stencil(OpCode::GetTuple as u8, "GetTuple")
}

/// **Loop** — decrement counter, jump back if counter > 0.
pub fn stencil_loop() -> StencilDef {
    effect_stencil(OpCode::Loop as u8, "Loop")
}

/// **ForPrep** — prepare for-loop iterator.
pub fn stencil_forprep() -> StencilDef {
    effect_stencil(OpCode::ForPrep as u8, "ForPrep")
}

/// **ForLoop** — iterate for-loop (numeric).
pub fn stencil_forloop() -> StencilDef {
    effect_stencil(OpCode::ForLoop as u8, "ForLoop")
}

/// **ForIn** — for-in iterator step.
pub fn stencil_forin() -> StencilDef {
    effect_stencil(OpCode::ForIn as u8, "ForIn")
}

/// **Closure** — create a closure from proto Bx capturing upvalues from registers.
pub fn stencil_closure() -> StencilDef {
    effect_stencil(OpCode::Closure as u8, "Closure")
}

/// **GetUpval** — `R[A] = upvalue[B]`.
pub fn stencil_get_upval() -> StencilDef {
    effect_stencil(OpCode::GetUpval as u8, "GetUpval")
}

/// **SetUpval** — `upvalue[B] = R[A]`.
pub fn stencil_set_upval() -> StencilDef {
    effect_stencil(OpCode::SetUpval as u8, "SetUpval")
}

/// **ToolCall** — call an external tool.
pub fn stencil_tool_call() -> StencilDef {
    effect_stencil(OpCode::ToolCall as u8, "ToolCall")
}

/// **Schema** — validate R[A] against schema type B.
pub fn stencil_schema() -> StencilDef {
    effect_stencil(OpCode::Schema as u8, "Schema")
}

/// **Emit** — emit output R[A].
pub fn stencil_emit() -> StencilDef {
    effect_stencil(OpCode::Emit as u8, "Emit")
}

/// **TraceRef** — R[A] = current trace reference.
pub fn stencil_trace_ref() -> StencilDef {
    effect_stencil(OpCode::TraceRef as u8, "TraceRef")
}

/// **Await** — `R[A] = await future R[B]`.
pub fn stencil_await() -> StencilDef {
    effect_stencil(OpCode::Await as u8, "Await")
}

/// **Spawn** — `R[A] = spawn async(proto=Bx)`.
pub fn stencil_spawn() -> StencilDef {
    effect_stencil(OpCode::Spawn as u8, "Spawn")
}

/// **Append** — append R[B] to list R[A].
pub fn stencil_append() -> StencilDef {
    effect_stencil(OpCode::Append as u8, "Append")
}

/// **IsVariant** — if R[A] is variant with tag Bx, skip next instruction.
pub fn stencil_is_variant() -> StencilDef {
    // Calls lm_rt_stencil_runtime(ctx, instr_word) and branches on its return:
    //   rax == 0  => continue
    //   rax != 0  => skip next instruction (stitcher patches JumpOffset32)
    //
    // Runtime return uses an ABI-stable u64 sentinel (not bool), so this stencil
    // can use `test rax, rax` + `jnz`.
    StencilDef::new(
        OpCode::IsVariant as u8,
        "IsVariant",
        code!(
            [0x4Cu8, 0x89, 0xFF], // mov rdi, r15
            [0x48u8, 0xB8],
            [0x00u8; 8],          // movabs rax, <instr_word>   hole@5
            [0x48u8, 0x89, 0xC6], // mov rsi, rax
            [0x48u8, 0xB8],
            [0x00u8; 8],                            // movabs rax, <runtime_func> hole@18
            [0x48u8, 0x83, 0xEC, 0x08],             // sub rsp, 8 (align)
            [0xFFu8, 0xD0],                         // call rax
            [0x48u8, 0x83, 0xC4, 0x08],             // add rsp, 8
            [0x48u8, 0x85, 0xC0],                   // test rax, rax
            [0x0Fu8, 0x85, 0x00, 0x00, 0x00, 0x00], // jnz rel32  hole@41
        ),
        vec![
            HoleDef::new(5, HoleType::InstructionWord, 8),
            HoleDef::new(18, HoleType::RuntimeFuncAddr, 8),
            HoleDef::new(41, HoleType::JumpOffset32, 4),
        ],
    )
}

/// **Unbox** — `R[A] = R[B].payload` (extract union payload).
pub fn stencil_unbox() -> StencilDef {
    effect_stencil(OpCode::Unbox as u8, "Unbox")
}

/// **NewUnion** — `R[A] = union(tag=B, payload=R[C])`.
pub fn stencil_new_union() -> StencilDef {
    effect_stencil(OpCode::NewUnion as u8, "NewUnion")
}

// ---------------------------------------------------------------------------
// OsrCheck stencil
// ---------------------------------------------------------------------------

/// **OsrCheck** — increment hot-loop counter; tier-up if threshold reached.
///
/// Calls `lm_rt_osr_check(ctx, cell_idx, ip)` from the stencil JIT.
///
/// Arguments:
/// - rdi: VmContext* (passed via r15)
/// - rsi: cell_idx (from instruction field A)
/// - rdx: current_ip (from instruction field B)
///
/// Returns:
/// - rax: 0 if no tier-up needed, else function pointer to jump to
///
/// After the call:
/// - If rax != 0 (compiled code ready), jump to rax
/// - If rax == 0 (no tier-up), continue to next instruction
pub fn stencil_osrcheck() -> StencilDef {
    StencilDef::new(
        OpCode::OsrCheck as u8,
        "OsrCheck",
        code!(
            [0x4Cu8, 0x89, 0xFF],             // mov rdi, r15 (VmContext*)
            [0xBEu8, 0x00, 0x00, 0x00, 0x00], // mov esi, <A> (cell_idx)
            [0xBAu8, 0x00, 0x00, 0x00, 0x00], // mov edx, <B> (current_ip)
            [0x48u8, 0xB8],
            [0x00u8; 8],                // movabs rax, <lm_rt_osr_check>
            [0x48u8, 0x83, 0xEC, 0x08], // sub rsp, 8 (align stack for helper call)
            [0xFFu8, 0xD0],             // call rax
            [0x48u8, 0x83, 0xC4, 0x08], // add rsp, 8
            // After call, rax contains fn pointer (0 = no tier-up, non-zero = jump to)
            [0x48u8, 0x85, 0xC0], // test rax, rax  (sets ZF if rax==0)
            [0x0Fu8, 0x84, 0x02, 0x00, 0x00, 0x00], // jz .skip (rel32=2 to skip jmp rax)
            [0xFFu8, 0xE0],       // jmp rax (jump to compiled code)
                                  // .skip: (continue to next instruction)
        ),
        vec![
            HoleDef::new(4, HoleType::RegAIndex, 1), // cell_idx in esi
            HoleDef::new(9, HoleType::RegBIndex, 1), // current_ip in edx
            HoleDef::new(15, HoleType::RuntimeFuncAddr, 8), // function address
        ],
    )
}

// ---------------------------------------------------------------------------
// Library builder
// ---------------------------------------------------------------------------

/// Build and return a [`StencilLibrary`] containing all defined stencils.
///
/// This is the primary entry point for the stitcher (`P3-STITCHER-1`).
pub fn build_stencil_library() -> StencilLibrary {
    let mut lib = StencilLibrary::new();

    // Misc
    lib.insert(stencil_nop());
    lib.insert(stencil_osrcheck());

    // Loads / moves
    lib.insert(stencil_loadk());
    lib.insert(stencil_loadnil());
    lib.insert(stencil_loadbool());
    lib.insert(stencil_loadint());
    lib.insert(stencil_move());
    lib.insert(stencil_moveown());

    // Arithmetic
    lib.insert(stencil_add());
    lib.insert(stencil_sub());
    lib.insert(stencil_mul());
    lib.insert(stencil_div());
    lib.insert(stencil_mod());
    lib.insert(stencil_neg());

    // Comparisons
    lib.insert(stencil_eq());
    lib.insert(stencil_lt());
    lib.insert(stencil_le());
    // Note: stencil_gt() and stencil_ge() are NOT registered here because the
    // LIR has no Gt/Ge opcodes — the compiler lowers > and >= to Lt/Le with
    // swapped operands.  The stencil functions exist as dead code for reference.
    lib.insert(stencil_not());

    // Control flow
    lib.insert(stencil_jmp());
    lib.insert(stencil_break());
    lib.insert(stencil_continue());
    lib.insert(stencil_test());

    // Call / return
    lib.insert(stencil_call());
    lib.insert(stencil_tailcall());
    lib.insert(stencil_return());
    lib.insert(stencil_halt());

    // Intrinsics
    lib.insert(stencil_intrinsic());
    lib.insert_intrinsic(24, stencil_intrinsic_append());
    lib.insert_intrinsic(25, stencil_intrinsic_range());
    lib.insert_intrinsic(29, stencil_intrinsic_sort());
    lib.insert_intrinsic(129, stencil_intrinsic_sort());
    lib.insert_intrinsic(0, stencil_intrinsic_length());
    lib.insert_intrinsic(1, stencil_intrinsic_length());
    lib.insert_intrinsic(72, stencil_intrinsic_length());
    lib.insert_intrinsic(14, stencil_intrinsic_keys());
    lib.insert_intrinsic(15, stencil_intrinsic_values());

    // Effects
    lib.insert(stencil_perform());
    lib.insert(stencil_handle_push());
    lib.insert(stencil_handle_pop());
    lib.insert(stencil_resume());

    // Collections
    lib.insert(stencil_new_list());
    lib.insert(stencil_new_list_stack());
    lib.insert(stencil_new_record());
    lib.insert(stencil_new_map());
    lib.insert(stencil_new_tuple());
    lib.insert(stencil_new_tuple_stack());
    lib.insert(stencil_new_set());

    // Field / index access
    lib.insert(stencil_get_field());
    lib.insert(stencil_set_field());
    lib.insert(stencil_get_index());
    lib.insert(stencil_set_index());
    lib.insert(stencil_get_tuple());

    // Bitwise (inline fast paths)
    lib.insert(stencil_bitor());
    lib.insert(stencil_bitand());
    lib.insert(stencil_bitxor());
    lib.insert(stencil_bitnot());
    lib.insert(stencil_shl());
    lib.insert(stencil_shr());
    lib.insert(stencil_floordiv());

    // Logical (inline bool ops)
    lib.insert(stencil_and());
    lib.insert(stencil_or());
    lib.insert(stencil_nullco());

    // Arithmetic (runtime dispatch)
    lib.insert(stencil_pow());
    lib.insert(stencil_concat());

    // Comparison / membership (runtime dispatch)
    lib.insert(stencil_in());
    lib.insert(stencil_is());

    // Control flow (runtime dispatch)
    lib.insert(stencil_loop());
    lib.insert(stencil_forprep());
    lib.insert(stencil_forloop());
    lib.insert(stencil_forin());

    // Closures / upvalues (runtime dispatch)
    lib.insert(stencil_closure());
    lib.insert(stencil_get_upval());
    lib.insert(stencil_set_upval());

    // Tool / async effects (runtime dispatch)
    lib.insert(stencil_tool_call());
    lib.insert(stencil_schema());
    lib.insert(stencil_emit());
    lib.insert(stencil_trace_ref());
    lib.insert(stencil_await());
    lib.insert(stencil_spawn());

    // Union / variant ops (runtime dispatch)
    lib.insert(stencil_new_union());
    lib.insert(stencil_is_variant());
    lib.insert(stencil_unbox());

    // List mutation (runtime dispatch)
    lib.insert(stencil_append());

    lib
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nop_stencil() {
        let s = stencil_nop();
        assert_eq!(s.code, vec![0x90]);
        assert!(s.holes.is_empty());
    }

    #[test]
    fn test_move_stencil_layout() {
        let s = stencil_move();
        assert_eq!(s.code.len(), 14);
        assert_eq!(s.holes.len(), 2);
        // RegB hole at offset 3
        assert_eq!(s.holes[0].offset, 3);
        assert_eq!(s.holes[0].hole_type, HoleType::RegB);
        assert_eq!(s.holes[0].size, 4);
        // RegA hole at offset 10
        assert_eq!(s.holes[1].offset, 10);
        assert_eq!(s.holes[1].hole_type, HoleType::RegA);
        assert_eq!(s.holes[1].size, 4);
    }

    #[test]
    fn test_loadk_stencil_layout() {
        let s = stencil_loadk();
        // movabs (2) + imm64 (8) + mov [r14+disp32] (3) + disp32 (4) = 17 bytes
        assert_eq!(s.code.len(), 17);
        assert_eq!(s.holes.len(), 2);
        assert_eq!(s.holes[0].hole_type, HoleType::Constant64);
        assert_eq!(s.holes[0].offset, 2);
        assert_eq!(s.holes[0].size, 8);
        assert_eq!(s.holes[1].hole_type, HoleType::RegA);
        assert_eq!(s.holes[1].offset, 13);
    }

    #[test]
    fn test_loadnil_has_correct_null_value() {
        let s = stencil_loadnil();
        // Bytes 2..10 should be the little-endian encoding of 0x7FFC_0000_0000_0000
        assert_eq!(&s.code[2..10], &NULL_VALUE_LE[..]);
        assert_eq!(s.holes.len(), 1);
        assert_eq!(s.holes[0].hole_type, HoleType::RegA);
    }

    #[test]
    fn test_jmp_stencil() {
        let s = stencil_jmp();
        assert_eq!(s.code[0], 0xE9); // jmp rel32 opcode
        assert_eq!(s.holes.len(), 1);
        assert_eq!(s.holes[0].hole_type, HoleType::JumpOffset32);
        assert_eq!(s.holes[0].offset, 1);
        assert_eq!(s.holes[0].size, 4);
    }

    #[test]
    fn test_add_stencil_has_tag_check_holes() {
        let s = stencil_add();
        // Holes: RegB, RegC, RegA, InstructionWord, RuntimeFuncAddr
        assert_eq!(s.holes.len(), 5);
        assert_eq!(s.holes[0].hole_type, HoleType::RegB);
        assert_eq!(s.holes[1].hole_type, HoleType::RegC);
        assert_eq!(s.holes[2].hole_type, HoleType::RegA);
        assert_eq!(s.holes[3].hole_type, HoleType::InstructionWord);
        assert_eq!(s.holes[4].hole_type, HoleType::RuntimeFuncAddr);

        // Baked `jne` rel32 offsets must land exactly at the slow path start.
        // Add compute bytes = 3, so slow path starts at 101 + 3 = 104.
        let slow_path_start = 104i32;

        let jne1_rel = i32::from_le_bytes(s.code[29..33].try_into().expect("jne1 rel32"));
        let jne1_target = 33 + jne1_rel;
        assert_eq!(jne1_target, slow_path_start);

        let jne2_rel = i32::from_le_bytes(s.code[48..52].try_into().expect("jne2 rel32"));
        let jne2_target = 52 + jne2_rel;
        assert_eq!(jne2_target, slow_path_start);
    }

    #[test]
    fn test_return_stencil() {
        let s = stencil_return();
        // mov rdi, r15 (3) + mov esi, imm32 (5) + movabs rax (10) + sub rsp (4) + call rax (2) + add rsp (4) = 28 bytes
        assert_eq!(s.code.len(), 28);
        assert_eq!(s.holes.len(), 2);
        assert_eq!(s.holes[0].hole_type, HoleType::RegAIndex);
        assert_eq!(s.holes[1].hole_type, HoleType::RuntimeFuncAddr);
    }

    #[test]
    fn test_test_stencil() {
        let s = stencil_test();
        // load (7) + and (3) + cmp (3) + je (6) = 19 bytes
        assert_eq!(s.code.len(), 19);
        assert_eq!(s.holes.len(), 3);
        assert_eq!(s.holes[0].hole_type, HoleType::RegA);
        assert_eq!(s.holes[1].hole_type, HoleType::RegCIndex);
        assert_eq!(s.holes[2].hole_type, HoleType::JumpOffset32);
    }

    #[test]
    fn test_build_stencil_library_completeness() {
        let lib = build_stencil_library();
        // Verify key opcodes are present
        assert!(lib.get(OpCode::Nop as u8).is_some(), "Nop missing");
        assert!(lib.get(OpCode::Move as u8).is_some(), "Move missing");
        assert!(lib.get(OpCode::Add as u8).is_some(), "Add missing");
        assert!(lib.get(OpCode::Jmp as u8).is_some(), "Jmp missing");
        assert!(lib.get(OpCode::Return as u8).is_some(), "Return missing");
        assert!(lib.get(OpCode::Perform as u8).is_some(), "Perform missing");
        assert!(lib.get(OpCode::NewList as u8).is_some(), "NewList missing");
        assert!(
            lib.get(OpCode::GetField as u8).is_some(),
            "GetField missing"
        );
        // At least 30 stencils
        assert!(lib.len() >= 30, "expected ≥30 stencils, got {}", lib.len());
    }

    #[test]
    fn test_library_round_trip() {
        let lib = build_stencil_library();
        let bytes = lib.to_bytes();
        let lib2 = crate::stencil_format::StencilLibrary::from_bytes(&bytes)
            .expect("round-trip deserialise failed");
        assert_eq!(lib.len(), lib2.len());
        // Spot-check Move
        let mv = lib2.get(OpCode::Move as u8).unwrap();
        assert_eq!(mv.name, "Move");
        assert_eq!(mv.code.len(), 14);
        assert_eq!(mv.holes.len(), 2);
    }

    #[test]
    fn test_neg_stencil() {
        let s = stencil_neg();
        // 1 load + 1 tag check + sign_extend + neg + rebox + store
        assert_eq!(s.holes.len(), 3);
        assert_eq!(s.holes[0].hole_type, HoleType::RegB);
        assert_eq!(s.holes[1].hole_type, HoleType::JumpOffset32);
        assert_eq!(s.holes[2].hole_type, HoleType::RegA);
    }

    #[test]
    fn test_is_variant_stencil_skip_hole() {
        let s = stencil_is_variant();
        assert_eq!(s.holes.len(), 3);
        assert_eq!(s.holes[0].hole_type, HoleType::InstructionWord);
        assert_eq!(s.holes[1].hole_type, HoleType::RuntimeFuncAddr);
        assert_eq!(s.holes[2].hole_type, HoleType::JumpOffset32);
        assert_eq!(s.holes[2].offset, 41);
    }
}
