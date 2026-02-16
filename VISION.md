# Lumen

**The programming language built for AI agents — and built entirely by them.**

Lumen is a statically typed, markdown-native language designed to be the default output of every AI agent that writes code. Portable, lightweight, runs everywhere. But it's not just for agents to write — it's a standalone language engineered to be a genuine pleasure to work in. A fresh start. A chance to do everything other languages got wrong, right.

---

## Origin Story

Lumen is the world's first serious programming language where no human has written a single line of code. It started as a single prompt and grew from there — every function, every opcode, every test, every line of the compiler, VM, and runtime authored entirely by AI agents.

It's partly an experiment: what happens when you vibecode an entire language? Not a toy, not a demo — a real language with a real type system, a real compiler pipeline, and a register-based VM written in Rust. Over 1,300 tests. Thirty runnable examples. A package manager with supply-chain integrity. All of it agent-developed, from the first commit.

The result speaks for itself.

---

## Core Identity

**Markdown-native.** Not markdown-first — markdown-native. The language lives in two modes: `.lm` files where code is default and fenced blocks are rich markdown docstrings, and `.lm.md` files where markdown is default and `lumen` blocks are executable code. Docstrings are markdown, so hover docs render with full formatting. Imagine if you could run what you need from Python, TypeScript, or Rust — but in markdown. That's Lumen.

**AI-native.** Tools, grants, effects, process runtimes, and orchestration primitives are language constructs, not libraries. Lumen doesn't bolt AI capabilities onto a general-purpose language — it builds them into the grammar. Agents write Lumen because the language already speaks their vocabulary: tool dispatch, policy enforcement, typed pipelines, state machines, memory, traces.

**Statically typed, zero ceremony.** Records, enums, pattern matching, exhaustiveness checks, `result[T, E]`, optional sugar (`T?`), union types. The type system catches errors at compile time. The grammar is small enough to hold in your head. Cells are functions. Types are concise. You spend time on logic, not boilerplate.

**High-level by default, genuinely low-level when needed.** Pipe operators, function composition, string interpolation, `when` expressions, generator cells. And when you need it: `extern` for FFI, `comptime` for compile-time evaluation, `defer` for scope-exit cleanup, bitwise ops and shifts, `@deterministic` mode. Rust-grade safety without Rust-grade complexity. No ceiling.

**Portable.** Compiles to LIR bytecode. Runs on a register-based VM written in Rust. Builds to WASM for browser, Node.js, and WASI targets. One language, every platform.

---

## Design Philosophy

**Beautiful syntax.** Indentation-based blocks terminated by `end`. Light on tokens — critical when agents are generating code at scale. Comments use `#`. The grammar is minimal and consistent. Every construct earns its place.

**No compromises.** The VM is rock solid — a register-based interpreter with a 32-bit fixed-width instruction set, deterministic execution semantics, and a seven-stage compiler pipeline that catches errors before your code ever runs. This is not a prototype. This is infrastructure.

**Correctness over cleverness.** Match exhaustiveness is checked. Pipeline stage arity is strict. Machine transition types must match target state payloads. Grant policies are validated at dispatch. Effect provenance is tracked — you know exactly what side effects a cell can perform and where they originate.

**The language other languages wish they'd been.** Take the ergonomics of Python, the safety of Rust, the composability of ML-family languages, and the AI-native primitives that none of them have. Strip out the historical baggage, the footguns, the ceremony. That's the target.

---

## Language Features

**Algebraic effects.** Full algebraic effects with handlers and resumptions. Cells declare effect rows (`/ {http, trace}`). Effect bindings map to tools. Grants scope capability with constraints. You get compile-time visibility into every side effect in your program — and the power to handle, intercept, and resume them.

**Process runtimes.** Memory, machine, pipeline, and orchestration are first-class constructs. Memory gives you structured recall and storage. Machines give you typed state graphs with guards and transitions. Pipelines auto-chain stages with strict type flow. Orchestration primitives — `parallel`, `race`, `vote`, `select`, `timeout` — execute with deterministic semantics.

**Type system.** Primitives, records with `where` clauses, enums with payloads, unions, generics, optional sugar, result types. Function types with effect rows. Collections — `list[T]`, `map[K, V]`, `set[T]`, `tuple[...]`. Exhaustive pattern matching. The compiler validates everything before the VM sees it.

**Module system.** File-based imports with automatic dependency resolution. Wildcard and named imports with aliasing. Circular import detection with full chain reporting. Compiled modules merge cleanly — no duplicate definitions, deduplicated string tables.

---

## Ecosystem

**Wares** is the package manager, and it's built for a world where AI agents depend on third-party code. Sigstore-style keyless signing. Append-only transparency log. SAT/CDCL dependency resolution. Content-addressed lockfiles. Trust policies that require signatures and verifiable provenance. When you `wares install`, you know exactly what you're getting and where it came from.

**Tooling.** Tree-sitter grammar. VS Code extension with syntax highlighting. Formatter with CI mode. REPL with history and multi-line input. Trace recording and replay. LSP in progress. WASM builds for browser and edge. The CLI covers the full lifecycle: `check`, `run`, `emit`, `fmt`, `pkg`, `trace`.

**Providers.** HTTP, JSON, filesystem, and MCP providers plug into the tool registry. The runtime dispatches based on capability and policy. Structured error types — rate limits, auth failures, timeouts, model-not-found — enable robust handling across any AI provider.

---

## The Direction

Lumen is heading toward being the language you reach for — period. Not just for AI workloads, but for anything where you want static types, beautiful syntax, and a runtime that doesn't let you down.

The path forward: maturing the LSP into a world-class editing experience. Growing the standard library. Hardening Wares for production supply chains. Expanding WASM deployment targets. Deepening the effect system. Building out the ecosystem so that agents and humans alike choose Lumen because it's simply the best tool for the job.

No tradeoffs we're comfortable with. No ceilings we accept. Every limitation is a bug on the roadmap.

---

A language for AI agents, by AI agents, that humans love writing in. High-level by default, low-level when it counts. Markdown-native, statically typed, effect-tracked, and portable. The language that starts where other languages stop.
