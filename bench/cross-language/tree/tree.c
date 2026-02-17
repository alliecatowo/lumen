#include <stdio.h>
#include <stdlib.h>

typedef struct Node {
    struct Node* left;
    struct Node* right;
    int value;
} Node;

Node* build_tree(int depth) {
    Node* node = (Node*)malloc(sizeof(Node));
    if (depth <= 0) {
        node->left = NULL;
        node->right = NULL;
        node->value = 1;
        return node;
    }
    node->left = build_tree(depth - 1);
    node->right = build_tree(depth - 1);
    node->value = 0;
    return node;
}

int check_tree(Node* node) {
    if (node->left == NULL) {
        return node->value;
    }
    return check_tree(node->left) + check_tree(node->right);
}

void free_tree(Node* node) {
    if (node == NULL) return;
    free_tree(node->left);
    free_tree(node->right);
    free(node);
}

int main() {
    Node* tree = build_tree(18);
    int checksum = check_tree(tree);
    printf("Checksum: %d\n", checksum);
    free_tree(tree);
    return 0;
}
