# Standard Library: Text

string manipulation utilities.

```lumen
# Pad a string on the left with a character
cell pad_left(s: string, width: int, pad_char: string) -> string
  let current_len = len(s)
  if current_len >= width
    return s
  end

  let pad_count = width - current_len
  let padding = ""
  let i = 0
  while i < pad_count
    padding = padding + pad_char
    i = i + 1
  end
  return padding + s
end

# Pad a string on the right with a character
cell pad_right(s: string, width: int, pad_char: string) -> string
  let current_len = len(s)
  if current_len >= width
    return s
  end

  let pad_count = width - current_len
  let padding = ""
  let i = 0
  while i < pad_count
    padding = padding + pad_char
    i = i + 1
  end
  return s + padding
end

# Truncate a string to a maximum length
cell truncate(s: string, max_len: int) -> string
  if len(s) <= max_len
    return s
  end
  if max_len <= 3
    return slice(s, 0, max_len)
  end
  return slice(s, 0, max_len - 3) + "..."
end

# Repeat a string n times
cell repeat(s: string, count: int) -> string
  if count <= 0
    return ""
  end
  let result = ""
  let i = 0
  while i < count
    result = result + s
    i = i + 1
  end
  return result
end

# Check if a string contains a substring (wraps builtin)
cell contains_str(haystack: string, needle: string) -> Bool
  return contains(haystack, needle)
end

# Check if a string starts with a prefix
cell starts_with(s: string, prefix: string) -> Bool
  let s_len = len(s)
  let prefix_len = len(prefix)
  if prefix_len > s_len
    return false
  end
  let start = slice(s, 0, prefix_len)
  return start == prefix
end

# Check if a string ends with a suffix
cell ends_with(s: string, suffix: string) -> Bool
  let s_len = len(s)
  let suffix_len = len(suffix)
  if suffix_len > s_len
    return false
  end
  let start_pos = s_len - suffix_len
  let end_part = slice(s, start_pos, s_len)
  return end_part == suffix
end

# Capitalize first letter
cell capitalize(s: string) -> string
  if len(s) == 0
    return s
  end
  let first = slice(s, 0, 1)
  let rest = slice(s, 1, len(s))
  return upper(first) + lower(rest)
end

# Title case (capitalize each word)
cell title_case(s: string) -> string
  let words = split(s, " ")
  let result = []
  for word in words
    result = append(result, capitalize(word))
  end
  return join(result, " ")
end

# Count words in a string
cell word_count(s: string) -> int
  let trimmed = trim(s)
  if len(trimmed) == 0
    return 0
  end
  let words = split(trimmed, " ")
  return len(words)
end

# Reverse a string
cell reverse(s: string) -> string
  let n = len(s)
  if n <= 1
    return s
  end
  let result = ""
  let i = n - 1
  while i >= 0
    result = result + slice(s, i, i + 1)
    i = i - 1
  end
  return result
end

# Check if string is palindrome
cell is_palindrome(s: string) -> Bool
  let cleaned = lower(trim(s))
  let reversed = reverse(cleaned)
  return cleaned == reversed
end

# Remove prefix if present
cell remove_prefix(s: string, prefix: string) -> string
  if starts_with(s, prefix)
    let prefix_len = len(prefix)
    return slice(s, prefix_len, len(s))
  end
  return s
end

# Remove suffix if present
cell remove_suffix(s: string, suffix: string) -> string
  if ends_with(s, suffix)
    let s_len = len(s)
    let suffix_len = len(suffix)
    return slice(s, 0, s_len - suffix_len)
  end
  return s
end

# Count occurrences of a substring
cell count_occurrences(s: string, substring: string) -> int
  let s_len = len(s)
  let sub_len = len(substring)
  if sub_len == 0 or sub_len > s_len
    return 0
  end

  let count = 0
  let i = 0
  while i <= s_len - sub_len
    let part = slice(s, i, i + sub_len)
    if part == substring
      count = count + 1
    end
    i = i + 1
  end
  return count
end
```
