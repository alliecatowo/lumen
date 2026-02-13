# WASM Compilation Strategy

This document outlines Lumen's strategy for WebAssembly (WASM) compilation support, including architecture options, recommended approach, and implementation roadmap.

## Executive Summary

**Recommended Approach**: Compile the Lumen VM itself to WASM using Rust's mature wasm-pack toolchain. This provides the fastest path to WASM support while enabling both browser and server (WASI) deployment scenarios.

## Market Context (2025)

### Key Trends

1. **Edge AI Inference**: AI workloads are moving to the edge, with WASM enabling in-browser ML inference. IDC predicts 80% of CIOs will use edge services for AI inference by 2027, with cost savings up to 86% compared to cloud-only approaches.

2. **WASI Evolution**: WASI 0.2 (Preview 2) shipped with the Component Model, defining composable interfaces. WASI 0.3 (expected H1 2025) will add native async I/O support via the Component Model.

3. **Language Ecosystem Growth**: WebAssembly 3.0 (Sept 2025) added 64-bit address spaces, exception handling, and GC types. Languages like MoonBit and Grain are built specifically for WASM, demonstrating viability of WASM-native languages.

4. **Adoption**: WASM usage in Chrome reached 5.5% of websites visited in 2025, growing 1% year-over-year.

## Architecture Options

### Option 1: Compile VM to WASM (Recommended)

**Description**: Compile the entire Lumen VM (Rust codebase) to WASM using wasm-pack, exposing `compile()` and `run()` functions via wasm-bindgen.

**Advantages**:
- Reuses 100% of existing VM code (lumen-vm, lumen-compiler, lumen-runtime)
- Leverages Rust's mature WASM tooling (wasm-pack, wasm-bindgen)
- Supports both browser (via wasm-bindgen) and server (via WASI) deployment
- Minimal additional code to maintain
- Fastest time-to-market (weeks, not months)
- Proven approach (many Rust VMs compile to WASM successfully)

**Disadvantages**:
- Slightly larger binary size vs native WASM codegen
- Carries full VM overhead (though negligible for AI workloads)
- Limited control over WASM output structure

**Browser Deployment**: Use wasm-bindgen to expose JavaScript API. WASM module loaded via `<script type="module">`, runs in main thread or Web Worker.

**Server Deployment**: Compile with WASI target (`wasm32-wasi`), run in Wasmtime/WasmEdge/Wasmer. Full access to WASI 0.2 syscalls (filesystem, networking, clocks).

### Option 2: Native WASM Codegen

**Description**: Write a new LIR → WASM bytecode compiler, generating `.wasm` modules directly from Lumen code.

**Advantages**:
- Smaller binary size (only generated code, no VM)
- Direct control over WASM structure
- Potential performance optimizations (tail calls, SIMD)

**Disadvantages**:
- Large engineering effort (3-6 months minimum)
- Requires implementing WASM binary format encoding
- Requires runtime library for Lumen semantics (closures, records, futures, tool dispatch)
- Complex debugging (need source maps, DWARF support)
- Must support two compilation backends (VM + WASM)

**Verdict**: Not recommended for initial release. Consider for v2 if binary size becomes critical.

### Option 3: Hybrid Approach

**Description**: Use Option 1 for initial release, migrate high-value use cases to Option 2 incrementally.

**Advantages**:
- Get to market fast with Option 1
- Optimize specific scenarios with Option 2 later

**Disadvantages**:
- Maintain two compilation paths long-term
- Complexity of ensuring semantic equivalence

## Recommended Approach: VM-to-WASM

### Rationale

1. **Time-to-Market**: Lumen is an AI-native language. Edge AI inference is a massive 2025-2026 trend. Getting WASM support shipped quickly unlocks this market.

2. **Proven Pattern**: MoonBit, AssemblyScript, and Grain all demonstrate successful WASM-native language implementations. However, they were designed for WASM from day one. For existing VM-based languages, compiling the VM is standard (e.g., Lua, Python, Ruby via Pyodide/ruby.wasm).

3. **Tool Dispatch Advantage**: Lumen's tool dispatch architecture (ToolProvider trait, ProviderRegistry) maps cleanly to WASM imports. Tools become WASM imports, invoked via WebAssembly interface.

4. **WASI Alignment**: Lumen's capability-based grant system aligns perfectly with WASI's capability-based security model.

### Technical Architecture

```
┌─────────────────────────────────────────────┐
│         JavaScript / WASM Host              │
│  - Browser (wasm-bindgen JS glue)           │
│  - Node.js (WASI bindings)                  │
│  - Wasmtime/WasmEdge (native WASI)          │
└─────────────────────────────────────────────┘
                     ↓
┌─────────────────────────────────────────────┐
│      lumen-wasm.wasm (Rust → WASM)          │
│                                             │
│  ┌───────────────────────────────────────┐  │
│  │  wasm-bindgen API                     │  │
│  │  - compile(source: &str) -> String    │  │
│  │  - run(source: &str) -> String        │  │
│  │  - check(source: &str) -> String      │  │
│  └───────────────────────────────────────┘  │
│                     ↓                       │
│  ┌───────────────────────────────────────┐  │
│  │  lumen-compiler                       │  │
│  │  - Parser, Resolver, Typechecker      │  │
│  │  - LIR lowering                       │  │
│  └───────────────────────────────────────┘  │
│                     ↓                       │
│  ┌───────────────────────────────────────┐  │
│  │  lumen-vm                             │  │
│  │  - Register VM execution              │  │
│  │  - Value system, closures, futures    │  │
│  └───────────────────────────────────────┘  │
│                     ↓                       │
│  ┌───────────────────────────────────────┐  │
│  │  lumen-runtime                        │  │
│  │  - ToolProvider trait                 │  │
│  │  - ProviderRegistry                   │  │
│  └───────────────────────────────────────┘  │
│                     ↓                       │
│       WASM imports (tool dispatch)          │
└─────────────────────────────────────────────┘
                     ↓
┌─────────────────────────────────────────────┐
│       Host-Provided Tool Implementations    │
│  - Browser: fetch(), localStorage, etc.     │
│  - WASI: HTTP, filesystem, etc.             │
└─────────────────────────────────────────────┘
```

### Tool Dispatch in WASM

**Browser**: Tools map to JavaScript functions via wasm-bindgen. `use tool fetch` becomes a WASM import that calls JavaScript `fetch()`.

**WASI**: Tools map to WASI interfaces. `use tool http_get` imports from `wasi:http/outgoing-handler`.

**Configuration**: Host provides `ProviderRegistry` before calling `run()`. WASM module calls back into host for tool execution.

## Implementation Roadmap

### Phase 1: Foundation (Week 1-2)

**Goal**: Prove VM compiles to WASM and runs simple programs.

**Deliverables**:
1. Create `rust/lumen-wasm/` crate
   - Cargo.toml with wasm-bindgen, wasm-pack dependencies
   - crate-type = ["cdylib", "rlib"]
2. Implement `src/lib.rs`:
   ```rust
   use wasm_bindgen::prelude::*;

   #[wasm_bindgen]
   pub fn compile(source: &str) -> String { ... }

   #[wasm_bindgen]
   pub fn run(source: &str) -> String { ... }

   #[wasm_bindgen]
   pub fn check(source: &str) -> String { ... }
   ```
3. Add build script: `wasm-pack build --target web`
4. Create `examples/wasm_hello.html` demonstrating browser usage
5. Document browser deployment in `docs/WASM_DEPLOYMENT.md`

**Blockers**: None (wasm-bindgen supports all Rust types Lumen uses).

### Phase 2: CLI Integration (Week 3)

**Goal**: Make WASM builds part of standard workflow.

**Deliverables**:
1. Add `lumen build --target wasm` command to lumen-cli
   - Invokes wasm-pack if available
   - Outputs pkg/ directory with .wasm + .js glue
2. Add `lumen-wasm` to workspace (behind feature flag to avoid breaking CI)
3. Document build process in CLAUDE.md

**Blockers**: None.

### Phase 3: Tool Provider Bridge (Week 4-5)

**Goal**: Enable Lumen tool dispatch to work across WASM boundary.

**Deliverables**:
1. Design WASM import interface for ToolProvider trait:
   ```rust
   #[wasm_bindgen]
   extern "C" {
       fn wasm_call_tool(
           alias: &str,
           args_json: &str
       ) -> String;
   }
   ```
2. Implement WasmToolProvider that forwards to host
3. Create example with `use tool fetch` working in browser
4. Document tool provider contract for WASM hosts

**Blockers**: May require refactoring ToolProvider trait for serialization.

### Phase 4: WASI Support (Week 6-7)

**Goal**: Run Lumen programs in WASI environments (Wasmtime, Node.js).

**Deliverables**:
1. Add wasm32-wasi target to build
2. Implement WASI tool providers (filesystem, HTTP via wasi:http)
3. Test in Wasmtime and Node.js (via @bytecodealliance/jco)
4. Create examples/wasm_wasi_http.lm.md
5. Document WASI deployment

**Blockers**: WASI 0.3 async support not finalized (use sync interfaces for now).

### Phase 5: Optimization & Polish (Week 8+)

**Goal**: Production-ready WASM builds.

**Deliverables**:
1. Enable wasm-opt in release builds (--opt-level z)
2. Benchmark WASM vs native performance
3. Add source maps for debugging
4. NPM package for easy browser/Node.js consumption
5. CDN hosting for quick demos (jsDelivr/unpkg)

**Blockers**: None.

## Dependencies

### Build-time
- wasm-pack (CLI tool, installed via cargo install wasm-pack)
- wasm-bindgen (Rust crate, dependency in Cargo.toml)
- wasm-opt (part of Binaryen, optional but recommended)

### Runtime (Browser)
- Modern browser with WASM support (Chrome 88+, Firefox 89+, Safari 15+)

### Runtime (WASI)
- Wasmtime 18+ (WASI 0.2 support)
- WasmEdge 0.13+ (WASI 0.2 support)
- Node.js 20+ with @bytecodealliance/jco

## Browser vs Server Considerations

### Browser

**Advantages**:
- Zero server cost for AI inference (runs client-side)
- Sub-100ms latency (no network round-trip)
- Privacy-preserving (data never leaves device)
- Perfect for Lumen's use case (AI agents in web apps)

**Limitations**:
- ~4GB WASM memory limit (32-bit address space until wider browser support for WASM64)
- No filesystem access (use IndexedDB for persistence)
- CORS restrictions on HTTP tools
- Security: must sandbox untrusted code via CSP

**Use Cases**:
- AI-powered chatbots/assistants in web apps
- Client-side data processing pipelines
- Browser-based development environments
- Privacy-sensitive AI applications

### Server (WASI)

**Advantages**:
- Full syscall access (filesystem, networking, threads)
- No memory limits (WASM64 supported in Wasmtime)
- Sandboxed execution (WASI capabilities model)
- Fast startup (WASM instantiation ~1-5ms)

**Limitations**:
- WASI 0.2 still maturing (async support coming in 0.3)
- Fewer host runtimes than browser WASM
- Tool ecosystem smaller than native

**Use Cases**:
- Edge functions (Fastly Compute@Edge, Cloudflare Workers)
- Serverless AI inference
- Plugin systems (extend apps with untrusted code)
- Multi-tenant compute (isolate customer workloads)

## Alternative Approaches Considered

### LLVM Backend
Compile LIR → LLVM IR → WASM using Rust's LLVM infrastructure.

**Rejected**: Requires learning LLVM API, similar effort to native WASM codegen but less control over output. VM-to-WASM is simpler.

### Binaryen API
Use Binaryen's C++ API to construct WASM from LIR.

**Rejected**: Requires C++ FFI, more complex than native WASM encoding. If we're writing WASM directly, better to use Rust libraries (wasm-encoder).

### Transpile to AssemblyScript
Generate AssemblyScript (TypeScript-like syntax) and compile via AssemblyScript compiler.

**Rejected**: Adds another compilation layer, slower builds, less control. AssemblyScript semantics may not match Lumen's.

## Success Metrics

**Phase 1 Success**:
- [ ] VM compiles to WASM without errors
- [ ] Simple "Hello, world!" runs in browser
- [ ] Binary size < 2MB (optimized)

**Phase 3 Success**:
- [ ] Tool dispatch works across WASM boundary
- [ ] Example with `use tool fetch` runs in browser
- [ ] Error messages propagate correctly

**Phase 4 Success**:
- [ ] Same .lm.md runs in browser AND Wasmtime
- [ ] WASI tools (filesystem, HTTP) functional

**Production Ready**:
- [ ] NPM package published
- [ ] Documentation complete
- [ ] 3+ real-world examples
- [ ] Performance within 2x of native (acceptable for WASM)

## References

### WebAssembly Ecosystem
- [The State of WebAssembly – 2025 and 2026](https://platform.uno/blog/the-state-of-webassembly-2025-2026/)
- [WebAssembly as an ecosystem for programming languages](https://2ality.com/2025/01/webassembly-language-ecosystem.html)
- [WebAssembly Language Support Matrix](https://developer.fermyon.com/wasm-languages/webassembly-language-support)

### Rust WASM Tooling
- [Compiling from Rust to WebAssembly - MDN](https://developer.mozilla.org/en-US/docs/WebAssembly/Guides/Rust_to_Wasm)
- [The Minimal Rust-Wasm Setup for 2024](https://dzfrias.dev/blog/rust-wasm-minimal-setup/)
- [Rust and WebAssembly Book](https://rustwasm.github.io/docs/book/print.html)

### WASI and Component Model
- [WASI and the WebAssembly Component Model: Current Status](https://eunomia.dev/blog/2025/02/16/wasi-and-the-webassembly-component-model-current-status/)
- [WASI Preview 2 and the Component Model](https://blog.whoisjsonapi.com/exploring-wasi-and-the-component-model/)
- [Looking Ahead to WASIp3](https://www.fermyon.com/blog/looking-ahead-to-wasip3)

### AI Inference at the Edge
- [Running AI Workloads with WebAssembly](https://www.fermyon.com/blog/ai-workloads-panel-discussion-wasm-io-2024)
- [Edge AI: The future of AI inference](https://www.infoworld.com/article/4117620/edge-ai-the-future-of-ai-inference-is-smarter-local-compute.html)
- [Edge vs. cloud TCO for AI inference](https://www.cio.com/article/4109609/edge-vs-cloud-tco-the-strategic-tipping-point-for-ai-inference.html)

### WASM-Native Languages
- [MoonBit: Wasm-Optimized Language](https://thenewstack.io/moonbit-wasm-optimized-language-creates-less-code-than-rust/)
- [MoonBit Official Site](https://www.moonbitlang.com/)
- [Grain Language](https://grain-lang.org/)

## Conclusion

Compiling the Lumen VM to WASM via wasm-pack is the clear winner for initial WASM support. It leverages existing code, proven tooling, and positions Lumen for the 2025-2026 edge AI inference boom. Native WASM codegen can be explored in future versions if binary size or performance become critical bottlenecks.

**Estimated Timeline**: 8 weeks from zero to production-ready WASM support.

**Key Risk**: None identified. Rust WASM tooling is mature and Lumen's architecture (no platform-specific dependencies) makes it WASM-friendly.
