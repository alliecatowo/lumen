# Architecture

## High-Level Components

- `lumen-cli`: user-facing entrypoint.
- `lumen-compiler`: front-end and lowering pipeline.
- `lumen-vm`: runtime for executing LIR.
- `lumen-runtime`: trace and tool runtime utilities.

## Compiler Pipeline

1. Markdown extraction
- Lumen source is typically embedded in markdown code fences.

2. Lexing
- Tokenizes source into the parser token stream.

3. Parsing
- Produces AST (`Program`, `Item`, `Stmt`, `Expr`, `Pattern`).

4. Name resolution
- Builds symbol table for types/cells/tools/agents/process declarations.
- Performs effect inference and strict effect diagnostics.

5. Typechecking
- Checks types and pattern compatibility.
- Strict mode is default; doc mode is explicit.

6. Lowering
- Converts AST to LIR module (cells, instructions, constants, metadata).

## Runtime Architecture

- Register-based VM executes LIR instructions.
- Runtime values include scalar and structured types plus closures, trace refs, and futures.
- Tool calls dispatch through optional runtime tool dispatcher.
- Process declarations (`memory`, `machine`, etc.) lower to constructor-backed runtime objects.

## Testing Strategy

- Unit tests for parser/resolver/typechecker/lowerer/vm.
- Markdown sweep test compiles all Lumen code blocks in `SPEC.md`.
- Runtime tests validate behavior for patterns, process runtimes, and VM operations.
