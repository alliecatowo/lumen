# Execution Tracker (Research -> Implementation)

Date: February 14, 2026  
Owner: Track D (implementation alignment)

## Repo Delta Since Last Sync (Verified)

- [x] VM register bounds checks and execution fuel are now implemented and tested (`rust/lumen-vm/src/vm.rs`).
- [x] Parser support for `if let` / `while let` and destructuring paths is present with recovery tests (`rust/lumen-compiler/src/compiler/parser.rs`).
- [x] CLI now exposes `test` and `ci` workflows (`rust/lumen-cli/src/main.rs`).
- [x] Lockfile frozen/locked install path is wired (`rust/lumen-cli/src/main.rs`, `rust/lumen-cli/src/pkg.rs`).
- [x] Intrinsic name mapping has been expanded significantly in lowering (`rust/lumen-compiler/src/compiler/lower.rs`).

## Ecosystem Phase Status (Package Registry)

- [x] Phase 1 complete (path-based package baseline):
  lockfile v2 compatibility path is implemented/tested, deterministic `pkg pack` exists, and `pkg publish --dry-run` runs local validation/checksum pipeline.
- [~] Phase 2 in progress (registry parity on local fixture infra):
  registry-backed `pkg search` and non-dry-run local fixture publish are implemented (with `LUMEN_REGISTRY_DIR`), while full publish/search/install dependency round-trip is still pending.

## Parity Goals (Language/Compiler/Runtime/Tooling/Docs/Ecosystem)

| Track | Baseline (2026-02-14) | Loop A target (2026-03-01) | Loop B target (2026-03-15) | Checkpoint metric |
|---|---|---|---|---|
| Language | Core parsing/lowering gaps narrowed; generics/traits/runtime `where` + defaults still incomplete. | Generic arity + bound validation enforced for declared generic cells/types. | Trait conformance + dispatch MVP with regression suite. | `cargo test -p lumen-compiler` adds targeted generic/trait failures with stable diagnostics. |
| Compiler | Recovery APIs exist, but compile/LSP hot path still behaves mostly fail-fast. | `lumen check` uses recovery path and emits multiple independent parse errors per file. | First fix-it hints for top parser errors (missing `end`, unmatched delimiters, bad tuple destructure). | Malformed fixture yields `>=3` diagnostics in one run. |
| Runtime | Arithmetic/UTF-8/fuel/register hardening landed; `TraceRef` still dummy; value ordering edge cases remain. | Trace events wired to runtime store; NaN/interned-string semantics made explicit and tested. | Tool dispatch becomes async-capable with timeout/cancel propagation and non-blocking behavior. | Replay hash stable across reruns; new VM ordering/concurrency tests green. |
| Tooling | CLI command surface improved (`test`, `ci`); LSP still recompiles whole file per edit. | LSP benchmark harness added with baseline latency tracking in CI artifacts. | Incremental diagnostics path ships with median `<100ms` single-line edit latency. | Bench run on corpus reports p50/p95 and enforces threshold. |
| Docs | Command docs are behind implementation (`docs/CLI.md`, SPEC CLI section drift). | `docs/CLI.md` + SPEC CLI section synchronized to real command surface. | Doc drift gate in CI prevents command/documentation mismatch. | `lumen --help` snapshot check passes in CI. |
| Ecosystem | Phase 1 complete: lockfile/frozen + deterministic pack/dry-run publish are in place. | Lockfile v2 schema + compatibility reader are implemented and tested. | Registry MVP now includes local-fixture `pkg search` + non-dry-run upload path behind `LUMEN_REGISTRY_DIR`; install-from-registry remains. | Integration test proves publish/search/install round-trip on local registry fixture. |

## Next 2 Loops Execution Plan

### Loop A (2026-02-16 -> 2026-03-01): Determinism + Diagnostics Closure

Checkpoints:
1. By 2026-02-20: Replace dummy `TraceRef` path with real trace-store IDs/sequence wiring.
2. By 2026-02-24: Wire parser recovery into CLI compile path and LSP diagnostics path.
3. By 2026-02-27: Lock NaN/interned-string comparison semantics in `values.rs` with explicit tests.
4. By 2026-03-01: Update CLI/SPEC command docs and add a help-text snapshot test.

Exit criteria:
1. Replay run on fixed fixture returns identical output and final state hash across 10 reruns.
2. `lumen check` on malformed fixture reports `>=3` independent diagnostics.
3. `cargo test -p lumen-vm -p lumen-compiler -p lumen-cli` passes with new regression cases.

### Loop B (2026-03-02 -> 2026-03-15): Parity Lift (Tooling + Ecosystem + Runtime)

Checkpoints:
1. By 2026-03-06: Land LSP incremental invalidation path and benchmark harness.
2. By 2026-03-10: Ship async tool-dispatch contract with timeout/cancel propagation tests.
3. By 2026-03-13: Implement registry-backed `pkg search` and non-dry-run publish upload path. Status: completed on local fixture registry (`LUMEN_REGISTRY_DIR`).
4. By 2026-03-15: Lockfile v2 format migration tested (read v1 + write v2 deterministically). Status: completed early.

Exit criteria:
1. LSP diagnostics median latency `<100ms` on benchmark corpus for single-line edits.
2. One slow tool provider call does not block unrelated runnable VM work.
3. Local registry integration test passes publish/search/install workflow end-to-end.

## Prioritized Delegable Backlog (Parallel Agents)

| Priority | Agent lane | Work item | Dependencies | Done when |
|---|---|---|---|---|
| P0 | Runtime-A | VM trace wiring (`TraceRef` -> trace store, stable IDs/hash inputs). | None | Replay determinism test passes 10/10 reruns. |
| P0 | Compiler-A | Enable recovery parser path in compiler/LSP hot paths. | None | Malformed file emits `>=3` diagnostics via CLI + LSP. |
| P0 | Runtime-B | Define/fix NaN + interned-string equality/ordering semantics. | None | `values.rs` comparison tests cover NaN/interned edge cases. |
| P1 | Tooling-A | LSP incremental parse/diagnostic invalidation by edit range. | Compiler-A | Benchmark p50 `<100ms`, p95 tracked. |
| P1 | Docs-A | Sync `docs/CLI.md` + SPEC CLI list and add drift check. | None | CI fails on command/doc mismatch. |
| P1 | Ecosystem-A | Registry MVP for `pkg search` + publish upload path. | Lockfile v2 metadata shape agreed | Integration tests pass against local registry fixture. |
| P1 | Runtime-C | Async tool dispatch trait + bounded concurrency tests. | Runtime-A | Slow provider no longer blocks independent runnable work. |
| P2 | Language-A | Generic constraint validation + trait conformance MVP. | Compiler-A | Targeted generic/trait suite green with stable diagnostics. |
