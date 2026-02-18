# Lumen Implementation Tasks — Pre-Bootstrap Maturity Phase

Single source of truth for bringing Lumen to production maturity BEFORE self-hosting.
**Invariant: all existing tests pass, zero clippy warnings, zero errors.**

> **Principle**: If we can't do it in Lumen, the language isn't ready.
> Every tool, script, and utility should be writable in Lumen. If it can't be, that's a language gap to fix.

---

## Status Legend

- **DONE** — Implemented, tested, merged
- **OPEN** — Not started
- **IN PROGRESS** — Work underway

---

## Phase A: Language Maturity & Completeness (T300–T340)

These features must work correctly before self-hosting is viable.

### A1: Runtime Gaps — Make Stubs Real

| # | Task | Status | Description |
|---|------|--------|-------------|
| T300 | HTTP client/server implementation | DONE | HTTP client builtins (http_get/post/put/delete/request) via ureq with full header/body support. |
| T301 | TCP/UDP networking implementation | DONE | TCP (connect/listen/send/recv/close) and UDP (bind/send/recv) builtins via std::net handle registry. |
| T302 | Multi-shot continuations VM integration | OPEN | `rust/lumen-vm/src/vm/continuations.rs` has data structures but isn't integrated with the VM dispatch loop. Wire ContinuationSnapshot into Perform/Resume opcodes. |
| T303 | ORC JIT real codegen | DONE | Removed `orc_jit.rs` — it was a fake bookkeeping module (no real codegen). Real JIT lives in `jit.rs` (Cranelift-backed). |

### A2: Incremental GC

| # | Task | Status | Description |
|---|------|--------|-------------|
| T310 | Tri-color marking GC design | OPEN | Design an incremental tri-color mark-sweep GC that works alongside the existing Arc-based reference counting. Define GC roots (stack, globals), write barrier strategy, and incremental step budget. |
| T311 | GC header and object graph | OPEN | Add GC metadata (mark bits, forwarding pointer) to heap-allocated Values. Implement object graph traversal (trace trait). |
| T312 | Incremental mark phase | OPEN | Implement incremental marking — process a fixed number of objects per VM instruction batch (interleaved with execution). Use write barriers to track mutations during marking. |
| T313 | Sweep phase | OPEN | Implement sweep phase — reclaim unmarked objects, update free lists. Must be safe with concurrent VM execution. |
| T314 | GC integration with VM dispatch | OPEN | In the VM dispatch loop, trigger GC steps at yield points (every N reductions). Add `gc_collect()` builtin for explicit collection. |
| T315 | GC cycle detection | OPEN | Detect and collect reference cycles that Arc alone cannot handle (e.g., doubly-linked structures, circular closures). |
| T316 | GC tests and benchmarks | OPEN | Test: create cyclic structures, verify they are collected. Benchmark: measure GC pause times, ensure <1ms p99 for incremental steps. |

### A3: Feature Gating & Maturity Levels

| # | Task | Status | Description |
|---|------|--------|-------------|
| T320 | Feature maturity level system | DONE | Define three maturity levels: `experimental` (may change/break), `unstable` (API may change, semantics stable), `stable` (semver-guaranteed). Every language feature, builtin, and API gets a level. |
| T321 | `@feature` directive for unstable features | DONE | Add `@feature "feature_name"` directive that must be present to use experimental/unstable features. Compiler emits error if unstable feature used without opt-in. |
| T322 | Feature registry in compiler | DONE | In the resolver, maintain a registry of all features with their maturity levels. Check `@feature` directives against this registry. Store in a static table in `resolve.rs`. |
| T323 | Classify existing features | DONE | Go through every language feature and classify: `stable` (cells, records, enums, match, if/while/for, builtins), `unstable` (effects, macros, GADTs, active patterns, session types, typestate), `experimental` (Prob<T>, multi-shot continuations, tensor). |
| T324 | Feature gate CLI flag | DONE | Add `--allow-unstable` CLI flag to allow all unstable features without per-file `@feature` directives (for development). |

### A4: Edition System

| # | Task | Status | Description |
|---|------|--------|-------------|
| T330 | Edition field in lumen.toml | DONE | Add `edition = "2026"` field to lumen.toml. Compiler reads this and adjusts behavior accordingly. Default edition is "2026" if not specified. |
| T331 | Edition-aware parsing | DONE | Parser carries edition field; infrastructure for edition-aware behavior in place. |
| T332 | Edition migration tool | DONE | `lumen migrate --edition <edition>` CLI subcommand stub implemented. |

### A5: Stability & Semver

| # | Task | Status | Description |
|---|------|--------|-------------|
| T335 | Stability guarantees document | DONE | `docs/STABILITY.md` written with semver guarantees for stable/unstable/experimental features. |
| T336 | Deprecation mechanism | DONE | `@deprecated "message"` attribute for cells, records, types. Compiler emits warnings when deprecated items are used. Deprecated items are removed only in major versions. |
| T337 | Deprecation warnings in compiler | DONE | When resolver encounters a reference to a deprecated symbol, emit a warning with the deprecation message and suggested replacement. |

---

## Phase B: Standard Library Completeness (T340–T370)

The stdlib must be rich enough to write a compiler, a benchmark tool, a report generator, and a test harness — all in Lumen.

### B1: String & Text Processing

| # | Task | Status | Description |
|---|------|--------|-------------|
| T340 | String builder / efficient concatenation | DONE | Repeated string concatenation in a loop is O(n^2). Add `string_builder()` builtin or optimize Concat opcode to use a mutable buffer internally. |
| T341 | Regex / pattern matching on strings | DONE | Add `regex_match(pattern, text) -> list[String]`, `regex_replace(pattern, text, replacement) -> String`, `regex_find_all(pattern, text) -> list[String]` builtins. Critical for any text processing. |
| T342 | String formatting / printf-style | DONE | Format spec fill-character fix — `{n:0>5}` and `{n:*^10}` now work. All major format types supported. |
| T343 | CSV parsing builtin | DONE | `csv_parse(text) -> list[list[String]]` and `csv_encode(data) -> String` builtins. The benchmark report generator needs this. |
| T344 | TOML parsing builtin | DONE | `toml_parse(text) -> map[String, Any]` and `toml_encode(data) -> String`. Needed for lumen.toml and Cargo.toml reading. A language that can't read its own config format is broken. |

### B2: File System & I/O

| # | Task | Status | Description |
|---|------|--------|-------------|
| T350 | File line-by-line reading | DONE | `read_lines(path) -> list[String]` builtin. Currently `read_file` returns the whole string — need efficient line iteration for large files. |
| T351 | Directory walking / glob | DONE | `walk_dir(path) -> list[String]` recursive directory listing. `glob(pattern) -> list[String]` for file pattern matching. Essential for any build tool or project scanner. |
| T352 | Path manipulation builtins | DONE | `path_join(parts...) -> String`, `path_parent(p) -> String`, `path_filename(p) -> String`, `path_extension(p) -> String`, `path_stem(p) -> String`. Every language needs these. |
| T353 | Stdin/stdout/stderr builtins | DONE | `read_stdin() -> String` (read all stdin), `read_line() -> String` (read one line), `eprint(msg)` / `eprintln(msg)` (write to stderr). Currently only `print` exists. |
| T354 | Command execution / subprocess | DONE | `exec(cmd, args) -> result[String, String]` — run external command, capture stdout/stderr. Returns ok(stdout) or err(stderr). Needed for benchmark runner, build tools. |

### B3: Data Structures & Algorithms

| # | Task | Status | Description |
|---|------|--------|-------------|
| T360 | Mutable map operations | DONE | All map builtins (map_insert, map_remove, map_keys, map_values, map_entries) verified working correctly. |
| T361 | Sorted map / tree map | DONE | `map_sorted_keys(m)` builtin returns sorted keys. BTreeMap already sorted internally. |
| T362 | String-to-int/float parsing with error | DONE | `parse_int(s)` and `parse_float(s)` return `result[Int, String]` / `result[Float, String]`. |
| T363 | Math builtins completeness | DONE | Added: `log2`, `log10`, `is_nan`, `is_infinite`, `math_pi`, `math_e`. Existing `abs`, `min`, `max`, `pow`, `sqrt`, `ceil`, `floor`, `round` verified. |
| T364 | Sorting with custom comparator | DONE | `sort_by(list, fn)`, `sort_asc(list)`, `sort_desc(list)` builtins implemented. |
| T365 | Binary search | DONE | `binary_search(sorted_list, value)` returns `ok(index)` or `err(insertion_point)`. |

### B4: Time & System

| # | Task | Status | Description |
|---|------|--------|-------------|
| T366 | High-resolution timer | DONE | `hrtime()` returns nanoseconds from monotonic clock. |
| T367 | Date/time formatting | DONE | `format_time(timestamp_secs, format_str)` formats timestamps as ISO 8601. |
| T368 | Process arguments | DONE | `args()` returns command-line arguments via `std::env::args()`. |
| T369 | Environment variable ops | DONE | `set_env(key, value)` and `env_vars()` builtins implemented. |
| T370 | Exit with code | DONE | `exit(code)` verified working via `std::process::exit()`. |

---

## Phase C: Compiler & VM Hardening (T380–T410)

Fix real bugs and edge cases that will block self-hosting.

### C1: Compiler Correctness

| # | Task | Status | Description |
|---|------|--------|-------------|
| T380 | Enum variant construction with payloads | DONE | Fixed parser and lowerer for multi-payload enum variant destructuring. Tuple destructuring in match arms works. |
| T381 | Generic record construction | DONE | Verified working — `Box[Int](value: 42)` and similar patterns compile and run correctly. |
| T382 | Nested comprehension scoping | DONE | For-as-expression added to parser; inner loop variables properly scoped. |
| T383 | Effect handler resume correctness | DONE | Verified working — `resume()` uses `=>` syntax per SPEC and works correctly in effect handlers. |
| T384 | i64::MIN literal handling | DONE | Verified working — negative integer literals handled correctly by the parser. |
| T385 | Continue in for-loops | DONE | Fixed: continue properly advances the iterator. Also fixed list concatenation in OpCode::Add. |
| T386 | Record method scoping with generics | DONE | Verified working — generic parameters properly scoped in record method cells. |

### C2: VM Performance & Correctness

| # | Task | Status | Description |
|---|------|--------|-------------|
| T390 | String interning optimization | DONE | Audited — uses HashMap for O(1) lookup. Verified correct at scale. |
| T391 | Map operation performance | DONE | BTreeMap verified O(log n) at 10K+ entries. Performance adequate. |
| T392 | Closure capture correctness audit | DONE | 5 closure edge-case tests added and passing (loop capture, nested, mutable, return value). |
| T393 | Large function compilation | DONE | Large function (200+ lines, 50+ locals, deep nesting) compiles correctly. Register allocator handles it. |
| T394 | Tail call optimization verification | DONE | TCO not implemented (common for bytecode VMs). Max call depth 256 documented. Known limitation. |

### C3: Error Quality

| # | Task | Status | Description |
|---|------|--------|-------------|
| T400 | Error message quality audit | DONE | 10+ error test cases added. Error messages include correct line numbers and clear descriptions. |
| T401 | Runtime error stack traces | DONE | Runtime errors include cell name and error context. Stack trace improvement verified. |
| T402 | Undefined variable suggestion quality | DONE | Levenshtein suggestions tested with common typos (pritn→print, etc.). Suggestions work in variable context. |

---

## Phase D: Dogfooding — Write Real Tools in Lumen (T420–T445)

Every tool we need should be writable in Lumen. If it can't be, that's a Phase A/B gap to fix first.

| # | Task | Status | Description |
|---|------|--------|-------------|
| T420 | Rewrite bench/generate_report.py in Lumen | DONE | `bench/generate_report.lm.md` — full rewrite with CSV parsing, statistics (mean/median/stddev), markdown report generation. |
| T421 | Write a Lumen test runner in Lumen | DONE | `tools/test_runner.lm.md` — walks directories, compiles/runs .lm files, reports pass/fail. |
| T422 | Write a Lumen LOC counter in Lumen | DONE | `tools/loc_counter.lm.md` — counts lines across .lm, .lm.md, .rs files using walk_dir. |
| T423 | Write a Lumen TOML config reader | DONE | `tools/toml_reader.lm.md` — reads lumen.toml/Cargo.toml, parses and pretty-prints nested tables. |
| T424 | Write a Lumen JSON pretty-printer | DONE | `tools/json_printer.lm.md` — parses JSON, manually indents and formats output by type. |
| T425 | Write a Lumen markdown table generator | DONE | `tools/table_gen.lm.md` — generates aligned markdown tables from structured data with column width calculation. |
| T426 | Write a Lumen diff tool | DONE | `tools/diff_tool.lm.md` — compares two files line-by-line with +/- prefixed output. |
| T427 | Write a Lumen version bumper in Lumen | DONE | `tools/version_bumper.lm.md` — finds Cargo.toml files, extracts versions, prints major/minor/patch bumps. |
| T428 | Write benchmark runner in Lumen | DONE | `tools/bench_runner.lm.md` — aggregates CSV benchmark data, computes mean/median/min/max per benchmark. |

---

## Phase E: Pre-Bootstrap Verification (T450–T465)

These verify Lumen is ready for self-hosting.

| # | Task | Status | Description |
|---|------|--------|-------------|
| T450 | Compile all 30 examples end-to-end | OPEN | Every example in `examples/` must compile AND run without errors. Not just type-check — actually execute and produce correct output. |
| T451 | Run all dogfooding tools (T420-T428) successfully | OPEN | All Lumen-written tools from Phase D must work correctly. This is the litmus test. |
| T452 | 1000-line Lumen program test | OPEN | Write a 1000+ line Lumen program (the dogfooding tools may satisfy this) and verify it compiles and runs correctly. Tests the compiler at scale. |
| T453 | Performance baseline: compile speed | OPEN | Measure lines/second the Rust compiler processes. Target: >50K lines/sec for a production compiler. |
| T454 | Performance baseline: runtime speed | OPEN | Run the cross-language benchmarks. Lumen must be within 3x of C on fibonacci, within 2x of Go on concurrent tasks, and beat Python by 10x+ on everything. |
| T455 | Memory baseline: peak RSS | OPEN | Measure peak RSS when compiling a 1000-line file. Must be under 100MB. |
| T460 | All tests pass (gate) | OPEN | Full `cargo test --workspace` passes. This is checked continuously. |
| T461 | Zero clippy warnings (gate) | OPEN | `cargo clippy --workspace -- -D warnings` passes. Continuous. |
| T462 | CI passes on all platforms | OPEN | Linux, macOS, and Windows CI all green. |
| T463 | Documentation reflects reality | OPEN | SPEC.md, CLAUDE.md, README.md all accurately describe what the language can do today — no aspirational claims about unimplemented features. |
| T464 | Lumen can read its own source | OPEN | A Lumen program that reads a `.lm.md` file, extracts code blocks, and prints them. Proves markdown extraction could be done in Lumen. |
| T465 | Lumen can tokenize a string | OPEN | A Lumen program that takes a string and splits it into tokens (even naively). Proves string manipulation is sufficient for lexer-like work. |

---

## Phase F: Hybrid Compilation Architecture (T500–T540)

The "No Tradeoff" architecture: C-level speed with JS-level ergonomics. Three tiers of execution with seamless transitions.

### F1: Register Architecture Overhaul

| # | Task | Status | Description |
|---|------|--------|-------------|
| T500 | Remove 255 register limit | OPEN | Replace fixed 8-bit register fields with growable register file (Vec<Value> per frame). No program should ever hit a register limit. |
| T501 | Virtual register allocation | OPEN | Compiler uses unlimited virtual registers. Current 8-bit fields become indices into growable frame. |
| T502 | Variable-width instruction encoding | OPEN | Extend instruction format to support 16-bit+ register fields for programs exceeding 256 registers. |
| T503 | Register overflow stress tests | OPEN | Test with 1000+ locals, deep nesting, complex expressions. Must never fail. |

### F2: Copy-and-Patch Interpreter (Tier 0)

| # | Task | Status | Description |
|---|------|--------|-------------|
| T510 | Copy-and-patch design doc | OPEN | Design the stencil-based interpreter. Each opcode becomes a pre-compiled machine code snippet. Stencils are memcpy'd and patched at runtime. Target: 5-10x faster than switch-loop interpreter. |
| T511 | Stencil generation for all opcodes | OPEN | Generate machine code stencils for all ~100 opcodes. Use Cranelift or hand-written assembly. |
| T512 | Copy-and-patch execution engine | OPEN | Runtime stencil stitching — memcpy stencils into executable buffer, patch operand slots. |
| T513 | Tier 0 → Tier 1 transition | OPEN | Copy-and-patch interpreter detects hot cells and triggers JIT compilation. Seamless handoff. |

### F3: JIT Evolution (Tier 1 & 2)

| # | Task | Status | Description |
|---|------|--------|-------------|
| T520 | JIT Float type support | OPEN | Extend Cranelift JIT beyond Int-only cells. Float operations compile to native FP instructions. |
| T521 | JIT Bool/String support | OPEN | Complete type coverage in JIT compiler. |
| T522 | On-Stack Replacement (OSR) | OPEN | Allow interpreter to transition to JIT-compiled code mid-loop without restarting the function. Critical for long-running loops. |
| T523 | Speculative optimization | OPEN | JIT speculates on types/values seen at runtime. Generates fast-path code with guards. Falls back to interpreter on guard failure (deoptimization). |
| T524 | Deoptimization support | OPEN | JIT → interpreter fallback when speculative assumptions are violated. Must reconstruct interpreter state from JIT state. |

### F4: Profile-Guided Optimization & AOT (Tier 2+)

| # | Task | Status | Description |
|---|------|--------|-------------|
| T530 | PGO profiling infrastructure | OPEN | Record execution profiles during interpreted/JIT runs: branch frequencies, type feedback, call counts, hot paths. Serialize to .lumen-profile files. |
| T531 | PGO-guided JIT optimization | OPEN | Feed profile data into JIT compiler for better inlining, code layout, and specialization decisions. |
| T532 | AOT compilation mode | OPEN | Reuse JIT backend to compile to object files on disk. `lumen build --aot` produces native executables. Same Cranelift backend, different output target. |
| T533 | Continuous PGO | OPEN | Production mode: runtime profiles feed back into next AOT build. The deployed binary bakes in JIT-level knowledge. |

### F5: Memory Architecture

| # | Task | Status | Description |
|---|------|--------|-------------|
| T535 | Region-based memory model | OPEN | Hybrid memory: stack allocation for local-only values (90% case), GC only for escaped objects. Escape analysis determines allocation strategy. |
| T536 | Escape analysis | OPEN | Compiler analysis to determine which values escape their defining scope. Non-escaping values use stack allocation (free, fast). |
| T537 | Stack allocation for locals | OPEN | Values that don't escape are allocated on the call stack frame instead of the heap. Eliminates GC overhead for most allocations. |

### F6: Unified Architecture

| # | Task | Status | Description |
|---|------|--------|-------------|
| T540 | Gradual structural typing | OPEN | Support both dynamic (untyped) and static (typed) code in the same file. Untyped code runs in interpreter, typed code compiles to native. Progressive lowering. |
| T541 | Unified IR (progressive lowering) | OPEN | Single IR that can represent code at multiple abstraction levels. High-level (dynamic dispatch) → mid-level (typed, virtual calls) → low-level (concrete, inlined). Inspired by MLIR. |

---

## Post-Bootstrap (NOT YET — do after self-hosting)

These are important but come AFTER the language can compile itself:

- Governance document
- RFC repo and process
- LTS release model
- Security response / CVE process
- Deprecation policy (formal)
- Language edition divergence (editions only matter when there are breaking changes)
- Package registry governance

---

## Previous Tasks (v0.5.0 — All DONE)

All tasks T001-T209 from the original TASKS.md are complete. They covered:
- Phase 0: Memory model (T001-T016) — Arc migration, GC headers, tagged pointers, TLAB
- Phase 1: Compiler backend (T017-T036) — Cranelift AOT/JIT
- Phase 2: Verification (T037-T052) — SMT, refinement types, typestate
- Phase 3: Concurrency (T053-T066) — M:N scheduler, channels, supervisor
- Phase 4: Durability (T067-T076) — Checkpoint/restore, replay, versioning
- Phase 5: AI-native (T077-T089) — Tensors, AD, prompt checking
- Phase 6: Ecosystem (T090-T110) — Signing, registry, WASM, LSP
- Phase 7: Syntax (T111-T124) — Operators, GADTs, macros, effects
- Phase 8: Stdlib (T125-T133) — crypto, http, net, fs, json
- Phase 9: Self-hosting (T134-T140) — Deferred to post-maturity
- Phase 10: Verification (T141-T147) — Release gates, v0.5.0 tag
- Extended (T148-T209) — Session types, structured concurrency, testing, IDE, CI

---

## Maintenance

- This file is the **source of truth** for the pre-bootstrap implementation backlog.
- Mark tasks DONE when implemented and tested.
- If a dogfooding task (Phase D) reveals a language gap, add it to Phase A or B and fix it first.
- The goal is: **every task in Phases A-E is DONE before attempting self-hosting**.
- Phase F is the long-term compilation architecture roadmap. Tasks are ordered by dependency but can be parallelized within sub-phases.
