# Editor Support

Lumen provides first-class support for VS Code through a dedicated extension that includes syntax highlighting, diagnostics, and deep AI-native features.

## VS Code Extension

The official Lumen extension is available on the **Open VSX Registry**.

### Installation

1. Open VS Code.
2. Go to the **Extensions** view (`Ctrl+Shift+X`).
3. Search for **Lumen**.
4. Click **Install**.

Alternatively, install it via the command line:

```bash
code --install-extension lumen-lang.lumen-lang
```

### Features

- **Syntax Highlighting**: Beautiful theme-aware highlighting for `.lm.md` files.
- **Language Server**: Powered by `lumen-lsp` (bundled with the extension).
- **Diagnostics**: Real-time error reporting and type-checking.
- **AI Tool Integration**: Hover over tool calls to see documentation and live status.

## Other Editors

Lumen uses the **Language Server Protocol (LSP)**. You can use any LSP-compatible editor (Vim/Neovim, Emacs, Sublime Text) by installing the `lumen-lsp` binary.

### Installing the Language Server

If you used the [One-liner Installation](./cli#one-liner-recommended), you already have `lumen-lsp` in your path.

Otherwise, install it via Cargo:

```bash
cargo install lumen-lsp
```

Configure your editor to use `lumen-lsp` for `.lm.md` files.
