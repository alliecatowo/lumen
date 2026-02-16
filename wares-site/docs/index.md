---
layout: home

hero:
  name: Lumen
  text: The AI-Native Programming Language
  tagline: Build deterministic agent workflows with static types, first-class AI primitives, and markdown-native source files.
  actions:
    - theme: brand
      text: Effects Tutorial
      link: /guide/effects
    - theme: alt
      text: Builtins Reference
      link: /reference/builtins

features:
  - icon: "📝"
    title: Markdown-Native
    details: Write code and documentation together in .lm.md files, or use raw .lm files when you want source-only modules.
  - icon: "🔒"
    title: Statically Typed
    details: Catch errors at compile time with generics, union types, optional sugar (T?), and exhaustive match checking.
  - icon: "🤖"
    title: AI-Native Constructs
    details: Tools, grants, agents, pipelines, state machines, and orchestration are first-class language features with effect tracking.
  - icon: "⚡"
    title: Algebraic Effects
    details: Structured side effects with perform/handle — testable, composable, and explicit in cell signatures.
---

## Quick Start

```bash
# Install via Cargo
cargo install lumen-lang

# Run a program
lumen run hello.lm.md
```

## Documentation

- [Algebraic Effects Tutorial](/guide/effects) — Learn the effects system
- [Architecture Overview](/guide/architecture) — Compiler, VM, and runtime
- [Editor Setup](/guide/editor-setup) — VS Code extension and LSP
- [Builtins Reference](/reference/builtins) — All built-in functions
