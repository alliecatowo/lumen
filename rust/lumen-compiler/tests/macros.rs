//! Wave 21 — T118: Hygienic Macro System tests
//!
//! Exercises the macro module:
//! - MacroRegistry: creation, registration, lookup, scope allocation
//! - Name mangling: format, uniqueness across scopes
//! - Expansion: parameter substitution, hygienic name introduction, error cases
//! - Validation: param names, body references, recursive detection
//! - MacroError: Display formatting for all variants

use lumen_compiler::compiler::macros::*;
use lumen_compiler::compiler::tokens::Span;

fn span() -> Span {
    Span::dummy()
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Build a simple `assert_eq!` macro definition used by many tests.
fn make_assert_eq_def() -> MacroDef {
    MacroDef {
        name: "assert_eq".into(),
        params: vec!["expected".into(), "actual".into()],
        body_template: vec![
            MacroBodyItem::Literal("let ".into()),
            MacroBodyItem::ScopeIntro("result".into()),
            MacroBodyItem::Literal(" = (".into()),
            MacroBodyItem::ParamRef("expected".into()),
            MacroBodyItem::Literal(") == (".into()),
            MacroBodyItem::ParamRef("actual".into()),
            MacroBodyItem::Literal(")".into()),
        ],
        span: span(),
    }
}

/// Build a trivial single-param macro: `log!(msg)` -> `print(msg)`.
fn make_log_def() -> MacroDef {
    MacroDef {
        name: "log".into(),
        params: vec!["msg".into()],
        body_template: vec![
            MacroBodyItem::Literal("print(".into()),
            MacroBodyItem::ParamRef("msg".into()),
            MacroBodyItem::Literal(")".into()),
        ],
        span: span(),
    }
}

// ── 1. Registry basics ───────────────────────────────────────────────

#[test]
fn registry_starts_empty() {
    let reg = MacroRegistry::new();
    assert!(reg.macros.is_empty());
    assert_eq!(reg.next_scope_id, 1);
}

#[test]
fn register_and_lookup_round_trip() {
    let mut reg = MacroRegistry::new();
    let def = make_assert_eq_def();
    reg.register("assert_eq".into(), def).unwrap();
    let looked = reg.lookup("assert_eq");
    assert!(looked.is_some());
    assert_eq!(looked.unwrap().name, "assert_eq");
    assert_eq!(looked.unwrap().params.len(), 2);
}

#[test]
fn register_rejects_duplicate() {
    let mut reg = MacroRegistry::new();
    reg.register("dup".into(), make_log_def()).unwrap();
    let err = reg.register("dup".into(), make_log_def()).unwrap_err();
    assert_eq!(err, MacroError::AlreadyDefined("dup".into()));
}

#[test]
fn lookup_unknown_returns_none() {
    let reg = MacroRegistry::new();
    assert!(reg.lookup("no_such_macro").is_none());
}

#[test]
fn register_multiple_macros() {
    let mut reg = MacroRegistry::new();
    reg.register("assert_eq".into(), make_assert_eq_def())
        .unwrap();
    reg.register("log".into(), make_log_def()).unwrap();
    assert_eq!(reg.macros.len(), 2);
    assert!(reg.lookup("assert_eq").is_some());
    assert!(reg.lookup("log").is_some());
}

// ── 2. Scope allocation ─────────────────────────────────────────────

#[test]
fn new_scope_increments_ids() {
    let mut reg = MacroRegistry::new();
    let s1 = reg.new_scope();
    let s2 = reg.new_scope();
    let s3 = reg.new_scope();
    assert_eq!(s1.id, 1);
    assert_eq!(s2.id, 2);
    assert_eq!(s3.id, 3);
}

#[test]
fn scope_has_no_parent_by_default() {
    let mut reg = MacroRegistry::new();
    let s = reg.new_scope();
    assert!(s.parent.is_none());
}

// ── 3. Name mangling ────────────────────────────────────────────────

#[test]
fn mangle_name_correct_format() {
    let h = mangle_name("result", 5);
    assert_eq!(h.original, "result");
    assert_eq!(h.scope_id, 5);
    assert_eq!(h.mangled, "__hyg_5_result");
}

#[test]
fn mangle_name_different_scopes_produce_unique_names() {
    let h1 = mangle_name("x", 1);
    let h2 = mangle_name("x", 2);
    assert_ne!(h1.mangled, h2.mangled);
    assert_eq!(h1.original, h2.original);
}

#[test]
fn mangle_name_different_names_same_scope() {
    let h1 = mangle_name("a", 1);
    let h2 = mangle_name("b", 1);
    assert_ne!(h1.mangled, h2.mangled);
    assert_eq!(h1.scope_id, h2.scope_id);
}

// ── 4. Expansion ────────────────────────────────────────────────────

#[test]
fn expand_simple_substitution() {
    let mut reg = MacroRegistry::new();
    reg.register("log".into(), make_log_def()).unwrap();

    let result = expand_macro(&mut reg, "log", &["\"hello\"".into()]).unwrap();
    assert_eq!(result.fragments.len(), 3);
    assert_eq!(result.fragments[0], ExpandedFragment::Text("print(".into()));
    assert_eq!(
        result.fragments[1],
        ExpandedFragment::Substituted {
            param: "msg".into(),
            arg: "\"hello\"".into()
        }
    );
    assert_eq!(result.fragments[2], ExpandedFragment::Text(")".into()));
    assert!(result.introduced_names.is_empty());
}

#[test]
fn expand_with_hygienic_name_introduction() {
    let mut reg = MacroRegistry::new();
    reg.register("assert_eq".into(), make_assert_eq_def())
        .unwrap();

    let result = expand_macro(&mut reg, "assert_eq", &["42".into(), "x".into()]).unwrap();

    // Should have introduced one hygienic name: "result"
    assert_eq!(result.introduced_names.len(), 1);
    assert_eq!(result.introduced_names[0].original, "result");
    assert_eq!(
        result.introduced_names[0].mangled,
        format!("__hyg_{}_result", result.scope.id)
    );

    // Check the fragments contain the hygienic binding
    let has_hygienic = result
        .fragments
        .iter()
        .any(|f| matches!(f, ExpandedFragment::HygienicBinding(h) if h.original == "result"));
    assert!(has_hygienic);
}

#[test]
fn expand_rejects_unknown_macro() {
    let mut reg = MacroRegistry::new();
    let err = expand_macro(&mut reg, "nonexistent", &[]).unwrap_err();
    assert_eq!(err, MacroError::Undefined("nonexistent".into()));
}

#[test]
fn expand_rejects_arg_count_mismatch_too_few() {
    let mut reg = MacroRegistry::new();
    reg.register("assert_eq".into(), make_assert_eq_def())
        .unwrap();

    let err = expand_macro(&mut reg, "assert_eq", &["42".into()]).unwrap_err();
    assert_eq!(
        err,
        MacroError::ArgCountMismatch {
            expected: 2,
            actual: 1
        }
    );
}

#[test]
fn expand_rejects_arg_count_mismatch_too_many() {
    let mut reg = MacroRegistry::new();
    reg.register("log".into(), make_log_def()).unwrap();

    let err = expand_macro(&mut reg, "log", &["a".into(), "b".into()]).unwrap_err();
    assert_eq!(
        err,
        MacroError::ArgCountMismatch {
            expected: 1,
            actual: 2
        }
    );
}

#[test]
fn expand_zero_arg_macro() {
    let mut reg = MacroRegistry::new();
    let def = MacroDef {
        name: "noop".into(),
        params: vec![],
        body_template: vec![MacroBodyItem::Literal("pass".into())],
        span: span(),
    };
    reg.register("noop".into(), def).unwrap();

    let result = expand_macro(&mut reg, "noop", &[]).unwrap();
    assert_eq!(result.fragments.len(), 1);
    assert_eq!(result.fragments[0], ExpandedFragment::Text("pass".into()));
}

#[test]
fn multiple_expansions_get_unique_scope_ids() {
    let mut reg = MacroRegistry::new();
    reg.register("log".into(), make_log_def()).unwrap();

    let r1 = expand_macro(&mut reg, "log", &["a".into()]).unwrap();
    let r2 = expand_macro(&mut reg, "log", &["b".into()]).unwrap();
    let r3 = expand_macro(&mut reg, "log", &["c".into()]).unwrap();

    assert_ne!(r1.scope.id, r2.scope.id);
    assert_ne!(r2.scope.id, r3.scope.id);
    assert_ne!(r1.scope.id, r3.scope.id);
}

#[test]
fn hygienic_names_differ_across_expansions() {
    let mut reg = MacroRegistry::new();
    reg.register("assert_eq".into(), make_assert_eq_def())
        .unwrap();

    let r1 = expand_macro(&mut reg, "assert_eq", &["1".into(), "2".into()]).unwrap();
    let r2 = expand_macro(&mut reg, "assert_eq", &["3".into(), "4".into()]).unwrap();

    assert_eq!(r1.introduced_names.len(), 1);
    assert_eq!(r2.introduced_names.len(), 1);
    assert_ne!(
        r1.introduced_names[0].mangled,
        r2.introduced_names[0].mangled
    );
}

// ── 5. Validation ───────────────────────────────────────────────────

#[test]
fn validate_accepts_valid_macro() {
    let def = make_assert_eq_def();
    assert!(validate_macro_def(&def).is_ok());
}

#[test]
fn validate_accepts_zero_param_macro() {
    let def = MacroDef {
        name: "noop".into(),
        params: vec![],
        body_template: vec![MacroBodyItem::Literal("pass".into())],
        span: span(),
    };
    assert!(validate_macro_def(&def).is_ok());
}

#[test]
fn validate_rejects_unknown_param_ref() {
    let def = MacroDef {
        name: "bad".into(),
        params: vec!["a".into()],
        body_template: vec![
            MacroBodyItem::ParamRef("a".into()),
            MacroBodyItem::ParamRef("b".into()), // not declared
        ],
        span: span(),
    };
    let errs = validate_macro_def(&def).unwrap_err();
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0], MacroError::InvalidParam("b".into()));
}

#[test]
fn validate_rejects_recursive_expansion() {
    let def = MacroDef {
        name: "recurse".into(),
        params: vec!["x".into()],
        body_template: vec![
            MacroBodyItem::Literal("recurse!(".into()),
            MacroBodyItem::ParamRef("x".into()),
            MacroBodyItem::Literal(")".into()),
        ],
        span: span(),
    };
    let errs = validate_macro_def(&def).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, MacroError::RecursiveExpansion(n) if n == "recurse")));
}

#[test]
fn validate_rejects_empty_param_name() {
    let def = MacroDef {
        name: "bad".into(),
        params: vec!["".into()],
        body_template: vec![],
        span: span(),
    };
    let errs = validate_macro_def(&def).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, MacroError::InvalidParam(n) if n.is_empty())));
}

#[test]
fn validate_rejects_param_with_special_chars() {
    let def = MacroDef {
        name: "bad".into(),
        params: vec!["a-b".into()],
        body_template: vec![],
        span: span(),
    };
    let errs = validate_macro_def(&def).unwrap_err();
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0], MacroError::InvalidParam("a-b".into()));
}

#[test]
fn validate_collects_multiple_errors() {
    let def = MacroDef {
        name: "multi_bad".into(),
        params: vec!["".into()], // bad param
        body_template: vec![
            MacroBodyItem::ParamRef("unknown".into()), // unknown ref
            MacroBodyItem::Literal("multi_bad!(foo)".into()), // recursive
        ],
        span: span(),
    };
    let errs = validate_macro_def(&def).unwrap_err();
    assert!(
        errs.len() >= 3,
        "expected at least 3 errors, got {}",
        errs.len()
    );
}

// ── 6. MacroError Display ───────────────────────────────────────────

#[test]
fn error_display_already_defined() {
    let e = MacroError::AlreadyDefined("foo".into());
    assert_eq!(e.to_string(), "macro 'foo' is already defined");
}

#[test]
fn error_display_undefined() {
    let e = MacroError::Undefined("bar".into());
    assert_eq!(e.to_string(), "undefined macro 'bar'");
}

#[test]
fn error_display_arg_count_mismatch() {
    let e = MacroError::ArgCountMismatch {
        expected: 2,
        actual: 0,
    };
    assert_eq!(
        e.to_string(),
        "macro expects 2 argument(s) but 0 were provided"
    );
}

#[test]
fn error_display_invalid_param() {
    let e = MacroError::InvalidParam("$$bad".into());
    assert_eq!(e.to_string(), "invalid parameter reference '$$bad'");
}

#[test]
fn error_display_recursive_expansion() {
    let e = MacroError::RecursiveExpansion("self_ref".into());
    assert_eq!(
        e.to_string(),
        "recursive expansion detected in macro 'self_ref'"
    );
}

// ── 7. Edge cases ───────────────────────────────────────────────────

#[test]
fn expand_multiple_scope_intros() {
    let mut reg = MacroRegistry::new();
    let def = MacroDef {
        name: "swap".into(),
        params: vec!["a".into(), "b".into()],
        body_template: vec![
            MacroBodyItem::Literal("let ".into()),
            MacroBodyItem::ScopeIntro("tmp".into()),
            MacroBodyItem::Literal(" = ".into()),
            MacroBodyItem::ParamRef("a".into()),
            MacroBodyItem::Literal("; ".into()),
            MacroBodyItem::ScopeIntro("tmp2".into()),
            MacroBodyItem::Literal(" = ".into()),
            MacroBodyItem::ParamRef("b".into()),
        ],
        span: span(),
    };
    reg.register("swap".into(), def).unwrap();

    let result = expand_macro(&mut reg, "swap", &["x".into(), "y".into()]).unwrap();
    assert_eq!(result.introduced_names.len(), 2);
    assert_eq!(result.introduced_names[0].original, "tmp");
    assert_eq!(result.introduced_names[1].original, "tmp2");
    assert_ne!(
        result.introduced_names[0].mangled,
        result.introduced_names[1].mangled
    );
}

#[test]
fn default_impl_works() {
    let reg = MacroRegistry::default();
    assert!(reg.macros.is_empty());
}

#[test]
fn macro_error_is_std_error() {
    // Ensure MacroError implements std::error::Error by using it as
    // a trait object.
    let e: Box<dyn std::error::Error> = Box::new(MacroError::Undefined("test".into()));
    assert!(!e.to_string().is_empty());
}

#[test]
fn scope_intro_in_body_only_no_params() {
    // A macro with no params but introducing hygienic names
    let def = MacroDef {
        name: "counter".into(),
        params: vec![],
        body_template: vec![
            MacroBodyItem::Literal("let ".into()),
            MacroBodyItem::ScopeIntro("count".into()),
            MacroBodyItem::Literal(" = 0".into()),
        ],
        span: span(),
    };
    assert!(validate_macro_def(&def).is_ok());

    let mut reg = MacroRegistry::new();
    reg.register("counter".into(), def).unwrap();
    let result = expand_macro(&mut reg, "counter", &[]).unwrap();
    assert_eq!(result.introduced_names.len(), 1);
    assert_eq!(result.introduced_names[0].original, "count");
}

#[test]
fn expansion_scope_matches_introduced_names_scope() {
    let mut reg = MacroRegistry::new();
    reg.register("assert_eq".into(), make_assert_eq_def())
        .unwrap();

    let result = expand_macro(&mut reg, "assert_eq", &["1".into(), "2".into()]).unwrap();
    for name in &result.introduced_names {
        assert_eq!(name.scope_id, result.scope.id);
    }
}
