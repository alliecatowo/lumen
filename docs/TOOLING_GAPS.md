# Developer Tooling Gap Analysis and Adoption Roadmap

**Date:** February 13, 2026
**Purpose:** Identify essential developer tooling gaps for Lumen language adoption

## Executive Summary

This document analyzes the current state of Lumen's developer tooling against industry standards set by mature languages (Rust, TypeScript, Go) in 2025-2026. While Lumen has a strong foundation with an LSP server, VS Code extension, CLI tools (fmt, lint, test, doc, repl, pkg), and tree-sitter grammar, significant gaps remain in areas critical for developer adoption.

**Priority Assessment:**
- **CRITICAL** (P0): Must-have for any serious adoption
- **HIGH** (P1): Expected by modern developers
- **MEDIUM** (P2): Nice-to-have, improves experience
- **LOW** (P3): Future enhancement

---

## Current State: What We Have

### ✅ Core Infrastructure (Strong Foundation)

1. **LSP Server** (`rust/lumen-lsp/src/main.rs`)
   - Go-to-definition
   - Hover documentation with formatted signatures
   - Completion (keywords, builtins, user-defined symbols)
   - Real-time diagnostics
   - Full document sync
   - Symbol indexing across open documents

2. **VS Code Extension** (`editors/vscode/`)
   - Syntax highlighting (TextMate grammar)
   - Language configuration (brackets, comments)
   - LSP client integration
   - Commands: check, run, fmt
   - Keybindings (F5 to run, Ctrl+Shift+B to check)
   - Snippets
   - Support for both `.lm` and `.lm.md` files

3. **CLI Tooling** (`rust/lumen-cli/src/`)
   - `lumen fmt` - Code formatter
   - `lumen lint` - Linter with configurable rules
   - `lumen test` - Test runner (discovers `test_*` cells)
   - `lumen doc` - Documentation generator
   - `lumen repl` - Interactive REPL
   - `lumen pkg` - Package manager (init, build, check)
   - `lumen check` - Type checker
   - `lumen run` - Executor with trace support
   - `lumen emit` - LIR bytecode emitter

4. **Tree-sitter Grammar** (`tree-sitter-lumen/`)
   - Comprehensive grammar coverage
   - Syntax highlighting queries
   - Locals/scopes queries
   - Foundation for advanced tooling

---

## Gap Analysis by Category

### 1. Language Server Protocol (LSP) Enhancement

**Current:** Basic LSP with definition, hover, completion, diagnostics
**Industry Standard:** Rust-analyzer (2025) - the gold standard

#### Gaps:

**CRITICAL (P0):**
- ❌ **Code actions / Quick fixes** - Essential for productivity
  - Example: "Add missing import", "Extract to cell", "Inline variable"
  - rust-analyzer provides 50+ code actions

- ❌ **Workspace-wide symbol search** (`workspace/symbol`)
  - Search all cells, records, enums across project
  - rust-analyzer indexes entire workspace instantly

- ❌ **Find all references** - Navigate usage sites
  - Critical for refactoring and understanding code flow

- ❌ **Rename refactoring** - Safe cross-file renaming
  - Must handle symbol resolution correctly

**HIGH (P1):**
- ❌ **Inlay hints** (LSP 3.17 feature, stabilized in 2025)
  - Show inferred types inline: `let x = get_user()  // : User`
  - Parameter names in calls: `fetch(url: "...", timeout: 5000)`
  - Dramatically improves code readability

- ❌ **Semantic tokens** (LSP 3.16 feature)
  - Context-aware syntax highlighting (distinguish locals vs params vs captures)
  - Highlight mutable variables differently

- ❌ **Document symbols** - Outline view / breadcrumbs
  - Show file structure (cells, records, processes)

- ❌ **Call hierarchy** - Navigate call graph up/down
  - "Show callers" and "Show callees"

- ❌ **Type hierarchy** (LSP 3.17 feature)
  - Navigate record inheritance, trait impls (when added)

**MEDIUM (P2):**
- ❌ **Folding ranges** - Collapse code blocks
- ❌ **Document formatting on-type** - Auto-format as you type
- ❌ **Signature help** - Show parameter info while typing call
- ❌ **Document links** - Click imports to navigate
- ❌ **Workspace diagnostics** - Show all errors across project
- ❌ **Code lens** - Inline "Run test" / "Debug" buttons

**Implementation Strategy:**
- rust-analyzer uses Salsa for incremental computation (see [Incremental Compilation](#9-incremental-compilation-and-caching))
- Requires full semantic analysis pass, not just AST
- Consider using `tower-lsp` crate (already standard in Rust ecosystem)
- Study rust-analyzer's approach: https://github.com/rust-lang/rust-analyzer

**References:**
- [LSP 3.17 Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/)
- [rust-analyzer 2025 releases](https://github.com/rust-lang/rust-analyzer/releases)

---

### 2. Debug Adapter Protocol (DAP)

**Current:** ❌ No debugger support
**Industry Standard:** VS Code debugger, lldb, gdb

#### What's Missing:

**CRITICAL (P0):**
- ❌ **DAP server implementation**
  - Protocol: https://microsoft.github.io/debug-adapter-protocol/
  - Intermediary between IDE and runtime debugger

- ❌ **Breakpoint support in VM**
  - Pause execution at source lines
  - Conditional breakpoints

- ❌ **Step execution** (step in/out/over)
  - Single-step through bytecode with source mapping

- ❌ **Variable inspection**
  - View locals, captures, upvalues
  - Drill into records, lists, maps

- ❌ **Call stack inspection**
  - Show active frames with source locations

**HIGH (P1):**
- ❌ **Watch expressions** - Evaluate arbitrary expressions in paused context
- ❌ **REPL in debug context** - Execute code at breakpoint
- ❌ **Exception breakpoints** - Pause on runtime errors
- ❌ **Hot reload / Edit and continue** - Modify code while debugging

**Implementation Strategy:**
1. Add instruction pointer tracking to VM with source line mapping
2. Implement DAP server (similar to rust-debugger, lldb-vscode)
3. Extend VM with pause/resume/step primitives
4. VS Code debug configuration in extension
5. Consider GDB integration for lower-level debugging

**Example Workflow:**
```jsonc
// .vscode/launch.json
{
  "type": "lumen",
  "request": "launch",
  "name": "Debug main cell",
  "program": "${file}",
  "cell": "main"
}
```

**References:**
- [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol/)
- [nvim-dap example](https://github.com/mfussenegger/nvim-dap)

---

### 3. Multi-Editor Support

**Current:** ✅ VS Code, ⚠️ Partial support via LSP for others
**Industry Standard:** First-class support in VS Code, Neovim, Helix, Zed, JetBrains

#### Gaps:

**HIGH (P1):**
- ❌ **JetBrains plugin** (IntelliJ IDEA, RustRover, etc.)
  - Large enterprise adoption demographic
  - Requires JetBrains Plugin SDK integration
  - 2025 trend: JetBrains AI Assistant integration

- ❌ **Neovim / vim-lumen**
  - Critical for terminal-based developers
  - Tree-sitter integration already possible
  - LSP client via nvim-lspconfig
  - Example config:
    ```lua
    require'lspconfig'.lumen_lsp.setup{}
    ```

**MEDIUM (P2):**
- ❌ **Helix editor support**
  - Minimalist Rust-powered editor (growing in 2025)
  - Just needs LSP config + tree-sitter grammar

- ❌ **Zed editor support**
  - Fastest editor in 2025 (Rust-based)
  - Windows support arrived in late 2025
  - Extension API: https://zed.dev/extensions

- ❌ **Sublime Text LSP plugin**
- ❌ **Emacs lumen-mode**

**Implementation Strategy:**
- LSP server already works cross-editor
- Publish tree-sitter-lumen to registries
- Create minimal editor configs / plugins
- Document setup in official docs
- Community-driven extensions (lower priority)

**2025-2026 Editor Trends:**
- Zed is now cross-platform (Windows support)
- Neovim + LazyVim popular for AI-augmented coding
- JetBrains IDEs added AI Assistant (deep integration)
- Helix gaining traction as Rust-powered vim alternative

**References:**
- [Neovim + LazyVim vs VS Code comparison](https://www.tuliocunha.dev/blog/neovim-lazyvim-vs-vscode-jetbrains-zed-helix-2025/)
- [Best IDEs for Rust 2025](https://www.analyticsinsight.net/programming/best-ides-for-rust-development-in-2025/)

---

### 4. Package Registry

**Current:** ✅ Local package system, ❌ No public registry
**Industry Standard:** crates.io, npm, PyPI

#### What's Missing:

**CRITICAL (P0):**
- ❌ **Public package registry** (lumen.io / lumenpkg.dev)
  - Central repository for community packages
  - Semantic versioning enforcement
  - Package signing / verification
  - Checksum validation

**HIGH (P1):**
- ❌ **Package search / discovery**
  - Web UI for browsing packages
  - CLI: `lumen pkg search <query>`
  - Metadata: downloads, stars, categories

- ❌ **Dependency resolution algorithm**
  - Cargo uses SAT solver for complex dep graphs
  - Handle version conflicts

- ❌ **Publishing workflow**
  - `lumen pkg publish` command
  - Authentication (API keys)
  - Ownership / teams

- ❌ **Registry API specification**
  - REST API for package metadata
  - Download endpoints
  - Versioned protocol

**MEDIUM (P2):**
- ❌ **Private registry support**
  - Enterprise use case
  - Self-hosted option
  - Alternative registry URLs in config

- ❌ **Yanking versions** - Mark broken versions as deprecated
- ❌ **Package badges** - Display version, downloads
- ❌ **Docs hosting** - Auto-generate docs on publish (like docs.rs)

**LOW (P3):**
- ❌ **Registry mirroring** - CDN distribution
- ❌ **Binary caching** - Pre-built artifacts

**Implementation Phases:**

1. **Phase 1: MVP Registry (3-6 months)**
   - Simple web server (Rust + Axum)
   - PostgreSQL database
   - Basic auth (API keys)
   - Package upload/download
   - CLI: `lumen pkg publish`, `lumen pkg install`

2. **Phase 2: Discovery & Metadata (2-3 months)**
   - Search endpoint
   - Web UI (Next.js / SolidJS)
   - Package pages with READMEs
   - Download statistics

3. **Phase 3: Advanced Features (ongoing)**
   - Docs hosting (docs.lumen.io)
   - Private packages
   - Teams / organizations
   - Yanking support

**Architecture Reference:**
- crates.io is open source: https://github.com/rust-lang/crates.io
- Study Cargo's registry API spec
- Consider using Cloudflare R2 for package storage

**References:**
- [crates.io package registry](https://crates.io/)
- [How to build a package registry](https://blog.packagecloud.io/how-do-i-build-a-package-registry/)

---

### 5. Online Playground

**Current:** ❌ No web playground
**Industry Standard:** Rust Playground, TypeScript Playground, Go Playground

#### What's Missing:

**HIGH (P1):**
- ❌ **Web-based code editor** (play.lumen.dev)
  - Monaco Editor (powers VS Code)
  - Syntax highlighting (TextMate grammar reuse)
  - LSP over WebSocket for intellisense

- ❌ **WASM compilation target**
  - Compile Lumen → LIR → WASM
  - Run VM in browser (see [WASM Compilation](#9-wasm-compilation-target))

- ❌ **Share/permalink feature**
  - Save snippets to database
  - Generate short URLs: `play.lumen.dev/abc123`

- ❌ **Example library**
  - Curated examples from `/examples/`
  - "Hello World", "HTTP Client", "State Machines"

**MEDIUM (P2):**
- ❌ **Run multiple cells** - Execute full programs
- ❌ **Output formatting** - Pretty-print results
- ❌ **Execution timeout** - Prevent infinite loops
- ❌ **Assembly view** - Show generated LIR bytecode
- ❌ **Dark/light theme**
- ❌ **Mobile responsive**

**LOW (P3):**
- ❌ **Multiplayer editing** - Collaborative coding (like repl.it)
- ❌ **Version selector** - Run against different Lumen versions
- ❌ **Embedding** - Iframe for docs

**Implementation Strategy:**

1. **Frontend (React/SolidJS + Monaco)**
   ```typescript
   import * as monaco from 'monaco-editor';
   import { LumenWorker } from './lumen-wasm';

   const editor = monaco.editor.create(element, {
     language: 'lumen',
     theme: 'lumen-dark',
   });

   // Compile and run via WASM
   const output = await LumenWorker.execute(code);
   ```

2. **Backend (Rust + Axum)**
   - Snippet storage (PostgreSQL)
   - Rate limiting (prevent abuse)
   - Optional: server-side execution in sandboxed containers

3. **WASM VM** (see [Section 9](#9-wasm-compilation-target))
   - Compile `lumen-vm` to WASM
   - Expose JS API: `compileAndRun(source)`

**WebAssembly in 2025-2026:**
- WASM 3.0 released (Sept 2025): GC types, multiple address spaces, exception handling
- Kotlin/Wasm reached beta (Sept 2025)
- 30+ languages compile to WASM
- Tools: wasm-pack, wasm-bindgen, wasmtime

**References:**
- [WebAssembly 3.0 announcement](https://webassembly.org/news/2025-09-17-wasm-3.0/)
- [State of WebAssembly 2025-2026](https://platform.uno/blog/the-state-of-webassembly-2025-2026/)
- [Awesome WASM Languages](https://wasmlang.org/)

---

### 6. Error Message Quality

**Current:** ✅ Good diagnostics with source context, ⚠️ Room for improvement
**Industry Standard:** Elm (best-in-class), Rust (exceptional)

#### Current Strengths:
- Source line/column tracking
- Formatted error output (`lumen_compiler::format_error`)
- Context snippets with line numbers

#### Gaps:

**HIGH (P1):**
- ❌ **Actionable suggestions** - Tell users HOW to fix, not just what's wrong
  - Example (Rust):
    ```
    error[E0308]: mistyped match arm
      --> src/main.rs:5:9
       |
    5  |         "hello" => 1,
       |         ^^^^^^^ expected Bool, found String
       |
    help: did you mean to compare the value?
       |
    5  |         x == "hello" => 1,
       |         ++++
    ```

- ❌ **Error codes** - Categorize errors with unique codes
  - `E1001: UndefinedType`, `E2015: TypeMismatch`
  - Enables documentation: `lumen explain E1001`

- ❌ **Friendly tone** - Avoid compiler jargon
  - Elm uses first-person: "I see you're trying to..."
  - Present tense over past tense

**MEDIUM (P2):**
- ❌ **Multiple error locations** - Highlight related spans
  - "Mismatch here... defined there ↑"

- ❌ **Color-coded severity**
  - Red: errors
  - Yellow: warnings
  - Blue: notes/hints

- ❌ **Did-you-mean suggestions**
  - "Cannot find `lenght`. Did you mean `len`?"
  - Use Levenshtein distance

**LOW (P3):**
- ❌ **Error recovery** - Continue parsing after errors
- ❌ **Batch errors** - Show multiple errors, not just first
- ❌ **Warning suppression** - `@allow(unused_variable)`

**Design Principles (Elm/Rust best practices):**

1. **Concise** - 1-2 sentences in editors
2. **Present tense** - "This is a type error" not "Found a type error"
3. **First person** - "I expected X" (Elm style) builds rapport
4. **Show, don't tell** - Highlight exact problematic code
5. **Suggest fixes** - Actionable next steps
6. **Avoid jargon** - "Cannot call `fetch` here" not "Effect row violation"

**Example Enhancement:**

Current:
```
error: Type mismatch at line 15
  expected: Int
  found: String
```

Improved:
```
error[E2108]: Type mismatch
  --> example.lm.md:15:12
   |
15 |     let x: Int = get_name()
   |            ^^^   ---------- this returns String
   |            |
   |            expected Int because of this type annotation
   |
help: remove the type annotation to let the compiler infer the type
   |
15 |     let x = get_name()
   |       - -
```

**References:**
- [Elm error message style guide](https://discourse.elm-lang.org/t/error-messages-style/7828)
- [Comparing compiler errors across languages](https://www.amazingcto.com/developer-productivity-compiler-errors/)
- [Writing good compiler error messages](https://calebmer.com/2019/07/01/writing-good-compiler-error-messages.html)

---

### 7. Profiling and Performance Tools

**Current:** ⚠️ Basic `--trace-dir` support, ❌ No visual profiling
**Industry Standard:** perf + flamegraphs, cargo-flamegraph, Chrome DevTools

#### What's Missing:

**HIGH (P1):**
- ❌ **Execution profiler**
  - Sample-based profiling (interrupt at intervals)
  - Record function call stacks
  - Generate flamegraphs (SVG visualization)
  - `lumen profile run program.lm.md`

- ❌ **Instruction-level profiling**
  - Which LIR opcodes are hot?
  - VM optimization opportunities

- ❌ **Memory profiler**
  - Heap allocation tracking
  - Leak detection
  - Object lifetime visualization

**MEDIUM (P2):**
- ❌ **Benchmarking framework**
  - `bench_*` cells (like Rust's `#[bench]`)
  - Statistical analysis (mean, std dev, outliers)
  - Compare before/after optimizations

- ❌ **Compile-time profiling**
  - Which compiler pass is slow?
  - Self-profiling (like rustc `-Zself-profile`)

- ❌ **Trace visualization**
  - Web UI for trace events (timeline view)
  - Filter by event type, cell name

**Implementation Strategy:**

1. **VM Profiler (Phase 1)**
   ```rust
   // In lumen-vm/src/profiler.rs
   pub struct Profiler {
       samples: Vec<Sample>,
       sampling_rate: Duration,
   }

   struct Sample {
       timestamp: Instant,
       instruction_ptr: usize,
       cell_name: String,
       call_stack: Vec<Frame>,
   }
   ```

2. **Flamegraph Generation**
   - Use existing Rust crates: `inferno`, `flamegraph`
   - Output format: Brendan Gregg's flamegraph SVG
   - Integration with Linux `perf`

3. **Benchmark Framework**
   ```lumen
   cell bench_map_performance()
     let data = generate_large_list()
     let start = timestamp()
     let result = map(data, fn(x) => x * 2 end)
     let elapsed = timestamp() - start
     print("Elapsed: ${elapsed}ms")
   end
   ```

**Tools to Integrate:**
- Linux `perf` - System-level profiling
- Brendan Gregg's flamegraphs: https://www.brendangregg.com/FlameGraphs/cpuflamegraphs.html
- Rust's `cargo-flamegraph` as reference

**References:**
- [Profiling with perf and flamegraphs](https://www.percona.com/blog/profiling-software-using-perf-and-flame-graphs/)
- [LLVM profiling tutorial](https://www.cs.cornell.edu/courses/cs6120/2019fa/blog/llvm-profiling/)
- [Flamegraph crate docs](https://docs.rs/flamegraph/)

---

### 8. Documentation Tools

**Current:** ✅ `lumen doc` command, ⚠️ Basic output
**Industry Standard:** rustdoc (gold standard), typedoc, godoc

#### Current Capabilities:
- Extract doc comments from source
- Generate HTML output

#### Gaps:

**HIGH (P1):**
- ❌ **Doc comment standards** - Formalize syntax
  - Example:
    ```lumen
    /// Fetches a user by ID from the database.
    ///
    /// # Parameters
    /// - `user_id` - The unique identifier for the user
    ///
    /// # Returns
    /// A `result[User, String]` containing the user or an error message.
    ///
    /// # Examples
    /// ```lumen
    /// let user = fetch_user(42)
    /// match user
    ///   ok(u) => print("Found: ${u.name}")
    ///   err(e) => print("Error: ${e}")
    /// end
    /// ```
    cell fetch_user(user_id: Int) -> result[User, String] / {db}
      ...
    end
    ```

- ❌ **Executable doc examples** - rustdoc's killer feature
  - Compile and run code blocks in doc comments
  - Fail build if examples break
  - Ensures documentation stays accurate

- ❌ **Search functionality**
  - Fuzzy search across all items
  - Instant results (like rustdoc's search bar)

- ❌ **Cross-references** - Link types, cells, processes
  - `[fetch_user]` → hyperlink to that cell

**MEDIUM (P2):**
- ❌ **Auto-generated docs on package publish**
  - docs.lumen.io (like docs.rs)
  - Triggered by registry publish

- ❌ **Type signatures in output**
  - Show inferred types even without annotations

- ❌ **Source code links** - Click to see implementation

- ❌ **Private item toggle** - Show/hide internal implementation

- ❌ **Dark theme**

**LOW (P3):**
- ❌ **Versioned docs** - Multiple versions of same package
- ❌ **Diagrams** - Mermaid.js for state machines, pipelines
- ❌ **Module-level docs** - Document entire files

**Rustdoc Excellence (Learn From):**

1. **Executable examples** - Tests disguised as documentation
2. **Fuzzy search** - Find items with typos tolerated
3. **Visual hierarchy** - Color-coded by item type (fn=yellow, struct=blue)
4. **Sidebar navigation** - Quick access to all items
5. **Responsive design** - Mobile-friendly

**Implementation Strategy:**
- Extend existing `lumen-cli/src/doc.rs`
- Use Handlebars/Tera templates for HTML generation
- Parse doc comments in compiler (add to AST metadata)
- Generate static site (like mdbook / rustdoc)
- Host on docs.lumen.dev subdomain

**References:**
- [Making great docs with rustdoc](https://www.tangramvision.com/blog/making-great-docs-with-rustdoc)
- [How rustdoc achieves genius design](https://blog.goose.love/posts/rustdoc/)
- [Rust documentation practices](https://andrewodendaal.com/rust-documentation-practices/)

---

### 9. WASM Compilation Target

**Current:** ❌ No WASM support
**Industry Standard:** Most languages compile to WASM (Rust, C++, Go, Kotlin, etc.)

#### Why WASM Matters:

1. **Web playground** - Run Lumen in browser (no server needed)
2. **Serverless edge computing** - Cloudflare Workers, Fastly Compute
3. **Plugin systems** - Sandboxed extensions (WASM Component Model)
4. **Cross-platform** - Run anywhere with WASM runtime (wasmtime, wasmer)

#### What's Missing:

**CRITICAL (P0):**
- ❌ **WASM bytecode backend**
  - Compile LIR → WASM instructions
  - Memory model mapping (Lumen values → WASM linear memory)
  - Stack vs register mapping (WASM is stack-based, Lumen VM is register-based)

- ❌ **Host bindings** - FFI to call JS/browser APIs
  - wasm-bindgen style interface
  - Web APIs: fetch, console.log, DOM manipulation

**HIGH (P1):**
- ❌ **WASI support** - Standard system interface
  - File I/O, networking, env vars
  - Portable across runtimes (wasmtime, wasmer, WasmEdge)

- ❌ **WASM Component Model** (WIT interfaces)
  - Define Lumen APIs in `.wit` files
  - Generate bindings automatically
  - Call Lumen from other languages (Rust, Python, etc.)

**MEDIUM (P2):**
- ❌ **GC integration** - Use WASM GC proposal
  - WASM 3.0 includes struct/array GC types
  - Share GC with host (browser/runtime)

- ❌ **Threads** - SharedArrayBuffer + Web Workers
  - Parallel futures execution

- ❌ **Streaming compilation** - Progressive download

**Implementation Approaches:**

**Option A: Direct LIR → WASM**
- Map register VM opcodes to WASM stack ops
- Challenge: Impedance mismatch (register vs stack)
- Benefit: Full control, optimized output

**Option B: Reuse Existing Backend**
- LIR → LLVM IR → WASM (via Emscripten)
- Challenge: Large runtime dependencies
- Benefit: Leverage mature toolchain

**Option C: Interpret LIR in WASM**
- Compile VM to WASM (`wasm-pack` on `lumen-vm`)
- Run LIR bytecode in WASM-compiled interpreter
- Challenge: Slower than native compilation
- Benefit: Easy to implement (~1 week)

**Recommended: Start with Option C, migrate to Option A**

**Example: Compiling VM to WASM**
```bash
# In lumen-vm/
cargo build --target wasm32-unknown-unknown --release
wasm-bindgen target/wasm32-unknown-unknown/release/lumen_vm.wasm \
  --out-dir pkg --target web
```

**JS API:**
```typescript
import init, { LumenVM } from './pkg/lumen_vm.js';

await init();
const vm = new LumenVM();
const result = vm.execute(`
  cell main() -> Int
    let x = 40 + 2
    x
  end
`);
console.log(result); // 42
```

**WASM Ecosystem in 2025-2026:**
- WASM 3.0: GC types, exception handling, 64-bit address space
- Component Model standardizing (allows polyglot composition)
- WASI 0.2 (async I/O, HTTP server capabilities)
- 30+ languages compile to WASM

**References:**
- [WebAssembly 3.0 release](https://webassembly.org/news/2025-09-17-wasm-3.0/)
- [State of WebAssembly 2025-2026](https://platform.uno/blog/the-state-of-webassembly-2025-2026/)
- [WebAssembly as language ecosystem](https://2ality.com/2025/01/webassembly-language-ecosystem.html)

---

### 10. Incremental Compilation and Caching

**Current:** ❌ Full recompilation every time
**Industry Standard:** Rust (query-based with Salsa), TypeScript (tsc --incremental)

#### Performance Problem:

Large projects recompile slowly because:
1. Parse entire files every time
2. Type-check unchanged code
3. Regenerate bytecode for unmodified cells

**Solution:** Incremental compilation with dependency tracking

#### What's Missing:

**HIGH (P1):**
- ❌ **Query-based architecture**
  - Salsa framework (used by rust-analyzer)
  - Each compiler stage = query
  - Memoize results, reuse if inputs unchanged

- ❌ **Incremental parsing**
  - Cache AST per file
  - Re-parse only changed files

- ❌ **Incremental type-checking**
  - Cache type info per cell/record
  - Re-check only affected items

- ❌ **Build cache**
  - Save LIR bytecode to disk
  - Load cached .lir files if source unchanged

**MEDIUM (P2):**
- ❌ **Dependency tracking**
  - Build graph of cell → dependencies
  - Invalidate only transitive dependents

- ❌ **Persistent cache**
  - `~/.lumen/cache/` directory
  - Content-addressed storage (hash-based)

**Implementation Strategy:**

1. **Adopt Salsa Framework**
   ```rust
   // In lumen-compiler/src/db.rs
   #[salsa::query_group(CompilerDatabase)]
   trait Compiler: salsa::Database {
       fn parse(&self, file: FileId) -> Arc<Program>;
       fn type_check(&self, file: FileId) -> TypeCheckResult;
       fn lower(&self, file: FileId) -> LirModule;
   }
   ```

2. **Try-Mark-Green Algorithm**
   - Query Q depends on inputs I1, I2, ...
   - If all inputs unchanged (green), Q is green (skip recomputation)
   - If any input changed (red), re-execute Q and compare output
   - If output unchanged, Q becomes green (stops propagation)

3. **Durability System**
   - Mark input durability (file, config, toolchain)
   - Low durability: source files (change often)
   - High durability: standard library (change rarely)
   - Skip checking high-durability inputs

**Salsa Features:**
- Automatic dependency tracking
- Cycle detection (handle recursive types)
- Parallel query execution
- Used by rust-analyzer (proven at scale)

**Expected Performance Gain:**
- Initial compile: same speed
- Incremental compile: 10-100x faster
- rust-analyzer demo: 648 MB + 31s saved on rustc codebase

**References:**
- [Salsa framework](https://github.com/salsa-rs/salsa)
- [Incremental compilation in rustc](https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation.html)
- [Durable incrementality blog post](https://rust-analyzer.github.io/blog/2023/07/24/durable-incrementality.html)

---

### 11. Build System Integration

**Current:** ✅ `lumen pkg build` (basic), ❌ No integration with external build systems
**Industry Standard:** Cargo (Rust), Buck2 (Meta), Bazel (Google)

#### What's Missing:

**MEDIUM (P2):**
- ❌ **Build.lumen manifest**
  - Declare build steps, custom tasks
  - Parallel execution graph

- ❌ **Buck2 / Bazel rules**
  - `lumen_binary()`, `lumen_library()` targets
  - Integrate Lumen into polyglot monorepos

- ❌ **Watch mode** - Auto-rebuild on file change
  - `lumen pkg build --watch`

- ❌ **Build reproducibility**
  - Lockfile for dependency versions
  - Hermetic builds (sandbox environment)

**LOW (P3):**
- ❌ **Remote execution** - Distribute builds across machines
- ❌ **Build artifacts** - Publish compiled `.lir` files
- ❌ **Custom build scripts** - Pre/post-build hooks

**Buck2 Overview (2025 state):**
- Successor to Buck1 (Meta's internal build system)
- Written in Rust (vs Buck1 in Java)
- Remote execution first (vs Buck1 added later)
- 2x faster than Buck1 at Meta
- Multi-language support (Python, C++, Rust, Go, etc.)
- Scriptable rules (Starlark language)

**Integration Example:**
```python
# In BUCK file
lumen_binary(
    name = "my_app",
    src = "src/main.lm.md",
    deps = [
        "//lib:http-client",
        "//lib:json",
    ],
)
```

**References:**
- [Buck2 open source release](https://engineering.fb.com/2023/04/06/open-source/buck2-open-source-large-scale-build-system/)
- [Why Buck2?](https://buck2.build/docs/about/why/)

---

### 12. AI-Assisted Development

**Current:** ❌ No AI tooling integration
**Industry Standard:** GitHub Copilot, Claude Code, Cursor, Codeium

#### Opportunity:

Lumen's markdown-literate format is IDEAL for LLM integration:
- Natural language context in prose sections
- Structured code in fenced blocks
- Explicit tool usage with grants (perfect for AI agents)

#### What's Missing:

**HIGH (P1):**
- ❌ **Copilot extension** - Train on Lumen corpus
  - GitHub Copilot supports custom languages via LSP
  - Fine-tune on Lumen examples, docs, stdlib

- ❌ **LSP semantic tokens** - Improve AI context
  - Helps models distinguish variable types

- ❌ **AI-friendly error messages**
  - Structured JSON output (`--format=json`)
  - LLMs can parse and suggest fixes

**MEDIUM (P2):**
- ❌ **Claude Code MCP server**
  - Lumen-specific tools for Claude Code
  - "Run Lumen tests", "Generate Lumen doc", etc.

- ❌ **Codeium integration**
  - Free alternative to Copilot
  - 70+ language support (could add Lumen)

- ❌ **Tree-sitter queries for AI**
  - Syntax-aware code search
  - Better context extraction

**LOW (P3):**
- ❌ **AI-generated tests** - LLM writes `test_*` cells
- ❌ **Natural language → Lumen** - English spec → code

**AI Landscape 2025-2026:**
- GitHub Copilot 4.0: "Agentic Coding" (autonomous PR fixes)
- Model Context Protocol (MCP) integration
- Claude Code: Custom IDE API support
- Codeium: Free, 70+ languages
- GPT-4o Copilot: 30+ languages

**Lumen Advantage:**
- Literate markdown format aligns with LLM training data
- Effect system makes tool usage explicit (AI can reason about grants)
- Deterministic mode (`@deterministic true`) → perfect for AI test generation

**References:**
- [GitHub Copilot 2026](https://github.com/features/copilot)
- [Claude Code vs GitHub Copilot](https://learn.ryzlabs.com/ai-coding-assistants/github-copilot-vs-claude-code-which-ai-coding-assistant-is-right-for-you-in-2026)
- [AI coding tools 2026](https://jellyfish.co/blog/best-ai-coding-tools/)

---

### 13. Code Formatting (Enhancement)

**Current:** ✅ `lumen fmt` exists, ⚠️ May need polish
**Industry Standard:** Prettier (JS), gofmt (Go), rustfmt (Rust), Black (Python)

#### Current State:
- Formatter implemented in `rust/lumen-cli/src/fmt.rs`
- Opinionated style (minimal config)
- `--check` flag for CI

#### Enhancement Opportunities:

**MEDIUM (P2):**
- ❌ **Format-on-save** - Editor integration
  - LSP: `textDocument/formatting` request
  - VS Code: `editor.formatOnSave`

- ❌ **Configuration options** - `lumen.toml` settings
  - Indent size, line width
  - Keep minimal (avoid bikeshedding like Prettier)

- ❌ **Diff-only formatting** - Format only changed lines
  - `lumen fmt --diff` (like `git diff`)

**Formatter Philosophy (gofmt approach):**
- **Zero configuration** - One canonical style
- **Destructive canonicalization** - Completely reformat (don't preserve quirks)
- **Fast** - Must be instant (<100ms)

**Current Best Practices (2025):**
- Prettier: Opinionated, wraps at line length
- gofmt: Zero config, community standard
- Black (Python): "The uncompromising formatter"
- rustfmt: Highly configurable (Lumen should avoid this)

**Recommendation:** Keep Lumen formatter minimal and fast. No config beyond line width.

**References:**
- [Best code formatters 2024](https://debugg.ai/resources/best-code-formatting-tools-2024)
- [Why Prettier is rock solid](https://lobste.rs/s/aevptj/why_is_prettier_rock_solid)

---

## Prioritized Roadmap

### Phase 1: Foundation (3-6 months) - CRITICAL FOR ADOPTION

**Goal:** Bring tooling to "usable for early adopters" level

1. **LSP Enhancement** (P0)
   - [ ] Code actions (quick fixes)
   - [ ] Workspace symbol search
   - [ ] Find all references
   - [ ] Rename refactoring
   - [ ] Inlay hints (types, parameters)
   - [ ] Semantic tokens

2. **Error Message Quality** (P0)
   - [ ] Actionable suggestions
   - [ ] Error codes (E1001 style)
   - [ ] Friendly tone (Elm-inspired)
   - [ ] Multi-span highlighting

3. **WASM Support (Option C)** (P0)
   - [ ] Compile VM to WASM (wasm-pack)
   - [ ] JS bindings (wasm-bindgen)
   - [ ] Basic web playground (Monaco + WASM)

4. **Documentation Enhancement** (P1)
   - [ ] Executable doc examples
   - [ ] Search functionality
   - [ ] Cross-references

### Phase 2: Professional (6-12 months) - COMPETITIVE PARITY

**Goal:** Match expectations of developers from mature ecosystems

5. **Package Registry** (P0)
   - [ ] MVP registry (upload/download)
   - [ ] Web UI for search/discovery
   - [ ] Publishing workflow
   - [ ] CLI: `lumen pkg publish`, `lumen pkg search`

6. **Debugger (DAP)** (P0)
   - [ ] Breakpoints + stepping
   - [ ] Variable inspection
   - [ ] Call stack view
   - [ ] VS Code debug adapter

7. **Multi-Editor Support** (P1)
   - [ ] JetBrains plugin
   - [ ] Neovim config (nvim-lspconfig)
   - [ ] Helix / Zed support
   - [ ] Documentation for setup

8. **Profiling** (P1)
   - [ ] Sample-based profiler
   - [ ] Flamegraph generation
   - [ ] `lumen profile` command

### Phase 3: Excellence (12-24 months) - BEST-IN-CLASS

**Goal:** Set new standards for language tooling

9. **Incremental Compilation** (P1)
   - [ ] Salsa integration
   - [ ] Query-based architecture
   - [ ] Persistent cache

10. **Advanced Debugging** (P1)
    - [ ] Watch expressions
    - [ ] Conditional breakpoints
    - [ ] REPL in debug context
    - [ ] Hot reload

11. **AI Integration** (P1)
    - [ ] GitHub Copilot training
    - [ ] Claude Code MCP server
    - [ ] AI-friendly error JSON

12. **Docs Hosting** (P2)
    - [ ] docs.lumen.io auto-generation
    - [ ] Versioned docs
    - [ ] Source links

### Phase 4: Future Enhancements (24+ months) - DIFFERENTIATION

**Goal:** Unique features that set Lumen apart

13. **Wasm Compilation Target (Option A)** (P2)
    - [ ] Direct LIR → WASM backend
    - [ ] Optimize for size and speed
    - [ ] WASI + Component Model

14. **Build System Integration** (P2)
    - [ ] Buck2 / Bazel rules
    - [ ] Monorepo support

15. **Advanced Playground** (P3)
    - [ ] Multiplayer editing
    - [ ] Embedded in docs
    - [ ] Mobile support

---

## Resource Allocation Estimates

### Engineering Effort (Full-Time Equivalents)

**Phase 1 (3-6 months):**
- LSP Enhancement: 1 FTE × 3 months
- Error Messages: 0.5 FTE × 2 months
- WASM Support: 0.5 FTE × 1 month
- Docs: 0.5 FTE × 2 months
- **Total: 2.5 FTE-months**

**Phase 2 (6-12 months):**
- Package Registry: 1 FTE × 4 months
- Debugger: 1 FTE × 3 months
- Multi-Editor: 0.5 FTE × 2 months
- Profiling: 0.5 FTE × 2 months
- **Total: 11 FTE-months**

**Phase 3 (12-24 months):**
- Incremental Compilation: 1 FTE × 4 months
- Advanced Debugging: 0.5 FTE × 2 months
- AI Integration: 0.5 FTE × 2 months
- Docs Hosting: 0.5 FTE × 2 months
- **Total: 10 FTE-months**

### Infrastructure Costs (Annual)

- **Package Registry Hosting:** $500-2000/year
  - Domain: $20/year
  - Server (Hetzner/DigitalOcean): $40/month × 12 = $480
  - CDN (Cloudflare R2): $100/year
  - PostgreSQL: Included in server

- **Docs Hosting:** $200/year
  - Static site (Netlify free tier or $5/month)

- **Playground Hosting:** $500/year
  - Serverless (Cloudflare Workers free tier)
  - Database (snippet storage): $5/month × 12 = $60

**Total Infrastructure: $1200-2700/year**

---

## Success Metrics

### Adoption Indicators

1. **LSP Usage**
   - Active users (telemetry opt-in)
   - Code actions invoked per session
   - Completion acceptance rate

2. **Package Registry**
   - Packages published (target: 50 in Year 1)
   - Daily downloads (target: 1000/day by Month 12)
   - Active publishers (target: 20 contributors)

3. **Playground**
   - Monthly active users (target: 1000 by Month 6)
   - Snippets created (target: 500/month)
   - Share link clicks

4. **Documentation**
   - Search queries per day
   - Doc example test pass rate (should be 100%)
   - Time spent on docs pages

5. **Community Growth**
   - GitHub stars (target: 1000 by Year 1)
   - Discord/forum members
   - Tutorial completions

---

## Competitive Analysis Summary

### How Lumen Compares (Current State)

| Feature | Lumen | Rust | TypeScript | Go | Elm |
|---------|-------|------|------------|-----|-----|
| LSP (basic) | ✅ | ✅ | ✅ | ✅ | ✅ |
| LSP (advanced) | ❌ | ✅✅ | ✅✅ | ✅ | ✅ |
| Debugger | ❌ | ✅ | ✅ | ✅ | ⚠️ |
| Package registry | ❌ | ✅ crates.io | ✅ npm | ✅ pkg.go.dev | ✅ |
| Playground | ❌ | ✅ | ✅ | ✅ | ✅ |
| Formatter | ✅ | ✅ | ✅ | ✅ | ✅ |
| Error messages | ⚠️ | ✅✅ | ⚠️ | ⚠️ | ✅✅ |
| Docs generator | ⚠️ | ✅✅ rustdoc | ✅ typedoc | ✅ | ⚠️ |
| Profiler | ❌ | ✅✅ | ✅ | ✅ pprof | ❌ |
| WASM target | ❌ | ✅ | ✅ | ✅ | ✅ |
| Multi-editor | ⚠️ | ✅ | ✅ | ✅ | ⚠️ |
| AI integration | ❌ | ✅ | ✅ | ✅ | ❌ |
| Incremental build | ❌ | ✅ | ✅ | ✅ | ⚠️ |

**Legend:** ✅✅ = Best-in-class, ✅ = Good, ⚠️ = Partial, ❌ = Missing

### Lumen's Unique Advantages (To Leverage)

1. **Literate Markdown Format**
   - Natural language + code → perfect for AI
   - Documentation is first-class (not comments)

2. **Effect System**
   - Explicit tool grants → provenance tracking
   - Great for AI agents (LLMs know what's allowed)

3. **Process Model**
   - State machines, pipelines, memory → built-in abstractions
   - Other languages require frameworks

4. **Deterministic Mode**
   - `@deterministic true` → reproducible execution
   - Perfect for testing, CI, AI training

**Strategic Recommendation:** Double down on AI integration. Lumen's design is uniquely suited for LLM-assisted development.

---

## References & Further Reading

### LSP & Language Servers
- [Language Server Protocol 3.17 Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/)
- [rust-analyzer 2025 releases](https://github.com/rust-lang/rust-analyzer/releases)
- [Rust in Visual Studio Code](https://code.visualstudio.com/docs/languages/rust)

### Debugging
- [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol/)
- [nvim-dap: DAP client for Neovim](https://github.com/mfussenegger/nvim-dap)

### Package Registries
- [crates.io: Rust Package Registry](https://crates.io/)
- [crates.io source code](https://github.com/rust-lang/crates.io)
- [How to build a package registry](https://blog.packagecloud.io/how-do-i-build-a-package-registry/)

### WebAssembly
- [WebAssembly 3.0 Announcement](https://webassembly.org/news/2025-09-17-wasm-3.0/)
- [State of WebAssembly 2025-2026](https://platform.uno/blog/the-state-of-webassembly-2025-2026/)
- [Awesome WASM Languages](https://wasmlang.org/)

### Error Messages
- [Comparing Compiler Errors Across Languages](https://www.amazingcto.com/developer-productivity-compiler-errors/)
- [Writing Good Compiler Error Messages](https://calebmer.com/2019/07/01/writing-good-compiler-error-messages.html)
- [Elm Error Message Style](https://discourse.elm-lang.org/t/error-messages-style/7828)

### Profiling
- [CPU Flame Graphs by Brendan Gregg](https://www.brendangregg.com/FlameGraphs/cpuflamegraphs.html)
- [Profiling with perf and flamegraphs](https://www.percona.com/blog/profiling-software-using-perf-and-flame-graphs/)
- [LLVM Runtime Profiling](https://www.cs.cornell.edu/courses/cs6120/2019fa/blog/llvm-profiling/)

### Documentation
- [Making Great Docs with Rustdoc](https://www.tangramvision.com/blog/making-great-docs-with-rustdoc)
- [How rustdoc achieves genius design](https://blog.goose.love/posts/rustdoc/)
- [Rust Documentation Practices](https://andrewodendaal.com/rust-documentation-practices/)

### Incremental Compilation
- [Salsa Framework](https://github.com/salsa-rs/salsa)
- [Incremental Compilation in rustc](https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation.html)
- [Durable Incrementality](https://rust-analyzer.github.io/blog/2023/07/24/durable-incrementality.html)

### Build Systems
- [Buck2: Open Source Build System](https://engineering.fb.com/2023/04/06/open-source/buck2-open-source-large-scale-build-system/)
- [Why Buck2?](https://buck2.build/docs/about/why/)

### AI-Assisted Development
- [GitHub Copilot](https://github.com/features/copilot)
- [GitHub Copilot vs Claude Code](https://learn.ryzlabs.com/ai-coding-assistants/github-copilot-vs-claude-code-which-ai-coding-assistant-is-right-for-you-in-2026)
- [Top AI Coding Tools 2026](https://jellyfish.co/blog/best-ai-coding-tools/)

### Editor Support
- [Neovim + LazyVim vs VS Code comparison](https://www.tuliocunha.dev/blog/neovim-lazyvim-vs-vscode-jetbrains-zed-helix-2025/)
- [Best IDEs for Rust 2025](https://www.analyticsinsight.net/programming/best-ides-for-rust-development-in-2025/)

---

## Conclusion

Lumen has a **solid foundation** with its LSP server, VS Code extension, CLI tools, and tree-sitter grammar. However, to achieve mainstream adoption, we must close critical gaps in:

1. **LSP capabilities** (code actions, inlay hints, semantic tokens)
2. **Debugging support** (DAP implementation)
3. **Package registry** (public registry infrastructure)
4. **Web playground** (WASM compilation target)
5. **Error message quality** (Elm/Rust-level clarity)

The **recommended approach** is a phased rollout:
- **Phase 1 (3-6 months)**: LSP enhancement, error messages, WASM VM, docs
- **Phase 2 (6-12 months)**: Package registry, debugger, multi-editor, profiler
- **Phase 3 (12-24 months)**: Incremental compilation, advanced debugging, AI integration

Total estimated effort: **23.5 FTE-months** over 2 years
Infrastructure cost: **$1200-2700/year**

**Strategic Advantage:** Lumen's literate markdown format and effect system are uniquely suited for AI-assisted development. Prioritizing AI integration (Copilot training, Claude Code MCP server) could differentiate Lumen in a crowded language market.

**Next Steps:**
1. Review and prioritize this roadmap with stakeholders
2. Allocate engineering resources for Phase 1 work
3. Set up infrastructure (domain registration, cloud accounts)
4. Begin LSP enhancement and error message improvements
5. Engage community for early feedback on tooling priorities
