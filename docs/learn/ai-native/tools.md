# Tutorial: Tools and Grants

Learn how to use external tools with policy constraints.

## What Are Tools?

Tools are typed interfaces to external services:
- LLM providers (OpenAI, Anthropic, Ollama)
- HTTP APIs
- Databases
- File systems
- MCP servers

Tools are abstract at the language level—the implementation is determined by runtime configuration.

## Declaring Tools

```lumen
use tool llm.chat as Chat
use tool http.get as HttpGet
use tool postgres.query as DbQuery
```

### The `as` Clause

The `as` keyword creates an alias for the tool:

```lumen
use tool llm.chat as AI          # Use 'AI' in your code
use tool http.get as Fetch       # Use 'Fetch' in your code
```

## Using Tools

Call tools like functions with named arguments:

```lumen
use tool llm.chat as Chat

cell ask(prompt: String) -> String
  return Chat(prompt: prompt)
end
```

With multiple parameters:

```lumen
cell search(query: String, limit: Int) -> Json
  return Search(query: query, max_results: limit)
end
```

## Grants: Policy Constraints

Grants restrict how tools can be used:

```lumen
use tool llm.chat as Chat

grant Chat
  model "gpt-4o"
  max_tokens 1024
  temperature 0.7
  timeout_ms 30000
```

### Common Constraints

| Constraint | Applies To | Description |
|------------|------------|-------------|
| `model` | LLM | Model identifier |
| `max_tokens` | LLM | Maximum output tokens |
| `temperature` | LLM | Sampling temperature |
| `timeout_ms` | All | Request timeout |
| `domain` | HTTP | Allowed URL patterns |

### Multiple Grants

```lumen
grant Chat max_tokens 512
grant Chat temperature 0.3
grant Chat timeout_ms 10000
```

### Domain Restrictions

```lumen
use tool http.get as Fetch

grant Fetch domain "*.trusted.com"
grant Fetch domain "api.example.com"
grant Fetch timeout_ms 5000
```

## Effects

Tools produce effects that must be declared:

```lumen
use tool llm.chat as Chat
bind effect llm to Chat

cell summarize(text: String) -> String / {llm}
  return Chat(prompt: "Summarize: {text}")
end
```

### Effect Inference

Effects are inferred through call chains:

```lumen
cell a() -> String / {llm}
  return Chat(prompt: "Hello")
end

cell b() -> String        # Infers {llm} from a()
  return a()
end
```

### Multiple Effects

```lumen
cell pipeline() -> String / {http, llm, trace}
  let data = Fetch(url: "https://api.example.com/data")
  let summary = Chat(prompt: "Summarize: {data}")
  emit("processed")
  return summary
end
```

## Binding Effects to Tools

The `bind effect` declaration maps effects to tools:

```lumen
use tool llm.chat as Chat
use tool http.get as Fetch

bind effect llm to Chat
bind effect http to Fetch
```

This enables:
- Effect provenance in error messages
- Policy enforcement at the effect level

## Practical Example: AI Assistant

```lumen
use tool llm.chat as Chat

grant Chat
  model "gpt-4o"
  max_tokens 2048
  temperature 0.7

bind effect llm to Chat

agent Assistant
  cell respond(message: String) -> String / {llm}
    role system: You are a helpful coding assistant.
    role user: {message}
    return Chat(prompt: message)
  end
end

cell main() -> String / {llm}
  let bot = Assistant()
  return bot.respond("What is pattern matching?")
end
```

## Configuration

Tools are configured in `lumen.toml`:

```toml
[providers]
llm.chat = "openai-compatible"
http.get = "builtin-http"

[providers.config.openai-compatible]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
default_model = "gpt-4"
```

### Switching Providers

Same code, different configuration:

```toml
# For Ollama (local)
[providers.config.openai-compatible]
base_url = "http://localhost:11434/v1"
default_model = "llama3"
```

## Best Practices

1. **Always use grants** — Constrain tool usage explicitly
2. **Declare effects** — Make side effects visible
3. **Use meaningful aliases** — `Chat` vs `LlmChat`
4. **Set timeouts** — Prevent hanging on slow responses
5. **Validate outputs** — Check results before using

## Next Steps

- [Agents](./agents) — Encapsulate AI behavior
- [Tool Reference](../../reference/tools) — Complete tool documentation
- [Providers Guide](../../guide/providers) — Configure tool providers
