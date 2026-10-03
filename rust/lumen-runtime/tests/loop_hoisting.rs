//! Regression tests for loop-invariant constant hoisting in the lowerer.
//!
//! The pass used to remember stale loop extents after inserting hoisted loads
//! and re-pointed outside jumps past them, so nested loops read clobbered
//! constants (fannkuch never terminated, `j + 1` became `j + 0`).

use lumen_runtime::vm::values::Value;
use lumen_runtime::vm::vm::VM;

fn run_raw(src: &str, budget: u64) -> (Result<Value, String>, Vec<String>) {
    let module = lumen_compiler::compile_raw(src).expect("source compiles");
    let mut vm = VM::new();
    vm.set_instruction_limit(budget);
    vm.load(module);
    let r = vm.execute("main", vec![]).map_err(|e| e.to_string());
    (r, vm.output)
}

/// The cross-language fannkuch benchmark shrunk to N = `n`.
fn fannkuch_source(n: usize) -> String {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../bench/cross-language/fannkuch/fannkuch.lm"),
    )
    .expect("fannkuch.lm");
    let zeros = format!("[{}]", vec!["0"; n].join(", "));
    let ident = format!(
        "[{}]",
        (0..n).map(|i| i.to_string()).collect::<Vec<_>>().join(", ")
    );
    src.replace("let n = 10", &format!("let n = {n}"))
        .replace("[0, 0, 0, 0, 0, 0, 0, 0, 0, 0]", &zeros)
        .replace("[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]", &ident)
        .replace("Pfannkuchen(10)", &format!("Pfannkuchen({n})"))
}

#[test]
fn fannkuch_terminates_with_the_reference_answer() {
    for (n, checksum, flips) in [(5usize, 11, 7), (6, 49, 10), (7, 228, 16)] {
        let (res, out) = run_raw(&fannkuch_source(n), 200_000_000);
        res.unwrap_or_else(|e| panic!("fannkuch({n}) failed: {e}"));
        let text = out.join("\n");
        assert!(text.contains(&checksum.to_string()), "n={n}: {text}");
        assert!(
            text.contains(&format!("Pfannkuchen({n}) = {flips}")),
            "n={n}: {text}"
        );
    }
}

#[test]
fn nested_loops_keep_constants_intact() {
    let src = r#"
cell main() -> Int
  let mut total = 0
  let mut a = 0
  while a < 3
    let mut b = 0
    while b < 3
      let mut c = 0
      while c < 2
        total = total + c + 1
        c = c + 1
      end
      total = total + b + 1
      b = b + 1
    end
    total = total + 100
    print("outer")
    a = a + 1
  end
  return total
end
"#;
    let (res, out) = run_raw(src, 1_000_000);
    // per b: 1+2 (inner) + (b+1); b=0..2 -> 9 + 6 = 15; plus 100 => 115 per a; x3.
    assert_eq!(res.unwrap(), Value::Int(345));
    assert_eq!(out.iter().filter(|l| l.as_str() == "outer").count(), 3);
}

#[test]
fn zero_iteration_and_conditional_loops_do_not_see_hoisted_values() {
    let src = r#"
cell main() -> Int
  let mut x = 0
  let mut i = 0
  while i < 4
    if i == 2
      x = 5
    end
    print(x)
    i = i + 1
  end
  let mut y = 7
  let mut j = 10
  while j < 3
    y = 9
    j = j + 1
  end
  print(y)
  let mut z = 1
  let mut k = 0
  while k < 3
    print(z)
    z = 8
    k = k + 1
  end
  return x + y + z
end
"#;
    let (res, out) = run_raw(src, 1_000_000);
    assert_eq!(res.unwrap(), Value::Int(5 + 7 + 8));
    assert_eq!(out, vec!["0", "0", "5", "5", "7", "1", "8", "8"]);
}
