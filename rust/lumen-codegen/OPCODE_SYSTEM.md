# Unified Opcode Definition System

## Overview

This prototype demonstrates a **single source of truth** approach for defining VM opcodes. Each opcode is defined once, and code generators produce both:

1. **Interpreter dispatch code** (for `lumen-rt/src/vm/mod.rs`)
2. **JIT compilation code** (for `lumen-codegen/src/jit.rs`)

## Problem Statement

Currently, opcodes are defined in THREE places:

1. **`lumen-core/src/lir.rs`** - OpCode enum
2. **`lumen-rt/src/vm/mod.rs`** - Interpreter dispatch (2000+ lines)
3. **`lumen-codegen/src/jit.rs`** - JIT compilation

When adding a new opcode or fixing a bug, you must:
- Define the enum variant
- Write interpreter logic
- Write JIT logic
- Keep semantics synchronized manually

**This is error-prone and causes divergence.**

## Solution

Define opcodes using a declarative system with code generation:

```rust
#[opcode(Add)]
fn op_add(ctx: &mut OpContext, dest: Reg, lhs: Reg, rhs: Reg) -> Result<()> {
    let l = ctx.get_value(lhs)?;
    let r = ctx.get_value(rhs)?;
    let result = BinaryOp::Add.apply(&l, &r)?;
    ctx.set_value(dest, result);
    Ok(())
}
```

This **one definition** generates:

### Interpreter Code (inserted into match statement)

```rust
OpCode::Add => {
    let lhs = &self.registers[base + b];
    let rhs = &self.registers[base + c];
    let result = BinaryOp::Add.apply(lhs, rhs)?;
    self.registers[base + a] = result;
}
```

### JIT Code (Cranelift IR builder)

```rust
fn jit_add(builder: &mut FunctionBuilder, instr: Instruction) -> Result<()> {
    // Type dispatch: Check runtime types and generate specialized paths
    let lhs = builder.use_var(Variable::new(instr.b as usize));
    let rhs = builder.use_var(Variable::new(instr.c as usize));
    
    // Generate type test and branches
    let is_int = test_nan_box_int(builder, lhs);
    // ... etc
}
```

## Architecture

```
┌──────────────────────────────────────────────┐
│ opcode_def.rs                                │
│                                              │
│  • OpcodeDef struct                          │
│  • OpContext trait (abstract execution)     │
│  • OpcodeRegistry (all opcodes)             │
│  • Metadata (format, operands, types)       │
└────────────────┬─────────────────────────────┘
                 │
        ┌────────┴────────┐
        │                 │
        ↓                 ↓
┌───────────────┐  ┌──────────────┐
│  Interpreter  │  │     JIT      │
│  Generator    │  │  Generator   │
└───────────────┘  └──────────────┘
        │                 │
        ↓                 ↓
┌───────────────┐  ┌──────────────┐
│ vm/mod.rs     │  │ jit.rs       │
│ (dispatch)    │  │ (native)     │
└───────────────┘  └──────────────┘
```

## OpContext Trait

The `OpContext` trait abstracts execution context:

```rust
pub trait OpContext {
    fn get_int(&self, reg: Reg) -> Result<i64>;
    fn set_int(&mut self, reg: Reg, val: i64);
    // ... etc
}
```

**Interpreter implementation:**
```rust
impl OpContext for Vm {
    fn get_int(&self, reg: Reg) -> Result<i64> {
        match self.registers[base + reg.0 as usize] {
            Value::Int(v) => Ok(v),
            _ => Err(type_error("expected Int")),
        }
    }
}
```

**JIT implementation:**
```rust
impl OpContext for JitBuilder<'_> {
    fn get_int(&self, reg: Reg) -> Result<i64> {
        // Generate IR to unbox NaN-boxed value
        let var = Variable::new(reg.0 as usize);
        let raw = self.use_var(var);
        let unboxed = self.ins().sshr_imm(raw, 1);
        Ok(unboxed)  // Returns IR Value, not i64
    }
}
```

Note: The JIT version doesn't return actual `i64` — it returns Cranelift `Value` handles. The trait would need refinement to support this, likely via associated types:

```rust
pub trait OpContext {
    type Value;  // Value::Int for interp, cranelift_codegen::ir::Value for JIT
    
    fn get_int(&self, reg: Reg) -> Result<Self::Value>;
    fn set_int(&mut self, reg: Reg, val: Self::Value);
}
```

## Example: Adding a New Opcode

### Before (Manual Sync)

**1. Define enum** (`lir.rs`)
```rust
pub enum OpCode {
    // ...
    Clamp = 0xA0,  // A, B, C: clamp B to range [C.lo, C.hi]
}
```

**2. Write interpreter** (`vm/mod.rs`)
```rust
OpCode::Clamp => {
    let val = self.registers[base + b].as_int()?;
    let range = self.registers[base + c].as_tuple()?;
    let lo = range[0].as_int()?;
    let hi = range[1].as_int()?;
    self.registers[base + a] = Value::Int(val.clamp(lo, hi));
}
```

**3. Write JIT** (`jit.rs`)
```rust
OpCode::Clamp => {
    let val_raw = builder.use_var(Variable::new(b as usize));
    let val = unbox_int(builder, val_raw);
    // ... 20 more lines of IR generation
}
```

### After (Unified Definition)

**One definition:**
```rust
#[opcode(Clamp)]
fn op_clamp(ctx: &mut OpContext, dest: Reg, val: Reg, range: Reg) -> Result<()> {
    let v = ctx.get_int(val)?;
    let tuple = ctx.get_value(range)?;
    let lo = tuple.as_tuple()?[0].as_int()?;
    let hi = tuple.as_tuple()?[1].as_int()?;
    ctx.set_int(dest, v.clamp(lo, hi));
    Ok(())
}
```

Code generators produce both interpreter and JIT implementations automatically.

## Code Generation Strategies

### Strategy 1: Direct Code Emission (Current Prototype)

Each `OpcodeDef` stores a `codegen` function that emits source code as strings:

```rust
pub struct InterpImpl {
    pub codegen: fn(&Instruction) -> String,
}
```

**Pros:**
- Simple
- No macro complexity
- Easy to debug (inspect generated strings)

**Cons:**
- String manipulation is fragile
- No syntax checking at definition time
- Requires post-processing (formatting, insertion into match)

### Strategy 2: Proc Macro with AST Manipulation

Use `#[opcode]` proc macro to parse function bodies and generate code:

```rust
#[proc_macro_attribute]
pub fn opcode(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse function
    let func = parse_macro_input!(item as ItemFn);
    
    // Generate interpreter code
    let interp_impl = generate_interpreter(&func);
    
    // Generate JIT code
    let jit_impl = generate_jit(&func);
    
    // Emit both
    quote! {
        #interp_impl
        #jit_impl
    }.into()
}
```

**Pros:**
- Type-checked at definition time
- No string manipulation
- Can rewrite/optimize AST before emission

**Cons:**
- Complex macro implementation
- Debugging is harder
- Compile times increase

### Strategy 3: Interpreter-First with JIT Inference

Write interpreter code, infer JIT code from interpreter patterns:

```rust
OpCode::Add => {
    // Pattern: get two regs, apply binary op, store result
    let lhs = self.registers[base + b];  // → builder.use_var(b)
    let rhs = self.registers[base + c];  // → builder.use_var(c)
    let result = BinaryOp::Add.apply(lhs, rhs);  // → builder.ins().iadd(lhs, rhs)
    self.registers[base + a] = result;   // → builder.def_var(a, result)
}
```

Pattern recognition generates JIT code:
- `self.registers[...]` → `builder.use_var()`
- `BinaryOp::Add` → `builder.ins().iadd()`
- Assignment → `builder.def_var()`

**Pros:**
- Write once (interpreter), JIT is free
- No DSL to learn
- Works for 80% of opcodes

**Cons:**
- Pattern matching is brittle
- Complex opcodes (loops, branches) need manual JIT
- Heuristics may fail

## Opcode Metadata

Each opcode carries rich metadata for tooling:

```rust
pub struct OpcodeDef {
    pub name: &'static str,           // "Add"
    pub opcode: OpCode,                // OpCode::Add
    pub format: OpcodeFormat,          // ABC, ABx, Ax, etc.
    pub operands: Vec<OperandSpec>,    // Register types
    pub description: &'static str,     // Documentation
    pub interp_impl: InterpImpl,       // Interpreter code
    pub jit_strategy: JitStrategy,     // Direct, TypeDispatch, etc.
}
```

This enables:
- **Auto-generated documentation** from opcode defs
- **Disassembler** (decode bytecode → readable mnemonics)
- **Debugger** (opcode name, register contents in UI)
- **Profiler** (opcode frequency analysis)
- **Optimizer** (pattern-based peephole optimization)

## Testing

Unified definitions enable **differential testing**:

```rust
#[test]
fn test_add_equivalence() {
    let module = compile("cell test() -> Int return 2 + 3 end");
    
    // Run interpreter
    let interp_result = run_interpreter(&module);
    
    // Run JIT
    let jit_result = run_jit(&module);
    
    // Must match
    assert_eq!(interp_result, jit_result);
}
```

Run ALL programs through both backends, assert identical results.

## Roadmap

### Phase 1: Prototype (Current)
- ✅ Define `OpcodeDef` structure
- ✅ Implement `OpcodeRegistry`
- ✅ Create 4 example opcodes (LoadK, Add, Move, LoadInt)
- ✅ Generate interpreter dispatch code (strings)
- ✅ Generate JIT stub (strings)

### Phase 2: Core Opcodes
- [ ] Define all arithmetic opcodes (Add, Sub, Mul, Div, etc.)
- [ ] Define all load/store opcodes (LoadK, Move, GetField, etc.)
- [ ] Generate real interpreter code (replace manual dispatch)
- [ ] Integrate with `lumen-rt/src/vm/mod.rs`

### Phase 3: JIT Integration
- [ ] Implement `OpContext` for Cranelift `FunctionBuilder`
- [ ] Generate actual Cranelift IR (not stubs)
- [ ] Handle type dispatch (Int/Float/String/etc.)
- [ ] Integrate with `lumen-codegen/src/jit.rs`

### Phase 4: Advanced Features
- [ ] Control flow opcodes (Jmp, Call, Return)
- [ ] Effect opcodes (Perform, HandlePush, Resume)
- [ ] Optimization hints (inline, unroll)
- [ ] Profiling instrumentation

### Phase 5: Tooling
- [ ] Auto-generated opcode reference docs
- [ ] Disassembler (`lumen disasm <file>`)
- [ ] Bytecode visualizer
- [ ] Differential fuzzer (interp vs JIT)

## Related Work

### Other VMs with Unified Opcode Defs

**LuaJIT:** Uses C preprocessor macros for opcode definitions. Interpreter and JIT share mnemonics but implementations diverge.

**V8 (TurboFan):** Uses a C++ DSL (`TURBOFAN_BACKEND_OP_LIST`) to define IR operations. Generates instruction selection, register allocation, and codegen.

**WASM runtimes:** Wasmer/Wasmtime use Cranelift's built-in instruction set. No need for custom opcode layer.

**JVM HotSpot:** Template interpreter uses macros to define bytecode semantics. JIT compilers (C1, C2) are hand-written.

## Open Questions

1. **How to handle control flow?** Jumps, loops, branches need block builders in JIT.
   - **Option A:** Mark opcodes as `ControlFlow`, require manual JIT impl
   - **Option B:** Provide CFG builder abstractions in `OpContext`

2. **Type dispatch strategy?** Runtime types aren't known at JIT time.
   - **Option A:** Generate guards/branches (polymorphic inline caches)
   - **Option B:** Specialize on observed types (speculative optimization)
   - **Option C:** Box all values, dispatch at runtime (slow but simple)

3. **Effect handlers?** Perform/Resume need continuation capture.
   - **Option A:** Effects are RuntimeCall strategy (no JIT)
   - **Option B:** Reify continuations as stackless frames (complex)

4. **Macro vs codegen?** Proc macro is powerful but slow. Codegen is fast but fragile.
   - **Decision:** Start with codegen (prototype), move to macro if successful.

5. **Build-time vs runtime generation?** Generate code at build time (build.rs) or runtime (proc macro)?
   - **Build-time:** Faster compiles, harder to debug
   - **Runtime:** Slower compiles, easier to debug

## Conclusion

This prototype demonstrates **feasibility** of a unified opcode definition system for Lumen.

**Benefits:**
- Single source of truth
- Reduced maintenance burden
- Guaranteed semantic equivalence
- Enables differential testing
- Foundation for advanced tooling

**Next steps:**
1. Implement 10-20 core opcodes
2. Integrate with interpreter (replace manual dispatch)
3. Generate simple JIT code (arithmetic only)
4. Measure performance overhead (should be zero)
5. If successful, expand to all opcodes

**Decision point:** After Phase 2, evaluate whether the complexity of code generation is worth the benefits vs. hand-written dispatch.
