//! Object file emission.
//!
//! Converts the compiled Cranelift module into a native object file (.o).

use std::path::Path;

use cranelift_object::ObjectModule;
use thiserror::Error;

/// Errors that can occur during code generation.
#[derive(Debug, Error)]
pub enum CodegenError {
    #[error("target error: {0}")]
    TargetError(String),

    #[error("lowering error: {0}")]
    LoweringError(String),

    #[error("emission error: {0}")]
    EmissionError(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Finish the module and return the raw object file bytes.
pub fn emit_object(module: ObjectModule) -> Result<Vec<u8>, CodegenError> {
    let product = module.finish();
    let bytes = product
        .emit()
        .map_err(|e| CodegenError::EmissionError(format!("failed to emit object file: {e}")))?;
    Ok(bytes)
}

/// Finish the module and write the object file to disk.
pub fn emit_to_file(module: ObjectModule, path: &Path) -> Result<(), CodegenError> {
    let bytes = emit_object(module)?;
    std::fs::write(path, &bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aot::compile_object_module;
    use lumen_core::lir::{Constant, Instruction, LirCell, LirModule, OpCode};

    #[test]
    fn emit_simple_object() {
        let lir = LirModule {
            version: "1.0.0".to_string(),
            doc_hash: "test".to_string(),
            strings: Vec::new(),
            types: Vec::new(),
            cells: vec![LirCell {
                name: "answer".to_string(),
                params: Vec::new(),
                returns: Some("Int".to_string()),
                registers: 2,
                constants: vec![Constant::Int(42)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
                effect_handler_metas: Vec::new(),
            }],
            tools: Vec::new(),
            policies: Vec::new(),
            agents: Vec::new(),
            addons: Vec::new(),
            effects: Vec::new(),
            effect_binds: Vec::new(),
            handlers: Vec::new(),
        };

        let ptr_ty = cranelift_codegen::ir::types::I64;
        let module = compile_object_module(&lir, ptr_ty).expect("compilation should succeed");

        let bytes = emit_object(module).expect("emission should succeed");
        assert!(!bytes.is_empty(), "object file should not be empty");
        // ELF magic number (Linux) or Mach-O magic (macOS).
        // Just verify we got some bytes — the exact format depends on the host.
        assert!(bytes.len() > 16, "object file should have reasonable size");
    }
}
