# Roadmap

This roadmap reflects implementation status as of February 2026. **Current status** (Phases 1–2.5) is below; **strategic trajectory** (systems-grade performance, verification, durability, AI-native) is in §Strategic trajectory. The granular task list and deficiency checklist live in **TASKS.md**. Competitive context (who beats us on which dimension, and how we close or leapfrog) is in **docs/research/COMPETITIVE_ANALYSIS.md**.

---

## Current status

### Phase 1: Core Language [Complete]

**Status:** Implemented and tested. Core language is production-ready.

- **Compiler:** Lexer, parser, resolver, typechecker, constraint validator, LIR lowerer. Register-based VM with 74+ opcodes (32-bit fixed-width). Multi-file compilation with import resolution and circular dependency detection.
- **Types:** Int, Float, String, Bool, Bytes, Json, Null; List, Map, Set, Tuple (Rc copy-on-write); records with where-clause constraints; enums with payloads; pattern matching with exhaustiveness; union types, `T?`, `result[T, E]`.
- **Control flow:** if/else, for, while, loop, match; break/continue with labels; try expressions.
- **Features:** String interpolation; ranges `1..5`, `1..=5`; pipe `|>`; compose `~>`; closures; import system.
- **Testing:** 30 examples; 1,365+ tests across compiler, VM, runtime.

### Phase 2: Advanced Features [Complete]

**Status:** Advanced language features and tooling implemented.

- **Language:** Algebraic effects (perform/handle/resume, one-shot continuations); `when`, `comptime`, `defer`, `extern`, `yield`.
- **Source:** Markdown-native `.lm`/`.lumen`; docstrings on declarations.
- **LSP:** Hover, completion, go-to-definition, semantic tokens, document symbols, signature help, folding, diagnostics.
- **VS Code:** TextMate + Tree-sitter grammar; format/lint on save.
- **Formatter:** `lumen fmt` with markdown/docstring preservation; `--check` for CI.
- **VM:** Rc-wrapped collections; BTreeSet for Set; module split (mod, intrinsics, processes, ops, helpers); bounds and fuel.

### Phase 2.5: Package Manager "Wares" [Mostly Complete]

**Status:** CLI and lockfile complete; real signing and registry deployment pending.

- **Manifest/lockfile:** `lumen.toml`, `lumen.lock` v4, SAT/CDCL resolver; `@namespace/name` naming.
- **CLI:** init, add, remove, install, update, publish, search, info, trust-check, policy; frozen/locked modes.
- **Security:** Sigstore-style signing (stub); trust policy; content hash in lockfile. Real Ed25519/Sigstore and transparency log not yet implemented.
- **Registry:** Cloudflare Workers scaffolded; D1 + R2 not deployed.

### Phase 3: Production Readiness [In Progress]

- Documentation: lang-ref from compiler; stdlib docs.
- Security: real crypto signing, transparency log, registry deployment.
- WASM: multi-file imports, tool providers, browser/Node/WASI.
- Performance: benchmarks, VM dispatch and compiler optimizations.
- Language: gradual ownership (`ref T`, `mut ref T`), stdlib bootstrap, self-hosting exploration.

### Phase 4: Ecosystem [Planned]

- Live registry, discovery, versioning policies.
- Tooling: rename, code actions, debugging, profiling; editors beyond VS Code.
- Playground, tutorials, AI agent SDK and tool provider ecosystem.

---

## Strategic trajectory

Long-term direction: move from interpreted scripting to **verified, durable, systems-level AI orchestration**. Gaps are enumerated in **TASKS.md** (§1 Deficiency checklist). The following phases and goals map to the task list in TASKS.md (§3).

### Memory and value representation

- **Objective:** Replace sole reliance on `Rc<T>` with thread-safe sharing and optional linear/affine types; reduce allocation and cache pressure.
- **Goals:** Arc (or equivalent) for shared VM structures; optional GC (e.g. Immix-style) or region/arena for process-local data; tagged immediates/small-value optimization; linear types for single-consumption and zero-copy handoff.
- **Tasks:** T001–T016 (value layout, Arc, GC header, move/borrow checking, arena/TLAB, string representation).
- **Competitive context:** Rust, Swift, Zig, C++, Nim, V lead on memory safety and/or zero-cost control (COMPETITIVE_ANALYSIS §3.1). Closing D01–D03 removes the “scripting language” memory profile and enables safe parallelism and zero-copy agent handoff.

### Compiler backend (AOT / JIT)

- **Objective:** Generate native code (LLVM or Cranelift) so compute-heavy and agent workloads approach C/Rust performance.
- **Goals:** Lower LIR to machine code; optional tiered execution (interpreter → baseline JIT → optimizing JIT); AOT binaries and optional PGO.
- **Tasks:** T017–T036 (codegen setup, type/control lowering, calls, TCO, JIT, benchmarks).
- **Competitive context:** Rust, C, C++, Go, Julia, LuaJIT, Mojo lead on raw or hot-path performance (COMPETITIVE_ANALYSIS §3.2). Closing D04 makes Lumen viable for high-throughput and numeric workloads; PGO persistence (roadmap) differentiates for agent loops.

### Formal verification

- **Objective:** Move `where` clauses and contracts from runtime checks to compile-time proofs where possible.
- **Goals:** Integrate SMT solver (Z3/CVC5); refinement types verified at compile time; path-sensitive refinement; optional typestate (e.g. File Open/Closed); effect budgets.
- **Tasks:** T037–T052, T148–T150 (solver, constraint lowering, verification pass, typestate, session types, counter-examples).
- **Competitive context:** Liquid Haskell, F*, TypeScript strict, Kotlin contracts lead on compile-time invariants (COMPETITIVE_ANALYSIS §3.3). Closing D05–D06 and adding effect budgets (T048) gives “if it compiles, it obeys bounds” for AI-generated code—a leapfrog over mainstream languages.

### Concurrency and scheduler

- **Objective:** True M:N parallelism and structured concurrency instead of single-threaded or ad-hoc spawning.
- **Goals:** Work-stealing scheduler with per-thread queues; typed channels; optional selective receive; supervisor/link/monitor semantics; structured concurrency (nursery/scope).
- **Tasks:** T053–T066, T151–T153 (PCB, scheduler, work-stealing, channels, supervisor, structured concurrency, select, actor interface).
- **Competitive context:** Go (goroutines/channels), Erlang/Elixir (OTP supervision), Rust (Tokio), Swift (structured concurrency) lead (COMPETITIVE_ANALYSIS §3.4). Closing D07–D09 matches Go/Erlang on concurrency and fault tolerance while layering grant-based capability security.

### Durable execution

- **Objective:** Processes that survive process death and machine restarts via checkpoint and replay.
- **Goals:** Snapshot format for stack and heap; checkpoint/restore intrinsics; durable log; deterministic replay (record nondeterminism under `@deterministic`); time-travel debugging and workflow versioning; idempotency for replayed side effects.
- **Tasks:** T067–T076, T154–T155 (snapshot, checkpoint, restore, replay, CLI, migration, idempotency keys, snapshot compression).
- **Competitive context:** Temporal, Inngest, Erlang offer durability as external services or process model (COMPETITIVE_ANALYSIS §3.5). First-class checkpoint/replay in the VM (D13) makes long-running agents and workflows a language feature rather than a separate stack.

### AI-native (tensors and differentiation)

- **Objective:** First-class tensors and automatic differentiation so agents can optimize parameters and use ML without ad-hoc FFI.
- **Goals:** Tensor type and storage; BLAS/SIMD bindings; dual numbers and tape-based reverse-mode AD; backward() intrinsic; prompt/type-to-grammar for constrained generation; schema drift detection; effect-budget runtime enforcement.
- **Tasks:** T077–T089, T156–T158 (tensor, AD, optimizers, prompt-as-code, Prob&lt;T&gt; design, schema drift, effect budgets).
- **Competitive context:** Python/Julia lead on ML libraries; Lumen already leads on tools, grants, trace, deterministic (COMPETITIVE_ANALYSIS §3.7). Adding tensors and AD closes the numeric gap; constrained generation and effect budgets extend the lead.

### Ecosystem and tooling

- **Objective:** Supply-chain security, zero-cost FFI, WASM component model, and professional tooling.
- **Goals:** Real signing (Ed25519/Sigstore), transparency log, registry deployment, OIDC/TUF; C/Rust bindgen; WASM + WIT; LSP rename/code actions and semantic search; DAP (breakpoints, stepping, inspection); multi-error reporting, fix-its, benchmarks and CI gates; optional import-site sandboxing; binary caching.
- **Tasks:** T090–T110, T159–T161 (signing, registry, FFI, WASM, LSP, DAP, CI, sandbox at import, binary cache, LSP semantic search).
- **Competitive context:** Cargo, npm, pip, Go modules, WASM WIT lead on packaging and interop (COMPETITIVE_ANALYSIS §3.6, §3.8). Closing D10–D11 and adding grant-aware sandboxing (T159) and binary caching (T160) brings parity and differentiates on security.

### Syntax and language surface

- **Objective:** Ergonomic and consistent syntax: null-safety, ranges, error propagation, checked arithmetic, macros.
- **Goals:** `?.`, `??`, `<=>`, `in`; active patterns; GADTs; hygienic macros; f-strings; trailing lambdas; `?` and try/else; checked arithmetic; virtual registers or higher register limit; multi-shot continuations (design); variadic parameters; @must_use for result.
- **Tasks:** T111–T124, T162–T164.
- **Work item (language):** **Scientific notation for float literals** — Support `1.5e10`, `2e-3` etc. in lexer/parser (see TASKS.md T191). Currently only `1e10`-style is exercised; full form is desired.
- **Competitive context:** TypeScript, Kotlin, Swift, Rust set expectations for null-safety and error handling (COMPETITIVE_ANALYSIS §3.3, §4). Matching these reduces friction for adoption; multi-shot continuations (T162) enable logic-programming-style search.

### Standard library

- **Objective:** Batteries-included std: SIMD, crypto, graph, tensor, async I/O, HTTP, JSON, and collections that work with linear types where beneficial.
- **Tasks:** T125–T133.
- **Competitive context:** Python, Go, Rust, Java have rich stdlibs (COMPETITIVE_ANALYSIS §3.6, §4). Closing “no cohesive stdlib” (COMPETITIVE_ANALYSIS §7.3) and adding graph/tensor/SIMD supports AI and data workloads.

### Self-hosting and bootstrap

- **Objective:** Compiler written in Lumen, bootstrapped by the Rust compiler; reproducible builds.
- **Tasks:** T134–T140 (lexer/parser/typecheck/lower in Lumen, bootstrap, reproducibility).
- **Competitive context:** Self-hosting is a maturity signal (e.g. Rust, Go, Swift); Lumen-in-Lumen validates the language for complex, long-lived tooling.

### Verification and release

- **Objective:** All tests pass; zero warnings; coverage and sanitizer gates; security audit; docs in sync; v1.0.0 when trajectory goals are met. Explicit parity checklists for memory, concurrency, verification, and durability.
- **Tasks:** T141–T147, T165–T168 (parity checklists).

---

## References

- **Competitive analysis:** docs/research/COMPETITIVE_ANALYSIS.md — dimensions, 50-language matrix, gap matrix, references.
- **Task list and deficiencies:** TASKS.md — D01–D18, T001–T192, protocol, rationale.
- **Competitive analysis:** docs/research/COMPETITIVE_ANALYSIS.md — completion/parity (§3), domain matrix (§4), dimensions (§5–6), deficits and gaps-by-priority (§7, §7.3), leapfrog (§8), 50-language matrix (§9), gap matrix (§10), execution backlog (§11), refs [1]–[29].

---

## Summary

- **Done:** Core language, advanced features, LSP, VS Code, formatter, Wares CLI, VM improvements (Phases 1–2.5).
- **In progress:** Production hardening (crypto, registry, WASM, performance, ownership, stdlib).
- **Planned:** Full strategic trajectory in TASKS.md: memory model, AOT/JIT, verification, scheduler, durability, tensors/AD, ecosystem, syntax, stdlib, self-hosting, release gates. Extended tasks T148–T180 add session types, structured concurrency, parity checklists, diagnostics, testing, IDE, CI, and service templates.

The granular task list (T001–T192), deficiency checklist (D01–D18), protocol, and competitive rationale are in **TASKS.md**. The 50-language comparison and gap matrix are in **docs/research/COMPETITIVE_ANALYSIS.md**.
