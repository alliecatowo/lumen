package main

import "fmt"

type Node struct {
	left  *Node
	right *Node
	value int
}

func buildTree(depth int) *Node {
	if depth <= 0 {
		return &Node{value: 1}
	}
	return &Node{
		left:  buildTree(depth - 1),
		right: buildTree(depth - 1),
	}
}

func checkTree(node *Node) int {
	if node.left == nil {
		return node.value
	}
	return checkTree(node.left) + checkTree(node.right)
}

func main() {
	tree := buildTree(18)
	checksum := checkTree(tree)
	fmt.Printf("Checksum: %d\n", checksum)
}
