//! Spec suite: nested definitions and impl block tests.
//!
//! Tests for:
//! - Nested cell/enum/record definitions inside cell bodies
//! - Record method scoping / generic T in impl blocks

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

#[allow(dead_code)]
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
// T194: Nested cell definitions
// ============================================================================

#[test]
fn t194_nested_cell_basic() {
    assert_ok(
        "t194_nested_cell_basic",
        r#"
cell outer() -> Int
  cell inner(x: Int) -> Int
    return x + 1
  end
  return inner(41)
end
"#,
    );
}

#[test]
fn t194_nested_cell_multiple() {
    assert_ok(
        "t194_nested_cell_multiple",
        r#"
cell main() -> Int
  cell add(a: Int, b: Int) -> Int
    return a + b
  end
  cell mul(a: Int, b: Int) -> Int
    return a * b
  end
  return add(2, 3) + mul(4, 5)
end
"#,
    );
}

#[test]
fn t194_nested_cell_deep() {
    assert_ok(
        "t194_nested_cell_deep",
        r#"
cell outer() -> Int
  cell middle() -> Int
    cell inner() -> Int
      return 42
    end
    return inner()
  end
  return middle()
end
"#,
    );
}

// ============================================================================
// T194: Nested record definitions
// ============================================================================

#[test]
fn t194_nested_record_basic() {
    assert_ok(
        "t194_nested_record_basic",
        r#"
cell main() -> String
  record Point
    x: Int
    y: Int
  end
  let p = Point(x: 1, y: 2)
  return "{p.x},{p.y}"
end
"#,
    );
}

#[test]
fn t194_nested_record_used_in_function() {
    assert_ok(
        "t194_nested_record_used_in_function",
        r#"
cell process() -> Int
  record Config
    width: Int
    height: Int
  end
  cell area(c: Config) -> Int
    return c.width * c.height
  end
  let cfg = Config(width: 10, height: 20)
  return area(cfg)
end
"#,
    );
}

// ============================================================================
// T194: Nested enum definitions
// ============================================================================

#[test]
fn t194_nested_enum_basic() {
    assert_ok(
        "t194_nested_enum_basic",
        r#"
cell main() -> String
  enum Color
    Red
    Green
    Blue
  end
  let c = Color.Red
  match c
    Color.Red -> return "red"
    Color.Green -> return "green"
    Color.Blue -> return "blue"
  end
end
"#,
    );
}

#[test]
fn t194_nested_enum_with_payload() {
    assert_ok(
        "t194_nested_enum_with_payload",
        r#"
cell main() -> Int
  enum Shape
    Circle(Float)
    Rectangle(Float)
  end
  let s = Shape.Circle(3.14)
  match s
    Shape.Circle(r) -> return 1
    Shape.Rectangle(w) -> return 2
  end
end
"#,
    );
}

// ============================================================================
// T194: Mixed nested definitions
// ============================================================================

#[test]
fn t194_mixed_nested_defs() {
    assert_ok(
        "t194_mixed_nested_defs",
        r#"
cell main() -> String
  record Person
    name: String
    age: Int
  end
  enum Status
    Active
    Inactive
  end
  cell describe(p: Person) -> String
    return p.name
  end
  let p = Person(name: "Alice", age: 30)
  return describe(p)
end
"#,
    );
}

// ============================================================================
// T208: Impl block method scoping — methods with shared param names
// ============================================================================

#[test]
fn t208_impl_methods_shared_param_names() {
    assert_ok(
        "t208_impl_methods_shared_param_names",
        r#"
record Vec2
  x: Float
  y: Float
end

trait Measurable
  cell length(self: Vec2) -> Float
  cell scale(self: Vec2, factor: Float) -> Vec2
end

impl Measurable for Vec2
  cell length(self: Vec2) -> Float
    return self.x + self.y
  end
  cell scale(self: Vec2, factor: Float) -> Vec2
    return Vec2(x: self.x * factor, y: self.y * factor)
  end
end
"#,
    );
}

#[test]
fn t208_impl_methods_distinct_scopes() {
    assert_ok(
        "t208_impl_methods_distinct_scopes",
        r#"
record Point
  x: Int
  y: Int
end

trait Ops
  cell add(a: Point, b: Point) -> Point
  cell sub(a: Point, b: Point) -> Point
end

impl Ops for Point
  cell add(a: Point, b: Point) -> Point
    return Point(x: a.x + b.x, y: a.y + b.y)
  end
  cell sub(a: Point, b: Point) -> Point
    return Point(x: a.x - b.x, y: a.y - b.y)
  end
end
"#,
    );
}

// ============================================================================
// T208: Impl block with generic type parameter T
// ============================================================================

#[test]
fn t208_impl_generic_type_param() {
    assert_ok(
        "t208_impl_generic_type_param",
        r#"
record Box[T]
  value: T
end

trait Container
  cell get[T](self: Box[T]) -> T
end

impl[T] Container for Box[T]
  cell get[T](self: Box[T]) -> T
    return self.value
  end
end
"#,
    );
}

#[test]
fn t208_impl_generic_multiple_methods() {
    assert_ok(
        "t208_impl_generic_multiple_methods",
        r#"
record Wrapper[T]
  inner: T
end

trait Transform
  cell unwrap[T](self: Wrapper[T]) -> T
  cell rewrap[T](self: Wrapper[T], val: T) -> Wrapper[T]
end

impl[T] Transform for Wrapper[T]
  cell unwrap[T](self: Wrapper[T]) -> T
    return self.inner
  end
  cell rewrap[T](self: Wrapper[T], val: T) -> Wrapper[T]
    return Wrapper(inner: val)
  end
end
"#,
    );
}

// ============================================================================
// T208: Impl block methods are lowered to LIR
// ============================================================================

#[test]
fn t208_impl_methods_produce_lir_cells() {
    let code = r#"
record Counter
  value: Int
end

trait Countable
  cell increment(self: Counter) -> Counter
end

impl Countable for Counter
  cell increment(self: Counter) -> Counter
    return Counter(value: self.value + 1)
  end
end
"#;
    let md = markdown(code);
    let module = compile(&md).expect("should compile");
    // The impl method should be lowered as "Counter.increment"
    let method_names: Vec<&str> = module.cells.iter().map(|c| c.name.as_str()).collect();
    assert!(
        method_names.contains(&"Counter.increment"),
        "expected 'Counter.increment' in cells, found: {:?}",
        method_names
    );
}

// ============================================================================
// T194: Verify local defs produce LIR types/cells
// ============================================================================

#[test]
fn t194_local_record_produces_lir_type() {
    let code = r#"
cell main() -> Int
  record Inner
    val: Int
  end
  let i = Inner(val: 42)
  return i.val
end
"#;
    let md = markdown(code);
    let module = compile(&md).expect("should compile");
    let type_names: Vec<&str> = module.types.iter().map(|t| t.name.as_str()).collect();
    assert!(
        type_names.contains(&"Inner"),
        "expected 'Inner' in types, found: {:?}",
        type_names
    );
}

#[test]
fn t194_local_cell_produces_lir_cell() {
    let code = r#"
cell main() -> Int
  cell helper(x: Int) -> Int
    return x * 2
  end
  return helper(21)
end
"#;
    let md = markdown(code);
    let module = compile(&md).expect("should compile");
    let cell_names: Vec<&str> = module.cells.iter().map(|c| c.name.as_str()).collect();
    assert!(
        cell_names.contains(&"helper"),
        "expected 'helper' in cells, found: {:?}",
        cell_names
    );
}

#[test]
fn t194_local_enum_produces_lir_type() {
    let code = r#"
cell main() -> Int
  enum Direction
    Up
    Down
  end
  let d = Direction.Up
  match d
    Direction.Up -> return 1
    Direction.Down -> return 2
  end
end
"#;
    let md = markdown(code);
    let module = compile(&md).expect("should compile");
    let type_names: Vec<&str> = module.types.iter().map(|t| t.name.as_str()).collect();
    assert!(
        type_names.contains(&"Direction"),
        "expected 'Direction' in types, found: {:?}",
        type_names
    );
}
