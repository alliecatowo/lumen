# Types

Lumen has a static type system with type inference.

## Primitive Types

| Type | Description | Examples |
|------|-------------|----------|
| `Int` | 64-bit signed integer | `42`, `-7`, `0` |
| `Float` | 64-bit floating point | `3.14`, `-0.5`, `1e10` |
| `Bool` | Boolean | `true`, `false` |
| `String` | UTF-8 string | `"hello"`, `"""multi"""` |
| `Bytes` | Byte sequence | `b"deadbeef"` |
| `Json` | JSON value | `{"key": "value"}` |
| `Null` | Null type | `null` |

## Collection Types

### List

Ordered, indexed collection:

```lumen
let numbers: list[Int] = [1, 2, 3, 4, 5]
let empty: list[String] = []

let first = numbers[0]     # 1
let len = length(numbers)  # 5
```

### Map

Key-value pairs:

```lumen
let scores: map[String, Int] = {"alice": 95, "bob": 87}

let alice = scores["alice"]  # 95
let has_key = "alice" in scores  # true
```

### Set

Unordered unique values:

```lumen
let unique: set[Int] = {1, 2, 3}

let has = 2 in unique  # true
```

### Tuple

Fixed-size heterogeneous collection:

```lumen
let pair: tuple[Int, String] = (1, "hello")
let triple = (1, 2, 3)

let first = pair[0]   # 1
let second = pair[1]  # "hello"
```

## Result Type

For error handling:

```lumen
let success: result[Int, String] = ok(42)
let failure: result[Int, String] = err("failed")

match result
  ok(value) -> # handle success
  err(msg) -> # handle error
end
```

See [Error Handling](../learn/tutorial/error-handling) for details.

## Union Types

Combine multiple types:

```lumen
let value: Int | String = 42
let maybe: Int | Null = null

match value
  n: Int -> # handle int
  s: String -> # handle string
end
```

## Function Types

```lumen
let add: fn(Int, Int) -> Int = fn(a, b) => a + b
let process: fn(String) -> String / {http}
```

With effects:

```lumen
type AsyncHandler = fn(Request) -> Response / {http, trace}
```

## User-Defined Types

### Records

```lumen
record Point
  x: Float
  y: Float
end

let p: Point = Point(x: 1.0, y: 2.0)
```

### Enums

```lumen
enum Status
  Pending
  Active
  Done
end

let s: Status = Active
```

With data:

```lumen
enum Result
  Ok(value: Int)
  Err(message: String)
end
```

### Type Aliases

```lumen
type UserId = String
type Point3D = tuple[Float, Float, Float]
type Handler = fn(Request) -> Response
```

## Generic Types

```lumen
record Box[T]
  value: T
end

let int_box: Box[Int] = Box(value: 42)
let str_box: Box[String] = Box(value: "hello")
```

Multiple type parameters:

```lumen
record Pair[A, B]
  first: A
  second: B
end

let pair: Pair[Int, String] = Pair(first: 1, second: "a")
```

## Type Constraints

Add validation with `where`:

```lumen
record Product
  name: String where length(name) > 0
  price: Float where price >= 0.0
  quantity: Int where quantity >= 0
end
```

Constraints are checked at record construction.

## Type Inference

Types are inferred when not specified:

```lumen
let x = 42           # Int
let y = 3.14         # Float
let z = "hello"      # String
let items = [1, 2]   # list[Int]
let map = {"a": 1}   # map[String, Int]
```

Function return types can be inferred but explicit is preferred:

```lumen
cell add(a: Int, b: Int) -> Int  # Explicit
  return a + b
end
```

## Type Compatibility

### Subtyping

Union types allow compatible assignments:

```lumen
let a: Int = 5
let b: Int | String = a  # OK: Int is compatible with Int | String
```

### Any Type

`Any` is compatible with all types:

```lumen
let x: Any = 42
let y: Any = "hello"
```

Use sparingly—prefer specific types.

## Next Steps

- [Expressions](./expressions) — Operators and literals
- [Pattern Matching](./patterns) — Destructuring values
