//! Conservative eligibility analysis for the strict JIT tier.
//!
//! The strict tier only compiles cells that it can *prove* behave exactly like
//! the interpreter:
//!
//! * every parameter and the return value are declared `Int` or `Bool`;
//! * every instruction is one of a small set of scalar opcodes, and a forward
//!   dataflow pass over the bytecode proves each operand has the type the
//!   opcode needs (so `1 + true`, `null`, floats, strings, reads of
//!   uninitialised or moved-from registers, and so on are all rejected);
//! * every `Call`/`TailCall` resolves, through a constant callee name, to
//!   another cell that is itself eligible with matching argument and return
//!   types (computed as a greatest fixed point over the module);
//! * control flow stays inside the cell and never falls off the end.
//!
//! Anything else is left to the interpreter. Cells accepted here are pure
//! (no side effects), which lets the runtime re-run a call in the interpreter
//! if the native code traps (overflow, division by zero, stack exhaustion).

use std::collections::HashMap;

use crate::compiler::lir::{Constant, LirCell, OpCode};

/// Scalar types the strict tier understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarTy {
    Int,
    Bool,
}

impl ScalarTy {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "Int" => Some(ScalarTy::Int),
            "Bool" => Some(ScalarTy::Bool),
            _ => None,
        }
    }
}

/// Maximum arity of a cell callable from the VM (see `JitEngine::execute_jit`).
pub const MAX_JIT_ARITY: usize = 6;

/// Declared signature of a candidate cell.
#[derive(Debug, Clone)]
pub struct Sig {
    pub params: Vec<ScalarTy>,
    pub ret: ScalarTy,
}

/// Result of analysing one cell.
#[derive(Debug, Clone)]
pub struct CellPlan {
    /// pc of every reachable instruction.
    pub reachable: Vec<bool>,
    /// Resolved callee name for each reachable `Call`/`TailCall` pc.
    pub callees: HashMap<usize, String>,
}

/// Returns the signature of `cell` if it is made only of strict-tier types.
pub fn cell_signature(cell: &LirCell) -> Option<Sig> {
    if cell.params.len() > MAX_JIT_ARITY || cell.registers > 256 {
        return None;
    }
    let ret = ScalarTy::parse(cell.returns.as_deref()?)?;
    let mut params = Vec::with_capacity(cell.params.len());
    for (i, p) in cell.params.iter().enumerate() {
        if p.variadic || p.register as usize != i {
            return None;
        }
        params.push(ScalarTy::parse(&p.ty)?);
    }
    if (cell.registers as usize) < params.len() {
        return None;
    }
    Some(Sig { params, ret })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Abs {
    /// Never written (the interpreter would see `Null` or a stale value).
    Undef,
    Int,
    Bool,
    /// A string constant (index into `cell.constants`) used as a callee name.
    Name(usize),
    /// Unknown / conflicting.
    Bad,
}

fn from_scalar(t: ScalarTy) -> Abs {
    match t {
        ScalarTy::Int => Abs::Int,
        ScalarTy::Bool => Abs::Bool,
    }
}

fn join(cell: &LirCell, a: Abs, b: Abs) -> Abs {
    if a == b {
        return a;
    }
    if let (Abs::Name(x), Abs::Name(y)) = (a, b) {
        if let (Some(Constant::String(s)), Some(Constant::String(t))) =
            (cell.constants.get(x), cell.constants.get(y))
        {
            if s == t {
                return a;
            }
        }
    }
    Abs::Bad
}

/// Analyse `cell` against the signatures of the currently-eligible cells.
/// Returns `None` if the cell cannot be compiled by the strict tier.
pub fn analyze_cell(cell: &LirCell, sigs: &HashMap<String, Sig>) -> Option<CellPlan> {
    let own = cell_signature(cell)?;
    let n = cell.instructions.len();
    if n == 0 {
        return None;
    }
    let nregs = cell.registers as usize;

    let mut entry = vec![Abs::Undef; nregs];
    for (i, t) in own.params.iter().enumerate() {
        entry[i] = from_scalar(*t);
    }

    let mut states: Vec<Option<Vec<Abs>>> = vec![None; n];
    states[0] = Some(entry);
    let mut work: Vec<usize> = vec![0];
    let mut callees: HashMap<usize, String> = HashMap::new();

    while let Some(pc) = work.pop() {
        let mut st = states[pc].clone()?;
        let inst = cell.instructions[pc];
        let a = inst.a as usize;
        let b = inst.b as usize;
        let c = inst.c as usize;

        let get = |st: &Vec<Abs>, r: usize| -> Abs { st.get(r).copied().unwrap_or(Abs::Bad) };
        let is_scalar = |t: Abs| matches!(t, Abs::Int | Abs::Bool);
        let mut succs: Vec<usize> = Vec::with_capacity(2);

        macro_rules! set {
            ($r:expr, $v:expr) => {{
                if $r >= nregs {
                    return None;
                }
                st[$r] = $v;
            }};
        }

        match inst.op {
            OpCode::Nop => succs.push(pc + 1),
            OpCode::LoadK => {
                let v = match cell.constants.get(inst.bx() as usize)? {
                    Constant::Int(_) => Abs::Int,
                    Constant::Bool(_) => Abs::Bool,
                    Constant::String(_) => Abs::Name(inst.bx() as usize),
                    _ => return None,
                };
                set!(a, v);
                succs.push(pc + 1);
            }
            OpCode::LoadBool => {
                set!(a, Abs::Bool);
                succs.push(if c != 0 { pc + 2 } else { pc + 1 });
            }
            OpCode::LoadInt => {
                set!(a, Abs::Int);
                succs.push(pc + 1);
            }
            OpCode::Move | OpCode::MoveOwn => {
                let v = get(&st, b);
                if matches!(v, Abs::Undef | Abs::Bad) {
                    return None;
                }
                set!(a, v);
                if inst.op == OpCode::MoveOwn && a != b {
                    // The interpreter nulls the source register.
                    set!(b, Abs::Undef);
                }
                succs.push(pc + 1);
            }
            OpCode::Add
            | OpCode::Sub
            | OpCode::Mul
            | OpCode::Div
            | OpCode::Mod
            | OpCode::FloorDiv
            | OpCode::Pow
            | OpCode::BitOr
            | OpCode::BitAnd
            | OpCode::BitXor
            | OpCode::Shl
            | OpCode::Shr => {
                if get(&st, b) != Abs::Int || get(&st, c) != Abs::Int {
                    return None;
                }
                set!(a, Abs::Int);
                succs.push(pc + 1);
            }
            OpCode::Neg | OpCode::BitNot => {
                if get(&st, b) != Abs::Int {
                    return None;
                }
                set!(a, Abs::Int);
                succs.push(pc + 1);
            }
            OpCode::Eq => {
                let (x, y) = (get(&st, b), get(&st, c));
                if !(is_scalar(x) && x == y) {
                    return None;
                }
                set!(a, Abs::Bool);
                succs.push(pc + 1);
            }
            OpCode::Lt | OpCode::Le => {
                if get(&st, b) != Abs::Int || get(&st, c) != Abs::Int {
                    return None;
                }
                set!(a, Abs::Bool);
                succs.push(pc + 1);
            }
            OpCode::Not => {
                if !is_scalar(get(&st, b)) {
                    return None;
                }
                set!(a, Abs::Bool);
                succs.push(pc + 1);
            }
            OpCode::And | OpCode::Or => {
                if !is_scalar(get(&st, b)) || !is_scalar(get(&st, c)) {
                    return None;
                }
                set!(a, Abs::Bool);
                succs.push(pc + 1);
            }
            OpCode::Test => {
                if !is_scalar(get(&st, a)) {
                    return None;
                }
                succs.push(pc + 1);
                succs.push(pc + 2);
            }
            OpCode::Jmp | OpCode::Break | OpCode::Continue => {
                let target = pc as i64 + 1 + inst.sax_val() as i64;
                if target < 0 {
                    return None;
                }
                succs.push(target as usize);
            }
            OpCode::Return => {
                if get(&st, a) != from_scalar(own.ret) {
                    return None;
                }
            }
            OpCode::Call | OpCode::TailCall => {
                let name = match get(&st, a) {
                    Abs::Name(k) => match cell.constants.get(k) {
                        Some(Constant::String(s)) => s.clone(),
                        _ => return None,
                    },
                    _ => return None,
                };
                let sig = sigs.get(&name)?;
                if sig.params.len() != b {
                    return None;
                }
                for (i, t) in sig.params.iter().enumerate() {
                    if get(&st, a + 1 + i) != from_scalar(*t) {
                        return None;
                    }
                }
                callees.insert(pc, name);
                if inst.op == OpCode::TailCall {
                    if sig.ret != own.ret {
                        return None;
                    }
                } else {
                    // The interpreter moves the argument registers out.
                    for i in 0..b {
                        set!(a + 1 + i, Abs::Undef);
                    }
                    set!(a, from_scalar(sig.ret));
                    succs.push(pc + 1);
                }
            }
            _ => return None,
        }

        for s in succs {
            if s >= n {
                // Falls off the end of the cell, or jumps outside it.
                return None;
            }
            match &mut states[s] {
                None => {
                    states[s] = Some(st.clone());
                    work.push(s);
                }
                Some(old) => {
                    let mut changed = false;
                    for r in 0..nregs {
                        let j = if old[r] == Abs::Undef && st[r] == Abs::Undef {
                            Abs::Undef
                        } else {
                            join(cell, old[r], st[r])
                        };
                        if j != old[r] {
                            old[r] = j;
                            changed = true;
                        }
                    }
                    if changed {
                        work.push(s);
                    }
                }
            }
        }
    }

    Some(CellPlan {
        reachable: states.iter().map(|s| s.is_some()).collect(),
        callees,
    })
}

/// Greatest fixed point: start with every cell that has a strict signature
/// and repeatedly drop cells whose analysis fails against the remaining set.
pub fn eligible_cells(cells: &[LirCell]) -> HashMap<String, (Sig, CellPlan)> {
    let mut sigs: HashMap<String, Sig> = HashMap::new();
    let mut dup: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for c in cells {
        if sigs.contains_key(&c.name) {
            dup.insert(&c.name);
        }
        if let Some(s) = cell_signature(c) {
            sigs.insert(c.name.clone(), s);
        }
    }
    // Ambiguous names are never eligible (the interpreter picks the first one).
    for d in &dup {
        sigs.remove(*d);
    }
    loop {
        let mut plans: HashMap<String, (Sig, CellPlan)> = HashMap::new();
        let mut dropped = false;
        for c in cells {
            let Some(sig) = sigs.get(&c.name) else {
                continue;
            };
            match analyze_cell(c, &sigs) {
                Some(plan) => {
                    plans.insert(c.name.clone(), (sig.clone(), plan));
                }
                None => {
                    dropped = true;
                    sigs.remove(&c.name);
                    break;
                }
            }
        }
        if !dropped {
            return plans;
        }
    }
}
