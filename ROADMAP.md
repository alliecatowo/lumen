# Lumen Roadmap

This roadmap describes the direction of the language and platform. It intentionally avoids dates and fixed timelines.

## Positioning

Lumen is the first language where AI agent behavior is statically verifiable — effect-tracked, cost-budgeted, policy-enforced, and deterministically reproducible. It occupies a new category: **compile-time verification for agent systems.**

---

## Phase 1 — Foundation (Complete)

### Test & Spec

- [x] 1,088+ tests passing, 0 failures across all crates
- [x] SPEC.md rewritten as ground truth (all 45 code blocks compile)

### Language Features

- [x] `when` expressions
- [x] `comptime` expressions
- [x] `extern` declarations
- [x] `yield` statements (generators)
- [x] `~>` compose operator
- [x] Bitwise OR (`|`)

### Compiler & VM

- [x] 76 built-in intrinsics implemented
- [x] Type narrowing in if-is conditions
- [x] Tail call optimization in LIR lowering
- [x] Typed builtin return types (reducing `Type::Any` usage)

### Security & Tooling

- [x] Security stubs replaced with real implementations (lockfile `content_hash` verification, Ed25519 signing active)
- [x] Wares CLI commands fleshed out (trust-check, info, `--frozen`/`--locked` modes)
- [x] URL canonicalization to wares.lumen-lang.com

### Grammar & Docs

- [x] Tree-sitter grammar updated for all new constructs

---

## Phase 2 — Hardening & Performance (Current)

### VM & Runtime

- [ ] VM performance: Rc-wrapping collections for COW semantics, eliminate deep clones
- [ ] Set data structure: replace Vec with proper hash-based set
- [ ] Split `vm.rs` (8,372 lines) into 5 modules
- [ ] Fix index OOB to return errors instead of null

### Compiler

- [ ] Complete let-destructuring lowering to LIR

### Builtins

- [ ] Add missing builtins: `parse_json`, `to_json`, `read_file`, `write_file`, `timestamp`, `random`, `get_env`

### Documentation

- [ ] Documentation cleanup and site updates

---

## Phase 3 — Future

### Language

- [ ] Reference model (`ref` / `mut ref` / `addr`) — gradual ownership
- [ ] For-else syntax

### VM & Performance

- [ ] VM dispatch table optimization

### Tooling

- [ ] LSP capabilities (go-to-definition, hover, completion)

### Self-Hosting & WASM

- [ ] Self-hosting exploration (bootstrapping compiler in Lumen)
- [ ] WebAssembly improvements (multi-file imports, tool providers)

---

## What's Built (Summary)

**Compiler:** Full pipeline — lexer, parser, resolver, typechecker, constraint validator, LIR lowering. ~100 opcodes, 32-bit fixed-width bytecode.

**VM:** Register-based interpreter, call frames (max depth 256), futures/async, memory/machine/pipeline runtimes, orchestration builtins (`parallel`, `race`, `vote`, `select`, `timeout`).

**Providers:** `ToolProvider` trait, `ProviderRegistry`, four crates (http, fs, json, mcp), `lumen.toml` config.

**CLI:** check, run, emit, trace, cache, init, repl, pkg, fmt, doc, lint, build.

**LSP:** Diagnostics, go-to-definition, hover, completion, semantic tokens, symbols, signature help, inlay hints, code actions, folding, references.

---

## Strategic Pillars (Reference)

1. **Language Core** — Mature static types, effect rows, expression completeness, strict diagnostics
2. **Deterministic Runtime** — Replayable execution, explicit async semantics, VM hardening
3. **Agent Semantics** — Typed machines, pipelines, orchestration, trace integration
4. **Capability Model** — Policy-backed enforcement, audit-quality diagnostics
5. **Tooling** — LSP, formatter, package manager, lockfile determinism
6. **Ecosystem** — Implementation-accurate spec, conformance tests, design docs

Provider architecture: language defines contracts, runtime loads implementations. See existing docs for `ToolProvider` trait, provider crates, and `lumen.toml` config.
