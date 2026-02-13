//! LIR module serialization to canonical JSON.

use crate::compiler::lir::*;
use serde_json;

/// Emit a LIR module as canonical JSON.
pub fn emit_json(module: &LirModule) -> String {
    serde_json::to_string_pretty(module).unwrap_or_else(|e| {
        panic!("Failed to serialize LIR module: {}", e);
    })
}

/// Emit a LIR module as compact canonical JSON (for hashing).
pub fn emit_canonical_json(module: &LirModule) -> String {
    serde_json::to_string(module).unwrap_or_else(|e| {
        panic!("Failed to serialize LIR module: {}", e);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::lexer::Lexer;
    use crate::compiler::lower;
    use crate::compiler::parser::Parser;
    use crate::compiler::resolve;

    #[test]
    fn test_emit_json() {
        let src = "cell main() -> Int\n  return 42\nend";
        let mut lexer = Lexer::new(src, 1, 0);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let prog = parser.parse_program(vec![]).unwrap();
        let symbols = resolve::resolve(&prog).unwrap();
        let module = lower::lower(&prog, &symbols, src);
        let json = emit_json(&module);
        assert!(json.contains("main"));
        assert!(json.contains("1.0.0"));
        // Verify it's valid JSON
        let _: serde_json::Value = serde_json::from_str(&json).unwrap();
    }
}
