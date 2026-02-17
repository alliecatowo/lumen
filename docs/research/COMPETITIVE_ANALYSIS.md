# Competitive Analysis: Lumen vs. Programming Language Landscape

**Date:** February 2026  
**Scope:** Single source of truth for competitive positioning, completion status, parity goals, and implementation gaps. Merges domain-level comparison, deficit lists, leapfrog opportunities, execution backlog, 50-language matrix, completion/parity/backlog (formerly §3), and gaps-by-priority (formerly IMPLEMENTATION_GAPS). Informs **TASKS.md** and **ROADMAP.md**.

---

## 1. Executive summary

Lumen is a statically typed, markdown-native language for AI-native systems with first-class **tools**, **grants**, **algebraic effects**, and **deterministic replay**. Differentiators: (1) effect-aware agent semantics and grant policies, (2) deterministic profile and trace/replay, (3) typed process runtimes (machine, pipeline, memory).

**Path to surpass:** (1) Reach Rust/Go baseline on correctness, tooling, and packaging. (2) Add typed-contract and capability guarantees that most ecosystems treat as library-level. (3) Make deterministic replay and schema-constrained execution first-class in the compile/runtime path.

**Gaps to close:** Memory model (Rc → Arc/GC/linear), runtime performance (interpreter → AOT/JIT), concurrency (scheduler, channels, supervision), verification (where → SMT), durability (checkpoint/restore), ecosystem (registry, FFI, WASM component model), tooling (DAP, profiler, multi-error). **Lumen leads** on AI-native primitives (tools, grants, trace, @deterministic); no other language in the 50-language set has these as language primitives.

---

## 2. How to use this document

| Audience | Use |
|----------|-----|
| **TASKS.md** | Deficiency IDs D01–D18 and task IDs T001–T190 map to §6 (gap matrix) and §5 (dimension-by-dimension). When closing a deficiency, mark RESOLVED in TASKS.md. |
| **ROADMAP.md** | Strategic phases align with §5 dimensions; “Competitive context” bullets cite this doc by section. |
| **Execution / parity** | §3 (completion status, parity goals, prioritized backlog) and §11 (execution backlog) give status and file-level 2-/6-/12-week work; coordinate with TASKS.md. |
| **Gaps by priority** | §7.3 (H/M/L) distills §7 deficits for prioritization; update when closing gaps. |

---

## 3. Completion status, parity goals, and prioritized backlog

*(Content formerly in EXECUTION_TRACKER.md.)*

### 3.1 Completion status overview

**COMPLETE:** Core compiler pipeline (lexer → parser → resolver → typechecker → constraints → LIR lowering); register-based VM (74+ opcodes, 32-bit fixed-width, call-frame stack); all primitive and collection types; pattern matching with exhaustiveness; algebraic effects (perform/handle/resume, one-shot continuations); LSP (hover, completion, go-to-def, document symbols, signature help, semantic tokens, folding, diagnostics); VS Code extension (TextMate + Tree-sitter); formatter (markdown block, docstring preservation); package manager CLI (init, add, remove, install, update, publish, search, info, trust-check); module system (import resolution, circular detection, multi-file); auto-generated language reference (`lumen lang-ref`); CI/CD automation.

**IN PROGRESS:** Real cryptographic signing (Ed25519/Sigstore); registry deployment (Cloudflare Workers scaffold; full publish/search/install pending); WASM improvements (multi-file imports, tool providers); standard library (intrinsics expanded; cohesive stdlib module and docs pending).

**PLANNED:** Gradual ownership (`ref T`, `mut ref T`); self-hosting; debugger (breakpoints, stepping, inspection); profiler.

### 3.2 Repo delta (recent verified)

- VM register bounds checks and execution fuel implemented and tested.
- Parser support for `if let` / `while let` and destructuring paths with recovery tests.
- CLI exposes `test` and `ci` workflows.
- Lockfile frozen/locked install path wired.
- Intrinsic name mapping expanded in lowering.

### 3.3 Ecosystem phase status (package registry)

- **Phase 1 complete:** Path-based package baseline; lockfile v2 compatibility; deterministic `pkg pack`; `pkg publish --dry-run` local validation/checksum pipeline.
- **Phase 2 in progress:** Registry-backed `pkg search` and non-dry-run fixture publish via `LUMEN_REGISTRY_DIR`; full publish/search/install round-trip pending.

### 3.4 Parity goals (by track)

| Track | Baseline (2026-02) | Loop A target | Loop B target | Checkpoint metric |
|-------|--------------------|---------------|----------------|-------------------|
| Language | Core parsing/lowering complete; generics/traits/runtime where + defaults incomplete | Generic arity + bound validation | Trait conformance + dispatch MVP | Generic/trait suite with stable diagnostics |
| Compiler | Recovery APIs exist; compile/LSP hot path mostly fail-fast | `lumen check` uses recovery path; multiple parse errors per file | Fix-it hints for top parser errors | Malformed fixture yields ≥3 diagnostics |
| Runtime | Arithmetic/UTF-8/fuel/register hardened; TraceRef still dummy | Trace events wired to runtime store | Async tool dispatch with timeout/cancel | Replay hash stable; VM ordering/concurrency tests green |
| Tooling | CLI improved; LSP recompiles whole file per edit | LSP benchmark harness | Incremental diagnostics path | Median &lt;100ms single-line edit latency |
| Docs | Command docs behind implementation | CLI + SPEC synchronized | Doc drift gate in CI | `lumen --help` snapshot check passes |
| Ecosystem | Phase 1 complete; Phase 2 local fixture in progress | Lockfile v2 schema tested | Registry MVP with local-fixture search + publish | Publish/search/install round-trip integration test |

### 3.5 Prioritized delegable backlog

| Priority | Lane | Work item | Dependencies | Done when |
|----------|------|-----------|--------------|-----------|
| P0 | Runtime-A | VM trace wiring (TraceRef → trace store, stable IDs/hash) | — | Replay determinism test passes 10/10 reruns |
| P0 | Compiler-A | Enable recovery parser path in compiler/LSP hot paths | — | Malformed file emits ≥3 diagnostics via CLI + LSP |
| P0 | Runtime-B | Define/fix NaN + interned-string equality/ordering semantics | — | `values.rs` comparison tests cover edge cases |
| P1 | Tooling-A | LSP incremental parse/diagnostic invalidation by edit range | Compiler-A | Benchmark p50 &lt;100ms, p95 tracked |
| P1 | Docs-A | Sync CLI docs + SPEC and add drift check | — | CI fails on command/doc mismatch |
| P1 | Ecosystem-A | Registry MVP (search + publish upload path) | Lockfile v2 | Integration tests pass against local registry fixture |
| P1 | Runtime-C | Async tool dispatch trait + bounded concurrency tests | Runtime-A | Slow provider does not block independent work |
| P2 | Language-A | Generic constraint validation + trait conformance MVP | Compiler-A | Generic/trait suite green |

---

## 4. Domain matrix: what best-in-class does and how Lumen surpasses

For each domain we list what leading languages do, why it wins, Lumen’s current gap, and the surpass action. File paths point to implementation evidence.

| Domain | What best languages do | Why it wins | Lumen gap now | Surpass action |
|--------|-------------------------|-------------|---------------|----------------|
| **Systems/perf** (Rust, C++, Zig) | Ownership + memory safety (Rust), RAII (C++), explicit allocators/comptime (Zig) [1][2][3][4] | Predictable performance, explicit resources | VM/runtime safety: panic paths, unchecked arithmetic, UTF-8 slicing, register bounds (`rust/lumen-vm/src/vm.rs`, `values.rs`; §3) | VM safety contract: checked arithmetic, UTF-8-safe slicing, register bounds, no unwrap in dispatch; perf baselines and CI regression gates. TASKS: T002, T123, T017–T036. |
| **Concurrency/reliability** (Go, Erlang, Elixir) | Lightweight concurrency + context cancellation (Go), supervision/restart (Erlang/Elixir OTP) [5][6][7][8] | Resilience under partial failure | No supervision tree; async tool dispatch sync-bound (`rust/lumen-runtime/src/tools.rs`) | Supervised process groups, failure policies; async tool dispatch with timeouts and cancellation. TASKS: T053–T066, T064–T065, T151–T153. |
| **Type safety** (Rust, TS, Kotlin, Swift) | Strong types, null/error ergonomics, strict modes [1][9][10][11] | Fewer production bugs | Generics/traits incomplete (`typecheck.rs`, `resolve.rs`) | Complete generic instantiation, trait conformance, better diagnostics; strict-by-default. TASKS: T037–T052. |
| **Data/science** (Python, R, Julia) | Rich packages, multiple dispatch, domain libs [12][13][14] | Fast iteration, reuse | No data-science stdlib; limited external ingestion (PACKAGE_REGISTRY) | Typed dataframe/table, vector ops, CSV/Parquet/Arrow/HTTP with deterministic transforms. TASKS: T077–T089, T125–T133. |
| **Web/backend** (TypeScript, Go, Rust) | Server libs, async frameworks, API DX [15][16][9] | Ship APIs quickly | No first-class service scaffolding; constrained output contracts incomplete (CLI, §3) | Service package template, typed route contracts, generated schemas, replayable API fixtures. |
| **Tooling/packages** (Cargo, npm, pip, Go) | Lockfiles, workspaces, discovery, dependency semantics [17][18][19][20] | Reproducibility, growth | `lumen pkg search` stub; no registry publish/install (PACKAGE_REGISTRY, `pkg.rs`) | Registry protocol, provenance/checksum, workspace resolver, offline cache. TASKS: T090–T094, T159–T160. |
| **Build/test/lint** (Rust, Go) | Built-in workflows (cargo test/doc/clippy, go test/fmt/vet) [21][22] | Team consistency, CI | CLI parity/coverage uneven (`main.rs`, CLI.md) | Single quality-gate command: check+lint+test+doc, machine-readable, CI profiles. TASKS: T105–T110. |
| **Interop/FFI** (Rust, Go, Zig) | C interop, multi-target, componentized WASM [23][24][25][26] | Reuse ecosystems, deploy | No explicit FFI; WASM partial (main.rs, WASM_STRATEGY) | Lumen ABI, extern tool/fn boundary, WASI component target. TASKS: T095–T099. |

### 4.1 Ecosystem benchmark (incumbent bar vs Lumen)

| Area | Incumbent bar (2026) | Lumen now | Gap / close with |
|------|----------------------|-----------|-------------------|
| Package registry | npm/Cargo/pip: search, publish, install, lockfile, provenance | Path deps; lockfile v2; publish dry-run; Phase 2 local fixture | Registry MVP (T092); signing (T090); TUF/OIDC (T093–T094). §3.3. |
| Stdlib | Python/Rust/Go: collections, I/O, crypto, net, testing | Intrinsics; no cohesive stdlib module | T125–T133; graph/tensor/SIMD. §7.3. |
| Tooling | LSP + DAP + formatter + linter + CI integration | LSP, formatter, CLI; no DAP, no multi-error | T100–T104, T105–T110, T173–T176. |
| Correctness | Rust/Go: sanitizers, coverage, strict CI | Tests; no sanitizer/coverage gates | T141–T147; parity checklists T165–T168. |
| AI-native | SDKs and libs only | First-class tools, grants, trace, @deterministic | Lumen leads; T077–T089, T156–T158. |

---

## 5. Dimensions and methodology

Comparison dimensions and Lumen baseline (v0.4):

| Dimension | Measured | Lumen baseline | TASKS mapping |
|-----------|----------|----------------|---------------|
| **Memory** | Ownership, GC/RC, thread safety, cycles | Rc only; no Arc/GC; no linear types | D01–D03, T001–T016 |
| **Performance** | Interpreter vs JIT vs AOT, SIMD | Bytecode VM only | D04, T017–T036 |
| **Types** | Refinement, SMT, typestate, null safety | Static + where (runtime); T? | D05–D06, T037–T052 |
| **Concurrency** | Scheduler, channels, supervision | Single-threaded; no channels/supervision | D07–D09, T053–T066, T151–T153 |
| **Durability** | Checkpoint, replay, versioning | None | D13, T067–T076, T154–T155 |
| **Ecosystem** | Packages, FFI, WASM | Wares CLI; extern stub; WASM partial | D10–D11, T090–T099, T159–T161 |
| **AI-native** | Tools, grants, trace, constrained gen | First-class tools/grants/trace/@deterministic | Lumen leads; T077–T089, T156–T158 |
| **Tooling** | LSP, DAP, profiler, multi-error | LSP; no DAP/profiler | D14–D15, T100–T110 |
| **Metaprogramming** | Macros, reflection | None (MacroDecl unused) | D16, T118 |

---

## 6. Dimension-by-dimension: gaps and leapfrog

### 6.1 Memory

**Who leads:** Rust (ownership/borrowing, zero-cost) [1], Swift (ARC, value semantics) [11], Zig (explicit allocators) [4], C++ (RAII) [3], Nim, V, Odin.

**Lumen gaps:** Rc only (D01); no cycle collection (D02); no linear/affine (D03). **Closing:** T002 (Arc), T012–T013 (GC/TLAB), T005, T007–T009 (linear/borrow).

**Leapfrog:** Linear types + effects enable *proven* single-consumption for agent handoff without Rust-style lifetime syntax. Document in ownership spec (T006).

---

### 6.2 Performance

**Who leads:** Rust, C, C++, Zig (AOT); Go (AOT + GC) [7]; Java, C# (JIT); V8, LuaJIT; Julia [16]; Mojo; Swift.

**Lumen gaps:** Interpreter only (D04). **Closing:** T017–T036 (AOT/JIT), T125, T080 (SIMD).

**Leapfrog:** Deterministic replay + AOT gives reproducible, auditable performance; PGO persistence for agent loops.

---

### 6.3 Type system and verification

**Who leads:** Rust (traits, lifetimes); Liquid Haskell, F* (refinement types, SMT-backed; see Additional refs); TypeScript (strict null) [9]; Kotlin (null, contracts) [10]; Swift (optionals); Zig (comptime); Koka (effect typing).

**Lumen gaps:** where runtime-only (D05); no SMT refinement (D06); no typestate. **Closing:** T037–T052 (solver, refinement, typestate), T148–T150 (session types, counter-examples).

**Research note:** Refinement types (e.g. `{v:Int | v > 0}`) are checked by encoding to SMT and solving; Liquid Haskell and F* do this for Haskell and F* respectively. Lumen's `where` clauses are a natural target.

**Leapfrog:** Refinement + effects + grants: prove “no path exceeds N network calls” or “tool only called with valid schema.” Effect budgets (T048). Rare in mainstream languages.

**Error/optional ergonomics:** Rust's `?` operator unwraps `Result`/`Option` or returns early, replacing repetitive `match`/`unwrap` (Rust By Example, Try trait). Swift uses optional binding (`if let`) and optional chaining; JavaScript has `?.` for null-safe access. Lumen today uses explicit `unwrap`, `unwrap_or`, `is_ok`/`is_err`, and `match` on `result[T,E]`—heavily in tests and error-handling code. Syntactic sugar for propagation (e.g. postfix `?` in cells that return `result`/optional) or optional chaining would improve DX and align with best-in-class type-safe error handling. TASKS.md T209; ROADMAP Phase 3.

---

### 6.4 Concurrency and reliability

**Who leads:** Go (goroutines, channels, context) [5][6]; Erlang/Elixir (OTP: supervision trees, workers, behaviours gen_server/gen_statem/supervisor) [7][8]; Rust (Tokio, Send/Sync) [2]; Swift (structured concurrency); Kotlin (coroutines); Java (virtual threads).

**Research note:** Erlang OTP structures code into supervisors and workers; supervisors restart workers on failure. Behaviours (gen_server, gen_statem, supervisor) formalize patterns with callback modules. [7]

**Lumen gaps:** Single-threaded (D07); no typed channels (D08); no supervision (D09). **Closing:** T053–T066 (scheduler, channels, supervisor, link/monitor), T151–T153 (structured concurrency, select, actor interface).

**Leapfrog:** Supervision + grants: “this agent tree can only use these tools with these limits.” OTP-style resilience + capability security.

---

### 6.5 Durability and long-running workflows

**Who leads:** Temporal, Inngest (durable workflow engines; replay from event history, determinism checks) [29]; Erlang (process state, restarts); Azure Durable Functions.

**Research note:** Temporal replays workflow event histories to verify determinism; non-deterministic code fails replay. Versioning supports workflow evolution. [29]

**Lumen gaps:** No checkpoint/resume (D13). **Closing:** T067–T076 (snapshot, checkpoint, restore, replay), T154–T155 (idempotency, compression).

**Leapfrog:** Durability in the language/VM: checkpoint intrinsic, deterministic replay, workflow versioning (T075)—no separate service.

---

### 6.6 Ecosystem and interop

**Who leads:** Rust (Cargo, crates.io, bindgen, audit) [17]; npm/Go modules/pip [18][19][20]; WASM component model (WIT) [26].

**Lumen gaps:** No zero-cost FFI/bindgen (D10); WASM not WIT (D11); signing/registry stubs. **Closing:** T095–T096, T097–T099, T090–T094, T159–T160.

**Leapfrog:** Wares with grant policies and import-site sandboxing (“this dependency cannot access network”).

---

### 6.7 AI-native

**Ecosystem lead:** Python (numpy, PyTorch, LangChain); TypeScript (Vercel AI SDK).

**Lumen leads:** First-class tools and grants; @deterministic; trace and replay; type-to-grammar (T087); effect budgets (T048). No other language in the 50-language set has these as primitives.

---

### 6.8 Tooling and DX

**Who leads:** Rust (rust-analyzer, clippy, rustfmt, cargo test/doc) [21]; TypeScript; Go (gofmt, govet, pprof) [22]; Java/Kotlin (DAP); Python (Pyright, Ruff).

**Lumen gaps:** No DAP (D14); no profiler (D15); single-error reporting. **Closing:** T103–T104, T110, T105–T107.

---

## 7. Deficits: canonical list

### 7.1 Architectural / system (TASKS.md D01–D18)

| ID | Deficit | Closing tasks |
|----|---------|----------------|
| D01 | Rc only; not thread-safe | T002, T011 |
| D02 | No cycle collection | T012, T013 |
| D03 | No linear/affine types | T005, T007–T009 |
| D04 | Interpreter only | T017–T036 |
| D05 | Constraints runtime-only | T040–T044 |
| D06 | No SMT refinement | T037–T052 |
| D07 | Single-threaded scheduler | T053–T066 |
| D08 | No typed channels | T062, T063 |
| D09 | No supervision | T064, T065 |
| D10 | No zero-cost FFI/bindgen | T095, T096 |
| D11 | No WASM component model | T097–T099 |
| D12 | No first-class tensors | T077–T089 |
| D13 | No checkpoint/resume | T067–T076 |
| D14 | No DAP | T103, T104 |
| D15 | No profiler | T110 |
| D16 | No hygienic macros | T118 |
| D17 | Overflow semantics unclear | T123 |
| D18 | Register limit (255) | T124 |

### 7.2 Implementation / correctness (file-level)

These are concrete defects or incomplete features; closing them supports parity with mainstream expectations. Sources: §3 parity/backlog, implementation audit.

| # | Deficit | Primary locations |
|---|---------|-------------------|
| 1 | Generic type instantiation incomplete | `typecheck.rs`, §3 |
| 2 | Trait conformance / method dispatch incomplete | `resolve.rs`, §3 |
| 3 | Parser recovery: main file uses recovery; import path uses `parse_program` (no recovery) | `parser.rs`, `lib.rs`; T181 |
| 4 | LSP incremental parsing missing | `lumen-lsp/src/main.rs` |
| 5 | `!=` lowering (verify/fix edge cases) | `lower.rs`; tests in compiler_fixes_suite |
| 6 | Closure capture / upvalue model (verify/fix) | `lower.rs`, `vm.rs`; T189 |
| 7 | Set/map comprehension lowering (verify edge cases) | `lower.rs`; tests in compiler_fixes_suite |
| 8 | `if let` / `while let` (verify completeness) | `parser.rs`; desugar tests in parser, round17/18 |
| 9 | Arithmetic overflow / div-by-zero unsafe | `vm.rs` |
| 10 | UTF-8 string slicing can panic | `vm.rs` |
| 11 | VM register bounds and unwrap safety | `vm.rs` |
| 12 | Trace wired when `--trace-dir` set; TraceRef ↔ run/store and replay-hash stability pending | `vm.rs`, `trace/store.rs`; §3.5 P0 |
| 13 | Cache persistence incomplete on startup | `cache.rs` |
| 14 | Tool dispatch synchronous | `tools.rs` |
| 15 | Package registry/publish not implemented | PACKAGE_REGISTRY, `pkg.rs` |
| 16 | MCP bridge missing | ROADMAP, §3 |
| 17 | Intrinsic stdlib mapping incomplete | `lower.rs` |
| 18 | Runtime where / field defaults incomplete | `constraints.rs` |
| 19 | Build/test/lint/doc contract not at Rust/Go level | `main.rs` |
| 20 | FFI + WASM component story not production-ready | WASM_STRATEGY, `main.rs` |

### 7.3 Gaps by priority (H/M/L)

*(Content formerly in IMPLEMENTATION_GAPS.md. Map to §7.1–7.2 and TASKS.md for closing work.)*

| Priority | Gap | Notes |
|----------|-----|-------|
| **HIGH** | Real Ed25519/Sigstore signing | Package provenance and trust-check depend on this (T090). |
| **HIGH** | Registry API deployment | Cloudflare Workers scaffold exists; full round-trip pending (T092). |
| **HIGH** | Standard library | Intrinsics exist; cohesive stdlib module and docs (T125–T133). |
| **HIGH** | Error recovery in parser | Recovery APIs exist; CLI/LSP hot path mostly fail-fast (§3.5 P0 Compiler-A). |
| **MEDIUM** | WASM multi-file imports | Single-file only today (T097–T099). |
| **MEDIUM** | WASM tool providers | Phase 3 in WASM roadmap. |
| **MEDIUM** | Performance benchmarks | No formal benchmark harness yet (T110). |
| **MEDIUM** | Debugger support in VM | Trace events exist; no DAP (T103–T104). |
| **MEDIUM** | Code actions in LSP | Rename, extract, quick fixes (T101). |
| **LOW** | Gradual ownership system | `ref T`, `mut ref T` (T005–T009, ROADMAP). |
| **LOW** | Self-hosting compiler | Long-term (T134–T140). |
| **LOW** | Profile-guided optimization | No profiling infrastructure yet (T110). |
| **LOW** | REPL improvements | Multi-line, history search. |
| **LOW** | Cache load on startup | Runtime `CacheStore` does not load from disk on init; cache is process-scoped only (T185). |
| **LOW** | `validate` builtin | SPEC documents `validate(Any) -> Bool` as stub; Schema opcode validates at tool/output; standalone builtin does not (T186). |
| **LOW** | Source mapping in interpolation | Parser TODO: correct span mapping inside string interpolation for v2 (T188). |

---

## 8. Leapfrog opportunities (20)

1. Compile-time effect + capability proofs for external calls (rare in mainstream).
2. Deterministic replay as default CI mode for agent programs [29].
3. Typed constrained decoding: Lumen types → JSON Schema / grammar masks [27][28].
4. First-class tool contract ABI aligned with MCP schemas [27].
5. Versioned workflow/program evolution (Temporal-style patching/versioning) [29].
6. Built-in policy language with static + runtime enforcement for tool grants.
7. Schema drift detector: fail build when tool/result schemas diverge from types (T157).
8. Effect-budgeted execution (max_tokens, timeout) in type/effect checker (T048, T158).
9. Deterministic pipelines with replay hashes in traces.
10. State-machine checks for `machine`: dead-state, unreachable, invalid-transition (verification toolkit).
11. Property-based differential tests: deterministic vs non-deterministic profiles.
12. Package trust: checksum + provenance + reproducible lock [17][18][19][20] (T090–T094).
13. Language-level service contracts: OpenAPI + JSON Schema from record/cell signatures.
14. Zero-config local cloud emulator for tools/providers with deterministic fixtures.
15. Strong interop via WASI components and typed interfaces [26] (T098–T099).
16. Multi-provider adapter conformance tests in `lumen test`.
17. Docs-as-tests: all fenced Lumen snippets in docs compiled in CI.
18. IDE quick-fix for effect/type violations (add effect, grant, import) (T101).
19. AI-focused lints: unsafe prompt interpolation, unconstrained outputs, non-replayable APIs.
20. Execution graph visualizer from trace events for debugging and audit.
21. **Result/optional syntactic sugar:** propagation operator (`?`) or optional chaining to reduce `unwrap`/match boilerplate (Rust/Swift/JS-style); see §6.3, T209.

---

## 9. Language-by-language summary (50 languages)

**L** = they lead on dimension; **~** = parity/mixed; **W** = Lumen leads or can lead. Dimensions: Mem, Perf, Type, Conc, Dur, Eco, AI, Tool.

| # | Language | Mem | Perf | Type | Conc | Dur | Eco | AI | Tool | Note |
|---|----------|-----|------|------|------|-----|-----|-----|------|------|
| 1 | Rust | L | L | L | L | ~ | L | ~ | L | We add effects+grants. |
| 2 | C | L | L | ~ | ~ | ~ | L | ~ | ~ | We add safety. |
| 3 | C++ | L | L | ~ | L | ~ | L | ~ | L | We add simplicity. |
| 4 | Zig | L | L | ~ | ~ | ~ | ~ | ~ | ~ | We add effects. |
| 5 | Go | ~ | L | ~ | L | ~ | L | ~ | L | We add verification. |
| 6 | Swift | L | L | L | L | ~ | L | ~ | L | We add AI-native. |
| 7 | Python | ~ | ~ | ~ | ~ | ~ | L | L libs | L | We add types+effects+perf. |
| 8 | TypeScript | ~ | ~ | L | ~ | ~ | L | ~ | L | We add effects. |
| 9 | JavaScript | ~ | ~ | ~ | ~ | ~ | L | ~ | L | We add types+correctness. |
| 10 | Java | ~ | L | ~ | L | ~ | L | ~ | L | We add effects+AI. |
| 11 | C# | ~ | L | L | L | ~ | L | ~ | L | We add AI-native. |
| 12 | Kotlin | ~ | ~ | L | L | ~ | L | ~ | L | We add verification. |
| 13 | Scala | ~ | L | L | L | ~ | L | ~ | L | We add effects. |
| 14 | Haskell | L | ~ | L | ~ | ~ | ~ | ~ | ~ | We add imperative+AI. |
| 15 | OCaml | L | L | L | ~ | ~ | ~ | ~ | ~ | We add effects+AI. |
| 16 | F# | ~ | L | L | ~ | ~ | L | ~ | L | We add AI-native. |
| 17 | Erlang | ~ | ~ | ~ | L | L | ~ | ~ | ~ | We add types+AI. |
| 18 | Elixir | ~ | ~ | ~ | L | L | ~ | ~ | ~ | We add types. |
| 19 | Julia | ~ | L | L | L | ~ | ~ | L libs | ~ | We add verification. |
| 20 | Ruby | ~ | ~ | ~ | ~ | ~ | L | ~ | ~ | We add types. |
| 21 | PHP | ~ | ~ | ~ | ~ | ~ | L | ~ | ~ | We add types+effects. |
| 22 | Lua | ~ | ~ | ~ | ~ | ~ | ~ | ~ | ~ | We add types+AI. |
| 23 | R | ~ | ~ | ~ | ~ | ~ | L | L stats | ~ | We add general-purpose. |
| 24 | Dart | ~ | L | L | ~ | ~ | L | ~ | L | We add AI-native. |
| 25 | Nim | L | L | ~ | L | ~ | ~ | ~ | ~ | We add effects. |
| 26 | V (Vlang) | L | L | ~ | L | ~ | ~ | ~ | ~ | We add verification. |
| 27 | Crystal | ~ | L | L | L | ~ | ~ | ~ | ~ | We add AI. |
| 28 | Raku | ~ | ~ | L | ~ | ~ | ~ | ~ | ~ | We add effects. |
| 29 | Clojure | ~ | ~ | L | L | ~ | L | ~ | ~ | We add static types. |
| 30 | Koka | ~ | L | L | ~ | ~ | ~ | ~ | ~ | We add AI. |
| 31 | Idris | ~ | ~ | L | ~ | ~ | ~ | ~ | ~ | We add pragmatic. |
| 32 | Elm | ~ | ~ | L | ~ | ~ | ~ | ~ | ~ | We add backend. |
| 33 | PureScript | ~ | ~ | L | ~ | ~ | ~ | ~ | ~ | We add AI. |
| 34 | Mojo | L | L | ~ | ~ | ~ | ~ | L | ~ | We add verification. |
| 35 | Ballerina | ~ | ~ | L | L | ~ | ~ | ~ | ~ | We add AI. |
| 36 | Ada | L | L | L | L | ~ | ~ | ~ | ~ | We add AI. |
| 37 | Fortran | ~ | L | ~ | L | ~ | L | L HPC | ~ | We add general. |
| 38 | COBOL | ~ | ~ | ~ | ~ | ~ | L | ~ | ~ | We add modern. |
| 39 | MATLAB | ~ | L | ~ | ~ | ~ | L | L | ~ | We add general. |
| 40 | Perl | ~ | ~ | ~ | ~ | ~ | L | ~ | ~ | We add types. |
| 41 | Groovy | ~ | ~ | ~ | L | ~ | L | ~ | ~ | We add static. |
| 42 | Haxe | ~ | L | L | ~ | ~ | ~ | ~ | ~ | We add AI. |
| 43 | F* | ~ | ~ | L | ~ | ~ | ~ | ~ | ~ | We add pragmatic. |
| 44 | Liquid Haskell | ~ | ~ | L | ~ | ~ | ~ | ~ | ~ | We add effects. |
| 45 | Carbon | L | L | L | ~ | ~ | ~ | ~ | ~ | We add AI. |
| 46 | Odin | L | L | ~ | ~ | ~ | ~ | ~ | ~ | We add effects. |
| 47 | APL/J | ~ | L | ~ | ~ | ~ | ~ | ~ | ~ | We add types. |
| 48 | Prolog | ~ | ~ | L | ~ | ~ | ~ | ~ | ~ | We add imperative. |
| 49 | Racket | ~ | ~ | L | ~ | ~ | L | ~ | ~ | We add AI-native. |
| 50 | **Lumen** | ~ | ~ | ~ | ~ | W | ~ | **W** | ~ | Effects, grants, trace, deterministic. Roadmap: memory, perf, verification, durability. |

---

## 10. Gap matrix: dimension × deficiency

| Deficiency | Beaten by (representative) | Closing tasks |
|------------|----------------------------|---------------|
| D01 Rc only | Rust, Swift, Zig, Go, Java | T002, T011 |
| D02 No cycle collection | Go, Java, Haskell, OCaml | T012, T013 |
| D03 No linear types | Rust, Zig, Clean | T005, T007–T009 |
| D04 Interpreter only | Rust, Go, Julia, LuaJIT, Mojo | T017–T036 |
| D05 Constraints runtime | Liquid Haskell, F*, TypeScript strict | T040–T044 |
| D06 No SMT refinement | Liquid Haskell, F*, Dafny | T037–T052 |
| D07 Single-threaded | Go, Erlang, Rust, Swift | T053–T066 |
| D08 No channels | Go, Erlang, Kotlin | T062, T063 |
| D09 No supervision | Erlang, Elixir | T064, T065 |
| D10 No zero-cost FFI | Rust, Go, Zig, C++ | T095, T096 |
| D11 No WASM component | Rust wasm-bindgen, JS | T097–T099 |
| D12 No first-class tensors | Python NumPy, Julia, Mojo | T077–T089 |
| D13 No checkpoint | Temporal, Erlang | T067–T076 |
| D14 No DAP | Rust, Go, Python, Java | T103, T104 |
| D15 No profiler | Go pprof, Rust | T110 |
| D16 No macros | Rust, Lisp, Julia | T118 |
| D17 Overflow semantics | Rust, Zig, Swift | T123 |
| D18 Register limit | Most compiled | T124 |

---

## 11. Execution backlog (file-level)

Actionable work with files and exit criteria. Align with TASKS.md phases; use for 2-/6-/12-week planning.

### 11.1 2-week: core correctness and DX baseline

| Item | Files | Deliverable | Exit criteria |
|------|-------|-------------|---------------|
| VM safety hardening | `lumen-vm/src/vm.rs`, `values.rs` | Checked arithmetic/division, UTF-8-safe slicing, register bounds, no unwrap in hot path | Regression tests; zero P0 panic paths |
| Lowering/parser fixes | `lower.rs`, `parser.rs` | Fix !=, closure captures, comprehensions, if let/while let | Dedicated tests; SPEC examples pass |
| Parser recovery MVP | `parser.rs`, `compiler/mod.rs` | Recovery for delimiters/end; multi-error reporting | One malformed file → ≥3 diagnostics |
| CLI quality gate | `main.rs`, `lint.rs`, `test_cmd.rs`, `doc.rs` | Single command: check+lint+test+doc, CI mode | Machine-readable output; nonzero exit on failure |

### 11.2 6-week: parity with mainstream expectations

| Item | Files | Deliverable | Exit criteria |
|------|-------|-------------|---------------|
| Generics and traits | `typecheck.rs`, `resolve.rs`, `ast.rs` | Generic instantiation, bounds, trait dispatch | Generic/trait test suite green |
| LSP performance | `lumen-lsp`, tree-sitter | Incremental parse/typecheck; references/rename | &lt;200ms feedback on medium edit |
| Async tool dispatch | `tools.rs`, `lib.rs`, `vm.rs` | Async provider invocation, timeout/cancel | Concurrency tests |
| Trace + replay | `trace/events.rs`, `trace/store.rs`, `vm.rs`, `main.rs` | Replay command, deterministic diff | Replay CI job with baseline |
| Registry MVP | `pkg.rs`, `lockfile.rs`, PACKAGE_REGISTRY | install/search/publish vs index, checksums | Publish/install roundtrip |
| Intrinsics and constraints | `lower.rs`, `constraints.rs`, SPEC | Map intrinsics; enforce where/field defaults | No parsed-but-ignored paths |

### 11.3 12-week: leapfrog layer

| Item | Files | Deliverable | Exit criteria |
|------|-------|-------------|---------------|
| Typed constrained decoding | `typecheck.rs`, `emit.rs`, `tools.rs`, ARCHITECTURE | Types → JSON Schema/grammar for tool+LLM | Schema-constrained decode; parse error reduction |
| MCP-native tool ABI | `tools.rs`, `config.rs`, RUNTIME | MCP transport/bridge, schemas, auth | External MCP tools from Lumen with trace |
| machine/pipeline verification | `resolve.rs`, `typecheck.rs`, `trace/events.rs` | Unreachable states, unsafe transitions; runtime invariants | Verification report in `lumen check` |
| Interop/deployment | WASM_STRATEGY, `main.rs`, `lib.rs` | ABI doc, WASI component flow, artifact spec | One service via typed component boundary |
| AI-grade lint suite | `lint.rs`, `resolve.rs`, SPEC | Unconstrained outputs, nondeterminism in @deterministic, policy drift | ≥5 high-value rules with autofix |

---

## 12. References

### External [1]–[29]

[1] Rust Book: Ownership — https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html  
[2] Rust Book: Concurrency — https://doc.rust-lang.org/book/ch16-00-concurrency.html  
[3] C++ RAII — https://en.cppreference.com/w/cpp/language/raii  
[4] Zig Reference — https://ziglang.org/documentation/master/  
[5] Go Tour: Concurrency — https://go.dev/tour/concurrency/1  
[6] Go Blog: Context — https://go.dev/blog/context  
[7] Erlang/OTP Design Principles — https://www.erlang.org/docs/24/design_principles/des_princ  
[8] Elixir Supervisor — https://hexdocs.pm/elixir/Supervisor.html  
[9] TypeScript strictNullChecks — https://www.typescriptlang.org/tsconfig#strictNullChecks  
[10] Kotlin Null Safety — https://kotlinlang.org/docs/null-safety.html  
[11] Swift Documentation — https://www.swift.org/documentation/  
[12] Python Packaging — https://packaging.python.org/en/latest/tutorials/packaging-projects/  
[13] Julia Methods — https://docs.julialang.org/en/v1/manual/methods/  
[14] R Manuals — https://cran.r-project.org/doc/manuals/r-release/R-intro.html  
[15] Go net/http — https://pkg.go.dev/net/http  
[16] Julia Performance — https://docs.julialang.org/en/v1/manual/performance-tips/  
[17] Cargo Workspaces — https://doc.rust-lang.org/cargo/reference/workspaces.html  
[18] npm Workspaces + lockfile — https://docs.npmjs.com/cli/v11/using-npm/workspaces  
[19] pip + pylock — https://pip.pypa.io/en/stable/user_guide/  
[20] Go Modules — https://go.dev/ref/mod  
[21] Cargo test/rustdoc/clippy/rustfmt — https://doc.rust-lang.org/cargo/commands/  
[22] Go test/fmt/vet/doc — https://pkg.go.dev/cmd/go  
[23] Rust Nomicon FFI — https://doc.rust-lang.org/nomicon/ffi.html  
[24] Go cgo — https://pkg.go.dev/cmd/cgo  
[25] Zig C interop — https://ziglang.org/documentation/master/#C  
[26] WASM Component Model / WIT — https://component-model.bytecodealliance.org/design/wit.html  
[27] MCP Tools spec — https://modelcontextprotocol.io/specification/  
[28] OpenAI Structured Outputs — https://platform.openai.com/docs/guides/structured-outputs  
[29] Temporal Workflow determinism/versioning — https://docs.temporal.io/workflow-definition  

Additional: Liquid Haskell — https://ucsd-progsys.github.io/liquidhaskell-blog/ ; Z3 — https://github.com/Z3Prover/z3 ; LuaJIT — https://luajit.org/ ; PyPy — https://www.pypy.org/ ; Tokio — https://tokio.rs/tokio/tutorial .

### Internal

- **TASKS.md** — Deficiencies D01–D18, tasks T001–T190, protocol.  
- **ROADMAP.md** — Current status, strategic trajectory, phase goals.  
- **ARCHITECTURE.md**, **RUNTIME.md**, **CLI.md**, **PACKAGE_REGISTRY.md**, **WASM_STRATEGY.md**.  
- **rust/lumen-compiler/src/compiler/** , **rust/lumen-vm/src/** , **rust/lumen-runtime/** , **rust/lumen-cli/** , **lumen-lsp** , **tree-sitter-lumen/** .

---

## 13. Cross-reference to TASKS and ROADMAP

- **Deficiency checklist (TASKS.md §1):** D01–D18 map to §6 (who beats us), §7.1, and §10 (closing tasks). Mark RESOLVED in TASKS when done.  
- **Strategic phases (ROADMAP):** Phase 0 → D01–D03; Phase 1 → D04; Phase 2 → D05–D06; Phase 3 → D07–D09; Phase 4 → D13; Phase 5 → AI lead; Phase 6 → D10–D11; tooling → D14–D15.  
- **50-language table (§9):** Use to prioritize “beat X on dimension Y” and to order work (e.g. concurrency before memory for server agents).  
- **Completion / parity (§3):** Status, parity goals, prioritized backlog; use for sprint planning. **Execution backlog (§11):** File-level plan; sync with TASKS phase ordering and §3 parity goals.
