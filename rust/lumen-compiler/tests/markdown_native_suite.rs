use lumen_compiler::{compile, compile_with_imports};

#[test]
fn compile_unfenced_source_succeeds() {
    let source = r#"
cell main() -> Int
  return 42
end
"#;

    let module = compile(source).expect("unfenced source should compile");
    assert!(module.cells.iter().any(|c| c.name == "main"));
}

#[test]
fn compile_unfenced_directives_are_respected() {
    let source = r#"
@doc_mode true

cell main() -> Int
  return completely_unknown_var_xyz
end
"#;

    let module = compile(source).expect("doc_mode directives should work without fences");
    assert!(module.cells.iter().any(|c| c.name == "main"));
}

#[test]
fn compile_with_imports_handles_unfenced_modules() {
    let main_source = r#"
import math: square

cell main() -> Int
  return square(5)
end
"#;

    let math_source = r#"
cell square(x: Int) -> Int
  return x * x
end
"#;

    let module = compile_with_imports(main_source, &|module| {
        if module == "math" {
            Some(math_source.to_string())
        } else {
            None
        }
    })
    .expect("unfenced imports should compile");

    assert!(module.cells.iter().any(|c| c.name == "main"));
    assert!(module.cells.iter().any(|c| c.name == "square"));
}

#[test]
fn compile_fenced_markdown_still_succeeds() {
    let source = r#"# Test

```lumen
cell main() -> Int
  return 1
end
```
"#;

    let module = compile(source).expect("fenced markdown should still compile");
    assert!(module.cells.iter().any(|c| c.name == "main"));
}

#[test]
fn role_inline_with_explicit_end_and_interpolation_compiles() {
    let source = r#"
cell main() -> Int
  let name = "Allie"
  let r = role assistant: I am {name}'s assistant. end
  print(r)
  return 0
end
"#;

    let module = compile(source).expect("inline role content should parse with explicit end");
    assert!(module.cells.iter().any(|c| c.name == "main"));
}

#[test]
fn prose_only_markdown_remains_empty() {
    let source = r#"# Heading

This markdown has no Lumen declarations.
"#;

    let module = compile(source).expect("prose-only markdown should not fail");
    assert!(module.cells.is_empty());
    assert!(module.types.is_empty());
}
