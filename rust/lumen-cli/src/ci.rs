//! CI configuration and runner support for Lumen projects.
//!
//! Provides unified configuration for Miri, coverage gating, sanitizers,
//! and Valgrind integration. Generates CI pipeline configurations (GitHub
//! Actions, GitLab CI) and parses coverage reports.

use std::fmt;

// ===========================================================================
// Configuration types
// ===========================================================================

/// Unified CI configuration covering all analysis tools.
#[derive(Debug, Clone)]
pub struct CiConfig {
    pub miri: MiriConfig,
    pub coverage: CoverageConfig,
    pub sanitizers: SanitizerConfig,
    pub test_config: TestConfig,
}

/// Configuration for Miri (Rust undefined-behavior checker).
#[derive(Debug, Clone)]
pub struct MiriConfig {
    pub enabled: bool,
    pub flags: Vec<String>,
    pub excluded_tests: Vec<String>,
    pub stacked_borrows: bool,
    pub isolation: bool,
    pub timeout_secs: u64,
}

/// Configuration for code coverage collection and gating.
#[derive(Debug, Clone)]
pub struct CoverageConfig {
    pub enabled: bool,
    /// Minimum line coverage percentage required to pass (e.g. 80.0).
    pub threshold_percent: f64,
    pub tool: CoverageTool,
    pub exclude_patterns: Vec<String>,
    pub fail_on_decrease: bool,
    pub report_format: CoverageFormat,
}

/// Supported coverage collection tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageTool {
    Tarpaulin,
    LlvmCov,
    Grcov,
}

impl fmt::Display for CoverageTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoverageTool::Tarpaulin => write!(f, "tarpaulin"),
            CoverageTool::LlvmCov => write!(f, "llvm-cov"),
            CoverageTool::Grcov => write!(f, "grcov"),
        }
    }
}

/// Output format for coverage reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageFormat {
    Html,
    Lcov,
    Json,
    Summary,
}

impl fmt::Display for CoverageFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoverageFormat::Html => write!(f, "html"),
            CoverageFormat::Lcov => write!(f, "lcov"),
            CoverageFormat::Json => write!(f, "json"),
            CoverageFormat::Summary => write!(f, "summary"),
        }
    }
}

/// Configuration for sanitizer and Valgrind runs.
#[derive(Debug, Clone)]
pub struct SanitizerConfig {
    /// Address sanitizer (detects out-of-bounds, use-after-free).
    pub asan: bool,
    /// Memory sanitizer (detects uninitialised reads).
    pub msan: bool,
    /// Thread sanitizer (detects data races).
    pub tsan: bool,
    /// Undefined-behavior sanitizer.
    pub ubsan: bool,
    /// Run tests under Valgrind memcheck.
    pub valgrind: bool,
    pub valgrind_flags: Vec<String>,
    /// Valgrind suppression file paths.
    pub suppressions: Vec<String>,
}

/// General test-runner configuration.
#[derive(Debug, Clone)]
pub struct TestConfig {
    pub parallel: bool,
    pub timeout_secs: u64,
    pub retry_failed: u32,
    pub fail_fast: bool,
}

// ===========================================================================
// Default implementations
// ===========================================================================

impl CiConfig {
    /// Balanced defaults: Miri and coverage enabled, sanitizers off.
    pub fn default_config() -> Self {
        CiConfig {
            miri: MiriConfig::default_config(),
            coverage: CoverageConfig::default_config(),
            sanitizers: SanitizerConfig::default_config(),
            test_config: TestConfig::default_config(),
        }
    }

    /// Everything enabled with strict thresholds.
    pub fn strict_config() -> Self {
        CiConfig {
            miri: MiriConfig::default_config(),
            coverage: CoverageConfig {
                enabled: true,
                threshold_percent: 90.0,
                tool: CoverageTool::LlvmCov,
                exclude_patterns: vec![],
                fail_on_decrease: true,
                report_format: CoverageFormat::Lcov,
            },
            sanitizers: SanitizerConfig {
                asan: true,
                msan: true,
                tsan: true,
                ubsan: true,
                valgrind: true,
                valgrind_flags: vec![
                    "--leak-check=full".to_string(),
                    "--error-exitcode=1".to_string(),
                    "--track-origins=yes".to_string(),
                ],
                suppressions: vec![],
            },
            test_config: TestConfig {
                parallel: true,
                timeout_secs: 600,
                retry_failed: 0,
                fail_fast: true,
            },
        }
    }

    /// Only tests and basic coverage, nothing else.
    pub fn minimal_config() -> Self {
        CiConfig {
            miri: MiriConfig {
                enabled: false,
                flags: vec![],
                excluded_tests: vec![],
                stacked_borrows: false,
                isolation: false,
                timeout_secs: 0,
            },
            coverage: CoverageConfig {
                enabled: true,
                threshold_percent: 50.0,
                tool: CoverageTool::Tarpaulin,
                exclude_patterns: vec![],
                fail_on_decrease: false,
                report_format: CoverageFormat::Summary,
            },
            sanitizers: SanitizerConfig {
                asan: false,
                msan: false,
                tsan: false,
                ubsan: false,
                valgrind: false,
                valgrind_flags: vec![],
                suppressions: vec![],
            },
            test_config: TestConfig {
                parallel: true,
                timeout_secs: 300,
                retry_failed: 1,
                fail_fast: false,
            },
        }
    }
}

impl MiriConfig {
    /// Sensible defaults: stacked borrows and isolation enabled.
    pub fn default_config() -> Self {
        MiriConfig {
            enabled: true,
            flags: vec![
                "-Zmiri-symbolic-alignment-check".to_string(),
                "-Zmiri-retag-fields".to_string(),
            ],
            excluded_tests: vec![],
            stacked_borrows: true,
            isolation: true,
            timeout_secs: 300,
        }
    }

    /// Create a config with custom flags (keeps other defaults).
    pub fn with_flags(flags: &[&str]) -> Self {
        MiriConfig {
            flags: flags.iter().map(|s| s.to_string()).collect(),
            ..Self::default_config()
        }
    }
}

impl CoverageConfig {
    pub fn default_config() -> Self {
        CoverageConfig {
            enabled: true,
            threshold_percent: 80.0,
            tool: CoverageTool::Tarpaulin,
            exclude_patterns: vec!["tests/*".to_string(), "benches/*".to_string()],
            fail_on_decrease: true,
            report_format: CoverageFormat::Html,
        }
    }
}

impl SanitizerConfig {
    pub fn default_config() -> Self {
        SanitizerConfig {
            asan: false,
            msan: false,
            tsan: false,
            ubsan: false,
            valgrind: false,
            valgrind_flags: vec![
                "--leak-check=full".to_string(),
                "--error-exitcode=1".to_string(),
            ],
            suppressions: vec![],
        }
    }
}

impl TestConfig {
    pub fn default_config() -> Self {
        TestConfig {
            parallel: true,
            timeout_secs: 300,
            retry_failed: 2,
            fail_fast: false,
        }
    }
}

// ===========================================================================
// Coverage result types
// ===========================================================================

/// Parsed coverage measurement.
#[derive(Debug, Clone)]
pub struct CoverageResult {
    pub line_coverage: f64,
    pub branch_coverage: Option<f64>,
    pub function_coverage: Option<f64>,
    pub uncovered_lines: Vec<UncoveredRegion>,
}

/// A contiguous uncovered region in a source file.
#[derive(Debug, Clone)]
pub struct UncoveredRegion {
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
}

/// Outcome of comparing coverage against a gate threshold.
#[derive(Debug, Clone)]
pub enum CoverageGateResult {
    Pass {
        coverage: f64,
        threshold: f64,
    },
    Fail {
        coverage: f64,
        threshold: f64,
        message: String,
    },
    Decreased {
        previous: f64,
        current: f64,
    },
}

impl CoverageGateResult {
    pub fn is_pass(&self) -> bool {
        matches!(self, CoverageGateResult::Pass { .. })
    }
}

// ===========================================================================
// CI report types
// ===========================================================================

/// Aggregated CI report with multiple sections.
#[derive(Debug, Clone)]
pub struct CiReport {
    pub sections: Vec<CiReportSection>,
}

/// One section of a CI report (e.g. "miri", "coverage", "asan").
#[derive(Debug, Clone)]
pub struct CiReportSection {
    pub name: String,
    pub status: CiStatus,
    pub details: String,
    pub duration_ms: u64,
}

/// Status of a single CI step.
#[derive(Debug, Clone)]
pub enum CiStatus {
    Pass,
    Fail(String),
    Skip(String),
    Warning(String),
}

impl fmt::Display for CiStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CiStatus::Pass => write!(f, "pass"),
            CiStatus::Fail(msg) => write!(f, "fail: {}", msg),
            CiStatus::Skip(msg) => write!(f, "skip: {}", msg),
            CiStatus::Warning(msg) => write!(f, "warning: {}", msg),
        }
    }
}

impl Default for CiReport {
    fn default() -> Self {
        Self::new()
    }
}

impl CiReport {
    pub fn new() -> Self {
        CiReport {
            sections: Vec::new(),
        }
    }

    pub fn add_section(&mut self, section: CiReportSection) {
        self.sections.push(section);
    }

    /// Overall status: Fail if any section failed, Warning if any warns,
    /// Skip if all skipped, Pass otherwise.
    pub fn overall_status(&self) -> CiStatus {
        let mut has_warning = false;
        let mut all_skip = !self.sections.is_empty();

        for s in &self.sections {
            match &s.status {
                CiStatus::Fail(msg) => return CiStatus::Fail(msg.clone()),
                CiStatus::Warning(_) => {
                    has_warning = true;
                    all_skip = false;
                }
                CiStatus::Skip(_) => {}
                CiStatus::Pass => {
                    all_skip = false;
                }
            }
        }

        if self.sections.is_empty() {
            return CiStatus::Skip("no sections".to_string());
        }
        if all_skip {
            return CiStatus::Skip("all sections skipped".to_string());
        }
        if has_warning {
            return CiStatus::Warning("some sections have warnings".to_string());
        }
        CiStatus::Pass
    }

    /// One-line summary of all sections.
    pub fn summary(&self) -> String {
        let pass = self
            .sections
            .iter()
            .filter(|s| matches!(s.status, CiStatus::Pass))
            .count();
        let fail = self
            .sections
            .iter()
            .filter(|s| matches!(s.status, CiStatus::Fail(_)))
            .count();
        let skip = self
            .sections
            .iter()
            .filter(|s| matches!(s.status, CiStatus::Skip(_)))
            .count();
        let warn = self
            .sections
            .iter()
            .filter(|s| matches!(s.status, CiStatus::Warning(_)))
            .count();
        let total_ms: u64 = self.sections.iter().map(|s| s.duration_ms).sum();

        format!(
            "{} passed, {} failed, {} skipped, {} warnings ({} ms)",
            pass, fail, skip, warn, total_ms
        )
    }

    /// Render the report as a Markdown document.
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str("# CI Report\n\n");
        md.push_str(&format!("**Status**: {}\n\n", self.overall_status()));
        md.push_str(&format!("**Summary**: {}\n\n", self.summary()));

        md.push_str("| Section | Status | Duration |\n");
        md.push_str("|---------|--------|----------|\n");

        for s in &self.sections {
            let status_str = match &s.status {
                CiStatus::Pass => "PASS".to_string(),
                CiStatus::Fail(msg) => format!("FAIL: {}", msg),
                CiStatus::Skip(msg) => format!("SKIP: {}", msg),
                CiStatus::Warning(msg) => format!("WARN: {}", msg),
            };
            md.push_str(&format!(
                "| {} | {} | {} ms |\n",
                s.name, status_str, s.duration_ms
            ));
        }

        if self.sections.iter().any(|s| !s.details.is_empty()) {
            md.push_str("\n## Details\n\n");
            for s in &self.sections {
                if !s.details.is_empty() {
                    md.push_str(&format!("### {}\n\n", s.name));
                    md.push_str(&format!("{}\n\n", s.details));
                }
            }
        }

        md
    }

    /// Render the report as a JSON string.
    pub fn to_json(&self) -> String {
        // Manual JSON construction to avoid adding serde derives.
        let mut parts: Vec<String> = Vec::new();
        for s in &self.sections {
            let status_json = match &s.status {
                CiStatus::Pass => r#""pass""#.to_string(),
                CiStatus::Fail(msg) => format!(r#"{{"fail": "{}"}}"#, json_escape(msg)),
                CiStatus::Skip(msg) => format!(r#"{{"skip": "{}"}}"#, json_escape(msg)),
                CiStatus::Warning(msg) => format!(r#"{{"warning": "{}"}}"#, json_escape(msg)),
            };
            parts.push(format!(
                r#"    {{"name": "{}", "status": {}, "details": "{}", "duration_ms": {}}}"#,
                json_escape(&s.name),
                status_json,
                json_escape(&s.details),
                s.duration_ms,
            ));
        }
        let overall = match self.overall_status() {
            CiStatus::Pass => "pass".to_string(),
            CiStatus::Fail(m) => format!("fail: {}", m),
            CiStatus::Skip(m) => format!("skip: {}", m),
            CiStatus::Warning(m) => format!("warning: {}", m),
        };
        format!(
            "{{\n  \"overall\": \"{}\",\n  \"summary\": \"{}\",\n  \"sections\": [\n{}\n  ]\n}}",
            json_escape(&overall),
            json_escape(&self.summary()),
            parts.join(",\n"),
        )
    }
}

/// Escape characters for JSON string values.
fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

// ===========================================================================
// CiRunner — command generation
// ===========================================================================

/// Generates shell commands and CI pipeline configs for each tool.
pub struct CiRunner;

impl CiRunner {
    // -----------------------------------------------------------------------
    // Miri
    // -----------------------------------------------------------------------

    /// Generate the command(s) to run Miri on the workspace.
    pub fn miri_command(config: &MiriConfig) -> Vec<String> {
        if !config.enabled {
            return vec![];
        }

        let mut env_flags: Vec<String> = Vec::new();
        if config.stacked_borrows {
            env_flags.push("-Zmiri-stacked-borrows".to_string());
        }
        if config.isolation {
            env_flags.push("-Zmiri-isolation-error=warn-nobacktrace".to_string());
        }
        for f in &config.flags {
            if !env_flags.contains(f) {
                env_flags.push(f.clone());
            }
        }

        let mut cmds = Vec::new();

        // Set MIRIFLAGS environment variable
        if !env_flags.is_empty() {
            cmds.push(format!("export MIRIFLAGS=\"{}\"", env_flags.join(" ")));
        }

        // Main miri command
        let mut cmd = "cargo +nightly miri test --workspace".to_string();

        // Exclusions
        for ex in &config.excluded_tests {
            cmd.push_str(&format!(" --exclude {}", ex));
        }

        // Timeout wrapper
        if config.timeout_secs > 0 {
            cmds.push(format!("timeout {}s {}", config.timeout_secs, cmd));
        } else {
            cmds.push(cmd);
        }

        cmds
    }

    // -----------------------------------------------------------------------
    // Coverage
    // -----------------------------------------------------------------------

    /// Generate the command(s) to collect coverage.
    pub fn coverage_command(config: &CoverageConfig) -> Vec<String> {
        if !config.enabled {
            return vec![];
        }

        match config.tool {
            CoverageTool::Tarpaulin => {
                let mut cmd = "cargo tarpaulin --workspace".to_string();

                match config.report_format {
                    CoverageFormat::Html => cmd.push_str(" --out Html"),
                    CoverageFormat::Lcov => cmd.push_str(" --out Lcov"),
                    CoverageFormat::Json => cmd.push_str(" --out Json"),
                    CoverageFormat::Summary => {} // default output
                }

                for pat in &config.exclude_patterns {
                    cmd.push_str(&format!(" --exclude-files '{}'", pat));
                }

                vec![cmd]
            }
            CoverageTool::LlvmCov => {
                let mut cmds = vec![
                    "rustup component add llvm-tools-preview".to_string(),
                    "cargo install cargo-llvm-cov".to_string(),
                ];

                let mut cmd = "cargo llvm-cov --workspace".to_string();

                match config.report_format {
                    CoverageFormat::Html => cmd.push_str(" --html"),
                    CoverageFormat::Lcov => cmd.push_str(" --lcov --output-path lcov.info"),
                    CoverageFormat::Json => cmd.push_str(" --json --output-path coverage.json"),
                    CoverageFormat::Summary => {} // default summary
                }

                for pat in &config.exclude_patterns {
                    cmd.push_str(&format!(" --ignore-filename-regex '{}'", pat));
                }

                cmds.push(cmd);
                cmds
            }
            CoverageTool::Grcov => {
                let cmds = vec![
                    "export CARGO_INCREMENTAL=0".to_string(),
                    "export RUSTFLAGS=\"-Cinstrument-coverage\"".to_string(),
                    "export LLVM_PROFILE_FILE=\"lumen-%p-%m.profraw\"".to_string(),
                    "cargo test --workspace".to_string(),
                    format!(
                        "grcov . -s . --binary-path ./target/debug/ -t {} --branch --ignore-not-existing -o coverage/",
                        config.report_format
                    ),
                ];
                cmds
            }
        }
    }

    // -----------------------------------------------------------------------
    // Sanitizers
    // -----------------------------------------------------------------------

    /// Generate commands for each enabled sanitizer.
    pub fn sanitizer_commands(config: &SanitizerConfig) -> Vec<String> {
        let mut cmds = Vec::new();

        let sanitizers: Vec<(&str, bool)> = vec![
            ("address", config.asan),
            ("memory", config.msan),
            ("thread", config.tsan),
        ];

        for (name, enabled) in sanitizers {
            if enabled {
                cmds.push(format!(
                    "RUSTFLAGS=\"-Zsanitizer={}\" cargo +nightly test --workspace --target x86_64-unknown-linux-gnu",
                    name
                ));
            }
        }

        if config.ubsan {
            // UBSan is not directly a Rust sanitizer flag; use a nightly feature.
            cmds.push(
                "RUSTFLAGS=\"-Zsanitizer=undefined\" cargo +nightly test --workspace --target x86_64-unknown-linux-gnu".to_string(),
            );
        }

        cmds
    }

    /// Generate the Valgrind command for a specific test binary.
    pub fn valgrind_command(config: &SanitizerConfig, test_binary: &str) -> Vec<String> {
        if !config.valgrind {
            return vec![];
        }

        let mut cmd = "valgrind".to_string();

        for flag in &config.valgrind_flags {
            cmd.push_str(&format!(" {}", flag));
        }

        for supp in &config.suppressions {
            cmd.push_str(&format!(" --suppressions={}", supp));
        }

        cmd.push_str(&format!(" {}", test_binary));

        vec![cmd]
    }

    // -----------------------------------------------------------------------
    // Coverage parsing
    // -----------------------------------------------------------------------

    /// Parse a coverage summary from tool output (tarpaulin or llvm-cov).
    ///
    /// Recognises patterns such as:
    /// - tarpaulin: `85.32% coverage, 1200/1407 lines covered`
    /// - llvm-cov:  `TOTAL ... 85.3%`
    /// - grcov:     `lines......: 85.3% (1200 of 1407 lines)`
    pub fn parse_coverage_summary(output: &str) -> Option<CoverageResult> {
        // Try tarpaulin format: "XX.XX% coverage, N/M lines covered"
        if let Some(result) = Self::parse_tarpaulin_output(output) {
            return Some(result);
        }

        // Try llvm-cov format: "TOTAL ... XX.X%"
        if let Some(result) = Self::parse_llvm_cov_output(output) {
            return Some(result);
        }

        // Try grcov/lcov format: "lines......: XX.X% (N of M lines)"
        if let Some(result) = Self::parse_grcov_output(output) {
            return Some(result);
        }

        None
    }

    fn parse_tarpaulin_output(output: &str) -> Option<CoverageResult> {
        // Pattern: "XX.XX% coverage"
        for line in output.lines() {
            let trimmed = line.trim();
            if let Some(idx) = trimmed.find("% coverage") {
                let before = &trimmed[..idx];
                // Find the start of the number (scan backwards for whitespace/start)
                let num_str = before
                    .rsplit(|c: char| !c.is_ascii_digit() && c != '.')
                    .next()?;
                if let Ok(pct) = num_str.parse::<f64>() {
                    return Some(CoverageResult {
                        line_coverage: pct,
                        branch_coverage: None,
                        function_coverage: None,
                        uncovered_lines: vec![],
                    });
                }
            }
        }
        None
    }

    fn parse_llvm_cov_output(output: &str) -> Option<CoverageResult> {
        // Pattern: "TOTAL ... XX.X%"
        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("TOTAL") || trimmed.contains("TOTAL") {
                // Find percentage values in this line
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                // llvm-cov summary has multiple percentages; the last one is
                // typically line coverage.  We look for the first percentage.
                for part in parts.iter().rev() {
                    if let Some(stripped) = part.strip_suffix('%') {
                        if let Ok(pct) = stripped.parse::<f64>() {
                            return Some(CoverageResult {
                                line_coverage: pct,
                                branch_coverage: None,
                                function_coverage: None,
                                uncovered_lines: vec![],
                            });
                        }
                    }
                }
            }
        }
        None
    }

    fn parse_grcov_output(output: &str) -> Option<CoverageResult> {
        // Pattern: "lines......: XX.X% (N of M lines)"
        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("lines") {
                if let Some(colon_idx) = trimmed.find(':') {
                    let after = trimmed[colon_idx + 1..].trim();
                    if let Some(pct_idx) = after.find('%') {
                        let num_str = after[..pct_idx].trim();
                        if let Ok(pct) = num_str.parse::<f64>() {
                            return Some(CoverageResult {
                                line_coverage: pct,
                                branch_coverage: None,
                                function_coverage: None,
                                uncovered_lines: vec![],
                            });
                        }
                    }
                }
            }
        }
        None
    }

    // -----------------------------------------------------------------------
    // Coverage gate
    // -----------------------------------------------------------------------

    /// Check whether coverage meets the configured threshold.
    pub fn check_coverage_gate(
        result: &CoverageResult,
        config: &CoverageConfig,
    ) -> CoverageGateResult {
        if result.line_coverage >= config.threshold_percent {
            CoverageGateResult::Pass {
                coverage: result.line_coverage,
                threshold: config.threshold_percent,
            }
        } else {
            CoverageGateResult::Fail {
                coverage: result.line_coverage,
                threshold: config.threshold_percent,
                message: format!(
                    "Coverage {:.1}% is below threshold {:.1}%",
                    result.line_coverage, config.threshold_percent
                ),
            }
        }
    }

    /// Check whether coverage decreased compared to a previous result.
    pub fn check_coverage_decrease(
        previous: f64,
        current: &CoverageResult,
        config: &CoverageConfig,
    ) -> CoverageGateResult {
        if config.fail_on_decrease && current.line_coverage < previous {
            CoverageGateResult::Decreased {
                previous,
                current: current.line_coverage,
            }
        } else if current.line_coverage >= config.threshold_percent {
            CoverageGateResult::Pass {
                coverage: current.line_coverage,
                threshold: config.threshold_percent,
            }
        } else {
            CoverageGateResult::Fail {
                coverage: current.line_coverage,
                threshold: config.threshold_percent,
                message: format!(
                    "Coverage {:.1}% is below threshold {:.1}%",
                    current.line_coverage, config.threshold_percent
                ),
            }
        }
    }

    // -----------------------------------------------------------------------
    // GitHub Actions YAML
    // -----------------------------------------------------------------------

    /// Generate a GitHub Actions workflow YAML for the given config.
    pub fn github_actions_yaml(config: &CiConfig) -> String {
        let mut yaml = String::new();

        yaml.push_str("name: CI\n\n");
        yaml.push_str(
            "on:\n  push:\n    branches: [main]\n  pull_request:\n    branches: [main]\n\n",
        );
        yaml.push_str("env:\n  CARGO_TERM_COLOR: always\n\n");
        yaml.push_str("jobs:\n");

        // -- Test job --
        yaml.push_str("  test:\n");
        yaml.push_str("    runs-on: ubuntu-latest\n");
        yaml.push_str("    steps:\n");
        yaml.push_str("      - uses: actions/checkout@v4\n");
        yaml.push_str("      - uses: dtolnay/rust-toolchain@stable\n");

        let mut test_cmd = "cargo test --workspace".to_string();
        if config.test_config.fail_fast {
            test_cmd.push_str(" -- --fail-fast");
        }
        yaml.push_str(&format!("      - run: {}\n", test_cmd));
        if config.test_config.timeout_secs > 0 {
            yaml.push_str(&format!(
                "        timeout-minutes: {}\n",
                config.test_config.timeout_secs.div_ceil(60)
            ));
        }

        // -- Miri job --
        if config.miri.enabled {
            yaml.push('\n');
            yaml.push_str("  miri:\n");
            yaml.push_str("    runs-on: ubuntu-latest\n");
            yaml.push_str("    steps:\n");
            yaml.push_str("      - uses: actions/checkout@v4\n");
            yaml.push_str("      - uses: dtolnay/rust-toolchain@nightly\n");
            yaml.push_str("        with:\n");
            yaml.push_str("          components: miri\n");

            let miri_cmds = Self::miri_command(&config.miri);
            for cmd in &miri_cmds {
                yaml.push_str(&format!("      - run: {}\n", cmd));
            }
            if config.miri.timeout_secs > 0 {
                yaml.push_str(&format!(
                    "        timeout-minutes: {}\n",
                    config.miri.timeout_secs.div_ceil(60)
                ));
            }
        }

        // -- Coverage job --
        if config.coverage.enabled {
            yaml.push('\n');
            yaml.push_str("  coverage:\n");
            yaml.push_str("    runs-on: ubuntu-latest\n");
            yaml.push_str("    steps:\n");
            yaml.push_str("      - uses: actions/checkout@v4\n");
            yaml.push_str("      - uses: dtolnay/rust-toolchain@stable\n");

            let cov_cmds = Self::coverage_command(&config.coverage);
            for cmd in &cov_cmds {
                yaml.push_str(&format!("      - run: {}\n", cmd));
            }
        }

        // -- Sanitizer jobs --
        let san_cmds = Self::sanitizer_commands(&config.sanitizers);
        if !san_cmds.is_empty() {
            yaml.push('\n');
            yaml.push_str("  sanitizers:\n");
            yaml.push_str("    runs-on: ubuntu-latest\n");
            yaml.push_str("    steps:\n");
            yaml.push_str("      - uses: actions/checkout@v4\n");
            yaml.push_str("      - uses: dtolnay/rust-toolchain@nightly\n");

            for cmd in &san_cmds {
                yaml.push_str(&format!("      - run: {}\n", cmd));
            }
        }

        // -- Valgrind job --
        if config.sanitizers.valgrind {
            yaml.push('\n');
            yaml.push_str("  valgrind:\n");
            yaml.push_str("    runs-on: ubuntu-latest\n");
            yaml.push_str("    steps:\n");
            yaml.push_str("      - uses: actions/checkout@v4\n");
            yaml.push_str("      - uses: dtolnay/rust-toolchain@stable\n");
            yaml.push_str("      - run: sudo apt-get install -y valgrind\n");
            yaml.push_str("      - run: cargo test --workspace --no-run\n");

            let vg_cmds =
                Self::valgrind_command(&config.sanitizers, "./target/debug/deps/lumen_tests");
            for cmd in &vg_cmds {
                yaml.push_str(&format!("      - run: {}\n", cmd));
            }
        }

        yaml
    }

    // -----------------------------------------------------------------------
    // GitLab CI YAML
    // -----------------------------------------------------------------------

    /// Generate a GitLab CI pipeline YAML for the given config.
    pub fn gitlab_ci_yaml(config: &CiConfig) -> String {
        let mut yaml = String::new();

        yaml.push_str("stages:\n");
        yaml.push_str("  - test\n");
        if config.miri.enabled {
            yaml.push_str("  - miri\n");
        }
        if config.coverage.enabled {
            yaml.push_str("  - coverage\n");
        }
        let san_cmds = Self::sanitizer_commands(&config.sanitizers);
        if !san_cmds.is_empty() || config.sanitizers.valgrind {
            yaml.push_str("  - sanitizers\n");
        }
        yaml.push('\n');

        // -- Test stage --
        yaml.push_str("test:\n");
        yaml.push_str("  stage: test\n");
        yaml.push_str("  image: rust:latest\n");
        yaml.push_str("  script:\n");

        let mut test_cmd = "cargo test --workspace".to_string();
        if config.test_config.fail_fast {
            test_cmd.push_str(" -- --fail-fast");
        }
        yaml.push_str(&format!("    - {}\n", test_cmd));
        if config.test_config.timeout_secs > 0 {
            yaml.push_str(&format!(
                "  timeout: {} minutes\n",
                config.test_config.timeout_secs.div_ceil(60)
            ));
        }

        // -- Miri stage --
        if config.miri.enabled {
            yaml.push('\n');
            yaml.push_str("miri:\n");
            yaml.push_str("  stage: miri\n");
            yaml.push_str("  image: rust:latest\n");
            yaml.push_str("  before_script:\n");
            yaml.push_str("    - rustup toolchain install nightly --component miri\n");
            yaml.push_str("  script:\n");

            let miri_cmds = Self::miri_command(&config.miri);
            for cmd in &miri_cmds {
                yaml.push_str(&format!("    - {}\n", cmd));
            }
        }

        // -- Coverage stage --
        if config.coverage.enabled {
            yaml.push('\n');
            yaml.push_str("coverage:\n");
            yaml.push_str("  stage: coverage\n");
            yaml.push_str("  image: rust:latest\n");
            yaml.push_str("  script:\n");

            let cov_cmds = Self::coverage_command(&config.coverage);
            for cmd in &cov_cmds {
                yaml.push_str(&format!("    - {}\n", cmd));
            }
        }

        // -- Sanitizers stage --
        if !san_cmds.is_empty() {
            yaml.push('\n');
            yaml.push_str("sanitizers:\n");
            yaml.push_str("  stage: sanitizers\n");
            yaml.push_str("  image: rust:latest\n");
            yaml.push_str("  before_script:\n");
            yaml.push_str("    - rustup toolchain install nightly\n");
            yaml.push_str("  script:\n");
            for cmd in &san_cmds {
                yaml.push_str(&format!("    - {}\n", cmd));
            }
        }

        // -- Valgrind --
        if config.sanitizers.valgrind {
            yaml.push('\n');
            yaml.push_str("valgrind:\n");
            yaml.push_str("  stage: sanitizers\n");
            yaml.push_str("  image: rust:latest\n");
            yaml.push_str("  before_script:\n");
            yaml.push_str("    - apt-get update && apt-get install -y valgrind\n");
            yaml.push_str("  script:\n");
            yaml.push_str("    - cargo test --workspace --no-run\n");

            let vg_cmds =
                Self::valgrind_command(&config.sanitizers, "./target/debug/deps/lumen_tests");
            for cmd in &vg_cmds {
                yaml.push_str(&format!("    - {}\n", cmd));
            }
        }

        yaml
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_miri_enabled() {
        let cfg = CiConfig::default_config();
        assert!(cfg.miri.enabled);
    }

    #[test]
    fn default_config_has_coverage_enabled() {
        let cfg = CiConfig::default_config();
        assert!(cfg.coverage.enabled);
    }

    #[test]
    fn strict_config_all_sanitizers() {
        let cfg = CiConfig::strict_config();
        assert!(cfg.sanitizers.asan);
        assert!(cfg.sanitizers.msan);
        assert!(cfg.sanitizers.tsan);
        assert!(cfg.sanitizers.ubsan);
        assert!(cfg.sanitizers.valgrind);
    }

    #[test]
    fn minimal_config_miri_disabled() {
        let cfg = CiConfig::minimal_config();
        assert!(!cfg.miri.enabled);
    }

    #[test]
    fn coverage_gate_pass() {
        let result = CoverageResult {
            line_coverage: 85.0,
            branch_coverage: None,
            function_coverage: None,
            uncovered_lines: vec![],
        };
        let cfg = CoverageConfig::default_config();
        let gate = CiRunner::check_coverage_gate(&result, &cfg);
        assert!(gate.is_pass());
    }

    #[test]
    fn coverage_gate_fail() {
        let result = CoverageResult {
            line_coverage: 50.0,
            branch_coverage: None,
            function_coverage: None,
            uncovered_lines: vec![],
        };
        let cfg = CoverageConfig::default_config();
        let gate = CiRunner::check_coverage_gate(&result, &cfg);
        assert!(!gate.is_pass());
    }

    #[test]
    fn json_escape_special_chars() {
        assert_eq!(json_escape("hello\"world"), "hello\\\"world");
        assert_eq!(json_escape("line\nnew"), "line\\nnew");
        assert_eq!(json_escape("tab\there"), "tab\\there");
    }
}
