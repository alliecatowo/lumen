# JIT Collection Test

```lumen
cell test_list() -> Int
  let xs = [1, 2, 3]
  return length(xs)
end

cell test_map() -> Int
  let m = {"a": 1, "b": 2}
  return length(m)
end

cell main() -> Int
  let list_len = test_list()
  let map_len = test_map()
  return list_len + map_len
end
```

This test creates a list with 3 elements and a map with 2 entries, then returns their combined length (3 + 2 = 5).
