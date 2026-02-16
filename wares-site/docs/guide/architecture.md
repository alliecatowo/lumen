# Architecture Overview

Lumen is built from three main components: the compiler, the VM, and the runtime.

## Compiler Pipeline

The compiler transforms Lumen source into LIR (Lumen Intermediate Representation) bytecode through seven sequential stages:

1. **Markdown extraction** — Pulls code blocks and `@directives` from `.lm.md` and `.lumen` files
2. **Lexing** — Tokenizes concatenated code blocks
3. **Parsing** — Produces `Program` AST (abstract syntax tree)
4. **Resolution** — Builds symbol table, infers effects, evaluates grant policies
5. **Typechecking** — Validates types and patterns
6. **Constraint validation** — Checks field `where` clauses
7. **Lowering** — Converts AST to `LirModule` with bytecode, constants, metadata

## VM (Virtual Machine)

The Lumen VM is a **register-based interpreter** that executes LIR bytecode:

- **32-bit fixed-width instructions** — Lua-style encoding with opcode and register fields
- **~100 opcodes** — Load/move, data construction, field/index access, arithmetic, comparison, control flow, intrinsics, closures, effects
- **Call-frame stack** — Max depth 256
- **Runtime values** — Scalars, collections, records, unions, closures, futures, trace refs

## Runtime

- **Tool dispatch** — Tool calls go through a dispatch trait with optional result caching
- **Trace events** — Recording and replay for debugging
- **Process runtimes** — Memory, machine (state graph), pipeline/orchestration
- **Effect handlers** — One-shot delimited continuations for algebraic effects
