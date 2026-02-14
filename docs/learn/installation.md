# Installation

## Requirements

- **Rust** 1.70+ (for building from source)
- **Cargo** (comes with Rust)

## Install from Crates.io

```bash
cargo install lumen-lang
```

## Build from Source

```bash
git clone https://github.com/alliecatowo/lumen.git
cd lumen
cargo build --release
```

The binary will be at `target/release/lumen`. Add it to your PATH:

```bash
export PATH="$PATH:$(pwd)/target/release"
```

## Verify Installation

```bash
lumen --version
```

## Editor Support

### VS Code

Install the Lumen extension from the marketplace:

```bash
code --install-extension lumen.lumen-vscode
```

Features:
- Syntax highlighting
- Basic autocompletion
- Error diagnostics

### Other Editors

Lumen has a Tree-sitter grammar at `tree-sitter-lumen/` that can be used with:
- Neovim (via nvim-treesitter)
- Helix
- Emacs (via tree-sitter)

## WASM Support

For browser/edge deployment:

```bash
# Install wasm-pack
cargo install wasm-pack

# Build for browser
cd rust/lumen-wasm
wasm-pack build --target web
```

See the [WASM Guide](/learn/advanced/wasm) for details.

## Next Steps

- [Quick Start](/learn/getting-started)
- [Your First Program](/learn/first-program)
