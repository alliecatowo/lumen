#!/usr/bin/env python3
"""
Comprehensive Python benchmark suite for fair language comparison.

All benchmarks are pure algorithmic work — no I/O, no C-extension library calls.
Each benchmark prints timing in machine-parseable format:
    BENCH category/name: X.XXXXs

Usage:
    python3 bench/python_bench.py
"""

import time
import sys


# ---------------------------------------------------------------------------
# 1. int_fib — recursive fibonacci(35)
# ---------------------------------------------------------------------------
def bench_int_fib():
    def fib(n):
        if n < 2:
            return n
        return fib(n - 1) + fib(n - 2)

    start = time.perf_counter()
    result = fib(35)
    elapsed = time.perf_counter() - start
    assert result == 9227465, f"fib(35) = {result}"
    return elapsed


# ---------------------------------------------------------------------------
# 2. int_sum_loop — sum integers 1..10_000_000 in a loop
# ---------------------------------------------------------------------------
def bench_int_sum_loop():
    start = time.perf_counter()
    total = 0
    i = 1
    while i <= 10_000_000:
        total += i
        i += 1
    elapsed = time.perf_counter() - start
    assert total == 50_000_005_000_000, f"sum = {total}"
    return elapsed


# ---------------------------------------------------------------------------
# 3. int_primes_sieve — sieve of Eratosthenes up to 100_000
# ---------------------------------------------------------------------------
def bench_int_primes_sieve():
    start = time.perf_counter()
    limit = 100_000
    is_prime = [True] * (limit + 1)
    is_prime[0] = False
    is_prime[1] = False
    p = 2
    while p * p <= limit:
        if is_prime[p]:
            multiple = p * p
            while multiple <= limit:
                is_prime[multiple] = False
                multiple += p
        p += 1
    count = 0
    for i in range(limit + 1):
        if is_prime[i]:
            count += 1
    elapsed = time.perf_counter() - start
    assert count == 9592, f"prime count = {count}"
    return elapsed


# ---------------------------------------------------------------------------
# 4. float_mandelbrot — 200x200 grid, max 100 iterations
# ---------------------------------------------------------------------------
def bench_float_mandelbrot():
    start = time.perf_counter()
    width = 200
    height = 200
    max_iter = 100
    x_min, x_max = -2.0, 1.0
    y_min, y_max = -1.5, 1.5
    count = 0

    for py in range(height):
        ci = y_min + (y_max - y_min) * py / height
        for px in range(width):
            cr = x_min + (x_max - x_min) * px / width
            zr = 0.0
            zi = 0.0
            iteration = 0
            while iteration < max_iter:
                zr2 = zr * zr
                zi2 = zi * zi
                if zr2 + zi2 > 4.0:
                    break
                zi = 2.0 * zr * zi + ci
                zr = zr2 - zi2 + cr
                iteration += 1
            if iteration == max_iter:
                count += 1

    elapsed = time.perf_counter() - start
    # count depends on grid resolution and iteration limit; just sanity check
    assert count > 0, f"mandelbrot count = {count}"
    return elapsed


# ---------------------------------------------------------------------------
# 5. float_nbody — 2-body gravitational simulation, 1_000_000 steps
# ---------------------------------------------------------------------------
def bench_float_nbody():
    start = time.perf_counter()

    # Body 1: "sun" at origin, Body 2: "planet" in orbit
    x1, y1 = 0.0, 0.0
    vx1, vy1 = 0.0, 0.0
    m1 = 1000.0

    x2, y2 = 1.0, 0.0
    vx2, vy2 = 0.0, 1.0
    m2 = 1.0

    dt = 0.001
    steps = 1_000_000
    softening = 1e-6

    for _ in range(steps):
        dx = x2 - x1
        dy = y2 - y1
        dist_sq = dx * dx + dy * dy + softening
        # Approximate inverse cube: dist^(-3/2) via dist_sq^(-3/2)
        dist = dist_sq**0.5
        inv_dist3 = 1.0 / (dist * dist_sq)

        fx = dx * inv_dist3
        fy = dy * inv_dist3

        # Update velocities
        vx1 += dt * m2 * fx
        vy1 += dt * m2 * fy
        vx2 -= dt * m1 * fx
        vy2 -= dt * m1 * fy

        # Update positions
        x1 += dt * vx1
        y1 += dt * vy1
        x2 += dt * vx2
        y2 += dt * vy2

    elapsed = time.perf_counter() - start
    # Sanity check: bodies haven't escaped to infinity
    assert abs(x1) < 1e6 and abs(x2) < 1e6, f"nbody diverged: x1={x1}, x2={x2}"
    return elapsed


# ---------------------------------------------------------------------------
# 6. string_concat — concatenate "hello" 100_000 times
# ---------------------------------------------------------------------------
def bench_string_concat():
    start = time.perf_counter()
    s = ""
    for _ in range(100_000):
        s = s + "hello"
    elapsed = time.perf_counter() - start
    assert len(s) == 500_000, f"len = {len(s)}"
    return elapsed


# ---------------------------------------------------------------------------
# 7. string_build — format/interpolation 100_000 times, accumulate lengths
# ---------------------------------------------------------------------------
def bench_string_build():
    start = time.perf_counter()
    total_len = 0
    for i in range(100_000):
        s = f"item_{i}_value_{i * 2}"
        total_len += len(s)
    elapsed = time.perf_counter() - start
    assert total_len > 0, f"total_len = {total_len}"
    return elapsed


# ---------------------------------------------------------------------------
# 8. list_create_sum — create list of 1_000_000 ints, sum them
# ---------------------------------------------------------------------------
def bench_list_create_sum():
    start = time.perf_counter()
    lst = []
    for i in range(1_000_000):
        lst.append(i)
    total = 0
    for x in lst:
        total += x
    elapsed = time.perf_counter() - start
    assert total == 499_999_500_000, f"sum = {total}"
    return elapsed


# ---------------------------------------------------------------------------
# 9. list_sort — 100_000 integers descending, sort ascending
# ---------------------------------------------------------------------------
def bench_list_sort():
    # Build outside the timer to isolate sort cost
    lst = list(range(100_000, 0, -1))

    start = time.perf_counter()
    lst.sort()
    elapsed = time.perf_counter() - start
    assert lst[0] == 1 and lst[-1] == 100_000, f"sort failed"
    return elapsed


# ---------------------------------------------------------------------------
# 10. map_ops — insert 100_000 k/v pairs, look up all
# ---------------------------------------------------------------------------
def bench_map_ops():
    start = time.perf_counter()
    d = {}
    for i in range(100_000):
        d[f"key_{i}"] = i

    total = 0
    for i in range(100_000):
        total += d[f"key_{i}"]
    elapsed = time.perf_counter() - start
    assert total == 4_999_950_000, f"total = {total}"
    return elapsed


# ---------------------------------------------------------------------------
# 11. call_overhead — trivial function called 10_000_000 times
# ---------------------------------------------------------------------------
def bench_call_overhead():
    def inc(x):
        return x + 1

    start = time.perf_counter()
    result = 0
    for _ in range(10_000_000):
        result = inc(result)
    elapsed = time.perf_counter() - start
    assert result == 10_000_000, f"result = {result}"
    return elapsed


# ---------------------------------------------------------------------------
# 12. recursion_ackermann — ackermann(3, 8)
# ---------------------------------------------------------------------------
def bench_recursion_ackermann():
    sys.setrecursionlimit(100_000)

    def ackermann(m, n):
        if m == 0:
            return n + 1
        elif n == 0:
            return ackermann(m - 1, 1)
        else:
            return ackermann(m - 1, ackermann(m, n - 1))

    start = time.perf_counter()
    result = ackermann(3, 8)
    elapsed = time.perf_counter() - start
    assert result == 2045, f"ackermann(3,8) = {result}"
    return elapsed


# ---------------------------------------------------------------------------
# Runner
# ---------------------------------------------------------------------------
BENCHMARKS = [
    ("int_fib", bench_int_fib),
    ("int_sum_loop", bench_int_sum_loop),
    ("int_primes_sieve", bench_int_primes_sieve),
    ("float_mandelbrot", bench_float_mandelbrot),
    ("float_nbody", bench_float_nbody),
    ("string_concat", bench_string_concat),
    ("string_build", bench_string_build),
    ("list_create_sum", bench_list_create_sum),
    ("list_sort", bench_list_sort),
    ("map_ops", bench_map_ops),
    ("call_overhead", bench_call_overhead),
    ("recursion_ackermann", bench_recursion_ackermann),
]


def main():
    print(f"Python {sys.version}")
    print(f"Running {len(BENCHMARKS)} benchmarks...\n")

    results = []
    for name, fn in BENCHMARKS:
        try:
            elapsed = fn()
            results.append((name, elapsed))
            print(f"BENCH {name}: {elapsed:.4f}s")
        except Exception as e:
            results.append((name, None))
            print(f"BENCH {name}: FAILED ({e})")

    # Summary table
    print("\n" + "=" * 40)
    print(f"{'Benchmark':<25} {'Time':>10}")
    print("-" * 40)
    total = 0.0
    for name, elapsed in results:
        if elapsed is not None:
            print(f"{name:<25} {elapsed:>9.4f}s")
            total += elapsed
        else:
            print(f"{name:<25} {'FAILED':>10}")
    print("-" * 40)
    print(f"{'TOTAL':<25} {total:>9.4f}s")
    print("=" * 40)


if __name__ == "__main__":
    main()
