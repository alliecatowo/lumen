# Module B

This module imports A, which imports B, creating a circular dependency.

```lumen
import circular_a: *
```

```lumen
cell function_b() -> String
  return "B"
end
```
