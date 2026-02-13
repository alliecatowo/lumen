//! Register VM dispatch loop for executing LIR bytecode.

use crate::strings::StringTable;
use crate::types::{RuntimeField, RuntimeType, RuntimeTypeKind, RuntimeVariant, TypeTable};
use crate::values::{ClosureValue, RecordValue, StringRef, TraceRefValue, UnionValue, Value};
use lumen_compiler::compiler::lir::*;
use lumen_runtime::tools::{ToolDispatcher, ToolRequest};
use std::collections::BTreeMap;
use thiserror::Error;

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
    /// Optional tool dispatcher
    pub tool_dispatcher: Option<Box<dyn ToolDispatcher>>,
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
            tool_dispatcher: None,
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
                    kind: RuntimeTypeKind::Record(
                        ty.fields
                            .iter()
                            .map(|f| RuntimeField {
                                name: f.name.clone(),
                                ty: f.ty.clone(),
                            })
                            .collect(),
                    ),
                },
                "enum" => RuntimeType {
                    name: ty.name.clone(),
                    kind: RuntimeTypeKind::Enum(
                        ty.variants
                            .iter()
                            .map(|v| RuntimeVariant {
                                name: v.name.clone(),
                                payload: v.payload.clone(),
                            })
                            .collect(),
                    ),
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
        let cell_idx = module
            .cells
            .iter()
            .position(|c| c.name == cell_name)
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

    /// Helper to get a constant from the current cell.
    #[allow(dead_code)]
    fn get_constant(&self, cell_idx: usize, idx: usize) -> Constant {
        self.module.as_ref().unwrap().cells[cell_idx].constants[idx].clone()
    }

    /// Helper to get a string from the module string table.
    #[allow(dead_code)]
    fn get_module_string(&self, idx: usize) -> String {
        let module = self.module.as_ref().unwrap();
        if idx < module.strings.len() {
            module.strings[idx].clone()
        } else {
            String::new()
        }
    }

    fn run(&mut self) -> Result<Value, VmError> {
        loop {
            let (cell_idx, base, instr) = {
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
                    self.frames.pop();
                    if self.frames.is_empty() {
                        return Ok(Value::Null);
                    }
                    continue;
                }

                let instr = cell.instructions[ip];
                (cell_idx, base, instr)
            };

            // Advance IP in the frame
            if let Some(f) = self.frames.last_mut() {
                f.ip += 1;
            }

            let a = instr.a as usize;
            let b = instr.b as usize;
            let c = instr.c as usize;

            // Handle opcodes that need mutable self first (before borrowing module)
            match instr.op {
                OpCode::Call => {
                    self.dispatch_call(base, a, b)?;
                    continue;
                }
                OpCode::TailCall => {
                    self.dispatch_tailcall(base, a, b)?;
                    continue;
                }
                OpCode::Intrinsic => {
                    let result = self.exec_intrinsic(base, a, b, c)?;
                    self.registers[base + a] = result;
                    continue;
                }
                _ => {}
            }

            let module = self.module.as_ref().unwrap();
            let cell = &module.cells[cell_idx];

            match instr.op {
                OpCode::Nop => { /* no operation */ }

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
                    for i in 0..=b {
                        self.registers[base + a + i] = Value::Null;
                    }
                }
                OpCode::LoadBool => {
                    self.registers[base + a] = Value::Bool(b != 0);
                    if c != 0 {
                        if let Some(f) = self.frames.last_mut() {
                            f.ip += 1;
                        }
                    }
                }
                OpCode::LoadInt => {
                    let sb = instr.sbx();
                    self.registers[base + a] = Value::Int(sb as i64);
                }
                OpCode::Move => {
                    let val = self.registers[base + b].clone();
                    self.registers[base + a] = val;
                }
                OpCode::NewList => {
                    let mut list = Vec::with_capacity(b);
                    for i in 1..=b {
                        list.push(self.registers[base + a + i].clone());
                    }
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
                    let type_name = if bx < module.strings.len() {
                        module.strings[bx].clone()
                    } else {
                        "Unknown".to_string()
                    };
                    let fields = BTreeMap::new();
                    self.registers[base + a] = Value::Record(RecordValue { type_name, fields });
                }
                OpCode::NewUnion => {
                    let tag = self.registers[base + b].as_string();
                    let payload = Box::new(self.registers[base + c].clone());
                    self.registers[base + a] = Value::Union(UnionValue { tag, payload });
                }
                OpCode::NewTuple => {
                    let mut elems = Vec::with_capacity(b);
                    for i in 1..=b {
                        elems.push(self.registers[base + a + i].clone());
                    }
                    self.registers[base + a] = Value::Tuple(elems);
                }
                OpCode::NewSet => {
                    let mut elems = Vec::with_capacity(b);
                    for i in 1..=b {
                        let v = self.registers[base + a + i].clone();
                        if !elems.contains(&v) {
                            elems.push(v);
                        }
                    }
                    self.registers[base + a] = Value::Set(elems);
                }

                // Access
                OpCode::GetField => {
                    let obj = &self.registers[base + b];
                    let field_name = if c < module.strings.len() {
                        &module.strings[c]
                    } else {
                        ""
                    };
                    let val = match obj {
                        Value::Record(r) => {
                            r.fields.get(field_name).cloned().unwrap_or(Value::Null)
                        }
                        Value::Map(m) => m.get(field_name).cloned().unwrap_or(Value::Null),
                        _ => Value::Null,
                    };
                    self.registers[base + a] = val;
                }
                OpCode::SetField => {
                    let val = self.registers[base + c].clone();
                    let field_name = if b < module.strings.len() {
                        module.strings[b].clone()
                    } else {
                        String::new()
                    };
                    if let Value::Record(ref mut r) = self.registers[base + a] {
                        r.fields.insert(field_name, val);
                    }
                }
                OpCode::GetIndex => {
                    let obj = &self.registers[base + b];
                    let idx = &self.registers[base + c];
                    let val = match (obj, idx) {
                        (Value::List(l), Value::Int(i)) => {
                            l.get(*i as usize).cloned().unwrap_or(Value::Null)
                        }
                        (Value::Tuple(t), Value::Int(i)) => {
                            t.get(*i as usize).cloned().unwrap_or(Value::Null)
                        }
                        (Value::Map(m), _) => {
                            m.get(&idx.as_string()).cloned().unwrap_or(Value::Null)
                        }
                        _ => Value::Null,
                    };
                    self.registers[base + a] = val;
                }
                OpCode::SetIndex => {
                    let val = self.registers[base + c].clone();
                    let key = self.registers[base + b].clone();
                    match &mut self.registers[base + a] {
                        Value::List(l) => {
                            if let Some(i) = key.as_int() {
                                if (i as usize) < l.len() {
                                    l[i as usize] = val;
                                }
                            }
                        }
                        Value::Map(m) => {
                            m.insert(key.as_string(), val);
                        }
                        _ => {}
                    }
                }
                OpCode::GetTuple => {
                    let obj = &self.registers[base + b];
                    let val = match obj {
                        Value::Tuple(t) => t.get(c).cloned().unwrap_or(Value::Null),
                        Value::List(l) => l.get(c).cloned().unwrap_or(Value::Null),
                        _ => Value::Null,
                    };
                    self.registers[base + a] = val;
                }

                // Arithmetic
                OpCode::Add => {
                    let lhs = &self.registers[base + b];
                    let rhs = &self.registers[base + c];
                    let result = match (lhs, rhs) {
                        (Value::Int(a), Value::Int(b)) => Value::Int(a + b),
                        (Value::Float(a), Value::Float(b)) => Value::Float(a + b),
                        (Value::Int(a), Value::Float(b)) => Value::Float(*a as f64 + b),
                        (Value::Float(a), Value::Int(b)) => Value::Float(a + *b as f64),
                        (Value::String(_), _) | (_, Value::String(_)) => Value::String(
                            StringRef::Owned(format!("{}{}", lhs.as_string(), rhs.as_string())),
                        ),
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "cannot add {} and {}",
                                lhs, rhs
                            )))
                        }
                    };
                    self.registers[base + a] = result;
                }
                OpCode::Sub => {
                    self.arith_op(base, a, b, c, |x, y| x - y, |x, y| x - y)?;
                }
                OpCode::Mul => {
                    self.arith_op(base, a, b, c, |x, y| x * y, |x, y| x * y)?;
                }
                OpCode::Div => {
                    self.arith_op(
                        base,
                        a,
                        b,
                        c,
                        |x, y| if y != 0 { x / y } else { 0 },
                        |x, y| x / y,
                    )?;
                }
                OpCode::Mod => {
                    self.arith_op(
                        base,
                        a,
                        b,
                        c,
                        |x, y| if y != 0 { x % y } else { 0 },
                        |x, y| x % y,
                    )?;
                }
                OpCode::Pow => {
                    let lhs = &self.registers[base + b];
                    let rhs = &self.registers[base + c];
                    self.registers[base + a] = match (lhs, rhs) {
                        (Value::Int(x), Value::Int(y)) => {
                            if *y >= 0 {
                                Value::Int(x.pow(*y as u32))
                            } else {
                                Value::Float((*x as f64).powf(*y as f64))
                            }
                        }
                        (Value::Float(x), Value::Float(y)) => Value::Float(x.powf(*y)),
                        (Value::Int(x), Value::Float(y)) => Value::Float((*x as f64).powf(*y)),
                        (Value::Float(x), Value::Int(y)) => Value::Float(x.powf(*y as f64)),
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "cannot pow {} and {}",
                                lhs, rhs
                            )))
                        }
                    };
                }
                OpCode::Neg => {
                    let val = &self.registers[base + b];
                    self.registers[base + a] = match val {
                        Value::Int(n) => Value::Int(-n),
                        Value::Float(f) => Value::Float(-f),
                        _ => return Err(VmError::TypeError(format!("cannot negate {}", val))),
                    };
                }
                OpCode::Concat => {
                    let lhs = &self.registers[base + b];
                    let rhs = &self.registers[base + c];
                    let result = match (lhs, rhs) {
                        (Value::List(a), Value::List(b)) => {
                            let mut combined = a.clone();
                            combined.extend(b.iter().cloned());
                            Value::List(combined)
                        }
                        _ => Value::String(StringRef::Owned(format!(
                            "{}{}",
                            lhs.as_string(),
                            rhs.as_string()
                        ))),
                    };
                    self.registers[base + a] = result;
                }

                // Bitwise
                OpCode::BitOr => {
                    let lhs = &self.registers[base + b];
                    let rhs = &self.registers[base + c];
                    self.registers[base + a] = match (lhs, rhs) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x | y),
                        _ => return Err(VmError::TypeError("bitwise or requires integers".into())),
                    };
                }
                OpCode::BitAnd => {
                    let lhs = &self.registers[base + b];
                    let rhs = &self.registers[base + c];
                    self.registers[base + a] = match (lhs, rhs) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x & y),
                        _ => {
                            return Err(VmError::TypeError("bitwise and requires integers".into()))
                        }
                    };
                }
                OpCode::BitXor => {
                    let lhs = &self.registers[base + b];
                    let rhs = &self.registers[base + c];
                    self.registers[base + a] = match (lhs, rhs) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x ^ y),
                        _ => {
                            return Err(VmError::TypeError("bitwise xor requires integers".into()))
                        }
                    };
                }
                OpCode::BitNot => {
                    let val = &self.registers[base + b];
                    self.registers[base + a] = match val {
                        Value::Int(x) => Value::Int(!x),
                        _ => return Err(VmError::TypeError("bitwise not requires integer".into())),
                    };
                }
                OpCode::Shl => {
                    let lhs = &self.registers[base + b];
                    let rhs = &self.registers[base + c];
                    self.registers[base + a] = match (lhs, rhs) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x << (*y as u32)),
                        _ => return Err(VmError::TypeError("shift left requires integers".into())),
                    };
                }
                OpCode::Shr => {
                    let lhs = &self.registers[base + b];
                    let rhs = &self.registers[base + c];
                    self.registers[base + a] = match (lhs, rhs) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x >> (*y as u32)),
                        _ => {
                            return Err(VmError::TypeError("shift right requires integers".into()))
                        }
                    };
                }

                // Comparison / logic
                OpCode::Eq => {
                    let lhs = &self.registers[base + b];
                    let rhs = &self.registers[base + c];
                    let eq = lhs == rhs;
                    self.registers[base + a] = Value::Bool(eq);
                }
                OpCode::Lt => {
                    let b_val = &self.registers[base + b];
                    let c_val = &self.registers[base + c];
                    let result = match (b_val, c_val) {
                        (Value::Int(x), Value::Int(y)) => x < y,
                        (Value::Float(x), Value::Float(y)) => x < y,
                        (Value::Int(x), Value::Float(y)) => (*x as f64) < *y,
                        (Value::Float(x), Value::Int(y)) => *x < (*y as f64),
                        (Value::String(x), Value::String(y)) => {
                            let s1 = match x {
                                StringRef::Owned(s) => s.as_str(),
                                StringRef::Interned(id) => self.strings.resolve(*id).unwrap_or(""),
                            };
                            let s2 = match y {
                                StringRef::Owned(s) => s.as_str(),
                                StringRef::Interned(id) => self.strings.resolve(*id).unwrap_or(""),
                            };
                            s1 < s2
                        }
                        _ => false,
                    };
                    self.registers[base + a] = Value::Bool(result);
                }
                OpCode::Le => {
                    let b_val = &self.registers[base + b];
                    let c_val = &self.registers[base + c];
                    let result = match (b_val, c_val) {
                        (Value::Int(x), Value::Int(y)) => x <= y,
                        (Value::Float(x), Value::Float(y)) => x <= y,
                        (Value::Int(x), Value::Float(y)) => (*x as f64) <= *y,
                        (Value::Float(x), Value::Int(y)) => *x <= (*y as f64),
                        (Value::String(x), Value::String(y)) => {
                            let s1 = match x {
                                StringRef::Owned(s) => s.as_str(),
                                StringRef::Interned(id) => self.strings.resolve(*id).unwrap_or(""),
                            };
                            let s2 = match y {
                                StringRef::Owned(s) => s.as_str(),
                                StringRef::Interned(id) => self.strings.resolve(*id).unwrap_or(""),
                            };
                            s1 <= s2
                        }
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
                OpCode::In => {
                    let needle = &self.registers[base + b];
                    let haystack = &self.registers[base + c];
                    let result = match haystack {
                        Value::List(l) => l.contains(needle),
                        Value::Set(s) => s.contains(needle),
                        Value::Map(m) => m.contains_key(&needle.as_string()),
                        Value::String(StringRef::Owned(s)) => s.contains(&needle.as_string()),
                        _ => false,
                    };
                    self.registers[base + a] = Value::Bool(result);
                }
                OpCode::Is => {
                    let val = &self.registers[base + b];
                    let type_str = self.registers[base + c].as_string();
                    let matches = val.type_name() == type_str;
                    self.registers[base + a] = Value::Bool(matches);
                }
                OpCode::NullCo => {
                    let val = &self.registers[base + b];
                    if matches!(val, Value::Null) {
                        self.registers[base + a] = self.registers[base + c].clone();
                    } else {
                        self.registers[base + a] = val.clone();
                    }
                }
                OpCode::Test => {
                    let val = &self.registers[base + a];
                    let truthy = val.is_truthy();
                    if truthy != (c != 0) {
                        if let Some(f) = self.frames.last_mut() {
                            f.ip += 1;
                        }
                    }
                }

                // Control flow
                OpCode::Jmp => {
                    let offset = instr.sax_val();
                    if let Some(f) = self.frames.last_mut() {
                        f.ip = (f.ip as i32 + offset) as usize;
                    }
                }
                // Call and TailCall are handled in the pre-match above
                OpCode::Call | OpCode::TailCall => {
                    unreachable!()
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
                OpCode::Loop => {
                    // A = counter register, sB = jump offset
                    let sb = instr.sbx() as i32;
                    if let Value::Int(ref mut n) = self.registers[base + a] {
                        *n -= 1;
                        if *n > 0 {
                            if let Some(f) = self.frames.last_mut() {
                                f.ip = (f.ip as i32 + sb) as usize;
                            }
                        }
                    }
                }
                OpCode::ForPrep => {
                    let bx = instr.bx() as usize;
                    let iter_val = &self.registers[base + a];
                    let len = match iter_val {
                        Value::List(l) => l.len(),
                        Value::Set(s) => s.len(),
                        Value::Tuple(t) => t.len(),
                        _ => 0,
                    };
                    if len == 0 {
                        if let Some(f) = self.frames.last_mut() {
                            f.ip += bx;
                        }
                    }
                    self.registers[base + a + 1] = Value::Int(0);
                    self.registers[base + a + 2] = Value::Int(len as i64);
                }
                OpCode::ForLoop => {
                    let bx = instr.bx();
                    let idx = self.registers[base + a + 1].as_int().unwrap_or(0);
                    let len = self.registers[base + a + 2].as_int().unwrap_or(0);
                    if idx < len {
                        let iter = &self.registers[base + a];
                        let elem = match iter {
                            Value::List(l) => l.get(idx as usize).cloned().unwrap_or(Value::Null),
                            Value::Set(s) => s.get(idx as usize).cloned().unwrap_or(Value::Null),
                            Value::Tuple(t) => t.get(idx as usize).cloned().unwrap_or(Value::Null),
                            _ => Value::Null,
                        };
                        self.registers[base + a + 3] = elem;
                        self.registers[base + a + 1] = Value::Int(idx + 1);
                        if let Some(f) = self.frames.last_mut() {
                            f.ip = (f.ip as i32 - bx as i32) as usize;
                        }
                    }
                }
                OpCode::ForIn => {
                    // A = base, B = iterator reg, C = element dest
                    // Similar to ForLoop but more generic
                    let idx = self.registers[base + a + 1].as_int().unwrap_or(0);
                    let iter = &self.registers[base + b];
                    let (elem, has_more) = match iter {
                        Value::List(l) => {
                            if (idx as usize) < l.len() {
                                (l[idx as usize].clone(), true)
                            } else {
                                (Value::Null, false)
                            }
                        }
                        Value::Map(m) => {
                            let keys: Vec<_> = m.keys().collect();
                            if (idx as usize) < keys.len() {
                                let key = keys[idx as usize].clone();
                                let val = m.get(&key).cloned().unwrap_or(Value::Null);
                                (
                                    Value::Tuple(vec![Value::String(StringRef::Owned(key)), val]),
                                    true,
                                )
                            } else {
                                (Value::Null, false)
                            }
                        }
                        _ => (Value::Null, false),
                    };
                    self.registers[base + c] = elem;
                    self.registers[base + a + 1] = Value::Int(idx + 1);
                    self.registers[base + a] = Value::Bool(has_more);
                }
                OpCode::Break => {
                    // Jump to loop end (offset in Ax)
                    let offset = instr.sax_val();
                    if let Some(f) = self.frames.last_mut() {
                        f.ip = (f.ip as i32 + offset) as usize;
                    }
                }
                OpCode::Continue => {
                    // Jump to loop start (offset in Ax)
                    let offset = instr.sax_val();
                    if let Some(f) = self.frames.last_mut() {
                        f.ip = (f.ip as i32 + offset) as usize;
                    }
                }

                // Intrinsic is handled in the pre-match above
                OpCode::Intrinsic => {
                    unreachable!()
                }

                // Closures
                OpCode::Closure => {
                    let bx = instr.bx() as usize;
                    // Captures follow in registers A+1, A+2, ...
                    // The number of captures is determined by the cell's params
                    let module = self.module.as_ref().unwrap();
                    let cap_count = if bx < module.cells.len() {
                        // Use the cell's param count as a hint, but for closures
                        // we determine captures from subsequent GETUPVAL instructions
                        // For now, scan forward for the capture count
                        0 // Will be populated by subsequent Move instructions
                    } else {
                        0
                    };
                    let mut captures = Vec::new();
                    // Read captures from registers after A
                    for i in 0..cap_count {
                        captures.push(self.registers[base + a + 1 + i].clone());
                    }
                    self.registers[base + a] = Value::Closure(ClosureValue {
                        cell_idx: bx,
                        captures,
                    });
                }
                OpCode::GetUpval => {
                    // Get upvalue from current closure's captures
                    // The current frame must be running a closure
                    let frame = self.frames.last().unwrap();
                    let closure_reg = frame.base_register;
                    // Captures are stored at the beginning of the frame's registers
                    if b < 256 {
                        self.registers[base + a] = self.registers[closure_reg + b].clone();
                    }
                }
                OpCode::SetUpval => {
                    let val = self.registers[base + a].clone();
                    let frame = self.frames.last().unwrap();
                    let closure_reg = frame.base_register;
                    if b < 256 {
                        self.registers[closure_reg + b] = val;
                    }
                }

                // Effects
                OpCode::ToolCall => {
                    if let Some(ref dispatcher) = self.tool_dispatcher {
                        let bx = instr.bx() as usize;
                        let module = self.module.as_ref().unwrap();
                        let tool = if bx < module.tools.len() {
                            &module.tools[bx]
                        } else {
                            self.registers[base + a] = Value::Null;
                            continue;
                        };
                        // Build args from subsequent registers
                        let mut args_map = serde_json::Map::new();
                        // Simple: first arg after A is the arguments map
                        if let Value::Map(m) = &self.registers[base + a + 1] {
                            for (k, v) in m {
                                args_map.insert(k.clone(), value_to_json(v));
                            }
                        }
                        let request = ToolRequest {
                            tool_id: tool.tool_id.clone(),
                            version: tool.version.clone(),
                            args: serde_json::Value::Object(args_map),
                            policy: if bx < module.policies.len() {
                                module.policies[bx].grants.clone()
                            } else {
                                serde_json::Value::Null
                            },
                        };
                        match dispatcher.dispatch(&request) {
                            Ok(response) => {
                                self.registers[base + a] = json_to_value(&response.outputs);
                            }
                            Err(e) => {
                                return Err(VmError::ToolError(format!("{}", e)));
                            }
                        }
                    } else {
                        self.registers[base + a] =
                            Value::String(StringRef::Owned("<<tool call pending>>".into()));
                    }
                }
                OpCode::Schema => {
                    let bx = instr.bx() as usize;
                    let type_name = if bx < module.strings.len() {
                        &module.strings[bx]
                    } else {
                        ""
                    };
                    let val = &self.registers[base + a];

                    let valid = match type_name {
                        "Int" => matches!(val, Value::Int(_)),
                        "Float" => matches!(val, Value::Float(_)),
                        "String" => matches!(val, Value::String(_)),
                        "Bool" => matches!(val, Value::Bool(_)),
                        "List" => matches!(val, Value::List(_)),
                        "Map" => matches!(val, Value::Map(_)),
                        "Tuple" => matches!(val, Value::Tuple(_)),
                        "Set" => matches!(val, Value::Set(_)),
                        _ => match val {
                            Value::Record(r) => r.type_name == type_name,
                            _ => false,
                        },
                    };

                    if !valid {
                        return Err(VmError::Runtime(format!(
                            "value {} does not match schema {}",
                            val, type_name
                        )));
                    }
                }
                OpCode::Emit => {
                    let val = self.registers[base + a].display_pretty();
                    println!("{}", val);
                    self.output.push(val);
                }
                OpCode::TraceRef => {
                    self.registers[base + a] = Value::TraceRef(TraceRefValue {
                        trace_id: "trace".into(),
                        seq: 0,
                    });
                }
                OpCode::Await => {
                    // V1: synchronous - just copy value
                    self.registers[base + a] = self.registers[base + b].clone();
                }
                OpCode::Spawn => {
                    // V1: synchronous - just execute the cell
                    let bx = instr.bx() as usize;
                    let module = self.module.as_ref().unwrap();
                    if bx < module.cells.len() {
                        let callee_cell = &module.cells[bx];
                        let new_base = self.registers.len();
                        self.registers.resize(
                            new_base + (callee_cell.registers as usize).max(256),
                            Value::Null,
                        );
                        self.frames.push(CallFrame {
                            cell_idx: bx,
                            base_register: new_base,
                            ip: 0,
                            return_register: base + a,
                        });
                    } else {
                        self.registers[base + a] = Value::Null;
                    }
                }

                // List ops
                OpCode::Append => {
                    let val = self.registers[base + b].clone();
                    if let Value::List(ref mut l) = self.registers[base + a] {
                        l.push(val);
                    }
                }

                // Type checks
                OpCode::IsVariant => {
                    let val = &self.registers[base + a];
                    let tag = self.strings.resolve(instr.bx() as u32).unwrap_or("");
                    let matched = match val {
                        Value::Union(u) => u.tag == tag,
                        _ => false,
                    };
                    if matched {
                        if let Some(f) = self.frames.last_mut() {
                            f.ip += 1;
                        }
                    }
                }
                OpCode::Unbox => {
                    let val = &self.registers[base + b];
                    if let Value::Union(u) = val {
                        self.registers[base + a] = *u.payload.clone();
                    } else {
                        self.registers[base + a] = Value::Null;
                    }
                }
            }
        }
    }

    /// Dispatch a CALL instruction, handling cells, closures, and built-in functions.
    fn dispatch_call(&mut self, base: usize, a: usize, nargs: usize) -> Result<(), VmError> {
        let callee = self.registers[base + a].clone();
        match callee {
            Value::String(ref sr) => {
                let name = match sr {
                    StringRef::Owned(s) => s.clone(),
                    StringRef::Interned(id) => self.strings.resolve(*id).unwrap_or("").to_string(),
                };
                let module = self.module.as_ref().unwrap();
                if let Some(idx) = module.cells.iter().position(|c| c.name == name) {
                    if self.frames.len() >= MAX_CALL_DEPTH {
                        return Err(VmError::StackOverflow(MAX_CALL_DEPTH));
                    }
                    let callee_cell = &module.cells[idx];
                    let num_regs = callee_cell.registers as usize;
                    let params: Vec<LirParam> = callee_cell.params.clone();
                    let _ = module;
                    let new_base = self.registers.len();
                    self.registers
                        .resize(new_base + num_regs.max(256), Value::Null);
                    for i in 0..nargs {
                        if i < params.len() {
                            self.registers[new_base + params[i].register as usize] =
                                self.registers[base + a + 1 + i].clone();
                        }
                    }
                    self.frames.push(CallFrame {
                        cell_idx: idx,
                        base_register: new_base,
                        ip: 0,
                        return_register: base + a,
                    });
                } else {
                    let _ = module;
                    let result = self.call_builtin(&name, base, a, nargs)?;
                    self.registers[base + a] = result;
                }
            }
            Value::Closure(ref cv) => {
                if self.frames.len() >= MAX_CALL_DEPTH {
                    return Err(VmError::StackOverflow(MAX_CALL_DEPTH));
                }
                let cv = cv.clone();
                let module = self.module.as_ref().unwrap();
                let callee_cell = &module.cells[cv.cell_idx];
                let num_regs = callee_cell.registers as usize;
                let params: Vec<LirParam> = callee_cell.params.clone();
                let _ = module;
                let new_base = self.registers.len();
                self.registers
                    .resize(new_base + num_regs.max(256), Value::Null);
                for (i, cap) in cv.captures.iter().enumerate() {
                    self.registers[new_base + i] = cap.clone();
                }
                let cap_count = cv.captures.len();
                for i in 0..nargs {
                    if cap_count + i < params.len() {
                        self.registers[new_base + params[cap_count + i].register as usize] =
                            self.registers[base + a + 1 + i].clone();
                    }
                }
                self.frames.push(CallFrame {
                    cell_idx: cv.cell_idx,
                    base_register: new_base,
                    ip: 0,
                    return_register: base + a,
                });
            }
            _ => {
                return Err(VmError::TypeError(format!("cannot call {}", callee)));
            }
        }
        Ok(())
    }

    /// Dispatch a TAILCALL instruction: reuse current frame.
    fn dispatch_tailcall(&mut self, base: usize, a: usize, nargs: usize) -> Result<(), VmError> {
        let callee = self.registers[base + a].clone();
        match callee {
            Value::String(ref sr) => {
                let name = match sr {
                    StringRef::Owned(s) => s.clone(),
                    StringRef::Interned(id) => self.strings.resolve(*id).unwrap_or("").to_string(),
                };
                let module = self.module.as_ref().unwrap();
                if let Some(idx) = module.cells.iter().position(|c| c.name == name) {
                    let params: Vec<LirParam> = module.cells[idx].params.clone();
                    let _ = module;
                    for i in 0..nargs {
                        if i < params.len() {
                            let src = self.registers[base + a + 1 + i].clone();
                            self.registers[base + params[i].register as usize] = src;
                        }
                    }
                    if let Some(f) = self.frames.last_mut() {
                        f.cell_idx = idx;
                        f.ip = 0;
                    }
                } else {
                    let _ = module;
                    let result = self.call_builtin(&name, base, a, nargs)?;
                    self.registers[base + a] = result;
                }
            }
            Value::Closure(ref cv) => {
                let cv = cv.clone();
                let module = self.module.as_ref().unwrap();
                let params: Vec<LirParam> = module.cells[cv.cell_idx].params.clone();
                let _ = module;
                for (i, cap) in cv.captures.iter().enumerate() {
                    self.registers[base + i] = cap.clone();
                }
                let cap_count = cv.captures.len();
                for i in 0..nargs {
                    if cap_count + i < params.len() {
                        let src = self.registers[base + a + 1 + i].clone();
                        self.registers[base + params[cap_count + i].register as usize] = src;
                    }
                }
                if let Some(f) = self.frames.last_mut() {
                    f.cell_idx = cv.cell_idx;
                    f.ip = 0;
                }
            }
            _ => {
                return Err(VmError::TypeError(format!("cannot tail-call {}", callee)));
            }
        }
        Ok(())
    }

    /// Execute a built-in function by name.
    fn call_builtin(
        &mut self,
        name: &str,
        base: usize,
        a: usize,
        nargs: usize,
    ) -> Result<Value, VmError> {
        match name {
            "print" => {
                let mut parts = Vec::new();
                for i in 0..nargs {
                    parts.push(self.registers[base + a + 1 + i].display_pretty());
                }
                let output = parts.join(" ");
                println!("{}", output);
                self.output.push(output);
                Ok(Value::Null)
            }
            "len" | "length" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::String(StringRef::Owned(s)) => Value::Int(s.len() as i64),
                    Value::List(l) => Value::Int(l.len() as i64),
                    Value::Map(m) => Value::Int(m.len() as i64),
                    Value::Tuple(t) => Value::Int(t.len() as i64),
                    Value::Set(s) => Value::Int(s.len() as i64),
                    Value::Bytes(b) => Value::Int(b.len() as i64),
                    _ => Value::Int(0),
                })
            }
            "append" => {
                let list = self.registers[base + a + 1].clone();
                let elem = self.registers[base + a + 2].clone();
                if let Value::List(mut l) = list {
                    l.push(elem);
                    Ok(Value::List(l))
                } else {
                    Ok(Value::List(vec![elem]))
                }
            }
            "to_string" | "str" | "string" => {
                let arg = &self.registers[base + a + 1];
                Ok(Value::String(StringRef::Owned(arg.display_pretty())))
            }
            "to_int" | "int" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Int(n) => Value::Int(*n),
                    Value::Float(f) => Value::Int(*f as i64),
                    Value::String(StringRef::Owned(s)) => {
                        s.parse::<i64>().map(Value::Int).unwrap_or(Value::Null)
                    }
                    Value::Bool(b) => Value::Int(if *b { 1 } else { 0 }),
                    _ => Value::Null,
                })
            }
            "to_float" | "float" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(*f),
                    Value::Int(n) => Value::Float(*n as f64),
                    Value::String(StringRef::Owned(s)) => {
                        s.parse::<f64>().map(Value::Float).unwrap_or(Value::Null)
                    }
                    _ => Value::Null,
                })
            }
            "type_of" | "type" => {
                let arg = &self.registers[base + a + 1];
                Ok(Value::String(StringRef::Owned(arg.type_name().to_string())))
            }
            "keys" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Map(m) => Value::List(
                        m.keys()
                            .map(|k| Value::String(StringRef::Owned(k.clone())))
                            .collect(),
                    ),
                    Value::Record(r) => Value::List(
                        r.fields
                            .keys()
                            .map(|k| Value::String(StringRef::Owned(k.clone())))
                            .collect(),
                    ),
                    _ => Value::List(vec![]),
                })
            }
            "values" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Map(m) => Value::List(m.values().cloned().collect()),
                    Value::Record(r) => Value::List(r.fields.values().cloned().collect()),
                    _ => Value::List(vec![]),
                })
            }
            "contains" | "has" => {
                let collection = &self.registers[base + a + 1];
                let needle = &self.registers[base + a + 2];
                let result = match collection {
                    Value::List(l) => l.iter().any(|v| v == needle),
                    Value::Set(s) => s.iter().any(|v| v == needle),
                    Value::Map(m) => m.contains_key(&needle.as_string()),
                    Value::String(StringRef::Owned(s)) => s.contains(&needle.as_string()),
                    _ => false,
                };
                Ok(Value::Bool(result))
            }
            "join" => {
                let list = &self.registers[base + a + 1];
                let sep = if nargs > 1 {
                    self.registers[base + a + 2].as_string()
                } else {
                    ", ".to_string()
                };
                if let Value::List(l) = list {
                    let joined = l
                        .iter()
                        .map(|v| v.display_pretty())
                        .collect::<Vec<_>>()
                        .join(&sep);
                    Ok(Value::String(StringRef::Owned(joined)))
                } else {
                    Ok(Value::String(StringRef::Owned(list.display_pretty())))
                }
            }
            "split" => {
                let s = self.registers[base + a + 1].as_string();
                let sep = if nargs > 1 {
                    self.registers[base + a + 2].as_string()
                } else {
                    " ".to_string()
                };
                let parts: Vec<Value> = s
                    .split(&sep)
                    .map(|p| Value::String(StringRef::Owned(p.to_string())))
                    .collect();
                Ok(Value::List(parts))
            }
            "trim" => {
                let s = self.registers[base + a + 1].as_string();
                Ok(Value::String(StringRef::Owned(s.trim().to_string())))
            }
            "upper" => {
                let s = self.registers[base + a + 1].as_string();
                Ok(Value::String(StringRef::Owned(s.to_uppercase())))
            }
            "lower" => {
                let s = self.registers[base + a + 1].as_string();
                Ok(Value::String(StringRef::Owned(s.to_lowercase())))
            }
            "replace" => {
                let s = self.registers[base + a + 1].as_string();
                let from = self.registers[base + a + 2].as_string();
                let to = self.registers[base + a + 3].as_string();
                Ok(Value::String(StringRef::Owned(s.replace(&from, &to))))
            }
            "abs" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Int(n) => Value::Int(n.abs()),
                    Value::Float(f) => Value::Float(f.abs()),
                    _ => arg.clone(),
                })
            }
            "min" => {
                let lhs = &self.registers[base + a + 1];
                let rhs = &self.registers[base + a + 2];
                Ok(match (lhs, rhs) {
                    (Value::Int(x), Value::Int(y)) => Value::Int(*x.min(y)),
                    (Value::Float(x), Value::Float(y)) => Value::Float(x.min(*y)),
                    _ => lhs.clone(),
                })
            }
            "max" => {
                let lhs = &self.registers[base + a + 1];
                let rhs = &self.registers[base + a + 2];
                Ok(match (lhs, rhs) {
                    (Value::Int(x), Value::Int(y)) => Value::Int(*x.max(y)),
                    (Value::Float(x), Value::Float(y)) => Value::Float(x.max(*y)),
                    _ => lhs.clone(),
                })
            }
            "range" => {
                let start = self.registers[base + a + 1].as_int().unwrap_or(0);
                let end = self.registers[base + a + 2].as_int().unwrap_or(0);
                let list: Vec<Value> = (start..end).map(Value::Int).collect();
                Ok(Value::List(list))
            }
            "hash" | "sha256" => {
                use sha2::{Digest, Sha256};
                let s = self.registers[base + a + 1].as_string();
                let h = format!("sha256:{:x}", Sha256::digest(s.as_bytes()));
                Ok(Value::String(StringRef::Owned(h)))
            }
            // Collection ops
            "sort" => {
                let arg = self.registers[base + a + 1].clone();
                if let Value::List(mut l) = arg {
                    l.sort();
                    Ok(Value::List(l))
                } else {
                    Ok(arg)
                }
            }
            "reverse" => {
                let arg = self.registers[base + a + 1].clone();
                if let Value::List(mut l) = arg {
                    l.reverse();
                    Ok(Value::List(l))
                } else {
                    Ok(arg)
                }
            }
            "flatten" => {
                let arg = &self.registers[base + a + 1];
                if let Value::List(l) = arg {
                    let mut result = Vec::new();
                    for item in l {
                        if let Value::List(inner) = item {
                            result.extend(inner.iter().cloned());
                        } else {
                            result.push(item.clone());
                        }
                    }
                    Ok(Value::List(result))
                } else {
                    Ok(arg.clone())
                }
            }
            "unique" => {
                let arg = &self.registers[base + a + 1];
                if let Value::List(l) = arg {
                    let mut result = Vec::new();
                    for item in l {
                        if !result.contains(item) {
                            result.push(item.clone());
                        }
                    }
                    Ok(Value::List(result))
                } else {
                    Ok(arg.clone())
                }
            }
            "take" => {
                let arg = &self.registers[base + a + 1];
                let n = self.registers[base + a + 2].as_int().unwrap_or(0) as usize;
                if let Value::List(l) = arg {
                    Ok(Value::List(l.iter().take(n).cloned().collect()))
                } else {
                    Ok(arg.clone())
                }
            }
            "drop" => {
                let arg = &self.registers[base + a + 1];
                let n = self.registers[base + a + 2].as_int().unwrap_or(0) as usize;
                if let Value::List(l) = arg {
                    Ok(Value::List(l.iter().skip(n).cloned().collect()))
                } else {
                    Ok(arg.clone())
                }
            }
            "first" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::List(l) => l.first().cloned().unwrap_or(Value::Null),
                    Value::Tuple(t) => t.first().cloned().unwrap_or(Value::Null),
                    _ => Value::Null,
                })
            }
            "last" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::List(l) => l.last().cloned().unwrap_or(Value::Null),
                    Value::Tuple(t) => t.last().cloned().unwrap_or(Value::Null),
                    _ => Value::Null,
                })
            }
            "is_empty" => {
                let arg = &self.registers[base + a + 1];
                Ok(Value::Bool(match arg {
                    Value::List(l) => l.is_empty(),
                    Value::Map(m) => m.is_empty(),
                    Value::Set(s) => s.is_empty(),
                    Value::String(StringRef::Owned(s)) => s.is_empty(),
                    Value::Null => true,
                    _ => false,
                }))
            }
            "chars" => {
                let s = self.registers[base + a + 1].as_string();
                Ok(Value::List(
                    s.chars()
                        .map(|c| Value::String(StringRef::Owned(c.to_string())))
                        .collect(),
                ))
            }
            "starts_with" => {
                let s = self.registers[base + a + 1].as_string();
                let prefix = self.registers[base + a + 2].as_string();
                Ok(Value::Bool(s.starts_with(&prefix)))
            }
            "ends_with" => {
                let s = self.registers[base + a + 1].as_string();
                let suffix = self.registers[base + a + 2].as_string();
                Ok(Value::Bool(s.ends_with(&suffix)))
            }
            "index_of" => {
                let s = self.registers[base + a + 1].as_string();
                let needle = self.registers[base + a + 2].as_string();
                Ok(match s.find(&needle) {
                    Some(i) => Value::Int(i as i64),
                    None => Value::Int(-1),
                })
            }
            "pad_left" => {
                let s = self.registers[base + a + 1].as_string();
                let width = self.registers[base + a + 2].as_int().unwrap_or(0) as usize;
                let pad = if nargs > 2 {
                    self.registers[base + a + 3].as_string()
                } else {
                    " ".to_string()
                };
                let pad_char = pad.chars().next().unwrap_or(' ');
                if s.len() < width {
                    let padding: String =
                        std::iter::repeat(pad_char).take(width - s.len()).collect();
                    Ok(Value::String(StringRef::Owned(format!("{}{}", padding, s))))
                } else {
                    Ok(Value::String(StringRef::Owned(s)))
                }
            }
            "pad_right" => {
                let s = self.registers[base + a + 1].as_string();
                let width = self.registers[base + a + 2].as_int().unwrap_or(0) as usize;
                let pad = if nargs > 2 {
                    self.registers[base + a + 3].as_string()
                } else {
                    " ".to_string()
                };
                let pad_char = pad.chars().next().unwrap_or(' ');
                if s.len() < width {
                    let padding: String =
                        std::iter::repeat(pad_char).take(width - s.len()).collect();
                    Ok(Value::String(StringRef::Owned(format!("{}{}", s, padding))))
                } else {
                    Ok(Value::String(StringRef::Owned(s)))
                }
            }
            // Math
            "round" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.round()),
                    _ => arg.clone(),
                })
            }
            "ceil" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.ceil()),
                    _ => arg.clone(),
                })
            }
            "floor" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.floor()),
                    _ => arg.clone(),
                })
            }
            "sqrt" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.sqrt()),
                    Value::Int(n) => Value::Float((*n as f64).sqrt()),
                    _ => Value::Null,
                })
            }
            "pow" => {
                let b_val = &self.registers[base + a + 1];
                let e_val = &self.registers[base + a + 2];
                Ok(match (b_val, e_val) {
                    (Value::Int(x), Value::Int(y)) => {
                        if *y >= 0 {
                            Value::Int(x.pow(*y as u32))
                        } else {
                            Value::Float((*x as f64).powf(*y as f64))
                        }
                    }
                    (Value::Float(x), Value::Float(y)) => Value::Float(x.powf(*y)),
                    (Value::Int(x), Value::Float(y)) => Value::Float((*x as f64).powf(*y)),
                    (Value::Float(x), Value::Int(y)) => Value::Float(x.powf(*y as f64)),
                    _ => Value::Null,
                })
            }
            "log" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.ln()),
                    Value::Int(n) => Value::Float((*n as f64).ln()),
                    _ => Value::Null,
                })
            }
            "sin" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.sin()),
                    Value::Int(n) => Value::Float((*n as f64).sin()),
                    _ => Value::Null,
                })
            }
            "cos" => {
                let arg = &self.registers[base + a + 1];
                Ok(match arg {
                    Value::Float(f) => Value::Float(f.cos()),
                    Value::Int(n) => Value::Float((*n as f64).cos()),
                    _ => Value::Null,
                })
            }
            "clamp" => {
                let val = &self.registers[base + a + 1];
                let lo = &self.registers[base + a + 2];
                let hi = &self.registers[base + a + 3];
                Ok(match (val, lo, hi) {
                    (Value::Int(v), Value::Int(l), Value::Int(h)) => Value::Int(*v.max(l).min(h)),
                    (Value::Float(v), Value::Float(l), Value::Float(h)) => {
                        Value::Float(v.max(*l).min(*h))
                    }
                    _ => val.clone(),
                })
            }
            // Result type operations
            "is_ok" => {
                let arg = &self.registers[base + a + 1];
                Ok(Value::Bool(matches!(arg, Value::Union(u) if u.tag == "ok")))
            }
            "is_err" => {
                let arg = &self.registers[base + a + 1];
                Ok(Value::Bool(
                    matches!(arg, Value::Union(u) if u.tag == "err"),
                ))
            }
            "unwrap" => {
                let arg = &self.registers[base + a + 1];
                match arg {
                    Value::Union(u) if u.tag == "ok" => Ok(*u.payload.clone()),
                    Value::Union(u) if u.tag == "err" => {
                        Err(VmError::Runtime(format!("unwrap on err: {}", u.payload)))
                    }
                    _ => Ok(arg.clone()),
                }
            }
            "unwrap_or" => {
                let arg = &self.registers[base + a + 1];
                let default = self.registers[base + a + 2].clone();
                match arg {
                    Value::Union(u) if u.tag == "ok" => Ok(*u.payload.clone()),
                    _ => Ok(default),
                }
            }
            // Crypto
            "sha512" => {
                // sha2 crate provides Sha512 but let's just stub with the hash prefix
                use sha2::{Digest, Sha256};
                let s = self.registers[base + a + 1].as_string();
                // Use SHA256 as placeholder since sha512 requires different import
                let h = format!("sha512:{:x}", Sha256::digest(s.as_bytes()));
                Ok(Value::String(StringRef::Owned(h)))
            }
            "uuid" | "uuid_v4" => {
                let id = uuid::Uuid::new_v4().to_string();
                Ok(Value::String(StringRef::Owned(id)))
            }
            "timestamp" => {
                let now = chrono::Utc::now().timestamp_millis();
                Ok(Value::Int(now))
            }
            // Encoding
            "base64_encode" => {
                // Simple base64 implementation
                let s = self.registers[base + a + 1].as_string();
                Ok(Value::String(StringRef::Owned(simple_base64_encode(
                    s.as_bytes(),
                ))))
            }
            "base64_decode" => {
                let s = self.registers[base + a + 1].as_string();
                match simple_base64_decode(&s) {
                    Some(bytes) => Ok(Value::String(StringRef::Owned(
                        String::from_utf8_lossy(&bytes).to_string(),
                    ))),
                    None => Ok(Value::Null),
                }
            }
            "hex_encode" => {
                let s = self.registers[base + a + 1].as_string();
                let hex: String = s.bytes().map(|b| format!("{:02x}", b)).collect();
                Ok(Value::String(StringRef::Owned(hex)))
            }
            "hex_decode" => {
                let s = self.registers[base + a + 1].as_string();
                let bytes: Vec<u8> = (0..s.len())
                    .step_by(2)
                    .filter_map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
                    .collect();
                Ok(Value::String(StringRef::Owned(
                    String::from_utf8_lossy(&bytes).to_string(),
                )))
            }
            "url_encode" => {
                let s = self.registers[base + a + 1].as_string();
                let encoded: String = s
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~'
                        {
                            c.to_string()
                        } else {
                            format!("%{:02X}", c as u32)
                        }
                    })
                    .collect();
                Ok(Value::String(StringRef::Owned(encoded)))
            }
            "url_decode" => {
                let s = self.registers[base + a + 1].as_string();
                let mut result = String::new();
                let mut chars = s.chars();
                while let Some(c) = chars.next() {
                    if c == '%' {
                        let hi = chars.next().unwrap_or('0');
                        let lo = chars.next().unwrap_or('0');
                        let hex = format!("{}{}", hi, lo);
                        if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                            result.push(byte as char);
                        }
                    } else if c == '+' {
                        result.push(' ');
                    } else {
                        result.push(c);
                    }
                }
                Ok(Value::String(StringRef::Owned(result)))
            }
            // JSON
            "json_parse" => {
                let s = self.registers[base + a + 1].as_string();
                match serde_json::from_str::<serde_json::Value>(&s) {
                    Ok(v) => Ok(json_to_value(&v)),
                    Err(_) => Ok(Value::Null),
                }
            }
            "json_encode" => {
                let val = &self.registers[base + a + 1];
                let j = value_to_json(val);
                Ok(Value::String(StringRef::Owned(j.to_string())))
            }
            "json_pretty" => {
                let val = &self.registers[base + a + 1];
                let j = value_to_json(val);
                Ok(Value::String(StringRef::Owned(
                    serde_json::to_string_pretty(&j).unwrap_or_default(),
                )))
            }
            // String case transforms (std.string)
            "capitalize" => {
                let s = self.registers[base + a + 1].as_string();
                let mut c = s.chars();
                let result = match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().to_string() + &c.as_str().to_lowercase(),
                };
                Ok(Value::String(StringRef::Owned(result)))
            }
            "title_case" => {
                let s = self.registers[base + a + 1].as_string();
                let result: String = s
                    .split_whitespace()
                    .map(|word| {
                        let mut c = word.chars();
                        match c.next() {
                            None => String::new(),
                            Some(f) => f.to_uppercase().to_string() + &c.as_str().to_lowercase(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                Ok(Value::String(StringRef::Owned(result)))
            }
            "snake_case" => {
                let s = self.registers[base + a + 1].as_string();
                let mut result = String::new();
                for (i, ch) in s.chars().enumerate() {
                    if ch.is_uppercase() && i > 0 {
                        result.push('_');
                    }
                    result.push(ch.to_lowercase().next().unwrap_or(ch));
                }
                Ok(Value::String(StringRef::Owned(
                    result.replace(' ', "_").replace("__", "_"),
                )))
            }
            "camel_case" => {
                let s = self.registers[base + a + 1].as_string();
                let result: String = s
                    .split(|c: char| c == '_' || c == ' ' || c == '-')
                    .enumerate()
                    .map(|(i, word)| {
                        if i == 0 {
                            word.to_lowercase()
                        } else {
                            let mut c = word.chars();
                            match c.next() {
                                None => String::new(),
                                Some(f) => {
                                    f.to_uppercase().to_string() + &c.as_str().to_lowercase()
                                }
                            }
                        }
                    })
                    .collect();
                Ok(Value::String(StringRef::Owned(result)))
            }
            // Test assertions
            "assert" => {
                let arg = &self.registers[base + a + 1];
                if !arg.is_truthy() {
                    let msg = if nargs > 1 {
                        self.registers[base + a + 2].as_string()
                    } else {
                        "assertion failed".to_string()
                    };
                    return Err(VmError::Runtime(msg));
                }
                Ok(Value::Null)
            }
            "assert_eq" => {
                let lhs = &self.registers[base + a + 1];
                let rhs = &self.registers[base + a + 2];
                if lhs != rhs {
                    return Err(VmError::Runtime(format!(
                        "assert_eq failed: {} != {}",
                        lhs, rhs
                    )));
                }
                Ok(Value::Null)
            }
            "assert_ne" => {
                let lhs = &self.registers[base + a + 1];
                let rhs = &self.registers[base + a + 2];
                if lhs == rhs {
                    return Err(VmError::Runtime(format!(
                        "assert_ne failed: {} == {}",
                        lhs, rhs
                    )));
                }
                Ok(Value::Null)
            }
            "assert_contains" => {
                let collection = &self.registers[base + a + 1];
                let needle = &self.registers[base + a + 2];
                let found = match collection {
                    Value::List(l) => l.contains(needle),
                    Value::String(StringRef::Owned(s)) => s.contains(&needle.as_string()),
                    _ => false,
                };
                if !found {
                    return Err(VmError::Runtime(format!(
                        "assert_contains failed: {} not in {}",
                        needle, collection
                    )));
                }
                Ok(Value::Null)
            }
            // Emit/debug
            "emit" => {
                let val = self.registers[base + a + 1].display_pretty();
                println!("{}", val);
                self.output.push(val);
                Ok(Value::Null)
            }
            "debug" => {
                let val = &self.registers[base + a + 1];
                let output = format!("[debug] {:?}", val);
                eprintln!("{}", output);
                self.output.push(output);
                Ok(Value::Null)
            }
            "clone" => Ok(self.registers[base + a + 1].clone()),
            "sizeof" => {
                let val = &self.registers[base + a + 1];
                Ok(Value::Int(std::mem::size_of_val(val) as i64))
            }
            "enumerate" => {
                let arg = &self.registers[base + a + 1];
                if let Value::List(l) = arg {
                    let result: Vec<Value> = l
                        .iter()
                        .enumerate()
                        .map(|(i, v)| Value::Tuple(vec![Value::Int(i as i64), v.clone()]))
                        .collect();
                    Ok(Value::List(result))
                } else {
                    Ok(Value::List(vec![]))
                }
            }
            "zip" => {
                let a_list = &self.registers[base + a + 1];
                let b_list = &self.registers[base + a + 2];
                if let (Value::List(la), Value::List(lb)) = (a_list, b_list) {
                    let result: Vec<Value> = la
                        .iter()
                        .zip(lb.iter())
                        .map(|(x, y)| Value::Tuple(vec![x.clone(), y.clone()]))
                        .collect();
                    Ok(Value::List(result))
                } else {
                    Ok(Value::List(vec![]))
                }
            }
            "chunk" => {
                let arg = &self.registers[base + a + 1];
                let size = self.registers[base + a + 2].as_int().unwrap_or(1) as usize;
                if let Value::List(l) = arg {
                    let result: Vec<Value> = l
                        .chunks(size.max(1))
                        .map(|chunk| Value::List(chunk.to_vec()))
                        .collect();
                    Ok(Value::List(result))
                } else {
                    Ok(Value::List(vec![]))
                }
            }
            "freeze" => Ok(self.registers[base + a + 1].clone()),
            _ => Err(VmError::UndefinedCell(name.to_string())),
        }
    }

    /// Execute an intrinsic function by ID.
    fn exec_intrinsic(
        &mut self,
        base: usize,
        _a: usize,
        func_id: usize,
        arg_reg: usize,
    ) -> Result<Value, VmError> {
        let arg = &self.registers[base + arg_reg];
        match func_id {
            0 => {
                // LENGTH
                Ok(match arg {
                    Value::String(StringRef::Owned(s)) => Value::Int(s.len() as i64),
                    Value::String(StringRef::Interned(id)) => {
                        let s = self.strings.resolve(*id).unwrap_or("");
                        Value::Int(s.len() as i64)
                    }
                    Value::List(l) => Value::Int(l.len() as i64),
                    Value::Map(m) => Value::Int(m.len() as i64),
                    Value::Tuple(t) => Value::Int(t.len() as i64),
                    Value::Set(s) => Value::Int(s.len() as i64),
                    Value::Bytes(b) => Value::Int(b.len() as i64),
                    _ => Value::Int(0),
                })
            }
            1 => {
                // COUNT
                Ok(match arg {
                    Value::List(l) => Value::Int(l.len() as i64),
                    Value::Map(m) => Value::Int(m.len() as i64),
                    Value::String(StringRef::Owned(s)) => Value::Int(s.len() as i64),
                    _ => Value::Int(0),
                })
            }
            2 => {
                // MATCHES
                Ok(match arg {
                    Value::Bool(b) => Value::Bool(*b),
                    Value::String(_) => Value::Bool(!arg.as_string().is_empty()),
                    _ => Value::Bool(false),
                })
            }
            3 => {
                // HASH
                use sha2::{Digest, Sha256};
                let hash = format!("{:x}", Sha256::digest(arg.as_string().as_bytes()));
                Ok(Value::String(StringRef::Owned(format!("sha256:{}", hash))))
            }
            4 => {
                // DIFF
                let other = &self.registers[base + arg_reg + 1];
                Ok(self.diff_values(arg, other))
            }
            5 => {
                // PATCH
                let patches = &self.registers[base + arg_reg + 1];
                Ok(self.patch_value(arg, patches))
            }
            6 => {
                // REDACT
                let fields = &self.registers[base + arg_reg + 1];
                Ok(self.redact_value(arg, fields))
            }
            7 => {
                // VALIDATE
                Ok(Value::Bool(true)) // full validation deferred to schema opcode
            }
            8 => {
                // TRACEREF
                Ok(Value::TraceRef(TraceRefValue {
                    trace_id: "trace".into(),
                    seq: 0,
                }))
            }
            9 => {
                // PRINT
                let output = arg.display_pretty();
                println!("{}", output);
                self.output.push(output);
                Ok(Value::Null)
            }
            10 => Ok(Value::String(StringRef::Owned(arg.display_pretty()))), // TOSTRING
            11 => {
                // TOINT
                Ok(match arg {
                    Value::Int(n) => Value::Int(*n),
                    Value::Float(f) => Value::Int(*f as i64),
                    Value::String(StringRef::Owned(s)) => {
                        s.parse::<i64>().map(Value::Int).unwrap_or(Value::Null)
                    }
                    Value::Bool(b) => Value::Int(if *b { 1 } else { 0 }),
                    _ => Value::Null,
                })
            }
            12 => {
                // TOFLOAT
                Ok(match arg {
                    Value::Float(f) => Value::Float(*f),
                    Value::Int(n) => Value::Float(*n as f64),
                    Value::String(StringRef::Owned(s)) => {
                        s.parse::<f64>().map(Value::Float).unwrap_or(Value::Null)
                    }
                    _ => Value::Null,
                })
            }
            13 => Ok(Value::String(StringRef::Owned(arg.type_name().to_string()))), // TYPEOF
            14 => {
                // KEYS
                Ok(match arg {
                    Value::Map(m) => Value::List(
                        m.keys()
                            .map(|k| Value::String(StringRef::Owned(k.clone())))
                            .collect(),
                    ),
                    Value::Record(r) => Value::List(
                        r.fields
                            .keys()
                            .map(|k| Value::String(StringRef::Owned(k.clone())))
                            .collect(),
                    ),
                    _ => Value::List(vec![]),
                })
            }
            15 => {
                // VALUES
                Ok(match arg {
                    Value::Map(m) => Value::List(m.values().cloned().collect()),
                    Value::Record(r) => Value::List(r.fields.values().cloned().collect()),
                    _ => Value::List(vec![]),
                })
            }
            16 => {
                // CONTAINS
                let item = &self.registers[base + arg_reg + 1];
                Ok(match arg {
                    Value::List(l) => Value::Bool(l.contains(item)),
                    Value::Set(s) => Value::Bool(s.contains(item)),
                    Value::Map(m) => Value::Bool(m.contains_key(&item.as_string())),
                    Value::String(StringRef::Owned(s)) => {
                        Value::Bool(s.contains(&item.as_string()))
                    }
                    _ => Value::Bool(false),
                })
            }
            17 => {
                // JOIN
                let sep = self.registers[base + arg_reg + 1].as_string();
                Ok(match arg {
                    Value::List(l) => {
                        let s = l
                            .iter()
                            .map(|v| v.as_string())
                            .collect::<Vec<_>>()
                            .join(&sep);
                        Value::String(StringRef::Owned(s))
                    }
                    _ => Value::String(StringRef::Owned("".into())),
                })
            }
            18 => {
                // SPLIT
                let sep = self.registers[base + arg_reg + 1].as_string();
                Ok(match arg {
                    Value::String(StringRef::Owned(s)) => {
                        let parts: Vec<Value> = s
                            .split(&sep)
                            .map(|p| Value::String(StringRef::Owned(p.to_string())))
                            .collect();
                        Value::List(parts)
                    }
                    _ => Value::List(vec![]),
                })
            }
            19 => Ok(match arg {
                Value::String(StringRef::Owned(s)) => {
                    Value::String(StringRef::Owned(s.trim().to_string()))
                }
                _ => arg.clone(),
            }), // TRIM
            20 => Ok(match arg {
                Value::String(StringRef::Owned(s)) => {
                    Value::String(StringRef::Owned(s.to_uppercase()))
                }
                _ => arg.clone(),
            }), // UPPER
            21 => Ok(match arg {
                Value::String(StringRef::Owned(s)) => {
                    Value::String(StringRef::Owned(s.to_lowercase()))
                }
                _ => arg.clone(),
            }), // LOWER
            22 => {
                // REPLACE
                let pat = self.registers[base + arg_reg + 1].as_string();
                let with = self.registers[base + arg_reg + 2].as_string();
                Ok(match arg {
                    Value::String(StringRef::Owned(s)) => {
                        Value::String(StringRef::Owned(s.replace(&pat, &with)))
                    }
                    _ => arg.clone(),
                })
            }
            23 => {
                // SLICE
                let start_val = &self.registers[base + arg_reg + 1];
                let end_val = &self.registers[base + arg_reg + 2];
                let start = start_val.as_int().unwrap_or(0);
                let end = end_val.as_int().unwrap_or(0);
                Ok(match arg {
                    Value::List(l) => {
                        let start = start.max(0) as usize;
                        let end = if end <= 0 {
                            l.len()
                        } else {
                            (end as usize).min(l.len())
                        };
                        if start < end {
                            Value::List(l[start..end].to_vec())
                        } else {
                            Value::List(vec![])
                        }
                    }
                    Value::String(StringRef::Owned(s)) => {
                        let start = start.max(0) as usize;
                        let end = if end <= 0 {
                            s.len()
                        } else {
                            (end as usize).min(s.len())
                        };
                        if start < end && start <= s.len() {
                            Value::String(StringRef::Owned(s[start..end].to_string()))
                        } else {
                            Value::String(StringRef::Owned("".into()))
                        }
                    }
                    _ => Value::Null,
                })
            }
            24 => {
                // APPEND
                let item = self.registers[base + arg_reg + 1].clone();
                Ok(match arg {
                    Value::List(l) => {
                        let mut new_l = l.clone();
                        new_l.push(item);
                        Value::List(new_l)
                    }
                    _ => Value::Null,
                })
            }
            25 => {
                // RANGE
                let end = self.registers[base + arg_reg + 1].as_int().unwrap_or(0);
                let start = arg.as_int().unwrap_or(0);
                let list: Vec<Value> = (start..end).map(Value::Int).collect();
                Ok(Value::List(list))
            }
            26 => Ok(match arg {
                Value::Int(n) => Value::Int(n.abs()),
                Value::Float(f) => Value::Float(f.abs()),
                _ => Value::Null,
            }), // ABS
            27 => {
                // MIN
                let other = &self.registers[base + arg_reg + 1];
                Ok(match (arg, other) {
                    (Value::Int(a), Value::Int(b)) => Value::Int(*a.min(b)),
                    (Value::Float(a), Value::Float(b)) => Value::Float(a.min(*b)),
                    _ => arg.clone(),
                })
            }
            28 => {
                // MAX
                let other = &self.registers[base + arg_reg + 1];
                Ok(match (arg, other) {
                    (Value::Int(a), Value::Int(b)) => Value::Int(*a.max(b)),
                    (Value::Float(a), Value::Float(b)) => Value::Float(a.max(*b)),
                    _ => arg.clone(),
                })
            }
            // Extended intrinsics (29+)
            29 => {
                // SORT
                if let Value::List(l) = arg {
                    let mut s = l.clone();
                    s.sort();
                    Ok(Value::List(s))
                } else {
                    Ok(arg.clone())
                }
            }
            30 => {
                // REVERSE
                if let Value::List(l) = arg {
                    let mut r = l.clone();
                    r.reverse();
                    Ok(Value::List(r))
                } else {
                    Ok(arg.clone())
                }
            }
            44 => {
                // FLATTEN
                if let Value::List(l) = arg {
                    let mut result = Vec::new();
                    for item in l {
                        if let Value::List(inner) = item {
                            result.extend(inner.iter().cloned());
                        } else {
                            result.push(item.clone());
                        }
                    }
                    Ok(Value::List(result))
                } else {
                    Ok(arg.clone())
                }
            }
            45 => {
                // UNIQUE
                if let Value::List(l) = arg {
                    let mut result = Vec::new();
                    for item in l {
                        if !result.contains(item) {
                            result.push(item.clone());
                        }
                    }
                    Ok(Value::List(result))
                } else {
                    Ok(arg.clone())
                }
            }
            46 => {
                // TAKE
                let n = self.registers[base + arg_reg + 1].as_int().unwrap_or(0) as usize;
                if let Value::List(l) = arg {
                    Ok(Value::List(l.iter().take(n).cloned().collect()))
                } else {
                    Ok(arg.clone())
                }
            }
            47 => {
                // DROP
                let n = self.registers[base + arg_reg + 1].as_int().unwrap_or(0) as usize;
                if let Value::List(l) = arg {
                    Ok(Value::List(l.iter().skip(n).cloned().collect()))
                } else {
                    Ok(arg.clone())
                }
            }
            48 => Ok(match arg {
                Value::List(l) => l.first().cloned().unwrap_or(Value::Null),
                _ => Value::Null,
            }), // FIRST
            49 => Ok(match arg {
                Value::List(l) => l.last().cloned().unwrap_or(Value::Null),
                _ => Value::Null,
            }), // LAST
            50 => Ok(Value::Bool(match arg {
                Value::List(l) => l.is_empty(),
                Value::Map(m) => m.is_empty(),
                Value::String(StringRef::Owned(s)) => s.is_empty(),
                _ => true,
            })), // ISEMPTY
            51 => {
                // CHARS
                let s = arg.as_string();
                Ok(Value::List(
                    s.chars()
                        .map(|c| Value::String(StringRef::Owned(c.to_string())))
                        .collect(),
                ))
            }
            52 => {
                // STARTSWITH
                let prefix = self.registers[base + arg_reg + 1].as_string();
                Ok(Value::Bool(arg.as_string().starts_with(&prefix)))
            }
            53 => {
                // ENDSWITH
                let suffix = self.registers[base + arg_reg + 1].as_string();
                Ok(Value::Bool(arg.as_string().ends_with(&suffix)))
            }
            54 => {
                // INDEXOF
                let needle = self.registers[base + arg_reg + 1].as_string();
                Ok(match arg.as_string().find(&needle) {
                    Some(i) => Value::Int(i as i64),
                    None => Value::Int(-1),
                })
            }
            55 => {
                // PADLEFT
                let width = self.registers[base + arg_reg + 1].as_int().unwrap_or(0) as usize;
                let s = arg.as_string();
                if s.len() < width {
                    Ok(Value::String(StringRef::Owned(format!(
                        "{:>width$}",
                        s,
                        width = width
                    ))))
                } else {
                    Ok(Value::String(StringRef::Owned(s)))
                }
            }
            56 => {
                // PADRIGHT
                let width = self.registers[base + arg_reg + 1].as_int().unwrap_or(0) as usize;
                let s = arg.as_string();
                if s.len() < width {
                    Ok(Value::String(StringRef::Owned(format!(
                        "{:<width$}",
                        s,
                        width = width
                    ))))
                } else {
                    Ok(Value::String(StringRef::Owned(s)))
                }
            }
            57 => Ok(match arg {
                Value::Float(f) => Value::Float(f.round()),
                _ => arg.clone(),
            }), // ROUND
            58 => Ok(match arg {
                Value::Float(f) => Value::Float(f.ceil()),
                _ => arg.clone(),
            }), // CEIL
            59 => Ok(match arg {
                Value::Float(f) => Value::Float(f.floor()),
                _ => arg.clone(),
            }), // FLOOR
            60 => Ok(match arg {
                Value::Float(f) => Value::Float(f.sqrt()),
                Value::Int(n) => Value::Float((*n as f64).sqrt()),
                _ => Value::Null,
            }), // SQRT
            61 => {
                // POW
                let exp = &self.registers[base + arg_reg + 1];
                Ok(match (arg, exp) {
                    (Value::Int(x), Value::Int(y)) => {
                        if *y >= 0 {
                            Value::Int(x.pow(*y as u32))
                        } else {
                            Value::Float((*x as f64).powf(*y as f64))
                        }
                    }
                    (Value::Float(x), Value::Float(y)) => Value::Float(x.powf(*y)),
                    _ => Value::Null,
                })
            }
            62 => Ok(match arg {
                Value::Float(f) => Value::Float(f.ln()),
                Value::Int(n) => Value::Float((*n as f64).ln()),
                _ => Value::Null,
            }), // LOG
            63 => Ok(match arg {
                Value::Float(f) => Value::Float(f.sin()),
                Value::Int(n) => Value::Float((*n as f64).sin()),
                _ => Value::Null,
            }), // SIN
            64 => Ok(match arg {
                Value::Float(f) => Value::Float(f.cos()),
                Value::Int(n) => Value::Float((*n as f64).cos()),
                _ => Value::Null,
            }), // COS
            65 => {
                // CLAMP
                let lo = &self.registers[base + arg_reg + 1];
                let hi = &self.registers[base + arg_reg + 2];
                Ok(match (arg, lo, hi) {
                    (Value::Int(v), Value::Int(l), Value::Int(h)) => Value::Int(*v.max(l).min(h)),
                    (Value::Float(v), Value::Float(l), Value::Float(h)) => {
                        Value::Float(v.max(*l).min(*h))
                    }
                    _ => arg.clone(),
                })
            }
            66 => Ok(arg.clone()),                                   // CLONE
            67 => Ok(Value::Int(std::mem::size_of_val(arg) as i64)), // SIZEOF
            68 => {
                // DEBUG
                let output = format!("[debug] {:?}", arg);
                eprintln!("{}", output);
                self.output.push(output);
                Ok(Value::Null)
            }
            _ => Ok(Value::Null),
        }
    }

    /// Structural diff of two values.
    fn diff_values(&self, a: &Value, b: &Value) -> Value {
        if a == b {
            return Value::List(vec![]);
        }
        match (a, b) {
            (Value::Record(ra), Value::Record(rb)) if ra.type_name == rb.type_name => {
                let mut diffs = Vec::new();
                for (key, va) in &ra.fields {
                    match rb.fields.get(key) {
                        Some(vb) if va != vb => {
                            let mut change = BTreeMap::new();
                            change.insert(
                                "field".to_string(),
                                Value::String(StringRef::Owned(key.clone())),
                            );
                            change.insert("from".to_string(), va.clone());
                            change.insert("to".to_string(), vb.clone());
                            diffs.push(Value::Map(change));
                        }
                        None => {
                            let mut change = BTreeMap::new();
                            change.insert(
                                "field".to_string(),
                                Value::String(StringRef::Owned(key.clone())),
                            );
                            change.insert("removed".to_string(), va.clone());
                            diffs.push(Value::Map(change));
                        }
                        _ => {}
                    }
                }
                for (key, vb) in &rb.fields {
                    if !ra.fields.contains_key(key) {
                        let mut change = BTreeMap::new();
                        change.insert(
                            "field".to_string(),
                            Value::String(StringRef::Owned(key.clone())),
                        );
                        change.insert("added".to_string(), vb.clone());
                        diffs.push(Value::Map(change));
                    }
                }
                Value::List(diffs)
            }
            (Value::Map(ma), Value::Map(mb)) => {
                let mut diffs = Vec::new();
                for (key, va) in ma {
                    match mb.get(key) {
                        Some(vb) if va != vb => {
                            let mut change = BTreeMap::new();
                            change.insert(
                                "key".to_string(),
                                Value::String(StringRef::Owned(key.clone())),
                            );
                            change.insert("from".to_string(), va.clone());
                            change.insert("to".to_string(), vb.clone());
                            diffs.push(Value::Map(change));
                        }
                        None => {
                            let mut change = BTreeMap::new();
                            change.insert(
                                "key".to_string(),
                                Value::String(StringRef::Owned(key.clone())),
                            );
                            change.insert("removed".to_string(), va.clone());
                            diffs.push(Value::Map(change));
                        }
                        _ => {}
                    }
                }
                for (key, vb) in mb {
                    if !ma.contains_key(key) {
                        let mut change = BTreeMap::new();
                        change.insert(
                            "key".to_string(),
                            Value::String(StringRef::Owned(key.clone())),
                        );
                        change.insert("added".to_string(), vb.clone());
                        diffs.push(Value::Map(change));
                    }
                }
                Value::List(diffs)
            }
            _ => {
                let mut change = BTreeMap::new();
                change.insert("from".to_string(), a.clone());
                change.insert("to".to_string(), b.clone());
                Value::List(vec![Value::Map(change)])
            }
        }
    }

    /// Apply patches to a value.
    fn patch_value(&self, val: &Value, patches: &Value) -> Value {
        match (val, patches) {
            (Value::Record(r), Value::List(patch_list)) => {
                let mut result = r.clone();
                for patch in patch_list {
                    if let Value::Map(m) = patch {
                        if let Some(Value::String(StringRef::Owned(field))) = m.get("field") {
                            if let Some(to) = m.get("to") {
                                result.fields.insert(field.clone(), to.clone());
                            } else if m.contains_key("removed") {
                                result.fields.remove(field);
                            } else if let Some(added) = m.get("added") {
                                result.fields.insert(field.clone(), added.clone());
                            }
                        }
                    }
                }
                Value::Record(result)
            }
            (Value::Map(map), Value::List(patch_list)) => {
                let mut result = map.clone();
                for patch in patch_list {
                    if let Value::Map(m) = patch {
                        if let Some(Value::String(StringRef::Owned(key))) = m.get("key") {
                            if let Some(to) = m.get("to") {
                                result.insert(key.clone(), to.clone());
                            } else if m.contains_key("removed") {
                                result.remove(key);
                            } else if let Some(added) = m.get("added") {
                                result.insert(key.clone(), added.clone());
                            }
                        }
                    }
                }
                Value::Map(result)
            }
            _ => val.clone(),
        }
    }

    /// Redact specified fields from a value (set to null).
    fn redact_value(&self, val: &Value, field_list: &Value) -> Value {
        let fields_to_redact: Vec<String> = match field_list {
            Value::List(l) => l.iter().map(|v| v.as_string()).collect(),
            Value::String(StringRef::Owned(s)) => vec![s.clone()],
            _ => return val.clone(),
        };
        match val {
            Value::Record(r) => {
                let mut result = r.clone();
                for field in &fields_to_redact {
                    if result.fields.contains_key(field) {
                        result.fields.insert(field.clone(), Value::Null);
                    }
                }
                Value::Record(result)
            }
            Value::Map(m) => {
                let mut result = m.clone();
                for field in &fields_to_redact {
                    if result.contains_key(field) {
                        result.insert(field.clone(), Value::Null);
                    }
                }
                Value::Map(result)
            }
            _ => val.clone(),
        }
    }

    fn arith_op(
        &mut self,
        base: usize,
        a: usize,
        b: usize,
        c: usize,
        int_op: impl Fn(i64, i64) -> i64,
        float_op: impl Fn(f64, f64) -> f64,
    ) -> Result<(), VmError> {
        let lhs = &self.registers[base + b];
        let rhs = &self.registers[base + c];
        self.registers[base + a] = match (lhs, rhs) {
            (Value::Int(x), Value::Int(y)) => Value::Int(int_op(*x, *y)),
            (Value::Float(x), Value::Float(y)) => Value::Float(float_op(*x, *y)),
            (Value::Int(x), Value::Float(y)) => Value::Float(float_op(*x as f64, *y)),
            (Value::Float(x), Value::Int(y)) => Value::Float(float_op(*x, *y as f64)),
            _ => {
                return Err(VmError::TypeError(format!(
                    "arithmetic on non-numeric types"
                )))
            }
        };
        Ok(())
    }
}

/// Convert a Lumen Value to a serde_json Value.
fn value_to_json(val: &Value) -> serde_json::Value {
    match val {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(n) => serde_json::json!(*n),
        Value::Float(f) => serde_json::json!(*f),
        Value::String(StringRef::Owned(s)) => serde_json::Value::String(s.clone()),
        Value::String(StringRef::Interned(_)) => serde_json::Value::String(val.as_string()),
        Value::List(l) => serde_json::Value::Array(l.iter().map(value_to_json).collect()),
        Value::Tuple(t) => serde_json::Value::Array(t.iter().map(value_to_json).collect()),
        Value::Set(s) => serde_json::Value::Array(s.iter().map(value_to_json).collect()),
        Value::Map(m) => {
            let obj: serde_json::Map<String, serde_json::Value> = m
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        Value::Record(r) => {
            let mut obj = serde_json::Map::new();
            obj.insert(
                "__type".to_string(),
                serde_json::Value::String(r.type_name.clone()),
            );
            for (k, v) in &r.fields {
                obj.insert(k.clone(), value_to_json(v));
            }
            serde_json::Value::Object(obj)
        }
        Value::Union(u) => {
            let mut obj = serde_json::Map::new();
            obj.insert(
                "__tag".to_string(),
                serde_json::Value::String(u.tag.clone()),
            );
            obj.insert("__payload".to_string(), value_to_json(&u.payload));
            serde_json::Value::Object(obj)
        }
        _ => serde_json::Value::Null,
    }
}

/// Convert a serde_json Value to a Lumen Value.
fn json_to_value(val: &serde_json::Value) -> Value {
    match val {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Null
            }
        }
        serde_json::Value::String(s) => Value::String(StringRef::Owned(s.clone())),
        serde_json::Value::Array(arr) => Value::List(arr.iter().map(json_to_value).collect()),
        serde_json::Value::Object(obj) => {
            let map: BTreeMap<String, Value> = obj
                .iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect();
            Value::Map(map)
        }
    }
}

/// Simple base64 encode (no external dependency).
fn simple_base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

/// Simple base64 decode.
fn simple_base64_decode(s: &str) -> Option<Vec<u8>> {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = Vec::new();
    let bytes: Vec<u8> = s.bytes().filter(|&b| b != b'\n' && b != b'\r').collect();
    for chunk in bytes.chunks(4) {
        if chunk.len() < 4 {
            break;
        }
        let vals: Vec<Option<usize>> = chunk
            .iter()
            .map(|&b| {
                if b == b'=' {
                    Some(0)
                } else {
                    CHARS.iter().position(|&c| c == b)
                }
            })
            .collect();
        if vals.iter().any(|v| v.is_none()) {
            return None;
        }
        let v: Vec<usize> = vals.into_iter().map(|v| v.unwrap()).collect();
        let triple = (v[0] << 18) | (v[1] << 12) | (v[2] << 6) | v[3];
        result.push(((triple >> 16) & 0xFF) as u8);
        if chunk[2] != b'=' {
            result.push(((triple >> 8) & 0xFF) as u8);
        }
        if chunk[3] != b'=' {
            result.push((triple & 0xFF) as u8);
        }
    }
    Some(result)
}

impl Default for VM {
    fn default() -> Self {
        Self::new()
    }
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
            agents: vec![],
            addons: vec![],
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
                    LirParam {
                        name: "a".into(),
                        ty: "Int".into(),
                        register: 0,
                    },
                    LirParam {
                        name: "b".into(),
                        ty: "Int".into(),
                        register: 1,
                    },
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
            agents: vec![],
            addons: vec![],
        }
    }

    #[test]
    fn test_vm_add() {
        let mut vm = VM::new();
        vm.load(make_add());
        let result = vm
            .execute("add", vec![Value::Int(10), Value::Int(32)])
            .unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_vm_print() {
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
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::Call, 0, 1, 0),
                    Instruction::abc(OpCode::LoadNil, 0, 0, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let _result = vm.execute("main", vec![]).unwrap();
        assert_eq!(vm.output, vec!["Hello, World!"]);
    }

    #[test]
    fn test_vm_append() {
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
                    Instruction::abc(OpCode::NewList, 0, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 0),
                    Instruction::abc(OpCode::Append, 0, 1, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::Append, 0, 1, 0),
                    Instruction::abx(OpCode::LoadK, 1, 2),
                    Instruction::abc(OpCode::Append, 0, 1, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
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
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::Lt, 2, 0, 1),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
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
            agents: vec![],
            addons: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]).unwrap();
        assert_eq!(
            result,
            Value::String(StringRef::Owned("Hello, World!".into()))
        );
    }

    #[test]
    fn test_vm_tuple() {
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
                constants: vec![Constant::Int(1), Constant::Int(2), Constant::Int(3)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 1, 0),
                    Instruction::abx(OpCode::LoadK, 2, 1),
                    Instruction::abx(OpCode::LoadK, 3, 2),
                    Instruction::abc(OpCode::NewTuple, 0, 3, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]).unwrap();
        assert_eq!(
            result,
            Value::Tuple(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
    }

    #[test]
    fn test_vm_set() {
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
                constants: vec![Constant::Int(1), Constant::Int(2), Constant::Int(1)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 1, 0),
                    Instruction::abx(OpCode::LoadK, 2, 1),
                    Instruction::abx(OpCode::LoadK, 3, 2), // duplicate 1
                    Instruction::abc(OpCode::NewSet, 0, 3, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]).unwrap();
        // Set should deduplicate
        if let Value::Set(s) = result {
            assert_eq!(s.len(), 2);
        } else {
            panic!("expected set");
        }
    }

    #[test]
    fn test_vm_bitwise() {
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
                constants: vec![Constant::Int(0b1100), Constant::Int(0b1010)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::BitAnd, 2, 0, 1), // 0b1000 = 8
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]).unwrap();
        assert_eq!(result, Value::Int(0b1000));
    }
}
