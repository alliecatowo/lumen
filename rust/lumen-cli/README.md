# lumen-cli

Command-line interface, package manager ("Wares"), and security infrastructure for Lumen.

## Overview

`lumen-cli` provides the primary interface for working with Lumen: compiling programs, running code, type-checking, formatting, managing packages, and interacting with the REPL. It orchestrates the compiler and VM, manages multi-file imports, handles configuration, and implements a full-featured package manager with supply-chain security (Ed25519 signing, TUF metadata verification, transparency logs).

The CLI is built with Clap for command parsing, includes a rustyline-based REPL with multi-line input support, and provides an AST-based code formatter. The package manager "Wares" handles dependency resolution, binary caching, workspace management, and registry operations backed by Cloudflare R2.

## Architecture

### CLI Commands

| Command | Module | Purpose |
|---------|--------|---------|
| `lumen check` | `bin/lumen.rs` | Type-check without execution |
| `lumen run` | `bin/lumen.rs` | Compile and execute |
| `lumen emit` | `bin/lumen.rs` | Emit LIR JSON |
| `lumen repl` | `repl.rs` | Interactive REPL |
| `lumen fmt` | `fmt.rs` | Code formatter |
| `lumen pkg` | `wares/` | Package manager commands |
| `lumen trace` | `bin/lumen.rs` | Trace inspection |
| `lumen cache` | `bin/lumen.rs` | Cache management |
| `lumen build wasm` | `bin/lumen.rs` | WASM target build |
| `lumen lang-ref` | `bin/lumen.rs` | Language reference |

### Package Manager ("Wares")

| Module | Purpose |
|--------|---------|
| `wares/` | Package manager root (commands, publishing, installation) |
| `registry.rs` | Content-addressed registry client (R2 storage) |
| `workspace.rs` | Multi-package workspace resolver with topological sort |
| `binary_cache.rs` | Content-addressable LRU build cache |
| `service_template.rs` | Service project scaffolding (REST, WebSocket, CRUD) |

### Security Infrastructure

| Module | Purpose |
|--------|---------|
| `auth.rs` | Ed25519 key generation and signing, API token management |
| `oidc.rs` | OpenID Connect ID token verification |
| `tuf.rs` | TUF 4-role metadata verification (Root, Targets, Snapshot, Timestamp) |
| `transparency.rs` | Merkle tree transparency log for package publishing |
| `audit.rs` | Structured audit logging for security events |
| `error_chain.rs` | Error chain formatting for diagnostics |

### Other Modules

| Module | Purpose |
|--------|---------|
| `module_resolver.rs` | File-based import resolution (`.lm.md` → `.lm` → `.lumen`) |
| `config.rs` | Configuration loading (`lumen.toml`) |
| `ci.rs` | CI configuration (Miri, coverage gates, sanitizers) |
| `bindgen.rs` | C-to-Lumen FFI bindgen |
| `dap.rs` | Debug Adapter Protocol server (WIP) |

## Key Commands

### Check

Type-check a Lumen source file without executing:

```bash
lumen check factorial.lm
lumen check examples/*.lm.md
```

### Run

Compile and execute a Lumen program:

```bash
lumen run main.lm                  # Execute default 'main' cell
lumen run script.lm --cell process # Execute specific cell
lumen run app.lm --trace-dir ./traces  # Enable trace recording
```

### Emit

Output LIR bytecode as JSON:

```bash
lumen emit code.lm                 # Print to stdout
lumen emit code.lm --output lir.json  # Write to file
```

### REPL

Launch an interactive Read-Eval-Print-Loop:

```bash
lumen repl
```

Features:
- Multi-line input detection
- Command history with navigation
- Line editing (rustyline)
- Access to previously defined functions/variables

### Format

Format Lumen source files:

```bash
lumen fmt src/*.lm                # Format files in-place
lumen fmt --check src/*.lm        # Check formatting (CI mode)
```

Preserves:
- Comments and docstrings
- Markdown blocks in `.lm`/`.lumen` files
- Code block attachment to declarations

### Package Manager

```bash
# Initialize new package
lumen pkg init my-app

# Build package and dependencies
lumen pkg build

# Type-check package
lumen pkg check

# Publish to registry (requires authentication)
lumen pkg publish

# Install package
lumen pkg install @namespace/package

# Show package info
lumen pkg info @namespace/package
```

Also available as standalone `wares` binary:

```bash
wares init my-package
wares build
wares publish
```

### Trace

Inspect recorded trace events:

```bash
lumen trace show <run-id>          # Display trace events
```

### Cache

Manage build cache:

```bash
lumen cache clear                  # Clear cached results
```

### Build WASM

Compile to WebAssembly:

```bash
lumen build wasm --target web      # Browser (ES modules)
lumen build wasm --target nodejs   # Node.js (CommonJS)
```

### Language Reference

Print language reference:

```bash
lumen lang-ref                     # Human-readable format
lumen lang-ref --format json       # Machine-readable JSON
```

## Configuration

Runtime configuration in `lumen.toml` (searched in `.`, parent dirs, then `~/.config/lumen/`):

```toml
# Provider mappings
[providers]
llm.chat = "openai-compatible"
http.get = "builtin-http"

# Provider settings
[providers.config.openai-compatible]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
default_model = "gpt-4"

# MCP servers
[providers.mcp.github]
uri = "npx -y @modelcontextprotocol/server-github"
tools = ["github.create_issue", "github.search_repos"]
```

Secrets use `api_key_env` to reference environment variables — never stored directly.

## Usage as a Library

```rust
use lumen_cli::module_resolver::ModuleResolver;
use lumen_compiler::compile_with_imports;

let resolver = ModuleResolver::new("/path/to/project");
let imports = resolver.resolve_imports(&main_source)?;
let module = compile_with_imports(&main_source, imports)?;
```

## Testing

```bash
cargo test -p lumen-cli

# Specific test suites
cargo test -p lumen-cli module_resolver
cargo test -p lumen-cli tuf
cargo test -p lumen-cli registry
```

## Security Features

**Ed25519 Signing**: Real key generation and signing via `ed25519-dalek` for package publishing and registry authentication.

**TUF Metadata**: Full The Update Framework implementation with:
- Four roles (Root, Targets, Snapshot, Timestamp)
- Threshold signing (configurable required signatures per role)
- Rollback detection (monotonic version numbers)
- Expiration enforcement
- Root rotation via cross-signed roots

**OIDC Authentication**: OpenID Connect token verification for registry login.

**Transparency Log**: Merkle tree append-only log providing tamper-evident record of all package publishes.

**Audit Logging**: Structured logging of security-relevant operations.

## Features

Default features: `http`, `json`, `fs`, `env`, `crypto`, `keyring`, `ed25519`, `jit`

- **`http`** — HTTP tool provider
- **`json`** — JSON tool provider
- **`fs`** — Filesystem tool provider
- **`env`** — Environment variable provider
- **`crypto`** — Cryptography provider
- **`gemini`** — Gemini AI provider
- **`keyring`** — OS keyring integration for API tokens
- **`ed25519`** — Ed25519 signing (requires `ed25519-dalek`)
- **`jit`** — JIT compilation via `lumen-codegen`

## Related Crates

- **lumen-compiler** — Invoked for compilation
- **lumen-rt** — Invoked for VM execution
- **lumen-provider-*** — Tool providers registered at runtime
- **lumen-lsp** — Language server (shares module resolution logic)
