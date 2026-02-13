//! Bidirectional type inference and checking for Lumen.

use crate::compiler::ast::*;
use crate::compiler::resolve::SymbolTable;

use std::collections::HashMap;
use thiserror::Error;

/// Check if a name is a built-in function
fn is_builtin_function(name: &str) -> bool {
    matches!(
        name,
        "print"
            | "len"
            | "length"
            | "append"
            | "range"
            | "to_string"
            | "str"
            | "to_int"
            | "int"
            | "to_float"
            | "float"
            | "type_of"
            | "keys"
            | "values"
            | "contains"
            | "join"
            | "split"
            | "trim"
            | "upper"
            | "lower"
            | "replace"
            | "abs"
            | "min"
            | "max"
            | "hash"
            | "not"
            | "count"
            | "matches"
            | "slice"
            | "sort"
            | "reverse"
            | "map"
            | "filter"
            | "reduce"
            | "parallel"
            | "race"
            | "vote"
            | "select"
            | "timeout"
            | "spawn"
            | "resume"
    )
}

/// In doc_mode / non-strict mode, allow undefined variable names that look like
/// plausible identifiers (e.g. doc snippet references). Returns false for names
/// that are likely typos or real errors even in non-strict mode.
fn is_doc_placeholder_var(name: &str) -> bool {
    // All well-formed identifiers are allowed in placeholder mode.
    // The guard (`allow_placeholders`) is only true in doc_mode or non-strict mode.
    !name.is_empty() && !name.starts_with("__")
}

fn type_contains_any(ty: &Type) -> bool {
    match ty {
        Type::Any => true,
        Type::List(inner) | Type::Set(inner) => type_contains_any(inner),
        Type::Map(k, v) | Type::Result(k, v) => type_contains_any(k) || type_contains_any(v),
        Type::Fn(params, ret) => params.iter().any(type_contains_any) || type_contains_any(ret),
        Type::Union(types) | Type::Tuple(types) => types.iter().any(type_contains_any),
        Type::TypeRef(_, args) => args.iter().any(type_contains_any),
        _ => false,
    }
}

#[derive(Debug, Error)]
pub enum TypeError {
    #[error("type mismatch at line {line}: expected {expected}, got {actual}")]
    Mismatch {
        expected: String,
        actual: String,
        line: usize,
    },
    #[error("undefined variable '{name}' at line {line}")]
    UndefinedVar { name: String, line: usize },
    #[error("not callable at line {line}")]
    NotCallable { line: usize },
    #[error("wrong number of arguments at line {line}: expected {expected}, got {actual}")]
    ArgCount {
        expected: usize,
        actual: usize,
        line: usize,
    },
    #[error("unknown field '{field}' on type '{ty}' at line {line}")]
    UnknownField {
        field: String,
        ty: String,
        line: usize,
    },
    #[error("undefined type '{name}' at line {line}")]
    UndefinedType { name: String, line: usize },
    #[error("missing return in cell '{name}' at line {line}")]
    MissingReturn { name: String, line: usize },
    #[error("cannot assign to immutable variable '{name}' at line {line}")]
    ImmutableAssign { name: String, line: usize },
}

/// Resolved type representation
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    String,
    Int,
    Float,
    Bool,
    Bytes,
    Json,
    Null,
    List(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Record(String),
    Enum(String),
    Result(Box<Type>, Box<Type>),
    Union(Vec<Type>),
    Tuple(Vec<Type>),
    Set(Box<Type>),
    Fn(Vec<Type>, Box<Type>),
    Generic(String),
    TypeRef(String, Vec<Type>),
    Any, // For unresolved / error recovery
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::String => write!(f, "String"),
            Type::Int => write!(f, "Int"),
            Type::Float => write!(f, "Float"),
            Type::Bool => write!(f, "Bool"),
            Type::Bytes => write!(f, "Bytes"),
            Type::Json => write!(f, "Json"),
            Type::Null => write!(f, "Null"),
            Type::Any => write!(f, "Any"),
            Type::List(t) => write!(f, "list[{}]", t),
            Type::Map(k, v) => write!(f, "map[{}, {}]", k, v),
            Type::Record(n) => write!(f, "{}", n),
            Type::Enum(n) => write!(f, "{}", n),
            Type::Result(o, e) => write!(f, "result[{}, {}]", o, e),
            Type::Union(ts) => {
                let parts: Vec<_> = ts.iter().map(|t| format!("{}", t)).collect();
                write!(f, "{}", parts.join(" | "))
            }
            Type::Tuple(ts) => {
                let parts: Vec<_> = ts.iter().map(|t| format!("{}", t)).collect();
                write!(f, "({})", parts.join(", "))
            }
            Type::Set(t) => write!(f, "set[{}]", t),
            Type::Fn(params, ret) => {
                let ps: Vec<_> = params.iter().map(|t| format!("{}", t)).collect();
                write!(f, "fn({}) -> {}", ps.join(", "), ret)
            }
            Type::Generic(n) => write!(f, "{}", n),
            Type::TypeRef(n, args) => {
                let as_: Vec<_> = args.iter().map(|t| format!("{}", t)).collect();
                write!(f, "{}[{}]", n, as_.join(", "))
            }
        }
    }
}

pub fn resolve_type_expr(ty: &TypeExpr, symbols: &SymbolTable) -> Type {
    match ty {
        TypeExpr::Named(name, _) => match name.as_str() {
            "String" => Type::String,
            "Int" => Type::Int,
            "Float" => Type::Float,
            "Bool" => Type::Bool,
            "Bytes" => Type::Bytes,
            "Json" => Type::Json,
            "ValidationError" => Type::Record("ValidationError".into()),
            _ => {
                if symbols.types.contains_key(name) {
                    use crate::compiler::resolve::TypeInfoKind;
                    match &symbols.types[name].kind {
                        TypeInfoKind::Record(_) => Type::Record(name.clone()),
                        TypeInfoKind::Enum(_) => Type::Enum(name.clone()),
                        TypeInfoKind::Builtin => Type::Record(name.clone()),
                    }
                } else if let Some(alias_target) = symbols.type_aliases.get(name) {
                    resolve_type_expr(alias_target, symbols)
                } else {
                    Type::Any
                }
            }
        },
        TypeExpr::List(inner, _) => Type::List(Box::new(resolve_type_expr(inner, symbols))),
        TypeExpr::Map(k, v, _) => Type::Map(
            Box::new(resolve_type_expr(k, symbols)),
            Box::new(resolve_type_expr(v, symbols)),
        ),
        TypeExpr::Result(ok, err, _) => Type::Result(
            Box::new(resolve_type_expr(ok, symbols)),
            Box::new(resolve_type_expr(err, symbols)),
        ),
        TypeExpr::Union(types, _) => Type::Union(
            types
                .iter()
                .map(|t| resolve_type_expr(t, symbols))
                .collect(),
        ),
        TypeExpr::Null(_) => Type::Null,
        TypeExpr::Tuple(types, _) => Type::Tuple(
            types
                .iter()
                .map(|t| resolve_type_expr(t, symbols))
                .collect(),
        ),
        TypeExpr::Set(inner, _) => Type::Set(Box::new(resolve_type_expr(inner, symbols))),
        TypeExpr::Fn(params, ret, _, _) => {
            let param_types = params
                .iter()
                .map(|t| resolve_type_expr(t, symbols))
                .collect();
            let ret_type = resolve_type_expr(ret, symbols);
            Type::Fn(param_types, Box::new(ret_type))
        }
        TypeExpr::Generic(name, args, _) => {
            let arg_types: Vec<_> = args.iter().map(|t| resolve_type_expr(t, symbols)).collect();
            Type::TypeRef(name.clone(), arg_types)
        }
    }
}

struct TypeChecker<'a> {
    symbols: &'a SymbolTable,
    allow_placeholders: bool,
    locals: HashMap<String, Type>,
    mutables: HashMap<String, bool>,
    errors: Vec<TypeError>,
}

impl<'a> TypeChecker<'a> {
    fn new(symbols: &'a SymbolTable, allow_placeholders: bool) -> Self {
        Self {
            symbols,
            allow_placeholders,
            locals: HashMap::new(),
            mutables: HashMap::new(),
            errors: Vec::new(),
        }
    }

    fn check_cell(&mut self, cell: &CellDef) {
        self.locals.clear();
        self.mutables.clear();
        for p in &cell.params {
            let ty = resolve_type_expr(&p.ty, self.symbols);
            self.locals.insert(p.name.clone(), ty);
            self.mutables.insert(p.name.clone(), true); // params are mutable by default
        }
        let return_type = if let Some(ref rt) = cell.return_type {
            Some(resolve_type_expr(rt, self.symbols))
        } else {
            None
        };

        for stmt in &cell.body {
            self.check_stmt(stmt, return_type.as_ref());
        }
    }

    fn check_agent_cell(&mut self, cell: &CellDef) {
        self.locals.clear();
        self.mutables.clear();
        self.locals.insert("self".into(), Type::Any);
        self.mutables.insert("self".into(), true);
        for p in &cell.params {
            if p.name == "self" {
                continue;
            }
            let ty = resolve_type_expr(&p.ty, self.symbols);
            self.locals.insert(p.name.clone(), ty);
            self.mutables.insert(p.name.clone(), true);
        }
        let return_type = if let Some(ref rt) = cell.return_type {
            Some(resolve_type_expr(rt, self.symbols))
        } else {
            None
        };
        for stmt in &cell.body {
            self.check_stmt(stmt, return_type.as_ref());
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt, expected_return: Option<&Type>) {
        match stmt {
            Stmt::Let(ls) => {
                let val_type = self.infer_expr(&ls.value);
                if let Some(ref ann) = ls.ty {
                    let expected = resolve_type_expr(ann, self.symbols);
                    self.check_compat(&expected, &val_type, ls.span.line);
                }
                self.locals.insert(ls.name.clone(), val_type);
                // In Lumen, all let bindings are reassignable by default
                // `let mut` is just documentation; `const` is immutable
                self.mutables.insert(ls.name.clone(), true);
            }
            Stmt::If(ifs) => {
                let ct = self.infer_expr(&ifs.condition);
                self.check_compat(&Type::Bool, &ct, ifs.span.line);
                for s in &ifs.then_body {
                    self.check_stmt(s, expected_return);
                }
                if let Some(ref eb) = ifs.else_body {
                    for s in eb {
                        self.check_stmt(s, expected_return);
                    }
                }
            }
            Stmt::For(fs) => {
                let iter_type = self.infer_expr(&fs.iter);
                let elem_type = match &iter_type {
                    Type::List(inner) => *inner.clone(),
                    Type::Set(inner) => *inner.clone(),
                    Type::Map(k, _) => *k.clone(),
                    Type::Any => Type::Any,
                    _ => {
                        self.errors.push(TypeError::Mismatch {
                            expected: "iterable".into(),
                            actual: format!("{}", iter_type),
                            line: fs.span.line,
                        });
                        Type::Any
                    }
                };
                self.locals.insert(fs.var.clone(), elem_type);
                for s in &fs.body {
                    self.check_stmt(s, expected_return);
                }
            }
            Stmt::Match(ms) => {
                let subject_type = self.infer_expr(&ms.subject);
                let mut covered_variants = Vec::new();
                let mut has_catchall = false;

                for arm in &ms.arms {
                    self.bind_match_pattern(
                        &arm.pattern,
                        &subject_type,
                        &mut covered_variants,
                        &mut has_catchall,
                        arm.span.line,
                    );
                    for s in &arm.body {
                        self.check_stmt(s, expected_return);
                    }
                }

                // Exhaustiveness Check for Enums
                if let Type::Enum(ref name) = subject_type {
                    if !has_catchall {
                        if let Some(ti) = self.symbols.types.get(name) {
                            if let crate::compiler::resolve::TypeInfoKind::Enum(def) = &ti.kind {
                                let missing: Vec<_> = def
                                    .variants
                                    .iter()
                                    .filter(|v| !covered_variants.contains(&v.name))
                                    .map(|v| v.name.clone())
                                    .collect();
                                if !missing.is_empty() {
                                    self.errors.push(TypeError::Mismatch {
                                        expected: format!("variants {:?}", missing),
                                        actual: "incomplete match".into(),
                                        line: ms.span.line,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            Stmt::Return(rs) => {
                let val_type = self.infer_expr(&rs.value);
                if let Some(expected) = expected_return {
                    self.check_compat(expected, &val_type, rs.span.line);
                }
            }
            Stmt::Halt(hs) => {
                self.infer_expr(&hs.message);
            }
            Stmt::Assign(asgn) => {
                let val_type = self.infer_expr(&asgn.value);
                // Check mutability
                if let Some(&is_mut) = self.mutables.get(&asgn.target) {
                    if !is_mut {
                        self.errors.push(TypeError::ImmutableAssign {
                            name: asgn.target.clone(),
                            line: asgn.span.line,
                        });
                    }
                }
                self.locals.insert(asgn.target.clone(), val_type);
            }
            Stmt::Expr(es) => {
                self.infer_expr(&es.expr);
            }
            Stmt::While(ws) => {
                let ct = self.infer_expr(&ws.condition);
                self.check_compat(&Type::Bool, &ct, ws.span.line);
                for s in &ws.body {
                    self.check_stmt(s, expected_return);
                }
            }
            Stmt::Loop(ls) => {
                for s in &ls.body {
                    self.check_stmt(s, expected_return);
                }
            }
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Emit(es) => {
                self.infer_expr(&es.value);
            }
            Stmt::CompoundAssign(ca) => {
                let val_type = self.infer_expr(&ca.value);
                // Check mutability
                if let Some(&is_mut) = self.mutables.get(&ca.target) {
                    if !is_mut {
                        self.errors.push(TypeError::ImmutableAssign {
                            name: ca.target.clone(),
                            line: ca.span.line,
                        });
                    }
                }
                if let Some(existing) = self.locals.get(&ca.target).cloned() {
                    self.check_compat(&existing, &val_type, ca.span.line);
                }
            }
        }
    }

    fn bind_match_pattern(
        &mut self,
        pattern: &Pattern,
        subject_type: &Type,
        covered_variants: &mut Vec<String>,
        has_catchall: &mut bool,
        line: usize,
    ) {
        match pattern {
            Pattern::Variant(tag, binding, _) => {
                let mut valid_variant = false;
                let mut bind_type = Type::Any;

                if let Type::Enum(ref name) = subject_type {
                    if let Some(ti) = self.symbols.types.get(name) {
                        if let crate::compiler::resolve::TypeInfoKind::Enum(def) = &ti.kind {
                            if def.variants.iter().any(|v| v.name == *tag) {
                                valid_variant = true;
                                covered_variants.push(tag.clone());
                                bind_type = subject_type.clone();
                            }
                        }
                    }
                    if !valid_variant {
                        self.errors.push(TypeError::Mismatch {
                            expected: format!("variant of {}", name),
                            actual: tag.clone(),
                            line,
                        });
                    }
                } else if let Type::Result(ref ok, ref err) = subject_type {
                    if tag == "ok" {
                        valid_variant = true;
                        bind_type = *ok.clone();
                    } else if tag == "err" {
                        valid_variant = true;
                        bind_type = *err.clone();
                    }
                    if !valid_variant {
                        self.errors.push(TypeError::Mismatch {
                            expected: "ok or err".into(),
                            actual: tag.clone(),
                            line,
                        });
                    }
                }

                if let Some(b) = binding {
                    self.locals.insert(b.clone(), bind_type);
                }
            }
            Pattern::Ident(name, _) => {
                self.locals.insert(name.clone(), subject_type.clone());
                *has_catchall = true;
            }
            Pattern::Wildcard(_) => {
                *has_catchall = true;
            }
            Pattern::Guard {
                inner, condition, ..
            } => {
                self.bind_match_pattern(inner, subject_type, covered_variants, has_catchall, line);
                let guard_ty = self.infer_expr(condition);
                self.check_compat(&Type::Bool, &guard_ty, line);
            }
            Pattern::Or { patterns, .. } => {
                for p in patterns {
                    self.bind_match_pattern(p, subject_type, covered_variants, has_catchall, line);
                }
            }
            Pattern::ListDestructure { elements, rest, .. } => {
                let elem_type = match subject_type {
                    Type::List(inner) => *inner.clone(),
                    Type::Any => Type::Any,
                    other => {
                        self.errors.push(TypeError::Mismatch {
                            expected: "List".into(),
                            actual: format!("{}", other),
                            line,
                        });
                        Type::Any
                    }
                };
                for p in elements {
                    self.bind_match_pattern(p, &elem_type, covered_variants, has_catchall, line);
                }
                if let Some(rest_name) = rest {
                    self.locals
                        .insert(rest_name.clone(), Type::List(Box::new(elem_type)));
                }
            }
            Pattern::TupleDestructure { elements, .. } => {
                match subject_type {
                    Type::Tuple(types) => {
                        for (idx, p) in elements.iter().enumerate() {
                            let ty = types.get(idx).cloned().unwrap_or(Type::Any);
                            self.bind_match_pattern(
                                p,
                                &ty,
                                covered_variants,
                                has_catchall,
                                line,
                            );
                        }
                    }
                    Type::Any => {
                        for p in elements {
                            self.bind_match_pattern(
                                p,
                                &Type::Any,
                                covered_variants,
                                has_catchall,
                                line,
                            );
                        }
                    }
                    other => {
                        self.errors.push(TypeError::Mismatch {
                            expected: "Tuple".into(),
                            actual: format!("{}", other),
                            line,
                        });
                    }
                }
            }
            Pattern::RecordDestructure {
                type_name,
                fields,
                open: _,
                ..
            } => {
                if let Type::Record(actual_name) = subject_type {
                    if actual_name != type_name {
                        self.errors.push(TypeError::Mismatch {
                            expected: type_name.clone(),
                            actual: actual_name.clone(),
                            line,
                        });
                    }
                }
                for (field_name, field_pat) in fields {
                    let field_ty = if let Some(ti) = self.symbols.types.get(type_name) {
                        if let crate::compiler::resolve::TypeInfoKind::Record(def) = &ti.kind {
                            if let Some(field) = def.fields.iter().find(|f| f.name == *field_name) {
                                resolve_type_expr(&field.ty, self.symbols)
                            } else {
                                Type::Any
                            }
                        } else {
                            Type::Any
                        }
                    } else {
                        Type::Any
                    };
                    if let Some(p) = field_pat {
                        self.bind_match_pattern(
                            p,
                            &field_ty,
                            covered_variants,
                            has_catchall,
                            line,
                        );
                    } else {
                        self.locals.insert(field_name.clone(), field_ty);
                    }
                }
            }
            Pattern::TypeCheck {
                name, type_expr, ..
            } => {
                let expected = resolve_type_expr(type_expr, self.symbols);
                self.check_compat(&expected, subject_type, line);
                self.locals.insert(name.clone(), expected);
            }
            Pattern::Literal(_) => {}
        }
    }

    fn infer_expr(&mut self, expr: &Expr) -> Type {
        match expr {
            Expr::IntLit(_, _) => Type::Int,
            Expr::FloatLit(_, _) => Type::Float,
            Expr::StringLit(_, _) => Type::String,
            Expr::StringInterp(_, _) => Type::String,
            Expr::BoolLit(_, _) => Type::Bool,
            Expr::NullLit(_) => Type::Null,
            Expr::Ident(name, span) => {
                if let Some(ty) = self.locals.get(name) {
                    ty.clone()
                } else if let Some(const_info) = self.symbols.consts.get(name) {
                    if let Some(ref ty) = const_info.ty {
                        resolve_type_expr(ty, self.symbols)
                    } else if let Some(ref val) = const_info.value {
                        match val {
                            Expr::IntLit(_, _) => Type::Int,
                            Expr::FloatLit(_, _) => Type::Float,
                            Expr::StringLit(_, _) | Expr::StringInterp(_, _) => Type::String,
                            Expr::BoolLit(_, _) => Type::Bool,
                            Expr::NullLit(_) => Type::Null,
                            Expr::ListLit(_, _) => Type::List(Box::new(Type::Any)),
                            Expr::MapLit(_, _) => Type::Map(Box::new(Type::String), Box::new(Type::Any)),
                            _ => Type::Any,
                        }
                    } else {
                        Type::Any
                    }
                } else if self.symbols.cells.contains_key(name) {
                    Type::Any
                }
                // cell ref
                else if self.symbols.tools.contains_key(name) {
                    Type::Any
                }
                // tool ref
                else if self.symbols.agents.contains_key(name) {
                    Type::Any
                }
                // agent constructor ref
                else if self
                    .symbols
                    .addons
                    .iter()
                    .any(|a| a.name.as_deref() == Some(name.as_str()))
                {
                    Type::Any
                }
                // addendum decl refs (handlers, guardrails, etc.)
                else if self.symbols.types.contains_key(name)
                    || self.symbols.type_aliases.contains_key(name)
                {
                    Type::Any
                }
                // type/value references in spec snippets
                else if is_builtin_function(name) {
                    Type::Any
                }
                // built-in
                else if name == "null" {
                    Type::Null
                } else if self.allow_placeholders && is_doc_placeholder_var(name) {
                    Type::Any
                } else {
                    // Check for Enum Variant
                    let mut found_enum = None;
                    for (type_name, type_info) in &self.symbols.types {
                        if let crate::compiler::resolve::TypeInfoKind::Enum(def) = &type_info.kind {
                            if def.variants.iter().any(|v| v.name == *name) {
                                found_enum = Some(Type::Enum(type_name.clone()));
                                break;
                            }
                        }
                    }
                    if let Some(ty) = found_enum {
                        ty
                    } else {
                        self.errors.push(TypeError::UndefinedVar {
                            name: name.clone(),
                            line: span.line,
                        });
                        Type::Any
                    }
                }
            }
            Expr::ListLit(elems, _) => {
                if elems.is_empty() {
                    Type::List(Box::new(Type::Any))
                } else {
                    let first = self.infer_expr(&elems[0]);
                    for e in &elems[1..] {
                        self.infer_expr(e);
                    }
                    Type::List(Box::new(first))
                }
            }
            Expr::MapLit(pairs, _) => {
                if pairs.is_empty() {
                    Type::Map(Box::new(Type::String), Box::new(Type::Any))
                } else {
                    let kt = self.infer_expr(&pairs[0].0);
                    let vt = self.infer_expr(&pairs[0].1);
                    for (k, v) in &pairs[1..] {
                        self.infer_expr(k);
                        self.infer_expr(v);
                    }
                    Type::Map(Box::new(kt), Box::new(vt))
                }
            }
            Expr::RecordLit(name, fields, span) => {
                if let Some(ti) = self.symbols.types.get(name) {
                    if let crate::compiler::resolve::TypeInfoKind::Record(def) = &ti.kind {
                        // 1. Check provided fields (unknown & type mismatch)
                        for (fname, fval) in fields {
                            let val_type = self.infer_expr(fval);
                            if let Some(field_def) = def.fields.iter().find(|f| f.name == *fname) {
                                let expected = resolve_type_expr(&field_def.ty, self.symbols);
                                self.check_compat(&expected, &val_type, span.line);
                            } else {
                                self.errors.push(TypeError::UnknownField {
                                    field: fname.clone(),
                                    ty: name.clone(),
                                    line: span.line,
                                });
                            }
                        }
                        // 2. Check for missing fields (fields with defaults are optional)
                        for field_def in &def.fields {
                            if field_def.default_value.is_none()
                                && !fields.iter().any(|(fname, _)| fname == &field_def.name)
                            {
                                self.errors.push(TypeError::Mismatch {
                                    expected: format!("field '{}'", field_def.name),
                                    actual: "missing".into(),
                                    line: span.line,
                                });
                            }
                        }
                    }
                } else {
                    self.errors.push(TypeError::UndefinedType {
                        name: name.clone(),
                        line: span.line,
                    });
                }
                Type::Record(name.clone())
            }
            Expr::BinOp(lhs, op, rhs, _span) => {
                let lt = self.infer_expr(lhs);
                let rt = self.infer_expr(rhs);
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                        if lt == Type::Any || rt == Type::Any {
                            Type::Any
                        } else if (lt == Type::String || rt == Type::String) && *op == BinOp::Add {
                            Type::String
                        } else if lt == Type::Float || rt == Type::Float {
                            Type::Float
                        } else {
                            Type::Int
                        }
                    }
                    BinOp::Eq
                    | BinOp::NotEq
                    | BinOp::Lt
                    | BinOp::LtEq
                    | BinOp::Gt
                    | BinOp::GtEq => Type::Bool,
                    BinOp::And | BinOp::Or => Type::Bool,
                    BinOp::Pow => {
                        if lt == Type::Float || rt == Type::Float {
                            Type::Float
                        } else {
                            Type::Int
                        }
                    }
                    BinOp::PipeForward => rt,
                    BinOp::Concat => lt,
                    BinOp::In => Type::Bool,
                    BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => Type::Int,
                }
            }
            Expr::UnaryOp(op, inner, _) => {
                let t = self.infer_expr(inner);
                match op {
                    UnaryOp::Neg => t,
                    UnaryOp::Not => Type::Bool,
                    UnaryOp::BitNot => Type::Int,
                }
            }
            Expr::Call(callee, args, _span) => {
                for arg in args {
                    match arg {
                        CallArg::Positional(e) => {
                            self.infer_expr(e);
                        }
                        CallArg::Named(_, e, _) => {
                            self.infer_expr(e);
                        }
                        CallArg::Role(_, _, _) => {}
                    }
                }
                // Try to resolve the return type
                if let Expr::Ident(name, _) = callee.as_ref() {
                    if let Some(ci) = self.symbols.cells.get(name) {
                        if let Some(ref rt) = ci.return_type {
                            return resolve_type_expr(rt, self.symbols);
                        }
                    }
                }
                Type::Any
            }
            Expr::ToolCall(_, args, _) => {
                for arg in args {
                    match arg {
                        CallArg::Positional(e) | CallArg::Named(_, e, _) => {
                            self.infer_expr(e);
                        }
                        _ => {}
                    }
                }
                Type::Any
            }
            Expr::DotAccess(obj, field, _span) => {
                let ot = self.infer_expr(obj);
                if let Type::Record(ref name) = ot {
                    if let Some(ti) = self.symbols.types.get(name) {
                        if let crate::compiler::resolve::TypeInfoKind::Record(ref rd) = ti.kind {
                            if let Some(f) = rd.fields.iter().find(|f| f.name == *field) {
                                return resolve_type_expr(&f.ty, self.symbols);
                            }
                        }
                    }
                }
                Type::Any
            }
            Expr::IndexAccess(obj, idx, _) => {
                let ot = self.infer_expr(obj);
                self.infer_expr(idx);
                match ot {
                    Type::List(inner) => *inner,
                    Type::Map(_, v) => *v,
                    _ => Type::Any,
                }
            }
            Expr::RoleBlock(_, content, _) => {
                self.infer_expr(content);
                Type::String
            }
            Expr::ExpectSchema(inner, schema_name, _) => {
                self.infer_expr(inner);
                if self.symbols.types.contains_key(schema_name) {
                    Type::Record(schema_name.clone())
                } else {
                    Type::Any
                }
            }
            Expr::RawStringLit(_, _) => Type::String,
            Expr::BytesLit(_, _) => Type::Bytes,
            Expr::Lambda {
                params,
                return_type,
                body,
                ..
            } => {
                let saved_locals = self.locals.clone();
                let saved_mutables = self.mutables.clone();
                let mut param_types = Vec::new();
                for p in params {
                    let pt = resolve_type_expr(&p.ty, self.symbols);
                    if let Some(ref def) = p.default_value {
                        self.infer_expr(def);
                    }
                    self.locals.insert(p.name.clone(), pt.clone());
                    self.mutables.insert(p.name.clone(), true);
                    param_types.push(pt);
                }
                let ret = if let Some(ref rt) = return_type {
                    resolve_type_expr(rt, self.symbols)
                } else {
                    match body {
                        LambdaBody::Expr(e) => self.infer_expr(e),
                        LambdaBody::Block(stmts) => {
                            for s in stmts {
                                self.check_stmt(s, None);
                            }
                            Type::Any
                        }
                    }
                };
                self.locals = saved_locals;
                self.mutables = saved_mutables;
                Type::Fn(param_types, Box::new(ret))
            }
            Expr::TupleLit(elems, _) => {
                let types: Vec<_> = elems.iter().map(|e| self.infer_expr(e)).collect();
                Type::Tuple(types)
            }
            Expr::SetLit(elems, _) => {
                if elems.is_empty() {
                    Type::Set(Box::new(Type::Any))
                } else {
                    let first = self.infer_expr(&elems[0]);
                    for e in &elems[1..] {
                        self.infer_expr(e);
                    }
                    Type::Set(Box::new(first))
                }
            }
            Expr::RangeExpr {
                start, end, step, ..
            } => {
                if let Some(ref s) = start {
                    self.infer_expr(s);
                }
                if let Some(ref e) = end {
                    self.infer_expr(e);
                }
                if let Some(ref st) = step {
                    self.infer_expr(st);
                }
                Type::List(Box::new(Type::Int))
            }
            Expr::TryExpr(inner, _) => {
                let t = self.infer_expr(inner);
                // If inner is Result[Ok, Err], return Ok type (propagating Err)
                if let Type::Result(ok, _) = t {
                    *ok
                } else {
                    t
                }
            }
            Expr::NullCoalesce(lhs, rhs, _) => {
                let lt = self.infer_expr(lhs);
                let rt = self.infer_expr(rhs);
                // If lhs is T | Null, result is T (or rhs type)
                match lt {
                    Type::Union(ref types) => {
                        let non_null: Vec<_> = types
                            .iter()
                            .filter(|t| **t != Type::Null)
                            .cloned()
                            .collect();
                        if non_null.len() == 1 {
                            non_null.into_iter().next().unwrap()
                        } else if non_null.is_empty() {
                            rt
                        } else {
                            Type::Union(non_null)
                        }
                    }
                    Type::Null => rt,
                    _ => lt,
                }
            }
            Expr::NullSafeAccess(obj, field, _span) => {
                let ot = self.infer_expr(obj);
                // Result is T | Null
                let field_type = if let Type::Record(ref name) = ot {
                    if let Some(ti) = self.symbols.types.get(name) {
                        if let crate::compiler::resolve::TypeInfoKind::Record(ref rd) = ti.kind {
                            if let Some(f) = rd.fields.iter().find(|f| f.name == *field) {
                                resolve_type_expr(&f.ty, self.symbols)
                            } else {
                                Type::Any
                            }
                        } else {
                            Type::Any
                        }
                    } else {
                        Type::Any
                    }
                } else {
                    Type::Any
                };
                Type::Union(vec![field_type, Type::Null])
            }
            Expr::NullAssert(inner, _) => {
                let t = self.infer_expr(inner);
                // Strip Null from union types
                match t {
                    Type::Union(ref types) => {
                        let non_null: Vec<_> = types
                            .iter()
                            .filter(|t| **t != Type::Null)
                            .cloned()
                            .collect();
                        if non_null.len() == 1 {
                            non_null.into_iter().next().unwrap()
                        } else if non_null.is_empty() {
                            Type::Any
                        } else {
                            Type::Union(non_null)
                        }
                    }
                    _ => t,
                }
            }
            Expr::SpreadExpr(inner, _) => self.infer_expr(inner),
            Expr::IfExpr {
                cond,
                then_val,
                else_val,
                ..
            } => {
                let ct = self.infer_expr(cond);
                self.check_compat(&Type::Bool, &ct, cond.span().line);
                let tt = self.infer_expr(then_val);
                self.infer_expr(else_val);
                tt
            }
            Expr::AwaitExpr(inner, _) => self.infer_expr(inner),
            Expr::Comprehension {
                body,
                var,
                iter,
                condition,
                kind,
                span: _,
            } => {
                let iter_type = self.infer_expr(iter);
                let elem_type = match &iter_type {
                    Type::List(inner) => *inner.clone(),
                    Type::Set(inner) => *inner.clone(),
                    _ => Type::Any,
                };
                self.locals.insert(var.clone(), elem_type);
                if let Some(ref cond) = condition {
                    let ct = self.infer_expr(cond);
                    self.check_compat(&Type::Bool, &ct, cond.span().line);
                }
                let body_type = self.infer_expr(body);
                match kind {
                    ComprehensionKind::List => Type::List(Box::new(body_type)),
                    ComprehensionKind::Set => Type::Set(Box::new(body_type)),
                    ComprehensionKind::Map => Type::Any, // map comprehension needs key+value
                }
            }
            Expr::MatchExpr {
                subject, arms, ..
            } => {
                let _subject_type = self.infer_expr(subject);
                let mut result_type = Type::Any;
                for arm in arms {
                    for s in &arm.body {
                        self.check_stmt(s, None);
                    }
                    // Infer type from last expression in arm body
                    if let Some(Stmt::Expr(es)) = arm.body.last() {
                        result_type = self.infer_expr(&es.expr);
                    } else if let Some(Stmt::Return(rs)) = arm.body.last() {
                        result_type = self.infer_expr(&rs.value);
                    }
                }
                result_type
            }
            Expr::BlockExpr(stmts, _) => {
                for s in stmts {
                    self.check_stmt(s, None);
                }
                // Infer type from last expression in block
                if let Some(Stmt::Expr(es)) = stmts.last() {
                    self.infer_expr(&es.expr)
                } else {
                    Type::Any
                }
            }
        }
    }

    fn check_compat(&mut self, expected: &Type, actual: &Type, line: usize) {
        if *expected == Type::Any || *actual == Type::Any {
            return;
        }
        if type_contains_any(expected) || type_contains_any(actual) {
            return;
        }
        if expected == actual {
            return;
        }

        // Union compatibility: actual is compatible if it matches any member of expected union
        if let Type::Union(ref types) = expected {
            if types.iter().any(|t| t == actual || *t == Type::Any) {
                return;
            }
        }
        // actual is union: compatible if all members are compatible with expected
        if let Type::Union(ref types) = actual {
            if types.iter().any(|t| t == expected || *t == Type::Any) {
                return;
            }
        }

        // Null is compatible with T | Null unions
        if *actual == Type::Null {
            if let Type::Union(ref types) = expected {
                if types.contains(&Type::Null) {
                    return;
                }
            }
        }

        if *expected == Type::Float && *actual == Type::Int {
            return;
        }

        // Result compatibility: Result[A, B] is compatible with Result[C, D] if A compat C, B compat D
        // Allow implicit wrapping into `ok(...)` when a plain value is returned for a Result type.
        if let Type::Result(ok, _) = expected {
            if **ok == *actual || **ok == Type::Any || *actual == Type::Any {
                return;
            }
        }
        // Generic type refs are compatible if the base name matches
        if let (Type::TypeRef(n1, _), Type::TypeRef(n2, _)) = (expected, actual) {
            if n1 == n2 {
                return;
            }
        }

        self.errors.push(TypeError::Mismatch {
            expected: format!("{}", expected),
            actual: format!("{}", actual),
            line,
        });
    }
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

    // Support attribute-style toggles, e.g. `@doc_mode true` parsed as Addon
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
    if has_attr { Some(true) } else { None }
}

/// Typecheck a program.
pub fn typecheck(program: &Program, symbols: &SymbolTable) -> Result<(), Vec<TypeError>> {
    let strict = parse_directive_bool(program, "strict").unwrap_or(true);
    let doc_mode = parse_directive_bool(program, "doc_mode").unwrap_or(false);
    let allow_placeholders = doc_mode || !strict;
    let mut checker = TypeChecker::new(symbols, allow_placeholders);
    for item in &program.items {
        match item {
            Item::Cell(c) => checker.check_cell(c),
            Item::Agent(a) => {
                for cell in &a.cells {
                    checker.check_agent_cell(cell);
                }
            }
            Item::Process(p) => {
                for cell in &p.cells {
                    checker.check_cell(cell);
                }
            }
            Item::Effect(e) => {
                for op in &e.operations {
                    checker.check_cell(op);
                }
            }
            Item::Handler(h) => {
                for handle in &h.handles {
                    checker.check_cell(handle);
                }
            }
            _ => {}
        }
    }
    if checker.errors.is_empty() {
        Ok(())
    } else {
        Err(checker.errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::lexer::Lexer;
    use crate::compiler::parser::Parser;
    use crate::compiler::resolve;

    fn typecheck_src(src: &str) -> Result<(), Vec<TypeError>> {
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let prog = parser.parse_program(vec![]).unwrap();
        let symbols = resolve::resolve(&prog).unwrap();
        typecheck(&prog, &symbols)
    }

    #[test]
    fn test_typecheck_basic() {
        typecheck_src("cell add(a: Int, b: Int) -> Int\n  return a + b\nend").unwrap();
    }

    #[test]
    fn test_typecheck_undefined_var() {
        let err = typecheck_src("cell bad() -> Int\n  return missing_var\nend").unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn test_type_alias_resolves_in_typecheck() {
        // Type alias should resolve to the underlying type
        typecheck_src(
            "type UserId = String\n\ncell greet(id: UserId) -> String\n  return id\nend",
        )
        .unwrap();
    }

    #[test]
    fn test_doc_mode_allows_any_undefined_var() {
        // In doc_mode, any undefined variable should be allowed
        typecheck_src(
            "@doc_mode true\n\ncell example() -> Int\n  return completely_unknown_var_xyz\nend",
        )
        .unwrap();
    }

    #[test]
    fn test_strict_mode_catches_undefined_var() {
        // In strict mode (default), undefined variables should be caught
        let err = typecheck_src(
            "cell example() -> Int\n  return completely_unknown_var_xyz\nend",
        )
        .unwrap_err();
        assert!(err.iter().any(|e| matches!(e, TypeError::UndefinedVar { name, .. } if name == "completely_unknown_var_xyz")));
    }

    #[test]
    fn test_is_doc_placeholder_var_rejects_dunder_names() {
        // Names starting with __ should not be placeholders (internal/generated names)
        assert!(!is_doc_placeholder_var("__pattern"));
        assert!(!is_doc_placeholder_var("__tuple"));
        // Normal names are fine
        assert!(is_doc_placeholder_var("x"));
        assert!(is_doc_placeholder_var("my_variable"));
    }

    #[test]
    fn test_type_alias_basic() {
        // Basic type alias to primitive type
        typecheck_src(
            "type UserId = String\n\ncell greet(id: UserId) -> UserId\n  return id\nend",
        )
        .unwrap();
    }

    #[test]
    fn test_type_alias_complex() {
        // Type alias to complex type
        typecheck_src(
            "type StringList = list[String]\n\ncell make_list() -> StringList\n  return [\"a\", \"b\"]\nend",
        )
        .unwrap();
    }

    #[test]
    fn test_type_alias_in_record() {
        // Type alias used in record field
        typecheck_src(
            "type Email = String\n\nrecord User\n  email: Email\nend\n\ncell get_email(u: User) -> Email\n  return u.email\nend",
        )
        .unwrap();
    }

    #[test]
    fn test_type_alias_chained() {
        // Chained type aliases: A -> B -> String
        typecheck_src(
            "type UserId = String\ntype Id = UserId\n\ncell make_id() -> Id\n  return \"123\"\nend",
        )
        .unwrap();
    }
}
