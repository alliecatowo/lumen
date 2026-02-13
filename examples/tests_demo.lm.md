# Test Demo

Demonstrates the Lumen test runner.

```lumen
cell test_arithmetic() -> Bool
  return 2 + 2 == 4
end

cell test_string_length() -> Bool
  return len("hello") == 5
end

cell test_list_operations() -> Bool
  let items = [1, 2, 3]
  return len(items) == 3
end

cell test_comparison() -> Bool
  return 10 > 5 and 3 < 7
end
```
