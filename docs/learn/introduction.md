# Introduction

Lumen is a statically typed programming language designed for building AI-native systems. It combines modern language features with first-class support for AI primitives like tools, agents, and workflows.

## What Makes Lumen Different?

### Markdown-Native Source

Lumen is markdown-native (`.lm.md`) so documentation and code stay together. Raw `.lm` files are also supported as first-class source when you prefer source-only modules.

````markdown
# User Service

This module handles user authentication and profile management.

```lumen
record User
  id: String
  name: String
  email: String where email.contains("@")
end

cell greet(user: User) -> String
  return "Hello, {user.name}!"
end
```
````

### AI Primitives as First-Class Citizens

Instead of importing AI frameworks, Lumen has built-in constructs:

- **Tools** — Typed interfaces to external services (LLMs, APIs, databases)
- **Grants** — Policy constraints on tool usage (tokens, timeouts, domains)
- **Agents** — Encapsulated behavior with scoped capabilities
- **Processes** — Structured workflows (pipelines, state machines, memory)
- **Effects** — Explicit side effects in type signatures

### Deterministic by Design

Lumen makes AI systems auditable:

- Effects are declared in function signatures
- `@deterministic` mode rejects nondeterministic operations
- Futures have explicit scheduling modes
- Tool calls are traced automatically

### Modern Type System

- **Static typing** with type inference
- **Union types** (`Int | String`)
- **Generic types** (`list[T]`, `map[K, V]`)
- **Result type** for error handling (`result[Ok, Err]`)
- **Constraint validation** (`where` clauses on record fields)

## Comparison with Other Languages

| Feature | Lumen | Python | TypeScript | Rust |
|---------|-------|--------|------------|------|
| Static Types | ✅ | ❌ | ✅ | ✅ |
| AI Primitives | ✅ Built-in | Framework | Framework | Framework |
| Effect Tracking | ✅ | ❌ | ❌ | ❌ |
| Determinism Mode | ✅ | ❌ | ❌ | ❌ |
| Markdown Source | ✅ | ❌ | ❌ | ❌ |
| WASM Target | ✅ | ❌ | ✅ | ✅ |
| Pattern Matching | ✅ | ❌ | ❌ | ✅ |

## When to Use Lumen

**Great for:**
- AI agent systems and chatbots
- Data processing pipelines with AI steps
- Auditable AI workflows
- Edge AI applications (via WASM)
- Prototyping AI features quickly

**Not ideal for:**
- Low-level systems programming (use Rust/C)
- Game development (use Unity/Unreal)
- Mobile apps (use Swift/Kotlin)

## Design Philosophy

1. **Explicit over implicit** — Side effects are visible in types
2. **Safety by default** — Strict mode catches errors early
3. **AI-first** — Language features designed for AI workflows
4. **Simple but powerful** — Easy to learn, scales to complex systems
5. **Reproducible** — Same input → same output (when deterministic)

## Next Steps

- [Install Lumen](./installation)
- [Write your first program](./first-program)
- [Learn the basics](./tutorial/basics)
