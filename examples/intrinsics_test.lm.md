# Intrinsics Test

```lumen
cell main()
  print("Testing intrinsics...")
  
  # String ops
  let s = "  Hello World  "
  print("Trim: '" + trim(s) + "'")
  print("Upper: " + upper(s))
  print("Lower: " + lower(s))
  
  # List/String ops
  print("Split:")
  print(split(trim(s), " "))
  print("Join: " + join(["a", "b", "c"], "-"))
  
  # Map ops
  let m = {"a": 1, "b": 2}
  print("Keys:")
  print(keys(m))
  print("Values:")
  print(values(m))
  print("Contains List (true): " + string(contains([1, 2], 1)))
  print("Contains Map (true): " + string(contains(m, "a")))
  
  # Range & Slice
  print("Range 0..5:")
  print(range(0, 5))
  print("Slice string:")
  print(slice("hello", 1, 4))
  print("Slice list:")
  print(slice([1, 2, 3, 4, 5], 1, 3))
  
  # Math
  print("Min(10, 5): " + string(min(10, 5)))
  print("Max(10, 5): " + string(max(10, 5)))
  print("Abs(-5): " + string(abs(-5)))
  
  # Append
  print("Append:")
  print(append([1], 2))

  # Typeof
  print("Typeof 1: " + type(1))
  print("Typeof 's': " + type("s"))
  print("Typeof []: " + type([]))
  
  # Conversions
  print("ToInt '123': " + string(int("123")))
  print("ToFloat '1.5': " + string(float("1.5")))
end
```
