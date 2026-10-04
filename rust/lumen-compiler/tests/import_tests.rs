use lumen_compiler::compile_with_imports;

#[test]
fn test_import_cell() {
    let lib_source = r#"
```lumen
pub cell square(x: Int) -> Int
  return x * x
end
```
"#;
    let main_source = r#"
```lumen
import mathlib: square

pub cell main() -> Int
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
        "Expected 'main' pub cell in output"
    );
    assert!(
        module.cells.iter().any(|c| c.name == "square"),
        "Expected imported 'square' pub cell in output"
    );
}

#[test]
fn test_import_record() {
    let lib_source = r#"
```lumen
pub record Point
  x: Int
  y: Int
end
```
"#;
    let main_source = r#"
```lumen
import geometry: Point

pub cell origin() -> Point
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
        "Expected 'origin' pub cell in output"
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
pub cell compute(x: Int) -> Int
  return x + 10
end
```
"#;
    let main_source = r#"
```lumen
import mathlib: compute as calc

pub cell main() -> Int
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
        "Expected 'main' pub cell in output"
    );
    assert!(
        module.cells.iter().any(|c| c.name == "compute"),
        "Expected imported 'compute' pub cell in output"
    );
}

#[test]
fn test_import_multiple_symbols() {
    let lib_source = r#"
```lumen
pub cell add(x: Int, y: Int) -> Int
  return x + y
end

pub cell multiply(x: Int, y: Int) -> Int
  return x * y
end
```
"#;
    let main_source = r#"
```lumen
import math: add, multiply

pub cell main() -> Int
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
pub cell add(x: Int, y: Int) -> Int
  return x + y
end

pub record Point
  x: Int
  y: Int
end
```
"#;
    let main_source = r#"
```lumen
import math: *

pub cell main() -> Point
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

#[test]
fn importing_a_private_symbol_is_an_error_and_wildcards_skip_it() {
    let util = "pub cell shown(n: Int) -> Int\n  return n\nend\n\ncell hidden(n: Int) -> Int\n  return n\nend\n";
    let resolver = |name: &str| -> Option<String> {
        if name == "util" {
            Some(util.to_string())
        } else {
            None
        }
    };
    let named = "import util: hidden\n\ncell main() -> Int\n  return hidden(1)\nend\n";
    let err = lumen_compiler::compile_raw_with_imports(named, &resolver)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("private") || err.contains("PrivateImport"),
        "{err}"
    );

    let ok = "import util: shown\n\ncell main() -> Int\n  return shown(1)\nend\n";
    lumen_compiler::compile_raw_with_imports(ok, &resolver).expect("pub symbol imports");
}

fn util_resolver(name: &str) -> Option<String> {
    (name == "util").then(|| {
        "pub cell shown(n: Int) -> Int\n  return n\nend\n\ncell hidden(n: Int) -> Int\n  return n\nend\n\nrecord Secret\n  v: Int\nend\n\npub record Open\n  v: Int\nend\n"
            .to_string()
    })
}

#[test]
fn wildcard_import_of_a_private_cell_is_a_compile_error() {
    let src = "import util: *\n\ncell main() -> Int\n  return hidden(1)\nend\n";
    let err = lumen_compiler::compile_raw_with_imports(src, &util_resolver)
        .unwrap_err()
        .to_string();
    assert!(err.contains("PrivateImport"), "{err}");
    assert!(err.contains("hidden") && err.contains("util"), "{err}");
}

#[test]
fn wildcard_import_of_a_private_type_is_a_compile_error() {
    let src = "import util: *\n\ncell main(s: Secret) -> Int\n  return 1\nend\n";
    let err = lumen_compiler::compile_raw_with_imports(src, &util_resolver)
        .unwrap_err()
        .to_string();
    assert!(err.contains("PrivateImport"), "{err}");
    assert!(err.contains("Secret") && err.contains("util"), "{err}");
}

#[test]
fn wildcard_import_still_reaches_pub_items() {
    let src = "import util: *\n\ncell main(o: Open) -> Int\n  return shown(1)\nend\n";
    lumen_compiler::compile_raw_with_imports(src, &util_resolver).expect("pub items import");
}

#[test]
fn wildcard_import_does_not_flag_a_private_name_defined_locally() {
    let src = "import util: *\n\ncell hidden(n: Int) -> Int\n  return n\nend\n\ncell main() -> Int\n  return hidden(2)\nend\n";
    lumen_compiler::compile_raw_with_imports(src, &util_resolver).expect("local definition wins");
}
