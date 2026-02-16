//! Register VM dispatch loop for executing LIR bytecode.

mod helpers;
mod intrinsics;
mod ops;
pub(crate) mod processes;

use helpers::*;
pub(crate) use processes::{
    MachineExpr, MachineGraphDef, MachineParamDef, MachineRuntime, MachineStateDef, MemoryRuntime,
};

use crate::strings::StringTable;
use crate::types::{RuntimeField, RuntimeType, RuntimeTypeKind, RuntimeVariant, TypeTable};
use crate::values::{
    values_equal, ClosureValue, FutureStatus, FutureValue, RecordValue, StringRef, TraceRefValue,
    UnionValue, Value,
};
use lumen_compiler::compiler::lir::*;

use lumen_runtime::tools::{ProviderRegistry, ToolDispatcher, ToolRequest};
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;
use thiserror::Error;

/// Type alias for debug callback to simplify type signatures
pub type DebugCallback = Option<Box<dyn FnMut(&DebugEvent)>>;

/// Debug events emitted during VM execution.
/// Used for step-through debugging and execution tracing.
#[derive(Debug, Clone)]
pub enum DebugEvent {
    /// Instruction step: cell name, instruction pointer, opcode name
    Step {
        cell_name: String,
        ip: usize,
        opcode: String,
    },
    /// Call enter: cell name being called
    CallEnter { cell_name: String },
    /// Call exit: cell name returning, result value
    CallExit { cell_name: String, result: Value },
    /// Runtime tool call: cell, tool metadata, latency, and success status
    ToolCall {
        cell_name: String,
        tool_id: String,
        tool_version: String,
        latency_ms: u64,
        success: bool,
        message: Option<String>,
    },
    /// Runtime schema validation: cell, schema name, and verdict
    SchemaValidate {
        cell_name: String,
        schema: String,
        valid: bool,
    },
}

#[derive(Debug, Clone)]
pub struct StackFrame {
    pub cell_name: String,
    pub ip: usize,
}

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
    #[error("arithmetic overflow")]
    ArithmeticOverflow,
    #[error("division by zero")]
    DivisionByZero,
    #[error("instruction limit exceeded: {0}")]
    InstructionLimitExceeded(u64),
    #[error("register out of bounds: {0}")]
    RegisterOutOfBounds(usize),
    #[error("{message}\nStack trace (most recent call last):{stack_trace}")]
    WithStackTrace {
        message: String,
        stack_trace: String,
        frames: Vec<StackFrame>,
    },
}

impl VmError {
    /// Attach stack trace to error, returning a new WithStackTrace variant.
    /// If frames is empty, returns self unchanged.
    pub fn with_stack_trace(self, frames: Vec<StackFrame>) -> Self {
        if frames.is_empty() {
            return self;
        }
        // Don't double-wrap
        if matches!(self, VmError::WithStackTrace { .. }) {
            return self;
        }
        let message = format!("{}", self);
        let mut trace = String::new();
        for (i, frame) in frames.iter().rev().enumerate() {
            trace.push_str(&format!(
                "\n  #{}: {} (instruction {})",
                i, frame.cell_name, frame.ip
            ));
        }
        VmError::WithStackTrace {
            message,
            stack_trace: trace,
            frames,
        }
    }

    /// Format stack trace as a string (for external use without wrapping).
    pub fn format_stack_trace(frames: &[StackFrame]) -> String {
        let mut msg = String::from("\nStack trace (most recent call last):");
        for (i, frame) in frames.iter().rev().enumerate() {
            msg.push_str(&format!(
                "\n  #{}: {} (instruction {})",
                i, frame.cell_name, frame.ip
            ));
        }
        msg
    }

    /// Check if the error message contains a specific string (works through WithStackTrace wrapper).
    pub fn message_contains(&self, needle: &str) -> bool {
        match self {
            VmError::WithStackTrace { message, .. } => message.contains(needle),
            other => format!("{}", other).contains(needle),
        }
    }

    /// Check if the underlying error is a DivisionByZero (works through WithStackTrace wrapper).
    pub fn is_division_by_zero(&self) -> bool {
        match self {
            VmError::DivisionByZero => true,
            VmError::WithStackTrace { message, .. } => message == "division by zero",
            _ => false,
        }
    }

    /// Check if the underlying error is an ArithmeticOverflow (works through WithStackTrace wrapper).
    pub fn is_arithmetic_overflow(&self) -> bool {
        match self {
            VmError::ArithmeticOverflow => true,
            VmError::WithStackTrace { message, .. } => message == "arithmetic overflow",
            _ => false,
        }
    }

    /// Check if the underlying error is an InstructionLimitExceeded (works through WithStackTrace wrapper).
    pub fn is_instruction_limit_exceeded(&self) -> bool {
        match self {
            VmError::InstructionLimitExceeded(_) => true,
            VmError::WithStackTrace { message, .. } => {
                message.starts_with("instruction limit exceeded")
            }
            _ => false,
        }
    }

    /// Check if the underlying error is a RegisterOOB (works through WithStackTrace wrapper).
    pub fn is_register_oob(&self) -> bool {
        match self {
            VmError::RegisterOOB(_, _) => true,
            VmError::WithStackTrace { message, .. } => {
                message.starts_with("register out of bounds: r")
            }
            _ => false,
        }
    }

    /// Check if the underlying error is a TypeError (works through WithStackTrace wrapper).
    pub fn is_type_error(&self) -> bool {
        match self {
            VmError::TypeError(_) => true,
            VmError::WithStackTrace { message, .. } => {
                message.starts_with("type error at runtime")
            }
            _ => false,
        }
    }

    /// Check if the underlying error is a ToolError (works through WithStackTrace wrapper).
    pub fn is_tool_error(&self) -> bool {
        match self {
            VmError::ToolError(_) => true,
            VmError::WithStackTrace { message, .. } => message.starts_with("tool call error"),
            _ => false,
        }
    }

    /// Get the stack frames from a WithStackTrace error, or empty vec for other variants.
    pub fn stack_frames(&self) -> &[StackFrame] {
        match self {
            VmError::WithStackTrace { frames, .. } => frames,
            _ => &[],
        }
    }

    /// Create a runtime error with context about current location
    pub fn runtime_with_context(message: String, cell_name: &str, ip: usize) -> Self {
        VmError::Runtime(format!(
            "{} (in cell '{}' at instruction {})",
            message, cell_name, ip
        ))
    }

    /// Create a type error with context about the values involved
    pub fn type_error_with_values(operation: &str, expected: &str, actual: &Value) -> Self {
        VmError::TypeError(format!(
            "cannot {} with {} (expected {}, got {})",
            operation,
            actual.display_pretty(),
            expected,
            actual.type_name()
        ))
    }
}

const MAX_CALL_DEPTH: usize = 256;

/// Call frame on the VM stack.
#[derive(Debug, Clone)]
pub(crate) struct CallFrame {
    pub(crate) cell_idx: usize,
    pub(crate) base_register: usize,
    pub(crate) ip: usize,
    pub(crate) return_register: usize,
    pub(crate) future_id: Option<u64>,
}

#[derive(Debug, Clone)]
pub(crate) enum FutureState {
    Pending,
    Completed(Value),
    Error(String),
}

#[derive(Debug, Clone)]
pub(crate) struct FutureTask {
    pub(crate) future_id: u64,
    pub(crate) target: FutureTarget,
    pub(crate) args: Vec<Value>,
}

#[derive(Debug, Clone)]
pub(crate) enum FutureTarget {
    Cell(usize),
    Closure(ClosureValue),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FutureSchedule {
    Eager,
    DeferredFifo,
}


/// Scope for an installed effect handler.
#[derive(Debug, Clone)]
pub(crate) struct EffectScope {
    pub handler_ip: usize,
    pub frame_idx: usize,
    pub base_register: usize,
    pub cell_idx: usize,
    /// The effect name this handler matches (e.g. "Console")
    pub effect_name: String,
    /// The operation name this handler matches (e.g. "log")
    pub operation: String,
}

/// A suspended continuation for algebraic effects (one-shot).
#[derive(Debug, Clone)]
pub(crate) struct SuspendedContinuation {
    pub frames: Vec<CallFrame>,
    pub registers: Vec<Value>,
    pub resume_ip: usize,
    pub resume_frame_count: usize,
    pub result_reg: usize,
}

/// The Lumen register VM.
pub struct VM {
    pub strings: StringTable,
    pub types: TypeTable,
    pub(crate) registers: Vec<Value>,
    pub(crate) frames: Vec<CallFrame>,
    pub(crate) module: Option<LirModule>,
    /// Captured stdout output (for testing and tracing)
    pub output: Vec<String>,
    /// Optional tool dispatcher
    pub tool_dispatcher: Option<Box<dyn ToolDispatcher>>,
    /// Optional debug callback for step-through debugging
    pub debug_callback: DebugCallback,
    pub(crate) next_future_id: u64,
    pub(crate) future_states: BTreeMap<u64, FutureState>,
    pub(crate) scheduled_futures: VecDeque<FutureTask>,
    pub(crate) future_schedule: FutureSchedule,
    pub(crate) future_schedule_explicit: bool,
    pub(crate) next_process_instance_id: u64,
    pub(crate) process_kinds: BTreeMap<String, String>,
    pub(crate) pipeline_stages: BTreeMap<String, Vec<String>>,
    pub(crate) machine_graphs: BTreeMap<String, MachineGraphDef>,
    pub(crate) memory_runtime: BTreeMap<u64, MemoryRuntime>,
    pub(crate) machine_runtime: BTreeMap<u64, MachineRuntime>,
    pub(crate) process_configs: BTreeMap<String, BTreeMap<String, Value>>,
    pub(crate) await_fuel: u32,
    pub(crate) effect_handlers: Vec<EffectScope>,
    pub(crate) suspended_continuation: Option<SuspendedContinuation>,
    pub(crate) max_instructions: u64,
    pub(crate) instruction_count: u64,
    /// Optional fuel counter. Each instruction decrements fuel by 1.
    /// When fuel hits 0, execution stops with a "fuel exhausted" error.
    pub(crate) fuel: Option<u64>,
    pub(crate) trace_id: Option<String>,
    pub(crate) trace_seq: u64,
}

const MAX_AWAIT_RETRIES: u32 = 10_000;
const DEFAULT_MAX_INSTRUCTIONS: u64 = 10_000_000;

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
            debug_callback: None,
            next_future_id: 1,
            future_states: BTreeMap::new(),
            scheduled_futures: VecDeque::new(),
            future_schedule: FutureSchedule::Eager,
            future_schedule_explicit: false,
            next_process_instance_id: 1,
            process_kinds: BTreeMap::new(),
            pipeline_stages: BTreeMap::new(),
            machine_graphs: BTreeMap::new(),
            memory_runtime: BTreeMap::new(),
            machine_runtime: BTreeMap::new(),
            process_configs: BTreeMap::new(),
            await_fuel: MAX_AWAIT_RETRIES,
            effect_handlers: Vec::new(),
            suspended_continuation: None,
            max_instructions: DEFAULT_MAX_INSTRUCTIONS,
            instruction_count: 0,
            fuel: None,
            trace_id: None,
            trace_seq: 0,
        }
    }

    /// Set a provider registry as the tool dispatcher.
    ///
    /// `ProviderRegistry` implements `ToolDispatcher`, so this replaces any
    /// previously configured dispatcher.
    pub fn set_provider_registry(&mut self, registry: ProviderRegistry) {
        self.tool_dispatcher = Some(Box::new(registry));
    }

    pub(crate) fn validate_schema(&self, val: &Value, schema_name: &str) -> bool {
        match schema_name {
            "Int" | "int" => matches!(val, Value::Int(_)),
            "Float" | "float" => matches!(val, Value::Float(_)),
            "String" | "string" => matches!(val, Value::String(_)),
            "Bool" | "bool" => matches!(val, Value::Bool(_)),
            "List" | "list" => matches!(val, Value::List(_)),
            "Map" | "map" => matches!(val, Value::Map(_)),
            "Tuple" | "tuple" => matches!(val, Value::Tuple(_)),
            "Set" | "set" => matches!(val, Value::Set(_)),
            "Any" | "any" => true,
            "Null" | "null" => matches!(val, Value::Null),
            _ => match val {
                Value::Record(r) => r.type_name == schema_name,
                _ => false,
            },
        }
    }

    pub(crate) fn extract_pattern_captures(
        &self,
        pattern: &str,
        input: &str,
    ) -> Option<BTreeMap<String, Value>> {
        let mut captures = BTreeMap::new();
        let mut current_input = input;
        let mut current_pattern = pattern;

        while let Some(placeholder_start) = current_pattern.find('{') {
            let prefix = &current_pattern[..placeholder_start];
            if !current_input.starts_with(prefix) {
                return None;
            }
            current_input = &current_input[prefix.len()..];
            current_pattern = &current_pattern[placeholder_start + 1..];

            if let Some(placeholder_end) = current_pattern.find('}') {
                let key = &current_pattern[..placeholder_end];
                current_pattern = &current_pattern[placeholder_end + 1..];

                if let Some(next_prefix_start) = current_pattern.find('{') {
                    let next_prefix = &current_pattern[..next_prefix_start];
                    if let Some(match_pos) = current_input.find(next_prefix) {
                        let val = &current_input[..match_pos];
                        captures.insert(key.to_string(), Value::String(StringRef::Owned(val.to_string())));
                        current_input = &current_input[match_pos..];
                    } else {
                        return None;
                    }
                } else {
                    if current_pattern.is_empty() {
                        captures.insert(key.to_string(), Value::String(StringRef::Owned(current_input.to_string())));
                        current_input = "";
                    } else if current_input.ends_with(current_pattern) {
                        let val = &current_input[..current_input.len() - current_pattern.len()];
                        captures.insert(key.to_string(), Value::String(StringRef::Owned(val.to_string())));
                        current_input = "";
                        current_pattern = "";
                    } else {
                        return None;
                    }
                }
            } else {
                return None;
            }
        }

        if current_input == current_pattern {
            Some(captures)
        } else {
            None
        }
    }

    fn emit_debug_event(&mut self, event: DebugEvent) {
        if let Some(ref mut cb) = self.debug_callback {
            cb(&event);
        }
    }

    /// Load a LIR module into the VM.
    pub fn load(&mut self, module: LirModule) {
        // Intern all strings
        for s in &module.strings {
            self.strings.intern(s);
        }
        if !self.future_schedule_explicit {
            self.future_schedule = future_schedule_from_addons(&module.addons);
        }
        self.next_process_instance_id = 1;
        self.process_kinds.clear();
        self.pipeline_stages.clear();
        self.next_future_id = 1;
        self.future_states.clear();
        self.scheduled_futures.clear();
        self.memory_runtime.clear();
        self.machine_runtime.clear();
        self.process_configs.clear();
        self.machine_graphs.clear();
        self.await_fuel = MAX_AWAIT_RETRIES;
        self.effect_handlers.clear();
        self.suspended_continuation = None;
        self.instruction_count = 0;
        let mut machine_initials: BTreeMap<String, String> = BTreeMap::new();
        for addon in &module.addons {
            if let Some(name) = &addon.name {
                if matches!(
                    addon.kind.as_str(),
                    "pipeline"
                        | "orchestration"
                        | "machine"
                        | "memory"
                        | "guardrail"
                        | "eval"
                        | "pattern"
                ) {
                    self.process_kinds.insert(name.clone(), addon.kind.clone());
                }
                if addon.kind == "machine.initial" {
                    if let Some((machine, initial)) = name.split_once('=') {
                        machine_initials.insert(machine.to_string(), initial.to_string());
                    }
                }
                if addon.kind == "pipeline.stages" {
                    if let Some((pipeline_name, stages_json)) = name.split_once('=') {
                        if let Ok(stages) =
                            serde_json::from_str::<Vec<String>>(stages_json)
                        {
                            self.pipeline_stages
                                .insert(pipeline_name.to_string(), stages);
                        }
                    }
                }
                if addon.kind == "process.config" {
                    if let Some(ref name) = addon.name {
                        if let Some((target, payload)) = name.split_once('=') {
                            if let Some((process_name, config_key)) = target.split_once('.') {
                                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(payload) {
                                    let val = helpers::json_to_value(&json_val);
                                    self.process_configs
                                        .entry(process_name.to_string())
                                        .or_default()
                                        .insert(config_key.to_string(), val);
                                }
                            }
                        }
                    }
                }
                if addon.kind == "machine.state" {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(name) {
                        let machine = v
                            .get("machine")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let state = v
                            .get("state")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        if machine.is_empty() || state.is_empty() {
                            continue;
                        }
                        let terminal = v.get("terminal").and_then(|v| v.as_bool()).unwrap_or(false);
                        let transition_to = v
                            .get("transition_to")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let params = v
                            .get("params")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|p| {
                                        Some(MachineParamDef {
                                            name: p.get("name")?.as_str()?.to_string(),
                                            ty: p.get("type")?.as_str()?.to_string(),
                                        })
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        let guard = v.get("guard").and_then(parse_machine_expr_json);
                        let transition_args = v
                            .get("transition_args")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(parse_machine_expr_json)
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        self.machine_graphs
                            .entry(machine)
                            .or_default()
                            .states
                            .insert(
                                state,
                                MachineStateDef {
                                    params,
                                    terminal,
                                    guard,
                                    transition_to,
                                    transition_args,
                                },
                            );
                    }
                }
            }
        }
        for (machine, initial) in machine_initials {
            self.machine_graphs.entry(machine).or_default().initial = initial;
        }
        for graph in self.machine_graphs.values_mut() {
            if graph.initial.is_empty() {
                if let Some(first_state) = graph.states.keys().next() {
                    graph.initial = first_state.clone();
                }
            }
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

    pub fn set_future_schedule(&mut self, schedule: FutureSchedule) {
        self.future_schedule = schedule;
        self.future_schedule_explicit = true;
    }

    pub fn set_trace_id<S: Into<String>>(&mut self, trace_id: S) {
        self.trace_id = Some(trace_id.into());
        self.trace_seq = 0;
    }

    pub fn future_schedule(&self) -> FutureSchedule {
        self.future_schedule
    }

    pub fn set_instruction_limit(&mut self, max_instructions: u64) {
        self.max_instructions = max_instructions;
    }

    /// Set the fuel counter. Each executed instruction consumes one unit of fuel.
    /// When fuel reaches 0, execution stops with a "fuel exhausted" error.
    pub fn set_fuel(&mut self, fuel: u64) {
        self.fuel = Some(fuel);
    }

    /// Capture the current call stack for error reporting.
    pub fn capture_stack_trace(&self) -> Vec<StackFrame> {
        let module = match &self.module {
            Some(m) => m,
            None => return vec![],
        };

        self.frames
            .iter()
            .map(|frame| {
                let cell_name = if frame.cell_idx < module.cells.len() {
                    module.cells[frame.cell_idx].name.clone()
                } else {
                    format!("<unknown-cell-{}>", frame.cell_idx)
                };
                StackFrame {
                    cell_name,
                    ip: frame.ip,
                }
            })
            .collect()
    }

    /// Checked register access (read-only).
    #[inline]
    #[allow(dead_code)]
    fn reg(&self, index: usize) -> Result<&Value, VmError> {
        self.registers
            .get(index)
            .ok_or(VmError::RegisterOutOfBounds(index))
    }

    /// Checked register access (mutable).
    #[inline]
    #[allow(dead_code)]
    fn reg_mut(&mut self, index: usize) -> Result<&mut Value, VmError> {
        self.registers
            .get_mut(index)
            .ok_or(VmError::RegisterOutOfBounds(index))
    }

    #[inline]
    fn check_register(&self, reg: usize, cell_registers: u8) -> Result<(), VmError> {
        if reg < cell_registers as usize {
            Ok(())
        } else {
            let offending = reg.min(u8::MAX as usize) as u8;
            Err(VmError::RegisterOOB(offending, cell_registers))
        }
    }

    #[inline]
    fn check_register_span(
        &self,
        start: usize,
        len: usize,
        cell_registers: u8,
    ) -> Result<(), VmError> {
        if len == 0 {
            return Ok(());
        }
        let end = start.saturating_add(len - 1);
        self.check_register(end, cell_registers)
    }

    /// Copy call arguments into parameter registers, packing trailing args
    /// into a list for the variadic parameter (if any).
    fn copy_args_to_params(
        &mut self,
        params: &[LirParam],
        new_base: usize,
        arg_base: usize,
        nargs: usize,
        param_offset: usize,
        cell_registers: u8,
    ) -> Result<(), VmError> {
        // If param_offset exceeds params length, there are no params to fill
        if param_offset >= params.len() {
            return Ok(());
        }
        // Find the variadic parameter index (if any)
        let variadic_idx = params[param_offset..]
            .iter()
            .position(|p| p.variadic)
            .map(|i| i + param_offset);

        if let Some(vi) = variadic_idx {
            // Copy fixed params before the variadic one
            let fixed_count = vi - param_offset;
            for i in 0..fixed_count.min(nargs) {
                let dst = params[param_offset + i].register as usize;
                self.check_register(dst, cell_registers)?;
                self.registers[new_base + dst] = self.registers[arg_base + i].clone();
            }
            // Pack remaining args into a list for the variadic param
            let variadic_args: Vec<Value> = (fixed_count..nargs)
                .map(|i| self.registers[arg_base + i].clone())
                .collect();
            let dst = params[vi].register as usize;
            self.check_register(dst, cell_registers)?;
            self.registers[new_base + dst] = Value::new_list(variadic_args);
        } else {
            // No variadic param — copy args 1:1 as before
            for i in 0..nargs {
                if param_offset + i < params.len() {
                    let dst = params[param_offset + i].register as usize;
                    self.check_register(dst, cell_registers)?;
                    self.registers[new_base + dst] = self.registers[arg_base + i].clone();
                }
            }
        }
        Ok(())
    }

    fn validate_instruction_registers(
        &self,
        instr: Instruction,
        cell_registers: u8,
    ) -> Result<(), VmError> {
        let a = instr.a as usize;
        let b = instr.b as usize;
        let c = instr.c as usize;

        match instr.op {
            OpCode::Nop | OpCode::Jmp | OpCode::Break | OpCode::Continue => Ok(()),

            OpCode::LoadK
            | OpCode::LoadBool
            | OpCode::LoadInt
            | OpCode::NewRecord
            | OpCode::Test
            | OpCode::Return
            | OpCode::Halt
            | OpCode::Loop
            | OpCode::Closure
            | OpCode::Schema
            | OpCode::Emit
            | OpCode::TraceRef
            | OpCode::Spawn
            | OpCode::IsVariant => self.check_register(a, cell_registers),

            OpCode::LoadNil => self.check_register_span(a, b + 1, cell_registers),

            OpCode::Move
            | OpCode::Neg
            | OpCode::BitNot
            | OpCode::Not
            | OpCode::Append
            | OpCode::Unbox => {
                self.check_register(a, cell_registers)?;
                self.check_register(b, cell_registers)
            }

            OpCode::NewList | OpCode::NewTuple | OpCode::NewSet => {
                self.check_register(a, cell_registers)?;
                self.check_register_span(a + 1, b, cell_registers)
            }

            OpCode::NewMap => {
                self.check_register(a, cell_registers)?;
                self.check_register_span(a + 1, b.saturating_mul(2), cell_registers)
            }

            OpCode::GetField
            | OpCode::GetIndex
            | OpCode::GetTuple
            | OpCode::Add
            | OpCode::Sub
            | OpCode::Mul
            | OpCode::Div
            | OpCode::FloorDiv
            | OpCode::Mod
            | OpCode::Pow
            | OpCode::Concat
            | OpCode::BitOr
            | OpCode::BitAnd
            | OpCode::BitXor
            | OpCode::Shl
            | OpCode::Shr
            | OpCode::Eq
            | OpCode::Lt
            | OpCode::Le
            | OpCode::And
            | OpCode::Or
            | OpCode::In
            | OpCode::Is
            | OpCode::NullCo
            | OpCode::SetIndex
            | OpCode::NewUnion
            | OpCode::Await => {
                self.check_register(a, cell_registers)?;
                self.check_register(b, cell_registers)?;
                self.check_register(c, cell_registers)
            }

            OpCode::SetField | OpCode::SetUpval => {
                self.check_register(a, cell_registers)?;
                self.check_register(c, cell_registers)
            }

            OpCode::ForPrep => self.check_register_span(a, 3, cell_registers),

            OpCode::ForLoop => self.check_register_span(a, 4, cell_registers),

            OpCode::ForIn => {
                self.check_register(a, cell_registers)?;
                self.check_register(a + 1, cell_registers)?;
                self.check_register(b, cell_registers)?;
                self.check_register(c, cell_registers)
            }

            OpCode::Call | OpCode::TailCall => {
                self.check_register(a, cell_registers)?;
                self.check_register_span(a + 1, b, cell_registers)
            }

            OpCode::Intrinsic => {
                self.check_register(a, cell_registers)?;
                self.check_register(c, cell_registers)
            }

            OpCode::GetUpval => self.check_register(a, cell_registers),

            OpCode::ToolCall => self.check_register(a, cell_registers),

            OpCode::Perform => {
                self.check_register(a, cell_registers)
            }
            OpCode::HandlePush | OpCode::HandlePop => Ok(()),
            OpCode::Resume => self.check_register(a, cell_registers),
        }
    }

    fn ensure_process_instance(&mut self, value: &mut Value) {
        let Value::Record(ref mut r) = value else {
            return;
        };
        if !self.process_kinds.contains_key(&r.type_name) {
            return;
        }
        if let Some(Value::Int(_)) = r.fields.get("__instance_id") {
            return;
        }
        let id = self.next_process_instance_id;
        self.next_process_instance_id += 1;
        let r_mut = Rc::make_mut(r);
        r_mut.fields
            .insert("__instance_id".to_string(), Value::Int(id as i64));
        r_mut.fields.insert(
            "__process_name".to_string(),
            Value::String(StringRef::Owned(r_mut.type_name.clone())),
        );
    }

    fn current_future_id(&self) -> Option<u64> {
        self.frames.last().and_then(|f| f.future_id)
    }

    fn fail_current_future(&mut self, message: String) -> bool {
        let Some(fid) = self.current_future_id() else {
            return false;
        };
        let _ = self.frames.pop();
        self.future_states.insert(fid, FutureState::Error(message));
        true
    }

    /// Check truthiness with interned string resolution.
    fn value_is_truthy(&self, val: &Value) -> bool {
        match val {
            Value::String(StringRef::Interned(id)) => {
                match self.strings.resolve(*id) {
                    Some(s) => !s.is_empty(),
                    None => true, // unknown interned string, assume truthy
                }
            }
            other => other.is_truthy(),
        }
    }

    fn start_future_task(&mut self, task: FutureTask) -> Result<(), VmError> {
        if self.frames.len() >= MAX_CALL_DEPTH {
            return Err(VmError::StackOverflow(MAX_CALL_DEPTH));
        }
        let module = self.module.as_ref().ok_or(VmError::NoModule)?;
        match task.target {
            FutureTarget::Cell(cell_idx) => {
                if cell_idx >= module.cells.len() {
                    self.future_states.insert(
                        task.future_id,
                        FutureState::Error(format!(
                            "spawn target cell index {} not found",
                            cell_idx
                        )),
                    );
                    return Ok(());
                }
                let callee_cell = &module.cells[cell_idx];
                let num_regs = (callee_cell.registers as usize).max(256);
                let params = callee_cell.params.clone();
                let new_base = self.registers.len();
                self.registers.resize(new_base + num_regs, Value::Null);
                for (i, arg) in task.args.into_iter().enumerate() {
                    if i < params.len() {
                        let dst = params[i].register as usize;
                        self.check_register(dst, callee_cell.registers)?;
                        self.registers[new_base + dst] = arg;
                    }
                }
                self.frames.push(CallFrame {
                    cell_idx,
                    base_register: new_base,
                    ip: 0,
                    return_register: 0,
                    future_id: Some(task.future_id),
                });
            }
            FutureTarget::Closure(cv) => {
                if cv.cell_idx >= module.cells.len() {
                    self.future_states.insert(
                        task.future_id,
                        FutureState::Error(format!(
                            "spawn target closure cell index {} not found",
                            cv.cell_idx
                        )),
                    );
                    return Ok(());
                }
                let callee_cell = &module.cells[cv.cell_idx];
                let num_regs = (callee_cell.registers as usize).max(256);
                let params = callee_cell.params.clone();
                let new_base = self.registers.len();
                self.registers.resize(new_base + num_regs, Value::Null);
                for (i, cap) in cv.captures.iter().enumerate() {
                    self.check_register(i, callee_cell.registers)?;
                    self.registers[new_base + i] = cap.clone();
                }
                let cap_count = cv.captures.len();
                for (i, arg) in task.args.into_iter().enumerate() {
                    if cap_count + i < params.len() {
                        let dst = params[cap_count + i].register as usize;
                        self.check_register(dst, callee_cell.registers)?;
                        self.registers[new_base + dst] = arg;
                    }
                }
                self.frames.push(CallFrame {
                    cell_idx: cv.cell_idx,
                    base_register: new_base,
                    ip: 0,
                    return_register: 0,
                    future_id: Some(task.future_id),
                });
            }
        }
        Ok(())
    }

    fn schedule_future_task(&mut self, task: FutureTask) -> Result<(), VmError> {
        match self.future_schedule {
            FutureSchedule::Eager => self.start_future_task(task),
            FutureSchedule::DeferredFifo => {
                self.scheduled_futures.push_back(task);
                Ok(())
            }
        }
    }

    fn spawn_future(&mut self, target: FutureTarget, args: Vec<Value>) -> Result<Value, VmError> {
        let future_id = self.next_future_id;
        self.next_future_id += 1;
        if let Some(module) = self.module.as_ref() {
            let invalid_target = match &target {
                FutureTarget::Cell(idx) => *idx >= module.cells.len(),
                FutureTarget::Closure(cv) => cv.cell_idx >= module.cells.len(),
            };
            if invalid_target {
                let msg = match &target {
                    FutureTarget::Cell(idx) => format!("spawn target cell index {} not found", idx),
                    FutureTarget::Closure(cv) => {
                        format!("spawn target closure cell index {} not found", cv.cell_idx)
                    }
                };
                self.future_states
                    .insert(future_id, FutureState::Error(msg.clone()));
                return Ok(Value::Future(FutureValue {
                    id: future_id,
                    state: FutureStatus::Error,
                }));
            }
        }
        self.future_states.insert(future_id, FutureState::Pending);
        let task = FutureTask {
            future_id,
            target,
            args,
        };
        self.schedule_future_task(task)?;
        Ok(Value::Future(FutureValue {
            id: future_id,
            state: FutureStatus::Pending,
        }))
    }

    fn start_scheduled_future(&mut self, id: u64) -> Result<bool, VmError> {
        let Some(pos) = self
            .scheduled_futures
            .iter()
            .position(|task| task.future_id == id)
        else {
            return Ok(false);
        };
        let task = self
            .scheduled_futures
            .remove(pos)
            .ok_or_else(|| VmError::Runtime("scheduled future queue corruption".to_string()))?;
        self.start_future_task(task)?;
        Ok(true)
    }

    fn await_future_value(&mut self, future: &FutureValue) -> Result<Option<Value>, VmError> {
        match self.future_states.get(&future.id).cloned() {
            Some(FutureState::Completed(val)) => Ok(Some(val)),
            Some(FutureState::Error(msg)) => Err(VmError::Runtime(format!(
                "await failed for future {}: {}",
                future.id, msg
            ))),
            Some(FutureState::Pending) => {
                let has_task = self
                    .scheduled_futures
                    .iter()
                    .any(|task| task.future_id == future.id);
                if has_task {
                    let _ = self.start_scheduled_future(future.id)?;
                    Ok(None)
                } else {
                    Err(VmError::Runtime(format!(
                        "future {} is pending with no runnable task",
                        future.id
                    )))
                }
            }
            None => Err(VmError::Runtime(format!("unknown future id {}", future.id))),
        }
    }

    fn await_value_recursive(&mut self, value: Value) -> Result<Option<Value>, VmError> {
        match value {
            Value::Future(f) => self.await_future_value(&f),
            Value::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items.iter().cloned() {
                    match self.await_value_recursive(item)? {
                        Some(v) => out.push(v),
                        None => return Ok(None),
                    }
                }
                Ok(Some(Value::new_list(out)))
            }
            Value::Tuple(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items.iter().cloned() {
                    match self.await_value_recursive(item)? {
                        Some(v) => out.push(v),
                        None => return Ok(None),
                    }
                }
                Ok(Some(Value::new_tuple(out)))
            }
            Value::Set(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items.iter().cloned() {
                    match self.await_value_recursive(item)? {
                        Some(v) => out.push(v),
                        None => return Ok(None),
                    }
                }
                Ok(Some(Value::new_set_from_vec(out)))
            }
            Value::Map(entries) => {
                let mut out = BTreeMap::new();
                for (k, v) in entries.iter() {
                    match self.await_value_recursive(v.clone())? {
                        Some(resolved) => {
                            out.insert(k.clone(), resolved);
                        }
                        None => return Ok(None),
                    }
                }
                Ok(Some(Value::new_map(out)))
            }
            Value::Record(mut record) => {
                let mut out = BTreeMap::new();
                for (k, v) in std::mem::take(&mut Rc::make_mut(&mut record).fields) {
                    match self.await_value_recursive(v)? {
                        Some(resolved) => {
                            out.insert(k, resolved);
                        }
                        None => return Ok(None),
                    }
                }
                Rc::make_mut(&mut record).fields = out;
                Ok(Some(Value::Record(record)))
            }
            other => Ok(Some(other)),
        }
    }

    fn resolve_trace_id(&self) -> String {
        if let Some(trace_id) = self.trace_id.as_ref() {
            return trace_id.clone();
        }
        self.module
            .as_ref()
            .map(|module| format!("doc:{}", module.doc_hash))
            .unwrap_or_else(|| "trace:unbound".to_string())
    }

    fn next_trace_ref(&mut self) -> TraceRefValue {
        self.trace_seq = self.trace_seq.saturating_add(1);
        TraceRefValue {
            trace_id: self.resolve_trace_id(),
            seq: self.trace_seq,
        }
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
                let dst = cell.params[i].register as usize;
                self.check_register(dst, cell.registers)?;
                self.registers[base + dst] = arg;
            }
        }

        // Push initial frame
        self.instruction_count = 0;
        self.trace_seq = 0;
        self.frames.push(CallFrame {
            cell_idx,
            base_register: base,
            ip: 0,
            return_register: 0,
            future_id: None,
        });

        // Execute
        self.run_until(0).map_err(|err| {
            let frames = self.capture_stack_trace();
            err.with_stack_trace(frames)
        })
    }

    /// Helper to get a constant from the current cell.
    #[allow(dead_code)]
    fn get_constant(&self, cell_idx: usize, idx: usize) -> Result<Constant, VmError> {
        let module = self.module.as_ref().ok_or(VmError::NoModule)?;
        let cell = module
            .cells
            .get(cell_idx)
            .ok_or_else(|| VmError::Runtime(format!("cell index {} out of bounds", cell_idx)))?;
        cell.constants
            .get(idx)
            .cloned()
            .ok_or_else(|| VmError::Runtime(format!("constant index {} out of bounds", idx)))
    }

    /// Helper to get a string from the module string table.
    #[allow(dead_code)]
    fn get_module_string(&self, idx: usize) -> Result<String, VmError> {
        let module = self.module.as_ref().ok_or(VmError::NoModule)?;
        if idx < module.strings.len() {
            Ok(module.strings[idx].clone())
        } else {
            Ok(String::new())
        }
    }

    pub(crate) fn run_until(&mut self, limit: usize) -> Result<Value, VmError> {
        loop {
            if self.frames.len() <= limit {
                return Ok(Value::Null);
            }

            let (cell_idx, base, instr, cell_registers) = {
                let frame = match self.frames.last() {
                    Some(f) => f,
                    None => return Ok(Value::Null),
                };
                let cell_idx = frame.cell_idx;
                let base = frame.base_register;
                let ip = frame.ip;

                let module = self.module.as_ref().ok_or(VmError::NoModule)?;
                let cell = module.cells.get(cell_idx).ok_or_else(|| {
                    VmError::Runtime(format!("cell index {} out of bounds", cell_idx))
                })?;

                if ip >= cell.instructions.len() {
                    self.frames.pop();
                    if self.frames.is_empty() {
                        return Ok(Value::Null);
                    }
                    continue;
                }

                let instr = cell.instructions[ip];
                (cell_idx, base, instr, cell.registers)
            };

            self.instruction_count = self.instruction_count.saturating_add(1);
            if self.instruction_count > self.max_instructions {
                return Err(VmError::InstructionLimitExceeded(self.max_instructions));
            }

            if let Some(ref mut fuel) = self.fuel {
                if *fuel == 0 {
                    return Err(VmError::Runtime("fuel exhausted".into()));
                }
                *fuel -= 1;
            }

            let module = self.module.as_ref().ok_or(VmError::NoModule)?;
            let cell_name = module.cells[cell_idx].name.clone();
            self.emit_debug_event(DebugEvent::Step {
                cell_name,
                ip: self.frames.last().map(|f| f.ip).unwrap_or(0),
                opcode: format!("{:?}", instr.op),
            });

            // Advance IP in the frame
            if let Some(f) = self.frames.last_mut() {
                f.ip += 1;
            }

            let a = instr.a as usize;
            let b = instr.b as usize;
            let c = instr.c as usize;
            self.validate_instruction_registers(instr, cell_registers)?;

            // Handle opcodes that need mutable self first (before borrowing module).
            if matches!(
                instr.op,
                OpCode::Call | OpCode::TailCall | OpCode::Intrinsic
            ) {
                match instr.op {
                    OpCode::Call => {
                        if let Err(err) = self.dispatch_call(base, a, b) {
                            if self.fail_current_future(err.to_string()) {
                                continue;
                            }
                            return Err(err);
                        }
                        continue;
                    }
                    OpCode::TailCall => {
                        if let Err(err) = self.dispatch_tailcall(base, a, b) {
                            if self.fail_current_future(err.to_string()) {
                                continue;
                            }
                            return Err(err);
                        }
                        continue;
                    }
                    OpCode::Intrinsic => {
                        let result = match self.exec_intrinsic(base, a, b, c) {
                            Ok(v) => v,
                            Err(err) => {
                                if self.fail_current_future(err.to_string()) {
                                    continue;
                                }
                                return Err(err);
                            }
                        };
                        self.registers[base + a] = result;
                        continue;
                    }
                    _ => unreachable!("guarded by matches! above"),
                }
            }

            let module = self.module.as_ref().ok_or(VmError::NoModule)?;
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
                    self.registers[base + a] = Value::new_list(list);
                }
                OpCode::NewMap => {
                    let mut map = BTreeMap::new();
                    for i in 0..b {
                        let k = self.registers[base + a + 1 + i * 2].as_string();
                        let v = self.registers[base + a + 2 + i * 2].clone();
                        map.insert(k, v);
                    }
                    self.registers[base + a] = Value::new_map(map);
                }
                OpCode::NewRecord => {
                    let bx = instr.bx() as usize;
                    let type_name = if bx < module.strings.len() {
                        module.strings[bx].clone()
                    } else {
                        "Unknown".to_string()
                    };
                    let fields = BTreeMap::new();
                    self.registers[base + a] = Value::new_record(RecordValue { type_name, fields });
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
                    self.registers[base + a] = Value::new_tuple(elems);
                }
                OpCode::NewSet => {
                    let mut elems = Vec::with_capacity(b);
                    for i in 1..=b {
                        let v = self.registers[base + a + i].clone();
                        if !elems.contains(&v) {
                            elems.push(v);
                        }
                    }
                    self.registers[base + a] = Value::new_set_from_vec(elems);
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
                        Rc::make_mut(r).fields.insert(field_name, val);
                    }
                }
                OpCode::GetIndex => {
                    let obj = &self.registers[base + b];
                    let idx = &self.registers[base + c];
                    let val = match (obj, idx) {
                        (Value::List(l), Value::Int(i)) => {
                            let ii = *i;
                            let len = l.len() as i64;
                            let effective = if ii < 0 { ii + len } else { ii };
                            if effective < 0 || effective >= len {
                                return Err(VmError::Runtime(format!(
                                    "index {} out of bounds for list of length {}",
                                    ii, len
                                )));
                            }
                            l[effective as usize].clone()
                        }
                        (Value::Tuple(t), Value::Int(i)) => {
                            let ii = *i;
                            let len = t.len() as i64;
                            let effective = if ii < 0 { ii + len } else { ii };
                            if effective < 0 || effective >= len {
                                return Err(VmError::Runtime(format!(
                                    "index {} out of bounds for tuple of length {}",
                                    ii, len
                                )));
                            }
                            t[effective as usize].clone()
                        }
                        (Value::Map(m), _) => {
                            m.get(&idx.as_string()).cloned().unwrap_or(Value::Null)
                        }
                        (Value::Record(r), _) => r
                            .fields
                            .get(&idx.as_string())
                            .cloned()
                            .unwrap_or(Value::Null),
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
                                let len = l.len() as i64;
                                let effective = if i < 0 { i + len } else { i };
                                if effective < 0 || effective >= len {
                                    return Err(VmError::Runtime(format!(
                                        "index {} out of bounds for list of length {}",
                                        i, len
                                    )));
                                }
                                Rc::make_mut(l)[effective as usize] = val;
                            } else {
                                return Err(VmError::TypeError(format!(
                                    "list index must be an integer, got {}",
                                    key.type_name()
                                )));
                            }
                        }
                        Value::Map(m) => {
                            Rc::make_mut(m).insert(key.as_string(), val);
                        }
                        Value::Record(r) => {
                            Rc::make_mut(r).fields.insert(key.as_string(), val);
                        }
                        target => {
                            return Err(VmError::TypeError(format!(
                                "cannot assign by index on {} (expected list, map, or record)",
                                target.type_name()
                            )));
                        }
                    }
                }
                OpCode::GetTuple => {
                    let obj = &self.registers[base + b];
                    let val = match obj {
                        Value::Tuple(t) => {
                            if c >= t.len() {
                                return Err(VmError::Runtime(format!(
                                    "index {} out of bounds for tuple of length {}",
                                    c,
                                    t.len()
                                )));
                            }
                            t[c].clone()
                        }
                        Value::List(l) => {
                            if c >= l.len() {
                                return Err(VmError::Runtime(format!(
                                    "index {} out of bounds for list of length {}",
                                    c,
                                    l.len()
                                )));
                            }
                            l[c].clone()
                        }
                        _ => Value::Null,
                    };
                    self.registers[base + a] = val;
                }

                // Arithmetic
                OpCode::Add => {
                    let lhs = &self.registers[base + b];
                    let rhs = &self.registers[base + c];
                    let result = match (lhs, rhs) {
                        (Value::Int(x), Value::Int(y)) => {
                            Value::Int(x.checked_add(*y).ok_or(VmError::ArithmeticOverflow)?)
                        }
                        (Value::Float(a), Value::Float(b)) => Value::Float(a + b),
                        (Value::Int(a), Value::Float(b)) => Value::Float(*a as f64 + b),
                        (Value::Float(a), Value::Int(b)) => Value::Float(a + *b as f64),
                        (Value::String(_), _) | (_, Value::String(_)) => Value::String(
                            StringRef::Owned(format!("{}{}", lhs.as_string(), rhs.as_string())),
                        ),
                        _ => {
                            let lhs_clone = lhs.clone();
                            let rhs_clone = rhs.clone();
                            return Err(VmError::TypeError(format!(
                                "cannot add {} ({}) to {} ({})",
                                lhs_clone.display_pretty(),
                                lhs_clone.type_name(),
                                rhs_clone.display_pretty(),
                                rhs_clone.type_name()
                            )));
                        }
                    };
                    self.registers[base + a] = result;
                }
                OpCode::Sub => {
                    self.arith_op(base, a, b, c, |x, y| x.checked_sub(y), |x, y| x - y)?;
                }
                OpCode::Mul => {
                    self.arith_op(base, a, b, c, |x, y| x.checked_mul(y), |x, y| x * y)?;
                }
                OpCode::Div => {
                    // Pre-check for integer division by zero
                    if matches!(
                        (&self.registers[base + b], &self.registers[base + c]),
                        (Value::Int(_), Value::Int(0))
                    ) {
                        return Err(VmError::DivisionByZero);
                    }
                    self.arith_op(base, a, b, c, |x, y| x.checked_div(y), |x, y| x / y)?;
                }
                OpCode::FloorDiv => {
                    // Floor division: integer division for ints, floor(a/b) for floats
                    let is_zero = matches!(
                        (&self.registers[base + b], &self.registers[base + c]),
                        (Value::Int(_), Value::Int(0))
                    ) || matches!(
                        &self.registers[base + c],
                        Value::Float(f) if *f == 0.0
                    );
                    if is_zero {
                        return Err(VmError::DivisionByZero);
                    }
                    let result = match (&self.registers[base + b], &self.registers[base + c]) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x.div_euclid(*y)),
                        (Value::Float(x), Value::Float(y)) => Value::Float((*x / *y).floor()),
                        (Value::Int(x), Value::Float(y)) => Value::Float((*x as f64 / *y).floor()),
                        (Value::Float(x), Value::Int(y)) => Value::Float((*x / *y as f64).floor()),
                        _ => Value::Null,
                    };
                    self.registers[base + a] = result;
                }
                OpCode::Mod => {
                    // Pre-check for integer modulo by zero
                    if matches!(
                        (&self.registers[base + b], &self.registers[base + c]),
                        (Value::Int(_), Value::Int(0))
                    ) {
                        return Err(VmError::DivisionByZero);
                    }
                    self.arith_op(base, a, b, c, |x, y| x.checked_rem(y), |x, y| x % y)?;
                }
                OpCode::Pow => {
                    let lhs = &self.registers[base + b];
                    let rhs = &self.registers[base + c];
                    self.registers[base + a] =
                        match (lhs, rhs) {
                            (Value::Int(x), Value::Int(y)) => {
                                if *y < 0 {
                                    Value::Float((*x as f64).powf(*y as f64))
                                } else if *y >= 64 {
                                    return Err(VmError::Runtime(
                                        "exponent out of range (must be 0..63 for integers)".into(),
                                    ));
                                } else {
                                    Value::Int(x.checked_pow(*y as u32).ok_or_else(|| {
                                        VmError::Runtime("integer overflow".into())
                                    })?)
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
                            let mut combined: Vec<Value> = (**a).clone();
                            combined.extend(b.iter().cloned());
                            Value::new_list(combined)
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
                        (Value::Int(x), Value::Int(y)) => {
                            if *y < 0 || *y > 63 {
                                return Err(VmError::Runtime(
                                    "shift amount out of range (must be 0..63)".into(),
                                ));
                            }
                            Value::Int(x << (*y as u32))
                        }
                        _ => return Err(VmError::TypeError("shift left requires integers".into())),
                    };
                }
                OpCode::Shr => {
                    let lhs = &self.registers[base + b];
                    let rhs = &self.registers[base + c];
                    self.registers[base + a] = match (lhs, rhs) {
                        (Value::Int(x), Value::Int(y)) => {
                            if *y < 0 || *y > 63 {
                                return Err(VmError::Runtime(
                                    "shift amount out of range (must be 0..63)".into(),
                                ));
                            }
                            Value::Int(x >> (*y as u32))
                        }
                        _ => {
                            return Err(VmError::TypeError("shift right requires integers".into()))
                        }
                    };
                }

                // Comparison / logic
                OpCode::Eq => {
                    let lhs = &self.registers[base + b];
                    let rhs = &self.registers[base + c];
                    let eq = values_equal(lhs, rhs, &self.strings);
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
                    let truthy = self.value_is_truthy(&self.registers[base + b]);
                    self.registers[base + a] = Value::Bool(!truthy);
                }
                OpCode::And => {
                    let lt = self.value_is_truthy(&self.registers[base + b]);
                    let rt = self.value_is_truthy(&self.registers[base + c]);
                    self.registers[base + a] = Value::Bool(lt && rt);
                }
                OpCode::Or => {
                    let lt = self.value_is_truthy(&self.registers[base + b]);
                    let rt = self.value_is_truthy(&self.registers[base + c]);
                    self.registers[base + a] = Value::Bool(lt || rt);
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
                    let truthy = self.value_is_truthy(&self.registers[base + a]);
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
                    let mut return_val = self.registers[base + a].clone();
                    self.ensure_process_instance(&mut return_val);
                    let frame = self
                        .frames
                        .pop()
                        .ok_or_else(|| VmError::Runtime("call stack underflow".into()))?;

                    let module = self.module.as_ref().ok_or(VmError::NoModule)?;
                    let cell_name = module.cells[frame.cell_idx].name.clone();
                    self.emit_debug_event(DebugEvent::CallExit {
                        cell_name,
                        result: return_val.clone(),
                    });

                    if let Some(fid) = frame.future_id {
                        self.future_states
                            .insert(fid, FutureState::Completed(return_val));
                        continue;
                    }
                    if self.frames.is_empty() {
                        return Ok(return_val);
                    }
                    self.registers[frame.return_register] = return_val;
                }
                OpCode::Halt => {
                    let msg = self.registers[base + a].as_string();
                    if let Some(fid) = self.current_future_id() {
                        let _ = self.frames.pop();
                        self.future_states.insert(fid, FutureState::Error(msg));
                        continue;
                    }
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
                            Value::Set(s) => s.iter().nth(idx as usize).cloned().unwrap_or(Value::Null),
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
                                    Value::new_tuple(vec![Value::String(StringRef::Owned(key)), val]),
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
                    // Create closure with empty captures; subsequent SetUpval
                    // instructions will populate the captures vector.
                    self.registers[base + a] = Value::Closure(ClosureValue {
                        cell_idx: bx,
                        captures: Vec::new(),
                    });
                }
                OpCode::GetUpval => {
                    // Get upvalue from current closure's captures
                    // The current frame must be running a closure
                    let frame = self
                        .frames
                        .last()
                        .ok_or_else(|| VmError::Runtime("no frame for GetUpval".into()))?;
                    let closure_reg = frame.base_register;
                    // Captures are stored at the beginning of the frame's registers
                    if b < 256 {
                        self.registers[base + a] = self.registers[closure_reg + b].clone();
                    }
                }
                OpCode::SetUpval => {
                    // A = source register, B = capture index, C = closure register
                    let val = self.registers[base + a].clone();
                    if let Value::Closure(ref mut cv) = self.registers[base + c] {
                        // Grow captures vector if needed and insert at index B
                        while cv.captures.len() <= b {
                            cv.captures.push(Value::Null);
                        }
                        cv.captures[b] = val;
                    }
                }

                // Effects
                OpCode::ToolCall => {
                    let bx = instr.bx() as usize;
                    let tool = if let Some(tool) = module.tools.get(bx) {
                        tool
                    } else {
                        self.registers[base + a] = Value::Null;
                        self.emit_debug_event(DebugEvent::ToolCall {
                            cell_name: cell.name.clone(),
                            tool_id: String::new(),
                            tool_version: String::new(),
                            latency_ms: 0,
                            success: false,
                            message: Some(format!("tool index {} out of bounds", bx)),
                        });
                        continue;
                    };

                    let mut args_map = serde_json::Map::new();
                    let primary = base + a;
                    let arg_map_reg = match self.registers.get(primary) {
                        Some(Value::Map(_)) => Some(primary),
                        Some(_) => primary.checked_add(1),
                        None => None,
                    };
                    if let Some(arg_map_reg) = arg_map_reg {
                        if let Some(Value::Map(m)) = self.registers.get(arg_map_reg) {
                            for (k, v) in m.iter() {
                                args_map.insert(k.clone(), value_to_json(v));
                            }
                        }
                    }

                    let tool_id = tool.tool_id.clone();
                    let tool_version = tool.version.clone();
                    let tool_alias = tool.alias.clone();
                    let args_json = serde_json::Value::Object(args_map);
                    let policy = merged_policy_for_tool(module, &tool_alias);
                    if let Err(msg) = validate_tool_policy(&policy, &args_json) {
                        let err_msg = format!("policy violation for '{}': {}", tool_alias, msg);
                        self.emit_debug_event(DebugEvent::ToolCall {
                            cell_name: cell.name.clone(),
                            tool_id: tool_id.clone(),
                            tool_version: tool_version.clone(),
                            latency_ms: 0,
                            success: false,
                            message: Some(err_msg.clone()),
                        });
                        if self.fail_current_future(err_msg.clone()) {
                            continue;
                        }
                        return Err(VmError::ToolError(err_msg));
                    }

                    let request = ToolRequest {
                        tool_id: tool_id.clone(),
                        version: tool_version.clone(),
                        args: args_json,
                        policy,
                    };
                    if let Some(dispatcher) = self.tool_dispatcher.as_ref() {
                        match dispatcher.dispatch(&request) {
                            Ok(response) => {
                                self.registers[base + a] = json_to_value(&response.outputs);
                                self.emit_debug_event(DebugEvent::ToolCall {
                                    cell_name: cell.name.clone(),
                                    tool_id,
                                    tool_version,
                                    latency_ms: response.latency_ms,
                                    success: true,
                                    message: None,
                                });
                            }
                            Err(e) => {
                                let err_msg = e.to_string();
                                self.emit_debug_event(DebugEvent::ToolCall {
                                    cell_name: cell.name.clone(),
                                    tool_id,
                                    tool_version,
                                    latency_ms: 0,
                                    success: false,
                                    message: Some(err_msg.clone()),
                                });
                                if self.fail_current_future(err_msg.clone()) {
                                    continue;
                                }
                                return Err(VmError::ToolError(err_msg));
                            }
                        }
                    } else {
                        let message = "<<tool call pending>>";
                        self.registers[base + a] =
                            Value::String(StringRef::Owned(message.to_string()));
                        self.emit_debug_event(DebugEvent::ToolCall {
                            cell_name: cell.name.clone(),
                            tool_id,
                            tool_version,
                            latency_ms: 0,
                            success: false,
                            message: Some("tool dispatcher not configured".to_string()),
                        });
                    }
                }
                OpCode::Schema => {
                    let bx = instr.bx() as usize;
                    let type_name = if bx < module.strings.len() {
                        module.strings[bx].clone()
                    } else {
                        String::new()
                    };
                    let val = self.registers[base + a].clone();

                    let valid = match type_name.as_str() {
                        "Int" => matches!(val, Value::Int(_)),
                        "Float" => matches!(val, Value::Float(_)),
                        "String" => matches!(val, Value::String(_)),
                        "Bool" => matches!(val, Value::Bool(_)),
                        "List" => matches!(val, Value::List(_)),
                        "Map" => matches!(val, Value::Map(_)),
                        "Tuple" => matches!(val, Value::Tuple(_)),
                        "Set" => matches!(val, Value::Set(_)),
                        _ => match &val {
                            Value::Record(r) => r.type_name == type_name,
                            _ => false,
                        },
                    };

                    self.emit_debug_event(DebugEvent::SchemaValidate {
                        cell_name: cell.name.clone(),
                        schema: type_name.clone(),
                        valid,
                    });

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
                    self.registers[base + a] = Value::TraceRef(self.next_trace_ref());
                }
                OpCode::Await => {
                    let caller_frame_idx = self.frames.len().saturating_sub(1);
                    let awaited = self.await_value_recursive(self.registers[base + b].clone())?;
                    match awaited {
                        Some(value) => {
                            self.registers[base + a] = value;
                            self.await_fuel = MAX_AWAIT_RETRIES;
                        }
                        None => {
                            if self.await_fuel == 0 {
                                return Err(VmError::Runtime(
                                    "await exceeded maximum retries on unresolvable future".into(),
                                ));
                            }
                            self.await_fuel -= 1;
                            if let Some(frame) = self.frames.get_mut(caller_frame_idx) {
                                frame.ip = frame.ip.saturating_sub(1);
                            }
                            continue;
                        }
                    }
                }
                OpCode::Spawn => {
                    let bx = instr.bx() as usize;
                    self.registers[base + a] =
                        self.spawn_future(FutureTarget::Cell(bx), Vec::new())?;
                }

                // List ops
                OpCode::Append => {
                    let val = self.registers[base + b].clone();
                    if let Value::List(ref mut l) = self.registers[base + a] {
                        Rc::make_mut(l).push(val);
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

                // Algebraic effects
                OpCode::HandlePush => {
                    let meta_idx = a as usize;
                    let offset = instr.bx() as usize;
                    let frame = self.frames.last().unwrap();
                    let handler_ip = frame.ip.saturating_sub(1) + offset;

                    let (eff_name, op_name) = if meta_idx < cell.effect_handler_metas.len() {
                        let meta = &cell.effect_handler_metas[meta_idx];
                        (meta.effect_name.clone(), meta.operation.clone())
                    } else {
                        // Fallback for legacy modules without handler metadata
                        (String::new(), String::new())
                    };

                    self.effect_handlers.push(EffectScope {
                        handler_ip,
                        frame_idx: self.frames.len() - 1,
                        base_register: base,
                        cell_idx,
                        effect_name: eff_name,
                        operation: op_name,
                    });
                }
                OpCode::HandlePop => {
                    self.effect_handlers.pop();
                }
                OpCode::Perform => {
                    let module = self.module.as_ref().ok_or(VmError::NoModule)?;
                    let cell = &module.cells[cell_idx];
                    let eff_name = match &cell.constants[b] {
                        Constant::String(s) => s.clone(),
                        _ => return Err(VmError::Runtime("perform: expected string constant for effect name".into())),
                    };
                    let op_name = match &cell.constants[c] {
                        Constant::String(s) => s.clone(),
                        _ => return Err(VmError::Runtime("perform: expected string constant for operation".into())),
                    };

                    // Search effect_handlers stack (top to bottom) for matching handler
                    let handler_scope = self.effect_handlers.iter().rev().find(|scope| {
                        scope.effect_name == eff_name && scope.operation == op_name
                    }).cloned();

                    if let Some(scope) = handler_scope {
                        // Save continuation: snapshot current execution state
                        let cont = SuspendedContinuation {
                            frames: self.frames.clone(),
                            registers: self.registers.clone(),
                            resume_ip: self.frames.last().map(|f| f.ip).unwrap_or(0),
                            resume_frame_count: self.frames.len(),
                            result_reg: base + a,
                        };
                        self.suspended_continuation = Some(cont);

                        // Jump to handler code
                        if let Some(f) = self.frames.get_mut(scope.frame_idx) {
                            f.ip = scope.handler_ip;
                        }

                        // Pass perform args to handler by storing them in the handler's registers
                        // The args start at base + a + 1 (set by lowerer)
                        // For now, the handler can read them from the same register region
                    } else {
                        return Err(VmError::Runtime(format!(
                            "unhandled effect: {}.{}",
                            eff_name, op_name
                        )));
                    }
                }
                OpCode::Resume => {
                    if let Some(cont) = self.suspended_continuation.take() {
                        let resume_value = self.registers[base + a].clone();
                        // Restore the suspended state
                        self.frames = cont.frames;
                        self.registers = cont.registers;
                        // Put the resume value into the result register
                        self.registers[cont.result_reg] = resume_value;
                        // Execution continues from the saved IP (already set in frames)
                    } else {
                        return Err(VmError::Runtime(
                            "resume called outside of effect handler".into(),
                        ));
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
                    StringRef::Interned(id) => self
                        .strings
                        .resolve(*id)
                        .ok_or_else(|| {
                            VmError::Runtime(format!(
                                "unknown interned string id {} for call target",
                                id
                            ))
                        })?
                        .to_string(),
                };
                let module = self.module.as_ref().ok_or(VmError::NoModule)?;
                if let Some(idx) = module.cells.iter().position(|c| c.name == name) {
                    if self.frames.len() >= MAX_CALL_DEPTH {
                        return Err(VmError::StackOverflow(MAX_CALL_DEPTH));
                    }
                    let callee_cell = module.cells.get(idx).ok_or_else(|| {
                        VmError::Runtime(format!("cell index {} out of bounds", idx))
                    })?;
                    let num_regs = callee_cell.registers as usize;
                    let params: Vec<LirParam> = callee_cell.params.clone();
                    let cell_regs = callee_cell.registers;
                    let _ = module;
                    let new_base = self.registers.len();
                    self.registers
                        .resize(new_base + num_regs.max(256), Value::Null);
                    self.copy_args_to_params(&params, new_base, base + a + 1, nargs, 0, cell_regs)?;
                    self.frames.push(CallFrame {
                        cell_idx: idx,
                        base_register: new_base,
                        ip: 0,
                        return_register: base + a,
                        future_id: None,
                    });
                    self.emit_debug_event(DebugEvent::CallEnter {
                        cell_name: name.clone(),
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
                let module = self.module.as_ref().ok_or(VmError::NoModule)?;
                let callee_cell = module.cells.get(cv.cell_idx).ok_or_else(|| {
                    VmError::Runtime(format!("closure cell index {} out of bounds", cv.cell_idx))
                })?;
                let num_regs = callee_cell.registers as usize;
                let params: Vec<LirParam> = callee_cell.params.clone();
                let cell_regs = callee_cell.registers;
                let _ = module;
                let new_base = self.registers.len();
                self.registers
                    .resize(new_base + num_regs.max(256), Value::Null);
                for (i, cap) in cv.captures.iter().enumerate() {
                    self.check_register(i, cell_regs)?;
                    self.registers[new_base + i] = cap.clone();
                }
                let cap_count = cv.captures.len();
                self.copy_args_to_params(
                    &params,
                    new_base,
                    base + a + 1,
                    nargs,
                    cap_count,
                    cell_regs,
                )?;
                self.frames.push(CallFrame {
                    cell_idx: cv.cell_idx,
                    base_register: new_base,
                    ip: 0,
                    return_register: base + a,
                    future_id: None,
                });
                let module = self.module.as_ref().ok_or(VmError::NoModule)?;
                let cell_name = module
                    .cells
                    .get(cv.cell_idx)
                    .ok_or_else(|| {
                        VmError::Runtime(format!(
                            "closure cell index {} out of bounds",
                            cv.cell_idx
                        ))
                    })?
                    .name
                    .clone();
                self.emit_debug_event(DebugEvent::CallEnter { cell_name });
            }
            _ => {
                println!("DEBUG: cannot call {:?} (type: {})", callee, callee.type_name());
                println!("DEBUG: Current frame: {:?}", self.frames.last());
                if let Some(frame) = self.frames.last() {
                     if let Some(module) = self.module.as_ref() {
                         if let Some(cell) = module.cells.get(frame.cell_idx) {
                             println!("DEBUG: Instructions for cell '{}':", cell.name);
                             println!("DEBUG: Constants: {:?}", cell.constants);
                             for (i, instr) in cell.instructions.iter().enumerate() {
                                 println!("  {:03}: {:?}", i, instr);
                             }
                         }
                     }
                }
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
                    StringRef::Interned(id) => self
                        .strings
                        .resolve(*id)
                        .ok_or_else(|| {
                            VmError::Runtime(format!(
                                "unknown interned string id {} for tailcall target",
                                id
                            ))
                        })?
                        .to_string(),
                };
                let module = self.module.as_ref().ok_or(VmError::NoModule)?;
                if let Some(idx) = module.cells.iter().position(|c| c.name == name) {
                    let callee_cell = module.cells.get(idx).ok_or_else(|| {
                        VmError::Runtime(format!("cell index {} out of bounds", idx))
                    })?;
                    let params: Vec<LirParam> = callee_cell.params.clone();
                    let cell_regs = callee_cell.registers;
                    let _ = module;
                    self.copy_args_to_params(&params, base, base + a + 1, nargs, 0, cell_regs)?;
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
                let module = self.module.as_ref().ok_or(VmError::NoModule)?;
                let callee_cell = module.cells.get(cv.cell_idx).ok_or_else(|| {
                    VmError::Runtime(format!("closure cell index {} out of bounds", cv.cell_idx))
                })?;
                let params: Vec<LirParam> = callee_cell.params.clone();
                let cell_regs = callee_cell.registers;
                let _ = module;
                for (i, cap) in cv.captures.iter().enumerate() {
                    self.check_register(i, cell_regs)?;
                    self.registers[base + i] = cap.clone();
                }
                let cap_count = cv.captures.len();
                self.copy_args_to_params(&params, base, base + a + 1, nargs, cap_count, cell_regs)?;
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
}


impl Default for VM {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumen_compiler::compile as compile_lumen;
    use lumen_runtime::tools::{
        StubDispatcher, ToolError as RtToolError, ToolProvider, ToolSchema,
    };

    fn run_main(source: &str) -> Value {
        let md = format!("# test\n\n```lumen\n{}\n```\n", source.trim());
        let module = compile_lumen(&md).expect("source should compile");
        let mut vm = VM::new();
        vm.load(module);
        vm.execute("main", vec![]).expect("main should execute")
    }

    fn run_main_with_dispatcher(
        source: &str,
        dispatcher: StubDispatcher,
    ) -> Result<Value, VmError> {
        let md = format!("# test\n\n```lumen\n{}\n```\n", source.trim());
        let module = compile_lumen(&md).expect("source should compile");
        let mut vm = VM::new();
        vm.tool_dispatcher = Some(Box::new(dispatcher));
        vm.load(module);
        vm.execute("main", vec![])
    }

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
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
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
                        variadic: false,
                    },
                    LirParam {
                        name: "b".into(),
                        ty: "Int".into(),
                        register: 1,
                        variadic: false,
                    },
                ],
                returns: Some("Int".into()),
                registers: 4,
                constants: vec![],
                instructions: vec![
                    Instruction::abc(OpCode::Add, 2, 0, 1),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        }
    }

    fn make_set_index_on_non_indexable() -> LirModule {
        LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Int".into()),
                registers: 3,
                constants: vec![Constant::Int(7), Constant::Int(0), Constant::Int(42)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abx(OpCode::LoadK, 2, 2),
                    Instruction::abc(OpCode::SetIndex, 0, 1, 2),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
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
    fn test_set_index_on_non_indexable_type_errors() {
        let mut vm = VM::new();
        vm.load(make_set_index_on_non_indexable());
        let err = vm
            .execute("main", vec![])
            .expect_err("set index on integer should fail");
        assert!(err.is_type_error(), "expected TypeError, got {err:?}");
        let msg = format!("{}", err);
        assert!(msg.contains("cannot assign by index"));
        assert!(msg.contains("expected list, map, or record"));
    }

    #[test]
    fn test_trace_ref_defaults_to_doc_hash_and_resets_each_execute() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "doc-hash-abc".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: None,
                registers: 2,
                constants: vec![],
                instructions: vec![
                    Instruction::abc(OpCode::TraceRef, 0, 0, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.load(module);

        let first = vm
            .execute("main", vec![])
            .expect("first run should succeed");
        let second = vm
            .execute("main", vec![])
            .expect("second run should succeed");

        match first {
            Value::TraceRef(trace) => {
                assert_eq!(trace.trace_id, "doc:doc-hash-abc");
                assert_eq!(trace.seq, 1);
            }
            other => panic!("expected trace ref, got {:?}", other),
        }

        match second {
            Value::TraceRef(trace) => {
                assert_eq!(trace.trace_id, "doc:doc-hash-abc");
                assert_eq!(trace.seq, 1);
            }
            other => panic!("expected trace ref, got {:?}", other),
        }
    }

    #[test]
    fn test_trace_ref_uses_explicit_trace_id_and_monotonic_seq() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "doc-hash-abc".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: None,
                registers: 6,
                constants: vec![],
                instructions: vec![
                    Instruction::abc(OpCode::TraceRef, 0, 0, 0),
                    Instruction::abc(OpCode::Intrinsic, 1, 8, 0),
                    Instruction::abc(OpCode::Move, 3, 0, 0),
                    Instruction::abc(OpCode::Move, 4, 1, 0),
                    Instruction::abc(OpCode::NewList, 2, 2, 0),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.set_trace_id("run-123");
        vm.load(module);
        let result = vm.execute("main", vec![]).expect("main should succeed");

        let refs = match result {
            Value::List(values) => values,
            other => panic!("expected list return, got {:?}", other),
        };

        assert_eq!(refs.len(), 2);
        match &refs[0] {
            Value::TraceRef(trace) => {
                assert_eq!(trace.trace_id, "run-123");
                assert_eq!(trace.seq, 1);
            }
            other => panic!("expected trace ref at index 0, got {:?}", other),
        }
        match &refs[1] {
            Value::TraceRef(trace) => {
                assert_eq!(trace.trace_id, "run-123");
                assert_eq!(trace.seq, 2);
            }
            other => panic!("expected trace ref at index 1, got {:?}", other),
        }
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
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
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
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
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
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
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
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
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
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]).unwrap();
        assert_eq!(
            result,
            Value::new_tuple(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
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
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
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
    fn test_toset_intrinsic_deduplicates() {
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
                    Constant::Int(1),
                    Constant::Int(2),
                    Constant::Int(1),
                    Constant::Int(3),
                ],
                instructions: vec![
                    Instruction::abc(OpCode::NewList, 0, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 0), // 1
                    Instruction::abc(OpCode::Append, 0, 1, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1), // 2
                    Instruction::abc(OpCode::Append, 0, 1, 0),
                    Instruction::abx(OpCode::LoadK, 1, 2), // 1 (duplicate)
                    Instruction::abc(OpCode::Append, 0, 1, 0),
                    Instruction::abx(OpCode::LoadK, 1, 3), // 3
                    Instruction::abc(OpCode::Append, 0, 1, 0),
                    // Convert list to set using ToSet intrinsic (ID 69)
                    Instruction::abc(OpCode::Intrinsic, 2, 69, 0),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]).unwrap();
        // ToSet should deduplicate: [1,2,1,3] -> {1,2,3}
        if let Value::Set(s) = result {
            assert_eq!(s.len(), 3, "ToSet should deduplicate elements");
            assert!(s.contains(&Value::Int(1)));
            assert!(s.contains(&Value::Int(2)));
            assert!(s.contains(&Value::Int(3)));
        } else {
            panic!("expected set, got {:?}", result);
        }
    }

    #[test]
    fn test_debug_hooks_capture_steps() {
        use std::sync::{Arc, Mutex};
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = Arc::clone(&events);

        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: None,
                registers: 4,
                constants: vec![Constant::Int(5), Constant::Int(3)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),  // r0 = 5
                    Instruction::abx(OpCode::LoadK, 1, 1),  // r1 = 3
                    Instruction::abc(OpCode::Add, 2, 0, 1), // r2 = r0 + r1
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.debug_callback = Some(Box::new(move |event| {
            events_clone.lock().unwrap().push(event.clone());
        }));
        vm.load(module);
        let result = vm.execute("main", vec![]).unwrap();
        assert_eq!(result, Value::Int(8));

        let captured_events = events.lock().unwrap();
        // Should have Step events for each instruction
        let step_count = captured_events
            .iter()
            .filter(|e| matches!(e, DebugEvent::Step { .. }))
            .count();
        assert!(
            step_count >= 4,
            "should capture at least 4 step events, got {}",
            step_count
        );
    }

    #[test]
    fn test_debug_hooks_capture_call_exit() {
        use std::sync::{Arc, Mutex};
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = Arc::clone(&events);

        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: None,
                registers: 2,
                constants: vec![Constant::Int(42)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0), // r0 = 42
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.debug_callback = Some(Box::new(move |event| {
            events_clone.lock().unwrap().push(event.clone());
        }));
        vm.load(module);
        let _ = vm.execute("main", vec![]);

        let captured_events = events.lock().unwrap();
        // Should have a CallExit event when returning
        let has_call_exit = captured_events
            .iter()
            .any(|e| matches!(e, DebugEvent::CallExit { .. }));
        assert!(has_call_exit, "should capture CallExit event");
    }

    #[test]
    fn test_debug_hooks_capture_tool_and_schema_events() {
        use std::sync::{Arc, Mutex};

        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = Arc::clone(&events);

        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec!["String".into()],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("String".into()),
                registers: 4,
                constants: vec![],
                instructions: vec![
                    Instruction::abx(OpCode::ToolCall, 0, 0),
                    Instruction::abx(OpCode::Schema, 0, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![LirTool {
                alias: "HttpGet".into(),
                tool_id: "http.get".into(),
                version: "1.0.0".into(),
                mcp_url: None,
            }],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut dispatcher = StubDispatcher::new();
        dispatcher.set_response("http.get", serde_json::json!("ok"));

        let mut vm = VM::new();
        vm.debug_callback = Some(Box::new(move |event| {
            events_clone
                .lock()
                .expect("events lock should succeed")
                .push(event.clone());
        }));
        vm.tool_dispatcher = Some(Box::new(dispatcher));
        vm.load(module);

        let result = vm.execute("main", vec![]).expect("main should execute");
        assert_eq!(result, Value::String(StringRef::Owned("ok".to_string())));

        let captured_events = events.lock().expect("events lock should succeed");
        let has_tool_call = captured_events.iter().any(|event| {
            matches!(
                event,
                DebugEvent::ToolCall {
                    cell_name,
                    tool_id,
                    tool_version,
                    success,
                    ..
                } if cell_name == "main"
                    && tool_id == "http.get"
                    && tool_version == "1.0.0"
                    && *success
            )
        });
        assert!(has_tool_call, "should capture successful tool call event");

        let has_schema_validate = captured_events.iter().any(|event| {
            matches!(
                event,
                DebugEvent::SchemaValidate {
                    cell_name,
                    schema,
                    valid
                } if cell_name == "main" && schema == "String" && *valid
            )
        });
        assert!(
            has_schema_validate,
            "should capture successful schema validation event"
        );
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
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]).unwrap();
        assert_eq!(result, Value::Int(0b1000));
    }

    #[test]
    fn test_match_guard_and_or_runtime() {
        let result = run_main(
            r#"
cell main() -> Int
  let x = ok(5)
  match x
    ok(v) if v > 3 -> return 1
    ok(v) | err(v) -> return 2
    _ -> return 0
  end
end
"#,
        );
        assert_eq!(result, Value::Int(1));
    }

    #[test]
    fn test_match_list_destructure_with_rest_runtime() {
        let result = run_main(
            r#"
cell main() -> Int
  match [1, 2, 3, 4]
    [head, second, ...rest] -> return length(rest)
    _ -> return 0
  end
end
"#,
        );
        assert_eq!(result, Value::Int(2));
    }

    #[test]
    fn test_match_tuple_destructure_runtime() {
        let result = run_main(
            r#"
cell main() -> Int
  match (2, 5)
    (a, b) -> return a + b
    _ -> return 0
  end
end
"#,
        );
        assert_eq!(result, Value::Int(7));
    }

    #[test]
    fn test_match_record_destructure_runtime() {
        let result = run_main(
            r#"
record Point
  x: Int
  y: Int
end

cell main() -> Int
  let p = Point(x: 3, y: 4)
  match p
    Point(x: x, y: y) -> return x + y
    _ -> return 0
  end
end
"#,
        );
        assert_eq!(result, Value::Int(7));
    }

    #[test]
    fn test_match_type_check_pattern_runtime() {
        let result = run_main(
            r#"
cell main() -> Int
  let v: Int | String = 9
  match v
    n: Int -> return n
    _ -> return 0
  end
end
"#,
        );
        assert_eq!(result, Value::Int(9));
    }

    #[test]
    fn test_process_constructor_and_method_dispatch() {
        let result = run_main(
            r#"
pipeline Incrementer
  cell run(x: Int) -> Int
    return x + 1
  end
end

cell main() -> Int
  let p = Incrementer()
  return p.run(4)
end
"#,
        );
        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn test_process_static_dot_dispatch_via_constructor() {
        let result = run_main(
            r#"
pipeline IdentityPipe
end

cell main() -> Int
  return IdentityPipe.run(7)
end
"#,
        );
        assert_eq!(result, Value::Int(7));
    }

    #[test]
    fn test_pipeline_declarative_stages_generate_run_semantics() {
        let result = run_main(
            r#"
cell inc(x: Int) -> Int
  return x + 1
end

cell dbl(x: Int) -> Int
  return x * 2
end

pipeline NumberPipe
  stages:
    inc
      -> dbl
  end
end

cell main() -> Int
  let p = NumberPipe()
  return p.run(3)
end
"#,
        );
        assert_eq!(result, Value::Int(8));
    }

    #[test]
    fn test_pipeline_three_stages() {
        let result = run_main(
            r#"
cell add_one(x: Int) -> Int
  return x + 1
end

cell double(x: Int) -> Int
  return x * 2
end

cell square(x: Int) -> Int
  return x * x
end

pipeline ThreeStage
  stages:
    add_one
      -> double
      -> square
  end
end

cell main() -> Int
  let p = ThreeStage()
  return p.run(4)
end
"#,
        );
        // 4 -> add_one -> 5 -> double -> 10 -> square -> 100
        assert_eq!(result, Value::Int(100));
    }

    #[test]
    fn test_pipeline_vm_level_stage_chaining() {
        // Test the VM-level pipeline stage chaining directly by compiling
        // stage cells, then manually registering a pipeline in the VM
        // without a generated `run` cell.
        let md = "# test\n\n```lumen\ncell inc(x: Int) -> Int\n  return x + 1\nend\n\ncell dbl(x: Int) -> Int\n  return x * 2\nend\n\ncell main() -> Int\n  return 0\nend\n```\n";
        let mut module = compile_lumen(md).expect("should compile");

        // Inject pipeline addon metadata (as if a pipeline process were declared)
        module.addons.push(LirAddon {
            kind: "pipeline".to_string(),
            name: Some("TestPipe".to_string()),
        });
        module.addons.push(LirAddon {
            kind: "pipeline.stages".to_string(),
            name: Some(r#"TestPipe=["inc","dbl"]"#.to_string()),
        });

        let mut vm = VM::new();
        vm.load(module);

        // Verify stage metadata was loaded
        assert_eq!(
            vm.pipeline_stages.get("TestPipe"),
            Some(&vec!["inc".to_string(), "dbl".to_string()])
        );

        // Directly test pipeline stage chaining via the VM
        // Input: 3 -> inc -> 4 -> dbl -> 8
        let result = vm
            .call_pipeline_run("TestPipe", &[Value::Null, Value::Int(3)])
            .expect("pipeline run should succeed");
        assert_eq!(result, Value::Int(8));
    }

    #[test]
    fn test_pipeline_vm_level_empty_stages() {
        // Pipeline with no stages should return the input unchanged
        let md = "# test\n\n```lumen\ncell main() -> Int\n  return 0\nend\n```\n";
        let mut module = compile_lumen(md).expect("should compile");

        module.addons.push(LirAddon {
            kind: "pipeline".to_string(),
            name: Some("EmptyPipe".to_string()),
        });

        let mut vm = VM::new();
        vm.load(module);

        let result = vm
            .call_pipeline_run("EmptyPipe", &[Value::Null, Value::Int(42)])
            .expect("empty pipeline should succeed");
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_orchestration_vm_level_fan_out() {
        // Orchestration runs all stages with the same input and collects results
        let md = "# test\n\n```lumen\ncell inc(x: Int) -> Int\n  return x + 1\nend\n\ncell dbl(x: Int) -> Int\n  return x * 2\nend\n\ncell main() -> Int\n  return 0\nend\n```\n";
        let mut module = compile_lumen(md).expect("should compile");

        module.addons.push(LirAddon {
            kind: "orchestration".to_string(),
            name: Some("FanOut".to_string()),
        });
        module.addons.push(LirAddon {
            kind: "pipeline.stages".to_string(),
            name: Some(r#"FanOut=["inc","dbl"]"#.to_string()),
        });

        let mut vm = VM::new();
        vm.load(module);

        // Orchestration: input 5 -> [inc(5)=6, dbl(5)=10]
        let result = vm
            .call_orchestration_run("FanOut", &[Value::Null, Value::Int(5)])
            .expect("orchestration should succeed");
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], Value::Int(6));
                assert_eq!(items[1], Value::Int(10));
            }
            other => panic!("expected list, got {:?}", other),
        }
    }

    #[test]
    fn test_memory_runtime_append_recent() {
        let result = run_main(
            r#"
memory ConversationBuffer
end

cell main() -> Int
  let m = ConversationBuffer()
  m.append("a")
  m.append("b")
  let recent = m.recent(1)
  return length(recent)
end
"#,
        );
        assert_eq!(result, Value::Int(1));
    }

    #[test]
    fn test_memory_runtime_upsert_get() {
        let result = run_main(
            r#"
memory UserFacts
end

cell main() -> String
  let m = UserFacts()
  m.upsert("user_123", "alice")
  return m.get("user_123")
end
"#,
        );
        assert_eq!(result, Value::String(StringRef::Owned("alice".to_string())));
    }

    #[test]
    fn test_memory_instances_are_isolated() {
        let result = run_main(
            r#"
memory Buf
end

cell main() -> Int
  let a = Buf()
  let b = Buf()
  a.append("x")
  return length(b.recent(10))
end
"#,
        );
        assert_eq!(result, Value::Int(0));
    }

    #[test]
    fn test_machine_runtime_methods() {
        let result = run_main(
            r#"
machine TicketHandler
end

cell main() -> Bool
  let machine = TicketHandler()
  machine.start("ticket")
  machine.step()
  return machine.is_terminal()
end
"#,
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_machine_instances_are_isolated() {
        let result = run_main(
            r#"
machine TicketHandler
end

cell main() -> Bool
  let a = TicketHandler()
  let b = TicketHandler()
  a.start("ticket")
  a.step()
  return b.is_terminal()
end
"#,
        );
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_machine_graph_start_step_transitions_to_terminal_state() {
        let result = run_main(
            r#"
machine TicketFlow
  initial: Start
  state Start
    on_enter()
      transition Done()
    end
  end
  state Done
    terminal: true
  end
end

cell main() -> Bool
  let machine = TicketFlow()
  machine.start("ticket")
  machine.step()
  return machine.is_terminal()
end
"#,
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_machine_graph_current_state_starts_at_initial() {
        let result = run_main(
            r#"
machine TicketFlow
  initial: Start
  state Start
    transition Done()
  end
  state Done
    terminal: true
  end
end

cell main() -> String
  let machine = TicketFlow()
  machine.start("ticket")
  let st = machine.current_state()
  return st.name
end
"#,
        );
        assert_eq!(result, Value::String(StringRef::Owned("Start".to_string())));
    }

    #[test]
    fn test_machine_graph_guard_and_typed_payload_transition() {
        let result = run_main(
            r#"
machine TypedFlow
  initial: Start
  state Start(x: Int)
    guard: x > 0
    transition Done(x + 1)
  end
  state Done(v: Int)
    terminal: true
  end
end

cell main() -> Int
  let m = TypedFlow()
  m.start(4)
  m.step()
  let st = m.current_state()
  return st.payload.v
end
"#,
        );
        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn test_machine_graph_guard_blocks_transition_when_false() {
        let result = run_main(
            r#"
machine TypedFlow
  initial: Start
  state Start(x: Int)
    guard: x > 0
    transition Done(x + 1)
  end
  state Done(v: Int)
    terminal: true
  end
end

cell main() -> Bool
  let m = TypedFlow()
  m.start(0)
  m.step()
  return m.is_terminal()
end
"#,
        );
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_machine_graph_guard_divide_by_zero_returns_error() {
        let md = r#"
# test

```lumen
machine RiskyFlow
  initial: Start
  state Start(x: Int)
    guard: x / 0 > 0
    transition Done(x)
  end
  state Done(v: Int)
    terminal: true
  end
end

cell main() -> Bool
  let m = RiskyFlow()
  m.start(1)
  m.step()
  return m.is_terminal()
end
```
"#;
        let module = compile_lumen(md).expect("source should compile");
        let mut vm = VM::new();
        vm.load(module);
        let err = vm.execute("main", vec![]).unwrap_err();
        assert!(err.is_division_by_zero(), "expected DivisionByZero, got: {:?}", err);
    }

    #[test]
    fn test_machine_graph_transition_modulo_by_zero_returns_error() {
        let md = r#"
# test

```lumen
machine RiskyFlow
  initial: Start
  state Start(x: Int)
    guard: true
    transition Done(x % 0)
  end
  state Done(v: Int)
    terminal: true
  end
end

cell main() -> Bool
  let m = RiskyFlow()
  m.start(1)
  m.step()
  return m.is_terminal()
end
```
"#;
        let module = compile_lumen(md).expect("source should compile");
        let mut vm = VM::new();
        vm.load(module);
        let err = vm.execute("main", vec![]).unwrap_err();
        assert!(err.is_division_by_zero(), "expected DivisionByZero, got: {:?}", err);
    }

    fn make_spawn_await_module(
        worker_instrs: Vec<Instruction>,
        worker_consts: Vec<Constant>,
    ) -> LirModule {
        LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![
                LirCell {
                    name: "main".into(),
                    params: vec![],
                    returns: Some("Int".into()),
                    registers: 8,
                    constants: vec![],
                    instructions: vec![
                        Instruction::abx(OpCode::Spawn, 0, 1),
                        Instruction::abc(OpCode::Await, 1, 0, 0),
                        Instruction::abc(OpCode::Return, 1, 1, 0),
                    ],
                    effect_handler_metas: vec![],
                },
                LirCell {
                    name: "worker".into(),
                    params: vec![],
                    returns: Some("Int".into()),
                    registers: 4,
                    constants: worker_consts,
                    instructions: worker_instrs,
                    effect_handler_metas: vec![],
                },
            ],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        }
    }

    #[test]
    fn test_spawn_await_eager_schedule() {
        let module = make_spawn_await_module(
            vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            vec![Constant::Int(42)],
        );
        let mut vm = VM::new();
        vm.set_future_schedule(FutureSchedule::Eager);
        vm.load(module);
        let out = vm
            .execute("main", vec![])
            .expect("spawn/await should resolve");
        assert_eq!(out, Value::Int(42));
    }

    #[test]
    fn test_spawn_await_deferred_fifo_schedule() {
        let module = make_spawn_await_module(
            vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            vec![Constant::Int(7)],
        );
        let mut vm = VM::new();
        vm.set_future_schedule(FutureSchedule::DeferredFifo);
        vm.load(module);
        let out = vm
            .execute("main", vec![])
            .expect("deferred spawn/await should resolve deterministically");
        assert_eq!(out, Value::Int(7));
    }

    #[test]
    fn test_spawn_await_failed_future_propagates_error() {
        let module = make_spawn_await_module(
            vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Halt, 0, 0, 0),
            ],
            vec![Constant::String("boom".into())],
        );
        let mut vm = VM::new();
        vm.set_future_schedule(FutureSchedule::DeferredFifo);
        vm.load(module);
        let err = vm.execute("main", vec![]).unwrap_err();
        assert!(
            err.to_string().contains("await failed for future"),
            "expected await failure, got: {}",
            err
        );
    }

    #[test]
    fn test_load_sets_deferred_schedule_from_deterministic_directive() {
        let mut module = make_return_42();
        module.addons.push(LirAddon {
            kind: "directive".to_string(),
            name: Some("deterministic=true".to_string()),
        });
        let mut vm = VM::new();
        vm.load(module);
        assert_eq!(vm.future_schedule(), FutureSchedule::DeferredFifo);
    }

    #[test]
    fn test_explicit_future_schedule_not_overridden_by_directive() {
        let mut module = make_return_42();
        module.addons.push(LirAddon {
            kind: "directive".to_string(),
            name: Some("deterministic=true".to_string()),
        });
        let mut vm = VM::new();
        vm.set_future_schedule(FutureSchedule::Eager);
        vm.load(module);
        assert_eq!(vm.future_schedule(), FutureSchedule::Eager);
    }

    #[test]
    fn test_parallel_builtin_collects_values() {
        let result = run_main(
            r#"
cell main() -> Int
  let xs = parallel(1, 2, 3)
  return length(xs)
end
"#,
        );
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn test_race_builtin_returns_first_value() {
        let result = run_main(
            r#"
cell main() -> Int
  return race(9, 10)
end
"#,
        );
        assert_eq!(result, Value::Int(9));
    }

    #[test]
    fn test_vote_builtin_returns_majority() {
        let result = run_main(
            r#"
cell main() -> Int
  return vote(2, 1, 2, 3)
end
"#,
        );
        assert_eq!(result, Value::Int(2));
    }

    #[test]
    fn test_select_builtin_returns_first_non_null() {
        let result = run_main(
            r#"
cell main() -> Int
  return select(null, 5, 7)
end
"#,
        );
        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn test_timeout_builtin_returns_value_for_non_future() {
        let result = run_main(
            r#"
cell main() -> Int
  return timeout(42, 10)
end
"#,
        );
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_await_parallel_for_block_runtime_deferred_schedule() {
        let result = run_main(
            r#"
@deterministic true

cell main() -> Int / {async}
  let values = await parallel for i in 0..5
    i * 2
  end
  return length(values)
end
"#,
        );
        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn test_await_race_block_runtime_deferred_schedule() {
        let result = run_main(
            r#"
@deterministic true

cell main() -> Int / {async}
  return await race
    7
    9
  end
end
"#,
        );
        assert_eq!(result, Value::Int(7));
    }

    #[test]
    fn test_await_parallel_block_runtime() {
        let result = run_main(
            r#"
cell main() -> Int / {async}
  let values = await parallel
    1
    2
    3
  end
  return length(values)
end
"#,
        );
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn test_tool_alias_call_dispatches_to_runtime_tool() {
        let mut dispatcher = StubDispatcher::new();
        dispatcher.set_response("http.get", serde_json::json!({"body": "ok"}));

        let result = run_main_with_dispatcher(
            r#"
use tool http.get as HttpGet
bind effect http to HttpGet
grant HttpGet

cell main() -> String / {http}
  let resp = HttpGet(url: "https://api.example.com")
  return resp.body
end
"#,
            dispatcher,
        )
        .expect("tool call should succeed");

        assert_eq!(result, Value::String(StringRef::Owned("ok".to_string())));
    }

    #[test]
    fn test_tool_policy_violation_blocks_dispatch() {
        let mut dispatcher = StubDispatcher::new();
        dispatcher.set_response("http.get", serde_json::json!({"body": "ok"}));

        let err = run_main_with_dispatcher(
            r#"
use tool http.get as HttpGet
bind effect http to HttpGet
grant HttpGet
  domain "*.example.com"

cell main() -> String / {http}
  let resp = HttpGet(url: "https://malicious.tld")
  return resp.body
end
"#,
            dispatcher,
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("policy violation"),
            "expected policy violation error, got: {}",
            err
        );
    }

    // ===== Arithmetic safety tests =====

    #[test]
    fn test_integer_overflow_add() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Int".into()),
                registers: 4,
                constants: vec![Constant::Int(i64::MAX), Constant::Int(1)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::Add, 2, 0, 1),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let err = vm.execute("main", vec![]).unwrap_err();
        assert!(
            err.is_arithmetic_overflow() || err.to_string().contains("overflow"),
            "expected arithmetic overflow, got: {}",
            err
        );
    }

    #[test]
    fn test_integer_overflow_sub() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Int".into()),
                registers: 4,
                constants: vec![Constant::Int(i64::MIN), Constant::Int(1)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::Sub, 2, 0, 1),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let err = vm.execute("main", vec![]).unwrap_err();
        assert!(
            err.is_arithmetic_overflow() || err.to_string().contains("overflow"),
            "expected arithmetic overflow, got: {}",
            err
        );
    }

    #[test]
    fn test_integer_overflow_mul() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Int".into()),
                registers: 4,
                constants: vec![Constant::Int(i64::MAX), Constant::Int(2)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::Mul, 2, 0, 1),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let err = vm.execute("main", vec![]).unwrap_err();
        assert!(
            err.is_arithmetic_overflow() || err.to_string().contains("overflow"),
            "expected arithmetic overflow, got: {}",
            err
        );
    }

    #[test]
    fn test_division_by_zero() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Int".into()),
                registers: 4,
                constants: vec![Constant::Int(42), Constant::Int(0)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::Div, 2, 0, 1),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let err = vm.execute("main", vec![]).unwrap_err();
        assert!(err.is_division_by_zero(), "expected DivisionByZero, got: {:?}", err);
    }

    #[test]
    fn test_modulo_by_zero() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Int".into()),
                registers: 4,
                constants: vec![Constant::Int(42), Constant::Int(0)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::Mod, 2, 0, 1),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let err = vm.execute("main", vec![]).unwrap_err();
        assert!(err.is_division_by_zero(), "expected DivisionByZero, got: {:?}", err);
    }

    #[test]
    fn test_pow_exponent_out_of_range() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Int".into()),
                registers: 4,
                constants: vec![Constant::Int(2), Constant::Int(64)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::Pow, 2, 0, 1),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let err = vm.execute("main", vec![]).unwrap_err();
        assert!(
            err.to_string().contains("exponent out of range"),
            "expected exponent out of range, got: {}",
            err
        );
    }

    #[test]
    fn test_shift_out_of_range() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Int".into()),
                registers: 4,
                constants: vec![Constant::Int(1), Constant::Int(64)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::Shl, 2, 0, 1),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let err = vm.execute("main", vec![]).unwrap_err();
        assert!(
            err.to_string().contains("shift amount out of range"),
            "expected shift amount out of range, got: {}",
            err
        );
    }

    #[test]
    fn test_negative_shift_out_of_range() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Int".into()),
                registers: 4,
                constants: vec![Constant::Int(1), Constant::Int(-1)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::Shr, 2, 0, 1),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let err = vm.execute("main", vec![]).unwrap_err();
        assert!(
            err.to_string().contains("shift amount out of range"),
            "expected shift amount out of range, got: {}",
            err
        );
    }

    #[test]
    fn test_stack_overflow_detection() {
        // Verify MAX_CALL_DEPTH is enforced
        let mut vm = VM::new();
        // Push frames up to the limit
        for _ in 0..MAX_CALL_DEPTH {
            vm.frames.push(CallFrame {
                cell_idx: 0,
                base_register: 0,
                ip: 0,
                return_register: 0,
                future_id: None,
            });
        }
        assert_eq!(vm.frames.len(), MAX_CALL_DEPTH);
    }

    #[test]
    fn test_closure_setupval_populates_captures() {
        // Verify SetUpval writes into the closure's capture vector, not frame registers.
        // Build a module with two cells: main creates a closure and calls it,
        // the closure reads its captured variable via GetUpval.
        //
        // main:
        //   r0 = 42               (LoadInt)
        //   r1 = Closure(1)       (Closure referencing cell index 1)
        //   SetUpval(r0, 0, r1)   (capture r0 into closure slot 0)
        //   r2 = "call_closure"   (dummy -- we'll call r1 directly)
        //   Call(r1, 0)           (call the closure with 0 args)
        //   Return(r1)            (return result -- closure puts result in r1 after call)
        //
        // closure (cell 1):
        //   GetUpval(r0, 0)       (read capture slot 0 into r0)
        //   Return(r0)            (return captured value)
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![
                LirCell {
                    name: "main".into(),
                    params: vec![],
                    returns: Some("Int".into()),
                    registers: 8,
                    constants: vec![],
                    instructions: vec![
                        Instruction::abx(OpCode::LoadInt, 0, 42_u16), // r0 = 42
                        Instruction::abx(OpCode::Closure, 1, 1),      // r1 = Closure(cell 1)
                        Instruction::abc(OpCode::SetUpval, 0, 0, 1), // capture r0 -> closure[0] in r1
                        Instruction::abc(OpCode::Call, 1, 0, 0),     // call r1 with 0 args
                        Instruction::abc(OpCode::Return, 1, 1, 0),   // return result
                    ],
                    effect_handler_metas: vec![],
                },
                LirCell {
                    name: "__closure_0".into(),
                    params: vec![],
                    returns: Some("Int".into()),
                    registers: 4,
                    constants: vec![],
                    instructions: vec![
                        Instruction::abc(OpCode::GetUpval, 0, 0, 0), // r0 = capture[0]
                        Instruction::abc(OpCode::Return, 0, 1, 0),   // return r0
                    ],
                    effect_handler_metas: vec![],
                },
            ],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_closure_multiple_captures() {
        // Verify a closure can capture multiple values via SetUpval.
        // main: r0=10, r1=20, create closure, capture both, call it
        // closure: reads both captures and returns their sum
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![
                LirCell {
                    name: "main".into(),
                    params: vec![],
                    returns: Some("Int".into()),
                    registers: 8,
                    constants: vec![],
                    instructions: vec![
                        Instruction::abx(OpCode::LoadInt, 0, 10_u16), // r0 = 10
                        Instruction::abx(OpCode::LoadInt, 1, 20_u16), // r1 = 20
                        Instruction::abx(OpCode::Closure, 2, 1),      // r2 = Closure(cell 1)
                        Instruction::abc(OpCode::SetUpval, 0, 0, 2),  // capture r0 -> closure[0]
                        Instruction::abc(OpCode::SetUpval, 1, 1, 2),  // capture r1 -> closure[1]
                        Instruction::abc(OpCode::Call, 2, 0, 0),      // call closure
                        Instruction::abc(OpCode::Return, 2, 1, 0),    // return result
                    ],
                    effect_handler_metas: vec![],
                },
                LirCell {
                    name: "__closure_1".into(),
                    params: vec![],
                    returns: Some("Int".into()),
                    registers: 4,
                    constants: vec![],
                    instructions: vec![
                        Instruction::abc(OpCode::GetUpval, 0, 0, 0), // r0 = capture[0] (10)
                        Instruction::abc(OpCode::GetUpval, 1, 1, 0), // r1 = capture[1] (20)
                        Instruction::abc(OpCode::Add, 2, 0, 1),      // r2 = r0 + r1
                        Instruction::abc(OpCode::Return, 2, 1, 0),   // return 30
                    ],
                    effect_handler_metas: vec![],
                },
            ],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };
        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]).unwrap();
        assert_eq!(result, Value::Int(30));
    }

    #[test]
    fn test_closure_e2e_make_adder() {
        // End-to-end test: compile and run a Lumen program with closures.
        let result = run_main(
            r#"
cell make_adder(x: Int) -> fn(Int) -> Int
  return fn(y: Int) => x + y
end

cell main() -> Int
  let add5 = make_adder(5)
  return add5(10)
end
"#,
        );
        assert_eq!(result, Value::Int(15));
    }

    #[test]
    fn test_value_is_truthy_interned_empty_string() {
        // VM's value_is_truthy should resolve interned strings and check emptiness
        let mut vm = VM::new();
        let empty_id = vm.strings.intern("");
        let nonempty_id = vm.strings.intern("hello");

        assert!(!vm.value_is_truthy(&Value::String(StringRef::Interned(empty_id))));
        assert!(vm.value_is_truthy(&Value::String(StringRef::Interned(nonempty_id))));
    }

    #[test]
    fn test_get_constant_returns_error_on_no_module() {
        let vm = VM::new();
        let result = vm.get_constant(0, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_module_string_returns_error_on_no_module() {
        let vm = VM::new();
        let result = vm.get_module_string(0);
        assert!(result.is_err());
    }

    #[test]
    fn test_eq_opcode_interned_vs_owned_string() {
        // Eq opcode should resolve interned strings for cross-representation equality.
        // Build a module where r0 = owned "hello", r1 = interned "hello"
        // then Eq(2, 0, 1) should produce true.
        //
        // We use the module string table for interning -- the VM interns all
        // module.strings on load. We use NewRecord with string index 0 to
        // get an interned reference, but that's complex. Instead, set up
        // registers directly and run via the internal dispatch loop.
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec!["hello".to_string()],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Bool".into()),
                registers: 8,
                constants: vec![Constant::String("hello".into())],
                instructions: vec![
                    // r0 = owned "hello" from constant
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    // Eq(2, 0, 1): compare r0 (owned) with r1 (will be interned)
                    Instruction::abc(OpCode::Eq, 2, 0, 1),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.load(module);

        // Manually place an interned string in r1 before execution
        let interned_id = vm.strings.intern("hello");
        // We need to set r1 in the frame that execute() will create.
        // execute() will create a frame at base = current registers.len().
        // So we pre-size and set up r1 at the right offset.
        let base = vm.registers.len();
        vm.registers.resize(base + 256, Value::Null);
        vm.registers[base + 1] = Value::String(StringRef::Interned(interned_id));

        vm.frames.push(CallFrame {
            cell_idx: 0,
            base_register: base,
            ip: 0,
            return_register: 0,
            future_id: None,
        });

        let result = vm.run().unwrap();
        assert_eq!(
            result,
            Value::Bool(true),
            "interned 'hello' should equal owned 'hello'"
        );
    }

    // ===== ProviderRegistry integration tests =====

    /// A simple ToolProvider that returns a fixed JSON response.
    struct FixedProvider {
        name: String,
        response: serde_json::Value,
        schema: ToolSchema,
    }

    impl FixedProvider {
        fn new(name: &str, response: serde_json::Value) -> Self {
            Self {
                name: name.to_string(),
                response,
                schema: ToolSchema {
                    name: name.to_string(),
                    description: format!("Fixed provider: {}", name),
                    input_schema: serde_json::json!({}),
                    output_schema: serde_json::json!({}),
                    effects: vec!["http".to_string()],
                },
            }
        }
    }

    impl ToolProvider for FixedProvider {
        fn name(&self) -> &str {
            &self.name
        }
        fn version(&self) -> &str {
            "1.0.0"
        }
        fn schema(&self) -> &ToolSchema {
            &self.schema
        }
        fn call(&self, _input: serde_json::Value) -> Result<serde_json::Value, RtToolError> {
            Ok(self.response.clone())
        }
    }

    fn run_main_with_registry(source: &str, registry: ProviderRegistry) -> Result<Value, VmError> {
        let md = format!("# test\n\n```lumen\n{}\n```\n", source.trim());
        let module = compile_lumen(&md).expect("source should compile");
        let mut vm = VM::new();
        vm.set_provider_registry(registry);
        vm.load(module);
        vm.execute("main", vec![])
    }

    #[test]
    fn test_provider_registry_dispatches_tool_call() {
        let mut registry = ProviderRegistry::new();
        registry.register(
            "http.get",
            Box::new(FixedProvider::new(
                "http.get",
                serde_json::json!({"body": "registry_ok"}),
            )),
        );

        let result = run_main_with_registry(
            r#"
use tool http.get as HttpGet
bind effect http to HttpGet
grant HttpGet

cell main() -> String / {http}
  let resp = HttpGet(url: "https://api.example.com")
  return resp.body
end
"#,
            registry,
        )
        .expect("tool call via registry should succeed");

        assert_eq!(
            result,
            Value::String(StringRef::Owned("registry_ok".to_string()))
        );
    }

    #[test]
    fn test_provider_registry_missing_provider_returns_error() {
        let registry = ProviderRegistry::new(); // empty -- no providers

        let err = run_main_with_registry(
            r#"
use tool http.get as HttpGet
bind effect http to HttpGet
grant HttpGet

cell main() -> String / {http}
  let resp = HttpGet(url: "https://api.example.com")
  return resp.body
end
"#,
            registry,
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("not registered"),
            "expected 'not registered' error, got: {}",
            err
        );
    }

    // ===== validate_tool_policy unit tests =====

    #[test]
    fn test_policy_generic_max_constraint_allows_within_limit() {
        let policy = serde_json::json!({"max_tokens": 100});
        let args = serde_json::json!({"max_tokens": 50});
        assert!(validate_tool_policy(&policy, &args).is_ok());
    }

    #[test]
    fn test_policy_generic_max_constraint_rejects_over_limit() {
        let policy = serde_json::json!({"max_tokens": 100});
        let args = serde_json::json!({"max_tokens": 200});
        let err = validate_tool_policy(&policy, &args).unwrap_err();
        assert!(
            err.contains("max_tokens"),
            "error should mention the key: {}",
            err
        );
        assert!(
            err.contains("200"),
            "error should mention the actual value: {}",
            err
        );
    }

    #[test]
    fn test_policy_generic_max_constraint_works_for_any_max_key() {
        let policy = serde_json::json!({"max_retries": 3});
        let args = serde_json::json!({"max_retries": 5});
        let err = validate_tool_policy(&policy, &args).unwrap_err();
        assert!(
            err.contains("max_retries"),
            "error should mention the key: {}",
            err
        );

        let policy = serde_json::json!({"max_cost": 1000});
        let args = serde_json::json!({"max_cost": 500});
        assert!(validate_tool_policy(&policy, &args).is_ok());
    }

    #[test]
    fn test_policy_generic_max_constraint_requires_integer_constraint() {
        let policy = serde_json::json!({"max_tokens": "100"});
        let args = serde_json::json!({"max_tokens": 50});
        let err = validate_tool_policy(&policy, &args).unwrap_err();
        assert!(
            err.contains("max_tokens constraint must be an integer"),
            "error should explain max_* type requirement: {}",
            err
        );
    }

    #[test]
    fn test_policy_domain_constraint() {
        let policy = serde_json::json!({"domain": "*.example.com"});
        let args = serde_json::json!({"url": "https://api.example.com/data"});
        assert!(validate_tool_policy(&policy, &args).is_ok());

        let args_bad = serde_json::json!({"url": "https://malicious.tld/data"});
        assert!(validate_tool_policy(&policy, &args_bad).is_err());
    }

    #[test]
    fn test_policy_timeout_ms_constraint() {
        let policy = serde_json::json!({"timeout_ms": 5000});
        let args = serde_json::json!({"timeout_ms": 3000});
        assert!(validate_tool_policy(&policy, &args).is_ok());

        let args_over = serde_json::json!({"timeout_ms": 10000});
        let err = validate_tool_policy(&policy, &args_over).unwrap_err();
        assert!(
            err.contains("timeout_ms"),
            "error should mention timeout_ms: {}",
            err
        );
    }

    // ---- Collection intrinsic tests ----

    #[test]
    fn test_intrinsic_sort() {
        let result = run_main(
            r#"
cell main() -> list[Int]
  let xs = [3, 1, 4, 1, 5, 9, 2]
  return sort(xs)
end
"#,
        );
        assert_eq!(
            result,
            Value::new_list(vec![
                Value::Int(1),
                Value::Int(1),
                Value::Int(2),
                Value::Int(3),
                Value::Int(4),
                Value::Int(5),
                Value::Int(9),
            ])
        );
    }

    #[test]
    fn test_intrinsic_reverse() {
        let result = run_main(
            r#"
cell main() -> list[Int]
  let xs = [1, 2, 3]
  return reverse(xs)
end
"#,
        );
        assert_eq!(
            result,
            Value::new_list(vec![Value::Int(3), Value::Int(2), Value::Int(1)])
        );
    }

    #[test]
    fn test_intrinsic_flatten() {
        let result = run_main(
            r#"
cell main() -> list[Int]
  let xs = [[1, 2], [3], [4, 5]]
  return flatten(xs)
end
"#,
        );
        assert_eq!(
            result,
            Value::new_list(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(3),
                Value::Int(4),
                Value::Int(5),
            ])
        );
    }

    #[test]
    fn test_intrinsic_unique() {
        let result = run_main(
            r#"
cell main() -> list[Int]
  let xs = [1, 2, 2, 3, 1, 4]
  return unique(xs)
end
"#,
        );
        assert_eq!(
            result,
            Value::new_list(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(3),
                Value::Int(4),
            ])
        );
    }

    #[test]
    fn test_intrinsic_take() {
        let result = run_main(
            r#"
cell main() -> list[Int]
  let xs = [10, 20, 30, 40, 50]
  return take(xs, 3)
end
"#,
        );
        assert_eq!(
            result,
            Value::new_list(vec![Value::Int(10), Value::Int(20), Value::Int(30)])
        );
    }

    #[test]
    fn test_intrinsic_drop() {
        let result = run_main(
            r#"
cell main() -> list[Int]
  let xs = [10, 20, 30, 40, 50]
  return drop(xs, 2)
end
"#,
        );
        assert_eq!(
            result,
            Value::new_list(vec![Value::Int(30), Value::Int(40), Value::Int(50)])
        );
    }

    #[test]
    fn test_intrinsic_chunk() {
        let result = run_main(
            r#"
cell main() -> list[list[Int]]
  let xs = [1, 2, 3, 4, 5]
  return chunk(xs, 2)
end
"#,
        );
        assert_eq!(
            result,
            Value::new_list(vec![
                Value::new_list(vec![Value::Int(1), Value::Int(2)]),
                Value::new_list(vec![Value::Int(3), Value::Int(4)]),
                Value::new_list(vec![Value::Int(5)]),
            ])
        );
    }

    #[test]
    fn test_intrinsic_zip() {
        let result = run_main(
            r#"
cell main() -> list[list[Int]]
  let a = [1, 2, 3]
  let b = [4, 5, 6]
  return zip(a, b)
end
"#,
        );
        // zip returns tuples, but let's check what we get
        if let Value::List(items) = &result {
            assert_eq!(items.len(), 3);
        } else {
            panic!("expected list from zip");
        }
    }

    #[test]
    fn test_intrinsic_enumerate() {
        let result = run_main(
            r#"
cell main() -> list[list[Int]]
  let xs = [10, 20, 30]
  return enumerate(xs)
end
"#,
        );
        if let Value::List(items) = &result {
            assert_eq!(items.len(), 3);
            // First element should be tuple (0, 10)
            if let Value::Tuple(t) = &items[0] {
                assert_eq!(t[0], Value::Int(0));
                assert_eq!(t[1], Value::Int(10));
            }
        } else {
            panic!("expected list from enumerate");
        }
    }

    // ---- String intrinsic tests ----

    #[test]
    fn test_intrinsic_starts_with() {
        let result = run_main(
            r#"
cell main() -> Bool
  return starts_with("hello world", "hello")
end
"#,
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_intrinsic_ends_with() {
        let result = run_main(
            r#"
cell main() -> Bool
  return ends_with("hello world", "world")
end
"#,
        );
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_intrinsic_index_of() {
        let result = run_main(
            r#"
cell main() -> Int
  return index_of("hello world", "world")
end
"#,
        );
        assert_eq!(result, Value::Int(6));
    }

    #[test]
    fn test_intrinsic_index_of_not_found() {
        let result = run_main(
            r#"
cell main() -> Int
  return index_of("hello world", "xyz")
end
"#,
        );
        assert_eq!(result, Value::Int(-1));
    }

    #[test]
    fn test_intrinsic_pad_left() {
        let result = run_main(
            r#"
cell main() -> String
  return pad_left("hi", 5)
end
"#,
        );
        assert_eq!(result, Value::String(StringRef::Owned("   hi".into())));
    }

    #[test]
    fn test_intrinsic_pad_right() {
        let result = run_main(
            r#"
cell main() -> String
  return pad_right("hi", 5)
end
"#,
        );
        assert_eq!(result, Value::String(StringRef::Owned("hi   ".into())));
    }

    // ---- Math intrinsic tests ----

    #[test]
    fn test_intrinsic_round() {
        let result = run_main(
            r#"
cell main() -> Float
  return round(3.7)
end
"#,
        );
        assert_eq!(result, Value::Float(4.0));
    }

    #[test]
    fn test_intrinsic_ceil() {
        let result = run_main(
            r#"
cell main() -> Float
  return ceil(3.2)
end
"#,
        );
        assert_eq!(result, Value::Float(4.0));
    }

    #[test]
    fn test_intrinsic_floor() {
        let result = run_main(
            r#"
cell main() -> Float
  return floor(3.9)
end
"#,
        );
        assert_eq!(result, Value::Float(3.0));
    }

    #[test]
    fn test_intrinsic_sqrt() {
        let result = run_main(
            r#"
cell main() -> Float
  return sqrt(16.0)
end
"#,
        );
        assert_eq!(result, Value::Float(4.0));
    }

    #[test]
    fn test_intrinsic_pow() {
        let result = run_main(
            r#"
cell main() -> Int
  return pow(2, 10)
end
"#,
        );
        assert_eq!(result, Value::Int(1024));
    }

    #[test]
    fn test_intrinsic_clamp() {
        let result = run_main(
            r#"
cell main() -> Int
  return clamp(15, 0, 10)
end
"#,
        );
        assert_eq!(result, Value::Int(10));
    }

    #[test]
    fn test_intrinsic_clamp_below() {
        let result = run_main(
            r#"
cell main() -> Int
  return clamp(-5, 0, 10)
end
"#,
        );
        assert_eq!(result, Value::Int(0));
    }

    // ---- Window intrinsic test ----

    #[test]
    fn test_intrinsic_window() {
        let result = run_main(
            r#"
cell main() -> list[list[Int]]
  let xs = [1, 2, 3, 4, 5]
  return window(xs, 3)
end
"#,
        );
        assert_eq!(
            result,
            Value::new_list(vec![
                Value::new_list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
                Value::new_list(vec![Value::Int(2), Value::Int(3), Value::Int(4)]),
                Value::new_list(vec![Value::Int(3), Value::Int(4), Value::Int(5)]),
            ])
        );
    }

    #[test]
    fn test_register_oob_reports_invalid_operand_in_hot_path() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Int".into()),
                registers: 1,
                constants: vec![],
                instructions: vec![
                    Instruction::abc(OpCode::LoadInt, 1, 0, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.load(module);
        let err = vm
            .execute("main", vec![])
            .expect_err("invalid register operand should fail");
        assert!(err.is_register_oob(), "expected RegisterOOB, got: {:?}", err);
    }

    #[test]
    fn test_register_oob_reports_invalid_call_argument_span() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Int".into()),
                registers: 2,
                constants: vec![Constant::String("len".into())],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abc(OpCode::Call, 0, 2, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.load(module);
        let err = vm
            .execute("main", vec![])
            .expect_err("invalid call argument span should fail");
        assert!(err.is_register_oob(), "expected RegisterOOB, got: {:?}", err);
    }

    #[test]
    fn test_call_with_unknown_interned_target_returns_runtime_error() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![LirParam {
                    name: "f".into(),
                    ty: "String".into(),
                    register: 0,
                    variadic: false,
                }],
                returns: Some("Int".into()),
                registers: 2,
                constants: vec![],
                instructions: vec![
                    Instruction::abc(OpCode::Call, 0, 0, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.load(module);
        let err = vm
            .execute("main", vec![Value::String(StringRef::Interned(999_999))])
            .expect_err("invalid interned function target should fail");
        assert!(
            err.to_string().contains("unknown interned string id"),
            "expected interned-id runtime error, got: {}",
            err
        );
    }

    // ---- P0 Runtime Safety Tests ----

    #[test]
    fn test_arithmetic_overflow_add() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Int".into()),
                registers: 4,
                constants: vec![Constant::Int(i64::MAX), Constant::Int(1)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::Add, 2, 0, 1),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().is_arithmetic_overflow());
    }

    #[test]
    fn test_arithmetic_overflow_sub() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Int".into()),
                registers: 4,
                constants: vec![Constant::Int(i64::MIN), Constant::Int(1)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::Sub, 2, 0, 1),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().is_arithmetic_overflow());
    }

    #[test]
    fn test_arithmetic_overflow_mul() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Int".into()),
                registers: 4,
                constants: vec![Constant::Int(i64::MAX), Constant::Int(2)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::Mul, 2, 0, 1),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().is_arithmetic_overflow());
    }

    #[test]
    fn test_div_by_zero_error_type() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Int".into()),
                registers: 4,
                constants: vec![Constant::Int(42), Constant::Int(0)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::Div, 2, 0, 1),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().is_division_by_zero());
    }

    #[test]
    fn test_mod_by_zero_error_type() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Int".into()),
                registers: 4,
                constants: vec![Constant::Int(42), Constant::Int(0)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::Mod, 2, 0, 1),
                    Instruction::abc(OpCode::Return, 2, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().is_division_by_zero());
    }

    #[test]
    fn test_string_slice_utf8_safe() {
        // Test that string slicing works on UTF-8 character boundaries
        let result = run_main(
            r#"
cell main() -> String
  let s = "Hello, 世界"
  return slice(s, 7, 9)
end
"#,
        );
        assert_eq!(result, Value::String(StringRef::Owned("世界".to_string())));
    }

    #[test]
    fn test_hex_decode_odd_length_returns_null() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: None,
                registers: 4,
                constants: vec![
                    Constant::String("hex_decode".into()),
                    Constant::String("abc".into()),
                ],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::Call, 0, 1, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.load(module);
        let result = vm
            .execute("main", vec![])
            .expect("execution should succeed");
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_hex_decode_non_ascii_returns_null() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: None,
                registers: 4,
                constants: vec![
                    Constant::String("hex_decode".into()),
                    Constant::String("a€".into()),
                ],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abx(OpCode::LoadK, 1, 1),
                    Instruction::abc(OpCode::Call, 0, 1, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.load(module);
        let result = vm
            .execute("main", vec![])
            .expect("execution should succeed");
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_instruction_limit_prevents_infinite_loop() {
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: None,
                registers: 1,
                constants: vec![],
                instructions: vec![Instruction::sax(OpCode::Jmp, -1)],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.set_instruction_limit(64);
        vm.load(module);
        let err = vm
            .execute("main", vec![])
            .expect_err("loop should hit instruction limit");
        assert!(err.is_instruction_limit_exceeded(), "expected InstructionLimitExceeded, got: {:?}", err);
    }

    #[test]
    fn test_stack_trace_capture() {
        let md = r#"
# test

```lumen
cell main() -> Int
  return helper()
end

cell helper() -> Int
  return divide(10, 0)
end

cell divide(a: Int, b: Int) -> Int
  return a / b
end
```
"#;
        let module = compile_lumen(md).expect("source should compile");
        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]);

        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.is_division_by_zero(), "expected DivisionByZero, got: {:?}", e);
            // WithStackTrace should include the stack frames
            let frames = e.stack_frames();
            assert!(!frames.is_empty(), "stack trace should have frames");
            // The error message should include the stack trace
            let msg = format!("{}", e);
            assert!(msg.contains("Stack trace"), "error should include stack trace: {}", msg);
        }
    }

    #[test]
    fn test_fuel_exhaustion() {
        // Create a simple infinite loop: Jmp -1
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: None,
                registers: 1,
                constants: vec![],
                instructions: vec![Instruction::sax(OpCode::Jmp, -1)],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.set_fuel(10);
        vm.load(module);
        let err = vm
            .execute("main", vec![])
            .expect_err("should run out of fuel");
        assert!(err.message_contains("fuel exhausted"), "expected fuel exhausted, got: {:?}", err);
    }

    #[test]
    fn test_fuel_sufficient_for_simple_program() {
        // A program that returns 42 — should succeed with enough fuel
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: None,
                registers: 1,
                constants: vec![Constant::Int(42)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.set_fuel(100);
        vm.load(module);
        let result = vm.execute("main", vec![]).expect("should have enough fuel");
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_no_fuel_set_runs_normally() {
        // Without set_fuel, the program runs normally (no fuel limit)
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: None,
                registers: 1,
                constants: vec![Constant::Int(99)],
                instructions: vec![
                    Instruction::abx(OpCode::LoadK, 0, 0),
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
                effect_handler_metas: vec![],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        // Don't set fuel — should run normally
        vm.load(module);
        let result = vm
            .execute("main", vec![])
            .expect("should run without fuel limit");
        assert_eq!(result, Value::Int(99));
    }

    // ── Algebraic Effects Tests ──

    #[test]
    fn test_perform_compiles() {
        // Verify a program with perform compiles and loads
        let source = r#"
effect Console
  cell log(message: String) -> Null
end

cell main() -> Int
  return 42
end
"#;
        let result = run_main(source);
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_handle_push_pop_opcodes() {
        // Verify HandlePush/HandlePop opcodes don't crash VM
        let module = LirModule {
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
                    Instruction::abx(OpCode::HandlePush, 0, 4), // meta_idx=0, handler code at offset +4
                    Instruction::abx(OpCode::LoadK, 0, 0),  // load 42
                    Instruction::ax(OpCode::HandlePop, 0),   // pop handler
                    Instruction::abc(OpCode::Return, 0, 1, 0), // return 42
                ],
                effect_handler_metas: vec![LirEffectHandlerMeta {
                    effect_name: "TestEffect".into(),
                    operation: "test_op".into(),
                    param_count: 0,
                    handler_ip: 4,
                }],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]).expect("should execute successfully");
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_effect_handler_stack_management() {
        // Verify effect handler stack is properly managed
        let mut vm = VM::new();
        assert!(vm.effect_handlers.is_empty());
        vm.effect_handlers.push(EffectScope {
            handler_ip: 0,
            frame_idx: 0,
            base_register: 0,
            cell_idx: 0,
            effect_name: "TestEffect".into(),
            operation: "test_op".into(),
        });
        assert_eq!(vm.effect_handlers.len(), 1);
        vm.effect_handlers.pop();
        assert!(vm.effect_handlers.is_empty());
    }

    #[test]
    fn test_perform_matches_correct_handler() {
        // Test that Perform finds the handler that matches effect_name + operation.
        //
        // Program structure:
        //   HandlePush meta=0 (Console.log), offset to handler code
        //   Perform Console.log("hello") -> result in r0
        //   HandlePop
        //   Return r0
        //   --- handler code ---
        //   Resume(r1) where r1 = 42
        //
        // The handler for Console.log should match the perform Console.log call.
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Int".into()),
                registers: 8,
                constants: vec![
                    Constant::String("Console".into()),  // 0: effect name for Perform
                    Constant::String("log".into()),       // 1: operation for Perform
                    Constant::String("hello".into()),     // 2: arg
                    Constant::Int(42),                    // 3: resume value
                ],
                instructions: vec![
                    // 0: HandlePush meta_idx=0, offset=5 (handler code at ip 0+5=5)
                    Instruction::abx(OpCode::HandlePush, 0, 5),
                    // 1: Perform Console.log -> result to r0, eff_name=const[0], op=const[1]
                    Instruction::abc(OpCode::Perform, 0, 0, 1),
                    // 2: HandlePop
                    Instruction::ax(OpCode::HandlePop, 0),
                    // 3: Return r0 (the resumed value)
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                    // 4: Jmp past handler (never reached in this test)
                    Instruction::sax(OpCode::Jmp, 2),
                    // 5: Handler code: load 42 into r1
                    Instruction::abx(OpCode::LoadK, 1, 3),
                    // 6: Resume with r1
                    Instruction::abc(OpCode::Resume, 1, 1, 0),
                ],
                effect_handler_metas: vec![LirEffectHandlerMeta {
                    effect_name: "Console".into(),
                    operation: "log".into(),
                    param_count: 1,
                    handler_ip: 5,
                }],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]).expect("should execute with matching handler");
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_perform_unhandled_effect_error() {
        // Test that performing an effect with no matching handler produces an error.
        //
        // We push a handler for Console.log but perform Console.read_line.
        // This should fail with "unhandled effect: Console.read_line".
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
                    Constant::String("Console".into()),      // 0: effect name
                    Constant::String("read_line".into()),     // 1: operation (not handled!)
                ],
                instructions: vec![
                    // 0: HandlePush for Console.log (meta_idx=0), offset=4
                    Instruction::abx(OpCode::HandlePush, 0, 4),
                    // 1: Perform Console.read_line (NOT Console.log)
                    Instruction::abc(OpCode::Perform, 0, 0, 1),
                    // 2: HandlePop
                    Instruction::ax(OpCode::HandlePop, 0),
                    // 3: Return
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                    // 4: Handler code (unreachable — wrong effect)
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                ],
                effect_handler_metas: vec![LirEffectHandlerMeta {
                    effect_name: "Console".into(),
                    operation: "log".into(), // handler is for "log", not "read_line"
                    param_count: 1,
                    handler_ip: 4,
                }],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.load(module);
        let err = vm.execute("main", vec![]).expect_err("should fail with unhandled effect");
        let msg = format!("{}", err);
        assert!(
            msg.contains("unhandled effect: Console.read_line"),
            "error should mention the unhandled effect, got: {}",
            msg
        );
    }

    #[test]
    fn test_perform_matches_among_multiple_handlers() {
        // Test that the correct handler is selected when multiple handlers are installed.
        //
        // We push handlers for Console.log (meta 0) and Console.read_line (meta 1).
        // Then perform Console.read_line. The second handler should match.
        let module = LirModule {
            version: "1.0.0".into(),
            doc_hash: "test".into(),
            strings: vec![],
            types: vec![],
            cells: vec![LirCell {
                name: "main".into(),
                params: vec![],
                returns: Some("Int".into()),
                registers: 8,
                constants: vec![
                    Constant::String("Console".into()),       // 0
                    Constant::String("read_line".into()),     // 1
                    Constant::Int(99),                        // 2: wrong handler value
                    Constant::Int(7),                         // 3: correct handler value
                ],
                instructions: vec![
                    // 0: HandlePush meta=0 (Console.log), offset to handler at 8
                    Instruction::abx(OpCode::HandlePush, 0, 8),
                    // 1: HandlePush meta=1 (Console.read_line), offset to handler at 9
                    Instruction::abx(OpCode::HandlePush, 1, 9),
                    // 2: Perform Console.read_line -> r0
                    Instruction::abc(OpCode::Perform, 0, 0, 1),
                    // 3: HandlePop (read_line)
                    Instruction::ax(OpCode::HandlePop, 0),
                    // 4: HandlePop (log)
                    Instruction::ax(OpCode::HandlePop, 0),
                    // 5: Return r0
                    Instruction::abc(OpCode::Return, 0, 1, 0),
                    // 6: Jmp past both handlers
                    Instruction::sax(OpCode::Jmp, 4),
                    // 7: (padding — ensures offsets work)
                    Instruction::abc(OpCode::Nop, 0, 0, 0),
                    // 8: Handler for Console.log: load 99 and resume
                    Instruction::abx(OpCode::LoadK, 1, 2),
                    // 9: (would be resume for log handler — but we also use this as start of read_line handler)
                    //    Actually, let's restructure: each handler gets separate code
                    Instruction::abc(OpCode::Resume, 1, 1, 0),
                    // 10: Handler for Console.read_line: load 7 and resume
                    Instruction::abx(OpCode::LoadK, 2, 3),
                    // 11: Resume with 7
                    Instruction::abc(OpCode::Resume, 2, 2, 0),
                ],
                effect_handler_metas: vec![
                    LirEffectHandlerMeta {
                        effect_name: "Console".into(),
                        operation: "log".into(),
                        param_count: 1,
                        handler_ip: 8,
                    },
                    LirEffectHandlerMeta {
                        effect_name: "Console".into(),
                        operation: "read_line".into(),
                        param_count: 0,
                        handler_ip: 10,
                    },
                ],
            }],
            tools: vec![],
            policies: vec![],
            agents: vec![],
            addons: vec![],
            effects: vec![],
            effect_binds: vec![],
            handlers: vec![],
        };

        let mut vm = VM::new();
        vm.load(module);
        let result = vm.execute("main", vec![]).expect("should match read_line handler");
        // The read_line handler resumes with 7, so result should be 7
        assert_eq!(result, Value::Int(7));
    }
}
