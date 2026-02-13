@lumen 1
@package "hello"

# Hello World

A simple hello-world program in Lumen.

```lumen
record Greeting
  message: String
  recipient: String
end
```

## The greet cell

```lumen
cell greet(name: String) -> Greeting
  let msg = "Hello, " + name + "!"
  return Greeting(message: msg, recipient: name)
end
```

## Entry point

```lumen
cell main() -> Int
  let g = greet("World")
  return 0
end
```
