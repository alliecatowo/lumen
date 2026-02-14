# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo build --release                    # Build all crates
cargo test --workspace                   # Run all tests (~1090 passing, 20 ignored)
cargo test -p lumen-compiler             # Tests for compiler only
cargo test -p lumen-vm                   # Tests for VM only
cargo test -p lumen-runtime              # Tests for runtime only
cargo test -p lumen-compiler -- spec_suite::test_name  # Single test by name
```

## CLI Commands

```bash
lumen check <file>                       # Type-check a .lm or .lm.md file
lumen run <file>                         # Compile and execute (default cell: main)
lumen run <file> --cell <name>           # Run specific cell
lumen run <file> --trace-dir <dir>       # Enable trace recording
lumen emit <file>                        # Emit LIR JSON to stdout
lumen emit <file> --output <path>        # Emit LIR JSON to file
lumen repl                               # Start interactive REPL
lumen fmt <files>                        # Format source files
lumen fmt --check <files>                # Check formatting (exit 1 if changes needed)
lumen init                               # Create lumen.toml config file
lumen pkg init [name]                    # Create new package
lumen pkg build                          # Build package and dependencies
lumen pkg check                          # Type-check package
lumen trace show <run-id>                # Display trace events
lumen cache clear                        # Clear tool result cache
lumen build wasm --target <web|nodejs|wasi>  # Build WASM target (requires wasm-pack)
```

## Project Overview

Lumen is a statically typed programming language for AI-native systems. Source can be markdown (`.lm.md`) with fenced Lumen code blocks or raw source (`.lm`). The compiler produces LIR bytecode executed on a register-based VM.

**Formal Grammar**: See `docs/GRAMMAR.md` for the complete EBNF grammar specification covering all language constructs, operator precedence, and lexical rules.

## Workspace Layout

The Cargo workspace root is `/Cargo.toml` with members under `rust/`:

- **lumen-compiler** — Front-end pipeline: markdown extraction → lexer → parser → resolver → typechecker → constraint validation → LIR lowering
- **lumen-vm** — Register VM that executes LIR bytecode (values, string interning, type tables, process runtimes)
- **lumen-runtime** — Infrastructure: tool dispatch trait, result caching, trace event storage
- **lumen-cli** — Clap-based CLI (`main.rs`) orchestrating compiler → VM
- **lumen-lsp** — Language Server Protocol implementation
- **lumen-wasm** — WebAssembly bindings (excluded from workspace, built via wasm-pack)
- **lumen-provider-http** — HTTP provider for tool calls
- **lumen-provider-json** — JSON provider for tool calls
- **lumen-provider-fs** — Filesystem provider for tool calls
- **lumen-provider-mcp** — MCP (Model Context Protocol) provider bridge

Other key paths:
- `SPEC.md` — Implementation-accurate language specification (source of truth)
- `docs/GRAMMAR.md` — Formal EBNF grammar specification
- `examples/*.lm.md` — Example programs (30 total)
- `docs/research/EXECUTION_TRACKER.md` — Outstanding implementation work
- `docs/GETTING_STARTED.md` — Installation and tutorial guide
- `docs/ARCHITECTURE.md` — Component overview
- `docs/RUNTIME.md` — Runtime semantics (futures, processes, tool dispatch, traces)
- `tree-sitter-lumen/` — Tree-sitter grammar for advanced tooling (located at `tree-sitter-lumen/grammar.js`)
- `editors/vscode/` — VS Code extension with TextMate grammar

## Compiler Pipeline

Entry point: `lumen_compiler::compile(source: &str) -> Result<LirModule, CompileError>` in `rust/lumen-compiler/src/lib.rs`.

**Multi-file compilation**: Use `lumen_compiler::compile_with_imports(source, imports)` to compile with import resolution. The `imports` map provides module sources by path.

**Error formatting**: `lumen_compiler::format_error(err, source, filename)` produces human-readable diagnostics with source context and location info.

Seven sequential stages:
1. **Markdown extraction** (`markdown/extract.rs`) — Pulls code blocks and `@directives` from `.lm.md`
2. **Lexing** (`compiler/lexer.rs`) — Tokenizes concatenated code blocks
3. **Parsing** (`compiler/parser.rs`) — Produces `Program` AST (`ast.rs` defines all node types)
4. **Resolution** (`compiler/resolve.rs`) — Builds symbol table, infers effects, evaluates grant policies, emits effect provenance diagnostics
5. **Typechecking** (`compiler/typecheck.rs`) — Validates types and patterns
6. **Constraint validation** (`compiler/constraints.rs`) — Checks field `where` clauses
7. **Lowering** (`compiler/lower.rs`) — Converts AST to `LirModule` with bytecode, constants, metadata

## Module System and Imports

Lumen supports multi-file programs through a file-based module system with automatic dependency resolution.

**Import syntax:**
```lumen
import module_name: *                    # Wildcard import (all public symbols)
import module.path: Name1, Name2         # Named imports
import module.path: Name1 as Alias      # Named import with alias
```

**Module resolution** (`rust/lumen-cli/src/module_resolver.rs`):
- `import models` → searches for `models.lm.md`, then `models.lm` in the same directory
- `import utils.math` → searches for `utils/math.lm.md`, then `utils/math.lm`
- `import std.foo` → searches stdlib/ directory (if configured)
- Also checks `mod.lm.md`, `mod.lm`, `main.lm.md`, `main.lm` for directory-based modules

**Compilation pipeline with imports** (`rust/lumen-compiler/src/lib.rs`):
1. Scan source for `import` declarations
2. For each import, recursively resolve and compile the imported module to LIR
3. Extract symbols (cells, types, type aliases) from imported modules into the main symbol table
4. Typecheck the main module with imported symbols available
5. Lower the main module to LIR
6. **Merge** all imported LirModules into the main module using `LirModule::merge()`
   - Deduplicates string table entries
   - Appends cells, types, tools, policies, handlers from imported modules
   - Prevents duplicate definitions by name

**Circular import detection**: Tracks compilation stack and produces clear error messages showing the import chain.

**Error handling**:
- `ModuleNotFound` — shows module path and line number
- `CircularImport` — shows full import chain (e.g., "a → b → c → a")
- `ImportedSymbolNotFound` — specific symbol not found in target module

**CLI integration**: `lumen check` and `lumen run` automatically resolve imports via `ModuleResolver`, searching:
- Source file's directory
- Project `src/` directory (if in a Lumen project)
- Project root directory

**Examples**: See `examples/import_test/` for working multi-file examples.

## LIR and VM Architecture

LIR uses 32-bit fixed-width instructions (Lua-style encoding) defined in `compiler/lir.rs`:
- Instruction fields: `op` (8-bit opcode), `a`/`b`/`c` (8-bit registers), `Bx` (16-bit constant index), `Ax` (24-bit jump offset)
- ~100 opcodes across families: load/move, data construction, field/index access, arithmetic, comparison, control flow, intrinsics, closures, effects

The VM (`vm/vm.rs`) is a register-based interpreter with a call-frame stack (max depth 256). Runtime values include scalars, collections, records, unions, closures, futures, and trace refs.

### Process Runtimes

Process declarations compile to constructor-backed record objects with typed methods:
- **memory** — entries/kv store: append, recent, recall, get, query, store, etc.
- **machine** — typed state graph: run, start, step, is_terminal, current_state, resume_from. States have typed payloads, optional guards (Bool), and transition targets with typed args.
- **pipeline/orchestration** — run method; pipeline stages auto-chain if no explicit `run` cell

### Futures and Async

- `FutureState` enum: Pending, Completed, Error
- `FutureSchedule`: `Eager` (default) or `DeferredFifo` (default under `@deterministic true`)
- Orchestration builtins: `parallel`, `race`, `vote`, `select`, `timeout` — all execute with deterministic argument-order semantics

### Tool Policy Enforcement

Tool calls go through `validate_tool_policy()` at runtime dispatch. Merged grant policies per tool alias support constraint keys: `domain` (pattern matching), `timeout_ms`, `max_tokens`, and exact-match keys. Violations fail with tool-policy errors.

### Tool Error Types and Capability Detection

The runtime provides structured error types for robust error handling across different AI providers:

**ToolError variants** (`rust/lumen-runtime/src/tools.rs`):
- `NotFound(String)` — Tool not registered
- `InvalidArgs(String)` — Missing or malformed input arguments
- `ExecutionFailed(String)` — Generic execution error
- `RateLimit { retry_after_ms: Option<u64>, message: String }` — Provider rate limit exceeded (may include retry delay)
- `AuthError { message: String }` — Authentication or authorization failure
- `ModelNotFound { model: String, provider: String }` — Requested model not available
- `Timeout { elapsed_ms: u64, limit_ms: u64 }` — Request exceeded time limit
- `ProviderUnavailable { provider: String, reason: String }` — Provider service is down
- `OutputValidationFailed { expected_schema: String, actual: String }` — Output doesn't match declared schema
- `NotRegistered(String)` — Provider not found in registry
- `InvocationFailed(String)` — Legacy fallback error (deprecated, use ExecutionFailed)

**Provider Capabilities** (`Capability` enum):
- `TextGeneration` — Basic text generation
- `Chat` — Multi-turn conversation
- `Embedding` — Text embeddings/vectors
- `Vision` — Image input processing
- `ToolUse` — Function/tool calling
- `StructuredOutput` — JSON schema-constrained output
- `Streaming` — Streaming response support

Providers implement `capabilities()` method to advertise supported features. The registry can check capabilities before dispatching calls.

**Retry Policy** (`RetryPolicy` struct):
- `max_retries: u32` — Maximum retry attempts (default: 3)
- `base_delay_ms: u64` — Initial retry delay (default: 100ms)
- `max_delay_ms: u64` — Maximum retry delay (default: 10s)

## Critical Implementation Details

**Signed jump offsets**: Backward jumps require sign extension. Use `Instruction::sax(OpCode, i32)` and `sax_val() -> i32` for all Jmp/Break/Continue instructions. Never use `ax`/`ax_val` for jumps — those are unsigned and truncate negative offsets to 24 bits.

**Match statement lowering**: When emitting `Eq` for literal patterns, allocate a temp register for the boolean result (don't clobber r0). Always emit a `Test` instruction before the conditional `Jmp`.

**Type::Any propagation**: Builtin functions return `Type::Any`. In BinOp type inference, check for `Type::Any` before falling through to type-specific branches.

**Reserved words**: `result` is a keyword (for `result[T, E]`). Type names (`string`, `int`, `float`, `bool`, etc.) are handled in `parse_prefix` as identifier expressions.

**Record construction syntax**: Records use parentheses `RecordName(field: value, ...)` not curly braces. For example: `Point(x: 1, y: 2)`.

**Set literal syntax**: Set literals use curly braces `{1, 2, 3}` not square brackets. The syntax `set[...]` is only valid in type position (e.g., `set[Int]`).

**Import syntax**: Imports use colon separator: `import module: symbol` not curly braces `import module.{symbol}`.

**Effect provenance**: `UndeclaredEffect` errors include a `cause` field tracing the source (e.g., "call to 'fetch'", "tool call 'HttpGet'"). Effect bindings (`bind effect <name> to <tool>`) prefer custom-mapped effects over heuristic tool-name matching.

**Deterministic mode**: `@deterministic true` directive rejects nondeterministic operations (uuid, timestamp, unknown external calls) at resolve time, and defaults future scheduling to `DeferredFifo`.

**Machine transitions**: State parameter types must match transition argument types. Guards must evaluate to Bool.

**Pipeline stage arity**: Strict — exactly one data argument per stage interface. Compiler validates type flow between stages and auto-generates `run` cell if missing.

**Floor division operator**: `//` performs integer division (truncating toward negative infinity). `//=` is the compound assignment form. Not to be confused with comments (which use `#`).

**Shift operators**: `<<` (left shift) and `>>` (right shift) are bitwise shift operators. Both operands must be `Int`. The lexer produces distinct `LeftShift` and `RightShift` tokens.

**Match exhaustiveness**: The typechecker validates that match statements on enum subjects cover all variants. Missing variants produce `IncompleteMatch` errors. Wildcard `_` or catch-all identifier patterns make any match exhaustive. Guard patterns do not contribute to exhaustiveness coverage.

**Optional type sugar**: `T?` in type position desugars to `T | Null` in the parser. This applies to parameter types, return types, let bindings, and record fields.

## Tooling and Editor Support

**Tree-sitter grammar**: Located at `tree-sitter-lumen/grammar.js`. Comprehensive coverage of all language constructs for building LSPs, formatters, and analysis tools.

**VS Code extension**: Located at `editors/vscode/`. Includes TextMate grammar (`.tmLanguage.json`) for syntax highlighting. Supports `.lm` and `.lm.md` files with fenced code block recognition.

**Diagnostics**: The `lumen_compiler::diagnostics` module provides error formatting with source context. `format_error()` generates human-readable output with line numbers, column offsets, and highlighted excerpts.

**CLI architecture**: `rust/lumen-cli/` uses Clap for command parsing. Main commands in `main.rs`; REPL in `repl.rs`; formatter in `fmt.rs`; package manager in `pkg.rs`; config loading in `config.rs`.

**REPL features**: The interactive REPL (`lumen repl`) supports:
- Multi-line input with automatic detection
- Command history and navigation
- Line editing with rustyline
- Immediate execution and output display
- Access to previously defined functions and variables within the session

**Formatter**: `lumen fmt` provides code formatting with:
- Consistent indentation and spacing
- Alignment of field declarations and match arms
- Preservation of comments and documentation
- `--check` mode for CI/CD integration

**VM debug capabilities**: The VM includes:
- Instruction tracing with `--trace-dir` flag
- Stack frame capture for error diagnostics
- Future state tracking for async operations
- Tool call recording and replay via trace events

**LSP capabilities** (future): Planned support includes go-to-definition, hover documentation, completion, diagnostics, and code actions.

## Test Structure

- `rust/lumen-compiler/tests/spec_markdown_sweep.rs` — Compiles every code block in `SPEC.md` (auto-stubs undefined types)
- `rust/lumen-compiler/tests/spec_suite.rs` — Semantic compiler tests (compile-ok and compile-err cases)
- Unit tests inline in source files across all crates
- **Test counts**: ~1090 tests passing, 20 ignored (ignored tests are integration tests requiring external services: Gemini API, MCP servers, provider registry)
- All 30 examples type-check successfully; most run end-to-end

## Language Essentials

- **Cells** are functions: `cell name(params) -> ReturnType ... end`
- **Records** are typed structs with optional field constraints (`where` clauses)
- **Enums** have variants with optional payloads
- **Effects** declared on cells as effect rows: `cell foo() -> Int / {http, trace}`
- **Processes** (memory, machine, pipeline, etc.) are constructor-backed runtime objects with typed methods
- **Grants** provide capability-scoped tool access with policy constraints
- Source format supports markdown (`.lm.md`) with fenced `lumen` blocks and raw (`.lm`) files

**Syntactic Sugar**:
- **Pipe operator** `|>`: `data |> transform() |> format()` — value becomes first argument
- **String interpolation**: `"Hello, {name}!"` — embed expressions in strings
- **Range expressions**: `1..5` (exclusive), `1..=5` (inclusive) — concise iteration
- **Optional type sugar**: `T?` is shorthand for `T | Null`
- **Floor division**: `//` for integer division, `//=` for compound assignment
- **Shift operators**: `<<` and `>>` for bitwise shifts (both operands must be `Int`)
- **Bitwise operators**: `&`, `|`, `^`, `~` for AND, OR, XOR, NOT
- **Compound assignments**: `+=`, `-=`, `*=`, `/=`, `//=`, `%=`, `**=`, `&=`, `|=`, `^=`
- **Labeled loops**: `for @label ...`, `while @label ...`, `loop @label ...` with `break @label` / `continue @label`
- **For-loop filters**: `for x in items if condition` — skip iterations where condition is false
- **Type expressions**: `expr is Type` (returns `Bool`), `expr as Type` (casts value)
- **Null-safe index**: `collection?[index]` — returns `null` if collection is null
- **Variadic parameters**: `...param` syntax is parsed in cell signatures (type system wiring pending)
- **Match exhaustiveness**: compiler checks all enum variants are covered in match statements
- See `examples/syntax_sugar.lm.md` for comprehensive examples


## WebAssembly Support

Lumen compiles to WebAssembly for browser and server deployment. The `lumen-wasm` crate provides WASM bindings by compiling the entire VM to WASM using Rust's wasm-pack toolchain.

**Building for WASM**:
```bash
cd rust/lumen-wasm
wasm-pack build --target web        # Browser (ES modules)
wasm-pack build --target nodejs     # Node.js (CommonJS)
cargo build --target wasm32-wasi    # WASI (Wasmtime, etc.)
```

Or use the CLI:
```bash
lumen build wasm --target web
lumen build wasm --target nodejs
```

**API**: The WASM module exposes `check(source)`, `compile(source)`, `run(source, cell)`, and `version()` functions via wasm-bindgen. All functions return `LumenResult` with `.is_ok()`, `.is_err()`, and `.to_json()` methods.

**Examples**: See `examples/wasm_hello.lm.md` for Lumen code examples and `examples/wasm_browser.html` for interactive browser demo.

**Strategy**: See `docs/WASM_STRATEGY.md` for architecture rationale, roadmap, and deployment options. The VM-to-WASM approach leverages existing code and enables both browser (zero-latency AI inference) and server (WASI edge functions) use cases.

**Current Limitations**: No filesystem in browser (use WASI), no tool providers yet (Phase 3), no multi-file imports. See roadmap for planned enhancements.
