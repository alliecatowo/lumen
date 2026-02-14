//! Comprehensive stress tests combining all implemented features.
//!
//! Tests cover: complex integration (labeled loop + filter + match + defer + pipe),
//! deep nesting (5+ levels), all operators, pattern matching edge cases,
//! closures and captures, process types, error cases, range patterns,
//! is/as expressions, and exhaustiveness with many variants.

use lumen_compiler::compile;

fn markdown(code: &str) -> String {
    format!("# test\n\n```lumen\n{}\n```\n", code.trim())
}

fn assert_ok(id: &str, code: &str) {
    let md = markdown(code);
    if let Err(err) = compile(&md) {
        panic!(
            "case '{}' failed to compile\n--- source ---\n{}\n--- error ---\n{}",
            id, code, err
        );
    }
}

fn assert_err(id: &str, code: &str, expect: &str) {
    let md = markdown(code);
    match compile(&md) {
        Ok(_) => panic!(
            "case '{}' unexpectedly compiled\n--- source ---\n{}",
            id, code
        ),
        Err(err) => {
            let msg = err.to_string().to_lowercase();
            let expect_lower = expect.to_lowercase();
            assert!(
                msg.contains(&expect_lower),
                "case '{}' error mismatch\nexpected substring: {}\nactual: {}",
                id,
                expect,
                err
            );
        }
    }
}

// ============================================================================
// 1. Complex Integration Tests
// ============================================================================

#[test]
fn stress_labeled_loop_filter_match_pipe() {
    assert_ok(
        "labeled_loop_filter_match_pipe",
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
  let mut best = 0
  for @search x in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] if x % 2 == 0
    let doubled = x |> double()
    if doubled > 12
      best = doubled
      break @search
    end
  end
  return classify(best)
end
"#,
    );
}

#[test]
fn stress_defer_loop_closure_compound() {
    assert_ok(
        "defer_loop_closure_compound",
        r#"
cell main() -> Int
  let mut total = 0
  defer
    total += 1
  end
  let adder = fn(a: Int, b: Int) -> Int => a + b
  for @outer i in [1, 2, 3]
    for @inner j in [10, 20, 30] if j > 10
      total += adder(i, j)
      if total > 50
        break @outer
      end
    end
  end
  return total
end
"#,
    );
}

#[test]
fn stress_pipe_match_optional_is_as() {
    assert_ok(
        "pipe_match_optional_is_as",
        r#"
cell to_float(x: Int) -> Float
  return x as Float
end

cell to_int(x: Float) -> Int
  return x as Int
end

cell safe_lookup(items: list[Int], idx: Int) -> Int?
  if idx >= 0
    return items?[idx]
  end
  return null
end

cell main() -> Int
  let items = [10, 20, 30, 40, 50]
  let result = safe_lookup(items, 2)
  let val = result ?? 0
  if val is Int
    return val |> to_float() |> to_int()
  end
  return 0
end
"#,
    );
}

#[test]
fn stress_match_inside_labeled_for_with_filter() {
    assert_ok(
        "match_inside_labeled_for_filter",
        r#"
enum Status
  Active
  Inactive
  Pending
end

cell main() -> Int
  let mut count = 0
  let statuses = [Active, Inactive, Pending, Active, Pending]
  for @scan s in statuses if s != Inactive
    match s
      Active -> count += 2
      Pending -> count += 1
      Inactive -> count += 0
    end
    if count >= 5
      break @scan
    end
  end
  return count
end
"#,
    );
}

#[test]
fn stress_nested_closures_with_match_and_pipe() {
    assert_ok(
        "nested_closures_match_pipe",
        r#"
cell inc(x: Int) -> Int
  return x + 1
end

cell main() -> Int
  let base = 100
  let make_adder = fn(n: Int) -> fn(Int) -> Int
    return fn(x: Int) -> Int => x + n + base
  end
  let add5 = make_adder(5)
  let result = 10 |> inc() |> add5()
  match result
    _ -> return result
  end
end
"#,
    );
}

#[test]
fn stress_defer_match_enum_closure() {
    assert_ok(
        "defer_match_enum_closure",
        r#"
enum Maybe
  Some(Int)
  None
end

cell main() -> Int
  let mut cleanup_count = 0
  defer
    cleanup_count += 1
  end
  let unwrap = fn(m: Maybe) -> Int
    match m
      Some(v) -> return v
      None -> return 0
    end
  end
  let x = Some(42)
  let y = None
  return unwrap(x) + unwrap(y) + cleanup_count
end
"#,
    );
}

#[test]
fn stress_all_loop_types_labeled_with_compound() {
    assert_ok(
        "all_loop_types_labeled_compound",
        r#"
cell main() -> Int
  let mut total = 0

  # labeled for with filter
  for @floop x in [1, 2, 3, 4, 5] if x > 1
    total += x
    if total > 10
      break @floop
    end
  end

  # labeled while
  let mut i = 0
  while @wloop i < 3
    i += 1
    total += i
  end

  # labeled loop
  let mut j = 0
  loop @lloop
    j += 1
    total *= 1
    if j >= 2
      break @lloop
    end
  end

  # range with compound
  for k in 1..=3
    total += k
  end

  return total
end
"#,
    );
}

// ============================================================================
// 2. Deep Nesting Tests (5+ levels)
// ============================================================================

#[test]
fn stress_five_level_nested_match() {
    assert_ok(
        "five_level_nested_match",
        r#"
cell deep(a: Int) -> String
  match a
    1 ->
      match a + 1
        2 ->
          match a + 2
            3 ->
              match a + 3
                4 ->
                  match a + 4
                    5 -> return "five deep"
                    _ -> return "?"
                  end
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

#[test]
fn stress_five_level_nested_if() {
    assert_ok(
        "five_level_nested_if",
        r#"
cell classify(a: Int, b: Int, c: Int, d: Int, e: Int) -> String
  if a > 0
    if b > 0
      if c > 0
        if d > 0
          if e > 0
            return "all positive"
          else
            return "e not positive"
          end
        else
          return "d not positive"
        end
      else
        return "c not positive"
      end
    else
      return "b not positive"
    end
  else
    return "a not positive"
  end
end
"#,
    );
}

#[test]
fn stress_five_level_nested_loops() {
    assert_ok(
        "five_level_nested_loops",
        r#"
cell main() -> Int
  let mut total = 0
  for a in [1, 2]
    let mut i = 0
    while i < 2
      i += 1
      loop
        for c in [10]
          let mut j = 0
          while j < 1
            j += 1
            total += a + c
          end
        end
        break
      end
    end
  end
  return total
end
"#,
    );
}

#[test]
fn stress_nested_closures_five_levels() {
    assert_ok(
        "nested_closures_5",
        r#"
cell main() -> Int
  let a = 1
  let f1 = fn() -> Int
    let b = 2
    let f2 = fn() -> Int
      let c = 3
      let f3 = fn() -> Int
        let d = 4
        let f4 = fn() -> Int
          let e = 5
          let f5 = fn() -> Int => a + b + c + d + e
          return f5()
        end
        return f4()
      end
      return f3()
    end
    return f2()
  end
  return f1()
end
"#,
    );
}

#[test]
fn stress_deep_record_nesting() {
    assert_ok(
        "deep_record_nesting",
        r#"
record A
  val: Int
end

record B
  a: A
end

record C
  b: B
end

record D
  c: C
end

record E
  d: D
end

cell main() -> Int
  let e = E(d: D(c: C(b: B(a: A(val: 42)))))
  return e.d.c.b.a.val
end
"#,
    );
}

// ============================================================================
// 3. All Operators
// ============================================================================

#[test]
fn stress_all_arithmetic_operators() {
    assert_ok(
        "all_arithmetic",
        r#"
cell main() -> Int
  let a = 100
  let b = 7
  let sum = a + b
  let diff = a - b
  let prod = a * b
  let quot = a / b
  let flr = a // b
  let rem = a % b
  let pow = 2 ** 10
  return sum + diff + prod + quot + flr + rem + pow
end
"#,
    );
}

#[test]
fn stress_all_compound_assignments() {
    assert_ok(
        "all_compound_assigns",
        r#"
cell main() -> Int
  let mut x = 1000
  x += 100
  x -= 50
  x *= 2
  x /= 5
  x //= 3
  x %= 7
  return x
end
"#,
    );
}

#[test]
fn stress_shift_operators_complex() {
    assert_ok(
        "shift_operators_complex",
        r#"
cell main() -> Int
  let a = 1 << 8
  let b = a >> 3
  let c = (a << 2) + (b >> 1)
  let flags = (1 << 0) + (1 << 3) + (1 << 7)
  return c + flags
end
"#,
    );
}

#[test]
fn stress_comparison_and_logical_chain() {
    assert_ok(
        "comparison_logical_chain",
        r#"
cell main() -> Bool
  let x = 42
  let y = 100
  let z = 0
  let a = x > 0 and x < 100 and y >= 100 and y <= 200
  let b = z == 0 or z != 1
  let c = not (x == y)
  let d = (x > z) and (y > x) and (not (z > x))
  return a and b and c and d
end
"#,
    );
}

#[test]
fn stress_floor_div_edge_cases() {
    assert_ok(
        "floor_div_edge_cases",
        r#"
cell main() -> Int
  let a = 7 // 2
  let b = 100 // 3
  let c = 1 // 1
  let d = 0 // 5
  return a + b + c + d
end
"#,
    );
}

#[test]
fn stress_exponentiation_chain() {
    assert_ok(
        "exponentiation_chain",
        r#"
cell main() -> Int
  let a = 2 ** 3
  let b = a ** 2
  let c = 3 ** 3 + 2 ** 4
  return a + b + c
end
"#,
    );
}

#[test]
fn stress_modulo_in_loop() {
    assert_ok(
        "modulo_in_loop",
        r#"
cell main() -> Int
  let mut even_count = 0
  for x in 1..=20
    if x % 2 == 0
      even_count += 1
    end
  end
  return even_count
end
"#,
    );
}

// ============================================================================
// 4. Pattern Matching Edge Cases
// ============================================================================

#[test]
fn stress_exhaustive_twelve_variants() {
    assert_ok(
        "exhaustive_12_variants",
        r#"
enum Month
  Jan
  Feb
  Mar
  Apr
  May
  Jun
  Jul
  Aug
  Sep
  Oct
  Nov
  Dec
end

cell days(m: Month) -> Int
  match m
    Jan -> return 31
    Feb -> return 28
    Mar -> return 31
    Apr -> return 30
    May -> return 31
    Jun -> return 30
    Jul -> return 31
    Aug -> return 31
    Sep -> return 30
    Oct -> return 31
    Nov -> return 30
    Dec -> return 31
  end
end
"#,
    );
}

#[test]
fn stress_nested_enum_match_three_levels() {
    assert_ok(
        "nested_enum_match_3",
        r#"
enum Inner
  X(Int)
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

cell extract(o: Outer) -> Int
  match o
    Box(m) ->
      match m
        Wrap(i) ->
          match i
            X(n) -> return n
            Y -> return 0
          end
        Empty -> return 0
      end
    Nil -> return 0
  end
end
"#,
    );
}

#[test]
fn stress_match_with_guards_multiple() {
    assert_ok(
        "match_guards_multiple",
        r#"
enum Value
  Num(Int)
  Str(String)
end

cell classify(v: Value) -> String
  match v
    Num(n) if n > 100 -> return "big number"
    Num(n) if n > 0 -> return "positive"
    Num(n) if n == 0 -> return "zero"
    Num(n) -> return "negative"
    Str(s) -> return s
  end
end
"#,
    );
}

#[test]
fn stress_wildcard_with_specific_variants() {
    assert_ok(
        "wildcard_specific_mix",
        r#"
enum HttpMethod
  Get
  Post
  Put
  Delete
  Patch
  Head
  Options
end

cell is_safe(m: HttpMethod) -> Bool
  match m
    Get -> return true
    Head -> return true
    Options -> return true
    _ -> return false
  end
end
"#,
    );
}

#[test]
fn stress_match_int_literals_many() {
    assert_ok(
        "match_int_literals_many",
        r#"
cell fizzbuzz(n: Int) -> String
  match n % 15
    0 -> return "fizzbuzz"
    3 -> return "fizz"
    5 -> return "buzz"
    6 -> return "fizz"
    9 -> return "fizz"
    10 -> return "buzz"
    12 -> return "fizz"
    _ -> return n as String
  end
end
"#,
    );
}

#[test]
fn stress_match_string_literals_many() {
    assert_ok(
        "match_string_literals_many",
        r#"
cell parse_command(cmd: String) -> Int
  match cmd
    "quit" -> return 0
    "exit" -> return 0
    "help" -> return 1
    "version" -> return 2
    "status" -> return 3
    "list" -> return 4
    "run" -> return 5
    _ -> return 99
  end
end
"#,
    );
}

#[test]
fn stress_if_let_chain() {
    assert_ok(
        "if_let_chain",
        r#"
enum Option
  Some(Int)
  None
end

cell safe_add(a: Option, b: Option) -> Int
  if let Some(x) = a
    if let Some(y) = b
      return x + y
    end
    return x
  end
  return 0
end
"#,
    );
}

#[test]
fn stress_match_expr_in_let() {
    assert_ok(
        "match_expr_in_let",
        r#"
enum Dir
  Up
  Down
  Left
  Right
end

cell main() -> Int
  let d = Up
  let dy = match d
    Up -> 1
    Down -> 0 - 1
    Left -> 0
    Right -> 0
  end
  let dx = match d
    Up -> 0
    Down -> 0
    Left -> 0 - 1
    Right -> 1
  end
  return dx + dy
end
"#,
    );
}

// ============================================================================
// 5. Closures and Captures
// ============================================================================

#[test]
fn stress_closure_capturing_multiple_scopes() {
    assert_ok(
        "closure_multi_scope",
        r#"
cell main() -> Int
  let x = 10
  let y = 20
  let z = 30
  let f = fn(a: Int) -> Int
    return a + x + y + z
  end
  return f(40)
end
"#,
    );
}

#[test]
fn stress_closure_as_argument_chain() {
    assert_ok(
        "closure_as_arg_chain",
        r#"
cell apply(x: Int, f: fn(Int) -> Int) -> Int
  return f(x)
end

cell compose(x: Int, f: fn(Int) -> Int, g: fn(Int) -> Int) -> Int
  return g(f(x))
end

cell main() -> Int
  let double = fn(n: Int) -> Int => n * 2
  let inc = fn(n: Int) -> Int => n + 1
  let a = apply(5, double)
  let b = compose(5, double, inc)
  return a + b
end
"#,
    );
}

#[test]
fn stress_closure_returning_closure() {
    assert_ok(
        "closure_returning_closure",
        r#"
cell make_multiplier(factor: Int) -> fn(Int) -> Int
  return fn(x: Int) -> Int => x * factor
end

cell main() -> Int
  let triple = make_multiplier(3)
  let quadruple = make_multiplier(4)
  return triple(10) + quadruple(10)
end
"#,
    );
}

#[test]
fn stress_closure_in_match_arm() {
    assert_ok(
        "closure_in_match_arm",
        r#"
cell main() -> Int
  let mode = "double"
  let f = match mode
    "double" -> fn(x: Int) -> Int => x * 2
    "triple" -> fn(x: Int) -> Int => x * 3
    _ -> fn(x: Int) -> Int => x
  end
  return f(21)
end
"#,
    );
}

#[test]
fn stress_closure_with_mutable_capture() {
    assert_ok(
        "closure_mutable_capture",
        r#"
cell main() -> Int
  let mut state = 0
  let tick = fn() -> Int
    state += 1
    return state
  end
  let a = tick()
  let b = tick()
  let c = tick()
  return a + b + c
end
"#,
    );
}

// ============================================================================
// 6. Process Types (machine, memory, pipeline)
// ============================================================================

#[test]
fn stress_machine_multi_state() {
    assert_ok(
        "machine_multi_state",
        r#"
machine TrafficLight
  initial: Red
  state Red
    on_enter() / {trace}
      transition Green()
    end
  end
  state Green
    on_enter() / {trace}
      transition Yellow()
    end
  end
  state Yellow
    on_enter() / {trace}
      transition Off()
    end
  end
  state Off
    terminal: true
  end
end
"#,
    );
}

#[test]
fn stress_machine_with_terminal() {
    assert_ok(
        "machine_terminal",
        r#"
machine OrderFlow
  initial: Created
  state Created
    on_enter() / {trace}
      transition Processing()
    end
  end
  state Processing
    on_enter() / {trace}
      transition Shipped()
    end
  end
  state Shipped
    on_enter() / {trace}
      transition Delivered()
    end
  end
  state Delivered
    terminal: true
  end
end
"#,
    );
}

#[test]
fn stress_memory_declaration() {
    assert_ok(
        "memory_decl",
        r#"
memory ChatHistory: short_term
  window: 50
end
"#,
    );
}

#[test]
fn stress_orchestration_declaration() {
    assert_ok(
        "orchestration_decl",
        r#"
orchestration ReviewPipeline
  coordinator: LeadReviewer
  workers: [CodeReviewer, SecurityReviewer, DocReviewer]
end
"#,
    );
}

// ============================================================================
// 7. Type Error Tests (assert_err)
// ============================================================================

#[test]
fn stress_err_return_type_mismatch_string_for_int() {
    assert_err(
        "err_return_mismatch",
        r#"
cell main() -> Int
  return "not a number"
end
"#,
        "type",
    );
}

#[test]
fn stress_err_undefined_variable() {
    assert_err(
        "err_undef_var",
        r#"
cell main() -> Int
  return totally_undefined
end
"#,
        "undefined",
    );
}

#[test]
fn stress_err_missing_return_type_mismatch() {
    assert_err(
        "err_bool_for_int",
        r#"
cell main() -> Int
  return true
end
"#,
        "type",
    );
}

#[test]
fn stress_err_shift_float_operand() {
    assert_err(
        "err_shift_float",
        r#"
cell main() -> Int
  return 1.0 << 2
end
"#,
        "type",
    );
}

#[test]
fn stress_err_string_plus_int() {
    assert_err(
        "err_string_plus_int",
        r#"
cell main() -> Int
  return "hello" + 42
end
"#,
        "type",
    );
}

#[test]
fn stress_err_incomplete_match_two_missing() {
    assert_err(
        "err_incomplete_match_2",
        r#"
enum Compass
  North
  South
  East
  West
end

cell go(d: Compass) -> String
  match d
    North -> return "up"
    South -> return "down"
  end
end
"#,
        "East",
    );
}

#[test]
fn stress_err_incomplete_match_single_variant() {
    assert_err(
        "err_incomplete_single",
        r#"
enum Wrapper
  Value(Int)
end

cell unwrap(w: Wrapper) -> Int
  match w
    _ if true -> return 0
  end
end
"#,
        "Value",
    );
}

#[test]
fn stress_err_undefined_type_in_record() {
    assert_err(
        "err_undef_type",
        r#"
record Broken
  x: NonExistentType
end
"#,
        "undefinedtype",
    );
}

#[test]
fn stress_err_effect_violation() {
    assert_err(
        "err_effect_violation",
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

// ============================================================================
// 8. Range Patterns in Match
// ============================================================================

#[test]
fn stress_range_pattern_exclusive() {
    assert_ok(
        "range_pattern_exclusive",
        r#"
cell classify(n: Int) -> String
  match n
    1..5 -> return "low"
    5..10 -> return "mid"
    _ -> return "other"
  end
end
"#,
    );
}

#[test]
fn stress_range_pattern_inclusive() {
    assert_ok(
        "range_pattern_inclusive",
        r#"
cell grade(score: Int) -> String
  match score
    90..=100 -> return "A"
    80..=89 -> return "B"
    70..=79 -> return "C"
    60..=69 -> return "D"
    _ -> return "F"
  end
end
"#,
    );
}

#[test]
fn stress_range_pattern_with_guard() {
    assert_ok(
        "range_pattern_guard",
        r#"
cell check(x: Int, flag: Bool) -> String
  match x
    1..=10 if flag -> return "flagged low"
    1..=10 -> return "low"
    _ -> return "other"
  end
end
"#,
    );
}

#[test]
fn stress_range_pattern_mixed_with_literals() {
    assert_ok(
        "range_pattern_mixed_literals",
        r#"
cell describe(n: Int) -> String
  match n
    0 -> return "zero"
    1..=9 -> return "single digit"
    10..=99 -> return "double digit"
    100 -> return "exactly hundred"
    _ -> return "large"
  end
end
"#,
    );
}

// ============================================================================
// 9. is/as Expressions
// ============================================================================

#[test]
fn stress_is_in_complex_condition() {
    assert_ok(
        "is_complex_condition",
        r#"
cell check(x: Int, y: Float) -> Bool
  let a = x is Int
  let b = y is Float
  return a and b
end
"#,
    );
}

#[test]
fn stress_as_chain_conversions() {
    assert_ok(
        "as_chain_conversions",
        r#"
cell main() -> String
  let x = 42
  let f = x as Float
  let s = f as String
  let i = f as Int
  return s
end
"#,
    );
}

#[test]
fn stress_is_as_in_match_arm() {
    assert_ok(
        "is_as_in_match",
        r#"
cell process(x: Int) -> String
  match x
    0 -> return "zero"
    _ ->
      if x is Int
        return x as String
      end
      return "?"
  end
end
"#,
    );
}

#[test]
fn stress_is_as_with_loop() {
    assert_ok(
        "is_as_loop",
        r#"
cell main() -> String
  let mut result = ""
  for x in [1, 2, 3]
    if x is Int
      let s = x as String
      result = result + s
    end
  end
  return result
end
"#,
    );
}

// ============================================================================
// 10. Exhaustiveness with Many Variants
// ============================================================================

#[test]
fn stress_exhaustive_fifteen_variants() {
    assert_ok(
        "exhaustive_15",
        r#"
enum Key
  A
  B
  C
  D
  E
  F
  G
  H
  I
  J
  K
  L
  M
  N
  O
end

cell index(k: Key) -> Int
  match k
    A -> return 0
    B -> return 1
    C -> return 2
    D -> return 3
    E -> return 4
    F -> return 5
    G -> return 6
    H -> return 7
    I -> return 8
    J -> return 9
    K -> return 10
    L -> return 11
    M -> return 12
    N -> return 13
    O -> return 14
  end
end
"#,
    );
}

#[test]
fn stress_exhaustive_wildcard_catches_many() {
    assert_ok(
        "exhaustive_wildcard_many",
        r#"
enum Planet
  Mercury
  Venus
  Earth
  Mars
  Jupiter
  Saturn
  Uranus
  Neptune
end

cell is_inner(p: Planet) -> Bool
  match p
    Mercury -> return true
    Venus -> return true
    Earth -> return true
    Mars -> return true
    _ -> return false
  end
end
"#,
    );
}

#[test]
fn stress_exhaustive_enum_with_payloads_mixed() {
    assert_ok(
        "exhaustive_payloads_mixed",
        r#"
enum Expr
  Lit(Int)
  Add(Int)
  Mul(Int)
  Neg
  Nop
end

cell eval(e: Expr) -> Int
  match e
    Lit(n) -> return n
    Add(n) -> return n
    Mul(n) -> return n
    Neg -> return 0 - 1
    Nop -> return 0
  end
end
"#,
    );
}

// ============================================================================
// 11. String Features
// ============================================================================

#[test]
fn stress_interpolation_complex_expressions() {
    assert_ok(
        "interp_complex",
        r#"
cell main() -> String
  let x = 10
  let y = 20
  let name = "world"
  let msg = "Hello {name}, x={x}, y={y}, sum={x + y}"
  return msg
end
"#,
    );
}

#[test]
fn stress_raw_string() {
    assert_ok(
        "raw_string",
        r#"
cell main() -> String
  let path = r"C:\Users\test\file.txt"
  return path
end
"#,
    );
}

#[test]
fn stress_string_concat_chain() {
    assert_ok(
        "string_concat_chain",
        r#"
cell main() -> String
  let a = "Hello"
  let b = " "
  let c = "World"
  let d = "!"
  return a + b + c + d
end
"#,
    );
}

// ============================================================================
// 12. Collections and Comprehensions
// ============================================================================

#[test]
fn stress_list_comprehension_chained() {
    assert_ok(
        "list_comp_chained",
        r#"
cell double(x: Int) -> Int
  return x * 2
end

cell main() -> list[Int]
  let xs = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
  return [double(x) for x in xs if x % 2 == 0]
end
"#,
    );
}

#[test]
fn stress_set_comprehension_with_filter() {
    assert_ok(
        "set_comp_filter",
        r#"
cell main() -> set[Int]
  return {x * x for x in [1, 2, 3, 4, 5] if x > 2}
end
"#,
    );
}

#[test]
fn stress_map_literal_complex() {
    assert_ok(
        "map_literal_complex",
        r#"
cell main() -> map[String, Int]
  return {"alpha": 1, "beta": 2, "gamma": 3, "delta": 4, "epsilon": 5}
end
"#,
    );
}

#[test]
fn stress_null_coalesce_deep_chain() {
    assert_ok(
        "null_coalesce_deep",
        r#"
cell main() -> Int
  let a: Int? = null
  let b: Int? = null
  let c: Int? = null
  let d: Int? = null
  let e: Int? = null
  let f: Int? = 99
  return a ?? b ?? c ?? d ?? e ?? f ?? 0
end
"#,
    );
}

#[test]
fn stress_spread_with_extra_elements() {
    assert_ok(
        "spread_extra_elements",
        r#"
cell main() -> list[Int]
  let head = [1, 2, 3]
  let tail = [7, 8, 9]
  return [0, ...head, 4, 5, 6, ...tail, 10]
end
"#,
    );
}

// ============================================================================
// 13. Tools, Grants, Effects
// ============================================================================

#[test]
fn stress_tool_grant_effect_chain() {
    assert_ok(
        "tool_grant_effect",
        r#"
use tool http.get as HttpGet
bind effect http to HttpGet
grant HttpGet
  domain "*.example.com"
  timeout_ms 5000

cell fetch(url: String) -> String / {http}
  let response = HttpGet(url: url)
  return string(response)
end
"#,
    );
}

#[test]
fn stress_multiple_tools_grants() {
    assert_ok(
        "multi_tools_grants",
        r#"
use tool http.get as HttpGet
use tool http.post as HttpPost
bind effect http to HttpGet
bind effect http to HttpPost
grant HttpGet
  domain "*.api.com"
grant HttpPost
  domain "*.api.com"
  timeout_ms 10000

cell read_data(url: String) -> String / {http}
  return string(HttpGet(url: url))
end

cell write_data(url: String) -> String / {http}
  return string(HttpPost(url: url))
end
"#,
    );
}

// ============================================================================
// 14. Records with Constraints and Defaults
// ============================================================================

#[test]
fn stress_record_where_multiple_constraints() {
    assert_ok(
        "record_multi_constraints",
        r#"
record User
  name: String where length(name) > 0
  age: Int where age >= 0
  email: String
end

cell main() -> User
  return User(name: "Alice", age: 30, email: "a@b.com")
end
"#,
    );
}

#[test]
fn stress_record_defaults_and_override() {
    assert_ok(
        "record_defaults_override",
        r#"
record Config
  host: String = "localhost"
  port: Int = 8080
  debug: Bool = false
  timeout: Int = 30
end

cell main() -> Config
  return Config(port: 3000, debug: true)
end
"#,
    );
}

#[test]
fn stress_record_property_shorthand_and_field_access() {
    assert_ok(
        "record_shorthand_access",
        r#"
record Vec2
  x: Int
  y: Int
end

cell dot(a: Vec2, b: Vec2) -> Int
  return a.x * b.x + a.y * b.y
end

cell main() -> Int
  let x = 3
  let y = 4
  let v1 = Vec2(x, y)
  let v2 = Vec2(x: 1, y: 2)
  return dot(v1, v2)
end
"#,
    );
}

// ============================================================================
// 15. Advanced Combinations
// ============================================================================

#[test]
fn stress_recursive_with_match_and_closure() {
    assert_ok(
        "recursive_match_closure",
        r#"
cell fib(n: Int) -> Int
  match n
    0 -> return 0
    1 -> return 1
    _ -> return fib(n - 1) + fib(n - 2)
  end
end

cell main() -> Int
  let compute = fn(x: Int) -> Int => fib(x)
  return compute(10)
end
"#,
    );
}

#[test]
fn stress_result_type_with_match() {
    assert_ok(
        "result_type_match",
        r#"
cell safe_div(a: Int, b: Int) -> result[Int, String]
  if b == 0
    return err("divide by zero")
  end
  return ok(a / b)
end

cell main() -> Int
  let r = safe_div(10, 2)
  match r
    ok(v) -> return v
    err(e) -> return 0
  end
end
"#,
    );
}

#[test]
fn stress_tuple_and_type_alias() {
    assert_ok(
        "tuple_type_alias",
        r#"
type Pair = tuple[Int, String]

cell make_pair(n: Int) -> Pair
  return (n, n as String)
end

cell main() -> Pair
  return make_pair(42)
end
"#,
    );
}

#[test]
fn stress_complex_interpolation_with_calls() {
    assert_ok(
        "interp_with_calls",
        r#"
cell double(x: Int) -> Int
  return x * 2
end

cell main() -> String
  let n = 5
  return "double of {n} is {double(n)}"
end
"#,
    );
}

#[test]
fn stress_async_await() {
    assert_ok(
        "async_await",
        r#"
async cell fetch_data() -> Int
  return 42
end

cell main() -> Int
  return await fetch_data()
end
"#,
    );
}

#[test]
fn stress_try_operator_result() {
    assert_ok(
        "try_operator_result",
        r#"
cell may_fail(flag: Bool) -> result[Int, String]
  if flag
    return ok(42)
  end
  return err("failed")
end

cell main() -> result[Int, String]
  let v = may_fail(true)?
  return ok(v + 1)
end
"#,
    );
}

#[test]
fn stress_for_range_compound_closure() {
    assert_ok(
        "for_range_compound_closure",
        r#"
cell main() -> Int
  let mut total = 0
  let mul = fn(a: Int, b: Int) -> Int => a * b
  for i in 1..=10
    total += mul(i, i)
  end
  return total
end
"#,
    );
}

#[test]
fn stress_nested_record_construction() {
    assert_ok(
        "nested_record_construction",
        r#"
record Address
  city: String
  zip: Int
end

record Person
  name: String
  addr: Address
end

record Company
  ceo: Person
  hq: Address
end

cell main() -> String
  let c = Company(
    ceo: Person(
      name: "Alice",
      addr: Address(city: "NYC", zip: 10001)
    ),
    hq: Address(city: "SF", zip: 94102)
  )
  return c.ceo.name
end
"#,
    );
}

#[test]
fn stress_handler_declaration() {
    assert_ok(
        "handler_decl",
        r#"
record Response
  status: Int
  body: String
end

handler MockHttp
  handle http.get(url: String) -> Response
    return Response(status: 200, body: "mock")
  end
end
"#,
    );
}

#[test]
fn stress_const_declaration() {
    assert_ok(
        "const_decl",
        r#"
const MAX_SIZE: Int = 1024
const PI: Float = 3.14159

cell main() -> Int
  return MAX_SIZE
end
"#,
    );
}
