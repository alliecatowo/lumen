//! Tests for .lm raw format compilation

use lumen_compiler::{compile_raw, compile_raw_with_imports};

#[test]
fn test_compile_raw_simple() {
    let source = r#"
cell add(a: Int, b: Int) -> Int
  return a + b
end

cell main() -> Int
  return add(5, 3)
end
"#;

    let result = compile_raw(source);
    assert!(result.is_ok(), "Raw compilation should succeed");

    let module = result.unwrap();
    assert_eq!(module.cells.len(), 2, "Should have 2 cells");

    let cell_names: Vec<&str> = module.cells.iter().map(|c| c.name.as_str()).collect();
    assert!(cell_names.contains(&"add"));
    assert!(cell_names.contains(&"main"));
}

#[test]
fn test_compile_raw_with_imports() {
    let math_source = r#"
cell multiply(a: Int, b: Int) -> Int
  return a * b
end

cell square(x: Int) -> Int
  return multiply(x, x)
end
"#;

    let main_source = r#"
import math: square

cell main() -> Int
  return square(5)
end
"#;

    let resolve_import = |module_path: &str| -> Option<String> {
        if module_path == "math" {
            Some(math_source.to_string())
        } else {
            None
        }
    };

    let result = compile_raw_with_imports(main_source, &resolve_import);
    assert!(
        result.is_ok(),
        "Raw compilation with imports should succeed: {:?}",
        result.err()
    );

    let module = result.unwrap();
    assert_eq!(module.cells.len(), 1, "Should have 1 cell (main)");
    assert_eq!(module.cells[0].name, "main");
}

#[test]
fn test_compile_markdown_with_raw_import() {
    let math_source = r#"
cell square(x: Int) -> Int
  return x * x
end
"#;

    let main_source = r#"# App

```lumen
import math: square

cell main() -> Int
  return square(9)
end
```
"#;

    let resolve_import = |module_path: &str| -> Option<String> {
        if module_path == "math" {
            Some(math_source.to_string())
        } else {
            None
        }
    };

    let result = lumen_compiler::compile_with_imports(main_source, &resolve_import);
    assert!(
        result.is_ok(),
        "Markdown compilation with raw import should succeed: {:?}",
        result.err()
    );
}

#[test]
fn test_compile_raw_with_markdown_import() {
    let math_source = r#"# Math

```lumen
cell square(x: Int) -> Int
  return x * x
end
```
"#;

    let main_source = r#"
import math: square

cell main() -> Int
  return square(6)
end
"#;

    let resolve_import = |module_path: &str| -> Option<String> {
        if module_path == "math" {
            Some(math_source.to_string())
        } else {
            None
        }
    };

    let result = compile_raw_with_imports(main_source, &resolve_import);
    assert!(
        result.is_ok(),
        "Raw compilation with markdown import should succeed: {:?}",
        result.err()
    );
}

#[test]
fn test_compile_raw_empty() {
    let result = compile_raw("");
    assert!(result.is_ok(), "Empty raw source should compile");
}

#[test]
fn test_compile_raw_with_types() {
    let source = r#"
record Point
  x: Int
  y: Int
end

cell origin() -> Point
  return Point(x: 0, y: 0)
end
"#;

    let result = compile_raw(source);
    assert!(result.is_ok(), "Raw compilation with types should succeed");

    let module = result.unwrap();
    assert_eq!(module.types.len(), 1);
    assert_eq!(module.cells.len(), 1);
}
