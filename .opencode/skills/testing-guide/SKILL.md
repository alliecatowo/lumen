---
name: testing-guide
description: How to write and run tests for Lumen - test commands, test structure, spec suite patterns, and test writing conventions
---

# Testing Guide for Lumen

## Commands
```bash
cargo test --workspace                                # All tests (~5,300+ passing)
cargo test -p lumen-compiler                          # Compiler tests only
cargo test -p lumen-rt                                # VM + runtime tests
cargo test -p lumen-cli                               # CLI tests
cargo test -p lumen-lsp                               # LSP tests
cargo test -p lumen-compiler -- spec_suite::test_name # Single spec test
cargo clippy --workspace                              # Lint check
```

## Test Structure

### Compiler Tests (`rust/lumen-compiler/tests/`)
- `spec_markdown_sweep.rs`: Compiles every code block in `SPEC.md` (auto-stubs undefined types)
- `spec_suite.rs`: Semantic compiler tests with compile-ok and compile-err cases

### Inline Unit Tests
All crates use `#[cfg(test)] mod tests { ... }` inline in source files.

### Ignored Tests (22 total)
Integration tests requiring external services (Gemini API, MCP servers, provider registry). Do NOT un-ignore these.

## Writing Tests

### Compile-OK Test Pattern
```rust
#[test]
fn test_feature_name() {
    let src = r#"
cell main() -> Int
  return 42
end
"#;
    let module = compile(src).expect("should compile");
    assert_eq!(module.cells.len(), 1);
}
```

### Compile-Error Test Pattern
```rust
#[test]
fn test_type_error() {
    let src = r#"
cell main() -> Int
  return "not an int"
end
"#;
    let err = compile(src).unwrap_err();
    assert!(matches!(err, CompileError::Type(_)));
}
```

### VM Execution Test Pattern
```rust
#[test]
fn test_vm_execution() {
    let src = r#"
cell main() -> Int
  return 1 + 2
end
"#;
    let module = compile(src).unwrap();
    let mut vm = VM::new(module);
    let result = vm.execute("main", &[]).unwrap();
    assert_eq!(result, Value::Int(3));
}
```

## Test Conventions
- Every code change MUST have corresponding tests
- Tests must pass before code is committed
- Use descriptive test names that explain the scenario
- Test both success and error paths
- For compiler changes: test the specific pipeline stage affected
- For VM changes: test with minimal Lumen source that exercises the opcode
