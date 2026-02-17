//! Proof hints and manual assertions for Lumen's verification system.
//!
//! This module implements proof hints — user-supplied annotations that help
//! the solver with difficult constraints, analogous to Dafny's `assert`,
//! `assume`, `invariant`, `decreases`, and F*'s lemma/unfold patterns.
//!
//! ## Usage
//!
//! ```lumen
//! cell binary_search(arr: List[Int], target: Int) -> Int
//!   @proof_hint "loop invariant: low <= high implies target in arr[low..high]"
//!   @assert arr_is_sorted(arr)
//!   ...
//! end
//! ```

use std::collections::HashMap;
use std::fmt;

use super::super::tokens::Span;

// ── Hint types ─────────────────────────────────────────────────────

/// A proof hint supplied by the user to guide the verification solver.
#[derive(Debug, Clone, PartialEq)]
pub enum ProofHint {
    /// User assertion the solver can assume after checking.
    Assert {
        /// The condition to assert (as a string expression).
        condition: String,
        /// Optional human-readable message.
        message: Option<String>,
        /// Source location.
        span: Span,
    },
    /// Assumption (unsound — used for exploration only).
    Assume {
        /// The condition to assume without proof.
        condition: String,
        /// Source location.
        span: Span,
    },
    /// Loop invariant that must hold at each iteration.
    LoopInvariant {
        /// The invariant condition.
        condition: String,
        /// Optional identifier for the loop this invariant applies to.
        loop_id: Option<String>,
        /// Source location.
        span: Span,
    },
    /// Termination measure (decreasing expression).
    Decreases {
        /// The expression that must decrease on each recursive call / loop iteration.
        expression: String,
        /// Source location.
        span: Span,
    },
    /// Named lemma that can be referenced elsewhere.
    Lemma {
        /// The lemma's name for reuse.
        name: String,
        /// The lemma body (proof or assertion text).
        body: String,
        /// Source location.
        span: Span,
    },
    /// Inline (unfold) a function definition for N levels.
    Unfold {
        /// The function to unfold.
        function: String,
        /// How many levels deep to inline.
        depth: u32,
        /// Source location.
        span: Span,
    },
}

impl ProofHint {
    /// Return the source span for this hint.
    pub fn span(&self) -> Span {
        match self {
            ProofHint::Assert { span, .. }
            | ProofHint::Assume { span, .. }
            | ProofHint::LoopInvariant { span, .. }
            | ProofHint::Decreases { span, .. }
            | ProofHint::Lemma { span, .. }
            | ProofHint::Unfold { span, .. } => *span,
        }
    }
}

// ── Hint effect (solver directives) ────────────────────────────────

/// The effect a proof hint has on the solver when applied.
#[derive(Debug, Clone, PartialEq)]
pub enum HintEffect {
    /// Add an assumption to the solver context.
    AddAssumption(String),
    /// Add a loop invariant, optionally scoped to a loop identifier.
    AddInvariant(String, Option<String>),
    /// Set a termination measure.
    SetTermination(String),
    /// Inline a function definition for the given depth.
    InlineFunction(String, u32),
    /// The hint has no direct solver effect (e.g. a lemma definition).
    NoEffect,
}

// ── Hint errors ────────────────────────────────────────────────────

/// Errors that can occur when validating or registering proof hints.
#[derive(Debug, Clone, PartialEq)]
pub enum HintError {
    /// A condition string was empty where it must not be.
    EmptyCondition(String),
    /// A lemma with the given name was already registered.
    DuplicateLemma(String),
    /// The unfold depth is out of the valid range.
    InvalidDepth {
        /// The requested depth.
        depth: u32,
        /// The maximum allowed depth.
        max: u32,
    },
    /// An assumption is unsound and may compromise verification.
    UnsoundAssumption(String),
    /// The hint text could not be parsed.
    ParseError(String),
}

impl fmt::Display for HintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HintError::EmptyCondition(ctx) => {
                write!(f, "empty condition in {}", ctx)
            }
            HintError::DuplicateLemma(name) => {
                write!(f, "duplicate lemma name: '{}'", name)
            }
            HintError::InvalidDepth { depth, max } => {
                write!(f, "invalid unfold depth {} (must be 1..={})", depth, max)
            }
            HintError::UnsoundAssumption(cond) => {
                write!(f, "unsound assumption: '{}' — use with caution", cond)
            }
            HintError::ParseError(msg) => {
                write!(f, "hint parse error: {}", msg)
            }
        }
    }
}

impl std::error::Error for HintError {}

// ── Hint warnings ──────────────────────────────────────────────────

/// Severity level for hint warnings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintSeverity {
    /// Informational message.
    Info,
    /// Potential issue that may affect verification.
    Warning,
    /// Serious issue that likely indicates a mistake.
    Error,
}

/// A warning produced during hint validation.
#[derive(Debug, Clone, PartialEq)]
pub struct HintWarning {
    /// Description of the warning.
    pub message: String,
    /// Index of the hint that triggered this warning.
    pub hint_index: usize,
    /// Severity level.
    pub severity: HintSeverity,
}

// ── Hint registry ──────────────────────────────────────────────────

/// Registry that collects and manages proof hints for a verification session.
#[derive(Debug, Clone)]
pub struct HintRegistry {
    /// All registered hints in insertion order.
    pub hints: Vec<ProofHint>,
    /// Named lemmas for lookup by name.
    pub lemmas: HashMap<String, ProofHint>,
    /// Warnings generated during validation.
    pub warnings: Vec<HintWarning>,
}

impl HintRegistry {
    /// Create a new, empty hint registry.
    pub fn new() -> Self {
        Self {
            hints: Vec::new(),
            lemmas: HashMap::new(),
            warnings: Vec::new(),
        }
    }

    /// Add a proof hint to the registry.
    pub fn add_hint(&mut self, hint: ProofHint) {
        self.hints.push(hint);
    }

    /// Register a named lemma. Returns an error if a lemma with that name
    /// already exists.
    pub fn add_lemma(&mut self, name: String, hint: ProofHint) -> Result<(), HintError> {
        if self.lemmas.contains_key(&name) {
            return Err(HintError::DuplicateLemma(name));
        }
        self.lemmas.insert(name, hint);
        Ok(())
    }

    /// Look up a previously registered lemma by name.
    pub fn get_lemma(&self, name: &str) -> Option<&ProofHint> {
        self.lemmas.get(name)
    }

    /// Return all hints whose span includes the given source line.
    pub fn hints_for_location(&self, line: usize) -> Vec<&ProofHint> {
        self.hints
            .iter()
            .filter(|h| h.span().line == line)
            .collect()
    }

    /// Validate the registry for common issues and return any warnings.
    ///
    /// Checks for:
    /// - Unsound `Assume` hints
    /// - Unused lemmas (registered but never referenced in hints)
    /// - Empty conditions
    pub fn validate(&self) -> Vec<HintWarning> {
        let mut warnings = Vec::new();

        for (i, hint) in self.hints.iter().enumerate() {
            // Flag all Assume hints as unsound.
            if let ProofHint::Assume { condition, .. } = hint {
                warnings.push(HintWarning {
                    message: format!(
                        "unsound assumption: '{}' — verification may be incomplete",
                        condition
                    ),
                    hint_index: i,
                    severity: HintSeverity::Warning,
                });
            }

            // Check for empty conditions in relevant variants.
            match hint {
                ProofHint::Assert { condition, .. } if condition.trim().is_empty() => {
                    warnings.push(HintWarning {
                        message: "assert hint has empty condition".to_string(),
                        hint_index: i,
                        severity: HintSeverity::Error,
                    });
                }
                ProofHint::LoopInvariant { condition, .. } if condition.trim().is_empty() => {
                    warnings.push(HintWarning {
                        message: "loop invariant has empty condition".to_string(),
                        hint_index: i,
                        severity: HintSeverity::Error,
                    });
                }
                _ => {}
            }
        }

        // Check for unused lemmas.
        for name in self.lemmas.keys() {
            let referenced = self.hints.iter().any(|h| {
                if let ProofHint::Assert { condition, .. }
                | ProofHint::Assume { condition, .. }
                | ProofHint::LoopInvariant { condition, .. } = h
                {
                    condition.contains(name.as_str())
                } else {
                    false
                }
            });
            if !referenced {
                warnings.push(HintWarning {
                    message: format!("lemma '{}' is defined but never referenced", name),
                    hint_index: 0,
                    severity: HintSeverity::Info,
                });
            }
        }

        warnings
    }
}

impl Default for HintRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Hint validation ────────────────────────────────────────────────

/// Maximum allowed unfold depth.
const MAX_UNFOLD_DEPTH: u32 = 10;

/// Validate a single proof hint, returning errors for invalid hints.
///
/// Rules:
/// - `Assert`: condition must be non-empty
/// - `Assume`: produces an `UnsoundAssumption` warning (returned as error)
/// - `LoopInvariant`: condition must be non-empty
/// - `Decreases`: expression must be non-empty
/// - `Lemma`: both name and body must be non-empty
/// - `Unfold`: depth must be in `1..=10`
pub fn validate_hint(hint: &ProofHint) -> Result<(), Vec<HintError>> {
    let mut errors = Vec::new();

    match hint {
        ProofHint::Assert { condition, .. } => {
            if condition.trim().is_empty() {
                errors.push(HintError::EmptyCondition("assert".to_string()));
            }
        }
        ProofHint::Assume { condition, .. } => {
            errors.push(HintError::UnsoundAssumption(condition.clone()));
        }
        ProofHint::LoopInvariant { condition, .. } => {
            if condition.trim().is_empty() {
                errors.push(HintError::EmptyCondition("loop invariant".to_string()));
            }
        }
        ProofHint::Decreases { expression, .. } => {
            if expression.trim().is_empty() {
                errors.push(HintError::EmptyCondition("decreases".to_string()));
            }
        }
        ProofHint::Lemma { name, body, .. } => {
            if name.trim().is_empty() {
                errors.push(HintError::EmptyCondition("lemma name".to_string()));
            }
            if body.trim().is_empty() {
                errors.push(HintError::EmptyCondition("lemma body".to_string()));
            }
        }
        ProofHint::Unfold { depth, .. } => {
            if *depth == 0 || *depth > MAX_UNFOLD_DEPTH {
                errors.push(HintError::InvalidDepth {
                    depth: *depth,
                    max: MAX_UNFOLD_DEPTH,
                });
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// ── Hint application ───────────────────────────────────────────────

/// Translate a proof hint into a solver directive.
///
/// This maps each hint variant to the corresponding `HintEffect` that the
/// solver should apply:
/// - `Assert` / `Assume` → `AddAssumption`
/// - `LoopInvariant` → `AddInvariant`
/// - `Decreases` → `SetTermination`
/// - `Unfold` → `InlineFunction`
/// - `Lemma` → `NoEffect` (lemmas are stored for reference, not directly applied)
pub fn apply_hint(hint: &ProofHint) -> HintEffect {
    match hint {
        ProofHint::Assert { condition, .. } => HintEffect::AddAssumption(condition.clone()),
        ProofHint::Assume { condition, .. } => HintEffect::AddAssumption(condition.clone()),
        ProofHint::LoopInvariant {
            condition, loop_id, ..
        } => HintEffect::AddInvariant(condition.clone(), loop_id.clone()),
        ProofHint::Decreases { expression, .. } => HintEffect::SetTermination(expression.clone()),
        ProofHint::Lemma { .. } => HintEffect::NoEffect,
        ProofHint::Unfold {
            function, depth, ..
        } => HintEffect::InlineFunction(function.clone(), *depth),
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_span() -> Span {
        Span {
            start: 0,
            end: 0,
            line: 1,
            col: 1,
        }
    }

    #[test]
    fn proof_hint_assert_construction() {
        let hint = ProofHint::Assert {
            condition: "x > 0".to_string(),
            message: Some("x must be positive".to_string()),
            span: dummy_span(),
        };
        assert!(matches!(hint, ProofHint::Assert { .. }));
    }

    #[test]
    fn proof_hint_assume_construction() {
        let hint = ProofHint::Assume {
            condition: "x > 0".to_string(),
            span: dummy_span(),
        };
        assert!(matches!(hint, ProofHint::Assume { .. }));
    }

    #[test]
    fn proof_hint_span_accessor() {
        let span = Span {
            start: 10,
            end: 20,
            line: 5,
            col: 3,
        };
        let hint = ProofHint::Assert {
            condition: "x > 0".to_string(),
            message: None,
            span,
        };
        assert_eq!(hint.span().line, 5);
        assert_eq!(hint.span().col, 3);
    }

    #[test]
    fn validate_valid_assert() {
        let hint = ProofHint::Assert {
            condition: "x > 0".to_string(),
            message: None,
            span: dummy_span(),
        };
        assert!(validate_hint(&hint).is_ok());
    }

    #[test]
    fn validate_empty_assert_condition() {
        let hint = ProofHint::Assert {
            condition: "".to_string(),
            message: None,
            span: dummy_span(),
        };
        let errs = validate_hint(&hint).unwrap_err();
        assert!(errs
            .iter()
            .any(|e| matches!(e, HintError::EmptyCondition(_))));
    }

    #[test]
    fn validate_assume_is_unsound() {
        let hint = ProofHint::Assume {
            condition: "x > 0".to_string(),
            span: dummy_span(),
        };
        let errs = validate_hint(&hint).unwrap_err();
        assert!(errs
            .iter()
            .any(|e| matches!(e, HintError::UnsoundAssumption(_))));
    }

    #[test]
    fn validate_unfold_depth_zero() {
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
    fn validate_unfold_depth_exceeds_max() {
        let hint = ProofHint::Unfold {
            function: "foo".to_string(),
            depth: 11,
            span: dummy_span(),
        };
        let errs = validate_hint(&hint).unwrap_err();
        assert!(errs
            .iter()
            .any(|e| matches!(e, HintError::InvalidDepth { depth: 11, .. })));
    }

    #[test]
    fn validate_unfold_valid_depth() {
        let hint = ProofHint::Unfold {
            function: "foo".to_string(),
            depth: 5,
            span: dummy_span(),
        };
        assert!(validate_hint(&hint).is_ok());
    }

    #[test]
    fn apply_assert_hint() {
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
    fn apply_assume_hint() {
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
    fn apply_loop_invariant_hint() {
        let hint = ProofHint::LoopInvariant {
            condition: "i < n".to_string(),
            loop_id: Some("main_loop".to_string()),
            span: dummy_span(),
        };
        assert_eq!(
            apply_hint(&hint),
            HintEffect::AddInvariant("i < n".to_string(), Some("main_loop".to_string()))
        );
    }

    #[test]
    fn apply_decreases_hint() {
        let hint = ProofHint::Decreases {
            expression: "n - i".to_string(),
            span: dummy_span(),
        };
        assert_eq!(
            apply_hint(&hint),
            HintEffect::SetTermination("n - i".to_string())
        );
    }

    #[test]
    fn apply_lemma_hint() {
        let hint = ProofHint::Lemma {
            name: "sorted_implies_bounded".to_string(),
            body: "forall i. arr[i] <= arr[i+1]".to_string(),
            span: dummy_span(),
        };
        assert_eq!(apply_hint(&hint), HintEffect::NoEffect);
    }

    #[test]
    fn apply_unfold_hint() {
        let hint = ProofHint::Unfold {
            function: "factorial".to_string(),
            depth: 3,
            span: dummy_span(),
        };
        assert_eq!(
            apply_hint(&hint),
            HintEffect::InlineFunction("factorial".to_string(), 3)
        );
    }
}
