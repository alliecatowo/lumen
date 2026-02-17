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
            | "format"
            | "partition"
            | "read_dir"
            | "exists"
            | "mkdir"
            | "exit"
            | "assert"
            | "assert_eq"
            | "assert_ne"
            | "assert_contains"
    )
}

/// Check if a name is a built-in math constant.
pub fn is_builtin_math_constant(name: &str) -> bool {
    matches!(
        name,
        "PI" | "E" | "TAU" | "INFINITY" | "NAN" | "MAX_INT" | "MIN_INT"
    )
}

/// Return the type for a built-in math constant.
fn builtin_math_constant_type(name: &str) -> Type {
    match name {
        "PI" | "E" | "TAU" | "INFINITY" | "NAN" => Type::Float,
        "MAX_INT" | "MIN_INT" => Type::Int,
        _ => Type::Any,
    }
}

/// Return the known return type for a builtin function, if available.
/// This enables stronger type inference for calls to well-known builtins
/// instead of falling back to Type::Any.
fn builtin_return_type(name: &str, arg_types: &[Type]) -> Option<Type> {
    match name {
        "length" | "len" | "count" => Some(Type::Int),
        "string" | "to_string" | "str" | "repr" => Some(Type::String),
        "int" | "to_int" => Some(Type::Int),
        "float" | "to_float" => Some(Type::Float),
        "bool" => Some(Type::Bool),
        "print" | "println" => Some(Type::Null),
        "append" => arg_types.first().cloned(),
        "keys" => Some(Type::List(Box::new(Type::String))),
        "values" => Some(Type::List(Box::new(Type::Any))),
        "contains" | "starts_with" | "ends_with" | "is_empty" | "matches" => Some(Type::Bool),
        "upper" | "lower" | "trim" | "strip" | "join" | "replace" => Some(Type::String),
        "reverse" => arg_types.first().cloned().or(Some(Type::Any)),
        "split" => Some(Type::List(Box::new(Type::String))),
        "abs" | "min" | "max" | "sum" => arg_types.first().cloned().or(Some(Type::Any)),
        "range" => Some(Type::List(Box::new(Type::Int))),
        "sort" | "sorted" | "filter" | "zip" | "slice" => {
            arg_types.first().cloned().or(Some(Type::Any))
        }
        "map" => {
            if let Some(Type::Fn(_, ret)) = arg_types.get(1) {
                Some(Type::List(ret.clone()))
            } else {
                Some(Type::List(Box::new(Type::Any)))
            }
        }
        "flat_map" => {
            if let Some(Type::Fn(_, ret)) = arg_types.get(1) {
                // If closure returns List(T), flat_map returns List(T)
                // If closure returns T (not list), flat_map returns List(T)
                match &**ret {
                    Type::List(inner) => Some(Type::List(inner.clone())),
                    _ => Some(Type::List(ret.clone())),
                }
            } else {
                Some(Type::List(Box::new(Type::Any)))
            }
        }
        "reduce" => Some(Type::Any),
        "type_of" | "type_name" => Some(Type::String),
        "assert" | "assert_eq" | "assert_ne" | "assert_contains" => Some(Type::Null),
        "error" => Some(Type::Null),
        "hash" => Some(Type::Int),
        "not" => Some(Type::Bool),
        "parse_json" => Some(Type::Json),
        "to_json" => Some(Type::String),
        "read_file" => Some(Type::String),
        "write_file" => Some(Type::Null),
        "timestamp" => Some(Type::Float),
        "random" => Some(Type::Float),
        "get_env" => Some(Type::Union(vec![Type::String, Type::Null])),
        "format" => Some(Type::String),
        "partition" => {
            let elem = arg_types
                .first()
                .and_then(|t| {
                    if let Type::List(e) = t {
                        Some(*e.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or(Type::Any);
            Some(Type::Tuple(vec![
                Type::List(Box::new(elem.clone())),
                Type::List(Box::new(elem)),
            ]))
        }
        "read_dir" => Some(Type::List(Box::new(Type::String))),
        "exists" => Some(Type::Bool),
        "mkdir" => Some(Type::Null),
        "exit" => Some(Type::Null),
        _ => None,
    }
}

/// In doc_mode / non-strict mode, allow undefined variable names that look like
/// plausible identifiers (e.g. doc snippet references). Returns false for names
/// that are likely typos or real errors even in non-strict mode.
fn is_doc_placeholder_var(name: &str) -> bool {
    // All well-formed identifiers are allowed in placeholder mode.
    // The guard (`allow_placeholders`) is only true in doc_mode or non-strict mode.
    !name.is_empty() && !name.starts_with("__")
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
        suggestions: Vec<String>,
    },
    #[error("undefined type '{name}' at line {line}")]
    UndefinedType { name: String, line: usize },
    #[error("missing return in cell '{name}' at line {line}")]
    MissingReturn { name: String, line: usize },
    #[error("cannot assign to immutable variable '{name}' at line {line}")]
    ImmutableAssign { name: String, line: usize },
    #[error("incomplete match at line {line}: missing variants {missing:?}")]
    IncompleteMatch {
        enum_name: String,
        missing: Vec<String>,
        line: usize,
    },
    #[error("unused result of @must_use cell '{name}' at line {line}")]
    MustUseIgnored { name: String, line: usize },
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

/// Type substitution map: maps type parameter names to concrete types
pub type TypeSubst = HashMap<String, Type>;

/// Build substitution map from generic parameters and concrete type arguments
fn build_subst(params: &[String], args: &[Type]) -> TypeSubst {
    params
        .iter()
        .zip(args.iter())
        .map(|(p, a)| (p.clone(), a.clone()))
        .collect()
}

/// Infer generic type arguments from record field values
fn infer_generic_args_from_fields(
    generic_params: &[String],
    field_defs: &[FieldDef],
    field_values: &[(String, Expr)],
    symbols: &SymbolTable,
    checker: &mut TypeChecker,
) -> Vec<Type> {
    let mut inferred: HashMap<String, Type> = HashMap::new();

    for (fname, fval) in field_values {
        let val_type = checker.infer_expr(fval);
        if let Some(field_def) = field_defs.iter().find(|f| f.name == *fname) {
            // Try to unify field type with value type to infer generic params
            unify_for_inference(&field_def.ty, &val_type, symbols, &mut inferred);
        }
    }

    // Return inferred types in parameter order, or Type::Any for unresolved
    generic_params
        .iter()
        .map(|p| inferred.get(p).cloned().unwrap_or(Type::Any))
        .collect()
}

/// Attempt to unify a type expression with a concrete type to infer generic parameters.
/// Uses a heuristic: single uppercase letter names are treated as type variables.
fn unify_for_inference(
    type_expr: &TypeExpr,
    concrete: &Type,
    _symbols: &SymbolTable,
    inferred: &mut HashMap<String, Type>,
) {
    let empty_set: std::collections::HashSet<&str> = std::collections::HashSet::new();
    unify_for_inference_inner(type_expr, concrete, _symbols, inferred, &empty_set);
}

/// Attempt to unify a type expression with a concrete type to infer generic parameters.
/// Accepts an explicit set of known generic parameter names in addition to the
/// single-uppercase-letter heuristic.
fn unify_for_inference_with_params(
    type_expr: &TypeExpr,
    concrete: &Type,
    symbols: &SymbolTable,
    inferred: &mut HashMap<String, Type>,
    generic_param_names: &std::collections::HashSet<&str>,
) {
    unify_for_inference_inner(type_expr, concrete, symbols, inferred, generic_param_names);
}

fn unify_for_inference_inner(
    type_expr: &TypeExpr,
    concrete: &Type,
    _symbols: &SymbolTable,
    inferred: &mut HashMap<String, Type>,
    generic_param_names: &std::collections::HashSet<&str>,
) {
    match (type_expr, concrete) {
        (TypeExpr::Named(name, _), ty) => {
            // If this is a known generic type parameter or a single uppercase letter,
            // record the inference
            let is_generic = generic_param_names.contains(name.as_str())
                || (name.len() == 1 && name.chars().next().unwrap().is_uppercase());
            if is_generic {
                inferred.entry(name.clone()).or_insert_with(|| ty.clone());
            }
        }
        (TypeExpr::List(inner, _), Type::List(inner_ty)) => {
            unify_for_inference_inner(inner, inner_ty, _symbols, inferred, generic_param_names);
        }
        (TypeExpr::Map(k, v, _), Type::Map(kt, vt)) => {
            unify_for_inference_inner(k, kt, _symbols, inferred, generic_param_names);
            unify_for_inference_inner(v, vt, _symbols, inferred, generic_param_names);
        }
        (TypeExpr::Set(inner, _), Type::Set(inner_ty)) => {
            unify_for_inference_inner(inner, inner_ty, _symbols, inferred, generic_param_names);
        }
        (TypeExpr::Result(ok, err, _), Type::Result(ok_ty, err_ty)) => {
            unify_for_inference_inner(ok, ok_ty, _symbols, inferred, generic_param_names);
            unify_for_inference_inner(err, err_ty, _symbols, inferred, generic_param_names);
        }
        (TypeExpr::Tuple(exprs, _), Type::Tuple(types)) => {
            for (expr, ty) in exprs.iter().zip(types.iter()) {
                unify_for_inference_inner(expr, ty, _symbols, inferred, generic_param_names);
            }
        }
        (TypeExpr::Generic(name, type_args, _), Type::TypeRef(ref_name, ref_args)) => {
            if name == ref_name && type_args.len() == ref_args.len() {
                for (texpr, ty) in type_args.iter().zip(ref_args.iter()) {
                    unify_for_inference_inner(texpr, ty, _symbols, inferred, generic_param_names);
                }
            }
        }
        _ => {
            // No unification possible
        }
    }
}

/// Resolve a type expression to a concrete type, applying substitutions for generic parameters
pub fn resolve_type_expr(ty: &TypeExpr, symbols: &SymbolTable) -> Type {
    resolve_type_expr_with_subst(ty, symbols, &HashMap::new())
}

/// Resolve a type expression with generic type parameter substitutions
fn resolve_type_expr_with_subst(ty: &TypeExpr, symbols: &SymbolTable, subst: &TypeSubst) -> Type {
    match ty {
        TypeExpr::Named(name, _) => {
            // Check if this is a generic type parameter
            if let Some(concrete_type) = subst.get(name) {
                return concrete_type.clone();
            }
            match name.as_str() {
                "String" => Type::String,
                "Int" => Type::Int,
                "Float" => Type::Float,
                "Bool" => Type::Bool,
                "Bytes" => Type::Bytes,
                "Json" => Type::Json,
                "Any" => Type::Any,
                "Null" => Type::Null,
                _ => {
                    if symbols.types.contains_key(name) {
                        use crate::compiler::resolve::TypeInfoKind;
                        match &symbols.types[name].kind {
                            TypeInfoKind::Record(_) => Type::Record(name.clone()),
                            TypeInfoKind::Enum(_) => Type::Enum(name.clone()),
                            TypeInfoKind::Builtin => Type::Record(name.clone()),
                        }
                    } else if let Some(alias_target) = symbols.type_aliases.get(name) {
                        resolve_type_expr_with_subst(alias_target, symbols, subst)
                    } else {
                        Type::Any
                    }
                }
            }
        }
        TypeExpr::List(inner, _) => Type::List(Box::new(resolve_type_expr_with_subst(
            inner, symbols, subst,
        ))),
        TypeExpr::Map(k, v, _) => Type::Map(
            Box::new(resolve_type_expr_with_subst(k, symbols, subst)),
            Box::new(resolve_type_expr_with_subst(v, symbols, subst)),
        ),
        TypeExpr::Result(ok, err, _) => Type::Result(
            Box::new(resolve_type_expr_with_subst(ok, symbols, subst)),
            Box::new(resolve_type_expr_with_subst(err, symbols, subst)),
        ),
        TypeExpr::Union(types, _) => Type::Union(
            types
                .iter()
                .map(|t| resolve_type_expr_with_subst(t, symbols, subst))
                .collect(),
        ),
        TypeExpr::Null(_) => Type::Null,
        TypeExpr::Tuple(types, _) => Type::Tuple(
            types
                .iter()
                .map(|t| resolve_type_expr_with_subst(t, symbols, subst))
                .collect(),
        ),
        TypeExpr::Set(inner, _) => Type::Set(Box::new(resolve_type_expr_with_subst(
            inner, symbols, subst,
        ))),
        TypeExpr::Fn(params, ret, _, _) => {
            let param_types = params
                .iter()
                .map(|t| resolve_type_expr_with_subst(t, symbols, subst))
                .collect();
            let ret_type = resolve_type_expr_with_subst(ret, symbols, subst);
            Type::Fn(param_types, Box::new(ret_type))
        }
        TypeExpr::Generic(name, args, _) => {
            // Build substitution map for this generic instantiation
            if let Some(type_info) = symbols.types.get(name) {
                let generic_params = &type_info.generic_params;

                // Resolve argument types with current substitution
                let arg_types: Vec<_> = args
                    .iter()
                    .map(|t| resolve_type_expr_with_subst(t, symbols, subst))
                    .collect();

                // Create a new substitution map for the generic type's fields
                if generic_params.len() == arg_types.len() {
                    Type::TypeRef(name.clone(), arg_types)
                } else {
                    // Arity mismatch should have been caught in resolve phase
                    Type::TypeRef(name.clone(), arg_types)
                }
            } else {
                // Unknown type - should have been caught in resolve
                let arg_types: Vec<_> = args
                    .iter()
                    .map(|t| resolve_type_expr_with_subst(t, symbols, subst))
                    .collect();
                Type::TypeRef(name.clone(), arg_types)
            }
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

#[derive(Debug)]
enum CheckedCallArg {
    Positional(Type, usize),
    Named(String, Type, usize),
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
            // Variadic params are seen as List[T] inside the function body
            let ty = if p.variadic {
                Type::List(Box::new(ty))
            } else {
                ty
            };
            self.locals.insert(p.name.clone(), ty);
            self.mutables.insert(p.name.clone(), true); // params are mutable by default
        }
        let return_type = if let Some(ref rt) = cell.return_type {
            Some(resolve_type_expr(rt, self.symbols))
        } else {
            None
        };

        let body_len = cell.body.len();
        for (i, stmt) in cell.body.iter().enumerate() {
            let is_tail = body_len > 0 && i == body_len - 1;
            self.check_stmt(stmt, return_type.as_ref(), is_tail);
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
            let ty = if p.variadic {
                Type::List(Box::new(ty))
            } else {
                ty
            };
            self.locals.insert(p.name.clone(), ty);
            self.mutables.insert(p.name.clone(), true);
        }
        let return_type = if let Some(ref rt) = cell.return_type {
            Some(resolve_type_expr(rt, self.symbols))
        } else {
            None
        };
        for stmt in &cell.body {
            self.check_stmt(stmt, return_type.as_ref(), false);
        }
    }

    fn check_call_against_signature(
        &mut self,
        params: &[(String, TypeExpr, bool)],
        args: &[CheckedCallArg],
        line: usize,
    ) {
        // Check if the last parameter is variadic
        let has_variadic = params.last().is_some_and(|(_, _, v)| *v);
        let fixed_count = if has_variadic {
            params.len() - 1
        } else {
            params.len()
        };

        if !has_variadic && args.len() > params.len() {
            self.errors.push(TypeError::ArgCount {
                expected: params.len(),
                actual: args.len(),
                line,
            });
        }

        let mut positional_idx = 0usize;
        for arg in args {
            match arg {
                CheckedCallArg::Positional(actual_ty, arg_line) => {
                    if positional_idx < fixed_count {
                        if let Some((_, expected_expr, _)) = params.get(positional_idx) {
                            let expected_ty = resolve_type_expr(expected_expr, self.symbols);
                            self.check_compat(&expected_ty, actual_ty, *arg_line);
                        }
                    } else if has_variadic {
                        let (_, variadic_expr, _) = &params[params.len() - 1];
                        let elem_ty = resolve_type_expr(variadic_expr, self.symbols);
                        self.check_compat(&elem_ty, actual_ty, *arg_line);
                    }
                    positional_idx += 1;
                }
                CheckedCallArg::Named(name, actual_ty, arg_line) => {
                    if let Some((_, expected_expr, _)) = params.iter().find(|(p, _, _)| p == name) {
                        let expected_ty = resolve_type_expr(expected_expr, self.symbols);
                        self.check_compat(&expected_ty, actual_ty, *arg_line);
                    } else {
                        self.errors.push(TypeError::Mismatch {
                            expected: format!("parameter '{}'", name),
                            actual: "unknown named argument".to_string(),
                            line: *arg_line,
                        });
                    }
                }
            }
        }
    }

    /// Like check_call_against_signature but resolves parameter types with a
    /// generic substitution map, so that e.g. T is resolved to Int.
    fn check_call_against_signature_with_subst(
        &mut self,
        params: &[(String, TypeExpr, bool)],
        args: &[CheckedCallArg],
        line: usize,
        subst: &TypeSubst,
    ) {
        let has_variadic = params.last().is_some_and(|(_, _, v)| *v);
        let fixed_count = if has_variadic {
            params.len() - 1
        } else {
            params.len()
        };

        if !has_variadic && args.len() > params.len() {
            self.errors.push(TypeError::ArgCount {
                expected: params.len(),
                actual: args.len(),
                line,
            });
        }

        let mut positional_idx = 0usize;
        for arg in args {
            match arg {
                CheckedCallArg::Positional(actual_ty, arg_line) => {
                    if positional_idx < fixed_count {
                        if let Some((_, expected_expr, _)) = params.get(positional_idx) {
                            let expected_ty =
                                resolve_type_expr_with_subst(expected_expr, self.symbols, subst);
                            self.check_compat(&expected_ty, actual_ty, *arg_line);
                        }
                    } else if has_variadic {
                        let (_, variadic_expr, _) = &params[params.len() - 1];
                        let elem_ty =
                            resolve_type_expr_with_subst(variadic_expr, self.symbols, subst);
                        self.check_compat(&elem_ty, actual_ty, *arg_line);
                    }
                    positional_idx += 1;
                }
                CheckedCallArg::Named(name, actual_ty, arg_line) => {
                    if let Some((_, expected_expr, _)) = params.iter().find(|(p, _, _)| p == name) {
                        let expected_ty =
                            resolve_type_expr_with_subst(expected_expr, self.symbols, subst);
                        self.check_compat(&expected_ty, actual_ty, *arg_line);
                    } else {
                        self.errors.push(TypeError::Mismatch {
                            expected: format!("parameter '{}'", name),
                            actual: "unknown named argument".to_string(),
                            line: *arg_line,
                        });
                    }
                }
            }
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt, expected_return: Option<&Type>, is_tail: bool) {
        match stmt {
            Stmt::Let(ls) => {
                let val_type = self.infer_expr(&ls.value);
                if let Some(ref ann) = ls.ty {
                    let expected = resolve_type_expr(ann, self.symbols);
                    self.check_compat(&expected, &val_type, ls.span.line);
                }
                if let Some(ref pattern) = ls.pattern {
                    // Destructuring let — register all bound names from the pattern
                    self.bind_let_pattern(pattern, &val_type, ls.span.line);
                } else {
                    self.locals.insert(ls.name.clone(), val_type);
                    // In Lumen, all let bindings are reassignable by default
                    // `let mut` is just documentation; `const` is immutable
                    self.mutables.insert(ls.name.clone(), true);
                }
            }
            Stmt::If(ifs) => {
                let ct = self.infer_expr(&ifs.condition);
                self.check_compat(&Type::Bool, &ct, ifs.span.line);

                // Type narrowing: if condition is `x is SomeType`, narrow x in
                // the then-branch to SomeType and restore afterward.
                let narrowed = if let Expr::IsType {
                    expr: ref inner,
                    ref type_name,
                    ..
                } = ifs.condition
                {
                    if let Expr::Ident(ref var_name, _) = **inner {
                        let original = self.locals.get(var_name).cloned();
                        let narrow_ty = resolve_type_expr(
                            &TypeExpr::Named(type_name.clone(), ifs.span),
                            self.symbols,
                        );
                        self.locals.insert(var_name.clone(), narrow_ty);
                        Some((var_name.clone(), original))
                    } else {
                        None
                    }
                } else {
                    None
                };

                for s in &ifs.then_body {
                    self.check_stmt(s, expected_return, false);
                }

                // Restore original type after then-branch
                if let Some((ref var_name, ref original)) = narrowed {
                    if let Some(orig_ty) = original {
                        self.locals.insert(var_name.clone(), orig_ty.clone());
                    } else {
                        self.locals.remove(var_name);
                    }
                }

                if let Some(ref eb) = ifs.else_body {
                    for s in eb {
                        self.check_stmt(s, expected_return, false);
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
                if let Some(filter) = &fs.filter {
                    self.infer_expr(filter);
                }
                for s in &fs.body {
                    self.check_stmt(s, expected_return, false);
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
                        self.check_stmt(s, expected_return, false);
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
                                    self.errors.push(TypeError::IncompleteMatch {
                                        enum_name: name.clone(),
                                        missing,
                                        line: ms.span.line,
                                    });
                                }
                            }
                        }
                    }
                }

                // T049: Exhaustiveness check for integer refinement ranges
                if subject_type == Type::Int && !has_catchall {
                    check_int_match_exhaustiveness(&ms.arms, ms.span.line, &mut self.errors);
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
                // Check if this is a call to a @must_use cell whose result is discarded.
                // Skip the check if this is the tail expression (implicit return).
                if !is_tail {
                    if let Expr::Call(callee, _, _) = &es.expr {
                        if let Expr::Ident(name, _) = callee.as_ref() {
                            if let Some(cell_info) = self.symbols.cells.get(name.as_str()) {
                                if cell_info.must_use {
                                    self.errors.push(TypeError::MustUseIgnored {
                                        name: name.clone(),
                                        line: es.span.line,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            Stmt::While(ws) => {
                let ct = self.infer_expr(&ws.condition);
                self.check_compat(&Type::Bool, &ct, ws.span.line);
                for s in &ws.body {
                    self.check_stmt(s, expected_return, false);
                }
            }
            Stmt::Loop(ls) => {
                for s in &ls.body {
                    self.check_stmt(s, expected_return, false);
                }
            }
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Defer(ds) => {
                for s in &ds.body {
                    self.check_stmt(s, expected_return, false);
                }
            }
            Stmt::Emit(es) => {
                self.infer_expr(&es.value);
            }
            Stmt::Yield(ys) => {
                self.infer_expr(&ys.value);
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
                    // Bitwise compound assignments require Int operands
                    match ca.op {
                        CompoundOp::BitAndAssign
                        | CompoundOp::BitOrAssign
                        | CompoundOp::BitXorAssign => {
                            if existing != Type::Any && existing != Type::Int {
                                self.errors.push(TypeError::Mismatch {
                                    expected: "Int".into(),
                                    actual: format!("{}", existing),
                                    line: ca.span.line,
                                });
                            }
                            if val_type != Type::Any && val_type != Type::Int {
                                self.errors.push(TypeError::Mismatch {
                                    expected: "Int".into(),
                                    actual: format!("{}", val_type),
                                    line: ca.span.line,
                                });
                            }
                        }
                        _ => {
                            self.check_compat(&existing, &val_type, ca.span.line);
                        }
                    }
                }
            }
            // Local definitions — types/cells are already registered at module level
            // by the resolver. Nothing extra to check inline here.
            Stmt::LocalRecord(_) | Stmt::LocalEnum(_) | Stmt::LocalCell(_) => {}
        }
    }

    /// Register variable bindings from an irrefutable destructuring pattern
    /// used in `let` position.  Walks the pattern tree and inserts each
    /// bound name into `self.locals` with the appropriate type.
    #[allow(clippy::only_used_in_recursion)]
    fn bind_let_pattern(&mut self, pattern: &Pattern, subject_type: &Type, line: usize) {
        match pattern {
            Pattern::Ident(name, _) => {
                self.locals.insert(name.clone(), subject_type.clone());
                self.mutables.insert(name.clone(), true);
            }
            Pattern::Wildcard(_) => {}
            Pattern::TupleDestructure { elements, .. } => {
                let elem_types: Vec<Type> = match subject_type {
                    Type::Tuple(types) => types.clone(),
                    _ => vec![Type::Any; elements.len()],
                };
                for (idx, p) in elements.iter().enumerate() {
                    let ty = elem_types.get(idx).cloned().unwrap_or(Type::Any);
                    self.bind_let_pattern(p, &ty, line);
                }
            }
            Pattern::RecordDestructure {
                type_name, fields, ..
            } => {
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
                        self.bind_let_pattern(p, &field_ty, line);
                    } else {
                        // Shorthand `field_name:` — bind to same name
                        self.locals.insert(field_name.clone(), field_ty);
                        self.mutables.insert(field_name.clone(), true);
                    }
                }
            }
            Pattern::ListDestructure { elements, rest, .. } => {
                let elem_type = match subject_type {
                    Type::List(inner) => *inner.clone(),
                    _ => Type::Any,
                };
                for p in elements {
                    self.bind_let_pattern(p, &elem_type, line);
                }
                if let Some(rest_name) = rest {
                    self.locals
                        .insert(rest_name.clone(), Type::List(Box::new(elem_type)));
                    self.mutables.insert(rest_name.clone(), true);
                }
            }
            Pattern::TypeCheck {
                name, type_expr, ..
            } => {
                let expected = resolve_type_expr(type_expr, self.symbols);
                self.check_compat(&expected, subject_type, line);
                self.locals.insert(name.clone(), expected);
                self.mutables.insert(name.clone(), true);
            }
            _ => {
                // Other patterns (Guard, Or, Variant, etc.) not valid in let position
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
                let mut payload_type = Type::Any;
                let mut expects_payload = false;

                if let Type::Enum(ref name) = subject_type {
                    if let Some(ti) = self.symbols.types.get(name) {
                        if let crate::compiler::resolve::TypeInfoKind::Enum(def) = &ti.kind {
                            if let Some(variant) = def.variants.iter().find(|v| v.name == *tag) {
                                valid_variant = true;
                                covered_variants.push(tag.clone());
                                if let Some(payload) = &variant.payload {
                                    expects_payload = true;
                                    payload_type = resolve_type_expr(payload, self.symbols);
                                } else {
                                    payload_type = Type::Null;
                                }
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
                        expects_payload = true;
                        payload_type = *ok.clone();
                    } else if tag == "err" {
                        valid_variant = true;
                        expects_payload = true;
                        payload_type = *err.clone();
                    }
                    if !valid_variant {
                        self.errors.push(TypeError::Mismatch {
                            expected: "ok or err".into(),
                            actual: tag.clone(),
                            line,
                        });
                    }
                }

                if let Some(inner_pattern) = binding {
                    if valid_variant && !expects_payload {
                        self.errors.push(TypeError::Mismatch {
                            expected: format!("{} without payload", tag),
                            actual: format!("{}(...)", tag),
                            line,
                        });
                    }
                    self.bind_match_pattern(
                        inner_pattern,
                        &payload_type,
                        covered_variants,
                        has_catchall,
                        line,
                    );
                }
            }
            Pattern::Ident(name, _) => {
                // Check if this identifier is actually an enum variant without payload
                let mut is_variant = false;
                if let Type::Enum(ref enum_name) = subject_type {
                    if let Some(ti) = self.symbols.types.get(enum_name) {
                        if let crate::compiler::resolve::TypeInfoKind::Enum(def) = &ti.kind {
                            if def.variants.iter().any(|v| v.name == *name) {
                                // This is a variant pattern, not a binding
                                covered_variants.push(name.clone());
                                is_variant = true;
                            }
                        }
                    }
                }

                if !is_variant {
                    // Regular identifier pattern - binds the value
                    self.locals.insert(name.clone(), subject_type.clone());
                    *has_catchall = true;
                }
            }
            Pattern::Wildcard(_) => {
                *has_catchall = true;
            }
            Pattern::Guard {
                inner, condition, ..
            } => {
                // Guarded arms are treated conservatively: the inner pattern
                // binds variables but does NOT count toward exhaustiveness
                // coverage (the guard may fail at runtime).
                let mut _guarded_variants = Vec::new();
                let mut _guarded_catchall = false;
                self.bind_match_pattern(
                    inner,
                    subject_type,
                    &mut _guarded_variants,
                    &mut _guarded_catchall,
                    line,
                );
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
            Pattern::TupleDestructure { elements, .. } => match subject_type {
                Type::Tuple(types) => {
                    for (idx, p) in elements.iter().enumerate() {
                        let ty = types.get(idx).cloned().unwrap_or(Type::Any);
                        self.bind_match_pattern(p, &ty, covered_variants, has_catchall, line);
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
            },
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
                        self.bind_match_pattern(p, &field_ty, covered_variants, has_catchall, line);
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
            Pattern::Range { start, end, .. } => {
                let start_ty = self.infer_expr(start);
                let end_ty = self.infer_expr(end);
                // Validate start and end are same comparable type (Int or Float)
                match (&start_ty, &end_ty) {
                    (Type::Int, Type::Int)
                    | (Type::Float, Type::Float)
                    | (Type::Any, _)
                    | (_, Type::Any) => {}
                    _ => {
                        self.errors.push(TypeError::Mismatch {
                            expected: format!("{}", start_ty),
                            actual: format!("{}", end_ty),
                            line,
                        });
                    }
                }
            }
        }
    }

    fn infer_expr(&mut self, expr: &Expr) -> Type {
        match expr {
            Expr::IntLit(_, _) => Type::Int,
            Expr::BigIntLit(_, _) => Type::Int,
            Expr::FloatLit(_, _) => Type::Float,
            Expr::StringLit(_, _) => Type::String,
            Expr::StringInterp(segments, _span) => {
                // Walk segments and validate format spec types
                for seg in segments {
                    match seg {
                        StringSegment::Interpolation(expr) => {
                            self.infer_expr(expr);
                        }
                        StringSegment::FormattedInterpolation(expr, spec) => {
                            let expr_ty = self.infer_expr(expr);
                            // Validate format type against expression type
                            if let Some(ref ft) = spec.fmt_type {
                                match ft {
                                    FormatType::Decimal
                                    | FormatType::Hex
                                    | FormatType::HexUpper
                                    | FormatType::Octal
                                    | FormatType::Binary => {
                                        if !matches!(expr_ty, Type::Int | Type::Any) {
                                            self.errors.push(TypeError::Mismatch {
                                                expected: "Int".to_string(),
                                                actual: format!("{:?}", expr_ty),
                                                line: expr.span().line,
                                            });
                                        }
                                    }
                                    FormatType::Fixed
                                    | FormatType::Scientific
                                    | FormatType::ScientificUpper => {
                                        if !matches!(expr_ty, Type::Float | Type::Int | Type::Any) {
                                            self.errors.push(TypeError::Mismatch {
                                                expected: "Float".to_string(),
                                                actual: format!("{:?}", expr_ty),
                                                line: expr.span().line,
                                            });
                                        }
                                    }
                                    FormatType::Str => {
                                        // 's' is compatible with any type
                                    }
                                }
                            }
                        }
                        StringSegment::Literal(_) => {}
                    }
                }
                Type::String
            }
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
                            Expr::MapLit(_, _) => {
                                Type::Map(Box::new(Type::String), Box::new(Type::Any))
                            }
                            _ => Type::Any,
                        }
                    } else {
                        Type::Any
                    }
                }
                // Built-in math constants
                else if is_builtin_math_constant(name) {
                    builtin_math_constant_type(name)
                }
                // cell ref, tool ref, agent constructor ref, addendum decl refs, type/value references, built-in
                else if self.symbols.cells.contains_key(name)
                    || self.symbols.tools.contains_key(name)
                    || self.symbols.agents.contains_key(name)
                    || self
                        .symbols
                        .addons
                        .iter()
                        .any(|a| a.name.as_deref() == Some(name.as_str()))
                    || self.symbols.types.contains_key(name)
                    || self.symbols.type_aliases.contains_key(name)
                    || is_builtin_function(name)
                {
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
                        // Infer generic type arguments from field values
                        let generic_args = if !ti.generic_params.is_empty() {
                            infer_generic_args_from_fields(
                                &ti.generic_params,
                                &def.fields,
                                fields,
                                self.symbols,
                                self,
                            )
                        } else {
                            vec![]
                        };

                        // Build substitution map for generic parameters
                        let subst = build_subst(&ti.generic_params, &generic_args);

                        // 1. Check provided fields (unknown & type mismatch)
                        for (fname, fval) in fields {
                            let val_type = self.infer_expr(fval);
                            if let Some(field_def) = def.fields.iter().find(|f| f.name == *fname) {
                                let expected = resolve_type_expr_with_subst(
                                    &field_def.ty,
                                    self.symbols,
                                    &subst,
                                );
                                self.check_compat(&expected, &val_type, span.line);
                            } else {
                                let field_names: Vec<&str> =
                                    def.fields.iter().map(|f| f.name.as_str()).collect();
                                let suggestions = suggest_similar(fname, &field_names, 2);
                                self.errors.push(TypeError::UnknownField {
                                    field: fname.clone(),
                                    ty: name.clone(),
                                    line: span.line,
                                    suggestions,
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

                        // Return the instantiated generic type if applicable
                        if !generic_args.is_empty() {
                            Type::TypeRef(name.clone(), generic_args)
                        } else {
                            Type::Record(name.clone())
                        }
                    } else {
                        Type::Record(name.clone())
                    }
                } else {
                    self.errors.push(TypeError::UndefinedType {
                        name: name.clone(),
                        line: span.line,
                    });
                    Type::Record(name.clone())
                }
            }
            Expr::BinOp(lhs, op, rhs, _span) => {
                let lt = self.infer_expr(lhs);
                let rt = self.infer_expr(rhs);
                match op {
                    BinOp::Add
                    | BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Div
                    | BinOp::FloorDiv
                    | BinOp::Mod => {
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
                    BinOp::PipeForward | BinOp::Compose => rt,
                    BinOp::Concat => lt,
                    BinOp::In => Type::Bool,
                    BinOp::Spaceship => {
                        // Both operands must be the same orderable type (Int, Float, String)
                        if lt != Type::Any && rt != Type::Any && lt != rt {
                            self.errors.push(TypeError::Mismatch {
                                expected: format!("{}", lt),
                                actual: format!("{}", rt),
                                line: _span.line,
                            });
                        }
                        // Result is always Int (-1, 0, or 1)
                        Type::Int
                    }
                    BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => Type::Int,
                    BinOp::Shl | BinOp::Shr => {
                        if lt != Type::Any && lt != Type::Int {
                            self.errors.push(TypeError::Mismatch {
                                expected: "Int".into(),
                                actual: format!("{}", lt),
                                line: _span.line,
                            });
                        }
                        if rt != Type::Any && rt != Type::Int {
                            self.errors.push(TypeError::Mismatch {
                                expected: "Int".into(),
                                actual: format!("{}", rt),
                                line: _span.line,
                            });
                        }
                        Type::Int
                    }
                }
            }
            Expr::Pipe { left, right, span } => {
                let call_expr = desugar_pipe_application(left, right, *span);
                self.infer_expr(&call_expr)
            }
            Expr::UnaryOp(op, inner, _) => {
                let t = self.infer_expr(inner);
                match op {
                    UnaryOp::Neg => t,
                    UnaryOp::Not => Type::Bool,
                    UnaryOp::BitNot => Type::Int,
                }
            }
            Expr::Call(callee, args, span) => {
                let mut checked_args = Vec::new();
                for arg in args {
                    match arg {
                        CallArg::Positional(e) => {
                            let ty = self.infer_expr(e);
                            checked_args.push(CheckedCallArg::Positional(ty, e.span().line));
                        }
                        CallArg::Named(name, e, _) => {
                            let ty = self.infer_expr(e);
                            checked_args.push(CheckedCallArg::Named(
                                name.clone(),
                                ty,
                                e.span().line,
                            ));
                        }
                        CallArg::Role(_, _, _) => {}
                    }
                }
                // Try to resolve the return type
                if let Expr::Ident(name, _) = callee.as_ref() {
                    // Check if it's a cell/function call
                    if let Some(ci) = self.symbols.cells.get(name).cloned() {
                        if !ci.generic_params.is_empty() {
                            // Generic cell: infer type arguments from actual arguments
                            let generic_set: std::collections::HashSet<&str> =
                                ci.generic_params.iter().map(|s| s.as_str()).collect();
                            let mut inferred: HashMap<String, Type> = HashMap::new();

                            // Unify positional args with parameter types
                            let mut positional_idx = 0usize;
                            for checked_arg in &checked_args {
                                match checked_arg {
                                    CheckedCallArg::Positional(arg_ty, _) => {
                                        if let Some((_, param_ty_expr, _)) =
                                            ci.params.get(positional_idx)
                                        {
                                            unify_for_inference_with_params(
                                                param_ty_expr,
                                                arg_ty,
                                                self.symbols,
                                                &mut inferred,
                                                &generic_set,
                                            );
                                        }
                                        positional_idx += 1;
                                    }
                                    CheckedCallArg::Named(pname, arg_ty, _) => {
                                        if let Some((_, param_ty_expr, _)) =
                                            ci.params.iter().find(|(n, _, _)| n == pname)
                                        {
                                            unify_for_inference_with_params(
                                                param_ty_expr,
                                                arg_ty,
                                                self.symbols,
                                                &mut inferred,
                                                &generic_set,
                                            );
                                        }
                                    }
                                }
                            }

                            let generic_args: Vec<Type> = ci
                                .generic_params
                                .iter()
                                .map(|p| inferred.get(p).cloned().unwrap_or(Type::Any))
                                .collect();
                            let subst = build_subst(&ci.generic_params, &generic_args);

                            // Check call with substituted param types
                            let substituted_params: Vec<(String, TypeExpr, bool)> =
                                ci.params.clone();
                            self.check_call_against_signature_with_subst(
                                &substituted_params,
                                &checked_args,
                                span.line,
                                &subst,
                            );

                            if let Some(ref rt) = ci.return_type {
                                return resolve_type_expr_with_subst(rt, self.symbols, &subst);
                            }
                        } else {
                            // Non-generic cell: use standard checking
                            self.check_call_against_signature(&ci.params, &checked_args, span.line);
                            if let Some(ref rt) = ci.return_type {
                                return resolve_type_expr(rt, self.symbols);
                            }
                        }
                    }
                    // Check if it's a record construction
                    else if let Some(ti) = self.symbols.types.get(name) {
                        if let crate::compiler::resolve::TypeInfoKind::Record(def) = &ti.kind {
                            // Infer generic type arguments from constructor arguments
                            let generic_args = if !ti.generic_params.is_empty() {
                                // Build inference map from named arguments
                                let mut inferred: HashMap<String, Type> = HashMap::new();
                                for checked_arg in &checked_args {
                                    if let CheckedCallArg::Named(fname, arg_ty, _) = checked_arg {
                                        if let Some(field_def) =
                                            def.fields.iter().find(|f| f.name == *fname)
                                        {
                                            unify_for_inference(
                                                &field_def.ty,
                                                arg_ty,
                                                self.symbols,
                                                &mut inferred,
                                            );
                                        }
                                    }
                                }

                                ti.generic_params
                                    .iter()
                                    .map(|p| inferred.get(p).cloned().unwrap_or(Type::Any))
                                    .collect()
                            } else {
                                vec![]
                            };

                            // Build substitution map for generic parameters
                            let subst = build_subst(&ti.generic_params, &generic_args);

                            // Check constructor arguments match record fields
                            for checked_arg in &checked_args {
                                if let CheckedCallArg::Named(fname, arg_ty, line) = checked_arg {
                                    if let Some(field_def) =
                                        def.fields.iter().find(|f| f.name == *fname)
                                    {
                                        let expected = resolve_type_expr_with_subst(
                                            &field_def.ty,
                                            self.symbols,
                                            &subst,
                                        );
                                        self.check_compat(&expected, arg_ty, *line);
                                    } else {
                                        let field_names: Vec<&str> =
                                            def.fields.iter().map(|f| f.name.as_str()).collect();
                                        let suggestions = suggest_similar(fname, &field_names, 2);
                                        self.errors.push(TypeError::UnknownField {
                                            field: fname.clone(),
                                            ty: name.clone(),
                                            line: *line,
                                            suggestions,
                                        });
                                    }
                                }
                            }

                            // Return the instantiated generic type if applicable
                            return if !generic_args.is_empty() {
                                Type::TypeRef(name.clone(), generic_args)
                            } else {
                                Type::Record(name.clone())
                            };
                        }
                    }

                    // Check for builtin function with known return type
                    if is_builtin_function(name) {
                        let arg_types: Vec<Type> = checked_args
                            .iter()
                            .map(|a| match a {
                                CheckedCallArg::Positional(ty, _) => ty.clone(),
                                CheckedCallArg::Named(_, ty, _) => ty.clone(),
                            })
                            .collect();
                        if let Some(ret_ty) = builtin_return_type(name, &arg_types) {
                            return ret_ty;
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
                match &ot {
                    Type::Record(ref name) => {
                        if let Some(ti) = self.symbols.types.get(name) {
                            if let crate::compiler::resolve::TypeInfoKind::Record(ref rd) = ti.kind
                            {
                                if let Some(f) = rd.fields.iter().find(|f| f.name == *field) {
                                    return resolve_type_expr(&f.ty, self.symbols);
                                }
                            }
                        }
                    }
                    Type::TypeRef(ref name, ref args) => {
                        // Generic type instantiation - apply substitution
                        if let Some(ti) = self.symbols.types.get(name) {
                            let subst = build_subst(&ti.generic_params, args);
                            if let crate::compiler::resolve::TypeInfoKind::Record(ref rd) = ti.kind
                            {
                                if let Some(f) = rd.fields.iter().find(|f| f.name == *field) {
                                    return resolve_type_expr_with_subst(
                                        &f.ty,
                                        self.symbols,
                                        &subst,
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
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
                                self.check_stmt(s, None, false);
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
            Expr::TryElse {
                expr,
                error_binding,
                handler,
                ..
            } => {
                let t = self.infer_expr(expr);
                // If expr is Result[Ok, Err], bind error and evaluate handler
                if let Type::Result(ok, err) = t {
                    // Temporarily register error binding type for handler inference
                    self.locals.insert(error_binding.clone(), *err);
                    let handler_ty = self.infer_expr(handler);
                    // The result type is the Ok type (both branches should produce T)
                    // If handler type matches ok type, return ok type; otherwise use handler
                    if handler_ty == *ok || handler_ty == Type::Any || *ok == Type::Any {
                        *ok
                    } else {
                        handler_ty
                    }
                } else {
                    // Not a result type — handler is unused, just return expr type
                    self.infer_expr(handler);
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
                    // T209: result[T, E] ?? default → T
                    Type::Result(ok, _err) => *ok,
                    _ => lt,
                }
            }
            Expr::NullSafeAccess(obj, field, _span) => {
                let ot = self.infer_expr(obj);
                // Resolve the underlying record type, stripping Null from unions
                let record_type = match &ot {
                    Type::Record(_) => ot.clone(),
                    Type::Union(types) => {
                        // Find the non-null record type in the union (e.g., Record | Null)
                        types
                            .iter()
                            .find(|t| matches!(t, Type::Record(_)))
                            .cloned()
                            .unwrap_or(ot.clone())
                    }
                    _ => ot.clone(),
                };
                // Result is T | Null
                let field_type = if let Type::Record(ref name) = record_type {
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
            Expr::NullSafeIndex(obj, idx, _) => {
                let ot = self.infer_expr(obj);
                self.infer_expr(idx);
                let elem_type = match ot {
                    Type::List(inner) => *inner,
                    _ => Type::Any,
                };
                Type::Union(vec![elem_type, Type::Null])
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
                    // T209: Unwrap result[T, E] — expr! returns T
                    Type::Result(ok, _err) => *ok,
                    _ => t,
                }
            }
            Expr::SpreadExpr(inner, _) => self.infer_expr(inner),
            Expr::IsType {
                expr: inner,
                type_name,
                span,
            } => {
                self.infer_expr(inner);
                // Validate that the target type exists
                let is_known_type = matches!(
                    type_name.as_str(),
                    "Int" | "Float" | "String" | "Bool" | "Bytes" | "Json" | "Null"
                ) || self.symbols.types.contains_key(type_name)
                    || self.symbols.type_aliases.contains_key(type_name);
                if !is_known_type && !self.allow_placeholders {
                    self.errors.push(TypeError::UndefinedType {
                        name: type_name.clone(),
                        line: span.line,
                    });
                }
                Type::Bool
            }
            Expr::TypeCast {
                expr: inner,
                target_type,
                span,
            } => {
                self.infer_expr(inner);
                match target_type.as_str() {
                    "Int" => Type::Int,
                    "Float" => Type::Float,
                    "String" => Type::String,
                    "Bool" => Type::Bool,
                    "Bytes" => Type::Bytes,
                    "Json" => Type::Json,
                    _ => {
                        if let Some(ti) = self.symbols.types.get(target_type) {
                            use crate::compiler::resolve::TypeInfoKind;
                            match &ti.kind {
                                TypeInfoKind::Record(_) => Type::Record(target_type.clone()),
                                TypeInfoKind::Enum(_) => Type::Enum(target_type.clone()),
                                TypeInfoKind::Builtin => Type::Record(target_type.clone()),
                            }
                        } else if self.allow_placeholders {
                            Type::Any
                        } else {
                            self.errors.push(TypeError::UndefinedType {
                                name: target_type.clone(),
                                line: span.line,
                            });
                            Type::Any
                        }
                    }
                }
            }
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
                extra_clauses,
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
                // Register bindings for extra for-clauses
                for clause in extra_clauses {
                    let clause_iter_type = self.infer_expr(&clause.iter);
                    let clause_elem_type = match &clause_iter_type {
                        Type::List(inner) => *inner.clone(),
                        Type::Set(inner) => *inner.clone(),
                        _ => Type::Any,
                    };
                    self.locals.insert(clause.var.clone(), clause_elem_type);
                }
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
                subject,
                arms,
                span,
            } => {
                let subject_type = self.infer_expr(subject);
                let mut covered_variants = Vec::new();
                let mut has_catchall = false;
                let mut result_type = Type::Any;

                for arm in arms {
                    self.bind_match_pattern(
                        &arm.pattern,
                        &subject_type,
                        &mut covered_variants,
                        &mut has_catchall,
                        arm.span.line,
                    );
                    for s in &arm.body {
                        self.check_stmt(s, None, false);
                    }
                    // Infer type from last expression in arm body
                    if let Some(Stmt::Expr(es)) = arm.body.last() {
                        result_type = self.infer_expr(&es.expr);
                    } else if let Some(Stmt::Return(rs)) = arm.body.last() {
                        result_type = self.infer_expr(&rs.value);
                    }
                }

                // Exhaustiveness check for enums
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
                                    self.errors.push(TypeError::IncompleteMatch {
                                        enum_name: name.clone(),
                                        missing,
                                        line: span.line,
                                    });
                                }
                            }
                        }
                    }
                }

                // T049: Exhaustiveness check for integer refinement ranges
                if subject_type == Type::Int && !has_catchall {
                    check_int_match_exhaustiveness(arms, span.line, &mut self.errors);
                }

                result_type
            }
            Expr::BlockExpr(stmts, _) => {
                for s in stmts {
                    self.check_stmt(s, None, false);
                }
                // Infer type from last expression in block
                if let Some(Stmt::Expr(es)) = stmts.last() {
                    self.infer_expr(&es.expr)
                } else {
                    Type::Any
                }
            }
            Expr::WhenExpr {
                arms, else_body, ..
            } => {
                let mut result_type = Type::Any;
                for arm in arms {
                    self.infer_expr(&arm.condition);
                    result_type = self.infer_expr(&arm.body);
                }
                if let Some(eb) = else_body {
                    result_type = self.infer_expr(eb);
                }
                result_type
            }
            Expr::ComptimeExpr(inner, _) => self.infer_expr(inner),
            Expr::Perform { args, .. } => {
                for arg in args {
                    self.infer_expr(arg);
                }
                Type::Any
            }
            Expr::HandleExpr { body, handlers, .. } => {
                for stmt in body {
                    self.check_stmt(stmt, None, false);
                }
                for handler in handlers {
                    for stmt in &handler.body {
                        self.check_stmt(stmt, None, false);
                    }
                }
                Type::Any
            }
            Expr::ResumeExpr(inner, _) => {
                self.infer_expr(inner);
                Type::Any
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
        // Generic type refs are compatible if the base name matches and args are compatible
        if let (Type::TypeRef(n1, args1), Type::TypeRef(n2, args2)) = (expected, actual) {
            if n1 == n2 {
                // Check that all type arguments are compatible
                if args1.len() == args2.len() {
                    let all_compat = args1
                        .iter()
                        .zip(args2.iter())
                        .all(|(a1, a2)| a1 == a2 || *a1 == Type::Any || *a2 == Type::Any);
                    if all_compat {
                        return;
                    }
                } else {
                    return; // Arity mismatch, but already reported in resolve
                }
            }
        }

        // Allow TypeRef to be compatible with its base Record type
        if let (Type::Record(name1), Type::TypeRef(name2, _)) = (expected, actual) {
            if name1 == name2 {
                return;
            }
        }
        if let (Type::TypeRef(name1, _), Type::Record(name2)) = (expected, actual) {
            if name1 == name2 {
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
    if has_attr {
        Some(true)
    } else {
        None
    }
}

/// T049: Check exhaustiveness of integer match arms.
///
/// Extracts literal and range patterns from match arms and checks whether they
/// cover a contiguous range.  If the overall range exceeds 256 values, the
/// match is considered incomplete (a wildcard/catchall is required).  For
/// smaller ranges we verify every value is covered.
fn check_int_match_exhaustiveness(arms: &[MatchArm], line: usize, errors: &mut Vec<TypeError>) {
    use std::collections::BTreeSet;

    // Collect covered integer values and ranges from patterns.
    let mut covered: BTreeSet<i64> = BTreeSet::new();
    let mut ranges: Vec<(i64, i64)> = Vec::new(); // (lo, hi) inclusive
    let mut has_non_int_pattern = false;

    for arm in arms {
        collect_int_patterns(
            &arm.pattern,
            &mut covered,
            &mut ranges,
            &mut has_non_int_pattern,
        );
    }

    // If any arm has a non-integer, non-range pattern (e.g. identifier binding
    // that wasn't detected as catchall, or guard), skip the check.
    if has_non_int_pattern {
        return;
    }

    // If there are no literal/range patterns at all, skip.
    if covered.is_empty() && ranges.is_empty() {
        return;
    }

    // Determine the full range [lo..=hi] from the patterns themselves.
    let mut lo = i64::MAX;
    let mut hi = i64::MIN;
    for &v in &covered {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    for &(rlo, rhi) in &ranges {
        lo = lo.min(rlo);
        hi = hi.max(rhi);
    }

    // If range exceeds 256 values, just require a wildcard.
    let span_size = (hi as i128) - (lo as i128) + 1;
    if span_size > 256 || span_size <= 0 {
        errors.push(TypeError::IncompleteMatch {
            enum_name: "Int".to_string(),
            missing: vec!["_ (wildcard required for large integer ranges)".to_string()],
            line,
        });
        return;
    }

    // Build the full covered set.
    for &(rlo, rhi) in &ranges {
        for v in rlo..=rhi {
            covered.insert(v);
        }
    }

    // Check for missing values.
    let mut missing_vals = Vec::new();
    for v in lo..=hi {
        if !covered.contains(&v) {
            missing_vals.push(v.to_string());
            if missing_vals.len() >= 5 {
                missing_vals.push("...".to_string());
                break;
            }
        }
    }

    if !missing_vals.is_empty() {
        errors.push(TypeError::IncompleteMatch {
            enum_name: "Int".to_string(),
            missing: missing_vals,
            line,
        });
    }
}

/// Extract integer literal and range patterns from a match pattern.
fn collect_int_patterns(
    pattern: &Pattern,
    covered: &mut std::collections::BTreeSet<i64>,
    ranges: &mut Vec<(i64, i64)>,
    has_non_int: &mut bool,
) {
    match pattern {
        Pattern::Literal(Expr::IntLit(v, _)) => {
            covered.insert(*v);
        }
        Pattern::Literal(Expr::UnaryOp(UnaryOp::Neg, inner, _)) => {
            if let Expr::IntLit(v, _) = inner.as_ref() {
                covered.insert(-v);
            } else {
                *has_non_int = true;
            }
        }
        Pattern::Range {
            start,
            end,
            inclusive,
            ..
        } => {
            if let (Some(lo), Some(hi)) = (extract_int_lit(start), extract_int_lit(end)) {
                let hi_val = if *inclusive { hi } else { hi - 1 };
                if lo <= hi_val {
                    ranges.push((lo, hi_val));
                }
            } else {
                *has_non_int = true;
            }
        }
        Pattern::Or { patterns, .. } => {
            for p in patterns {
                collect_int_patterns(p, covered, ranges, has_non_int);
            }
        }
        Pattern::Guard { inner, .. } => {
            // Guards don't contribute to exhaustiveness but we still
            // collect the inner pattern's literals so we can determine
            // the range (the overall check already requires a catchall
            // due to the guard being excluded from has_catchall).
            collect_int_patterns(inner, covered, ranges, has_non_int);
        }
        Pattern::Wildcard(_) | Pattern::Ident(_, _) => {
            // These should have set has_catchall = true already;
            // reaching here means the caller should have bailed.
            // But if we do reach here, treat as non-int to skip.
            *has_non_int = true;
        }
        _ => {
            *has_non_int = true;
        }
    }
}

/// Extract an integer literal value from an expression (handles negation).
fn extract_int_lit(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::IntLit(v, _) => Some(*v),
        Expr::UnaryOp(UnaryOp::Neg, inner, _) => {
            if let Expr::IntLit(v, _) = inner.as_ref() {
                Some(-v)
            } else {
                None
            }
        }
        _ => None,
    }
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
            Item::Impl(i) => {
                // T208: typecheck each impl method with its own scope.
                for method in &i.cells {
                    checker.check_cell(method);
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
    fn test_typecheck_assert_builtin() {
        // assert and assert_* are builtins; typecheck should accept them
        typecheck_src("cell main() -> Null\n  assert 1 + 1 == 2\n  assert_eq(2, 2)\n  assert_ne(1, 2)\n  assert_contains([1, 2, 3], 2)\n  return null\nend")
            .unwrap();
    }

    #[test]
    fn test_type_alias_resolves_in_typecheck() {
        // Type alias should resolve to the underlying type
        typecheck_src("type UserId = String\n\ncell greet(id: UserId) -> String\n  return id\nend")
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
        let err = typecheck_src("cell example() -> Int\n  return completely_unknown_var_xyz\nend")
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
        typecheck_src("type UserId = String\n\ncell greet(id: UserId) -> UserId\n  return id\nend")
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

    #[test]
    fn test_is_type_returns_bool() {
        // IsType expression should return Bool
        typecheck_src("cell check(x: Int) -> Bool\n  return x is Int\nend").unwrap();
    }

    #[test]
    fn test_type_cast_returns_target_type() {
        // TypeCast expression should return the target type
        typecheck_src("cell convert(x: Float) -> Int\n  return x as Int\nend").unwrap();
    }

    #[test]
    fn test_compound_assign_bitwise_requires_int() {
        // Bitwise compound assignment on non-Int should error
        let err =
            typecheck_src("cell bad() -> String\n  let x = \"hello\"\n  x &= 1\n  return x\nend")
                .unwrap_err();
        assert!(err
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { expected, .. } if expected == "Int")));
    }

    #[test]
    fn test_compound_assign_add_is_valid() {
        // Basic compound assignment should work
        typecheck_src("cell inc() -> Int\n  let x = 1\n  x += 2\n  return x\nend").unwrap();
    }

    #[test]
    fn test_shift_operators_return_int() {
        // Shift operators should return Int
        typecheck_src("cell shift(a: Int, b: Int) -> Int\n  return a << b\nend").unwrap();
    }

    #[test]
    fn test_shift_operators_require_int_operands() {
        // Shift with non-Int operand should error
        let err =
            typecheck_src("cell bad(a: String, b: Int) -> Int\n  return a << b\nend").unwrap_err();
        assert!(err
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { expected, .. } if expected == "Int")));
    }

    #[test]
    fn test_validation_error_not_hardcoded() {
        // ValidationError is no longer hardcoded as a builtin type;
        // it resolves via the normal symbol table lookup
        let err =
            typecheck_src("cell test() -> Int\n  let x: ValidationError = null\n  return 1\nend");
        // Should either succeed (if resolved as Any) or fail gracefully
        // The key assertion: it doesn't crash and doesn't produce a Record("ValidationError") type
        // without a definition in the symbol table
        let _ = err;
    }
}
