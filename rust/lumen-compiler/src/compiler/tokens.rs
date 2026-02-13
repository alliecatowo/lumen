use serde::{Deserialize, Serialize};
use std::fmt;

/// Source location in the original `.lm.md` file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    /// Byte offset of the start in the source
    pub start: usize,
    /// Byte offset of the end (exclusive) in the source
    pub end: usize,
    /// 1-based line number
    pub line: usize,
    /// 1-based column number
    pub col: usize,
}

impl Span {
    pub fn new(start: usize, end: usize, line: usize, col: usize) -> Self {
        Self { start, end, line, col }
    }

    pub fn dummy() -> Self {
        Self { start: 0, end: 0, line: 0, col: 0 }
    }

    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
            line: self.line.min(other.line),
            col: if self.line <= other.line { self.col } else { other.col },
        }
    }
}

/// Token types for the Lumen language
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TokenKind {
    // Literals
    IntLit(i64),
    FloatLit(f64),
    StringLit(String),
    BoolLit(bool),

    // Identifiers and keywords
    Ident(String),

    // Keywords
    Record,
    Enum,
    Cell,
    Let,
    If,
    Else,
    For,
    In,
    Match,
    Return,
    Halt,
    End,
    Use,
    Tool,
    As,
    Grant,
    Expect,
    Schema,
    Role,
    Where,
    And,
    Or,
    Not,
    Null,
    Result,
    Ok_,
    Err_,
    List,
    Map,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eq,       // ==
    NotEq,    // !=
    Lt,       // <
    LtEq,    // <=
    Gt,       // >
    GtEq,    // >=
    Assign,   // =
    Arrow,    // ->
    Dot,
    Comma,
    Colon,
    Pipe,     // |

    // Delimiters
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,

    // Indentation
    Indent,
    Dedent,
    Newline,

    // Special
    Eof,

    // Directives (parsed at top-level)
    Directive(String), // e.g. @lumen, @package, etc.
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::IntLit(n) => write!(f, "{}", n),
            TokenKind::FloatLit(n) => write!(f, "{}", n),
            TokenKind::StringLit(s) => write!(f, "\"{}\"", s),
            TokenKind::BoolLit(b) => write!(f, "{}", b),
            TokenKind::Ident(s) => write!(f, "{}", s),
            TokenKind::Record => write!(f, "record"),
            TokenKind::Enum => write!(f, "enum"),
            TokenKind::Cell => write!(f, "cell"),
            TokenKind::Let => write!(f, "let"),
            TokenKind::If => write!(f, "if"),
            TokenKind::Else => write!(f, "else"),
            TokenKind::For => write!(f, "for"),
            TokenKind::In => write!(f, "in"),
            TokenKind::Match => write!(f, "match"),
            TokenKind::Return => write!(f, "return"),
            TokenKind::Halt => write!(f, "halt"),
            TokenKind::End => write!(f, "end"),
            TokenKind::Use => write!(f, "use"),
            TokenKind::Tool => write!(f, "tool"),
            TokenKind::As => write!(f, "as"),
            TokenKind::Grant => write!(f, "grant"),
            TokenKind::Expect => write!(f, "expect"),
            TokenKind::Schema => write!(f, "schema"),
            TokenKind::Role => write!(f, "role"),
            TokenKind::Where => write!(f, "where"),
            TokenKind::And => write!(f, "and"),
            TokenKind::Or => write!(f, "or"),
            TokenKind::Not => write!(f, "not"),
            TokenKind::Null => write!(f, "Null"),
            TokenKind::Result => write!(f, "result"),
            TokenKind::Ok_ => write!(f, "ok"),
            TokenKind::Err_ => write!(f, "err"),
            TokenKind::List => write!(f, "list"),
            TokenKind::Map => write!(f, "map"),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Star => write!(f, "*"),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::Percent => write!(f, "%"),
            TokenKind::Eq => write!(f, "=="),
            TokenKind::NotEq => write!(f, "!="),
            TokenKind::Lt => write!(f, "<"),
            TokenKind::LtEq => write!(f, "<="),
            TokenKind::Gt => write!(f, ">"),
            TokenKind::GtEq => write!(f, ">="),
            TokenKind::Assign => write!(f, "="),
            TokenKind::Arrow => write!(f, "->"),
            TokenKind::Dot => write!(f, "."),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::LParen => write!(f, "("),
            TokenKind::RParen => write!(f, ")"),
            TokenKind::LBracket => write!(f, "["),
            TokenKind::RBracket => write!(f, "]"),
            TokenKind::LBrace => write!(f, "{{"),
            TokenKind::RBrace => write!(f, "}}"),
            TokenKind::Indent => write!(f, "INDENT"),
            TokenKind::Dedent => write!(f, "DEDENT"),
            TokenKind::Newline => write!(f, "NEWLINE"),
            TokenKind::Eof => write!(f, "EOF"),
            TokenKind::Directive(s) => write!(f, "@{}", s),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}
