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
