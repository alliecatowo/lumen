const std = @import("std");

pub fn main() !void {
    const stdout_file = std.fs.File.stdout();
    var buf: [4096]u8 = undefined;
    var w = stdout_file.writer(&buf);
    const limit = 1000000;

    var sieve: [limit + 1]bool = undefined;
    @memset(&sieve, false);
    sieve[0] = true;
    sieve[1] = true;

    var i: usize = 2;
    while (i * i <= limit) : (i += 1) {
        if (!sieve[i]) {
            var j: usize = i * i;
            while (j <= limit) : (j += i) {
                sieve[j] = true;
            }
        }
    }

    var count: u32 = 0;
    for (sieve[2..]) |is_composite| {
        if (!is_composite) count += 1;
    }

    try w.interface.print("primes_sieve(1000000): count = {d}\n", .{count});
    try w.interface.flush();
}
