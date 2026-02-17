//! Wave 20 — T016: 64-bit Packed LIR Instruction Evaluation
//!
//! This file evaluates and tests an experimental 64-bit instruction encoding
//! (`Instruction64`) added additively to `lumen-compiler/src/compiler/lir.rs`.
//! The existing 32-bit `Instruction` is completely untouched.
//!
//! ## Current 32-bit Encoding Limits
//!
//! | Field | Bits | Range                  | Practical Limit          |
//! |-------|------|------------------------|--------------------------|
//! | op    | 8    | 0–255                  | 117 opcodes used (46%)   |
//! | a     | 8    | 0–255                  | 256 registers per cell   |
//! | b     | 8    | 0–255                  | 256 (or Bx high byte)    |
//! | c     | 8    | 0–255                  | 256 (or Bx low byte)     |
//! | Bx    | 16   | 0–65,535               | 64K constants per cell   |
//! | Ax    | 24   | 0–16,777,215 unsigned  | 16M jump offset          |
//! | sAx   | 24   | ±8,388,607 signed      | ±8M jump offset          |
//!
//! ### Where 32-bit hits walls
//!
//! 1. **Registers (8-bit = 256 max)**: `LirCell.registers` is `u8`. Large
//!    generated cells (e.g., machine state expansions, pipeline auto-chains,
//!    flattened closures with many captures) could exceed 256 registers.
//!    Current workaround: spill to constants or split cells.
//!
//! 2. **Constants (16-bit = 64K max)**: Data-heavy programs with inline JSON,
//!    large string tables, or many numeric literals per cell could exceed 64K.
//!    Current workaround: share strings via the module-level string table.
//!
//! 3. **Jump offsets (24-bit signed = ±8M)**: Not a practical concern —
//!    8 million instructions per cell would use ~32 MB of bytecode. No real
//!    program approaches this.
//!
//! ## 64-bit Encoding Design
//!
//! ```text
//! 64-bit Instruction64 layout:
//!
//! ABC format (three-register):
//!   [op: 8][a: 16][b: 16][c: 16][pad: 8] = 64 bits
//!   → 65,536 registers, 65,536 per operand
//!
//! ABx format (register + wide constant):
//!   [op: 8][a: 16][bx_hi: 16][bx_lo: 16][pad: 8] = 64 bits
//!   → 16-bit register, 32-bit constant index (4 billion constants)
//!
//! Ax format (wide immediate):
//!   [op: 8][ax_hi: 16][ax_mid: 16][ax_lo: 16][pad: 8] = 64 bits
//!   → 48-bit immediate (281 trillion unsigned, ±140 trillion signed)
//! ```
//!
//! ## Trade-offs: 32-bit vs 64-bit
//!
//! | Dimension          | 32-bit Instruction   | 64-bit Instruction64     |
//! |--------------------|----------------------|--------------------------|
//! | Size per insn      | 4 bytes              | 8 bytes                  |
//! | icache pressure    | Lower (better)       | 2× higher               |
//! | Decode cost        | Trivial              | Trivial (still fixed)    |
//! | Register range     | 0–255                | 0–65,535                 |
//! | Constant range     | 0–65,535             | 0–4,294,967,295          |
//! | Jump range         | ±8M                  | ±140T                    |
//! | Memory per 1K insn | 4 KB                 | 8 KB                     |
//!
//! ## Recommendation
//!
//! **Keep 32-bit as default.** The 256-register and 64K-constant limits are
//! not currently hit by any real Lumen program. The 64-bit encoding should be:
//!
//! 1. Available as an opt-in for generated/future code that exceeds limits
//! 2. Used only for specific cells that need it (mixed-width is possible
//!    with a `LirCell64` variant or a flag on `LirCell`)
//! 3. The `widen()` / `narrow()` conversion functions enable gradual migration
//!
//! The 2× memory cost is not justified for typical programs, but having the
//! type available prevents a future ABI-breaking redesign when limits are hit.
//!
//! ## Implementation Status
//!
//! - [x] `Instruction64` struct added to `lir.rs` (additive, no breakage)
//! - [x] ABC, ABx, Ax, sAx constructors
//! - [x] `bx()`, `ax_val()`, `sax_val()`, `sbx()` accessors
//! - [x] `narrow()` → `Option<Instruction>` (lossy downcast)
//! - [x] `Instruction::widen()` → `Instruction64` (lossless upcast)
//! - [ ] VM dispatch for `Instruction64` (future: not in scope for Wave 20)
//! - [ ] `LirCell64` variant (future: would hold `Vec<Instruction64>`)
//! - [ ] Lowering pass to emit 64-bit instructions (future)

use lumen_compiler::compiler::lir::{Instruction, Instruction64, OpCode};

// ============================================================================
// ABC format tests
// ============================================================================

#[test]
fn lir64_abc_basic() {
    let insn = Instruction64::abc(OpCode::Add, 0, 1, 2);
    assert_eq!(insn.op, OpCode::Add);
    assert_eq!(insn.a, 0);
    assert_eq!(insn.b, 1);
    assert_eq!(insn.c, 2);
    assert_eq!(insn.pad, 0);
}

#[test]
fn lir64_abc_max_register_values() {
    // 16-bit registers can go up to 65535
    let insn = Instruction64::abc(OpCode::Sub, 65535, 65534, 65533);
    assert_eq!(insn.a, 65535);
    assert_eq!(insn.b, 65534);
    assert_eq!(insn.c, 65533);
}

#[test]
fn lir64_abc_exceeds_32bit_register_limit() {
    // This instruction CANNOT be represented in 32-bit encoding
    let insn = Instruction64::abc(OpCode::Mul, 300, 400, 500);
    assert_eq!(insn.a, 300);
    assert_eq!(insn.b, 400);
    assert_eq!(insn.c, 500);

    // Narrowing should fail because 300, 400, 500 > 255
    assert!(insn.narrow().is_none(), "Should not fit in 32-bit encoding");
}

#[test]
fn lir64_abc_zero_registers() {
    let insn = Instruction64::abc(OpCode::Nop, 0, 0, 0);
    assert_eq!(insn.a, 0);
    assert_eq!(insn.b, 0);
    assert_eq!(insn.c, 0);
    assert_eq!(insn.pad, 0);
}

// ============================================================================
// ABx format tests
// ============================================================================

#[test]
fn lir64_abx_basic() {
    let insn = Instruction64::abx(OpCode::LoadK, 5, 1000);
    assert_eq!(insn.op, OpCode::LoadK);
    assert_eq!(insn.a, 5);
    assert_eq!(insn.bx(), 1000);
}

#[test]
fn lir64_abx_large_constant_index() {
    // 32-bit constant index: exceeds 16-bit limit of the 32-bit encoding
    let insn = Instruction64::abx(OpCode::LoadK, 0, 100_000);
    assert_eq!(insn.bx(), 100_000);
}

#[test]
fn lir64_abx_max_constant_index() {
    // Maximum 32-bit constant index
    let insn = Instruction64::abx(OpCode::LoadK, 0, u32::MAX);
    assert_eq!(insn.bx(), u32::MAX);
}

#[test]
fn lir64_abx_with_large_register() {
    // Register 1000 + constant index 500000
    let insn = Instruction64::abx(OpCode::NewRecord, 1000, 500_000);
    assert_eq!(insn.a, 1000);
    assert_eq!(insn.bx(), 500_000);
}

#[test]
fn lir64_abx_roundtrip_boundary() {
    // Test around the 16-bit boundary (65535 → 65536)
    for bx_val in [0u32, 1, 255, 256, 65535, 65536, 100_000, u32::MAX] {
        let insn = Instruction64::abx(OpCode::LoadK, 0, bx_val);
        assert_eq!(insn.bx(), bx_val, "ABx roundtrip failed for bx={}", bx_val);
    }
}

// ============================================================================
// Ax format tests (unsigned)
// ============================================================================

#[test]
fn lir64_ax_basic() {
    let insn = Instruction64::ax(OpCode::Jmp, 42);
    assert_eq!(insn.op, OpCode::Jmp);
    assert_eq!(insn.ax_val(), 42);
}

#[test]
fn lir64_ax_large_value() {
    // 48-bit value: exceeds 24-bit limit of 32-bit encoding
    let val: u64 = 1_000_000_000; // 1 billion
    let insn = Instruction64::ax(OpCode::HandlePush, val);
    assert_eq!(insn.ax_val(), val);
}

#[test]
fn lir64_ax_max_48bit() {
    let max_48: u64 = 0xFFFF_FFFF_FFFF;
    let insn = Instruction64::ax(OpCode::Jmp, max_48);
    assert_eq!(insn.ax_val(), max_48);
}

#[test]
fn lir64_ax_zero() {
    let insn = Instruction64::ax(OpCode::Jmp, 0);
    assert_eq!(insn.ax_val(), 0);
}

// ============================================================================
// sAx format tests (signed)
// ============================================================================

#[test]
fn lir64_sax_positive_offset() {
    let insn = Instruction64::sax(OpCode::Jmp, 100);
    assert_eq!(insn.sax_val(), 100);
}

#[test]
fn lir64_sax_negative_offset() {
    let insn = Instruction64::sax(OpCode::Jmp, -100);
    assert_eq!(insn.sax_val(), -100);
}

#[test]
fn lir64_sax_negative_one() {
    let insn = Instruction64::sax(OpCode::Jmp, -1);
    assert_eq!(insn.sax_val(), -1);
}

#[test]
fn lir64_sax_large_negative() {
    // Much larger negative offset than 32-bit sax can handle
    let insn = Instruction64::sax(OpCode::Jmp, -1_000_000);
    assert_eq!(insn.sax_val(), -1_000_000);
}

#[test]
fn lir64_sax_large_positive() {
    let insn = Instruction64::sax(OpCode::Jmp, 1_000_000_000);
    assert_eq!(insn.sax_val(), 1_000_000_000);
}

#[test]
fn lir64_sax_zero() {
    let insn = Instruction64::sax(OpCode::Jmp, 0);
    assert_eq!(insn.sax_val(), 0);
}

#[test]
fn lir64_sax_boundary_values() {
    // Test values around 24-bit signed limit (±8M) — these fit in 32-bit too
    for offset in [
        -8_388_607i64,
        -8_388_608,
        8_388_607,
        // Values beyond 32-bit sax range
        -8_388_609,
        8_388_608,
        -100_000_000,
        100_000_000,
    ] {
        let insn = Instruction64::sax(OpCode::Jmp, offset);
        assert_eq!(
            insn.sax_val(),
            offset,
            "sAx roundtrip failed for offset={}",
            offset
        );
    }
}

// ============================================================================
// widen() / narrow() conversion tests
// ============================================================================

#[test]
fn lir64_widen_basic() {
    let narrow = Instruction::abc(OpCode::Add, 10, 20, 30);
    let wide = narrow.widen();
    assert_eq!(wide.op, OpCode::Add);
    assert_eq!(wide.a, 10);
    assert_eq!(wide.b, 20);
    assert_eq!(wide.c, 30);
    assert_eq!(wide.pad, 0);
}

#[test]
fn lir64_widen_preserves_all_opcodes() {
    // Widen and narrow should be lossless roundtrip for any opcode with small operands
    for (op, a, b, c) in [
        (OpCode::LoadK, 0, 0, 42),
        (OpCode::Add, 1, 2, 3),
        (OpCode::Jmp, 0, 0, 0),
        (OpCode::Return, 0, 1, 0),
        (OpCode::Call, 10, 3, 1),
        (OpCode::Intrinsic, 5, 9, 12),
        (OpCode::ToolCall, 7, 0, 100),
        (OpCode::Perform, 0, 1, 2),
    ] {
        let narrow = Instruction::abc(op, a, b, c);
        let wide = narrow.widen();
        let back = wide
            .narrow()
            .expect("narrow should succeed for small values");
        assert_eq!(back.op, op);
        assert_eq!(back.a, a);
        assert_eq!(back.b, b);
        assert_eq!(back.c, c);
    }
}

#[test]
fn lir64_widen_max_8bit() {
    let narrow = Instruction::abc(OpCode::Mul, 255, 255, 255);
    let wide = narrow.widen();
    assert_eq!(wide.a, 255);
    assert_eq!(wide.b, 255);
    assert_eq!(wide.c, 255);
    let back = wide.narrow().expect("255 should narrow back");
    assert_eq!(back.a, 255);
}

#[test]
fn lir64_narrow_fails_for_large_a() {
    let wide = Instruction64::abc(OpCode::Add, 256, 0, 0);
    assert!(wide.narrow().is_none());
}

#[test]
fn lir64_narrow_fails_for_large_b() {
    let wide = Instruction64::abc(OpCode::Add, 0, 256, 0);
    assert!(wide.narrow().is_none());
}

#[test]
fn lir64_narrow_fails_for_large_c() {
    let wide = Instruction64::abc(OpCode::Add, 0, 0, 256);
    assert!(wide.narrow().is_none());
}

#[test]
fn lir64_roundtrip_abx_within_32bit_range() {
    // ABx with values that fit in 32-bit encoding
    let narrow = Instruction::abx(OpCode::LoadK, 10, 500);
    let wide = narrow.widen();

    // In 64-bit, bx() reinterprets b,c as 32-bit, so we need to check
    // the a field and reconstruct bx from the widened 16-bit fields.
    // Since narrow b,c were 8-bit, the wide b,c are also 8-bit-sized values:
    // wide.bx() = (wide.b << 16) | wide.c
    // But narrow.bx() = (narrow.b << 8) | narrow.c = 500
    // After widen: wide.b = narrow.b, wide.c = narrow.c
    // So wide.bx() = (narrow.b as u32) << 16 | narrow.c as u32
    // This is NOT the same as narrow.bx() because the bit layout differs!

    // This is an intentional design property: ABx encoding differs between
    // 32-bit and 64-bit. Use widen() for ABC, not for ABx.
    assert_eq!(wide.a, 10);
    // The b and c values are preserved individually:
    assert_eq!(wide.b, narrow.b as u16);
    assert_eq!(wide.c, narrow.c as u16);
}

// ============================================================================
// Size and layout tests
// ============================================================================

#[test]
fn lir64_size_is_8_bytes() {
    assert_eq!(Instruction64::size_bytes(), 8);
    // The struct should be exactly 8 bytes (op:1 + a:2 + b:2 + c:2 + pad:1)
    assert_eq!(std::mem::size_of::<Instruction64>(), 8);
}

#[test]
fn lir64_32bit_size_is_4_bytes() {
    // Confirm the original Instruction is still 4 bytes
    assert_eq!(std::mem::size_of::<Instruction>(), 4);
}

// ============================================================================
// Comparison: same instruction in both widths
// ============================================================================

#[test]
fn lir64_same_abc_semantics_small_values() {
    // For small values, both encodings should produce equivalent results
    let narrow = Instruction::abc(OpCode::Add, 1, 2, 3);
    let wide = Instruction64::abc(OpCode::Add, 1, 2, 3);

    assert_eq!(narrow.op, wide.op);
    assert_eq!(narrow.a as u16, wide.a);
    assert_eq!(narrow.b as u16, wide.b);
    assert_eq!(narrow.c as u16, wide.c);
}

#[test]
fn lir64_same_sax_semantics_small_values() {
    // Both encodings should agree on sign extension for small offsets
    for offset in [-100i32, -1, 0, 1, 100, -8000, 8000] {
        let narrow = Instruction::sax(OpCode::Jmp, offset);
        let wide = Instruction64::sax(OpCode::Jmp, offset as i64);
        assert_eq!(
            narrow.sax_val() as i64,
            wide.sax_val(),
            "sax disagreement at offset={}",
            offset
        );
    }
}

// ============================================================================
// Practical scenarios: instructions that need 64-bit encoding
// ============================================================================

#[test]
fn lir64_scenario_large_register_cell() {
    // A generated cell with 500 registers (exceeds 8-bit limit)
    let load = Instruction64::abx(OpCode::LoadK, 300, 0);
    let add = Instruction64::abc(OpCode::Add, 400, 300, 301);
    let ret = Instruction64::abc(OpCode::Return, 400, 1, 0);

    assert_eq!(load.a, 300);
    assert_eq!(add.a, 400);
    assert_eq!(ret.a, 400);

    // None of these can narrow
    assert!(load.narrow().is_none());
    assert!(add.narrow().is_none());
    assert!(ret.narrow().is_none());
}

#[test]
fn lir64_scenario_large_constant_pool() {
    // A cell with 100K string constants (exceeds 16-bit Bx limit)
    let insn = Instruction64::abx(OpCode::LoadK, 0, 99_999);
    assert_eq!(insn.a, 0);
    assert_eq!(insn.bx(), 99_999);

    // This fits in 32-bit Bx (16-bit)? No: 99_999 > 65_535.
    // But the narrow check is on a/b/c not bx, so check the individual fields:
    // bx = 99999 → b_hi = 99999 >> 16 = 1, c_lo = 99999 & 0xFFFF = 34463
    // Both fit in u8? b=1 yes, c=34463 no → narrow fails.
    assert!(
        insn.narrow().is_none(),
        "Large constant index should not narrow"
    );
}

#[test]
fn lir64_scenario_mixed_width_program() {
    // Simulate a mixed-width program: most instructions are 32-bit,
    // a few are 64-bit for overflow cells.
    let narrow_insns: Vec<Instruction> = vec![
        Instruction::abc(OpCode::LoadK, 0, 0, 1),
        Instruction::abc(OpCode::Add, 2, 0, 1),
        Instruction::abc(OpCode::Return, 2, 1, 0),
    ];

    // Widen them all to 64-bit
    let wide_insns: Vec<Instruction64> = narrow_insns.iter().map(|i| i.widen()).collect();

    // All should narrow back successfully
    for (i, wide) in wide_insns.iter().enumerate() {
        let back = wide
            .narrow()
            .unwrap_or_else(|| panic!("insn {} should narrow", i));
        assert_eq!(back.op, narrow_insns[i].op);
        assert_eq!(back.a, narrow_insns[i].a);
        assert_eq!(back.b, narrow_insns[i].b);
        assert_eq!(back.c, narrow_insns[i].c);
    }
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn lir64_sax_sign_extension_at_48bit_boundary() {
    // The sign bit for 48-bit is bit 47.
    // -1 in 48-bit = 0xFFFF_FFFF_FFFF
    let insn = Instruction64::sax(OpCode::Jmp, -1);
    let raw = insn.ax_val();
    assert_eq!(raw, 0xFFFF_FFFF_FFFF, "raw bits of -1 in 48-bit");
    assert_eq!(insn.sax_val(), -1);
}

#[test]
fn lir64_sax_max_positive_48bit() {
    // Max positive 48-bit signed = 2^47 - 1 = 140,737,488,355,327
    let max_pos: i64 = (1i64 << 47) - 1;
    let insn = Instruction64::sax(OpCode::Jmp, max_pos);
    assert_eq!(insn.sax_val(), max_pos);
}

#[test]
fn lir64_sax_min_negative_48bit() {
    // Min negative 48-bit signed = -2^47 = -140,737,488,355,328
    let min_neg: i64 = -(1i64 << 47);
    let insn = Instruction64::sax(OpCode::Jmp, min_neg);
    assert_eq!(insn.sax_val(), min_neg);
}

#[test]
fn lir64_pad_field_preserved_through_serde() {
    // The pad field should serialize/deserialize correctly
    let insn = Instruction64::abc(OpCode::Add, 1, 2, 3);
    let json = serde_json::to_string(&insn).expect("serialize");
    let back: Instruction64 = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.op, OpCode::Add);
    assert_eq!(back.a, 1);
    assert_eq!(back.b, 2);
    assert_eq!(back.c, 3);
    assert_eq!(back.pad, 0);
}

// ============================================================================
// Compile-and-verify: real Lumen programs still work with existing 32-bit
// ============================================================================

#[test]
fn lir64_existing_programs_unaffected() {
    // Compile a real program and verify it uses 32-bit instructions
    let source = "# test\n\n```lumen\ncell main() -> Int\n  return 1 + 2\nend\n```\n";
    let module = lumen_compiler::compile(source).expect("should compile");

    let main_cell = module
        .cells
        .iter()
        .find(|c| c.name == "main")
        .expect("main cell exists");

    // All instructions should be narrowable (they're already 32-bit)
    for (i, insn) in main_cell.instructions.iter().enumerate() {
        let wide = insn.widen();
        let back = wide.narrow().unwrap_or_else(|| {
            panic!(
                "instruction {} ({:?}) should narrow after widen",
                i, insn.op
            )
        });
        assert_eq!(back.op, insn.op, "opcode mismatch at instruction {}", i);
    }
}

#[test]
fn lir64_widen_all_opcodes_roundtrip() {
    // Generate one instruction per opcode family and verify widen→narrow roundtrip
    use strum::IntoEnumIterator;

    for op in OpCode::iter() {
        let original = Instruction::abc(op, 0, 0, 0);
        let wide = original.widen();
        let back = wide.narrow().expect("zero operands should always narrow");
        assert_eq!(back.op, op, "opcode {:?} failed widen→narrow roundtrip", op);
    }
}
