use std::cmp::Ordering;
use std::env;
use std::time::Instant;

fn main() {
    let config = Config::from_env_and_args();
    let iterations = config.iterations;
    let source = r#"
cell main() -> Int
  let x = 41
  return x + 1
end
"#;

    for _ in 0..config.warmup_iterations {
        let _ = lumen_compiler::compile_raw(source);
    }

    let mut compile_samples_ms = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let started = Instant::now();
        let _ = lumen_compiler::compile_raw(source);
        compile_samples_ms.push(started.elapsed().as_secs_f64() * 1_000.0);
    }

    let p50 = percentile(&compile_samples_ms, 0.50).unwrap_or(0.0);
    let p95 = percentile(&compile_samples_ms, 0.95).unwrap_or(0.0);
    let min = compile_samples_ms
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    let max = compile_samples_ms
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);

    println!(
        "[lumen-lsp][diag-bench] warmup_iterations={} iterations={} compile_p50_ms={:.4} compile_p95_ms={:.4} min_ms={:.4} max_ms={:.4}",
        config.warmup_iterations, iterations, p50, p95, min, max
    );

    if let Some(threshold_ms) = config.threshold_ms {
        let observed_ms = config.metric.read(p50, p95, max);
        if observed_ms > threshold_ms {
            println!(
                "[lumen-lsp][diag-bench] threshold_exceeded metric={} observed_ms={:.4} threshold_ms={:.4}",
                config.metric.as_str(),
                observed_ms,
                threshold_ms
            );
            std::process::exit(1);
        }
        println!(
            "[lumen-lsp][diag-bench] threshold_ok metric={} observed_ms={:.4} threshold_ms={:.4}",
            config.metric.as_str(),
            observed_ms,
            threshold_ms
        );
    }
}

fn percentile(samples: &[f64], percentile: f64) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }

    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let rank = ((sorted.len().saturating_sub(1)) as f64 * percentile).round() as usize;
    sorted.get(rank).copied()
}

#[derive(Clone, Copy)]
enum Metric {
    P50,
    P95,
    Max,
}

impl Metric {
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "p50" => Some(Self::P50),
            "p95" => Some(Self::P95),
            "max" => Some(Self::Max),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::P50 => "p50",
            Self::P95 => "p95",
            Self::Max => "max",
        }
    }

    fn read(self, p50: f64, p95: f64, max: f64) -> f64 {
        match self {
            Self::P50 => p50,
            Self::P95 => p95,
            Self::Max => max,
        }
    }
}

struct Config {
    iterations: usize,
    warmup_iterations: usize,
    threshold_ms: Option<f64>,
    metric: Metric,
}

impl Config {
    fn from_env_and_args() -> Self {
        let mut iterations = read_env_usize("LUMEN_DIAG_BENCH_ITERATIONS").unwrap_or(50);
        let mut warmup_iterations =
            read_env_usize("LUMEN_DIAG_BENCH_WARMUP_ITERATIONS").unwrap_or(5);
        let mut threshold_ms = read_env_f64("LUMEN_DIAG_BENCH_THRESHOLD_MS");
        let mut metric = env::var("LUMEN_DIAG_BENCH_THRESHOLD_METRIC")
            .ok()
            .and_then(|v| Metric::parse(&v))
            .unwrap_or(Metric::P95);

        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--iterations" => {
                    if let Some(value) = args.next().and_then(|v| v.parse::<usize>().ok()) {
                        iterations = value;
                    }
                }
                "--warmup-iterations" => {
                    if let Some(value) = args.next().and_then(|v| v.parse::<usize>().ok()) {
                        warmup_iterations = value;
                    }
                }
                "--threshold-ms" => {
                    threshold_ms = args.next().and_then(|v| v.parse::<f64>().ok());
                }
                "--threshold-metric" => {
                    if let Some(value) = args.next().and_then(|v| Metric::parse(&v)) {
                        metric = value;
                    }
                }
                _ => {}
            }
        }

        if iterations == 0 {
            iterations = 1;
        }

        Self {
            iterations,
            warmup_iterations,
            threshold_ms,
            metric,
        }
    }
}

fn read_env_usize(name: &str) -> Option<usize> {
    env::var(name).ok().and_then(|v| v.parse::<usize>().ok())
}

fn read_env_f64(name: &str) -> Option<f64> {
    env::var(name).ok().and_then(|v| v.parse::<f64>().ok())
}
