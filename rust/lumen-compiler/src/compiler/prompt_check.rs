//! T088 — Static prompt checking: validate interpolation variables in prompt
//! template strings against the enclosing scope.

use crate::compiler::resolve::SymbolTable;
use crate::compiler::tokens::Span;

/// Errors detected during prompt template validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptError {
    /// The interpolated variable `{name}` is not defined in the current scope.
    UndefinedVariable { name: String, span: Span },
    /// The interpolated variable has a type that does not support string
    /// serialisation (currently only function types).
    TypeNotStringable {
        name: String,
        type_desc: String,
        span: Span,
    },
    /// The interpolated variable has a complex type (record, enum, map, etc.)
    /// that may not have a clear human-readable string representation.
    ComplexTypeWarning {
        name: String,
        type_desc: String,
        span: Span,
    },
}

impl std::fmt::Display for PromptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PromptError::UndefinedVariable { name, span } => {
                write!(
                    f,
                    "undefined variable '{}' in prompt template at line {}",
                    name, span.line
                )
            }
            PromptError::TypeNotStringable {
                name,
                type_desc,
                span,
            } => {
                write!(
                    f,
                    "variable '{}' has type '{}' which cannot be serialised to string at line {}",
                    name, type_desc, span.line
                )
            }
            PromptError::ComplexTypeWarning {
                name,
                type_desc,
                span,
            } => {
                write!(
                    f,
                    "variable '{}' has complex type '{}' without a clear string representation at line {}",
                    name, type_desc, span.line
                )
            }
        }
    }
}

/// A variable reference found inside a prompt template string.
#[derive(Debug, Clone)]
struct Interpolation {
    name: String,
    span: Span,
}

/// Scan a prompt template string for `{variable_name}` interpolations and
/// validate each one against the given scope.
///
/// `scope` maps variable names to an optional type description string.
/// If the type description is `None`, the variable is known but untyped.
///
/// `base_span` is the span of the overall string literal so that reported
/// spans have meaningful line/col info.
pub fn check_prompt_template(
    template: &str,
    scope: &SymbolTable,
    local_vars: &std::collections::HashMap<String, Option<String>>,
    base_span: Span,
) -> Vec<PromptError> {
    let interpolations = extract_interpolations(template, base_span);
    let mut errors = Vec::new();

    for interp in &interpolations {
        // Check if the variable is in local scope
        if let Some(type_desc_opt) = local_vars.get(&interp.name) {
            if let Some(type_desc) = type_desc_opt {
                check_type_stringable(&interp.name, type_desc, interp.span, &mut errors);
            }
            continue;
        }

        // Check if it's a known cell (function name usable as a reference)
        if scope.cells.contains_key(&interp.name) {
            continue;
        }

        // Check if it's a constant
        if scope.consts.contains_key(&interp.name) {
            continue;
        }

        // Not found
        errors.push(PromptError::UndefinedVariable {
            name: interp.name.clone(),
            span: interp.span,
        });
    }

    errors
}

/// Simplified entry point that only checks variable names against a flat set.
/// Useful when type information is not available.
pub fn check_prompt_variables(
    template: &str,
    known_vars: &std::collections::HashSet<String>,
    base_span: Span,
) -> Vec<PromptError> {
    let interpolations = extract_interpolations(template, base_span);
    let mut errors = Vec::new();

    for interp in &interpolations {
        if !known_vars.contains(&interp.name) {
            errors.push(PromptError::UndefinedVariable {
                name: interp.name.clone(),
                span: interp.span,
            });
        }
    }

    errors
}

/// Check whether a type description implies a type that is not string-serialisable
/// or produces a warning for complex types.
fn check_type_stringable(name: &str, type_desc: &str, span: Span, errors: &mut Vec<PromptError>) {
    let td = type_desc.trim();

    // Function types cannot be stringified meaningfully
    if td.starts_with("fn(") || td.starts_with("Fn(") {
        errors.push(PromptError::TypeNotStringable {
            name: name.to_string(),
            type_desc: td.to_string(),
            span,
        });
        return;
    }

    // Complex types that may not have a clear string representation
    let is_complex = td.starts_with("map[")
        || td.starts_with("Map[")
        || td.starts_with("set[")
        || td.starts_with("Set[")
        || td.starts_with("list[")
        || td.starts_with("List[")
        || td.starts_with("(")       // tuple
        || td.starts_with("result[")
        || td.starts_with("Result[");

    if is_complex {
        errors.push(PromptError::ComplexTypeWarning {
            name: name.to_string(),
            type_desc: td.to_string(),
            span,
        });
    }
}

/// Extract `{variable_name}` interpolation references from a template string.
///
/// Handles:
/// - Simple identifiers: `{name}`
/// - Ignores escaped braces: `{{` / `}}`
/// - Ignores expressions (anything containing `.`, `(`, `[`, `+`, `-`, `*`, `/`, spaces)
///   — we only flag simple variable references since complex expressions would need the
///   full expression parser.
fn extract_interpolations(template: &str, base_span: Span) -> Vec<Interpolation> {
    let mut results = Vec::new();
    let chars: Vec<char> = template.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut byte_offset = 0;

    while i < len {
        if chars[i] == '{' {
            // Skip escaped braces `{{`
            if i + 1 < len && chars[i + 1] == '{' {
                byte_offset += chars[i].len_utf8() + chars[i + 1].len_utf8();
                i += 2;
                continue;
            }

            let start_byte = byte_offset;
            byte_offset += chars[i].len_utf8();
            i += 1; // skip '{'

            // Collect content until closing '}'
            let mut content = String::new();
            let mut found_close = false;
            while i < len {
                if chars[i] == '}' {
                    found_close = true;
                    byte_offset += chars[i].len_utf8();
                    i += 1;
                    break;
                }
                content.push(chars[i]);
                byte_offset += chars[i].len_utf8();
                i += 1;
            }

            if !found_close {
                continue;
            }

            let trimmed = content.trim().to_string();
            if trimmed.is_empty() {
                continue;
            }

            // Only flag simple identifiers (no dots, parens, operators, spaces)
            if is_simple_identifier(&trimmed) {
                results.push(Interpolation {
                    name: trimmed,
                    span: Span {
                        start: base_span.start + start_byte,
                        end: base_span.start + byte_offset,
                        line: base_span.line,
                        col: base_span.col + start_byte,
                    },
                });
            }
        } else {
            // Skip escaped closing braces `}}`
            if chars[i] == '}' && i + 1 < len && chars[i + 1] == '}' {
                byte_offset += chars[i].len_utf8() + chars[i + 1].len_utf8();
                i += 2;
                continue;
            }
            byte_offset += chars[i].len_utf8();
            i += 1;
        }
    }

    results
}

/// A simple identifier contains only alphanumeric chars and underscores and
/// starts with a letter or underscore.
fn is_simple_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    fn dummy_span() -> Span {
        Span::new(0, 0, 1, 1)
    }

    // -- extract_interpolations -------------------------------------------

    #[test]
    fn extract_single_var() {
        let result = extract_interpolations("Hello {name}!", dummy_span());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "name");
    }

    #[test]
    fn extract_multiple_vars() {
        let result =
            extract_interpolations("{greeting}, {name}! You are {age} years old.", dummy_span());
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].name, "greeting");
        assert_eq!(result[1].name, "name");
        assert_eq!(result[2].name, "age");
    }

    #[test]
    fn extract_ignores_escaped_braces() {
        let result = extract_interpolations("{{not_a_var}} but {real}", dummy_span());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "real");
    }

    #[test]
    fn extract_ignores_expressions() {
        // Expressions with dots, parens, operators are not simple identifiers
        let result = extract_interpolations("{user.name} {len(x)} {a + b}", dummy_span());
        assert_eq!(
            result.len(),
            0,
            "should ignore non-identifier interpolations"
        );
    }

    #[test]
    fn extract_empty_braces() {
        let result = extract_interpolations("{} and { }", dummy_span());
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn extract_underscore_var() {
        let result = extract_interpolations("{_private} {my_var_2}", dummy_span());
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "_private");
        assert_eq!(result[1].name, "my_var_2");
    }

    // -- check_prompt_variables -------------------------------------------

    #[test]
    fn check_all_defined() {
        let mut known = HashSet::new();
        known.insert("name".into());
        known.insert("age".into());
        let errors = check_prompt_variables("Hello {name}, age {age}", &known, dummy_span());
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn check_undefined_variable() {
        let known = HashSet::new();
        let errors = check_prompt_variables("Hello {unknown_var}", &known, dummy_span());
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            PromptError::UndefinedVariable { name, .. } => {
                assert_eq!(name, "unknown_var");
            }
            other => panic!("expected UndefinedVariable, got {:?}", other),
        }
    }

    // -- check_prompt_template (with SymbolTable) -------------------------

    #[test]
    fn check_with_local_vars() {
        let symbols = SymbolTable::new();
        let mut locals = HashMap::new();
        locals.insert("user".into(), Some("String".into()));
        locals.insert("count".into(), Some("Int".into()));

        let errors = check_prompt_template(
            "User {user} has {count} items",
            &symbols,
            &locals,
            dummy_span(),
        );
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn check_fn_type_not_stringable() {
        let symbols = SymbolTable::new();
        let mut locals = HashMap::new();
        locals.insert("callback".into(), Some("fn(Int) -> String".into()));

        let errors = check_prompt_template("Result: {callback}", &symbols, &locals, dummy_span());
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            PromptError::TypeNotStringable { name, .. } => {
                assert_eq!(name, "callback");
            }
            other => panic!("expected TypeNotStringable, got {:?}", other),
        }
    }

    #[test]
    fn check_complex_type_warning() {
        let symbols = SymbolTable::new();
        let mut locals = HashMap::new();
        locals.insert("data".into(), Some("map[String, Int]".into()));

        let errors = check_prompt_template("Data: {data}", &symbols, &locals, dummy_span());
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            PromptError::ComplexTypeWarning { name, .. } => {
                assert_eq!(name, "data");
            }
            other => panic!("expected ComplexTypeWarning, got {:?}", other),
        }
    }

    #[test]
    fn check_known_cell_as_variable() {
        use crate::compiler::resolve::{CellInfo, SymbolTable};
        let mut symbols = SymbolTable::new();
        symbols.cells.insert(
            "greet".into(),
            CellInfo {
                params: vec![],
                return_type: None,
                effects: vec![],
                generic_params: vec![],
                must_use: false,
            },
        );
        let locals = HashMap::new();

        let errors = check_prompt_template("Call {greet}", &symbols, &locals, dummy_span());
        assert!(
            errors.is_empty(),
            "cell name should be accepted: {:?}",
            errors
        );
    }

    #[test]
    fn check_mixed_defined_and_undefined() {
        let symbols = SymbolTable::new();
        let mut locals = HashMap::new();
        locals.insert("name".into(), Some("String".into()));

        let errors = check_prompt_template(
            "Hello {name}, your id is {user_id}",
            &symbols,
            &locals,
            dummy_span(),
        );
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            PromptError::UndefinedVariable { name, .. } => {
                assert_eq!(name, "user_id");
            }
            other => panic!("expected UndefinedVariable, got {:?}", other),
        }
    }
}
