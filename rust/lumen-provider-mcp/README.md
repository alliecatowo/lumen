# lumen-provider-mcp

Model Context Protocol (MCP) bridge provider for Lumen.

## Overview

`lumen-provider-mcp` implements the `ToolProvider` trait to expose MCP-compatible tool servers as Lumen tools. It provides a bridge between Lumen's tool dispatch system and external MCP servers (like GitHub, Slack, database adapters), allowing Lumen programs to call any MCP tool via JSON-RPC 2.0.

The provider manages subprocess lifecycle, stdio transport, tool discovery via `tools/list`, and bidirectional JSON-RPC communication. It supports multiple MCP servers simultaneously, each exposing their own set of tools.

## Architecture

| Component | Purpose |
|-----------|---------|
| **McpTransport** | Transport abstraction (stdio, HTTP future) |
| **StdioTransport** | Subprocess stdio implementation |
| **McpProvider** | ToolProvider implementation per server |
| **McpToolSchema** | Tool schema from MCP `tools/list` |

## MCP Server Configuration

In `lumen.toml`:

```toml
[providers.mcp.github]
uri = "npx -y @modelcontextprotocol/server-github"
tools = ["github.create_issue", "github.search_repos", "github.get_file"]
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }

[providers.mcp.postgres]
uri = "npx -y @modelcontextprotocol/server-postgres"
tools = ["postgres.query", "postgres.schema"]
env = { DATABASE_URL = "${DATABASE_URL}" }

[providers.mcp.slack]
uri = "npx -y @modelcontextprotocol/server-slack"
tools = ["slack.send_message", "slack.list_channels"]
env = { SLACK_TOKEN = "${SLACK_TOKEN}" }
```

- **`uri`**: Command to spawn MCP server process
- **`tools`**: List of tool names to expose
- **`env`**: Environment variables (with `${VAR}` expansion)

## Usage in Lumen

### GitHub Integration

```lumen
use tool github.create_issue as CreateIssue
use tool github.search_repos as SearchRepos

grant CreateIssue timeout_ms 10000

cell file_bug(title: String, body: String) -> Json / {mcp}
  let result = CreateIssue(
    title: title,
    body: body,
    repo: "owner/repo"
  )
  return result
end

cell find_repos(query: String) -> list[Json] / {mcp}
  let results = SearchRepos(query: query, limit: 10)
  return results.items
end
```

### Database Queries

```lumen
use tool postgres.query as Query

grant Query timeout_ms 5000

cell get_users() -> list[Json] / {mcp}
  let rows = Query(sql: "SELECT * FROM users WHERE active = true")
  return rows
end
```

### Slack Notifications

```lumen
use tool slack.send_message as SendMessage

cell notify(channel: String, text: String) -> Null / {mcp}
  SendMessage(channel: channel, text: text)
  return null
end
```

## MCP Protocol

The provider implements the MCP JSON-RPC 2.0 protocol:

### Tool Discovery (tools/list)

```json
{
  "jsonrpc": "2.0",
  "method": "tools/list",
  "params": {},
  "id": 1
}
```

**Response**:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "tools": [
      {
        "name": "github.create_issue",
        "description": "Create a new GitHub issue",
        "input_schema": {
          "type": "object",
          "properties": {
            "title": {"type": "string"},
            "body": {"type": "string"}
          },
          "required": ["title", "body"]
        }
      }
    ]
  },
  "id": 1
}
```

### Tool Call (tools/call)

```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "github.create_issue",
    "arguments": {
      "title": "Bug report",
      "body": "Description of bug"
    }
  },
  "id": 2
}
```

**Response**:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"issue_number\": 123, \"url\": \"https://...\"}"
      }
    ]
  },
  "id": 2
}
```

## Integration

The provider is automatically registered when configured in `lumen.toml`:

```rust
use lumen_rt::services::tools::ProviderRegistry;
use lumen_provider_mcp::McpProvider;

let mut registry = ProviderRegistry::new();

// Providers created from config
for (server_name, config) in mcp_servers {
    let provider = McpProvider::new(server_name, config.uri, config.tools)?;
    registry.register(Box::new(provider));
}
```

## Process Lifecycle

1. **Lazy spawn**: Process spawned on first tool call
2. **Stdio transport**: JSON-RPC over stdin/stdout
3. **Keep-alive**: Process persists across multiple calls
4. **Graceful shutdown**: Process killed when provider dropped

## Error Handling

Returns `ToolError` variants:
- **`NotFound`**: Tool not discovered from `tools/list`
- **`InvalidArgs`**: Arguments don't match input schema
- **`ExecutionFailed`**: Process spawn failure, JSON-RPC error
- **`Timeout`**: Request exceeded time limit
- **`ProviderUnavailable`**: MCP server crashed or unresponsive

## Testing

```bash
cargo test -p lumen-provider-mcp

# Integration tests (require MCP servers)
MCP_TEST_GITHUB=1 cargo test -p lumen-provider-mcp github
```

Integration tests are ignored by default (require external services).

## Supported MCP Servers

| Server | Package | Tools |
|--------|---------|-------|
| GitHub | `@modelcontextprotocol/server-github` | create_issue, search_repos, get_file |
| Postgres | `@modelcontextprotocol/server-postgres` | query, schema, tables |
| Slack | `@modelcontextprotocol/server-slack` | send_message, list_channels |
| Filesystem | `@modelcontextprotocol/server-filesystem` | read, write, list |

See [MCP documentation](https://modelcontextprotocol.io/) for full server list.

## Future Enhancements

- HTTP transport for remote MCP servers
- Tool schema caching for faster startup
- Bidirectional streaming (MCP 2.0)
- Tool composition (chaining MCP tools)

## Related Crates

- **lumen-rt** — Defines `ToolProvider` trait
- **lumen-cli** — Loads MCP config from `lumen.toml`
- **serde_json** — JSON-RPC serialization
