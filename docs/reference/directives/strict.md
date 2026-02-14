# @strict Directive

Control strict type checking.

## Syntax

```lumen
@strict true   # Enable strict checking (default)
@strict false  # Relax some checks
```

## Default Behavior

`@strict true` is the default. It enables:

- Unresolved symbol reporting
- Type mismatch detection
- Effect declaration requirements
- Constraint validation
- Effect compatibility checking

## Strict Mode

```lumen
@strict true

cell a() -> String / {http}
  return fetch()
end

cell b() -> String        # ERROR: Undeclared effect {http}
  return a()
end
```

Fix by declaring:

```lumen
@strict true

cell a() -> String / {http}
  return fetch()
end

cell b() -> String / {http}  # OK
  return a()
end
```

## Relaxed Mode

With `@strict false`:

```lumen
@strict false

# Some checks are relaxed
# Useful for documentation snippets
```

## What Strict Mode Checks

| Check | Strict | Relaxed |
|-------|--------|---------|
| Unresolved symbols | Error | Warning |
| Type mismatches | Error | Warning |
| Undeclared effects | Error | Allowed |
| Effect compatibility | Error | Allowed |
| Constraint violations | Error | Runtime |

## When to Use Relaxed

Use `@strict false` for:

- **Documentation** — Example snippets
- **Prototyping** — Quick experiments
- **Learning** — Beginner tutorials

## Example: Documentation Snippet

```lumen
@strict false

# This is for documentation
# Some types may not be defined

cell example(data: CustomType) -> String
  return process(data)
end
```

## Example: Strict Production Code

```lumen
@strict true

record Config
  api_key: String where length(api_key) > 0
  timeout_ms: Int where timeout_ms > 0
end

cell load_config() -> result[Config, String]
  # All types checked
  # All effects declared
end
```

## Effect Provenance

Strict mode includes cause in error messages:

```
Error: UndeclaredEffect {http}
  at: cell b() in main.lm.md:10
  cause: call to fetch() in main.lm.md:5
  chain: b() -> a() -> fetch()
```

## Best Practices

1. **Keep strict mode on** — Catch errors early
2. **Only relax for docs** — Documentation snippets
3. **Declare all effects** — Make side effects visible
4. **Fix warnings** — Don't ignore type issues
