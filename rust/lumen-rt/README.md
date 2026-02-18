# lumen-rt

The Lumen runtime combining the register-based virtual machine and runtime services.

## Overview

`lumen-rt` provides the execution engine for Lumen programs. It includes a register-based VM that interprets LIR bytecode, 80+ builtin functions, algebraic effect handling with delimited continuations, an M:N work-stealing scheduler, actor-based process management, tool dispatch infrastructure, structured tracing, and comprehensive runtime services (HTTP, filesystem, crypto, networking).

The VM uses a call-frame stack with 32-bit fixed-width instructions (Lua-style encoding) and supports up to 256 nested calls. Collections use `Arc<T>` wrappers for efficient reference counting with copy-on-write semantics. The runtime is designed for both single-threaded deterministic execution and concurrent multi-process workloads.

Optional JIT compilation is available via the `jit` feature (enabled by default), which compiles hot code paths to native machine code using the Cranelift backend.

## Architecture

### Core VM

| Module | Purpose |
|--------|---------|
| `vm/mod.rs` | Main dispatch loop, instruction execution, frame management |
| `vm/intrinsics.rs` | 80+ builtin function implementations |
| `vm/ops.rs` | Arithmetic operations (Int/Float fast path, BigInt fallback) |
| `vm/helpers.rs` | VM utility functions (type coercion, truthiness) |
| `vm/processes.rs` | MemoryRuntime (KV store), MachineRuntime (state machines) |
| `vm/continuations.rs` | Multi-shot delimited continuations for algebraic effects |

### Runtime Services

| Module | Purpose |
|--------|---------|
| `services/tools.rs` | ToolProvider trait, ProviderRegistry, policy validation |
| `services/scheduler.rs` | M:N work-stealing scheduler, task queue, executor |
| `services/process.rs` | Actor model (PCB, mailbox, priority queues) |
| `services/trace/` | Structured tracing (events, spans, recording) |
| `services/execution_graph.rs` | DAG visualization (DOT, Mermaid, JSON rendering) |
| `services/schema_drift.rs` | API shape change detection via recursive type comparison |
| `services/crypto.rs` | SHA-256, BLAKE3, HMAC-SHA256, HKDF, Ed25519 signing |
| `services/retry.rs` | Retry-After with exponential/Fibonacci backoff |
| `services/http.rs` | RequestBuilder, Router with path parameter extraction |
| `services/fs_async.rs` | Async file operations, batch I/O, file watching |
| `services/net.rs` | TCP/UDP configuration, socket addresses, DNS resolution |

### Memory Management

| Module | Purpose |
|--------|---------|
| `arena.rs` | Bump allocator for short-lived allocations |
| `gc.rs` | Tracing garbage collector interface |
| `immix.rs` | Immix GC implementation (block-based, mark-and-sweep) |
| `tagged.rs` | NaN-boxed tagged value representation (WIP) |
| `tlab.rs` | Thread-local allocation buffers |

### Other

| Module | Purpose |
|--------|---------|
| `jit_tier.rs` | JIT tiering integration with Cranelift backend |
| `interpreter.rs` | High-level interpreter entry points |
| `intrinsics.rs` | Builtin function registry |
| `parity_concurrency.rs` | Concurrency parity checklist (38 items) |

## Key APIs

### VM Execution

```rust
use lumen_rt::vm::VM;
use lumen_core::lir::LirModule;

let module = /* compiled LirModule */;
let mut vm = VM::new(module);

// Execute main cell
let result = vm.call_cell_by_name("main", vec![])?;
println!("Result: {:?}", result);

// Execute specific cell with arguments
let args = vec![Value::Int(42), Value::String("test".into())];
let result = vm.call_cell_by_name("process", args)?;
```

### Tool Dispatch

```rust
use lumen_rt::services::tools::{ToolProvider, ToolError, ToolSchema};

struct MyProvider;

impl ToolProvider for MyProvider {
    fn name(&self) -> &str { "my_tool" }
    
    fn call(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        // Implementation
        Ok(json!({"result": "success"}))
    }
    
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "my_tool".to_string(),
            description: "My custom tool".to_string(),
            input_schema: json!({"type": "object"}),
            output_schema: json!({"type": "object"}),
            effects: vec![],
        }
    }
}
```

### Scheduler

```rust
use lumen_rt::services::scheduler::{Scheduler, Task};

let mut scheduler = Scheduler::new(num_cpus::get());

scheduler.spawn(Task::new(/* ... */));
scheduler.spawn(Task::new(/* ... */));

scheduler.run_until_idle()?;
```

## Critical Implementation Details

**Signed jump offsets**: Use `Instruction::sax()` and `sax_val()` for Jmp/Break/Continue instructions. Never use `ax`/`ax_val` (unsigned, silently truncates negative offsets).

**Set representation**: Uses `BTreeSet<Value>` for O(log n) membership testing, NOT `Vec<Value>`.

**Collection cloning**: All collections (List, Map, Set, Tuple, Record) are wrapped in `Arc<T>`. Mutation uses `Arc::make_mut()` for copy-on-write.

**Call frame limit**: Maximum stack depth is 256 frames. Deeper recursion produces a runtime error.

**Effect handling**: One-shot delimited continuations. Each `SuspendedContinuation` can only be resumed once.

**Type::Any propagation**: Builtin functions return `Type::Any`. Binary operations check for `Type::Any` before type-specific branches.

## Usage

### As a Library

```rust
use lumen_rt::vm::VM;
use lumen_compiler::compile;

let source = r#"
cell main() -> Int
  return 42
end
"#;

let module = compile(source)?;
let mut vm = VM::new(module);
let result = vm.call_cell_by_name("main", vec![])?;

assert_eq!(result, Value::Int(42));
```

### With Tool Providers

```rust
use lumen_rt::services::tools::ProviderRegistry;
use lumen_provider_http::HttpProvider;
use lumen_provider_json::JsonProvider;

let mut registry = ProviderRegistry::new();
registry.register(Box::new(HttpProvider::new()));
registry.register(Box::new(JsonProvider::new()));

// VM will use registry for tool calls
let mut vm = VM::with_registry(module, registry);
```

## Testing

```bash
# All runtime tests
cargo test -p lumen-rt

# VM-specific tests
cargo test -p lumen-rt vm::

# Service tests
cargo test -p lumen-rt services::

# With JIT disabled
cargo test -p lumen-rt --no-default-features
```

## Features

- **`default`** — Enables JIT compilation
- **`jit`** — Includes `lumen-codegen` for Cranelift JIT backend

## Related Crates

- **lumen-core** — LIR instruction encoding and value types
- **lumen-compiler** — Produces LIR modules that the VM executes
- **lumen-codegen** — JIT compilation backend (optional)
- **lumen-cli** — CLI that orchestrates compilation and VM execution
- **lumen-provider-*** — Tool provider implementations
