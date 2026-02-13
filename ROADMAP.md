# Lumen Roadmap

This roadmap describes the direction of the language and platform.
It intentionally avoids dates and fixed timelines.

## Vision

Lumen is a general-purpose, statically typed language for AI-native systems.
It should provide mainstream language quality while offering first-class constructs for effects, agents, orchestration, policy, and deterministic execution.

## Strategic Pillars

## 1. Language Core

- Mature static type system with robust generics.
  - Type alias resolution (aliases parsed but not expanded -- immediate gap).
  - Generic type instantiation and bounded generics.
  - Trait conformance checking and method dispatch.
- First-class effect rows integrated into typing and call compatibility.
  - Real algebraic effect handlers with one-shot continuations.
  - Scoped handling (`with handler in ... end`) for mocking, guardrails, middleware.
- Expression completeness: expression-position match/if/loop, let destructuring, closures with upvalue capture, set/map comprehensions, spread operator.
- Strong compile-time diagnostics with strict defaults.
  - Remove placeholder/whitelist workarounds; replace with proper scoping.
  - Duplicate definition detection across all declaration kinds.
- Clean module/package semantics and import boundaries.

## 2. Deterministic Runtime

- Deterministic execution profile for workflow-style programs.
- Replayable execution model with trace and checkpoint support.
- Explicit async/future semantics with predictable scheduling behavior.
- VM hardening: checked arithmetic, division-by-zero errors, register bounds checking, UTF-8-safe string operations, instruction fuel limits.
- Portable runtime interfaces for long-term multi-target support.

## 3. Agent and Orchestration Semantics

- Fully typed machine-state semantics with transition trace events.
- Typed pipelines and orchestration graph compilation.
- Real effect handlers with continuation semantics.
- Memory, guardrail, and eval declarations as executable first-class runtime entities.
- Trace system wired into VM execution (currently disconnected).

## 4. Capability and Security Model

- Policy-backed capability enforcement rather than heuristics.
- Static + runtime checks with audit-quality diagnostics.
- Traceable policy decisions and denial reasons.
- Native tool execution layer (MCP client, subprocess protocol) -- existential for the language's purpose.

## 5. Tooling and Developer Experience

- First-party language server (diagnostics, go-to-definition, completion).
- `lumen fmt` formatter.
- Package/dependency tooling with lockfile determinism.
- Compatibility analysis for upgrades.
- Full intrinsic stdlib mapping (51 of 69 intrinsics unreachable from source).

## 6. Ecosystem and Trust

- Implementation-accurate language specification.
  - Spec must distinguish "parsed" from "implemented" for each feature.
  - Lambda/closure and intrinsic stdlib documentation.
- Semantic conformance test suite tied to spec behavior.
  - Regression tests for all known bugs.
  - Example files as automated integration tests.
  - Typechecker and lowering test expansion (currently 2 tests each).
- Public design docs for major language/runtime decisions.

## Type System Maturation Path

Incremental path to a complete type system:
1. Type alias resolution (substitute aliases during type checking).
2. Generic type instantiation (monomorphization or erasure).
3. Trait conformance checking (verify impls satisfy trait contracts).
4. Trait method dispatch (resolve method calls through impl lookup).
5. Bounded generics (`where T: Trait` constraints).

## Execution Principles

- No metadata-only features presented as full semantics.
- Strict mode is the baseline; permissive modes are explicit opt-in.
- Runtime state must be instance-safe and deterministic-profile aware.
- Every major language feature requires parser, resolver, lowering, runtime, and tests.
- Fix correctness bugs before adding new features.
