import subprocess
import time
import sys

def run_bench(file_path, jit_threshold):
    cmd = ["lumen", "run", file_path, "--jit-threshold", str(jit_threshold)]
    start = time.time()
    subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    end = time.time()
    return (end - start) * 1000

benches = [
    ("fibonacci", "bench/cross-language/fibonacci/fib.lm"),
    ("string_ops", "bench/cross-language/string_ops/string_ops.lm")
]

print(f"{'Benchmark':<15} | {'JIT (ms)':<10} | {'Interpreter (ms)':<16} | {'Speedup':<8}")
print("-" * 55)

for name, path in benches:
    # Warm up
    run_bench(path, 0)
    
    jit_times = [run_bench(path, 0) for _ in range(3)]
    jit_median = sorted(jit_times)[1]
    
    int_times = [run_bench(path, 1000000) for _ in range(3)]
    int_median = sorted(int_times)[1]
    
    speedup = int_median / jit_median
    print(f"{name:<15} | {jit_median:<10.2f} | {int_median:<16.2f} | {speedup:<8.2f}x")
