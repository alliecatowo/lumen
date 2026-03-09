# Benchmark: Recursive Fibonacci
# Tests: function call overhead, deep recursion, integer arithmetic
# Expected result: fib(35) = 9227465

cell fib(n: Int) -> Int
  if n <= 1
    n
  else
    fib(n - 1) + fib(n - 2)
  end
end

cell main() -> Int
  fib(35)
end
