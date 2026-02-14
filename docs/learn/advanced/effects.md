# Advanced: Effects System

Effects track side effects through your program.

## What Are Effects?

Effects describe what a function might do besides compute a value:
- Make HTTP requests
- Call LLM APIs
- Write to databases
- Emit trace events
- Access the filesystem

## Effect Rows

Declare effects in function signatures:

```lumen
cell fetch(url: String) -> String / {http}
  # This function makes HTTP requests
end

cell process(data: String) -> String / {llm, trace}
  # This function uses LLM and emits traces
end
```

### No Effects

Functions without effects are pure:

```lumen
cell add(a: Int, b: Int) -> Int
  # No side effects
  return a + b
end
```

### Multiple Effects

```lumen
cell pipeline() -> String / {http, llm, db, trace}
  # Multiple side effects
end
```

## Effect Inference

Effects are automatically inferred:

```lumen
cell a() -> String / {http}
  return fetch("https://api.example.com")
end

cell b() -> String        # Infers {http} from a()
  return a()
end

cell c() -> String        # Infers {http} from b()
  return b() ++ " processed"
end
```

## Effect Propagation

Effects flow through:
- Function calls
- Method calls
- Tool calls

```lumen
cell handler() -> String / {llm}
  let bot = Assistant()
  return bot.respond("Hello")  # Inherits llm effect
end
```

## Effect Bindings

Map effects to tools explicitly:

```lumen
use tool llm.chat as Chat
use tool http.get as Fetch

bind effect llm to Chat
bind effect http to Fetch
```

This enables:
- Clear error messages (which tool caused the effect)
- Policy enforcement at the effect level
- Effect provenance tracking

## Strict Mode

In strict mode, inferred effects must be declared:

```lumen
@strict true

cell a() -> String / {http}
  return fetch()
end

cell b() -> String        # ERROR: Undeclared effect {http}
  return a()
end
```

Fix by declaring:

```lumen
cell b() -> String / {http}
  return a()
end
```

## Effect Errors

### UndeclaredEffect

```lumen
cell process() -> String    # Missing / {llm}
  return Chat(prompt: "Hello")
end

# Error: Undeclared effect {llm}
# Caused by: call to Chat at line 2
```

### Effect Mismatch

```lumen
cell safe() -> String       # Declares no effects
  return risky()            # But calls {http} function
end

# Error: Effect mismatch - expected no effects, found {http}
```

## Common Effects

| Effect | Description | Tools |
|--------|-------------|-------|
| `http` | HTTP requests | `http.get`, `http.post` |
| `llm` | LLM API calls | `llm.chat`, `llm.embed` |
| `db` | Database access | `postgres.query` |
| `fs` | Filesystem access | `fs.read`, `fs.write` |
| `trace` | Trace events | `emit` |
| `mcp` | MCP server calls | MCP tools |
| `emit` | Event emission | `emit()` |

## Custom Effects

Declare custom effects:

```lumen
effect analytics
  cell track(event: String) -> Null
  cell identify(user: String) -> Null
end
```

Then use in functions:

```lumen
cell track_purchase(item: Item) -> Null / {analytics}
  analytics.track("purchase")
end
```

## Effect Handlers

Implement effects:

```lumen
handler MockAnalytics
  handle analytics.track(event: String) -> Null
    print("Tracked: {event}")
    return null
  end
  
  handle analytics.identify(user: String) -> Null
    print("Identified: {user}")
    return null
  end
end
```

## Benefits of Effects

1. **Explicit side effects** — See what a function does
2. **Compile-time checking** — Catch missing declarations
3. **Documentation** — Types serve as documentation
4. **Refactoring safety** — Know what changing code affects
5. **Testing** — Mock effects easily

## Effects vs Exceptions

| Effects | Exceptions |
|---------|------------|
| Declared in types | Implicit |
| Checked at compile time | Runtime |
| Part of function signature | Hidden |
| Composable | Non-local |

## Best Practices

1. **Declare all effects** — Let the compiler help you
2. **Use strict mode** — Catch undeclared effects early
3. **Bind effects to tools** — Clear provenance
4. **Keep effects minimal** — Don't declare what you don't use
5. **Document custom effects** — Explain what they mean

## Next Steps

- [Determinism](../../reference/directives/deterministic) — Controlling nondeterminism
- [Tool Reference](../../reference/tools) — Tool system details
