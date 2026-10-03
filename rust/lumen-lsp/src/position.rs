//! Position helpers shared by every request handler.
//!
//! LSP columns are UTF-16 code units. Rust strings are indexed by UTF-8 byte,
//! and the compiler's spans count `char`s. Mixing them up slices strings in the
//! middle of a character and panics on any non-ASCII line, so every handler
//! converts through these functions.

use lsp_types::Position;

/// Convert a UTF-16 column on `line` to a byte index into `line`.
///
/// Columns past the end of the line clamp to the end (LSP 3.17: "if the
/// character value is greater than the line length it defaults back to the
/// line length"). A column that points into the middle of a surrogate pair
/// snaps back to the start of that character.
pub fn utf16_col_to_byte(line: &str, col: u32) -> usize {
    let mut units: u32 = 0;
    for (idx, ch) in line.char_indices() {
        let next = units.saturating_add(ch.len_utf16() as u32);
        if col < next {
            return idx;
        }
        units = next;
    }
    line.len()
}

/// Convert a byte index into `line` to a UTF-16 column. The index is clamped to
/// the line and snapped back to a character boundary.
pub fn byte_to_utf16_col(line: &str, byte: usize) -> u32 {
    let mut byte = byte.min(line.len());
    while !line.is_char_boundary(byte) {
        byte -= 1;
    }
    line[..byte].chars().map(|c| c.len_utf16() as u32).sum()
}

/// Number of UTF-16 code units in `s`.
pub fn utf16_len(s: &str) -> u32 {
    s.chars().map(|c| c.len_utf16() as u32).sum()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Byte range `(start, end)` of the identifier touching UTF-16 column `col`.
pub fn word_range_in_line(line: &str, col: u32) -> Option<(usize, usize)> {
    let at = utf16_col_to_byte(line, col);

    let start = line[..at]
        .char_indices()
        .rev()
        .find(|(_, c)| !is_word_char(*c))
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);

    let end = line[at..]
        .char_indices()
        .find(|(_, c)| !is_word_char(*c))
        .map(|(i, _)| at + i)
        .unwrap_or(line.len());

    if start >= end {
        None
    } else {
        Some((start, end))
    }
}

/// The text of line `line` (without the terminator), if it exists.
pub fn line_at(text: &str, line: u32) -> Option<&str> {
    text.lines().nth(line as usize)
}

/// The identifier under (or immediately before) the cursor.
pub fn word_at(text: &str, position: Position) -> Option<String> {
    let line = line_at(text, position.line)?;
    let (start, end) = word_range_in_line(line, position.character)?;
    Some(line[start..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    #[test]
    fn ascii_columns_are_unchanged() {
        assert_eq!(utf16_col_to_byte("let x = 1", 4), 4);
        assert_eq!(byte_to_utf16_col("let x = 1", 4), 4);
    }

    #[test]
    fn bmp_chars_are_one_unit_but_several_bytes() {
        let line = "let s = \"é\" + x";
        // 'é' is 2 bytes, 1 UTF-16 unit: `x` is at byte 15 but column 14.
        assert_eq!(line.find('x').unwrap(), 15);
        assert_eq!(utf16_col_to_byte(line, 14), 15);
        assert_eq!(byte_to_utf16_col(line, 15), 14);
    }

    #[test]
    fn astral_chars_are_two_units() {
        let line = "a😀b";
        assert_eq!(utf16_col_to_byte(line, 1), 1);
        assert_eq!(utf16_col_to_byte(line, 3), 5);
        assert_eq!(byte_to_utf16_col(line, 5), 3);
        // Pointing between the surrogates snaps to the start of the emoji.
        assert_eq!(utf16_col_to_byte(line, 2), 1);
    }

    #[test]
    fn out_of_range_columns_clamp() {
        assert_eq!(utf16_col_to_byte("abc", 99), 3);
        assert_eq!(byte_to_utf16_col("é", 99), 1);
        assert_eq!(byte_to_utf16_col("é", 1), 0, "mid-char byte snaps back");
    }

    #[test]
    fn words_are_found_on_non_ascii_lines() {
        let line = "let s = \"é\" + naïve_x";
        // Cursor inside `naïve_x`.
        let col = utf16_len(&line[..line.find("naïve").unwrap()]) + 2;
        let (s, e) = word_range_in_line(line, col).unwrap();
        assert_eq!(&line[s..e], "naïve_x");
        assert_eq!(word_at(line, pos(0, col)).as_deref(), Some("naïve_x"));
        assert!(word_at(line, pos(1, 0)).is_none());
    }

    #[test]
    fn word_range_handles_cursor_at_both_edges_and_gaps() {
        assert_eq!(word_range_in_line("foo bar", 3), Some((0, 3)));
        assert_eq!(word_range_in_line("foo bar", 4), Some((4, 7)));
        assert_eq!(word_range_in_line("a  b", 2), None);
        assert_eq!(word_range_in_line("", 0), None);
    }
}

/// Helpers shared by the handler tests.
#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use lsp_types::{Range, TextEdit};
    use lumen_compiler::compiler::ast::Program;

    /// A document with non-ASCII text on the lines the handlers inspect.
    pub const UNICODE_DOC: &str = "cell greet(name: String) -> String\n  let s = \"héllo 😀 wörld\" + name\n  return s\nend\n\ncell main() -> String\n  let msg = \"é\" + greet(\"ü\")\n  return msg\nend\n";

    pub fn parse_program(source: &str) -> Option<Program> {
        let mut lexer = lumen_compiler::compiler::lexer::Lexer::new(source, 1, 0);
        let tokens = lexer.tokenize().ok()?;
        let mut parser = lumen_compiler::compiler::parser::Parser::new(tokens);
        parser.parse_program(vec![]).ok()
    }

    /// UTF-16 column where `needle` starts on `line` of `text` (plus `plus` units).
    pub fn col_of(text: &str, line: u32, needle: &str, plus: u32) -> u32 {
        let l = line_at(text, line).expect("line");
        let byte = l
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} not on line {line}"));
        byte_to_utf16_col(l, byte) + plus
    }

    /// Apply LSP text edits (UTF-16 ranges on a single document).
    pub fn apply_edits(text: &str, edits: &[TextEdit]) -> String {
        let to_byte = |text: &str, p: lsp_types::Position| -> usize {
            let mut offset = 0;
            for (i, l) in text.split_inclusive('\n').enumerate() {
                if i as u32 == p.line {
                    let content = l.trim_end_matches(['\n', '\r']);
                    return offset + utf16_col_to_byte(content, p.character);
                }
                offset += l.len();
            }
            text.len()
        };
        let mut ranges: Vec<(usize, usize, &str)> = edits
            .iter()
            .map(|e: &TextEdit| {
                let Range { start, end } = e.range;
                (
                    to_byte(text, start),
                    to_byte(text, end),
                    e.new_text.as_str(),
                )
            })
            .collect();
        ranges.sort_by_key(|r| std::cmp::Reverse(r.0));
        let mut out = text.to_string();
        for (s, e, t) in ranges {
            out.replace_range(s..e, t);
        }
        out
    }
}
