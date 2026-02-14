# Syntactic Sugar Examples

This file demonstrates Lumen's syntactic sugar features that make the language feel amazing to write.

## 1. Pipe Operator |>

The pipe operator `|>` allows for readable data transformation chains. The value on the left becomes the first argument to the function call on the right.

```lumen
cell double(x: Int) -> Int
  return x * 2
end

cell add(a: Int, b: Int) -> Int
  return a + b
end

cell square(x: Int) -> Int
  return x * x
end

cell test_pipes() -> Int
  # Basic pipe: value becomes first argument
  let a = 5 |> double()          # double(5) = 10

  # Chain multiple transformations
  let b = 5 |> double() |> square()   # square(double(5)) = 100

  # Pipe into functions with multiple arguments
  let c = 5 |> add(3)            # add(5, 3) = 8

  # Complex chains
  let d = 10 |> double() |> add(5) |> square()  # square(add(double(10), 5)) = 625

  if a == 10 and b == 100 and c == 8 and d == 625
    return 0
  else
    return 1
  end
end
```

## 2. String Interpolation

Embed expressions directly in strings using `{expression}` syntax.

```lumen
cell test_interpolation() -> String
  let name = "Alice"
  let age = 30
  let score = 95.5

  # Simple variable interpolation
  let msg1 = "Hello, {name}!"

  # Multiple interpolations
  let msg2 = "{name} is {age} years old"

  # Expression interpolation
  let msg3 = "Next year, {name} will be {age + 1}"

  # Works with all types
  let msg4 = "Score: {score}"

  return msg2
end
```

## 3. Range Expressions

Concise syntax for creating numeric ranges.

```lumen
cell test_ranges() -> Int
  let sum = 0

  # Exclusive range: 1..5 means [1, 2, 3, 4]
  for i in 1..5
    sum = sum + i
  end
  # sum is now 10 (1+2+3+4)

  # Inclusive range: 1..=5 means [1, 2, 3, 4, 5]
  let sum2 = 0
  for i in 1..=5
    sum2 = sum2 + i
  end
  # sum2 is now 15 (1+2+3+4+5)

  return sum + sum2
end
```

## Main Test Runner

```lumen
cell main() -> Int
  print("Testing Lumen syntactic sugar features...")

  let pipe_result = test_pipes()
  if pipe_result == 0
    print("✓ Pipe operator tests passed")
  else
    print("✗ Pipe operator tests failed")
  end

  let interp_result = test_interpolation()
  if interp_result == "Alice is 30 years old"
    print("✓ String interpolation tests passed")
  else
    print("✗ String interpolation tests failed")
  end

  let range_result = test_ranges()
  if range_result == 25
    print("✓ Range expression tests passed")
  else
    print("✗ Range expression tests failed")
  end

  if pipe_result == 0
    print("All syntactic sugar tests passed!")
    return 0
  else
    return 1
  end
end
```
