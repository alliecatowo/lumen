//! AST → LIR lowering. Converts typed AST to LIR instructions.

use crate::compiler::ast::*;
use crate::compiler::lir::*;
use crate::compiler::regalloc::RegAlloc;
use crate::compiler::resolve::SymbolTable;
use crate::compiler::tokens::Span;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

fn collect_effect_tool_bindings(program: &Program) -> HashMap<String, String> {
    let mut bindings = HashMap::new();
    for item in &program.items {
        if let Item::EffectBind(bind) = item {
            bindings
                .entry(bind.effect_path.clone())
                .or_insert(bind.tool_alias.clone());
        }
    }
    bindings
}

fn collect_effect_handler_cells(program: &Program) -> HashMap<String, String> {
    let mut handlers = HashMap::new();
    for item in &program.items {
        if let Item::Handler(handler) = item {
            for handle in &handler.handles {
                handlers.entry(handle.name.clone()).or_insert_with(|| {
                    format!("{}.handle_{}", handler.name, handle.name.replace('.', "_"))
                });
            }
        }
    }
    handlers
}

fn effect_operation_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::DotAccess(obj, field, _) => {
            if let Expr::Ident(owner, _) = obj.as_ref() {
                Some(format!("{}.{}", owner, field))
            } else {
                None
            }
        }
        Expr::Ident(name, _) if name.contains('.') => Some(name.clone()),
        _ => None,
    }
}

fn desugar_pipe_application(input: &Expr, stage: &Expr, span: Span) -> Expr {
    match stage {
        Expr::Call(callee, args, call_span) => {
            let mut call_args = Vec::with_capacity(args.len() + 1);
            call_args.push(CallArg::Positional(input.clone()));
            call_args.extend(args.clone());
            Expr::Call(callee.clone(), call_args, *call_span)
        }
        Expr::ToolCall(callee, args, call_span) => {
            let mut call_args = Vec::with_capacity(args.len() + 1);
            call_args.push(CallArg::Positional(input.clone()));
            call_args.extend(args.clone());
            Expr::ToolCall(callee.clone(), call_args, *call_span)
        }
        _ => Expr::Call(
            Box::new(stage.clone()),
            vec![CallArg::Positional(input.clone())],
            span,
        ),
    }
}

/// Lower an entire program to a LIR module.
pub fn lower(program: &Program, symbols: &SymbolTable, source: &str) -> LirModule {
    let doc_hash = format!("sha256:{:x}", Sha256::digest(source.as_bytes()));
    let mut module = LirModule::new(doc_hash);
    let mut lowerer = Lowerer::new(
        symbols,
        collect_effect_tool_bindings(program),
        collect_effect_handler_cells(program),
    );

    for d in &program.directives {
        let name = match &d.value {
            Some(v) => format!("{}={}", d.name, v),
            None => d.name.clone(),
        };
        module.addons.push(LirAddon {
            kind: "directive".to_string(),
            name: Some(name),
        });
    }

    let mut tool_aliases: Vec<String> = symbols.tools.keys().cloned().collect();
    tool_aliases.sort();
    for alias in tool_aliases {
        if let Some(tool) = symbols.tools.get(&alias) {
            module.tools.push(LirTool {
                alias: alias.clone(),
                tool_id: tool.tool_path.clone(),
                version: "1.0.0".to_string(),
                mcp_url: tool.mcp_url.clone(),
            });
        }
    }

    // Lower types
    for item in &program.items {
        match item {
            Item::Record(r) => module.types.push(lowerer.lower_record(r)),
            Item::Enum(e) => module.types.push(lowerer.lower_enum(e)),
            Item::Cell(c) => module.cells.push(lowerer.lower_cell(c)),
            Item::Agent(a) => {
                module.types.push(lowerer.lower_agent_type(a));
                module.cells.push(lowerer.lower_agent_constructor(a));
                module.agents.push(LirAgent {
                    name: a.name.clone(),
                    methods: a.cells.iter().map(|c| c.name.clone()).collect(),
                });
                for cell in &a.cells {
                    let mut method = cell.clone();
                    method.name = format!("{}.{}", a.name, cell.name);
                    module.cells.push(lowerer.lower_cell(&method));
                }
                for g in &a.grants {
                    let mut grants = serde_json::Map::new();
                    for c in &g.constraints {
                        let val = match &c.value {
                            Expr::StringLit(s, _) => serde_json::Value::String(s.clone()),
                            Expr::IntLit(n, _) => serde_json::json!(*n),
                            Expr::FloatLit(f, _) => serde_json::json!(*f),
                            Expr::BoolLit(b, _) => serde_json::json!(*b),
                            _ => serde_json::Value::Null,
                        };
                        grants.insert(c.key.clone(), val);
                    }
                    module.policies.push(LirPolicy {
                        tool_alias: g.tool_alias.clone(),
                        grants: serde_json::Value::Object(grants),
                    });
                }
            }
            Item::Process(p) => {
                module.types.push(lowerer.lower_process_type(p));
                module.cells.push(lowerer.lower_process_constructor(p));
                module.addons.push(LirAddon {
                    kind: p.kind.clone(),
                    name: Some(p.name.clone()),
                });
                if p.kind == "machine" && !p.machine_states.is_empty() {
                    let initial = p
                        .machine_initial
                        .clone()
                        .or_else(|| p.machine_states.first().map(|s| s.name.clone()))
                        .unwrap_or_default();
                    module.addons.push(LirAddon {
                        kind: "machine.initial".to_string(),
                        name: Some(format!("{}={}", p.name, initial)),
                    });
                    for state in &p.machine_states {
                        let params: Vec<serde_json::Value> = state
                            .params
                            .iter()
                            .map(|param| {
                                serde_json::json!({
                                    "name": param.name,
                                    "type": format_type_expr(&param.ty),
                                })
                            })
                            .collect();
                        let guard = state.guard.as_ref().and_then(encode_machine_expr);
                        let transition_args: Vec<serde_json::Value> = state
                            .transition_args
                            .iter()
                            .filter_map(encode_machine_expr)
                            .collect();
                        let payload = serde_json::json!({
                            "machine": p.name,
                            "state": state.name,
                            "terminal": state.terminal,
                            "transition_to": state.transition_to,
                            "params": params,
                            "guard": guard,
                            "transition_args": transition_args,
                        });
                        module.addons.push(LirAddon {
                            kind: "machine.state".to_string(),
                            name: Some(payload.to_string()),
                        });
                    }
                }
                for cell in &p.cells {
                    let mut lowered = cell.clone();
                    lowered.name = format!("{}.{}", p.name, cell.name);
                    module.cells.push(lowerer.lower_cell(&lowered));
                }
                if p.kind == "pipeline"
                    && !p.pipeline_stages.is_empty()
                    && !p.cells.iter().any(|c| c.name == "run")
                {
                    let span = p.span;
                    let value_name = "__pipeline_value".to_string();
                    let mut body = vec![Stmt::Let(LetStmt {
                        name: value_name.clone(),
                        mutable: true,
                        pattern: None,
                        ty: Some(TypeExpr::Named("Any".to_string(), span)),
                        value: Expr::Ident("input".to_string(), span),
                        span,
                    })];
                    for stage in &p.pipeline_stages {
                        let callee = pipeline_stage_callee_expr(stage, span);
                        let call = Expr::Call(
                            Box::new(callee),
                            vec![CallArg::Positional(Expr::Ident(value_name.clone(), span))],
                            span,
                        );
                        body.push(Stmt::Assign(AssignStmt {
                            target: value_name.clone(),
                            value: call,
                            span,
                        }));
                    }
                    body.push(Stmt::Return(ReturnStmt {
                        value: Expr::Ident(value_name, span),
                        span,
                    }));
                    let generated = CellDef {
                        name: format!("{}.run", p.name),
                        generic_params: vec![],
                        params: vec![
                            Param {
                                name: "self".to_string(),
                                ty: TypeExpr::Named("Json".to_string(), span),
                                default_value: None,
                                variadic: false,
                                span,
                            },
                            Param {
                                name: "input".to_string(),
                                ty: TypeExpr::Named("Any".to_string(), span),
                                default_value: None,
                                variadic: false,
                                span,
                            },
                        ],
                        return_type: Some(TypeExpr::Named("Any".to_string(), span)),
                        effects: vec![],
                        body,
                        is_pub: false,
                        is_async: false,
                        where_clauses: vec![],
                        span,
                    };
                    module.cells.push(lowerer.lower_cell(&generated));
                }
                for g in &p.grants {
                    let mut grants = serde_json::Map::new();
                    for c in &g.constraints {
                        let val = match &c.value {
                            Expr::StringLit(s, _) => serde_json::Value::String(s.clone()),
                            Expr::IntLit(n, _) => serde_json::json!(*n),
                            Expr::FloatLit(f, _) => serde_json::json!(*f),
                            Expr::BoolLit(b, _) => serde_json::json!(*b),
                            _ => serde_json::Value::Null,
                        };
                        grants.insert(c.key.clone(), val);
                    }
                    module.policies.push(LirPolicy {
                        tool_alias: g.tool_alias.clone(),
                        grants: serde_json::Value::Object(grants),
                    });
                }
            }
            Item::Effect(e) => module.effects.push(lowerer.lower_effect(e)),
            Item::EffectBind(b) => module.effect_binds.push(LirEffectBind {
                effect_path: b.effect_path.clone(),
                tool_alias: b.tool_alias.clone(),
            }),
            Item::Handler(h) => {
                module
                    .handlers
                    .push(lowerer.lower_handler(h, &mut module.cells));
            }
            Item::Addon(a) => module.addons.push(LirAddon {
                kind: a.kind.clone(),
                name: a.name.clone(),
            }),
            Item::UseTool(_) => {}
            Item::Grant(g) => {
                let mut grants = serde_json::Map::new();
                for c in &g.constraints {
                    let val = match &c.value {
                        Expr::StringLit(s, _) => serde_json::Value::String(s.clone()),
                        Expr::IntLit(n, _) => serde_json::json!(*n),
                        Expr::FloatLit(f, _) => serde_json::json!(*f),
                        Expr::BoolLit(b, _) => serde_json::json!(*b),
                        _ => serde_json::Value::Null,
                    };
                    grants.insert(c.key.clone(), val);
                }
                module.policies.push(LirPolicy {
                    tool_alias: g.tool_alias.clone(),
                    grants: serde_json::Value::Object(grants),
                });
            }
            Item::TypeAlias(ta) => module.addons.push(LirAddon {
                kind: "type_alias".into(),
                name: Some(ta.name.clone()),
            }),
            Item::Trait(t) => module.addons.push(LirAddon {
                kind: "trait".into(),
                name: Some(t.name.clone()),
            }),
            Item::Impl(i) => module.addons.push(LirAddon {
                kind: "impl".into(),
                name: Some(format!("{} for {}", i.trait_name, i.target_type)),
            }),
            Item::Import(i) => module.addons.push(LirAddon {
                kind: "import".into(),
                name: Some(i.path.join(".")),
            }),
            Item::ConstDecl(c) => module.addons.push(LirAddon {
                kind: "const".into(),
                name: Some(c.name.clone()),
            }),
            Item::MacroDecl(m) => module.addons.push(LirAddon {
                kind: "macro_decl".into(),
                name: Some(m.name.clone()),
            }),
        }
    }

    // Lambda cells are appended after top-level cells. Patch closure opcodes
    // to absolute module cell indices before we append.
    let lambda_base = module.cells.len() as u16;
    for cell in &mut module.cells {
        patch_lambda_closure_indices(cell, lambda_base);
    }
    for cell in &mut lowerer.lambda_cells {
        patch_lambda_closure_indices(cell, lambda_base);
    }
    module.cells.append(&mut lowerer.lambda_cells);

    // Collect string table
    module.strings = lowerer.strings;
    module
}

/// Tracks a loop for break/continue patching
struct LoopContext {
    label: Option<String>,
    start: usize,
    break_jumps: Vec<usize>,
}

struct Lowerer<'a> {
    symbols: &'a SymbolTable,
    tool_indices: HashMap<String, u16>,
    effect_tool_bindings: HashMap<String, String>,
    effect_handler_cells: HashMap<String, String>,
    strings: Vec<String>,
    loop_stack: Vec<LoopContext>,
    lambda_cells: Vec<LirCell>,
    /// Accumulated defer blocks for the current function scope (emitted in LIFO order before returns)
    defer_stack: Vec<Vec<Stmt>>,
}

impl<'a> Lowerer<'a> {
    fn new(
        symbols: &'a SymbolTable,
        effect_tool_bindings: HashMap<String, String>,
        effect_handler_cells: HashMap<String, String>,
    ) -> Self {
        let mut tool_aliases: Vec<String> = symbols.tools.keys().cloned().collect();
        tool_aliases.sort();
        let tool_indices = tool_aliases
            .into_iter()
            .enumerate()
            .map(|(idx, alias)| (alias, idx as u16))
            .collect();
        Self {
            symbols,
            tool_indices,
            effect_tool_bindings,
            effect_handler_cells,
            strings: Vec::new(),
            loop_stack: Vec::new(),
            lambda_cells: Vec::new(),
            defer_stack: Vec::new(),
        }
    }

    fn intern_string(&mut self, s: &str) -> u16 {
        if let Some(idx) = self.strings.iter().position(|x| x == s) {
            idx as u16
        } else {
            let idx = self.strings.len() as u16;
            self.strings.push(s.to_string());
            idx
        }
    }

    fn resolve_effect_tool_binding<'b>(&'b self, effect_path: &'b str) -> Option<&'b str> {
        if let Some(alias) = self.effect_tool_bindings.get(effect_path) {
            return Some(alias.as_str());
        }
        let mut prefix = effect_path;
        while let Some((head, _)) = prefix.rsplit_once('.') {
            if let Some(alias) = self.effect_tool_bindings.get(head) {
                return Some(alias.as_str());
            }
            prefix = head;
        }
        None
    }

    fn lower_call_arg_regs(
        &mut self,
        args: &[CallArg],
        implicit_self_arg: Option<u8>,
        ra: &mut RegAlloc,
        consts: &mut Vec<Constant>,
        instrs: &mut Vec<Instruction>,
    ) -> Vec<u8> {
        let mut arg_regs = Vec::new();
        if let Some(self_reg) = implicit_self_arg {
            arg_regs.push(self_reg);
        }
        for arg in args {
            match arg {
                CallArg::Positional(e) | CallArg::Named(_, e, _) => {
                    arg_regs.push(self.lower_expr(e, ra, consts, instrs));
                }
                CallArg::Role(name, content, _) => {
                    let content_reg = self.lower_expr(content, ra, consts, instrs);
                    let prefix_reg = ra.alloc_temp();
                    let kidx = consts.len() as u16;
                    consts.push(Constant::String(format!("{}: ", name)));
                    instrs.push(Instruction::abx(OpCode::LoadK, prefix_reg, kidx));
                    let dest = ra.alloc_temp();
                    instrs.push(Instruction::abc(
                        OpCode::Concat,
                        dest,
                        prefix_reg,
                        content_reg,
                    ));
                    arg_regs.push(dest);
                }
            }
        }
        arg_regs
    }

    fn emit_call_with_regs(
        &mut self,
        callee_reg: u8,
        arg_regs: &[u8],
        ra: &mut RegAlloc,
        instrs: &mut Vec<Instruction>,
    ) -> u8 {
        let base = ra.alloc_temp();
        if callee_reg != base {
            instrs.push(Instruction::abc(OpCode::Move, base, callee_reg, 0));
        }

        for (i, &reg) in arg_regs.iter().enumerate() {
            let target = base + 1 + i as u8;
            ra.alloc_temp();
            if reg != target {
                instrs.push(Instruction::abc(OpCode::Move, target, reg, 0));
            }
        }

        let result_reg = ra.alloc_temp();
        instrs.push(Instruction::abc(
            OpCode::Call,
            base,
            arg_regs.len() as u8,
            1,
        ));
        instrs.push(Instruction::abc(OpCode::Move, result_reg, base, 0));
        result_reg
    }

    fn lower_named_call_target(
        &mut self,
        callee_name: &str,
        args: &[CallArg],
        implicit_self_arg: Option<u8>,
        ra: &mut RegAlloc,
        consts: &mut Vec<Constant>,
        instrs: &mut Vec<Instruction>,
    ) -> u8 {
        let callee_reg = ra.alloc_temp();
        let callee_idx = consts.len() as u16;
        consts.push(Constant::String(callee_name.to_string()));
        instrs.push(Instruction::abx(OpCode::LoadK, callee_reg, callee_idx));
        let arg_regs = self.lower_call_arg_regs(args, implicit_self_arg, ra, consts, instrs);
        self.emit_call_with_regs(callee_reg, &arg_regs, ra, instrs)
    }

    fn lower_record(&mut self, r: &RecordDef) -> LirType {
        self.intern_string(&r.name);
        LirType {
            kind: "record".to_string(),
            name: r.name.clone(),
            fields: r
                .fields
                .iter()
                .map(|f| {
                    self.intern_string(&f.name);
                    LirField {
                        name: f.name.clone(),
                        ty: format_type_expr(&f.ty),
                        constraints: if f.constraint.is_some() {
                            vec!["has_constraint".into()]
                        } else {
                            vec![]
                        },
                    }
                })
                .collect(),
            variants: vec![],
        }
    }

    fn lower_enum(&mut self, e: &EnumDef) -> LirType {
        self.intern_string(&e.name);
        LirType {
            kind: "enum".to_string(),
            name: e.name.clone(),
            fields: vec![],
            variants: e
                .variants
                .iter()
                .map(|v| {
                    self.intern_string(&v.name);
                    LirVariant {
                        name: v.name.clone(),
                        payload: v.payload.as_ref().map(format_type_expr),
                    }
                })
                .collect(),
        }
    }

    fn lower_agent_type(&mut self, a: &AgentDecl) -> LirType {
        self.intern_string(&a.name);
        LirType {
            kind: "record".to_string(),
            name: a.name.clone(),
            fields: a
                .cells
                .iter()
                .map(|c| {
                    self.intern_string(&c.name);
                    LirField {
                        name: c.name.clone(),
                        ty: "String".to_string(),
                        constraints: vec![],
                    }
                })
                .collect(),
            variants: vec![],
        }
    }

    fn process_runtime_method_names<'b>(&self, p: &'b ProcessDecl) -> Vec<&'b str> {
        let mut methods: Vec<&'b str> = p.cells.iter().map(|c| c.name.as_str()).collect();
        match p.kind.as_str() {
            "memory" => {
                methods.extend([
                    "append", "recent", "remember", "recall", "upsert", "get", "query", "store",
                ]);
            }
            "machine" => {
                methods.extend([
                    "run",
                    "start",
                    "step",
                    "is_terminal",
                    "current_state",
                    "resume_from",
                ]);
            }
            "pipeline" | "orchestration" => {
                methods.push("run");
            }
            _ => {}
        }
        methods.sort_unstable();
        methods.dedup();
        methods
    }

    fn lower_process_type(&mut self, p: &ProcessDecl) -> LirType {
        self.intern_string(&p.name);
        let methods = self.process_runtime_method_names(p);
        LirType {
            kind: "record".to_string(),
            name: p.name.clone(),
            fields: methods
                .iter()
                .map(|name| {
                    self.intern_string(name);
                    LirField {
                        name: (*name).to_string(),
                        ty: "String".to_string(),
                        constraints: vec![],
                    }
                })
                .collect(),
            variants: vec![],
        }
    }

    fn lower_effect(&mut self, e: &EffectDecl) -> LirEffect {
        self.intern_string(&e.name);
        LirEffect {
            name: e.name.clone(),
            operations: e
                .operations
                .iter()
                .map(|op| LirEffectOp {
                    name: op.name.clone(),
                    params: op
                        .params
                        .iter()
                        .map(|p| LirParam {
                            name: p.name.clone(),
                            ty: format_type_expr(&p.ty),
                            register: 0,
                            variadic: false,
                        })
                        .collect(),
                    returns: op.return_type.as_ref().map(format_type_expr),
                    effects: op.effects.clone(),
                })
                .collect(),
        }
    }

    fn lower_handler(&mut self, h: &HandlerDecl, cells: &mut Vec<LirCell>) -> LirHandler {
        self.intern_string(&h.name);
        let mut handles = Vec::new();
        for handle in &h.handles {
            let mut lowered = handle.clone();
            let sanitized_op = handle.name.replace('.', "_");
            lowered.name = format!("{}.handle_{}", h.name, sanitized_op);
            let lowered_name = lowered.name.clone();
            cells.push(self.lower_cell(&lowered));
            handles.push(LirHandle {
                operation: handle.name.clone(),
                cell: lowered_name,
            });
        }
        LirHandler {
            name: h.name.clone(),
            handles,
        }
    }

    fn lower_process_constructor(&mut self, p: &ProcessDecl) -> LirCell {
        self.intern_string(&p.name);
        let mut ra = RegAlloc::new();
        let mut constants: Vec<Constant> = Vec::new();
        let mut instructions: Vec<Instruction> = Vec::new();

        let dest = ra.alloc_temp();
        let type_idx = self.intern_string(&p.name);
        instructions.push(Instruction::abx(OpCode::NewRecord, dest, type_idx));

        for method in self.process_runtime_method_names(p) {
            let val_reg = ra.alloc_temp();
            let kidx = constants.len() as u16;
            constants.push(Constant::String(format!("{}.{}", p.name, method)));
            instructions.push(Instruction::abx(OpCode::LoadK, val_reg, kidx));
            self.emit_set_field(
                dest,
                method,
                val_reg,
                &mut ra,
                &mut constants,
                &mut instructions,
            );
        }

        instructions.push(Instruction::abc(OpCode::Return, dest, 1, 0));

        LirCell {
            name: p.name.clone(),
            params: vec![],
            returns: Some(p.name.clone()),
            registers: ra.max_regs(),
            constants,
            instructions,
        }
    }

    fn lower_agent_constructor(&mut self, a: &AgentDecl) -> LirCell {
        self.intern_string(&a.name);
        let mut ra = RegAlloc::new();
        let mut constants: Vec<Constant> = Vec::new();
        let mut instructions: Vec<Instruction> = Vec::new();

        let dest = ra.alloc_temp();
        let type_idx = self.intern_string(&a.name);
        instructions.push(Instruction::abx(OpCode::NewRecord, dest, type_idx));

        for cell in &a.cells {
            let val_reg = ra.alloc_temp();
            let kidx = constants.len() as u16;
            constants.push(Constant::String(format!("{}.{}", a.name, cell.name)));
            instructions.push(Instruction::abx(OpCode::LoadK, val_reg, kidx));
            self.emit_set_field(
                dest,
                &cell.name,
                val_reg,
                &mut ra,
                &mut constants,
                &mut instructions,
            );
        }

        instructions.push(Instruction::abc(OpCode::Return, dest, 1, 0));

        LirCell {
            name: a.name.clone(),
            params: vec![],
            returns: Some(a.name.clone()),
            registers: ra.max_regs(),
            constants,
            instructions,
        }
    }

    fn lower_cell(&mut self, cell: &CellDef) -> LirCell {
        self.intern_string(&cell.name);
        let mut ra = RegAlloc::new();
        let mut constants: Vec<Constant> = Vec::new();
        let mut instructions: Vec<Instruction> = Vec::new();

        // Save and reset defer stack for this cell scope
        let saved_defers = std::mem::take(&mut self.defer_stack);

        // Allocate param registers
        let params: Vec<LirParam> = cell
            .params
            .iter()
            .map(|p| {
                let reg = ra.alloc_named(&p.name);
                LirParam {
                    name: p.name.clone(),
                    ty: format_type_expr(&p.ty),
                    register: reg,
                    variadic: p.variadic,
                }
            })
            .collect();

        // Lower body with implicit return support
        let has_return_type = cell.return_type.is_some();
        let body_len = cell.body.len();
        for (idx, stmt) in cell.body.iter().enumerate() {
            let is_last = idx == body_len - 1;
            // Implicit return: if last statement is an expression and cell has a return type
            if is_last && has_return_type {
                if let Stmt::Expr(es) = stmt {
                    let val_reg =
                        self.lower_expr(&es.expr, &mut ra, &mut constants, &mut instructions);
                    // Emit accumulated defer blocks in LIFO order before return
                    self.emit_defers(&mut ra, &mut constants, &mut instructions);
                    instructions.push(Instruction::abc(OpCode::Return, val_reg, 1, 0));
                    continue;
                }
            }
            self.lower_stmt(stmt, &mut ra, &mut constants, &mut instructions);
        }

        // Ensure return at end
        if instructions.is_empty()
            || !matches!(
                instructions.last().map(|i| i.op),
                Some(OpCode::Return) | Some(OpCode::Halt)
            )
        {
            // Emit accumulated defer blocks in LIFO order before implicit return
            self.emit_defers(&mut ra, &mut constants, &mut instructions);
            let r = ra.alloc_temp();
            instructions.push(Instruction::abc(OpCode::LoadNil, r, 0, 0));
            instructions.push(Instruction::abc(OpCode::Return, r, 1, 0));
        }

        // Restore defer stack
        self.defer_stack = saved_defers;

        LirCell {
            name: cell.name.clone(),
            params,
            returns: cell.return_type.as_ref().map(format_type_expr),
            registers: ra.max_regs(),
            constants,
            instructions,
        }
    }

    fn lower_stmt(
        &mut self,
        stmt: &Stmt,
        ra: &mut RegAlloc,
        consts: &mut Vec<Constant>,
        instrs: &mut Vec<Instruction>,
    ) {
        match stmt {
            Stmt::Let(ls) => {
                let val_reg = self.lower_expr(&ls.value, ra, consts, instrs);
                if let Some(Pattern::TupleDestructure { elements, .. }) = &ls.pattern {
                    // Destructuring let: let (a, b, c) = expr
                    // Evaluate RHS into val_reg, then extract each element
                    for (i, pat) in elements.iter().enumerate() {
                        match pat {
                            Pattern::Ident(name, _) => {
                                let dest = ra.alloc_named(name);
                                instrs.push(Instruction::abc(
                                    OpCode::GetTuple,
                                    dest,
                                    val_reg,
                                    i as u8,
                                ));
                            }
                            Pattern::Wildcard(_) => {
                                // Skip extraction for wildcard patterns
                            }
                            _ => {
                                // For other patterns (nested destructuring etc.),
                                // extract to a temp and ignore for now
                                let temp = ra.alloc_temp();
                                instrs.push(Instruction::abc(
                                    OpCode::GetTuple,
                                    temp,
                                    val_reg,
                                    i as u8,
                                ));
                            }
                        }
                    }
                } else {
                    let dest = ra.alloc_named(&ls.name);
                    if dest != val_reg {
                        instrs.push(Instruction::abc(OpCode::Move, dest, val_reg, 0));
                    }
                }
            }
            Stmt::If(ifs) => {
                let cond_reg = self.lower_expr(&ifs.condition, ra, consts, instrs);
                // Compare with True
                let true_reg = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::LoadBool, true_reg, 1, 0));

                // Result register for Eq (fixes VM restriction on A=0 mode)
                let cmp_dest = ra.alloc_temp();

                // Skip if Equal (cond == true)
                // Eq(dest, cond, true) -> dest = (cond == true)
                instrs.push(Instruction::abc(OpCode::Eq, cmp_dest, cond_reg, true_reg));

                // If cmp_dest is FALSE, we want to JUMP to Else.
                // Test(cmp_dest, 0) -> If cmp_dest is Truthy (True) != 0 (False) -> True -> Skip Next.
                // So if True, Skip Jump.
                // If False, Don't Skip Jump -> Jump.
                instrs.push(Instruction::abc(OpCode::Test, cmp_dest, 0, 0));

                let jmp_idx = instrs.len();
                instrs.push(Instruction::sax(OpCode::Jmp, 0)); // Jump to else/end

                for s in &ifs.then_body {
                    self.lower_stmt(s, ra, consts, instrs);
                }

                if let Some(ref else_body) = ifs.else_body {
                    let else_jmp_idx = instrs.len();
                    instrs.push(Instruction::sax(OpCode::Jmp, 0)); // skip else
                    let else_start = instrs.len();
                    // Patch the conditional jump
                    let offset = (else_start - jmp_idx - 1) as i32;
                    instrs[jmp_idx] = Instruction::sax(OpCode::Jmp, offset);

                    for s in else_body {
                        self.lower_stmt(s, ra, consts, instrs);
                    }

                    let after_else = instrs.len();
                    let else_offset = (after_else - else_jmp_idx - 1) as i32;
                    instrs[else_jmp_idx] = Instruction::sax(OpCode::Jmp, else_offset);
                } else {
                    let after = instrs.len();
                    let offset = (after - jmp_idx - 1) as i32;
                    instrs[jmp_idx] = Instruction::sax(OpCode::Jmp, offset);
                }
            }
            Stmt::For(fs) => {
                let iter_reg = self.lower_expr(&fs.iter, ra, consts, instrs);
                let idx_reg = ra.alloc_temp();
                let len_reg = ra.alloc_temp();
                let elem_reg = ra.alloc_named(&fs.var);

                // idx = 0
                let zero_idx = consts.len() as u16;
                consts.push(Constant::Int(0));
                instrs.push(Instruction::abx(OpCode::LoadK, idx_reg, zero_idx));
                // len = length(iter)
                instrs.push(Instruction::abc(
                    OpCode::Intrinsic,
                    len_reg,
                    IntrinsicId::Length as u8,
                    iter_reg,
                ));

                let loop_start = instrs.len();
                // if idx >= len, break
                // Lt evaluates to True if idx < len.
                // We want to Continue (Skip Break) if idx < len.
                // Break if Not (idx < len).
                let lt_reg = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::Lt, lt_reg, idx_reg, len_reg));

                // If lt_reg is True, Skip Next (Break).
                // Test(lt, 0): Skip if True.
                instrs.push(Instruction::abc(OpCode::Test, lt_reg, 0, 0));
                let break_jmp = instrs.len();
                instrs.push(Instruction::sax(OpCode::Jmp, 0)); // placeholder

                // elem = iter[idx]
                instrs.push(Instruction::abc(
                    OpCode::GetIndex,
                    elem_reg,
                    iter_reg,
                    idx_reg,
                ));

                self.loop_stack.push(LoopContext {
                    label: fs.label.clone(),
                    start: loop_start,
                    break_jumps: Vec::new(),
                });

                // If there's a filter condition, skip the body if it's false
                if let Some(filter) = &fs.filter {
                    let cond_reg = self.lower_expr(filter, ra, consts, instrs);
                    let true_reg = ra.alloc_temp();
                    instrs.push(Instruction::abc(OpCode::LoadBool, true_reg, 1, 0));
                    let cmp_reg = ra.alloc_temp();
                    instrs.push(Instruction::abc(OpCode::Eq, cmp_reg, cond_reg, true_reg));
                    // If condition is false, skip body (jump to increment)
                    instrs.push(Instruction::abc(OpCode::Test, cmp_reg, 0, 0));
                    let skip_jmp = instrs.len();
                    instrs.push(Instruction::sax(OpCode::Jmp, 0)); // placeholder

                    for s in &fs.body {
                        self.lower_stmt(s, ra, consts, instrs);
                    }

                    // Patch skip jump to point past the body (to the increment)
                    let body_end = instrs.len();
                    instrs[skip_jmp] =
                        Instruction::sax(OpCode::Jmp, (body_end - skip_jmp - 1) as i32);
                } else {
                    for s in &fs.body {
                        self.lower_stmt(s, ra, consts, instrs);
                    }
                }

                // idx = idx + 1
                let one_idx = consts.len() as u16;
                consts.push(Constant::Int(1));
                let one_reg = ra.alloc_temp();
                instrs.push(Instruction::abx(OpCode::LoadK, one_reg, one_idx));
                instrs.push(Instruction::abc(OpCode::Add, idx_reg, idx_reg, one_reg));

                // Jump back to loop start (negative offset)
                let back_offset = loop_start as i32 - instrs.len() as i32 - 1;
                instrs.push(Instruction::sax(OpCode::Jmp, back_offset));

                // Patch break jump (forward offset, always positive)
                let after_loop = instrs.len();
                let break_offset = (after_loop - break_jmp - 1) as i32;
                instrs[break_jmp] = Instruction::sax(OpCode::Jmp, break_offset);

                // Patch any break jumps from the loop body
                let ctx = self.loop_stack.pop().unwrap();
                for bj in ctx.break_jumps {
                    instrs[bj] = Instruction::sax(OpCode::Jmp, (after_loop - bj - 1) as i32);
                }
            }
            Stmt::Match(ms) => {
                let subj_reg = self.lower_expr(&ms.subject, ra, consts, instrs);
                let mut end_jumps = Vec::new();

                for arm in &ms.arms {
                    let mut fail_jumps = Vec::new();
                    self.lower_match_pattern(
                        &arm.pattern,
                        subj_reg,
                        ra,
                        consts,
                        instrs,
                        &mut fail_jumps,
                    );

                    for s in &arm.body {
                        self.lower_stmt(s, ra, consts, instrs);
                    }
                    end_jumps.push(instrs.len());
                    instrs.push(Instruction::sax(OpCode::Jmp, 0));

                    let next_arm = instrs.len();
                    for j in fail_jumps {
                        instrs[j] = Instruction::sax(OpCode::Jmp, (next_arm - j - 1) as i32);
                    }
                }

                let end = instrs.len();
                for jmp_idx in end_jumps {
                    instrs[jmp_idx] = Instruction::sax(OpCode::Jmp, (end - jmp_idx - 1) as i32);
                }
            }
            Stmt::Return(rs) => {
                let val_reg = self.lower_expr(&rs.value, ra, consts, instrs);
                // Emit accumulated defer blocks in LIFO order before return
                self.emit_defers(ra, consts, instrs);
                instrs.push(Instruction::abc(OpCode::Return, val_reg, 1, 0));
            }
            Stmt::Halt(hs) => {
                let msg_reg = self.lower_expr(&hs.message, ra, consts, instrs);
                instrs.push(Instruction::abc(OpCode::Halt, msg_reg, 0, 0));
            }
            Stmt::Assign(asgn) => {
                let val_reg = self.lower_expr(&asgn.value, ra, consts, instrs);
                if let Some(dest) = ra.lookup(&asgn.target) {
                    if dest != val_reg {
                        instrs.push(Instruction::abc(OpCode::Move, dest, val_reg, 0));
                    }
                } else {
                    let dest = ra.alloc_named(&asgn.target);
                    if dest != val_reg {
                        instrs.push(Instruction::abc(OpCode::Move, dest, val_reg, 0));
                    }
                }
            }
            Stmt::Expr(es) => {
                self.lower_expr(&es.expr, ra, consts, instrs);
            }
            Stmt::While(ws) => {
                let loop_start = instrs.len();
                self.loop_stack.push(LoopContext {
                    label: ws.label.clone(),
                    start: loop_start,
                    break_jumps: Vec::new(),
                });

                let cond_reg = self.lower_expr(&ws.condition, ra, consts, instrs);
                let true_reg = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::LoadBool, true_reg, 1, 0));
                let cmp_dest = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::Eq, cmp_dest, cond_reg, true_reg));
                instrs.push(Instruction::abc(OpCode::Test, cmp_dest, 0, 0));
                let cond_jmp = instrs.len();
                instrs.push(Instruction::sax(OpCode::Jmp, 0));

                for s in &ws.body {
                    self.lower_stmt(s, ra, consts, instrs);
                }

                let back_offset = loop_start as i32 - instrs.len() as i32 - 1;
                instrs.push(Instruction::sax(OpCode::Jmp, back_offset));

                let after = instrs.len();
                instrs[cond_jmp] = Instruction::sax(OpCode::Jmp, (after - cond_jmp - 1) as i32);

                // Patch all break jumps
                let ctx = self.loop_stack.pop().unwrap();
                for bj in ctx.break_jumps {
                    instrs[bj] = Instruction::sax(OpCode::Jmp, (after - bj - 1) as i32);
                }
            }
            Stmt::Loop(ls) => {
                let loop_start = instrs.len();
                self.loop_stack.push(LoopContext {
                    label: ls.label.clone(),
                    start: loop_start,
                    break_jumps: Vec::new(),
                });

                for s in &ls.body {
                    self.lower_stmt(s, ra, consts, instrs);
                }
                let back_offset = loop_start as i32 - instrs.len() as i32 - 1;
                instrs.push(Instruction::sax(OpCode::Jmp, back_offset));

                let after = instrs.len();
                let ctx = self.loop_stack.pop().unwrap();
                for bj in ctx.break_jumps {
                    instrs[bj] = Instruction::sax(OpCode::Jmp, (after - bj - 1) as i32);
                }
            }
            Stmt::Break(bs) => {
                let jmp_idx = instrs.len();
                instrs.push(Instruction::sax(OpCode::Jmp, 0)); // placeholder
                let target = if let Some(label) = &bs.label {
                    self.loop_stack
                        .iter_mut()
                        .rev()
                        .find(|ctx| ctx.label.as_deref() == Some(label))
                } else {
                    self.loop_stack.last_mut()
                };
                if let Some(ctx) = target {
                    ctx.break_jumps.push(jmp_idx);
                }
            }
            Stmt::Continue(cs) => {
                let target = if let Some(label) = &cs.label {
                    self.loop_stack
                        .iter()
                        .rev()
                        .find(|ctx| ctx.label.as_deref() == Some(label))
                } else {
                    self.loop_stack.last()
                };
                if let Some(ctx) = target {
                    let back_offset = ctx.start as i32 - instrs.len() as i32 - 1;
                    instrs.push(Instruction::sax(OpCode::Jmp, back_offset));
                } else {
                    instrs.push(Instruction::sax(OpCode::Jmp, 0));
                }
            }
            Stmt::Emit(es) => {
                let val_reg = self.lower_expr(&es.value, ra, consts, instrs);
                instrs.push(Instruction::abc(OpCode::Emit, val_reg, 0, 0));
            }
            Stmt::CompoundAssign(ca) => {
                let val_reg = self.lower_expr(&ca.value, ra, consts, instrs);
                let target_reg = if let Some(r) = ra.lookup(&ca.target) {
                    r
                } else {
                    ra.alloc_named(&ca.target)
                };
                let opcode = match ca.op {
                    CompoundOp::AddAssign => OpCode::Add,
                    CompoundOp::SubAssign => OpCode::Sub,
                    CompoundOp::MulAssign => OpCode::Mul,
                    CompoundOp::DivAssign => OpCode::Div,
                    CompoundOp::FloorDivAssign => OpCode::FloorDiv,
                    CompoundOp::ModAssign => OpCode::Mod,
                    CompoundOp::PowAssign => OpCode::Pow,
                    CompoundOp::BitAndAssign => OpCode::BitAnd,
                    CompoundOp::BitOrAssign => OpCode::BitOr,
                    CompoundOp::BitXorAssign => OpCode::BitXor,
                };
                instrs.push(Instruction::abc(opcode, target_reg, target_reg, val_reg));
            }
            Stmt::Defer(ds) => {
                // Collect defer block body for emission before returns (LIFO order)
                self.defer_stack.push(ds.body.clone());
            }
        }
    }

    /// Emit all accumulated defer blocks in LIFO order (last defer first).
    /// This is called before every return point in a function.
    fn emit_defers(
        &mut self,
        ra: &mut RegAlloc,
        consts: &mut Vec<Constant>,
        instrs: &mut Vec<Instruction>,
    ) {
        // Clone the defer stack so we can iterate in reverse without borrowing issues
        let defers: Vec<Vec<Stmt>> = self.defer_stack.clone();
        for defer_body in defers.iter().rev() {
            for s in defer_body {
                self.lower_stmt(s, ra, consts, instrs);
            }
        }
    }

    fn push_const_int(
        &mut self,
        n: i64,
        ra: &mut RegAlloc,
        consts: &mut Vec<Constant>,
        instrs: &mut Vec<Instruction>,
    ) -> u8 {
        let reg = ra.alloc_temp();
        let kidx = consts.len() as u16;
        consts.push(Constant::Int(n));
        instrs.push(Instruction::abx(OpCode::LoadK, reg, kidx));
        reg
    }

    fn push_const_string(
        &mut self,
        s: &str,
        ra: &mut RegAlloc,
        consts: &mut Vec<Constant>,
        instrs: &mut Vec<Instruction>,
    ) -> u8 {
        let reg = ra.alloc_temp();
        let kidx = consts.len() as u16;
        consts.push(Constant::String(s.to_string()));
        instrs.push(Instruction::abx(OpCode::LoadK, reg, kidx));
        reg
    }

    fn emit_jump_if_false(&mut self, cond_reg: u8, instrs: &mut Vec<Instruction>) -> usize {
        instrs.push(Instruction::abc(OpCode::Test, cond_reg, 0, 0));
        let jmp_idx = instrs.len();
        instrs.push(Instruction::sax(OpCode::Jmp, 0));
        jmp_idx
    }

    fn emit_get_field(
        &mut self,
        dest: u8,
        obj_reg: u8,
        field_name: &str,
        ra: &mut RegAlloc,
        consts: &mut Vec<Constant>,
        instrs: &mut Vec<Instruction>,
    ) {
        let key_reg = self.push_const_string(field_name, ra, consts, instrs);
        instrs.push(Instruction::abc(OpCode::GetIndex, dest, obj_reg, key_reg));
    }

    fn emit_set_field(
        &mut self,
        obj_reg: u8,
        field_name: &str,
        value_reg: u8,
        ra: &mut RegAlloc,
        consts: &mut Vec<Constant>,
        instrs: &mut Vec<Instruction>,
    ) {
        let key_reg = self.push_const_string(field_name, ra, consts, instrs);
        instrs.push(Instruction::abc(
            OpCode::SetIndex,
            obj_reg,
            key_reg,
            value_reg,
        ));
    }

    fn lower_match_pattern(
        &mut self,
        pattern: &Pattern,
        value_reg: u8,
        ra: &mut RegAlloc,
        consts: &mut Vec<Constant>,
        instrs: &mut Vec<Instruction>,
        fail_jumps: &mut Vec<usize>,
    ) {
        match pattern {
            Pattern::Literal(lit_expr) => {
                let lit_reg = self.lower_expr(lit_expr, ra, consts, instrs);
                let cmp_reg = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::Eq, cmp_reg, value_reg, lit_reg));
                fail_jumps.push(self.emit_jump_if_false(cmp_reg, instrs));
            }
            Pattern::Variant(tag, binding, _) => {
                let tag_idx = self.intern_string(tag);
                instrs.push(Instruction::abx(OpCode::IsVariant, value_reg, tag_idx));
                let fail_jmp = instrs.len();
                instrs.push(Instruction::sax(OpCode::Jmp, 0));
                fail_jumps.push(fail_jmp);
                if let Some(inner_pattern) = binding {
                    // Unbox variant payload and recursively match/bind it.
                    let payload_reg = ra.alloc_temp();
                    instrs.push(Instruction::abc(OpCode::Unbox, payload_reg, value_reg, 0));
                    self.lower_match_pattern(
                        inner_pattern,
                        payload_reg,
                        ra,
                        consts,
                        instrs,
                        fail_jumps,
                    );
                }
            }
            Pattern::Wildcard(_) => {}
            Pattern::Ident(name, _) => {
                let breg = ra.alloc_named(name);
                instrs.push(Instruction::abc(OpCode::Move, breg, value_reg, 0));
            }
            Pattern::Guard {
                inner, condition, ..
            } => {
                self.lower_match_pattern(inner, value_reg, ra, consts, instrs, fail_jumps);
                let cond_reg = self.lower_expr(condition, ra, consts, instrs);
                fail_jumps.push(self.emit_jump_if_false(cond_reg, instrs));
            }
            Pattern::Or { patterns, .. } => {
                if patterns.is_empty() {
                    return;
                }
                let mut success_jumps = Vec::new();
                for (idx, p) in patterns.iter().enumerate() {
                    let mut alt_fail_jumps = Vec::new();
                    self.lower_match_pattern(p, value_reg, ra, consts, instrs, &mut alt_fail_jumps);
                    let is_last = idx + 1 == patterns.len();
                    if is_last {
                        fail_jumps.extend(alt_fail_jumps);
                    } else {
                        let success_jmp = instrs.len();
                        instrs.push(Instruction::sax(OpCode::Jmp, 0));
                        success_jumps.push(success_jmp);

                        let next_alt = instrs.len();
                        for j in alt_fail_jumps {
                            instrs[j] = Instruction::sax(OpCode::Jmp, (next_alt - j - 1) as i32);
                        }
                    }
                }
                let after_or = instrs.len();
                for j in success_jumps {
                    instrs[j] = Instruction::sax(OpCode::Jmp, (after_or - j - 1) as i32);
                }
            }
            Pattern::ListDestructure { elements, rest, .. } => {
                let list_type = self.push_const_string("List", ra, consts, instrs);
                let is_list = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::Is, is_list, value_reg, list_type));
                fail_jumps.push(self.emit_jump_if_false(is_list, instrs));

                let len_reg = ra.alloc_temp();
                instrs.push(Instruction::abc(
                    OpCode::Intrinsic,
                    len_reg,
                    IntrinsicId::Length as u8,
                    value_reg,
                ));
                let expected_reg = self.push_const_int(elements.len() as i64, ra, consts, instrs);
                let arity_ok = ra.alloc_temp();
                if rest.is_some() {
                    // expected_len <= actual_len
                    instrs.push(Instruction::abc(
                        OpCode::Le,
                        arity_ok,
                        expected_reg,
                        len_reg,
                    ));
                } else {
                    instrs.push(Instruction::abc(
                        OpCode::Eq,
                        arity_ok,
                        len_reg,
                        expected_reg,
                    ));
                }
                fail_jumps.push(self.emit_jump_if_false(arity_ok, instrs));

                for (idx, elem_pat) in elements.iter().enumerate() {
                    let idx_reg = self.push_const_int(idx as i64, ra, consts, instrs);
                    let elem_reg = ra.alloc_temp();
                    instrs.push(Instruction::abc(
                        OpCode::GetIndex,
                        elem_reg,
                        value_reg,
                        idx_reg,
                    ));
                    self.lower_match_pattern(elem_pat, elem_reg, ra, consts, instrs, fail_jumps);
                }

                if let Some(rest_name) = rest {
                    let list_arg = ra.alloc_temp();
                    instrs.push(Instruction::abc(OpCode::Move, list_arg, value_reg, 0));
                    let n_kidx = consts.len() as u16;
                    consts.push(Constant::Int(elements.len() as i64));
                    let n_reg = ra.alloc_temp();
                    instrs.push(Instruction::abx(OpCode::LoadK, n_reg, n_kidx));
                    debug_assert_eq!(n_reg, list_arg + 1);
                    let rest_reg = ra.alloc_named(rest_name);
                    instrs.push(Instruction::abc(
                        OpCode::Intrinsic,
                        rest_reg,
                        IntrinsicId::Drop as u8,
                        list_arg,
                    ));
                }
            }
            Pattern::TupleDestructure { elements, .. } => {
                let tuple_type = self.push_const_string("Tuple", ra, consts, instrs);
                let is_tuple = ra.alloc_temp();
                instrs.push(Instruction::abc(
                    OpCode::Is,
                    is_tuple,
                    value_reg,
                    tuple_type,
                ));
                fail_jumps.push(self.emit_jump_if_false(is_tuple, instrs));

                let len_reg = ra.alloc_temp();
                instrs.push(Instruction::abc(
                    OpCode::Intrinsic,
                    len_reg,
                    IntrinsicId::Length as u8,
                    value_reg,
                ));
                let expected_reg = self.push_const_int(elements.len() as i64, ra, consts, instrs);
                let arity_ok = ra.alloc_temp();
                instrs.push(Instruction::abc(
                    OpCode::Eq,
                    arity_ok,
                    len_reg,
                    expected_reg,
                ));
                fail_jumps.push(self.emit_jump_if_false(arity_ok, instrs));

                for (idx, elem_pat) in elements.iter().enumerate() {
                    let idx_reg = self.push_const_int(idx as i64, ra, consts, instrs);
                    let elem_reg = ra.alloc_temp();
                    instrs.push(Instruction::abc(
                        OpCode::GetIndex,
                        elem_reg,
                        value_reg,
                        idx_reg,
                    ));
                    self.lower_match_pattern(elem_pat, elem_reg, ra, consts, instrs, fail_jumps);
                }
            }
            Pattern::RecordDestructure {
                type_name,
                fields,
                open: _,
                ..
            } => {
                let ty_reg = self.push_const_string(type_name, ra, consts, instrs);
                let is_ty = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::Is, is_ty, value_reg, ty_reg));
                fail_jumps.push(self.emit_jump_if_false(is_ty, instrs));

                for (field_name, pat) in fields {
                    let field_reg = ra.alloc_temp();
                    self.emit_get_field(field_reg, value_reg, field_name, ra, consts, instrs);
                    if let Some(field_pat) = pat {
                        self.lower_match_pattern(
                            field_pat, field_reg, ra, consts, instrs, fail_jumps,
                        );
                    } else {
                        let bind_reg = ra.alloc_named(field_name);
                        instrs.push(Instruction::abc(OpCode::Move, bind_reg, field_reg, 0));
                    }
                }
            }
            Pattern::TypeCheck {
                name, type_expr, ..
            } => {
                let bind_reg = ra.alloc_named(name);
                instrs.push(Instruction::abc(OpCode::Move, bind_reg, value_reg, 0));
                if let TypeExpr::Named(type_name, _) = type_expr.as_ref() {
                    let ty_reg = self.push_const_string(type_name, ra, consts, instrs);
                    let is_ty = ra.alloc_temp();
                    instrs.push(Instruction::abc(OpCode::Is, is_ty, value_reg, ty_reg));
                    fail_jumps.push(self.emit_jump_if_false(is_ty, instrs));
                }
            }
        }
    }

    fn lower_expr(
        &mut self,
        expr: &Expr,
        ra: &mut RegAlloc,
        consts: &mut Vec<Constant>,
        instrs: &mut Vec<Instruction>,
    ) -> u8 {
        match expr {
            Expr::IntLit(n, _) => {
                let dest = ra.alloc_temp();
                let kidx = consts.len() as u16;
                consts.push(Constant::Int(*n));
                instrs.push(Instruction::abx(OpCode::LoadK, dest, kidx));
                dest
            }
            Expr::FloatLit(f, _) => {
                let dest = ra.alloc_temp();
                let kidx = consts.len() as u16;
                consts.push(Constant::Float(*f));
                instrs.push(Instruction::abx(OpCode::LoadK, dest, kidx));
                dest
            }
            Expr::StringLit(s, _) => {
                let dest = ra.alloc_temp();
                let kidx = consts.len() as u16;
                consts.push(Constant::String(s.clone()));
                instrs.push(Instruction::abx(OpCode::LoadK, dest, kidx));
                dest
            }

            Expr::StringInterp(segments, _) => {
                // Lower each segment and concat
                let dest = ra.alloc_temp();
                let mut first = true;
                for seg in segments {
                    match seg {
                        StringSegment::Literal(s) => {
                            if s.is_empty() {
                                continue;
                            }
                            let seg_reg = ra.alloc_temp();
                            let kidx = consts.len() as u16;
                            consts.push(Constant::String(s.clone()));
                            instrs.push(Instruction::abx(OpCode::LoadK, seg_reg, kidx));
                            if first {
                                instrs.push(Instruction::abc(OpCode::Move, dest, seg_reg, 0));
                                first = false;
                            } else {
                                instrs.push(Instruction::abc(OpCode::Concat, dest, dest, seg_reg));
                            }
                        }
                        StringSegment::Interpolation(expr) => {
                            let expr_reg = self.lower_expr(expr, ra, consts, instrs);
                            if first {
                                // Convert to string via a temp concat with empty
                                let empty_reg = ra.alloc_temp();
                                let kidx = consts.len() as u16;
                                consts.push(Constant::String(String::new()));
                                instrs.push(Instruction::abx(OpCode::LoadK, empty_reg, kidx));
                                instrs.push(Instruction::abc(
                                    OpCode::Concat,
                                    dest,
                                    empty_reg,
                                    expr_reg,
                                ));
                                first = false;
                            } else {
                                instrs.push(Instruction::abc(OpCode::Concat, dest, dest, expr_reg));
                            }
                        }
                    }
                }
                if first {
                    // Empty interpolation - load empty string
                    let kidx = consts.len() as u16;
                    consts.push(Constant::String(String::new()));
                    instrs.push(Instruction::abx(OpCode::LoadK, dest, kidx));
                }
                dest
            }
            Expr::BoolLit(b, _) => {
                let dest = ra.alloc_temp();
                instrs.push(Instruction::abc(
                    OpCode::LoadBool,
                    dest,
                    if *b { 1 } else { 0 },
                    0,
                ));
                dest
            }
            Expr::NullLit(_) => {
                let dest = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::LoadNil, dest, 0, 0));
                dest
            }
            Expr::Ident(name, _) => {
                if let Some(reg) = ra.lookup(name) {
                    reg
                } else if let Some(const_info) = self.symbols.consts.get(name) {
                    if let Some(ref value_expr) = const_info.value {
                        self.lower_expr(value_expr, ra, consts, instrs)
                    } else {
                        let dest = ra.alloc_temp();
                        instrs.push(Instruction::abc(OpCode::LoadNil, dest, 0, 0));
                        dest
                    }
                } else if self.symbols.types.values().any(|t| matches!(&t.kind, crate::compiler::resolve::TypeInfoKind::Enum(e) if e.variants.iter().any(|v| v.name == *name))) {
                    // Enum Variant Constructor (Union with no payload)
                    let dest = ra.alloc_temp();
                    let tag_reg = ra.alloc_temp();
                    let kidx = consts.len() as u16;
                    consts.push(Constant::String(name.clone()));
                    instrs.push(Instruction::abx(OpCode::LoadK, tag_reg, kidx));

                    let nil_reg = ra.alloc_temp();
                    instrs.push(Instruction::abc(OpCode::LoadNil, nil_reg, 0, 0));

                    instrs.push(Instruction::abc(OpCode::NewUnion, dest, tag_reg, nil_reg));
                    dest
                } else {
                    let dest = ra.alloc_temp();
                    let kidx = consts.len() as u16;
                    consts.push(Constant::String(name.clone()));
                    instrs.push(Instruction::abx(OpCode::LoadK, dest, kidx));
                    dest
                }
            }
            Expr::ListLit(elems, _) => {
                let dest = ra.alloc_temp();
                // Check if any element is a spread - if so, use append-based lowering
                let has_spread = elems.iter().any(|e| matches!(e, Expr::SpreadExpr(_, _)));

                if has_spread {
                    // Create empty list and append each element (or spread elements)
                    instrs.push(Instruction::abc(OpCode::NewList, dest, 0, 0));
                    for elem in elems {
                        match elem {
                            Expr::SpreadExpr(inner, _) => {
                                // Iterate over the spread value and append each element
                                let src_reg = self.lower_expr(inner, ra, consts, instrs);
                                let idx_reg = ra.alloc_temp();
                                let len_reg = ra.alloc_temp();

                                let zero_idx = consts.len() as u16;
                                consts.push(Constant::Int(0));
                                instrs.push(Instruction::abx(OpCode::LoadK, idx_reg, zero_idx));
                                instrs.push(Instruction::abc(
                                    OpCode::Intrinsic,
                                    len_reg,
                                    IntrinsicId::Length as u8,
                                    src_reg,
                                ));

                                let loop_start = instrs.len();
                                let lt_reg = ra.alloc_temp();
                                instrs.push(Instruction::abc(OpCode::Lt, lt_reg, idx_reg, len_reg));
                                instrs.push(Instruction::abc(OpCode::Test, lt_reg, 0, 0));
                                let break_jmp = instrs.len();
                                instrs.push(Instruction::sax(OpCode::Jmp, 0));

                                let elem_reg = ra.alloc_temp();
                                instrs.push(Instruction::abc(
                                    OpCode::GetIndex,
                                    elem_reg,
                                    src_reg,
                                    idx_reg,
                                ));
                                instrs.push(Instruction::abc(OpCode::Append, dest, elem_reg, 0));

                                let one_idx = consts.len() as u16;
                                consts.push(Constant::Int(1));
                                let one_reg = ra.alloc_temp();
                                instrs.push(Instruction::abx(OpCode::LoadK, one_reg, one_idx));
                                instrs.push(Instruction::abc(
                                    OpCode::Add,
                                    idx_reg,
                                    idx_reg,
                                    one_reg,
                                ));

                                let back_offset = loop_start as i32 - instrs.len() as i32 - 1;
                                instrs.push(Instruction::sax(OpCode::Jmp, back_offset));

                                let after_loop = instrs.len();
                                instrs[break_jmp] = Instruction::sax(
                                    OpCode::Jmp,
                                    (after_loop - break_jmp - 1) as i32,
                                );
                            }
                            _ => {
                                let er = self.lower_expr(elem, ra, consts, instrs);
                                instrs.push(Instruction::abc(OpCode::Append, dest, er, 0));
                            }
                        }
                    }
                } else {
                    // No spread - use original efficient lowering
                    let mut elem_regs = Vec::new();
                    for elem in elems {
                        let er = self.lower_expr(elem, ra, consts, instrs);
                        elem_regs.push(er);
                    }
                    // Move elements into consecutive positions dest+1..dest+N
                    for (i, er) in elem_regs.iter().enumerate() {
                        let target = dest + 1 + i as u8;
                        if *er != target {
                            instrs.push(Instruction::abc(OpCode::Move, target, *er, 0));
                        }
                    }
                    instrs.push(Instruction::abc(
                        OpCode::NewList,
                        dest,
                        elems.len() as u8,
                        0,
                    ));
                }
                dest
            }
            Expr::MapLit(pairs, _) => {
                let dest = ra.alloc_temp();
                let mut kv_regs = Vec::new();
                for (k, v) in pairs {
                    kv_regs.push(self.lower_expr(k, ra, consts, instrs));
                    kv_regs.push(self.lower_expr(v, ra, consts, instrs));
                }

                // Move KVs into consecutive positions dest+1..
                for (i, reg) in kv_regs.iter().enumerate() {
                    let target = dest + 1 + i as u8;
                    if *reg != target {
                        instrs.push(Instruction::abc(OpCode::Move, target, *reg, 0));
                    }
                }
                instrs.push(Instruction::abc(OpCode::NewMap, dest, pairs.len() as u8, 0));
                dest
            }
            Expr::RecordLit(name, fields, _) => {
                let dest = ra.alloc_temp();
                let type_idx = self.intern_string(name);
                instrs.push(Instruction::abx(OpCode::NewRecord, dest, type_idx));
                // Now set each field
                for (field_name, val) in fields {
                    let val_reg = self.lower_expr(val, ra, consts, instrs);
                    self.emit_set_field(dest, field_name, val_reg, ra, consts, instrs);
                }
                dest
            }
            Expr::Pipe { left, right, span } => {
                let call_expr = desugar_pipe_application(left, right, *span);
                self.lower_expr(&call_expr, ra, consts, instrs)
            }
            Expr::Illuminate {
                input,
                transform,
                span,
            } => {
                let call_expr = desugar_pipe_application(input, transform, *span);
                self.lower_expr(&call_expr, ra, consts, instrs)
            }
            Expr::BinOp(lhs, op, rhs, _) => {
                // Special case: pipe forward desugars to function call
                // a |> f(b, c) becomes f(a, b, c)
                if *op == BinOp::PipeForward {
                    let piped_val = self.lower_expr(lhs, ra, consts, instrs);

                    // If RHS is a Call, inject piped value as first argument
                    if let Expr::Call(func_expr, args, _) = rhs.as_ref() {
                        let func_reg = self.lower_expr(func_expr, ra, consts, instrs);

                        // Build arg_regs with piped value first, then existing args
                        let mut arg_regs = vec![piped_val];
                        for arg in args {
                            let arg_val = match arg {
                                CallArg::Positional(e)
                                | CallArg::Named(_, e, _)
                                | CallArg::Role(_, e, _) => self.lower_expr(e, ra, consts, instrs),
                            };
                            arg_regs.push(arg_val);
                        }

                        return self.emit_call_with_regs(func_reg, &arg_regs, ra, instrs);
                    }

                    // RHS is not a Call - treat as function value, call with piped value
                    let fn_reg = self.lower_expr(rhs, ra, consts, instrs);
                    return self.emit_call_with_regs(fn_reg, &[piped_val], ra, instrs);
                }

                let lr = self.lower_expr(lhs, ra, consts, instrs);
                let rr = self.lower_expr(rhs, ra, consts, instrs);
                let dest = ra.alloc_temp();
                let opcode = match op {
                    BinOp::Add => OpCode::Add,
                    BinOp::Sub => OpCode::Sub,
                    BinOp::Mul => OpCode::Mul,
                    BinOp::Div => OpCode::Div,
                    BinOp::FloorDiv => OpCode::FloorDiv,
                    BinOp::Mod => OpCode::Mod,
                    BinOp::Eq => OpCode::Eq,
                    BinOp::NotEq => OpCode::Eq, // inverted in post
                    BinOp::Lt => OpCode::Lt,
                    BinOp::LtEq => OpCode::Le,
                    BinOp::Gt => OpCode::Lt,   // swap operands
                    BinOp::GtEq => OpCode::Le, // swap operands
                    BinOp::And => OpCode::And,
                    BinOp::Or => OpCode::Or,
                    BinOp::Pow => OpCode::Pow,
                    BinOp::Concat => OpCode::Concat,
                    BinOp::In => OpCode::In,
                    BinOp::BitAnd => OpCode::BitAnd,
                    BinOp::BitOr => OpCode::BitOr,
                    BinOp::BitXor => OpCode::BitXor,
                    BinOp::Shl => OpCode::Shl,
                    BinOp::Shr => OpCode::Shr,
                    BinOp::PipeForward => unreachable!(), // handled above
                };
                match op {
                    BinOp::Gt => instrs.push(Instruction::abc(opcode, dest, rr, lr)),
                    BinOp::GtEq => instrs.push(Instruction::abc(opcode, dest, rr, lr)),
                    _ => instrs.push(Instruction::abc(opcode, dest, lr, rr)),
                }
                if *op == BinOp::NotEq {
                    instrs.push(Instruction::abc(OpCode::Not, dest, dest, 0));
                }
                dest
            }
            Expr::UnaryOp(op, inner, _) => {
                let ir = self.lower_expr(inner, ra, consts, instrs);
                let dest = ra.alloc_temp();
                match op {
                    UnaryOp::Neg => instrs.push(Instruction::abc(OpCode::Neg, dest, ir, 0)),
                    UnaryOp::Not => instrs.push(Instruction::abc(OpCode::Not, dest, ir, 0)),
                    UnaryOp::BitNot => instrs.push(Instruction::abc(OpCode::BitNot, dest, ir, 0)),
                }
                dest
            }

            Expr::Call(callee, args, _) => {
                if let Some(effect_path) = effect_operation_name(callee.as_ref()) {
                    if let Some(handler_cell) = self.effect_handler_cells.get(&effect_path).cloned()
                    {
                        return self.lower_named_call_target(
                            &handler_cell,
                            args,
                            None,
                            ra,
                            consts,
                            instrs,
                        );
                    }
                    if let Some(tool_alias) = self
                        .resolve_effect_tool_binding(&effect_path)
                        .map(|s| s.to_string())
                    {
                        return self.lower_tool_call(Some(&tool_alias), args, ra, consts, instrs);
                    }
                }

                // Check for intrinsic call or Enum/Result constructor
                if let Expr::Ident(ref name, _) = **callee {
                    if self.tool_indices.contains_key(name)
                        && !self.symbols.cells.contains_key(name)
                    {
                        return self.lower_tool_call(Some(name.as_str()), args, ra, consts, instrs);
                    }

                    let is_agent_ctor = self.symbols.agents.contains_key(name);
                    let is_process_ctor = self.symbols.processes.values().any(|p| p.name == *name);
                    // Check Result/Enum constructors
                    // "ok" / "err"
                    let is_result = name == "ok" || name == "err";
                    let is_enum = self.symbols.types.values().any(|t| matches!(&t.kind, crate::compiler::resolve::TypeInfoKind::Enum(e) if e.variants.iter().any(|v| v.name == *name)));
                    let is_record = self.symbols.types.values().any(|t| matches!(&t.kind, crate::compiler::resolve::TypeInfoKind::Record(r) if r.name == *name));

                    if is_record && !is_agent_ctor && !is_process_ctor {
                        let dest = ra.alloc_temp();
                        let type_idx = self.intern_string(name);
                        instrs.push(Instruction::abx(OpCode::NewRecord, dest, type_idx));

                        for arg in args {
                            match arg {
                                CallArg::Named(field, expr, _) => {
                                    let val_reg = self.lower_expr(expr, ra, consts, instrs);

                                    // Check for 'where' constraint on this field
                                    // We need to look up the RecordDef fields
                                    let mut constraint_expr = None;
                                    if let Some(crate::compiler::resolve::TypeInfo {
                                        kind: crate::compiler::resolve::TypeInfoKind::Record(def),
                                        ..
                                    }) = self.symbols.types.get(name)
                                    {
                                        if let Some(f) =
                                            def.fields.iter().find(|fd| fd.name == *field)
                                        {
                                            constraint_expr = f.constraint.clone();
                                        }
                                    }

                                    if let Some(cexpr) = constraint_expr {
                                        // 1. Save old binding
                                        let old_bind = ra.lookup(field);
                                        // 2. Bind field name to val_reg (shadowing)
                                        ra.bind(field, val_reg);

                                        // 3. Lower constraint expression
                                        let cond_reg = self.lower_expr(&cexpr, ra, consts, instrs);

                                        // 4. Restore old binding
                                        if let Some(old) = old_bind {
                                            ra.bind(field, old);
                                        } else {
                                            ra.unbind(field);
                                        }

                                        // 5. Emit Check
                                        // 5. Emit Check
                                        // Check against false: if cond == false, skip Jmp and Halt
                                        let false_idx = consts.len() as u16;
                                        consts.push(Constant::Bool(false));
                                        let false_reg = ra.alloc_temp();
                                        instrs.push(Instruction::abx(
                                            OpCode::LoadK,
                                            false_reg,
                                            false_idx,
                                        ));

                                        // Eq(cond, false)
                                        // result = (cond == false)
                                        let cmp_reg = ra.alloc_temp();
                                        instrs.push(Instruction::abc(
                                            OpCode::Eq,
                                            cmp_reg,
                                            cond_reg,
                                            false_reg,
                                        ));

                                        // If cmp_reg is True (Constraint Failed), we want to Halt.
                                        // If cmp_reg is False (Constraint OK), we want to Jmp over Halt.

                                        // Test(cmp_reg, 0, 0) skips next instruction if cmp_reg is Truthy.
                                        // So if Failed (True), Skip Next (Jmp) -> Execute Halt.
                                        // If OK (False), Don't Skip -> Execute Next (Jmp) -> Skip Halt.
                                        instrs.push(Instruction::abc(OpCode::Test, cmp_reg, 0, 0));

                                        // If OK, jump over Halt
                                        let jmp_idx = instrs.len();
                                        instrs.push(Instruction::sax(OpCode::Jmp, 0));

                                        // Halt(msg)
                                        let msg =
                                            format!("Constraint failed for field '{}'", field);
                                        let msg_idx = consts.len() as u16;
                                        consts.push(Constant::String(msg));
                                        let msg_reg = ra.alloc_temp();
                                        instrs.push(Instruction::abx(
                                            OpCode::LoadK,
                                            msg_reg,
                                            msg_idx,
                                        ));
                                        instrs.push(Instruction::abc(OpCode::Halt, msg_reg, 0, 0));

                                        // Patch Jmp
                                        let after_halt = instrs.len();
                                        let offset = (after_halt - jmp_idx - 1) as i32;
                                        instrs[jmp_idx] = Instruction::sax(OpCode::Jmp, offset);
                                    }

                                    self.emit_set_field(dest, field, val_reg, ra, consts, instrs);
                                }
                                CallArg::Positional(expr) => {
                                    // Positional not supported for records yet? Or map by index?
                                    // Typecheck didn't validate positional args for records.
                                    // Assume named args for now or ignore.
                                    let _ = self.lower_expr(expr, ra, consts, instrs);
                                    // consume
                                }
                                _ => {}
                            }
                        }
                        return dest;
                    }

                    if is_result || is_enum {
                        // Expect 1 argument (payload)
                        // If 0 args -> Nil payload (handled in Ident if bare, but if ok(), then explicit nil?)
                        // If >1 args -> Tuple? Result only takes 1. Enum payload is 1 type (maybe tuple).
                        // For now assume 1 arg.
                        let payload_reg = if args.is_empty() {
                            let r = ra.alloc_temp();
                            instrs.push(Instruction::abc(OpCode::LoadNil, r, 0, 0));
                            r
                        } else {
                            match &args[0] {
                                CallArg::Positional(e) | CallArg::Named(_, e, _) => {
                                    self.lower_expr(e, ra, consts, instrs)
                                }
                                _ => {
                                    let r = ra.alloc_temp();
                                    instrs.push(Instruction::abc(OpCode::LoadNil, r, 0, 0));
                                    r
                                }
                            }
                        };

                        let dest = ra.alloc_temp();
                        let tag_reg = ra.alloc_temp();
                        let kidx = consts.len() as u16;
                        consts.push(Constant::String(name.clone()));
                        instrs.push(Instruction::abx(OpCode::LoadK, tag_reg, kidx));

                        instrs.push(Instruction::abc(
                            OpCode::NewUnion,
                            dest,
                            tag_reg,
                            payload_reg,
                        ));
                        return dest;
                    }

                    // Only treat as intrinsic if it's not a defined cell
                    let intrinsic = if !self.symbols.cells.contains_key(name) {
                        match name.as_str() {
                            "print" => Some(IntrinsicId::Print),
                            "len" | "length" => Some(IntrinsicId::Length),
                            "range" => Some(IntrinsicId::Range),
                            "string" => Some(IntrinsicId::ToString),
                            "int" => Some(IntrinsicId::ToInt),
                            "float" => Some(IntrinsicId::ToFloat),
                            "type" | "type_of" => Some(IntrinsicId::TypeOf),
                            "keys" => Some(IntrinsicId::Keys),
                            "values" => Some(IntrinsicId::Values),
                            "join" => Some(IntrinsicId::Join),
                            "split" => Some(IntrinsicId::Split),
                            "append" => Some(IntrinsicId::Append),
                            "contains" | "has" => Some(IntrinsicId::Contains),
                            "slice" => Some(IntrinsicId::Slice),
                            "min" => Some(IntrinsicId::Min),
                            "max" => Some(IntrinsicId::Max),
                            "confirm" | "matches" => Some(IntrinsicId::Matches),
                            "trace_ref" => Some(IntrinsicId::TraceRef),
                            "abs" => Some(IntrinsicId::Abs),
                            // Collection operations
                            "sort" => Some(IntrinsicId::Sort),
                            "reverse" => Some(IntrinsicId::Reverse),
                            "map" => Some(IntrinsicId::Map),
                            "filter" => Some(IntrinsicId::Filter),
                            "reduce" => Some(IntrinsicId::Reduce),
                            "flat_map" => Some(IntrinsicId::FlatMap),
                            "zip" => Some(IntrinsicId::Zip),
                            "enumerate" => Some(IntrinsicId::Enumerate),
                            "any" => Some(IntrinsicId::Any),
                            "all" => Some(IntrinsicId::All),
                            "find" => Some(IntrinsicId::Find),
                            "position" => Some(IntrinsicId::Position),
                            "group_by" => Some(IntrinsicId::GroupBy),
                            "chunk" => Some(IntrinsicId::Chunk),
                            "window" => Some(IntrinsicId::Window),
                            "flatten" => Some(IntrinsicId::Flatten),
                            "unique" => Some(IntrinsicId::Unique),
                            "take" => Some(IntrinsicId::Take),
                            "drop" => Some(IntrinsicId::Drop),
                            "first" => Some(IntrinsicId::First),
                            "last" => Some(IntrinsicId::Last),
                            "is_empty" => Some(IntrinsicId::IsEmpty),
                            // String operations
                            "chars" => Some(IntrinsicId::Chars),
                            "starts_with" => Some(IntrinsicId::StartsWith),
                            "ends_with" => Some(IntrinsicId::EndsWith),
                            "index_of" => Some(IntrinsicId::IndexOf),
                            "pad_left" => Some(IntrinsicId::PadLeft),
                            "pad_right" => Some(IntrinsicId::PadRight),
                            "trim" => Some(IntrinsicId::Trim),
                            "upper" => Some(IntrinsicId::Upper),
                            "lower" => Some(IntrinsicId::Lower),
                            "replace" => Some(IntrinsicId::Replace),
                            // Math operations
                            "round" => Some(IntrinsicId::Round),
                            "ceil" => Some(IntrinsicId::Ceil),
                            "floor" => Some(IntrinsicId::Floor),
                            "sqrt" => Some(IntrinsicId::Sqrt),
                            "pow" => Some(IntrinsicId::Pow),
                            "log" => Some(IntrinsicId::Log),
                            "sin" => Some(IntrinsicId::Sin),
                            "cos" => Some(IntrinsicId::Cos),
                            "clamp" => Some(IntrinsicId::Clamp),
                            // Utility operations
                            "clone" => Some(IntrinsicId::Clone),
                            "sizeof" => Some(IntrinsicId::Sizeof),
                            "debug" => Some(IntrinsicId::Debug),
                            "count" => Some(IntrinsicId::Count),
                            "hash" => Some(IntrinsicId::Hash),
                            "diff" => Some(IntrinsicId::Diff),
                            "patch" => Some(IntrinsicId::Patch),
                            "redact" => Some(IntrinsicId::Redact),
                            "validate" => Some(IntrinsicId::Validate),
                            // Map/Set operations
                            "has_key" => Some(IntrinsicId::HasKey),
                            "merge" => Some(IntrinsicId::Merge),
                            "size" => Some(IntrinsicId::Size),
                            "add" => Some(IntrinsicId::Add),
                            "remove" => Some(IntrinsicId::Remove),
                            "entries" => Some(IntrinsicId::Entries),
                            _ => None,
                        }
                    } else {
                        None
                    };

                    if let Some(id) = intrinsic {
                        // Evaluate args
                        let mut arg_regs = Vec::new();
                        for arg in args {
                            match arg {
                                CallArg::Positional(e) | CallArg::Named(_, e, _) => {
                                    arg_regs.push(self.lower_expr(e, ra, consts, instrs));
                                }
                                _ => {} // Role not supported in intrinsics yet?
                            }
                        }

                        // Move args to contiguous block
                        let start_reg = ra.alloc_temp();
                        // We need count registers
                        for _ in 0..arg_regs.len().saturating_sub(1) {
                            ra.alloc_temp();
                        }

                        for (i, &reg) in arg_regs.iter().enumerate() {
                            let target = start_reg + i as u8;
                            if reg != target {
                                instrs.push(Instruction::abc(OpCode::Move, target, reg, 0));
                            }
                        }

                        let dest = ra.alloc_temp();
                        instrs.push(Instruction::abc(
                            OpCode::Intrinsic,
                            dest,
                            id as u8,
                            start_reg,
                        ));
                        return dest;
                    }
                }

                // Normal call (including agent-style method calls via dot access).
                let mut implicit_self_arg: Option<u8> = None;
                let callee_reg = if let Expr::DotAccess(obj, field, _) = callee.as_ref() {
                    if let Expr::Ident(agent_name, _) = obj.as_ref() {
                        let is_ctor_target = self.symbols.agents.contains_key(agent_name)
                            || self
                                .symbols
                                .processes
                                .values()
                                .any(|p| p.name == *agent_name);
                        if is_ctor_target {
                            let ctor_reg = ra.alloc_temp();
                            let ctor_idx = consts.len() as u16;
                            consts.push(Constant::String(agent_name.clone()));
                            instrs.push(Instruction::abx(OpCode::LoadK, ctor_reg, ctor_idx));
                            instrs.push(Instruction::abc(OpCode::Call, ctor_reg, 0, 1));
                            implicit_self_arg = Some(ctor_reg);

                            let dest = ra.alloc_temp();
                            let kidx = consts.len() as u16;
                            consts.push(Constant::String(format!("{}.{}", agent_name, field)));
                            instrs.push(Instruction::abx(OpCode::LoadK, dest, kidx));
                            dest
                        } else {
                            let obj_reg = self.lower_expr(obj, ra, consts, instrs);
                            implicit_self_arg = Some(obj_reg);
                            let dest = ra.alloc_temp();
                            self.emit_get_field(dest, obj_reg, field, ra, consts, instrs);
                            dest
                        }
                    } else {
                        let obj_reg = self.lower_expr(obj, ra, consts, instrs);
                        implicit_self_arg = Some(obj_reg);
                        let dest = ra.alloc_temp();
                        self.emit_get_field(dest, obj_reg, field, ra, consts, instrs);
                        dest
                    }
                } else {
                    self.lower_expr(callee, ra, consts, instrs)
                };

                let arg_regs =
                    self.lower_call_arg_regs(args, implicit_self_arg, ra, consts, instrs);
                self.emit_call_with_regs(callee_reg, &arg_regs, ra, instrs)
            }
            Expr::ToolCall(callee, args, _) => {
                let alias = match callee.as_ref() {
                    Expr::Ident(name, _) => Some(name.as_str()),
                    _ => None,
                };
                self.lower_tool_call(alias, args, ra, consts, instrs)
            }
            Expr::DotAccess(obj, field, _) => {
                let or = self.lower_expr(obj, ra, consts, instrs);
                let dest = ra.alloc_temp();
                self.emit_get_field(dest, or, field, ra, consts, instrs);
                dest
            }
            Expr::IndexAccess(obj, idx, _) => {
                let or = self.lower_expr(obj, ra, consts, instrs);
                let ir = self.lower_expr(idx, ra, consts, instrs);
                let dest = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::GetIndex, dest, or, ir));
                dest
            }
            Expr::RoleBlock(name, content, _) => {
                let content_reg = self.lower_expr(content, ra, consts, instrs);

                // Prefix: "name: "
                let prefix_reg = ra.alloc_temp();
                let kidx = consts.len() as u16;
                consts.push(Constant::String(format!("{}: ", name)));
                instrs.push(Instruction::abx(OpCode::LoadK, prefix_reg, kidx));

                // Concat
                let dest = ra.alloc_temp();
                instrs.push(Instruction::abc(
                    OpCode::Concat,
                    dest,
                    prefix_reg,
                    content_reg,
                ));
                dest
            }
            Expr::ExpectSchema(inner, schema_name, _) => {
                let ir = self.lower_expr(inner, ra, consts, instrs);
                let schema_idx = self.intern_string(schema_name);
                instrs.push(Instruction::abx(OpCode::Schema, ir, schema_idx));
                ir
            }
            Expr::RawStringLit(s, _) => {
                let dest = ra.alloc_temp();
                let kidx = consts.len() as u16;
                consts.push(Constant::String(s.clone()));
                instrs.push(Instruction::abx(OpCode::LoadK, dest, kidx));
                dest
            }
            Expr::BytesLit(bytes, _) => {
                let dest = ra.alloc_temp();
                let kidx = consts.len() as u16;
                // Store bytes as hex string constant for now
                let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
                consts.push(Constant::String(hex));
                instrs.push(Instruction::abx(OpCode::LoadK, dest, kidx));
                dest
            }
            Expr::Lambda {
                params,
                return_type,
                body,
                ..
            } => {
                // Collect identifiers referenced in the lambda body
                let mut referenced = Vec::new();
                match body {
                    LambdaBody::Expr(e) => collect_free_idents_expr(e, &mut referenced),
                    LambdaBody::Block(stmts) => {
                        for s in stmts {
                            collect_free_idents_stmt(s, &mut referenced);
                        }
                    }
                }

                // Determine which referenced names are captures from the enclosing scope
                let param_names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
                let mut captures: Vec<(String, u8)> = Vec::new(); // (name, outer_reg)
                let mut seen = std::collections::HashSet::new();
                for name in &referenced {
                    if param_names.contains(&name.as_str()) {
                        continue;
                    }
                    if !seen.insert(name.clone()) {
                        continue;
                    }
                    if let Some(outer_reg) = ra.lookup(name) {
                        captures.push((name.clone(), outer_reg));
                    }
                }

                // Lower lambda body as a separate LirCell
                let lambda_name = format!("<lambda/{}>", self.lambda_cells.len());
                let mut lra = RegAlloc::new();
                let mut lconsts: Vec<Constant> = Vec::new();
                let mut linstrs: Vec<Instruction> = Vec::new();

                // VM convention: captures occupy registers 0..captures.len(),
                // then parameters follow. The params list in LirCell must include
                // placeholder entries for captures so the VM can index correctly.
                let mut lparams: Vec<LirParam> = Vec::new();

                // 1. Allocate registers for captures first (r0, r1, ...)
                for (idx, (name, _)) in captures.iter().enumerate() {
                    let reg = lra.alloc_named(name);
                    lparams.push(LirParam {
                        name: format!("__capture_{}", name),
                        ty: "Any".to_string(),
                        register: reg,
                        variadic: false,
                    });
                    // GetUpval loads the capture from the closure's capture list.
                    // The VM already copies captures to regs 0..cap_count on call,
                    // but GetUpval is still emitted for correctness when the frame
                    // is entered via non-closure dispatch paths.
                    linstrs.push(Instruction::abc(OpCode::GetUpval, reg, idx as u8, 0));
                }

                // 2. Allocate registers for actual parameters after captures
                for p in params.iter() {
                    let reg = lra.alloc_named(&p.name);
                    lparams.push(LirParam {
                        name: p.name.clone(),
                        ty: format_type_expr(&p.ty),
                        register: reg,
                        variadic: p.variadic,
                    });
                }

                // Save and reset defer stack for lambda scope
                let saved_defers = std::mem::take(&mut self.defer_stack);

                match body {
                    LambdaBody::Expr(e) => {
                        let val = self.lower_expr(e, &mut lra, &mut lconsts, &mut linstrs);
                        linstrs.push(Instruction::abc(OpCode::Return, val, 1, 0));
                    }
                    LambdaBody::Block(stmts) => {
                        for s in stmts {
                            self.lower_stmt(s, &mut lra, &mut lconsts, &mut linstrs);
                        }
                        if linstrs.is_empty()
                            || !matches!(
                                linstrs.last().map(|i| i.op),
                                Some(OpCode::Return) | Some(OpCode::Halt)
                            )
                        {
                            // Emit defers before implicit return in lambda
                            self.emit_defers(&mut lra, &mut lconsts, &mut linstrs);
                            let r = lra.alloc_temp();
                            linstrs.push(Instruction::abc(OpCode::LoadNil, r, 0, 0));
                            linstrs.push(Instruction::abc(OpCode::Return, r, 1, 0));
                        }
                    }
                }

                // Restore defer stack
                self.defer_stack = saved_defers;

                let proto_idx = self.lambda_cells.len() as u16;
                self.lambda_cells.push(LirCell {
                    name: lambda_name,
                    params: lparams,
                    returns: return_type.as_ref().map(|t| format_type_expr(t)),
                    registers: lra.max_regs(),
                    constants: lconsts,
                    instructions: linstrs,
                });

                let dest = ra.alloc_temp();
                instrs.push(Instruction::abx(OpCode::Closure, dest, proto_idx));

                // Emit SetUpval for each captured variable to populate the closure
                for (idx, (_, outer_reg)) in captures.iter().enumerate() {
                    instrs.push(Instruction::abc(
                        OpCode::SetUpval,
                        *outer_reg,
                        idx as u8,
                        dest,
                    ));
                }

                dest
            }
            Expr::TupleLit(elems, _) => {
                let dest = ra.alloc_temp();
                let mut elem_regs = Vec::new();
                for elem in elems {
                    let er = self.lower_expr(elem, ra, consts, instrs);
                    elem_regs.push(er);
                }
                for (i, er) in elem_regs.iter().enumerate() {
                    let target = dest + 1 + i as u8;
                    if *er != target {
                        instrs.push(Instruction::abc(OpCode::Move, target, *er, 0));
                    }
                }
                instrs.push(Instruction::abc(
                    OpCode::NewTuple,
                    dest,
                    elems.len() as u8,
                    0,
                ));
                dest
            }
            Expr::SetLit(elems, _) => {
                let dest = ra.alloc_temp();
                let mut elem_regs = Vec::new();
                for elem in elems {
                    let er = self.lower_expr(elem, ra, consts, instrs);
                    elem_regs.push(er);
                }
                for (i, er) in elem_regs.iter().enumerate() {
                    let target = dest + 1 + i as u8;
                    if *er != target {
                        instrs.push(Instruction::abc(OpCode::Move, target, *er, 0));
                    }
                }
                instrs.push(Instruction::abc(OpCode::NewSet, dest, elems.len() as u8, 0));
                dest
            }
            Expr::RangeExpr { start, end, .. } => {
                // Lower as range(start, end) intrinsic call
                let sr = if let Some(s) = start {
                    self.lower_expr(s, ra, consts, instrs)
                } else {
                    let r = ra.alloc_temp();
                    let kidx = consts.len() as u16;
                    consts.push(Constant::Int(0));
                    instrs.push(Instruction::abx(OpCode::LoadK, r, kidx));
                    r
                };
                let er = if let Some(e) = end {
                    self.lower_expr(e, ra, consts, instrs)
                } else {
                    let r = ra.alloc_temp();
                    let kidx = consts.len() as u16;
                    consts.push(Constant::Int(0));
                    instrs.push(Instruction::abx(OpCode::LoadK, r, kidx));
                    r
                };
                let dest = ra.alloc_temp();
                let start_reg = ra.alloc_temp();
                ra.alloc_temp(); // for end
                if sr != start_reg {
                    instrs.push(Instruction::abc(OpCode::Move, start_reg, sr, 0));
                }
                if er != start_reg + 1 {
                    instrs.push(Instruction::abc(OpCode::Move, start_reg + 1, er, 0));
                }
                instrs.push(Instruction::abc(
                    OpCode::Intrinsic,
                    dest,
                    IntrinsicId::Range as u8,
                    start_reg,
                ));
                dest
            }
            Expr::TryExpr(inner, _) => {
                let ir = self.lower_expr(inner, ra, consts, instrs);
                let dest = ra.alloc_temp();
                // Check if result is err variant
                let err_idx = self.intern_string("err");
                instrs.push(Instruction::abx(OpCode::IsVariant, ir, err_idx));
                // IsVariant: if matched (is err), skip next instruction
                // If err: skip Jmp -> go to Return(err)
                // If ok: execute Jmp -> jump over return
                let jmp_ok = instrs.len();
                instrs.push(Instruction::sax(OpCode::Jmp, 0)); // jump past return-err
                                                               // Return the error as-is
                instrs.push(Instruction::abc(OpCode::Return, ir, 1, 0));
                // Patch jump
                let after = instrs.len();
                instrs[jmp_ok] = Instruction::sax(OpCode::Jmp, (after - jmp_ok - 1) as i32);
                // Unbox ok value
                instrs.push(Instruction::abc(OpCode::Unbox, dest, ir, 0));
                dest
            }
            Expr::NullCoalesce(lhs, rhs, _) => {
                let lr = self.lower_expr(lhs, ra, consts, instrs);
                let rr = self.lower_expr(rhs, ra, consts, instrs);
                let dest = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::NullCo, dest, lr, rr));
                dest
            }
            Expr::NullSafeAccess(obj, field, _) => {
                let or = self.lower_expr(obj, ra, consts, instrs);
                let dest = ra.alloc_temp();
                // Test if obj is null: skip Jmp (to null case) when NOT null
                let nil_reg = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::LoadNil, nil_reg, 0, 0));
                let cmp = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::Eq, cmp, or, nil_reg));
                instrs.push(Instruction::abc(OpCode::Test, cmp, 0, 1));
                let jmp_null = instrs.len();
                instrs.push(Instruction::sax(OpCode::Jmp, 0));
                // Not null: get field
                self.emit_get_field(dest, or, field, ra, consts, instrs);
                let jmp_end = instrs.len();
                instrs.push(Instruction::sax(OpCode::Jmp, 0));
                // Null case: load nil
                let null_start = instrs.len();
                instrs[jmp_null] =
                    Instruction::sax(OpCode::Jmp, (null_start - jmp_null - 1) as i32);
                instrs.push(Instruction::abc(OpCode::LoadNil, dest, 0, 0));
                let after = instrs.len();
                instrs[jmp_end] = Instruction::sax(OpCode::Jmp, (after - jmp_end - 1) as i32);
                dest
            }
            Expr::NullSafeIndex(obj, index, _) => {
                let or = self.lower_expr(obj, ra, consts, instrs);
                let dest = ra.alloc_temp();
                // Test if obj is null: skip Jmp (to null case) when NOT null
                let nil_reg = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::LoadNil, nil_reg, 0, 0));
                let cmp = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::Eq, cmp, or, nil_reg));
                instrs.push(Instruction::abc(OpCode::Test, cmp, 0, 1));
                let jmp_null = instrs.len();
                instrs.push(Instruction::sax(OpCode::Jmp, 0));
                // Not null: get index
                let idx_reg = self.lower_expr(index, ra, consts, instrs);
                instrs.push(Instruction::abc(OpCode::GetIndex, dest, or, idx_reg));
                let jmp_end = instrs.len();
                instrs.push(Instruction::sax(OpCode::Jmp, 0));
                // Null case: load nil
                let null_start = instrs.len();
                instrs[jmp_null] =
                    Instruction::sax(OpCode::Jmp, (null_start - jmp_null - 1) as i32);
                instrs.push(Instruction::abc(OpCode::LoadNil, dest, 0, 0));
                let after = instrs.len();
                instrs[jmp_end] = Instruction::sax(OpCode::Jmp, (after - jmp_end - 1) as i32);
                dest
            }
            Expr::NullAssert(inner, _) => {
                let ir = self.lower_expr(inner, ra, consts, instrs);
                // If null, halt with error
                let nil_reg = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::LoadNil, nil_reg, 0, 0));
                let cmp = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::Eq, cmp, ir, nil_reg));
                instrs.push(Instruction::abc(OpCode::Test, cmp, 0, 0));
                let jmp_ok = instrs.len();
                instrs.push(Instruction::sax(OpCode::Jmp, 0));
                // Null case: halt
                let msg_idx = consts.len() as u16;
                consts.push(Constant::String("null assertion failed".to_string()));
                let msg_reg = ra.alloc_temp();
                instrs.push(Instruction::abx(OpCode::LoadK, msg_reg, msg_idx));
                instrs.push(Instruction::abc(OpCode::Halt, msg_reg, 0, 0));
                // Patch jump
                let after = instrs.len();
                instrs[jmp_ok] = Instruction::sax(OpCode::Jmp, (after - jmp_ok - 1) as i32);
                ir
            }
            Expr::SpreadExpr(inner, _) => {
                // Spread produces a list by iterating the inner value.
                // Emit: result = []; for i in 0..len(inner) { append(result, inner[i]) }
                let src_reg = self.lower_expr(inner, ra, consts, instrs);
                let dest = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::NewList, dest, 0, 0));

                let idx_reg = ra.alloc_temp();
                let len_reg = ra.alloc_temp();

                let zero_idx = consts.len() as u16;
                consts.push(Constant::Int(0));
                instrs.push(Instruction::abx(OpCode::LoadK, idx_reg, zero_idx));
                instrs.push(Instruction::abc(
                    OpCode::Intrinsic,
                    len_reg,
                    IntrinsicId::Length as u8,
                    src_reg,
                ));

                let loop_start = instrs.len();
                let lt_reg = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::Lt, lt_reg, idx_reg, len_reg));
                instrs.push(Instruction::abc(OpCode::Test, lt_reg, 0, 0));
                let break_jmp = instrs.len();
                instrs.push(Instruction::sax(OpCode::Jmp, 0));

                let elem_reg = ra.alloc_temp();
                instrs.push(Instruction::abc(
                    OpCode::GetIndex,
                    elem_reg,
                    src_reg,
                    idx_reg,
                ));
                instrs.push(Instruction::abc(OpCode::Append, dest, elem_reg, 0));

                let one_idx = consts.len() as u16;
                consts.push(Constant::Int(1));
                let one_reg = ra.alloc_temp();
                instrs.push(Instruction::abx(OpCode::LoadK, one_reg, one_idx));
                instrs.push(Instruction::abc(OpCode::Add, idx_reg, idx_reg, one_reg));

                let back_offset = loop_start as i32 - instrs.len() as i32 - 1;
                instrs.push(Instruction::sax(OpCode::Jmp, back_offset));

                let after_loop = instrs.len();
                instrs[break_jmp] =
                    Instruction::sax(OpCode::Jmp, (after_loop - break_jmp - 1) as i32);
                dest
            }
            Expr::IfExpr {
                cond,
                then_val,
                else_val,
                ..
            } => {
                let cond_reg = self.lower_expr(cond, ra, consts, instrs);
                let dest = ra.alloc_temp();
                let true_reg = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::LoadBool, true_reg, 1, 0));
                let cmp = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::Eq, cmp, cond_reg, true_reg));
                instrs.push(Instruction::abc(OpCode::Test, cmp, 0, 0));
                let jmp_idx = instrs.len();
                instrs.push(Instruction::sax(OpCode::Jmp, 0));

                let then_reg = self.lower_expr(then_val, ra, consts, instrs);
                instrs.push(Instruction::abc(OpCode::Move, dest, then_reg, 0));
                let else_jmp = instrs.len();
                instrs.push(Instruction::sax(OpCode::Jmp, 0));

                let else_start = instrs.len();
                instrs[jmp_idx] = Instruction::sax(OpCode::Jmp, (else_start - jmp_idx - 1) as i32);
                let else_reg = self.lower_expr(else_val, ra, consts, instrs);
                instrs.push(Instruction::abc(OpCode::Move, dest, else_reg, 0));

                let after = instrs.len();
                instrs[else_jmp] = Instruction::sax(OpCode::Jmp, (after - else_jmp - 1) as i32);
                dest
            }
            Expr::AwaitExpr(inner, _) => {
                let ir = self.lower_expr(inner, ra, consts, instrs);
                let dest = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::Await, dest, ir, 0));
                dest
            }
            Expr::Comprehension {
                body,
                var,
                iter,
                condition,
                kind,
                span: _,
            } => {
                // Lower as: result = new_collection; for var in iter { if cond { add(result, body) } }
                // For sets, build as list first (Append only works on lists), then convert using ToSet intrinsic
                let temp_reg = ra.alloc_temp();
                let build_as_list = matches!(kind, ComprehensionKind::Set);

                match kind {
                    ComprehensionKind::List | ComprehensionKind::Set => {
                        instrs.push(Instruction::abc(OpCode::NewList, temp_reg, 0, 0));
                    }
                    ComprehensionKind::Map => {
                        instrs.push(Instruction::abc(OpCode::NewMap, temp_reg, 0, 0));
                    }
                }

                let iter_reg = self.lower_expr(iter, ra, consts, instrs);
                let idx_reg = ra.alloc_temp();
                let len_reg = ra.alloc_temp();
                let elem_reg = ra.alloc_named(var);

                let zero_idx = consts.len() as u16;
                consts.push(Constant::Int(0));
                instrs.push(Instruction::abx(OpCode::LoadK, idx_reg, zero_idx));
                instrs.push(Instruction::abc(
                    OpCode::Intrinsic,
                    len_reg,
                    IntrinsicId::Length as u8,
                    iter_reg,
                ));

                let loop_start = instrs.len();
                let lt_reg = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::Lt, lt_reg, idx_reg, len_reg));
                instrs.push(Instruction::abc(OpCode::Test, lt_reg, 0, 0));
                let break_jmp = instrs.len();
                instrs.push(Instruction::sax(OpCode::Jmp, 0));

                instrs.push(Instruction::abc(
                    OpCode::GetIndex,
                    elem_reg,
                    iter_reg,
                    idx_reg,
                ));

                // Optional condition
                let mut cond_jmp = None;
                if let Some(ref cond) = condition {
                    let cr = self.lower_expr(cond, ra, consts, instrs);
                    let tr = ra.alloc_temp();
                    instrs.push(Instruction::abc(OpCode::LoadBool, tr, 1, 0));
                    let cmp = ra.alloc_temp();
                    instrs.push(Instruction::abc(OpCode::Eq, cmp, cr, tr));
                    instrs.push(Instruction::abc(OpCode::Test, cmp, 0, 0));
                    cond_jmp = Some(instrs.len());
                    instrs.push(Instruction::sax(OpCode::Jmp, 0));
                }

                let body_reg = self.lower_expr(body, ra, consts, instrs);
                match kind {
                    ComprehensionKind::List | ComprehensionKind::Set => {
                        // Append to list (for both list and set)
                        instrs.push(Instruction::abc(OpCode::Append, temp_reg, body_reg, 0));
                    }
                    ComprehensionKind::Map => {
                        // For map comprehension, body should be a tuple (key, value).
                        // Extract key at index 0 and value at index 1.
                        let zero_reg = ra.alloc_temp();
                        let kidx = consts.len() as u16;
                        consts.push(Constant::Int(0));
                        instrs.push(Instruction::abx(OpCode::LoadK, zero_reg, kidx));
                        let key_reg = ra.alloc_temp();
                        instrs.push(Instruction::abc(
                            OpCode::GetIndex,
                            key_reg,
                            body_reg,
                            zero_reg,
                        ));

                        let one_k = ra.alloc_temp();
                        let kidx2 = consts.len() as u16;
                        consts.push(Constant::Int(1));
                        instrs.push(Instruction::abx(OpCode::LoadK, one_k, kidx2));
                        let val_reg = ra.alloc_temp();
                        instrs.push(Instruction::abc(OpCode::GetIndex, val_reg, body_reg, one_k));

                        instrs.push(Instruction::abc(
                            OpCode::SetIndex,
                            temp_reg,
                            key_reg,
                            val_reg,
                        ));
                    }
                }

                if let Some(cj) = cond_jmp {
                    let after_body = instrs.len();
                    instrs[cj] = Instruction::sax(OpCode::Jmp, (after_body - cj - 1) as i32);
                }

                let one_idx = consts.len() as u16;
                consts.push(Constant::Int(1));
                let one_reg = ra.alloc_temp();
                instrs.push(Instruction::abx(OpCode::LoadK, one_reg, one_idx));
                instrs.push(Instruction::abc(OpCode::Add, idx_reg, idx_reg, one_reg));

                let back_offset = loop_start as i32 - instrs.len() as i32 - 1;
                instrs.push(Instruction::sax(OpCode::Jmp, back_offset));

                let after_loop = instrs.len();
                instrs[break_jmp] =
                    Instruction::sax(OpCode::Jmp, (after_loop - break_jmp - 1) as i32);

                // For set comprehensions, convert the list to a set using ToSet intrinsic
                if build_as_list {
                    let dest = ra.alloc_temp();
                    instrs.push(Instruction::abc(
                        OpCode::Intrinsic,
                        dest,
                        IntrinsicId::ToSet as u8,
                        temp_reg,
                    ));
                    dest
                } else {
                    temp_reg
                }
            }
            Expr::MatchExpr { subject, arms, .. } => {
                let subj_reg = self.lower_expr(subject, ra, consts, instrs);
                let dest = ra.alloc_temp();
                let mut end_jumps = Vec::new();

                for arm in arms {
                    let mut fail_jumps = Vec::new();
                    self.lower_match_pattern(
                        &arm.pattern,
                        subj_reg,
                        ra,
                        consts,
                        instrs,
                        &mut fail_jumps,
                    );

                    // Lower arm body; last statement's value goes into dest
                    for (idx, s) in arm.body.iter().enumerate() {
                        if idx == arm.body.len() - 1 {
                            if let Stmt::Expr(es) = s {
                                let val = self.lower_expr(&es.expr, ra, consts, instrs);
                                instrs.push(Instruction::abc(OpCode::Move, dest, val, 0));
                            } else if let Stmt::Return(rs) = s {
                                let val = self.lower_expr(&rs.value, ra, consts, instrs);
                                instrs.push(Instruction::abc(OpCode::Return, val, 1, 0));
                            } else {
                                self.lower_stmt(s, ra, consts, instrs);
                            }
                        } else {
                            self.lower_stmt(s, ra, consts, instrs);
                        }
                    }
                    end_jumps.push(instrs.len());
                    instrs.push(Instruction::sax(OpCode::Jmp, 0));

                    let next_arm = instrs.len();
                    for j in fail_jumps {
                        instrs[j] = Instruction::sax(OpCode::Jmp, (next_arm - j - 1) as i32);
                    }
                }

                let end = instrs.len();
                for jmp_idx in end_jumps {
                    instrs[jmp_idx] = Instruction::sax(OpCode::Jmp, (end - jmp_idx - 1) as i32);
                }
                dest
            }
            Expr::IsType {
                expr: inner,
                type_name,
                ..
            } => {
                let val_reg = self.lower_expr(inner, ra, consts, instrs);
                let type_str_reg = ra.alloc_temp();
                let kidx = consts.len() as u16;
                consts.push(Constant::String(type_name.clone()));
                instrs.push(Instruction::abx(OpCode::LoadK, type_str_reg, kidx));
                let dest = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::Is, dest, val_reg, type_str_reg));
                dest
            }
            Expr::TypeCast {
                expr: inner,
                target_type,
                ..
            } => {
                let val_reg = self.lower_expr(inner, ra, consts, instrs);
                let dest = ra.alloc_temp();
                let intrinsic = match target_type.as_str() {
                    "Int" => IntrinsicId::ToInt,
                    "Float" => IntrinsicId::ToFloat,
                    "String" => IntrinsicId::ToString,
                    "Bool" => {
                        // Truthiness check: not(not(val))
                        instrs.push(Instruction::abc(OpCode::Not, dest, val_reg, 0));
                        instrs.push(Instruction::abc(OpCode::Not, dest, dest, 0));
                        return dest;
                    }
                    _ => {
                        // Unknown cast: just move the value through
                        instrs.push(Instruction::abc(OpCode::Move, dest, val_reg, 0));
                        return dest;
                    }
                };
                // Emit intrinsic call: dest = intrinsic(val_reg)
                let arg_start = dest + 1;
                if val_reg != arg_start {
                    instrs.push(Instruction::abc(OpCode::Move, arg_start, val_reg, 0));
                }
                instrs.push(Instruction::abc(
                    OpCode::Intrinsic,
                    dest,
                    intrinsic as u8,
                    arg_start,
                ));
                dest
            }
            Expr::BlockExpr(stmts, _) => {
                let dest = ra.alloc_temp();
                for (idx, s) in stmts.iter().enumerate() {
                    if idx == stmts.len() - 1 {
                        if let Stmt::Expr(es) = s {
                            let val = self.lower_expr(&es.expr, ra, consts, instrs);
                            instrs.push(Instruction::abc(OpCode::Move, dest, val, 0));
                        } else if let Stmt::Return(rs) = s {
                            let val = self.lower_expr(&rs.value, ra, consts, instrs);
                            instrs.push(Instruction::abc(OpCode::Return, val, 1, 0));
                        } else {
                            self.lower_stmt(s, ra, consts, instrs);
                            instrs.push(Instruction::abc(OpCode::LoadNil, dest, 0, 0));
                        }
                    } else {
                        self.lower_stmt(s, ra, consts, instrs);
                    }
                }
                if stmts.is_empty() {
                    instrs.push(Instruction::abc(OpCode::LoadNil, dest, 0, 0));
                }
                dest
            }
        }
    }

    fn lower_tool_call(
        &mut self,
        alias: Option<&str>,
        args: &[CallArg],
        ra: &mut RegAlloc,
        consts: &mut Vec<Constant>,
        instrs: &mut Vec<Instruction>,
    ) -> u8 {
        let mut kv_regs = Vec::new();
        for (idx, arg) in args.iter().enumerate() {
            let (key, value_reg) = match arg {
                CallArg::Named(name, e, _) => {
                    (name.clone(), self.lower_expr(e, ra, consts, instrs))
                }
                CallArg::Positional(e) => (
                    format!("arg{}", idx),
                    self.lower_expr(e, ra, consts, instrs),
                ),
                CallArg::Role(name, content, _) => {
                    let content_reg = self.lower_expr(content, ra, consts, instrs);
                    let prefix_reg = ra.alloc_temp();
                    let kidx = consts.len() as u16;
                    consts.push(Constant::String(format!("{}: ", name)));
                    instrs.push(Instruction::abx(OpCode::LoadK, prefix_reg, kidx));
                    let rendered_reg = ra.alloc_temp();
                    instrs.push(Instruction::abc(
                        OpCode::Concat,
                        rendered_reg,
                        prefix_reg,
                        content_reg,
                    ));
                    (name.clone(), rendered_reg)
                }
            };

            let key_reg = ra.alloc_temp();
            let key_idx = consts.len() as u16;
            consts.push(Constant::String(key));
            instrs.push(Instruction::abx(OpCode::LoadK, key_reg, key_idx));

            let value_copy_reg = ra.alloc_temp();
            if value_reg != value_copy_reg {
                instrs.push(Instruction::abc(OpCode::Move, value_copy_reg, value_reg, 0));
            }
            kv_regs.push((key_reg, value_copy_reg));
        }

        let dest = ra.alloc_temp();
        for _ in 0..(kv_regs.len() * 2) {
            ra.alloc_temp();
        }

        for (i, (key_reg, value_reg)) in kv_regs.iter().enumerate() {
            let key_target = dest + 1 + (i as u8) * 2;
            let value_target = key_target + 1;
            if *key_reg != key_target {
                instrs.push(Instruction::abc(OpCode::Move, key_target, *key_reg, 0));
            }
            if *value_reg != value_target {
                instrs.push(Instruction::abc(OpCode::Move, value_target, *value_reg, 0));
            }
        }

        instrs.push(Instruction::abc(
            OpCode::NewMap,
            dest,
            kv_regs.len() as u8,
            0,
        ));
        let tool_index = alias
            .and_then(|name| self.tool_indices.get(name).copied())
            .unwrap_or(0);
        instrs.push(Instruction::abx(OpCode::ToolCall, dest, tool_index));
        dest
    }
}

/// Collect identifiers referenced in an expression (non-recursive into lambdas).
fn collect_free_idents_expr(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Ident(name, _) => out.push(name.clone()),
        Expr::BinOp(lhs, _, rhs, _) => {
            collect_free_idents_expr(lhs, out);
            collect_free_idents_expr(rhs, out);
        }
        Expr::Pipe { left, right, .. } => {
            collect_free_idents_expr(left, out);
            collect_free_idents_expr(right, out);
        }
        Expr::Illuminate {
            input, transform, ..
        } => {
            collect_free_idents_expr(input, out);
            collect_free_idents_expr(transform, out);
        }
        Expr::UnaryOp(_, inner, _) => collect_free_idents_expr(inner, out),
        Expr::Call(callee, args, _) | Expr::ToolCall(callee, args, _) => {
            collect_free_idents_expr(callee, out);
            for arg in args {
                match arg {
                    CallArg::Positional(e) | CallArg::Named(_, e, _) | CallArg::Role(_, e, _) => {
                        collect_free_idents_expr(e, out);
                    }
                }
            }
        }
        Expr::DotAccess(obj, _, _) => collect_free_idents_expr(obj, out),
        Expr::IndexAccess(obj, idx, _) => {
            collect_free_idents_expr(obj, out);
            collect_free_idents_expr(idx, out);
        }
        Expr::ListLit(elems, _) | Expr::TupleLit(elems, _) | Expr::SetLit(elems, _) => {
            for e in elems {
                collect_free_idents_expr(e, out);
            }
        }
        Expr::MapLit(pairs, _) => {
            for (k, v) in pairs {
                collect_free_idents_expr(k, out);
                collect_free_idents_expr(v, out);
            }
        }
        Expr::RecordLit(_, fields, _) => {
            for (_, v) in fields {
                collect_free_idents_expr(v, out);
            }
        }
        Expr::StringInterp(segs, _) => {
            for seg in segs {
                if let StringSegment::Interpolation(e) = seg {
                    collect_free_idents_expr(e, out);
                }
            }
        }
        Expr::IfExpr {
            cond,
            then_val,
            else_val,
            ..
        } => {
            collect_free_idents_expr(cond, out);
            collect_free_idents_expr(then_val, out);
            collect_free_idents_expr(else_val, out);
        }
        Expr::NullCoalesce(l, r, _) => {
            collect_free_idents_expr(l, out);
            collect_free_idents_expr(r, out);
        }
        Expr::NullSafeAccess(obj, _, _) => collect_free_idents_expr(obj, out),
        Expr::NullSafeIndex(obj, idx, _) => {
            collect_free_idents_expr(obj, out);
            collect_free_idents_expr(idx, out);
        }
        Expr::NullAssert(inner, _)
        | Expr::SpreadExpr(inner, _)
        | Expr::TryExpr(inner, _)
        | Expr::AwaitExpr(inner, _) => {
            collect_free_idents_expr(inner, out);
        }
        Expr::ExpectSchema(inner, _, _) => collect_free_idents_expr(inner, out),
        Expr::IsType { expr: inner, .. } | Expr::TypeCast { expr: inner, .. } => {
            collect_free_idents_expr(inner, out);
        }
        Expr::RoleBlock(_, content, _) => collect_free_idents_expr(content, out),
        Expr::RangeExpr { start, end, .. } => {
            if let Some(s) = start {
                collect_free_idents_expr(s, out);
            }
            if let Some(e) = end {
                collect_free_idents_expr(e, out);
            }
        }
        Expr::Comprehension {
            body,
            iter,
            condition,
            ..
        } => {
            collect_free_idents_expr(body, out);
            collect_free_idents_expr(iter, out);
            if let Some(c) = condition {
                collect_free_idents_expr(c, out);
            }
        }
        Expr::Lambda { body, .. } => {
            // Don't recurse into nested lambdas - they handle their own captures
            match body {
                LambdaBody::Expr(e) => collect_free_idents_expr(e, out),
                LambdaBody::Block(stmts) => {
                    for s in stmts {
                        collect_free_idents_stmt(s, out);
                    }
                }
            }
        }
        Expr::MatchExpr { subject, arms, .. } => {
            collect_free_idents_expr(subject, out);
            for arm in arms {
                for s in &arm.body {
                    collect_free_idents_stmt(s, out);
                }
            }
        }
        Expr::BlockExpr(stmts, _) => {
            for s in stmts {
                collect_free_idents_stmt(s, out);
            }
        }
        _ => {} // literals, etc.
    }
}

fn collect_free_idents_stmt(stmt: &Stmt, out: &mut Vec<String>) {
    match stmt {
        Stmt::Let(ls) => collect_free_idents_expr(&ls.value, out),
        Stmt::Assign(a) => collect_free_idents_expr(&a.value, out),
        Stmt::Return(r) => collect_free_idents_expr(&r.value, out),
        Stmt::Halt(h) => collect_free_idents_expr(&h.message, out),
        Stmt::Expr(e) => collect_free_idents_expr(&e.expr, out),
        Stmt::Emit(e) => collect_free_idents_expr(&e.value, out),
        Stmt::If(ifs) => {
            collect_free_idents_expr(&ifs.condition, out);
            for s in &ifs.then_body {
                collect_free_idents_stmt(s, out);
            }
            if let Some(ref eb) = ifs.else_body {
                for s in eb {
                    collect_free_idents_stmt(s, out);
                }
            }
        }
        Stmt::While(ws) => {
            collect_free_idents_expr(&ws.condition, out);
            for s in &ws.body {
                collect_free_idents_stmt(s, out);
            }
        }
        Stmt::For(fs) => {
            collect_free_idents_expr(&fs.iter, out);
            if let Some(filter) = &fs.filter {
                collect_free_idents_expr(filter, out);
            }
            for s in &fs.body {
                collect_free_idents_stmt(s, out);
            }
        }
        Stmt::Loop(ls) => {
            for s in &ls.body {
                collect_free_idents_stmt(s, out);
            }
        }
        Stmt::Match(ms) => {
            collect_free_idents_expr(&ms.subject, out);
            for arm in &ms.arms {
                for s in &arm.body {
                    collect_free_idents_stmt(s, out);
                }
            }
        }
        Stmt::CompoundAssign(ca) => collect_free_idents_expr(&ca.value, out),
        Stmt::Defer(ds) => {
            for s in &ds.body {
                collect_free_idents_stmt(s, out);
            }
        }
        _ => {}
    }
}

fn format_type_expr(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Named(n, _) => n.clone(),
        TypeExpr::List(inner, _) => format!("list[{}]", format_type_expr(inner)),
        TypeExpr::Map(k, v, _) => format!("map[{}, {}]", format_type_expr(k), format_type_expr(v)),
        TypeExpr::Result(ok, err, _) => format!(
            "result[{}, {}]",
            format_type_expr(ok),
            format_type_expr(err)
        ),
        TypeExpr::Union(types, _) => types
            .iter()
            .map(format_type_expr)
            .collect::<Vec<_>>()
            .join(" | "),
        TypeExpr::Null(_) => "Null".to_string(),
        TypeExpr::Tuple(types, _) => {
            let inner: Vec<_> = types.iter().map(format_type_expr).collect();
            format!("({})", inner.join(", "))
        }
        TypeExpr::Set(inner, _) => format!("set[{}]", format_type_expr(inner)),
        TypeExpr::Fn(params, ret, _, _) => {
            let ps: Vec<_> = params.iter().map(format_type_expr).collect();
            format!("fn({}) -> {}", ps.join(", "), format_type_expr(ret))
        }
        TypeExpr::Generic(name, args, _) => {
            let as_: Vec<_> = args.iter().map(format_type_expr).collect();
            format!("{}[{}]", name, as_.join(", "))
        }
    }
}

fn patch_lambda_closure_indices(cell: &mut LirCell, lambda_base: u16) {
    for instr in &mut cell.instructions {
        if instr.op == OpCode::Closure {
            let patched = instr.bx().saturating_add(lambda_base);
            *instr = Instruction::abx(OpCode::Closure, instr.a, patched);
        }
    }
}

fn encode_machine_expr(expr: &Expr) -> Option<serde_json::Value> {
    match expr {
        Expr::IntLit(n, _) => Some(serde_json::json!({"kind": "int", "value": n})),
        Expr::FloatLit(f, _) => Some(serde_json::json!({"kind": "float", "value": f})),
        Expr::StringLit(s, _) => Some(serde_json::json!({"kind": "string", "value": s})),
        Expr::BoolLit(b, _) => Some(serde_json::json!({"kind": "bool", "value": b})),
        Expr::NullLit(_) => Some(serde_json::json!({"kind": "null"})),
        Expr::Ident(name, _) => Some(serde_json::json!({"kind": "ident", "value": name})),
        Expr::UnaryOp(op, inner, _) => {
            let inner = encode_machine_expr(inner)?;
            let op = match op {
                UnaryOp::Neg => "-",
                UnaryOp::Not => "not",
                UnaryOp::BitNot => "~",
            };
            Some(serde_json::json!({"kind": "unary", "op": op, "expr": inner}))
        }
        Expr::BinOp(lhs, op, rhs, _) => {
            let left = encode_machine_expr(lhs)?;
            let right = encode_machine_expr(rhs)?;
            let op = match op {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "/",
                BinOp::FloorDiv => "//",
                BinOp::Mod => "%",
                BinOp::Eq => "==",
                BinOp::NotEq => "!=",
                BinOp::Lt => "<",
                BinOp::LtEq => "<=",
                BinOp::Gt => ">",
                BinOp::GtEq => ">=",
                BinOp::And => "and",
                BinOp::Or => "or",
                BinOp::Pow => "**",
                BinOp::In => "in",
                BinOp::Concat => "++",
                BinOp::BitAnd => "&",
                BinOp::BitOr => "|",
                BinOp::BitXor => "^",
                BinOp::Shl => "<<",
                BinOp::Shr => ">>",
                BinOp::PipeForward => "|>",
            };
            Some(serde_json::json!({
                "kind": "bin",
                "op": op,
                "lhs": left,
                "rhs": right
            }))
        }
        _ => None,
    }
}

fn pipeline_stage_callee_expr(stage: &str, span: Span) -> Expr {
    let mut parts = stage.split('.');
    let first = parts
        .next()
        .map(|s| s.to_string())
        .unwrap_or_else(|| stage.to_string());
    let mut expr = Expr::Ident(first, span);
    for part in parts {
        expr = Expr::DotAccess(Box::new(expr), part.to_string(), span);
    }
    expr
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::lexer::Lexer;
    use crate::compiler::parser::Parser;
    use crate::compiler::resolve;

    fn lower_src(src: &str) -> LirModule {
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let prog = parser.parse_program(vec![]).unwrap();
        let symbols = resolve::resolve(&prog).unwrap();
        lower(&prog, &symbols, src)
    }

    #[test]
    fn test_lower_simple_cell() {
        let module = lower_src("cell main() -> Int\n  return 42\nend");
        assert_eq!(module.cells.len(), 1);
        assert_eq!(module.cells[0].name, "main");
        assert!(!module.cells[0].instructions.is_empty());
    }

    #[test]
    fn test_lower_arithmetic() {
        let module = lower_src("cell add(a: Int, b: Int) -> Int\n  return a + b\nend");
        assert_eq!(module.cells[0].params.len(), 2);
        // Should have: ADD, RETURN instructions
        let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
        assert!(ops.contains(&OpCode::Add));
        assert!(ops.contains(&OpCode::Return));
    }

    #[test]
    fn test_noteq_emits_eq_then_not() {
        let module = lower_src("cell neq(a: Int, b: Int) -> Bool\n  return a != b\nend");
        let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
        // Should have Eq followed by Not
        let eq_idx = ops
            .iter()
            .position(|o| *o == OpCode::Eq)
            .expect("expected Eq opcode");
        assert_eq!(
            ops[eq_idx + 1],
            OpCode::Not,
            "Not should follow Eq for NotEq"
        );
        assert!(ops.contains(&OpCode::Return));
    }

    #[test]
    fn test_if_else_emits_jmp_test() {
        let src =
            "cell check(x: Bool) -> Int\n  if x\n    return 1\n  else\n    return 0\n  end\nend";
        let module = lower_src(src);
        let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
        // Should have: Eq, Test, Jmp sequence for condition check
        assert!(ops.contains(&OpCode::Eq), "if should emit Eq");
        assert!(ops.contains(&OpCode::Test), "if should emit Test");
        assert!(ops.contains(&OpCode::Jmp), "if should emit Jmp");
        // Should have two Returns (one for each branch)
        let ret_count = ops.iter().filter(|o| **o == OpCode::Return).count();
        assert!(
            ret_count >= 2,
            "if/else should have returns in both branches"
        );
    }

    #[test]
    fn test_while_loop_emits_signed_backward_jump() {
        let src = "cell loop_test(n: Int) -> Int\n  let x = 0\n  while x < n\n    x = x + 1\n  end\n  return x\nend";
        let module = lower_src(src);
        let instrs = &module.cells[0].instructions;
        // Find all Jmp instructions
        let jmps: Vec<(usize, i32)> = instrs
            .iter()
            .enumerate()
            .filter(|(_, i)| i.op == OpCode::Jmp)
            .map(|(idx, i)| (idx, i.sax_val()))
            .collect();
        // At least one backward jump (negative offset)
        assert!(
            jmps.iter().any(|(_, offset)| *offset < 0),
            "while loop should have a backward jump, got: {:?}",
            jmps
        );
    }

    #[test]
    fn test_match_literal_emits_eq_test_jmp() {
        let src = "cell check(x: Int) -> String\n  match x\n    1 -> return \"one\"\n    2 -> return \"two\"\n    _ -> return \"other\"\n  end\nend";
        let module = lower_src(src);
        let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
        // Match literal should emit Eq for comparison
        assert!(ops.contains(&OpCode::Eq), "match literal should emit Eq");
        // Should have Test+Jmp for conditional branching
        assert!(
            ops.contains(&OpCode::Test),
            "match literal should emit Test"
        );
        assert!(ops.contains(&OpCode::Jmp), "match literal should emit Jmp");
    }

    #[test]
    fn test_string_interpolation_emits_concat_chain() {
        let src = "cell greet(name: String) -> String\n  return \"hello #{name}!\"\nend";
        let module = lower_src(src);
        let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
        // String interpolation should produce Concat instructions
        let concat_count = ops.iter().filter(|o| **o == OpCode::Concat).count();
        assert!(
            concat_count >= 1,
            "string interpolation should emit at least one Concat, got {}",
            concat_count
        );
    }

    #[test]
    fn test_record_construction_emits_newrecord_setindex() {
        let src = "record Point\n  x: Int\n  y: Int\nend\n\ncell make() -> Point\n  return Point(x: 1, y: 2)\nend";
        let module = lower_src(src);
        let make_cell = module.cells.iter().find(|c| c.name == "make").unwrap();
        let ops: Vec<_> = make_cell.instructions.iter().map(|i| i.op).collect();
        assert!(
            ops.contains(&OpCode::NewRecord),
            "record construction should emit NewRecord"
        );
        // SetIndex is used for setting fields (via emit_set_field)
        let setindex_count = ops.iter().filter(|o| **o == OpCode::SetIndex).count();
        assert!(
            setindex_count >= 2,
            "record with 2 fields should emit at least 2 SetIndex, got {}",
            setindex_count
        );
    }

    #[test]
    fn test_set_comprehension_emits_newset() {
        let src =
            "cell make_set() -> set[Int]\n  let xs = [1, 2, 3]\n  return {x for x in xs}\nend";
        let module = lower_src(src);
        let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
        // Set comprehension now builds as list then converts using ToSet intrinsic
        assert!(
            ops.contains(&OpCode::Append),
            "set comprehension should emit Append"
        );
        let intrinsics: Vec<u8> = module.cells[0]
            .instructions
            .iter()
            .filter(|i| i.op == OpCode::Intrinsic)
            .map(|i| i.b)
            .collect();
        assert!(
            intrinsics.contains(&(IntrinsicId::ToSet as u8)),
            "set comprehension should use ToSet intrinsic"
        );
    }

    #[test]
    fn test_lambda_with_capture_emits_getupval() {
        let src = "cell make_adder(x: Int) -> fn(Int) -> Int\n  return fn(y: Int) => x + y\nend";
        let module = lower_src(src);
        // The lambda should be emitted as a separate cell
        let lambda_cell = module
            .cells
            .iter()
            .find(|c| c.name.starts_with("<lambda/"))
            .unwrap();
        let ops: Vec<_> = lambda_cell.instructions.iter().map(|i| i.op).collect();
        assert!(
            ops.contains(&OpCode::GetUpval),
            "lambda capturing outer variable should emit GetUpval"
        );
        // The outer cell should have Closure and SetUpval
        let outer_cell = module
            .cells
            .iter()
            .find(|c| c.name == "make_adder")
            .unwrap();
        let outer_ops: Vec<_> = outer_cell.instructions.iter().map(|i| i.op).collect();
        assert!(
            outer_ops.contains(&OpCode::Closure),
            "should emit Closure for lambda"
        );
        assert!(
            outer_ops.contains(&OpCode::SetUpval),
            "should emit SetUpval for captured variable"
        );
    }

    #[test]
    fn test_for_loop_emits_iteration() {
        let src = "cell sum_list(xs: list[Int]) -> Int\n  let total = 0\n  for x in xs\n    total = total + x\n  end\n  return total\nend";
        let module = lower_src(src);
        let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
        assert!(
            ops.contains(&OpCode::Lt),
            "for loop should emit Lt for bound check"
        );
        assert!(
            ops.contains(&OpCode::GetIndex),
            "for loop should emit GetIndex for element access"
        );
        assert!(ops.contains(&OpCode::Add), "for loop body should emit Add");
        // Should have backward jump
        let instrs = &module.cells[0].instructions;
        let has_backward_jmp = instrs
            .iter()
            .any(|i| i.op == OpCode::Jmp && i.sax_val() < 0);
        assert!(has_backward_jmp, "for loop should have backward jump");
    }

    #[test]
    fn test_intrinsic_sort_maps_correctly() {
        let src = "cell test_sort(xs: list[Int]) -> list[Int]\n  return sort(xs)\nend";
        let module = lower_src(src);
        let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
        assert!(
            ops.contains(&OpCode::Intrinsic),
            "sort should emit Intrinsic opcode"
        );
        // Check the intrinsic ID
        let intr = module.cells[0]
            .instructions
            .iter()
            .find(|i| i.op == OpCode::Intrinsic)
            .unwrap();
        assert_eq!(
            intr.b,
            IntrinsicId::Sort as u8,
            "sort should use Sort intrinsic ID"
        );
    }

    #[test]
    fn test_intrinsic_filter_maps_correctly() {
        let src = "cell test_filter(xs: list[Int], f: fn(Int) -> Bool) -> list[Int]\n  return filter(xs, f)\nend";
        let module = lower_src(src);
        let intr = module.cells[0]
            .instructions
            .iter()
            .find(|i| i.op == OpCode::Intrinsic)
            .unwrap();
        assert_eq!(
            intr.b,
            IntrinsicId::Filter as u8,
            "filter should use Filter intrinsic ID"
        );
    }

    #[test]
    fn test_break_continue_in_while() {
        let src = "cell test() -> Int\n  let i = 0\n  while true\n    i = i + 1\n    if i > 10\n      break\n    end\n  end\n  return i\nend";
        let module = lower_src(src);
        let instrs = &module.cells[0].instructions;
        // Should have both forward jumps (break) and backward jumps (while loop back)
        let jmps: Vec<i32> = instrs
            .iter()
            .filter(|i| i.op == OpCode::Jmp)
            .map(|i| i.sax_val())
            .collect();
        assert!(
            jmps.iter().any(|o| *o < 0),
            "while should have backward jump"
        );
        assert!(
            jmps.iter().any(|o| *o > 0),
            "break should have forward jump"
        );
    }

    #[test]
    fn test_list_literal() {
        let module = lower_src("cell make() -> list[Int]\n  return [1, 2, 3]\nend");
        let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
        assert!(
            ops.contains(&OpCode::NewList),
            "list literal should emit NewList"
        );
    }

    #[test]
    fn test_intrinsic_matches_maps_correctly() {
        let src =
            "cell test_matches(s: String, pat: String) -> Bool\n  return matches(s, pat)\nend";
        let module = lower_src(src);
        let intr = module.cells[0]
            .instructions
            .iter()
            .find(|i| i.op == OpCode::Intrinsic)
            .unwrap();
        assert_eq!(
            intr.b,
            IntrinsicId::Matches as u8,
            "matches should use Matches intrinsic ID"
        );
    }

    #[test]
    fn test_intrinsic_trace_ref_maps_correctly() {
        let src = "cell test_trace() -> String\n  return trace_ref()\nend";
        let module = lower_src(src);
        let intr = module.cells[0]
            .instructions
            .iter()
            .find(|i| i.op == OpCode::Intrinsic)
            .unwrap();
        assert_eq!(
            intr.b,
            IntrinsicId::TraceRef as u8,
            "trace_ref should use TraceRef intrinsic ID"
        );
    }

    #[test]
    fn test_noteq_result_register_distinct_from_operands() {
        // Verify that NotEq emits Eq into a fresh dest, then Not into the same dest,
        // and neither clobbers the operand registers.
        let module = lower_src("cell neq2(a: Int, b: Int) -> Bool\n  return a != b\nend");
        let instrs = &module.cells[0].instructions;
        let eq_instr = instrs.iter().find(|i| i.op == OpCode::Eq).unwrap();
        let not_instr = instrs.iter().find(|i| i.op == OpCode::Not).unwrap();
        // Eq writes to dest (a field), Not reads from and writes to the same register
        assert_eq!(
            eq_instr.a, not_instr.b,
            "Not should read from Eq's dest register"
        );
        assert_eq!(
            eq_instr.a, not_instr.a,
            "Not should write to the same register as Eq's dest"
        );
        // dest should not be the same as operand registers
        assert_ne!(
            eq_instr.a, eq_instr.b,
            "Eq dest should not clobber lhs operand"
        );
        assert_ne!(
            eq_instr.a, eq_instr.c,
            "Eq dest should not clobber rhs operand"
        );
    }

    #[test]
    fn test_noteq_in_if_condition_emits_inversion_before_test() {
        let src = "cell pick(a: Int, b: Int) -> Int\n  if a != b\n    return 1\n  else\n    return 0\n  end\nend";
        let module = lower_src(src);
        let instrs = &module.cells[0].instructions;

        let found_noteq_inversion_pair = instrs.windows(2).any(|window| {
            let eq = &window[0];
            let not = &window[1];
            eq.op == OpCode::Eq
                && not.op == OpCode::Not
                && eq.a == not.a
                && eq.a == not.b
                && eq.a != eq.b
                && eq.a != eq.c
        });
        assert!(
            found_noteq_inversion_pair,
            "expected NotEq condition to lower as Eq(dest, lhs, rhs) then Not(dest, dest)"
        );

        assert!(
            instrs.iter().any(|i| i.op == OpCode::Test),
            "if condition should emit Test"
        );
    }

    #[test]
    fn test_closure_capture_count_matches_setupval() {
        // Verify that the number of SetUpval instructions matches the number of captures
        let src = "cell make_adder(x: Int) -> fn(Int) -> Int\n  return fn(y: Int) => x + y\nend";
        let module = lower_src(src);
        let outer_cell = module
            .cells
            .iter()
            .find(|c| c.name == "make_adder")
            .unwrap();
        let setupval_count = outer_cell
            .instructions
            .iter()
            .filter(|i| i.op == OpCode::SetUpval)
            .count();
        let lambda_cell = module
            .cells
            .iter()
            .find(|c| c.name.starts_with("<lambda/"))
            .unwrap();
        let getupval_count = lambda_cell
            .instructions
            .iter()
            .filter(|i| i.op == OpCode::GetUpval)
            .count();
        assert_eq!(
            setupval_count, getupval_count,
            "SetUpval count ({}) should match GetUpval count ({})",
            setupval_count, getupval_count
        );
        assert_eq!(setupval_count, 1, "should have exactly 1 capture (x)");
    }

    #[test]
    fn test_closure_multiple_captures() {
        let src = "cell make_fn(a: Int, b: String) -> fn() -> String\n  return fn() => \"#{a} #{b}\"\nend";
        let module = lower_src(src);
        let outer_cell = module.cells.iter().find(|c| c.name == "make_fn").unwrap();
        let setupval_count = outer_cell
            .instructions
            .iter()
            .filter(|i| i.op == OpCode::SetUpval)
            .count();
        let lambda_cell = module
            .cells
            .iter()
            .find(|c| c.name.starts_with("<lambda/"))
            .unwrap();
        let getupval_count = lambda_cell
            .instructions
            .iter()
            .filter(|i| i.op == OpCode::GetUpval)
            .count();
        assert_eq!(setupval_count, 2, "should capture both a and b");
        assert_eq!(getupval_count, 2, "lambda should load both captures");
    }

    #[test]
    fn test_closure_no_capture_of_own_params() {
        // Lambda params should not be captured as upvalues
        let src = "cell identity() -> fn(Int) -> Int\n  return fn(x: Int) => x\nend";
        let module = lower_src(src);
        let outer_cell = module.cells.iter().find(|c| c.name == "identity").unwrap();
        let setupval_count = outer_cell
            .instructions
            .iter()
            .filter(|i| i.op == OpCode::SetUpval)
            .count();
        assert_eq!(
            setupval_count, 0,
            "lambda using only its own params should have no captures"
        );
    }

    #[test]
    fn test_spread_expr_emits_iteration_loop() {
        let src = "cell test_spread(xs: list[Int]) -> list[Int]\n  return [...xs]\nend";
        let module = lower_src(src);
        let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
        assert!(
            ops.contains(&OpCode::NewList),
            "spread in list should create initial list"
        );
        assert!(
            ops.contains(&OpCode::Append),
            "spread should emit Append for each element"
        );
        assert!(
            ops.contains(&OpCode::GetIndex),
            "spread loop should emit GetIndex"
        );
        // Should have a backward jump for the loop
        let instrs = &module.cells[0].instructions;
        let has_backward_jmp = instrs
            .iter()
            .any(|i| i.op == OpCode::Jmp && i.sax_val() < 0);
        assert!(
            has_backward_jmp,
            "spread should have iteration loop with backward jump"
        );
    }

    #[test]
    fn test_gt_swaps_operands() {
        // a > b should lower as Lt(dest, b, a) — swapped operands
        let module = lower_src("cell gt(a: Int, b: Int) -> Bool\n  return a > b\nend");
        let instrs = &module.cells[0].instructions;
        let lt_instr = instrs.iter().find(|i| i.op == OpCode::Lt).unwrap();
        // params: a=r0, b=r1; Lt should be Lt(dest, r1, r0) i.e. b < a
        assert_eq!(
            lt_instr.b, 1,
            "Gt should swap: Lt first operand should be b (r1)"
        );
        assert_eq!(
            lt_instr.c, 0,
            "Gt should swap: Lt second operand should be a (r0)"
        );
    }

    #[test]
    fn test_gte_swaps_operands() {
        // a >= b should lower as Le(dest, b, a) — swapped operands
        let module = lower_src("cell gte(a: Int, b: Int) -> Bool\n  return a >= b\nend");
        let instrs = &module.cells[0].instructions;
        let le_instr = instrs.iter().find(|i| i.op == OpCode::Le).unwrap();
        assert_eq!(
            le_instr.b, 1,
            "GtEq should swap: Le first operand should be b (r1)"
        );
        assert_eq!(
            le_instr.c, 0,
            "GtEq should swap: Le second operand should be a (r0)"
        );
    }

    #[test]
    fn test_set_comprehension_uses_toset_intrinsic() {
        let src =
            "cell make_set() -> set[Int]\n  let xs = [1, 2, 3]\n  return {x for x in xs}\nend";
        let module = lower_src(src);
        let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
        // Set comprehension should build as list then convert using ToSet intrinsic
        assert!(
            ops.contains(&OpCode::Append),
            "set comprehension should use Append during iteration"
        );
        let intrinsics: Vec<u8> = module.cells[0]
            .instructions
            .iter()
            .filter(|i| i.op == OpCode::Intrinsic)
            .map(|i| i.b)
            .collect();
        assert!(
            intrinsics.contains(&(IntrinsicId::ToSet as u8)),
            "set comprehension should use ToSet intrinsic to convert list to set"
        );
    }

    #[test]
    fn test_list_with_spread_uses_append() {
        let src = "cell merge() -> list[Int]\n  let a = [1, 2]\n  let b = [3, 4]\n  return [0, ...a, ...b, 5]\nend";
        let module = lower_src(src);
        let ops: Vec<_> = module.cells[0].instructions.iter().map(|i| i.op).collect();
        // List with spread should use Append-based construction
        assert!(
            ops.contains(&OpCode::Append),
            "list with spread should use Append"
        );
        // Should iterate over spread values
        assert!(
            ops.contains(&OpCode::GetIndex),
            "should iterate over spread values"
        );
        assert!(
            ops.contains(&OpCode::Lt),
            "should check bounds during spread iteration"
        );
    }
}
