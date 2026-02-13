# Language Features Test

> Exercises new language features: while/loop/break, mutable bindings,
> compound assignments, match patterns, and more.

```lumen
cell test_while() -> Int
  let sum = 0
  let i = 0
  while i < 10
    sum = sum + i
    i = i + 1
  end
  return sum
end

cell test_loop_break() -> Int
  let count = 0
  loop
    count = count + 1
    if count >= 5
      break
    end
  end
  return count
end

cell test_compound_assign() -> Int
  let x = 10
  x += 5
  x -= 3
  x *= 2
  return x
end

cell test_match_patterns() -> String
  let out = ""
  for i in range(0, 5)
    let label = "other"
    match i
      0 -> label = "zero"
      1 -> label = "one"
      2 -> label = "two"
      _ -> label = "other"
    end
    out = out + label + " "
  end
  return out
end

cell test_list_ops() -> String
  let items = []
  items = append(items, "a")
  items = append(items, "b")
  items = append(items, "c")
  let joined = join(items, "-")
  return joined
end

cell test_string_ops() -> String
  let s = "  Hello, World!  "
  let trimmed = trim(s)
  let up = upper(trimmed)
  let lo = lower(trimmed)
  return trimmed + " | " + up + " | " + lo
end

cell test_nested_calls() -> Int
  let a = max(min(10, 20), min(5, 15))
  return a
end

cell test_recursion(n: Int) -> Int
  if n <= 1
    return 1
  end
  return n * test_recursion(n - 1)
end

cell main() -> Null
  print("=== Language Features Test ===")
  print("")

  print("1. While loop (sum 0..9):")
  let w = test_while()
  print("   " + string(w))
  assert_eq(w, 45, "while loop sum should be 45")

  print("2. Loop + break:")
  let lb = test_loop_break()
  print("   count = " + string(lb))
  assert_eq(lb, 5, "loop/break count should be 5")

  print("3. Compound assignment (10 +5 -3 *2):")
  let ca = test_compound_assign()
  print("   " + string(ca))
  assert_eq(ca, 24, "compound assign should be 24")

  print("4. Match patterns:")
  let mp = test_match_patterns()
  print("   " + mp)

  print("5. List operations:")
  let lo = test_list_ops()
  print("   " + lo)
  assert_eq(lo, "a-b-c", "list join should be a-b-c")

  print("6. String operations:")
  let so = test_string_ops()
  print("   " + so)

  print("7. Nested calls (max(min(10,20), min(5,15))):")
  let nc = test_nested_calls()
  print("   " + string(nc))
  assert_eq(nc, 10, "nested calls should be 10")

  print("8. Recursion (5!):")
  let r = test_recursion(5)
  print("   " + string(r))
  assert_eq(r, 120, "5! should be 120")

  print("")
  print("All tests passed!")
  return null
end
```
