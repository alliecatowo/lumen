//! Wave 21 — T116: Active patterns (F#-style) tests
//!
//! Exercises the active pattern module:
//! - Validation (PascalCase, non-empty bindings, recursion detection)
//! - Resolution (registry lookup, unknown pattern errors)
//! - Lowering call info (function name, binding count, partiality)
//! - All three return type variants: Option, Choice, Partial

use lumen_compiler::compiler::active_patterns::*;
use lumen_compiler::compiler::ast::*;
use lumen_compiler::compiler::tokens::Span;

use std::collections::HashMap;

fn span() -> Span {
    Span::dummy()
}

/// Helper: build an ActivePatternDef with an empty body.
fn make_def(
    name: &str,
    param_name: &str,
    param_type: &str,
    return_type: ActivePatternReturn,
) -> ActivePatternDef {
    ActivePatternDef {
        name: name.to_string(),
        param_name: param_name.to_string(),
        param_type: param_type.to_string(),
        return_type,
        body: vec![],
        span: span(),
    }
}

/// Helper: build a body containing a single call expression to `callee`.
fn body_with_call(callee: &str) -> Vec<Stmt> {
    vec![Stmt::Expr(ExprStmt {
        expr: Expr::Call(
            Box::new(Expr::Ident(callee.to_string(), span())),
            vec![Expr::Ident("x".to_string(), span())]
                .into_iter()
                .map(CallArg::Positional)
                .collect(),
            span(),
        ),
        span: span(),
    })]
}

/// Helper: build a body with a nested let that references `callee`.
fn body_with_let_call(callee: &str) -> Vec<Stmt> {
    vec![Stmt::Let(LetStmt {
        name: "tmp".to_string(),
        mutable: false,
        pattern: None,
        ty: None,
        value: Expr::Call(
            Box::new(Expr::Ident(callee.to_string(), span())),
            vec![],
            span(),
        ),
        span: span(),
    })]
}

/// Helper: build a body with an if statement whose condition references `callee`.
fn body_with_if_call(callee: &str) -> Vec<Stmt> {
    vec![Stmt::If(IfStmt {
        condition: Expr::Call(
            Box::new(Expr::Ident(callee.to_string(), span())),
            vec![],
            span(),
        ),
        then_body: vec![],
        else_body: None,
        span: span(),
    })]
}

// ── Validation: valid definitions ──────────────────────────────────

#[test]
fn validate_option_pattern_ok() {
    let def = make_def(
        "ValidEmail",
        "input",
        "String",
        ActivePatternReturn::Option(vec!["user".into(), "domain".into()]),
    );
    assert!(validate_active_pattern(&def).is_ok());
}

#[test]
fn validate_choice_pattern_ok() {
    let def = make_def(
        "Classify",
        "value",
        "Int",
        ActivePatternReturn::Choice(vec!["Positive".into(), "Zero".into(), "Negative".into()]),
    );
    assert!(validate_active_pattern(&def).is_ok());
}

#[test]
fn validate_partial_pattern_ok() {
    let def = make_def(
        "ParseInt",
        "s",
        "String",
        ActivePatternReturn::Partial(vec!["value".into()]),
    );
    assert!(validate_active_pattern(&def).is_ok());
}

#[test]
fn validate_single_binding_ok() {
    let def = make_def(
        "IsEven",
        "n",
        "Int",
        ActivePatternReturn::Option(vec!["n".into()]),
    );
    assert!(validate_active_pattern(&def).is_ok());
}

// ── Validation: invalid definitions ────────────────────────────────

#[test]
fn validate_rejects_lowercase_name() {
    let def = make_def(
        "validEmail",
        "input",
        "String",
        ActivePatternReturn::Option(vec!["user".into()]),
    );
    let errs = validate_active_pattern(&def).unwrap_err();
    assert_eq!(errs.len(), 1);
    assert!(errs[0].contains("PascalCase"));
}

#[test]
fn validate_rejects_underscore_name() {
    let def = make_def(
        "Valid_Email",
        "input",
        "String",
        ActivePatternReturn::Option(vec!["user".into()]),
    );
    let errs = validate_active_pattern(&def).unwrap_err();
    assert!(errs[0].contains("PascalCase"));
}

#[test]
fn validate_rejects_empty_option_bindings() {
    let def = make_def(
        "Empty",
        "input",
        "String",
        ActivePatternReturn::Option(vec![]),
    );
    let errs = validate_active_pattern(&def).unwrap_err();
    assert!(errs[0].contains("at least one binding"));
}

#[test]
fn validate_rejects_empty_choice_alternatives() {
    let def = make_def(
        "Empty",
        "input",
        "String",
        ActivePatternReturn::Choice(vec![]),
    );
    let errs = validate_active_pattern(&def).unwrap_err();
    assert!(errs[0].contains("at least one alternative"));
}

#[test]
fn validate_rejects_empty_partial_bindings() {
    let def = make_def(
        "Empty",
        "input",
        "String",
        ActivePatternReturn::Partial(vec![]),
    );
    let errs = validate_active_pattern(&def).unwrap_err();
    assert!(errs[0].contains("at least one binding"));
}

#[test]
fn validate_rejects_recursive_call_in_body() {
    let mut def = make_def(
        "Recur",
        "x",
        "String",
        ActivePatternReturn::Option(vec!["a".into()]),
    );
    def.body = body_with_call("Recur");
    let errs = validate_active_pattern(&def).unwrap_err();
    assert!(errs[0].contains("recursively reference"));
}

#[test]
fn validate_rejects_recursive_in_let() {
    let mut def = make_def(
        "Recur",
        "x",
        "String",
        ActivePatternReturn::Partial(vec!["v".into()]),
    );
    def.body = body_with_let_call("Recur");
    let errs = validate_active_pattern(&def).unwrap_err();
    assert!(errs[0].contains("recursively reference"));
}

#[test]
fn validate_rejects_recursive_in_if_condition() {
    let mut def = make_def(
        "Recur",
        "x",
        "String",
        ActivePatternReturn::Option(vec!["v".into()]),
    );
    def.body = body_with_if_call("Recur");
    let errs = validate_active_pattern(&def).unwrap_err();
    assert!(errs[0].contains("recursively reference"));
}

#[test]
fn validate_multiple_errors_at_once() {
    // lowercase name + empty bindings + recursive body
    let mut def = ActivePatternDef {
        name: "bad_name".to_string(),
        param_name: "x".to_string(),
        param_type: "String".to_string(),
        return_type: ActivePatternReturn::Option(vec![]),
        body: vec![],
        span: span(),
    };
    // Add a recursive call to a body referencing "bad_name"
    def.body = body_with_call("bad_name");
    let errs = validate_active_pattern(&def).unwrap_err();
    // Should have at least two errors (PascalCase + empty bindings)
    // recursion also triggers since the body calls "bad_name"
    assert!(
        errs.len() >= 2,
        "expected at least 2 errors, got {:?}",
        errs
    );
}

// ── Resolution ─────────────────────────────────────────────────────

#[test]
fn resolve_known_pattern_returns_shape() {
    let mut registry = HashMap::new();
    let def = make_def(
        "ValidEmail",
        "input",
        "String",
        ActivePatternReturn::Option(vec!["user".into(), "domain".into()]),
    );
    registry.insert("ValidEmail".to_string(), def);

    let result = resolve_active_pattern_match("ValidEmail", &registry).unwrap();
    assert_eq!(
        result,
        ActivePatternReturn::Option(vec!["user".into(), "domain".into()])
    );
}

#[test]
fn resolve_choice_pattern() {
    let mut registry = HashMap::new();
    let def = make_def(
        "Classify",
        "n",
        "Int",
        ActivePatternReturn::Choice(vec!["Positive".into(), "Zero".into(), "Negative".into()]),
    );
    registry.insert("Classify".to_string(), def);

    let result = resolve_active_pattern_match("Classify", &registry).unwrap();
    assert_eq!(
        result,
        ActivePatternReturn::Choice(vec!["Positive".into(), "Zero".into(), "Negative".into(),])
    );
}

#[test]
fn resolve_unknown_pattern_returns_error() {
    let registry: HashMap<String, ActivePatternDef> = HashMap::new();
    let result = resolve_active_pattern_match("DoesNotExist", &registry);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown active pattern"));
}

// ── Lowering call info ─────────────────────────────────────────────

#[test]
fn lower_option_pattern_call_info() {
    let def = make_def(
        "ValidEmail",
        "input",
        "String",
        ActivePatternReturn::Option(vec!["user".into(), "domain".into()]),
    );
    let info = lower_active_pattern_call(&def);
    assert_eq!(info.fn_name, "__active_pattern_ValidEmail");
    assert_eq!(info.binding_count, 2);
    assert!(
        info.is_partial,
        "Option patterns are partial (may return null)"
    );
}

#[test]
fn lower_choice_pattern_call_info() {
    let def = make_def(
        "Classify",
        "n",
        "Int",
        ActivePatternReturn::Choice(vec!["Positive".into(), "Zero".into(), "Negative".into()]),
    );
    let info = lower_active_pattern_call(&def);
    assert_eq!(info.fn_name, "__active_pattern_Classify");
    assert_eq!(info.binding_count, 3);
    assert!(
        !info.is_partial,
        "Choice patterns are total (always match one alternative)"
    );
}

#[test]
fn lower_partial_pattern_call_info() {
    let def = make_def(
        "ParseInt",
        "s",
        "String",
        ActivePatternReturn::Partial(vec!["value".into()]),
    );
    let info = lower_active_pattern_call(&def);
    assert_eq!(info.fn_name, "__active_pattern_ParseInt");
    assert_eq!(info.binding_count, 1);
    assert!(info.is_partial, "Partial patterns are partial (may fail)");
}

// ── ActivePatternMatch struct ──────────────────────────────────────

#[test]
fn active_pattern_match_struct_fields() {
    let m = ActivePatternMatch {
        pattern_name: "ValidEmail".to_string(),
        bindings: vec!["user".into(), "domain".into()],
        span: span(),
    };
    assert_eq!(m.pattern_name, "ValidEmail");
    assert_eq!(m.bindings.len(), 2);
    assert_eq!(m.bindings[0], "user");
    assert_eq!(m.bindings[1], "domain");
}

// ── Edge cases ─────────────────────────────────────────────────────

#[test]
fn validate_non_recursive_call_ok() {
    // Body calls a different function — should be fine
    let mut def = make_def(
        "ParseHex",
        "s",
        "String",
        ActivePatternReturn::Option(vec!["value".into()]),
    );
    def.body = body_with_call("some_other_function");
    assert!(validate_active_pattern(&def).is_ok());
}

#[test]
fn lower_single_binding_option() {
    let def = make_def(
        "IsPositive",
        "n",
        "Int",
        ActivePatternReturn::Option(vec!["n".into()]),
    );
    let info = lower_active_pattern_call(&def);
    assert_eq!(info.binding_count, 1);
    assert!(info.is_partial);
}
