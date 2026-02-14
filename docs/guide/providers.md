# Tool Providers

Configure and use tool providers.

## Overview

Tool providers are runtime implementations of tool interfaces. The same Lumen code works with different providers by changing configuration.

## Provider Types

### OpenAI-Compatible

Works with OpenAI, Ollama, vLLM, and compatible APIs:

```toml
[providers]
llm.chat = "openai-compatible"

[providers.config.openai-compatible]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
default_model = "gpt-4o"
```

#### Configuration Options

| Option | Description | Default |
|--------|-------------|---------|
| `base_url` | API base URL | Required |
| `api_key_env` | Environment variable for API key | Required |
| `default_model` | Default model ID | `gpt-4` |
| `organization` | OpenAI organization ID | None |
| `timeout_ms` | Request timeout | 30000 |
| `max_retries` | Maximum retries | 3 |

#### Ollama (Local)

```toml
[providers]
llm.chat = "openai-compatible"

[providers.config.openai-compatible]
base_url = "http://localhost:11434/v1"
default_model = "llama3"
# No API key needed
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

Built-in HTTP client:

```toml
[providers]
http.get = "builtin-http"
http.post = "builtin-http"

[providers.config.builtin-http]
max_redirects = 5
timeout_ms = 30000
user_agent = "Lumen/1.0"
```

### PostgreSQL

```toml
[providers]
postgres.query = "postgres"

[providers.config.postgres]
connection_string_env = "DATABASE_URL"
pool_size = 10
```

## Provider Interface

Every provider implements:

```rust
trait ToolProvider {
    fn name() -> String;
    fn version() -> String;
    fn schema() -> ToolSchema;
    fn call(input: Json) -> result;
    fn effects() -> list[EffectKind];
    fn capabilities() -> list[Capability>;
}
```

## Capabilities

Providers advertise capabilities:

| Capability | Description |
|------------|-------------|
| `TextGeneration` | Basic text generation |
| `Chat` | Multi-turn conversation |
| `Embedding` | Text embeddings |
| `Vision` | Image processing |
| `ToolUse` | Function calling |
| `StructuredOutput` | JSON schema output |
| `Streaming` | Streaming responses |

Check capabilities:

```rust
provider.has_capability(Capability::Vision)
```

## Retry Policy

Configure retry behavior:

```toml
[providers.config.openai-compatible.retry]
max_retries = 3
base_delay_ms = 100
max_delay_ms = 10000
retry_on = ["RateLimit", "Timeout", "ProviderUnavailable"]
```

## Error Handling

### Error Types

| Error | Description |
|-------|-------------|
| `NotFound` | Tool not registered |
| `InvalidArgs` | Invalid arguments |
| `ExecutionFailed` | General failure |
| `RateLimit` | Rate limited |
| `AuthError` | Auth failure |
| `ModelNotFound` | Model unavailable |
| `Timeout` | Request timeout |
| `ProviderUnavailable` | Service down |

### Handling in Code

```lumen
cell safe_call(prompt: String) -> result[String, String] / {llm}
  let result = Chat(prompt: prompt)
  
  match result
    response: String -> return ok(response)
    error: ToolError ->
      match error
        RateLimit(retry_after) -> return err("Rate limited, retry in {retry_after}ms")
        Timeout(elapsed, limit) -> return err("Timed out after {elapsed}ms")
        AuthError(msg) -> return err("Auth failed: {msg}")
        _ -> return err("Failed: {error}")
      end
  end
end
```

## Multiple Providers

Configure multiple providers:

```toml
[providers]
llm.chat = "openai-compatible"
llm.embed = "openai-compatible-embed"

[providers.config.openai-compatible]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
default_model = "gpt-4o"

[providers.config.openai-compatible-embed]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
default_model = "text-embedding-3-small"
```

## Provider Selection

Choose providers at runtime:

```toml
[providers]
llm.chat = "${LLM_PROVIDER:-openai-compatible}"
```

Then:

```bash
# Use default
lumen run app.lm.md

# Use different provider
LLM_PROVIDER=anthropic lumen run app.lm.md
```

## Best Practices

1. **Use environment variables** — Never hardcode secrets
2. **Set timeouts** — Prevent hanging
3. **Configure retries** — Handle transient failures
4. **Test with local** — Use Ollama for development
5. **Monitor usage** — Track costs and performance

## Next Steps

- [Configuration](./configuration) — Full configuration reference
- [Tools](../reference/tools) — Tool system
