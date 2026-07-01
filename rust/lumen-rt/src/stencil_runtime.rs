//! Runtime helpers for stencil (Tier 1) execution.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use lumen_core::heap_value::{ClosureData, HeapValue};
use lumen_core::lir::Instruction;
use lumen_core::nb_value::NbValue;
use lumen_core::vm_context::VmContext;

use crate::services::tools::ToolRequest;
use crate::vm::helpers::{merged_policy_for_tool, validate_tool_policy};
use crate::vm::VM;

// ---------------------------------------------------------------------------
// Thread-local flag for IsVariant skip semantics
// ---------------------------------------------------------------------------

thread_local! {
    /// Set to `true` by the IsVariant stencil runtime handler when the union
    /// tag matches, so the stitcher (or interpreter re-dispatch) can skip the
    /// next instruction.
    ///
    /// Reset to `false` at the start of each `lm_rt_stencil_runtime` call to
    /// avoid stale values from previous instructions.
    static IS_VARIANT_SKIP: Cell<bool> = const { Cell::new(false) };
}

/// ABI-stable sentinel returned by `lm_rt_stencil_runtime` to request
/// "skip next instruction" in stitched code.
const STENCIL_SKIP_NEXT_SENTINEL: u64 = 1;

#[inline]
fn stencil_moveown(vm: &mut VM, base: usize, a: usize, b: usize) {
    // Destructive move for heap values only. For inline values, a plain copy
    // avoids unnecessary zeroing and prevents accidental loss if the compiler
    // mis-emits MoveOwn for non-heap temporaries.
    let src = base + b;
    let dst = base + a;
    let nb = vm.reg_nb(src);
    if nb.is_heap_allocated() {
        // Take ownership: clear source to avoid double-drop.
        let val = vm.reg_take(src);
        vm.set_reg(dst, val);
    } else {
        // Inline value: copy without zeroing source.
        vm.set_reg_nb(dst, nb);
    }
}

/// Returns whether the most recent `IsVariant` stencil matched, consuming the flag.
///
/// Called by the stencil runtime dispatch path to consume the most recent
/// `IsVariant` match decision.
///
/// # Safety
///
/// This function is safe to call from any context, but is only meaningful
/// immediately after `lm_rt_stencil_runtime` processes an `IsVariant`
/// instruction.
#[no_mangle]
pub extern "C" fn lm_rt_is_variant_skip_flag() -> bool {
    IS_VARIANT_SKIP.with(|f| f.replace(false))
}

fn vm_from_ctx(ctx: *mut VmContext) -> &'static mut VM {
    debug_assert!(!ctx.is_null(), "stencil runtime: null VmContext");
    unsafe {
        let ptr = (*ctx).stack_pool as *mut VM;
        debug_assert!(!ptr.is_null(), "stencil runtime: null VM pointer");
        &mut *ptr
    }
}

#[derive(Clone, Copy)]
enum StencilArithOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    FloorDiv,
}

fn stencil_arith_numeric(
    vm: &mut VM,
    base: usize,
    a: usize,
    b: usize,
    c: usize,
    op: StencilArithOp,
) {
    let lhs = vm.reg_nb(base + b);
    let rhs = vm.reg_nb(base + c);

    if let (Some(x), Some(y)) = (lhs.as_int(), rhs.as_int()) {
        let out = match op {
            StencilArithOp::Add => x.checked_add(y),
            StencilArithOp::Sub => x.checked_sub(y),
            StencilArithOp::Mul => x.checked_mul(y),
            StencilArithOp::Div => {
                if y == 0 {
                    None
                } else {
                    x.checked_div(y)
                }
            }
            StencilArithOp::Mod => {
                if y == 0 {
                    None
                } else {
                    Some(x.rem_euclid(y))
                }
            }
            StencilArithOp::FloorDiv => {
                if y == 0 {
                    None
                } else {
                    Some(x.div_euclid(y))
                }
            }
        };

        if let Some(n) = out {
            vm.set_reg_nb(base + a, NbValue::new_int(n));
        } else {
            vm.set_reg_nb(base + a, NbValue::new_null());
        }
        return;
    }

    if let (Some(x), Some(y)) = (lhs.as_float(), rhs.as_float()) {
        let out = match op {
            StencilArithOp::Add => x + y,
            StencilArithOp::Sub => x - y,
            StencilArithOp::Mul => x * y,
            StencilArithOp::Div => x / y,
            StencilArithOp::Mod => x.rem_euclid(y),
            StencilArithOp::FloorDiv => (x / y).floor(),
        };
        vm.set_reg_nb(base + a, NbValue::new_float(out));
        return;
    }

    if let (Some(x), Some(y)) = (lhs.as_int(), rhs.as_float()) {
        let xf = x as f64;
        let out = match op {
            StencilArithOp::Add => xf + y,
            StencilArithOp::Sub => xf - y,
            StencilArithOp::Mul => xf * y,
            StencilArithOp::Div => xf / y,
            StencilArithOp::Mod => xf.rem_euclid(y),
            StencilArithOp::FloorDiv => (xf / y).floor(),
        };
        vm.set_reg_nb(base + a, NbValue::new_float(out));
        return;
    }

    if let (Some(x), Some(y)) = (lhs.as_float(), rhs.as_int()) {
        let yf = y as f64;
        let out = match op {
            StencilArithOp::Add => x + yf,
            StencilArithOp::Sub => x - yf,
            StencilArithOp::Mul => x * yf,
            StencilArithOp::Div => x / yf,
            StencilArithOp::Mod => x.rem_euclid(yf),
            StencilArithOp::FloorDiv => (x / yf).floor(),
        };
        vm.set_reg_nb(base + a, NbValue::new_float(out));
        return;
    }

    // The stencil runtime ABI cannot propagate VM errors; keep register state valid.
    vm.set_reg_nb(base + a, NbValue::new_null());
}

fn stencil_add(vm: &mut VM, base: usize, a: usize, b: usize, c: usize) {
    let lhs = vm.reg_nb(base + b);
    let rhs = vm.reg_nb(base + c);

    if let (Some(HeapValue::Str(l)), Some(HeapValue::Str(r))) =
        (lhs.as_heap_ref(), rhs.as_heap_ref())
    {
        let mut s = String::with_capacity(l.len() + r.len());
        s.push_str(l);
        s.push_str(r);
        vm.set_reg_nb(base + a, NbValue::new_str(&s));
        return;
    }

    if let (Some(HeapValue::List(la)), Some(HeapValue::List(lb))) =
        (lhs.as_heap_ref(), rhs.as_heap_ref())
    {
        let mut combined = Vec::with_capacity(la.len() + lb.len());
        combined.extend(la.iter().copied());
        combined.extend(lb.iter().copied());
        vm.set_reg_nb(base + a, NbValue::new_list(combined));
        return;
    }

    stencil_arith_numeric(vm, base, a, b, c, StencilArithOp::Add);
}

#[no_mangle]
pub unsafe extern "C" fn lm_rt_call(ctx: *mut VmContext, instr_word: u64) {
    let vm = vm_from_ctx(ctx);
    let instr: Instruction = unsafe { std::mem::transmute(instr_word) };
    // Nested Tier-1 execution from within a Tier-1 runtime callback is currently
    // fragile (it can corrupt the outer stitched frame state). Temporarily force
    // nested calls through the interpreter / other tiers.
    let saved = std::mem::replace(
        &mut vm.stencil_tier,
        crate::stencil_tier::StencilTier::disabled(),
    );
    let _ = vm.dispatch_call_from_stencil(instr.a as usize, instr.b as usize);
    vm.stencil_tier = saved;
}

#[no_mangle]
pub unsafe extern "C" fn lm_rt_tailcall(ctx: *mut VmContext, instr_word: u64) {
    let vm = vm_from_ctx(ctx);
    let instr: Instruction = unsafe { std::mem::transmute(instr_word) };
    let saved = std::mem::replace(
        &mut vm.stencil_tier,
        crate::stencil_tier::StencilTier::disabled(),
    );
    let _ = vm.dispatch_tailcall_from_stencil(instr.a as usize, instr.b as usize);
    vm.stencil_tier = saved;
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
pub unsafe extern "C" fn lm_rt_stencil_runtime(ctx: *mut VmContext, instr_word: u64) -> u64 {
    // Reset the IsVariant skip flag before processing this instruction so that
    // stale values from a previous IsVariant call cannot leak into non-IsVariant
    // stencil handlers.
    IS_VARIANT_SKIP.with(|f| f.set(false));

    let vm = vm_from_ctx(ctx);
    let instr: Instruction = unsafe { std::mem::transmute(instr_word) };
    // Use stencil_base (set before call_stitched) not current_base() (caller frame).
    let base = vm.stencil_base;
    let a = instr.a as usize;
    let b = instr.b as usize;
    let c = instr.c as usize;

    match instr.op {
        lumen_core::lir::OpCode::Move => {
            vm.reg_copy(base + a, base + b);
        }
        lumen_core::lir::OpCode::MoveOwn => {
            stencil_moveown(vm, base, a, b);
        }
        lumen_core::lir::OpCode::Add => {
            stencil_add(vm, base, a, b, c);
        }
        lumen_core::lir::OpCode::Sub => {
            stencil_arith_numeric(vm, base, a, b, c, StencilArithOp::Sub);
        }
        lumen_core::lir::OpCode::Mul => {
            stencil_arith_numeric(vm, base, a, b, c, StencilArithOp::Mul);
        }
        lumen_core::lir::OpCode::Div => {
            stencil_arith_numeric(vm, base, a, b, c, StencilArithOp::Div);
        }
        lumen_core::lir::OpCode::Mod => {
            stencil_arith_numeric(vm, base, a, b, c, StencilArithOp::Mod);
        }
        lumen_core::lir::OpCode::FloorDiv => {
            stencil_arith_numeric(vm, base, a, b, c, StencilArithOp::FloorDiv);
        }
        lumen_core::lir::OpCode::NewList | lumen_core::lir::OpCode::NewListStack => {
            let mut list = Vec::with_capacity(b);
            for i in 1..=b {
                list.push(vm.reg_nb(base + a + i));
            }
            vm.set_reg_nb(base + a, NbValue::new_list(list));
        }
        lumen_core::lir::OpCode::NewMap => {
            let mut map = BTreeMap::new();
            for i in 0..b {
                let k = vm.reg_nb(base + a + 1 + i * 2);
                let v = vm.reg_nb(base + a + 2 + i * 2);
                map.insert(k.display(), v);
            }
            vm.set_reg_nb(base + a, NbValue::new_map(map));
        }
        lumen_core::lir::OpCode::NewRecord => {
            let module = vm.module().expect("stencil runtime: no module");
            let type_idx = instr.bx() as usize;
            let type_name = if type_idx < module.strings.len() {
                &module.strings[type_idx]
            } else {
                "Unknown"
            };
            let mut fields = BTreeMap::new();
            let field_names_start = type_idx + 1;
            for i in 0..b {
                let field_name = if field_names_start + i < module.strings.len() {
                    &module.strings[field_names_start + i]
                } else {
                    ""
                };
                let val = vm.reg_nb(base + a + i + 1);
                fields.insert(field_name.to_string(), val);
            }
            vm.set_reg_nb(base + a, NbValue::new_record(type_name, fields));
        }
        lumen_core::lir::OpCode::NewTuple | lumen_core::lir::OpCode::NewTupleStack => {
            let mut elems = Vec::with_capacity(b);
            for i in 1..=b {
                elems.push(vm.reg_nb(base + a + i));
            }
            vm.set_reg_nb(base + a, NbValue::new_tuple(elems));
        }
        lumen_core::lir::OpCode::NewSet => {
            let mut set = BTreeSet::new();
            for i in 1..=b {
                set.insert(vm.reg_nb(base + a + i));
            }
            vm.set_reg_nb(base + a, NbValue::new_heap(HeapValue::Set(Arc::new(set))));
        }
        lumen_core::lir::OpCode::GetField => {
            let module = vm.module().expect("stencil runtime: no module");
            let obj = vm.reg_nb(base + b);
            let field_name = if c < module.strings.len() {
                &module.strings[c]
            } else {
                ""
            };
            let val = match obj.as_heap_ref() {
                Some(HeapValue::Record(r)) => r
                    .fields
                    .get(field_name)
                    .copied()
                    .unwrap_or(NbValue::new_null()),
                Some(HeapValue::Map(m)) => {
                    m.get(field_name).copied().unwrap_or(NbValue::new_null())
                }
                _ => NbValue::new_null(),
            };
            vm.set_reg_nb(base + a, val);
        }
        lumen_core::lir::OpCode::SetField => {
            let module = vm.module().expect("stencil runtime: no module");
            let val = vm.reg_nb(base + c);
            let field_name = if b < module.strings.len() {
                module.strings[b].clone()
            } else {
                String::new()
            };
            let mut target = vm.reg_nb(base + a);
            if let Some(HeapValue::Record(r)) = target.as_heap_ref() {
                let mut updated = (**r).clone();
                updated.fields.insert(field_name, val);
                target = NbValue::new_heap(HeapValue::Record(Arc::new(updated)));
            }
            vm.set_reg_nb(base + a, target);
        }
        lumen_core::lir::OpCode::GetIndex => {
            let obj = vm.reg_nb(base + b);
            let idx = vm.reg_nb(base + c);
            let val = match (obj.as_heap_ref(), idx.as_int()) {
                (Some(HeapValue::List(l)), Some(i)) => {
                    let len = l.len() as i64;
                    let effective = if i < 0 { i + len } else { i };
                    if effective < 0 || effective >= len {
                        return 0;
                    }
                    l[effective as usize]
                }
                (Some(HeapValue::Tuple(t)), Some(i)) => {
                    let len = t.len() as i64;
                    let effective = if i < 0 { i + len } else { i };
                    if effective < 0 || effective >= len {
                        return 0;
                    }
                    t[effective as usize]
                }
                (Some(HeapValue::Map(m)), _) => m
                    .get(&idx.display())
                    .copied()
                    .unwrap_or(NbValue::new_null()),
                (Some(HeapValue::Record(r)), _) => r
                    .fields
                    .get(&idx.display())
                    .copied()
                    .unwrap_or(NbValue::new_null()),
                (Some(HeapValue::Set(s)), Some(i)) => {
                    let len = s.len() as i64;
                    let effective = if i < 0 { i + len } else { i };
                    if effective < 0 || effective >= len {
                        return 0;
                    }
                    s.iter()
                        .nth(effective as usize)
                        .copied()
                        .unwrap_or(NbValue::new_null())
                }
                _ => NbValue::new_null(),
            };
            vm.set_reg_nb(base + a, val);
        }
        lumen_core::lir::OpCode::SetIndex => {
            let val = vm.reg_nb(base + c);
            let key = vm.reg_nb(base + b);
            let mut target = vm.reg_nb(base + a);
            match target.as_heap_ref() {
                Some(HeapValue::List(l)) => {
                    if let Some(i) = key.as_int() {
                        let len = l.len() as i64;
                        let effective = if i < 0 { i + len } else { i };
                        if effective < 0 || effective >= len {
                            return 0;
                        }
                        let mut new_list = (**l).clone();
                        new_list[effective as usize] = val;
                        target = NbValue::new_list(new_list);
                    }
                }
                Some(HeapValue::Tuple(t)) => {
                    if let Some(i) = key.as_int() {
                        let len = t.len() as i64;
                        let effective = if i < 0 { i + len } else { i };
                        if effective < 0 || effective >= len {
                            return 0;
                        }
                        let mut new_tuple = (**t).clone();
                        new_tuple[effective as usize] = val;
                        target = NbValue::new_tuple(new_tuple);
                    }
                }
                Some(HeapValue::Map(m)) => {
                    let k = key.display();
                    let mut new_map = (**m).clone();
                    new_map.insert(k, val);
                    target = NbValue::new_map(new_map);
                }
                Some(HeapValue::Record(r)) => {
                    let k = key.display();
                    let mut updated = (**r).clone();
                    updated.fields.insert(k, val);
                    target = NbValue::new_heap(HeapValue::Record(Arc::new(updated)));
                }
                _ => {}
            }
            vm.set_reg_nb(base + a, target);
        }

        // Tuple element access by constant index.
        lumen_core::lir::OpCode::GetTuple => {
            let obj = vm.reg_nb(base + b);
            let val = match obj.as_heap_ref() {
                Some(HeapValue::Tuple(t)) => t.get(c).copied().unwrap_or(NbValue::new_null()),
                Some(HeapValue::List(l)) => l.get(c).copied().unwrap_or(NbValue::new_null()),
                _ => NbValue::new_null(),
            };
            vm.set_reg_nb(base + a, val);
        }

        // Union construction.
        lumen_core::lir::OpCode::NewUnion => {
            let tag_val = vm.reg_nb(base + b);
            let payload = vm.reg_nb(base + c);
            let tag = match tag_val.as_heap_ref() {
                Some(HeapValue::Str(s)) => Arc::clone(s),
                _ => match vm.reg(base + b) {
                    lumen_core::values::Value::String(lumen_core::values::StringRef::Interned(
                        id,
                    )) => vm.strings.get_arc(id).unwrap_or_else(|| Arc::from("")),
                    lumen_core::values::Value::String(lumen_core::values::StringRef::Owned(s)) => {
                        vm.strings.get_or_intern_arc(&s)
                    }
                    _ => {
                        let tag_display = tag_val.display();
                        vm.strings.get_or_intern_arc(tag_display.as_str())
                    }
                },
            };
            vm.set_reg_nb(base + a, NbValue::new_union_arc(tag, payload));
        }

        // Type variant check (skip next if matched).
        lumen_core::lir::OpCode::IsVariant => {
            // Perform the actual tag comparison and record the match result in a
            // thread-local flag. This is consumed at function end and converted
            // into an ABI-stable integer sentinel for stitched branching.
            let tag_idx = instr.bx() as usize;
            // Clone the tag string to avoid borrow conflict between module and vm.strings.
            let tag = {
                let module = vm.module().expect("stencil runtime: no module");
                if tag_idx < module.strings.len() {
                    module.strings[tag_idx].as_str()
                } else {
                    ""
                }
            };
            let val = vm.reg_nb(base + a);
            let matched = match val.as_heap_ref() {
                Some(HeapValue::Union(u)) => u.tag.as_ref() == tag,
                _ => false,
            };
            IS_VARIANT_SKIP.with(|f| f.set(matched));
        }

        // Union payload extraction.
        lumen_core::lir::OpCode::Unbox => {
            let val = vm.reg_nb(base + b);
            let result = if let Some(HeapValue::Union(u)) = val.as_heap_ref() {
                u.payload
            } else {
                NbValue::new_null()
            };
            vm.set_reg_nb(base + a, result);
        }

        // List append.
        lumen_core::lir::OpCode::Append => {
            let val = vm.reg_nb(base + b);
            let mut target = vm.reg_nb(base + a);
            if let Some(HeapValue::List(l)) = target.as_heap_ref() {
                let mut new_list = (**l).clone();
                new_list.push(val);
                target = NbValue::new_list(new_list);
            }
            vm.set_reg_nb(base + a, target);
        }

        // String / list concatenation.
        lumen_core::lir::OpCode::Concat => {
            let lhs = vm.reg_nb(base + b);
            let rhs = vm.reg_nb(base + c);
            let result = match (lhs.as_heap_ref(), rhs.as_heap_ref()) {
                (Some(HeapValue::Str(l)), Some(HeapValue::Str(r))) => {
                    NbValue::new_str(&format!("{}{}", l, r))
                }
                (Some(HeapValue::List(l)), Some(HeapValue::List(r))) => {
                    let mut v = (**l).clone();
                    v.extend_from_slice(r);
                    NbValue::new_list(v)
                }
                _ => NbValue::new_str(&format!("{}{}", lhs.display(), rhs.display())),
            };
            vm.set_reg_nb(base + a, result);
        }

        // Membership test.
        lumen_core::lir::OpCode::In => {
            let needle = vm.reg_nb(base + b);
            let haystack = vm.reg_nb(base + c);
            let result = match haystack.as_heap_ref() {
                Some(HeapValue::List(l)) => l.iter().any(|v| *v == needle),
                Some(HeapValue::Set(s)) => s.contains(&needle),
                Some(HeapValue::Map(m)) => m.contains_key(&needle.display()),
                Some(HeapValue::Str(s)) => s.contains(needle.display().as_str()),
                _ => false,
            };
            vm.set_reg_nb(base + a, NbValue::new_bool(result));
        }

        // Type check.
        lumen_core::lir::OpCode::Is => {
            let val = vm.reg_nb(base + b);
            let type_val = vm.reg_nb(base + c);
            let type_str = type_val.display();
            let matches = val.type_name() == type_str;
            vm.set_reg_nb(base + a, NbValue::new_bool(matches));
        }

        // Closure creation.
        lumen_core::lir::OpCode::Closure => {
            let bx = instr.bx() as usize;
            vm.set_reg_nb(
                base + a,
                NbValue::new_heap(HeapValue::Closure(Arc::new(ClosureData {
                    cell_idx: bx,
                    captures: Vec::new(),
                }))),
            );
        }

        // Upvalue load (treat as register read — captures are stored in low registers).
        lumen_core::lir::OpCode::GetUpval => {
            let val = vm.reg_nb(base + b);
            vm.set_reg_nb(base + a, val);
        }

        // Upvalue store (inject into closure's capture vector).
        lumen_core::lir::OpCode::SetUpval => {
            let val = vm.reg_nb(base + a);
            let mut closure = vm.reg_nb(base + c);
            if let Some(HeapValue::Closure(c)) = closure.as_heap_ref() {
                let mut updated = (**c).clone();
                while updated.captures.len() <= b {
                    updated.captures.push(NbValue::new_null());
                }
                updated.captures[b] = val;
                closure = NbValue::new_heap(HeapValue::Closure(Arc::new(updated)));
            }
            vm.set_reg_nb(base + c, closure);
        }

        // Trace reference.
        lumen_core::lir::OpCode::TraceRef => {
            let trace_ref = vm.next_trace_ref();
            vm.set_reg_nb(
                base + a,
                NbValue::new_heap(HeapValue::TraceRef(trace_ref.seq)),
            );
        }

        // Emit output.
        lumen_core::lir::OpCode::Emit => {
            let emit_val = vm.reg_nb(base + a);
            let s = emit_val.display();
            println!("{}", s);
            vm.output.push(s);
        }

        // Exponentiation — inline integer power (fast path for positive exponents).
        lumen_core::lir::OpCode::Pow => {
            let lhs = vm.reg_nb(base + b);
            let rhs = vm.reg_nb(base + c);
            let result = match (lhs.as_int(), lhs.as_float(), rhs.as_int(), rhs.as_float()) {
                (Some(base_v), _, Some(exp), _) => {
                    if exp >= 0 {
                        NbValue::new_int(base_v.wrapping_pow(exp as u32))
                    } else {
                        NbValue::new_float((base_v as f64).powi(exp as i32))
                    }
                }
                (_, Some(base_v), Some(exp), _) => NbValue::new_float(base_v.powi(exp as i32)),
                (Some(base_v), _, _, Some(exp)) => NbValue::new_float((base_v as f64).powf(exp)),
                (_, Some(base_v), _, Some(exp)) => NbValue::new_float(base_v.powf(exp)),
                _ => NbValue::new_null(),
            };
            vm.set_reg_nb(base + a, result);
        }

        // Loop / iteration — these opcodes require IP manipulation and cannot
        // be safely executed from stencil_runtime (no IP pointer available here).
        // The stencil for these opcodes routes through lm_rt_stencil_runtime but
        // the actual control-flow side-effects are handled by the interpreter
        // when it re-executes after stencil exit.  We implement the data-side
        // effects only (counter decrement for Loop; index advance for ForLoop/ForIn).
        lumen_core::lir::OpCode::Loop => {
            // Decrement counter in R[A]; jump semantics handled by interpreter.
            let counter = vm.reg_nb(base + a);
            if let Some(n) = counter.as_int() {
                vm.set_reg_nb(base + a, NbValue::new_int(n - 1));
            }
        }
        lumen_core::lir::OpCode::ForPrep => {
            // Initialize loop: set index=0, len=collection_length in R[A+1], R[A+2].
            let iter_val = vm.reg_nb(base + a);
            let len = match iter_val.as_heap_ref() {
                Some(HeapValue::List(l)) => l.len(),
                Some(HeapValue::Set(s)) => s.len(),
                Some(HeapValue::Tuple(t)) => t.len(),
                _ => 0,
            } as i64;
            vm.set_reg_nb(base + a + 1, NbValue::new_int(0));
            vm.set_reg_nb(base + a + 2, NbValue::new_int(len));
        }
        lumen_core::lir::OpCode::ForLoop => {
            // Advance loop: load element into R[A+3], increment index in R[A+1].
            let idx = vm.reg_nb(base + a + 1).as_int().unwrap_or(0);
            let len = vm.reg_nb(base + a + 2).as_int().unwrap_or(0);
            if idx < len {
                let iter = vm.reg_nb(base + a);
                let elem = match iter.as_heap_ref() {
                    Some(HeapValue::List(l)) => {
                        l.get(idx as usize).copied().unwrap_or(NbValue::new_null())
                    }
                    Some(HeapValue::Set(s)) => s
                        .iter()
                        .nth(idx as usize)
                        .copied()
                        .unwrap_or(NbValue::new_null()),
                    Some(HeapValue::Tuple(t)) => {
                        t.get(idx as usize).copied().unwrap_or(NbValue::new_null())
                    }
                    _ => NbValue::new_null(),
                };
                vm.set_reg_nb(base + a + 3, elem);
                vm.set_reg_nb(base + a + 1, NbValue::new_int(idx + 1));
            }
        }
        lumen_core::lir::OpCode::ForIn => {
            // for-in step: elem → R[C], index advance in R[A+1], bool in R[A].
            let idx = vm.reg_nb(base + a + 1).as_int().unwrap_or(0);
            let iter = vm.reg_nb(base + b);
            let (elem, has_more) = match iter.as_heap_ref() {
                Some(HeapValue::List(l)) => {
                    let i = idx as usize;
                    if i < l.len() {
                        (l[i], true)
                    } else {
                        (NbValue::new_null(), false)
                    }
                }
                Some(HeapValue::Map(m)) => {
                    let keys: Vec<_> = m.keys().cloned().collect();
                    let i = idx as usize;
                    if i < keys.len() {
                        let key = keys[i].clone();
                        let val = m.get(&key).copied().unwrap_or(NbValue::new_null());
                        (NbValue::new_tuple(vec![NbValue::new_str(&key), val]), true)
                    } else {
                        (NbValue::new_null(), false)
                    }
                }
                Some(HeapValue::Set(s)) => {
                    let items: Vec<_> = s.iter().copied().collect();
                    let i = idx as usize;
                    if i < items.len() {
                        (items[i], true)
                    } else {
                        (NbValue::new_null(), false)
                    }
                }
                _ => (NbValue::new_null(), false),
            };
            vm.set_reg_nb(base + c, elem);
            vm.set_reg_nb(base + a + 1, NbValue::new_int(idx + 1));
            vm.set_reg_nb(base + a, NbValue::new_bool(has_more));
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
            let nb = vm
                .registers
                .get(base + a)
                .copied()
                .unwrap_or(NbValue::new_null());
            let _valid = vm.validate_schema(&nb, &type_name);
            // Schema validation errors cannot be propagated from this extern "C" context.
            // The interpreter will re-validate if necessary.
        }

        // Await / Spawn — these require full interpreter involvement for futures.
        // In stencil tier we store a placeholder so register layout is consistent.
        lumen_core::lir::OpCode::Await => {
            // In stencil tier, attempt a simple resolved-future check.
            // If the future is already in completed state in future_states, extract it.
            let awaited_val = vm.reg_nb(base + b);
            let result = match awaited_val.as_heap_ref() {
                Some(HeapValue::Future(f)) => {
                    let fid = f.id;
                    if let Some(crate::vm::FutureState::Completed(v)) = vm.future_states.get(&fid) {
                        NbValue::new_str(&v.display_pretty())
                    } else {
                        NbValue::new_null()
                    }
                }
                _ => awaited_val,
            };
            vm.set_reg_nb(base + a, result);
        }
        lumen_core::lir::OpCode::Spawn => {
            // In stencil tier, Spawn creates a placeholder future value.
            // Full eager execution requires interpreter involvement.
            let bx = instr.bx() as usize;
            let future_id = vm.next_future_id;
            vm.next_future_id += 1;
            // Register as Pending in future_states so the interpreter can pick it up.
            vm.future_states
                .insert(future_id, crate::vm::FutureState::Pending);
            let _ = bx; // cell_idx tracked via FutureTask, not needed here directly
            vm.set_reg_nb(
                base + a,
                NbValue::new_heap(HeapValue::Future(Arc::new(
                    lumen_core::heap_value::FutureData {
                        id: future_id,
                        status: lumen_core::heap_value::FutureStatus::Pending,
                        schedule: lumen_core::heap_value::FutureSchedule::Eager,
                    },
                ))),
            );
        }

        // ToolCall — dispatch via the VM's synchronous tool dispatcher.
        lumen_core::lir::OpCode::ToolCall => {
            // `bx` is the index into module.tools for the tool descriptor.
            let bx = instr.bx() as usize;
            let (tool_id, tool_version, tool_alias) = {
                let module = vm.module().expect("stencil runtime: no module");
                if let Some(tool) = module.tools.get(bx) {
                    (
                        tool.tool_id.clone(),
                        tool.version.clone(),
                        tool.alias.clone(),
                    )
                } else {
                    // Tool index out of bounds — store Null and return.
                    vm.set_reg_nb(base + a, NbValue::new_null());
                    return 0;
                }
            };

            // Match interpreter convention: if R[A] is a map, treat it as args;
            // otherwise look at R[A+1].
            let mut args_map = serde_json::Map::new();
            let primary = base + a;
            let primary_val = if primary < vm.registers.len() {
                Some(vm.reg_nb(primary))
            } else {
                None
            };
            let arg_map_reg = match primary_val.as_ref().and_then(|v| v.as_heap_ref()) {
                Some(HeapValue::Map(_)) => Some(primary),
                Some(_) => primary.checked_add(1),
                None => None,
            };
            if let Some(arg_map_reg) = arg_map_reg {
                if arg_map_reg < vm.registers.len() {
                    let map_val = vm.reg_nb(arg_map_reg);
                    if let Some(HeapValue::Map(m)) = map_val.as_heap_ref() {
                        for (k, v) in m.iter() {
                            args_map.insert(k.clone(), serde_json::Value::String(v.display()));
                        }
                    }
                }
            }
            let args_json = serde_json::Value::Object(args_map);

            // Validate the tool policy (if any).
            let policy = {
                let module = vm.module().expect("stencil runtime: no module");
                merged_policy_for_tool(module, &tool_alias)
            };
            if let Err(msg) = validate_tool_policy(&policy, &args_json) {
                let err_msg = format!("policy violation for '{}': {}", tool_alias, msg);
                vm.set_reg_nb(base + a, NbValue::new_str(&err_msg));
                return 0;
            }

            // Mirror interpreter effect-budget enforcement for tool alias and
            // tool_id prefix (e.g. "http" from "http.get").
            for budget_key in [tool_alias.as_str(), tool_id.split('.').next().unwrap_or("")] {
                if let Some((remaining, limit)) = vm.effect_budgets.get_mut(budget_key) {
                    if *remaining == 0 {
                        let err_msg = format!(
                            "effect budget exceeded for '{}': limit {} reached",
                            budget_key, limit
                        );
                        vm.set_reg_nb(base + a, NbValue::new_str(&err_msg));
                        return 0;
                    }
                    *remaining -= 1;
                }
            }

            // Dispatch synchronously via the tool_dispatcher if one is configured.
            let request = ToolRequest {
                tool_id: tool_id.clone(),
                version: tool_version.clone(),
                args: args_json,
                policy,
            };
            if let Some(dispatcher) = vm.tool_dispatcher.as_ref() {
                match dispatcher.dispatch(&request) {
                    Ok(response) => {
                        vm.set_reg_nb(base + a, NbValue::new_str(&response.outputs.to_string()));
                    }
                    Err(e) => {
                        vm.set_reg_nb(base + a, NbValue::new_str(&e.to_string()));
                    }
                }
            } else {
                // No dispatcher configured — store a pending placeholder string.
                vm.set_reg_nb(base + a, NbValue::new_str("<<tool call pending>>"));
            }
        }

        // TraceRef already handled above.
        _ => {}
    }

    // Use an ABI-stable integer sentinel (not bool) so stitched code can
    // safely branch with `test rax, rax` / `jnz` regardless of Rust bool ABI.
    if matches!(instr.op, lumen_core::lir::OpCode::IsVariant) && lm_rt_is_variant_skip_flag() {
        STENCIL_SKIP_NEXT_SENTINEL
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::services::tools::{ToolDispatcher, ToolError, ToolResponse};
    use lumen_core::lir::{Instruction, LirCell, LirModule, LirTool, OpCode};

    struct EchoArgsDispatcher;

    impl ToolDispatcher for EchoArgsDispatcher {
        fn dispatch(&self, request: &ToolRequest) -> Result<ToolResponse, ToolError> {
            Ok(ToolResponse {
                outputs: request.args.clone(),
                latency_ms: 0,
            })
        }
    }

    fn module_with_tool(alias: &str, tool_id: &str) -> LirModule {
        LirModule {
            version: "1.0".into(),
            doc_hash: String::new(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: None,
                registers: 4,
                constants: vec![],
                instructions: vec![],
                effect_handler_metas: vec![],
                osr_points: vec![],
            }],
            tools: vec![LirTool {
                alias: alias.to_string(),
                tool_id: tool_id.to_string(),
                version: "1".into(),
                mcp_url: None,
            }],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            handlers: vec![],
            effect_binds: vec![],
        }
    }

    fn run_toolcall(vm: &mut VM, a: u16) -> u64 {
        vm.stencil_base = 0;
        vm.registers.resize(8, NbValue::new_null());
        let ctx = vm.vm_ctx.as_ptr();
        unsafe {
            (*ctx).stack_pool = vm as *mut VM as *mut ();
        }
        let instr = Instruction::abx(OpCode::ToolCall, a, 0);
        let word: u64 = unsafe { std::mem::transmute(instr) };
        unsafe { lm_rt_stencil_runtime(ctx, word) }
    }

    #[test]
    fn toolcall_uses_primary_arg_map_when_present() {
        let mut vm = VM::new();
        vm.tool_dispatcher = Some(Box::new(EchoArgsDispatcher));
        vm.load(module_with_tool("Echo", "echo.call"));
        vm.registers.resize(8, NbValue::new_null());

        let mut args = BTreeMap::new();
        args.insert("x".to_string(), lumen_core::values::Value::Int(7));
        vm.set_reg(0, lumen_core::values::Value::new_map(args));

        let sentinel = run_toolcall(&mut vm, 0);
        assert_eq!(sentinel, 0);

        match vm.reg(0) {
            lumen_core::values::Value::Map(m) => {
                assert_eq!(m.get("x"), Some(&lumen_core::values::Value::Int(7)));
            }
            other => panic!("expected map output from dispatcher, got {other:?}"),
        }
    }

    #[test]
    fn toolcall_enforces_effect_budget_in_stencil_runtime() {
        let mut vm = VM::new();
        vm.tool_dispatcher = Some(Box::new(EchoArgsDispatcher));
        vm.set_effect_budget("Echo", 0);
        vm.load(module_with_tool("Echo", "echo.call"));
        vm.registers.resize(8, NbValue::new_null());

        vm.set_reg(0, lumen_core::values::Value::new_map(BTreeMap::new()));
        let sentinel = run_toolcall(&mut vm, 0);
        assert_eq!(sentinel, 0);

        match vm.reg(0) {
            lumen_core::values::Value::String(lumen_core::values::StringRef::Owned(msg)) => {
                assert!(msg.contains("effect budget exceeded"));
                assert!(msg.contains("Echo"));
            }
            other => panic!("expected budget error string, got {other:?}"),
        }
    }
}
