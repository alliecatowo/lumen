//! Integration tests for CI configuration and runner (T109, T143, T144).

use lumen_cli::ci::{
    CiConfig, CiReport, CiReportSection, CiRunner, CiStatus, CoverageConfig, CoverageFormat,
    CoverageGateResult, CoverageResult, CoverageTool, MiriConfig, SanitizerConfig, TestConfig,
    UncoveredRegion,
};

// =============================================================================
// Helper: quick coverage result
// =============================================================================

fn cov_result(line: f64) -> CoverageResult {
    CoverageResult {
        line_coverage: line,
        branch_coverage: None,
        function_coverage: None,
        uncovered_lines: vec![],
    }
}

fn cov_result_full(line: f64, branch: f64, func: f64) -> CoverageResult {
    CoverageResult {
        line_coverage: line,
        branch_coverage: Some(branch),
        function_coverage: Some(func),
        uncovered_lines: vec![],
    }
}

// =============================================================================
// CiConfig presets
// =============================================================================

#[test]
fn wave25_ci_config_default_miri_enabled() {
    let cfg = CiConfig::default_config();
    assert!(cfg.miri.enabled);
    assert!(cfg.miri.stacked_borrows);
    assert!(cfg.miri.isolation);
}

#[test]
fn wave25_ci_config_default_coverage_threshold() {
    let cfg = CiConfig::default_config();
    assert!(cfg.coverage.enabled);
    assert!((cfg.coverage.threshold_percent - 80.0).abs() < f64::EPSILON);
}

#[test]
fn wave25_ci_config_default_sanitizers_off() {
    let cfg = CiConfig::default_config();
    assert!(!cfg.sanitizers.asan);
    assert!(!cfg.sanitizers.msan);
    assert!(!cfg.sanitizers.tsan);
    assert!(!cfg.sanitizers.valgrind);
}

#[test]
fn wave25_ci_config_strict_all_sanitizers() {
    let cfg = CiConfig::strict_config();
    assert!(cfg.sanitizers.asan);
    assert!(cfg.sanitizers.msan);
    assert!(cfg.sanitizers.tsan);
    assert!(cfg.sanitizers.ubsan);
    assert!(cfg.sanitizers.valgrind);
}

#[test]
fn wave25_ci_config_strict_high_coverage() {
    let cfg = CiConfig::strict_config();
    assert!((cfg.coverage.threshold_percent - 90.0).abs() < f64::EPSILON);
    assert!(cfg.coverage.fail_on_decrease);
}

#[test]
fn wave25_ci_config_minimal_miri_disabled() {
    let cfg = CiConfig::minimal_config();
    assert!(!cfg.miri.enabled);
}

#[test]
fn wave25_ci_config_minimal_low_threshold() {
    let cfg = CiConfig::minimal_config();
    assert!((cfg.coverage.threshold_percent - 50.0).abs() < f64::EPSILON);
}

#[test]
fn wave25_ci_config_minimal_no_fail_on_decrease() {
    let cfg = CiConfig::minimal_config();
    assert!(!cfg.coverage.fail_on_decrease);
}

// =============================================================================
// MiriConfig
// =============================================================================

#[test]
fn wave25_ci_miri_default_has_flags() {
    let m = MiriConfig::default_config();
    assert!(!m.flags.is_empty());
    assert!(m.flags.iter().any(|f| f.contains("symbolic-alignment")));
}

#[test]
fn wave25_ci_miri_with_custom_flags() {
    let m = MiriConfig::with_flags(&["-Zmiri-disable-validation"]);
    assert_eq!(m.flags.len(), 1);
    assert_eq!(m.flags[0], "-Zmiri-disable-validation");
    // Other defaults preserved
    assert!(m.stacked_borrows);
    assert!(m.isolation);
}

// =============================================================================
// Miri command generation
// =============================================================================

#[test]
fn wave25_ci_miri_command_disabled_returns_empty() {
    let mut m = MiriConfig::default_config();
    m.enabled = false;
    assert!(CiRunner::miri_command(&m).is_empty());
}

#[test]
fn wave25_ci_miri_command_includes_nightly() {
    let m = MiriConfig::default_config();
    let cmds = CiRunner::miri_command(&m);
    assert!(cmds.iter().any(|c| c.contains("nightly")));
}

#[test]
fn wave25_ci_miri_command_includes_stacked_borrows_flag() {
    let m = MiriConfig::default_config();
    let cmds = CiRunner::miri_command(&m);
    let env_line = cmds.iter().find(|c| c.starts_with("export")).unwrap();
    assert!(env_line.contains("stacked-borrows"));
}

#[test]
fn wave25_ci_miri_command_includes_timeout() {
    let m = MiriConfig::default_config();
    let cmds = CiRunner::miri_command(&m);
    assert!(cmds.iter().any(|c| c.starts_with("timeout")));
}

#[test]
fn wave25_ci_miri_command_exclusions() {
    let mut m = MiriConfig::default_config();
    m.excluded_tests = vec!["lumen-wasm".to_string()];
    let cmds = CiRunner::miri_command(&m);
    assert!(cmds.iter().any(|c| c.contains("--exclude lumen-wasm")));
}

// =============================================================================
// Coverage command generation
// =============================================================================

#[test]
fn wave25_ci_coverage_command_disabled_returns_empty() {
    let mut c = CoverageConfig::default_config();
    c.enabled = false;
    assert!(CiRunner::coverage_command(&c).is_empty());
}

#[test]
fn wave25_ci_coverage_command_tarpaulin() {
    let c = CoverageConfig {
        enabled: true,
        threshold_percent: 80.0,
        tool: CoverageTool::Tarpaulin,
        exclude_patterns: vec![],
        fail_on_decrease: false,
        report_format: CoverageFormat::Html,
    };
    let cmds = CiRunner::coverage_command(&c);
    assert!(!cmds.is_empty());
    assert!(cmds[0].contains("cargo tarpaulin"));
    assert!(cmds[0].contains("--out Html"));
}

#[test]
fn wave25_ci_coverage_command_llvm_cov() {
    let c = CoverageConfig {
        enabled: true,
        threshold_percent: 80.0,
        tool: CoverageTool::LlvmCov,
        exclude_patterns: vec![],
        fail_on_decrease: false,
        report_format: CoverageFormat::Lcov,
    };
    let cmds = CiRunner::coverage_command(&c);
    assert!(cmds.iter().any(|c| c.contains("cargo llvm-cov")));
    assert!(cmds.iter().any(|c| c.contains("--lcov")));
}

#[test]
fn wave25_ci_coverage_command_grcov() {
    let c = CoverageConfig {
        enabled: true,
        threshold_percent: 80.0,
        tool: CoverageTool::Grcov,
        exclude_patterns: vec![],
        fail_on_decrease: false,
        report_format: CoverageFormat::Html,
    };
    let cmds = CiRunner::coverage_command(&c);
    assert!(cmds.iter().any(|c| c.contains("grcov")));
    assert!(cmds.iter().any(|c| c.contains("instrument-coverage")));
}

#[test]
fn wave25_ci_coverage_command_exclude_patterns() {
    let c = CoverageConfig {
        enabled: true,
        threshold_percent: 80.0,
        tool: CoverageTool::Tarpaulin,
        exclude_patterns: vec!["tests/*".to_string()],
        fail_on_decrease: false,
        report_format: CoverageFormat::Summary,
    };
    let cmds = CiRunner::coverage_command(&c);
    assert!(cmds.iter().any(|c| c.contains("--exclude-files")));
}

// =============================================================================
// Sanitizer command generation
// =============================================================================

#[test]
fn wave25_ci_sanitizer_commands_none_enabled() {
    let s = SanitizerConfig::default_config();
    assert!(CiRunner::sanitizer_commands(&s).is_empty());
}

#[test]
fn wave25_ci_sanitizer_commands_asan() {
    let mut s = SanitizerConfig::default_config();
    s.asan = true;
    let cmds = CiRunner::sanitizer_commands(&s);
    assert_eq!(cmds.len(), 1);
    assert!(cmds[0].contains("sanitizer=address"));
}

#[test]
fn wave25_ci_sanitizer_commands_multiple() {
    let mut s = SanitizerConfig::default_config();
    s.asan = true;
    s.tsan = true;
    let cmds = CiRunner::sanitizer_commands(&s);
    assert_eq!(cmds.len(), 2);
}

#[test]
fn wave25_ci_sanitizer_commands_ubsan() {
    let mut s = SanitizerConfig::default_config();
    s.ubsan = true;
    let cmds = CiRunner::sanitizer_commands(&s);
    assert!(cmds[0].contains("sanitizer=undefined"));
}

// =============================================================================
// Valgrind command generation
// =============================================================================

#[test]
fn wave25_ci_valgrind_command_disabled() {
    let s = SanitizerConfig::default_config();
    assert!(CiRunner::valgrind_command(&s, "test_bin").is_empty());
}

#[test]
fn wave25_ci_valgrind_command_enabled() {
    let mut s = SanitizerConfig::default_config();
    s.valgrind = true;
    let cmds = CiRunner::valgrind_command(&s, "./target/debug/my_test");
    assert_eq!(cmds.len(), 1);
    assert!(cmds[0].starts_with("valgrind"));
    assert!(cmds[0].contains("./target/debug/my_test"));
}

#[test]
fn wave25_ci_valgrind_command_includes_flags() {
    let mut s = SanitizerConfig::default_config();
    s.valgrind = true;
    let cmds = CiRunner::valgrind_command(&s, "test_bin");
    assert!(cmds[0].contains("--leak-check=full"));
    assert!(cmds[0].contains("--error-exitcode=1"));
}

#[test]
fn wave25_ci_valgrind_command_with_suppressions() {
    let mut s = SanitizerConfig::default_config();
    s.valgrind = true;
    s.suppressions = vec!["lumen.supp".to_string()];
    let cmds = CiRunner::valgrind_command(&s, "test_bin");
    assert!(cmds[0].contains("--suppressions=lumen.supp"));
}

// =============================================================================
// Coverage parsing — tarpaulin
// =============================================================================

#[test]
fn wave25_ci_parse_tarpaulin_output() {
    let output = "running tests...\n85.32% coverage, 1200/1407 lines covered\n";
    let result = CiRunner::parse_coverage_summary(output).unwrap();
    assert!((result.line_coverage - 85.32).abs() < 0.01);
}

#[test]
fn wave25_ci_parse_tarpaulin_integer_coverage() {
    let output = "100% coverage, 500/500 lines covered";
    let result = CiRunner::parse_coverage_summary(output).unwrap();
    assert!((result.line_coverage - 100.0).abs() < 0.01);
}

// =============================================================================
// Coverage parsing — llvm-cov
// =============================================================================

#[test]
fn wave25_ci_parse_llvm_cov_output() {
    let output = "Filename  Regions  Functions  Lines\n---\nTOTAL     1234     567        89.7%\n";
    let result = CiRunner::parse_coverage_summary(output).unwrap();
    assert!((result.line_coverage - 89.7).abs() < 0.1);
}

// =============================================================================
// Coverage parsing — grcov
// =============================================================================

#[test]
fn wave25_ci_parse_grcov_output() {
    let output = "lines......: 78.5% (1200 of 1528 lines)\nfunctions..: 65.3%\n";
    let result = CiRunner::parse_coverage_summary(output).unwrap();
    assert!((result.line_coverage - 78.5).abs() < 0.1);
}

// =============================================================================
// Coverage parsing — no match
// =============================================================================

#[test]
fn wave25_ci_parse_coverage_unrecognised_format() {
    let output = "some random output with no coverage info";
    assert!(CiRunner::parse_coverage_summary(output).is_none());
}

// =============================================================================
// Coverage gate
// =============================================================================

#[test]
fn wave25_ci_coverage_gate_pass() {
    let result = cov_result(85.0);
    let cfg = CoverageConfig::default_config(); // threshold 80
    match CiRunner::check_coverage_gate(&result, &cfg) {
        CoverageGateResult::Pass {
            coverage,
            threshold,
        } => {
            assert!((coverage - 85.0).abs() < f64::EPSILON);
            assert!((threshold - 80.0).abs() < f64::EPSILON);
        }
        other => panic!("Expected Pass, got {:?}", other),
    }
}

#[test]
fn wave25_ci_coverage_gate_fail() {
    let result = cov_result(50.0);
    let cfg = CoverageConfig::default_config();
    match CiRunner::check_coverage_gate(&result, &cfg) {
        CoverageGateResult::Fail {
            coverage, message, ..
        } => {
            assert!((coverage - 50.0).abs() < f64::EPSILON);
            assert!(message.contains("50.0%"));
        }
        other => panic!("Expected Fail, got {:?}", other),
    }
}

#[test]
fn wave25_ci_coverage_gate_exact_threshold() {
    let result = cov_result(80.0);
    let cfg = CoverageConfig::default_config();
    let gate = CiRunner::check_coverage_gate(&result, &cfg);
    assert!(gate.is_pass());
}

#[test]
fn wave25_ci_coverage_gate_decrease_detection() {
    let current = cov_result(75.0);
    let mut cfg = CoverageConfig::default_config();
    cfg.fail_on_decrease = true;
    cfg.threshold_percent = 70.0;

    match CiRunner::check_coverage_decrease(80.0, &current, &cfg) {
        CoverageGateResult::Decreased { previous, current } => {
            assert!((previous - 80.0).abs() < f64::EPSILON);
            assert!((current - 75.0).abs() < f64::EPSILON);
        }
        other => panic!("Expected Decreased, got {:?}", other),
    }
}

#[test]
fn wave25_ci_coverage_gate_no_decrease_when_disabled() {
    let current = cov_result(75.0);
    let mut cfg = CoverageConfig::default_config();
    cfg.fail_on_decrease = false;
    cfg.threshold_percent = 70.0;

    let gate = CiRunner::check_coverage_decrease(80.0, &current, &cfg);
    assert!(gate.is_pass());
}

// =============================================================================
// CoverageResult with uncovered regions
// =============================================================================

#[test]
fn wave25_ci_coverage_result_with_uncovered_regions() {
    let result = CoverageResult {
        line_coverage: 75.0,
        branch_coverage: Some(60.0),
        function_coverage: Some(80.0),
        uncovered_lines: vec![
            UncoveredRegion {
                file: "src/main.rs".to_string(),
                start_line: 10,
                end_line: 20,
            },
            UncoveredRegion {
                file: "src/lib.rs".to_string(),
                start_line: 5,
                end_line: 8,
            },
        ],
    };
    assert_eq!(result.uncovered_lines.len(), 2);
    assert_eq!(result.uncovered_lines[0].file, "src/main.rs");
    assert_eq!(result.uncovered_lines[1].start_line, 5);
}

#[test]
fn wave25_ci_coverage_result_full_metrics() {
    let result = cov_result_full(85.0, 70.0, 90.0);
    assert!((result.branch_coverage.unwrap() - 70.0).abs() < f64::EPSILON);
    assert!((result.function_coverage.unwrap() - 90.0).abs() < f64::EPSILON);
}

// =============================================================================
// CiReport
// =============================================================================

#[test]
fn wave25_ci_report_new_is_empty() {
    let report = CiReport::new();
    assert!(report.sections.is_empty());
}

#[test]
fn wave25_ci_report_add_section() {
    let mut report = CiReport::new();
    report.add_section(CiReportSection {
        name: "test".to_string(),
        status: CiStatus::Pass,
        details: String::new(),
        duration_ms: 100,
    });
    assert_eq!(report.sections.len(), 1);
}

#[test]
fn wave25_ci_report_overall_pass() {
    let mut report = CiReport::new();
    report.add_section(CiReportSection {
        name: "test".to_string(),
        status: CiStatus::Pass,
        details: String::new(),
        duration_ms: 100,
    });
    assert!(matches!(report.overall_status(), CiStatus::Pass));
}

#[test]
fn wave25_ci_report_overall_fail() {
    let mut report = CiReport::new();
    report.add_section(CiReportSection {
        name: "test".to_string(),
        status: CiStatus::Pass,
        details: String::new(),
        duration_ms: 100,
    });
    report.add_section(CiReportSection {
        name: "miri".to_string(),
        status: CiStatus::Fail("UB detected".to_string()),
        details: String::new(),
        duration_ms: 200,
    });
    assert!(matches!(report.overall_status(), CiStatus::Fail(_)));
}

#[test]
fn wave25_ci_report_overall_warning() {
    let mut report = CiReport::new();
    report.add_section(CiReportSection {
        name: "test".to_string(),
        status: CiStatus::Pass,
        details: String::new(),
        duration_ms: 100,
    });
    report.add_section(CiReportSection {
        name: "coverage".to_string(),
        status: CiStatus::Warning("low coverage".to_string()),
        details: String::new(),
        duration_ms: 50,
    });
    assert!(matches!(report.overall_status(), CiStatus::Warning(_)));
}

#[test]
fn wave25_ci_report_overall_all_skip() {
    let mut report = CiReport::new();
    report.add_section(CiReportSection {
        name: "miri".to_string(),
        status: CiStatus::Skip("disabled".to_string()),
        details: String::new(),
        duration_ms: 0,
    });
    assert!(matches!(report.overall_status(), CiStatus::Skip(_)));
}

#[test]
fn wave25_ci_report_overall_empty() {
    let report = CiReport::new();
    assert!(matches!(report.overall_status(), CiStatus::Skip(_)));
}

#[test]
fn wave25_ci_report_summary_format() {
    let mut report = CiReport::new();
    report.add_section(CiReportSection {
        name: "test".to_string(),
        status: CiStatus::Pass,
        details: String::new(),
        duration_ms: 100,
    });
    report.add_section(CiReportSection {
        name: "miri".to_string(),
        status: CiStatus::Fail("UB".to_string()),
        details: String::new(),
        duration_ms: 200,
    });
    let summary = report.summary();
    assert!(summary.contains("1 passed"));
    assert!(summary.contains("1 failed"));
    assert!(summary.contains("300 ms"));
}

// =============================================================================
// CiReport — markdown rendering
// =============================================================================

#[test]
fn wave25_ci_report_markdown_has_header() {
    let report = CiReport::new();
    let md = report.to_markdown();
    assert!(md.contains("# CI Report"));
}

#[test]
fn wave25_ci_report_markdown_table() {
    let mut report = CiReport::new();
    report.add_section(CiReportSection {
        name: "test".to_string(),
        status: CiStatus::Pass,
        details: String::new(),
        duration_ms: 150,
    });
    let md = report.to_markdown();
    assert!(md.contains("| test | PASS | 150 ms |"));
}

#[test]
fn wave25_ci_report_markdown_details_section() {
    let mut report = CiReport::new();
    report.add_section(CiReportSection {
        name: "miri".to_string(),
        status: CiStatus::Fail("UB".to_string()),
        details: "Undefined behavior in foo.rs:42".to_string(),
        duration_ms: 500,
    });
    let md = report.to_markdown();
    assert!(md.contains("## Details"));
    assert!(md.contains("### miri"));
    assert!(md.contains("Undefined behavior in foo.rs:42"));
}

// =============================================================================
// CiReport — JSON rendering
// =============================================================================

#[test]
fn wave25_ci_report_json_valid() {
    let mut report = CiReport::new();
    report.add_section(CiReportSection {
        name: "test".to_string(),
        status: CiStatus::Pass,
        details: String::new(),
        duration_ms: 100,
    });
    let json = report.to_json();
    assert!(json.contains("\"overall\""));
    assert!(json.contains("\"sections\""));
    // Should be parseable
    assert!(json.starts_with('{'));
    assert!(json.ends_with('}'));
}

#[test]
fn wave25_ci_report_json_escape() {
    let mut report = CiReport::new();
    report.add_section(CiReportSection {
        name: "test".to_string(),
        status: CiStatus::Fail("error with \"quotes\"".to_string()),
        details: "line1\nline2".to_string(),
        duration_ms: 100,
    });
    let json = report.to_json();
    assert!(json.contains("\\\"quotes\\\""));
    assert!(json.contains("\\n"));
}

// =============================================================================
// GitHub Actions YAML generation
// =============================================================================

#[test]
fn wave25_ci_github_actions_yaml_default() {
    let cfg = CiConfig::default_config();
    let yaml = CiRunner::github_actions_yaml(&cfg);
    assert!(yaml.contains("name: CI"));
    assert!(yaml.contains("on:"));
    assert!(yaml.contains("jobs:"));
    assert!(yaml.contains("test:"));
}

#[test]
fn wave25_ci_github_actions_yaml_miri_job() {
    let cfg = CiConfig::default_config();
    let yaml = CiRunner::github_actions_yaml(&cfg);
    assert!(yaml.contains("miri:"));
    assert!(yaml.contains("components: miri"));
}

#[test]
fn wave25_ci_github_actions_yaml_coverage_job() {
    let cfg = CiConfig::default_config();
    let yaml = CiRunner::github_actions_yaml(&cfg);
    assert!(yaml.contains("coverage:"));
    assert!(yaml.contains("tarpaulin"));
}

#[test]
fn wave25_ci_github_actions_yaml_no_sanitizers_when_disabled() {
    let cfg = CiConfig::default_config();
    let yaml = CiRunner::github_actions_yaml(&cfg);
    assert!(!yaml.contains("sanitizers:"));
}

#[test]
fn wave25_ci_github_actions_yaml_sanitizers_when_enabled() {
    let cfg = CiConfig::strict_config();
    let yaml = CiRunner::github_actions_yaml(&cfg);
    assert!(yaml.contains("sanitizers:"));
    assert!(yaml.contains("sanitizer=address"));
}

#[test]
fn wave25_ci_github_actions_yaml_valgrind_when_enabled() {
    let cfg = CiConfig::strict_config();
    let yaml = CiRunner::github_actions_yaml(&cfg);
    assert!(yaml.contains("valgrind:"));
    assert!(yaml.contains("apt-get install"));
}

// =============================================================================
// GitLab CI YAML generation
// =============================================================================

#[test]
fn wave25_ci_gitlab_ci_yaml_stages() {
    let cfg = CiConfig::default_config();
    let yaml = CiRunner::gitlab_ci_yaml(&cfg);
    assert!(yaml.contains("stages:"));
    assert!(yaml.contains("- test"));
}

#[test]
fn wave25_ci_gitlab_ci_yaml_test_job() {
    let cfg = CiConfig::default_config();
    let yaml = CiRunner::gitlab_ci_yaml(&cfg);
    assert!(yaml.contains("test:\n"));
    assert!(yaml.contains("stage: test"));
    assert!(yaml.contains("image: rust:latest"));
}

#[test]
fn wave25_ci_gitlab_ci_yaml_miri_stage() {
    let cfg = CiConfig::default_config();
    let yaml = CiRunner::gitlab_ci_yaml(&cfg);
    assert!(yaml.contains("- miri"));
    assert!(yaml.contains("miri:\n"));
    assert!(yaml.contains("stage: miri"));
}

#[test]
fn wave25_ci_gitlab_ci_yaml_no_sanitizer_stage_when_disabled() {
    let cfg = CiConfig::default_config();
    let yaml = CiRunner::gitlab_ci_yaml(&cfg);
    assert!(!yaml.contains("- sanitizers"));
}

#[test]
fn wave25_ci_gitlab_ci_yaml_all_stages_strict() {
    let cfg = CiConfig::strict_config();
    let yaml = CiRunner::gitlab_ci_yaml(&cfg);
    assert!(yaml.contains("- test"));
    assert!(yaml.contains("- miri"));
    assert!(yaml.contains("- coverage"));
    assert!(yaml.contains("- sanitizers"));
}

// =============================================================================
// CiStatus Display
// =============================================================================

#[test]
fn wave25_ci_status_display_pass() {
    assert_eq!(format!("{}", CiStatus::Pass), "pass");
}

#[test]
fn wave25_ci_status_display_fail() {
    assert_eq!(
        format!("{}", CiStatus::Fail("oops".to_string())),
        "fail: oops"
    );
}

#[test]
fn wave25_ci_status_display_skip() {
    assert_eq!(
        format!("{}", CiStatus::Skip("reason".to_string())),
        "skip: reason"
    );
}

#[test]
fn wave25_ci_status_display_warning() {
    assert_eq!(
        format!("{}", CiStatus::Warning("slow".to_string())),
        "warning: slow"
    );
}

// =============================================================================
// CoverageTool / CoverageFormat Display
// =============================================================================

#[test]
fn wave25_ci_coverage_tool_display() {
    assert_eq!(format!("{}", CoverageTool::Tarpaulin), "tarpaulin");
    assert_eq!(format!("{}", CoverageTool::LlvmCov), "llvm-cov");
    assert_eq!(format!("{}", CoverageTool::Grcov), "grcov");
}

#[test]
fn wave25_ci_coverage_format_display() {
    assert_eq!(format!("{}", CoverageFormat::Html), "html");
    assert_eq!(format!("{}", CoverageFormat::Lcov), "lcov");
    assert_eq!(format!("{}", CoverageFormat::Json), "json");
    assert_eq!(format!("{}", CoverageFormat::Summary), "summary");
}

// =============================================================================
// TestConfig
// =============================================================================

#[test]
fn wave25_ci_test_config_defaults() {
    let tc = TestConfig::default_config();
    assert!(tc.parallel);
    assert_eq!(tc.timeout_secs, 300);
    assert_eq!(tc.retry_failed, 2);
    assert!(!tc.fail_fast);
}

// =============================================================================
// Coverage command format variations
// =============================================================================

#[test]
fn wave25_ci_coverage_tarpaulin_lcov_format() {
    let c = CoverageConfig {
        enabled: true,
        threshold_percent: 80.0,
        tool: CoverageTool::Tarpaulin,
        exclude_patterns: vec![],
        fail_on_decrease: false,
        report_format: CoverageFormat::Lcov,
    };
    let cmds = CiRunner::coverage_command(&c);
    assert!(cmds[0].contains("--out Lcov"));
}

#[test]
fn wave25_ci_coverage_tarpaulin_json_format() {
    let c = CoverageConfig {
        enabled: true,
        threshold_percent: 80.0,
        tool: CoverageTool::Tarpaulin,
        exclude_patterns: vec![],
        fail_on_decrease: false,
        report_format: CoverageFormat::Json,
    };
    let cmds = CiRunner::coverage_command(&c);
    assert!(cmds[0].contains("--out Json"));
}

#[test]
fn wave25_ci_coverage_llvm_cov_json_format() {
    let c = CoverageConfig {
        enabled: true,
        threshold_percent: 80.0,
        tool: CoverageTool::LlvmCov,
        exclude_patterns: vec![],
        fail_on_decrease: false,
        report_format: CoverageFormat::Json,
    };
    let cmds = CiRunner::coverage_command(&c);
    assert!(cmds.iter().any(|c| c.contains("--json")));
}

#[test]
fn wave25_ci_coverage_llvm_cov_exclude_patterns() {
    let c = CoverageConfig {
        enabled: true,
        threshold_percent: 80.0,
        tool: CoverageTool::LlvmCov,
        exclude_patterns: vec!["test_.*".to_string()],
        fail_on_decrease: false,
        report_format: CoverageFormat::Summary,
    };
    let cmds = CiRunner::coverage_command(&c);
    assert!(cmds.iter().any(|c| c.contains("--ignore-filename-regex")));
}

// =============================================================================
// Edge cases
// =============================================================================

#[test]
fn wave25_ci_miri_command_no_timeout() {
    let mut m = MiriConfig::default_config();
    m.timeout_secs = 0;
    let cmds = CiRunner::miri_command(&m);
    assert!(!cmds.iter().any(|c| c.starts_with("timeout")));
}

#[test]
fn wave25_ci_coverage_gate_result_is_pass_method() {
    let pass = CoverageGateResult::Pass {
        coverage: 90.0,
        threshold: 80.0,
    };
    let fail = CoverageGateResult::Fail {
        coverage: 50.0,
        threshold: 80.0,
        message: "too low".to_string(),
    };
    let decreased = CoverageGateResult::Decreased {
        previous: 80.0,
        current: 75.0,
    };
    assert!(pass.is_pass());
    assert!(!fail.is_pass());
    assert!(!decreased.is_pass());
}

#[test]
fn wave25_ci_github_actions_yaml_minimal_no_miri_job() {
    let cfg = CiConfig::minimal_config();
    let yaml = CiRunner::github_actions_yaml(&cfg);
    assert!(!yaml.contains("  miri:"));
}

#[test]
fn wave25_ci_github_actions_fail_fast_flag() {
    let mut cfg = CiConfig::default_config();
    cfg.test_config.fail_fast = true;
    let yaml = CiRunner::github_actions_yaml(&cfg);
    assert!(yaml.contains("--fail-fast"));
}

#[test]
fn wave25_ci_gitlab_ci_fail_fast_flag() {
    let mut cfg = CiConfig::default_config();
    cfg.test_config.fail_fast = true;
    let yaml = CiRunner::gitlab_ci_yaml(&cfg);
    assert!(yaml.contains("--fail-fast"));
}

#[test]
fn wave25_ci_report_no_details_section_when_empty() {
    let mut report = CiReport::new();
    report.add_section(CiReportSection {
        name: "test".to_string(),
        status: CiStatus::Pass,
        details: String::new(),
        duration_ms: 100,
    });
    let md = report.to_markdown();
    assert!(!md.contains("## Details"));
}
