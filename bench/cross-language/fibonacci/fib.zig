const std = @import("std");

fn fibonacci(n: i32) i32 {
    if (n < 2) return n;
    return fibonacci(n - 1) + fibonacci(n - 2);
}

pub fn main() void {
    const result = fibonacci(35);
    const stdout = std.io.getStdOut().writer();
    stdout.print("fib(35) = {d}\n", .{result}) catch {};
}
