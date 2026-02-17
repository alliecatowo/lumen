import sys

sys.setrecursionlimit(1000000)


class Node:
    __slots__ = ["left", "right", "value"]

    def __init__(self, left=None, right=None, value=0):
        self.left = left
        self.right = right
        self.value = value


def build_tree(depth: int) -> Node:
    if depth <= 0:
        return Node(value=1)
    return Node(
        left=build_tree(depth - 1),
        right=build_tree(depth - 1),
    )


def check_tree(node: Node) -> int:
    if node.left is None:
        return node.value
    return check_tree(node.left) + check_tree(node.right)


if __name__ == "__main__":
    tree = build_tree(18)
    checksum = check_tree(tree)
    print(f"Checksum: {checksum}")
