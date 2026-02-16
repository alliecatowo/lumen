# Algebraic Effects

Lumen has full algebraic effects — a powerful mechanism for structured side effects.

## What Are Effects?

Instead of directly performing I/O, code "performs" an effect operation. An enclosing handler intercepts it and decides what to do.

## Performing Effects

```lumen
cell fetch_data(url: String) -> String / {http}
  let response = perform http.get(url)
  return response
end
```

## Handling Effects

```lumen
cell main() -> String
  let result = handle
    fetch_data("https://example.com")
  with
    http.get(url) -> resume("mock response")
  end
  return result
end
```

## Why Effects?

- **Testability**: Mock any side effect by swapping the handler
- **Composability**: Effects compose naturally without callback hell
- **Explicitness**: Cell signatures declare which effects they perform
- **Control**: Handlers can resume, abort, or transform results
