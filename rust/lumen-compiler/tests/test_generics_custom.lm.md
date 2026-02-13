# Test Custom Generic Types

```lumen
record Box[T]
  value: T
end

record Pair[A, B]
  first: A
  second: B
end

cell test_box_int() -> Box[Int]
  return Box[Int] { value: 42 }
end

cell test_pair_string_int() -> Pair[String, Int]
  return Pair[String, Int] { first: "count", second: 10 }
end

cell main() -> Int
  let b = test_box_int()
  let p = test_pair_string_int()
  return 0
end
```
