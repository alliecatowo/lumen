# Tutorial: Pattern Matching

Pattern matching lets you destructure and inspect data elegantly.

## Basic Patterns

### Literal Patterns

Match exact values:

```lumen
cell describe(n: Int) -> String
  match n
    0 -> return "zero"
    1 -> return "one"
    2 -> return "two"
    _ -> return "many"  # Wildcard (default)
  end
end
```

### Boolean Patterns

```lumen
cell bool_word(b: Bool) -> String
  match b
    true -> return "yes"
    false -> return "no"
  end
end
```

## Binding Patterns

Capture the matched value:

```lumen
cell sign(n: Int) -> String
  match n
    0 -> return "zero"
    x if x > 0 -> return "positive: {x}"
    x -> return "negative: {x}"
  end
end
```

## Guards

Add conditions with `if`:

```lumen
cell categorize(n: Int) -> String
  match n
    x if x < 0 -> return "negative"
    x if x == 0 -> return "zero"
    x if x < 10 -> return "single digit"
    x if x < 100 -> return "double digit"
    _ -> return "large"
  end
end
```

## Destructuring

### Tuples

```lumen
cell describe_point(point: tuple[Int, Int]) -> String
  match point
    (0, 0) -> return "origin"
    (x, 0) -> return "on x-axis at {x}"
    (0, y) -> return "on y-axis at {y}"
    (x, y) -> return "at ({x}, {y})"
  end
end
```

### Lists

```lumen
cell describe_list(items: list[Int]) -> String
  match items
    [] -> return "empty"
    [x] -> return "single element: {x}"
    [x, y] -> return "two elements: {x} and {y}"
    [first, ...rest] -> return "first is {first}, {length(rest)} more"
  end
end
```

### Records

```lumen
record Point
  x: Int
  y: Int
end

cell quadrant(point: Point) -> String
  match point
    Point(x: 0, y: 0) -> return "origin"
    Point(x: x, y: y) if x > 0 and y > 0 -> return "Q1"
    Point(x: x, y: y) if x < 0 and y > 0 -> return "Q2"
    Point(x: x, y: y) if x < 0 and y < 0 -> return "Q3"
    Point(x: x, y: y) if x > 0 and y < 0 -> return "Q4"
    _ -> return "on axis"
  end
end
```

Shorthand with `..` to ignore other fields:

```lumen
record User
  id: Int
  name: String
  email: String
  active: Bool
end

cell check_status(user: User) -> String
  match user
    User(active: true, ..) -> return "Active user"
    User(active: false, ..) -> return "Inactive user"
  end
end
```

## Enum Patterns

### Simple Enums

```lumen
enum Color
  Red
  Green
  Blue
end

cell hex(color: Color) -> String
  match color
    Red -> return "#FF0000"
    Green -> return "#00FF00"
    Blue -> return "#0000FF"
  end
end
```

### Enums with Data

```lumen
enum Result
  Ok(value: Int)
  Err(message: String)
end

cell handle(result: Result) -> String
  match result
    Ok(value) -> return "Success: {value}"
    Err(msg) -> return "Error: {msg}"
  end
end
```

### Nested Enum Patterns

```lumen
enum Option
  Some(value: Int)
  None
end

enum Result
  Ok(opt: Option)
  Err(msg: String)
end

cell extract(result: Result) -> Int
  match result
    Ok(Some(value)) -> return value
    Ok(None) -> return 0
    Err(_) -> return -1
  end
end
```

## Or Patterns

Match multiple patterns:

```lumen
cell is_vowel(c: String) -> Bool
  match c
    "a" | "e" | "i" | "o" | "u" -> return true
    _ -> return false
  end
end
```

## Type Patterns

Check types in unions:

```lumen
cell describe(value: Int | String | Bool) -> String
  match value
    n: Int -> return "Integer: {n}"
    s: String -> return "String: {s}"
    b: Bool -> return "Boolean: {b}"
  end
end
```

## Wildcard Pattern

The `_` pattern matches anything:

```lumen
cell first_digit(n: Int) -> Int
  match n
    0 -> return 0
    1 -> return 1
    _ -> return 9  # Everything else
  end
end
```

Use `_` to ignore parts:

```lumen
cell second(items: list[Int]) -> Int | Null
  match items
    [_, x, ...] -> return x
    _ -> return null
  end
end
```

## Match Expressions

Match can be used as an expression:

```lumen
cell grade_label(score: Int) -> String
  let label = match score
    s if s >= 90 -> "A"
    s if s >= 80 -> "B"
    s if s >= 70 -> "C"
    s if s >= 60 -> "D"
    _ -> "F"
  end
  return label
end
```

## Complex Example

A calculator with pattern matching:

```lumen
enum Expr
  Number(value: Float)
  Add(left: Expr, right: Expr)
  Subtract(left: Expr, right: Expr)
  Multiply(left: Expr, right: Expr)
  Divide(left: Expr, right: Expr)
end

cell evaluate(expr: Expr) -> Float | Null
  match expr
    Number(n) -> return n
    Add(left, right) -> return (evaluate(left) ?? 0.0) + (evaluate(right) ?? 0.0)
    Subtract(left, right) -> return (evaluate(left) ?? 0.0) - (evaluate(right) ?? 0.0)
    Multiply(left, right) -> return (evaluate(left) ?? 0.0) * (evaluate(right) ?? 0.0)
    Divide(left, right) ->
      let r = evaluate(right) ?? 0.0
      if r == 0.0
        return null
      end
      return (evaluate(left) ?? 0.0) / r
  end
end

cell main() -> String
  let expr = Add(
    Number(5.0),
    Multiply(Number(3.0), Number(4.0))
  )
  
  match evaluate(expr)
    n: Float -> return "Result: {n}"
    null -> return "Evaluation error"
  end
end
```

## Match Exhaustiveness

The compiler checks that match statements on enum subjects cover all variants. Missing variants produce compile errors:

```lumen
enum Direction
  North
  South
  East
  West
end

cell label(d: Direction) -> String
  match d
    North -> return "up"
    South -> return "down"
    # Compile error: IncompleteMatch — missing East, West
  end
end
```

A wildcard `_` or catch-all identifier pattern makes any match exhaustive. Guard patterns do not contribute to exhaustiveness coverage.

## Let-Destructuring

Patterns can be used in `let` bindings to extract values directly, without a `match` statement.

### Tuple Destructuring

```lumen
cell get_point() -> tuple[Int, Int]
  return (3, 7)
end

cell main() -> String
  let (x, y) = get_point()
  return "x={x}, y={y}"
end
```

### Record Destructuring

Use field punning (shorthand) when the binding name matches the field name:

```lumen
record Point
  x: Int
  y: Int
end

let origin = Point(x: 0, y: 0)
let Point(x:, y:) = origin     # binds x = 0, y = 0
print("({x}, {y})")           # "(0, 0)"
```

Rename fields during destructuring:

```lumen
let Point(x: px, y: py) = origin
print("px={px}, py={py}")
```

### Practical Example

Destructuring is useful for working with functions that return structured data:

```lumen
record ParseResult
  tokens: list[String]
  errors: list[String]
  ok: Bool
end

cell process(input: String) -> String
  let ParseResult(tokens:, errors:, ok:) = parse(input)
  
  if not ok
    return "Parse failed: {join(errors, ", ")}"
  end
  
  return "Got {length(tokens)} tokens"
end
```

## Best Practices

1. **Be exhaustive** — Cover all cases or use `_` as fallback; the compiler checks enum coverage
2. **Order matters** — First matching pattern wins
3. **Use guards for complex conditions** — Keep patterns simple
4. **Destructure in the pattern** — Don't extract then check
5. **Use let-destructuring for known shapes** — When the structure is guaranteed, destructure in `let` instead of `match`

## Next Steps

- [Error Handling](./error-handling) — Using result types
- [AI-Native Tutorial](../ai-native/tools) — Tools and grants
