# Roadmap

This roadmap reflects the actual implementation status of Lumen as of February 2025. Items are marked as Complete, In Progress, or Planned based on what's actually built and working.

## Phase 1: Core Language [Complete]

**Status:** Fully implemented and tested. All core language features are production-ready.

### Compiler Pipeline
- Lexer, parser, resolver, typechecker, constraint validator, LIR lowerer
- Register-based VM with 74+ opcodes (32-bit fixed-width instructions)
- Multi-file compilation with import resolution and circular dependency detection

### Type System
- All primitive types: Int, Float, String, Bool, Bytes, Json, Null
- Collections: List, Map, Set, Tuple with Rc copy-on-write semantics
- Records with where-clause constraints
- Enums with payloads
- Pattern matching with exhaustiveness checking
- Union types, optional sugar (`T?`), result types (`result[T, E]`)

### Control Flow
- if/else, for, while, loop, match statements
- break/continue with labels
- try expressions for error handling

### Language Features
- String interpolation
- Range expressions (`1..5`, `1..=5`)
- Pipe operator (`|>`)
- Compose operator (`~>`)
- Closures with upvalue capture
- Import system with wildcard and named imports

### Examples & Testing
- 30 working examples covering all language features
- 1,365+ tests passing across compiler, VM, and runtime

---

## Phase 2: Advanced Features [Complete]

**Status:** All advanced language features implemented and integrated into the compiler, VM, and tooling.

### Language Features
- Full algebraic effects: `perform`, `handle`, `resume` with one-shot continuations
- `when` expressions (multi-branch conditionals)
- `comptime` expressions (compile-time evaluation)
- `defer` statements (LIFO scope-exit cleanup)
- `extern` declarations (FFI boundary)
- `yield` statements (generator-style values)

### Source Format
- Markdown-native `.lm` files (triple-backtick blocks as markdown comments/docstrings)
- `.lumen` file extension support
- Docstrings attached to declarations (cells, records, enums, handlers)

### LSP
- Hover with markdown docstring rendering
- Completion with context-aware suggestions
- Go-to-definition
- Semantic tokens
- Document symbols (cells as Functions, records as Structs, enums with members)
- Signature help with parameter labels and docstrings
- Folding ranges (code blocks and markdown comments)
- Diagnostics with source context

### VS Code Extension
- TextMate grammar for syntax highlighting
- Tree-sitter grammar for advanced tooling
- Language configuration (folding, indentation)
- Format-on-save and lint-on-save support

### Formatter
- `lumen fmt` command with markdown block preservation
- Docstring attachment preservation
- CI mode (`--check` flag)

### Builtins
- 7 new builtins: `parse_json`, `to_json`, `read_file`, `write_file`, `timestamp`, `random`, `get_env`
- 76 total builtin intrinsics with typed return signatures

### VM Performance
- Rc-wrapped collections (List, Tuple, Map, Record) for copy-on-write
- BTreeSet for Set (replacing Vec-based implementation)
- VM module split into `mod.rs`, `intrinsics.rs`, `processes.rs`, `ops.rs`, `helpers.rs`
- Index out-of-bounds returns runtime errors (with Python-style negative indexing)

---

## Phase 2.5: Package Manager "Wares" [Mostly Complete]

**Status:** Core package manager infrastructure complete. Cryptographic signing and registry deployment pending.

### Manifest & Lockfile
- `lumen.toml` manifest with full schema
- `lumen.lock` v4 content-addressed lockfile
- SAT/CDCL dependency resolver

### Package Naming
- Mandatory `@namespace/name` package naming
- Correct `@scope/name@version` parsing

### CLI Commands
- `init` — Create new package
- `add` / `remove` — Manage dependencies
- `list` — Show installed packages
- `install` / `update` — Install/update dependencies
- `publish` — Publish packages
- `login` / `logout` — Registry authentication
- `search` — Search registry
- `info` — Package information
- `trust-check` — Verify package signatures
- `policy` — Manage trust policies
- `--frozen` and `--locked` modes enforced

### Security Infrastructure
- Sigstore-style keyless signing (stub implementation — needs real crypto)
- Trust policy enforcement
- Content hash verification in lockfile
- URL canonicalization to `wares.lumen-lang.com`

### Registry Infrastructure
- Cloudflare Workers registry scaffolded
- D1 + R2 storage planned
- Not yet deployed

---

## Phase 3: Production Readiness [In Progress]

**Status:** Core language is production-ready. Remaining work focuses on ecosystem maturity, performance optimization, and advanced language features.

### Documentation
- [ ] Auto-generated language reference from compiler source
- [ ] Comprehensive standard library documentation

### Security
- [ ] Real cryptographic signing (replace stubs with ed25519/sigstore)
- [ ] Transparency log implementation
- [ ] Registry deployment (Cloudflare Workers + D1 + R2)

### WASM Target
- [ ] Multi-file imports in WASM builds
- [ ] Tool providers in WASM runtime
- [ ] Improved browser/Node.js/WASI integration

### Performance
- [ ] Performance benchmarks and optimization pass
- [ ] VM dispatch table optimization
- [ ] Compiler performance improvements

### Language Features
- [ ] Gradual ownership system (`ref T`, `mut ref T`, `addr`)
- [ ] Standard library bootstrap
- [ ] Self-hosting exploration (Lumen-in-Lumen compiler)

---

## Phase 4: Ecosystem [Planned]

**Status:** Future work to grow the Lumen ecosystem and developer experience.

### Package Registry
- [ ] Live registry with real community packages
- [ ] Package discovery and curation tools
- [ ] Versioning and compatibility policies

### Tooling
- [ ] Community formatters and linters
- [ ] Editor plugins beyond VS Code (Vim, Emacs, etc.)
- [ ] Language server improvements (rename, refactor, code actions)
- [ ] Debugging support
- [ ] Profiling tools

### Developer Experience
- [ ] WebAssembly playground
- [ ] Interactive tutorials and guides
- [ ] Community examples and patterns

### AI Agent Integration
- [ ] AI agent SDK / Lumen runtime for agent frameworks
- [ ] Tool provider ecosystem expansion
- [ ] Agent-specific debugging and tracing tools

---

## Summary

**Complete:** Core language, advanced features, LSP, VS Code extension, formatter, package manager CLI, and VM performance optimizations.

**In Progress:** Production hardening (crypto signing, registry deployment, WASM improvements, performance benchmarks, standard library).

**Planned:** Ecosystem growth (live registry, community tooling, debugging support, AI agent SDK).

The language is ready for real-world use. Remaining work focuses on ecosystem maturity and production infrastructure.
