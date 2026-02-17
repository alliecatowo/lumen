//! Probabilistic type support for the Lumen compiler.
//!
//! This module defines the `Prob[T]` probabilistic type, which represents a
//! probability distribution over values of type `T`. It enables Bayesian
//! reasoning in agent programs:
//!
//! ```text
//! let coin: Prob[Bool] = bernoulli(0.5)
//! let die: Prob[Int] = uniform(1, 6)
//! let temp: Prob[Float] = normal(98.6, 0.5)
//!
//! # Operations on distributions
//! let sum: Prob[Int] = die + die           # Convolution
//! let cond: Prob[Float] = given(temp, temp > 99.0)  # Conditioning
//! let sampled: Int = sample(die)           # Draw a concrete value
//! let expected: Float = expectation(die)   # E[X]
//! let p: Float = probability(die > 3)     # P(X > 3)
//! ```
//!
//! ## Integration
//!
//! This module is **not yet wired** into the main `compile()` pipeline.
//! The coordinator will integrate it with the parser, typechecker, and
//! lowering passes.

use std::fmt;

// ── Distribution kinds ─────────────────────────────────────────────────

/// The kind of a built-in probability distribution.
///
/// Each variant corresponds to a well-known parametric distribution family.
/// `Custom` allows user-defined distributions.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DistributionKind {
    /// Bernoulli distribution over `Bool` with parameter `p`.
    Bernoulli,
    /// Uniform distribution over a range `[low, high]`.
    Uniform,
    /// Normal (Gaussian) distribution with `mean` and `std_dev`.
    Normal,
    /// Categorical distribution over a finite set of weighted values.
    Categorical,
    /// A user-defined distribution identified by name.
    Custom(String),
}

// ── Distribution parameters ────────────────────────────────────────────

/// A single parameter of a probability distribution.
///
/// For example, the Bernoulli distribution has one parameter `p` of type `Float`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct ProbParam {
    /// Parameter name (e.g. `"p"`, `"mean"`).
    pub name: String,
    /// Type of the parameter value (e.g. `"Float"`, `"T"`).
    pub value_type: String,
    /// Human-readable description of the parameter.
    pub description: String,
}

/// Type-level representation of a `Prob[T]` type with its distribution
/// and parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct ProbTypeDef {
    /// The inner type `T` in `Prob[T]`.
    pub inner_type: String,
    /// Which distribution family this represents.
    pub distribution: DistributionKind,
    /// The actual parameters supplied for this distribution instance.
    pub params: Vec<ProbParam>,
}

/// Returns the expected parameters for a given distribution kind.
///
/// The `inner_type` argument is used for distributions whose parameters
/// depend on the inner type (e.g. `Uniform` has `low: T, high: T`).
///
/// # Examples
///
/// ```text
/// distribution_params(&DistributionKind::Bernoulli, "Bool")
///   => [ProbParam { name: "p", value_type: "Float", ... }]
/// ```
#[allow(dead_code)]
pub fn distribution_params(kind: &DistributionKind, inner_type: &str) -> Vec<ProbParam> {
    match kind {
        DistributionKind::Bernoulli => vec![ProbParam {
            name: "p".to_string(),
            value_type: "Float".to_string(),
            description: "Probability of true".to_string(),
        }],
        DistributionKind::Uniform => vec![
            ProbParam {
                name: "low".to_string(),
                value_type: inner_type.to_string(),
                description: "Lower bound (inclusive)".to_string(),
            },
            ProbParam {
                name: "high".to_string(),
                value_type: inner_type.to_string(),
                description: "Upper bound (inclusive)".to_string(),
            },
        ],
        DistributionKind::Normal => vec![
            ProbParam {
                name: "mean".to_string(),
                value_type: "Float".to_string(),
                description: "Mean of the distribution".to_string(),
            },
            ProbParam {
                name: "std_dev".to_string(),
                value_type: "Float".to_string(),
                description: "Standard deviation".to_string(),
            },
        ],
        DistributionKind::Categorical => vec![
            ProbParam {
                name: "weights".to_string(),
                value_type: "List[Float]".to_string(),
                description: "Probability weights for each value".to_string(),
            },
            ProbParam {
                name: "values".to_string(),
                value_type: format!("List[{inner_type}]"),
                description: "Possible values".to_string(),
            },
        ],
        DistributionKind::Custom(_) => vec![],
    }
}

// ── Operations on Prob[T] ──────────────────────────────────────────────

/// An operation that can be applied to a `Prob[T]` value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ProbOperation {
    /// `sample(d)` — draw a concrete value from the distribution.
    /// Type: `Prob[T] -> T`
    Sample,
    /// `expectation(d)` — compute the expected value.
    /// Type: `Prob[numeric] -> Float` (rejects non-numeric `T`)
    Expectation,
    /// `probability(d, predicate)` — compute the probability of a predicate.
    /// Type: `Prob[T] + predicate -> Float`
    Probability,
    /// `given(d, predicate)` — condition the distribution on a predicate.
    /// Type: `Prob[T] + predicate -> Prob[T]`
    Condition,
    /// `map(d, f)` — transform distribution values.
    /// Type: `Prob[T] + (T -> U) -> Prob[U]`
    Map,
    /// `flat_map(d, f)` — monadic bind.
    /// Type: `Prob[T] + (T -> Prob[U]) -> Prob[U]`
    FlatMap,
    /// `combine(d1, d2)` — joint distribution.
    /// Type: `Prob[T] + Prob[U] -> Prob[(T, U)]`
    Combine,
}

// ── Error types ────────────────────────────────────────────────────────

/// Errors that can occur when working with probabilistic types.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ProbError {
    /// Distribution parameters are invalid (wrong count, wrong types).
    InvalidParams(String),
    /// A type mismatch in a probabilistic operation.
    TypeMismatch {
        /// The type that was expected.
        expected: String,
        /// The type that was actually provided.
        actual: String,
    },
    /// Attempted to compute expectation on a non-numeric distribution.
    NonNumericExpectation(String),
    /// A distribution kind is not supported or not recognized.
    UnsupportedDistribution(String),
}

impl fmt::Display for ProbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProbError::InvalidParams(msg) => write!(f, "invalid distribution parameters: {msg}"),
            ProbError::TypeMismatch { expected, actual } => {
                write!(f, "type mismatch: expected `{expected}`, found `{actual}`")
            }
            ProbError::NonNumericExpectation(ty) => {
                write!(f, "expectation requires a numeric type, but found `{ty}`")
            }
            ProbError::UnsupportedDistribution(name) => {
                write!(f, "unsupported distribution: `{name}`")
            }
        }
    }
}

impl std::error::Error for ProbError {}

// ── Validation ─────────────────────────────────────────────────────────

/// Checks whether a type name is numeric (`Int` or `Float`).
///
/// Used by operations like `Expectation` that require numeric inner types.
#[allow(dead_code)]
pub fn is_numeric_type(type_name: &str) -> bool {
    matches!(type_name, "Int" | "Float")
}

/// Validates that a distribution has the correct number of parameters.
///
/// Returns `Ok(())` if `param_count` matches the expected count for the
/// given distribution kind, or a [`ProbError::InvalidParams`] otherwise.
///
/// # Examples
///
/// ```text
/// validate_distribution(&DistributionKind::Bernoulli, "Bool", 1) // Ok
/// validate_distribution(&DistributionKind::Bernoulli, "Bool", 2) // Err
/// ```
#[allow(dead_code)]
pub fn validate_distribution(
    kind: &DistributionKind,
    inner_type: &str,
    param_count: usize,
) -> Result<(), ProbError> {
    let expected = distribution_params(kind, inner_type);
    let expected_count = expected.len();
    if param_count != expected_count {
        return Err(ProbError::InvalidParams(format!(
            "{kind:?} expects {expected_count} parameter(s), got {param_count}"
        )));
    }
    Ok(())
}

/// Type-checks a probabilistic operation.
///
/// Verifies that `input_type` (the inner type of the `Prob[T]`) and
/// `output_type` (the declared result type) are consistent with the
/// semantics of the operation.
///
/// # Errors
///
/// - [`ProbError::NonNumericExpectation`] if `Expectation` is applied to
///   a non-numeric type.
/// - [`ProbError::TypeMismatch`] if the output type does not match what
///   the operation produces.
#[allow(dead_code)]
pub fn validate_prob_operation(
    op: &ProbOperation,
    input_type: &str,
    output_type: &str,
) -> Result<(), ProbError> {
    let expected_output = prob_return_type(op, input_type);

    match op {
        ProbOperation::Sample => {
            // Prob[T] -> T
            if output_type != input_type {
                return Err(ProbError::TypeMismatch {
                    expected: input_type.to_string(),
                    actual: output_type.to_string(),
                });
            }
        }
        ProbOperation::Expectation => {
            // Prob[numeric] -> Float — reject non-numeric T
            if !is_numeric_type(input_type) {
                return Err(ProbError::NonNumericExpectation(input_type.to_string()));
            }
            if output_type != "Float" {
                return Err(ProbError::TypeMismatch {
                    expected: "Float".to_string(),
                    actual: output_type.to_string(),
                });
            }
        }
        ProbOperation::Probability => {
            // Prob[T] + predicate -> Float
            if output_type != "Float" {
                return Err(ProbError::TypeMismatch {
                    expected: "Float".to_string(),
                    actual: output_type.to_string(),
                });
            }
        }
        ProbOperation::Condition => {
            // Prob[T] + predicate -> Prob[T]
            let expected_prob = format!("Prob[{input_type}]");
            if output_type != expected_prob {
                return Err(ProbError::TypeMismatch {
                    expected: expected_prob,
                    actual: output_type.to_string(),
                });
            }
        }
        ProbOperation::Map => {
            // Prob[T] + (T -> U) -> Prob[U]
            // We accept any Prob[...] output — the actual U is determined
            // by the mapping function and checked separately.
            if !output_type.starts_with("Prob[") || !output_type.ends_with(']') {
                return Err(ProbError::TypeMismatch {
                    expected: "Prob[U]".to_string(),
                    actual: output_type.to_string(),
                });
            }
        }
        ProbOperation::FlatMap => {
            // Prob[T] + (T -> Prob[U]) -> Prob[U]
            if !output_type.starts_with("Prob[") || !output_type.ends_with(']') {
                return Err(ProbError::TypeMismatch {
                    expected: "Prob[U]".to_string(),
                    actual: output_type.to_string(),
                });
            }
        }
        ProbOperation::Combine => {
            // Prob[T] + Prob[U] -> Prob[(T, U)]
            // Here input_type is the first distribution's inner type.
            // We check the output structurally.
            if !output_type.starts_with("Prob[(") || !output_type.ends_with(")]") {
                return Err(ProbError::TypeMismatch {
                    expected: expected_output,
                    actual: output_type.to_string(),
                });
            }
        }
    }
    Ok(())
}

/// Returns the result type name for a probabilistic operation.
///
/// The `input_inner` argument is the `T` in `Prob[T]`. For operations
/// that produce a new type (e.g. `Map`, `FlatMap`), the output uses
/// a placeholder `U` since the actual type depends on the supplied function.
///
/// # Examples
///
/// ```text
/// prob_return_type(&ProbOperation::Sample, "Int")      => "Int"
/// prob_return_type(&ProbOperation::Expectation, "Int")  => "Float"
/// prob_return_type(&ProbOperation::Condition, "Float")  => "Prob[Float]"
/// ```
#[allow(dead_code)]
pub fn prob_return_type(op: &ProbOperation, input_inner: &str) -> String {
    match op {
        ProbOperation::Sample => input_inner.to_string(),
        ProbOperation::Expectation => "Float".to_string(),
        ProbOperation::Probability => "Float".to_string(),
        ProbOperation::Condition => format!("Prob[{input_inner}]"),
        ProbOperation::Map => "Prob[U]".to_string(),
        ProbOperation::FlatMap => "Prob[U]".to_string(),
        ProbOperation::Combine => format!("Prob[({input_inner}, U)]"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── distribution_params tests ──────────────────────────────────────

    #[test]
    fn bernoulli_has_one_param() {
        let params = distribution_params(&DistributionKind::Bernoulli, "Bool");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "p");
        assert_eq!(params[0].value_type, "Float");
    }

    #[test]
    fn uniform_has_two_params_matching_inner_type() {
        let params = distribution_params(&DistributionKind::Uniform, "Int");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "low");
        assert_eq!(params[0].value_type, "Int");
        assert_eq!(params[1].name, "high");
        assert_eq!(params[1].value_type, "Int");
    }

    #[test]
    fn normal_has_mean_and_std_dev() {
        let params = distribution_params(&DistributionKind::Normal, "Float");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "mean");
        assert_eq!(params[1].name, "std_dev");
        assert_eq!(params[0].value_type, "Float");
        assert_eq!(params[1].value_type, "Float");
    }

    #[test]
    fn categorical_has_weights_and_values() {
        let params = distribution_params(&DistributionKind::Categorical, "String");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "weights");
        assert_eq!(params[0].value_type, "List[Float]");
        assert_eq!(params[1].name, "values");
        assert_eq!(params[1].value_type, "List[String]");
    }

    #[test]
    fn custom_has_no_params() {
        let params = distribution_params(&DistributionKind::Custom("MyDist".into()), "Int");
        assert!(params.is_empty());
    }

    // ── is_numeric_type tests ──────────────────────────────────────────

    #[test]
    fn int_is_numeric() {
        assert!(is_numeric_type("Int"));
    }

    #[test]
    fn float_is_numeric() {
        assert!(is_numeric_type("Float"));
    }

    #[test]
    fn string_is_not_numeric() {
        assert!(!is_numeric_type("String"));
    }

    #[test]
    fn bool_is_not_numeric() {
        assert!(!is_numeric_type("Bool"));
    }
}
