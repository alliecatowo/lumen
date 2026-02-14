# Language Tour

A quick tour of Lumen's syntax and features. This page covers all major language constructs with short examples.

## Cells (Functions)

Functions in Lumen are called **cells**:

```lumen
cell greet(name: String) -> String
  return "Hello, {name}!"
end
```

Cells support default parameters, named arguments, and expression bodies:

```lumen
cell power(base: Int, exp: Int = 2) -> Int
  let mut result = 1
  for _ in 0..exp
    result *= base
  end
  return result
end

cell double(x: Int) -> Int = x * 2
```

## Variables and Mutability

```lumen
let x = 42                # Immutable
let mut counter = 0        # Mutable
counter += 1
```

### Destructuring Let

Unpack values directly in let bindings:

```lumen
let (a, b) = (1, 2)                # Tuple destructuring
let [first, second] = [10, 20]     # List destructuring
let Point(x, y) = my_point         # Record destructuring
```

## Types

### Primitive Types

```lumen
let n: Int = 42
let f: Float = 3.14
let b: Bool = true
let s: String = "hello"
let nothing: Null = null
```

### Optional Type Sugar

`T?` is shorthand for `T | Null`:

```lumen
cell find(items: list[Int], target: Int) -> Int?
  for item in items
    if item == target
      return item
    end
  end
  return null
end
```

This works in parameter types, return types, let bindings, and record fields.

### Union Types

```lumen
let value: Int | String = 42
let maybe: Int | Null = null    # Same as Int?
```

### Collections

```lumen
let numbers: list[Int] = [1, 2, 3]
let scores: map[String, Int] = {"alice": 95, "bob": 87}
let unique: set[Int] = {1, 2, 3}
let pair: tuple[Int, String] = (1, "hello")
```

### Records

Records use parentheses for construction:

```lumen
record Point
  x: Float
  y: Float
end

let p = Point(x: 1.0, y: 2.0)
```

#### Property Shorthand

When variable names match field names, use shorthand:

```lumen
let x = 1.0
let y = 2.0
let p = Point(x, y)    # Same as Point(x: x, y: y)
```

#### Field Constraints

```lumen
record Product
  name: String where length(name) > 0
  price: Float where price >= 0.0
end
```

### Enums

```lumen
enum Color
  Red
  Green
  Blue
end

enum Shape
  Circle(radius: Float)
  Rectangle(width: Float, height: Float)
end
```

### Type Aliases and Generics

```lumen
type UserId = String
type Result[T] = result[T, String]

record Box[T]
  value: T
end
```

## Operators

### Arithmetic

```lumen
let sum = 10 + 5           # Addition
let diff = 10 - 3          # Subtraction
let prod = 4 * 3           # Multiplication
let quot = 10 / 4          # Division
let floor_div = 10 // 3    # Floor division (truncates toward negative infinity)
let rem = 10 % 3           # Modulo
let pow = 2 ** 10          # Exponentiation
```

### Floor Division

`//` performs integer floor division. `//=` is the compound assignment form:

```lumen
let x = 7 // 2       # 3
let mut n = 100
n //= 3              # n is now 33
```

### Bitwise Operators

```lumen
let a = 0xFF & 0x0F     # AND -> 0x0F
let b = 0xF0 | 0x0F     # OR  -> 0xFF
let c = 0xFF ^ 0x0F     # XOR -> 0xF0
let d = ~0xFF           # NOT
```

### Shift Operators

`<<` (left shift) and `>>` (right shift) require `Int` operands:

```lumen
let shifted = 1 << 8     # 256
let halved = 256 >> 1    # 128

let flags = 0
let flags = flags | (1 << 3)   # Set bit 3
let has_bit = (flags >> 3) & 1  # Check bit 3
```

### Compound Assignments

All arithmetic, bitwise, and shift operators have compound forms:

```lumen
let mut x = 10
x += 5       # x = x + 5
x -= 3       # x = x - 3
x *= 2       # x = x * 2
x /= 4       # x = x / 4
x //= 2      # x = floor(x / 2)
x %= 3       # x = x % 3
x **= 2      # x = x ** 2
x &= 0xFF   # x = x & 0xFF
x |= 0x01   # x = x | 0x01
x ^= 0x10   # x = x ^ 0x10
```

### String Concatenation

```lumen
let combined = "Hello" ++ " " ++ "World"
```

### Type Operators

#### `is` (Type Test)

Returns `Bool` -- checks if a value is a given type at runtime:

```lumen
let value: Int | String = 42
if value is Int
  print("It's an integer")
end
```

#### `as` (Type Cast)

Casts a value to a target type:

```lumen
let value: Any = 42
let n = value as Int
```

### Range Operators

```lumen
let exclusive = 1..5      # [1, 2, 3, 4]
let inclusive = 1..=5     # [1, 2, 3, 4, 5]
```

### Pipe Operator

`|>` passes the left value as the first argument to the right function:

```lumen
let result = "data"
  |> process()
  |> format()
  |> validate()
```

### Null-Safe Operators

```lumen
let name = user?.name          # Safe access -- null if user is null
let value = maybe ?? 0         # Null coalescing -- 0 if maybe is null
let certain = maybe!           # Force unwrap -- panics if null
```

#### Null-Safe Index

`?[]` returns `null` if the collection is null instead of panicking:

```lumen
let items: list[Int]? = null
let first = items?[0]          # null (no panic)
```

## Control Flow

### Conditionals

```lumen
if score >= 90
  grade = "A"
else if score >= 80
  grade = "B"
else
  grade = "F"
end
```

Inline if expression:

```lumen
let max = if a > b then a else b
```

### For Loops

```lumen
for item in items
  print(item)
end
```

#### For-Loop Filters

Skip iterations where a condition is false:

```lumen
for x in numbers if x > 0
  print(x)         # Only positive numbers
end

for user in users if user.active
  process(user)
end
```

#### Labeled Loops

Use `@label` for multi-level break/continue:

```lumen
for @outer row in matrix
  for col in row
    if col == target
      break @outer       # Exit both loops
    end
  end
end

while @retry attempts < max_attempts
  if success
    break @retry
  end
  attempts += 1
end

loop @main
  if should_stop()
    break @main
  end
end
```

### Range Loops

```lumen
for i in 1..5
  print(i)          # 1, 2, 3, 4
end

for i in 1..=5
  print(i)          # 1, 2, 3, 4, 5
end
```

### While and Loop

```lumen
while count < 10
  count += 1
end

loop
  if done
    break
  end
end
```

### If Let and While Let

```lumen
if let Some(value) = maybe
  print(value)
end

while let Some(item) = iterator.next()
  process(item)
end
```

## Pattern Matching

### Basic Match

```lumen
match status_code
  200 -> return "OK"
  404 -> return "Not Found"
  _ -> return "Unknown"
end
```

### Destructuring Patterns

```lumen
match point
  Point(x: 0, y: 0) -> "origin"
  Point(x: x, y: 0) -> "on x-axis"
  Point(x: 0, y: y) -> "on y-axis"
  Point(x: x, y: y) -> "({x}, {y})"
end
```

### Guards

```lumen
match n
  x if x < 0 -> "negative"
  0 -> "zero"
  x if x < 100 -> "small"
  _ -> "large"
end
```

### Or Patterns

```lumen
match c
  "a" | "e" | "i" | "o" | "u" -> "vowel"
  _ -> "consonant"
end
```

### Match Exhaustiveness

The compiler checks that match statements on enums cover all variants:

```lumen
enum Direction
  North
  South
  East
  West
end

cell describe(d: Direction) -> String
  match d
    North -> return "up"
    South -> return "down"
    # Compile error: missing East, West
  end
end
```

A wildcard `_` or catch-all identifier pattern makes any match exhaustive. Guard patterns do not contribute to exhaustiveness coverage.

### Match as Expression

```lumen
let label = match status
  Active -> "active"
  Pending -> "pending"
  _ -> "other"
end
```

## Lambdas

```lumen
let double = fn(x: Int) => x * 2
let add = fn(a: Int, b: Int) => a + b

# Multi-line lambda
let complex = fn(x: Int) -> Int
  let doubled = x * 2
  return doubled * doubled
end
```

## Defer

`defer` schedules cleanup code to run when the current scope exits:

```lumen
cell process_file(path: String) -> String
  let handle = open(path)
  defer
    close(handle)
  end

  # handle is closed automatically when scope exits
  return read(handle)
end
```

## Error Handling

### Result Type

```lumen
cell divide(a: Float, b: Float) -> result[Float, String]
  if b == 0.0
    return err("Division by zero")
  end
  return ok(a / b)
end

match divide(10.0, 3.0)
  ok(value) -> print("Result: {value}")
  err(msg) -> print("Error: {msg}")
end
```

### Try Expression

Propagate errors upward:

```lumen
cell safe_compute(a: Float, b: Float) -> result[Float, String]
  let quotient = try divide(a, b)
  return ok(quotient * 2.0)
end
```

## Effects

Declare side effects in function signatures:

```lumen
cell fetch(url: String) -> String / {http}
  return HttpGet(url: url)
end

cell process(url: String) -> String / {http, llm, trace}
  let data = fetch(url)
  let summary = Chat(prompt: "Summarize: {data}")
  emit("processed")
  return summary
end
```

Effects are inferred through call chains and checked at compile time.

## Tools and Grants

```lumen
use tool llm.chat as Chat
use tool http.get as Fetch

grant Chat
  model "gpt-4o"
  max_tokens 1024
  temperature 0.7

grant Fetch
  domain "*.example.com"
  timeout_ms 5000

bind effect llm to Chat
bind effect http to Fetch
```

## Agents

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

cell main() -> String / {llm}
  let bot = Assistant()
  return bot.respond("Hello!")
end
```

## Processes

### Pipeline

```lumen
pipeline TextProcessor
  stages:
    -> read
    -> clean
    -> analyze

  cell read(source: String) -> String
    return file_read(source)
  end

  cell clean(text: String) -> String
    return text.lower().trim()
  end

  cell analyze(text: String) -> map[String, Int]
    # Count word frequencies
  end
end
```

### Memory

```lumen
memory ConversationBuffer

cell main() -> Int
  let buffer = ConversationBuffer()
  buffer.append("Hello")
  buffer.append("Goodbye")
  return length(buffer.recent(10))
end
```

### Machine (State Machine)

```lumen
machine OrderWorkflow
  initial: Created

  state Created(order: Order)
    transition Processing(order)
  end

  state Processing(order: Order)
    guard: order.total > 0
    transition Shipped(order.tracking)
  end

  state Shipped(tracking: String)
    terminal: true
  end
end
```

## Async and Orchestration

```lumen
# Parallel execution
let results = await parallel for url in urls
  fetch(url)
end

# Race for first result
let first = await race
  fetch(url_a)
  fetch(url_b)
end

# Consensus voting
let answer = await vote
  model_a(question)
  model_b(question)
  model_c(question)
end
```

## Imports

```lumen
import math: add, subtract      # Named imports
import utils: *                  # Wildcard import
import helpers: format as fmt   # Aliased import
```

## Directives

```lumen
@strict true              # Enable strict mode (default)
@deterministic true       # Reject nondeterministic operations
```

## Next Steps

- [Basics Tutorial](./tutorial/basics) -- Core syntax in depth
- [AI-Native Tutorial](./ai-native/tools) -- Tools, agents, and workflows
- [Language Reference](../reference/overview) -- Complete specification
