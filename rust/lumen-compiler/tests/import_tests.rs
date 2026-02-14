use lumen_compiler::compile_with_imports;

#[test]
fn test_import_cell() {
    let lib_source = r#"
```lumen
cell square(x: Int) -> Int
  return x * x
end
```
"#;
    let main_source = r#"
```lumen
import mathlib: square

cell main() -> Int
  return square(5)
end
```
"#;
    let result = compile_with_imports(main_source, &|module| {
        if module == "mathlib" {
            Some(lib_source.to_string())
        } else {
            None
        }
    });
    assert!(
        result.is_ok(),
        "Expected successful compilation, got: {:?}",
        result.err()
    );
    let module = result.unwrap();
    // Imported cells are merged into the output module for linking
    assert!(
        module.cells.iter().any(|c| c.name == "main"),
        "Expected 'main' cell in output"
    );
    assert!(
        module.cells.iter().any(|c| c.name == "square"),
        "Expected imported 'square' cell in output"
    );
}

#[test]
fn test_import_record() {
    let lib_source = r#"
```lumen
record Point
  x: Int
  y: Int
end
```
"#;
    let main_source = r#"
```lumen
import geometry: Point

cell origin() -> Point
  return Point(x: 0, y: 0)
end
```
"#;
    let result = compile_with_imports(main_source, &|module| {
        if module == "geometry" {
            Some(lib_source.to_string())
        } else {
            None
        }
    });
    assert!(
        result.is_ok(),
        "Expected successful compilation, got: {:?}",
        result.err()
    );
    let module = result.unwrap();
    assert!(
        module.cells.iter().any(|c| c.name == "origin"),
        "Expected 'origin' cell in output"
    );
}

#[test]
fn test_circular_import() {
    let module_a = r#"
```lumen
import b: foo

cell bar() -> Int
  return foo()
end
```
"#;
    let module_b = r#"
```lumen
import a: bar

cell foo() -> Int
  return bar()
end
```
"#;
    let result = compile_with_imports(module_a, &|module| match module {
        "b" => Some(module_b.to_string()),
        "a" => Some(module_a.to_string()),
        _ => None,
    });
    assert!(result.is_err(), "Expected circular import error");
    if let Err(e) = result {
        let err_str = format!("{:?}", e);
        assert!(
            err_str.contains("CircularImport") || err_str.contains("circular"),
            "Expected CircularImport error, got: {}",
            err_str
        );
    }
}

#[test]
fn test_module_not_found() {
    let main_source = r#"
```lumen
import nonexistent: foo

cell main() -> Int
  return foo()
end
```
"#;
    let result = compile_with_imports(main_source, &|_module| None);
    assert!(result.is_err(), "Expected module not found error");
    if let Err(e) = result {
        let err_str = format!("{:?}", e);
        assert!(
            err_str.contains("ModuleNotFound") || err_str.contains("not found"),
            "Expected ModuleNotFound error, got: {}",
            err_str
        );
    }
}

#[test]
fn test_aliased_import() {
    let lib_source = r#"
```lumen
cell compute(x: Int) -> Int
  return x + 10
end
```
"#;
    let main_source = r#"
```lumen
import mathlib: compute as calc

cell main() -> Int
  return calc(5)
end
```
"#;
    let result = compile_with_imports(main_source, &|module| {
        if module == "mathlib" {
            Some(lib_source.to_string())
        } else {
            None
        }
    });
    assert!(
        result.is_ok(),
        "Expected successful compilation with aliased import, got: {:?}",
        result.err()
    );
    let module = result.unwrap();
    // Imported cells are merged into output for linking
    assert!(
        module.cells.iter().any(|c| c.name == "main"),
        "Expected 'main' cell in output"
    );
    assert!(
        module.cells.iter().any(|c| c.name == "compute"),
        "Expected imported 'compute' cell in output"
    );
}

#[test]
fn test_import_multiple_symbols() {
    let lib_source = r#"
```lumen
cell add(x: Int, y: Int) -> Int
  return x + y
end

cell multiply(x: Int, y: Int) -> Int
  return x * y
end
```
"#;
    let main_source = r#"
```lumen
import math: add, multiply

cell main() -> Int
  return add(multiply(2, 3), 4)
end
```
"#;
    let result = compile_with_imports(main_source, &|module| {
        if module == "math" {
            Some(lib_source.to_string())
        } else {
            None
        }
    });
    assert!(
        result.is_ok(),
        "Expected successful compilation with multiple imports, got: {:?}",
        result.err()
    );
}

#[test]
fn test_import_wildcard() {
    let lib_source = r#"
```lumen
cell add(x: Int, y: Int) -> Int
  return x + y
end

record Point
  x: Int
  y: Int
end
```
"#;
    let main_source = r#"
```lumen
import math: *

cell main() -> Point
  let x = add(1, 2)
  return Point(x: x, y: 0)
end
```
"#;
    let result = compile_with_imports(main_source, &|module| {
        if module == "math" {
            Some(lib_source.to_string())
        } else {
            None
        }
    });
    assert!(
        result.is_ok(),
        "Expected successful compilation with wildcard import, got: {:?}",
        result.err()
    );
}

#[test]
fn test_imported_symbol_not_found() {
    let lib_source = r#"
```lumen
cell foo() -> Int
  return 42
end
```
"#;
    let main_source = r#"
```lumen
import mylib: bar

cell main() -> Int
  return bar()
end
```
"#;
    let result = compile_with_imports(main_source, &|module| {
        if module == "mylib" {
            Some(lib_source.to_string())
        } else {
            None
        }
    });
    assert!(result.is_err(), "Expected symbol not found error");
    if let Err(e) = result {
        let err_str = format!("{:?}", e);
        assert!(
            err_str.contains("ImportedSymbolNotFound") || err_str.contains("not found"),
            "Expected ImportedSymbolNotFound error, got: {}",
            err_str
        );
    }
}
