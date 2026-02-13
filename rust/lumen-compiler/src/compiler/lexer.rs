//! Indentation-aware lexer for Lumen source code.

use crate::compiler::tokens::{Span, Token, TokenKind};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LexError {
    #[error("unexpected character '{ch}' at line {line}, col {col}")]
    UnexpectedChar { ch: char, line: usize, col: usize },
    #[error("unterminated string at line {line}, col {col}")]
    UnterminatedString { line: usize, col: usize },
    #[error("inconsistent indentation at line {line}")]
    InconsistentIndent { line: usize },
    #[error("invalid number at line {line}, col {col}")]
    InvalidNumber { line: usize, col: usize },
}

pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
    byte_offset: usize,
    base_line: usize,
    base_offset: usize,
    indent_stack: Vec<usize>,
    pending: Vec<Token>,
    at_line_start: bool,
}

impl Lexer {
    pub fn new(source: &str, base_line: usize, base_offset: usize) -> Self {
        Self {
            source: source.chars().collect(),
            pos: 0, line: 1, col: 1, byte_offset: 0,
            base_line, base_offset,
            indent_stack: vec![0],
            pending: Vec::new(),
            at_line_start: true,
        }
    }

    fn current(&self) -> Option<char> { self.source.get(self.pos).copied() }
    fn peek(&self) -> Option<char> { self.source.get(self.pos + 1).copied() }

    fn advance(&mut self) -> Option<char> {
        let ch = self.source.get(self.pos).copied()?;
        self.pos += 1;
        self.byte_offset += ch.len_utf8();
        if ch == '\n' { self.line += 1; self.col = 1; self.at_line_start = true; }
        else { self.col += 1; }
        Some(ch)
    }

    fn span_here(&self) -> Span {
        Span::new(self.base_offset + self.byte_offset, self.base_offset + self.byte_offset, self.base_line + self.line - 1, self.col)
    }

    fn span_from(&self, so: usize, sl: usize, sc: usize) -> Span {
        Span::new(self.base_offset + so, self.base_offset + self.byte_offset, self.base_line + sl - 1, sc)
    }

    fn handle_indentation(&mut self) -> Result<(), LexError> {
        let mut indent = 0;
        while let Some(ch) = self.current() {
            match ch {
                ' ' => { indent += 1; self.advance(); }
                '\t' => { indent += 2; self.advance(); }
                _ => break,
            }
        }
        if matches!(self.current(), None | Some('\n') | Some('#')) {
            if self.current().is_none() {
                while self.indent_stack.len() > 1 {
                    self.indent_stack.pop();
                    self.pending.push(Token::new(TokenKind::Dedent, self.span_here()));
                }
            }
            return Ok(());
        }
        let cur = *self.indent_stack.last().unwrap();
        if indent > cur {
            self.indent_stack.push(indent);
            self.pending.push(Token::new(TokenKind::Indent, self.span_here()));
        } else if indent < cur {
            while let Some(&top) = self.indent_stack.last() {
                if top > indent { self.indent_stack.pop(); self.pending.push(Token::new(TokenKind::Dedent, self.span_here())); }
                else { break; }
            }
            if *self.indent_stack.last().unwrap() != indent {
                return Err(LexError::InconsistentIndent { line: self.base_line + self.line - 1 });
            }
        }
        Ok(())
    }

    fn read_string(&mut self) -> Result<Token, LexError> {
        let (so, sl, sc) = (self.byte_offset, self.line, self.col);
        self.advance(); // opening quote
        let mut s = String::new();
        loop {
            match self.current() {
                None | Some('\n') => return Err(LexError::UnterminatedString { line: self.base_line + sl - 1, col: sc }),
                Some('\\') => {
                    self.advance();
                    match self.current() {
                        Some('n') => { s.push('\n'); self.advance(); }
                        Some('t') => { s.push('\t'); self.advance(); }
                        Some('\\') => { s.push('\\'); self.advance(); }
                        Some('"') => { s.push('"'); self.advance(); }
                        Some(c) => { s.push('\\'); s.push(c); self.advance(); }
                        None => return Err(LexError::UnterminatedString { line: self.base_line + sl - 1, col: sc }),
                    }
                }
                Some('"') => { self.advance(); break; }
                Some(c) => { s.push(c); self.advance(); }
            }
        }
        Ok(Token::new(TokenKind::StringLit(s), self.span_from(so, sl, sc)))
    }

    fn read_number(&mut self) -> Result<Token, LexError> {
        let (so, sl, sc) = (self.byte_offset, self.line, self.col);
        let mut ns = String::new();
        let mut is_float = false;
        while let Some(ch) = self.current() {
            if ch.is_ascii_digit() { ns.push(ch); self.advance(); }
            else if ch == '.' && !is_float && matches!(self.peek(), Some(d) if d.is_ascii_digit()) {
                is_float = true; ns.push(ch); self.advance();
            }
            else if ch == '_' { self.advance(); }
            else { break; }
        }
        let span = self.span_from(so, sl, sc);
        if is_float {
            ns.parse::<f64>().map(|f| Token::new(TokenKind::FloatLit(f), span))
                .map_err(|_| LexError::InvalidNumber { line: self.base_line + sl - 1, col: sc })
        } else {
            ns.parse::<i64>().map(|n| Token::new(TokenKind::IntLit(n), span))
                .map_err(|_| LexError::InvalidNumber { line: self.base_line + sl - 1, col: sc })
        }
    }

    fn read_ident(&mut self) -> Token {
        let (so, sl, sc) = (self.byte_offset, self.line, self.col);
        let mut id = String::new();
        while let Some(ch) = self.current() {
            if ch.is_alphanumeric() || ch == '_' { id.push(ch); self.advance(); } else { break; }
        }
        let span = self.span_from(so, sl, sc);
        let kind = match id.as_str() {
            "record" => TokenKind::Record, "enum" => TokenKind::Enum, "cell" => TokenKind::Cell,
            "let" => TokenKind::Let, "if" => TokenKind::If, "else" => TokenKind::Else,
            "for" => TokenKind::For, "in" => TokenKind::In, "match" => TokenKind::Match,
            "return" => TokenKind::Return, "halt" => TokenKind::Halt, "end" => TokenKind::End,
            "use" => TokenKind::Use, "tool" => TokenKind::Tool, "as" => TokenKind::As,
            "grant" => TokenKind::Grant, "expect" => TokenKind::Expect, "schema" => TokenKind::Schema,
            "role" => TokenKind::Role, "where" => TokenKind::Where, "and" => TokenKind::And,
            "or" => TokenKind::Or, "not" => TokenKind::Not, "Null" => TokenKind::Null,
            "result" => TokenKind::Result, "ok" => TokenKind::Ok_, "err" => TokenKind::Err_,
            "list" => TokenKind::List, "map" => TokenKind::Map,
            "true" => TokenKind::BoolLit(true), "false" => TokenKind::BoolLit(false),
            _ => TokenKind::Ident(id),
        };
        Token::new(kind, span)
    }

    fn two_char(&mut self, second: char, matched: TokenKind, single: TokenKind) -> Token {
        let (so, sl, sc) = (self.byte_offset, self.line, self.col);
        self.advance();
        if self.current() == Some(second) { self.advance(); Token::new(matched, self.span_from(so, sl, sc)) }
        else { Token::new(single, self.span_from(so, sl, sc)) }
    }

    fn single(&mut self, kind: TokenKind) -> Token {
        let span = self.span_here(); self.advance(); Token::new(kind, span)
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        while self.pos < self.source.len() {
            if self.at_line_start {
                self.at_line_start = false;
                self.handle_indentation()?;
                tokens.append(&mut self.pending);
            }
            let ch = match self.current() { Some(c) => c, None => break };
            match ch {
                '\n' => {
                    let span = self.span_here(); self.advance();
                    if !matches!(tokens.last().map(|t| &t.kind), Some(TokenKind::Newline) | Some(TokenKind::Indent) | None) {
                        tokens.push(Token::new(TokenKind::Newline, span));
                    }
                }
                ' ' | '\t' | '\r' => { while matches!(self.current(), Some(' ' | '\t' | '\r')) { self.advance(); } }
                '#' => { while matches!(self.current(), Some(c) if c != '\n') { self.advance(); } }
                '"' => tokens.push(self.read_string()?),
                '0'..='9' => tokens.push(self.read_number()?),
                'a'..='z' | 'A'..='Z' | '_' => tokens.push(self.read_ident()),
                '+' => tokens.push(self.single(TokenKind::Plus)),
                '-' => { let (so, sl, sc) = (self.byte_offset, self.line, self.col); self.advance();
                    if self.current() == Some('>') { self.advance(); tokens.push(Token::new(TokenKind::Arrow, self.span_from(so, sl, sc))); }
                    else { tokens.push(Token::new(TokenKind::Minus, self.span_from(so, sl, sc))); }
                }
                '*' => tokens.push(self.single(TokenKind::Star)),
                '/' => tokens.push(self.single(TokenKind::Slash)),
                '%' => tokens.push(self.single(TokenKind::Percent)),
                '=' => tokens.push(self.two_char('=', TokenKind::Eq, TokenKind::Assign)),
                '!' => { let (so, sl, sc) = (self.byte_offset, self.line, self.col); self.advance();
                    if self.current() == Some('=') { self.advance(); tokens.push(Token::new(TokenKind::NotEq, self.span_from(so, sl, sc))); }
                    else { return Err(LexError::UnexpectedChar { ch: '!', line: self.base_line + sl - 1, col: sc }); }
                }
                '<' => tokens.push(self.two_char('=', TokenKind::LtEq, TokenKind::Lt)),
                '>' => tokens.push(self.two_char('=', TokenKind::GtEq, TokenKind::Gt)),
                '.' => tokens.push(self.single(TokenKind::Dot)),
                ',' => tokens.push(self.single(TokenKind::Comma)),
                ':' => tokens.push(self.single(TokenKind::Colon)),
                '|' => tokens.push(self.single(TokenKind::Pipe)),
                '(' => tokens.push(self.single(TokenKind::LParen)),
                ')' => tokens.push(self.single(TokenKind::RParen)),
                '[' => tokens.push(self.single(TokenKind::LBracket)),
                ']' => tokens.push(self.single(TokenKind::RBracket)),
                '{' => tokens.push(self.single(TokenKind::LBrace)),
                '}' => tokens.push(self.single(TokenKind::RBrace)),
                _ => return Err(LexError::UnexpectedChar { ch, line: self.base_line + self.line - 1, col: self.col }),
            }
        }
        while self.indent_stack.len() > 1 { self.indent_stack.pop(); tokens.push(Token::new(TokenKind::Dedent, self.span_here())); }
        tokens.push(Token::new(TokenKind::Eof, self.span_here()));
        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lex_cell() {
        let src = "cell main() -> Int\n  return 42\nend";
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(&tokens[0].kind, TokenKind::Cell));
        assert!(matches!(&tokens[1].kind, TokenKind::Ident(s) if s == "main"));
    }

    #[test]
    fn test_lex_operators() {
        let src = "a + b == c";
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(&tokens[1].kind, TokenKind::Plus));
        assert!(matches!(&tokens[3].kind, TokenKind::Eq));
    }

    #[test]
    fn test_lex_string() {
        let mut lexer = Lexer::new(r#""hello""#, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(&tokens[0].kind, TokenKind::StringLit(s) if s == "hello"));
    }

    #[test]
    fn test_lex_indent() {
        let mut lexer = Lexer::new("if x\n  return 1\nend", 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();
        assert!(kinds.contains(&&TokenKind::Indent));
        assert!(kinds.contains(&&TokenKind::Dedent));
    }
}
