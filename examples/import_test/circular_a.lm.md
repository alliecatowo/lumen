# Module A

This module imports B, which imports A, creating a circular dependency.

```lumen
import circular_b: *
```

```lumen
cell function_a() -> String
  return "A"
end
```
