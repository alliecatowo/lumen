# Pattern Matching

Pattern matching lets you destructure and inspect values.

## Pattern Types

### Literal Patterns

Match exact values:

```lumen
match n
  0 -> "zero"
  1 -> "one"
  2 -> "two"
  _ -> "many"
end
```

Works with: `Int`, `Float`, `Bool`, `String`

### Wildcard Pattern

Match anything:

```lumen
match x
  0 -> "zero"
  _ -> "not zero"
end
```

### Binding Patterns

Capture matched value:

```lumen
match n
  0 -> "zero"
  x -> "got {x}"
end
```

### Tuple Patterns

```lumen
match point
  (0, 0) -> "origin"
  (x, 0) -> "on x-axis"
  (0, y) -> "on y-axis"
  (x, y) -> "({x}, {y})"
end
```

### List Patterns

```lumen
match items
  [] -> "empty"
  [x] -> "one: {x}"
  [x, y] -> "two: {x}, {y}"
  [first, ...rest] -> "first: {first}, rest: {length(rest)}"
end
```

With rest capture:

```lumen
match items
  [first, second, ...rest] -> # first two elements, rest in list
  [...rest, last] -> # all but last in rest, last separately
end
```

### Record Patterns

```lumen
match user
  User(name: "Admin", ..) -> "administrator"
  User(name: n, active: true, ..) -> "active: {n}"
  User(active: false, ..) -> "inactive"
end
```

### Enum Patterns

Simple enum:

```lumen
match status
  Pending -> "waiting"
  Active -> "running"
  Done -> "complete"
end
```

With data:

```lumen
match result
  Ok(value) -> "success: {value}"
  Err(msg) -> "error: {msg}"
end
```

### Type Patterns

Check type in unions:

```lumen
match value
  n: Int -> "int: {n}"
  s: String -> "string: {s}"
  b: Bool -> "bool: {b}"
end
```

### Guard Patterns

Add conditions:

```lumen
match n
  x if x < 0 -> "negative"
  x if x == 0 -> "zero"
  x if x < 10 -> "small positive"
  _ -> "large positive"
end
```

### Or Patterns

Match multiple patterns:

```lumen
match c
  "a" | "e" | "i" | "o" | "u" -> "vowel"
  _ -> "consonant"
end
```

### Nested Patterns

```lumen
match result
  Some(Ok(value)) -> "success: {value}"
  Some(Err(msg)) -> "failed: {msg}"
  None -> "nothing"
end
```

## Match Expression

Match returns a value:

```lumen
let label = match status
  Pending -> "pending"
  Active -> "active"
  Done -> "done"
end
```

Each arm must return the same type.

## Match Statement

Match can be used as a statement:

```lumen
match status
  Pending -> print("still waiting")
  Active -> print("now active")
  Done -> print("completed")
end
```

## Destructuring in let

Patterns work in `let` bindings to extract values from structured data in a single declaration.

### Tuple Destructuring

Bind each element of a tuple to a variable:

```lumen
let (a, b) = (1, 2)
let (x, y, z) = get_coordinates()
```

Nested tuple destructuring:

```lumen
let (name, (lat, lon)) = ("Office", (37.7749, -122.4194))
```

### List Destructuring

Bind list elements, optionally capturing the rest:

```lumen
let [first, ...rest] = [1, 2, 3, 4]
let [head, second, ...tail] = items
```

### Record Destructuring

Extract fields from a record by name. Use field punning (shorthand) when the binding name matches the field name:

```lumen
let Point(x:, y:) = origin        # field punning: binds x and y
let Point(x: px, y: py) = point   # rename: binds px and py
let User(name: n, ..) = user      # partial: binds n, ignores other fields
```

Record destructuring with positional syntax:

```lumen
let Point(x, y) = point
```

### Type Annotations

Destructured bindings can include type annotations:

```lumen
let (a: Int, b: String) = (42, "hello")
```

## Destructuring in for

```lumen
for (index, item) in enumerate(items)
  print("{index}: {item}")
end

for [key, value] in pairs
  print("{key} = {value}")
end
```

## Pattern Matching Rules

1. **Patterns are tried in order** — First match wins
2. **Matches must be exhaustive** — Use `_` for fallback
3. **Guards are evaluated after match** — Pattern must match first
4. **Bindings must be unique** — Can't bind same name twice

## Examples

### HTTP Status

```lumen
cell status_message(code: Int) -> String
  match code
    200 -> "OK"
    201 -> "Created"
    204 -> "No Content"
    400 -> "Bad Request"
    401 -> "Unauthorized"
    403 -> "Forbidden"
    404 -> "Not Found"
    500 -> "Internal Server Error"
    c if c >= 200 and c < 300 -> "Success"
    c if c >= 400 and c < 500 -> "Client Error"
    c if c >= 500 -> "Server Error"
    _ -> "Unknown"
  end
end
```

### JSON Navigation

```lumen
cell get_string(json: Json, key: String) -> String | Null
  match json
    obj: map[String, Json] ->
      match obj[key]
        s: String -> s
        _ -> null
      end
    _ -> null
  end
end
```

### Expression Evaluator

```lumen
enum Expr
  Num(value: Float)
  Add(left: Expr, right: Expr)
  Sub(left: Expr, right: Expr)
  Mul(left: Expr, right: Expr)
  Div(left: Expr, right: Expr)
end

cell eval(expr: Expr) -> Float
  match expr
    Num(n) -> n
    Add(l, r) -> eval(l) + eval(r)
    Sub(l, r) -> eval(l) - eval(r)
    Mul(l, r) -> eval(l) * eval(r)
    Div(l, r) -> eval(l) / eval(r)
  end
end
```

## Best Practices

1. **Order from specific to general** — Put `_` last
2. **Use guards for complex conditions** — Keep patterns simple
3. **Be exhaustive** — Cover all cases or use `_`
4. **Name bindings meaningfully** — `x` vs `user_id`

## Next Steps

- [Declarations](./declarations) — Records, enums, cells
- [Error Handling](../learn/tutorial/error-handling) — Using result types
