# Sorting Algorithms

Implementations of common sorting algorithms in Lumen.

This example demonstrates sorting algorithms using lists, loops, and comparison operations.

```lumen
cell min_value(items: list[Int]) -> Int
  let n = len(items)
  if n == 0
    return 0
  end
  let min_val = items[0]
  let i = 1
  while i < n
    let val = items[i]
    if val < min_val
      min_val = val
    end
    i = i + 1
  end
  return min_val
end

cell max_value(items: list[Int]) -> Int
  let n = len(items)
  if n == 0
    return 0
  end
  let max_val = items[0]
  let i = 1
  while i < n
    let val = items[i]
    if val > max_val
      max_val = val
    end
    i = i + 1
  end
  return max_val
end

cell selection_sort(items: list[Int]) -> list[Int]
  let n = len(items)
  if n <= 1
    return items
  end

  let sorted = []
  let remaining = items

  while len(remaining) > 0
    let min_val = min_value(remaining)
    sorted = append(sorted, min_val)

    let new_remaining = []
    let found = false
    let i = 0
    while i < len(remaining)
      let val = remaining[i]
      if val == min_val and not found
        found = true
      else
        new_remaining = append(new_remaining, val)
      end
      i = i + 1
    end
    remaining = new_remaining
  end

  return sorted
end

cell insert_sorted(sorted: list[Int], value: Int) -> list[Int]
  let n = len(sorted)
  if n == 0
    return [value]
  end

  let result = []
  let inserted = false
  let i = 0

  while i < n
    let curr = sorted[i]
    if not inserted and value <= curr
      result = append(result, value)
      inserted = true
    end
    result = append(result, curr)
    i = i + 1
  end

  if not inserted
    result = append(result, value)
  end

  return result
end

cell insertion_sort(items: list[Int]) -> list[Int]
  let n = len(items)
  if n <= 1
    return items
  end

  let sorted = []
  let i = 0
  while i < n
    sorted = insert_sorted(sorted, items[i])
    i = i + 1
  end
  return sorted
end

cell is_sorted(items: list[Int]) -> Bool
  let n = len(items)
  if n <= 1
    return true
  end

  let i = 0
  while i < n - 1
    let a = get(items, i)
    let b = get(items, i + 1)
    if a > b
      return false
    end
    i = i + 1
  end
  return true
end

cell main() -> Null
  print("=== Sorting Algorithms ===")
  print("")

  let unsorted = [64, 34, 25, 12, 22, 11, 90]
  print("Original list:")
  print(join(unsorted, ", "))
  print("")

  print("Min value: " + string(min_value(unsorted)))
  print("Max value: " + string(max_value(unsorted)))
  print("")

  print("Selection sort result:")
  let selection_result = selection_sort(unsorted)
  print(join(selection_result, ", "))
  let selection_ok = is_sorted(selection_result)
  print("Sorted: " + string(selection_ok))
  print("")

  print("Insertion sort result:")
  let insertion_result = insertion_sort(unsorted)
  print(join(insertion_result, ", "))
  let insertion_ok = is_sorted(insertion_result)
  print("Sorted: " + string(insertion_ok))
  print("")

  print("Small list test:")
  let small = [3, 1, 2]
  print("Before: " + join(small, ", "))
  let small_sorted = insertion_sort(small)
  print("After:  " + join(small_sorted, ", "))

  return null
end
```
