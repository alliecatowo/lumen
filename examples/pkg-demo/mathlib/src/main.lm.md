# Math Library

```lumen
cell square(x: Int) -> Int
  return x * x
end

cell cube(x: Int) -> Int
  return x * x * x
end

cell factorial(n: Int) -> Int
  if n <= 1
    return 1
  end
  return n * factorial(n - 1)
end
```
