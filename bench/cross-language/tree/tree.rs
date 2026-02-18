struct Node {
    left: Option<Box<Node>>,
    right: Option<Box<Node>>,
    value: i32,
}

fn build_tree(depth: i32) -> Box<Node> {
    if depth <= 0 {
        return Box::new(Node {
            left: None,
            right: None,
            value: 1,
        });
    }
    Box::new(Node {
        left: Some(build_tree(depth - 1)),
        right: Some(build_tree(depth - 1)),
        value: 0,
    })
}

fn check_tree(node: &Node) -> i32 {
    match (&node.left, &node.right) {
        (Some(left), Some(right)) => check_tree(left) + check_tree(right),
        _ => node.value,
    }
}

fn main() {
    let tree = build_tree(18);
    let checksum = check_tree(&tree);
    println!("Checksum: {}", checksum);
}
