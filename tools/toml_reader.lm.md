# TOML Config Reader

Reads `lumen.toml` (or any TOML file) from the current directory and prints all fields
with nice formatting. Handles nested tables by indenting sub-keys.

```lumen
cell print_value(prefix: String, val: Any) -> Null
  let t = type_of(val)
  if t == "map"
    let ks = keys(val)
    let i = 0
    while i < len(ks)
      let k = ks[i]
      let child = val[k]
      let child_type = type_of(child)
      if child_type == "map"
        print("{prefix}{k}:")
        print_value(prefix + "  ", child)
      else
        print("{prefix}{k} = {child}")
      end
      i = i + 1
    end
  else
    print("{prefix}{val}")
  end
  null
end

cell main() -> Null
  let path = "lumen.toml"
  if not exists(path)
    print("No lumen.toml found in current directory.")
    print("Trying Cargo.toml as fallback...")
    path = "Cargo.toml"
    if not exists(path)
      print("No Cargo.toml found either. Nothing to read.")
      return null
    end
  end

  print("=== Reading {path} ===")
  print("")

  let content = read_file(path)
  let parsed = toml_parse(content)

  let top_keys = keys(parsed)
  let i = 0
  while i < len(top_keys)
    let k = top_keys[i]
    let val = parsed[k]
    let t = type_of(val)
    if t == "map"
      print("[{k}]")
      print_value("  ", val)
    else
      print("{k} = {val}")
    end
    print("")
    i = i + 1
  end

  print("=== Done ===")
  null
end
```
