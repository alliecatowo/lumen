# Changelog

## [0.3,0] - 2026-02-16

### Changed
- Version bump to 0.3,0

All notable changes to the Lumen project will be documented in this file.

## [0.2.0] - 2026-02-15

### Language Features

- **Full algebraic effects**: `perform Effect.operation(args)`, `handle ... with ... end`, `resume(value)` — complete with lexer, parser, AST, resolver, typechecker, LIR lowering, and VM execution via one-shot delimited continuations.
- **Markdown-native `.lm` files**: Triple-backtick blocks in `.lm`/`.lumen` files are now markdown comments/docstrings. Blocks preceding declarations become rich docstrings visible in LSP hover.
- **`.lumen` file extension**: Full support alongside `.lm` and `.lm.md`.
- **`when` expressions**: Multi-branch conditionals as expressions.
- **`comptime` expressions**: Compile-time evaluated expressions.
- **`extern` declarations**: FFI boundary declarations.
- **`yield` statements**: Generator-style value yielding.
- **`defer` statements**: LIFO scope-exit cleanup.
- **`~>` compose operator**: Lazy function composition.
- **Bitwise OR operator** (`|`): Distinct from union types in type position.

### Compiler

- Full algebraic effects pipeline: 4 new LIR opcodes (`Perform`, `HandlePush`, `HandlePop`, `Resume`).
- `MarkdownBlock` token type with lexer recognition for triple-backtick blocks.
- Parser captures docstrings from markdown blocks and attaches to declarations.
- `doc: Option<String>` field on `CellDef`, `RecordDef`, `EnumDef`, `HandlerDecl`, `TypeAliasDef`.
- Type narrowing in `if x is Type` conditions.
- Tail call optimization in LIR lowering.
- 76 built-in intrinsic type signatures for reduced `Type::Any` usage.
- Let-destructuring patterns (tuple, list, record) in typechecker and lowerer.

### VM and Runtime

- **Rc-wrapped collections**: `List`, `Tuple`, `Map`, `Record` use `Rc<T>` with `Rc::make_mut()` for copy-on-write semantics — eliminates deep clones.
- **BTreeSet for sets**: Proper set data structure replacing `Vec<Value>`.
- **VM module split**: Monolithic `vm.rs` split into `mod.rs`, `intrinsics.rs`, `processes.rs`, `ops.rs`, `helpers.rs`.
- **Effect handler stack**: `EffectScope`, `SuspendedContinuation` for algebraic effect execution.
- **Index out-of-bounds**: Returns runtime error instead of null, with Python-style negative indexing.
- **7 new builtins**: `parse_json`, `to_json`, `read_file`, `write_file`, `timestamp` (returns Float), `random`, `get_env`.

### LSP

- **Hover with docstrings**: Markdown docstrings render above type signatures with rich formatting.
- **Document symbols**: Full implementation — cells as Functions, records as Structs, enums with member children, processes as Classes, effects as Interfaces.
- **Signature help**: Parameter labels, return types, docstrings, builtin function table.
- **Folding ranges**: Region kind for code blocks, Comment kind for markdown blocks.
- **Semantic tokens**: `MarkdownBlock` mapped to COMMENT with multi-line delta handling.

### VS Code Extension

- `.lumen` extension registered for syntax highlighting and LSP.
- TextMate grammar: markdown block pattern with embedded `text.html.markdown` highlighting.
- Tree-sitter grammar: `markdown_block` rule.
- Language configuration: `when`, `handle`, `defer`, `comptime` in folding and indent rules.
- LSP document selector includes `**/*.lumen` files.
- Format-on-save and lint-on-save for `.lumen` files.

### Formatter

- Markdown block preservation in `.lm`/`.lumen` files (code-first mode).
- Docstrings stay attached to declarations (no blank line insertion).
- 10 new formatter tests for markdown block handling.

### Package Manager (Wares)

- **Mandatory namespacing**: All package names must be `@namespace/name`. Bare top-level names rejected.
- Correct `@scope/name@version` parsing (version separator distinguished from namespace prefix).
- Security stubs replaced: lockfile `content_hash` verification, Ed25519 signing.
- `--frozen` and `--locked` modes enforced.
- `wares info` and `wares trust-check` implemented.
- URL canonicalization to `wares.lumen-lang.com`.

### Documentation

- SPEC.md rewritten as compilable ground truth (all code blocks compile).
- VISION.md rewritten to reflect language-for-AI-agents identity.
- Tree-sitter grammar updated for all new constructs.

### Tests

- **1,365+ tests passing**, 0 failures, 22 ignored (up from ~1,088).
- New test coverage: markdown blocks (lexer + parser), algebraic effects, formatter, LSP features.

## [0.1.10] - 2026-02-14

### Changed
- Switched HTTP providers to use `rustls-tls` to fix cross-compilation linking issues with `openssl`.
- Improved VS Code extension packaging to include platform-specific LSP binaries.
- Updated extension publishing to use `npx ovsx` with Node 20.

### Fixed
- Fixed critical bug in `lower.rs` where `where` clause record constraints were overwriting registers.
- Resolved hardcoded API key security vulnerability in Gemini provider.
- Whitelisted `out/` directory in `.vscodeignore` to ensure compiled JavaScript is included in VSIX.
- Fixed MUSL build issues by using `cross` and static linking for `openssl`.

## [0.1.0] - 2026-02-12

### Added
- Initial release of Lumen compiler, LSP, and CLI.
- AI-native primitives: tools, grants, and processes.
- Markdown-native source format support.
