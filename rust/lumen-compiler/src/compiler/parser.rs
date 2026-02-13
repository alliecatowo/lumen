//! Recursive descent parser with Pratt expression parsing for Lumen.

use crate::compiler::ast::*;
use crate::compiler::tokens::{Token, TokenKind};
use thiserror::Error;

#[derive(Debug, Error)]
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
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    bracket_depth: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            bracket_depth: 0,
        }
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
        while !self.at_end() {
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
            if self.is_top_level_stmt_start() {
                top_level_stmts.push(self.parse_stmt()?);
            } else {
                items.push(self.parse_item()?);
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
                where_clauses: vec![],
                span: span_start.merge(end_span),
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
                let mut c = self.parse_cell()?;
                c.is_pub = is_pub;
                c.is_async = is_async;
                Ok(Item::Cell(c))
            }
            TokenKind::At => Ok(Item::Addon(self.parse_attribute_decl()?)),
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
            TokenKind::Extern | TokenKind::Comptime => {
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
                    let start = self.current().span;
                    let name = self.expect_ident().ok();
                    self.consume_rest_of_line();
                    Ok(Item::Addon(AddonDecl {
                        kind: "unknown".into(),
                        name,
                        span: start.merge(self.current().span),
                    }))
                }
            },
            _ => {
                let start = self.current().span;
                let kind = format!("{}", self.peek_kind());
                self.consume_rest_of_line();
                Ok(Item::Addon(AddonDecl {
                    kind: "unknown".into(),
                    name: Some(kind),
                    span: start.merge(self.current().span),
                }))
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
                methods.push(self.parse_cell()?);
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
                    let save = self.pos;
                    let parsed = self.parse_type();
                    if let Ok(ty) = parsed {
                        if matches!(self.peek_kind(), TokenKind::RParen) {
                            self.advance();
                            Some(ty)
                        } else {
                            self.pos = save;
                            self.consume_variant_arg_tokens();
                            while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                                if matches!(self.peek_kind(), TokenKind::Comma) {
                                    self.advance();
                                }
                                self.consume_variant_arg_tokens();
                            }
                            if matches!(self.peek_kind(), TokenKind::RParen) {
                                self.advance();
                            }
                            Some(TypeExpr::Named("Any".into(), vs))
                        }
                    } else {
                        self.pos = save;
                        self.consume_variant_arg_tokens();
                        while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                            if matches!(self.peek_kind(), TokenKind::Comma) {
                                self.advance();
                            }
                            self.consume_variant_arg_tokens();
                        }
                        if matches!(self.peek_kind(), TokenKind::RParen) {
                            self.advance();
                        }
                        Some(TypeExpr::Named("Any".into(), vs))
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
        })
    }

    // ── Cell ──

    fn parse_cell(&mut self) -> Result<CellDef, ParseError> {
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
            let _variadic = if matches!(self.peek_kind(), TokenKind::DotDot | TokenKind::DotDotDot)
            {
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
                where_clauses: vec![],
                span,
            });
        }

        // Prototype/signature form (used in effect declarations and trait-like stubs):
        // cell f(x: Int) -> Int / {http}
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
                    generic_params,
                    params,
                    return_type: ret,
                    effects,
                    body: vec![],
                    is_pub: false,
                    is_async: false,
                    where_clauses: vec![],
                    span: start.merge(end_span),
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
            where_clauses: vec![],
            span: start.merge(end_span),
        })
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut stmts = Vec::new();
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent {
            self.advance();
        }
        self.skip_newlines();
        while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof | TokenKind::Else) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::End | TokenKind::Eof | TokenKind::Else) {
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
                    Some(TokenKind::End | TokenKind::Else | TokenKind::Eof)
                ) {
                    break;
                }
                self.advance();
                continue;
            }
            stmts.push(self.parse_stmt()?);
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
                Some(TokenKind::End | TokenKind::Else | TokenKind::Eof)
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
            stmts.push(self.parse_stmt()?);
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
        // by lowering them to `emit(payload)` and skip their optional body.
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

        self.skip_newlines();
        if matches!(self.peek_kind(), TokenKind::In) {
            self.advance();
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Indent) {
                self.consume_indented_payload();
                self.skip_newlines();
                if matches!(self.peek_kind(), TokenKind::End) {
                    self.advance();
                }
            } else {
                self.consume_block_until_end();
            }
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
        let name = if matches!(self.peek_kind(), TokenKind::LParen) {
            self.advance();
            let first = if !matches!(self.peek_kind(), TokenKind::RParen) {
                Some(self.expect_ident()?)
            } else {
                None
            };
            while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                self.advance();
            }
            if matches!(self.peek_kind(), TokenKind::RParen) {
                self.advance();
            }
            first.unwrap_or_else(|| "__tuple".to_string())
        } else if matches!(self.peek_kind(), TokenKind::LBracket | TokenKind::LBrace) {
            self.consume_balanced_group();
            "__pattern".to_string()
        } else {
            let first = self.expect_ident()?;
            if matches!(self.peek_kind(), TokenKind::LParen) {
                self.consume_balanced_group();
                "__pattern".to_string()
            } else {
                first
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
            pattern: None,
            ty,
            value,
            span,
        }))
    }

    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::If)?.span;
        let cond = if matches!(self.peek_kind(), TokenKind::Let) {
            self.advance();
            self.consume_rest_of_line();
            Expr::BoolLit(true, start)
        } else {
            self.parse_expr(0)?
        };
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
        if matches!(self.peek_kind(), TokenKind::If) {
            self.advance();
            let _ = self.parse_expr(0)?;
        }
        self.skip_newlines();
        let body = self.parse_block()?;
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(Stmt::For(ForStmt {
            var,
            iter,
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
                    TokenKind::Arrow if depth == 0 => break,
                    _ => {
                        self.advance();
                    }
                }
            }
            return Ok(Pattern::Wildcard(s));
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
                Ok(Pattern::Literal(Expr::IntLit(n, s)))
            }
            TokenKind::FloatLit(n) => {
                let s = self.advance().span;
                Ok(Pattern::Literal(Expr::FloatLit(n, s)))
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
                    let binding = self.parse_variant_binding_candidate()?;
                    self.expect(&TokenKind::RParen)?;
                    Ok(Pattern::Variant(vname, binding, s))
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
                    self.advance();
                    let binding = self.parse_variant_binding_candidate()?;
                    self.expect(&TokenKind::RParen)?;
                    Ok(Pattern::Variant(name, binding, s))
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
        let cond = if matches!(self.peek_kind(), TokenKind::Let) {
            self.advance();
            self.consume_rest_of_line();
            Expr::BoolLit(true, start)
        } else {
            self.parse_expr(0)?
        };
        self.skip_newlines();
        let body = self.parse_block()?;
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(Stmt::While(WhileStmt {
            condition: cond,
            body,
            span: start.merge(end_span),
        }))
    }

    fn parse_loop(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::Loop)?.span;
        self.skip_newlines();
        let body = self.parse_block()?;
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(Stmt::Loop(LoopStmt {
            body,
            span: start.merge(end_span),
        }))
    }

    fn parse_break(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::Break)?.span;
        if matches!(self.peek_kind(), TokenKind::At) {
            self.advance();
            let _ = self.expect_ident();
            return Ok(Stmt::Break(BreakStmt {
                value: None,
                span: start,
            }));
        }
        let value = if !matches!(
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
        Ok(Stmt::Break(BreakStmt { value, span }))
    }

    fn parse_continue(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::Continue)?.span;
        Ok(Stmt::Continue(ContinueStmt { span: start }))
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
            methods.push(self.parse_cell()?);
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
            cells.push(self.parse_cell()?);
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
                    let mut cell = self.parse_cell()?;
                    cell.is_async = is_async;
                    if cell.params.first().map(|p| p.name.as_str()) != Some("self") {
                        let self_span = cell.span;
                        cell.params.insert(
                            0,
                            Param {
                                name: "self".into(),
                                ty: TypeExpr::Named("Json".into(), self_span),
                                default_value: None,
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
                operations.push(self.parse_cell()?);
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
                    let mut cell = self.parse_cell()?;
                    cell.is_async = is_async;
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
            cells,
            grants,
            span: start.merge(end_span),
        })
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
                handles.push(self.parse_cell()?);
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
            let _variadic = if matches!(self.peek_kind(), TokenKind::DotDot | TokenKind::DotDotDot)
            {
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
                where_clauses: vec![],
                span,
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
                    where_clauses: vec![],
                    span: start.merge(end_span),
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
            where_clauses: vec![],
            span: start.merge(end_span),
        })
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

    fn parse_variant_binding_candidate(&mut self) -> Result<Option<String>, ParseError> {
        let mut binding = None;
        if !matches!(self.peek_kind(), TokenKind::RParen) {
            if matches!(self.peek_kind(), TokenKind::Ident(_)) {
                binding = Some(self.expect_ident()?);
            } else {
                self.consume_variant_arg_tokens();
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
        let ty = if matches!(self.peek_kind(), TokenKind::Pipe | TokenKind::Ampersand) {
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
            TypeExpr::Union(types, span)
        } else {
            base
        };

        // Optional effect row in type position: fn(...) -> T / E
        if matches!(self.peek_kind(), TokenKind::Slash) {
            let _ = self.parse_optional_effect_row()?;
        }

        Ok(ty)
    }

    fn parse_base_type(&mut self) -> Result<TypeExpr, ParseError> {
        match self.peek_kind().clone() {
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
        }
    }

    // ── Expressions (Pratt parser) ──

    fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_prefix()?;
        loop {
            if matches!(
                self.peek_kind(),
                TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
            ) {
                let mut i = self.pos;
                while matches!(
                    self.tokens.get(i).map(|t| &t.kind),
                    Some(TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent)
                ) {
                    i += 1;
                }
                if matches!(
                    self.tokens.get(i).map(|t| &t.kind),
                    Some(TokenKind::PipeForward | TokenKind::Compose | TokenKind::Dot)
                ) {
                    while self.pos < i {
                        self.advance();
                    }
                }
            }
            let kind = self.peek_kind();
            let (op, bp) = match kind {
                TokenKind::Plus => (BinOp::Add, (22, 23)),
                TokenKind::Minus => (BinOp::Sub, (22, 23)),
                TokenKind::Star => (BinOp::Mul, (24, 25)),
                TokenKind::Slash => (BinOp::Div, (24, 25)),
                TokenKind::Percent => (BinOp::Mod, (24, 25)),
                TokenKind::StarStar => (BinOp::Pow, (27, 26)), // right-assoc
                TokenKind::Eq => (BinOp::Eq, (14, 15)),
                TokenKind::NotEq => (BinOp::NotEq, (14, 15)),
                TokenKind::Lt => (BinOp::Lt, (14, 15)),
                TokenKind::LtEq => (BinOp::LtEq, (14, 15)),
                TokenKind::Gt => (BinOp::Gt, (14, 15)),
                TokenKind::GtEq => (BinOp::GtEq, (14, 15)),
                TokenKind::In => {
                    if matches!(
                        self.peek_n_kind(1),
                        Some(TokenKind::Newline | TokenKind::Eof | TokenKind::Dedent)
                    ) {
                        break;
                    }
                    (BinOp::In, (14, 15))
                }
                TokenKind::And => (BinOp::And, (12, 13)),
                TokenKind::Or => (BinOp::Or, (10, 11)),
                TokenKind::PlusPlus => (BinOp::Concat, (18, 19)),
                TokenKind::PipeForward => (BinOp::PipeForward, (16, 17)),
                TokenKind::Compose => (BinOp::PipeForward, (16, 17)),
                TokenKind::Step => (BinOp::PipeForward, (16, 17)),
                TokenKind::Ampersand => (BinOp::BitAnd, (14, 15)),
                TokenKind::Caret => (BinOp::BitXor, (14, 15)),
                TokenKind::PlusAssign => (BinOp::Add, (2, 3)),
                TokenKind::MinusAssign => (BinOp::Sub, (2, 3)),
                TokenKind::StarAssign => (BinOp::Mul, (2, 3)),
                TokenKind::SlashAssign => (BinOp::Div, (2, 3)),
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
                TokenKind::LBracket => {
                    if min_bp > 32 {
                        break;
                    }
                    self.advance();
                    if matches!(self.peek_kind(), TokenKind::RBracket) {
                        self.expect(&TokenKind::RBracket)?;
                        let span = lhs.span().merge(self.current().span);
                        lhs = Expr::IndexAccess(
                            Box::new(lhs),
                            Box::new(Expr::IntLit(0, span)),
                            span,
                        );
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
                for (is_expr, text) in segments {
                    if is_expr {
                        // Parse the expression string
                        // We need a fresh lexer/parser for the snippet
                        // Use base offsets from the current span?
                        // For simplicity in v1, we won't perfectly map the span inside the string,
                        // but we could if we tracked offsets in StringInterpLit.
                        // The Lexer change I made doesn't track offsets per segment yet, just strings.
                        // So correct source mapping is a TODO for v2.
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
                        ast_segments.push(StringSegment::Interpolation(Box::new(expr)));
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
                Ok(Expr::UnaryOp(UnaryOp::Neg, Box::new(expr), span))
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
                let mut span = s.merge(expr.span());
                if self.await_block_follows() && self.is_await_orchestration_expr(&expr) {
                    self.skip_newlines();
                    self.consume_block_until_end();
                    span = span.merge(self.current().span);
                }
                Ok(Expr::AwaitExpr(Box::new(expr), span))
            }
            TokenKind::Fn => self.parse_lambda(),
            TokenKind::Parallel => {
                let s = self.advance().span;
                Ok(Expr::Ident("parallel".into(), s))
            }
            TokenKind::Match => {
                let s = self.advance().span;
                let _ = self.parse_expr(0)?;
                self.skip_newlines();
                self.consume_block_until_end();
                Ok(Expr::Ident("match_expr".into(), s))
            }
            TokenKind::If => {
                let s = self.advance().span;
                while !matches!(
                    self.peek_kind(),
                    TokenKind::RBrace | TokenKind::Newline | TokenKind::Comma | TokenKind::Eof
                ) {
                    self.advance();
                }
                if matches!(self.peek_kind(), TokenKind::Newline) {
                    self.skip_newlines();
                    if matches!(self.peek_kind(), TokenKind::Indent) {
                        self.consume_indented_payload();
                        self.skip_newlines();
                    } else {
                        self.consume_rest_of_line();
                        self.skip_newlines();
                    }
                    if matches!(self.peek_kind(), TokenKind::Else) {
                        self.advance();
                        self.skip_newlines();
                        if matches!(self.peek_kind(), TokenKind::Indent) {
                            self.consume_indented_payload();
                            self.skip_newlines();
                        } else {
                            self.consume_rest_of_line();
                            self.skip_newlines();
                        }
                    }
                    if matches!(self.peek_kind(), TokenKind::End) {
                        self.advance();
                    }
                }
                Ok(Expr::Ident("if_expr".into(), s))
            }
            TokenKind::When => {
                let s = self.advance().span;
                self.skip_newlines();
                self.consume_block_until_end();
                Ok(Expr::Ident("when_expr".into(), s))
            }
            TokenKind::Loop => {
                let s = self.advance().span;
                self.skip_newlines();
                self.consume_block_until_end();
                Ok(Expr::Ident("loop_expr".into(), s))
            }
            TokenKind::Let => {
                let s = self.advance().span;
                self.consume_rest_of_line();
                Ok(Expr::Ident("let_expr".into(), s))
            }
            TokenKind::Try => {
                let s = self.advance().span;
                self.consume_rest_of_line();
                Ok(Expr::Ident("try_expr".into(), s))
            }
            TokenKind::Async => {
                let s = self.advance().span;
                if matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Indent) {
                    self.skip_newlines();
                    self.consume_block_until_end();
                }
                Ok(Expr::Ident("async_expr".into(), s))
            }
            TokenKind::Comptime => {
                let s = self.advance().span;
                if matches!(self.peek_kind(), TokenKind::LBrace) {
                    self.consume_balanced_group();
                } else {
                    self.consume_rest_of_line();
                }
                Ok(Expr::Ident("comptime".into(), s))
            }
            TokenKind::Use => {
                let s = self.advance().span;
                Ok(Expr::Ident("use".into(), s))
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
                    while matches!(self.peek_kind(), TokenKind::For) {
                        self.advance();
                        let _ = self.expect_ident()?;
                        self.expect(&TokenKind::In)?;
                        let _ = self.parse_expr(0)?;
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
                        condition,
                        kind: ComprehensionKind::Set,
                        span: s.merge(end),
                    });
                }
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
            while matches!(self.peek_kind(), TokenKind::For) {
                self.advance();
                if matches!(self.peek_kind(), TokenKind::LParen) {
                    self.advance();
                    while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                        self.advance();
                    }
                    if matches!(self.peek_kind(), TokenKind::RParen) {
                        self.advance();
                    }
                } else {
                    let _ = self.expect_ident()?;
                }
                self.expect(&TokenKind::In)?;
                let _ = self.parse_expr(0)?;
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
                let content_expr =
                    self.parse_role_content(&[TokenKind::Comma, TokenKind::RParen], has_indent)?;
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
                    self.pos = save;
                    let expr = self.parse_expr(0)?;
                    args.push(CallArg::Positional(expr));
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
        let mut pairs = Vec::new();
        self.skip_whitespace_tokens();
        while !matches!(self.peek_kind(), TokenKind::RBrace) {
            if !pairs.is_empty() {
                self.expect(&TokenKind::Comma)?;
                self.skip_whitespace_tokens();
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
            if matches!(self.peek_kind(), TokenKind::Colon | TokenKind::Assign) {
                self.advance();
            } else {
                let tok = self.current().clone();
                return Err(ParseError::Unexpected {
                    found: format!("{}", tok.kind),
                    expected: ":".into(),
                    line: tok.span.line,
                    col: tok.span.col,
                });
            }
            self.skip_whitespace_tokens();
            let val = self.parse_expr(0)?;
            if matches!(self.peek_kind(), TokenKind::For) {
                while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                    self.advance();
                }
                pairs.push((key, val));
                break;
            }
            self.skip_whitespace_tokens();
            pairs.push((key, val));
        }
        self.bracket_depth -= 1;
        let end = self.expect(&TokenKind::RBrace)?.span;
        Ok(Expr::MapLit(pairs, start.merge(end)))
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

        let end_span = if has_indent {
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
                    // Interpolation start
                    self.advance(); // consume {

                    // Flush existing text
                    if !text_buf.is_empty() {
                        segments.push(StringSegment::Literal(text_buf.clone()));
                        text_buf.clear();
                    }

                    // Parse expression
                    let expr = self.parse_expr(0)?;
                    self.expect(&TokenKind::RBrace)?;
                    segments.push(StringSegment::Interpolation(Box::new(expr)));
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
        matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Indent | TokenKind::Ident(_))
        )
    }

    fn is_await_orchestration_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Ident(name, _) => matches!(
                name.as_str(),
                "parallel" | "race" | "vote" | "select" | "timeout"
            ),
            Expr::Call(callee, _, _) => {
                matches!(callee.as_ref(), Expr::Ident(name, _) if matches!(name.as_str(), "parallel" | "race" | "vote" | "select" | "timeout"))
            }
            _ => false,
        }
    }

    fn parse_dotted_ident(&mut self) -> Result<String, ParseError> {
        let mut parts = vec![self.expect_ident()?];
        while matches!(self.peek_kind(), TokenKind::Dot) {
            self.advance();
            parts.push(self.expect_ident()?);
        }
        Ok(parts.join("."))
    }
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
}
