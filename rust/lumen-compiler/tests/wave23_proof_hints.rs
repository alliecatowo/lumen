//! Comprehensive tests for proof hints and manual assertions (T150).

use lumen_compiler::compiler::tokens::Span;
use lumen_compiler::compiler::verification::proof_hints::*;

// ── Helper ─────────────────────────────────────────────────────────

fn span_at(line: usize) -> Span {
    Span {
        start: 0,
        end: 0,
        line,
        col: 1,
    }
}

fn dummy_span() -> Span {
    span_at(1)
}

// ── ProofHint construction ─────────────────────────────────────────

#[test]
fn proof_hint_assert_construction() {
    let hint = ProofHint::Assert {
        condition: "x > 0".to_string(),
        message: Some("must be positive".to_string()),
        span: dummy_span(),
    };
    if let ProofHint::Assert {
        condition, message, ..
    } = &hint
    {
        assert_eq!(condition, "x > 0");
        assert_eq!(message.as_deref(), Some("must be positive"));
    } else {
        panic!("expected Assert variant");
    }
}

#[test]
fn proof_hint_assume_construction() {
    let hint = ProofHint::Assume {
        condition: "x < 100".to_string(),
        span: dummy_span(),
    };
    if let ProofHint::Assume { condition, .. } = &hint {
        assert_eq!(condition, "x < 100");
    } else {
        panic!("expected Assume variant");
    }
}

#[test]
fn proof_hint_loop_invariant_construction() {
    let hint = ProofHint::LoopInvariant {
        condition: "low <= high".to_string(),
        loop_id: Some("search_loop".to_string()),
        span: dummy_span(),
    };
    if let ProofHint::LoopInvariant {
        condition, loop_id, ..
    } = &hint
    {
        assert_eq!(condition, "low <= high");
        assert_eq!(loop_id.as_deref(), Some("search_loop"));
    } else {
        panic!("expected LoopInvariant variant");
    }
}

#[test]
fn proof_hint_decreases_construction() {
    let hint = ProofHint::Decreases {
        expression: "n - i".to_string(),
        span: dummy_span(),
    };
    if let ProofHint::Decreases { expression, .. } = &hint {
        assert_eq!(expression, "n - i");
    } else {
        panic!("expected Decreases variant");
    }
}

#[test]
fn proof_hint_lemma_construction() {
    let hint = ProofHint::Lemma {
        name: "sorted_lemma".to_string(),
        body: "forall i. arr[i] <= arr[i+1]".to_string(),
        span: dummy_span(),
    };
    if let ProofHint::Lemma { name, body, .. } = &hint {
        assert_eq!(name, "sorted_lemma");
        assert_eq!(body, "forall i. arr[i] <= arr[i+1]");
    } else {
        panic!("expected Lemma variant");
    }
}

#[test]
fn proof_hint_unfold_construction() {
    let hint = ProofHint::Unfold {
        function: "factorial".to_string(),
        depth: 3,
        span: dummy_span(),
    };
    if let ProofHint::Unfold {
        function, depth, ..
    } = &hint
    {
        assert_eq!(function, "factorial");
        assert_eq!(*depth, 3);
    } else {
        panic!("expected Unfold variant");
    }
}

// ── HintRegistry ───────────────────────────────────────────────────

#[test]
fn proof_hint_registry_new_is_empty() {
    let reg = HintRegistry::new();
    assert!(reg.hints.is_empty());
    assert!(reg.lemmas.is_empty());
    assert!(reg.warnings.is_empty());
}

#[test]
fn proof_hint_registry_add_and_retrieve() {
    let mut reg = HintRegistry::new();
    reg.add_hint(ProofHint::Assert {
        condition: "x > 0".to_string(),
        message: None,
        span: dummy_span(),
    });
    reg.add_hint(ProofHint::Assume {
        condition: "y < 100".to_string(),
        span: dummy_span(),
    });
    assert_eq!(reg.hints.len(), 2);
}

#[test]
fn proof_hint_registry_add_lemma_and_get() {
    let mut reg = HintRegistry::new();
    let lemma = ProofHint::Lemma {
        name: "my_lemma".to_string(),
        body: "x + y == y + x".to_string(),
        span: dummy_span(),
    };
    assert!(reg.add_lemma("my_lemma".to_string(), lemma).is_ok());
    let retrieved = reg.get_lemma("my_lemma");
    assert!(retrieved.is_some());
    if let Some(ProofHint::Lemma { name, .. }) = retrieved {
        assert_eq!(name, "my_lemma");
    }
}

#[test]
fn proof_hint_registry_duplicate_lemma_error() {
    let mut reg = HintRegistry::new();
    let lemma1 = ProofHint::Lemma {
        name: "dup".to_string(),
        body: "body1".to_string(),
        span: dummy_span(),
    };
    let lemma2 = ProofHint::Lemma {
        name: "dup".to_string(),
        body: "body2".to_string(),
        span: dummy_span(),
    };
    assert!(reg.add_lemma("dup".to_string(), lemma1).is_ok());
    let err = reg.add_lemma("dup".to_string(), lemma2).unwrap_err();
    assert!(matches!(err, HintError::DuplicateLemma(ref n) if n == "dup"));
}

#[test]
fn proof_hint_registry_get_lemma_missing() {
    let reg = HintRegistry::new();
    assert!(reg.get_lemma("nonexistent").is_none());
}

#[test]
fn proof_hint_registry_hints_for_location() {
    let mut reg = HintRegistry::new();
    reg.add_hint(ProofHint::Assert {
        condition: "a > 0".to_string(),
        message: None,
        span: span_at(10),
    });
    reg.add_hint(ProofHint::Assert {
        condition: "b > 0".to_string(),
        message: None,
        span: span_at(20),
    });
    reg.add_hint(ProofHint::Assume {
        condition: "c > 0".to_string(),
        span: span_at(10),
    });

    let at_10 = reg.hints_for_location(10);
    assert_eq!(at_10.len(), 2);

    let at_20 = reg.hints_for_location(20);
    assert_eq!(at_20.len(), 1);

    let at_99 = reg.hints_for_location(99);
    assert!(at_99.is_empty());
}

#[test]
fn proof_hint_registry_validate_detects_assume() {
    let mut reg = HintRegistry::new();
    reg.add_hint(ProofHint::Assume {
        condition: "x > 0".to_string(),
        span: dummy_span(),
    });
    let warnings = reg.validate();
    assert!(warnings
        .iter()
        .any(|w| w.severity == HintSeverity::Warning && w.message.contains("unsound")));
}

#[test]
fn proof_hint_registry_validate_detects_empty_condition() {
    let mut reg = HintRegistry::new();
    reg.add_hint(ProofHint::Assert {
        condition: "".to_string(),
        message: None,
        span: dummy_span(),
    });
    let warnings = reg.validate();
    assert!(warnings
        .iter()
        .any(|w| w.severity == HintSeverity::Error && w.message.contains("empty")));
}

#[test]
fn proof_hint_registry_validate_detects_unused_lemma() {
    let mut reg = HintRegistry::new();
    let lemma = ProofHint::Lemma {
        name: "unused_lemma".to_string(),
        body: "some body".to_string(),
        span: dummy_span(),
    };
    reg.add_lemma("unused_lemma".to_string(), lemma).unwrap();
    // No hints reference "unused_lemma"
    let warnings = reg.validate();
    assert!(warnings
        .iter()
        .any(|w| w.severity == HintSeverity::Info && w.message.contains("unused_lemma")));
}

// ── validate_hint ──────────────────────────────────────────────────

#[test]
fn proof_hint_validate_valid_assert() {
    let hint = ProofHint::Assert {
        condition: "n >= 0".to_string(),
        message: None,
        span: dummy_span(),
    };
    assert!(validate_hint(&hint).is_ok());
}

#[test]
fn proof_hint_validate_empty_condition_assert() {
    let hint = ProofHint::Assert {
        condition: "   ".to_string(),
        message: None,
        span: dummy_span(),
    };
    let errs = validate_hint(&hint).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, HintError::EmptyCondition(_))));
}

#[test]
fn proof_hint_validate_assume_warns_unsound() {
    let hint = ProofHint::Assume {
        condition: "trusted_input".to_string(),
        span: dummy_span(),
    };
    let errs = validate_hint(&hint).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, HintError::UnsoundAssumption(_))));
}

#[test]
fn proof_hint_validate_empty_loop_invariant() {
    let hint = ProofHint::LoopInvariant {
        condition: "".to_string(),
        loop_id: None,
        span: dummy_span(),
    };
    let errs = validate_hint(&hint).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, HintError::EmptyCondition(_))));
}

#[test]
fn proof_hint_validate_valid_loop_invariant() {
    let hint = ProofHint::LoopInvariant {
        condition: "i < n".to_string(),
        loop_id: None,
        span: dummy_span(),
    };
    assert!(validate_hint(&hint).is_ok());
}

#[test]
fn proof_hint_validate_empty_decreases() {
    let hint = ProofHint::Decreases {
        expression: "".to_string(),
        span: dummy_span(),
    };
    let errs = validate_hint(&hint).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, HintError::EmptyCondition(_))));
}

#[test]
fn proof_hint_validate_valid_decreases() {
    let hint = ProofHint::Decreases {
        expression: "n".to_string(),
        span: dummy_span(),
    };
    assert!(validate_hint(&hint).is_ok());
}

#[test]
fn proof_hint_validate_lemma_empty_name() {
    let hint = ProofHint::Lemma {
        name: "".to_string(),
        body: "some body".to_string(),
        span: dummy_span(),
    };
    let errs = validate_hint(&hint).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, HintError::EmptyCondition(ref s) if s.contains("name"))));
}

#[test]
fn proof_hint_validate_lemma_empty_body() {
    let hint = ProofHint::Lemma {
        name: "my_lemma".to_string(),
        body: "".to_string(),
        span: dummy_span(),
    };
    let errs = validate_hint(&hint).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, HintError::EmptyCondition(ref s) if s.contains("body"))));
}

#[test]
fn proof_hint_validate_unfold_depth_zero() {
    let hint = ProofHint::Unfold {
        function: "foo".to_string(),
        depth: 0,
        span: dummy_span(),
    };
    let errs = validate_hint(&hint).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, HintError::InvalidDepth { depth: 0, max: 10 })));
}

#[test]
fn proof_hint_validate_unfold_depth_exceeds_max() {
    let hint = ProofHint::Unfold {
        function: "foo".to_string(),
        depth: 11,
        span: dummy_span(),
    };
    let errs = validate_hint(&hint).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, HintError::InvalidDepth { depth: 11, max: 10 })));
}

#[test]
fn proof_hint_validate_unfold_depth_boundary_valid() {
    // depth=1 and depth=10 are both valid
    let hint1 = ProofHint::Unfold {
        function: "foo".to_string(),
        depth: 1,
        span: dummy_span(),
    };
    let hint10 = ProofHint::Unfold {
        function: "bar".to_string(),
        depth: 10,
        span: dummy_span(),
    };
    assert!(validate_hint(&hint1).is_ok());
    assert!(validate_hint(&hint10).is_ok());
}

// ── apply_hint ─────────────────────────────────────────────────────

#[test]
fn proof_hint_apply_assert() {
    let hint = ProofHint::Assert {
        condition: "x > 0".to_string(),
        message: None,
        span: dummy_span(),
    };
    assert_eq!(
        apply_hint(&hint),
        HintEffect::AddAssumption("x > 0".to_string())
    );
}

#[test]
fn proof_hint_apply_assume() {
    let hint = ProofHint::Assume {
        condition: "y < 100".to_string(),
        span: dummy_span(),
    };
    assert_eq!(
        apply_hint(&hint),
        HintEffect::AddAssumption("y < 100".to_string())
    );
}

#[test]
fn proof_hint_apply_loop_invariant() {
    let hint = ProofHint::LoopInvariant {
        condition: "i >= 0".to_string(),
        loop_id: Some("loop1".to_string()),
        span: dummy_span(),
    };
    assert_eq!(
        apply_hint(&hint),
        HintEffect::AddInvariant("i >= 0".to_string(), Some("loop1".to_string()))
    );
}

#[test]
fn proof_hint_apply_loop_invariant_no_id() {
    let hint = ProofHint::LoopInvariant {
        condition: "i >= 0".to_string(),
        loop_id: None,
        span: dummy_span(),
    };
    assert_eq!(
        apply_hint(&hint),
        HintEffect::AddInvariant("i >= 0".to_string(), None)
    );
}

#[test]
fn proof_hint_apply_decreases() {
    let hint = ProofHint::Decreases {
        expression: "high - low".to_string(),
        span: dummy_span(),
    };
    assert_eq!(
        apply_hint(&hint),
        HintEffect::SetTermination("high - low".to_string())
    );
}

#[test]
fn proof_hint_apply_lemma() {
    let hint = ProofHint::Lemma {
        name: "commutativity".to_string(),
        body: "x + y == y + x".to_string(),
        span: dummy_span(),
    };
    assert_eq!(apply_hint(&hint), HintEffect::NoEffect);
}

#[test]
fn proof_hint_apply_unfold() {
    let hint = ProofHint::Unfold {
        function: "fib".to_string(),
        depth: 2,
        span: dummy_span(),
    };
    assert_eq!(
        apply_hint(&hint),
        HintEffect::InlineFunction("fib".to_string(), 2)
    );
}

// ── HintError Display ──────────────────────────────────────────────

#[test]
fn proof_hint_error_display_empty_condition() {
    let err = HintError::EmptyCondition("assert".to_string());
    let msg = format!("{}", err);
    assert!(msg.contains("empty condition"));
    assert!(msg.contains("assert"));
}

#[test]
fn proof_hint_error_display_duplicate_lemma() {
    let err = HintError::DuplicateLemma("my_lemma".to_string());
    let msg = format!("{}", err);
    assert!(msg.contains("duplicate"));
    assert!(msg.contains("my_lemma"));
}

#[test]
fn proof_hint_error_display_invalid_depth() {
    let err = HintError::InvalidDepth { depth: 15, max: 10 };
    let msg = format!("{}", err);
    assert!(msg.contains("15"));
    assert!(msg.contains("10"));
}

#[test]
fn proof_hint_error_display_unsound() {
    let err = HintError::UnsoundAssumption("x > 0".to_string());
    let msg = format!("{}", err);
    assert!(msg.contains("unsound"));
    assert!(msg.contains("x > 0"));
}

#[test]
fn proof_hint_error_display_parse_error() {
    let err = HintError::ParseError("unexpected token".to_string());
    let msg = format!("{}", err);
    assert!(msg.contains("parse error"));
    assert!(msg.contains("unexpected token"));
}

// ── HintSeverity ───────────────────────────────────────────────────

#[test]
fn proof_hint_severity_variants() {
    // Ensure all variants are distinct and constructible.
    let info = HintSeverity::Info;
    let warn = HintSeverity::Warning;
    let err = HintSeverity::Error;
    assert_ne!(info, warn);
    assert_ne!(warn, err);
    assert_ne!(info, err);
}

// ── HintRegistry default trait ─────────────────────────────────────

#[test]
fn proof_hint_registry_default() {
    let reg = HintRegistry::default();
    assert!(reg.hints.is_empty());
    assert!(reg.lemmas.is_empty());
}
