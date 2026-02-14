# Source Model

Lumen source is authored in markdown files with the `.lm.md` extension.

## Markdown Files

Lumen extracts code from fenced code blocks:

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

When importing `import foo: bar`:

1. Look for `foo.lm.md` in the same directory
2. Look for `foo.lm` in the same directory
3. Look for `foo/mod.lm.md` in subdirectory
4. Look for `foo/main.lm.md` in subdirectory

## Best Practices

### Documentation

Write documentation alongside code:

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

- [Types](/reference/types) — Type system overview
- [Declarations](/reference/declarations) — Top-level declarations
