//! WASM code generation from LIR modules.
//!
//! Produces WebAssembly binary (`.wasm`) files directly from Lumen LIR,
//! bypassing Cranelift's native-target pipeline. Cranelift does not support
//! wasm32 as an *output* target (it consumes wasm, not produces it), so this
//! module encodes the wasm binary format directly using a lightweight encoder.
//!
//! ## Architecture
//!
//! [`WasmCodegen`] walks the cells in an [`LirModule`] and emits a valid wasm
//! module with:
//!
//! - **Type section** — one function signature per unique cell signature.
//! - **Function section** — maps each cell to its type index.
//! - **Export section** — exports all cells by name (the `main` cell is always
//!   exported if present).
//! - **Code section** — wasm bytecode for each cell body, translated from LIR
//!   opcodes.
//!
//! The public entry point is [`compile_to_wasm`].

use lumen_compiler::compiler::lir::{Constant, LirCell, LirModule, OpCode};

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

/// Compile an LIR module to a WebAssembly binary.
///
/// Returns the raw `.wasm` bytes. The `target` selects whether WASI imports
/// are assumed (currently informational — no WASI imports are emitted yet).
pub fn compile_to_wasm(lir: &LirModule, _target: WasmTarget) -> Result<Vec<u8>, CodegenError> {
    if lir.cells.is_empty() {
        return Err(CodegenError::LoweringError(
            "cannot compile empty module to wasm".to_string(),
        ));
    }

    let mut wasm = Vec::new();

    // ---- Header ----------------------------------------------------------
    // Magic number: \0asm
    wasm.extend_from_slice(b"\0asm");
    // Version: 1
    wasm.extend_from_slice(&1u32.to_le_bytes());

    // ---- 1. Type section (id=1) ------------------------------------------
    // Collect unique signatures: (param_count, has_return).
    let sigs: Vec<CellSig> = lir.cells.iter().map(CellSig::from_cell).collect();
    let unique_sigs = deduplicate_sigs(&sigs);

    let type_section = encode_type_section(&unique_sigs);
    emit_section(&mut wasm, 1, &type_section);

    // ---- 3. Function section (id=3) --------------------------------------
    // Map each cell to the index of its signature in the unique list.
    let func_section = encode_function_section(&sigs, &unique_sigs);
    emit_section(&mut wasm, 3, &func_section);

    // ---- 7. Export section (id=7) ----------------------------------------
    let export_section = encode_export_section(&lir.cells);
    emit_section(&mut wasm, 7, &export_section);

    // ---- 10. Code section (id=10) ----------------------------------------
    let code_section = encode_code_section(lir)?;
    emit_section(&mut wasm, 10, &code_section);

    Ok(wasm)
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
// Section encoders
// ---------------------------------------------------------------------------

/// Encode the type section: a vector of function types.
///
/// Each function type is encoded as:
///   0x60  (func type indicator)
///   vec(param types)  — all i64 for now
///   vec(return types) — 0 or 1 i64
fn encode_type_section(unique_sigs: &[(usize, bool)]) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_u32_leb128(&mut buf, unique_sigs.len() as u32);

    for &(param_count, has_return) in unique_sigs {
        buf.push(0x60); // functype
                        // Params: all i64 (0x7E)
        encode_u32_leb128(&mut buf, param_count as u32);
        buf.extend(std::iter::repeat_n(0x7E_u8, param_count)); // i64
                                                               // Returns
        if has_return {
            encode_u32_leb128(&mut buf, 1);
            buf.push(0x7E); // i64
        } else {
            encode_u32_leb128(&mut buf, 0);
        }
    }

    buf
}

/// Encode the function section: a vector of type indices.
fn encode_function_section(sigs: &[CellSig], unique_sigs: &[(usize, bool)]) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_u32_leb128(&mut buf, sigs.len() as u32);

    for sig in sigs {
        let key = sig.sig_key();
        let type_idx = unique_sigs.iter().position(|s| *s == key).unwrap_or(0);
        encode_u32_leb128(&mut buf, type_idx as u32);
    }

    buf
}

/// Encode the export section: export every cell as a function.
fn encode_export_section(cells: &[LirCell]) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_u32_leb128(&mut buf, cells.len() as u32);

    for (i, cell) in cells.iter().enumerate() {
        // Export name
        let name_bytes = cell.name.as_bytes();
        encode_u32_leb128(&mut buf, name_bytes.len() as u32);
        buf.extend_from_slice(name_bytes);
        // Export kind: function = 0x00
        buf.push(0x00);
        // Function index
        encode_u32_leb128(&mut buf, i as u32);
    }

    buf
}

/// Encode the code section: function bodies.
fn encode_code_section(lir: &LirModule) -> Result<Vec<u8>, CodegenError> {
    let mut section_buf = Vec::new();
    encode_u32_leb128(&mut section_buf, lir.cells.len() as u32);

    for cell in &lir.cells {
        let body = encode_function_body(cell, lir)?;
        encode_u32_leb128(&mut section_buf, body.len() as u32);
        section_buf.extend_from_slice(&body);
    }

    Ok(section_buf)
}

/// Encode a single function body from LIR instructions.
///
/// The body consists of:
///   local declarations (registers beyond params)
///   instruction bytecodes
///   0x0B (end)
fn encode_function_body(cell: &LirCell, _lir: &LirModule) -> Result<Vec<u8>, CodegenError> {
    let mut buf = Vec::new();

    // Local declarations: we need (registers - params) additional locals, all i64.
    let num_params = cell.params.len();
    let num_regs = (cell.registers as usize).max(num_params);
    let extra_locals = num_regs.saturating_sub(num_params);

    if extra_locals > 0 {
        // One local declaration: count of i64 locals.
        encode_u32_leb128(&mut buf, 1); // 1 group of locals
        encode_u32_leb128(&mut buf, extra_locals as u32);
        buf.push(0x7E); // i64
    } else {
        encode_u32_leb128(&mut buf, 0); // 0 local declaration groups
    }

    // Translate LIR instructions to wasm opcodes.
    for inst in &cell.instructions {
        match inst.op {
            OpCode::LoadK => {
                let a = inst.a;
                let bx = inst.bx() as usize;
                emit_wasm_load_constant(&mut buf, cell, bx, a)?;
            }
            OpCode::LoadInt => {
                let a = inst.a;
                let imm = inst.b as i8 as i64;
                // i64.const imm
                buf.push(0x42);
                encode_i64_leb128(&mut buf, imm);
                emit_local_set(&mut buf, a as u32);
            }
            OpCode::LoadBool => {
                let a = inst.a;
                let b_val = inst.b;
                buf.push(0x42); // i64.const
                encode_i64_leb128(&mut buf, b_val as i64);
                emit_local_set(&mut buf, a as u32);
            }
            OpCode::LoadNil => {
                let a = inst.a;
                let count = inst.b as usize;
                for i in 0..=count {
                    let r = a as usize + i;
                    buf.push(0x42); // i64.const
                    encode_i64_leb128(&mut buf, 0);
                    emit_local_set(&mut buf, r as u32);
                }
            }
            OpCode::Move => {
                emit_local_get(&mut buf, inst.b as u32);
                emit_local_set(&mut buf, inst.a as u32);
            }
            // Arithmetic
            OpCode::Add => {
                emit_local_get(&mut buf, inst.b as u32);
                emit_local_get(&mut buf, inst.c as u32);
                buf.push(0x7C); // i64.add
                emit_local_set(&mut buf, inst.a as u32);
            }
            OpCode::Sub => {
                emit_local_get(&mut buf, inst.b as u32);
                emit_local_get(&mut buf, inst.c as u32);
                buf.push(0x7D); // i64.sub
                emit_local_set(&mut buf, inst.a as u32);
            }
            OpCode::Mul => {
                emit_local_get(&mut buf, inst.b as u32);
                emit_local_get(&mut buf, inst.c as u32);
                buf.push(0x7E); // i64.mul
                emit_local_set(&mut buf, inst.a as u32);
            }
            OpCode::Div => {
                emit_local_get(&mut buf, inst.b as u32);
                emit_local_get(&mut buf, inst.c as u32);
                buf.push(0x7F); // i64.div_s
                emit_local_set(&mut buf, inst.a as u32);
            }
            OpCode::Mod => {
                emit_local_get(&mut buf, inst.b as u32);
                emit_local_get(&mut buf, inst.c as u32);
                buf.push(0x81); // i64.rem_s
                emit_local_set(&mut buf, inst.a as u32);
            }
            OpCode::Neg => {
                buf.push(0x42); // i64.const 0
                encode_i64_leb128(&mut buf, 0);
                emit_local_get(&mut buf, inst.b as u32);
                buf.push(0x7D); // i64.sub (0 - x)
                emit_local_set(&mut buf, inst.a as u32);
            }
            OpCode::FloorDiv => {
                emit_local_get(&mut buf, inst.b as u32);
                emit_local_get(&mut buf, inst.c as u32);
                buf.push(0x7F); // i64.div_s
                emit_local_set(&mut buf, inst.a as u32);
            }
            // Bitwise
            OpCode::BitAnd => {
                emit_local_get(&mut buf, inst.b as u32);
                emit_local_get(&mut buf, inst.c as u32);
                buf.push(0x83); // i64.and
                emit_local_set(&mut buf, inst.a as u32);
            }
            OpCode::BitOr => {
                emit_local_get(&mut buf, inst.b as u32);
                emit_local_get(&mut buf, inst.c as u32);
                buf.push(0x84); // i64.or
                emit_local_set(&mut buf, inst.a as u32);
            }
            OpCode::BitXor => {
                emit_local_get(&mut buf, inst.b as u32);
                emit_local_get(&mut buf, inst.c as u32);
                buf.push(0x85); // i64.xor
                emit_local_set(&mut buf, inst.a as u32);
            }
            OpCode::Shl => {
                emit_local_get(&mut buf, inst.b as u32);
                emit_local_get(&mut buf, inst.c as u32);
                buf.push(0x86); // i64.shl
                emit_local_set(&mut buf, inst.a as u32);
            }
            OpCode::Shr => {
                emit_local_get(&mut buf, inst.b as u32);
                emit_local_get(&mut buf, inst.c as u32);
                buf.push(0x87); // i64.shr_s
                emit_local_set(&mut buf, inst.a as u32);
            }
            // Comparison (result stored as i64: 0 or 1)
            OpCode::Eq => {
                emit_local_get(&mut buf, inst.b as u32);
                emit_local_get(&mut buf, inst.c as u32);
                buf.push(0x51); // i64.eq
                buf.push(0xAD); // i64.extend_i32_u
                emit_local_set(&mut buf, inst.a as u32);
            }
            OpCode::Lt => {
                emit_local_get(&mut buf, inst.b as u32);
                emit_local_get(&mut buf, inst.c as u32);
                buf.push(0x53); // i64.lt_s
                buf.push(0xAD); // i64.extend_i32_u
                emit_local_set(&mut buf, inst.a as u32);
            }
            OpCode::Le => {
                emit_local_get(&mut buf, inst.b as u32);
                emit_local_get(&mut buf, inst.c as u32);
                buf.push(0x57); // i64.le_s
                buf.push(0xAD); // i64.extend_i32_u
                emit_local_set(&mut buf, inst.a as u32);
            }
            OpCode::Not => {
                emit_local_get(&mut buf, inst.b as u32);
                buf.push(0x50); // i64.eqz
                buf.push(0xAD); // i64.extend_i32_u
                emit_local_set(&mut buf, inst.a as u32);
            }
            // Logic
            OpCode::And => {
                emit_local_get(&mut buf, inst.b as u32);
                emit_local_get(&mut buf, inst.c as u32);
                buf.push(0x83); // i64.and
                emit_local_set(&mut buf, inst.a as u32);
            }
            OpCode::Or => {
                emit_local_get(&mut buf, inst.b as u32);
                emit_local_get(&mut buf, inst.c as u32);
                buf.push(0x84); // i64.or
                emit_local_set(&mut buf, inst.a as u32);
            }
            // Return
            OpCode::Return => {
                emit_local_get(&mut buf, inst.a as u32);
                buf.push(0x0F); // return
            }
            // Nop and unsupported opcodes
            OpCode::Nop => {
                buf.push(0x01); // nop
            }
            // Everything else: emit a placeholder i64.const 0 + nop
            _ => {
                // Unsupported opcodes are skipped — they become no-ops.
                // This matches the native backend's approach of trapping on
                // unrecognized opcodes, but in wasm we simply ignore them.
                buf.push(0x01); // nop
            }
        }
    }

    // If the function has a return type but the last instruction wasn't
    // a Return, push a default return value.
    let last_is_return = cell
        .instructions
        .last()
        .map(|i| i.op == OpCode::Return)
        .unwrap_or(false);
    if !last_is_return {
        buf.push(0x42); // i64.const 0
        encode_i64_leb128(&mut buf, 0);
    }

    // End of function body.
    buf.push(0x0B); // end

    Ok(buf)
}

// ---------------------------------------------------------------------------
// Wasm instruction helpers
// ---------------------------------------------------------------------------

fn emit_local_get(buf: &mut Vec<u8>, idx: u32) {
    buf.push(0x20); // local.get
    encode_u32_leb128(buf, idx);
}

fn emit_local_set(buf: &mut Vec<u8>, idx: u32) {
    buf.push(0x21); // local.set
    encode_u32_leb128(buf, idx);
}

fn emit_wasm_load_constant(
    buf: &mut Vec<u8>,
    cell: &LirCell,
    const_idx: usize,
    dest_reg: u8,
) -> Result<(), CodegenError> {
    let constant = cell.constants.get(const_idx).ok_or_else(|| {
        CodegenError::LoweringError(format!(
            "wasm: constant index {const_idx} out of range (cell has {})",
            cell.constants.len()
        ))
    })?;

    match constant {
        Constant::Int(n) => {
            buf.push(0x42); // i64.const
            encode_i64_leb128(buf, *n);
        }
        Constant::Float(f) => {
            buf.push(0x44); // f64.const
            buf.extend_from_slice(&f.to_le_bytes());
            // Float on the stack needs to be reinterpreted as i64 since our
            // locals are all i64. Use i64.reinterpret_f64.
            buf.push(0xBD); // i64.reinterpret_f64
        }
        Constant::Bool(b) => {
            buf.push(0x42); // i64.const
            encode_i64_leb128(buf, *b as i64);
        }
        Constant::Null => {
            buf.push(0x42); // i64.const
            encode_i64_leb128(buf, 0);
        }
        Constant::String(_) => {
            // Strings become 0 (null pointer) in the wasm context.
            // A full implementation would store strings in a data section.
            buf.push(0x42);
            encode_i64_leb128(buf, 0);
        }
        Constant::BigInt(_) => {
            buf.push(0x42);
            encode_i64_leb128(buf, 0);
        }
    }

    emit_local_set(buf, dest_reg as u32);
    Ok(())
}

// ---------------------------------------------------------------------------
// Section framing
// ---------------------------------------------------------------------------

/// Emit a wasm section: section_id byte + LEB128 length + payload.
fn emit_section(out: &mut Vec<u8>, section_id: u8, payload: &[u8]) {
    out.push(section_id);
    encode_u32_leb128(out, payload.len() as u32);
    out.extend_from_slice(payload);
}

// ---------------------------------------------------------------------------
// LEB128 encoding
// ---------------------------------------------------------------------------

fn encode_u32_leb128(buf: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn encode_i64_leb128(buf: &mut Vec<u8>, mut value: i64) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        // If the sign bit of the byte is set and value is -1, or
        // the sign bit is not set and value is 0, we're done.
        let done = (value == 0 && (byte & 0x40) == 0) || (value == -1 && (byte & 0x40) != 0);
        if done {
            buf.push(byte);
            break;
        } else {
            buf.push(byte | 0x80);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use lumen_compiler::compiler::lir::{
        Constant, Instruction, LirCell, LirModule, LirParam, OpCode,
    };

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
    fn leb128_encoding_correctness() {
        // Test unsigned LEB128
        let mut buf = Vec::new();
        encode_u32_leb128(&mut buf, 0);
        assert_eq!(buf, vec![0x00]);

        buf.clear();
        encode_u32_leb128(&mut buf, 127);
        assert_eq!(buf, vec![0x7F]);

        buf.clear();
        encode_u32_leb128(&mut buf, 128);
        assert_eq!(buf, vec![0x80, 0x01]);

        buf.clear();
        encode_u32_leb128(&mut buf, 624485);
        assert_eq!(buf, vec![0xE5, 0x8E, 0x26]);

        // Test signed LEB128
        buf.clear();
        encode_i64_leb128(&mut buf, 0);
        assert_eq!(buf, vec![0x00]);

        buf.clear();
        encode_i64_leb128(&mut buf, -1);
        assert_eq!(buf, vec![0x7F]);

        buf.clear();
        encode_i64_leb128(&mut buf, 42);
        assert_eq!(buf, vec![42]);

        buf.clear();
        encode_i64_leb128(&mut buf, -128);
        assert_eq!(buf, vec![0x80, 0x7F]);
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
}
