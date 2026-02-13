# Linked List

A functional linked list using enums.

This example demonstrates recursive data structures using enums and pattern matching.

```lumen
enum Node
  Cons(value: Int, next: Node)
  Nil
end

cell prepend(value: Int, list: Node) -> Node
  Cons(value: value, next: list)
end

cell length(list: Node) -> Int
  match list
    Cons -> 1 + length(list.next)
    Nil -> 0
  end
end

cell contains(list: Node, target: Int) -> Bool
  match list
    Cons ->
      if list.value == target
        true
      else
        contains(list.next, target)
      end
    Nil -> false
  end
end

cell sum(list: Node) -> Int
  match list
    Cons -> list.value + sum(list.next)
    Nil -> 0
  end
end

cell to_list(node: Node) -> list[Int]
  match node
    Cons -> append([node.value], to_list(node.next))
    Nil -> []
  end
end

cell from_list(items: list[Int], index: Int) -> Node
  let n = len(items)
  if index >= n
    return Nil
  end
  let value = items[index]
  let next = from_list(items, index + 1)
  return Cons(value: value, next: next)
end

cell reverse_helper(list: Node, acc: Node) -> Node
  match list
    Cons -> reverse_helper(list.next, prepend(list.value, acc))
    Nil -> acc
  end
end

cell reverse(list: Node) -> Node
  reverse_helper(list, Nil)
end

cell main() -> Null
  print("=== Linked List Example ===")
  print("")

  print("Building list: 1 -> 2 -> 3 -> Nil")
  let list1 = Nil
  list1 = prepend(3, list1)
  list1 = prepend(2, list1)
  list1 = prepend(1, list1)

  print("Length: " + string(length(list1)))
  print("Sum: " + string(sum(list1)))
  print("Contains 2: " + string(contains(list1, 2)))
  print("Contains 5: " + string(contains(list1, 5)))
  print("")

  print("Convert to list:")
  let items = to_list(list1)
  print(join(items, " -> "))
  print("")

  print("Create from list [10, 20, 30]:")
  let list2 = from_list([10, 20, 30], 0)
  print("Length: " + string(length(list2)))
  print("Sum: " + string(sum(list2)))
  print("As list: " + join(to_list(list2), " -> "))
  print("")

  print("Reverse list [1, 2, 3]:")
  let reversed = reverse(list1)
  print("Original: " + join(to_list(list1), " -> "))
  print("Reversed: " + join(to_list(reversed), " -> "))

  return null
end
```
