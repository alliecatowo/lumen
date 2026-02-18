//! Lumen Benchmark Runner
//!
//! Standalone binary for running compiler and VM benchmarks with
//! JSON/CSV output and peak memory tracking.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Result of a single benchmark run.
#[derive(Debug, Clone, Serialize)]
pub struct BenchResult {
    pub name: String,
    pub corpus_file: String,
    pub source_lines: usize,
    pub source_bytes: usize,
    pub duration_ms: f64,
    pub throughput_lines_per_sec: f64,
    pub throughput_bytes_per_sec: f64,
    pub peak_rss_kb: Option<u64>,
    pub iterations: u32,
}

/// Read peak RSS from /proc/self/status on Linux.
/// Returns None on non-Linux or if the file cannot be parsed.
pub fn peak_rss_kb() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = fs::read_to_string("/proc/self/status").ok()?;
        for line in status.lines() {
            if line.starts_with("VmPeak:") || line.starts_with("VmHWM:") {
                // Format: "VmPeak:   123456 kB"
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    return parts[1].parse::<u64>().ok();
                }
            }
        }
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Read current RSS from /proc/self/status on Linux.
pub fn current_rss_kb() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = fs::read_to_string("/proc/self/status").ok()?;
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    return parts[1].parse::<u64>().ok();
                }
            }
        }
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Count lines in source code.
fn count_lines(source: &str) -> usize {
    source.lines().count()
}

/// Locate the corpus directory relative to the workspace root.
fn find_corpus_dir() -> PathBuf {
    // Try relative paths from likely execution locations
    let candidates = [
        PathBuf::from("bench/corpus"),
        PathBuf::from("../../bench/corpus"),
        PathBuf::from("../bench/corpus"),
    ];
    for c in &candidates {
        if c.is_dir() {
            return c.clone();
        }
    }
    // Fallback
    PathBuf::from("bench/corpus")
}

/// Load a corpus file, returning (filename, source).
fn load_corpus(name: &str) -> Option<(String, String)> {
    let corpus_dir = find_corpus_dir();
    let path = corpus_dir.join(name);
    let source = fs::read_to_string(&path).ok()?;
    Some((name.to_string(), source))
}

/// Run the lexer on source, measuring time.
fn bench_lex(source: &str, iterations: u32) -> Duration {
    let mut total = Duration::ZERO;
    for _ in 0..iterations {
        let start = Instant::now();
        let mut lexer = lumen_compiler::compiler::lexer::Lexer::new(source, 1, 0);
        let _ = lexer.tokenize();
        total += start.elapsed();
    }
    total
}

/// Run the parser on source, measuring time.
fn bench_parse(source: &str, iterations: u32) -> Duration {
    let mut total = Duration::ZERO;
    for _ in 0..iterations {
        let start = Instant::now();
        let mut lexer = lumen_compiler::compiler::lexer::Lexer::new(source, 1, 0);
        if let Ok(tokens) = lexer.tokenize() {
            let mut parser = lumen_compiler::compiler::parser::Parser::new(tokens);
            let _ = parser.parse_program_with_recovery(vec![]);
        }
        total += start.elapsed();
    }
    total
}

/// Run full compile on source, measuring time.
fn bench_compile(source: &str, iterations: u32) -> Duration {
    let mut total = Duration::ZERO;
    for _ in 0..iterations {
        let start = Instant::now();
        let _ = lumen_compiler::compile_raw(source);
        total += start.elapsed();
    }
    total
}

/// Run a single benchmark and produce a BenchResult.
fn run_bench(
    name: &str,
    corpus_file: &str,
    source: &str,
    bench_fn: fn(&str, u32) -> Duration,
    iterations: u32,
) -> BenchResult {
    let lines = count_lines(source);
    let bytes = source.len();

    // Warm up
    bench_fn(source, 3);

    // Measure
    let rss_before = current_rss_kb();
    let total = bench_fn(source, iterations);
    let rss_after = peak_rss_kb();

    let avg_duration = total.as_secs_f64() / iterations as f64;
    let throughput_lines = lines as f64 / avg_duration;
    let throughput_bytes = bytes as f64 / avg_duration;

    BenchResult {
        name: name.to_string(),
        corpus_file: corpus_file.to_string(),
        source_lines: lines,
        source_bytes: bytes,
        duration_ms: avg_duration * 1000.0,
        throughput_lines_per_sec: throughput_lines,
        throughput_bytes_per_sec: throughput_bytes,
        peak_rss_kb: rss_after,
        iterations,
    }
}

fn print_csv_header() {
    println!("name,corpus_file,source_lines,source_bytes,duration_ms,lines_per_sec,bytes_per_sec,peak_rss_kb,iterations");
}

fn print_csv_row(r: &BenchResult) {
    println!(
        "{},{},{},{},{:.3},{:.0},{:.0},{},{}",
        r.name,
        r.corpus_file,
        r.source_lines,
        r.source_bytes,
        r.duration_ms,
        r.throughput_lines_per_sec,
        r.throughput_bytes_per_sec,
        r.peak_rss_kb.map_or("N/A".to_string(), |v| v.to_string()),
        r.iterations,
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let output_format = args.get(1).map(|s| s.as_str()).unwrap_or("text");
    let iterations: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);

    let corpus_files = ["tiny.lm", "small.lm", "medium.lm", "large.lm", "huge.lm"];

    let mut results: Vec<BenchResult> = Vec::new();

    for file in &corpus_files {
        let (filename, source) = match load_corpus(file) {
            Some(v) => v,
            None => {
                eprintln!("Warning: corpus file '{}' not found, skipping", file);
                continue;
            }
        };

        // Lex benchmark
        results.push(run_bench("lex", &filename, &source, bench_lex, iterations));

        // Parse benchmark
        results.push(run_bench(
            "parse",
            &filename,
            &source,
            bench_parse,
            iterations,
        ));

        // Full compile benchmark
        results.push(run_bench(
            "full_compile",
            &filename,
            &source,
            bench_compile,
            iterations,
        ));
    }

    match output_format {
        "csv" => {
            print_csv_header();
            for r in &results {
                print_csv_row(r);
            }
        }
        "json" => {
            println!("{}", serde_json::to_string_pretty(&results).unwrap());
        }
        _ => {
            // Human-readable text output
            println!("Lumen Compiler Benchmarks");
            println!("========================");
            println!();
            for r in &results {
                println!(
                    "[{}/{}] {:.3}ms avg ({} iters) | {:.0} lines/s | {:.0} bytes/s | RSS: {}",
                    r.name,
                    r.corpus_file,
                    r.duration_ms,
                    r.iterations,
                    r.throughput_lines_per_sec,
                    r.throughput_bytes_per_sec,
                    r.peak_rss_kb
                        .map_or("N/A".to_string(), |v| format!("{}kB", v)),
                );
            }
        }
    }
}
