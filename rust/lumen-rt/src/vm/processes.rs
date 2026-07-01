//! Memory, machine, and pipeline process runtime methods for the VM.

use super::*;
use lumen_compiler::compile_raw;
use lumen_core::heap_value::HeapValue;
use std::collections::BTreeMap;

#[derive(Debug, Default, Clone)]
pub(crate) struct MemoryRuntime {
    pub(crate) entries: Vec<NbValue>,
    pub(crate) kv: BTreeMap<String, NbValue>,
}

#[derive(Debug, Clone)]
pub(crate) struct MachineRuntime {
    pub(crate) started: bool,
    pub(crate) terminal: bool,
    pub(crate) steps: u64,
    pub(crate) current_state: String,
    pub(crate) payload: BTreeMap<String, NbValue>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct MachineGraphDef {
    pub(crate) initial: String,
    pub(crate) states: BTreeMap<String, MachineStateDef>,
}

#[derive(Debug, Clone)]
pub(crate) struct MachineStateDef {
    pub(crate) params: Vec<MachineParamDef>,
    pub(crate) terminal: bool,
    pub(crate) guard: Option<MachineExpr>,
    pub(crate) transition_to: Option<String>,
    pub(crate) transition_args: Vec<MachineExpr>,
}

#[derive(Debug, Clone)]
pub(crate) struct MachineParamDef {
    pub(crate) name: String,
    pub(crate) ty: String,
}

#[derive(Debug, Clone)]
pub(crate) enum MachineExpr {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
    Ident(String),
    Unary {
        op: String,
        expr: Box<MachineExpr>,
    },
    Bin {
        op: String,
        lhs: Box<MachineExpr>,
        rhs: Box<MachineExpr>,
    },
}

impl Default for MachineRuntime {
    fn default() -> Self {
        Self {
            started: false,
            terminal: false,
            steps: 0,
            current_state: "init".to_string(),
            payload: BTreeMap::new(),
        }
    }
}

impl VM {
    pub(crate) fn try_call_process_builtin(
        &mut self,
        name: &str,
        base: usize,
        a: usize,
        nargs: usize,
    ) -> Option<Result<NbValue, VmError>> {
        let (owner, method) = name.split_once('.')?;
        let kind = self.process_kinds.get(owner)?.clone();
        match kind.as_str() {
            "memory" => Some(self.call_memory_method(owner, method, base, a, nargs)),
            "machine" => Some(self.call_machine_method(owner, method, base, a, nargs)),
            "pipeline" if method == "run" => {
                let args: Vec<NbValue> = (0..nargs)
                    .map(|i| self.registers[base + a + 1 + i])
                    .collect();
                Some(self.call_pipeline_run(owner, &args))
            }
            "orchestration" if method == "run" => {
                let args: Vec<NbValue> = (0..nargs)
                    .map(|i| self.registers[base + a + 1 + i])
                    .collect();
                Some(self.call_orchestration_run(owner, &args))
            }
            "eval" if method == "run" => {
                let config = self.process_configs.get(owner);
                if let Some(source) = config.and_then(|c| c.get("source")).map(|v| v.as_string()) {
                    // Source-based eval: compile and execute
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos();
                    let cell_name = format!("__eval_{}", now);
                    let wrapped_src =
                        format!("cell {}(input) -> Any\n  {}\nend", cell_name, source);

                    match compile_raw(&wrapped_src) {
                        Ok(new_module) => {
                            if let Some(current_mod) = self.module.as_mut() {
                                current_mod.merge(&new_module);
                                let input_val = self.registers[base + a + 2];
                                Some(self.call_cell_sync(&cell_name, vec![input_val]))
                            } else {
                                Some(Err(VmError::Runtime(
                                    "VM has no module loaded for eval".into(),
                                )))
                            }
                        }
                        Err(e) => Some(Err(VmError::Runtime(format!(
                            "eval compilation failed: {}",
                            e
                        )))),
                    }
                } else {
                    // Argument-based eval: run specific cell by name
                    let cell_name_val = self.registers[base + a + 2];
                    let cell_name = cell_name_val.display();
                    if cell_name.is_empty() {
                        return Some(Err(VmError::Runtime(
                            "eval requires a cell name argument or 'source' config".to_string(),
                        )));
                    }

                    // Collect remaining arguments
                    let start_arg = base + a + 3;
                    let end_arg = base + a + 1 + nargs;
                    let call_args: Vec<NbValue> =
                        (start_arg..end_arg).map(|i| self.registers[i]).collect();

                    Some(self.call_cell_sync(&cell_name, call_args))
                }
            }
            "guardrail" if method == "run" => {
                let value = self.registers[base + a + 2].clone();
                let config = self.process_configs.get(owner);
                if let Some(schema_name) =
                    config.and_then(|c| c.get("schema")).map(|v| v.as_string())
                {
                    // Perform schema validation
                    if self.validate_schema(&value, &schema_name) {
                        Some(Ok(value))
                    } else {
                        Some(Err(VmError::Runtime(format!(
                            "Guardrail violation: value does not match schema '{}'",
                            schema_name
                        ))))
                    }
                } else {
                    // Passthrough if no schema configured
                    Some(Ok(value))
                }
            }
            "pattern" if method == "run" => {
                let value = self.registers[base + a + 2].display();
                let config = self.process_configs.get(owner);
                if let Some(pattern_def) =
                    config.and_then(|c| c.get("pattern")).map(|v| v.as_string())
                {
                    // Extract captures if pattern matches
                    if let Some(captures) = self.extract_pattern_captures(&pattern_def, &value) {
                        let nb_map: BTreeMap<String, NbValue> = captures
                            .into_iter()
                            .map(|(k, v)| (k, NbValue::new_str(&v.as_string())))
                            .collect();
                        Some(Ok(NbValue::new_map(nb_map)))
                    } else {
                        Some(Ok(NbValue::new_null()))
                    }
                } else {
                    Some(Ok(NbValue::new_null()))
                }
            }
            _ => None,
        }
    }

    /// Execute a named cell synchronously with the given arguments, saving and
    /// restoring the current frame/register state so this can be called from
    /// within a process builtin handler.
    pub(crate) fn call_cell_sync(
        &mut self,
        cell_name: &str,
        args: Vec<NbValue>,
    ) -> Result<NbValue, VmError> {
        let module = self.module.as_ref().ok_or(VmError::NoModule)?;
        let cell_idx = module
            .cells
            .iter()
            .position(|c| c.name == cell_name)
            .ok_or_else(|| VmError::UndefinedCell(cell_name.into()))?;

        let cell = &module.cells[cell_idx];
        let num_regs = cell.registers as usize;
        let params = cell.params.clone();

        // Save current execution state
        let saved_frames = std::mem::take(&mut self.frames);
        let saved_registers = std::mem::take(&mut self.registers);

        // Set up a fresh execution context for the target cell
        self.registers.resize(num_regs.max(16), NbValue::new_null());
        for (i, arg) in args.into_iter().enumerate() {
            if i < params.len() {
                let dst = params[i].register as usize;
                if dst < self.registers.len() {
                    self.registers[dst] = arg;
                }
            }
        }
        self.frames.push(CallFrame {
            cell_idx,
            base_register: 0,
            ip: 0,
            return_register: 0,
            future_id: None,
            osr_points: 0,
        });

        let result = self.run_until(0);
        let ret_nb = self
            .registers
            .get(0)
            .copied()
            .unwrap_or(NbValue::new_null());

        // Restore execution state
        self.frames = saved_frames;
        self.registers = saved_registers;

        result.map(|_| ret_nb)
    }

    /// Execute a pipeline's `run` method by chaining stage calls.
    /// Each stage cell is called with the output of the previous stage,
    /// starting from the provided input argument.
    pub(crate) fn call_pipeline_run(
        &mut self,
        owner: &str,
        args: &[NbValue],
    ) -> Result<NbValue, VmError> {
        let input = args.get(1).copied().unwrap_or(NbValue::new_null());
        let stages = self.pipeline_stages.get(owner).cloned().unwrap_or_default();
        if stages.is_empty() {
            return Ok(input);
        }
        let mut value = input;
        for stage in &stages {
            value = self.call_cell_sync(stage, vec![value])?;
        }
        Ok(value)
    }

    /// Execute an orchestration's `run` method by running all stage cells
    /// with the same input and collecting results into a list.
    pub(crate) fn call_orchestration_run(
        &mut self,
        owner: &str,
        args: &[NbValue],
    ) -> Result<NbValue, VmError> {
        let input = args.get(1).copied().unwrap_or(NbValue::new_null());
        let stages = self.pipeline_stages.get(owner).cloned().unwrap_or_default();
        if stages.is_empty() {
            return Ok(input);
        }
        let mut results = Vec::with_capacity(stages.len());
        for stage in &stages {
            let result = self.call_cell_sync(stage, vec![input.clone()])?;
            results.push(result);
        }
        Ok(NbValue::new_list(results))
    }

    fn call_memory_method(
        &mut self,
        owner: &str,
        method: &str,
        base: usize,
        a: usize,
        nargs: usize,
    ) -> Result<NbValue, VmError> {
        let args: Vec<NbValue> = (0..nargs)
            .map(|i| self.registers[base + a + 1 + i])
            .collect();

        let instance_id = helpers::process_instance_id(args.first()).ok_or_else(|| {
            VmError::TypeError(format!(
                "{}.{} requires a process instance as the first argument",
                owner, method
            ))
        })?;

        let store = self.memory_runtime.entry(instance_id).or_default();
        match method {
            "append" | "remember" => {
                if let Some(val) = args.get(1) {
                    store.entries.push(val.clone());
                }
                Ok(NbValue::new_null())
            }
            "recent" => {
                let n = args.get(1).and_then(|v| v.as_int()).unwrap_or(10).max(0) as usize;
                let len = store.entries.len();
                let start = len.saturating_sub(n);
                Ok(NbValue::new_list(store.entries[start..].to_vec()))
            }
            "recall" => {
                let n = args
                    .get(2)
                    .or_else(|| args.get(1))
                    .and_then(|v| v.as_int())
                    .unwrap_or(5)
                    .max(0) as usize;
                let len = store.entries.len();
                let start = len.saturating_sub(n);
                Ok(NbValue::new_list(store.entries[start..].to_vec()))
            }
            "upsert" | "store" => {
                if let (Some(key), Some(value)) = (args.get(1), args.get(2)) {
                    let key_str = key.display();
                    store.kv.insert(key_str, value.clone());
                }
                Ok(NbValue::new_null())
            }
            "get" => {
                let key = args.get(1).map(|v| v.display()).unwrap_or_default();
                Ok(store.kv.get(&key).copied().unwrap_or(NbValue::new_null()))
            }
            "query" => {
                let filter = args.get(1).map(|v| v.display());
                let mut out = Vec::new();
                for (k, v) in &store.kv {
                    if let Some(ref f) = filter {
                        if !k.contains(f) {
                            continue;
                        }
                    }
                    out.push(v.clone());
                }
                Ok(NbValue::new_list(out))
            }
            _ => Err(VmError::UndefinedCell(format!("{}.{}", owner, method))),
        }
    }

    fn machine_state_value(owner: &str, state: &MachineRuntime) -> NbValue {
        let mut fields = BTreeMap::new();
        fields.insert("name".to_string(), NbValue::new_str(&state.current_state));
        fields.insert("steps".to_string(), NbValue::new_int(state.steps as i64));
        fields.insert("terminal".to_string(), NbValue::new_bool(state.terminal));
        fields.insert(
            "payload".to_string(),
            NbValue::new_map(state.payload.clone()),
        );
        NbValue::new_record(&format!("{}.State", owner), fields)
    }

    fn bind_machine_payload(
        params: &[MachineParamDef],
        values: &[NbValue],
    ) -> BTreeMap<String, NbValue> {
        let mut payload = BTreeMap::new();
        for (idx, param) in params.iter().enumerate() {
            let value = values.get(idx).copied().unwrap_or(NbValue::new_null());
            let value = match param.ty.as_str() {
                "Int" => value
                    .as_int()
                    .map(NbValue::new_int)
                    .unwrap_or(NbValue::new_null()),
                "Float" => value
                    .as_float()
                    .map(NbValue::new_float)
                    .unwrap_or(NbValue::new_null()),
                "Bool" => value
                    .as_bool()
                    .map(NbValue::new_bool)
                    .unwrap_or(NbValue::new_null()),
                "String" => match value.as_heap_ref() {
                    Some(HeapValue::Str(_)) => value,
                    _ => NbValue::new_null(),
                },
                "Null" => NbValue::new_null(),
                _ => value,
            };
            payload.insert(param.name.clone(), value);
        }
        payload
    }

    fn eval_machine_expr(
        expr: &MachineExpr,
        payload: &BTreeMap<String, NbValue>,
    ) -> Result<NbValue, VmError> {
        match expr {
            MachineExpr::Int(n) => Ok(NbValue::new_int(*n)),
            MachineExpr::Float(f) => Ok(NbValue::new_float(*f)),
            MachineExpr::String(s) => Ok(NbValue::new_str(s)),
            MachineExpr::Bool(b) => Ok(NbValue::new_bool(*b)),
            MachineExpr::Null => Ok(NbValue::new_null()),
            MachineExpr::Ident(name) => {
                Ok(payload.get(name).copied().unwrap_or(NbValue::new_null()))
            }
            MachineExpr::Unary { op, expr } => {
                let value = Self::eval_machine_expr(expr, payload)?;
                match op.as_str() {
                    "-" => match (value.as_int(), value.as_float()) {
                        (Some(n), _) => Ok(NbValue::new_int(
                            n.checked_neg()
                                .ok_or(VmError::ArithmeticOverflow("negation".to_string()))?,
                        )),
                        (_, Some(f)) => Ok(NbValue::new_float(-f)),
                        _ => Ok(NbValue::new_null()),
                    },
                    "not" => Ok(NbValue::new_bool(!value.is_truthy())),
                    "~" => match value.as_int() {
                        Some(n) => Ok(NbValue::new_int(!n)),
                        None => Ok(NbValue::new_null()),
                    },
                    _ => Ok(NbValue::new_null()),
                }
            }
            MachineExpr::Bin { op, lhs, rhs } => {
                let left = Self::eval_machine_expr(lhs, payload)?;
                let right = Self::eval_machine_expr(rhs, payload)?;
                match op.as_str() {
                    "+" => match (
                        left.as_int(),
                        left.as_float(),
                        right.as_int(),
                        right.as_float(),
                    ) {
                        (Some(a), _, Some(b), _) => Ok(NbValue::new_int(
                            a.checked_add(b)
                                .ok_or(VmError::ArithmeticOverflow("addition".to_string()))?,
                        )),
                        (_, Some(a), _, Some(b)) => Ok(NbValue::new_float(a + b)),
                        (Some(a), _, _, Some(b)) => Ok(NbValue::new_float(a as f64 + b)),
                        (_, Some(a), Some(b), _) => Ok(NbValue::new_float(a + b as f64)),
                        _ => Ok(NbValue::new_null()),
                    },
                    "-" => match (
                        left.as_int(),
                        left.as_float(),
                        right.as_int(),
                        right.as_float(),
                    ) {
                        (Some(a), _, Some(b), _) => Ok(NbValue::new_int(
                            a.checked_sub(b)
                                .ok_or(VmError::ArithmeticOverflow("subtraction".to_string()))?,
                        )),
                        (_, Some(a), _, Some(b)) => Ok(NbValue::new_float(a - b)),
                        (Some(a), _, _, Some(b)) => Ok(NbValue::new_float(a as f64 - b)),
                        (_, Some(a), Some(b), _) => Ok(NbValue::new_float(a - b as f64)),
                        _ => Ok(NbValue::new_null()),
                    },
                    "*" => match (
                        left.as_int(),
                        left.as_float(),
                        right.as_int(),
                        right.as_float(),
                    ) {
                        (Some(a), _, Some(b), _) => Ok(NbValue::new_int(
                            a.checked_mul(b)
                                .ok_or(VmError::ArithmeticOverflow("multiplication".to_string()))?,
                        )),
                        (_, Some(a), _, Some(b)) => Ok(NbValue::new_float(a * b)),
                        (Some(a), _, _, Some(b)) => Ok(NbValue::new_float(a as f64 * b)),
                        (_, Some(a), Some(b), _) => Ok(NbValue::new_float(a * b as f64)),
                        _ => Ok(NbValue::new_null()),
                    },
                    "/" => match (
                        left.as_int(),
                        left.as_float(),
                        right.as_int(),
                        right.as_float(),
                    ) {
                        (Some(_), _, Some(0), _) => Err(VmError::DivisionByZero),
                        (Some(a), _, Some(b), _) => Ok(NbValue::new_int(
                            a.checked_div(b)
                                .ok_or(VmError::ArithmeticOverflow("division".to_string()))?,
                        )),
                        (_, Some(a), _, Some(b)) => Ok(NbValue::new_float(a / b)),
                        (Some(a), _, _, Some(b)) => Ok(NbValue::new_float(a as f64 / b)),
                        (_, Some(a), Some(b), _) => Ok(NbValue::new_float(a / b as f64)),
                        _ => Ok(NbValue::new_null()),
                    },
                    "%" => match (left.as_int(), right.as_int()) {
                        (Some(_), Some(0)) => Err(VmError::DivisionByZero),
                        (Some(a), Some(b)) => Ok(NbValue::new_int(
                            a.checked_rem(b)
                                .ok_or(VmError::ArithmeticOverflow("remainder".to_string()))?,
                        )),
                        _ => Ok(NbValue::new_null()),
                    },
                    "==" => Ok(NbValue::new_bool(left == right)),
                    "!=" => Ok(NbValue::new_bool(left != right)),
                    "<" => Ok(NbValue::new_bool(left < right)),
                    "<=" => Ok(NbValue::new_bool(left <= right)),
                    ">" => Ok(NbValue::new_bool(left > right)),
                    ">=" => Ok(NbValue::new_bool(left >= right)),
                    "and" => Ok(NbValue::new_bool(left.is_truthy() && right.is_truthy())),
                    "or" => Ok(NbValue::new_bool(left.is_truthy() || right.is_truthy())),
                    _ => Ok(NbValue::new_null()),
                }
            }
        }
    }

    fn call_machine_method(
        &mut self,
        owner: &str,
        method: &str,
        base: usize,
        a: usize,
        nargs: usize,
    ) -> Result<NbValue, VmError> {
        let args: Vec<NbValue> = (0..nargs)
            .map(|i| self.registers[base + a + 1 + i])
            .collect();
        let instance_id = helpers::process_instance_id(args.first()).ok_or_else(|| {
            VmError::TypeError(format!(
                "{}.{} requires a process instance as the first argument",
                owner, method
            ))
        })?;
        let graph = self.machine_graphs.get(owner).cloned();
        let state = self.machine_runtime.entry(instance_id).or_default();
        match method {
            "start" => {
                state.started = true;
                state.steps = 0;
                if let Some(graph) = graph.as_ref() {
                    state.current_state = if graph.initial.is_empty() {
                        "started".to_string()
                    } else {
                        graph.initial.clone()
                    };
                    if let Some(state_def) = graph.states.get(&state.current_state) {
                        state.payload = Self::bind_machine_payload(&state_def.params, &args[1..]);
                    } else {
                        state.payload.clear();
                    }
                    state.terminal = graph
                        .states
                        .get(&state.current_state)
                        .map(|s| s.terminal)
                        .unwrap_or(false);
                } else {
                    state.terminal = false;
                    state.current_state = "started".to_string();
                    state.payload.clear();
                }
                Ok(NbValue::new_null())
            }
            "step" => {
                if !state.started {
                    state.started = true;
                    if let Some(graph) = graph.as_ref() {
                        state.current_state = if graph.initial.is_empty() {
                            "started".to_string()
                        } else {
                            graph.initial.clone()
                        };
                        if let Some(state_def) = graph.states.get(&state.current_state) {
                            state.payload =
                                Self::bind_machine_payload(&state_def.params, &args[1..]);
                        }
                    }
                }
                state.steps += 1;
                if let Some(graph) = graph.as_ref() {
                    let current = state.current_state.clone();
                    let mut next_payload = None;
                    if let Some(def) = graph.states.get(&current) {
                        let guard_ok = def
                            .guard
                            .as_ref()
                            .map(|expr| {
                                Self::eval_machine_expr(expr, &state.payload)
                                    .map(|value| value.is_truthy())
                            })
                            .transpose()?
                            .unwrap_or(true);
                        if guard_ok {
                            if let Some(next) = &def.transition_to {
                                if let Some(next_def) = graph.states.get(next) {
                                    let evaluated: Vec<NbValue> = def
                                        .transition_args
                                        .iter()
                                        .map(|expr| Self::eval_machine_expr(expr, &state.payload))
                                        .collect::<Result<Vec<_>, _>>()?;
                                    next_payload = Some(Self::bind_machine_payload(
                                        &next_def.params,
                                        &evaluated,
                                    ));
                                }
                                state.current_state = next.clone();
                            }
                        }
                    }
                    if let Some(payload) = next_payload {
                        state.payload = payload;
                    }
                    state.terminal = graph
                        .states
                        .get(&state.current_state)
                        .map(|s| s.terminal)
                        .unwrap_or(false);
                } else if state.steps >= 1 {
                    state.terminal = true;
                    state.current_state = "terminal".to_string();
                } else {
                    state.current_state = format!("step_{}", state.steps);
                }
                Ok(Self::machine_state_value(owner, state))
            }
            "is_terminal" => Ok(NbValue::new_bool(state.terminal)),
            "current_state" => Ok(Self::machine_state_value(owner, state)),
            "run" => {
                state.started = true;
                if let Some(graph) = graph.as_ref() {
                    if state.current_state.is_empty() {
                        state.current_state = graph.initial.clone();
                    }
                    if state.current_state.is_empty() {
                        state.current_state = "started".to_string();
                    }
                    if state.payload.is_empty() {
                        if let Some(state_def) = graph.states.get(&state.current_state) {
                            state.payload =
                                Self::bind_machine_payload(&state_def.params, &args[1..]);
                        }
                    }
                    let mut guard = 0usize;
                    let max_steps = graph.states.len().saturating_mul(4).max(1);
                    while guard < max_steps {
                        guard += 1;
                        state.steps += 1;
                        let Some(def) = graph.states.get(&state.current_state).cloned() else {
                            break;
                        };
                        let guard_ok = def
                            .guard
                            .as_ref()
                            .map(|expr| {
                                Self::eval_machine_expr(expr, &state.payload)
                                    .map(|value| value.is_truthy())
                            })
                            .transpose()?
                            .unwrap_or(true);
                        if !guard_ok {
                            break;
                        }
                        state.terminal = def.terminal;
                        if state.terminal {
                            break;
                        }
                        if let Some(next) = def.transition_to {
                            if let Some(next_def) = graph.states.get(&next) {
                                let evaluated: Vec<NbValue> = def
                                    .transition_args
                                    .iter()
                                    .map(|expr| Self::eval_machine_expr(expr, &state.payload))
                                    .collect::<Result<Vec<_>, _>>()?;
                                state.payload =
                                    Self::bind_machine_payload(&next_def.params, &evaluated);
                            }
                            state.current_state = next;
                        } else {
                            break;
                        }
                    }
                    state.terminal = graph
                        .states
                        .get(&state.current_state)
                        .map(|s| s.terminal)
                        .unwrap_or(state.terminal);
                } else {
                    state.steps += 1;
                    state.terminal = true;
                    state.current_state = "terminal".to_string();
                }
                Ok(Self::machine_state_value(owner, state))
            }
            "resume_from" => {
                state.started = true;
                state.steps = 0;
                let target = args
                    .get(1)
                    .map(|v| v.display())
                    .filter(|s| !s.is_empty())
                    .or_else(|| graph.as_ref().map(|g| g.initial.clone()))
                    .unwrap_or_else(|| "resumed".to_string());
                state.current_state = target;
                if let Some(graph) = graph.as_ref() {
                    if let Some(state_def) = graph.states.get(&state.current_state) {
                        state.payload = Self::bind_machine_payload(&state_def.params, &args[2..]);
                    } else {
                        state.payload.clear();
                    }
                    state.terminal = graph
                        .states
                        .get(&state.current_state)
                        .map(|s| s.terminal)
                        .unwrap_or(false);
                } else {
                    state.terminal = false;
                }
                Ok(Self::machine_state_value(owner, state))
            }
            _ => Err(VmError::UndefinedCell(format!("{}.{}", owner, method))),
        }
    }

    pub(crate) fn orchestration_args(&self, base: usize, a: usize, nargs: usize) -> Vec<NbValue> {
        if nargs == 1 {
            let first = self.registers[base + a + 1];
            if let Some(HeapValue::List(items)) = first.as_heap_ref() {
                return (**items).clone();
            }
            return vec![first];
        }
        (0..nargs)
            .map(|i| self.registers[base + a + 1 + i])
            .collect()
    }
}
