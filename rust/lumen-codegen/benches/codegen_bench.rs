//! Codegen benchmarks — measures compilation throughput.
//!
//! Uses simple `std::time::Instant` timing with multiple iterations to get
//! stable results. Run with:
//!
//! ```bash
//! cargo bench -p lumen-codegen
//! ```

use std::time::{Duration, Instant};

use lumen_codegen::bench_programs;
use lumen_codegen::context::CodegenContext;
use lumen_codegen::emit::emit_object;
use lumen_codegen::lower::lower_module;
use lumen_compiler::compiler::lir::LirModule;

/// Number of iterations for each benchmark.
const ITERATIONS: u32 = 20;

/// Compile an LIR module to object code, returning the elapsed time.
fn compile_lir(lir: &LirModule) -> (Vec<u8>, Duration) {
    let start = Instant::now();
    let mut ctx = CodegenContext::new().expect("host context");
    let ptr_ty = ctx.pointer_type();
    lower_module(&mut ctx.module, lir, ptr_ty).expect("lowering");
    let bytes = emit_object(ctx.module).expect("emission");
    (bytes, start.elapsed())
}

/// Run a benchmark: compile the given module `ITERATIONS` times and report
/// min / mean / max timings.
fn run_bench(name: &str, lir: &LirModule) {
    // Warm-up run (not counted).
    let _ = compile_lir(lir);

    let mut durations = Vec::with_capacity(ITERATIONS as usize);
    let mut total_bytes = 0usize;

    for _ in 0..ITERATIONS {
        let (bytes, dur) = compile_lir(lir);
        total_bytes = bytes.len();
        durations.push(dur);
    }

    durations.sort();
    let min = durations[0];
    let max = durations[durations.len() - 1];
    let mean: Duration = durations.iter().sum::<Duration>() / ITERATIONS;
    let median = durations[durations.len() / 2];

    println!("  {name}");
    println!("    iterations : {ITERATIONS}");
    println!("    object size: {total_bytes} bytes");
    println!("    min        : {:.3} ms", min.as_secs_f64() * 1000.0);
    println!("    median     : {:.3} ms", median.as_secs_f64() * 1000.0);
    println!("    mean       : {:.3} ms", mean.as_secs_f64() * 1000.0);
    println!("    max        : {:.3} ms", max.as_secs_f64() * 1000.0);
    println!();
}

fn main() {
    println!();
    println!("=== lumen-codegen benchmarks ({ITERATIONS} iterations each) ===");
    println!();

    run_bench(
        "fibonacci (1 cell, recursive)",
        &bench_programs::fibonacci_lir(),
    );
    run_bench(
        "arithmetic (1 cell, heavy ALU)",
        &bench_programs::arithmetic_lir(),
    );
    run_bench("simple loop (1 cell)", &bench_programs::simple_loop_lir());
    run_bench(
        "multi-cell (20 cells, arithmetic)",
        &bench_programs::multi_cell_lir(20),
    );
    run_bench(
        "tail-recursive countdown (1 cell, TCO)",
        &bench_programs::tail_recursive_countdown_lir(),
    );

    println!("=== done ===");
}
