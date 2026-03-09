# Benchmark: String processing — concatenation and search
# Tests: string allocation, scanning, basic string operations
# Builds a string by joining "hello world " 10000 times, counts "world"

cell count_occurrences(haystack: String, needle: String) -> Int
  let hay_len = len(haystack)
  let needle_len = len(needle)
  let mut count = 0
  let mut i = 0
  while i <= hay_len - needle_len
    let chunk = slice(haystack, i, i + needle_len)
    if chunk == needle
      count = count + 1
    end
    i = i + 1
  end
  return count
end

cell main() -> String
  let unit = "hello world "
  let mut s = ""
  let mut i = 0
  while i < 10000
    s = s + unit
    i = i + 1
  end
  let c = count_occurrences(s, "world")
  let result = "string_len=" + to_string(len(s)) + " occurrences=" + to_string(c)
  print(result)
  return result
end
