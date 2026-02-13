# Lumen for Visual Studio Code

Syntax highlighting and language support for the [Lumen](https://github.com/lumen-lang/lumen) programming language.

## Features

- **Syntax highlighting** for `.lm` files and embedded Lumen code in `.lm.md` markdown files
- **TextMate grammar** for VS Code with support for all language constructs
- **Tree-sitter grammar** available for advanced tooling (see `tree-sitter-lumen/` in the repository)
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
- **Primitives**: `Int`, `Float`, `String`, `Bool`, `Bytes`, `Json`, `Null`, `Any`
- **Collections**: `list`, `map`, `set`, `tuple`
- **Results**: `result`, `ok`, `err`

### Special Features
- **String interpolation**: `"Hello, {name}!"`
- **Raw strings**: `r"no\escape"`
- **Directives**: `@deterministic`, `@version`, `@doc_mode`, `@strict`, `@location`, and others
- **Comments**: `# line comment`, `// line comment`, `/* block comment */`

## Tree-sitter Grammar

A comprehensive tree-sitter grammar is available at `tree-sitter-lumen/` in the repository. This can be used for:
- Building language servers with precise syntax understanding
- Advanced editor features (semantic highlighting, structural navigation)
- Static analysis tools
- Code formatters and refactoring tools

The tree-sitter grammar covers all Lumen constructs including declarations, statements, expressions, patterns, and type annotations.

## Language Server (Future)

LSP support is planned with features including:
- Go-to-definition
- Hover documentation
- Auto-completion
- Diagnostic reporting
- Code actions
