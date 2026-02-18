# lumen-codegen

Native code generation backends for Lumen via Cranelift.

## Overview

`lumen-codegen` provides code generation backends that compile LIR bytecode to native machine code. It includes a JIT (Just-In-Time) compiler using Cranelift for hot code paths, an AOT (Ahead-Of-Time) compiler for standalone executables, and a WebAssembly backend for browser and WASI deployment.

The JIT compiler is integrated with `lumen-rt` via tiering: the VM starts in interpretation mode, tracks hot functions, and promotes them to native code for performance-critical workloads. The AOT compiler produces portable executables with the VM embedded. The WASM backend generates `.wasm` modules with Lumen-specific extensions via WIT (WebAssembly Interface Types).

## Architecture

| Module | Purpose |
|--------|---------|
| `jit.rs` | JIT compiler entry point, Cranelift module management |
| `aot.rs` | Ahead-of-time compilation to native executables |
| `wasm.rs` | WebAssembly code generation (browser, Node.js, WASI) |
| `context.rs` | Compilation context (symbol tables, type mappings) |
| `ir.rs` | LIR → Cranelift IR translation |
| `types.rs` | Type lowering (Lumen types → Cranelift types) |
| `emit.rs` | Code emission utilities |
| `opt.rs` | Optimization passes (inlining, dead code elimination) |
| `ffi.rs` | Foreign function interface (C interop) |
| `wit.rs` | WIT (WebAssembly Interface Types) generation |
| `bench_programs.rs` | Benchmark programs for performance testing |

## Key APIs

### JIT Compilation

```rust
use lumen_codegen::jit::JitCompiler;
use lumen_core::lir::LirModule;

let module = /* compiled LIR module */;
let mut jit = JitCompiler::new();

// Compile a cell to native code
let native_fn = jit.compile_cell(&module, cell_idx)?;

// Call the native function
let result = native_fn.call(&[arg1, arg2])?;
```

### AOT Compilation

```rust
use lumen_codegen::aot::compile_to_executable;

let module = /* compiled LIR module */;
let executable_path = "output/program";

compile_to_executable(&module, executable_path)?;
// Produces a standalone native executable
```

### WebAssembly Backend

```rust
use lumen_codegen::wasm::{compile_to_wasm, WasmTarget};

let module = /* compiled LIR module */;

// Browser target (ES modules)
let wasm_bytes = compile_to_wasm(&module, WasmTarget::Web)?;

// Node.js target (CommonJS)
let wasm_bytes = compile_to_wasm(&module, WasmTarget::NodeJs)?;

// WASI target (filesystem access)
let wasm_bytes = compile_to_wasm(&module, WasmTarget::Wasi)?;
```

## Usage

### As a Library (JIT)

```rust
use lumen_codegen::jit::JitCompiler;
use lumen_compiler::compile;

let source = r#"
cell fib(n: Int) -> Int
  if n <= 1
    return n
  end
  return fib(n - 1) + fib(n - 2)
end
"#;

let module = compile(source)?;
let mut jit = JitCompiler::new();

// Compile hot function
let cell_idx = module.find_cell("fib").unwrap();
let native_fib = jit.compile_cell(&module, cell_idx)?;

// Call native code
let result = native_fib.call(&[Value::Int(10)])?;
assert_eq!(result, Value::Int(55));
```

### Integration with VM

The VM automatically promotes hot functions to native code when the `jit` feature is enabled:

```rust
use lumen_rt::vm::VM;

let mut vm = VM::new(module);

// VM interprets code initially
vm.call_cell_by_name("main", vec![])?;

// Hot paths are automatically JIT-compiled after threshold calls
// (default: 100 invocations)
```

### WASM Compilation

```rust
use lumen_codegen::wasm::{compile_to_wasm, WasmTarget};
use std::fs;

let module = compile(source)?;
let wasm_bytes = compile_to_wasm(&module, WasmTarget::Web)?;

fs::write("output.wasm", wasm_bytes)?;
```

## Optimization Levels

The compiler supports multiple optimization levels:

- **`O0`** — No optimization (fast compilation)
- **`O1`** — Basic optimization (balanced)
- **`O2`** — Aggressive optimization (slower compilation, fast execution)
- **`O3`** — Maximum optimization (slowest compilation, maximum performance)

Set via environment variable:

```bash
LUMEN_OPT_LEVEL=2 cargo run -p lumen-cli -- run program.lm
```

## Performance

Benchmarks show:
- **JIT warmup**: ~50µs per function on first compile
- **Native speedup**: 5-10x over interpreted bytecode for numeric code
- **Compilation overhead**: Amortized after 100+ invocations

See `benches/codegen_bench.rs` for detailed benchmarks.

## Testing

```bash
# All codegen tests
cargo test -p lumen-codegen

# JIT-specific tests
cargo test -p lumen-codegen jit::

# WASM tests
cargo test -p lumen-codegen wasm::

# Run benchmarks
cargo bench -p lumen-codegen
```

## Dependencies

Uses Cranelift 0.116:
- `cranelift-codegen` — IR and code generation
- `cranelift-frontend` — Function builder API
- `cranelift-module` — Module management
- `cranelift-native` — Native target detection
- `cranelift-object` — Object file generation
- `cranelift-jit` — JIT execution engine

## Current Limitations

- No SIMD intrinsics yet (planned)
- Limited inlining heuristics
- No escape analysis for heap allocation
- WASM backend doesn't support full effect system yet

See `ROADMAP.md` for planned enhancements.

## Related Crates

- **lumen-core** — LIR instruction definitions
- **lumen-rt** — VM that uses JIT for hot code paths
- **lumen-wasm** — WASM bindings (uses this crate's WASM backend)
- **lumen-cli** — CLI that invokes AOT and WASM compilation
