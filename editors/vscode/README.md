# Lumen for Visual Studio Code

Syntax highlighting and language support for the [Lumen](https://github.com/lumen-lang/lumen) programming language.

## Features

### Syntax Highlighting
- **Rich syntax highlighting** for `.lm` files and embedded Lumen code in `.lm.md` markdown files
- **TextMate grammar** with support for all language constructs
- Highlights keywords, types, operators, strings with interpolation, numbers, comments, and directives
- Special highlighting for AI-specific constructs (roles, effects, tools, grants)
- Auto-closing pairs and bracket matching
- Folding support for `cell`/`end`, `record`/`end`, `if`/`end`, etc.

### Commands
The extension provides the following commands (accessible via Command Palette):

- **Lumen: Check File** (`Ctrl+Shift+L` / `Cmd+Shift+L`) — Type-check the current file
- **Lumen: Run File** (`Ctrl+Shift+R` / `Cmd+Shift+R`) — Compile and execute the current file
- **Lumen: Format File** (`Ctrl+Shift+F` / `Cmd+Shift+F`) — Format the current file
- **Lumen: Lint File** — Lint the current file for errors
- **Lumen: Open REPL** — Open an interactive Lumen REPL in the terminal

### Code Snippets
The extension includes comprehensive snippets for common Lumen constructs:

| Prefix | Description |
|--------|-------------|
| `cell` | Define a cell (function) |
| `celleff` | Define a cell with effects |
| `record` | Define a record type |
| `enum` | Define an enum type |
| `match` | Match expression |
| `if` | If statement |
| `ifel` | If-else statement |
| `for` | For loop |
| `while` | While loop |
| `let` | Variable declaration |
| `grant` | Grant with policy |
| `test` | Test cell |
| `fn` | Lambda expression |
| `memory` | Memory process |
| `machine` | Machine process |
| `pipeline` | Pipeline process |
| `result` | Result type |
| `@det` | Deterministic directive |
| `rolesys` | System role |
| `roleuser` | User role |
| `roleast` | Assistant role |

### Configuration
The extension can be configured via VS Code settings:

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `lumen.executablePath` | string | `"lumen"` | Path to the lumen CLI binary |
| `lumen.lspPath` | string | `"lumen-lsp"` | Path to the lumen-lsp binary |
| `lumen.formatOnSave` | boolean | `false` | Automatically format files on save |
| `lumen.lintOnSave` | boolean | `true` | Automatically lint files on save |

### Language Server (LSP)
The extension integrates with the Lumen Language Server (`lumen-lsp`) for advanced features:
- Real-time diagnostics
- Go-to-definition (planned)
- Hover documentation (planned)
- Auto-completion (planned)
- Code actions (planned)

### Markdown Preview
For `.lm.md` files, the extension provides syntax-highlighted previews of Lumen code blocks in VS Code's markdown preview.

## Installation

### From VSIX (Recommended)
1. Build the extension: `cd editors/vscode && npm install && npm run compile`
2. Package it: `vsce package` (requires `npm install -g @vscode/vsce`)
3. Install the `.vsix` file in VS Code: Extensions → `...` → Install from VSIX

### From Source
1. Copy the `editors/vscode` directory to `~/.vscode/extensions/lumen-lang-0.1.0`
2. Restart VS Code

### Development
1. Open this directory in VS Code
2. Press `F5` to launch an Extension Development Host
3. Open a `.lm` or `.lm.md` file to test syntax highlighting and features

## Requirements

The extension requires the Lumen toolchain to be installed:
- **lumen** — The Lumen CLI (for check, run, fmt commands)
- **lumen-lsp** — The Lumen Language Server (optional, for LSP features)

Install from source:
```bash
cd /path/to/lumen
cargo build --release
cargo install --path rust/lumen-cli
cargo install --path rust/lumen-lsp  # Optional
```

Or ensure the binaries are on your PATH and configure `lumen.executablePath` and `lumen.lspPath` in settings.

## Supported Language Features

### Core Syntax
- **Control flow**: `cell`, `end`, `let`, `if`, `else`, `for`, `in`, `while`, `loop`, `match`, `return`, `break`, `continue`, `async`, `await`, `try`, `catch`, `finally`, `halt`, `spawn`
- **Declarations**: `record`, `enum`, `type`, `const`, `import`, `use`, `tool`, `grant`, `bind`
- **Effects**: `effect`, `handler`, `handle`
- **Processes**: `agent`, `pipeline`, `orchestration`, `machine`, `memory`, `guardrail`, `eval`, `pattern`
- **Machine constructs**: `state`, `transition`, `guard`, `stage`
- **Traits**: `trait`, `impl`

### AI & Runtime
- **AI keywords**: `role`, `expect`, `schema`, `emit`, `observe`, `approve`, `checkpoint`, `escalate`
- **Orchestration**: `parallel`, `race`, `vote`, `select`, `timeout`

### Types
- **Primitives**: `Int`, `Float`, `String`, `Bool`, `Bytes`, `Json`, `Null`, `Any`, `Void`
- **Collections**: `list`, `map`, `set`, `tuple`
- **Results**: `result`, `ok`, `err`

### Special Features
- **String interpolation**: `"Hello, {name}!"`
- **Raw strings**: `r"no\escape"`
- **Directives**: `@deterministic`, `@version`, `@doc_mode`, `@strict`, `@location`, and others
- **Comments**: `# line comment`, `// line comment`, `/* block comment */`

## Screenshots

### Syntax Highlighting
![Syntax highlighting example showing a Lumen function with type annotations, effects, and control flow](preview/syntax-highlight.png)

### Code Snippets
![Autocomplete showing available Lumen snippets like cell, record, match](preview/snippets.png)

### Markdown Preview
![Lumen code blocks rendered with syntax highlighting in VS Code's markdown preview](preview/markdown-preview.png)

> Note: Screenshot images are illustrative. Actual appearance depends on your color theme.

## Tree-sitter Grammar

A comprehensive tree-sitter grammar is available at `tree-sitter-lumen/` in the main repository. This can be used for:
- Building language servers with precise syntax understanding
- Advanced editor features (semantic highlighting, structural navigation)
- Static analysis tools
- Code formatters and refactoring tools

The tree-sitter grammar covers all Lumen constructs including declarations, statements, expressions, patterns, and type annotations.

## Contributing

Contributions are welcome! Please see the main [Lumen repository](https://github.com/lumen-lang/lumen) for contribution guidelines.

### Building from Source
```bash
cd editors/vscode
npm install
npm run compile
```

### Testing
```bash
npm run watch  # Watch mode for development
```

Then press `F5` in VS Code to launch the Extension Development Host.

## License

MIT — See LICENSE file in the main repository.

## Links

- [Lumen Language](https://github.com/lumen-lang/lumen)
- [Documentation](https://github.com/lumen-lang/lumen/tree/main/docs)
- [Examples](https://github.com/lumen-lang/lumen/tree/main/examples)
- [Language Specification](https://github.com/lumen-lang/lumen/blob/main/SPEC.md)
