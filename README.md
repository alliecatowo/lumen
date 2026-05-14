<p align="center">
  <img src="./docs/public/logo.svg" alt="Lumen Logo" width="200" />
</p>

<h1 align="center">LUMEN</h1>

<p align="center">
  <strong>The AI-Native Programming Language.</strong><br/>
  Static types. First-class AI primitives. Markdown-native source.<br/>
  Built for deterministic, auditable agent workflows.
</p>

<p align="center">
  <a href="https://alliecatowo.github.io/lumen/"><img src="https://img.shields.io/badge/📚_Docs-alliecatowo.github.io%2Flumen-4f46e5?style=for-the-badge" alt="Docs" /></a>
  <a href="https://alliecatowo.github.io/lumen/playground"><img src="https://img.shields.io/badge/🎮_Playground-Try_Online-06b6d4?style=for-the-badge" alt="Playground" /></a>
</p>

<p align="center">
  <img src="https://img.shields.io/github/actions/workflow/status/alliecatowo/lumen/ci.yml?branch=main&label=CI&style=flat-square" alt="CI" />
  <img src="https://img.shields.io/github/actions/workflow/status/alliecatowo/lumen/pages.yml?branch=main&label=Docs&style=flat-square" alt="Docs" />
  <a href="https://open-vsx.org/extension/alliecatowo/lumen-lang"><img src="https://img.shields.io/open-vsx/v/alliecatowo/lumen-lang?style=flat-square&label=Open%20VSX" alt="Open VSX" /></a>
  <img src="https://img.shields.io/crates/v/lumen-lang?style=flat-square" alt="Crates.io" />
  <img src="https://img.shields.io/github/license/alliecatowo/lumen?style=flat-square" alt="License" />
  <img src="https://img.shields.io/github/stars/alliecatowo/lumen?style=flat-square" alt="Stars" />
  <img src="https://img.shields.io/badge/tests-5300%2B_passing-22c55e?style=flat-square" alt="Tests" />
</p>

---

> **Lumen** is a statically typed language purpose-built for AI-native systems — where your tools, agents, state machines, and data pipelines are *language constructs*, not library abstractions.

```lumen
# Write code and docs together. Run anywhere.

use tool llm.chat as Chat

grant Chat
  model  "gpt-4o"
  max_tokens 1024

cell summarize(text: String) -> String / {llm}
  role system: You are a concise technical writer.
  role user:   Summarize the following — {text}
  return Chat(prompt: text)
end
```

---

## ✨ What makes Lumen different

| Capability | Lumen | Typical stack |
|---|---|---|
| **AI tools** | `use tool llm.chat as Chat` — typed, grant-scoped, policy-enforced | Framework wrappers, env-var soup |
| **Safety limits** | `grant Chat max_tokens 1024 domain "api.openai.com"` — compile-time constraints | Manual validation scattered everywhere |
| **Agents & processes** | First-class `agent`, `pipeline`, `machine`, `memory` constructs | Class hierarchies or third-party orchestrators |
| **Algebraic effects** | `perform`/`handle`/`resume` — effects explicit in every type signature | Implicit exceptions, monads, or "magic" middleware |
| **Determinism** | `@deterministic true` — non-deterministic ops rejected at compile time | Hope and `random.seed(42)` |
| **Source format** | `.lm.md` and `.lumen` — code and docs live together, natively | Separate notebooks, Markdown, and source files |
| **Package security** | TUF metadata + Ed25519 signing + OIDC auth + Merkle transparency log | `pip install` 🙏 |

---

## 🚀 Quick Start

```bash
# Install
curl -fsSL https://raw.githubusercontent.com/alliecatowo/lumen/main/scripts/install.sh | sh

# …or via Cargo
cargo install lumen-lang
```

```bash
# Create and run your first program
cat > hello.lm.md << 'EOF'
cell main() -> String
  return "Hello, Lumen!"
end
EOF

lumen run hello.lm.md
# → Hello, Lumen!
```

```bash
lumen check hello.lm.md   # type-check only
lumen fmt   hello.lm.md   # auto-format
lumen repl                 # interactive REPL
```

---

## 🗺 Language Tour

### Markdown-Native Source

Write prose and code side-by-side in `.lm.md` or `.lumen`. Documentation is a first-class concern — LSP hover pulls docstrings directly from the surrounding markdown block.

````markdown
# User Authentication

Handles login and session management.

```lumen
record User
  id:    String
  name:  String
  email: String where email.contains("@")
end

cell authenticate(email: String, password: String) -> result[User, String]
  # ...
end
```
````

---

### Static Types + Rich Constraints

```lumen
record Product
  name:  String where length(name) > 0
  price: Float  where price >= 0.0
  sku:   String where length(sku) == 8
end

cell divide(a: Int, b: Int) -> result[Int, String]
  if b == 0
    return err("division by zero")
  end
  return ok(a / b)
end
```

---

### Pattern Matching & Exhaustiveness

```lumen
enum Shape
  Circle(radius: Float)
  Rect(w: Float, h: Float)
  Triangle(base: Float, height: Float)
end

cell area(s: Shape) -> Float
  match s
    Circle(r)      -> return 3.14159 * r * r
    Rect(w, h)     -> return w * h
    Triangle(b, h) -> return 0.5 * b * h
  end
  # Compiler enforces all variants are covered — missing one is a compile error.
end
```

---

### Algebraic Effects

Effects are declared in type signatures and handled via one-shot delimited continuations — no implicit exceptions, no hidden control flow.

```lumen
effect Log
  cell info(msg: String) -> Unit
  cell warn(msg: String) -> Unit
end

effect Db
  cell query(sql: String) -> list[Json]
end

cell fetch_users() -> list[String] / {Log, Db}
  perform Log.info("Fetching users")
  let rows = perform Db.query("SELECT name FROM users")
  return map(rows, fn(r) -> r["name"] as String end)
end

# Wire up handlers at the boundary:
handle fetch_users() with
  Log.info(msg) -> resume(unit)
    print("[INFO] {msg}")
  end
  Db.query(sql) -> resume(rows)
    rows = real_db_execute(sql)
  end
end
```

---

### AI-Native: Tools, Grants, Agents

```lumen
use tool llm.chat   as Chat
use tool http.get   as Fetch
use tool fs.write   as WriteFile

grant Chat
  model        "gpt-4o"
  max_tokens   2048
  temperature  0.3

grant Fetch
  allowed_domains ["api.github.com"]
  timeout_ms      8000

agent CodeReviewer
  cell review(diff: String) -> String / {llm, http}
    role system: You are a senior engineer. Review diffs for bugs and style.
    role user:   {diff}
    return Chat(prompt: diff)
  end
end
```

---

### Deterministic Execution

Mark a cell `@deterministic true` and the compiler enforces it — no `uuid()`, no `timestamp()`, no unresolved external calls. Perfect for reproducible test suites and auditable AI outputs.

```lumen
@deterministic true

cell process(input: String) -> String
  # uuid()      ← compile error: non-deterministic
  # timestamp() ← compile error: non-deterministic
  return transform(input)
end
```

---

### Process Primitives

Pipelines, state machines, and memory stores are *language-level* constructs:

```lumen
# Data pipeline — stages auto-chain, run cell auto-generated
pipeline ETL
  stages:
    -> extract
    -> transform
    -> load

  cell extract(source: String) -> list[Json]   … end
  cell transform(data: list[Json]) -> list[Map] … end
  cell load(records: list[Map]) -> Int          … end
end

# State machine
machine TrafficLight
  state Red    -> transition(go:   Green)
  state Green  -> transition(slow: Yellow)
  state Yellow -> transition(stop: Red)
end

# Persistent key-value memory
memory SessionStore
  entry: Map
end
```

---

### Concise Syntax

```lumen
# Pipe operator  |>  — eager left-to-right value threading
let result = raw_text |> clean() |> tokenize() |> embed()

# Compose operator  ~>  — lazy function composition
let pipeline = parse ~> validate ~> normalize

# String interpolation
let msg = "Hello, {user.name}! You have {count} messages."

# Range expressions
for i in 1..=10
  print("{i}")
end

# Optional sugar  T?  =  T | Null
cell find(id: String) -> User?
  # ...
end

# When expression
let grade = when score
  >= 90 -> "A"
  >= 80 -> "B"
  >= 70 -> "C"
  _     -> "F"
end
```

---

## 📦 Wares — the Lumen Package Manager

Wares brings supply-chain security that most ecosystems only dream about:

```bash
lumen pkg init my-agent        # scaffold a new package
lumen pkg add   @acme/llm-kit  # resolve + lock dependency
lumen pkg build                # compile with all imports resolved
lumen pkg publish              # sign with Ed25519, submit to registry
```

- 🔐 **Ed25519 package signing** — every publish is cryptographically signed
- 🛡 **TUF metadata verification** — threshold signing, rollback detection, expiration enforcement
- 🪪 **OIDC authentication** — token-based registry login
- 🌳 **Merkle transparency log** — tamper-evident record of every published version
- 🔒 **Content-addressed lockfile** (`lumen.lock`) — reproducible installs across machines

---

## 🌐 WebAssembly

Compile your Lumen programs to WASM for zero-latency browser inference or WASI edge functions:

```bash
lumen build wasm --target web     # ES modules for the browser
lumen build wasm --target nodejs  # CommonJS for Node.js
lumen build wasm --target wasi    # WASI for Wasmtime / edge runtimes
```

The `lumen-wasm` crate exposes `check()`, `compile()`, `run()`, and `version()` via `wasm-bindgen`. See [`examples/wasm_browser.html`](examples/wasm_browser.html) for an interactive demo.

---

## 🛠 Tooling

| Tool | Status |
|------|--------|
| **VS Code extension** | ✅ TextMate + Tree-sitter grammars, format-on-save |
| **LSP** | ✅ Hover, completion, go-to-def, diagnostics, semantic tokens, folding |
| **Formatter** (`lumen fmt`) | ✅ Markdown-aware, docstring-preserving, `--check` for CI |
| **REPL** (`lumen repl`) | ✅ Multi-line, history, immediate execution |
| **Trace recorder** | ✅ `--trace-dir` for full execution traces |
| **Tree-sitter grammar** | ✅ `tree-sitter-lumen/grammar.js` |

---

## 🏗 Architecture

```
 .lm.md / .lm / .lumen source
         │
         ▼
 ┌───────────────────────────────────┐
 │   Markdown extraction             │  (pulls fenced blocks + @directives)
 └──────────────┬────────────────────┘
                ▼
 ┌───────────────────────────────────┐
 │   Lexer → Parser → AST            │
 └──────────────┬────────────────────┘
                ▼
 ┌───────────────────────────────────┐
 │   Resolver  (effects, imports)    │
 │   Typechecker                     │
 │   Constraint validator            │
 └──────────────┬────────────────────┘
                ▼
 ┌───────────────────────────────────┐
 │   LIR bytecode  (32-bit fixed)    │  ~100 opcodes
 └──────────────┬────────────────────┘
                ▼
 ┌─────────────────────────────────────────────────────┐
 │   Register VM                                       │
 │  ┌──────────┐ ┌──────────┐ ┌────────┐ ┌─────────┐  │
 │  │  Values  │ │ Futures  │ │ Tools  │ │ Traces  │  │
 │  └──────────┘ └──────────┘ └────────┘ └─────────┘  │
 │  ┌───────────────────────────────────────────────┐  │
 │  │  Effects stack  (perform / handle / resume)   │  │
 │  └───────────────────────────────────────────────┘  │
 └─────────────────────────────────────────────────────┘
```

---

## 📚 Documentation

| Resource | |
|----------|-|
| [Getting Started](https://alliecatowo.github.io/lumen/learn/getting-started) | Install + first program |
| [Tutorial](https://alliecatowo.github.io/lumen/learn/tutorial/basics) | Step-by-step language guide |
| [AI-Native Features](https://alliecatowo.github.io/lumen/learn/ai-native/tools) | Tools, grants, agents, processes |
| [Language Reference](https://alliecatowo.github.io/lumen/reference/overview) | Complete specification |
| [API Reference](https://alliecatowo.github.io/lumen/api/builtins) | Standard library (83 builtins) |
| [Playground](https://alliecatowo.github.io/lumen/playground) | Try Lumen in your browser |

---

## 🧪 Examples (30 total)

| Example | Highlights |
|---------|------------|
| [`hello.lm.md`](examples/hello.lm.md) | Hello World |
| [`ai_chat.lm.md`](examples/ai_chat.lm.md) | Gemini-powered chatbot |
| [`invoice_agent.lm.md`](examples/invoice_agent.lm.md) | AI invoice auditing, tool grants, schema validation |
| [`code_reviewer.lm.md`](examples/code_reviewer.lm.md) | LLM code analysis agent |
| [`state_machine.lm.md`](examples/state_machine.lm.md) | Typed state machine process |
| [`data_pipeline.lm.md`](examples/data_pipeline.lm.md) | ETL pipeline process |
| [`syntax_sugar.lm.md`](examples/syntax_sugar.lm.md) | Pipes, ranges, interpolation, `when` |
| [`linked_list.lm.md`](examples/linked_list.lm.md) | Generic data structures |
| [`fibonacci.lm.md`](examples/fibonacci.lm.md) | Recursive algorithms |
| [`task_tracker.lm.md`](examples/task_tracker.lm.md) | Memory process + structured records |

→ [Browse all 30 examples](https://github.com/alliecatowo/lumen/tree/main/examples)

---

## 🔧 Development

```bash
git clone https://github.com/alliecatowo/lumen.git
cd lumen

cargo build --release                         # build everything
cargo test --workspace                        # 5,300+ tests
cargo run --bin lumen -- run examples/hello.lm.md
```

### Repository layout

```
lumen/
├── rust/
│   ├── lumen-compiler/     compiler pipeline (lexer → parser → resolver → typechecker → LIR)
│   ├── lumen-vm/           register VM, continuations, processes
│   ├── lumen-runtime/      tool dispatch, caching, tracing, crypto, retry
│   ├── lumen-cli/          CLI, REPL, formatter, package manager, WASM builder
│   ├── lumen-lsp/          Language Server Protocol, semantic search
│   ├── lumen-wasm/         WebAssembly bindings (wasm-bindgen)
│   └── lumen-provider-*/   HTTP / JSON / FS / MCP providers
├── examples/               30 annotated example programs
├── docs/                   VitePress documentation site
├── editors/vscode/         VS Code extension + TextMate grammar
├── tree-sitter-lumen/      Tree-sitter grammar
└── SPEC.md                 Implementation-accurate language specification
```

---

## 🤝 Contributing

Contributions are very welcome!

- 📖 [Contributing Guide](https://github.com/alliecatowo/lumen/blob/main/CONTRIBUTING.md)
- 🌐 [Code of Conduct](https://github.com/alliecatowo/lumen/blob/main/CODE_OF_CONDUCT.md)
- 🐣 [Good First Issues](https://github.com/alliecatowo/lumen/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)
- 💬 [Discussions](https://github.com/alliecatowo/lumen/discussions)

---

## License

MIT — see [LICENSE](https://github.com/alliecatowo/lumen/blob/main/LICENSE).

---

<p align="center">
  Made with ❤️ by the Lumen community
</p>
