# Tool Reference

Complete documentation for Lumen's tool system.

## Overview

Tools are typed interfaces to external services. At the language level, a tool is:
- A qualified name (e.g., `llm.chat`, `http.get`)
- Typed input (record of named arguments)
- Typed output
- Declared effects
- Automatic trace events

## Declaring Tools

```lumen
use tool llm.chat as Chat
use tool http.get as Fetch
use tool postgres.query as DbQuery
```

The `as` keyword creates an alias for use in your code.

## Calling Tools

Tools are called like functions with named arguments:

```lumen
let response = Chat(prompt: "Hello")
let data = Fetch(url: "https://api.example.com")
let results = DbQuery(sql: "SELECT * FROM users")
```

## Tool Types

### LLM Tools

| Tool | Input | Output | Effects |
|------|-------|--------|---------|
| `llm.chat` | prompt, model, temperature, etc. | String | llm |
| `llm.embed` | text, model | list[Float] | llm |
| `llm.complete` | prompt, max_tokens | String | llm |

### HTTP Tools

| Tool | Input | Output | Effects |
|------|-------|--------|---------|
| `http.get` | url, headers | Json | http |
| `http.post` | url, body, headers | Json | http |
| `http.put` | url, body, headers | Json | http |
| `http.delete` | url, headers | Json | http |

### Database Tools

| Tool | Input | Output | Effects |
|------|-------|--------|---------|
| `postgres.query` | sql, params | list[Json] | db |
| `postgres.execute` | sql, params | Int | db |
| `redis.get` | key | String | db |
| `redis.set` | key, value | Null | db |

### Filesystem Tools

| Tool | Input | Output | Effects |
|------|-------|--------|---------|
| `fs.read` | path | String | fs |
| `fs.write` | path, content | Null | fs |
| `fs.list` | path | list[String] | fs |
| `fs.delete` | path | Null | fs |

## Tool Schema

Every tool has a schema:

```lumen
# Input schema defines accepted arguments
input schema:
  prompt: String (required)
  model: String (default: "gpt-4")
  temperature: Float (default: 0.7)
  max_tokens: Int (default: 1024)

# Output schema defines return type
output schema:
  content: String
  tokens_used: Int
```

## Effect Binding

Map effects to tools:

```lumen
use tool llm.chat as Chat
bind effect llm to Chat

# Now Chat calls produce {llm} effect
cell ask(question: String) -> String / {llm}
  return Chat(prompt: question)
end
```

## Multiple Tools with Same Effect

```lumen
use tool llm.chat as Chat
use tool llm.embed as Embed

bind effect llm to Chat
bind effect llm to Embed

# Both produce {llm} effect
```

## Tool Aliases

Create multiple aliases with different constraints:

```lumen
use tool llm.chat as FastChat
use tool llm.chat as SmartChat

grant FastChat model "gpt-3.5-turbo" max_tokens 256
grant SmartChat model "gpt-4o" max_tokens 4096
```

## Tool Providers

Tools are backed by providers configured in `lumen.toml`:

```toml
[providers]
llm.chat = "openai-compatible"

[providers.config.openai-compatible]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
```

### Provider Interface

Every provider implements:

```rust
trait ToolProvider {
    fn name() -> String;
    fn version() -> String;
    fn schema() -> ToolSchema;
    fn call(input: Json) -> result;
    fn effects() -> list[EffectKind];
}
```

### Provider Capabilities

| Capability | Description |
|------------|-------------|
| `TextGeneration` | Basic text generation |
| `Chat` | Multi-turn conversation |
| `Embedding` | Text embeddings/vectors |
| `Vision` | Image input processing |
| `ToolUse` | Function/tool calling |
| `StructuredOutput` | JSON schema output |
| `Streaming` | Streaming responses |

## Custom Tools

Define custom tool interfaces:

```lumen
use tool mycompany.analyze as Analyze

grant Analyze
  timeout_ms 5000
  version "v2"

cell process(data: String) -> Analysis / {external}
  return Analyze(input: data, format: "json")
end
```

## Error Types

| Error | Description |
|-------|-------------|
| `NotFound` | Tool not registered |
| `InvalidArgs` | Missing or malformed arguments |
| `ExecutionFailed` | General execution error |
| `RateLimit` | Rate limit exceeded |
| `AuthError` | Authentication failure |
| `ModelNotFound` | Model not available |
| `Timeout` | Request timed out |
| `ProviderUnavailable` | Provider service down |
| `OutputValidationFailed` | Schema mismatch |

## Tracing

All tool calls are automatically traced:

```lumen
# Automatic trace includes:
# - Tool name
# - Input arguments
# - Output value
# - Duration
# - Provider identity
# - Status (success/failure)
```

View traces:

```bash
lumen trace show <run-id>
```

## Best Practices

1. **Always use grants** — Constrain tool behavior
2. **Bind effects** — Enable provenance tracking
3. **Handle errors** — Tools can fail
4. **Set timeouts** — Prevent hanging
5. **Use typed results** — Parse and validate outputs

## Next Steps

- [Grants Reference](./grants) — Policy constraints
- [Configuration](../guide/configuration) — Provider setup
