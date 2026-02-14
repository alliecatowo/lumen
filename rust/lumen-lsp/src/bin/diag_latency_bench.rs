use std::cmp::Ordering;
use std::time::Instant;

fn main() {
    let iterations = 50usize;
    let source = r#"
cell main() -> Int
  let x = 41
  return x + 1
end
"#;

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
        "[lumen-lsp][diag-bench] iterations={} compile_p50_ms={:.4} compile_p95_ms={:.4} min_ms={:.4} max_ms={:.4}",
        iterations, p50, p95, min, max
    );
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
