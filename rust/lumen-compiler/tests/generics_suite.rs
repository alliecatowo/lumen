//! Comprehensive test suite for generic type system implementation

use lumen_compiler::compile_raw;

fn compile_ok(src: &str) {
    match compile_raw(src) {
        Ok(_) => {}
        Err(e) => panic!("Expected successful compilation, got error:\n{}", e),
    }
}

fn compile_err(src: &str) -> String {
    match compile_raw(src) {
        Ok(_) => panic!("Expected compilation error, but succeeded"),
        Err(e) => format!("{}", e),
    }
}

#[test]
fn test_generic_record_one_param() {
    compile_ok(
        "
record Box[T]
  value: T
end

cell main() -> Int
  let b = Box(value: 42)
  return b.value
end
",
    );
}

#[test]
fn test_generic_record_two_params() {
    compile_ok(
        "
record Pair[A, B]
  first: A
  second: B
end

cell main() -> String
  let p = Pair(first: 1, second: \"hello\")
  return p.second
end
",
    );
}

#[test]
fn test_generic_record_explicit_instantiation() {
    compile_ok(
        "
record Box[T]
  value: T
end

cell make_int_box() -> Box[Int]
  return Box(value: 42)
end

cell make_string_box() -> Box[String]
  return Box(value: \"hello\")
end
",
    );
}

#[test]
fn test_generic_record_field_type_checked() {
    let err = compile_err(
        "
record Box[T]
  value: T
end

cell bad() -> Box[Int]
  return Box(value: \"not an int\")
end
",
    );
    // Should report type mismatch between Box[Int] and Box[String]
    assert!(
        (err.contains("Box[Int]") && err.contains("Box[String]"))
            || err.contains("type mismatch")
            || err.contains("expected") && err.contains("Int")
    );
}

#[test]
fn test_generic_enum_one_param() {
    compile_ok(
        "
enum Option[T]
  some(T)
  none
end

cell get_value(opt: Option[Int]) -> Int
  match opt
    some(x) -> return x
    none -> return 0
  end
end
",
    );
}

#[test]
fn test_generic_enum_two_params() {
    compile_ok(
        "
enum Either[L, R]
  left(L)
  right(R)
end

cell process(e: Either[Int, String]) -> String
  match e
    left(n) -> return to_string(n)
    right(s) -> return s
  end
end
",
    );
}

#[test]
fn test_builtin_list_generic() {
    compile_ok(
        "
cell sum(nums: list[Int]) -> Int
  let total = 0
  for n in nums
    total = total + n
  end
  return total
end
",
    );
}

#[test]
fn test_builtin_map_generic() {
    compile_ok(
        "
cell lookup(m: map[String, Int], key: String) -> Int
  return m[key]
end
",
    );
}

#[test]
fn test_builtin_result_generic() {
    compile_ok(
        "
cell divide(a: Int, b: Int) -> result[Int, String]
  if b == 0
    return err(\"division by zero\")
  end
  return ok(a / b)
end
",
    );
}

#[test]
fn test_builtin_set_generic() {
    compile_ok(
        "
cell contains_elem(s: set[String], elem: String) -> Bool
  return elem in s
end
",
    );
}

#[test]
fn test_generic_function_one_param() {
    compile_ok(
        "
cell identity[T](x: T) -> T
  return x
end

cell main() -> Int
  return identity(42)
end
",
    );
}

#[test]
fn test_generic_function_two_params() {
    compile_ok(
        "
cell first[A, B](a: A, b: B) -> A
  return a
end

cell main() -> Int
  return first(5, \"hello\")
end
",
    );
}

#[test]
fn test_generic_function_with_list() {
    compile_ok(
        "
cell head[T](items: list[T]) -> T
  return items[0]
end

cell main() -> Int
  return head([1, 2, 3])
end
",
    );
}

#[test]
fn test_wrong_generic_arity_zero_expected_one() {
    let err = compile_err(
        "
record Box[T]
  value: T
end

cell bad() -> Box
  return Box(value: 1)
end
",
    );
    assert!(
        err.contains("GenericArityMismatch") && err.contains("expected: 1"),
        "Error should be GenericArityMismatch with expected: 1. Got: {}",
        err
    );
}

#[test]
fn test_wrong_generic_arity_one_expected_zero() {
    let err = compile_err(
        "
record Simple
  x: Int
end

cell bad() -> Simple[Int]
  return Simple(x: 1)
end
",
    );
    assert!(
        err.contains("GenericArityMismatch") && err.contains("expected: 0"),
        "Error should be GenericArityMismatch with expected: 0. Got: {}",
        err
    );
}

#[test]
fn test_wrong_generic_arity_one_expected_two() {
    let err = compile_err(
        "
record Pair[A, B]
  first: A
  second: B
end

cell bad() -> Pair[Int]
  return Pair(first: 1, second: 2)
end
",
    );
    assert!(
        err.contains("GenericArityMismatch") && err.contains("expected: 2"),
        "Error should be GenericArityMismatch with expected: 2. Got: {}",
        err
    );
}

#[test]
fn test_generic_type_param_cannot_be_resolved() {
    // T is not bound in this context - should error
    let err = compile_err(
        "
cell bad() -> T
  return 1
end
",
    );
    assert!(err.contains("undefined") || err.contains("unresolved") || err.contains("T"));
}

#[test]
fn test_nested_generics() {
    compile_ok(
        "
record Box[T]
  value: T
end

cell nested() -> Box[list[Int]]
  return Box(value: [1, 2, 3])
end
",
    );
}

#[test]
fn test_generic_record_in_list() {
    compile_ok(
        "
record Box[T]
  value: T
end

cell make_boxes() -> list[Box[Int]]
  return [Box(value: 1), Box(value: 2)]
end
",
    );
}

#[test]
fn test_generic_type_alias() {
    compile_ok(
        "
type IntBox = Box[Int]

record Box[T]
  value: T
end

cell make() -> IntBox
  return Box(value: 42)
end
",
    );
}

#[test]
fn test_generic_with_constraint() {
    compile_ok(
        "
record Box[T]
  value: T where value > 0
end

cell make() -> Box[Int]
  return Box(value: 42)
end
",
    );
}

#[test]
fn test_multiple_generic_types_in_signature() {
    compile_ok(
        "
record Box[T]
  value: T
end

cell swap[A, B](a: Box[A], b: Box[B]) -> Box[B]
  return b
end
",
    );
}
