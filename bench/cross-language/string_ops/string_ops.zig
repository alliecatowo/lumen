const std = @import("std");

pub fn main() !void {
    const count = 100000;
    var buf: [count]u8 = undefined;
    @memset(&buf, 'x');

    const stdout = std.io.getStdOut().writer();
    try stdout.print("Length: {d}\n", .{buf.len});
}
