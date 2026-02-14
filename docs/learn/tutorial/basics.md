# Tutorial: Basics

Learn the fundamental building blocks of Lumen.

## Comments

```lumen
# This is a single-line comment

# Comments can explain
# multiple lines
```

## Variables

Use `let` to declare variables:

```lumen
let x = 42              # Type inferred as Int
let name = "Lumen"      # Type inferred as String
let pi = 3.14159        # Type inferred as Float
let active = true       # Type inferred as Bool
```

### Mutable Variables

By default, variables are immutable. Add `mut` to allow changes:

```lumen
let mut counter = 0
counter = counter + 1   # OK
counter += 1            # Also OK (compound assignment)
```

### Type Annotations

Explicit types are optional but can be added:

```lumen
let count: Int = 10
let message: String = "Hello"
```

## Basic Types

| Type | Example | Description |
|------|---------|-------------|
| `Int` | `42`, `-7` | 64-bit signed integer |
| `Float` | `3.14`, `-0.5` | 64-bit floating point |
| `Bool` | `true`, `false` | Boolean |
| `String` | `"hello"` | UTF-8 string |
| `Null` | `null` | Null value |

## Strings

### String Literals

```lumen
let single = "Hello, World!"
let multi = """
  This is a
  multi-line string
"""
```

### String Interpolation

Embed expressions in strings with `{}`:

```lumen
let name = "Alice"
let age = 30

let greeting = "Hello, {name}!"           # "Hello, Alice!"
let info = "{name} is {age} years old"    # "Alice is 30 years old"
let math = "2 + 2 = {2 + 2}"              # "2 + 2 = 4"
```

### Raw Strings

For strings with backslashes:

```lumen
let path = r"C:\Users\name"     # Backslashes preserved
let regex = r"\d+\.\d+"         # No escape needed
```

### String Concatenation

```lumen
let first = "Hello"
let second = "World"
let combined = first ++ " " ++ second  # "Hello World"
```

## Arithmetic

```lumen
let a = 10 + 5     # 15 (addition)
let b = 10 - 3     # 7  (subtraction)
let c = 4 * 3      # 12 (multiplication)
let d = 10 / 4     # 2  (integer division)
let e = 10.0 / 4.0 # 2.5 (float division)
let f = 10 % 3     # 1  (modulo)
let g = 2 ** 10    # 1024 (exponentiation)
```

## Comparison

```lumen
let eq = 5 == 5     # true
let ne = 5 != 3     # true
let lt = 3 < 5      # true
let gt = 5 > 3      # true
let le = 5 <= 5     # true
let ge = 5 >= 5     # true
```

## Boolean Logic

```lumen
let a = true and false   # false
let b = true or false    # true
let c = not true         # false
```

## Null Safety

```lumen
let maybe: Int | Null = 42

# Safe access with ?.
let value = maybe?.abs()  # 42 if maybe is Int, null otherwise

# Default with ??
let or_default = maybe ?? 0  # 42 (use maybe, or 0 if null)
```

## Collections

### Lists

```lumen
let numbers = [1, 2, 3, 4, 5]
let empty: list[Int] = []

let first = numbers[0]      # 1
let len = length(numbers)   # 5
```

### Maps

```lumen
let scores = {"alice": 95, "bob": 87}
let empty: map[String, Int] = {}

let alice_score = scores["alice"]  # 95
```

### Sets

```lumen
let unique = {1, 2, 3}           # Set literal
let empty: set[Int] = set[]()

let has_two = 2 in unique        # true
```

### Tuples

```lumen
let pair = (1, "hello")
let triple = (1, 2, 3)

let first = pair[0]  # 1
let second = pair[1] # "hello"
```

## Practice Exercise

Create a program that:
1. Defines a list of numbers
2. Calculates their sum
3. Returns a message with the result

```lumen
cell main() -> String
  let numbers = [1, 2, 3, 4, 5]
  let mut sum = 0
  
  for n in numbers
    sum += n
  end
  
  return "Sum of {length(numbers)} numbers is {sum}"
end
```

## Next Steps

- [Control Flow](/learn/tutorial/control-flow) — Conditionals and loops
- [Data Structures](/learn/tutorial/data-structures) — Records and enums
