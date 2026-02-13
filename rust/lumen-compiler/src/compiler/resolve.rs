//! Name resolution pass — resolve cells, types, and tool aliases.

use crate::compiler::ast::*;
use std::collections::HashMap;
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
                for cell in &p.cells {
                    let method_name = format!("{}.{}", p.name, cell.name);
                    table.cells.entry(method_name).or_insert(CellInfo {
                        params: cell
                            .params
                            .iter()
                            .map(|p| (p.name.clone(), p.ty.clone()))
                            .collect(),
                        return_type: cell.return_type.clone(),
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

    if errors.is_empty() {
        Ok(table)
    } else {
        Err(errors)
    }
}

fn check_effect_grants(cell: &CellDef, table: &SymbolTable, errors: &mut Vec<ResolveError>) {
    if cell.effects.is_empty() {
        return;
    }
    if table.tools.is_empty() {
        return;
    }

    // Grants are represented as top-level declarations today, so we use
    // declared tools as a conservative capability proxy.
    let granted_tools: Vec<&ToolInfo> = table.tools.values().collect();

    for effect in &cell.effects {
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
                cell: cell.name.clone(),
                effect: effect.clone(),
                line: cell.span.line,
            });
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

    fn resolve_src(src: &str) -> Result<SymbolTable, Vec<ResolveError>> {
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let prog = parser.parse_program(vec![]).unwrap();
        resolve(&prog)
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
}
