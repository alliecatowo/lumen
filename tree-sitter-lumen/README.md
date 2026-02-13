# tree-sitter-lumen

Tree-sitter grammar for the Lumen programming language.

Lumen is a statically typed programming language for AI-native systems. This grammar enables syntax highlighting, code navigation, and structural editing for Lumen source files.

## Features

- Complete grammar coverage for Lumen syntax
- Syntax highlighting queries
- Local variable scope tracking
- Support for all Lumen language constructs:
  - Cells (functions)
  - Records and enums
  - Type annotations
  - Effect rows
  - Process declarations (memory, machine, pipeline, etc.)
  - Tool and grant declarations
  - Pattern matching
  - String interpolation
  - And more

## Installation

```bash
npm install tree-sitter-lumen
```

## Usage

### With tree-sitter CLI

```bash
tree-sitter generate
tree-sitter parse examples/hello.lm.md
```

### With Node.js

```javascript
const Parser = require('tree-sitter');
const Lumen = require('tree-sitter-lumen');

const parser = new Parser();
parser.setLanguage(Lumen);

const sourceCode = `
cell main() -> Int
  return 42
end
`;

const tree = parser.parse(sourceCode);
console.log(tree.rootNode.toString());
```

### With Neovim

Add to your Neovim configuration:

```lua
local parser_config = require("nvim-treesitter.parsers").get_parser_configs()
parser_config.lumen = {
  install_info = {
    url = "~/develop/lumen/tree-sitter-lumen",
    files = {"src/parser.c"},
    branch = "main",
  },
  filetype = "lumen",
}

vim.filetype.add({
  extension = {
    lm = "lumen",
  },
})
```

## Development

```bash
# Generate the parser
npm run build

# Run tests
npm test

# Parse a file
npm run parse examples/hello.lm.md
```

## License

MIT
