# Lumen Tasks

This file tracks outstanding implementation work.
Completed work should be removed from this list and reflected in docs/changelog.

## Language Semantics

- [ ] Implement full typed `machine` graph semantics.
  - Parse/resolve/lower state graphs with typed transitions.
  - Validate reachability, terminal-state coverage, transition arg compatibility.
  - Execute deterministic state transitions in VM with trace events.

- [ ] Implement real effect handler semantics.
  - Add continuation-aware `handler` execution model.
  - Support scoped handling (`with <handler> in ...`) with interception/resume behavior.

- [ ] Implement declarative `pipeline` and `orchestration` semantics.
  - Compile stage graphs and coordinator/worker patterns.
  - Type-check stage interfaces end-to-end.
  - Define deterministic scheduling/merge behavior.

- [ ] Implement guardrail and eval semantics end-to-end.
  - Compile declaration blocks into executable runtime structures.
  - Add policy/eval execution paths with deterministic diagnostics.

## Effect and Capability System

- [ ] Replace heuristic grant matching with explicit policy semantics.
  - Define policy AST and evaluator for static/resolve-time checks.
  - Replace remaining heuristic grant/effect matching in resolver.

## Runtime and Determinism

- [ ] Add replay/checkpoint model for long-running workflows.
  - Stable event log format.
  - Replay verifier for deterministic mode.
  - Checkpoint resume primitives for machine/orchestration states.

- [ ] Harden future/async semantics.
  - Explicit pending/completed/error future states.
  - Deterministic scheduling controls for parallel constructs.

## Toolchain and Ecosystem

- [ ] Build first-party LSP server.
  - Diagnostics, go-to-definition, references, rename, completion.

- [ ] Add package/module system and dependency lockfile.
  - Deterministic resolution and reproducible builds.

- [ ] Add compatibility tooling.
  - API/symbol diff checks across versions.
  - Semver policy checks in CI.

- [ ] Expand semantic conformance tests.
  - Move beyond compile-sweep to runtime behavior assertions per spec section.

## Documentation

- [ ] Keep `SPEC.md` implementation-accurate.
- [ ] Keep `ROADMAP.md` aligned with major direction.
- [ ] Keep this file limited to concrete outstanding tasks.
