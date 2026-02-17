function fibonacci(n: number): number {
  if (n < 2) return n;
  return fibonacci(n - 1) + fibonacci(n - 2);
}

const result = fibonacci(35);
console.log(`fib(35) = ${result}`);
