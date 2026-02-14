# Lumen Language Leapfrog Plan

Date: February 14, 2026  
Owner: Research Agent C

## Scope and constraints

This plan targets a leap in language quality without speculative rewrites. It prioritizes implementation-feasible changes in the current architecture:

- compiler pipeline (`lexer -> parser -> resolver -> typechecker -> lowering`)
- register VM runtime
- provider-based tool/runtime model
- existing CLI/LSP/test infrastructure

Out of scope for this plan window: borrow checking, full dependent types, whole-program optimization passes, and native-code backend work.

## Design stance

1. Prefer battle-tested semantics over novelty when they directly reduce production risk.
2. Introduce one new semantic axis per milestone (effects, determinism, module boundaries, etc.) to keep regressions local.
3. Every proposal must include acceptance tests runnable in CI.

## Baseline snapshot (from repo state)

- Effects are parsed and typechecked at a basic level, but handler/runtime semantics are not fully realized.
- Generics/traits remain incomplete in verification and dispatch.
- Async exists (`Future`, `await`, spawn forms), but provider dispatch remains synchronous and lifecycle guarantees are limited.
- VM safety floor improved (checked arithmetic/UTF-8/fuel/register bounds), but replay wiring is still incomplete (`TraceRef` path).
- Parser recovery primitives exist, but compile/LSP hot paths still need full multi-error integration and incremental execution.
- Tooling surface expanded (`test`, `ci`, `pkg --frozen/--locked`), while docs parity and registry-backed package workflows remain behind.

## Immediate maturity-loop addendum (next 2 loops)

This addendum is the execution bridge from research to delivery for the next four weeks.  
Detailed task tracking lives in `docs/research/EXECUTION_TRACKER.md`.

### Parity targets by domain

| Domain | Current state | Loop A (by 2026-03-01) | Loop B (by 2026-03-15) |
|---|---|---|---|
| Language | Core parsing/lowering fixes landed; generic/trait/runtime-constraint gaps remain. | Generic argument + bound validation baseline. | Trait conformance/dispatch MVP. |
| Compiler | Recovery APIs present but not consistently surfaced in normal compile path. | CLI/LSP emit multiple parse errors per malformed file. | Add first fix-it diagnostics for frequent syntax failures. |
| Runtime | Deterministic guardrails improved; trace linkage + async dispatch still missing. | Trace-store wiring + explicit value ordering semantics (NaN/interned strings). | Async tool dispatch with timeout/cancel propagation. |
| Tooling | `test` and `ci` commands exist; LSP still recompiles whole file per edit. | Benchmark harness and baseline latency thresholds. | Incremental LSP diagnostics with `<100ms` median target. |
| Docs | CLI/SPEC command docs drift from implementation. | Sync command docs to current CLI surface. | CI drift gate for command/documentation parity. |
| Ecosystem | Lockfile/frozen works for path-based flows; registry search/upload still stubbed. | Lockfile v2 schema + migration tests. | Local-registry publish/search/install MVP path. |

### Loop A (2026-02-16 -> 2026-03-01)

1. Determinism closure: wire VM trace references/events into runtime trace store and replay hash path.
2. Diagnostics closure: route compiler/LSP through recovery path for multi-error reporting.
3. Runtime semantics closure: fix/define NaN and interned-string ordering behavior.
4. Docs closure: synchronize CLI/SPEC command references and add automated parity check.

Loop A measurable exit:

1. Replay fixture hash/output matches across 10 reruns.
2. Malformed fixture reports `>=3` independent diagnostics through `lumen check`.
3. Regression suites pass in `lumen-vm`, `lumen-compiler`, and `lumen-cli`.

### Loop B (2026-03-02 -> 2026-03-15)

1. Tooling parity: LSP incremental invalidation and semantic cache path.
2. Runtime parity: async tool dispatch contract with non-blocking concurrency behavior.
3. Ecosystem parity: registry-backed `pkg search` and publish upload path (non-dry-run) on local fixture infra.
4. Packaging parity: lockfile v2 read/write compatibility + deterministic serialization guarantees.

Loop B measurable exit:

1. LSP benchmark median diagnostics latency `<100ms` for single-line edits.
2. Concurrency test proves slow tool call does not block unrelated runnable work.
3. Publish/search/install round-trip passes against local registry fixture.

### Prioritized delegable backlog (parallel lanes)

| Priority | Lane | Deliverable |
|---|---|---|
| P0 | Runtime-A | Trace-store integration + replay determinism checks |
| P0 | Compiler-A | Recovery parser integration into CLI/LSP paths |
| P0 | Runtime-B | NaN/interned-string comparison semantics + tests |
| P1 | Tooling-A | Incremental LSP diagnostics pipeline |
| P1 | Docs-A | CLI/SPEC parity sync + CI drift gate |
| P1 | Ecosystem-A | Registry MVP (`search`, publish upload) + lockfile v2 migration |
| P1 | Runtime-C | Async tool dispatch contract + timeout/cancel tests |
| P2 | Language-A | Generic constraints + trait conformance MVP |

## Theme-by-theme leapfrog plan

### 1) Type and effect systems

### Adopt now

- **Row-normalized effect checking with capability binding as a first-class constraint.**
  - Keep Lumen's explicit `bind effect ... to tool` model.
  - Add row normalization/canonicalization so effect comparison is order-insensitive and stable in diagnostics.
  - Enforce call compatibility as `callee_effects ⊆ caller_effect_budget`.
- **Incremental generics implementation, trait checks second.**
  - Stage 1: monomorphic instantiation and generic argument validation.
  - Stage 2: trait conformance checking and method dispatch coherence.
- **Constraint enforcement split:**
  - Compile-time validation for statically known `where` predicates.
  - Runtime enforcement at record construction for dynamic values.

### Adopt later

- Effect polymorphism with inferred effect variables on generic cells.
- Algebraic effect handlers with resumable continuations.

### Why this is the right leap

- Row-polymorphic effects are proven practical in Koka and map directly to Lumen's effect-row syntax and goals.
- OCaml 5 shows effect handlers are powerful but difficult to stabilize; keeping full handler semantics out of early milestones avoids overreach.

### 2) Module and package semantics

### Adopt now

- **Deterministic package resolution profile:** lockfile-required in CI, explicit update command semantics.
- **Major-version import boundaries:** incompatible major versions must have distinct import paths/IDs.
- **Package API boundary checks:** prevent private symbol leaks across package boundaries.
- **Reproducibility mode in CLI:** `--locked` behavior mirroring battle-tested package managers.

### Adopt later

- Content-addressed package cache with signature verification/provenance policy.

### Why this is the right leap

- Cargo lockfile behavior and Go module semantics demonstrate practical, scalable dependency determinism and version isolation.

### 3) Error handling model

### Adopt now

- **Single failable abstraction in surface language:** `result[Ok, Err]` + propagation operator.
- **Typed error taxonomy for VM/runtime faults:**
  - deterministic runtime errors (`division_by_zero`, bounds, bad_utf8_slice, fuel_exhausted)
  - tool/provider errors
  - policy/capability denials
- **Error context chaining in traces and diagnostics** (source span + call chain + runtime frame).

### Adopt later

- Pattern-based exhaustiveness checks over domain-specific error enums.

### Why this is the right leap

- Rust and Zig both show explicit failable types scale better than hidden exceptions for systems-level reliability.

### 4) Async model

### Adopt now

- **Structured concurrency profile over current futures runtime:**
  - parent scope owns child tasks
  - scope exit waits/cancels children
  - cancellation propagation is explicit and testable
- **Deterministic scheduler mode for language-level async tests and replay.**

### Adopt later

- Actor isolation for mutable shared state.

### Why this is the right leap

- Swift/Kotlin structured concurrency demonstrates practical cancellation/lifecycle correctness without forcing actor adoption day one.

### 5) Determinism and replay

### Adopt now

- **Deterministic profile directive as enforceable contract** (not advisory):
  - ban non-deterministic intrinsics unless routed through effect/tool boundaries with recorded outputs
  - require deterministic time/random APIs under profile
- **Trace-store integration in VM execution path** with stable event IDs and replay checkpoints.
- **Replay test harness**: run once, capture trace, replay bytecode+trace, assert identical observable outputs.

### Adopt later

- Workflow evolution/version gates (patch markers) for long-running execution upgrades.

### Why this is the right leap

- Temporal's deterministic replay constraints are battle-tested for long-lived workflows and map to Lumen's stated replay goals.

### 6) Compiler diagnostics

### Adopt now

- **Parser panic-mode recovery** with multi-error reporting per file.
- **Diagnostic objects with machine-actionable fixes** (suggestions/fix-its) for common syntax/type mistakes.
- **LSP incremental parse and semantic diagnostic caching** to avoid whole-file reparse on each keystroke.

### Adopt later

- Full code-action engine (imports/refactors) and template-style type diffing.

### Why this is the right leap

- Clang and rustc demonstrate that fix-it diagnostics materially improve throughput.
- Tree-sitter is already aligned with the needed incremental parse strategy.

### 7) Safety and runtime guarantees

### Adopt now

- **Defined arithmetic semantics** (`checked` with explicit runtime error by default).
- **VM guardrails**: register bounds checks, instruction fuel, UTF-8-safe slicing, unwrap elimination.
- **Deterministic runtime contract tests** for overflow/div-zero/NaN/string indexing behavior.

### Adopt later

- WASM component sandbox as an optional execution backend.

### Why this is the right leap

- These changes close known P0 safety gaps with minimal design risk.

### 8) Interop strategy

### Adopt now

- **Stable tool interface contract v1** (schema + effect + grant metadata as part of compiled artifact).
- **Schema-first interop bridge**:
  - emit JSON Schema/OpenAPI fragments for exported tool/cell signatures
  - validate tool I/O at runtime against declared schemas
- **MCP-aligned provider bridge** for tools/resources with explicit capability mapping.

### Adopt later

- WIT/component-model export path for stronger polyglot interop and sandboxing.

### Why this is the right leap

- Schema-driven contracts and MCP-style tool envelopes are implementable in the current provider model.
- WIT/component model is promising, but best as a second-phase interop target.

## Concrete roadmap with milestones

### Milestone 0 (2 weeks): Correctness floor and deterministic contract

Deliverables:

- Fix P0 safety defects already tracked in `docs/research/EXECUTION_TRACKER.md` (arithmetic, slicing, bounds, unwraps, fuel).
- Add deterministic-profile enforcement hooks in typechecker/lowering.
- Wire trace store into VM for real execution events.

Acceptance tests:

1. `cargo test --workspace` passes with new regression tests for all P0 bug classes.
2. `lumen check` on deterministic code using non-deterministic intrinsic fails with a stable error code.
3. Replay test: `run -> trace -> replay` yields identical output and final state hash.

### Milestone 1 (3 weeks): Type/effect hardening + error model unification

Deliverables:

- Generic type parameter checking and instantiation.
- Effect row normalization + budget compatibility enforcement.
- Runtime `where` constraint checks on record construction.
- Unified runtime error taxonomy and propagation paths.

Acceptance tests:

1. Generic misuse cases fail with targeted diagnostics (wrong arity, wrong constraints).
2. Effect mismatch call graph emits one error per violating edge with normalized rows in message.
3. Runtime record construction violating `where` fails deterministically with typed error payload.

### Milestone 2 (3 weeks): Modules/packages and structured concurrency

Deliverables:

- Lockfile/`--locked` enforcement in CI profile.
- Major-version boundary rules in resolver/import logic.
- Structured-concurrency scope rules over current future runtime.
- Cancellation propagation tests.

Acceptance tests:

1. Build/check in locked mode fails if dependency graph would mutate lockfile.
2. Importing incompatible major versions without explicit path separation fails resolution.
3. Child tasks are cancelled on parent failure; no orphan futures remain after scope exit.

### Milestone 3 (3 weeks): Diagnostics and LSP leap

Deliverables:

- Parser recovery with multiple diagnostics per file.
- First fix-it diagnostics for high-frequency syntax/type errors.
- Incremental parsing path in LSP with invalidation granularity by edit range.

Acceptance tests:

1. Single malformed file reports >=3 independent diagnostics in one run where applicable.
2. LSP benchmark corpus: median diagnostic latency under 100ms for single-line edits.
3. Fix-it tests verify exact replacement spans and resulting parse validity.

### Milestone 4 (4 weeks): Interop contract v1 + replay confidence

Deliverables:

- Emit schema artifacts for exported cells/tools.
- Runtime schema validation boundary on provider call/return.
- MCP bridge with explicit grant/effect mapping.
- Replay differential harness for representative workflows.

Acceptance tests:

1. Exported schema passes JSON Schema/OpenAPI validator checks.
2. Tool provider returning schema-invalid payload fails with deterministic, typed runtime error.
3. MCP tool calls respect grant/effect constraints and are trace-auditable.
4. Replay differential suite passes across 100+ seeded executions.

## Prioritization rubric (used for all backlog items)

Score each candidate 1-5 per axis and execute highest weighted total first:

- User-visible reliability impact (x3)
- Implementation risk (inverse, x2)
- Architectural leverage across compiler+runtime+tooling (x2)
- Compatibility with deterministic/replay goals (x2)
- Time-to-ship in current codebase (x3)

## Explicit non-goals (for this plan window)

- Borrow checker / ownership system.
- Full algebraic effect handlers with continuations.
- New native backend (LLVM/Cranelift) before VM semantics are hardened.
- Advanced macro/metaprogramming expansion beyond current parser support.

## Source links

Type/effects and handlers:

- Koka row-polymorphic effects (MSR): https://www.microsoft.com/en-us/research/publication/koka-programming-with-row-polymorphic-effect-types/
- OCaml 5 effect handlers reference: https://ocaml.org/releases/5.0/manual/effects.html

Error handling:

- Rust recoverable errors (`Result`, `?`): https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html
- Zig error unions and `try`: https://ziglang.org/documentation/master/

Async and structured concurrency:

- Swift SE-0304 structured concurrency (raw spec): https://raw.githubusercontent.com/swiftlang/swift-evolution/main/proposals/0304-structured-concurrency.md
- Kotlin coroutines basics (structured concurrency section): https://kotlinlang.org/docs/coroutines-basics.html

Determinism/replay:

- Temporal workflow versioning and determinism constraints: https://docs.temporal.io/develop/typescript/versioning

Diagnostics/tooling:

- Clang expressive diagnostics and fix-it hints: https://clang.llvm.org/diagnostics
- rustc diagnostics guide: https://rustc-dev-guide.rust-lang.org/diagnostics/diagnostic-structs.html
- Tree-sitter incremental parsing docs: https://tree-sitter.github.io/tree-sitter/index.html

Module/package semantics:

- Cargo dependency resolver and lockfile behavior: https://doc.rust-lang.org/nightly/cargo/reference/resolver.html
- Go modules reference (versioning/import-path rules): https://go.dev/ref/mod
- npm lockfile semantics: https://docs.npmjs.com/cli/v10/configuring-npm/package-lock-json/

Interop/schemas:

- WebAssembly component model rationale: https://component-model.bytecodealliance.org/design/why-component-model.html
- WIT reference: https://component-model.bytecodealliance.org/design/wit.html
- OpenAPI spec index (current versions): https://spec.openapis.org/oas/
- JSON Schema draft 2020-12: https://json-schema.org/draft/2020-12/json-schema-core.html
- MCP specification (official): https://modelcontextprotocol.io/specification/draft
