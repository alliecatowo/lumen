# Language Reference Overview

This section provides complete documentation for Lumen's syntax, semantics, and features.

## Organization

| Section | Description |
|---------|-------------|
| [Source Model](./source-model) | `.lm.md` markdown-native and `.lm` raw source |
| [Types](./types) | Built-in and user-defined types |
| [Expressions](./expressions) | Literals, operators, function calls |
| [Statements](./statements) | Control flow, assignments |
| [Pattern Matching](./patterns) | Match expressions and patterns |
| [Declarations](./declarations) | Records, enums, cells, etc. |

## Quick Reference

### Declarations

```lumen
record Name ... end           # Structured data
enum Name ... end             # Sum types
cell name() -> Type ... end   # Functions
agent Name ... end            # AI agents
effect Name ... end           # Effect declarations
handler Name ... end          # Effect handlers
use tool name as Alias        # Tool declarations
grant Alias ...               # Policy constraints
type Name = Type              # Type aliases
const NAME: Type = value      # Constants
import module: Symbol         # Imports
```

### Types

```lumen
Int, Float, Bool, String, Null, Bytes, Json     # Primitives
list[T], map[K, V], set[T], tuple[A, B]         # Collections
result[Ok, Err]                                  # Error handling
A | B                                           # Union types
fn(A, B) -> C                                   # Function types
```

### Expressions

```lumen
# Literals
42, 3.14, true, "hello", null

# Collections
[1, 2, 3], {"key": "value"}, {1, 2, 3}

# Operators
a + b, a - b, a * b, a / b, a % b, a ** b
a == b, a != b, a < b, a <= b, a > b, a >= b
a and b, a or b, not a
a ++ b                    # Concatenation
a |> f(b)                 # Pipe
a ?? b                    # Null coalescing
a?.field                  # Safe access
1..5, 1..=5              # Ranges
a << b, a >> b            # Shift operators
value is Int, value as Int # Type test/cast

# Function call
func(arg1, arg2)
func(named: value)

# String interpolation
"Hello, {name}!"

# Lambda
fn(x) => x * 2
fn(x) -> Type ... end

# Match expression
match value
  pattern -> result
  _ -> default
end
```

### Statements

```lumen
let x = value
let mut x = value
x = new_value
x += 1

if condition ... end
if condition ... else ... end

for x in items ... end
for @outer x in items if cond ... end
while @loop cond ... end
loop @spin ... end
while condition ... end
loop ... end

match value ... end

return value
break @outer
continue @outer
defer ... end
halt(message)
emit(event)
```

### Directives

```lumen
@strict true              # Enable strict mode (default)
@deterministic true       # Reject nondeterministic operations
@doc_mode true            # Relax for documentation
```

### Effects

```lumen
cell name() -> Type / {effect1, effect2}
```

## Semantic Model

### Strict Mode

Default behavior that:
- Reports unresolved symbols
- Catches type mismatches
- Requires explicit effect declarations
- Validates constraints

### Effect Tracking

Effects are tracked through call chains:

```lumen
cell a() -> Int / {http}      # Declares http effect
  return fetch()
end

cell b() -> Int               # Infers http effect from a()
  return a()
end
```

### Determinism

With `@deterministic true`:
- `uuid()` is rejected
- `timestamp()` is rejected
- Unknown tool calls are rejected
- Future scheduling defaults to deferred FIFO

## Compilation Model

1. **Input Loading** — Parse `.lm` directly, or extract fenced `lumen` blocks from `.lm.md`
2. **Lexing** — Tokenize source
3. **Parsing** — Build AST
4. **Resolution** — Build symbol table, infer effects
5. **Typechecking** — Validate types
6. **Lowering** — Generate LIR bytecode
7. **Execution** — Run on register-based VM

## Next Steps

- [Source Model](./source-model) — How markdown files work
- [Types](./types) — Complete type system
- [Expressions](./expressions) — All expression forms
