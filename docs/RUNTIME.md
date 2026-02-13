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
- `spawn(...)` builtin creates futures from callable cell refs/closures (used by orchestration block desugaring).
- Futures have explicit lifecycle states: `pending`, `completed`, `error`.
- Spawned frame completion stores `completed` value; failures store `error`.
- `Await` resolves completed values and raises runtime errors for failed futures.
- `Await` recursively resolves futures nested inside lists/tuples/maps/records.
- Runtime supports configurable future scheduling:
  - `Eager` executes spawned futures immediately.
  - `DeferredFifo` queues spawned futures and runs deterministically when awaited.
- `@deterministic true` is lowered into module metadata; VM load defaults scheduling to `DeferredFifo` from that metadata unless an explicit runtime override is set.
- Orchestration builtins (`parallel`, `race`, `vote`, `select`, `timeout`) execute with deterministic argument-order semantics and integrate with future state tracking.

## Process Runtime Objects

Process-family declarations compile to constructor-backed records with callable methods.

Current runtime support:

- `memory` methods:
  - `append`, `recent`, `remember`, `recall`, `upsert`, `get`, `query`, `store`
- `machine` methods:
  - `run`, `start`, `step`, `is_terminal`, `current_state`, `resume_from`
- machine graph metadata is lowered via addons (`machine.initial`, `machine.state`) and drives deterministic runtime state transitions.

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
