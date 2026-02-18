---
name: compiler-pipeline
description: Deep reference for Lumen's 7-stage compiler pipeline - markdown extraction, lexing, parsing, resolution, typechecking, constraints, and LIR lowering
---

# Lumen Compiler Pipeline

Entry point: `lumen_compiler::compile(source)` in `rust/lumen-compiler/src/lib.rs`

## Stage 1: Markdown Extraction (`rust/lumen-compiler/src/markdown/extract.rs`)
- Extracts fenced `lumen` code blocks from `.lm.md` and `.lumen` files
- Processes `@directives` (e.g., `@strict true`, `@deterministic true`)
- Concatenates extracted blocks into raw source for lexing
- `.lm` files skip this stage (already raw source)
- ~500 lines

## Stage 2: Lexing (`rust/lumen-compiler/src/compiler/lexer.rs`)
- Indentation-aware tokenizer producing `INDENT`/`DEDENT`/`NEWLINE` tokens
- Handles string interpolation (`{expr}` inside double-quoted strings)
- Raw strings (`r"..."`), byte literals (`b"..."`), triple-quoted strings
- Token types defined in `compiler/tokens.rs` (~400 lines)
- Floor division `//` is a token, NOT a comment (comments use `#`)
- ~1,800 lines

## Stage 3: Parsing (`rust/lumen-compiler/src/compiler/parser.rs`)
- Recursive descent with Pratt parsing for expressions (precedence climbing)
- Produces `Program` AST (node types in `compiler/ast.rs`, ~1,000 lines)
- Error recovery via `synchronize()` and `synchronize_stmt()`
- Supports multiple language editions
- Key parse methods: `parse_program()`, `parse_declaration()`, `parse_statement()`, `parse_expr(min_bp)`
- Desugarings: `if let` → match, `while let` → loop+match, `T?` → `T | Null`
- ~8,100 lines (largest module)

## Stage 4: Resolution (`rust/lumen-compiler/src/compiler/resolve.rs`)
- Builds `SymbolTable` with all declarations (cells, records, enums, types, tools, effects, processes, agents)
- Infers effect rows for cells that omit explicit declarations
- Evaluates grant policies and validates tool bindings
- Emits effect provenance diagnostics with cause chains
- Handles import resolution and circular import detection
- Feature flags and maturity levels (Stable, Unstable, Experimental)
- ~4,800 lines

## Stage 5: Typechecking (`rust/lumen-compiler/src/compiler/typecheck.rs`)
- Bidirectional type inference
- Validates function calls, operator usage, pattern matching
- Match exhaustiveness checking (all enum variants must be covered)
- Contains the full list of 80+ built-in function signatures
- `Type::Any` propagation: builtins return `Type::Any`, MUST check before type-specific BinOp branches
- ~2,900 lines

## Stage 6: Constraint Validation (`rust/lumen-compiler/src/compiler/constraints.rs`)
- Validates `where` clauses on record fields
- Only allows deterministic expressions (comparisons, logical ops)
- ~100 lines

## Stage 7: Lowering (`rust/lumen-compiler/src/compiler/lower.rs`)
- Converts AST to `LirModule` with bytecode, constants, and metadata
- Register allocation in `compiler/regalloc.rs` (~400 lines, up to 65,536 registers per cell)
- CRITICAL: Signed jumps use `Instruction::sax()`/`sax_val()`, NEVER `ax`/`ax_val` (unsigned, truncates negatives)
- Match lowering: allocate temp register for Eq result (don't clobber r0), always emit Test before conditional Jmp
- ~6,700 lines (second largest module)

## Optional Analysis Passes (between stages 6-7)
- **Ownership checking** (`compiler/ownership.rs`, ~1,600 lines): Opt-in affine type system, tracks moves/borrows
- **Typestate analysis** (`compiler/typestate.rs`, ~1,400 lines): Validates state transitions
- **Session types** (`compiler/session.rs`, ~1,000 lines): Verifies communication protocols
- Enabled via `CompileOptions` (OwnershipCheckMode: Off/Warn/Error)

## Multi-File Compilation
- `compile_with_imports(source, imports)` for import resolution
- `LirModule::merge()` deduplicates string tables, prevents duplicate definitions
- Circular import detection with full chain reporting

## Key Entry Points
```rust
compile(source: &str) -> Result<LirModule, CompileError>           // markdown pipeline
compile_raw(source: &str) -> Result<LirModule, CompileError>       // raw .lm pipeline
compile_with_imports(source, imports) -> Result<LirModule, ...>    // multi-file
compile_with_options(source, CompileOptions) -> Result<LirModule>  // full options
format_error(err, source, filename) -> String                      // human-readable diagnostics
```
