//! Criterion benchmarks for the sort builtin with homogeneous specialization.
//!
//! Measures the performance improvement from specialized Int/Float/String sorting
//! compared to general Value sorting.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use lumen_compiler::compile_raw;
use lumen_rt::vm::VM;

/// Generate a Lumen source that sorts a list of integers
fn gen_sort_ints(size: usize) -> String {
    let nums: Vec<String> = (0..size).rev().map(|i| i.to_string()).collect();
    format!(
        r#"
cell main() -> list[Int]
  let xs = [{}]
  return sort(xs)
end
"#,
        nums.join(", ")
    )
}

/// Generate a Lumen source that sorts a list of floats
fn gen_sort_floats(size: usize) -> String {
    let nums: Vec<String> = (0..size).rev().map(|i| format!("{}.5", i)).collect();
    format!(
        r#"
cell main() -> list[Float]
  let xs = [{}]
  return sort(xs)
end
"#,
        nums.join(", ")
    )
}

/// Generate a Lumen source that sorts a list of strings
fn gen_sort_strings(size: usize) -> String {
    let strs: Vec<String> = (0..size)
        .map(|i| format!(r#""string_{}""#, (b'z' as u8 - (i % 26) as u8) as char))
        .collect();
    format!(
        r#"
cell main() -> list[String]
  let xs = [{}]
  return sort(xs)
end
"#,
        strs.join(", ")
    )
}

fn bench_sort_ints(c: &mut Criterion) {
    let sizes = [10, 100, 1000, 10000];
    let mut group = c.benchmark_group("sort_ints");

    for size in sizes {
        let source = gen_sort_ints(size);
        let module = compile_raw(&source).expect("compilation failed");

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| {
                let mut vm = VM::new();
                vm.load_module(module.clone());
                let _ = vm.run_cell_by_name(black_box("main"), &[]);
            });
        });
    }

    group.finish();
}

fn bench_sort_floats(c: &mut Criterion) {
    let sizes = [10, 100, 1000, 10000];
    let mut group = c.benchmark_group("sort_floats");

    for size in sizes {
        let source = gen_sort_floats(size);
        let module = compile_raw(&source).expect("compilation failed");

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| {
                let mut vm = VM::new();
                vm.load_module(module.clone());
                let _ = vm.run_cell_by_name(black_box("main"), &[]);
            });
        });
    }

    group.finish();
}

fn bench_sort_strings(c: &mut Criterion) {
    let sizes = [10, 100, 1000, 10000];
    let mut group = c.benchmark_group("sort_strings");

    for size in sizes {
        let source = gen_sort_strings(size);
        let module = compile_raw(&source).expect("compilation failed");

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| {
                let mut vm = VM::new();
                vm.load_module(module.clone());
                let _ = vm.run_cell_by_name(black_box("main"), &[]);
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_sort_ints,
    bench_sort_floats,
    bench_sort_strings
);
criterion_main!(benches);
