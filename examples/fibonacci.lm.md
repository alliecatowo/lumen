# Fibonacci

> Computes Fibonacci numbers using both recursive and iterative approaches.
> Demonstrates: recursion, pattern matching, for loops, arithmetic, list building.

```lumen
cell fib_recursive(n: Int) -> Int
  if n < 2
    return n
  end
  let a = fib_recursive(n - 1)
  let b = fib_recursive(n - 2)
  return a + b
end

cell fib_iterative(n: Int) -> Int
  if n < 2
    return n
  end
  let prev = 0
  let curr = 1
  let i = 2
  for _ in range(2, n + 1)
    let next = prev + curr
    prev = curr
    curr = next
  end
  return curr
end

cell fib_sequence(count: Int) -> list[Int]
  let items = []
  for i in range(0, count)
    let val = fib_recursive(i)
    items = append(items, val)
  end
  return items
end

cell classify(n: Int) -> String
  let f = fib_recursive(n)
  match f
    0 -> return "zero"
    1 -> return "one"
    _ -> return "fib(" + to_string(n) + ") = " + to_string(f)
  end
end

cell main() -> Null
  print("═══ Fibonacci Sequence ═══")
  print("")
  let seq = fib_sequence(15)
  print("First 15 Fibonacci numbers:")
  print(join(seq, ", "))
  print("")
  print("Classifications:")
  for i in range(0, 10)
    print("  " + classify(i))
  end
  print("")
  print("fib(20) = " + to_string(fib_recursive(20)))
  return null
end
```
