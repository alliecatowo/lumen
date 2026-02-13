# Lumen

Lumen is a statically typed, general-purpose programming language for AI-native systems.

The project combines mainstream language fundamentals (types, control flow, compilation, VM runtime) with first-class constructs for effects, tools, orchestration, and policy-driven execution.

## Current State

- Compiler pipeline: lexer -> parser -> resolver -> typechecker -> LIR lowering.
- Runtime: register VM with tool dispatch, structured values, futures, traces, and process runtime objects.
- CLI commands: `check`, `run`, `emit`, `trace show`, `cache clear`.

## Quick Start

## Prerequisites

- Rust stable
- Cargo

## Build

```bash
git clone https://github.com/lumen-lang/lumen.git
cd lumen
cargo build --release
```

## Run

```bash
cargo run --bin lumen -- run examples/hello.lm.md
```

## Check

```bash
cargo run --bin lumen -- check examples/hello.lm.md
```

## Emit LIR JSON

```bash
cargo run --bin lumen -- emit examples/hello.lm.md --output out.json
```

## Repository Guide

- `SPEC.md`: implementation-accurate language specification.
- `tasks.md`: concrete outstanding implementation tasks.
- `ROADMAP.md`: long-horizon direction and platform goals.
- `docs/`: architecture and developer documentation.
- `rust/lumen-compiler`: compiler implementation.
- `rust/lumen-vm`: VM/runtime implementation.
- `rust/lumen-cli`: command-line interface.

## Development

Run all tests:

```bash
cargo test --workspace
```

## Contributing

Contributions are welcome. Start with:

1. `SPEC.md` for current language behavior.
2. `tasks.md` for concrete open work.
3. `ROADMAP.md` for broader direction.
