//! Canonical opcode + intrinsic coverage table for tiered execution.
//!
//! This table is consumed by tests in `lumen-rt` to ensure that Tier 1/2
//! coverage stays in sync with the VM and JIT implementations.

use crate::lir::{IntrinsicId, OpCode};
use std::sync::OnceLock;
use strum::IntoEnumIterator;

/// Tier-0 interpreter support flag.
pub const TIER_INTERP: u8 = 1 << 0;
/// Tier-1 stencil support flag.
pub const TIER_STENCIL: u8 = 1 << 1;
/// Tier-2 Cranelift JIT support flag.
pub const TIER_JIT: u8 = 1 << 2;

/// Simple metadata record for opcodes/intrinsics.
#[derive(Debug, Clone)]
pub struct OpInfo {
    pub id: u16,
    pub name: String,
    pub tiers: u8,
}

const fn tiers(jit: bool) -> u8 {
    if jit {
        TIER_INTERP | TIER_JIT
    } else {
        TIER_INTERP
    }
}

fn opcode_support(op: OpCode) -> OpInfo {
    let name = op.as_ref().to_string();
    let tiers = match op {
        OpCode::LoadK
        | OpCode::LoadNil
        | OpCode::LoadBool
        | OpCode::LoadInt
        | OpCode::Move
        | OpCode::MoveOwn
        | OpCode::NewList
        | OpCode::NewMap
        | OpCode::NewRecord
        | OpCode::NewUnion
        | OpCode::NewTuple
        | OpCode::NewSet
        | OpCode::GetField
        | OpCode::SetField
        | OpCode::GetIndex
        | OpCode::SetIndex
        | OpCode::Add
        | OpCode::Sub
        | OpCode::Mul
        | OpCode::Div
        | OpCode::Mod
        | OpCode::Pow
        | OpCode::Neg
        | OpCode::Concat
        | OpCode::FloorDiv
        | OpCode::BitOr
        | OpCode::BitAnd
        | OpCode::BitXor
        | OpCode::BitNot
        | OpCode::Shl
        | OpCode::Shr
        | OpCode::Eq
        | OpCode::Lt
        | OpCode::Le
        | OpCode::Not
        | OpCode::And
        | OpCode::Or
        | OpCode::In
        | OpCode::Is
        | OpCode::NullCo
        | OpCode::Test
        | OpCode::Jmp
        | OpCode::Call
        | OpCode::TailCall
        | OpCode::Return
        | OpCode::Halt
        | OpCode::Loop
        | OpCode::ForPrep
        | OpCode::ForLoop
        | OpCode::ForIn
        | OpCode::Break
        | OpCode::Continue
        | OpCode::Intrinsic
        | OpCode::Append
        | OpCode::NewListStack
        | OpCode::NewTupleStack
        | OpCode::IsVariant
        | OpCode::Unbox
        | OpCode::Spawn
        | OpCode::Await
        | OpCode::Perform
        | OpCode::HandlePush
        | OpCode::HandlePop
        | OpCode::Resume
        | OpCode::ToolCall
        | OpCode::Schema
        | OpCode::Emit
        | OpCode::TraceRef
        | OpCode::Nop
        | OpCode::OsrCheck => tiers(true),
        OpCode::GetTuple | OpCode::Closure | OpCode::GetUpval | OpCode::SetUpval => tiers(false),
    };

    OpInfo {
        id: op as u16,
        name,
        tiers,
    }
}

fn intrinsic_support(intrinsic: IntrinsicId) -> OpInfo {
    let name = intrinsic.as_ref().to_string();
    let tiers = match intrinsic as u16 {
        0 | 1 | 2 | 3 | 9 | 10 | 11 | 12 | 13 | 14 | 15 | 16 | 17 | 18 | 19 | 20 | 21 | 22 | 23
        | 24 | 25 | 26 | 27 | 28 | 29 | 30 | 31 | 32 | 33 | 34 | 35 | 36 | 37 | 38 | 39 | 40
        | 41 | 42 | 43 | 44 | 45 | 46 | 47 | 48 | 49 | 50 | 51 | 52 | 53 | 54 | 55 | 56 | 57
        | 58 | 59 | 60 | 61 | 62 | 63 | 64 | 65 | 66 | 67 | 68 | 69 | 70 | 71 | 72 | 73 | 74
        | 75 | 77 | 85 | 96 | 97 | 106 | 120 | 121 | 122 | 123 | 124 | 125 | 126 | 127 | 128
        | 129 | 130 | 131 | 133 | 138 | 139 => tiers(true),
        _ => tiers(false),
    };

    OpInfo {
        id: intrinsic as u16,
        name,
        tiers,
    }
}

/// Canonical OpCode table with tier support flags.
///
/// Tier mapping is derived from current JIT coverage (strict mode).
static OPCODE_TABLE: OnceLock<Vec<OpInfo>> = OnceLock::new();
static INTRINSIC_TABLE: OnceLock<Vec<OpInfo>> = OnceLock::new();

pub fn opcode_table() -> &'static [OpInfo] {
    OPCODE_TABLE
        .get_or_init(|| {
            let mut entries: Vec<OpInfo> = OpCode::iter().map(opcode_support).collect();
            entries.sort_by_key(|info| info.id);
            entries
        })
        .as_slice()
}

/// Canonical IntrinsicId table with tier support flags.
///
/// Tier mapping matches JIT intrinsic lowering; intrinsics that require
/// external services remain interpreter-only.
pub fn intrinsic_table() -> &'static [OpInfo] {
    INTRINSIC_TABLE
        .get_or_init(|| {
            let mut entries: Vec<OpInfo> = IntrinsicId::iter().map(intrinsic_support).collect();
            entries.sort_by_key(|info| info.id);
            entries
        })
        .as_slice()
}
