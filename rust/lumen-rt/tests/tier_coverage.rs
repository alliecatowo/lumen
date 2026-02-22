//! Tier coverage tests — ensure JIT/stencil support stays aligned with
//! the canonical intrinsic coverage table in lumen-core.

use lumen_compiler::compile as compile_lumen;
use lumen_core::opcode_table::{INTRINSIC_TABLE, TIER_JIT, TIER_STENCIL};
use lumen_core::values::Value;
use lumen_rt::jit_tier::JitTierConfig;
use lumen_rt::vm::VM;

const ZERO_STUB_SENTINEL: u64 = 0x7FF9_0000_0000_0000;

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
fn run_tier1(module: lumen_core::lir::LirModule) -> Option<Result<Value, String>> {
    let mut vm = VM::new();
    vm.enable_stencil_with_config(lumen_rt::stencil_tier::StencilTierConfig::from_threshold(1));
    vm.load(module);
    let _ = vm.execute("main", vec![]);
    let result = vm
        .execute("main", vec![])
        .map_err(|e| "tier1 error: ".to_string() + &e.to_string());
    if vm.stencil_stats().stencil_executions == 0 {
        return None;
    }
    Some(result)
}

#[cfg(feature = "jit")]
fn run_tier2(module: lumen_core::lir::LirModule) -> Option<Result<Value, String>> {
    let mut vm = VM::new();
    vm.enable_jit_with_config(JitTierConfig::from_threshold(1));
    vm.load(module);
    let _ = vm.execute("main", vec![]);
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
        9 | 68 | 85 | 96 | 97 | 133 => true,
        _ => false,
    }
}

fn build_intrinsic_program(id: u16) -> String {
    let id_str = id.to_string();
    let mut lines: Vec<String> = Vec::new();
    lines.push("cell main() -> Int".to_string());
    lines.push("  return intrinsic_".to_string() + &id_str + "()");
    lines.push("end".to_string());
    lines.push(String::from(""));
    lines.push(String::from("cell intrinsic_") + &id_str + "() -> Int");
    lines.push(String::from("  let x = 1"));
    lines.push(String::from("  let y = 2"));
    lines.push(String::from("  let s = \"hello\""));
    lines.push(String::from("  let xs = [1, 2, 3]"));
    lines.push(String::from("  let zs = [1, 2, 3]"));
    lines.push(String::from("  let m = [\"a\", \"b\"]"));
    lines.push(String::from(""));
    lines.push("  match ".to_string() + &id_str);
    lines.push("    0 -> return length(xs)".to_string());
    lines.push("    1 -> return count(xs)".to_string());
    lines.push("    2 -> return if matches(x) then 1 else 0".to_string());
    lines.push("    3 -> return length(hash(s))".to_string());
    lines.push("    9 -> return 0".to_string());
    lines.push("    10 -> return length(to_string(x))".to_string());
    lines.push("    11 -> return int(123)".to_string());
    lines.push("    12 -> return int(float(123))".to_string());
    lines.push("    13 -> return length(type_of(x))".to_string());
    lines.push(r#"    16 -> return if contains(s, "ell") then 1 else 0"#.to_string());
    lines.push(r#"    17 -> return length(join(["a", "b"], ","))"#.to_string());
    lines.push(r#"    18 -> return length(split("a,b", ","))"#.to_string());
    lines.push(r#"    19 -> return length(trim(" hi "))"#.to_string());
    lines.push(r#"    20 -> return length(upper("hi"))"#.to_string());
    lines.push(r#"    21 -> return length(lower("HI"))"#.to_string());
    lines.push(r#"    22 -> return length(replace("a", "a", "b"))"#.to_string());
    lines.push(r#"    23 -> return length(slice("hello", 0, 2))"#.to_string());
    lines.push("    24 -> return length(append(xs, 4))".to_string());
    lines.push("    25 -> return length(range(0, 3))".to_string());
    lines.push("    26 -> return abs(-7)".to_string());
    lines.push("    27 -> return min(1, 2)".to_string());
    lines.push("    28 -> return max(1, 2)".to_string());
    lines.push("    29 -> return length(sort(xs))".to_string());
    lines.push("    30 -> return length(reverse(xs))".to_string());
    lines.push("    31 -> return length(map(xs, fn(n: Int) -> Int => n + 1))".to_string());
    lines.push("    32 -> return length(filter(xs, fn(n: Int) -> Bool => n > 1))".to_string());
    lines.push("    33 -> return reduce(xs, fn(a: Int, b: Int) -> Int => a + b, 0)".to_string());
    lines.push(
        "    34 -> return length(flat_map(xs, fn(n: Int) -> list[Int] => [n, n]))".to_string(),
    );
    lines.push("    35 -> return length(zip([1, 2], [3, 4]))".to_string());
    lines.push("    36 -> return length(enumerate([1, 2]))".to_string());
    lines
        .push("    37 -> return if any(xs, fn(n: Int) -> Bool => n > 2) then 1 else 0".to_string());
    lines
        .push("    38 -> return if all(xs, fn(n: Int) -> Bool => n > 0) then 1 else 0".to_string());
    lines.push(
        "    39 -> return if find(xs, fn(n: Int) -> Bool => n > 2) is Null then 0 else 1"
            .to_string(),
    );
    lines.push("    40 -> return position(xs, fn(n: Int) -> Bool => n > 2)".to_string());
    lines.push(
        "    41 -> return length(keys(group_by(xs, fn(n: Int) -> String => to_string(n))))"
            .to_string(),
    );
    lines.push("    42 -> return length(chunk(xs, 2))".to_string());
    lines.push("    43 -> return length(window(xs, 2))".to_string());
    lines.push("    44 -> return length(flatten([[1, 2], [3]]))".to_string());
    lines.push("    45 -> return length(unique([1, 1, 2]))".to_string());
    lines.push("    46 -> return length(take(xs, 2))".to_string());
    lines.push("    47 -> return length(drop(xs, 2))".to_string());
    lines.push("    48 -> return if first(xs) is Null then 0 else 1".to_string());
    lines.push("    49 -> return if last(xs) is Null then 0 else 1".to_string());
    lines.push("    50 -> return if is_empty([]) then 1 else 0".to_string());
    lines.push(r#"    51 -> return length(chars("hi"))"#.to_string());
    lines.push(r#"    52 -> return if starts_with("hello", "he") then 1 else 0"#.to_string());
    lines.push(r#"    53 -> return if ends_with("hello", "lo") then 1 else 0"#.to_string());
    lines.push(r#"    54 -> return index_of("hello", "lo")"#.to_string());
    lines.push(r#"    55 -> return length(pad_left("hi", 4))"#.to_string());
    lines.push(r#"    56 -> return length(pad_right("hi", 4))"#.to_string());
    lines.push("    57 -> return int(round(3.2))".to_string());
    lines.push("    58 -> return int(ceil(3.2))".to_string());
    lines.push("    59 -> return int(floor(3.2))".to_string());
    lines.push("    60 -> return int(sqrt(4.0))".to_string());
    lines.push("    61 -> return int(pow(2, 3))".to_string());
    lines.push("    62 -> return int(log(1.0))".to_string());
    lines.push("    63 -> return int(sin(0.0))".to_string());
    lines.push("    64 -> return int(cos(0.0))".to_string());
    lines.push("    65 -> return clamp(5, 1, 10)".to_string());
    lines.push("    66 -> return length(to_string(clone(xs)))".to_string());
    lines.push("    68 -> return 0".to_string());
    lines.push("    69 -> return length(to_string(to_set(xs)))".to_string());
    lines.push(r#"    70 -> return if has_key(["a", "b"], "a") then 1 else 0"#.to_string());
    lines.push(r#"    71 -> return length(keys(merge(["a"], ["c"])))"#.to_string());
    lines.push("    72 -> return size(xs)".to_string());
    lines.push("    73 -> return length(to_string(add(to_set(zs), 4)))".to_string());
    lines.push(r#"    74 -> return length(keys(remove(["a"], "a")))"#.to_string());
    lines.push(r#"    75 -> return length(entries(["a", "b"]))"#.to_string());
    lines.push("    85 -> return 0".to_string());
    lines.push("    96 -> return 0".to_string());
    lines.push("    97 -> return 0".to_string());
    lines.push("    106 -> return length(string_concat(123))".to_string());
    lines.push(r#"    120 -> return length(map_sorted_keys(["a", "b"]))"#.to_string());
    lines.push(r#"    121 -> return parse_int("123")"#.to_string());
    lines.push(r#"    122 -> return int(parse_float("123.0"))"#.to_string());
    lines.push("    123 -> return int(log2(4.0))".to_string());
    lines.push("    124 -> return int(log10(100.0))".to_string());
    lines.push("    125 -> return if is_nan(0.0) then 1 else 0".to_string());
    lines.push("    126 -> return if is_infinite(0.0) then 1 else 0".to_string());
    lines.push("    127 -> return int(math_pi)".to_string());
    lines.push("    128 -> return int(math_e)".to_string());
    lines.push("    129 -> return length(sort_asc(xs))".to_string());
    lines.push("    130 -> return length(sort_desc(xs))".to_string());
    lines.push("    131 -> return length(sort_by(xs, fn(n: Int) -> Int => -n))".to_string());
    lines.push("    133 -> return 0".to_string());
    lines.push("    138 -> return int(tan(0.0))".to_string());
    lines.push("    139 -> return int(trunc(3.9))".to_string());
    lines.push("    _ -> return 0".to_string());
    lines.push("  end".to_string());
    lines.push("end".to_string());
    lines.join("\n")
}

#[test]
fn tier_intrinsic_coverage() {
    for intrinsic in INTRINSIC_TABLE {
        if intrinsic.tiers & TIER_JIT == 0 {
            continue;
        }
        if should_skip_intrinsic(intrinsic.id) {
            continue;
        }

        let source = build_intrinsic_program(intrinsic.id);
        let module = compile_program(&source);
        let interp = run_interp(module.clone());
        let _ = interp.expect("interpreter should execute intrinsic");

        #[cfg(feature = "jit")]
        {
            if intrinsic.tiers & TIER_STENCIL != 0 {
                if let Some(t1) = run_tier1(module.clone()) {
                    let result = t1.expect("tier1 should execute intrinsic");
                    if let Value::Int(n) = result {
                        if n as u64 == ZERO_STUB_SENTINEL {
                            let mut message =
                                String::from("tier1 returned zero-stub sentinel for intrinsic ");
                            message.push_str(intrinsic.name);
                            message.push_str(" (id ");
                            message.push_str(&intrinsic.id.to_string());
                            message.push(')');
                            std::panic::panic_any(message);
                        }
                    }
                }
            }

            if let Some(t2) = run_tier2(module) {
                let result = t2.expect("tier2 should execute intrinsic");
                if let Value::Int(n) = result {
                    if n as u64 == ZERO_STUB_SENTINEL {
                        let mut message =
                            String::from("tier2 returned zero-stub sentinel for intrinsic ");
                        message.push_str(intrinsic.name);
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
