---
name: vm-architecture
description: Deep reference for Lumen's register-based VM - dispatch loop, values, intrinsics, processes, effects, continuations, and JIT tiering
---

# Lumen VM Architecture

The VM lives in `rust/lumen-rt/src/vm/` and `rust/lumen-core/src/`.

## Core Data Structures (`rust/lumen-core/src/`)
- `lir.rs`: `OpCode` (u8 enum, ~100 opcodes), `Instruction` (32-bit fixed-width), `LirModule`, `LirCell`
- `values.rs`: `Value` enum - scalars inline, collections `Arc<T>`-wrapped with COW via `Arc::make_mut()`
- `types.rs`: Runtime type definitions for schema validation

## Value Representation
| Type | Storage | Notes |
|------|---------|-------|
| Int, Float, Bool, Null | Inline (no heap) | Fast path in arithmetic |
| String | Interned via `StringTable` or owned | Dedup for constants |
| List | `Arc<Vec<Value>>` | COW mutation |
| Tuple | `Arc<Vec<Value>>` | Fixed-length |
| Set | `Arc<BTreeSet<Value>>` | O(log n) membership, NOT Vec |
| Map | `Arc<BTreeMap<String, Value>>` | String keys at runtime |
| Record | `Arc<RecordValue>` | Named type + field map |
| Union | `Arc<(String, Value)>` | Tag + payload |
| Closure | `Arc<ClosureValue>` | Function + captured env |
| Future | `Arc<FutureValue>` | Pending/Completed/Error |

## VM Dispatch Loop (`rust/lumen-rt/src/vm/mod.rs`)
- `VM` struct: register file (`Vec<Value>`), call stack, instruction pointer
- `run_until()`: hot dispatch loop
- `execute()`: entry point
- Call-frame stack: max 256 depth
- Effect handler stack: `EffectScope` tracks active handlers

## Module Organization
- `vm/mod.rs` - Core dispatch loop
- `vm/intrinsics.rs` - 80+ builtin dispatch (`call_builtin`)
- `vm/ops.rs` - Arithmetic (inlined hot path for Int/Float, BigInt fallback)
- `vm/helpers.rs` - Utility functions
- `vm/processes.rs` - MemoryRuntime (KV), MachineRuntime (state graphs)
- `vm/continuations.rs` - Multi-shot delimited continuations

## Algebraic Effects
- `perform Effect.operation(args)` → searches handler stack upward
- `HandlePush` → pushes handler scope
- `HandlePop` → pops handler scope
- `Resume` → resumes captured continuation with value
- `SuspendedContinuation` captures full execution state
- One-shot semantics: each continuation resumed exactly once

## Opcode Families (~100 opcodes)
- **Load/Move**: LoadConst, LoadNull, LoadTrue, LoadFalse, Move, Copy
- **Data**: NewList, NewTuple, NewSet, NewMap, NewRecord, NewUnion
- **Access**: GetField, SetField, GetIndex, SetIndex, GetFieldDynamic
- **Arithmetic**: Add, Sub, Mul, Div, FloorDiv, Mod, Pow, Neg, Shl, Shr
- **Comparison**: Eq, Neq, Lt, Le, Gt, Ge
- **Control**: Jmp, Test, Call, Return, Break, Continue
- **Intrinsics**: CallBuiltin (dispatches 83 builtins)
- **Closures**: Closure, ClosureCall
- **Effects**: Perform, HandlePush, HandlePop, Resume

## Process Runtimes
- **memory**: append, recent, remember, recall, upsert, get, query, store (instances isolated)
- **machine**: run, start, step, is_terminal, current_state, resume_from (typed state graphs with guards)
- **pipeline**: auto-chaining stages, strict single-argument arity

## JIT Tiering (`rust/lumen-rt/src/jit_tier.rs` + `rust/lumen-codegen/src/`)
- Profiling: counts calls to each cell
- Hot threshold triggers Cranelift compilation
- Caches native function pointers, bypasses interpreter
- WASM backend via `wasm-encoder`

## Concurrency
- M:N work-stealing scheduler (`rust/lumen-rt/src/services/scheduler.rs`)
- Actors with mailboxes and priorities
- Typed bounded/unbounded MPSC channels
- Supervisors (one-for-one, one-for-all restart)
- Nurseries (structured concurrency scopes)
- `@deterministic true` forces FIFO scheduling
