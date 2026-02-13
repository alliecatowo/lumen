# Lumen Roadmap

This roadmap describes the direction of the language and platform.
It intentionally avoids dates and fixed timelines.

## Positioning

Lumen is the first language where AI agent behavior is statically verifiable — effect-tracked, cost-budgeted, policy-enforced, and deterministically reproducible.

It is not "better LangChain" (integration glue) or "typed Python for AI" (framework). It occupies a new category: **compile-time verification for agent systems.** Effect rows, capability grants, typed state machines, and deterministic execution are features that cannot be retrofitted onto existing frameworks — they require language-level integration.

## Strategic Pillars

### 1. Language Core

- Mature static type system with robust generics.
  - Type alias resolution (aliases parsed but not expanded — immediate gap).
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

### 2. Deterministic Runtime

- Deterministic execution profile for workflow-style programs.
- Replayable execution model with trace and checkpoint support.
- Explicit async/future semantics with predictable scheduling behavior.
- VM hardening: checked arithmetic, division-by-zero errors, register bounds checking, UTF-8-safe string operations, instruction fuel limits.
- Portable runtime interfaces for long-term multi-target support.

### 3. Agent and Orchestration Semantics

- Fully typed machine-state semantics with transition trace events.
- Typed pipelines and orchestration graph compilation.
- Real effect handlers with continuation semantics.
- Memory, guardrail, and eval declarations as executable first-class runtime entities.
- Trace system wired into VM execution (currently disconnected).

### 4. Capability and Security Model

- Policy-backed capability enforcement rather than heuristics.
- Static + runtime checks with audit-quality diagnostics.
- Traceable policy decisions and denial reasons.
- Native tool execution layer (MCP client, subprocess protocol) — existential for the language's purpose.

### 5. Tooling and Developer Experience

- First-party language server (diagnostics, go-to-definition, completion).
- `lumen fmt` formatter.
- Package/dependency tooling with lockfile determinism.
- Compatibility analysis for upgrades.
- Full intrinsic stdlib mapping (51 of 69 intrinsics unreachable from source).

### 6. Ecosystem and Trust

- Implementation-accurate language specification.
  - Spec must distinguish "parsed" from "implemented" for each feature.
  - Lambda/closure and intrinsic stdlib documentation.
- Semantic conformance test suite tied to spec behavior.
  - Regression tests for all known bugs.
  - Example files as automated integration tests.
  - Typechecker and lowering test expansion (currently 2 tests each).
- Public design docs for major language/runtime decisions.

## Novel Feature Track: AI-First Differentiators

These features build on existing Lumen scaffolding to create capabilities no other language provides. Each one extends something already parsed or partially implemented.

### Records → JSON Schemas (builds on `expect schema`)
Record types auto-compile to JSON schemas for LLM structured output APIs. `lumen emit --schema RecordName` generates the schema. The `ToolCall` opcode includes the schema in requests. Closes the loop from type definition to LLM output constraint.

### Typed Prompt Templates (builds on `role` blocks)
New `prompt` declaration bundles `role` blocks with typed input/output annotations. Compiler verifies interpolated variables exist with correct types. Output type compiles to JSON schema — connects directly to the schema work above.

### Cost-Aware Types (builds on `grant { max_tokens }`, effect rows)
`cost` annotation on effect rows: `cell summarize() -> String / cost[~2000]`. Compiler sums costs through the call graph and checks against `@budget` directives. Extends the existing grant `max_tokens` constraint into a type-level budget. No language on earth does this.

### Constraint-Driven Automatic Retry (builds on `where` clauses)
Combine `where` constraints with tool/LLM call semantics. When structured output violates constraints, the runtime auto-retries with the specific violation as feedback. Compile-time constraint verification + typed structured output + automatic retry in a single language construct.

### Effect-Guaranteed Deterministic Replay (builds on effect rows, `@deterministic`, traces)
The effect system proves all side effects are declared and traced. Replay mode substitutes recorded responses. No framework can guarantee trace completeness because they have no effect system. This is "audit completeness by construction."

### First-Class Capability Grants with Attenuation (builds on `grant` system)
Grants become first-class values that can be passed as arguments, stored in records, and attenuated (narrowed, never widened). Agents delegate subsets of capabilities to sub-agents. Compiler verifies delegated capabilities never exceed the delegator's.

### Session-Typed Multi-Agent Protocols (builds on `agent`/`process` model)
`protocol` declarations specify valid message sequences between agents. Compiler verifies each agent implements its role, all message types are handled, and no deadlocks occur. Multiparty session types (Honda/Yoshida) applied to AI agents for the first time.

### CRDT Memory Types (builds on `memory` process)
`memory SharedState: crdt` with typed CRDT fields (G-Counter, G-Set, LWW-Register). Multi-agent shared state with automatic conflict-free merging. No language has CRDT primitives.

### Event-Sourced Memory (builds on `memory` process, traces)
`memory AuditTrail: event_sourced` with typed `event` declarations. Runtime stores events immutably. State projections derived from event replay. Integrates with existing trace system.

### Linear Resource Types (builds on resolver, type system)
`once` and `consume` type qualifiers for API keys, session handles, context windows, tool call budgets. Single-use enforcement at compile time. Rust's ownership model applied to AI resources.

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
