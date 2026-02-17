//! Benchmark suite for the Lumen compiler pipeline.
//!
//! Measures compile, lex+parse, and type-check performance on both simple
//! and complex programs. Also includes a regression gate test that compares
//! against stored baselines.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

// ---------------------------------------------------------------------------
// Benchmark programs (inline source)
// ---------------------------------------------------------------------------

/// Simple program: a single cell with basic arithmetic.
const SIMPLE_PROGRAM: &str = r#"
cell main() -> Int
    let x = 10
    let y = 20
    x + y
end
"#;

/// Medium program: multiple cells, a record, and control flow.
const MEDIUM_PROGRAM: &str = r#"
record Point
    x: Int
    y: Int
end

cell distance(a: Point, b: Point) -> Float
    let dx = a.x - b.x
    let dy = a.y - b.y
    to_float(dx * dx + dy * dy)
end

cell classify(score: Int) -> String
    when
        score >= 90 -> "A"
        score >= 80 -> "B"
        score >= 70 -> "C"
        _ -> "F"
    end
end

cell fibonacci(n: Int) -> Int
    if n <= 1
        n
    else
        fibonacci(n - 1) + fibonacci(n - 2)
    end
end

cell main() -> String
    let p1 = Point(x: 0, y: 0)
    let p2 = Point(x: 3, y: 4)
    let d = distance(p1, p2)
    let grade = classify(85)
    let fib = fibonacci(10)
    "done"
end
"#;

/// Complex program: enums, match, lists, multiple records.
const COMPLEX_PROGRAM: &str = r#"
record Config
    name: String
    max_retries: Int
    timeout_ms: Int
end

enum Status
    Pending
    Running(progress: Float)
    Complete(result: String)
    Failed(error: String)
end

record Task
    id: Int
    name: String
    status: Status
end

cell status_label(s: Status) -> String
    match s
        Status.Pending -> "PENDING"
        Status.Running(progress:) -> "RUNNING ({progress}%)"
        Status.Complete(result:) -> "DONE: {result}"
        Status.Failed(error:) -> "FAIL: {error}"
    end
end

cell count_complete(tasks: list[Task]) -> Int
    let count = 0
    for task in tasks
        match task.status
            Status.Complete(result:) -> count = count + 1
            _ -> count = count
        end
    end
    count
end

cell make_tasks(n: Int) -> list[Task]
    let tasks: list[Task] = []
    for i in 0..n
        let status = if i % 3 == 0
            Status.Complete(result: "ok")
        else if i % 3 == 1
            Status.Running(progress: 50.0)
        else
            Status.Pending
        end
        tasks = append(tasks, Task(id: i, name: "task-{i}", status: status))
    end
    tasks
end

cell process_batch(config: Config) -> String
    let tasks = make_tasks(10)
    let done = count_complete(tasks)
    "{config.name}: {done} complete"
end

cell main() -> String
    let cfg = Config(name: "batch-1", max_retries: 3, timeout_ms: 5000)
    process_batch(cfg)
end
"#;

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_compile_simple(c: &mut Criterion) {
    c.bench_function("compile_simple", |b| {
        b.iter(|| lumen_compiler::compile_raw(black_box(SIMPLE_PROGRAM)))
    });
}

fn bench_compile_medium(c: &mut Criterion) {
    c.bench_function("compile_medium", |b| {
        b.iter(|| lumen_compiler::compile_raw(black_box(MEDIUM_PROGRAM)))
    });
}

fn bench_compile_complex(c: &mut Criterion) {
    c.bench_function("compile_complex", |b| {
        b.iter(|| lumen_compiler::compile_raw(black_box(COMPLEX_PROGRAM)))
    });
}

fn bench_lex_parse_simple(c: &mut Criterion) {
    c.bench_function("lex_parse_simple", |b| {
        b.iter(|| {
            let mut lexer =
                lumen_compiler::compiler::lexer::Lexer::new(black_box(SIMPLE_PROGRAM), 1, 0);
            let tokens = lexer.tokenize().unwrap();
            let mut parser = lumen_compiler::compiler::parser::Parser::new(tokens);
            let _ = parser.parse_program_with_recovery(vec![]);
        })
    });
}

fn bench_lex_parse_complex(c: &mut Criterion) {
    c.bench_function("lex_parse_complex", |b| {
        b.iter(|| {
            let mut lexer =
                lumen_compiler::compiler::lexer::Lexer::new(black_box(COMPLEX_PROGRAM), 1, 0);
            let tokens = lexer.tokenize().unwrap();
            let mut parser = lumen_compiler::compiler::parser::Parser::new(tokens);
            let _ = parser.parse_program_with_recovery(vec![]);
        })
    });
}

fn bench_typecheck_complex(c: &mut Criterion) {
    // Pre-parse and resolve once, then benchmark only typechecking
    let mut lexer = lumen_compiler::compiler::lexer::Lexer::new(COMPLEX_PROGRAM, 1, 0);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = lumen_compiler::compiler::parser::Parser::new(tokens);
    let (program, _errors) = parser.parse_program_with_recovery(vec![]);
    let (symbols, _) = lumen_compiler::compiler::resolve::resolve_partial(&program);

    c.bench_function("typecheck_complex", |b| {
        b.iter(|| {
            let _ = lumen_compiler::compiler::typecheck::typecheck(
                black_box(&program),
                black_box(&symbols),
            );
        })
    });
}

criterion_group!(
    benches,
    bench_compile_simple,
    bench_compile_medium,
    bench_compile_complex,
    bench_lex_parse_simple,
    bench_lex_parse_complex,
    bench_typecheck_complex,
);
criterion_main!(benches);

// ---------------------------------------------------------------------------
// Regression gate test
// ---------------------------------------------------------------------------
// The regression gate is a regular #[test] that asserts compilation completes
// within a generous wall-clock budget. It lives alongside the benchmarks so
// `cargo test` can catch catastrophic performance regressions.

#[cfg(test)]
mod regression {
    use super::*;
    use std::time::Instant;

    /// Maximum allowed wall-clock time (ms) for compiling the simple program.
    /// This is intentionally generous (100 ms) to avoid flaky CI failures
    /// while still catching 10x regressions.
    const SIMPLE_BUDGET_MS: u128 = 100;

    /// Maximum allowed wall-clock time (ms) for compiling the complex program.
    const COMPLEX_BUDGET_MS: u128 = 500;

    /// Number of iterations to average over.
    const ITERATIONS: u32 = 5;

    fn avg_compile_ms(source: &str, iters: u32) -> u128 {
        // Warm up
        let _ = lumen_compiler::compile_raw(source);

        let start = Instant::now();
        for _ in 0..iters {
            let _ = lumen_compiler::compile_raw(source);
        }
        start.elapsed().as_millis() / u128::from(iters)
    }

    #[test]
    fn bench_regression_simple() {
        let avg = avg_compile_ms(SIMPLE_PROGRAM, ITERATIONS);
        assert!(
            avg < SIMPLE_BUDGET_MS,
            "Simple program compile regression: {}ms avg exceeds {}ms budget",
            avg,
            SIMPLE_BUDGET_MS
        );
    }

    #[test]
    fn bench_regression_complex() {
        let avg = avg_compile_ms(COMPLEX_PROGRAM, ITERATIONS);
        assert!(
            avg < COMPLEX_BUDGET_MS,
            "Complex program compile regression: {}ms avg exceeds {}ms budget",
            avg,
            COMPLEX_BUDGET_MS
        );
    }
}
