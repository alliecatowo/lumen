# Benchmark: Mergesort on 100,000 integers
# Tests: list operations, recursion, higher-order sort, integer arithmetic
# Generates [100000, 99999, ..., 1] and sorts it, printing the sum

cell make_data(n: Int) -> list[Int]
  let mut data = []
  let mut i = n
  while i >= 1
    data = append(data, i)
    i = i - 1
  end
  return data
end

cell sum_list(items: list[Int]) -> Int
  let mut total = 0
  let mut i = 0
  let n = len(items)
  while i < n
    total = total + items[i]
    i = i + 1
  end
  return total
end

cell main() -> String
  let n = 100000
  let data = make_data(n)
  let sorted = sort(data)
  let s = sum_list(sorted)
  let result = "sort(" + to_string(n) + ") sum=" + to_string(s)
  print(result)
  return result
end
