# Source Model

Lumen accepts two first-class source formats:

- Markdown files: `.lm.md`, `.lumen.md`
- Unfenced source files: `.lm`, `.lumen`

## Markdown-Native Files (`.lm.md`, `.lumen.md`)

For markdown source, Lumen extracts code from fenced `lumen` blocks:

````markdown
# My Program

This is documentation that stays with the code.

```lumen
cell main() -> String
  return "Hello, World!"
end
```

More documentation here.

```lumen
cell helper() -> Int
  return 42
end
```
````

The compiler:
1. Reads the markdown file
2. Extracts all ` ```lumen ` code blocks
3. Concatenates them into a single source
4. Compiles and executes

## Unfenced Source Files (`.lm`, `.lumen`)

Unfenced source files are compiled without requiring fenced code blocks:

```lumen
cell main() -> String
  return "Hello from unfenced source"
end
```

Use `.lm`/`.lumen` when you want direct source files. Use `.lm.md`/`.lumen.md` when you want code and narrative in one file.

The compiler accepts the same directives and interpolation features in both fenced and unfenced source.

## Top-Level Directives

Directives control compilation behavior:

```lumen
@strict true
@deterministic true
@doc_mode false
```

Directives must appear at the top of the file, before any declarations.

### @strict

Controls strict type checking:

```lumen
@strict true   # Enable strict checking (default)
@strict false  # Relax some checks
```

Strict mode:
- Reports unresolved symbols
- Catches type mismatches
- Requires explicit effect declarations
- Validates where constraints

### @deterministic

Rejects nondeterministic operations:

```lumen
@deterministic true
```

When enabled:
- `uuid()` and `uuid_v4()` are rejected
- `timestamp()` is rejected
- External tool calls require explicit effect declarations
- Future scheduling defaults to `DeferredFifo`

### @doc_mode

Relaxes checks for documentation:

```lumen
@doc_mode true
```

Useful for writing example snippets that reference undefined types.

## Code Blocks

### Standard Blocks

````markdown
```lumen
cell main() -> Int
  return 42
end
```
````

### Ignored Blocks

Code blocks without the `lumen` language tag are ignored:

````markdown
```python
# This is not processed
print("hello")
```
````

## Module Structure

A single file is a module:

````markdown
# Math Utilities

```lumen
export cell add(a: Int, b: Int) -> Int
  return a + b
end

export cell multiply(a: Int, b: Int) -> Int
  return a * b
end
```
````

## Imports

Import from other modules:

```lumen
import math: add, multiply

cell main() -> Int
  return add(2, 3)
end
```

Import all exports:

```lumen
import utils: *
```

Import with alias:

```lumen
import math: add as plus
```

## File Resolution

When importing `import foo: bar`, resolution checks:

1. `foo.lm`
2. `foo.lumen`
3. `foo.lm.md`
4. `foo.lumen.md`
5. `foo/mod.lm`
6. `foo/mod.lumen`
7. `foo/mod.lm.md`
8. `foo/mod.lumen.md`
9. `foo/main.lm`
10. `foo/main.lumen`
11. `foo/main.lm.md`
12. `foo/main.lumen.md`

## Best Practices

### Documentation-First Modules

Prefer `.lm.md`/`.lumen.md` for user-facing modules so design notes, examples, and implementation stay together:

````markdown
# User Service

Handles user authentication and profile management.

## Data Model

```lumen
record User
  id: String
  name: String
  email: String where email.contains("@")
end
```

## Authentication

```lumen
cell authenticate(email: String, password: String) -> result[User, String]
  # Implementation
end
```
````

### Source-Only Modules

Use `.lm`/`.lumen` for modules that do not need embedded prose, such as generated code or low-noise internals.

### Organization

Keep related code together:

````markdown
# Error Types

All error types for the user service.

```lumen
enum AuthError
  InvalidCredentials
  UserNotFound
  AccountLocked
end

enum ProfileError
  InvalidEmail
  UsernameTaken
end
```
````

## Next Steps

- [Types](./types) — Type system overview
- [Declarations](./declarations) — Top-level declarations
