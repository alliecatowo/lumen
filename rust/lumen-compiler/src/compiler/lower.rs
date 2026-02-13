//! AST → LIR lowering. Converts typed AST to LIR instructions.

use crate::compiler::ast::*;
use crate::compiler::lir::*;
use crate::compiler::regalloc::RegAlloc;
use crate::compiler::resolve::SymbolTable;
use crate::compiler::tokens::Span;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Lower an entire program to a LIR module.
pub fn lower(program: &Program, symbols: &SymbolTable, source: &str) -> LirModule {
    let doc_hash = format!("sha256:{:x}", Sha256::digest(source.as_bytes()));
    let mut module = LirModule::new(doc_hash);
    let mut lowerer = Lowerer::new(symbols);

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
                                span,
                            },
                            Param {
                                name: "input".to_string(),
                                ty: TypeExpr::Named("Any".to_string(), span),
                                default_value: None,
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
                module.handlers.push(lowerer.lower_handler(h, &mut module.cells));
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
    module.cells.extend(lowerer.lambda_cells.drain(..));

    // Collect string table
    module.strings = lowerer.strings;
    module
}

/// Tracks a loop for break/continue patching
struct LoopContext {
    start: usize,
    break_jumps: Vec<usize>,
}

struct Lowerer<'a> {
    symbols: &'a SymbolTable,
    tool_indices: HashMap<String, u16>,
    strings: Vec<String>,
    loop_stack: Vec<LoopContext>,
    lambda_cells: Vec<LirCell>,
}

impl<'a> Lowerer<'a> {
    fn new(symbols: &'a SymbolTable) -> Self {
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
            strings: Vec::new(),
            loop_stack: Vec::new(),
            lambda_cells: Vec::new(),
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
            self.emit_set_field(dest, method, val_reg, &mut ra, &mut constants, &mut instructions);
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
                }
            })
            .collect();

        // Lower body
        for stmt in &cell.body {
            self.lower_stmt(stmt, &mut ra, &mut constants, &mut instructions);
        }

        // Ensure return at end
        if instructions.is_empty()
            || !matches!(
                instructions.last().map(|i| i.op),
                Some(OpCode::Return) | Some(OpCode::Halt)
            )
        {
            let r = ra.alloc_temp();
            instructions.push(Instruction::abc(OpCode::LoadNil, r, 0, 0));
            instructions.push(Instruction::abc(OpCode::Return, r, 1, 0));
        }

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
                let dest = ra.alloc_named(&ls.name);
                if dest != val_reg {
                    instrs.push(Instruction::abc(OpCode::Move, dest, val_reg, 0));
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

                for s in &fs.body {
                    self.lower_stmt(s, ra, consts, instrs);
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
            Stmt::Break(_) => {
                let jmp_idx = instrs.len();
                instrs.push(Instruction::sax(OpCode::Jmp, 0)); // placeholder
                if let Some(ctx) = self.loop_stack.last_mut() {
                    ctx.break_jumps.push(jmp_idx);
                }
            }
            Stmt::Continue(_) => {
                if let Some(ctx) = self.loop_stack.last() {
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
                };
                instrs.push(Instruction::abc(opcode, target_reg, target_reg, val_reg));
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

    fn emit_jump_if_false(
        &mut self,
        cond_reg: u8,
        instrs: &mut Vec<Instruction>,
    ) -> usize {
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
        instrs.push(Instruction::abc(OpCode::SetIndex, obj_reg, key_reg, value_reg));
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
                if let Some(ref b) = binding {
                    let breg = ra.alloc_named(b);
                    instrs.push(Instruction::abc(OpCode::Unbox, breg, value_reg, 0));
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
                    self.lower_match_pattern(
                        p,
                        value_reg,
                        ra,
                        consts,
                        instrs,
                        &mut alt_fail_jumps,
                    );
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
                let expected_reg =
                    self.push_const_int(elements.len() as i64, ra, consts, instrs);
                let arity_ok = ra.alloc_temp();
                if rest.is_some() {
                    // expected_len <= actual_len
                    instrs.push(Instruction::abc(OpCode::Le, arity_ok, expected_reg, len_reg));
                } else {
                    instrs.push(Instruction::abc(OpCode::Eq, arity_ok, len_reg, expected_reg));
                }
                fail_jumps.push(self.emit_jump_if_false(arity_ok, instrs));

                for (idx, elem_pat) in elements.iter().enumerate() {
                    let idx_reg = self.push_const_int(idx as i64, ra, consts, instrs);
                    let elem_reg = ra.alloc_temp();
                    instrs.push(Instruction::abc(OpCode::GetIndex, elem_reg, value_reg, idx_reg));
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
                instrs.push(Instruction::abc(OpCode::Is, is_tuple, value_reg, tuple_type));
                fail_jumps.push(self.emit_jump_if_false(is_tuple, instrs));

                let len_reg = ra.alloc_temp();
                instrs.push(Instruction::abc(
                    OpCode::Intrinsic,
                    len_reg,
                    IntrinsicId::Length as u8,
                    value_reg,
                ));
                let expected_reg =
                    self.push_const_int(elements.len() as i64, ra, consts, instrs);
                let arity_ok = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::Eq, arity_ok, len_reg, expected_reg));
                fail_jumps.push(self.emit_jump_if_false(arity_ok, instrs));

                for (idx, elem_pat) in elements.iter().enumerate() {
                    let idx_reg = self.push_const_int(idx as i64, ra, consts, instrs);
                    let elem_reg = ra.alloc_temp();
                    instrs.push(Instruction::abc(OpCode::GetIndex, elem_reg, value_reg, idx_reg));
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
                        self.lower_match_pattern(field_pat, field_reg, ra, consts, instrs, fail_jumps);
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
                // Evaluate each element and move into consecutive registers after dest
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
            Expr::BinOp(lhs, op, rhs, _) => {
                // Special case: pipe forward desugars to function call
                if *op == BinOp::PipeForward {
                    let arg_reg = self.lower_expr(lhs, ra, consts, instrs);
                    let fn_reg = self.lower_expr(rhs, ra, consts, instrs);
                    let base = ra.alloc_temp();
                    if fn_reg != base {
                        instrs.push(Instruction::abc(OpCode::Move, base, fn_reg, 0));
                    }
                    let arg_dest = ra.alloc_temp();
                    if arg_reg != arg_dest {
                        instrs.push(Instruction::abc(OpCode::Move, arg_dest, arg_reg, 0));
                    }
                    let result = ra.alloc_temp();
                    instrs.push(Instruction::abc(OpCode::Call, base, 1, 1));
                    instrs.push(Instruction::abc(OpCode::Move, result, base, 0));
                    return result;
                }

                let lr = self.lower_expr(lhs, ra, consts, instrs);
                let rr = self.lower_expr(rhs, ra, consts, instrs);
                let dest = ra.alloc_temp();
                let opcode = match op {
                    BinOp::Add => OpCode::Add,
                    BinOp::Sub => OpCode::Sub,
                    BinOp::Mul => OpCode::Mul,
                    BinOp::Div => OpCode::Div,
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
                    BinOp::PipeForward => unreachable!(), // handled above
                };
                match op {
                    BinOp::Gt => instrs.push(Instruction::abc(opcode, dest, rr, lr)),
                    BinOp::GtEq => instrs.push(Instruction::abc(opcode, dest, rr, lr)),
                    _ => instrs.push(Instruction::abc(opcode, dest, lr, rr)),
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
                // Check for intrinsic call or Enum/Result constructor
                if let Expr::Ident(ref name, _) = **callee {
                    if self.tool_indices.contains_key(name) && !self.symbols.cells.contains_key(name)
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
                                        // If cond is false (Invalid), equality is true -> Skip Next (Jmp) -> Execute Halt
                                        // If cond is true (Valid), equality is false -> Exec Next (Jmp) -> Jump Over Halt
                                        instrs.push(Instruction::abc(
                                            OpCode::Eq,
                                            0,
                                            cond_reg,
                                            false_reg,
                                        ));

                                        // If true, skip Halt
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

                    let intrinsic = match name.as_str() {
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
                        "confirm" => Some(IntrinsicId::Matches),
                        "abs" => Some(IntrinsicId::Abs),
                        _ => None,
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

                // Move callee + args to contiguous block
                // base = callee
                // base+1.. = args
                let base = ra.alloc_temp();
                if callee_reg != base {
                    instrs.push(Instruction::abc(OpCode::Move, base, callee_reg, 0));
                }

                for (i, &reg) in arg_regs.iter().enumerate() {
                    let target = base + 1 + i as u8;
                    ra.alloc_temp(); // ensure allocated
                    if reg != target {
                        instrs.push(Instruction::abc(OpCode::Move, target, reg, 0));
                    }
                }

                let result_reg = ra.alloc_temp();
                // Call A B C. A=base. B=nargs+1. C=nresults+1 (1 result -> 2).
                // Or checking usage: Call(callee_reg, nargs, 1) in previous code suggests B=nargs?
                // Lua: B=0 -> all; B=1 -> 0 args? No.
                // Lua: A(A+1, ..., A+B-1).
                // Lumen LIR might differ.
                // Previous code: `OpCode::Call, callee_reg, nargs as u8, 1`
                // I will assume B = nargs (count).
                // And args start at A+1.
                // Or maybe A is just the function, args are inferred? No.
                // Let's assume Lua style 5.1: A is function register. Args follow. B = nargs+1.
                // Given previous code passed `nargs`, maybe it was `nargs`?
                // I'll stick to `nargs` for now but with contiguous registers.
                // Wait, if I pass logic `nargs` and regs are contiguous, VM can find them.

                // If I use `nargs` as B:
                instrs.push(Instruction::abc(
                    OpCode::Call,
                    base,
                    arg_regs.len() as u8,
                    1,
                ));

                // Result is in base? Or C?
                // Lua: results start at A.
                // Previous code: `Move result_reg, callee_reg`.
                // So results overwrite function register.
                instrs.push(Instruction::abc(OpCode::Move, result_reg, base, 0));

                result_reg
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
                // Lower lambda body as a separate LirCell
                let lambda_name = format!("<lambda/{}>", self.lambda_cells.len());
                let mut lra = RegAlloc::new();
                let mut lconsts: Vec<Constant> = Vec::new();
                let mut linstrs: Vec<Instruction> = Vec::new();

                let lparams: Vec<LirParam> = params
                    .iter()
                    .map(|p| {
                        let reg = lra.alloc_named(&p.name);
                        LirParam {
                            name: p.name.clone(),
                            ty: format_type_expr(&p.ty),
                            register: reg,
                        }
                    })
                    .collect();

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
                            let r = lra.alloc_temp();
                            linstrs.push(Instruction::abc(OpCode::LoadNil, r, 0, 0));
                            linstrs.push(Instruction::abc(OpCode::Return, r, 1, 0));
                        }
                    }
                }

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
                // Test if obj is null
                let nil_reg = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::LoadNil, nil_reg, 0, 0));
                let cmp = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::Eq, cmp, or, nil_reg));
                instrs.push(Instruction::abc(OpCode::Test, cmp, 0, 0));
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
            Expr::SpreadExpr(inner, _) => self.lower_expr(inner, ra, consts, instrs),
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
                kind: _,
                span: _,
            } => {
                // Lower as: result = []; for var in iter { if cond { append(result, body) } }
                let dest = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::NewList, dest, 0, 0));

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
                instrs.push(Instruction::abc(OpCode::Append, dest, body_reg, 0));

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
                CallArg::Named(name, e, _) => (name.clone(), self.lower_expr(e, ra, consts, instrs)),
                CallArg::Positional(e) => {
                    (format!("arg{}", idx), self.lower_expr(e, ra, consts, instrs))
                }
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

        instrs.push(Instruction::abc(OpCode::NewMap, dest, kv_regs.len() as u8, 0));
        let tool_index = alias
            .and_then(|name| self.tool_indices.get(name).copied())
            .unwrap_or(0);
        instrs.push(Instruction::abx(OpCode::ToolCall, dest, tool_index));
        dest
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
}
