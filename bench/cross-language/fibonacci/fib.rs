fn fibonacci(n: i32) -> i32 {
    if n < 2 {
        return n;
    }
    fibonacci(n - 1) + fibonacci(n - 2)
}

fn main() {
    let result = fibonacci(35);
    println!("fib(35) = {}", result);
}
