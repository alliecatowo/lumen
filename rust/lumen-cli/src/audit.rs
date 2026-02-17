//! Security audit module for Lumen dependencies.
//!
//! This module provides a framework for auditing project dependencies
//! against known security vulnerabilities. It parses `Cargo.lock` files
//! to extract dependency information and checks them against a local
//! advisory database.
//!
//! ## Usage
//!
//! ```text
//! lumen audit                    # Audit current project
//! lumen audit --format json      # Output as JSON
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

// =============================================================================
// Severity Levels
// =============================================================================

/// CVSS-based severity classification for vulnerabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational — no direct security impact.
    None,
    /// Low severity (CVSS 0.1–3.9).
    Low,
    /// Medium severity (CVSS 4.0–6.9).
    Medium,
    /// High severity (CVSS 7.0–8.9).
    High,
    /// Critical severity (CVSS 9.0–10.0).
    Critical,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::None => write!(f, "none"),
            Severity::Low => write!(f, "low"),
            Severity::Medium => write!(f, "medium"),
            Severity::High => write!(f, "high"),
            Severity::Critical => write!(f, "critical"),
        }
    }
}

impl std::str::FromStr for Severity {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(Severity::None),
            "low" => Ok(Severity::Low),
            "medium" | "moderate" => Ok(Severity::Medium),
            "high" => Ok(Severity::High),
            "critical" => Ok(Severity::Critical),
            _ => Err(format!("invalid severity: '{}'", s)),
        }
    }
}

// =============================================================================
// Advisory
// =============================================================================

/// A security advisory for a specific crate/package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Advisory {
    /// Advisory identifier (e.g., "RUSTSEC-2023-0001" or "CVE-2023-12345").
    pub id: String,

    /// Affected crate/package name.
    pub package: String,

    /// Human-readable title/summary of the vulnerability.
    pub title: String,

    /// Detailed description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Severity level.
    pub severity: Severity,

    /// CVE identifiers (may have multiple).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cve_ids: Vec<String>,

    /// URL to the advisory details.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Affected version ranges (simplified — e.g., "< 1.2.3", ">= 2.0.0, < 2.1.0").
    #[serde(default)]
    pub affected_versions: Vec<String>,

    /// Fixed version (if a patch is available).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patched_version: Option<String>,

    /// Date the advisory was published.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

impl Advisory {
    /// Check if a specific version is affected by this advisory.
    ///
    /// Uses simple semver prefix matching. For a production implementation,
    /// use the `semver` crate for proper range evaluation.
    pub fn affects_version(&self, version: &str) -> bool {
        if self.affected_versions.is_empty() {
            // If no version constraints, assume all versions affected
            return true;
        }

        for constraint in &self.affected_versions {
            if version_matches_constraint(version, constraint) {
                return true;
            }
        }

        false
    }
}

// =============================================================================
// Parsed Dependency (from Cargo.lock)
// =============================================================================

/// A dependency parsed from a Cargo.lock file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ParsedDependency {
    /// Crate name.
    pub name: String,

    /// Exact version.
    pub version: String,

    /// Source (e.g., registry URL, git URL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    /// Checksum (if available).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,

    /// Direct dependencies of this package.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
}

// =============================================================================
// Audit Result
// =============================================================================

/// The result of a security audit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditResult {
    /// Total number of dependencies scanned.
    pub dependencies_scanned: usize,

    /// Vulnerabilities found.
    pub vulnerabilities: Vec<VulnerabilityFinding>,

    /// Warnings (non-vulnerability issues like yanked crates).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<AuditWarning>,

    /// Summary counts by severity.
    pub summary: AuditSummary,
}

impl AuditResult {
    /// Check if the audit found any vulnerabilities.
    pub fn has_vulnerabilities(&self) -> bool {
        !self.vulnerabilities.is_empty()
    }

    /// Check if the audit found critical or high severity issues.
    pub fn has_critical_or_high(&self) -> bool {
        self.vulnerabilities
            .iter()
            .any(|v| matches!(v.severity, Severity::Critical | Severity::High))
    }

    /// Get all vulnerabilities of a specific severity or higher.
    pub fn vulnerabilities_at_or_above(
        &self,
        min_severity: Severity,
    ) -> Vec<&VulnerabilityFinding> {
        self.vulnerabilities
            .iter()
            .filter(|v| v.severity >= min_severity)
            .collect()
    }
}

/// A specific vulnerability finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilityFinding {
    /// The advisory that matched.
    pub advisory_id: String,

    /// Affected package name.
    pub package: String,

    /// Installed version.
    pub installed_version: String,

    /// Title of the vulnerability.
    pub title: String,

    /// Severity level.
    pub severity: Severity,

    /// CVE identifiers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cve_ids: Vec<String>,

    /// URL for more information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Recommended patched version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patched_version: Option<String>,
}

/// Summary of audit findings by severity.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditSummary {
    pub critical: usize,
    pub high: usize,
    pub medium: usize,
    pub low: usize,
    pub none: usize,
    pub warnings: usize,
}

/// A non-vulnerability warning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditWarning {
    /// Warning type (e.g., "yanked", "unmaintained", "unsound").
    pub kind: String,

    /// Affected package.
    pub package: String,

    /// Version.
    pub version: String,

    /// Warning message.
    pub message: String,
}

// =============================================================================
// Audit Error
// =============================================================================

/// Errors that can occur during auditing.
#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("Failed to read lockfile: {0}")]
    LockfileReadError(String),

    #[error("Failed to parse lockfile: {0}")]
    LockfileParseError(String),

    #[error("Advisory database error: {0}")]
    DatabaseError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// =============================================================================
// Cargo.lock Parsing
// =============================================================================

/// Parse a Cargo.lock file into a list of dependencies.
///
/// Supports both v1 (with `[metadata]`) and v2/v3 (with `checksum` per package) formats.
pub fn parse_cargo_lock(content: &str) -> Result<Vec<ParsedDependency>, AuditError> {
    // Cargo.lock is a TOML file with [[package]] entries
    let parsed: toml::Value = content
        .parse()
        .map_err(|e| AuditError::LockfileParseError(format!("Invalid TOML: {}", e)))?;

    let packages = parsed
        .get("package")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            AuditError::LockfileParseError("Missing [[package]] entries in Cargo.lock".to_string())
        })?;

    let mut deps = Vec::new();

    for pkg in packages {
        let name = pkg.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
            AuditError::LockfileParseError("Package missing 'name' field".to_string())
        })?;

        let version = pkg.get("version").and_then(|v| v.as_str()).ok_or_else(|| {
            AuditError::LockfileParseError(format!("Package '{}' missing 'version' field", name))
        })?;

        let source = pkg.get("source").and_then(|v| v.as_str()).map(String::from);
        let checksum = pkg
            .get("checksum")
            .and_then(|v| v.as_str())
            .map(String::from);

        let dependencies = pkg
            .get("dependencies")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        deps.push(ParsedDependency {
            name: name.to_string(),
            version: version.to_string(),
            source,
            checksum,
            dependencies,
        });
    }

    Ok(deps)
}

/// Parse a Cargo.lock file from a path.
pub fn parse_cargo_lock_file(path: &Path) -> Result<Vec<ParsedDependency>, AuditError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| AuditError::LockfileReadError(format!("{}: {}", path.display(), e)))?;
    parse_cargo_lock(&content)
}

// =============================================================================
// Advisory Database
// =============================================================================

/// An in-memory advisory database for checking vulnerabilities.
///
/// In a production implementation, this would sync from RustSec or another
/// advisory feed. For now, it supports manual loading and built-in test advisories.
#[derive(Debug, Clone, Default)]
pub struct AdvisoryDatabase {
    /// Advisories indexed by package name.
    advisories: HashMap<String, Vec<Advisory>>,
}

impl AdvisoryDatabase {
    /// Create an empty advisory database.
    pub fn new() -> Self {
        Self {
            advisories: HashMap::new(),
        }
    }

    /// Add an advisory to the database.
    pub fn add_advisory(&mut self, advisory: Advisory) {
        self.advisories
            .entry(advisory.package.clone())
            .or_default()
            .push(advisory);
    }

    /// Load advisories from a JSON file.
    pub fn load_from_json(content: &str) -> Result<Self, AuditError> {
        let advisories: Vec<Advisory> = serde_json::from_str(content)
            .map_err(|e| AuditError::DatabaseError(format!("Failed to parse advisories: {}", e)))?;

        let mut db = Self::new();
        for advisory in advisories {
            db.add_advisory(advisory);
        }
        Ok(db)
    }

    /// Get advisories for a specific package.
    pub fn get_advisories(&self, package: &str) -> &[Advisory] {
        self.advisories
            .get(package)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get total number of advisories.
    pub fn advisory_count(&self) -> usize {
        self.advisories.values().map(|v| v.len()).sum()
    }

    /// Get number of packages with advisories.
    pub fn affected_package_count(&self) -> usize {
        self.advisories.len()
    }
}

// =============================================================================
// Core Audit Function
// =============================================================================

/// Run a security audit against a set of dependencies.
pub fn run_audit(dependencies: &[ParsedDependency], database: &AdvisoryDatabase) -> AuditResult {
    let mut vulnerabilities = Vec::new();
    let mut warnings = Vec::new();
    let mut summary = AuditSummary::default();

    for dep in dependencies {
        // Check for known vulnerabilities
        let advisories = database.get_advisories(&dep.name);
        for advisory in advisories {
            if advisory.affects_version(&dep.version) {
                let finding = VulnerabilityFinding {
                    advisory_id: advisory.id.clone(),
                    package: dep.name.clone(),
                    installed_version: dep.version.clone(),
                    title: advisory.title.clone(),
                    severity: advisory.severity,
                    cve_ids: advisory.cve_ids.clone(),
                    url: advisory.url.clone(),
                    patched_version: advisory.patched_version.clone(),
                };

                match advisory.severity {
                    Severity::Critical => summary.critical += 1,
                    Severity::High => summary.high += 1,
                    Severity::Medium => summary.medium += 1,
                    Severity::Low => summary.low += 1,
                    Severity::None => summary.none += 1,
                }

                vulnerabilities.push(finding);
            }
        }

        // Check for yanked crates (source contains "registry" but no checksum)
        if dep.source.as_deref().unwrap_or("").contains("registry")
            && dep.checksum.is_none()
            && !dep.source.as_deref().unwrap_or("").is_empty()
        {
            warnings.push(AuditWarning {
                kind: "missing-checksum".to_string(),
                package: dep.name.clone(),
                version: dep.version.clone(),
                message: format!(
                    "Registry dependency '{}@{}' has no checksum — may indicate a yanked or tampered package",
                    dep.name, dep.version
                ),
            });
            summary.warnings += 1;
        }
    }

    // Sort findings by severity (critical first)
    vulnerabilities.sort_by(|a, b| b.severity.cmp(&a.severity));

    AuditResult {
        dependencies_scanned: dependencies.len(),
        vulnerabilities,
        warnings,
        summary,
    }
}

// =============================================================================
// Report Formatting
// =============================================================================

/// Format an audit result as a human-readable report.
pub fn format_audit_report(result: &AuditResult) -> String {
    let mut output = String::new();

    // Header
    output.push_str(&format!("Security Audit Report\n{}\n\n", "=".repeat(50)));

    output.push_str(&format!(
        "Dependencies scanned: {}\n\n",
        result.dependencies_scanned
    ));

    // Vulnerabilities
    if result.vulnerabilities.is_empty() {
        output.push_str("No vulnerabilities found.\n");
    } else {
        output.push_str(&format!(
            "Found {} vulnerability(ies):\n\n",
            result.vulnerabilities.len()
        ));

        for (i, vuln) in result.vulnerabilities.iter().enumerate() {
            output.push_str(&format!(
                "  {}. [{}] {}\n",
                i + 1,
                severity_label(vuln.severity),
                vuln.title
            ));
            output.push_str(&format!(
                "     Package: {}@{}\n",
                vuln.package, vuln.installed_version
            ));
            output.push_str(&format!("     Advisory: {}\n", vuln.advisory_id));

            if !vuln.cve_ids.is_empty() {
                output.push_str(&format!("     CVE: {}\n", vuln.cve_ids.join(", ")));
            }

            if let Some(ref url) = vuln.url {
                output.push_str(&format!("     URL: {}\n", url));
            }

            if let Some(ref patched) = vuln.patched_version {
                output.push_str(&format!("     Fix: Upgrade to >= {}\n", patched));
            }

            output.push('\n');
        }
    }

    // Warnings
    if !result.warnings.is_empty() {
        output.push_str(&format!("Warnings ({}):\n\n", result.warnings.len()));

        for warning in &result.warnings {
            output.push_str(&format!(
                "  [{}] {}@{}: {}\n",
                warning.kind, warning.package, warning.version, warning.message
            ));
        }
        output.push('\n');
    }

    // Summary
    output.push_str(&format!("Summary\n{}\n", "-".repeat(30)));
    output.push_str(&format!("  Critical: {}\n", result.summary.critical));
    output.push_str(&format!("  High:     {}\n", result.summary.high));
    output.push_str(&format!("  Medium:   {}\n", result.summary.medium));
    output.push_str(&format!("  Low:      {}\n", result.summary.low));
    output.push_str(&format!("  Warnings: {}\n", result.summary.warnings));

    output
}

/// Format an audit result as JSON.
pub fn format_audit_report_json(result: &AuditResult) -> Result<String, String> {
    serde_json::to_string_pretty(result)
        .map_err(|e| format!("Failed to serialize audit report: {}", e))
}

/// Get a human-readable severity label.
fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "CRITICAL",
        Severity::High => "HIGH",
        Severity::Medium => "MEDIUM",
        Severity::Low => "LOW",
        Severity::None => "INFO",
    }
}

// =============================================================================
// Version Matching
// =============================================================================

/// Simple version constraint matching.
///
/// Supports:
/// - `< 1.2.3` — less than
/// - `<= 1.2.3` — less than or equal
/// - `> 1.2.3` — greater than
/// - `>= 1.2.3` — greater than or equal
/// - `= 1.2.3` — exact match
/// - `>= 1.0.0, < 2.0.0` — range (comma-separated, all must match)
/// - `*` — matches everything
fn version_matches_constraint(version: &str, constraint: &str) -> bool {
    let constraint = constraint.trim();

    if constraint == "*" {
        return true;
    }

    // Handle comma-separated constraints (AND logic)
    if constraint.contains(',') {
        return constraint
            .split(',')
            .all(|c| version_matches_constraint(version, c.trim()));
    }

    // Parse operator and version
    let (op, target) = if let Some(rest) = constraint.strip_prefix("<=") {
        ("<=", rest.trim())
    } else if let Some(rest) = constraint.strip_prefix(">=") {
        (">=", rest.trim())
    } else if let Some(rest) = constraint.strip_prefix('<') {
        ("<", rest.trim())
    } else if let Some(rest) = constraint.strip_prefix('>') {
        (">", rest.trim())
    } else if let Some(rest) = constraint.strip_prefix('=') {
        ("=", rest.trim())
    } else {
        // No operator — treat as exact match
        ("=", constraint)
    };

    let cmp = compare_versions(version, target);

    match op {
        "<" => cmp < 0,
        "<=" => cmp <= 0,
        ">" => cmp > 0,
        ">=" => cmp >= 0,
        "=" => cmp == 0,
        _ => false,
    }
}

/// Compare two semver-style version strings.
///
/// Returns:
/// - negative if a < b
/// - 0 if a == b
/// - positive if a > b
fn compare_versions(a: &str, b: &str) -> i32 {
    let a_parts: Vec<u64> = a
        .split('.')
        .map(|s| {
            // Strip pre-release suffix (e.g., "1-beta" -> 1)
            let numeric = s.split('-').next().unwrap_or(s);
            numeric.parse().unwrap_or(0)
        })
        .collect();

    let b_parts: Vec<u64> = b
        .split('.')
        .map(|s| {
            let numeric = s.split('-').next().unwrap_or(s);
            numeric.parse().unwrap_or(0)
        })
        .collect();

    let max_len = a_parts.len().max(b_parts.len());

    for i in 0..max_len {
        let a_val = a_parts.get(i).copied().unwrap_or(0);
        let b_val = b_parts.get(i).copied().unwrap_or(0);

        if a_val < b_val {
            return -1;
        }
        if a_val > b_val {
            return 1;
        }
    }

    0
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Version comparison tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_compare_versions_equal() {
        assert_eq!(compare_versions("1.2.3", "1.2.3"), 0);
        assert_eq!(compare_versions("0.0.0", "0.0.0"), 0);
        assert_eq!(compare_versions("10.20.30", "10.20.30"), 0);
    }

    #[test]
    fn test_compare_versions_less() {
        assert!(compare_versions("1.2.3", "1.2.4") < 0);
        assert!(compare_versions("1.2.3", "1.3.0") < 0);
        assert!(compare_versions("1.2.3", "2.0.0") < 0);
        assert!(compare_versions("0.9.9", "1.0.0") < 0);
    }

    #[test]
    fn test_compare_versions_greater() {
        assert!(compare_versions("1.2.4", "1.2.3") > 0);
        assert!(compare_versions("2.0.0", "1.9.9") > 0);
    }

    #[test]
    fn test_compare_versions_different_lengths() {
        assert_eq!(compare_versions("1.2", "1.2.0"), 0);
        assert!(compare_versions("1.2", "1.2.1") < 0);
    }

    // -------------------------------------------------------------------------
    // Version constraint tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_version_matches_exact() {
        assert!(version_matches_constraint("1.2.3", "= 1.2.3"));
        assert!(!version_matches_constraint("1.2.4", "= 1.2.3"));
    }

    #[test]
    fn test_version_matches_less_than() {
        assert!(version_matches_constraint("1.2.2", "< 1.2.3"));
        assert!(!version_matches_constraint("1.2.3", "< 1.2.3"));
        assert!(!version_matches_constraint("1.2.4", "< 1.2.3"));
    }

    #[test]
    fn test_version_matches_less_or_equal() {
        assert!(version_matches_constraint("1.2.2", "<= 1.2.3"));
        assert!(version_matches_constraint("1.2.3", "<= 1.2.3"));
        assert!(!version_matches_constraint("1.2.4", "<= 1.2.3"));
    }

    #[test]
    fn test_version_matches_greater_than() {
        assert!(version_matches_constraint("1.2.4", "> 1.2.3"));
        assert!(!version_matches_constraint("1.2.3", "> 1.2.3"));
    }

    #[test]
    fn test_version_matches_greater_or_equal() {
        assert!(version_matches_constraint("1.2.3", ">= 1.2.3"));
        assert!(version_matches_constraint("1.2.4", ">= 1.2.3"));
        assert!(!version_matches_constraint("1.2.2", ">= 1.2.3"));
    }

    #[test]
    fn test_version_matches_range() {
        assert!(version_matches_constraint("1.5.0", ">= 1.0.0, < 2.0.0"));
        assert!(version_matches_constraint("1.0.0", ">= 1.0.0, < 2.0.0"));
        assert!(!version_matches_constraint("2.0.0", ">= 1.0.0, < 2.0.0"));
        assert!(!version_matches_constraint("0.9.0", ">= 1.0.0, < 2.0.0"));
    }

    #[test]
    fn test_version_matches_wildcard() {
        assert!(version_matches_constraint("1.2.3", "*"));
        assert!(version_matches_constraint("0.0.1", "*"));
    }

    // -------------------------------------------------------------------------
    // Severity tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_severity_display() {
        assert_eq!(Severity::Critical.to_string(), "critical");
        assert_eq!(Severity::High.to_string(), "high");
        assert_eq!(Severity::Medium.to_string(), "medium");
        assert_eq!(Severity::Low.to_string(), "low");
        assert_eq!(Severity::None.to_string(), "none");
    }

    #[test]
    fn test_severity_from_str() {
        assert_eq!("critical".parse::<Severity>(), Ok(Severity::Critical));
        assert_eq!("HIGH".parse::<Severity>(), Ok(Severity::High));
        assert_eq!("moderate".parse::<Severity>(), Ok(Severity::Medium));
        assert!("invalid".parse::<Severity>().is_err());
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Critical > Severity::High);
        assert!(Severity::High > Severity::Medium);
        assert!(Severity::Medium > Severity::Low);
        assert!(Severity::Low > Severity::None);
    }

    // -------------------------------------------------------------------------
    // Cargo.lock parsing tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_cargo_lock_v2() {
        let content = r#"
# This file is automatically @generated by Cargo.
# It is not intended for manual editing.
version = 3

[[package]]
name = "serde"
version = "1.0.193"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "abc123"
dependencies = [
 "serde_derive",
]

[[package]]
name = "serde_derive"
version = "1.0.193"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "def456"
dependencies = [
 "proc-macro2",
 "quote",
 "syn",
]

[[package]]
name = "my-app"
version = "0.1.0"
dependencies = [
 "serde",
]
"#;

        let deps = parse_cargo_lock(content).unwrap();
        assert_eq!(deps.len(), 3);

        let serde = deps.iter().find(|d| d.name == "serde").unwrap();
        assert_eq!(serde.version, "1.0.193");
        assert!(serde.source.as_ref().unwrap().contains("crates.io"));
        assert_eq!(serde.checksum, Some("abc123".to_string()));
        assert_eq!(serde.dependencies, vec!["serde_derive"]);

        let my_app = deps.iter().find(|d| d.name == "my-app").unwrap();
        assert_eq!(my_app.version, "0.1.0");
        assert!(my_app.source.is_none());
        assert!(my_app.checksum.is_none());
    }

    #[test]
    fn test_parse_cargo_lock_empty() {
        let result = parse_cargo_lock("invalid toml {{{");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_cargo_lock_no_packages() {
        let content = r#"
version = 3
"#;
        let result = parse_cargo_lock(content);
        assert!(result.is_err());
    }

    // -------------------------------------------------------------------------
    // Advisory database tests
    // -------------------------------------------------------------------------

    fn sample_advisory() -> Advisory {
        Advisory {
            id: "RUSTSEC-2023-0001".to_string(),
            package: "vulnerable-crate".to_string(),
            title: "Memory safety issue in vulnerable-crate".to_string(),
            description: Some("A use-after-free vulnerability...".to_string()),
            severity: Severity::High,
            cve_ids: vec!["CVE-2023-12345".to_string()],
            url: Some("https://rustsec.org/advisories/RUSTSEC-2023-0001".to_string()),
            affected_versions: vec!["< 1.5.0".to_string()],
            patched_version: Some("1.5.0".to_string()),
            date: Some("2023-06-15".to_string()),
        }
    }

    #[test]
    fn test_advisory_database_add_and_get() {
        let mut db = AdvisoryDatabase::new();
        assert_eq!(db.advisory_count(), 0);

        db.add_advisory(sample_advisory());
        assert_eq!(db.advisory_count(), 1);
        assert_eq!(db.affected_package_count(), 1);

        let advisories = db.get_advisories("vulnerable-crate");
        assert_eq!(advisories.len(), 1);
        assert_eq!(advisories[0].id, "RUSTSEC-2023-0001");

        let empty = db.get_advisories("safe-crate");
        assert!(empty.is_empty());
    }

    #[test]
    fn test_advisory_database_load_json() {
        let json = r#"[
            {
                "id": "TEST-001",
                "package": "test-crate",
                "title": "Test vulnerability",
                "severity": "medium",
                "affected_versions": ["< 2.0.0"]
            }
        ]"#;

        let db = AdvisoryDatabase::load_from_json(json).unwrap();
        assert_eq!(db.advisory_count(), 1);
        assert_eq!(db.get_advisories("test-crate").len(), 1);
    }

    #[test]
    fn test_advisory_affects_version() {
        let advisory = sample_advisory();

        // Version 1.4.0 is < 1.5.0 → affected
        assert!(advisory.affects_version("1.4.0"));
        assert!(advisory.affects_version("1.0.0"));
        assert!(advisory.affects_version("0.9.0"));

        // Version 1.5.0 is NOT < 1.5.0 → not affected
        assert!(!advisory.affects_version("1.5.0"));
        assert!(!advisory.affects_version("2.0.0"));
    }

    #[test]
    fn test_advisory_no_version_constraints() {
        let advisory = Advisory {
            id: "TEST-002".to_string(),
            package: "bad-crate".to_string(),
            title: "All versions affected".to_string(),
            description: None,
            severity: Severity::Critical,
            cve_ids: vec![],
            url: None,
            affected_versions: vec![],
            patched_version: None,
            date: None,
        };

        // No constraints means all versions affected
        assert!(advisory.affects_version("1.0.0"));
        assert!(advisory.affects_version("99.99.99"));
    }

    // -------------------------------------------------------------------------
    // Core audit tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_run_audit_no_vulnerabilities() {
        let deps = vec![ParsedDependency {
            name: "safe-crate".to_string(),
            version: "1.0.0".to_string(),
            source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
            checksum: Some("abc123".to_string()),
            dependencies: vec![],
        }];

        let db = AdvisoryDatabase::new();
        let result = run_audit(&deps, &db);

        assert_eq!(result.dependencies_scanned, 1);
        assert!(!result.has_vulnerabilities());
        assert!(!result.has_critical_or_high());
        assert_eq!(result.summary.critical, 0);
    }

    #[test]
    fn test_run_audit_with_vulnerability() {
        let deps = vec![
            ParsedDependency {
                name: "vulnerable-crate".to_string(),
                version: "1.4.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                checksum: Some("abc123".to_string()),
                dependencies: vec![],
            },
            ParsedDependency {
                name: "safe-crate".to_string(),
                version: "2.0.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                checksum: Some("def456".to_string()),
                dependencies: vec![],
            },
        ];

        let mut db = AdvisoryDatabase::new();
        db.add_advisory(sample_advisory());

        let result = run_audit(&deps, &db);

        assert_eq!(result.dependencies_scanned, 2);
        assert!(result.has_vulnerabilities());
        assert!(result.has_critical_or_high());
        assert_eq!(result.vulnerabilities.len(), 1);
        assert_eq!(result.summary.high, 1);

        let vuln = &result.vulnerabilities[0];
        assert_eq!(vuln.package, "vulnerable-crate");
        assert_eq!(vuln.installed_version, "1.4.0");
        assert_eq!(vuln.patched_version, Some("1.5.0".to_string()));
    }

    #[test]
    fn test_run_audit_patched_version_not_flagged() {
        let deps = vec![ParsedDependency {
            name: "vulnerable-crate".to_string(),
            version: "1.5.0".to_string(), // Patched version
            source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
            checksum: Some("abc123".to_string()),
            dependencies: vec![],
        }];

        let mut db = AdvisoryDatabase::new();
        db.add_advisory(sample_advisory());

        let result = run_audit(&deps, &db);
        assert!(!result.has_vulnerabilities());
    }

    #[test]
    fn test_run_audit_missing_checksum_warning() {
        let deps = vec![ParsedDependency {
            name: "suspicious-crate".to_string(),
            version: "1.0.0".to_string(),
            source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
            checksum: None, // Missing checksum
            dependencies: vec![],
        }];

        let db = AdvisoryDatabase::new();
        let result = run_audit(&deps, &db);

        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].kind, "missing-checksum");
        assert_eq!(result.summary.warnings, 1);
    }

    #[test]
    fn test_run_audit_multiple_severities() {
        let deps = vec![
            ParsedDependency {
                name: "crate-a".to_string(),
                version: "1.0.0".to_string(),
                source: None,
                checksum: None,
                dependencies: vec![],
            },
            ParsedDependency {
                name: "crate-b".to_string(),
                version: "1.0.0".to_string(),
                source: None,
                checksum: None,
                dependencies: vec![],
            },
        ];

        let mut db = AdvisoryDatabase::new();
        db.add_advisory(Advisory {
            id: "ADV-001".to_string(),
            package: "crate-a".to_string(),
            title: "Critical issue".to_string(),
            description: None,
            severity: Severity::Critical,
            cve_ids: vec![],
            url: None,
            affected_versions: vec![],
            patched_version: None,
            date: None,
        });
        db.add_advisory(Advisory {
            id: "ADV-002".to_string(),
            package: "crate-b".to_string(),
            title: "Low issue".to_string(),
            description: None,
            severity: Severity::Low,
            cve_ids: vec![],
            url: None,
            affected_versions: vec![],
            patched_version: None,
            date: None,
        });

        let result = run_audit(&deps, &db);
        assert_eq!(result.summary.critical, 1);
        assert_eq!(result.summary.low, 1);
        assert!(result.has_critical_or_high());

        // Vulnerabilities should be sorted by severity (critical first)
        assert_eq!(result.vulnerabilities[0].severity, Severity::Critical);
        assert_eq!(result.vulnerabilities[1].severity, Severity::Low);
    }

    #[test]
    fn test_audit_result_vulnerabilities_at_or_above() {
        let result = AuditResult {
            dependencies_scanned: 3,
            vulnerabilities: vec![
                VulnerabilityFinding {
                    advisory_id: "A".to_string(),
                    package: "a".to_string(),
                    installed_version: "1.0.0".to_string(),
                    title: "Critical".to_string(),
                    severity: Severity::Critical,
                    cve_ids: vec![],
                    url: None,
                    patched_version: None,
                },
                VulnerabilityFinding {
                    advisory_id: "B".to_string(),
                    package: "b".to_string(),
                    installed_version: "1.0.0".to_string(),
                    title: "Medium".to_string(),
                    severity: Severity::Medium,
                    cve_ids: vec![],
                    url: None,
                    patched_version: None,
                },
                VulnerabilityFinding {
                    advisory_id: "C".to_string(),
                    package: "c".to_string(),
                    installed_version: "1.0.0".to_string(),
                    title: "Low".to_string(),
                    severity: Severity::Low,
                    cve_ids: vec![],
                    url: None,
                    patched_version: None,
                },
            ],
            warnings: vec![],
            summary: AuditSummary {
                critical: 1,
                high: 0,
                medium: 1,
                low: 1,
                none: 0,
                warnings: 0,
            },
        };

        assert_eq!(result.vulnerabilities_at_or_above(Severity::High).len(), 1);
        assert_eq!(
            result.vulnerabilities_at_or_above(Severity::Medium).len(),
            2
        );
        assert_eq!(result.vulnerabilities_at_or_above(Severity::Low).len(), 3);
    }

    // -------------------------------------------------------------------------
    // Report formatting tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_format_audit_report_no_vulns() {
        let result = AuditResult {
            dependencies_scanned: 5,
            vulnerabilities: vec![],
            warnings: vec![],
            summary: AuditSummary::default(),
        };

        let report = format_audit_report(&result);
        assert!(report.contains("Dependencies scanned: 5"));
        assert!(report.contains("No vulnerabilities found"));
    }

    #[test]
    fn test_format_audit_report_with_vulns() {
        let result = AuditResult {
            dependencies_scanned: 10,
            vulnerabilities: vec![VulnerabilityFinding {
                advisory_id: "RUSTSEC-2023-0001".to_string(),
                package: "bad-crate".to_string(),
                installed_version: "1.0.0".to_string(),
                title: "Memory safety issue".to_string(),
                severity: Severity::High,
                cve_ids: vec!["CVE-2023-12345".to_string()],
                url: Some("https://example.com/advisory".to_string()),
                patched_version: Some("1.5.0".to_string()),
            }],
            warnings: vec![],
            summary: AuditSummary {
                high: 1,
                ..Default::default()
            },
        };

        let report = format_audit_report(&result);
        assert!(report.contains("Found 1 vulnerability"));
        assert!(report.contains("[HIGH]"));
        assert!(report.contains("bad-crate@1.0.0"));
        assert!(report.contains("RUSTSEC-2023-0001"));
        assert!(report.contains("CVE-2023-12345"));
        assert!(report.contains("Upgrade to >= 1.5.0"));
    }

    #[test]
    fn test_format_audit_report_json() {
        let result = AuditResult {
            dependencies_scanned: 2,
            vulnerabilities: vec![],
            warnings: vec![],
            summary: AuditSummary::default(),
        };

        let json = format_audit_report_json(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["dependencies_scanned"], 2);
    }

    #[test]
    fn test_format_audit_report_with_warnings() {
        let result = AuditResult {
            dependencies_scanned: 3,
            vulnerabilities: vec![],
            warnings: vec![AuditWarning {
                kind: "missing-checksum".to_string(),
                package: "suspect".to_string(),
                version: "1.0.0".to_string(),
                message: "No checksum found".to_string(),
            }],
            summary: AuditSummary {
                warnings: 1,
                ..Default::default()
            },
        };

        let report = format_audit_report(&result);
        assert!(report.contains("Warnings (1)"));
        assert!(report.contains("missing-checksum"));
        assert!(report.contains("suspect@1.0.0"));
    }
}
