//! JIT runtime helpers that require access to the VM instance.

use lumen_codegen::jit::JitString;
use lumen_core::heap_value::HeapValue;
use lumen_core::nb_value::NbValue;
use lumen_core::vm_context::VmContext;

use crate::json_parser::parse_json_optimized;
use crate::services::tools::ToolRequest;
use crate::vm::helpers::{merged_policy_for_tool, validate_tool_policy};
use crate::vm::intrinsics::json_encode::{encode_json_compact, encode_json_pretty};
use crate::vm::VM;

#[inline]
fn decode_nb_value(nb: i64) -> NbValue {
    NbValue::from_bits(nb as u64)
}

#[inline]
fn wrap_nb_value(value: NbValue) -> i64 {
    value.to_bits() as i64
}

/// Invoke a tool by index with a pre-built args map.
///
/// Returns a NaN-boxed value or NAN_BOX_NULL on failure. On failure, records a
/// message in `vm_ctx.last_error` for the interpreter/JIT boundary to surface.
#[no_mangle]
pub extern "C" fn jit_rt_tool_call(vm_ctx: *mut VmContext, tool_id: i32, args_map_ptr: i64) -> i64 {
    let ctx = unsafe { vm_ctx.as_mut() };
    let Some(ctx) = ctx else {
        return NbValue::NAN_BOX_NULL as i64;
    };

    let vm_ptr = ctx.stack_pool as *mut VM;
    if vm_ptr.is_null() {
        ctx.set_error("jit_rt_tool_call: VM pointer missing".to_string());
        return NbValue::NAN_BOX_NULL as i64;
    }
    let vm = unsafe { &mut *vm_ptr };

    let module = match vm.module.as_ref() {
        Some(module) => module,
        None => {
            ctx.set_error("jit_rt_tool_call: no module loaded".to_string());
            return NbValue::NAN_BOX_NULL as i64;
        }
    };

    let tool_idx = tool_id as usize;
    let tool = if let Some(tool) = module.tools.get(tool_idx) {
        tool
    } else {
        ctx.set_error(format!(
            "jit_rt_tool_call: tool index {} out of bounds",
            tool_idx
        ));
        return NbValue::NAN_BOX_NULL as i64;
    };

    let mut args_map = serde_json::Map::new();
    let args_val = decode_nb_value(args_map_ptr);
    if let Some(HeapValue::Map(m)) = args_val.as_heap_ref() {
        for (k, v) in m.iter() {
            args_map.insert(k.clone(), serde_json::Value::String(v.display()));
        }
    }
    let args_json = serde_json::Value::Object(args_map);

    let tool_id = tool.tool_id.clone();
    let tool_version = tool.version.clone();
    let tool_alias = tool.alias.clone();
    let policy = merged_policy_for_tool(module, &tool_alias);
    if let Err(msg) = validate_tool_policy(&policy, &args_json) {
        ctx.set_error(format!("policy violation for '{}': {}", tool_alias, msg));
        return NbValue::NAN_BOX_NULL as i64;
    }

    for budget_key in [tool_alias.as_str(), tool_id.split('.').next().unwrap_or("")] {
        if let Some((remaining, limit)) = vm.effect_budgets.get_mut(budget_key) {
            if *remaining == 0 {
                ctx.set_error(format!(
                    "effect budget exceeded for '{}': limit {} reached",
                    budget_key, limit
                ));
                return NbValue::NAN_BOX_NULL as i64;
            }
            *remaining -= 1;
        }
    }

    let request = ToolRequest {
        tool_id,
        version: tool_version,
        args: args_json,
        policy,
    };

    if let Some(dispatcher) = vm.tool_dispatcher.as_ref() {
        match dispatcher.dispatch(&request) {
            Ok(response) => wrap_nb_value(NbValue::new_str(&response.outputs.to_string())),
            Err(e) => {
                ctx.set_error(e.to_string());
                NbValue::NAN_BOX_NULL as i64
            }
        }
    } else {
        ctx.set_error("tool dispatcher not configured".to_string());
        NbValue::NAN_BOX_NULL as i64
    }
}

/// Validate a value against a schema by name.
#[no_mangle]
pub extern "C" fn jit_rt_schema_validate(
    vm_ctx: *mut VmContext,
    value_ptr: i64,
    schema_id: i32,
) -> i64 {
    let ctx = unsafe { vm_ctx.as_mut() };
    let Some(ctx) = ctx else {
        return NbValue::NAN_BOX_FALSE as i64;
    };

    let vm_ptr = ctx.stack_pool as *mut VM;
    if vm_ptr.is_null() {
        ctx.set_error("jit_rt_schema_validate: VM pointer missing".to_string());
        return NbValue::NAN_BOX_FALSE as i64;
    }
    let vm = unsafe { &mut *vm_ptr };

    let module = match vm.module.as_ref() {
        Some(module) => module,
        None => {
            ctx.set_error("jit_rt_schema_validate: no module loaded".to_string());
            return NbValue::NAN_BOX_FALSE as i64;
        }
    };

    let schema_idx = schema_id as usize;
    let schema_name = if schema_idx < module.strings.len() {
        module.strings[schema_idx].as_str()
    } else {
        ""
    };

    let nb = NbValue(value_ptr as u64);
    let valid = vm.validate_schema(&nb, schema_name);
    if valid {
        NbValue::NAN_BOX_TRUE as i64
    } else {
        NbValue::NAN_BOX_FALSE as i64
    }
}

/// Create a new trace reference value.
#[no_mangle]
pub extern "C" fn jit_rt_trace_ref(vm_ctx: *mut VmContext) -> i64 {
    let ctx = unsafe { vm_ctx.as_mut() };
    let Some(ctx) = ctx else {
        return NbValue::NAN_BOX_NULL as i64;
    };

    let vm_ptr = ctx.stack_pool as *mut VM;
    if vm_ptr.is_null() {
        ctx.set_error("jit_rt_trace_ref: VM pointer missing".to_string());
        return NbValue::NAN_BOX_NULL as i64;
    }
    let vm = unsafe { &mut *vm_ptr };
    let trace = vm.next_trace_ref();
    wrap_nb_value(NbValue::new_heap(HeapValue::TraceRef(trace.seq)))
}

/// For-in iterator step helper.
#[no_mangle]
pub extern "C" fn jit_rt_for_in(_ctx: *mut VmContext, iterator_ptr: i64, index_nb: i64) -> i64 {
    let iterator = decode_nb_value(iterator_ptr);
    let index_val = decode_nb_value(index_nb);
    let index = index_val.as_int().unwrap_or(0);

    if index < 0 {
        return NbValue::NAN_BOX_NULL as i64;
    }

    let element = match iterator.as_heap_ref() {
        Some(HeapValue::List(list)) => list
            .get(index as usize)
            .copied()
            .unwrap_or(NbValue::new_null()),
        Some(HeapValue::Tuple(tuple)) => tuple
            .get(index as usize)
            .copied()
            .unwrap_or(NbValue::new_null()),
        Some(HeapValue::Map(map)) => {
            let keys: Vec<_> = map.keys().collect();
            if (index as usize) < keys.len() {
                let key = keys[index as usize].clone();
                let value = map.get(&key).copied().unwrap_or(NbValue::new_null());
                NbValue::new_tuple(vec![NbValue::new_str(&key), value])
            } else {
                NbValue::new_null()
            }
        }
        Some(HeapValue::Set(set)) => {
            let items: Vec<_> = set.iter().copied().collect();
            items
                .get(index as usize)
                .copied()
                .unwrap_or(NbValue::new_null())
        }
        _ => NbValue::new_null(),
    };

    if element.is_null() {
        NbValue::NAN_BOX_NULL as i64
    } else {
        element.to_bits() as i64
    }
}

/// Membership test helper (In opcode).
#[no_mangle]
pub extern "C" fn jit_rt_in(_ctx: *mut VmContext, value_ptr: i64, collection_ptr: i64) -> i64 {
    let needle = NbValue::from_bits(value_ptr as u64);
    let coll = NbValue::from_bits(collection_ptr as u64);
    let found = match coll.as_heap_ref() {
        Some(HeapValue::List(l)) => l.iter().any(|v| *v == needle),
        Some(HeapValue::Tuple(t)) => t.iter().any(|v| *v == needle),
        Some(HeapValue::Set(s)) => s.contains(&needle),
        Some(HeapValue::Map(m)) => m.contains_key(&needle.display()),
        Some(HeapValue::Str(s)) => s.contains(needle.display().as_str()),
        _ => false,
    };
    NbValue::new_bool(found).to_bits() as i64
}

/// Type test helper (Is opcode).
#[no_mangle]
pub extern "C" fn jit_rt_is(vm_ctx: *mut VmContext, value_ptr: i64, type_id: i64) -> i64 {
    let ctx = unsafe { vm_ctx.as_mut() };
    let Some(ctx) = ctx else {
        return NbValue::NAN_BOX_FALSE as i64;
    };

    let vm_ptr = ctx.stack_pool as *mut VM;
    if vm_ptr.is_null() {
        ctx.set_error("jit_rt_is: VM pointer missing".to_string());
        return NbValue::NAN_BOX_FALSE as i64;
    }
    let _vm = unsafe { &mut *vm_ptr };

    let type_val = decode_nb_value(type_id);
    let type_str = type_val.display();
    let value = decode_nb_value(value_ptr);
    let matches = value.type_name() == type_str;

    if matches {
        NbValue::NAN_BOX_TRUE as i64
    } else {
        NbValue::NAN_BOX_FALSE as i64
    }
}

/// Parse JSON from a JitString pointer and return a NaN-boxed NbValue.
#[no_mangle]
pub extern "C" fn jit_rt_json_parse(vm_ctx: *mut VmContext, str_ptr: i64) -> i64 {
    let ctx = unsafe { vm_ctx.as_mut() };
    let Some(ctx) = ctx else {
        return NbValue::NAN_BOX_NULL as i64;
    };

    let vm_ptr = ctx.stack_pool as *mut VM;
    if vm_ptr.is_null() {
        ctx.set_error("jit_rt_json_parse: VM pointer missing".to_string());
        return NbValue::NAN_BOX_NULL as i64;
    }
    let _vm = unsafe { &mut *vm_ptr };

    if str_ptr == 0 {
        ctx.set_error("jit_rt_json_parse: null string".to_string());
        return NbValue::NAN_BOX_NULL as i64;
    }

    let js = unsafe { &*(str_ptr as *const JitString) };
    let input = unsafe { js.as_str() };
    match parse_json_optimized(input) {
        Ok(nb) => nb.to_bits() as i64,
        Err(_) => {
            ctx.set_error("jit_rt_json_parse: parse failed".to_string());
            NbValue::NAN_BOX_NULL as i64
        }
    }
}

/// Encode an NbValue to compact JSON, returning a JitString pointer.
#[no_mangle]
pub extern "C" fn jit_rt_json_encode(vm_ctx: *mut VmContext, value_ptr: i64) -> i64 {
    let ctx = unsafe { vm_ctx.as_mut() };
    let Some(ctx) = ctx else {
        return 0;
    };

    let vm_ptr = ctx.stack_pool as *mut VM;
    if vm_ptr.is_null() {
        ctx.set_error("jit_rt_json_encode: VM pointer missing".to_string());
        return 0;
    }
    let _vm = unsafe { &mut *vm_ptr };

    let nb = decode_nb_value(value_ptr);
    match encode_json_compact(nb) {
        Ok(encoded) => wrap_nb_value(NbValue::new_str(&encoded)),
        Err(e) => {
            ctx.set_error(format!("jit_rt_json_encode: {e}"));
            NbValue::NAN_BOX_NULL as i64
        }
    }
}

/// Encode an NbValue to pretty JSON, returning a JitString pointer.
#[no_mangle]
pub extern "C" fn jit_rt_json_pretty(vm_ctx: *mut VmContext, value_ptr: i64) -> i64 {
    let ctx = unsafe { vm_ctx.as_mut() };
    let Some(ctx) = ctx else {
        return 0;
    };

    let vm_ptr = ctx.stack_pool as *mut VM;
    if vm_ptr.is_null() {
        ctx.set_error("jit_rt_json_pretty: VM pointer missing".to_string());
        return 0;
    }
    let _vm = unsafe { &mut *vm_ptr };

    let nb = decode_nb_value(value_ptr);
    match encode_json_pretty(nb) {
        Ok(encoded) => wrap_nb_value(NbValue::new_str(&encoded)),
        Err(e) => {
            ctx.set_error(format!("jit_rt_json_pretty: {e}"));
            NbValue::NAN_BOX_NULL as i64
        }
    }
}
