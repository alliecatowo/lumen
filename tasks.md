# Lumen Tasks

This file tracks outstanding implementation work.
Completed work should be removed from this list and reflected in docs/changelog.

## P0 — Blocking V1 Release (Critical Bugs)

- [ ] Fix `!=` operator: `BinOp::NotEq` maps to `OpCode::Eq` but never emits a `Not` inversion.
  - `lower.rs:1465` — add `Not` instruction after `Eq` when op is `NotEq`.

- [ ] Fix closure/lambda upvalue capture: lambda bodies get fresh `RegAlloc` with no enclosing scope.
  - `lower.rs:1833-1892` — build upvalue list, emit `GetUpval`/`SetUpval` for captured variables.

- [ ] Fix set/map comprehension lowering: `kind` field ignored, always emits `NewList`.
  - `lower.rs:2076-2151` — dispatch on `ComprehensionKind` to emit `NewSet`/`NewMap`.

- [ ] Fix `if let` / `while let`: parser discards pattern, replaces with `BoolLit(true)`.
  - `parser.rs:1056-1060, 1457-1460` — parse the binding pattern and lower it properly.

- [ ] Fix integer arithmetic overflow: unchecked ops panic in debug, wrap in release.
  - `vm.rs` Add/Sub/Mul/Pow handlers — use `checked_*` or `wrapping_*` with defined semantics.

- [ ] Fix integer division/modulo by zero: silently returns 0.
  - `vm.rs:911-928` — return `VmError::Runtime` on zero divisor.

- [ ] Fix bit shift panic on negative amounts: `-1i64 as u32` causes panic.
  - `vm.rs:1018,1026` — clamp or error on out-of-range shift amounts.

- [ ] Fix string slice panic on non-ASCII: byte-indexed `s[start..end]` panics mid-codepoint.
  - `vm.rs:3157-3169` — use `char_indices()` for character-based slicing.

- [ ] Fix `hex_decode` panic on odd-length input.
  - `vm.rs:2696-2703` — guard against odd-length strings.

## P1 — Core Language Completeness

### Compiler Correctness

- [ ] Add register bounds checking in VM dispatch loop.
  - `RegisterOOB` error variant exists but is never used. Add bounds checks or debug assertions.

- [ ] Replace `unwrap()` calls in VM with proper error propagation.
  - `vm.rs:658,1150,1275,1297,1306` — convert to `ok_or(VmError::...)`.

- [ ] Add duplicate definition detection for records, enums, cells, processes, effects, handlers.
  - `resolve.rs` — currently only agents check for duplicates.

- [ ] Fix `for` loop tuple destructuring: discards all variables except the first.
  - `parser.rs:1128-1137` — capture all identifiers in the destructuring pattern.

- [ ] Fix expression-position `match`/`if`/`loop`/`try`: returns placeholder `Ident("match_expr")` etc.
  - Already partially fixed with `MatchExpr` and `BlockExpr` AST variants; verify completeness.

- [ ] Remove hardcoded application-specific type names from resolver builtins.
  - `resolve.rs:244-285` — `Invoice`, `MyRecord`, `LineItem`, `Pair`, `A/B/C` etc. are not builtins.

- [ ] Clean up `is_doc_placeholder_var` whitelist in typechecker.
  - `typecheck.rs:57-153` — ~100 hardcoded variable names bypass type checking. Replace with proper scoping.

- [ ] Wire intrinsic name mapping for unmapped builtins (51 of 69 unreachable from source).
  - `lower.rs:1670-1686` — add mappings for `sort`, `reverse`, `map`, `filter`, `reduce`, `trim`, `upper`, `lower`, `replace`, `find`, `zip`, `enumerate`, `flatten`, `unique`, `take`, `drop`, `first`, `last`, `is_empty`, `chars`, `starts_with`, `ends_with`, `index_of`, `round`, `ceil`, `floor`, `sqrt`, `pow`, `log`, `sin`, `cos`, `clamp`, `clone`, `debug`, etc.

- [ ] Fix `url_encode` for multi-byte UTF-8: encodes codepoint instead of UTF-8 bytes.
  - `vm.rs:2706-2718` — iterate bytes, not chars.

### Type System

- [x] Implement type alias resolution: aliases are stored and can be expanded.
  - `typecheck.rs:190-191` — type aliases resolve correctly.

- [ ] Implement generic type parameter checking: generics parsed but never instantiated.
  - Path: generic instantiation → bounded generics → trait bounds.

- [ ] Implement trait conformance checking and method dispatch.
  - `resolve.rs:567-586` — traits/impls stored, never verified or dispatched.

- [ ] Implement record field default values at construction time.
  - `FieldDef.default_value` is parsed and stored but never applied.

- [ ] Implement runtime `where` constraint evaluation on record construction.
  - Constraints validated for form in `constraints.rs` but never enforced at runtime.

### Parser Error Recovery

- [ ] Add error recovery in parser for partial parses.
  - Currently fails fast on first syntax error; should continue parsing to collect multiple errors.

- [ ] Add panic mode recovery for common syntax errors.
  - Missing `end`, unmatched parens, incomplete expressions.

### Runtime Infrastructure

- [ ] Wire trace system into VM: `TraceRef` opcode currently creates dummy values.
  - VM has no `TraceStore` field. Trace infrastructure exists in runtime but is disconnected.

- [ ] Fix closure captures in VM: `Closure` opcode always creates zero captures.
  - `vm.rs:1271-1293` — `cap_count` is always 0.

- [ ] Fix `Await` potential infinite loop when future never resolves.
  - `vm.rs:1411-1425` — add fuel/timeout mechanism.

- [ ] Fix NaN handling: `Eq` for `Value::Float(NaN)` violates reflexivity; `Ord` treats NaN as Equal.
  - `values.rs:314,362` — define consistent NaN semantics.

- [ ] Fix interned string comparison: all interned strings compare as empty string in `Ord`.
  - `values.rs:366-377` — `Ord` impl needs `StringTable` access or owned-string fallback.

- [ ] Add instruction fuel/step counter for infinite loop protection.

- [ ] Implement disk cache loading on startup (currently memory-only despite writing to disk).
  - `cache.rs:34-36` — `get()` only checks memory, ignores files from previous runs.

- [ ] Make tool dispatch async-capable.
  - `tools.rs:34` — trait is synchronous, blocks entire VM.

### Test Coverage

- [ ] Add regression tests for known bugs: signed jumps, match register clobber, Type::Any BinOp.
- [ ] Add example files as automated integration tests (compile + execute the working examples).
- [ ] Expand typechecker tests (currently minimal): BinOp inference, record fields, call args, union returns.
- [ ] Expand lowering tests: control flow, match, closures, string interp, records.
- [ ] Add constraint validation tests.
- [ ] Add VM error path tests: stack overflow, undefined cell, register OOB, halt.
- [ ] Add end-to-end tests: while loop, for loop, string interp, closures, null coalesce, float arithmetic.

## P2 — Developer Experience

### Language Semantics

- [ ] Implement real effect handler semantics with continuations.
  - Support scoped handling (`with <handler> in ...`) with interception/resume.

- [ ] Complete orchestration semantics beyond pipeline stage chains.
  - Coordinator-worker patterns, deterministic scheduling/merge.

- [ ] Implement guardrail and eval as executable runtime structures (currently stubs).

- [ ] Implement `let` destructuring patterns properly.
  - `parser.rs:1047` — `pattern` field always `None`, uses `__tuple` workarounds.

- [ ] Implement spread operator semantics.
  - `lower.rs:2039` — `SpreadExpr` just unwraps inner, discards spread.

- [ ] Implement `async cell` as semantically distinct from `cell`.

- [ ] Add machine transition trace events and replay hooks.

### LSP Server

- [x] Basic LSP server exists (`lumen-lsp`)
  - Implements diagnostics, go-to-definition, hover, completion

- [ ] Add incremental parsing for LSP performance.
  - Currently re-parses entire file on every keystroke.

- [ ] Add semantic tokens for syntax highlighting.
  - LSP currently only uses TextMate grammar.

- [ ] Add code actions (quick fixes, refactorings).
  - Auto-import, extract cell, rename symbol.

- [ ] Add workspace symbol search.
  - Currently only searches open documents.

- [ ] Add document outline/symbols.
  - Show cells, records, enums in editor sidebar.

- [ ] Add signature help for function calls.

- [ ] Add inlay hints for type annotations.

### Formatter

- [x] `lumen fmt` formatter exists
  - `lumen-cli/src/fmt.rs` — 1259 lines, implements formatting

- [ ] Add configuration options for formatter.
  - Indent size, line width, brace style.

- [ ] Add format-on-save integration for LSP.

### REPL

- [x] `lumen repl` command exists
  - `lumen-cli/src/repl.rs` — 265 lines, basic REPL

- [ ] Add multi-line input support.
  - Currently single-line only.

- [ ] Add REPL history persistence.

- [ ] Add tab completion in REPL.

- [ ] Add `:type` command for type inspection.

- [ ] Add `:doc` command for documentation lookup.

### Package Manager

- [x] `lumen pkg` command exists
  - `lumen-cli/src/pkg.rs` — 654 lines, implements init/build/check

- [ ] Add `lumen pkg test` command.

- [ ] Add `lumen pkg publish` command.

- [ ] Add dependency resolution and lockfile.

- [ ] Add package registry support.

- [ ] Add version constraint parsing.

### CLI Improvements

- [ ] Add `lumen doc` command to generate documentation.

- [ ] Add `lumen bench` command for benchmarking.

- [ ] Add `lumen profile` command for profiling.

- [ ] Add `lumen upgrade` command for version migration.

- [ ] Add `lumen new` command for project scaffolding.

- [ ] Add colored diagnostics with source context (partially done).

- [ ] Add JSON output mode for machine-readable diagnostics.

### Debugging and Profiling

- [ ] Build debugger with breakpoints and step-through.
  - VM has `DebugEvent` infrastructure; need frontend.

- [ ] Add execution profiler for performance analysis.
  - Instruction counts, hot paths, allocation tracking.

- [ ] Add memory profiler for heap analysis.

- [ ] Add flamegraph generation.

## P3 — Ecosystem

### Provider Architecture

- [x] Define `ToolProvider` trait in `lumen-runtime/src/tools.rs`.
- [x] Create `ProviderRegistry` struct with `register`, `get`, `list` methods.
- [x] Wire `ProviderRegistry` into VM (`set_provider_registry` method).
- [x] Create `NullProvider` stub for unregistered tools.
- [x] Add provider registry tests (20 tests).
- [x] Add VM integration tests (2 tests).

- [x] Parse `lumen.toml` config file.
  - `lumen-cli/src/config.rs` — 335 lines, implements config parsing

- [x] Create `LumenConfig` struct.
  - Deserialize from `lumen.toml`, validate provider references.

- [x] Wire config loading into CLI startup.
  - `lumen run` and `lumen check` load config, populate registry.

- [x] Add `lumen init` command to generate default `lumen.toml`.

- [ ] Remove heuristic `effect_from_tool()` substring matching.
  - Replace with provider-declared effect mappings.

- [ ] Generalize `max_tokens` to generic `max_*` range constraints.
  - Grant constraints should support arbitrary numeric limits, not just `max_tokens`.

- [ ] Review `NONDETERMINISTIC_EFFECTS` list.
  - Ensure nondeterministic classification comes from provider declarations, not hardcoded list.

- [ ] Update `SPEC.md` with provider architecture.
  - Document ToolProvider trait, provider registry, lumen.toml config format.

- [ ] Audit compiler for hardcoded provider-specific knowledge.
  - Ensure no tool names, API shapes, or provider assumptions leak into compiler crates.

- [ ] Ensure effect kinds come from provider declarations, not hardcoded lists.
  - Providers declare their effects via `ToolProvider::effects()`.

- [ ] Desugar `role` blocks to standard tool call shapes.
  - Conversation structure maps to provider-agnostic tool invocations.

### First-Party Providers

- [x] `lumen-provider-http` crate (reqwest-based HTTP client).
  - Exists but needs feature completion and testing.

- [x] `lumen-provider-fs` crate (filesystem operations).
  - Exists but needs feature completion and testing.

- [x] `lumen-provider-json` crate (JSON operations).
  - Exists but needs feature completion and testing.

- [ ] `lumen-provider-mcp` crate (MCP server bridge — universal tool adapter).
  - Connects to any MCP server and exposes its tools as Lumen `ToolProvider` instances.
  - Config-driven: `lumen.toml` lists MCP server URIs and tool mappings.

- [ ] `lumen-provider-openai` crate (OpenAI-compatible chat/embeddings).
  - Chat completions, structured output, embeddings. Configurable base URL for compatible APIs.

- [ ] `lumen-provider-anthropic` crate (Claude API adapter).
  - Messages API, tool use, streaming. Separate from OpenAI-compatible due to different API shape.

- [ ] `lumen-provider-process` crate (subprocess execution).
  - Spawn, stdin/stdout/stderr, exit code. Respects grant constraints.

### Community Ecosystem

- [ ] Provider hot-reload / dynamic loading.
  - Reload providers without restarting the VM. Useful for development workflows.

- [ ] Provider version negotiation.
  - Schema versioning so providers can evolve without breaking existing programs.

- [ ] Community provider template/scaffold.
  - `lumen new-provider <name>` generates a crate skeleton with `ToolProvider` impl.

- [ ] Package registry (like crates.io, npm).

- [ ] Community templates for common use cases.
  - API client, data pipeline, chatbot, workflow automation.

- [ ] CI/CD integrations.
  - GitHub Actions, GitLab CI, Jenkins plugins.

- [ ] Playground / online editor.
  - In-browser Lumen REPL with WASM compilation.

### Documentation

- [ ] Keep `SPEC.md` implementation-accurate.
  - Revise Section 2.7: say "parsed" not "supported" for type aliases, traits, impls, imports, macros.
  - Add note that `where` constraints are compile-time validation only, not runtime enforcement.
  - Specify lambda/closure semantics (they exist and work despite VISION saying "no closures in v1").
  - Document the intrinsic stdlib (69 intrinsics, major undocumented surface area).

- [ ] Keep `ROADMAP.md` aligned with major direction.

- [ ] Keep this file limited to concrete outstanding tasks.

- [ ] Write tutorial documentation.
  - Getting started guide, language tour, pattern examples.

- [ ] Write API reference documentation.
  - All intrinsics, standard library, runtime APIs.

- [ ] Write architecture documentation.
  - Compiler pipeline, VM design, provider system.

- [ ] Generate API docs from source comments.
  - `lumen doc` command to extract docstrings.

## P4 — Advanced Features

### Novel AI-First Features

- [ ] Record types → JSON Schema compilation.
  - Emit JSON schemas from record definitions for LLM structured output APIs.
  - `lumen emit --schema RecordName` CLI command.
  - Include schema in `ToolCall` opcode requests. Builds on existing `expect schema` opcode.

- [ ] Typed prompt templates.
  - New `prompt` declaration bundling `role` blocks with typed input/output annotations.
  - Compiler verifies interpolated variables exist and have correct types.
  - Output type compiles to JSON schema. Builds on existing `role` block parsing.

- [ ] Cost-aware types with `@budget` enforcement.
  - `cost` annotation on effect rows: `cell foo() -> String / cost[~2000]`.
  - Compiler sums costs through call graph, checks against `@budget` directives.
  - Extends existing `grant { max_tokens }` into type-level budgets.

- [ ] Runtime `where` constraint enforcement with automatic retry.
  - Evaluate `where` predicates on record construction at runtime.
  - When LLM structured output violates constraints, auto-retry with violation feedback.
  - Builds on existing `constraints.rs` compile-time validation.

- [ ] First-class capability grants with attenuation.
  - Grants become values: passable as args, storable in records, attenuable (narrow, never widen).
  - Compiler verifies delegated capabilities never exceed delegator's.
  - Builds on existing `grant` parsing and runtime policy enforcement.

- [ ] Effect-guaranteed deterministic replay.
  - The effect system proves all side effects are declared and traced.
  - Replay mode substitutes recorded responses.

- [ ] Session-typed multi-agent protocols.
  - `protocol` declarations specify valid message sequences between agents.
  - Compiler verifies each agent implements its role.

- [ ] CRDT memory types.
  - `memory SharedState: crdt` with typed CRDT fields (G-Counter, G-Set, LWW-Register).

- [ ] Event-sourced memory.
  - `memory AuditTrail: event_sourced` with typed `event` declarations.

- [ ] Linear resource types.
  - `once` and `consume` type qualifiers for API keys, session handles, context windows.

### Compilation Targets

- [ ] WASM compilation target.
  - Compile Lumen to WebAssembly for browser execution.

- [ ] Native binary compilation.
  - AOT compilation to native executables.

- [ ] JIT compilation.
  - Just-in-time compilation for hot paths.

### Advanced Tooling

- [ ] Incremental compilation.
  - Cache compilation artifacts, recompile only changed modules.

- [ ] Source maps / debug info.
  - Map LIR bytecode back to source for debugging.

- [ ] Stack traces on runtime errors.
  - Capture and format call stack on VM errors.

- [ ] Garbage collection or ownership.
  - Current VM has no memory management strategy for long-running programs.

- [ ] Concurrency model.
  - Thread safety, message passing, shared state.

- [ ] FFI / native interop.
  - Call Rust/C functions from Lumen.

### Language Server Protocol (Full)

- [ ] Implement full LSP 3.17 specification.
  - All diagnostics, code actions, refactorings.

- [ ] Add rename refactoring.

- [ ] Add extract cell refactoring.

- [ ] Add inline cell refactoring.

- [ ] Add organize imports.

- [ ] Add auto-import suggestions.

### Compatibility and Migration

- [ ] Add compatibility tooling (API/symbol diff, semver checks).

- [ ] Expand semantic conformance tests tied to spec sections.

- [ ] Add versioned language spec (Lumen 1.0, 1.1, etc.).

- [ ] Add migration guides for breaking changes.
