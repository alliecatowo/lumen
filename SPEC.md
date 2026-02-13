# The Lumen Language Specification

**Version 1.0 — Final**
**Status: Complete Language Definition**

> Lumen is Markdown that executes — with typed schemas, capability-scoped tool calls,
> hash-chained traces, and a register VM — compiled through its own IR for near-instant,
> verifiable, replayable agent workflows.

---

# Part I: Foundations

---

## 1. Notation and Conventions

This specification uses Extended Backus-Naur Form (EBNF) for grammar productions.

```
Production  = name "=" expression ";" .
Terminal    = '"' characters '"' | "'" characters "'" .
Alternation = expression "|" expression .
Grouping    = "(" expression ")" .
Optional    = "[" expression "]" .
Repetition  = "{" expression "}" .
```

**Typographic conventions:**

- `monospace` — keywords, operators, and literal syntax
- *italic* — metavariables and grammar non-terminals
- **bold** — defined terms on first use

Throughout this specification, "must" indicates a requirement. "Should" indicates a recommendation. "May" indicates an option. Programs that violate a "must" are ill-formed and produce a compile error or runtime panic.

### 1.1 Versioning

The language version is declared in every source file:

```lumen
@lumen 1
```

The version number is a single integer. Breaking changes increment the version. The compiler must reject files with unsupported version numbers. The LIR format is versioned separately via semver (`"version": "1.0.0"`).

### 1.2 Unicode

All Lumen source text is UTF-8. Identifiers may contain Unicode letters and digits per UAX #31. String literals may contain any valid UTF-8 sequence. The canonical hashing algorithm operates on raw UTF-8 bytes.

---

## 2. Lexical Structure

### 2.1 Character Classes

```
letter        = unicode_letter | "_" ;
digit         = "0" ... "9" ;
hex_digit     = digit | "a" ... "f" | "A" ... "F" ;
identifier    = letter { letter | digit } ;
```

### 2.2 Keywords

The following identifiers are reserved and may not be used as names:

```
and       as        async     await     bool      break
bytes     cell      const     continue  else      emit
end       enum      expect    false     float     fn
for       from      grant     halt      if        import
in        int       json      let       list      loop
map       match     mod       mut       not       null
or        parallel  pub       record    return    role
schema    self      set       string    trait     true
try       tuple     type      union     use       where
while     with      yield
```

### 2.3 Operators and Punctuation

```
+    -    *    /    %    **
==   !=   <    <=   >    >=
=    +=   -=   *=   /=
->   =>   |>   >>   ..   ...
:    ;    ,    .    @    #
(    )    [    ]    {    }
?    !    &    |    ~
```

### 2.4 Literals

```
int_literal     = digit { digit | "_" }
                | "0x" hex_digit { hex_digit | "_" }
                | "0b" ("0" | "1") { "0" | "1" | "_" }
                | "0o" ("0"..."7") { "0"..."7" | "_" } ;

float_literal   = digit { digit } "." digit { digit } [ exponent ]
                | digit { digit } exponent ;
exponent        = ("e" | "E") ["+" | "-"] digit { digit } ;

string_literal  = '"' { string_char | interpolation } '"'
                | '"""' { any_char | interpolation } '"""'
                | "r\"" { raw_char } "\""
                | "r\"\"\"" { any_char } "\"\"\"" ;

interpolation   = "{" expression "}" ;

bool_literal    = "true" | "false" ;
null_literal    = "null" ;
bytes_literal   = "b\"" { hex_digit hex_digit } "\"" ;
```

**Numeric separators:** Underscores may appear between digits for readability: `1_000_000`, `0xFF_FF`. They carry no semantic meaning.

**Multiline strings:** Triple-quoted strings (`"""..."""`) preserve internal newlines and strip common leading whitespace (dedent). Interpolation works inside them.

**Raw strings:** Prefixed with `r`, disable escape sequences and interpolation: `r"no \n here"`.

### 2.5 Indentation

Lumen is indentation-sensitive. The lexer emits synthetic `INDENT` and `DEDENT` tokens based on leading whitespace changes. Indentation must use spaces (not tabs). Each indentation level must be exactly 2 spaces.

```
INDENT   — emitted when indentation increases
DEDENT   — emitted when indentation decreases
NEWLINE  — emitted at end of logical line
```

Lines ending with `\` continue onto the next line (line continuation). Blank lines and comment-only lines are ignored for indentation purposes.

### 2.6 Comments

```
# Single-line comment (to end of line)

## Doc comment (attached to the next declaration)
## These are extracted by the documentation generator.

#! Module-level doc comment (must appear before any declarations)
```

There are no block comments. This is intentional — block comments nest poorly and create parser ambiguity in Markdown-embedded code.

---

## 3. Source Format

### 3.1 The `.lm.md` File

A Lumen program is a Markdown file with the extension `.lm.md`. The file contains:

1. **Doc-level directives** — lines starting with `@` at the document level
2. **Fenced code blocks** labeled `` ```lumen `` — the executable code
3. **Standard Markdown** — prose, headings, lists, links, images

The compiler extracts all `` ```lumen `` blocks in document order and concatenates them into a single compilation unit. Source locations map back to the original Markdown file for diagnostics.

### 3.2 Directives

Directives configure the compilation unit. They appear at document level (outside code fences) or inside code fences:

```
@lumen <version>              # Required. Language version.
@package "<name>"             # Package identifier (reverse domain).
@trace <algorithm>            # Hash algorithm for traces. Default: sha256.
@cache <on|off>               # Enable/disable tool output caching.
@profile <name>               # Active configuration profile.
@lock <strict|warn|off>       # Lock file enforcement mode.
@entry <cell_name>            # Override default entry point (default: "run").
@wasm                         # Enable WASM compilation target.
@parallel <max_concurrency>   # Default max parallel tool calls.
@timeout <ms>                 # Default tool call timeout.
@feature <flag>               # Enable experimental features.
```

### 3.3 Multi-File Projects

A Lumen **project** is a directory containing a `lumen.toml` manifest and one or more `.lm.md` files.

```toml
# lumen.toml
[package]
name = "acme.invoice_agent"
version = "0.1.0"
edition = 1
entry = "main.lm.md"

[dependencies]
lumen-std = "1.0"
acme-tools = { path = "../shared-tools" }

[tools]
llm-provider = "anthropic"

[profiles.dev]
cache = true
trace = true

[profiles.prod]
cache = true
trace = true
lock = "strict"
```

### 3.4 Module System

Each `.lm.md` file is a **module**. Modules expose declarations via `pub`:

```lumen
## In types.lm.md:
pub record Invoice
  id: String
  total: Float
end

pub cell validate_invoice(inv: Invoice) -> Bool
  return inv.total >= 0.0
end

# Private — not accessible from other modules
cell internal_helper() -> String
  return "helper"
end
```

Import declarations bring names into scope:

```lumen
# Import specific names
import types: Invoice, validate_invoice

# Import all public names
import types: *

# Import with alias
import types: Invoice as Inv

# Import from a dependency package
import acme_tools.http: fetch, post

# Re-export
pub import types: Invoice
```

**Module resolution order:**
1. Local project files (relative to `lumen.toml`)
2. Dependency packages (from `lumen.toml [dependencies]`)
3. Standard library (`lumen-std`)

---

# Part II: Type System

---

## 4. Primitive Types

| Type | Description | Size | Default |
|------|-------------|------|---------|
| `String` | UTF-8 text, immutable, interned | Variable | `""` |
| `Int` | 64-bit signed integer | 8 bytes | `0` |
| `Float` | 64-bit IEEE 754 double | 8 bytes | `0.0` |
| `Bool` | Boolean truth value | 1 byte | `false` |
| `Bytes` | Immutable byte sequence, content-addressed | Variable | `b""` |
| `Json` | Arbitrary JSON value (dynamic) | Variable | `null` |
| `Null` | The absence of a value | 0 bytes | `null` |

### 4.1 String Operations

Strings are immutable, UTF-8, and interned. String comparison is pointer-equality after interning.

```lumen
let s = "hello, world"
let len = s.length            # 12 (byte length)
let chars = s.chars           # 12 (character count, handles Unicode)
let upper = s.upper()         # "HELLO, WORLD"
let lower = s.lower()         # "hello, world"
let trimmed = s.trim()        # removes leading/trailing whitespace
let parts = s.split(", ")    # ["hello", "world"]
let found = s.contains("lo") # true
let idx = s.index_of("lo")   # 3 (or null if not found)
let sub = s.slice(0, 5)      # "hello"
let rep = s.replace("lo", "LO") # "helLO, world"
let starts = s.starts_with("he")  # true
let ends = s.ends_with("ld")      # true
```

**String interpolation** works inside double-quoted strings:

```lumen
let name = "world"
let msg = "hello, {name}"              # simple
let msg = "total: {price * quantity}"  # expression
let msg = "result: {format(val, 2)}"   # function call
```

**Multiline strings** with automatic dedent:

```lumen
let prompt = """
  You are a helpful assistant.
  Your task is to extract invoice data.
  Return only valid JSON.
  """
# Leading common whitespace (2 spaces) is stripped.
```

**Raw strings** disable interpolation and escapes:

```lumen
let pattern = r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$"
```

### 4.2 Numeric Operations

```lumen
let x = 42
let y = 3.14
let hex = 0xFF
let bin = 0b1010
let oct = 0o777
let big = 1_000_000

# Arithmetic
x + y        # Float (auto-promotion)
x / 3        # 14 (integer division)
x / 3.0      # 14.0 (float division)
x % 5        # 2 (modulo)
x ** 3       # 74088 (exponentiation)

# Int-specific
x.abs()      # absolute value
x.clamp(0, 100)  # clamp to range
x.to_float() # explicit conversion

# Float-specific
y.round()    # 3
y.ceil()     # 4
y.floor()    # 3
y.to_int()   # 3 (truncate)
```

### 4.3 Bool Operations

```lumen
let a = true
let b = false
a and b      # false
a or b       # true
not a        # false
```

Short-circuit evaluation: `and` does not evaluate the right operand if the left is `false`. `or` does not evaluate the right if the left is `true`.

### 4.4 Bytes

Bytes are immutable, content-addressed sequences. Values larger than 1KB are stored as blobs in the content-addressed store; the runtime holds only the hash reference.

```lumen
let data = b"48656C6C6F"
let len = data.length          # 5
let slice = data.slice(0, 3)   # first 3 bytes
let hex = data.to_hex()        # "48656C6C6F"
let b64 = data.to_base64()     # "SGVsbG8="
let hash = data.sha256()       # content hash
```

### 4.5 Json

The `Json` type represents dynamically-typed JSON values. It exists as an escape hatch for unstructured data and interop with external systems.

```lumen
let j: Json = json_parse("{\"key\": \"value\"}")
let val = j["key"]              # Json
let s = j["key"].as_string()    # result[String, TypeError]
let typed = j.as_schema(MyRecord)  # result[MyRecord, ValidationError]
```

### 4.6 Null Safety

Lumen has no implicit nulls. A value can only be null if its type explicitly permits it via union:

```lumen
let name: String = "Alice"       # Cannot be null
let nickname: String | Null = null  # Can be null

# Null-coalescing operator
let display = nickname ?? "Anonymous"

# Null-safe member access
let len = nickname?.length  # Int | Null

# Null assertion (panics if null)
let forced = nickname!.length  # Int — halts if nickname is null
```

---

## 5. Compound Types

### 5.1 Records

Records are named product types with typed fields. They are the primary structured data type.

```lumen
record Point
  x: Float
  y: Float
end

record User
  id: String
  name: String
  email: String
  age: Int
  active: Bool = true          # default value
  metadata: map[String, Json] = {}  # default empty
end
```

**Record literals:**

```lumen
let p = Point(x: 1.0, y: 2.0)
let u = User(id: "u1", name: "Alice", email: "a@b.com", age: 30)
```

**Field access:**

```lumen
let name = u.name
let coords = (p.x, p.y)
```

**Functional update (spread):**

```lumen
let p2 = Point(..p, x: 3.0)            # copy p, override x
let u2 = User(..u, active: false)       # deactivate user
```

**Destructuring:**

```lumen
let Point(x:, y:) = p                   # bind x and y
let User(name:, email:, ..) = u         # bind name and email, ignore rest
```

### 5.2 Enums

Enums are named sum types. Variants may carry data or be unit variants.

```lumen
# Simple enum (unit variants only)
enum Color
  Red
  Green
  Blue
end

# Enum with associated data
enum Shape
  Circle(radius: Float)
  Rectangle(width: Float, height: Float)
  Triangle(a: Float, b: Float, c: Float)
  Point    # unit variant (no data)
end

# Enum with methods
enum Direction
  North
  South
  East
  West

  cell opposite(self) -> Direction
    match self
      North -> South
      South -> North
      East  -> West
      West  -> East
    end
  end
end
```

**Construction:**

```lumen
let c = Color.Red
let s = Shape.Circle(radius: 5.0)
let d = Direction.North
```

### 5.3 Union Types

Union types represent a value that could be one of several types:

```lumen
type StringOrInt = String | Int
type Nullable[T] = T | Null
type ApiResponse = SuccessData | ErrorData | Null
```

Unions are narrowed via `match` or type checks:

```lumen
cell process(value: String | Int) -> String
  match value
    v: String -> return "string: {v}"
    v: Int    -> return "int: {v}"
  end
end
```

### 5.4 The Result Type

`result[Ok, Err]` is a built-in tagged union for fallible operations:

```lumen
# Explicitly defined as:
enum result[Ok, Err]
  ok(value: Ok)
  err(error: Err)
end
```

**Usage:**

```lumen
cell divide(a: Float, b: Float) -> result[Float, String]
  if b == 0.0
    return err("division by zero")
  end
  return ok(a / b)
end
```

**The `?` operator** — early-return on error:

```lumen
cell compute(x: Float, y: Float) -> result[Float, String]
  let quotient = divide(x, y)?    # returns err(...) if divide fails
  let root = sqrt(quotient)?      # chains error propagation
  return ok(root)
end
```

The `?` operator desugars to:

```lumen
let quotient = match divide(x, y)
  ok(v) -> v
  err(e) -> return err(e)
end
```

**Result combinators:**

```lumen
let r = divide(10.0, 3.0)
r.map(fn(v) => v * 2)            # result[Float, String]
r.map_err(fn(e) => "Error: {e}") # result[Float, String]
r.unwrap()                        # Float (halts on err)
r.unwrap_or(0.0)                 # Float (default on err)
r.and_then(fn(v) => divide(v, 2.0))  # chain fallible ops
r.is_ok()                        # Bool
r.is_err()                       # Bool
```

---

## 6. Container Types

### 6.1 Lists

Ordered, homogeneous, immutable by default:

```lumen
let nums = [1, 2, 3, 4, 5]
let empty: list[String] = []

# Access
nums[0]            # 1
nums[-1]           # 5 (negative indexing from end)
nums[1..3]         # [2, 3] (slice, exclusive end)
nums[1..=3]        # [2, 3, 4] (slice, inclusive end)
nums[2..]          # [3, 4, 5] (from index to end)
nums[..3]          # [1, 2, 3] (from start to index)

# Properties
nums.length        # 5
nums.is_empty      # false
nums.first         # 1 | Null
nums.last          # 5 | Null

# Transformations (all return new lists)
nums.map(fn(x) => x * 2)                   # [2, 4, 6, 8, 10]
nums.filter(fn(x) => x > 2)                # [3, 4, 5]
nums.reduce(0, fn(acc, x) => acc + x)      # 15
nums.flat_map(fn(x) => [x, x * 10])        # [1, 10, 2, 20, ...]
nums.zip(["a", "b", "c"])                   # [(1,"a"), (2,"b"), (3,"c")]
nums.enumerate()                            # [(0,1), (1,2), (2,3), ...]
nums.take(3)                                # [1, 2, 3]
nums.drop(3)                                # [4, 5]
nums.sort()                                 # [1, 2, 3, 4, 5]
nums.sort_by(fn(a, b) => b - a)            # [5, 4, 3, 2, 1]
nums.reverse()                              # [5, 4, 3, 2, 1]
nums.unique()                               # deduplicate
nums.contains(3)                            # true
nums.find(fn(x) => x > 3)                  # 4 | Null
nums.position(fn(x) => x > 3)              # 3 | Null
nums.any(fn(x) => x > 4)                   # true
nums.all(fn(x) => x > 0)                   # true
nums.group_by(fn(x) => x % 2)              # map[Int, list[Int]]
nums.chunk(2)                               # [[1,2], [3,4], [5]]
nums.window(3)                              # [[1,2,3], [2,3,4], [3,4,5]]
nums.flatten()                              # for list[list[T]] -> list[T]
nums.join(", ")                             # "1, 2, 3, 4, 5" (for list[String])

# Concatenation
[1, 2] + [3, 4]   # [1, 2, 3, 4]
nums ++ [6, 7]     # [1, 2, 3, 4, 5, 6, 7]

# Spread
[0, ..nums, 6]    # [0, 1, 2, 3, 4, 5, 6]
```

### 6.2 Maps

Ordered key-value collections. Keys must be `String` (v1) or any hashable type (v2+):

```lumen
let config = {
  "host": "localhost",
  "port": "8080",
  "debug": "true"
}

let typed: map[String, Int] = {
  "width": 800,
  "height": 600
}

# Access
config["host"]              # "localhost" | Null
config.get("host")          # result[String, KeyError]
config.get_or("host", "")   # "localhost"

# Properties
config.length               # 3
config.is_empty             # false
config.keys                 # ["debug", "host", "port"] (sorted)
config.values               # values in key order
config.entries              # [(key, value)] pairs in key order

# Transformations
config.map_values(fn(v) => v.upper())
config.filter(fn(k, v) => k != "debug")
config.merge(other_map)     # other_map values override
config.without("debug")     # remove key
config.contains_key("host") # true
config.contains_value("8080") # true

# Functional update
let config2 = {..config, "port": "9090"}

# Destructuring
let {"host": host, "port": port, ..rest} = config
```

### 6.3 Sets

Unordered collections of unique values:

```lumen
let tags = set["ai", "agent", "tools"]
let empty: set[Int] = set[]

tags.contains("ai")         # true
tags.length                  # 3

# Set operations
let a = set[1, 2, 3]
let b = set[2, 3, 4]
a.union(b)                   # set[1, 2, 3, 4]
a.intersection(b)            # set[2, 3]
a.difference(b)              # set[1]
a.symmetric_difference(b)    # set[1, 4]
a.is_subset(b)               # false
a.is_superset(b)             # false

# Add/remove (return new sets)
tags.add("new")              # set["ai", "agent", "tools", "new"]
tags.remove("ai")            # set["agent", "tools"]
```

### 6.4 Tuples

Fixed-size, heterogeneous, ordered collections:

```lumen
let pair = (1, "hello")
let triple = (true, 3.14, "world")

# Access by index (compile-time)
pair.0                       # 1
pair.1                       # "hello"
triple.2                     # "world"

# Destructuring
let (x, y) = pair
let (_, pi, _) = triple      # ignore first and third

# Type annotation
let coords: (Float, Float, Float) = (1.0, 2.0, 3.0)
```

Tuples are structurally typed. `(Int, String)` and `(Int, String)` are the same type regardless of where they appear.

---

## 7. Generic Types

Generics allow type-parameterized definitions for records, enums, cells, and traits.

### 7.1 Generic Records

```lumen
record Pair[A, B]
  first: A
  second: B
end

record Stack[T]
  items: list[T]
  size: Int = 0
end

let p = Pair(first: 1, second: "hello")  # Pair[Int, String]
```

### 7.2 Generic Enums

```lumen
enum Option[T]
  Some(value: T)
  None
end

enum Tree[T]
  Leaf(value: T)
  Branch(left: Tree[T], right: Tree[T])
end
```

### 7.3 Generic Cells

```lumen
cell identity[T](value: T) -> T
  return value
end

cell map_pair[A, B, C](pair: Pair[A, B], f: fn(A) -> C) -> Pair[C, B]
  return Pair(first: f(pair.first), second: pair.second)
end

# With trait bounds
cell sort_list[T: Comparable](items: list[T]) -> list[T]
  # T must implement Comparable
  return items.sort()
end

cell transform[T: Serializable + Validatable](item: T) -> Bytes
  return item.serialize()
end
```

### 7.4 Generic Constraints

Type parameters can be constrained via trait bounds:

```lumen
# Single bound
cell max[T: Comparable](a: T, b: T) -> T
  if a > b then a else b
end

# Multiple bounds
cell process[T: Serializable + Hashable](item: T) -> String
  return item.hash()
end

# Where clause (complex constraints)
cell merge[K, V](a: map[K, V], b: map[K, V]) -> map[K, V]
  where K: Hashable + Comparable
  where V: Mergeable
  return a.merge(b)
end
```

---

## 8. Type Aliases

Type aliases create named references to other types:

```lumen
type UserId = String
type Timestamp = Int
type Headers = map[String, String]
type Callback[T] = fn(T) -> result[T, String]
type JsonObject = map[String, Json]

# Constrained alias
type Email = String where matches(self, r"^[^@]+@[^@]+\.[^@]+$")
type Port = Int where self >= 0 and self <= 65535
type NonEmpty[T] = list[T] where length(self) > 0
```

Aliases are fully transparent to the type system — `UserId` and `String` are interchangeable. For opaque wrappers, use a single-field record:

```lumen
record UserId
  value: String where length(value) >= 6
end
```

---

## 9. Trait System

Traits define shared behavior across types. They replace interfaces, protocols, and type classes.

### 9.1 Trait Declaration

```lumen
trait Serializable
  cell serialize(self) -> Bytes
  cell deserialize(data: Bytes) -> result[Self, String]
end

trait Displayable
  cell display(self) -> String
end

trait Hashable
  cell hash(self) -> String
end

trait Comparable
  cell compare(self, other: Self) -> Int  # -1, 0, 1
end

# Trait with default implementations
trait Summarizable
  cell summary(self) -> String

  cell short_summary(self) -> String
    return self.summary().slice(0, 100)
  end
end

# Trait inheritance
trait Printable: Displayable + Serializable
  cell print(self) -> Null
    emit(self.display())
  end
end
```

### 9.2 Trait Implementation

```lumen
record User
  id: String
  name: String
  email: String
end

impl Displayable for User
  cell display(self) -> String
    return "{self.name} <{self.email}>"
  end
end

impl Serializable for User
  cell serialize(self) -> Bytes
    return json_encode(self).to_bytes()
  end

  cell deserialize(data: Bytes) -> result[User, String]
    return json_decode(data.to_string()) |> as_schema(User)
  end
end

impl Hashable for User
  cell hash(self) -> String
    return sha256("{self.id}:{self.email}")
  end
end

# Implement for generic types
impl[T: Displayable] Displayable for list[T]
  cell display(self) -> String
    return "[" + self.map(fn(x) => x.display()).join(", ") + "]"
  end
end
```

### 9.3 Built-in Traits

| Trait | Description | Auto-derived |
|-------|-------------|-------------|
| `Eq` | Equality comparison (`==`, `!=`) | Yes |
| `Comparable` | Ordering (`<`, `<=`, `>`, `>=`) | Yes for primitives |
| `Hashable` | Content-addressed hashing | Yes |
| `Displayable` | Human-readable string output | Yes |
| `Serializable` | Encode/decode to `Bytes` | Yes for records/enums |
| `Cloneable` | Deep copy | Yes |
| `Default` | Default value construction | If all fields have defaults |

**Deriving:** Records and enums can auto-derive traits:

```lumen
@derive(Eq, Hashable, Displayable, Serializable)
record Point
  x: Float
  y: Float
end
```

---

## 10. Constraint System

Constraints attach runtime validation rules to types and fields.

### 10.1 Field Constraints

```lumen
record Invoice
  id: String
    where length(id) >= 6
    where matches(id, r"^INV-\d+$")
  vendor: String
    where length(vendor) >= 1
  total: Float
    where total >= 0.0
  currency: String
    where currency in ["USD", "EUR", "GBP", "JPY"]
  items: list[LineItem]
    where length(items) >= 1
end
```

### 10.2 Constraint Expressions

Constraints use a restricted expression language:

```
constraint = expression ;  # must evaluate to Bool

# Available in constraints:
length(field)              # string/list/bytes length
matches(field, pattern)    # regex match
field in [values...]       # membership test
field >= value             # comparisons
field.some_property        # nested field access
and, or, not              # boolean combinators
```

### 10.3 Cross-Field Constraints

```lumen
record DateRange
  start: Int
  stop: Int
  where stop > start      # constraint referencing multiple fields
end

record Pagination
  page: Int where page >= 1
  per_page: Int where per_page >= 1 and per_page <= 100
  where page * per_page <= 10000  # cross-field constraint
end
```

### 10.4 Custom Constraint Functions

```lumen
cell is_valid_email(s: String) -> Bool
  return matches(s, r"^[^@]+@[^@]+\.[^@]+$")
end

record Contact
  email: String where is_valid_email(email)
end
```

Constraint functions must be pure (no tool calls, no side effects). The compiler verifies this statically.

---

## 11. Type Inference

Lumen uses bidirectional type inference with local inference only (no global Hindley-Milner). Types flow:

- **Forward** from annotations and literal types
- **Backward** from expected return types and context
- **Through** operators and function calls

### 11.1 Where Inference Applies

```lumen
let x = 42                   # inferred: Int
let y = 3.14                 # inferred: Float
let name = "Alice"           # inferred: String
let items = [1, 2, 3]        # inferred: list[Int]
let config = {"key": "val"}  # inferred: map[String, String]
let pair = (1, "hello")      # inferred: (Int, String)

# From function return types
cell get_name() -> String
  let result = "Alice"       # inferred: String (from return type)
  return result
end

# From function parameter types
cell process(items: list[Int]) -> Int
  return items.reduce(0, fn(acc, x) => acc + x)
  # fn type inferred from reduce signature
end
```

### 11.2 Where Annotations Are Required

```lumen
# Function parameters always require types
cell add(a: Int, b: Int) -> Int

# Empty collections require annotation
let empty: list[String] = []

# Ambiguous expressions
let val: Float = 0  # 0 could be Int or Float
```

---

# Part III: Expressions

---

## 12. Operators and Precedence

From highest to lowest precedence:

| Prec | Operator | Assoc | Description |
|------|----------|-------|-------------|
| 14 | `.` `[]` `()` | Left | Member access, indexing, call |
| 13 | `!` `?` | Postfix | Null assert, null propagate |
| 12 | `-` `not` `~` | Prefix | Negate, logical not, bitwise not |
| 11 | `**` | Right | Exponentiation |
| 10 | `*` `/` `%` | Left | Multiplicative |
| 9 | `+` `-` | Left | Additive |
| 8 | `..` `..=` | None | Range |
| 7 | `++` | Left | Concatenation |
| 6 | `|>` `>>` | Left | Pipe, compose |
| 5 | `==` `!=` `<` `<=` `>` `>=` `in` | None | Comparison |
| 4 | `and` | Left | Logical and |
| 3 | `or` | Left | Logical or |
| 2 | `??` | Left | Null coalescing |
| 1 | `\|` | Left | Union type constructor |

---

## 13. Pipe Operators

The pipe operator threads a value through a chain of transformations:

### 13.1 The `|>` (Pipe Forward) Operator

```lumen
# x |> f  desugars to  f(x)
# x |> f(y)  desugars to  f(x, y)

let result = raw_text
  |> trim()
  |> split("\n")
  |> filter(fn(line) => line.length > 0)
  |> map(fn(line) => line.upper())
  |> join(", ")
```

The left-hand value becomes the **first argument** of the right-hand function.

### 13.2 The `>>` (Compose) Operator

```lumen
# f >> g  creates  fn(x) => g(f(x))

let process = trim >> split("\n") >> filter(non_empty) >> join(", ")
let result = process(raw_text)
```

### 13.3 Pipe with Method Calls

When the right side is a method, pipe passes the value as `self`:

```lumen
let result = data
  |> json_parse()
  |> .get("items")       # method-style pipe
  |> .unwrap_or([])
  |> .map(fn(x) => x["name"])
```

---

## 14. Collection Comprehensions

List, map, and set comprehensions provide concise collection construction:

### 14.1 List Comprehensions

```lumen
# Basic
let squares = [x ** 2 for x in 1..=10]

# With filter
let evens = [x for x in 1..100 if x % 2 == 0]

# Nested
let pairs = [(x, y) for x in 1..=3 for y in 1..=3 if x != y]

# With transformation
let names = [user.name.upper() for user in users if user.active]
```

### 14.2 Map Comprehensions

```lumen
let scores = {name: score * 10 for (name, score) in entries}
let index = {item.id: item for item in items}
```

### 14.3 Set Comprehensions

```lumen
let unique_domains = set[email.split("@")[1] for email in emails]
```

---

## 15. Pattern Matching

Pattern matching is Lumen's primary control-flow mechanism for working with enums, unions, and structured data.

### 15.1 Match Expression

```lumen
let description = match shape
  Shape.Circle(radius:) ->
    "circle with radius {radius}"
  Shape.Rectangle(width:, height:) ->
    "rectangle {width}x{height}"
  Shape.Triangle(a:, b:, c:) ->
    "triangle with sides {a}, {b}, {c}"
  Shape.Point ->
    "point"
end
```

### 15.2 Pattern Types

```lumen
# Literal patterns
match x
  0 -> "zero"
  1 -> "one"
  _ -> "other"
end

# Variable binding
match value
  n: Int -> "integer {n}"
  s: String -> "string {s}"
end

# Record destructuring
match user
  User(name: "admin", ..) -> "admin user"
  User(name:, active: true, ..) -> "active: {name}"
  User(name:, active: false, ..) -> "inactive: {name}"
end

# Nested patterns
match response
  ok(User(name:, email:, ..)) -> "user: {name}"
  err(msg) -> "error: {msg}"
end

# Guard patterns
match temperature
  t if t < 0 -> "freezing"
  t if t < 20 -> "cold"
  t if t < 30 -> "comfortable"
  _ -> "hot"
end

# Or patterns
match direction
  Direction.North | Direction.South -> "vertical"
  Direction.East | Direction.West -> "horizontal"
end

# List patterns
match items
  [] -> "empty"
  [single] -> "one item: {single}"
  [first, second] -> "two items"
  [head, ..tail] -> "head: {head}, rest: {tail.length} items"
end

# Tuple patterns
match pair
  (0, _) -> "starts with zero"
  (_, 0) -> "ends with zero"
  (a, b) if a == b -> "equal: {a}"
  (a, b) -> "different: {a}, {b}"
end
```

### 15.3 Exhaustiveness

The compiler checks that match expressions are exhaustive — every possible value is handled. If not, a compile error is emitted listing missing patterns.

```lumen
# Compile error: non-exhaustive patterns
# Missing: Color.Blue
match color
  Color.Red -> "red"
  Color.Green -> "green"
end
```

### 15.4 If-Let

A shorthand for matching a single pattern:

```lumen
if let ok(value) = risky_operation()
  use(value)
end

if let User(name:, active: true, ..) = get_user(id)
  greet(name)
else
  log("user not found or inactive")
end
```

### 15.5 While-Let

Loop while a pattern matches:

```lumen
while let Some(item) = iterator.next()
  process(item)
end
```

---

## 16. Conditional Expressions

### 16.1 If Expression

`if` is an expression that returns a value:

```lumen
let status = if score >= 90 then "A"
  else if score >= 80 then "B"
  else if score >= 70 then "C"
  else "F"

# Single-line form
let abs_val = if x >= 0 then x else -x
```

### 16.2 Ternary-Style `when`

```lumen
let label = when
  score >= 90 -> "excellent"
  score >= 70 -> "good"
  score >= 50 -> "pass"
  else -> "fail"
end
```

---

## 17. Range Expressions

```lumen
1..5             # exclusive: [1, 2, 3, 4]
1..=5            # inclusive: [1, 2, 3, 4, 5]
5..1             # empty (no reverse ranges via ..)
(5..=1).rev()    # reverse: [5, 4, 3, 2, 1]
0..10 step 2     # [0, 2, 4, 6, 8]
'a'..='z'        # character range
```

Ranges are lazy iterators — they don't allocate a list until collected.

---

# Part IV: Statements and Declarations

---

## 18. Variable Bindings

### 18.1 Immutable Bindings

```lumen
let x = 42           # immutable, type inferred
let y: Float = 3.14  # immutable, type annotated
```

Immutable bindings cannot be reassigned. Attempting to assign to a `let` binding is a compile error.

### 18.2 Mutable Bindings

```lumen
let mut counter = 0
counter += 1         # ok
counter = 10         # ok

let mut items: list[String] = []
items = items ++ ["new"]  # reassign with new value
```

Mutable bindings are tracked by the trace system. Mutations appear in trace events when they affect values flowing to tool calls.

### 18.3 Constants

```lumen
const MAX_RETRIES = 3
const API_BASE = "https://api.example.com"
const TIMEOUT_MS = 30_000
```

Constants must be compile-time evaluable expressions. They are inlined at every use site.

### 18.4 Destructuring Bindings

```lumen
let (x, y) = get_coordinates()
let Point(x:, y:) = get_point()
let [first, second, ..rest] = get_items()
let {"name": name, "age": age} = get_config()
let ok(value) = risky_op()  # halts if err
```

---

## 19. Control Flow

### 19.1 If/Else

```lumen
if condition
  do_something()
else if other_condition
  do_other()
else
  do_default()
end
```

### 19.2 For Loops

```lumen
# Iterate over a list
for item in items
  process(item)
end

# With index
for (index, item) in items.enumerate()
  log("{index}: {item}")
end

# Over a range
for i in 0..10
  log(i)
end

# Over map entries
for (key, value) in config.entries
  log("{key} = {value}")
end

# With filter
for item in items if item.active
  process(item)
end

# Nested
for row in matrix
  for cell in row
    process(cell)
  end
end
```

### 19.3 While Loops

```lumen
let mut attempts = 0
while attempts < MAX_RETRIES
  let result = try_operation()
  if result.is_ok()
    break
  end
  attempts += 1
end
```

### 19.4 Loop (Infinite)

```lumen
loop
  let event = wait_for_event()
  match event
    Event.Shutdown -> break
    Event.Message(msg) -> handle(msg)
    Event.Tick -> continue
  end
end
```

### 19.5 Break and Continue

```lumen
# Break with value (loop as expression)
let found = loop
  let item = next()
  if item.matches_criteria()
    break item    # loop evaluates to item
  end
end

# Labeled loops
@outer for row in matrix
  for cell in row
    if cell == target
      break @outer  # break the outer loop
    end
  end
end
```

---

## 20. Cell Declarations

Cells are Lumen's functions — the units of execution and tracing.

### 20.1 Basic Cells

```lumen
cell greet(name: String) -> String
  return "Hello, {name}!"
end

# Expression body (single expression, no return needed)
cell double(x: Int) -> Int = x * 2

# No return value
cell log_message(msg: String)
  emit(msg)
end

# Multiple return via tuple
cell divide_with_remainder(a: Int, b: Int) -> (Int, Int)
  return (a / b, a % b)
end
```

### 20.2 Default Parameters

```lumen
cell fetch(url: String, timeout: Int = 5000, retries: Int = 3) -> Bytes
  # ...
end

# Called with defaults
fetch("https://example.com")
# Called with overrides
fetch("https://example.com", timeout: 10000)
```

### 20.3 Named Arguments

All cell calls support named arguments. Positional arguments must come before named:

```lumen
fetch("https://example.com", retries: 5)
```

### 20.4 Variadic Arguments

```lumen
cell format(template: String, ..args: list[String]) -> String
  let mut result = template
  for (i, arg) in args.enumerate()
    result = result.replace("{" + i.to_string() + "}", arg)
  end
  return result
end

format("Hello {0}, you have {1} messages", "Alice", "5")
```

### 20.5 Async Cells

```lumen
async cell fetch_all(urls: list[String]) -> list[Bytes]
  let results = await parallel
    for url in urls
      yield HttpGet(url: url)
    end
  end
  return results
end
```

### 20.6 Generic Cells

```lumen
cell first[T](items: list[T]) -> T | Null
  return items.first
end

cell zip_with[A, B, C](
  a: list[A],
  b: list[B],
  f: fn(A, B) -> C
) -> list[C]
  return a.zip(b).map(fn((x, y)) => f(x, y))
end
```

### 20.7 Cell Attributes

```lumen
@pure              # Compiler-verified: no tool calls, no side effects
@cached            # Results cached by input hash
@traced            # Override: always emit trace events (default for effectful)
@untraced          # Override: suppress trace events (for hot inner loops)
@deprecated("Use new_cell instead")
@timeout(5000)     # Default timeout for tool calls in this cell
@retry(3)          # Auto-retry on tool errors
cell my_cell() -> String
  # ...
end
```

---

## 21. Closures and Lambdas

### 21.1 Lambda Expressions

```lumen
# Full form
let add = fn(a: Int, b: Int) -> Int
  return a + b
end

# Expression body
let double = fn(x: Int) -> Int => x * 2

# Type-inferred (when context provides types)
let doubled = items.map(fn(x) => x * 2)

# Multi-parameter shorthand
let sum = items.reduce(0, fn(acc, x) => acc + x)
```

### 21.2 Closure Capture

Closures capture variables from their enclosing scope by value (immutable capture):

```lumen
let multiplier = 3
let scale = fn(x: Int) -> Int => x * multiplier

scale(5)  # 15
```

For mutable captures, use explicit `mut` capture:

```lumen
let mut count = 0
let increment = fn() => count += 1  # compile error: cannot mutate captured var

# Instead, use a cell-local mutable
cell counter() -> fn() -> Int
  let mut c = 0
  return fn() -> Int
    c += 1  # allowed: closure owns the mutable
    return c
  end
end
```

### 21.3 Closures as Trace Boundaries

When a closure performs tool calls, each invocation creates a trace event tagged with the enclosing cell's trace context. Closures inherit the capability scope of their enclosing cell.

### 21.4 Cell References

Named cells can be passed as values:

```lumen
cell apply(f: fn(Int) -> Int, value: Int) -> Int
  return f(value)
end

cell double(x: Int) -> Int = x * 2

apply(double, 5)  # 10
```

---

## 22. Decorators and Attributes

Attributes modify declarations with metadata and compiler directives:

```lumen
# On cells
@pure
@cached
@timeout(5000)
@retry(max: 3, backoff: "exponential")
@deprecated("Use v2 instead")
@test
@benchmark
cell my_cell() -> String
  # ...
end

# On records
@derive(Eq, Hashable, Serializable, Displayable)
@schema(strict: true, unknown_fields: "reject")
record MyRecord
  # ...
end

# On fields
record Config
  @env("API_KEY")
  api_key: String

  @default(3000)
  port: Int

  @secret
  password: String  # redacted in traces
end

# On modules (file-level)
@module(visibility: "public")
@author("Alice")
@version("1.0.0")
```

---

# Part V: Concurrency

---

## 23. Async/Await

Lumen's concurrency model is designed for agent workflows: multiple independent tool calls that can execute in parallel, with deterministic trace ordering.

### 23.1 Async Cells

```lumen
async cell fetch_data(url: String) -> Bytes
  let response = await HttpGet(url: url)
  return response.body
end
```

`async` cells return a `Future[T]` that must be `await`ed. All tool calls inside async cells are implicitly awaitable.

### 23.2 Parallel Blocks

The primary concurrency primitive for agent workflows:

```lumen
cell gather_context(topic: String) -> Context
  # These three calls execute concurrently
  let (weather, news, prices) = await parallel
    HttpGet(url: "https://api.weather.com/{topic}")
    HttpGet(url: "https://api.news.com/{topic}")
    HttpGet(url: "https://api.prices.com/{topic}")
  end

  return Context(weather: weather, news: news, prices: prices)
end
```

The compiler proves the parallel branches are independent (no data dependencies). If they're not independent, it's a compile error.

### 23.3 Parallel For

```lumen
cell process_batch(items: list[Item]) -> list[Result]
  return await parallel for item in items
    process_single(item)
  end
end

# With concurrency limit
cell rate_limited_batch(items: list[Item]) -> list[Result]
  return await parallel(max: 5) for item in items
    process_single(item)
  end
end
```

### 23.4 Select (First Completion)

```lumen
cell fastest_response(urls: list[String]) -> Bytes
  let (result, index) = await select
    for url in urls
      yield HttpGet(url: url)
    end
  end
  return result.body
end
```

`select` returns the result of the first completed branch and cancels the rest.

### 23.5 Timeout

```lumen
cell with_timeout(url: String) -> result[Bytes, String]
  return await timeout(5000)
    HttpGet(url: url)
  end
end
```

### 23.6 Channels

For complex coordination patterns between concurrent cells:

```lumen
cell producer_consumer() -> list[Result]
  let ch = channel[Item](buffer: 10)

  await parallel
    # Producer
    async
      for item in get_items()
        ch.send(item)
      end
      ch.close()
    end

    # Consumer
    async
      let mut results: list[Result] = []
      for item in ch.receive_all()
        results = results ++ [process(item)]
      end
      return results
    end
  end
end
```

### 23.7 Deterministic Trace Ordering

Despite parallel execution, traces maintain a deterministic order:

1. `parallel_start` event records the branch count
2. Each branch's events are grouped contiguously
3. Branches are ordered by their lexical position in the source
4. `parallel_end` event records completion order (for debugging) and the canonical order (for replay)

This means replaying a trace always produces identical output regardless of actual execution timing.

---

# Part VI: Effect System

---

## 24. Tool Declarations and Grants

### 24.1 Tool Import

```lumen
# Import a native tool
use tool http.get as HttpGet
use tool http.post as HttpPost
use tool llm.chat as Chat
use tool fs.read as ReadFile
use tool fs.write as WriteFile

# Import from MCP server
use tool mcp "https://mcp.notion.com/sse" as Notion
use tool mcp "https://mcp.github.com/sse" as GitHub

# Import with version pinning
use tool llm.chat@1.2.0 as Chat

# Import all tools from an MCP server
use tool mcp "https://mcp.slack.com/sse" as Slack.*
```

### 24.2 Capability Grants

```lumen
# HTTP constraints
grant HttpGet
  domain "api.example.com"
  domain "api.backup.com"
  timeout_ms 5000
  rate_limit 10 per_minute
  max_response_bytes 1_048_576

# LLM constraints
grant Chat
  model "claude-sonnet-4-20250514"
  max_tokens 4096
  temperature 0.0
  budget_usd 10.00
  rate_limit 20 per_minute

# Filesystem constraints
grant ReadFile
  path "/data/**"                  # glob patterns
  path "/config/*.json"
  max_file_bytes 10_485_760

grant WriteFile
  path "/output/**"
  max_file_bytes 5_242_880
  create_dirs true

# MCP tool constraints
grant Notion.create_page
  workspace "my-workspace"
  timeout_ms 15000

grant GitHub.create_issue
  repo "myorg/myrepo"
  labels ["bug", "automated"]
```

### 24.3 Grant Attenuation

Grants can be narrowed but never widened:

```lumen
# Parent module grants
grant HttpGet domain "*.example.com" timeout_ms 10000

# Child cell can narrow
cell restricted_fetch(url: String) -> Bytes
  @narrow HttpGet domain "api.example.com" timeout_ms 5000
  return HttpGet(url: url).body
end
```

### 24.4 Dynamic Tool Selection

```lumen
cell smart_fetch(urls: list[String]) -> list[Bytes]
  let results: list[Bytes] = []
  for url in urls
    let tool = if url.starts_with("https://api.notion.com")
      Notion.fetch
    else
      HttpGet
    end
    results = results ++ [tool(url: url).body]
  end
  return results
end
```

### 24.5 Tool Composition

```lumen
# Define a composite tool from primitives
cell fetch_and_parse[T](url: String, schema: type[T]) -> result[T, String]
  let response = HttpGet(url: url)
  if response.status != 200
    return err("HTTP {response.status}")
  end
  return response.body
    |> bytes_to_string()
    |> json_parse()
    |> validate(schema: T)
end
```

---

## 25. LLM Integration

### 25.1 Role Blocks

```lumen
let response = Chat(
  role system:
    You are a strict JSON extractor.
    Return only valid JSON matching the schema.
    Do not include any explanation or markdown.
  end,

  role user:
    Extract the invoice data from this text:
    {raw_text}
  end,

  role assistant:
    Here is the extracted data:
  end
) expect schema Invoice
```

### 25.2 Multi-Turn Conversations

```lumen
record Conversation
  messages: list[Message]
end

record Message
  role: String
  content: String
end

cell chat_loop(initial_prompt: String) -> String
  let mut conv = Conversation(messages: [
    Message(role: "system", content: "You are a helpful assistant."),
    Message(role: "user", content: initial_prompt)
  ])

  let response = Chat(messages: conv.messages)
  conv = Conversation(messages: conv.messages ++ [
    Message(role: "assistant", content: response)
  ])

  return response
end
```

### 25.3 Structured Output with Repair

```lumen
cell extract_with_repair(text: String) -> result[Invoice, String]
  let mut attempts = 0
  let mut errors: list[String] = []

  while attempts < 3
    let raw = Chat(
      role system:
        You are a strict JSON invoice extractor.
        {if errors.length > 0 then "Previous errors: " + errors.join("; ") else ""}
        Return only valid JSON matching the Invoice schema.
      end,
      role user:
        Extract the invoice from: {text}
      end
    )

    let result = validate(schema Invoice, json_parse(raw))
    match result
      ok(invoice) -> return ok(invoice)
      err(e) ->
        errors = errors ++ [e.message]
        attempts += 1
    end
  end

  return err("Failed after 3 attempts: {errors.join('; ')}")
end
```

### 25.4 Streaming

```lumen
async cell stream_response(prompt: String) -> String
  let mut buffer = ""

  await Chat.stream(
    role user: {prompt}
  ) on_chunk fn(chunk: String)
    buffer = buffer ++ chunk
    emit(chunk)  # real-time output
  end

  return buffer
end
```

### 25.5 Tool-Use / Function Calling

```lumen
cell agent_with_tools(query: String) -> String
  let tools = [
    tool_def("search", "Search the web", {
      "query": "String"
    }),
    tool_def("calculate", "Evaluate math", {
      "expression": "String"
    })
  ]

  let mut messages = [
    Message(role: "user", content: query)
  ]

  loop
    let response = Chat(
      messages: messages,
      tools: tools
    )

    match response
      TextResponse(text) ->
        return text
      ToolUse(name, args) ->
        let result = match name
          "search" -> web_search(args["query"])
          "calculate" -> evaluate(args["expression"])
          _ -> halt("Unknown tool: {name}")
        end
        messages = messages ++ [
          Message(role: "assistant", content: response.raw),
          Message(role: "tool", content: result, tool_use_id: response.id)
        ]
    end
  end
end
```

---

## 26. Schema Validation

### 26.1 Expect Schema

```lumen
# Strict mode: halts on failure
let invoice = json_parse(raw) expect schema Invoice

# Soft mode: returns result
let result = validate(schema Invoice, json_parse(raw))

# On LLM output
let data = Chat(...) expect schema MyType

# With custom error
let data = json_parse(raw) expect schema Invoice
  or_halt "Failed to parse invoice from API response"
```

### 26.2 Schema Composition

```lumen
# Extend schemas
record BaseEntity
  id: String
  created_at: Int
  updated_at: Int
end

record User extends BaseEntity
  name: String
  email: String
end

# Schema intersection
type Timestamped[T] = T & BaseEntity

# Schema refinement
type ValidatedInvoice = Invoice
  where total == items.map(fn(i) => i.quantity * i.unit_price).reduce(0.0, fn(a, b) => a + b)
```

### 26.3 Dynamic Schema

```lumen
cell validate_dynamic(data: Json, schema_name: String) -> result[Json, String]
  let schema = get_schema(schema_name)?
  return validate(schema: schema, data)
end
```

---

# Part VII: Error Handling

---

## 27. Error Model

Lumen has no exceptions. All errors are values.

### 27.1 The Result Type

```lumen
# Every fallible operation returns result[Ok, Err]
cell parse_int(s: String) -> result[Int, ParseError]
cell fetch(url: String) -> result[Response, HttpError]
cell validate(data: Json) -> result[Record, ValidationError]
```

### 27.2 The `?` Operator

Early-return on error:

```lumen
cell process(url: String) -> result[Invoice, String]
  let response = fetch(url)?           # returns err if fetch fails
  let data = json_parse(response.body)? # returns err if parse fails
  let invoice = validate(schema Invoice, data)?
  return ok(invoice)
end
```

### 27.3 Error Mapping

```lumen
cell process(url: String) -> result[Invoice, AppError]
  let response = fetch(url)
    .map_err(fn(e) => AppError.Network(e.message))?

  let data = json_parse(response.body)
    .map_err(fn(e) => AppError.Parse(e.message))?

  return ok(data)
end
```

### 27.4 Try Blocks

```lumen
let result = try
  let response = fetch(url)?
  let data = json_parse(response.body)?
  let invoice = validate(schema Invoice, data)?
  invoice
end
# result: result[Invoice, Error]
```

### 27.5 Halt

`halt` terminates execution immediately with an error message. It produces a trace event and is non-recoverable within the current run.

```lumen
cell critical_operation() -> Data
  let data = fetch_data()
  if data.is_empty
    halt("Critical: no data available")
  end
  return process(data)
end
```

### 27.6 Error Types

```lumen
# Built-in error hierarchy
enum LumenError
  ValidationError(field: String, message: String, value: Json)
  ToolError(tool: String, message: String, code: Int)
  PolicyError(tool: String, grant: String, message: String)
  TypeError(expected: String, got: String, location: String)
  RuntimeError(message: String)
  ParseError(message: String, line: Int, column: Int)
  TimeoutError(tool: String, timeout_ms: Int)
  CancelledError(reason: String)
end

# User-defined errors
enum AppError
  NotFound(entity: String, id: String)
  Unauthorized(message: String)
  RateLimit(retry_after: Int)
  Custom(code: String, message: String)
end
```

---

# Part VIII: Trace System

---

## 28. Trace Events

Every effectful operation produces a trace event. Traces are append-only, hash-chained JSONL files.

### 28.1 Event Types

| Event | Trigger | Data |
|-------|---------|------|
| `run_start` | Program begins | doc_hash, entry_cell, args_hash |
| `run_end` | Program completes | exit_code, total_latency_ms |
| `cell_enter` | Cell invocation | cell_name, args_hash, depth |
| `cell_exit` | Cell return | cell_name, result_hash, latency_ms |
| `tool_call` | Tool invocation | tool_id, inputs_hash, outputs_hash, latency_ms, cached |
| `schema_validate` | Schema validation | schema_name, input_hash, result, errors |
| `parallel_start` | Parallel block begins | branch_count |
| `parallel_end` | Parallel block ends | results_hash, execution_order |
| `error` | Runtime error | error_type, message, stack |
| `emit` | User output | content_hash |
| `cache_hit` | Cache lookup succeeded | tool_id, cache_key |
| `cache_miss` | Cache lookup failed | tool_id, cache_key |
| `grant_check` | Capability verification | tool_id, grant_id, allowed |

### 28.2 Hash Chain

Every event contains:

```json
{
  "seq": 7,
  "kind": "tool_call",
  "prev_hash": "sha256:abc123...",
  "hash": "sha256:def456...",
  "timestamp": "2026-02-12T10:30:00.000Z",
  ...
}
```

`hash = sha256(prev_hash + canonical_json(event_data))`

This creates a tamper-evident chain. Modifying any event invalidates all subsequent hashes.

### 28.3 Trace Queries

```lumen
# In code
let trace = trace_ref()
let events = trace.events(kind: "tool_call")
let cost = trace.total_cost()
let latency = trace.total_latency_ms()
```

```bash
# From CLI
lumen trace last
lumen trace show <run_id>
lumen trace diff <run_id_1> <run_id_2>
lumen trace query --kind tool_call --tool llm.chat
lumen trace stats <run_id>
lumen trace export <run_id> --format json
lumen trace verify <run_id>  # verify hash chain integrity
```

### 28.4 Replay

```bash
lumen replay <run_id>
```

Replay re-executes the program using cached tool outputs from the original run. Pure computation re-runs; tool calls return cached results. The output trace must hash-identically to the original.

### 28.5 Redaction

Sensitive data can be redacted from traces:

```lumen
@secret
record Credentials
  api_key: String
  password: String
end

# In trace: field values replaced with "[REDACTED]"
# In cache: field values replaced with hash references
```

---

## 29. Cache System

### 29.1 Cache Key Formula

```
cache_key = sha256(
  tool_id + ":" +
  tool_version + ":" +
  policy_hash + ":" +
  canonical_json(args)
)
```

### 29.2 Cache Control

```lumen
# File-level
@cache on           # enable for all tool calls
@cache off          # disable caching

# Per-call
let result = Chat(...) @no_cache         # skip cache for this call
let result = Chat(...) @cache_ttl(3600)  # cache expires in 1 hour
```

### 29.3 Cache CLI

```bash
lumen cache ls                      # list all cached entries
lumen cache ls --tool llm.chat      # filter by tool
lumen cache stats                   # hit rate, size, entries
lumen cache clear                   # clear everything
lumen cache clear --tool http.get   # clear specific tool
lumen cache clear --before 2026-01-01  # clear old entries
lumen cache export <path>           # export for sharing
lumen cache import <path>           # import shared cache
```

---

# Part IX: Standard Library

---

## 30. Core Modules

### 30.1 `std.string`

```lumen
import std.string: *

pad_left("42", 5, "0")      # "00042"
pad_right("hi", 10, " ")    # "hi        "
truncate("hello world", 5)  # "hello..."
slugify("Hello World!")      # "hello-world"
capitalize("hello")         # "Hello"
title_case("hello world")   # "Hello World"
snake_case("helloWorld")    # "hello_world"
camel_case("hello_world")   # "helloWorld"
levenshtein("kitten", "sitting")  # 3
```

### 30.2 `std.math`

```lumen
import std.math: *

abs(-5)        # 5
min(3, 7)      # 3
max(3, 7)      # 7
clamp(15, 0, 10)  # 10
sqrt(16.0)     # 4.0
pow(2.0, 10.0) # 1024.0
log(100.0, 10.0)  # 2.0
sin(PI / 2)    # 1.0
cos(0.0)       # 1.0
round(3.7)     # 4
ceil(3.2)      # 4
floor(3.8)     # 3
random_int(1, 100)   # random (traced for reproducibility)
random_float(0.0, 1.0)
```

### 30.3 `std.json`

```lumen
import std.json: *

let obj = json_parse("{\"key\": \"value\"}")
let str = json_encode(my_record)
let pretty = json_pretty(obj, indent: 2)
let merged = json_merge(a, b)
let path_val = json_path(obj, "$.items[0].name")
let diff = json_diff(old, new)
let patched = json_patch(obj, diff)
```

### 30.4 `std.time`

```lumen
import std.time: *

let now = timestamp()                    # Unix millis
let iso = format_time(now, "iso8601")    # "2026-02-12T..."
let parsed = parse_time("2026-02-12", "YYYY-MM-DD")
let diff = duration_between(start, end)  # Duration
let later = add_duration(now, hours: 2)

# Duration construction
let d = duration(hours: 1, minutes: 30, seconds: 0)
d.total_seconds()   # 5400
d.total_minutes()   # 90
```

### 30.5 `std.crypto`

```lumen
import std.crypto: *

sha256("hello")         # content hash
sha512("hello")
hmac_sha256(key, data)
md5("hello")            # for compatibility only
uuid_v4()               # random UUID
```

### 30.6 `std.collections`

```lumen
import std.collections: *

# OrderedMap — preserves insertion order
let om = ordered_map[("a", 1), ("b", 2)]

# Deque — double-ended queue
let dq = deque[1, 2, 3]
dq.push_front(0)
dq.push_back(4)
dq.pop_front()
dq.pop_back()

# Counter
let counter = count(["a", "b", "a", "c", "b", "a"])
# {"a": 3, "b": 2, "c": 1}
counter.most_common(2)  # [("a", 3), ("b", 2)]
```

### 30.7 `std.regex`

```lumen
import std.regex: *

let re = regex(r"(\d{4})-(\d{2})-(\d{2})")
re.is_match("2026-02-12")        # true
re.find("date: 2026-02-12 ok")   # "2026-02-12"
re.find_all("a1 b2 c3")          # ["1", "2", "3"]
re.captures("2026-02-12")        # ["2026", "02", "12"]
re.replace("hello world", r"\w+", "X")  # "X X"
re.split("a,b,,c", ",")          # ["a", "b", "", "c"]
```

### 30.8 `std.encoding`

```lumen
import std.encoding: *

base64_encode(data)
base64_decode(encoded)
url_encode("hello world")   # "hello%20world"
url_decode("hello%20world") # "hello world"
hex_encode(bytes)
hex_decode(hex_string)
```

### 30.9 `std.template`

```lumen
import std.template: *

let tmpl = template("""
  Hello {{name}},

  Your order #{{order_id}} contains:
  {{#each items}}
  - {{description}}: ${{price}}
  {{/each}}

  Total: ${{total}}
""")

let result = tmpl.render({
  "name": "Alice",
  "order_id": "12345",
  "items": [...],
  "total": "99.99"
})
```

### 30.10 `std.test`

```lumen
import std.test: *

@test
cell test_addition()
  assert_eq(1 + 1, 2)
  assert_ne(1 + 1, 3)
  assert(true)
  assert_gt(5, 3)
  assert_lt(3, 5)
  assert_contains([1, 2, 3], 2)
  assert_matches("hello", r"^h")
end

@test
cell test_parsing()
  let result = parse_invoice(sample_text)
  assert(result.is_ok())
  let invoice = result.unwrap()
  assert_eq(invoice.total, 99.99)
  assert_eq(invoice.items.length, 3)
end

@test
@should_fail(ValidationError)
cell test_invalid_invoice()
  let inv = Invoice(id: "x", vendor: "", total: -1.0, currency: "FAKE", items: [])
  # Should fail validation
end

@test
cell test_with_mock_tool()
  let mock_chat = mock_tool("llm.chat", fn(input) =>
    "{\"id\": \"INV-001\", \"total\": 42.0}"
  )

  with_tool(mock_chat)
    let result = extract_invoice("some text")
    assert(result.is_ok())
  end
end

# Run tests
# lumen test                       # run all tests
# lumen test file.lm.md            # run tests in file
# lumen test --filter "parsing"    # filter by name
# lumen test --parallel            # concurrent execution
```

---

# Part X: Metaprogramming

---

## 31. Compile-Time Evaluation

### 31.1 Const Expressions

```lumen
const MAX_ITEMS = 100
const API_URL = "https://api.example.com/v{API_VERSION}"
const SCHEMA_HASH = sha256(Invoice.schema_json)  # evaluated at compile time
```

### 31.2 Comptime Blocks

```lumen
comptime
  # This code runs at compile time
  let version = env("LUMEN_VERSION") ?? "dev"
  let features = env("LUMEN_FEATURES")?.split(",") ?? []
end

# Conditional compilation
comptime if env("TARGET") == "wasm"
  use tool wasm.fetch as HttpGet
else
  use tool http.get as HttpGet
end
```

### 31.3 Type-Level Computation

```lumen
# Compile-time type selection
type Response[Format] = comptime match Format
  "json" -> JsonResponse
  "xml"  -> XmlResponse
  "csv"  -> CsvResponse
end
```

---

## 32. Macros

Lumen macros are hygienic, AST-to-AST transformations that run at compile time.

### 32.1 Declarative Macros

```lumen
macro retry!(attempts, body)
  let mut _count = 0
  let mut _last_err = ""
  while _count < {attempts}
    let _result = try
      {body}
    end
    match _result
      ok(v) -> return ok(v)
      err(e) ->
        _last_err = e.message
        _count += 1
    end
  end
  return err("Failed after {attempts} attempts: " + _last_err)
end

# Usage
let result = retry!(3,
  fetch_unreliable_api(url)
)
```

### 32.2 Built-in Macros

```lumen
dbg!(expression)          # prints expression and value, returns value
todo!("not implemented")  # compile-time warning, runtime halt
unreachable!()            # marks code as unreachable (runtime halt if reached)
assert!(condition)        # debug assertion (removed in release)
env!("VAR_NAME")          # compile-time environment variable
include!("path/to/file")  # include file contents as string literal
stringify!(expression)    # convert expression to string
```

---

# Part XI: Interop and Targets

---

## 33. WASM Backend

Lumen compiles to WebAssembly for browser execution, edge deployment, and universal distribution.

### 33.1 WASM Compilation

```bash
lumen build --target wasm agent.lm.md -o agent.wasm
lumen build --target wasm-component agent.lm.md -o agent.component.wasm
```

### 33.2 WASM-Specific Directives

```lumen
@wasm
@wasm_export("run")
cell run(input: String) -> String
  # This cell is exported as a WASM function
end

@wasm_import("env", "log")
cell external_log(msg: String)
```

### 33.3 WASM Component Model

Lumen generates WASM components with WIT interfaces:

```wit
// Auto-generated agent.wit
package acme:invoice-agent@1.0.0;

interface agent {
  record invoice {
    id: string,
    vendor: string,
    total: float64,
    currency: string,
  }

  run: func(text: string) -> result<invoice, string>;
}

world invoice-agent {
  export agent;
}
```

### 33.4 Browser Execution

```html
<script type="module">
  import init, { run } from './agent.js';
  await init();
  const result = run("Invoice text...");
  console.log(result);
</script>
```

### 33.5 WASI Support

For non-browser WASM execution:

```bash
wasmtime agent.wasm -- "Invoice text..."
```

Tool calls in WASM mode are proxied through WASI interfaces or JavaScript host bindings, depending on the runtime.

---

## 34. Foreign Function Interface

### 34.1 Native FFI

```lumen
@ffi("rust")
extern cell fast_hash(data: Bytes) -> String

@ffi("c")
extern cell compress(data: Bytes, level: Int) -> Bytes
```

FFI functions are treated as tools with `@unsafe` capability — they bypass the capability model and must be explicitly granted.

### 34.2 Plugin Architecture

```lumen
# lumen-plugin.toml
[plugin]
name = "custom-validator"
version = "1.0.0"
entry = "src/lib.rs"

[plugin.tools]
validate_custom = { input = "Json", output = "result[Json, String]" }
```

Plugins compile to native shared libraries or WASM modules and are loaded by the runtime as tool providers.

---

## 35. Embedding API

Lumen can be embedded in other applications:

```rust
// Rust embedding
use lumen_runtime::{Runtime, Config};

let rt = Runtime::new(Config::default());
let module = rt.compile_file("agent.lm.md")?;
let result = rt.run(&module, "extract", &[Value::String("invoice text".into())])?;
```

```javascript
// JavaScript embedding (via WASM)
import { LumenRuntime } from '@lumen/runtime';

const rt = new LumenRuntime();
const module = await rt.compile(sourceCode);
const result = await module.call('extract', ['invoice text']);
```

```python
# Python embedding (via native extension)
from lumen import Runtime

rt = Runtime()
module = rt.compile_file("agent.lm.md")
result = module.call("extract", "invoice text")
```

---

# Part XII: Tooling

---

## 36. CLI Reference

```
lumen <command> [options] [args]

COMMANDS:
  new <name>              Create a new Lumen project
  init                    Initialize Lumen in current directory
  check <file>            Type-check without executing
  run <file>              Compile and execute
  build <file>            Compile to LIR (or WASM with --target)
  test [file]             Run test cells
  fmt <file>              Format Lumen blocks
  lint <file>             Run linter
  doc <file>              Generate documentation
  repl                    Interactive REPL
  trace <subcommand>      Trace inspection tools
  cache <subcommand>      Cache management
  tools <subcommand>      Tool management
  package <subcommand>    Package management
  lsp                     Start language server
  upgrade                 Self-update

RUN OPTIONS:
  --cell <name>           Run a specific cell
  --stdin <param>         Read parameter from stdin
  --args <json>           Pass arguments as JSON
  --profile <name>        Use configuration profile
  --no-cache              Disable caching
  --no-trace              Disable tracing
  --offline               Use only cached tool outputs
  --timeout <ms>          Global timeout
  --verbose               Verbose output
  --dry-run               Show what would execute without executing

BUILD OPTIONS:
  --target <native|wasm>  Compilation target
  --optimize <0|1|2>      Optimization level
  --output <path>         Output path
  --sourcemap             Generate source map

TEST OPTIONS:
  --filter <pattern>      Filter tests by name
  --parallel              Run tests concurrently
  --coverage              Generate coverage report
  --snapshot              Update test snapshots

PACKAGE OPTIONS:
  publish                 Publish package to registry
  install <package>       Install a dependency
  update                  Update dependencies
  audit                   Security audit of dependencies
  search <query>          Search package registry
```

---

## 37. LSP Specification

The Lumen Language Server implements LSP 3.17 with the following capabilities:

### 37.1 Core Features

| Feature | Description |
|---------|-------------|
| Diagnostics | Real-time errors and warnings (< 50ms) |
| Completion | Cells, types, fields, tools, intrinsics, keywords |
| Hover | Type information, documentation, constraint details |
| Go to Definition | Cells, records, enums, tool aliases, imports |
| Find References | All usages of a symbol |
| Rename | Safe rename across module |
| Document Symbols | Outline view of cells, types, tools |
| Workspace Symbols | Cross-file symbol search |
| Code Actions | Quick fixes, import suggestions, grant additions |
| Formatting | Auto-format Lumen blocks (preserve Markdown) |
| Code Lens | "Run Cell" / "Debug Cell" / "Show Trace" above cells |
| Inlay Hints | Inferred types, parameter names |
| Semantic Tokens | Rich syntax highlighting |
| Signature Help | Parameter info during function calls |
| Call Hierarchy | Incoming/outgoing call graph |
| Folding | Collapse cells, records, code blocks |

### 37.2 Lumen-Specific Extensions

| Feature | Description |
|---------|-------------|
| Run Cell | Execute a cell inline, show result |
| Trace View | Display trace for last run |
| Cache Inspector | View cached tool outputs |
| Tool Discovery | Browse available MCP tools |
| Schema Viewer | Visualize record schemas |
| Grant Auditor | Show all capability grants |

### 37.3 Incremental Processing

The LSP maintains per-document state and re-processes only affected code blocks on edit. Dependency tracking across cells enables minimal re-typechecking.

---

## 38. Debug Adapter Protocol

Lumen implements DAP for step-through debugging:

### 38.1 Breakpoints

```
- Line breakpoints (in Lumen code blocks)
- Cell entry/exit breakpoints
- Tool call breakpoints (break before any tool call)
- Conditional breakpoints (break when expression is true)
- Schema validation breakpoints (break on validation failure)
```

### 38.2 Trace-Based Time-Travel Debugging

```bash
lumen debug --replay <run_id>
```

Using a recorded trace, step forward and backward through execution. Tool call results are replayed from cache. This enables debugging of completed runs without re-executing tools.

### 38.3 Watch Expressions

The debugger supports watching arbitrary expressions, register values, and trace state during execution.

---

## 39. Package Manager

### 39.1 Registry

Packages are published to the Lumen Package Registry (lumen.dev/packages).

```bash
lumen package publish      # publish current package
lumen package install <n>  # install dependency
lumen package search <q>   # search registry
lumen package audit        # security check
lumen package update       # update to latest compatible
lumen package lock         # regenerate lock file
```

### 39.2 Package Manifest

```toml
# lumen.toml
[package]
name = "acme.invoice_agent"
version = "1.0.0"
authors = ["Alice <alice@example.com>"]
description = "Invoice extraction agent"
license = "MIT"
repository = "https://github.com/acme/invoice-agent"
keywords = ["invoice", "extraction", "llm"]
edition = 1

[dependencies]
lumen-std = "1.0"
lumen-llm = "1.2"
acme-schemas = { git = "https://github.com/acme/schemas.git", tag = "v2.0" }
local-tools = { path = "../shared-tools" }

[dev-dependencies]
lumen-test = "1.0"

[tools]
http.get = { version = "1.0", bundled = true }
llm.chat = { version = "1.0", bundled = true }

[tools.mcp]
notion = "https://mcp.notion.com/sse"
github = "https://mcp.github.com/sse"

[features]
default = ["json-validation"]
streaming = []
experimental-repair = []
```

### 39.3 Lock File

`.lumen/lock.json` pins all dependency versions and tool manifests for reproducible builds. The lock file is committed to version control.

---

# Part XIII: LIR Specification

---

## 40. Instruction Set (Complete)

### 40.1 Instruction Format

32-bit fixed-width instructions following Lua 5.0 design:

```
Format ABC:  [opcode:8][A:8][B:8][C:8]
Format ABx:  [opcode:8][A:8][Bx:16]
Format Ax:   [opcode:8][Ax:24]
Format AsB:  [opcode:8][A:8][sB:16]    (signed Bx)
```

### 40.2 Complete Opcode Table

| Op | Name | Format | Semantics |
|----|------|--------|-----------|
| 0x00 | `NOP` | Ax | No operation |
| 0x01 | `LOADK` | ABx | R[A] = K[Bx] |
| 0x02 | `LOADNIL` | ABC | R[A..A+B] = nil |
| 0x03 | `LOADBOOL` | ABC | R[A] = Bool(B); if C then PC++ |
| 0x04 | `LOADINT` | AsB | R[A] = sB (small integer, -32768..32767) |
| 0x05 | `MOVE` | ABC | R[A] = R[B] |
| 0x06 | `NEWLIST` | ABC | R[A] = list(R[A+1]..R[A+B]) |
| 0x07 | `NEWMAP` | ABC | R[A] = map(R[A+1]..R[A+2*B]) |
| 0x08 | `NEWRECORD` | ABx | R[A] = record(type=Bx, fields from subsequent regs) |
| 0x09 | `NEWUNION` | ABC | R[A] = union(tag=B, payload=R[C]) |
| 0x0A | `NEWTUPLE` | ABC | R[A] = tuple(R[A+1]..R[A+B]) |
| 0x0B | `NEWSET` | ABC | R[A] = set(R[A+1]..R[A+B]) |
| 0x10 | `GETFIELD` | ABC | R[A] = R[B].field[C] |
| 0x11 | `SETFIELD` | ABC | R[A].field[B] = R[C] |
| 0x12 | `GETINDEX` | ABC | R[A] = R[B][R[C]] |
| 0x13 | `SETINDEX` | ABC | R[A][R[B]] = R[C] |
| 0x14 | `GETTUPLE` | ABC | R[A] = R[B].element[C] |
| 0x20 | `ADD` | ABC | R[A] = R[B] + R[C] |
| 0x21 | `SUB` | ABC | R[A] = R[B] - R[C] |
| 0x22 | `MUL` | ABC | R[A] = R[B] * R[C] |
| 0x23 | `DIV` | ABC | R[A] = R[B] / R[C] |
| 0x24 | `MOD` | ABC | R[A] = R[B] % R[C] |
| 0x25 | `POW` | ABC | R[A] = R[B] ** R[C] |
| 0x26 | `NEG` | ABC | R[A] = -R[B] |
| 0x27 | `CONCAT` | ABC | R[A] = R[B] ++ R[C] |
| 0x28 | `BITOR` | ABC | R[A] = R[B] \| R[C] (bitwise) |
| 0x29 | `BITAND` | ABC | R[A] = R[B] & R[C] (bitwise) |
| 0x2A | `BITXOR` | ABC | R[A] = R[B] ^ R[C] (bitwise) |
| 0x2B | `BITNOT` | ABC | R[A] = ~R[B] (bitwise) |
| 0x2C | `SHL` | ABC | R[A] = R[B] << R[C] |
| 0x2D | `SHR` | ABC | R[A] = R[B] >> R[C] |
| 0x30 | `EQ` | ABC | if (R[B] == R[C]) != A then PC++ |
| 0x31 | `LT` | ABC | if (R[B] < R[C]) != A then PC++ |
| 0x32 | `LE` | ABC | if (R[B] <= R[C]) != A then PC++ |
| 0x33 | `NOT` | ABC | R[A] = not R[B] |
| 0x34 | `AND` | ABC | R[A] = R[B] and R[C] |
| 0x35 | `OR` | ABC | R[A] = R[B] or R[C] |
| 0x36 | `IN` | ABC | R[A] = R[B] in R[C] |
| 0x37 | `IS` | ABC | R[A] = R[B] is_type R[C] |
| 0x38 | `NULLCO` | ABC | R[A] = R[B] ?? R[C] |
| 0x40 | `JMP` | AsB | PC += sB |
| 0x41 | `CALL` | ABC | R[A..A+C-1] = R[A](R[A+1]..R[A+B]) |
| 0x42 | `TAILCALL` | ABC | return R[A](R[A+1]..R[A+B]) |
| 0x43 | `RETURN` | ABC | return R[A..A+B-1] |
| 0x44 | `HALT` | ABC | halt with message R[A] |
| 0x45 | `LOOP` | AsB | loop counter check + jump |
| 0x46 | `FORPREP` | AsB | initialize for-loop |
| 0x47 | `FORLOOP` | AsB | iterate for-loop |
| 0x48 | `FORIN` | ABC | for-in iterator step |
| 0x49 | `BREAK` | Ax | break from enclosing loop |
| 0x4A | `CONTINUE` | Ax | continue to next iteration |
| 0x50 | `INTRINSIC` | ABC | R[A] = intrinsic[B](args at C) |
| 0x51 | `CLOSURE` | ABx | R[A] = closure(proto=Bx, upvalues from regs) |
| 0x52 | `GETUPVAL` | ABC | R[A] = upvalue[B] |
| 0x53 | `SETUPVAL` | ABC | upvalue[B] = R[A] |
| 0x60 | `TOOLCALL` | ABx | R[A] = tool_call(tool=Bx, args from subsequent regs) |
| 0x61 | `SCHEMA` | ABC | R[A] = validate(R[A], schema=B, mode=C) |
| 0x62 | `EMIT` | ABC | emit output R[A] |
| 0x63 | `TRACEREF` | ABC | R[A] = current trace reference |
| 0x64 | `AWAIT` | ABC | R[A] = await future R[B] |
| 0x65 | `SPAWN` | ABx | R[A] = spawn async(proto=Bx) |
| 0x66 | `CHANNEL` | ABC | R[A] = new channel(buffer=B) |
| 0x67 | `SEND` | ABC | R[A].send(R[B]) |
| 0x68 | `RECV` | ABC | R[A] = R[B].receive() |

---

# Part XIV: Formal Grammar

---

## 41. Complete EBNF Grammar

```ebnf
(* Top-level *)
program          = { directive | declaration } ;
directive        = "@" IDENT [ directive_args ] NEWLINE ;
directive_args   = { IDENT | STRING | INT | FLOAT } ;

(* Declarations *)
declaration      = cell_decl
                 | record_decl
                 | enum_decl
                 | trait_decl
                 | impl_decl
                 | type_alias
                 | const_decl
                 | use_decl
                 | grant_decl
                 | import_decl
                 | macro_decl ;

(* Imports *)
import_decl      = [ "pub" ] "import" module_path ":" import_list NEWLINE ;
module_path      = IDENT { "." IDENT } ;
import_list      = "*" | import_item { "," import_item } ;
import_item      = IDENT [ "as" IDENT ] ;

(* Tool declarations *)
use_decl         = "use" "tool" tool_ref "as" IDENT [ "." "*" ] NEWLINE ;
tool_ref         = IDENT { "." IDENT } [ "@" VERSION ]
                 | "mcp" STRING ;
grant_decl       = "grant" IDENT { "." IDENT } NEWLINE
                   INDENT { grant_clause NEWLINE } DEDENT ;
grant_clause     = IDENT grant_value { grant_value } ;
grant_value      = STRING | INT | FLOAT | IDENT
                 | IDENT "per_minute" | IDENT "per_hour" ;

(* Type declarations *)
record_decl      = { attribute } [ "pub" ] "record" IDENT [ type_params ]
                   [ "extends" type_ref ]
                   NEWLINE INDENT { field_decl } { where_clause } DEDENT "end" ;
field_decl       = { attribute } IDENT ":" type_expr [ "=" expression ]
                   { where_clause } NEWLINE ;
where_clause     = "where" expression NEWLINE ;

enum_decl        = { attribute } [ "pub" ] "enum" IDENT [ type_params ] NEWLINE
                   INDENT { variant_decl } { cell_decl } DEDENT "end" ;
variant_decl     = IDENT [ "(" field_list ")" ] NEWLINE ;

trait_decl       = [ "pub" ] "trait" IDENT [ ":" trait_bounds ] NEWLINE
                   INDENT { cell_sig | cell_decl } DEDENT "end" ;
cell_sig         = "cell" IDENT "(" param_list ")" "->" type_expr NEWLINE ;

impl_decl        = "impl" [ type_params ] IDENT "for" type_ref NEWLINE
                   INDENT { cell_decl } DEDENT "end" ;

type_alias       = [ "pub" ] "type" IDENT [ type_params ] "=" type_expr
                   { where_clause } NEWLINE ;

const_decl       = "const" IDENT [ ":" type_expr ] "=" expression NEWLINE ;

(* Cell declarations *)
cell_decl        = { attribute } [ "pub" ] [ "async" ] "cell" IDENT
                   [ type_params ] "(" param_list ")" [ "->" type_expr ]
                   [ where_clauses ]
                   ( "=" expression NEWLINE
                   | NEWLINE INDENT { statement } DEDENT "end" ) ;

attribute        = "@" IDENT [ "(" attr_args ")" ] NEWLINE ;
attr_args        = attr_arg { "," attr_arg } ;
attr_arg         = expression | IDENT ":" expression ;

(* Type expressions *)
type_expr        = type_primary { "|" type_primary } ;
type_primary     = "String" | "Int" | "Float" | "Bool" | "Bytes"
                 | "Json" | "Null"
                 | "list" "[" type_expr "]"
                 | "map" "[" type_expr "," type_expr "]"
                 | "set" "[" type_expr "]"
                 | "tuple" "[" type_expr { "," type_expr } "]"
                 | "result" "[" type_expr "," type_expr "]"
                 | "fn" "(" [ type_expr { "," type_expr } ] ")" "->" type_expr
                 | "Future" "[" type_expr "]"
                 | "channel" "[" type_expr "]"
                 | IDENT [ "[" type_expr { "," type_expr } "]" ]
                 | "(" type_expr { "," type_expr } ")"
                 | "type" "[" type_expr "]" ;

type_params      = "[" type_param { "," type_param } "]" ;
type_param       = IDENT [ ":" trait_bounds ] ;
trait_bounds      = IDENT { "+" IDENT } ;

(* Parameters *)
param_list       = [ param { "," param } ] ;
param            = IDENT ":" type_expr [ "=" expression ] ;

(* Statements *)
statement        = let_stmt
                 | assign_stmt
                 | if_stmt
                 | match_stmt
                 | for_stmt
                 | while_stmt
                 | loop_stmt
                 | return_stmt
                 | break_stmt
                 | continue_stmt
                 | expression_stmt ;

let_stmt         = "let" [ "mut" ] pattern [ ":" type_expr ] "=" expression NEWLINE ;
assign_stmt      = lvalue assign_op expression NEWLINE ;
assign_op        = "=" | "+=" | "-=" | "*=" | "/=" ;
lvalue           = IDENT | IDENT "." IDENT | IDENT "[" expression "]" ;

if_stmt          = "if" [ "let" pattern "=" ] expression NEWLINE
                   INDENT { statement } DEDENT
                   { "else" "if" [ "let" pattern "=" ] expression NEWLINE
                     INDENT { statement } DEDENT }
                   [ "else" NEWLINE INDENT { statement } DEDENT ]
                   "end" ;

match_stmt       = "match" expression NEWLINE
                   INDENT { match_arm } DEDENT "end" ;
match_arm        = pattern [ "if" expression ] "->"
                   ( expression NEWLINE
                   | NEWLINE INDENT { statement } DEDENT ) ;

for_stmt         = [ "@" IDENT ] "for" pattern "in" expression
                   [ "if" expression ] NEWLINE
                   INDENT { statement } DEDENT "end" ;

while_stmt       = "while" [ "let" pattern "=" ] expression NEWLINE
                   INDENT { statement } DEDENT "end" ;

loop_stmt        = "loop" NEWLINE INDENT { statement } DEDENT "end" ;

return_stmt      = "return" [ expression ] NEWLINE ;
break_stmt       = "break" [ "@" IDENT ] [ expression ] NEWLINE ;
continue_stmt    = "continue" [ "@" IDENT ] NEWLINE ;
expression_stmt  = expression NEWLINE ;

(* Patterns *)
pattern          = "_"
                 | literal
                 | IDENT
                 | IDENT ":" type_expr
                 | IDENT "(" [ field_pattern { "," field_pattern } [ "," ".." ] ] ")"
                 | "(" [ pattern { "," pattern } ] ")"
                 | "[" [ pattern { "," pattern } [ "," ".." IDENT ] ] "]"
                 | "{" [ STRING ":" pattern { "," STRING ":" pattern } [ "," ".." IDENT ] ] "}"
                 | "ok" "(" pattern ")"
                 | "err" "(" pattern ")"
                 | pattern "|" pattern ;

field_pattern    = IDENT ":" pattern | IDENT ":" ;

(* Expressions *)
expression       = pipe_expr ;
pipe_expr        = or_expr { "|>" or_expr } ;
or_expr          = and_expr { "or" and_expr } ;
and_expr         = null_co_expr { "and" null_co_expr } ;
null_co_expr     = comparison { "??" comparison } ;
comparison       = concat_expr [ comp_op concat_expr ] ;
comp_op          = "==" | "!=" | "<" | "<=" | ">" | ">=" | "in" ;
concat_expr      = range_expr { "++" range_expr } ;
range_expr       = add_expr [ ( ".." | "..=" ) add_expr [ "step" add_expr ] ] ;
add_expr         = mul_expr { ("+" | "-") mul_expr } ;
mul_expr         = pow_expr { ("*" | "/" | "%") pow_expr } ;
pow_expr         = unary_expr [ "**" pow_expr ] ;
unary_expr       = ( "-" | "not" | "~" ) unary_expr | postfix_expr ;
postfix_expr     = primary { "." IDENT | "[" expression "]" | "(" arg_list ")"
                           | "?" | "!" | "expect" "schema" type_ref } ;

primary          = literal
                 | IDENT
                 | "(" expression { "," expression } ")"
                 | "[" [ expression { "," expression } ] "]"
                 | "[" expression "for" pattern "in" expression
                       [ "if" expression ] "]"
                 | "{" [ map_entry { "," map_entry } ] "}"
                 | "{" expression "for" pattern "in" expression
                       [ "if" expression ] "}"
                 | "set" "[" [ expression { "," expression } ] "]"
                 | "fn" "(" param_list ")" [ "->" type_expr ]
                       ( "=>" expression | NEWLINE INDENT { statement } DEDENT "end" )
                 | if_expr
                 | match_expr
                 | when_expr
                 | "try" NEWLINE INDENT { statement } DEDENT "end"
                 | "await" expression
                 | "await" "parallel" parallel_body
                 | role_block
                 | IDENT "(" arg_list ")" ;

literal          = INT | FLOAT | STRING | BOOL | "null" | BYTES ;
arg_list         = [ arg { "," arg } ] ;
arg              = [ IDENT ":" ] expression ;
map_entry        = expression ":" expression | ".." expression ;

role_block       = "role" IDENT ":" NEWLINE
                   INDENT { role_line } DEDENT "end" ;
role_line        = { TEXT | interpolation } NEWLINE ;

if_expr          = "if" expression "then" expression
                   { "else" "if" expression "then" expression }
                   "else" expression ;

match_expr       = "match" expression NEWLINE
                   INDENT { match_arm } DEDENT "end" ;

when_expr        = "when" NEWLINE
                   INDENT { expression "->" expression NEWLINE }
                   [ "else" "->" expression NEWLINE ]
                   DEDENT "end" ;

parallel_body    = NEWLINE INDENT { parallel_branch } DEDENT "end"
                 | "for" pattern "in" expression NEWLINE
                   INDENT { statement } DEDENT "end" ;
parallel_branch  = expression NEWLINE
                 | "async" NEWLINE INDENT { statement } DEDENT "end" ;

(* Macros *)
macro_decl       = "macro" IDENT "!" "(" macro_params ")" NEWLINE
                   INDENT { macro_body } DEDENT "end" ;
macro_params     = IDENT { "," IDENT } ;
```

---

# Part XV: Appendices

---

## A. Complete List of Intrinsics

| Intrinsic | Signature | Description |
|-----------|-----------|-------------|
| `length` | `(v: String \| list \| Bytes \| map \| set) -> Int` | Element/byte count |
| `count` | `(v: list[T], pred: fn(T) -> Bool) -> Int` | Conditional count |
| `matches` | `(v: String, pattern: String) -> Bool` | Regex match |
| `hash` | `(v: Any) -> String` | Content hash (SHA-256) |
| `validate` | `(schema: type[T], v: Any) -> result[T, ValidationError]` | Schema validation |
| `diff` | `(a: T, b: T) -> list[Patch]` | Structural diff |
| `patch` | `(v: T, patches: list[Patch]) -> T` | Apply patches |
| `redact` | `(v: T, fields: list[String]) -> T` | Redact fields |
| `trace_ref` | `() -> TraceRef` | Current trace reference |
| `emit` | `(v: Any)` | Emit output |
| `typeof` | `(v: Any) -> String` | Runtime type name |
| `sizeof` | `(v: Any) -> Int` | Memory size in bytes |
| `debug` | `(v: Any) -> String` | Debug representation |
| `clone` | `(v: T) -> T` | Deep copy |
| `freeze` | `(v: T) -> T` | Ensure deeply immutable |
| `timestamp` | `() -> Int` | Current Unix milliseconds |
| `uuid` | `() -> String` | Generate UUID v4 |

## B. Error Codes

| Range | Category |
|-------|----------|
| E0001–E0099 | Syntax errors |
| E0100–E0199 | Type errors |
| E0200–E0299 | Name resolution errors |
| E0300–E0399 | Constraint errors |
| E0400–E0499 | Tool/grant errors |
| E0500–E0599 | Import/module errors |
| E0600–E0699 | Concurrency errors |
| E0700–E0799 | Macro errors |
| E0800–E0899 | WASM target errors |
| W0001–W0099 | Warnings |

## C. Canonical Hashing Rules

1. All values serialized as Canonical JSON (RFC 8785 subset)
2. Object keys sorted lexicographically (Unicode code point order)
3. No whitespace between tokens
4. UTF-8 encoding, no BOM
5. Integers as JSON integers (no unnecessary `.0`)
6. Floats normalized: no trailing zeros, scientific notation for |x| > 1e15 or |x| < 1e-6
7. Large values (> 1KB) replaced by blob hash reference
8. Null serialized as `null`
9. Booleans as `true` / `false`
10. Lists as JSON arrays (order preserved)
11. Maps as JSON objects (keys sorted)
12. Records as JSON objects (field names sorted)
13. Enums as `{"tag": "VariantName", "data": {...}}` or `{"tag": "VariantName"}` for unit variants
14. Hash algorithm: SHA-256 producing lowercase hex

## D. Reserved for Future Versions

The following features are intentionally deferred:

| Feature | Target Version | Notes |
|---------|---------------|-------|
| Dependent types | v3 | Full compile-time constraint verification |
| Linear types | v3 | Move semantics for capability tokens |
| Effect handlers | v2 | Algebraic effects beyond tool calls |
| Hot code reload | v2 | Swap cells without restarting |
| Distributed execution | v3 | Multi-node agent coordination |
| GPU offload | v3 | Compute-intensive cells on GPU via WASM+WebGPU |
| Formal verification | v3 | Machine-checkable proofs of cell properties |
| Visual editor | v2 | Node-based workflow editor generating `.lm.md` |

---

*End of Specification*

*This document defines the complete Lumen programming language. Implementations are conformant if they accept all well-formed programs described herein, reject all ill-formed programs with appropriate diagnostics, and produce semantically identical results for all deterministic operations.*