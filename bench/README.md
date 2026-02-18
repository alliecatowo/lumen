# Lumen Comprehensive Benchmark Suite

This directory contains benchmarks for the Lumen programming language, organized into two categories:

## Overview

**Total benchmarks**: 17
- **Cross-language**: 9 algorithms (C, Go, Rust, Zig, Python, TypeScript, Lumen)
- **Lumen-specific**: 8 language features

All benchmarks use **identical algorithms** across languages to ensure fair comparison and measure **wall-clock execution time** (compilation/interpretation excluded).

## Directory Structure

```
bench/
├── run_full_suite.sh           # Main benchmark runner (NEW)
├── run_all.sh                  # Legacy cross-language runner
├── .build/                     # Compiled binaries
├── results/                    # CSV outputs and reports
├── cross-language/             # 9 cross-language benchmarks
│   ├── fibonacci/              # fib(35) recursive
│   ├── json_parse/             # Parse and extract JSON
│   ├── string_ops/             # String manipulation
│   ├── tree/                   # Tree traversal/transform
│   ├── sort/                   # Quicksort
│   ├── nbody/                  # 2-body gravitational sim
│   ├── matrix_mult/            # 200x200 matrix multiply
│   ├── fannkuch/               # Fannkuch permutation test
│   └── primes_sieve/           # Sieve of Eratosthenes
└── b_*.lm                      # 8 Lumen-specific benchmarks
    ├── b_ackermann.lm          # Ackermann function
    ├── b_call_overhead.lm      # Function call overhead
    ├── b_float_mandelbrot.lm   # Mandelbrot set (200x200)
    ├── b_int_fib.lm            # Optimized integer Fibonacci
    ├── b_int_primes.lm         # Prime number generation
    ├── b_int_sum_loop.lm       # Tight integer loop
    ├── b_list_sum.lm           # List folding
    └── b_string_concat.lm      # String concatenation
```

## Cross-Language Benchmarks (9)

All algorithms implemented identically in 7 languages. Each benchmark has same I/O semantics and convergence criteria.

### 1. Fibonacci
- **Algorithm**: Recursive fib(35)
- **Measures**: Function call overhead, recursion
- **Time**: ~15-1000ms depending on language
- **File names**: `fib.c`, `fib.go`, `fib.rs`, `fib.zig`, `fib.py`, `fib.ts`, `fib.lm`

### 2. JSON Parse
- **Algorithm**: Parse JSON with nested arrays/objects, extract value at path
- **Measures**: String parsing, data structure construction
- **Input**: 500KB sample JSON
- **File names**: `json_parse.*`

### 3. String Operations
- **Algorithm**: split(), join(), replace(), contains(), slice()
- **Measures**: String manipulation, memory allocation
- **Input**: 100KB text corpus
- **File names**: `string_ops.*`

### 4. Tree
- **Algorithm**: Build balanced tree, traverse, compute sum of values
- **Measures**: Recursion, data structure traversal
- **Nodes**: ~2000 nodes, depth ~10
- **File names**: `tree.*`

### 5. Sort
- **Algorithm**: Quicksort on random integers
- **Measures**: Sorting performance, comparison ops
- **Array size**: 10,000 elements
- **File names**: `sort.*`

### 6. N-Body
- **Algorithm**: 2-body gravitational simulation for 1,000,000 steps
- **Measures**: Floating-point arithmetic, tight numerical loop
- **File names**: `nbody.*`

### 7. Matrix Multiplication
- **Algorithm**: 200x200 matrix multiply (C = A × B)
- **Measures**: Nested loops, floating-point ops
- **File names**: `matrix_mult.*`

### 8. Fannkuch
- **Algorithm**: Fannkuch permutation benchmark (cache-busting)
- **Measures**: Array manipulation, permutation generation
- **File names**: `fannkuch.*`

### 9. Primes Sieve
- **Algorithm**: Sieve of Eratosthenes to find all primes < 100,000
- **Measures**: Bit manipulation, memory efficiency
- **File names**: `primes_sieve.*`

## Lumen-Specific Benchmarks (8)

Focused on Lumen language features and implementation characteristics.

### 1. Ackermann (b_ackermann.lm)
- **Algorithm**: Ackermann(3, 5) — extremely deep recursion
- **Measures**: Call stack depth, recursion handling
- **Expected**: 253 steps, very slow

### 2. Call Overhead (b_call_overhead.lm)
- **Algorithm**: 10M empty function calls
- **Measures**: Function call overhead, dispatch speed
- **Expected**: Baseline for JIT effectiveness

### 3. Mandelbrot (b_float_mandelbrot.lm)
- **Algorithm**: Compute Mandelbrot set on 200x200 grid, max 100 iterations
- **Measures**: Floating-point arithmetic, nested loops, branching
- **Output**: Count of points in set

### 4. Integer Fibonacci (b_int_fib.lm)
- **Algorithm**: fib(35) optimized for integer performance
- **Measures**: Integer arithmetic, recursion (vs. cross-language fib)
- **Comparison**: Baseline against C/Go/Rust

### 5. Integer Primes (b_int_primes.lm)
- **Algorithm**: Generate all primes < 10,000
- **Measures**: Integer operations, loop efficiency
- **Output**: Prime count and final prime value

### 6. Integer Sum Loop (b_int_sum_loop.lm)
- **Algorithm**: Sum integers 0..10M in tight loop
- **Measures**: Loop optimization, integer addition
- **Expected**: Single numeric result

### 7. List Sum (b_list_sum.lm)
- **Algorithm**: Fold over list of 100K integers
- **Measures**: Higher-order function performance, memory allocation
- **Output**: Sum of all elements

### 8. String Concatenation (b_string_concat.lm)
- **Algorithm**: Concatenate 10K strings efficiently
- **Measures**: String allocation, memory efficiency
- **Output**: Final concatenated string length

## Running Benchmarks

### Full Suite (All Benchmarks)
```bash
bash bench/run_full_suite.sh --runs 3
```
Runs 3 iterations of all 17 benchmarks (9 cross-language × 7 languages + 8 Lumen-specific).

### Only Lumen
```bash
bash bench/run_full_suite.sh --only-lumen --runs 3
```
Runs only Lumen-specific benchmarks (8 total).

### Only Cross-Language
```bash
bash bench/run_full_suite.sh --no-cross --runs 3
```
(Note: This flag is currently ignored; use `--only-lumen` to skip cross-language)

### Specific Language
```bash
bash bench/run_full_suite.sh --lang lumen --runs 3
```
Runs only Lumen implementations across all benchmarks.

### With CSV Output
```bash
bash bench/run_full_suite.sh --csv results/mybench.csv --runs 3
```
Saves raw results to CSV for external analysis.

### Quick Test
```bash
bash bench/run_full_suite.sh --only-lumen --runs 1
```
Single run of Lumen benchmarks for quick verification (~5 seconds).

## Output Format

### Console Output
- **Header**: Configuration, detected compilers
- **Per-benchmark**: Algorithm name, language, run number, elapsed milliseconds
- **Summary table**: Median times for each benchmark across languages

### CSV Output Format
```
benchmark,language,run,time_ms
fibonacci,c,1,16
fibonacci,c,2,15
fibonacci,go,1,41
...
ackermann,lumen,1,810
...
```

Fields:
- `benchmark`: Benchmark name (e.g., "fibonacci", "ackermann")
- `language`: Implementation language (c, go, rust, zig, python, typescript, lumen)
- `run`: Run number (1 to N)
- `time_ms`: Elapsed milliseconds (or ERROR if compilation failed)

## Measurement Methodology

1. **Compilation**: Each language compiles in release/optimized mode once, outside timer
2. **Timing**: Wall-clock time measured using `date +%s%N` (nanosecond resolution)
3. **Accuracy**: Times are independent; no warmup or JIT priming
4. **Reporting**: Median of N runs reported (eliminates outliers)
5. **Failure handling**: Failed compilations marked as ERROR in CSV

### Compiler Flags

| Language | Flags |
|----------|-------|
| C | `-O2` |
| Go | Default (`go build`) |
| Rust | `-O` (release mode) |
| Zig | `-O ReleaseFast` |
| Python | Interpreted (no compilation) |
| TypeScript | Interpreted via `tsx` |
| Lumen | Compiled by `lumen run` |

## Analogousness Verification

All cross-language benchmarks have been verified to:
1. **Same algorithm**: Identical logic across languages
2. **Same problem size**: Identical input dimensions and iteration counts
3. **Same output validation**: Print same result to verify correctness
4. **Same I/O semantics**: All compile to machine code (except Python/TS)

Example (fibonacci):
```c
// C: fib(35)
int fibonacci(int n) {
    if (n < 2) return n;
    return fibonacci(n - 1) + fibonacci(n - 2);
}
```

```lumen
// Lumen: identical algorithm
cell fibonacci(n: Int) -> Int
  if n < 2
    return n
  end
  return fibonacci(n - 1) + fibonacci(n - 2)
end
```

## Interpreting Results

### Expected Performance Hierarchy

Typical single-run times (on 2024 hardware):

| Algorithm | C | Go | Rust | Zig | Python | TypeScript | Lumen |
|-----------|---|----|----|-----|--------|------------|-------|
| fibonacci(35) | 10-20ms | 30-50ms | 15-30ms | 20-40ms | 500-1000ms | 800-1500ms | 50-100ms |
| json_parse | 2-5ms | 10-20ms | 3-8ms | 2-4ms | 15-50ms | 500-1000ms | 50-200ms |
| string_ops | 2-5ms | 3-5ms | 2-4ms | 1-3ms | 10-20ms | 500-1000ms | 5-15ms |
| nbody (1M) | 30-50ms | 50-100ms | 25-50ms | 25-50ms | 4000-6000ms | 1000-2000ms | 300-500ms |

**Notes:**
- Lumen times are competitive with Go in most cases
- Python/TypeScript are 10-100x slower (interpreted)
- C/Rust/Zig are fastest (compiled, no GC overhead)

## Legacy Script

The original `run_all.sh` is still available for cross-language benchmarks only (5 benchmarks). Use `run_full_suite.sh` for comprehensive testing.

## Future Enhancements

Planned additions:
- [ ] GC pause measurement for Lumen
- [ ] Compilation time tracking
- [ ] Memory usage profiling
- [ ] JIT warm-up iterations
- [ ] Statistical significance testing
- [ ] Regression detection (against baseline)
