# Lumen Implementation Tasks

Single source of truth for the transition from interpreted scripting language (v0.4.x) to a verified, systems-level AI orchestration language. Tone: strictly technical. All tasks assume the invariant: **all existing tests pass, the workspace compiles with zero warnings, zero errors.**

---

## 1. Deficiency and problem statement checklist (baseline)

Use this list to verify whether a given deficiency is still present. Mark items RESOLVED when the corresponding implementation work is complete and validated.

| ID | Deficiency | Rationale | Status |
|----|------------|-----------|--------|
| D01 | **Memory: Rc&lt;T&gt; only** | Reference counting is not thread-safe; prevents true parallelism. No `Arc` or GC. | OPEN |
| D02 | **Memory: No cycle collection** | Simple reference counting cannot reclaim cycles (e.g. doubly linked structures); long-running agents risk leaks. | OPEN |
| D03 | **Memory: No linear/affine types** | No single-consumption guarantee; zero-copy handoff of large buffers between agents impossible. | OPEN |
| D04 | **Runtime: Interpreter-only** | Bytecode VM only; no JIT or AOT. High-throughput numeric/tensor workloads are not competitive. | OPEN |
| D05 | **Types: Constraints are runtime** | `where` clauses and record constraints are asserted at runtime; violations cause production failures instead of compile-time errors. | OPEN |
| D06 | **Types: No SMT-backed refinement** | Refinement types (e.g. `Int where x > 0`) cannot be proved at compile time. | OPEN |
| D07 | **Concurrency: Single-threaded scheduler** | No M:N work-stealing; `spawn`/parallel may not utilize multiple cores. | OPEN |
| D08 | **Concurrency: No typed channels** | No first-class `Channel<T>` or session types for agent-to-agent communication. | OPEN |
| D09 | **Concurrency: No supervision** | No hierarchical restart or supervision of failed processes. | OPEN |
| D10 | **Ecosystem: No zero-cost FFI** | `extern` exists but no bindgen-style generation from C/Rust headers. | OPEN |
| D11 | **Ecosystem: No WASM component model** | WASM target exists; WIT / component model not adopted. | OPEN |
| D12 | **AI: No first-class tensors** | No primitive tensor type or differentiable runtime; ML relies on external calls and serialization. | OPEN |
| D13 | **Durability: No checkpoint/resume** | No serialization of stack/heap or replay; long-running workflows cannot survive process death. | OPEN |
| D14 | **Tooling: No DAP** | No Debug Adapter Protocol; no breakpoints, stepping, or value inspection. | OPEN |
| D15 | **Tooling: No sampling profiler** | No flamegraph or allocation profiling. | OPEN |
| D16 | **Metaprogramming: No hygienic macros** | No user-extensible syntax or DSLs; MacroDecl in AST has no documented semantics. | OPEN |
| D17 | **Arithmetic: Overflow semantics** | Checked vs wrapping arithmetic not consistently defined. | OPEN |
| D18 | **Register limit** | 255-register limit can cause compilation failure in large functions. | OPEN |

---

## 2. Operational protocol

- **Principal Architect (Opus)**  
  AST design, formal verification (SMT, refinement, typestate), roadmap and RFC ownership. High-level decomposition only.

- **Systems Engineer (Sonnet)**  
  Compiler backend (LIR, codegen), memory model (allocator, GC, value representation), FFI, unsafe Rust. Implementation in `rust/lumen-compiler`, `rust/lumen-vm`.

- **Runtime Engineer (Sonnet)**  
  VM dispatch, scheduler, concurrency primitives, durable execution (checkpoint/restore), channels. Implementation in `rust/lumen-vm`, `rust/lumen-runtime`.

- **QA Engineer (Sonnet)**  
  Integration tests, fuzzing, benchmarks, CI (clippy, Miri, coverage). Ensures no regressions and no new warnings.

Workflow: select next unchecked task from §3 → implement → run `cargo test --workspace`, `cargo build --release` with zero warnings → update this file (check off task) → commit. On block: escalate to Principal Architect for design change.

---

## 3. Master task list

Each entry: **Task ID**, **Title**, **Problem statement / context**. Rationale and competitive context for phases are in §4. Extended tasks T148–T190 (session types, structured concurrency, parity checklists, diagnostics, testing, IDE, CI, services, cache/validate/parse/workspace) are in §5.

---

### Phase 0: Memory model and value representation

| # | Task | Problem statement / context |
|---|------|-----------------------------|
| T001 | Define new `Value` enum layout | Current `Value` in `rust/lumen-vm/src/values.rs` carries `Rc`-wrapped collections. Define a layout (e.g. 64-bit nan-boxed or 128-bit tagged) that supports immediate scalars and heap object references without mandatory reference counting for all data. |
| T002 | Replace `Rc<T>` with `Arc<T>` in shared VM structures | `Rc` is not `Send`; multi-threaded execution requires thread-safe sharing. Replace in value and runtime structures where shared ownership is required. |
| T003 | Introduce `GcHeader` / tagged pointer for GC | To support a future GC (e.g. Immix-style), define a header (e.g. color, pinned bit) and tagging so the runtime can distinguish immediates from heap pointers and support tracing. |
| T004 | Tagged-pointer / small-value optimization | Store small integers, bools, null in the value word to avoid heap allocation and reduce cache pressure. |
| T005 | Linear/affine wrapper in type system | Add a way to mark values as consumed-once (e.g. `owned T` or linear marker) so the compiler can enforce single use and enable zero-copy handoff. |
| T006 | Ownership rules document | Write a spec (e.g. in `docs/` or SPEC.md) defining move, copy, and borrow semantics for the planned ownership system. |
| T007 | Symbol table scope tracking for ownership | In `rust/lumen-compiler/src/compiler/typecheck.rs` (or resolver), maintain per-variable state: alive, moved, dropped. |
| T008 | Move-check pass | After a value is moved (e.g. passed to a function consuming `owned T`), any subsequent use must be a compile error. Implement check and emit `UseAfterMove` (or equivalent) diagnostic. |
| T009 | Borrow-check pass | Enforce at most one mutable borrow or many immutable borrows; no use-after-free for linear types. Implement in typecheck/resolve. |
| T010 | Negative tests for use-after-move | Add tests in `rust/lumen-compiler/tests/` that expect compilation failure when a moved variable is used. |
| T011 | Arena or region allocator for process-local data | In `rust/lumen-vm/src/` (e.g. new `memory.rs`), implement an arena or region allocator for values that are local to a single process/agent to reduce global heap pressure. |
| T012 | Optional: Immix-style block/line allocator | For a concurrent GC, implement the block/line layout and allocation interface; can be behind a feature flag until scheduler and GC are integrated. |
| T013 | Thread-local allocation buffers (TLAB) | If moving to a GC, provide per-thread allocation buffers to reduce contention on the global allocator. |
| T014 | Copy optimization for small scalars | In the VM or lowering, avoid allocating for small scalars (Int, Bool, Float) when copying; keep them in registers or immediate form. |
| T015 | String representation: SmolStr or interning | In `rust/lumen-vm/src/strings.rs`, consider SmolStr or interned IDs for string comparison and to reduce allocations. |
| T016 | Optional: 64-bit packed LIR instructions | In `rust/lumen-compiler/src/compiler/lir.rs`, evaluate 64-bit instruction encoding to reduce cache pressure; document tradeoffs vs 32-bit. |

---

### Phase 1: Compiler backend (AOT / JIT)

| # | Task | Problem statement / context |
|---|------|-----------------------------|
| T017 | Add LLVM or Cranelift dependency | Introduce `inkwell` (or equivalent) in `rust/lumen-compiler` or a new `lumen-codegen` crate for native code generation. |
| T018 | Codegen module skeleton | Create `rust/lumen-compiler/src/codegen/` (or equivalent) with context, module, and builder initialization. |
| T019 | Target machine configuration | Detect host (x86_64, aarch64) and set optimization level and target attributes. |
| T020 | Lower Lumen Int to LLVM i64 | Map Lumen integer type to a fixed-width LLVM integer type for AOT. |
| T021 | Lower Lumen Float to LLVM double | Map Lumen float to LLVM double (or f64). |
| T022 | Lower Lumen Bool to LLVM i1 | Map Lumen bool to single-bit integer. |
| T023 | Lower Lumen struct/record to LLVM struct | Map record types to packed or non-packed LLVM struct types. |
| T024 | Lower Lumen enum to LLVM (tag + payload) | Represent enums as tag plus union payload in LLVM IR. |
| T025 | Codegen: binary operations | Emit LLVM IR for Add, Sub, Mul, Div (and optionally checked variants). |
| T026 | Codegen: comparison operations | Emit integer and float comparison (Eq, Ne, Lt, Gt, etc.). |
| T027 | Codegen: function prologue | Stack frame setup, parameter passing, callee-saved registers. |
| T028 | Codegen: return values | Emit return instruction and ABI-compliant return value handling. |
| T029 | Codegen: if/else to basic blocks | Lower conditionals to branches and phi nodes where needed. |
| T030 | Codegen: loops (while/loop) | Lower loops to basic blocks with conditional back-edges. |
| T031 | Codegen: match to switch/br | Lower match to switch or branch chain. |
| T032 | Codegen: function calls (ABI) | Correct calling convention and argument marshalling. |
| T033 | Tail-call optimization flag | Mark or optimize tail calls in the LLVM pipeline. |
| T034 | Optional: Template JIT for hot paths | Identify hot loops in LIR and compile them to native code on first execution (baseline JIT). |
| T035 | Optional: JIT engine (OrcJIT) | If using LLVM, implement a small JIT engine to run generated code in-process. |
| T036 | Benchmark: matrix multiply vs C/Rust | Add a benchmark that compares Lumen AOT/JIT to a C or Rust baseline; target within a few percent. |

---

### Phase 2: Formal verification (SMT and refinement)

| # | Task | Problem statement / context |
|---|------|-----------------------------|
| T037 | Add Z3 or CVC5 bindings | Add `z3-sys` or equivalent to the compiler crate for SMT solving. |
| T038 | Verification module skeleton | Create `rust/lumen-compiler/src/verification/` (or `verifier/`) with solver wrapper. |
| T039 | Map Lumen types to SMT sorts | Implement mapping from Lumen Int, Bool, etc., to solver sorts (e.g. Z3_mk_int). |
| T040 | Parse `where` clauses into AST | Ensure `where` expressions in records and function contracts are available in the AST. |
| T041 | Lower `where` to SMT assertions | Translate boolean expressions in `where` to SMT-LIB or solver API calls. |
| T042 | Verify function preconditions | For each call site, assert caller’s context implies callee’s precondition; check satisfiability. |
| T043 | Verify function postconditions | After call, assume callee’s postcondition for subsequent reasoning. |
| T044 | Verification pass in pipeline | Run verification after typecheck; on UNSAT (invariant violated), emit compiler error. |
| T045 | UNKNOWN handling | When solver returns UNKNOWN, emit warning or require explicit assertion. |
| T046 | Path-sensitive refinement | In typecheck, update refinement info after conditionals (e.g. `if x > 0` then `x` is positive in then-branch). |
| T047 | Array/list bounds in refinement | Where possible, prove index in range so runtime bounds checks can be elided. |
| T048 | Effect budget checking | If effect row includes bounds (e.g. `network(max_calls: 5)`), prove no path exceeds the bound. |
| T049 | Exhaustiveness for refinement ranges | For match on refined integers (e.g. 0..100), ensure all cases are covered or warn. |
| T050 | Typestate (e.g. File Open/Closed) | Design and implement typestate so that operations (e.g. read) are only valid in certain states; compiler error otherwise. |
| T051 | Test: refinement verification | Add tests that expect success or failure of verification (e.g. division by positive divisor). |
| T052 | Fuzz type checker with constraints | Property-based or random programs with `where` clauses; ensure solver results are consistent. |

---

### Phase 3: Concurrency and scheduler

| # | Task | Problem statement / context |
|---|------|-----------------------------|
| T053 | Process/task control block (PCB) | Define a structure that holds enough state to suspend and resume a task (e.g. stack, IP, locals). |
| T054 | Scheduler module in runtime | Create `rust/lumen-runtime/src/scheduler.rs` (or equivalent) with run queues. |
| T055 | Per-thread run queues | Each worker thread has a local queue of runnable tasks. |
| T056 | Work-stealing algorithm | When local queue is empty, steal from another thread’s queue (e.g. Chase–Lev deque). |
| T057 | Global injection queue | New tasks (e.g. from `spawn`) go into a global or per-worker queue. |
| T058 | Replace Tokio spawn with scheduler | Make `spawn` push a task into the new scheduler instead of delegating to Tokio (or document hybrid). |
| T059 | Explicit yield points in VM | In the VM dispatch loop, periodically check for yield/reduction count to allow preemption. |
| T060 | Reduction counting | After N instructions (e.g. 2000), force a context switch to avoid starvation. |
| T061 | Mailbox or MPSC queue for agents | Lock-free or low-contention queue for messages to a single agent. |
| T062 | Channel&lt;T&gt; type in runtime | Typed channel (MPMC or SPSC) for inter-agent communication. |
| T063 | Selective receive (Erlang-style) | Allow receiving only messages matching a pattern; document semantics. |
| T064 | Supervisor behaviour (design) | Define in spec or std how a supervisor restarts or escalates on child failure. |
| T065 | link / monitor primitives | When process A links to B, A is notified if B crashes; implement or stub in runtime. |
| T066 | C10K-style test | Spawn many agents and measure latency and memory; establish baseline. |

---

### Phase 4: Durable execution

| # | Task | Problem statement / context |
|---|------|-----------------------------|
| T067 | Snapshot format specification | Define serialization format (e.g. Cap’n Proto, bincode) for stack frames, heap reachability, and IP. |
| T068 | Serialize VM stack frames | Implement traversal of call stack and serialization of locals and return addresses. |
| T069 | Serialize heap reachable from stack | Only serialize reachable heap objects to bound snapshot size. |
| T070 | checkpoint intrinsic | VM intrinsic that triggers snapshot and writes to configured storage (e.g. file or log). |
| T071 | Durable log interface | In `rust/lumen-runtime`, define an abstraction for append-only log used for checkpointing. |
| T072 | restore / rehydrate | Load snapshot from storage and restore VM state; resume execution from saved IP. |
| T073 | Deterministic replay: record nondeterminism | Under `@deterministic`, record all nondeterministic inputs (time, random, network) to a trace. |
| T074 | Time-travel debugger CLI | Tool to replay a trace (e.g. `lumen replay &lt;trace_file&gt;`) and step forward/backward. |
| T075 | Workflow versioning / migration | Design how to resume an old snapshot when the code has changed (schema evolution or migration). |
| T076 | Integration test: kill and resume | Run a program, kill process, restart from last checkpoint, assert it resumes correctly. |

---

### Phase 5: AI-native (tensors and differentiation)

| # | Task | Problem statement / context |
|---|------|-----------------------------|
| T077 | Tensor type in VM or std | Define a primitive or standard type for N-dimensional arrays (shape, dtype, storage). |
| T078 | Tensor storage and strides | Contiguous or strided storage; layout suitable for SIMD/BLAS. |
| T079 | Bind BLAS/LAPACK or tensor backend | Use ndarray, tch-rs, candle, or similar for actual ops; expose to Lumen. |
| T080 | SIMD-friendly allocation | Align tensor buffers for AVX-512 or NEON when available. |
| T081 | Dual number type for AD | Type that carries value and derivative (adjoint) for forward-mode AD. |
| T082 | Operator overloading for Dual | Arithmetic and math ops on Dual apply chain rule. |
| T083 | Tape (Wengert list) for reverse-mode | Record operations on tensors during forward pass for backward pass. |
| T084 | backward() intrinsic | Trigger gradient computation from tape. |
| T085 | Dimension checking for tensor ops | At typecheck or runtime, ensure shapes are compatible (e.g. matrix multiply). |
| T086 | Optimizer in std (SGD, Adam) | Standard library or example implementing basic optimizers using the AD primitives. |
| T087 | Prompt-as-code: type to grammar | Compile a Lumen type (e.g. record) to a grammar (e.g. GBNF) for constrained LLM output. |
| T088 | Static prompt checking | Ensure variables referenced in prompt templates are in scope and typed (e.g. string). |
| T089 | Test: gradient of f(x)=x^2 | Verify that AD yields gradient 2x at a point. |

---

### Phase 6: Ecosystem and tooling

| # | Task | Problem statement / context |
|---|------|-----------------------------|
| T090 | Real Ed25519/Sigstore signing | Replace package manager signing stubs with actual cryptographic signing (see COMPETITIVE_ANALYSIS §7.3). |
| T091 | Transparency log for packages | Append-only log for published artifacts; verification against log. |
| T092 | Registry deployment | Deploy registry (e.g. Cloudflare Workers + D1/R2) so publish/search/install round-trip works. |
| T093 | OIDC for registry auth | Use OpenID Connect for publisher identity where applicable. |
| T094 | TUF or similar for repository metadata | The Update Framework (or equivalent) for secure package metadata. |
| T095 | extern "C" and ABI | Support C calling convention and correct ABI in FFI. |
| T096 | Header-to-Lumen bindgen | Tool that parses C (or Rust) headers and generates Lumen extern declarations. |
| T097 | WASM target in codegen | Emit wasm32-wasi (or wasm32-unknown-unknown) from the same codegen path. |
| T098 | WASM component model (WIT) | Adopt WIT IDL for imports/exports and component composition. |
| T099 | WASI host bindings | Implement or bind filesystem, clock, random for WASI. |
| T100 | LSP: rename symbol | Rename cell/type/variable across the workspace. |
| T101 | LSP: code actions | Quick fixes (e.g. add match arm, add import). |
| T102 | LSP: inlay hints (types, params) | Show inferred types and parameter names inline. |
| T103 | DAP: breakpoints and stepping | Debug Adapter Protocol server; breakpoints, step in/out/over. |
| T104 | DAP: value inspection | Inspect variables and stack frames in debug session. |
| T105 | Multi-error reporting in compiler | Emit all recoverable errors in one run, not only the first. |
| T106 | Fix-it hints in diagnostics | Attach suggested edits to diagnostics where possible. |
| T107 | Error codes and documentation | Assign codes to errors and link to documentation. |
| T108 | Clippy: deny warnings in CI | Ensure `cargo clippy -- -D warnings` passes. |
| T109 | Miri in CI | Run tests under Miri where applicable to catch UB. |
| T110 | Benchmark suite and regression gate | Formal benchmark suite; CI fails on performance regression beyond a threshold. |

---

### Phase 7: Syntax and language features

| # | Task | Problem statement / context |
|---|------|-----------------------------|
| T111 | Pipeline operator semantics | Ensure `|>` has well-defined evaluation order and types (already in grammar; verify and document). |
| T112 | Null-conditional and null-coalescing | `?.` and `??` (or equivalent) for optional chaining and default values; ensure consistent with `T?`. |
| T113 | Spaceship operator | Three-way comparison `<=>` returning Less/Equal/Greater. |
| T114 | Inclusive/exclusive range | `..=` and `..` already present; ensure full coverage in parser and lowering. |
| T115 | Membership operator | `in` for collection membership; typecheck and lower. |
| T116 | Active patterns (F#-style) | Match on result of a function (e.g. `ValidEmail(user, domain)`) with compiler support. |
| T117 | GADTs | Generalized algebraic data types with type parameters in variant return types. |
| T118 | Hygienic macro system | Macro expansion without variable capture; define syntax and scope rules. |
| T119 | String interpolation with format spec | e.g. `f"Value: {x:.2f}"` with typed formatting. |
| T120 | Trailing lambda / DSL blocks | Allow block or lambda at end of call for DSLs (e.g. `html div { ... }`). |
| T121 | Error propagation operator | Postfix `?` that unwraps `result[T,E]` or returns early with error. |
| T122 | Try/else expression | `try expr else handler` for local error handling. |
| T123 | Checked arithmetic by default | `+`, `-`, `*` check overflow; provide wrapping variants (e.g. `+%`). |
| T124 | Register limit fix | Increase register set or use virtual registers and allocation so large functions compile. |

---

### Phase 8: Standard library and runtime

| # | Task | Problem statement / context |
|---|------|-----------------------------|
| T125 | std::simd (or intrinsics) | Expose hardware SIMD where available for hot loops. |
| T126 | std::crypto | Native or binding for Ed25519, BLAKE3, etc.; avoid unnecessary bindings where feasible. |
| T127 | std::graph | First-class graph structure (e.g. for knowledge graphs). |
| T128 | std::tensor | N-dimensional array API built on Phase 5 primitives. |
| T129 | std::fs async | Async file I/O if async runtime is standardized. |
| T130 | std::net | Async TCP/UDP or equivalent. |
| T131 | std::http client/server | HTTP using hyper or equivalent; zero-copy where possible. |
| T132 | std::json fast path | High-performance JSON using serde or similar. |
| T133 | Collections with linear types | Where applicable, offer APIs that consume `self` (e.g. linear vector) for zero-copy pipelines. |

---

### Phase 9: Self-hosting and bootstrap

| # | Task | Problem statement / context |
|---|------|-----------------------------|
| T134 | Port lexer to Lumen | Implement lexer in Lumen (e.g. `stdlib/compiler/lexer.lm` or separate repo). |
| T135 | Port parser to Lumen | Parser producing same AST (or compatible) as Rust compiler. |
| T136 | Port typecheck to Lumen | Type checking and constraint checking in Lumen. |
| T137 | Port LIR lowering to Lumen | Emit LIR (or equivalent) from Lumen compiler. |
| T138 | Bootstrap: compile Lumen compiler with Rust | Use existing Rust compiler to compile the Lumen compiler (Lumen source). |
| T139 | Binary reproducibility | Build the Lumen compiler twice; compare binaries or build hashes. |
| T140 | Optional: retire Rust compiler | Long-term: full self-hosting; only after bootstrap is stable and tested. |

---

### Phase 10: Verification and release

| # | Task | Problem statement / context |
|---|------|-----------------------------|
| T141 | All 1365+ tests pass | After any change, full workspace test suite passes. |
| T142 | Zero clippy warnings | `cargo clippy -- -D warnings` for all crates. |
| T143 | Coverage gate | Maintain or improve coverage (e.g. >95% for critical paths). |
| T144 | Valgrind or sanitizers | No leaks; address sanitizer clean where applicable. |
| T145 | Security audit (cargo audit) | Zero known vulnerabilities in dependencies. |
| T146 | Documentation sync | SPEC, CLAUDE.md, and ROADMAP aligned with implementation. |
| T147 | Release v1.0.0 tag | Tag and release when Phase 0–9 goals are met and gates pass. |

---

## 4. References and competitive context

- **Gap analysis:** See **docs/research/COMPETITIVE_ANALYSIS.md** (single competitive doc: domain matrix, dimension-by-dimension gaps, deficit lists, leapfrog opportunities, execution backlog, 50-language matrix, refs). Each deficiency D01–D18 is mapped there to “who beats us” and to the task IDs below that close it.
- **Rationale for phases:** Phase 0 (memory) addresses Rust/Swift/Zig/Go lead on safety and parallelism (D01–D03). Phase 1 (AOT/JIT) addresses interpreter-only gap vs Rust, Go, Julia, LuaJIT (D04). Phase 2 (verification) addresses Liquid Haskell / F* / TypeScript-strict lead on compile-time invariants (D05–D06). Phase 3 (concurrency) addresses Go/Erlang lead on scheduler and supervision (D07–D09). Phase 4 (durability) addresses Temporal/Erlang-style resilience (D13). Phase 5 (AI) extends Lumen’s lead; Phase 6 (ecosystem) addresses Cargo/npm/pip/WASM (D10–D11). Phase 7–8 (syntax, stdlib) close ergonomics and batteries-included gaps. Phase 9 (self-hosting) is a long-term proof of language maturity.
- **Primary external refs:** Rust Book (ownership), Go spec (concurrency), Erlang OTP (supervision), Z3 (SMT), WASM WIT (components), MCP (tools). Full list in COMPETITIVE_ANALYSIS.md §6.

---

## 5. Extended task list (comprehensive coverage)

The following tasks add depth and explicit competitive parity. Problem statements include *why* (context) and, where useful, *reference* to the competitive analysis.

---

### Phase 2 (verification) — additional

| # | Task | Problem statement / context |
|---|------|-----------------------------|
| T148 | Session types (design) | Multiparty session types for agent protocols (e.g. “Client sends Hello; Server sends Ack”). Compiler enforces ordering. Ref: research on session types; competitive gap: no mainstream language has this for agents. |
| T149 | Counter-example generation | When SMT reports UNSAT, emit concrete input values that violate the invariant so the user can fix the code. Improves DX vs “invariant violated” alone. |
| T150 | Proof hints / manual assertions | Allow user to supply proof hints or assert at a point to help the solver (e.g. for non-linear constraints). Ref: F*, Dafny. |

### Phase 3 (concurrency) — additional

| # | Task | Problem statement / context |
|---|------|-----------------------------|
| T151 | Structured concurrency (nursery/scope) | When a parent task is cancelled or fails, all child tasks are cancelled. Prevents orphaned agents. Ref: Swift structured concurrency; Kotlin coroutine scope. COMPETITIVE_ANALYSIS §3.4. |
| T152 | Channel select / multiplexing | Block until one of several channels is ready (select or similar). Required for robust agent coordination. Ref: Go select, Erlang receive. |
| T153 | Actor trait or process interface | Standard interface: mailbox + state + message handler. Enables uniform supervision and testing. Ref: Erlang/OTP gen_server. |

### Phase 4 (durability) — additional

| # | Task | Problem statement / context |
|---|------|-----------------------------|
| T154 | Idempotency keys for side effects | When replaying, reuse cached results for side effects (e.g. HTTP) keyed by idempotency key so replay is deterministic and does not re-execute external calls. Ref: Temporal, durable workflows. |
| T155 | Snapshot compression and pruning | Limit snapshot size by compressing or pruning old stack frames/heap so long-running agents do not exhaust storage. |

### Phase 5 (AI-native) — additional

| # | Task | Problem statement / context |
|---|------|-----------------------------|
| T156 | Probabilistic type (Prob&lt;T&gt;) (design) | Value representing a distribution; “if x” on Prob&lt;Bool&gt; could weight both branches. Enables Bayesian agents. Ref: roadmap “probabilistic type system.” |
| T157 | Schema drift detector | Fail build or test when tool/API response schema diverges from declared Lumen types. Closes “silent breakage” gap vs ad-hoc SDKs. |
| T158 | Effect-budget enforcement at runtime | If effect row says `network(max_calls: 5)`, runtime (or compiled check) enforces the bound and fails fast. Complements T048 (compile-time proof). |

### Phase 6 (ecosystem) — additional

| # | Task | Problem statement / context |
|---|------|-----------------------------|
| T159 | Recursive sandbox at import | At `import "pkg"`, optionally restrict capabilities (e.g. `granting { none }` so the package cannot access network or disk). Ref: roadmap “Fortress” registry; capability-based security. |
| T160 | Binary caching for packages | Registry serves precompiled artifacts per platform so `ware install` avoids compilation. Ref: npm, cargo build cache; COMPETITIVE_ANALYSIS §3.6. |
| T161 | LSP: semantic search | “Find all call sites where temperature &gt; 0.7” over AST, not text. Improves refactoring and audit. Ref: roadmap “Agentic LSP.” |

### Phase 7 (syntax) — additional

| # | Task | Problem statement / context |
|---|------|-----------------------------|
| T162 | Multi-shot continuations (design) | Allow `resume` to be called multiple times for backtracking/search. Currently one-shot; design semantics and VM support. Ref: roadmap “effect handlers deep.” |
| T163 | Variadic parameters (complete) | Complete variadic `...param` in typecheck and lowering so stdlib and user code can define variadic cells. Ref: SPEC.md; COMPETITIVE_ANALYSIS §3. |
| T164 | Must-use result attribute | `@must_use` for cells returning `result[T,E]` so ignoring the result is a warning or error. Ref: Rust must_use; COMPETITIVE_ANALYSIS “leapfrog.” |

### Phase 10 (verification) — competitive parity

| # | Task | Problem statement / context |
|---|------|-----------------------------|
| T165 | Parity checklist: memory | Document and test that Lumen meets parity with baseline (e.g. Arc in use, no Rc in shared paths; or GC/region in place). Ref: COMPETITIVE_ANALYSIS §3.1, D01–D03. |
| T166 | Parity checklist: concurrency | Document and test C10K-style scenario; supervision and channels exercised. Ref: §3.4, D07–D09. |
| T167 | Parity checklist: verification | Document and test that at least one refinement (e.g. positive index) is proved by SMT. Ref: §3.3, D05–D06. |
| T168 | Parity checklist: durability | Document and test kill-and-resume and replay. Ref: §3.5, D13. |

### Extended: diagnostics, testing, IDE, CI, services (T169–T180)

| # | Task | Problem statement / context |
|---|------|-----------------------------|
| T169 | Error context chaining | Propagate and display cause chains (e.g. "tool X failed because network unreachable because TLS handshake failed"). Improves debuggability vs single-message errors. |
| T170 | Panic vs result (halt vs return err) | Define clear boundary: which failures panic (e.g. unreachable, invariant violation) vs return `result[T,E]`; document and enforce so agents can recover predictably. Ref: Rust panic vs Result. |
| T171 | Inline / property-based / snapshot testing | Built-in or std test helpers: inline unit tests, property-based (e.g. QuickCheck-style), snapshot output comparison. Ref: COMPETITIVE_ANALYSIS §8 (leapfrog 11). |
| T172 | Mock effects for tests | Test harness to stub effect operations (e.g. `perform HttpGet`) so tests run without real I/O. Complements deterministic replay. |
| T173 | LSP: go-to-implementations | "Go to implementation(s)" for cells and types; navigate to defining and overriding sites. Ref: IDE parity. |
| T174 | Diagnostics: type diff and import suggestions | On type error, show concise type diff (expected vs actual); on unknown symbol, suggest imports or similar names. |
| T175 | Watch mode (recheck on save) | `lumen watch` or LSP-driven re-check when files change; fast feedback without full rebuild. |
| T176 | CI machine-readable output | Emit check/test results in a standard format (e.g. SARIF, JUnit XML) for CI dashboards and gates. Ref: T105–T110. |
| T177 | Service package template | Scaffold for HTTP/gRPC services: typed route contracts, generated schemas, replayable fixtures. Ref: COMPETITIVE_ANALYSIS §4 (Web/backend). |
| T178 | Array bounds propagation (refinement) | Use refinement/SMT or flow analysis to prove or warn on list/tuple index bounds. Reduces runtime index errors. Ref: D05–D06. |
| T179 | Docs-as-tests (snippets in CI) | All fenced Lumen code blocks in SPEC/docs compiled (or run) in CI; doc drift fails the build. Ref: COMPETITIVE_ANALYSIS §8 (leapfrog 17). |
| T180 | Execution graph visualizer | Tool or LSP view that renders execution/trace events as a graph (calls, effects, tool invocations) for debugging and audit. Ref: COMPETITIVE_ANALYSIS §8 (leapfrog 20). |
| T181 | Import path error recovery | Use `parse_program_with_recovery` when compiling imported modules so multiple parse errors in a dependency are reported. Ref: COMPETITIVE_ANALYSIS §7.4 (A). |
| T182 | LSP document formatting | Expose existing formatter via LSP `textDocument/formatting` (document_formatting_provider). Ref: lumen-lsp main.rs; §7.4 (B). |
| T183 | Semver constraint `!=` operator | Implement `!=` in semver constraint parser (e.g. `!=1.2.3`) for version ranges. Ref: semver.rs test note; §7.4 (C). |
| T184 | Retry-After header in provider errors | Extract `Retry-After` HTTP header into `ToolError::RateLimit { retry_after_ms }` in Gemini (and other HTTP) providers. Ref: lumen-provider-gemini. |
| T185 | Cache persistence on startup | Runtime `CacheStore` (`lumen-runtime/src/cache.rs`) only writes on put; add load-from-disk on init so cache survives process restart. Ref: deficit 13. |
| T186 | Validate builtin (runtime schema validation) | SPEC documents `validate(Any) -> Bool` as stub. Implement real schema-constrained validation for the standalone builtin (Schema opcode already validates at tool/output). |
| T187 | Fix role_interpolation.lm.md parse and un-ignore | Resolve known parse issue in `examples/role_interpolation.lm.md`; remove from SKIP_COMPILE and re-enable `examples_compile` test. |
| T188 | Source mapping for string interpolation spans | Parser TODO v2: map spans correctly inside string interpolation (track offsets per segment so diagnostics point into the interpolated expression). Ref: parser.rs. |
| T189 | Verify/fix closure and upvalue model | Audit and fix any remaining closure capture or upvalue bugs in lower and VM; tests may pass but edge cases or replay/serialization may expose issues. Ref: deficit 6. |
| T190 | Workspace (monorepo) resolver | Multi-package workspace support: resolve and build multiple packages in one repo with shared deps (Cargo/npm-style). Ref: COMPETITIVE_ANALYSIS domain matrix "workspace resolver". |

---

## 6. Maintenance

- This file is the **source of truth** for the implementation backlog. Update checkboxes or status when tasks are completed.
- Link commits or PRs to task IDs where helpful.
- When a deficiency in §1 is fully addressed, mark it RESOLVED and note the task IDs that closed it.
- When adding tasks, include problem statement/context and, if relevant, a reference to COMPETITIVE_ANALYSIS.md or ROADMAP.md.
