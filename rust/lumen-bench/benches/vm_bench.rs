use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput,
};
use lumen_bench::{compile_vm_benchmark, execute_main, load_vm_benchmark_source, VM_BENCHMARKS};

fn bench_vm_execution(c: &mut Criterion) {
    let mut group = c.benchmark_group("vm_execute");

    for (label, file_name, _) in VM_BENCHMARKS {
        let source = load_vm_benchmark_source(file_name).expect("load benchmark source");
        let module = compile_vm_benchmark(&source).expect("compile benchmark source");
        group.throughput(Throughput::Bytes(source.len() as u64));

        group.bench_with_input(
            BenchmarkId::new("interpreter", label),
            &module,
            |b, module| {
                b.iter_batched(
                    || module.clone(),
                    |module| {
                        black_box(
                            execute_main(&module, false, 0).expect("run interpreter benchmark"),
                        )
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        #[cfg(target_arch = "x86_64")]
        group.bench_with_input(BenchmarkId::new("jit", label), &module, |b, module| {
            b.iter_batched(
                || module.clone(),
                |module| black_box(execute_main(&module, true, 0).expect("run jit benchmark")),
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(benches, bench_vm_execution);
criterion_main!(benches);
