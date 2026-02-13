//! Register VM dispatch loop for executing LIR bytecode.

use crate::values::{Value, StringRef, RecordValue};
use crate::strings::StringTable;
use crate::types::{TypeTable, RuntimeType, RuntimeTypeKind, RuntimeField, RuntimeVariant};
use lumen_compiler::compiler::lir::*;
use thiserror::Error;
use std::collections::BTreeMap;

#[derive(Debug, Error)]
pub enum VmError {
    #[error("runtime error: {0}")]
    Runtime(String),
    #[error("halt: {0}")]
    Halt(String),
    #[error("stack overflow: call depth exceeded {0}")]
    StackOverflow(usize),
    #[error("undefined cell: {0}")]
    UndefinedCell(String),
    #[error("register out of bounds: r{0} in cell with {1} registers")]
    RegisterOOB(u8, u8),
    #[error("tool call error: {0}")]
    ToolError(String),
    #[error("type error at runtime: {0}")]
    TypeError(String),
    #[error("no module loaded")]
    NoModule,
}

const MAX_CALL_DEPTH: usize = 256;

/// Call frame on the VM stack.
#[derive(Debug)]
struct CallFrame {
    cell_idx: usize,
    base_register: usize,
    ip: usize,
    return_register: usize,
}

/// The Lumen register VM.
pub struct VM {
    pub strings: StringTable,
    pub types: TypeTable,
    registers: Vec<Value>,
    frames: Vec<CallFrame>,
    module: Option<LirModule>,
    /// Captured stdout output (for testing and tracing)
    pub output: Vec<String>,
}

impl VM {
    pub fn new() -> Self {
        Self {
            strings: StringTable::new(),
            types: TypeTable::new(),
            registers: Vec::new(),
            frames: Vec::new(),
            module: None,
            output: Vec::new(),
        }
    }

    /// Load a LIR module into the VM.
    pub fn load(&mut self, module: LirModule) {
        // Intern all strings
        for s in &module.strings {
            self.strings.intern(s);
        }
        // Register types
        for ty in &module.types {
            let rt = match ty.kind.as_str() {
                "record" => RuntimeType {
                    name: ty.name.clone(),
                    kind: RuntimeTypeKind::Record(ty.fields.iter().map(|f| RuntimeField {
                        name: f.name.clone(), ty: f.ty.clone(),
                    }).collect()),
                },
                "enum" => RuntimeType {
                    name: ty.name.clone(),
                    kind: RuntimeTypeKind::Enum(ty.variants.iter().map(|v| RuntimeVariant {
                        name: v.name.clone(), payload: v.payload.clone(),
                    }).collect()),
                },
                _ => continue,
            };
            self.types.register(rt);
        }
        self.module = Some(module);
    }

    /// Execute a cell by name with arguments.
    pub fn execute(&mut self, cell_name: &str, args: Vec<Value>) -> Result<Value, VmError> {
        let module = self.module.as_ref().ok_or(VmError::NoModule)?;
        let cell_idx = module.cells.iter().position(|c| c.name == cell_name)
            .ok_or_else(|| VmError::UndefinedCell(cell_name.into()))?;

        let cell = &module.cells[cell_idx];
        let num_regs = cell.registers as usize;

        // Grow register file
        let base = self.registers.len();
        self.registers.resize(base + num_regs.max(256), Value::Null);

        // Load arguments into parameter registers
        for (i, arg) in args.into_iter().enumerate() {
            if i < cell.params.len() {
                self.registers[base + cell.params[i].register as usize] = arg;
            }
        }

        // Push initial frame
        self.frames.push(CallFrame {
            cell_idx,
            base_register: base,
            ip: 0,
            return_register: 0,
        });

        // Execute
        self.run()
    }

    fn run(&mut self) -> Result<Value, VmError> {
        loop {
            let frame = match self.frames.last() {
                Some(f) => f,
                None => return Ok(Value::Null),
            };
            let cell_idx = frame.cell_idx;
            let base = frame.base_register;
            let ip = frame.ip;

            let module = self.module.as_ref().unwrap();
            let cell = &module.cells[cell_idx];

            if ip >= cell.instructions.len() {
                // Implicit return
                self.frames.pop();
                if self.frames.is_empty() {
                    return Ok(Value::Null);
                }
                continue;
            }

            let instr = cell.instructions[ip];

            // Advance IP in the frame
            if let Some(f) = self.frames.last_mut() { f.ip += 1; }

            let a = instr.a as usize;
            let b = instr.b as usize;
            let c = instr.c as usize;

            match instr.op {
                OpCode::LoadK => {
                    let bx = instr.bx() as usize;
                    let val = match &cell.constants[bx] {
                        Constant::Null => Value::Null,
                        Constant::Bool(v) => Value::Bool(*v),
                        Constant::Int(v) => Value::Int(*v),
                        Constant::Float(v) => Value::Float(*v),
                        Constant::String(v) => Value::String(StringRef::Owned(v.clone())),
                    };
                    self.registers[base + a] = val;
                }
                OpCode::LoadNil => {
                    for i in 0..=b { self.registers[base + a + i] = Value::Null; }
                }
                OpCode::LoadBool => {
                    self.registers[base + a] = Value::Bool(b != 0);
                    if c != 0 {
                        if let Some(f) = self.frames.last_mut() { f.ip += 1; }
                    }
                }
                OpCode::Move => {
                    let val = self.registers[base + b].clone();
                    self.registers[base + a] = val;
                }
                OpCode::NewList => {
                    let mut list = Vec::with_capacity(b);
                    for i in 1..=b { list.push(self.registers[base + a + i].clone()); }
                    self.registers[base + a] = Value::List(list);
                }
                OpCode::NewMap => {
                    let mut map = BTreeMap::new();
                    for i in 0..b {
                        let k = self.registers[base + a + 1 + i * 2].as_string();
                        let v = self.registers[base + a + 2 + i * 2].clone();
                        map.insert(k, v);
                    }
                    self.registers[base + a] = Value::Map(map);
                }
                OpCode::NewRecord => {
                    let bx = instr.bx() as usize;
                    let type_name = if bx < module.strings.len() { module.strings[bx].clone() } else { "Unknown".to_string() };
                    let fields = BTreeMap::new();
                    self.registers[base + a] = Value::Record(RecordValue { type_name, fields });
                }
                OpCode::NewUnion => {
                    let tag = self.registers[base + b].as_string();
                    let payload = Box::new(self.registers[base + c].clone());
                    self.registers[base + a] = Value::Union(crate::values::UnionValue { tag, payload });
                }
                OpCode::GetField => {
                    let obj = &self.registers[base + b];
                    let field_name = if c < module.strings.len() { &module.strings[c] } else { "" };
                    let val = match obj {
                        Value::Record(r) => r.fields.get(field_name).cloned().unwrap_or(Value::Null),
                        Value::Map(m) => m.get(field_name).cloned().unwrap_or(Value::Null),
                        _ => Value::Null,
                    };
                    self.registers[base + a] = val;
                }
                OpCode::SetField => {
                    let val = self.registers[base + c].clone();
                    let field_name = if b < module.strings.len() { module.strings[b].clone() } else { String::new() };
                    if let Value::Record(ref mut r) = self.registers[base + a] {
                        r.fields.insert(field_name, val);
                    }
                }
                OpCode::GetIndex => {
                    let obj = &self.registers[base + b];
                    let idx = &self.registers[base + c];
                    let val = match (obj, idx) {
                        (Value::List(l), Value::Int(i)) => l.get(*i as usize).cloned().unwrap_or(Value::Null),
                        (Value::Map(m), _) => m.get(&idx.as_string()).cloned().unwrap_or(Value::Null),
                        _ => Value::Null,
                    };
                    self.registers[base + a] = val;
                }
                OpCode::SetIndex => {
                    let val = self.registers[base + c].clone();
                    let key = self.registers[base + b].clone();
                    match &mut self.registers[base + a] {
                        Value::List(l) => { if let Some(i) = key.as_int() { if (i as usize) < l.len() { l[i as usize] = val; } } }
                        Value::Map(m) => { m.insert(key.as_string(), val); }
                        _ => {}
                    }
                }
                OpCode::Add => {
                    let lhs = &self.registers[base + b];
                    let rhs = &self.registers[base + c];
                    let result = match (lhs, rhs) {
                        (Value::Int(a), Value::Int(b)) => Value::Int(a + b),
                        (Value::Float(a), Value::Float(b)) => Value::Float(a + b),
                        (Value::Int(a), Value::Float(b)) => Value::Float(*a as f64 + b),
                        (Value::Float(a), Value::Int(b)) => Value::Float(a + *b as f64),
                        (Value::String(_), _) | (_, Value::String(_)) => {
                            Value::String(StringRef::Owned(format!("{}{}", lhs.as_string(), rhs.as_string())))
                        }
                        _ => return Err(VmError::TypeError(format!("cannot add {} and {}", lhs, rhs))),
                    };
                    self.registers[base + a] = result;
                }
                OpCode::Sub => { self.arith_op(base, a, b, c, |x, y| x - y, |x, y| x - y)?; }
                OpCode::Mul => { self.arith_op(base, a, b, c, |x, y| x * y, |x, y| x * y)?; }
                OpCode::Div => { self.arith_op(base, a, b, c, |x, y| x / y, |x, y| x / y)?; }
                OpCode::Mod => { self.arith_op(base, a, b, c, |x, y| x % y, |x, y| x % y)?; }
                OpCode::Neg => {
                    let val = &self.registers[base + b];
                    self.registers[base + a] = match val {
                        Value::Int(n) => Value::Int(-n),
                        Value::Float(f) => Value::Float(-f),
                        _ => return Err(VmError::TypeError(format!("cannot negate {}", val))),
                    };
                }
                OpCode::Eq => {
                    let lhs = &self.registers[base + b];
                    let rhs = &self.registers[base + c];
                    let eq = lhs == rhs;
                    // If A == 0, test for equality; if A != 0, test for inequality
                    let test = if a == 0 { !eq } else { eq };
                    if test {
                        // Skip next instruction
                        if let Some(f) = self.frames.last_mut() { f.ip += 1; }
                    }
                    // Also store the result in case it's used as a value
                    // We store based on b == c comparison result into the first operand position
                    self.registers[base + a] = Value::Bool(eq);
                }
                OpCode::Lt => {
                    let lhs = &self.registers[base + b];
                    let rhs = &self.registers[base + c];
                    let result = match (lhs, rhs) {
                        (Value::Int(a), Value::Int(b)) => a < b,
                        (Value::Float(a), Value::Float(b)) => a < b,
                        (Value::Int(a), Value::Float(b)) => (*a as f64) < *b,
                        (Value::Float(a), Value::Int(b)) => *a < (*b as f64),
                        _ => false,
                    };
                    self.registers[base + a] = Value::Bool(result);
                }
                OpCode::Le => {
                    let lhs = &self.registers[base + b];
                    let rhs = &self.registers[base + c];
                    let result = match (lhs, rhs) {
                        (Value::Int(a), Value::Int(b)) => a <= b,
                        (Value::Float(a), Value::Float(b)) => a <= b,
                        (Value::Int(a), Value::Float(b)) => (*a as f64) <= *b,
                        (Value::Float(a), Value::Int(b)) => *a <= (*b as f64),
                        _ => false,
                    };
                    self.registers[base + a] = Value::Bool(result);
                }
                OpCode::Not => {
                    let val = &self.registers[base + b];
                    self.registers[base + a] = Value::Bool(!val.is_truthy());
                }
                OpCode::And => {
                    let lhs = &self.registers[base + b];
                    let rhs = &self.registers[base + c];
                    self.registers[base + a] = Value::Bool(lhs.is_truthy() && rhs.is_truthy());
                }
                OpCode::Or => {
                    let lhs = &self.registers[base + b];
                    let rhs = &self.registers[base + c];
                    self.registers[base + a] = Value::Bool(lhs.is_truthy() || rhs.is_truthy());
                }
                OpCode::Concat => {
                    let lhs = self.registers[base + b].as_string();
                    let rhs = self.registers[base + c].as_string();
                    self.registers[base + a] = Value::String(StringRef::Owned(format!("{}{}", lhs, rhs)));
                }
                OpCode::Jmp => {
                    let offset = instr.ax_val() as i32;
                    if let Some(f) = self.frames.last_mut() {
                        f.ip = (f.ip as i32 + offset) as usize;
                    }
                }
                OpCode::Call => {
                    let _nargs = b;
                    let _nresults = c;
                    // Resolve callee — look up cell by name
                    if let Value::String(ref sr) = self.registers[base + a] {
                        let name = match sr { StringRef::Owned(s) => s.clone(), StringRef::Interned(id) => self.strings.resolve(*id).unwrap_or("").to_string() };
                        if let Some(idx) = module.cells.iter().position(|c| c.name == name) {
                            if self.frames.len() >= MAX_CALL_DEPTH {
                                return Err(VmError::StackOverflow(MAX_CALL_DEPTH));
                            }
                            let callee_cell = &module.cells[idx];
                            let new_base = self.registers.len();
                            self.registers.resize(new_base + (callee_cell.registers as usize).max(256), Value::Null);
                            // Copy args into callee's parameter registers
                            for i in 0.._nargs {
                                if i < callee_cell.params.len() {
                                    self.registers[new_base + callee_cell.params[i].register as usize] = self.registers[base + a + 1 + i].clone();
                                }
                            }
                            self.frames.push(CallFrame { cell_idx: idx, base_register: new_base, ip: 0, return_register: base + a });
                        } else {
                            // Check built-in functions
                            match name.as_str() {
                                "print" => {
                                    let mut parts = Vec::new();
                                    for i in 0.._nargs {
                                        parts.push(self.registers[base + a + 1 + i].display_pretty());
                                    }
                                    let output = parts.join(" ");
                                    println!("{}", output);
                                    self.output.push(output);
                                    self.registers[base + a] = Value::Null;
                                }
                                "len" | "length" => {
                                    let arg = &self.registers[base + a + 1];
                                    let result = match arg {
                                        Value::String(StringRef::Owned(s)) => Value::Int(s.len() as i64),
                                        Value::List(l) => Value::Int(l.len() as i64),
                                        Value::Map(m) => Value::Int(m.len() as i64),
                                        _ => Value::Int(0),
                                    };
                                    self.registers[base + a] = result;
                                }
                                "append" => {
                                    let list = self.registers[base + a + 1].clone();
                                    let elem = self.registers[base + a + 2].clone();
                                    if let Value::List(mut l) = list {
                                        l.push(elem);
                                        self.registers[base + a] = Value::List(l);
                                    } else {
                                        self.registers[base + a] = Value::List(vec![elem]);
                                    }
                                }
                                "to_string" | "str" => {
                                    let arg = &self.registers[base + a + 1];
                                    self.registers[base + a] = Value::String(StringRef::Owned(arg.display_pretty()));
                                }
                                "to_int" | "int" => {
                                    let arg = &self.registers[base + a + 1];
                                    let result = match arg {
                                        Value::Int(n) => Value::Int(*n),
                                        Value::Float(f) => Value::Int(*f as i64),
                                        Value::String(StringRef::Owned(s)) => s.parse::<i64>().map(Value::Int).unwrap_or(Value::Null),
                                        Value::Bool(b) => Value::Int(if *b { 1 } else { 0 }),
                                        _ => Value::Null,
                                    };
                                    self.registers[base + a] = result;
                                }
                                "to_float" | "float" => {
                                    let arg = &self.registers[base + a + 1];
                                    let result = match arg {
                                        Value::Float(f) => Value::Float(*f),
                                        Value::Int(n) => Value::Float(*n as f64),
                                        Value::String(StringRef::Owned(s)) => s.parse::<f64>().map(Value::Float).unwrap_or(Value::Null),
                                        _ => Value::Null,
                                    };
                                    self.registers[base + a] = result;
                                }
                                "type_of" => {
                                    let arg = &self.registers[base + a + 1];
                                    let type_name = match arg {
                                        Value::Null => "Null",
                                        Value::Bool(_) => "Bool",
                                        Value::Int(_) => "Int",
                                        Value::Float(_) => "Float",
                                        Value::String(_) => "String",
                                        Value::Bytes(_) => "Bytes",
                                        Value::List(_) => "List",
                                        Value::Map(_) => "Map",
                                        Value::Record(r) => &r.type_name,
                                        Value::Union(u) => &u.tag,
                                        Value::TraceRef(_) => "TraceRef",
                                    };
                                    self.registers[base + a] = Value::String(StringRef::Owned(type_name.to_string()));
                                }
                                "keys" => {
                                    let arg = &self.registers[base + a + 1];
                                    let result = match arg {
                                        Value::Map(m) => Value::List(m.keys().map(|k| Value::String(StringRef::Owned(k.clone()))).collect()),
                                        Value::Record(r) => Value::List(r.fields.keys().map(|k| Value::String(StringRef::Owned(k.clone()))).collect()),
                                        _ => Value::List(vec![]),
                                    };
                                    self.registers[base + a] = result;
                                }
                                "values" => {
                                    let arg = &self.registers[base + a + 1];
                                    let result = match arg {
                                        Value::Map(m) => Value::List(m.values().cloned().collect()),
                                        Value::Record(r) => Value::List(r.fields.values().cloned().collect()),
                                        _ => Value::List(vec![]),
                                    };
                                    self.registers[base + a] = result;
                                }
                                "contains" => {
                                    let collection = &self.registers[base + a + 1];
                                    let needle = &self.registers[base + a + 2];
                                    let result = match collection {
                                        Value::List(l) => l.iter().any(|v| v == needle),
                                        Value::Map(m) => m.contains_key(&needle.as_string()),
                                        Value::String(StringRef::Owned(s)) => s.contains(&needle.as_string()),
                                        _ => false,
                                    };
                                    self.registers[base + a] = Value::Bool(result);
                                }
                                "join" => {
                                    let list = &self.registers[base + a + 1];
                                    let sep = if _nargs > 1 { self.registers[base + a + 2].as_string() } else { ", ".to_string() };
                                    if let Value::List(l) = list {
                                        let joined = l.iter().map(|v| v.display_pretty()).collect::<Vec<_>>().join(&sep);
                                        self.registers[base + a] = Value::String(StringRef::Owned(joined));
                                    } else {
                                        self.registers[base + a] = Value::String(StringRef::Owned(list.display_pretty()));
                                    }
                                }
                                "split" => {
                                    let s = self.registers[base + a + 1].as_string();
                                    let sep = if _nargs > 1 { self.registers[base + a + 2].as_string() } else { " ".to_string() };
                                    let parts: Vec<Value> = s.split(&sep).map(|p| Value::String(StringRef::Owned(p.to_string()))).collect();
                                    self.registers[base + a] = Value::List(parts);
                                }
                                "trim" => {
                                    let s = self.registers[base + a + 1].as_string();
                                    self.registers[base + a] = Value::String(StringRef::Owned(s.trim().to_string()));
                                }
                                "upper" => {
                                    let s = self.registers[base + a + 1].as_string();
                                    self.registers[base + a] = Value::String(StringRef::Owned(s.to_uppercase()));
                                }
                                "lower" => {
                                    let s = self.registers[base + a + 1].as_string();
                                    self.registers[base + a] = Value::String(StringRef::Owned(s.to_lowercase()));
                                }
                                "replace" => {
                                    let s = self.registers[base + a + 1].as_string();
                                    let from = self.registers[base + a + 2].as_string();
                                    let to = self.registers[base + a + 3].as_string();
                                    self.registers[base + a] = Value::String(StringRef::Owned(s.replace(&from, &to)));
                                }
                                "abs" => {
                                    let arg = &self.registers[base + a + 1];
                                    self.registers[base + a] = match arg {
                                        Value::Int(n) => Value::Int(n.abs()),
                                        Value::Float(f) => Value::Float(f.abs()),
                                        _ => arg.clone(),
                                    };
                                }
                                "min" => {
                                    let lhs = &self.registers[base + a + 1];
                                    let rhs = &self.registers[base + a + 2];
                                    self.registers[base + a] = match (lhs, rhs) {
                                        (Value::Int(x), Value::Int(y)) => Value::Int(*x.min(y)),
                                        (Value::Float(x), Value::Float(y)) => Value::Float(x.min(*y)),
                                        _ => lhs.clone(),
                                    };
                                }
                                "max" => {
                                    let lhs = &self.registers[base + a + 1];
                                    let rhs = &self.registers[base + a + 2];
                                    self.registers[base + a] = match (lhs, rhs) {
                                        (Value::Int(x), Value::Int(y)) => Value::Int(*x.max(y)),
                                        (Value::Float(x), Value::Float(y)) => Value::Float(x.max(*y)),
                                        _ => lhs.clone(),
                                    };
                                }
                                "range" => {
                                    let start = self.registers[base + a + 1].as_int().unwrap_or(0);
                                    let end = self.registers[base + a + 2].as_int().unwrap_or(0);
                                    let list: Vec<Value> = (start..end).map(Value::Int).collect();
                                    self.registers[base + a] = Value::List(list);
                                }
                                "hash" => {
                                    use sha2::{Sha256, Digest};
                                    let s = self.registers[base + a + 1].as_string();
                                    let h = format!("sha256:{:x}", Sha256::digest(s.as_bytes()));
                                    self.registers[base + a] = Value::String(StringRef::Owned(h));
                                }
                                _ => {
                                    return Err(VmError::UndefinedCell(name));
                                }
                            }
                        }
                    }
                }
                OpCode::Return => {
                    let return_val = self.registers[base + a].clone();
                    let frame = self.frames.pop().unwrap();
                    if self.frames.is_empty() {
                        return Ok(return_val);
                    }
                    self.registers[frame.return_register] = return_val;
                }
                OpCode::Halt => {
                    let msg = self.registers[base + a].as_string();
                    return Err(VmError::Halt(msg));
                }
                OpCode::Intrinsic => {
                    let func_id = b;
                    let arg_reg = c;
                    let arg = &self.registers[base + arg_reg];
                    let result = match func_id {
                        0 => { // LENGTH
                            match arg {
                                Value::String(StringRef::Owned(s)) => Value::Int(s.len() as i64),
                                Value::String(StringRef::Interned(id)) => {
                                    let s = self.strings.resolve(*id).unwrap_or("");
                                    Value::Int(s.len() as i64)
                                }
                                Value::List(l) => Value::Int(l.len() as i64),
                                Value::Map(m) => Value::Int(m.len() as i64),
                                _ => Value::Int(0),
                            }
                        }
                        1 => { // COUNT
                            match arg {
                                Value::List(l) => Value::Int(l.len() as i64),
                                Value::Map(m) => Value::Int(m.len() as i64),
                                Value::String(StringRef::Owned(s)) => Value::Int(s.len() as i64),
                                _ => Value::Int(0),
                            }
                        }
                        2 => { // MATCHES
                            match arg {
                                Value::Bool(b) => Value::Bool(*b),
                                Value::String(_) => Value::Bool(!arg.as_string().is_empty()),
                                _ => Value::Bool(false),
                            }
                        }
                        3 => { // HASH
                            use sha2::{Sha256, Digest};
                            let hash = format!("{:x}", Sha256::digest(arg.as_string().as_bytes()));
                            Value::String(StringRef::Owned(format!("sha256:{}", hash)))
                        }
                        9 => { // PRINT
                            let output = arg.display_pretty();
                            println!("{}", output);
                            self.output.push(output);
                            Value::Null
                        }
                        10 => { // TOSTRING
                            Value::String(StringRef::Owned(arg.display_pretty()))
                        }
                        11 => { // TOINT
                            match arg {
                                Value::Int(n) => Value::Int(*n),
                                Value::Float(f) => Value::Int(*f as i64),
                                Value::String(StringRef::Owned(s)) => s.parse::<i64>().map(Value::Int).unwrap_or(Value::Null),
                                Value::Bool(b) => Value::Int(if *b { 1 } else { 0 }),
                                _ => Value::Null,
                            }
                        }
                        12 => { // TOFLOAT
                            match arg {
                                Value::Float(f) => Value::Float(*f),
                                Value::Int(n) => Value::Float(*n as f64),
                                Value::String(StringRef::Owned(s)) => s.parse::<f64>().map(Value::Float).unwrap_or(Value::Null),
                                _ => Value::Null,
                            }
                        }
                        13 => { // TYPEOF
                            let type_name = match arg {
                                Value::Null => "Null",
                                Value::Bool(_) => "Bool",
                                Value::Int(_) => "Int",
                                Value::Float(_) => "Float",
                                Value::String(_) => "String",
                                Value::Bytes(_) => "Bytes",
                                Value::List(_) => "List",
                                Value::Map(_) => "Map",
                                Value::Record(_) => "Record",
                                Value::Union(_) => "Union",
                                Value::TraceRef(_) => "TraceRef",
                            };
                            Value::String(StringRef::Owned(type_name.to_string()))
                        }
                        _ => Value::Null,
                    };
                    self.registers[base + a] = result;
                }
                OpCode::ToolCall => {
                    // Tool calls are handled by the runtime layer
                    // For now, placeholder — future: dispatch to ToolDispatcher
                    self.registers[base + a] = Value::String(StringRef::Owned("<<tool call pending>>".into()));
                }
                OpCode::Schema => {
                    // Schema validation — check against type registry
                    let bx = instr.bx() as usize;
                    let _type_name = if bx < module.strings.len() { &module.strings[bx] } else { "" };
                    // For now, pass through — validation is advisory
                }
                OpCode::ForPrep => {
                    // A = iterator state register, Bx = jump offset past loop on empty
                    // Initialize: set loop counter in A to 0
                    let bx = instr.bx() as usize;
                    let iter_val = &self.registers[base + a];
                    let len = match iter_val {
                        Value::List(l) => l.len(),
                        _ => 0,
                    };
                    if len == 0 {
                        // Skip past the loop
                        if let Some(f) = self.frames.last_mut() {
                            f.ip += bx;
                        }
                    }
                    // Store length for the loop to use
                    self.registers[base + a + 1] = Value::Int(0); // index
                    self.registers[base + a + 2] = Value::Int(len as i64); // length
                }
                OpCode::ForLoop => {
                    // A = base of loop vars (iter, idx, len, elem)
                    // Bx = jump back offset
                    let bx = instr.bx();
                    let idx = self.registers[base + a + 1].as_int().unwrap_or(0);
                    let len = self.registers[base + a + 2].as_int().unwrap_or(0);
                    if idx < len {
                        // Load current element
                        let iter = &self.registers[base + a];
                        let elem = match iter {
                            Value::List(l) => l.get(idx as usize).cloned().unwrap_or(Value::Null),
                            _ => Value::Null,
                        };
                        self.registers[base + a + 3] = elem;
                        // Increment index
                        self.registers[base + a + 1] = Value::Int(idx + 1);
                        // Jump back
                        if let Some(f) = self.frames.last_mut() {
                            f.ip = (f.ip as i32 - bx as i32) as usize;
                        }
                    }
                    // If idx >= len, fall through (loop ends)
                }
                OpCode::Append => {
                    // A = list register, B = value to append
                    let val = self.registers[base + b].clone();
                    if let Value::List(ref mut l) = self.registers[base + a] {
                        l.push(val);
                    }
                }
            }
        }
    }

    fn arith_op(&mut self, base: usize, a: usize, b: usize, c: usize,
        int_op: impl Fn(i64, i64) -> i64, float_op: impl Fn(f64, f64) -> f64) -> Result<(), VmError> {
        let lhs = &self.registers[base + b];
        let rhs = &self.registers[base + c];
        self.registers[base + a] = match (lhs, rhs) {
            (Value::Int(x), Value::Int(y)) => Value::Int(int_op(*x, *y)),
            (Value::Float(x), Value::Float(y)) => Value::Float(float_op(*x, *y)),
            (Value::Int(x), Value::Float(y)) => Value::Float(float_op(*x as f64, *y)),
            (Value::Float(x), Value::Int(y)) => Value::Float(float_op(*x, *y as f64)),
            _ => return Err(VmError::TypeError(format!("arithmetic on non-numeric types"))),
        };
        Ok(())
    }
}

impl Default for VM {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_return_42() -> LirModule {
        LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Int".into()),
                registers: 4,
                constants: vec![Constant::Int(42)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
            }],
            tools: vec![],
            policies: vec![],
        }
    }

    #[test]
    fn test_vm_return_42() {
        let mut vm = VM::new();
        vm.load(make_return_42());
        let result = vm.execute("main", vec![]).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    fn make_add() -> LirModule {
        LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "add".into(),
                params: vec![
                    LirParam { name: "a".into(), ty: "Int".into(), register: 0 },
                    LirParam { name: "b".into(), ty: "Int".into(), register: 1 },
                ],
                returns: Some("Int".into()),
                registers: 4,
                constants: vec![],
                instructions: vec![
                    Instruction::abc(OpCode::Add, 2, 0, 1),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
            }],
            tools: vec![],
            policies: vec![],
        }
    }

    #[test]
    fn test_vm_add() {
        let mut vm = VM::new();
        vm.load(make_add());
        let result = vm.execute("add", vec![Value::Int(10), Value::Int(32)]).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_vm_print() {
        // Test print as a built-in function call
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: None,
                registers: 8,
                constants: vec![
                    Constant::String("print".into()),
                    Constant::String("Hello, World!".into()),
                ],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0), // r0 = "print"
                    Instruction::abx(OpCode::LoadK, 1, 1), // r1 = "Hello, World!"
                    Instruction::abc(OpCode::Call, 0, 1, 0), // print(r1)
                    Instruction::abc(OpCode::LoadNil, 0, 0, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
            }],
            tools: vec![],
            policies: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let _result = vm.execute("main", vec![]).unwrap();
        assert_eq!(vm.output, vec!["Hello, World!"]);
    }

    #[test]
    fn test_vm_append() {
        // Test the Append opcode
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("list[Int]".into()),
                registers: 8,
                constants: vec![Constant::Int(1), Constant::Int(2), Constant::Int(3)],
                instructions: vec![
                    Instruction::abc(OpCode::NewList, 0, 0, 0), // r0 = []
                    Instruction::abx(OpCode::LoadK, 1, 0),      // r1 = 1
                    Instruction::abc(OpCode::Append, 0, 1, 0),  // r0.append(1)
                    Instruction::abx(OpCode::LoadK, 1, 1),      // r1 = 2
                    Instruction::abc(OpCode::Append, 0, 1, 0),  // r0.append(2)
                    Instruction::abx(OpCode::LoadK, 1, 2),      // r1 = 3
                    Instruction::abc(OpCode::Append, 0, 1, 0),  // r0.append(3)
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
            }],
            tools: vec![],
            policies: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]).unwrap();
        if let Value::List(l) = result {
            assert_eq!(l.len(), 3);
            assert_eq!(l[0], Value::Int(1));
            assert_eq!(l[1], Value::Int(2));
            assert_eq!(l[2], Value::Int(3));
        } else {
            panic!("expected list");
        }
    }

    #[test]
    fn test_vm_comparison() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Bool".into()),
                registers: 8,
                constants: vec![Constant::Int(5), Constant::Int(10)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),      // r0 = 5
                    Instruction::abx(OpCode::LoadK, 1, 1),      // r1 = 10
                    Instruction::abc(OpCode::Lt, 2, 0, 1),      // r2 = 5 < 10 = true
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
            }],
            tools: vec![],
            policies: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_vm_string_concat() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("String".into()),
                registers: 8,
                constants: vec![
                    Constant::String("Hello, ".into()),
                    Constant::String("World!".into()),
                ],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::Concat, 2, 0, 1),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
            }],
            tools: vec![],
            policies: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]).unwrap();
        assert_eq!(result, Value::String(StringRef::Owned("Hello, World!".into())));
    }
}
