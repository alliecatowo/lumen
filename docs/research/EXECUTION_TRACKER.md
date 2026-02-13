# Execution Tracker (Research -> Implementation)

Date: February 13, 2026  
Owner: Track D (implementation alignment)

## Recently Completed (Verified)

- [x] `!=` lowering emits `Eq` + `Not` (`8449533`, `rust/lumen-compiler/src/compiler/lower.rs`)
- [x] VM arithmetic and UTF-8 safety fixes (`f73bc03`, `c709de2`, `rust/lumen-vm/src/vm.rs`)
- [x] LSP capability expansion (`d7a19db`, `rust/lumen-lsp/src/main.rs`)
- [x] MCP provider scaffold + stdio transport (`1a80db2`, `1e6541d`, `rust/lumen-provider-mcp/src/lib.rs`)

## Next 3 Rounds

### Round 1 — Correctness Closure (Compiler + VM)

Checklist:
- [ ] Closure capture/upvalues in lowering and runtime.
  - Files: `rust/lumen-compiler/src/compiler/lower.rs`, `rust/lumen-vm/src/vm.rs`, `rust/lumen-compiler/tests/spec_suite.rs`
- [ ] Fix `if let` / `while let` / tuple destructuring parse paths.
  - Files: `rust/lumen-compiler/src/compiler/parser.rs`, `rust/lumen-compiler/src/compiler/lower.rs`, `rust/lumen-compiler/tests/spec_suite.rs`
- [ ] Add VM register bounds checks and execution fuel ceiling.
  - Files: `rust/lumen-vm/src/vm.rs`, `rust/lumen-vm/tests/e2e.rs`

Acceptance criteria:
1. `cargo test -p lumen-compiler --tests` passes with new regression tests for each bug class above.
2. `cargo test -p lumen-vm` includes explicit `RegisterOOB` and fuel-exhaustion coverage.
3. No P0 item in `tasks.md` remains open for `!=`, arithmetic overflow/div0, UTF-8 slicing, await fuel.

### Round 2 — Determinism + Diagnostics

Checklist:
- [ ] Wire VM `TraceRef`/events to runtime trace store.
  - Files: `rust/lumen-vm/src/vm.rs`, `rust/lumen-runtime/src/trace/store.rs`, `rust/lumen-runtime/src/trace/events.rs`, `rust/lumen-cli/src/main.rs`
- [ ] Parser panic-mode recovery with multi-error reporting.
  - Files: `rust/lumen-compiler/src/compiler/parser.rs`, `rust/lumen-compiler/src/compiler/mod.rs`
- [ ] LSP incremental parsing path for edit-range updates.
  - Files: `rust/lumen-lsp/src/main.rs`, `tree-sitter-lumen/grammar.js`, `tree-sitter-lumen/queries/locals.scm`

Acceptance criteria:
1. One malformed file can report >=3 independent parser diagnostics in one run.
2. Replay check: same trace + same program => identical output and final state hash.
3. LSP median diagnostics latency <100ms for single-line edits on benchmark corpus.

### Round 3 — Package + Provider Reliability

Checklist:
- [ ] Lockfile determinism (`--locked`/frozen behavior) and stable serialization.
  - Files: `rust/lumen-cli/src/pkg.rs`, `rust/lumen-cli/src/lockfile.rs`, `rust/lumen-cli/src/main.rs`
- [ ] MCP provider reliability pass (startup, timeout, error mapping).
  - Files: `rust/lumen-provider-mcp/src/lib.rs`, `rust/lumen-runtime/src/tools.rs`, `rust/lumen-cli/src/config.rs`
- [ ] Async-capable tool dispatch contract.
  - Files: `rust/lumen-runtime/src/tools.rs`, `rust/lumen-runtime/src/lib.rs`, `rust/lumen-vm/src/vm.rs`

Acceptance criteria:
1. `lumen pkg install --locked` fails when lockfile would change and exits non-zero.
2. MCP integration tests cover tool listing + call + timeout + invalid payload paths.
3. Tool invocation concurrency test proves one slow provider call does not block unrelated runnable work.
