# lumen-provider-json

JSON manipulation tool provider for Lumen's runtime tool dispatch system.

## Overview

`lumen-provider-json` implements the `ToolProvider` trait to expose JSON operations as callable tools in Lumen programs. It provides parsing, stringifying, dot-path navigation, deep merge, flattening, and structural diff capabilities for JSON data manipulation.

All operations work with `serde_json::Value` types and integrate seamlessly with Lumen's built-in `Json` type and `parse_json`/`to_json` builtins. The provider is pure Rust with no external service dependencies.

## Provided Tools

| Tool | Description |
|------|-------------|
| `json.parse` | Parse JSON string into structured value |
| `json.stringify` | Serialize value to JSON string |
| `json.get` | Extract value using dot-path notation |
| `json.set` | Set value at dot-path location |
| `json.merge` | Deep merge two JSON objects |
| `json.flatten` | Flatten nested object to dot-path keys |
| `json.diff` | Compute structural difference |

## Usage in Lumen

### Parsing and Stringifying

```lumen
use tool json.parse as JsonParse
use tool json.stringify as JsonStringify

cell process_json(raw: String) -> String / {json}
  let data = JsonParse(input: raw)
  let name = data.user.name
  return JsonStringify(value: {"greeting": "Hello, {name}"})
end
```

### Dot-Path Navigation

```lumen
use tool json.get as JsonGet

cell extract_field(data: Json, path: String) -> String / {json}
  let result = JsonGet(json: data, path: path)
  return result
end

# Example: extract_field(user_data, "address.city")
```

### Deep Merge

```lumen
use tool json.merge as JsonMerge

cell merge_configs(base: Json, overrides: Json) -> Json / {json}
  return JsonMerge(a: base, b: overrides)
end
```

### Flatten and Diff

```lumen
use tool json.flatten as JsonFlatten
use tool json.diff as JsonDiff

cell detect_changes(old: Json, new: Json) -> list[String] / {json}
  let diff = JsonDiff(a: old, b: new)
  let flat = JsonFlatten(json: diff)
  return keys(flat)
end
```

## Tool Schemas

### json.parse

**Input**:
```json
{
  "input": "{\"name\": \"Alice\", \"age\": 30}"
}
```

**Output**:
```json
{
  "name": "Alice",
  "age": 30
}
```

### json.get

**Input**:
```json
{
  "json": {"user": {"profile": {"name": "Alice"}}},
  "path": "user.profile.name"
}
```

**Output**:
```json
"Alice"
```

### json.merge

**Input**:
```json
{
  "a": {"x": 1, "y": 2},
  "b": {"y": 3, "z": 4}
}
```

**Output**:
```json
{
  "x": 1,
  "y": 3,
  "z": 4
}
```

### json.flatten

**Input**:
```json
{
  "json": {
    "user": {"name": "Alice", "age": 30},
    "active": true
  }
}
```

**Output**:
```json
{
  "user.name": "Alice",
  "user.age": 30,
  "active": true
}
```

### json.diff

**Input**:
```json
{
  "a": {"x": 1, "y": 2},
  "b": {"x": 1, "y": 3, "z": 4}
}
```

**Output**:
```json
{
  "changed": {"y": [2, 3]},
  "added": {"z": 4}
}
```

## Configuration

In `lumen.toml`:

```toml
[providers]
json.parse = "builtin-json"
json.stringify = "builtin-json"
json.get = "builtin-json"
json.set = "builtin-json"
json.merge = "builtin-json"
json.flatten = "builtin-json"
json.diff = "builtin-json"
```

## Integration

Register with the runtime:

```rust
use lumen_rt::services::tools::ProviderRegistry;
use lumen_provider_json::JsonProvider;

let mut registry = ProviderRegistry::new();
registry.register(Box::new(JsonProvider::new()));
```

## Error Handling

Returns `ToolError::InvocationFailed` for:
- Invalid JSON syntax
- Missing required fields
- Type mismatches (e.g., setting a value on a non-object)
- Path not found

## Testing

```bash
cargo test -p lumen-provider-json

# Specific tests
cargo test -p lumen-provider-json parse
cargo test -p lumen-provider-json get
cargo test -p lumen-provider-json merge
```

## Performance

- Parse/stringify use `serde_json` (highly optimized)
- Dot-path lookup is O(depth) via pointer traversal
- Deep merge is recursive O(n) where n = total nodes
- Flatten is O(n) single-pass

## Related Crates

- **lumen-rt** — Defines `ToolProvider` trait
- **serde_json** — JSON parsing and manipulation
- **lumen-cli** — Registers JSON provider by default
