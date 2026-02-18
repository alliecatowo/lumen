# Lumen Runtime Crate

This crate contains the VM, scheduler, process runtimes, and all runtime services.

## Quick Reference
- **VM dispatch**: `src/vm/mod.rs` → `run_until()` hot loop
- **Intrinsics**: `src/vm/intrinsics.rs` → 80+ builtins
- **Test command**: `cargo test -p lumen-rt`

## Critical Rules
- Signed jumps: use `sax()`/`sax_val()`, NEVER `ax`/`ax_val`
- Set uses `BTreeSet<Value>` (O(log n)), NOT `Vec<Value>`
- Collections are `Arc<T>`-wrapped with COW via `Arc::make_mut()`
- Call-frame stack max depth: 256

## Module Map
| Path | Purpose |
|------|---------|
| `vm/mod.rs` | Core VM dispatch loop |
| `vm/intrinsics.rs` | Builtin function dispatch |
| `vm/ops.rs` | Arithmetic (Int/Float fast path, BigInt fallback) |
| `vm/helpers.rs` | VM utility functions |
| `vm/processes.rs` | MemoryRuntime, MachineRuntime |
| `vm/continuations.rs` | Multi-shot delimited continuations |
| `services/tools.rs` | Tool dispatch, ProviderRegistry |
| `services/scheduler.rs` | M:N work-stealing scheduler |
| `services/process.rs` | Actor model (PCB, mailbox, priority) |
| `services/trace/` | Structured tracing |
| `services/execution_graph.rs` | DAG visualization (DOT/Mermaid) |
| `services/schema_drift.rs` | API shape change detection |
| `services/crypto.rs` | SHA-256, BLAKE3, HMAC, HKDF, Ed25519 |
| `services/retry.rs` | Exponential/Fibonacci backoff |
| `services/http.rs` | RequestBuilder, Router |
| `services/fs_async.rs` | Async file operations |
| `services/net.rs` | TCP/UDP, DNS |
| `jit_tier.rs` | JIT tiering integration |
