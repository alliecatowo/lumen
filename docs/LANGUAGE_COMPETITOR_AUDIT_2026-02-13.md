# Lumen Competitor Audit (2026-02-13)

This document defines what Lumen needs to compete with mainstream programming languages, not just AI workflow DSLs.
It combines external research with implementation-level gaps and acceptance criteria.

## Positioning

Lumen target: a general-purpose, statically typed language with first-class AI/runtime primitives.

To be credible as a competitor, Lumen must provide all of:

1. A sound core language (types, effects, determinism profile, modules, diagnostics).
2. A production runtime model (isolation, replay semantics, portability).
3. Industrial tooling (formatter, LSP, package manager, tests, docs, release channels).
4. Security/capability enforcement that is machine-checkable.

## External Research (Primary Sources)

1. Koka (effect typing + handlers + inference)
- Source: https://koka-lang.github.io/koka/doc/index.html
- Source: https://www.microsoft.com/en-us/research/publication/principal-type-inference-under-a-prefix/
- Implication for Lumen: effect rows must be central in typechecking and call compatibility, not metadata.

2. OCaml 5 effect handlers
- Source: https://v2.ocaml.org/manual/effects.html
- Implication: handlers need real continuation semantics if `handler` is language-level, not only declarative stubs.

3. Unison (content-addressed language artifacts)
- Source: https://www.unison-lang.org/docs/language-reference/abilities-and-ability-handlers/
- Source: https://www.unison-lang.org/docs/language-reference/hashes/
- Implication: if Lumen keeps content-addressed identity in spec, hashes must participate in compilation and linking.

4. Deterministic workflow runtimes (Temporal)
- Source: https://docs.temporal.io/workflow-execution/event
- Source: https://javadoc.io/static/io.temporal/temporal-sdk/1.23.2/io/temporal/workflow/package-summary.html
- Implication: deterministic mode must ban nondeterministic operations in workflow code or force explicit boundaries.

5. WASI 0.2 + Component Model
- Source: https://bytecodealliance.org/articles/WASI-0.2
- Source: https://component-model.bytecodealliance.org/
- Implication: long-term portability should target component interfaces, not VM-internal ABI only.

6. Tree-sitter and modern language tooling
- Source: https://tree-sitter.github.io/tree-sitter/
- Implication: competitive editor/tooling requires incremental parse + robust syntax trees.

7. Language Server Protocol
- Source: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/
- Implication: first-party LSP is mandatory for serious adoption.

8. Policy as code (Cedar)
- Source: https://docs.cedarpolicy.com/
- Source: https://github.com/cedar-policy/cedar-spec
- Implication: grant/capability checks should converge to explicit policy semantics, not alias heuristics.

9. MCP standardization
- Source: https://modelcontextprotocol.info/specification/2025-11-25/
- Implication: tool/runtime contracts should align to stable MCP schema and versioning behavior.

10. Structured output/runtime constraints for AI interfaces
- Source: https://openai.com/index/introducing-structured-outputs-in-the-api/
- Implication: schema-constrained generation should be runtime-verified and typed end-to-end.

## Competitive Baseline Matrix

## Language Core

- Required:
  - sound type system with principled generics and effect rows
  - strict default diagnostics with explicit permissive/doc mode
  - module/package semantics and import boundaries
  - trait/interface coherence rules
- Current:
  - strong progress on strict mode + effect inference
  - module/package coherence still limited

## Runtime Semantics

- Required:
  - deterministic execution profile with compile-time enforcement
  - instance isolation for runtime objects
  - replay/checkpoint model for orchestrations/machines
- Current:
  - deterministic profile enforcement exists (batch 2)
  - process instance isolation exists (batch 2)
  - replay/checkpoint semantics still missing

## Effect/Capability Model

- Required:
  - effect inference + declared effect conformance
  - effect-subtyping/call-site compatibility
  - non-heuristic capability policies
  - handler execution semantics
- Current:
  - inference + undeclared effect errors implemented
  - capability checks remain partially heuristic
  - handler continuation semantics missing

## Toolchain

- Required:
  - formatter stability guarantees
  - LSP (diagnostics, completion, rename, go-to-def)
  - package manager/build graph
  - semantic versioning and compatibility checks
- Current:
  - compiler + VM core is present
  - LSP/package/distribution workflow incomplete

## Ecosystem and Trust

- Required:
  - executable language specification tests
  - conformance test corpus by feature area
  - security model and policy proofs for high-stakes paths
- Current:
  - markdown compile sweep and runtime tests exist
  - semantic coverage still incomplete for machine/handler/pipeline DSL semantics

## Glaring Gaps (Priority Ordered)

1. `machine` DSL semantics are not compiled as a typed transition system.
- Needed: parse/resolve/lower machine graph + transition typing + runtime execution model.

2. `handler` declarations are not effect handlers semantically.
- Needed: continuation-aware effect interception semantics.

3. Declarative `pipeline`/`orchestration` semantics remain partial.
- Needed: stage graph typing, fan-out/fan-in semantics, deterministic scheduling model.

4. Capability model is heuristic.
- Needed: explicit policy language and static/runtime enforcement model.

5. Tooling gap to compete with mainstream languages is still large.
- Needed: LSP + package manager + compatibility tooling.

## Batch 2 Changes Implemented

1. Process runtime isolation fixed.
- Runtime state now keyed per process instance, not per type name singleton.
- Multiple `memory`/`machine` instances no longer alias each other.

2. Deterministic profile enforcement added.
- `@deterministic true` now rejects nondeterministic effects/operations (`random`, `time`, external/tool-like effects).
- Effect inference now marks `uuid/uuid_v4` as `random`, `timestamp` as `time`.

3. Project positioning updated.
- README now positions Lumen as a general-purpose language platform target, not a narrow DSL.

## Next Mega Batch (Concrete)

1. Machine Graph Compiler
- Parse explicit machine state graph into AST.
- Add resolve checks: reachability, terminal coverage, transition argument typing.
- Lower to machine IR and execute in VM with deterministic transition log.
- Acceptance:
  - state DSL examples in SPEC_ADDENDUM execute with typed transitions.
  - invalid transitions fail at compile time.

2. Real Effect Handlers
- Introduce runtime continuation representation for handled operations.
- Implement `with <handler> in ...` with proper interception semantics.
- Acceptance:
  - handler tests prove interception and resumption behavior.

3. Capability Policy Core
- Replace alias/path heuristics with explicit policy AST + evaluator.
- Integrate policy checks into compile + runtime boundaries.
- Acceptance:
  - deny-by-default model with deterministic diagnostics and policy traces.

4. Language Tooling Foundation
- Build LSP MVP from compiler diagnostics/AST symbols.
- Add module/package lockfile and semantic version compatibility checks.
- Acceptance:
  - editor diagnostics and definition navigation work on examples/spec suite.

## Non-Negotiable Quality Bars

1. No metadata-only language features presented as executable semantics.
2. No hidden global shared state for instance-bound runtime constructs.
3. Determinism profile must be statically enforceable and test-backed.
4. Every major spec section must have semantic tests, not compile-only tests.

