//! LIR (Lumen Intermediate Representation) data types.
//! 32-bit fixed-width instructions, Lua-style register VM.

use serde::{Deserialize, Serialize};

/// Opcodes for the Lumen register VM
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum OpCode {
    // Register and constant ops
    LoadK = 0,       // A, Bx: load constant Bx into register A
    LoadNil = 1,     // A, B:  set registers A..A+B to nil
    LoadBool = 2,    // A, B, C: load bool B into A; if C, skip next
    Move = 3,        // A, B:  copy register B to A

    // Data construction
    NewList = 4,     // A, B:  create list from B values at A+1..
    NewMap = 5,      // A, B:  create map from B kv pairs at A+1..
    NewRecord = 6,   // A, Bx: create record of type Bx
    NewUnion = 7,    // A, B, C: create union tag=B payload=C

    // Access
    GetField = 8,    // A, B, C: A = B.field[C]
    SetField = 9,    // A, B, C: A.field[B] = C
    GetIndex = 10,   // A, B, C: A = B[C]
    SetIndex = 11,   // A, B, C: A[B] = C

    // Arithmetic
    Add = 12,        // A, B, C: A = B + C
    Sub = 13,        // A, B, C: A = B - C
    Mul = 14,        // A, B, C: A = B * C
    Div = 15,        // A, B, C: A = B / C
    Mod = 16,        // A, B, C: A = B % C
    Neg = 17,        // A, B:    A = -B

    // Comparison
    Eq = 18,         // A, B, C: if (B == C) != A then skip next
    Lt = 19,         // A, B, C: if (B < C) != A then skip next
    Le = 20,         // A, B, C: if (B <= C) != A then skip next

    // Logic
    Not = 21,        // A, B:    A = not B
    And = 22,        // A, B, C: A = B and C
    Or = 23,         // A, B, C: A = B or C
    Concat = 24,     // A, B, C: A = B .. C

    // Control flow
    Jmp = 25,        // Ax: jump by signed offset
    Call = 26,       // A, B, C: call A with B args, C results
    Return = 27,     // A, B: return B values starting from A
    Halt = 28,       // A: halt with error message in A

    // Intrinsics
    Intrinsic = 29,  // A, B, C: A = intrinsic[B](args at C)

    // Effects (trace boundaries)
    ToolCall = 30,   // A, B, C, D: call tool B, policy C, D args at A+1
    Schema = 31,     // A, B: validate A against schema type B

    // For loop support
    ForPrep = 32,    // A, Bx: prepare for loop
    ForLoop = 33,    // A, Bx: iterate

    // List append
    Append = 34,     // A, B: append B to list A
}

/// Intrinsic function IDs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum IntrinsicId {
    Length = 0,
    Count = 1,
    Matches = 2,
    Hash = 3,
    Diff = 4,
    Patch = 5,
    Redact = 6,
    Validate = 7,
    TraceRef = 8,
    Print = 9,
    ToString = 10,
    ToInt = 11,
    ToFloat = 12,
    TypeOf = 13,
    Keys = 14,
    Values = 15,
    Contains = 16,
    Join = 17,
    Split = 18,
    Trim = 19,
    Upper = 20,
    Lower = 21,
    Replace = 22,
    Slice = 23,
    Append = 24,
    Range = 25,
    Abs = 26,
    Min = 27,
    Max = 28,
}

/// A 32-bit instruction
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Instruction {
    pub op: OpCode,
    pub a: u8,
    pub b: u8,
    pub c: u8,
}

impl Instruction {
    pub fn abc(op: OpCode, a: u8, b: u8, c: u8) -> Self { Self { op, a, b, c } }
    pub fn abx(op: OpCode, a: u8, bx: u16) -> Self { Self { op, a, b: (bx >> 8) as u8, c: (bx & 0xFF) as u8 } }
    pub fn ax(op: OpCode, ax: u32) -> Self { Self { op, a: ((ax >> 16) & 0xFF) as u8, b: ((ax >> 8) & 0xFF) as u8, c: (ax & 0xFF) as u8 } }
    pub fn bx(&self) -> u16 { ((self.b as u16) << 8) | (self.c as u16) }
    pub fn ax_val(&self) -> u32 { ((self.a as u32) << 16) | ((self.b as u32) << 8) | (self.c as u32) }
    pub fn sbx(&self) -> i16 { self.bx() as i16 }
}

/// Constant value in the constant pool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Constant {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

/// Type definition in LIR
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirType {
    pub kind: String,
    pub name: String,
    pub fields: Vec<LirField>,
    pub variants: Vec<LirVariant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirField {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub constraints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirVariant {
    pub name: String,
    pub payload: Option<String>,
}

/// A compiled cell in LIR
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirCell {
    pub name: String,
    pub params: Vec<LirParam>,
    pub returns: Option<String>,
    pub registers: u8,
    pub constants: Vec<Constant>,
    pub instructions: Vec<Instruction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub register: u8,
}

/// Tool declaration in LIR
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirTool {
    pub alias: String,
    pub tool_id: String,
    pub version: String,
    pub mcp_url: Option<String>,
}

/// Policy/grant in LIR
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirPolicy {
    pub tool_alias: String,
    pub grants: serde_json::Value,
}

/// Complete LIR module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirModule {
    pub version: String,
    pub doc_hash: String,
    pub strings: Vec<String>,
    pub types: Vec<LirType>,
    pub cells: Vec<LirCell>,
    pub tools: Vec<LirTool>,
    pub policies: Vec<LirPolicy>,
}

impl LirModule {
    pub fn new(doc_hash: String) -> Self {
        Self {
            version: "1.0.0".to_string(),
            doc_hash,
            strings: Vec::new(),
            types: Vec::new(),
            cells: Vec::new(),
            tools: Vec::new(),
            policies: Vec::new(),
        }
    }
}
