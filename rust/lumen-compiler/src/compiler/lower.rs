//! AST → LIR lowering. Converts typed AST to LIR instructions.

use crate::compiler::ast::*;
use crate::compiler::lir::*;
use crate::compiler::regalloc::RegAlloc;
use crate::compiler::resolve::SymbolTable;
use sha2::{Sha256, Digest};

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
        }
    }

    // Collect string table
    module.strings = lowerer.strings;
    module
}

struct Lowerer<'a> {
    symbols: &'a SymbolTable,
    strings: Vec<String>,
}

impl<'a> Lowerer<'a> {
    fn new(symbols: &'a SymbolTable) -> Self {
        Self { symbols, strings: Vec::new() }
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
            fields: r.fields.iter().map(|f| {
                self.intern_string(&f.name);
                LirField {
                    name: f.name.clone(),
                    ty: format_type_expr(&f.ty),
                    constraints: if f.constraint.is_some() { vec!["has_constraint".into()] } else { vec![] },
                }
            }).collect(),
            variants: vec![],
        }
    }

    fn lower_enum(&mut self, e: &EnumDef) -> LirType {
        self.intern_string(&e.name);
        LirType {
            kind: "enum".to_string(),
            name: e.name.clone(),
            fields: vec![],
            variants: e.variants.iter().map(|v| {
                self.intern_string(&v.name);
                LirVariant { name: v.name.clone(), payload: v.payload.as_ref().map(format_type_expr) }
            }).collect(),
        }
    }

    fn lower_cell(&mut self, cell: &CellDef) -> LirCell {
        self.intern_string(&cell.name);
        let mut ra = RegAlloc::new();
        let mut constants: Vec<Constant> = Vec::new();
        let mut instructions: Vec<Instruction> = Vec::new();

        // Allocate param registers
        let params: Vec<LirParam> = cell.params.iter().map(|p| {
            let reg = ra.alloc_named(&p.name);
            LirParam { name: p.name.clone(), ty: format_type_expr(&p.ty), register: reg }
        }).collect();

        // Lower body
        for stmt in &cell.body {
            self.lower_stmt(stmt, &mut ra, &mut constants, &mut instructions);
        }

        // Ensure return at end
        if instructions.is_empty() || !matches!(instructions.last().map(|i| i.op), Some(OpCode::Return) | Some(OpCode::Halt)) {
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

    fn lower_stmt(&mut self, stmt: &Stmt, ra: &mut RegAlloc, consts: &mut Vec<Constant>, instrs: &mut Vec<Instruction>) {
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
                // Jump over then-block if false
                let jmp_idx = instrs.len();
                instrs.push(Instruction::abc(OpCode::Eq, 0, cond_reg, 0)); // placeholder
                instrs.push(Instruction::ax(OpCode::Jmp, 0)); // placeholder

                for s in &ifs.then_body { self.lower_stmt(s, ra, consts, instrs); }

                if let Some(ref else_body) = ifs.else_body {
                    let else_jmp_idx = instrs.len();
                    instrs.push(Instruction::ax(OpCode::Jmp, 0)); // skip else
                    let else_start = instrs.len();
                    // Patch the conditional jump
                    let offset = (else_start - jmp_idx - 2) as u32;
                    instrs[jmp_idx + 1] = Instruction::ax(OpCode::Jmp, offset);

                    for s in else_body { self.lower_stmt(s, ra, consts, instrs); }

                    let after_else = instrs.len();
                    let else_offset = (after_else - else_jmp_idx - 1) as u32;
                    instrs[else_jmp_idx] = Instruction::ax(OpCode::Jmp, else_offset);
                } else {
                    let after = instrs.len();
                    let offset = (after - jmp_idx - 2) as u32;
                    instrs[jmp_idx + 1] = Instruction::ax(OpCode::Jmp, offset);
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
                instrs.push(Instruction::abc(OpCode::Intrinsic, len_reg, IntrinsicId::Length as u8, iter_reg));

                let loop_start = instrs.len();
                // if idx >= len, break
                instrs.push(Instruction::abc(OpCode::Lt, 1, idx_reg, len_reg));
                let break_jmp = instrs.len();
                instrs.push(Instruction::ax(OpCode::Jmp, 0)); // placeholder

                // elem = iter[idx]
                instrs.push(Instruction::abc(OpCode::GetIndex, elem_reg, iter_reg, idx_reg));

                for s in &fs.body { self.lower_stmt(s, ra, consts, instrs); }

                // idx = idx + 1
                let one_idx = consts.len() as u16;
                consts.push(Constant::Int(1));
                let one_reg = ra.alloc_temp();
                instrs.push(Instruction::abx(OpCode::LoadK, one_reg, one_idx));
                instrs.push(Instruction::abc(OpCode::Add, idx_reg, idx_reg, one_reg));

                // Jump back to loop start
                let back_offset = (loop_start as i32 - instrs.len() as i32 - 1) as u32;
                instrs.push(Instruction::ax(OpCode::Jmp, back_offset));

                // Patch break jump
                let after_loop = instrs.len();
                let break_offset = (after_loop - break_jmp - 1) as u32;
                instrs[break_jmp] = Instruction::ax(OpCode::Jmp, break_offset);
            }
            Stmt::Match(ms) => {
                let subj_reg = self.lower_expr(&ms.subject, ra, consts, instrs);
                let mut end_jumps = Vec::new();

                for arm in &ms.arms {
                    match &arm.pattern {
                        Pattern::Literal(lit_expr) => {
                            let lit_reg = self.lower_expr(lit_expr, ra, consts, instrs);
                            instrs.push(Instruction::abc(OpCode::Eq, 0, subj_reg, lit_reg));
                            let skip_jmp = instrs.len();
                            instrs.push(Instruction::ax(OpCode::Jmp, 0));

                            for s in &arm.body { self.lower_stmt(s, ra, consts, instrs); }
                            end_jumps.push(instrs.len());
                            instrs.push(Instruction::ax(OpCode::Jmp, 0));

                            let after = instrs.len();
                            instrs[skip_jmp] = Instruction::ax(OpCode::Jmp, (after - skip_jmp - 1) as u32);
                        }
                        Pattern::Variant(_, binding, _) => {
                            if let Some(ref b) = binding {
                                let breg = ra.alloc_named(b);
                                instrs.push(Instruction::abc(OpCode::Move, breg, subj_reg, 0));
                            }
                            for s in &arm.body { self.lower_stmt(s, ra, consts, instrs); }
                            end_jumps.push(instrs.len());
                            instrs.push(Instruction::ax(OpCode::Jmp, 0));
                        }
                        Pattern::Wildcard(_) | Pattern::Ident(_, _) => {
                            if let Pattern::Ident(name, _) = &arm.pattern {
                                let breg = ra.alloc_named(name);
                                instrs.push(Instruction::abc(OpCode::Move, breg, subj_reg, 0));
                            }
                            for s in &arm.body { self.lower_stmt(s, ra, consts, instrs); }
                            end_jumps.push(instrs.len());
                            instrs.push(Instruction::ax(OpCode::Jmp, 0));
                        }
                    }
                }

                let end = instrs.len();
                for jmp_idx in end_jumps {
                    instrs[jmp_idx] = Instruction::ax(OpCode::Jmp, (end - jmp_idx - 1) as u32);
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
            Stmt::Expr(es) => {
                self.lower_expr(&es.expr, ra, consts, instrs);
            }
        }
    }

    fn lower_expr(&mut self, expr: &Expr, ra: &mut RegAlloc, consts: &mut Vec<Constant>, instrs: &mut Vec<Instruction>) -> u8 {
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
            Expr::StringInterp(_, _) => {
                let dest = ra.alloc_temp();
                let kidx = consts.len() as u16;
                consts.push(Constant::String("<interp>".to_string()));
                instrs.push(Instruction::abx(OpCode::LoadK, dest, kidx));
                dest
            }
            Expr::BoolLit(b, _) => {
                let dest = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::LoadBool, dest, if *b { 1 } else { 0 }, 0));
                dest
            }
            Expr::NullLit(_) => {
                let dest = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::LoadNil, dest, 0, 0));
                dest
            }
            Expr::Ident(name, _) => {
                ra.lookup(name).unwrap_or_else(|| {
                    let dest = ra.alloc_temp();
                    instrs.push(Instruction::abc(OpCode::LoadNil, dest, 0, 0));
                    dest
                })
            }
            Expr::ListLit(elems, _) => {
                let dest = ra.alloc_temp();
                let base = ra.alloc_temp();
                for (i, elem) in elems.iter().enumerate() {
                    let er = self.lower_expr(elem, ra, consts, instrs);
                    if er != base + i as u8 + 1 {
                        // May need to move
                    }
                }
                instrs.push(Instruction::abc(OpCode::NewList, dest, elems.len() as u8, 0));
                dest
            }
            Expr::MapLit(pairs, _) => {
                let dest = ra.alloc_temp();
                for (k, v) in pairs {
                    self.lower_expr(k, ra, consts, instrs);
                    self.lower_expr(v, ra, consts, instrs);
                }
                instrs.push(Instruction::abc(OpCode::NewMap, dest, pairs.len() as u8, 0));
                dest
            }
            Expr::RecordLit(name, fields, _) => {
                let dest = ra.alloc_temp();
                let type_idx = self.intern_string(name);
                for (_, val) in fields {
                    self.lower_expr(val, ra, consts, instrs);
                }
                instrs.push(Instruction::abx(OpCode::NewRecord, dest, type_idx));
                dest
            }
            Expr::BinOp(lhs, op, rhs, _) => {
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
                    BinOp::Gt => OpCode::Lt, // swap operands
                    BinOp::GtEq => OpCode::Le, // swap operands
                    BinOp::And => OpCode::And,
                    BinOp::Or => OpCode::Or,
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
                }
                dest
            }
            Expr::Call(callee, args, _) => {
                let callee_reg = self.lower_expr(callee, ra, consts, instrs);
                let nargs = args.iter().filter(|a| !matches!(a, CallArg::Role(_, _, _))).count();
                for arg in args {
                    match arg {
                        CallArg::Positional(e) | CallArg::Named(_, e, _) => { self.lower_expr(e, ra, consts, instrs); }
                        CallArg::Role(name, content, _) => {
                            let dest = ra.alloc_temp();
                            let kidx = consts.len() as u16;
                            consts.push(Constant::String(format!("{}:{}", name, content)));
                            instrs.push(Instruction::abx(OpCode::LoadK, dest, kidx));
                        }
                    }
                }
                let result_reg = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::Call, callee_reg, nargs as u8, 1));
                instrs.push(Instruction::abc(OpCode::Move, result_reg, callee_reg, 0));
                result_reg
            }
            Expr::ToolCall(callee, args, _) => {
                self.lower_expr(callee, ra, consts, instrs);
                for arg in args {
                    match arg { CallArg::Positional(e) | CallArg::Named(_, e, _) => { self.lower_expr(e, ra, consts, instrs); } _ => {} }
                }
                let dest = ra.alloc_temp();
                instrs.push(Instruction::abc(OpCode::ToolCall, dest, 0, args.len() as u8));
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
                let dest = ra.alloc_temp();
                let kidx = consts.len() as u16;
                consts.push(Constant::String(format!("{}:{}", name, content)));
                instrs.push(Instruction::abx(OpCode::LoadK, dest, kidx));
                dest
            }
            Expr::ExpectSchema(inner, schema_name, _) => {
                let ir = self.lower_expr(inner, ra, consts, instrs);
                let schema_idx = self.intern_string(schema_name);
                instrs.push(Instruction::abx(OpCode::Schema, ir, schema_idx));
                ir
            }
        }
    }
}

fn format_type_expr(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Named(n, _) => n.clone(),
        TypeExpr::List(inner, _) => format!("list[{}]", format_type_expr(inner)),
        TypeExpr::Map(k, v, _) => format!("map[{}, {}]", format_type_expr(k), format_type_expr(v)),
        TypeExpr::Result(ok, err, _) => format!("result[{}, {}]", format_type_expr(ok), format_type_expr(err)),
        TypeExpr::Union(types, _) => types.iter().map(format_type_expr).collect::<Vec<_>>().join(" | "),
        TypeExpr::Null(_) => "Null".to_string(),
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
