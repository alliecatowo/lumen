//! LIR (Lumen Intermediate Representation) data types.
//! 32-bit fixed-width instructions, Lua-style register VM.

use serde::{Deserialize, Serialize};

/// Opcodes for the Lumen register VM.
/// Hex values match SPEC section 40.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum_macros::EnumIter, strum_macros::EnumCount)]
#[repr(u8)]
pub enum OpCode {
    // Misc
    Nop = 0x00, // Ax: no operation

    // Register and constant ops
    LoadK = 0x01,    // A, Bx: load constant Bx into register A
    LoadNil = 0x02,  // A, B:  set registers A..A+B to nil
    LoadBool = 0x03, // A, B, C: load bool B into A; if C, skip next
    LoadInt = 0x04,  // A, sB: R[A] = sB as i64 (small integer)
    Move = 0x05,     // A, B:  copy register B to A

    // Data construction
    NewList = 0x06,   // A, B:  create list from B values at A+1..
    NewMap = 0x07,    // A, B:  create map from B kv pairs at A+1..
    NewRecord = 0x08, // A, Bx: create record of type Bx
    NewUnion = 0x09,  // A, B, C: create union tag=B payload=C
    NewTuple = 0x0A,  // A, B:  create tuple from B values at A+1..
    NewSet = 0x0B,    // A, B:  create set from B values at A+1..

    // Access
    GetField = 0x10, // A, B, C: A = B.field[C]
    SetField = 0x11, // A, B, C: A.field[B] = C
    GetIndex = 0x12, // A, B, C: A = B[C]
    SetIndex = 0x13, // A, B, C: A[B] = C
    GetTuple = 0x14, // A, B, C: A = R[B].elements[C]

    // Arithmetic
    Add = 0x20,      // A, B, C: A = B + C
    Sub = 0x21,      // A, B, C: A = B - C
    Mul = 0x22,      // A, B, C: A = B * C
    Div = 0x23,      // A, B, C: A = B / C
    Mod = 0x24,      // A, B, C: A = B % C
    Pow = 0x25,      // A, B, C: A = B ** C
    Neg = 0x26,      // A, B:    A = -B
    Concat = 0x27,   // A, B, C: A = B ++ C
    FloorDiv = 0x2E, // A, B, C: A = B // C (floor division)

    // Bitwise
    BitOr = 0x28,  // A, B, C: A = B | C
    BitAnd = 0x29, // A, B, C: A = B & C
    BitXor = 0x2A, // A, B, C: A = B ^ C
    BitNot = 0x2B, // A, B:    A = ~B
    Shl = 0x2C,    // A, B, C: A = B << C
    Shr = 0x2D,    // A, B, C: A = B >> C

    // Comparison / logic
    Eq = 0x30,     // A, B, C: if (B == C) != A then skip next
    Lt = 0x31,     // A, B, C: if (B < C) != A then skip next
    Le = 0x32,     // A, B, C: if (B <= C) != A then skip next
    Not = 0x33,    // A, B:    A = not B
    And = 0x34,    // A, B, C: A = B and C
    Or = 0x35,     // A, B, C: A = B or C
    In = 0x36,     // A, B, C: A = B in C
    Is = 0x37,     // A, B, C: A = typeof(B) == type(C)
    NullCo = 0x38, // A, B, C: A = if B != null then B else C
    Test = 0x39,   // A, C: if (Reg[A] is truthy) != C then skip next

    // Control flow
    Jmp = 0x40,      // Ax: jump by signed offset
    Call = 0x41,     // A, B, C: call A with B args, C results
    TailCall = 0x42, // A, B, C: tail-call A with B args
    Return = 0x43,   // A, B: return B values starting from A
    Halt = 0x44,     // A: halt with error message in A
    Loop = 0x45,     // AsB: decrement counter, jump if > 0
    ForPrep = 0x46,  // A, sB: prepare for-loop
    ForLoop = 0x47,  // A, sB: iterate for-loop
    ForIn = 0x48,    // A, B, C: for-in iterator step
    Break = 0x49,    // Ax: break from enclosing loop
    Continue = 0x4A, // Ax: continue to next iteration

    // Intrinsics
    Intrinsic = 0x50, // A, B, C: A = intrinsic[B](args at C)

    // Closures
    Closure = 0x51,  // A, Bx: R[A] = closure(proto=Bx, upvalues from regs)
    GetUpval = 0x52, // A, B:  R[A] = upvalue[B]
    SetUpval = 0x53, // A, B:  upvalue[B] = R[A]

    // Effects
    ToolCall = 0x60, // A, Bx: tool_call(tool=Bx, args from subsequent regs)
    Schema = 0x61,   // A, B: validate A against schema type B
    Emit = 0x62,     // A: emit output R[A]
    TraceRef = 0x63, // A: R[A] = current trace reference
    Await = 0x64,       // A, B: R[A] = await future R[B]
    Spawn = 0x65,       // A, Bx: R[A] = spawn async(proto=Bx)
    Perform = 0x66,     // A, B, C: perform effect B, operation C, result to A
    HandlePush = 0x67,  // Ax: push effect handler scope at offset Ax
    HandlePop = 0x68,   // pop current effect handler scope
    Resume = 0x69,      // A: resume suspended computation with value in A

    // List ops
    Append = 0x70, // A, B: append B to list A

    // Type checks
    IsVariant = 0x71, // A, Bx: if A is variant w/ tag Bx, skip next
    Unbox = 0x72,     // A, B: A = B.payload (for unions)
}

/// Intrinsic function IDs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum_macros::EnumIter, strum_macros::EnumCount)]
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
    // New stdlib intrinsics
    Sort = 29,
    Reverse = 30,
    Map = 31,
    Filter = 32,
    Reduce = 33,
    FlatMap = 34,
    Zip = 35,
    Enumerate = 36,
    Any = 37,
    All = 38,
    Find = 39,
    Position = 40,
    GroupBy = 41,
    Chunk = 42,
    Window = 43,
    Flatten = 44,
    Unique = 45,
    Take = 46,
    Drop = 47,
    First = 48,
    Last = 49,
    IsEmpty = 50,
    Chars = 51,
    StartsWith = 52,
    EndsWith = 53,
    IndexOf = 54,
    PadLeft = 55,
    PadRight = 56,
    Round = 57,
    Ceil = 58,
    Floor = 59,
    Sqrt = 60,
    Pow = 61,
    Log = 62,
    Sin = 63,
    Cos = 64,
    Clamp = 65,
    Clone = 66,
    Sizeof = 67,
    Debug = 68,
    ToSet = 69,
    // Map/Set operations
    HasKey = 70,
    Merge = 71,
    Size = 72,
    Add = 73,
    Remove = 74,
    Entries = 75,
    Compose = 76,
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
    pub fn abc(op: OpCode, a: u8, b: u8, c: u8) -> Self {
        Self { op, a, b, c }
    }
    pub fn abx(op: OpCode, a: u8, bx: u16) -> Self {
        Self {
            op,
            a,
            b: (bx >> 8) as u8,
            c: (bx & 0xFF) as u8,
        }
    }
    pub fn ax(op: OpCode, ax: u32) -> Self {
        Self {
            op,
            a: ((ax >> 16) & 0xFF) as u8,
            b: ((ax >> 8) & 0xFF) as u8,
            c: (ax & 0xFF) as u8,
        }
    }
    /// Signed 24-bit AX constructor for jump offsets (supports negative values)
    pub fn sax(op: OpCode, offset: i32) -> Self {
        let bits = (offset as u32) & 0xFFFFFF;
        Self {
            op,
            a: ((bits >> 16) & 0xFF) as u8,
            b: ((bits >> 8) & 0xFF) as u8,
            c: (bits & 0xFF) as u8,
        }
    }
    pub fn bx(&self) -> u16 {
        ((self.b as u16) << 8) | (self.c as u16)
    }
    pub fn ax_val(&self) -> u32 {
        ((self.a as u32) << 16) | ((self.b as u32) << 8) | (self.c as u32)
    }
    /// Signed 24-bit AX value with sign extension for jump offsets
    pub fn sax_val(&self) -> i32 {
        let raw = self.ax_val();
        if raw & 0x800000 != 0 {
            (raw | 0xFF000000) as i32
        } else {
            raw as i32
        }
    }
    pub fn sbx(&self) -> i16 {
        self.bx() as i16
    }
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
    /// Metadata for effect handler scopes pushed by HandlePush instructions in this cell.
    /// Each entry maps a HandlePush's `a` field (meta index) to the effect name and operation
    /// that the handler scope matches against.
    #[serde(default)]
    pub effect_handler_metas: Vec<LirEffectHandlerMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub register: u8,
    #[serde(default)]
    pub variadic: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirAgent {
    pub name: String,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirAddon {
    pub kind: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirEffect {
    pub name: String,
    pub operations: Vec<LirEffectOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirEffectOp {
    pub name: String,
    pub params: Vec<LirParam>,
    pub returns: Option<String>,
    pub effects: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirEffectBind {
    pub effect_path: String,
    pub tool_alias: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirHandler {
    pub name: String,
    pub handles: Vec<LirHandle>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirHandle {
    pub operation: String,
    pub cell: String,
}

/// Metadata for an inline effect handler in a handle...with...end expression
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LirEffectHandlerMeta {
    pub effect_name: String,
    pub operation: String,
    pub param_count: u8,
    pub handler_ip: usize,
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
    pub agents: Vec<LirAgent>,
    pub addons: Vec<LirAddon>,
    pub effects: Vec<LirEffect>,
    pub effect_binds: Vec<LirEffectBind>,
    pub handlers: Vec<LirHandler>,
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
            agents: Vec::new(),
            addons: Vec::new(),
            effects: Vec::new(),
            effect_binds: Vec::new(),
            handlers: Vec::new(),
        }
    }

    /// Merge another module's definitions into this module.
    ///
    /// This is used during import resolution to link imported modules into the main module.
    /// String table entries are deduplicated. Other items (cells, types, etc.) are appended,
    /// assuming no name conflicts (the resolver should have already checked this).
    pub fn merge(&mut self, other: &LirModule) {
        use std::collections::HashMap;

        // Build a map from old string indices in `other` to new indices in `self`
        let mut string_remap: HashMap<usize, usize> = HashMap::new();
        for (old_idx, s) in other.strings.iter().enumerate() {
            if let Some(existing_idx) = self.strings.iter().position(|x| x == s) {
                string_remap.insert(old_idx, existing_idx);
            } else {
                string_remap.insert(old_idx, self.strings.len());
                self.strings.push(s.clone());
            }
        }

        // Merge types (no string remapping needed for simple names)
        for ty in &other.types {
            if !self.types.iter().any(|t| t.name == ty.name) {
                self.types.push(ty.clone());
            }
        }

        // Merge cells (no string remapping needed for simple names)
        for cell in &other.cells {
            if !self.cells.iter().any(|c| c.name == cell.name) {
                self.cells.push(cell.clone());
            }
        }

        // Merge tools
        for tool in &other.tools {
            if !self.tools.iter().any(|t| t.alias == tool.alias) {
                self.tools.push(tool.clone());
            }
        }

        // Merge policies
        for policy in &other.policies {
            if !self
                .policies
                .iter()
                .any(|p| p.tool_alias == policy.tool_alias)
            {
                self.policies.push(policy.clone());
            }
        }

        // Merge agents
        for agent in &other.agents {
            if !self.agents.iter().any(|a| a.name == agent.name) {
                self.agents.push(agent.clone());
            }
        }

        // Merge addons
        self.addons.extend_from_slice(&other.addons);

        // Merge effects
        for effect in &other.effects {
            if !self.effects.iter().any(|e| e.name == effect.name) {
                self.effects.push(effect.clone());
            }
        }

        // Merge effect bindings
        for bind in &other.effect_binds {
            if !self
                .effect_binds
                .iter()
                .any(|b| b.effect_path == bind.effect_path && b.tool_alias == bind.tool_alias)
            {
                self.effect_binds.push(bind.clone());
            }
        }

        // Merge handlers
        for handler in &other.handlers {
            if !self.handlers.iter().any(|h| h.name == handler.name) {
                self.handlers.push(handler.clone());
            }
        }
    }
}
