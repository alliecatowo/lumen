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
        Self {
            start,
            end,
            line,
            col,
        }
    }

    pub fn dummy() -> Self {
        Self {
            start: 0,
            end: 0,
            line: 0,
            col: 0,
        }
    }

    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
            line: self.line.min(other.line),
            col: if self.line <= other.line {
                self.col
            } else {
                other.col
            },
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
    /// Interpolated string segments: (is_expr, text). is_expr=true means {expr}, false means literal text.
    StringInterpLit(Vec<(bool, String)>),
    BoolLit(bool),
    /// Raw string literal (no escapes, no interpolation)
    RawStringLit(String),
    /// Bytes literal: b"HEXHEX..."
    BytesLit(Vec<u8>),
    /// Null literal
    NullLit,

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
    // New keywords
    While,
    Loop,
    Break,
    Continue,
    Mut,
    Const,
    Pub,
    Import,
    From,
    Async,
    Await,
    Parallel,
    Fn,
    Trait,
    Impl,
    Type,
    Set,
    Tuple,
    Emit,
    Yield,
    Mod,
    SelfKw,
    With,
    Try,
    Union,
    Step,
    Comptime,
    Macro,
    Extern,
    Then,
    When,
    Is,
    // Existing type keywords used in SPEC
    Bool,
    Int_,
    Float_,
    String_,
    Bytes,
    Json,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eq,     // ==
    NotEq,  // !=
    Lt,     // <
    LtEq,   // <=
    Gt,     // >
    GtEq,   // >=
    Assign, // =
    Arrow,  // ->
    Dot,
    Comma,
    Colon,
    Semicolon, // ;
    Pipe,      // |
    At,        // @
    Hash,      // #
    // Compound assignments
    PlusAssign,    // +=
    MinusAssign,   // -=
    StarAssign,    // *=
    SlashAssign,   // /=
    PercentAssign, // %=
    StarStarAssign, // **=
    AmpAssign,     // &=
    PipeAssign,    // |=
    CaretAssign,   // ^=
    // New operators
    StarStar,         // ** exponentiation
    DotDot,           // .. exclusive range
    DotDotEq,         // ..= inclusive range
    PipeForward,      // |>
    Compose,          // >> function composition
    QuestionQuestion, // ?? null-coalescing
    QuestionDot,      // ?. null-safe member access
    Bang,             // ! standalone
    Question,         // ? standalone postfix try
    DotDotDot,        // ... spread/rest
    FatArrow,         // =>
    PlusPlus,         // ++ concatenation
    Ampersand,        // & bitwise and / schema intersection
    Tilde,            // ~ bitwise not
    TildeArrow,       // ~> illuminate operator
    Caret,            // ^ bitwise xor
    FloorDiv,         // // floor division
    FloorDivAssign,   // //= floor division assignment
    QuestionBracket,  // ?[ null-safe index

    // Delimiters
    Symbol(char),
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
            TokenKind::StringInterpLit(_) => write!(f, "string-interp"),
            TokenKind::BoolLit(b) => write!(f, "{}", b),
            TokenKind::RawStringLit(s) => write!(f, "r\"{}\"", s),
            TokenKind::BytesLit(_) => write!(f, "bytes-lit"),
            TokenKind::NullLit => write!(f, "null"),
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
            // New keywords
            TokenKind::While => write!(f, "while"),
            TokenKind::Loop => write!(f, "loop"),
            TokenKind::Break => write!(f, "break"),
            TokenKind::Continue => write!(f, "continue"),
            TokenKind::Mut => write!(f, "mut"),
            TokenKind::Const => write!(f, "const"),
            TokenKind::Pub => write!(f, "pub"),
            TokenKind::Import => write!(f, "import"),
            TokenKind::From => write!(f, "from"),
            TokenKind::Async => write!(f, "async"),
            TokenKind::Await => write!(f, "await"),
            TokenKind::Parallel => write!(f, "parallel"),
            TokenKind::Fn => write!(f, "fn"),
            TokenKind::Trait => write!(f, "trait"),
            TokenKind::Impl => write!(f, "impl"),
            TokenKind::Type => write!(f, "type"),
            TokenKind::Set => write!(f, "set"),
            TokenKind::Tuple => write!(f, "tuple"),
            TokenKind::Emit => write!(f, "emit"),
            TokenKind::Yield => write!(f, "yield"),
            TokenKind::Mod => write!(f, "mod"),
            TokenKind::SelfKw => write!(f, "self"),
            TokenKind::With => write!(f, "with"),
            TokenKind::Try => write!(f, "try"),
            TokenKind::Union => write!(f, "union"),
            TokenKind::Step => write!(f, "step"),
            TokenKind::Comptime => write!(f, "comptime"),
            TokenKind::Macro => write!(f, "macro"),
            TokenKind::Extern => write!(f, "extern"),
            TokenKind::Then => write!(f, "then"),
            TokenKind::When => write!(f, "when"),
            TokenKind::Is => write!(f, "is"),
            TokenKind::Bool => write!(f, "bool"),
            TokenKind::Int_ => write!(f, "int"),
            TokenKind::Float_ => write!(f, "float"),
            TokenKind::String_ => write!(f, "string"),
            TokenKind::Bytes => write!(f, "bytes"),
            TokenKind::Json => write!(f, "json"),
            // Operators
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
            TokenKind::Semicolon => write!(f, ";"),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::At => write!(f, "@"),
            TokenKind::Hash => write!(f, "#"),
            // Compound assignments
            TokenKind::PlusAssign => write!(f, "+="),
            TokenKind::MinusAssign => write!(f, "-="),
            TokenKind::StarAssign => write!(f, "*="),
            TokenKind::SlashAssign => write!(f, "/="),
            TokenKind::PercentAssign => write!(f, "%="),
            TokenKind::StarStarAssign => write!(f, "**="),
            TokenKind::AmpAssign => write!(f, "&="),
            TokenKind::PipeAssign => write!(f, "|="),
            TokenKind::CaretAssign => write!(f, "^="),
            // New operators
            TokenKind::StarStar => write!(f, "**"),
            TokenKind::DotDot => write!(f, ".."),
            TokenKind::DotDotEq => write!(f, "..="),
            TokenKind::PipeForward => write!(f, "|>"),
            TokenKind::Compose => write!(f, ">>"),
            TokenKind::QuestionQuestion => write!(f, "??"),
            TokenKind::QuestionDot => write!(f, "?."),
            TokenKind::Bang => write!(f, "!"),
            TokenKind::Question => write!(f, "?"),
            TokenKind::DotDotDot => write!(f, "..."),
            TokenKind::FatArrow => write!(f, "=>"),
            TokenKind::PlusPlus => write!(f, "++"),
            TokenKind::Ampersand => write!(f, "&"),
            TokenKind::Tilde => write!(f, "~"),
            TokenKind::TildeArrow => write!(f, "~>"),
            TokenKind::Caret => write!(f, "^"),
            TokenKind::FloorDiv => write!(f, "//"),
            TokenKind::FloorDivAssign => write!(f, "//="),
            TokenKind::QuestionBracket => write!(f, "?["),
            // Delimiters
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
            TokenKind::Symbol(c) => write!(f, "{}", c),
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
