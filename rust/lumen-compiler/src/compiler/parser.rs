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
        match self.peek_kind() {
            TokenKind::Record => Ok(Item::Record(self.parse_record()?)),
            TokenKind::Enum => Ok(Item::Enum(self.parse_enum()?)),
            TokenKind::Cell => Ok(Item::Cell(self.parse_cell()?)),
            TokenKind::Use => Ok(Item::UseTool(self.parse_use_tool()?)),
            TokenKind::Grant => Ok(Item::Grant(self.parse_grant()?)),
            _ => {
                let tok = self.current().clone();
                Err(ParseError::Unexpected {
                    found: format!("{}", tok.kind), expected: "record, enum, cell, use, or grant".into(),
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
        Ok(RecordDef { name, fields, span: start.merge(end_span) })
    }

    fn parse_field(&mut self) -> Result<FieldDef, ParseError> {
        let start = self.current().span;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon)?;
        let ty = self.parse_type()?;
        let constraint = if matches!(self.peek_kind(), TokenKind::Where) {
            self.advance();
            Some(self.parse_expr(0)?)
        } else { None };
        let span = start.merge(constraint.as_ref().map(|c| c.span()).unwrap_or(ty.span()));
        Ok(FieldDef { name, ty, constraint, span })
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
        Ok(EnumDef { name, variants, span: start.merge(end_span) })
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
            params.push(Param { name: pname, ty: pty, span: ps });
        }
        self.expect(&TokenKind::RParen)?;
        let ret = if matches!(self.peek_kind(), TokenKind::Arrow) {
            self.advance();
            Some(self.parse_type()?)
        } else { None };
        self.skip_newlines();
        let body = self.parse_block()?;
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(CellDef { name, params, return_type: ret, body, span: start.merge(end_span) })
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
            TokenKind::Ident(_) => {
                // Check if this is an assignment: ident = expr
                if self.is_assignment() {
                    self.parse_assign()
                } else {
                    self.parse_expr_stmt()
                }
            }
            _ => self.parse_expr_stmt(),
        }
    }

    fn parse_let(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect(&TokenKind::Let)?.span;
        let name = self.expect_ident()?;
        let ty = if matches!(self.peek_kind(), TokenKind::Colon) {
            self.advance(); Some(self.parse_type()?)
        } else { None };
        self.expect(&TokenKind::Assign)?;
        let value = self.parse_expr(0)?;
        let span = start.merge(value.span());
        Ok(Stmt::Let(LetStmt { name, ty, value, span }))
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
            TokenKind::Ident(_) => {
                let name = self.expect_ident()?;
                Ok(TypeExpr::Named(name, self.current().span))
            }
            _ => { let tok = self.current().clone();
                Err(ParseError::Unexpected { found: format!("{}", tok.kind), expected: "type".into(), line: tok.span.line, col: tok.span.col })
            }
        }
    }

    // ── Expressions (Pratt parser) ──

    fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_prefix()?;
        loop {
            let (op, bp) = match self.peek_kind() {
                TokenKind::Plus => (BinOp::Add, (10, 11)),
                TokenKind::Minus => (BinOp::Sub, (10, 11)),
                TokenKind::Star => (BinOp::Mul, (12, 13)),
                TokenKind::Slash => (BinOp::Div, (12, 13)),
                TokenKind::Percent => (BinOp::Mod, (12, 13)),
                TokenKind::Eq => (BinOp::Eq, (6, 7)),
                TokenKind::NotEq => (BinOp::NotEq, (6, 7)),
                TokenKind::Lt => (BinOp::Lt, (8, 9)),
                TokenKind::LtEq => (BinOp::LtEq, (8, 9)),
                TokenKind::Gt => (BinOp::Gt, (8, 9)),
                TokenKind::GtEq => (BinOp::GtEq, (8, 9)),
                TokenKind::And => (BinOp::And, (4, 5)),
                TokenKind::Or => (BinOp::Or, (2, 3)),
                // Postfix: dot, index, call
                TokenKind::Dot => {
                    if min_bp > 16 { break; }
                    self.advance();
                    let field = self.expect_ident()?;
                    let span = lhs.span().merge(self.current().span);
                    lhs = Expr::DotAccess(Box::new(lhs), field, span);
                    continue;
                }
                TokenKind::LBracket => {
                    if min_bp > 16 { break; }
                    self.advance();
                    let idx = self.parse_expr(0)?;
                    self.expect(&TokenKind::RBracket)?;
                    let span = lhs.span().merge(self.current().span);
                    lhs = Expr::IndexAccess(Box::new(lhs), Box::new(idx), span);
                    continue;
                }
                TokenKind::LParen => {
                    if min_bp > 16 { break; }
                    lhs = self.parse_call(lhs)?;
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
                let expr = self.parse_expr(14)?; // high bp for unary
                let span = s.merge(expr.span());
                Ok(Expr::UnaryOp(UnaryOp::Neg, Box::new(expr), span))
            }
            TokenKind::Not => {
                let s = self.advance().span;
                let expr = self.parse_expr(14)?;
                let span = s.merge(expr.span());
                Ok(Expr::UnaryOp(UnaryOp::Not, Box::new(expr), span))
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expr(0)?;
                self.expect(&TokenKind::RParen)?;
                Ok(expr)
            }
            TokenKind::LBracket => self.parse_list_lit(),
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
            TokenKind::Ident(_) => {
                let name = self.expect_ident()?;
                let span = self.current().span;
                Ok(Expr::Ident(name, span))
            }
            _ => {
                let tok = self.current().clone();
                Err(ParseError::Unexpected {
                    found: format!("{}", tok.kind), expected: "expression".into(),
                    line: tok.span.line, col: tok.span.col,
                })
            }
        }
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
                
                let content_expr = self.parse_role_content()?;
                
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

    fn parse_list_lit(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect(&TokenKind::LBracket)?.span;
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
        Ok(Expr::ListLit(elems, start.merge(end)))
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
        let start = self.expect(&TokenKind::Role)?.span;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon)?;
        
        let content_expr = self.parse_role_content()?;
        let end_span = self.expect(&TokenKind::End)?.span;
        Ok(Expr::RoleBlock(name, Box::new(content_expr), start.merge(end_span)))
    }

    /// Parse role block content with interpolation support.
    /// Stops at TokenKind::End, TokenKind::Role (peeked), or EOF/Dedent.
    /// Does NOT consume 'end' or 'role', but consumes the content.
    fn parse_role_content(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;
        let mut segments = Vec::new();
        let mut text_buf = String::new();
        let mut has_indent = matches!(self.peek_kind(), TokenKind::Indent);
        if has_indent { self.advance(); }
        self.skip_newlines();

        loop {
            match self.peek_kind() {
                TokenKind::End | TokenKind::Role | TokenKind::Eof | TokenKind::RParen => break,
                TokenKind::Dedent if has_indent => { break; }
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
                    segments.push(StringSegment::Interpolation(Box::new(expr)));
                    
                    // Expect }
                    self.expect(&TokenKind::RBrace)?;
                }
                TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent => {
                    if !text_buf.is_empty() { text_buf.push('\n'); }
                    self.advance();
                }
                _ => {
                    // Accumulate text
                    let tok = self.advance().clone();
                    if !text_buf.is_empty() && !text_buf.ends_with('\n') { text_buf.push(' '); }
                    text_buf.push_str(&format!("{}", tok.kind));
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
