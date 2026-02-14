# WASM Hello World

This example demonstrates running Lumen code in WebAssembly environments.

## Simple Computation

```lumen
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
```

Expected output: `120`

## String Operations

```lumen
cell greet(name: String) -> String
    "Hello, " ++ name ++ "!"
end

cell demo() -> String
    greet("WASM")
end
```

Expected output: `"Hello, WASM!"`

## Record Types

```lumen
record Point
    x: Int
    y: Int
end

cell distance(p: Point) -> Int
    let dx = p.x * p.x
    let dy = p.y * p.y
    dx + dy
end

cell test_point() -> Int
    let p = Point(x: 3, y: 4)
    distance(p)
end
```

Expected output: `25`

## Usage Notes

### Browser

Load the WASM module and call functions:

```javascript
import init, { run, compile, check, version } from './pkg/lumen_wasm.js';

await init();

// Check source
const checkResult = check(sourceCode);
console.log(checkResult.to_json());

// Compile to LIR
const lirResult = compile(sourceCode);
console.log(lirResult.to_json());

// Run a cell
const runResult = run(sourceCode, "main");
console.log(runResult.to_json());

// Get version
console.log("Lumen version:", version());
```

### Node.js (WASI)

```javascript
const { run, check, compile } = require('./pkg/lumen_wasm.js');

const source = `
cell main() -> Int
    42
end
`;

const result = run(source, "main");
console.log(result.to_json());
```

### Command Line (Wasmtime)

```bash
# Build with WASI target
cd rust/lumen-wasm
cargo build --target wasm32-wasi --release

# Run with Wasmtime
wasmtime target/wasm32-wasi/release/lumen_wasm.wasm
```

## Building

### Browser Target

```bash
cd rust/lumen-wasm
wasm-pack build --target web --release
```

Output in `pkg/`:
- `lumen_wasm.js` - JavaScript glue code
- `lumen_wasm_bg.wasm` - WASM binary
- `lumen_wasm.d.ts` - TypeScript definitions

### Node.js Target

```bash
wasm-pack build --target nodejs --release
```

### WASI Target

```bash
cargo build --target wasm32-wasi --release
```

## Size Optimization

The release profile in `Cargo.toml` is optimized for small binary size:

- `opt-level = "z"` - Optimize for size
- `lto = true` - Link-time optimization
- `codegen-units = 1` - Single codegen unit
- `panic = "abort"` - Smaller panic handler

Further optimize with `wasm-opt`:

```bash
wasm-opt -Oz -o optimized.wasm pkg/lumen_wasm_bg.wasm
```

## Limitations

Current limitations in WASM builds:

1. **No filesystem access** in browser (use WASI for filesystem)
2. **No tool providers** yet (coming in Phase 3)
3. **No trace recording** (requires file I/O)
4. **No imports** (multi-file compilation not yet wired)

Future enhancements:

- Tool provider bridge for `use tool` declarations
- WASI filesystem and HTTP support
- IndexedDB persistence in browser
- Source maps for debugging
