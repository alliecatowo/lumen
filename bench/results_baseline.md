# Lumen Benchmark Baseline Results

Generated: 2026-02-19
Build: `cargo build --release` (lumen-cli)
Platform: Linux x86_64 (Fedora 43, kernel 6.18)
JIT: Tier 2 (Cranelift) enabled, threshold=1

---

## Cross-Language Benchmarks

### Fibonacci (N=35)

| Language | Time (s) | Status |
|----------|----------|--------|
| C (-O2)  | 0.02     | PASS |
| Python 3 | 1.48     | PASS |
| Lumen    | 0.21     | PASS (JIT: 1 cell compiled) |

**Notes**: Lumen is ~10.5x slower than C, ~7x faster than Python.

---

### Tree Traversal (recursive, depth=18, ~262144 nodes)

| Language | Time (s) | Status |
|----------|----------|--------|
| C (-O2)  | 0.03     | PASS |
| Python 3 | 0.74     | PASS |
| Lumen    | 1.21     | PASS (interpreter — JIT OSR not yet firing) |

**Notes**: Lumen is slower than Python on tree traversal. The JIT is not helping because OSR is not firing on the recursive tree traversal pattern. This is a known gap — recursive cells need better OSR detection.

---

### String Operations

| Language | Time (s) | Status |
|----------|----------|--------|
| C (-O2)  | ~0.00    | PASS |
| Python 3 | 0.02     | PASS |
| Lumen    | ~0.00    | PASS (JIT: 1 cell compiled) |

**Notes**: Lumen is competitive with C on string operations, likely because the workload is dominated by allocator overhead and both use the same underlying system allocator.

---

### Sort (N=1,000,000)

| Language | Time (s) | Status |
|----------|----------|--------|
| Lumen    | N/A      | FAIL (JIT lowering error: unhandled opcode `Append`) |

**Failure**: The sort benchmark uses `Append` to build the array, which is not yet implemented in the JIT lowering. Falls back to interpreted `Append` with OSR compilation failing.

---

### N-Body (steps=1,000,000)

| Language | Time (s) | Status |
|----------|----------|--------|
| Lumen    | N/A      | FAIL (JIT verifier error in `advance`) |

**Failure**: JIT Cranelift verifier rejects the `advance` cell. Root cause: likely a phi-node or SSA form issue in the Cranelift IR generation for the n-body advance loop.

---

### Primes Sieve (N=1,000,000)

| Language | Time (s) | Status |
|----------|----------|--------|
| Lumen    | N/A      | TIMEOUT (interpreter too slow, JIT verifier error) |

**Failure**: JIT verifier fails for `is_prime`. Without JIT, interpreter is too slow to complete in a reasonable time.

---

### Matrix Multiply (N=200)

| Language | Time (s) | Status |
|----------|----------|--------|
| Lumen    | N/A      | FAIL (JIT lowering error: unhandled opcode `Append`) |

**Failure**: Same as Sort — `Append` opcode not implemented in JIT lowering.

---

### JSON Parse

| Language | Time (s) | Status |
|----------|----------|--------|
| Lumen    | N/A      | FAIL (JIT verifier error in `build_chunk`) |

**Failure**: Cranelift verifier error in `build_chunk`. Likely same SSA/phi-node issue as nbody.

---

### Fannkuch (N=10)

| Language | Time (s) | Status |
|----------|----------|--------|
| Lumen    | N/A      | TIMEOUT |

**Failure**: Interpreter too slow for this compute-intensive benchmark. JIT status unknown (test timed out before JIT kicked in or JIT compilation also failed).

---

## Lumen-Specific Benchmarks

| Benchmark | Time (s) | JIT Status |
|-----------|----------|------------|
| b_int_fib | 0.12 | JIT: 1 cell, 1 native call |
| b_string_concat | 0.01 | JIT: 1 cell, 1 native call |
| b_float_mandelbrot | 0.11 | JIT: 1 cell, 40000 native calls |
| b_ackermann | 2.37 | Interpreter (deep recursion, JIT not triggered) |
| b_call_overhead | 5.84 | JIT: 1 cell, 10M native calls |
| b_int_sum_loop | 2.03 | JIT status unknown (slow loop) |
| b_list_sum | FAIL | JIT: unhandled opcode `Append` |
| b_int_primes | TIMEOUT | JIT verifier error, interp too slow |

---

## Summary: Known Failure Modes

### 1. Unhandled `Append` opcode in JIT lowering
Affects: `b_list_sum`, `sort`, `matrix_mult`
Fix needed in: `rust/lumen-codegen/src/ir.rs` — add Append → Cranelift lowering
Task: Create #34 sub-task

### 2. JIT Cranelift verifier errors
Affects: `nbody` (advance), `json_parse` (build_chunk), `b_int_primes` (is_prime)
Root cause: Likely invalid SSA form / phi nodes in generated Cranelift IR
Fix needed in: `rust/lumen-codegen/src/ir.rs` and/or `rust/lumen-codegen/src/jit.rs`
Task: Create #34 sub-task

### 3. OSR not firing for recursive benchmarks
Affects: `tree`, `b_ackermann`
Root cause: OSR detection threshold not reached for small-N recursive calls
Fix needed in: `rust/lumen-rt/src/vm/osr.rs`
This is partially blocked by task #24 (OSR stackmaps)

---

## Performance Gaps vs Target Languages

| Benchmark | Lumen | Python | Gap | Notes |
|-----------|-------|--------|-----|-------|
| Fibonacci | 0.21s | 1.48s  | Lumen 7x FASTER | JIT working well |
| Tree | 1.21s | 0.74s | Lumen 1.6x SLOWER | OSR not firing |
| String Ops | ~0ms | 0.02s | Lumen ~same | JIT helping |

**Target**: Beat Python on all benchmarks, approach Go performance.
**Current state**: Lumen beats Python on simple loops (fibonacci), lags on recursive/collection-heavy workloads.
