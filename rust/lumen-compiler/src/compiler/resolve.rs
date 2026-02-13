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
}

/// Symbol table built during resolution
#[derive(Debug, Clone)]
pub struct SymbolTable {
    pub types: HashMap<String, TypeInfo>,
    pub cells: HashMap<String, CellInfo>,
    pub tools: HashMap<String, ToolInfo>,
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

impl SymbolTable {
    pub fn new() -> Self {
        let mut types = HashMap::new();
        // Register builtin types
        for name in &["String", "Int", "Float", "Bool", "Bytes", "Json", "ValidationError"] {
            types.insert(name.to_string(), TypeInfo { kind: TypeInfoKind::Builtin });
        }
        Self { types, cells: HashMap::new(), tools: HashMap::new() }
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
                if table.types.contains_key(&r.name) {
                    errors.push(ResolveError::Duplicate { name: r.name.clone(), line: r.span.line });
                } else {
                    table.types.insert(r.name.clone(), TypeInfo { kind: TypeInfoKind::Record(r.clone()) });
                }
            }
            Item::Enum(e) => {
                if table.types.contains_key(&e.name) {
                    errors.push(ResolveError::Duplicate { name: e.name.clone(), line: e.span.line });
                } else {
                    table.types.insert(e.name.clone(), TypeInfo { kind: TypeInfoKind::Enum(e.clone()) });
                }
            }
            Item::Cell(c) => {
                if table.cells.contains_key(&c.name) {
                    errors.push(ResolveError::Duplicate { name: c.name.clone(), line: c.span.line });
                } else {
                    table.cells.insert(c.name.clone(), CellInfo {
                        params: c.params.iter().map(|p| (p.name.clone(), p.ty.clone())).collect(),
                        return_type: c.return_type.clone(),
                    });
                }
            }
            Item::UseTool(u) => {
                table.tools.insert(u.alias.clone(), ToolInfo {
                    tool_path: u.tool_path.clone(), mcp_url: u.mcp_url.clone(),
                });
            }
            Item::Grant(_) => {} // Grants reference tools, checked below
        }
    }

    // Second pass: verify all type references exist
    for item in &program.items {
        match item {
            Item::Record(r) => {
                for field in &r.fields {
                    check_type_refs(&field.ty, &table, &mut errors);
                }
            }
            Item::Cell(c) => {
                for p in &c.params { check_type_refs(&p.ty, &table, &mut errors); }
                if let Some(ref rt) = c.return_type { check_type_refs(rt, &table, &mut errors); }
            }
            Item::Grant(g) => {
                if !table.tools.contains_key(&g.tool_alias) {
                    errors.push(ResolveError::UndefinedTool { name: g.tool_alias.clone(), line: g.span.line });
                }
            }
            _ => {}
        }
    }

    if errors.is_empty() { Ok(table) } else { Err(errors) }
}

fn check_type_refs(ty: &TypeExpr, table: &SymbolTable, errors: &mut Vec<ResolveError>) {
    match ty {
        TypeExpr::Named(name, span) => {
            if !table.types.contains_key(name) {
                errors.push(ResolveError::UndefinedType { name: name.clone(), line: span.line });
            }
        }
        TypeExpr::List(inner, _) => check_type_refs(inner, table, errors),
        TypeExpr::Map(k, v, _) => { check_type_refs(k, table, errors); check_type_refs(v, table, errors); }
        TypeExpr::Result(ok, err, _) => { check_type_refs(ok, table, errors); check_type_refs(err, table, errors); }
        TypeExpr::Union(types, _) => { for t in types { check_type_refs(t, table, errors); } }
        TypeExpr::Null(_) => {}
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
        let table = resolve_src("record Foo\n  x: Int\nend\n\ncell main() -> Foo\n  return Foo(x: 1)\nend").unwrap();
        assert!(table.types.contains_key("Foo"));
        assert!(table.cells.contains_key("main"));
    }

    #[test]
    fn test_resolve_undefined_type() {
        let err = resolve_src("record Bar\n  x: Unknown\nend").unwrap_err();
        assert!(!err.is_empty());
    }
}
