# Lumen for Visual Studio Code

Syntax highlighting for the [Lumen](https://github.com/lumen-lang/lumen) programming language.

## Features

- Syntax highlighting for `.lm` files
- Embedded Lumen highlighting in `.lm.md` markdown files (fenced `lumen` code blocks)
- Highlights keywords, types, operators, strings with interpolation, numbers, comments, and directives
- Auto-closing pairs and bracket matching
- Folding support for `cell`/`end`, `record`/`end`, `if`/`end`, etc.

## Installation

### From source

1. Copy the `editors/vscode` directory into `~/.vscode/extensions/lumen-lang`
2. Restart VS Code

### Development

1. Open this directory in VS Code
2. Press `F5` to launch an Extension Development Host
3. Open a `.lm` or `.lm.md` file to see syntax highlighting

## Supported Language Features

- **Control flow**: `cell`, `end`, `let`, `if`, `else`, `for`, `while`, `loop`, `match`, `return`, `break`, `continue`
- **Declarations**: `record`, `enum`, `agent`, `pipeline`, `machine`, `memory`, `effect`, `handler`, `trait`, `impl`
- **AI/runtime**: `role`, `expect`, `schema`, `emit`, `parallel`, `race`, `vote`, `select`, `timeout`
- **Types**: `Int`, `Float`, `String`, `Bool`, `list`, `map`, `set`, `tuple`, `result`
- **String interpolation**: `"Hello, {name}!"`
- **Directives**: `@deterministic`, `@version`, `@doc_mode`, `@strict`
- **Comments**: `# line comment`
