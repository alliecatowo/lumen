# Lumen Project Rules

This file provides instructions to all AI agents working on this repository.

## Project Identity

Lumen is a statically typed programming language for AI-native systems. Source files can be markdown (`.lm.md`) with fenced Lumen code blocks, raw source (`.lm`), or markdown-native (`.lumen`). The compiler produces LIR bytecode executed on a register-based VM.

**Repository**: Rust workspace with 12+ crates under `rust/`
**Language**: Rust 2021 edition
**Tests**: ~5,300+ passing across all crates

## Build & Test Commands

```bash
cargo build --release                    # Build all crates
cargo test --workspace                   # Run all tests
cargo test -p lumen-compiler             # Compiler tests only
cargo test -p lumen-vm                   # VM tests only
cargo test -p lumen-runtime              # Runtime tests only
cargo test -p lumen-compiler -- spec_suite::test_name  # Single test
cargo clippy --workspace                 # Lint check
lumen check <file>                       # Type-check a Lumen source file
lumen run <file>                         # Compile and execute
lumen emit <file>                        # Emit LIR bytecode as JSON
lumen fmt <files>                        # Format Lumen source files
```

## Workspace Layout

| Crate | Purpose |
|-------|---------|
| `lumen-compiler` | 7-stage pipeline: markdown extraction -> lexer -> parser -> resolver -> typechecker -> constraints -> LIR lowering |
| `lumen-vm` | Register-based VM executing 32-bit LIR bytecode (~100 opcodes) |
| `lumen-runtime` | Tool dispatch, caching, tracing, futures, retry, crypto, HTTP, filesystem |
| `lumen-cli` | CLI commands, package manager, module resolver, auth/TUF/transparency |
| `lumen-lsp` | Language Server Protocol with semantic search |
| `lumen-codegen` | ORC JIT code generation backend |
| `lumen-wasm` | WebAssembly bindings (excluded from workspace) |
| `lumen-provider-*` | Tool providers (HTTP, JSON, FS, MCP, Gemini, Crypto, Env) |
| `lumen-tensor` | Tensor operations |

## Mandatory Rules for ALL Agents

### Git Safety
- **NEVER** use `git stash`, `git reset --hard`, `git clean`, `git checkout -- .`, or `git restore`
- **NEVER** use `git push --force` or `git rebase`
- **ONLY** use `git add`, `git commit`, `git status`, `git log`, `git diff`
- Only the Delegator agent commits code

### Code Quality
- No `unwrap()` in library code -- use `?` or explicit error handling
- No clippy warnings -- run `cargo clippy --workspace` mentally
- Write doc comments for all public functions and types
- Follow Rust 2021 edition conventions
- Use `thiserror` for error types, `serde` for serialization
- Match the style of surrounding code exactly

### Testing
- Every code change must have corresponding tests
- Tests must pass before code is committed
- Use `cargo test -p <crate>` for targeted testing, `cargo test --workspace` for full validation
- The 22 ignored tests require external services -- do not un-ignore them

### Error Reporting
- All agents must bubble up errors clearly with file paths and line numbers
- Never silently swallow errors
- If you cannot fix something, report exactly what you found and what the root cause is

## Critical Lumen-Specific Knowledge

### Compiler Pipeline (entry: `lumen_compiler::compile()` in `rust/lumen-compiler/src/lib.rs`)
1. Markdown extraction (`markdown/extract.rs`)
2. Lexing (`compiler/lexer.rs`)
3. Parsing (`compiler/parser.rs`) -> `Program` AST (`compiler/ast.rs`)
4. Resolution (`compiler/resolve.rs`) -> `SymbolTable`
5. Typechecking (`compiler/typecheck.rs`)
6. Constraint validation (`compiler/constraints.rs`)
7. Lowering (`compiler/lower.rs`) -> `LirModule` (bytecode in `compiler/lir.rs`, register allocation in `compiler/regalloc.rs`)

### LIR Instruction Encoding
- 32-bit fixed-width (Lua-style): `op`(8) | `a`(8) | `b`(8) | `c`(8)
- Alternative: `op|a|Bx` (16-bit const), `op|Ax` (24-bit jump)
- **CRITICAL**: Signed jumps use `Instruction::sax()` / `sax_val()`. NEVER use `ax`/`ax_val` for jumps (unsigned, truncates negatives)

### Lumen Language Syntax
- **Cells** = functions: `cell name(params) -> ReturnType ... end`
- **Records** use PARENTHESES for construction: `Point(x: 1, y: 2)` NOT curly braces
- **Sets** use curly braces for literals: `{1, 2, 3}`; `set[Int]` only in type position
- **Imports** use colon: `import module: symbol` NOT curly braces
- **Floor division**: `//` is integer division (comments use `#`)
- **Optional sugar**: `T?` desugars to `T | Null`
- **Pipe**: `|>` pipes values (eager); `~>` composes functions (lazy)
- **String interpolation**: `"Hello, {name}!"`
- **`result` is a reserved keyword** (for `result[T, E]`)

### VM Architecture
- Register-based with call-frame stack (max 256 depth)
- Values: scalars, `Rc<T>`-wrapped collections (COW via `Rc::make_mut()`), records, unions, closures, futures
- String interning via `StringTable`
- 83 builtins dispatched through `CallBuiltin` opcode
- Algebraic effects: `Perform`/`HandlePush`/`HandlePop`/`Resume` opcodes
- Process runtimes: memory (kv store), machine (state graphs), pipeline (auto-chaining)

### Match Lowering (frequent bug source)
- Allocate temp register for `Eq` boolean result -- NEVER clobber r0
- ALWAYS emit `Test` before conditional `Jmp`
- `Type::Any` from builtins must be checked BEFORE type-specific BinOp branches

## Key Reference Documents
- `SPEC.md` -- Language specification (source of truth)
- `docs/GRAMMAR.md` -- Formal EBNF grammar
- `CLAUDE.md` -- Extended AI agent guidance
- `docs/ARCHITECTURE.md` -- Component overview
- `docs/RUNTIME.md` -- Runtime semantics
- `ROADMAP.md` -- Project roadmap
- `docs/research/COMPETITIVE_ANALYSIS.md` -- Parity goals and gaps

## Agent Team

This project uses a specialized agent team managed by the Delegator:

| Agent | Model | Role |
|-------|-------|------|
| **delegator** | `github-copilot/claude-sonnet-4.5` | Orchestrator (primary) -- manages tasks, delegates, commits |
| **auditor** | `google/gemini-3-pro-preview` | Deep codebase auditor, planner, researcher |
| **debugger** | `github-copilot/claude-opus-4.6` | Hardcore LIR/VM/compiler debugging |
| **coder** | `github-copilot/claude-sonnet-4.5` | Feature implementation, refactoring |
| **tester** | `google/gemini-3-flash-preview` | Fast QA -- writes and runs tests |
| **task-manager** | `google/gemini-3-flash-preview` | Task list management, planning |
| **performance** | `github-copilot/claude-opus-4.6` | Optimization, architecture enforcement |
| **planner** | `google/gemini-3-pro-preview` | Strategic planning for large features |
