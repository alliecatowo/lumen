# Lumen Benchmark Suite

Goal: **Lumen must beat Python 3 and TypeScript/Node.js in all four benchmarks.**

This directory contains four micro-benchmarks in Lumen, Python, and JavaScript/Node.
Once the JIT reaches full coverage for the hot opcodes (`NewList`, `GetIndex`, `SetIndex`,
`Intrinsic`, `Concat`), Lumen should be competitive with Node and approach compiled-language speeds.

## Benchmarks

| Benchmark | What it tests |
|-----------|---------------|
| `fib/` | Recursive fibonacci(35) — function call overhead, deep recursion |
| `nbody/` | N-body simulation (5 bodies, 50k steps) — FP arithmetic, nested loops |
| `sort/` | Sort 100k integers (builtin) — list ops, stdlib |
| `strings/` | String concat 10k × "hello world " + count "world" — allocation + scan |

## Running

```bash
# Build Lumen first
cargo build --release --bin lumen

# Run all benchmarks (Lumen + Python + Node)
./benchmarks/run.sh

# JIT on/off comparison
./benchmarks/run.sh --jit
./benchmarks/run.sh --no-jit

# Single benchmark
./benchmarks/run.sh fib
```

## Results

> **Environment:** CI / GitHub Actions, ubuntu-latest  
> **Lumen:** `feat/benchmark-suite-pr5`, interpreter mode (JIT fallback — collection opcodes not yet JIT-compiled)  
> Times are wall-clock, single run (CI will use median of 3).

| Benchmark | Lumen | Python 3 | Node.js | Lumen vs Python | Lumen vs Node |
|-----------|------:|--------:|--------:|:---------------:|:-------------:|
| `fib(35)` | pending CI | **2470 ms** | **328 ms** | pending CI | pending CI |
| `nbody(50k)` | pending CI | **611 ms** | **79 ms** | pending CI | pending CI |
| `sort(100k)` | pending CI | **42 ms** | **62 ms** | pending CI | pending CI |
| `strings(10k)` | pending CI | **38 ms** | **64 ms** | pending CI | pending CI |

> Lumen times are **pending CI** — the binary must be built with `cargo build --release --bin lumen`.
> Python and Node times were measured on the dev machine (Linux x86-64, Python 3.12, Node 20).

### Python and Node baseline (dev machine, 2026-03-09)

```
fibonacci(35):   Python 2470ms  Node 328ms
nbody(50k):      Python  611ms  Node  79ms
sort(100k):      Python   42ms  Node  62ms
strings(10k):    Python   38ms  Node  64ms
```

### Analysis

- **Python** is already beaten by Node on all four benchmarks.
- **Node** (V8 JIT) is the harder target. Lumen must close the gap via Cranelift JIT.
- **Fib** is the purest JIT test — once Cranelift compiles recursive calls, Lumen should beat Python easily and approach Node.
- **Nbody** is blocked on JIT coverage for `SetIndex`/`GetIndex` (list mutation). Until those are JIT-compiled, Lumen falls back to interpreter.
- **Sort** and **Strings** depend on `Intrinsic` and `Concat` being JIT-compiled. Currently interpreter-only.

## JIT Coverage Gap (blocking beating Node)

See [`docs/opcode-coverage.md`](../docs/opcode-coverage.md) for the full audit.

Key missing JIT opcodes that affect these benchmarks:

| Opcode | Affects | Fix |
|--------|---------|-----|
| `NewList` | nbody, sort | Add GC-alloc shim in Cranelift codegen |
| `GetIndex` / `SetIndex` | nbody, sort | Inline GC-aware index ops |
| `Intrinsic` (sort, len) | sort | Extern shim to call `lumen_sort` |
| `Concat` | strings | Extern shim to call `lumen_concat` |
| `Append` | strings, sort | Extern shim to call `lumen_append` |

Once these 5 opcode families are JIT-compiled, Lumen should:
- **Beat Python** on all 4 benchmarks (Python has no native JIT for number-crunching)
- **Approach Node** on fib/nbody (native arithmetic), likely still slower on sort/strings due to runtime overhead

## CI Integration

Copy `docs/workflows/bench.yml` to `.github/workflows/bench.yml` to enable automated benchmarking on every push to `main`.
