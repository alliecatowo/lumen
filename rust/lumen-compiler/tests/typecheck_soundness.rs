//! Typechecker soundness regressions: `let` immutability, block scoping,
//! call arity, record constructor completeness and union assignability.

use lumen_compiler::compile;

fn wrap(source: &str) -> String {
    format!("# t\n\n```lumen\n{}\n```\n", source.trim())
}

fn err_of(source: &str) -> String {
    match compile(&wrap(source)) {
        Ok(_) => panic!("expected a compile error for:\n{source}"),
        Err(e) => e.to_string().to_lowercase(),
    }
}

fn ok(source: &str) {
    if let Err(e) = compile(&wrap(source)) {
        panic!("expected success, got: {e}\n{source}");
    }
}

#[test]
fn let_is_immutable_and_let_mut_is_not() {
    let e = err_of("cell main() -> Int\n  let x = 1\n  x = 2\n  return x\nend");
    assert!(e.contains("immutable"), "{e}");
    let e = err_of("cell main() -> Int\n  let x = 1\n  x += 2\n  return x\nend");
    assert!(e.contains("immutable"), "{e}");
    ok("cell main() -> Int\n  let mut x = 1\n  x = 2\n  x += 1\n  return x\nend");
    // params and loop variables stay assignable
    ok("cell f(a: Int) -> Int\n  a = a + 1\n  return a\nend");
}

#[test]
fn let_bound_collection_cannot_be_mutated_in_place() {
    let e = err_of("cell main() -> Int\n  let xs = [1, 2]\n  xs[0] = 5\n  return xs[0]\nend");
    assert!(e.contains("immutable"), "{e}");
}

#[test]
fn block_scoped_bindings_do_not_leak() {
    let e = err_of("cell main() -> Int\n  if true\n    let y = 5\n  end\n  return y\nend");
    assert!(e.contains("cannot find") || e.contains("undefined"), "{e}");
    let e = err_of(
        "cell main() -> Int\n  let mut t = 0\n  for i in [1, 2]\n    t = t + i\n  end\n  return i\nend",
    );
    assert!(e.contains("cannot find") || e.contains("undefined"), "{e}");
    let e = err_of("cell main() -> Int\n  while false\n    let z = 1\n  end\n  return z\nend");
    assert!(e.contains("cannot find") || e.contains("undefined"), "{e}");
}

#[test]
fn shadowing_restores_the_outer_binding() {
    // The inner `x` is a String; after the block the outer Int is back.
    ok("cell main() -> Int\n  let x = 1\n  if true\n    let x = \"s\"\n    print(x)\n  end\n  return x + 1\nend");
    // Assigning an outer `let mut` from inside a block still works.
    ok("cell main() -> Int\n  let mut x = 0\n  if true\n    x = 5\n  end\n  return x\nend");
}

#[test]
fn too_few_call_arguments_are_rejected() {
    let e = err_of(
        "cell add(a: Int, b: Int) -> Int\n  return a + b\nend\ncell main() -> Int\n  return add(1)\nend",
    );
    assert!(
        e.contains("argcount") || e.contains("wrong number of arguments"),
        "{e}"
    );
    // defaults, named arguments and variadics are still fine
    ok("cell add(a: Int, b: Int = 2) -> Int\n  return a + b\nend\ncell main() -> Int\n  return add(1)\nend");
    ok("cell add(a: Int, b: Int) -> Int\n  return a + b\nend\ncell main() -> Int\n  return add(b: 1, a: 2)\nend");
    ok("cell sum(...xs: Int) -> Int\n  return 0\nend\ncell main() -> Int\n  return sum()\nend");
}

#[test]
fn record_constructor_requires_all_fields() {
    let rec = "record P\n  x: Int\n  y: Int\nend\n";
    let e = err_of(&format!(
        "{rec}cell main() -> Int\n  let p = P(x: 1)\n  return p.y\nend"
    ));
    assert!(e.contains("field 'y'") && e.contains("missing"), "{e}");
    ok(&format!(
        "{rec}cell main() -> Int\n  let p = P(x: 1, y: 2)\n  return p.y\nend"
    ));
    ok("record Q\n  x: Int\n  y: Int = 7\nend\ncell main() -> Int\n  let q = Q(x: 1)\n  return q.y\nend");
}

#[test]
fn union_to_member_is_rejected_but_reordering_and_widening_are_fine() {
    let e = err_of(
        "cell f(x: Int | String) -> Int\n  return x\nend\ncell main() -> Int\n  return f(1)\nend",
    );
    assert!(e.contains("int | string"), "{e}");
    ok("cell g(x: Int | String) -> Int | String | Null\n  return x\nend\ncell main() -> Int\n  return 0\nend");
    ok("cell h(x: Int | String) -> String | Int\n  return x\nend\ncell main() -> Int\n  return 0\nend");
    // A member passed where the union is expected stays valid.
    ok("cell k(x: Int | String) -> Int\n  return 1\nend\ncell main() -> Int\n  return k(1) + k(\"a\")\nend");
}
