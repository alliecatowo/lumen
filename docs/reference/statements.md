# Statements

Statements perform actions. Unlike expressions, they don't produce values.

## Variable Declarations

### let

Declare an immutable variable:

```lumen
let x = 42
let name = "Alice"
let pair = (1, 2)
```

### let mut

Declare a mutable variable:

```lumen
let mut counter = 0
counter = counter + 1
counter += 1
```

### Type Annotations

```lumen
let count: Int = 10
let message: String = "hello"
let items: list[Int] = [1, 2, 3]
```

### Destructuring

```lumen
let (a, b) = (1, 2)          # Tuple
let [first, second] = [1, 2]  # List
let Point(x, y) = point      # Record
```

## Assignments

```lumen
x = 42
user.name = "Alice"
items[0] = 100
```

### Compound Assignment

```lumen
x += 1      # x = x + 1
x -= 1      # x = x - 1
x *= 2      # x = x * 2
x /= 2      # x = x / 2
x //= 2     # x = floor(x / 2)
x %= 2
x **= 2
x &= 0xFF
x |= 0x01
x ^= 0x10
```

## Conditionals

### if

```lumen
if x > 0
  print("positive")
end
```

### if-else

```lumen
if x > 0
  print("positive")
else
  print("not positive")
end
```

### if-else if-else

```lumen
if score >= 90
  grade = "A"
else if score >= 80
  grade = "B"
else if score >= 70
  grade = "C"
else
  grade = "F"
end
```

### if let

```lumen
if let Some(value) = maybe
  print(value)
end
```

## Loops

### for

```lumen
for item in items
  print(item)
end
```

Labeled for-loop:

```lumen
for @outer item in items
  if item == "skip-all"
    continue @outer
  end
end
```

With index:

```lumen
for (index, item) in enumerate(items)
  print("{index}: {item}")
end
```

With filter:

```lumen
for x in numbers if x > 0
  print(x)
end
```

### while

```lumen
while count < 10
  print(count)
  count += 1
end
```

Labeled while-loop:

```lumen
while @retry should_continue()
  if done()
    break @retry
  end
end
```

### while let

```lumen
while let Some(item) = next()
  process(item)
end
```

### loop

```lumen
loop
  let input = read()
  if input == "quit"
    break
  end
  process(input)
end
```

Labeled loop:

```lumen
loop @main
  if should_stop()
    break @main
  end
end
```

### break

Exit a loop:

```lumen
for x in items
  if x == target
    break
  end
end
```

Target a label in nested loops:

```lumen
break @outer
```

With value (for loop expressions):

```lumen
let found = for x in items
  if x == target
    break x
  end
end
```

### continue

Skip to next iteration:

```lumen
for x in items
  if x % 2 == 0
    continue
  end
  print(x)
end
```

Target a label:

```lumen
continue @outer
```

## defer

`defer` schedules statements to run when the current scope exits.

```lumen
cell run() -> Int
  defer
    print("cleanup")
  end

  print("work")
  return 1
end
```

## Match

```lumen
match value
  0 -> print("zero")
  1 -> print("one")
  _ -> print("many")
end
```

With guards:

```lumen
match n
  x if x < 0 -> print("negative")
  x if x == 0 -> print("zero")
  _ -> print("positive")
end
```

With bindings:

```lumen
match result
  ok(value) -> print(value)
  err(msg) -> print("Error: {msg}")
end
```

## return

Return from a cell:

```lumen
cell add(a: Int, b: Int) -> Int
  return a + b
end
```

Early return:

```lumen
cell find(items: list[Int], target: Int) -> Int | Null
  for item in items
    if item == target
      return item
    end
  end
  return null
end
```

## halt

Stop the entire program:

```lumen
cell main() -> Null
  let config = load_config()
  
  if config == null
    halt("Configuration required")
  end
  
  run()
  return null
end
```

## emit

Emit a trace event:

```lumen
cell process(data: String) -> String / {emit}
  emit("Processing: {data}")
  return transform(data)
end
```

## Expression Statements

Expressions can be statements when their result is ignored:

```lumen
print("hello")        # Function call as statement
items.append(1)       # Method call as statement
spawn(background())   # Spawn as statement
```

## Statement Blocks

Groups of statements:

```lumen
cell example() -> Int
  let a = 1
  let b = 2
  
  if a > 0
    let c = a + b
    return c
  end
  
  return 0
end
```

## Control Flow Summary

| Statement | Purpose |
|-----------|---------|
| `if` / `else` | Conditional execution |
| `for` | Iterate over collections |
| `while` | Loop while condition is true |
| `loop` | Infinite loop (use with `break`) |
| `match` | Pattern-based branching |
| `return` | Exit function with value |
| `break` | Exit loop |
| `continue` | Skip to next iteration |
| `defer` | Run cleanup code on scope exit |
| `halt` | Stop program |

## Next Steps

- [Pattern Matching](./patterns) — Detailed pattern reference
- [Declarations](./declarations) — Top-level declarations
