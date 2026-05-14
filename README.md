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
  <a href="https://alliecatowo.github.io/lumen/"><strong>📚 Docs</strong></a> ·
  <a href="https://alliecatowo.github.io/lumen/playground"><strong>🎮 Playground</strong></a> ·
  <a href="https://alliecatowo.github.io/lumen/learn/getting-started"><strong>🚀 Get Started</strong></a> ·
  <a href="https://github.com/alliecatowo/lumen/issues"><strong>🐛 Issues</strong></a> ·
  <a href="https://github.com/alliecatowo/lumen/discussions"><strong>💬 Discussions</strong></a>
</p>

<p align="center">
  <!-- Build & quality -->
  <a href="https://github.com/alliecatowo/lumen/actions/workflows/ci.yml">
    <img src="https://img.shields.io/github/actions/workflow/status/alliecatowo/lumen/ci.yml?branch=main&label=CI&style=flat-square&logo=github-actions&logoColor=white&color=22c55e" alt="CI" />
  </a>
  <a href="https://alliecatowo.github.io/lumen/">
    <img src="https://img.shields.io/github/actions/workflow/status/alliecatowo/lumen/pages.yml?branch=main&label=Docs&style=flat-square&logo=readthedocs&logoColor=white&color=3b82f6" alt="Docs" />
  </a>
  <!-- Language & registry -->
  <img src="https://img.shields.io/badge/built%20with-Rust-f97316?style=flat-square&logo=rust&logoColor=white" alt="Built with Rust" />
  <a href="https://crates.io/crates/lumen-lang">
    <img src="https://img.shields.io/crates/v/lumen-lang?style=flat-square&logo=rust&logoColor=white&color=f97316" alt="Crates.io" />
  </a>
  <!-- Editor -->
  <a href="https://open-vsx.org/extension/alliecatowo/lumen-lang">
    <img src="https://img.shields.io/open-vsx/v/alliecatowo/lumen-lang?style=flat-square&label=Open%20VSX&logo=vscodium&logoColor=white&color=a855f7" alt="Open VSX" />
  </a>
  <!-- Community -->
  <a href="https://github.com/alliecatowo/lumen/blob/main/LICENSE">
    <img src="https://img.shields.io/github/license/alliecatowo/lumen?style=flat-square&color=64748b" alt="License: MIT" />
  </a>
  <a href="https://github.com/alliecatowo/lumen/stargazers">
    <img src="https://img.shields.io/github/stars/alliecatowo/lumen?style=flat-square&logo=github&color=eab308" alt="Stars" />
  </a>
  <!-- Tests -->
  <img src="https://img.shields.io/badge/tests-5%2C300%2B%20passing-22c55e?style=flat-square" alt="5,300+ tests passing" />
</p>

---

## What is Lumen?

Lumen is a **statically-typed, AI-native programming language** that treats tools, agents, pipelines, and effects as first-class language constructs — not bolted-on libraries. Source files can be plain `.lm`, raw Lumen `.lumen`, or **literate markdown** `.lm.md` where prose and code live together.

It compiles to LIR bytecode that runs on a fast register-based VM, and it targets **WebAssembly** for browser and edge deployment.

```lumen
use tool llm.chat as Chat

grant Chat
  model    "gpt-4o"
  max_tokens 512
  timeout_ms 8000

record Message
  role: String
  content: String
end

cell summarise(docs: list[String]) -> String / {llm}
  role system: You are a concise technical writer.
  role user: Summarise these {length(docs)} documents: {docs |> join(", ")}
  return Chat(prompt: "summarise", temperature: 0.3)
end
```

---

## Why Lumen?

Building AI systems today means juggling Python notebooks, API clients, prompt templates, and fragile orchestration frameworks. **Lumen unifies all of it:**

| Capability | Lumen | Traditional stack |
|---|---|---|
| **Typed tool calls** | `use tool llm.chat as Chat` — interfaces enforced at compile time | Untyped dicts, runtime errors |
| **Safety grants** | `grant Chat max_tokens 512 timeout_ms 5000` — hard limits baked in | Manual validation, easily forgotten |
| **Effect tracking** | `cell fetch() -> String / {http}` — side effects in the signature | Implicit, invisible side effects |
| **Determinism mode** | `@deterministic true` — rejects `uuid()`, `timestamp()` at compile time | Roll your own reproducibility |
| **Processes** | Pipelines, state machines, memory stores built into the language | Third-party libs with bespoke APIs |
| **Markdown source** | Code and docs live in the same `.lm.md` file | Separate READMEs that fall out of sync |
| **WASM** | `lumen build wasm --target web` — run in the browser or on the edge | Python can't; JS has no type safety |

---

## Quick Start

### Install

```bash
# Linux / macOS (one-liner)
curl -fsSL https://raw.githubusercontent.com/alliecatowo/lumen/main/scripts/install.sh | sh

# Via Cargo
cargo install lumen-lang

# Verify
lumen --version
```

### Hello, World

```bash
# Create a program (plain .lm is fine; .lm.md lets you mix prose)
cat > hello.lm << 'EOF'
cell main() -> String
  return "Hello, World!"
end
EOF

lumen run hello.lm
# => Hello, World!
```

### Hello, AI

```bash
cat > hello_ai.lm << 'EOF'
use tool gemini.generate as Generate

grant Generate max_tokens 100

cell main() -> String
  return Generate(
    prompt: "Say hello to a Lumen developer in one sentence.",
    temperature: 0.8
  )
end
EOF

lumen run hello_ai.lm
```

---

## Key Features

### 📝 Markdown-Native Source

Write code and documentation together. The compiler extracts fenced ` ```lumen ` blocks from `.lm.md` files:

````markdown
# User Service

Handles authentication and profile management.

```lumen
record User
  id:    String
  name:  String
  email: String where email.contains("@")
end

cell greet(user: User) -> String
  return "Hello, {user.name}!"
end
```
````

---

### 🔒 Static Types + Constraint Validation

Catch errors at compile time. Field `where` clauses are checked at construction:

```lumen
record Product
  name:  String where length(name) > 0
  price: Float  where price >= 0.0
  sku:   String where sku.starts_with("SKU-")
end

cell safe_divide(a: Int, b: Int) -> result[Int, String]
  if b == 0
    return err("Division by zero")
  end
  return ok(a / b)
end
```

---

### ⚡ Algebraic Effects

Side effects are **declared in type signatures** and handled with one-shot delimited continuations:

```lumen
effect Log
  cell info(msg: String) -> Unit
  cell warn(msg: String) -> Unit
end

cell process(items: list[String]) -> Int / {Log}
  for item in items
    perform Log.info("Processing: {item}")
  end
  return length(items)
end

# Provide a handler at the call site
handle process(["a", "b", "c"]) with
  Log.info(msg) -> resume(unit)
    print("[INFO] {msg}")
  end
  Log.warn(msg) -> resume(unit)
    print("[WARN] {msg}")
  end
end
```

---

### 🤖 AI-Native Constructs

Tools, grants, and agents are **first-class language features**, not library imports:

```lumen
use tool llm.chat   as Chat
use tool http.get   as Fetch

grant Chat
  model   "gpt-4o"
  max_tokens 2048
  temperature 0.2

grant Fetch
  allowed_domains ["api.example.com"]
  timeout_ms      5000

record AuditResult
  status:      String
  issue_count: Int
  summary:     String
end

agent Auditor
  cell audit(vendor: String, amount: Float) -> AuditResult / {llm, http}
    role system: You are a financial auditor. Be concise.
    role user: Review a {amount} invoice from {vendor}. Flag anything unusual.
    let summary = Chat(prompt: "audit")
    return AuditResult(status: "REVIEWED", issue_count: 0, summary: summary)
  end
end
```

---

### 🔄 Processes: Pipelines, Machines & Memory

**Pipelines** auto-chain stages and validate the data flow at compile time:

```lumen
pipeline ETL
  stages: -> extract -> transform -> load

  cell extract(source: String)            -> list[Json]   ... end
  cell transform(rows: list[Json])        -> list[Record] ... end
  cell load(records: list[Record])        -> Int          ... end
end
```

**State machines** with typed payloads and guarded transitions:

```lumen
machine OrderFlow
  state Pending
  state Processing(order_id: String)
  state Shipped(tracking: String)
  state Delivered

  transition Pending    -> Processing(order_id: String)
  transition Processing -> Shipped(tracking: String)
  transition Shipped    -> Delivered
end
```

---

### 🎯 Deterministic Runtime

`@deterministic true` makes AI workflows reproducible and auditable:

```lumen
@deterministic true

cell score_resume(text: String) -> Float / {llm}
  # uuid()      ← compile error: nondeterministic
  # timestamp() ← compile error: nondeterministic
  role system: Score this resume from 0.0 to 1.0. Reply with a single float.
  role user: {text}
  return to_float(Chat(prompt: "score"))
end
```

---

### 🌐 WebAssembly

Compile to WASM for browser, Node.js, or WASI targets:

```bash
lumen build wasm --target web    # ES modules for browsers
lumen build wasm --target nodejs # CommonJS for Node.js
```

The full VM — compiler, runtime, and your program — runs in the browser with zero back-end.

---

## Language Tour

<details>
<summary><strong>Cells (functions)</strong></summary>

```lumen
# Basic cell
cell greet(name: String) -> String
  return "Hello, {name}!"
end

# With effects declared
cell fetch_user(id: String) -> result[User, String] / {http}
  let data = HttpGet(url: "https://api.example.com/users/{id}")
  match parse_json(data)
    ok(u)    -> return ok(u)
    err(msg) -> return err(msg)
  end
end

# Pipe operator  |>  threads a value through calls
cell slugify(title: String) -> String
  return title |> to_lower() |> replace(" ", "-") |> trim()
end
```

</details>

<details>
<summary><strong>Types & pattern matching</strong></summary>

```lumen
enum Shape
  Circle(radius: Float)
  Rect(w: Float, h: Float)
  Triangle(base: Float, height: Float)
end

cell area(s: Shape) -> Float
  match s
    Circle  -> PI * s.radius ** 2.0
    Rect    -> s.w * s.h
    Triangle -> 0.5 * s.base * s.height
  end
end
```

</details>

<details>
<summary><strong>Async / futures</strong></summary>

```lumen
use tool llm.chat as Chat

cell analyse_all(docs: list[String]) -> list[String] / {llm}
  # Fan out — all calls run concurrently
  let futures = []
  for doc in docs
    futures = append(futures, async Chat(prompt: "Summarise: {doc}"))
  end
  return parallel(futures)  # Wait for all
end
```

</details>

<details>
<summary><strong>Imports & modules</strong></summary>

```lumen
# Import specific symbols
import utils.math: clamp, lerp

# Import everything from a module
import models: *

# Import with alias
import services.auth: authenticate as auth
```

</details>

---

## Ecosystem & Integrations

| Integration | Description |
|---|---|
| **VS Code / Open VSX** | Syntax highlighting, LSP diagnostics, hover docs, go-to-definition |
| **Tree-sitter grammar** | Used by Neovim, Helix, and other editors for precise highlighting |
| **MCP (Model Context Protocol)** | `lumen-provider-mcp` bridges any MCP server as a typed Lumen tool |
| **HTTP provider** | `use tool http.get` — typed HTTP calls with grant-level domain allow-lists |
| **Filesystem provider** | `use tool fs.read` — sandboxed file I/O |
| **Wares package manager** | `lumen pkg` — sigstore-style keyless signing, TUF metadata, SAT resolver |

---

## Documentation

| Resource | Link |
|---|---|
| Getting Started | [alliecatowo.github.io/lumen/learn/getting-started](https://alliecatowo.github.io/lumen/learn/getting-started) |
| Language Tour | [alliecatowo.github.io/lumen/learn/tour](https://alliecatowo.github.io/lumen/learn/tour) |
| AI-Native Guide | [alliecatowo.github.io/lumen/learn/ai-native/tools](https://alliecatowo.github.io/lumen/learn/ai-native/tools) |
| Language Reference | [alliecatowo.github.io/lumen/reference/overview](https://alliecatowo.github.io/lumen/reference/overview) |
| Builtins API | [alliecatowo.github.io/lumen/api/builtins](https://alliecatowo.github.io/lumen/api/builtins) |
| Interactive Playground | [alliecatowo.github.io/lumen/playground](https://alliecatowo.github.io/lumen/playground) |
| Full Spec (SPEC.md) | [SPEC.md](./SPEC.md) |
| Formal Grammar | [docs/GRAMMAR.md](./docs/GRAMMAR.md) |

---

## Examples

| Example | What it shows |
|---|---|
| [hello.lm.md](examples/hello.lm.md) | The simplest possible Lumen program |
| [hello_ai.lm.md](examples/hello_ai.lm.md) | First LLM tool call with `gemini.generate` |
| [invoice_agent.lm.md](examples/invoice_agent.lm.md) | Records, grants, agents, pattern matching |
| [state_machine.lm.md](examples/state_machine.lm.md) | Enums and exhaustive matching |
| [data_pipeline.lm.md](examples/data_pipeline.lm.md) | List processing and records |
| [code_reviewer.lm.md](examples/code_reviewer.lm.md) | AI-powered code analysis |
| [syntax_sugar.lm.md](examples/syntax_sugar.lm.md) | Pipes, ranges, interpolation, compose |
| [fibonacci.lm.md](examples/fibonacci.lm.md) | Recursion and pattern matching |
| [mcp_demo.lm.md](examples/mcp_demo.lm.md) | MCP server integration |
| [wasm_hello.lm.md](examples/wasm_hello.lm.md) | Running Lumen in the browser |

→ Browse all **30 examples** in [`examples/`](./examples)

---

## Architecture

```
.lm.md / .lm / .lumen  (source files)
        │
        ▼
┌─────────────────────────────────────────┐
│  Markdown extraction  (.lm.md/.lumen)   │
│  Direct tokenise      (.lm)             │
└───────────────────┬─────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────┐
│  Lexer → Parser → Resolver              │
│  → Typechecker → Constraint validator   │
│  → LIR lowering                         │
└───────────────────┬─────────────────────┘
                    │  LIR bytecode
                    ▼
┌─────────────────────────────────────────┐
│            Register VM                  │
│  ┌────────┐ ┌────────┐ ┌─────────────┐ │
│  │ Values │ │Futures │ │Tool dispatch│ │
│  └────────┘ └────────┘ └─────────────┘ │
│  ┌────────┐ ┌────────┐ ┌─────────────┐ │
│  │Effects │ │Traces  │ │  Processes  │ │
│  └────────┘ └────────┘ └─────────────┘ │
└─────────────────────────────────────────┘
        │
        ├── Native binary  (Linux / macOS / Windows)
        └── WebAssembly    (web / Node.js / WASI)
```

---

## Development

```bash
# Clone
git clone https://github.com/alliecatowo/lumen.git
cd lumen

# Build everything
cargo build --release

# Run the full test suite (5,300+ tests)
cargo test --workspace

# Run a single example
cargo run --bin lumen -- run examples/hello.lm.md

# Type-check without running
lumen check examples/invoice_agent.lm.md
```

### Repository layout

```
lumen/
├── rust/
│   ├── lumen-compiler/     # Markdown extraction → lexer → parser → resolver
│   │                       # → typechecker → constraint validator → LIR lowering
│   ├── lumen-vm/           # Register VM: dispatch loop, intrinsics, processes, futures
│   ├── lumen-runtime/      # Tool dispatch, caching, traces, retry, crypto
│   ├── lumen-cli/          # `lumen` binary: run, check, fmt, repl, pkg, build wasm
│   ├── lumen-lsp/          # Full LSP: hover, diagnostics, go-to-def, semantic tokens
│   ├── lumen-wasm/         # wasm-bindgen bindings (built with wasm-pack)
│   └── lumen-provider-*/   # Tool providers: http, json, fs, mcp
├── examples/               # 30 example programs (.lm.md)
├── docs/                   # VitePress documentation site
├── editors/vscode/         # VS Code extension
├── tree-sitter-lumen/      # Tree-sitter grammar (Neovim, Helix, …)
├── SPEC.md                 # Implementation-accurate language specification
└── docs/GRAMMAR.md         # Formal EBNF grammar
```

---

## Contributing

We warmly welcome contributions of all sizes!

- 📖 [Contributing Guide](./CONTRIBUTING.md)
- 📜 [Code of Conduct](./CODE_OF_CONDUCT.md)
- 🏷️ [Good First Issues](https://github.com/alliecatowo/lumen/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)
- 🗺️ [Roadmap](./ROADMAP.md)

---

## License

MIT — see [LICENSE](./LICENSE) for details.

---

<p align="center">
  Made with ❤️ by the Lumen community
</p>
