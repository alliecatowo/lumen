# Self-Host Module Index

Module index for the Lumen self-hosted compiler.

## Modules

```lumen
# Core compiler infrastructure
import errors: *
import symbols: *
import abi: *
import serialize: *
import intern: *
```

## Overview

| Module | Purpose |
|--------|---------|
| `errors` | LexError, ParseError, ResolveError, TypecheckError, CompileError |
| `symbols` | SymbolTable, ScopeStack, 14 supporting record types |
| `abi` | Opcode constants, intrinsic IDs, instruction encoding |
| `serialize` | ByteWriter/ByteReader, LIR binary serialization |
| `intern` | StringInterner with intern/resolve/batch operations |
| `main` | Compiler pipeline entry point and phase stubs |
| `hybrid` | Design doc for `--use-lumen-frontend` CLI flag |

## Test Infrastructure

- `tests/diff_test.lm.md` — Differential test harness
- `tests/corpus/trivial/` — 20 minimal programs
- `tests/corpus/expressions/` — 30 expression tests
- `tests/corpus/statements/` — 20 statement tests
- `tests/corpus/patterns/` — 15 pattern matching tests
- `tests/corpus/items/` — 20 item definition tests
- `tests/corpus/complex/` — 10 multi-feature programs
