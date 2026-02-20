//! Runtime helpers for stencil (Tier 1) execution.

use std::collections::BTreeMap;
use std::sync::Arc;

use lumen_core::lir::Instruction;
use lumen_core::nb_value::NbValue;
use lumen_core::values::{ClosureValue, RecordValue, StringRef, UnionValue, Value};
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
    // Use stencil_base (set by StencilTier::execute) rather than current_base()
    // (which is the interpreter's caller frame, not the stencil frame).
    let base = vm.stencil_base;
    let err = vm.reg(base + reg_idx as usize);
    vm.halt_from_stencil(err);
}

#[no_mangle]
pub unsafe extern "C" fn lm_rt_stencil_runtime(ctx: *mut VmContext, instr_word: u64) {
    let vm = vm_from_ctx(ctx);
    let instr: Instruction = unsafe { std::mem::transmute(instr_word) };
    // Use stencil_base (set before call_stitched) not current_base() (caller frame).
    let base = vm.stencil_base;
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
                        Arc::make_mut(l)[effective as usize] = val;
                    }
                }
                Value::Tuple(t) => {
                    if let Some(i) = key.as_int() {
                        let len = t.len() as i64;
                        let effective = if i < 0 { i + len } else { i };
                        if effective < 0 || effective >= len {
                            return;
                        }
                        Arc::make_mut(t)[effective as usize] = val;
                    }
                }
                Value::Map(m) => {
                    let k = key.as_string_resolved(&vm.strings);
                    Arc::make_mut(m).insert(k, val);
                }
                Value::Record(r) => {
                    let k = key.as_string_resolved(&vm.strings);
                    Arc::make_mut(r).fields.insert(k, val);
                }
                _ => {}
            }
            vm.set_reg(base + a, target);
        }

        // Tuple element access by constant index.
        lumen_core::lir::OpCode::GetTuple => {
            let obj = vm.reg(base + b);
            let val = match &obj {
                Value::Tuple(t) => t.get(c).cloned().unwrap_or(Value::Null),
                Value::List(l) => l.get(c).cloned().unwrap_or(Value::Null),
                _ => Value::Null,
            };
            vm.set_reg(base + a, val);
        }

        // Union construction.
        lumen_core::lir::OpCode::NewUnion => {
            let tag_val = vm.reg(base + b);
            let tag_str = tag_val.as_string_resolved(&vm.strings);
            let tag = vm.strings.intern(&tag_str);
            let payload = Arc::new(vm.reg(base + c));
            vm.set_reg(base + a, Value::Union(UnionValue { tag, payload }));
        }

        // Type variant check (skip next if matched).
        lumen_core::lir::OpCode::IsVariant => {
            // IsVariant is a conditional skip — in the stencil tier this is a
            // no-op for control flow (IP manipulation not available here).
            // The stencil exists for library completeness; the interpreter handles
            // the skip semantics when it re-dispatches.
            let tag_idx = instr.bx() as usize;
            // Clone the tag string to avoid borrow conflict between module and vm.strings.
            let tag_str = {
                let module = vm.module().expect("stencil runtime: no module");
                if tag_idx < module.strings.len() {
                    Some(module.strings[tag_idx].clone())
                } else {
                    None
                }
            };
            let _tag_id = tag_str.map(|s| vm.strings.intern(&s)).unwrap_or(u32::MAX);
            // Control-flow side-effect (skip) not implemented in stencil tier.
        }

        // Union payload extraction.
        lumen_core::lir::OpCode::Unbox => {
            let val = vm.reg(base + b);
            let result = if let Value::Union(u) = &val {
                (*u.payload).clone()
            } else {
                Value::Null
            };
            vm.set_reg(base + a, result);
        }

        // List append.
        lumen_core::lir::OpCode::Append => {
            let val = vm.reg(base + b);
            let mut target = vm.reg_take(base + a);
            if let Value::List(ref mut l) = target {
                Arc::make_mut(l).push(val);
            }
            vm.set_reg(base + a, target);
        }

        // String / list concatenation.
        lumen_core::lir::OpCode::Concat => {
            let lhs = vm.reg(base + b);
            let rhs = vm.reg(base + c);
            let result = match (&lhs, &rhs) {
                (Value::List(la), Value::List(lb)) => {
                    let mut combined = Vec::with_capacity(la.len() + lb.len());
                    combined.extend(la.iter().cloned());
                    combined.extend(lb.iter().cloned());
                    Value::new_list(combined)
                }
                _ => {
                    let lhs_str = lhs.as_string_resolved(&vm.strings);
                    let rhs_str = rhs.as_string_resolved(&vm.strings);
                    let mut s = String::with_capacity(lhs_str.len() + rhs_str.len());
                    s.push_str(&lhs_str);
                    s.push_str(&rhs_str);
                    Value::String(StringRef::Owned(s))
                }
            };
            vm.set_reg(base + a, result);
        }

        // Membership test.
        lumen_core::lir::OpCode::In => {
            let needle = vm.reg(base + b);
            let haystack = vm.reg(base + c);
            let result = match &haystack {
                Value::List(l) => l.contains(&needle),
                Value::Set(s) => s.contains(&needle),
                Value::Map(m) => {
                    let key = needle.as_string_resolved(&vm.strings);
                    m.contains_key(&key)
                }
                Value::String(StringRef::Owned(s)) => {
                    let n = needle.as_string_resolved(&vm.strings);
                    s.contains(n.as_str())
                }
                Value::String(StringRef::Interned(id)) => {
                    let s = vm.strings.resolve(*id).unwrap_or("").to_string();
                    let n = needle.as_string_resolved(&vm.strings);
                    s.contains(n.as_str())
                }
                _ => false,
            };
            vm.set_reg(base + a, Value::Bool(result));
        }

        // Type check.
        lumen_core::lir::OpCode::Is => {
            let val = vm.reg(base + b);
            let type_val = vm.reg(base + c);
            let type_str = type_val.as_string_resolved(&vm.strings);
            let matches = val.type_name_resolved(&vm.strings) == type_str;
            vm.set_reg(base + a, Value::Bool(matches));
        }

        // Closure creation.
        lumen_core::lir::OpCode::Closure => {
            let bx = instr.bx() as usize;
            vm.set_reg(
                base + a,
                Value::Closure(ClosureValue {
                    cell_idx: bx,
                    captures: Vec::new(),
                }),
            );
        }

        // Upvalue load (treat as register read — captures are stored in low registers).
        lumen_core::lir::OpCode::GetUpval => {
            let val = vm.reg(base + b);
            vm.set_reg(base + a, val);
        }

        // Upvalue store (inject into closure's capture vector).
        lumen_core::lir::OpCode::SetUpval => {
            let val = vm.reg(base + a);
            let mut closure = vm.reg_take(base + c);
            if let Value::Closure(ref mut cv) = closure {
                while cv.captures.len() <= b {
                    cv.captures.push(Value::Null);
                }
                cv.captures[b] = val;
            }
            vm.set_reg(base + c, closure);
        }

        // Trace reference.
        lumen_core::lir::OpCode::TraceRef => {
            let trace_ref = vm.next_trace_ref();
            vm.set_reg(base + a, Value::TraceRef(trace_ref));
        }

        // Emit output.
        lumen_core::lir::OpCode::Emit => {
            let emit_val = vm.reg(base + a);
            let s = emit_val.display_pretty();
            println!("{}", s);
            vm.output.push(s);
        }

        // Exponentiation — inline integer power (fast path for positive exponents).
        lumen_core::lir::OpCode::Pow => {
            let lhs = vm.reg(base + b);
            let rhs = vm.reg(base + c);
            let result = match (&lhs, &rhs) {
                (Value::Int(base_v), Value::Int(exp)) => {
                    if *exp >= 0 {
                        Value::Int(base_v.wrapping_pow(*exp as u32))
                    } else {
                        // Negative exponent → float result
                        Value::Float((*base_v as f64).powi(*exp as i32))
                    }
                }
                (Value::Float(base_v), Value::Int(exp)) => {
                    Value::Float(base_v.powi(*exp as i32))
                }
                (Value::Int(base_v), Value::Float(exp)) => {
                    Value::Float((*base_v as f64).powf(*exp))
                }
                (Value::Float(base_v), Value::Float(exp)) => {
                    Value::Float(base_v.powf(*exp))
                }
                _ => Value::Null,
            };
            vm.set_reg(base + a, result);
        }

        // Loop / iteration — these opcodes require IP manipulation and cannot
        // be safely executed from stencil_runtime (no IP pointer available here).
        // The stencil for these opcodes routes through lm_rt_stencil_runtime but
        // the actual control-flow side-effects are handled by the interpreter
        // when it re-executes after stencil exit.  We implement the data-side
        // effects only (counter decrement for Loop; index advance for ForLoop/ForIn).
        lumen_core::lir::OpCode::Loop => {
            // Decrement counter in R[A]; jump semantics handled by interpreter.
            let mut counter = vm.reg_take(base + a);
            if let Value::Int(ref mut n) = counter {
                *n -= 1;
            }
            vm.set_reg(base + a, counter);
        }
        lumen_core::lir::OpCode::ForPrep => {
            // Initialize loop: set index=0, len=collection_length in R[A+1], R[A+2].
            let iter_val = vm.reg(base + a);
            let len = match &iter_val {
                Value::List(l) => l.len(),
                Value::Set(s) => s.len(),
                Value::Tuple(t) => t.len(),
                _ => 0,
            } as i64;
            vm.set_reg(base + a + 1, Value::Int(0));
            vm.set_reg(base + a + 2, Value::Int(len));
        }
        lumen_core::lir::OpCode::ForLoop => {
            // Advance loop: load element into R[A+3], increment index in R[A+1].
            let idx = vm.reg(base + a + 1).as_int().unwrap_or(0);
            let len = vm.reg(base + a + 2).as_int().unwrap_or(0);
            if idx < len {
                let iter = vm.reg(base + a);
                let elem = match &iter {
                    Value::List(l) => l.get(idx as usize).cloned().unwrap_or(Value::Null),
                    Value::Set(s) => s.iter().nth(idx as usize).cloned().unwrap_or(Value::Null),
                    Value::Tuple(t) => t.get(idx as usize).cloned().unwrap_or(Value::Null),
                    _ => Value::Null,
                };
                vm.set_reg(base + a + 3, elem);
                vm.set_reg(base + a + 1, Value::Int(idx + 1));
            }
        }
        lumen_core::lir::OpCode::ForIn => {
            // for-in step: elem → R[C], index advance in R[A+1], bool in R[A].
            let idx = vm.reg(base + a + 1).as_int().unwrap_or(0);
            let iter = vm.reg(base + b);
            let (elem, has_more) = match &iter {
                Value::List(l) => {
                    let i = idx as usize;
                    if i < l.len() { (l[i].clone(), true) } else { (Value::Null, false) }
                }
                Value::Map(m) => {
                    let keys: Vec<_> = m.keys().cloned().collect();
                    let i = idx as usize;
                    if i < keys.len() {
                        let key = keys[i].clone();
                        let val = m.get(&key).cloned().unwrap_or(Value::Null);
                        (Value::new_tuple(vec![Value::String(StringRef::Owned(key)), val]), true)
                    } else {
                        (Value::Null, false)
                    }
                }
                Value::Set(s) => {
                    let items: Vec<_> = s.iter().cloned().collect();
                    let i = idx as usize;
                    if i < items.len() { (items[i].clone(), true) } else { (Value::Null, false) }
                }
                _ => (Value::Null, false),
            };
            vm.set_reg(base + c, elem);
            vm.set_reg(base + a + 1, Value::Int(idx + 1));
            vm.set_reg(base + a, Value::Bool(has_more));
        }

        // Schema validation — best-effort in stencil tier (no error propagation).
        lumen_core::lir::OpCode::Schema => {
            let module = vm.module().expect("stencil runtime: no module");
            let bx = instr.bx() as usize;
            let type_name = if bx < module.strings.len() {
                module.strings[bx].clone()
            } else {
                String::new()
            };
            let nb = vm.registers.get(base + a).copied().unwrap_or(NbValue::new_null());
            let _valid = vm.validate_schema(&nb, &type_name);
            // Schema validation errors cannot be propagated from this extern "C" context.
            // The interpreter will re-validate if necessary.
        }

        // Await / Spawn — these require full interpreter involvement for futures.
        // In stencil tier we store a placeholder so register layout is consistent.
        lumen_core::lir::OpCode::Await => {
            // In stencil tier, attempt a simple resolved-future check.
            // If the future is already in completed state in future_states, extract it.
            let awaited_val = vm.reg(base + b);
            let result = match &awaited_val {
                Value::Future(f) => {
                    let fid = f.id;
                    if let Some(crate::vm::FutureState::Completed(v)) =
                        vm.future_states.get(&fid)
                    {
                        v.clone()
                    } else {
                        // Future not yet resolved — leave Null; interpreter will handle.
                        Value::Null
                    }
                }
                other => other.clone(),
            };
            vm.set_reg(base + a, result);
        }
        lumen_core::lir::OpCode::Spawn => {
            // In stencil tier, Spawn creates a placeholder future value.
            // Full eager execution requires interpreter involvement.
            let bx = instr.bx() as usize;
            let future_id = vm.next_future_id;
            vm.next_future_id += 1;
            // Register as Pending in future_states so the interpreter can pick it up.
            vm.future_states.insert(future_id, crate::vm::FutureState::Pending);
            let _ = bx; // cell_idx tracked via FutureTask, not needed here directly
            vm.set_reg(
                base + a,
                Value::Future(lumen_core::values::FutureValue {
                    id: future_id,
                    state: lumen_core::values::FutureStatus::Pending,
                }),
            );
        }

        // ToolCall — requires runtime dispatcher, which is not accessible from
        // this extern "C" context without unsafe indirection. Log a no-op.
        lumen_core::lir::OpCode::ToolCall => {
            // ToolCall from the stencil tier: store Null in the result register.
            // The cell should not have been compiled to stencil if it has tool calls,
            // but we provide a safe fallback.
            vm.set_reg(base + a, Value::Null);
        }

        // TraceRef already handled above.

        _ => {}
    }
}
