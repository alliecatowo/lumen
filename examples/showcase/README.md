# Lumen Showcase (Multi-File)

This showcase is a runnable multi-file example using imports.

## Files

- `main.lm.md`: runnable board summary app
- `models.lm.md`: shared records and type aliases
- `utils.lm.md`: shared utility types plus local helper cells
- `minimal.lm.md`: smallest runnable import example
- `lumen.toml`: package metadata

## Import Pattern Used

The current showcase uses imports for shared types:

- `main.lm.md` imports `Task`, `Board`, `BoardStats`, `RenderOptions`, and `DisplayLine`
- `minimal.lm.md` imports `Task`

This works with the current compiler/runtime for runnable entrypoints.

## Commands

From repo root:

```bash
./target/debug/lumen check examples/showcase/main.lm.md
./target/debug/lumen run examples/showcase/main.lm.md
./target/debug/lumen check examples/showcase/minimal.lm.md
./target/debug/lumen run examples/showcase/minimal.lm.md
./target/debug/lumen check examples/showcase/utils.lm.md
./target/debug/lumen run examples/showcase/utils.lm.md
./target/debug/lumen pkg check
```

For `pkg check`, run the command in `examples/showcase/`.

## Current Runtime Scope

Imported shared types are supported and used here.

Imported cell calls across files are still limited at runtime (`undefined cell`), so executable flows keep runtime behavior inside the same file while sharing data models through imports.
