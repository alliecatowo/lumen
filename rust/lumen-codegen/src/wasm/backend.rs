//! WASM code generation from LIR modules using wasm-encoder.
//!
//! Produces WebAssembly binary (`.wasm`) files directly from Lumen LIR.
//! This module uses the `wasm-encoder` crate to build valid WASM modules.
//!
//! ## Architecture
//!
//! [`WasmCodegen`] walks the cells in an [`LirModule`] and emits a valid wasm
//! module with:
//!
//! - **Type section** — one function signature per unique cell signature.
//! - **Function section** — maps each cell to its type index.
//! - **Memory section** — single linear memory.
//! - **Export section** — exports all cells by name (the `main` cell is always
//!   exported if present).
//! - **Code section** — wasm bytecode for each cell body, translated from LIR
//!   opcodes.
//!
//! The public entry point is [`compile_to_wasm`].

use lumen_core::lir::{LirCell, LirModule};
use wasm_encoder::{
    CodeSection, ExportKind, ExportSection, Function, FunctionSection, MemorySection, MemoryType,
    Module, TypeSection, ValType,
};

use super::control;
use crate::emit::CodegenError;

// ---------------------------------------------------------------------------
// Target enum
// ---------------------------------------------------------------------------

/// WebAssembly compilation target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmTarget {
    /// WASI — for server-side / CLI runtimes (Wasmtime, Wasmer, etc.).
    /// Triple: `wasm32-wasi`.
    Wasm32Wasi,
    /// Browser / unknown environment — pure wasm with no WASI imports.
    /// Triple: `wasm32-unknown-unknown`.
    Wasm32Unknown,
}

impl WasmTarget {
    /// Return the target-lexicon triple string for this target.
    pub fn triple_str(&self) -> &'static str {
        match self {
            WasmTarget::Wasm32Wasi => "wasm32-wasi",
            WasmTarget::Wasm32Unknown => "wasm32-unknown-unknown",
        }
    }
}

impl std::fmt::Display for WasmTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.triple_str())
    }
}

// ---------------------------------------------------------------------------
// WasmCodegen — the encoder
// ---------------------------------------------------------------------------

/// Compiles an [`LirModule`] to a WebAssembly binary.
///
/// This struct accumulates section data and produces a valid `.wasm` file
/// conforming to the WebAssembly 1.0 binary format.
pub struct WasmCodegen {
    target: WasmTarget,
}

impl WasmCodegen {
    /// Create a new WASM codegen instance for the given target.
    pub fn new(target: WasmTarget) -> Self {
        Self { target }
    }

    /// Return the target this codegen is configured for.
    pub fn target(&self) -> WasmTarget {
        self.target
    }

    /// Compile an LIR module to WASM binary bytes.
    pub fn compile(&self, lir: &LirModule) -> Result<Vec<u8>, CodegenError> {
        compile_to_wasm(lir, self.target)
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Compile an LIR module to a WebAssembly binary using wasm-encoder.
///
/// Returns the raw `.wasm` bytes. The `target` selects whether WASI imports
/// are assumed (currently informational — no WASI imports are emitted yet).
pub fn compile_to_wasm(lir: &LirModule, _target: WasmTarget) -> Result<Vec<u8>, CodegenError> {
    if lir.cells.is_empty() {
        return Err(CodegenError::LoweringError(
            "cannot compile empty module to wasm".to_string(),
        ));
    }

    let mut module = Module::new();

    // ---- 1. Type section (id=1) ------------------------------------------
    // Collect unique signatures: (param_count, has_return).
    let sigs: Vec<CellSig> = lir.cells.iter().map(CellSig::from_cell).collect();
    let unique_sigs = deduplicate_sigs(&sigs);

    let mut types = TypeSection::new();
    for &(param_count, has_return) in &unique_sigs {
        // All params are i64
        let params = vec![ValType::I64; param_count];
        let results = if has_return {
            vec![ValType::I64]
        } else {
            vec![]
        };
        types.ty().function(params, results);
    }
    module.section(&types);

    // ---- 3. Function section (id=3) --------------------------------------
    // Map each cell to the index of its signature in the unique list.
    let mut functions = FunctionSection::new();
    for sig in &sigs {
        let key = sig.sig_key();
        let type_idx = unique_sigs.iter().position(|s| *s == key).unwrap_or(0);
        functions.function(type_idx as u32);
    }
    module.section(&functions);

    // ---- 5. Memory section (id=5) ----------------------------------------
    // Define a single linear memory with initial size of 1 page (64KB).
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: 1,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memories);

    // ---- 7. Export section (id=7) ----------------------------------------
    let mut exports = ExportSection::new();
    for (i, cell) in lir.cells.iter().enumerate() {
        exports.export(&cell.name, ExportKind::Func, i as u32);
    }
    // Export memory as "memory"
    exports.export("memory", ExportKind::Memory, 0);
    module.section(&exports);

    // ---- 10. Code section (id=10) ----------------------------------------
    let mut code = CodeSection::new();
    for cell in &lir.cells {
        let func = encode_function_body(cell, lir)?;
        code.function(&func);
    }
    module.section(&code);

    Ok(module.finish())
}

// ---------------------------------------------------------------------------
// Signature helpers
// ---------------------------------------------------------------------------

/// Simplified cell signature: (number of params, has i64 return).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CellSig {
    param_count: usize,
    has_return: bool,
    /// Return type hint from LIR (used for wasm type mapping).
    return_type_str: Option<String>,
}

impl CellSig {
    fn from_cell(cell: &LirCell) -> Self {
        Self {
            param_count: cell.params.len(),
            has_return: cell.returns.is_some(),
            return_type_str: cell.returns.clone(),
        }
    }

    fn sig_key(&self) -> (usize, bool) {
        (self.param_count, self.has_return)
    }
}

fn deduplicate_sigs(sigs: &[CellSig]) -> Vec<(usize, bool)> {
    let mut unique: Vec<(usize, bool)> = Vec::new();
    for sig in sigs {
        let key = sig.sig_key();
        if !unique.contains(&key) {
            unique.push(key);
        }
    }
    unique
}

// ---------------------------------------------------------------------------
// Function body encoding
// ---------------------------------------------------------------------------

/// Encode a single function body from LIR instructions.
///
/// The body consists of:
///   local declarations (registers beyond params)
///   instruction bytecodes
///   end
fn encode_function_body(cell: &LirCell, lir: &LirModule) -> Result<Function, CodegenError> {
    // Local declarations: we need (registers - params) additional locals, all i64.
    // Plus one extra local for the PC (program counter) used in switch-loop.
    let num_params = cell.params.len();
    let num_regs = (cell.registers as usize).max(num_params);
    let extra_locals = num_regs.saturating_sub(num_params);

    // Build local declarations vector: registers + $pc (i32)
    let mut locals = vec![];
    if extra_locals > 0 {
        locals.push((extra_locals as u32, ValType::I64));
    }
    // Add PC local (i32 for instruction index)
    locals.push((1, ValType::I32));

    let mut func = Function::new(locals);
    let pc_local = num_regs as u32; // PC is the last local

    // Use control flow module to emit structured control flow
    control::emit_function_with_control_flow(&mut func, cell, lir, num_params, pc_local)?;

    Ok(func)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use lumen_core::lir::{Constant, Instruction, LirCell, LirModule, LirParam, OpCode};

    fn empty_lir_module(cells: Vec<LirCell>) -> LirModule {
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

    fn simple_add_cell() -> LirCell {
        LirCell {
            name: "add".to_string(),
            params: vec![
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
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![],
            instructions: vec![
                Instruction::abc(OpCode::Add, 2, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }
    }

    fn const_cell() -> LirCell {
        LirCell {
            name: "answer".to_string(),
            params: vec![],
            returns: Some("Int".to_string()),
            registers: 2,
            constants: vec![Constant::Int(42)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }
    }

    // -- T097 tests --------------------------------------------------------

    #[test]
    fn wasm_target_triple_strings() {
        assert_eq!(WasmTarget::Wasm32Wasi.triple_str(), "wasm32-wasi");
        assert_eq!(
            WasmTarget::Wasm32Unknown.triple_str(),
            "wasm32-unknown-unknown"
        );
    }

    #[test]
    fn wasm_target_display() {
        assert_eq!(format!("{}", WasmTarget::Wasm32Wasi), "wasm32-wasi");
        assert_eq!(
            format!("{}", WasmTarget::Wasm32Unknown),
            "wasm32-unknown-unknown"
        );
    }

    #[test]
    fn compile_empty_module_fails() {
        let lir = empty_lir_module(vec![]);
        let result = compile_to_wasm(&lir, WasmTarget::Wasm32Unknown);
        assert!(result.is_err(), "empty module should fail");
    }

    #[test]
    fn compile_simple_add() {
        let lir = empty_lir_module(vec![simple_add_cell()]);
        let bytes =
            compile_to_wasm(&lir, WasmTarget::Wasm32Unknown).expect("simple add should compile");

        // Verify wasm magic: \0asm
        assert_eq!(&bytes[0..4], b"\0asm", "should start with wasm magic");
        // Verify version: 1
        assert_eq!(
            u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            1,
            "wasm version should be 1"
        );
        assert!(bytes.len() > 8, "should have section data beyond header");
    }

    #[test]
    fn compile_const_return() {
        let lir = empty_lir_module(vec![const_cell()]);
        let bytes =
            compile_to_wasm(&lir, WasmTarget::Wasm32Wasi).expect("const cell should compile");
        assert_eq!(&bytes[0..4], b"\0asm");
        assert!(bytes.len() > 16);
    }

    #[test]
    fn compile_multi_cell_module() {
        let lir = empty_lir_module(vec![simple_add_cell(), const_cell()]);
        let bytes = compile_to_wasm(&lir, WasmTarget::Wasm32Unknown)
            .expect("multi-cell module should compile");
        assert_eq!(&bytes[0..4], b"\0asm");
        assert!(bytes.len() > 20);
    }

    #[test]
    fn compile_arithmetic_ops() {
        let cell = LirCell {
            name: "arith".to_string(),
            params: vec![
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
            returns: Some("Int".to_string()),
            registers: 8,
            constants: vec![],
            instructions: vec![
                Instruction::abc(OpCode::Add, 2, 0, 1),
                Instruction::abc(OpCode::Sub, 3, 0, 1),
                Instruction::abc(OpCode::Mul, 4, 2, 3),
                Instruction::abc(OpCode::Div, 5, 4, 0),
                Instruction::abc(OpCode::Mod, 6, 5, 1),
                Instruction::abc(OpCode::Neg, 7, 6, 0),
                Instruction::abc(OpCode::Return, 7, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };
        let lir = empty_lir_module(vec![cell]);
        let bytes =
            compile_to_wasm(&lir, WasmTarget::Wasm32Unknown).expect("arithmetic should compile");
        assert_eq!(&bytes[0..4], b"\0asm");
    }

    #[test]
    fn compile_bitwise_ops() {
        let cell = LirCell {
            name: "bits".to_string(),
            params: vec![
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
            returns: Some("Int".to_string()),
            registers: 7,
            constants: vec![],
            instructions: vec![
                Instruction::abc(OpCode::BitAnd, 2, 0, 1),
                Instruction::abc(OpCode::BitOr, 3, 0, 1),
                Instruction::abc(OpCode::BitXor, 4, 0, 1),
                Instruction::abc(OpCode::Shl, 5, 0, 1),
                Instruction::abc(OpCode::Shr, 6, 0, 1),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };
        let lir = empty_lir_module(vec![cell]);
        let bytes =
            compile_to_wasm(&lir, WasmTarget::Wasm32Unknown).expect("bitwise should compile");
        assert_eq!(&bytes[0..4], b"\0asm");
    }

    #[test]
    fn compile_comparison_ops() {
        let cell = LirCell {
            name: "cmp".to_string(),
            params: vec![
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
            returns: Some("Int".to_string()),
            registers: 6,
            constants: vec![],
            instructions: vec![
                Instruction::abc(OpCode::Eq, 2, 0, 1),
                Instruction::abc(OpCode::Lt, 3, 0, 1),
                Instruction::abc(OpCode::Le, 4, 0, 1),
                Instruction::abc(OpCode::Not, 5, 2, 0),
                Instruction::abc(OpCode::Return, 2, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };
        let lir = empty_lir_module(vec![cell]);
        let bytes =
            compile_to_wasm(&lir, WasmTarget::Wasm32Unknown).expect("comparison should compile");
        assert_eq!(&bytes[0..4], b"\0asm");
    }

    #[test]
    fn wasm_codegen_struct_api() {
        let codegen = WasmCodegen::new(WasmTarget::Wasm32Wasi);
        assert_eq!(codegen.target(), WasmTarget::Wasm32Wasi);

        let lir = empty_lir_module(vec![const_cell()]);
        let bytes = codegen.compile(&lir).expect("struct API should work");
        assert_eq!(&bytes[0..4], b"\0asm");
    }

    #[test]
    fn compile_with_float_constant() {
        let cell = LirCell {
            name: "pi".to_string(),
            params: vec![],
            returns: Some("Float".to_string()),
            registers: 2,
            constants: vec![Constant::Float(3.14159)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };
        let lir = empty_lir_module(vec![cell]);
        let bytes = compile_to_wasm(&lir, WasmTarget::Wasm32Unknown)
            .expect("float constant should compile");
        assert_eq!(&bytes[0..4], b"\0asm");
    }

    #[test]
    fn compile_simple_loop() {
        // Simple loop: sum from 0 to n
        // while i < n:
        //   sum = sum + i
        //   i = i + 1
        // Uses: LoadInt, Lt, Test, Jmp, Add, Return
        let cell = LirCell {
            name: "sum_loop".to_string(),
            params: vec![LirParam {
                name: "n".to_string(),
                ty: "Int".to_string(),
                register: 0,
                variadic: false,
            }],
            returns: Some("Int".to_string()),
            registers: 4,
            constants: vec![],
            instructions: vec![
                // r1 = 0 (sum)
                Instruction::abx(OpCode::LoadInt, 1, 0),
                // r2 = 0 (i)
                Instruction::abx(OpCode::LoadInt, 2, 0),
                // loop_start (idx 2):
                // r3 = (r2 < r0)
                Instruction::abc(OpCode::Lt, 3, 2, 0),
                // Test r3, 0 (if false, skip next)
                Instruction::abc(OpCode::Test, 3, 0, 0),
                // Jmp +4 (exit loop if false)
                Instruction::sax(OpCode::Jmp, 4),
                // r1 = r1 + r2
                Instruction::abc(OpCode::Add, 1, 1, 2),
                // r2 = r2 + 1
                Instruction::abx(OpCode::LoadInt, 3, 1),
                Instruction::abc(OpCode::Add, 2, 2, 3),
                // Jmp -6 (back to loop_start)
                Instruction::sax(OpCode::Jmp, -6),
                // Return r1
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };
        let lir = empty_lir_module(vec![cell]);
        let bytes = compile_to_wasm(&lir, WasmTarget::Wasm32Unknown).expect("loop should compile");
        assert_eq!(&bytes[0..4], b"\0asm");
        // Verify it's a valid wasm module (has sections beyond header)
        assert!(bytes.len() > 100, "compiled module should have content");
    }

    #[test]
    fn compile_conditional_branch() {
        // Simple if-then-else
        // if a > b then a else b
        let cell = LirCell {
            name: "max".to_string(),
            params: vec![
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
            returns: Some("Int".to_string()),
            registers: 3,
            constants: vec![],
            instructions: vec![
                // r2 = (a > b)
                Instruction::abc(OpCode::Lt, 2, 1, 0), // b < a
                // Test r2, 0 (if false, skip next)
                Instruction::abc(OpCode::Test, 2, 0, 0),
                // Jmp +2 (skip return a)
                Instruction::sax(OpCode::Jmp, 2),
                // Return a
                Instruction::abc(OpCode::Return, 0, 1, 0),
                // Return b
                Instruction::abc(OpCode::Return, 1, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        };
        let lir = empty_lir_module(vec![cell]);
        let bytes =
            compile_to_wasm(&lir, WasmTarget::Wasm32Unknown).expect("conditional should compile");
        assert_eq!(&bytes[0..4], b"\0asm");
    }
}
