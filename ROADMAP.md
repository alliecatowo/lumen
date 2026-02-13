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

## Provider Architecture: Pluggable Runtime

This is the critical architectural decision for Lumen: **the language defines contracts, the runtime loads implementations.** No transport layer or provider-specific code is baked into the compiler. The compiler verifies types, effects, and grants with zero knowledge of what a tool actually does. Providers are external, replaceable, and evolve independently.

### Layer 1 — Language Primitives (baked in, never changes)

These are part of the language grammar and compiler semantics. They ship with the compiler and define the contracts that providers must satisfy.

- `use tool <provider>.<method> as <Alias>` — tool import syntax
- `grant <Alias> <constraints>` — capability-scoped tool access with policy constraints
- Effect rows: `cell foo() -> T / {http, trace}` — declared side effects
- `ToolCall` opcodes in LIR — VM-level tool invocation
- `role` / conversation blocks — provider-agnostic conversation structure (the provider translates to wire format)
- `expect schema` validation — structural output contracts
- Trace event emission — structured execution logs for replay and audit

The compiler verifies types, effects, and grants. It has zero knowledge of what a tool actually does at runtime.

Example Lumen source using tools:

```lumen
use tool github.create_issue as CreateIssue
use tool slack.send_message as SlackMsg

grant CreateIssue timeout_ms 10000
grant SlackMsg timeout_ms 5000
```

### Layer 2 — ToolProvider Interface (trait in runtime, implementations external)

A single trait in `lumen-runtime` that all providers implement:

```rust
pub trait ToolProvider: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn schema(&self) -> &ToolSchema;
    fn call(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError>;
    fn effects(&self) -> &[&str];
}
```

- The VM holds `HashMap<String, Box<dyn ToolProvider>>`
- `ToolCall` opcode looks up the provider by name and calls it
- The VM does not know or care what the provider does internally
- This trait is the **only** coupling point between the language and any external service

### Layer 3 — First-Party Provider Packages (ships with CLI, NOT the compiler)

Separate crates implementing `ToolProvider`, maintained and versioned independently of the compiler:

```
lumen-providers/
  lumen-provider-http/       # reqwest-based HTTP client
  lumen-provider-openai/     # OpenAI-compatible chat/embeddings
  lumen-provider-anthropic/  # Claude API adapter
  lumen-provider-mcp/        # MCP client bridge (every MCP server = Lumen tool)
  lumen-provider-fs/         # filesystem operations
  lumen-provider-process/    # subprocess execution
```

Each provider is a library crate. They ship with the Lumen CLI for convenience but can be replaced entirely with custom implementations. They update independently of the compiler.

### Layer 4 — Provider Registration (lumen.toml runtime config)

Runtime configuration maps tool names to provider implementations:

```toml
[providers]
llm.chat = "openai-compatible"
http.get = "builtin-http"
http.post = "builtin-http"
mcp = "builtin-mcp"

[providers.config.openai-compatible]
base_url = "https://api.anthropic.com/v1"
api_key_env = "ANTHROPIC_API_KEY"
default_model = "claude-sonnet-4-20250514"

[providers.config.builtin-mcp]
servers = ["npx -y @modelcontextprotocol/server-filesystem /tmp"]

[providers.mcp.github]
uri = "npx -y @modelcontextprotocol/server-github"
tools = ["github.create_issue", "github.search_repos"]

[providers.mcp.slack]
uri = "npx -y @modelcontextprotocol/server-slack"
tools = ["slack.send_message", "slack.list_channels"]
```

**Zero code changes for provider switching:**

- Switch from OpenAI to Anthropic? Change one line in `lumen.toml`.
- New provider? Write a `ToolProvider` impl and register it.
- Provider changes its API? Update the provider package, not the compiler.
- Running locally with Ollama? Same interface, different config.

Language source never mentions "openai" or "anthropic" — those are config-level concerns.

**MCP as universal bridge:** Every MCP server is already a `ToolProvider`. The `lumen-provider-mcp` crate is a generic bridge — every MCP tool in the ecosystem becomes a Lumen tool with zero custom code. MCP config in `lumen.toml` maps server URIs to tool namespaces.

**Effect kinds come from provider declarations**, not hardcoded lists in the compiler. Each provider declares its effects via `ToolProvider::effects()`, and the resolver uses those declarations for effect checking.

**Role blocks** define provider-agnostic conversation structure. The `role` / `end` syntax captures the shape of a conversation (system, user, assistant turns) without prescribing any wire format. The provider translates role blocks to its native API format — OpenAI's message array, Anthropic's Messages API, or any future format.

### What Goes Where

| Baked into language | Provider library |
|---|---|
| `use tool` / `grant` syntax | HTTP client |
| Effect rows and capability checking | OpenAI/Anthropic/Google API adapters |
| `role` / `end` conversation blocks | MCP client bridge |
| `expect schema` validation | Filesystem operations |
| Trace event emission | Subprocess execution |
| `ToolCall` opcode in VM | Embedding model adapters |
| `ToolProvider` trait | Database drivers |
| Provider registry lookup | Custom user providers |

### Why This Separation Is Non-Negotiable

The language is stable for decades. The providers evolve weekly. They must never be in the same crate.

Historical lessons:

- **PHP baked in `mysql_query()`** — 20 years of deprecation warnings, security holes, and migration pain (`mysql_`, `mysqli_`, PDO) because the database driver was a language built-in.
- **Go baked `net/http` into stdlib** — frozen at 2012 design decisions. Every HTTP/2 and HTTP/3 improvement requires stdlib changes gated by Go's compatibility promise.
- **Java JDBC is 25 years old and still works perfectly** — because it is an interface. Driver implementations evolve freely behind it.

AI providers are changing monthly. OpenAI changed their API format three times in 2024. Anthropic's message format is different from OpenAI's. Google's is different from both. MCP is still evolving. If you bake any of this in, you're chasing spec changes in your compiler forever.

The `ToolProvider` trait is Lumen's JDBC moment — a stable contract that outlives any individual provider.

## Execution Principles

- No metadata-only features presented as full semantics.
- Strict mode is the baseline; permissive modes are explicit opt-in.
- Runtime state must be instance-safe and deterministic-profile aware.
- Every major language feature requires parser, resolver, lowering, runtime, and tests.
- Fix correctness bugs before adding new features.
