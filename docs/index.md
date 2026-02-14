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
      text: Language Reference
      link: /reference/overview
    - theme: alt
      text: Try Playground
      link: /playground

features:
  - icon: "📝"
    title: Markdown-Native
    details: Write code and documentation together in .lm.md files. Lumen extracts and runs fenced code blocks while preserving your notes.
  - icon: "🔒"
    title: Statically Typed
    details: Catch errors at compile time with a powerful type system including generics, union types, and constraint validation.
  - icon: "🤖"
    title: AI-Native Constructs
    details: Tools, grants, agents, pipelines, and state machines are first-class language features—not framework add-ons.
  - icon: "⚡"
    title: Deterministic Runtime
    details: Explicit effects, controlled nondeterminism, and reproducible execution for auditable AI workflows.
  - icon: "🌐"
    title: WASM Ready
    details: Compile to WebAssembly for browser execution or edge deployment with zero external dependencies.
  - icon: "🛠️"
    title: Rich Tooling
    details: Built-in CLI, REPL, formatter, LSP, and trace recording for debugging complex agent systems.
---

## Quick Start

```bash
# Install
cargo install lumen-lang

# Create your first program
echo '# My First Program
```lumen
cell main() -> String
  return "Hello, World!"
end
```' > hello.lm.md

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

## Choose Your Path

<div class="path-cards">

### New to Lumen?
[Start with the Tutorial](/learn/getting-started) — Learn the basics step by step.

### Coming from Python/JS?
[See What's Different](/learn/introduction#comparison) — Quick orientation for experienced developers.

### Building AI Agents?
[AI-Native Features Guide](/learn/ai-native/tools) — Deep dive into tools, grants, and orchestration.

### Need Reference?
[Language Reference](/reference/overview) — Complete specification of syntax and semantics.

</div>

## Example: AI Chat Agent

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

cell main() -> String / {llm}
  let bot = Assistant()
  return bot.respond("What is Lumen?")
end
```

## Community

- [GitHub](https://github.com/alliecatowo/lumen) — Source code and issues
- [Examples](/examples/hello-world) — Working code samples
- [FAQ](/learn/faq) — Common questions answered

<style>
.path-cards {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
  gap: 1rem;
  margin: 1.5rem 0;
}
.path-cards > div {
  padding: 1rem;
  border: 1px solid var(--vp-c-divider);
  border-radius: 8px;
}
.path-cards h3 {
  margin-top: 0;
}
.path-cards p {
  margin-bottom: 0;
  color: var(--vp-c-text-2);
}
</style>
