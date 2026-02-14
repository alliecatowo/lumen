# Language Tour

Lumen is a statically typed language with modern primitives plus AI-native constructs.

## Cells (functions)

```lumen
cell greet(name: String) -> String
  return "Hello, {name}!"
end
```

## Flow control

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

## Records + invariants

```lumen
record Invoice
  subtotal: Float where subtotal >= 0.0
  tax: Float where tax >= 0.0
  total: Float where total == subtotal + tax
end
```

## Enums + pattern matching

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

## Effects

Effects are explicit in signatures:

```lumen
cell fetch_text(url: String) -> String / {http}
  return "..."
end
```

## Markdown-native source

Lumen code can live directly in markdown:

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
- CLI commands: [CLI Reference](/CLI)
- Runtime details: [Runtime Model](/RUNTIME)
