# Standard Library: JSON

JSON parsing and manipulation utilities.

```lumen
use tool "json_parse"
use tool "json_stringify"

effect json

grant json_access
  use tool "json_parse"
  use tool "json_stringify"
  policy
    timeout_ms: 5000
  end
end

bind effect json to "json_parse"
bind effect json to "json_stringify"

# Parse JSON string to value
cell parse(json_str: string) / {json}
  let result = json_parse({input: json_str})
  return result
end

# Stringify value to JSON
cell stringify(value) -> string / {json}
  let result = json_stringify({value: value})
  return result
end

# Pretty print JSON with indentation
cell stringify_pretty(value, indent: int) -> string / {json}
  let result = json_stringify({
    value: value,
    indent: indent
  })
  return result
end

# Get a value at a JSON path (dot-separated)
cell get_path(obj, path: string)
  let parts = split(path, ".")
  let current = obj

  for part in parts
    if contains(current, part)
      current = current[part]
    else
      return null
    end
  end

  return current
end

# Set a value at a JSON path (simple one-level only)
cell set_path(obj, key: string, value)
  let result = obj
  result[key] = value
  return result
end

# Merge two JSON objects (shallow merge)
cell merge(obj1, obj2)
  let result = obj1
  let keys2 = keys(obj2)

  for key in keys2
    result[key] = obj2[key]
  end

  return result
end

# Deep merge two JSON objects
cell merge_deep(obj1, obj2)
  let result = obj1
  let keys2 = keys(obj2)

  for key in keys2
    if contains(obj1, key)
      let val1 = obj1[key]
      let val2 = obj2[key]
      let type1 = type_of(val1)
      let type2 = type_of(val2)
      if type1 == "map" and type2 == "map"
        result[key] = merge_deep(val1, val2)
      else
        result[key] = val2
      end
    else
      result[key] = obj2[key]
    end
  end

  return result
end

# Pick specific keys from an object
cell pick(obj, keys_to_pick: list[string])
  let result = {}

  for key in keys_to_pick
    if contains(obj, key)
      result[key] = obj[key]
    end
  end

  return result
end

# Omit specific keys from an object
cell omit(obj, keys_to_omit: list[string])
  let result = {}
  let all_keys = keys(obj)

  for key in all_keys
    let should_omit = false
    for omit_key in keys_to_omit
      if key == omit_key
        should_omit = true
      end
    end
    if not should_omit
      result[key] = obj[key]
    end
  end

  return result
end

# Check if object has a key
cell has_key(obj, key: string) -> bool
  return contains(obj, key)
end

# Get all values from an object
cell get_values(obj)
  return values(obj)
end

# Get all keys from an object
cell get_keys(obj) -> list[string]
  return keys(obj)
end
```
