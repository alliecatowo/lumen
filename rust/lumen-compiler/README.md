# lumen-compiler

The front-end compiler for the Lumen programming language.

## Overview

`lumen-compiler` transforms Lumen source files (`.lm`, `.lm.md`, `.lumen`) into executable LIR bytecode modules. It implements a 7-stage compilation pipeline with bidirectional type inference, algebraic effect tracking, and constraint validation. The compiler is designed for fast incremental compilation with comprehensive error diagnostics.

The compiler supports three source formats: raw Lumen (`.lm`), literate markdown (`.lm.md` with fenced code blocks), and markdown-native (`.lumen` where code is default and triple-backticks are docstrings). All formats produce identical bytecode output.

Advanced features include an optional verification subsystem with SMT solver integration, active patterns, GADTs with type refinement, and hygienic macro expansion.

## Architecture

### Compilation Pipeline (7 stages)

```
Source → [Markdown] → [Lexer] → [Parser] → [Resolver] → [Typechecker] → [Constraints] → [Lowering] → LIR
```

| Stage | Module | Lines | Purpose |
|-------|--------|-------|---------|
| **1. Markdown** | `markdown/extract.rs` | ~500 | Extract code blocks and directives from `.lm.md`/`.lumen` files |
| **2. Lexer** | `compiler/lexer.rs` | ~1,800 | Indentation-aware tokenizer with INDENT/DEDENT tokens |
| **3. Parser** | `compiler/parser.rs` | ~8,100 | Recursive descent + Pratt parsing → AST |
| **4. Resolver** | `compiler/resolve.rs` | ~4,800 | Symbol table, name resolution, effect inference, grant policies |
| **5. Typechecker** | `compiler/typecheck.rs` | ~2,900 | Bidirectional type inference, match exhaustiveness |
| **6. Constraints** | `compiler/constraints.rs` | ~100 | Record `where` clause validation |
| **7. Lowering** | `compiler/lower.rs` | ~6,700 | AST → 32-bit LIR bytecode with register allocation |

### Key Modules

| Module | Purpose |
|--------|---------|
| `compiler/ast.rs` | AST node definitions (~1,000 lines) |
| `compiler/tokens.rs` | Token types, spans, source locations |
| `compiler/regalloc.rs` | Register allocation with liveness analysis |
| `compiler/ownership.rs` | Opt-in affine types (linear/borrow checking) |
| `compiler/typestate.rs` | Opt-in typestate analysis (protocol checking) |
| `compiler/session.rs` | Session type checking for concurrent protocols |
| `compiler/fixit.rs` | "Did you mean?" suggestions for typos |
| `compiler/error_codes.rs` | Stable error code registry |
| `compiler/sandbox.rs` | Capability sandboxing |
| `compiler/prompt_check.rs` | Prompt template validation |
| `compiler/gadts.rs` | GADTs with type refinement in match branches |
| `compiler/active_patterns.rs` | F#-style active patterns |
| `compiler/macros.rs` | Hygienic macro registry |
| `compiler/testing_helpers.rs` | Property-based testing utilities |
| `compiler/docs_as_tests.rs` | Documentation code block extraction |
| `verification/` | SMT solver integration, counter-examples, proof hints (~6,200 lines) |
| `diagnostics.rs` | Error formatting with source context |
| `emit.rs` | LIR JSON serialization |
| `lang_ref.rs` | Programmatic language reference |

## Key APIs

### Main Entry Points

```rust
/// Compile Lumen source to LIR bytecode (default options)
pub fn compile(source: &str) -> Result<LirModule, CompileError>

/// Compile with custom options (ownership mode, typestate, sessions)
pub fn compile_with_options(
    source: &str,
    options: CompileOptions,
) -> Result<LirModule, CompileError>

/// Compile with import resolution (multi-file)
pub fn compile_with_imports(
    source: &str,
    imports: HashMap<String, String>,
) -> Result<LirModule, CompileError>

/// Format compile error with source context
pub fn format_error(
    err: &CompileError,
    source: &str,
    filename: &str,
) -> String
```

### CompileOptions

```rust
pub struct CompileOptions {
    pub ownership_mode: OwnershipCheckMode,  // Off / Warn / Error
    pub typestate_declarations: HashMap<String, TypestateDecl>,
    pub session_protocols: HashMap<String, SessionType>,
    pub allow_unstable: bool,
    pub edition: String,  // "2026" default
}

pub enum OwnershipCheckMode {
    Off,      // Skip ownership analysis
    Warn,     // Detect violations but don't fail (default)
    Error,    // Treat violations as compile errors
}
```

## Usage

### Basic Compilation

```rust
use lumen_compiler::{compile, format_error};

let source = r#"
cell factorial(n: Int) -> Int
  if n <= 1
    return 1
  end
  return n * factorial(n - 1)
end
"#;

match compile(source) {
    Ok(module) => {
        println!("Compiled {} cells", module.cells.len());
    }
    Err(e) => {
        eprintln!("{}", format_error(&e, source, "factorial.lm"));
    }
}
```

### With Ownership Checking

```rust
use lumen_compiler::{compile_with_options, CompileOptions, OwnershipCheckMode};

let options = CompileOptions {
    ownership_mode: OwnershipCheckMode::Error,
    ..Default::default()
};

let result = compile_with_options(source, options);
```

### Multi-file Compilation

```rust
use lumen_compiler::compile_with_imports;
use std::collections::HashMap;

let mut imports = HashMap::new();
imports.insert(
    "utils".to_string(),
    std::fs::read_to_string("utils.lm")?,
);

let module = compile_with_imports(main_source, imports)?;
```

## Testing

```bash
# All compiler tests
cargo test -p lumen-compiler

# Specific test file
cargo test -p lumen-compiler -- spec_suite::test_arithmetic

# Markdown spec sweep (compiles every code block in SPEC.md)
cargo test -p lumen-compiler -- spec_markdown_sweep
```

The test suite includes:
- **5,300+ passing tests** across all compiler stages
- Spec markdown sweep (validates SPEC.md code blocks)
- Semantic test suite (compile-ok and compile-err cases)
- Unit tests for lexer, parser, resolver, typechecker
- Integration tests for multi-file imports

## Critical Implementation Details

**Floor division**: `//` is integer division (NOT comments). Comments use `#`.

**Record construction**: Parentheses `Point(x: 1, y: 2)` NOT curly braces.

**Set literals**: Curly braces `{1, 2, 3}` for values; `set[Int]` only in type position.

**Import syntax**: Colon separator `import module: symbol` NOT curly braces.

**Optional sugar**: `T?` desugars to `T | Null` in the parser.

**Signed jumps**: Jmp/Break/Continue must use `Instruction::sax()` for signed 24-bit offsets.

**Match exhaustiveness**: Compiler checks all enum variants are covered; wildcard `_` makes any match exhaustive.

## Related Crates

- **lumen-core** — LIR types, values, and instruction encoding
- **lumen-rt** — VM that executes compiled LIR
- **lumen-cli** — CLI commands that invoke the compiler
- **lumen-lsp** — LSP server that uses the compiler for diagnostics
