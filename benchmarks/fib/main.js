// Fibonacci benchmark: recursive fib(35) = 9227465
function fib(n) {
  return n <= 1 ? n : fib(n - 1) + fib(n - 2);
}
console.log(fib(35));
