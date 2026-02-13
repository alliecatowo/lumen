//! End-to-end tests: compile Lumen source and execute it in the VM.

use lumen_compiler::compile;
use lumen_vm::values::{StringRef, Value};
use lumen_vm::vm::VM;

/// Helper: wrap raw Lumen code in markdown, compile it, run `main`, return the result.
fn run_main(source: &str) -> Value {
    let md = format!("# e2e-test\n\n```lumen\n{}\n```\n", source.trim());
    let module = compile(&md).expect("source should compile");
    let mut vm = VM::new();
    vm.load(module);
    vm.execute("main", vec![]).expect("main should execute")
}

/// Helper: run and also capture printed output.
fn run_main_with_output(source: &str) -> (Value, Vec<String>) {
    let md = format!("# e2e-test\n\n```lumen\n{}\n```\n", source.trim());
    let module = compile(&md).expect("source should compile");
    let mut vm = VM::new();
    vm.load(module);
    let result = vm.execute("main", vec![]).expect("main should execute");
    (result, vm.output)
}

// ─── Simple arithmetic ───

#[test]
fn e2e_simple_addition() {
    let result = run_main(
        r#"
cell main() -> Int
  return 2 + 3
end
"#,
    );
    assert_eq!(result, Value::Int(5));
}

#[test]
fn e2e_arithmetic_precedence() {
    let result = run_main(
        r#"
cell main() -> Int
  return 2 + 3 * 4
end
"#,
    );
    assert_eq!(result, Value::Int(14));
}

#[test]
fn e2e_subtraction() {
    let result = run_main(
        r#"
cell main() -> Int
  return 10 - 3
end
"#,
    );
    assert_eq!(result, Value::Int(7));
}

#[test]
fn e2e_integer_division() {
    let result = run_main(
        r#"
cell main() -> Int
  return 10 / 3
end
"#,
    );
    assert_eq!(result, Value::Int(3));
}

#[test]
fn e2e_modulo() {
    let result = run_main(
        r#"
cell main() -> Int
  return 10 % 3
end
"#,
    );
    assert_eq!(result, Value::Int(1));
}

// ─── Boolean logic ───

#[test]
fn e2e_bool_and_false() {
    let result = run_main(
        r#"
cell main() -> Bool
  return true and false
end
"#,
    );
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn e2e_bool_or_true() {
    let result = run_main(
        r#"
cell main() -> Bool
  return false or true
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn e2e_bool_not() {
    let result = run_main(
        r#"
cell main() -> Bool
  return not false
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

// ─── String operations ───

#[test]
fn e2e_string_concat() {
    let result = run_main(
        r#"
cell main() -> String
  return "hello" + " world"
end
"#,
    );
    match &result {
        Value::String(StringRef::Owned(s)) => assert_eq!(s, "hello world"),
        other => panic!("expected Owned string, got {:?}", other),
    }
}

#[test]
fn e2e_string_interpolation() {
    let result = run_main(
        r#"
cell main() -> String
  let name = "Lumen"
  return "Hello, {name}!"
end
"#,
    );
    match &result {
        Value::String(StringRef::Owned(s)) => assert_eq!(s, "Hello, Lumen!"),
        other => panic!("expected Owned string, got {:?}", other),
    }
}

// ─── If/else ───

#[test]
fn e2e_if_true_branch() {
    let result = run_main(
        r#"
cell main() -> Int
  if true
    return 1
  else
    return 2
  end
end
"#,
    );
    assert_eq!(result, Value::Int(1));
}

#[test]
fn e2e_if_false_branch() {
    let result = run_main(
        r#"
cell main() -> Int
  if false
    return 1
  else
    return 2
  end
end
"#,
    );
    assert_eq!(result, Value::Int(2));
}

// ─── While loop accumulation ───

#[test]
fn e2e_while_loop_sum() {
    let result = run_main(
        r#"
cell main() -> Int
  let mut i = 0
  let mut sum = 0
  while i < 5
    sum = sum + i
    i = i + 1
  end
  return sum
end
"#,
    );
    assert_eq!(result, Value::Int(10)); // 0+1+2+3+4 = 10
}

#[test]
fn e2e_while_loop_with_break() {
    let result = run_main(
        r#"
cell main() -> Int
  let mut i = 0
  while true
    if i >= 5
      break
    end
    i = i + 1
  end
  return i
end
"#,
    );
    assert_eq!(result, Value::Int(5));
}

// ─── Function calls between cells ───

#[test]
fn e2e_cell_calls() {
    let result = run_main(
        r#"
cell double(x: Int) -> Int
  return x * 2
end

cell main() -> Int
  return double(21)
end
"#,
    );
    assert_eq!(result, Value::Int(42));
}

#[test]
fn e2e_recursive_factorial() {
    let result = run_main(
        r#"
cell factorial(n: Int) -> Int
  if n <= 1
    return 1
  end
  return n * factorial(n - 1)
end

cell main() -> Int
  return factorial(5)
end
"#,
    );
    assert_eq!(result, Value::Int(120));
}

// ─── For loop ───

#[test]
fn e2e_for_loop_sum() {
    let result = run_main(
        r#"
cell main() -> Int
  let mut sum = 0
  for x in [1, 2, 3, 4, 5]
    sum += x
  end
  return sum
end
"#,
    );
    assert_eq!(result, Value::Int(15));
}

// ─── List operations ───

#[test]
fn e2e_list_length() {
    let result = run_main(
        r#"
cell main() -> Int
  let xs = [10, 20, 30]
  return length(xs)
end
"#,
    );
    assert_eq!(result, Value::Int(3));
}

#[test]
fn e2e_list_append() {
    let result = run_main(
        r#"
cell main() -> list[Int]
  let xs = [1, 2]
  return append(xs, 3)
end
"#,
    );
    if let Value::List(items) = &result {
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], Value::Int(1));
        assert_eq!(items[1], Value::Int(2));
        assert_eq!(items[2], Value::Int(3));
    } else {
        panic!("expected list, got {:?}", result);
    }
}

// ─── Match ───

#[test]
fn e2e_match_literals() {
    let result = run_main(
        r#"
cell classify(x: Int) -> String
  match x
    0 -> return "zero"
    1 -> return "one"
    _ -> return "other"
  end
end

cell main() -> String
  return classify(1)
end
"#,
    );
    match &result {
        Value::String(StringRef::Owned(s)) => assert_eq!(s, "one"),
        other => panic!("expected Owned string, got {:?}", other),
    }
}

// ─── Print / output capture ───

#[test]
fn e2e_print_captures_output() {
    let (_result, output) = run_main_with_output(
        r#"
cell main()
  print("hello")
  print("world")
end
"#,
    );
    assert_eq!(output, vec!["hello", "world"]);
}

// ─── Comparison operators ───

#[test]
fn e2e_greater_than() {
    let result = run_main(
        r#"
cell main() -> Bool
  return 3 > 2
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn e2e_less_than() {
    let result = run_main(
        r#"
cell main() -> Bool
  return 3 < 2
end
"#,
    );
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn e2e_equality() {
    let result = run_main(
        r#"
cell main() -> Bool
  return 5 == 5
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn e2e_inequality() {
    let result = run_main(
        r#"
cell main() -> Bool
  return 5 != 5
end
"#,
    );
    assert_eq!(result, Value::Bool(false));
}

// ─── Compound assignment ───

#[test]
fn e2e_compound_assignment() {
    let result = run_main(
        r#"
cell main() -> Int
  let mut x = 10
  x += 5
  x -= 3
  x *= 2
  return x
end
"#,
    );
    assert_eq!(result, Value::Int(24));
}

// ─── Null value ───

#[test]
fn e2e_null_return() {
    let result = run_main(
        r#"
cell main() -> Null
  return null
end
"#,
    );
    assert_eq!(result, Value::Null);
}

// ─── Nested function calls ───

#[test]
fn e2e_nested_calls() {
    let result = run_main(
        r#"
cell add(a: Int, b: Int) -> Int
  return a + b
end

cell mul(a: Int, b: Int) -> Int
  return a * b
end

cell main() -> Int
  return add(mul(3, 4), mul(5, 6))
end
"#,
    );
    assert_eq!(result, Value::Int(42)); // 12 + 30
}

// ─── Let binding ───

#[test]
fn e2e_let_binding() {
    let result = run_main(
        r#"
cell main() -> Int
  let x = 10
  let y = 20
  return x + y
end
"#,
    );
    assert_eq!(result, Value::Int(30));
}

// ─── Mutable variables ───

#[test]
fn e2e_mutable_reassignment() {
    let result = run_main(
        r#"
cell main() -> Int
  let mut x = 1
  x = 2
  x = 3
  return x
end
"#,
    );
    assert_eq!(result, Value::Int(3));
}

// ─── Example files that should run end-to-end ───

fn run_example(filename: &str) {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../examples").join(filename);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", path.display(), e));
    let module = compile(&source).unwrap_or_else(|e| panic!("{} failed to compile: {}", filename, e));
    let mut vm = VM::new();
    vm.load(module);
    vm.execute("main", vec![])
        .unwrap_or_else(|e| panic!("{} failed at runtime: {}", filename, e));
}

#[test]
fn e2e_example_hello() {
    run_example("hello.lm.md");
}

#[test]
fn e2e_example_fibonacci() {
    run_example("fibonacci.lm.md");
}

#[test]
fn e2e_example_language_features() {
    run_example("language_features.lm.md");
}

#[test]
fn e2e_example_intrinsics_test() {
    run_example("intrinsics_test.lm.md");
}

#[test]
fn e2e_example_typecheck_pass() {
    run_example("typecheck_pass.lm.md");
}

#[test]
fn e2e_example_record_validation() {
    run_example("record_validation.lm.md");
}

#[test]
fn e2e_example_where_constraints() {
    run_example("where_constraints.lm.md");
}

#[test]
fn e2e_example_data_pipeline() {
    run_example("data_pipeline.lm.md");
}

#[test]
fn e2e_example_code_reviewer() {
    run_example("code_reviewer.lm.md");
}

#[test]
fn e2e_example_expect_schema() {
    run_example("expect_schema.lm.md");
}

#[test]
fn e2e_example_invoice_agent() {
    run_example("invoice_agent.lm.md");
}

#[test]
fn e2e_example_todo_manager() {
    run_example("todo_manager.lm.md");
}

// ═══════════════════════════════════════════════════════════════════
// VM error path tests
// ═══════════════════════════════════════════════════════════════════

#[test]
fn vm_error_stack_overflow() {
    // Deep recursion should trigger stack overflow, not crash.
    let md = format!("# e2e-test\n\n```lumen\n{}\n```\n", r#"
cell recurse(n: Int) -> Int
  return recurse(n + 1)
end

cell main() -> Int
  return recurse(0)
end
"#.trim());
    let module = compile(&md).expect("should compile");
    let mut vm = VM::new();
    vm.load(module);
    let result = vm.execute("main", vec![]);
    assert!(result.is_err(), "deep recursion should return an error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("stack overflow") || err_msg.contains("call depth"),
        "error should mention stack overflow, got: {}",
        err_msg
    );
}

#[test]
fn vm_error_undefined_cell() {
    // Calling a cell that doesn't exist in the module should error.
    let md = format!("# e2e-test\n\n```lumen\n{}\n```\n", r#"
cell main() -> Int
  return 1
end
"#.trim());
    let module = compile(&md).expect("should compile");
    let mut vm = VM::new();
    vm.load(module);
    let result = vm.execute("nonexistent", vec![]);
    assert!(result.is_err(), "calling undefined cell should error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("undefined cell") || err_msg.contains("nonexistent"),
        "error should mention undefined cell, got: {}",
        err_msg
    );
}

#[test]
fn vm_error_halt_statement() {
    // The halt statement should produce a Halt error.
    let md = format!("# e2e-test\n\n```lumen\n{}\n```\n", r#"
cell main() -> Int
  halt("something went wrong")
  return 0
end
"#.trim());
    let module = compile(&md).expect("should compile");
    let mut vm = VM::new();
    vm.load(module);
    let result = vm.execute("main", vec![]);
    assert!(result.is_err(), "halt should produce an error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("halt") || err_msg.contains("something went wrong"),
        "error should contain halt message, got: {}",
        err_msg
    );
}

#[test]
fn vm_error_no_module_loaded() {
    // Executing without loading a module should error.
    let mut vm = VM::new();
    let result = vm.execute("main", vec![]);
    assert!(result.is_err(), "executing without module should error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("no module") || err_msg.contains("NoModule"),
        "error should mention no module, got: {}",
        err_msg
    );
}

// ═══════════════════════════════════════════════════════════════════
// Regression e2e tests: verify actual execution results
// ═══════════════════════════════════════════════════════════════════

#[test]
fn e2e_regression_while_loop_accumulation_result() {
    // Regression for signed jump offsets: while loop must produce correct sum.
    let result = run_main(
        r#"
cell main() -> Int
  let mut x = 0
  let mut i = 0
  while i < 5
    x = x + 1
    i = i + 1
  end
  return x
end
"#,
    );
    assert_eq!(result, Value::Int(5));
}

#[test]
fn e2e_regression_match_preserves_param() {
    // Regression for match register clobber: parameter in r0 must not be
    // overwritten by the Eq result register.
    let result = run_main(
        r#"
cell check(x: Int) -> String
  match x
    1 -> return "one"
    2 -> return "two"
    _ -> return "other"
  end
end

cell main() -> String
  return check(2)
end
"#,
    );
    match &result {
        Value::String(StringRef::Owned(s)) => assert_eq!(s, "two"),
        other => panic!("expected 'two', got {:?}", other),
    }
}

#[test]
fn e2e_regression_match_preserves_param_wildcard() {
    // Match with wildcard case should also work correctly.
    let result = run_main(
        r#"
cell check(x: Int) -> String
  match x
    1 -> return "one"
    2 -> return "two"
    _ -> return "other"
  end
end

cell main() -> String
  return check(99)
end
"#,
    );
    match &result {
        Value::String(StringRef::Owned(s)) => assert_eq!(s, "other"),
        other => panic!("expected 'other', got {:?}", other),
    }
}

#[test]
fn e2e_regression_countdown_while() {
    // Counting down also requires backward jumps.
    let result = run_main(
        r#"
cell main() -> Int
  let mut i = 10
  while i > 0
    i = i - 1
  end
  return i
end
"#,
    );
    assert_eq!(result, Value::Int(0));
}

// ═══════════════════════════════════════════════════════════════════
// Additional execution tests for deeper coverage
// ═══════════════════════════════════════════════════════════════════

#[test]
fn e2e_fibonacci_value() {
    let result = run_main(
        r#"
cell fib(n: Int) -> Int
  if n <= 1
    return n
  end
  return fib(n - 1) + fib(n - 2)
end

cell main() -> Int
  return fib(10)
end
"#,
    );
    assert_eq!(result, Value::Int(55));
}

#[test]
fn e2e_nested_while_loops() {
    let result = run_main(
        r#"
cell main() -> Int
  let mut total = 0
  let mut i = 0
  while i < 3
    let mut j = 0
    while j < 4
      total = total + 1
      j = j + 1
    end
    i = i + 1
  end
  return total
end
"#,
    );
    assert_eq!(result, Value::Int(12)); // 3 * 4
}

#[test]
fn e2e_deeply_nested_if() {
    let result = run_main(
        r#"
cell classify(x: Int) -> Int
  if x > 0
    if x > 100
      return 3
    else
      if x > 10
        return 2
      else
        return 1
      end
    end
  else
    return 0
  end
end

cell main() -> Int
  return classify(50)
end
"#,
    );
    assert_eq!(result, Value::Int(2));
}

#[test]
fn e2e_multiple_cell_calls_chained() {
    let result = run_main(
        r#"
cell inc(x: Int) -> Int
  return x + 1
end

cell double(x: Int) -> Int
  return x * 2
end

cell main() -> Int
  return double(inc(double(inc(0))))
end
"#,
    );
    // inc(0) = 1, double(1) = 2, inc(2) = 3, double(3) = 6
    assert_eq!(result, Value::Int(6));
}

#[test]
fn e2e_while_with_continue() {
    let result = run_main(
        r#"
cell main() -> Int
  let mut sum = 0
  let mut i = 0
  while i < 10
    i = i + 1
    if i % 2 == 0
      continue
    end
    sum = sum + i
  end
  return sum
end
"#,
    );
    // Sum of odd numbers 1-9: 1+3+5+7+9 = 25
    assert_eq!(result, Value::Int(25));
}

#[test]
fn e2e_loop_with_break() {
    let result = run_main(
        r#"
cell main() -> Int
  let mut i = 0
  loop
    i = i + 1
    if i >= 7
      break
    end
  end
  return i
end
"#,
    );
    assert_eq!(result, Value::Int(7));
}

#[test]
fn e2e_string_concat_multi() {
    let result = run_main(
        r#"
cell main() -> String
  let a = "hello"
  let b = " "
  let c = "world"
  return a + b + c
end
"#,
    );
    match &result {
        Value::String(StringRef::Owned(s)) => assert_eq!(s, "hello world"),
        other => panic!("expected 'hello world', got {:?}", other),
    }
}

#[test]
fn e2e_negative_numbers() {
    let result = run_main(
        r#"
cell main() -> Int
  let x = -10
  let y = 3
  return x + y
end
"#,
    );
    assert_eq!(result, Value::Int(-7));
}

#[test]
fn e2e_comparison_chain() {
    let result = run_main(
        r#"
cell main() -> Bool
  let a = 5
  let b = 10
  return a < b and b > a
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn e2e_bool_complex_logic() {
    let result = run_main(
        r#"
cell main() -> Bool
  let a = true
  let b = false
  return (a or b) and not b
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn e2e_multiple_let_bindings() {
    let result = run_main(
        r#"
cell main() -> Int
  let a = 1
  let b = 2
  let c = 3
  let d = 4
  let e = 5
  return a + b + c + d + e
end
"#,
    );
    assert_eq!(result, Value::Int(15));
}

#[test]
fn e2e_three_cell_chain() {
    let result = run_main(
        r#"
cell a() -> Int
  return 1
end

cell b() -> Int
  return a() + 2
end

cell c() -> Int
  return b() + 3
end

cell main() -> Int
  return c()
end
"#,
    );
    assert_eq!(result, Value::Int(6));
}

#[test]
fn e2e_gauss_sum() {
    // Sum 1..100 using a while loop (Gauss: 5050)
    let result = run_main(
        r#"
cell main() -> Int
  let mut sum = 0
  let mut i = 1
  while i <= 100
    sum = sum + i
    i = i + 1
  end
  return sum
end
"#,
    );
    assert_eq!(result, Value::Int(5050));
}

#[test]
fn e2e_list_empty() {
    let result = run_main(
        r#"
cell main() -> Int
  let xs = []
  return length(xs)
end
"#,
    );
    assert_eq!(result, Value::Int(0));
}

#[test]
fn e2e_match_first_arm() {
    let result = run_main(
        r#"
cell check(x: Int) -> String
  match x
    0 -> return "zero"
    _ -> return "nonzero"
  end
end

cell main() -> String
  return check(0)
end
"#,
    );
    match &result {
        Value::String(StringRef::Owned(s)) => assert_eq!(s, "zero"),
        other => panic!("expected 'zero', got {:?}", other),
    }
}

#[test]
fn e2e_string_interpolation_with_expr() {
    let result = run_main(
        r#"
cell main() -> String
  let x = 2
  let y = 3
  return "sum is {x + y}"
end
"#,
    );
    match &result {
        Value::String(StringRef::Owned(s)) => assert_eq!(s, "sum is 5"),
        other => panic!("expected 'sum is 5', got {:?}", other),
    }
}

#[test]
fn e2e_for_loop_over_empty_list() {
    let result = run_main(
        r#"
cell main() -> Int
  let mut sum = 0
  for x in []
    sum += x
  end
  return sum
end
"#,
    );
    assert_eq!(result, Value::Int(0));
}

#[test]
fn e2e_multiple_match_arms() {
    // Test matching all arms including wildcard
    let result = run_main(
        r#"
cell describe(n: Int) -> String
  match n
    0 -> return "zero"
    1 -> return "one"
    2 -> return "two"
    3 -> return "three"
    _ -> return "many"
  end
end

cell main() -> String
  return describe(3)
end
"#,
    );
    match &result {
        Value::String(StringRef::Owned(s)) => assert_eq!(s, "three"),
        other => panic!("expected 'three', got {:?}", other),
    }
}

#[test]
fn e2e_cell_with_three_params() {
    let result = run_main(
        r#"
cell add3(a: Int, b: Int, c: Int) -> Int
  return a + b + c
end

cell main() -> Int
  return add3(10, 20, 30)
end
"#,
    );
    assert_eq!(result, Value::Int(60));
}

#[test]
fn e2e_bool_equality() {
    let result = run_main(
        r#"
cell main() -> Bool
  return true == true
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn e2e_print_int_output() {
    let (_result, output) = run_main_with_output(
        r#"
cell main()
  let mut i = 1
  while i <= 3
    print(i)
    i = i + 1
  end
end
"#,
    );
    assert_eq!(output, vec!["1", "2", "3"]);
}

#[test]
fn e2e_compound_mul_assign() {
    let result = run_main(
        r#"
cell main() -> Int
  let mut x = 1
  x *= 2
  x *= 3
  x *= 4
  return x
end
"#,
    );
    assert_eq!(result, Value::Int(24));
}
