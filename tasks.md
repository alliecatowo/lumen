# Lumen Tasks

This file tracks outstanding implementation work.
Completed work should be removed from this list and reflected in docs/changelog.

---

## Research Alignment Snapshot (February 2026)

Recently completed (verified in code and commit history):

- [x] `!=` lowering now emits `Eq` + `Not` (`8449533`, `rust/lumen-compiler/src/compiler/lower.rs`).
- [x] Core VM arithmetic safety landed (`f73bc03`, `c709de2`, `rust/lumen-vm/src/vm.rs`): checked overflow, div/mod by zero errors, and shift-range guards.
- [x] VM UTF-8/encoding safety fixes landed (`f73bc03`, `rust/lumen-vm/src/vm.rs`): UTF-8-safe string slicing, odd-length `hex_decode` guard, byte-wise `url_encode`.
- [x] LSP feature surface expanded (`d7a19db`, `rust/lumen-lsp/src/main.rs`): document/workspace symbols, semantic tokens, signature help, inlay hints, code actions, folding, references.
- [x] All example files and test suite updated for correct syntax (Feb 2026): Fixed record construction to use parentheses, set literals to use curly braces, import syntax to use colon separator. Removed unused parser error recovery infrastructure.

Execution tracker for next three rounds: `docs/research/EXECUTION_TRACKER.md`.

---

## P0 — Critical Bugs and Safety Issues (Blocking V1 Release)

### Arithmetic Safety

- [x] **Fix `!=` operator**: `BinOp::NotEq` now lowers to `OpCode::Eq` followed by `OpCode::Not`.
  - **File**: `rust/lumen-compiler/src/compiler/lower.rs:1465`
  - **Fix**: Add `Not` instruction after `Eq` when op is `NotEq`.

- [x] **Fix integer arithmetic overflow**: Integer Add/Sub/Mul/Pow now use checked semantics and error on overflow.
  - **File**: `rust/lumen-vm/src/vm.rs` Add/Sub/Mul/Pow handlers
  - **Fix**: Use `checked_*` or `wrapping_*` with defined semantics.

- [x] **Fix integer division/modulo by zero**: Integer `Div`/`Mod` now return `VmError::DivisionByZero`.
  - **File**: `rust/lumen-vm/src/vm.rs:911-928`
  - **Fix**: Return `VmError::DivisionByZero` on zero divisor.

- [x] **Fix bit shift panic on negative amounts**: VM now validates shift amounts and errors if out of range.
  - **File**: `rust/lumen-vm/src/vm.rs:1018,1026`
  - **Fix**: Clamp or error on out-of-range shift amounts.

### String/Bytes Safety

- [x] **Fix string slice panic on non-ASCII**: String slicing now uses character boundaries.
  - **File**: `rust/lumen-vm/src/vm.rs:3157-3169`
  - **Fix**: Use `char_indices()` for character-based slicing.

- [x] **Fix `hex_decode` panic on odd-length input**.
  - **File**: `rust/lumen-vm/src/vm.rs:2696-2703`
  - **Fix**: Guard against odd-length strings.

- [x] **Fix `url_encode` for multi-byte UTF-8**: Encoding now iterates UTF-8 bytes.
  - **File**: `rust/lumen-vm/src/vm.rs:2706-2718`
  - **Fix**: Iterate bytes, not chars.

### Control Flow Safety

- [x] **Fix `Await` potential infinite loop**: Await retries are now fuel-bounded and return an error on exhaustion.
  - **File**: `rust/lumen-vm/src/vm.rs:1411-1425`
  - **Fix**: Add fuel/timeout mechanism.

- [ ] **Add instruction fuel/step counter**: Infinite loop protection.
  - **Files**: `rust/lumen-vm/src/vm.rs`
  - **Fix**: Add configurable max instruction count per execution.

### Comparison/Equality Safety

- [ ] **Fix NaN handling**: `Eq` for `Value::Float(NaN)` violates reflexivity; `Ord` treats NaN as Equal.
  - **File**: `rust/lumen-vm/src/values.rs:314,362`
  - **Fix**: Define consistent NaN semantics (IEEE 754 unordered).

- [ ] **Fix interned string comparison**: All interned strings compare as empty string in `Ord`.
  - **File**: `rust/lumen-vm/src/values.rs:366-377`
  - **Fix**: `Ord` impl needs `StringTable` access or owned-string fallback.

### VM Runtime Safety

- [ ] **Add register bounds checking**: `RegisterOOB` error variant exists but is never used.
  - **File**: `rust/lumen-vm/src/vm.rs`
  - **Fix**: Add bounds checks or debug assertions in dispatch loop.

- [ ] **Replace `unwrap()` calls in VM**: Proper error propagation.
  - **Files**: `rust/lumen-vm/src/vm.rs:658,1150,1275,1297,1306`
  - **Fix**: Convert to `ok_or(VmError::...)`.

---

## P1 — Core Language Features Missing or Broken

### Type System (Critical Gap)

- [ ] **Implement generic type parameter checking**: Generics parsed but never instantiated.
  - **Files**: `rust/lumen-compiler/src/compiler/typecheck.rs`
  - **Impact**: Type-safe collections impossible, major feature gap vs Rust/TypeScript/Go/Gleam
  - **Path**: Generic instantiation → bounded generics → trait bounds
  - **Competitive**: Gap 1 in COMPETITIVE_ANALYSIS.md — blocking ecosystem growth

- [ ] **Implement trait conformance checking and method dispatch**.
  - **File**: `rust/lumen-compiler/src/compiler/resolve.rs:567-586`
  - **Status**: Traits/impls stored, never verified or dispatched.
  - **Impact**: Cannot use trait-based polymorphism.

- [x] **Implement type alias resolution**: Aliases are stored and can be expanded.
  - **File**: `rust/lumen-compiler/src/compiler/typecheck.rs:190-191`
  - **Status**: COMPLETED — type aliases resolve correctly.

### Lowering Bugs

- [ ] **Fix closure/lambda upvalue capture**: Lambda bodies get fresh `RegAlloc` with no enclosing scope.
  - **File**: `rust/lumen-compiler/src/compiler/lower.rs:1833-1892`
  - **Fix**: Build upvalue list, emit `GetUpval`/`SetUpval` for captured variables.
  - **Related**: VM `Closure` opcode always creates zero captures (`rust/lumen-vm/src/vm.rs:1271-1293` — `cap_count` always 0).

- [ ] **Fix set/map comprehension lowering**: `kind` field ignored, always emits `NewList`.
  - **File**: `rust/lumen-compiler/src/compiler/lower.rs:2076-2151`
  - **Fix**: Dispatch on `ComprehensionKind` to emit `NewSet`/`NewMap`.

- [ ] **Fix `if let` / `while let`**: Parser discards pattern, replaces with `BoolLit(true)`.
  - **File**: `rust/lumen-compiler/src/compiler/parser.rs:1056-1060, 1457-1460`
  - **Fix**: Parse the binding pattern and lower it properly.
  - **Note**: This is listed in MEMORY.md as already fixed via AST desugaring — verify completeness.

- [ ] **Fix `for` loop tuple destructuring**: Discards all variables except the first.
  - **File**: `rust/lumen-compiler/src/compiler/parser.rs:1128-1137`
  - **Fix**: Capture all identifiers in the destructuring pattern.

- [ ] **Fix expression-position `match`/`if`/`loop`/`try`**: Returns placeholder `Ident("match_expr")` etc.
  - **Status**: Already partially fixed with `MatchExpr` and `BlockExpr` AST variants — verify completeness.
  - **Files**: `rust/lumen-compiler/src/compiler/ast.rs`, `parser.rs`, `resolve.rs`, `typecheck.rs`, `lower.rs`

### Parser/Resolver Correctness

- [ ] **Add duplicate definition detection**: For records, enums, cells, processes, effects, handlers.
  - **File**: `rust/lumen-compiler/src/compiler/resolve.rs`
  - **Status**: Currently only agents check for duplicates.

- [ ] **Remove hardcoded application-specific type names from resolver builtins**.
  - **File**: `rust/lumen-compiler/src/compiler/resolve.rs:244-285`
  - **Issue**: `Invoice`, `MyRecord`, `LineItem`, `Pair`, `A/B/C` etc. are not builtins.

- [ ] **Clean up `is_doc_placeholder_var` whitelist in typechecker**.
  - **File**: `rust/lumen-compiler/src/compiler/typecheck.rs:57-153`
  - **Issue**: ~100 hardcoded variable names bypass type checking.
  - **Fix**: Replace with proper scoping.

### Runtime Infrastructure

- [ ] **Wire trace system into VM**: `TraceRef` opcode currently creates dummy values.
  - **File**: `rust/lumen-vm/src/vm.rs`
  - **Status**: VM has no `TraceStore` field. Trace infrastructure exists in runtime but is disconnected.

- [ ] **Implement disk cache loading on startup**: Currently memory-only despite writing to disk.
  - **File**: `rust/lumen-runtime/src/cache.rs:34-36`
  - **Issue**: `get()` only checks memory, ignores files from previous runs.

- [ ] **Make tool dispatch async-capable**.
  - **File**: `rust/lumen-runtime/src/tools.rs:34`
  - **Issue**: Trait is synchronous, blocks entire VM.

### Unimplemented Language Features

- [ ] **Implement record field default values at construction time**.
  - **Status**: `FieldDef.default_value` is parsed and stored but never applied.

- [ ] **Implement runtime `where` constraint evaluation on record construction**.
  - **Status**: Constraints validated for form in `constraints.rs` but never enforced at runtime.

- [ ] **Implement `let` destructuring patterns properly**.
  - **File**: `rust/lumen-compiler/src/compiler/parser.rs:1047`
  - **Issue**: `pattern` field always `None`, uses `__tuple` workarounds.

- [ ] **Implement spread operator semantics**.
  - **File**: `rust/lumen-compiler/src/compiler/lower.rs:2039`
  - **Issue**: `SpreadExpr` just unwraps inner, discards spread.

- [ ] **Implement `async cell` as semantically distinct from `cell`**.
  - **Status**: Parsed but treated identically to regular cells.

- [ ] **Wire intrinsic name mapping for unmapped builtins**: 51 of 69 unreachable from source.
  - **File**: `rust/lumen-compiler/src/compiler/lower.rs:1670-1686`
  - **Missing**: `sort`, `reverse`, `map`, `filter`, `reduce`, `trim`, `upper`, `lower`, `replace`, `find`, `zip`, `enumerate`, `flatten`, `unique`, `take`, `drop`, `first`, `last`, `is_empty`, `chars`, `starts_with`, `ends_with`, `index_of`, `round`, `ceil`, `floor`, `sqrt`, `pow`, `log`, `sin`, `cos`, `clamp`, `clone`, `debug`, etc.

### Parser Error Recovery (Competitive Gap 5)

- [ ] **Add error recovery in parser for partial parses**.
  - **Status**: Currently fails fast on first syntax error; should continue parsing to collect multiple errors.
  - **Competitive**: Gap 5 in COMPETITIVE_ANALYSIS.md — behind Rust/TypeScript multi-error reporting.

- [ ] **Add panic mode recovery for common syntax errors**.
  - **Examples**: Missing `end`, unmatched parens, incomplete expressions.

---

## P2 — Standard Library Gaps

### Missing Intrinsics

The VM implements 69 intrinsics but only ~18 are callable from source code. The following intrinsics exist in the VM but have no compiler mapping:

**String operations** (exist but unmapped):
- `trim`, `upper`, `lower`, `replace`, `find`, `starts_with`, `ends_with`, `index_of`, `chars`

**Collection operations** (exist but unmapped):
- `sort`, `reverse`, `map`, `filter`, `reduce`, `flatten`, `unique`, `zip`, `enumerate`
- `take`, `drop`, `first`, `last`, `is_empty`

**Math operations** (exist but unmapped):
- `round`, `ceil`, `floor`, `sqrt`, `pow`, `log`, `sin`, `cos`, `clamp`

**Utility operations** (exist but unmapped):
- `clone`, `debug`

**Action items**:

- [ ] **Add compiler mappings for all 51 unmapped intrinsics**.
  - **File**: `rust/lumen-compiler/src/compiler/lower.rs:1670-1686`
  - **Impact**: Major stdlib surface area increase.

- [ ] **Document all intrinsics in SPEC.md**.
  - **Current**: SPEC.md lists no intrinsic documentation — major undocumented surface area.

- [ ] **Expand stdlib with higher-order functions**.
  - Examples: `filter_map`, `fold_right`, `partition`, `group_by`.

- [ ] **Add JSON manipulation intrinsics**.
  - Examples: `json_parse`, `json_stringify`, `json_get`, `json_set`.

- [ ] **Add date/time intrinsics** (non-deterministic by default).
  - Examples: `timestamp`, `format_time`, `parse_time`, `time_add`, `time_diff`.

### Process Runtimes

- [ ] **Implement guardrail and eval as executable runtime structures**: Currently stubs.
  - **Status**: Parsed and stored but no runtime semantics.

- [ ] **Complete orchestration semantics beyond pipeline stage chains**.
  - Examples: Coordinator-worker patterns, deterministic scheduling/merge.

- [ ] **Add machine transition trace events and replay hooks**.
  - **Impact**: Enable deterministic replay for state machines.

- [ ] **Implement real effect handler semantics with continuations**.
  - Support scoped handling (`with <handler> in ...`) with interception/resume.

---

## P3 — Tooling Improvements

### LSP Server (Competitive Gap 2)

- [x] **Basic LSP server exists** (`lumen-lsp`)
  - Implements diagnostics, go-to-definition, hover, completion.

- [ ] **Add incremental parsing for LSP performance**.
  - **File**: `rust/lumen-lsp/src/main.rs`
  - **Issue**: Currently re-parses entire file on every keystroke.
  - **Competitive**: Gap 2 in COMPETITIVE_ANALYSIS.md — 10x slower than TypeScript LSP.
  - **Target**: <100ms diagnostics after typing (match TypeScript LSP).

- [x] **Add semantic tokens for syntax highlighting**.
  - **Status**: Implemented in `rust/lumen-lsp/src/main.rs` (`textDocument/semanticTokens/full`).

- [x] **Add code actions** (quick fixes, refactorings).
  - Examples: Auto-import, extract cell, rename symbol.

- [x] **Add workspace symbol search**.
  - **Status**: Implemented (`workspace/symbol`) across indexed open documents.

- [x] **Add document outline/symbols**.
  - **Purpose**: Show cells, records, enums in editor sidebar.

- [x] **Add signature help for function calls**.

- [x] **Add inlay hints for type annotations**.

### Formatter

- [x] **`lumen fmt` formatter exists**
  - **File**: `rust/lumen-cli/src/fmt.rs` — 1259 lines, implements formatting.

- [ ] **Add configuration options for formatter**.
  - Examples: Indent size, line width, brace style.

- [ ] **Add format-on-save integration for LSP**.

### REPL

- [x] **`lumen repl` command exists**
  - **File**: `rust/lumen-cli/src/repl.rs` — 265 lines, basic REPL.

- [ ] **Add multi-line input support**.
  - **Status**: Currently single-line only.

- [ ] **Add REPL history persistence**.

- [ ] **Add tab completion in REPL**.

- [ ] **Add `:type` command for type inspection**.

- [ ] **Add `:doc` command for documentation lookup**.

### Package Manager (Competitive Gap 4)

- [x] **`lumen pkg` command exists**
  - **File**: `rust/lumen-cli/src/pkg.rs`
  - **Status**: `init`, `build`, `check`, `add`, `remove`, `list`, `install`, `update`, `search`.

- [ ] **Add `lumen pkg test` command**.

- [ ] **Add `lumen pkg publish` command**.
  - **Competitive**: Gap 4 in COMPETITIVE_ANALYSIS.md — no registry like crates.io/npm.

- [x] **Add dependency resolution and lockfile for path dependencies**.
  - **Status**: `install`/`update` resolve path deps and write `lumen.lock` entries with `path+...` sources.

- [ ] **Add registry-backed dependency resolution and lockfile metadata**.
  - **Gap**: no registry index fetch, semver resolution, download, or checksum population in lockfile.

- [ ] **Add package registry support**.
  - **Target**: `lumen pkg add github-client@1.0` downloads and integrates package.

- [ ] **Use parsed version constraints for real registry installs** (SemVer resolution).

### Test Runner (Competitive Gap 6)

- [ ] **Add `test` declaration form** (like `cell` but for tests).
  - **Competitive**: Gap 6 in COMPETITIVE_ANALYSIS.md — behind `cargo test`, `go test`, `gleam test`.

- [ ] **Implement `lumen test` command**: Discovers and runs test cells.

- [ ] **Add assertion intrinsics**: `assert_eq`, `assert_ne`, `assert_ok`, `assert_err`.

- [ ] **Support test filtering** (by name/path) and parallel execution.

- [ ] **Target**: `lumen test` runs all tests with colored pass/fail output.

### Documentation (Competitive Gap 7)

- [x] **Add `lumen doc` command**: Generate documentation.
  - **Status**: Command exists with markdown/json output for `.lm.md` inputs.

- [ ] **Expand `lumen doc` to support `.lm` inputs and richer output UX**.
  - **Competitive**: Gap 7 in COMPETITIVE_ANALYSIS.md — still behind rustdoc/godoc/TSDoc.
  - **Target**: `lumen doc --open` generates and opens navigable API docs.

- [ ] **Generate HTML/markdown with symbol links**.

- [ ] **Include examples from doc comments**.

- [ ] **Publish to static site** (like docs.rs for Rust).

### CLI Improvements

- [ ] **Add `lumen bench` command for benchmarking**.

- [ ] **Add `lumen profile` command for profiling**.

- [ ] **Add `lumen upgrade` command for version migration**.

- [ ] **Add `lumen new` command for project scaffolding**.

- [ ] **Add JSON output mode for machine-readable diagnostics**.

### Debugging and Profiling

- [ ] **Build debugger with breakpoints and step-through**.
  - **Status**: VM has `DebugEvent` infrastructure; need frontend.

- [ ] **Add execution profiler for performance analysis**.
  - **Features**: Instruction counts, hot paths, allocation tracking.

- [ ] **Add memory profiler for heap analysis**.

- [ ] **Add flamegraph generation**.

---

## P4 — Nice-to-Have / Future

### Provider Architecture

- [x] **Define `ToolProvider` trait in `lumen-runtime/src/tools.rs`**.
- [x] **Create `ProviderRegistry` struct with `register`, `get`, `list` methods**.
- [x] **Wire `ProviderRegistry` into VM (`set_provider_registry` method)**.
- [x] **Create `NullProvider` stub for unregistered tools**.
- [x] **Add provider registry tests** (20 tests).
- [x] **Add VM integration tests** (2 tests).
- [x] **Parse `lumen.toml` config file** (`lumen-cli/src/config.rs` — 335 lines).
- [x] **Create `LumenConfig` struct**.
- [x] **Wire config loading into CLI startup**.
- [x] **Add `lumen init` command** to generate default `lumen.toml`.

- [ ] **Remove heuristic `effect_from_tool()` substring matching**.
  - Replace with provider-declared effect mappings.

- [ ] **Generalize `max_tokens` to generic `max_*` range constraints**.
  - Grant constraints should support arbitrary numeric limits, not just `max_tokens`.

- [ ] **Review `NONDETERMINISTIC_EFFECTS` list**.
  - Ensure nondeterministic classification comes from provider declarations, not hardcoded list.

- [ ] **Audit compiler for hardcoded provider-specific knowledge**.
  - Ensure no tool names, API shapes, or provider assumptions leak into compiler crates.

- [ ] **Ensure effect kinds come from provider declarations, not hardcoded lists**.
  - Providers declare their effects via `ToolProvider::effects()`.

- [ ] **Desugar `role` blocks to standard tool call shapes**.
  - Conversation structure maps to provider-agnostic tool invocations.

### First-Party Providers

- [x] **`lumen-provider-http` crate** (reqwest-based HTTP client).
  - **Status**: Exists but needs feature completion and testing.

- [x] **`lumen-provider-fs` crate** (filesystem operations).
  - **Status**: Exists but needs feature completion and testing.

- [x] **`lumen-provider-json` crate** (JSON operations).
  - **Status**: Exists but needs feature completion and testing.

- [ ] **`lumen-provider-mcp` crate** (MCP server bridge — universal tool adapter).
  - **Competitive**: Gap 3 in COMPETITIVE_ANALYSIS.md — P0 ecosystem blocker.
  - **Status**: Provider crate + stdio transport exist; end-to-end external server reliability and one-command UX remain incomplete.
  - **Target**: `lumen run example.lm.md` with MCP GitHub server works in one command.

- [ ] **`lumen-provider-openai` crate** (OpenAI-compatible chat/embeddings).
  - Chat completions, structured output, embeddings. Configurable base URL for compatible APIs.

- [ ] **`lumen-provider-anthropic` crate** (Claude API adapter).
  - Messages API, tool use, streaming. Separate from OpenAI-compatible due to different API shape.

- [ ] **`lumen-provider-process` crate** (subprocess execution).
  - Spawn, stdin/stdout/stderr, exit code. Respects grant constraints.

### Community Ecosystem

- [ ] **Provider hot-reload / dynamic loading**.
  - Reload providers without restarting the VM. Useful for development workflows.

- [ ] **Provider version negotiation**.
  - Schema versioning so providers can evolve without breaking existing programs.

- [ ] **Community provider template/scaffold**.
  - `lumen new-provider <name>` generates a crate skeleton with `ToolProvider` impl.

- [ ] **Community templates for common use cases**.
  - API client, data pipeline, chatbot, workflow automation.

- [ ] **CI/CD integrations**.
  - GitHub Actions, GitLab CI, Jenkins plugins.

- [ ] **Playground / online editor**.
  - In-browser Lumen REPL with WASM compilation.

### Advanced Tooling

- [ ] **Incremental compilation**.
  - Cache compilation artifacts, recompile only changed modules.

- [ ] **Source maps / debug info**.
  - Map LIR bytecode back to source for debugging.

- [ ] **Stack traces on runtime errors**.
  - Capture and format call stack on VM errors.

- [ ] **Garbage collection or ownership** (Competitive Gap 9).
  - **Options**: Tracing GC (mark-and-sweep), ownership/borrow checker (Rust-style), reference counting.
  - **Target**: Long-running process (24hr+) maintains stable memory footprint.

- [ ] **Concurrency model**.
  - Thread safety, message passing, shared state.

- [ ] **FFI / native interop**.
  - Call Rust/C functions from Lumen.

### Compilation Targets (Competitive Gap 8)

- [ ] **WASM compilation target**.
  - **Competitive**: Gap 8 in COMPETITIVE_ANALYSIS.md — behind Rust/Gleam/Go WASM support.
  - **Target**: Lumen REPL running in browser at `play.lumenlang.dev`.

- [ ] **Native binary compilation**.
  - AOT compilation to native executables.

- [ ] **JIT compilation**.
  - Just-in-time compilation for hot paths.

### Language Server Protocol (Full)

- [ ] **Implement full LSP 3.17 specification**.
  - All diagnostics, code actions, refactorings.

- [ ] **Add rename refactoring**.

- [ ] **Add extract cell refactoring**.

- [ ] **Add inline cell refactoring**.

- [ ] **Add organize imports**.

- [ ] **Add auto-import suggestions**.

### Compatibility and Migration

- [ ] **Add compatibility tooling** (API/symbol diff, semver checks).

- [ ] **Expand semantic conformance tests tied to spec sections**.

- [ ] **Add versioned language spec** (Lumen 1.0, 1.1, etc.).

- [ ] **Add migration guides for breaking changes**.

### Novel AI-First Features (V2 Differentiators)

These build on Lumen's unique strengths to create capabilities no other language provides.

- [ ] **Record types → JSON Schema compilation**.
  - Emit JSON schemas from record definitions for LLM structured output APIs.
  - `lumen emit --schema RecordName` CLI command.
  - Include schema in `ToolCall` opcode requests. Builds on existing `expect schema` opcode.

- [ ] **Typed prompt templates**.
  - New `prompt` declaration bundling `role` blocks with typed input/output annotations.
  - Compiler verifies interpolated variables exist and have correct types.
  - Output type compiles to JSON schema. Builds on existing `role` block parsing.

- [ ] **Cost-aware types with `@budget` enforcement**.
  - `cost` annotation on effect rows: `cell foo() -> String / cost[~2000]`.
  - Compiler sums costs through call graph, checks against `@budget` directives.
  - Extends existing `grant { max_tokens }` into type-level budgets.

- [ ] **Runtime `where` constraint enforcement with automatic retry**.
  - Evaluate `where` predicates on record construction at runtime.
  - When LLM structured output violates constraints, auto-retry with violation feedback.
  - Builds on existing `constraints.rs` compile-time validation.

- [ ] **First-class capability grants with attenuation**.
  - Grants become values: passable as args, storable in records, attenuable (narrow, never widen).
  - Compiler verifies delegated capabilities never exceed delegator's.
  - Builds on existing `grant` parsing and runtime policy enforcement.

- [ ] **Effect-guaranteed deterministic replay**.
  - The effect system proves all side effects are declared and traced.
  - Replay mode substitutes recorded responses.

- [ ] **Session-typed multi-agent protocols**.
  - `protocol` declarations specify valid message sequences between agents.
  - Compiler verifies each agent implements its role.

- [ ] **CRDT memory types**.
  - `memory SharedState: crdt` with typed CRDT fields (G-Counter, G-Set, LWW-Register).

- [ ] **Event-sourced memory**.
  - `memory AuditTrail: event_sourced` with typed `event` declarations.

- [ ] **Linear resource types**.
  - `once` and `consume` type qualifiers for API keys, session handles, context windows.

---

## Documentation Gaps

- [ ] **Keep `SPEC.md` implementation-accurate**.
  - Revise Section 2.7: say "parsed" not "supported" for type aliases, traits, impls, imports, macros.
  - Add note that `where` constraints are compile-time validation only, not runtime enforcement.
  - Specify lambda/closure semantics (they exist and work despite VISION saying "no closures in v1").
  - Document the intrinsic stdlib (69 intrinsics, major undocumented surface area).

- [ ] **Keep `ROADMAP.md` aligned with major direction**.

- [ ] **Keep this file limited to concrete outstanding tasks**.

- [ ] **Write tutorial documentation**.
  - Getting started guide, language tour, pattern examples.

- [ ] **Write API reference documentation**.
  - All intrinsics, standard library, runtime APIs.

- [ ] **Write architecture documentation**.
  - Compiler pipeline, VM design, provider system.

---

## Test Coverage Gaps

- [ ] **Add regression tests for known bugs**: Signed jumps, match register clobber, Type::Any BinOp.

- [ ] **Add example files as automated integration tests**: Compile + execute the working examples.

- [ ] **Expand typechecker tests**: BinOp inference, record fields, call args, union returns.
  - **Current**: Currently minimal coverage.

- [ ] **Expand lowering tests**: Control flow, match, closures, string interp, records.

- [ ] **Add constraint validation tests**.

- [ ] **Add VM error path tests**: Stack overflow, undefined cell, register OOB, halt.

- [ ] **Add end-to-end tests**: While loop, for loop, string interp, closures, null coalesce, float arithmetic.

---

## Competitive Gap Summary (from COMPETITIVE_ANALYSIS.md)

### Must-Fix for V1

1. **Gap 1: Generic Type Instantiation** (P0) — Behind Rust/TypeScript/Go/Gleam
   - Blocks type-safe collections, ecosystem growth.

2. **Gap 3: MCP Bridge** (P0) — Ecosystem blocker
   - Basic bridge exists; remaining gap is production-ready external server reliability and UX.

3. **Gap 5: Parser Error Recovery** (P1) — Behind Rust/TypeScript
   - Fails on first error, slow iteration.

### Should-Fix for V1

4. **Gap 2: LSP Incremental Parsing** (P1) — 10x slower than TypeScript
   - Unusable for files >1000 lines.

5. **Gap 6: Test Runner** (P1) — Behind cargo/go/gleam
   - No built-in test command.

6. **Gap 7: Documentation Generation** (P2) — Behind rustdoc/godoc/TSDoc
   - No API reference, hard to discover stdlib.

### Post-V1

7. **Gap 4: Package Registry** (P2) — Behind crates.io/npm
   - Cannot share/reuse packages.

8. **Gap 8: WASM Compilation** (P3) — Behind Rust/Gleam/Go
   - Cannot run in browser.

9. **Gap 9: Memory Management** (P3) — No GC/ownership
   - Production deployments leak memory.

---

## Example Status

From test runs and manual inspection:

**Compile + run successfully** (12/13):
- fibonacci.lm.md, hello.lm.md, calculator.lm.md, sorting.lm.md, etc.

**Known issues**:
- role_interpolation.lm.md — parse issue with role blocks

**Action items**:
- [ ] Fix role_interpolation.lm.md parsing.
- [ ] Add all working examples as e2e tests.

---

## Notes

- **Current test count**: ~454 tests passing, 13 ignored
  - 206 compiler tests (107 lib + 83 spec_suite + 14 examples + 1 sweep + 1 raw_format)
  - 22 runtime tests
  - 156 VM tests (81 lib + 75 e2e)
  - 7 CLI config tests
  - Plus provider and CLI tests

- **Critical memory from MEMORY.md**:
  - Use `sax`/`sax_val` for jumps, never `ax`/`ax_val`
  - Allocate temp register for Eq result in match statements
  - Check for `Type::Any` first in BinOp type inference
  - Captures allocated first (r0, r1...), then params after
  - Lambda syntax: `fn(params) -> ReturnType => expr` (NOT `|x| ...` syntax)
  - `result` is a keyword (for `result[T, E]` type)
  - Record construction uses parentheses: `RecordName(field: value)` NOT curly braces
  - Set literals use curly braces: `{1, 2, 3}` NOT `set[1, 2, 3]`
  - Import syntax uses colon: `import module: symbol` NOT `import module.{symbol}`

- **Effect system is complete and working** (Lumen's killer feature).
- **Process runtimes (memory, machine, pipeline) operational**.
- **Rich diagnostics implemented** (Rust-quality error messages).
- **Provider architecture fully wired** (config parsing, registry, VM integration).
