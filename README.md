<p align="center">
  <img src="./docs/public/logo.svg" alt="Lumen Logo" width="180" />
</p>

<h1 align="center">Lumen</h1>

<p align="center">
  <strong>The AI-Native Programming Language</strong>
</p>

<p align="center">
  <em>Build deterministic agent workflows with static types, first-class AI primitives, and markdown-native source files.</em>
</p>

<p align="center">
  <a href="https://alliecatowo.github.io/lumen/"><strong>📚 Documentation</strong></a> ·
  <a href="https://alliecatowo.github.io/lumen/playground"><strong>🎮 Playground</strong></a> ·
  <a href="https://github.com/alliecatowo/lumen/issues"><strong>🐛 Issues</strong></a>
</p>

<p align="center">
  <img src="https://img.shields.io/github/actions/workflow/status/alliecatowo/lumen/ci.yml?branch=main&label=CI&style=flat-square" alt="CI Status" />
  <img src="https://img.shields.io/github/actions/workflow/status/alliecatowo/lumen/pages.yml?branch=main&label=Docs&style=flat-square" alt="Docs Status" />
  <a href="https://open-vsx.org/extension/lumen-lang/lumen-lang"><img src="https://img.shields.io/open-vsx/v/lumen-lang/lumen-lang?style=flat-square&label=Open%20VSX" alt="Open VSX" /></a>
  <a href="https://crates.io/crates/lumen-cli"><img src="https://img.shields.io/crates/v/lumen-cli?style=flat-square" alt="Crates.io" /></a>
  <img src="https://img.shields.io/github/license/alliecatowo/lumen?style=flat-square" alt="License" />
  <img src="https://img.shields.io/github/stars/alliecatowo/lumen?style=flat-square" alt="Stars" />
</p>

---

## Why Lumen?

Building AI systems today means juggling Python notebooks, API clients, prompt templates, and orchestration frameworks. **Lumen unifies this into one language:**

| Feature | Lumen | Traditional Stack |
|---------|-------|-------------------|
| **Tools** | Typed interfaces with policy constraints | Framework wrappers |
| **Grants** | Built-in safety limits (tokens, timeouts, domains) | Manual validation |
| **Agents** | First-class language construct | Class hierarchies |
| **Processes** | Pipelines, state machines, memory built-in | External libraries |
| **Effects** | Algebraic effects with handlers, explicit in type signatures | Try/catch or monads, implicit |
| **Source** | Markdown-native (`.lm.md`, `.lumen`) + raw (`.lm`) | Separate code and docs |

## Quick Start

```bash
# Install (One-liner)
curl -fsSL https://raw.githubusercontent.com/alliecatowo/lumen/main/scripts/install.sh | sh

# Or via Cargo
cargo install lumen-cli

# Create your first program
cat > hello.lm.md << 'EOF'
cell main() -> String
  return "Hello, World!"
end
EOF

# Run it
lumen run hello.lm.md
```

## Features

- **Algebraic Effects**: First-class effect handling with `perform` and `handle` constructs
- **Markdown-Native Source**: Write code and docs together in `.lm.md` or `.lumen` files
- **Static Typing**: Full type inference with compile-time error checking
- **AI Tool Dispatch**: Typed tool interfaces with policy constraints
- **Register-Based VM**: Efficient bytecode execution
- **Full LSP Support**: Hover, document symbols, signature help, semantic tokens, diagnostics

### 📝 Markdown-Native Source

Write code and documentation together in `.lm.md` or `.lumen`, or use `.lm` for source-only modules:

````markdown
# User Authentication

This module handles user login and session management.

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

### 🔒 Statically Typed

Catch errors at compile time:

```lumen
cell divide(a: Int, b: Int) -> result[Int, String]
  if b == 0
    return err("Division by zero")
  end
  return ok(a / b)
end
```

### 🎯 Algebraic Effects

First-class effect handling with one-shot delimited continuations:

```lumen
effect Log
  cell info(msg: String) -> Unit
end

cell main() -> String / {Log}
  perform Log.info("Starting")
  return "Done"
end

handle main() with Log.info(msg) -> resume(unit)
  print("LOG: {msg}")
end
```

### 🤖 AI-Native Constructs

Tools, grants, and agents are built-in:

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

### ⚡ Deterministic Runtime

Reproducible execution for auditable AI:

```lumen
@deterministic true

cell main() -> String
  # Nondeterministic operations rejected at compile time
  # uuid()      # Error!
  # timestamp() # Error!
  return "Deterministic output"
end
```

### 🌐 WASM Ready

Compile to WebAssembly for browser execution:

```bash
lumen build wasm --target web
```

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

## Language Tour

### Cells (Functions)

```lumen
cell greet(name: String) -> String
  return "Hello, {name}!"
end
```

### Records with Constraints

```lumen
record Product
  name: String where length(name) > 0
  price: Float where price >= 0.0
end
```

### Pattern Matching

```lumen
cell classify(n: Int) -> String
  match n
    0 -> return "zero"
    1 -> return "one"
    _ -> return "many"
  end
end
```

### Error Handling

```lumen
cell safe_divide(a: Int, b: Int) -> String
  match divide(a, b)
    ok(value) -> return "Result: {value}"
    err(msg) -> return "Error: {msg}"
  end
end
```

### Processes

```lumen
pipeline DataProcessor
  stages:
    -> extract
    -> transform
    -> load
  
  cell extract(source: String) -> list[Json]
    # Extract data
  end
  
  cell transform(data: list[Json]) -> list[Record]
    # Transform data
  end
  
  cell load(records: list[Record]) -> Int
    # Load data
  end
end
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│            .lm.md / .lm / .lumen Source Files                 │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│  Markdown Extraction (.lm.md/.lumen) / Direct Parse (.lm)    │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│  Lexer → Parser → Resolver → Typechecker → Constraint Val   │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                    LIR Bytecode                              │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                    Register VM                               │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐       │
│  │  Values  │ │ Futures  │ │  Tools   │ │ Traces   │       │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘       │
└─────────────────────────────────────────────────────────────┘
```

## Development

```bash
# Clone
git clone https://github.com/alliecatowo/lumen.git
cd lumen

# Build
cargo build --release

# Test (1365+ tests)
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
├── examples/               # Example programs
├── editors/               # Editor support (VS Code)
├── rust/
│   ├── lumen-compiler/    # Compiler pipeline
│   ├── lumen-vm/          # Register-based virtual machine
│   ├── lumen-runtime/     # Runtime: tool dispatch, caching, tracing
│   ├── lumen-cli/         # Command-line interface
│   ├── lumen-lsp/         # Language Server Protocol
│   ├── lumen-wasm/        # WebAssembly bindings
│   └── lumen-provider-*/  # Tool providers (HTTP, JSON, FS, MCP)
├── SPEC.md                # Implementation-accurate spec
└── CLAUDE.md              # AI assistant instructions
```

## Contributing

We welcome contributions! Please see:

- [Contributing Guide](https://github.com/alliecatowo/lumen/blob/main/CONTRIBUTING.md)
- [Code of Conduct](https://github.com/alliecatowo/lumen/blob/main/CODE_OF_CONDUCT.md)
- [Good First Issues](https://github.com/alliecatowo/lumen/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)

## License

MIT License - see [LICENSE](https://github.com/alliecatowo/lumen/blob/main/LICENSE) for details.

---

<p align="center">
  Made with ❤️ by the Lumen community
</p>
