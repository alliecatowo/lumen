# Lumen Specification (Current Implementation)

This specification describes behavior implemented in the current compiler/runtime.
It intentionally excludes planned work that is not fully implemented.

Planned and outstanding work lives in:

- `docs/research/EXECUTION_TRACKER.md`
- `ROADMAP.md`

## 1. Source Model

Lumen source is typically authored in markdown (`.lm.md`), with raw `.lm` files also supported as first-class input.
For markdown files, compiler input may contain fenced Lumen blocks.

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

`bind effect` declarations explicitly associate an effect with a tool alias.
This is the only supported mechanism for mapping effects to tool implementations — the compiler does not infer effect-to-tool bindings heuristically.

```lumen
use tool postgres.query as DbQuery
bind effect database.query to DbQuery
```

## 2.7 Additional Declaration Forms

The parser/resolver supports:

- `use tool` — declares a tool by name; the language treats tools as abstract typed interfaces (see Section 10)
- `grant` — attaches policy constraints to a tool alias; constraints are provider-agnostic (see Section 10.3)
- `type` alias
- `trait`
- `impl`
- `import`
- `const`
- `macro`

```lumen
use tool llm.chat as Chat
grant Chat timeout_ms 30000

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
- optional sugar: `T?` (shorthand for `T | Null`)
- function types: `fn(A, B) -> C`

### Optional Type Sugar (`T?`)

`T?` is syntactic sugar for `T | Null`. It can be used anywhere a type expression is expected:

```lumen
cell find(name: String) -> Int?
  if name == "alice"
    return 42
  end
  return null
end

cell main() -> Int
  let x: Int? = find("alice")
  return x ?? 0
end
```

### Type Test and Cast Expressions

- `expr is Type` — returns `Bool`, testing whether the value is of the given type
- `expr as Type` — casts the value to the target type

```lumen
cell main() -> Bool
  let v: Int | String = 42
  return v is Int
end
```

```lumen
cell main() -> tuple[list[Int], map[String, Int], set[Int], Null]
  return ([1, 2], {"x": 1}, {1, 2}, null)
end
```

## 4. Expressions

Implemented expression families include:

- literals (`Int`, `Float`, `Bool`, `String`, raw strings, bytes, null)
- records, maps, lists, tuples, sets
- unary and binary operators
- **pipe operator** (`|>`) for function chaining
- calls and named args
- tool calls
- field/index access
- lambdas
- comprehensions
- **range expressions** (`..`, `..=`)
- **string interpolation** (`{expr}`)
- null operators (`?.`, `??`, `!`, `?[]`)
- **floor division** (`//`) — integer division truncating toward negative infinity
- **shift operators** (`<<`, `>>`) — bitwise left and right shift (both operands must be `Int`)
- **bitwise operators** (`&`, `|`, `^`, `~`) — AND, OR, XOR, NOT
- **is/as expressions** — `expr is Type` (type test), `expr as Type` (type cast)
- `await`
- orchestration await block forms:
  - `await parallel for ... end`
  - `await parallel ... end`
  - `await race ... end`
  - `await vote ... end`
  - `await select ... end`
- `spawn` builtin for async closure/cell scheduling
- `try` operator for `result`

### Syntactic Sugar

**Pipe operator** `|>` — The value on the left becomes the first argument to the function call on the right:
```lumen
cell double(x: Int) -> Int
  return x * 2
end

cell add(a: Int, b: Int) -> Int
  return a + b
end

cell main() -> Int
  # a |> f(b) desugars to f(a, b)
  let result = 5 |> double() |> add(3)  # add(double(5), 3) = 13
  return result
end
```

**String interpolation** — Embed expressions in strings with `{expr}`:
```lumen
cell main() -> String
  let name = "Alice"
  let age = 30
  return "Hello, {name}! You are {age} years old."
end
```

**Range expressions** — Concise numeric ranges for loops:
```lumen
cell main() -> Int
  let sum = 0
  for i in 1..5      # exclusive: [1, 2, 3, 4]
    sum = sum + i
  end
  for i in 1..=5     # inclusive: [1, 2, 3, 4, 5]
    sum = sum + i
  end
  return sum
end
```

### Null Safety

Null-safe operators propagate `null` without crashing:

- `?.` — null-safe field access: `x?.field` returns `null` if `x` is null
- `?[]` — null-safe index access: `x?[i]` returns `null` if `x` is null
- `??` — null coalescing: `x ?? default` returns `default` if `x` is null
- `!` — null assert: `x!` unwraps or errors if `x` is null

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
- `for` loops (with optional filter)
- `while` loops
- `loop`
- `break` / `continue` (with optional label)
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

### Compound Assignment Operators

All compound assignment forms:

- `+=`, `-=`, `*=`, `/=` — arithmetic
- `//=` — floor division assign
- `%=` — modulo assign
- `**=` — power assign
- `&=`, `|=`, `^=` — bitwise assign

```lumen
cell main() -> Int
  let mut x = 10
  x += 5
  x -= 2
  x *= 3
  x //= 4
  x %= 7
  return x
end
```

### Labeled Loops

Loops (`for`, `while`, `loop`) can be labeled with `@name`. `break` and `continue` can target a label to control nested loops:

```lumen
cell main() -> Int
  let mut count = 0
  for @outer i in 0..3
    for j in 0..3
      if j == 1
        continue @outer
      end
      count += 1
    end
  end
  return count
end
```

### For-Loop Filters

`for` loops support an optional `if` filter clause that skips iterations where the condition is false:

```lumen
cell main() -> Int
  let mut sum = 0
  for x in 1..=10 if x % 2 == 0
    sum += x
  end
  return sum
end
```

### Variadic Parameters (Syntax)

Cell parameters support the `...` prefix syntax for variadic parameters. The parser accepts this syntax and records it in the AST:

```
cell sum(...nums: list[Int]) -> Int
```

Note: Variadic parameter expansion is parsed but not yet fully wired through the type system. The parameter is stored with a `variadic: true` flag in the AST.

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

### Match Exhaustiveness Checking

The compiler checks that `match` statements on enum types cover all variants. If a match is non-exhaustive, the compiler reports an `IncompleteMatch` error listing the missing variants:

```lumen
enum Color
  Red
  Green
  Blue
end

cell describe(c: Color) -> String
  match c
    Red -> return "red"
    Green -> return "green"
    Blue -> return "blue"
  end
end
```

A wildcard (`_`) or catch-all identifier pattern makes any match exhaustive. Guard patterns are treated conservatively and do not contribute to exhaustiveness coverage (since the guard may fail at runtime).

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
- strict mode enforces call-boundary effect compatibility for statically resolved cell/tool calls
- undeclared-effect diagnostics include source-level cause hints (for example, specific call sites/tool calls)
- effectful external capabilities require matching grants in scope
- runtime tool calls apply merged grant-policy constraints (for example `domain`, `timeout_ms`, `max_tokens`) and reject violations

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
- effect bindings (`bind effect ... to <ToolAlias>`) are the explicit mechanism for mapping effects to tools (no heuristic inference)
- grant policies may restrict allowed effects via `effect`/`effects` constraints; resolver enforces these restrictions
- tool calls automatically produce trace events recording input, output, duration, and provider identity

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

Runtime scheduling behavior under deterministic profile:

- compiler lowers directives into module metadata
- VM defaults future scheduling to deferred FIFO when `@deterministic true` is present
- explicit VM scheduler overrides still take precedence

## 8. Runtime Semantics (Current)

## 8.1 Futures

- `spawn` produces `Future` handles.
- `spawn(...)` builtin creates futures for callable cells/closures.
- futures have explicit runtime states: `pending`, `completed`, `error`.
- `await` resolves completed futures, reports failed futures as runtime errors.
- `await` recursively resolves futures nested inside collection/record values.
- VM supports deterministic future scheduling modes (`eager`, `deferred FIFO`) for spawn execution.
- runtime orchestration builtins `parallel`, `race`, `vote`, `select`, and `timeout` are implemented with deterministic argument-order behavior.

## 8.2 Process Runtime Objects

Process declarations compile to constructor-backed records.

For `pipeline` declarations:

- declarative `stages:` chains are parsed and validated against known stage cells
- stage interfaces are type-checked end-to-end (single data-argument pipeline shape)
- compiler generates executable `<Pipeline>.run` semantics from the stage chain when not user-defined

### `memory` runtime methods

- `append`, `recent`, `remember`, `recall`, `upsert`, `get`, `query`, `store`

### `machine` runtime methods

- `run`, `start`, `step`, `is_terminal`, `current_state`, `resume_from`
- machine declarations parse typed state payloads (`state S(x: Int, ...)`), optional guards (`guard: <expr>`), and transition args (`transition T(expr, ...)`)
- resolver validates machine graph consistency and typing (unknown initial/transition targets, unreachable states, missing terminal states, transition arg count/type compatibility, guard bool compatibility)
- VM machine runtime uses compiled graph metadata for deterministic transitions and carries typed payload bindings per state

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

## 10. Tool Providers

Lumen separates tool contracts (declared in source) from tool implementations (loaded at runtime).
The language defines what a tool looks like; the runtime decides how to call it.

### 10.1 Tools Are Abstract

A `use tool` declaration introduces a tool by name.
At the language level, a tool is a typed interface with:

- a qualified name (e.g. `llm.chat`, `http.get`, `github.search_repos`)
- typed input (record of named arguments)
- typed output
- declared effects
- automatic trace events on every call

The compiler validates tool usage (argument types, effect compatibility, grant policy constraints) without knowing which provider will handle the call.

```lumen
use tool llm.chat as Chat
grant Chat timeout_ms 30000
bind effect llm to Chat

cell ask(prompt: String) -> String / {llm}
  return Chat(prompt: prompt)
end
```

This code is valid regardless of whether `llm.chat` is backed by OpenAI, Anthropic, Ollama, or any custom provider.
The provider is determined by runtime configuration (see Section 11).

### 10.2 ToolProvider Interface

At runtime, every tool name resolves to a `ToolProvider` implementation.
The ToolProvider trait exposes:

- `name() -> String` — canonical tool name
- `version() -> String` — provider version string
- `schema() -> ToolSchema` — input/output JSON schemas and declared effect kinds
- `call(input) -> result` — execute the tool with validated input
- `effects() -> list of effect kinds` — effects this provider may produce

`ToolSchema` contains:

- `input_schema` — JSON schema describing accepted input fields
- `output_schema` — JSON schema describing the return value
- `effect_kinds` — set of effect kind strings the tool may trigger (e.g. `"http"`, `"llm"`, `"mcp"`)

Provider implementations live in separate crates (e.g. `lumen-provider-openai`, `lumen-provider-http`).
The runtime loads and registers providers at startup based on configuration.

### 10.3 Grants and Policy Constraints

`grant` declarations attach policy constraints to tool aliases.
Constraints are provider-agnostic — they restrict how any provider may be used.

Supported constraint keys:

- `domain` — URL pattern matching (e.g. `"*.example.com"`)
- `timeout_ms` — maximum execution time in milliseconds
- `max_tokens` — maximum token budget (for LLM providers)
- custom keys — matched exactly against provider schema

```lumen
use tool http.get as Fetch
grant Fetch domain "*.trusted.com"
grant Fetch timeout_ms 5000

use tool llm.chat as Chat
grant Chat max_tokens 4096
grant Chat timeout_ms 30000
```

At runtime, `validate_tool_policy()` merges all grants for a tool alias and checks them before dispatch.
Policy violations produce tool-policy errors.

### 10.4 Effect Bindings

`bind effect` explicitly maps an effect to a tool alias.
This is the only mechanism for associating effects with tool implementations.

```lumen
use tool postgres.query as DbQuery
bind effect database.query to DbQuery
```

The compiler uses these bindings for effect provenance diagnostics (e.g. `UndeclaredEffect` errors include a `cause` field tracing the binding).

### 10.5 Tool Call Semantics

When a tool is called at runtime:

1. The VM looks up the tool alias in the provider registry.
2. Grant policies are merged and validated via `validate_tool_policy()`.
3. The provider's `call()` method executes with the validated input.
4. A trace event is recorded with: tool name, input, output, duration, provider identity, and status.
5. The result is returned to the calling cell.

Tool results can be validated against expected schemas (feature planned).

### 10.6 MCP Server Bridge

MCP (Model Context Protocol) servers are automatically exposed as Lumen tool providers.
An MCP server registered in configuration becomes a set of tools following the `server.tool_name` naming convention.

Semantics:

- Each tool exposed by the MCP server becomes a Lumen tool (e.g. `github.create_issue`, `github.search_repos`)
- MCP tools carry the `"mcp"` effect kind by default
- MCP tool schemas (input/output) are derived from the MCP server's tool descriptions
- MCP tools participate in grant policies and effect bindings like any other tool

```lumen
use tool github.create_issue as CreateIssue
grant CreateIssue timeout_ms 10000
bind effect mcp to CreateIssue

cell file_bug(title: String, body: String) -> Json / {mcp}
  return CreateIssue(title: title, body: body)
end
```

## 11. Configuration

Runtime configuration is specified in `lumen.toml`.
This file maps tool names to provider implementations and supplies provider-specific settings.

### 11.1 Config File Resolution

The runtime searches for `lumen.toml` in the following order:

1. `./lumen.toml` — current working directory
2. Parent directories — walk up the filesystem tree
3. `~/.config/lumen/lumen.toml` — global default

The first file found is used. Files are not merged across locations.

### 11.2 Provider Mapping

The `[providers]` table maps tool names to provider types:

```toml
[providers]
llm.chat = "openai-compatible"
http.get = "builtin-http"
```

Each key is a tool name as used in `use tool` declarations.
Each value is a provider type identifier that the runtime resolves to a `ToolProvider` implementation.

### 11.3 Provider Configuration

Provider-specific settings go under `[providers.config.<provider_type>]`:

```toml
[providers.config.openai-compatible]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
default_model = "gpt-4"

[providers.config.builtin-http]
max_redirects = 5
```

The `api_key_env` field names an environment variable — secrets are never stored directly in config files.

### 11.4 MCP Server Configuration

MCP servers are registered under `[providers.mcp.<server_name>]`:

```toml
[providers.mcp.github]
uri = "npx -y @modelcontextprotocol/server-github"
tools = ["github.create_issue", "github.search_repos"]

[providers.mcp.filesystem]
uri = "npx -y @modelcontextprotocol/server-filesystem /tmp"
tools = ["filesystem.read_file", "filesystem.write_file"]
```

- `uri` — command or URL to launch/connect to the MCP server
- `tools` — list of tool names this server exposes (following `server.tool_name` convention)

### 11.5 Example: Same Code, Different Providers

The same Lumen source works with different providers by changing only `lumen.toml`:

```lumen
use tool llm.chat as Chat
grant Chat timeout_ms 30000
bind effect llm to Chat

cell summarize(text: String) -> String / {llm}
  return Chat(prompt: "Summarize: " + text)
end
```

With OpenAI:
```toml
[providers]
llm.chat = "openai-compatible"

[providers.config.openai-compatible]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
default_model = "gpt-4"
```

With Ollama (local):
```toml
[providers]
llm.chat = "openai-compatible"

[providers.config.openai-compatible]
base_url = "http://localhost:11434/v1"
default_model = "llama3"
```

The Lumen code is identical. Only the configuration changes.

## 12. Boundaries of This Spec

This spec covers implemented behavior only.

Not-yet-complete language areas are intentionally excluded and tracked in `docs/research/EXECUTION_TRACKER.md` / `ROADMAP.md`.

### Nested Pattern Matching

Patterns can be nested arbitrarily deep, allowing complex destructuring in a single match arm:

```lumen
enum Result
  Ok(val: Int)
  Err(msg: String)
end

enum Option
  Some(result: Result)
  None
end

cell unwrap_or_zero(opt: Option) -> Int
  match opt
    Some(Ok(val)) -> val              # 2-level nesting
    Some(Err(msg)) -> 0
    None -> 0
  end
end

# Deeper nesting also works
match deeply_nested
  Wrapper(Some(Ok(value))) -> value   # 3-level nesting
  _ -> default
end

# Nested patterns work with guards and OR patterns
match result
  Some(Ok(n)) | Some(Err(_)) if n > 0 -> "positive success"
  _ -> "other"
end
```

Nested patterns are compiled to efficient sequential checks with early exit on mismatch.
