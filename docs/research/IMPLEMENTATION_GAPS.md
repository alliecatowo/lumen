# Implementation Gaps

Date: February 15, 2026

Remaining gaps in the Lumen implementation, organized by priority.

---

## HIGH PRIORITY

| Gap | Description | Notes |
|-----|-------------|-------|
| **Real Ed25519/Sigstore signing** | Replace cryptographic stubs with actual signing | Package provenance and trust-check depend on this |
| **Registry API deployment** | Deploy registry service for publish/search/install | Cloudflare Workers scaffold exists; full round-trip pending |
| **Standard library** | Basic collections, string utils, math | Intrinsics exist; cohesive stdlib module and docs needed |
| **Error recovery in parser** | Partial recovery for some constructs | Recovery APIs exist; CLI/LSP hot path still mostly fail-fast |

---

## MEDIUM PRIORITY

| Gap | Description | Notes |
|-----|-------------|-------|
| **WASM multi-file imports** | Import resolution in WASM build | Single-file only today |
| **WASM tool providers** | Tool dispatch in WASM targets | Phase 3 in WASM roadmap |
| **Performance benchmarks** | Baseline and regression suite | No formal benchmark harness yet |
| **Debugger support in VM** | Breakpoints, stepping, value inspection | Trace events exist; no interactive debugger |
| **Code actions in LSP** | Rename, extract, quick fixes | Hover, completion, diagnostics present; code actions missing |

---

## LOW PRIORITY

| Gap | Description | Notes |
|-----|-------------|-------|
| **Gradual ownership system** | `ref T`, `mut ref T` for controlled mutation | Planned language extension |
| **Self-hosting compiler** | Compiler written in Lumen | Long-term goal |
| **Profile-guided optimization** | PGO for VM hot paths | No profiling infrastructure yet |
| **REPL improvements** | Multi-line, history search | Basic REPL works; UX enhancements pending |

---

## Cross-References

- **EXECUTION_TRACKER.md** — Completion status and parity goals
- **COMPETITIVE_ANALYSIS.md** — Comparison with other languages
- **PACKAGE_REGISTRY.md** — Registry protocol and lockfile design
- **WASM_STRATEGY.md** — WASM architecture and roadmap
