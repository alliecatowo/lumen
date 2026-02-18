const std = @import("std");

fn fibonacci(n: i32) i32 {
    if (n < 2) return n;
    return fibonacci(n - 1) + fibonacci(n - 2);
}

pub fn main() !void {
    const result = fibonacci(35);
    const stdout_file = std.fs.File.stdout();
    var buf: [4096]u8 = undefined;
    var w = stdout_file.writer(&buf);
    try w.interface.print("fib(35) = {d}\n", .{result});
    try w.interface.flush();
}
