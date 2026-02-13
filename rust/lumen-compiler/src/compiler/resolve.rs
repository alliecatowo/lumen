//! Name resolution pass — resolve cells, types, and tool aliases.

use crate::compiler::ast::*;
use std::collections::{BTreeSet, HashMap};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("undefined type '{name}' at line {line}")]
    UndefinedType { name: String, line: usize },
    #[error("undefined cell '{name}' at line {line}")]
    UndefinedCell { name: String, line: usize },
    #[error("undefined tool alias '{name}' at line {line}")]
    UndefinedTool { name: String, line: usize },
    #[error("duplicate definition '{name}' at line {line}")]
    Duplicate { name: String, line: usize },
    #[error("cell '{cell}' requires effect '{effect}' but no compatible grant is in scope (line {line})")]
    MissingEffectGrant {
        cell: String,
        effect: String,
        line: usize,
    },
    #[error("cell '{cell}' performs effect '{effect}' but it is not declared in its effect row (line {line})")]
    UndeclaredEffect {
        cell: String,
        effect: String,
        line: usize,
    },
}

/// Symbol table built during resolution
#[derive(Debug, Clone)]
pub struct SymbolTable {
    pub types: HashMap<String, TypeInfo>,
    pub cells: HashMap<String, CellInfo>,
    pub tools: HashMap<String, ToolInfo>,
    pub agents: HashMap<String, AgentInfo>,
    pub processes: HashMap<String, ProcessInfo>,
    pub effects: HashMap<String, EffectInfo>,
    pub effect_binds: Vec<EffectBindInfo>,
    pub handlers: HashMap<String, HandlerInfo>,
    pub addons: Vec<AddonInfo>,
    pub type_aliases: HashMap<String, TypeExpr>,
    pub traits: HashMap<String, TraitInfo>,
    pub impls: Vec<ImplInfo>,
    pub consts: HashMap<String, ConstInfo>,
}

#[derive(Debug, Clone)]
pub struct TypeInfo {
    pub kind: TypeInfoKind,
}

#[derive(Debug, Clone)]
pub enum TypeInfoKind {
    Builtin,
    Record(RecordDef),
    Enum(EnumDef),
}

#[derive(Debug, Clone)]
pub struct CellInfo {
    pub params: Vec<(String, TypeExpr)>,
    pub return_type: Option<TypeExpr>,
    pub effects: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub tool_path: String,
    pub mcp_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub name: String,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub kind: String,
    pub name: String,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EffectInfo {
    pub name: String,
    pub operations: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EffectBindInfo {
    pub effect_path: String,
    pub tool_alias: String,
}

#[derive(Debug, Clone)]
pub struct HandlerInfo {
    pub name: String,
    pub handles: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AddonInfo {
    pub kind: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TraitInfo {
    pub name: String,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ImplInfo {
    pub trait_name: Option<String>,
    pub target_type: String,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ConstInfo {
    pub name: String,
    pub ty: Option<TypeExpr>,
    pub value: Option<Expr>,
}

impl SymbolTable {
    pub fn new() -> Self {
        let mut types = HashMap::new();
        // Register builtin types
        for name in &[
            "String",
            "Int",
            "Float",
            "Bool",
            "Bytes",
            "Json",
            "type",
            "ValidationError",
            "Embedding",
            "Record",
            "Item",
            "Paper",
            "Message",
            "LumenError",
            "GuardrailViolation",
            "Response",
            "Result",
            "Invoice",
            "ExtractionError",
            "AnalysisResult",
            "Report",
            "Resolution",
            "TestCase",
            "EvalResult",
            "JudgmentScore",
            "AppError",
            "TypeError",
            "MyRecord",
            "LineItem",
            "Context",
            "Data",
            "Pair",
            "Event",
            "A",
            "B",
            "C",
            "T",
            "U",
            "V",
            "Self",
        ] {
            types.insert(
                name.to_string(),
                TypeInfo {
                    kind: TypeInfoKind::Builtin,
                },
            );
        }
        Self {
            types,
            cells: HashMap::new(),
            tools: HashMap::new(),
            agents: HashMap::new(),
            processes: HashMap::new(),
            effects: HashMap::new(),
            effect_binds: Vec::new(),
            handlers: HashMap::new(),
            addons: Vec::new(),
            type_aliases: HashMap::new(),
            traits: HashMap::new(),
            impls: Vec::new(),
            consts: HashMap::new(),
        }
    }
}

/// Resolve all names in a program, building the symbol table.
pub fn resolve(program: &Program) -> Result<SymbolTable, Vec<ResolveError>> {
    let mut table = SymbolTable::new();
    let mut errors = Vec::new();

    // First pass: register all type and cell definitions
    for item in &program.items {
        match item {
            Item::Record(r) => {
                table.types.insert(
                    r.name.clone(),
                    TypeInfo {
                        kind: TypeInfoKind::Record(r.clone()),
                    },
                );
            }
            Item::Enum(e) => {
                table.types.insert(
                    e.name.clone(),
                    TypeInfo {
                        kind: TypeInfoKind::Enum(e.clone()),
                    },
                );
            }
            Item::Cell(c) => {
                table.cells.insert(
                    c.name.clone(),
                    CellInfo {
                        params: c
                            .params
                            .iter()
                            .map(|p| (p.name.clone(), p.ty.clone()))
                            .collect(),
                        return_type: c.return_type.clone(),
                        effects: c.effects.clone(),
                    },
                );
            }
            Item::Agent(a) => {
                if table.agents.contains_key(&a.name) {
                    errors.push(ResolveError::Duplicate {
                        name: a.name.clone(),
                        line: a.span.line,
                    });
                } else {
                    table.agents.insert(
                        a.name.clone(),
                        AgentInfo {
                            name: a.name.clone(),
                            methods: a.cells.iter().map(|c| c.name.clone()).collect(),
                        },
                    );
                }

                if !table.types.contains_key(&a.name) {
                    table.types.insert(
                        a.name.clone(),
                        TypeInfo {
                            kind: TypeInfoKind::Record(RecordDef {
                                name: a.name.clone(),
                                generic_params: vec![],
                                fields: vec![],
                                is_pub: true,
                                span: a.span,
                            }),
                        },
                    );
                }

                if !table.cells.contains_key(&a.name) {
                    table.cells.insert(
                        a.name.clone(),
                        CellInfo {
                            params: vec![],
                            return_type: Some(TypeExpr::Named(a.name.clone(), a.span)),
                            effects: vec![],
                        },
                    );
                }

                for cell in &a.cells {
                    let method_name = format!("{}.{}", a.name, cell.name);
                    if table.cells.contains_key(&method_name) {
                        errors.push(ResolveError::Duplicate {
                            name: method_name.clone(),
                            line: cell.span.line,
                        });
                    } else {
                        table.cells.insert(
                            method_name,
                            CellInfo {
                                params: cell
                                    .params
                                    .iter()
                                    .map(|p| (p.name.clone(), p.ty.clone()))
                                    .collect(),
                                return_type: cell.return_type.clone(),
                                effects: cell.effects.clone(),
                            },
                        );
                    }
                }

                for g in &a.grants {
                    table.tools.entry(g.tool_alias.clone()).or_insert(ToolInfo {
                        tool_path: g.tool_alias.to_lowercase(),
                        mcp_url: None,
                    });
                }
            }
            Item::Process(p) => {
                let process_key = format!("{}:{}", p.kind, p.name);
                table.processes.insert(
                    process_key,
                    ProcessInfo {
                        kind: p.kind.clone(),
                        name: p.name.clone(),
                        methods: p.cells.iter().map(|c| c.name.clone()).collect(),
                    },
                );
                if !table.types.contains_key(&p.name) {
                    table.types.insert(
                        p.name.clone(),
                        TypeInfo {
                            kind: TypeInfoKind::Record(RecordDef {
                                name: p.name.clone(),
                                generic_params: vec![],
                                fields: vec![],
                                is_pub: true,
                                span: p.span,
                            }),
                        },
                    );
                }
                if !table.cells.contains_key(&p.name) {
                    table.cells.insert(
                        p.name.clone(),
                        CellInfo {
                            params: vec![],
                            return_type: Some(TypeExpr::Named(p.name.clone(), p.span)),
                            effects: vec![],
                        },
                    );
                }
                for cell in &p.cells {
                    let method_name = format!("{}.{}", p.name, cell.name);
                    table.cells.entry(method_name).or_insert(CellInfo {
                        params: cell
                            .params
                            .iter()
                            .map(|p| (p.name.clone(), p.ty.clone()))
                            .collect(),
                        return_type: cell.return_type.clone(),
                        effects: cell.effects.clone(),
                    });
                }
                for g in &p.grants {
                    table.tools.entry(g.tool_alias.clone()).or_insert(ToolInfo {
                        tool_path: g.tool_alias.to_lowercase(),
                        mcp_url: None,
                    });
                }
            }
            Item::Effect(e) => {
                table.effects.insert(
                    e.name.clone(),
                    EffectInfo {
                        name: e.name.clone(),
                        operations: e.operations.iter().map(|c| c.name.clone()).collect(),
                    },
                );
                for op in &e.operations {
                    let fq_name = format!("{}.{}", e.name, op.name);
                    table.cells.entry(fq_name).or_insert(CellInfo {
                        params: op
                            .params
                            .iter()
                            .map(|p| (p.name.clone(), p.ty.clone()))
                            .collect(),
                        return_type: op.return_type.clone(),
                        effects: op.effects.clone(),
                    });
                }
            }
            Item::EffectBind(b) => {
                table.effect_binds.push(EffectBindInfo {
                    effect_path: b.effect_path.clone(),
                    tool_alias: b.tool_alias.clone(),
                });
                table.tools.entry(b.tool_alias.clone()).or_insert(ToolInfo {
                    tool_path: b.tool_alias.to_lowercase(),
                    mcp_url: None,
                });
            }
            Item::Handler(h) => {
                table.handlers.insert(
                    h.name.clone(),
                    HandlerInfo {
                        name: h.name.clone(),
                        handles: h.handles.iter().map(|c| c.name.clone()).collect(),
                    },
                );
                for handle in &h.handles {
                    let fq_name = format!("{}.{}", h.name, handle.name);
                    table.cells.entry(fq_name).or_insert(CellInfo {
                        params: handle
                            .params
                            .iter()
                            .map(|p| (p.name.clone(), p.ty.clone()))
                            .collect(),
                        return_type: handle.return_type.clone(),
                        effects: handle.effects.clone(),
                    });
                }
            }
            Item::Addon(a) => {
                table.addons.push(AddonInfo {
                    kind: a.kind.clone(),
                    name: a.name.clone(),
                });
            }
            Item::UseTool(u) => {
                table.tools.insert(
                    u.alias.clone(),
                    ToolInfo {
                        tool_path: u.tool_path.clone(),
                        mcp_url: u.mcp_url.clone(),
                    },
                );
            }
            Item::Grant(_) => {} // Grants reference tools, checked below
            Item::TypeAlias(ta) => {
                table
                    .type_aliases
                    .insert(ta.name.clone(), ta.type_expr.clone());
            }
            Item::Trait(t) => {
                let methods: Vec<String> = t.methods.iter().map(|m| m.name.clone()).collect();
                table.traits.insert(
                    t.name.clone(),
                    TraitInfo {
                        name: t.name.clone(),
                        methods,
                    },
                );
            }
            Item::Impl(i) => {
                let methods: Vec<String> = i.cells.iter().map(|m| m.name.clone()).collect();
                table.impls.push(ImplInfo {
                    trait_name: Some(i.trait_name.clone()),
                    target_type: i.target_type.clone(),
                    methods,
                });
            }
            Item::ConstDecl(c) => {
                table.consts.insert(
                    c.name.clone(),
                    ConstInfo {
                        name: c.name.clone(),
                        ty: c.type_ann.clone(),
                        value: Some(c.value.clone()),
                    },
                );
            }
            Item::Import(_) | Item::MacroDecl(_) => {}
        }
    }

    // Second pass: verify all type references exist
    for item in &program.items {
        match item {
            Item::Record(r) => {
                let generics: Vec<String> =
                    r.generic_params.iter().map(|g| g.name.clone()).collect();
                for field in &r.fields {
                    check_type_refs_with_generics(&field.ty, &table, &mut errors, &generics);
                }
            }
            Item::Cell(c) => {
                if c.body.is_empty() {
                    continue;
                }
                let generics: Vec<String> =
                    c.generic_params.iter().map(|g| g.name.clone()).collect();
                for p in &c.params {
                    check_type_refs_with_generics(&p.ty, &table, &mut errors, &generics);
                }
                if let Some(ref rt) = c.return_type {
                    check_type_refs_with_generics(rt, &table, &mut errors, &generics);
                }
                check_effect_grants(c, &table, &mut errors);
            }
            Item::Agent(a) => {
                for c in &a.cells {
                    if c.body.is_empty() {
                        continue;
                    }
                    let generics: Vec<String> =
                        c.generic_params.iter().map(|g| g.name.clone()).collect();
                    for p in &c.params {
                        check_type_refs_with_generics(&p.ty, &table, &mut errors, &generics);
                    }
                    if let Some(ref rt) = c.return_type {
                        check_type_refs_with_generics(rt, &table, &mut errors, &generics);
                    }
                    check_effect_grants(c, &table, &mut errors);
                }
            }
            Item::Process(p) => {
                for c in &p.cells {
                    if c.body.is_empty() {
                        continue;
                    }
                    let generics: Vec<String> =
                        c.generic_params.iter().map(|g| g.name.clone()).collect();
                    for par in &c.params {
                        check_type_refs_with_generics(&par.ty, &table, &mut errors, &generics);
                    }
                    if let Some(ref rt) = c.return_type {
                        check_type_refs_with_generics(rt, &table, &mut errors, &generics);
                    }
                    check_effect_grants(c, &table, &mut errors);
                }
                for g in &p.grants {
                    table.tools.entry(g.tool_alias.clone()).or_insert(ToolInfo {
                        tool_path: g.tool_alias.to_lowercase(),
                        mcp_url: None,
                    });
                }
            }
            Item::Effect(e) => {
                for c in &e.operations {
                    let generics: Vec<String> =
                        c.generic_params.iter().map(|g| g.name.clone()).collect();
                    for p in &c.params {
                        check_type_refs_with_generics(&p.ty, &table, &mut errors, &generics);
                    }
                    if let Some(ref rt) = c.return_type {
                        check_type_refs_with_generics(rt, &table, &mut errors, &generics);
                    }
                }
            }
            Item::EffectBind(b) => {
                table.tools.entry(b.tool_alias.clone()).or_insert(ToolInfo {
                    tool_path: b.tool_alias.to_lowercase(),
                    mcp_url: None,
                });
            }
            Item::Handler(h) => {
                for c in &h.handles {
                    let generics: Vec<String> =
                        c.generic_params.iter().map(|g| g.name.clone()).collect();
                    for p in &c.params {
                        check_type_refs_with_generics(&p.ty, &table, &mut errors, &generics);
                    }
                    if let Some(ref rt) = c.return_type {
                        check_type_refs_with_generics(rt, &table, &mut errors, &generics);
                    }
                    if !c.body.is_empty() {
                        check_effect_grants(c, &table, &mut errors);
                    }
                }
            }
            Item::Grant(g) => {
                table.tools.entry(g.tool_alias.clone()).or_insert(ToolInfo {
                    tool_path: g.tool_alias.to_lowercase(),
                    mcp_url: None,
                });
            }
            Item::Addon(_) => {}
            _ => {}
        }
    }

    apply_effect_inference(program, &mut table, &mut errors);

    if errors.is_empty() {
        Ok(table)
    } else {
        Err(errors)
    }
}

fn check_effect_grants(cell: &CellDef, table: &SymbolTable, errors: &mut Vec<ResolveError>) {
    check_effect_grants_for(&cell.name, cell.span.line, &cell.effects, table, errors);
}

fn check_effect_grants_for(
    cell_name: &str,
    line: usize,
    effects: &[String],
    table: &SymbolTable,
    errors: &mut Vec<ResolveError>,
) {
    if effects.is_empty() {
        return;
    }
    if table.tools.is_empty() {
        return;
    }

    // Grants are represented as top-level declarations today, so we use
    // declared tools as a conservative capability proxy.
    let granted_tools: Vec<&ToolInfo> = table.tools.values().collect();

    for effect in effects {
        let effect = normalize_effect(effect);
        if matches!(
            effect.as_str(),
            "pure" | "trace" | "state" | "approve" | "emit" | "cache"
        ) {
            continue;
        }

        let mut satisfied = false;
        for tool in &granted_tools {
            let path = tool.tool_path.to_lowercase();
            let has_mcp = tool.mcp_url.is_some();
            satisfied = match effect.as_str() {
                "http" => path.contains("http"),
                "llm" => path.contains("llm") || path.contains("chat"),
                "fs" => path.contains("fs") || path.contains("file"),
                "database" => {
                    path.contains("db") || path.contains("sql") || path.contains("postgres")
                }
                "email" => path.contains("email"),
                "mcp" => has_mcp,
                _ => true,
            };
            if satisfied {
                break;
            }
        }

        if !satisfied {
            errors.push(ResolveError::MissingEffectGrant {
                cell: cell_name.to_string(),
                effect,
                line,
            });
        }
    }
}

#[derive(Debug, Clone)]
struct EffectCell {
    name: String,
    declared: Vec<String>,
    body: Vec<Stmt>,
    line: usize,
}

fn normalize_effect(effect: &str) -> String {
    effect.trim().to_ascii_lowercase()
}

fn parse_directive_bool(program: &Program, name: &str) -> Option<bool> {
    let raw = program
        .directives
        .iter()
        .find(|d| d.name.eq_ignore_ascii_case(name))?
        .value
        .as_deref()
        .unwrap_or("true")
        .trim()
        .to_ascii_lowercase();
    match raw.as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn collect_effect_cells(program: &Program) -> Vec<EffectCell> {
    let mut out = Vec::new();
    for item in &program.items {
        match item {
            Item::Cell(c) => out.push(EffectCell {
                name: c.name.clone(),
                declared: c.effects.clone(),
                body: c.body.clone(),
                line: c.span.line,
            }),
            Item::Agent(a) => {
                for c in &a.cells {
                    out.push(EffectCell {
                        name: format!("{}.{}", a.name, c.name),
                        declared: c.effects.clone(),
                        body: c.body.clone(),
                        line: c.span.line,
                    });
                }
            }
            Item::Process(p) => {
                for c in &p.cells {
                    out.push(EffectCell {
                        name: format!("{}.{}", p.name, c.name),
                        declared: c.effects.clone(),
                        body: c.body.clone(),
                        line: c.span.line,
                    });
                }
            }
            Item::Effect(e) => {
                for op in &e.operations {
                    out.push(EffectCell {
                        name: format!("{}.{}", e.name, op.name),
                        declared: op.effects.clone(),
                        body: op.body.clone(),
                        line: op.span.line,
                    });
                }
            }
            Item::Handler(h) => {
                for handle in &h.handles {
                    out.push(EffectCell {
                        name: format!("{}.{}", h.name, handle.name),
                        declared: handle.effects.clone(),
                        body: handle.body.clone(),
                        line: handle.span.line,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

fn effect_from_tool(alias: &str, table: &SymbolTable) -> Option<String> {
    if let Some(bind) = table.effect_binds.iter().find(|b| b.tool_alias == alias) {
        let root = bind
            .effect_path
            .split('.')
            .next()
            .unwrap_or(bind.effect_path.as_str());
        return Some(normalize_effect(root));
    }

    let lower = alias.to_ascii_lowercase();
    if lower.contains("http") {
        return Some("http".into());
    }
    if lower.contains("llm") || lower.contains("chat") {
        return Some("llm".into());
    }
    if lower.contains("db") || lower.contains("sql") || lower.contains("postgres") {
        return Some("database".into());
    }
    if lower.contains("email") {
        return Some("email".into());
    }
    if lower.contains("file") || lower.contains("fs") {
        return Some("fs".into());
    }
    if let Some(tool) = table.tools.get(alias) {
        let path = tool.tool_path.to_ascii_lowercase();
        if path.contains("http") {
            return Some("http".into());
        }
        if path.contains("llm") || path.contains("chat") {
            return Some("llm".into());
        }
        if path.contains("db") || path.contains("sql") || path.contains("postgres") {
            return Some("database".into());
        }
        if path.contains("email") {
            return Some("email".into());
        }
        if path.contains("file") || path.contains("fs") {
            return Some("fs".into());
        }
        if tool.mcp_url.is_some() {
            return Some("mcp".into());
        }
    }
    None
}

fn infer_pattern_effects(
    pat: &Pattern,
    table: &SymbolTable,
    current: &HashMap<String, BTreeSet<String>>,
    out: &mut BTreeSet<String>,
) {
    match pat {
        Pattern::Guard {
            inner, condition, ..
        } => {
            infer_pattern_effects(inner, table, current, out);
            infer_expr_effects(condition, table, current, out);
        }
        Pattern::Or { patterns, .. } => {
            for p in patterns {
                infer_pattern_effects(p, table, current, out);
            }
        }
        Pattern::ListDestructure { elements, .. } | Pattern::TupleDestructure { elements, .. } => {
            for p in elements {
                infer_pattern_effects(p, table, current, out);
            }
        }
        Pattern::RecordDestructure { fields, .. } => {
            for (_, p) in fields {
                if let Some(p) = p {
                    infer_pattern_effects(p, table, current, out);
                }
            }
        }
        _ => {}
    }
}

fn infer_stmt_effects(
    stmt: &Stmt,
    table: &SymbolTable,
    current: &HashMap<String, BTreeSet<String>>,
    out: &mut BTreeSet<String>,
) {
    match stmt {
        Stmt::Let(s) => infer_expr_effects(&s.value, table, current, out),
        Stmt::If(s) => {
            infer_expr_effects(&s.condition, table, current, out);
            for st in &s.then_body {
                infer_stmt_effects(st, table, current, out);
            }
            if let Some(else_body) = &s.else_body {
                for st in else_body {
                    infer_stmt_effects(st, table, current, out);
                }
            }
        }
        Stmt::For(s) => {
            infer_expr_effects(&s.iter, table, current, out);
            for st in &s.body {
                infer_stmt_effects(st, table, current, out);
            }
        }
        Stmt::Match(s) => {
            infer_expr_effects(&s.subject, table, current, out);
            for arm in &s.arms {
                infer_pattern_effects(&arm.pattern, table, current, out);
                for st in &arm.body {
                    infer_stmt_effects(st, table, current, out);
                }
            }
        }
        Stmt::Return(s) => infer_expr_effects(&s.value, table, current, out),
        Stmt::Halt(s) => infer_expr_effects(&s.message, table, current, out),
        Stmt::Assign(s) => infer_expr_effects(&s.value, table, current, out),
        Stmt::Expr(s) => infer_expr_effects(&s.expr, table, current, out),
        Stmt::While(s) => {
            infer_expr_effects(&s.condition, table, current, out);
            for st in &s.body {
                infer_stmt_effects(st, table, current, out);
            }
        }
        Stmt::Loop(s) => {
            for st in &s.body {
                infer_stmt_effects(st, table, current, out);
            }
        }
        Stmt::Emit(s) => {
            infer_expr_effects(&s.value, table, current, out);
            out.insert("emit".into());
        }
        Stmt::CompoundAssign(s) => infer_expr_effects(&s.value, table, current, out),
        Stmt::Break(_) | Stmt::Continue(_) => {}
    }
}

fn infer_expr_effects(
    expr: &Expr,
    table: &SymbolTable,
    current: &HashMap<String, BTreeSet<String>>,
    out: &mut BTreeSet<String>,
) {
    match expr {
        Expr::BinOp(lhs, _, rhs, _) | Expr::NullCoalesce(lhs, rhs, _) => {
            infer_expr_effects(lhs, table, current, out);
            infer_expr_effects(rhs, table, current, out);
        }
        Expr::UnaryOp(_, inner, _)
        | Expr::ExpectSchema(inner, _, _)
        | Expr::TryExpr(inner, _)
        | Expr::AwaitExpr(inner, _)
        | Expr::NullAssert(inner, _)
        | Expr::SpreadExpr(inner, _) => {
            infer_expr_effects(inner, table, current, out);
            if matches!(expr, Expr::AwaitExpr(_, _)) {
                out.insert("async".into());
            }
        }
        Expr::Call(callee, args, _) => {
            infer_expr_effects(callee, table, current, out);
            for a in args {
                match a {
                    CallArg::Positional(e) | CallArg::Named(_, e, _) | CallArg::Role(_, e, _) => {
                        infer_expr_effects(e, table, current, out)
                    }
                }
            }
            match callee.as_ref() {
                Expr::Ident(name, _) => {
                    if let Some(effects) = current.get(name) {
                        out.extend(effects.iter().cloned());
                    }
                    if name == "emit" || name == "print" {
                        out.insert("emit".into());
                    }
                    if name == "parallel" || name == "race" {
                        out.insert("async".into());
                    }
                }
                Expr::DotAccess(obj, field, _) => {
                    if let Expr::Ident(owner, _) = obj.as_ref() {
                        let fq = format!("{}.{}", owner, field);
                        if let Some(effects) = current.get(&fq) {
                            out.extend(effects.iter().cloned());
                        }
                        if let Some(process) = table.processes.values().find(|p| p.name == *owner) {
                            match process.kind.as_str() {
                                "memory" => {
                                    if matches!(
                                        field.as_str(),
                                        "append"
                                            | "remember"
                                            | "upsert"
                                            | "store"
                                            | "recent"
                                            | "recall"
                                            | "query"
                                            | "get"
                                    ) {
                                        out.insert("state".into());
                                    }
                                }
                                "machine" => {
                                    if matches!(
                                        field.as_str(),
                                        "run"
                                            | "start"
                                            | "step"
                                            | "is_terminal"
                                            | "current_state"
                                            | "resume_from"
                                    ) {
                                        out.insert("state".into());
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Expr::ToolCall(callee, args, _) => {
            for a in args {
                match a {
                    CallArg::Positional(e) | CallArg::Named(_, e, _) | CallArg::Role(_, e, _) => {
                        infer_expr_effects(e, table, current, out)
                    }
                }
            }
            if let Expr::Ident(alias, _) = callee.as_ref() {
                if let Some(effect) = effect_from_tool(alias, table) {
                    out.insert(effect);
                }
            }
        }
        Expr::ListLit(items, _) | Expr::TupleLit(items, _) | Expr::SetLit(items, _) => {
            for e in items {
                infer_expr_effects(e, table, current, out);
            }
        }
        Expr::MapLit(items, _) => {
            for (k, v) in items {
                infer_expr_effects(k, table, current, out);
                infer_expr_effects(v, table, current, out);
            }
        }
        Expr::RecordLit(_, fields, _) => {
            for (_, e) in fields {
                infer_expr_effects(e, table, current, out);
            }
        }
        Expr::DotAccess(obj, _, _) | Expr::NullSafeAccess(obj, _, _) => {
            infer_expr_effects(obj, table, current, out);
        }
        Expr::IndexAccess(obj, idx, _) => {
            infer_expr_effects(obj, table, current, out);
            infer_expr_effects(idx, table, current, out);
        }
        Expr::RoleBlock(_, inner, _) => infer_expr_effects(inner, table, current, out),
        Expr::Lambda { body, .. } => match body {
            LambdaBody::Expr(e) => infer_expr_effects(e, table, current, out),
            LambdaBody::Block(stmts) => {
                for s in stmts {
                    infer_stmt_effects(s, table, current, out);
                }
            }
        },
        Expr::IfExpr {
            cond,
            then_val,
            else_val,
            ..
        } => {
            infer_expr_effects(cond, table, current, out);
            infer_expr_effects(then_val, table, current, out);
            infer_expr_effects(else_val, table, current, out);
        }
        Expr::Comprehension {
            body,
            iter,
            condition,
            ..
        } => {
            infer_expr_effects(iter, table, current, out);
            if let Some(c) = condition {
                infer_expr_effects(c, table, current, out);
            }
            infer_expr_effects(body, table, current, out);
        }
        Expr::IntLit(_, _)
        | Expr::FloatLit(_, _)
        | Expr::StringLit(_, _)
        | Expr::StringInterp(_, _)
        | Expr::BoolLit(_, _)
        | Expr::NullLit(_)
        | Expr::Ident(_, _)
        | Expr::RawStringLit(_, _)
        | Expr::BytesLit(_, _)
        | Expr::RangeExpr { .. } => {}
    }
}

fn infer_cell_effects(
    cell: &EffectCell,
    table: &SymbolTable,
    current: &HashMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for s in &cell.body {
        infer_stmt_effects(s, table, current, &mut out);
    }
    out
}

fn apply_effect_inference(
    program: &Program,
    table: &mut SymbolTable,
    errors: &mut Vec<ResolveError>,
) {
    let strict = parse_directive_bool(program, "strict").unwrap_or(true);
    let doc_mode = parse_directive_bool(program, "doc_mode").unwrap_or(false);
    let enforce_declared_effect_rows = strict && !doc_mode;
    let cells = collect_effect_cells(program);
    if cells.is_empty() {
        return;
    }

    let mut effective: HashMap<String, BTreeSet<String>> = HashMap::new();
    for cell in &cells {
        let declared: BTreeSet<String> = cell.declared.iter().map(|e| normalize_effect(e)).collect();
        effective.insert(
            cell.name.clone(),
            if declared.is_empty() {
                BTreeSet::new()
            } else {
                declared
            },
        );
    }

    for _ in 0..32 {
        let mut changed = false;
        for cell in &cells {
            if !cell.declared.is_empty() {
                continue;
            }
            let inferred = infer_cell_effects(cell, table, &effective);
            let entry = effective.entry(cell.name.clone()).or_default();
            if *entry != inferred {
                *entry = inferred;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    for cell in &cells {
        let inferred = infer_cell_effects(cell, table, &effective);
        let declared: BTreeSet<String> = cell.declared.iter().map(|e| normalize_effect(e)).collect();
        let final_effects = if declared.is_empty() {
            inferred.clone()
        } else {
            if enforce_declared_effect_rows {
                for missing in inferred.difference(&declared) {
                    errors.push(ResolveError::UndeclaredEffect {
                        cell: cell.name.clone(),
                        effect: missing.clone(),
                        line: cell.line,
                    });
                }
            }
            declared
        };

        if cell.declared.is_empty() {
            let inferred_vec: Vec<String> = final_effects.iter().cloned().collect();
            check_effect_grants_for(&cell.name, cell.line, &inferred_vec, table, errors);
        }

        if let Some(info) = table.cells.get_mut(&cell.name) {
            info.effects = final_effects.iter().cloned().collect();
        }
    }
}

fn check_type_refs_with_generics(
    ty: &TypeExpr,
    table: &SymbolTable,
    errors: &mut Vec<ResolveError>,
    generics: &[String],
) {
    match ty {
        TypeExpr::Named(name, span) => {
            if generics.iter().any(|g| g == name) {
                return;
            }
            if !table.types.contains_key(name) {
                errors.push(ResolveError::UndefinedType {
                    name: name.clone(),
                    line: span.line,
                });
            }
        }
        TypeExpr::List(inner, _) => check_type_refs_with_generics(inner, table, errors, generics),
        TypeExpr::Map(k, v, _) => {
            check_type_refs_with_generics(k, table, errors, generics);
            check_type_refs_with_generics(v, table, errors, generics);
        }
        TypeExpr::Result(ok, err, _) => {
            check_type_refs_with_generics(ok, table, errors, generics);
            check_type_refs_with_generics(err, table, errors, generics);
        }
        TypeExpr::Union(types, _) => {
            for t in types {
                check_type_refs_with_generics(t, table, errors, generics);
            }
        }
        TypeExpr::Null(_) => {}
        TypeExpr::Tuple(types, _) => {
            for t in types {
                check_type_refs_with_generics(t, table, errors, generics);
            }
        }
        TypeExpr::Set(inner, _) => check_type_refs_with_generics(inner, table, errors, generics),
        TypeExpr::Fn(params, ret, _, _) => {
            for t in params {
                check_type_refs_with_generics(t, table, errors, generics);
            }
            check_type_refs_with_generics(ret, table, errors, generics);
        }
        TypeExpr::Generic(name, args, span) => {
            if !table.types.contains_key(name) {
                errors.push(ResolveError::UndefinedType {
                    name: name.clone(),
                    line: span.line,
                });
            }
            for t in args {
                check_type_refs_with_generics(t, table, errors, generics);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::lexer::Lexer;
    use crate::compiler::parser::Parser;
    use crate::compiler::tokens::Span;

    fn resolve_src(src: &str) -> Result<SymbolTable, Vec<ResolveError>> {
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let prog = parser.parse_program(vec![]).unwrap();
        resolve(&prog)
    }

    fn s() -> Span {
        Span {
            start: 0,
            end: 0,
            line: 1,
            col: 1,
        }
    }

    #[test]
    fn test_resolve_basic() {
        let table =
            resolve_src("record Foo\n  x: Int\nend\n\ncell main() -> Foo\n  return Foo(x: 1)\nend")
                .unwrap();
        assert!(table.types.contains_key("Foo"));
        assert!(table.cells.contains_key("main"));
    }

    #[test]
    fn test_resolve_undefined_type() {
        let err = resolve_src("record Bar\n  x: Unknown\nend").unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn test_effect_inference_for_implicit_row() {
        let table = resolve_src("cell main() -> Int\n  emit(\"x\")\n  return 1\nend").unwrap();
        let effects = &table.cells.get("main").unwrap().effects;
        assert!(effects.contains(&"emit".to_string()));
    }

    #[test]
    fn test_effect_inference_transitive_cell_call() {
        let table = resolve_src(
            "cell a() -> Int / {emit}\n  emit(\"x\")\n  return 1\nend\n\ncell b() -> Int\n  return a()\nend",
        )
        .unwrap();
        let effects = &table.cells.get("b").unwrap().effects;
        assert!(effects.contains(&"emit".to_string()));
    }

    #[test]
    fn test_undeclared_effect_error_in_strict_mode() {
        let sp = s();
        let program = Program {
            directives: vec![],
            items: vec![Item::Cell(CellDef {
                name: "main".into(),
                generic_params: vec![],
                params: vec![],
                return_type: Some(TypeExpr::Named("Int".into(), sp)),
                effects: vec!["emit".into()],
                body: vec![Stmt::Expr(ExprStmt {
                    expr: Expr::Call(
                        Box::new(Expr::Ident("parallel".into(), sp)),
                        vec![CallArg::Positional(Expr::IntLit(1, sp))],
                        sp,
                    ),
                    span: sp,
                })],
                is_pub: false,
                is_async: false,
                where_clauses: vec![],
                span: sp,
            })],
            span: sp,
        };
        let err = resolve(&program).unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::UndeclaredEffect { cell, effect, .. } if cell == "main" && effect == "async"
        )));
    }

    #[test]
    fn test_doc_mode_allows_undeclared_effects() {
        let sp = s();
        let program = Program {
            directives: vec![Directive {
                name: "doc_mode".into(),
                value: Some("true".into()),
                span: sp,
            }],
            items: vec![Item::Cell(CellDef {
                name: "main".into(),
                generic_params: vec![],
                params: vec![],
                return_type: Some(TypeExpr::Named("Int".into(), sp)),
                effects: vec!["emit".into()],
                body: vec![Stmt::Expr(ExprStmt {
                    expr: Expr::Call(
                        Box::new(Expr::Ident("parallel".into(), sp)),
                        vec![CallArg::Positional(Expr::IntLit(1, sp))],
                        sp,
                    ),
                    span: sp,
                })],
                is_pub: false,
                is_async: false,
                where_clauses: vec![],
                span: sp,
            })],
            span: sp,
        };
        let table = resolve(&program).unwrap();
        assert!(table.cells.contains_key("main"));
    }
}
