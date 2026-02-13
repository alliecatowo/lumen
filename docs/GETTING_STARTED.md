# Getting Started with Lumen

Welcome to Lumen, a statically typed programming language for AI-native systems. This guide will help you get up and running quickly.

## Installation

### Prerequisites

- Rust toolchain (1.70 or later)
- Cargo package manager

### Building from source

1. Clone the repository:
   ```bash
   git clone https://github.com/lumen-lang/lumen.git
   cd lumen
   ```

2. Build the compiler and CLI:
   ```bash
   cargo build --release
   ```

3. The `lumen` binary will be at `target/release/lumen`. Add it to your PATH or run directly:
   ```bash
   export PATH="$PWD/target/release:$PATH"
   ```

4. Verify installation:
   ```bash
   lumen --version
   ```

## Your First Program

### Hello World

Create a file called `hello.lm.md`:

```markdown
# Hello World Example

```lumen
cell main()
    print("Hello, world!")
    let name = "Lumen"
    print("Hello, {name}!")
end
```
```

Note: Lumen source files are markdown documents (`.lm.md`) with fenced `lumen` code blocks. This allows you to document your code naturally alongside the implementation.

### Running Your Program

Type-check the file:
```bash
lumen check hello.lm.md
```

Run the program:
```bash
lumen run hello.lm.md
```

You should see:
```
Hello, world!
Hello, Lumen!
```

## CLI Commands Overview

### Essential Commands

- **`lumen check <file>`** — Type-check a source file without running it
- **`lumen run <file>`** — Compile and execute a program (default entry: `main` cell)
- **`lumen emit <file>`** — Compile to LIR bytecode JSON for inspection
- **`lumen init`** — Create a `lumen.toml` config file in the current directory

### Development Commands

- **`lumen repl`** — Start an interactive REPL for experimenting with Lumen
- **`lumen fmt <files>`** — Format Lumen source files
- **`lumen fmt --check <files>`** — Check if files need formatting (exits 1 if changes needed)

### Package Management

- **`lumen pkg init [name]`** — Create a new Lumen package (creates subdirectory if name provided)
- **`lumen pkg build`** — Compile the package and all dependencies
- **`lumen pkg check`** — Type-check the package without running

### Advanced Commands

- **`lumen run <file> --cell <name>`** — Run a specific cell (not just `main`)
- **`lumen run <file> --trace-dir <dir>`** — Enable trace recording to directory
- **`lumen emit <file> --output <path>`** — Write LIR JSON to file instead of stdout
- **`lumen trace show <run-id>`** — Display trace events for a previous run
- **`lumen cache clear`** — Clear the tool result cache

## VS Code Setup

Lumen includes syntax highlighting for Visual Studio Code.

### Installation

1. Copy the extension:
   ```bash
   mkdir -p ~/.vscode/extensions
   cp -r editors/vscode ~/.vscode/extensions/lumen-lang
   ```

2. Restart VS Code

3. Open a `.lm.md` file to see syntax highlighting

### Features

- Syntax highlighting for all keywords, types, and constructs
- Auto-closing pairs for `cell`/`end`, `record`/`end`, etc.
- Code folding support
- Comment toggling
- Markdown integration (fenced `lumen` blocks highlighted in `.lm.md` files)

See `editors/vscode/README.md` for detailed feature documentation.

## Language Basics

### Cells (Functions)

Cells are Lumen's term for functions:

```lumen
cell add(x: Int, y: Int) -> Int
    return x + y
end

cell greet(name: String)
    print("Hello, {name}!")
end
```

### Variables and Types

```lumen
cell example()
    let x: Int = 42
    let name = "Lumen"        # Type inferred as String
    let mut counter = 0       # Mutable variable
    counter = counter + 1
end
```

### Records (Structs)

```lumen
record User
    name: String
    age: Int
    email: String
end

cell create_user() -> User
    return User(name: "Alice", age: 30, email: "alice@example.com")
end
```

### Enums

```lumen
enum Status
    Pending
    Active
    Completed
end

enum Result[T, E]
    Ok(T)
    Err(E)
end
```

### Pattern Matching

```lumen
cell handle_status(status: Status) -> String
    match status
        Pending -> "Waiting..."
        Active -> "In progress"
        Completed -> "Done!"
    end
end
```

### Effects

Lumen tracks side effects in the type system:

```lumen
cell fetch_data() -> String / {http}
    # This cell declares it performs HTTP effects
    return "data"
end

cell pure_computation() -> Int
    # No effects declared - pure function
    return 42
end
```

## Example Programs

The `examples/` directory contains several example programs:

- **`hello.lm.md`** — Basic hello world with string interpolation
- **`fibonacci.lm.md`** — Recursive Fibonacci calculator
- **`record_validation.lm.md`** — Records with field constraints
- **`data_pipeline.lm.md`** — Pipeline process example
- **`invoice_agent.lm.md`** — Agent with AI capabilities
- **`code_reviewer.lm.md`** — Code review agent
- **`todo_manager.lm.md`** — Todo list with memory process

Try running any example:
```bash
lumen run examples/fibonacci.lm.md
```

## Next Steps

### Learn More

- **[SPEC.md](../SPEC.md)** — Complete language specification (implementation-accurate)
- **[ARCHITECTURE.md](ARCHITECTURE.md)** — Compiler and VM architecture overview
- **[RUNTIME.md](RUNTIME.md)** — Runtime semantics (processes, futures, effects, traces)

### Advanced Features

Lumen includes powerful features for AI-native systems:

- **Processes** — Built-in abstractions for memory, state machines, pipelines, and orchestration
- **Deterministic execution** — `@deterministic true` directive for reproducible runs
- **Tool policy enforcement** — Fine-grained control over external tool access with grants
- **Effect system** — Track and control side effects at compile time
- **Async/await** — Native support for concurrent operations
- **Orchestration builtins** — `parallel`, `race`, `vote`, `select`, `timeout` for coordinating async work

### Contributing

- Report issues at: https://github.com/lumen-lang/lumen/issues
- See `tasks.md` for implementation roadmap
- Run tests: `cargo test --workspace`

## Troubleshooting

### Common Issues

**Error: "cannot read file"**
- Ensure the file path is correct and the file has a `.lm.md` extension

**Error: "cell not found"**
- Verify that a `cell main()` exists in your source file
- Or specify a different cell: `lumen run file.lm.md --cell other_cell`

**Type errors**
- Run `lumen check` to see detailed error messages with line numbers and context
- The compiler provides helpful diagnostics with the source location

**Runtime errors**
- Check that all required effects are declared on cells
- Verify tool grants are configured if using external tools

### Getting Help

- Read the error messages carefully — Lumen provides detailed diagnostics
- Check `SPEC.md` for language reference
- Review example programs for patterns and idioms
- File an issue on GitHub if you encounter bugs or unexpected behavior

## Quick Reference

### File Format
```markdown
# My Program

```lumen
cell main()
    print("Code goes in fenced lumen blocks")
end
```
```

### Basic Types
- `Int`, `Float`, `String`, `Bool`
- `list[T]`, `map[K, V]`, `set[T]`, `tuple[T1, T2, ...]`
- `result[T, E]` — Ok or Err variants

### Control Flow
- `if`/`else`/`end`
- `match`/`end` with pattern matching
- `for`/`in`/`end` loops
- `while`/`end` loops
- `loop`/`end` (infinite, use `break` to exit)

### Declarations
- `cell` — Functions
- `record` — Structs
- `enum` — Sum types
- `type` — Type aliases
- `const` — Constants

### Directives
- `@deterministic true` — Enable deterministic execution mode
- `@version "1.0"` — Specify language version
- `@strict true` — Enable strict mode

---

Ready to build AI-native systems? Start exploring the examples and dive into the [language specification](../SPEC.md)!
