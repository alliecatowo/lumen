---
name: lumen-syntax
description: Complete Lumen language syntax reference - cells, records, enums, operators, patterns, effects, processes, and all syntactic sugar
---

# Lumen Language Syntax Reference

## Declarations

### Cells (Functions)
```lumen
cell name(param: Type, opt: Type = default) -> ReturnType / {effect1, effect2}
  # body
end

pub async cell fetch() -> String / {http}
  return "data"
end

extern cell malloc(size: Int) -> Int  # no body, runtime-supplied
```

### Records (Structs)
```lumen
record User
  name: String
  age: Int where age >= 0       # constraint clause
  role: String = "viewer"       # default value
end

# Construction uses PARENTHESES, not curly braces:
let u = User(name: "Alice", age: 30)
```

### Enums
```lumen
enum Shape
  Circle(Float)
  Rect(Float)
  Point                         # no payload
end

enum Option[T]                  # generic
  Some(T)
  None
  cell is_some(self: Option[T]) -> Bool  # methods
    match self
      Some(_) -> return true
      None -> return false
    end
  end
end
```

### Effects & Handlers
```lumen
effect Console
  cell log(message: String) -> Null
end

# Performing effects:
perform Console.log("hello")

# Handling effects (one-shot delimited continuations):
handle
  perform Console.log("test")
with
  Console.log(msg) =>
    resume(null)  # continue execution
end
```

### Imports (colon separator, NOT curly braces)
```lumen
import utils: helper_fn
import models.user: User, Role
import std.collections: *
import io.file: read_file as read
```

## Operators (by precedence, highest first)
1. `.` `()` `[]` `?.` `?[]` `!` `?` — access, call, index, null-safe, try
2. `-` `not` `~` — prefix negation, logical not, bitwise not
3. `**` — exponentiation (right-associative)
4. `*` `/` `//` `%` — multiply, divide, floor-divide, modulo
5. `+` `-` — add, subtract
6. `<<` `>>` — bitwise shift
7. `..` `..=` — range (exclusive, inclusive)
8. `++` — concatenation
9. `|>` `~>` — pipe (eager), compose (lazy)
10. `&` — bitwise AND
11. `^` — bitwise XOR
12. `==` `!=` `<` `<=` `>` `>=` `in` `is` `as` `|` — comparison, membership, type ops
13. `and` — logical AND
14. `or` — logical OR
15. `??` — null coalescing

## Key Syntax Rules
- **Comments**: `#` (NOT `//`, that's floor division)
- **Blocks**: Indentation-based, terminated by `end`
- **String interpolation**: `"Hello, {name}!"` with `{expr}` syntax
- **Optional sugar**: `T?` desugars to `T | Null`
- **Pipe**: `5 |> double() |> add(3)` — eager, value as first arg
- **Compose**: `double ~> add_one` — lazy, creates closure
- **Floor division**: `//` (integer division, NOT comments)
- **Set literals**: `{1, 2, 3}` (curly braces); `set[Int]` only in type position
- **Record construction**: `Point(x: 1, y: 2)` (parentheses, NOT curly braces)
- **`result` is a keyword**: `result[T, E]` with `ok(val)` / `err(msg)`
- **Labeled loops**: `for @label i in 0..3` with `break @label` / `continue @label`
- **Defer**: LIFO execution order (last deferred runs first)

## Patterns
| Pattern | Example |
|---------|---------|
| Literal | `42`, `"hello"`, `true` |
| Wildcard | `_` |
| Identifier | `x` |
| Variant | `Some(x)`, `None` |
| Guard | `x if x > 0` |
| Or | `Red \| Blue` |
| List | `[first, ...tail]` |
| Tuple | `(a, b)` |
| Record | `Point(x: px, y: py)` |
| Type check | `n: Int` |
| Range | `1..10` |

## Process Declarations
```lumen
memory Buf end                  # KV store
machine FSM                     # state graph
  state Open(desc: String)
    guard: true
    transition InProgress(desc)
  end
  state Done(desc: String)
    terminal: true
  end
  initial: Open
end
pipeline Pipe end               # auto-chaining stages
```

## Statements
```lumen
# Let bindings with optional destructuring
let x = 42
let (a, b) = (1, 2)
let mut counter = 0

# Conditionals
if x > 0
  return "positive"
else
  if x < 0
    return "negative"
  else
    return "zero"
  end
end

# Loops
for i in 0..10
  if i % 2 == 0
    continue
  end
end

while x < 100
  x += 1
end

# Match with exhaustiveness checking
match shape
  Circle(r) -> return r * r
  Rect(w, h) -> return w * h
  Point -> return 0
end

# Control flow
return value
halt("error message")
break @label
continue @label

# Effects and cleanup
perform Effect.operation(args)
defer cleanup_code() end
yield value

# Null-safe operators
let val = obj?.field ?? default
let item = list?[index]
```
