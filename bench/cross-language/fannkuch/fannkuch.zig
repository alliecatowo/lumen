const std = @import("std");

const N = 10;

pub fn main() !void {
    const stdout_file = std.fs.File.stdout();
    var buf: [4096]u8 = undefined;
    var w = stdout_file.writer(&buf);

    var perm: [N]usize = undefined;
    var perm1: [N]usize = undefined;
    var count: [N]usize = undefined;
    var max_flips: i32 = 0;
    var checksum: i32 = 0;
    var r: usize = N;
    var perm_count: usize = 0;

    for (0..N) |i| {
        perm1[i] = i;
    }

    outer: while (true) {
        while (r > 1) {
            count[r - 1] = r;
            r -= 1;
        }

        @memcpy(&perm, &perm1);

        // Count flips
        var flips: i32 = 0;
        var k: usize = perm[0];
        while (k != 0) {
            // Reverse first k+1 elements
            std.mem.reverse(usize, perm[0 .. k + 1]);
            flips += 1;
            k = perm[0];
        }

        if (flips > max_flips) max_flips = flips;
        if (perm_count % 2 == 0) {
            checksum += flips;
        } else {
            checksum -= flips;
        }
        perm_count += 1;

        // Next permutation
        while (true) {
            if (r == N) break :outer;
            const p0 = perm1[0];
            for (0..r) |i| {
                perm1[i] = perm1[i + 1];
            }
            perm1[r] = p0;
            count[r] -= 1;
            if (count[r] > 0) break;
            r += 1;
        }
    }

    try w.interface.print("{d}\nPfannkuchen({d}) = {d}\n", .{ checksum, N, max_flips });
    try w.interface.flush();
}
