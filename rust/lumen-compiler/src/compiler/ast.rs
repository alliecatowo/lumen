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
    Agent(AgentDecl),
    Addon(AddonDecl),
    UseTool(UseToolDecl),
    Grant(GrantDecl),
    TypeAlias(TypeAliasDef),
    Trait(TraitDef),
    Impl(ImplDef),
    Import(ImportDecl),
    ConstDecl(ConstDeclDef),
    MacroDecl(MacroDeclDef),
}

impl Item {
    pub fn span(&self) -> Span {
        match self {
            Item::Record(r) => r.span,
            Item::Enum(e) => e.span,
            Item::Cell(c) => c.span,
            Item::Agent(a) => a.span,
            Item::Addon(a) => a.span,
            Item::UseTool(u) => u.span,
            Item::Grant(g) => g.span,
            Item::TypeAlias(t) => t.span,
            Item::Trait(t) => t.span,
            Item::Impl(i) => i.span,
            Item::Import(i) => i.span,
            Item::ConstDecl(c) => c.span,
            Item::MacroDecl(m) => m.span,
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
    /// Tuple type: (A, B, C)
    Tuple(Vec<TypeExpr>, Span),
    /// Set type: set[T]
    Set(Box<TypeExpr>, Span),
    /// Function type: fn(A, B) -> C / {effects}
    Fn(Vec<TypeExpr>, Box<TypeExpr>, Vec<String>, Span),
    /// Generic type: Name[T, U]
    Generic(String, Vec<TypeExpr>, Span),
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
            TypeExpr::Tuple(_, s) => *s,
            TypeExpr::Set(_, s) => *s,
            TypeExpr::Fn(_, _, _, s) => *s,
            TypeExpr::Generic(_, _, s) => *s,
        }
    }
}

// ── Generic parameters ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericParam {
    pub name: String,
    pub bounds: Vec<String>,
    pub span: Span,
}

// ── Records ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordDef {
    pub name: String,
    pub generic_params: Vec<GenericParam>,
    pub fields: Vec<FieldDef>,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    pub name: String,
    pub ty: TypeExpr,
    pub default_value: Option<Expr>,
    pub constraint: Option<Expr>,
    pub span: Span,
}

// ── Enums ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumDef {
    pub name: String,
    pub generic_params: Vec<GenericParam>,
    pub variants: Vec<EnumVariant>,
    pub methods: Vec<CellDef>,
    pub is_pub: bool,
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
    pub generic_params: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub effects: Vec<String>,
    pub body: Vec<Stmt>,
    pub is_pub: bool,
    pub is_async: bool,
    pub where_clauses: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDecl {
    pub name: String,
    pub cells: Vec<CellDef>,
    pub grants: Vec<GrantDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonDecl {
    pub kind: String,
    pub name: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
    pub default_value: Option<Expr>,
    pub span: Span,
}

// ── Type aliases, traits, impls, imports ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeAliasDef {
    pub name: String,
    pub generic_params: Vec<GenericParam>,
    pub type_expr: TypeExpr,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitDef {
    pub name: String,
    pub parent_traits: Vec<String>,
    pub methods: Vec<CellDef>,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplDef {
    pub trait_name: String,
    pub generic_params: Vec<GenericParam>,
    pub target_type: String,
    pub cells: Vec<CellDef>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImportList {
    Names(Vec<ImportName>),
    Wildcard,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportName {
    pub name: String,
    pub alias: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportDecl {
    pub path: Vec<String>,
    pub names: ImportList,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstDeclDef {
    pub name: String,
    pub type_ann: Option<TypeExpr>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroDeclDef {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

// ── Statements ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompoundOp {
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
}

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
    While(WhileStmt),
    Loop(LoopStmt),
    Break(BreakStmt),
    Continue(ContinueStmt),
    Emit(EmitStmt),
    CompoundAssign(CompoundAssignStmt),
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
            Stmt::While(s) => s.span,
            Stmt::Loop(s) => s.span,
            Stmt::Break(s) => s.span,
            Stmt::Continue(s) => s.span,
            Stmt::Emit(s) => s.span,
            Stmt::CompoundAssign(s) => s.span,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LetStmt {
    pub name: String,
    pub mutable: bool,
    pub pattern: Option<Pattern>,
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
    /// Guard: pattern if condition
    Guard {
        inner: Box<Pattern>,
        condition: Box<Expr>,
        span: Span,
    },
    /// Or: pattern1 | pattern2
    Or { patterns: Vec<Pattern>, span: Span },
    /// List destructure: [a, b, ...rest]
    ListDestructure {
        elements: Vec<Pattern>,
        rest: Option<String>,
        span: Span,
    },
    /// Tuple destructure: (a, b, c)
    TupleDestructure { elements: Vec<Pattern>, span: Span },
    /// Record destructure: TypeName(field1:, field2: pat, ..)
    RecordDestructure {
        type_name: String,
        fields: Vec<(String, Option<Pattern>)>,
        open: bool,
        span: Span,
    },
    /// Type check: name: Type
    TypeCheck {
        name: String,
        type_expr: Box<TypeExpr>,
        span: Span,
    },
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhileStmt {
    pub condition: Expr,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopStmt {
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakStmt {
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinueStmt {
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmitStmt {
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompoundAssignStmt {
    pub target: String,
    pub op: CompoundOp,
    pub value: Expr,
    pub span: Span,
}

// ── Expressions ──

/// Lambda body can be a single expression or a block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LambdaBody {
    Expr(Box<Expr>),
    Block(Vec<Stmt>),
}

/// Comprehension kinds
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComprehensionKind {
    List,
    Map,
    Set,
}

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
    /// Raw string literal
    RawStringLit(String, Span),
    /// Bytes literal
    BytesLit(Vec<u8>, Span),
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
    /// Lambda: fn(params) -> type => expr | fn(params) block end
    Lambda {
        params: Vec<Param>,
        return_type: Option<Box<TypeExpr>>,
        body: LambdaBody,
        span: Span,
    },
    /// Tuple literal: (a, b, c)
    TupleLit(Vec<Expr>, Span),
    /// Set literal: set[a, b, c]
    SetLit(Vec<Expr>, Span),
    /// Range expression: start..end or start..=end
    RangeExpr {
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
        inclusive: bool,
        step: Option<Box<Expr>>,
        span: Span,
    },
    /// Postfix try: expr?
    TryExpr(Box<Expr>, Span),
    /// Null coalescing: lhs ?? rhs
    NullCoalesce(Box<Expr>, Box<Expr>, Span),
    /// Null-safe access: expr?.field
    NullSafeAccess(Box<Expr>, String, Span),
    /// Null assert: expr!
    NullAssert(Box<Expr>, Span),
    /// Spread: ...expr
    SpreadExpr(Box<Expr>, Span),
    /// If expression: if cond then a else b
    IfExpr {
        cond: Box<Expr>,
        then_val: Box<Expr>,
        else_val: Box<Expr>,
        span: Span,
    },
    /// Await expression: await expr
    AwaitExpr(Box<Expr>, Span),
    /// Comprehension: [expr for pat in iter if cond]
    Comprehension {
        body: Box<Expr>,
        var: String,
        iter: Box<Expr>,
        condition: Option<Box<Expr>>,
        kind: ComprehensionKind,
        span: Span,
    },
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
            | Expr::RawStringLit(_, s)
            | Expr::BytesLit(_, s)
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
            | Expr::ExpectSchema(_, _, s)
            | Expr::TupleLit(_, s)
            | Expr::SetLit(_, s)
            | Expr::TryExpr(_, s)
            | Expr::NullCoalesce(_, _, s)
            | Expr::NullSafeAccess(_, _, s)
            | Expr::NullAssert(_, s)
            | Expr::SpreadExpr(_, s)
            | Expr::AwaitExpr(_, s) => *s,
            Expr::Lambda { span, .. } => *span,
            Expr::RangeExpr { span, .. } => *span,
            Expr::IfExpr { span, .. } => *span,
            Expr::Comprehension { span, .. } => *span,
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
    Pow,
    PipeForward,
    Concat,
    In,
    BitAnd,
    BitOr,
    BitXor,
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
            BinOp::Pow => write!(f, "**"),
            BinOp::PipeForward => write!(f, "|>"),
            BinOp::Concat => write!(f, "++"),
            BinOp::In => write!(f, "in"),
            BinOp::BitAnd => write!(f, "&"),
            BinOp::BitOr => write!(f, "|"),
            BinOp::BitXor => write!(f, "^"),
        }
    }
}

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
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
