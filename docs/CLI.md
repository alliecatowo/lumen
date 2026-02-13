# CLI Reference

Binary: `lumen`

## Commands

## `check`

Type-check/compile source.

```bash
lumen check <file>
```

## `run`

Compile and execute a cell (default `main`).

```bash
lumen run <file> [--cell main] [--trace-dir .lumen/trace]
```

## `emit`

Compile and emit LIR JSON.

```bash
lumen emit <file> [--output out.json]
```

## `trace show`

Show pretty-printed trace events for a run.

```bash
lumen trace show <run_id> [--trace-dir .lumen/trace]
```

## `cache clear`

Clear tool result cache.

```bash
lumen cache clear [--cache-dir .lumen/cache]
```
