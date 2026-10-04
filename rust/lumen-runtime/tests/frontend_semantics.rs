//! End-to-end checks for front-end fixes: contextual keywords as ordinary
//! names, multi-line strings inside expressions, and similar parser/lexer
//! behaviour that only shows up when the program runs.

use lumen_runtime::vm::values::Value;
use lumen_runtime::vm::vm::VM;

fn run(src: &str) -> (Result<Value, String>, Vec<String>) {
    let module = lumen_compiler::compile_raw(src).unwrap_or_else(|e| panic!("{e}\n{src}"));
    let mut vm = VM::new();
    vm.set_instruction_limit(5_000_000);
    vm.load(module);
    let r = vm.execute("main", vec![]).map_err(|e| e.to_string());
    (r, vm.output)
}

#[test]
fn user_cell_named_like_an_addon_keyword_is_called() {
    let src = r#"
cell observe(x: Int) -> Int
  print("observe called with {x}")
  return x
end
cell approve(x: Int) -> Int
  return x + 1
end
cell main() -> Int
  observe(5)
  return approve(1)
end
"#;
    let (res, out) = run(src);
    assert_eq!(res.unwrap(), Value::Int(2));
    assert_eq!(out, vec!["observe called with 5"]);
}

#[test]
fn agent_and_confirm_can_be_variable_names() {
    let src = r#"
cell main() -> Int
  let agent = 10
  let confirm = 3
  return agent + confirm + 1
end
"#;
    assert_eq!(run(src).0.unwrap(), Value::Int(14));
}

#[test]
fn multiline_string_inside_an_expression_keeps_the_logical_line() {
    let src =
        "cell main() -> String\n  return join([\"x\", \"\"\"\n a\n b\n \"\"\"], \",\")\nend\n";
    assert_eq!(
        run(src).0.unwrap(),
        Value::String(lumen_runtime::vm::values::StringRef::Owned("x,a\nb".into()))
    );
}

#[test]
fn negative_and_null_patterns_match() {
    let src = r#"
cell classify(x: Int) -> String
  match x
    -1 -> return "minus one"
    0 -> return "zero"
    -10..=-2 -> return "very negative"
    _ -> return "other"
  end
end
cell nullable(x: Int?) -> String
  match x
    null -> return "nothing"
    _ -> return "something"
  end
end
cell main() -> String
  print(classify(-1))
  print(classify(0))
  print(classify(-5))
  print(classify(7))
  print(nullable(null))
  print(nullable(3))
  return "done"
end
"#;
    let (res, out) = run(src);
    res.unwrap();
    assert_eq!(
        out,
        vec![
            "minus one",
            "zero",
            "very negative",
            "other",
            "nothing",
            "something"
        ]
    );
}

#[test]
fn nested_tuple_index_chain_is_not_a_float() {
    let src = "cell main() -> Int\n  let t = ((1, 2), 3)\n  return t.0.1 + t.1\nend\n";
    assert_eq!(run(src).0.unwrap(), Value::Int(5));
}

#[test]
fn chained_comparison_evaluates_each_operand_once() {
    let src = r#"
cell g() -> Int
  print("g called")
  return 5
end
cell main() -> Int
  let mut hits = 0
  if 1 < g() < 10
    hits = hits + 1
  end
  if 1 < 2 < 3 < 4
    hits = hits + 10
  end
  if 1 < 5 < 3 < 4
    hits = hits + 100
  end
  if 3 >= g() >= 5
    hits = hits + 1000
  end
  return hits
end
"#;
    let (res, out) = run(src);
    assert_eq!(res.unwrap(), Value::Int(11));
    assert_eq!(out, vec!["g called", "g called"], "g() once per chain");
}
