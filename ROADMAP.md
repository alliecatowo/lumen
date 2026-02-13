# Lumen Roadmap

This roadmap describes the direction of the language and platform.
It intentionally avoids dates and fixed timelines.

## Vision

Lumen is a general-purpose, statically typed language for AI-native systems.
It should provide mainstream language quality while offering first-class constructs for effects, agents, orchestration, policy, and deterministic execution.

## Strategic Pillars

## 1. Language Core

- Mature static type system with robust generics.
- First-class effect rows integrated into typing and call compatibility.
- Strong compile-time diagnostics with strict defaults.
- Clean module/package semantics and import boundaries.

## 2. Deterministic Runtime

- Deterministic execution profile for workflow-style programs.
- Replayable execution model with trace and checkpoint support.
- Explicit async/future semantics with predictable scheduling behavior.
- Portable runtime interfaces for long-term multi-target support.

## 3. Agent and Orchestration Semantics

- Fully typed machine-state semantics.
- Typed pipelines and orchestration graph compilation.
- Real effect handlers with continuation semantics.
- Memory, guardrail, and eval declarations as executable first-class runtime entities.

## 4. Capability and Security Model

- Policy-backed capability enforcement rather than heuristics.
- Static + runtime checks with audit-quality diagnostics.
- Traceable policy decisions and denial reasons.

## 5. Tooling and Developer Experience

- First-party language server.
- Formatter and linting standards.
- Package/dependency tooling with lockfile determinism.
- Compatibility analysis for upgrades.

## 6. Ecosystem and Trust

- Implementation-accurate language specification.
- Semantic conformance test suite tied to spec behavior.
- Public design docs for major language/runtime decisions.

## Execution Principles

- No metadata-only features presented as full semantics.
- Strict mode is the baseline; permissive modes are explicit opt-in.
- Runtime state must be instance-safe and deterministic-profile aware.
- Every major language feature requires parser, resolver, lowering, runtime, and tests.
