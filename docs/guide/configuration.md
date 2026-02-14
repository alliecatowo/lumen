# Configuration

Configure Lumen with `lumen.toml`.

## Configuration File

Lumen looks for `lumen.toml` in:
1. Current directory
2. Parent directories (walking up)
3. `~/.config/lumen/lumen.toml`

## Package Configuration

```toml
[package]
name = "my-project"
version = "0.1.0"
description = "A Lumen project"
authors = ["Your Name <you@example.com>"]
license = "MIT"
```

## Tool Providers

Map tools to implementations:

```toml
[providers]
llm.chat = "openai-compatible"
llm.embed = "openai-compatible"
http.get = "builtin-http"
http.post = "builtin-http"
postgres.query = "postgres"
```

### OpenAI-Compatible

```toml
[providers.config.openai-compatible]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
default_model = "gpt-4o"

# Optional
organization = "org-xxx"
timeout_ms = 30000
max_retries = 3
```

### Ollama (Local)

```toml
[providers]
llm.chat = "openai-compatible"

[providers.config.openai-compatible]
base_url = "http://localhost:11434/v1"
default_model = "llama3"
# No API key needed for local
```

### Anthropic

```toml
[providers]
llm.chat = "anthropic"

[providers.config.anthropic]
api_key_env = "ANTHROPIC_API_KEY"
default_model = "claude-3-opus-20240229"
```

### HTTP Provider

```toml
[providers.config.builtin-http]
max_redirects = 5
timeout_ms = 30000
user_agent = "Lumen/1.0"
```

### PostgreSQL

```toml
[providers.config.postgres]
connection_string_env = "DATABASE_URL"
pool_size = 10
```

## MCP Servers

Configure Model Context Protocol servers:

```toml
[providers.mcp.github]
uri = "npx -y @modelcontextprotocol/server-github"
tools = ["github.create_issue", "github.search_repos"]
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }

[providers.mcp.filesystem]
uri = "npx -y @modelcontextprotocol/server-filesystem /allowed/path"
tools = ["filesystem.read_file", "filesystem.write_file"]
```

### MCP Configuration Options

| Field | Description |
|-------|-------------|
| `uri` | Command or URL to launch server |
| `tools` | List of tool names exposed |
| `env` | Environment variables |
| `timeout_ms` | Request timeout |

## Provider Types

| Type | Description |
|------|-------------|
| `openai-compatible` | OpenAI, Ollama, vLLM, etc. |
| `anthropic` | Anthropic Claude API |
| `builtin-http` | Built-in HTTP client |
| `postgres` | PostgreSQL client |
| `mcp` | MCP server bridge |

## Secrets

**Never** put secrets directly in `lumen.toml`:

```toml
# BAD
api_key = "sk-xxx"  # Don't do this!

# GOOD
api_key_env = "OPENAI_API_KEY"  # Reference environment variable
```

## Multiple Environments

Use environment-specific config:

```bash
# Development
LUMEN_ENV=dev lumen run app.lm.md

# Production
LUMEN_ENV=prod lumen run app.lm.md
```

With `lumen.dev.toml` and `lumen.prod.toml`.

## Example: Full Configuration

```toml
[package]
name = "ai-assistant"
version = "1.0.0"

[providers]
llm.chat = "openai-compatible"
llm.embed = "openai-compatible"
http.get = "builtin-http"

[providers.config.openai-compatible]
base_url = "${OPENAI_BASE_URL:-https://api.openai.com/v1}"
api_key_env = "OPENAI_API_KEY"
default_model = "gpt-4o"
timeout_ms = 60000

[providers.config.builtin-http]
max_redirects = 3
timeout_ms = 10000

[providers.mcp.github]
uri = "npx -y @modelcontextprotocol/server-github"
tools = ["github.search_repos", "github.create_issue"]
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }

[cache]
enabled = true
ttl_ms = 3600000  # 1 hour

[logging]
level = "info"
format = "json"
```

## Next Steps

- [CLI Reference](./cli) — Command-line tools
- [Providers](./providers) — Provider details
