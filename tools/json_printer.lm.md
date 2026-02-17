# JSON Pretty-Printer

Reads a JSON file, parses it, and prints it with indentation. Implements manual
pretty-printing with proper nesting for objects and arrays.

```lumen
cell repeat_str(s: String, n: Int) -> String
  let result = ""
  let i = 0
  while i < n
    result = result + s
    i = i + 1
  end
  result
end

cell pretty_print(val: Any, depth: Int) -> String
  let indent = repeat_str("  ", depth)
  let inner = repeat_str("  ", depth + 1)
  let t = type_of(val)

  if t == "null"
    return "null"
  end
  if t == "bool"
    return to_string(val)
  end
  if t == "int"
    return to_string(val)
  end
  if t == "float"
    return to_string(val)
  end
  if t == "string"
    # Escape quotes inside the string
    let escaped = replace(val, "\\", "\\\\")
    escaped = replace(escaped, "\"", "\\\"")
    return "\"" + escaped + "\""
  end
  if t == "list"
    let n = len(val)
    if n == 0
      return "[]"
    end
    let parts = []
    let i = 0
    while i < n
      let item_str = pretty_print(val[i], depth + 1)
      parts = append(parts, inner + item_str)
      i = i + 1
    end
    return "[\n" + join(parts, ",\n") + "\n" + indent + "]"
  end
  if t == "map"
    let ks = keys(val)
    let n = len(ks)
    if n == 0
      return "{}"
    end
    let parts = []
    let i = 0
    while i < n
      let k = ks[i]
      let v_str = pretty_print(val[k], depth + 1)
      let escaped_k = replace(k, "\"", "\\\"")
      parts = append(parts, inner + "\"" + escaped_k + "\": " + v_str)
      i = i + 1
    end
    return "{\n" + join(parts, ",\n") + "\n" + indent + "}"
  end

  # Fallback for other types
  to_string(val)
end

cell main() -> Null
  # Try to read a JSON file — use a sample if no file exists
  let path = "package.json"
  let content = ""
  let using_sample = false

  if exists(path)
    content = read_file(path)
  else
    print("No {path} found — using sample JSON data")
    print("")
    using_sample = true
    content = to_json({
      "name": "lumen-project",
      "version": "0.1.0",
      "description": "A Lumen language project",
      "keywords": ["lumen", "language", "compiler"],
      "author": "Lumen Team",
      "settings": {
        "debug": true,
        "level": 3,
        "tags": ["alpha", "preview"]
      }
    })
  end

  let parsed = parse_json(content)
  let output = pretty_print(parsed, 0)

  print("=== JSON Pretty-Print ===")
  print("")
  print(output)
  print("")
  print("=== Done ===")
  null
end
```
