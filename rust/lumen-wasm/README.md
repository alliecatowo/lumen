# lumen-wasm

WebAssembly bindings for the Lumen compiler and VM.

This crate provides a WASM-compatible interface for compiling and executing Lumen programs in browser and WASI environments.

## Prerequisites

Install wasm-pack:

```bash
cargo install wasm-pack
```

## Building

### Browser Target

```bash
wasm-pack build --target web
```

Or use the CLI:

```bash
lumen build wasm --target web
```

Output in `pkg/`:
- `lumen_wasm.js` - JavaScript glue code
- `lumen_wasm_bg.wasm` - WASM binary
- `lumen_wasm.d.ts` - TypeScript definitions

### Node.js Target

```bash
wasm-pack build --target nodejs
```

Or:

```bash
lumen build wasm --target nodejs
```

### WASI Target

```bash
cargo build --target wasm32-wasi
```

(Requires `wasm32-wasi` target installed: `rustup target add wasm32-wasi`)

## Usage

### Browser

```html
<script type="module">
import init, { run, compile, check, version } from './pkg/lumen_wasm.js';

await init();

const source = `
cell main() -> Int
    42
end
`;

// Type-check
const checkResult = check(source);
console.log(checkResult.to_json());

// Compile to LIR
const lirResult = compile(source);
console.log(lirResult.to_json());

// Execute
const runResult = run(source, "main");
console.log(runResult.to_json());

// Get version
console.log("Lumen version:", version());
</script>
```

### Node.js

```javascript
const { run, check, compile, version } = require('./pkg/lumen_wasm.js');

const source = `
cell factorial(n: Int) -> Int
    if n <= 1
        1
    else
        n * factorial(n - 1)
    end
end

cell main() -> Int
    factorial(5)
end
`;

const result = run(source, "main");
const parsed = JSON.parse(result.to_json());

if (parsed.ok) {
    console.log("Result:", parsed.ok);
} else {
    console.error("Error:", parsed.error);
}
```

### Wasmtime (WASI)

```bash
cargo build --target wasm32-wasi --release
wasmtime target/wasm32-wasi/release/lumen_wasm.wasm
```

## API

### `check(source: &str) -> LumenResult`

Type-check Lumen source code.

Returns:
- Success: `{"ok": "Type-checked successfully"}`
- Error: `{"error": "error message with diagnostics"}`

### `compile(source: &str) -> LumenResult`

Compile Lumen source to LIR JSON.

Returns:
- Success: `{"ok": "<LIR JSON>"}`
- Error: `{"error": "error message with diagnostics"}`

### `run(source: &str, cell_name: Option<String>) -> LumenResult`

Compile and execute Lumen source.

Parameters:
- `source` - Lumen source code
- `cell_name` - Cell to execute (default: "main")

Returns:
- Success: `{"ok": "<output>"}`
- Error: `{"error": "error message"}`

### `version() -> String`

Get the Lumen compiler version.

### `LumenResult`

Result wrapper with helper methods:

- `is_ok() -> bool` - Returns true if successful
- `is_err() -> bool` - Returns true if failed
- `to_json() -> String` - Get result as JSON string

## Examples

See `examples/wasm_hello.lm.md` for Lumen code examples.

See `examples/wasm_browser.html` for a complete browser demo.

## Current Limitations

- No filesystem access in browser (WASI supports filesystem)
- No tool providers yet (coming in Phase 3)
- No trace recording (requires file I/O)
- No multi-file imports yet

## Size Optimization

The release profile is optimized for small binary size:

```toml
[profile.release]
opt-level = "z"       # Optimize for size
lto = true            # Link-time optimization
codegen-units = 1     # Single codegen unit
panic = "abort"       # Smaller panic handler
```

Further optimize with `wasm-opt`:

```bash
wasm-opt -Oz -o optimized.wasm pkg/lumen_wasm_bg.wasm
```

## Development

Run tests:

```bash
cargo test
```

Run WASM tests:

```bash
wasm-pack test --headless --firefox
```

## See Also

- `docs/WASM_STRATEGY.md` - Overall WASM compilation strategy
- `examples/wasm_hello.lm.md` - Example Lumen programs for WASM
- `examples/wasm_browser.html` - Interactive browser demo
