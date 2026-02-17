//! Semantic token highlighting using real lexer output

use lsp_types::{SemanticToken, SemanticTokens, SemanticTokensResult};
use lumen_compiler::compiler::lexer::Lexer;
use lumen_compiler::compiler::tokens::TokenKind;
use lumen_compiler::markdown::extract::extract_blocks;

/// Token type indices (must match the legend in main.rs)
const TOKEN_TYPE_KEYWORD: u32 = 0;
const TOKEN_TYPE_TYPE: u32 = 1;
#[allow(dead_code)]
const TOKEN_TYPE_FUNCTION: u32 = 2;
const TOKEN_TYPE_VARIABLE: u32 = 3;
#[allow(dead_code)]
const TOKEN_TYPE_PARAMETER: u32 = 4;
const TOKEN_TYPE_OPERATOR: u32 = 5;
const TOKEN_TYPE_STRING: u32 = 6;
const TOKEN_TYPE_NUMBER: u32 = 7;
#[allow(dead_code)]
const TOKEN_TYPE_COMMENT: u32 = 8;
#[allow(dead_code)]
const TOKEN_TYPE_ENUM_MEMBER: u32 = 9;
#[allow(dead_code)]
const TOKEN_TYPE_STRUCT: u32 = 10;
#[allow(dead_code)]
const TOKEN_TYPE_ENUM: u32 = 11;
const TOKEN_TYPE_DECORATOR: u32 = 12;

pub fn build_semantic_tokens(text: &str, is_markdown: bool) -> Option<SemanticTokensResult> {
    let (code, first_line, first_offset) = if is_markdown {
        let extracted = extract_blocks(text);
        let mut full_code = String::new();
        let mut first_block_line = 1;
        let mut first_block_offset = 0;

        for (i, block) in extracted.code_blocks.iter().enumerate() {
            if i == 0 {
                first_block_line = block.code_start_line;
                first_block_offset = block.code_offset;
            }
            if !full_code.is_empty() {
                full_code.push('\n');
            }
            full_code.push_str(&block.code);
        }

        if full_code.is_empty() {
            return Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            }));
        }

        (full_code, first_block_line, first_block_offset)
    } else {
        (text.to_string(), 1, 0)
    };

    let mut lexer = Lexer::new(&code, first_line, first_offset);
    let tokens = lexer.tokenize().ok()?;

    let mut semantic_tokens = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_char = 0u32;

    for token in tokens {
        let token_type = match &token.kind {
            // Keywords
            TokenKind::Record
            | TokenKind::Enum
            | TokenKind::Cell
            | TokenKind::Let
            | TokenKind::Mut
            | TokenKind::If
            | TokenKind::Else
            | TokenKind::For
            | TokenKind::While
            | TokenKind::Loop
            | TokenKind::Match
            | TokenKind::Return
            | TokenKind::Halt
            | TokenKind::End
            | TokenKind::Use
            | TokenKind::As
            | TokenKind::Grant
            | TokenKind::Tool
            | TokenKind::Where
            | TokenKind::And
            | TokenKind::Or
            | TokenKind::Not
            | TokenKind::Import
            | TokenKind::From
            | TokenKind::In
            | TokenKind::Break
            | TokenKind::Continue
            | TokenKind::Async
            | TokenKind::Await
            | TokenKind::Fn
            | TokenKind::Type
            | TokenKind::Pub
            | TokenKind::Const
            | TokenKind::Trait
            | TokenKind::Impl
            | TokenKind::Macro
            | TokenKind::Extern
            | TokenKind::Comptime
            | TokenKind::When
            | TokenKind::Schema
            | TokenKind::Expect
            | TokenKind::Role
            | TokenKind::Then
            | TokenKind::Step
            | TokenKind::With
            | TokenKind::Yield
            | TokenKind::Emit
            | TokenKind::Try
            | TokenKind::Null => TOKEN_TYPE_KEYWORD,

            // Type names and identifiers starting with uppercase
            TokenKind::Ident(name) if name.starts_with(char::is_uppercase) => TOKEN_TYPE_TYPE,

            // Regular identifiers (variables)
            TokenKind::Ident(_) => TOKEN_TYPE_VARIABLE,

            // String literals
            TokenKind::StringLit(_)
            | TokenKind::RawStringLit(_)
            | TokenKind::StringInterpLit(_) => TOKEN_TYPE_STRING,

            // Number literals
            TokenKind::IntLit(_) | TokenKind::FloatLit(_) => TOKEN_TYPE_NUMBER,

            // Operators
            TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Percent
            | TokenKind::Assign
            | TokenKind::Eq
            | TokenKind::NotEq
            | TokenKind::Lt
            | TokenKind::Gt
            | TokenKind::LtEq
            | TokenKind::GtEq
            | TokenKind::Bang
            | TokenKind::Question
            | TokenKind::Pipe
            | TokenKind::Ampersand
            | TokenKind::Caret
            | TokenKind::Tilde
            | TokenKind::PlusAssign
            | TokenKind::MinusAssign
            | TokenKind::StarAssign
            | TokenKind::SlashAssign => TOKEN_TYPE_OPERATOR,

            // Decorator (@)
            TokenKind::At => TOKEN_TYPE_DECORATOR,

            // Markdown blocks map to comment tokens, with multi-line handling
            TokenKind::MarkdownBlock(content) => {
                let base_line = if token.span.line > 0 {
                    (token.span.line - 1) as u32
                } else {
                    0
                };
                let base_char = token.span.start as u32;
                let lines: Vec<&str> = content.split('\n').collect();

                for (i, md_line) in lines.iter().enumerate() {
                    let line = base_line + i as u32;
                    let char_start = if i == 0 { base_char } else { 0 };
                    let length = if md_line.is_empty() {
                        1
                    } else {
                        md_line.len() as u32
                    };

                    let delta_line = line.saturating_sub(prev_line);
                    let delta_char = if delta_line == 0 && char_start >= prev_char {
                        char_start - prev_char
                    } else {
                        char_start
                    };

                    semantic_tokens.push(SemanticToken {
                        delta_line,
                        delta_start: delta_char,
                        length,
                        token_type: TOKEN_TYPE_COMMENT,
                        token_modifiers_bitset: 0,
                    });

                    prev_line = line;
                    prev_char = char_start;
                }
                continue;
            }

            // Skip other tokens (punctuation, newlines, etc.)
            _ => continue,
        };

        let line = if token.span.line > 0 {
            (token.span.line - 1) as u32
        } else {
            0
        };
        let char = token.span.start as u32;
        let length = (token.span.end - token.span.start).max(1) as u32;

        let delta_line = line.saturating_sub(prev_line);
        let delta_char = if delta_line == 0 && char >= prev_char {
            char - prev_char
        } else {
            char
        };

        semantic_tokens.push(SemanticToken {
            delta_line,
            delta_start: delta_char,
            length,
            token_type,
            token_modifiers_bitset: 0,
        });

        prev_line = line;
        prev_char = char;
    }

    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data: semantic_tokens,
    }))
}
