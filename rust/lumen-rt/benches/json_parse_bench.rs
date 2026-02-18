use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use lumen_compiler::compile_raw;
use lumen_rt::json_parser::parse_json_optimized;
use lumen_rt::vm::VM;

fn parse_json_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_parse");

    // Small JSON object (common case)
    let small_json = r#"{"name":"Alice","age":30,"city":"NYC"}"#;

    // Medium JSON with array of numbers
    let medium_json = r#"{"data":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20],"count":20}"#;

    // Large JSON with nested objects
    let large_json = r#"{
        "users": [
            {"id":1,"name":"Alice","email":"alice@example.com","active":true},
            {"id":2,"name":"Bob","email":"bob@example.com","active":false},
            {"id":3,"name":"Charlie","email":"charlie@example.com","active":true},
            {"id":4,"name":"Diana","email":"diana@example.com","active":true},
            {"id":5,"name":"Eve","email":"eve@example.com","active":false}
        ],
        "meta": {"total":5,"page":1,"per_page":10}
    }"#;

    for (name, json) in [
        ("small", small_json),
        ("medium", medium_json),
        ("large", large_json),
    ] {
        // Direct optimized parser (no VM overhead)
        group.bench_with_input(
            BenchmarkId::new("optimized_parser", name),
            &json,
            |b, json_str| {
                b.iter(|| {
                    let val = parse_json_optimized(black_box(json_str)).unwrap();
                    black_box(val)
                });
            },
        );

        // Full Lumen VM execution (compile + execute)
        group.bench_with_input(BenchmarkId::new("lumen_vm", name), &json, |b, json_str| {
            let code = format!(
                "cell main() -> Int\n  let data = parse_json(\"{}\")\n  return 1\nend\n",
                json_str.replace('\"', "\\\"").replace('\n', "\\n")
            );

            let module = compile_raw(&code).expect("compile failed");

            b.iter(|| {
                let mut vm = VM::new();
                vm.load(module.clone());
                let result = vm.execute("main", vec![]);
                black_box(result)
            });
        });

        // Baseline: raw serde_json parsing
        group.bench_with_input(BenchmarkId::new("serde_json", name), &json, |b, json| {
            b.iter(|| {
                let val: serde_json::Value = serde_json::from_str(black_box(json)).unwrap();
                black_box(val)
            });
        });
    }

    group.finish();
}

criterion_group!(benches, parse_json_benchmark);
criterion_main!(benches);
