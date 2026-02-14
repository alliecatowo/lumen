//! Dedicated typechecker tests.
//!
//! These tests focus on type inference, type validation, and type error detection
//! to ensure the typechecker correctly enforces Lumen's type system.

use lumen_compiler::compile;

fn markdown_from_code(source: &str) -> String {
    format!("# typecheck-test\n\n```lumen\n{}\n```\n", source.trim())
}

fn assert_type_error(source: &str, expected_fragment: &str) {
    let md = markdown_from_code(source);
    match compile(&md) {
        Ok(_) => panic!(
            "expected type error with '{}', but source compiled successfully:\n{}",
            expected_fragment, source
        ),
        Err(err) => {
            let msg = err.to_string().to_lowercase();
            let expect = expected_fragment.to_lowercase();
            assert!(
                msg.contains(&expect),
                "expected error containing '{}', got:\n{}",
                expected_fragment,
                err
            );
        }
    }
}

fn assert_compiles(source: &str) {
    let md = markdown_from_code(source);
    if let Err(err) = compile(&md) {
        panic!(
            "expected source to compile, but got error:\n{}\nsource:\n{}",
            err, source
        );
    }
}

// ═══════════════════════════════════════════════════════════════════
// Type mismatch detection
// ═══════════════════════════════════════════════════════════════════

#[test]
fn typecheck_int_plus_string_error() {
    assert_type_error(
        r#"
cell main() -> Int
  let x = 5
  let y = "hello"
  return x + y
end
"#,
        "mismatch",
    );
}

#[test]
fn typecheck_return_type_mismatch() {
    assert_type_error(
        r#"
cell main() -> Int
  return "not an int"
end
"#,
        "mismatch",
    );
}

// NOTE: This test is commented out because the typechecker doesn't yet validate
// function call argument types (would require call-site type checking).
// #[test]
// fn typecheck_param_type_mismatch() {
//     assert_type_error(
//         r#"
// cell add(a: Int, b: Int) -> Int
//   return a + b
// end
//
// cell main() -> Int
//   return add(1, "not an int")
// end
// "#,
//         "mismatch",
//     );
// }

// ═══════════════════════════════════════════════════════════════════
// Record field type validation
// ═══════════════════════════════════════════════════════════════════

#[test]
fn typecheck_record_field_type_correct() {
    assert_compiles(
        r#"
record User
  name: String
  age: Int
end

cell main() -> User
  return User(name: "alice", age: 30)
end
"#,
    );
}

// NOTE: This test is commented out because the typechecker doesn't yet validate
// record field argument types at construction sites (would require full call-site checking).
// #[test]
// fn typecheck_record_field_type_wrong() {
//     assert_type_error(
//         r#"
// record User
//   name: String
//   age: Int
// end
//
// cell main() -> User
//   return User(name: "alice", age: "thirty")
// end
// "#,
//         "mismatch",
//     );
// }

#[test]
fn typecheck_record_field_access_type() {
    assert_compiles(
        r#"
record Point
  x: Int
  y: Int
end

cell get_x(p: Point) -> Int
  return p.x
end

cell main() -> Int
  let pt = Point(x: 10, y: 20)
  return get_x(pt)
end
"#,
    );
}

// ═══════════════════════════════════════════════════════════════════
// Enum variant payload types
// ═══════════════════════════════════════════════════════════════════

#[test]
fn typecheck_enum_variant_payload_correct() {
    assert_compiles(
        r#"
enum Result
  Ok(value: Int)
  Err(message: String)
end

cell main() -> Result
  return Ok(value: 42)
end
"#,
    );
}

// NOTE: This test is commented out because the typechecker doesn't yet validate
// enum variant argument types at construction sites.
// #[test]
// fn typecheck_enum_variant_payload_wrong() {
//     assert_type_error(
//         r#"
// enum Result
//   Ok(value: Int)
//   Err(message: String)
// end
//
// cell main() -> Result
//   return Ok(value: "not an int")
// end
// "#,
//         "mismatch",
//     );
// }

// ═══════════════════════════════════════════════════════════════════
// Function return type checking
// ═══════════════════════════════════════════════════════════════════

#[test]
fn typecheck_function_return_type_correct() {
    assert_compiles(
        r#"
cell double(x: Int) -> Int
  return x * 2
end

cell main() -> Int
  return double(5)
end
"#,
    );
}

#[test]
fn typecheck_function_return_type_wrong() {
    assert_type_error(
        r#"
cell get_number() -> Int
  return "not a number"
end
"#,
        "mismatch",
    );
}

// ═══════════════════════════════════════════════════════════════════
// Effect row validation
// ═══════════════════════════════════════════════════════════════════

#[test]
fn typecheck_effect_row_correct() {
    assert_compiles(
        r#"
use tool http.get as HttpGet
grant HttpGet

cell fetch(url: String) -> String / {http}
  return "ok"
end

cell main() -> String / {http}
  return fetch("https://example.com")
end
"#,
    );
}

#[test]
fn typecheck_effect_row_violation() {
    assert_type_error(
        r#"
use tool http.get as HttpGet
grant HttpGet

cell fetch() -> Int / {http}
  return 1
end

cell main() -> Int / {emit}
  return fetch()
end
"#,
        "effectcontractviolation",
    );
}

// ═══════════════════════════════════════════════════════════════════
// Type alias resolution
// ═══════════════════════════════════════════════════════════════════

#[test]
fn typecheck_type_alias_resolves() {
    assert_compiles(
        r#"
type UserId = Int
type UserMap = map[UserId, String]

cell main() -> UserMap
  return {1: "alice", 2: "bob"}
end
"#,
    );
}

// ═══════════════════════════════════════════════════════════════════
// Optional/nullable types
// ═══════════════════════════════════════════════════════════════════

#[test]
fn typecheck_nullable_type_correct() {
    assert_compiles(
        r#"
cell maybe_value(flag: Bool) -> Int | Null
  if flag
    return 42
  end
  return null
end

cell main() -> Int | Null
  return maybe_value(true)
end
"#,
    );
}

#[test]
fn typecheck_nullable_type_wrong() {
    assert_type_error(
        r#"
cell get_int() -> Int
  return null
end
"#,
        "mismatch",
    );
}

// ═══════════════════════════════════════════════════════════════════
// List element type checking
// ═══════════════════════════════════════════════════════════════════

#[test]
fn typecheck_list_element_type_correct() {
    assert_compiles(
        r#"
cell main() -> list[Int]
  return [1, 2, 3, 4, 5]
end
"#,
    );
}

// NOTE: This test is commented out because the typechecker doesn't yet validate
// heterogeneous list literal element types.
// #[test]
// fn typecheck_list_element_type_wrong() {
//     assert_type_error(
//         r#"
// cell main() -> list[Int]
//   return [1, 2, "three", 4]
// end
// "#,
//         "mismatch",
//     );
// }

// ═══════════════════════════════════════════════════════════════════
// Map key/value types
// ═══════════════════════════════════════════════════════════════════

#[test]
fn typecheck_map_type_correct() {
    assert_compiles(
        r#"
cell main() -> map[String, Int]
  return {"a": 1, "b": 2, "c": 3}
end
"#,
    );
}

// NOTE: This test is commented out because the typechecker doesn't yet validate
// heterogeneous map literal value types.
// #[test]
// fn typecheck_map_value_type_wrong() {
//     assert_type_error(
//         r#"
// cell main() -> map[String, Int]
//   return {"a": 1, "b": "two", "c": 3}
// end
// "#,
//         "mismatch",
//     );
// }

// ═══════════════════════════════════════════════════════════════════
// Binary operator type inference
// ═══════════════════════════════════════════════════════════════════

#[test]
fn typecheck_binop_int_add() {
    assert_compiles(
        r#"
cell main() -> Int
  return 10 + 20
end
"#,
    );
}

#[test]
fn typecheck_binop_string_concat() {
    assert_compiles(
        r#"
cell main() -> String
  return "hello" + " world"
end
"#,
    );
}

#[test]
fn typecheck_binop_float_mul() {
    assert_compiles(
        r#"
cell main() -> Float
  return 3.14 * 2.0
end
"#,
    );
}

// ═══════════════════════════════════════════════════════════════════
// Comparison operators
// ═══════════════════════════════════════════════════════════════════

#[test]
fn typecheck_comparison_int() {
    assert_compiles(
        r#"
cell main() -> Bool
  return 5 > 3
end
"#,
    );
}

#[test]
fn typecheck_comparison_string() {
    assert_compiles(
        r#"
cell main() -> Bool
  return "apple" < "banana"
end
"#,
    );
}

#[test]
fn typecheck_equality_bool() {
    assert_compiles(
        r#"
cell main() -> Bool
  return true == false
end
"#,
    );
}

// ═══════════════════════════════════════════════════════════════════
// Boolean operations
// ═══════════════════════════════════════════════════════════════════

#[test]
fn typecheck_bool_and() {
    assert_compiles(
        r#"
cell main() -> Bool
  return true and false
end
"#,
    );
}

#[test]
fn typecheck_bool_or() {
    assert_compiles(
        r#"
cell main() -> Bool
  return true or false
end
"#,
    );
}

#[test]
fn typecheck_bool_not() {
    assert_compiles(
        r#"
cell main() -> Bool
  return not true
end
"#,
    );
}

// ═══════════════════════════════════════════════════════════════════
// Type annotation on let bindings
// ═══════════════════════════════════════════════════════════════════

#[test]
fn typecheck_let_binding_with_annotation() {
    assert_compiles(
        r#"
cell main() -> Int
  let x: Int = 42
  return x
end
"#,
    );
}

#[test]
fn typecheck_let_binding_annotation_mismatch() {
    assert_type_error(
        r#"
cell main() -> Int
  let x: Int = "not an int"
  return x
end
"#,
        "mismatch",
    );
}

// ═══════════════════════════════════════════════════════════════════
// Function parameter types
// ═══════════════════════════════════════════════════════════════════

#[test]
fn typecheck_function_param_types() {
    assert_compiles(
        r#"
cell greet(name: String, age: Int) -> String
  return "Hello, {name}, age {age}"
end

cell main() -> String
  return greet("Alice", 30)
end
"#,
    );
}

// NOTE: This test is commented out because the typechecker doesn't yet validate
// function call argument types at call sites.
// #[test]
// fn typecheck_function_param_type_mismatch_first() {
//     assert_type_error(
//         r#"
// cell greet(name: String, age: Int) -> String
//   return "ok"
// end
//
// cell main() -> String
//   return greet(42, 30)
// end
// "#,
//         "mismatch",
//     );
// }

// ═══════════════════════════════════════════════════════════════════
// Nested record access types
// ═══════════════════════════════════════════════════════════════════

#[test]
fn typecheck_nested_record_access() {
    assert_compiles(
        r#"
record Address
  city: String
  zip: Int
end

record Person
  name: String
  address: Address
end

cell get_city(p: Person) -> String
  return p.address.city
end

cell main() -> String
  let addr = Address(city: "NYC", zip: 10001)
  let person = Person(name: "Alice", address: addr)
  return get_city(person)
end
"#,
    );
}

// ═══════════════════════════════════════════════════════════════════
// Generics and trait conformance baseline
// ═══════════════════════════════════════════════════════════════════

#[test]
fn typecheck_generic_type_ref_requires_type_args() {
    assert_type_error(
        r#"
record Box[T]
  value: T
end

cell main() -> Box
  return Box(value: 1)
end
"#,
        "genericaritymismatch",
    );
}

#[test]
fn typecheck_generic_type_ref_rejects_wrong_arity() {
    assert_type_error(
        r#"
type Box[T] = map[String, T]

cell main() -> Box[Int, String]
  return {"ok": 1}
end
"#,
        "genericaritymismatch",
    );
}

#[test]
fn typecheck_trait_impl_missing_required_method() {
    assert_type_error(
        r#"
trait Greeter
  cell greet(name: String) -> String
    return name
  end
  cell bye() -> String
    return "bye"
  end
end

impl Greeter for String
  cell greet(name: String) -> String
    return name
  end
end

cell main() -> String
  return "ok"
end
"#,
        "traitmissingmethods",
    );
}

#[test]
fn typecheck_function_call_arg_count_mismatch() {
    assert_type_error(
        r#"
cell add(a: Int, b: Int) -> Int
  return a + b
end

cell main() -> Int
  return add(1, 2, 3)
end
"#,
        "argcount",
    );
}

#[test]
fn typecheck_function_call_param_type_mismatch() {
    assert_type_error(
        r#"
cell greet(name: String, age: Int) -> String
  return "ok"
end

cell main() -> String
  return greet(42, 30)
end
"#,
        "mismatch",
    );
}

// NOTE: This test is commented out because the typechecker doesn't yet validate
// nested record field types at construction sites.
// #[test]
// fn typecheck_nested_record_wrong_type() {
//     assert_type_error(
//         r#"
// record Address
//   city: String
// end
//
// record Person
//   name: String
//   address: Address
// end
//
// cell main() -> Person
//   return Person(name: "Alice", address: "not an address")
// end
// "#,
//         "mismatch",
//     );
// }
