//! Canonical opcode + intrinsic coverage table for tiered execution.
//!
//! This table is consumed by tests in `lumen-rt` to ensure that Tier 1/2
//! coverage stays in sync with the VM and JIT implementations.

use crate::lir::{IntrinsicId, OpCode};

/// Tier-0 interpreter support flag.
pub const TIER_INTERP: u8 = 1 << 0;
/// Tier-1 stencil support flag.
pub const TIER_STENCIL: u8 = 1 << 1;
/// Tier-2 Cranelift JIT support flag.
pub const TIER_JIT: u8 = 1 << 2;

/// Simple metadata record for opcodes/intrinsics.
#[derive(Debug, Clone, Copy)]
pub struct OpInfo {
    pub id: u16,
    pub name: &'static str,
    pub tiers: u8,
}

/// Canonical OpCode table with tier support flags.
///
/// Tier mapping is derived from current stencil/JIT coverage (strict mode).
pub const OPCODE_TABLE: &[OpInfo] = &[
    // Misc
    OpInfo {
        id: OpCode::Nop as u16,
        name: "Nop",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    // Register and constant ops
    OpInfo {
        id: OpCode::LoadK as u16,
        name: "LoadK",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::LoadNil as u16,
        name: "LoadNil",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::LoadBool as u16,
        name: "LoadBool",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::LoadInt as u16,
        name: "LoadInt",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Move as u16,
        name: "Move",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::MoveOwn as u16,
        name: "MoveOwn",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    // Data construction
    OpInfo {
        id: OpCode::NewList as u16,
        name: "NewList",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::NewMap as u16,
        name: "NewMap",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::NewRecord as u16,
        name: "NewRecord",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::NewUnion as u16,
        name: "NewUnion",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::NewTuple as u16,
        name: "NewTuple",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::NewSet as u16,
        name: "NewSet",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    // Access
    OpInfo {
        id: OpCode::GetField as u16,
        name: "GetField",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::SetField as u16,
        name: "SetField",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::GetIndex as u16,
        name: "GetIndex",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::SetIndex as u16,
        name: "SetIndex",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::GetTuple as u16,
        name: "GetTuple",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    // Arithmetic
    OpInfo {
        id: OpCode::Add as u16,
        name: "Add",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Sub as u16,
        name: "Sub",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Mul as u16,
        name: "Mul",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Div as u16,
        name: "Div",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Mod as u16,
        name: "Mod",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Pow as u16,
        name: "Pow",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Neg as u16,
        name: "Neg",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Concat as u16,
        name: "Concat",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::FloorDiv as u16,
        name: "FloorDiv",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    // Bitwise
    OpInfo {
        id: OpCode::BitOr as u16,
        name: "BitOr",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::BitAnd as u16,
        name: "BitAnd",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::BitXor as u16,
        name: "BitXor",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::BitNot as u16,
        name: "BitNot",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Shl as u16,
        name: "Shl",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Shr as u16,
        name: "Shr",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    // Comparison / logic
    OpInfo {
        id: OpCode::Eq as u16,
        name: "Eq",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Lt as u16,
        name: "Lt",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Le as u16,
        name: "Le",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Not as u16,
        name: "Not",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::And as u16,
        name: "And",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Or as u16,
        name: "Or",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::In as u16,
        name: "In",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Is as u16,
        name: "Is",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::NullCo as u16,
        name: "NullCo",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Test as u16,
        name: "Test",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    // Control flow
    OpInfo {
        id: OpCode::Jmp as u16,
        name: "Jmp",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Call as u16,
        name: "Call",
        tiers: TIER_INTERP | TIER_JIT,
    },
    OpInfo {
        id: OpCode::TailCall as u16,
        name: "TailCall",
        tiers: TIER_INTERP | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Return as u16,
        name: "Return",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Halt as u16,
        name: "Halt",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Loop as u16,
        name: "Loop",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::ForPrep as u16,
        name: "ForPrep",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::ForLoop as u16,
        name: "ForLoop",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::ForIn as u16,
        name: "ForIn",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Break as u16,
        name: "Break",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Continue as u16,
        name: "Continue",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    // Intrinsics
    OpInfo {
        id: OpCode::Intrinsic as u16,
        name: "Intrinsic",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    // Closures
    OpInfo {
        id: OpCode::Closure as u16,
        name: "Closure",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::GetUpval as u16,
        name: "GetUpval",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::SetUpval as u16,
        name: "SetUpval",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    // Effects
    OpInfo {
        id: OpCode::ToolCall as u16,
        name: "ToolCall",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Schema as u16,
        name: "Schema",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Emit as u16,
        name: "Emit",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::TraceRef as u16,
        name: "TraceRef",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Await as u16,
        name: "Await",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Spawn as u16,
        name: "Spawn",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Perform as u16,
        name: "Perform",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::HandlePush as u16,
        name: "HandlePush",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::HandlePop as u16,
        name: "HandlePop",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Resume as u16,
        name: "Resume",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    // List ops
    OpInfo {
        id: OpCode::Append as u16,
        name: "Append",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    // Stack allocations (optimization)
    OpInfo {
        id: OpCode::NewListStack as u16,
        name: "NewListStack",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::NewTupleStack as u16,
        name: "NewTupleStack",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    // Type checks
    OpInfo {
        id: OpCode::IsVariant as u16,
        name: "IsVariant",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: OpCode::Unbox as u16,
        name: "Unbox",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    // JIT optimization
    OpInfo {
        id: OpCode::OsrCheck as u16,
        name: "OsrCheck",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
];

/// Canonical IntrinsicId table with tier support flags.
///
/// Tier mapping matches JIT intrinsic lowering; intrinsics that require
/// external services remain interpreter-only.
pub const INTRINSIC_TABLE: &[OpInfo] = &[
    OpInfo {
        id: IntrinsicId::Length as u16,
        name: "Length",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Count as u16,
        name: "Count",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Matches as u16,
        name: "Matches",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Hash as u16,
        name: "Hash",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Diff as u16,
        name: "Diff",
        tiers: TIER_INTERP,
    },
    OpInfo {
        id: IntrinsicId::Patch as u16,
        name: "Patch",
        tiers: TIER_INTERP,
    },
    OpInfo {
        id: IntrinsicId::Redact as u16,
        name: "Redact",
        tiers: TIER_INTERP,
    },
    OpInfo {
        id: IntrinsicId::Validate as u16,
        name: "Validate",
        tiers: TIER_INTERP,
    },
    OpInfo {
        id: IntrinsicId::TraceRef as u16,
        name: "TraceRef",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Print as u16,
        name: "Print",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::ToString as u16,
        name: "ToString",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::ToInt as u16,
        name: "ToInt",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::ToFloat as u16,
        name: "ToFloat",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::TypeOf as u16,
        name: "TypeOf",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Keys as u16,
        name: "Keys",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Values as u16,
        name: "Values",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Contains as u16,
        name: "Contains",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Join as u16,
        name: "Join",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Split as u16,
        name: "Split",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Trim as u16,
        name: "Trim",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Upper as u16,
        name: "Upper",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Lower as u16,
        name: "Lower",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Replace as u16,
        name: "Replace",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Slice as u16,
        name: "Slice",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Append as u16,
        name: "Append",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Range as u16,
        name: "Range",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Abs as u16,
        name: "Abs",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Min as u16,
        name: "Min",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Max as u16,
        name: "Max",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Sort as u16,
        name: "Sort",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Reverse as u16,
        name: "Reverse",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Map as u16,
        name: "Map",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Filter as u16,
        name: "Filter",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Reduce as u16,
        name: "Reduce",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::FlatMap as u16,
        name: "FlatMap",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Zip as u16,
        name: "Zip",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Enumerate as u16,
        name: "Enumerate",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Any as u16,
        name: "Any",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::All as u16,
        name: "All",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Find as u16,
        name: "Find",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Position as u16,
        name: "Position",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::GroupBy as u16,
        name: "GroupBy",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Chunk as u16,
        name: "Chunk",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Window as u16,
        name: "Window",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Flatten as u16,
        name: "Flatten",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Unique as u16,
        name: "Unique",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Take as u16,
        name: "Take",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Drop as u16,
        name: "Drop",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::First as u16,
        name: "First",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Last as u16,
        name: "Last",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::IsEmpty as u16,
        name: "IsEmpty",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Chars as u16,
        name: "Chars",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::StartsWith as u16,
        name: "StartsWith",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::EndsWith as u16,
        name: "EndsWith",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::IndexOf as u16,
        name: "IndexOf",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::PadLeft as u16,
        name: "PadLeft",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::PadRight as u16,
        name: "PadRight",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Round as u16,
        name: "Round",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Ceil as u16,
        name: "Ceil",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Floor as u16,
        name: "Floor",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Sqrt as u16,
        name: "Sqrt",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Pow as u16,
        name: "Pow",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Log as u16,
        name: "Log",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Sin as u16,
        name: "Sin",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Cos as u16,
        name: "Cos",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Clamp as u16,
        name: "Clamp",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Clone as u16,
        name: "Clone",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Sizeof as u16,
        name: "Sizeof",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Debug as u16,
        name: "Debug",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::ToSet as u16,
        name: "ToSet",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::HasKey as u16,
        name: "HasKey",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Merge as u16,
        name: "Merge",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Size as u16,
        name: "Size",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Add as u16,
        name: "Add",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Remove as u16,
        name: "Remove",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Entries as u16,
        name: "Entries",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Compose as u16,
        name: "Compose",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Format as u16,
        name: "Format",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Partition as u16,
        name: "Partition",
        tiers: TIER_INTERP,
    },
    OpInfo {
        id: IntrinsicId::ReadDir as u16,
        name: "ReadDir",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::Exists as u16,
        name: "Exists",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::Mkdir as u16,
        name: "Mkdir",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::Eval as u16,
        name: "Eval",
        tiers: TIER_INTERP,
    },
    OpInfo {
        id: IntrinsicId::Guardrail as u16,
        name: "Guardrail",
        tiers: TIER_INTERP,
    },
    OpInfo {
        id: IntrinsicId::Pattern as u16,
        name: "Pattern",
        tiers: TIER_INTERP,
    },
    OpInfo {
        id: IntrinsicId::Exit as u16,
        name: "Exit",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::ReadLines as u16,
        name: "ReadLines",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::WalkDir as u16,
        name: "WalkDir",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::GlobMatch as u16,
        name: "GlobMatch",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::PathJoin as u16,
        name: "PathJoin",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::PathParent as u16,
        name: "PathParent",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::PathExtension as u16,
        name: "PathExtension",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::PathFilename as u16,
        name: "PathFilename",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::PathStem as u16,
        name: "PathStem",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Exec as u16,
        name: "Exec",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::ReadStdin as u16,
        name: "ReadStdin",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::Eprint as u16,
        name: "Eprint",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Eprintln as u16,
        name: "Eprintln",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::CsvParse as u16,
        name: "CsvParse",
        tiers: TIER_INTERP,
    },
    OpInfo {
        id: IntrinsicId::CsvEncode as u16,
        name: "CsvEncode",
        tiers: TIER_INTERP,
    },
    OpInfo {
        id: IntrinsicId::TomlParse as u16,
        name: "TomlParse",
        tiers: TIER_INTERP,
    },
    OpInfo {
        id: IntrinsicId::TomlEncode as u16,
        name: "TomlEncode",
        tiers: TIER_INTERP,
    },
    OpInfo {
        id: IntrinsicId::RegexMatch as u16,
        name: "RegexMatch",
        tiers: TIER_INTERP,
    },
    OpInfo {
        id: IntrinsicId::RegexReplace as u16,
        name: "RegexReplace",
        tiers: TIER_INTERP,
    },
    OpInfo {
        id: IntrinsicId::RegexFindAll as u16,
        name: "RegexFindAll",
        tiers: TIER_INTERP,
    },
    OpInfo {
        id: IntrinsicId::ReadLine as u16,
        name: "ReadLine",
        tiers: TIER_INTERP,
    },
    OpInfo {
        id: IntrinsicId::StringConcat as u16,
        name: "StringConcat",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::HttpGet as u16,
        name: "HttpGet",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::HttpPost as u16,
        name: "HttpPost",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::HttpPut as u16,
        name: "HttpPut",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::HttpDelete as u16,
        name: "HttpDelete",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::HttpRequest as u16,
        name: "HttpRequest",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::TcpConnect as u16,
        name: "TcpConnect",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::TcpListen as u16,
        name: "TcpListen",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::TcpSend as u16,
        name: "TcpSend",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::TcpRecv as u16,
        name: "TcpRecv",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::UdpBind as u16,
        name: "UdpBind",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::UdpSend as u16,
        name: "UdpSend",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::UdpRecv as u16,
        name: "UdpRecv",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::TcpClose as u16,
        name: "TcpClose",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::MapSortedKeys as u16,
        name: "MapSortedKeys",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::ParseInt as u16,
        name: "ParseInt",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::ParseFloat as u16,
        name: "ParseFloat",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Log2 as u16,
        name: "Log2",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Log10 as u16,
        name: "Log10",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::IsNan as u16,
        name: "IsNan",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::IsInfinite as u16,
        name: "IsInfinite",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::MathPi as u16,
        name: "MathPi",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::MathE as u16,
        name: "MathE",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::SortAsc as u16,
        name: "SortAsc",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::SortDesc as u16,
        name: "SortDesc",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::SortBy as u16,
        name: "SortBy",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::BinarySearch as u16,
        name: "BinarySearch",
        tiers: TIER_INTERP,
    },
    OpInfo {
        id: IntrinsicId::Hrtime as u16,
        name: "Hrtime",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::FormatTime as u16,
        name: "FormatTime",
        tiers: TIER_INTERP,
    },
    OpInfo {
        id: IntrinsicId::Args as u16,
        name: "Args",
        tiers: TIER_INTERP,
    },
    OpInfo {
        id: IntrinsicId::SetEnv as u16,
        name: "SetEnv",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::EnvVars as u16,
        name: "EnvVars",
        tiers: TIER_INTERP, // skip: requires external service
    },
    OpInfo {
        id: IntrinsicId::Tan as u16,
        name: "Tan",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
    OpInfo {
        id: IntrinsicId::Trunc as u16,
        name: "Trunc",
        tiers: TIER_INTERP | TIER_STENCIL | TIER_JIT,
    },
];
