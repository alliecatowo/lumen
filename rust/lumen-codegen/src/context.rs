//! Compilation context: ISA detection, module creation, and settings.

use std::sync::Arc;

use cranelift_codegen::isa::{self, TargetIsa};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_object::{ObjectBuilder, ObjectModule};
use target_lexicon::Triple;

use crate::emit::CodegenError;

/// Holds the Cranelift compilation state for a single codegen session.
pub struct CodegenContext {
    /// The target ISA (instruction set architecture).
    pub isa: Arc<dyn TargetIsa>,
    /// The object module being built.
    pub module: ObjectModule,
}

impl CodegenContext {
    /// Create a new codegen context targeting the host platform.
    pub fn new() -> Result<Self, CodegenError> {
        let triple = Triple::host();
        Self::new_with_triple(triple)
    }

    /// Create a new codegen context for cross-compilation to the given target triple string.
    pub fn new_with_target(triple_str: &str) -> Result<Self, CodegenError> {
        let triple: Triple = triple_str
            .parse()
            .map_err(|e| CodegenError::TargetError(format!("invalid target triple: {e}")))?;
        Self::new_with_triple(triple)
    }

    fn new_with_triple(triple: Triple) -> Result<Self, CodegenError> {
        let mut flag_builder = settings::builder();
        flag_builder
            .set("opt_level", "speed")
            .map_err(|e| CodegenError::TargetError(format!("failed to set opt_level: {e}")))?;

        let isa_builder = isa::lookup(triple.clone())
            .map_err(|e| CodegenError::TargetError(format!("unsupported target {triple}: {e}")))?;

        let flags = settings::Flags::new(flag_builder);
        let isa = isa_builder
            .finish(flags)
            .map_err(|e| CodegenError::TargetError(format!("failed to build ISA: {e}")))?;

        let obj_builder = ObjectBuilder::new(
            isa.clone(),
            "lumen_module",
            cranelift_module::default_libcall_names(),
        )
        .map_err(|e| CodegenError::TargetError(format!("failed to create ObjectBuilder: {e}")))?;

        let module = ObjectModule::new(obj_builder);

        Ok(Self { isa, module })
    }

    /// Return the pointer type for the current target (e.g. I64 on 64-bit).
    pub fn pointer_type(&self) -> cranelift_codegen::ir::Type {
        self.isa.pointer_type()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_context_creation() {
        let ctx = CodegenContext::new();
        assert!(ctx.is_ok(), "CodegenContext::new() should succeed on host");
        let ctx = ctx.unwrap();
        // Pointer type should be I64 on any 64-bit host.
        assert_eq!(
            ctx.pointer_type(),
            cranelift_codegen::ir::types::I64,
            "expected 64-bit pointer type on host"
        );
    }

    #[test]
    fn invalid_target_triple() {
        let result = CodegenContext::new_with_target("not-a-real-triple");
        assert!(result.is_err());
    }
}
