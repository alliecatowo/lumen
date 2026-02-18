# lumen-provider-http

HTTP tool provider for Lumen's runtime tool dispatch system.

## Overview

`lumen-provider-http` implements the `ToolProvider` trait to expose HTTP operations as callable tools in Lumen programs. It provides GET, POST, PUT, and DELETE requests with configurable headers, timeout, and response handling. Each HTTP method is exposed as a separate tool with a well-defined JSON schema.

The provider uses `reqwest` with blocking I/O and TLS support via `rustls`. All operations return structured responses with status code, headers, and body for comprehensive error handling and response processing.

## Provided Tools

| Tool | Method | Description |
|------|--------|-------------|
| `http.get` | GET | Fetch resource from URL |
| `http.post` | POST | Send data to URL |
| `http.put` | PUT | Update resource at URL |
| `http.delete` | DELETE | Remove resource at URL |

## Tool Schema

### Request Format

```json
{
  "url": "https://api.example.com/users",
  "headers": {
    "Authorization": "Bearer token",
    "Content-Type": "application/json"
  },
  "body": "{\"name\": \"Alice\"}"
}
```

- **`url`** (required): Target URL
- **`headers`** (optional): Custom HTTP headers
- **`body`** (optional): Request body for POST/PUT

### Response Format

```json
{
  "status": 200,
  "body": "{\"id\": 123, \"name\": \"Alice\"}",
  "headers": {
    "content-type": "application/json",
    "content-length": "32"
  }
}
```

- **`status`**: HTTP status code
- **`body`**: Response body as string
- **`headers`**: Response headers (lowercase keys)

## Usage in Lumen

```lumen
use tool http.get as HttpGet
use tool http.post as HttpPost

grant HttpGet domain "https://api.github.com/*"
grant HttpGet timeout_ms 5000

cell fetch_user(username: String) -> Json / {http}
  let url = "https://api.github.com/users/{username}"
  let response = HttpGet(url: url)
  return parse_json(response.body)
end

cell create_user(name: String) -> Json / {http}
  let body = to_json({"name": name})
  let response = HttpPost(
    url: "https://api.example.com/users",
    body: body,
    headers: {"Content-Type": "application/json"}
  )
  return parse_json(response.body)
end
```

## Configuration

In `lumen.toml`:

```toml
[providers]
http.get = "builtin-http"
http.post = "builtin-http"
http.put = "builtin-http"
http.delete = "builtin-http"

[providers.config.builtin-http]
timeout_ms = 30000
max_redirects = 5
user_agent = "Lumen/0.5.0"
```

## Integration

Register with the runtime:

```rust
use lumen_rt::services::tools::ProviderRegistry;
use lumen_provider_http::HttpProvider;

let mut registry = ProviderRegistry::new();
registry.register(Box::new(HttpProvider::new()));
```

## Error Handling

The provider returns `ToolError` variants:
- **`ExecutionFailed`**: Network error, DNS failure, connection timeout
- **`InvalidArgs`**: Missing `url` field or malformed input
- **`Timeout`**: Request exceeded time limit
- **`RateLimit`**: Server returned 429 Too Many Requests

## Testing

```bash
cargo test -p lumen-provider-http
```

Tests use local HTTP server fixtures to avoid network dependencies.

## Security

- Enforces grant policies (domain patterns, timeouts)
- No automatic cookie persistence (stateless)
- TLS verification enabled by default (rustls)
- No automatic redirects across domains

## Related Crates

- **lumen-rt** — Defines `ToolProvider` trait
- **reqwest** — HTTP client library
- **lumen-cli** — Registers HTTP provider by default
