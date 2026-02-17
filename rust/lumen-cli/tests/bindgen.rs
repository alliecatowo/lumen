//! Integration tests for the C header → Lumen bindgen module.

use lumen_cli::bindgen::*;

// =============================================================================
// c_type_to_lumen tests
// =============================================================================

#[test]
fn bindgen_c_type_to_lumen_int() {
    assert_eq!(c_type_to_lumen(&CType::Int), "Int");
}

#[test]
fn bindgen_c_type_to_lumen_float() {
    assert_eq!(c_type_to_lumen(&CType::Float), "Float");
}

#[test]
fn bindgen_c_type_to_lumen_double() {
    assert_eq!(c_type_to_lumen(&CType::Double), "Float");
}

#[test]
fn bindgen_c_type_to_lumen_char() {
    assert_eq!(c_type_to_lumen(&CType::Char), "Int");
}

#[test]
fn bindgen_c_type_to_lumen_void() {
    assert_eq!(c_type_to_lumen(&CType::Void), "Null");
}

#[test]
fn bindgen_c_type_to_lumen_bool() {
    assert_eq!(c_type_to_lumen(&CType::Bool), "Bool");
}

#[test]
fn bindgen_c_type_to_lumen_pointer() {
    let ptr = CType::Pointer(Box::new(CType::Int));
    assert_eq!(c_type_to_lumen(&ptr), "addr[Int]");

    let void_ptr = CType::Pointer(Box::new(CType::Void));
    assert_eq!(c_type_to_lumen(&void_ptr), "addr[Null]");
}

#[test]
fn bindgen_c_type_to_lumen_const_pointer() {
    let cptr = CType::ConstPointer(Box::new(CType::Char));
    assert_eq!(c_type_to_lumen(&cptr), "addr[Int]");
}

#[test]
fn bindgen_c_type_to_lumen_array() {
    let arr = CType::Array(Box::new(CType::Int), 10);
    assert_eq!(c_type_to_lumen(&arr), "List[Int]");
}

#[test]
fn bindgen_c_type_to_lumen_struct_ref() {
    assert_eq!(
        c_type_to_lumen(&CType::Struct("my_point".into())),
        "MyPoint"
    );
}

#[test]
fn bindgen_c_type_to_lumen_enum_ref() {
    assert_eq!(
        c_type_to_lumen(&CType::Enum("color_mode".into())),
        "ColorMode"
    );
}

#[test]
fn bindgen_c_type_to_lumen_fn_pointer() {
    let fnp = CType::FnPointer {
        return_type: Box::new(CType::Int),
        params: vec![CType::Int, CType::Float],
    };
    assert_eq!(c_type_to_lumen(&fnp), "Fn[Int, Float] -> Int");
}

#[test]
fn bindgen_c_type_to_lumen_nested_pointer() {
    let pp = CType::Pointer(Box::new(CType::Pointer(Box::new(CType::Char))));
    assert_eq!(c_type_to_lumen(&pp), "addr[addr[Int]]");
}

// =============================================================================
// parse_c_type tests
// =============================================================================

#[test]
fn bindgen_parse_c_type_basic_types() {
    assert_eq!(parse_c_type("int").unwrap(), CType::Int);
    assert_eq!(parse_c_type("float").unwrap(), CType::Float);
    assert_eq!(parse_c_type("double").unwrap(), CType::Double);
    assert_eq!(parse_c_type("void").unwrap(), CType::Void);
    assert_eq!(parse_c_type("char").unwrap(), CType::Char);
    assert_eq!(parse_c_type("bool").unwrap(), CType::Bool);
    assert_eq!(parse_c_type("short").unwrap(), CType::Short);
    assert_eq!(parse_c_type("long").unwrap(), CType::Long);
    assert_eq!(parse_c_type("long long").unwrap(), CType::LongLong);
}

#[test]
fn bindgen_parse_c_type_pointers() {
    assert_eq!(
        parse_c_type("int *").unwrap(),
        CType::Pointer(Box::new(CType::Int))
    );
    assert_eq!(
        parse_c_type("void *").unwrap(),
        CType::Pointer(Box::new(CType::Void))
    );
}

#[test]
fn bindgen_parse_c_type_unsigned_types() {
    assert_eq!(parse_c_type("unsigned int").unwrap(), CType::UInt);
    assert_eq!(parse_c_type("unsigned").unwrap(), CType::UInt);
    assert_eq!(parse_c_type("unsigned long").unwrap(), CType::ULong);
    assert_eq!(parse_c_type("unsigned char").unwrap(), CType::UChar);
}

#[test]
fn bindgen_parse_c_type_const_pointer() {
    assert_eq!(
        parse_c_type("const char *").unwrap(),
        CType::ConstPointer(Box::new(CType::Char))
    );
}

#[test]
fn bindgen_parse_c_type_struct_ref() {
    assert_eq!(
        parse_c_type("struct Foo").unwrap(),
        CType::Struct("Foo".into())
    );
}

#[test]
fn bindgen_parse_c_type_enum_ref() {
    assert_eq!(parse_c_type("enum Bar").unwrap(), CType::Enum("Bar".into()));
}

#[test]
fn bindgen_parse_c_type_fn_pointer() {
    assert_eq!(
        parse_c_type("int (*)(int, float)").unwrap(),
        CType::FnPointer {
            return_type: Box::new(CType::Int),
            params: vec![CType::Int, CType::Float],
        }
    );
}

#[test]
fn bindgen_parse_c_type_typedef_name() {
    assert_eq!(
        parse_c_type("size_t").unwrap(),
        CType::Typedef("size_t".into())
    );
}

// =============================================================================
// parse_c_declaration tests
// =============================================================================

#[test]
fn bindgen_parse_c_declaration_function() {
    let decl = parse_c_declaration("int add(int a, int b);")
        .unwrap()
        .unwrap();
    match decl {
        CDecl::Function {
            name,
            return_type,
            params,
            is_variadic,
        } => {
            assert_eq!(name, "add");
            assert_eq!(return_type, CType::Int);
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].name, Some("a".into()));
            assert!(!is_variadic);
        }
        _ => panic!("expected Function"),
    }
}

#[test]
fn bindgen_parse_c_declaration_variadic() {
    let decl = parse_c_declaration("int printf(const char *fmt, ...);")
        .unwrap()
        .unwrap();
    match decl {
        CDecl::Function {
            name, is_variadic, ..
        } => {
            assert_eq!(name, "printf");
            assert!(is_variadic);
        }
        _ => panic!("expected Function"),
    }
}

#[test]
fn bindgen_parse_c_declaration_typedef() {
    let decl = parse_c_declaration("typedef unsigned long size_t;")
        .unwrap()
        .unwrap();
    match decl {
        CDecl::TypedefDecl { name, target } => {
            assert_eq!(name, "size_t");
            assert_eq!(target, CType::ULong);
        }
        _ => panic!("expected TypedefDecl"),
    }
}

#[test]
fn bindgen_parse_c_declaration_skips_comments() {
    assert!(parse_c_declaration("// comment").unwrap().is_none());
    assert!(parse_c_declaration("/* block */").unwrap().is_none());
}

#[test]
fn bindgen_parse_c_declaration_skips_preprocessor() {
    assert!(parse_c_declaration("#include <stdio.h>").unwrap().is_none());
    assert!(parse_c_declaration("#define FOO 42").unwrap().is_none());
}

// =============================================================================
// generate_extern tests
// =============================================================================

#[test]
fn bindgen_generate_extern_function() {
    let decl = CDecl::Function {
        name: "strlen".to_string(),
        return_type: CType::Int,
        params: vec![CParam {
            name: Some("s".to_string()),
            ctype: CType::ConstPointer(Box::new(CType::Char)),
        }],
        is_variadic: false,
    };
    assert_eq!(
        generate_extern(&decl),
        "extern cell strlen(s: addr[Int]) -> Int"
    );
}

#[test]
fn bindgen_generate_extern_struct() {
    let decl = CDecl::StructDecl {
        name: "point".to_string(),
        fields: vec![
            CField {
                name: "x".to_string(),
                ctype: CType::Int,
            },
            CField {
                name: "y".to_string(),
                ctype: CType::Int,
            },
        ],
    };
    let result = generate_extern(&decl);
    assert!(result.contains("record Point"));
    assert!(result.contains("x: Int"));
    assert!(result.contains("y: Int"));
    assert!(result.contains("end"));
}

// =============================================================================
// generate_bindings tests
// =============================================================================

#[test]
fn bindgen_generate_bindings_multi_line_header() {
    let header = r#"
#include <stdlib.h>

void *malloc(size_t size);
void free(void *ptr);
int strlen(const char *s);
"#;
    let output = generate_bindings(header).unwrap();
    assert_eq!(output.extern_cells.len(), 3);
    assert!(output.extern_cells[0].contains("malloc"));
    assert!(output.extern_cells[1].contains("free"));
    assert!(output.extern_cells[2].contains("strlen"));
}

#[test]
fn bindgen_generate_bindings_mixed_declarations() {
    let header = r#"
struct Point { int x; int y; };
enum Color { RED, GREEN, BLUE };
typedef unsigned int uint32_t;
int compute(int a, int b);
"#;
    let output = generate_bindings(header).unwrap();
    assert_eq!(output.records.len(), 1);
    assert_eq!(output.enums.len(), 1);
    assert_eq!(output.type_aliases.len(), 1);
    assert_eq!(output.extern_cells.len(), 1);
}

// =============================================================================
// BindgenOutput::to_lumen_source tests
// =============================================================================

#[test]
fn bindgen_output_to_lumen_source_formatting() {
    let output = BindgenOutput {
        extern_cells: vec!["extern cell foo(x: Int) -> Int".to_string()],
        records: vec!["record Bar\n  val: Float\nend".to_string()],
        enums: vec!["enum Baz\n  A\n  B\nend".to_string()],
        type_aliases: vec!["type MyInt = Int".to_string()],
        warnings: vec![],
    };
    let source = output.to_lumen_source();
    assert!(source.contains("# Auto-generated Lumen bindings"));
    assert!(source.contains("# Type aliases"));
    assert!(source.contains("type MyInt = Int"));
    assert!(source.contains("# Enums"));
    assert!(source.contains("enum Baz"));
    assert!(source.contains("# Records"));
    assert!(source.contains("record Bar"));
    assert!(source.contains("# Extern functions"));
    assert!(source.contains("extern cell foo"));
}

#[test]
fn bindgen_output_to_lumen_source_empty() {
    let output = BindgenOutput::default();
    let source = output.to_lumen_source();
    assert!(source.contains("# Auto-generated Lumen bindings"));
    assert!(!source.contains("# Extern functions"));
}

// =============================================================================
// Name conversion tests
// =============================================================================

#[test]
fn bindgen_snake_to_pascal() {
    assert_eq!(snake_to_pascal("my_struct"), "MyStruct");
    assert_eq!(snake_to_pascal("hello"), "Hello");
    assert_eq!(snake_to_pascal("a_b_c"), "ABC");
    assert_eq!(snake_to_pascal("__internal"), "Internal");
}

#[test]
fn bindgen_strip_prefix_works() {
    assert_eq!(strip_prefix("SDL_Window", "SDL"), "Window");
    assert_eq!(strip_prefix("GL_TEXTURE", "GL"), "TEXTURE");
    assert_eq!(strip_prefix("MyThing", "Other"), "MyThing");
}

// =============================================================================
// BindgenError Display tests
// =============================================================================

#[test]
fn bindgen_error_display() {
    let e1 = BindgenError::ParseError {
        line: 5,
        message: "unexpected token".into(),
    };
    assert_eq!(e1.to_string(), "parse error at line 5: unexpected token");

    let e2 = BindgenError::UnsupportedType("__int128".into());
    assert_eq!(e2.to_string(), "unsupported C type: __int128");

    let e3 = BindgenError::InvalidDeclaration("garbage".into());
    assert_eq!(e3.to_string(), "invalid declaration: garbage");
}

// =============================================================================
// Additional edge-case tests
// =============================================================================

#[test]
fn bindgen_parse_void_function() {
    let decl = parse_c_declaration("void exit(int status);")
        .unwrap()
        .unwrap();
    match &decl {
        CDecl::Function {
            name, return_type, ..
        } => {
            assert_eq!(name, "exit");
            assert_eq!(*return_type, CType::Void);
        }
        _ => panic!("expected Function"),
    }
    let gen = generate_extern(&decl);
    assert_eq!(gen, "extern cell exit(status: Int)");
}

#[test]
fn bindgen_pointer_return_function() {
    let decl = parse_c_declaration("void *malloc(size_t size);")
        .unwrap()
        .unwrap();
    match &decl {
        CDecl::Function {
            name, return_type, ..
        } => {
            assert_eq!(name, "malloc");
            assert_eq!(*return_type, CType::Pointer(Box::new(CType::Void)));
        }
        _ => panic!("expected Function"),
    }
}

#[test]
fn bindgen_full_pipeline_libc_subset() {
    let header = r#"
// libc subset
#ifndef _STDLIB_H
#define _STDLIB_H

typedef unsigned long size_t;

void *malloc(size_t size);
void free(void *ptr);
void *realloc(void *ptr, size_t size);
int atoi(const char *str);

#endif
"#;
    let output = generate_bindings(header).unwrap();
    assert_eq!(output.type_aliases.len(), 1);
    assert!(output.type_aliases[0].contains("SizeT"));
    assert_eq!(output.extern_cells.len(), 4);

    let source = output.to_lumen_source();
    assert!(source.contains("extern cell malloc"));
    assert!(source.contains("extern cell free"));
    assert!(source.contains("extern cell realloc"));
    assert!(source.contains("extern cell atoi"));
}
