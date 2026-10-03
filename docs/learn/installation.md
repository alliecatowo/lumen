# Installation

## Requirements

- **Rust** 1.70+ (for building from source)
- **Cargo** (comes with Rust)

## Homebrew (macOS and Linux)

```bash
brew install alliecatowo/tap/lumen
```

## Release binary

```bash
curl -fsSL https://raw.githubusercontent.com/alliecatowo/lumen/main/scripts/install.sh | sh
```

Downloads the latest release for your platform and installs `lumen` and `lumen-lsp` to
`/usr/local/bin`, or `~/.lumen/bin` when that is not writable. Archives for Linux (x64, x64-musl,
arm64), macOS (x64, arm64) and Windows (x64) are on the
[releases page](https://github.com/alliecatowo/lumen/releases).

## Install from Crates.io

```bash
cargo install lumen-cli
cargo install lumen-lsp   # optional language server
```

The crate is `lumen-cli`; it installs the `lumen` binary. (The unrelated `lumen-lang` crate on
crates.io belongs to a different project.) To track the source tree instead:

```bash
cargo install --git https://github.com/alliecatowo/lumen lumen-cli
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

Install the Lumen extension from [Open VSX](https://open-vsx.org/extension/alliecatowo/lumen):

```bash
# Via command line if you use code-server or compatible editors
code --install-extension alliecatowo.lumen
```

Or search for "Lumen" in the Extensions view (ensure you are using a registry that includes Open VSX if not using official VS Code).

Features:
- Syntax highlighting
- Basic autocompletion
- Error diagnostics

### Other Editors

Lumen has a Tree-sitter grammar (`npm install tree-sitter-lumen`, source in `tree-sitter-lumen/`) that can be used with:
- Neovim (via nvim-treesitter)
- Helix
- Emacs (via tree-sitter)

## WASM Support

For browser/edge deployment:

Use the published package from npm:

```bash
npm install lumen-wasm
```

Or build it yourself:

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
