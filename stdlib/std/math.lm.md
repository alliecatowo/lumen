# Standard Library: Math

Mathematical constants and utility functions.

```lumen
# Mathematical constants
pub cell PI() -> float
  return 3.141592653589793
end

pub cell E() -> float
  return 2.718281828459045
end

pub cell TAU() -> float
  return 6.283185307179586
end

# Absolute value (wraps builtin)
pub cell abs_int(x: int) -> int
  return abs(x)
end

pub cell abs_float(x: float) -> float
  if x < 0.0
    return 0.0 - x
  end
  return x
end

# Min/max (wraps builtins)
pub cell min_int(a: int, b: int) -> int
  return min(a, b)
end

pub cell max_int(a: int, b: int) -> int
  return max(a, b)
end

pub cell min_float(a: float, b: float) -> float
  if a < b
    return a
  end
  return b
end

pub cell max_float(a: float, b: float) -> float
  if a > b
    return a
  end
  return b
end

# Clamp a value between min and max
pub cell clamp_int(value: int, min_val: int, max_val: int) -> int
  if value < min_val
    return min_val
  end
  if value > max_val
    return max_val
  end
  return value
end

pub cell clamp_float(value: float, min_val: float, max_val: float) -> float
  if value < min_val
    return min_val
  end
  if value > max_val
    return max_val
  end
  return value
end

# Power function (integer exponent)
pub cell pow(base: float, exp: int) -> float
  if exp == 0
    return 1.0
  end
  if exp < 0
    return 1.0 / pow(base, 0 - exp)
  end

  let mut result = 1.0
  let mut i = 0
  while i < exp
    result = result * base
    i = i + 1
  end
  return result
end

# Square root using Newton's method
pub cell sqrt(x: float) -> float
  if x < 0.0
    return 0.0
  end
  if x == 0.0
    return 0.0
  end

  let mut guess = x / 2.0
  let epsilon = 0.00001
  let mut iterations = 0
  let max_iterations = 100

  while iterations < max_iterations
    let new_guess = (guess + x / guess) / 2.0
    let diff = abs_float(new_guess - guess)
    if diff < epsilon
      return new_guess
    end
    guess = new_guess
    iterations = iterations + 1
  end

  return guess
end

# Natural logarithm (base e) using Taylor series approximation
pub cell log(x: float) -> float
  if x <= 0.0
    return 0.0
  end
  if x == 1.0
    return 0.0
  end

  # Transform to range (0.5, 1.5) for better convergence
  let mut exp_adjust = 0
  let mut y = x
  while y > 1.5
    y = y / E()
    exp_adjust = exp_adjust + 1
  end
  while y < 0.5
    y = y * E()
    exp_adjust = exp_adjust - 1
  end

  # Taylor series: ln(1+z) = z - z^2/2 + z^3/3 - z^4/4 + ...
  let z = y - 1.0
  let mut result = 0.0
  let mut term = z
  let mut n = 1

  while n <= 20
    result = result + term / float(n)
    term = 0.0 - term * z
    n = n + 1
  end

  return result + float(exp_adjust)
end

# Floor function (round down)
pub cell floor(x: float) -> int
  let i = int(x)
  if x >= 0.0 or x == float(i)
    return i
  end
  return i - 1
end

# Ceiling function (round up)
pub cell ceil(x: float) -> int
  let i = int(x)
  if x <= 0.0 or x == float(i)
    return i
  end
  return i + 1
end

# Round to nearest integer
pub cell round(x: float) -> int
  if x >= 0.0
    return floor(x + 0.5)
  else
    return ceil(x - 0.5)
  end
end

# Sign function
pub cell sign_int(x: int) -> int
  if x > 0
    return 1
  end
  if x < 0
    return 0 - 1
  end
  return 0
end

pub cell sign_float(x: float) -> float
  if x > 0.0
    return 1.0
  end
  if x < 0.0
    return 0.0 - 1.0
  end
  return 0.0
end
```
