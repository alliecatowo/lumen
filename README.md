# Lumen Code

**Lumen** is a statically typed, general-purpose programming language for AI-native systems.
It is designed to compete with mainstream languages on rigor, tooling, and runtime behavior while providing first-class primitives for agents, effects, orchestration, and policy.

## 🌟 Key Features

*   **Role-Based Control Flow**: Define `role user`, `role system`, and `role assistant` blocks naturally.
*   **Type Safety**: Static type checking for `Int`, `Float`, `String`, `List`, `Map`, `Record`, and `Enum`.
*   **Tool Usage**: First-class support for `use tool` and `grant` permissions (MCP compatible).
*   **Structured Data**: Native JSON-like records and maps with strict schema validation.
*   **Resilience**: `result[Ok, Err]` error handling and `expect schema` validation instructions.
*   **Constraint Validation**: `where` clauses on record fields for runtime invariants.
*   **Effect Rows + Strict Mode**: Inferred/declared effects with strict diagnostics and deterministic-profile enforcement.
*   **Process Runtime Objects**: `pipeline`, `orchestration`, `memory`, and `machine` declarations compile to executable constructors and methods.

## 🚀 Getting Started

### Prerequisites
*   Rust (latest stable)
*   Cargo

### Installation

Clone the repository and build the compiler/VM:

```bash
git clone https://github.com/lumen-lang/lumen.git
cd lumen
cargo build --release
```

The binary will be located at `target/release/lumen`.

### Running Examples

Lumen source files use the `.lm.md` extension (Lumen Markdown).

```bash
# Run the "Hello World" example
cargo run --bin lumen -- run examples/hello.lm.md

# Run the Invoice Agent example
cargo run --bin lumen -- run examples/invoice_agent.lm.md
```

## 🛠️ Language Tour

### Roles and Interpolation
```lumen
cell main() -> String
  let name = "Allie"
  role user: Hello, {name}!
  role assistant: Hi there! How can I help?
  return "Conversation complete"
end
```

### Records with Constraints
```lumen
record Account
  id: Int
  balance: Int where balance >= 0
end
```

### Pattern Matching
```lumen
match result
  ok(val) -> print("Success: " + val)
  err(e)  -> print("Error: " + e)
end
```

## 🏗️ Architecture

*   **lumen-cli**: Command-line interface (`run`, `check`, `fmt`).
*   **lumen-compiler**: Lexer, Parser, Typechecker, and Lowerer (AST -> LIR).
*   **lumen-vm**: Register-based VM executing LIR with tool dispatch, process runtime state, and futures.

## 🎯 Direction

Lumen is not being developed as a narrow embedded DSL. The goal is a full language platform:

*   production-grade compiler diagnostics and static guarantees
*   deterministic execution profiles for orchestrations
*   first-class capabilities/effects and policy enforcement
*   high-quality tooling (formatter, language server, package workflow)

Current research-driven audit and roadmap:

*   `docs/LANGUAGE_AUDIT_2026-02-13.md`
*   `docs/LANGUAGE_COMPETITOR_AUDIT_2026-02-13.md`

## 🤝 Contributing

Contributions are welcome! Please check out the `task.md` file for current roadmap items.
