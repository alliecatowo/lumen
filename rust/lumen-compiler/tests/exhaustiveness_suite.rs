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

// ── Exhaustive matches (all variants covered) ──

#[test]
fn exhaustive_all_variants() {
    assert_ok(
        "exhaustive_all_variants",
        r#"
enum Color
  Red
  Green
  Blue
end

cell describe(c: Color) -> String
  match c
    Red -> return "red"
    Green -> return "green"
    Blue -> return "blue"
  end
end
"#,
    );
}

#[test]
fn exhaustive_variants_with_payload() {
    assert_ok(
        "exhaustive_variants_with_payload",
        r#"
enum Shape
  Circle(Float)
  Rectangle(Float)
  Triangle(Float)
end

cell area(s: Shape) -> Float
  match s
    Circle(r) -> return r
    Rectangle(w) -> return w
    Triangle(b) -> return b
  end
end
"#,
    );
}

// ── Wildcard makes it exhaustive ──

#[test]
fn exhaustive_with_wildcard() {
    assert_ok(
        "exhaustive_with_wildcard",
        r#"
enum Status
  Active
  Inactive
  Pending
  Archived
end

cell is_active(s: Status) -> Bool
  match s
    Active -> return true
    _ -> return false
  end
end
"#,
    );
}

#[test]
fn exhaustive_with_catch_all_ident() {
    assert_ok(
        "exhaustive_with_catch_all_ident",
        r#"
enum Level
  Low
  Medium
  High
end

cell describe(l: Level) -> String
  match l
    High -> return "high"
    other -> return "not high"
  end
end
"#,
    );
}

// ── Non-exhaustive (missing variants) ──

#[test]
fn non_exhaustive_missing_one() {
    assert_err(
        "non_exhaustive_missing_one",
        r#"
enum Direction
  North
  South
  East
  West
end

cell go(d: Direction) -> String
  match d
    North -> return "up"
    South -> return "down"
    East -> return "right"
  end
end
"#,
        "West",
    );
}

#[test]
fn non_exhaustive_missing_multiple() {
    assert_err(
        "non_exhaustive_missing_multiple",
        r#"
enum Color
  Red
  Green
  Blue
  Yellow
end

cell name(c: Color) -> String
  match c
    Red -> return "red"
    Green -> return "green"
  end
end
"#,
        "Blue",
    );
}

// ── Match on non-enum type — no exhaustiveness check ──

#[test]
fn no_check_on_non_enum() {
    assert_ok(
        "no_check_on_non_enum",
        r#"
cell classify(n: Int) -> String
  match n
    0 -> return "zero"
    1 -> return "one"
  end
end
"#,
    );
}

#[test]
fn no_check_on_string() {
    assert_ok(
        "no_check_on_string",
        r#"
cell greet(name: String) -> String
  match name
    "Alice" -> return "Hi Alice"
  end
end
"#,
    );
}

// ── Guards — currently treated as covering the variant ──
// Note: guard predicates aren't tracked for exhaustiveness (would need SMT solver).
// A guarded arm for a variant still counts as covering that variant.

#[test]
fn guard_still_covers_variant() {
    assert_ok(
        "guard_still_covers_variant",
        r#"
enum Value
  Small(Int)
  Large(Int)
end

cell classify(v: Value) -> String
  match v
    Small(n) if n > 0 -> return "positive small"
    Large(n) -> return "large"
  end
end
"#,
    );
}

#[test]
fn guard_with_wildcard_still_exhaustive() {
    assert_ok(
        "guard_with_wildcard_still_exhaustive",
        r#"
enum Value
  Small(Int)
  Large(Int)
end

cell classify(v: Value) -> String
  match v
    Small(n) if n > 0 -> return "positive small"
    _ -> return "other"
  end
end
"#,
    );
}

// ── Match expression (expression position) ──

#[test]
fn match_expr_exhaustive() {
    assert_ok(
        "match_expr_exhaustive",
        r#"
enum Toggle
  On
  Off
end

cell to_int(t: Toggle) -> Int
  let x = match t
    On -> 1
    Off -> 0
  end
  return x
end
"#,
    );
}

#[test]
fn match_expr_non_exhaustive() {
    assert_err(
        "match_expr_non_exhaustive",
        r#"
enum Light
  Red
  Yellow
  Green
end

cell action(l: Light) -> String
  let x = match l
    Red -> "stop"
    Green -> "go"
  end
  return x
end
"#,
        "Yellow",
    );
}

// ── Edge cases ──

#[test]
fn single_variant_enum_exhaustive() {
    assert_ok(
        "single_variant_enum_exhaustive",
        r#"
enum Unit
  Value
end

cell check(u: Unit) -> Bool
  match u
    Value -> return true
  end
end
"#,
    );
}

#[test]
fn single_variant_enum_missing() {
    assert_err(
        "single_variant_enum_missing",
        r#"
enum Wrapper
  Inner(Int)
end

cell unwrap(w: Wrapper) -> Int
  match w
    _ if true -> return 0
  end
end
"#,
        "Inner",
    );
}
