# Tutorial: Control Flow

Learn how to control the execution path of your programs.

## Conditionals

### If Statements

```lumen
cell classify(n: Int) -> String
  if n < 0
    return "negative"
  end
  
  if n == 0
    return "zero"
  end
  
  return "positive"
end
```

### If-Else

```lumen
cell describe(n: Int) -> String
  if n > 0
    return "positive"
  else
    return "not positive"
  end
end
```

### If-Else If-Else

```lumen
cell grade(score: Int) -> String
  if score >= 90
    return "A"
  else if score >= 80
    return "B"
  else if score >= 70
    return "C"
  else if score >= 60
    return "D"
  else
    return "F"
  end
end
```

### If Expressions

For simple cases, use inline `if-then-else`:

```lumen
let max = if a > b then a else b
let sign = if x >= 0 then 1 else -1
```

## Loops

### For Loops

Iterate over collections:

```lumen
let fruits = ["apple", "banana", "cherry"]

for fruit in fruits
  print(fruit)
end
```

Iterate with index using tuple destructuring:

```lumen
let items = ["a", "b", "c"]

for (index, item) in enumerate(items)
  print("{index}: {item}")
end
```

### Range Loops

```lumen
# Exclusive range (1, 2, 3, 4)
for i in 1..5
  print(i)
end

# Inclusive range (1, 2, 3, 4, 5)
for i in 1..=5
  print(i)
end

# With step
for i in 0..10 step 2
  print(i)  # 0, 2, 4, 6, 8
end
```

### While Loops

```lumen
let mut count = 0

while count < 5
  print(count)
  count += 1
end
```

### Loop (Infinite)

```lumen
let mut x = 0

loop
  x += 1
  if x > 10
    break
  end
end
```

### Break and Continue

```lumen
# Skip even numbers
for i in 1..10
  if i % 2 == 0
    continue
  end
  print(i)  # 1, 3, 5, 7, 9
end

# Exit early
for i in 1..100
  if i > 5
    break
  end
  print(i)  # 1, 2, 3, 4, 5
end
```

## Match Expressions

Pattern matching is powerful for branching:

```lumen
cell http_status(code: Int) -> String
  match code
    200 -> return "OK"
    201 -> return "Created"
    404 -> return "Not Found"
    500 -> return "Server Error"
    _ -> return "Unknown"
  end
end
```

### Match with Guards

```lumen
cell categorize(n: Int) -> String
  match n
    x if x < 0 -> return "negative"
    x if x == 0 -> return "zero"
    x if x < 10 -> return "small positive"
    _ -> return "large positive"
  end
end
```

### Match on Enums

```lumen
enum Color
  Red
  Green
  Blue
end

cell to_hex(color: Color) -> String
  match color
    Red -> return "#FF0000"
    Green -> return "#00FF00"
    Blue -> return "#0000FF"
  end
end
```

## Early Return

Functions can return early:

```lumen
cell find_first_even(numbers: list[Int]) -> Int | Null
  for n in numbers
    if n % 2 == 0
      return n
    end
  end
  return null
end
```

## Halt

`halt` stops the entire program:

```lumen
cell main() -> Null
  let config = load_config()
  
  if config == null
    halt("Configuration not found")
  end
  
  # Continue...
  return null
end
```

## Practice Exercise

Create a FizzBuzz implementation:

```lumen
cell fizzbuzz(n: Int) -> String
  if n % 15 == 0
    return "FizzBuzz"
  else if n % 3 == 0
    return "Fizz"
  else if n % 5 == 0
    return "Buzz"
  else
    return "{n}"
  end
end

cell main() -> Null
  for i in 1..=30
    print(fizzbuzz(i))
  end
  return null
end
```

## Next Steps

- [Data Structures](/learn/tutorial/data-structures) — Records and enums
- [Functions](/learn/tutorial/functions) — Define reusable code
