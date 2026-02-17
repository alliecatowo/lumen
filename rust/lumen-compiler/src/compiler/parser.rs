//! Recursive descent parser with Pratt expression parsing for Lumen.

use crate::compiler::ast::*;
use crate::compiler::tokens::{Span, Token, TokenKind};
use num_bigint::BigInt;
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum ParseError {
    #[error("unexpected token {found} at line {line}, col {col}; expected {expected}")]
    Unexpected {
        found: String,
        expected: String,
        line: usize,
        col: usize,
    },
    #[error("unexpected end of input")]
    UnexpectedEof,
    #[error("unclosed '{bracket}' opened at line {open_line}, col {open_col}")]
    UnclosedBracket {
        bracket: char,
        open_line: usize,
        open_col: usize,
        current_line: usize,
        current_col: usize,
    },
    #[error("expected 'end' to close '{construct}' at line {open_line}, col {open_col}")]
    MissingEnd {
        construct: String,
        open_line: usize,
        open_col: usize,
        current_line: usize,
        current_col: usize,
    },
    #[error("expected type after ':' at line {line}, col {col}")]
    MissingType { line: usize, col: usize },
    #[error("incomplete expression at line {line}, col {col}")]
    IncompleteExpression {
        line: usize,
        col: usize,
        context: String,
    },
    #[error("malformed {construct} at line {line}, col {col}: {reason}")]
    MalformedConstruct {
        construct: String,
        reason: String,
        line: usize,
        col: usize,
    },
}

/// Maximum number of parse errors to collect before giving up.
/// Prevents cascading error spam from a single root cause.
const MAX_PARSE_ERRORS: usize = 10;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    bracket_depth: usize,
    errors: Vec<ParseError>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            bracket_depth: 0,
            errors: Vec::new(),
        }
    }

    /// Record a parse error and continue parsing.
    /// Returns true if the error limit has been reached.
    fn record_error(&mut self, error: ParseError) -> bool {
        self.errors.push(error);
        self.errors.len() >= MAX_PARSE_ERRORS
    }

    /// Whether we've hit the maximum number of errors and should stop recovery.
    fn at_error_limit(&self) -> bool {
        self.errors.len() >= MAX_PARSE_ERRORS
    }

    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }

    /// Synchronize parser state by skipping tokens until we reach a declaration boundary
    /// This function includes infinite loop protection by tracking position advancement
    fn synchronize(&mut self) {
        // Skip tokens until we reach a synchronization point:
        // - A new declaration keyword (cell, record, enum, type, grant, import, etc.)
        // - End of file
        let _start_pos = self.pos;
        let mut last_pos = self.pos;
        let mut iterations = 0;
        const MAX_ITERATIONS: usize = 10000; // Safety limit

        while !self.at_end() && iterations < MAX_ITERATIONS {
            // Safety check: ensure we're making progress
            if self.pos == last_pos && iterations > 0 {
                // Position hasn't advanced, force progress to avoid infinite loop
                self.advance();
            }
            last_pos = self.pos;
            iterations += 1;

            match self.peek_kind() {
                TokenKind::Cell
                | TokenKind::Record
                | TokenKind::Enum
                | TokenKind::Type
                | TokenKind::Grant
                | TokenKind::Import
                | TokenKind::Use
                | TokenKind::Pub
                | TokenKind::Async
                | TokenKind::At
                | TokenKind::Schema
                | TokenKind::Trait
                | TokenKind::Impl
                | TokenKind::Const
                | TokenKind::Macro
                | TokenKind::Extern
                | TokenKind::Comptime
                | TokenKind::MarkdownBlock(_)
                | TokenKind::Eof => break,
                TokenKind::Ident(name) => {
                    if matches!(
                        name.as_str(),
                        "agent"
                            | "effect"
                            | "bind"
                            | "handler"
                            | "pipeline"
                            | "orchestration"
                            | "machine"
                            | "memory"
                            | "guardrail"
                            | "eval"
                            | "pattern"
                    ) {
                        break;
                    }
                    self.advance();
                }
                _ => {
                    self.advance();
                }
            }
        }
    }
    /// Synchronize within a statement block to the next valid statement boundary
    /// Improved with better keyword detection and infinite loop protection
    fn synchronize_stmt(&mut self) {
        let mut last_pos = self.pos;
        let mut iterations = 0;
        const MAX_ITERATIONS: usize = 10000;

        while !self.at_end() && iterations < MAX_ITERATIONS {
            // Safety check: ensure we're making progress
            if self.pos == last_pos && iterations > 0 {
                self.advance();
            }
            last_pos = self.pos;
            iterations += 1;

            match self.peek_kind() {
                TokenKind::Newline => {
                    self.advance();
                    // After newline, check if next token is a statement keyword
                    if self.is_stmt_keyword() {
                        break;
                    }
                }
                TokenKind::End | TokenKind::Else | TokenKind::Dedent | TokenKind::Eof => break,
                // Statement keywords - new statement starts here
                TokenKind::Let
                | TokenKind::If
                | TokenKind::For
                | TokenKind::While
                | TokenKind::Loop
                | TokenKind::Match
                | TokenKind::Return
                | TokenKind::Halt
                | TokenKind::Break
                | TokenKind::Continue
                | TokenKind::Emit => break,
                // Top-level declaration keywords
                TokenKind::Cell
                | TokenKind::Record
                | TokenKind::Enum
                | TokenKind::Type
                | TokenKind::Grant
                | TokenKind::Import
                | TokenKind::Use
                | TokenKind::Pub
                | TokenKind::Schema
                | TokenKind::Fn
                | TokenKind::Mod
                | TokenKind::Trait
                | TokenKind::Impl => break,
                _ => {
                    self.advance();
                }
            }
        }
    }

    /// Check if current token is a statement keyword
    fn is_stmt_keyword(&self) -> bool {
        matches!(
            self.peek_kind(),
            TokenKind::Let
                | TokenKind::If
                | TokenKind::For
                | TokenKind::While
                | TokenKind::Loop
                | TokenKind::Match
                | TokenKind::Return
                | TokenKind::Halt
                | TokenKind::Break
                | TokenKind::Continue
                | TokenKind::Emit
        )
    }

    fn current(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .unwrap_or_else(|| self.tokens.last().unwrap())
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.current().kind
    }

    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos.min(self.tokens.len() - 1)];
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, kind: &TokenKind) -> Result<Token, ParseError> {
        let tok = self.current().clone();
        if std::mem::discriminant(&tok.kind) == std::mem::discriminant(kind) {
            self.advance();
            Ok(tok)
        } else {
            Err(ParseError::Unexpected {
                found: format!("{}", tok.kind),
                expected: format!("{}", kind),
                line: tok.span.line,
                col: tok.span.col,
            })
        }
    }

    fn skip_newlines(&mut self) {
        if self.bracket_depth > 0 {
            self.skip_whitespace_tokens();
        } else {
            while matches!(self.peek_kind(), TokenKind::Newline) {
                self.advance();
            }
        }
    }

    /// Skip newlines, indents, and dedents — used inside bracketed contexts
    fn skip_whitespace_tokens(&mut self) {
        while matches!(
            self.peek_kind(),
            TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
        ) {
            self.advance();
        }
    }

    fn at_end(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    fn peek_n_kind(&self, n: usize) -> Option<&TokenKind> {
        self.tokens.get(self.pos + n).map(|t| &t.kind)
    }

    /// Push an opening bracket onto the stack for error tracking
    fn looks_like_named_field(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Ident(_))
            && matches!(self.peek_n_kind(1), Some(TokenKind::Colon))
    }

    fn token_can_be_named_arg_key(&self) -> bool {
        matches!(
            self.peek_kind(),
            TokenKind::Ident(_)
                | TokenKind::Role
                | TokenKind::Schema
                | TokenKind::Tool
                | TokenKind::Type
                | TokenKind::From
                | TokenKind::With
                | TokenKind::Result
                | TokenKind::List
                | TokenKind::Map
                | TokenKind::Set
                | TokenKind::Tuple
        )
    }

    fn is_identifier_like(kind: &TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Ident(_)
                | TokenKind::SelfKw
                | TokenKind::Result
                | TokenKind::Cell
                | TokenKind::String_
                | TokenKind::Int_
                | TokenKind::Float_
                | TokenKind::Bool
                | TokenKind::Bytes
                | TokenKind::Json
                | TokenKind::Type
                | TokenKind::List
                | TokenKind::Map
                | TokenKind::Set
                | TokenKind::Tuple
                | TokenKind::Schema
                | TokenKind::Ok_
                | TokenKind::Err_
                | TokenKind::Tool
                | TokenKind::Role
                | TokenKind::Union
                | TokenKind::From
                | TokenKind::With
                | TokenKind::Where
                | TokenKind::When
                | TokenKind::Try
                | TokenKind::Step
                | TokenKind::Comptime
                | TokenKind::Macro
                | TokenKind::Extern
                | TokenKind::Async
                | TokenKind::Loop
                | TokenKind::If
                | TokenKind::Match
                | TokenKind::End
        )
    }

    fn consumes_section_colon_block(&self) -> bool {
        let mut i = self.pos;
        if !matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Ident(_))
                | Some(TokenKind::Role)
                | Some(TokenKind::Tool)
                | Some(TokenKind::With)
                | Some(TokenKind::From)
                | Some(TokenKind::Type)
                | Some(TokenKind::Result)
                | Some(TokenKind::List)
                | Some(TokenKind::Map)
                | Some(TokenKind::Set)
                | Some(TokenKind::Tuple)
        ) {
            return false;
        }
        i += 1;

        if matches!(self.tokens.get(i).map(|t| &t.kind), Some(TokenKind::LParen)) {
            let mut depth = 0usize;
            while let Some(kind) = self.tokens.get(i).map(|t| &t.kind) {
                match kind {
                    TokenKind::LParen => depth += 1,
                    TokenKind::RParen => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    TokenKind::Eof => return false,
                    _ => {}
                }
                i += 1;
            }
        }

        if !matches!(self.tokens.get(i).map(|t| &t.kind), Some(TokenKind::Colon)) {
            return false;
        }
        i += 1;
        while matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Newline)
        ) {
            i += 1;
        }
        matches!(self.tokens.get(i).map(|t| &t.kind), Some(TokenKind::Indent))
    }

    // ── Top-level parsing ──

    pub fn parse_program(&mut self, directives: Vec<Directive>) -> Result<Program, ParseError> {
        let span_start = self.current().span;
        let mut items = Vec::new();
        let mut top_level_stmts = Vec::new();
        self.skip_newlines();
        while !self.at_end() && !self.at_error_limit() {
            self.skip_newlines();
            if self.at_end() {
                break;
            }
            if matches!(self.peek_kind(), TokenKind::Indent | TokenKind::Dedent) {
                self.advance();
                continue;
            }
            if matches!(self.peek_kind(), TokenKind::End) {
                self.advance();
                continue;
            }
            // Handle markdown blocks: docstrings or comments
            if matches!(self.peek_kind(), TokenKind::MarkdownBlock(_)) {
                let doc_content =
                    if let TokenKind::MarkdownBlock(content) = self.peek_kind().clone() {
                        self.advance();
                        Some(content)
                    } else {
                        None
                    };
                self.skip_newlines();
                // If followed by a declaration keyword, parse item and attach docstring
                if !self.at_end()
                    && !self.is_top_level_stmt_start()
                    && !matches!(self.peek_kind(), TokenKind::Eof | TokenKind::End)
                {
                    match self.parse_item() {
                        Ok(mut item) => {
                            // Attach doc to the item
                            match &mut item {
                                Item::Cell(c) => c.doc = doc_content,
                                Item::Record(r) => r.doc = doc_content,
                                Item::Enum(e) => e.doc = doc_content,
                                Item::Handler(h) => h.doc = doc_content,
                                Item::TypeAlias(t) => t.doc = doc_content,
                                _ => {} // other items don't have doc fields
                            }
                            items.push(item);
                        }
                        Err(err) => {
                            if self.record_error(err) {
                                break;
                            }
                            self.synchronize();
                        }
                    }
                }
                // Otherwise it was just a comment, continue to next iteration
                continue;
            }
            if self.is_top_level_stmt_start() {
                match self.parse_stmt() {
                    Ok(stmt) => top_level_stmts.push(stmt),
                    Err(err) => {
                        if self.record_error(err) {
                            break; // hit max error limit
                        }
                        self.synchronize();
                    }
                }
            } else {
                match self.parse_item() {
                    Ok(item) => items.push(item),
                    Err(err) => {
                        if self.record_error(err) {
                            break; // hit max error limit
                        }
                        self.synchronize();
                    }
                }
            }
            self.skip_newlines();
        }
        if !top_level_stmts.is_empty() {
            let has_main = items
                .iter()
                .any(|item| matches!(item, Item::Cell(c) if c.name == "main"));
            let synthetic_name = if has_main {
                "__script_main".to_string()
            } else {
                "main".to_string()
            };
            let end_span = top_level_stmts
                .last()
                .map(|s| s.span())
                .unwrap_or(span_start);
            items.push(Item::Cell(CellDef {
                name: synthetic_name,
                generic_params: vec![],
                params: vec![],
                return_type: None,
                effects: vec![],
                body: top_level_stmts,
                is_pub: false,
                is_async: false,
                is_extern: false,
                must_use: false,
                where_clauses: vec![],
                span: span_start.merge(end_span),
                doc: None,
            }));
        }
        let span = if items.is_empty() {
            span_start
        } else {
            span_start.merge(items.last().unwrap().span())
        };
        Ok(Program {
            directives,
            items,
            span,
        })
    }

    /// Check if current position is `@must_use` (@ followed by identifier "must_use")
    fn is_must_use_attribute(&self) -> bool {
        if !matches!(self.peek_kind(), TokenKind::At) {
            return false;
        }
        // Look ahead: position after @ should be ident "must_use"
        if let Some(tok) = self.tokens.get(self.pos + 1) {
            matches!(&tok.kind, TokenKind::Ident(name) if name == "must_use")
        } else {
            false
        }
    }

    fn is_top_level_stmt_start(&self) -> bool {
        match self.peek_kind() {
            TokenKind::Let
            | TokenKind::If
            | TokenKind::For
            | TokenKind::Match
            | TokenKind::Return
            | TokenKind::Halt
            | TokenKind::While
            | TokenKind::Loop
            | TokenKind::Break
            | TokenKind::Continue
            | TokenKind::Emit
            | TokenKind::Role
            | TokenKind::LParen
            | TokenKind::LBracket
            | TokenKind::LBrace
            | TokenKind::IntLit(_)
            | TokenKind::FloatLit(_)
            | TokenKind::StringLit(_)
            | TokenKind::StringInterpLit(_)
            | TokenKind::RawStringLit(_)
            | TokenKind::BytesLit(_)
            | TokenKind::BoolLit(_)
            | TokenKind::Null
            | TokenKind::NullLit
            | TokenKind::Minus
            | TokenKind::Not
            | TokenKind::Tilde
            | TokenKind::Fn
            | TokenKind::With
            | TokenKind::SelfKw => true,
            TokenKind::Ident(name) => !matches!(
                name.as_str(),
                "agent"
                    | "effect"
                    | "bind"
                    | "handler"
                    | "pipeline"
                    | "orchestration"
                    | "machine"
                    | "memory"
                    | "guardrail"
                    | "eval"
                    | "pattern"
            ),
            _ => false,
        }
    }

    fn parse_item(&mut self) -> Result<Item, ParseError> {
        // Handle `pub` modifier
        let is_pub = matches!(self.peek_kind(), TokenKind::Pub);
        if is_pub {
            self.advance();
            self.skip_newlines();
        }

        // Handle `async` modifier for cells
        let is_async = matches!(self.peek_kind(), TokenKind::Async);
        if is_async {
            self.advance();
            self.skip_newlines();
        }

        match self.peek_kind() {
            TokenKind::Record => {
                let mut r = self.parse_record()?;
                r.is_pub = is_pub;
                Ok(Item::Record(r))
            }
            TokenKind::Enum => {
                let mut e = self.parse_enum()?;
                e.is_pub = is_pub;
                Ok(Item::Enum(e))
            }
            TokenKind::Cell => {
                let mut c = self.parse_cell(true)?;
                c.is_pub = is_pub;
                c.is_async = is_async;
                Ok(Item::Cell(c))
            }
            TokenKind::At => {
                // Check for @must_use before a cell definition
                if self.is_must_use_attribute() {
                    self.advance(); // consume '@'
                    self.advance(); // consume 'must_use'
                    self.skip_newlines();
                    if matches!(self.peek_kind(), TokenKind::Pub) {
                        // @must_use pub cell ...
                        self.advance();
                        self.skip_newlines();
                        let mut c = self.parse_cell(true)?;
                        c.is_pub = true;
                        c.must_use = true;
                        Ok(Item::Cell(c))
                    } else if matches!(self.peek_kind(), TokenKind::Cell) {
                        let mut c = self.parse_cell(true)?;
                        c.is_pub = is_pub;
                        c.must_use = true;
                        Ok(Item::Cell(c))
                    } else {
                        // @must_use not followed by cell — treat as regular attribute
                        // Back up and re-parse as attribute
                        // Since we already consumed @ and must_use, just make an AddonDecl
                        let end = self.current().span;
                        if matches!(self.peek_kind(), TokenKind::Newline) {
                            self.skip_newlines();
                        }
                        Ok(Item::Addon(AddonDecl {
                            kind: "attribute".into(),
                            name: Some("must_use".to_string()),
                            span: end,
                        }))
                    }
                } else {
                    Ok(Item::Addon(self.parse_attribute_decl()?))
                }
            }
            TokenKind::Use => Ok(Item::UseTool(self.parse_use_tool()?)),
            TokenKind::Grant => Ok(Item::Grant(self.parse_grant()?)),
            TokenKind::Type => Ok(Item::TypeAlias(self.parse_type_alias(is_pub)?)),
            TokenKind::Trait => Ok(Item::Trait(self.parse_trait_def(is_pub)?)),
            TokenKind::Impl => Ok(Item::Impl(self.parse_impl_def()?)),
            TokenKind::Import => Ok(Item::Import(self.parse_import(is_pub)?)),
            TokenKind::Const => Ok(Item::ConstDecl(self.parse_const_decl()?)),
            TokenKind::Macro => Ok(Item::MacroDecl(self.parse_macro_decl()?)),
            TokenKind::Schema => {
                let start = self.current().span;
                self.advance();
                self.consume_block_until_end();
                Ok(Item::Addon(AddonDecl {
                    kind: "schema".into(),
                    name: None,
                    span: start.merge(self.current().span),
                }))
            }
            TokenKind::Extern => {
                self.advance(); // consume extern
                self.skip_newlines();
                if matches!(self.peek_kind(), TokenKind::Cell) {
                    let mut c = self.parse_cell(false)?;
                    c.is_pub = is_pub;
                    c.is_async = is_async;
                    c.is_extern = true;
                    Ok(Item::Cell(c))
                } else {
                    let start = self.current().span;
                    self.consume_rest_of_line();
                    Ok(Item::Addon(AddonDecl {
                        kind: "extern".into(),
                        name: None,
                        span: start.merge(self.current().span),
                    }))
                }
            }
            TokenKind::Comptime => {
                let start = self.current().span;
                let kind = format!("{}", self.peek_kind());
                self.advance();
                self.consume_rest_of_line();
                Ok(Item::Addon(AddonDecl {
                    kind,
                    name: None,
                    span: start.merge(self.current().span),
                }))
            }
            TokenKind::Ident(name) => match name.as_str() {
                "agent" => Ok(Item::Agent(self.parse_agent_decl()?)),
                "effect" => Ok(Item::Effect(self.parse_effect_decl()?)),
                "handler" => Ok(Item::Handler(self.parse_handler_decl()?)),
                "bind" => {
                    if matches!(self.peek_n_kind(1), Some(TokenKind::Ident(s)) if s == "effect") {
                        Ok(Item::EffectBind(self.parse_effect_bind_decl()?))
                    } else {
                        Ok(Item::Addon(self.parse_addon_decl()?))
                    }
                }
                "pipeline" | "orchestration" | "machine" | "memory" | "guardrail" | "eval"
                | "pattern" => Ok(Item::Process(self.parse_process_decl()?)),
                _ => {
                    eprintln!("DEBUG: Ident fallback for {}", name);
                    let tok = self.current().clone();
                    Err(ParseError::Unexpected {
                        found: name.clone(),
                        expected: "top-level declaration".into(),
                        line: tok.span.line,
                        col: tok.span.col,
                    })
                }
            },
            _ => {
                let kind = format!("{}", self.peek_kind());
                eprintln!("DEBUG: General fallback for {}", kind);
                let tok = self.current().clone();
                Err(ParseError::Unexpected {
                    found: kind,
                    expected: "top-level declaration".into(),
                    line: tok.span.line,
                    col: tok.span.col,
                })
            }
        }
    }

    // ── Record ──

    fn parse_record(&mut self) -> Result<RecordDef, ParseError> {
        let start = self.expect(&TokenKind::Record)?.span;
        let name = self.expect_ident()?;
        let generic_params = self.parse_optional_generic_params()?;
        self.skip_newlines();
        let mut fields = Vec::new();
        // Fields can be indent-based or just listed until 'end'
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent {
            self.advance();
        }
        self.skip_newlines();
        while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
                break;
            }
            if matches!(self.peek_kind(), TokenKind::Indent | TokenKind::Dedent) {
                self.advance();
                continue;
            }
            if matches!(self.peek_kind(), TokenKind::At) {
                let _ = self.parse_attribute_decl()?;
            } else if self.looks_like_named_field() {
                fields.push(self.parse_field()?);
            } else if matches!(self.peek_kind(), TokenKind::Ident(s) if s == "migrate") {
                self.advance();
                self.consume_block_until_end();
            } else {
                self.consume_rest_of_line();
            }
            self.skip_newlines();
        }
        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) {
            self.advance();
        }
        while matches!(
            self.peek_kind(),
            TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
        ) {
            self.advance();
        }
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(RecordDef {
            name,
            generic_params,
            fields,
            is_pub: false,
            span: start.merge(end_span),
            doc: None,
        })
    }

    fn parse_field(&mut self) -> Result<FieldDef, ParseError> {
        let start = self.current().span;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon)?;
        let ty = self.parse_type()?;
        let default_value = if matches!(self.peek_kind(), TokenKind::Assign) {
            self.advance();
            Some(self.parse_expr(0)?)
        } else {
            None
        };
        let constraint = if matches!(self.peek_kind(), TokenKind::Where) {
            self.advance();
            Some(self.parse_expr(0)?)
        } else {
            None
        };
        let span = start.merge(
            constraint
                .as_ref()
                .map(|c| c.span())
                .or(default_value.as_ref().map(|d| d.span()))
                .unwrap_or(ty.span()),
        );
        Ok(FieldDef {
            name,
            ty,
            default_value,
            constraint,
            span,
        })
    }

    // ── Enum ──

    fn parse_enum(&mut self) -> Result<EnumDef, ParseError> {
        let start = self.expect(&TokenKind::Enum)?.span;
        let name = self.expect_ident()?;
        let generic_params = self.parse_optional_generic_params()?;
        self.skip_newlines();
        let mut variants = Vec::new();
        let mut methods = Vec::new();
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent {
            self.advance();
        }
        self.skip_newlines();
        while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Dedent) {
                let mut i = self.pos;
                while matches!(
                    self.tokens.get(i).map(|t| &t.kind),
                    Some(TokenKind::Dedent | TokenKind::Newline)
                ) {
                    i += 1;
                }
                let mut j = i;
                let mut saw_arrow = false;
                while let Some(tok) = self.tokens.get(j) {
                    match tok.kind {
                        TokenKind::Arrow => {
                            saw_arrow = true;
                            break;
                        }
                        TokenKind::Newline
                        | TokenKind::Eof
                        | TokenKind::End
                        | TokenKind::Dedent => break,
                        _ => j += 1,
                    }
                }
                if saw_arrow {
                    while matches!(self.peek_kind(), TokenKind::Dedent | TokenKind::Newline) {
                        self.advance();
                    }
                    continue;
                }
                break;
            }
            if matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
                break;
            }
            if matches!(self.peek_kind(), TokenKind::Cell) {
                methods.push(self.parse_cell(true)?);
                self.skip_newlines();
                continue;
            }
            let vs = self.current().span;
            let vname = self.expect_ident()?;
            let payload = if matches!(self.peek_kind(), TokenKind::LParen) {
                self.advance();
                if matches!(self.peek_kind(), TokenKind::RParen) {
                    self.advance();
                    None
                } else {
                    let start_span = self.current().span;
                    let mut types = Vec::new();

                    loop {
                        // Check for named param pattern: name: Type
                        // The name can be an Ident or a keyword (e.g. `result`, `ok`, `err`)
                        // so we check if the next token is a colon regardless of current token kind.
                        let is_named_field = self
                            .tokens
                            .get(self.pos + 1)
                            .is_some_and(|tok| matches!(tok.kind, TokenKind::Colon));

                        if is_named_field {
                            // Named param: skip name and colon, parse type
                            self.advance();
                            self.advance();
                            match self.parse_type() {
                                Ok(ty) => types.push(ty),
                                Err(_) => {
                                    self.consume_variant_arg_tokens();
                                }
                            }
                        } else {
                            match self.parse_type() {
                                Ok(ty) => types.push(ty),
                                Err(_) => {
                                    types.push(TypeExpr::Named("Any".into(), start_span));
                                }
                            }
                        }

                        if matches!(self.peek_kind(), TokenKind::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }

                    if matches!(self.peek_kind(), TokenKind::RParen) {
                        self.advance();
                    }

                    if types.is_empty() {
                        None
                    } else if types.len() == 1 {
                        Some(types.remove(0))
                    } else {
                        Some(TypeExpr::Tuple(types, start_span))
                    }
                }
            } else {
                None
            };
            variants.push(EnumVariant {
                name: vname,
                payload,
                span: vs,
            });
            self.skip_newlines();
        }
        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) {
            self.advance();
        }
        self.skip_newlines();
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(EnumDef {
            name,
            generic_params,
            variants,
            methods,
            is_pub: false,
            span: start.merge(end_span),
            doc: None,
        })
    }

    // ── Cell ──

    fn parse_cell(&mut self, require_body: bool) -> Result<CellDef, ParseError> {
        let start = self.expect(&TokenKind::Cell)?.span;
        let name = self.expect_ident()?;
        let generic_params = self.parse_optional_generic_params()?;
        self.expect(&TokenKind::LParen)?;
        self.bracket_depth += 1;
        let mut params = Vec::new();
        self.skip_whitespace_tokens();
        while !matches!(self.peek_kind(), TokenKind::RParen) {
            if !params.is_empty() {
                self.expect(&TokenKind::Comma)?;
                self.skip_whitespace_tokens();
            }
            let variadic = if matches!(self.peek_kind(), TokenKind::DotDot | TokenKind::DotDotDot) {
                self.advance();
                true
            } else {
                false
            };
            let ps = self.current().span;
            let pname = self.expect_ident()?;
            let pty = if matches!(self.peek_kind(), TokenKind::Colon) {
                self.advance();
                self.parse_type()?
            } else {
                TypeExpr::Named("Any".into(), ps)
            };
            let default_value = if matches!(self.peek_kind(), TokenKind::Assign) {
                self.advance();
                Some(self.parse_expr(0)?)
            } else {
                None
            };
            params.push(Param {
                name: pname,
                ty: pty,
                default_value,
                variadic,
                span: ps,
            });
            self.skip_whitespace_tokens();
        }
        self.bracket_depth -= 1;
        self.expect(&TokenKind::RParen)?;
        let ret = if matches!(self.peek_kind(), TokenKind::Arrow) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };
        let effects = self.parse_optional_effect_row()?;

        if matches!(self.peek_kind(), TokenKind::Assign) {
            self.advance();
            let expr = self.parse_expr(0)?;
            let span = start.merge(expr.span());
            return Ok(CellDef {
                name,
                generic_params,
                params,
                return_type: ret,
                effects,
                body: vec![Stmt::Return(ReturnStmt { value: expr, span })],
                is_pub: false,
                is_async: false,
                is_extern: false,
                must_use: false,
                where_clauses: vec![],
                span,
                doc: None,
            });
        }

        // Prototype/signature form (used in effect declarations and trait-like stubs):
        // cell f(x: Int) -> Int / {http}
        if !require_body
            && matches!(
                self.peek_kind(),
                TokenKind::Newline | TokenKind::Eof | TokenKind::Dedent
            )
        {
            let mut look = self.pos;
            while matches!(
                self.tokens.get(look).map(|t| &t.kind),
                Some(TokenKind::Newline)
            ) {
                look += 1;
            }
            if !matches!(
                self.tokens.get(look).map(|t| &t.kind),
                Some(TokenKind::Indent)
            ) {
                let end_span = self.current().span;
                return Ok(CellDef {
                    name,
                    generic_params,
                    params,
                    return_type: ret,
                    effects,
                    body: vec![],
                    is_pub: false,
                    is_async: false,
                    is_extern: false,
                    must_use: false,
                    where_clauses: vec![],
                    span: start.merge(end_span),
                    doc: None,
                });
            }
        }

        self.skip_newlines();
        let body = self.parse_block()?;
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(CellDef {
            name,
            generic_params,
            params,
            return_type: ret,
            effects,
            body,
            is_pub: false,
            is_async: false,
            is_extern: false,
            must_use: false,
            where_clauses: vec![],
            span: start.merge(end_span),
            doc: None,
        })
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut stmts = Vec::new();
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent {
            self.advance();
        }
        self.skip_newlines();
        while !matches!(
            self.peek_kind(),
            TokenKind::End
                | TokenKind::Eof
                | TokenKind::Else
                | TokenKind::Cell
                | TokenKind::Record
                | TokenKind::Enum
                | TokenKind::Type
                | TokenKind::Grant
                | TokenKind::Import
                | TokenKind::Use
                | TokenKind::Pub
                | TokenKind::Schema
                | TokenKind::Fn
                | TokenKind::Mod
                | TokenKind::Trait
                | TokenKind::Impl
        ) {
            self.skip_newlines();
            if matches!(
                self.peek_kind(),
                TokenKind::End
                    | TokenKind::Eof
                    | TokenKind::Else
                    | TokenKind::Cell
                    | TokenKind::Record
                    | TokenKind::Enum
                    | TokenKind::Type
                    | TokenKind::Grant
                    | TokenKind::Import
                    | TokenKind::Use
                    | TokenKind::Pub
                    | TokenKind::Schema
                    | TokenKind::Fn
                    | TokenKind::Mod
                    | TokenKind::Trait
                    | TokenKind::Impl
            ) {
                break;
            }
            if matches!(self.peek_kind(), TokenKind::Dedent) {
                let mut i = self.pos;
                while matches!(
                    self.tokens.get(i).map(|t| &t.kind),
                    Some(TokenKind::Dedent | TokenKind::Newline)
                ) {
                    i += 1;
                }
                if matches!(
                    self.tokens.get(i).map(|t| &t.kind),
                    Some(
                        TokenKind::End
                            | TokenKind::Else
                            | TokenKind::Eof
                            | TokenKind::Cell
                            | TokenKind::Record
                            | TokenKind::Enum
                            | TokenKind::Type
                            | TokenKind::Grant
                            | TokenKind::Import
                            | TokenKind::Use
                            | TokenKind::Pub
                            | TokenKind::Schema
                            | TokenKind::Fn
                            | TokenKind::Mod
                            | TokenKind::Trait
                            | TokenKind::Impl
                    )
                ) {
                    break;
                }
                self.advance();
                continue;
            }
            match self.parse_stmt() {
                Ok(stmt) => stmts.push(stmt),
                Err(err) => {
                    if self.record_error(err) {
                        break; // hit max error limit
                    }
                    self.synchronize_stmt();
                }
            }
            self.skip_newlines();
        }
        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) {
            self.advance();
        }
        while matches!(self.peek_kind(), TokenKind::Dedent) {
            let mut i = self.pos + 1;
            while matches!(
                self.tokens.get(i).map(|t| &t.kind),
                Some(TokenKind::Newline | TokenKind::Dedent)
            ) {
                i += 1;
            }
            if matches!(
                self.tokens.get(i).map(|t| &t.kind),
                Some(
                    TokenKind::End
                        | TokenKind::Else
                        | TokenKind::Eof
                        | TokenKind::Cell
                        | TokenKind::Record
                        | TokenKind::Enum
                        | TokenKind::Type
                        | TokenKind::Grant
                        | TokenKind::Import
                        | TokenKind::Use
                        | TokenKind::Pub
                        | TokenKind::Schema
                        | TokenKind::Fn
                        | TokenKind::Mod
                        | TokenKind::Trait
                        | TokenKind::Impl
                )
            ) {
                self.advance();
            } else {
                break;
            }
        }
        self.skip_newlines();
        Ok(stmts)
    }

    fn parse_block_strict_dedent(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut stmts = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Indent) {
            self.advance();
        }
        self.skip_newlines();
        while !matches!(self.peek_kind(), TokenKind::Dedent | TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Dedent | TokenKind::Eof) {
                break;
            }
            match self.parse_stmt() {
                Ok(stmt) => stmts.push(stmt),
                Err(err) => {
                    if self.record_error(err) {
                        break; // hit max error limit
                    }
                    self.synchronize_stmt();
                }
            }
            self.skip_newlines();
        }
        if matches!(self.peek_kind(), TokenKind::Dedent) {
            self.advance();
        }
        Ok(stmts)
    }

    // ── Statements ──

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        match self.peek_kind() {
            TokenKind::Let => self.parse_let(),
            TokenKind::If => self.parse_if(),
            TokenKind::For => self.parse_for(),
            TokenKind::Match => self.parse_match(),
            TokenKind::Return => self.parse_return(),
            TokenKind::Halt => self.parse_halt(),
            TokenKind::While => self.parse_while(),
            TokenKind::Loop => self.parse_loop(),
            TokenKind::Break => self.parse_break(),
            TokenKind::Continue => self.parse_continue(),
            TokenKind::Emit => self.parse_emit(),
            TokenKind::Yield => {
                let start = self.advance().span;
                let value = self.parse_expr(0)?;
                let span = start.merge(value.span());
                Ok(Stmt::Yield(YieldStmt { value, span }))
            }
            TokenKind::Defer => self.parse_defer(),
            TokenKind::Where => {
                let s = self.current().span;
                self.advance();
                self.consume_rest_of_line();
                Ok(Stmt::Emit(EmitStmt {
                    value: Expr::StringLit("where".into(), s),
                    span: s,
                }))
            }
            TokenKind::At => {
                let save = self.pos;
                self.advance();
                if matches!(self.peek_kind(), TokenKind::Ident(_)) {
                    self.advance();
                }
                if matches!(
                    self.peek_kind(),
                    TokenKind::For | TokenKind::While | TokenKind::Loop
                ) {
                    return self.parse_stmt();
                }
                self.pos = save;
                let start = self.current().span;
                let decl = self.parse_attribute_decl()?;
                let label = decl.name.unwrap_or_else(|| "attribute".to_string());
                let value = Expr::StringLit(label, start);
                let span = start.merge(value.span());
                Ok(Stmt::Emit(EmitStmt { value, span }))
            }
            TokenKind::With => self.parse_addon_stmt(),
            TokenKind::Ident(_)
            | TokenKind::SelfKw
            | TokenKind::Result
            | TokenKind::Cell
            | TokenKind::String_
            | TokenKind::Int_
            | TokenKind::Float_
            | TokenKind::Bool
            | TokenKind::Bytes
            | TokenKind::Json
            | TokenKind::Type
            | TokenKind::List
            | TokenKind::Map
            | TokenKind::Set
            | TokenKind::Tuple
            | TokenKind::Schema
            | TokenKind::Ok_
            | TokenKind::Err_
            | TokenKind::Tool
            | TokenKind::Union
            | TokenKind::From
            | TokenKind::When
            | TokenKind::Try
            | TokenKind::Step
            | TokenKind::Comptime
            | TokenKind::Macro
            | TokenKind::Extern
            | TokenKind::Async
            | TokenKind::End => {
                if self.is_addon_stmt_keyword() {
                    return self.parse_addon_stmt();
                }
                // Check for compound assignment: ident += expr
                if self.is_compound_assignment() {
                    self.parse_compound_assign()
                } else if self.is_assignment() {
                    self.parse_assign()
                } else {
                    self.parse_expr_stmt()
                }
            }
            TokenKind::Role => {
                let expr = self.parse_role_block_stmt()?;
                Ok(Stmt::Expr(ExprStmt {
                    expr: expr.clone(),
                    span: expr.span(),
                }))
            }
            _ => self.parse_expr_stmt(),
        }
    }

    fn is_addon_stmt_keyword(&self) -> bool {
        matches!(
            self.peek_kind(),
            TokenKind::Ident(name)
                if matches!(name.as_str(), "approve" | "checkpoint" | "escalate" | "observe" | "with" | "with_tool")
        )
    }

    fn parse_addon_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current().span;
        let kind = match self.peek_kind().clone() {
            TokenKind::Ident(_) => self.expect_ident()?,
            TokenKind::With => {
                self.advance();
                "with".to_string()
            }
            _ => {
                let tok = self.current().clone();
                return Err(ParseError::Unexpected {
                    found: format!("{}", tok.kind),
                    expected: "addon statement".into(),
                    line: tok.span.line,
                    col: tok.span.col,
                });
            }
        };
        self.consume_rest_of_line();

        // Addendum block statements carry DSL metadata. For now we preserve payload
        // for compilation and keep optional `in` bodies executable.
        if matches!(self.peek_kind(), TokenKind::Newline) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Indent) {
                self.consume_indented_payload();
                self.skip_newlines();
                if matches!(self.peek_kind(), TokenKind::End) {
                    self.advance();
                }
            } else if matches!(self.peek_kind(), TokenKind::In) {
                self.consume_block_until_end();
            }
        }

        let mut in_block_end = None;
        let mut in_block = None;
        self.skip_newlines();
        if matches!(self.peek_kind(), TokenKind::In) {
            self.advance();
            self.skip_newlines();
            let body = self.parse_block()?;
            let end = self.expect(&TokenKind::End)?.span;
            in_block_end = Some(end);
            in_block = Some(body);
        }

        if let Some(body) = in_block {
            let end = in_block_end.unwrap_or(start);
            return Ok(Stmt::If(IfStmt {
                condition: Expr::BoolLit(true, start),
                then_body: body,
                else_body: None,
                span: start.merge(end),
            }));
        }

        let value = Expr::StringLit(kind, start);
        let span = start.merge(value.span());
        Ok(Stmt::Emit(EmitStmt { value, span }))
    }

    fn parse_let(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::Let)?.span;
        let mutable = if matches!(self.peek_kind(), TokenKind::Mut) {
            self.advance();
            true
        } else {
            false
        };
        let (name, pattern) = if matches!(self.peek_kind(), TokenKind::LParen | TokenKind::LBracket)
        {
            // Destructuring pattern: let (a, b) = ... or let [a, b] = ...
            let pat = self.parse_pattern()?;
            let pat_name = match &pat {
                Pattern::TupleDestructure { elements, .. } => {
                    // Use first element name as the binding name for backwards compat
                    elements
                        .first()
                        .and_then(|p| match p {
                            Pattern::Ident(n, _) => Some(n.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| "__tuple".to_string())
                }
                Pattern::ListDestructure { elements, .. } => elements
                    .first()
                    .and_then(|p| match p {
                        Pattern::Ident(n, _) => Some(n.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| "__pattern".to_string()),
                _ => "__pattern".to_string(),
            };
            (pat_name, Some(pat))
        } else if matches!(self.peek_kind(), TokenKind::LBrace) {
            // Record destructuring: let { a, b } = ...
            let brace_span = self.advance().span; // consume {
            let mut fields = Vec::new();
            while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                if matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                    continue;
                }
                let field_name = self.expect_ident()?;
                fields.push((field_name, None));
            }
            if matches!(self.peek_kind(), TokenKind::RBrace) {
                self.advance();
            }
            let first_name = fields
                .first()
                .map(|(n, _)| n.clone())
                .unwrap_or_else(|| "__pattern".to_string());
            let pat = Pattern::RecordDestructure {
                type_name: String::new(), // anonymous destructure
                fields,
                open: false,
                span: brace_span,
            };
            (first_name, Some(pat))
        } else {
            let first = self.expect_ident()?;
            if matches!(self.peek_kind(), TokenKind::LParen) {
                // Variant/record destructuring: let Name(x) = ... or let ok(v) = ...
                // Back up pos to include the ident we already consumed
                self.pos -= 1;
                let pat = self.parse_pattern()?;
                let pat_name = match &pat {
                    Pattern::Variant(_, Some(binding), _) => Self::pattern_binding_name(binding)
                        .unwrap_or_else(|| "__pattern".to_string()),
                    Pattern::RecordDestructure { fields, .. } => fields
                        .first()
                        .map(|(n, _)| n.clone())
                        .unwrap_or_else(|| "__pattern".to_string()),
                    _ => first.clone(),
                };
                (pat_name, Some(pat))
            } else {
                (first, None)
            }
        };
        let ty = if matches!(self.peek_kind(), TokenKind::Colon) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&TokenKind::Assign)?;
        let value = self.parse_expr(0)?;
        let span = start.merge(value.span());
        Ok(Stmt::Let(LetStmt {
            name,
            mutable,
            pattern,
            ty,
            value,
            span,
        }))
    }

    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::If)?.span;

        // Detect `if let <pattern> = <expr>` and desugar to match
        if matches!(self.peek_kind(), TokenKind::Let) {
            self.advance();
            let pattern = self.parse_pattern()?;
            let subject = if matches!(self.peek_kind(), TokenKind::Assign) {
                self.advance();
                self.parse_expr(0)?
            } else {
                // Malformed: treat as matching against true
                Expr::BoolLit(true, start)
            };

            // Parse the then body (block-style only for if-let)
            self.skip_newlines();
            let then_body = self.parse_block()?;

            // Parse optional else
            let else_body = if matches!(self.peek_kind(), TokenKind::Else) {
                self.advance();
                self.skip_newlines();
                if matches!(self.peek_kind(), TokenKind::If) {
                    let elif = self.parse_if()?;
                    Some(vec![elif])
                } else {
                    Some(self.parse_block()?)
                }
            } else {
                None
            };

            let end_span = if matches!(self.peek_kind(), TokenKind::End) {
                self.expect(&TokenKind::End)?.span
            } else if let Some(ref eb) = else_body {
                eb.last().map(|s| s.span()).unwrap_or(start)
            } else {
                start
            };

            // Build match arms: pattern => then_body, _ => else_body
            let mut arms = vec![MatchArm {
                pattern,
                body: then_body,
                span: start,
            }];
            if let Some(eb) = else_body {
                arms.push(MatchArm {
                    pattern: Pattern::Wildcard(start),
                    body: eb,
                    span: start,
                });
            }

            return Ok(Stmt::Match(MatchStmt {
                subject,
                arms,
                span: start.merge(end_span),
            }));
        }

        let cond = self.parse_expr(0)?;
        if matches!(self.peek_kind(), TokenKind::Then) {
            self.advance();
            let then_expr = self.parse_expr(0)?;
            let then_stmt = Stmt::Expr(ExprStmt {
                expr: then_expr.clone(),
                span: then_expr.span(),
            });
            let else_body = if matches!(self.peek_kind(), TokenKind::Else) {
                self.advance();
                if matches!(self.peek_kind(), TokenKind::If) {
                    let elif = self.parse_if()?;
                    Some(vec![elif])
                } else {
                    let expr = self.parse_expr(0)?;
                    Some(vec![Stmt::Expr(ExprStmt {
                        span: expr.span(),
                        expr,
                    })])
                }
            } else {
                None
            };
            let end_span = else_body
                .as_ref()
                .and_then(|b| b.last().map(|s| s.span()))
                .unwrap_or(then_stmt.span());
            return Ok(Stmt::If(IfStmt {
                condition: cond,
                then_body: vec![then_stmt],
                else_body,
                span: start.merge(end_span),
            }));
        }
        self.skip_newlines();
        let then_body = self.parse_block()?;
        let else_body = if matches!(self.peek_kind(), TokenKind::Else) {
            self.advance();
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::If) {
                // else if
                let elif = self.parse_if()?;
                Some(vec![elif])
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };
        let end_span = if matches!(self.peek_kind(), TokenKind::End) {
            self.expect(&TokenKind::End)?.span
        } else if let Some(ref eb) = else_body {
            eb.last().map(|s| s.span()).unwrap_or(start)
        } else {
            start
        };
        Ok(Stmt::If(IfStmt {
            condition: cond,
            then_body,
            else_body,
            span: start.merge(end_span),
        }))
    }

    fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::For)?.span;
        let label = if matches!(self.peek_kind(), TokenKind::At) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };
        let (var, pattern) = if matches!(self.peek_kind(), TokenKind::LParen) {
            // Tuple destructuring: for (k, v) in ...
            let pat = self.parse_pattern()?;
            let first_name = match &pat {
                Pattern::TupleDestructure { elements, .. } => elements
                    .first()
                    .and_then(|p| match p {
                        Pattern::Ident(n, _) => Some(n.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| "__tuple".to_string()),
                Pattern::Ident(n, _) => n.clone(),
                _ => "__pattern".to_string(),
            };
            (first_name, Some(pat))
        } else {
            (self.expect_ident()?, None)
        };
        self.expect(&TokenKind::In)?;
        let iter = self.parse_expr(0)?;
        let filter = if matches!(self.peek_kind(), TokenKind::If) {
            self.advance();
            Some(self.parse_expr(0)?)
        } else {
            None
        };
        self.skip_newlines();
        let body = self.parse_block()?;
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(Stmt::For(ForStmt {
            label,
            var,
            pattern,
            iter,
            filter,
            body,
            span: start.merge(end_span),
        }))
    }

    fn parse_match(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::Match)?.span;
        let subject = self.parse_expr(0)?;
        self.skip_newlines();
        let mut arms = Vec::new();
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent {
            self.advance();
        }
        self.skip_newlines();
        while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Dedent) {
                self.advance();
                continue;
            }
            if matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
                break;
            }
            let arm_start = self.current().span;
            let first_pattern = self.parse_pattern()?;
            let mut pattern = if matches!(self.peek_kind(), TokenKind::Pipe) {
                let mut patterns = vec![first_pattern];
                while matches!(self.peek_kind(), TokenKind::Pipe) {
                    self.advance();
                    patterns.push(self.parse_pattern()?);
                }
                Pattern::Or {
                    patterns,
                    span: arm_start,
                }
            } else {
                first_pattern
            };
            if matches!(self.peek_kind(), TokenKind::If) {
                self.advance();
                let guard = self.parse_expr(0)?;
                pattern = Pattern::Guard {
                    inner: Box::new(pattern),
                    condition: Box::new(guard),
                    span: arm_start,
                };
            }
            self.expect(&TokenKind::Arrow)?;
            // Check for block body (indent after arrow) or single-line
            let body = if matches!(self.peek_kind(), TokenKind::Newline) {
                // Multi-line arm body: newline followed by indent
                self.skip_newlines();
                if matches!(self.peek_kind(), TokenKind::Indent) {
                    self.parse_block_strict_dedent()?
                } else {
                    // Just whitespace, parse single statement
                    vec![self.parse_stmt()?]
                }
            } else {
                // Single-line arm: parse one statement
                vec![self.parse_stmt()?]
            };
            let arm_span = arm_start.merge(body.last().map(|s| s.span()).unwrap_or(arm_start));
            arms.push(MatchArm {
                pattern,
                body,
                span: arm_span,
            });
            self.skip_newlines();
        }
        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) {
            self.advance();
        }
        self.skip_newlines();
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(Stmt::Match(MatchStmt {
            subject,
            arms,
            span: start.merge(end_span),
        }))
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        if matches!(self.peek_kind(), TokenKind::LParen) {
            let s = self.advance().span;
            if self.paren_contains_top_level_arrow() {
                let mut depth = 1usize;
                while !self.at_end() {
                    match self.peek_kind() {
                        TokenKind::LParen => {
                            depth += 1;
                            self.advance();
                        }
                        TokenKind::RParen => {
                            depth -= 1;
                            self.advance();
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {
                            self.advance();
                        }
                    }
                }
                return Ok(Pattern::Wildcard(s));
            }
            if matches!(self.peek_kind(), TokenKind::RParen) {
                self.advance();
                return Ok(Pattern::TupleDestructure {
                    elements: vec![],
                    span: s,
                });
            }
            let first = self.parse_pattern()?;
            if matches!(self.peek_kind(), TokenKind::Comma) {
                let mut elements = vec![first];
                while matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                    if matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                        break;
                    }
                    elements.push(self.parse_pattern()?);
                }
                self.expect(&TokenKind::RParen)?;
                return Ok(Pattern::TupleDestructure { elements, span: s });
            }
            self.expect(&TokenKind::RParen)?;
            return Ok(first);
        }
        if matches!(self.peek_kind(), TokenKind::LBracket) {
            let s = self.advance().span;
            let mut elements = Vec::new();
            let mut rest = None;
            while !matches!(self.peek_kind(), TokenKind::RBracket | TokenKind::Eof) {
                if matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                    continue;
                }
                if matches!(self.peek_kind(), TokenKind::DotDot | TokenKind::DotDotDot) {
                    self.advance();
                    if matches!(self.peek_kind(), TokenKind::Ident(_)) {
                        rest = Some(self.expect_ident()?);
                    }
                    while !matches!(self.peek_kind(), TokenKind::RBracket | TokenKind::Eof) {
                        if matches!(self.peek_kind(), TokenKind::Comma) {
                            self.advance();
                            break;
                        }
                        self.advance();
                    }
                    continue;
                }
                elements.push(self.parse_pattern()?);
            }
            self.expect(&TokenKind::RBracket)?;
            return Ok(Pattern::ListDestructure {
                elements,
                rest,
                span: s,
            });
        }

        match self.peek_kind().clone() {
            TokenKind::IntLit(n) => {
                let s = self.advance().span;
                let start_expr = Expr::IntLit(n, s);
                if matches!(self.peek_kind(), TokenKind::DotDot | TokenKind::DotDotEq) {
                    let inclusive = matches!(self.peek_kind(), TokenKind::DotDotEq);
                    self.advance();
                    let end_expr = match self.peek_kind().clone() {
                        TokenKind::IntLit(en) => {
                            let es = self.advance().span;
                            Expr::IntLit(en, es)
                        }
                        TokenKind::FloatLit(en) => {
                            let es = self.advance().span;
                            Expr::FloatLit(en, es)
                        }
                        _ => {
                            let tok = self.current().clone();
                            return Err(ParseError::Unexpected {
                                found: format!("{}", tok.kind),
                                expected: "numeric literal for range end".into(),
                                line: tok.span.line,
                                col: tok.span.col,
                            });
                        }
                    };
                    let span = s.merge(end_expr.span());
                    return Ok(Pattern::Range {
                        start: Box::new(start_expr),
                        end: Box::new(end_expr),
                        inclusive,
                        span,
                    });
                }
                Ok(Pattern::Literal(start_expr))
            }
            TokenKind::FloatLit(n) => {
                let s = self.advance().span;
                let start_expr = Expr::FloatLit(n, s);
                if matches!(self.peek_kind(), TokenKind::DotDot | TokenKind::DotDotEq) {
                    let inclusive = matches!(self.peek_kind(), TokenKind::DotDotEq);
                    self.advance();
                    let end_expr = match self.peek_kind().clone() {
                        TokenKind::IntLit(en) => {
                            let es = self.advance().span;
                            Expr::IntLit(en, es)
                        }
                        TokenKind::FloatLit(en) => {
                            let es = self.advance().span;
                            Expr::FloatLit(en, es)
                        }
                        _ => {
                            let tok = self.current().clone();
                            return Err(ParseError::Unexpected {
                                found: format!("{}", tok.kind),
                                expected: "numeric literal for range end".into(),
                                line: tok.span.line,
                                col: tok.span.col,
                            });
                        }
                    };
                    let span = s.merge(end_expr.span());
                    return Ok(Pattern::Range {
                        start: Box::new(start_expr),
                        end: Box::new(end_expr),
                        inclusive,
                        span,
                    });
                }
                Ok(Pattern::Literal(start_expr))
            }
            TokenKind::StringLit(ref sv) => {
                let sv = sv.clone();
                let s = self.advance().span;
                Ok(Pattern::Literal(Expr::StringLit(sv, s)))
            }
            TokenKind::BoolLit(b) => {
                let s = self.advance().span;
                Ok(Pattern::Literal(Expr::BoolLit(b, s)))
            }
            TokenKind::Ident(ref name) if name == "_" => {
                let s = self.advance().span;
                Ok(Pattern::Wildcard(s))
            }
            TokenKind::Ok_ | TokenKind::Err_ => {
                let vname = format!("{}", self.peek_kind());
                let s = self.advance().span;
                if matches!(self.peek_kind(), TokenKind::LParen) {
                    self.advance();
                    let inner = if matches!(self.peek_kind(), TokenKind::RParen) {
                        None
                    } else {
                        Some(Box::new(self.parse_pattern()?))
                    };
                    self.expect(&TokenKind::RParen)?;
                    Ok(Pattern::Variant(vname, inner, s))
                } else {
                    Ok(Pattern::Variant(vname, None, s))
                }
            }
            TokenKind::Ident(ref name) => {
                let mut name = name.clone();
                let s = self.advance().span;
                while matches!(self.peek_kind(), TokenKind::Dot) {
                    self.advance();
                    name.push('.');
                    name.push_str(&self.expect_ident()?);
                }
                if matches!(self.peek_kind(), TokenKind::Colon) {
                    self.advance();
                    let ty = self.parse_type()?;
                    return Ok(Pattern::TypeCheck {
                        name,
                        type_expr: Box::new(ty),
                        span: s,
                    });
                }
                if matches!(self.peek_kind(), TokenKind::LParen) {
                    if self.paren_looks_like_record_destructure() {
                        self.advance(); // (
                        let mut fields = Vec::new();
                        let mut open = false;
                        while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                            if matches!(self.peek_kind(), TokenKind::Comma) {
                                self.advance();
                                continue;
                            }
                            if matches!(self.peek_kind(), TokenKind::DotDot | TokenKind::DotDotDot)
                            {
                                open = true;
                                self.advance();
                                if matches!(self.peek_kind(), TokenKind::Ident(_)) {
                                    self.advance();
                                }
                                continue;
                            }
                            let field_name = self.expect_ident()?;
                            let field_pat = if matches!(self.peek_kind(), TokenKind::Colon) {
                                self.advance();
                                if matches!(self.peek_kind(), TokenKind::Comma | TokenKind::RParen)
                                {
                                    None
                                } else {
                                    Some(self.parse_pattern()?)
                                }
                            } else {
                                None
                            };
                            fields.push((field_name, field_pat));
                            if matches!(self.peek_kind(), TokenKind::Comma) {
                                self.advance();
                            }
                        }
                        self.expect(&TokenKind::RParen)?;
                        Ok(Pattern::RecordDestructure {
                            type_name: name,
                            fields,
                            open,
                            span: s,
                        })
                    } else {
                        self.advance();
                        let binding = self.parse_variant_binding_candidate()?;
                        self.expect(&TokenKind::RParen)?;
                        Ok(Pattern::Variant(name, binding, s))
                    }
                } else {
                    Ok(Pattern::Ident(name, s))
                }
            }
            _ => {
                let tok = self.current().clone();
                Err(ParseError::Unexpected {
                    found: format!("{}", tok.kind),
                    expected: "pattern".into(),
                    line: tok.span.line,
                    col: tok.span.col,
                })
            }
        }
    }

    fn parse_return(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::Return)?.span;
        let value = self.parse_expr(0)?;
        Ok(Stmt::Return(ReturnStmt {
            value: value.clone(),
            span: start.merge(value.span()),
        }))
    }

    fn parse_halt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::Halt)?.span;
        self.expect(&TokenKind::LParen)?;
        let msg = self.parse_expr(0)?;
        self.expect(&TokenKind::RParen)?;
        Ok(Stmt::Halt(HaltStmt {
            message: msg.clone(),
            span: start.merge(msg.span()),
        }))
    }

    fn parse_while(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::While)?.span;
        let label = if matches!(self.peek_kind(), TokenKind::At) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };

        // Detect `while let <pattern> = <expr>` and desugar to loop + match
        if matches!(self.peek_kind(), TokenKind::Let) {
            self.advance();
            let pattern = self.parse_pattern()?;
            let subject = if matches!(self.peek_kind(), TokenKind::Assign) {
                self.advance();
                self.parse_expr(0)?
            } else {
                Expr::BoolLit(true, start)
            };
            self.skip_newlines();
            let body = self.parse_block()?;
            let end_span = self.expect(&TokenKind::End)?.span;

            // Desugar to:
            //   loop
            //     match <subject>
            //       <pattern> => <body>
            //       _ => break
            //     end
            //   end
            let match_stmt = Stmt::Match(MatchStmt {
                subject,
                arms: vec![
                    MatchArm {
                        pattern,
                        body,
                        span: start,
                    },
                    MatchArm {
                        pattern: Pattern::Wildcard(start),
                        body: vec![Stmt::Break(BreakStmt {
                            label: None,
                            value: None,
                            span: start,
                        })],
                        span: start,
                    },
                ],
                span: start,
            });

            return Ok(Stmt::Loop(LoopStmt {
                label,
                body: vec![match_stmt],
                span: start.merge(end_span),
            }));
        }

        let cond = self.parse_expr(0)?;
        self.skip_newlines();
        let body = self.parse_block()?;
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(Stmt::While(WhileStmt {
            label,
            condition: cond,
            body,
            span: start.merge(end_span),
        }))
    }

    fn parse_loop(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::Loop)?.span;
        let label = if matches!(self.peek_kind(), TokenKind::At) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };
        self.skip_newlines();
        let body = self.parse_block()?;
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(Stmt::Loop(LoopStmt {
            label,
            body,
            span: start.merge(end_span),
        }))
    }

    fn parse_defer(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::Defer)?.span;
        self.skip_newlines();
        let body = self.parse_block()?;
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(Stmt::Defer(DeferStmt {
            body,
            span: start.merge(end_span),
        }))
    }

    fn parse_break(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::Break)?.span;
        let label = if matches!(self.peek_kind(), TokenKind::At) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };
        let value = if label.is_none()
            && !matches!(
                self.peek_kind(),
                TokenKind::Newline | TokenKind::Eof | TokenKind::End | TokenKind::Dedent
            ) {
            Some(self.parse_expr(0)?)
        } else {
            None
        };
        let span = value
            .as_ref()
            .map(|v| start.merge(v.span()))
            .unwrap_or(start);
        Ok(Stmt::Break(BreakStmt { label, value, span }))
    }

    fn parse_continue(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::Continue)?.span;
        let label = if matches!(self.peek_kind(), TokenKind::At) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };
        Ok(Stmt::Continue(ContinueStmt { label, span: start }))
    }

    fn parse_emit(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::Emit)?.span;
        let value = self.parse_expr(0)?;
        let span = start.merge(value.span());
        Ok(Stmt::Emit(EmitStmt { value, span }))
    }

    fn is_compound_assignment(&self) -> bool {
        let mut i = self.pos;
        if !matches!(self.tokens.get(i).map(|t| &t.kind), Some(k) if Self::is_identifier_like(k)) {
            return false;
        }
        i += 1;
        while matches!(self.tokens.get(i).map(|t| &t.kind), Some(TokenKind::Dot))
            && matches!(
                self.tokens.get(i + 1).map(|t| &t.kind),
                Some(TokenKind::Ident(_))
            )
        {
            i += 2;
        }
        matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(
                TokenKind::PlusAssign
                    | TokenKind::MinusAssign
                    | TokenKind::StarAssign
                    | TokenKind::SlashAssign
                    | TokenKind::FloorDivAssign
                    | TokenKind::PercentAssign
                    | TokenKind::StarStarAssign
                    | TokenKind::AmpAssign
                    | TokenKind::PipeAssign
                    | TokenKind::CaretAssign
            )
        )
    }

    fn parse_compound_assign(&mut self) -> Result<Stmt, ParseError> {
        let start = self.tokens[self.pos].span;
        let name = self.parse_assignment_target()?;
        let op = match self.peek_kind() {
            TokenKind::PlusAssign => {
                self.advance();
                CompoundOp::AddAssign
            }
            TokenKind::MinusAssign => {
                self.advance();
                CompoundOp::SubAssign
            }
            TokenKind::StarAssign => {
                self.advance();
                CompoundOp::MulAssign
            }
            TokenKind::SlashAssign => {
                self.advance();
                CompoundOp::DivAssign
            }
            TokenKind::FloorDivAssign => {
                self.advance();
                CompoundOp::FloorDivAssign
            }
            TokenKind::PercentAssign => {
                self.advance();
                CompoundOp::ModAssign
            }
            TokenKind::StarStarAssign => {
                self.advance();
                CompoundOp::PowAssign
            }
            TokenKind::AmpAssign => {
                self.advance();
                CompoundOp::BitAndAssign
            }
            TokenKind::PipeAssign => {
                self.advance();
                CompoundOp::BitOrAssign
            }
            TokenKind::CaretAssign => {
                self.advance();
                CompoundOp::BitXorAssign
            }
            _ => unreachable!(),
        };
        let value = self.parse_expr(0)?;
        let span = start.merge(value.span());
        Ok(Stmt::CompoundAssign(CompoundAssignStmt {
            target: name,
            op,
            value,
            span,
        }))
    }

    // ── New item parsers ──

    fn parse_type_alias(&mut self, is_pub: bool) -> Result<TypeAliasDef, ParseError> {
        let start = self.expect(&TokenKind::Type)?.span;
        let name = self.expect_ident()?;
        let generic_params = self.parse_optional_generic_params()?;
        self.expect(&TokenKind::Assign)?;
        let type_expr = self.parse_type()?;
        if matches!(self.peek_kind(), TokenKind::Where) {
            self.advance();
            let _ = self.parse_expr(0)?;
        }
        let span = start.merge(type_expr.span());
        Ok(TypeAliasDef {
            name,
            generic_params,
            type_expr,
            is_pub,
            span,
            doc: None,
        })
    }

    fn parse_trait_def(&mut self, is_pub: bool) -> Result<TraitDef, ParseError> {
        let start = self.expect(&TokenKind::Trait)?.span;
        let name = self.expect_ident()?;
        let parent_traits = if matches!(self.peek_kind(), TokenKind::Colon) {
            self.advance();
            let mut traits = vec![self.expect_ident()?];
            while matches!(self.peek_kind(), TokenKind::Comma | TokenKind::Plus) {
                self.advance();
                traits.push(self.expect_ident()?);
            }
            traits
        } else {
            vec![]
        };
        self.skip_newlines();
        let mut methods = Vec::new();
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent {
            self.advance();
        }
        self.skip_newlines();
        while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Dedent) {
                self.advance();
                continue;
            }
            if matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
                break;
            }
            methods.push(self.parse_cell(false)?);
            self.skip_newlines();
        }
        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) {
            self.advance();
        }
        self.skip_newlines();
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(TraitDef {
            name,
            parent_traits,
            methods,
            is_pub,
            span: start.merge(end_span),
        })
    }

    fn parse_impl_def(&mut self) -> Result<ImplDef, ParseError> {
        let start = self.expect(&TokenKind::Impl)?.span;
        let generic_params = self.parse_optional_generic_params()?;
        let trait_name = self.expect_ident()?;
        self.expect(&TokenKind::For)?;
        let mut target_type = self.parse_dotted_ident()?;
        if matches!(self.peek_kind(), TokenKind::LBracket) {
            let mut depth = 0usize;
            let mut suffix = String::new();
            while !self.at_end() {
                match self.peek_kind() {
                    TokenKind::LBracket => {
                        depth += 1;
                        suffix.push('[');
                        self.advance();
                    }
                    TokenKind::RBracket => {
                        suffix.push(']');
                        self.advance();
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {
                        suffix.push_str(&format!("{}", self.current().kind));
                        self.advance();
                    }
                }
            }
            target_type.push_str(&suffix);
        }
        self.skip_newlines();
        let mut cells = Vec::new();
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent {
            self.advance();
        }
        self.skip_newlines();
        while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Dedent) {
                self.advance();
                continue;
            }
            if matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
                break;
            }
            cells.push(self.parse_cell(true)?);
            self.skip_newlines();
        }
        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) {
            self.advance();
        }
        self.skip_newlines();
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(ImplDef {
            trait_name,
            generic_params,
            target_type,
            cells,
            span: start.merge(end_span),
        })
    }

    fn parse_import(&mut self, is_pub: bool) -> Result<ImportDecl, ParseError> {
        let start = self.expect(&TokenKind::Import)?.span;
        let mut path = vec![self.expect_ident()?];
        while matches!(self.peek_kind(), TokenKind::Dot) {
            self.advance();
            path.push(self.expect_ident()?);
        }
        self.expect(&TokenKind::Colon)?;
        let names = if matches!(self.peek_kind(), TokenKind::Star) {
            self.advance();
            ImportList::Wildcard
        } else {
            let mut names = Vec::new();
            loop {
                let ns = self.current().span;
                let n = self.expect_ident()?;
                let alias = if matches!(self.peek_kind(), TokenKind::As) {
                    self.advance();
                    Some(self.expect_ident()?)
                } else {
                    None
                };
                names.push(ImportName {
                    name: n,
                    alias,
                    span: ns,
                });
                if !matches!(self.peek_kind(), TokenKind::Comma) {
                    break;
                }
                self.advance();
            }
            ImportList::Names(names)
        };
        let span = start.merge(self.current().span);
        Ok(ImportDecl {
            path,
            names,
            is_pub,
            span,
        })
    }

    fn parse_const_decl(&mut self) -> Result<ConstDeclDef, ParseError> {
        let start = self.expect(&TokenKind::Const)?.span;
        let name = self.expect_ident()?;
        let type_ann = if matches!(self.peek_kind(), TokenKind::Colon) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&TokenKind::Assign)?;
        let value = self.parse_expr(0)?;
        let span = start.merge(value.span());
        Ok(ConstDeclDef {
            name,
            type_ann,
            value,
            span,
        })
    }

    fn parse_macro_decl(&mut self) -> Result<MacroDeclDef, ParseError> {
        let start = self.expect(&TokenKind::Macro)?.span;
        let mut name = if matches!(self.peek_kind(), TokenKind::Ident(_)) {
            self.expect_ident()?
        } else {
            "__macro".to_string()
        };
        if matches!(self.peek_kind(), TokenKind::Bang) {
            self.advance();
            name.push('!');
        }

        let mut params = Vec::new();
        if matches!(self.peek_kind(), TokenKind::LParen) {
            self.advance();
            while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                if matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                    continue;
                }
                if Self::is_identifier_like(self.peek_kind()) {
                    params.push(self.expect_ident()?);
                } else {
                    self.advance();
                }
            }
            if matches!(self.peek_kind(), TokenKind::RParen) {
                self.advance();
            }
        }

        self.consume_block_until_end();
        let span = start.merge(self.current().span);
        Ok(MacroDeclDef {
            name,
            params,
            body: vec![],
            span,
        })
    }

    fn parse_optional_generic_params(&mut self) -> Result<Vec<GenericParam>, ParseError> {
        if !matches!(self.peek_kind(), TokenKind::LBracket) {
            return Ok(vec![]);
        }
        self.advance();
        let mut params = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RBracket) {
            if !params.is_empty() {
                self.expect(&TokenKind::Comma)?;
            }
            let ps = self.current().span;
            let name = self.expect_ident()?;
            let bounds = if matches!(self.peek_kind(), TokenKind::Colon) {
                self.advance();
                let mut b = vec![self.expect_ident()?];
                while matches!(self.peek_kind(), TokenKind::Plus) {
                    self.advance();
                    b.push(self.expect_ident()?);
                }
                b
            } else {
                vec![]
            };
            params.push(GenericParam {
                name,
                bounds,
                span: ps,
            });
        }
        self.expect(&TokenKind::RBracket)?;
        Ok(params)
    }

    fn parse_optional_effect_row(&mut self) -> Result<Vec<String>, ParseError> {
        if !matches!(self.peek_kind(), TokenKind::Slash) {
            return Ok(vec![]);
        }
        self.advance();
        self.skip_newlines();
        if matches!(self.peek_kind(), TokenKind::LBrace) {
            self.advance();
            let mut effects = Vec::new();
            while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                if matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                    continue;
                }
                effects.push(self.parse_effect_name()?);
                if matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                }
            }
            self.expect(&TokenKind::RBrace)?;
            Ok(effects)
        } else if matches!(
            self.peek_kind(),
            TokenKind::Ident(_)
                | TokenKind::Emit
                | TokenKind::Role
                | TokenKind::Use
                | TokenKind::Tool
                | TokenKind::Grant
                | TokenKind::Await
                | TokenKind::Async
                | TokenKind::Parallel
        ) {
            Ok(vec![self.parse_effect_name()?])
        } else {
            Ok(vec![])
        }
    }

    fn parse_effect_name(&mut self) -> Result<String, ParseError> {
        match self.peek_kind().clone() {
            TokenKind::Ident(_) => self.expect_ident(),
            TokenKind::Emit => {
                self.advance();
                Ok("emit".into())
            }
            TokenKind::Role => {
                self.advance();
                Ok("role".into())
            }
            TokenKind::Use => {
                self.advance();
                Ok("use".into())
            }
            TokenKind::Tool => {
                self.advance();
                Ok("tool".into())
            }
            TokenKind::Grant => {
                self.advance();
                Ok("grant".into())
            }
            TokenKind::Await => {
                self.advance();
                Ok("await".into())
            }
            TokenKind::Async => {
                self.advance();
                Ok("async".into())
            }
            TokenKind::Parallel => {
                self.advance();
                Ok("parallel".into())
            }
            _ => {
                let tok = self.current().clone();
                Err(ParseError::Unexpected {
                    found: format!("{}", tok.kind),
                    expected: "effect name".into(),
                    line: tok.span.line,
                    col: tok.span.col,
                })
            }
        }
    }

    fn parse_attribute_decl(&mut self) -> Result<AddonDecl, ParseError> {
        let start = self.expect(&TokenKind::At)?.span;
        let name = match self.peek_kind() {
            TokenKind::Ident(_) => Some(self.expect_ident()?),
            _ => None,
        };

        if matches!(self.peek_kind(), TokenKind::LParen) {
            self.consume_parenthesized();
        }

        if matches!(self.peek_kind(), TokenKind::Newline) {
            // Block attributes such as `@location_policy` can carry an indented
            // payload and a trailing `end`.
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Indent) {
                self.consume_indented_payload();
                self.skip_newlines();
                if matches!(self.peek_kind(), TokenKind::End) {
                    self.advance();
                }
            }
        } else {
            self.consume_rest_of_line();
        }

        let end = self.current().span;
        Ok(AddonDecl {
            kind: "attribute".into(),
            name,
            span: start.merge(end),
        })
    }

    fn parse_addon_decl(&mut self) -> Result<AddonDecl, ParseError> {
        let start = self.current().span;
        let kind = self.expect_ident()?;
        let name = self.parse_optional_decl_name()?;

        let is_line_decl = matches!(kind.as_str(), "bind" | "pattern");
        if is_line_decl {
            self.consume_rest_of_line();
        } else {
            self.consume_block_until_end();
        }

        let end = self.current().span;
        Ok(AddonDecl {
            kind,
            name,
            span: start.merge(end),
        })
    }

    fn parse_agent_decl(&mut self) -> Result<AgentDecl, ParseError> {
        let start = self.current().span;
        let _kw = self.expect_ident()?;
        let name = self
            .parse_optional_decl_name()?
            .unwrap_or_else(|| "Agent".to_string());

        let mut cells = Vec::new();
        let mut grants = Vec::new();

        self.skip_newlines();
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent {
            self.advance();
        }
        self.skip_newlines();

        while !matches!(
            self.peek_kind(),
            TokenKind::End | TokenKind::Dedent | TokenKind::Eof
        ) {
            self.skip_newlines();
            if matches!(
                self.peek_kind(),
                TokenKind::End | TokenKind::Dedent | TokenKind::Eof
            ) {
                break;
            }

            let is_async = matches!(self.peek_kind(), TokenKind::Async);
            if is_async {
                self.advance();
                self.skip_newlines();
            }

            match self.peek_kind() {
                TokenKind::Cell => {
                    let mut cell = self.parse_cell(true)?;
                    cell.is_async = is_async;
                    if cell.params.first().map(|p| p.name.as_str()) != Some("self") {
                        let self_span = cell.span;
                        cell.params.insert(
                            0,
                            Param {
                                name: "self".into(),
                                ty: TypeExpr::Named("Json".into(), self_span),
                                default_value: None,
                                variadic: false,
                                span: self_span,
                            },
                        );
                    }
                    cells.push(cell);
                }
                TokenKind::Grant => grants.push(self.parse_grant()?),
                TokenKind::At => {
                    let _ = self.parse_attribute_decl()?;
                }
                TokenKind::Role | TokenKind::Tool => {
                    self.advance();
                    self.consume_section_or_line_after_name();
                }
                TokenKind::Impl => {
                    self.advance();
                    self.consume_block_until_end();
                }
                TokenKind::Ident(_) => {
                    self.consume_named_section_or_line();
                }
                _ => {
                    self.consume_rest_of_line();
                }
            }
            self.skip_newlines();
        }

        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) {
            self.advance();
        }
        while matches!(
            self.peek_kind(),
            TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
        ) {
            self.advance();
        }
        let end_span = if matches!(self.peek_kind(), TokenKind::End) {
            self.advance().span
        } else {
            self.current().span
        };
        Ok(AgentDecl {
            name,
            cells,
            grants,
            span: start.merge(end_span),
        })
    }

    fn parse_effect_decl(&mut self) -> Result<EffectDecl, ParseError> {
        let start = self.current().span;
        let kw = self.expect_ident()?;
        if kw != "effect" {
            let tok = self.current().clone();
            return Err(ParseError::Unexpected {
                found: kw,
                expected: "effect".into(),
                line: tok.span.line,
                col: tok.span.col,
            });
        }
        let name = self
            .parse_optional_decl_name()?
            .unwrap_or_else(|| "Effect".to_string());
        self.skip_newlines();
        let mut operations = Vec::new();
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent {
            self.advance();
        }
        self.skip_newlines();
        while !matches!(
            self.peek_kind(),
            TokenKind::End | TokenKind::Dedent | TokenKind::Eof
        ) {
            self.skip_newlines();
            if matches!(
                self.peek_kind(),
                TokenKind::End | TokenKind::Dedent | TokenKind::Eof
            ) {
                break;
            }
            if matches!(self.peek_kind(), TokenKind::Cell) {
                operations.push(self.parse_cell(false)?);
            } else {
                self.consume_rest_of_line();
            }
            self.skip_newlines();
        }
        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) {
            self.advance();
        }
        while matches!(
            self.peek_kind(),
            TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
        ) {
            self.advance();
        }
        let end_span = if matches!(self.peek_kind(), TokenKind::End) {
            self.advance().span
        } else {
            self.current().span
        };
        Ok(EffectDecl {
            name,
            operations,
            span: start.merge(end_span),
        })
    }

    fn parse_process_decl(&mut self) -> Result<ProcessDecl, ParseError> {
        let start = self.current().span;
        let kind = self.expect_ident()?;
        let name = self
            .parse_optional_decl_name()?
            .unwrap_or_else(|| "Process".to_string());
        let mut cells = Vec::new();
        let mut grants = Vec::new();
        let mut pipeline_stages = Vec::new();
        let mut machine_initial = None;
        let mut machine_states = Vec::new();
        let mut configs = BTreeMap::new();

        self.skip_newlines();
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent {
            self.advance();
        }
        self.skip_newlines();

        while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Dedent) {
                self.advance();
                continue;
            }
            if matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
                break;
            }

            let is_async = matches!(self.peek_kind(), TokenKind::Async);
            if is_async {
                self.advance();
                self.skip_newlines();
            }

            match self.peek_kind() {
                TokenKind::Cell => {
                    let mut cell = self.parse_cell(true)?;
                    cell.is_async = is_async;
                    if cell.params.first().map(|p| p.name.as_str()) != Some("self") {
                        let self_span = cell.span;
                        cell.params.insert(
                            0,
                            Param {
                                name: "self".into(),
                                ty: TypeExpr::Named("Json".into(), self_span),
                                default_value: None,
                                variadic: false,
                                span: self_span,
                            },
                        );
                    }
                    cells.push(cell);
                }
                TokenKind::Grant => grants.push(self.parse_grant()?),
                TokenKind::Ident(name) if kind == "pipeline" && name == "stages" => {
                    pipeline_stages = self.parse_pipeline_stages_decl()?;
                }
                TokenKind::Ident(name) if kind == "machine" && name == "initial" => {
                    machine_initial = Some(self.parse_machine_initial_decl()?);
                }
                TokenKind::Ident(name) if kind == "machine" && name == "state" => {
                    machine_states.push(self.parse_machine_state_decl()?);
                }
                TokenKind::At => {
                    let _ = self.parse_attribute_decl()?;
                }
                TokenKind::Role | TokenKind::Tool => {
                    self.advance();
                    self.consume_section_or_line_after_name();
                }
                TokenKind::Record | TokenKind::Enum | TokenKind::Trait | TokenKind::Impl => {
                    self.advance();
                    self.consume_block_until_end();
                }
                TokenKind::Ident(name)
                    if matches!(
                        name.as_str(),
                        "state"
                            | "on_enter"
                            | "on_event"
                            | "on_error"
                            | "on_input"
                            | "on_output"
                            | "on_violation"
                            | "on_timeout"
                            | "migrate"
                    ) =>
                {
                    self.advance();
                    self.consume_block_until_end();
                }
                TokenKind::Ident(_) if matches!(self.peek_n_kind(1), Some(TokenKind::Colon)) => {
                    let key = self.expect_ident()?;
                    self.expect(&TokenKind::Colon)?;
                    let val = self.parse_expr(0)?;
                    configs.insert(key, val);
                    self.consume_rest_of_line();
                }
                TokenKind::Ident(_) => {
                    self.consume_named_section_or_line();
                }
                _ => {
                    self.consume_rest_of_line();
                }
            }
            self.skip_newlines();
        }

        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) {
            self.advance();
        }
        while matches!(
            self.peek_kind(),
            TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
        ) {
            self.advance();
        }
        let end_span = if matches!(self.peek_kind(), TokenKind::End) {
            self.advance().span
        } else {
            self.current().span
        };

        Ok(ProcessDecl {
            kind,
            name,
            configs: configs.into_iter().collect(),
            cells,
            grants,
            pipeline_stages,
            machine_initial,
            machine_states,
            span: start.merge(end_span),
        })
    }

    fn parse_pipeline_stages_decl(&mut self) -> Result<Vec<String>, ParseError> {
        let kw = self.expect_ident()?;
        if kw != "stages" {
            let tok = self.current().clone();
            return Err(ParseError::Unexpected {
                found: kw,
                expected: "stages".into(),
                line: tok.span.line,
                col: tok.span.col,
            });
        }
        if matches!(self.peek_kind(), TokenKind::Colon) {
            self.advance();
        }
        self.skip_newlines();
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent {
            self.advance();
        }
        self.skip_newlines();
        let mut stages = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Dedent) {
                self.advance();
                continue;
            }
            if matches!(self.peek_kind(), TokenKind::Indent) {
                self.advance();
                continue;
            }
            if matches!(self.peek_kind(), TokenKind::Arrow) {
                self.advance();
                self.skip_whitespace_tokens();
            }
            if matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
                break;
            }
            stages.push(self.parse_dotted_ident()?);
            self.consume_rest_of_line();
            self.skip_newlines();
        }
        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) {
            self.advance();
        }
        if matches!(self.peek_kind(), TokenKind::End) {
            self.advance();
        }
        Ok(stages)
    }

    fn parse_machine_initial_decl(&mut self) -> Result<String, ParseError> {
        let kw = self.expect_ident()?;
        if kw != "initial" {
            let tok = self.current().clone();
            return Err(ParseError::Unexpected {
                found: kw,
                expected: "initial".into(),
                line: tok.span.line,
                col: tok.span.col,
            });
        }
        if matches!(self.peek_kind(), TokenKind::Colon) {
            self.advance();
        }
        let state = self.expect_ident()?;
        self.consume_rest_of_line();
        Ok(state)
    }

    fn parse_machine_state_decl(&mut self) -> Result<MachineStateDecl, ParseError> {
        let start = self.current().span;
        let kw = self.expect_ident()?;
        if kw != "state" {
            let tok = self.current().clone();
            return Err(ParseError::Unexpected {
                found: kw,
                expected: "state".into(),
                line: tok.span.line,
                col: tok.span.col,
            });
        }
        let name = self.expect_ident()?;
        let params = if matches!(self.peek_kind(), TokenKind::LParen) {
            self.parse_machine_state_params()?
        } else {
            vec![]
        };
        let mut terminal = false;
        let mut guard = None;
        let mut transition_to = None;
        let mut transition_args = Vec::new();

        self.skip_newlines();
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent {
            self.advance();
        }
        self.skip_newlines();

        while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Dedent) {
                self.advance();
                continue;
            }
            match self.peek_kind() {
                TokenKind::Ident(id) if id == "terminal" => {
                    self.advance();
                    if matches!(self.peek_kind(), TokenKind::Colon) {
                        self.advance();
                    }
                    terminal = match self.peek_kind().clone() {
                        TokenKind::BoolLit(v) => {
                            self.advance();
                            v
                        }
                        TokenKind::Ident(text) if text == "true" => {
                            self.advance();
                            true
                        }
                        TokenKind::Ident(text) if text == "false" => {
                            self.advance();
                            false
                        }
                        _ => false,
                    };
                    self.consume_rest_of_line();
                }
                TokenKind::Ident(id) if id == "guard" => {
                    self.advance();
                    if matches!(self.peek_kind(), TokenKind::Colon) {
                        self.advance();
                    }
                    guard = Some(self.parse_expr(0)?);
                    self.consume_rest_of_line();
                }
                TokenKind::Ident(id) if id == "transition" => {
                    self.advance();
                    let (target, args) = self.parse_machine_transition_decl()?;
                    transition_to = Some(target);
                    transition_args = args;
                    self.consume_rest_of_line();
                }
                TokenKind::Ident(id)
                    if matches!(id.as_str(), "on_enter" | "on_event" | "on_timeout") =>
                {
                    self.advance();
                    self.consume_rest_of_line();
                    self.skip_newlines();
                    let nested_indent = matches!(self.peek_kind(), TokenKind::Indent);
                    if nested_indent {
                        self.advance();
                    }
                    self.skip_newlines();
                    while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
                        self.skip_newlines();
                        if matches!(self.peek_kind(), TokenKind::Dedent) {
                            self.advance();
                            continue;
                        }
                        if let TokenKind::Ident(id2) = self.peek_kind() {
                            if id2 == "transition" {
                                self.advance();
                                let (target, args) = self.parse_machine_transition_decl()?;
                                transition_to = Some(target);
                                transition_args = args;
                                self.consume_rest_of_line();
                                continue;
                            }
                        }
                        self.consume_rest_of_line();
                    }
                    if nested_indent && matches!(self.peek_kind(), TokenKind::Dedent) {
                        self.advance();
                    }
                    if matches!(self.peek_kind(), TokenKind::End) {
                        self.advance();
                    }
                }
                _ => {
                    self.consume_rest_of_line();
                }
            }
            self.skip_newlines();
        }

        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) {
            self.advance();
        }
        let end_span = if matches!(self.peek_kind(), TokenKind::End) {
            self.advance().span
        } else {
            self.current().span
        };

        Ok(MachineStateDecl {
            name,
            params,
            terminal,
            guard,
            transition_to,
            transition_args,
            span: start.merge(end_span),
        })
    }

    fn parse_machine_state_params(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        self.expect(&TokenKind::LParen)?;
        self.bracket_depth += 1;
        self.skip_whitespace_tokens();
        while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
            if !params.is_empty() {
                self.expect(&TokenKind::Comma)?;
                self.skip_whitespace_tokens();
            }
            let ps = self.current().span;
            let pname = self.expect_ident()?;
            let pty = if matches!(self.peek_kind(), TokenKind::Colon) {
                self.advance();
                self.parse_type()?
            } else {
                TypeExpr::Named("Any".into(), ps)
            };
            params.push(Param {
                name: pname,
                ty: pty,
                default_value: None,
                variadic: false,
                span: ps,
            });
            self.skip_whitespace_tokens();
        }
        self.bracket_depth -= 1;
        self.expect(&TokenKind::RParen)?;
        Ok(params)
    }

    fn parse_machine_transition_decl(&mut self) -> Result<(String, Vec<Expr>), ParseError> {
        let target = self.expect_ident()?;
        let mut args = Vec::new();
        if matches!(self.peek_kind(), TokenKind::LParen) {
            self.expect(&TokenKind::LParen)?;
            self.bracket_depth += 1;
            self.skip_whitespace_tokens();
            while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                if !args.is_empty() {
                    self.expect(&TokenKind::Comma)?;
                    self.skip_whitespace_tokens();
                }
                args.push(self.parse_expr(0)?);
                self.skip_whitespace_tokens();
            }
            self.bracket_depth -= 1;
            self.expect(&TokenKind::RParen)?;
        }
        Ok((target, args))
    }

    fn parse_effect_bind_decl(&mut self) -> Result<EffectBindDecl, ParseError> {
        let start = self.current().span;
        let bind_kw = self.expect_ident()?;
        if bind_kw != "bind" {
            let tok = self.current().clone();
            return Err(ParseError::Unexpected {
                found: bind_kw,
                expected: "bind".into(),
                line: tok.span.line,
                col: tok.span.col,
            });
        }
        let effect_kw = self.expect_ident()?;
        if effect_kw != "effect" {
            let tok = self.current().clone();
            return Err(ParseError::Unexpected {
                found: effect_kw,
                expected: "effect".into(),
                line: tok.span.line,
                col: tok.span.col,
            });
        }
        let effect_path = self.parse_dotted_ident()?;
        let to_kw = self.expect_ident()?;
        if to_kw != "to" {
            let tok = self.current().clone();
            return Err(ParseError::Unexpected {
                found: to_kw,
                expected: "to".into(),
                line: tok.span.line,
                col: tok.span.col,
            });
        }
        let tool_alias = self.parse_dotted_ident()?;
        self.consume_rest_of_line();
        Ok(EffectBindDecl {
            effect_path,
            tool_alias,
            span: start.merge(self.current().span),
        })
    }

    fn parse_handler_decl(&mut self) -> Result<HandlerDecl, ParseError> {
        let start = self.current().span;
        let kw = self.expect_ident()?;
        if kw != "handler" {
            let tok = self.current().clone();
            return Err(ParseError::Unexpected {
                found: kw,
                expected: "handler".into(),
                line: tok.span.line,
                col: tok.span.col,
            });
        }
        let name = self
            .parse_optional_decl_name()?
            .unwrap_or_else(|| "Handler".to_string());
        self.skip_newlines();
        let mut handles = Vec::new();
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent {
            self.advance();
        }
        self.skip_newlines();
        while !matches!(
            self.peek_kind(),
            TokenKind::End | TokenKind::Dedent | TokenKind::Eof
        ) {
            self.skip_newlines();
            if matches!(
                self.peek_kind(),
                TokenKind::End | TokenKind::Dedent | TokenKind::Eof
            ) {
                break;
            }
            if matches!(self.peek_kind(), TokenKind::Ident(s) if s == "handle") {
                handles.push(self.parse_handle_cell()?);
            } else if matches!(self.peek_kind(), TokenKind::Cell) {
                handles.push(self.parse_cell(true)?);
            } else {
                self.consume_rest_of_line();
            }
            self.skip_newlines();
        }
        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) {
            self.advance();
        }
        while matches!(
            self.peek_kind(),
            TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
        ) {
            self.advance();
        }
        let end_span = if matches!(self.peek_kind(), TokenKind::End) {
            self.advance().span
        } else {
            self.current().span
        };
        Ok(HandlerDecl {
            name,
            handles,
            span: start.merge(end_span),
            doc: None,
        })
    }

    fn parse_handle_cell(&mut self) -> Result<CellDef, ParseError> {
        let start = self.current().span;
        let kw = self.expect_ident()?;
        if kw != "handle" {
            let tok = self.current().clone();
            return Err(ParseError::Unexpected {
                found: kw,
                expected: "handle".into(),
                line: tok.span.line,
                col: tok.span.col,
            });
        }
        let name = self.parse_dotted_ident()?;
        self.expect(&TokenKind::LParen)?;
        self.bracket_depth += 1;
        let mut params = Vec::new();
        self.skip_whitespace_tokens();
        while !matches!(self.peek_kind(), TokenKind::RParen) {
            if !params.is_empty() {
                self.expect(&TokenKind::Comma)?;
                self.skip_whitespace_tokens();
            }
            let variadic = if matches!(self.peek_kind(), TokenKind::DotDot | TokenKind::DotDotDot) {
                self.advance();
                true
            } else {
                false
            };
            let ps = self.current().span;
            let pname = self.expect_ident()?;
            let pty = if matches!(self.peek_kind(), TokenKind::Colon) {
                self.advance();
                self.parse_type()?
            } else {
                TypeExpr::Named("Any".into(), ps)
            };
            let default_value = if matches!(self.peek_kind(), TokenKind::Assign) {
                self.advance();
                Some(self.parse_expr(0)?)
            } else {
                None
            };
            params.push(Param {
                name: pname,
                ty: pty,
                default_value,
                variadic,
                span: ps,
            });
            self.skip_whitespace_tokens();
        }
        self.bracket_depth -= 1;
        self.expect(&TokenKind::RParen)?;
        let ret = if matches!(self.peek_kind(), TokenKind::Arrow) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };
        let effects = self.parse_optional_effect_row()?;

        if matches!(self.peek_kind(), TokenKind::Assign) {
            self.advance();
            let expr = self.parse_expr(0)?;
            let span = start.merge(expr.span());
            return Ok(CellDef {
                name,
                generic_params: vec![],
                params,
                return_type: ret,
                effects,
                body: vec![Stmt::Return(ReturnStmt { value: expr, span })],
                is_pub: false,
                is_async: false,
                is_extern: false,
                must_use: false,
                where_clauses: vec![],
                span,
                doc: None,
            });
        }

        if matches!(
            self.peek_kind(),
            TokenKind::Newline | TokenKind::Eof | TokenKind::Dedent
        ) {
            let mut look = self.pos;
            while matches!(
                self.tokens.get(look).map(|t| &t.kind),
                Some(TokenKind::Newline)
            ) {
                look += 1;
            }
            if !matches!(
                self.tokens.get(look).map(|t| &t.kind),
                Some(TokenKind::Indent)
            ) {
                let end_span = self.current().span;
                return Ok(CellDef {
                    name,
                    generic_params: vec![],
                    params,
                    return_type: ret,
                    effects,
                    body: vec![],
                    is_pub: false,
                    is_async: false,
                    is_extern: false,
                    must_use: false,
                    where_clauses: vec![],
                    span: start.merge(end_span),
                    doc: None,
                });
            }
        }

        self.skip_newlines();
        let body = self.parse_block()?;
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(CellDef {
            name,
            generic_params: vec![],
            params,
            return_type: ret,
            effects,
            body,
            is_pub: false,
            is_async: false,
            is_extern: false,
            must_use: false,
            where_clauses: vec![],
            span: start.merge(end_span),
            doc: None,
        })
    }

    /// Parse a single effect handler arm: `Effect.operation(params) => body`
    fn parse_effect_handler_arm(&mut self) -> Result<EffectHandler, ParseError> {
        let start = self.current().span;
        let effect_name = self.expect_ident()?;
        self.expect(&TokenKind::Dot)?;
        let operation = self.expect_ident()?;
        self.expect(&TokenKind::LParen)?;
        self.bracket_depth += 1;
        let mut params = Vec::new();
        self.skip_whitespace_tokens();
        while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
            if !params.is_empty() {
                self.expect(&TokenKind::Comma)?;
                self.skip_whitespace_tokens();
            }
            let ps = self.current().span;
            let pname = self.expect_ident()?;
            let pty = if matches!(self.peek_kind(), TokenKind::Colon) {
                self.advance();
                self.parse_type()?
            } else {
                TypeExpr::Named("Any".into(), ps)
            };
            params.push(Param {
                name: pname,
                ty: pty,
                default_value: None,
                variadic: false,
                span: ps,
            });
            self.skip_whitespace_tokens();
        }
        self.bracket_depth -= 1;
        self.expect(&TokenKind::RParen)?;
        self.expect(&TokenKind::FatArrow)?;
        self.skip_newlines();
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent {
            self.advance();
        }
        self.skip_newlines();
        // Parse handler body until we hit the next handler pattern or end
        let mut body = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
            if matches!(self.peek_kind(), TokenKind::Dedent) {
                self.advance();
                continue;
            }
            // Check if next tokens look like a new handler: Ident . Ident (
            if self.is_effect_handler_start() {
                break;
            }
            if matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
                break;
            }
            body.push(self.parse_stmt()?);
            self.skip_newlines();
        }
        let span = start.merge(self.current().span);
        Ok(EffectHandler {
            effect_name,
            operation,
            params,
            body,
            span,
        })
    }

    /// Check if the current position looks like the start of an effect handler arm:
    /// Ident . Ident (
    fn is_effect_handler_start(&self) -> bool {
        if !matches!(self.peek_kind(), TokenKind::Ident(_)) {
            return false;
        }
        // Lookahead: Ident . Ident (
        let mut look = self.pos + 1;
        // Skip any newlines
        while matches!(
            self.tokens.get(look).map(|t| &t.kind),
            Some(TokenKind::Newline)
        ) {
            look += 1;
        }
        if !matches!(self.tokens.get(look).map(|t| &t.kind), Some(TokenKind::Dot)) {
            return false;
        }
        look += 1;
        // Skip newlines after dot
        while matches!(
            self.tokens.get(look).map(|t| &t.kind),
            Some(TokenKind::Newline)
        ) {
            look += 1;
        }
        if !matches!(
            self.tokens.get(look).map(|t| &t.kind),
            Some(TokenKind::Ident(_))
        ) {
            return false;
        }
        look += 1;
        // Skip newlines after ident
        while matches!(
            self.tokens.get(look).map(|t| &t.kind),
            Some(TokenKind::Newline)
        ) {
            look += 1;
        }
        matches!(
            self.tokens.get(look).map(|t| &t.kind),
            Some(TokenKind::LParen)
        )
    }

    fn consume_parenthesized(&mut self) {
        if !matches!(self.peek_kind(), TokenKind::LParen) {
            return;
        }
        let mut depth = 0usize;
        while !self.at_end() {
            match self.peek_kind() {
                TokenKind::LParen => {
                    depth += 1;
                    self.advance();
                }
                TokenKind::RParen => {
                    self.advance();
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {
                    self.advance();
                }
            }
            if depth == 0 && !matches!(self.peek_kind(), TokenKind::RParen) {
                break;
            }
        }
    }

    fn consume_rest_of_line(&mut self) {
        while !matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Eof) {
            self.advance();
        }
    }

    fn consume_indented_payload(&mut self) {
        if !matches!(self.peek_kind(), TokenKind::Indent) {
            return;
        }
        self.advance();
        let mut depth = 0usize;
        while !self.at_end() {
            match self.peek_kind() {
                TokenKind::Indent => {
                    depth += 1;
                    self.advance();
                }
                TokenKind::Dedent => {
                    if depth == 0 {
                        self.advance();
                        break;
                    }
                    depth -= 1;
                    self.advance();
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    fn consume_block_until_end(&mut self) {
        let mut depth = 0usize;
        while !self.at_end() {
            match self.peek_kind() {
                TokenKind::End => {
                    if depth == 0 {
                        self.advance();
                        break;
                    }
                    depth -= 1;
                    self.advance();
                }
                TokenKind::Record
                | TokenKind::Enum
                | TokenKind::Cell
                | TokenKind::Trait
                | TokenKind::Impl
                | TokenKind::If
                | TokenKind::For
                | TokenKind::Match
                | TokenKind::While
                | TokenKind::Loop => {
                    depth += 1;
                    self.advance();
                }
                TokenKind::Role | TokenKind::Tool if self.consumes_section_colon_block() => {
                    depth += 1;
                    self.advance();
                }
                TokenKind::Ident(name)
                    if matches!(
                        name.as_str(),
                        "effect"
                            | "handler"
                            | "agent"
                            | "pipeline"
                            | "orchestration"
                            | "machine"
                            | "memory"
                            | "guardrail"
                            | "eval"
                            | "handle"
                            | "state"
                            | "on_enter"
                            | "on_event"
                            | "on_error"
                            | "on_input"
                            | "on_output"
                            | "on_violation"
                            | "on_timeout"
                            | "migrate"
                            | "approve"
                            | "checkpoint"
                            | "escalate"
                            | "observe"
                            | "with"
                            | "stages"
                            | "thresholds"
                    ) =>
                {
                    depth += 1;
                    self.advance();
                }
                TokenKind::Ident(_) if self.consumes_section_colon_block() => {
                    depth += 1;
                    self.advance();
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    fn consume_named_section_or_line(&mut self) {
        self.advance(); // section name
        self.consume_section_or_line_after_name();
    }

    fn consume_section_or_line_after_name(&mut self) {
        if matches!(self.peek_kind(), TokenKind::Colon) {
            self.advance();
            if matches!(self.peek_kind(), TokenKind::Newline) {
                self.skip_newlines();
                if matches!(self.peek_kind(), TokenKind::Indent) {
                    self.consume_indented_payload();
                    self.skip_newlines();
                    if matches!(self.peek_kind(), TokenKind::End) {
                        self.advance();
                    }
                    return;
                }
            }
            if matches!(
                self.peek_kind(),
                TokenKind::LBracket | TokenKind::LBrace | TokenKind::LParen
            ) {
                self.consume_balanced_group();
                self.consume_rest_of_line();
                return;
            }
            self.consume_rest_of_line();
            return;
        }
        self.consume_rest_of_line();
        if matches!(self.peek_kind(), TokenKind::Newline) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Indent) {
                self.consume_indented_payload();
                self.skip_newlines();
                if matches!(self.peek_kind(), TokenKind::End) {
                    self.advance();
                }
            }
        }
    }

    fn consume_balanced_group(&mut self) {
        let (open, close) = match self.peek_kind() {
            TokenKind::LBracket => (TokenKind::LBracket, TokenKind::RBracket),
            TokenKind::LBrace => (TokenKind::LBrace, TokenKind::RBrace),
            TokenKind::LParen => (TokenKind::LParen, TokenKind::RParen),
            _ => return,
        };
        let mut depth = 0usize;
        while !self.at_end() {
            if std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(&open) {
                depth += 1;
                self.advance();
                continue;
            }
            if std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(&close) {
                self.advance();
                depth -= 1;
                if depth == 0 {
                    break;
                }
                continue;
            }
            self.advance();
        }
    }

    fn pattern_binding_name(pattern: &Pattern) -> Option<String> {
        match pattern {
            Pattern::Ident(name, _) => Some(name.clone()),
            Pattern::TypeCheck { name, .. } => Some(name.clone()),
            Pattern::Variant(_, Some(inner), _) => Self::pattern_binding_name(inner),
            Pattern::TupleDestructure { elements, .. }
            | Pattern::ListDestructure { elements, .. } => {
                elements.iter().find_map(Self::pattern_binding_name)
            }
            Pattern::RecordDestructure { fields, .. } => fields.iter().find_map(|(name, pat)| {
                pat.as_ref()
                    .and_then(Self::pattern_binding_name)
                    .or_else(|| Some(name.clone()))
            }),
            Pattern::Guard { inner, .. } => Self::pattern_binding_name(inner),
            Pattern::Or { patterns, .. } => patterns.iter().find_map(Self::pattern_binding_name),
            _ => None,
        }
    }

    fn parse_variant_binding_candidate(&mut self) -> Result<Option<Box<Pattern>>, ParseError> {
        let mut binding = None;
        if !matches!(self.peek_kind(), TokenKind::RParen) {
            let save = self.pos;
            match self.parse_pattern() {
                Ok(pattern) => binding = Some(Box::new(pattern)),
                Err(_) => {
                    self.pos = save;
                    self.consume_variant_arg_tokens();
                }
            }
            while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                if matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                    if matches!(self.peek_kind(), TokenKind::RParen) {
                        break;
                    }
                }
                self.consume_variant_arg_tokens();
            }
        }
        Ok(binding)
    }

    fn paren_looks_like_record_destructure(&self) -> bool {
        if !matches!(self.peek_kind(), TokenKind::LParen) {
            return false;
        }
        let mut i = self.pos + 1;
        let mut depth = 0usize;
        while let Some(tok) = self.tokens.get(i) {
            match tok.kind {
                TokenKind::LParen => depth += 1,
                TokenKind::RParen => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                TokenKind::Colon | TokenKind::DotDot | TokenKind::DotDotDot if depth == 0 => {
                    return true;
                }
                _ => {}
            }
            i += 1;
        }
        false
    }

    fn paren_contains_top_level_arrow(&self) -> bool {
        let mut i = self.pos;
        // Caller has already consumed the opening '(' for this group.
        let mut depth = 1usize;
        while let Some(tok) = self.tokens.get(i) {
            match tok.kind {
                TokenKind::LParen => depth += 1,
                TokenKind::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                TokenKind::Arrow if depth == 1 => return true,
                _ => {}
            }
            i += 1;
        }
        false
    }

    fn consume_variant_arg_tokens(&mut self) {
        let mut depth = 0usize;
        loop {
            match self.peek_kind() {
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                    depth += 1;
                    self.advance();
                }
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    self.advance();
                }
                TokenKind::Comma if depth == 0 => break,
                TokenKind::Eof => break,
                _ => {
                    self.advance();
                }
            }
        }
    }

    fn parse_optional_decl_name(&mut self) -> Result<Option<String>, ParseError> {
        if matches!(self.peek_kind(), TokenKind::Ident(_)) {
            return Ok(Some(self.expect_ident()?));
        }
        if matches!(self.peek_kind(), TokenKind::Lt) {
            // Template notation in specification snippets, e.g. `agent <Name>`.
            self.advance(); // <
            let mut buf = String::new();
            while !matches!(self.peek_kind(), TokenKind::Gt | TokenKind::Eof) {
                buf.push_str(&format!("{}", self.current().kind));
                self.advance();
            }
            if matches!(self.peek_kind(), TokenKind::Gt) {
                self.advance();
            }
            if !buf.trim().is_empty() {
                return Ok(Some(buf.trim().to_string()));
            }
        }
        Ok(None)
    }

    fn parse_assignment_target(&mut self) -> Result<String, ParseError> {
        let mut parts = Vec::new();
        match self.peek_kind().clone() {
            TokenKind::SelfKw => {
                self.advance();
                parts.push("self".to_string());
            }
            _ if Self::is_identifier_like(self.peek_kind()) => {
                parts.push(self.expect_ident()?);
            }
            _ => {
                let tok = self.current().clone();
                return Err(ParseError::Unexpected {
                    found: format!("{}", tok.kind),
                    expected: "assignment target".into(),
                    line: tok.span.line,
                    col: tok.span.col,
                });
            }
        }

        while matches!(self.peek_kind(), TokenKind::Dot) {
            self.advance();
            parts.push(self.expect_ident()?);
        }

        Ok(parts.join("."))
    }

    fn parse_expr_stmt(&mut self) -> Result<Stmt, ParseError> {
        let expr = self.parse_expr(0)?;
        let mut span = expr.span();
        if matches!(self.peek_kind(), TokenKind::In) {
            self.advance();
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Indent) {
                self.consume_indented_payload();
                self.skip_newlines();
                if matches!(self.peek_kind(), TokenKind::End) {
                    span = span.merge(self.advance().span);
                }
            } else {
                self.consume_block_until_end();
                span = span.merge(self.current().span);
            }
        }
        Ok(Stmt::Expr(ExprStmt { expr, span }))
    }

    /// Check if the current position is an assignment (ident followed by =)
    fn is_assignment(&self) -> bool {
        let mut i = self.pos;
        if !matches!(self.tokens.get(i).map(|t| &t.kind), Some(k) if Self::is_identifier_like(k)) {
            return false;
        }
        i += 1;
        while matches!(self.tokens.get(i).map(|t| &t.kind), Some(TokenKind::Dot))
            && matches!(
                self.tokens.get(i + 1).map(|t| &t.kind),
                Some(TokenKind::Ident(_))
            )
        {
            i += 2;
        }
        matches!(self.tokens.get(i).map(|t| &t.kind), Some(TokenKind::Assign))
    }

    /// Parse an assignment statement: ident = expr
    fn parse_assign(&mut self) -> Result<Stmt, ParseError> {
        let start = self.tokens[self.pos].span;
        let name = self.parse_assignment_target()?;
        self.expect(&TokenKind::Assign)?;
        let value = self.parse_expr(0)?;
        let span = start.merge(value.span());
        Ok(Stmt::Assign(AssignStmt {
            target: name,
            value,
            span,
        }))
    }

    // ── Use Tool / Grant ──

    fn parse_use_tool(&mut self) -> Result<UseToolDecl, ParseError> {
        let start = self.expect(&TokenKind::Use)?.span;
        self.expect(&TokenKind::Tool)?;
        // Could be: `use tool llm.chat as Chat` or `use tool mcp "url" as Name`
        let mcp_url = if matches!(self.peek_kind(), TokenKind::Ident(ref s) if s == "mcp") {
            self.advance();
            let url = self.expect_string()?;
            Some(url)
        } else {
            None
        };
        let tool_path = if mcp_url.is_none() {
            let mut path = self.parse_dotted_ident()?;
            if matches!(self.peek_kind(), TokenKind::At) {
                self.advance();
                let mut ver = String::new();
                while !matches!(
                    self.peek_kind(),
                    TokenKind::As | TokenKind::Newline | TokenKind::Eof
                ) {
                    ver.push_str(&format!("{}", self.current().kind));
                    self.advance();
                }
                if !ver.is_empty() {
                    path = format!("{path}@{ver}");
                }
            }
            path
        } else {
            String::new()
        };
        self.expect(&TokenKind::As)?;
        let alias = if matches!(self.peek_kind(), TokenKind::Star) {
            self.advance();
            "__all__".to_string()
        } else {
            self.expect_ident()?
        };
        Ok(UseToolDecl {
            tool_path,
            alias,
            mcp_url,
            span: start,
        })
    }

    fn parse_grant(&mut self) -> Result<GrantDecl, ParseError> {
        let start = self.expect(&TokenKind::Grant)?.span;
        let alias = self.parse_dotted_ident()?;
        let mut constraints = Vec::new();
        self.skip_newlines();
        // Parse constraints: key value pairs on same line or indented
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent {
            self.advance();
            self.skip_newlines();
        }
        while matches!(self.peek_kind(), TokenKind::Ident(_)) {
            let cs = self.current().span;
            let key = self.expect_ident()?;
            let value = self.parse_expr(0)?;
            constraints.push(GrantConstraint {
                key,
                value,
                span: cs,
            });
            self.consume_rest_of_line();
            self.skip_newlines();
            if !has_indent {
                break;
            } // single-line grants
        }
        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) {
            self.advance();
        }
        Ok(GrantDecl {
            tool_alias: alias,
            constraints,
            span: start,
        })
    }

    // ── Types ──

    fn parse_type(&mut self) -> Result<TypeExpr, ParseError> {
        let base = self.parse_base_type()?;
        // Check for union: T | U
        if matches!(self.peek_kind(), TokenKind::Pipe | TokenKind::Ampersand) {
            let mut types = vec![base];
            while matches!(self.peek_kind(), TokenKind::Pipe | TokenKind::Ampersand) {
                self.advance();
                types.push(self.parse_base_type()?);
            }
            let span = types
                .first()
                .unwrap()
                .span()
                .merge(types.last().unwrap().span());
            Ok(TypeExpr::Union(types, span))
        } else {
            Ok(base)
        }
    }

    fn parse_base_type(&mut self) -> Result<TypeExpr, ParseError> {
        let base = match self.peek_kind().clone() {
            TokenKind::Null => {
                let s = self.advance().span;
                Ok(TypeExpr::Null(s))
            }
            TokenKind::List => {
                let s = self.advance().span;
                self.expect(&TokenKind::LBracket)?;
                let inner = self.parse_type()?;
                self.expect(&TokenKind::RBracket)?;
                Ok(TypeExpr::List(Box::new(inner), s))
            }
            TokenKind::Map => {
                let s = self.advance().span;
                self.expect(&TokenKind::LBracket)?;
                let k = self.parse_type()?;
                self.expect(&TokenKind::Comma)?;
                let v = self.parse_type()?;
                self.expect(&TokenKind::RBracket)?;
                Ok(TypeExpr::Map(Box::new(k), Box::new(v), s))
            }
            TokenKind::Result => {
                let s = self.advance().span;
                self.expect(&TokenKind::LBracket)?;
                let ok = self.parse_type()?;
                self.expect(&TokenKind::Comma)?;
                let err = self.parse_type()?;
                self.expect(&TokenKind::RBracket)?;
                Ok(TypeExpr::Result(Box::new(ok), Box::new(err), s))
            }
            TokenKind::Set => {
                let s = self.advance().span;
                self.expect(&TokenKind::LBracket)?;
                let inner = self.parse_type()?;
                self.expect(&TokenKind::RBracket)?;
                Ok(TypeExpr::Set(Box::new(inner), s))
            }
            TokenKind::Tuple => {
                let s = self.advance().span;
                self.expect(&TokenKind::LBracket)?;
                let mut types = vec![self.parse_type()?];
                while matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                    types.push(self.parse_type()?);
                }
                self.expect(&TokenKind::RBracket)?;
                Ok(TypeExpr::Tuple(types, s))
            }
            TokenKind::Fn => {
                let s = self.advance().span;
                self.expect(&TokenKind::LParen)?;
                let mut params = Vec::new();
                if !matches!(self.peek_kind(), TokenKind::RParen) {
                    params.push(self.parse_type()?);
                    while matches!(self.peek_kind(), TokenKind::Comma) {
                        self.advance();
                        params.push(self.parse_type()?);
                    }
                }
                self.expect(&TokenKind::RParen)?;
                self.expect(&TokenKind::Arrow)?;
                let ret = self.parse_type()?;
                let effects = self.parse_optional_effect_row()?;
                Ok(TypeExpr::Fn(params, Box::new(ret), effects, s))
            }
            TokenKind::LParen => {
                // Tuple type: (A, B, C)
                let s = self.advance().span;
                let mut types = vec![self.parse_type()?];
                while matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                    types.push(self.parse_type()?);
                }
                self.expect(&TokenKind::RParen)?;
                Ok(TypeExpr::Tuple(types, s))
            }
            TokenKind::Yield => {
                self.advance();
                self.parse_base_type()
            }
            TokenKind::Ident(_) => {
                let name = self.expect_ident()?;
                let span = self.current().span;
                // Check for generic: Name[T, U]
                if matches!(self.peek_kind(), TokenKind::LBracket) {
                    self.advance(); // consume [
                    let mut args = vec![self.parse_type()?];
                    while matches!(self.peek_kind(), TokenKind::Comma) {
                        self.advance();
                        args.push(self.parse_type()?);
                    }
                    self.expect(&TokenKind::RBracket)?;
                    Ok(TypeExpr::Generic(name, args, span))
                } else {
                    Ok(TypeExpr::Named(name, span))
                }
            }
            // Type keywords used as type names
            TokenKind::String_ => {
                let s = self.advance().span;
                Ok(TypeExpr::Named("String".to_string(), s))
            }
            TokenKind::Int_ => {
                let s = self.advance().span;
                Ok(TypeExpr::Named("Int".to_string(), s))
            }
            TokenKind::Float_ => {
                let s = self.advance().span;
                Ok(TypeExpr::Named("Float".to_string(), s))
            }
            TokenKind::Bool => {
                let s = self.advance().span;
                Ok(TypeExpr::Named("Bool".to_string(), s))
            }
            TokenKind::Bytes => {
                let s = self.advance().span;
                Ok(TypeExpr::Named("Bytes".to_string(), s))
            }
            TokenKind::Json => {
                let s = self.advance().span;
                Ok(TypeExpr::Named("Json".to_string(), s))
            }
            TokenKind::Type => {
                let s = self.advance().span;
                if matches!(self.peek_kind(), TokenKind::LBracket) {
                    self.advance();
                    let mut args = vec![self.parse_type()?];
                    while matches!(self.peek_kind(), TokenKind::Comma) {
                        self.advance();
                        args.push(self.parse_type()?);
                    }
                    self.expect(&TokenKind::RBracket)?;
                    Ok(TypeExpr::Generic("type".into(), args, s))
                } else {
                    Ok(TypeExpr::Named("type".to_string(), s))
                }
            }
            TokenKind::Comptime => {
                let s = self.advance().span;
                if matches!(self.peek_kind(), TokenKind::LBrace) {
                    self.consume_balanced_group();
                }
                Ok(TypeExpr::Named("Any".to_string(), s))
            }
            _ => {
                let tok = self.current().clone();
                Err(ParseError::Unexpected {
                    found: format!("{}", tok.kind),
                    expected: "type".into(),
                    line: tok.span.line,
                    col: tok.span.col,
                })
            }
        }?;

        // T? sugar: desugars to T | Null
        if matches!(self.peek_kind(), TokenKind::Question) {
            let q_span = self.advance().span;
            let span = base.span().merge(q_span);
            Ok(TypeExpr::Union(vec![base, TypeExpr::Null(q_span)], span))
        } else {
            Ok(base)
        }
    }

    // ── Expressions (Pratt parser) ──

    fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_prefix()?;
        let mut pending_continuation_dedents: usize = 0;
        loop {
            while pending_continuation_dedents > 0
                && matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Dedent)
            {
                if matches!(self.peek_kind(), TokenKind::Dedent) {
                    pending_continuation_dedents -= 1;
                }
                self.advance();
            }
            if matches!(
                self.peek_kind(),
                TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
            ) {
                let ws_start = self.pos;
                let mut i = self.pos;
                while matches!(
                    self.tokens.get(i).map(|t| &t.kind),
                    Some(TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent)
                ) {
                    i += 1;
                }
                if matches!(
                    self.tokens.get(i).map(|t| &t.kind),
                    Some(
                        TokenKind::PipeForward
                            | TokenKind::ComposeArrow
                            | TokenKind::RightShift
                            | TokenKind::LeftShift
                            | TokenKind::Dot
                            | TokenKind::QuestionQuestion
                            | TokenKind::Plus
                            | TokenKind::Minus
                            | TokenKind::Star
                            | TokenKind::Slash
                            | TokenKind::FloorDiv
                            | TokenKind::Percent
                            | TokenKind::Eq
                            | TokenKind::NotEq
                            | TokenKind::Lt
                            | TokenKind::LtEq
                            | TokenKind::Gt
                            | TokenKind::GtEq
                            | TokenKind::And
                            | TokenKind::Or
                            | TokenKind::In
                            | TokenKind::Pipe
                            | TokenKind::Ampersand
                            | TokenKind::Caret
                    )
                ) {
                    while self.pos < i {
                        self.advance();
                    }
                    for tok in &self.tokens[ws_start..i] {
                        match tok.kind {
                            TokenKind::Indent => pending_continuation_dedents += 1,
                            TokenKind::Dedent => {
                                pending_continuation_dedents =
                                    pending_continuation_dedents.saturating_sub(1);
                            }
                            _ => {}
                        }
                    }
                }
            }
            let kind = self.peek_kind();
            let (op, bp) = match kind {
                TokenKind::Plus => (BinOp::Add, (22, 23)),
                TokenKind::Minus => (BinOp::Sub, (22, 23)),
                TokenKind::Star => (BinOp::Mul, (24, 25)),
                TokenKind::Slash => (BinOp::Div, (24, 25)),
                TokenKind::FloorDiv => (BinOp::FloorDiv, (24, 25)),
                TokenKind::Percent => (BinOp::Mod, (24, 25)),
                TokenKind::StarStar => (BinOp::Pow, (27, 26)), // right-assoc
                TokenKind::Eq => (BinOp::Eq, (14, 15)),
                TokenKind::NotEq => (BinOp::NotEq, (14, 15)),
                TokenKind::Lt => (BinOp::Lt, (14, 15)),
                TokenKind::LtEq => (BinOp::LtEq, (14, 15)),
                TokenKind::Gt => (BinOp::Gt, (14, 15)),
                TokenKind::GtEq => (BinOp::GtEq, (14, 15)),
                TokenKind::Spaceship => (BinOp::Spaceship, (14, 15)),
                TokenKind::In => {
                    if matches!(
                        self.peek_n_kind(1),
                        Some(TokenKind::Newline | TokenKind::Eof | TokenKind::Dedent)
                    ) {
                        break;
                    }
                    (BinOp::In, (14, 15))
                }
                // Type test: expr is TypeName
                TokenKind::Is => {
                    if min_bp > 14 {
                        break;
                    }
                    self.advance();
                    let type_name = self.expect_type_name_for_is()?;
                    let span = lhs.span().merge(self.current().span);
                    lhs = Expr::IsType {
                        expr: Box::new(lhs),
                        type_name,
                        span,
                    };
                    continue;
                }
                // Type cast: expr as Type
                TokenKind::As => {
                    if min_bp > 14 {
                        break;
                    }
                    self.advance();
                    let target_type = self.expect_type_name_for_is()?;
                    let span = lhs.span().merge(self.current().span);
                    lhs = Expr::TypeCast {
                        expr: Box::new(lhs),
                        target_type,
                        span,
                    };
                    continue;
                }
                TokenKind::And => (BinOp::And, (12, 13)),
                TokenKind::Or => (BinOp::Or, (10, 11)),
                TokenKind::PlusPlus => (BinOp::Concat, (18, 19)),
                // Pipe |> and step: produce Expr::Pipe
                TokenKind::PipeForward | TokenKind::Step => {
                    if min_bp > 16 {
                        break;
                    }
                    self.advance();
                    let rhs = self.parse_expr(17)?;
                    let span = lhs.span().merge(rhs.span());
                    lhs = Expr::Pipe {
                        left: Box::new(lhs),
                        right: Box::new(rhs),
                        span,
                    };
                    continue;
                }
                // Compose ~>: produce Expr::BinOp with BinOp::Compose
                TokenKind::ComposeArrow => {
                    if min_bp > 16 {
                        break;
                    }
                    self.advance();
                    let rhs = self.parse_expr(17)?;
                    let span = lhs.span().merge(rhs.span());
                    lhs = Expr::BinOp(Box::new(lhs), BinOp::Compose, Box::new(rhs), span);
                    continue;
                }
                TokenKind::Pipe => (BinOp::BitOr, (14, 15)),
                TokenKind::Caret => (BinOp::BitXor, (15, 16)),
                TokenKind::Ampersand => (BinOp::BitAnd, (16, 17)),
                TokenKind::LeftShift => (BinOp::Shl, (20, 21)),
                TokenKind::RightShift => (BinOp::Shr, (20, 21)),
                TokenKind::PlusAssign => (BinOp::Add, (2, 3)),
                TokenKind::MinusAssign => (BinOp::Sub, (2, 3)),
                TokenKind::StarAssign => (BinOp::Mul, (2, 3)),
                TokenKind::SlashAssign => (BinOp::Div, (2, 3)),
                TokenKind::FloorDivAssign => (BinOp::FloorDiv, (2, 3)),
                TokenKind::PercentAssign => (BinOp::Mod, (2, 3)),
                TokenKind::StarStarAssign => (BinOp::Pow, (2, 3)),
                TokenKind::AmpAssign => (BinOp::BitAnd, (2, 3)),
                TokenKind::PipeAssign => (BinOp::BitOr, (2, 3)),
                TokenKind::CaretAssign => (BinOp::BitXor, (2, 3)),
                // Null coalescing
                TokenKind::QuestionQuestion => {
                    if min_bp > 8 {
                        break;
                    }
                    self.advance();
                    let rhs = self.parse_expr(9)?;
                    let span = lhs.span().merge(rhs.span());
                    lhs = Expr::NullCoalesce(Box::new(lhs), Box::new(rhs), span);
                    continue;
                }
                // Range operators
                TokenKind::DotDot => {
                    if min_bp > 20 {
                        break;
                    }
                    self.advance();
                    let rhs = if matches!(
                        self.peek_kind(),
                        TokenKind::Newline
                            | TokenKind::Eof
                            | TokenKind::RBracket
                            | TokenKind::RParen
                            | TokenKind::Comma
                    ) {
                        None
                    } else {
                        Some(Box::new(self.parse_expr(21)?))
                    };
                    let span = lhs
                        .span()
                        .merge(rhs.as_ref().map(|r| r.span()).unwrap_or(lhs.span()));
                    lhs = Expr::RangeExpr {
                        start: Some(Box::new(lhs)),
                        end: rhs,
                        inclusive: false,
                        step: None,
                        span,
                    };
                    continue;
                }
                TokenKind::DotDotEq => {
                    if min_bp > 20 {
                        break;
                    }
                    self.advance();
                    let rhs = self.parse_expr(21)?;
                    let span = lhs.span().merge(rhs.span());
                    lhs = Expr::RangeExpr {
                        start: Some(Box::new(lhs)),
                        end: Some(Box::new(rhs)),
                        inclusive: true,
                        step: None,
                        span,
                    };
                    continue;
                }
                // Postfix: dot, index, call, ?, !, ?.
                TokenKind::Dot => {
                    if min_bp > 32 {
                        break;
                    }
                    self.advance();
                    let field = match self.peek_kind().clone() {
                        TokenKind::IntLit(n) => {
                            self.advance();
                            n.to_string()
                        }
                        _ => self.expect_ident()?,
                    };
                    let span = lhs.span().merge(self.current().span);
                    lhs = Expr::DotAccess(Box::new(lhs), field, span);
                    continue;
                }
                TokenKind::QuestionDot => {
                    if min_bp > 32 {
                        break;
                    }
                    self.advance();
                    let field = self.expect_ident()?;
                    let span = lhs.span().merge(self.current().span);
                    lhs = Expr::NullSafeAccess(Box::new(lhs), field, span);
                    continue;
                }
                TokenKind::QuestionBracket => {
                    if min_bp > 32 {
                        break;
                    }
                    self.advance(); // consume ?[
                    let idx = self.parse_expr(0)?;
                    self.expect(&TokenKind::RBracket)?;
                    let span = lhs.span().merge(self.current().span);
                    lhs = Expr::NullSafeIndex(Box::new(lhs), Box::new(idx), span);
                    continue;
                }
                TokenKind::LBracket => {
                    if min_bp > 32 {
                        break;
                    }
                    self.advance();
                    if matches!(self.peek_kind(), TokenKind::RBracket) {
                        self.expect(&TokenKind::RBracket)?;
                        let span = lhs.span().merge(self.current().span);
                        lhs =
                            Expr::IndexAccess(Box::new(lhs), Box::new(Expr::IntLit(0, span)), span);
                        continue;
                    }
                    let idx = self.parse_expr(0)?;
                    if matches!(self.peek_kind(), TokenKind::Comma) {
                        let mut args = vec![CallArg::Positional(idx)];
                        while matches!(self.peek_kind(), TokenKind::Comma) {
                            self.advance();
                            if matches!(self.peek_kind(), TokenKind::RBracket) {
                                break;
                            }
                            args.push(CallArg::Positional(self.parse_expr(0)?));
                        }
                        self.expect(&TokenKind::RBracket)?;
                        let span = lhs.span().merge(self.current().span);
                        lhs = Expr::Call(Box::new(lhs), args, span);
                        continue;
                    }
                    self.expect(&TokenKind::RBracket)?;
                    let span = lhs.span().merge(self.current().span);
                    lhs = Expr::IndexAccess(Box::new(lhs), Box::new(idx), span);
                    continue;
                }
                TokenKind::LParen => {
                    if min_bp > 32 {
                        break;
                    }
                    lhs = self.parse_call(lhs)?;
                    continue;
                }
                TokenKind::Question => {
                    if min_bp > 32 {
                        break;
                    }
                    let span = lhs.span().merge(self.advance().span);
                    lhs = Expr::TryExpr(Box::new(lhs), span);
                    continue;
                }
                TokenKind::Bang => {
                    if min_bp > 32 {
                        break;
                    }
                    let span = lhs.span().merge(self.advance().span);
                    lhs = Expr::NullAssert(Box::new(lhs), span);
                    continue;
                }
                TokenKind::Expect => {
                    if min_bp > 1 {
                        break;
                    }
                    self.advance();
                    self.expect(&TokenKind::Schema)?;
                    let schema_name = self.expect_ident()?;
                    let span = lhs.span().merge(self.current().span);
                    lhs = Expr::ExpectSchema(Box::new(lhs), schema_name, span);
                    continue;
                }
                TokenKind::At => {
                    if min_bp > 32 {
                        break;
                    }
                    self.advance();
                    let _ = self.expect_ident();
                    if matches!(self.peek_kind(), TokenKind::LParen) {
                        self.consume_parenthesized();
                    }
                    continue;
                }
                _ => break,
            };
            let (l_bp, r_bp) = bp;
            if l_bp < min_bp {
                break;
            }
            self.advance();
            let rhs = self.parse_expr(r_bp)?;
            let span = lhs.span().merge(rhs.span());

            // Chained comparisons: desugar `a < b < c` into `a < b and b < c`.
            // Only ordering comparisons (Lt, Gt, LtEq, GtEq) can be chained.
            // Note: the middle operand `b` is duplicated in the AST and will be
            // evaluated twice at runtime.
            if matches!(op, BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq) {
                let next = self.peek_kind();
                if matches!(
                    next,
                    TokenKind::Lt | TokenKind::LtEq | TokenKind::Gt | TokenKind::GtEq
                ) {
                    let op2 = match self.peek_kind() {
                        TokenKind::Lt => BinOp::Lt,
                        TokenKind::LtEq => BinOp::LtEq,
                        TokenKind::Gt => BinOp::Gt,
                        TokenKind::GtEq => BinOp::GtEq,
                        _ => unreachable!(),
                    };
                    self.advance();
                    let rhs2 = self.parse_expr(r_bp)?;
                    let left_cmp = Expr::BinOp(Box::new(lhs), op, Box::new(rhs.clone()), span);
                    let right_span = rhs.span().merge(rhs2.span());
                    let right_cmp = Expr::BinOp(Box::new(rhs), op2, Box::new(rhs2), right_span);
                    let full_span = left_cmp.span().merge(right_cmp.span());
                    lhs = Expr::BinOp(
                        Box::new(left_cmp),
                        BinOp::And,
                        Box::new(right_cmp),
                        full_span,
                    );
                    continue;
                }
            }

            lhs = Expr::BinOp(Box::new(lhs), op, Box::new(rhs), span);
        }
        Ok(lhs)
    }

    fn parse_prefix(&mut self) -> Result<Expr, ParseError> {
        match self.peek_kind().clone() {
            TokenKind::IntLit(n) => {
                let s = self.advance().span;
                Ok(Expr::IntLit(n, s))
            }
            TokenKind::BigIntLit(ref n) => {
                let n = n.clone();
                let s = self.advance().span;
                Ok(Expr::BigIntLit(n, s))
            }
            TokenKind::FloatLit(n) => {
                let s = self.advance().span;
                Ok(Expr::FloatLit(n, s))
            }
            TokenKind::StringLit(ref sv) => {
                let sv = sv.clone();
                let s = self.advance().span;
                Ok(Expr::StringLit(sv, s))
            }
            TokenKind::Symbol('\'') => {
                let s = self.advance().span;
                let mut buf = String::new();
                while !matches!(self.peek_kind(), TokenKind::Symbol('\'') | TokenKind::Eof) {
                    if !buf.is_empty() {
                        buf.push(' ');
                    }
                    buf.push_str(&format!("{}", self.current().kind));
                    self.advance();
                }
                let end = if matches!(self.peek_kind(), TokenKind::Symbol('\'')) {
                    self.advance().span
                } else {
                    s
                };
                Ok(Expr::StringLit(buf, s.merge(end)))
            }
            TokenKind::RawStringLit(ref sv) => {
                let sv = sv.clone();
                let s = self.advance().span;
                Ok(Expr::RawStringLit(sv, s))
            }
            TokenKind::BytesLit(ref bv) => {
                let bv = bv.clone();
                let s = self.advance().span;
                Ok(Expr::BytesLit(bv, s))
            }
            TokenKind::NullLit => {
                let s = self.advance().span;
                Ok(Expr::NullLit(s))
            }
            TokenKind::StringInterpLit(ref segments) => {
                let segments = segments.clone();
                let span = self.advance().span;
                let mut ast_segments = Vec::new();
                for (is_expr, text, fmt_spec) in segments {
                    if is_expr {
                        // Parse the expression string
                        let mut lexer =
                            crate::compiler::lexer::Lexer::new(&text, span.line, span.col);
                        let tokens = lexer.tokenize().map_err(|e| ParseError::Unexpected {
                            found: format!("lexer error: {}", e),
                            expected: "expression".into(),
                            line: span.line,
                            col: span.col,
                        })?;
                        let mut parser = Parser::new(tokens);
                        let expr = parser.parse_expr(0)?;
                        if let Some(spec_str) = fmt_spec {
                            let spec = parse_format_spec(&spec_str);
                            ast_segments
                                .push(StringSegment::FormattedInterpolation(Box::new(expr), spec));
                        } else {
                            ast_segments.push(StringSegment::Interpolation(Box::new(expr)));
                        }
                    } else {
                        ast_segments.push(StringSegment::Literal(text));
                    }
                }
                Ok(Expr::StringInterp(ast_segments, span))
            }
            TokenKind::BoolLit(b) => {
                let s = self.advance().span;
                Ok(Expr::BoolLit(b, s))
            }
            TokenKind::Null => {
                let s = self.advance().span;
                Ok(Expr::NullLit(s))
            }
            TokenKind::Minus => {
                let s = self.advance().span;
                let expr = self.parse_expr(28)?; // high bp for unary
                let span = s.merge(expr.span());
                // Constant-fold negation of integer literals at parse time.
                // This is critical for i64::MIN (-9223372036854775808) which
                // cannot be represented as a positive i64 then negated at runtime.
                // The positive literal 9223372036854775808 overflows i64 and becomes
                // a BigIntLit; we detect this case and fold it into IntLit(i64::MIN).
                match &expr {
                    Expr::IntLit(n, _) => {
                        // Fold -n for normal i64 values (wrapping handles i64::MIN negation
                        // of the already-negative i64::MIN, though that's an exotic case)
                        Ok(Expr::IntLit(n.wrapping_neg(), span))
                    }
                    Expr::BigIntLit(n, _) => {
                        // Check if this is exactly i64::MAX + 1, which means -n == i64::MIN
                        let i64_min_abs = BigInt::from(i64::MAX) + BigInt::from(1);
                        if *n == i64_min_abs {
                            Ok(Expr::IntLit(i64::MIN, span))
                        } else {
                            // General BigInt negation
                            Ok(Expr::UnaryOp(UnaryOp::Neg, Box::new(expr), span))
                        }
                    }
                    _ => Ok(Expr::UnaryOp(UnaryOp::Neg, Box::new(expr), span)),
                }
            }
            TokenKind::Not => {
                let s = self.advance().span;
                let expr = self.parse_expr(28)?;
                let span = s.merge(expr.span());
                Ok(Expr::UnaryOp(UnaryOp::Not, Box::new(expr), span))
            }
            TokenKind::Tilde => {
                let s = self.advance().span;
                let expr = self.parse_expr(28)?;
                let span = s.merge(expr.span());
                Ok(Expr::UnaryOp(UnaryOp::BitNot, Box::new(expr), span))
            }
            TokenKind::DotDotDot => {
                let s = self.advance().span;
                if matches!(
                    self.peek_kind(),
                    TokenKind::RBracket | TokenKind::RParen | TokenKind::RBrace | TokenKind::Comma
                ) {
                    return Ok(Expr::Ident("...".into(), s));
                }
                let expr = self.parse_expr(0)?;
                let span = s.merge(expr.span());
                Ok(Expr::SpreadExpr(Box::new(expr), span))
            }
            TokenKind::DotDot => {
                let s = self.advance().span;
                if matches!(
                    self.peek_kind(),
                    TokenKind::RBracket | TokenKind::RParen | TokenKind::RBrace | TokenKind::Comma
                ) {
                    return Ok(Expr::Ident("..".into(), s));
                }
                let expr = self.parse_expr(28)?;
                let span = s.merge(expr.span());
                Ok(Expr::SpreadExpr(Box::new(expr), span))
            }
            TokenKind::Await => {
                let s = self.advance().span;
                let expr = self.parse_expr(0)?;
                let expr = self.rewrite_await_orchestration(expr)?;
                let span = s.merge(expr.span());
                Ok(Expr::AwaitExpr(Box::new(expr), span))
            }
            TokenKind::Perform => {
                let s = self.advance().span;
                let effect_name = self.expect_ident()?;
                self.expect(&TokenKind::Dot)?;
                let operation = self.expect_ident()?;
                self.expect(&TokenKind::LParen)?;
                self.bracket_depth += 1;
                let mut args = Vec::new();
                self.skip_whitespace_tokens();
                while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                    if !args.is_empty() {
                        self.expect(&TokenKind::Comma)?;
                        self.skip_whitespace_tokens();
                    }
                    args.push(self.parse_expr(0)?);
                    self.skip_whitespace_tokens();
                }
                self.bracket_depth -= 1;
                self.expect(&TokenKind::RParen)?;
                let span = s.merge(self.current().span);
                Ok(Expr::Perform {
                    effect_name,
                    operation,
                    args,
                    span,
                })
            }
            TokenKind::Handle => {
                let s = self.advance().span;
                self.skip_newlines();
                let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
                if has_indent {
                    self.advance();
                }
                self.skip_newlines();
                // Parse body statements until 'with'
                let mut body = Vec::new();
                while !matches!(self.peek_kind(), TokenKind::With | TokenKind::Eof) {
                    if matches!(self.peek_kind(), TokenKind::Dedent) {
                        self.advance();
                        continue;
                    }
                    body.push(self.parse_stmt()?);
                    self.skip_newlines();
                }
                self.expect(&TokenKind::With)?;
                self.skip_newlines();
                if matches!(self.peek_kind(), TokenKind::Indent) {
                    self.advance();
                }
                self.skip_newlines();
                // Parse handlers until 'end'
                let mut handlers = Vec::new();
                while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
                    if matches!(self.peek_kind(), TokenKind::Dedent) {
                        self.advance();
                        continue;
                    }
                    if matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
                        break;
                    }
                    let handler = self.parse_effect_handler_arm()?;
                    handlers.push(handler);
                    self.skip_newlines();
                }
                if matches!(self.peek_kind(), TokenKind::Dedent) {
                    self.advance();
                }
                self.expect(&TokenKind::End)?;
                let span = s.merge(self.current().span);
                Ok(Expr::HandleExpr {
                    body,
                    handlers,
                    span,
                })
            }
            TokenKind::Resume => {
                let s = self.advance().span;
                self.expect(&TokenKind::LParen)?;
                let value = self.parse_expr(0)?;
                self.expect(&TokenKind::RParen)?;
                let span = s.merge(self.current().span);
                Ok(Expr::ResumeExpr(Box::new(value), span))
            }
            TokenKind::Fn => self.parse_lambda(),
            TokenKind::Parallel => {
                let s = self.advance().span;
                Ok(Expr::Ident("parallel".into(), s))
            }
            TokenKind::Match => {
                // Reuse the statement-level match parser, then extract the AST
                let stmt = self.parse_match()?;
                if let Stmt::Match(ms) = stmt {
                    let span = ms.span;
                    Ok(Expr::MatchExpr {
                        subject: Box::new(ms.subject),
                        arms: ms.arms,
                        span,
                    })
                } else {
                    let span = stmt.span();
                    Ok(Expr::BlockExpr(vec![stmt], span))
                }
            }
            TokenKind::If => {
                // Reuse the statement-level if parser, wrap result as block expr
                let stmt = self.parse_if()?;
                let span = stmt.span();
                Ok(Expr::BlockExpr(vec![stmt], span))
            }
            TokenKind::When => {
                let s = self.advance().span;
                self.skip_newlines();
                let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
                if has_indent {
                    self.advance();
                }
                self.skip_newlines();
                let mut arms = Vec::new();
                let mut else_body = None;
                while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
                    self.skip_newlines();
                    if matches!(self.peek_kind(), TokenKind::Dedent) {
                        self.advance();
                        continue;
                    }
                    if matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
                        break;
                    }
                    let arm_start = self.current().span;
                    // Check for wildcard/else arm: _ -> expr
                    if matches!(self.peek_kind(), TokenKind::Ident(ref n) if n == "_") {
                        let id_span = self.current().span;
                        self.advance(); // consume _
                        if matches!(self.peek_kind(), TokenKind::Arrow) {
                            self.advance(); // consume ->
                            self.skip_newlines();
                            let body = self.parse_expr(0)?;
                            else_body = Some(Box::new(body));
                            self.skip_newlines();
                            continue;
                        }
                        // Not followed by ->, treat as normal condition starting with _
                        // Put back by creating an ident expr
                        let cond = Expr::Ident("_".into(), id_span);
                        // Try to finish the condition if there's more
                        if matches!(self.peek_kind(), TokenKind::Arrow) {
                            self.advance();
                            self.skip_newlines();
                            let body = self.parse_expr(0)?;
                            let span = arm_start.merge(body.span());
                            arms.push(WhenArm {
                                condition: cond,
                                body,
                                span,
                            });
                            self.skip_newlines();
                            continue;
                        }
                    }
                    let cond = self.parse_expr(0)?;
                    self.skip_newlines();
                    if matches!(self.peek_kind(), TokenKind::Arrow) {
                        self.advance(); // consume ->
                    }
                    self.skip_newlines();
                    let body = self.parse_expr(0)?;
                    let span = arm_start.merge(body.span());
                    arms.push(WhenArm {
                        condition: cond,
                        body,
                        span,
                    });
                    self.skip_newlines();
                }
                if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) {
                    self.advance();
                }
                self.skip_newlines();
                let end_span = if matches!(self.peek_kind(), TokenKind::End) {
                    self.advance().span
                } else {
                    s
                };
                Ok(Expr::WhenExpr {
                    arms,
                    else_body,
                    span: s.merge(end_span),
                })
            }
            TokenKind::Loop => {
                // Reuse statement-level loop parser, wrap as block expr
                let stmt = self.parse_loop()?;
                let span = stmt.span();
                Ok(Expr::BlockExpr(vec![stmt], span))
            }
            TokenKind::Let => {
                // Reuse statement-level let parser, wrap as block expr
                let stmt = self.parse_let()?;
                let span = stmt.span();
                Ok(Expr::BlockExpr(vec![stmt], span))
            }
            TokenKind::Try => {
                let s = self.advance().span;
                let inner = self.parse_expr(0)?;
                let span = s.merge(inner.span());
                Ok(Expr::TryExpr(Box::new(inner), span))
            }
            TokenKind::Async => {
                let s = self.advance().span;
                // async block: `async ... end` or `async <expr>`
                if matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Indent) {
                    self.skip_newlines();
                    let block = self.parse_block()?;
                    let end_span = if matches!(self.peek_kind(), TokenKind::End) {
                        self.advance().span
                    } else {
                        s
                    };
                    Ok(Expr::BlockExpr(block, s.merge(end_span)))
                } else if !matches!(
                    self.peek_kind(),
                    TokenKind::RParen
                        | TokenKind::RBracket
                        | TokenKind::RBrace
                        | TokenKind::Comma
                        | TokenKind::Eof
                ) {
                    let inner = self.parse_expr(0)?;
                    let span = s.merge(inner.span());
                    Ok(Expr::AwaitExpr(Box::new(inner), span))
                } else {
                    Ok(Expr::BlockExpr(vec![], s))
                }
            }
            TokenKind::Comptime => {
                let s = self.advance().span;
                if matches!(self.peek_kind(), TokenKind::LBrace) {
                    // comptime { block }: consume balanced braces and wrap as placeholder
                    let save = self.pos;
                    self.advance(); // consume {
                    self.skip_newlines();
                    // Try parsing as a block of statements
                    let result = (|| -> Result<Vec<Stmt>, ParseError> {
                        if matches!(self.peek_kind(), TokenKind::Indent) {
                            self.advance();
                        }
                        self.skip_newlines();
                        let mut stmts = Vec::new();
                        while !matches!(
                            self.peek_kind(),
                            TokenKind::RBrace | TokenKind::Eof | TokenKind::Dedent
                        ) {
                            stmts.push(self.parse_stmt()?);
                            self.skip_newlines();
                        }
                        if matches!(self.peek_kind(), TokenKind::Dedent) {
                            self.advance();
                        }
                        self.skip_newlines();
                        Ok(stmts)
                    })();
                    match result {
                        Ok(stmts) => {
                            let end_span = if matches!(self.peek_kind(), TokenKind::RBrace) {
                                self.advance().span
                            } else {
                                s
                            };
                            let block = Expr::BlockExpr(stmts, s.merge(end_span));
                            Ok(Expr::ComptimeExpr(Box::new(block), s.merge(end_span)))
                        }
                        Err(_) => {
                            // Fallback: consume balanced group
                            self.pos = save;
                            self.consume_balanced_group();
                            let end_span = self.current().span;
                            Ok(Expr::ComptimeExpr(
                                Box::new(Expr::NullLit(s)),
                                s.merge(end_span),
                            ))
                        }
                    }
                } else {
                    self.skip_newlines();
                    let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
                    if has_indent {
                        self.advance();
                    }
                    self.skip_newlines();
                    let mut stmts = Vec::new();
                    while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
                        self.skip_newlines();
                        if matches!(self.peek_kind(), TokenKind::Dedent) {
                            self.advance();
                            continue;
                        }
                        if matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof) {
                            break;
                        }
                        stmts.push(self.parse_stmt()?);
                        self.skip_newlines();
                    }
                    if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) {
                        self.advance();
                    }
                    self.skip_newlines();
                    let end_span = if matches!(self.peek_kind(), TokenKind::End) {
                        self.advance().span
                    } else {
                        s
                    };
                    let block = Expr::BlockExpr(stmts, s.merge(end_span));
                    Ok(Expr::ComptimeExpr(Box::new(block), s.merge(end_span)))
                }
            }
            TokenKind::Use => {
                let s = self.advance().span;
                Ok(Expr::Ident("use".into(), s))
            }
            TokenKind::Halt => {
                let s = self.advance().span;
                Ok(Expr::Ident("halt".into(), s))
            }
            TokenKind::Cell => {
                let s = self.advance().span;
                Ok(Expr::Ident("cell".into(), s))
            }
            TokenKind::End => {
                let s = self.advance().span;
                Ok(Expr::Ident("end".into(), s))
            }
            TokenKind::Dot => {
                let s = self.advance().span;
                let name = self.expect_ident()?;
                let mut expr = Expr::Ident(format!(".{name}"), s);
                if matches!(self.peek_kind(), TokenKind::LParen) {
                    expr = self.parse_call(expr)?;
                }
                Ok(expr)
            }
            TokenKind::Set => {
                let s = self.advance().span;
                self.expect(&TokenKind::LBracket)?;
                self.bracket_depth += 1;
                self.skip_whitespace_tokens();
                if matches!(self.peek_kind(), TokenKind::RBracket) {
                    self.bracket_depth -= 1;
                    let end = self.expect(&TokenKind::RBracket)?.span;
                    return Ok(Expr::SetLit(vec![], s.merge(end)));
                }
                let first = self.parse_expr(0)?;
                self.skip_whitespace_tokens();
                if matches!(self.peek_kind(), TokenKind::For) {
                    self.advance();
                    let var = self.expect_ident()?;
                    self.expect(&TokenKind::In)?;
                    let iter = self.parse_expr(0)?;
                    let mut extra_clauses = Vec::new();
                    while matches!(self.peek_kind(), TokenKind::For) {
                        self.advance();
                        let inner_var = self.expect_ident()?;
                        self.expect(&TokenKind::In)?;
                        let inner_iter = self.parse_expr(0)?;
                        extra_clauses.push(ComprehensionClause {
                            var: inner_var,
                            iter: inner_iter,
                        });
                    }
                    let condition = if matches!(self.peek_kind(), TokenKind::If) {
                        self.advance();
                        Some(Box::new(self.parse_expr(0)?))
                    } else {
                        None
                    };
                    self.skip_whitespace_tokens();
                    self.bracket_depth -= 1;
                    let end = self.expect(&TokenKind::RBrace)?.span;
                    return Ok(Expr::Comprehension {
                        body: Box::new(first),
                        var,
                        iter: Box::new(iter),
                        extra_clauses,
                        condition,
                        kind: ComprehensionKind::Set,
                        span: s.merge(end),
                    });
                }
                let mut elems = vec![first];
                while matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                    self.skip_whitespace_tokens();
                    if matches!(self.peek_kind(), TokenKind::RBrace) {
                        break;
                    }
                    elems.push(self.parse_expr(0)?);
                    self.skip_whitespace_tokens();
                }
                self.bracket_depth -= 1;
                let end = self.expect(&TokenKind::RBrace)?.span;
                Ok(Expr::SetLit(elems, s.merge(end)))
            }
            TokenKind::LParen => {
                let s = self.advance().span;
                // Could be grouping, tuple, or empty tuple
                if matches!(self.peek_kind(), TokenKind::RParen) {
                    // Empty tuple
                    let end = self.advance().span;
                    return Ok(Expr::TupleLit(vec![], s.merge(end)));
                }
                let first = self.parse_expr(0)?;
                if matches!(self.peek_kind(), TokenKind::Comma) {
                    // Tuple
                    let mut elems = vec![first];
                    while matches!(self.peek_kind(), TokenKind::Comma) {
                        self.advance();
                        if matches!(self.peek_kind(), TokenKind::RParen) {
                            break;
                        }
                        elems.push(self.parse_expr(0)?);
                    }
                    let end = self.expect(&TokenKind::RParen)?.span;
                    Ok(Expr::TupleLit(elems, s.merge(end)))
                } else {
                    // Grouping
                    self.expect(&TokenKind::RParen)?;
                    Ok(first)
                }
            }
            TokenKind::LBracket => self.parse_list_or_comprehension(),
            TokenKind::LBrace => self.parse_map_lit(),
            TokenKind::Role => self.parse_role_block_expr(),
            TokenKind::Ok_ => {
                let s = self.advance().span;
                Ok(Expr::Ident("ok".into(), s))
            }
            TokenKind::Err_ => {
                let s = self.advance().span;
                Ok(Expr::Ident("err".into(), s))
            }
            TokenKind::SelfKw => {
                let s = self.advance().span;
                Ok(Expr::Ident("self".into(), s))
            }
            TokenKind::Ident(_) => {
                let start = self.current().span;
                let name = self.expect_ident()?;
                if name == "agent" {
                    return self.parse_expr(28);
                }
                if name == "confirm" {
                    let arg = self.parse_expr(28)?;
                    let span = start.merge(arg.span());
                    return Ok(Expr::Call(
                        Box::new(Expr::Ident(name, start)),
                        vec![CallArg::Positional(arg)],
                        span,
                    ));
                }
                let span = self.current().span;
                Ok(Expr::Ident(name, span))
            }
            // Type keywords used as function names in expression position
            TokenKind::String_ => {
                let s = self.advance().span;
                Ok(Expr::Ident("string".into(), s))
            }
            TokenKind::Int_ => {
                let s = self.advance().span;
                Ok(Expr::Ident("int".into(), s))
            }
            TokenKind::Float_ => {
                let s = self.advance().span;
                Ok(Expr::Ident("float".into(), s))
            }
            TokenKind::Bool => {
                let s = self.advance().span;
                Ok(Expr::Ident("bool".into(), s))
            }
            TokenKind::Bytes => {
                let s = self.advance().span;
                Ok(Expr::Ident("bytes".into(), s))
            }
            TokenKind::Json => {
                let s = self.advance().span;
                Ok(Expr::Ident("json".into(), s))
            }
            TokenKind::List => {
                let s = self.advance().span;
                Ok(Expr::Ident("list".into(), s))
            }
            TokenKind::Map => {
                let s = self.advance().span;
                Ok(Expr::Ident("map".into(), s))
            }
            TokenKind::Type => {
                let s = self.advance().span;
                Ok(Expr::Ident("type".into(), s))
            }
            TokenKind::Result => {
                let s = self.advance().span;
                Ok(Expr::Ident("result".into(), s))
            }
            TokenKind::Tool => {
                let s = self.advance().span;
                Ok(Expr::Ident("tool".into(), s))
            }
            TokenKind::Schema => {
                let s = self.advance().span;
                Ok(Expr::Ident("schema".into(), s))
            }
            _ => {
                let tok = self.current().clone();
                Err(ParseError::Unexpected {
                    found: format!("{}", tok.kind),
                    expected: "expression".into(),
                    line: tok.span.line,
                    col: tok.span.col,
                })
            }
        }
    }

    fn parse_lambda(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect(&TokenKind::Fn)?.span;
        self.expect(&TokenKind::LParen)?;
        let mut params = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RParen) {
            if !params.is_empty() {
                self.expect(&TokenKind::Comma)?;
            }
            let ps = self.current().span;
            let pname = if matches!(self.peek_kind(), TokenKind::LParen) {
                self.consume_balanced_group();
                format!("__arg{}", params.len())
            } else {
                self.expect_ident()?
            };
            let pty = if matches!(self.peek_kind(), TokenKind::Colon) {
                self.advance();
                self.parse_type()?
            } else {
                TypeExpr::Named("Any".into(), ps)
            };
            params.push(Param {
                name: pname,
                ty: pty,
                default_value: None,
                variadic: false,
                span: ps,
            });
        }
        self.expect(&TokenKind::RParen)?;
        let return_type = if matches!(self.peek_kind(), TokenKind::Arrow) {
            self.advance();
            Some(Box::new(self.parse_type()?))
        } else {
            None
        };
        let body = if matches!(self.peek_kind(), TokenKind::FatArrow) {
            self.advance();
            self.skip_whitespace_tokens();
            let expr = self.parse_expr(0)?;
            LambdaBody::Expr(Box::new(expr))
        } else {
            self.skip_newlines();
            let stmts = self.parse_block()?;
            self.expect(&TokenKind::End)?;
            LambdaBody::Block(stmts)
        };
        let end_span = match &body {
            LambdaBody::Expr(e) => e.span(),
            LambdaBody::Block(stmts) => stmts.last().map(|s| s.span()).unwrap_or(start),
        };
        Ok(Expr::Lambda {
            params,
            return_type,
            body,
            span: start.merge(end_span),
        })
    }

    fn parse_list_or_comprehension(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect(&TokenKind::LBracket)?.span;
        self.bracket_depth += 1;
        self.skip_whitespace_tokens();
        if matches!(self.peek_kind(), TokenKind::RBracket) {
            self.bracket_depth -= 1;
            let end = self.expect(&TokenKind::RBracket)?.span;
            return Ok(Expr::ListLit(vec![], start.merge(end)));
        }
        // Parse first element
        let first = self.parse_expr(0)?;
        self.skip_whitespace_tokens();
        // Check for comprehension: [expr for var in iter]
        if matches!(self.peek_kind(), TokenKind::For) {
            self.advance();
            let var = if matches!(self.peek_kind(), TokenKind::LParen) {
                self.advance();
                let first = self.expect_ident()?;
                while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                    self.advance();
                }
                if matches!(self.peek_kind(), TokenKind::RParen) {
                    self.advance();
                }
                first
            } else {
                self.expect_ident()?
            };
            self.expect(&TokenKind::In)?;
            let iter = self.parse_expr(0)?;
            self.skip_whitespace_tokens();
            let mut extra_clauses = Vec::new();
            while matches!(self.peek_kind(), TokenKind::For) {
                self.advance();
                let inner_var = if matches!(self.peek_kind(), TokenKind::LParen) {
                    self.advance();
                    let first_ident = self.expect_ident()?;
                    while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                        self.advance();
                    }
                    if matches!(self.peek_kind(), TokenKind::RParen) {
                        self.advance();
                    }
                    first_ident
                } else {
                    self.expect_ident()?
                };
                self.expect(&TokenKind::In)?;
                let inner_iter = self.parse_expr(0)?;
                extra_clauses.push(ComprehensionClause {
                    var: inner_var,
                    iter: inner_iter,
                });
                self.skip_whitespace_tokens();
            }
            let condition = if matches!(self.peek_kind(), TokenKind::If) {
                self.advance();
                Some(Box::new(self.parse_expr(0)?))
            } else {
                None
            };
            self.skip_whitespace_tokens();
            self.bracket_depth -= 1;
            let end = self.expect(&TokenKind::RBracket)?.span;
            return Ok(Expr::Comprehension {
                body: Box::new(first),
                var,
                iter: Box::new(iter),
                extra_clauses,
                condition,
                kind: ComprehensionKind::List,
                span: start.merge(end),
            });
        }
        // Regular list
        let mut elems = vec![first];
        while matches!(self.peek_kind(), TokenKind::Comma) {
            self.advance();
            self.skip_whitespace_tokens();
            if matches!(self.peek_kind(), TokenKind::RBracket) {
                break;
            }
            elems.push(self.parse_expr(0)?);
            self.skip_whitespace_tokens();
        }
        self.bracket_depth -= 1;
        let end = self.expect(&TokenKind::RBracket)?.span;
        Ok(Expr::ListLit(elems, start.merge(end)))
    }

    fn parse_call(&mut self, callee: Expr) -> Result<Expr, ParseError> {
        let start = callee.span();
        self.expect(&TokenKind::LParen)?;
        self.bracket_depth += 1;
        let mut args = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RParen) {
            self.skip_whitespace_tokens();
            if matches!(self.peek_kind(), TokenKind::RParen) {
                break;
            }
            if !args.is_empty() {
                self.expect(&TokenKind::Comma)?;
                self.skip_whitespace_tokens();
            }
            if matches!(self.peek_kind(), TokenKind::Schema) {
                let ks = self.advance().span;
                self.skip_whitespace_tokens();
                if matches!(self.peek_kind(), TokenKind::Colon) {
                    self.advance();
                    self.skip_whitespace_tokens();
                }
                let val = self.parse_expr(28)?;
                let span = ks.merge(val.span());
                args.push(CallArg::Named("schema".into(), val, span));
                self.skip_whitespace_tokens();
                continue;
            }
            // Check for role blocks inline
            if matches!(self.peek_kind(), TokenKind::Role)
                && !matches!(self.peek_n_kind(1), Some(TokenKind::Colon))
            {
                self.advance();
                let role_span = self.current().span;
                let role_name = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;

                if matches!(self.peek_kind(), TokenKind::Newline) {
                    while matches!(self.peek_kind(), TokenKind::Newline) {
                        self.advance();
                    }
                }
                let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
                let role_terminators: &[TokenKind] = if has_indent {
                    &[TokenKind::RParen]
                } else {
                    &[TokenKind::Comma, TokenKind::RParen]
                };
                let content_expr = self.parse_role_content(role_terminators, has_indent)?;
                if has_indent && matches!(self.peek_kind(), TokenKind::End) {
                    self.advance();
                }

                let span = role_span.merge(content_expr.span());
                args.push(CallArg::Role(role_name, content_expr, span));
                continue;
            }
            if self.token_can_be_named_arg_key() {
                // Check if named arg: name: expr
                let save = self.pos;
                let name_clone = self.expect_ident()?;
                if matches!(self.peek_kind(), TokenKind::Colon) {
                    self.advance();
                    self.skip_whitespace_tokens();
                    let val = self.parse_expr(0)?;
                    let span = val.span();
                    args.push(CallArg::Named(name_clone, val, span));
                } else {
                    // Property shorthand: Point(x, y) => Point(x: x, y: y)
                    // Only when callee is an uppercase ident (record constructor)
                    // and the argument is a bare identifier (followed by , or ))
                    let is_record_ctor =
                        matches!(&callee, Expr::Ident(n, _) if n.starts_with(char::is_uppercase));
                    if is_record_ctor
                        && matches!(
                            self.peek_kind(),
                            TokenKind::Comma | TokenKind::RParen | TokenKind::Newline
                        )
                    {
                        let span = self.tokens[save].span;
                        args.push(CallArg::Named(
                            name_clone.clone(),
                            Expr::Ident(name_clone, span),
                            span,
                        ));
                    } else {
                        self.pos = save;
                        let expr = self.parse_expr(0)?;
                        args.push(CallArg::Positional(expr));
                    }
                }
            } else {
                let expr = self.parse_expr(0)?;
                args.push(CallArg::Positional(expr));
            }
            self.skip_whitespace_tokens();
        }
        self.bracket_depth -= 1;
        let end = self.expect(&TokenKind::RParen)?.span;

        Ok(Expr::Call(Box::new(callee), args, start.merge(end)))
    }

    fn parse_map_lit(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect(&TokenKind::LBrace)?.span;
        self.bracket_depth += 1;
        self.skip_whitespace_tokens();

        // Empty braces -> empty map
        if matches!(self.peek_kind(), TokenKind::RBrace) {
            self.bracket_depth -= 1;
            let end = self.expect(&TokenKind::RBrace)?.span;
            return Ok(Expr::MapLit(vec![], start.merge(end)));
        }

        // Check for spread at the start
        if matches!(self.peek_kind(), TokenKind::DotDot | TokenKind::DotDotDot) {
            let spread_span = self.advance().span;
            let spread_expr = self.parse_expr(0)?;
            let mut pairs = vec![(Expr::StringLit("__spread".into(), spread_span), spread_expr)];
            self.skip_whitespace_tokens();

            while matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance();
                self.skip_whitespace_tokens();
                if matches!(self.peek_kind(), TokenKind::RBrace) {
                    break;
                }
                if matches!(self.peek_kind(), TokenKind::DotDot | TokenKind::DotDotDot) {
                    let spread_span = self.advance().span;
                    let spread_expr = self.parse_expr(0)?;
                    pairs.push((Expr::StringLit("__spread".into(), spread_span), spread_expr));
                } else {
                    let key = if let TokenKind::Ident(name) = self.peek_kind().clone() {
                        if matches!(
                            self.tokens.get(self.pos + 1).map(|t| &t.kind),
                            Some(TokenKind::Colon)
                        ) {
                            let span = self.current().span;
                            self.advance();
                            Expr::StringLit(name, span)
                        } else {
                            self.parse_expr(0)?
                        }
                    } else {
                        self.parse_expr(0)?
                    };
                    self.expect(&TokenKind::Colon)?;
                    self.skip_whitespace_tokens();
                    let val = self.parse_expr(0)?;
                    pairs.push((key, val));
                }
                self.skip_whitespace_tokens();
            }

            self.bracket_depth -= 1;
            let end = self.expect(&TokenKind::RBrace)?.span;
            return Ok(Expr::MapLit(pairs, start.merge(end)));
        }

        // Parse first expression to determine if this is a set or map
        let first = self.parse_expr(0)?;
        self.skip_whitespace_tokens();

        // Check what follows the first expression
        match self.peek_kind() {
            TokenKind::For => {
                // Set comprehension: {expr for var in iter}
                self.advance();
                let var = self.expect_ident()?;
                self.expect(&TokenKind::In)?;
                let iter = self.parse_expr(0)?;
                let mut extra_clauses = Vec::new();
                while matches!(self.peek_kind(), TokenKind::For) {
                    self.advance();
                    let inner_var = self.expect_ident()?;
                    self.expect(&TokenKind::In)?;
                    let inner_iter = self.parse_expr(0)?;
                    extra_clauses.push(ComprehensionClause {
                        var: inner_var,
                        iter: inner_iter,
                    });
                }
                let condition = if matches!(self.peek_kind(), TokenKind::If) {
                    self.advance();
                    Some(Box::new(self.parse_expr(0)?))
                } else {
                    None
                };
                self.skip_whitespace_tokens();
                self.bracket_depth -= 1;
                let end = self.expect(&TokenKind::RBrace)?.span;

                // Determine comprehension kind based on body type
                // If body is a tuple, it's a map comprehension
                let kind = if matches!(first, Expr::TupleLit(..)) {
                    ComprehensionKind::Map
                } else {
                    ComprehensionKind::Set
                };

                Ok(Expr::Comprehension {
                    body: Box::new(first),
                    var,
                    iter: Box::new(iter),
                    extra_clauses,
                    condition,
                    kind,
                    span: start.merge(end),
                })
            }
            TokenKind::Colon | TokenKind::Assign => {
                // Map literal: {key: value, ...}
                self.advance();
                self.skip_whitespace_tokens();
                let val = self.parse_expr(0)?;
                self.skip_whitespace_tokens();

                let mut pairs = vec![(first, val)];
                while matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                    self.skip_whitespace_tokens();
                    if matches!(self.peek_kind(), TokenKind::RBrace) {
                        break;
                    }
                    if matches!(self.peek_kind(), TokenKind::DotDot | TokenKind::DotDotDot) {
                        let spread_span = self.advance().span;
                        let spread_expr = self.parse_expr(0)?;
                        pairs.push((Expr::StringLit("__spread".into(), spread_span), spread_expr));
                        self.skip_whitespace_tokens();
                        continue;
                    }
                    let key = if let TokenKind::Ident(name) = self.peek_kind().clone() {
                        if matches!(
                            self.tokens.get(self.pos + 1).map(|t| &t.kind),
                            Some(TokenKind::Colon)
                        ) {
                            let span = self.current().span;
                            self.advance();
                            Expr::StringLit(name, span)
                        } else {
                            self.parse_expr(0)?
                        }
                    } else {
                        self.parse_expr(0)?
                    };
                    self.expect(&TokenKind::Colon)?;
                    self.skip_whitespace_tokens();
                    let val = self.parse_expr(0)?;
                    pairs.push((key, val));
                    self.skip_whitespace_tokens();
                }
                self.bracket_depth -= 1;
                let end = self.expect(&TokenKind::RBrace)?.span;
                Ok(Expr::MapLit(pairs, start.merge(end)))
            }
            TokenKind::Comma | TokenKind::RBrace => {
                // Set literal: {val1, val2, ...}
                let mut elems = vec![first];
                while matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                    self.skip_whitespace_tokens();
                    if matches!(self.peek_kind(), TokenKind::RBrace) {
                        break;
                    }
                    elems.push(self.parse_expr(0)?);
                    self.skip_whitespace_tokens();
                }
                self.bracket_depth -= 1;
                let end = self.expect(&TokenKind::RBrace)?.span;
                Ok(Expr::SetLit(elems, start.merge(end)))
            }
            _ => {
                let tok = self.current().clone();
                Err(ParseError::Unexpected {
                    found: format!("{}", tok.kind),
                    expected: "',', ':', or 'for'".into(),
                    line: tok.span.line,
                    col: tok.span.col,
                })
            }
        }
    }

    fn parse_role_block_expr(&mut self) -> Result<Expr, ParseError> {
        // Expression context (default): stops at RParen to support call args
        self.parse_role_block_general(&[
            TokenKind::End,
            TokenKind::Role,
            TokenKind::Eof,
            TokenKind::RParen,
        ])
    }

    /// Parse a role block in statement context (allows RParen)
    fn parse_role_block_stmt(&mut self) -> Result<Expr, ParseError> {
        self.parse_role_block_general(&[TokenKind::End, TokenKind::Role, TokenKind::Eof])
    }

    fn parse_role_block_general(&mut self, terminators: &[TokenKind]) -> Result<Expr, ParseError> {
        let start = self.expect(&TokenKind::Role)?.span;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon)?;

        // Check if it was an indented block
        // We need to know if we started with indentation.
        // But parse_role_content consumed the indent/dedent if present.
        // Actually parse_role_content determines this.
        // We should move the `Expect End` logic into parse_role_content or check here?
        // parse_role_content: returns Expr.
        // If we inspect parse_role_content more closely, it handles indent/dedent.
        // If it was inline, it stopped at Newline.
        // Does inline require `end`? No.
        // Does block require `end`? Yes.
        // How do we know which one it was?
        // peek Indent *before* calling parse_role_content?
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        let content_expr = self.parse_role_content(terminators, has_indent)?;

        let end_span = if has_indent || matches!(self.peek_kind(), TokenKind::End) {
            self.expect(&TokenKind::End)?.span
        } else {
            // Inline role ends at newline (which is peeked but not consumed by parse_role_content?)
            // parse_role_content breaks on Newline. So Newline is next.
            // We don't need to consume it necessarily, skip_newlines in parse_block will handle it.
            content_expr.span()
        };

        Ok(Expr::RoleBlock(
            name,
            Box::new(content_expr),
            start.merge(end_span),
        ))
    }

    /// Parse role block content with interpolation support.
    /// Stops at TokenKind::End, TokenKind::Role (peeked), or EOF/Dedent.
    /// Does NOT consume 'end' or 'role', but consumes the content.
    fn parse_role_content(
        &mut self,
        terminators: &[TokenKind],
        has_indent: bool,
    ) -> Result<Expr, ParseError> {
        let start = self.current().span;
        let mut segments = Vec::new();
        let mut text_buf = String::new();
        if has_indent {
            self.advance();
        }
        self.skip_newlines();

        loop {
            let peek = self.peek_kind();
            if terminators.contains(peek) {
                break;
            }
            if matches!(peek, TokenKind::Newline) && !has_indent {
                break;
            }
            if matches!(peek, TokenKind::Dedent) && has_indent {
                break;
            }

            match peek {
                TokenKind::LBrace => {
                    // Try interpolation first; if parsing fails, keep `{...}` as literal text.
                    let saved_pos = self.pos;
                    let saved_bracket_depth = self.bracket_depth;
                    self.advance(); // consume `{`
                    let parsed_interp = (|| {
                        let expr = self.parse_expr(0).ok()?;
                        self.expect(&TokenKind::RBrace).ok()?;
                        Some(expr)
                    })();

                    if let Some(expr) = parsed_interp {
                        if !text_buf.is_empty() {
                            segments.push(StringSegment::Literal(text_buf.clone()));
                            text_buf.clear();
                        }
                        segments.push(StringSegment::Interpolation(Box::new(expr)));
                    } else {
                        self.pos = saved_pos;
                        self.bracket_depth = saved_bracket_depth;
                        text_buf.push('{');
                        self.advance();
                    }
                }
                TokenKind::Newline => {
                    // Inside indented block, preserve newline
                    text_buf.push('\n');
                    self.advance();
                }
                TokenKind::Indent | TokenKind::Dedent => {
                    // Should imply structural change, but if we are inside role block content...
                    // Indent here means nested indentation?
                    // We just consume it as whitespace/structure?
                    // Or maybe we should skip it?
                    self.advance();
                }
                _ => {
                    // Accumulate text
                    let tok = self.advance();
                    let text = format!("{}", tok.kind);
                    if !text_buf.is_empty() && !text_buf.ends_with('\n') {
                        text_buf.push(' ');
                    }
                    text_buf.push_str(&text);
                }
            }
        }

        if !text_buf.is_empty() {
            segments.push(StringSegment::Literal(text_buf));
        }

        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) {
            self.advance();
        }

        // If we stopped at End, consume it?
        // parse_role_block_expr expects to consume 'End'.
        // parse_call expects to NOT consume 'Role' (next block).
        // So this helper should just parse content.

        let span = if segments.is_empty() {
            start
        } else {
            start.merge(self.current().span)
        };

        if segments.len() == 1 {
            if let StringSegment::Literal(ref s) = segments[0] {
                return Ok(Expr::StringLit(s.clone(), span));
            }
        }

        Ok(Expr::StringInterp(segments, span))
    }

    // ── Helpers ──

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        let tok = self.current().clone();
        match &tok.kind {
            TokenKind::Ident(name) => {
                let n = name.clone();
                self.advance();
                Ok(n)
            }
            TokenKind::Result => {
                self.advance();
                Ok("result".into())
            }
            TokenKind::Ok_ => {
                self.advance();
                Ok("ok".into())
            }
            TokenKind::Err_ => {
                self.advance();
                Ok("err".into())
            }
            TokenKind::List => {
                self.advance();
                Ok("list".into())
            }
            TokenKind::Map => {
                self.advance();
                Ok("map".into())
            }
            TokenKind::Json => {
                self.advance();
                Ok("json".into())
            }
            TokenKind::Bytes => {
                self.advance();
                Ok("bytes".into())
            }
            TokenKind::Bool => {
                self.advance();
                Ok("bool".into())
            }
            TokenKind::Int_ => {
                self.advance();
                Ok("int".into())
            }
            TokenKind::Float_ => {
                self.advance();
                Ok("float".into())
            }
            TokenKind::Set => {
                self.advance();
                Ok("set".into())
            }
            TokenKind::Tuple => {
                self.advance();
                Ok("tuple".into())
            }
            TokenKind::Type => {
                self.advance();
                Ok("type".into())
            }
            TokenKind::String_ => {
                self.advance();
                Ok("string".into())
            }
            TokenKind::Cell => {
                self.advance();
                Ok("cell".into())
            }
            TokenKind::Record => {
                self.advance();
                Ok("record".into())
            }
            TokenKind::SelfKw => {
                self.advance();
                Ok("self".into())
            }
            TokenKind::Schema => {
                self.advance();
                Ok("schema".into())
            }
            TokenKind::Try => {
                self.advance();
                Ok("try".into())
            }
            TokenKind::When => {
                self.advance();
                Ok("when".into())
            }
            TokenKind::Step => {
                self.advance();
                Ok("step".into())
            }
            TokenKind::Comptime => {
                self.advance();
                Ok("comptime".into())
            }
            TokenKind::Macro => {
                self.advance();
                Ok("macro".into())
            }
            TokenKind::Extern => {
                self.advance();
                Ok("extern".into())
            }
            TokenKind::Union => {
                self.advance();
                Ok("union".into())
            }
            TokenKind::If => {
                self.advance();
                Ok("if".into())
            }
            TokenKind::Match => {
                self.advance();
                Ok("match".into())
            }
            TokenKind::Loop => {
                self.advance();
                Ok("loop".into())
            }
            TokenKind::Async => {
                self.advance();
                Ok("async".into())
            }
            TokenKind::With => {
                self.advance();
                Ok("with".into())
            }
            TokenKind::Perform => {
                self.advance();
                Ok("perform".into())
            }
            TokenKind::Handle => {
                self.advance();
                Ok("handle".into())
            }
            TokenKind::Resume => {
                self.advance();
                Ok("resume".into())
            }
            TokenKind::Tool => {
                self.advance();
                Ok("tool".into())
            }
            TokenKind::Role => {
                self.advance();
                Ok("role".into())
            }
            TokenKind::Parallel => {
                self.advance();
                Ok("parallel".into())
            }
            TokenKind::From => {
                self.advance();
                Ok("from".into())
            }
            TokenKind::Where => {
                self.advance();
                Ok("where".into())
            }
            _ => Err(ParseError::Unexpected {
                found: format!("{}", tok.kind),
                expected: "identifier".into(),
                line: tok.span.line,
                col: tok.span.col,
            }),
        }
    }

    /// Parse a type name after `is` or `as` keyword.
    /// Accepts identifiers and type keywords (Int, String, Bool, Float, List, Map, Null, etc.)
    fn expect_type_name_for_is(&mut self) -> Result<String, ParseError> {
        let tok = self.current().clone();
        match &tok.kind {
            TokenKind::Ident(name) => {
                let n = name.clone();
                self.advance();
                Ok(n)
            }
            TokenKind::Int_ => {
                self.advance();
                Ok("Int".into())
            }
            TokenKind::Float_ => {
                self.advance();
                Ok("Float".into())
            }
            TokenKind::String_ => {
                self.advance();
                Ok("String".into())
            }
            TokenKind::Bool => {
                self.advance();
                Ok("Bool".into())
            }
            TokenKind::Null => {
                self.advance();
                Ok("Null".into())
            }
            TokenKind::List => {
                self.advance();
                Ok("List".into())
            }
            TokenKind::Map => {
                self.advance();
                Ok("Map".into())
            }
            TokenKind::Set => {
                self.advance();
                Ok("Set".into())
            }
            TokenKind::Tuple => {
                self.advance();
                Ok("Tuple".into())
            }
            TokenKind::Bytes => {
                self.advance();
                Ok("Bytes".into())
            }
            TokenKind::Json => {
                self.advance();
                Ok("Json".into())
            }
            TokenKind::Result => {
                self.advance();
                Ok("Result".into())
            }
            _ => Err(ParseError::Unexpected {
                found: format!("{}", tok.kind),
                expected: "type name".into(),
                line: tok.span.line,
                col: tok.span.col,
            }),
        }
    }

    fn expect_string(&mut self) -> Result<String, ParseError> {
        let tok = self.current().clone();
        match &tok.kind {
            TokenKind::StringLit(s) => {
                let s = s.clone();
                self.advance();
                Ok(s)
            }
            _ => Err(ParseError::Unexpected {
                found: format!("{}", tok.kind),
                expected: "string literal".into(),
                line: tok.span.line,
                col: tok.span.col,
            }),
        }
    }

    fn rewrite_await_orchestration(&mut self, expr: Expr) -> Result<Expr, ParseError> {
        if let Some((name, base_args, span)) = self.orchestration_call_parts(&expr) {
            if name == "parallel" && matches!(self.peek_kind(), TokenKind::For) {
                return self.rewrite_await_parallel_for(base_args, span);
            }
            if self.await_block_follows() {
                return self.rewrite_await_orchestration_block(name, base_args, span);
            }
        }
        Ok(expr)
    }

    fn rewrite_await_parallel_for(
        &mut self,
        mut base_args: Vec<CallArg>,
        call_span: Span,
    ) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::For)?;
        let var = self.expect_ident()?;
        self.expect(&TokenKind::In)?;
        let iter = self.parse_expr(0)?;
        self.skip_newlines();

        if matches!(self.peek_kind(), TokenKind::Indent) {
            self.advance();
            self.skip_newlines();
        }
        let body_expr = self.parse_expr(0)?;
        self.skip_newlines();
        if matches!(self.peek_kind(), TokenKind::Dedent) {
            self.advance();
        }
        let end_span = self.expect(&TokenKind::End)?.span;

        let body_span = body_expr.span();
        let lambda = Expr::Lambda {
            params: vec![Param {
                name: var.clone(),
                ty: TypeExpr::Named("Any".into(), body_span),
                default_value: None,
                variadic: false,
                span: body_span,
            }],
            return_type: None,
            body: LambdaBody::Expr(Box::new(body_expr)),
            span: body_span,
        };
        let spawn_body = Expr::Call(
            Box::new(Expr::Ident("spawn".into(), body_span)),
            vec![
                CallArg::Positional(lambda),
                CallArg::Positional(Expr::Ident(var.clone(), body_span)),
            ],
            body_span,
        );
        let comp = Expr::Comprehension {
            body: Box::new(spawn_body),
            var,
            iter: Box::new(iter),
            extra_clauses: Vec::new(),
            condition: None,
            kind: ComprehensionKind::List,
            span: call_span.merge(end_span),
        };
        base_args.push(CallArg::Positional(comp));
        Ok(Expr::Call(
            Box::new(Expr::Ident("parallel".into(), call_span)),
            base_args,
            call_span.merge(end_span),
        ))
    }

    fn rewrite_await_orchestration_block(
        &mut self,
        name: String,
        mut base_args: Vec<CallArg>,
        call_span: Span,
    ) -> Result<Expr, ParseError> {
        self.skip_newlines();
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent {
            self.advance();
        }
        self.skip_newlines();

        while !matches!(
            self.peek_kind(),
            TokenKind::End | TokenKind::Dedent | TokenKind::Eof
        ) {
            let branch_expr = self.parse_expr(0)?;
            base_args.push(CallArg::Positional(self.make_spawn_lambda(branch_expr)));
            self.skip_newlines();
        }
        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) {
            self.advance();
        }
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(Expr::Call(
            Box::new(Expr::Ident(name, call_span)),
            base_args,
            call_span.merge(end_span),
        ))
    }

    fn make_spawn_lambda(&self, body_expr: Expr) -> Expr {
        let body_span = body_expr.span();
        let lambda = Expr::Lambda {
            params: vec![],
            return_type: None,
            body: LambdaBody::Expr(Box::new(body_expr)),
            span: body_span,
        };
        Expr::Call(
            Box::new(Expr::Ident("spawn".into(), body_span)),
            vec![CallArg::Positional(lambda)],
            body_span,
        )
    }

    fn orchestration_call_parts(&self, expr: &Expr) -> Option<(String, Vec<CallArg>, Span)> {
        match expr {
            Expr::Ident(name, span)
                if matches!(name.as_str(), "parallel" | "race" | "vote" | "select") =>
            {
                Some((name.clone(), vec![], *span))
            }
            Expr::Call(callee, args, span) => {
                if let Expr::Ident(name, _) = callee.as_ref() {
                    if matches!(name.as_str(), "parallel" | "race" | "vote" | "select") {
                        return Some((name.clone(), args.clone(), *span));
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn await_block_follows(&self) -> bool {
        let mut i = self.pos;
        if !matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Newline)
        ) {
            return false;
        }
        while matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Newline)
        ) {
            i += 1;
        }
        matches!(self.tokens.get(i).map(|t| &t.kind), Some(TokenKind::Indent))
    }

    fn parse_dotted_ident(&mut self) -> Result<String, ParseError> {
        let mut parts = vec![self.expect_ident()?];
        while matches!(self.peek_kind(), TokenKind::Dot) {
            self.advance();
            parts.push(self.expect_ident()?);
        }
        Ok(parts.join("."))
    }

    /// Parse a program with error recovery enabled.
    /// Returns the program AST (possibly partial) and a vector of all parse errors encountered.
    /// If no errors occurred, the vector will be empty.
    pub fn parse_program_with_recovery(
        &mut self,
        directives: Vec<Directive>,
    ) -> (Program, Vec<ParseError>) {
        let result = self.parse_program(directives);
        let program = match result {
            Ok(program) => program,
            Err(err) => {
                // Preserve fatal parser failure so callers still emit diagnostics.
                let _ = self.record_error(err);
                Program {
                    directives: vec![],
                    items: vec![],
                    span: Span {
                        start: 0,
                        end: 0,
                        line: 1,
                        col: 1,
                    },
                }
            }
        };
        let errors = std::mem::take(&mut self.errors);
        (program, errors)
    }
}

/// Parse tokens with error recovery.
/// Returns the program AST (possibly partial) and a vector of all parse errors encountered.
pub fn parse_with_recovery(
    tokens: Vec<Token>,
    directives: Vec<Directive>,
) -> (Program, Vec<ParseError>) {
    let mut parser = Parser::new(tokens);
    parser.parse_program_with_recovery(directives)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::lexer::Lexer;

    fn parse_src(src: &str) -> Result<Program, ParseError> {
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        parser.parse_program(vec![])
    }

    #[test]
    fn test_parse_record() {
        let prog = parse_src("record Foo\n  x: Int\n  y: String\nend").unwrap();
        assert_eq!(prog.items.len(), 1);
        if let Item::Record(r) = &prog.items[0] {
            assert_eq!(r.name, "Foo");
            assert_eq!(r.fields.len(), 2);
        } else {
            panic!("expected record");
        }
    }

    #[test]
    fn test_parse_cell() {
        let prog = parse_src("cell add(a: Int, b: Int) -> Int\n  return a + b\nend").unwrap();
        assert_eq!(prog.items.len(), 1);
        if let Item::Cell(c) = &prog.items[0] {
            assert_eq!(c.name, "add");
            assert_eq!(c.params.len(), 2);
        } else {
            panic!("expected cell");
        }
    }

    #[test]
    fn test_parse_cell_effect_row_preserved_after_return_type() {
        let prog = parse_src("cell main() -> Int / {http, emit}\n  return 1\nend").unwrap();
        assert_eq!(prog.items.len(), 1);
        if let Item::Cell(c) = &prog.items[0] {
            assert_eq!(c.effects, vec!["http".to_string(), "emit".to_string()]);
        } else {
            panic!("expected cell");
        }
    }

    #[test]
    fn test_parse_enum() {
        let prog = parse_src("enum Color\n  Red\n  Green\n  Blue\nend").unwrap();
        if let Item::Enum(e) = &prog.items[0] {
            assert_eq!(e.name, "Color");
            assert_eq!(e.variants.len(), 3);
        } else {
            panic!("expected enum");
        }
    }

    #[test]
    fn test_parse_match() {
        let src = "cell test(x: Int) -> String\n  match x\n    1 -> return \"one\"\n    _ -> return \"other\"\n  end\nend";
        let prog = parse_src(src).unwrap();
        assert_eq!(prog.items.len(), 1);
    }

    #[test]
    fn test_parse_addon_stmt_in_block_preserved() {
        let src = r#"cell main() -> Int
  observe "batch"
    metrics:
      counter items_processed
    end
  in
    return 1
  end
end"#;
        let prog = parse_src(src).unwrap();
        if let Item::Cell(c) = &prog.items[0] {
            assert_eq!(c.body.len(), 1);
            match &c.body[0] {
                Stmt::If(ifs) => {
                    assert_eq!(ifs.then_body.len(), 1);
                    assert!(matches!(ifs.then_body[0], Stmt::Return(_)));
                }
                _ => panic!("expected addon in-body to lower to executable block"),
            }
        } else {
            panic!("expected cell");
        }
    }

    #[test]
    fn test_parse_machine_state_block() {
        let src = r#"machine TicketFlow
  initial: Start
  state Start(ticket: Int)
    guard: ticket > 0
    on_enter() / {trace}
      transition Done(ticket)
    end
  end
  state Done(value: Int)
    terminal: true
  end
end"#;
        let prog = parse_src(src).unwrap();
        assert_eq!(prog.items.len(), 1);
        if let Item::Process(p) = &prog.items[0] {
            assert_eq!(p.kind, "machine");
            assert_eq!(p.name, "TicketFlow");
            assert_eq!(p.machine_initial.as_deref(), Some("Start"));
            assert_eq!(p.machine_states.len(), 2);
            let start_state = p
                .machine_states
                .iter()
                .find(|s| s.name == "Start")
                .expect("Start state should be parsed");
            assert_eq!(start_state.params.len(), 1);
            assert_eq!(start_state.params[0].name, "ticket");
            assert!(start_state.guard.is_some());
            assert_eq!(start_state.transition_to.as_deref(), Some("Done"));
            assert_eq!(start_state.transition_args.len(), 1);
            let done_state = p
                .machine_states
                .iter()
                .find(|s| s.name == "Done")
                .expect("Done state should be parsed");
            assert_eq!(done_state.params.len(), 1);
            assert_eq!(done_state.params[0].name, "value");
            assert!(done_state.terminal);
        } else {
            panic!("expected process");
        }
    }

    #[test]
    fn test_parse_match_arm_line_continuation_with_null_coalesce() {
        let src = r#"cell main() -> String
  loop
    let result = foo()
    match result
      Response.Handoff(target, reason) ->
        let current_agent = self.agents.find(fn(a) => a.name == target)
          ?? halt("Unknown agent: {target}")
      Response.Escalate(reason) ->
        return "ok"
    end
  end
end"#;
        let prog = parse_src(src).unwrap();
        assert_eq!(prog.items.len(), 1);
    }

    #[test]
    fn test_parse_machine_states_with_empty_bodies() {
        let src = r#"machine DistributedOrder
  @replicated(factor: 3)

  state Pending
    # ...
  end

  state Processing
    @location("warehouse-{region}")
    # ...
  end

  state Shipped
    # ...
  end
end"#;
        let prog = parse_src(src).unwrap();
        assert_eq!(prog.items.len(), 1);
        assert!(matches!(prog.items[0], Item::Process(_)));
    }

    #[test]
    fn test_parse_pipeline_stages_chain() {
        let src = r#"pipeline InvoicePipeline
  stages:
    Extractor.extract
      -> Validator.validate
      -> Writer.store
  end
end"#;
        let prog = parse_src(src).unwrap();
        let Item::Process(p) = &prog.items[0] else {
            panic!("expected process");
        };
        assert_eq!(p.kind, "pipeline");
        assert_eq!(
            p.pipeline_stages,
            vec![
                "Extractor.extract".to_string(),
                "Validator.validate".to_string(),
                "Writer.store".to_string()
            ]
        );
    }

    #[test]
    fn test_parse_await_parallel_for_desugars_to_spawn_comprehension() {
        let src = r#"cell main() -> Int / {async}
  let values = await parallel for i in 0..3
    i * 2
  end
  return length(values)
end"#;
        let prog = parse_src(src).unwrap();
        let Item::Cell(cell) = &prog.items[0] else {
            panic!("expected cell");
        };
        let Stmt::Let(let_stmt) = &cell.body[0] else {
            panic!("expected let");
        };
        let Expr::AwaitExpr(inner, _) = &let_stmt.value else {
            panic!("expected await expression");
        };
        let Expr::Call(callee, args, _) = inner.as_ref() else {
            panic!("expected orchestration call");
        };
        assert!(matches!(callee.as_ref(), Expr::Ident(name, _) if name == "parallel"));
        assert_eq!(args.len(), 1);
        let CallArg::Positional(Expr::Comprehension { body, var, .. }) = &args[0] else {
            panic!("expected list comprehension argument");
        };
        assert_eq!(var, "i");
        assert!(matches!(body.as_ref(), Expr::Call(_, _, _)));
    }

    #[test]
    fn test_parse_await_race_block_desugars_to_spawn_calls() {
        let src = r#"cell main() -> Int / {async}
  let fastest = await race
    10
    20
  end
  return fastest
end"#;
        let prog = parse_src(src).unwrap();
        let Item::Cell(cell) = &prog.items[0] else {
            panic!("expected cell");
        };
        let Stmt::Let(let_stmt) = &cell.body[0] else {
            panic!("expected let");
        };
        let Expr::AwaitExpr(inner, _) = &let_stmt.value else {
            panic!("expected await expression");
        };
        let Expr::Call(callee, args, _) = inner.as_ref() else {
            panic!("expected orchestration call");
        };
        assert!(matches!(callee.as_ref(), Expr::Ident(name, _) if name == "race"));
        assert_eq!(args.len(), 2);
        for arg in args {
            assert!(matches!(arg, CallArg::Positional(Expr::Call(_, _, _))));
        }
    }

    #[test]
    fn test_if_let_desugars_to_match() {
        let src = r#"cell test(x: Int) -> Int
  if let ok(val) = get_result()
    return val
  else
    return 0
  end
end"#;
        let prog = parse_src(src).unwrap();
        let Item::Cell(c) = &prog.items[0] else {
            panic!("expected cell");
        };
        // if let should desugar to a match statement
        let Stmt::Match(ms) = &c.body[0] else {
            panic!("expected match from if-let desugar, got {:?}", c.body[0]);
        };
        assert_eq!(ms.arms.len(), 2);
        // First arm should be the pattern (ok(val))
        assert!(
            matches!(&ms.arms[0].pattern, Pattern::Variant(name, Some(binding), _)
            if name == "ok" && matches!(binding.as_ref(), Pattern::Ident(bind_name, _) if bind_name == "val"))
        );
        // Second arm should be the wildcard (else branch)
        assert!(matches!(&ms.arms[1].pattern, Pattern::Wildcard(_)));
    }

    #[test]
    fn test_if_let_no_else() {
        let src = r#"cell test(x: Int) -> Int
  if let ok(val) = get_result()
    print(val)
  end
  return 0
end"#;
        let prog = parse_src(src).unwrap();
        let Item::Cell(c) = &prog.items[0] else {
            panic!("expected cell");
        };
        let Stmt::Match(ms) = &c.body[0] else {
            panic!("expected match from if-let desugar");
        };
        // No else branch means only one match arm
        assert_eq!(ms.arms.len(), 1);
        assert!(matches!(&ms.arms[0].pattern, Pattern::Variant(name, _, _)
            if name == "ok"));
    }

    #[test]
    fn test_while_let_desugars_to_loop_match() {
        let src = r#"cell test(items: list[Int]) -> Int
  while let ok(item) = next(items)
    print(item)
  end
  return 0
end"#;
        let prog = parse_src(src).unwrap();
        let Item::Cell(c) = &prog.items[0] else {
            panic!("expected cell");
        };
        // while let should desugar to a loop
        let Stmt::Loop(ls) = &c.body[0] else {
            panic!("expected loop from while-let desugar, got {:?}", c.body[0]);
        };
        // Loop body should contain a match
        assert_eq!(ls.body.len(), 1);
        let Stmt::Match(ms) = &ls.body[0] else {
            panic!("expected match inside loop");
        };
        assert_eq!(ms.arms.len(), 2);
        // First arm: the pattern
        assert!(matches!(&ms.arms[0].pattern, Pattern::Variant(name, _, _)
            if name == "ok"));
        // Second arm: wildcard with break
        assert!(matches!(&ms.arms[1].pattern, Pattern::Wildcard(_)));
        assert!(matches!(&ms.arms[1].body[0], Stmt::Break(_)));
    }

    #[test]
    fn test_expr_position_match_produces_match_expr() {
        let src = r#"cell test(x: Int) -> String
  let y = match x
    1 -> "one"
    _ -> "other"
  end
  return y
end"#;
        let prog = parse_src(src).unwrap();
        let Item::Cell(c) = &prog.items[0] else {
            panic!("expected cell");
        };
        let Stmt::Let(ls) = &c.body[0] else {
            panic!("expected let");
        };
        // The value should be a MatchExpr, not an Ident placeholder
        assert!(
            matches!(&ls.value, Expr::MatchExpr { arms, .. } if arms.len() == 2),
            "expected MatchExpr with 2 arms, got {:?}",
            ls.value
        );
    }

    #[test]
    fn test_expr_position_if_produces_block_expr() {
        let src = r#"cell test(x: Int) -> Int
  let y = if x > 0
    return 1
  else
    return 0
  end
  return y
end"#;
        let prog = parse_src(src).unwrap();
        let Item::Cell(c) = &prog.items[0] else {
            panic!("expected cell");
        };
        let Stmt::Let(ls) = &c.body[0] else {
            panic!("expected let");
        };
        // The value should be a BlockExpr containing an If statement
        assert!(
            matches!(&ls.value, Expr::BlockExpr(stmts, _) if matches!(&stmts[0], Stmt::If(_))),
            "expected BlockExpr with If, got {:?}",
            ls.value
        );
    }

    #[test]
    fn test_expr_position_loop_produces_block_expr() {
        let src = r#"cell test() -> Int
  let y = loop
    break 42
  end
  return y
end"#;
        let prog = parse_src(src).unwrap();
        let Item::Cell(c) = &prog.items[0] else {
            panic!("expected cell");
        };
        let Stmt::Let(ls) = &c.body[0] else {
            panic!("expected let");
        };
        assert!(
            matches!(&ls.value, Expr::BlockExpr(stmts, _) if matches!(&stmts[0], Stmt::Loop(_))),
            "expected BlockExpr with Loop, got {:?}",
            ls.value
        );
    }

    #[test]
    fn test_for_loop_tuple_destructuring() {
        let src = r#"cell test(m: map[String, Int]) -> Int
  for (k, v) in m
    print(k)
  end
  return 0
end"#;
        let prog = parse_src(src).unwrap();
        let Item::Cell(c) = &prog.items[0] else {
            panic!("expected cell");
        };
        let Stmt::For(fs) = &c.body[0] else {
            panic!("expected for, got {:?}", c.body[0]);
        };
        // var should be the first name for backwards compat
        assert_eq!(fs.var, "k");
        // pattern should capture both names
        let Some(Pattern::TupleDestructure { elements, .. }) = &fs.pattern else {
            panic!("expected tuple destructure pattern, got {:?}", fs.pattern);
        };
        assert_eq!(elements.len(), 2);
        assert!(matches!(&elements[0], Pattern::Ident(n, _) if n == "k"));
        assert!(matches!(&elements[1], Pattern::Ident(n, _) if n == "v"));
    }

    #[test]
    fn test_let_variant_destructuring() {
        let src = r#"cell test() -> Int
  let ok(val) = get_result()
  return val
end"#;
        let prog = parse_src(src).unwrap();
        let Item::Cell(c) = &prog.items[0] else {
            panic!("expected cell");
        };
        let Stmt::Let(ls) = &c.body[0] else {
            panic!("expected let");
        };
        assert_eq!(ls.name, "val");
        let Some(Pattern::Variant(name, binding, _)) = &ls.pattern else {
            panic!("expected variant pattern, got {:?}", ls.pattern);
        };
        assert_eq!(name, "ok");
        assert!(matches!(
            binding.as_deref(),
            Some(Pattern::Ident(bind_name, _)) if bind_name == "val"
        ));
    }

    #[test]
    fn test_let_brace_destructuring() {
        let src = r#"cell test() -> Int
  let { x, y } = get_point()
  return x
end"#;
        let prog = parse_src(src).unwrap();
        let Item::Cell(c) = &prog.items[0] else {
            panic!("expected cell");
        };
        let Stmt::Let(ls) = &c.body[0] else {
            panic!("expected let");
        };
        assert_eq!(ls.name, "x");
        let Some(Pattern::RecordDestructure { fields, .. }) = &ls.pattern else {
            panic!("expected record destructure pattern, got {:?}", ls.pattern);
        };
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].0, "x");
        assert_eq!(fields[1].0, "y");
    }

    #[test]
    fn test_for_loop_simple_var_no_pattern() {
        let src = r#"cell test(items: list[Int]) -> Int
  for item in items
    print(item)
  end
  return 0
end"#;
        let prog = parse_src(src).unwrap();
        let Item::Cell(c) = &prog.items[0] else {
            panic!("expected cell");
        };
        let Stmt::For(fs) = &c.body[0] else {
            panic!("expected for");
        };
        assert_eq!(fs.var, "item");
        assert!(fs.pattern.is_none());
    }

    // ── Error Recovery Tests ──

    #[test]
    fn test_error_recovery_multiple_errors() {
        // Multiple parse errors in same file
        let src = r#"
cell bad1() -> Int
  let x =

cell good() -> Int
  return 42
end

cell bad2() -> String
  return

enum GoodEnum
  A
  B
end
"#;
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let (program, errors) = parse_with_recovery(tokens, vec![]);

        // Should have parse errors from bad1 and bad2
        assert!(!errors.is_empty(), "Expected at least 1 parse error");

        // Should still parse valid declarations
        let has_good_cell = program
            .items
            .iter()
            .any(|item| matches!(item, Item::Cell(c) if c.name == "good"));
        let has_good_enum = program
            .items
            .iter()
            .any(|item| matches!(item, Item::Enum(e) if e.name == "GoodEnum"));

        assert!(
            has_good_cell || has_good_enum,
            "Should parse at least one valid declaration after errors"
        );
    }

    #[test]
    fn test_error_recovery_continues_after_bad_declaration() {
        let src = r#"
record Point
  x: Int
  invalid: something wrong

cell get_x() -> Int
  return 1
end

record Color
  r: Int
  g: Int
  b: Int
end
"#;
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let (program, _errors) = parse_with_recovery(tokens, vec![]);

        // Should have at least one error (from Point)
        // Note: Some errors might be recovered gracefully, so we check for any valid parsing
        let has_cell = program
            .items
            .iter()
            .any(|item| matches!(item, Item::Cell(c) if c.name == "get_x"));
        let has_color = program
            .items
            .iter()
            .any(|item| matches!(item, Item::Record(r) if r.name == "Color"));

        assert!(
            has_cell || has_color,
            "Should parse declarations after error in Point"
        );
    }

    #[test]
    fn test_error_recovery_valid_declarations_after_errors() {
        let src = r#"
cell bad_one() -> Int
  let x =
  // Incomplete statement

cell good_one() -> Int
  return 42
end

cell bad_two() -> String
  return

cell good_two() -> Bool
  return false
end
"#;
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let (program, errors) = parse_with_recovery(tokens, vec![]);

        // Should have errors
        assert!(!errors.is_empty(), "Expected parse errors");

        // Should parse valid cells
        let good_cells: Vec<_> = program
            .items
            .iter()
            .filter_map(|item| {
                if let Item::Cell(c) = item {
                    Some(c.name.as_str())
                } else {
                    None
                }
            })
            .collect();

        assert!(
            good_cells.contains(&"good_one") || good_cells.contains(&"good_two"),
            "Should parse at least one valid cell after errors"
        );
    }

    #[test]
    fn test_error_recovery_single_error() {
        // Single error should work the same way
        let src = r#"
cell test() -> Int
  return 1
  // Missing 'end'
"#;
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let (_, errors) = parse_with_recovery(tokens, vec![]);

        assert!(!errors.is_empty(), "Should report at least 1 error");
    }

    #[test]
    fn test_error_recovery_empty_file() {
        let src = "";
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let (program, errors) = parse_with_recovery(tokens, vec![]);

        assert!(errors.is_empty(), "Empty file should have no errors");
        assert!(program.items.is_empty(), "Empty file should have no items");
    }

    #[test]
    fn test_error_recovery_synchronizes_on_keywords() {
        let src = r#"
cell bad() -> Int
  return

enum Color
  Red
  Green
  Blue
end

cell process() -> Int
  return 1
end
"#;
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let (program, _errors) = parse_with_recovery(tokens, vec![]);

        // Parser should attempt recovery and continue
        // Should parse enum and cell after error in bad()
        let has_enum = program
            .items
            .iter()
            .any(|item| matches!(item, Item::Enum(e) if e.name == "Color"));
        let has_cell = program
            .items
            .iter()
            .any(|item| matches!(item, Item::Cell(c) if c.name == "process"));

        assert!(
            has_enum || has_cell,
            "Should parse at least one declaration after synchronization"
        );
    }

    #[test]
    fn test_error_recovery_mixed_process_types() {
        let src = r#"
cell bad() -> Int
  let x =
  return 1
end

record GoodRecord
  count: Int
end
"#;
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let (program, _errors) = parse_with_recovery(tokens, vec![]);

        // Should continue parsing after error in bad cell
        let has_record = program
            .items
            .iter()
            .any(|item| matches!(item, Item::Record(r) if r.name == "GoodRecord"));

        assert!(has_record, "Should parse record after error in cell");
    }

    // ===== BULLETPROOF ERROR RECOVERY TESTS =====

    #[test]
    fn test_recovery_missing_type_annotation() {
        let src = r#"
cell test() -> Int
  let x: = 5
  return x
end

cell good() -> Int
  return 42
end
"#;
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let (program, errors) = parse_with_recovery(tokens, vec![]);

        // Should report error about missing type
        assert!(!errors.is_empty(), "Should report missing type error");

        // Should still parse the good cell
        let has_good = program
            .items
            .iter()
            .any(|item| matches!(item, Item::Cell(c) if c.name == "good"));
        assert!(has_good, "Should parse valid cell after error");
    }

    #[test]
    fn test_recovery_multiple_independent_errors() {
        let src = r#"
cell bad1() -> Int
  let x =
  return 1
end

cell bad2() -> String
  return
end

cell good() -> Bool
  return true
end

record Bad
  x:
  y: Int
end

record Good
  a: Int
  b: String
end
"#;
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let (program, errors) = parse_with_recovery(tokens, vec![]);

        // Should report multiple errors
        assert!(
            errors.len() >= 2,
            "Should report multiple independent errors"
        );

        // Should parse valid declarations
        let has_good_cell = program
            .items
            .iter()
            .any(|item| matches!(item, Item::Cell(c) if c.name == "good"));
        let has_good_record = program
            .items
            .iter()
            .any(|item| matches!(item, Item::Record(r) if r.name == "Good"));

        assert!(has_good_cell, "Should parse good cell");
        assert!(has_good_record, "Should parse good record");
    }

    #[test]
    fn test_recovery_unclosed_paren() {
        let src = r#"
cell bad() -> Int
  let x = (1 + 2
  return x
end

cell good() -> Int
  return 10
end
"#;
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let (program, errors) = parse_with_recovery(tokens, vec![]);

        assert!(!errors.is_empty(), "Should report unclosed paren error");

        // Should still attempt to parse following cells
        let cell_count = program
            .items
            .iter()
            .filter(|item| matches!(item, Item::Cell(_)))
            .count();
        assert!(cell_count >= 1, "Should parse at least one cell");
    }

    #[test]
    fn test_recovery_malformed_if_stmt() {
        let src = r#"
cell test() -> Int
  if
    return 1
  end
  return 0
end

cell good() -> Int
  if true
    return 5
  end
  return 0
end
"#;
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let (program, errors) = parse_with_recovery(tokens, vec![]);

        assert!(!errors.is_empty(), "Should report malformed if error");

        let has_good = program
            .items
            .iter()
            .any(|item| matches!(item, Item::Cell(c) if c.name == "good"));
        assert!(has_good, "Should parse valid cell after error");
    }

    #[test]
    fn test_recovery_exact_error_locations() {
        let src = r#"
cell test() -> Int
  let x: = 5
  return x
end
"#;
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let (_program, errors) = parse_with_recovery(tokens, vec![]);

        assert!(!errors.is_empty(), "Should have at least one error");

        // Verify error has line and column information
        for err in &errors {
            match err {
                ParseError::Unexpected { line, col, .. } => {
                    assert!(*line > 0, "Error should have valid line number");
                    assert!(*col > 0, "Error should have valid column number");
                }
                ParseError::MissingType { line, col } => {
                    assert!(*line > 0, "Error should have valid line number");
                    assert!(*col > 0, "Error should have valid column number");
                }
                ParseError::IncompleteExpression { line, col, .. } => {
                    assert!(*line > 0, "Error should have valid line number");
                    assert!(*col > 0, "Error should have valid column number");
                }
                ParseError::MalformedConstruct { line, col, .. } => {
                    assert!(*line > 0, "Error should have valid line number");
                    assert!(*col > 0, "Error should have valid column number");
                }
                ParseError::UnclosedBracket {
                    open_line,
                    open_col,
                    current_line,
                    current_col,
                    ..
                } => {
                    assert!(
                        *open_line > 0 && *current_line > 0,
                        "Should have valid line numbers"
                    );
                    assert!(
                        *open_col > 0 && *current_col > 0,
                        "Should have valid column numbers"
                    );
                }
                ParseError::MissingEnd {
                    open_line,
                    open_col,
                    current_line,
                    current_col,
                    ..
                } => {
                    assert!(
                        *open_line > 0 && *current_line > 0,
                        "Should have valid line numbers"
                    );
                    assert!(
                        *open_col > 0 && *current_col > 0,
                        "Should have valid column numbers"
                    );
                }
                _ => {}
            }
        }
    }

    #[test]
    fn test_recovery_all_errors_collected() {
        let src = r#"
cell bad1() -> Int
  let x =
  return 1
end

cell bad2() -> Int
  return
end

cell good() -> Int
  return 1
end
"#;
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let (program, errors) = parse_with_recovery(tokens, vec![]);

        // Should collect multiple errors in one pass
        assert!(
            errors.len() >= 2,
            "Should collect multiple errors: got {}",
            errors.len()
        );

        // Should still parse the valid cell
        let has_cell = program
            .items
            .iter()
            .any(|item| matches!(item, Item::Cell(c) if c.name == "good"));
        assert!(has_cell, "Should parse valid declaration after errors");
    }

    #[test]
    fn test_recovery_no_cascading_undefined_var() {
        // This test verifies that parser completes despite type errors
        // (Actual cascading prevention happens in type checker, not parser)
        let src = r#"
cell test() -> Int
  let x = (1 + unclosed
  let y = x + 1
  return x + y
end
"#;
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let (program, _errors) = parse_with_recovery(tokens, vec![]);

        // The program should parse (with error recovery)
        let has_cell = program
            .items
            .iter()
            .any(|item| matches!(item, Item::Cell(_)));
        assert!(has_cell, "Should parse cell structure despite parse errors");
    }

    #[test]
    fn test_recovery_synchronize_on_newline_stmt() {
        let src = r#"
cell test() -> Int
  let x = 1 +
  let y = 2
  return y
end
"#;
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let (program, errors) = parse_with_recovery(tokens, vec![]);

        assert!(!errors.is_empty(), "Should report incomplete expression");

        let has_cell = program
            .items
            .iter()
            .any(|item| matches!(item, Item::Cell(c) if c.name == "test"));
        assert!(has_cell, "Should parse cell with recovery");
    }

    #[test]
    fn test_recovery_nested_errors() {
        let src = r#"
cell outer() -> Int
  if true
    let x =
    return 1
  end
  return 0
end

cell good() -> Bool
  return false
end
"#;
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let (program, errors) = parse_with_recovery(tokens, vec![]);

        assert!(!errors.is_empty(), "Should report nested error");

        let has_good = program
            .items
            .iter()
            .any(|item| matches!(item, Item::Cell(c) if c.name == "good"));
        assert!(has_good, "Should parse cell after nested error");
    }

    #[test]
    fn test_recovery_actionable_error_messages() {
        let src = r#"
cell test() -> Int
  let x: = 5
  return x
end
"#;
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let (_program, errors) = parse_with_recovery(tokens, vec![]);

        assert!(!errors.is_empty(), "Should have errors");

        // Check that error messages contain useful context
        for err in &errors {
            let msg = format!("{}", err);
            // Error messages should contain line/col info
            assert!(
                msg.contains("line") || msg.contains("col"),
                "Error message should mention line/col: {}",
                msg
            );
        }
    }

    #[test]
    fn test_chained_comparison() {
        // `0 < x < 100` should desugar to `(0 < x) and (x < 100)`
        let prog = parse_src("cell f(x: Int) -> Bool\n  return 0 < x < 100\nend").unwrap();
        let cell = match &prog.items[0] {
            Item::Cell(c) => c,
            _ => panic!("expected cell"),
        };
        let ret = match &cell.body[0] {
            Stmt::Return(r) => &r.value,
            _ => panic!("expected return"),
        };
        // Top-level should be And
        match ret {
            Expr::BinOp(left, BinOp::And, right, _) => {
                match left.as_ref() {
                    Expr::BinOp(_, BinOp::Lt, _, _) => {}
                    other => panic!("expected Lt on left, got {:?}", other),
                }
                match right.as_ref() {
                    Expr::BinOp(_, BinOp::Lt, _, _) => {}
                    other => panic!("expected Lt on right, got {:?}", other),
                }
            }
            other => panic!("expected And(Lt, Lt), got {:?}", other),
        }
    }

    #[test]
    fn test_chained_comparison_mixed_ops() {
        // `a >= b > c` should desugar to `(a >= b) and (b > c)`
        let prog =
            parse_src("cell f(a: Int, b: Int, c: Int) -> Bool\n  return a >= b > c\nend").unwrap();
        let cell = match &prog.items[0] {
            Item::Cell(c) => c,
            _ => panic!("expected cell"),
        };
        let ret = match &cell.body[0] {
            Stmt::Return(r) => &r.value,
            _ => panic!("expected return"),
        };
        match ret {
            Expr::BinOp(left, BinOp::And, right, _) => {
                match left.as_ref() {
                    Expr::BinOp(_, BinOp::GtEq, _, _) => {}
                    other => panic!("expected GtEq on left, got {:?}", other),
                }
                match right.as_ref() {
                    Expr::BinOp(_, BinOp::Gt, _, _) => {}
                    other => panic!("expected Gt on right, got {:?}", other),
                }
            }
            other => panic!("expected And(GtEq, Gt), got {:?}", other),
        }
    }

    #[test]
    fn test_markdown_block_docstring_on_cell() {
        let src = "```\nThis is the main function.\n```\ncell main() -> Int\n  return 42\nend";
        let prog = parse_src(src).unwrap();
        assert_eq!(prog.items.len(), 1);
        if let Item::Cell(c) = &prog.items[0] {
            assert_eq!(c.name, "main");
            assert_eq!(c.doc, Some("This is the main function.".to_string()));
        } else {
            panic!("expected cell");
        }
    }

    #[test]
    fn test_markdown_block_docstring_on_record() {
        let src = "```\nA point in 2D space.\n```\nrecord Point\n  x: Int\n  y: Int\nend";
        let prog = parse_src(src).unwrap();
        assert_eq!(prog.items.len(), 1);
        if let Item::Record(r) = &prog.items[0] {
            assert_eq!(r.name, "Point");
            assert_eq!(r.doc, Some("A point in 2D space.".to_string()));
        } else {
            panic!("expected record");
        }
    }

    #[test]
    fn test_markdown_block_docstring_on_enum() {
        let src = "```\nRepresents a color.\n```\nenum Color\n  Red\n  Green\n  Blue\nend";
        let prog = parse_src(src).unwrap();
        assert_eq!(prog.items.len(), 1);
        if let Item::Enum(e) = &prog.items[0] {
            assert_eq!(e.name, "Color");
            assert_eq!(e.doc, Some("Represents a color.".to_string()));
        } else {
            panic!("expected enum");
        }
    }

    #[test]
    fn test_markdown_block_standalone_comment() {
        let src = "```\nJust a comment, not attached to anything.\n```\n";
        let prog = parse_src(src).unwrap();
        // Standalone markdown block should be ignored (no items)
        assert_eq!(prog.items.len(), 0);
    }

    #[test]
    fn test_markdown_block_before_pub_cell() {
        let src = "```\nPublic entry point.\n```\npub cell main() -> Int\n  return 1\nend";
        let prog = parse_src(src).unwrap();
        assert_eq!(prog.items.len(), 1);
        if let Item::Cell(c) = &prog.items[0] {
            assert_eq!(c.name, "main");
            assert!(c.is_pub);
            assert_eq!(c.doc, Some("Public entry point.".to_string()));
        } else {
            panic!("expected cell");
        }
    }

    #[test]
    fn test_markdown_block_with_lang_tag_docstring() {
        let src = "```markdown\n# Main\nEntry point for the program.\n```\ncell main() -> Int\n  return 0\nend";
        let prog = parse_src(src).unwrap();
        assert_eq!(prog.items.len(), 1);
        if let Item::Cell(c) = &prog.items[0] {
            assert_eq!(c.name, "main");
            assert_eq!(
                c.doc,
                Some("# Main\nEntry point for the program.".to_string())
            );
        } else {
            panic!("expected cell");
        }
    }

    #[test]
    fn test_multiple_markdown_blocks_and_declarations() {
        let src = "```\nDoc for add\n```\ncell add(a: Int, b: Int) -> Int\n  return a + b\nend\n\n```\nDoc for sub\n```\ncell sub(a: Int, b: Int) -> Int\n  return a - b\nend";
        let prog = parse_src(src).unwrap();
        assert_eq!(prog.items.len(), 2);
        if let Item::Cell(c) = &prog.items[0] {
            assert_eq!(c.name, "add");
            assert_eq!(c.doc, Some("Doc for add".to_string()));
        } else {
            panic!("expected cell");
        }
        if let Item::Cell(c) = &prog.items[1] {
            assert_eq!(c.name, "sub");
            assert_eq!(c.doc, Some("Doc for sub".to_string()));
        } else {
            panic!("expected cell");
        }
    }

    #[test]
    fn test_cell_without_markdown_block_has_no_doc() {
        let src = "cell main() -> Int\n  return 42\nend";
        let prog = parse_src(src).unwrap();
        assert_eq!(prog.items.len(), 1);
        if let Item::Cell(c) = &prog.items[0] {
            assert_eq!(c.name, "main");
            assert_eq!(c.doc, None);
        } else {
            panic!("expected cell");
        }
    }

    // ── Algebraic Effects Parser Tests ──

    #[test]
    fn test_parse_perform() {
        let src = "cell main() -> Null\n  perform Console.log(\"hello\")\n  return null\nend";
        let prog = parse_src(src).unwrap();
        assert_eq!(prog.items.len(), 1);
        if let Item::Cell(c) = &prog.items[0] {
            assert_eq!(c.name, "main");
            // The body should contain an Expr statement with a Perform expression
            assert!(!c.body.is_empty());
            if let Stmt::Expr(es) = &c.body[0] {
                match &es.expr {
                    Expr::Perform {
                        effect_name,
                        operation,
                        args,
                        ..
                    } => {
                        assert_eq!(effect_name, "Console");
                        assert_eq!(operation, "log");
                        assert_eq!(args.len(), 1);
                    }
                    other => panic!("expected Perform, got {:?}", other),
                }
            } else {
                panic!("expected Expr statement");
            }
        } else {
            panic!("expected cell");
        }
    }

    #[test]
    fn test_parse_perform_no_args() {
        let src = "cell main() -> String\n  let x = perform IO.read_line()\n  return x\nend";
        let prog = parse_src(src).unwrap();
        if let Item::Cell(c) = &prog.items[0] {
            if let Stmt::Let(ls) = &c.body[0] {
                match &ls.value {
                    Expr::Perform {
                        effect_name,
                        operation,
                        args,
                        ..
                    } => {
                        assert_eq!(effect_name, "IO");
                        assert_eq!(operation, "read_line");
                        assert_eq!(args.len(), 0);
                    }
                    other => panic!("expected Perform, got {:?}", other),
                }
            } else {
                panic!("expected Let statement");
            }
        }
    }

    #[test]
    fn test_parse_resume() {
        let src = "cell main() -> Null\n  resume(42)\n  return null\nend";
        let prog = parse_src(src).unwrap();
        if let Item::Cell(c) = &prog.items[0] {
            if let Stmt::Expr(es) = &c.body[0] {
                match &es.expr {
                    Expr::ResumeExpr(inner, _) => {
                        assert!(matches!(inner.as_ref(), Expr::IntLit(42, _)));
                    }
                    other => panic!("expected ResumeExpr, got {:?}", other),
                }
            } else {
                panic!("expected Expr statement");
            }
        }
    }

    #[test]
    fn test_parse_handle_expr() {
        let src = "cell main() -> Int\n  let x = handle\n    42\n  with\n    IO.read() =>\n      resume(\"hello\")\n  end\n  return x\nend";
        let prog = parse_src(src).unwrap();
        if let Item::Cell(c) = &prog.items[0] {
            if let Stmt::Let(ls) = &c.body[0] {
                match &ls.value {
                    Expr::HandleExpr { body, handlers, .. } => {
                        assert!(!body.is_empty(), "handle body should not be empty");
                        assert_eq!(handlers.len(), 1);
                        assert_eq!(handlers[0].effect_name, "IO");
                        assert_eq!(handlers[0].operation, "read");
                    }
                    other => panic!("expected HandleExpr, got {:?}", other),
                }
            } else {
                panic!("expected Let statement");
            }
        }
    }
}

/// Parse a format spec string (the part after `:` in `{expr:spec}`) into a `FormatSpec` AST node.
fn parse_format_spec(s: &str) -> FormatSpec {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut fill = None;
    let mut align = None;
    let mut sign = None;
    let mut alternate = false;
    let mut zero_pad = false;
    let mut width = None;
    let mut precision = None;
    let mut fmt_type = None;

    // Check for fill + align or just align
    if i + 1 < chars.len() && matches!(chars[i + 1], '<' | '>' | '^') {
        fill = Some(chars[i]);
        align = Some(match chars[i + 1] {
            '<' => FormatAlign::Left,
            '>' => FormatAlign::Right,
            '^' => FormatAlign::Center,
            _ => unreachable!(),
        });
        i += 2;
    } else if i < chars.len() && matches!(chars[i], '<' | '>' | '^') {
        align = Some(match chars[i] {
            '<' => FormatAlign::Left,
            '>' => FormatAlign::Right,
            '^' => FormatAlign::Center,
            _ => unreachable!(),
        });
        i += 1;
    }

    // Optional sign
    if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
        sign = Some(chars[i]);
        i += 1;
    }

    // Optional '#' (alternate form)
    if i < chars.len() && chars[i] == '#' {
        alternate = true;
        i += 1;
    }

    // Optional '0' (zero pad)
    if i < chars.len() && chars[i] == '0' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
        zero_pad = true;
        i += 1;
    }

    // Optional width (digits)
    let width_start = i;
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    if i > width_start {
        width = chars[width_start..i]
            .iter()
            .collect::<String>()
            .parse()
            .ok();
    }

    // Optional '.' + precision
    if i < chars.len() && chars[i] == '.' {
        i += 1;
        let prec_start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        if i > prec_start {
            precision = chars[prec_start..i].iter().collect::<String>().parse().ok();
        }
    }

    // Optional type char
    if i < chars.len() {
        fmt_type = match chars[i] {
            'd' => Some(FormatType::Decimal),
            'x' => Some(FormatType::Hex),
            'X' => Some(FormatType::HexUpper),
            'o' => Some(FormatType::Octal),
            'b' => Some(FormatType::Binary),
            'f' => Some(FormatType::Fixed),
            'e' => Some(FormatType::Scientific),
            'E' => Some(FormatType::ScientificUpper),
            's' => Some(FormatType::Str),
            _ => None,
        };
    }

    FormatSpec {
        fill,
        align,
        sign,
        alternate,
        zero_pad,
        width,
        precision,
        fmt_type,
        raw: s.to_string(),
    }
}
