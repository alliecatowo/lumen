# Lumen Language Specification

This specification describes the Lumen programming language as implemented in the current
compiler and runtime. It is the ground truth that tests are validated against.

Planned work lives in `docs/research/EXECUTION_TRACKER.md` and `ROADMAP.md`.

## 1. Overview

Lumen is a statically typed programming language for AI-native systems. It compiles to
LIR bytecode and executes on a register-based virtual machine.

**Source formats.** Lumen source may be authored in markdown (`.lm.md`) with fenced
`lumen` code blocks, or as raw source (`.lm`). The compiler extracts and concatenates
all fenced lumen blocks from markdown files.

**Markdown blocks in .lm/.lumen files.** In `.lm` and `.lumen` files, code is the default
mode. Triple-backtick blocks (fenced code blocks using three backticks) are treated as
markdown comments/docstrings, not as executable code. If a markdown block immediately
precedes a declaration (cell, record, enum, etc.), it becomes that declaration's docstring,
which is displayed in LSP hover with rich markdown formatting.

Example (showing a markdown block as a docstring):

```lumen
# A markdown block (triple backticks) containing:
# "Computes the factorial of a non-negative integer."
# appears before the cell declaration below:

cell factorial(n: Int) -> Int
  if n <= 1
    return 1
  end
  return n * factorial(n - 1)
end
```

The markdown block above `factorial` becomes its docstring, shown when hovering over
the cell name in an LSP-enabled editor.

If a markdown block immediately precedes a declaration (cell, record, enum, etc.), it
becomes that declaration's docstring. Docstrings are displayed in LSP hover with rich
markdown formatting, enabling documentation to be embedded alongside code.

**Design philosophy.** Minimal syntax, light on tokens (indentation-based blocks terminated
by `end`), high-level defaults with low-level escape hatches when needed.

**Top-level directives** configure compiler behavior:

```lumen
@strict true
@doc_mode true
@deterministic true
```

- `@strict` (default `true`) — report unresolved symbols and type mismatches.
- `@doc_mode` — relax strict diagnostics for documentation snippets.
- `@deterministic` — reject nondeterministic operations (`uuid`, `timestamp`, unknown externals).

## 2. Lexical Elements

### 2.1 Comments

Line comments begin with `#` and extend to end of line:

```lumen
# This is a comment
let x = 42  # inline comment
```

There are no block comments.

### 2.2 Identifiers

Identifiers start with a letter or underscore, followed by letters, digits, or underscores.
Type names are conventionally `PascalCase`. Variables and functions are `snake_case`.

### 2.3 Keywords

All reserved keywords in the language:

| Category | Keywords |
|---|---|
| Declarations | `record`, `enum`, `cell`, `type`, `const`, `trait`, `impl`, `import`, `pub`, `extern`, `macro` |
| Modifiers | `async`, `mut`, `fn` |
| Control flow | `if`, `else`, `for`, `in`, `while`, `loop`, `match`, `return`, `halt`, `break`, `continue`, `when`, `then` |
| Expressions | `and`, `or`, `not`, `is`, `as`, `try`, `await`, `comptime`, `yield`, `defer` |
| Literals | `true`, `false`, `null`, `ok`, `err` |
| Types | `Int`, `Float`, `String`, `Bool`, `Bytes`, `Json`, `Null`, `list`, `map`, `set`, `tuple`, `result`, `union` |
| AI-native | `use`, `tool`, `grant`, `role`, `schema`, `expect`, `emit`, `step`, `parallel`, `with`, `from` |
| Other | `end`, `where`, `self`, `mod` |

### 2.4 Literals

**Integers.** Decimal integer literals: `0`, `42`, `-17`.

**Floats.** Decimal with fractional or exponent part: `3.14`, `1.0`, `-0.5`.

**Strings.** Double-quoted with escape sequences and interpolation:

```lumen
let plain = "hello world"
let escaped = "line1\nline2"
let name = "Alice"
let greeting = "Hello, {name}!"
let expr = "sum is {1 + 2}"
```

Interpolation uses `{expression}` inside double-quoted strings.

**Raw strings.** Prefixed with `r`, no escape processing or interpolation:

```lumen
let raw = r"no \n escapes here"
```

**Bytes.** Prefixed with `b`, hex-encoded: `b"48656C6C6F"`.

**Booleans.** `true` and `false`.

**Null.** The literal `null`.

### 2.5 Operators

#### Operator Precedence (highest to lowest)

| Precedence | Operators | Associativity | Description |
|---|---|---|---|
| 15 | `.` `()` `[]` `?.` `?[]` `!` `?` | left / postfix | Access, call, index, null-safe, try |
| 14 | `-` `not` `~` | prefix | Negation, logical not, bitwise not |
| 13 | `**` | right | Exponentiation |
| 12 | `*` `/` `//` `%` | left | Multiply, divide, floor-divide, modulo |
| 11 | `+` `-` | left | Add, subtract |
| 10 | `<<` `>>` | left | Bitwise shift |
| 9 | `..` `..=` | left | Range (exclusive, inclusive) |
| 8 | `++` | left | Concatenation |
| 7 | `\|>` `~>` | left | Pipe, compose |
| 7 | `&` | left | Bitwise AND |
| 6 | `^` | left | Bitwise XOR |
| 5 | `==` `!=` `<` `<=` `>` `>=` `in` `is` `as` `\|` | left | Comparison, membership, type ops, bitwise OR |
| 4 | `and` | left | Logical AND |
| 3 | `or` | left | Logical OR |
| 2 | `??` | left | Null coalescing |

#### Compound Assignment Operators

`+=`, `-=`, `*=`, `/=`, `//=`, `%=`, `**=`, `&=`, `|=`, `^=`

## 3. Types

### 3.1 Primitive Types

| Type | Description |
|---|---|
| `Int` | 64-bit signed integer |
| `Float` | 64-bit IEEE 754 float |
| `String` | UTF-8 string (interned or owned) |
| `Bool` | `true` or `false` |
| `Bytes` | Byte sequence |
| `Json` | Opaque JSON value |
| `Null` | The null type with single value `null` |

### 3.2 Collection Types

```lumen
cell collections() -> Int
  let xs: list[Int] = [1, 2, 3]
  let m: map[String, Int] = {"a": 1, "b": 2}
  let s: set[Int] = {10, 20, 30}
  let t: tuple[Int, String, Bool] = (1, "hi", true)
  return length(xs)
end
```

- `list[T]` — ordered, variable-length sequence
- `map[K, V]` — key-value mapping (keys are strings at runtime)
- `set[T]` — unordered collection of unique elements
- `tuple[T1, T2, ...]` — fixed-length, heterogeneous sequence

### 3.3 Result Type

```lumen
cell safe_div(a: Int, b: Int) -> result[Int, String]
  if b == 0
    return err("division by zero")
  end
  return ok(a / b)
end
```

`result[T, E]` is a built-in union with `ok(T)` and `err(E)` variants.

### 3.4 Union Types

```lumen
cell example(v: Int | String) -> String
  if v is Int
    return "integer"
  end
  return "string"
end
```

`A | B | C` declares a union of types.

### 3.5 Optional Type Sugar

`T?` is syntactic sugar for `T | Null`:

```lumen
cell find(name: String) -> Int?
  if name == "alice"
    return 42
  end
  return null
end
```

### 3.6 Function Types

`fn(T1, T2) -> R` denotes a function type, optionally with an effect row:
`fn(String) -> Int / {http}`.

### 3.7 Generic Type Parameters

Type definitions and cells accept generic parameters in square brackets:

```lumen
record Pair[A, B]
  first: A
  second: B
end
```

### 3.8 Type Aliases

```lumen
type UserId = String
type Callback = fn(Int) -> Bool
```

## 4. Declarations

### 4.1 Records

Records are typed structs with named fields:

```lumen
record User
  name: String
  age: Int where age >= 0
  role: String = "viewer"
end
```

Fields support:
- Type annotations (required)
- Default values (`= expr`)
- Constraint clauses (`where expr`) validated at construction

Records may be generic and marked `pub`:

```lumen
pub record Box[T]
  value: T
end
```

Construction uses parenthesized named fields:

```lumen
cell main() -> String
  let u = User(name: "Alice", age: 30)
  return u.name
end
```

### 4.2 Enums

Enums define a closed set of variants, optionally with payloads:

```lumen
enum Shape
  Circle(Float)
  Rect(Float)
  Point
end
```

Enums may be generic and contain methods:

```lumen
enum Option[T]
  Some(T)
  None

  cell is_some(self: Option[T]) -> Bool
    match self
      Some(_) -> return true
      None -> return false
    end
  end
end
```

### 4.3 Cells (Functions)

Cells are the primary function construct:

```lumen
cell add(a: Int, b: Int) -> Int
  return a + b
end
```

Features:
- Parameters with types: `name: Type`
- Default parameter values: `name: Type = default`
- Named arguments at call site: `f(name: value)`
- Optional return type (inferred if omitted)
- Effect row: `-> ReturnType / {effect1, effect2}`
- Modifiers: `pub`, `async`, `extern`
- Generic parameters: `cell swap[T](a: T, b: T) -> tuple[T, T]`
- Where clauses on the cell itself

```lumen
async cell fetch_value() -> Int
  return 1
end
```

```lumen
extern cell malloc(size: Int) -> Int
extern cell free(ptr: Int) -> Null
```

Extern cells have no body; the runtime supplies the implementation.

### 4.4 Constants

```lumen
const MAX_RETRIES: Int = 3

cell main() -> Int
  return MAX_RETRIES
end
```

### 4.5 Traits and Implementations

```lumen
trait Printable
  cell to_display(self: Self) -> String
end

impl Printable for Int
  cell to_display(self: Self) -> String
    return to_string(self)
  end
end
```

Traits define method signatures. `impl` blocks provide implementations for specific types.
Traits may extend other traits (parent traits).

### 4.6 Agent Declarations

Agents group cells and grants into a named entity:

```lumen
agent Assistant
  cell respond(input: String) -> String
    return input
  end
end
```

Agents compile to constructor-backed runtime records with method cells.

### 4.7 Process Declarations

Processes are runtime objects with built-in behavior. Supported kinds:
`pipeline`, `machine`, `memory`, `orchestration`, `guardrail`, `eval`, `pattern`.

```lumen
memory ConversationBuffer
end

machine TicketHandler
end
```

See Section 10 for process runtime semantics.

### 4.8 Effect Declarations

Effects declare typed operation interfaces:

```lumen
effect database
  cell query(sql: String) -> list[Json]
end
```

See Section 9.2 for comprehensive documentation of algebraic effects, including `perform`
statements, `handle` expressions, and effect handlers.

### 4.9 Effect Bindings

`bind effect` explicitly maps an effect to a tool alias:

```lumen
use tool postgres.query as DbQuery
bind effect database.query to DbQuery
```

### 4.10 Handlers

Handlers provide implementations for effect operations. There are two forms:

**Top-level handler declarations** provide default implementations:

```lumen
handler MockDb
  handle database.query(sql: String) -> list[Json]
    return []
  end
end
```

**Handler expressions** (`handle...with...end`) install handlers inline. See Section 9.2
for comprehensive documentation of algebraic effects, including the `handle` expression
syntax, `perform` statements, and `resume` continuations.

### 4.11 Tool Declarations and Grants

`use tool` introduces a tool by qualified name with an alias.
`grant` attaches policy constraints:

```lumen
use tool llm.chat as Chat
grant Chat timeout_ms 30000
grant Chat max_tokens 4096
```

Supported constraint keys: `domain` (URL pattern), `timeout_ms`, `max_tokens`, and
custom keys matched against provider schemas.

### 4.12 Imports

```lumen
import utils: helpers
```

Import forms:
- Named: `import module.path: Name1, Name2`
- Aliased: `import module.path: Name1 as Alias`
- Wildcard: `import module_name: *`

Module resolution searches for `.lm.md` then `.lm` files, converting dot paths to
directory separators. See Section 12.

### 4.13 Macro Declarations

```lumen
macro assert_eq(a, b)
  if a != b
    halt "assertion failed"
  end
end
```

Macros are parsed but have limited compile-time expansion support.

## 5. Statements

### 5.1 Let Bindings

```lumen
cell main() -> Int
  let x = 42
  let y: Float = 3.14
  let mut counter = 0
  counter = counter + 1
  return x + counter
end
```

`let` introduces a binding. `let mut` allows reassignment. Optional type annotation
with `: Type`. Pattern destructuring is supported:

```lumen
cell main() -> Int
  let (a, b) = (1, 2)
  return a + b
end
```

### 5.2 Assignment and Compound Assignment

```lumen
cell main() -> Int
  let mut x = 10
  x = 20
  x += 5
  x -= 2
  x *= 3
  x //= 4
  x %= 7
  return x
end
```

All compound forms: `+=`, `-=`, `*=`, `/=`, `//=`, `%=`, `**=`, `&=`, `|=`, `^=`.

### 5.3 If / Else

```lumen
cell classify(x: Int) -> String
  if x > 0
    return "positive"
  else
    if x < 0
      return "negative"
    else
      return "zero"
    end
  end
end
```

Lumen uses `if`/`else` with nested `if` for chaining. There is no `elif` keyword.

### 5.4 For Loops

```lumen
cell main() -> Int
  let mut sum = 0
  for x in [1, 2, 3, 4, 5]
    sum += x
  end
  return sum
end
```

**Filters.** An optional `if` clause skips non-matching iterations:

```lumen
cell main() -> Int
  let mut sum = 0
  for x in 1..=10 if x % 2 == 0
    sum += x
  end
  return sum
end
```

**Labeled loops.** Use `@label` for `break`/`continue` targeting:

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

### 5.5 While Loops

```lumen
cell main() -> Int
  let mut sum = 0
  let mut i = 0
  while i < 10
    sum += i
    i += 1
  end
  return sum
end
```

### 5.6 Loop (Infinite)

```lumen
cell main() -> Int
  let mut count = 0
  loop
    count += 1
    if count >= 5
      break
    end
  end
  return count
end
```

`loop` runs indefinitely until `break` or `return`.

### 5.7 Break and Continue

`break` exits the innermost loop. `continue` skips to the next iteration. Both accept
an optional label: `break @label`, `continue @label`. `break` may carry a value:
`break value`.

### 5.8 Match

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

Match arms consist of a pattern, `->`, and a body. See Section 7 for all pattern forms.

**Exhaustiveness.** The compiler checks that match on an enum covers all variants.
Missing variants produce `IncompleteMatch` errors. A wildcard `_` or catch-all identifier
makes any match exhaustive. Guard patterns do not contribute to exhaustiveness.

### 5.9 Return and Halt

`return expr` exits the current cell with a value. `halt expr` terminates execution
with an error message:

```lumen
cell checked_div(a: Int, b: Int) -> Int
  if b == 0
    halt("division by zero")
  end
  return a / b
end
```

### 5.10 Emit

`emit expr` outputs a value as a side-effect (for streaming/logging):

```lumen
cell process() -> Int / {emit}
  emit "step 1 complete"
  return 42
end
```

### 5.11 Defer

`defer` schedules code to run when the enclosing scope exits. Multiple defers execute
in LIFO order (last deferred, first executed):

```lumen
cell example() -> String
  let result = "start"
  defer
    print("cleanup 1")
  end
  defer
    print("cleanup 2")
  end
  return result
end
```

In the above, `"cleanup 2"` prints first, then `"cleanup 1"`.

### 5.12 Yield

`yield` produces a value from a generator cell without terminating it:

```lumen
cell fibonacci() -> yield Int
  let mut a = 0
  let mut b = 1
  loop
    yield a
    let temp = b
    b = a + b
    a = temp
  end
end
```

## 6. Expressions

### 6.1 Literals

All literal forms described in Section 2.4 are expressions: integers, floats, strings
(with interpolation), raw strings, bytes, booleans, and null.

### 6.2 Collection Constructors

```lumen
cell main() -> Int
  let xs = [1, 2, 3]           # list
  let m = {"key": "value"}     # map
  let s = {10, 20, 30}         # set
  let t = (1, "hello", true)   # tuple
  return length(xs)
end
```

Record construction: `TypeName(field: value, ...)`.

### 6.3 Binary Operators

Arithmetic: `+`, `-`, `*`, `/`, `//` (floor division), `%`, `**` (power).

Comparison: `==`, `!=`, `<`, `<=`, `>`, `>=`.

Logical: `and`, `or`.

Bitwise: `&` (AND), `|` (OR), `^` (XOR), `<<` (left shift), `>>` (right shift).

Membership: `in` — tests whether a value is contained in a collection.

Concatenation: `++` — concatenates two lists or strings.

See Section 2.5 for the full precedence table.

### 6.4 Unary Operators

- `-expr` — numeric negation
- `not expr` — logical negation
- `~expr` — bitwise NOT

### 6.5 Pipe Operator (`|>`)

The pipe operator passes the left value as the first argument to the right function:

```lumen
cell double(x: Int) -> Int
  return x * 2
end

cell add(a: Int, b: Int) -> Int
  return a + b
end

cell main() -> Int
  let result = 5 |> double() |> add(3)
  return result
end
```

`a |> f()` desugars to `f(a)`. `a |> f(b)` desugars to `f(a, b)`.

### 6.6 Compose Operator (`~>`)

The compose operator creates a new function by chaining two functions. Unlike `|>` which
evaluates eagerly, `~>` produces a closure:

```lumen
cell double(x: Int) -> Int
  return x * 2
end

cell add_one(x: Int) -> Int
  return x + 1
end

cell main() -> Int
  let transform = double ~> add_one
  return transform(5)
end
```

`transform(5)` evaluates as `add_one(double(5))` = `11`.

### 6.7 Null Safety Operators

- `expr?.field` — null-safe field access (returns `null` if `expr` is null)
- `expr?[index]` — null-safe index access
- `expr ?? default` — null coalescing (returns `default` if `expr` is null)
- `expr!` — null assert (unwraps or errors if null)

```lumen
record Box
  value: Int
end

cell main() -> Int
  let b: Box | Null = Box(value: 7)
  return b?.value ?? 0
end
```

### 6.8 Range Expressions

```lumen
cell main() -> Int
  let mut sum = 0
  for i in 1..5
    sum += i
  end
  for j in 1..=5
    sum += j
  end
  return sum
end
```

`start..end` is exclusive. `start..=end` is inclusive.

### 6.9 String Interpolation

```lumen
cell main() -> String
  let name = "Alice"
  let age = 30
  return "Hello, {name}! You are {age} years old."
end
```

Expressions inside `{...}` in double-quoted strings are evaluated and converted to strings.

### 6.10 When Expression

`when` is a multi-branch conditional expression:

```lumen
cell grade(score: Int) -> String
  return when
    score >= 90 -> "A"
    score >= 80 -> "B"
    score >= 70 -> "C"
    _ -> "F"
  end
end
```

Branches are evaluated top-to-bottom. `_` is the default. `when` is an expression and
can appear anywhere a value is expected.

### 6.11 Comptime Expression

`comptime` evaluates an expression at compile time:

```lumen
const MAX = comptime
  1024 * 1024
end
```

The body must be pure and deterministic. The result is embedded as a constant.

### 6.12 Lambda / Closure

```lumen
cell main() -> Int
  let double = fn(x: Int) -> Int => x * 2
  return double(5)
end
```

Lambda syntax: `fn(params) -> ReturnType => expr` for single expressions,
or `fn(params) ... end` for block bodies.

### 6.13 Comprehensions

```lumen
cell main() -> list[Int]
  let doubled = [x * 2 for x in [1, 2, 3, 4, 5] if x > 2]
  return doubled
end
```

List comprehension: `[expr for var in iter if condition]`.
Map comprehension: `{key: val for var in iter}`.
Set comprehension: `{expr for var in iter}`.

### 6.14 Is / As (Type Test and Cast)

```lumen
cell main() -> Bool
  let v: Int | String = 42
  return v is Int
end
```

- `expr is Type` — returns `Bool`, tests runtime type
- `expr as Type` — casts value to target type

### 6.15 If Expression

```lumen
cell main() -> Int
  let x = if true then 1 else 2
  return x
end
```

When used as an expression, `if cond then a else b` produces a value.

### 6.16 Match Expression

`match` can be used in expression position:

```lumen
cell main() -> String
  let x = 42
  let label = match x
    0 -> "zero"
    _ -> "nonzero"
  end
  return label
end
```

### 6.17 Await

`await expr` resolves a future value:

```lumen
async cell fetch() -> Int
  return 42
end

cell main() -> Int
  let f = fetch()
  return await f
end
```

### 6.18 Try Expression

`expr?` (postfix `?`) unwraps a `result`, propagating errors.

### 6.19 Spread

`...expr` spreads an iterable into a collection constructor or function call.

### 6.20 Block Expressions

A block `do ... end` evaluates statements and returns the last expression's value.

## 7. Patterns

Patterns are used in `match` arms, `let` destructuring, and `for` loops.

### 7.1 Pattern Forms

| Pattern | Syntax | Example |
|---|---|---|
| Literal | value | `42`, `"hello"`, `true` |
| Wildcard | `_` | `_` |
| Identifier | name | `x` |
| Variant | `Name(pattern)` | `Some(x)`, `None` |
| Guard | `pattern if expr` | `x if x > 0` |
| Or | `p1 \| p2` | `Red \| Blue` |
| List | `[p1, p2, ...rest]` | `[first, ...tail]` |
| Tuple | `(p1, p2)` | `(a, b)` |
| Record | `Type(field: pat)` | `Point(x: px, y: py)` |
| Type check | `name: Type` | `n: Int` |
| Range | `start..end` / `start..=end` | `1..10` |

### 7.2 Nested Patterns

Patterns nest arbitrarily:

```lumen
enum Result
  Ok(Int)
  Err(String)
end

enum Wrapper
  Some(Result)
  None
end

cell unwrap(w: Wrapper) -> Int
  match w
    Some(Ok(n)) -> return n
    Some(Err(_)) -> return -1
    None -> return 0
  end
end
```

### 7.3 Example: Combined Patterns

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

## 8. Builtin Functions

Lumen provides 76 intrinsic functions compiled directly to VM opcodes. They are always
available without imports.

### 8.1 Core / I/O

| Function | Signature | Description |
|---|---|---|
| `print` | `(Any) -> Null` | Print to stdout with newline |
| `debug` | `(Any) -> Null` | Print debug representation to stderr |
| `to_string` / `string` | `(Any) -> String` | Convert to string |
| `to_int` / `int` | `(Any) -> Int?` | Convert to integer |
| `to_float` / `float` | `(Any) -> Float?` | Convert to float |
| `type_of` | `(Any) -> String` | Runtime type name |
| `clone` | `(Any) -> Any` | Deep copy a value |
| `sizeof` | `(Any) -> Int` | In-memory size in bytes |

### 8.2 String Functions

| Function | Signature | Description |
|---|---|---|
| `length` / `len` | `(String) -> Int` | Character count (Unicode-aware) |
| `upper` | `(String) -> String` | Uppercase |
| `lower` | `(String) -> String` | Lowercase |
| `trim` | `(String) -> String` | Strip leading/trailing whitespace |
| `split` | `(String, String) -> list[String]` | Split by separator |
| `join` | `(list[String], String) -> String` | Join with separator |
| `replace` | `(String, String, String) -> String` | Replace all occurrences |
| `contains` | `(String, String) -> Bool` | Substring test |
| `starts_with` | `(String, String) -> Bool` | Prefix test |
| `ends_with` | `(String, String) -> Bool` | Suffix test |
| `chars` | `(String) -> list[String]` | Split into characters |
| `index_of` | `(String, String) -> Int` | First index of substring, or -1 |
| `slice` | `(String, Int, Int) -> String` | Substring by character indices |
| `pad_left` | `(String, Int) -> String` | Left-pad with spaces |
| `pad_right` | `(String, Int) -> String` | Right-pad with spaces |

### 8.3 Math Functions

| Function | Signature | Description |
|---|---|---|
| `abs` | `(Num) -> Num` | Absolute value |
| `min` | `(Num, Num) -> Num` | Smaller of two values |
| `max` | `(Num, Num) -> Num` | Larger of two values |
| `round` | `(Float) -> Float` | Round to nearest (ties to even) |
| `ceil` | `(Float) -> Float` | Ceiling |
| `floor` | `(Float) -> Float` | Floor |
| `sqrt` | `(Num) -> Float` | Square root |
| `pow` | `(Num, Num) -> Num` | Raise to power |
| `log` | `(Num) -> Float` | Natural logarithm |
| `sin` | `(Num) -> Float` | Sine (radians) |
| `cos` | `(Num) -> Float` | Cosine (radians) |
| `clamp` | `(Num, Num, Num) -> Num` | Clamp to range `[lo, hi]` |

(`Num` means `Int | Float` — both are accepted.)

### 8.4 Collection Functions

| Function | Signature | Description |
|---|---|---|
| `length` / `count` / `size` | `(Collection) -> Int` | Element count |
| `append` | `(list[T], T) -> list[T]` | Add element to end |
| `sort` | `(list[T]) -> list[T]` | Sort in natural order |
| `reverse` | `(list[T]) -> list[T]` | Reverse order |
| `flatten` | `(list[list[T]]) -> list[T]` | Flatten one level |
| `unique` | `(list[T]) -> list[T]` | Remove duplicates (first-occurrence order) |
| `zip` | `(list[T], list[U]) -> list[tuple[T,U]]` | Pair elements |
| `enumerate` | `(list[T]) -> list[tuple[Int,T]]` | Index-element pairs |
| `take` | `(list[T], Int) -> list[T]` | First n elements |
| `drop` | `(list[T], Int) -> list[T]` | Remove first n elements |
| `first` | `(list[T]) -> T?` | First element or null |
| `last` | `(list[T]) -> T?` | Last element or null |
| `is_empty` | `(Collection) -> Bool` | True if empty |
| `contains` | `(Collection, T) -> Bool` | Membership test |
| `slice` | `(list[T], Int, Int) -> list[T]` | Sub-list by index |
| `chunk` | `(list[T], Int) -> list[list[T]]` | Split into fixed-size chunks |
| `window` | `(list[T], Int) -> list[list[T]]` | Sliding windows |
| `range` | `(Int, Int) -> list[Int]` | Generate integer sequence |

### 8.5 Higher-Order Functions

| Function | Signature | Description |
|---|---|---|
| `map` | `(list[T], fn(T)->U) -> list[U]` | Transform each element |
| `filter` | `(list[T], fn(T)->Bool) -> list[T]` | Keep matching elements |
| `reduce` | `(list[T], fn(T,T)->T, T) -> T` | Fold from the left |
| `flat_map` | `(list[T], fn(T)->list[U]) -> list[U]` | Map then flatten |
| `any` | `(list[T], fn(T)->Bool) -> Bool` | True if any match |
| `all` | `(list[T], fn(T)->Bool) -> Bool` | True if all match |
| `find` | `(list[T], fn(T)->Bool) -> T?` | First matching element |
| `position` | `(list[T], fn(T)->Bool) -> Int` | Index of first match, or -1 |
| `group_by` | `(list[T], fn(T)->String) -> map[String,list[T]]` | Group by key |

```lumen
cell main() -> list[Int]
  let doubled = map([1, 2, 3], fn(x: Int) -> Int => x * 2)
  let evens = filter(doubled, fn(x: Int) -> Bool => x % 2 == 0)
  let sum = reduce(evens, fn(a: Int, b: Int) -> Int => a + b, 0)
  return doubled
end
```

### 8.6 Map and Record Functions

| Function | Signature | Description |
|---|---|---|
| `keys` | `(map[K,V]) -> list[K]` | All keys |
| `values` | `(map[K,V]) -> list[V]` | All values |
| `entries` | `(map[K,V]) -> list[tuple[K,V]]` | Key-value tuples |
| `has_key` | `(map[K,V], K) -> Bool` | Key exists |
| `merge` | `(map[K,V], map[K,V]) -> map[K,V]` | Merge maps (right wins) |
| `remove` | `(map[K,V], K) -> map[K,V]` | Remove key |

### 8.7 Set Functions

| Function | Signature | Description |
|---|---|---|
| `to_set` | `(list[T]) -> set[T]` | List to set |
| `add` | `(set[T], T) -> set[T]` | Add element |
| `remove` | `(set[T], T) -> set[T]` | Remove element |

### 8.8 Data Integrity

| Function | Signature | Description |
|---|---|---|
| `hash` | `(String) -> String` | SHA-256 hash (prefixed `"sha256:"`) |
| `diff` | `(Any, Any) -> Any` | Structural diff |
| `patch` | `(Any, Any) -> Any` | Apply diff patches |
| `redact` | `(Any, Any) -> Any` | Redact fields |
| `validate` | `(Any) -> Bool` | Schema validation (stub) |
| `matches` | `(Any) -> Bool` | Truthiness test |
| `trace_ref` | `() -> TraceRef` | Generate trace reference |

## 9. Effects and Tool System

### 9.1 Effect Rows

Cells may declare effects they perform:

```lumen
cell a() -> Int / {emit}
  emit "event"
  return 1
end
```

The resolver infers effects for cells that omit explicit rows. Strict mode reports
inferred-but-undeclared effects. Effect diagnostics include cause hints tracing the source.

### 9.2 Algebraic Effects

Effects are a mechanism for structured side effects. Instead of directly performing I/O or
other side effects, code "performs" an effect operation, and an enclosing handler
intercepts it and decides what to do.

**Declaring Effects.** Effects declare typed operation interfaces:

```lumen
effect Console
  cell log(message: String) -> Null
  cell read_line() -> String
end
```

**Effect Bindings.** `bind effect` explicitly maps an effect to a tool alias:

```lumen
use tool http.get as HttpGet
bind effect http to HttpGet
```

**Performing Effects.** The `perform` statement invokes an effect operation:

```lumen
cell fetch_data(url: String) -> String / {http}
  let response = perform http.get(url)
  return response
end
```

When `perform` is executed, control transfers to the nearest matching handler on the
effect handler stack. If no handler is found, the effect propagates up the call stack.

**Handling Effects.** The `handle...with...end` expression installs an effect handler:

```lumen
cell main() -> String
  let result = handle
    fetch_data("https://example.com")
  with
    http.get(url) =>
      resume("mock response for " + url)
  end
  return result
end
```

The handler syntax is `Effect.operation(params) => body`. The handler body can call
`resume(value)` to continue the suspended computation with the given value. This uses
one-shot delimited continuations — each continuation can only be resumed once.

**Multiple Handlers.** A single `handle` expression can handle multiple effect operations:

```lumen
effect IO
  cell read() -> String
  cell write(msg: String) -> Null
end

cell main() -> Null / {IO}
  let result = handle
    perform IO.write("hello")
    let input = perform IO.read()
    return null
  with
    IO.read() =>
      resume("input")
    IO.write(msg) =>
      resume(null)
  end
  return result
end
```

**Effect Rows on Signatures.** Cells declare which effects they may perform using effect
rows:

```lumen
effect http
  cell get(url: String) -> String
end

cell process(data: String) -> Int / {http, trace}
  # This cell may perform http and trace effects
  emit "processing"
  let resp = perform http.get(data)
  return len(resp)
end
```

**Key Concepts:**
- `perform Effect.operation(args)` — invokes an effect operation, transferring control
  to the nearest matching handler
- `handle body with handlers end` — installs effect handlers that intercept operations
- `resume(value)` — continues the suspended computation with the given value (one-shot
  continuation)
- Effect handlers are stack-based — inner handlers shadow outer ones
- Effect rows `/ {effect1, effect2}` — declare which effects a cell may perform
- Unhandled effects propagate up the call stack

**Handler Stack Semantics.** When an effect is performed:
1. The VM searches up the effect handler stack for a matching handler
2. If found, the continuation is captured (current execution state is saved)
3. Control transfers to the handler code
4. The handler can call `resume(value)` to continue execution with a value, or return
   normally to abort the continuation
5. Each continuation can only be resumed once (one-shot semantics)

**Top-Level Handlers.** Handlers can also be declared at the top level:

```lumen
handler MockDb
  handle database.query(sql: String) -> list[Json]
    return []
  end
end
```

Top-level handlers provide default implementations that can be used when no explicit
handler is installed.

### 9.3 Tool Abstraction

A `use tool` declaration introduces a tool as a typed interface. At the language level,
the compiler validates tool usage without knowing which provider handles the call:

```lumen
use tool llm.chat as Chat
grant Chat timeout_ms 30000
bind effect llm to Chat

cell ask(prompt: String) -> String / {llm}
  return Chat(prompt: prompt)
end
```

### 9.4 Grant Policies

Grants attach constraints to tool aliases. At runtime, `validate_tool_policy()` merges
all grants and checks them before dispatch. Violations produce errors.

Constraint keys: `domain` (URL pattern), `timeout_ms`, `max_tokens`, and custom keys.

### 9.5 Tool Call Semantics

When a tool is called:
1. Look up tool alias in the provider registry
2. Merge and validate grant policies
3. Execute via the provider's `call()` method
4. Record trace event (tool name, input, output, duration, provider)
5. Return result

### 9.6 MCP Server Bridge

MCP servers are exposed as Lumen tool providers. Each tool from an MCP server becomes
available as `server.tool_name` with the `"mcp"` effect kind:

```lumen
use tool github.create_issue as CreateIssue
grant CreateIssue timeout_ms 10000

cell file_bug(title: String, body: String) -> Json / {mcp}
  return CreateIssue(title: title, body: body)
end
```

## 10. Process Runtimes

Process declarations compile to constructor-backed records with typed methods.

### 10.1 Memory

Memory processes provide key-value and entry-based storage:

Methods: `append`, `recent`, `remember`, `recall`, `upsert`, `get`, `query`, `store`.

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

Instances are isolated — `main` returns `0` because `a` and `b` are separate.

### 10.2 Machine

Machine processes model typed state graphs with transitions and guards:

```lumen
machine TicketFSM
  state Open(desc: String)
    guard: true
    transition InProgress(desc)
  end

  state InProgress(desc: String)
    transition Done(desc)
  end

  state Done(desc: String)
    terminal: true
  end

  initial: Open
end
```

Methods: `run`, `start`, `step`, `is_terminal`, `current_state`, `resume_from`.

The resolver validates: unknown initial/transition targets, unreachable states, missing
terminal states, transition argument count and type compatibility, guard boolean type.

### 10.3 Pipeline

Pipeline processes declare ordered stages with type-checked data flow:

Stages auto-chain if no explicit `run` cell is defined. Stage interfaces enforce
single data-argument shape.

### 10.4 Orchestration

Orchestration processes coordinate async work. Built-in patterns:

- `parallel` — run tasks concurrently, collect all results
- `race` — run tasks concurrently, take first result
- `vote` — run tasks concurrently, take majority result
- `select` — run tasks concurrently, pick by criteria
- `timeout` — wrap with time limit

All orchestration builtins use deterministic argument-order semantics.

## 11. Runtime Semantics

### 11.1 Value Types

The VM operates on 15 runtime value types:

| Value | Description |
|---|---|
| `Null` | The null value |
| `Bool` | Boolean |
| `Int` | 64-bit signed integer |
| `Float` | 64-bit float |
| `String` | Interned or owned UTF-8 string |
| `Bytes` | Byte sequence |
| `List` | Ordered value sequence |
| `Tuple` | Fixed-length value sequence |
| `Set` | Unordered unique values |
| `Map` | String-keyed value map |
| `Record` | Named type with field map |
| `Union` | Tagged variant with payload |
| `Closure` | Function with captured environment |
| `TraceRef` | Trace/span reference |
| `Future` | Async computation handle |

### 11.2 Truthiness

| Value | Truthy when |
|---|---|
| `Null` | never |
| `Bool` | `true` |
| `Int` | nonzero |
| `Float` | nonzero |
| `String` | non-empty |
| `List`, `Tuple`, `Set` | non-empty |
| `Map`, `Record`, `Union`, `Closure`, `Future` | always |

### 11.3 Futures and Async

- `spawn` creates futures for callable cells or closures
- Futures have states: `Pending`, `Completed`, `Error`
- `await` resolves futures (recursively through nested collections)
- Deterministic mode (`@deterministic true`) defaults to deferred FIFO scheduling

### 11.4 Deterministic Mode

When `@deterministic true` is set:
- `uuid` / `timestamp` calls are rejected at compile time
- Unknown external tool effects are rejected
- Future scheduling defaults to deferred FIFO

### 11.5 Defer Execution Order

Multiple `defer` blocks execute in LIFO order when the scope exits.

### 11.6 Trace System

Tool calls automatically produce trace events recording: input, output, duration,
provider identity, and status. Use `--trace-dir` to enable trace recording.

## 12. Module System

### 12.1 Import Syntax

```lumen
import utils: helper_fn
import models.user: User, Role
import std.collections: *
import io.file: read_file as read
```

### 12.2 Resolution Rules

The module resolver converts import paths to file paths:
- `import foo` → searches for `foo.lm.md`, then `foo.lm`
- `import foo.bar` → searches for `foo/bar.lm.md`, then `foo/bar.lm`
- Directory modules: checks `mod.lm.md`, `mod.lm`, `main.lm.md`, `main.lm`

Search locations (in order):
1. Source file's directory
2. Project `src/` directory
3. Project root directory

### 12.3 Circular Import Detection

The compiler tracks the import stack. Circular imports produce an error showing the
full chain (e.g., `a → b → c → a`).

### 12.4 Multi-File Compilation

`compile_with_imports(source, imports)` compiles with import resolution.
Imported modules are compiled to LIR and merged via `LirModule::merge()`, which
deduplicates string tables and prevents duplicate definitions.

### 12.5 Package Naming Policy

All packages must be namespaced using the `@namespace/name` format. Bare top-level
package names are not allowed. This ensures clear ownership and prevents naming conflicts:

```lumen
# Valid package names
@lumen/stdlib
@acme/utils
@myorg/analytics

# Invalid: bare top-level names
stdlib  # Error: must be namespaced
utils   # Error: must be namespaced
```

The `@` prefix indicates a package namespace, and the `/` separator distinguishes the
namespace from the package name. This policy applies to all package declarations and
imports.

## 13. Configuration

Runtime configuration is specified in `lumen.toml`, searched in order:
`./lumen.toml`, parent directories, then `~/.config/lumen/lumen.toml`.

### 13.1 Provider Mapping

```toml
[providers]
llm.chat = "openai-compatible"
http.get = "builtin-http"
```

### 13.2 Provider Settings

```toml
[providers.config.openai-compatible]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
default_model = "gpt-4"
```

Secrets use `api_key_env` to reference environment variables — never stored in config.

### 13.3 MCP Servers

```toml
[providers.mcp.github]
uri = "npx -y @modelcontextprotocol/server-github"
tools = ["github.create_issue", "github.search_repos"]
```

The same Lumen source works with different providers by changing only `lumen.toml`.

## 14. Language Server Protocol (LSP)

Lumen provides a full Language Server Protocol implementation for editor integration.
The LSP server supports the following capabilities:

**Hover.** Rich docstrings with markdown formatting are displayed when hovering over
declarations. Docstrings come from markdown blocks that immediately precede declarations
in `.lm` and `.lumen` files.

**Go-to-definition.** Jump to the definition of any symbol (cells, records, enums,
types, etc.) across the codebase, including symbols imported from other modules.

**Completion.** Context-aware code completion suggests:
- Available cells, records, enums, and types
- Field names for record construction
- Enum variants in match expressions
- Import suggestions for unresolved symbols

**Document symbols.** Hierarchical symbol outline showing:
- Cells as Functions
- Records as Structs
- Enums with member children (variants)
- Processes as Classes
- Effects as Interfaces

**Signature help.** Parameter information displayed when calling cells:
- Parameter names and types
- Return types
- Docstrings
- Builtin function signatures

**Semantic tokens.** Syntax highlighting enhanced with semantic information:
- Distinguishes types, functions, variables, and constants
- Marks markdown blocks as comments
- Handles multi-line markdown blocks with delta encoding

**Folding ranges.** Code folding support:
- Region kind for code blocks
- Comment kind for markdown blocks

**Diagnostics.** Real-time type checking and error reporting:
- Type mismatches
- Unresolved symbols
- Effect errors
- Constraint violations
- Exhaustiveness checks for match statements

The LSP server integrates with any editor that supports the Language Server Protocol,
including VS Code, Neovim, Emacs, and others.
