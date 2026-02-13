//! Recursive descent parser with Pratt expression parsing for Lumen.

use crate::compiler::ast::*;
use crate::compiler::tokens::{Token, TokenKind};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("unexpected token {found} at line {line}, col {col}; expected {expected}")]
    Unexpected { found: String, expected: String, line: usize, col: usize },
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
        Self { tokens, pos: 0, bracket_depth: 0 }
    }

    fn current(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or_else(|| self.tokens.last().unwrap())
    }

    fn peek_kind(&self) -> &TokenKind { &self.current().kind }

    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos.min(self.tokens.len() - 1)];
        if self.pos < self.tokens.len() { self.pos += 1; }
        tok
    }

    fn expect(&mut self, kind: &TokenKind) -> Result<Token, ParseError> {
        let tok = self.current().clone();
        if std::mem::discriminant(&tok.kind) == std::mem::discriminant(kind) {
            self.advance();
            Ok(tok)
        } else {
            Err(ParseError::Unexpected {
                found: format!("{}", tok.kind), expected: format!("{}", kind),
                line: tok.span.line, col: tok.span.col,
            })
        }
    }

    fn skip_newlines(&mut self) {
        if self.bracket_depth > 0 {
            self.skip_whitespace_tokens();
        } else {
            while matches!(self.peek_kind(), TokenKind::Newline) { self.advance(); }
        }
    }

    /// Skip newlines, indents, and dedents — used inside bracketed contexts
    fn skip_whitespace_tokens(&mut self) {
        while matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent) {
            self.advance();
        }
    }

    fn at_end(&self) -> bool { matches!(self.peek_kind(), TokenKind::Eof) }

    // ── Top-level parsing ──

    pub fn parse_program(&mut self, directives: Vec<Directive>) -> Result<Program, ParseError> {
        let span_start = self.current().span;
        let mut items = Vec::new();
        self.skip_newlines();
        while !self.at_end() {
            self.skip_newlines();
            if self.at_end() { break; }
            items.push(self.parse_item()?);
            self.skip_newlines();
        }
        let span = if items.is_empty() { span_start } else { span_start.merge(items.last().unwrap().span()) };
        Ok(Program { directives, items, span })
    }

    fn parse_item(&mut self) -> Result<Item, ParseError> {
        // Handle `pub` modifier
        let is_pub = matches!(self.peek_kind(), TokenKind::Pub);
        if is_pub { self.advance(); self.skip_newlines(); }

        // Handle `async` modifier for cells
        let is_async = matches!(self.peek_kind(), TokenKind::Async);
        if is_async { self.advance(); self.skip_newlines(); }

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
            TokenKind::Use => Ok(Item::UseTool(self.parse_use_tool()?)),
            TokenKind::Grant => Ok(Item::Grant(self.parse_grant()?)),
            TokenKind::Type => Ok(Item::TypeAlias(self.parse_type_alias(is_pub)?)),
            TokenKind::Trait => Ok(Item::Trait(self.parse_trait_def(is_pub)?)),
            TokenKind::Impl => Ok(Item::Impl(self.parse_impl_def()?)),
            TokenKind::Import => Ok(Item::Import(self.parse_import(is_pub)?)),
            TokenKind::Const => Ok(Item::ConstDecl(self.parse_const_decl()?)),
            _ => {
                let tok = self.current().clone();
                Err(ParseError::Unexpected {
                    found: format!("{}", tok.kind), expected: "record, enum, cell, use, grant, type, trait, impl, import, or const".into(),
                    line: tok.span.line, col: tok.span.col,
                })
            }
        }
    }

    // ── Record ──

    fn parse_record(&mut self) -> Result<RecordDef, ParseError> {
        let start = self.expect(&TokenKind::Record)?.span;
        let name = self.expect_ident()?;
        self.skip_newlines();
        let mut fields = Vec::new();
        // Fields can be indent-based or just listed until 'end'
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent { self.advance(); }
        self.skip_newlines();
        while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Dedent | TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::End | TokenKind::Dedent | TokenKind::Eof) { break; }
            fields.push(self.parse_field()?);
            self.skip_newlines();
        }
        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) { self.advance(); }
        self.skip_newlines();
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(RecordDef { name, generic_params: vec![], fields, is_pub: false, span: start.merge(end_span) })
    }

    fn parse_field(&mut self) -> Result<FieldDef, ParseError> {
        let start = self.current().span;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon)?;
        let ty = self.parse_type()?;
        let default_value = if matches!(self.peek_kind(), TokenKind::Assign) {
            self.advance();
            Some(self.parse_expr(0)?)
        } else { None };
        let constraint = if matches!(self.peek_kind(), TokenKind::Where) {
            self.advance();
            Some(self.parse_expr(0)?)
        } else { None };
        let span = start.merge(constraint.as_ref().map(|c| c.span())
            .or(default_value.as_ref().map(|d| d.span()))
            .unwrap_or(ty.span()));
        Ok(FieldDef { name, ty, default_value, constraint, span })
    }

    // ── Enum ──

    fn parse_enum(&mut self) -> Result<EnumDef, ParseError> {
        let start = self.expect(&TokenKind::Enum)?.span;
        let name = self.expect_ident()?;
        self.skip_newlines();
        let mut variants = Vec::new();
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent { self.advance(); }
        self.skip_newlines();
        while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Dedent | TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::End | TokenKind::Dedent | TokenKind::Eof) { break; }
            let vs = self.current().span;
            let vname = self.expect_ident()?;
            let payload = if matches!(self.peek_kind(), TokenKind::LParen) {
                self.advance();
                let ty = self.parse_type()?;
                self.expect(&TokenKind::RParen)?;
                Some(ty)
            } else { None };
            variants.push(EnumVariant { name: vname, payload, span: vs });
            self.skip_newlines();
        }
        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) { self.advance(); }
        self.skip_newlines();
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(EnumDef { name, generic_params: vec![], variants, methods: vec![], is_pub: false, span: start.merge(end_span) })
    }

    // ── Cell ──

    fn parse_cell(&mut self) -> Result<CellDef, ParseError> {
        let start = self.expect(&TokenKind::Cell)?.span;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LParen)?;
        let mut params = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RParen) {
            if !params.is_empty() { self.expect(&TokenKind::Comma)?; }
            let ps = self.current().span;
            let pname = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let pty = self.parse_type()?;
            let default_value = if matches!(self.peek_kind(), TokenKind::Assign) {
                self.advance();
                Some(self.parse_expr(0)?)
            } else { None };
            params.push(Param { name: pname, ty: pty, default_value, span: ps });
        }
        self.expect(&TokenKind::RParen)?;
        let ret = if matches!(self.peek_kind(), TokenKind::Arrow) {
            self.advance();
            Some(self.parse_type()?)
        } else { None };
        self.skip_newlines();
        let body = self.parse_block()?;
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(CellDef { name, generic_params: vec![], params, return_type: ret, body, is_pub: false, is_async: false, where_clauses: vec![], span: start.merge(end_span) })
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut stmts = Vec::new();
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent { self.advance(); }
        self.skip_newlines();
        while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Dedent | TokenKind::Eof | TokenKind::Else) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::End | TokenKind::Dedent | TokenKind::Eof | TokenKind::Else) { break; }
            stmts.push(self.parse_stmt()?);
            self.skip_newlines();
        }
        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) { self.advance(); }
        self.skip_newlines();
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
            TokenKind::Ident(_) => {
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
                Ok(Stmt::Expr(ExprStmt { expr: expr.clone(), span: expr.span() }))
            }
            _ => self.parse_expr_stmt(),
        }
    }

    fn parse_let(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::Let)?.span;
        let mutable = if matches!(self.peek_kind(), TokenKind::Mut) {
            self.advance(); true
        } else { false };
        let name = self.expect_ident()?;
        let ty = if matches!(self.peek_kind(), TokenKind::Colon) {
            self.advance(); Some(self.parse_type()?)
        } else { None };
        self.expect(&TokenKind::Assign)?;
        let value = self.parse_expr(0)?;
        let span = start.merge(value.span());
        Ok(Stmt::Let(LetStmt { name, mutable, pattern: None, ty, value, span }))
    }

    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::If)?.span;
        let cond = self.parse_expr(0)?;
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
        } else { None };
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(Stmt::If(IfStmt { condition: cond, then_body, else_body, span: start.merge(end_span) }))
    }

    fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::For)?.span;
        let var = self.expect_ident()?;
        self.expect(&TokenKind::In)?;
        let iter = self.parse_expr(0)?;
        self.skip_newlines();
        let body = self.parse_block()?;
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(Stmt::For(ForStmt { var, iter, body, span: start.merge(end_span) }))
    }

    fn parse_match(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::Match)?.span;
        let subject = self.parse_expr(0)?;
        self.skip_newlines();
        let mut arms = Vec::new();
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent { self.advance(); }
        self.skip_newlines();
        while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Dedent | TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::End | TokenKind::Dedent | TokenKind::Eof) { break; }
            let arm_start = self.current().span;
            let pattern = self.parse_pattern()?;
            self.expect(&TokenKind::Arrow)?;
            // Check for block body (indent after arrow) or single-line
            let body = if matches!(self.peek_kind(), TokenKind::Newline) {
                // Multi-line arm body: newline followed by indent
                self.skip_newlines();
                if matches!(self.peek_kind(), TokenKind::Indent) {
                    self.parse_block()?
                } else {
                    // Just whitespace, parse single statement
                    vec![self.parse_stmt()?]
                }
            } else {
                // Single-line arm: parse one statement
                vec![self.parse_stmt()?]
            };
            let arm_span = arm_start.merge(body.last().map(|s| s.span()).unwrap_or(arm_start));
            arms.push(MatchArm { pattern, body, span: arm_span });
            self.skip_newlines();
        }
        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) { self.advance(); }
        self.skip_newlines();
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(Stmt::Match(MatchStmt { subject, arms, span: start.merge(end_span) }))
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        match self.peek_kind().clone() {
            TokenKind::IntLit(n) => { let s = self.advance().span; Ok(Pattern::Literal(Expr::IntLit(n, s))) }
            TokenKind::FloatLit(n) => { let s = self.advance().span; Ok(Pattern::Literal(Expr::FloatLit(n, s))) }
            TokenKind::StringLit(ref sv) => { let sv = sv.clone(); let s = self.advance().span; Ok(Pattern::Literal(Expr::StringLit(sv, s))) }
            TokenKind::BoolLit(b) => { let s = self.advance().span; Ok(Pattern::Literal(Expr::BoolLit(b, s))) }
            TokenKind::Ident(ref name) if name == "_" => { let s = self.advance().span; Ok(Pattern::Wildcard(s)) }
            TokenKind::Ok_ | TokenKind::Err_ => {
                let vname = format!("{}", self.peek_kind());
                let s = self.advance().span;
                if matches!(self.peek_kind(), TokenKind::LParen) {
                    self.advance();
                    let binding = self.expect_ident()?;
                    self.expect(&TokenKind::RParen)?;
                    Ok(Pattern::Variant(vname, Some(binding), s))
                } else { Ok(Pattern::Variant(vname, None, s)) }
            }
            TokenKind::Ident(ref name) => {
                let name = name.clone();
                let s = self.advance().span;
                if matches!(self.peek_kind(), TokenKind::LParen) {
                    self.advance();
                    let binding = if !matches!(self.peek_kind(), TokenKind::RParen) { Some(self.expect_ident()?) } else { None };
                    self.expect(&TokenKind::RParen)?;
                    Ok(Pattern::Variant(name, binding, s))
                } else { Ok(Pattern::Ident(name, s)) }
            }
            _ => { let tok = self.current().clone();
                Err(ParseError::Unexpected { found: format!("{}", tok.kind), expected: "pattern".into(), line: tok.span.line, col: tok.span.col })
            }
        }
    }

    fn parse_return(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::Return)?.span;
        let value = self.parse_expr(0)?;
        Ok(Stmt::Return(ReturnStmt { value: value.clone(), span: start.merge(value.span()) }))
    }

    fn parse_halt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::Halt)?.span;
        self.expect(&TokenKind::LParen)?;
        let msg = self.parse_expr(0)?;
        self.expect(&TokenKind::RParen)?;
        Ok(Stmt::Halt(HaltStmt { message: msg.clone(), span: start.merge(msg.span()) }))
    }

    fn parse_while(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::While)?.span;
        let cond = self.parse_expr(0)?;
        self.skip_newlines();
        let body = self.parse_block()?;
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(Stmt::While(WhileStmt { condition: cond, body, span: start.merge(end_span) }))
    }

    fn parse_loop(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::Loop)?.span;
        self.skip_newlines();
        let body = self.parse_block()?;
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(Stmt::Loop(LoopStmt { body, span: start.merge(end_span) }))
    }

    fn parse_break(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::Break)?.span;
        let value = if !matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Eof | TokenKind::End | TokenKind::Dedent) {
            Some(self.parse_expr(0)?)
        } else { None };
        let span = value.as_ref().map(|v| start.merge(v.span())).unwrap_or(start);
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
        if matches!(self.peek_kind(), TokenKind::Ident(_)) {
            if self.pos + 1 < self.tokens.len() {
                return matches!(self.tokens[self.pos + 1].kind,
                    TokenKind::PlusAssign | TokenKind::MinusAssign | TokenKind::StarAssign | TokenKind::SlashAssign);
            }
        }
        false
    }

    fn parse_compound_assign(&mut self) -> Result<Stmt, ParseError> {
        let start = self.tokens[self.pos].span;
        let name = self.expect_ident()?;
        let op = match self.peek_kind() {
            TokenKind::PlusAssign => { self.advance(); CompoundOp::AddAssign }
            TokenKind::MinusAssign => { self.advance(); CompoundOp::SubAssign }
            TokenKind::StarAssign => { self.advance(); CompoundOp::MulAssign }
            TokenKind::SlashAssign => { self.advance(); CompoundOp::DivAssign }
            _ => unreachable!(),
        };
        let value = self.parse_expr(0)?;
        let span = start.merge(value.span());
        Ok(Stmt::CompoundAssign(CompoundAssignStmt { target: name, op, value, span }))
    }

    // ── New item parsers ──

    fn parse_type_alias(&mut self, is_pub: bool) -> Result<TypeAliasDef, ParseError> {
        let start = self.expect(&TokenKind::Type)?.span;
        let name = self.expect_ident()?;
        let generic_params = self.parse_optional_generic_params()?;
        self.expect(&TokenKind::Assign)?;
        let type_expr = self.parse_type()?;
        let span = start.merge(type_expr.span());
        Ok(TypeAliasDef { name, generic_params, type_expr, is_pub, span })
    }

    fn parse_trait_def(&mut self, is_pub: bool) -> Result<TraitDef, ParseError> {
        let start = self.expect(&TokenKind::Trait)?.span;
        let name = self.expect_ident()?;
        let parent_traits = if matches!(self.peek_kind(), TokenKind::Colon) {
            self.advance();
            let mut traits = vec![self.expect_ident()?];
            while matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance();
                traits.push(self.expect_ident()?);
            }
            traits
        } else { vec![] };
        self.skip_newlines();
        let mut methods = Vec::new();
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent { self.advance(); }
        self.skip_newlines();
        while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Dedent | TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::End | TokenKind::Dedent | TokenKind::Eof) { break; }
            methods.push(self.parse_cell()?);
            self.skip_newlines();
        }
        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) { self.advance(); }
        self.skip_newlines();
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(TraitDef { name, parent_traits, methods, is_pub, span: start.merge(end_span) })
    }

    fn parse_impl_def(&mut self) -> Result<ImplDef, ParseError> {
        let start = self.expect(&TokenKind::Impl)?.span;
        let generic_params = self.parse_optional_generic_params()?;
        let trait_name = self.expect_ident()?;
        self.expect(&TokenKind::For)?;
        let target_type = self.expect_ident()?;
        self.skip_newlines();
        let mut cells = Vec::new();
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent { self.advance(); }
        self.skip_newlines();
        while !matches!(self.peek_kind(), TokenKind::End | TokenKind::Dedent | TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::End | TokenKind::Dedent | TokenKind::Eof) { break; }
            cells.push(self.parse_cell()?);
            self.skip_newlines();
        }
        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) { self.advance(); }
        self.skip_newlines();
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(ImplDef { trait_name, generic_params, target_type, cells, span: start.merge(end_span) })
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
                } else { None };
                names.push(ImportName { name: n, alias, span: ns });
                if !matches!(self.peek_kind(), TokenKind::Comma) { break; }
                self.advance();
            }
            ImportList::Names(names)
        };
        let span = start.merge(self.current().span);
        Ok(ImportDecl { path, names, is_pub, span })
    }

    fn parse_const_decl(&mut self) -> Result<ConstDeclDef, ParseError> {
        let start = self.expect(&TokenKind::Const)?.span;
        let name = self.expect_ident()?;
        let type_ann = if matches!(self.peek_kind(), TokenKind::Colon) {
            self.advance(); Some(self.parse_type()?)
        } else { None };
        self.expect(&TokenKind::Assign)?;
        let value = self.parse_expr(0)?;
        let span = start.merge(value.span());
        Ok(ConstDeclDef { name, type_ann, value, span })
    }

    fn parse_optional_generic_params(&mut self) -> Result<Vec<GenericParam>, ParseError> {
        if !matches!(self.peek_kind(), TokenKind::LBracket) { return Ok(vec![]); }
        self.advance();
        let mut params = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RBracket) {
            if !params.is_empty() { self.expect(&TokenKind::Comma)?; }
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
            } else { vec![] };
            params.push(GenericParam { name, bounds, span: ps });
        }
        self.expect(&TokenKind::RBracket)?;
        Ok(params)
    }

    fn parse_expr_stmt(&mut self) -> Result<Stmt, ParseError> {
        let expr = self.parse_expr(0)?;
        let span = expr.span();
        Ok(Stmt::Expr(ExprStmt { expr, span }))
    }

    /// Check if the current position is an assignment (ident followed by =)
    fn is_assignment(&self) -> bool {
        if matches!(self.peek_kind(), TokenKind::Ident(_)) {
            if self.pos + 1 < self.tokens.len() {
                return matches!(self.tokens[self.pos + 1].kind, TokenKind::Assign);
            }
        }
        false
    }

    /// Parse an assignment statement: ident = expr
    fn parse_assign(&mut self) -> Result<Stmt, ParseError> {
        let start = self.tokens[self.pos].span;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Assign)?;
        let value = self.parse_expr(0)?;
        let span = start.merge(value.span());
        Ok(Stmt::Assign(AssignStmt { target: name, value, span }))
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
        } else { None };
        let tool_path = if mcp_url.is_none() { self.parse_dotted_ident()? } else { String::new() };
        self.expect(&TokenKind::As)?;
        let alias = self.expect_ident()?;
        Ok(UseToolDecl { tool_path, alias, mcp_url, span: start })
    }

    fn parse_grant(&mut self) -> Result<GrantDecl, ParseError> {
        let start = self.expect(&TokenKind::Grant)?.span;
        let alias = self.expect_ident()?;
        let mut constraints = Vec::new();
        self.skip_newlines();
        // Parse constraints: key value pairs on same line or indented
        let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent { self.advance(); self.skip_newlines(); }
        while matches!(self.peek_kind(), TokenKind::Ident(_)) {
            let cs = self.current().span;
            let key = self.expect_ident()?;
            let value = self.parse_expr(0)?;
            constraints.push(GrantConstraint { key, value, span: cs });
            self.skip_newlines();
            if !has_indent { break; } // single-line grants
        }
        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) { self.advance(); }
        Ok(GrantDecl { tool_alias: alias, constraints, span: start })
    }

    // ── Types ──

    fn parse_type(&mut self) -> Result<TypeExpr, ParseError> {
        let base = self.parse_base_type()?;
        // Check for union: T | U
        if matches!(self.peek_kind(), TokenKind::Pipe) {
            let mut types = vec![base];
            while matches!(self.peek_kind(), TokenKind::Pipe) {
                self.advance();
                types.push(self.parse_base_type()?);
            }
            let span = types.first().unwrap().span().merge(types.last().unwrap().span());
            Ok(TypeExpr::Union(types, span))
        } else { Ok(base) }
    }

    fn parse_base_type(&mut self) -> Result<TypeExpr, ParseError> {
        match self.peek_kind().clone() {
            TokenKind::Null => { let s = self.advance().span; Ok(TypeExpr::Null(s)) }
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
                Ok(TypeExpr::Fn(params, Box::new(ret), s))
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
            TokenKind::String_ => { let s = self.advance().span; Ok(TypeExpr::Named("String".to_string(), s)) }
            TokenKind::Int_ => { let s = self.advance().span; Ok(TypeExpr::Named("Int".to_string(), s)) }
            TokenKind::Float_ => { let s = self.advance().span; Ok(TypeExpr::Named("Float".to_string(), s)) }
            TokenKind::Bool => { let s = self.advance().span; Ok(TypeExpr::Named("Bool".to_string(), s)) }
            TokenKind::Bytes => { let s = self.advance().span; Ok(TypeExpr::Named("Bytes".to_string(), s)) }
            TokenKind::Json => { let s = self.advance().span; Ok(TypeExpr::Named("Json".to_string(), s)) }
            _ => { let tok = self.current().clone();
                Err(ParseError::Unexpected { found: format!("{}", tok.kind), expected: "type".into(), line: tok.span.line, col: tok.span.col })
            }
        }
    }

    // ── Expressions (Pratt parser) ──

    fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_prefix()?;
        loop {
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
                TokenKind::In => (BinOp::In, (14, 15)),
                TokenKind::And => (BinOp::And, (12, 13)),
                TokenKind::Or => (BinOp::Or, (10, 11)),
                TokenKind::PlusPlus => (BinOp::Concat, (18, 19)),
                TokenKind::PipeForward => (BinOp::PipeForward, (16, 17)),
                TokenKind::Compose => (BinOp::PipeForward, (16, 17)),
                TokenKind::Ampersand => (BinOp::BitAnd, (14, 15)),
                TokenKind::Caret => (BinOp::BitXor, (14, 15)),
                // Null coalescing
                TokenKind::QuestionQuestion => {
                    if min_bp > 8 { break; }
                    self.advance();
                    let rhs = self.parse_expr(9)?;
                    let span = lhs.span().merge(rhs.span());
                    lhs = Expr::NullCoalesce(Box::new(lhs), Box::new(rhs), span);
                    continue;
                }
                // Range operators
                TokenKind::DotDot => {
                    if min_bp > 20 { break; }
                    self.advance();
                    let rhs = if matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Eof | TokenKind::RBracket | TokenKind::RParen | TokenKind::Comma) {
                        None
                    } else {
                        Some(Box::new(self.parse_expr(21)?))
                    };
                    let span = lhs.span().merge(rhs.as_ref().map(|r| r.span()).unwrap_or(lhs.span()));
                    lhs = Expr::RangeExpr { start: Some(Box::new(lhs)), end: rhs, inclusive: false, step: None, span };
                    continue;
                }
                TokenKind::DotDotEq => {
                    if min_bp > 20 { break; }
                    self.advance();
                    let rhs = self.parse_expr(21)?;
                    let span = lhs.span().merge(rhs.span());
                    lhs = Expr::RangeExpr { start: Some(Box::new(lhs)), end: Some(Box::new(rhs)), inclusive: true, step: None, span };
                    continue;
                }
                // Postfix: dot, index, call, ?, !, ?.
                TokenKind::Dot => {
                    if min_bp > 32 { break; }
                    self.advance();
                    let field = self.expect_ident()?;
                    let span = lhs.span().merge(self.current().span);
                    lhs = Expr::DotAccess(Box::new(lhs), field, span);
                    continue;
                }
                TokenKind::QuestionDot => {
                    if min_bp > 32 { break; }
                    self.advance();
                    let field = self.expect_ident()?;
                    let span = lhs.span().merge(self.current().span);
                    lhs = Expr::NullSafeAccess(Box::new(lhs), field, span);
                    continue;
                }
                TokenKind::LBracket => {
                    if min_bp > 32 { break; }
                    self.advance();
                    let idx = self.parse_expr(0)?;
                    self.expect(&TokenKind::RBracket)?;
                    let span = lhs.span().merge(self.current().span);
                    lhs = Expr::IndexAccess(Box::new(lhs), Box::new(idx), span);
                    continue;
                }
                TokenKind::LParen => {
                    if min_bp > 32 { break; }
                    lhs = self.parse_call(lhs)?;
                    continue;
                }
                TokenKind::Question => {
                    if min_bp > 32 { break; }
                    let span = lhs.span().merge(self.advance().span);
                    lhs = Expr::TryExpr(Box::new(lhs), span);
                    continue;
                }
                TokenKind::Bang => {
                    if min_bp > 32 { break; }
                    let span = lhs.span().merge(self.advance().span);
                    lhs = Expr::NullAssert(Box::new(lhs), span);
                    continue;
                }
                TokenKind::Expect => {
                    if min_bp > 1 { break; }
                    self.advance();
                    self.expect(&TokenKind::Schema)?;
                    let schema_name = self.expect_ident()?;
                    let span = lhs.span().merge(self.current().span);
                    lhs = Expr::ExpectSchema(Box::new(lhs), schema_name, span);
                    continue;
                }
                _ => break,
            };
            let (l_bp, r_bp) = bp;
            if l_bp < min_bp { break; }
            self.advance();
            let rhs = self.parse_expr(r_bp)?;
            let span = lhs.span().merge(rhs.span());
            lhs = Expr::BinOp(Box::new(lhs), op, Box::new(rhs), span);
        }
        Ok(lhs)
    }

    fn parse_prefix(&mut self) -> Result<Expr, ParseError> {
        match self.peek_kind().clone() {
            TokenKind::IntLit(n) => { let s = self.advance().span; Ok(Expr::IntLit(n, s)) }
            TokenKind::FloatLit(n) => { let s = self.advance().span; Ok(Expr::FloatLit(n, s)) }
            TokenKind::StringLit(ref sv) => { let sv = sv.clone(); let s = self.advance().span; Ok(Expr::StringLit(sv, s)) }
            TokenKind::RawStringLit(ref sv) => { let sv = sv.clone(); let s = self.advance().span; Ok(Expr::RawStringLit(sv, s)) }
            TokenKind::BytesLit(ref bv) => { let bv = bv.clone(); let s = self.advance().span; Ok(Expr::BytesLit(bv, s)) }
            TokenKind::NullLit => { let s = self.advance().span; Ok(Expr::NullLit(s)) }
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
                        let mut lexer = crate::compiler::lexer::Lexer::new(&text, span.line, span.col);
                        let tokens = lexer.tokenize().map_err(|e| ParseError::Unexpected {
                            found: format!("lexer error: {}", e), expected: "expression".into(),
                            line: span.line, col: span.col 
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
            TokenKind::BoolLit(b) => { let s = self.advance().span; Ok(Expr::BoolLit(b, s)) }
            TokenKind::Null => { let s = self.advance().span; Ok(Expr::NullLit(s)) }
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
                let expr = self.parse_expr(0)?;
                let span = s.merge(expr.span());
                Ok(Expr::SpreadExpr(Box::new(expr), span))
            }
            TokenKind::Await => {
                let s = self.advance().span;
                let expr = self.parse_expr(0)?;
                let span = s.merge(expr.span());
                Ok(Expr::AwaitExpr(Box::new(expr), span))
            }
            TokenKind::Fn => self.parse_lambda(),
            TokenKind::Set => {
                let s = self.advance().span;
                self.expect(&TokenKind::LBracket)?;
                self.bracket_depth += 1;
                let mut elems = Vec::new();
                self.skip_whitespace_tokens();
                while !matches!(self.peek_kind(), TokenKind::RBracket) {
                    if !elems.is_empty() {
                        self.expect(&TokenKind::Comma)?;
                        self.skip_whitespace_tokens();
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
                        if matches!(self.peek_kind(), TokenKind::RParen) { break; }
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
                let name = self.expect_ident()?;
                let span = self.current().span;
                Ok(Expr::Ident(name, span))
            }
            // Type keywords used as function names in expression position
            TokenKind::String_ => { let s = self.advance().span; Ok(Expr::Ident("string".into(), s)) }
            TokenKind::Int_ => { let s = self.advance().span; Ok(Expr::Ident("int".into(), s)) }
            TokenKind::Float_ => { let s = self.advance().span; Ok(Expr::Ident("float".into(), s)) }
            TokenKind::Bool => { let s = self.advance().span; Ok(Expr::Ident("bool".into(), s)) }
            TokenKind::Bytes => { let s = self.advance().span; Ok(Expr::Ident("bytes".into(), s)) }
            TokenKind::Json => { let s = self.advance().span; Ok(Expr::Ident("json".into(), s)) }
            TokenKind::List => { let s = self.advance().span; Ok(Expr::Ident("list".into(), s)) }
            TokenKind::Map => { let s = self.advance().span; Ok(Expr::Ident("map".into(), s)) }
            TokenKind::Type => { let s = self.advance().span; Ok(Expr::Ident("type".into(), s)) }
            _ => {
                let tok = self.current().clone();
                Err(ParseError::Unexpected {
                    found: format!("{}", tok.kind), expected: "expression".into(),
                    line: tok.span.line, col: tok.span.col,
                })
            }
        }
    }

    fn parse_lambda(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect(&TokenKind::Fn)?.span;
        self.expect(&TokenKind::LParen)?;
        let mut params = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RParen) {
            if !params.is_empty() { self.expect(&TokenKind::Comma)?; }
            let ps = self.current().span;
            let pname = self.expect_ident()?;
            let pty = if matches!(self.peek_kind(), TokenKind::Colon) {
                self.advance();
                self.parse_type()?
            } else {
                TypeExpr::Named("Any".into(), ps)
            };
            params.push(Param { name: pname, ty: pty, default_value: None, span: ps });
        }
        self.expect(&TokenKind::RParen)?;
        let return_type = if matches!(self.peek_kind(), TokenKind::Arrow) {
            self.advance();
            Some(Box::new(self.parse_type()?))
        } else { None };
        let body = if matches!(self.peek_kind(), TokenKind::FatArrow) {
            self.advance();
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
        Ok(Expr::Lambda { params, return_type, body, span: start.merge(end_span) })
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
            let var = self.expect_ident()?;
            self.expect(&TokenKind::In)?;
            let iter = self.parse_expr(0)?;
            self.skip_whitespace_tokens();
            let condition = if matches!(self.peek_kind(), TokenKind::If) {
                self.advance();
                Some(Box::new(self.parse_expr(0)?))
            } else { None };
            self.skip_whitespace_tokens();
            self.bracket_depth -= 1;
            let end = self.expect(&TokenKind::RBracket)?.span;
            return Ok(Expr::Comprehension {
                body: Box::new(first), var, iter: Box::new(iter),
                condition, kind: ComprehensionKind::List, span: start.merge(end),
            });
        }
        // Regular list
        let mut elems = vec![first];
        while matches!(self.peek_kind(), TokenKind::Comma) {
            self.advance();
            self.skip_whitespace_tokens();
            if matches!(self.peek_kind(), TokenKind::RBracket) { break; }
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
        self.skip_whitespace_tokens();
        while !matches!(self.peek_kind(), TokenKind::RParen) {
            if !args.is_empty() {
                self.expect(&TokenKind::Comma)?;
                self.skip_whitespace_tokens();
            }
            // Check for role blocks inline
            if matches!(self.peek_kind(), TokenKind::Role) {
                self.advance();
                let role_span = self.current().span;
                let role_name = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                
                let has_indent = matches!(self.peek_kind(), TokenKind::Indent);
                let content_expr = self.parse_role_content(&[TokenKind::Comma, TokenKind::RParen], has_indent)?;
                
                let span = role_span.merge(content_expr.span());
                args.push(CallArg::Role(role_name, content_expr, span));
                continue;
            }
            if let TokenKind::Ident(ref name) = self.peek_kind().clone() {
                // Check if named arg: name: expr
                let name_clone = name.clone();
                let save = self.pos;
                self.advance();
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

        // Convert Call with all-named-args + Ident callee to RecordLit
        if let Expr::Ident(ref name, _) = callee {
            if !args.is_empty() && args.iter().all(|a| matches!(a, CallArg::Named(..))) {
                // Check if name starts with uppercase (record constructor convention)
                if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                    let fields: Vec<(String, Expr)> = args.into_iter().map(|a| {
                        if let CallArg::Named(n, e, _) = a { (n, e) } else { unreachable!() }
                    }).collect();
                    return Ok(Expr::RecordLit(name.clone(), fields, start.merge(end)));
                }
            }
        }

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
            let key = self.parse_expr(0)?;
            self.expect(&TokenKind::Colon)?;
            self.skip_whitespace_tokens();
            let val = self.parse_expr(0)?;
            self.skip_whitespace_tokens();
            pairs.push((key, val));
        }
        self.bracket_depth -= 1;
        let end = self.expect(&TokenKind::RBrace)?.span;
        Ok(Expr::MapLit(pairs, start.merge(end)))
    }

    fn parse_role_block_expr(&mut self) -> Result<Expr, ParseError> {
        // Expression context (default): stops at RParen to support call args
        self.parse_role_block_general(&[TokenKind::End, TokenKind::Role, TokenKind::Eof, TokenKind::RParen])
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
        
        Ok(Expr::RoleBlock(name, Box::new(content_expr), start.merge(end_span)))
    }

    /// Parse role block content with interpolation support.
    /// Stops at TokenKind::End, TokenKind::Role (peeked), or EOF/Dedent.
    /// Does NOT consume 'end' or 'role', but consumes the content.
    fn parse_role_content(&mut self, terminators: &[TokenKind], has_indent: bool) -> Result<Expr, ParseError> {
        let start = self.current().span;
        let mut segments = Vec::new();
        let mut text_buf = String::new();
        if has_indent { self.advance(); }
        self.skip_newlines();

        loop {
            let peek = self.peek_kind();
            if terminators.contains(peek) { break; }
            if matches!(peek, TokenKind::Newline) && !has_indent { break; }
            if matches!(peek, TokenKind::Dedent) && has_indent { break; }
            
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
                    if !text_buf.is_empty() && !text_buf.ends_with('\n') { text_buf.push(' '); }
                    text_buf.push_str(&text);
                }
            }
        }
        
        if !text_buf.is_empty() {
            segments.push(StringSegment::Literal(text_buf));
        }

        if has_indent && matches!(self.peek_kind(), TokenKind::Dedent) { self.advance(); }
        
        // If we stopped at End, consume it? 
        // parse_role_block_expr expects to consume 'End'.
        // parse_call expects to NOT consume 'Role' (next block).
        // So this helper should just parse content.
        
        let span = if segments.is_empty() { start } else { start.merge(self.current().span) };
        
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
            TokenKind::Ident(name) => { let n = name.clone(); self.advance(); Ok(n) }
            // Allow some keywords as identifiers in certain contexts
            _ => Err(ParseError::Unexpected {
                found: format!("{}", tok.kind), expected: "identifier".into(),
                line: tok.span.line, col: tok.span.col,
            })
        }
    }

    fn expect_string(&mut self) -> Result<String, ParseError> {
        let tok = self.current().clone();
        match &tok.kind {
            TokenKind::StringLit(s) => { let s = s.clone(); self.advance(); Ok(s) }
            _ => Err(ParseError::Unexpected {
                found: format!("{}", tok.kind), expected: "string literal".into(),
                line: tok.span.line, col: tok.span.col,
            })
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
        } else { panic!("expected record"); }
    }

    #[test]
    fn test_parse_cell() {
        let prog = parse_src("cell add(a: Int, b: Int) -> Int\n  return a + b\nend").unwrap();
        assert_eq!(prog.items.len(), 1);
        if let Item::Cell(c) = &prog.items[0] {
            assert_eq!(c.name, "add");
            assert_eq!(c.params.len(), 2);
        } else { panic!("expected cell"); }
    }

    #[test]
    fn test_parse_enum() {
        let prog = parse_src("enum Color\n  Red\n  Green\n  Blue\nend").unwrap();
        if let Item::Enum(e) = &prog.items[0] {
            assert_eq!(e.name, "Color");
            assert_eq!(e.variants.len(), 3);
        } else { panic!("expected enum"); }
    }

    #[test]
    fn test_parse_match() {
        let src = "cell test(x: Int) -> String\n  match x\n    1 -> return \"one\"\n    _ -> return \"other\"\n  end\nend";
        let prog = parse_src(src).unwrap();
        assert_eq!(prog.items.len(), 1);
    }
}
