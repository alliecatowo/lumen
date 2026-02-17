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
    let module =
        compile(&source).unwrap_or_else(|e| panic!("{} failed to compile: {}", filename, e));
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
    let md = format!(
        "# e2e-test\n\n```lumen\n{}\n```\n",
        r#"
cell recurse(n: Int) -> Int
  return recurse(n + 1)
end

cell main() -> Int
  return recurse(0)
end
"#
        .trim()
    );
    let module = compile(&md).expect("should compile");
    let mut vm = VM::new();
    vm.load(module);
    let result = vm.execute("main", vec![]);
    assert!(result.is_err(), "deep recursion should return an error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("stack overflow")
            || err_msg.contains("call depth")
            || err_msg.contains("instruction limit"),
        "error should mention stack overflow, got: {}",
        err_msg
    );
}

#[test]
fn vm_error_undefined_cell() {
    // Calling a cell that doesn't exist in the module should error.
    let md = format!(
        "# e2e-test\n\n```lumen\n{}\n```\n",
        r#"
cell main() -> Int
  return 1
end
"#
        .trim()
    );
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
    let md = format!(
        "# e2e-test\n\n```lumen\n{}\n```\n",
        r#"
cell main() -> Int
  halt("something went wrong")
  return 0
end
"#
        .trim()
    );
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

// ═══════════════════════════════════════════════════════════════════
// Higher-order functions and closures
// ═══════════════════════════════════════════════════════════════════

#[test]
fn e2e_higher_order_map() {
    let result = run_main(
        r#"
cell apply_to_list(f: fn(Int) -> Int, xs: list[Int]) -> list[Int]
  let mut result = []
  for x in xs
    result = append(result, f(x))
  end
  return result
end

cell double(x: Int) -> Int
  return x * 2
end

cell main() -> list[Int]
  return apply_to_list(double, [1, 2, 3])
end
"#,
    );
    if let Value::List(items) = &result {
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], Value::Int(2));
        assert_eq!(items[1], Value::Int(4));
        assert_eq!(items[2], Value::Int(6));
    } else {
        panic!("expected list, got {:?}", result);
    }
}

#[test]
fn e2e_higher_order_filter() {
    let result = run_main(
        r#"
cell filter_list(pred: fn(Int) -> Bool, xs: list[Int]) -> list[Int]
  let mut result = []
  for x in xs
    if pred(x)
      result = append(result, x)
    end
  end
  return result
end

cell is_even(x: Int) -> Bool
  return x % 2 == 0
end

cell main() -> list[Int]
  return filter_list(is_even, [1, 2, 3, 4, 5, 6])
end
"#,
    );
    if let Value::List(items) = &result {
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], Value::Int(2));
        assert_eq!(items[1], Value::Int(4));
        assert_eq!(items[2], Value::Int(6));
    } else {
        panic!("expected list, got {:?}", result);
    }
}

#[test]
fn e2e_higher_order_reduce() {
    let result = run_main(
        r#"
cell reduce_list(f: fn(Int, Int) -> Int, init: Int, xs: list[Int]) -> Int
  let mut acc = init
  for x in xs
    acc = f(acc, x)
  end
  return acc
end

cell add(a: Int, b: Int) -> Int
  return a + b
end

cell main() -> Int
  return reduce_list(add, 0, [1, 2, 3, 4, 5])
end
"#,
    );
    assert_eq!(result, Value::Int(15));
}

#[test]
fn e2e_closure_capture_single() {
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
fn e2e_closure_nested() {
    let result = run_main(
        r#"
cell make_multiplier(factor: Int) -> fn(Int) -> Int
  return fn(x: Int) => x * factor
end

cell main() -> Int
  let mul3 = make_multiplier(3)
  let mul4 = make_multiplier(4)
  return mul3(5) + mul4(5)
end
"#,
    );
    assert_eq!(result, Value::Int(35)); // 15 + 20
}

// ═══════════════════════════════════════════════════════════════════
// Recursive function calls
// ═══════════════════════════════════════════════════════════════════

#[test]
fn e2e_recursive_sum_to_n() {
    let result = run_main(
        r#"
cell sum_to(n: Int) -> Int
  if n <= 0
    return 0
  end
  return n + sum_to(n - 1)
end

cell main() -> Int
  return sum_to(10)
end
"#,
    );
    assert_eq!(result, Value::Int(55)); // 1+2+3+...+10
}

#[test]
fn e2e_recursive_power() {
    let result = run_main(
        r#"
cell power(base: Int, exp: Int) -> Int
  if exp == 0
    return 1
  end
  return base * power(base, exp - 1)
end

cell main() -> Int
  return power(2, 10)
end
"#,
    );
    assert_eq!(result, Value::Int(1024));
}

// ═══════════════════════════════════════════════════════════════════
// String intrinsics
// ═══════════════════════════════════════════════════════════════════

#[test]
fn e2e_string_split() {
    let result = run_main(
        r#"
cell main() -> Int
  let parts = split("a,b,c", ",")
  return length(parts)
end
"#,
    );
    assert_eq!(result, Value::Int(3));
}

#[test]
fn e2e_string_join() {
    let result = run_main(
        r#"
cell main() -> String
  let parts = ["hello", "world"]
  return join(parts, " ")
end
"#,
    );
    match &result {
        Value::String(StringRef::Owned(s)) => assert_eq!(s, "hello world"),
        other => panic!("expected 'hello world', got {:?}", other),
    }
}

#[test]
fn e2e_string_replace() {
    let result = run_main(
        r#"
cell main() -> String
  return replace("hello world", "world", "Lumen")
end
"#,
    );
    match &result {
        Value::String(StringRef::Owned(s)) => assert_eq!(s, "hello Lumen"),
        other => panic!("expected 'hello Lumen', got {:?}", other),
    }
}

#[test]
fn e2e_string_trim() {
    let result = run_main(
        r#"
cell main() -> String
  return trim("  hello  ")
end
"#,
    );
    match &result {
        Value::String(StringRef::Owned(s)) => assert_eq!(s, "hello"),
        other => panic!("expected 'hello', got {:?}", other),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Math intrinsics
// ═══════════════════════════════════════════════════════════════════

#[test]
fn e2e_math_abs() {
    let result = run_main(
        r#"
cell main() -> Int
  return abs(-42)
end
"#,
    );
    assert_eq!(result, Value::Int(42));
}

#[test]
fn e2e_math_min() {
    let result = run_main(
        r#"
cell main() -> Int
  return min(10, 5)
end
"#,
    );
    assert_eq!(result, Value::Int(5));
}

#[test]
fn e2e_math_max() {
    let result = run_main(
        r#"
cell main() -> Int
  return max(10, 5)
end
"#,
    );
    assert_eq!(result, Value::Int(10));
}

#[test]
fn e2e_math_clamp() {
    let result = run_main(
        r#"
cell main() -> Int
  return clamp(15, 0, 10)
end
"#,
    );
    assert_eq!(result, Value::Int(10));
}

// ═══════════════════════════════════════════════════════════════════
// List intrinsics
// ═══════════════════════════════════════════════════════════════════

#[test]
fn e2e_list_sort() {
    let result = run_main(
        r#"
cell main() -> list[Int]
  return sort([3, 1, 4, 1, 5, 9, 2, 6])
end
"#,
    );
    if let Value::List(items) = &result {
        assert_eq!(items[0], Value::Int(1));
        assert_eq!(items[1], Value::Int(1));
        assert_eq!(items[2], Value::Int(2));
        assert_eq!(items[3], Value::Int(3));
    } else {
        panic!("expected list, got {:?}", result);
    }
}

#[test]
fn e2e_list_reverse() {
    let result = run_main(
        r#"
cell main() -> list[Int]
  return reverse([1, 2, 3, 4, 5])
end
"#,
    );
    if let Value::List(items) = &result {
        assert_eq!(items.len(), 5);
        assert_eq!(items[0], Value::Int(5));
        assert_eq!(items[1], Value::Int(4));
        assert_eq!(items[2], Value::Int(3));
    } else {
        panic!("expected list, got {:?}", result);
    }
}

#[test]
fn e2e_list_flatten() {
    let result = run_main(
        r#"
cell main() -> list[Int]
  let nested = [[1, 2], [3, 4], [5]]
  return flatten(nested)
end
"#,
    );
    if let Value::List(items) = &result {
        assert_eq!(items.len(), 5);
        assert_eq!(items[0], Value::Int(1));
        assert_eq!(items[4], Value::Int(5));
    } else {
        panic!("expected list, got {:?}", result);
    }
}

#[test]
fn e2e_list_unique() {
    let result = run_main(
        r#"
cell main() -> list[Int]
  return unique([1, 2, 2, 3, 3, 3, 4])
end
"#,
    );
    if let Value::List(items) = &result {
        assert_eq!(items.len(), 4);
        assert_eq!(items[0], Value::Int(1));
        assert_eq!(items[3], Value::Int(4));
    } else {
        panic!("expected list, got {:?}", result);
    }
}

// ═══════════════════════════════════════════════════════════════════
// Record operations
// ═══════════════════════════════════════════════════════════════════

#[test]
fn e2e_record_construction_and_field_access() {
    let result = run_main(
        r#"
record Person
  name: String
  age: Int
end

cell get_age(p: Person) -> Int
  return p.age
end

cell main() -> Int
  let person = Person(name: "Alice", age: 30)
  return get_age(person)
end
"#,
    );
    assert_eq!(result, Value::Int(30));
}

#[test]
fn e2e_record_nested_access() {
    let result = run_main(
        r#"
record Address
  city: String
  zip: Int
end

record Person
  name: String
  address: Address
end

cell main() -> Int
  let addr = Address(city: "NYC", zip: 10001)
  let person = Person(name: "Bob", address: addr)
  return person.address.zip
end
"#,
    );
    assert_eq!(result, Value::Int(10001));
}

// ═══════════════════════════════════════════════════════════════════
// Enum construction and pattern matching
// ═══════════════════════════════════════════════════════════════════

#[test]
fn e2e_enum_construction_and_match() {
    let result = run_main(
        r#"
enum Color
  Red
  Green
  Blue
end

cell to_number(c: Color) -> Int
  match c
    Red() -> return 1
    Green() -> return 2
    Blue() -> return 3
  end
end

cell main() -> Int
  let c1 = Red
  let c2 = Blue
  return to_number(c1) + to_number(c2)
end
"#,
    );
    assert_eq!(result, Value::Int(4)); // 1 + 3
}

#[test]
fn e2e_enum_complex_payload() {
    let result = run_main(
        r#"
enum Shape
  Circle(radius: Int)
  Square(side: Int)
end

cell area(s: Shape) -> Int
  match s
    Circle(r) -> return r * r * 3
    Square(side) -> return side * side
  end
end

cell main() -> Int
  let circle = Circle(radius: 5)
  let square = Square(side: 4)
  return area(circle) + area(square)
end
"#,
    );
    assert_eq!(result, Value::Int(91)); // 75 + 16
}

// ═══════════════════════════════════════════════════════════════════
// Enum dot-access construction: Enum.Variant and Enum.Variant(payload)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn e2e_enum_dot_access_no_payload() {
    // Regression: Color.Red should produce Union("Red", Null), not Null
    let result = run_main(
        r#"
enum Color
  Red
  Green
  Blue
end

cell main() -> Int
  let c = Color.Red
  match c
    Red() -> return 1
    Green() -> return 2
    Blue() -> return 3
  end
end
"#,
    );
    assert_eq!(result, Value::Int(1));
}

#[test]
fn e2e_enum_dot_access_with_payload() {
    // Regression: Shape.Circle(radius: 5) produced "cannot call null"
    let result = run_main(
        r#"
enum Shape
  Circle(radius: Int)
  Square(side: Int)
end

cell main() -> Int
  let s = Shape.Circle(radius: 5)
  match s
    Circle(r) -> return r
    Square(side) -> return side
  end
end
"#,
    );
    assert_eq!(result, Value::Int(5));
}

#[test]
fn e2e_enum_dot_access_multiple_variants() {
    // Ensure all variants work with dot access
    let result = run_main(
        r#"
enum Shape
  Circle(radius: Int)
  Square(side: Int)
end

cell area(s: Shape) -> Int
  match s
    Circle(r) -> return r * r * 3
    Square(side) -> return side * side
  end
end

cell main() -> Int
  let c = Shape.Circle(radius: 5)
  let s = Shape.Square(side: 4)
  return area(c) + area(s)
end
"#,
    );
    assert_eq!(result, Value::Int(91)); // 75 + 16
}

#[test]
fn e2e_enum_dot_access_mixed_with_bare() {
    // Mix dot-access and bare variant construction
    let result = run_main(
        r#"
enum Shape
  Circle(radius: Int)
  Square(side: Int)
end

cell main() -> Int
  let c = Shape.Circle(radius: 5)
  let s = Square(side: 4)
  match c
    Circle(r) ->
      match s
        Circle(r2) -> return r + r2
        Square(side) -> return r + side
      end
    Square(side) -> return side
  end
end
"#,
    );
    assert_eq!(result, Value::Int(9)); // 5 + 4
}

#[test]
fn e2e_enum_dot_access_no_payload_all_variants() {
    // Test all no-payload variants via dot access
    let result = run_main(
        r#"
enum Color
  Red
  Green
  Blue
end

cell to_num(c: Color) -> Int
  match c
    Red() -> return 1
    Green() -> return 2
    Blue() -> return 3
  end
end

cell main() -> Int
  return to_num(Color.Red) + to_num(Color.Green) + to_num(Color.Blue)
end
"#,
    );
    assert_eq!(result, Value::Int(6)); // 1 + 2 + 3
}

#[test]
fn e2e_enum_dot_access_in_function_return() {
    // Return dot-access constructed enum from a function
    let result = run_main(
        r#"
enum Result
  Ok(value: Int)
  Err(message: String)
end

cell make_ok(x: Int) -> Result
  return Result.Ok(value: x)
end

cell main() -> Int
  let r = make_ok(42)
  match r
    Ok(v) -> return v
    Err(e) -> return 0
  end
end
"#,
    );
    assert_eq!(result, Value::Int(42));
}

#[test]
fn e2e_enum_dot_access_positional_arg() {
    // Dot access with positional argument
    let result = run_main(
        r#"
enum Option
  Some(value: Int)
  None
end

cell main() -> Int
  let x = Option.Some(42)
  match x
    Some(v) -> return v
    None() -> return 0
  end
end
"#,
    );
    assert_eq!(result, Value::Int(42));
}

// ═══════════════════════════════════════════════════════════════════
// For loop with ranges
// ═══════════════════════════════════════════════════════════════════

#[test]
fn e2e_for_loop_range() {
    let result = run_main(
        r#"
cell main() -> Int
  let mut sum = 0
  for i in 0..10
    sum += i
  end
  return sum
end
"#,
    );
    assert_eq!(result, Value::Int(45)); // 0+1+2+...+9
}

#[test]
fn e2e_for_loop_range_with_step() {
    let result = run_main(
        r#"
cell main() -> Int
  let mut sum = 0
  for i in 0..10
    if i % 2 == 0
      sum += i
    end
  end
  return sum
end
"#,
    );
    assert_eq!(result, Value::Int(20)); // 0+2+4+6+8
}

// ═══════════════════════════════════════════════════════════════════
// While loop with complex conditions
// ═══════════════════════════════════════════════════════════════════

#[test]
fn e2e_while_loop_complex_condition() {
    let result = run_main(
        r#"
cell main() -> Int
  let mut x = 0
  let mut y = 100
  while x < 10 and y > 90
    x += 1
    y -= 1
  end
  return x + y
end
"#,
    );
    assert_eq!(result, Value::Int(100)); // x=10, y=90
}

#[test]
fn e2e_while_loop_with_multiple_breaks() {
    let result = run_main(
        r#"
cell main() -> Int
  let mut i = 0
  let mut sum = 0
  while i < 100
    i += 1
    if i == 5
      continue
    end
    if i == 10
      break
    end
    sum += i
  end
  return sum
end
"#,
    );
    assert_eq!(result, Value::Int(40)); // 1+2+3+4+6+7+8+9 (skip 5, break at 10)
}

// ─── Unicode string operations ───

#[test]
fn e2e_unicode_length() {
    let result = run_main(
        r#"
cell main() -> Int
  return length("café")
end
"#,
    );
    assert_eq!(result, Value::Int(4)); // 4 characters, not 5 bytes
}

#[test]
fn e2e_unicode_slice() {
    let result = run_main(
        r#"
cell main() -> String
  return slice("café", 0, 3)
end
"#,
    );
    assert_eq!(result, Value::String(StringRef::Owned("caf".to_string())));
}

#[test]
fn e2e_unicode_index_of() {
    let result = run_main(
        r#"
cell main() -> Int
  return index_of("café", "é")
end
"#,
    );
    assert_eq!(result, Value::Int(3)); // character index, not byte index
}

#[test]
fn e2e_unicode_pad_left() {
    let result = run_main(
        r#"
cell main() -> String
  return pad_left("café", 6)
end
"#,
    );
    assert_eq!(
        result,
        Value::String(StringRef::Owned("  café".to_string()))
    );
}

#[test]
fn e2e_unicode_pad_right() {
    let result = run_main(
        r#"
cell main() -> String
  return pad_right("café", 6)
end
"#,
    );
    assert_eq!(
        result,
        Value::String(StringRef::Owned("café  ".to_string()))
    );
}

#[test]
fn e2e_unicode_chars() {
    let result = run_main(
        r#"
cell main() -> Int
  let chars = chars("café")
  return length(chars)
end
"#,
    );
    assert_eq!(result, Value::Int(4));
}

// ─── Map/Set operations ───
// Note: Map literal syntax is broken in a separate bug, so these tests are commented out

// #[test]
// fn e2e_has_key() {
//     let result = run_main(
//         r#"
// cell main() -> Bool
//   let m = #{"a": 1, "b": 2}
//   return has_key(m, "a")
// end
// "#,
//     );
//     assert_eq!(result, Value::Bool(true));
// }

#[test]
fn e2e_set_add() {
    let result = run_main(
        r#"
cell main() -> Int
  let s = {1, 2, 3}
  let s2 = add(s, 4)
  return size(s2)
end
"#,
    );
    assert_eq!(result, Value::Int(4));
}

#[test]
fn e2e_set_add_duplicate() {
    let result = run_main(
        r#"
cell main() -> Int
  let s = {1, 2, 3}
  let s2 = add(s, 2)
  return size(s2)
end
"#,
    );
    assert_eq!(result, Value::Int(3)); // no duplicate added
}

#[test]
fn e2e_set_remove() {
    let result = run_main(
        r#"
cell main() -> Int
  let s = {1, 2, 3}
  let s2 = remove(s, 2)
  return size(s2)
end
"#,
    );
    assert_eq!(result, Value::Int(2));
}

#[test]
fn e2e_regalloc_stress() {
    let result = run_main(
        r#"
cell main() -> list[Int]
  let a = 1
  let b = 2
  let c = 3
  let d = 4
  let e = 5
  
  let list_one = [a + 1, b + 2, c + 3, d + 4, e + 5]
  
  let f = 6
  let g = 7
  let h = 8
  
  let complex_list = [
    a + b + c + d + e,
    f * g * h,
    length([a, b, c]),
    length({ "key": "val" }),
    (a + b) * (c + d)
  ]
  
  return complex_list
end
"#,
    );
    if let Value::List(items) = &result {
        assert_eq!(items.len(), 5);
        assert_eq!(items[0], Value::Int(15));
        assert_eq!(items[1], Value::Int(336)); // 6 * 7 * 8
        assert_eq!(items[2], Value::Int(3));
        assert_eq!(items[3], Value::Int(1));
        assert_eq!(items[4], Value::Int(21)); // (1+2) * (3+4)
    } else {
        panic!("expected list, got {:?}", result);
    }
}

// ─── Algebraic Effects ───

#[test]
fn e2e_effect_handle_resume_basic() {
    // Minimal handle/perform/resume: perform an effect, handler resumes with a value
    let result = run_main(
        r#"
effect Ask
  cell ask(prompt: String) -> String
end

cell main() -> String / {Ask}
  let result = handle
    perform Ask.ask("name?")
  with
    Ask.ask(prompt) =>
      resume("Alice")
  end
  return result
end
"#,
    );
    match &result {
        Value::String(StringRef::Owned(s)) => assert_eq!(s, "Alice"),
        other => panic!("expected String(\"Alice\"), got {:?}", other),
    }
}

#[test]
fn e2e_effect_resume_with_int() {
    // Resume with an integer value
    let result = run_main(
        r#"
effect Counter
  cell next() -> Int
end

cell main() -> Int / {Counter}
  let result = handle
    perform Counter.next()
  with
    Counter.next() =>
      resume(42)
  end
  return result
end
"#,
    );
    assert_eq!(result, Value::Int(42));
}

#[test]
fn e2e_effect_multiple_performs() {
    // Multiple performs in sequence — each should be handled and resumed
    let result = run_main(
        r#"
effect Ask
  cell ask(prompt: String) -> String
end

cell main() -> String / {Ask}
  let result = handle
    let first = perform Ask.ask("name?")
    let second = perform Ask.ask("age?")
    first
  with
    Ask.ask(prompt) =>
      resume("answered")
  end
  return result
end
"#,
    );
    match &result {
        Value::String(StringRef::Owned(s)) => assert_eq!(s, "answered"),
        other => panic!("expected String(\"answered\"), got {:?}", other),
    }
}

#[test]
fn e2e_effect_resume_value_used_in_computation() {
    // The resumed value is used in further computation
    let result = run_main(
        r#"
effect Get
  cell get() -> Int
end

cell main() -> Int / {Get}
  let result = handle
    let x = perform Get.get()
    x + 10
  with
    Get.get() =>
      resume(32)
  end
  return result
end
"#,
    );
    assert_eq!(result, Value::Int(42));
}

#[test]
fn e2e_effect_handler_with_multiple_operations() {
    // Handle expression with two different operations on the same effect
    let result = run_main(
        r#"
effect Console
  cell log(message: String) -> Null
  cell read_line() -> String
end

cell main() -> String / {Console}
  let result = handle
    perform Console.log("hello")
    perform Console.read_line()
  with
    Console.log(message) =>
      resume(null)
    Console.read_line() =>
      resume("user input")
  end
  return result
end
"#,
    );
    match &result {
        Value::String(StringRef::Owned(s)) => assert_eq!(s, "user input"),
        other => panic!("expected String(\"user input\"), got {:?}", other),
    }
}

#[test]
fn e2e_effect_resume_null() {
    // Resuming with null (common for side-effectful operations like logging)
    let result = run_main(
        r#"
effect Logger
  cell log(msg: String) -> Null
end

cell main() -> Int / {Logger}
  let result = handle
    perform Logger.log("hello")
    100
  with
    Logger.log(msg) =>
      resume(null)
  end
  return result
end
"#,
    );
    assert_eq!(result, Value::Int(100));
}

#[test]
fn e2e_effect_resume_bool() {
    // Resume with a boolean value
    let result = run_main(
        r#"
effect Check
  cell is_valid(x: Int) -> Bool
end

cell main() -> Bool / {Check}
  let result = handle
    perform Check.is_valid(42)
  with
    Check.is_valid(x) =>
      resume(true)
  end
  return result
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn e2e_effect_body_is_let_binding() {
    // Handle body that uses let binding before performing
    let result = run_main(
        r#"
effect Ask
  cell ask(prompt: String) -> String
end

cell main() -> String / {Ask}
  let result = handle
    let greeting = "Hello"
    let name = perform Ask.ask("name?")
    greeting
  with
    Ask.ask(prompt) =>
      resume("World")
  end
  return result
end
"#,
    );
    match &result {
        Value::String(StringRef::Owned(s)) => assert_eq!(s, "Hello"),
        other => panic!("expected String(\"Hello\"), got {:?}", other),
    }
}

// ─── For-loop continue (T199) ───

/// Basic continue in a for-loop: skip element 2, collect the rest.
#[test]
fn e2e_for_loop_continue_basic() {
    let result = run_main(
        r#"
cell main() -> list[Int]
  let mut results: list[Int] = []
  for x in [1, 2, 3, 4, 5]
    if x == 2
      continue
    end
    results = append(results, x)
  end
  return results
end
"#,
    );
    assert_eq!(
        result,
        Value::new_list(vec![
            Value::Int(1),
            Value::Int(3),
            Value::Int(4),
            Value::Int(5),
        ])
    );
}

/// Continue with filter: filter removes even numbers, continue skips 3.
#[test]
fn e2e_for_loop_continue_with_filter() {
    let result = run_main(
        r#"
cell main() -> list[Int]
  let mut results: list[Int] = []
  for x in [1, 2, 3, 4, 5, 6] if x % 2 != 0
    if x == 3
      continue
    end
    results = append(results, x)
  end
  return results
end
"#,
    );
    assert_eq!(result, Value::new_list(vec![Value::Int(1), Value::Int(5)]));
}

/// Continue in nested loops: inner continue should not affect outer loop.
#[test]
fn e2e_for_loop_continue_nested() {
    let result = run_main(
        r#"
cell main() -> list[Int]
  let mut results: list[Int] = []
  for i in [1, 2]
    for j in [10, 20, 30]
      if j == 20
        continue
      end
      results = append(results, i * 100 + j)
    end
  end
  return results
end
"#,
    );
    // i=1: j=10 -> 110, j=20 skip, j=30 -> 130
    // i=2: j=10 -> 210, j=20 skip, j=30 -> 230
    assert_eq!(
        result,
        Value::new_list(vec![
            Value::Int(110),
            Value::Int(130),
            Value::Int(210),
            Value::Int(230),
        ])
    );
}

/// Labeled continue: continue outer for-loop from inside inner loop.
#[test]
fn e2e_for_loop_labeled_continue() {
    let result = run_main(
        r#"
cell main() -> list[Int]
  let mut results: list[Int] = []
  for @outer i in [1, 2, 3]
    for j in [10, 20]
      if i == 2
        continue @outer
      end
      results = append(results, i * 100 + j)
    end
  end
  return results
end
"#,
    );
    // i=1: j=10 -> 110, j=20 -> 120
    // i=2: j=10 -> continue @outer (skip rest of i=2 entirely)
    // i=3: j=10 -> 310, j=20 -> 320
    assert_eq!(
        result,
        Value::new_list(vec![
            Value::Int(110),
            Value::Int(120),
            Value::Int(310),
            Value::Int(320),
        ])
    );
}

/// Continue as first statement in for-loop body (skip every element).
#[test]
fn e2e_for_loop_continue_first_statement() {
    let result = run_main(
        r#"
cell main() -> list[Int]
  let mut results: list[Int] = []
  for x in [1, 2, 3]
    continue
    results = append(results, x)
  end
  return results
end
"#,
    );
    // Every element is skipped, so the list stays empty.
    assert_eq!(result, Value::new_list(vec![]));
}

/// Continue with accumulator: sum all elements except the skipped one.
#[test]
fn e2e_for_loop_continue_with_accumulator() {
    let result = run_main(
        r#"
cell main() -> Int
  let mut sum = 0
  for x in [10, 20, 30, 40, 50]
    if x == 30
      continue
    end
    sum = sum + x
  end
  return sum
end
"#,
    );
    // 10 + 20 + 40 + 50 = 120 (skip 30)
    assert_eq!(result, Value::Int(120));
}

/// Multiple continues in a single for-loop body.
#[test]
fn e2e_for_loop_multiple_continues() {
    let result = run_main(
        r#"
cell main() -> list[Int]
  let mut results: list[Int] = []
  for x in [1, 2, 3, 4, 5, 6]
    if x == 2
      continue
    end
    if x == 5
      continue
    end
    results = append(results, x)
  end
  return results
end
"#,
    );
    assert_eq!(
        result,
        Value::new_list(vec![
            Value::Int(1),
            Value::Int(3),
            Value::Int(4),
            Value::Int(6),
        ])
    );
}

/// Continue in a for-loop with range-like list, verifying all elements after
/// the skipped one are still processed (regression: ensures iterator advances).
#[test]
fn e2e_for_loop_continue_no_infinite_loop() {
    let result = run_main(
        r#"
cell main() -> Int
  let mut count = 0
  for x in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    if x % 3 == 0
      continue
    end
    count = count + 1
  end
  return count
end
"#,
    );
    // Skip 3, 6, 9 => count 7 elements
    assert_eq!(result, Value::Int(7));
}
