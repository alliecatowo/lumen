//! Shared opcode capability metadata for Tier 1 (stencil) and Tier 2 (JIT).

use crate::lir::OpCode;

/// Tier 2 JIT handling mode for an opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier2CapabilityFlag {
    /// Lowered directly to Cranelift IR.
    Native,
    /// Lowered as a runtime-helper call from generated Cranelift IR.
    RuntimeCall,
    /// Reserved for future gaps where Tier 2 has no implementation.
    Unsupported,
}

/// Tier 1 runtime helper category for stencils with `RuntimeFuncAddr` holes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier1RuntimeHelper {
    /// No runtime helper is needed for this opcode's stencil.
    None,
    /// Generic dispatcher helper (`lm_rt_stencil_runtime`).
    StencilRuntime,
    /// Dedicated return helper.
    Return,
    /// Dedicated halt helper.
    Halt,
    /// Dedicated call helper.
    Call,
    /// Dedicated tail-call helper.
    TailCall,
    /// Dedicated intrinsic helper.
    Intrinsic,
    /// Dedicated effect perform helper.
    Perform,
    /// Dedicated effect handler push helper.
    HandlePush,
    /// Dedicated effect handler pop helper.
    HandlePop,
    /// Dedicated effect resume helper.
    Resume,
    /// Dedicated OSR safepoint helper.
    OsrCheck,
}

impl Tier1RuntimeHelper {
    /// Returns `true` when this opcode needs a runtime helper patch in Tier 1.
    pub const fn required(self) -> bool {
        !matches!(self, Self::None)
    }

    /// Returns the runtime symbol name used to patch Tier 1 helper holes.
    pub const fn symbol(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::StencilRuntime => Some("lm_rt_stencil_runtime"),
            Self::Return => Some("lm_rt_return"),
            Self::Halt => Some("lm_rt_halt"),
            Self::Call => Some("lm_rt_call"),
            Self::TailCall => Some("lm_rt_tailcall"),
            Self::Intrinsic => Some("lm_rt_intrinsic"),
            Self::Perform => Some("lm_rt_perform"),
            Self::HandlePush => Some("lm_rt_handle_push"),
            Self::HandlePop => Some("lm_rt_handle_pop"),
            Self::Resume => Some("lm_rt_resume"),
            Self::OsrCheck => Some("lm_rt_osr_check"),
        }
    }
}

/// Shared per-opcode metadata used by tiered execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpCodeCapability {
    /// Whether Tier 1 should attempt stencil compilation for this opcode.
    pub tier1_supported: bool,
    /// Whether this opcode is safe in Tier 1 multi-step cells.
    pub tier1_multi_step_safe: bool,
    /// Runtime helper requirements for Tier 1 patching.
    pub tier1_runtime_helper: Tier1RuntimeHelper,
    /// Tier 2 JIT handling mode.
    pub tier2: Tier2CapabilityFlag,
}

const fn cap(
    tier1_supported: bool,
    tier1_multi_step_safe: bool,
    tier1_runtime_helper: Tier1RuntimeHelper,
    tier2: Tier2CapabilityFlag,
) -> OpCodeCapability {
    OpCodeCapability {
        tier1_supported,
        tier1_multi_step_safe,
        tier1_runtime_helper,
        tier2,
    }
}

/// Returns shared capability metadata for one opcode.
pub const fn opcode_capability(op: OpCode) -> OpCodeCapability {
    match op {
        // Misc
        OpCode::Nop => cap(
            true,
            false,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),

        // Loads / moves
        OpCode::LoadK => cap(
            true,
            true,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::LoadNil => cap(
            true,
            true,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::LoadBool => cap(
            true,
            true,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::LoadInt => cap(
            true,
            true,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Move => cap(
            true,
            false,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::MoveOwn => cap(
            true,
            false,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),

        // Collection construction / access
        OpCode::NewList => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::NewMap => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::NewRecord => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::NewUnion => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::NewTuple => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::NewSet => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::NewListStack => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::NewTupleStack => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::GetField => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::SetField => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::GetIndex => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::SetIndex => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::GetTuple => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::Append => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::IsVariant => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Unbox => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),

        // Arithmetic / bitwise
        OpCode::Add => cap(
            true,
            true,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Sub => cap(
            true,
            true,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Mul => cap(
            true,
            true,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Div => cap(
            true,
            true,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Mod => cap(
            true,
            true,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Pow => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Neg => cap(
            true,
            true,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Concat => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::FloorDiv => cap(
            true,
            true,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::BitOr => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::BitAnd => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::BitXor => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::BitNot => cap(
            true,
            false,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Shl => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Shr => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::Native,
        ),

        // Comparison / logic
        OpCode::Eq => cap(
            true,
            false,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Lt => cap(
            true,
            false,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Le => cap(
            true,
            false,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Not => cap(
            true,
            false,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::And => cap(
            true,
            false,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Or => cap(
            true,
            false,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::In => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::Is => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::NullCo => cap(
            true,
            false,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Test => cap(
            true,
            false,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),

        // Control flow
        OpCode::Jmp => cap(
            true,
            false,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Call => cap(
            true,
            true,
            Tier1RuntimeHelper::Call,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::TailCall => cap(
            true,
            true,
            Tier1RuntimeHelper::TailCall,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Return => cap(
            true,
            true,
            Tier1RuntimeHelper::Return,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Halt => cap(
            true,
            true,
            Tier1RuntimeHelper::Halt,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Loop => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::ForPrep => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::ForLoop => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::ForIn => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::Break => cap(
            true,
            false,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),
        OpCode::Continue => cap(
            true,
            false,
            Tier1RuntimeHelper::None,
            Tier2CapabilityFlag::Native,
        ),

        // Intrinsic / closures
        OpCode::Intrinsic => cap(
            true,
            false,
            Tier1RuntimeHelper::Intrinsic,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::Closure => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::GetUpval => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::SetUpval => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),

        // Effects / tools / async
        OpCode::ToolCall => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::Schema => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::Emit => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::TraceRef => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::Await => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::Spawn => cap(
            true,
            false,
            Tier1RuntimeHelper::StencilRuntime,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::Perform => cap(
            true,
            false,
            Tier1RuntimeHelper::Perform,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::HandlePush => cap(
            true,
            false,
            Tier1RuntimeHelper::HandlePush,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::HandlePop => cap(
            true,
            false,
            Tier1RuntimeHelper::HandlePop,
            Tier2CapabilityFlag::RuntimeCall,
        ),
        OpCode::Resume => cap(
            true,
            false,
            Tier1RuntimeHelper::Resume,
            Tier2CapabilityFlag::RuntimeCall,
        ),

        // JIT optimization
        OpCode::OsrCheck => cap(
            true,
            false,
            Tier1RuntimeHelper::OsrCheck,
            Tier2CapabilityFlag::RuntimeCall,
        ),
    }
}

/// Returns shared capability metadata for all opcodes.
pub fn opcode_capabilities() -> Vec<(OpCode, OpCodeCapability)> {
    use strum::IntoEnumIterator;
    OpCode::iter()
        .map(|op| (op, opcode_capability(op)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::{EnumCount, IntoEnumIterator};

    #[test]
    fn metadata_covers_all_opcodes() {
        let caps = opcode_capabilities();
        assert_eq!(
            caps.len(),
            OpCode::COUNT,
            "opcode_capabilities() must cover all {} opcodes, got {}",
            OpCode::COUNT,
            caps.len()
        );

        let seen: std::collections::HashSet<OpCode> = caps.iter().map(|(op, _)| *op).collect();
        for op in OpCode::iter() {
            assert!(seen.contains(&op), "missing metadata for OpCode::{op:?}");
        }
    }

    #[test]
    fn tier1_support_matches_call_guard_policy() {
        assert!(opcode_capability(OpCode::Call).tier1_supported);
        assert!(opcode_capability(OpCode::TailCall).tier1_supported);
        assert!(opcode_capability(OpCode::Call).tier1_multi_step_safe);
        assert!(opcode_capability(OpCode::TailCall).tier1_multi_step_safe);
        assert!(opcode_capability(OpCode::Add).tier1_supported);
    }

    #[test]
    fn runtime_helper_symbol_is_present_when_required() {
        for op in OpCode::iter() {
            let helper = opcode_capability(op).tier1_runtime_helper;
            if helper.required() {
                assert!(
                    helper.symbol().is_some(),
                    "OpCode::{op:?} requires a helper symbol"
                );
            }
        }
    }
}
