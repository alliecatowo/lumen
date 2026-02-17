//! Criterion benchmarks for the Lumen compiler pipeline.
//!
//! Measures throughput of individual compiler stages (lex, parse, typecheck,
//! full compile) across corpus files of varying sizes.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::fs;
use std::path::PathBuf;

/// Locate the corpus directory. Criterion runs from the crate root,
/// so we check a few relative paths.
fn corpus_dir() -> PathBuf {
    let candidates = [
        PathBuf::from("../../bench/corpus"),
        PathBuf::from("bench/corpus"),
        PathBuf::from("../bench/corpus"),
    ];
    for c in &candidates {
        if c.is_dir() {
            return c.clone();
        }
    }
    // Fallback — will fail gracefully in the benchmark body
    PathBuf::from("../../bench/corpus")
}

/// Load a corpus file by name. Returns None if not found.
fn load_corpus(name: &str) -> Option<String> {
    let path = corpus_dir().join(name);
    fs::read_to_string(&path).ok()
}

/// Read peak RSS from /proc/self/status (Linux only).
#[cfg(target_os = "linux")]
fn peak_rss_kb() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if line.starts_with("VmHWM:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                return parts[1].parse::<u64>().ok();
            }
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn peak_rss_kb() -> Option<u64> {
    None
}

fn bench_lex(c: &mut Criterion) {
    let corpus_files = [
        ("tiny", "tiny.lm"),
        ("small", "small.lm"),
        ("medium", "medium.lm"),
        ("large", "large.lm"),
        ("huge", "huge.lm"),
    ];

    let mut group = c.benchmark_group("lex");

    for (label, file) in &corpus_files {
        let source = match load_corpus(file) {
            Some(s) => s,
            None => {
                eprintln!("Skipping lex/{}: corpus file not found", label);
                continue;
            }
        };
        let lines = source.lines().count();
        group.throughput(Throughput::Elements(lines as u64));
        group.bench_with_input(
            BenchmarkId::new("tokens_per_sec", label),
            &source,
            |b, src| {
                b.iter(|| {
                    let mut lexer =
                        lumen_compiler::compiler::lexer::Lexer::new(black_box(src), 1, 0);
                    let _ = lexer.tokenize();
                });
            },
        );
    }

    if let Some(rss) = peak_rss_kb() {
        eprintln!("[lex] Peak RSS after benchmarks: {} kB", rss);
    }

    group.finish();
}

fn bench_parse(c: &mut Criterion) {
    let corpus_files = [
        ("tiny", "tiny.lm"),
        ("small", "small.lm"),
        ("medium", "medium.lm"),
        ("large", "large.lm"),
        ("huge", "huge.lm"),
    ];

    let mut group = c.benchmark_group("parse");

    for (label, file) in &corpus_files {
        let source = match load_corpus(file) {
            Some(s) => s,
            None => {
                eprintln!("Skipping parse/{}: corpus file not found", label);
                continue;
            }
        };
        let lines = source.lines().count();
        group.throughput(Throughput::Elements(lines as u64));
        group.bench_with_input(
            BenchmarkId::new("nodes_per_sec", label),
            &source,
            |b, src| {
                b.iter(|| {
                    let mut lexer =
                        lumen_compiler::compiler::lexer::Lexer::new(black_box(src), 1, 0);
                    if let Ok(tokens) = lexer.tokenize() {
                        let mut parser = lumen_compiler::compiler::parser::Parser::new(tokens);
                        let _ = parser.parse_program_with_recovery(vec![]);
                    }
                });
            },
        );
    }

    if let Some(rss) = peak_rss_kb() {
        eprintln!("[parse] Peak RSS after benchmarks: {} kB", rss);
    }

    group.finish();
}

fn bench_typecheck(c: &mut Criterion) {
    let corpus_files = [
        ("tiny", "tiny.lm"),
        ("small", "small.lm"),
        ("medium", "medium.lm"),
        ("large", "large.lm"),
        ("huge", "huge.lm"),
    ];

    let mut group = c.benchmark_group("typecheck");

    for (label, file) in &corpus_files {
        let source = match load_corpus(file) {
            Some(s) => s,
            None => {
                eprintln!("Skipping typecheck/{}: corpus file not found", label);
                continue;
            }
        };
        let lines = source.lines().count();
        group.throughput(Throughput::Elements(lines as u64));
        group.bench_with_input(
            BenchmarkId::new("lines_per_sec", label),
            &source,
            |b, src| {
                b.iter(|| {
                    // Full compile includes typecheck — this is the best proxy
                    // since typecheck can't run in isolation (needs resolved symbols).
                    let _ = lumen_compiler::compile_raw(black_box(src));
                });
            },
        );
    }

    if let Some(rss) = peak_rss_kb() {
        eprintln!("[typecheck] Peak RSS after benchmarks: {} kB", rss);
    }

    group.finish();
}

fn bench_full_compile(c: &mut Criterion) {
    let corpus_files = [
        ("tiny", "tiny.lm"),
        ("small", "small.lm"),
        ("medium", "medium.lm"),
        ("large", "large.lm"),
        ("huge", "huge.lm"),
    ];

    let mut group = c.benchmark_group("full_compile");

    for (label, file) in &corpus_files {
        let source = match load_corpus(file) {
            Some(s) => s,
            None => {
                eprintln!("Skipping full_compile/{}: corpus file not found", label);
                continue;
            }
        };
        let lines = source.lines().count();
        group.throughput(Throughput::Elements(lines as u64));
        group.bench_with_input(
            BenchmarkId::new("lines_per_sec", label),
            &source,
            |b, src| {
                b.iter(|| {
                    let _ = lumen_compiler::compile_raw(black_box(src));
                });
            },
        );
    }

    if let Some(rss) = peak_rss_kb() {
        eprintln!("[full_compile] Peak RSS after benchmarks: {} kB", rss);
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_lex,
    bench_parse,
    bench_typecheck,
    bench_full_compile
);
criterion_main!(benches);
