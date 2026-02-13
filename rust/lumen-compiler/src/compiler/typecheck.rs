//! Bidirectional type inference and checking for Lumen.

use crate::compiler::ast::*;
use crate::compiler::resolve::SymbolTable;
use crate::compiler::tokens::Span;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TypeError {
    #[error("type mismatch at line {line}: expected {expected}, got {actual}")]
    Mismatch { expected: String, actual: String, line: usize },
    #[error("undefined variable '{name}' at line {line}")]
    UndefinedVar { name: String, line: usize },
    #[error("not callable at line {line}")]
    NotCallable { line: usize },
    #[error("wrong number of arguments at line {line}: expected {expected}, got {actual}")]
    ArgCount { expected: usize, actual: usize, line: usize },
    #[error("unknown field '{field}' on type '{ty}' at line {line}")]
    UnknownField { field: String, ty: String, line: usize },
    #[error("missing return in cell '{name}' at line {line}")]
    MissingReturn { name: String, line: usize },
}

/// Resolved type representation
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    String, Int, Float, Bool, Bytes, Json, Null,
    List(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Record(String),
    Enum(String),
    Result(Box<Type>, Box<Type>),
    Union(Vec<Type>),
    Any, // For unresolved / error recovery
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::String => write!(f, "String"), Type::Int => write!(f, "Int"),
            Type::Float => write!(f, "Float"), Type::Bool => write!(f, "Bool"),
            Type::Bytes => write!(f, "Bytes"), Type::Json => write!(f, "Json"),
            Type::Null => write!(f, "Null"), Type::Any => write!(f, "Any"),
            Type::List(t) => write!(f, "list[{}]", t),
            Type::Map(k, v) => write!(f, "map[{}, {}]", k, v),
            Type::Record(n) => write!(f, "{}", n),
            Type::Enum(n) => write!(f, "{}", n),
            Type::Result(o, e) => write!(f, "result[{}, {}]", o, e),
            Type::Union(ts) => {
                let parts: Vec<_> = ts.iter().map(|t| format!("{}", t)).collect();
                write!(f, "{}", parts.join(" | "))
            }
        }
    }
}

pub fn resolve_type_expr(ty: &TypeExpr, symbols: &SymbolTable) -> Type {
    match ty {
        TypeExpr::Named(name, _) => match name.as_str() {
            "String" => Type::String, "Int" => Type::Int, "Float" => Type::Float,
            "Bool" => Type::Bool, "Bytes" => Type::Bytes, "Json" => Type::Json,
            "ValidationError" => Type::Record("ValidationError".into()),
            _ => {
                if symbols.types.contains_key(name) {
                    use crate::compiler::resolve::TypeInfoKind;
                    match &symbols.types[name].kind {
                        TypeInfoKind::Record(_) => Type::Record(name.clone()),
                        TypeInfoKind::Enum(_) => Type::Enum(name.clone()),
                        TypeInfoKind::Builtin => Type::Record(name.clone()),
                    }
                } else { Type::Any }
            }
        },
        TypeExpr::List(inner, _) => Type::List(Box::new(resolve_type_expr(inner, symbols))),
        TypeExpr::Map(k, v, _) => Type::Map(Box::new(resolve_type_expr(k, symbols)), Box::new(resolve_type_expr(v, symbols))),
        TypeExpr::Result(ok, err, _) => Type::Result(Box::new(resolve_type_expr(ok, symbols)), Box::new(resolve_type_expr(err, symbols))),
        TypeExpr::Union(types, _) => Type::Union(types.iter().map(|t| resolve_type_expr(t, symbols)).collect()),
        TypeExpr::Null(_) => Type::Null,
    }
}

struct TypeChecker<'a> {
    symbols: &'a SymbolTable,
    locals: HashMap<String, Type>,
    errors: Vec<TypeError>,
}

impl<'a> TypeChecker<'a> {
    fn new(symbols: &'a SymbolTable) -> Self {
        Self { symbols, locals: HashMap::new(), errors: Vec::new() }
    }

    fn check_cell(&mut self, cell: &CellDef) {
        self.locals.clear();
        for p in &cell.params {
            let ty = resolve_type_expr(&p.ty, self.symbols);
            self.locals.insert(p.name.clone(), ty);
        }
        for stmt in &cell.body {
            self.check_stmt(stmt);
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let(ls) => {
                let val_type = self.infer_expr(&ls.value);
                if let Some(ref ann) = ls.ty {
                    let expected = resolve_type_expr(ann, self.symbols);
                    self.check_compat(&expected, &val_type, ls.span.line);
                }
                self.locals.insert(ls.name.clone(), val_type);
            }
            Stmt::If(ifs) => {
                let ct = self.infer_expr(&ifs.condition);
                self.check_compat(&Type::Bool, &ct, ifs.span.line);
                for s in &ifs.then_body { self.check_stmt(s); }
                if let Some(ref eb) = ifs.else_body { for s in eb { self.check_stmt(s); } }
            }
            Stmt::For(fs) => {
                let iter_type = self.infer_expr(&fs.iter);
                let elem_type = match &iter_type {
                    Type::List(inner) => *inner.clone(),
                    _ => { self.errors.push(TypeError::Mismatch {
                        expected: "list[T]".into(), actual: format!("{}", iter_type), line: fs.span.line,
                    }); Type::Any }
                };
                self.locals.insert(fs.var.clone(), elem_type);
                for s in &fs.body { self.check_stmt(s); }
            }
            Stmt::Match(ms) => {
                let _sub_type = self.infer_expr(&ms.subject);
                for arm in &ms.arms {
                    // Bind pattern variables
                    match &arm.pattern {
                        Pattern::Variant(_, Some(binding), _) => { self.locals.insert(binding.clone(), Type::Any); }
                        Pattern::Ident(name, _) => { self.locals.insert(name.clone(), Type::Any); }
                        _ => {}
                    }
                    for s in &arm.body { self.check_stmt(s); }
                }
            }
            Stmt::Return(rs) => { self.infer_expr(&rs.value); }
            Stmt::Halt(hs) => { self.infer_expr(&hs.message); }
            Stmt::Expr(es) => { self.infer_expr(&es.expr); }
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
                if let Some(ty) = self.locals.get(name) { ty.clone() }
                else if self.symbols.cells.contains_key(name) { Type::Any } // cell ref
                else if self.symbols.tools.contains_key(name) { Type::Any } // tool ref
                else {
                    self.errors.push(TypeError::UndefinedVar { name: name.clone(), line: span.line });
                    Type::Any
                }
            }
            Expr::ListLit(elems, _) => {
                if elems.is_empty() { Type::List(Box::new(Type::Any)) }
                else {
                    let first = self.infer_expr(&elems[0]);
                    for e in &elems[1..] { self.infer_expr(e); }
                    Type::List(Box::new(first))
                }
            }
            Expr::MapLit(pairs, _) => {
                if pairs.is_empty() { Type::Map(Box::new(Type::String), Box::new(Type::Any)) }
                else {
                    let kt = self.infer_expr(&pairs[0].0);
                    let vt = self.infer_expr(&pairs[0].1);
                    for (k, v) in &pairs[1..] { self.infer_expr(k); self.infer_expr(v); }
                    Type::Map(Box::new(kt), Box::new(vt))
                }
            }
            Expr::RecordLit(name, fields, _) => {
                for (_, val) in fields { self.infer_expr(val); }
                Type::Record(name.clone())
            }
            Expr::BinOp(lhs, op, rhs, span) => {
                let lt = self.infer_expr(lhs);
                let rt = self.infer_expr(rhs);
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                        if lt == Type::String && *op == BinOp::Add { Type::String }
                        else if lt == Type::Float || rt == Type::Float { Type::Float }
                        else { Type::Int }
                    }
                    BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq => Type::Bool,
                    BinOp::And | BinOp::Or => Type::Bool,
                }
            }
            Expr::UnaryOp(op, inner, _) => {
                let t = self.infer_expr(inner);
                match op {
                    UnaryOp::Neg => t,
                    UnaryOp::Not => Type::Bool,
                }
            }
            Expr::Call(callee, args, span) => {
                for arg in args {
                    match arg {
                        CallArg::Positional(e) => { self.infer_expr(e); }
                        CallArg::Named(_, e, _) => { self.infer_expr(e); }
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
                    match arg { CallArg::Positional(e) | CallArg::Named(_, e, _) => { self.infer_expr(e); } _ => {} }
                }
                Type::Any
            }
            Expr::DotAccess(obj, field, span) => {
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
            Expr::RoleBlock(_, _, _) => Type::String,
            Expr::ExpectSchema(inner, schema_name, _) => {
                self.infer_expr(inner);
                if self.symbols.types.contains_key(schema_name) {
                    Type::Record(schema_name.clone())
                } else { Type::Any }
            }
        }
    }

    fn check_compat(&mut self, expected: &Type, actual: &Type, line: usize) {
        if *expected == Type::Any || *actual == Type::Any { return; }
        if expected != actual {
            self.errors.push(TypeError::Mismatch {
                expected: format!("{}", expected), actual: format!("{}", actual), line,
            });
        }
    }
}

/// Typecheck a program.
pub fn typecheck(program: &Program, symbols: &SymbolTable) -> Result<(), Vec<TypeError>> {
    let mut checker = TypeChecker::new(symbols);
    for item in &program.items {
        if let Item::Cell(c) = item {
            checker.check_cell(c);
        }
    }
    if checker.errors.is_empty() { Ok(()) } else { Err(checker.errors) }
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
        let err = typecheck_src("cell bad() -> Int\n  return x\nend").unwrap_err();
        assert!(!err.is_empty());
    }
}
