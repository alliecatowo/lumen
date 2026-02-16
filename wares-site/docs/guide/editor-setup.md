# Editor Setup

Lumen provides first-class support for VS Code through a dedicated extension.

## VS Code Extension

The official Lumen extension is available on the **Open VSX Registry**.

### Installation

1. Open VS Code.
2. Go to the **Extensions** view (`Ctrl+Shift+X`).
3. Search for **Lumen**.
4. Click **Install**.

Alternatively, install via the command line:

```bash
code --install-extension lumen-lang.lumen-lang
```

### Features

- **Syntax highlighting** — Theme-aware highlighting for `.lm`, `.lm.md`, and `.lumen` files
- **Language Server (LSP)** — Powered by `lumen-lsp`, bundled with the extension
- **Hover** — Rich docstrings with type signatures
- **Completions** — Keywords, builtins, types, and symbols
- **Go-to-definition** — Jump to declarations
- **Diagnostics** — Real-time type-checking and error reporting

## File Support

The extension recognizes:

- `.lm` — Raw Lumen source
- `.lm.md` — Markdown with fenced Lumen code blocks
- `.lumen` — Markdown-native format

## Other Editors

Lumen uses the **Language Server Protocol (LSP)**. You can use any LSP-compatible editor (Vim/Neovim, Emacs, Sublime Text) by installing the `lumen-lsp` binary:

```bash
cargo install lumen-lsp
```

Configure your editor to use `lumen-lsp` for Lumen source files.
