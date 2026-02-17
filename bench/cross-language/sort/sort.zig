const std = @import("std");

fn partition(arr: []i32, lo_arg: usize, hi_arg: usize) usize {
    const pivot = arr[hi_arg];
    var i: usize = lo_arg;
    var j: usize = lo_arg;
    while (j < hi_arg) : (j += 1) {
        if (arr[j] <= pivot) {
            const tmp = arr[i];
            arr[i] = arr[j];
            arr[j] = tmp;
            i += 1;
        }
    }
    const tmp = arr[i];
    arr[i] = arr[hi_arg];
    arr[hi_arg] = tmp;
    return i;
}

fn quicksort(arr: []i32, lo_arg: usize, hi_arg: usize) void {
    if (lo_arg >= hi_arg) return;
    const p = partition(arr, lo_arg, hi_arg);
    if (p > 0) quicksort(arr, lo_arg, p - 1);
    quicksort(arr, p + 1, hi_arg);
}

pub fn main() !void {
    const n = 1000000;
    var data: [n]i32 = undefined;

    // Deterministic pseudo-random fill (LCG)
    var val: u32 = 42;
    for (0..n) |i| {
        val = val *% 1103515245 +% 12345;
        data[i] = @intCast(val % 100000);
    }

    quicksort(&data, 0, n - 1);

    // Verify sorted
    var ok = true;
    for (0..n - 1) |i| {
        if (data[i] > data[i + 1]) {
            ok = false;
            break;
        }
    }

    const stdout = std.io.getStdOut().writer();
    try stdout.print("sort({d}) sorted={s}\n", .{ n, if (ok) "true" else "false" });
}
