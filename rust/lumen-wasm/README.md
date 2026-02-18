# lumen-wasm

**WebAssembly bindings for the Lumen compiler and VM.**

## Overview

`lumen-wasm` provides WASM-compatible JavaScript/TypeScript bindings for compiling and executing Lumen programs in browser and Node.js environments. It compiles the entire Lumen compiler and VM to WebAssembly using Rust's wasm-pack toolchain, enabling zero-latency AI inference and Lumen code execution directly in web browsers or server-side JavaScript runtimes.

The crate exposes four main functions (`check`, `compile`, `run`, `version`) via wasm-bindgen, allowing JavaScript applications to type-check, compile, and execute Lumen source code without requiring a native Lumen installation. This enables use cases like interactive code playgrounds, browser-based AI agents, edge function execution, and client-side model inference with Lumen's AI-native features.

The VM-to-WASM compilation strategy leverages the existing compiler and runtime infrastructure with minimal WASM-specific glue code. All core language features work in WASM except those requiring OS-level I/O (filesystem access in browsers, though WASI supports filesystem operations). See `docs/WASM_STRATEGY.md` for the complete architectural rationale and roadmap.

## Architecture

The crate is organized as a thin wasm-bindgen wrapper around the core Lumen compiler and VM:

| Module | Path | Purpose |
|--------|------|---------|
| **Main module** | `src/lib.rs` | WASM entry points (`check`, `compile`, `run`, `version`) |
| **Result wrapper** | `src/lib.rs` (LumenResult) | JavaScript-friendly result type with `.is_ok()`, `.is_err()`, `.to_json()` |

**Build targets:**
- **Web** (`--target web`) — ES modules for browsers
- **Node.js** (`--target nodejs`) — CommonJS for Node.js
- **WASI** (`--target wasm32-wasi`) — WASI for Wasmtime, Wasmer, etc.

**Key design decisions:**
- **Full VM compilation**: Compiles the entire VM to WASM instead of implementing a separate WASM-specific runtime
- **Single-file execution**: No multi-file imports yet (planned for future phase)
- **Synchronous API**: All functions are synchronous (no async/await) for simplicity
- **JSON serialization**: Results are JSON strings for easy JavaScript interop
- **Size optimization**: Profile optimized for small binary size (`opt-level = "z"`, LTO enabled)

## Key Types

### LumenResult

The main result type returned by all WASM functions:

```rust
#[wasm_bindgen]
pub struct LumenResult {
    success: bool,
    data: String,
}
```

**Methods:**
- `is_ok() -> bool` — Returns `true` if successful
- `is_err() -> bool` — Returns `true` if failed
- `to_json() -> String` — Returns JSON string: `{"ok": "..."}` or `{"error": "..."}`

## WASM API

### check(source: &str) -> LumenResult

Type-check Lumen source code without compiling or executing.

**Returns:**
- Success: `{"ok": "Type-checked successfully"}`
- Error: `{"error": "error message with diagnostics"}`

### compile(source: &str) -> LumenResult

Compile Lumen source to LIR JSON (intermediate representation).

**Returns:**
- Success: `{"ok": "<LIR JSON>"}`
- Error: `{"error": "error message with diagnostics"}`

### run(source: &str, cell_name: Option<String>) -> LumenResult

Compile and execute Lumen source code, returning the result.

**Parameters:**
- `source` — Lumen source code string
- `cell_name` — Cell to execute (default: `"main"`)

**Returns:**
- Success: `{"ok": "<output>"}`
- Error: `{"error": "error message"}`

### version() -> String

Get the Lumen compiler version string (e.g., `"0.5.0"`).

## Usage

### Prerequisites

Install wasm-pack:

```bash
cargo install wasm-pack
```

Install the `wasm32-wasi` target for WASI builds:

```bash
rustup target add wasm32-wasi
```

### Building for Browser (Web Target)

```bash
# Using wasm-pack directly
wasm-pack build --target web

# Or using Lumen CLI
lumen build wasm --target web
```

**Output in `pkg/` directory:**
- `lumen_wasm.js` — JavaScript glue code (ES modules)
- `lumen_wasm_bg.wasm` — WebAssembly binary
- `lumen_wasm.d.ts` — TypeScript type definitions

### Building for Node.js

```bash
# Using wasm-pack directly
wasm-pack build --target nodejs

# Or using Lumen CLI
lumen build wasm --target nodejs
```

**Output:** CommonJS-compatible JavaScript in `pkg/`

### Building for WASI

```bash
cargo build --target wasm32-wasi --release
```

**Output:** `target/wasm32-wasi/release/lumen_wasm.wasm`

### Browser Usage

```html
<!DOCTYPE html>
<html>
<head>
    <title>Lumen WASM Demo</title>
</head>
<body>
    <script type="module">
    import init, { run, compile, check, version } from './pkg/lumen_wasm.js';

    // Initialize the WASM module
    await init();

    const source = `
    cell factorial(n: Int) -> Int
        if n <= 1
            return 1
        end
        return n * factorial(n - 1)
    end

    cell main() -> Int
        return factorial(5)
    end
    `;

    // Type-check
    const checkResult = check(source);
    console.log("Type-check:", checkResult.to_json());

    // Compile to LIR
    const lirResult = compile(source);
    if (lirResult.is_ok()) {
        console.log("Compiled LIR:", JSON.parse(lirResult.to_json()).ok);
    }

    // Execute
    const runResult = run(source, "main");
    const parsed = JSON.parse(runResult.to_json());
    
    if (parsed.ok) {
        console.log("Execution result:", parsed.ok);  // "120"
    } else {
        console.error("Execution error:", parsed.error);
    }

    // Get version
    console.log("Lumen version:", version());
    </script>
</body>
</html>
```

### Node.js Usage

```javascript
const { run, check, compile, version } = require('./pkg/lumen_wasm.js');

const source = `
cell add(a: Int, b: Int) -> Int
    return a + b
end

cell main() -> Int
    return add(40, 2)
end
`;

// Type-check
const checkResult = check(source);
console.log("Type-check:", checkResult.to_json());

// Compile
const compileResult = compile(source);
if (compileResult.is_ok()) {
    console.log("Compiled successfully");
}

// Execute
const runResult = run(source, "main");
const parsed = JSON.parse(runResult.to_json());

if (parsed.ok) {
    console.log("Result:", parsed.ok);  // "42"
} else {
    console.error("Error:", parsed.error);
}

console.log("Version:", version());  // "0.5.0"
```

### WASI Usage with Wasmtime

```bash
# Build for WASI
cargo build --target wasm32-wasi --release

# Run with Wasmtime
wasmtime target/wasm32-wasi/release/lumen_wasm.wasm
```

WASI targets support filesystem access and other OS-level features not available in browsers.

## Testing

Run Rust tests:

```bash
cargo test -p lumen-wasm
```

Run WASM tests in headless browser:

```bash
# Firefox
wasm-pack test --headless --firefox

# Chrome
wasm-pack test --headless --chrome
```

The test suite validates all public API functions (`check`, `compile`, `run`, `version`) and error handling.

## Size Optimization

The WASM binary is optimized for small size using aggressive compiler settings:

```toml
[profile.release]
opt-level = "z"       # Optimize aggressively for size
lto = true            # Link-time optimization
codegen-units = 1     # Single codegen unit (slower build, smaller binary)
panic = "abort"       # Abort on panic (smaller handler)
```

Further optimize using `wasm-opt` from Binaryen:

```bash
# Install wasm-opt (part of Binaryen)
npm install -g wasm-opt

# Optimize for size
wasm-opt -Oz -o optimized.wasm pkg/lumen_wasm_bg.wasm
```

The `-Oz` flag applies maximum size optimizations. Typical reductions: 10-30% smaller than unoptimized builds.

## Current Limitations

- **No filesystem in browsers**: File I/O operations will fail (use WASI target for filesystem access)
- **No tool providers yet**: HTTP, JSON, FS providers not yet available in WASM (planned for Phase 3)
- **No trace recording**: Requires file I/O (trace system not yet WASM-compatible)
- **No multi-file imports**: Only single-file compilation supported (module system work in progress)
- **Synchronous API**: All operations block (async support planned)

See `docs/WASM_STRATEGY.md` for the full roadmap and planned enhancements.

## Examples

- **`examples/wasm_hello.lm.md`** — Example Lumen programs demonstrating WASM features
- **`examples/wasm_browser.html`** — Complete interactive browser demo with editor

## Related Crates

- **[lumen-compiler](../lumen-compiler/)** — Compiler pipeline (compiled to WASM)
- **[lumen-rt](../lumen-rt/)** — VM and runtime (compiled to WASM)
- **[lumen-core](../lumen-core/)** — Shared types (LIR, values, types)
- **[docs/WASM_STRATEGY.md](../../docs/WASM_STRATEGY.md)** — Overall WASM strategy and roadmap
