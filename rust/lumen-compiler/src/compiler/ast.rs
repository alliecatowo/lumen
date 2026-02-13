use crate::compiler::tokens::Span;
use serde::{Deserialize, Serialize};

/// A complete Lumen program (one `.lm.md` file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    pub directives: Vec<Directive>,
    pub items: Vec<Item>,
    pub span: Span,
}

/// Top-level directive (@lumen, @package, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Directive {
    pub name: String,
    pub value: Option<String>,
    pub span: Span,
}

/// Top-level items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Item {
    Record(RecordDef),
    Enum(EnumDef),
    Cell(CellDef),
    UseTool(UseToolDecl),
    Grant(GrantDecl),
}

impl Item {
    pub fn span(&self) -> Span {
        match self {
            Item::Record(r) => r.span,
            Item::Enum(e) => e.span,
            Item::Cell(c) => c.span,
            Item::UseTool(u) => u.span,
            Item::Grant(g) => g.span,
        }
    }
}

// ── Type System ──

/// A type expression
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TypeExpr {
    /// Named type: String, Int, Float, Bool, Bytes, Json, or user-defined
    Named(String, Span),
    /// list[T]
    List(Box<TypeExpr>, Span),
    /// map[String, T]
    Map(Box<TypeExpr>, Box<TypeExpr>, Span),
    /// result[Ok, Err]
    Result(Box<TypeExpr>, Box<TypeExpr>, Span),
    /// Union: A | B | C
    Union(Vec<TypeExpr>, Span),
    /// Null type
    Null(Span),
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Named(_, s) => *s,
            TypeExpr::List(_, s) => *s,
            TypeExpr::Map(_, _, s) => *s,
            TypeExpr::Result(_, _, s) => *s,
            TypeExpr::Union(_, s) => *s,
            TypeExpr::Null(s) => *s,
        }
    }
}

// ── Records ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordDef {
    pub name: String,
    pub fields: Vec<FieldDef>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    pub name: String,
    pub ty: TypeExpr,
    pub constraint: Option<Expr>,
    pub span: Span,
}

// ── Enums ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumVariant {
    pub name: String,
    pub payload: Option<TypeExpr>,
    pub span: Span,
}

// ── Cells (functions) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellDef {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
}

// ── Statements ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Stmt {
    Let(LetStmt),
    If(IfStmt),
    For(ForStmt),
    Match(MatchStmt),
    Return(ReturnStmt),
    Halt(HaltStmt),
    Assign(AssignStmt),
    Expr(ExprStmt),
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::Let(s) => s.span,
            Stmt::If(s) => s.span,
            Stmt::For(s) => s.span,
            Stmt::Match(s) => s.span,
            Stmt::Return(s) => s.span,
            Stmt::Halt(s) => s.span,
            Stmt::Assign(s) => s.span,
            Stmt::Expr(s) => s.span,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LetStmt {
    pub name: String,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfStmt {
    pub condition: Expr,
    pub then_body: Vec<Stmt>,
    pub else_body: Option<Vec<Stmt>>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForStmt {
    pub var: String,
    pub iter: Expr,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchStmt {
    pub subject: Expr,
    pub arms: Vec<MatchArm>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Pattern {
    /// Literal pattern: 200, "hello", true
    Literal(Expr),
    /// Variant with optional binding: ok(value), err(e)
    Variant(String, Option<String>, Span),
    /// Wildcard: _
    Wildcard(Span),
    /// Ident binding
    Ident(String, Span),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReturnStmt {
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HaltStmt {
    pub message: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExprStmt {
    pub expr: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignStmt {
    pub target: String,
    pub value: Expr,
    pub span: Span,
}

// ── Expressions ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expr {
    /// Integer literal
    IntLit(i64, Span),
    /// Float literal
    FloatLit(f64, Span),
    /// String literal (may contain interpolation)
    StringLit(String, Span),
    /// Interpolated string with segments
    StringInterp(Vec<StringSegment>, Span),
    /// Boolean literal
    BoolLit(bool, Span),
    /// Null literal
    NullLit(Span),
    /// Variable reference
    Ident(String, Span),
    /// List literal: [a, b, c]
    ListLit(Vec<Expr>, Span),
    /// Map literal: {"key": value, ...}
    MapLit(Vec<(Expr, Expr)>, Span),
    /// Record literal: TypeName(field1: val1, field2: val2)
    RecordLit(String, Vec<(String, Expr)>, Span),
    /// Binary operation
    BinOp(Box<Expr>, BinOp, Box<Expr>, Span),
    /// Unary operation
    UnaryOp(UnaryOp, Box<Expr>, Span),
    /// Function/cell call: name(args)
    Call(Box<Expr>, Vec<CallArg>, Span),
    /// Tool call with role blocks
    ToolCall(Box<Expr>, Vec<CallArg>, Span),
    /// Dot access: expr.field
    DotAccess(Box<Expr>, String, Span),
    /// Index access: expr[index]
    IndexAccess(Box<Expr>, Box<Expr>, Span),
    /// Role block: role system: ... end
    RoleBlock(String, Box<Expr>, Span),
    /// expect schema Type
    ExpectSchema(Box<Expr>, String, Span),
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::IntLit(_, s)
            | Expr::FloatLit(_, s)
            | Expr::StringLit(_, s)
            | Expr::StringInterp(_, s)
            | Expr::BoolLit(_, s)
            | Expr::NullLit(s)
            | Expr::Ident(_, s)
            | Expr::ListLit(_, s)
            | Expr::MapLit(_, s)
            | Expr::RecordLit(_, _, s)
            | Expr::BinOp(_, _, _, s)
            | Expr::UnaryOp(_, _, s)
            | Expr::Call(_, _, s)
            | Expr::ToolCall(_, _, s)
            | Expr::DotAccess(_, _, s)
            | Expr::IndexAccess(_, _, s)
            | Expr::RoleBlock(_, _, s)
            | Expr::ExpectSchema(_, _, s) => *s,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StringSegment {
    Literal(String),
    Interpolation(Box<Expr>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CallArg {
    Positional(Expr),
    Named(String, Expr, Span),
    Role(String, Expr, Span),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinOp::Add => write!(f, "+"),
            BinOp::Sub => write!(f, "-"),
            BinOp::Mul => write!(f, "*"),
            BinOp::Div => write!(f, "/"),
            BinOp::Mod => write!(f, "%"),
            BinOp::Eq => write!(f, "=="),
            BinOp::NotEq => write!(f, "!="),
            BinOp::Lt => write!(f, "<"),
            BinOp::LtEq => write!(f, "<="),
            BinOp::Gt => write!(f, ">"),
            BinOp::GtEq => write!(f, ">="),
            BinOp::And => write!(f, "and"),
            BinOp::Or => write!(f, "or"),
        }
    }
}

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    Neg,
    Not,
}

// ── Tool Declarations ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UseToolDecl {
    pub tool_path: String,
    pub alias: String,
    pub mcp_url: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrantDecl {
    pub tool_alias: String,
    pub constraints: Vec<GrantConstraint>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrantConstraint {
    pub key: String,
    pub value: Expr,
    pub span: Span,
}
