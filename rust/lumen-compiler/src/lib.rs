//! Lumen Compiler
//!
//! Transforms `.lm.md` source files into LIR modules.

pub mod compiler;
pub mod markdown;

use compiler::ast::Directive;
use compiler::lir::LirModule;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("lex error: {0}")]
    Lex(#[from] compiler::lexer::LexError),
    #[error("parse error: {0}")]
    Parse(#[from] compiler::parser::ParseError),
    #[error("resolve errors: {0:?}")]
    Resolve(Vec<compiler::resolve::ResolveError>),
    #[error("type errors: {0:?}")]
    Type(Vec<compiler::typecheck::TypeError>),
    #[error("constraint errors: {0:?}")]
    Constraint(Vec<compiler::constraints::ConstraintError>),
}

/// Compile a `.lm.md` source file to a LIR module.
pub fn compile(source: &str) -> Result<LirModule, CompileError> {
    // 1. Extract Markdown blocks
    let extracted = markdown::extract::extract_blocks(source);

    // 2. Build directives
    let directives: Vec<Directive> = extracted.directives.iter().map(|d| {
        Directive { name: d.name.clone(), value: d.value.clone(), span: d.span }
    }).collect();

    // 3. Concatenate all code blocks
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
        return Ok(LirModule::new("sha256:empty".to_string()));
    }

    // 4. Lex
    let mut lexer = compiler::lexer::Lexer::new(&full_code, first_block_line, first_block_offset);
    let tokens = lexer.tokenize()?;

    // 5. Parse
    let mut parser = compiler::parser::Parser::new(tokens);
    let program = parser.parse_program(directives)?;

    // 6. Resolve
    let symbols = compiler::resolve::resolve(&program).map_err(CompileError::Resolve)?;

    // 7. Typecheck
    compiler::typecheck::typecheck(&program, &symbols).map_err(CompileError::Type)?;

    // 8. Validate constraints
    compiler::constraints::validate_constraints(&program).map_err(CompileError::Constraint)?;

    // 9. Lower to LIR
    let module = compiler::lower::lower(&program, &symbols, source);

    Ok(module)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_simple() {
        let src = r#"# Test

```lumen
cell main() -> Int
  return 42
end
```
"#;
        let module = compile(src).unwrap();
        assert_eq!(module.cells.len(), 1);
        assert_eq!(module.cells[0].name, "main");
    }

    #[test]
    fn test_compile_with_record() {
        let src = r#"# Test

```lumen
record Point
  x: Int
  y: Int
end
```

```lumen
cell origin() -> Point
  return Point(x: 0, y: 0)
end
```
"#;
        let module = compile(src).unwrap();
        assert_eq!(module.types.len(), 1);
        assert_eq!(module.cells.len(), 1);
    }

    #[test]
    fn test_compile_full_example() {
        let src = r#"@lumen 1
@package "test"

# Hello World

```lumen
record Greeting
  message: String
end
```

```lumen
cell greet(name: String) -> Greeting
  let msg = "Hello, " + name
  return Greeting(message: msg)
end
```
"#;
        let module = compile(src).unwrap();
        assert_eq!(module.types.len(), 1);
        assert_eq!(module.cells.len(), 1);
        assert_eq!(module.version, "1.0.0");
    }
}
