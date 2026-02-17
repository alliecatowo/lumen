//! WIT (WebAssembly Interface Types) generation from LIR modules.
//!
//! Produces WIT IDL text conforming to the [Component Model] specification.
//! The generator inspects an [`LirModule`] and maps:
//!
//! - **Cells** → WIT functions in an exported `lumen` interface.
//! - **Lumen types** → WIT types (records, enums, type aliases).
//! - **Tool declarations** → WIT imported interfaces.
//!
//! ## Usage
//!
//! ```ignore
//! use lumen_codegen::wit::{WitGenerator, generate_wit};
//!
//! let wit_text = generate_wit(&lir_module);
//! println!("{wit_text}");
//! ```
//!
//! [Component Model]: https://github.com/WebAssembly/component-model

use lumen_compiler::compiler::lir::{LirModule, LirType};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Generates WIT IDL text from an [`LirModule`].
pub struct WitGenerator {
    /// The package name for the WIT output.
    package_name: String,
}

impl WitGenerator {
    /// Create a new generator with the given package name.
    ///
    /// The package name appears in the `package` declaration, e.g.
    /// `package lumen:my-module;`.
    pub fn new(package_name: impl Into<String>) -> Self {
        Self {
            package_name: package_name.into(),
        }
    }

    /// Generate WIT text from the given LIR module.
    pub fn generate(&self, lir: &LirModule) -> String {
        generate_wit_with_package(lir, &self.package_name)
    }
}

/// Generate WIT text from an LIR module using the default package name
/// `lumen:module`.
pub fn generate_wit(lir: &LirModule) -> String {
    generate_wit_with_package(lir, "lumen:module")
}

/// Generate WIT text from an LIR module with a custom package name.
fn generate_wit_with_package(lir: &LirModule, package_name: &str) -> String {
    let mut out = String::new();

    // Package declaration
    out.push_str(&format!("package {package_name};\n\n"));

    // Imported interfaces for tool declarations
    if !lir.tools.is_empty() {
        for tool in &lir.tools {
            out.push_str(&format!("/// Imported tool: {}\n", tool.tool_id));
            let iface_name = sanitize_wit_ident(&tool.alias);
            out.push_str(&format!("interface {iface_name} {{\n"));
            out.push_str(&format!("  /// Invoke the {} tool.\n", tool.tool_id));
            out.push_str("  invoke: func(input: string) -> result<string, string>;\n");
            out.push_str("}\n\n");
        }
    }

    // Exported interface: types + functions
    out.push_str("interface exports {\n");

    // Type definitions
    for ty in &lir.types {
        emit_wit_type(&mut out, ty);
    }

    // Cell → function mappings
    for cell in &lir.cells {
        out.push_str(&format!("  /// Lumen cell: {}\n", cell.name));
        let func_name = sanitize_wit_ident(&cell.name);

        out.push_str(&format!("  {func_name}: func("));

        // Parameters
        let params: Vec<String> = cell
            .params
            .iter()
            .map(|p| {
                let pname = sanitize_wit_ident(&p.name);
                let pty = lumen_type_to_wit(&p.ty);
                format!("{pname}: {pty}")
            })
            .collect();
        out.push_str(&params.join(", "));

        out.push(')');

        // Return type
        if let Some(ref ret) = cell.returns {
            let wit_ret = lumen_type_to_wit(ret);
            out.push_str(&format!(" -> {wit_ret}"));
        }

        out.push_str(";\n");
    }

    out.push_str("}\n\n");

    // World declaration
    out.push_str("world lumen-module {\n");

    // Import tool interfaces
    for tool in &lir.tools {
        let iface_name = sanitize_wit_ident(&tool.alias);
        out.push_str(&format!("  import {iface_name};\n"));
    }

    out.push_str("  export exports;\n");
    out.push_str("}\n");

    out
}

// ---------------------------------------------------------------------------
// Type mapping
// ---------------------------------------------------------------------------

/// Map a Lumen type string (as stored in LIR metadata) to a WIT type.
///
/// Mapping rules:
/// - `Int` → `s64`
/// - `Float` → `float64`
/// - `String` → `string`
/// - `Bool` → `bool`
/// - `Null` / `Void` → (no return / empty tuple)
/// - `list[T]` → `list<T>`
/// - `map[K, V]` → `list<tuple<K, V>>`
/// - `result[T, E]` → `result<T, E>`
/// - `T?` → `option<T>`
/// - Records/Enums → referenced by name
/// - Everything else → `s64` (opaque)
pub fn lumen_type_to_wit(ty_str: &str) -> String {
    match ty_str {
        "Int" => "s64".to_string(),
        "Float" => "float64".to_string(),
        "String" => "string".to_string(),
        "Bool" => "bool".to_string(),
        "Null" | "Void" => "tuple<>".to_string(),
        "Bytes" => "list<u8>".to_string(),
        "Json" => "string".to_string(),
        "Any" => "s64".to_string(),
        s if s.ends_with('?') => {
            let inner = &s[..s.len() - 1];
            format!("option<{}>", lumen_type_to_wit(inner))
        }
        s if s.starts_with("list[") && s.ends_with(']') => {
            let inner = &s[5..s.len() - 1];
            format!("list<{}>", lumen_type_to_wit(inner))
        }
        s if s.starts_with("set[") && s.ends_with(']') => {
            let inner = &s[4..s.len() - 1];
            format!("list<{}>", lumen_type_to_wit(inner))
        }
        s if s.starts_with("map[") && s.ends_with(']') => {
            // map[K, V] → list<tuple<K, V>>
            let inner = &s[4..s.len() - 1];
            if let Some(comma_pos) = inner.find(", ") {
                let k = &inner[..comma_pos];
                let v = &inner[comma_pos + 2..];
                format!(
                    "list<tuple<{}, {}>>",
                    lumen_type_to_wit(k),
                    lumen_type_to_wit(v)
                )
            } else {
                "list<tuple<s64, s64>>".to_string()
            }
        }
        s if s.starts_with("result[") && s.ends_with(']') => {
            let inner = &s[7..s.len() - 1];
            if let Some(comma_pos) = inner.find(", ") {
                let ok = &inner[..comma_pos];
                let err = &inner[comma_pos + 2..];
                format!(
                    "result<{}, {}>",
                    lumen_type_to_wit(ok),
                    lumen_type_to_wit(err)
                )
            } else {
                format!("result<{}, string>", lumen_type_to_wit(inner))
            }
        }
        s if s.starts_with("tuple[") && s.ends_with(']') => {
            let inner = &s[6..s.len() - 1];
            let parts: Vec<String> = inner.split(", ").map(lumen_type_to_wit).collect();
            format!("tuple<{}>", parts.join(", "))
        }
        _ => {
            // Named type (record/enum) — emit as a kebab-case WIT reference.
            sanitize_wit_ident(ty_str)
        }
    }
}

/// Emit a WIT type definition from an LIR type.
fn emit_wit_type(out: &mut String, lir_type: &LirType) {
    let name = sanitize_wit_ident(&lir_type.name);

    match lir_type.kind.as_str() {
        "record" => {
            out.push_str(&format!("  record {name} {{\n"));
            for field in &lir_type.fields {
                let fname = sanitize_wit_ident(&field.name);
                let fty = lumen_type_to_wit(&field.ty);
                out.push_str(&format!("    {fname}: {fty},\n"));
            }
            out.push_str("  }\n\n");
        }
        "enum" => {
            if lir_type.variants.iter().all(|v| v.payload.is_none()) {
                // Simple enum (no payloads) — WIT enum.
                out.push_str(&format!("  enum {name} {{\n"));
                for variant in &lir_type.variants {
                    let vname = sanitize_wit_ident(&variant.name);
                    out.push_str(&format!("    {vname},\n"));
                }
                out.push_str("  }\n\n");
            } else {
                // Enum with payloads — WIT variant.
                out.push_str(&format!("  variant {name} {{\n"));
                for variant in &lir_type.variants {
                    let vname = sanitize_wit_ident(&variant.name);
                    if let Some(ref payload) = variant.payload {
                        let pty = lumen_type_to_wit(payload);
                        out.push_str(&format!("    {vname}({pty}),\n"));
                    } else {
                        out.push_str(&format!("    {vname},\n"));
                    }
                }
                out.push_str("  }\n\n");
            }
        }
        _ => {
            // Unknown kind — emit as a type alias to s64.
            out.push_str(&format!("  type {name} = s64;\n\n"));
        }
    }
}

// ---------------------------------------------------------------------------
// Identifier sanitization
// ---------------------------------------------------------------------------

/// Convert a Lumen identifier to a valid WIT identifier.
///
/// WIT identifiers are kebab-case: lowercase ASCII letters, digits, and hyphens.
/// This function converts CamelCase and snake_case to kebab-case.
fn sanitize_wit_ident(name: &str) -> String {
    let mut result = String::with_capacity(name.len() + 4);

    for (i, ch) in name.chars().enumerate() {
        if ch == '_' {
            result.push('-');
        } else if ch.is_ascii_uppercase() {
            if i > 0 {
                // Insert a hyphen before an uppercase letter following a lowercase.
                let prev = name.as_bytes()[i - 1];
                if prev.is_ascii_lowercase() || prev.is_ascii_digit() {
                    result.push('-');
                }
            }
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }

    // WIT identifiers cannot start with a digit.
    if result
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        result.insert(0, 'n');
    }

    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use lumen_compiler::compiler::lir::{
        Constant, Instruction, LirCell, LirField, LirModule, LirParam, LirTool, LirType,
        LirVariant, OpCode,
    };

    fn empty_lir_module(cells: Vec<LirCell>) -> LirModule {
        LirModule {
            version: "1.0.0".to_string(),
            doc_hash: "test".to_string(),
            strings: Vec::new(),
            types: Vec::new(),
            cells,
            tools: Vec::new(),
            policies: Vec::new(),
            agents: Vec::new(),
            addons: Vec::new(),
            effects: Vec::new(),
            effect_binds: Vec::new(),
            handlers: Vec::new(),
        }
    }

    fn simple_cell(name: &str, params: Vec<LirParam>, returns: Option<&str>) -> LirCell {
        LirCell {
            name: name.to_string(),
            params,
            returns: returns.map(|s| s.to_string()),
            registers: 4,
            constants: vec![Constant::Int(0)],
            instructions: vec![
                Instruction::abx(OpCode::LoadK, 0, 0),
                Instruction::abc(OpCode::Return, 0, 1, 0),
            ],
            effect_handler_metas: Vec::new(),
        }
    }

    // -- Type mapping tests -----------------------------------------------

    #[test]
    fn wit_type_mapping_primitives() {
        assert_eq!(lumen_type_to_wit("Int"), "s64");
        assert_eq!(lumen_type_to_wit("Float"), "float64");
        assert_eq!(lumen_type_to_wit("String"), "string");
        assert_eq!(lumen_type_to_wit("Bool"), "bool");
        assert_eq!(lumen_type_to_wit("Null"), "tuple<>");
        assert_eq!(lumen_type_to_wit("Bytes"), "list<u8>");
        assert_eq!(lumen_type_to_wit("Json"), "string");
    }

    #[test]
    fn wit_type_mapping_collections() {
        assert_eq!(lumen_type_to_wit("list[Int]"), "list<s64>");
        assert_eq!(lumen_type_to_wit("list[String]"), "list<string>");
        assert_eq!(lumen_type_to_wit("set[Int]"), "list<s64>");
        assert_eq!(
            lumen_type_to_wit("map[String, Int]"),
            "list<tuple<string, s64>>"
        );
    }

    #[test]
    fn wit_type_mapping_optional() {
        assert_eq!(lumen_type_to_wit("Int?"), "option<s64>");
        assert_eq!(lumen_type_to_wit("String?"), "option<string>");
    }

    #[test]
    fn wit_type_mapping_result() {
        assert_eq!(
            lumen_type_to_wit("result[Int, String]"),
            "result<s64, string>"
        );
    }

    #[test]
    fn wit_type_mapping_tuple() {
        assert_eq!(
            lumen_type_to_wit("tuple[Int, String]"),
            "tuple<s64, string>"
        );
    }

    #[test]
    fn wit_type_mapping_named() {
        // CamelCase → kebab-case
        assert_eq!(lumen_type_to_wit("MyRecord"), "my-record");
        assert_eq!(lumen_type_to_wit("HttpResponse"), "http-response");
    }

    // -- Identifier sanitization tests ------------------------------------

    #[test]
    fn sanitize_identifiers() {
        assert_eq!(sanitize_wit_ident("hello_world"), "hello-world");
        assert_eq!(sanitize_wit_ident("MyRecord"), "my-record");
        assert_eq!(sanitize_wit_ident("HTTPClient"), "httpclient");
        assert_eq!(sanitize_wit_ident("simple"), "simple");
        assert_eq!(sanitize_wit_ident("get_value"), "get-value");
    }

    // -- WIT generation: functions ----------------------------------------

    #[test]
    fn generate_wit_single_cell() {
        let cell = simple_cell("main", vec![], Some("Int"));
        let lir = empty_lir_module(vec![cell]);
        let wit = generate_wit(&lir);

        assert!(wit.contains("package lumen:module;"));
        assert!(wit.contains("interface exports {"));
        assert!(wit.contains("main: func() -> s64;"));
        assert!(wit.contains("world lumen-module {"));
        assert!(wit.contains("export exports;"));
    }

    #[test]
    fn generate_wit_cell_with_params() {
        let cell = simple_cell(
            "add",
            vec![
                LirParam {
                    name: "a".to_string(),
                    ty: "Int".to_string(),
                    register: 0,
                    variadic: false,
                },
                LirParam {
                    name: "b".to_string(),
                    ty: "Int".to_string(),
                    register: 1,
                    variadic: false,
                },
            ],
            Some("Int"),
        );
        let lir = empty_lir_module(vec![cell]);
        let wit = generate_wit(&lir);

        assert!(wit.contains("add: func(a: s64, b: s64) -> s64;"));
    }

    #[test]
    fn generate_wit_with_record_type() {
        let mut lir = empty_lir_module(vec![simple_cell("main", vec![], Some("Int"))]);
        lir.types.push(LirType {
            kind: "record".to_string(),
            name: "Point".to_string(),
            fields: vec![
                LirField {
                    name: "x".to_string(),
                    ty: "Float".to_string(),
                    constraints: vec![],
                },
                LirField {
                    name: "y".to_string(),
                    ty: "Float".to_string(),
                    constraints: vec![],
                },
            ],
            variants: vec![],
        });
        let wit = generate_wit(&lir);

        assert!(wit.contains("record point {"));
        assert!(wit.contains("x: float64,"));
        assert!(wit.contains("y: float64,"));
    }

    #[test]
    fn generate_wit_with_enum_type() {
        let mut lir = empty_lir_module(vec![simple_cell("main", vec![], Some("Int"))]);
        lir.types.push(LirType {
            kind: "enum".to_string(),
            name: "Color".to_string(),
            fields: vec![],
            variants: vec![
                LirVariant {
                    name: "Red".to_string(),
                    payload: None,
                },
                LirVariant {
                    name: "Green".to_string(),
                    payload: None,
                },
                LirVariant {
                    name: "Blue".to_string(),
                    payload: None,
                },
            ],
        });
        let wit = generate_wit(&lir);

        assert!(wit.contains("enum color {"));
        assert!(wit.contains("red,"));
        assert!(wit.contains("green,"));
        assert!(wit.contains("blue,"));
    }

    #[test]
    fn generate_wit_with_variant_type() {
        let mut lir = empty_lir_module(vec![simple_cell("main", vec![], Some("Int"))]);
        lir.types.push(LirType {
            kind: "enum".to_string(),
            name: "Shape".to_string(),
            fields: vec![],
            variants: vec![
                LirVariant {
                    name: "Circle".to_string(),
                    payload: Some("Float".to_string()),
                },
                LirVariant {
                    name: "Square".to_string(),
                    payload: Some("Float".to_string()),
                },
                LirVariant {
                    name: "None".to_string(),
                    payload: None,
                },
            ],
        });
        let wit = generate_wit(&lir);

        assert!(wit.contains("variant shape {"));
        assert!(wit.contains("circle(float64),"));
        assert!(wit.contains("square(float64),"));
        assert!(wit.contains("none,"));
    }

    #[test]
    fn generate_wit_with_tools() {
        let mut lir = empty_lir_module(vec![simple_cell("main", vec![], Some("Int"))]);
        lir.tools.push(LirTool {
            alias: "http_get".to_string(),
            tool_id: "HttpGet".to_string(),
            version: "1.0".to_string(),
            mcp_url: None,
        });
        let wit = generate_wit(&lir);

        assert!(wit.contains("interface http-get {"));
        assert!(wit.contains("invoke: func(input: string) -> result<string, string>;"));
        assert!(wit.contains("import http-get;"));
    }

    #[test]
    fn generate_wit_custom_package() {
        let gen = WitGenerator::new("myorg:mymodule");
        let lir = empty_lir_module(vec![simple_cell("main", vec![], Some("Int"))]);
        let wit = gen.generate(&lir);

        assert!(wit.contains("package myorg:mymodule;"));
    }

    #[test]
    fn generate_wit_cell_no_return() {
        let cell = simple_cell("do_stuff", vec![], None);
        let lir = empty_lir_module(vec![cell]);
        let wit = generate_wit(&lir);

        assert!(wit.contains("do-stuff: func();"));
    }
}
