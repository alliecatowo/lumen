---
layout: page
title: Playground
---

# Playground

Try Lumen directly in your browser. Edit the code and click **Run** to see the results.

<WasmPlayground />

## Quick Examples

<ExampleBlock title="Hello World" :initialOpen="true">

```lumen
cell main() -> String
  return "Hello, World!"
end
```

</ExampleBlock>

<ExampleBlock title="Fibonacci">

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

<ExampleBlock title="Pattern Matching">

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

<ExampleBlock title="Records with Constraints">

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

<ExampleBlock title="Error Handling">

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
