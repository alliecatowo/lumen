//! CI-oriented output formats for Lumen diagnostics.
//!
//! Supports JUnit XML and JSON output for integration with CI/CD pipelines
//! (GitHub Actions, Jenkins, GitLab CI, etc.).

use std::path::Path;

// ---------------------------------------------------------------------------
// Output format enum
// ---------------------------------------------------------------------------

/// Supported output formats for CI integration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Default human-readable text output.
    Text,
    /// JUnit XML format (compatible with most CI systems).
    Junit,
    /// Structured JSON format.
    Json,
}

impl OutputFormat {
    /// Parse an output format from a string.
    pub fn from_str_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "text" => Some(OutputFormat::Text),
            "junit" | "junit-xml" | "xml" => Some(OutputFormat::Junit),
            "json" => Some(OutputFormat::Json),
            _ => None,
        }
    }

    /// Return the format names for help text.
    pub fn names() -> &'static [&'static str] {
        &["text", "junit", "json"]
    }
}

// ---------------------------------------------------------------------------
// Diagnostic types
// ---------------------------------------------------------------------------

/// Severity level for a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

impl std::fmt::Display for DiagnosticSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiagnosticSeverity::Error => write!(f, "error"),
            DiagnosticSeverity::Warning => write!(f, "warning"),
        }
    }
}

/// A single diagnostic (error or warning) from the compiler.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// The source file that produced this diagnostic.
    pub file: String,
    /// Line number (1-indexed).
    pub line: Option<usize>,
    /// Column number (1-indexed).
    pub column: Option<usize>,
    /// Severity level.
    pub severity: DiagnosticSeverity,
    /// Short message describing the issue.
    pub message: String,
    /// Optional longer description or source context.
    pub details: Option<String>,
}

/// Result of checking a single file.
#[derive(Debug, Clone)]
pub struct FileCheckResult {
    /// The source file path.
    pub file: String,
    /// Whether the check passed (no errors).
    pub passed: bool,
    /// Time taken to check this file, in seconds.
    pub duration_secs: f64,
    /// Diagnostics (errors and warnings) produced.
    pub diagnostics: Vec<Diagnostic>,
}

/// Aggregate result of checking multiple files.
#[derive(Debug, Clone)]
pub struct CheckReport {
    /// Name for the test suite (typically the command or project).
    pub suite_name: String,
    /// Individual file results.
    pub results: Vec<FileCheckResult>,
    /// Total time for all checks, in seconds.
    pub total_duration_secs: f64,
}

impl CheckReport {
    /// Create a new empty report.
    pub fn new(suite_name: &str) -> Self {
        Self {
            suite_name: suite_name.to_string(),
            results: Vec::new(),
            total_duration_secs: 0.0,
        }
    }

    /// Number of files that passed.
    pub fn passed_count(&self) -> usize {
        self.results.iter().filter(|r| r.passed).count()
    }

    /// Number of files that failed.
    pub fn failed_count(&self) -> usize {
        self.results.iter().filter(|r| !r.passed).count()
    }

    /// Total number of errors across all files.
    pub fn error_count(&self) -> usize {
        self.results
            .iter()
            .flat_map(|r| &r.diagnostics)
            .filter(|d| d.severity == DiagnosticSeverity::Error)
            .count()
    }

    /// Total number of warnings across all files.
    pub fn warning_count(&self) -> usize {
        self.results
            .iter()
            .flat_map(|r| &r.diagnostics)
            .filter(|d| d.severity == DiagnosticSeverity::Warning)
            .count()
    }
}

// ---------------------------------------------------------------------------
// JUnit XML output
// ---------------------------------------------------------------------------

/// Render a check report as JUnit XML.
///
/// Produces output conforming to the JUnit XML schema used by CI systems:
///
/// ```xml
/// <?xml version="1.0" encoding="UTF-8"?>
/// <testsuites>
///   <testsuite name="lumen-check" tests="3" failures="1" time="0.5">
///     <testcase name="src/main.lm.md" classname="lumen.check" time="0.1" />
///     <testcase name="src/utils.lm" classname="lumen.check" time="0.2">
///       <failure message="type error" type="error">details...</failure>
///     </testcase>
///   </testsuite>
/// </testsuites>
/// ```
pub fn render_junit_xml(report: &CheckReport) -> String {
    let mut xml = String::new();

    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<testsuites>\n");

    let total = report.results.len();
    let failures = report.failed_count();

    xml.push_str(&format!(
        "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" errors=\"0\" time=\"{:.3}\">\n",
        xml_escape(&report.suite_name),
        total,
        failures,
        report.total_duration_secs,
    ));

    for result in &report.results {
        let classname = classname_from_path(&result.file);

        if result.passed {
            xml.push_str(&format!(
                "    <testcase name=\"{}\" classname=\"{}\" time=\"{:.3}\" />\n",
                xml_escape(&result.file),
                xml_escape(&classname),
                result.duration_secs,
            ));
        } else {
            xml.push_str(&format!(
                "    <testcase name=\"{}\" classname=\"{}\" time=\"{:.3}\">\n",
                xml_escape(&result.file),
                xml_escape(&classname),
                result.duration_secs,
            ));

            for diag in &result.diagnostics {
                let tag = match diag.severity {
                    DiagnosticSeverity::Error => "failure",
                    DiagnosticSeverity::Warning => "failure",
                };
                let location = match (diag.line, diag.column) {
                    (Some(l), Some(c)) => format!("{}:{}:{}", diag.file, l, c),
                    (Some(l), None) => format!("{}:{}", diag.file, l),
                    _ => diag.file.clone(),
                };

                let detail_text = if let Some(ref details) = diag.details {
                    format!("{}\n\n{}", location, details)
                } else {
                    location
                };

                xml.push_str(&format!(
                    "      <{} message=\"{}\" type=\"{}\">{}</{}>\n",
                    tag,
                    xml_escape(&diag.message),
                    diag.severity,
                    xml_escape(&detail_text),
                    tag,
                ));
            }

            xml.push_str("    </testcase>\n");
        }
    }

    xml.push_str("  </testsuite>\n");
    xml.push_str("</testsuites>\n");

    xml
}

// ---------------------------------------------------------------------------
// JSON output
// ---------------------------------------------------------------------------

/// Render a check report as structured JSON.
///
/// ```json
/// {
///   "suite": "lumen-check",
///   "total": 3,
///   "passed": 2,
///   "failed": 1,
///   "errors": 1,
///   "warnings": 0,
///   "duration_secs": 0.5,
///   "results": [ ... ]
/// }
/// ```
pub fn render_json(report: &CheckReport) -> String {
    let results: Vec<serde_json::Value> = report
        .results
        .iter()
        .map(|r| {
            let diagnostics: Vec<serde_json::Value> = r
                .diagnostics
                .iter()
                .map(|d| {
                    let mut obj = serde_json::json!({
                        "file": d.file,
                        "severity": format!("{}", d.severity),
                        "message": d.message,
                    });
                    if let Some(line) = d.line {
                        obj["line"] = serde_json::json!(line);
                    }
                    if let Some(col) = d.column {
                        obj["column"] = serde_json::json!(col);
                    }
                    if let Some(ref details) = d.details {
                        obj["details"] = serde_json::json!(details);
                    }
                    obj
                })
                .collect();

            serde_json::json!({
                "file": r.file,
                "passed": r.passed,
                "duration_secs": r.duration_secs,
                "diagnostics": diagnostics,
            })
        })
        .collect();

    let output = serde_json::json!({
        "suite": report.suite_name,
        "total": report.results.len(),
        "passed": report.passed_count(),
        "failed": report.failed_count(),
        "errors": report.error_count(),
        "warnings": report.warning_count(),
        "duration_secs": report.total_duration_secs,
        "results": results,
    });

    serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
}

// ---------------------------------------------------------------------------
// Helper: extract diagnostic info from CompileError
// ---------------------------------------------------------------------------

/// Extract a diagnostic message and optional location from a compile error.
///
/// Parses the formatted error output to extract line/column information.
pub fn diagnostic_from_compile_error(
    error: &lumen_compiler::CompileError,
    source: &str,
    filename: &str,
) -> Diagnostic {
    let formatted = lumen_compiler::format_error(error, source, filename);
    let message = extract_error_summary(error);
    let (line, column) = extract_location_from_error(error);

    Diagnostic {
        file: filename.to_string(),
        line,
        column,
        severity: DiagnosticSeverity::Error,
        message,
        details: Some(formatted),
    }
}

/// Extract a concise summary message from a compile error.
fn extract_error_summary(error: &lumen_compiler::CompileError) -> String {
    // CompileError has a Display impl that we can use
    format!("{}", error)
}

/// Try to extract line/column from a compile error.
///
/// The CompileError types typically include span information. We parse the
/// formatted error for "line N" and "column N" patterns as a fallback.
fn extract_location_from_error(
    error: &lumen_compiler::CompileError,
) -> (Option<usize>, Option<usize>) {
    // Use the formatted error string to find line/column info.
    // Format is typically: "error[...] at line N, column M" or includes "line N"
    let text = format!("{:?}", error);

    let line = extract_number_after(&text, "line:");
    let col = extract_number_after(&text, "col:");

    // Also try span-based extraction from the Debug output
    let line = line.or_else(|| extract_span_field(&text, "start_line"));
    let col = col.or_else(|| extract_span_field(&text, "start_col"));

    (line, col)
}

/// Extract a number following a pattern like "field: N" from a debug string.
fn extract_number_after(text: &str, prefix: &str) -> Option<usize> {
    text.find(prefix).and_then(|i| {
        let rest = &text[i + prefix.len()..];
        let trimmed = rest.trim_start();
        let num_str: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
        num_str.parse().ok()
    })
}

/// Extract a span field value from a Debug representation.
fn extract_span_field(text: &str, field: &str) -> Option<usize> {
    let pattern = format!("{}: ", field);
    extract_number_after(text, &pattern)
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

/// Escape special XML characters.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Derive a JUnit classname from a file path.
///
/// Converts path separators to dots and drops the extension:
///   "src/utils/math.lm.md" -> "src.utils.math"
fn classname_from_path(path: &str) -> String {
    let p = Path::new(path);

    // Strip all known extensions
    let stem = p
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| {
            n.strip_suffix(".lm.md")
                .or_else(|| n.strip_suffix(".lumen.md"))
                .or_else(|| n.strip_suffix(".lm"))
                .or_else(|| n.strip_suffix(".lumen"))
                .unwrap_or(n)
        })
        .unwrap_or("");

    let parent = p.parent().and_then(|p| p.to_str()).unwrap_or("");

    if parent.is_empty() {
        format!("lumen.check.{}", stem)
    } else {
        let parent_dotted = parent.replace(['/', '\\'], ".");
        format!("{}.{}", parent_dotted, stem)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- OutputFormat tests -------------------------------------------------

    #[test]
    fn output_format_parsing() {
        assert_eq!(
            OutputFormat::from_str_name("text"),
            Some(OutputFormat::Text)
        );
        assert_eq!(
            OutputFormat::from_str_name("junit"),
            Some(OutputFormat::Junit)
        );
        assert_eq!(
            OutputFormat::from_str_name("junit-xml"),
            Some(OutputFormat::Junit)
        );
        assert_eq!(
            OutputFormat::from_str_name("xml"),
            Some(OutputFormat::Junit)
        );
        assert_eq!(
            OutputFormat::from_str_name("json"),
            Some(OutputFormat::Json)
        );
        assert_eq!(
            OutputFormat::from_str_name("TEXT"),
            Some(OutputFormat::Text)
        );
        assert_eq!(
            OutputFormat::from_str_name("JSON"),
            Some(OutputFormat::Json)
        );
        assert_eq!(OutputFormat::from_str_name("invalid"), None);
    }

    #[test]
    fn output_format_names() {
        let names = OutputFormat::names();
        assert!(names.contains(&"text"));
        assert!(names.contains(&"junit"));
        assert!(names.contains(&"json"));
    }

    // -- CheckReport tests -------------------------------------------------

    fn sample_report() -> CheckReport {
        let mut report = CheckReport::new("lumen-check");
        report.total_duration_secs = 0.567;
        report.results.push(FileCheckResult {
            file: "src/main.lm.md".to_string(),
            passed: true,
            duration_secs: 0.123,
            diagnostics: vec![],
        });
        report.results.push(FileCheckResult {
            file: "src/utils.lm".to_string(),
            passed: false,
            duration_secs: 0.234,
            diagnostics: vec![Diagnostic {
                file: "src/utils.lm".to_string(),
                line: Some(10),
                column: Some(5),
                severity: DiagnosticSeverity::Error,
                message: "undefined variable 'x'".to_string(),
                details: Some("let y = x + 1\n        ^".to_string()),
            }],
        });
        report.results.push(FileCheckResult {
            file: "src/lib.lm".to_string(),
            passed: false,
            duration_secs: 0.210,
            diagnostics: vec![
                Diagnostic {
                    file: "src/lib.lm".to_string(),
                    line: Some(3),
                    column: None,
                    severity: DiagnosticSeverity::Error,
                    message: "type mismatch".to_string(),
                    details: None,
                },
                Diagnostic {
                    file: "src/lib.lm".to_string(),
                    line: Some(15),
                    column: Some(1),
                    severity: DiagnosticSeverity::Warning,
                    message: "unused variable 'z'".to_string(),
                    details: None,
                },
            ],
        });
        report
    }

    #[test]
    fn report_counts() {
        let report = sample_report();
        assert_eq!(report.passed_count(), 1);
        assert_eq!(report.failed_count(), 2);
        assert_eq!(report.error_count(), 2);
        assert_eq!(report.warning_count(), 1);
    }

    // -- JUnit XML tests ---------------------------------------------------

    #[test]
    fn junit_xml_well_formed() {
        let report = sample_report();
        let xml = render_junit_xml(&report);

        // Basic XML structure checks
        assert!(xml.starts_with("<?xml version=\"1.0\""));
        assert!(xml.contains("<testsuites>"));
        assert!(xml.contains("</testsuites>"));
        assert!(xml.contains("<testsuite"));
        assert!(xml.contains("</testsuite>"));
    }

    #[test]
    fn junit_xml_testsuite_attributes() {
        let report = sample_report();
        let xml = render_junit_xml(&report);

        assert!(xml.contains("name=\"lumen-check\""));
        assert!(xml.contains("tests=\"3\""));
        assert!(xml.contains("failures=\"2\""));
        assert!(xml.contains("errors=\"0\""));
    }

    #[test]
    fn junit_xml_passing_testcase() {
        let report = sample_report();
        let xml = render_junit_xml(&report);

        // Passing test should be self-closing with no failure element
        assert!(xml.contains("name=\"src/main.lm.md\""));
        // Check for self-closing tag (passing test)
        assert!(
            xml.contains("<testcase name=\"src/main.lm.md\"") && xml.contains("time=\"0.123\" />")
        );
    }

    #[test]
    fn junit_xml_failing_testcase() {
        let report = sample_report();
        let xml = render_junit_xml(&report);

        // Failing test should have a failure element
        assert!(xml.contains("<failure message=\"undefined variable &apos;x&apos;\""));
        assert!(xml.contains("type=\"error\""));
        assert!(xml.contains("</failure>"));
    }

    #[test]
    fn junit_xml_multiple_failures() {
        let report = sample_report();
        let xml = render_junit_xml(&report);

        // src/lib.lm has 2 diagnostics
        assert!(xml.contains("type mismatch"));
        assert!(xml.contains("unused variable"));
    }

    #[test]
    fn junit_xml_empty_report() {
        let report = CheckReport::new("empty");
        let xml = render_junit_xml(&report);

        assert!(xml.contains("tests=\"0\""));
        assert!(xml.contains("failures=\"0\""));
    }

    #[test]
    fn junit_xml_escapes_special_chars() {
        let mut report = CheckReport::new("test<>&\"'");
        report.results.push(FileCheckResult {
            file: "file<with>.lm".to_string(),
            passed: false,
            duration_secs: 0.0,
            diagnostics: vec![Diagnostic {
                file: "file<with>.lm".to_string(),
                line: None,
                column: None,
                severity: DiagnosticSeverity::Error,
                message: "error with <special> & \"chars\"".to_string(),
                details: None,
            }],
        });
        let xml = render_junit_xml(&report);

        assert!(xml.contains("&lt;"));
        assert!(xml.contains("&gt;"));
        assert!(xml.contains("&amp;"));
        assert!(xml.contains("&quot;"));
        // No raw < or > inside attribute values
        assert!(!xml.contains("name=\"test<"));
    }

    // -- JSON output tests -------------------------------------------------

    #[test]
    fn json_output_valid() {
        let report = sample_report();
        let json_str = render_json(&report);
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("should be valid JSON");

        assert_eq!(parsed["suite"], "lumen-check");
        assert_eq!(parsed["total"], 3);
        assert_eq!(parsed["passed"], 1);
        assert_eq!(parsed["failed"], 2);
        assert_eq!(parsed["errors"], 2);
        assert_eq!(parsed["warnings"], 1);
    }

    #[test]
    fn json_output_results_structure() {
        let report = sample_report();
        let json_str = render_json(&report);
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("should be valid JSON");

        let results = parsed["results"]
            .as_array()
            .expect("results should be array");
        assert_eq!(results.len(), 3);

        // First result: passed
        assert_eq!(results[0]["file"], "src/main.lm.md");
        assert_eq!(results[0]["passed"], true);
        assert_eq!(
            results[0]["diagnostics"]
                .as_array()
                .expect("diagnostics array")
                .len(),
            0
        );

        // Second result: failed with diagnostic
        assert_eq!(results[1]["file"], "src/utils.lm");
        assert_eq!(results[1]["passed"], false);
        let diags = results[1]["diagnostics"]
            .as_array()
            .expect("diagnostics array");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0]["line"], 10);
        assert_eq!(diags[0]["column"], 5);
        assert_eq!(diags[0]["severity"], "error");
        assert_eq!(diags[0]["message"], "undefined variable 'x'");
    }

    #[test]
    fn json_output_empty_report() {
        let report = CheckReport::new("empty");
        let json_str = render_json(&report);
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("should be valid JSON");

        assert_eq!(parsed["total"], 0);
        assert_eq!(parsed["passed"], 0);
        assert_eq!(parsed["failed"], 0);
    }

    // -- classname_from_path tests -----------------------------------------

    #[test]
    fn classname_simple() {
        assert_eq!(classname_from_path("main.lm"), "lumen.check.main");
    }

    #[test]
    fn classname_with_directory() {
        assert_eq!(classname_from_path("src/utils/math.lm"), "src.utils.math");
    }

    #[test]
    fn classname_markdown_extension() {
        assert_eq!(classname_from_path("src/main.lm.md"), "src.main");
    }

    #[test]
    fn classname_lumen_extension() {
        assert_eq!(classname_from_path("lib/core.lumen"), "lib.core");
    }

    #[test]
    fn classname_lumen_md_extension() {
        assert_eq!(classname_from_path("src/types.lumen.md"), "src.types");
    }

    // -- xml_escape tests --------------------------------------------------

    #[test]
    fn xml_escape_all_chars() {
        assert_eq!(
            xml_escape("a<b>c&d\"e'f"),
            "a&lt;b&gt;c&amp;d&quot;e&apos;f"
        );
    }

    #[test]
    fn xml_escape_no_special() {
        assert_eq!(xml_escape("hello world"), "hello world");
    }

    // -- DiagnosticSeverity tests ------------------------------------------

    #[test]
    fn severity_display() {
        assert_eq!(format!("{}", DiagnosticSeverity::Error), "error");
        assert_eq!(format!("{}", DiagnosticSeverity::Warning), "warning");
    }

    // -- Location extraction tests -----------------------------------------

    #[test]
    fn extract_number_after_basic() {
        assert_eq!(extract_number_after("line: 42 col: 5", "line:"), Some(42));
        assert_eq!(extract_number_after("line: 42 col: 5", "col:"), Some(5));
        assert_eq!(extract_number_after("no numbers here", "line:"), None);
    }
}
