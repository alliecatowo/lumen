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
| T300 | HTTP client/server implementation | OPEN | `rust/lumen-runtime/src/http.rs` is type definitions only — no actual I/O. Implement real HTTP client using ureq or reqwest (sync, behind builtin). Add builtins: `http_get(url) -> result[String, String]`, `http_post(url, body, headers) -> result[String, String]`. |
| T301 | TCP/UDP networking implementation | OPEN | `rust/lumen-runtime/src/net.rs` is type stubs only. Implement real TCP connect/listen/accept/send/recv and UDP send/recv builtins using std::net. |
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
| T331 | Edition-aware parsing | OPEN | Parser checks edition to determine which syntax is available. Future editions can change defaults (e.g., strictness levels) without breaking old code. |
| T332 | Edition migration tool | OPEN | `lumen migrate --edition 2027` command that automatically updates code for a new edition (when editions diverge). Stub for now — the infrastructure matters. |

### A5: Stability & Semver

| # | Task | Status | Description |
|---|------|--------|-------------|
| T335 | Stability guarantees document | OPEN | Write `docs/STABILITY.md` defining what is guaranteed across minor/patch versions. Builtins, syntax, semantics that are `stable` will not break in minor versions. |
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
| T342 | String formatting / printf-style | OPEN | Ensure string interpolation with format specs covers all cases: `{value:>10.2f}`, `{value:#x}`, `{value:08b}`. Verify the `__format_spec` builtin handles all format types. |
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
| T360 | Mutable map operations | OPEN | Verify `map_insert(m, k, v)`, `map_remove(m, k)`, `map_keys(m)`, `map_values(m)`, `map_entries(m)` all work correctly. Maps are critical for symbol tables. |
| T361 | Sorted map / tree map | OPEN | `sorted_map[K, V]` type or `map_sorted_keys(m) -> list[K]` — ordered iteration. Needed for deterministic output (e.g., compiler producing same output regardless of hash ordering). |
| T362 | String-to-int/float parsing with error | OPEN | `parse_int(s) -> result[Int, String]` and `parse_float(s) -> result[Float, String]` — currently `to_int`/`to_float` exist but error behavior may not be clean. Ensure they return proper result types. |
| T363 | Math builtins completeness | OPEN | Verify: `abs`, `min`, `max`, `clamp`, `pow`, `sqrt`, `log`, `log2`, `log10`, `ceil`, `floor`, `round`, `pi`, `e` (constants), `inf`, `nan`, `is_nan`, `is_infinite` all work. |
| T364 | Sorting with custom comparator | OPEN | `sort_by(list, comparator_fn) -> list[T]` — sort with a user-provided comparison function. Needed for flexible data processing. |
| T365 | Binary search | OPEN | `binary_search(sorted_list, value) -> result[Int, Int]` — returns ok(index) if found, err(insertion_point) if not. |

### B4: Time & System

| # | Task | Status | Description |
|---|------|--------|-------------|
| T366 | High-resolution timer | OPEN | `hrtime() -> Int` — nanosecond-resolution monotonic timer for benchmarking. `timestamp()` exists but returns Float seconds. |
| T367 | Date/time formatting | OPEN | `format_time(ts, format) -> String` — format a timestamp. At minimum ISO 8601 output. |
| T368 | Process arguments | OPEN | `args() -> list[String]` — get command-line arguments passed to the program. Essential for any CLI tool written in Lumen. |
| T369 | Environment variable ops | OPEN | `get_env` exists — verify `set_env(key, value)` works too. Add `env_vars() -> map[String, String]` to get all env vars. |
| T370 | Exit with code | OPEN | `exit(code: Int)` — verify this works. `exit(0)` for success, `exit(1)` for failure. |

---

## Phase C: Compiler & VM Hardening (T380–T410)

Fix real bugs and edge cases that will block self-hosting.

### C1: Compiler Correctness

| # | Task | Status | Description |
|---|------|--------|-------------|
| T380 | Enum variant construction with payloads | OPEN | `Option.Some(42)` and `Shape.Circle(radius: 5.0)` were reported broken at runtime ("cannot call null"). Verify this is fixed and add thorough tests. If not fixed, fix it. |
| T381 | Generic record construction | OPEN | `Box[Int](value: 42)` and `Pair[String, Int]("hello", 1)` were reported broken. Verify and fix. |
| T382 | Nested comprehension scoping | OPEN | `[f(x, y) for x in a for y in b]` — inner loop variable was undefined. Verify fix and add tests. |
| T383 | Effect handler resume correctness | OPEN | `resume()` inside effect handlers was reported failing with "resume called outside of effect handler". This is fundamental — effects are a core feature. Fix and add comprehensive tests. |
| T384 | i64::MIN literal handling | OPEN | Literal `-9223372036854775808` triggers "cannot negate" because the parser parses `9223372036854775808` (which overflows i64) then negates. Fix: parse negative literals as a unit, or handle the overflow case. |
| T385 | Continue in for-loops | OPEN | `continue` in for-loops was reported hitting instruction limits (possible VM bug). Fix so continue correctly advances the iterator. |
| T386 | Record method scoping with generics | OPEN | Records with generic method cells (e.g., `Stack[T]` with `push`, `pop`) cause duplicate definition and undefined type T errors. Fix resolver to properly scope generic parameters in methods. |

### C2: VM Performance & Correctness

| # | Task | Status | Description |
|---|------|--------|-------------|
| T390 | String interning optimization | OPEN | Profile string operations in the VM. If string comparison is a bottleneck (it will be for a compiler), optimize the interner — consider using a hash map with pre-computed hashes. |
| T391 | Map operation performance | OPEN | BTreeMap is used for maps — profile insert/lookup for large maps (10K+ entries). Consider offering a HashMap alternative for performance-critical paths. |
| T392 | Closure capture correctness audit | OPEN | Audit all closure capture and upvalue patterns. Ensure closures closing over mutable variables work correctly, especially in loops. Add edge-case tests. |
| T393 | Large function compilation | OPEN | Compile a function with 500+ lines, many locals, deep nesting. Ensure register allocator doesn't overflow or produce incorrect code. |
| T394 | Tail call optimization verification | OPEN | Write a tail-recursive function that recurses 1M+ times. Verify TCO prevents stack overflow. If it doesn't, fix it. |

### C3: Error Quality

| # | Task | Status | Description |
|---|------|--------|-------------|
| T400 | Error message quality audit | OPEN | Compile 20 intentionally broken programs. Review every error message for: clarity, source location accuracy, suggested fix quality. Fix any that are confusing or wrong. |
| T401 | Runtime error stack traces | OPEN | When a runtime error occurs (index out of bounds, type mismatch, etc.), ensure the error includes: cell name, line number, source file. Currently errors may be opaque. |
| T402 | Undefined variable suggestion quality | OPEN | When an undefined variable is used, the Levenshtein suggestion should actually help. Test with common typos and verify suggestions are useful. |

---

## Phase D: Dogfooding — Write Real Tools in Lumen (T420–T445)

Every tool we need should be writable in Lumen. If it can't be, that's a Phase A/B gap to fix first.

| # | Task | Status | Description |
|---|------|--------|-------------|
| T420 | Rewrite bench/generate_report.py in Lumen | OPEN | The benchmark report generator is currently Python. Rewrite it as `bench/generate_report.lm`. It needs: CSV parsing, statistics (mean/median/stddev), string formatting, file I/O. If any of these are missing, implement them first (T343, T350, etc.). |
| T421 | Write a Lumen test runner in Lumen | OPEN | A program that: walks a directory of `.lm` files, compiles each, runs each, reports pass/fail with colors. Proves Lumen can do file I/O, subprocess execution, string manipulation. |
| T422 | Write a Lumen LOC counter in Lumen | OPEN | Count lines of code across the project — `.lm`, `.lm.md`, `.rs` files. Simple but proves directory walking and file reading work. |
| T423 | Write a Lumen TOML config reader | OPEN | Read `lumen.toml` and print parsed values. Proves TOML parsing works. |
| T424 | Write a Lumen JSON pretty-printer | OPEN | Read a JSON file, pretty-print it with indentation. Proves JSON builtins work end-to-end. |
| T425 | Write a Lumen markdown table generator | OPEN | Generate this TASKS.md-style markdown tables from structured data. Proves string formatting is adequate. |
| T426 | Write a Lumen diff tool | OPEN | Compare two files line-by-line, output unified diff format. Proves string comparison and file I/O work. |
| T427 | Write a Lumen version bumper in Lumen | OPEN | Replace `scripts/bump-version.sh` with a Lumen program that reads Cargo.toml, package.json, etc., and updates version strings. Proves file read/write and string replacement work. |
| T428 | Write benchmark runner in Lumen | OPEN | Replace parts of `bench/run_all.sh` that can be Lumen — at minimum the results aggregation and CSV output. The shell script can call Lumen for the report part. |

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
