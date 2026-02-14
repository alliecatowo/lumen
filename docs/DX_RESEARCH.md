# Developer Experience Research — Best-in-Class Features

**Research Date**: February 2026
**Goal**: Identify the top 20 features that will make Lumen's developer experience noticeably better than everything else.

## Executive Summary

Based on extensive research into modern programming language tooling (Rust, TypeScript, Python, Elm, Gleam, Zig, Biome, Ruff, and Zed), this document identifies the features that developers LOVE about their tools. The common thread: **speed, clarity, and zero-friction integration**.

Key insights:
- **Performance is table stakes**: Tools must be 10-100x faster than alternatives (Ruff, Biome, rust-analyzer)
- **Error messages are UX**: Elm-style friendly errors build trust and reduce cognitive load
- **Real-time feedback loops**: Sub-second diagnostics enable flow state
- **Single binary, zero config**: Gleam's unified toolchain is the gold standard
- **Contextual intelligence**: Inlay hints, semantic highlighting, and code actions reduce mental overhead

---

## Top 20 Features Ranked by Impact

### Tier 1: Game-Changing Features (Must-Have)

#### 1. **Elm-Style Error Messages with Suggestions**
**What it is**: Error messages that read like a helpful colleague, not a compiler.

**Why developers love it**:
- Uses first-person ("I see an error") to personify the compiler
- Provides plain English explanations without jargon
- Includes actionable suggestions for fixing issues
- Rust referenced Elm as inspiration for their excellent error messages

**Examples**:
```
❌ Bad (typical compiler):
  Type mismatch: expected Int, got String

✅ Good (Elm-style):
  I'm having trouble with this function call:

    5│   calculate("42")
                    ^^^^
  You're passing a String to `calculate`, but it expects an Int.

  Hint: Did you mean to parse this string first? Try `parse_int("42")`.
```

**Implementation difficulty**: Medium
**Differentiation**: HIGH — Most loved feature across all languages
**Source**: [Writing Good Compiler Error Messages](https://calebmer.com/2019/07/01/writing-good-compiler-error-messages.html), [Elm Amazing Error Messages](https://jamalambda.com/posts/2021-06-13-elm-errors.html)

---

#### 2. **Sub-Second Incremental Type Checking**
**What it is**: Real-time type checking with intelligent caching and incremental updates.

**Why developers love it**:
- Pyright is 3-5x faster than mypy on large codebases
- Watch mode provides immediate feedback as you type
- Enables flow state by removing context-switching delays

**Performance targets**:
- < 200ms for incremental checks on 10k LOC files
- < 1s for full project analysis on 100k LOC codebases

**Implementation difficulty**: High (requires sophisticated caching)
**Differentiation**: HIGH — Speed is the #1 LSP complaint
**Source**: [Python Type Checking: mypy vs Pyright Performance](https://medium.com/@asma.shaikh_19478/python-type-checking-mypy-vs-pyright-performance-battle-fce38c8cb874), [Pyright Using Static Type Checking](https://www.pythoncentral.io/pyright-using-static-type-checking/)

---

#### 3. **Structural Search and Replace (SSR)**
**What it is**: Refactoring tool that matches syntax trees instead of text patterns.

**Why developers love it**:
- Prevents false positives from string-based find/replace
- Enables safe large-scale refactorings
- Works across variable renames and code restructuring

**Example**:
```
Search:  if let Some($x) = $expr { $x } else { $default }
Replace: $expr.unwrap_or($default)
```

**Implementation difficulty**: High (requires AST pattern matching)
**Differentiation**: MEDIUM — rust-analyzer pioneered this, few others have it
**Source**: [Rust Analyzer's Secret Features](https://medium.com/@theopinionatedev/rust-analyzers-secret-features-that-even-core-devs-forget-about-9516efecf09e)

---

#### 4. **Context-Aware Inlay Hints**
**What it is**: Display inferred types, parameter names, and other contextual info inline.

**Why developers love it**:
- Reduces mental overhead by showing types without hovering
- Parameter name hints clarify function calls with multiple arguments
- Configurable to avoid clutter (show on literals only, or all)

**Best practices**:
- Lazy resolution to avoid blocking the main thread
- Language-specific adaptation (avoid where inappropriate like Haskell currying)
- User control over verbosity (none/literals/all)

**Implementation difficulty**: Medium
**Differentiation**: MEDIUM — Now standard in LSP 3.17, but quality varies
**Source**: [Feature: Inlay Hints](https://github.com/microsoft/language-server-protocol/issues/956), [ts-inlay-hints Guide](https://jellydn.github.io/ts-inlay-hints/)

---

#### 5. **Single Binary Toolchain (Gleam-Style)**
**What it is**: One executable containing compiler, LSP, formatter, linter, package manager.

**Why developers love it**:
- Zero installation friction (no npm install with 127+ packages)
- Version consistency across all tools
- Instant startup (native binary, no interpreter overhead)

**Comparison**:
- Gleam: Single binary for everything
- Biome: 1 binary vs ESLint/Prettier's 127+ npm packages
- Lumen: Already has this! (Ensure LSP is in the same binary)

**Implementation difficulty**: Low (Lumen already does this)
**Differentiation**: HIGH — Rare outside Rust ecosystem
**Source**: [Gleam programming language](https://gleam.run/), [Gleam Developer Survey 2024](https://gleam.run/news/developer-survey-2024-results/)

---

#### 6. **800+ Built-in Lint Rules with Autofix (Ruff-Style)**
**What it is**: Comprehensive linting with automatic fixes for common issues.

**Why developers love it**:
- Ruff is 10-100x faster than Flake8 (0.5s vs 30s on pandas codebase)
- Auto-upgrades to newer syntax (e.g., Python 3.10+ features)
- Removes unused imports, variables, and dead code
- Unified tool replaces 6+ separate linters

**Key features**:
- Over 800 rules (port best of ESLint, Clippy, Flake8)
- Automatic import organization
- Unused variable removal
- Dead code elimination
- Style consistency enforcement

**Implementation difficulty**: High (requires building rule library)
**Differentiation**: HIGH — Most languages have weak linters
**Source**: [Ruff: An extremely fast Python linter](https://github.com/astral-sh/ruff), [Ruff Tutorial](https://medium.com/@amjadraza24/ruff-tutorial-a-complete-guide-for-python-developers-1aa62272596d)

---

### Tier 2: Premium Features (Highly Desirable)

#### 7. **Prettier-Compatible Formatter with Custom Proc Macro Support**
**What it is**: Opinionated code formatter that produces beautiful, consistent output.

**Why developers love it**:
- Rustfmt's core principle: "Never make OK code worse"
- Semantics-preserving but not syntax-preserving (can reorganize imports, move bounds)
- Formats embedded code in doc comments and markdown
- 97% Prettier compatibility (Biome formatter)

**Design principles**:
- Do no harm: If formatting can't improve code, leave it alone
- Use source as hint where multiple valid styles exist
- Automatic import sorting and organization

**Implementation difficulty**: Medium (formatter exists, needs refinement)
**Differentiation**: MEDIUM — Most languages have formatters now
**Source**: [rustfmt Design.md](https://github.com/rust-lang/rustfmt/blob/master/Design.md), [Rustfmt: Essential Guide](https://typevar.dev/articles/rust-lang/rustfmt)

---

#### 8. **Advanced Code Actions (Quick Fixes + Refactorings)**
**What it is**: Context-aware code transformations available via lightbulb/menu.

**Why developers love it**:
- TypeScript's code actions: add missing imports, remove unused imports, organize imports, fix all issues
- One-click fixes for common errors
- Automated refactorings (extract variable, inline function, etc.)

**Code action kinds**:
- `source.fixAll.lumen` — Fix all auto-fixable issues
- `source.organizeImports.lumen` — Sort and combine imports
- `source.removeUnused.lumen` — Remove unused variables/imports
- `refactor.extract.function` — Extract selection to function
- `refactor.inline` — Inline variable/function

**Best practices**:
- Contextually relevant (only show when applicable)
- Efficient (may be called frequently)
- Clear descriptions
- Mark `isPreferred` for best option
- Easily undoable

**Implementation difficulty**: Medium to High
**Differentiation**: HIGH — Quality varies widely
**Source**: [TypeScript Language Server](https://github.com/typescript-language-server/typescript-language-server), [Code Actions and Quick Fixes](https://app.studyraid.com/en/read/8400/231869/code-actions-and-quick-fixes)

---

#### 9. **Semantic Highlighting with Token Modifiers**
**What it is**: Syntax highlighting based on semantic analysis, not just textual patterns.

**Why developers love it**:
- Distinguishes local variables from parameters from fields
- Highlights mutable vs immutable bindings
- Shows deprecated code, unsafe blocks, etc.
- Works correctly with macro-generated code

**Token types**: namespace, type, class, enum, interface, struct, typeParameter, parameter, variable, property, function, method, macro, keyword, modifier, comment, string, number, regexp, operator

**Token modifiers**: declaration, definition, readonly, static, deprecated, async, documentation, defaultLibrary

**Implementation difficulty**: Medium
**Differentiation**: MEDIUM — Part of LSP 3.16+, but quality varies
**Source**: [Semantic Highlight Guide](https://code.visualstudio.com/api/language-extensions/semantic-highlight-guide), [Using semantic highlighting in neovim](https://gist.github.com/swarn/fb37d9eefe1bc616c2a7e476c0bc0316)

---

#### 10. **Error Recovery with AST Placeholders**
**What it is**: Parser continues after syntax errors, building structurally sound AST with dummy nodes.

**Why developers love it**:
- Enables diagnostics for entire file, not just up to first error
- LSP features work in broken code (autocomplete, go-to-def, etc.)
- More fluid IDE experience

**Techniques**:
- Panic mode recovery (skip to next statement)
- Phase-level recovery (insert/delete tokens to resync)
- AST placeholders with diagnostic info
- Token synchronization

**Implementation difficulty**: Medium (Lumen already has basic recovery)
**Differentiation**: MEDIUM — Most modern parsers have this
**Source**: [Error Recovery Strategies](https://www.geeksforgeeks.org/compiler-design/error-recovery-strategies-in-compiler-design/), [Error-Tolerant Parsing](https://repository.tudelft.nl/file/File_c8b9533c-f030-4f38-95ff-8f53c5665fca)

---

#### 11. **Call Hierarchy and Type Hierarchy Navigation**
**What it is**: Visualize incoming/outgoing calls and type inheritance relationships.

**Why developers love it**:
- Essential for understanding medium-to-large codebases
- Quickly find all callers of a function
- Navigate type hierarchies (supertypes/subtypes)
- Refactor function signatures safely

**LSP support**:
- Call hierarchy: LSP 3.16 (`textDocument/prepareCallHierarchy`, `callHierarchy/incomingCalls`, `callHierarchy/outgoingCalls`)
- Type hierarchy: LSP 3.17 (`textDocument/prepareTypeHierarchy`, `typeHierarchy/supertypes`, `typeHierarchy/subtypes`)

**Implementation difficulty**: Medium
**Differentiation**: MEDIUM — LSP standard, but not all servers implement
**Source**: [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/), [Call Hierarchy Discussion](https://github.com/microsoft/language-server-protocol/issues/468)

---

#### 12. **Workspace Symbol Search with Filtering**
**What it is**: Fast fuzzy search across all symbols in workspace, filterable by kind.

**Why developers love it**:
- Jump to any function/type/variable without navigating file tree
- Fuzzy matching handles typos
- Filter by symbol kind (e.g., "only show types")
- Works across monorepo boundaries

**Performance targets**:
- < 100ms for symbol search on 100k LOC codebase
- Support for monorepo workspaces with per-package configuration

**Challenges**:
- Monorepo performance degradation (single TypeScript project vs one per package)
- Only queries LSP clients attached to current buffer in some editors

**Implementation difficulty**: Medium
**Differentiation**: MEDIUM — Common but performance varies
**Source**: [Add support for filtering workspace symbols](https://github.com/microsoft/language-server-protocol/issues/941), [Slow LSP on large monorepo](https://neovim.discourse.group/t/slow-lsp-on-large-ts-monorepo-project-caching/2668)

---

### Tier 3: Nice-to-Have Features (Competitive Parity)

#### 13. **Go to Definition, Find All References, Rename**
**What it is**: Core navigation features every LSP must have.

**Why developers love it**:
- Basic productivity multipliers
- Instant codebase navigation
- Safe refactoring

**Implementation difficulty**: Low to Medium
**Differentiation**: LOW — Expected baseline
**Source**: [Language Server Extension Guide](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide)

---

#### 14. **Hover Documentation with Type Information**
**What it is**: Show function signatures, parameter types, and doc comments on hover.

**Why developers love it**:
- Eliminates need to navigate to definition
- Contextual learning
- rust-analyzer shows struct padding info on hover (advanced)

**Implementation difficulty**: Low
**Differentiation**: LOW — Standard LSP feature
**Source**: [Rust Analyzer Release Notes](https://github.com/rust-lang/rust-analyzer/releases/2025-06-02)

---

#### 15. **Signature Help (Parameter Hints)**
**What it is**: Show function signature and parameter info while typing call.

**Why developers love it**:
- Reduces need to look up function signatures
- Shows which parameter you're currently typing

**Implementation difficulty**: Low
**Differentiation**: LOW — Standard LSP feature
**Source**: [Official LSP Documentation](https://microsoft.github.io/language-server-protocol/)

---

#### 16. **Code Completion with Snippets**
**What it is**: Context-aware autocomplete with snippet expansion.

**Why developers love it**:
- 20% reduction in typing time (research-backed)
- Snippet expansion for common patterns
- Import auto-insertion

**Implementation difficulty**: Medium
**Differentiation**: LOW — Standard, but quality matters
**Source**: [How IDEs Improve Productivity](https://www.researchgate.net/publication/269646496_How_Much_Integrated_Development_Environments_IDEs_Improve_Productivity)

---

#### 17. **Integrated Formatting (rustfmt/Prettier Integration)**
**What it is**: Format-on-save with LSP integration.

**Why developers love it**:
- Zero-thought code formatting
- Team-wide consistency
- Never discuss formatting in PRs again

**Implementation difficulty**: Low (already have formatter)
**Differentiation**: LOW — Standard feature
**Source**: [Language Server Extension Guide](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide)

---

#### 18. **Integrated Diagnostics (Compiler + Linter)**
**What it is**: Show errors, warnings, and hints inline in editor.

**Why developers love it**:
- 25% faster error detection (research)
- Fix errors before running code
- Pyright survey: 68% report type errors as top debugging frustration

**Implementation difficulty**: Low (compiler already provides this)
**Differentiation**: LOW — Standard LSP feature
**Source**: [Pyright Type Checking for VSCode LSP](https://johal.in/pyright-type-checking-vscode-lsp-for-strict-annotations-2026-2/)

---

#### 19. **Document Outline and Breadcrumbs**
**What it is**: Hierarchical view of file structure, breadcrumb navigation.

**Why developers love it**:
- Quick navigation within large files
- Visual overview of code structure
- Jump between sections

**Implementation difficulty**: Low
**Differentiation**: LOW — Standard LSP feature
**Source**: [Language Server Extension Guide](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide)

---

#### 20. **Monorepo Support with Multi-Root Workspaces**
**What it is**: Handle multiple packages/projects in single workspace.

**Why developers love it**:
- Per-package configuration (Pyright execution environments)
- Cross-package navigation and refactoring
- Shared tooling configuration

**Challenges**:
- Performance issues with large monorepos
- LSP working directory configuration critical

**Implementation difficulty**: Medium
**Differentiation**: MEDIUM — Many LSPs struggle with this
**Source**: [Pyright monorepo support](https://github.com/microsoft/pyright/blob/main/docs/mypy-comparison.md), [LSP monorepo config](https://github.com/emacs-lsp/lsp-mode/discussions/3236)

---

## Additional Insights

### Performance Benchmarks (Target Numbers)

Based on industry leaders:

| Tool | Operation | Performance | Source |
|------|-----------|-------------|--------|
| Ruff | Lint 10k LOC | 0.5s (vs Flake8 30s) | 60x faster |
| Biome | Lint monorepo | 200ms (vs ESLint 3-5s) | 15x faster |
| Pyright | Type check large codebase | 3-5x faster than mypy | Incremental updates |
| rust-analyzer | Incremental compile | Real-time (< 100ms) | Background compilation |

**Lumen targets**:
- Compile + type check: < 1s for 10k LOC
- LSP diagnostics: < 200ms incremental update
- Formatting: < 50ms for 1k LOC
- Workspace symbol search: < 100ms

### Developer Experience Metrics

Research shows:
- **30% productivity gain** from using modern IDEs
- **20% typing reduction** from code completion
- **25% faster error detection** from inline diagnostics
- **$22 billion economic benefit** from IDE adoption
- **68% of developers** cite type errors as top debugging frustration

### Language Comparison: What Developers Admire

From Stack Overflow 2025 survey:
1. **Rust** (72% admired) — Best tooling, error messages, LSP
2. **Gleam** (70% admired) — Single binary toolchain, friendly errors
3. **Elixir** (66% admired) — Great REPL, documentation
4. **Zig** (64% admired) — Clear errors (when they work)

Common themes:
- Fast feedback loops
- Helpful error messages
- Unified toolchain
- Zero-config defaults

---

## Implementation Roadmap

### Phase 1: Foundation (3-6 months)
**Goal**: Match rust-analyzer baseline

1. ✅ Basic LSP (go-to-def, hover, completion) — Already exists
2. **Elm-style error messages** — High impact, medium effort
3. **Sub-second incremental checking** — Performance optimization
4. **Error recovery with AST placeholders** — Parser improvements
5. **Integrated diagnostics** — Wire compiler errors to LSP

**Deliverable**: LSP that feels as responsive as rust-analyzer

### Phase 2: Premium Features (6-12 months)
**Goal**: Exceed rust-analyzer + Pyright

6. **Inlay hints** — Type display, parameter names
7. **Semantic highlighting** — Better than TextMate grammar
8. **Code actions** — Quick fixes, refactorings
9. **Advanced linting** — 100+ rules with autofix
10. **Call/type hierarchy** — Navigation for large codebases

**Deliverable**: Best-in-class LSP experience

### Phase 3: Innovation (12-18 months)
**Goal**: Unique features no one else has

11. **Structural search/replace** — rust-analyzer pioneered this
12. **AI-assisted error fixes** — Suggest fixes using LLMs
13. **Effect system visualization** — Show effect propagation
14. **Trace-based debugging** — Integrated with Lumen's trace runtime
15. **Cross-language imports** — Import Python/JS with type checking

**Deliverable**: Features that make Lumen uniquely powerful

---

## Differentiation Strategy

### What Makes Lumen Unique

**Already differentiated**:
- ✅ Single binary toolchain (Gleam-style)
- ✅ AI-native with tool calls and effects
- ✅ Trace runtime for observability
- ✅ Process abstractions (memory, machine, pipeline)

**Add LSP differentiation**:
1. **Effect system awareness**: Inlay hints show effect rows, code actions suggest effect declarations
2. **Tool call integration**: Hover shows tool schemas, autocomplete for grant policies
3. **Trace visualization**: Jump from LSP to trace viewer for runtime debugging
4. **Process state inspection**: Hover on machine state to see transitions and guards
5. **Deterministic mode linting**: Catch nondeterministic operations at compile time

### Competitive Positioning

| Language | Tooling Strength | Lumen Advantage |
|----------|------------------|-----------------|
| TypeScript | LSP quality, ecosystem | Simpler type system, AI-native, trace runtime |
| Rust | rust-analyzer, error messages | Faster compile times, garbage collection, simpler syntax |
| Python | Ecosystem, ease of learning | Static types, performance, effect system |
| Gleam | Unified toolchain, friendly errors | More mature ecosystem, AI integration, richer type system |
| Zig | Performance, simple compilation | Better LSP, friendlier errors, effect tracking |

**Key message**: "All the tooling quality of Rust + TypeScript, with AI superpowers and 10x simpler syntax"

---

## UX Design Principles

Based on research, apply these principles:

### 1. Elm's Error Message Philosophy
- Use first person ("I see...")
- Plain English, no jargon
- Show exactly where the problem is
- Suggest concrete fixes

### 2. Rustfmt's "Do No Harm"
- If formatter can't improve code, leave it alone
- Never take OK code and make it worse
- Be useful immediately, not just when perfect

### 3. Pyright's Performance Focus
- Watch mode with sub-second updates
- Incremental analysis only
- Graceful degradation on large files

### 4. Biome's Unified Experience
- One tool, one config file
- Sane defaults (no config needed to start)
- Clear, readable error messages

### 5. Gleam's Approachability
- 30-minute learning curve for basics
- Example-heavy documentation
- Compiler as programming assistant

---

## Measurement & Success Criteria

### Quantitative Metrics

**Performance**:
- [ ] Compile + type check 10k LOC in < 1s
- [ ] LSP diagnostics update in < 200ms
- [ ] Format 1k LOC in < 50ms
- [ ] Workspace symbol search < 100ms

**Functionality**:
- [ ] 100+ lint rules with autofix
- [ ] 20+ code actions
- [ ] Full LSP 3.17 compliance
- [ ] Zero-config startup

### Qualitative Metrics

**Developer surveys** (conduct quarterly):
- "Lumen's error messages are helpful": > 90% agree
- "Lumen's LSP is faster than [previous language]": > 80% agree
- "I would recommend Lumen's tooling": Net Promoter Score > 50

**Community feedback**:
- Track GitHub issues/stars on LSP repo
- Monitor Twitter/Reddit mentions of tooling quality
- Count blog posts/videos about DX

**Comparison benchmarks**:
- Regularly benchmark against rust-analyzer, Pyright, TypeScript LSP
- Publish results transparently
- Fix performance regressions within 1 release

---

## References

### Rust Ecosystem
- [Rust Analyzer's Secret Features](https://medium.com/@theopinionatedev/rust-analyzers-secret-features-that-even-core-devs-forget-about-9516efecf09e)
- [The State of Rust Ecosystem 2025](https://blog.jetbrains.com/rust/2026/02/11/state-of-rust-2025/)
- [rustfmt Design Principles](https://github.com/rust-lang/rustfmt/blob/master/Design.md)

### Python Tooling
- [Pyright Static Type Checking](https://www.pythoncentral.io/pyright-using-static-type-checking/)
- [mypy vs Pyright Performance](https://medium.com/@asma.shaikh_19478/python-type-checking-mypy-vs-pyright-performance-battle-fce38c8cb874)
- [Ruff: Extremely Fast Python Linter](https://github.com/astral-sh/ruff)

### TypeScript
- [TypeScript Language Server](https://github.com/typescript-language-server/typescript-language-server)
- [Biome vs ESLint 2025 Showdown](https://medium.com/@harryespant/biome-vs-eslint-the-ultimate-2025-showdown-for-javascript-developers-speed-features-and-3e5130be4a3c)

### Language Design
- [Elm Compiler Error Messages](https://calebmer.com/2019/07/01/writing-good-compiler-error-messages.html)
- [Gleam Developer Survey 2024](https://gleam.run/news/developer-survey-2024-results/)
- [2025 Stack Overflow Developer Survey](https://survey.stackoverflow.co/2025/technology)

### LSP Specification
- [Language Server Protocol 3.17](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/)
- [Inlay Hints Feature](https://github.com/microsoft/language-server-protocol/issues/956)
- [Call Hierarchy Discussion](https://github.com/microsoft/language-server-protocol/issues/468)

### Editor Integration
- [Zed Language Support](https://zed.dev/docs/languages)
- [VS Code Language Server Extension Guide](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide)
- [Semantic Highlight Guide](https://code.visualstudio.com/api/language-extensions/semantic-highlight-guide)

### Productivity Research
- [How IDEs Improve Productivity](https://www.researchgate.net/publication/269646496_How_Much_Integrated_Development_Environments_IDEs_Improve_Productivity)
- [Developer Productivity Tips 2026](https://zencoder.ai/blog/how-to-improve-developer-productivity)

### Compiler Design
- [Error Recovery Strategies](https://www.geeksforgeeks.org/compiler-design/error-recovery-strategies-in-compiler-design/)
- [Error-Tolerant Parsing Research](https://repository.tudelft.nl/file/File_c8b9533c-f030-4f38-95ff-8f53c5665fca)

---

## Conclusion

The research reveals a clear pattern: developers love tools that are **fast**, **helpful**, and **invisible**. Speed is non-negotiable (10-100x faster than alternatives), error messages must be friendly and actionable (Elm-style), and the toolchain should be unified with sane defaults (Gleam-style).

Lumen is uniquely positioned to deliver this experience because:
1. **Single binary toolchain** already exists
2. **Effect system** enables unique LSP features no one else can offer
3. **Trace runtime** provides debugging superpowers
4. **Clean slate** means we can learn from everyone's mistakes

The top 5 priorities for immediate impact:
1. ✅ **Elm-style error messages** — Highest impact for developer trust
2. ✅ **Sub-second incremental checking** — Enables flow state
3. ✅ **Inlay hints** — Reduces cognitive load
4. ✅ **Code actions with autofix** — Automates common tasks
5. ✅ **Semantic highlighting** — Better than any TextMate grammar

Implement these 5 features at rust-analyzer quality levels, and Lumen will have the best developer experience of any new language in 2026.
