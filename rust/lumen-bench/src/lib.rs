use lumen_core::lir::LirModule;
use lumen_core::values::Value;
use lumen_rt::vm::VM;
use std::io;
use std::path::PathBuf;

pub const VM_BENCHMARKS: &[(&str, &str, i64)] = &[
    ("fib", "b_int_fib.lm", 9_227_465),
    ("sum_loop", "b_int_sum_loop.lm", 50_000_005_000_000),
    ("string_concat", "b_string_concat.lm", 500_000),
];

fn bench_dir() -> PathBuf {
    let candidates = [
        PathBuf::from("../../bench"),
        PathBuf::from("bench"),
        PathBuf::from("../bench"),
    ];
    for candidate in candidates {
        if candidate.is_dir() {
            return candidate;
        }
    }
    PathBuf::from("../../bench")
}

pub fn load_vm_benchmark_source(file_name: &str) -> io::Result<String> {
    std::fs::read_to_string(bench_dir().join(file_name))
}

pub fn compile_vm_benchmark(source: &str) -> Result<LirModule, String> {
    lumen_compiler::compile_raw(source).map_err(|err| err.to_string())
}

pub fn execute_main(
    module: &LirModule,
    enable_jit: bool,
    hot_threshold: u64,
) -> Result<Value, String> {
    let mut vm = VM::new();
    if enable_jit {
        vm.enable_jit(hot_threshold);
    }
    vm.load(module.clone());
    vm.execute("main", vec![]).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::{compile_vm_benchmark, execute_main, load_vm_benchmark_source, VM_BENCHMARKS};
    use lumen_core::values::Value;

    #[test]
    fn interpreter_vm_benchmarks_match_expected_results() {
        for (_, file_name, expected) in VM_BENCHMARKS {
            let source = load_vm_benchmark_source(file_name).expect("load benchmark source");
            let module = compile_vm_benchmark(&source).expect("compile benchmark source");
            let value = execute_main(&module, false, 0).expect("execute benchmark");
            match value {
                Value::Int(actual) => {
                    assert_eq!(actual, *expected, "{file_name} returned {actual}")
                }
                other => panic!("{file_name} returned non-int value: {other:?}"),
            }
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn jit_vm_benchmarks_match_expected_results() {
        for (_, file_name, expected) in VM_BENCHMARKS {
            let source = load_vm_benchmark_source(file_name).expect("load benchmark source");
            let module = compile_vm_benchmark(&source).expect("compile benchmark source");
            let value = execute_main(&module, true, 0).expect("execute benchmark with jit");
            match value {
                Value::Int(actual) => {
                    assert_eq!(actual, *expected, "{file_name} returned {actual}")
                }
                other => panic!("{file_name} returned non-int value: {other:?}"),
            }
        }
    }
}
