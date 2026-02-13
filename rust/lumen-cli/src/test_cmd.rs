//! Lumen test runner — discovers and executes test_* cells.

use lumen_compiler;
use lumen_vm::vm::VM;
use lumen_vm::values::Value;
use std::path::{Path, PathBuf};
use std::fs;

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

#[derive(Debug)]
struct TestResult {
    file: String,
    test_name: String,
    passed: bool,
    error_message: Option<String>,
}

pub fn cmd_test(
    path: Option<PathBuf>,
    filter: Option<String>,
    verbose: bool,
) {
    let target_path = path.unwrap_or_else(|| PathBuf::from("."));

    // Collect all .lm.md files
    let mut test_files = Vec::new();
    collect_test_files(&target_path, &mut test_files);

    if test_files.is_empty() {
        eprintln!("{} no .lm.md files found in {}", red("error:"), target_path.display());
        std::process::exit(1);
    }

    // Run tests and collect results
    let mut results = Vec::new();
    let mut total_tests = 0;

    for file_path in &test_files {
        let source = match fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{} cannot read {}: {}", red("error:"), file_path.display(), e);
                continue;
            }
        };

        let filename = file_path.display().to_string();
        let module = match lumen_compiler::compile(&source) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("{} compilation failed for {}", red("error:"), filename);
                if verbose {
                    let formatted = lumen_compiler::format_error(&e, &source, &filename);
                    eprint!("{}", formatted);
                }
                continue;
            }
        };

        // Find all test_* cells
        let test_cells: Vec<_> = module.cells.iter()
            .filter(|c| c.name.starts_with("test_"))
            .filter(|c| {
                if let Some(ref f) = filter {
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

    // Print running summary
    println!("running {} test{}", total_tests, if total_tests == 1 { "" } else { "s" });

    // Print results as they come
    let mut passed = 0;
    let mut failed = 0;

    for result in &results {
        let status = if result.passed {
            passed += 1;
            green("ok")
        } else {
            failed += 1;
            red("FAILED")
        };

        println!("test {}::{} ... {}", result.file, result.test_name, status);
    }

    // Print failure details
    if failed > 0 {
        println!("\n{}", bold("--- FAILURES ---"));
        for result in &results {
            if !result.passed {
                println!("test {}::{}:", result.file, result.test_name);
                if let Some(ref msg) = result.error_message {
                    println!("  {}", msg);
                }
                println!();
            }
        }
    }

    // Print summary
    let summary = if failed == 0 {
        format!(
            "{} {}. {} passed; {} failed; 0 ignored",
            green("test result:"),
            green("ok"),
            passed,
            failed
        )
    } else {
        format!(
            "{} {}. {} passed; {} failed; 0 ignored",
            red("test result:"),
            red("FAILED"),
            passed,
            failed
        )
    };

    println!("{}", summary);

    if failed > 0 {
        std::process::exit(1);
    }
}

fn collect_test_files(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_file() {
        if path.extension().and_then(|s| s.to_str()) == Some("md")
            && path.to_str().map(|s| s.ends_with(".lm.md")).unwrap_or(false) {
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
