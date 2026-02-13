# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo build --release                    # Build all crates
cargo test --workspace                   # Run all tests (~125 total)
cargo test -p lumen-compiler             # Tests for compiler only
cargo test -p lumen-vm                   # Tests for VM only
cargo test -p lumen-runtime              # Tests for runtime only
cargo test -p lumen-compiler -- spec_suite::test_name  # Single test by name
cargo run --bin lumen -- check examples/hello.lm.md    # Type-check a file
cargo run --bin lumen -- run examples/hello.lm.md      # Compile and execute
cargo run --bin lumen -- emit examples/hello.lm.md --output out.json  # Emit LIR JSON
```

## Project Overview

Lumen is a statically typed programming language for AI-native systems. Source files are markdown (`.lm.md`) with fenced Lumen code blocks. The compiler produces LIR bytecode executed on a register-based VM.

## Workspace Layout

The Cargo workspace root is `/Cargo.toml` with members under `rust/`:

- **lumen-compiler** — Front-end pipeline: markdown extraction → lexer → parser → resolver → typechecker → constraint validation → LIR lowering
- **lumen-vm** — Register VM that executes LIR bytecode (values, string interning, type tables, process runtimes)
- **lumen-runtime** — Infrastructure: tool dispatch trait, result caching, trace event storage
- **lumen-cli** — Clap-based CLI (`main.rs`) orchestrating compiler → VM

Other key paths:
- `SPEC.md` — Implementation-accurate language specification (source of truth)
- `examples/*.lm.md` — Example programs
- `tasks.md` — Outstanding implementation work
- `docs/ARCHITECTURE.md` — Component overview
- `docs/RUNTIME.md` — Runtime semantics (futures, processes, tool dispatch, traces)

## Compiler Pipeline

Entry point: `lumen_compiler::compile(source: &str) -> Result<LirModule, CompileError>` in `rust/lumen-compiler/src/lib.rs`.

Seven sequential stages:
1. **Markdown extraction** (`markdown/extract.rs`) — Pulls code blocks and `@directives` from `.lm.md`
2. **Lexing** (`compiler/lexer.rs`) — Tokenizes concatenated code blocks
3. **Parsing** (`compiler/parser.rs`) — Produces `Program` AST (`ast.rs` defines all node types)
4. **Resolution** (`compiler/resolve.rs`) — Builds symbol table, infers effects, evaluates grant policies, emits effect provenance diagnostics
5. **Typechecking** (`compiler/typecheck.rs`) — Validates types and patterns
6. **Constraint validation** (`compiler/constraints.rs`) — Checks field `where` clauses
7. **Lowering** (`compiler/lower.rs`) — Converts AST to `LirModule` with bytecode, constants, metadata

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

## Critical Implementation Details

**Signed jump offsets**: Backward jumps require sign extension. Use `Instruction::sax(OpCode, i32)` and `sax_val() -> i32` for all Jmp/Break/Continue instructions. Never use `ax`/`ax_val` for jumps — those are unsigned and truncate negative offsets to 24 bits.

**Match statement lowering**: When emitting `Eq` for literal patterns, allocate a temp register for the boolean result (don't clobber r0). Always emit a `Test` instruction before the conditional `Jmp`.

**Type::Any propagation**: Builtin functions return `Type::Any`. In BinOp type inference, check for `Type::Any` before falling through to type-specific branches.

**Reserved words**: `result` is a keyword (for `result[T, E]`). Type names (`string`, `int`, `float`, `bool`, etc.) are handled in `parse_prefix` as identifier expressions.

**Effect provenance**: `UndeclaredEffect` errors include a `cause` field tracing the source (e.g., "call to 'fetch'", "tool call 'HttpGet'"). Effect bindings (`bind effect <name> to <tool>`) prefer custom-mapped effects over heuristic tool-name matching.

**Deterministic mode**: `@deterministic true` directive rejects nondeterministic operations (uuid, timestamp, unknown external calls) at resolve time, and defaults future scheduling to `DeferredFifo`.

**Machine transitions**: State parameter types must match transition argument types. Guards must evaluate to Bool.

**Pipeline stage arity**: Strict — exactly one data argument per stage interface. Compiler validates type flow between stages and auto-generates `run` cell if missing.

## Test Structure

- `rust/lumen-compiler/tests/spec_markdown_sweep.rs` — Compiles every code block in `SPEC.md` (auto-stubs undefined types)
- `rust/lumen-compiler/tests/spec_suite.rs` — Semantic compiler tests (compile-ok and compile-err cases)
- Unit tests inline in source files across all crates
- 12/13 examples compile; 6 run end-to-end (`role_interpolation.lm.md` has a known parse issue)

## Language Essentials

- **Cells** are functions: `cell name(params) -> ReturnType ... end`
- **Records** are typed structs with optional field constraints (`where` clauses)
- **Enums** have variants with optional payloads
- **Effects** declared on cells as effect rows: `cell foo() -> Int / {http, trace}`
- **Processes** (memory, machine, pipeline, etc.) are constructor-backed runtime objects with typed methods
- **Grants** provide capability-scoped tool access with policy constraints
- Source format is always markdown with fenced `lumen` code blocks
