const std = @import("std");

pub fn main() !void {
    const count = 100_000;
    var buffer: [count]u8 = undefined;
    @memset(&buffer, 'x');

    const stdout_file = std.fs.File.stdout();
    var buf: [4096]u8 = undefined;
    var w = stdout_file.writer(&buf);
    try w.interface.print("Length: {d}\n", .{buffer.len});
    try w.interface.flush();
}
