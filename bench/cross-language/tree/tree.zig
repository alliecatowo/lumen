const std = @import("std");

const Node = struct {
    left: ?*Node,
    right: ?*Node,
    value: i32,
};

var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);

fn buildTree(depth: i32) *Node {
    const node = arena.allocator().create(Node) catch unreachable;
    if (depth <= 0) {
        node.* = .{ .left = null, .right = null, .value = 1 };
        return node;
    }
    node.* = .{
        .left = buildTree(depth - 1),
        .right = buildTree(depth - 1),
        .value = 0,
    };
    return node;
}

fn checkTree(node: *Node) i32 {
    if (node.left == null) {
        return node.value;
    }
    return checkTree(node.left.?) + checkTree(node.right.?);
}

pub fn main() !void {
    const tree = buildTree(18);
    const checksum = checkTree(tree);

    const stdout = std.io.getStdOut().writer();
    try stdout.print("Checksum: {d}\n", .{checksum});

    arena.deinit();
}
