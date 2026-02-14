# Lumen Roadmap

This roadmap describes the direction of the language and platform.
It intentionally avoids dates and fixed timelines.

## Current Status (February 2026)

### Execution Alignment Snapshot (February 2026)

Recently completed (implementation verified):

- `!=` lowering fix (`8449533`, `rust/lumen-compiler/src/compiler/lower.rs`)
- VM arithmetic and UTF-8 safety pass (`f73bc03`, `c709de2`, `rust/lumen-vm/src/vm.rs`)
- LSP capability expansion (`d7a19db`, `rust/lumen-lsp/src/main.rs`)
- MCP provider crate + stdio transport (`1a80db2`, `1e6541d`, `rust/lumen-provider-mcp/src/lib.rs`)

Execution plan and measurable acceptance criteria for the next three rounds are tracked in `docs/research/EXECUTION_TRACKER.md`.

### What's Built

**Compiler Pipeline (Complete):**
- Source loading for `.lm` and markdown extraction for `.lm.md` with `@directive` support
- Lexer (full token support)
- Parser (all declaration forms, expressions, statements, patterns)
- Name resolution with symbol table
- Type checker with bidirectional inference
- Constraint validator for `where` clauses
- LIR lowering to 32-bit fixed-width bytecode

**VM Runtime (Functional):**
- Register-based interpreter (~100 opcodes)
- Call frames with max depth 256
- Future/async support with eager and deferred FIFO scheduling
- Memory runtime (append, recent, recall, get, query, store)
- Machine runtime (typed state graph with transitions)
- Pipeline runtime (stage chains with type-checked data flow)
- Orchestration builtins (parallel, race, vote, select, timeout)
- 69 intrinsic functions (string, math, collections, JSON, encoding)

**Provider Architecture (Implemented):**
- `ToolProvider` trait in `lumen-runtime`
- `ProviderRegistry` with register/get/list methods
- `lumen.toml` config file parsing (`lumen-cli/src/config.rs`, 335 lines)
- CLI wired to load config and populate registry
- `lumen init` command generates default config
- Four provider crates started: http, fs, json, mcp

**Tooling (Functional):**
- CLI with 12 commands: check, run, emit, trace, cache, init, repl, pkg, fmt, doc, lint, build
- LSP server: diagnostics, go-to-definition, hover, completion, semantic tokens, symbols, signature help, inlay hints, code actions, folding, references
- REPL (265 lines): basic interactive loop
- Formatter (1259 lines): AST-based code formatting
- Package manager: path dependency resolution, lockfile generation, and registry-facing stubs
- Config system (335 lines): lumen.toml parsing

**Test Coverage:**
- ~485 tests across workspace (25 test files)
- Compiler: lexer, parser, resolver, typecheck, lowering
- VM: instruction dispatch, futures, process runtimes
- Runtime: provider registry, tool dispatch
- Examples in `.lm`/`.lm.md` formats (compile-tested)

**Crate Layout:**
- `lumen-compiler` — front-end pipeline
- `lumen-vm` — bytecode interpreter
- `lumen-runtime` — tool dispatch, caching, traces
- `lumen-cli` — command-line interface
- `lumen-lsp` — language server
- `lumen-provider-http` — HTTP client provider
- `lumen-provider-fs` — filesystem provider
- `lumen-provider-json` — JSON utilities provider
- `lumen-provider-mcp` — MCP bridge provider

### What's Missing (V1 Blockers)

**Critical Bugs (P0):**
- Closure upvalue capture broken (no capture list)
- Set/map comprehensions broken (always emit list)
- `if let` / `while let` broken (discard pattern)
- Remaining VM safety issues (register bounds checks, NaN/interned-string ordering, global instruction fuel)

**Type System Gaps (P1):**
- Generic type instantiation (parsed but never checked)
- Trait conformance checking (parsed but never verified)
- Record field defaults (parsed but never applied)
- Runtime `where` constraint enforcement (compile-time only)

**Tooling Gaps (P1):**
- Parser error recovery (fails fast on first error)
- LSP incremental parsing (re-parses entire file)
- Trace system disconnected from VM
- 51 of 69 intrinsics unmapped from source names

**Provider Gaps (P3):**
- MCP bridge hardening (external server reliability + one-command setup)
- No LLM providers (OpenAI, Anthropic)
- Effect kinds hardcoded (should come from providers)

## Positioning

Lumen is the first language where AI agent behavior is statically verifiable — effect-tracked, cost-budgeted, policy-enforced, and deterministically reproducible.

It is not "better LangChain" (integration glue) or "typed Python for AI" (framework). It occupies a new category: **compile-time verification for agent systems.** Effect rows, capability grants, typed state machines, and deterministic execution are features that cannot be retrofitted onto existing frameworks — they require language-level integration.

## V1 Milestone Checklist

The V1 release focuses on **core correctness and essential tooling**, not feature completeness.

### Must-Fix for V1

- [ ] Close remaining P0 correctness and VM safety bugs (see `tasks.md`)
- [ ] Add parser error recovery (collect multiple errors)
- [ ] Wire trace system into VM
- [ ] Complete intrinsic stdlib mapping (51 unmapped functions)
- [ ] Add stack traces on runtime errors
- [ ] Remove hardcoded type whitelists (resolver, typechecker)
- [ ] Add duplicate definition detection (records, enums, cells, processes)
- [ ] Document intrinsic stdlib in SPEC.md

### Nice-to-Have for V1

- [ ] Generic type instantiation (enables real type-safe collections)
- [ ] MCP bridge hardening (external server reliability + setup UX)
- [ ] LSP incremental parsing (performance)
- [ ] Field defaults and runtime `where` (correctness)

### Deferred to V2

- Trait conformance checking
- Real effect handlers with continuations
- Cost-aware types with budgets
- Session-typed protocols
- WASM compilation target
- Inline AI prompt operator: `data ~> "summarize this"` — ad-hoc AI calls where the RHS is a prompt string, not a function. Would need: default provider binding, prompt template semantics, return type inference, streaming support. Unique to Lumen — no other language has this.
- Consolidate provider crates into `lumen-std` with feature flags (fs, http, json, crypto, env, gemini, mcp)
- All novel AI-first features (JSON schema compilation, prompt templates, etc.)

## Strategic Pillars

### 1. Language Core

- Mature static type system with robust generics.
  - Type alias resolution (implemented); remaining work is generics + trait conformance.
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
- `lumen fmt` formatter (exists, needs config options).
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
  - Typechecker and lowering test expansion (currently minimal).
- Public design docs for major language/runtime decisions.

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
  lumen-provider-http/       # reqwest-based HTTP client (exists, needs completion)
  lumen-provider-fs/         # filesystem operations (exists, needs completion)
  lumen-provider-json/       # JSON utilities (exists, needs completion)
  lumen-provider-openai/     # OpenAI-compatible chat/embeddings (planned)
  lumen-provider-anthropic/  # Claude API adapter (planned)
  lumen-provider-mcp/        # MCP client bridge (planned, critical)
  lumen-provider-process/    # subprocess execution (planned)
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

**MCP as universal bridge (target):** `lumen-provider-mcp` is planned as the generic bridge so MCP tools can be surfaced as Lumen tools without compiler changes. The bridge is not functional yet.

**Effect kinds from provider declarations (target):** long-term, resolver effect checking should come from `ToolProvider::effects()` rather than hardcoded lists.

**Role blocks** define provider-agnostic conversation structure. Provider-specific translation of role blocks remains a key architecture goal and still needs completion.

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
1. Type alias resolution (substitute aliases during type checking) — **DONE**
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

## V2 Vision

After V1 correctness stabilization, V2 focuses on:

1. **AI-first differentiators**: JSON schema compilation, typed prompts, cost budgets, constraint retry
2. **Ecosystem growth**: MCP bridge, LLM providers, package registry, community templates
3. **Advanced type system**: trait conformance, effect handlers, session types, linear resources
4. **Compilation targets**: WASM for browser, native binaries for deployment
5. **Tooling maturity**: incremental compilation, debugger, profiler, full LSP 3.17

## Open Questions

- **Garbage collection strategy**: Current VM has no memory management for long-running programs
- **Concurrency model**: Thread safety, message passing, shared state semantics
- **FFI design**: How to safely call native code from Lumen
- **Package versioning**: SemVer enforcement, compatibility checking
- **Backward compatibility promise**: When does the language stabilize?
