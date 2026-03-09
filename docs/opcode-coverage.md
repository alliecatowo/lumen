# LIR Opcode Coverage Audit

> **Generated:** 2026-03-09  
> **Audit goal:** Ensure every opcode defined in `lumen-compiler/src/compiler/lir.rs` is implemented across all execution tiers.  
> **Tracking issue:** ALLIE-232

## Execution Tiers

| Tier | Location | Description |
|------|----------|-------------|
| **Interpreter** | `rust/lumen-vm/src/vm/mod.rs` | Bytecode dispatch loop — full coverage expected |
| **Cranelift JIT** | `rust/lumen-codegen/src/jit.rs` + `lower.rs` | Native codegen via Cranelift; partial coverage — unsupported opcodes fall back to interpreter |
| **Stencil** | *(not yet implemented)* | Planned middle tier: stencil-and-stitch copy-and-patch JIT |

## Coverage Table

Legend: ✅ implemented · ❌ not implemented · ⚠️ partial/fallback

| Opcode | Hex | Interpreter | Cranelift JIT | Stencil | Notes |
|--------|-----|:-----------:|:-------------:|:-------:|-------|
| `Nop` | 0x00 | ✅ | ✅ | ❌ | No-op |
| `LoadK` | 0x01 | ✅ | ✅ | ❌ | Load constant |
| `LoadNil` | 0x02 | ✅ | ✅ | ❌ | Set registers to nil |
| `LoadBool` | 0x03 | ✅ | ✅ | ❌ | Load boolean |
| `LoadInt` | 0x04 | ✅ | ✅ | ❌ | Load small integer |
| `Move` | 0x05 | ✅ | ✅ | ❌ | Copy register |
| `NewList` | 0x06 | ✅ | ❌ | ❌ | Create list — heap alloc, not JIT-able yet |
| `NewMap` | 0x07 | ✅ | ❌ | ❌ | Create map — heap alloc |
| `NewRecord` | 0x08 | ✅ | ❌ | ❌ | Create record — heap alloc |
| `NewUnion` | 0x09 | ✅ | ❌ | ❌ | Create union tag/payload |
| `NewTuple` | 0x0A | ✅ | ❌ | ❌ | Create tuple — heap alloc |
| `NewSet` | 0x0B | ✅ | ❌ | ❌ | Create set — heap alloc |
| `MoveOwn` | 0x0C | ✅ | ✅ | ❌ | Move (ownership transfer) |
| `GetField` | 0x10 | ✅ | ❌ | ❌ | Field access — requires GC value |
| `SetField` | 0x11 | ✅ | ❌ | ❌ | Field mutation |
| `GetIndex` | 0x12 | ✅ | ❌ | ❌ | Index access |
| `SetIndex` | 0x13 | ✅ | ❌ | ❌ | Index mutation |
| `GetTuple` | 0x14 | ✅ | ❌ | ❌ | Tuple element access |
| `Add` | 0x20 | ✅ | ✅ | ❌ | Integer/float add |
| `Sub` | 0x21 | ✅ | ✅ | ❌ | Subtract |
| `Mul` | 0x22 | ✅ | ✅ | ❌ | Multiply |
| `Div` | 0x23 | ✅ | ✅ | ❌ | Divide |
| `Mod` | 0x24 | ✅ | ✅ | ❌ | Modulo |
| `Pow` | 0x25 | ✅ | ✅ | ❌ | Power |
| `Neg` | 0x26 | ✅ | ✅ | ❌ | Negate |
| `Concat` | 0x27 | ✅ | ❌ | ❌ | String concat — heap alloc |
| `BitOr` | 0x28 | ✅ | ✅ | ❌ | Bitwise OR |
| `BitAnd` | 0x29 | ✅ | ✅ | ❌ | Bitwise AND |
| `BitXor` | 0x2A | ✅ | ✅ | ❌ | Bitwise XOR |
| `BitNot` | 0x2B | ✅ | ✅ | ❌ | Bitwise NOT |
| `Shl` | 0x2C | ✅ | ✅ | ❌ | Shift left |
| `Shr` | 0x2D | ✅ | ✅ | ❌ | Shift right |
| `FloorDiv` | 0x2E | ✅ | ✅ | ❌ | Floor division |
| `Eq` | 0x30 | ✅ | ✅ | ❌ | Equality test |
| `Lt` | 0x31 | ✅ | ✅ | ❌ | Less-than |
| `Le` | 0x32 | ✅ | ✅ | ❌ | Less-or-equal |
| `Not` | 0x33 | ✅ | ✅ | ❌ | Logical NOT |
| `And` | 0x34 | ✅ | ✅ | ❌ | Logical AND |
| `Or` | 0x35 | ✅ | ✅ | ❌ | Logical OR |
| `In` | 0x36 | ✅ | ❌ | ❌ | Membership test — requires GC value |
| `Is` | 0x37 | ✅ | ❌ | ❌ | Type check |
| `NullCo` | 0x38 | ✅ | ❌ | ❌ | Null coalescing |
| `Test` | 0x39 | ✅ | ✅ | ❌ | Truthiness test + skip |
| `Jmp` | 0x40 | ✅ | ✅ | ❌ | Unconditional jump |
| `Call` | 0x41 | ✅ | ✅ | ❌ | Function call |
| `TailCall` | 0x42 | ✅ | ✅ | ❌ | Tail call |
| `Return` | 0x43 | ✅ | ✅ | ❌ | Return |
| `Halt` | 0x44 | ✅ | ✅ | ❌ | Halt with error |
| `Loop` | 0x45 | ✅ | ✅ | ❌ | Loop counter + jump |
| `ForPrep` | 0x46 | ✅ | ✅ | ❌ | Numeric for prep |
| `ForLoop` | 0x47 | ✅ | ✅ | ❌ | Numeric for step |
| `ForIn` | 0x48 | ✅ | ✅ | ❌ | Iterator for-in step |
| `Break` | 0x49 | ✅ | ✅ | ❌ | Break from loop |
| `Continue` | 0x4A | ✅ | ✅ | ❌ | Continue loop |
| `Intrinsic` | 0x50 | ✅ | ❌ | ❌ | Builtin call (len, sort, etc.) — not JIT-able yet |
| `Closure` | 0x51 | ✅ | ❌ | ❌ | Create closure — heap alloc |
| `GetUpval` | 0x52 | ✅ | ❌ | ❌ | Get upvalue |
| `SetUpval` | 0x53 | ✅ | ❌ | ❌ | Set upvalue |
| `ToolCall` | 0x60 | ✅ | ❌ | ❌ | AI tool call — async, not JIT-able |
| `Schema` | 0x61 | ✅ | ❌ | ❌ | Schema validation |
| `Emit` | 0x62 | ✅ | ❌ | ❌ | Emit output |
| `TraceRef` | 0x63 | ✅ | ❌ | ❌ | Trace reference |
| `Await` | 0x64 | ✅ | ❌ | ❌ | Await future |
| `Spawn` | 0x65 | ✅ | ❌ | ❌ | Spawn async |
| `Perform` | 0x66 | ✅ | ❌ | ❌ | Perform algebraic effect |
| `HandlePush` | 0x67 | ✅ | ❌ | ❌ | Push effect handler |
| `HandlePop` | 0x68 | ✅ | ❌ | ❌ | Pop effect handler |
| `Resume` | 0x69 | ✅ | ❌ | ❌ | Resume suspended computation |
| `Append` | 0x70 | ✅ | ❌ | ❌ | Append to list |
| `IsVariant` | 0x71 | ✅ | ❌ | ❌ | Union variant check |
| `Unbox` | 0x72 | ✅ | ❌ | ❌ | Unbox union payload |

## Summary

| Tier | Supported | Total | Coverage |
|------|-----------|-------|----------|
| Interpreter | 71 | 71 | **100%** |
| Cranelift JIT | 39 | 71 | **55%** |
| Stencil | 0 | 71 | **0%** (not yet built) |

## JIT Coverage Analysis

### Cranelift JIT — Supported opcodes (39/71)

All arithmetic, logic, comparison, control-flow, and basic register ops are JIT-compiled. This covers the core numeric hot paths (fibonacci, nbody, mergesort) needed to beat Python/TypeScript.

**Fully supported categories:**
- ✅ Arithmetic: `Add`, `Sub`, `Mul`, `Div`, `Mod`, `Pow`, `Neg`, `FloorDiv`
- ✅ Bitwise: `BitOr`, `BitAnd`, `BitXor`, `BitNot`, `Shl`, `Shr`
- ✅ Comparison/logic: `Eq`, `Lt`, `Le`, `Not`, `And`, `Or`, `Test`
- ✅ Control flow: `Jmp`, `Call`, `TailCall`, `Return`, `Halt`, `Loop`, `ForPrep`, `ForLoop`, `ForIn`, `Break`, `Continue`
- ✅ Register ops: `Nop`, `LoadK`, `LoadNil`, `LoadBool`, `LoadInt`, `Move`, `MoveOwn`

### Cranelift JIT — Missing opcodes (32/71)

These opcodes cause graceful fallback to the interpreter. They are primarily:

1. **Heap-allocated collection construction** — `NewList`, `NewMap`, `NewRecord`, `NewUnion`, `NewTuple`, `NewSet`  
   *Path to fix:* Add extern C shims that call into the GC allocator; return pointer as i64.

2. **Collection access/mutation** — `GetField`, `SetField`, `GetIndex`, `SetIndex`, `GetTuple`  
   *Path to fix:* Inline GC-aware read/write barriers.

3. **String operations** — `Concat`  
   *Path to fix:* Extern shim for `lumen_concat`.

4. **Advanced control** — `NullCo`, `In`, `Is`, `Intrinsic`, `Closure`, `GetUpval`, `SetUpval`, `Append`, `IsVariant`, `Unbox`  
   *Path to fix:* Mix of extern shims and inline lowering.

5. **AI-specific / async** — `ToolCall`, `Schema`, `Emit`, `TraceRef`, `Await`, `Spawn`, `Perform`, `HandlePush`, `HandlePop`, `Resume`  
   *Path to fix:* These require coroutine support in Cranelift IR — complex, deferred.

## Action Items

| Priority | Opcode(s) | Effort | Impact |
|----------|-----------|--------|--------|
| 🔴 High | `NewList`, `GetIndex`, `SetIndex`, `Append` | Medium | Enables JIT for nbody/sort benchmarks |
| 🔴 High | `Concat` | Low | Enables JIT for string benchmarks |
| 🔴 High | `Intrinsic` | Medium | Enables JIT for stdlib usage |
| 🟡 Med | `Closure`, `GetUpval`, `SetUpval` | High | Enables JIT for higher-order code |
| 🟡 Med | `NullCo`, `Is`, `In`, `IsVariant`, `Unbox` | Medium | Type-polymorphic patterns |
| 🟢 Low | `ToolCall`, `Await`, `Spawn`, `Perform`, etc. | Very High | Async/effects — deferred |

## Stencil Tier (Planned)

The stencil-and-stitch tier (middle JIT, inspired by CPython 3.13+ specializing adaptive interpreter) is not yet implemented.  
When built, it should sit between the interpreter and Cranelift, providing faster warmup at lower compilation cost.  
Target: cover the same 55 opcodes as Cranelift JIT plus `Intrinsic` and `Concat`.

---

*To re-audit: `grep -rn "OpCode::" rust/lumen-codegen/src/jit.rs | sed 's/.*OpCode::\([A-Za-z]*\).*/\1/' | sort | uniq`*
