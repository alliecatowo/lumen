# Quickstart

This is the shortest path from clone to running your first Lumen program.

## 1) Build Lumen

```bash
git clone https://github.com/alliecatowo/lumen.git
cd lumen
cargo build --release
```

Binary location:

- `target/release/lumen`

## 2) Write a Program

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

Lumen supports both source formats:

- `.lm` for raw source
- `.lm.md` for markdown + fenced `lumen` code

## 3) Type-check and Run

```bash
lumen check hello.lm.md
lumen run hello.lm.md
```

## 4) Useful Next Commands

```bash
lumen emit hello.lm.md --output out.json
lumen trace show <run_id>
lumen cache clear
```

## 5) Explore Examples

Start with:

- `examples/hello.lm.md`
- `examples/language_features.lm.md`
- `examples/invoice_agent.lm.md`
- `examples/data_pipeline.lm.md`
