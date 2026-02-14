# Tutorial: Functions

Learn how to define and use functions (called "cells" in Lumen).

## Basic Functions

Functions in Lumen are called **cells**:

```lumen
cell greet(name: String) -> String
  return "Hello, {name}!"
end
```

### Calling Functions

```lumen
cell main() -> String
  let message = greet("World")
  return message
end
```

## Parameters

### Multiple Parameters

```lumen
cell add(a: Int, b: Int) -> Int
  return a + b
end

cell main() -> Int
  return add(3, 5)  # 8
end
```

### Default Values

```lumen
cell power(base: Int, exp: Int = 2) -> Int
  let mut result = 1
  for _ in 0..exp
    result *= base
  end
  return result
end

cell main() -> Int
  let squared = power(5)      # 25 (exp defaults to 2)
  let cubed = power(5, 3)     # 125
  return squared + cubed
end
```

### Named Arguments

Call with named arguments for clarity:

```lumen
cell create_user(name: String, email: String, active: Bool) -> String
  return "{name} ({email}) - Active: {active}"
end

cell main() -> String
  return create_user(
    name: "Alice",
    email: "alice@example.com",
    active: true
  )
end
```

## Return Types

### Explicit Return Type

```lumen
cell double(x: Int) -> Int
  return x * 2
end
```

### Void Functions

Use `Null` for functions that don't return a meaningful value:

```lumen
cell log(message: String) -> Null
  print(message)
  return null
end
```

### Multiple Return Types

```lumen
cell divide(a: Int, b: Int) -> Float | Null
  if b == 0
    return null
  end
  return a / b
end
```

## Effects

Declare side effects in the function signature:

```lumen
cell fetch_data(url: String) -> String / {http}
  # This function performs HTTP requests
  return "..."
end

cell log_event(event: String) -> Null / {trace}
  # This function emits trace events
  return null
end
```

Multiple effects:

```lumen
cell process_request(url: String) -> String / {http, trace}
  let data = fetch_data(url)
  log_event("processed {url}")
  return data
end
```

## Lambdas

Anonymous functions:

```lumen
cell main() -> Int
  let double = fn(x: Int) => x * 2
  let add = fn(a: Int, b: Int) => a + b
  
  return double(5) + add(3, 4)  # 10 + 7 = 17
end
```

Multi-line lambdas:

```lumen
cell main() -> Int
  let complex = fn(x: Int) -> Int
    let doubled = x * 2
    let squared = doubled * doubled
    return squared
  end
  
  return complex(3)  # (3 * 2)² = 36
end
```

## Pipe Operator

Chain function calls with `|>`:

```lumen
cell double(x: Int) -> Int
  return x * 2
end

cell add_one(x: Int) -> Int
  return x + 1
end

cell main() -> Int
  # Without pipe
  let result1 = add_one(double(5))
  
  # With pipe - more readable
  let result2 = 5 |> double() |> add_one()
  
  return result2  # 11
end
```

The value on the left becomes the first argument:

```lumen
cell greet(greeting: String, name: String) -> String
  return "{greeting}, {name}!"
end

cell main() -> String
  return "Alice" |> greet("Hello")  # "Hello, Alice!"
end
```

## Closures

Lambdas capture variables from their scope:

```lumen
cell make_adder(n: Int) -> fn(Int) -> Int
  return fn(x: Int) => x + n
end

cell main() -> Int
  let add5 = make_adder(5)
  let add10 = make_adder(10)
  
  return add5(3) + add10(3)  # 8 + 13 = 21
end
```

## Higher-Order Functions

Functions that take or return functions:

```lumen
cell apply_twice(f: fn(Int) -> Int, x: Int) -> Int
  return f(f(x))
end

cell main() -> Int
  let double = fn(x: Int) => x * 2
  return apply_twice(double, 3)  # double(double(3)) = 12
end
```

## Recursive Functions

```lumen
cell factorial(n: Int) -> Int
  if n <= 1
    return 1
  end
  return n * factorial(n - 1)
end

cell main() -> Int
  return factorial(5)  # 120
end
```

## Async Functions

Mark functions as async:

```lumen
async cell fetch_user(id: String) -> String
  # Async operations
  return "User {id}"
end
```

## Public Functions

Use `pub` to make functions accessible from other modules:

```lumen
pub cell helper() -> String
  return "I'm public"
end

cell internal() -> String
  return "I'm private"
end
```

## Practice Exercise

Create a collection of math utilities:

```lumen
cell abs(x: Int) -> Int
  if x < 0
    return -x
  end
  return x
end

cell max(a: Int, b: Int) -> Int
  if a > b
    return a
  end
  return b
end

cell min(a: Int, b: Int) -> Int
  if a < b
    return a
  end
  return b
end

cell clamp(value: Int, low: Int, high: Int) -> Int
  return max(low, min(value, high))
end

cell main() -> String
  let tests = [
    ("abs(-5)", abs(-5)),
    ("max(3, 7)", max(3, 7)),
    ("min(3, 7)", min(3, 7)),
    ("clamp(15, 0, 10)", clamp(15, 0, 10))
  ]
  
  let mut results = []
  for (desc, result) in tests
    results = results ++ ["{desc} = {result}"]
  end
  
  return results
end
```

## Next Steps

- [Pattern Matching](/learn/tutorial/pattern-matching) — Powerful data destructuring
- [Error Handling](/learn/tutorial/error-handling) — Working with results
