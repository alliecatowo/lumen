# Missing Import Test

This tries to import a non-existent module.

```lumen
import nonexistent_module: *
```

```lumen
cell main() -> String
  return "Hello"
end
```
