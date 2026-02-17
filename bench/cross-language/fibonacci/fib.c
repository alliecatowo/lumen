#include <stdio.h>

int fibonacci(int n) {
    if (n < 2) return n;
    return fibonacci(n - 1) + fibonacci(n - 2);
}

int main() {
    int result = fibonacci(35);
    printf("fib(35) = %d\n", result);
    return 0;
}
