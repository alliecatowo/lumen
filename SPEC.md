# Lumen Specification (Current Implementation)

This specification describes behavior implemented in the current compiler/runtime.
It intentionally excludes planned work that is not fully implemented.

Planned and outstanding work lives in:

- `tasks.md`
- `ROADMAP.md`

## 1. Source Model

Lumen source is typically authored in markdown.
Compiler input may contain fenced Lumen blocks.

```lumen
cell main() -> Int
  return 42
end
```

Top-level directives are supported.

```lumen
@strict true
@doc_mode false
@deterministic true
```

## 2. Top-Level Declarations

## 2.1 Records

```lumen
record User
  name: String
  age: Int where age >= 0
end
```

Supported field features:

- type annotations
- default values
- `where` constraints

## 2.2 Enums

```lumen
enum Status
  Pending
  Done
  Failed(String)
end
```

## 2.3 Cells

```lumen
cell add(a: Int, b: Int) -> Int
  return a + b
end
```

Supported:

- parameters, defaults, named arguments
- optional return type
- optional effect row
- `async cell` parsing

```lumen
async cell fetch_value() -> Int
  return 1
end
```

## 2.4 Agent Declarations

```lumen
agent Assistant
  cell respond(input: String) -> String
    return input
  end
end
```

Agents compile to constructor-backed runtime records plus method cells.

## 2.5 Process-Family Declarations

Supported process kinds:

- `pipeline`
- `orchestration`
- `machine`
- `memory`
- `guardrail`
- `eval`
- `pattern`

```lumen
memory ConversationBuffer
end

machine TicketHandler
end
```

These declarations compile to constructor-backed runtime records.
`memory` and `machine` have implemented runtime method behavior (see runtime sections below).

## 2.6 Effects and Handlers

```lumen
effect database
  cell query(sql: String) -> list[Json]
end

handler MockDb
  handle database.query(sql: String) -> list[Json]
    return []
  end
end
```

`bind effect` declarations are supported.

```lumen
use tool postgres.query as DbQuery
bind effect database.query to DbQuery
```

## 2.7 Additional Declaration Forms

The parser/resolver supports:

- `use tool`
- `grant`
- `type` alias
- `trait`
- `impl`
- `import`
- `const`
- `macro`

```lumen
type UserId = String

const MAX_RETRIES: Int = 3

cell main() -> Int
  return MAX_RETRIES
end
```

## 3. Type System

Built-in scalar types:

- `Int`, `Float`, `Bool`, `String`, `Bytes`, `Json`, `Null`

Composite and functional forms:

- `list[T]`
- `map[K, V]`
- `set[T]`
- `tuple[T1, T2, ...]`
- `result[Ok, Err]`
- unions: `A | B`
- function types: `fn(A, B) -> C`

```lumen
cell main() -> tuple[list[Int], map[String, Int], set[Int], Null]
  return ([1, 2], {"x": 1}, set[1, 2], null)
end
```

## 4. Expressions

Implemented expression families include:

- literals (`Int`, `Float`, `Bool`, `String`, raw strings, bytes, null)
- records, maps, lists, tuples, sets
- unary and binary operators
- calls and named args
- tool calls
- field/index access
- lambdas
- comprehensions
- null operators (`?.`, `??`, `!`)
- `await`
- `try` operator for `result`

```lumen
record Box
  value: Int
end

cell main() -> Int
  let b: Box | Null = Box(value: 7)
  return b?.value ?? 0
end
```

## 5. Statements and Control Flow

Implemented statement families include:

- `let` / assignment / compound assignment
- `if` / `else`
- `for` loops
- `while` loops
- `loop`
- `break` / `continue`
- `match`
- `return`
- `halt`
- `emit`

```lumen
cell main() -> Int
  let mut sum = 0
  for x in [1, 2, 3]
    sum += x
  end
  return sum
end
```

## 6. Pattern Matching

Implemented pattern forms:

- literal patterns
- variant patterns (`ok(v)`, `err(e)`, enum variants)
- wildcard (`_`)
- identifier binding
- guard patterns (`p if cond`)
- OR patterns (`p1 | p2`)
- list destructuring (`[a, b, ...rest]`)
- tuple destructuring (`(a, b)`)
- record destructuring (`Type(field: p, ...)`)
- type-check patterns (`x: Int`)

```lumen
record Point
  x: Int
  y: Int
end

cell main() -> Int
  let v: Int | String = 9
  match (v, Point(x: 3, y: 4), [1, 2, 3])
    (n: Int, Point(x: px, y: py), [a, ...rest]) if n > 0 ->
      return n + px + py + length(rest)
    _ -> return 0
  end
end
```

## 7. Effect Rows, Strictness, and Determinism

## 7.1 Strict Mode

- `@strict` defaults to `true`.
- strict mode reports unresolved symbols/type mismatches as diagnostics.

## 7.2 Doc Mode

- `@doc_mode true` relaxes some strict diagnostics for documentation/spec snippets.

## 7.3 Effect Rows

- cells may declare effect rows (`/ {http, trace}`)
- resolver infers effects for cells that omit explicit rows
- strict mode reports inferred-but-undeclared effects for explicitly declared rows
- effectful external capabilities require matching grants in scope

```lumen
cell a() -> Int / {emit}
  emit("x")
  return 1
end

cell b() -> Int
  return a()
end
```

`b` inherits inferred `emit` effect.

Capability checks use grant scope:

- top-level cells use top-level `grant` declarations
- agent/process methods use top-level grants plus local grants declared in that agent/process
- effect bindings (`bind effect ... to <ToolAlias>`) are preferred for mapping custom/bound effects to tools

## 7.4 Deterministic Profile

`@deterministic` enables nondeterminism checks.

Examples currently treated as nondeterministic:

- `uuid` / `uuid_v4` (`random`)
- `timestamp` (`time`)
- external/unknown tool effects (`external`)

```lumen
@deterministic true

cell main() -> String
  return uuid()
end
```

This is rejected under deterministic mode.

## 8. Runtime Semantics (Current)

## 8.1 Futures

- `spawn` produces `Future` handles.
- `await` resolves completed futures.

## 8.2 Process Runtime Objects

Process declarations compile to constructor-backed records.

### `memory` runtime methods

- `append`, `recent`, `remember`, `recall`, `upsert`, `get`, `query`, `store`

### `machine` runtime methods

- `run`, `start`, `step`, `is_terminal`, `current_state`, `resume_from`

Process runtime state is instance-scoped.
Two constructed objects do not share memory/machine state unless explicitly passed/shared.

```lumen
memory Buf
end

cell main() -> Int
  let a = Buf()
  let b = Buf()
  a.append("x")
  return length(b.recent(10))
end
```

`main` returns `0` because instances are isolated.

## 9. Tooling Interface

CLI commands currently implemented:

- `lumen check <file>`
- `lumen run <file> [--cell main] [--trace-dir ...]`
- `lumen emit <file> [--output ...]`
- `lumen trace show <run_id> [--trace-dir ...]`
- `lumen cache clear [--cache-dir ...]`

## 10. Boundaries of This Spec

This spec covers implemented behavior only.

Not-yet-complete language areas are intentionally excluded and tracked in `tasks.md` / `ROADMAP.md`.
