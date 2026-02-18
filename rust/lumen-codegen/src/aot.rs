//! AOT (Ahead-Of-Time) compilation to native object files.
//!
//! Compiles LIR modules to native object code using Cranelift's ObjectModule.
//! Uses the unified `ir::lower_cell` implementation for lowering.

use std::collections::HashMap;

use cranelift_codegen::ir::{AbiParam, Type as ClifType};
use cranelift_codegen::Context;
use cranelift_frontend::FunctionBuilderContext;
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::ObjectModule;

use lumen_core::lir::LirModule;

use crate::emit::CodegenError;
use crate::types::lir_type_str_to_cl_type;

/// Result of compiling an entire LIR module.
pub struct CompiledModule {
    /// One entry per cell, in the same order as `LirModule::cells`.
    pub functions: Vec<CompiledFunction>,
}

pub struct CompiledFunction {
    pub name: String,
    pub func_id: FuncId,
}

/// Compile an entire LIR module into an ObjectModule ready for emission.
///
/// Each cell becomes a separate function. After this call the module is ready
/// to be finalized via `emit::emit_object`.
///
/// This function uses the unified `ir::lower_cell` implementation, ensuring
/// consistency with the JIT backend.
pub fn compile_object_module(
    lir: &LirModule,
    pointer_type: ClifType,
) -> Result<ObjectModule, CodegenError> {
    // Create target ISA and ObjectModule
    let isa_builder = cranelift_native::builder()
        .map_err(|e| CodegenError::TargetError(format!("failed to create ISA builder: {e}")))?;
    let isa = isa_builder
        .finish(cranelift_codegen::settings::Flags::new(
            cranelift_codegen::settings::builder(),
        ))
        .map_err(|e| CodegenError::TargetError(format!("failed to create ISA: {e}")))?;

    let obj_builder = cranelift_object::ObjectBuilder::new(
        isa,
        "lumen_module",
        cranelift_module::default_libcall_names(),
    )
    .map_err(|e| CodegenError::TargetError(format!("failed to create object builder: {e}")))?;

    let mut module = ObjectModule::new(obj_builder);

    // First pass: declare all cell signatures so we can resolve Call targets.
    let mut func_ids: HashMap<String, FuncId> = HashMap::new();
    for cell in &lir.cells {
        let mut sig = module.make_signature();
        for param in &cell.params {
            let param_ty = lir_type_str_to_cl_type(&param.ty, pointer_type);
            // Cranelift ABI requires I8 to be extended; use I64 for Bool params.
            let abi_ty = if param_ty == cranelift_codegen::ir::types::I8 {
                cranelift_codegen::ir::types::I64
            } else {
                param_ty
            };
            sig.params.push(AbiParam::new(abi_ty));
        }
        let ret_ty = cell
            .returns
            .as_deref()
            .map(|s| lir_type_str_to_cl_type(s, pointer_type))
            .unwrap_or(pointer_type);
        let abi_ret = if ret_ty == cranelift_codegen::ir::types::I8 {
            cranelift_codegen::ir::types::I64
        } else {
            ret_ty
        };
        sig.returns.push(AbiParam::new(abi_ret));
        let func_id = module
            .declare_function(&cell.name, Linkage::Export, &sig)
            .map_err(|e| {
                CodegenError::LoweringError(format!("declare_function({}): {e}", cell.name))
            })?;
        func_ids.insert(cell.name.clone(), func_id);
    }

    // Second pass: lower each cell body using ir::lower_cell.
    let mut fb_ctx = FunctionBuilderContext::new();
    for cell in &lir.cells {
        let func_id = func_ids[&cell.name];
        let mut ctx = Context::new();
        crate::ir::lower_cell(
            &mut ctx,
            &mut fb_ctx,
            cell,
            &mut module,
            pointer_type,
            func_id,
            &func_ids,
        )?;
    }

    Ok(module)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::emit_object;
    use lumen_core::lir::{Constant, Instruction, LirCell, LirModule, LirParam, OpCode};

    /// Build a minimal LirModule with a single cell for testing.
    fn make_module(
        name: &str,
        constants: Vec<Constant>,
        instructions: Vec<Instruction>,
    ) -> LirModule {
        make_module_with_params(name, Vec::new(), 4, constants, instructions)
    }

    fn make_module_with_params(
        name: &str,
        params: Vec<LirParam>,
        registers: u16,
        constants: Vec<Constant>,
        instructions: Vec<Instruction>,
    ) -> LirModule {
        LirModule {
            version: "1.0.0".to_string(),
            doc_hash: "test".to_string(),
            strings: Vec::new(),
            types: Vec::new(),
            cells: vec![LirCell {
                name: name.to_string(),
                params,
                returns: Some("Int".to_string()),
                registers,
                constants,
                instructions,
                effect_handler_metas: Vec::new(),
            }],
            tools: Vec::new(),
            policies: Vec::new(),
            agents: Vec::new(),
            addons: Vec::new(),
            effects: Vec::new(),
            effect_binds: Vec::new(),
            handlers: Vec::new(),
        }
    }

    fn make_multi_cell_module(cells: Vec<LirCell>) -> LirModule {
        LirModule {
            version: "1.0.0".to_string(),
            doc_hash: "test".to_string(),
            strings: Vec::new(),
            types: Vec::new(),
            cells,
            tools: Vec::new(),
            policies: Vec::new(),
            agents: Vec::new(),
            addons: Vec::new(),
            effects: Vec::new(),
            effect_binds: Vec::new(),
            handlers: Vec::new(),
        }
    }

    fn pointer_type() -> cranelift_codegen::ir::Type {
        cranelift_codegen::ir::types::I64
    }

    #[test]
    fn compile_load_const_add_return() {
        let lir = make_module(
            "add_two",
            vec![Constant::Int(10), Constant::Int(32)],
            vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Add, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
        );

        let ptr_ty = pointer_type();
        let module = compile_object_module(&lir, ptr_ty).expect("compilation should succeed");
        let bytes = emit_object(module).expect("emission should succeed");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn compile_bool_constants() {
        let lir = make_module(
            "bool_test",
            vec![],
            vec![
                Instruction::abc(OpCode::LoadBool, 0, 1, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
        );

        let ptr_ty = pointer_type();
        let module = compile_object_module(&lir, ptr_ty).expect("compilation should succeed");
        let bytes = emit_object(module).expect("emission should succeed");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn compile_comparison_ops() {
        let lir = make_module(
            "cmp_test",
            vec![Constant::Int(5), Constant::Int(10)],
            vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abx(OpCode::LoadK, 1, 1),
                Instruction::abc(OpCode::Eq, 2, 0, 1),
                Instruction::abc(OpCode::Lt, 3, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
        );

        let ptr_ty = pointer_type();
        let module = compile_object_module(&lir, ptr_ty).expect("compilation should succeed");
        let bytes = emit_object(module).expect("emission should succeed");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn compile_if_else() {
        let lir = make_module(
            "if_else_test",
            vec![],
            vec![
                Instruction::abc(OpCode::LoadBool, 0, 1, 0), // 0
                Instruction::abc(OpCode::LoadBool, 2, 1, 0), // 1
                Instruction::abc(OpCode::Eq, 3, 0, 2),       // 2
                Instruction::abc(OpCode::Test, 3, 0, 0),     // 3
                Instruction::sax(OpCode::Jmp, 2),            // 4 → 7
                Instruction::abc(OpCode::LoadInt, 1, 10, 0), // 5 (then)
                Instruction::sax(OpCode::Jmp, 1),            // 6 → 8
                Instruction::abc(OpCode::LoadInt, 1, 20, 0), // 7 (else)
                Instruction::abc(OpCode::Return, 1, 1, 0),   // 8
            ],
        );

        let ptr_ty = pointer_type();
        let module = compile_object_module(&lir, ptr_ty).expect("compilation should succeed");
        let bytes = emit_object(module).expect("emission should succeed");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn compile_while_loop() {
        let lir = make_module(
            "while_loop",
            vec![],
            vec![
                Instruction::abc(OpCode::LoadInt, 0, 0, 0), // 0
                Instruction::abc(OpCode::LoadInt, 1, 5, 0), // 1
                Instruction::abc(OpCode::LoadInt, 2, 1, 0), // 2
                Instruction::abc(OpCode::Lt, 3, 0, 1),      // 3
                Instruction::abc(OpCode::Test, 3, 0, 0),    // 4
                Instruction::sax(OpCode::Jmp, 2),           // 5 → 8
                Instruction::abc(OpCode::Add, 0, 0, 2),     // 6
                Instruction::sax(OpCode::Jmp, -5),          // 7 → 3
                Instruction::abc(OpCode::Return, 0, 1, 0),  // 8
            ],
        );

        let ptr_ty = pointer_type();
        let module = compile_object_module(&lir, ptr_ty).expect("compilation should succeed");
        let bytes = emit_object(module).expect("emission should succeed");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn compile_function_call() {
        let double_cell = LirCell {
            name: "double".to_string(),
            params: vec![LirParam {
                name: "x".to_string(),
                ty: "Int".to_string(),
                register: 0,
                variadic: false,
            }],
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![],
            instructions: vec![
                Instruction::abc(OpCode::Add, 1, 0, 0),
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };

        let main_cell = LirCell {
            name: "main".to_string(),
            params: vec![],
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![Constant::String("double".to_string()), Constant::Int(21)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0), // r0 = "double"
                Instruction::abx(OpCode::LoadK, 1, 1), // r1 = 21
                Instruction::abc(OpCode::Call, 0, 1, 1),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };

        let lir = make_multi_cell_module(vec![double_cell, main_cell]);

        let ptr_ty = pointer_type();
        let module = compile_object_module(&lir, ptr_ty).expect("compilation should succeed");
        let bytes = emit_object(module).expect("emission should succeed");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn compile_function_with_params() {
        let lir = make_module_with_params(
            "add_params",
            vec![
                LirParam {
                    name: "a".to_string(),
                    ty: "Int".to_string(),
                    register: 0,
                    variadic: false,
                },
                LirParam {
                    name: "b".to_string(),
                    ty: "Int".to_string(),
                    register: 1,
                    variadic: false,
                },
            ],
            4,
            vec![],
            vec![
                Instruction::abc(OpCode::Add, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
        );

        let ptr_ty = pointer_type();
        let module = compile_object_module(&lir, ptr_ty).expect("compilation should succeed");
        let bytes = emit_object(module).expect("emission should succeed");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn integration_compile_and_emit() {
        let source = "cell main() -> Int\n  1 + 2\nend\n";
        let lir = lumen_compiler::compile(source).expect("compilation should succeed");

        let ptr_ty = pointer_type();
        let module = compile_object_module(&lir, ptr_ty).expect("compilation should succeed");
        let bytes = emit_object(module).expect("emission should succeed");
        assert!(!bytes.is_empty(), "object file should not be empty");
        assert!(bytes.len() > 16, "object file should have reasonable size");
    }

    #[test]
    fn integration_if_else_from_compiler() {
        let source = r#"
cell choose(x: Int) -> Int
  if x > 0
    100
  else
    200
  end
end
"#;
        let lir = lumen_compiler::compile(source).expect("compilation should succeed");

        let ptr_ty = pointer_type();
        let module = compile_object_module(&lir, ptr_ty).expect("compilation should succeed");
        let bytes = emit_object(module).expect("emission should succeed");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn integration_function_call_from_compiler() {
        let source = r#"
cell double(x: Int) -> Int
  x + x
end

cell main() -> Int
  double(21)
end
"#;
        let lir = lumen_compiler::compile(source).expect("compilation should succeed");

        let ptr_ty = pointer_type();
        let module = compile_object_module(&lir, ptr_ty).expect("compilation should succeed");
        let bytes = emit_object(module).expect("emission should succeed");
        assert!(!bytes.is_empty());
    }
}
