# Examples: Hello World

The classic first program in Lumen.

## Basic Hello World

```lumen
# hello.lm.md

cell main() -> String
  return "Hello, World!"
end
```

Run it:

```bash
lumen run hello.lm.md
```

Output:
```
Hello, World!
```

## With Greeting Function

```lumen
cell greet(name: String) -> String
  return "Hello, {name}!"
end

cell main() -> String
  return greet("Lumen")
end
```

## Interactive with Input

```lumen
cell main() -> Null
  print("What is your name?")
  # In a real scenario, you'd read input
  let name = "World"
  print("Hello, {name}!")
  return null
end
```

## Markdown Documentation

````markdown
# Hello World Example

This is the classic first program.

```lumen
cell main() -> String
  return "Hello, World!"
end
```

## How It Works

1. We define a `cell` (function) called `main`
2. It returns a `String`
3. The return value is printed automatically
````

## Multiple Greetings

```lumen
cell main() -> String
  let greetings = [
    greet("World"),
    greet("Lumen"),
    greet("AI")
  ]
  return join(greetings, "\n")
end

cell greet(name: String) -> String
  return "Hello, {name}!"
end
```

Output:
```
Hello, World!
Hello, Lumen!
Hello, AI!
```

## Next Example

[Calculator](/examples/calculator) — Simple arithmetic
