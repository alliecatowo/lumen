# String Utilities

Common string operations implemented in Lumen.

This example demonstrates string manipulation using built-in intrinsics and custom functions.

```lumen
cell capitalize(s: String) -> String
  if len(s) == 0
    return s
  end
  let first = slice(s, 0, 1)
  let rest = slice(s, 1, len(s))
  return upper(first) + lower(rest)
end

cell title_case(s: String) -> String
  let words = split(s, " ")
  let result = []
  for word in words
    result = append(result, capitalize(word))
  end
  return join(result, " ")
end

cell word_count(s: String) -> Int
  let trimmed = trim(s)
  if len(trimmed) == 0
    return 0
  end
  let words = split(trimmed, " ")
  return len(words)
end

cell truncate(s: String, max_len: Int) -> String
  if len(s) <= max_len
    return s
  end
  if max_len <= 3
    return slice(s, 0, max_len)
  end
  return slice(s, 0, max_len - 3) + "..."
end

cell repeat_string(s: String, count: Int) -> String
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

cell reverse_string(s: String) -> String
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

cell is_palindrome(s: String) -> Bool
  let cleaned = lower(trim(s))
  let reversed = reverse_string(cleaned)
  return cleaned == reversed
end

cell main() -> Null
  print("=== String Utilities ===")
  print("")

  let text = "hello world"
  print("Original: '{text}'")
  print("Capitalize: '" + capitalize(text) + "'")
  print("Title case: '" + title_case(text) + "'")
  print("Upper: '" + upper(text) + "'")
  print("Lower: '" + lower(text) + "'")
  print("")

  let sentence = "  The quick brown fox jumps  "
  print("Sentence: '{sentence}'")
  print("Trimmed: '" + trim(sentence) + "'")
  print("Word count: " + string(word_count(sentence)))
  print("")

  let long_text = "This is a very long string that needs truncation"
  print("Long text: '{long_text}'")
  print("Truncated (20): '" + truncate(long_text, 20) + "'")
  print("Truncated (10): '" + truncate(long_text, 10) + "'")
  print("")

  print("Repeat 'Hi' 5 times: '" + repeat_string("Hi", 5) + "'")
  print("Repeat '-' 10 times: '" + repeat_string("-", 10) + "'")
  print("")

  let test_word = "racecar"
  print("'{test_word}' reversed: '" + reverse_string(test_word) + "'")
  print("Is palindrome: " + string(is_palindrome(test_word)))
  print("")

  let not_palindrome = "hello"
  print("'{not_palindrome}' reversed: '" + reverse_string(not_palindrome) + "'")
  print("Is palindrome: " + string(is_palindrome(not_palindrome)))

  return null
end
```
