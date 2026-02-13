# Test Generics

```lumen
cell test_list_int() -> list[Int]
  let items: list[Int] = [1, 2, 3]
  return items
end

cell test_map_string_int() -> map[String, Int]
  let scores: map[String, Int] = {"alice": 100, "bob": 85}
  return scores
end

cell test_result_ok() -> result[String, String]
  return ok("success")
end

cell test_result_err() -> result[Int, String]
  return err("failure")
end

cell test_set_string() -> set[String]
  let tags: set[String] = {"tag1", "tag2"}
  return tags
end

cell main() -> Int
  return 0
end
```
