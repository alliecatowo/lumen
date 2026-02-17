//! Wave 21 — T156: Prob<T> Probabilistic Type tests
//!
//! Exercises the probabilistic module:
//! - Distribution parameter definitions for each kind
//! - Distribution validation (param count checks)
//! - Operation type-checking (Sample, Expectation, Probability, etc.)
//! - Return type computation
//! - Numeric type detection
//! - Error display formatting

use lumen_compiler::compiler::probabilistic::*;

// ═══════════════════════════════════════════════════════════════════════
// distribution_params — 5 tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn probabilistic_bernoulli_params() {
    let params = distribution_params(&DistributionKind::Bernoulli, "Bool");
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].name, "p");
    assert_eq!(params[0].value_type, "Float");
    assert!(!params[0].description.is_empty());
}

#[test]
fn probabilistic_uniform_params_int() {
    let params = distribution_params(&DistributionKind::Uniform, "Int");
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].name, "low");
    assert_eq!(params[0].value_type, "Int");
    assert_eq!(params[1].name, "high");
    assert_eq!(params[1].value_type, "Int");
}

#[test]
fn probabilistic_uniform_params_float() {
    let params = distribution_params(&DistributionKind::Uniform, "Float");
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].value_type, "Float");
    assert_eq!(params[1].value_type, "Float");
}

#[test]
fn probabilistic_normal_params() {
    let params = distribution_params(&DistributionKind::Normal, "Float");
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].name, "mean");
    assert_eq!(params[0].value_type, "Float");
    assert_eq!(params[1].name, "std_dev");
    assert_eq!(params[1].value_type, "Float");
}

#[test]
fn probabilistic_categorical_params() {
    let params = distribution_params(&DistributionKind::Categorical, "String");
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].name, "weights");
    assert_eq!(params[0].value_type, "List[Float]");
    assert_eq!(params[1].name, "values");
    assert_eq!(params[1].value_type, "List[String]");
}

#[test]
fn probabilistic_custom_params_empty() {
    let params = distribution_params(&DistributionKind::Custom("Poisson".into()), "Int");
    assert!(params.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════
// validate_distribution — correct param counts (4 tests)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn probabilistic_validate_bernoulli_ok() {
    assert!(validate_distribution(&DistributionKind::Bernoulli, "Bool", 1).is_ok());
}

#[test]
fn probabilistic_validate_uniform_ok() {
    assert!(validate_distribution(&DistributionKind::Uniform, "Int", 2).is_ok());
}

#[test]
fn probabilistic_validate_normal_ok() {
    assert!(validate_distribution(&DistributionKind::Normal, "Float", 2).is_ok());
}

#[test]
fn probabilistic_validate_categorical_ok() {
    assert!(validate_distribution(&DistributionKind::Categorical, "String", 2).is_ok());
}

#[test]
fn probabilistic_validate_custom_ok() {
    assert!(validate_distribution(&DistributionKind::Custom("MyDist".into()), "Int", 0).is_ok());
}

// ═══════════════════════════════════════════════════════════════════════
// validate_distribution — wrong param counts (3 tests)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn probabilistic_validate_bernoulli_wrong_count() {
    let err = validate_distribution(&DistributionKind::Bernoulli, "Bool", 2).unwrap_err();
    assert!(matches!(err, ProbError::InvalidParams(_)));
}

#[test]
fn probabilistic_validate_normal_wrong_count() {
    let err = validate_distribution(&DistributionKind::Normal, "Float", 0).unwrap_err();
    assert!(matches!(err, ProbError::InvalidParams(_)));
}

#[test]
fn probabilistic_validate_uniform_wrong_count() {
    let err = validate_distribution(&DistributionKind::Uniform, "Int", 3).unwrap_err();
    assert!(matches!(err, ProbError::InvalidParams(_)));
}

// ═══════════════════════════════════════════════════════════════════════
// validate_prob_operation — type checks (7 tests)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn probabilistic_op_sample_int() {
    // Prob[Int] -> Int
    assert!(validate_prob_operation(&ProbOperation::Sample, "Int", "Int").is_ok());
}

#[test]
fn probabilistic_op_sample_type_mismatch() {
    // Prob[Int] -> String should fail
    let err = validate_prob_operation(&ProbOperation::Sample, "Int", "String").unwrap_err();
    assert!(matches!(err, ProbError::TypeMismatch { .. }));
}

#[test]
fn probabilistic_op_expectation_numeric() {
    // Prob[Int] -> Float
    assert!(validate_prob_operation(&ProbOperation::Expectation, "Int", "Float").is_ok());
    // Prob[Float] -> Float
    assert!(validate_prob_operation(&ProbOperation::Expectation, "Float", "Float").is_ok());
}

#[test]
fn probabilistic_op_expectation_rejects_non_numeric() {
    // Prob[String] -> Float should fail with NonNumericExpectation
    let err = validate_prob_operation(&ProbOperation::Expectation, "String", "Float").unwrap_err();
    assert!(matches!(err, ProbError::NonNumericExpectation(_)));
}

#[test]
fn probabilistic_op_probability() {
    // Prob[T] + predicate -> Float
    assert!(validate_prob_operation(&ProbOperation::Probability, "Int", "Float").is_ok());
    assert!(validate_prob_operation(&ProbOperation::Probability, "String", "Float").is_ok());
}

#[test]
fn probabilistic_op_condition() {
    // Prob[Float] + predicate -> Prob[Float]
    assert!(validate_prob_operation(&ProbOperation::Condition, "Float", "Prob[Float]").is_ok());
}

#[test]
fn probabilistic_op_condition_type_mismatch() {
    // Prob[Float] + predicate -> Prob[Int] should fail
    let err = validate_prob_operation(&ProbOperation::Condition, "Float", "Prob[Int]").unwrap_err();
    assert!(matches!(err, ProbError::TypeMismatch { .. }));
}

#[test]
fn probabilistic_op_map() {
    // Prob[Int] + (Int -> String) -> Prob[String]
    assert!(validate_prob_operation(&ProbOperation::Map, "Int", "Prob[String]").is_ok());
}

#[test]
fn probabilistic_op_map_invalid_output() {
    // Map must return Prob[...], not a bare type
    let err = validate_prob_operation(&ProbOperation::Map, "Int", "String").unwrap_err();
    assert!(matches!(err, ProbError::TypeMismatch { .. }));
}

#[test]
fn probabilistic_op_flatmap() {
    // Prob[Int] + (Int -> Prob[String]) -> Prob[String]
    assert!(validate_prob_operation(&ProbOperation::FlatMap, "Int", "Prob[String]").is_ok());
}

#[test]
fn probabilistic_op_flatmap_invalid_output() {
    // FlatMap must return Prob[...], not a bare type
    let err = validate_prob_operation(&ProbOperation::FlatMap, "Int", "String").unwrap_err();
    assert!(matches!(err, ProbError::TypeMismatch { .. }));
}

#[test]
fn probabilistic_op_combine() {
    // Prob[Int] + Prob[String] -> Prob[(Int, String)]
    assert!(validate_prob_operation(&ProbOperation::Combine, "Int", "Prob[(Int, String)]").is_ok());
}

#[test]
fn probabilistic_op_combine_invalid_output() {
    // Combine must return Prob[(...)]
    let err = validate_prob_operation(&ProbOperation::Combine, "Int", "Prob[Int]").unwrap_err();
    assert!(matches!(err, ProbError::TypeMismatch { .. }));
}

// ═══════════════════════════════════════════════════════════════════════
// prob_return_type — 7 tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn probabilistic_return_type_sample() {
    assert_eq!(prob_return_type(&ProbOperation::Sample, "Int"), "Int");
}

#[test]
fn probabilistic_return_type_expectation() {
    assert_eq!(
        prob_return_type(&ProbOperation::Expectation, "Int"),
        "Float"
    );
}

#[test]
fn probabilistic_return_type_probability() {
    assert_eq!(
        prob_return_type(&ProbOperation::Probability, "Bool"),
        "Float"
    );
}

#[test]
fn probabilistic_return_type_condition() {
    assert_eq!(
        prob_return_type(&ProbOperation::Condition, "Float"),
        "Prob[Float]"
    );
}

#[test]
fn probabilistic_return_type_map() {
    assert_eq!(prob_return_type(&ProbOperation::Map, "Int"), "Prob[U]");
}

#[test]
fn probabilistic_return_type_flatmap() {
    assert_eq!(prob_return_type(&ProbOperation::FlatMap, "Int"), "Prob[U]");
}

#[test]
fn probabilistic_return_type_combine() {
    assert_eq!(
        prob_return_type(&ProbOperation::Combine, "Int"),
        "Prob[(Int, U)]"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// is_numeric_type — 4 tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn probabilistic_int_is_numeric() {
    assert!(is_numeric_type("Int"));
}

#[test]
fn probabilistic_float_is_numeric() {
    assert!(is_numeric_type("Float"));
}

#[test]
fn probabilistic_string_not_numeric() {
    assert!(!is_numeric_type("String"));
}

#[test]
fn probabilistic_bool_not_numeric() {
    assert!(!is_numeric_type("Bool"));
}

// ═══════════════════════════════════════════════════════════════════════
// ProbError Display — 4 tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn probabilistic_error_display_invalid_params() {
    let err = ProbError::InvalidParams("wrong count".to_string());
    let msg = err.to_string();
    assert!(msg.contains("invalid distribution parameters"));
    assert!(msg.contains("wrong count"));
}

#[test]
fn probabilistic_error_display_type_mismatch() {
    let err = ProbError::TypeMismatch {
        expected: "Int".to_string(),
        actual: "String".to_string(),
    };
    let msg = err.to_string();
    assert!(msg.contains("type mismatch"));
    assert!(msg.contains("Int"));
    assert!(msg.contains("String"));
}

#[test]
fn probabilistic_error_display_non_numeric() {
    let err = ProbError::NonNumericExpectation("Bool".to_string());
    let msg = err.to_string();
    assert!(msg.contains("expectation requires a numeric type"));
    assert!(msg.contains("Bool"));
}

#[test]
fn probabilistic_error_display_unsupported() {
    let err = ProbError::UnsupportedDistribution("Zipf".to_string());
    let msg = err.to_string();
    assert!(msg.contains("unsupported distribution"));
    assert!(msg.contains("Zipf"));
}

// ═══════════════════════════════════════════════════════════════════════
// Struct construction / equality — bonus coverage
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn probabilistic_prob_type_def_construction() {
    let def = ProbTypeDef {
        inner_type: "Int".to_string(),
        distribution: DistributionKind::Uniform,
        params: vec![
            ProbParam {
                name: "low".to_string(),
                value_type: "Int".to_string(),
                description: "Lower bound".to_string(),
            },
            ProbParam {
                name: "high".to_string(),
                value_type: "Int".to_string(),
                description: "Upper bound".to_string(),
            },
        ],
    };
    assert_eq!(def.inner_type, "Int");
    assert_eq!(def.distribution, DistributionKind::Uniform);
    assert_eq!(def.params.len(), 2);
}

#[test]
fn probabilistic_distribution_kind_equality() {
    assert_eq!(DistributionKind::Bernoulli, DistributionKind::Bernoulli);
    assert_ne!(DistributionKind::Bernoulli, DistributionKind::Normal);
    assert_eq!(
        DistributionKind::Custom("X".into()),
        DistributionKind::Custom("X".into())
    );
    assert_ne!(
        DistributionKind::Custom("X".into()),
        DistributionKind::Custom("Y".into())
    );
}

#[test]
fn probabilistic_prob_error_is_std_error() {
    // Verify ProbError implements std::error::Error
    let err: Box<dyn std::error::Error> = Box::new(ProbError::InvalidParams("test".to_string()));
    assert!(err.to_string().contains("test"));
}
