# Test Demo

Demonstrates first-party fail-fast assertions for `.lm` and `.lm.md` programs.
For suite-style result collection, see `stdlib/std/testing.lm.md`.

```lumen
cell test_assert_eq() -> Bool
  assert_eq(2 + 2, 4)
  return true
end

cell test_assert_ne() -> Bool
  assert_ne("lumen", "LUMEN")
  return true
end

cell test_assert_contains_list() -> Bool
  assert_contains([1, 2, 3], 2)
  return true
end

cell test_assert_contains_string() -> Bool
  assert_contains("lumen language", "lumen")
  return true
end

cell test_assert_condition() -> Bool
  assert(10 > 3, "10 should be greater than 3")
  return true
end

cell main() -> Null
  print("Running built-in assertion demo...")

  assert(test_assert_eq(), "test_assert_eq should pass")
  assert(test_assert_ne(), "test_assert_ne should pass")
  assert(test_assert_contains_list(), "test_assert_contains_list should pass")
  assert(test_assert_contains_string(), "test_assert_contains_string should pass")
  assert(test_assert_condition(), "test_assert_condition should pass")

  print("All assertion demo checks passed.")
  return null
end
```
