# Lumen — AI-Native Programming Language

Lumen is a statically typed programming language for AI-native systems. It compiles to LIR bytecode and executes on a register-based VM. Version 0.5.0, Rust 2021 edition.

## Build & Test

```bash
cargo build --release                    # Build all crates
cargo test --workspace                   # Run all tests (~5,300+ passing)
cargo test -p lumen-compiler             # Compiler tests only
cargo test -p lumen-rt                   # VM + runtime tests only
cargo test -p lumen-cli                  # CLI tests only
cargo test -p lumen-compiler -- spec_suite::test_name  # Single test
cargo clippy --workspace                 # Lint check
```

## Workspace Layout

Cargo workspace root at `/Cargo.toml`, all crates under `rust/`:

| Crate | Purpose | Key Entry Point |
|-------|---------|-----------------|
| **lumen-core** | Shared types (LIR, Values, Types) | `src/lir.rs`, `src/values.rs` |
| **lumen-compiler** | 7-stage compiler pipeline | `src/lib.rs` → `compile()` |
| **lumen-rt** | VM, scheduler, runtime services | `src/vm/mod.rs` → `run_until()` |
| **lumen-cli** | CLI, package manager, security | `src/bin/lumen.rs` |
| **lumen-lsp** | Language Server Protocol | `src/main.rs` |
| **lumen-codegen** | Cranelift JIT + WASM backend | `src/jit.rs` |
| **lumen-tensor** | Tensor ops, autodiff | `src/lib.rs` |
| **lumen-provider-*** | Tool providers (HTTP, JSON, FS, MCP, Gemini, Env, Crypto) | `src/lib.rs` |
| **lumen-wasm** | WASM bindings (excluded, via wasm-pack) | `src/lib.rs` |

## Compiler Pipeline (7 stages)

```
Source → [1.Markdown Extract] → [2.Lexer] → [3.Parser] → [4.Resolver] → [5.Typechecker] → [6.Constraints] → [7.Lowering] → LirModule
```

1. **Markdown extraction** (`markdown/extract.rs`) — pulls code from `.lm.md`/`.lumen`
2. **Lexing** (`compiler/lexer.rs`) — indentation-aware tokenizer
3. **Parsing** (`compiler/parser.rs`) — recursive descent + Pratt parsing → AST
4. **Resolution** (`compiler/resolve.rs`) — symbol table, effect inference, grants
5. **Typechecking** (`compiler/typecheck.rs`) — bidirectional inference, exhaustiveness
6. **Constraints** (`compiler/constraints.rs`) — record `where` clause validation
7. **Lowering** (`compiler/lower.rs`) — AST → 32-bit LIR bytecode

## VM Architecture

- Register-based interpreter with 32-bit fixed-width instructions (Lua-style)
- ~100 opcodes, 80+ builtins, call-frame stack (max 256 depth)
- Values: scalars inline, collections `Arc<T>` with COW
- Algebraic effects: `Perform`/`HandlePush`/`HandlePop`/`Resume` (one-shot continuations)
- Process runtimes: memory (KV), machine (state graphs), pipeline (auto-chain)
- M:N work-stealing scheduler, actors, supervisors, nurseries

## ⚠️ Critical Gotchas (READ THESE)

1. **Signed jumps**: Use `Instruction::sax()` / `sax_val()` for Jmp/Break/Continue. NEVER `ax`/`ax_val` (unsigned, truncates negative offsets silently)
2. **Match lowering**: Allocate TEMP register for `Eq` result (don't clobber r0). Always emit `Test` before conditional `Jmp`
3. **Type::Any**: Builtins return `Type::Any`. Check for it BEFORE type-specific BinOp branches
4. **`result` is a keyword**: Used for `result[T, E]`
5. **Record syntax**: Parentheses `RecordName(field: value)` NOT curly braces
6. **Set syntax**: Curly braces `{1, 2, 3}` for literals; `set[Int]` only in type position
7. **Import syntax**: Colon `import module: symbol` NOT curly braces
8. **Floor division**: `//` is integer division, NOT comments (comments use `#`)
9. **Pipe vs Compose**: `|>` is eager (passes value), `~>` is lazy (creates closure)
10. **Defer order**: LIFO (last deferred runs first)

## Lumen Syntax Quick Reference

```lumen
# Cells (functions)
cell add(a: Int, b: Int) -> Int
  return a + b
end

# Records with constraints
record User
  name: String
  age: Int where age >= 0
end
let u = User(name: "Alice", age: 30)  # parentheses!

# Enums and match
enum Color
  Red
  Green
  Blue
end
match c
  Red -> "red"
  Green -> "green"
  Blue -> "blue"
end

# Effects
effect Console
  cell log(msg: String) -> Null
end
perform Console.log("hello")

# Imports (colon separator!)
import utils: helper_fn
import models: User, Role

# Optional sugar
cell find(name: String) -> Int?   # T? = T | Null
  return null
end

# Pipe and compose
5 |> double() |> add(3)           # eager: add(double(5), 3)
let f = double ~> add_one          # lazy: creates composed closure
```

## Code Quality Rules

- No `unwrap()` in library code — use `?` or explicit error handling
- No clippy warnings
- Write doc comments for all public functions and types
- Use `thiserror` for error types, `serde` for serialization
- Every code change must have corresponding tests
- 22 ignored tests require external services — do NOT un-ignore them

## Git Safety Rules

- **NEVER** use `git stash`, `git reset --hard`, `git clean`, `git checkout -- .`, `git restore`
- **NEVER** use `git push --force` or `git rebase`
- **ONLY** use `git add`, `git commit`, `git status`, `git log`, `git diff`

## On-Demand Deep Knowledge (Skills)

For deeper context on specific areas, load these skills:

| Skill | When to Load |
|-------|-------------|
| `compiler-pipeline` | Working on any compiler stage |
| `vm-architecture` | Working on VM, values, opcodes |
| `lir-encoding` | Debugging bytecode, jump offsets, lowering |
| `lumen-syntax` | Writing or parsing Lumen code |
| `runtime-tools` | Working on tool dispatch, providers |
| `cli-commands` | Working on CLI, package manager |
| `testing-guide` | Writing or running tests |
| `security-infra` | Working on auth, TUF, transparency |

## Key Reference Documents

| Document | Purpose |
|----------|---------|
| `SPEC.md` | Language specification (source of truth) |
| `docs/GRAMMAR.md` | Formal EBNF grammar |
| `CLAUDE.md` | Extended AI agent guidance |
| `docs/ARCHITECTURE.md` | Component overview |
| `docs/RUNTIME.md` | Runtime semantics |
| `ROADMAP.md` | Project roadmap |
| `docs/research/COMPETITIVE_ANALYSIS.md` | Competitive positioning |

## Agent Team

| Agent | Model | Role |
|-------|-------|------|
| **delegator** | gemini-3-pro-preview | Orchestrator — manages tasks, delegates, commits |
| **auditor** | gemini-3-pro-preview | Deep codebase auditor, planner, researcher |
| **competitive-auditor** | gemini-3-pro-preview (temp 0.8) | Cross-language competitive analysis |
| **debugger** | claude-opus-4.6 | Hardcore LIR/VM/compiler debugging |
| **coder** | claude-sonnet-4.5 | Feature implementation, refactoring |
| **worker** | claude-haiku-4.5 | Fast general-purpose tasks |
| **tester** | gemini-3-flash-preview | QA — writes and runs tests |
| **task-manager** | gemini-3-flash-preview | Task list management |
| **performance** | claude-opus-4.6 | Optimization, architecture enforcement |
| **planner** | gemini-3-pro-preview | Strategic planning for large features |
