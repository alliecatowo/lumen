const std = @import("std");

const Entry = struct {
    key: [16]u8 = undefined,
    value: [16]u8 = undefined,
    key_len: usize = 0,
    value_len: usize = 0,
};

fn writeStr(buf: []u8, prefix: []const u8, num: usize) usize {
    var pos: usize = 0;
    for (prefix) |c| {
        buf[pos] = c;
        pos += 1;
    }
    // Write number digits
    if (num == 0) {
        buf[pos] = '0';
        pos += 1;
        return pos;
    }
    var n = num;
    var digits: [10]u8 = undefined;
    var dlen: usize = 0;
    while (n > 0) {
        digits[dlen] = @intCast(n % 10 + '0');
        dlen += 1;
        n /= 10;
    }
    var i: usize = dlen;
    while (i > 0) {
        i -= 1;
        buf[pos] = digits[i];
        pos += 1;
    }
    return pos;
}

fn strEql(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    for (a, b) |ac, bc| {
        if (ac != bc) return false;
    }
    return true;
}

pub fn main() !void {
    const count = 10_000;
    var entries: [count]Entry = undefined;

    for (0..count) |i| {
        entries[i].key_len = writeStr(&entries[i].key, "key_", i);
        entries[i].value_len = writeStr(&entries[i].value, "value_", i);
    }

    // Find key_9999
    var found: []const u8 = "";
    const target = "key_9999";
    for (0..count) |i| {
        if (strEql(entries[i].key[0..entries[i].key_len], target)) {
            found = entries[i].value[0..entries[i].value_len];
            break;
        }
    }

    const stdout_file = std.fs.File.stdout();
    var buf: [4096]u8 = undefined;
    var w = stdout_file.writer(&buf);
    try w.interface.print("Found: {s}\n", .{found});
    try w.interface.print("Count: {d}\n", .{count});
    try w.interface.flush();
}
