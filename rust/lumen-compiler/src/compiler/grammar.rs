//! T087 — Type-to-grammar: compile Lumen type definitions into GBNF grammars.
//!
//! GBNF (GGML BNF) is the grammar format used by llama.cpp and other constrained
//! decoding engines. This module converts Lumen [`TypeExpr`] / [`Type`] definitions
//! into GBNF rule sets so that an LLM's output can be constrained to only produce
//! values that conform to a given Lumen type.

use crate::compiler::ast::{EnumDef, EnumVariant, FieldDef, RecordDef, TypeExpr};
use crate::compiler::resolve::{SymbolTable, TypeInfoKind};

/// Convert a Lumen [`TypeExpr`] into a complete, self-contained GBNF grammar string.
///
/// The returned string contains a `root` production plus any auxiliary rules
/// (e.g. `ws`, `int`, `float`, `string`, `bool`) that the root references.
pub fn type_to_gbnf(type_expr: &TypeExpr, symbols: &SymbolTable) -> String {
    let mut ctx = GbnfContext::new(symbols);
    let root_body = ctx.lower_type_expr(type_expr);
    ctx.set_root(root_body);
    ctx.emit()
}

// ---------------------------------------------------------------------------
// Internal context
// ---------------------------------------------------------------------------

/// Accumulates named GBNF rules while lowering a type tree.
struct GbnfContext<'a> {
    symbols: &'a SymbolTable,
    /// Named rules other than `root`.  (name, body)
    rules: Vec<(String, String)>,
    /// Track which primitive helpers have already been emitted.
    emitted_helpers: std::collections::HashSet<String>,
    /// The body expression for the `root` rule.
    root_body: Option<String>,
}

impl<'a> GbnfContext<'a> {
    fn new(symbols: &'a SymbolTable) -> Self {
        Self {
            symbols,
            rules: Vec::new(),
            emitted_helpers: std::collections::HashSet::new(),
            root_body: None,
        }
    }

    fn set_root(&mut self, body: String) {
        self.root_body = Some(body);
    }

    /// Emit the full GBNF grammar text.
    fn emit(&self) -> String {
        let mut out = String::new();
        if let Some(ref root) = self.root_body {
            out.push_str(&format!("root ::= {}\n", root));
        }
        for (name, body) in &self.rules {
            out.push_str(&format!("{} ::= {}\n", name, body));
        }
        out
    }

    // -- helpers ----------------------------------------------------------

    fn ensure_ws(&mut self) {
        if self.emitted_helpers.insert("ws".into()) {
            self.rules.push(("ws".into(), r#"[ \t\n\r]*"#.into()));
        }
    }

    fn ensure_string(&mut self) {
        if self.emitted_helpers.insert("string-value".into()) {
            self.rules
                .push(("string-value".into(), r#"'"' [^"]* '"'"#.into()));
        }
    }

    fn ensure_int(&mut self) {
        if self.emitted_helpers.insert("int-value".into()) {
            self.rules
                .push(("int-value".into(), r#"'-'? [0-9]+"#.into()));
        }
    }

    fn ensure_float(&mut self) {
        if self.emitted_helpers.insert("float-value".into()) {
            self.rules
                .push(("float-value".into(), r#"'-'? [0-9]+ '.' [0-9]+"#.into()));
        }
    }

    fn ensure_bool(&mut self) {
        if self.emitted_helpers.insert("bool-value".into()) {
            self.rules
                .push(("bool-value".into(), r#"'true' | 'false'"#.into()));
        }
    }

    fn ensure_null(&mut self) {
        if self.emitted_helpers.insert("null-value".into()) {
            self.rules.push(("null-value".into(), r#"'null'"#.into()));
        }
    }

    fn ensure_json_value(&mut self) {
        if self.emitted_helpers.insert("json-value".into()) {
            self.ensure_ws();
            self.ensure_string();
            self.ensure_int();
            self.ensure_bool();
            self.ensure_null();
            self.rules.push((
                "json-value".into(),
                "string-value | int-value | bool-value | null-value".into(),
            ));
        }
    }

    // -- type lowering ----------------------------------------------------

    /// Lower a [`TypeExpr`] to a GBNF body expression (not a full rule line).
    fn lower_type_expr(&mut self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Named(name, _) => self.lower_named(name),
            TypeExpr::List(inner, _) => {
                self.ensure_ws();
                let elem = self.lower_type_expr(inner);
                format!("'[' ws ({elem} (',' ws {elem})*)? ws ']'")
            }
            TypeExpr::Map(key, val, _) => {
                self.ensure_ws();
                let k = self.lower_type_expr(key);
                let v = self.lower_type_expr(val);
                format!("'{{' ws ({k} ':' ws {v} (',' ws {k} ':' ws {v})*)? ws '}}'")
            }
            TypeExpr::Tuple(elems, _) => {
                self.ensure_ws();
                let parts: Vec<String> = elems.iter().map(|e| self.lower_type_expr(e)).collect();
                let inner = parts.join(" ',' ws ");
                format!("'[' ws {} ws ']'", inner)
            }
            TypeExpr::Set(inner, _) => {
                // Same JSON representation as list
                self.ensure_ws();
                let elem = self.lower_type_expr(inner);
                format!("'[' ws ({elem} (',' ws {elem})*)? ws ']'")
            }
            TypeExpr::Union(variants, _) => {
                let parts: Vec<String> = variants.iter().map(|v| self.lower_type_expr(v)).collect();
                parts.join(" | ")
            }
            TypeExpr::Null(_) => {
                self.ensure_null();
                "null-value".into()
            }
            TypeExpr::Result(ok, err, _) => {
                // JSON tagged union: {"ok": <val>} | {"err": <val>}
                self.ensure_ws();
                let ok_body = self.lower_type_expr(ok);
                let err_body = self.lower_type_expr(err);
                format!(
                    "'{{' ws '\"ok\"' ':' ws {ok_body} ws '}}' | '{{' ws '\"err\"' ':' ws {err_body} ws '}}'"
                )
            }
            TypeExpr::Fn(_, _, _, _) => {
                // Functions can't be serialised — emit a string placeholder
                self.ensure_string();
                "string-value".into()
            }
            TypeExpr::Generic(name, _args, _) => {
                // Attempt to resolve as a named type in the symbol table
                self.lower_named(name)
            }
        }
    }

    /// Lower a named type (primitive or user-defined).
    fn lower_named(&mut self, name: &str) -> String {
        match name {
            "String" | "string" => {
                self.ensure_string();
                "string-value".into()
            }
            "Int" | "int" => {
                self.ensure_int();
                "int-value".into()
            }
            "Float" | "float" => {
                self.ensure_float();
                "float-value".into()
            }
            "Bool" | "bool" => {
                self.ensure_bool();
                "bool-value".into()
            }
            "Null" | "null" => {
                self.ensure_null();
                "null-value".into()
            }
            "Json" | "Any" => {
                self.ensure_json_value();
                "json-value".into()
            }
            "Bytes" => {
                self.ensure_string();
                "string-value".into()
            }
            _ => {
                // Look up in the symbol table
                if let Some(type_info) = self.symbols.types.get(name) {
                    match &type_info.kind {
                        TypeInfoKind::Record(def) => self.lower_record(def),
                        TypeInfoKind::Enum(def) => self.lower_enum(def),
                        TypeInfoKind::Builtin => {
                            self.ensure_json_value();
                            "json-value".into()
                        }
                    }
                } else {
                    // Unknown type — fall back to json-value
                    self.ensure_json_value();
                    "json-value".into()
                }
            }
        }
    }

    /// Lower a record definition to a JSON-object grammar.
    fn lower_record(&mut self, def: &RecordDef) -> String {
        let rule_name = format!("{}-rule", kebab(&def.name));
        // Avoid infinite recursion for self-referential types: if the rule is
        // already emitted (or being emitted), just reference it.
        if self.emitted_helpers.contains(&rule_name) {
            return rule_name;
        }
        // Mark as emitted *before* lowering fields (handles recursive types).
        self.emitted_helpers.insert(rule_name.clone());

        self.ensure_ws();
        let fields = &def.fields;
        if fields.is_empty() {
            let body = "'{' ws '}'".to_string();
            self.rules.push((rule_name.clone(), body));
            return rule_name;
        }

        let field_strs: Vec<String> = fields.iter().map(|f| self.lower_field(f)).collect();
        let body = format!("'{{' ws {} ws '}}'", field_strs.join(" ',' ws "));
        self.rules.push((rule_name.clone(), body));
        rule_name
    }

    fn lower_field(&mut self, field: &FieldDef) -> String {
        let val = self.lower_type_expr(&field.ty);
        format!("'\"{}\"' ':' ws {}", field.name, val)
    }

    /// Lower an enum definition.
    fn lower_enum(&mut self, def: &EnumDef) -> String {
        let rule_name = format!("{}-rule", kebab(&def.name));
        if self.emitted_helpers.contains(&rule_name) {
            return rule_name;
        }
        self.emitted_helpers.insert(rule_name.clone());

        let all_unit = def.variants.iter().all(|v| v.payload.is_none());
        if all_unit {
            let body = def
                .variants
                .iter()
                .map(|v| format!("'\"{}\"'", v.name))
                .collect::<Vec<_>>()
                .join(" | ");
            self.rules.push((rule_name.clone(), body));
        } else {
            // Tagged-union JSON: {"variant": "Name", ...payload fields}
            self.ensure_ws();
            let alts: Vec<String> = def.variants.iter().map(|v| self.lower_variant(v)).collect();
            let body = alts.join(" | ");
            self.rules.push((rule_name.clone(), body));
        }
        rule_name
    }

    fn lower_variant(&mut self, variant: &EnumVariant) -> String {
        match &variant.payload {
            None => {
                // Unit variant in a mixed enum → tagged object with just the tag
                format!(
                    "'{{' ws '\"variant\"' ':' ws '\"{}\"' ws '}}'",
                    variant.name
                )
            }
            Some(TypeExpr::Named(name, _)) => {
                // If the payload refers to a record, inline the record fields
                if let Some(type_info) = self.symbols.types.get(name.as_str()) {
                    if let TypeInfoKind::Record(def) = &type_info.kind {
                        return self.lower_variant_with_record_payload(&variant.name, def);
                    }
                }
                // Otherwise treat the payload as a single "value" field
                self.ensure_ws();
                let val = self.lower_named(name);
                format!(
                    "'{{' ws '\"variant\"' ':' ws '\"{}\"' ',' ws '\"value\"' ':' ws {} ws '}}'",
                    variant.name, val
                )
            }
            Some(payload_ty) => {
                self.ensure_ws();
                let val = self.lower_type_expr(payload_ty);
                format!(
                    "'{{' ws '\"variant\"' ':' ws '\"{}\"' ',' ws '\"value\"' ':' ws {} ws '}}'",
                    variant.name, val
                )
            }
        }
    }

    /// Variant whose payload is a record — inline the record fields alongside the tag.
    fn lower_variant_with_record_payload(&mut self, variant_name: &str, def: &RecordDef) -> String {
        self.ensure_ws();
        let mut parts = vec![format!("'\"variant\"' ':' ws '\"{}\"'", variant_name)];
        for f in &def.fields {
            parts.push(self.lower_field(f));
        }
        format!("'{{' ws {} ws '}}'", parts.join(" ',' ws "))
    }
}

/// Convert a PascalCase or camelCase name to kebab-case for GBNF rule names.
fn kebab(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push('-');
        }
        out.push(ch.to_lowercase().next().unwrap_or(ch));
    }
    out
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::ast::{FieldDef, TypeExpr};
    use crate::compiler::resolve::SymbolTable;
    use crate::compiler::tokens::Span;

    fn dummy_span() -> Span {
        Span::dummy()
    }

    #[test]
    fn gbnf_string() {
        let ty = TypeExpr::Named("String".into(), dummy_span());
        let gbnf = type_to_gbnf(&ty, &SymbolTable::new());
        assert!(gbnf.contains("root ::="));
        assert!(gbnf.contains("string-value"));
        assert!(gbnf.contains("[^\"]"));
    }

    #[test]
    fn gbnf_int() {
        let ty = TypeExpr::Named("Int".into(), dummy_span());
        let gbnf = type_to_gbnf(&ty, &SymbolTable::new());
        assert!(gbnf.contains("int-value"));
        assert!(gbnf.contains("[0-9]+"));
    }

    #[test]
    fn gbnf_float() {
        let ty = TypeExpr::Named("Float".into(), dummy_span());
        let gbnf = type_to_gbnf(&ty, &SymbolTable::new());
        assert!(gbnf.contains("float-value"));
        assert!(gbnf.contains("'.'"));
    }

    #[test]
    fn gbnf_bool() {
        let ty = TypeExpr::Named("Bool".into(), dummy_span());
        let gbnf = type_to_gbnf(&ty, &SymbolTable::new());
        assert!(gbnf.contains("bool-value"));
        assert!(gbnf.contains("'true'"));
        assert!(gbnf.contains("'false'"));
    }

    #[test]
    fn gbnf_list_of_int() {
        let inner = TypeExpr::Named("Int".into(), dummy_span());
        let ty = TypeExpr::List(Box::new(inner), dummy_span());
        let gbnf = type_to_gbnf(&ty, &SymbolTable::new());
        assert!(gbnf.contains("'['"));
        assert!(gbnf.contains("']'"));
        assert!(gbnf.contains("int-value"));
    }

    #[test]
    fn gbnf_nullable_string() {
        // String | Null  →  T | 'null'
        let ty = TypeExpr::Union(
            vec![
                TypeExpr::Named("String".into(), dummy_span()),
                TypeExpr::Null(dummy_span()),
            ],
            dummy_span(),
        );
        let gbnf = type_to_gbnf(&ty, &SymbolTable::new());
        assert!(gbnf.contains("string-value"));
        assert!(gbnf.contains("null-value"));
    }

    #[test]
    fn gbnf_record() {
        // record Point(x: Int, y: String)
        let mut symbols = SymbolTable::new();
        symbols.types.insert(
            "Point".into(),
            crate::compiler::resolve::TypeInfo {
                kind: TypeInfoKind::Record(RecordDef {
                    name: "Point".into(),
                    generic_params: vec![],
                    fields: vec![
                        FieldDef {
                            name: "x".into(),
                            ty: TypeExpr::Named("Int".into(), dummy_span()),
                            default_value: None,
                            constraint: None,
                            span: dummy_span(),
                        },
                        FieldDef {
                            name: "y".into(),
                            ty: TypeExpr::Named("String".into(), dummy_span()),
                            default_value: None,
                            constraint: None,
                            span: dummy_span(),
                        },
                    ],
                    is_pub: false,
                    span: dummy_span(),
                    doc: None,
                }),
                generic_params: vec![],
            },
        );

        let ty = TypeExpr::Named("Point".into(), dummy_span());
        let gbnf = type_to_gbnf(&ty, &symbols);
        assert!(gbnf.contains("'\"x\"'"), "expected field x: {}", gbnf);
        assert!(gbnf.contains("'\"y\"'"), "expected field y: {}", gbnf);
        assert!(gbnf.contains("int-value"), "expected int-value: {}", gbnf);
        assert!(
            gbnf.contains("string-value"),
            "expected string-value: {}",
            gbnf
        );
        assert!(gbnf.contains("'{'"), "expected {{ open: {}", gbnf);
        assert!(gbnf.contains("'}'"), "expected }} close: {}", gbnf);
    }

    #[test]
    fn gbnf_simple_enum() {
        // enum Color { Red, Green, Blue }
        let mut symbols = SymbolTable::new();
        symbols.types.insert(
            "Color".into(),
            crate::compiler::resolve::TypeInfo {
                kind: TypeInfoKind::Enum(EnumDef {
                    name: "Color".into(),
                    generic_params: vec![],
                    variants: vec![
                        EnumVariant {
                            name: "Red".into(),
                            payload: None,
                            span: dummy_span(),
                        },
                        EnumVariant {
                            name: "Green".into(),
                            payload: None,
                            span: dummy_span(),
                        },
                        EnumVariant {
                            name: "Blue".into(),
                            payload: None,
                            span: dummy_span(),
                        },
                    ],
                    methods: vec![],
                    is_pub: false,
                    span: dummy_span(),
                    doc: None,
                }),
                generic_params: vec![],
            },
        );

        let ty = TypeExpr::Named("Color".into(), dummy_span());
        let gbnf = type_to_gbnf(&ty, &symbols);
        assert!(gbnf.contains("'\"Red\"'"), "gbnf: {}", gbnf);
        assert!(gbnf.contains("'\"Green\"'"), "gbnf: {}", gbnf);
        assert!(gbnf.contains("'\"Blue\"'"), "gbnf: {}", gbnf);
    }

    #[test]
    fn gbnf_tagged_enum() {
        // enum Shape { Circle(radius: Float), Rect(w: Float, h: Float) }
        // The payload for Circle is a record-like type; we model it via a Named
        // type that resolves to a record in the symbol table.
        let mut symbols = SymbolTable::new();

        symbols.types.insert(
            "CirclePayload".into(),
            crate::compiler::resolve::TypeInfo {
                kind: TypeInfoKind::Record(RecordDef {
                    name: "CirclePayload".into(),
                    generic_params: vec![],
                    fields: vec![FieldDef {
                        name: "radius".into(),
                        ty: TypeExpr::Named("Float".into(), dummy_span()),
                        default_value: None,
                        constraint: None,
                        span: dummy_span(),
                    }],
                    is_pub: false,
                    span: dummy_span(),
                    doc: None,
                }),
                generic_params: vec![],
            },
        );
        symbols.types.insert(
            "RectPayload".into(),
            crate::compiler::resolve::TypeInfo {
                kind: TypeInfoKind::Record(RecordDef {
                    name: "RectPayload".into(),
                    generic_params: vec![],
                    fields: vec![
                        FieldDef {
                            name: "w".into(),
                            ty: TypeExpr::Named("Float".into(), dummy_span()),
                            default_value: None,
                            constraint: None,
                            span: dummy_span(),
                        },
                        FieldDef {
                            name: "h".into(),
                            ty: TypeExpr::Named("Float".into(), dummy_span()),
                            default_value: None,
                            constraint: None,
                            span: dummy_span(),
                        },
                    ],
                    is_pub: false,
                    span: dummy_span(),
                    doc: None,
                }),
                generic_params: vec![],
            },
        );
        symbols.types.insert(
            "Shape".into(),
            crate::compiler::resolve::TypeInfo {
                kind: TypeInfoKind::Enum(EnumDef {
                    name: "Shape".into(),
                    generic_params: vec![],
                    variants: vec![
                        EnumVariant {
                            name: "Circle".into(),
                            payload: Some(TypeExpr::Named("CirclePayload".into(), dummy_span())),
                            span: dummy_span(),
                        },
                        EnumVariant {
                            name: "Rect".into(),
                            payload: Some(TypeExpr::Named("RectPayload".into(), dummy_span())),
                            span: dummy_span(),
                        },
                    ],
                    methods: vec![],
                    is_pub: false,
                    span: dummy_span(),
                    doc: None,
                }),
                generic_params: vec![],
            },
        );

        let ty = TypeExpr::Named("Shape".into(), dummy_span());
        let gbnf = type_to_gbnf(&ty, &symbols);
        assert!(gbnf.contains("'\"variant\"'"), "gbnf:\n{}", gbnf);
        assert!(gbnf.contains("'\"Circle\"'"), "gbnf:\n{}", gbnf);
        assert!(gbnf.contains("'\"Rect\"'"), "gbnf:\n{}", gbnf);
        assert!(gbnf.contains("'\"radius\"'"), "gbnf:\n{}", gbnf);
        assert!(gbnf.contains("'\"w\"'"), "gbnf:\n{}", gbnf);
        assert!(gbnf.contains("'\"h\"'"), "gbnf:\n{}", gbnf);
    }

    #[test]
    fn gbnf_tuple() {
        let ty = TypeExpr::Tuple(
            vec![
                TypeExpr::Named("Int".into(), dummy_span()),
                TypeExpr::Named("String".into(), dummy_span()),
            ],
            dummy_span(),
        );
        let gbnf = type_to_gbnf(&ty, &SymbolTable::new());
        assert!(gbnf.contains("'['"), "gbnf:\n{}", gbnf);
        assert!(gbnf.contains("int-value"), "gbnf:\n{}", gbnf);
        assert!(gbnf.contains("string-value"), "gbnf:\n{}", gbnf);
    }

    #[test]
    fn gbnf_result_type() {
        let ty = TypeExpr::Result(
            Box::new(TypeExpr::Named("String".into(), dummy_span())),
            Box::new(TypeExpr::Named("Int".into(), dummy_span())),
            dummy_span(),
        );
        let gbnf = type_to_gbnf(&ty, &SymbolTable::new());
        assert!(gbnf.contains("'\"ok\"'"), "gbnf:\n{}", gbnf);
        assert!(gbnf.contains("'\"err\"'"), "gbnf:\n{}", gbnf);
    }

    #[test]
    fn gbnf_map_type() {
        let ty = TypeExpr::Map(
            Box::new(TypeExpr::Named("String".into(), dummy_span())),
            Box::new(TypeExpr::Named("Int".into(), dummy_span())),
            dummy_span(),
        );
        let gbnf = type_to_gbnf(&ty, &SymbolTable::new());
        assert!(gbnf.contains("string-value"), "gbnf:\n{}", gbnf);
        assert!(gbnf.contains("int-value"), "gbnf:\n{}", gbnf);
        assert!(gbnf.contains("'{'"), "gbnf:\n{}", gbnf);
    }
}
