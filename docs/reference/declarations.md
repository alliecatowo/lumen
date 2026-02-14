# Declarations

Top-level declarations define the structure and behavior of your program.

## Records

Structured data with typed fields:

```lumen
record User
  name: String
  age: Int
  active: Bool
end
```

### Field Constraints

Add validation with `where`:

```lumen
record Product
  name: String where length(name) > 0
  price: Float where price >= 0.0
  quantity: Int where quantity >= 0
end
```

### Default Values

```lumen
record Config
  host: String = "localhost"
  port: Int = 8080
  debug: Bool = false
end
```

### Generic Records

```lumen
record Box[T]
  value: T
end

record Pair[A, B]
  first: A
  second: B
end
```

### Public Records

```lumen
pub record User
  id: String
  name: String
end
```

## Enums

Sum types with variants:

```lumen
enum Status
  Pending
  Active
  Done
end
```

### Enums with Data

```lumen
enum Result
  Ok(value: Int)
  Err(message: String)
end

enum Option
  Some(value: String)
  None
end
```

### Complex Enums

```lumen
enum Expr
  Number(value: Float)
  String(value: String)
  Variable(name: String)
  BinOp(op: String, left: Expr, right: Expr)
  Call(func: String, args: list[Expr])
end
```

## Cells (Functions)

Functions are called "cells" in Lumen:

```lumen
cell greet(name: String) -> String
  return "Hello, {name}!"
end
```

### Parameters

```lumen
cell add(a: Int, b: Int) -> Int
  return a + b
end
```

Variadic syntax is supported with `...`:

```lumen
cell log(...parts: list[String]) -> Null
  print(parts)
  return null
end
```

The parser records variadic parameters in the AST. Full variadic expansion behavior is still being completed in type/lowering paths.

### Default Parameters

```lumen
cell power(base: Int, exp: Int = 2) -> Int
  return base ** exp
end
```

### Effects

Declare side effects:

```lumen
cell fetch(url: String) -> String / {http}
  return HttpGet(url: url)
end
```

Multiple effects:

```lumen
cell process(url: String) -> String / {http, trace}
  let data = fetch(url)
  emit("processed")
  return data
end
```

### Async Cells

```lumen
async cell fetch_async(url: String) -> String
  return await fetch(url)
end
```

### Public Cells

```lumen
pub cell helper() -> String
  return "I'm exported"
end
```

### Expression Body

Short form for simple functions:

```lumen
cell double(x: Int) -> Int = x * 2
```

## Agents

Encapsulate AI behavior:

```lumen
agent Assistant
  use tool llm.chat as Chat
  grant Chat model "gpt-4o"
  
  cell respond(message: String) -> String / {llm}
    role system: You are a helpful assistant.
    role user: {message}
    return Chat(prompt: message)
  end
end
```

### Agent Usage

```lumen
cell main() -> String / {llm}
  let bot = Assistant()
  return bot.respond("Hello!")
end
```

## Effects

Declare effect interfaces:

```lumen
effect database
  cell query(sql: String) -> list[Json]
  cell execute(sql: String) -> Int
end
```

## Handlers

Implement effects:

```lumen
handler MockDb
  handle database.query(sql: String) -> list[Json]
    return []
  end
  
  handle database.execute(sql: String) -> Int
    return 0
  end
end
```

## Effect Bindings

Map effects to tools:

```lumen
use tool postgres.query as DbQuery
bind effect database.query to DbQuery
```

## Traits

Define shared interfaces:

```lumen
trait Show
  cell show(self) -> String
end
```

## Impls

Implement traits:

```lumen
impl Show for User
  cell show(self) -> String
    return "User({self.name})"
  end
end
```

## Type Aliases

Create type shortcuts:

```lumen
type UserId = String
type Point = tuple[Float, Float]
type Handler = fn(Request) -> Response
```

Generic aliases:

```lumen
type Result[T] = result[T, String]
type Map[V] = map[String, V]
```

## Constants

Compile-time constants:

```lumen
const MAX_SIZE: Int = 1000
const VERSION: String = "1.0.0"
const PI: Float = 3.14159
```

## Imports

Import from other modules:

```lumen
import math: add, subtract
import utils: *
import helpers: format as fmt
```

### Module Resolution

For `import foo: bar`:
1. `foo.lm.md`
2. `foo.lm`
3. `foo/mod.lm.md`
4. `foo/mod.lm`
5. `foo/main.lm.md`
6. `foo/main.lm`

## Use Tool

Declare tool dependencies:

```lumen
use tool llm.chat as Chat
use tool http.get as HttpGet
use tool postgres.query as DbQuery
```

### With Source

```lumen
use tool llm.chat as Chat from "openai"
```

## Grants

Constrain tool usage:

```lumen
grant Chat
  model "gpt-4o"
  max_tokens 1024
  temperature 0.7
  timeout_ms 30000

grant HttpGet
  domain "*.example.com"
  timeout_ms 5000
```

## Processes

### Pipeline

```lumen
pipeline DataProcessor
  stages:
    -> extract
    -> transform
    -> load
  
  cell extract(source: String) -> list[Json]
    # ...
  end
  
  cell transform(data: list[Json]) -> list[Record]
    # ...
  end
  
  cell load(records: list[Record]) -> Int
    # ...
  end
end
```

### Memory

```lumen
memory ConversationBuffer
  # Built-in methods: append, recent, recall, get, query, store
end
```

### Machine

```lumen
machine OrderWorkflow
  initial: Created
  
  state Created(order: Order)
    transition Process(order)
  end
  
  state Process(order: Order)
    transition Shipped(order.tracking)
  end
  
  state Shipped(tracking: String)
    terminal: true
  end
end
```

## Macros

```lumen
macro debug!(expr)
  print("{expr} = {expr}")
end
```

## Summary

| Declaration | Purpose |
|-------------|---------|
| `record` | Structured data |
| `enum` | Sum types |
| `cell` | Functions |
| `agent` | AI agents |
| `effect` | Effect interfaces |
| `handler` | Effect implementations |
| `trait` | Shared interfaces |
| `impl` | Trait implementations |
| `type` | Type aliases |
| `const` | Constants |
| `import` | Module imports |
| `use tool` | Tool declarations |
| `grant` | Tool constraints |
| `pipeline` | Data pipelines |
| `memory` | Stateful storage |
| `machine` | State machines |
| `macro` | Code generation |

## Next Steps

- [Tools](/reference/tools) — Tool system details
- [AI-Native Features](/language/ai-native) — Pipeline, machine, memory, and orchestration reference
