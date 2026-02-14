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

Install the Lumen extension from [Open VSX](https://open-vsx.org/extension/lumen-lang/lumen-lang):

```bash
# Via command line if you use code-server or compatible editors
code --install-extension lumen-lang.lumen-lang
```

Or search for "Lumen" in the Extensions view (ensure you are using a registry that includes Open VSX if not using official VS Code).

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

See the [Wasm Guide](../guide/wasm-browser) for details.

## Next Steps

- [Quick Start](./getting-started)
- [Your First Program](./first-program)
