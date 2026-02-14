# Language Tour

Lumen is a statically typed language with modern primitives plus AI-native constructs.

## Cells (functions)

```lumen
cell greet(name: String) -> String
  return "Hello, {name}!"
end
```

## Variables and Mutability

Variables are immutable by default. Use `mut` for mutable bindings:

```lumen
cell main() -> Int
  let x = 10              # immutable
  let mut y = 0            # mutable
  y += x
  return y
end
```

## Flow Control

```lumen
cell classify(n: Int) -> String
  if n == 0
    return "zero"
  end

  match n
    1 -> return "one"
    _ -> return "many"
  end
end
```

### Inline if-then-else

```lumen
cell abs(n: Int) -> Int
  return if n >= 0 then n else -n
end
```

## Records + Invariants

```lumen
record Invoice
  subtotal: Float where subtotal >= 0.0
  tax: Float where tax >= 0.0
  total: Float where total == subtotal + tax
end
```

### Property Shorthand

When constructing a record, if a variable name matches the field name you can omit the colon:

```lumen
record Point
  x: Int
  y: Int
end

cell main() -> Int
  let x = 3
  let y = 4
  let p = Point(x, y)    # shorthand for Point(x: x, y: y)
  return p.x + p.y
end
```

## Enums + Pattern Matching

```lumen
enum Status
  Pending
  Active
  Complete
end

cell label(status: Status) -> String
  match status
    Pending -> return "pending"
    Active -> return "active"
    Complete -> return "complete"
  end
end
```

### Match Exhaustiveness

The compiler checks that `match` on enum types covers all variants. Missing variants produce a compile-time error:

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
    Blue -> return "blue"   # all variants covered
  end
end
```

A wildcard `_` or catch-all identifier makes any match exhaustive. Guard patterns are treated conservatively and do not contribute to exhaustiveness coverage.

## Optional Types (`T?`)

`T?` is syntactic sugar for `T | Null`:

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

## Type Test and Cast (`is` / `as`)

```lumen
cell main() -> Bool
  let v: Int | String = 42
  return v is Int          # true
end
```

`expr as Type` casts a value to the target type.

## Null Safety Operators

```lumen
record Box
  value: Int
end

cell main() -> Int
  let b: Box | Null = Box(value: 7)
  let v = b?.value         # null-safe field access
  let w = b!               # null assert (errors if null)
  return v ?? 0            # null coalescing
end
```

- `?.` -- null-safe field access
- `?[]` -- null-safe index access
- `??` -- null coalescing (default if null)
- `!` -- null assert (unwrap or error)

## Floor Division (`//`)

Integer division that truncates toward negative infinity:

```lumen
cell main() -> Int
  return 7 // 2            # 3
end
```

## Shift and Bitwise Operators

```lumen
cell main() -> Int
  let a = 1 << 4           # 16 (left shift)
  let b = 32 >> 2          # 8 (right shift)
  let c = 0xFF & 0x0F      # 15 (bitwise AND)
  let d = 0x0F | 0xF0      # 255 (bitwise OR)
  let e = 0xFF ^ 0x0F      # 240 (bitwise XOR)
  return a + b
end
```

## Compound Assignment

All compound assignment operators:

```lumen
cell main() -> Int
  let mut x = 10
  x += 5       # add-assign
  x -= 2       # sub-assign
  x *= 3       # mul-assign
  x //= 4      # floor-div-assign
  x %= 7       # mod-assign
  x **= 2      # power-assign
  x &= 0xFF    # bitwise-and-assign
  x |= 0x01    # bitwise-or-assign
  x ^= 0x10    # bitwise-xor-assign
  return x
end
```

## Pipe Operator (`|>`)

The value on the left becomes the first argument to the call on the right:

```lumen
cell double(x: Int) -> Int
  return x * 2
end

cell add(a: Int, b: Int) -> Int
  return a + b
end

cell main() -> Int
  let result = 5 |> double() |> add(3)   # add(double(5), 3) = 13
  return result
end
```

## String Interpolation

Embed expressions in strings with `{expr}`:

```lumen
cell main() -> String
  let name = "Alice"
  let age = 30
  return "Hello, {name}! Next year you'll be {age + 1}."
end
```

## Range Expressions

```lumen
cell main() -> Int
  let mut sum = 0
  for i in 1..5       # exclusive: [1, 2, 3, 4]
    sum += i
  end
  for i in 1..=5      # inclusive: [1, 2, 3, 4, 5]
    sum += i
  end
  return sum
end
```

## Labeled Loops

Loops can be labeled with `@name`. `break` and `continue` can target a label to control nested loops:

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

## For-Loop Filters

`for` loops support an optional `if` clause that skips iterations where the condition is false:

```lumen
cell main() -> Int
  let mut sum = 0
  for x in 1..=10 if x % 2 == 0
    sum += x              # sums even numbers: 2+4+6+8+10 = 30
  end
  return sum
end
```

## Destructuring Let

Destructure tuples, lists, and records directly in `let` bindings:

```lumen
cell main() -> Int
  # Tuple destructuring
  let (a, b) = (10, 20)

  # List destructuring
  let [x, y, ...rest] = [1, 2, 3, 4]

  return a + b + x + y
end
```

## Defer Blocks

`defer` schedules a block of code to run when the enclosing scope exits:

```lumen
cell process() -> Int
  defer
    print("cleanup done")
  end
  print("working...")
  return 42
end
```

## Variadic Parameters (Syntax)

Cells accept `...` parameter syntax for variadic inputs:

```lumen
cell log(...parts: list[String]) -> Null
  print(parts)
  return null
end
```

The parser records the variadic flag in the AST. Full variadic expansion behavior is still being completed in type/lowering paths.

## Lambdas

```lumen
cell main() -> Int
  # Expression body
  let double = fn(x: Int) -> Int => x * 2

  # Block body
  let complex = fn(x: Int) -> Int
    let y = x * 2
    return y + 1
  end

  return double(5) + complex(3)
end
```

## Effects

Effects are explicit in signatures:

```lumen
cell fetch_text(url: String) -> String / {http}
  return "..."
end
```

## Markdown-Native Source

Lumen is markdown-native by default (`.lm.md`), and also supports raw `.lm` source files as first-class input.

Code can live directly in markdown:

````markdown
# My Program Notes

```lumen
cell main() -> Null
  print("code and docs stay together")
  return null
end
```
````

## Continue

- Browser runtime path: [Browser WASM Guide](/guide/wasm-browser)
- AI-native details: [AI-Native Features](/language/ai-native)
- CLI commands: [CLI Reference](/guide/cli)
- Runtime details: [Runtime Model](/RUNTIME)
