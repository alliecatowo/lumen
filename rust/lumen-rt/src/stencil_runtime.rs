//! Runtime helpers for stencil (Tier 1) execution.

use std::collections::BTreeMap;

use lumen_core::lir::Instruction;
use lumen_core::nb_value::NbValue;
use lumen_core::values::{RecordValue, Value};
use lumen_core::vm_context::VmContext;

use crate::vm::VM;

fn vm_from_ctx(ctx: *mut VmContext) -> &'static mut VM {
    debug_assert!(!ctx.is_null(), "stencil runtime: null VmContext");
    unsafe {
        let ptr = (*ctx).stack_pool as *mut VM;
        debug_assert!(!ptr.is_null(), "stencil runtime: null VM pointer");
        &mut *ptr
    }
}

fn decode_nb_value(_vm: &mut VM, raw: u64) -> Value {
    let nb = NbValue(raw);
    nb.to_legacy()
}

#[no_mangle]
pub unsafe extern "C" fn lm_rt_call(ctx: *mut VmContext, instr_word: u64) {
    let vm = vm_from_ctx(ctx);
    let instr: Instruction = unsafe { std::mem::transmute(instr_word) };
    let _ = vm.dispatch_call_from_stencil(instr.a as usize, instr.b as usize);
}

#[no_mangle]
pub unsafe extern "C" fn lm_rt_tailcall(ctx: *mut VmContext, instr_word: u64) {
    let vm = vm_from_ctx(ctx);
    let instr: Instruction = unsafe { std::mem::transmute(instr_word) };
    let _ = vm.dispatch_tailcall_from_stencil(instr.a as usize, instr.b as usize);
}

#[no_mangle]
pub unsafe extern "C" fn lm_rt_intrinsic(ctx: *mut VmContext, instr_word: u64) {
    let vm = vm_from_ctx(ctx);
    let instr: Instruction = unsafe { std::mem::transmute(instr_word) };
    let _ = vm.exec_intrinsic_from_stencil(instr.a as usize, instr.b as usize, instr.c as usize);
}

#[no_mangle]
pub unsafe extern "C" fn lm_rt_return(ctx: *mut VmContext, reg_idx: u32) {
    let vm = vm_from_ctx(ctx);
    vm.return_from_stencil(reg_idx as usize);
}

#[no_mangle]
pub unsafe extern "C" fn lm_rt_halt(ctx: *mut VmContext, reg_idx: u32) {
    let vm = vm_from_ctx(ctx);
    let err = vm.reg(vm.current_base() + reg_idx as usize);
    vm.halt_from_stencil(err);
}

#[no_mangle]
pub unsafe extern "C" fn lm_rt_stencil_runtime(ctx: *mut VmContext, instr_word: u64) {
    let vm = vm_from_ctx(ctx);
    let instr: Instruction = unsafe { std::mem::transmute(instr_word) };
    let base = vm.current_base();
    let a = instr.a as usize;
    let b = instr.b as usize;
    let c = instr.c as usize;

    match instr.op {
        lumen_core::lir::OpCode::NewList | lumen_core::lir::OpCode::NewListStack => {
            let mut list = Vec::with_capacity(b);
            for i in 1..=b {
                list.push(vm.reg(base + a + i));
            }
            vm.set_reg(base + a, Value::new_list(list));
        }
        lumen_core::lir::OpCode::NewMap => {
            let mut map = BTreeMap::new();
            for i in 0..b {
                let k = vm.reg(base + a + 1 + i * 2).as_string_resolved(&vm.strings);
                let v = vm.reg(base + a + 2 + i * 2);
                map.insert(k, v);
            }
            vm.set_reg(base + a, Value::new_map(map));
        }
        lumen_core::lir::OpCode::NewRecord => {
            let module = vm.module().expect("stencil runtime: no module");
            let type_name = if (instr.bx() as usize) < module.strings.len() {
                module.strings[instr.bx() as usize].clone()
            } else {
                "Unknown".to_string()
            };
            let fields = BTreeMap::new();
            vm.set_reg(
                base + a,
                Value::new_record(RecordValue { type_name, fields }),
            );
        }
        lumen_core::lir::OpCode::NewTuple | lumen_core::lir::OpCode::NewTupleStack => {
            let mut elems = Vec::with_capacity(b);
            for i in 1..=b {
                elems.push(vm.reg(base + a + i));
            }
            vm.set_reg(base + a, Value::new_tuple(elems));
        }
        lumen_core::lir::OpCode::NewSet => {
            let mut elems = Vec::with_capacity(b);
            for i in 1..=b {
                let v = vm.reg(base + a + i);
                if !elems.contains(&v) {
                    elems.push(v);
                }
            }
            vm.set_reg(base + a, Value::new_set_from_vec(elems));
        }
        lumen_core::lir::OpCode::GetField => {
            let module = vm.module().expect("stencil runtime: no module");
            let obj = vm.reg(base + b);
            let field_name = if c < module.strings.len() {
                &module.strings[c]
            } else {
                ""
            };
            let val = match &obj {
                Value::Record(r) => r.fields.get(field_name).cloned().unwrap_or(Value::Null),
                Value::Map(m) => m.get(field_name).cloned().unwrap_or(Value::Null),
                _ => Value::Null,
            };
            vm.set_reg(base + a, val);
        }
        lumen_core::lir::OpCode::SetField => {
            let module = vm.module().expect("stencil runtime: no module");
            let val = vm.reg(base + c);
            let field_name = if b < module.strings.len() {
                module.strings[b].clone()
            } else {
                String::new()
            };
            let mut target = vm.reg_take(base + a);
            if let Value::Record(ref mut r) = target {
                std::sync::Arc::make_mut(r).fields.insert(field_name, val);
            }
            vm.set_reg(base + a, target);
        }
        lumen_core::lir::OpCode::GetIndex => {
            let obj = vm.reg(base + b);
            let idx = vm.reg(base + c);
            let val = match (&obj, &idx) {
                (Value::List(l), Value::Int(i)) => {
                    let ii = *i;
                    let len = l.len() as i64;
                    let effective = if ii < 0 { ii + len } else { ii };
                    if effective < 0 || effective >= len {
                        return;
                    }
                    l[effective as usize].clone()
                }
                (Value::Tuple(t), Value::Int(i)) => {
                    let ii = *i;
                    let len = t.len() as i64;
                    let effective = if ii < 0 { ii + len } else { ii };
                    if effective < 0 || effective >= len {
                        return;
                    }
                    t[effective as usize].clone()
                }
                (Value::Map(m), _) => m
                    .get(&idx.as_string_resolved(&vm.strings))
                    .cloned()
                    .unwrap_or(Value::Null),
                (Value::Record(r), _) => r
                    .fields
                    .get(&idx.as_string_resolved(&vm.strings))
                    .cloned()
                    .unwrap_or(Value::Null),
                (Value::Set(s), Value::Int(i)) => {
                    let ii = *i;
                    let len = s.len() as i64;
                    let effective = if ii < 0 { ii + len } else { ii };
                    if effective < 0 || effective >= len {
                        return;
                    }
                    s.iter()
                        .nth(effective as usize)
                        .cloned()
                        .unwrap_or(Value::Null)
                }
                _ => Value::Null,
            };
            vm.set_reg(base + a, val);
        }
        lumen_core::lir::OpCode::SetIndex => {
            let val = vm.reg(base + c);
            let key = vm.reg(base + b);
            let mut target = vm.reg_take(base + a);
            match &mut target {
                Value::List(l) => {
                    if let Some(i) = key.as_int() {
                        let len = l.len() as i64;
                        let effective = if i < 0 { i + len } else { i };
                        if effective < 0 || effective >= len {
                            return;
                        }
                        std::sync::Arc::make_mut(l)[effective as usize] = val;
                    }
                }
                Value::Tuple(t) => {
                    if let Some(i) = key.as_int() {
                        let len = t.len() as i64;
                        let effective = if i < 0 { i + len } else { i };
                        if effective < 0 || effective >= len {
                            return;
                        }
                        std::sync::Arc::make_mut(t)[effective as usize] = val;
                    }
                }
                Value::Map(m) => {
                    let k = key.as_string_resolved(&vm.strings);
                    std::sync::Arc::make_mut(m).insert(k, val);
                }
                Value::Record(r) => {
                    let k = key.as_string_resolved(&vm.strings);
                    std::sync::Arc::make_mut(r).fields.insert(k, val);
                }
                _ => {}
            }
            vm.set_reg(base + a, target);
        }
        _ => {}
    }
}
