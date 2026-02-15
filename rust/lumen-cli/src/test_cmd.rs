//! Lumen test runner — discovers and executes test_* cells.

use lumen_vm::values::Value;
use lumen_vm::vm::VM;
use std::fs;
use std::path::{Path, PathBuf};

fn green(s: &str) -> String {
    format!("\x1b[32m{}\x1b[0m", s)
}

fn red(s: &str) -> String {
    format!("\x1b[31m{}\x1b[0m", s)
}

fn gray(s: &str) -> String {
    format!("\x1b[90m{}\x1b[0m", s)
}

fn bold(s: &str) -> String {
    format!("\x1b[1m{}\x1b[0m", s)
}

fn status_label(label: &str) -> String {
    format!("\x1b[1;32m{:>12}\x1b[0m", label)
}

#[derive(Debug)]
struct TestResult {
    #[allow(dead_code)]
    file: String,
    test_name: String,
    passed: bool,
    error_message: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct TestRunSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
}

impl TestRunSummary {
    pub fn is_success(&self) -> bool {
        self.failed == 0
    }
}

pub fn run_tests(
    path: Option<PathBuf>,
    filter: Option<&str>,
    verbose: bool,
) -> Result<TestRunSummary, String> {
    let target_path = path.unwrap_or_else(|| PathBuf::from("."));

    // Collect all supported Lumen source files.
    let mut test_files = Vec::new();
    collect_test_files(&target_path, &mut test_files);

    if test_files.is_empty() {
        return Err(format!(
            "no lumen source files (.lm/.lumen/.lm.md/.lumen.md) found in {}",
            target_path.display()
        ));
    }

    // Run tests and collect results
    let mut results = Vec::new();
    let mut total_tests = 0;

    for file_path in &test_files {
        let source = match fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                results.push(TestResult {
                    file: file_path.display().to_string(),
                    test_name: "<load>".to_string(),
                    passed: false,
                    error_message: Some(format!("cannot read file: {}", e)),
                });
                total_tests += 1;
                continue;
            }
        };

        let filename = file_path.display().to_string();
        let module = match crate::module_resolver::compile_source_file(file_path, &source) {
            Ok(m) => m,
            Err(e) => {
                let mut error_message = "compilation failed".to_string();
                if verbose {
                    let formatted = lumen_compiler::format_error(&e, &source, &filename);
                    error_message = format!("compilation failed\n{}", formatted);
                }
                results.push(TestResult {
                    file: filename,
                    test_name: "<compile>".to_string(),
                    passed: false,
                    error_message: Some(error_message),
                });
                total_tests += 1;
                continue;
            }
        };

        // Find all test_* cells
        let test_cells: Vec<_> = module
            .cells
            .iter()
            .filter(|c| c.name.starts_with("test_"))
            .filter(|c| {
                if let Some(f) = filter {
                    c.name.contains(f)
                } else {
                    true
                }
            })
            .collect();

        if test_cells.is_empty() {
            if verbose {
                println!("{} no test cells found in {}", gray("info:"), filename);
            }
            continue;
        }

        total_tests += test_cells.len();

        // Run each test cell
        for cell in test_cells {
            let test_name = cell.name.clone();
            let mut vm = VM::new();
            let registry = lumen_runtime::tools::ProviderRegistry::new();
            vm.set_provider_registry(registry);
            vm.load(module.clone());

            let result = match vm.execute(&test_name, vec![]) {
                Ok(value) => {
                    // A test passes if it returns Bool(true) or any value without error
                    // A test fails if it returns Bool(false)
                    match value {
                        Value::Bool(false) => TestResult {
                            file: filename.clone(),
                            test_name: test_name.clone(),
                            passed: false,
                            error_message: Some("returned: false".to_string()),
                        },
                        _ => TestResult {
                            file: filename.clone(),
                            test_name: test_name.clone(),
                            passed: true,
                            error_message: None,
                        },
                    }
                }
                Err(e) => TestResult {
                    file: filename.clone(),
                    test_name: test_name.clone(),
                    passed: false,
                    error_message: Some(e.to_string()),
                },
            };

            results.push(result);
        }
    }

    // Print running summary with status label
    println!(
        "{} {} test{}",
        status_label("Running"),
        total_tests,
        if total_tests == 1 { "" } else { "s" }
    );

    let start = std::time::Instant::now();

    // Print results as they come
    let mut passed = 0;
    let mut failed = 0;

    for result in &results {
        let status = if result.passed {
            passed += 1;
            green("✓ ok")
        } else {
            failed += 1;
            red("✗ FAILED")
        };

        println!(
            "  {} {} ... {}",
            gray("test"),
            bold(&result.test_name),
            status
        );
    }

    // Print failure details
    if failed > 0 {
        println!("\n{}", bold("--- FAILURES ---"));
        for result in &results {
            if !result.passed {
                println!("  {} {}:", gray("test"), bold(&result.test_name));
                if let Some(ref msg) = result.error_message {
                    println!("    {}", msg);
                }
                println!();
            }
        }
    }

    let elapsed = start.elapsed();

    // Print summary
    if failed == 0 {
        println!(
            "{} Finished in {:.2}s — {} passed, {} failed",
            green("✓"),
            elapsed.as_secs_f64(),
            passed,
            failed
        );
    } else {
        println!(
            "{} Finished in {:.2}s — {} passed, {} failed",
            red("✗"),
            elapsed.as_secs_f64(),
            passed,
            failed
        );
    }

    Ok(TestRunSummary {
        total: total_tests,
        passed,
        failed,
    })
}

pub fn cmd_test(path: Option<PathBuf>, filter: Option<String>, verbose: bool) {
    match run_tests(path, filter.as_deref(), verbose) {
        Ok(summary) => {
            if !summary.is_success() {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("{} {}", red("error:"), e);
            std::process::exit(1);
        }
    }
}

fn collect_test_files(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_file() {
        if is_lumen_source(path) {
            files.push(path.to_path_buf());
        }
    } else if path.is_dir() {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                collect_test_files(&entry.path(), files);
            }
        }
    }
}

fn is_lumen_source(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.ends_with(".lm.md") || n.ends_with(".lumen.md"))
        .unwrap_or(false)
        || matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("lm") | Some("lumen")
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), stamp));
            fs::create_dir_all(&path).expect("should create temp test directory");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn run_tests_resolves_imports_from_project_src_context() {
        let temp = TempDir::new("lumen_test_pkg_context");
        let root = temp.path();
        let src_dir = root.join("src");
        let tests_dir = root.join("tests");

        fs::create_dir_all(&src_dir).expect("should create src dir");
        fs::create_dir_all(&tests_dir).expect("should create tests dir");
        fs::write(
            root.join("lumen.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        )
        .expect("should write lumen.toml");
        fs::write(
            src_dir.join("math.lm"),
            "cell add(a: Int, b: Int) -> Int\n  return a + b\nend\n",
        )
        .expect("should write module");
        fs::write(
            tests_dir.join("math_test.lm.md"),
            "```lumen\nimport math: add\n```\n\n```lumen\ncell test_add() -> Bool\n  return add(2, 3) == 5\nend\n```\n",
        )
        .expect("should write test file");

        let summary = run_tests(Some(root.to_path_buf()), None, false)
            .expect("tests should run with package-context imports");

        assert_eq!(summary.total, 1);
        assert_eq!(summary.passed, 1);
        assert_eq!(summary.failed, 0);
    }

    #[test]
    fn run_tests_discovers_lm_and_markdown_sources() {
        let temp = TempDir::new("lumen_test_source_discovery");
        let root = temp.path();

        fs::write(
            root.join("raw_test.lm"),
            "cell test_raw() -> Bool\n  return true\nend\n",
        )
        .expect("should write raw test file");
        fs::write(
            root.join("markdown_test.lm.md"),
            "```lumen\ncell test_markdown() -> Bool\n  return true\nend\n```\n",
        )
        .expect("should write markdown test file");

        let summary = run_tests(Some(root.to_path_buf()), None, false).expect("tests should run");

        assert_eq!(summary.total, 2);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 0);
    }
}
