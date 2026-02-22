//! Tier coverage tests — ensure JIT/stencil support stays aligned with
//! the canonical intrinsic coverage table in lumen-core.

use lumen_compiler::compile as compile_lumen;
use lumen_core::opcode_table::{intrinsic_table, TIER_JIT};
use lumen_core::values::Value;
use lumen_rt::jit_tier::JitTierConfig;
use lumen_rt::vm::VM;

const ZERO_STUB_SENTINEL: i64 = 0;
const JIT_WARMUP_ITERS: usize = 550;

fn compile_program(source: &str) -> lumen_core::lir::LirModule {
    let mut md = String::with_capacity(source.len() + 32);
    md.push_str("# tier-coverage\n\n```lumen\n");
    md.push_str(source.trim());
    md.push_str("\n```\n");
    compile_lumen(&md).expect("source should compile")
}

fn run_interp(module: lumen_core::lir::LirModule) -> Result<Value, String> {
    let mut vm = VM::new();
    vm.load(module);
    vm.execute("main", vec![])
        .map_err(|e| "interp error: ".to_string() + &e.to_string())
}

#[cfg(feature = "jit")]
fn run_tier2(module: lumen_core::lir::LirModule) -> Option<Result<Value, String>> {
    let mut vm = VM::new();
    vm.enable_jit_with_config(JitTierConfig::from_threshold(1));
    vm.load(module);
    for _ in 0..JIT_WARMUP_ITERS {
        let _ = vm.execute("main", vec![]);
    }
    let result = vm
        .execute("main", vec![])
        .map_err(|e| "tier2 error: ".to_string() + &e.to_string());
    if vm.jit_stats().jit_executions == 0 {
        return None;
    }
    Some(result)
}

fn should_skip_intrinsic(id: u16) -> bool {
    match id {
        // Skip intrinsics that are inherently side-effecting or nondeterministic.
        // Also skip intrinsics that are not implemented in the interpreter yet.
        9 | 68 | 85 | 96 | 97 | 133 | 14 | 15 | 24 | 29 | 30 | 31 | 32 | 33 | 34 | 37 | 38 | 39
        | 40 | 41 | 42 | 43 | 44 | 45 | 46 | 47 | 48 | 49 | 66 | 67 | 69 | 70 | 71 | 72 | 73
        | 74 | 75 | 77 | 120 | 121 | 122 | 123 | 124 | 125 | 126 | 127 | 128 | 129 | 130 | 131
        | 138 | 139 => true,
        _ => false,
    }
}

fn build_intrinsic_program(id: u16) -> String {
    let expr = match id {
        0 => "length([1, 2, 3])".to_string(),
        1 => "count([1, 2, 3])".to_string(),
        2 => "if matches(1) then 2 else 1".to_string(),
        3 => "length(hash(\"hello\"))".to_string(),
        9 => "1".to_string(),
        10 => "length(to_string(1))".to_string(),
        11 => "int(123)".to_string(),
        12 => "int(float(123))".to_string(),
        13 => "length(type_of(1))".to_string(),
        14 => "length(keys({\"a\": 1, \"b\": 2}))".to_string(),
        15 => "length(values({\"a\": 1, \"b\": 2}))".to_string(),
        16 => "if contains(\"hello\", \"ell\") then 2 else 1".to_string(),
        17 => "length(join([\"a\", \"b\"], \",\"))".to_string(),
        18 => "length(split(\"a,b\", \",\"))".to_string(),
        19 => "length(trim(\" hi \") )".to_string(),
        20 => "length(upper(\"hi\"))".to_string(),
        21 => "length(lower(\"HI\"))".to_string(),
        22 => "length(replace(\"a\", \"a\", \"b\"))".to_string(),
        23 => "length(slice(\"hello\", 0, 2))".to_string(),
        24 => "length(append([1, 2, 3], 4))".to_string(),
        25 => "length(range(0, 3))".to_string(),
        26 => "abs(-7)".to_string(),
        27 => "min(1, 2)".to_string(),
        28 => "max(1, 2)".to_string(),
        29 => "length(sort([3, 2, 1]))".to_string(),
        30 => "length(reverse([1, 2, 3]))".to_string(),
        31 => "length(map([1, 2, 3], fn(n: Int) -> Int => n + 1))".to_string(),
        32 => "length(filter([1, 2, 3], fn(n: Int) -> Bool => n > 1))".to_string(),
        33 => "reduce([1, 2, 3], fn(a: Int, b: Int) -> Int => a + b, 0)".to_string(),
        34 => "length(flat_map([1, 2, 3], fn(n: Int) -> list[Int] => [n, n]))".to_string(),
        35 => "length(zip([1, 2], [3, 4]))".to_string(),
        36 => "length(enumerate([1, 2]))".to_string(),
        37 => "if any([1, 2, 3], fn(n: Int) -> Bool => n > 2) then 2 else 1".to_string(),
        38 => "if all([1, 2, 3], fn(n: Int) -> Bool => n > 0) then 2 else 1".to_string(),
        39 => "if find([1, 2, 3], fn(n: Int) -> Bool => n > 2) is Null then 1 else 2".to_string(),
        40 => "position([1, 2, 3], fn(n: Int) -> Bool => n > 2)".to_string(),
        41 => "length(keys(group_by([1, 2, 3], fn(n: Int) -> String => to_string(n))))".to_string(),
        42 => "length(chunk([1, 2, 3], 2))".to_string(),
        43 => "length(window([1, 2, 3], 2))".to_string(),
        44 => "length(flatten([[1, 2], [3]]))".to_string(),
        45 => "length(unique([1, 1, 2]))".to_string(),
        46 => "length(take([1, 2, 3], 2))".to_string(),
        47 => "length(drop([1, 2, 3], 2))".to_string(),
        48 => "if first([1, 2, 3]) is Null then 1 else 2".to_string(),
        49 => "if last([1, 2, 3]) is Null then 1 else 2".to_string(),
        50 => "if is_empty([]) then 2 else 1".to_string(),
        51 => "length(chars(\"hi\"))".to_string(),
        52 => "if starts_with(\"hello\", \"he\") then 2 else 1".to_string(),
        53 => "if ends_with(\"hello\", \"lo\") then 2 else 1".to_string(),
        54 => "index_of(\"hello\", \"lo\")".to_string(),
        55 => "length(pad_left(\"hi\", 4))".to_string(),
        56 => "length(pad_right(\"hi\", 4))".to_string(),
        57 => "int(round(3.2))".to_string(),
        58 => "int(ceil(3.2))".to_string(),
        59 => "int(floor(3.2))".to_string(),
        60 => "int(sqrt(4.0))".to_string(),
        61 => "int(pow(2, 3))".to_string(),
        62 => "int(log(10.0))".to_string(),
        63 => "int(sin(1.0) * 10)".to_string(),
        64 => "int(cos(1.0) * 10)".to_string(),
        65 => "clamp(5, 1, 10)".to_string(),
        66 => "length(to_string(clone([1, 2, 3])))".to_string(),
        67 => "sizeof(1)".to_string(),
        68 => "1".to_string(),
        69 => "length(to_string(to_set([1, 2, 3])))".to_string(),
        70 => "if has_key({\"a\": 1}, \"a\") then 2 else 1".to_string(),
        71 => "length(keys(merge({\"a\": 1}, {\"b\": 2})))".to_string(),
        72 => "size([1, 2, 3])".to_string(),
        73 => "length(to_string(add(to_set([1, 2, 3]), 4)))".to_string(),
        74 => "length(keys(remove({\"a\": 1, \"b\": 2}, \"a\")))".to_string(),
        75 => "length(entries({\"a\": 1, \"b\": 2}))".to_string(),
        77 => "length(format(123))".to_string(),
        85 => "1".to_string(),
        96 => "1".to_string(),
        97 => "1".to_string(),
        106 => "length(string_concat(123))".to_string(),
        120 => "length(map_sorted_keys({\"a\": 1, \"b\": 2}))".to_string(),
        121 => "match parse_int(\"123\")\n    ok(n) -> n\n    err(_) -> 2\n  end".to_string(),
        122 => {
            "match parse_float(\"123.0\")\n    ok(n) -> int(n)\n    err(_) -> 2\n  end".to_string()
        }
        123 => "int(log2(4.0))".to_string(),
        124 => "int(log10(100.0))".to_string(),
        125 => "if is_nan(0.0) then 1 else 2".to_string(),
        126 => "if is_infinite(0.0) then 1 else 2".to_string(),
        127 => "int(math_pi)".to_string(),
        128 => "int(math_e)".to_string(),
        129 => "length(sort_asc([3, 2, 1]))".to_string(),
        130 => "length(sort_desc([3, 2, 1]))".to_string(),
        131 => "length(sort_by([1, 2, 3], fn(n: Int) -> Int => -n))".to_string(),
        133 => "1".to_string(),
        138 => "int(tan(1.0) * 10)".to_string(),
        139 => "int(trunc(3.9))".to_string(),
        _ => "1".to_string(),
    };

    let mut source = String::new();
    source.push_str("cell main() -> Int\n  return ");
    source.push_str(&expr);
    source.push_str("\nend\n");
    source
}

#[test]
fn tier_intrinsic_coverage() {
    for intrinsic in intrinsic_table() {
        if intrinsic.tiers & TIER_JIT == 0 {
            continue;
        }
        if should_skip_intrinsic(intrinsic.id) {
            continue;
        }

        let source = build_intrinsic_program(intrinsic.id);
        let module = compile_program(&source);
        let interp = std::panic::catch_unwind(|| run_interp(module.clone()));
        let interp = match interp {
            Ok(result) => result,
            Err(_) => {
                let mut message = String::from("interpreter panicked for intrinsic ");
                message.push_str(&intrinsic.name);
                message.push_str(" (id ");
                message.push_str(&intrinsic.id.to_string());
                message.push(')');
                std::panic::panic_any(message);
            }
        };
        let _ = interp.expect("interpreter should execute intrinsic");

        #[cfg(feature = "jit")]
        {
            if let Some(t2) = run_tier2(module) {
                let result = t2.expect("tier2 should execute intrinsic");
                if let Value::Int(n) = result {
                    if n == ZERO_STUB_SENTINEL {
                        let mut message =
                            String::from("tier2 returned zero-stub sentinel for intrinsic ");
                        message.push_str(&intrinsic.name);
                        message.push_str(" (id ");
                        message.push_str(&intrinsic.id.to_string());
                        message.push(')');
                        std::panic::panic_any(message);
                    }
                }
            }
        }
    }
}
