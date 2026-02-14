//! Round 18 comprehensive test suite.
//!
//! Tests for: property shorthand, destructuring let, defer blocks,
//! stress tests for existing features, and edge cases.

use lumen_compiler::compile;

fn markdown(code: &str) -> String {
    format!("# test\n\n```lumen\n{}\n```\n", code.trim())
}

fn assert_ok(id: &str, code: &str) {
    let md = markdown(code);
    if let Err(err) = compile(&md) {
        panic!("case '{}' failed to compile:\n{}", id, err);
    }
}

fn assert_err(id: &str, code: &str, expect: &str) {
    let md = markdown(code);
    match compile(&md) {
        Ok(_) => panic!("case '{}' unexpectedly compiled", id),
        Err(err) => {
            let msg = err.to_string().to_lowercase();
            assert!(
                msg.contains(&expect.to_lowercase()),
                "case '{}' error mismatch\nexpected substring: {}\nactual: {}",
                id,
                expect,
                err
            );
        }
    }
}

// ============================================================================
// 1. Property shorthand — Point(x, y) compiles with matching variable names
// ============================================================================

#[test]
fn property_shorthand_basic() {
    assert_ok(
        "property_shorthand_basic",
        r#"
record Point
  x: Int
  y: Int
end

cell main() -> Int
  let x = 10
  let y = 20
  let p = Point(x, y)
  return p.x + p.y
end
"#,
    );
}

#[test]
fn property_shorthand_single_field() {
    assert_ok(
        "property_shorthand_single",
        r#"
record Wrapper
  value: Int
end

cell main() -> Int
  let value = 42
  let w = Wrapper(value)
  return w.value
end
"#,
    );
}

#[test]
fn property_shorthand_mixed_with_named() {
    assert_ok(
        "property_shorthand_mixed",
        r#"
record Config
  name: String
  count: Int
end

cell main() -> String
  let name = "test"
  let c = Config(name, count: 5)
  return c.name
end
"#,
    );
}

#[test]
fn property_shorthand_three_fields() {
    assert_ok(
        "property_shorthand_three",
        r#"
record Vec3
  x: Float
  y: Float
  z: Float
end

cell main() -> Float
  let x = 1.0
  let y = 2.0
  let z = 3.0
  let v = Vec3(x, y, z)
  return v.x + v.y + v.z
end
"#,
    );
}

#[test]
fn property_shorthand_with_record_defaults() {
    assert_ok(
        "property_shorthand_defaults",
        r#"
record Settings
  name: String = "default"
  verbose: Bool = false
  count: Int
end

cell main() -> Int
  let count = 10
  let s = Settings(count)
  return s.count
end
"#,
    );
}

// ============================================================================
// 2. Destructuring let — let (a, b) = expr
// NOTE: Destructuring let is parsed but lowering is in progress (task #2).
// Tests are commented out until lowering is wired.
// ============================================================================

// #[test]
// fn destructure_let_tuple_basic() {
//     assert_ok(
//         "destructure_tuple_basic",
//         r#"
// cell main() -> Int
//   let pair = (1, 2)
//   let (a, b) = pair
//   return a + b
// end
// "#,
//     );
// }

// #[test]
// fn destructure_let_tuple_three() {
//     assert_ok(
//         "destructure_tuple_three",
//         r#"
// cell main() -> Int
//   let triple = (10, 20, 30)
//   let (a, b, c) = triple
//   return a + b + c
// end
// "#,
//     );
// }

// #[test]
// fn destructure_let_with_wildcard() {
//     assert_ok(
//         "destructure_wildcard",
//         r#"
// cell main() -> Int
//   let pair = (1, 2)
//   let (_, b) = pair
//   return b
// end
// "#,
//     );
// }

// #[test]
// fn destructure_let_record() {
//     assert_ok(
//         "destructure_record",
//         r#"
// record Point
//   x: Int
//   y: Int
// end
//
// cell main() -> Int
//   let p = Point(x: 3, y: 4)
//   let Point(x:, y:) = p
//   return x + y
// end
// "#,
//     );
// }

// #[test]
// fn destructure_nested() {
//     assert_ok(
//         "destructure_nested",
//         r#"
// cell main() -> Int
//   let nested = ((1, 2), 3)
//   let ((a, b), c) = nested
//   return a + b + c
// end
// "#,
//     );
// }

// ============================================================================
// 3. Defer blocks — defer ... end compiles
// ============================================================================

#[test]
fn defer_basic() {
    assert_ok(
        "defer_basic",
        r#"
cell main() -> Int
  let mut x = 0
  defer
    x = x + 1
  end
  return x
end
"#,
    );
}

#[test]
fn defer_with_function_call() {
    assert_ok(
        "defer_with_call",
        r#"
cell cleanup(x: Int) -> Int
  return x
end

cell main() -> Int
  let mut result = 0
  defer
    result = cleanup(result)
  end
  result = 42
  return result
end
"#,
    );
}

#[test]
fn defer_multiple() {
    // Multiple defers should compile (runtime order: reverse)
    assert_ok(
        "defer_multiple",
        r#"
cell main() -> Int
  let mut x = 0
  defer
    x = x + 1
  end
  defer
    x = x * 2
  end
  return x
end
"#,
    );
}

#[test]
fn defer_empty_body_only_defer() {
    // A function whose body is just a defer block
    assert_ok(
        "defer_only",
        r#"
cell side_effect() -> Int
  defer
    let _ = 0
  end
  return 0
end
"#,
    );
}

#[test]
fn defer_with_if_inside() {
    assert_ok(
        "defer_with_if",
        r#"
cell main() -> Int
  let mut x = 0
  defer
    if x > 0
      x = x + 10
    end
  end
  x = 5
  return x
end
"#,
    );
}

// ============================================================================
// 4. Stress tests for existing features
// ============================================================================

// --- 4a. Nested match 3+ levels deep ---

#[test]
fn nested_match_three_levels() {
    assert_ok(
        "nested_match_3_levels",
        r#"
enum Outer
  A(Int)
  B
end

cell main() -> String
  let x = A(42)
  match x
    A(n) ->
      match n
        42 ->
          let msg = "found"
          match msg
            "found" -> return "deep match"
            _ -> return "nope"
          end
        _ -> return "not 42"
      end
    B -> return "B"
  end
end
"#,
    );
}

#[test]
fn nested_match_enum_inside_enum() {
    assert_ok(
        "nested_match_enum_in_enum",
        r#"
enum Inner
  X
  Y
end

enum Middle
  Wrap(Inner)
  Empty
end

enum Outer
  Box(Middle)
  Nil
end

cell classify(o: Outer) -> String
  match o
    Box(m) ->
      match m
        Wrap(i) ->
          match i
            X -> return "X"
            Y -> return "Y"
          end
        Empty -> return "empty"
      end
    Nil -> return "nil"
  end
end
"#,
    );
}

#[test]
fn nested_match_four_levels() {
    assert_ok(
        "nested_match_4_levels",
        r#"
cell deep(x: Int) -> String
  match x
    1 ->
      match x + 1
        2 ->
          match x + 2
            3 ->
              match x + 3
                4 -> return "four"
                _ -> return "?"
              end
            _ -> return "?"
          end
        _ -> return "?"
      end
    _ -> return "not one"
  end
end
"#,
    );
}

// --- 4b. Chained pipe operators ---

#[test]
fn pipe_chain_four_stages() {
    assert_ok(
        "pipe_chain_four",
        r#"
cell add_one(x: Int) -> Int
  return x + 1
end

cell double(x: Int) -> Int
  return x * 2
end

cell negate(x: Int) -> Int
  return 0 - x
end

cell to_str(x: Int) -> String
  return x as String
end

cell main() -> String
  return 5 |> add_one() |> double() |> negate() |> to_str()
end
"#,
    );
}

#[test]
fn pipe_with_multi_arg_functions() {
    assert_ok(
        "pipe_multi_arg",
        r#"
cell add(a: Int, b: Int) -> Int
  return a + b
end

cell mul(a: Int, b: Int) -> Int
  return a * b
end

cell main() -> Int
  return 10 |> add(5) |> mul(3)
end
"#,
    );
}

#[test]
fn pipe_chain_six_stages() {
    assert_ok(
        "pipe_chain_six",
        r#"
cell inc(x: Int) -> Int
  return x + 1
end

cell main() -> Int
  return 0 |> inc() |> inc() |> inc() |> inc() |> inc() |> inc()
end
"#,
    );
}

// --- 4c. Complex string interpolation ---

#[test]
fn interpolation_multiple_vars() {
    assert_ok(
        "interp_multi_vars",
        r#"
cell main() -> String
  let name = "Alice"
  let age = 30
  let city = "NYC"
  return "{name} is {age} and lives in {city}"
end
"#,
    );
}

#[test]
fn interpolation_with_arithmetic() {
    assert_ok(
        "interp_arithmetic",
        r#"
cell main() -> String
  let x = 10
  let y = 20
  return "sum is {x + y}"
end
"#,
    );
}

#[test]
fn interpolation_nested_in_function() {
    assert_ok(
        "interp_in_function",
        r#"
cell greet(name: String, greeting: String) -> String
  return "{greeting}, {name}!"
end

cell main() -> String
  return greet("World", "Hello")
end
"#,
    );
}

// --- 4d. Nested closures ---

#[test]
fn nested_closures_capture() {
    assert_ok(
        "nested_closures",
        r#"
cell main() -> Int
  let x = 10
  let f = fn(a: Int) -> Int
    let g = fn(b: Int) -> Int => a + b + x
    return g(5)
  end
  return f(20)
end
"#,
    );
}

#[test]
fn nested_closures_three_levels() {
    assert_ok(
        "nested_closures_3",
        r#"
cell main() -> Int
  let a = 1
  let f = fn() -> Int
    let b = 2
    let g = fn() -> Int
      let c = 3
      let h = fn() -> Int => a + b + c
      return h()
    end
    return g()
  end
  return f()
end
"#,
    );
}

// --- 4e. Multiple labeled loops ---

#[test]
fn three_labeled_loops() {
    assert_ok(
        "three_labeled_loops",
        r#"
cell main() -> Int
  let mut result = 0
  for @outer i in [1, 2, 3]
    for @middle j in [10, 20, 30]
      for @inner k in [100, 200, 300]
        if k == 200 and j == 20
          result = i + j + k
          break @outer
        end
      end
    end
  end
  return result
end
"#,
    );
}

#[test]
fn labeled_loops_mixed_types() {
    assert_ok(
        "labeled_mixed",
        r#"
cell main() -> Int
  let mut sum = 0
  let mut i = 0
  while @outer i < 5
    i = i + 1
    for @inner x in [1, 2, 3]
      if x == 2 and i == 3
        break @outer
      end
      sum = sum + x
    end
  end
  return sum
end
"#,
    );
}

// --- 4f. Compound assignments in loops ---

#[test]
fn compound_assign_all_in_loop() {
    assert_ok(
        "compound_in_loop",
        r#"
cell main() -> Int
  let mut x = 100
  for i in [1, 2, 3, 4, 5]
    x += i
    x -= 1
    x *= 1
  end
  return x
end
"#,
    );
}

#[test]
fn compound_assign_with_accumulator() {
    assert_ok(
        "compound_accumulator",
        r#"
cell main() -> Int
  let mut product = 1
  let mut sum = 0
  for x in [2, 3, 4]
    product *= x
    sum += x
  end
  return product + sum
end
"#,
    );
}

// --- 4g. is/as with types ---

#[test]
fn is_as_chain() {
    assert_ok(
        "is_as_chain",
        r#"
cell convert(x: Int) -> String
  if x is Int
    let f = x as Float
    let s = f as String
    return s
  end
  return "not int"
end
"#,
    );
}

#[test]
fn is_with_enum_variant() {
    assert_ok(
        "is_enum",
        r#"
enum Shape
  Circle(Float)
  Square(Float)
end

cell describe(s: Shape) -> String
  match s
    Circle(r) ->
      if r is Float
        return "circle with radius"
      end
      return "circle"
    Square(side) -> return "square"
  end
end
"#,
    );
}

// --- 4h. Exhaustiveness with many variants ---

#[test]
fn exhaustive_ten_variants() {
    assert_ok(
        "exhaustive_10",
        r#"
enum Color
  Red
  Orange
  Yellow
  Green
  Blue
  Indigo
  Violet
  White
  Black
  Gray
end

cell name(c: Color) -> String
  match c
    Red -> return "red"
    Orange -> return "orange"
    Yellow -> return "yellow"
    Green -> return "green"
    Blue -> return "blue"
    Indigo -> return "indigo"
    Violet -> return "violet"
    White -> return "white"
    Black -> return "black"
    Gray -> return "gray"
  end
end
"#,
    );
}

#[test]
fn exhaustive_with_wildcard_covers_rest() {
    assert_ok(
        "exhaustive_wildcard_rest",
        r#"
enum Direction
  North
  South
  East
  West
  NorthEast
  NorthWest
  SouthEast
  SouthWest
end

cell is_north(d: Direction) -> Bool
  match d
    North -> return true
    NorthEast -> return true
    NorthWest -> return true
    _ -> return false
  end
end
"#,
    );
}

// --- 4i. For-loop with filter + break + continue ---

#[test]
fn for_filter_break_continue() {
    assert_ok(
        "for_filter_break_continue",
        r#"
cell main() -> Int
  let mut sum = 0
  for x in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] if x > 2
    if x == 5
      continue
    end
    if x >= 8
      break
    end
    sum += x
  end
  return sum
end
"#,
    );
}

#[test]
fn for_filter_with_labeled_break() {
    assert_ok(
        "for_filter_labeled_break",
        r#"
cell main() -> Int
  let mut result = 0
  for @search x in [10, 20, 30, 40, 50] if x > 15
    for @inner y in [1, 2, 3]
      if x == 30 and y == 2
        result = x + y
        break @search
      end
    end
  end
  return result
end
"#,
    );
}

// --- 4j. Shift operators in complex expressions ---

#[test]
fn shift_in_complex_expr() {
    assert_ok(
        "shift_complex",
        r#"
cell main() -> Int
  let flags = (1 << 0) + (1 << 2) + (1 << 4)
  let mask = flags >> 1
  return flags + mask
end
"#,
    );
}

#[test]
fn shift_with_variables_and_ops() {
    assert_ok(
        "shift_vars_ops",
        r#"
cell bit_set(flags: Int, bit: Int) -> Int
  return flags + (1 << bit)
end

cell bit_test(flags: Int, bit: Int) -> Bool
  return (flags >> bit) % 2 == 1
end

cell main() -> Int
  let mut f = 0
  f = bit_set(f, 3)
  f = bit_set(f, 5)
  if bit_test(f, 3)
    return f
  end
  return 0
end
"#,
    );
}

// --- 4k. Mix of all operators together ---

#[test]
fn operator_mix_arithmetic() {
    assert_ok(
        "op_mix_arith",
        r#"
cell main() -> Int
  let a = 10
  let b = 3
  let sum = a + b
  let diff = a - b
  let prod = a * b
  let quot = a / b
  let flr = a // b
  let rem = a % b
  let pow = 2 ** 4
  return sum + diff + prod + quot + flr + rem + pow
end
"#,
    );
}

#[test]
fn operator_mix_comparison_logical() {
    assert_ok(
        "op_mix_cmp_log",
        r#"
cell main() -> Bool
  let x = 5
  let y = 10
  let a = x > 3 and x < 10
  let b = y >= 10 or y <= 0
  let c = x != y and not (x == y)
  return a and b and c
end
"#,
    );
}

#[test]
fn operator_mix_bitwise_and_shift() {
    assert_ok(
        "op_mix_bitshift",
        r#"
cell main() -> Int
  let x = 1 << 4
  let y = x >> 2
  let z = x + y
  let w = z % 7
  return w ** 2
end
"#,
    );
}

// ============================================================================
// 5. Edge cases
// ============================================================================

// --- 5a. Single-variant enum in match ---

#[test]
fn single_variant_enum() {
    assert_ok(
        "single_variant",
        r#"
enum Unit
  Only
end

cell check(u: Unit) -> String
  match u
    Only -> return "only"
  end
end
"#,
    );
}

// --- 5b. Record with all fields having defaults ---

#[test]
fn record_all_defaults() {
    assert_ok(
        "record_all_defaults",
        r#"
record Defaults
  a: Int = 0
  b: String = "hello"
  c: Bool = true
end

cell main() -> Int
  let d = Defaults()
  return d.a
end
"#,
    );
}

// --- 5c. Break/continue without labels inside labeled loops ---

#[test]
fn unlabeled_break_in_labeled_loop() {
    assert_ok(
        "unlabeled_in_labeled",
        r#"
cell main() -> Int
  let mut count = 0
  for @outer i in [1, 2, 3]
    for j in [10, 20, 30]
      if j == 20
        break
      end
      count += 1
    end
  end
  return count
end
"#,
    );
}

#[test]
fn unlabeled_continue_in_labeled_loop() {
    assert_ok(
        "unlabeled_continue_in_labeled",
        r#"
cell main() -> Int
  let mut sum = 0
  for @outer i in [1, 2, 3]
    for j in [1, 2, 3]
      if j == 2
        continue
      end
      sum += j
    end
  end
  return sum
end
"#,
    );
}

// --- 5d. Empty for-loop body ---

#[test]
fn empty_for_body() {
    assert_ok(
        "empty_for_body",
        r#"
cell main() -> Int
  for x in [1, 2, 3]
    let _ = x
  end
  return 0
end
"#,
    );
}

// --- 5e. Deeply nested optional types ---

#[test]
fn double_optional() {
    assert_ok(
        "double_optional",
        r#"
cell main() -> Int
  let x: Int? = null
  let y = x ?? 42
  return y
end
"#,
    );
}

// --- 5f. Complex if-else chain ---

#[test]
fn complex_if_else_chain() {
    assert_ok(
        "complex_if_else",
        r#"
cell classify(x: Int) -> String
  if x > 100
    return "huge"
  else if x > 50
    return "large"
  else if x > 25
    return "medium"
  else if x > 10
    return "small"
  else if x > 0
    return "tiny"
  else if x == 0
    return "zero"
  else
    return "negative"
  end
end
"#,
    );
}

// --- 5g. Recursive with multiple base cases ---

#[test]
fn recursive_multiple_bases() {
    assert_ok(
        "recursive_multi_base",
        r#"
cell fib(n: Int) -> Int
  if n <= 0
    return 0
  else if n == 1
    return 1
  end
  return fib(n - 1) + fib(n - 2)
end
"#,
    );
}

// --- 5h. Lambda as argument ---

#[test]
fn lambda_as_argument() {
    assert_ok(
        "lambda_as_arg",
        r#"
cell apply(x: Int, f: fn(Int) -> Int) -> Int
  return f(x)
end

cell main() -> Int
  return apply(5, fn(x: Int) -> Int => x * 2)
end
"#,
    );
}

// --- 5i. Closure capturing mutable variable ---

#[test]
fn closure_captures_mutable() {
    assert_ok(
        "closure_mut_capture",
        r#"
cell main() -> Int
  let mut counter = 0
  let inc = fn() -> Int
    counter = counter + 1
    return counter
  end
  let a = inc()
  let b = inc()
  return a + b
end
"#,
    );
}

// --- 5j. Multiple match arms with same body pattern ---

#[test]
fn match_multiple_wildcard_patterns() {
    assert_ok(
        "match_multi_wild",
        r#"
enum Token
  Plus
  Minus
  Star
  Slash
  LParen
  RParen
end

cell is_operator(t: Token) -> Bool
  match t
    Plus -> return true
    Minus -> return true
    Star -> return true
    Slash -> return true
    _ -> return false
  end
end
"#,
    );
}

// --- 5k. Comprehension with complex transform ---

#[test]
fn comprehension_complex() {
    assert_ok(
        "comprehension_complex",
        r#"
cell double(x: Int) -> Int
  return x * 2
end

cell main() -> list[Int]
  return [double(x) for x in [1, 2, 3, 4, 5] if x > 2]
end
"#,
    );
}

// --- 5l. Null coalescing chain ---

#[test]
fn null_coalesce_deep_chain() {
    assert_ok(
        "null_coalesce_deep",
        r#"
cell main() -> Int
  let a: Int? = null
  let b: Int? = null
  let c: Int? = null
  let d: Int? = null
  let e: Int? = 99
  return a ?? b ?? c ?? d ?? e ?? 0
end
"#,
    );
}

// --- 5m. Map with string keys and access ---

#[test]
fn map_with_operations() {
    assert_ok(
        "map_operations",
        r#"
cell main() -> map[String, Int]
  let m = {"x": 1, "y": 2, "z": 3}
  return m
end
"#,
    );
}

// --- 5n. Set literal ---

#[test]
fn set_literal_multiple_elements() {
    assert_ok(
        "set_multi",
        r#"
cell main() -> set[Int]
  return {1, 2, 3, 4, 5}
end
"#,
    );
}

// --- 5o. Range in various contexts ---

#[test]
fn range_in_for_with_filter() {
    assert_ok(
        "range_for_filter",
        r#"
cell main() -> Int
  let mut sum = 0
  for x in 1..=100 if x % 2 == 0
    sum += x
  end
  return sum
end
"#,
    );
}

// --- 5p. Exponentiation precedence ---

#[test]
fn power_precedence() {
    assert_ok(
        "power_prec",
        r#"
cell main() -> Int
  let a = 2 ** 3 + 1
  let b = 1 + 2 ** 3
  return a + b
end
"#,
    );
}

// --- 5q. Enum with payload in match ---

#[test]
fn enum_payload_types() {
    assert_ok(
        "enum_payload_types",
        r#"
enum Value
  IntVal(Int)
  FloatVal(Float)
  StrVal(String)
  BoolVal(Bool)
  NullVal
end

cell describe(v: Value) -> String
  match v
    IntVal(n) -> return "int"
    FloatVal(f) -> return "float"
    StrVal(s) -> return s
    BoolVal(b) -> return "bool"
    NullVal -> return "null"
  end
end
"#,
    );
}

// --- 5r. If-let with else ---

#[test]
fn if_let_with_else() {
    assert_ok(
        "if_let_else",
        r#"
enum Maybe
  Some(Int)
  None
end

cell unwrap_or(m: Maybe, default: Int) -> Int
  if let Some(v) = m
    return v
  else
    return default
  end
end
"#,
    );
}

// --- 5s. While loop with complex condition ---

#[test]
fn while_complex_condition() {
    assert_ok(
        "while_complex_cond",
        r#"
cell main() -> Int
  let mut x = 0
  let mut y = 100
  while x < 50 and y > 50
    x = x + 1
    y = y - 1
  end
  return x + y
end
"#,
    );
}

// --- 5t. Multiple cells calling each other ---

#[test]
fn multi_cell_chain() {
    assert_ok(
        "multi_cell_chain",
        r#"
cell step1(x: Int) -> Int
  return x + 1
end

cell step2(x: Int) -> Int
  return step1(x) * 2
end

cell step3(x: Int) -> Int
  return step2(x) + step1(x)
end

cell main() -> Int
  return step3(10)
end
"#,
    );
}

// --- 5u. Spread in list ---

#[test]
fn spread_in_list_with_elements() {
    assert_ok(
        "spread_mixed",
        r#"
cell main() -> list[Int]
  let xs = [2, 3, 4]
  return [1, ...xs, 5]
end
"#,
    );
}

// --- 5v. Record field access chain ---

#[test]
fn record_field_chain() {
    assert_ok(
        "record_chain",
        r#"
record Inner
  value: Int
end

record Outer
  inner: Inner
end

cell main() -> Int
  let o = Outer(inner: Inner(value: 42))
  return o.inner.value
end
"#,
    );
}

// --- 5w. Type alias with complex type ---

#[test]
fn type_alias_complex() {
    assert_ok(
        "type_alias_complex",
        r#"
type StringList = list[String]

cell count(items: StringList) -> Int
  let mut n = 0
  for s in items
    n += 1
  end
  return n
end
"#,
    );
}

// --- 5x. Implicit return with match expression ---

#[test]
fn implicit_return_match_expr() {
    assert_ok(
        "implicit_return_match",
        r#"
enum Color
  Red
  Blue
end

cell name(c: Color) -> String
  match c
    Red -> return "red"
    Blue -> return "blue"
  end
end
"#,
    );
}

// --- 5y. Null-safe chaining ---

#[test]
fn null_safe_field_access() {
    assert_ok(
        "null_safe_field",
        r#"
record Point
  x: Int
  y: Int
end

cell get_x(p: Point) -> Int
  let val = p?.x
  return val ?? 0
end
"#,
    );
}

// --- 5z. Floor division with compound assignment ---

#[test]
fn floor_div_compound_in_loop() {
    assert_ok(
        "floor_div_compound_loop",
        r#"
cell main() -> Int
  let mut x = 1000
  for i in [2, 3, 4, 5]
    x //= i
  end
  return x
end
"#,
    );
}

// ============================================================================
// 6. Error case tests
// ============================================================================

#[test]
fn err_undefined_in_match() {
    assert_err(
        "undef_in_match",
        r#"
cell main() -> Int
  match undefined_var
    _ -> return 0
  end
end
"#,
        "undefined",
    );
}

#[test]
fn err_type_mismatch_in_binop() {
    assert_err(
        "type_mismatch_binop",
        r#"
cell main() -> Int
  return "hello" + 42
end
"#,
        "type",
    );
}

#[test]
fn err_shift_requires_int() {
    assert_err(
        "shift_type_err",
        r#"
cell main() -> Int
  return "hello" << 2
end
"#,
        "type",
    );
}

// ============================================================================
// 7. Comprehensive combinations
// ============================================================================

#[test]
fn combo_pipe_match_closure() {
    assert_ok(
        "combo_pipe_match_closure",
        r#"
cell double(x: Int) -> Int
  return x * 2
end

cell classify(x: Int) -> String
  match x
    0 -> return "zero"
    _ -> return "nonzero"
  end
end

cell main() -> String
  let result = 5 |> double()
  return classify(result)
end
"#,
    );
}

#[test]
fn combo_loop_closure_compound() {
    assert_ok(
        "combo_loop_closure_compound",
        r#"
cell main() -> Int
  let mut total = 0
  let adder = fn(x: Int) -> Int => x + 10
  for i in 1..=5
    total += adder(i)
  end
  return total
end
"#,
    );
}

#[test]
fn combo_record_match_pipe() {
    assert_ok(
        "combo_record_match_pipe",
        r#"
record Point
  x: Int
  y: Int
end

cell magnitude(p: Point) -> Int
  return p.x * p.x + p.y * p.y
end

cell classify(n: Int) -> String
  if n > 100
    return "far"
  else
    return "near"
  end
end

cell main() -> String
  let p = Point(x: 6, y: 8)
  return p |> magnitude() |> classify()
end
"#,
    );
}

#[test]
fn combo_optional_match_closure() {
    assert_ok(
        "combo_optional_match_closure",
        r#"
cell safe_div(a: Int, b: Int) -> Int?
  if b == 0
    return null
  end
  return a / b
end

cell main() -> Int
  let result = safe_div(10, 2)
  return result ?? 0
end
"#,
    );
}

#[test]
fn combo_all_loop_types() {
    assert_ok(
        "combo_all_loops",
        r#"
cell main() -> Int
  let mut total = 0

  # for loop
  for x in [1, 2, 3]
    total += x
  end

  # while loop
  let mut i = 0
  while i < 3
    total += 1
    i += 1
  end

  # loop with break
  let mut j = 0
  loop
    j += 1
    if j > 2
      break
    end
    total += j
  end

  # for with filter
  for y in [10, 20, 30] if y > 15
    total += 1
  end

  # for with range
  for z in 1..4
    total += z
  end

  return total
end
"#,
    );
}

#[test]
fn combo_defer_and_match() {
    assert_ok(
        "combo_defer_match",
        r#"
enum Status
  Ok
  Err
end

cell process(s: Status) -> String
  defer
    let _ = 0
  end
  match s
    Ok -> return "success"
    Err -> return "failure"
  end
end
"#,
    );
}

#[test]
fn combo_property_shorthand_and_pipe() {
    assert_ok(
        "combo_shorthand_pipe",
        r#"
record Pair
  a: Int
  b: Int
end

cell sum_pair(p: Pair) -> Int
  return p.a + p.b
end

cell main() -> Int
  let a = 10
  let b = 20
  return Pair(a, b) |> sum_pair()
end
"#,
    );
}
