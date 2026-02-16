# Expressions

Expressions produce values. This page covers all expression forms in Lumen.

## Literals

### Numbers

```lumen
42              # Int
-17             # Negative Int
3.14            # Float
-0.5            # Negative Float
1e10            # Scientific notation Float
1_000_000       # Underscore separators
```

### Booleans

```lumen
true
false
```

### Strings

```lumen
"hello"                    # Basic string
"Hello, {name}!"          # Interpolation
"""Multi
   line
   string"""              # Multi-line
r"C:\path\to\file"        # Raw string
```

### Null and Bytes

```lumen
null              # Null literal
b"cafe"           # Bytes literal (hex)
```

## Collections

### Lists

```lumen
[1, 2, 3]                    # List literal
[]                           # Empty list
[[1, 2], [3, 4]]            # Nested
[x for x in 1..5]           # Comprehension
[x * 2 for x in items]      # Transform comprehension
[x for x in items if x > 0] # Filtered comprehension
```

### Maps

```lumen
{"a": 1, "b": 2}             # Map literal
{}                           # Empty map
```

### Sets

```lumen
{1, 2, 3}                    # Set literal
set[]()                      # Empty set
```

### Tuples

```lumen
(1, "hello")                 # Tuple
(1, 2, 3, 4)                 # Longer tuple
```

### Records

```lumen
Point(x: 1, y: 2)            # Record construction
User(name: "Alice", age: 30)
Point(x, y)                  # Property shorthand (x: x, y: y)
```

## Operators

### Arithmetic

| Operator | Description | Example |
|----------|-------------|---------|
| `+` | Addition | `a + b` |
| `-` | Subtraction | `a - b` |
| `*` | Multiplication | `a * b` |
| `/` | Division | `a / b` |
| `//` | Floor division | `a // b` |
| `%` | Modulo | `a % b` |
| `**` | Exponentiation | `a ** b` |

### Comparison

| Operator | Description | Example |
|----------|-------------|---------|
| `==` | Equal | `a == b` |
| `!=` | Not equal | `a != b` |
| `<` | Less than | `a < b` |
| `<=` | Less or equal | `a <= b` |
| `>` | Greater than | `a > b` |
| `>=` | Greater or equal | `a >= b` |

### Logical

| Operator | Description | Example |
|----------|-------------|---------|
| `and` | Logical AND | `a and b` |
| `or` | Logical OR | `a or b` |
| `not` | Logical NOT | `not a` |

### Bitwise

| Operator | Description | Example |
|----------|-------------|---------|
| `&` | Bitwise AND | `a & b` |
| `\|` | Bitwise OR | `a \| b` |
| `^` | Bitwise XOR | `a ^ b` |
| `~` | Bitwise NOT | `~a` |
| `<<` | Left shift | `a << 2` |
| `>>` | Right shift | `a >> 2` |

Shift operators require `Int` operands.

### String/Collection

| Operator | Description | Example |
|----------|-------------|---------|
| `++` | Concatenation | `a ++ b` |

### Range

| Operator | Description | Example |
|----------|-------------|---------|
| `..` | Exclusive range | `1..5` → [1,2,3,4] |
| `..=` | Inclusive range | `1..=5` → [1,2,3,4,5] |

### Type Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `is` | Runtime type test | `value is Int` |
| `as` | Type cast | `value as Int` |

## Null-Safe Operators

### Safe Access `?.`

```lumen
let maybe: User | Null = get_user()
let name = maybe?.name  # name is String | Null
```

### Null Coalescing `??`

```lumen
let value: Int | Null = get_value()
let result = value ?? 0  # Use 0 if null
```

### Force Unwrap `!`

```lumen
let value: Int | Null = 42
let certain = value!  # Panics if null; use carefully
```

## Pipe Operator

Forward value to function:

```lumen
5 |> double()          # double(5)
5 |> double() |> add(3) # add(double(5), 3)

"data" |> process() |> format()
```

## Compose Operator

Create a new function by composing two functions with `~>`. Unlike `|>` which eagerly evaluates, `~>` is lazy — it returns a closure that applies the left function then the right:

```lumen
let transform = double ~> add_one
transform(5)   # add_one(double(5)) → 11

let pipeline = parse ~> validate ~> normalize
let result = pipeline(input)
```

The compose operator creates a new callable. No computation happens until the composed function is invoked:

```lumen
let process = trim ~> lower ~> split(",")
let parts = process("  A, B, C  ")   # ["a", " b", " c"]
```

## Function Calls

```lumen
func()                         # No arguments
func(1, 2, 3)                  # Positional
func(a: 1, b: 2)               # Named
func(1, b: 2)                  # Mixed
result |> func()               # Pipe
```

## Field Access

```lumen
user.name                      # Field access
users[0].name                  # Chained
map["key"]                     # Map access
list[0]                        # List access
```

## Lambdas

Single expression:

```lumen
fn(x) => x * 2
fn(a, b) => a + b
```

Multi-line:

```lumen
fn(x: Int) -> Int
  let doubled = x * 2
  return doubled
end
```

With type annotations:

```lumen
fn(x: Int, y: Int) -> Int => x + y
```

## Control Flow Expressions

### If Expression

```lumen
let max = if a > b then a else b
let sign = if x >= 0 then "positive" else "negative"
```

### Match Expression

```lumen
let label = match status
  Active -> "active"
  Pending -> "pending"
  _ -> "other"
end
```

### When Expression

Multi-branch conditional expression. Each arm has a condition and a result, evaluated top-to-bottom. Use `_` for the default arm:

```lumen
let grade = when
  score >= 90 -> "A"
  score >= 80 -> "B"
  score >= 70 -> "C"
  score >= 60 -> "D"
  _ -> "F"
end
```

Unlike chained `if`/`else if`, `when` is an expression that returns a value. All arms must produce the same type:

```lumen
let category = when
  age < 13 -> "child"
  age < 18 -> "teenager"
  age < 65 -> "adult"
  _ -> "senior"
end
```

### Comptime Expression

Evaluate an expression at compile time. The body must be a constant expression:

```lumen
let table = comptime build_lookup(256) end
let mask = comptime (1 << 16) - 1 end
```

`comptime` is useful for precomputing lookup tables, bitmasks, or other values that don't change at runtime:

```lumen
let primes = comptime sieve(1000) end
```

## Await

Wait for async operations:

```lumen
let result = await fetch_data()
```

### Parallel

```lumen
await parallel for item in items
  process(item)
end
```

### Race

```lumen
await race
  fetch_from_a()
  fetch_from_b()
end
```

### Vote

```lumen
await vote
  model_a(prompt)
  model_b(prompt)
  model_c(prompt)
end
```

## Spawn

Create futures:

```lumen
let future = spawn(fetch_data())
let results = spawn([
  task_a(),
  task_b()
])
```

## Try Expression

Propagate errors:

```lumen
let value = try parse_int(input)
let result = try divide(a, b)
```

## Precedence

From lowest to highest:

| Precedence | Operators |
|------------|-----------|
| 1 | `\|>` `~>` |
| 2 | `??` |
| 3 | `or` |
| 4 | `and` |
| 5 | `==` `!=` `<` `<=` `>` `>=` `in` `is` `as` `&` `^` `<<` `>>` |
| 6 | `\|` |
| 7 | `++` |
| 8 | `..` `..=` |
| 9 | `+` `-` |
| 10 | `*` `/` `//` `%` |
| 11 | `**` |
| 12 | `-` `not` `~` `!` `...` (unary) |
| 13 | `.` `?.` `[]` `?[]` `()` `?` `!` (postfix) |

Use parentheses for clarity or to override precedence.

## Next Steps

- [Statements](./statements) — Control flow and assignments
- [Pattern Matching](./patterns) — Match expressions
