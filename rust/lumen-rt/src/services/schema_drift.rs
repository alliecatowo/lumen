//! Schema drift detection for tool and API responses.
//!
//! Detects when a tool/API response schema diverges from the declared Lumen
//! types — preventing silent breakage.  For example, a tool declaration says
//! it returns `Record { name: String, age: Int }` but the actual API response
//! has `{ "name": "Alice", "age": "25" }` (age is String, not Int) or
//! `{ "name": "Alice" }` (missing age field).
//!
//! This module provides:
//!
//! - [`SchemaType`] / [`SchemaField`] — schema representation.
//! - [`detect_drift`] — recursively compare two schemas and find all
//!   differences.
//! - [`check_value_against_schema`] — validate a JSON value string against an
//!   expected schema.
//! - [`DriftHistory`] — accumulate reports and query trends.
//! - Formatting helpers for human-readable output.

use std::fmt;

// ---------------------------------------------------------------------------
// Schema types
// ---------------------------------------------------------------------------

/// Represents a Lumen type for schema comparison.
#[derive(Debug, Clone, PartialEq)]
pub enum SchemaType {
    /// String type.
    String,
    /// Integer type.
    Int,
    /// Floating-point type.
    Float,
    /// Boolean type.
    Bool,
    /// Null type.
    Null,
    /// Homogeneous list with element type.
    List(Box<SchemaType>),
    /// Map with key and value types.
    Map(Box<SchemaType>, Box<SchemaType>),
    /// Named record with typed fields.
    Record {
        /// Record name (e.g. `"Person"`).
        name: std::string::String,
        /// Ordered field declarations.
        fields: Vec<SchemaField>,
    },
    /// Union of possible types.
    Union(Vec<SchemaType>),
    /// Optional wrapper — `T | Null`.
    Optional(Box<SchemaType>),
    /// Wildcard that matches any type.
    Any,
}

impl fmt::Display for SchemaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchemaType::String => write!(f, "String"),
            SchemaType::Int => write!(f, "Int"),
            SchemaType::Float => write!(f, "Float"),
            SchemaType::Bool => write!(f, "Bool"),
            SchemaType::Null => write!(f, "Null"),
            SchemaType::List(el) => write!(f, "List[{el}]"),
            SchemaType::Map(k, v) => write!(f, "Map[{k}, {v}]"),
            SchemaType::Record { name, .. } => write!(f, "{name}"),
            SchemaType::Union(variants) => {
                let parts: Vec<std::string::String> =
                    variants.iter().map(|v| v.to_string()).collect();
                write!(f, "{}", parts.join(" | "))
            }
            SchemaType::Optional(inner) => write!(f, "{inner}?"),
            SchemaType::Any => write!(f, "Any"),
        }
    }
}

/// A single field in a [`SchemaType::Record`].
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaField {
    /// Field name.
    pub name: std::string::String,
    /// Field type.
    pub field_type: SchemaType,
    /// Whether the field is required (`true`) or optional (`false`).
    pub required: bool,
}

// ---------------------------------------------------------------------------
// Drift types
// ---------------------------------------------------------------------------

/// A single detected schema drift.
#[derive(Debug, Clone, PartialEq)]
pub struct Drift {
    /// Dot-separated path to the drifted element (e.g. `"root.address.city"`).
    pub path: std::string::String,
    /// Category of drift.
    pub kind: DriftKind,
    /// Human-readable description of the expected schema element.
    pub expected: std::string::String,
    /// Human-readable description of the actual schema element.
    pub actual: std::string::String,
    /// How severe the drift is.
    pub severity: DriftSeverity,
}

/// Classifies the nature of a schema drift.
#[derive(Debug, Clone, PartialEq)]
pub enum DriftKind {
    /// Two completely different base types.
    TypeMismatch,
    /// A required or optional field is absent.
    MissingField,
    /// A field is present in the actual schema but not declared in expected.
    ExtraField,
    /// Null appeared where a non-null type was expected (or vice-versa).
    NullabilityChange,
    /// A type was widened (e.g. `Int` → `Int | String`).
    TypeWidened,
    /// A type was narrowed (e.g. `Int | String` → `Int`).
    TypeNarrowed,
    /// A field may have been renamed. `similarity` is in `[0.0, 1.0]`.
    FieldRenamed {
        /// Similarity score between old and new field names.
        similarity: f64,
    },
}

impl fmt::Display for DriftKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriftKind::TypeMismatch => write!(f, "TypeMismatch"),
            DriftKind::MissingField => write!(f, "MissingField"),
            DriftKind::ExtraField => write!(f, "ExtraField"),
            DriftKind::NullabilityChange => write!(f, "NullabilityChange"),
            DriftKind::TypeWidened => write!(f, "TypeWidened"),
            DriftKind::TypeNarrowed => write!(f, "TypeNarrowed"),
            DriftKind::FieldRenamed { similarity } => {
                write!(f, "FieldRenamed(similarity={similarity:.2})")
            }
        }
    }
}

/// Severity level of a drift.
///
/// Ordering: `Info < Warning < Breaking`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DriftSeverity {
    /// Informational — no immediate breakage expected.
    Info,
    /// Warning — may cause issues depending on usage.
    Warning,
    /// Breaking — will cause runtime failures.
    Breaking,
}

impl DriftSeverity {
    /// Numeric rank for ordering.
    fn rank(self) -> u8 {
        match self {
            DriftSeverity::Info => 0,
            DriftSeverity::Warning => 1,
            DriftSeverity::Breaking => 2,
        }
    }
}

impl PartialOrd for DriftSeverity {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DriftSeverity {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.rank().cmp(&other.rank())
    }
}

impl fmt::Display for DriftSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriftSeverity::Info => write!(f, "INFO"),
            DriftSeverity::Warning => write!(f, "WARNING"),
            DriftSeverity::Breaking => write!(f, "BREAKING"),
        }
    }
}

// ---------------------------------------------------------------------------
// DriftReport
// ---------------------------------------------------------------------------

/// Aggregated result of a schema comparison.
#[derive(Debug, Clone)]
pub struct DriftReport {
    /// All drifts found in this comparison.
    pub drifts: Vec<Drift>,
    /// Name/identifier of the schema that was compared.
    pub schema_name: std::string::String,
    /// Millisecond timestamp when the comparison was performed.
    pub timestamp_ms: u64,
}

impl DriftReport {
    /// Create a new report.
    pub fn new(
        drifts: Vec<Drift>,
        schema_name: impl Into<std::string::String>,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            drifts,
            schema_name: schema_name.into(),
            timestamp_ms,
        }
    }

    /// Whether the report contains any breaking drifts.
    pub fn has_breaking(&self) -> bool {
        self.drifts
            .iter()
            .any(|d| d.severity == DriftSeverity::Breaking)
    }

    /// Number of drifts.
    pub fn len(&self) -> usize {
        self.drifts.len()
    }

    /// Whether the report has zero drifts.
    pub fn is_empty(&self) -> bool {
        self.drifts.is_empty()
    }
}

// ---------------------------------------------------------------------------
// DriftError
// ---------------------------------------------------------------------------

/// Errors that can occur during schema drift detection.
#[derive(Debug)]
pub enum DriftError {
    /// The schema definition is invalid.
    InvalidSchema(std::string::String),
    /// Failed to parse an input (e.g. JSON).
    ParseError(std::string::String),
    /// An internal comparison error.
    ComparisonError(std::string::String),
}

impl fmt::Display for DriftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriftError::InvalidSchema(msg) => write!(f, "invalid schema: {msg}"),
            DriftError::ParseError(msg) => write!(f, "parse error: {msg}"),
            DriftError::ComparisonError(msg) => write!(f, "comparison error: {msg}"),
        }
    }
}

impl std::error::Error for DriftError {}

// ---------------------------------------------------------------------------
// Schema comparison — detect_drift
// ---------------------------------------------------------------------------

/// Recursively compare two schemas and collect all detected drifts.
///
/// `path` is the dot-separated prefix for the current position in the schema
/// tree (pass `"root"` at the top level).
pub fn detect_drift(expected: &SchemaType, actual: &SchemaType, path: &str) -> Vec<Drift> {
    let mut drifts = Vec::new();
    detect_drift_inner(expected, actual, path, &mut drifts);
    drifts
}

fn detect_drift_inner(
    expected: &SchemaType,
    actual: &SchemaType,
    path: &str,
    drifts: &mut Vec<Drift>,
) {
    // Any matches everything — no drift.
    if matches!(expected, SchemaType::Any) || matches!(actual, SchemaType::Any) {
        return;
    }

    // Identical types — no drift.
    if expected == actual {
        return;
    }

    match (expected, actual) {
        // ---- Optional handling ----
        (SchemaType::Optional(inner), SchemaType::Null) => {
            // Null is valid for Optional — no drift.
            let _ = inner;
        }
        (SchemaType::Optional(inner_exp), _) => {
            // Compare the inner type against actual.
            detect_drift_inner(inner_exp, actual, path, drifts);
        }
        (_, SchemaType::Optional(inner_act)) => {
            // Expected non-optional but got optional: nullability change.
            // Still compare inner types.
            drifts.push(Drift {
                path: path.to_string(),
                kind: DriftKind::NullabilityChange,
                expected: expected.to_string(),
                actual: format!("{}?", inner_act),
                severity: DriftSeverity::Breaking,
            });
        }

        // ---- Null where non-null expected ----
        (_, SchemaType::Null) => {
            drifts.push(Drift {
                path: path.to_string(),
                kind: DriftKind::NullabilityChange,
                expected: expected.to_string(),
                actual: "Null".to_string(),
                severity: DriftSeverity::Breaking,
            });
        }
        (SchemaType::Null, _) => {
            drifts.push(Drift {
                path: path.to_string(),
                kind: DriftKind::NullabilityChange,
                expected: "Null".to_string(),
                actual: actual.to_string(),
                severity: DriftSeverity::Breaking,
            });
        }

        // ---- List ----
        (SchemaType::List(exp_el), SchemaType::List(act_el)) => {
            let child_path = format!("{path}[]");
            detect_drift_inner(exp_el, act_el, &child_path, drifts);
        }

        // ---- Map ----
        (SchemaType::Map(ek, ev), SchemaType::Map(ak, av)) => {
            detect_drift_inner(ek, ak, &format!("{path}<key>"), drifts);
            detect_drift_inner(ev, av, &format!("{path}<value>"), drifts);
        }

        // ---- Record ----
        (
            SchemaType::Record {
                name: exp_name,
                fields: exp_fields,
            },
            SchemaType::Record {
                name: _act_name,
                fields: act_fields,
            },
        ) => {
            // Check each expected field.
            for exp_f in exp_fields {
                if let Some(act_f) = act_fields.iter().find(|f| f.name == exp_f.name) {
                    let child_path = format!("{path}.{}", exp_f.name);
                    detect_drift_inner(&exp_f.field_type, &act_f.field_type, &child_path, drifts);
                } else {
                    let severity = if exp_f.required {
                        DriftSeverity::Breaking
                    } else {
                        DriftSeverity::Warning
                    };
                    drifts.push(Drift {
                        path: format!("{path}.{}", exp_f.name),
                        kind: DriftKind::MissingField,
                        expected: format!("{} ({})", exp_f.field_type, exp_name),
                        actual: "absent".to_string(),
                        severity,
                    });
                }
            }

            // Check extra fields in actual.
            for act_f in act_fields {
                if !exp_fields.iter().any(|f| f.name == act_f.name) {
                    drifts.push(Drift {
                        path: format!("{path}.{}", act_f.name),
                        kind: DriftKind::ExtraField,
                        expected: "absent".to_string(),
                        actual: act_f.field_type.to_string(),
                        severity: DriftSeverity::Info,
                    });
                }
            }
        }

        // ---- Union ----
        (SchemaType::Union(exp_variants), SchemaType::Union(act_variants)) => {
            // Check for narrowed variants (in expected but not in actual).
            for ev in exp_variants {
                if !act_variants.iter().any(|av| av == ev) {
                    drifts.push(Drift {
                        path: path.to_string(),
                        kind: DriftKind::TypeNarrowed,
                        expected: ev.to_string(),
                        actual: "absent from union".to_string(),
                        severity: DriftSeverity::Breaking,
                    });
                }
            }
            // Check for widened variants (in actual but not in expected).
            for av in act_variants {
                if !exp_variants.iter().any(|ev| ev == av) {
                    drifts.push(Drift {
                        path: path.to_string(),
                        kind: DriftKind::TypeWidened,
                        expected: "absent from union".to_string(),
                        actual: av.to_string(),
                        severity: DriftSeverity::Warning,
                    });
                }
            }
        }

        // ---- Union expected vs concrete actual ----
        (SchemaType::Union(variants), _) => {
            // If actual matches any variant, no drift.
            if variants.iter().any(|v| v == actual) {
                return;
            }
            // Otherwise type mismatch.
            drifts.push(Drift {
                path: path.to_string(),
                kind: DriftKind::TypeMismatch,
                expected: expected.to_string(),
                actual: actual.to_string(),
                severity: DriftSeverity::Breaking,
            });
        }

        // ---- Concrete expected vs union actual ----
        (_, SchemaType::Union(variants)) => {
            if variants.iter().any(|v| v == expected) {
                // Expected is a member — type widened.
                drifts.push(Drift {
                    path: path.to_string(),
                    kind: DriftKind::TypeWidened,
                    expected: expected.to_string(),
                    actual: actual.to_string(),
                    severity: DriftSeverity::Warning,
                });
            } else {
                drifts.push(Drift {
                    path: path.to_string(),
                    kind: DriftKind::TypeMismatch,
                    expected: expected.to_string(),
                    actual: actual.to_string(),
                    severity: DriftSeverity::Breaking,
                });
            }
        }

        // ---- Catch-all: type mismatch ----
        _ => {
            drifts.push(Drift {
                path: path.to_string(),
                kind: DriftKind::TypeMismatch,
                expected: expected.to_string(),
                actual: actual.to_string(),
                severity: DriftSeverity::Breaking,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Value-level drift detection
// ---------------------------------------------------------------------------

/// Check a JSON value string against an expected schema.
///
/// Parses the JSON into a structural [`SchemaType`] and then delegates to
/// [`detect_drift`].
pub fn check_value_against_schema(value: &str, schema: &SchemaType) -> Vec<Drift> {
    let parsed = match serde_json::from_str::<serde_json::Value>(value) {
        Ok(v) => v,
        Err(e) => {
            return vec![Drift {
                path: "root".to_string(),
                kind: DriftKind::TypeMismatch,
                expected: schema.to_string(),
                actual: format!("unparseable: {e}"),
                severity: DriftSeverity::Breaking,
            }];
        }
    };

    let actual_schema = json_value_to_schema(&parsed);
    detect_drift(schema, &actual_schema, "root")
}

/// Convert a [`serde_json::Value`] into a structural [`SchemaType`].
fn json_value_to_schema(val: &serde_json::Value) -> SchemaType {
    match val {
        serde_json::Value::Null => SchemaType::Null,
        serde_json::Value::Bool(_) => SchemaType::Bool,
        serde_json::Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                SchemaType::Int
            } else {
                SchemaType::Float
            }
        }
        serde_json::Value::String(_) => SchemaType::String,
        serde_json::Value::Array(arr) => {
            if arr.is_empty() {
                SchemaType::List(Box::new(SchemaType::Any))
            } else {
                // Infer element type from first element.
                let el = json_value_to_schema(&arr[0]);
                SchemaType::List(Box::new(el))
            }
        }
        serde_json::Value::Object(map) => {
            let fields: Vec<SchemaField> = map
                .iter()
                .map(|(k, v)| SchemaField {
                    name: k.clone(),
                    field_type: json_value_to_schema(v),
                    required: true,
                })
                .collect();
            SchemaType::Record {
                name: "object".to_string(),
                fields,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// DriftHistory
// ---------------------------------------------------------------------------

/// Accumulates [`DriftReport`]s over time and provides query helpers.
#[derive(Debug)]
pub struct DriftHistory {
    /// Stored reports (newest last).
    pub reports: Vec<DriftReport>,
    /// Maximum number of reports to keep.
    pub max_reports: usize,
}

impl DriftHistory {
    /// Create a new history with a capacity limit.
    pub fn new(max_reports: usize) -> Self {
        Self {
            reports: Vec::new(),
            max_reports,
        }
    }

    /// Add a report, pruning the oldest if the capacity is exceeded.
    pub fn add_report(&mut self, report: DriftReport) {
        self.reports.push(report);
        while self.reports.len() > self.max_reports {
            self.reports.remove(0);
        }
    }

    /// Return all breaking drifts across the entire history.
    pub fn breaking_drifts(&self) -> Vec<&Drift> {
        self.reports
            .iter()
            .flat_map(|r| r.drifts.iter())
            .filter(|d| d.severity == DriftSeverity::Breaking)
            .collect()
    }

    /// Return all drifts affecting a specific `field_path` over time.
    pub fn drift_trend(&self, field_path: &str) -> Vec<&Drift> {
        self.reports
            .iter()
            .flat_map(|r| r.drifts.iter())
            .filter(|d| d.path == field_path)
            .collect()
    }

    /// Whether any report in the history contains a breaking drift.
    pub fn has_breaking(&self) -> bool {
        self.reports.iter().any(|r| r.has_breaking())
    }
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

/// Format a [`DriftReport`] as a human-readable multi-line string.
pub fn format_drift_report(report: &DriftReport) -> std::string::String {
    let mut out = std::string::String::new();
    out.push_str(&format!(
        "Schema Drift Report: {} (at {}ms)\n",
        report.schema_name, report.timestamp_ms
    ));
    if report.drifts.is_empty() {
        out.push_str("  No drifts detected.\n");
    } else {
        out.push_str(&format!("  {} drift(s) found:\n", report.drifts.len()));
        for drift in &report.drifts {
            out.push_str(&format!("    {}\n", format_drift(drift)));
        }
    }
    out
}

/// Format a single [`Drift`] as a one-line summary.
pub fn format_drift(drift: &Drift) -> std::string::String {
    format!(
        "[{}] {} at '{}': expected {}, got {}",
        drift.severity, drift.kind, drift.path, drift.expected, drift.actual
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- helpers ------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // detect_drift — matching schemas
    // -----------------------------------------------------------------------

    #[test]
    fn drift_matching_schemas_empty() {
        let drifts = detect_drift(&SchemaType::String, &SchemaType::String, "root");
        assert!(drifts.is_empty());
    }

    #[test]
    fn drift_matching_records() {
        let schema = person_schema();
        let drifts = detect_drift(&schema, &schema, "root");
        assert!(drifts.is_empty());
    }

    // -----------------------------------------------------------------------
    // detect_drift — type mismatch
    // -----------------------------------------------------------------------

    #[test]
    fn drift_type_mismatch_int_vs_string() {
        let drifts = detect_drift(&SchemaType::Int, &SchemaType::String, "root");
        assert_eq!(drifts.len(), 1);
        assert_eq!(drifts[0].kind, DriftKind::TypeMismatch);
        assert_eq!(drifts[0].severity, DriftSeverity::Breaking);
        assert_eq!(drifts[0].path, "root");
    }

    #[test]
    fn drift_type_mismatch_bool_vs_float() {
        let drifts = detect_drift(&SchemaType::Bool, &SchemaType::Float, "root");
        assert_eq!(drifts.len(), 1);
        assert_eq!(drifts[0].kind, DriftKind::TypeMismatch);
    }

    // -----------------------------------------------------------------------
    // detect_drift — missing field
    // -----------------------------------------------------------------------

    #[test]
    fn drift_missing_required_field() {
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
    fn drift_missing_optional_field() {
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

    // -----------------------------------------------------------------------
    // detect_drift — extra field
    // -----------------------------------------------------------------------

    #[test]
    fn drift_extra_field() {
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

    // -----------------------------------------------------------------------
    // detect_drift — nullability change
    // -----------------------------------------------------------------------

    #[test]
    fn drift_nullability_change() {
        let drifts = detect_drift(&SchemaType::Int, &SchemaType::Null, "root.count");
        assert_eq!(drifts.len(), 1);
        assert_eq!(drifts[0].kind, DriftKind::NullabilityChange);
        assert_eq!(drifts[0].severity, DriftSeverity::Breaking);
    }

    #[test]
    fn drift_optional_accepts_null() {
        let expected = SchemaType::Optional(Box::new(SchemaType::Int));
        let drifts = detect_drift(&expected, &SchemaType::Null, "root.opt");
        assert!(drifts.is_empty(), "Optional should accept Null");
    }

    #[test]
    fn drift_optional_accepts_inner() {
        let expected = SchemaType::Optional(Box::new(SchemaType::Int));
        let drifts = detect_drift(&expected, &SchemaType::Int, "root.opt");
        assert!(drifts.is_empty(), "Optional should accept inner type");
    }

    // -----------------------------------------------------------------------
    // detect_drift — nested record mismatch
    // -----------------------------------------------------------------------

    #[test]
    fn drift_nested_record_mismatch() {
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
                        field_type: SchemaType::String,
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

    // -----------------------------------------------------------------------
    // detect_drift — list element type mismatch
    // -----------------------------------------------------------------------

    #[test]
    fn drift_list_element_type_mismatch() {
        let expected = SchemaType::List(Box::new(SchemaType::Int));
        let actual = SchemaType::List(Box::new(SchemaType::String));
        let drifts = detect_drift(&expected, &actual, "root");
        assert_eq!(drifts.len(), 1);
        assert_eq!(drifts[0].path, "root[]");
        assert_eq!(drifts[0].kind, DriftKind::TypeMismatch);
    }

    #[test]
    fn drift_list_matching() {
        let schema = SchemaType::List(Box::new(SchemaType::Int));
        let drifts = detect_drift(&schema, &schema, "root");
        assert!(drifts.is_empty());
    }

    // -----------------------------------------------------------------------
    // detect_drift — Any matches anything
    // -----------------------------------------------------------------------

    #[test]
    fn drift_any_expected_matches_all() {
        let drifts = detect_drift(&SchemaType::Any, &SchemaType::Int, "root");
        assert!(drifts.is_empty());
    }

    #[test]
    fn drift_any_actual_matches_all() {
        let drifts = detect_drift(&SchemaType::String, &SchemaType::Any, "root");
        assert!(drifts.is_empty());
    }

    // -----------------------------------------------------------------------
    // detect_drift — union types
    // -----------------------------------------------------------------------

    #[test]
    fn drift_union_matching() {
        let u = SchemaType::Union(vec![SchemaType::Int, SchemaType::String]);
        let drifts = detect_drift(&u, &u, "root");
        assert!(drifts.is_empty());
    }

    #[test]
    fn drift_union_narrowed() {
        let expected = SchemaType::Union(vec![SchemaType::Int, SchemaType::String]);
        let actual = SchemaType::Union(vec![SchemaType::Int]);
        let drifts = detect_drift(&expected, &actual, "root");
        assert!(drifts.iter().any(|d| d.kind == DriftKind::TypeNarrowed));
        assert!(drifts.iter().any(|d| d.severity == DriftSeverity::Breaking));
    }

    #[test]
    fn drift_union_widened() {
        let expected = SchemaType::Union(vec![SchemaType::Int]);
        let actual = SchemaType::Union(vec![SchemaType::Int, SchemaType::String]);
        let drifts = detect_drift(&expected, &actual, "root");
        assert!(drifts.iter().any(|d| d.kind == DriftKind::TypeWidened));
        assert!(drifts.iter().any(|d| d.severity == DriftSeverity::Warning));
    }

    #[test]
    fn drift_union_expected_accepts_member() {
        let expected = SchemaType::Union(vec![SchemaType::Int, SchemaType::String]);
        let actual = SchemaType::Int;
        let drifts = detect_drift(&expected, &actual, "root");
        assert!(drifts.is_empty(), "union should accept any member type");
    }

    // -----------------------------------------------------------------------
    // check_value_against_schema
    // -----------------------------------------------------------------------

    #[test]
    fn value_valid_json_string() {
        let schema = SchemaType::String;
        let drifts = check_value_against_schema(r#""hello""#, &schema);
        assert!(drifts.is_empty());
    }

    #[test]
    fn value_type_mismatch_in_json() {
        let schema = SchemaType::Int;
        let drifts = check_value_against_schema(r#""not_a_number""#, &schema);
        assert_eq!(drifts.len(), 1);
        assert_eq!(drifts[0].kind, DriftKind::TypeMismatch);
    }

    #[test]
    fn value_missing_field_in_json() {
        let schema = person_schema();
        let drifts = check_value_against_schema(r#"{"name": "Alice"}"#, &schema);
        assert!(drifts.iter().any(|d| d.kind == DriftKind::MissingField));
    }

    #[test]
    fn value_valid_record() {
        let schema = person_schema();
        let drifts = check_value_against_schema(r#"{"name": "Alice", "age": 30}"#, &schema);
        assert!(drifts.is_empty());
    }

    #[test]
    fn value_unparseable_json() {
        let schema = SchemaType::Int;
        let drifts = check_value_against_schema("{invalid", &schema);
        assert_eq!(drifts.len(), 1);
        assert!(drifts[0].actual.contains("unparseable"));
    }

    #[test]
    fn value_null_json() {
        let schema = SchemaType::Int;
        let drifts = check_value_against_schema("null", &schema);
        assert_eq!(drifts.len(), 1);
        assert_eq!(drifts[0].kind, DriftKind::NullabilityChange);
    }

    #[test]
    fn value_json_array() {
        let schema = SchemaType::List(Box::new(SchemaType::Int));
        let drifts = check_value_against_schema("[1, 2, 3]", &schema);
        assert!(drifts.is_empty());
    }

    #[test]
    fn value_json_array_element_mismatch() {
        let schema = SchemaType::List(Box::new(SchemaType::Int));
        let drifts = check_value_against_schema(r#"["a", "b"]"#, &schema);
        assert_eq!(drifts.len(), 1);
        assert_eq!(drifts[0].path, "root[]");
    }

    // -----------------------------------------------------------------------
    // DriftHistory
    // -----------------------------------------------------------------------

    #[test]
    fn history_new_empty() {
        let h = DriftHistory::new(10);
        assert!(h.reports.is_empty());
        assert_eq!(h.max_reports, 10);
        assert!(!h.has_breaking());
    }

    #[test]
    fn history_add_report() {
        let mut h = DriftHistory::new(10);
        let report = DriftReport::new(vec![], "test", 1000);
        h.add_report(report);
        assert_eq!(h.reports.len(), 1);
    }

    #[test]
    fn history_breaking_drifts() {
        let mut h = DriftHistory::new(10);
        let d1 = Drift {
            path: "root.x".to_string(),
            kind: DriftKind::TypeMismatch,
            expected: "Int".to_string(),
            actual: "String".to_string(),
            severity: DriftSeverity::Breaking,
        };
        let d2 = Drift {
            path: "root.y".to_string(),
            kind: DriftKind::ExtraField,
            expected: "absent".to_string(),
            actual: "Int".to_string(),
            severity: DriftSeverity::Info,
        };
        let report = DriftReport::new(vec![d1, d2], "test", 1000);
        h.add_report(report);

        let breaking = h.breaking_drifts();
        assert_eq!(breaking.len(), 1);
        assert_eq!(breaking[0].path, "root.x");
    }

    #[test]
    fn history_has_breaking_true() {
        let mut h = DriftHistory::new(10);
        let d = Drift {
            path: "root".to_string(),
            kind: DriftKind::TypeMismatch,
            expected: "Int".to_string(),
            actual: "String".to_string(),
            severity: DriftSeverity::Breaking,
        };
        h.add_report(DriftReport::new(vec![d], "s", 100));
        assert!(h.has_breaking());
    }

    #[test]
    fn history_has_breaking_false() {
        let mut h = DriftHistory::new(10);
        let d = Drift {
            path: "root".to_string(),
            kind: DriftKind::ExtraField,
            expected: "absent".to_string(),
            actual: "Int".to_string(),
            severity: DriftSeverity::Info,
        };
        h.add_report(DriftReport::new(vec![d], "s", 100));
        assert!(!h.has_breaking());
    }

    #[test]
    fn history_drift_trend() {
        let mut h = DriftHistory::new(10);
        let d1 = Drift {
            path: "root.name".to_string(),
            kind: DriftKind::TypeMismatch,
            expected: "Int".to_string(),
            actual: "String".to_string(),
            severity: DriftSeverity::Breaking,
        };
        let d2 = Drift {
            path: "root.other".to_string(),
            kind: DriftKind::ExtraField,
            expected: "absent".to_string(),
            actual: "Bool".to_string(),
            severity: DriftSeverity::Info,
        };
        let d3 = Drift {
            path: "root.name".to_string(),
            kind: DriftKind::NullabilityChange,
            expected: "String".to_string(),
            actual: "Null".to_string(),
            severity: DriftSeverity::Breaking,
        };
        h.add_report(DriftReport::new(vec![d1, d2], "s", 100));
        h.add_report(DriftReport::new(vec![d3], "s", 200));

        let trend = h.drift_trend("root.name");
        assert_eq!(trend.len(), 2);
    }

    #[test]
    fn history_max_reports_pruning() {
        let mut h = DriftHistory::new(3);
        for i in 0..5 {
            h.add_report(DriftReport::new(vec![], &format!("schema_{i}"), i as u64));
        }
        assert_eq!(h.reports.len(), 3);
        // Oldest two should have been pruned.
        assert_eq!(h.reports[0].schema_name, "schema_2");
        assert_eq!(h.reports[1].schema_name, "schema_3");
        assert_eq!(h.reports[2].schema_name, "schema_4");
    }

    // -----------------------------------------------------------------------
    // Formatting
    // -----------------------------------------------------------------------

    #[test]
    fn format_drift_report_empty() {
        let report = DriftReport::new(vec![], "TestSchema", 5000);
        let formatted = format_drift_report(&report);
        assert!(formatted.contains("TestSchema"));
        assert!(formatted.contains("5000ms"));
        assert!(formatted.contains("No drifts detected"));
    }

    #[test]
    fn format_drift_report_with_drifts() {
        let d = Drift {
            path: "root.age".to_string(),
            kind: DriftKind::TypeMismatch,
            expected: "Int".to_string(),
            actual: "String".to_string(),
            severity: DriftSeverity::Breaking,
        };
        let report = DriftReport::new(vec![d], "Person", 1234);
        let formatted = format_drift_report(&report);
        assert!(formatted.contains("Person"));
        assert!(formatted.contains("1 drift(s)"));
        assert!(formatted.contains("BREAKING"));
        assert!(formatted.contains("root.age"));
    }

    #[test]
    fn format_drift_single() {
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
        assert!(formatted.contains("expected Int"));
        assert!(formatted.contains("got absent"));
    }

    // -----------------------------------------------------------------------
    // DriftSeverity ordering
    // -----------------------------------------------------------------------

    #[test]
    fn severity_ordering() {
        assert!(DriftSeverity::Info < DriftSeverity::Warning);
        assert!(DriftSeverity::Warning < DriftSeverity::Breaking);
        assert!(DriftSeverity::Info < DriftSeverity::Breaking);
    }

    #[test]
    fn severity_display() {
        assert_eq!(DriftSeverity::Info.to_string(), "INFO");
        assert_eq!(DriftSeverity::Warning.to_string(), "WARNING");
        assert_eq!(DriftSeverity::Breaking.to_string(), "BREAKING");
    }

    // -----------------------------------------------------------------------
    // DriftError
    // -----------------------------------------------------------------------

    #[test]
    fn drift_error_display() {
        let e1 = DriftError::InvalidSchema("bad record".to_string());
        assert!(e1.to_string().contains("invalid schema"));
        assert!(e1.to_string().contains("bad record"));

        let e2 = DriftError::ParseError("unexpected token".to_string());
        assert!(e2.to_string().contains("parse error"));

        let e3 = DriftError::ComparisonError("depth limit".to_string());
        assert!(e3.to_string().contains("comparison error"));
    }

    #[test]
    fn drift_error_is_std_error() {
        let e: Box<dyn std::error::Error> = Box::new(DriftError::ParseError("test".to_string()));
        assert!(e.to_string().contains("test"));
    }

    // -----------------------------------------------------------------------
    // SchemaType Display
    // -----------------------------------------------------------------------

    #[test]
    fn schema_type_display() {
        assert_eq!(SchemaType::String.to_string(), "String");
        assert_eq!(SchemaType::Int.to_string(), "Int");
        assert_eq!(SchemaType::Float.to_string(), "Float");
        assert_eq!(SchemaType::Bool.to_string(), "Bool");
        assert_eq!(SchemaType::Null.to_string(), "Null");
        assert_eq!(SchemaType::Any.to_string(), "Any");
        assert_eq!(
            SchemaType::List(Box::new(SchemaType::Int)).to_string(),
            "List[Int]"
        );
        assert_eq!(
            SchemaType::Optional(Box::new(SchemaType::String)).to_string(),
            "String?"
        );
        assert_eq!(
            SchemaType::Map(Box::new(SchemaType::String), Box::new(SchemaType::Int)).to_string(),
            "Map[String, Int]"
        );
    }

    // -----------------------------------------------------------------------
    // DriftKind Display
    // -----------------------------------------------------------------------

    #[test]
    fn drift_kind_display() {
        assert_eq!(DriftKind::TypeMismatch.to_string(), "TypeMismatch");
        assert_eq!(DriftKind::MissingField.to_string(), "MissingField");
        assert_eq!(DriftKind::ExtraField.to_string(), "ExtraField");
        assert_eq!(
            DriftKind::NullabilityChange.to_string(),
            "NullabilityChange"
        );
        assert_eq!(DriftKind::TypeWidened.to_string(), "TypeWidened");
        assert_eq!(DriftKind::TypeNarrowed.to_string(), "TypeNarrowed");
        assert_eq!(
            DriftKind::FieldRenamed { similarity: 0.85 }.to_string(),
            "FieldRenamed(similarity=0.85)"
        );
    }

    // -----------------------------------------------------------------------
    // DriftReport helpers
    // -----------------------------------------------------------------------

    #[test]
    fn drift_report_has_breaking() {
        let d = Drift {
            path: "root".to_string(),
            kind: DriftKind::TypeMismatch,
            expected: "Int".to_string(),
            actual: "String".to_string(),
            severity: DriftSeverity::Breaking,
        };
        let report = DriftReport::new(vec![d], "s", 0);
        assert!(report.has_breaking());
        assert_eq!(report.len(), 1);
        assert!(!report.is_empty());
    }

    #[test]
    fn drift_report_empty_no_breaking() {
        let report = DriftReport::new(vec![], "s", 0);
        assert!(!report.has_breaking());
        assert!(report.is_empty());
    }

    // -----------------------------------------------------------------------
    // Map type drift
    // -----------------------------------------------------------------------

    #[test]
    fn drift_map_value_type_mismatch() {
        let expected = SchemaType::Map(Box::new(SchemaType::String), Box::new(SchemaType::Int));
        let actual = SchemaType::Map(Box::new(SchemaType::String), Box::new(SchemaType::Float));
        let drifts = detect_drift(&expected, &actual, "root");
        assert_eq!(drifts.len(), 1);
        assert_eq!(drifts[0].path, "root<value>");
        assert_eq!(drifts[0].kind, DriftKind::TypeMismatch);
    }

    #[test]
    fn drift_map_key_type_mismatch() {
        let expected = SchemaType::Map(Box::new(SchemaType::String), Box::new(SchemaType::Int));
        let actual = SchemaType::Map(Box::new(SchemaType::Int), Box::new(SchemaType::Int));
        let drifts = detect_drift(&expected, &actual, "root");
        assert_eq!(drifts.len(), 1);
        assert_eq!(drifts[0].path, "root<key>");
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn drift_non_optional_vs_optional_actual() {
        let expected = SchemaType::Int;
        let actual = SchemaType::Optional(Box::new(SchemaType::Int));
        let drifts = detect_drift(&expected, &actual, "root");
        assert_eq!(drifts.len(), 1);
        assert_eq!(drifts[0].kind, DriftKind::NullabilityChange);
    }

    #[test]
    fn drift_multiple_issues_in_record() {
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
                    field_type: SchemaType::String, // mismatch
                    required: true,
                },
                // b missing
                SchemaField {
                    name: "c".to_string(), // extra
                    field_type: SchemaType::Bool,
                    required: true,
                },
            ],
        };
        let drifts = detect_drift(&expected, &actual, "root");
        // a: type mismatch, b: missing, c: extra
        assert_eq!(drifts.len(), 3);
        let kinds: Vec<&DriftKind> = drifts.iter().map(|d| &d.kind).collect();
        assert!(kinds.contains(&&DriftKind::TypeMismatch));
        assert!(kinds.contains(&&DriftKind::MissingField));
        assert!(kinds.contains(&&DriftKind::ExtraField));
    }

    #[test]
    fn value_json_float() {
        let schema = SchemaType::Float;
        let drifts = check_value_against_schema("3.14", &schema);
        assert!(drifts.is_empty());
    }

    #[test]
    fn value_json_bool() {
        let schema = SchemaType::Bool;
        let drifts = check_value_against_schema("true", &schema);
        assert!(drifts.is_empty());
    }

    #[test]
    fn value_json_nested_object() {
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
}
