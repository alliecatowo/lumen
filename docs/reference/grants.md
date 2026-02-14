# Grants & Policies

Policy constraints for tool usage.

## Overview

Grants attach policy constraints to tool aliases. They are provider-agnostic and restrict how any provider may be used.

## Basic Syntax

```lumen
use tool llm.chat as Chat

grant Chat
  model "gpt-4o"
  max_tokens 1024
  temperature 0.7
```

## Constraint Types

### LLM Constraints

| Constraint | Type | Description |
|------------|------|-------------|
| `model` | String | Model identifier |
| `max_tokens` | Int | Maximum output tokens |
| `temperature` | Float | Sampling temperature (0-2) |
| `top_p` | Float | Nucleus sampling |
| `stop` | String | Stop sequences |
| `timeout_ms` | Int | Request timeout |

### HTTP Constraints

| Constraint | Type | Description |
|------------|------|-------------|
| `domain` | String | Allowed URL pattern |
| `timeout_ms` | Int | Request timeout |
| `max_redirects` | Int | Maximum redirects |

### General Constraints

| Constraint | Type | Description |
|------------|------|-------------|
| `timeout_ms` | Int | Request timeout |
| `max_retries` | Int | Maximum retries |
| `effect` | String | Allowed effect kind |
| `effects` | list[String] | Allowed effect kinds |

## Multiple Constraints

```lumen
grant Chat
  model "gpt-4o"
  max_tokens 2048
  temperature 0.7
  timeout_ms 30000
```

Or separate declarations:

```lumen
grant Chat model "gpt-4o"
grant Chat max_tokens 2048
grant Chat temperature 0.7
```

## Domain Patterns

HTTP grants support glob patterns:

```lumen
grant Fetch domain "*.example.com"     # All subdomains
grant Fetch domain "api.example.com"   # Exact match
grant Fetch domain "*.trusted.org"     # Multiple patterns
```

### Pattern Syntax

| Pattern | Matches |
|---------|---------|
| `exact.com` | Exact domain only |
| `*.domain.com` | Any subdomain |
| `**.domain.com` | Any subdomain (recursive) |
| `*` | Any domain (not recommended) |

## Scoped Grants

Grants inside agents/processes are scoped:

```lumen
agent ConservativeBot
  use tool llm.chat as Chat
  grant Chat max_tokens 100    # Only affects this agent

agent CreativeBot
  use tool llm.chat as Chat
  grant Chat max_tokens 4000   # Different limit
```

## Effect Constraints

Restrict which effects a tool can produce:

```lumen
use tool external.api as External

grant External
  effect "http"
  effects ["http", "trace"]
  timeout_ms 5000
```

## Runtime Enforcement

Grants are validated at runtime:

```lumen
use tool http.get as Fetch
grant Fetch domain "*.example.com"

cell main() -> String / {http}
  # OK - matches pattern
  return Fetch(url: "https://api.example.com/data")
end

cell blocked() -> String / {http}
  # ERROR - domain not allowed
  return Fetch(url: "https://evil.com/data")
end
```

### Policy Violation Error

```
Error: ToolPolicyViolation
  Tool: Fetch
  Constraint: domain
  Value: "evil.com"
  Pattern: "*.example.com"
```

## Timeout Enforcement

```lumen
use tool llm.chat as Chat
grant Chat timeout_ms 5000

cell main() -> String / {llm}
  # If Chat takes > 5 seconds, it's terminated
  return Chat(prompt: "Hello")
end
```

## Token Budget

```lumen
use tool llm.chat as Chat
grant Chat max_tokens 512

cell main() -> String / {llm}
  # Response truncated at 512 tokens
  return Chat(prompt: "Write a novel")
end
```

## Multiple Aliases

Create aliases with different constraints:

```lumen
use tool llm.chat as FastChat
use tool llm.chat as SmartChat

grant FastChat model "gpt-3.5-turbo" max_tokens 256
grant SmartChat model "gpt-4o" max_tokens 4096

cell quick(question: String) -> String / {llm}
  return FastChat(prompt: question)
end

cell thorough(question: String) -> String / {llm}
  return SmartChat(prompt: question)
end
```

## Grant Merging

Multiple grants for the same alias are merged:

```lumen
grant Chat model "gpt-4o"
grant Chat max_tokens 1024
grant Chat timeout_ms 30000

# Equivalent to:
grant Chat
  model "gpt-4o"
  max_tokens 1024
  timeout_ms 30000
```

## Best Practices

1. **Always set timeouts** — Prevent hanging
2. **Use domain restrictions** — Limit HTTP access
3. **Set token limits** — Control costs
4. **Scope grants appropriately** — Least privilege
5. **Document constraints** — Explain why limits exist

## Example: Secure HTTP Client

```lumen
use tool http.get as Fetch

grant Fetch
  domain "*.internal.company.com"
  domain "api.trusted-partner.com"
  timeout_ms 10000
  max_redirects 3

cell fetch_internal(path: String) -> Json / {http}
  return Fetch(url: "https://internal.company.com{path}")
end

cell fetch_partner(endpoint: String) -> Json / {http}
  return Fetch(url: "https://api.trusted-partner.com/{endpoint}")
end
```

## Example: Production Bot

```lumen
use tool llm.chat as Chat

grant Chat
  model "gpt-4o"
  max_tokens 1024
  temperature 0.3      # Lower for consistency
  timeout_ms 15000

agent ProductionBot
  use tool llm.chat as Chat
  # Inherits grants from above
  
  cell respond(message: String) -> String / {llm}
    role system: You are a production assistant. Be concise and accurate.
    role user: {message}
    return Chat(prompt: message)
  end
end
```

## Next Steps

- [Tool Reference](/reference/tools) — Tool system
- [Configuration](/guide/configuration) — Provider setup
