# Quickstart (CLI)

This is the shortest path from clone to running a Lumen program locally.

## Prerequisites

- Rust toolchain (`cargo`, stable)
- Git

## 1) Clone and Build

```bash
git clone https://github.com/alliecatowo/lumen.git
cd lumen
cargo build --release
```

Binary path:

- `target/release/lumen`

## 2) Create a Program

Create `hello.lm.md`:

````markdown
# Hello from Lumen

```lumen
cell main() -> Null
  let name = "Lumen"
  print("Hello, {name}!")
  return null
end
```
````

Lumen supports:

- `.lm` for source-only files
- `.lm.md` for markdown + fenced `lumen` blocks

## 3) Check, Compile, Run

```bash
target/release/lumen check hello.lm.md
target/release/lumen emit hello.lm.md --output out.json
target/release/lumen run hello.lm.md
```

## 4) Explore Working Examples

- `examples/hello.lm.md`
- `examples/language_features.lm.md`
- `examples/invoice_agent.lm.md`
- `examples/data_pipeline.lm.md`

## 5) Next Steps

- Browser execution: [Browser WASM Guide](/guide/wasm-browser)
- Language syntax: [Language Tour](/language/tour)
- CLI deep reference: [CLI](/CLI)
