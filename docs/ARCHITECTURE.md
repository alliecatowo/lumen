# Architecture

## High-Level Components

- `lumen-cli`: user-facing entrypoint.
- `lumen-compiler`: front-end and lowering pipeline.
- `lumen-vm`: runtime for executing LIR.
- `lumen-runtime`: trace and tool runtime utilities.

## Compiler Pipeline

1. Markdown extraction
- Lumen source is typically embedded in markdown code fences.

2. Lexing
- Tokenizes source into the parser token stream.

3. Parsing
- Produces AST (`Program`, `Item`, `Stmt`, `Expr`, `Pattern`).

4. Name resolution
- Builds symbol table for types/cells/tools/agents/process declarations.
- Performs effect inference and strict effect diagnostics.

5. Typechecking
- Checks types and pattern compatibility.
- Strict mode is default; doc mode is explicit.

6. Lowering
- Converts AST to LIR module (cells, instructions, constants, metadata).

## Runtime Architecture

- Register-based VM executes LIR instructions.
- Runtime values include scalar and structured types plus closures, trace refs, and futures.
- Tool calls dispatch through optional runtime tool dispatcher.
- Process declarations (`memory`, `machine`, etc.) lower to constructor-backed runtime objects.

## Testing Strategy

- Unit tests for parser/resolver/typechecker/lowerer/vm.
- Markdown sweep test compiles all Lumen code blocks in `SPEC.md`.
- Runtime tests validate behavior for patterns, process runtimes, and VM operations.

---

## v0.5.0 New Subsystems

### Verification Subsystem

- SMT solver abstraction layer (`rust/lumen-compiler/src/verify/`) with pluggable backends: Z3, CVC5, and a built-in bitwise solver for simple cases
- Counter-example generation — when verification fails, the solver produces concrete inputs that violate the assertion
- Proof hints — `@hint` annotations let authors guide the solver toward faster convergence on inductive proofs
- Array bounds propagation — the constraint system tracks index ranges through loops and slices, eliminating redundant bounds checks
- Parity checklist — compiler-enforced checklist ensuring every public cell has at least one verified property before publishing

### Advanced Type System

- **Active patterns** — user-defined pattern decompositions, desugared during resolution into match + extractor calls
- **GADTs with type refinement** — variant payloads refine the type parameter in match arms; implemented in `typecheck.rs` via substitution maps
- **Hygienic macros** — `macro` declarations expand at parse time with fresh gensyms to prevent capture; expansion in `compiler/macro_expand.rs`
- **`Prob<T>`** — probability-typed wrapper for stochastic tool outputs; carries a confidence score alongside the value
- **Session types** — channel endpoints carry a protocol type that advances on each send/receive; checked statically in the typechecker
- **Typestate** — records can declare state-indexed methods; the compiler tracks the current state through linear use analysis

### Runtime Extensions

- **Schema drift detection** — at tool-call boundaries, the runtime compares the actual JSON shape against the declared schema and emits structured warnings on mismatch (`rust/lumen-runtime/src/drift.rs`)
- **Execution graph visualization** — `--trace-dir` now also emits a DAG of cell calls, future spawns, and effect invocations; viewable via `lumen trace show --graph <run-id>`
- **Retry with backoff** — `RetryPolicy` in `rust/lumen-runtime/src/tools.rs` supports exponential backoff, jitter, and per-error-class strategies (rate-limit vs. transient failure)

### Standard Library Modules

Located under `rust/lumen-runtime/src/stdlib/` (runtime builtins) and future `std/` source packages:

- **crypto** — SHA-256, BLAKE3, HMAC-SHA256; constant-time comparison; key derivation via HKDF
- **http** — request builder (method/headers/body), response type, simple router for handler dispatch
- **fs_async** — async file read/write/stat/glob using the VM's future model; sandboxed by capability grants
- **net** — IP address parsing, TCP socket open/read/write, DNS resolution; all effect-tracked (`/ {net}`)

### JIT Compilation

- **Cranelift JIT** (`rust/lumen-vm/src/jit/cranelift.rs`) — hot-loop detection triggers compilation of LIR cells to native code via `cranelift-jit` `JITModule`; falls back to interpreter on unsupported opcodes
- **OrcJIT engine** (`rust/lumen-vm/src/jit/orc.rs`) — LLVM OrcJIT v2 integration for ahead-of-time and lazy compilation; manages module lifetimes and symbol resolution across compiled cells

### Concurrency Model

- **M:N work-stealing scheduler** (`rust/lumen-vm/src/scheduler.rs`) — N OS threads each run a local deque of lightweight tasks; idle threads steal from peers
- **Channels** — typed bounded/unbounded MPSC channels; session-typed variant enforces protocol ordering
- **Actors** — each actor is a single-threaded mailbox consumer; supervised restart on panic
- **Supervisors** — one-for-one and one-for-all restart strategies; configurable backoff and max-restart limits
- **Nurseries** — structured concurrency scope; all child tasks must complete (or be cancelled) before the nursery exits
- **Deterministic mode** — `@deterministic true` forces FIFO scheduling (no stealing), seeded RNG, and timestamp stubs for reproducible execution

### Durability

- **Checkpoint/restore** — `rust/lumen-vm/src/checkpoint.rs` serializes the full VM state (registers, call stack, heap) to a versioned binary format; restore resumes execution from the snapshot
- **Replay** — trace logs recorded with `--trace-dir` can be replayed deterministically, re-executing tool calls from cached results
- **Time-travel debug** — `lumen trace step <run-id> [--back N]` walks backward through recorded execution states
- **Versioned state** — process runtimes (`memory`, `machine`) maintain an append-only log of state transitions; any prior version is retrievable by sequence number

### Package Ecosystem

- **Binary caching** — compiled LIR modules cached by content hash in `~/.lumen/cache/`; `lumen cache clear` evicts all entries
- **Workspace resolver** (`rust/lumen-cli/src/workspace.rs`) — multi-package workspaces with shared dependency resolution and cross-package imports
- **Transparency log** — append-only Merkle log of published package hashes; clients verify inclusion proofs before install
- **Registry client** (`rust/lumen-cli/src/registry.rs`) — authenticated publish/fetch against the Lumen package registry; TUF-secured metadata
- **Service templates** — `lumen pkg init --template <name>` scaffolds common patterns (API agent, pipeline, tool provider)

### Developer Tools

- **LSP semantic search** — `lumen-lsp` indexes all symbols and supports workspace-wide go-to-definition, find-references, and rename across multi-file projects
- **CI configurations** — reference configs for Miri (undefined behavior checks), coverage (llvm-cov), and sanitizers (AddressSanitizer, ThreadSanitizer) in `ci/`
- **Testing helpers** — `@test` annotation on cells; `assert_eq`, `assert_err`, `assert_matches` builtins; snapshot testing for LIR output
- **Docs-as-tests** — fenced Lumen blocks in `.lm.md` files are compiled and optionally executed during `cargo test` via the markdown sweep harness

### Security

- **Capability sandbox** — each cell runs with only the grants explicitly declared; undeclared tool calls are rejected at compile time (resolve phase) and enforced at runtime
- **Tool policy enforcement** — `validate_tool_policy()` in `rust/lumen-runtime/src/tools.rs` checks grant constraints (domain patterns, timeout, max tokens) before every tool dispatch
- **TUF metadata verification** (`rust/lumen-cli/src/tuf.rs`) — four-role key hierarchy (root → targets → snapshot → timestamp), threshold signing, rollback detection, and expiration enforcement for package metadata
