# lumen-provider-fs

Filesystem tool provider for Lumen's runtime tool dispatch system.

## Overview

`lumen-provider-fs` implements the `ToolProvider` trait to expose filesystem operations as callable tools in Lumen programs. It provides safe, sandboxed file I/O operations including reading, writing, listing directories, and checking file existence. All operations respect grant policy constraints for path-based security.

The provider wraps Rust's standard library filesystem APIs with proper error handling and JSON serialization for integration with Lumen's tool dispatch system.

## Provided Tools

| Tool | Description |
|------|-------------|
| `fs.read` | Read file contents as UTF-8 string |
| `fs.write` | Write string content to file |
| `fs.exists` | Check if path exists |
| `fs.list` | List directory entries |
| `fs.mkdir` | Create directory recursively |
| `fs.remove` | Remove file or empty directory |

## Usage in Lumen

### Reading and Writing Files

```lumen
use tool fs.read as ReadFile
use tool fs.write as WriteFile

grant ReadFile path "/data/*.txt"
grant WriteFile path "/output/*.txt"

cell process_file(input_path: String, output_path: String) -> Null / {fs}
  let content = ReadFile(path: input_path)
  let processed = upper(content)
  WriteFile(path: output_path, content: processed)
  return null
end
```

### Checking Existence and Listing

```lumen
use tool fs.exists as FileExists
use tool fs.list as ListDir

cell find_config() -> String? / {fs}
  if FileExists(path: "./lumen.toml")
    return "./lumen.toml"
  end
  if FileExists(path: "~/.config/lumen/lumen.toml")
    return "~/.config/lumen/lumen.toml"
  end
  return null
end

cell list_files(dir: String) -> list[String] / {fs}
  let entries = ListDir(path: dir)
  return entries
end
```

### Creating Directories

```lumen
use tool fs.mkdir as MakeDir
use tool fs.write as WriteFile

cell create_project(name: String) -> Null / {fs}
  MakeDir(path: "{name}/src")
  MakeDir(path: "{name}/tests")
  WriteFile(path: "{name}/lumen.toml", content: "# Config")
  return null
end
```

## Tool Schemas

### fs.read

**Input**:
```json
{
  "path": "/data/input.txt"
}
```

**Output**:
```json
{
  "content": "file contents as string"
}
```

### fs.write

**Input**:
```json
{
  "path": "/output/result.txt",
  "content": "data to write"
}
```

**Output**:
```json
{
  "success": true
}
```

### fs.exists

**Input**:
```json
{
  "path": "/data/file.txt"
}
```

**Output**:
```json
{
  "exists": true
}
```

### fs.list

**Input**:
```json
{
  "path": "/data"
}
```

**Output**:
```json
{
  "entries": ["file1.txt", "file2.txt", "subdir/"]
}
```

Directories have trailing `/`.

### fs.mkdir

**Input**:
```json
{
  "path": "/data/new/nested/dir",
  "recursive": true
}
```

**Output**:
```json
{
  "success": true
}
```

### fs.remove

**Input**:
```json
{
  "path": "/data/temp.txt"
}
```

**Output**:
```json
{
  "success": true
}
```

## Configuration

In `lumen.toml`:

```toml
[providers]
fs.read = "builtin-fs"
fs.write = "builtin-fs"
fs.exists = "builtin-fs"
fs.list = "builtin-fs"
fs.mkdir = "builtin-fs"
fs.remove = "builtin-fs"

# Grant constraints
[tools.fs]
allowed_paths = ["/data/*", "/output/*", "~/.config/lumen/*"]
deny_paths = ["/etc/*", "/sys/*"]
max_file_size = 10485760  # 10MB
```

## Integration

Register with the runtime:

```rust
use lumen_rt::services::tools::ProviderRegistry;
use lumen_provider_fs::FsProvider;

let mut registry = ProviderRegistry::new();
registry.register(Box::new(FsProvider::new()));
```

## Error Handling

Returns `ToolError` variants:
- **`InvalidArgs`**: Missing `path` field or malformed input
- **`ExecutionFailed`**: Permission denied, file not found, I/O error
- **`Timeout`**: Operation exceeded grant policy timeout

## Security

- **Path validation**: Grant policies can restrict allowed/denied paths using glob patterns
- **Sandbox support**: Integration with capability-based sandboxing (future)
- **No symlink following**: Prevents directory traversal attacks
- **UTF-8 validation**: `fs.read` only returns valid UTF-8 (binary files rejected)

## Testing

```bash
cargo test -p lumen-provider-fs

# Specific tests
cargo test -p lumen-provider-fs read
cargo test -p lumen-provider-fs write
cargo test -p lumen-provider-fs mkdir
```

Tests use temporary directories via `tempfile` to avoid side effects.

## Limitations

- `fs.read` requires UTF-8 encoding (use `fs.read_bytes` for binary data)
- `fs.remove` only removes empty directories (use recursive variant for non-empty)
- No glob expansion (caller must expand patterns)
- No file watching (see `lumen-rt::services::fs_async` for async watching)

## Related Crates

- **lumen-rt** — Defines `ToolProvider` trait, async file operations
- **lumen-cli** — Registers filesystem provider by default
- **std::fs** — Underlying filesystem API
