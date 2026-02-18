# JIT Collection Debug Test

```lumen
cell test_list() -> list[Int]
  let xs = [1, 2, 3]
  return xs
end

cell test_map() -> map[String, Int]
  let m = {"a": 1, "b": 2}
  return m
end

cell main() -> Int
  let xs = test_list()
  print(xs)
  let m = test_map()
  print(m)
  let list_len = length(xs)
  print(list_len)
  let map_len = length(m)
  print(map_len)
  return list_len + map_len
end
```
