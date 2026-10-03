//! Differential tests: every program runs once with the JIT off and once with
//! the JIT forced on (hot threshold 0), and the observable behaviour (return
//! value or error message, plus everything printed) must be identical.
//!
//! Covered programs: `tests/fixtures/jit_diff/*.lm`, `bench/b_*.lm`,
//! `bench/cross-language/*/*.lm`, `examples/*.lm.md`, and every `.lm`/`.lm.md`
//! under the repo's `tests/` directory that has a `main` cell.
//!
//! In debug builds an instruction budget skips programs the interpreter
//! cannot finish quickly (the benchmarks); run
//! `cargo test --release -p lumen-runtime --test jit_differential` to include
//! them.

#![cfg(feature = "jit")]

use lumen_runtime::vm::vm::VM;
use std::path::{Path, PathBuf};

/// Interpreter instruction budget in debug builds.
const DEBUG_BUDGET: u64 = 10_000_000;

#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    /// `Ok(display of value)` or `Err(message without stack trace)`.
    result: Result<String, String>,
    output: Vec<String>,
}

enum Run {
    Done(Outcome),
    /// Over the instruction budget (only happens in debug builds).
    TooHeavy,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn compile_file(path: &Path) -> Option<lumen_compiler::compiler::lir::LirModule> {
    let src = std::fs::read_to_string(path).ok()?;
    let name = path.to_string_lossy();
    let r = if name.ends_with(".lm.md") || name.ends_with(".lumen") {
        lumen_compiler::compile(&src)
    } else {
        lumen_compiler::compile_raw(&src)
    };
    r.ok()
}

fn run_module(module: &lumen_compiler::compiler::lir::LirModule, jit: bool) -> Run {
    let mut vm = VM::new();
    if cfg!(debug_assertions) {
        vm.set_instruction_limit(DEBUG_BUDGET);
    }
    if jit {
        vm.enable_jit(0);
    }
    vm.load(module.clone());
    let res = vm.execute("main", vec![]);
    let result = match res {
        Ok(v) => Ok(format!("{v}")),
        Err(e) => {
            if e.is_instruction_limit_exceeded() && cfg!(debug_assertions) {
                return Run::TooHeavy;
            }
            // Compare the message only; the stack trace legitimately differs in
            // length when native code has already unwound frames.
            let msg = e.to_string();
            Err(msg
                .split("\nStack trace")
                .next()
                .unwrap_or(&msg)
                .to_string())
        }
    };
    Run::Done(Outcome {
        result,
        output: vm.output.clone(),
    })
}

/// Returns `Some(true)` if compared, `Some(false)` if skipped as too heavy,
/// `None` if the file does not compile or has no `main`.
fn check_file(path: &Path) -> Option<bool> {
    let module = compile_file(path)?;
    if !module.cells.iter().any(|c| c.name == "main") {
        return None;
    }
    let off = match run_module(&module, false) {
        Run::Done(o) => o,
        Run::TooHeavy => return Some(false),
    };
    let on = match run_module(&module, true) {
        Run::Done(o) => o,
        Run::TooHeavy => panic!(
            "{}: JIT run exceeded the instruction budget but the interpreter run did not",
            path.display()
        ),
    };
    assert_eq!(
        off,
        on,
        "{}: JIT-on behaviour differs from interpreter",
        path.display()
    );
    Some(true)
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>, recursive: bool) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            if recursive {
                collect(&p, out, recursive);
            }
        } else {
            let n = p.to_string_lossy();
            if n.ends_with(".lm") || n.ends_with(".lm.md") {
                out.push(p);
            }
        }
    }
}

fn run_dir(label: &str, dir: PathBuf, recursive: bool, min_compared: usize) {
    let mut files = Vec::new();
    collect(&dir, &mut files, recursive);
    let mut compared = 0;
    let mut skipped_heavy = Vec::new();
    for f in &files {
        match check_file(f) {
            Some(true) => compared += 1,
            Some(false) => skipped_heavy.push(f.display().to_string()),
            None => {}
        }
    }
    eprintln!("{label}: compared {compared} program(s)");
    if !skipped_heavy.is_empty() {
        eprintln!(
            "{label}: skipped {} heavy program(s) in a debug build: {:?}",
            skipped_heavy.len(),
            skipped_heavy
        );
    }
    assert!(
        compared + skipped_heavy.len() >= min_compared,
        "{label}: only {} runnable program(s) found in {} (expected >= {min_compared})",
        compared + skipped_heavy.len(),
        dir.display()
    );
}

#[test]
fn differential_fixtures() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/jit_diff");
    run_dir("fixtures", dir, false, 15);
}

#[test]
fn differential_bench_programs() {
    let root = repo_root().join("bench");
    run_dir("bench", root.clone(), false, 9);
    run_dir("bench/cross-language", root.join("cross-language"), true, 9);
}

#[test]
fn differential_examples() {
    run_dir("examples", repo_root().join("examples"), false, 10);
}

#[test]
fn differential_repo_tests_dir() {
    run_dir("tests", repo_root().join("tests"), true, 1);
}

/// The semantic spot checks below pin the exact answers (not just agreement
/// between tiers), so a regression in both tiers at once is still caught.
fn eval(src: &str, jit: bool) -> Result<String, String> {
    let module = lumen_compiler::compile_raw(src).expect("compiles");
    match run_module(&module, jit) {
        Run::Done(o) => o.result,
        Run::TooHeavy => panic!("too heavy"),
    }
}

#[test]
fn pinned_results_match_in_both_tiers() {
    let cases: &[(&str, &str)] = &[
        (
            "cell f(a: Int, b: Int) -> Int\n  return a // b\nend\ncell main() -> Int\n  return f(-7, 2)\nend\n",
            "-4",
        ),
        (
            "cell f(a: Int, b: Int) -> Int\n  return a % b\nend\ncell main() -> Int\n  return f(-7, 2)\nend\n",
            "1",
        ),
        (
            "cell p(n: Int) -> Bool\n  return n % 2 == 1\nend\ncell main() -> Bool\n  return p(3)\nend\n",
            "true",
        ),
    ];
    for (src, want) in cases {
        for jit in [false, true] {
            assert_eq!(eval(src, jit).as_deref(), Ok(*want), "jit={jit}: {src}");
        }
    }
    for (src, frag) in [
        (
            "cell f(a: Int, b: Int) -> Int\n  return a / b\nend\ncell main() -> Int\n  return f(7, 0)\nend\n",
            "division by zero",
        ),
        (
            "cell f(a: Int) -> Int\n  return a * 2\nend\ncell main() -> Int\n  return f(4611686018427387904)\nend\n",
            "overflow",
        ),
    ] {
        for jit in [false, true] {
            let err = eval(src, jit).expect_err("must error");
            assert!(err.contains(frag), "jit={jit}: {err}");
        }
    }
}

/// Guard against the differential suite passing vacuously because nothing was
/// ever compiled: the strict tier must actually run these natively, and must
/// fall back (not crash) when the native code traps.
#[test]
fn strict_tier_runs_natively_and_falls_back_on_trap() {
    let src = "cell isodd(n: Int) -> Bool\n  return n % 2 == 1\nend\n\
               cell dbl(n: Int) -> Int\n  return n * 2\nend\n\
               cell main() -> Int\n  let mut c = 0\n  let mut i = 0\n  \
               while i < 10\n    if isodd(i)\n      c = c + 1\n    end\n    i = i + 1\n  end\n  \
               print(dbl(4611686018427387904))\n  return c\nend\n";
    let module = lumen_compiler::compile_raw(src).expect("compiles");
    let mut vm = VM::new();
    vm.enable_jit(0);
    vm.load(module);
    let err = vm
        .execute("main", vec![])
        .expect_err("overflow must surface");
    assert!(err.is_arithmetic_overflow(), "{err}");
    let stats = vm.jit_stats();
    assert!(stats.jit_executions >= 10, "{stats:?}");
    assert!(stats.jit_fallbacks >= 1, "{stats:?}");
}
