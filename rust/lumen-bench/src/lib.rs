use lumen_core::lir::{Constant, Instruction, LirCell, LirModule, OpCode};
use lumen_core::values::Value;
use lumen_rt::jit_tier::JitTierStats;
use lumen_rt::vm::VM;
use std::io;
use std::path::PathBuf;

pub const VM_BENCHMARKS: &[(&str, &str, i64)] = &[
    ("fib", "b_int_fib.lm", 9_227_465),
    ("sum_loop", "b_int_sum_loop.lm", 50_000_005_000_000),
    ("string_concat", "b_string_concat.lm", 500_000),
];

const BENCH_ENTRY_CELL: &str = "__lumen_bench_entry";

pub struct VmExecution {
    pub value: Value,
    pub jit_stats: JitTierStats,
}

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

fn wrap_entrypoint(module: &LirModule, entry: &str) -> Result<LirModule, String> {
    let entry_cell = module
        .cells
        .iter()
        .find(|cell| cell.name == entry)
        .ok_or_else(|| format!("entry cell not found: {entry}"))?;
    if module.cells.iter().any(|cell| cell.name == BENCH_ENTRY_CELL) {
        return Err(format!("module already contains reserved cell: {BENCH_ENTRY_CELL}"));
    }

    let mut wrapped = module.clone();
    wrapped.cells.push(LirCell {
        name: BENCH_ENTRY_CELL.to_string(),
        params: Vec::new(),
        returns: entry_cell.returns.clone(),
        registers: 1,
        constants: vec![Constant::String(entry.to_string())],
        instructions: vec![
            Instruction::abx(OpCode::LoadK, 0, 0),
            Instruction::abc(OpCode::Call, 0, 0, 1),
            Instruction::abc(OpCode::Return, 0, 1, 0),
        ],
        effect_handler_metas: Vec::new(),
    });
    Ok(wrapped)
}

pub fn execute_main_with_stats(
    module: &LirModule,
    enable_jit: bool,
    hot_threshold: u64,
) -> Result<VmExecution, String> {
    let mut vm = VM::new();
    if enable_jit {
        vm.enable_jit(hot_threshold);
    }
    let (module_to_run, entry) = if enable_jit {
        (wrap_entrypoint(module, "main")?, BENCH_ENTRY_CELL)
    } else {
        (module.clone(), "main")
    };
    vm.load(module_to_run);
    let value = vm.execute(entry, vec![]).map_err(|err| err.to_string())?;
    Ok(VmExecution {
        value,
        jit_stats: vm.jit_stats(),
    })
}

pub fn execute_main(
    module: &LirModule,
    enable_jit: bool,
    hot_threshold: u64,
) -> Result<Value, String> {
    execute_main_with_stats(module, enable_jit, hot_threshold).map(|execution| execution.value)
}

#[cfg(test)]
mod tests {
    use super::{
        compile_vm_benchmark, execute_main, execute_main_with_stats, load_vm_benchmark_source,
        VM_BENCHMARKS,
    };
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

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn jit_wrapper_allows_single_cell_entry_benchmarks_to_go_native() {
        let source = load_vm_benchmark_source("b_int_sum_loop.lm").expect("load sum_loop");
        let module = compile_vm_benchmark(&source).expect("compile sum_loop");
        let execution = execute_main_with_stats(&module, true, 0).expect("execute with jit");

        match execution.value {
            Value::Int(actual) => assert_eq!(actual, 50_000_005_000_000),
            other => panic!("sum_loop returned non-int value: {other:?}"),
        }

        assert!(
            execution.jit_stats.jit_executions > 0,
            "expected wrapper entry to trigger native JIT execution for main"
        );
        assert!(
            execution.jit_stats.cells_compiled > 0,
            "expected wrapper entry to compile at least one cell"
        );
    }
}
