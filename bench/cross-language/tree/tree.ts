interface TreeNode {
  left: TreeNode | null;
  right: TreeNode | null;
  value: number;
}

function buildTree(depth: number): TreeNode {
  if (depth <= 0) {
    return { left: null, right: null, value: 1 };
  }
  return {
    left: buildTree(depth - 1),
    right: buildTree(depth - 1),
    value: 0,
  };
}

function checkTree(node: TreeNode): number {
  if (node.left === null) {
    return node.value;
  }
  return checkTree(node.left!) + checkTree(node.right!);
}

const tree = buildTree(18);
const checksum = checkTree(tree);
console.log(`Checksum: ${checksum}`);
