use lumen_compiler::compile;

fn markdown_from_code(source: &str) -> String {
    format!("# pattern-test\n\n```lumen\n{}\n```\n", source.trim())
}

#[test]
fn nested_variant_in_variant() {
    let source = r#"
enum Inner
  Value(n: Int)
  Empty
end

enum Outer
  Some(inner: Inner)
  None
end

cell extract(o: Outer) -> Int
  match o
    Some(Value(n)) -> return n
    _ -> return 0
  end
end

cell main() -> Int
  return extract(Some(inner: Value(n: 42)))
end
"#;
    let md = markdown_from_code(source);
    if let Err(err) = compile(&md) {
        panic!("nested_variant_in_variant failed:\n{}", err);
    }
}

#[test]
fn deeply_nested_destructure() {
    let source = r#"
enum Result
  Ok(val: Int)
  Err(msg: String)
end

enum Option
  Some(result: Result)
  None
end

cell unwrap_deep(opt: Option) -> Int
  match opt
    Some(Ok(val)) -> return val
    _ -> return -1
  end
end

cell main() -> Int
  return unwrap_deep(Some(result: Ok(val: 99)))
end
"#;
    let md = markdown_from_code(source);
    if let Err(err) = compile(&md) {
        panic!("deeply_nested_destructure failed:\n{}", err);
    }
}
