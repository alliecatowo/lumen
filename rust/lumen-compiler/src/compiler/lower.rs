//! AST → LIR lowering. Converts typed AST to LIR instructions.

use crate::compiler::ast::*;
use crate::compiler::lir::*;
use crate::compiler::regalloc::RegAlloc;
use crate::compiler::resolve::SymbolTable;
use sha2::{Digest, Sha256};

/// Lower an entire program to a LIR module.
pub fn lower(program: &Program, symbols: &SymbolTable, source: &str) -> LirModule {
    let doc_hash = format!("sha256:{:x}", Sha256::digest(source.as_bytes()));
    let mut module = LirModule::new(doc_hash);
    let mut lowerer = Lowerer::new(symbols);

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
            Item::UseTool(u) => module.tools.push(LirTool {
                alias: u.alias.clone(),
                tool_id: u.tool_path.clone(),
                version: "1.0.0".to_string(),
                mcp_url: u.mcp_url.clone(),
            }),
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

    // Collect lambda cells
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
    strings: Vec<String>,
    loop_stack: Vec<LoopContext>,
    lambda_cells: Vec<LirCell>,
}

impl<'a> Lowerer<'a> {
    fn new(symbols: &'a SymbolTable) -> Self {
        Self {
            symbols,
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

    fn lower_agent_constructor(&mut self, a: &AgentDecl) -> LirCell {
        self.intern_string(&a.name);
        let mut ra = RegAlloc::new();
        let mut constants: Vec<Constant> = Vec::new();
        let mut instructions: Vec<Instruction> = Vec::new();

        let dest = ra.alloc_temp();
        let type_idx = self.intern_string(&a.name);
        instructions.push(Instruction::abx(OpCode::NewRecord, dest, type_idx));

        for cell in &a.cells {
            let field_idx = self.intern_string(&cell.name) as u8;
            let val_reg = ra.alloc_temp();
            let kidx = constants.len() as u16;
            constants.push(Constant::String(format!("{}.{}", a.name, cell.name)));
            instructions.push(Instruction::abx(OpCode::LoadK, val_reg, kidx));
            instructions.push(Instruction::abc(OpCode::SetField, dest, field_idx, val_reg));
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
                    match &arm.pattern {
                        Pattern::Literal(lit_expr) => {
                            let lit_reg = self.lower_expr(lit_expr, ra, consts, instrs);
                            let cmp_reg = ra.alloc_temp();
                            instrs.push(Instruction::abc(OpCode::Eq, cmp_reg, subj_reg, lit_reg));
                            // Test: skip next instruction (Jmp) if cmp_reg is truthy
                            instrs.push(Instruction::abc(OpCode::Test, cmp_reg, 0, 0));
                            let skip_jmp = instrs.len();
                            instrs.push(Instruction::sax(OpCode::Jmp, 0));

                            for s in &arm.body {
                                self.lower_stmt(s, ra, consts, instrs);
                            }
                            end_jumps.push(instrs.len());
                            instrs.push(Instruction::sax(OpCode::Jmp, 0));

                            let after = instrs.len();
                            instrs[skip_jmp] =
                                Instruction::sax(OpCode::Jmp, (after - skip_jmp - 1) as i32);
                        }
                        Pattern::Variant(tag, binding, _) => {
                            // Check Variant Tag
                            let tag_idx = self.intern_string(tag);
                            instrs.push(Instruction::abx(OpCode::IsVariant, subj_reg, tag_idx));
                            // If NOT matched, skip next instruction (the body execution)
                            // Wait, IsVariant logic in VM: "if matched, skip next".
                            // So if Matched, we Skip the JUMP.
                            // So next instruction should be JUMP (to next arm).
                            let skip_jmp = instrs.len();
                            instrs.push(Instruction::sax(OpCode::Jmp, 0)); // Jump to next arm (if NOT matched)

                            // Matched: Execute body
                            if let Some(ref b) = binding {
                                let breg = ra.alloc_named(b);
                                instrs.push(Instruction::abc(OpCode::Unbox, breg, subj_reg, 0));
                            }
                            for s in &arm.body {
                                self.lower_stmt(s, ra, consts, instrs);
                            }
                            end_jumps.push(instrs.len());
                            instrs.push(Instruction::sax(OpCode::Jmp, 0)); // Jump to end of match

                            // Patch skip_jmp (failure case) to point here
                            let after = instrs.len();
                            instrs[skip_jmp] =
                                Instruction::sax(OpCode::Jmp, (after - skip_jmp - 1) as i32);
                        }
                        Pattern::Wildcard(_) | Pattern::Ident(_, _) => {
                            if let Pattern::Ident(name, _) = &arm.pattern {
                                let breg = ra.alloc_named(name);
                                instrs.push(Instruction::abc(OpCode::Move, breg, subj_reg, 0));
                            }
                            for s in &arm.body {
                                self.lower_stmt(s, ra, consts, instrs);
                            }
                            end_jumps.push(instrs.len());
                            instrs.push(Instruction::sax(OpCode::Jmp, 0));
                        }
                        // New pattern types: treat as wildcard for now
                        Pattern::Guard { .. }
                        | Pattern::Or { .. }
                        | Pattern::ListDestructure { .. }
                        | Pattern::TupleDestructure { .. }
                        | Pattern::RecordDestructure { .. }
                        | Pattern::TypeCheck { .. } => {
                            for s in &arm.body {
                                self.lower_stmt(s, ra, consts, instrs);
                            }
                            end_jumps.push(instrs.len());
                            instrs.push(Instruction::sax(OpCode::Jmp, 0));
                        }
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
                    let field_idx = self.intern_string(field_name) as u8;
                    instrs.push(Instruction::abc(OpCode::SetField, dest, field_idx, val_reg));
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
                    // Check Result/Enum constructors
                    // "ok" / "err"
                    let is_result = name == "ok" || name == "err";
                    let is_enum = self.symbols.types.values().any(|t| matches!(&t.kind, crate::compiler::resolve::TypeInfoKind::Enum(e) if e.variants.iter().any(|v| v.name == *name)));
                    let is_record = self.symbols.types.values().any(|t| matches!(&t.kind, crate::compiler::resolve::TypeInfoKind::Record(r) if r.name == *name));

                    if is_record {
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

                                    let field_idx = self.intern_string(field) as u8;
                                    instrs.push(Instruction::abc(
                                        OpCode::SetField,
                                        dest,
                                        field_idx,
                                        val_reg,
                                    ));
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
                        if self.symbols.agents.contains_key(agent_name) {
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
                            let fidx = self.intern_string(field);
                            instrs.push(Instruction::abc(
                                OpCode::GetField,
                                dest,
                                obj_reg,
                                fidx as u8,
                            ));
                            dest
                        }
                    } else {
                        let obj_reg = self.lower_expr(obj, ra, consts, instrs);
                        implicit_self_arg = Some(obj_reg);
                        let dest = ra.alloc_temp();
                        let fidx = self.intern_string(field);
                        instrs.push(Instruction::abc(
                            OpCode::GetField,
                            dest,
                            obj_reg,
                            fidx as u8,
                        ));
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
                let _ = self.lower_expr(callee, ra, consts, instrs); // Callee is ignored? "tool" token?
                                                                     // Should we lower callee if it's an expression? "tool" is keyword.
                                                                     // If callee is Expr::Ident("tool"), it evaluates to nothing?
                                                                     // But ToolCall syntax is `tool name(args)`. name is Ident.
                                                                     // The AST has `callee: Expr`.

                let mut arg_regs = Vec::new();
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

                // Pack args into contiguous registers starting at dest+1
                let dest = ra.alloc_temp(); // A
                                            // Allocate space for args
                for _ in 0..arg_regs.len() {
                    ra.alloc_temp();
                }

                for (i, &reg) in arg_regs.iter().enumerate() {
                    let target = dest + 1 + i as u8;
                    if reg != target {
                        instrs.push(Instruction::abc(OpCode::Move, target, reg, 0));
                    }
                }

                instrs.push(Instruction::abc(
                    OpCode::ToolCall,
                    dest,
                    0,
                    args.len() as u8,
                ));
                dest
            }
            Expr::DotAccess(obj, field, _) => {
                let or = self.lower_expr(obj, ra, consts, instrs);
                let dest = ra.alloc_temp();
                let fidx = self.intern_string(field);
                instrs.push(Instruction::abc(OpCode::GetField, dest, or, fidx as u8));
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
                let fidx = self.intern_string(field);
                instrs.push(Instruction::abc(OpCode::GetField, dest, or, fidx as u8));
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
                instrs.push(Instruction::abc(
                    OpCode::Intrinsic,
                    dest,
                    IntrinsicId::Append as u8,
                    body_reg,
                ));

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
