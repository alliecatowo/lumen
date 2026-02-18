---
name: cli-commands
description: Complete CLI command reference and package manager documentation for lumen check, run, emit, fmt, pkg, repl, and build commands
---

# Lumen CLI Reference

## Commands

### `lumen check <file>`
Type-check a `.lm`, `.lm.md`, or `.lumen` file. Automatically resolves imports.

### `lumen run <file> [--cell <name>] [--trace-dir <dir>]`
Compile and execute. Default cell: `main`. Enable trace recording with `--trace-dir`.

### `lumen emit <file> [--output <path>]`
Emit LIR JSON to stdout or file. Useful for debugging bytecode.

### `lumen repl`
Interactive REPL with multi-line input, history, and session persistence.

### `lumen fmt <files> [--check]`
Format source files. `--check` mode for CI (exit 1 if changes needed).

### `lumen pkg init [name]` / `lumen pkg build` / `lumen pkg check`
Package manager commands. All packages must use `@namespace/name` format.

### `lumen trace show <run-id> [--graph]`
Display trace events. `--graph` shows execution DAG.

### `lumen cache clear`
Clear tool result cache.

### `lumen build wasm --target <web|nodejs|wasi>`
Build WASM target (requires wasm-pack).

### `lumen lang-ref [--format json]`
Print language reference documentation.

### `lumen init`
Create `lumen.toml` config file.

## CLI Architecture (`rust/lumen-cli/src/`)
- `bin/lumen.rs`: Clap-based entry point
- `repl.rs`: Interactive REPL with rustyline
- `fmt.rs`: AST-based pretty-printer
- `module_resolver.rs`: File-based module resolution (.lm.md → .lm → .lumen)

## Package Manager ("Wares")
- `wares/`: Package manager implementation
- `registry.rs`: Content-addressed registry (Cloudflare R2), artifact upload/download
- `workspace.rs`: Multi-package workspace with topological sort
- `binary_cache.rs`: Content-addressable LRU build cache

## Security Infrastructure
- `auth.rs`: Ed25519 signing via `ed25519-dalek`, API tokens
- `oidc.rs`: OpenID Connect token verification
- `tuf.rs`: The Update Framework (4 roles: Root/Targets/Snapshot/Timestamp)
- `transparency.rs`: Merkle tree append-only log for package publishing
- `audit.rs`: Structured audit logging

## Configuration (`lumen.toml`)
```toml
[providers]
llm.chat = "openai-compatible"
http.get = "builtin-http"

[providers.config.openai-compatible]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
default_model = "gpt-4"

[providers.mcp.github]
uri = "npx -y @modelcontextprotocol/server-github"
tools = ["github.create_issue"]
```
Search order: `./lumen.toml` → parent dirs → `~/.config/lumen/lumen.toml`
