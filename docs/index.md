---
layout: home

hero:
  name: Lumen
  text: The AI-Native Programming Language
  tagline: Build deterministic agent workflows with static types, first-class AI primitives, and markdown-native source files.
  image:
    src: /logo.svg
    alt: Lumen
  actions:
    - theme: brand
      text: Get Started
      link: /learn/getting-started
    - theme: alt
      text: Language Tour
      link: /learn/tour
    - theme: alt
      text: Try Playground
      link: ./playground

features:
  - icon: "📝"
    title: Markdown-Native
    details: Write code and documentation together in .lm.md files, or use raw .lm files when you want source-only modules. Both are first-class inputs.
  - icon: "🔒"
    title: Statically Typed
    details: Catch errors at compile time with generics, union types, optional sugar (T?), constraint validation, and exhaustive match checking.
  - icon: "🤖"
    title: AI-Native Constructs
    details: Tools, grants, agents, pipelines, state machines, and orchestration are first-class language features with effect tracking.
  - icon: "⚡"
    title: Deterministic Runtime
    details: Explicit effects, controlled nondeterminism, and reproducible execution for auditable AI workflows. 1180+ tests passing.
  - icon: "🌐"
    title: WASM Ready
    details: Compile to WebAssembly for browser execution or edge deployment with zero external dependencies.
  - icon: "🛠️"
    title: Rich Tooling
    details: Built-in CLI, REPL, formatter, LSP, trace recording, multi-file imports, and a VS Code extension.
---

## Quick Start

```bash
# Install (One-liner)
curl -fsSL https://raw.githubusercontent.com/alliecatowo/lumen/main/scripts/install.sh | sh

# Or via Cargo
cargo install lumen-lang
```

Set up your editor with the [VS Code extension](./guide/editors).

```bash
# Create your first program
cat > hello.lm.md << 'EOF'
cell main() -> String
  return "Hello, World!"
end
EOF

# Run it
lumen run hello.lm.md
```

## Why Lumen?

Building AI systems today means juggling Python notebooks, API clients, prompt templates, and orchestration frameworks. Lumen unifies this into one language:

- **Tools** are typed interfaces with policy constraints
- **Grants** enforce safety limits (tokens, timeouts, domains)
- **Agents** encapsulate behavior with scoped capabilities
- **Processes** provide structured workflows (pipelines, state machines, memory)
- **Effects** make side effects explicit and auditable
- **Pipes** chain transformations with `|>` for readable data flow

## Language Highlights

```lumen
# Optional types with T? sugar
cell find_user(id: Int) -> User?
  # ...
end

# Labeled loops with filters
for @outer item in items if item.active
  for sub in item.children
    if sub.done
      break @outer
    end
  end
end

# Floor division, shifts, and bitwise ops
let page = offset // page_size
let flags = 1 << 3 | 1 << 5

# Destructuring let
let (x, y) = get_coordinates()

# Defer for cleanup
defer
  close(handle)
end

# Type tests and casts
if value is String
  let s = value as String
end
```

## Try It Now

Head to the [Playground](./playground) to run Lumen code directly in your browser -- no installation required.

## Example: AI Chat Agent

```lumen
use tool llm.chat as Chat

grant Chat
  model "gpt-4o"
  max_tokens 1024
  temperature 0.7

bind effect llm to Chat

agent Assistant
  cell respond(message: String) -> String / {llm}
    role system: You are a helpful assistant.
    role user: {message}
    return Chat(prompt: message)
  end
end

cell main() -> String / {llm}
  let bot = Assistant()
  return bot.respond("What is pattern matching?")
end
```

## Community

<img src="https://img.shields.io/github/actions/workflow/status/alliecatowo/lumen/pages.yml?branch=main&label=Docs&style=flat-square" alt="Docs Status" />
<a href="https://open-vsx.org/extension/lumen-lang/lumen-lang"><img src="https://img.shields.io/open-vsx/v/lumen-lang/lumen-lang?style=flat-square&label=Open%20VSX" alt="Open VSX" /></a>
<img src="https://img.shields.io/crates/v/lumen-lang/lumen-lang?style=flat-square" alt="Crates.io" />
