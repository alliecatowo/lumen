//! Wave-20 tests: T186 — validate builtin (runtime schema validation).

use lumen_compiler::compile;
use lumen_rt::values::Value;
use lumen_rt::vm::VM;

/// Helper: wrap raw Lumen code in markdown, compile, run `main`, return the result.
fn run_main(source: &str) -> Value {
    let md = format!(
        "# wave20-validate-test\n\n```lumen\n{}\n```\n",
        source.trim()
    );
    let module = compile(&md).expect("source should compile");
    let mut vm = VM::new();
    vm.load(module);
    vm.execute("main", vec![]).expect("main should execute")
}

// ─── Single-arg validate: not-null check ───

#[test]
fn validate_single_arg_non_null() {
    let result = run_main(
        r#"
cell main() -> Bool
  return validate(42)
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn validate_single_arg_string() {
    let result = run_main(
        r#"
cell main() -> Bool
  return validate("hello")
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn validate_single_arg_null() {
    let result = run_main(
        r#"
cell main() -> Bool
  return validate(null)
end
"#,
    );
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn validate_single_arg_bool() {
    let result = run_main(
        r#"
cell main() -> Bool
  return validate(true)
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn validate_single_arg_false_still_not_null() {
    let result = run_main(
        r#"
cell main() -> Bool
  return validate(false)
end
"#,
    );
    // false is not null, so validate(false) == true
    assert_eq!(result, Value::Bool(true));
}

// ─── Two-arg validate: type schema ───

#[test]
fn validate_int_against_int_schema() {
    let result = run_main(
        r#"
cell main() -> Bool
  return validate(42, "Int")
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn validate_int_against_string_schema() {
    let result = run_main(
        r#"
cell main() -> Bool
  return validate(42, "String")
end
"#,
    );
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn validate_string_against_string_schema() {
    let result = run_main(
        r#"
cell main() -> Bool
  return validate("hello", "String")
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn validate_bool_against_bool_schema() {
    let result = run_main(
        r#"
cell main() -> Bool
  return validate(true, "Bool")
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn validate_float_against_float_schema() {
    let result = run_main(
        r#"
cell main() -> Bool
  return validate(3.14, "Float")
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn validate_float_against_int_schema() {
    let result = run_main(
        r#"
cell main() -> Bool
  return validate(3.14, "Int")
end
"#,
    );
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn validate_null_against_null_schema() {
    let result = run_main(
        r#"
cell main() -> Bool
  return validate(null, "Null")
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn validate_anything_against_any_schema() {
    let result = run_main(
        r#"
cell main() -> Bool
  let a = validate(42, "Any")
  let b = validate("hello", "Any")
  let c = validate(null, "Any")
  return a and b and c
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

// ─── List type validation ───

#[test]
fn validate_list_against_list_schema() {
    let result = run_main(
        r#"
cell main() -> Bool
  let items = [1, 2, 3]
  return validate(items, "List")
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn validate_non_list_against_list_schema() {
    let result = run_main(
        r#"
cell main() -> Bool
  return validate(42, "List")
end
"#,
    );
    assert_eq!(result, Value::Bool(false));
}

// ─── Map schema validation ───

#[test]
fn validate_map_against_map_schema() {
    let result = run_main(
        r#"
cell main() -> Bool
  let data = {"name": "Alice", "age": 30}
  let spec = {"name": "String", "age": "Int"}
  return validate(data, spec)
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn validate_map_missing_field() {
    let result = run_main(
        r#"
cell main() -> Bool
  let data = {"name": "Alice"}
  let spec = {"name": "String", "age": "Int"}
  return validate(data, spec)
end
"#,
    );
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn validate_map_wrong_type() {
    let result = run_main(
        r#"
cell main() -> Bool
  let data = {"name": "Alice", "age": "thirty"}
  let spec = {"name": "String", "age": "Int"}
  return validate(data, spec)
end
"#,
    );
    assert_eq!(result, Value::Bool(false));
}

// ─── Empty values ───

#[test]
fn validate_empty_list() {
    let result = run_main(
        r#"
cell main() -> Bool
  let items: list[Int] = []
  return validate(items, "List")
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}

// ─── validate with map type ───

#[test]
fn validate_non_map_against_map_type() {
    let result = run_main(
        r#"
cell main() -> Bool
  return validate("not a map", "Map")
end
"#,
    );
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn validate_map_against_map_type() {
    let result = run_main(
        r#"
cell main() -> Bool
  let data = {"key": "value"}
  return validate(data, "Map")
end
"#,
    );
    assert_eq!(result, Value::Bool(true));
}
