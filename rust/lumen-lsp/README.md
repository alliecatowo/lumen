# lumen-lsp

Language Server Protocol implementation for Lumen.

## Overview

`lumen-lsp` provides a full-featured Language Server Protocol implementation for Lumen, enabling rich IDE support in any editor that implements the LSP specification (VS Code, Neovim, Emacs, Sublime Text, etc.). The server provides real-time type checking, hover documentation with markdown formatting, code completion, go-to-definition, signature help, document symbols, semantic tokens, folding ranges, and comprehensive diagnostics.

The LSP server integrates tightly with the compiler to provide accurate diagnostics and leverages the module resolution logic from `lumen-cli` for multi-file support. It includes a semantic search module with trigram and fuzzy matching for fast symbol lookup across large codebases.

## Architecture

| Module | Purpose |
|--------|---------|
| `src/main.rs` | LSP server entry point, stdio transport, message dispatch |
| `src/lib.rs` | Reusable library components |
| `semantic_search.rs` | Symbol index with trigram/fuzzy search for fast symbol lookup |

The server uses `lsp-server` for transport and `lsp-types` for LSP protocol types.

## Capabilities

### Hover

Rich docstrings with markdown formatting displayed when hovering over declarations. Docstrings are extracted from markdown blocks that immediately precede declarations in `.lm` and `.lumen` files.

**Example hover content**:
```
cell factorial(n: Int) -> Int

Computes the factorial of a non-negative integer.
Recursively multiplies n by factorial(n-1).
```

### Go-to-Definition

Jump to the definition of any symbol across files:
- Cells (functions)
- Records (structs)
- Enums
- Type aliases
- Imported symbols

Supports multi-file navigation via import resolution.

### Completion

Context-aware code completion suggests:
- Available cells, records, enums, types
- Field names for record construction
- Enum variants in match expressions
- Import suggestions for unresolved symbols

### Document Symbols

Hierarchical symbol outline for navigation:
- Cells → **Function** symbol kind
- Records → **Struct** symbol kind
- Enums → **Enum** kind with member children (variants)
- Processes → **Class** kind
- Effects → **Interface** kind

### Signature Help

Parameter information displayed when calling cells:
- Parameter names and types
- Return types
- Docstrings
- Builtin function signatures

**Example**:
```
factorial(n: Int) -> Int
          ^^^^^
          n: Int — The input number
```

### Semantic Tokens

Enhanced syntax highlighting with semantic information:
- Distinguishes types, functions, variables, constants
- Marks markdown blocks as comments
- Handles multi-line markdown blocks with delta encoding

### Folding Ranges

Code folding support:
- **Region** kind for code blocks (cell bodies, record definitions)
- **Comment** kind for markdown blocks

### Diagnostics

Real-time type checking and error reporting:
- Type mismatches
- Unresolved symbols
- Effect errors (undeclared effects, missing handlers)
- Constraint violations (record `where` clauses)
- Match exhaustiveness checks

Diagnostics include:
- Source location (line, column)
- Error message with context
- Severity (error, warning, hint)
- Optional fix-it suggestions

## Usage

### Standalone Server

```bash
# Build and run the LSP server
cargo build --release -p lumen-lsp
./target/release/lumen-lsp
```

The server communicates via stdin/stdout using JSON-RPC 2.0.

### VS Code Integration

In `settings.json`:

```json
{
  "lumen.lsp.path": "/path/to/lumen-lsp",
  "lumen.lsp.trace.server": "verbose"
}
```

Or install the VS Code extension from `editors/vscode/`.

### Neovim Integration (nvim-lspconfig)

```lua
require'lspconfig'.lumen_lsp.setup{
  cmd = {"/path/to/lumen-lsp"},
  filetypes = {"lumen"},
  root_dir = function(fname)
    return vim.fn.getcwd()
  end,
}
```

### Emacs Integration (lsp-mode)

```elisp
(add-to-list 'lsp-language-id-configuration '(lumen-mode . "lumen"))

(lsp-register-client
  (make-lsp-client :new-connection (lsp-stdio-connection "/path/to/lumen-lsp")
                   :major-modes '(lumen-mode)
                   :server-id 'lumen-lsp))
```

## Configuration

The LSP server respects `lumen.toml` configuration in the workspace root:

```toml
[lsp]
max_diagnostics = 100
enable_semantic_tokens = true
enable_hover = true
```

## Testing

```bash
cargo test -p lumen-lsp

# Test semantic search
cargo test -p lumen-lsp semantic_search

# Run with verbose logging
RUST_LOG=lumen_lsp=debug cargo run -p lumen-lsp
```

## Architecture Details

### Request Handling

The server handles LSP requests in a single-threaded event loop:
1. Parse JSON-RPC 2.0 message from stdin
2. Dispatch to appropriate handler based on method
3. Compile source to get AST and diagnostics
4. Compute response (hover content, completions, etc.)
5. Send JSON-RPC response to stdout

### Symbol Index

The semantic search module maintains an in-memory symbol index:
- Trigram-based fuzzy matching for fast "type-ahead" search
- Incremental updates on document changes
- Symbol metadata: name, kind, location, parent scope

### Diagnostic Pipeline

1. On document open/change, compile source
2. Collect errors from all compiler stages (lex, parse, resolve, typecheck, constraints)
3. Convert `CompileError` to LSP `Diagnostic` with ranges and severity
4. Publish diagnostics to editor
5. Cache diagnostics per document URI

## Performance

- Compilation is incremental per file (no workspace-wide analysis yet)
- Symbol index is in-memory with fast lookup (O(1) trigram query)
- Diagnostics are computed on-demand (not on idle timer)

Future optimizations:
- Workspace-wide symbol index
- Incremental compilation with change tracking
- Background diagnostic refresh

## Related Crates

- **lumen-compiler** — Type-checks and parses Lumen code
- **lumen-cli** — Provides module resolution for multi-file support
- **lsp-server** — LSP transport layer
- **lsp-types** — LSP protocol types
