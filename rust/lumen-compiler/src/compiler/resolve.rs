//! Name resolution pass — resolve cells, types, and tool aliases.

use crate::compiler::ast::*;
use std::collections::{BTreeSet, HashMap, HashSet};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("undefined type '{name}' at line {line}")]
    UndefinedType {
        name: String,
        line: usize,
        suggestions: Vec<String>,
    },
    #[error(
        "generic type '{name}' has wrong number of type arguments at line {line}: expected {expected}, got {actual}"
    )]
    GenericArityMismatch {
        name: String,
        expected: usize,
        actual: usize,
        line: usize,
    },
    #[error("undefined cell '{name}' at line {line}")]
    UndefinedCell {
        name: String,
        line: usize,
        suggestions: Vec<String>,
    },
    #[error("undefined trait '{name}' at line {line}")]
    UndefinedTrait { name: String, line: usize },
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
    #[error("cell '{cell}' performs effect '{effect}' but it is not declared in its effect row (line {line}){cause}")]
    UndeclaredEffect {
        cell: String,
        effect: String,
        line: usize,
        cause: String,
    },
    #[error("cell '{caller}' calls '{callee}' which requires effect '{effect}' not present in caller effect row (line {line})")]
    EffectContractViolation {
        caller: String,
        callee: String,
        effect: String,
        line: usize,
    },
    #[error("cell '{cell}' uses nondeterministic operation/effect '{operation}' under @deterministic (line {line})")]
    NondeterministicOperation {
        cell: String,
        operation: String,
        line: usize,
    },
    #[error("machine '{machine}' initial state '{state}' is undefined (line {line})")]
    MachineUnknownInitial {
        machine: String,
        state: String,
        line: usize,
    },
    #[error("machine '{machine}' state '{state}' transitions to undefined state '{target}' (line {line})")]
    MachineUnknownTransition {
        machine: String,
        state: String,
        target: String,
        line: usize,
    },
    #[error("machine '{machine}' state '{state}' is unreachable from initial state '{initial}' (line {line})")]
    MachineUnreachableState {
        machine: String,
        state: String,
        initial: String,
        line: usize,
    },
    #[error("machine '{machine}' declares no terminal states (line {line})")]
    MachineMissingTerminal { machine: String, line: usize },
    #[error("machine '{machine}' state '{state}' transition arg count mismatch for '{target}' at line {line}: expected {expected}, got {actual}")]
    MachineTransitionArgCount {
        machine: String,
        state: String,
        target: String,
        expected: usize,
        actual: usize,
        line: usize,
    },
    #[error("machine '{machine}' state '{state}' transition arg type mismatch for '{target}' at line {line}: expected {expected}, got {actual}")]
    MachineTransitionArgType {
        machine: String,
        state: String,
        target: String,
        expected: String,
        actual: String,
        line: usize,
    },
    #[error("machine '{machine}' state '{state}' has unsupported expression in {context} at line {line}")]
    MachineUnsupportedExpr {
        machine: String,
        state: String,
        context: String,
        line: usize,
    },
    #[error("machine '{machine}' state '{state}' guard must be Bool-compatible, got {actual} at line {line}")]
    MachineGuardType {
        machine: String,
        state: String,
        actual: String,
        line: usize,
    },
    #[error("pipeline '{pipeline}' references unknown stage cell '{stage}' at line {line}")]
    PipelineUnknownStage {
        pipeline: String,
        stage: String,
        line: usize,
    },
    #[error("pipeline '{pipeline}' stage '{stage}' has invalid arity at line {line}: expected exactly one data argument")]
    PipelineStageArity {
        pipeline: String,
        stage: String,
        line: usize,
    },
    #[error("pipeline '{pipeline}' stage type mismatch from '{from_stage}' to '{to_stage}' at line {line}: expected {expected}, got {actual}")]
    PipelineStageTypeMismatch {
        pipeline: String,
        from_stage: String,
        to_stage: String,
        expected: String,
        actual: String,
        line: usize,
    },
    #[error(
        "circular import detected: module '{module}' is already being compiled (chain: {chain})"
    )]
    CircularImport { module: String, chain: String },
    #[error("module '{module}' not found at line {line}")]
    ModuleNotFound { module: String, line: usize },
    #[error("imported symbol '{symbol}' not found in module '{module}' at line {line}")]
    ImportedSymbolNotFound {
        symbol: String,
        module: String,
        line: usize,
    },
    #[error(
        "impl for trait '{trait_name}' on '{target_type}' is missing required methods {missing:?} at line {line}"
    )]
    TraitMissingMethods {
        trait_name: String,
        target_type: String,
        missing: Vec<String>,
        line: usize,
    },
    #[error(
        "impl method '{method}' for trait '{trait_name}' on '{target_type}' has incompatible signature at line {line}: {reason}. expected `{expected}`, found `{actual}`"
    )]
    TraitMethodSignatureMismatch {
        trait_name: String,
        target_type: String,
        method: String,
        reason: String,
        expected: String,
        actual: String,
        line: usize,
    },
}

/// Symbol table built during resolution
#[derive(Debug, Clone)]
pub struct SymbolTable {
    pub types: HashMap<String, TypeInfo>,
    pub cells: HashMap<String, CellInfo>,
    pub cell_policies: HashMap<String, Vec<GrantPolicy>>,
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
    pub generic_params: Vec<String>,
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
    pub pipeline_stages: Vec<String>,
    pub machine_initial: Option<String>,
    pub machine_states: Vec<MachineStateInfo>,
}

#[derive(Debug, Clone)]
pub struct MachineStateInfo {
    pub name: String,
    pub params: Vec<(String, TypeExpr)>,
    pub terminal: bool,
    pub guard: Option<Expr>,
    pub transition_to: Option<String>,
    pub transition_args: Vec<Expr>,
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
    pub parent_traits: Vec<String>,
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

#[derive(Debug, Clone)]
pub struct GrantPolicy {
    pub tool_alias: String,
    pub allowed_effects: Option<BTreeSet<String>>,
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolTable {
    pub fn new() -> Self {
        let mut types = HashMap::new();
        // Register builtin types
        for name in &[
            "String", "Int", "Float", "Bool", "Bytes", "Json", "Null", "Self",
        ] {
            types.insert(
                name.to_string(),
                TypeInfo {
                    kind: TypeInfoKind::Builtin,
                    generic_params: vec![],
                },
            );
        }
        Self {
            types,
            cells: HashMap::new(),
            cell_policies: HashMap::new(),
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

    /// Import a cell from an external module
    pub fn import_cell(&mut self, name: String, info: CellInfo) {
        self.cells.insert(name, info);
    }

    /// Import a type from an external module
    pub fn import_type(&mut self, name: String, info: TypeInfo) {
        self.types.insert(name, info);
    }

    /// Import a type alias from an external module
    pub fn import_type_alias(&mut self, name: String, type_expr: TypeExpr) {
        self.type_aliases.insert(name, type_expr);
    }
}

/// Resolve all names in a program, building the symbol table.
pub fn resolve(program: &Program) -> Result<SymbolTable, Vec<ResolveError>> {
    resolve_with_base(program, SymbolTable::new())
}

/// Resolve all names in a program, using a pre-populated symbol table as the base.
/// This is useful for multi-file compilation where imported symbols need to be available.
pub fn resolve_with_base(
    program: &Program,
    mut table: SymbolTable,
) -> Result<SymbolTable, Vec<ResolveError>> {
    let mut errors = Vec::new();
    let doc_mode = parse_directive_bool(program, "doc_mode").unwrap_or(false);

    // First pass: register all type and cell definitions
    for item in &program.items {
        use std::collections::hash_map::Entry;
        match item {
            Item::Record(r) => match table.types.entry(r.name.clone()) {
                Entry::Occupied(_) => {
                    errors.push(ResolveError::Duplicate {
                        name: r.name.clone(),
                        line: r.span.line,
                    });
                }
                Entry::Vacant(entry) => {
                    entry.insert(TypeInfo {
                        kind: TypeInfoKind::Record(r.clone()),
                        generic_params: r.generic_params.iter().map(|gp| gp.name.clone()).collect(),
                    });
                }
            },
            Item::Enum(e) => match table.types.entry(e.name.clone()) {
                Entry::Occupied(_) => {
                    errors.push(ResolveError::Duplicate {
                        name: e.name.clone(),
                        line: e.span.line,
                    });
                }
                Entry::Vacant(entry) => {
                    entry.insert(TypeInfo {
                        kind: TypeInfoKind::Enum(e.clone()),
                        generic_params: e.generic_params.iter().map(|gp| gp.name.clone()).collect(),
                    });
                }
            },
            Item::Cell(c) => match table.cells.entry(c.name.clone()) {
                Entry::Occupied(_) => {
                    errors.push(ResolveError::Duplicate {
                        name: c.name.clone(),
                        line: c.span.line,
                    });
                }
                Entry::Vacant(entry) => {
                    entry.insert(CellInfo {
                        params: c
                            .params
                            .iter()
                            .map(|p| (p.name.clone(), p.ty.clone()))
                            .collect(),
                        return_type: c.return_type.clone(),
                        effects: c.effects.clone(),
                    });
                }
            },
            Item::Agent(a) => {
                match table.agents.entry(a.name.clone()) {
                    Entry::Occupied(_) => {
                        errors.push(ResolveError::Duplicate {
                            name: a.name.clone(),
                            line: a.span.line,
                        });
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(AgentInfo {
                            name: a.name.clone(),
                            methods: a.cells.iter().map(|c| c.name.clone()).collect(),
                        });
                    }
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
                            generic_params: vec![],
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
                    match table.cells.entry(method_name.clone()) {
                        Entry::Occupied(_) => {
                            errors.push(ResolveError::Duplicate {
                                name: method_name,
                                line: cell.span.line,
                            });
                        }
                        Entry::Vacant(entry) => {
                            entry.insert(CellInfo {
                                params: cell
                                    .params
                                    .iter()
                                    .map(|p| (p.name.clone(), p.ty.clone()))
                                    .collect(),
                                return_type: cell.return_type.clone(),
                                effects: cell.effects.clone(),
                            });
                        }
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
                match table.processes.entry(process_key) {
                    Entry::Occupied(_) => {
                        errors.push(ResolveError::Duplicate {
                            name: p.name.clone(),
                            line: p.span.line,
                        });
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(ProcessInfo {
                            kind: p.kind.clone(),
                            name: p.name.clone(),
                            methods: p.cells.iter().map(|c| c.name.clone()).collect(),
                            pipeline_stages: p.pipeline_stages.clone(),
                            machine_initial: p.machine_initial.clone(),
                            machine_states: p
                                .machine_states
                                .iter()
                                .map(|s| MachineStateInfo {
                                    name: s.name.clone(),
                                    params: s
                                        .params
                                        .iter()
                                        .map(|p| (p.name.clone(), p.ty.clone()))
                                        .collect(),
                                    terminal: s.terminal,
                                    guard: s.guard.clone(),
                                    transition_to: s.transition_to.clone(),
                                    transition_args: s.transition_args.clone(),
                                })
                                .collect(),
                        });
                    }
                }
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
                            generic_params: vec![],
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
                match table.effects.entry(e.name.clone()) {
                    Entry::Occupied(_) => {
                        errors.push(ResolveError::Duplicate {
                            name: e.name.clone(),
                            line: e.span.line,
                        });
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(EffectInfo {
                            name: e.name.clone(),
                            operations: e.operations.iter().map(|c| c.name.clone()).collect(),
                        });
                    }
                }
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
                match table.handlers.entry(h.name.clone()) {
                    Entry::Occupied(_) => {
                        errors.push(ResolveError::Duplicate {
                            name: h.name.clone(),
                            line: h.span.line,
                        });
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(HandlerInfo {
                            name: h.name.clone(),
                            handles: h.handles.iter().map(|c| c.name.clone()).collect(),
                        });
                    }
                }
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
            Item::TypeAlias(ta) => match table.type_aliases.entry(ta.name.clone()) {
                Entry::Occupied(_) => {
                    errors.push(ResolveError::Duplicate {
                        name: ta.name.clone(),
                        line: ta.span.line,
                    });
                }
                Entry::Vacant(entry) => {
                    entry.insert(ta.type_expr.clone());
                }
            },
            Item::Trait(t) => {
                let methods: Vec<String> = t.methods.iter().map(|m| m.name.clone()).collect();
                match table.traits.entry(t.name.clone()) {
                    Entry::Occupied(_) => {
                        errors.push(ResolveError::Duplicate {
                            name: t.name.clone(),
                            line: t.span.line,
                        });
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(TraitInfo {
                            name: t.name.clone(),
                            parent_traits: t.parent_traits.clone(),
                            methods,
                        });
                    }
                }
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

    table.cell_policies = build_cell_policies(program);
    let type_alias_arities = collect_type_alias_arities(program);
    let trait_defs = collect_trait_defs(program);

    // Second pass: verify all type references exist
    for item in &program.items {
        match item {
            Item::Record(r) => {
                check_generic_param_bounds(&r.generic_params, &table, &mut errors);
                let generics: Vec<String> =
                    r.generic_params.iter().map(|g| g.name.clone()).collect();
                for field in &r.fields {
                    check_type_refs_with_generics(
                        &field.ty,
                        &table,
                        &type_alias_arities,
                        &mut errors,
                        &generics,
                    );
                }
            }
            Item::Enum(e) => {
                check_generic_param_bounds(&e.generic_params, &table, &mut errors);
                let enum_generics: Vec<String> =
                    e.generic_params.iter().map(|g| g.name.clone()).collect();
                for variant in &e.variants {
                    if let Some(payload) = &variant.payload {
                        check_type_refs_with_generics(
                            payload,
                            &table,
                            &type_alias_arities,
                            &mut errors,
                            &enum_generics,
                        );
                    }
                }
                for method in &e.methods {
                    check_generic_param_bounds(&method.generic_params, &table, &mut errors);
                    let mut method_generics = enum_generics.clone();
                    method_generics.extend(method.generic_params.iter().map(|g| g.name.clone()));
                    for param in &method.params {
                        check_type_refs_with_generics(
                            &param.ty,
                            &table,
                            &type_alias_arities,
                            &mut errors,
                            &method_generics,
                        );
                    }
                    if let Some(return_type) = &method.return_type {
                        check_type_refs_with_generics(
                            return_type,
                            &table,
                            &type_alias_arities,
                            &mut errors,
                            &method_generics,
                        );
                    }
                }
            }
            Item::Cell(c) => {
                if c.body.is_empty() {
                    continue;
                }
                check_generic_param_bounds(&c.generic_params, &table, &mut errors);
                let generics: Vec<String> =
                    c.generic_params.iter().map(|g| g.name.clone()).collect();
                for p in &c.params {
                    check_type_refs_with_generics(
                        &p.ty,
                        &table,
                        &type_alias_arities,
                        &mut errors,
                        &generics,
                    );
                }
                if let Some(ref rt) = c.return_type {
                    check_type_refs_with_generics(
                        rt,
                        &table,
                        &type_alias_arities,
                        &mut errors,
                        &generics,
                    );
                }
                if !doc_mode {
                    check_effect_grants_for(&c.name, c.span.line, &c.effects, &table, &mut errors);
                }
            }
            Item::Agent(a) => {
                for c in &a.cells {
                    if c.body.is_empty() {
                        continue;
                    }
                    check_generic_param_bounds(&c.generic_params, &table, &mut errors);
                    let generics: Vec<String> =
                        c.generic_params.iter().map(|g| g.name.clone()).collect();
                    for p in &c.params {
                        check_type_refs_with_generics(
                            &p.ty,
                            &table,
                            &type_alias_arities,
                            &mut errors,
                            &generics,
                        );
                    }
                    if let Some(ref rt) = c.return_type {
                        check_type_refs_with_generics(
                            rt,
                            &table,
                            &type_alias_arities,
                            &mut errors,
                            &generics,
                        );
                    }
                    if !doc_mode {
                        let fq = format!("{}.{}", a.name, c.name);
                        check_effect_grants_for(&fq, c.span.line, &c.effects, &table, &mut errors);
                    }
                }
            }
            Item::Process(p) => {
                if p.kind == "pipeline" {
                    validate_pipeline_stages(p, &table, &mut errors);
                }
                if p.kind == "machine" {
                    validate_machine_graph(p, &mut errors);
                    for state in &p.machine_states {
                        for param in &state.params {
                            check_type_refs_with_generics(
                                &param.ty,
                                &table,
                                &type_alias_arities,
                                &mut errors,
                                &[],
                            );
                        }
                    }
                }
                for c in &p.cells {
                    if c.body.is_empty() {
                        continue;
                    }
                    check_generic_param_bounds(&c.generic_params, &table, &mut errors);
                    let generics: Vec<String> =
                        c.generic_params.iter().map(|g| g.name.clone()).collect();
                    for par in &c.params {
                        check_type_refs_with_generics(
                            &par.ty,
                            &table,
                            &type_alias_arities,
                            &mut errors,
                            &generics,
                        );
                    }
                    if let Some(ref rt) = c.return_type {
                        check_type_refs_with_generics(
                            rt,
                            &table,
                            &type_alias_arities,
                            &mut errors,
                            &generics,
                        );
                    }
                    if !doc_mode {
                        let fq = format!("{}.{}", p.name, c.name);
                        check_effect_grants_for(&fq, c.span.line, &c.effects, &table, &mut errors);
                    }
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
                    check_generic_param_bounds(&c.generic_params, &table, &mut errors);
                    let generics: Vec<String> =
                        c.generic_params.iter().map(|g| g.name.clone()).collect();
                    for p in &c.params {
                        check_type_refs_with_generics(
                            &p.ty,
                            &table,
                            &type_alias_arities,
                            &mut errors,
                            &generics,
                        );
                    }
                    if let Some(ref rt) = c.return_type {
                        check_type_refs_with_generics(
                            rt,
                            &table,
                            &type_alias_arities,
                            &mut errors,
                            &generics,
                        );
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
                    check_generic_param_bounds(&c.generic_params, &table, &mut errors);
                    let generics: Vec<String> =
                        c.generic_params.iter().map(|g| g.name.clone()).collect();
                    for p in &c.params {
                        check_type_refs_with_generics(
                            &p.ty,
                            &table,
                            &type_alias_arities,
                            &mut errors,
                            &generics,
                        );
                    }
                    if let Some(ref rt) = c.return_type {
                        check_type_refs_with_generics(
                            rt,
                            &table,
                            &type_alias_arities,
                            &mut errors,
                            &generics,
                        );
                    }
                    if !doc_mode && !c.body.is_empty() {
                        let fq = format!("{}.{}", h.name, c.name);
                        check_effect_grants_for(&fq, c.span.line, &c.effects, &table, &mut errors);
                    }
                }
            }
            Item::Trait(t) => {
                for parent in &t.parent_traits {
                    if !table.traits.contains_key(parent) {
                        errors.push(ResolveError::UndefinedTrait {
                            name: parent.clone(),
                            line: t.span.line,
                        });
                    }
                }
                for method in &t.methods {
                    check_generic_param_bounds(&method.generic_params, &table, &mut errors);
                    let generics: Vec<String> = method
                        .generic_params
                        .iter()
                        .map(|g| g.name.clone())
                        .collect();
                    for p in &method.params {
                        check_type_refs_with_generics(
                            &p.ty,
                            &table,
                            &type_alias_arities,
                            &mut errors,
                            &generics,
                        );
                    }
                    if let Some(ref rt) = method.return_type {
                        check_type_refs_with_generics(
                            rt,
                            &table,
                            &type_alias_arities,
                            &mut errors,
                            &generics,
                        );
                    }
                }
            }
            Item::Impl(i) => {
                check_generic_param_bounds(&i.generic_params, &table, &mut errors);
                let impl_generics: Vec<String> =
                    i.generic_params.iter().map(|g| g.name.clone()).collect();
                check_impl_target_type_refs(
                    i,
                    &table,
                    &type_alias_arities,
                    &mut errors,
                    &impl_generics,
                );
                for method in &i.cells {
                    check_generic_param_bounds(&method.generic_params, &table, &mut errors);
                    let mut generics = impl_generics.clone();
                    generics.extend(method.generic_params.iter().map(|g| g.name.clone()));
                    for p in &method.params {
                        check_type_refs_with_generics(
                            &p.ty,
                            &table,
                            &type_alias_arities,
                            &mut errors,
                            &generics,
                        );
                    }
                    if let Some(ref rt) = method.return_type {
                        check_type_refs_with_generics(
                            rt,
                            &table,
                            &type_alias_arities,
                            &mut errors,
                            &generics,
                        );
                    }
                }

                let Some(_) = table.traits.get(&i.trait_name) else {
                    errors.push(ResolveError::UndefinedTrait {
                        name: i.trait_name.clone(),
                        line: i.span.line,
                    });
                    continue;
                };

                let required = collect_required_trait_methods(&i.trait_name, &table);
                let implemented: HashSet<&str> = i.cells.iter().map(|m| m.name.as_str()).collect();
                let missing: Vec<String> = required
                    .into_iter()
                    .filter(|name| !implemented.contains(name.as_str()))
                    .collect();
                if !missing.is_empty() {
                    errors.push(ResolveError::TraitMissingMethods {
                        trait_name: i.trait_name.clone(),
                        target_type: i.target_type.clone(),
                        missing,
                        line: i.span.line,
                    });
                }

                let mut implemented_methods: HashMap<&str, &CellDef> = HashMap::new();
                for method in &i.cells {
                    implemented_methods
                        .entry(method.name.as_str())
                        .or_insert(method);
                }

                for required_method in
                    collect_required_trait_method_defs(&i.trait_name, &trait_defs)
                {
                    let Some(actual_method) =
                        implemented_methods.get(required_method.name.as_str())
                    else {
                        continue;
                    };
                    if let Some(reason) =
                        trait_method_signature_mismatch_reason(required_method, actual_method)
                    {
                        errors.push(ResolveError::TraitMethodSignatureMismatch {
                            trait_name: i.trait_name.clone(),
                            target_type: i.target_type.clone(),
                            method: required_method.name.clone(),
                            reason,
                            expected: format_method_signature(required_method),
                            actual: format_method_signature(actual_method),
                            line: actual_method.span.line,
                        });
                    }
                }
            }
            Item::Grant(g) => {
                table.tools.entry(g.tool_alias.clone()).or_insert(ToolInfo {
                    tool_path: g.tool_alias.to_lowercase(),
                    mcp_url: None,
                });
            }
            Item::TypeAlias(ta) => {
                check_generic_param_bounds(&ta.generic_params, &table, &mut errors);
                let generics: Vec<String> =
                    ta.generic_params.iter().map(|g| g.name.clone()).collect();
                check_type_refs_with_generics(
                    &ta.type_expr,
                    &table,
                    &type_alias_arities,
                    &mut errors,
                    &generics,
                );
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

fn check_generic_param_bounds(
    _params: &[GenericParam],
    _table: &SymbolTable,
    _errors: &mut Vec<ResolveError>,
) {
    // TODO: Implement generic param bound validation
    // For now, this is a stub to allow compilation
}

fn check_impl_target_type_refs(
    _impl_decl: &ImplDef,
    _table: &SymbolTable,
    _type_alias_arities: &HashMap<String, usize>,
    _errors: &mut Vec<ResolveError>,
    _generics: &[String],
) {
    // TODO: Implement impl target type reference validation
    // For now, this is a stub to allow compilation
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
    let policies = table
        .cell_policies
        .get(cell_name)
        .cloned()
        .unwrap_or_default();
    let effect_bind_map = build_effect_bind_map(table);

    for effect in effects {
        let effect = normalize_effect(effect);
        if matches!(
            effect.as_str(),
            "pure" | "trace" | "state" | "approve" | "emit" | "cache" | "async" | "random" | "time"
        ) {
            continue;
        }

        let satisfied =
            is_effect_satisfied_by_policies(&effect, table, &policies, &effect_bind_map);

        if !satisfied {
            errors.push(ResolveError::MissingEffectGrant {
                cell: cell_name.to_string(),
                effect,
                line,
            });
        }
    }
}

fn build_effect_bind_map(table: &SymbolTable) -> HashMap<String, BTreeSet<String>> {
    let mut map: HashMap<String, BTreeSet<String>> = HashMap::new();
    for bind in &table.effect_binds {
        let root = bind
            .effect_path
            .split('.')
            .next()
            .unwrap_or(bind.effect_path.as_str())
            .to_ascii_lowercase();
        map.entry(root).or_default().insert(bind.tool_alias.clone());
    }
    map
}

fn parse_policy_effects_from_expr(expr: &Expr, out: &mut BTreeSet<String>) {
    match expr {
        Expr::StringLit(s, _) => {
            for part in s.split(',') {
                let normalized = normalize_effect(part);
                if !normalized.is_empty() {
                    out.insert(normalized);
                }
            }
        }
        Expr::Ident(name, _) => {
            let normalized = normalize_effect(name);
            if !normalized.is_empty() {
                out.insert(normalized);
            }
        }
        Expr::ListLit(items, _) | Expr::SetLit(items, _) | Expr::TupleLit(items, _) => {
            for item in items {
                parse_policy_effects_from_expr(item, out);
            }
        }
        _ => {}
    }
}

fn grant_to_policy(grant: &GrantDecl) -> GrantPolicy {
    let mut declared_effects = BTreeSet::new();
    let mut has_effect_clause = false;

    for constraint in &grant.constraints {
        let key = constraint.key.to_ascii_lowercase();
        if key == "effect" || key == "effects" {
            has_effect_clause = true;
            parse_policy_effects_from_expr(&constraint.value, &mut declared_effects);
        }
    }

    GrantPolicy {
        tool_alias: grant.tool_alias.clone(),
        allowed_effects: if has_effect_clause {
            Some(declared_effects)
        } else {
            None
        },
    }
}

fn build_cell_policies(program: &Program) -> HashMap<String, Vec<GrantPolicy>> {
    let mut map: HashMap<String, Vec<GrantPolicy>> = HashMap::new();
    let mut global_policies: Vec<GrantPolicy> = Vec::new();

    for item in &program.items {
        if let Item::Grant(g) = item {
            global_policies.push(grant_to_policy(g));
        }
    }

    for item in &program.items {
        match item {
            Item::Cell(c) => {
                map.insert(c.name.clone(), global_policies.clone());
            }
            Item::Agent(a) => {
                let mut scoped = global_policies.clone();
                scoped.extend(a.grants.iter().map(grant_to_policy));
                for c in &a.cells {
                    map.insert(format!("{}.{}", a.name, c.name), scoped.clone());
                }
            }
            Item::Process(p) => {
                let mut scoped = global_policies.clone();
                scoped.extend(p.grants.iter().map(grant_to_policy));
                for c in &p.cells {
                    map.insert(format!("{}.{}", p.name, c.name), scoped.clone());
                }
            }
            Item::Effect(e) => {
                for op in &e.operations {
                    map.insert(format!("{}.{}", e.name, op.name), global_policies.clone());
                }
            }
            Item::Handler(h) => {
                for handle in &h.handles {
                    map.insert(
                        format!("{}.{}", h.name, handle.name),
                        global_policies.clone(),
                    );
                }
            }
            _ => {}
        }
    }

    map
}

fn is_effect_satisfied_by_policies(
    effect: &str,
    table: &SymbolTable,
    policies: &[GrantPolicy],
    effect_bind_map: &HashMap<String, BTreeSet<String>>,
) -> bool {
    if policies.is_empty() {
        return false;
    }

    for policy in policies {
        let alias = &policy.tool_alias;
        if !table.tools.contains_key(alias) {
            continue;
        }

        let bound_to_alias = effect_bind_map
            .get(effect)
            .map(|aliases| aliases.contains(alias))
            .unwrap_or(false);

        if let Some(allowed) = &policy.allowed_effects {
            if allowed.contains(effect) || bound_to_alias {
                return true;
            }
            continue;
        }

        if bound_to_alias {
            return true;
        }

        // Unrestricted policies allow external effects by default.
        return true;
    }

    false
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

fn normalized_non_pure_effects(effects: &[String]) -> BTreeSet<String> {
    effects
        .iter()
        .map(|e| normalize_effect(e))
        .filter(|e| !e.is_empty() && e != "pure")
        .collect()
}

fn parse_directive_bool(program: &Program, name: &str) -> Option<bool> {
    if let Some(directive) = program
        .directives
        .iter()
        .find(|d| d.name.eq_ignore_ascii_case(name))
    {
        let raw = directive
            .value
            .as_deref()
            .unwrap_or("true")
            .trim()
            .to_ascii_lowercase();
        return match raw.as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        };
    }

    // Support attribute-style toggles in source snippets, e.g. `@deterministic`.
    let has_attr = program.items.iter().any(|item| {
        matches!(
            item,
            Item::Addon(AddonDecl {
                kind,
                name: Some(attr_name),
                ..
            }) if kind == "attribute" && attr_name.eq_ignore_ascii_case(name)
        )
    });
    if has_attr {
        Some(true)
    } else {
        None
    }
}

fn validate_machine_graph(process: &ProcessDecl, errors: &mut Vec<ResolveError>) {
    if process.machine_states.is_empty() {
        return;
    }

    let state_names: HashSet<String> = process
        .machine_states
        .iter()
        .map(|s| s.name.clone())
        .collect();
    let initial = process
        .machine_initial
        .clone()
        .or_else(|| process.machine_states.first().map(|s| s.name.clone()))
        .unwrap_or_default();

    if !state_names.contains(&initial) {
        errors.push(ResolveError::MachineUnknownInitial {
            machine: process.name.clone(),
            state: initial.clone(),
            line: process.span.line,
        });
        return;
    }

    for state in &process.machine_states {
        if let Some(guard) = &state.guard {
            if !is_supported_machine_expr(guard) {
                errors.push(ResolveError::MachineUnsupportedExpr {
                    machine: process.name.clone(),
                    state: state.name.clone(),
                    context: "guard".to_string(),
                    line: guard.span().line,
                });
            } else {
                let scope: HashMap<String, TypeExpr> = state
                    .params
                    .iter()
                    .map(|p| (p.name.clone(), p.ty.clone()))
                    .collect();
                let guard_ty = infer_machine_expr_type(guard, &scope);
                if !matches!(guard_ty.as_deref(), Some("Bool") | Some("Any")) {
                    errors.push(ResolveError::MachineGuardType {
                        machine: process.name.clone(),
                        state: state.name.clone(),
                        actual: guard_ty.unwrap_or_else(|| "Unknown".to_string()),
                        line: guard.span().line,
                    });
                }
            }
        }
        if let Some(target) = &state.transition_to {
            if !state_names.contains(target) {
                errors.push(ResolveError::MachineUnknownTransition {
                    machine: process.name.clone(),
                    state: state.name.clone(),
                    target: target.clone(),
                    line: state.span.line,
                });
            } else if let Some(target_state) =
                process.machine_states.iter().find(|s| s.name == *target)
            {
                if state.transition_args.len() != target_state.params.len() {
                    errors.push(ResolveError::MachineTransitionArgCount {
                        machine: process.name.clone(),
                        state: state.name.clone(),
                        target: target.clone(),
                        expected: target_state.params.len(),
                        actual: state.transition_args.len(),
                        line: state.span.line,
                    });
                } else {
                    let source_scope: HashMap<String, TypeExpr> = state
                        .params
                        .iter()
                        .map(|p| (p.name.clone(), p.ty.clone()))
                        .collect();
                    for (idx, arg) in state.transition_args.iter().enumerate() {
                        if !is_supported_machine_expr(arg) {
                            errors.push(ResolveError::MachineUnsupportedExpr {
                                machine: process.name.clone(),
                                state: state.name.clone(),
                                context: format!("transition arg {}", idx + 1),
                                line: arg.span().line,
                            });
                            continue;
                        }
                        let actual = infer_machine_expr_type(arg, &source_scope)
                            .unwrap_or_else(|| "Unknown".to_string());
                        let expected_ty = &target_state.params[idx].ty;
                        if !machine_type_compatible(expected_ty, &actual) {
                            errors.push(ResolveError::MachineTransitionArgType {
                                machine: process.name.clone(),
                                state: state.name.clone(),
                                target: target.clone(),
                                expected: machine_type_key(expected_ty),
                                actual,
                                line: arg.span().line,
                            });
                        }
                    }
                }
            }
        }
    }

    let mut reachable = HashSet::new();
    let mut cursor = Some(initial.clone());
    while let Some(state_name) = cursor {
        if !reachable.insert(state_name.clone()) {
            break;
        }
        cursor = process
            .machine_states
            .iter()
            .find(|s| s.name == state_name)
            .and_then(|s| s.transition_to.clone());
    }

    for state in &process.machine_states {
        if !reachable.contains(&state.name) {
            errors.push(ResolveError::MachineUnreachableState {
                machine: process.name.clone(),
                state: state.name.clone(),
                initial: initial.clone(),
                line: state.span.line,
            });
        }
    }

    if !process.machine_states.iter().any(|s| s.terminal) {
        errors.push(ResolveError::MachineMissingTerminal {
            machine: process.name.clone(),
            line: process.span.line,
        });
    }
}

fn validate_pipeline_stages(
    process: &ProcessDecl,
    table: &SymbolTable,
    errors: &mut Vec<ResolveError>,
) {
    if process.pipeline_stages.is_empty() {
        return;
    }

    let mut previous_output: Option<TypeExpr> = None;
    let mut previous_stage: Option<String> = None;
    for stage in &process.pipeline_stages {
        let Some(cell) = table.cells.get(stage) else {
            errors.push(ResolveError::PipelineUnknownStage {
                pipeline: process.name.clone(),
                stage: stage.clone(),
                line: process.span.line,
            });
            previous_output = None;
            previous_stage = Some(stage.clone());
            continue;
        };

        let non_self_params: Vec<&(String, TypeExpr)> = cell
            .params
            .iter()
            .filter(|(name, _)| name != "self")
            .collect();
        if non_self_params.len() != 1 {
            errors.push(ResolveError::PipelineStageArity {
                pipeline: process.name.clone(),
                stage: stage.clone(),
                line: process.span.line,
            });
        } else if let Some(prev_out) = previous_output.as_ref() {
            let expected = &non_self_params[0].1;
            if !pipeline_type_compatible(expected, prev_out) {
                errors.push(ResolveError::PipelineStageTypeMismatch {
                    pipeline: process.name.clone(),
                    from_stage: previous_stage
                        .clone()
                        .unwrap_or_else(|| "<entry>".to_string()),
                    to_stage: stage.clone(),
                    expected: machine_type_key(expected),
                    actual: machine_type_key(prev_out),
                    line: process.span.line,
                });
            }
        }

        previous_output = Some(
            cell.return_type
                .clone()
                .unwrap_or(TypeExpr::Named("Any".to_string(), process.span)),
        );
        previous_stage = Some(stage.clone());
    }
}

fn pipeline_type_compatible(expected: &TypeExpr, actual: &TypeExpr) -> bool {
    match expected {
        TypeExpr::Named(name, _) if name == "Any" => true,
        TypeExpr::Union(types, _) => types
            .iter()
            .any(|candidate| pipeline_type_compatible(candidate, actual)),
        _ => {
            let actual_key = machine_type_key(actual);
            if actual_key == "Any" {
                true
            } else {
                machine_type_key(expected) == actual_key
            }
        }
    }
}

fn machine_type_key(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Named(name, _) => name.clone(),
        TypeExpr::List(inner, _) => format!("list[{}]", machine_type_key(inner)),
        TypeExpr::Map(k, v, _) => format!("map[{},{}]", machine_type_key(k), machine_type_key(v)),
        TypeExpr::Result(ok, err, _) => {
            format!("result[{},{}]", machine_type_key(ok), machine_type_key(err))
        }
        TypeExpr::Union(types, _) => types
            .iter()
            .map(machine_type_key)
            .collect::<Vec<_>>()
            .join("|"),
        TypeExpr::Null(_) => "Null".to_string(),
        TypeExpr::Tuple(types, _) => {
            let inner = types
                .iter()
                .map(machine_type_key)
                .collect::<Vec<_>>()
                .join(",");
            format!("({})", inner)
        }
        TypeExpr::Set(inner, _) => format!("set[{}]", machine_type_key(inner)),
        TypeExpr::Fn(_, _, _, _) => "fn".to_string(),
        TypeExpr::Generic(name, _, _) => name.clone(),
    }
}

fn machine_type_compatible(expected: &TypeExpr, actual_key: &str) -> bool {
    if actual_key == "Any" {
        return true;
    }
    match expected {
        TypeExpr::Named(name, _) if name == "Any" => true,
        TypeExpr::Union(types, _) => types
            .iter()
            .any(|candidate| machine_type_compatible(candidate, actual_key)),
        _ => machine_type_key(expected) == actual_key,
    }
}

fn is_supported_machine_expr(expr: &Expr) -> bool {
    match expr {
        Expr::IntLit(_, _)
        | Expr::FloatLit(_, _)
        | Expr::StringLit(_, _)
        | Expr::BoolLit(_, _)
        | Expr::NullLit(_) => true,
        Expr::Ident(_, _) => true,
        Expr::UnaryOp(_, inner, _) => is_supported_machine_expr(inner),
        Expr::BinOp(lhs, _, rhs, _) => {
            is_supported_machine_expr(lhs) && is_supported_machine_expr(rhs)
        }
        _ => false,
    }
}

fn infer_machine_expr_type(expr: &Expr, scope: &HashMap<String, TypeExpr>) -> Option<String> {
    match expr {
        Expr::IntLit(_, _) => Some("Int".to_string()),
        Expr::FloatLit(_, _) => Some("Float".to_string()),
        Expr::StringLit(_, _) => Some("String".to_string()),
        Expr::BoolLit(_, _) => Some("Bool".to_string()),
        Expr::NullLit(_) => Some("Null".to_string()),
        Expr::Ident(name, _) => scope
            .get(name)
            .map(machine_type_key)
            .or_else(|| Some("Any".to_string())),
        Expr::UnaryOp(UnaryOp::Not, inner, _) => {
            let inner_ty = infer_machine_expr_type(inner, scope).unwrap_or_else(|| "Any".into());
            if inner_ty == "Bool" || inner_ty == "Any" {
                Some("Bool".to_string())
            } else {
                Some("Any".to_string())
            }
        }
        Expr::UnaryOp(UnaryOp::Neg, inner, _) => {
            let inner_ty = infer_machine_expr_type(inner, scope).unwrap_or_else(|| "Any".into());
            if inner_ty == "Int" || inner_ty == "Float" {
                Some(inner_ty)
            } else {
                Some("Any".to_string())
            }
        }
        Expr::UnaryOp(UnaryOp::BitNot, _inner, _) => Some("Int".to_string()),
        Expr::BinOp(lhs, op, rhs, _) => {
            let lt = infer_machine_expr_type(lhs, scope).unwrap_or_else(|| "Any".into());
            let rt = infer_machine_expr_type(rhs, scope).unwrap_or_else(|| "Any".into());
            match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::Pow => {
                    if lt == "Float" || rt == "Float" {
                        Some("Float".to_string())
                    } else if lt == "Int" && rt == "Int" {
                        Some("Int".to_string())
                    } else {
                        Some("Any".to_string())
                    }
                }
                BinOp::Eq
                | BinOp::NotEq
                | BinOp::Lt
                | BinOp::LtEq
                | BinOp::Gt
                | BinOp::GtEq
                | BinOp::And
                | BinOp::Or
                | BinOp::In => Some("Bool".to_string()),
                BinOp::PipeForward
                | BinOp::Concat
                | BinOp::BitAnd
                | BinOp::BitOr
                | BinOp::BitXor => Some("Any".to_string()),
            }
        }
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
    // Check explicit effect bindings (bind effect X to Y)
    if let Some(bind) = table.effect_binds.iter().find(|b| b.tool_alias == alias) {
        let root = bind
            .effect_path
            .split('.')
            .next()
            .unwrap_or(bind.effect_path.as_str());
        return Some(normalize_effect(root));
    }

    // Check grant-declared effects for this tool
    for policy in table.cell_policies.values().flatten() {
        if policy.tool_alias == alias {
            if let Some(ref allowed) = policy.allowed_effects {
                if let Some(first) = allowed.iter().next() {
                    return Some(first.clone());
                }
            }
        }
    }

    // No explicit effect declaration found -- caller decides the fallback
    None
}

fn infer_pattern_effects(
    pat: &Pattern,
    table: &SymbolTable,
    current: &HashMap<String, BTreeSet<String>>,
    out: &mut BTreeSet<String>,
) {
    match pat {
        Pattern::Variant(_, Some(inner), _) => {
            infer_pattern_effects(inner, table, current, out);
        }
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

#[derive(Debug, Clone)]
struct CallRequirement {
    callee: String,
    effects: BTreeSet<String>,
    line: usize,
}

#[derive(Debug, Clone)]
struct EffectEvidence {
    effect: String,
    line: usize,
    cause: String,
}

fn push_effect_evidence(out: &mut Vec<EffectEvidence>, effect: &str, line: usize, cause: String) {
    let effect = normalize_effect(effect);
    if effect.is_empty() || effect == "pure" {
        return;
    }
    out.push(EffectEvidence {
        effect,
        line,
        cause,
    });
}

fn resolve_call_target_effects(
    callee: &Expr,
    table: &SymbolTable,
) -> Option<(String, BTreeSet<String>)> {
    match callee {
        Expr::Ident(name, _) => {
            if let Some(info) = table.cells.get(name) {
                return Some((name.clone(), normalized_non_pure_effects(&info.effects)));
            }
            if table.tools.contains_key(name) {
                let mut effects = BTreeSet::new();
                effects.insert(
                    effect_from_tool(name, table).unwrap_or_else(|| "external".to_string()),
                );
                return Some((format!("tool {}", name), effects));
            }
            None
        }
        Expr::DotAccess(obj, field, _) => {
            if let Expr::Ident(owner, _) = obj.as_ref() {
                let fq = format!("{}.{}", owner, field);
                table
                    .cells
                    .get(&fq)
                    .map(|info| (fq, normalized_non_pure_effects(&info.effects)))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn resolve_tool_call_effect(callee: &Expr, table: &SymbolTable) -> (String, String) {
    match callee {
        Expr::Ident(alias, _) => {
            let effect = effect_from_tool(alias, table).unwrap_or_else(|| "external".into());
            (format!("tool {}", alias), effect)
        }
        _ => ("tool <dynamic>".into(), "external".into()),
    }
}

fn desugar_pipe_application(
    input: &Expr,
    stage: &Expr,
    span: crate::compiler::tokens::Span,
) -> Expr {
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

fn collect_pattern_call_requirements(
    pat: &Pattern,
    table: &SymbolTable,
    out: &mut Vec<CallRequirement>,
) {
    match pat {
        Pattern::Variant(_, Some(inner), _) => {
            collect_pattern_call_requirements(inner, table, out);
        }
        Pattern::Guard {
            inner, condition, ..
        } => {
            collect_pattern_call_requirements(inner, table, out);
            collect_expr_call_requirements(condition, table, out);
        }
        Pattern::Or { patterns, .. } => {
            for p in patterns {
                collect_pattern_call_requirements(p, table, out);
            }
        }
        Pattern::ListDestructure { elements, .. } | Pattern::TupleDestructure { elements, .. } => {
            for p in elements {
                collect_pattern_call_requirements(p, table, out);
            }
        }
        Pattern::RecordDestructure { fields, .. } => {
            for (_, p) in fields {
                if let Some(p) = p {
                    collect_pattern_call_requirements(p, table, out);
                }
            }
        }
        _ => {}
    }
}

fn collect_stmt_call_requirements(
    stmt: &Stmt,
    table: &SymbolTable,
    out: &mut Vec<CallRequirement>,
) {
    match stmt {
        Stmt::Let(s) => collect_expr_call_requirements(&s.value, table, out),
        Stmt::If(s) => {
            collect_expr_call_requirements(&s.condition, table, out);
            for st in &s.then_body {
                collect_stmt_call_requirements(st, table, out);
            }
            if let Some(else_body) = &s.else_body {
                for st in else_body {
                    collect_stmt_call_requirements(st, table, out);
                }
            }
        }
        Stmt::For(s) => {
            collect_expr_call_requirements(&s.iter, table, out);
            for st in &s.body {
                collect_stmt_call_requirements(st, table, out);
            }
        }
        Stmt::Match(s) => {
            collect_expr_call_requirements(&s.subject, table, out);
            for arm in &s.arms {
                collect_pattern_call_requirements(&arm.pattern, table, out);
                for st in &arm.body {
                    collect_stmt_call_requirements(st, table, out);
                }
            }
        }
        Stmt::Return(s) => collect_expr_call_requirements(&s.value, table, out),
        Stmt::Halt(s) => collect_expr_call_requirements(&s.message, table, out),
        Stmt::Assign(s) => collect_expr_call_requirements(&s.value, table, out),
        Stmt::Expr(s) => collect_expr_call_requirements(&s.expr, table, out),
        Stmt::While(s) => {
            collect_expr_call_requirements(&s.condition, table, out);
            for st in &s.body {
                collect_stmt_call_requirements(st, table, out);
            }
        }
        Stmt::Loop(s) => {
            for st in &s.body {
                collect_stmt_call_requirements(st, table, out);
            }
        }
        Stmt::Emit(s) => collect_expr_call_requirements(&s.value, table, out),
        Stmt::CompoundAssign(s) => collect_expr_call_requirements(&s.value, table, out),
        Stmt::Break(_) | Stmt::Continue(_) => {}
    }
}

fn collect_expr_call_requirements(
    expr: &Expr,
    table: &SymbolTable,
    out: &mut Vec<CallRequirement>,
) {
    match expr {
        Expr::BinOp(lhs, _, rhs, _) | Expr::NullCoalesce(lhs, rhs, _) => {
            collect_expr_call_requirements(lhs, table, out);
            collect_expr_call_requirements(rhs, table, out);
        }
        Expr::Pipe { left, right, span } => {
            let call_expr = desugar_pipe_application(left, right, *span);
            collect_expr_call_requirements(&call_expr, table, out);
        }
        Expr::Illuminate {
            input,
            transform,
            span,
        } => {
            let call_expr = desugar_pipe_application(input, transform, *span);
            collect_expr_call_requirements(&call_expr, table, out);
        }
        Expr::UnaryOp(_, inner, _)
        | Expr::ExpectSchema(inner, _, _)
        | Expr::TryExpr(inner, _)
        | Expr::AwaitExpr(inner, _)
        | Expr::NullAssert(inner, _)
        | Expr::SpreadExpr(inner, _) => collect_expr_call_requirements(inner, table, out),
        Expr::Call(callee, args, span) => {
            collect_expr_call_requirements(callee, table, out);
            for a in args {
                match a {
                    CallArg::Positional(e) | CallArg::Named(_, e, _) | CallArg::Role(_, e, _) => {
                        collect_expr_call_requirements(e, table, out)
                    }
                }
            }
            if let Some((target, effects)) = resolve_call_target_effects(callee, table) {
                if !effects.is_empty() {
                    out.push(CallRequirement {
                        callee: target,
                        effects,
                        line: span.line,
                    });
                }
            }
        }
        Expr::ToolCall(callee, args, span) => {
            for a in args {
                match a {
                    CallArg::Positional(e) | CallArg::Named(_, e, _) | CallArg::Role(_, e, _) => {
                        collect_expr_call_requirements(e, table, out)
                    }
                }
            }
            let (callee_name, effect) = resolve_tool_call_effect(callee, table);
            let mut effects = BTreeSet::new();
            effects.insert(normalize_effect(&effect));
            out.push(CallRequirement {
                callee: callee_name,
                effects,
                line: span.line,
            });
        }
        Expr::ListLit(items, _) | Expr::TupleLit(items, _) | Expr::SetLit(items, _) => {
            for e in items {
                collect_expr_call_requirements(e, table, out);
            }
        }
        Expr::MapLit(items, _) => {
            for (k, v) in items {
                collect_expr_call_requirements(k, table, out);
                collect_expr_call_requirements(v, table, out);
            }
        }
        Expr::RecordLit(_, fields, _) => {
            for (_, e) in fields {
                collect_expr_call_requirements(e, table, out);
            }
        }
        Expr::DotAccess(obj, _, _) | Expr::NullSafeAccess(obj, _, _) => {
            collect_expr_call_requirements(obj, table, out);
        }
        Expr::IndexAccess(obj, idx, _) => {
            collect_expr_call_requirements(obj, table, out);
            collect_expr_call_requirements(idx, table, out);
        }
        Expr::RoleBlock(_, inner, _) => collect_expr_call_requirements(inner, table, out),
        Expr::Lambda { body, .. } => match body {
            LambdaBody::Expr(e) => collect_expr_call_requirements(e, table, out),
            LambdaBody::Block(stmts) => {
                for s in stmts {
                    collect_stmt_call_requirements(s, table, out);
                }
            }
        },
        Expr::IfExpr {
            cond,
            then_val,
            else_val,
            ..
        } => {
            collect_expr_call_requirements(cond, table, out);
            collect_expr_call_requirements(then_val, table, out);
            collect_expr_call_requirements(else_val, table, out);
        }
        Expr::Comprehension {
            body,
            iter,
            condition,
            ..
        } => {
            collect_expr_call_requirements(iter, table, out);
            if let Some(c) = condition {
                collect_expr_call_requirements(c, table, out);
            }
            collect_expr_call_requirements(body, table, out);
        }
        Expr::RangeExpr {
            start, end, step, ..
        } => {
            if let Some(s) = start {
                collect_expr_call_requirements(s, table, out);
            }
            if let Some(e) = end {
                collect_expr_call_requirements(e, table, out);
            }
            if let Some(st) = step {
                collect_expr_call_requirements(st, table, out);
            }
        }
        Expr::MatchExpr { subject, arms, .. } => {
            collect_expr_call_requirements(subject, table, out);
            for arm in arms {
                for s in &arm.body {
                    collect_stmt_call_requirements(s, table, out);
                }
            }
        }
        Expr::BlockExpr(stmts, _) => {
            for s in stmts {
                collect_stmt_call_requirements(s, table, out);
            }
        }
        Expr::IntLit(_, _)
        | Expr::FloatLit(_, _)
        | Expr::StringLit(_, _)
        | Expr::StringInterp(_, _)
        | Expr::BoolLit(_, _)
        | Expr::NullLit(_)
        | Expr::Ident(_, _)
        | Expr::RawStringLit(_, _)
        | Expr::BytesLit(_, _) => {}
    }
}

fn collect_pattern_effect_evidence(
    pat: &Pattern,
    table: &SymbolTable,
    current: &HashMap<String, BTreeSet<String>>,
    out: &mut Vec<EffectEvidence>,
) {
    match pat {
        Pattern::Variant(_, Some(inner), _) => {
            collect_pattern_effect_evidence(inner, table, current, out);
        }
        Pattern::Guard {
            inner, condition, ..
        } => {
            collect_pattern_effect_evidence(inner, table, current, out);
            collect_expr_effect_evidence(condition, table, current, out);
        }
        Pattern::Or { patterns, .. } => {
            for p in patterns {
                collect_pattern_effect_evidence(p, table, current, out);
            }
        }
        Pattern::ListDestructure { elements, .. } | Pattern::TupleDestructure { elements, .. } => {
            for p in elements {
                collect_pattern_effect_evidence(p, table, current, out);
            }
        }
        Pattern::RecordDestructure { fields, .. } => {
            for (_, p) in fields {
                if let Some(p) = p {
                    collect_pattern_effect_evidence(p, table, current, out);
                }
            }
        }
        _ => {}
    }
}

fn collect_stmt_effect_evidence(
    stmt: &Stmt,
    table: &SymbolTable,
    current: &HashMap<String, BTreeSet<String>>,
    out: &mut Vec<EffectEvidence>,
) {
    match stmt {
        Stmt::Let(s) => collect_expr_effect_evidence(&s.value, table, current, out),
        Stmt::If(s) => {
            collect_expr_effect_evidence(&s.condition, table, current, out);
            for st in &s.then_body {
                collect_stmt_effect_evidence(st, table, current, out);
            }
            if let Some(else_body) = &s.else_body {
                for st in else_body {
                    collect_stmt_effect_evidence(st, table, current, out);
                }
            }
        }
        Stmt::For(s) => {
            collect_expr_effect_evidence(&s.iter, table, current, out);
            for st in &s.body {
                collect_stmt_effect_evidence(st, table, current, out);
            }
        }
        Stmt::Match(s) => {
            collect_expr_effect_evidence(&s.subject, table, current, out);
            for arm in &s.arms {
                collect_pattern_effect_evidence(&arm.pattern, table, current, out);
                for st in &arm.body {
                    collect_stmt_effect_evidence(st, table, current, out);
                }
            }
        }
        Stmt::Return(s) => collect_expr_effect_evidence(&s.value, table, current, out),
        Stmt::Halt(s) => collect_expr_effect_evidence(&s.message, table, current, out),
        Stmt::Assign(s) => collect_expr_effect_evidence(&s.value, table, current, out),
        Stmt::Expr(s) => collect_expr_effect_evidence(&s.expr, table, current, out),
        Stmt::While(s) => {
            collect_expr_effect_evidence(&s.condition, table, current, out);
            for st in &s.body {
                collect_stmt_effect_evidence(st, table, current, out);
            }
        }
        Stmt::Loop(s) => {
            for st in &s.body {
                collect_stmt_effect_evidence(st, table, current, out);
            }
        }
        Stmt::Emit(s) => {
            collect_expr_effect_evidence(&s.value, table, current, out);
            push_effect_evidence(out, "emit", s.span.line, "emit statement".to_string());
        }
        Stmt::CompoundAssign(s) => collect_expr_effect_evidence(&s.value, table, current, out),
        Stmt::Break(_) | Stmt::Continue(_) => {}
    }
}

fn collect_expr_effect_evidence(
    expr: &Expr,
    table: &SymbolTable,
    current: &HashMap<String, BTreeSet<String>>,
    out: &mut Vec<EffectEvidence>,
) {
    match expr {
        Expr::BinOp(lhs, _, rhs, _) | Expr::NullCoalesce(lhs, rhs, _) => {
            collect_expr_effect_evidence(lhs, table, current, out);
            collect_expr_effect_evidence(rhs, table, current, out);
        }
        Expr::Pipe { left, right, span } => {
            let call_expr = desugar_pipe_application(left, right, *span);
            collect_expr_effect_evidence(&call_expr, table, current, out);
        }
        Expr::Illuminate {
            input,
            transform,
            span,
        } => {
            let call_expr = desugar_pipe_application(input, transform, *span);
            collect_expr_effect_evidence(&call_expr, table, current, out);
        }
        Expr::UnaryOp(_, inner, _)
        | Expr::ExpectSchema(inner, _, _)
        | Expr::TryExpr(inner, _)
        | Expr::NullAssert(inner, _)
        | Expr::SpreadExpr(inner, _) => {
            collect_expr_effect_evidence(inner, table, current, out);
        }
        Expr::AwaitExpr(inner, span) => {
            collect_expr_effect_evidence(inner, table, current, out);
            push_effect_evidence(out, "async", span.line, "await expression".to_string());
        }
        Expr::Call(callee, args, span) => {
            collect_expr_effect_evidence(callee, table, current, out);
            for a in args {
                match a {
                    CallArg::Positional(e) | CallArg::Named(_, e, _) | CallArg::Role(_, e, _) => {
                        collect_expr_effect_evidence(e, table, current, out)
                    }
                }
            }
            match callee.as_ref() {
                Expr::Ident(name, _) => {
                    if let Some(effects) = current.get(name) {
                        for effect in effects {
                            push_effect_evidence(
                                out,
                                effect,
                                span.line,
                                format!("call to '{}'", name),
                            );
                        }
                    }
                    if table.tools.contains_key(name) {
                        let effect =
                            effect_from_tool(name, table).unwrap_or_else(|| "external".into());
                        push_effect_evidence(
                            out,
                            &effect,
                            span.line,
                            format!("tool call '{}'", name),
                        );
                    }
                    if name == "emit" || name == "print" {
                        push_effect_evidence(out, "emit", span.line, format!("call to '{}'", name));
                    }
                    if matches!(
                        name.as_str(),
                        "parallel" | "race" | "vote" | "select" | "timeout" | "spawn"
                    ) {
                        push_effect_evidence(
                            out,
                            "async",
                            span.line,
                            format!("call to '{}'", name),
                        );
                    }
                    if matches!(name.as_str(), "uuid" | "uuid_v4") {
                        push_effect_evidence(
                            out,
                            "random",
                            span.line,
                            format!("call to '{}'", name),
                        );
                    }
                    if matches!(name.as_str(), "timestamp") {
                        push_effect_evidence(out, "time", span.line, format!("call to '{}'", name));
                    }
                }
                Expr::DotAccess(obj, field, _) => {
                    if let Expr::Ident(owner, _) = obj.as_ref() {
                        let fq = format!("{}.{}", owner, field);
                        if let Some(effects) = current.get(&fq) {
                            for effect in effects {
                                push_effect_evidence(
                                    out,
                                    effect,
                                    span.line,
                                    format!("call to '{}'", fq),
                                );
                            }
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
                                        push_effect_evidence(
                                            out,
                                            "state",
                                            span.line,
                                            format!("process call '{}'", fq),
                                        );
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
                                        push_effect_evidence(
                                            out,
                                            "state",
                                            span.line,
                                            format!("process call '{}'", fq),
                                        );
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
        Expr::ToolCall(callee, args, span) => {
            for a in args {
                match a {
                    CallArg::Positional(e) | CallArg::Named(_, e, _) | CallArg::Role(_, e, _) => {
                        collect_expr_effect_evidence(e, table, current, out)
                    }
                }
            }
            match callee.as_ref() {
                Expr::Ident(alias, _) => {
                    let effect =
                        effect_from_tool(alias, table).unwrap_or_else(|| "external".into());
                    push_effect_evidence(out, &effect, span.line, format!("tool call '{}'", alias));
                }
                _ => push_effect_evidence(
                    out,
                    "external",
                    span.line,
                    "dynamic tool call".to_string(),
                ),
            }
        }
        Expr::ListLit(items, _) | Expr::TupleLit(items, _) | Expr::SetLit(items, _) => {
            for e in items {
                collect_expr_effect_evidence(e, table, current, out);
            }
        }
        Expr::MapLit(items, _) => {
            for (k, v) in items {
                collect_expr_effect_evidence(k, table, current, out);
                collect_expr_effect_evidence(v, table, current, out);
            }
        }
        Expr::RecordLit(_, fields, _) => {
            for (_, e) in fields {
                collect_expr_effect_evidence(e, table, current, out);
            }
        }
        Expr::DotAccess(obj, _, _) | Expr::NullSafeAccess(obj, _, _) => {
            collect_expr_effect_evidence(obj, table, current, out);
        }
        Expr::IndexAccess(obj, idx, _) => {
            collect_expr_effect_evidence(obj, table, current, out);
            collect_expr_effect_evidence(idx, table, current, out);
        }
        Expr::RoleBlock(_, inner, _) => collect_expr_effect_evidence(inner, table, current, out),
        Expr::Lambda { body, .. } => match body {
            LambdaBody::Expr(e) => collect_expr_effect_evidence(e, table, current, out),
            LambdaBody::Block(stmts) => {
                for s in stmts {
                    collect_stmt_effect_evidence(s, table, current, out);
                }
            }
        },
        Expr::IfExpr {
            cond,
            then_val,
            else_val,
            ..
        } => {
            collect_expr_effect_evidence(cond, table, current, out);
            collect_expr_effect_evidence(then_val, table, current, out);
            collect_expr_effect_evidence(else_val, table, current, out);
        }
        Expr::Comprehension {
            body,
            iter,
            condition,
            ..
        } => {
            collect_expr_effect_evidence(iter, table, current, out);
            if let Some(c) = condition {
                collect_expr_effect_evidence(c, table, current, out);
            }
            collect_expr_effect_evidence(body, table, current, out);
        }
        Expr::RangeExpr {
            start, end, step, ..
        } => {
            if let Some(s) = start {
                collect_expr_effect_evidence(s, table, current, out);
            }
            if let Some(e) = end {
                collect_expr_effect_evidence(e, table, current, out);
            }
            if let Some(st) = step {
                collect_expr_effect_evidence(st, table, current, out);
            }
        }
        Expr::MatchExpr { subject, arms, .. } => {
            collect_expr_effect_evidence(subject, table, current, out);
            for arm in arms {
                for s in &arm.body {
                    collect_stmt_effect_evidence(s, table, current, out);
                }
            }
        }
        Expr::BlockExpr(stmts, _) => {
            for s in stmts {
                collect_stmt_effect_evidence(s, table, current, out);
            }
        }
        Expr::IntLit(_, _)
        | Expr::FloatLit(_, _)
        | Expr::StringLit(_, _)
        | Expr::StringInterp(_, _)
        | Expr::BoolLit(_, _)
        | Expr::NullLit(_)
        | Expr::Ident(_, _)
        | Expr::RawStringLit(_, _)
        | Expr::BytesLit(_, _) => {}
    }
}

fn collect_cell_effect_evidence(
    cell: &EffectCell,
    table: &SymbolTable,
    current: &HashMap<String, BTreeSet<String>>,
) -> HashMap<String, EffectEvidence> {
    let mut raw = Vec::new();
    for stmt in &cell.body {
        collect_stmt_effect_evidence(stmt, table, current, &mut raw);
    }

    let mut by_effect: HashMap<String, EffectEvidence> = HashMap::new();
    for ev in raw {
        match by_effect.get(&ev.effect) {
            Some(existing) if existing.line <= ev.line => {}
            _ => {
                by_effect.insert(ev.effect.clone(), ev);
            }
        }
    }
    by_effect
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
        Expr::Pipe { left, right, span } => {
            let call_expr = desugar_pipe_application(left, right, *span);
            infer_expr_effects(&call_expr, table, current, out);
        }
        Expr::Illuminate {
            input,
            transform,
            span,
        } => {
            let call_expr = desugar_pipe_application(input, transform, *span);
            infer_expr_effects(&call_expr, table, current, out);
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
                    if table.tools.contains_key(name) {
                        if let Some(effect) = effect_from_tool(name, table) {
                            out.insert(effect);
                        } else {
                            out.insert("external".into());
                        }
                    }
                    if name == "emit" || name == "print" {
                        out.insert("emit".into());
                    }
                    if matches!(
                        name.as_str(),
                        "parallel" | "race" | "vote" | "select" | "timeout" | "spawn"
                    ) {
                        out.insert("async".into());
                    }
                    if matches!(name.as_str(), "uuid" | "uuid_v4") {
                        out.insert("random".into());
                    }
                    if matches!(name.as_str(), "timestamp") {
                        out.insert("time".into());
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
                } else {
                    out.insert("external".into());
                }
            } else {
                out.insert("external".into());
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
        Expr::MatchExpr { subject, arms, .. } => {
            infer_expr_effects(subject, table, current, out);
            for arm in arms {
                for s in &arm.body {
                    infer_stmt_effects(s, table, current, out);
                }
            }
        }
        Expr::BlockExpr(stmts, _) => {
            for s in stmts {
                infer_stmt_effects(s, table, current, out);
            }
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
        let declared: BTreeSet<String> =
            cell.declared.iter().map(|e| normalize_effect(e)).collect();
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
        let evidence = collect_cell_effect_evidence(cell, table, &effective);
        let declared: BTreeSet<String> =
            cell.declared.iter().map(|e| normalize_effect(e)).collect();
        let final_effects = if declared.is_empty() {
            inferred.clone()
        } else {
            if enforce_declared_effect_rows {
                for missing in inferred.difference(&declared) {
                    let (line, cause) = if let Some(ev) = evidence.get(missing) {
                        (ev.line, format!("; cause: {}", ev.cause))
                    } else {
                        (cell.line, String::new())
                    };
                    errors.push(ResolveError::UndeclaredEffect {
                        cell: cell.name.clone(),
                        effect: missing.clone(),
                        line,
                        cause,
                    });
                }
            }
            declared
        };

        if cell.declared.is_empty() && !doc_mode {
            let inferred_vec: Vec<String> = final_effects.iter().cloned().collect();
            check_effect_grants_for(&cell.name, cell.line, &inferred_vec, table, errors);
        }

        if let Some(info) = table.cells.get_mut(&cell.name) {
            info.effects = final_effects.iter().cloned().collect();
        }
    }

    enforce_effect_call_compatibility(program, table, &cells, errors);
    enforce_deterministic_profile(program, table, &cells, errors);
}

fn enforce_effect_call_compatibility(
    program: &Program,
    table: &SymbolTable,
    cells: &[EffectCell],
    errors: &mut Vec<ResolveError>,
) {
    let strict = parse_directive_bool(program, "strict").unwrap_or(true);
    let doc_mode = parse_directive_bool(program, "doc_mode").unwrap_or(false);
    if !strict || doc_mode {
        return;
    }

    for cell in cells {
        let Some(info) = table.cells.get(&cell.name) else {
            continue;
        };
        let caller_effects = normalized_non_pure_effects(&info.effects);

        let mut reqs = Vec::new();
        for stmt in &cell.body {
            collect_stmt_call_requirements(stmt, table, &mut reqs);
        }

        let mut seen = BTreeSet::new();
        for req in reqs {
            for effect in req.effects {
                if caller_effects.contains(&effect) {
                    continue;
                }
                if seen.insert((req.callee.clone(), effect.clone(), req.line)) {
                    errors.push(ResolveError::EffectContractViolation {
                        caller: cell.name.clone(),
                        callee: req.callee.clone(),
                        effect,
                        line: req.line,
                    });
                }
            }
        }
    }
}

fn enforce_deterministic_profile(
    program: &Program,
    table: &SymbolTable,
    cells: &[EffectCell],
    errors: &mut Vec<ResolveError>,
) {
    let deterministic = parse_directive_bool(program, "deterministic").unwrap_or(false);
    let doc_mode = parse_directive_bool(program, "doc_mode").unwrap_or(false);
    if !deterministic || doc_mode {
        return;
    }

    // Effects that represent real I/O and are therefore nondeterministic.
    // "external" is the fallback for any tool without an explicit `bind effect`
    // declaration.  The rest are well-known effect names that users may bind
    // via `bind effect <name> to <tool>`.
    const NONDETERMINISTIC_EFFECTS: &[&str] = &[
        "database", "email", "external", "fs", "http", "llm", "mcp", "random", "time",
    ];

    for cell in cells {
        let Some(info) = table.cells.get(&cell.name) else {
            continue;
        };
        let mut seen = BTreeSet::new();
        for effect in &info.effects {
            let effect = normalize_effect(effect);
            if NONDETERMINISTIC_EFFECTS.contains(&effect.as_str()) && seen.insert(effect.clone()) {
                errors.push(ResolveError::NondeterministicOperation {
                    cell: cell.name.clone(),
                    operation: effect,
                    line: cell.line,
                });
            }
        }
    }
}

/// Compute Levenshtein edit distance between two strings
fn edit_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut matrix = vec![vec![0; b_len + 1]; a_len + 1];

    for (i, row) in matrix.iter_mut().enumerate() {
        row[0] = i;
    }
    #[allow(clippy::needless_range_loop)]
    for j in 0..=b_len {
        matrix[0][j] = j;
    }

    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[a_len][b_len]
}

/// Find similar names for "did you mean?" suggestions
fn suggest_similar(name: &str, candidates: &[&str], max_distance: usize) -> Vec<String> {
    let mut matches: Vec<(usize, String)> = candidates
        .iter()
        .filter_map(|c| {
            let d = edit_distance(name, c);
            if d <= max_distance && d < name.len() {
                Some((d, c.to_string()))
            } else {
                None
            }
        })
        .collect();

    matches.sort_by_key(|(d, _)| *d);
    matches.into_iter().map(|(_, s)| s).take(3).collect()
}

fn collect_type_alias_arities(program: &Program) -> HashMap<String, usize> {
    program
        .items
        .iter()
        .filter_map(|item| {
            if let Item::TypeAlias(alias) = item {
                Some((alias.name.clone(), alias.generic_params.len()))
            } else {
                None
            }
        })
        .collect()
}

fn expected_type_arity(
    name: &str,
    table: &SymbolTable,
    type_alias_arities: &HashMap<String, usize>,
) -> Option<usize> {
    if let Some(info) = table.types.get(name) {
        return Some(info.generic_params.len());
    }

    if let Some(arity) = type_alias_arities.get(name) {
        return Some(*arity);
    }

    if table.type_aliases.contains_key(name) {
        return Some(0);
    }

    None
}

fn collect_required_trait_methods(trait_name: &str, table: &SymbolTable) -> Vec<String> {
    fn walk(name: &str, table: &SymbolTable, visited: &mut HashSet<String>, out: &mut Vec<String>) {
        if !visited.insert(name.to_string()) {
            return;
        }
        let Some(info) = table.traits.get(name) else {
            return;
        };
        for parent in &info.parent_traits {
            walk(parent, table, visited, out);
        }
        for method in &info.methods {
            if !out.contains(method) {
                out.push(method.clone());
            }
        }
    }

    let mut out = Vec::new();
    let mut visited = HashSet::new();
    walk(trait_name, table, &mut visited, &mut out);
    out
}

fn collect_trait_defs(program: &Program) -> HashMap<String, &TraitDef> {
    let mut defs = HashMap::new();
    for item in &program.items {
        if let Item::Trait(t) = item {
            defs.entry(t.name.clone()).or_insert(t);
        }
    }
    defs
}

fn collect_required_trait_method_defs<'a>(
    trait_name: &str,
    trait_defs: &HashMap<String, &'a TraitDef>,
) -> Vec<&'a CellDef> {
    fn walk<'a>(
        name: &str,
        trait_defs: &HashMap<String, &'a TraitDef>,
        visited: &mut HashSet<String>,
        seen_methods: &mut HashSet<String>,
        out: &mut Vec<&'a CellDef>,
    ) {
        if !visited.insert(name.to_string()) {
            return;
        }
        let Some(trait_def) = trait_defs.get(name).copied() else {
            return;
        };
        for parent in &trait_def.parent_traits {
            walk(parent, trait_defs, visited, seen_methods, out);
        }
        for method in &trait_def.methods {
            if seen_methods.insert(method.name.clone()) {
                out.push(method);
            }
        }
    }

    let mut out = Vec::new();
    let mut visited = HashSet::new();
    let mut seen_methods = HashSet::new();
    walk(
        trait_name,
        trait_defs,
        &mut visited,
        &mut seen_methods,
        &mut out,
    );
    out
}

fn trait_method_signature_mismatch_reason(expected: &CellDef, actual: &CellDef) -> Option<String> {
    if expected.generic_params.len() != actual.generic_params.len() {
        return Some(format!(
            "generic parameter count mismatch: expected {}, found {}",
            expected.generic_params.len(),
            actual.generic_params.len()
        ));
    }

    let expected_generics: Vec<&str> = expected
        .generic_params
        .iter()
        .map(|g| g.name.as_str())
        .collect();
    let actual_generics: Vec<&str> = actual
        .generic_params
        .iter()
        .map(|g| g.name.as_str())
        .collect();

    if expected.params.len() != actual.params.len() {
        return Some(format!(
            "parameter count mismatch: expected {}, found {}",
            expected.params.len(),
            actual.params.len()
        ));
    }

    for (idx, (expected_param, actual_param)) in
        expected.params.iter().zip(&actual.params).enumerate()
    {
        if !type_expr_compatible(
            &expected_param.ty,
            &actual_param.ty,
            &expected_generics,
            &actual_generics,
        ) {
            return Some(format!(
                "parameter {} type mismatch: expected '{}', found '{}'",
                idx + 1,
                format_type_expr(&expected_param.ty),
                format_type_expr(&actual_param.ty)
            ));
        }
    }

    if !return_type_compatible(
        expected.return_type.as_ref(),
        actual.return_type.as_ref(),
        &expected_generics,
        &actual_generics,
    ) {
        return Some(format!(
            "return type mismatch: expected '{}', found '{}'",
            format_optional_type_expr(expected.return_type.as_ref()),
            format_optional_type_expr(actual.return_type.as_ref())
        ));
    }

    None
}

fn return_type_compatible(
    expected: Option<&TypeExpr>,
    actual: Option<&TypeExpr>,
    expected_generics: &[&str],
    actual_generics: &[&str],
) -> bool {
    match (expected, actual) {
        (None, None) => true,
        (Some(expected_ty), Some(actual_ty)) => {
            type_expr_compatible(expected_ty, actual_ty, expected_generics, actual_generics)
        }
        _ => false,
    }
}

fn type_expr_compatible(
    expected: &TypeExpr,
    actual: &TypeExpr,
    expected_generics: &[&str],
    actual_generics: &[&str],
) -> bool {
    match (expected, actual) {
        (TypeExpr::Named(expected_name, _), TypeExpr::Named(actual_name, _)) => names_compatible(
            expected_name,
            actual_name,
            expected_generics,
            actual_generics,
        ),
        (TypeExpr::List(expected_inner, _), TypeExpr::List(actual_inner, _))
        | (TypeExpr::Set(expected_inner, _), TypeExpr::Set(actual_inner, _)) => {
            type_expr_compatible(
                expected_inner,
                actual_inner,
                expected_generics,
                actual_generics,
            )
        }
        (TypeExpr::Map(expected_k, expected_v, _), TypeExpr::Map(actual_k, actual_v, _))
        | (TypeExpr::Result(expected_k, expected_v, _), TypeExpr::Result(actual_k, actual_v, _)) => {
            type_expr_compatible(expected_k, actual_k, expected_generics, actual_generics)
                && type_expr_compatible(expected_v, actual_v, expected_generics, actual_generics)
        }
        (TypeExpr::Union(expected_types, _), TypeExpr::Union(actual_types, _))
        | (TypeExpr::Tuple(expected_types, _), TypeExpr::Tuple(actual_types, _)) => {
            expected_types.len() == actual_types.len()
                && expected_types
                    .iter()
                    .zip(actual_types)
                    .all(|(expected_ty, actual_ty)| {
                        type_expr_compatible(
                            expected_ty,
                            actual_ty,
                            expected_generics,
                            actual_generics,
                        )
                    })
        }
        (TypeExpr::Null(_), TypeExpr::Null(_)) => true,
        (
            TypeExpr::Fn(expected_params, expected_ret, expected_effects, _),
            TypeExpr::Fn(actual_params, actual_ret, actual_effects, _),
        ) => {
            if expected_params.len() != actual_params.len() {
                return false;
            }
            let mut expected_effects_sorted = expected_effects.clone();
            expected_effects_sorted.sort();
            let mut actual_effects_sorted = actual_effects.clone();
            actual_effects_sorted.sort();
            expected_effects_sorted == actual_effects_sorted
                && expected_params
                    .iter()
                    .zip(actual_params)
                    .all(|(expected_ty, actual_ty)| {
                        type_expr_compatible(
                            expected_ty,
                            actual_ty,
                            expected_generics,
                            actual_generics,
                        )
                    })
                && type_expr_compatible(
                    expected_ret,
                    actual_ret,
                    expected_generics,
                    actual_generics,
                )
        }
        (
            TypeExpr::Generic(expected_name, expected_args, _),
            TypeExpr::Generic(actual_name, actual_args, _),
        ) => {
            names_compatible(
                expected_name,
                actual_name,
                expected_generics,
                actual_generics,
            ) && expected_args.len() == actual_args.len()
                && expected_args
                    .iter()
                    .zip(actual_args)
                    .all(|(expected_arg, actual_arg)| {
                        type_expr_compatible(
                            expected_arg,
                            actual_arg,
                            expected_generics,
                            actual_generics,
                        )
                    })
        }
        _ => false,
    }
}

fn names_compatible(
    expected: &str,
    actual: &str,
    expected_generics: &[&str],
    actual_generics: &[&str],
) -> bool {
    let expected_generic_idx = expected_generics.iter().position(|name| *name == expected);
    let actual_generic_idx = actual_generics.iter().position(|name| *name == actual);
    match (expected_generic_idx, actual_generic_idx) {
        (Some(expected_idx), Some(actual_idx)) => expected_idx == actual_idx,
        (None, None) => expected == actual,
        _ => false,
    }
}

fn format_method_signature(method: &CellDef) -> String {
    let mut signature = String::new();
    signature.push_str("cell ");
    signature.push_str(&method.name);
    if !method.generic_params.is_empty() {
        let generic_names: Vec<&str> = method
            .generic_params
            .iter()
            .map(|generic_param| generic_param.name.as_str())
            .collect();
        signature.push('[');
        signature.push_str(&generic_names.join(", "));
        signature.push(']');
    }
    signature.push('(');
    let params = method
        .params
        .iter()
        .map(|param| format!("{}: {}", param.name, format_type_expr(&param.ty)))
        .collect::<Vec<_>>();
    signature.push_str(&params.join(", "));
    signature.push(')');
    if let Some(return_type) = &method.return_type {
        signature.push_str(" -> ");
        signature.push_str(&format_type_expr(return_type));
    }
    signature
}

fn format_optional_type_expr(ty: Option<&TypeExpr>) -> String {
    match ty {
        Some(ty) => format_type_expr(ty),
        None => "no return type".to_string(),
    }
}

fn format_type_expr(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Named(name, _) => name.clone(),
        TypeExpr::List(inner, _) => format!("list[{}]", format_type_expr(inner)),
        TypeExpr::Map(key, value, _) => {
            format!(
                "map[{}, {}]",
                format_type_expr(key),
                format_type_expr(value)
            )
        }
        TypeExpr::Result(ok, err, _) => {
            format!(
                "result[{}, {}]",
                format_type_expr(ok),
                format_type_expr(err)
            )
        }
        TypeExpr::Union(types, _) => types
            .iter()
            .map(format_type_expr)
            .collect::<Vec<_>>()
            .join(" | "),
        TypeExpr::Null(_) => "Null".to_string(),
        TypeExpr::Tuple(types, _) => {
            let rendered = types.iter().map(format_type_expr).collect::<Vec<_>>();
            format!("({})", rendered.join(", "))
        }
        TypeExpr::Set(inner, _) => format!("set[{}]", format_type_expr(inner)),
        TypeExpr::Fn(params, ret, effects, _) => {
            let rendered_params = params.iter().map(format_type_expr).collect::<Vec<_>>();
            if effects.is_empty() {
                format!(
                    "fn({}) -> {}",
                    rendered_params.join(", "),
                    format_type_expr(ret)
                )
            } else {
                format!(
                    "fn({}) -> {} / {{{}}}",
                    rendered_params.join(", "),
                    format_type_expr(ret),
                    effects.join(", ")
                )
            }
        }
        TypeExpr::Generic(name, args, _) => {
            let rendered_args = args.iter().map(format_type_expr).collect::<Vec<_>>();
            format!("{}[{}]", name, rendered_args.join(", "))
        }
    }
}

fn check_type_refs_with_generics(
    ty: &TypeExpr,
    table: &SymbolTable,
    type_alias_arities: &HashMap<String, usize>,
    errors: &mut Vec<ResolveError>,
    generics: &[String],
) {
    match ty {
        TypeExpr::Named(name, span) => {
            if generics.iter().any(|g| g == name) {
                return;
            }
            if !table.types.contains_key(name) && !table.type_aliases.contains_key(name) {
                let mut candidates: Vec<&str> = table.types.keys().map(|s| s.as_str()).collect();
                candidates.extend(table.type_aliases.keys().map(|s| s.as_str()));
                let suggestions = suggest_similar(name, &candidates, 2);
                errors.push(ResolveError::UndefinedType {
                    name: name.clone(),
                    line: span.line,
                    suggestions,
                });
            } else if expected_type_arity(name, table, type_alias_arities).is_some_and(|n| n > 0) {
                let expected = expected_type_arity(name, table, type_alias_arities).unwrap_or(0);
                errors.push(ResolveError::GenericArityMismatch {
                    name: name.clone(),
                    expected,
                    actual: 0,
                    line: span.line,
                });
            }
        }
        TypeExpr::List(inner, _) => {
            check_type_refs_with_generics(inner, table, type_alias_arities, errors, generics)
        }
        TypeExpr::Map(k, v, _) => {
            check_type_refs_with_generics(k, table, type_alias_arities, errors, generics);
            check_type_refs_with_generics(v, table, type_alias_arities, errors, generics);
        }
        TypeExpr::Result(ok, err, _) => {
            check_type_refs_with_generics(ok, table, type_alias_arities, errors, generics);
            check_type_refs_with_generics(err, table, type_alias_arities, errors, generics);
        }
        TypeExpr::Union(types, _) => {
            for t in types {
                check_type_refs_with_generics(t, table, type_alias_arities, errors, generics);
            }
        }
        TypeExpr::Null(_) => {}
        TypeExpr::Tuple(types, _) => {
            for t in types {
                check_type_refs_with_generics(t, table, type_alias_arities, errors, generics);
            }
        }
        TypeExpr::Set(inner, _) => {
            check_type_refs_with_generics(inner, table, type_alias_arities, errors, generics)
        }
        TypeExpr::Fn(params, ret, _, _) => {
            for t in params {
                check_type_refs_with_generics(t, table, type_alias_arities, errors, generics);
            }
            check_type_refs_with_generics(ret, table, type_alias_arities, errors, generics);
        }
        TypeExpr::Generic(name, args, span) => {
            if generics.iter().any(|g| g == name) {
                if !args.is_empty() {
                    errors.push(ResolveError::GenericArityMismatch {
                        name: name.clone(),
                        expected: 0,
                        actual: args.len(),
                        line: span.line,
                    });
                }
            } else if !table.types.contains_key(name) && !table.type_aliases.contains_key(name) {
                let mut candidates: Vec<&str> = table.types.keys().map(|s| s.as_str()).collect();
                candidates.extend(table.type_aliases.keys().map(|s| s.as_str()));
                let suggestions = suggest_similar(name, &candidates, 2);
                errors.push(ResolveError::UndefinedType {
                    name: name.clone(),
                    line: span.line,
                    suggestions,
                });
            } else if let Some(expected) = expected_type_arity(name, table, type_alias_arities) {
                if expected != args.len() {
                    errors.push(ResolveError::GenericArityMismatch {
                        name: name.clone(),
                        expected,
                        actual: args.len(),
                        line: span.line,
                    });
                }
            }
            for t in args {
                check_type_refs_with_generics(t, table, type_alias_arities, errors, generics);
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

    #[test]
    fn test_effect_inference_marks_uuid_and_timestamp() {
        let table = resolve_src(
            "cell main() -> String\n  let id = uuid()\n  let ts = timestamp()\n  return to_string(ts) + id\nend",
        )
        .unwrap();
        let effects = &table.cells.get("main").unwrap().effects;
        assert!(effects.contains(&"random".to_string()));
        assert!(effects.contains(&"time".to_string()));
    }

    #[test]
    fn test_effect_inference_marks_async_orchestration_builtins() {
        let table = resolve_src(
            "cell main() -> Int\n  let f = spawn(fn() => 1)\n  let a = parallel(1, 2)\n  let b = race(1, 2)\n  let c = vote(1, 1, 2)\n  let d = select(null, 1)\n  return timeout(d, 10)\nend",
        )
        .unwrap();
        let effects = &table.cells.get("main").unwrap().effects;
        assert!(effects.contains(&"async".to_string()));
    }

    #[test]
    fn test_deterministic_profile_rejects_nondeterminism() {
        let err = resolve_src("@deterministic true\n\ncell main() -> String\n  return uuid()\nend")
            .unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::NondeterministicOperation { cell, operation, .. }
            if cell == "main" && operation == "random"
        )));
    }

    #[test]
    fn test_effect_contract_violation_on_cell_call() {
        let err = resolve_src(
            "use tool http.get as HttpGet\ngrant HttpGet\n\ncell fetch() -> Int / {http}\n  return 1\nend\n\ncell main() -> Int / {emit}\n  return fetch()\nend",
        )
        .unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::EffectContractViolation { caller, callee, effect, .. }
            if caller == "main" && callee == "fetch" && effect == "http"
        )));
    }

    #[test]
    fn test_effect_contract_violation_on_tool_call() {
        let err = resolve_src(
            "use tool http.get as HttpGet\nbind effect http to HttpGet\n\ngrant HttpGet\n\ncell main() -> String / {emit}\n  return string(HttpGet(url: \"https://example.com\"))\nend",
        )
        .unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::EffectContractViolation { caller, callee, effect, .. }
            if caller == "main" && callee == "tool HttpGet" && effect == "http"
        )));
    }

    #[test]
    fn test_effect_contract_allows_declared_callee_effects() {
        let table = resolve_src(
            "use tool http.get as HttpGet\ngrant HttpGet\n\ncell fetch() -> Int / {http}\n  return 1\nend\n\ncell main() -> Int / {http}\n  return fetch()\nend",
        )
        .unwrap();
        let effects = &table.cells.get("main").unwrap().effects;
        assert!(effects.contains(&"http".to_string()));
    }

    #[test]
    fn test_undeclared_effect_includes_call_cause() {
        let err = resolve_src(
            "use tool http.get as HttpGet\ngrant HttpGet\n\ncell fetch() -> Int / {http}\n  return 1\nend\n\ncell main() -> Int / {emit}\n  return fetch()\nend",
        )
        .unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::UndeclaredEffect { cell, effect, cause, .. }
            if cell == "main" && effect == "http" && cause.contains("call to 'fetch'")
        )));
    }

    #[test]
    fn test_undeclared_effect_includes_tool_cause() {
        let err = resolve_src(
            "use tool http.get as HttpGet\nbind effect http to HttpGet\ngrant HttpGet\n\ncell main() -> String / {emit}\n  return string(HttpGet(url: \"https://example.com\"))\nend",
        )
        .unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::UndeclaredEffect { cell, effect, cause, .. }
            if cell == "main" && effect == "http" && cause.contains("tool call 'HttpGet'")
        )));
    }

    #[test]
    fn test_grant_policy_effect_clause_restricts_effects() {
        let err = resolve_src(
            "use tool http.get as HttpGet\ngrant HttpGet\n  effect http\n\ncell main() -> Int / {llm}\n  return 1\nend",
        )
        .unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::MissingEffectGrant { cell, effect, .. }
            if cell == "main" && effect == "llm"
        )));
    }

    #[test]
    fn test_grant_policy_effects_list_allows_effect() {
        let table = resolve_src(
            "use tool http.get as HttpGet\ngrant HttpGet\n  effects [\"http\", \"llm\"]\n\ncell main() -> Int / {llm}\n  return 1\nend",
        )
        .unwrap();
        let effects = &table.cells.get("main").unwrap().effects;
        assert!(effects.contains(&"llm".to_string()));
    }

    #[test]
    fn test_machine_graph_validation_accepts_reachable_terminal_graph() {
        let table = resolve_src(
            "machine TicketFlow\n  initial: Start\n  state Start\n    transition Done()\n  end\n  state Done\n    terminal: true\n  end\nend",
        )
        .unwrap();
        let process = table
            .processes
            .get("machine:TicketFlow")
            .expect("machine should be registered");
        assert_eq!(process.machine_initial.as_deref(), Some("Start"));
        assert_eq!(process.machine_states.len(), 2);
    }

    #[test]
    fn test_machine_graph_validation_reports_transition_and_reachability_errors() {
        let err = resolve_src(
            "machine Broken\n  initial: Start\n  state Start\n    transition Missing()\n  end\n  state DeadEnd\n    terminal: false\n  end\nend",
        )
        .unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::MachineUnknownTransition { machine, state, target, .. }
            if machine == "Broken" && state == "Start" && target == "Missing"
        )));
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::MachineUnreachableState { machine, state, .. }
            if machine == "Broken" && state == "DeadEnd"
        )));
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::MachineMissingTerminal { machine, .. }
            if machine == "Broken"
        )));
    }

    #[test]
    fn test_machine_graph_validation_checks_transition_arg_count_and_type() {
        let err = resolve_src(
            "machine Typed\n  initial: Start\n  state Start(x: Int)\n    transition Done(x, \"bad\")\n  end\n  state Done(v: Int)\n    terminal: true\n  end\nend",
        )
        .unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::MachineTransitionArgCount { machine, state, target, expected, actual, .. }
            if machine == "Typed" && state == "Start" && target == "Done" && *expected == 1 && *actual == 2
        )));

        let err = resolve_src(
            "machine Typed\n  initial: Start\n  state Start(x: String)\n    transition Done(x)\n  end\n  state Done(v: Int)\n    terminal: true\n  end\nend",
        )
        .unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::MachineTransitionArgType { machine, state, target, expected, actual, .. }
            if machine == "Typed" && state == "Start" && target == "Done" && expected == "Int" && actual == "String"
        )));
    }

    #[test]
    fn test_machine_graph_validation_checks_guard_type() {
        let err = resolve_src(
            "machine Guarded\n  initial: Start\n  state Start(x: Int)\n    guard: x + 1\n    transition Done(x)\n  end\n  state Done(v: Int)\n    terminal: true\n  end\nend",
        )
        .unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::MachineGuardType { machine, state, actual, .. }
            if machine == "Guarded" && state == "Start" && actual == "Int"
        )));
    }

    #[test]
    fn test_pipeline_stage_validation_rejects_unknown_stage() {
        let err = resolve_src("pipeline P\n  stages:\n    UnknownStage\n  end\nend").unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::PipelineUnknownStage { pipeline, stage, .. }
            if pipeline == "P" && stage == "UnknownStage"
        )));
    }

    #[test]
    fn test_pipeline_stage_validation_rejects_type_mismatch() {
        let err = resolve_src(
            "cell one(x: Int) -> String\n  return \"x\"\nend\n\ncell two(y: Int) -> Int\n  return y\nend\n\npipeline P\n  stages:\n    one\n      -> two\n  end\nend",
        )
        .unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::PipelineStageTypeMismatch { pipeline, from_stage, to_stage, expected, actual, .. }
            if pipeline == "P" && from_stage == "one" && to_stage == "two" && expected == "Int" && actual == "String"
        )));
    }

    #[test]
    fn test_duplicate_record_detection() {
        let err =
            resolve_src("record Foo\n  x: Int\nend\n\nrecord Foo\n  y: String\nend").unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::Duplicate { name, .. } if name == "Foo"
        )));
    }

    #[test]
    fn test_duplicate_cell_detection() {
        let err =
            resolve_src("cell foo() -> Int\n  return 1\nend\n\ncell foo() -> Int\n  return 2\nend")
                .unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::Duplicate { name, .. } if name == "foo"
        )));
    }

    #[test]
    fn test_type_alias_not_undefined() {
        // A type alias should not produce an UndefinedType error
        let table = resolve_src(
            "type UserId = String\n\ncell greet(id: UserId) -> String\n  return id\nend",
        )
        .unwrap();
        assert!(table.type_aliases.contains_key("UserId"));
    }

    #[test]
    fn test_duplicate_enum_detection() {
        let err =
            resolve_src("enum Color\n  Red\n  Blue\nend\n\nenum Color\n  Green\nend").unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::Duplicate { name, .. } if name == "Color"
        )));
    }

    #[test]
    fn test_duplicate_effect_detection() {
        let err = resolve_src("effect http\n  cell get(url: String) -> String\nend\n\neffect http\n  cell post(url: String) -> String\nend").unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::Duplicate { name, .. } if name == "http"
        )));
    }

    #[test]
    fn test_builtin_types_are_minimal() {
        let table = SymbolTable::new();
        // Core builtins should be present
        assert!(table.types.contains_key("String"));
        assert!(table.types.contains_key("Int"));
        assert!(table.types.contains_key("Float"));
        assert!(table.types.contains_key("Bool"));
        assert!(table.types.contains_key("Bytes"));
        assert!(table.types.contains_key("Json"));
        assert!(table.types.contains_key("Null"));
        // Generic placeholders should not be implicitly accepted.
        assert!(!table.types.contains_key("A"));
        assert!(!table.types.contains_key("T"));
        // App-specific types should NOT be present
        assert!(!table.types.contains_key("Invoice"));
        assert!(!table.types.contains_key("MyRecord"));
        assert!(!table.types.contains_key("Report"));
        assert!(!table.types.contains_key("Response"));
    }

    #[test]
    fn test_tool_without_binding_gets_external_effect() {
        // A tool with no explicit `bind effect` should produce "external" effect,
        // not a heuristic guess based on tool name or path.
        let err = resolve_src(
            "use tool http.get as HttpGet\ngrant HttpGet\n\ncell main() -> String / {http}\n  return string(HttpGet(url: \"https://example.com\"))\nend",
        )
        .unwrap_err();
        // The tool call should produce "external" (not "http"), so declaring {http}
        // should cause an UndeclaredEffect for "external".
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::UndeclaredEffect { cell, effect, .. }
            if cell == "main" && effect == "external"
        )));
    }

    #[test]
    fn test_explicit_bind_effect_maps_tool_to_effect() {
        // With an explicit `bind effect http to HttpGet`, the tool should
        // produce "http" effect.
        let table = resolve_src(
            "use tool http.get as HttpGet\nbind effect http to HttpGet\ngrant HttpGet\n\ncell main() -> String / {http}\n  return string(HttpGet(url: \"https://example.com\"))\nend",
        )
        .unwrap();
        let effects = &table.cells.get("main").unwrap().effects;
        assert!(effects.contains(&"http".to_string()));
    }

    #[test]
    fn test_generic_type_alias_resolves_without_placeholder_builtins() {
        let table = resolve_src(
            "type Box[T] = map[String, T]\n\ncell main() -> Box[Int]\n  return {\"ok\": 1}\nend",
        )
        .unwrap();
        assert!(table.type_aliases.contains_key("Box"));
    }

    #[test]
    fn test_trait_impl_signature_reports_parameter_count_mismatch() {
        let err = resolve_src(
            "trait Greeter\n  cell greet(name: String) -> String\n    return name\n  end\nend\n\nimpl Greeter for String\n  cell greet(name: String, suffix: String) -> String\n    return name\n  end\nend",
        )
        .unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::TraitMethodSignatureMismatch { method, reason, .. }
            if method == "greet" && reason.contains("parameter count mismatch")
        )));
    }

    #[test]
    fn test_trait_impl_signature_reports_parameter_type_mismatch() {
        let err = resolve_src(
            "trait Greeter\n  cell greet(name: String) -> String\n    return name\n  end\nend\n\nimpl Greeter for String\n  cell greet(name: Int) -> String\n    return \"x\"\n  end\nend",
        )
        .unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::TraitMethodSignatureMismatch { method, reason, .. }
            if method == "greet" && reason.contains("parameter 1 type mismatch")
        )));
    }

    #[test]
    fn test_trait_impl_signature_reports_return_type_mismatch() {
        let err = resolve_src(
            "trait Greeter\n  cell greet(name: String) -> String\n    return name\n  end\nend\n\nimpl Greeter for String\n  cell greet(name: String) -> Int\n    return 1\n  end\nend",
        )
        .unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            ResolveError::TraitMethodSignatureMismatch { method, reason, .. }
            if method == "greet" && reason.contains("return type mismatch")
        )));
    }

    #[test]
    fn test_trait_impl_signature_accepts_compatible_method() {
        let table = resolve_src(
            "trait Greeter\n  cell greet(name: String) -> String\n    return name\n  end\nend\n\nimpl Greeter for String\n  cell greet(name: String) -> String\n    return name\n  end\nend",
        )
        .unwrap();
        assert_eq!(table.impls.len(), 1);
    }
}
