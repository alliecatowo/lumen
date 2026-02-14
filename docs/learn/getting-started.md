# Getting Started

This guide walks you through creating and running your first Lumen program.

## Prerequisites

Make sure you have [installed Lumen](./installation).

## Create Your First Program

Create a file called `hello.lm.md`:

```markdown
# My First Lumen Program

This is a simple hello world example.

```lumen
cell main() -> String
  return "Hello, World!"
end
```
```

## Run It

```bash
lumen run hello.lm.md
```

Output:
```
Hello, World!
```

## What Just Happened?

1. Lumen extracted the code block from the markdown
2. It compiled the `main` cell
3. It executed the cell and printed the result

## Adding Parameters

Let's make it more interesting:

```lumen
cell greet(name: String) -> String
  return "Hello, {name}!"
end

cell main() -> String
  return greet("Lumen")
end
```

Output:
```
Hello, Lumen!
```

## Running a Specific Cell

By default, `lumen run` executes the `main` cell. You can specify a different cell:

```bash
lumen run hello.lm.md --cell greet
```

## Type Checking

Check your program without running it:

```bash
lumen check hello.lm.md
```

This validates types and catches errors early.

## Using the REPL

For interactive experimentation:

```bash
lumen repl
```

```
lumen> let x = 42
lumen> x * 2
84
lumen> :help
```

REPL commands:
- `:help` — Show available commands
- `:quit` — Exit the REPL
- `:load <file>` — Load a file
- `:type <expr>` — Show the type of an expression

## Next Steps

Now that you can run Lumen programs, continue with:

- [Language Tour](./tour) — Quick overview of all language features
- [Tutorial: Basics](./tutorial/basics) — Learn core syntax
- [Tutorial: Control Flow](./tutorial/control-flow) — Conditionals and loops
- [Language Reference](../reference/overview) — Complete specification
