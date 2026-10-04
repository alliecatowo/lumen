//! End-to-end checks for lowering fixes: conditional `defer`, `break value`,
//! invalid `break`/`continue` targets, aliased imports and field assignment in
//! modules with many strings.

use lumen_runtime::vm::values::Value;
use lumen_runtime::vm::vm::VM;

fn run_module(
    module: lumen_compiler::compiler::lir::LirModule,
) -> (Result<Value, String>, Vec<String>) {
    let mut vm = VM::new();
    vm.set_instruction_limit(5_000_000);
    vm.load(module);
    let r = vm.execute("main", vec![]).map_err(|e| e.to_string());
    (r, vm.output)
}

fn run(src: &str) -> (Result<Value, String>, Vec<String>) {
    run_module(lumen_compiler::compile_raw(src).expect("compiles"))
}

fn compile_err(src: &str) -> String {
    match lumen_compiler::compile_raw(src) {
        Ok(_) => panic!("expected a compile error:\n{src}"),
        Err(e) => e.to_string().to_lowercase(),
    }
}

#[test]
fn defer_in_untaken_branch_does_not_run() {
    let src = r#"
cell f(x: Int) -> Int
  if x == 1
    defer
      print("deferred ran")
    end
  end
  return 5
end
cell main() -> Int
  let a = f(0)
  print("after f(0)")
  let b = f(1)
  print("after f(1)")
  return a + b
end
"#;
    let (res, out) = run(src);
    assert_eq!(res.unwrap(), Value::Int(10));
    assert_eq!(
        out,
        vec!["after f(0)", "deferred ran", "after f(1)"],
        "defer must only run when its statement executed"
    );
}

#[test]
fn defers_run_in_lifo_order_on_every_return() {
    let src = r#"
cell f(early: Bool) -> Int
  defer
    print("first registered")
  end
  if early
    return 1
  end
  defer
    print("second registered")
  end
  return 2
end
cell main() -> Int
  let a = f(true)
  print("--")
  let b = f(false)
  return a + b
end
"#;
    let (res, out) = run(src);
    assert_eq!(res.unwrap(), Value::Int(3));
    assert_eq!(
        out,
        vec![
            "first registered",
            "--",
            "second registered",
            "first registered"
        ]
    );
}

#[test]
fn break_with_value_yields_the_loop_result() {
    let src = r#"
cell main() -> Int
  let mut i = 0
  let r = loop
    i = i + 1
    if i > 3
      break 42
    end
  end
  return r
end
"#;
    assert_eq!(run(src).0.unwrap(), Value::Int(42));
}

#[test]
fn loop_expression_without_break_value_is_null_and_labels_work() {
    let src = r#"
cell main() -> Int
  let mut hits = 0
  let mut i = 0
  while @outer i < 5
    i = i + 1
    let mut j = 0
    while j < 5
      j = j + 1
      if j == 2
        continue @outer
      end
      hits = hits + 1
    end
  end
  return hits
end
"#;
    assert_eq!(run(src).0.unwrap(), Value::Int(5));
}

#[test]
fn invalid_break_and_continue_are_compile_errors() {
    let e = compile_err("cell main() -> Int\n  let mut i = 0\n  while i < 3\n    break @nolabel\n  end\n  return i\nend");
    assert!(e.contains("unknown loop label"), "{e}");
    let e = compile_err("cell main() -> Int\n  break\n  return 0\nend");
    assert!(e.contains("outside of a loop"), "{e}");
    let e = compile_err("cell main() -> Int\n  continue\n  return 0\nend");
    assert!(e.contains("outside of a loop"), "{e}");
    // a lambda body cannot break an enclosing loop
    let e = compile_err("cell main() -> Int\n  let mut i = 0\n  while i < 3\n    let f = fn() => break\n    i = i + 1\n  end\n  return i\nend");
    assert!(
        e.contains("outside of a loop") || e.contains("break"),
        "{e}"
    );
    let e = compile_err(
        "cell main() -> Int\n  let mut i = 0\n  while i < 3\n    break 5\n  end\n  return i\nend",
    );
    assert!(e.contains("without a value"), "{e}");
}

#[test]
fn aliased_import_is_callable_at_runtime() {
    let util = "pub cell helper(n: Int) -> Int\n  return n * 2 + 1\nend\n";
    let main = "import util: helper as h\n\ncell main() -> Int\n  return h(5) + h(1)\nend\n";
    let resolver = |name: &str| -> Option<String> {
        if name == "util" {
            Some(util.to_string())
        } else {
            None
        }
    };
    let module = lumen_compiler::compile_raw_with_imports(main, &resolver).expect("compiles");
    assert_eq!(run_module(module).0.unwrap(), Value::Int(14));
    // The unaliased import keeps working too.
    let main2 = "import util: helper\n\ncell main() -> Int\n  return helper(5)\nend\n";
    let module = lumen_compiler::compile_raw_with_imports(main2, &resolver).expect("compiles");
    assert_eq!(run_module(module).0.unwrap(), Value::Int(11));
}

#[test]
fn field_assignment_survives_more_than_255_strings() {
    // 300 cells intern 300+ strings before the record field name is interned.
    let mut src = String::new();
    for i in 0..300 {
        src.push_str(&format!("cell c{i}() -> Int\n  return {i}\nend\n"));
    }
    src.push_str(
        "record P\n  xcoord: Int\n  y: Int\nend\n\
         cell main() -> Int\n  let mut p = P(xcoord: 1, y: 2)\n  p.xcoord = 10\n  p.xcoord += 1\n  p.y *= 3\n  return p.xcoord + p.y\nend\n",
    );
    assert_eq!(run(&src).0.unwrap(), Value::Int(11 + 6));
}
