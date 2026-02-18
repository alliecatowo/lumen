# Lumen Compiler Crate

This is the front-end compiler for the Lumen programming language.

## Quick Reference
- **Entry point**: `lib.rs` → `compile()`, `compile_with_options()`
- **Pipeline**: markdown extraction → lexer → parser → resolver → typechecker → constraints → lowering
- **Test command**: `cargo test -p lumen-compiler`
- **Largest modules**: `parser.rs` (8.1k lines), `lower.rs` (6.7k lines), `resolve.rs` (4.8k lines)

## Critical Rules
- `//` is floor division, NOT comments (comments use `#`)
- Records use parentheses for construction: `Point(x: 1, y: 2)`
- Sets use curly braces for literals: `{1, 2, 3}`
- Imports use colon: `import module: symbol`
- `T?` desugars to `T | Null`
- `result` is a reserved keyword
- Match exhaustiveness: all enum variants must be covered

## Module Map
| File | Lines | Purpose |
|------|-------|---------|
| `compiler/parser.rs` | ~8,100 | Recursive descent + Pratt parsing |
| `compiler/lower.rs` | ~6,700 | AST → LIR bytecode |
| `compiler/resolve.rs` | ~4,800 | Name resolution, symbol table, effects |
| `compiler/typecheck.rs` | ~2,900 | Bidirectional type inference |
| `compiler/lexer.rs` | ~1,800 | Indentation-aware tokenizer |
| `compiler/ownership.rs` | ~1,600 | Opt-in affine types |
| `compiler/typestate.rs` | ~1,400 | Opt-in typestate analysis |
| `compiler/ast.rs` | ~1,000 | AST node definitions |
| `compiler/session.rs` | ~1,000 | Session type checking |
| `compiler/fixit.rs` | ~660 | "Did you mean?" suggestions |
| `compiler/testing_helpers.rs` | ~760 | Property-based testing utils |
| `compiler/error_codes.rs` | ~480 | Stable error code registry |
| `compiler/sandbox.rs` | ~500 | Capability sandboxing |
| `compiler/prompt_check.rs` | ~450 | Prompt template validation |
| `compiler/regalloc.rs` | ~400 | Register allocation |
| `compiler/tokens.rs` | ~400 | Token types and spans |
| `compiler/docs_as_tests.rs` | ~370 | Doc code block extraction |
| `compiler/gadts.rs` | ~350 | GADTs with type refinement |
| `compiler/active_patterns.rs` | ~300 | F#-style active patterns |
| `compiler/constraints.rs` | ~100 | Record where-clause validation |
| `compiler/emit.rs` | ~50 | LIR JSON serialization |
| `markdown/extract.rs` | ~500 | Markdown code block extraction |
| `verification/` | ~6,200 | SMT solver, counter-examples, proof hints |
