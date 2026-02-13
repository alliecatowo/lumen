# Standard Library: Collections

List and collection utility functions.

```lumen
# Generate a range of integers (wraps builtin)
cell range_list(start: int, end_val: int) -> list[int]
  return range(start, end_val)
end

# Chunk a list into sublists of size n
cell chunk(lst, size: int)
  if size <= 0
    return []
  end

  let result = []
  let current_chunk = []
  let count = 0

  for item in lst
    current_chunk = append(current_chunk, item)
    count = count + 1
    if count == size
      result = append(result, current_chunk)
      current_chunk = []
      count = 0
    end
  end

  if len(current_chunk) > 0
    result = append(result, current_chunk)
  end

  return result
end

# Zip two lists together
cell zip(list1, list2)
  let len1 = len(list1)
  let len2 = len(list2)
  let min_len = min(len1, len2)

  let result = []
  let i = 0
  while i < min_len
    let pair = [list1[i], list2[i]]
    result = append(result, pair)
    i = i + 1
  end

  return result
end

# Flatten a list of lists
cell flatten(nested)
  let result = []
  for sublist in nested
    for item in sublist
      result = append(result, item)
    end
  end
  return result
end

# Get unique elements from a list
cell unique(lst)
  let result = []
  let seen = {}

  for item in lst
    let key = hash(item)
    if not contains(seen, key)
      seen[key] = true
      result = append(result, item)
    end
  end

  return result
end

# Partition a list based on a predicate (returns [matching, not_matching])
cell partition_by_bool(lst, pred_results: list[bool])
  let matching = []
  let not_matching = []
  let len_lst = len(lst)
  let len_pred = len(pred_results)

  let i = 0
  while i < len_lst and i < len_pred
    if pred_results[i]
      matching = append(matching, lst[i])
    else
      not_matching = append(not_matching, lst[i])
    end
    i = i + 1
  end

  return [matching, not_matching]
end

# Take first n elements
cell take(lst, n: int)
  if n <= 0
    return []
  end
  let len_lst = len(lst)
  let count = min(n, len_lst)

  let result = []
  let i = 0
  while i < count
    result = append(result, lst[i])
    i = i + 1
  end
  return result
end

# Drop first n elements
cell drop(lst, n: int)
  if n <= 0
    return lst
  end
  let len_lst = len(lst)
  if n >= len_lst
    return []
  end

  let result = []
  let i = n
  while i < len_lst
    result = append(result, lst[i])
    i = i + 1
  end
  return result
end

# Find index of first occurrence
cell index_of(lst, item) -> int
  let len_lst = len(lst)
  let i = 0
  while i < len_lst
    if lst[i] == item
      return i
    end
    i = i + 1
  end
  return 0 - 1
end

# Check if all elements satisfy a condition (requires bool list)
cell all(pred_results: list[bool]) -> bool
  for result in pred_results
    if not result
      return false
    end
  end
  return true
end

# Check if any element satisfies a condition (requires bool list)
cell any(pred_results: list[bool]) -> bool
  for result in pred_results
    if result
      return true
    end
  end
  return false
end

# Sum of integers
cell sum_ints(lst: list[int]) -> int
  let total = 0
  for item in lst
    total = total + item
  end
  return total
end

# Product of integers
cell product_ints(lst: list[int]) -> int
  if len(lst) == 0
    return 0
  end
  let total = 1
  for item in lst
    total = total * item
  end
  return total
end

# Intersperse a separator between list elements
cell intersperse(lst, separator)
  let len_lst = len(lst)
  if len_lst <= 1
    return lst
  end

  let result = []
  let i = 0
  while i < len_lst
    result = append(result, lst[i])
    if i < len_lst - 1
      result = append(result, separator)
    end
    i = i + 1
  end
  return result
end

# Group consecutive equal elements
cell group(lst)
  if len(lst) == 0
    return []
  end

  let result = []
  let current_group = [lst[0]]
  let i = 1
  while i < len(lst)
    if lst[i] == lst[i - 1]
      current_group = append(current_group, lst[i])
    else
      result = append(result, current_group)
      current_group = [lst[i]]
    end
    i = i + 1
  end
  result = append(result, current_group)
  return result
end
```
