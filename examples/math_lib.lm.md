# Math Library

Mathematical functions and algorithms.

This example demonstrates recursion, iteration, and mathematical computations in Lumen.

```lumen
cell factorial(n: Int) -> Int
  if n <= 1
    return 1
  end
  return n * factorial(n - 1)
end

cell fibonacci(n: Int) -> Int
  if n < 2
    return n
  end
  return fibonacci(n - 1) + fibonacci(n - 2)
end

cell gcd(a: Int, b: Int) -> Int
  if b == 0
    return abs(a)
  end
  return gcd(b, a % b)
end

cell lcm(a: Int, b: Int) -> Int
  if a == 0 or b == 0
    return 0
  end
  let product = abs(a * b)
  let divisor = gcd(a, b)
  return product / divisor
end

cell is_prime(n: Int) -> Bool
  if n < 2
    return false
  end
  if n == 2
    return true
  end
  if n % 2 == 0
    return false
  end

  let i = 3
  while i * i <= n
    if n % i == 0
      return false
    end
    i = i + 2
  end
  return true
end

cell power(base: Int, exp: Int) -> Int
  if exp == 0
    return 1
  end
  if exp < 0
    return 0
  end

  let result = 1
  let i = 0
  while i < exp
    result = result * base
    i = i + 1
  end
  return result
end

cell power_fast(base: Int, exp: Int) -> Int
  if exp == 0
    return 1
  end
  if exp < 0
    return 0
  end
  if exp % 2 == 0
    let half = power_fast(base, exp / 2)
    return half * half
  else
    return base * power_fast(base, exp - 1)
  end
end

cell sum_range(start: Int, end_val: Int) -> Int
  if start > end_val
    return 0
  end
  let sum = 0
  for i in range(start, end_val + 1)
    sum = sum + i
  end
  return sum
end

cell count_digits(n: Int) -> Int
  if n == 0
    return 1
  end
  let num = abs(n)
  let count = 0
  while num > 0
    count = count + 1
    num = num / 10
  end
  return count
end

cell reverse_number(n: Int) -> Int
  let num = abs(n)
  let result = 0
  while num > 0
    let digit = num % 10
    result = result * 10 + digit
    num = num / 10
  end
  if n < 0
    return 0 - result
  end
  return result
end

cell main() -> Null
  print("=== Math Library ===")
  print("")

  print("Factorials:")
  print("  5! = " + string(factorial(5)))
  print("  10! = " + string(factorial(10)))
  print("")

  print("Fibonacci:")
  print("  fib(10) = " + string(fibonacci(10)))
  print("  fib(15) = " + string(fibonacci(15)))
  print("")

  print("GCD and LCM:")
  print("  gcd(48, 18) = " + string(gcd(48, 18)))
  print("  lcm(12, 18) = " + string(lcm(12, 18)))
  print("")

  print("Prime numbers:")
  print("  is_prime(17): " + string(is_prime(17)))
  print("  is_prime(20): " + string(is_prime(20)))
  print("  is_prime(97): " + string(is_prime(97)))
  print("")

  print("Powers:")
  print("  2^10 = " + string(power(2, 10)))
  print("  3^4 = " + string(power(3, 4)))
  print("  2^16 (fast) = " + string(power_fast(2, 16)))
  print("")

  print("Ranges:")
  print("  sum(1..10) = " + string(sum_range(1, 10)))
  print("  sum(5..15) = " + string(sum_range(5, 15)))
  print("")

  print("Number operations:")
  print("  count_digits(12345) = " + string(count_digits(12345)))
  print("  reverse_number(12345) = " + string(reverse_number(12345)))
  print("  reverse_number(-678) = " + string(reverse_number(-678)))

  return null
end
```
