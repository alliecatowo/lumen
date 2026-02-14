# CLI Reference

Complete command-line interface documentation.

### One-liner (Recommended)

Install the latest version of Lumen and the Language Server:

```bash
curl -fsSL https://raw.githubusercontent.com/alliecatowo/lumen/main/scripts/install.sh | sh
```

### From Source

Requires [Rust](https://rustup.rs/):

```bash
cargo install lumen-lang
```

## Global Options

| Option | Description |
|--------|-------------|
| `--help` | Show help |
| `--version` | Show version |

## Commands

### check

Type-check a Lumen file without running:

```bash
lumen check <file>
```

Options:
| Flag | Description |
|------|-------------|
| `--strict` | Enable strict mode (default) |
| `--no-strict` | Disable strict mode |

Example:
```bash
lumen check program.lm.md
```

### run

Compile and execute a Lumen file:

```bash
lumen run <file> [--cell <name>] [--trace-dir <dir>]
```

Options:
| Flag | Description |
|------|-------------|
| `--cell <name>` | Cell to execute (default: `main`) |
| `--trace-dir <dir>` | Directory for trace output |
| `--strict` | Enable strict mode |
| `--no-strict` | Disable strict mode |

Examples:
```bash
# Run main cell
lumen run program.lm.md

# Run specific cell
lumen run program.lm.md --cell process

# Enable tracing
lumen run program.lm.md --trace-dir ./traces
```

### emit

Emit LIR (intermediate representation) as JSON:

```bash
lumen emit <file> [--output <path>]
```

Options:
| Flag | Description |
|------|-------------|
| `--output <path>` | Output file path (default: stdout) |

Example:
```bash
lumen emit program.lm.md --output program.lir.json
```

### repl

Start an interactive REPL:

```bash
lumen repl
```

REPL Commands:
| Command | Description |
|---------|-------------|
| `:help` | Show available commands |
| `:quit` | Exit REPL |
| `:load <file>` | Load a file |
| `:type <expr>` | Show type of expression |
| `:clear` | Clear the screen |

Example:
```
lumen> let x = 42
lumen> x * 2
84
lumen> :type x
Int
lumen> :quit
```

### fmt

Format Lumen source files:

```bash
lumen fmt <files...> [--check]
```

Options:
| Flag | Description |
|------|-------------|
| `--check` | Check formatting without modifying |

Examples:
```bash
# Format files
lumen fmt src/*.lm.md

# Check formatting (CI)
lumen fmt --check src/*.lm.md
```

### init

Create a new Lumen project:

```bash
lumen init
```

Creates:
- `lumen.toml` — Configuration file
- `src/main.lm.md` — Entry point

### pkg

Package management commands:

```bash
lumen pkg init [name]     # Create new package
lumen pkg build           # Build package
lumen pkg check           # Type-check package
lumen pkg test            # Run tests
lumen pkg add <dep>       # Add dependency
lumen pkg publish         # Publish to registry
```

### trace

Work with execution traces:

```bash
lumen trace show <run-id> [--trace-dir <dir>]
```

Options:
| Flag | Description |
|------|-------------|
| `--trace-dir <dir>` | Trace directory |

Example:
```bash
lumen trace show abc123 --trace-dir ./traces
```

### cache

Manage tool result cache:

```bash
lumen cache clear [--cache-dir <dir>]
```

### build wasm

Build for WebAssembly:

```bash
lumen build wasm --target <web|nodejs|wasi>
```

Options:
| Flag | Description |
|------|-------------|
| `--target <target>` | Target platform |

Examples:
```bash
# Browser (ES modules)
lumen build wasm --target web

# Node.js (CommonJS)
lumen build wasm --target nodejs

# WASI (Wasmtime, etc.)
lumen build wasm --target wasi
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Type error |
| 3 | Runtime error |
| 4 | Parse error |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `LUMEN_CACHE_DIR` | Cache directory |
| `LUMEN_CONFIG` | Config file path |
| `LUMEN_TRACE_DIR` | Default trace directory |

## Configuration File

`lumen.toml` in project root:

```toml
[package]
name = "my-project"
version = "0.1.0"

[providers]
llm.chat = "openai-compatible"
http.get = "builtin-http"

[providers.config.openai-compatible]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
default_model = "gpt-4"

[providers.mcp.github]
uri = "npx -y @modelcontextprotocol/server-github"
tools = ["github.create_issue", "github.search_repos"]
```

## Next Steps

- [Configuration](./configuration) — Detailed configuration
- [Providers](./providers) — Tool provider setup
