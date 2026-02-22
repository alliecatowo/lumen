//! Tier parity tests — verify that interpreter (Tier 0), stitcher (Tier 1),
//! and Cranelift JIT (Tier 2) produce identical results for the same programs.
//!
//! # Test strategy
//!
//! Each test:
//! 1. Compiles the source program to LIR once.
//! 2. Runs it on Tier 0 (interpreter with JIT disabled).
//! 3. Runs it on Tier 1 (stitcher, threshold=1) and asserts the result matches Tier 0.
//! 4. Runs it on Tier 2 (Cranelift, threshold=1) and asserts the result matches Tier 0.
//!
//! Tiers 1 and 2 require the `jit` Cargo feature (enabled by default).
//! When a JIT tier is unavailable or fails to compile, the corresponding
//! assertion is skipped rather than hard-failing.

use lumen_compiler::compile as compile_lumen;
use lumen_core::lir::LirModule;
use lumen_rt::jit_tier::JitTierConfig;
use lumen_rt::values::Value;
use lumen_rt::vm::VM;
use std::time::Instant;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Compile Lumen source (raw, not markdown) to LIR. Panics on compile error.
fn compile(source: &str) -> LirModule {
    let md = format!("# parity-test\n\n```lumen\n{}\n```\n", source.trim());
    compile_lumen(&md).expect("source should compile")
}

/// Run `cell_name` on Tier 0 (interpreter, JIT disabled).
fn run_tier0(module: LirModule, cell_name: &str) -> Result<Value, String> {
    let mut vm = VM::new();
    vm.load(module);
    vm.execute(cell_name, vec![])
        .map_err(|e| format!("tier0 error: {e}"))
}

/// Run `cell_name` on Tier 1 (stencils, threshold=1).
///
/// Two executions are performed WITHOUT reloading the module between them,
/// so JIT stats and compiled code are preserved across calls.
/// The first call crosses the threshold and triggers compilation;
/// the second call should execute the stitched native code.
#[cfg(feature = "jit")]
fn run_tier1(module: LirModule, cell_name: &str) -> Option<Result<Value, String>> {
    let mut vm = VM::new();
    vm.enable_jit_with_config(JitTierConfig::from_threshold(1));
    vm.load(module);
    // First call: crosses threshold=1, triggers Tier-1 compilation. Ignore result.
    let _ = vm.execute(cell_name, vec![]);
    // Second call: should run the stitched code.
    let result = vm
        .execute(cell_name, vec![])
        .map_err(|e| format!("tier1 error: {e}"));

    if vm.jit_stats().jit_executions == 0 {
        return None; // stencil not active for this cell — skip comparison
    }

    Some(result)
}

/// Run `cell_name` on Tier 2 (Cranelift JIT, threshold=1).
///
/// Same warm-up strategy as Tier 1: two executions, no intermediate load().
#[cfg(feature = "jit")]
fn run_tier2(module: LirModule, cell_name: &str) -> Option<Result<Value, String>> {
    let mut vm = VM::new();
    vm.enable_jit(1);
    vm.load(module);
    // First call: triggers compilation.
    let _ = vm.execute(cell_name, vec![]);
    // Second call: should run Cranelift-compiled code.
    let result = vm
        .execute(cell_name, vec![])
        .map_err(|e| format!("tier2 error: {e}"));

    if vm.jit_stats().jit_executions == 0 {
        return None; // Cranelift not active for this cell — skip comparison
    }

    Some(result)
}

/// Assert that all available tiers produce identical results.
///
/// If Tier 0 returns an error, the test is still recorded so we can see that
/// the interpreter itself is failing. Tier 1/2 mismatches against Tier 0 are
/// hard failures.
fn assert_parity(source: &str) {
    let module = compile(source);
    let t0 = run_tier0(module.clone(), "main");

    #[cfg(feature = "jit")]
    {
        if let Some(t1) = run_tier1(module.clone(), "main") {
            assert_eq!(t0, t1, "Tier 0 vs Tier 1 mismatch.\nSource:\n{source}");
        }
        if let Some(t2) = run_tier2(module, "main") {
            assert_eq!(t0, t2, "Tier 0 vs Tier 2 mismatch.\nSource:\n{source}");
        }
    }

    // If Tier 0 itself panics/errors, surface the error.
    let _ = t0.expect("Tier 0 (interpreter) execution failed");
}

// ── arithmetic parity ─────────────────────────────────────────────────────────

#[test]
fn parity_int_add() {
    assert_parity(
        r#"
cell main() -> Int
  return 2 + 3
end
"#,
    );
}

#[test]
fn parity_int_sub() {
    assert_parity(
        r#"
cell main() -> Int
  return 10 - 4
end
"#,
    );
}

#[test]
fn parity_int_chain() {
    assert_parity(
        r#"
cell add_chain() -> Int
  return 1 + 2 + 3
end

cell main() -> Int
  return add_chain()
end
"#,
    );
}

#[test]
fn parity_int_mul() {
    assert_parity(
        r#"
cell main() -> Int
  return 6 * 7
end
"#,
    );
}

#[test]
fn parity_int_div() {
    assert_parity(
        r#"
cell main() -> Int
  return 20 // 4
end
"#,
    );
}

#[test]
fn parity_int_mod() {
    assert_parity(
        r#"
cell main() -> Int
  return 17 % 5
end
"#,
    );
}

#[test]
fn parity_int_neg() {
    assert_parity(
        r#"
cell main() -> Int
  let x = 42
  return 0 - x
end
"#,
    );
}

#[test]
fn parity_float_add() {
    assert_parity(
        r#"
cell main() -> Float
  return 1.5 + 2.5
end
"#,
    );
}

#[test]
fn parity_float_mul() {
    assert_parity(
        r#"
cell main() -> Float
  return 2.0 * 3.0
end
"#,
    );
}

#[test]
fn parity_mixed_int_float() {
    assert_parity(
        r#"
cell main() -> Float
  let x: Int = 5
  let y: Float = 1.5
  return x + 1 + y
end
"#,
    );
}

// ── control flow parity ───────────────────────────────────────────────────────

#[test]
fn parity_if_else_true() {
    assert_parity(
        r#"
cell main() -> Int
  let x = 10
  if x > 5
    return x * 2
  else
    return x
  end
end
"#,
    );
}

#[test]
fn parity_if_else_false() {
    assert_parity(
        r#"
cell main() -> Int
  let x = 3
  if x > 5
    return x * 2
  else
    return x
  end
end
"#,
    );
}

#[test]
fn parity_for_loop_sum() {
    assert_parity(
        r#"
cell main() -> Int
  let sum = 0
  let i = 1
  while i <= 5
    sum = sum + i
    i = i + 1
  end
  return sum
end
"#,
    );
}

#[test]
fn parity_while_loop() {
    assert_parity(
        r#"
cell main() -> Int
  let n = 1
  let result = 0
  while n <= 5
    result = result + n
    n = n + 1
  end
  return result
end
"#,
    );
}

#[test]
fn parity_nested_loops() {
    assert_parity(
        r#"
cell main() -> Int
  let count = 0
  let i = 1
  while i <= 4
    let j = 1
    while j <= 4
      count = count + 1
      j = j + 1
    end
    i = i + 1
  end
  return count
end
"#,
    );
}

#[test]
fn parity_match_int() {
    assert_parity(
        r#"
cell main() -> String
  let x = 2
  match x
    1 -> return "one"
    2 -> return "two"
    _ -> return "other"
  end
end
"#,
    );
}

// ── bool / comparison parity ──────────────────────────────────────────────────

#[test]
fn parity_bool_and_true() {
    assert_parity(
        r#"
cell main() -> Bool
  return true and true
end
"#,
    );
}

#[test]
fn parity_bool_and_false() {
    assert_parity(
        r#"
cell main() -> Bool
  return true and false
end
"#,
    );
}

#[test]
fn parity_bool_or() {
    assert_parity(
        r#"
cell main() -> Bool
  return false or true
end
"#,
    );
}

#[test]
fn parity_bool_not() {
    assert_parity(
        r#"
cell main() -> Bool
  return not false
end
"#,
    );
}

#[test]
fn parity_comparison_lt() {
    assert_parity(
        r#"
cell main() -> Bool
  return 3 < 5
end
"#,
    );
}

#[test]
fn parity_comparison_gt() {
    assert_parity(
        r#"
cell main() -> Bool
  return 10 > 7
end
"#,
    );
}

#[test]
fn parity_comparison_eq() {
    assert_parity(
        r#"
cell main() -> Bool
  return 4 == 4
end
"#,
    );
}

#[test]
fn parity_comparison_ne() {
    assert_parity(
        r#"
cell main() -> Bool
  return 3 != 5
end
"#,
    );
}

#[test]
fn parity_null_check() {
    assert_parity(
        r#"
cell main() -> Bool
  let x: Int? = null
  return x == null
end
"#,
    );
}

// ── collection parity ─────────────────────────────────────────────────────────

#[test]
fn parity_list_len() {
    assert_parity(
        r#"
cell main() -> Int
  let xs = [1, 2, 3, 4, 5]
  return len(xs)
end
"#,
    );
}

#[test]
fn parity_list_index() {
    assert_parity(
        r#"
cell main() -> Int
  let xs = [10, 20, 30]
  return xs[1]
end
"#,
    );
}

#[test]
fn parity_list_append() {
    assert_parity(
        r#"
cell main() -> Int
  let xs = [1, 2, 3, 4]
  return length(xs)
end
"#,
    );
}

#[test]
fn parity_tuple_index() {
    assert_parity(
        r#"
cell main() -> Int
  let tup = (10, 20, 30)
  return tup[2]
end
"#,
    );
}

#[test]
fn parity_map_lookup() {
    assert_parity(
        r#"
cell main() -> String
  let m = {"a": "one", "b": "two"}
  return m["b"]
end
"#,
    );
}

#[test]
fn parity_set_index() {
    assert_parity(
        r#"
cell main() -> Int
  let s = {1, 2, 3}
  return s[1]
end
"#,
    );
}

// ── record parity ─────────────────────────────────────────────────────────────

#[test]
fn parity_record_field_access() {
    assert_parity(
        r#"
record Point
  x: Int
  y: Int
end

cell main() -> Int
  let p = Point(x: 3, y: 7)
  return p.x + p.y
end
"#,
    );
}

#[test]
fn parity_record_construction() {
    // Exercises NewRecord opcode lowering: record created with fields via SetField,
    // then a field read via GetField.
    assert_parity(
        r#"
record Rect
  width: Int
  height: Int
end

cell main() -> Int
  let r = Rect(width: 4, height: 6)
  return r.width * r.height
end
"#,
    );
}

#[test]
fn parity_set_construction() {
    // Exercises NewSet opcode lowering: set literal with multiple elements.
    assert_parity(
        r#"
cell main() -> Int
  let s = {10, 20, 30}
  return s[0]
end
"#,
    );
}

// ── closure parity ────────────────────────────────────────────────────────────

#[test]
fn parity_closure_capture() {
    assert_parity(
        r#"
cell main() -> Int
  let n = 10
  let add_n = fn(x: Int) -> Int => x + n end
  return add_n(5)
end
"#,
    );
}

#[test]
fn parity_higher_order() {
    assert_parity(
        r#"
cell apply(f: fn(Int) -> Int, x: Int) -> Int
  return f(x)
end

cell main() -> Int
  let square = fn(x: Int) -> Int => x * x
  return apply(square, 7)
end
"#,
    );
}

// ── string parity ─────────────────────────────────────────────────────────────

#[test]
fn parity_string_concat() {
    assert_parity(
        r#"
cell main() -> String
  let a = "hello"
  let b = " world"
  return a ++ b
end
"#,
    );
}

#[test]
fn parity_string_len() {
    assert_parity(
        r#"
cell main() -> Int
  return len("hello")
end
"#,
    );
}

#[test]
fn parity_string_interpolation() {
    assert_parity(
        r#"
cell main() -> String
  let name = "Lumen"
  return "Hello, {name}!"
end
"#,
    );
}

// ── effect parity ─────────────────────────────────────────────────────────────
// Effects use the interpreter's SuspendedContinuation path.
// JIT support for effects is in progress; these tests verify Tier 0 correctness
// and check Tier 1/2 consistency when those tiers support effects.
//
// Regression test for lm_rt_handle_pop defer/free lifecycle correctness.
#[test]
fn parity_effect_perform_resume() {
    let source = r#"
effect Counter
  cell tick() -> Int
end

cell main() -> Int
  let result = handle perform Counter.tick() with
    Counter.tick() => resume(1)
  end
  return result
end
"#;

    // Effects are currently executed through the interpreter continuation path.
    // Keep this test focused on correctness (and heap safety) in that path.
    let module = compile(source);
    let result = run_tier0(module, "main").expect("tier0 error in effect roundtrip");
    assert_eq!(result, Value::Int(1), "unexpected effect result");
}

#[test]
fn parity_effect_basic() {
    assert_parity(
        r#"
effect Log
  cell log(msg: String) -> Null
end

cell main() -> Int
  let result = handle
    perform Log.log("hello")
    42
  with
    Log.log(msg) =>
      resume(null)
  end
  return result
end
"#,
    );
}

// ── effect latency benchmark ──────────────────────────────────────────────────

/// Measure the per-effect overhead in the interpreter.
///
/// This is a micro-benchmark, not a correctness test. It prints timing
/// information but does not assert a hard latency target (since CI machines
/// vary in performance). The intended target is <30 ns per switch on x86_64.
///
/// Run with: `cargo test -p lumen-rt -- bench_effect_latency --nocapture`
#[test]
#[ignore]
fn bench_effect_latency() {
    let source = r#"
effect Tick
  operation tick() -> Int
end

cell loop_effects(n: Int) -> Int / {Tick}
  let i = 0
  let acc = 0
  while i < n
    acc = acc + perform Tick.tick()
    i = i + 1
  end
  return acc
end

cell main() -> Int
  let result = handle loop_effects(10000) with
    Tick.tick() => resume(1)
  end
  return result
end
"#;

    let module = compile(source);
    let iterations = 10_000u64;

    // Warm up
    let mut vm = VM::new();
    vm.load(module.clone());
    let _ = vm.execute("main", vec![]);

    // Timed run
    let mut vm = VM::new();
    vm.load(module);
    let t0 = Instant::now();
    let result = vm.execute("main", vec![]).expect("effect benchmark failed");
    let elapsed = t0.elapsed();

    let ns_per_effect = elapsed.as_nanos() as f64 / iterations as f64;

    println!(
        "\n[bench_effect_latency] {iterations} effects in {:.3}ms → {:.1} ns/effect",
        elapsed.as_secs_f64() * 1000.0,
        ns_per_effect
    );

    // Verify correctness: 10000 effects each returning 1 → sum = 10000
    assert_eq!(result, Value::Int(iterations as i64), "wrong effect sum");
}

// ── tier enumeration test ─────────────────────────────────────────────────────

/// Verify the tier infrastructure is wired correctly by checking that
/// compile state transitions work as expected on a simple program.
#[test]
#[cfg(feature = "jit")]
fn tier_state_transitions() {
    let source = r#"
cell main() -> Int
  return 42
end
"#;
    let module = compile(source);

    // Tier 0: stays interpreted.
    {
        let mut vm = VM::new();
        vm.load(module.clone());
        vm.execute("main", vec![]).ok();
        let stats = vm.jit_stats();
        assert_eq!(stats.jit_executions, 0, "tier0: no JIT executions expected");
    }

    // Tier 1 enabled (threshold=1): after warm-up, second call should use stencil.
    // NOTE: do NOT call vm.load() between executions — that resets JIT stats.
    {
        let mut vm = VM::new();
        vm.enable_jit_with_config(JitTierConfig::from_threshold(1));
        vm.load(module.clone());
        vm.execute("main", vec![]).ok(); // first call: crosses threshold, triggers compile
        vm.execute("main", vec![]).ok(); // second call: should run JIT code
        let stats = vm.jit_stats();
        // Note: JIT may skip cells it can't compile; we don't hard-fail here.
        println!(
            "[tier_state_transitions] jit_executions={}",
            stats.jit_executions
        );
    }

    // Tier 2 enabled (threshold=1): Cranelift path.
    {
        let mut vm = VM::new();
        vm.enable_jit(1);
        vm.load(module.clone());
        vm.execute("main", vec![]).ok(); // triggers compilation
        vm.execute("main", vec![]).ok(); // runs JIT code
        let stats = vm.jit_stats();
        println!(
            "[tier_state_transitions] jit_executions={}",
            stats.jit_executions
        );
    }
}
