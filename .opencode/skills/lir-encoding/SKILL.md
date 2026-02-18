---
name: lir-encoding
description: Critical reference for LIR 32-bit instruction encoding, signed jumps, and common lowering bugs
---

# LIR Instruction Encoding

## 32-bit Fixed-Width Format (Lua-style)

### Standard format: op(8) | a(8) | b(8) | c(8)
- `op`: 8-bit opcode (OpCode enum)
- `a`: 8-bit destination/first operand register
- `b`: 8-bit second operand register
- `c`: 8-bit third operand register

### Extended format: op(8) | a(8) | Bx(16)
- `Bx`: 16-bit unsigned constant index
- Used for: LoadConst, LoadGlobal, etc.

### Jump format: op(8) | Ax(24) / sAx(24)
- `Ax`: 24-bit unsigned offset (forward jumps only)
- `sAx`: 24-bit SIGNED offset (forward AND backward jumps)

## ⚠️ CRITICAL: Signed Jump Encoding

This is the #1 source of subtle VM bugs.

### CORRECT: For ALL jump instructions (Jmp, Break, Continue):
```rust
Instruction::sax(OpCode::Jmp, offset_i32)  // Encoding
instruction.sax_val() -> i32                // Decoding (sign-extends)
```

### WRONG: NEVER use these for jumps:
```rust
Instruction::ax(OpCode::Jmp, offset_u32)   // WRONG! Unsigned, truncates negatives
instruction.ax_val() -> u32                 // WRONG! No sign extension
```

Backward jumps (loops) produce negative offsets. `ax`/`ax_val` silently truncate
these to 24-bit unsigned values, causing the VM to jump to random forward locations
instead of looping back.

## Common Lowering Bugs

### Match Statement Lowering
1. **Allocate temp register** for `Eq` boolean result — NEVER clobber r0
2. **Always emit `Test`** instruction BEFORE the conditional `Jmp`
3. Each pattern branch needs its own jump target
4. Exhaustiveness checked at typecheck time, but lowering handles `_` fallthrough

### Type::Any Propagation
- Builtin functions return `Type::Any`
- In BinOp type inference, check for `Type::Any` BEFORE falling through to type-specific branches
- Missing this check → spurious type errors on valid code

### Register Allocation (`compiler/regalloc.rs`)
- Simple allocator with temporary recycling
- Up to 65,536 registers per cell
- Off-by-one errors corrupt everything downstream
