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
    #[error("invalid bytes literal at line {line}, col {col}")]
    InvalidBytesLiteral { line: usize, col: usize },
    #[error("invalid unicode escape at line {line}, col {col}")]
    InvalidUnicodeEscape { line: usize, col: usize },
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
            pos: 0,
            line: 1,
            col: 1,
            byte_offset: 0,
            base_line,
            base_offset,
            indent_stack: vec![0],
            pending: Vec::new(),
            at_line_start: true,
        }
    }

    fn current(&self) -> Option<char> {
        self.source.get(self.pos).copied()
    }
    fn peek(&self) -> Option<char> {
        self.source.get(self.pos + 1).copied()
    }
    fn peek2(&self) -> Option<char> {
        self.source.get(self.pos + 2).copied()
    }

    fn looks_like_interpolation_start(&self) -> bool {
        let mut i = self.pos + 1; // after '{'
        while let Some(ch) = self.source.get(i).copied() {
            if ch.is_whitespace() {
                i += 1;
                continue;
            }
            return matches!(
                ch,
                'a'..='z' | 'A'..='Z' | '_' | '(' | '[' | '-' | '0'..='9'
            );
        }
        false
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.source.get(self.pos).copied()?;
        self.pos += 1;
        self.byte_offset += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.col = 1;
            self.at_line_start = true;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    fn span_here(&self) -> Span {
        Span::new(
            self.base_offset + self.byte_offset,
            self.base_offset + self.byte_offset,
            self.base_line + self.line - 1,
            self.col,
        )
    }

    fn span_from(&self, so: usize, sl: usize, sc: usize) -> Span {
        Span::new(
            self.base_offset + so,
            self.base_offset + self.byte_offset,
            self.base_line + sl - 1,
            sc,
        )
    }

    fn handle_indentation(&mut self) -> Result<(), LexError> {
        let mut indent = 0;
        while let Some(ch) = self.current() {
            match ch {
                ' ' => {
                    indent += 1;
                    self.advance();
                }
                '\t' => {
                    indent += 2;
                    self.advance();
                }
                _ => break,
            }
        }
        if matches!(self.current(), None | Some('\n') | Some('#')) {
            if self.current().is_none() {
                while self.indent_stack.len() > 1 {
                    self.indent_stack.pop();
                    self.pending
                        .push(Token::new(TokenKind::Dedent, self.span_here()));
                }
            }
            return Ok(());
        }
        let cur = *self.indent_stack.last().unwrap();
        if indent > cur {
            self.indent_stack.push(indent);
            self.pending
                .push(Token::new(TokenKind::Indent, self.span_here()));
        } else if indent < cur {
            while let Some(&top) = self.indent_stack.last() {
                if top > indent {
                    self.indent_stack.pop();
                    self.pending
                        .push(Token::new(TokenKind::Dedent, self.span_here()));
                } else {
                    break;
                }
            }
            if *self.indent_stack.last().unwrap() != indent {
                return Err(LexError::InconsistentIndent {
                    line: self.base_line + self.line - 1,
                });
            }
        }
        Ok(())
    }

    /// Read a unicode escape sequence \u{XXXX}
    fn read_unicode_escape(&mut self, sl: usize, sc: usize) -> Result<char, LexError> {
        // We've consumed \u, now expect {
        if self.current() != Some('{') {
            return Err(LexError::InvalidUnicodeEscape {
                line: self.base_line + sl - 1,
                col: sc,
            });
        }
        self.advance(); // skip {
        let mut hex = String::new();
        while let Some(c) = self.current() {
            if c == '}' {
                break;
            }
            hex.push(c);
            self.advance();
        }
        if self.current() != Some('}') {
            return Err(LexError::InvalidUnicodeEscape {
                line: self.base_line + sl - 1,
                col: sc,
            });
        }
        self.advance(); // skip }
        u32::from_str_radix(&hex, 16)
            .ok()
            .and_then(char::from_u32)
            .ok_or(LexError::InvalidUnicodeEscape {
                line: self.base_line + sl - 1,
                col: sc,
            })
    }

    /// Process an escape sequence after consuming the backslash
    fn process_escape(&mut self, buf: &mut String, sl: usize, sc: usize) -> Result<(), LexError> {
        match self.current() {
            Some('n') => {
                buf.push('\n');
                self.advance();
            }
            Some('t') => {
                buf.push('\t');
                self.advance();
            }
            Some('r') => {
                buf.push('\r');
                self.advance();
            }
            Some('\\') => {
                buf.push('\\');
                self.advance();
            }
            Some('"') => {
                buf.push('"');
                self.advance();
            }
            Some('{') => {
                buf.push('{');
                self.advance();
            }
            Some('0') => {
                buf.push('\0');
                self.advance();
            }
            Some('u') => {
                self.advance(); // skip u
                let ch = self.read_unicode_escape(sl, sc)?;
                buf.push(ch);
            }
            Some('x') => {
                self.advance(); // skip x
                let mut hex = String::new();
                for _ in 0..2 {
                    match self.current() {
                        Some(c) if c.is_ascii_hexdigit() => {
                            hex.push(c);
                            self.advance();
                        }
                        _ => {
                            return Err(LexError::InvalidUnicodeEscape {
                                line: self.base_line + sl - 1,
                                col: sc,
                            })
                        }
                    }
                }
                let byte =
                    u8::from_str_radix(&hex, 16).map_err(|_| LexError::InvalidUnicodeEscape {
                        line: self.base_line + sl - 1,
                        col: sc,
                    })?;
                buf.push(byte as char);
            }
            Some(c) => {
                buf.push('\\');
                buf.push(c);
                self.advance();
            }
            None => {
                return Err(LexError::UnterminatedString {
                    line: self.base_line + sl - 1,
                    col: sc,
                })
            }
        }
        Ok(())
    }

    fn read_triple_quoted_string(&mut self) -> Result<Token, LexError> {
        let (so, sl, sc) = (self.byte_offset, self.line, self.col);
        // Skip the three opening quotes
        self.advance();
        self.advance();
        self.advance();

        let mut segments = Vec::new();
        let mut cur_segment = String::new();
        let mut is_interp = false;

        loop {
            match self.current() {
                None => {
                    return Err(LexError::UnterminatedString {
                        line: self.base_line + sl - 1,
                        col: sc,
                    })
                }
                Some('"') if self.peek() == Some('"') && self.peek2() == Some('"') => {
                    self.advance();
                    self.advance();
                    self.advance();
                    break;
                }
                Some('\\') => {
                    self.advance();
                    self.process_escape(&mut cur_segment, sl, sc)?;
                }
                Some('{') if self.looks_like_interpolation_start() => {
                    is_interp = true;
                    if !cur_segment.is_empty() {
                        segments.push((false, cur_segment.clone()));
                        cur_segment.clear();
                    }
                    self.advance(); // skip {
                    let mut expr_str = String::new();
                    let mut brace_balance = 1;
                    while let Some(c) = self.current() {
                        if c == '}' {
                            brace_balance -= 1;
                            if brace_balance == 0 {
                                break;
                            }
                            expr_str.push(c);
                            self.advance();
                        } else if c == '{' {
                            brace_balance += 1;
                            expr_str.push(c);
                            self.advance();
                        } else if c == '"' {
                            expr_str.push(c);
                            self.advance();
                            while let Some(ic) = self.current() {
                                expr_str.push(ic);
                                self.advance();
                                if ic == '"' && !expr_str.ends_with("\\\"") {
                                    break;
                                }
                            }
                        } else {
                            expr_str.push(c);
                            self.advance();
                        }
                    }
                    if brace_balance != 0 {
                        return Err(LexError::UnterminatedString {
                            line: self.base_line + sl - 1,
                            col: sc,
                        });
                    }
                    self.advance(); // skip }
                    segments.push((true, expr_str.trim().to_string()));
                }
                Some('{') => {
                    cur_segment.push('{');
                    self.advance();
                }
                Some(c) => {
                    cur_segment.push(c);
                    self.advance();
                }
            }
        }

        // Dedent: strip common leading whitespace
        let raw_content = if is_interp {
            if !cur_segment.is_empty() {
                segments.push((false, cur_segment));
            }
            // For interpolated triple-quoted, apply dedent to text segments
            self.dedent_interp_segments(&mut segments);
            let span = self.span_from(so, sl, sc);
            return Ok(Token::new(TokenKind::StringInterpLit(segments), span));
        } else {
            cur_segment
        };

        let dedented = self.dedent_string(&raw_content);
        let span = self.span_from(so, sl, sc);
        Ok(Token::new(TokenKind::StringLit(dedented), span))
    }

    fn dedent_string(&self, s: &str) -> String {
        let lines: Vec<&str> = s.split('\n').collect();
        if lines.len() <= 1 {
            return s.to_string();
        }
        // Find minimum indentation of non-empty lines (skip first line which follows """)
        let min_indent = lines
            .iter()
            .skip(1)
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.len() - l.trim_start().len())
            .min()
            .unwrap_or(0);

        let mut result = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if i == 0 {
                result.push(*line);
            } else if line.len() >= min_indent {
                result.push(&line[min_indent..]);
            } else {
                result.push(line.trim());
            }
        }
        // Trim leading/trailing empty lines from the result
        let joined = result.join("\n");
        let trimmed = joined.trim_start_matches('\n');
        let trimmed = trimmed.trim_end_matches('\n');
        trimmed.to_string()
    }

    fn dedent_interp_segments(&self, segments: &mut [(bool, String)]) {
        // Apply dedent to text-only segments
        for seg in segments.iter_mut() {
            if !seg.0 {
                seg.1 = self.dedent_string(&seg.1);
            }
        }
    }

    fn read_raw_string(&mut self) -> Result<Token, LexError> {
        let (so, sl, sc) = (self.byte_offset, self.line, self.col);
        self.advance(); // skip 'r'
                        // Check for triple-quoted raw: r"""..."""
        if self.current() == Some('"') && self.peek() == Some('"') && self.peek2() == Some('"') {
            self.advance();
            self.advance();
            self.advance(); // skip """
            let mut content = String::new();
            loop {
                match self.current() {
                    None => {
                        return Err(LexError::UnterminatedString {
                            line: self.base_line + sl - 1,
                            col: sc,
                        })
                    }
                    Some('"') if self.peek() == Some('"') && self.peek2() == Some('"') => {
                        self.advance();
                        self.advance();
                        self.advance();
                        break;
                    }
                    Some(c) => {
                        content.push(c);
                        self.advance();
                    }
                }
            }
            let dedented = self.dedent_string(&content);
            let span = self.span_from(so, sl, sc);
            return Ok(Token::new(TokenKind::RawStringLit(dedented), span));
        }
        // Regular raw string: r"..."
        if self.current() != Some('"') {
            // Not a raw string, it's an identifier starting with 'r'
            // Put back by not advancing and let read_ident handle it
            // Actually we already advanced past 'r', so we need to handle this differently
            // This shouldn't happen as we check for '"' before calling read_raw_string
            return Err(LexError::UnexpectedChar {
                ch: self.current().unwrap_or(' '),
                line: self.base_line + sl - 1,
                col: sc,
            });
        }
        self.advance(); // skip opening "
        let mut content = String::new();
        loop {
            match self.current() {
                None | Some('\n') => {
                    return Err(LexError::UnterminatedString {
                        line: self.base_line + sl - 1,
                        col: sc,
                    })
                }
                Some('"') => {
                    self.advance();
                    break;
                }
                Some(c) => {
                    content.push(c);
                    self.advance();
                }
            }
        }
        let span = self.span_from(so, sl, sc);
        Ok(Token::new(TokenKind::RawStringLit(content), span))
    }

    fn read_bytes_literal(&mut self) -> Result<Token, LexError> {
        let (so, sl, sc) = (self.byte_offset, self.line, self.col);
        self.advance(); // skip 'b'
        if self.current() != Some('"') {
            return Err(LexError::InvalidBytesLiteral {
                line: self.base_line + sl - 1,
                col: sc,
            });
        }
        self.advance(); // skip opening "
        let mut bytes = Vec::new();
        loop {
            match self.current() {
                None | Some('\n') => {
                    return Err(LexError::UnterminatedString {
                        line: self.base_line + sl - 1,
                        col: sc,
                    })
                }
                Some('"') => {
                    self.advance();
                    break;
                }
                Some(c) if c.is_ascii_hexdigit() => {
                    let hi = c;
                    self.advance();
                    match self.current() {
                        Some(lo) if lo.is_ascii_hexdigit() => {
                            self.advance();
                            let byte =
                                u8::from_str_radix(&format!("{}{}", hi, lo), 16).map_err(|_| {
                                    LexError::InvalidBytesLiteral {
                                        line: self.base_line + sl - 1,
                                        col: sc,
                                    }
                                })?;
                            bytes.push(byte);
                        }
                        _ => {
                            return Err(LexError::InvalidBytesLiteral {
                                line: self.base_line + sl - 1,
                                col: sc,
                            })
                        }
                    }
                }
                _ => {
                    return Err(LexError::InvalidBytesLiteral {
                        line: self.base_line + sl - 1,
                        col: sc,
                    })
                }
            }
        }
        let span = self.span_from(so, sl, sc);
        Ok(Token::new(TokenKind::BytesLit(bytes), span))
    }

    fn read_string(&mut self) -> Result<Token, LexError> {
        // Check for triple-quoted string
        if self.peek() == Some('"') && self.peek2() == Some('"') {
            return self.read_triple_quoted_string();
        }

        let (so, sl, sc) = (self.byte_offset, self.line, self.col);
        self.advance(); // opening quote
        let mut segments = Vec::new();
        let mut cur_segment = String::new();
        let mut is_interp = false;

        loop {
            match self.current() {
                None | Some('\n') => {
                    return Err(LexError::UnterminatedString {
                        line: self.base_line + sl - 1,
                        col: sc,
                    })
                }
                Some('\\') => {
                    self.advance();
                    self.process_escape(&mut cur_segment, sl, sc)?;
                }
                Some('{') if self.looks_like_interpolation_start() => {
                    // Start of interpolation
                    is_interp = true;
                    if !cur_segment.is_empty() {
                        segments.push((false, cur_segment.clone()));
                        cur_segment.clear();
                    }
                    self.advance(); // skip {
                                    // Read until }
                    let mut expr_str = String::new();
                    let mut brace_balance = 1;
                    while let Some(c) = self.current() {
                        if c == '}' {
                            brace_balance -= 1;
                            if brace_balance == 0 {
                                break;
                            }
                            expr_str.push(c);
                            self.advance();
                        } else if c == '{' {
                            brace_balance += 1;
                            expr_str.push(c);
                            self.advance();
                        } else if c == '"' {
                            // Handle strings inside interpolation to avoid incorrect brace matching
                            expr_str.push(c);
                            self.advance();
                            while let Some(ic) = self.current() {
                                expr_str.push(ic);
                                self.advance();
                                if ic == '"' && !expr_str.ends_with("\\\"") {
                                    break;
                                }
                            }
                        } else {
                            expr_str.push(c);
                            self.advance();
                        }
                    }
                    if brace_balance != 0 {
                        return Err(LexError::UnterminatedString {
                            line: self.base_line + sl - 1,
                            col: sc,
                        });
                    }
                    self.advance(); // skip }
                    segments.push((true, expr_str.trim().to_string()));
                }
                Some('{') => {
                    cur_segment.push('{');
                    self.advance();
                }
                Some('"') => {
                    self.advance();
                    break;
                }
                Some(c) => {
                    cur_segment.push(c);
                    self.advance();
                }
            }
        }

        let span = self.span_from(so, sl, sc);
        if is_interp {
            if !cur_segment.is_empty() {
                segments.push((false, cur_segment));
            }
            Ok(Token::new(TokenKind::StringInterpLit(segments), span))
        } else {
            Ok(Token::new(TokenKind::StringLit(cur_segment), span))
        }
    }

    fn read_number(&mut self) -> Result<Token, LexError> {
        let (so, sl, sc) = (self.byte_offset, self.line, self.col);

        // Check for 0x, 0b, 0o prefixes
        if self.current() == Some('0') {
            match self.peek() {
                Some('x') | Some('X') => return self.read_hex_number(so, sl, sc),
                Some('b') if matches!(self.peek2(), Some('0') | Some('1')) => {
                    return self.read_bin_number(so, sl, sc)
                }
                Some('o') => return self.read_oct_number(so, sl, sc),
                _ => {}
            }
        }

        let mut ns = String::new();
        let mut is_float = false;
        while let Some(ch) = self.current() {
            if ch.is_ascii_digit() {
                ns.push(ch);
                self.advance();
            } else if ch == '.' && !is_float {
                // Check for .. (range) and ... (spread) - don't consume the dot
                if self.peek() == Some('.') {
                    break;
                }
                if matches!(self.peek(), Some(d) if d.is_ascii_digit()) {
                    is_float = true;
                    ns.push(ch);
                    self.advance();
                } else {
                    break;
                }
            } else if ch == '_' {
                self.advance();
            } else if (ch == 'e' || ch == 'E') && !is_float {
                // Scientific notation
                is_float = true;
                ns.push(ch);
                self.advance();
                // Optional +/- sign
                if matches!(self.current(), Some('+') | Some('-')) {
                    ns.push(self.current().unwrap());
                    self.advance();
                }
            } else {
                break;
            }
        }
        let span = self.span_from(so, sl, sc);
        if is_float {
            ns.parse::<f64>()
                .map(|f| Token::new(TokenKind::FloatLit(f), span))
                .map_err(|_| LexError::InvalidNumber {
                    line: self.base_line + sl - 1,
                    col: sc,
                })
        } else {
            ns.parse::<i64>()
                .map(|n| Token::new(TokenKind::IntLit(n), span))
                .map_err(|_| LexError::InvalidNumber {
                    line: self.base_line + sl - 1,
                    col: sc,
                })
        }
    }

    fn read_hex_number(&mut self, so: usize, sl: usize, sc: usize) -> Result<Token, LexError> {
        self.advance(); // skip 0
        self.advance(); // skip x/X
        let mut hex = String::new();
        while let Some(ch) = self.current() {
            if ch.is_ascii_hexdigit() {
                hex.push(ch);
                self.advance();
            } else if ch == '_' {
                self.advance();
            } else {
                break;
            }
        }
        if hex.is_empty() {
            return Err(LexError::InvalidNumber {
                line: self.base_line + sl - 1,
                col: sc,
            });
        }
        let span = self.span_from(so, sl, sc);
        i64::from_str_radix(&hex, 16)
            .map(|n| Token::new(TokenKind::IntLit(n), span))
            .map_err(|_| LexError::InvalidNumber {
                line: self.base_line + sl - 1,
                col: sc,
            })
    }

    fn read_bin_number(&mut self, so: usize, sl: usize, sc: usize) -> Result<Token, LexError> {
        self.advance(); // skip 0
        self.advance(); // skip b
        let mut bin = String::new();
        while let Some(ch) = self.current() {
            if ch == '0' || ch == '1' {
                bin.push(ch);
                self.advance();
            } else if ch == '_' {
                self.advance();
            } else {
                break;
            }
        }
        if bin.is_empty() {
            return Err(LexError::InvalidNumber {
                line: self.base_line + sl - 1,
                col: sc,
            });
        }
        let span = self.span_from(so, sl, sc);
        i64::from_str_radix(&bin, 2)
            .map(|n| Token::new(TokenKind::IntLit(n), span))
            .map_err(|_| LexError::InvalidNumber {
                line: self.base_line + sl - 1,
                col: sc,
            })
    }

    fn read_oct_number(&mut self, so: usize, sl: usize, sc: usize) -> Result<Token, LexError> {
        self.advance(); // skip 0
        self.advance(); // skip o
        let mut oct = String::new();
        while let Some(ch) = self.current() {
            if ('0'..='7').contains(&ch) {
                oct.push(ch);
                self.advance();
            } else if ch == '_' {
                self.advance();
            } else {
                break;
            }
        }
        if oct.is_empty() {
            return Err(LexError::InvalidNumber {
                line: self.base_line + sl - 1,
                col: sc,
            });
        }
        let span = self.span_from(so, sl, sc);
        i64::from_str_radix(&oct, 8)
            .map(|n| Token::new(TokenKind::IntLit(n), span))
            .map_err(|_| LexError::InvalidNumber {
                line: self.base_line + sl - 1,
                col: sc,
            })
    }

    fn read_ident(&mut self) -> Token {
        let (so, sl, sc) = (self.byte_offset, self.line, self.col);
        let mut id = String::new();
        while let Some(ch) = self.current() {
            if ch.is_alphanumeric() || ch == '_' {
                id.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        let span = self.span_from(so, sl, sc);
        let kind = match id.as_str() {
            "record" => TokenKind::Record,
            "enum" => TokenKind::Enum,
            "cell" => TokenKind::Cell,
            "let" => TokenKind::Let,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "match" => TokenKind::Match,
            "return" => TokenKind::Return,
            "halt" => TokenKind::Halt,
            "end" => TokenKind::End,
            "use" => TokenKind::Use,
            "tool" => TokenKind::Tool,
            "as" => TokenKind::As,
            "grant" => TokenKind::Grant,
            "expect" => TokenKind::Expect,
            "schema" => TokenKind::Schema,
            "role" => TokenKind::Role,
            "where" => TokenKind::Where,
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "not" => TokenKind::Not,
            "null" => TokenKind::NullLit,
            "Null" => TokenKind::Null,
            "result" => TokenKind::Result,
            "ok" => TokenKind::Ok_,
            "err" => TokenKind::Err_,
            "list" => TokenKind::List,
            "map" => TokenKind::Map,
            "true" => TokenKind::BoolLit(true),
            "false" => TokenKind::BoolLit(false),
            // New keywords
            "while" => TokenKind::While,
            "loop" => TokenKind::Loop,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "mut" => TokenKind::Mut,
            "const" => TokenKind::Const,
            "pub" => TokenKind::Pub,
            "import" => TokenKind::Import,
            "from" => TokenKind::From,
            "async" => TokenKind::Async,
            "await" => TokenKind::Await,
            "parallel" => TokenKind::Parallel,
            "fn" => TokenKind::Fn,
            "trait" => TokenKind::Trait,
            "impl" => TokenKind::Impl,
            "type" => TokenKind::Type,
            "set" => TokenKind::Set,
            "tuple" => TokenKind::Tuple,
            "emit" => TokenKind::Emit,
            "yield" => TokenKind::Yield,
            "mod" => TokenKind::Mod,
            "self" => TokenKind::SelfKw,
            "with" => TokenKind::With,
            "try" => TokenKind::Try,
            "union" => TokenKind::Union,
            "step" => TokenKind::Step,
            "comptime" => TokenKind::Comptime,
            "macro" => TokenKind::Macro,
            "extern" => TokenKind::Extern,
            "then" => TokenKind::Then,
            "when" => TokenKind::When,
            "is" => TokenKind::Is,
            // Type keywords
            "bool" => TokenKind::Bool,
            "int" => TokenKind::Int_,
            "float" => TokenKind::Float_,
            "string" => TokenKind::String_,
            "bytes" => TokenKind::Bytes,
            "json" => TokenKind::Json,
            _ => TokenKind::Ident(id),
        };
        Token::new(kind, span)
    }

    fn two_char(&mut self, second: char, matched: TokenKind, single: TokenKind) -> Token {
        let (so, sl, sc) = (self.byte_offset, self.line, self.col);
        self.advance();
        if self.current() == Some(second) {
            self.advance();
            Token::new(matched, self.span_from(so, sl, sc))
        } else {
            Token::new(single, self.span_from(so, sl, sc))
        }
    }

    fn single(&mut self, kind: TokenKind) -> Token {
        let span = self.span_here();
        self.advance();
        Token::new(kind, span)
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        while self.pos < self.source.len() {
            if self.at_line_start {
                self.at_line_start = false;
                self.handle_indentation()?;
                tokens.append(&mut self.pending);
            }
            let ch = match self.current() {
                Some(c) => c,
                None => break,
            };
            match ch {
                '\n' => {
                    let span = self.span_here();
                    self.advance();
                    if !matches!(
                        tokens.last().map(|t| &t.kind),
                        Some(TokenKind::Newline) | Some(TokenKind::Indent) | None
                    ) {
                        tokens.push(Token::new(TokenKind::Newline, span));
                    }
                }
                ' ' | '\t' | '\r' => {
                    while matches!(self.current(), Some(' ' | '\t' | '\r')) {
                        self.advance();
                    }
                }
                '#' => {
                    while matches!(self.current(), Some(c) if c != '\n') {
                        self.advance();
                    }
                }
                '"' => tokens.push(self.read_string()?),
                '0'..='9' => tokens.push(self.read_number()?),
                'r' if self.peek() == Some('"') => tokens.push(self.read_raw_string()?),
                'b' if self.peek() == Some('"') => tokens.push(self.read_bytes_literal()?),
                'a'..='z' | 'A'..='Z' | '_' => tokens.push(self.read_ident()),
                '+' => {
                    let (so, sl, sc) = (self.byte_offset, self.line, self.col);
                    self.advance();
                    match self.current() {
                        Some('=') => {
                            self.advance();
                            tokens.push(Token::new(
                                TokenKind::PlusAssign,
                                self.span_from(so, sl, sc),
                            ));
                        }
                        Some('+') => {
                            self.advance();
                            tokens
                                .push(Token::new(TokenKind::PlusPlus, self.span_from(so, sl, sc)));
                        }
                        _ => {
                            tokens.push(Token::new(TokenKind::Plus, self.span_from(so, sl, sc)));
                        }
                    }
                }
                '-' => {
                    let (so, sl, sc) = (self.byte_offset, self.line, self.col);
                    self.advance();
                    match self.current() {
                        Some('>') => {
                            self.advance();
                            tokens.push(Token::new(TokenKind::Arrow, self.span_from(so, sl, sc)));
                        }
                        Some('=') => {
                            self.advance();
                            tokens.push(Token::new(
                                TokenKind::MinusAssign,
                                self.span_from(so, sl, sc),
                            ));
                        }
                        _ => {
                            tokens.push(Token::new(TokenKind::Minus, self.span_from(so, sl, sc)));
                        }
                    }
                }
                '*' => {
                    let (so, sl, sc) = (self.byte_offset, self.line, self.col);
                    self.advance();
                    match self.current() {
                        Some('*') => {
                            self.advance();
                            if self.current() == Some('=') {
                                self.advance();
                                tokens.push(Token::new(
                                    TokenKind::StarStarAssign,
                                    self.span_from(so, sl, sc),
                                ));
                            } else {
                                tokens.push(Token::new(TokenKind::StarStar, self.span_from(so, sl, sc)));
                            }
                        }
                        Some('=') => {
                            self.advance();
                            tokens.push(Token::new(
                                TokenKind::StarAssign,
                                self.span_from(so, sl, sc),
                            ));
                        }
                        _ => {
                            tokens.push(Token::new(TokenKind::Star, self.span_from(so, sl, sc)));
                        }
                    }
                }
                '/' => {
                    let (so, sl, sc) = (self.byte_offset, self.line, self.col);
                    self.advance();
                    match self.current() {
                        Some('/') => {
                            self.advance();
                            if self.current() == Some('=') {
                                self.advance();
                                tokens.push(Token::new(
                                    TokenKind::FloorDivAssign,
                                    self.span_from(so, sl, sc),
                                ));
                            } else {
                                tokens.push(Token::new(
                                    TokenKind::FloorDiv,
                                    self.span_from(so, sl, sc),
                                ));
                            }
                        }
                        Some('=') => {
                            self.advance();
                            tokens.push(Token::new(
                                TokenKind::SlashAssign,
                                self.span_from(so, sl, sc),
                            ));
                        }
                        _ => {
                            tokens.push(Token::new(TokenKind::Slash, self.span_from(so, sl, sc)));
                        }
                    }
                }
                '%' => {
                    let (so, sl, sc) = (self.byte_offset, self.line, self.col);
                    self.advance();
                    if self.current() == Some('=') {
                        self.advance();
                        tokens.push(Token::new(
                            TokenKind::PercentAssign,
                            self.span_from(so, sl, sc),
                        ));
                    } else {
                        tokens.push(Token::new(TokenKind::Percent, self.span_from(so, sl, sc)));
                    }
                }
                '=' => {
                    let (so, sl, sc) = (self.byte_offset, self.line, self.col);
                    self.advance();
                    match self.current() {
                        Some('=') => {
                            self.advance();
                            tokens.push(Token::new(TokenKind::Eq, self.span_from(so, sl, sc)));
                        }
                        Some('>') => {
                            self.advance();
                            tokens
                                .push(Token::new(TokenKind::FatArrow, self.span_from(so, sl, sc)));
                        }
                        _ => {
                            tokens.push(Token::new(TokenKind::Assign, self.span_from(so, sl, sc)));
                        }
                    }
                }
                '!' => {
                    let (so, sl, sc) = (self.byte_offset, self.line, self.col);
                    self.advance();
                    match self.current() {
                        Some('=') => {
                            self.advance();
                            tokens.push(Token::new(TokenKind::NotEq, self.span_from(so, sl, sc)));
                        }
                        _ => {
                            tokens.push(Token::new(TokenKind::Bang, self.span_from(so, sl, sc)));
                        }
                    }
                }
                '?' => {
                    let (so, sl, sc) = (self.byte_offset, self.line, self.col);
                    self.advance();
                    match self.current() {
                        Some('?') => {
                            self.advance();
                            tokens.push(Token::new(
                                TokenKind::QuestionQuestion,
                                self.span_from(so, sl, sc),
                            ));
                        }
                        Some('.') => {
                            self.advance();
                            tokens.push(Token::new(
                                TokenKind::QuestionDot,
                                self.span_from(so, sl, sc),
                            ));
                        }
                        Some('[') => {
                            self.advance();
                            tokens.push(Token::new(
                                TokenKind::QuestionBracket,
                                self.span_from(so, sl, sc),
                            ));
                        }
                        _ => {
                            tokens
                                .push(Token::new(TokenKind::Question, self.span_from(so, sl, sc)));
                        }
                    }
                }
                '<' => tokens.push(self.two_char('=', TokenKind::LtEq, TokenKind::Lt)),
                '>' => {
                    let (so, sl, sc) = (self.byte_offset, self.line, self.col);
                    self.advance();
                    match self.current() {
                        Some('=') => {
                            self.advance();
                            tokens.push(Token::new(TokenKind::GtEq, self.span_from(so, sl, sc)));
                        }
                        Some('>') => {
                            self.advance();
                            tokens.push(Token::new(TokenKind::Compose, self.span_from(so, sl, sc)));
                        }
                        _ => {
                            tokens.push(Token::new(TokenKind::Gt, self.span_from(so, sl, sc)));
                        }
                    }
                }
                '.' => {
                    let (so, sl, sc) = (self.byte_offset, self.line, self.col);
                    self.advance();
                    if self.current() == Some('.') {
                        self.advance();
                        if self.current() == Some('.') {
                            self.advance();
                            tokens
                                .push(Token::new(TokenKind::DotDotDot, self.span_from(so, sl, sc)));
                        } else if self.current() == Some('=') {
                            self.advance();
                            tokens
                                .push(Token::new(TokenKind::DotDotEq, self.span_from(so, sl, sc)));
                        } else {
                            tokens.push(Token::new(TokenKind::DotDot, self.span_from(so, sl, sc)));
                        }
                    } else {
                        tokens.push(Token::new(TokenKind::Dot, self.span_from(so, sl, sc)));
                    }
                }
                ',' => tokens.push(self.single(TokenKind::Comma)),
                ':' => tokens.push(self.single(TokenKind::Colon)),
                ';' => tokens.push(self.single(TokenKind::Semicolon)),
                '|' => {
                    let (so, sl, sc) = (self.byte_offset, self.line, self.col);
                    self.advance();
                    match self.current() {
                        Some('>') => {
                            self.advance();
                            tokens.push(Token::new(
                                TokenKind::PipeForward,
                                self.span_from(so, sl, sc),
                            ));
                        }
                        Some('=') => {
                            self.advance();
                            tokens.push(Token::new(
                                TokenKind::PipeAssign,
                                self.span_from(so, sl, sc),
                            ));
                        }
                        _ => {
                            tokens.push(Token::new(TokenKind::Pipe, self.span_from(so, sl, sc)));
                        }
                    }
                }
                '@' => tokens.push(self.single(TokenKind::At)),
                '&' => {
                    let (so, sl, sc) = (self.byte_offset, self.line, self.col);
                    self.advance();
                    if self.current() == Some('=') {
                        self.advance();
                        tokens.push(Token::new(
                            TokenKind::AmpAssign,
                            self.span_from(so, sl, sc),
                        ));
                    } else {
                        tokens.push(Token::new(TokenKind::Ampersand, self.span_from(so, sl, sc)));
                    }
                }
                '~' => {
                    let so = self.byte_offset;
                    let sl = self.line;
                    let sc = self.col;
                    self.advance();
                    match self.current() {
                        Some('>') => {
                            self.advance();
                            tokens.push(Token::new(
                                TokenKind::TildeArrow,
                                self.span_from(so, sl, sc),
                            ));
                        }
                        _ => {
                            tokens.push(Token::new(TokenKind::Tilde, self.span_from(so, sl, sc)));
                        }
                    }
                }
                '^' => {
                    let (so, sl, sc) = (self.byte_offset, self.line, self.col);
                    self.advance();
                    if self.current() == Some('=') {
                        self.advance();
                        tokens.push(Token::new(
                            TokenKind::CaretAssign,
                            self.span_from(so, sl, sc),
                        ));
                    } else {
                        tokens.push(Token::new(TokenKind::Caret, self.span_from(so, sl, sc)));
                    }
                }
                '(' => tokens.push(self.single(TokenKind::LParen)),
                ')' => tokens.push(self.single(TokenKind::RParen)),
                '[' => tokens.push(self.single(TokenKind::LBracket)),
                ']' => tokens.push(self.single(TokenKind::RBracket)),
                '{' => tokens.push(self.single(TokenKind::LBrace)),
                '}' => tokens.push(self.single(TokenKind::RBrace)),
                '\\' if self.peek() == Some('\n') => {
                    // Line continuation
                    self.advance(); // skip backslash
                    self.advance(); // skip newline
                                    // Don't emit newline, don't set at_line_start
                    self.at_line_start = false;
                    // Skip leading whitespace on the continuation line
                    while matches!(self.current(), Some(' ' | '\t')) {
                        self.advance();
                    }
                }
                c => tokens.push(self.single(TokenKind::Symbol(c))),
            }
        }
        while self.indent_stack.len() > 1 {
            self.indent_stack.pop();
            tokens.push(Token::new(TokenKind::Dedent, self.span_here()));
        }
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

    #[test]
    fn test_lex_hex_number() {
        let mut lexer = Lexer::new("0xFF", 1, 0);
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(&tokens[0].kind, TokenKind::IntLit(255)));
    }

    #[test]
    fn test_lex_bin_number() {
        let mut lexer = Lexer::new("0b1010", 1, 0);
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(&tokens[0].kind, TokenKind::IntLit(10)));
    }

    #[test]
    fn test_lex_oct_number() {
        let mut lexer = Lexer::new("0o777", 1, 0);
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(&tokens[0].kind, TokenKind::IntLit(511)));
    }

    #[test]
    fn test_lex_scientific() {
        let mut lexer = Lexer::new("1e10", 1, 0);
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(&tokens[0].kind, TokenKind::FloatLit(f) if *f == 1e10));
    }

    #[test]
    fn test_lex_compound_assign() {
        let mut lexer = Lexer::new("+= -= *= /=", 1, 0);
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(&tokens[0].kind, TokenKind::PlusAssign));
        assert!(matches!(&tokens[1].kind, TokenKind::MinusAssign));
        assert!(matches!(&tokens[2].kind, TokenKind::StarAssign));
        assert!(matches!(&tokens[3].kind, TokenKind::SlashAssign));
    }

    #[test]
    fn test_lex_new_operators() {
        let mut lexer = Lexer::new("** .. ..= |> >> ?? ?. ! ? ... => ++ & ~ ^", 1, 0);
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(&tokens[0].kind, TokenKind::StarStar));
        assert!(matches!(&tokens[1].kind, TokenKind::DotDot));
        assert!(matches!(&tokens[2].kind, TokenKind::DotDotEq));
        assert!(matches!(&tokens[3].kind, TokenKind::PipeForward));
        assert!(matches!(&tokens[4].kind, TokenKind::Compose));
        assert!(matches!(&tokens[5].kind, TokenKind::QuestionQuestion));
        assert!(matches!(&tokens[6].kind, TokenKind::QuestionDot));
        assert!(matches!(&tokens[7].kind, TokenKind::Bang));
        assert!(matches!(&tokens[8].kind, TokenKind::Question));
        assert!(matches!(&tokens[9].kind, TokenKind::DotDotDot));
        assert!(matches!(&tokens[10].kind, TokenKind::FatArrow));
        assert!(matches!(&tokens[11].kind, TokenKind::PlusPlus));
        assert!(matches!(&tokens[12].kind, TokenKind::Ampersand));
        assert!(matches!(&tokens[13].kind, TokenKind::Tilde));
        assert!(matches!(&tokens[14].kind, TokenKind::Caret));
    }

    #[test]
    fn test_lex_new_keywords() {
        let mut lexer = Lexer::new("while loop break continue mut const pub import from async await parallel fn trait impl type set tuple emit yield mod self with try union step comptime macro extern then when", 1, 0);
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(&tokens[0].kind, TokenKind::While));
        assert!(matches!(&tokens[1].kind, TokenKind::Loop));
        assert!(matches!(&tokens[2].kind, TokenKind::Break));
        assert!(matches!(&tokens[3].kind, TokenKind::Continue));
        assert!(matches!(&tokens[4].kind, TokenKind::Mut));
        assert!(matches!(&tokens[5].kind, TokenKind::Const));
        assert!(matches!(&tokens[6].kind, TokenKind::Pub));
        assert!(matches!(&tokens[7].kind, TokenKind::Import));
        assert!(matches!(&tokens[8].kind, TokenKind::From));
        assert!(matches!(&tokens[9].kind, TokenKind::Async));
        assert!(matches!(&tokens[10].kind, TokenKind::Await));
        assert!(matches!(&tokens[11].kind, TokenKind::Parallel));
        assert!(matches!(&tokens[12].kind, TokenKind::Fn));
        assert!(matches!(&tokens[13].kind, TokenKind::Trait));
        assert!(matches!(&tokens[14].kind, TokenKind::Impl));
        assert!(matches!(&tokens[15].kind, TokenKind::Type));
        assert!(matches!(&tokens[16].kind, TokenKind::Set));
        assert!(matches!(&tokens[17].kind, TokenKind::Tuple));
        assert!(matches!(&tokens[18].kind, TokenKind::Emit));
        assert!(matches!(&tokens[19].kind, TokenKind::Yield));
        assert!(matches!(&tokens[20].kind, TokenKind::Mod));
        assert!(matches!(&tokens[21].kind, TokenKind::SelfKw));
        assert!(matches!(&tokens[22].kind, TokenKind::With));
        assert!(matches!(&tokens[23].kind, TokenKind::Try));
        assert!(matches!(&tokens[24].kind, TokenKind::Union));
        assert!(matches!(&tokens[25].kind, TokenKind::Step));
        assert!(matches!(&tokens[26].kind, TokenKind::Comptime));
        assert!(matches!(&tokens[27].kind, TokenKind::Macro));
        assert!(matches!(&tokens[28].kind, TokenKind::Extern));
        assert!(matches!(&tokens[29].kind, TokenKind::Then));
        assert!(matches!(&tokens[30].kind, TokenKind::When));
    }

    #[test]
    fn test_lex_raw_string() {
        let mut lexer = Lexer::new(r#"r"no \n here""#, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(&tokens[0].kind, TokenKind::RawStringLit(s) if s == r"no \n here"));
    }

    #[test]
    fn test_lex_bytes_literal() {
        let mut lexer = Lexer::new(r#"b"48656C6C6F""#, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        assert!(
            matches!(&tokens[0].kind, TokenKind::BytesLit(b) if b == &[0x48, 0x65, 0x6C, 0x6C, 0x6F])
        );
    }

    #[test]
    fn test_lex_fat_arrow() {
        let mut lexer = Lexer::new("=>", 1, 0);
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(&tokens[0].kind, TokenKind::FatArrow));
    }

    #[test]
    fn test_lex_line_continuation() {
        let mut lexer = Lexer::new("a +\\\n  b", 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();
        // Should NOT have a Newline between + and b
        assert!(!kinds.contains(&&TokenKind::Newline));
        assert!(matches!(&tokens[0].kind, TokenKind::Ident(s) if s == "a"));
        assert!(matches!(&tokens[1].kind, TokenKind::Plus));
        assert!(matches!(&tokens[2].kind, TokenKind::Ident(s) if s == "b"));
    }

    #[test]
    fn test_lex_null_literal() {
        let mut lexer = Lexer::new("null", 1, 0);
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(&tokens[0].kind, TokenKind::NullLit));
    }

    #[test]
    fn test_lex_unicode_escape() {
        let mut lexer = Lexer::new(r#""\u{0041}""#, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(&tokens[0].kind, TokenKind::StringLit(s) if s == "A"));
    }

    #[test]
    fn test_lex_hex_byte_escape() {
        let mut lexer = Lexer::new(r#""\x41""#, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(&tokens[0].kind, TokenKind::StringLit(s) if s == "A"));
    }
}
