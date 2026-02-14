# Lumen

![Status](https://img.shields.io/badge/status-active-success)
![Markdown Native](https://img.shields.io/badge/authoring-markdown--native-blue)
![AI Native](https://img.shields.io/badge/focus-ai--native-purple)
![Runtime](https://img.shields.io/badge/runtime-register--vm-orange)
![Built with Agents](https://img.shields.io/badge/built%20with-agents-ff69b4)

Lumen is a **statically typed, markdown-native programming language** for building AI-native systems.

Write programs in `.lm` or `.lm.md` files, keep your code and intent together, and compile to a VM runtime that supports modern language fundamentals **plus** first-class tooling for effects, orchestration, policy-aware execution, and agent workflows.

---

## Why Lumen?

Most AI software today is stitched together from prompts, glue code, and infrastructure spread across too many layers.

Lumen gives you one language and one runtime for the whole flow:

- **Language ergonomics you expect**: types, functions, control flow, records, pattern matching.
- **Agentic features you actually need**: tool declarations, grants, role prompts, and policy controls.
- **Dual-format source files**: `.lm` (raw) and `.lm.md` (markdown with fenced `lumen` blocks) are both first-class.
- **Compiler + VM stack**: from lexer/parser/typechecker to LIR lowering and register-based execution.

If you want to build robust AI systems without duct-taping five frameworks together, Lumen is the bet.

## What makes it markdown-native?

Lumen source is commonly authored in markdown with fenced `lumen` code blocks:

````markdown
# My Agent

This cell greets the operator.

```lumen
cell main() -> Null
  let name = "Lumen"
  print("Hello from markdown, {name}!")
  return null
end
```
````

That means you can keep architecture notes, rationale, and runnable language code in one artifact. If you prefer raw source files, use `.lm` with the same CLI commands.

## Current State

- Compiler pipeline: `lexer -> parser -> resolver -> typechecker -> LIR lowering`
- Runtime: register VM with tool dispatch, structured values, futures, traces, and process runtime objects
- CLI commands: `check`, `run`, `emit`, `trace show`, `cache clear`

## Quick Start

### Prerequisites

- Rust stable
- Cargo

### Build

```bash
git clone https://github.com/lumen-lang/lumen.git
cd lumen
cargo build --release
```

### Run a program (`.lm.md` or `.lm`)

```bash
cargo run --bin lumen -- run examples/hello.lm.md
```

Use `examples/hello.lm` the same way for raw source files.

### Typecheck only

```bash
cargo run --bin lumen -- check examples/hello.lm.md
```

### Emit LIR JSON

```bash
cargo run --bin lumen -- emit examples/hello.lm.md --output out.json
```

## Try lumen-orbit

The flagship example runs from `examples/lumen-orbit/`.

```bash
cargo run --bin lumen -- run examples/lumen-orbit/src/main.lm.md
```

CI-style check for the flagship example:

```bash
cargo run --release --bin lumen -- check examples/lumen-orbit/src/main.lm.md
cargo run --release --bin lumen -- ci examples/lumen-orbit
```

## Language Tour (Inline Examples)

### 1) Cells, bindings, and interpolation

```lumen
cell greet(name: String) -> String
  let punctuation = "!"
  return "Hello, {name}{punctuation}"
end
```

### 2) Control flow with `if`, `while`, and `match`

```lumen
cell classify(n: Int) -> String
  let i = 0
  while i < n
    i += 1
  end

  let label = "other"
  match n
    0 -> label = "zero"
    1 -> label = "one"
    _ -> label = "many"
  end

  if i == n
    return label
  end
  return "unreachable"
end
```

### 3) Records with constraints

```lumen
record Invoice
  subtotal: Float where subtotal >= 0.0
  tax: Float where tax >= 0.0
  total: Float where total == subtotal + tax
end

cell make_invoice() -> Invoice
  return Invoice(subtotal: 100.0, tax: 10.0, total: 110.0)
end
```

### 4) Agentic primitives (tools, grants, and roles)

```lumen
use tool llm.chat as Chat

grant Chat
  model "gpt-4o"
  max_tokens 512
  temperature 0.2

cell summarize(text: String) -> String
  role system: You are a concise assistant.
  role user: Summarize this text: {text}
  return "summary placeholder"
end
```

For runnable examples, start here:

- `examples/hello.lm.md`
- `examples/language_features.lm.md`
- `examples/invoice_agent.lm.md`
- `examples/data_pipeline.lm.md`

## Repository Guide

- `SPEC.md`: implementation-accurate language specification
- `tasks.md`: concrete outstanding implementation tasks
- `ROADMAP.md`: long-horizon direction and platform goals
- `docs/`: architecture and developer documentation
- `rust/lumen-compiler`: compiler implementation
- `rust/lumen-vm`: VM/runtime implementation
- `rust/lumen-cli`: command-line interface

## Development

Run all tests:

```bash
cargo test --workspace
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for workflow and contribution policy.
Please also review the [Code of Conduct](CODE_OF_CONDUCT.md).
Use the issue templates for [bug reports](.github/ISSUE_TEMPLATE/bug_report.md) and [feature requests](.github/ISSUE_TEMPLATE/feature_request.md).

## License

This project is licensed under the [MIT License](LICENSE).
