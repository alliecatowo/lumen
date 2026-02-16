# Execution Tracker (Research → Implementation)

Date: February 15, 2026  
Owner: Track D (implementation alignment)

## Completion Status Overview

### COMPLETE

- **Core compiler pipeline** — Lexer, parser, resolver, typechecker, constraints, LIR lowering (seven sequential stages)
- **Register-based VM** — 74+ opcodes, Lua-style 32-bit fixed-width instructions, call-frame stack
- **All primitive and collection types** — Int, Float, Bool, String, Null; List, Tuple, Set, Map, Record, Union
- **Pattern matching with exhaustiveness** — Match on enums with variant coverage validation
- **Algebraic effects** — `perform` / `handle` / `resume` with one-shot delimited continuations
- **LSP** — Hover, completion, go-to-def, document symbols, signature help, semantic tokens, folding, diagnostics
- **VS Code extension** — TextMate grammar + Tree-sitter grammar for advanced tooling
- **Formatter** — Code formatting with markdown block support, docstring preservation
- **Package manager CLI** — `init`, `add`, `remove`, `install`, `update`, `publish`, `search`, `info`, `trust-check`
- **Module system** — Import resolution, circular import detection, multi-file compilation
- **Auto-generated language reference** — `lumen lang-ref` and `lumen lang-ref --format json`
- **CI/CD automation** — Auto-release and deploy workflows

### IN PROGRESS

- **Real cryptographic signing** — Currently stub; Ed25519/Sigstore integration pending
- **Registry deployment** — Cloudflare Workers scaffolded; full publish/search/install round-trip pending
- **WASM improvements** — Browser/Node/WASI targets exist; multi-file imports and tool providers pending
- **Standard library** — Intrinsics expanded; cohesive stdlib module and documentation pending

### PLANNED

- **Gradual ownership system** — `ref T`, `mut ref T` for controlled mutation
- **Self-hosting** — Compiler written in Lumen
- **Debugger** — VM breakpoints, stepping, inspection
- **Profiler** — Execution profiling and optimization guidance

---

## Repo Delta Since Last Sync (Verified)

- [x] VM register bounds checks and execution fuel implemented and tested
- [x] Parser support for `if let` / `while let` and destructuring paths with recovery tests
- [x] CLI exposes `test` and `ci` workflows
- [x] Lockfile frozen/locked install path wired
- [x] Intrinsic name mapping expanded in lowering

## Ecosystem Phase Status (Package Registry)

- [x] **Phase 1 complete** (path-based package baseline): lockfile v2 compatibility, deterministic `pkg pack`, `pkg publish --dry-run` local validation/checksum pipeline
- [~] **Phase 2 in progress** (registry parity on local fixture infra): registry-backed `pkg search` and non-dry-run local fixture publish via `LUMEN_REGISTRY_DIR`; full publish/search/install round-trip pending

## Parity Goals (Language/Compiler/Runtime/Tooling/Docs/Ecosystem)

| Track | Baseline (2026-02-15) | Loop A target | Loop B target | Checkpoint metric |
|------|----------------------|---------------|---------------|-------------------|
| Language | Core parsing/lowering complete; generics/traits/runtime `where` + defaults still incomplete | Generic arity + bound validation | Trait conformance + dispatch MVP | Targeted generic/trait suite with stable diagnostics |
| Compiler | Recovery APIs exist; compile/LSP hot path mostly fail-fast | `lumen check` uses recovery path, multiple parse errors per file | Fix-it hints for top parser errors | Malformed fixture yields ≥3 diagnostics |
| Runtime | Arithmetic/UTF-8/fuel/register hardening; TraceRef still dummy | Trace events wired to runtime store | Async tool dispatch with timeout/cancel | Replay hash stable; new VM ordering/concurrency tests green |
| Tooling | CLI command surface improved; LSP recompiles whole file per edit | LSP benchmark harness | Incremental diagnostics path | Median &lt;100ms single-line edit latency |
| Docs | Command docs behind implementation | CLI + SPEC synchronized | Doc drift gate in CI | `lumen --help` snapshot check passes |
| Ecosystem | Phase 1 complete; Phase 2 local fixture in progress | Lockfile v2 schema tested | Registry MVP with local-fixture search + publish | Publish/search/install round-trip integration test |

## Prioritized Delegable Backlog (Parallel Agents)

| Priority | Agent lane | Work item | Dependencies | Done when |
|----------|------------|-----------|--------------|-----------|
| P0 | Runtime-A | VM trace wiring (TraceRef → trace store, stable IDs/hash) | None | Replay determinism test passes 10/10 reruns |
| P0 | Compiler-A | Enable recovery parser path in compiler/LSP hot paths | None | Malformed file emits ≥3 diagnostics via CLI + LSP |
| P0 | Runtime-B | Define/fix NaN + interned-string equality/ordering semantics | None | `values.rs` comparison tests cover edge cases |
| P1 | Tooling-A | LSP incremental parse/diagnostic invalidation by edit range | Compiler-A | Benchmark p50 &lt;100ms, p95 tracked |
| P1 | Docs-A | Sync CLI docs + SPEC and add drift check | None | CI fails on command/doc mismatch |
| P1 | Ecosystem-A | Registry MVP (search + publish upload path) | Lockfile v2 | Integration tests pass against local registry fixture |
| P1 | Runtime-C | Async tool dispatch trait + bounded concurrency tests | Runtime-A | Slow provider no longer blocks independent work |
| P2 | Language-A | Generic constraint validation + trait conformance MVP | Compiler-A | Targeted generic/trait suite green |
