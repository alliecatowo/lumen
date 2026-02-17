# Tokenizer

A naive Lumen tokenizer that splits source code into tokens and classifies
each one as a keyword, identifier, number, string literal, operator, or
punctuation. Demonstrates pattern matching, string operations, and list
processing.

```lumen
cell is_keyword(token: String) -> Bool
  if token == "cell"
    return true
  end
  if token == "end"
    return true
  end
  if token == "if"
    return true
  end
  if token == "else"
    return true
  end
  if token == "for"
    return true
  end
  if token == "while"
    return true
  end
  if token == "match"
    return true
  end
  if token == "let"
    return true
  end
  if token == "mut"
    return true
  end
  if token == "in"
    return true
  end
  if token == "return"
    return true
  end
  if token == "record"
    return true
  end
  if token == "enum"
    return true
  end
  if token == "import"
    return true
  end
  if token == "true"
    return true
  end
  if token == "false"
    return true
  end
  if token == "null"
    return true
  end
  if token == "loop"
    return true
  end
  if token == "break"
    return true
  end
  if token == "continue"
    return true
  end
  if token == "defer"
    return true
  end
  if token == "yield"
    return true
  end
  return false
end

cell is_operator(token: String) -> Bool
  if token == "+"
    return true
  end
  if token == "-"
    return true
  end
  if token == "*"
    return true
  end
  if token == "/"
    return true
  end
  if token == "="
    return true
  end
  if token == "=="
    return true
  end
  if token == "!="
    return true
  end
  if token == "<"
    return true
  end
  if token == ">"
    return true
  end
  if token == "<="
    return true
  end
  if token == ">="
    return true
  end
  if token == "->"
    return true
  end
  if token == "|>"
    return true
  end
  if token == "~>"
    return true
  end
  return false
end

cell is_punctuation(token: String) -> Bool
  if token == "("
    return true
  end
  if token == ")"
    return true
  end
  if token == "["
    return true
  end
  if token == "]"
    return true
  end
  if token == ":"
    return true
  end
  if token == ","
    return true
  end
  return false
end

cell is_digit(ch: String) -> Bool
  if ch == "0"
    return true
  end
  if ch == "1"
    return true
  end
  if ch == "2"
    return true
  end
  if ch == "3"
    return true
  end
  if ch == "4"
    return true
  end
  if ch == "5"
    return true
  end
  if ch == "6"
    return true
  end
  if ch == "7"
    return true
  end
  if ch == "8"
    return true
  end
  if ch == "9"
    return true
  end
  return false
end

cell is_number(token: String) -> Bool
  if len(token) == 0
    return false
  end
  let chs = chars(token)
  let i = 0
  while i < len(chs)
    if is_digit(chs[i]) == false
      if chs[i] != "."
        return false
      end
    end
    i = i + 1
  end
  return true
end

cell classify(token: String) -> String
  if is_keyword(token)
    return "KEYWORD"
  end
  if is_operator(token)
    return "OPERATOR"
  end
  if is_punctuation(token)
    return "PUNCTUATION"
  end
  if starts_with(token, "\"")
    return "STRING"
  end
  if is_number(token)
    return "NUMBER"
  end
  return "IDENTIFIER"
end

cell main() -> Int
  # Sample Lumen source to tokenize
  let source = "cell greet ( name : String ) -> String\n  let msg = \"Hello\"\n  return msg\nend"

  print("=== Lumen Tokenizer ===")
  print("")
  print("Source:")
  print(source)
  print("")
  print("Tokens:")
  print("")

  # Split into lines and tokenize each
  let lines = split(source, "\n")
  let token_count = 0
  let line_idx = 0

  while line_idx < len(lines)
    let line = lines[line_idx]
    let parts = split(trim(line), " ")
    let j = 0
    while j < len(parts)
      let token = trim(parts[j])
      if len(token) > 0
        let kind = classify(token)
        token_count = token_count + 1
        print("  {to_string(token_count)}. {token} -> {kind}")
      end
      j = j + 1
    end
    line_idx = line_idx + 1
  end

  print("")
  print("Total tokens: {to_string(token_count)}")
  0
end
```
