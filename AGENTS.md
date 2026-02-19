# Lumen — AI-Native Programming Language

Lumen is a statically typed programming language for AI-native systems. It compiles to LIR bytecode and executes on a register-based VM. Version 0.5.0, Rust 2021 edition.

## Build & Test

```bash
cargo build --release                    # Build all crates
timeout 180 cargo test --workspace       # All tests (~5,300+ passing)
timeout 60 cargo test -p lumen-compiler  # Compiler tests only
timeout 60 cargo test -p lumen-rt --test tier_parity  # Cross-tier parity (37 tests)
timeout 60 cargo test -p lumen-compiler -- spec_suite::test_name  # Single test
timeout 120 cargo check --workspace      # Type-check without building
cargo clippy --workspace                 # Lint check
```

**MANDATORY**: Always use `timeout` on test/check commands. The test harness can hang
indefinitely (OSR Cranelift panic loops, fiber stack corruption). Never run bare
`cargo test` or `cargo check` without a timeout wrapper.

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

## VM Architecture — Transcendent Architecture (v0.5+)

The VM uses the **3-tier JIT + fiber-based effect system** ("Transcendent Architecture"):

### Value Representation: NbValue (NaN-boxing)

All VM registers are `NbValue` — a 64-bit NaN-boxed value in `lumen-core/src/nb_value.rs`:

```
Tag 0 = f64 float  |  Tag 1 = i32 SMI  |  Tag 2 = bool
Tag 3 = null       |  Tag 4 = Arc<Value> heap ptr  |  Tag 5 = Arc<str>
```

SMI (small integer) fast-paths keep hot arithmetic loops **allocation-free**. The bridge
to the legacy `Value` enum is `NbValue::peek_legacy()` / `NbValue::from_legacy()` — used
only for tool dispatch, process runtimes, and effect handlers.

### 3-Tier JIT

| Tier | Trigger | Backend | Latency |
|------|---------|---------|---------|
| **Tier 0** | always | Interpreter (`vm/mod.rs`) | ~1–5 ns/op |
| **Tier 1** | call count ≥ threshold, stencil enabled | Stencil stitcher (`lumen-codegen/stitcher.rs`) | ~0 compile latency |
| **Tier 2** | call count ≥ threshold, JIT enabled | Cranelift (`lumen-codegen/jit.rs`) | ~ms compile, fastest runtime |

`OsrCheck` safepoints are inserted at loop back-edges; when a cell goes hot mid-loop, the
VM snapshot transfers to the JIT frame (OSR — On-Stack Replacement). See `vm/osr.rs`.

### Fiber-Based Effects

Effects use OS-level fiber stacks (not Rust async), implemented in `vm/fiber.rs`:

- `lm_rt_handle_push(pool, performer, eff_id, op_id)` — allocates handler fiber, wires into parent chain
- `lm_rt_perform(performer, eff_id, op_id, arg)` — walks parent chain, `fiber_switch`es to handler
- `lm_rt_resume(handler, performer, val)` — `fiber_switch`es back, returns value
- `lm_rt_handle_pop(pool, handler, performer)` — unwires from chain, returns stack to pool

`fiber_switch` is pure assembly (x86_64 and aarch64). The pool recycles stacks to avoid `mmap` per perform.

### ⚠️ Additional Gotchas for Transcendent Architecture

- **Never use `ax`/`ax_val` for jumps** — always `sax`/`sax_val` (signed 24-bit offset)
- **`lm_rt_handle_pop` takes 3 args**: `(pool, handler, performer)` — performer can be null in tests
- **`NewListStack`/`NewTupleStack`** — stack-allocated variants emitted by escape analysis in opt.rs; the VM handles them identically to `NewList`/`NewTuple` at runtime
- **JIT Tier 2 known gaps**: `Append` opcode not yet lowered in `ir.rs`; Cranelift verifier errors on some loop patterns (tracked in bench failures)
- **Effect test `parity_effect_perform_resume`** is `#[ignore]` pending fiber heap fix

### Key New Files (Transcendent Architecture)

```
rust/lumen-core/src/nb_value.rs       — NbValue type + constructors + accessors
rust/lumen-core/src/vm_context.rs     — VmContext (#[repr(C)] for stencil ABI)
rust/lumen-rt/src/vm/fiber.rs         — Fiber, FiberPool, fiber_switch, handle_push/pop
rust/lumen-rt/src/vm/fiber_effects.rs — lm_rt_perform, lm_rt_resume C-ABI wrappers
rust/lumen-rt/src/vm/osr.rs           — OSR snapshot and transfer logic
rust/lumen-rt/src/jit_tier.rs         — JitTier (Tier 2 Cranelift integration)
rust/lumen-rt/src/stencil_tier.rs     — StencilTier (Tier 1 stitcher integration)
rust/lumen-codegen/src/opt.rs         — Optimization passes (nop, escape, MIC)
rust/lumen-codegen/src/stencils.rs    — Pre-compiled C stencil templates
rust/lumen-codegen/src/stitcher.rs    — Stencil stitcher (patch + link)
rust/lumen-codegen/src/stackmap.rs    — OSR stackmap generation
rust/lumen-rt/tests/tier_parity.rs    — Cross-tier correctness tests (37 cases)
bench/results_baseline.md             — Benchmark baseline (run 2026-02-19)
```

### Legacy `Value` enum

The `Value` enum (`lumen-core/src/values.rs`) still exists as the "heap representation" used by:
- Tool dispatch (providers receive/return `serde_json::Value`)
- Process runtimes (memory, machine, pipeline)
- Complex collections (Arc-backed for COW semantics)
- Effect handler arguments

It is NOT dead code — it is the bridge between NbValue and the outside world. The conversion
bridge in `vm/mod.rs` (`reg()` / `set_reg_legacy()`) is load-bearing.

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

| Agent | Model | Effort | Role |
|-------|-------|--------|------|
| **delegator** | copilot/gpt-5.2-codex | xhigh | Orchestrator — manages tasks, delegates, commits |
| **auditor** | copilot/gpt-5.2-codex | high | Deep codebase auditor, planner, researcher |
| **competitive-auditor** | copilot/gpt-5.2-codex (temp 0.8) | high | Cross-language competitive analysis |
| **security-auditor** | copilot/gpt-5.2-codex | high | Security reviews, crypto, auth, TUF |
| **planner** | copilot/gpt-5.2-codex | high | Strategic planning for large features |
| **debugger** | copilot/gpt-5.2-codex | xhigh | Hardcore LIR/VM/compiler debugging |
| **coder** | copilot/gpt-5.2-codex | medium | Feature implementation, refactoring |
| **refactoring-specialist** | copilot/gpt-5.2-codex | high | Complex restructuring, API migrations |
| **worker** | copilot/gpt-5.2-codex | low | Fast general-purpose tasks |
| **tester** | copilot/gpt-5.2-codex | low | QA — writes and runs tests |
| **benchmark-runner** | copilot/gpt-5.2-codex | medium | Performance measurement, regression detection |
| **performance** | copilot/gpt-5.2-codex | xhigh | Optimization, architecture enforcement |
| **task-manager** | copilot/gpt-5.2-codex | low | Task list management |
| **spec-validator** | copilot/gpt-5.2-codex | high | Spec compliance, gap analysis |
| **docs-writer** | copilot/gpt-5.2-codex | medium | Documentation, examples, API refs |
