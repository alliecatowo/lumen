# Runtime Model

## VM

Lumen executes LIR on a register VM.

- Call frames hold cell index, base register, instruction pointer, and return target.
- Values are immutable-by-default runtime objects represented by `Value` variants.
- Control flow and expression evaluation are instruction-driven.

## Runtime Values

Core value families:

- Scalars: `Int`, `Float`, `Bool`, `String`, `Null`, `Bytes`
- Collections: `List`, `Map`, `Tuple`, `Set`
- Structured: `Record`, `Union`
- Execution/runtime: `Closure`, `TraceRef`, `Future`

## Futures and Async

- `Spawn` creates a `Future` handle.
- Spawned frame completion stores resolved future value.
- `Await` reads resolved future values.

## Process Runtime Objects

Process-family declarations compile to constructor-backed records with callable methods.

Current runtime support:

- `memory` methods:
  - `append`, `recent`, `remember`, `recall`, `upsert`, `get`, `query`, `store`
- `machine` methods:
  - `run`, `start`, `step`, `is_terminal`, `current_state`, `resume_from`

Process runtime state is instance-scoped, not globally shared by type name.

## Tool Dispatch and Policy

- Tool aliases lower to VM `ToolCall` operations.
- Runtime dispatch builds a structured args object from call arguments.
- Grant policies are merged per tool alias and enforced at dispatch boundaries.
- Current enforced constraint keys include `domain`, `timeout_ms`, `max_tokens`, and exact-match keys.
- Policy violations fail execution with tool-policy errors before dispatcher invocation.

## Deterministic Profile

`@deterministic` enables stricter resolver checks that reject nondeterministic operations/effects.

Examples of inferred nondeterministic signals:

- `uuid`/`uuid_v4` -> `random`
- `timestamp` -> `time`
- unknown external tool calls -> `external`

## Tracing

CLI `run` can emit trace events to `.lumen/trace` via runtime trace store.
