use lumen_compiler::compile;

struct CompileCase {
    id: &'static str,
    source: &'static str,
}

fn markdown_from_code(source: &str) -> String {
    format!("# test\n\n```lumen\n{}\n```\n", source.trim())
}

fn assert_compile_ok(case: &CompileCase) {
    let md = markdown_from_code(case.source);
    if let Err(err) = compile(&md) {
        panic!(
            "case '{}' failed to compile\n--- source ---\n{}\n--- error ---\n{}",
            case.id, case.source, err
        );
    }
}

// ── T? optional type sugar ──

#[test]
fn optional_type_sugar_param() {
    assert_compile_ok(&CompileCase {
        id: "optional_type_param",
        source: r#"
cell greet(name: String?) -> String
  if name == null
    return "Hello, stranger!"
  end
  return "Hello!"
end
"#,
    });
}

#[test]
fn optional_type_sugar_return() {
    assert_compile_ok(&CompileCase {
        id: "optional_type_return",
        source: r#"
cell find(key: String) -> Int?
  return null
end
"#,
    });
}

#[test]
fn optional_type_sugar_let() {
    assert_compile_ok(&CompileCase {
        id: "optional_type_let",
        source: r#"
cell main() -> Null
  let x: Int? = null
  let y: Int? = 42
end
"#,
    });
}

#[test]
fn optional_type_sugar_record_field() {
    assert_compile_ok(&CompileCase {
        id: "optional_type_record_field",
        source: r#"
record User
  name: String
  email: String?
end

cell main() -> Null
  let u = User(name: "Alice", email: null)
end
"#,
    });
}

#[test]
fn optional_type_sugar_nested() {
    // list[Int?] should parse as list[Int | Null]
    assert_compile_ok(&CompileCase {
        id: "optional_type_nested",
        source: r#"
cell takes_list(xs: list[Int?]) -> Null
  return null
end
"#,
    });
}

#[test]
fn optional_type_sugar_in_union() {
    assert_compile_ok(&CompileCase {
        id: "optional_type_in_union",
        source: r#"
cell main() -> String?
  return null
end
"#,
    });
}

// ── Compound assignment operators ──

#[test]
fn compound_assign_modulo() {
    assert_compile_ok(&CompileCase {
        id: "compound_assign_modulo",
        source: r#"
cell main() -> Int
  let mut x = 10
  x %= 3
  return x
end
"#,
    });
}

#[test]
fn compound_assign_power() {
    assert_compile_ok(&CompileCase {
        id: "compound_assign_power",
        source: r#"
cell main() -> Int
  let mut x = 2
  x **= 3
  return x
end
"#,
    });
}

#[test]
fn compound_assign_bitand() {
    assert_compile_ok(&CompileCase {
        id: "compound_assign_bitand",
        source: r#"
cell main() -> Int
  let mut x = 0xFF
  x &= 0x0F
  return x
end
"#,
    });
}

#[test]
fn compound_assign_bitor() {
    assert_compile_ok(&CompileCase {
        id: "compound_assign_bitor",
        source: r#"
cell main() -> Int
  let mut x = 0x0F
  x |= 0xF0
  return x
end
"#,
    });
}

#[test]
fn compound_assign_bitxor() {
    assert_compile_ok(&CompileCase {
        id: "compound_assign_bitxor",
        source: r#"
cell main() -> Int
  let mut x = 0xFF
  x ^= 0x0F
  return x
end
"#,
    });
}

#[test]
fn compound_assign_existing_plus() {
    assert_compile_ok(&CompileCase {
        id: "compound_assign_existing_plus",
        source: r#"
cell main() -> Int
  let mut x = 10
  x += 5
  return x
end
"#,
    });
}

#[test]
fn compound_assign_existing_minus() {
    assert_compile_ok(&CompileCase {
        id: "compound_assign_existing_minus",
        source: r#"
cell main() -> Int
  let mut x = 10
  x -= 3
  return x
end
"#,
    });
}

#[test]
fn compound_assign_existing_star() {
    assert_compile_ok(&CompileCase {
        id: "compound_assign_existing_star",
        source: r#"
cell main() -> Int
  let mut x = 5
  x *= 3
  return x
end
"#,
    });
}

#[test]
fn compound_assign_existing_slash() {
    assert_compile_ok(&CompileCase {
        id: "compound_assign_existing_slash",
        source: r#"
cell main() -> Int
  let mut x = 15
  x /= 3
  return x
end
"#,
    });
}

#[test]
fn compound_assign_with_mut() {
    // Compound assignment works on mutable variables
    assert_compile_ok(&CompileCase {
        id: "compound_assign_with_mut",
        source: r#"
cell main() -> Int
  let mut x = 10
  x += 5
  x -= 2
  x *= 3
  x /= 2
  x %= 7
  return x
end
"#,
    });
}

#[test]
fn compound_assign_all_new_ops() {
    assert_compile_ok(&CompileCase {
        id: "compound_assign_all_new_ops",
        source: r#"
cell main() -> Int
  let mut a = 100
  a %= 7
  let mut b = 2
  b **= 8
  let mut c = 0xFF
  c &= 0x0F
  let mut d = 0x0F
  d |= 0xF0
  let mut e = 0xFF
  e ^= 0x0F
  return a + b + c + d + e
end
"#,
    });
}
