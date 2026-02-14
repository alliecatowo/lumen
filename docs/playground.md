---
layout: page
title: Playground
---

# Playground

Try Lumen directly in your browser. Edit the code and click **Run** to see the results.

<WasmPlayground />

## Quick Examples

<ExampleBlock title="Hello World" :initialOpen="true" :source="'cell main() -> String\n  return \"Hello, World!\"\nend'">

```lumen
cell main() -> String
  return "Hello, World!"
end
```

</ExampleBlock>

<ExampleBlock title="Fibonacci" :source="'cell fibonacci(n: Int) -> Int\n  if n <= 1\n    return n\n  end\n  return fibonacci(n - 1) + fibonacci(n - 2)\nend\n\ncell main() -> Int\n  return fibonacci(10)\nend'">

```lumen
cell fibonacci(n: Int) -> Int
  if n <= 1
    return n
  end
  return fibonacci(n - 1) + fibonacci(n - 2)
end

cell main() -> Int
  return fibonacci(10)
end
```

</ExampleBlock>

<ExampleBlock title="Pattern Matching" :source="'cell classify(n: Int) -> String\n  match n\n    0 -> return \"zero\"\n    1 -> return \"one\"\n    _ -> return \"many\"\n  end\nend\n\ncell main() -> String\n  return classify(5)\nend'">

```lumen
cell classify(n: Int) -> String
  match n
    0 -> return "zero"
    1 -> return "one"
    _ -> return "many"
  end
end

cell main() -> String
  return classify(5)
end
```

</ExampleBlock>

<ExampleBlock title="Records with Constraints" :source="'record Product\n  name: String where length(name) > 0\n  price: Float where price >= 0.0\nend\n\ncell total(products: list[Product]) -> Float\n  let sum = 0.0\n  for p in products\n    sum += p.price\n  end\n  return sum\nend\n\ncell main() -> Float\n  let items = [\n    Product(name: \"Apple\", price: 1.50),\n    Product(name: \"Banana\", price: 0.75)\n  ]\n  return total(items)\nend'">

```lumen
record Product
  name: String where length(name) > 0
  price: Float where price >= 0.0
end

cell total(products: list[Product]) -> Float
  let sum = 0.0
  for p in products
    sum += p.price
  end
  return sum
end

cell main() -> Float
  let items = [
    Product(name: "Apple", price: 1.50),
    Product(name: "Banana", price: 0.75)
  ]
  return total(items)
end
```

</ExampleBlock>

<ExampleBlock title="Error Handling" :source="'cell divide(a: Int, b: Int) -> result[Int, String]\n  if b == 0\n    return err(\"Division by zero\")\n  end\n  return ok(a / b)\nend\n\ncell main() -> String\n  match divide(10, 2)\n    ok(value) -> return \"Result: {value}\"\n    err(msg) -> return \"Error: {msg}\"\n  end\nend'">

```lumen
cell divide(a: Int, b: Int) -> result[Int, String]
  if b == 0
    return err("Division by zero")
  end
  return ok(a / b)
end

cell main() -> String
  match divide(10, 2)
    ok(value) -> return "Result: {value}"
    err(msg) -> return "Error: {msg}"
  end
end
```

</ExampleBlock>

## Running Locally

To run Lumen on your machine:

```bash
# Install
cargo install lumen-lang

# Create a file
cat > hello.lm.md << 'EOF'
cell main() -> String
  return "Hello, World!"
end
EOF

# Run it
lumen run hello.lm.md
```

## More Examples

- [Hello World](./examples/hello-world) — Basic program
- [AI Chat](./examples/ai-chat) — AI-powered chatbot
