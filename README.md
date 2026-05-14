<p align="center">
  <img src="./docs/public/logo.svg" alt="Lumen Logo" width="180" />
</p>

<h1 align="center">Lumen</h1>

<p align="center">
  <strong>The AI-Native Programming Language</strong>
</p>

<p align="center">
  <em>Statically typed. Effect-aware. Markdown-native.<br>Built for the age of AI — where code, docs, and intelligence live together.</em>
</p>

<p align="center">
  <a href="https://alliecatowo.github.io/lumen/"><strong>📚 Docs</strong></a> &nbsp;·&nbsp;
  <a href="https://alliecatowo.github.io/lumen/playground"><strong>🎮 Playground</strong></a> &nbsp;·&nbsp;
  <a href="https://github.com/alliecatowo/lumen/issues"><strong>🐛 Issues</strong></a> &nbsp;·&nbsp;
  <a href="https://github.com/alliecatowo/lumen/discussions"><strong>💬 Discuss</strong></a>
</p>

<p align="center">
  <img src="https://img.shields.io/github/actions/workflow/status/alliecatowo/lumen/ci.yml?branch=main&label=CI&style=flat-square" alt="CI Status" />
  <img src="https://img.shields.io/github/actions/workflow/status/alliecatowo/lumen/pages.yml?branch=main&label=Docs&style=flat-square" alt="Docs Status" />
  <a href="https://open-vsx.org/extension/alliecatowo/lumen-lang"><img src="https://img.shields.io/open-vsx/v/alliecatowo/lumen-lang?style=flat-square&label=Open%20VSX" alt="Open VSX" /></a>
  <img src="https://img.shields.io/crates/v/lumen-lang?style=flat-square" alt="Crates.io" />
  <img src="https://img.shields.io/github/license/alliecatowo/lumen?style=flat-square" alt="License" />
  <img src="https://img.shields.io/github/stars/alliecatowo/lumen?style=flat-square" alt="Stars" />
</p>

---

## Why Lumen?

Building AI systems today means juggling Python notebooks, API clients, prompt templates, and orchestration frameworks. **Lumen unifies this into one language** — with the type system as the single source of truth for *what code is allowed to do*.

| Capability | Lumen | Traditional Stack |
|-----------|-------|-------------------|
| **Effects** | Algebraic effects declared in the type signature — tracked, composable, and handleable | Try/catch or monads, implicit and untracked |
| **Tools** | Typed interfaces with compile-time policy constraints | Framework wrappers, runtime surprises |
| **Grants** | Built-in safety limits (tokens, timeouts, domains) enforced by the VM | Manual validation, easy to skip |
| **Processes** | Pipelines, state machines, memory stores as first-class constructs | Bolted-on libraries |
| **Source format** | Markdown-native `.lm.md` / `.lumen` — code and docs are one file | Separate code files and documentation forever out of sync |
| **Determinism** | `@deterministic true` rejects non-deterministic ops at compile time | Hope and convention |

---

## ⚡ The Killer Feature: Algebraic Effects

This is what gets me most excited about Lumen.

In most languages, *side effects are invisible* — a function can log, call an API, or throw without the caller knowing. Lumen fixes that at the type level. Every cell declares exactly what effects it can perform, right in its signature:

```lumen
cell fetch_user(id: String) -> result[User, String] / {Http, Log}
```

The `/ {Http, Log}` part isn't a comment — it's enforced by the compiler. If you call something that uses `Http` without declaring it, the build fails.

Better still: effects are *handleable*. You can intercept, mock, or redirect them at any call site using **one-shot delimited continuations**:

```lumen
# Declare the effects
effect Http
  cell get(url: String) -> String
end

effect Log
  cell info(msg: String) -> Unit
end

# A cell that uses both — declared in its signature
cell load_profile(user_id: String) -> String / {Http, Log}
  perform Log.info("Fetching profile for {user_id}")
  let raw = perform Http.get("https://api.example.com/users/{user_id}")
  return "Profile: {raw}"
end

# In production: real HTTP + structured logging
cell main() -> String
  handle
    handle load_profile("42")
    with Http.get(url) ->
      let res = http_client_get(url)
      resume(res)
    end
  with Log.info(msg) ->
    print("[INFO] {msg}")
    resume(unit)
  end
end

# In tests: swap both out without touching load_profile at all
cell test_load_profile() -> Bool
  let result = handle
    handle load_profile("42")
    with Http.get(_url) ->
      resume("{\"name\": \"Ada\"}")
    end
  with Log.info(_msg) ->
    resume(unit)  # silently swallow logs
  end
  return result == "Profile: {\"name\": \"Ada\"}"
end
```

No dependency injection framework. No mock libraries. No monkey-patching. **The effect system *is* the seam.**

---

## Quick Start

```bash
# Install
curl -fsSL https://raw.githubusercontent.com/alliecatowo/lumen/main/scripts/install.sh | sh

# Or via Cargo
cargo install lumen-lang

# Write your first program
cat > hello.lm.md << 'EOF'
cell main() -> String
  return "Hello, World!"
end
EOF

# Run it
lumen run hello.lm.md
```

---

## Feature Highlights

### 📝 Markdown-Native Source

Lumen source files *are* documents. Write prose and code together — no separate README needed:

````markdown
# User Authentication

This module handles login and session management.

```lumen
record User
  id: String
  name: String
  email: String where email.contains("@")
end

cell authenticate(email: String, password: String) -> result[User, String]
  # Implementation here
end
```
````

### 🔒 Statically Typed with Constraint Validation

Types aren't just shapes — they carry invariants:

```lumen
record Product
  name: String where length(name) > 0
  price: Float where price >= 0.0
end

cell safe_divide(a: Int, b: Int) -> result[Int, String]
  if b == 0
    return err("Division by zero")
  end
  return ok(a / b)
end
```

### 🤖 AI-Native Constructs

Tools, grants, and agents are built into the language — not bolted on:

```lumen
use tool llm.chat as Chat

grant Chat
  model "gpt-4o"
  max_tokens 1024
  temperature 0.7

agent Assistant
  cell respond(message: String) -> String / {llm}
    role system: You are a helpful assistant.
    role user: {message}
    return Chat(prompt: message)
  end
end
```

### ⏱ Deterministic Runtime

Reproduce any AI run exactly — perfect for audits, debugging, and compliance:

```lumen
@deterministic true

cell main() -> String
  # uuid()      # Compile error: nondeterministic
  # timestamp() # Compile error: nondeterministic
  return "Always the same output"
end
```

### 🔄 First-Class Pipelines & State Machines

```lumen
pipeline DataProcessor
  stages:
    -> extract
    -> transform
    -> load

  cell extract(source: String) -> list[Json]
    # Pull raw data
  end

  cell transform(data: list[Json]) -> list[Record]
    # Shape it
  end

  cell load(records: list[Record]) -> Int
    # Persist it; return row count
  end
end
```

### 🌐 WASM Ready

```bash
lumen build wasm --target web     # Browser (ES modules)
lumen build wasm --target nodejs  # Node.js
```

---

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│           .lm.md / .lm / .lumen  Source Files                    │
└──────────────────────────┬───────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│   Markdown Extraction (.lm.md/.lumen) │ Direct Parse (.lm)       │
└──────────────────────────┬───────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│   Lexer → Parser → Resolver → Typechecker → Constraint Val       │
│                        (effect rows tracked at every stage)      │
└──────────────────────────┬───────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│                      LIR Bytecode                                │
└──────────────────────────┬───────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│                      Register VM                                 │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌─────────────────────┐ │
│  │  Values  │ │ Futures  │ │  Tools   │ │ Effect Handler Stack│ │
│  └──────────┘ └──────────┘ └──────────┘ └─────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

---

## Language Tour

### Cells (Functions)

```lumen
cell greet(name: String) -> String
  return "Hello, {name}!"
end
```

### Pattern Matching (Exhaustiveness Checked)

```lumen
cell classify(n: Int) -> String
  match n
    0 -> return "zero"
    1 -> return "one"
    _ -> return "many"
  end
end
```

### Error Handling with `result[T, E]`

```lumen
cell safe_divide(a: Int, b: Int) -> String
  match divide(a, b)
    ok(value) -> return "Result: {value}"
    err(msg)  -> return "Error: {msg}"
  end
end
```

### Pipes and Composition

```lumen
# |>  pipes a VALUE through functions (eager)
let result = raw_data |> parse() |> validate() |> format()

# ~>  COMPOSES functions into a new function (lazy)
let pipeline = parse ~> validate ~> format
let result   = pipeline(raw_data)
```

---

## Documentation

| Resource | Description |
|----------|-------------|
| [Getting Started](https://alliecatowo.github.io/lumen/learn/getting-started) | Installation and first program |
| [Tutorial](https://alliecatowo.github.io/lumen/learn/tutorial/basics) | Step-by-step language guide |
| [AI-Native Features](https://alliecatowo.github.io/lumen/learn/ai-native/tools) | Tools, grants, agents, processes |
| [Language Reference](https://alliecatowo.github.io/lumen/reference/overview) | Complete specification |
| [API Reference](https://alliecatowo.github.io/lumen/api/builtins) | Standard library |
| [Playground](https://alliecatowo.github.io/lumen/playground) | Try Lumen in your browser |

## Examples

| Example | Description |
|---------|-------------|
| [Hello World](examples/hello.lm.md) | Basic program |
| [AI Chat](examples/ai_chat.lm.md) | LLM-powered chatbot |
| [State Machine](examples/state_machine.lm.md) | Machine process |
| [Data Pipeline](examples/data_pipeline.lm.md) | Pipeline process |
| [Code Reviewer](examples/code_reviewer.lm.md) | AI code analysis |
| [Syntax Sugar](examples/syntax_sugar.lm.md) | Pipes, ranges, interpolation |
| [Fibonacci](examples/fibonacci.lm.md) | Recursive algorithms |
| [Linked List](examples/linked_list.lm.md) | Generic data structures |

See all [30 examples](https://github.com/alliecatowo/lumen/tree/main/examples) in the examples directory.

---

## Development

```bash
# Clone
git clone https://github.com/alliecatowo/lumen.git
cd lumen

# Build
cargo build --release

# Test (5,300+ passing)
cargo test --workspace

# Run
cargo run --bin lumen -- run examples/hello.lm.md
```

## Repository Structure

```
lumen/
├── docs/                    # VitePress documentation site
│   ├── learn/              # Tutorials and guides
│   ├── reference/          # Language specification
│   ├── api/                # Standard library docs
│   └── examples/           # Example documentation
├── examples/               # 30 example programs
├── editors/                # Editor support (VS Code)
├── rust/
│   ├── lumen-compiler/     # Full compiler pipeline
│   ├── lumen-vm/           # Register-based VM
│   ├── lumen-runtime/      # Tool dispatch, caching, tracing
│   ├── lumen-cli/          # CLI (check, run, fmt, repl, pkg, …)
│   ├── lumen-lsp/          # Language Server Protocol
│   ├── lumen-wasm/         # WebAssembly bindings
│   └── lumen-provider-*/   # Tool providers (HTTP, JSON, FS, MCP)
├── SPEC.md                 # Implementation-accurate language spec
└── CLAUDE.md               # AI assistant instructions
```

---

## Contributing

We welcome contributions!

- [Contributing Guide](https://github.com/alliecatowo/lumen/blob/main/CONTRIBUTING.md)
- [Code of Conduct](https://github.com/alliecatowo/lumen/blob/main/CODE_OF_CONDUCT.md)
- [Good First Issues](https://github.com/alliecatowo/lumen/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)

## License

MIT — see [LICENSE](https://github.com/alliecatowo/lumen/blob/main/LICENSE) for details.

---

<p align="center">
  Made with ❤️ by the Lumen community
</p>
