# lumen-core

Shared types and data structures for the Lumen compiler, VM, and runtime.

## Overview

`lumen-core` is the foundation crate that defines the core data structures used across the entire Lumen toolchain. It provides the LIR (Lumen Intermediate Representation) instruction set, value types, type system definitions, and string interning infrastructure. By centralizing these types in a single crate, we ensure consistency between the compiler (which generates LIR) and the VM (which executes it).

The crate is designed to be lightweight with minimal dependencies, making it suitable for use in WebAssembly builds and embedded environments. All types implement `Serialize` and `Deserialize` for persistence and network transport.

## Architecture

| Module | File | Purpose |
|--------|------|---------|
| **lir** | `src/lir.rs` | 32-bit fixed-width instruction encoding, ~100 opcodes, Lua-style register VM format |
| **values** | `src/values.rs` | Runtime value representation (15 types: primitives, collections, closures, futures) |
| **types** | `src/types.rs` | Type system definitions (primitives, generics, unions, function types, effect rows) |
| **strings** | `src/strings.rs` | String interning infrastructure for constant pool and runtime values |

## Key Types

### LIR Instructions

```rust
pub enum OpCode {
    LoadK, LoadNil, LoadBool, Move, MoveOwn,     // Registers & constants
    NewList, NewMap, NewRecord, NewUnion,         // Data construction
    GetField, SetField, GetIndex, SetIndex,       // Access
    Add, Sub, Mul, Div, FloorDiv, Pow,           // Arithmetic
    Eq, Lt, Le, Not, And, Or, In, Is,            // Comparison/logic
    Jmp, Call, Return, Halt, ForLoop,            // Control flow
    Intrinsic, Closure, Perform, Resume,         // Advanced features
    // ... ~100 total opcodes
}

pub struct Instruction(pub u32);  // 32-bit encoding: op(8) | a(8) | b(8) | c(8)

pub struct LirModule {
    pub cells: Vec<LirCell>,           // Function definitions
    pub strings: Vec<String>,          // String constant pool
    pub types: Vec<TypeDef>,          // Type metadata
    pub tools: Vec<ToolDecl>,         // Tool declarations
    pub policies: Vec<GrantPolicy>,   // Security policies
    // ...
}
```

**Critical**: Jump instructions (Jmp, Break, Continue) use **signed 24-bit offsets**. Always use `Instruction::sax()` and `sax_val()` accessors, never `ax()`/`ax_val()` (unsigned, truncates negative offsets).

### Value Types

```rust
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(InternedString),
    Bytes(Rc<Vec<u8>>),
    List(Rc<Vec<Value>>),
    Tuple(Rc<Vec<Value>>),
    Set(Rc<BTreeSet<Value>>),         // O(log n) membership
    Map(Rc<BTreeMap<String, Value>>),
    Record(Rc<RecordValue>),          // Named fields
    Union(Rc<UnionValue>),            // Tagged variant
    Closure(Rc<ClosureValue>),        // Function + environment
    TraceRef(String),                 // Trace handle
    Future(Rc<RefCell<FutureValue>>), // Async computation
}
```

Collections use `Rc<T>` for cheap reference-counted cloning with copy-on-write via `Rc::make_mut()`.

### Type System

```rust
pub enum Type {
    Any, Null, Bool, Int, Float, String, Bytes, Json,
    List(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Set(Box<Type>),
    Tuple(Vec<Type>),
    Result(Box<Type>, Box<Type>),
    Union(Vec<Type>),
    Function(FunctionType),          // Parameters + return + effects
    Named(String),                   // User-defined type
    Generic(String, Vec<Type>),      // Parameterized type
    // ...
}
```

## Usage

This is a library crate used as a dependency by other Lumen crates:

```toml
[dependencies]
lumen-core = { path = "../lumen-core", version = "0.1.0" }
```

```rust
use lumen_core::lir::{Instruction, OpCode, LirModule};
use lumen_core::values::Value;
use lumen_core::types::Type;

// Construct an instruction
let inst = Instruction::abc(OpCode::Add, 0, 1, 2);  // r0 = r1 + r2
assert_eq!(inst.opcode(), OpCode::Add);
assert_eq!(inst.a(), 0);
assert_eq!(inst.b(), 1);
assert_eq!(inst.c(), 2);

// Create a value
let v = Value::new_list(vec![
    Value::Int(1),
    Value::Int(2),
    Value::Int(3),
]);
```

## Testing

```bash
cargo test -p lumen-core
```

All types have comprehensive unit tests covering encoding/decoding, serialization, and edge cases.

## Related Crates

- **lumen-compiler** — Generates LIR from source code
- **lumen-rt** — Executes LIR via the register VM
- **lumen-wasm** — WASM bindings (includes lumen-core compiled to WASM)
