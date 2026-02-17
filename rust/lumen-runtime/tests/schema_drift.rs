//! Comprehensive tests for `lumen_runtime::schema_drift` (T157: Schema drift detector).
//!
//! Covers schema comparison, value-level drift detection, drift history,
//! formatting, severity ordering, and error display.

use lumen_runtime::schema_drift::*;

// ===========================================================================
// Helpers
// ===========================================================================

fn person_schema() -> SchemaType {
    SchemaType::Record {
        name: "Person".to_string(),
        fields: vec![
            SchemaField {
                name: "name".to_string(),
                field_type: SchemaType::String,
                required: true,
            },
            SchemaField {
                name: "age".to_string(),
                field_type: SchemaType::Int,
                required: true,
            },
        ],
    }
}

fn make_breaking_drift(path: &str) -> Drift {
    Drift {
        path: path.to_string(),
        kind: DriftKind::TypeMismatch,
        expected: "Int".to_string(),
        actual: "String".to_string(),
        severity: DriftSeverity::Breaking,
    }
}

// ===========================================================================
// detect_drift — matching schemas
// ===========================================================================

#[test]
fn schema_drift_matching_schemas_empty() {
    let drifts = detect_drift(&SchemaType::String, &SchemaType::String, "root");
    assert!(drifts.is_empty());
}

#[test]
fn schema_drift_matching_int() {
    let drifts = detect_drift(&SchemaType::Int, &SchemaType::Int, "root");
    assert!(drifts.is_empty());
}

#[test]
fn schema_drift_matching_record() {
    let s = person_schema();
    let drifts = detect_drift(&s, &s, "root");
    assert!(drifts.is_empty());
}

// ===========================================================================
// detect_drift — type mismatch
// ===========================================================================

#[test]
fn schema_drift_type_mismatch_int_vs_string() {
    let drifts = detect_drift(&SchemaType::Int, &SchemaType::String, "root");
    assert_eq!(drifts.len(), 1);
    assert_eq!(drifts[0].kind, DriftKind::TypeMismatch);
    assert_eq!(drifts[0].severity, DriftSeverity::Breaking);
    assert_eq!(drifts[0].expected, "Int");
    assert_eq!(drifts[0].actual, "String");
}

#[test]
fn schema_drift_type_mismatch_bool_vs_float() {
    let drifts = detect_drift(&SchemaType::Bool, &SchemaType::Float, "root");
    assert_eq!(drifts.len(), 1);
    assert_eq!(drifts[0].kind, DriftKind::TypeMismatch);
}

// ===========================================================================
// detect_drift — missing field
// ===========================================================================

#[test]
fn schema_drift_missing_required_field() {
    let expected = person_schema();
    let actual = SchemaType::Record {
        name: "Person".to_string(),
        fields: vec![SchemaField {
            name: "name".to_string(),
            field_type: SchemaType::String,
            required: true,
        }],
    };
    let drifts = detect_drift(&expected, &actual, "root");
    assert_eq!(drifts.len(), 1);
    assert_eq!(drifts[0].kind, DriftKind::MissingField);
    assert_eq!(drifts[0].severity, DriftSeverity::Breaking);
    assert_eq!(drifts[0].path, "root.age");
}

#[test]
fn schema_drift_missing_optional_field() {
    let expected = SchemaType::Record {
        name: "Profile".to_string(),
        fields: vec![
            SchemaField {
                name: "name".to_string(),
                field_type: SchemaType::String,
                required: true,
            },
            SchemaField {
                name: "bio".to_string(),
                field_type: SchemaType::String,
                required: false,
            },
        ],
    };
    let actual = SchemaType::Record {
        name: "Profile".to_string(),
        fields: vec![SchemaField {
            name: "name".to_string(),
            field_type: SchemaType::String,
            required: true,
        }],
    };
    let drifts = detect_drift(&expected, &actual, "root");
    assert_eq!(drifts.len(), 1);
    assert_eq!(drifts[0].kind, DriftKind::MissingField);
    assert_eq!(drifts[0].severity, DriftSeverity::Warning);
}

// ===========================================================================
// detect_drift — extra field
// ===========================================================================

#[test]
fn schema_drift_extra_field() {
    let expected = SchemaType::Record {
        name: "Item".to_string(),
        fields: vec![SchemaField {
            name: "id".to_string(),
            field_type: SchemaType::Int,
            required: true,
        }],
    };
    let actual = SchemaType::Record {
        name: "Item".to_string(),
        fields: vec![
            SchemaField {
                name: "id".to_string(),
                field_type: SchemaType::Int,
                required: true,
            },
            SchemaField {
                name: "created_at".to_string(),
                field_type: SchemaType::String,
                required: true,
            },
        ],
    };
    let drifts = detect_drift(&expected, &actual, "root");
    assert_eq!(drifts.len(), 1);
    assert_eq!(drifts[0].kind, DriftKind::ExtraField);
    assert_eq!(drifts[0].severity, DriftSeverity::Info);
    assert_eq!(drifts[0].path, "root.created_at");
}

// ===========================================================================
// detect_drift — nullability change
// ===========================================================================

#[test]
fn schema_drift_nullability_change() {
    let drifts = detect_drift(&SchemaType::Int, &SchemaType::Null, "root.count");
    assert_eq!(drifts.len(), 1);
    assert_eq!(drifts[0].kind, DriftKind::NullabilityChange);
    assert_eq!(drifts[0].severity, DriftSeverity::Breaking);
}

#[test]
fn schema_drift_optional_accepts_null() {
    let expected = SchemaType::Optional(Box::new(SchemaType::Int));
    let drifts = detect_drift(&expected, &SchemaType::Null, "root");
    assert!(drifts.is_empty());
}

// ===========================================================================
// detect_drift — nested record mismatch
// ===========================================================================

#[test]
fn schema_drift_nested_record_mismatch() {
    let expected = SchemaType::Record {
        name: "Outer".to_string(),
        fields: vec![SchemaField {
            name: "inner".to_string(),
            field_type: SchemaType::Record {
                name: "Inner".to_string(),
                fields: vec![SchemaField {
                    name: "value".to_string(),
                    field_type: SchemaType::Int,
                    required: true,
                }],
            },
            required: true,
        }],
    };
    let actual = SchemaType::Record {
        name: "Outer".to_string(),
        fields: vec![SchemaField {
            name: "inner".to_string(),
            field_type: SchemaType::Record {
                name: "Inner".to_string(),
                fields: vec![SchemaField {
                    name: "value".to_string(),
                    field_type: SchemaType::String, // mismatch
                    required: true,
                }],
            },
            required: true,
        }],
    };
    let drifts = detect_drift(&expected, &actual, "root");
    assert_eq!(drifts.len(), 1);
    assert_eq!(drifts[0].path, "root.inner.value");
    assert_eq!(drifts[0].kind, DriftKind::TypeMismatch);
}

// ===========================================================================
// detect_drift — list element type mismatch
// ===========================================================================

#[test]
fn schema_drift_list_element_type_mismatch() {
    let expected = SchemaType::List(Box::new(SchemaType::Int));
    let actual = SchemaType::List(Box::new(SchemaType::String));
    let drifts = detect_drift(&expected, &actual, "root");
    assert_eq!(drifts.len(), 1);
    assert_eq!(drifts[0].path, "root[]");
    assert_eq!(drifts[0].kind, DriftKind::TypeMismatch);
}

// ===========================================================================
// detect_drift — Any matches anything
// ===========================================================================

#[test]
fn schema_drift_any_expected() {
    let drifts = detect_drift(&SchemaType::Any, &SchemaType::Int, "root");
    assert!(drifts.is_empty());
}

#[test]
fn schema_drift_any_actual() {
    let drifts = detect_drift(&SchemaType::String, &SchemaType::Any, "root");
    assert!(drifts.is_empty());
}

// ===========================================================================
// detect_drift — union types
// ===========================================================================

#[test]
fn schema_drift_union_matching() {
    let u = SchemaType::Union(vec![SchemaType::Int, SchemaType::String]);
    let drifts = detect_drift(&u, &u, "root");
    assert!(drifts.is_empty());
}

#[test]
fn schema_drift_union_narrowed() {
    let expected = SchemaType::Union(vec![SchemaType::Int, SchemaType::String]);
    let actual = SchemaType::Union(vec![SchemaType::Int]);
    let drifts = detect_drift(&expected, &actual, "root");
    assert!(drifts.iter().any(|d| d.kind == DriftKind::TypeNarrowed));
}

#[test]
fn schema_drift_union_widened() {
    let expected = SchemaType::Union(vec![SchemaType::Int]);
    let actual = SchemaType::Union(vec![SchemaType::Int, SchemaType::String]);
    let drifts = detect_drift(&expected, &actual, "root");
    assert!(drifts.iter().any(|d| d.kind == DriftKind::TypeWidened));
}

// ===========================================================================
// check_value_against_schema
// ===========================================================================

#[test]
fn schema_drift_value_valid_json_string() {
    let drifts = check_value_against_schema(r#""hello""#, &SchemaType::String);
    assert!(drifts.is_empty());
}

#[test]
fn schema_drift_value_type_mismatch_in_json() {
    let drifts = check_value_against_schema(r#""not_a_number""#, &SchemaType::Int);
    assert_eq!(drifts.len(), 1);
    assert_eq!(drifts[0].kind, DriftKind::TypeMismatch);
}

#[test]
fn schema_drift_value_missing_field_in_json() {
    let drifts = check_value_against_schema(r#"{"name": "Alice"}"#, &person_schema());
    assert!(drifts.iter().any(|d| d.kind == DriftKind::MissingField));
}

// ===========================================================================
// DriftHistory
// ===========================================================================

#[test]
fn schema_drift_history_new() {
    let h = DriftHistory::new(10);
    assert!(h.reports.is_empty());
    assert_eq!(h.max_reports, 10);
}

#[test]
fn schema_drift_history_add_report() {
    let mut h = DriftHistory::new(10);
    h.add_report(DriftReport::new(vec![], "test", 0));
    assert_eq!(h.reports.len(), 1);
}

#[test]
fn schema_drift_history_breaking_drifts() {
    let mut h = DriftHistory::new(10);
    let d = make_breaking_drift("root.x");
    h.add_report(DriftReport::new(vec![d], "s", 0));
    assert_eq!(h.breaking_drifts().len(), 1);
}

#[test]
fn schema_drift_history_drift_trend() {
    let mut h = DriftHistory::new(10);
    let d1 = make_breaking_drift("root.name");
    let d2 = Drift {
        path: "root.name".to_string(),
        kind: DriftKind::NullabilityChange,
        expected: "String".to_string(),
        actual: "Null".to_string(),
        severity: DriftSeverity::Breaking,
    };
    h.add_report(DriftReport::new(vec![d1], "s", 100));
    h.add_report(DriftReport::new(vec![d2], "s", 200));
    assert_eq!(h.drift_trend("root.name").len(), 2);
    assert!(h.drift_trend("root.other").is_empty());
}

#[test]
fn schema_drift_history_has_breaking() {
    let mut h = DriftHistory::new(10);
    assert!(!h.has_breaking());
    h.add_report(DriftReport::new(vec![make_breaking_drift("x")], "s", 0));
    assert!(h.has_breaking());
}

#[test]
fn schema_drift_history_max_reports_pruning() {
    let mut h = DriftHistory::new(3);
    for i in 0..5 {
        h.add_report(DriftReport::new(vec![], &format!("s{i}"), i as u64));
    }
    assert_eq!(h.reports.len(), 3);
    assert_eq!(h.reports[0].schema_name, "s2");
    assert_eq!(h.reports[1].schema_name, "s3");
    assert_eq!(h.reports[2].schema_name, "s4");
}

// ===========================================================================
// Formatting
// ===========================================================================

#[test]
fn schema_drift_format_drift_report() {
    let report = DriftReport::new(vec![], "TestSchema", 5000);
    let formatted = format_drift_report(&report);
    assert!(formatted.contains("TestSchema"));
    assert!(formatted.contains("5000ms"));
    assert!(formatted.contains("No drifts detected"));
}

#[test]
fn schema_drift_format_drift_report_with_items() {
    let d = make_breaking_drift("root.age");
    let report = DriftReport::new(vec![d], "Person", 1234);
    let formatted = format_drift_report(&report);
    assert!(formatted.contains("1 drift(s)"));
    assert!(formatted.contains("BREAKING"));
}

#[test]
fn schema_drift_format_drift_single() {
    let d = Drift {
        path: "root.x".to_string(),
        kind: DriftKind::MissingField,
        expected: "Int".to_string(),
        actual: "absent".to_string(),
        severity: DriftSeverity::Breaking,
    };
    let formatted = format_drift(&d);
    assert!(formatted.contains("[BREAKING]"));
    assert!(formatted.contains("MissingField"));
    assert!(formatted.contains("root.x"));
}

// ===========================================================================
// DriftSeverity ordering
// ===========================================================================

#[test]
fn schema_drift_severity_ordering() {
    assert!(DriftSeverity::Info < DriftSeverity::Warning);
    assert!(DriftSeverity::Warning < DriftSeverity::Breaking);
    assert!(DriftSeverity::Info < DriftSeverity::Breaking);
    assert_eq!(DriftSeverity::Info, DriftSeverity::Info);
}

// ===========================================================================
// DriftError Display
// ===========================================================================

#[test]
fn schema_drift_error_display() {
    let e1 = DriftError::InvalidSchema("bad".to_string());
    assert!(e1.to_string().contains("invalid schema"));
    assert!(e1.to_string().contains("bad"));

    let e2 = DriftError::ParseError("unexpected".to_string());
    assert!(e2.to_string().contains("parse error"));

    let e3 = DriftError::ComparisonError("depth".to_string());
    assert!(e3.to_string().contains("comparison error"));
}

#[test]
fn schema_drift_error_is_std_error() {
    let e: Box<dyn std::error::Error> = Box::new(DriftError::ParseError("test".to_string()));
    assert!(e.to_string().contains("test"));
}

// ===========================================================================
// Additional edge cases
// ===========================================================================

#[test]
fn schema_drift_map_value_mismatch() {
    let expected = SchemaType::Map(Box::new(SchemaType::String), Box::new(SchemaType::Int));
    let actual = SchemaType::Map(Box::new(SchemaType::String), Box::new(SchemaType::Float));
    let drifts = detect_drift(&expected, &actual, "root");
    assert_eq!(drifts.len(), 1);
    assert_eq!(drifts[0].path, "root<value>");
}

#[test]
fn schema_drift_value_json_float() {
    let drifts = check_value_against_schema("3.14", &SchemaType::Float);
    assert!(drifts.is_empty());
}

#[test]
fn schema_drift_value_json_bool() {
    let drifts = check_value_against_schema("true", &SchemaType::Bool);
    assert!(drifts.is_empty());
}

#[test]
fn schema_drift_value_json_null_vs_optional() {
    let schema = SchemaType::Optional(Box::new(SchemaType::Int));
    let drifts = check_value_against_schema("null", &schema);
    assert!(drifts.is_empty());
}

#[test]
fn schema_drift_value_json_nested_object_mismatch() {
    let schema = SchemaType::Record {
        name: "Outer".to_string(),
        fields: vec![SchemaField {
            name: "inner".to_string(),
            field_type: SchemaType::Record {
                name: "Inner".to_string(),
                fields: vec![SchemaField {
                    name: "val".to_string(),
                    field_type: SchemaType::Int,
                    required: true,
                }],
            },
            required: true,
        }],
    };
    let drifts = check_value_against_schema(r#"{"inner": {"val": "oops"}}"#, &schema);
    assert_eq!(drifts.len(), 1);
    assert_eq!(drifts[0].path, "root.inner.val");
}

#[test]
fn schema_drift_value_unparseable() {
    let drifts = check_value_against_schema("{bad json", &SchemaType::Int);
    assert_eq!(drifts.len(), 1);
    assert!(drifts[0].actual.contains("unparseable"));
}

#[test]
fn schema_drift_value_json_array_element_mismatch() {
    let schema = SchemaType::List(Box::new(SchemaType::Int));
    let drifts = check_value_against_schema(r#"["a", "b"]"#, &schema);
    assert_eq!(drifts.len(), 1);
    assert_eq!(drifts[0].path, "root[]");
}

#[test]
fn schema_drift_multiple_issues_in_record() {
    let expected = SchemaType::Record {
        name: "R".to_string(),
        fields: vec![
            SchemaField {
                name: "a".to_string(),
                field_type: SchemaType::Int,
                required: true,
            },
            SchemaField {
                name: "b".to_string(),
                field_type: SchemaType::String,
                required: true,
            },
        ],
    };
    let actual = SchemaType::Record {
        name: "R".to_string(),
        fields: vec![
            SchemaField {
                name: "a".to_string(),
                field_type: SchemaType::String,
                required: true,
            },
            SchemaField {
                name: "c".to_string(),
                field_type: SchemaType::Bool,
                required: true,
            },
        ],
    };
    let drifts = detect_drift(&expected, &actual, "root");
    assert_eq!(drifts.len(), 3); // a mismatch, b missing, c extra
}

#[test]
fn schema_drift_schema_type_display_union() {
    let u = SchemaType::Union(vec![SchemaType::Int, SchemaType::String]);
    assert_eq!(u.to_string(), "Int | String");
}
