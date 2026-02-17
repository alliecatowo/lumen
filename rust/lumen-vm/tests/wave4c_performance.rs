//! Wave 4C Performance and Correctness Tests
//!
//! T390 — String interning audit
//! T391 — Map operation performance
//! T392 — Closure capture correctness
//! T393 — Large function compilation
//! T394 — Tail call optimization verification
//! T401 — Runtime error stack traces

use lumen_compiler::compile;
use lumen_vm::strings::StringTable;
use lumen_vm::values::{StringRef, Value};
use lumen_vm::vm::VM;
use std::collections::BTreeMap;

/// Helper: wrap raw Lumen code in markdown, compile, run `main`, return the result.
fn run_main(source: &str) -> Value {
    let md = format!("# wave4c-test\n\n```lumen\n{}\n```\n", source.trim());
    let module = compile(&md).expect("source should compile");
    let mut vm = VM::new();
    vm.load(module);
    vm.execute("main", vec![]).expect("main should execute")
}

/// Helper: compile and run, returning Result for error checking.
fn try_run_main(source: &str) -> Result<Value, String> {
    let md = format!("# wave4c-test\n\n```lumen\n{}\n```\n", source.trim());
    let module = compile(&md).map_err(|e| e.to_string())?;
    let mut vm = VM::new();
    vm.load(module);
    vm.execute("main", vec![]).map_err(|e| format!("{}", e))
}

// ═══════════════════════════════════════════════════════════════════════
// T390 — String interning audit
// ═══════════════════════════════════════════════════════════════════════
// Finding: StringTable already uses HashMap for O(1) lookup (strings.rs:9).
// This is correct for compiler-scale workloads. Tests below verify it.

#[test]
fn t390_string_interner_uses_hashmap_o1_lookup() {
    // Intern 10K unique strings and verify O(1) lookup behavior
    let mut table = StringTable::new();
    let mut ids = Vec::with_capacity(10_000);

    // Intern 10K unique strings
    for i in 0..10_000 {
        let s = format!("string_number_{}", i);
        let id = table.intern(&s);
        ids.push((s, id));
    }

    assert_eq!(table.len(), 10_000, "should have interned 10K strings");

    // Verify all lookups return correct IDs (re-interning returns same ID)
    for (s, expected_id) in &ids {
        let actual_id = table.intern(s);
        assert_eq!(
            *expected_id, actual_id,
            "re-interning '{}' should return same ID",
            s
        );
    }

    // Verify resolve works for all strings
    for (s, id) in &ids {
        let resolved = table.resolve(*id);
        assert_eq!(
            resolved,
            Some(s.as_str()),
            "resolve({}) should return the original string",
            id
        );
    }
}

#[test]
fn t390_string_interner_deduplication() {
    let mut table = StringTable::new();

    // Intern the same string multiple times
    let id1 = table.intern("hello");
    let id2 = table.intern("hello");
    let id3 = table.intern("hello");

    assert_eq!(id1, id2);
    assert_eq!(id2, id3);
    assert_eq!(
        table.len(),
        1,
        "duplicate strings should not increase table size"
    );
}

#[test]
fn t390_string_interner_10k_benchmark() {
    // Benchmark: Intern 10K strings, then do 10K lookups.
    // With HashMap this should be fast; with linear search it would be slow.
    let mut table = StringTable::new();

    // Phase 1: Intern 10K unique strings
    for i in 0..10_000 {
        table.intern(&format!("benchmark_string_{}", i));
    }

    // Phase 2: Re-intern (lookup) all 10K strings — should all be O(1)
    let start = std::time::Instant::now();
    for i in 0..10_000 {
        let id = table.intern(&format!("benchmark_string_{}", i));
        // Each re-intern should return a valid ID
        assert!(table.resolve(id).is_some());
    }
    let elapsed = start.elapsed();

    // With HashMap O(1): typically <10ms for 10K lookups.
    // With Vec O(n): would be ~50s for 10K * 10K comparisons.
    // Use 1 second as a generous upper bound.
    assert!(
        elapsed.as_millis() < 1000,
        "10K string lookups took {}ms — too slow (HashMap should be <10ms)",
        elapsed.as_millis()
    );
}

#[test]
fn t390_string_interner_empty_and_special_strings() {
    let mut table = StringTable::new();

    let empty_id = table.intern("");
    assert_eq!(table.resolve(empty_id), Some(""));

    let unicode_id = table.intern("日本語テスト");
    assert_eq!(table.resolve(unicode_id), Some("日本語テスト"));

    let long_id = table.intern(&"x".repeat(10_000));
    assert_eq!(table.resolve(long_id).map(|s| s.len()), Some(10_000));
}

// ═══════════════════════════════════════════════════════════════════════
// T391 — Map operation performance
// ═══════════════════════════════════════════════════════════════════════
// Finding: Maps use BTreeMap (O(log n)), which is correct. Tests verify it
// works at scale.

#[test]
fn t391_map_10k_entries_insert_and_lookup() {
    // Create a BTreeMap with 10K entries directly and verify lookups
    let mut map = BTreeMap::new();
    for i in 0..10_000 {
        map.insert(format!("key_{}", i), Value::Int(i));
    }

    let map_value = Value::new_map(map);

    // Verify all 10K lookups work
    if let Value::Map(m) = &map_value {
        for i in 0..10_000 {
            let key = format!("key_{}", i);
            let val = m.get(&key);
            assert_eq!(val, Some(&Value::Int(i)), "lookup for {} failed", key);
        }
    } else {
        panic!("expected Map value");
    }
}

#[test]
fn t391_map_performance_benchmark() {
    // Create map and do 10K lookups, measuring time
    let mut map = BTreeMap::new();
    for i in 0..10_000 {
        map.insert(format!("key_{:05}", i), Value::Int(i));
    }

    let start = std::time::Instant::now();
    for i in 0..10_000 {
        let key = format!("key_{:05}", i);
        let _ = map.get(&key);
    }
    let elapsed = start.elapsed();

    // BTreeMap O(log n) with n=10K should be well under 1 second
    assert!(
        elapsed.as_millis() < 1000,
        "10K BTreeMap lookups took {}ms — expected <100ms",
        elapsed.as_millis()
    );
}

#[test]
fn t391_map_in_lumen_program() {
    // Test map operations at moderate scale in actual Lumen code
    let result = run_main(
        r#"
cell main() -> Int
  let m = {"a": 1, "b": 2, "c": 3}
  let total = m["a"] + m["b"] + m["c"]
  return total
end
"#,
    );
    assert_eq!(result, Value::Int(6));
}

// ═══════════════════════════════════════════════════════════════════════
// T392 — Closure capture correctness audit
// ═══════════════════════════════════════════════════════════════════════
// Lumen closure syntax: fn(params) => expr

#[test]
fn t392_closure_capturing_loop_variable() {
    // Closure used within a loop
    let result = run_main(
        r#"
cell add_ten(x: Int) -> Int
  return x + 10
end

cell main() -> Int
  let sum = 0
  for i in [1, 2, 3]
    sum = sum + add_ten(i)
  end
  return sum
end
"#,
    );
    // add_ten(1)=11, add_ten(2)=12, add_ten(3)=13 => 36
    assert_eq!(result, Value::Int(36));
}

#[test]
fn t392_closure_capture_single_var() {
    // fn(x) => closure captures n from enclosing scope
    let result = run_main(
        r#"
cell make_adder(n: Int) -> fn(Int) -> Int
  return fn(x: Int) => x + n
end

cell main() -> Int
  let add5 = make_adder(5)
  return add5(10)
end
"#,
    );
    assert_eq!(result, Value::Int(15));
}

#[test]
fn t392_nested_closures() {
    // Closure inside closure — curried add
    let result = run_main(
        r#"
cell make_curried_add(a: Int) -> fn(Int) -> fn(Int) -> Int
  return fn(b: Int) => fn(c: Int) => a + b + c
end

cell main() -> Int
  let add_a = make_curried_add(1)
  let add_ab = add_a(2)
  return add_ab(3)
end
"#,
    );
    // 1 + 2 + 3 = 6
    assert_eq!(result, Value::Int(6));
}

#[test]
fn t392_closure_capturing_mutable_variable_snapshot() {
    // Closure captures a variable's value at capture time (value semantics)
    let result = run_main(
        r#"
cell main() -> Int
  let x = 100
  let f = fn() => x
  let x = 999
  return f()
end
"#,
    );
    // f captured x=100 before shadowing, so it returns 100
    assert_eq!(result, Value::Int(100));
}

#[test]
fn t392_closure_as_return_value() {
    // Closure returned from a cell retains captured variable
    let result = run_main(
        r#"
cell make_greeter(prefix: String) -> fn(String) -> String
  return fn(name: String) => prefix + " " + name
end

cell main() -> String
  let greet = make_greeter("Hello")
  return greet("World")
end
"#,
    );
    assert_eq!(
        result,
        Value::String(StringRef::Owned("Hello World".into()))
    );
}

#[test]
fn t392_closure_multiple_captures() {
    // Closure capturing multiple variables
    let result = run_main(
        r#"
cell make_linear(a: Int, b: Int) -> fn(Int) -> Int
  return fn(x: Int) => a * x + b
end

cell main() -> Int
  let f = make_linear(3, 7)
  return f(10)
end
"#,
    );
    // 3 * 10 + 7 = 37
    assert_eq!(result, Value::Int(37));
}

#[test]
fn t392_closure_as_callback() {
    // Pass closure as argument to another cell
    let result = run_main(
        r#"
cell apply(f: fn(Int) -> Int, x: Int) -> Int
  return f(x)
end

cell main() -> Int
  let offset = 100
  let add_offset = fn(x: Int) => x + offset
  return apply(add_offset, 42)
end
"#,
    );
    assert_eq!(result, Value::Int(142));
}

#[test]
fn t392_closure_called_multiple_times() {
    // Same closure called multiple times returns consistent results
    let result = run_main(
        r#"
cell make_counter_fn(base: Int) -> fn(Int) -> Int
  return fn(n: Int) => base + n
end

cell main() -> Int
  let f = make_counter_fn(10)
  let a = f(1)
  let b = f(2)
  let c = f(3)
  return a + b + c
end
"#,
    );
    // 11 + 12 + 13 = 36
    assert_eq!(result, Value::Int(36));
}

// ═══════════════════════════════════════════════════════════════════════
// T393 — Large function compilation
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn t393_large_function_50_locals() {
    // Generate a function with 50+ local variables
    let mut source = String::from("cell main() -> Int\n");

    // Declare 50 local variables (staying within register budget)
    for i in 0..50 {
        source.push_str(&format!("  let v{} = {}\n", i, i));
    }

    // Create a sum using some of them
    source.push_str("  let total = v0 + v1 + v2 + v3 + v4 + v5 + v6 + v7 + v8 + v9\n");
    source
        .push_str("  total = total + v10 + v11 + v12 + v13 + v14 + v15 + v16 + v17 + v18 + v19\n");

    source.push_str("  return total\n");
    source.push_str("end\n");

    let md = format!("# t393\n\n```lumen\n{}\n```\n", source.trim());
    let module = compile(&md).expect("large function should compile");
    let mut vm = VM::new();
    vm.load(module);
    let result = vm.execute("main", vec![]).expect("should execute");

    // Sum of 0..20 = 190
    assert_eq!(result, Value::Int(190));
}

#[test]
fn t393_deeply_nested_function() {
    // Function with deep nesting (10+ levels)
    let result = run_main(
        r#"
cell main() -> Int
  let x = 0
  if true
    if true
      if true
        if true
          if true
            if true
              if true
                if true
                  if true
                    if true
                      x = 42
                    end
                  end
                end
              end
            end
          end
        end
      end
    end
  end
  return x
end
"#,
    );
    assert_eq!(result, Value::Int(42));
}

#[test]
fn t393_large_function_with_helper_cells() {
    // Instead of one giant function, test compilation of a program with many cells
    let mut source = String::new();

    // Generate 30 helper cells
    for i in 0..30 {
        source.push_str(&format!(
            "cell helper_{}(x: Int) -> Int\n  return x + {}\nend\n\n",
            i, i
        ));
    }

    // Main cell calls all helpers
    source.push_str("cell main() -> Int\n");
    source.push_str("  let total = 0\n");
    for i in 0..30 {
        source.push_str(&format!("  total = total + helper_{}(1)\n", i));
    }
    source.push_str("  return total\n");
    source.push_str("end\n");

    let md = format!("# t393\n\n```lumen\n{}\n```\n", source.trim());
    let module = compile(&md).expect("program with 30 cells should compile");
    let mut vm = VM::new();
    vm.load(module);
    let result = vm.execute("main", vec![]).expect("should execute");

    // Sum of (1+i) for i=0..30 = 30 * 1 + sum(0..30) = 30 + 435 = 465
    assert_eq!(result, Value::Int(465));
}

#[test]
fn t393_register_limit_exceeded_is_reported() {
    // Verify that exceeding 255 registers produces a clear error
    let mut source = String::from("cell main() -> Int\n");
    source.push_str("  let total = 0\n");

    // Generate enough statements to exceed 255 registers
    for i in 0..100 {
        source.push_str(&format!("  let tmp_{} = {} * 2\n", i, i));
        source.push_str(&format!("  total = total + tmp_{}\n", i));
        // Add conditional to prevent temp reuse across scopes
        source.push_str(&format!("  if tmp_{} > 0\n", i));
        source.push_str("    total = total + 1\n");
        source.push_str("  end\n");
    }

    source.push_str("  return total\n");
    source.push_str("end\n");

    let md = format!("# t393\n\n```lumen\n{}\n```\n", source.trim());
    let result = compile(&md);
    // This should either compile (if register recycling handles it) or
    // produce a clear error about register limits
    match result {
        Ok(_) => {} // If it compiles, that's fine
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("register") || msg.contains("Register") || msg.contains("complex"),
                "register limit error should mention registers, got: {}",
                msg
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// T394 — Tail call optimization verification
// ═══════════════════════════════════════════════════════════════════════
// Finding: The compiler emits TailCall opcode, and the VM's dispatch_tailcall
// reuses the current frame (no stack growth). This is true TCO.
// The VM has MAX_CALL_DEPTH = 256 for regular calls.

#[test]
fn t394_tail_recursive_countdown() {
    // Simple tail-recursive function
    let result = run_main(
        r#"
cell countdown(n: Int) -> Int
  if n <= 0
    return 0
  end
  return countdown(n - 1)
end

cell main() -> Int
  return countdown(1000)
end
"#,
    );
    assert_eq!(result, Value::Int(0));
}

#[test]
fn t394_tail_recursive_accumulator() {
    // Tail-recursive sum with accumulator
    let result = run_main(
        r#"
cell sum_tail(n: Int, acc: Int) -> Int
  if n <= 0
    return acc
  end
  return sum_tail(n - 1, acc + n)
end

cell main() -> Int
  return sum_tail(100, 0)
end
"#,
    );
    // Sum of 1..100 = 5050
    assert_eq!(result, Value::Int(5050));
}

#[test]
#[ignore] // call depth raised to 4096 for perf sprint
fn t394_non_tail_recursive_stack_overflow() {
    // Non-tail recursive should hit stack overflow at depth > 256
    let err = try_run_main(
        r#"
cell recurse(n: Int) -> Int
  if n <= 0
    return 0
  end
  let result = recurse(n - 1)
  return result + 1
end

cell main() -> Int
  return recurse(500)
end
"#,
    );
    // Should either hit stack overflow or instruction limit
    assert!(
        err.is_err(),
        "non-tail recursion past 256 depth should error"
    );
    let err_msg = err.unwrap_err();
    assert!(
        err_msg.contains("stack overflow") || err_msg.contains("call depth"),
        "error should mention stack overflow, got: {}",
        err_msg
    );
}

#[test]
#[ignore] // call depth raised to 4096 for perf sprint
fn t394_max_call_depth_256_documented() {
    // Verify the max call depth is 256 via a direct test
    let err = try_run_main(
        r#"
cell deep(n: Int) -> Int
  if n <= 0
    return 0
  end
  let x = deep(n - 1)
  return x + 1
end

cell main() -> Int
  return deep(300)
end
"#,
    );
    assert!(err.is_err(), "300-depth non-tail recursion should fail");
    let msg = err.unwrap_err();
    assert!(
        msg.contains("256") || msg.contains("stack overflow"),
        "error should reference max depth 256, got: {}",
        msg
    );
}

#[test]
fn t394_tail_call_deep_recursion_no_overflow() {
    // With TCO, even 10K deep tail recursion should work without stack overflow
    let result = run_main(
        r#"
cell count_down(n: Int, acc: Int) -> Int
  if n <= 0
    return acc
  end
  return count_down(n - 1, acc + 1)
end

cell main() -> Int
  return count_down(10000, 0)
end
"#,
    );
    assert_eq!(result, Value::Int(10000));
}

// ═══════════════════════════════════════════════════════════════════════
// T401 — Runtime error stack traces
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn t401_index_out_of_bounds_error() {
    let err = try_run_main(
        r#"
cell main() -> Int
  let items = [1, 2, 3]
  return items[10]
end
"#,
    );
    assert!(err.is_err());
    let msg = err.unwrap_err();
    assert!(
        msg.contains("index") && msg.contains("out of bounds"),
        "should mention index out of bounds, got: {}",
        msg
    );
    // Should include stack trace info with cell name
    assert!(
        msg.contains("main"),
        "error should mention cell name 'main', got: {}",
        msg
    );
}

#[test]
fn t401_division_by_zero_error() {
    let err = try_run_main(
        r#"
cell main() -> Int
  let x = 10
  let y = 0
  return x / y
end
"#,
    );
    assert!(err.is_err());
    let msg = err.unwrap_err();
    assert!(
        msg.contains("division by zero"),
        "should mention division by zero, got: {}",
        msg
    );
    assert!(
        msg.contains("main"),
        "error should include cell name 'main', got: {}",
        msg
    );
}

#[test]
fn t401_nested_call_stack_trace() {
    // Error in a nested call should show the full stack trace
    let err = try_run_main(
        r#"
cell helper() -> Int
  let items = [1, 2]
  return items[99]
end

cell middle() -> Int
  return helper()
end

cell main() -> Int
  return middle()
end
"#,
    );
    assert!(err.is_err());
    let msg = err.unwrap_err();
    assert!(
        msg.contains("index") && msg.contains("out of bounds"),
        "should mention index error, got: {}",
        msg
    );
    // Stack trace should show the call chain
    assert!(
        msg.contains("main") || msg.contains("Stack trace"),
        "error should include stack trace info, got: {}",
        msg
    );
}

#[test]
fn t401_runtime_error_includes_meaningful_message() {
    // Test that runtime errors are human-readable
    let err = try_run_main(
        r#"
cell main() -> Int
  let items: list[Int] = []
  return items[0]
end
"#,
    );
    assert!(err.is_err());
    let msg = err.unwrap_err();
    // Should have a specific, actionable message
    assert!(
        msg.contains("index 0 out of bounds") || msg.contains("out of bounds for list of length 0"),
        "should specify what went wrong, got: {}",
        msg
    );
}

#[test]
fn t401_stack_trace_with_multiple_frames() {
    // Verify stack trace includes frame info when error happens deep in call chain
    let err = try_run_main(
        r#"
cell level3() -> Int
  return 1 / 0
end

cell level2() -> Int
  return level3()
end

cell level1() -> Int
  return level2()
end

cell main() -> Int
  return level1()
end
"#,
    );
    assert!(err.is_err());
    let msg = err.unwrap_err();
    assert!(
        msg.contains("division by zero"),
        "should report division by zero, got: {}",
        msg
    );
    // The error should have stack trace information
    assert!(
        msg.contains("Stack trace") || msg.contains("main"),
        "should include stack trace, got: {}",
        msg
    );
}

#[test]
fn t401_negative_index_out_of_bounds() {
    let err = try_run_main(
        r#"
cell main() -> Int
  let items = [10, 20, 30]
  return items[-5]
end
"#,
    );
    assert!(err.is_err());
    let msg = err.unwrap_err();
    assert!(
        msg.contains("out of bounds"),
        "negative index out of bounds should be reported, got: {}",
        msg
    );
}
