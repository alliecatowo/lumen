const std = @import("std");

pub fn main() !void {
    const n = 1_000_000;
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const data = try allocator.alloc(i32, n);
    defer allocator.free(data);

    // Deterministic pseudo-random fill (LCG)
    var val: u32 = 42;
    for (data) |*slot| {
        val = val *% 1103515245 +% 12345;
        slot.* = @intCast(val % 100000);
    }

    std.mem.sort(i32, data, {}, std.sort.asc(i32));

    // Verify sorted
    var ok = true;
    for (0..n - 1) |i| {
        if (data[i] > data[i + 1]) {
            ok = false;
            break;
        }
    }

    const stdout_file = std.fs.File.stdout();
    var buf: [4096]u8 = undefined;
    var w = stdout_file.writer(&buf);
    try w.interface.print("sort({d}) sorted={s}\n", .{ n, if (ok) "true" else "false" });
    try w.interface.flush();
}
