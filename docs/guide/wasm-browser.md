# Browser WASM Guide

This page runs Lumen directly in your browser with `rust/lumen-wasm`.

## Interactive Playground

The runner below executes real `check`, `compile`, and `run` calls through WebAssembly.

<WasmPlayground />

## How This Works

At deploy time, GitHub Pages builds `rust/lumen-wasm` and ships these assets with the docs:

- `wasm/lumen_wasm.js`
- `wasm/lumen_wasm_bg.wasm`

The playground dynamically imports that module and calls:

- `check(source)`
- `compile(source)`
- `run(source, "main")`
- `version()`

## Run Locally

### 1) Build WASM package

```bash
cd rust/lumen-wasm
wasm-pack build --target web --release
```

### 2) Start docs dev server

```bash
cd docs
npm ci
npm run docs:dev
```

If you want a standalone HTML version, see `examples/wasm_browser.html`.

## Troubleshooting

- If the runner says WASM module failed to load, verify the module exists at `.../wasm/lumen_wasm.js`.
- If `compile` succeeds but `run` fails, check the returned runtime error in the output panel.
- For product direction and constraints, see `docs/WASM_STRATEGY.md`.
