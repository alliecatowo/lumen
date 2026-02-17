# Lumen Stability Guarantees

## Editions
Lumen uses an edition system (similar to Rust editions) to manage language evolution.
The current edition is **2026**. Specify the edition in `lumen.toml`:

```toml
[package]
edition = "2026"
```

## Feature Maturity Levels

Every language feature has a maturity level:

### Stable
These features will not break in minor/patch releases:
- Core syntax: cells, records, enums, match, if/while/for, let
- Operators: arithmetic, comparison, logical, pipe (`|>`), string interpolation
- Builtins: print, assert, type conversion, string/list/map operations
- Module system: imports, public/private visibility
- Process types: memory, machine, pipeline

### Unstable
Semantics are stable, but API details may change in minor releases.
Requires `@feature "name"` directive or `--allow-unstable` flag:
- Effect system and algebraic effects
- Macros
- Defer blocks
- Compile-time evaluation (`comptime`)
- Generators and `yield`
- Compose operator (`~>`)

### Experimental
May change or be removed entirely. Requires `@feature "name"`:
- GADTs
- Active patterns
- Probabilistic types (`Prob<T>`)
- Multi-shot continuations
- Tensor types
- Linear types

## Deprecation Policy
- Deprecated features are marked with `@deprecated "message"`
- Deprecated features emit compiler warnings
- Deprecated features are only removed in major version bumps
- At least one full minor release cycle between deprecation and removal

## Semver Guarantees
- **Patch releases** (0.5.x): Bug fixes only. No API changes.
- **Minor releases** (0.x.0): New features, unstable API changes. Stable features unchanged.
- **Major releases** (x.0.0): May remove deprecated features, change stable APIs.
