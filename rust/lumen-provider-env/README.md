# lumen-provider-env

Environment variable and system information tool provider for Lumen.

## Overview

`lumen-provider-env` implements the `ToolProvider` trait to expose environment operations as callable tools in Lumen programs. It provides safe access to environment variables, working directory, home directory, platform information, and command-line arguments without requiring direct system call privileges.

All operations are read-only (except `env.set` which modifies process-local environment) and respect grant policy constraints for security.

## Provided Tools

| Tool | Description |
|------|-------------|
| `env.get` | Get environment variable value |
| `env.set` | Set environment variable (process-local) |
| `env.list` | List all environment variables |
| `env.has` | Check if environment variable exists |
| `env.cwd` | Get current working directory |
| `env.home` | Get user home directory |
| `env.platform` | Get platform string (os-arch) |
| `env.args` | Get command-line arguments |

## Usage in Lumen

### Environment Variables

```lumen
use tool env.get as GetEnv
use tool env.has as HasEnv

cell load_config() -> String? / {env}
  if HasEnv(name: "CONFIG_PATH")
    return GetEnv(name: "CONFIG_PATH")
  end
  return null
end

cell get_api_key() -> String / {env}
  let key = GetEnv(name: "API_KEY")
  if key == null
    halt("API_KEY environment variable not set")
  end
  return key
end
```

### System Information

```lumen
use tool env.cwd as GetCwd
use tool env.home as GetHome
use tool env.platform as GetPlatform

cell show_info() -> String / {env}
  let cwd = GetCwd()
  let home = GetHome()
  let platform = GetPlatform()
  return "Platform: {platform}\nHome: {home}\nCwd: {cwd}"
end
```

### Command-Line Arguments

```lumen
use tool env.args as GetArgs

cell parse_cli() -> list[String] / {env}
  let args = GetArgs()
  # Skip program name (first arg)
  return drop(args, 1)
end
```

### Listing All Variables

```lumen
use tool env.list as ListEnv

cell debug_env() -> map[String, String] / {env}
  return ListEnv()
end
```

## Tool Schemas

### env.get

**Input**:
```json
{
  "name": "HOME"
}
```

**Output**:
```json
{
  "value": "/home/user"
}
```

Returns `null` if variable not set.

### env.set

**Input**:
```json
{
  "name": "MY_VAR",
  "value": "my_value"
}
```

**Output**:
```json
{
  "success": true
}
```

**Note**: Only affects current process and child processes, not parent.

### env.has

**Input**:
```json
{
  "name": "PATH"
}
```

**Output**:
```json
{
  "exists": true
}
```

### env.list

**Input**: (none)

**Output**:
```json
{
  "vars": {
    "PATH": "/usr/bin:/bin",
    "HOME": "/home/user",
    "USER": "user"
  }
}
```

### env.cwd

**Input**: (none)

**Output**:
```json
{
  "path": "/home/user/project"
}
```

### env.home

**Input**: (none)

**Output**:
```json
{
  "path": "/home/user"
}
```

### env.platform

**Input**: (none)

**Output**:
```json
{
  "platform": "linux-x86_64"
}
```

Possible values: `linux-x86_64`, `darwin-aarch64`, `windows-x86_64`, etc.

### env.args

**Input**: (none)

**Output**:
```json
{
  "args": ["program", "arg1", "arg2"]
}
```

First element is the program name.

## Configuration

In `lumen.toml`:

```toml
[providers]
env.get = "builtin-env"
env.set = "builtin-env"
env.list = "builtin-env"
env.has = "builtin-env"
env.cwd = "builtin-env"
env.home = "builtin-env"
env.platform = "builtin-env"
env.args = "builtin-env"

# Grant constraints
[tools.env]
allowed_vars = ["HOME", "USER", "PATH", "API_KEY"]
deny_vars = ["PASSWORD", "SECRET"]
```

## Integration

Register with the runtime:

```rust
use lumen_rt::services::tools::ProviderRegistry;
use lumen_provider_env::EnvProvider;

let mut registry = ProviderRegistry::new();
registry.register(Box::new(EnvProvider::new()));
```

## Error Handling

Returns `ToolError` variants:
- **`InvalidArgs`**: Missing `name` field
- **`ExecutionFailed`**: Environment variable not found (for `env.get`), I/O error

## Security

- **Read-only by default**: Only `env.set` mutates state
- **Process-local**: `env.set` doesn't affect parent process or other programs
- **Grant policies**: Can restrict which variables are accessible
- **No privilege escalation**: Cannot access privileged system variables

## Testing

```bash
cargo test -p lumen-provider-env

# Specific tests
cargo test -p lumen-provider-env get
cargo test -p lumen-provider-env set
cargo test -p lumen-provider-env platform
```

## Platform Support

| Operation | Linux | macOS | Windows |
|-----------|-------|-------|---------|
| `env.get` | ✓ | ✓ | ✓ |
| `env.set` | ✓ | ✓ | ✓ |
| `env.list` | ✓ | ✓ | ✓ |
| `env.cwd` | ✓ | ✓ | ✓ |
| `env.home` | ✓ | ✓ | ✓ |
| `env.platform` | ✓ | ✓ | ✓ |
| `env.args` | ✓ | ✓ | ✓ |

## Related Crates

- **lumen-rt** — Defines `ToolProvider` trait
- **lumen-cli** — Registers environment provider by default
- **std::env** — Underlying Rust environment API
