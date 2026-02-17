# Hybrid Compiler CLI Flag

Design document for the `--use-lumen-frontend` flag that selects between
the Rust and self-hosted front-ends at runtime.

## Overview

During the bootstrap period both compilers coexist. The Rust CLI gains a
`--use-lumen-frontend` flag (and a `LUMEN_FRONTEND=lumen` env var) that
routes compilation through the self-hosted front-end instead of the Rust
one. The VM remains the Rust VM in both cases — only the front-end
(extraction, lexing, parsing, resolution, typechecking, lowering) is
swapped.

## CLI Interface

```
lumen check <file> --use-lumen-frontend
lumen run <file> --use-lumen-frontend
lumen emit <file> --use-lumen-frontend
```

Alternatively, set `LUMEN_FRONTEND=lumen` in the environment.

## Architecture

```
                        ┌──────────────────┐
   source ──────────────►  Markdown Extract │
                        └────────┬─────────┘
                                 │
                    ┌────────────▼────────────┐
                    │   --use-lumen-frontend?  │
                    └────┬──────────────┬─────┘
                         │              │
                    ┌────▼────┐   ┌─────▼──────┐
                    │  Rust   │   │  Self-Host  │
                    │ Lexer   │   │  Lexer      │
                    │ Parser  │   │  Parser     │
                    │ Resolve │   │  Resolve    │
                    │ TC      │   │  TC         │
                    │ Lower   │   │  Lower      │
                    └────┬────┘   └─────┬──────┘
                         │              │
                    ┌────▼──────────────▼─────┐
                    │   Same LIR Module JSON   │
                    └────────────┬─────────────┘
                                │
                    ┌───────────▼───────────┐
                    │     Rust VM (always)   │
                    └───────────────────────┘
```

## Implementation Plan

The implementation lives in `rust/lumen-cli/src/main.rs`. Changes needed:

### 1. Add CLI flag

```lumen
# Pseudocode for what the Rust CLI changes look like:
#
# In the Clap struct:
#   #[arg(long)]
#   use_lumen_frontend: bool,
#
# Or via environment:
#   std::env::var("LUMEN_FRONTEND") == Ok("lumen")
```

### 2. Dispatch logic

When `--use-lumen-frontend` is active:

1. Read the source file (same as today)
2. Compile `self-host/main.lm.md` using the Rust compiler
3. Run the compiled self-host module on the Rust VM, passing the user's source file as input
4. The self-host compiler produces an LIR module
5. Serialize the LIR module to JSON
6. Deserialize back into the Rust `LirModule` struct
7. Continue with the normal VM execution path

### 3. Output compatibility

Both front-ends MUST produce identical `LirModule` JSON for the same
input. The differential test harness (`self-host/tests/diff_test.lm.md`)
validates this property across the test corpus.

### 4. Gradual migration

Phase 1: Flag is experimental, off by default.
Phase 2: Flag is stable, default remains Rust.
Phase 3: Flag is stable, default flips to Lumen.
Phase 4: Rust front-end removed; `--use-rust-frontend` added as escape hatch.
Phase 5: Escape hatch removed. Self-hosted compiler is the only compiler.

## Error Behavior

Errors from the self-hosted front-end must produce the same diagnostic
format as the Rust front-end. The error types in `self-host/errors.lm.md`
mirror the Rust enums exactly for this reason.

When the self-hosted compiler itself crashes (as opposed to producing a
compile error), the CLI should fall back to the Rust compiler and print a
warning:

```
warning: self-hosted compiler failed internally, falling back to Rust compiler
```

## Testing Strategy

1. Run the full test suite with `--use-lumen-frontend` as a CI matrix axis
2. Differential testing compares LIR output byte-for-byte
3. Any mismatch is a blocking bug — both compilers must agree
