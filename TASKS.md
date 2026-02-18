# Lumen Implementation Tasks — Production Hardening & Performance

## Phase 7: Cranelift JIT World-Class (Priority: Critical)
- [ ] P701: Audit JIT opcode coverage — identify missing opcodes in `is_cell_jit_compilable`
- [ ] P702: Implement missing JIT opcodes: `Intrinsic`, `GetField`, `SetField`, `GetIndex`, `SetIndex`, `NewList`, `NewRecord`
- [ ] P703: Expand JIT arity limit from 3 → N arguments
- [ ] P704: Implement OSR (On-Stack Replacement) for hot loops
- [ ] P705: Add speculative type guards and deoptimization
- [ ] P706: Profile-Guided Optimization (PGO) infrastructure
- [ ] P707: JIT inline caching for property access

## Phase 8: WASM World-Class (Priority: High)
- [ ] P801: Implement WASM control flow: `block`, `loop`, `br`, `br_if`, `return`
- [ ] P802: Add WASM linear memory and string support
- [ ] P803: Implement WASM exception handling
- [ ] P804: WASM SIMD support for numerical operations
- [ ] P805: WASM bulk memory operations
- [ ] P806: Cranelift WASM backend integration (replace hand-rolled)

## Phase 9: Performance & Benchmarks (Priority: High)
- [ ] P901: Fix string concat benchmark — O(n²) → O(n) optimization
- [ ] P902: Run all benchmarks and identify bottlenecks
- [ ] P903: Implement escape analysis for stack allocation
- [ ] P904: Optimize interpreter dispatch (direct threading or computed goto)
- [ ] P905: Constant pool optimization — inline small constants
- [ ] P906: Register allocator improvements

## Phase 10: Codebase Hardening (Priority: Medium)
- [ ] P1001: Audit all `todo!()`, `unimplemented!()`, `panic!()` — replace with proper error handling
- [ ] P1002: Identify stub implementations and implement real functionality
- [ ] P1003: Improve error messages with source location context
- [ ] P1004: Add comprehensive logging/tracing throughout pipeline
- [ ] P1005: Security audit — input validation, bounds checking
- [ ] P1006: Documentation completeness audit

## Phase 11: CI/CD & Tooling (Priority: Medium)
- [ ] P1101: Ensure all GitHub Actions workflows pass
- [ ] P1102: Add benchmark regression testing to CI
- [ ] P1103: Improve `run_all.sh` script performance and reliability
- [ ] P1104: Add code coverage reporting
- [ ] P1105: Implement DAP (Debug Adapter Protocol)
- [ ] P1106: Fix LSP markdown comment handling
