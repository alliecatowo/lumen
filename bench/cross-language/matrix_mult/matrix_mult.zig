const std = @import("std");

const N = 200;

pub fn main() !void {
    const stdout_file = std.fs.File.stdout();
    var buf: [4096]u8 = undefined;
    var w = stdout_file.writer(&buf);

    // Use arrays on the heap via page allocator since 200x200 is large for stack
    var a: [N][N]f64 = undefined;
    var b: [N][N]f64 = undefined;
    var c: [N][N]f64 = undefined;

    // Initialize matrices
    for (0..N) |i| {
        for (0..N) |j| {
            a[i][j] = @as(f64, @floatFromInt((i * N + j) % 1000)) / 1000.0;
            b[i][j] = @as(f64, @floatFromInt((j * N + i) % 1000)) / 1000.0;
            c[i][j] = 0.0;
        }
    }

    // Multiply C = A * B
    for (0..N) |i| {
        for (0..N) |j| {
            var sum: f64 = 0.0;
            for (0..N) |k| {
                sum += a[i][k] * b[k][j];
            }
            c[i][j] = sum;
        }
    }

    // Checksum
    var checksum: f64 = 0.0;
    for (0..N) |i| {
        for (0..N) |j| {
            checksum += c[i][j];
        }
    }

    try w.interface.print("matrix_mult(200): checksum = {d:.6}\n", .{checksum});
    try w.interface.flush();
}
