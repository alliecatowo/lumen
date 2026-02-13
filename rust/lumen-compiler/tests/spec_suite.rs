use lumen_compiler::compile;

struct CompileCase {
    id: &'static str,
    source: &'static str,
}

struct ErrorCase {
    id: &'static str,
    source: &'static str,
    expect_substring: &'static str,
}

fn markdown_from_code(source: &str) -> String {
    format!("# spec-case\n\n```lumen\n{}\n```\n", source.trim())
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

fn assert_compile_err(case: &ErrorCase) {
    let md = markdown_from_code(case.source);
    match compile(&md) {
        Ok(_) => panic!(
            "case '{}' unexpectedly compiled\n--- source ---\n{}",
            case.id, case.source
        ),
        Err(err) => {
            let msg = err.to_string().to_lowercase();
            let expect = case.expect_substring.to_lowercase();
            assert!(
                msg.contains(&expect),
                "case '{}' error mismatch\nexpected substring: {}\nactual: {}",
                case.id,
                case.expect_substring,
                err
            );
        }
    }
}

#[test]
fn spec_markdown_directives_compile() {
    let src = r#"
@lumen 1
@package "spec.tests"

# directives

```lumen
cell main() -> Int
  return 42
end
```
"#;
    compile(src).expect("directives markdown should compile");
}

#[test]
fn spec_core_declarations_compile() {
    let cases = [
        CompileCase {
            id: "record_defaults_constraints",
            source: r#"
record User
  name: String = "anon"
  age: Int where age >= 0
end

cell main() -> User
  return User(name: "allie", age: 20)
end
"#,
        },
        CompileCase {
            id: "enum_and_match_exhaustive",
            source: r#"
enum Color
  Red
  Green
  Blue
end

cell main() -> Int
  let c = Green
  match c
    Red -> return 1
    Green -> return 2
    Blue -> return 3
  end
end
"#,
        },
        CompileCase {
            id: "type_alias_generic_decl",
            source: r#"
type Box[T] = map[String, T]

cell main() -> Int
  return 1
end
"#,
        },
        CompileCase {
            id: "trait_and_impl",
            source: r#"
trait Greeter
  cell greet(name: String) -> String
    return name
  end
end

impl Greeter for String
  cell greet(name: String) -> String
    return name
  end
end

cell main() -> String
  return "ok"
end
"#,
        },
        CompileCase {
            id: "imports_consts",
            source: r#"
import foo.bar: Baz as Qux, Quux
import std.core: *

const PI: Float = 3.14

cell main() -> Int
  return 1
end
"#,
        },
        CompileCase {
            id: "tools_and_grants",
            source: r#"
use tool http.get as HttpGet
grant HttpGet
  domain "*.example.com"
  timeout_ms 5000

cell main() -> String
  let response = HttpGet(url: "https://api.example.com")
  return string(response)
end
"#,
        },
        CompileCase {
            id: "union_and_null_types",
            source: r#"
cell maybe(flag: Bool) -> Int | Null
  if flag
    return 1
  end
  return null
end

cell main() -> Int | Null
  return maybe(true)
end
"#,
        },
    ];

    for case in &cases {
        assert_compile_ok(case);
    }
}

#[test]
fn spec_core_expressions_compile() {
    let cases = [
        CompileCase {
            id: "numeric_literals_and_ops",
            source: r#"
cell main() -> Int
  let a = 0b1010
  let b = 0o12
  let c = 0x0A
  let d = 1_000
  return a + b + c + d
end
"#,
        },
        CompileCase {
            id: "strings_raw_bytes_interp",
            source: r#"
cell main() -> String
  let regular = "hello {1 + 1}"
  let raw = r"line\nliteral"
  let blob = b"6869"
  return regular + " " + raw + " " + string(blob)
end
"#,
        },
        CompileCase {
            id: "collections_literals",
            source: r#"
cell main() -> tuple[list[Int], map[String, Int], set[Int], tuple[Int, String]]
  let xs = [1, 2, 3]
  let m = {"a": 1, "b": 2}
  let s = set[1, 2, 2, 3]
  let t = (1, "x")
  return (xs, m, s, t)
end
"#,
        },
        CompileCase {
            id: "lambda_fn_type_and_call",
            source: r#"
cell apply(f: fn(Int) -> Int, x: Int) -> Int
  return f(x)
end

cell main() -> Int
  let f = fn(n: Int) => n + 1
  return apply(f, 41)
end
"#,
        },
        CompileCase {
            id: "default_params_named_args",
            source: r#"
cell greet(name: String = "world") -> String
  return "hi " + name
end

cell main() -> String
  return greet(name: "lumen")
end
"#,
        },
        CompileCase {
            id: "range_and_comprehension",
            source: r#"
cell main() -> list[Int]
  return [x * 2 for x in 0..4 if x >= 0]
end
"#,
        },
        CompileCase {
            id: "null_operators",
            source: r#"
record Box
  value: Int
end

cell main() -> Int
  let b: Box | Null = Box(value: 7)
  return b?.value ?? 0
end
"#,
        },
        CompileCase {
            id: "try_operator_with_result",
            source: r#"
cell may_fail(flag: Bool) -> result[Int, String]
  if flag
    return ok(1)
  end
  return err("bad")
end

cell main() -> result[Int, String]
  let v = may_fail(true)?
  return ok(v)
end
"#,
        },
        CompileCase {
            id: "role_and_expect_schema",
            source: r#"
record Invoice
  id: String
end

cell main() -> Invoice
  let prompt = role user: "invoice id: 123"
  return prompt expect schema Invoice
end
"#,
        },
        CompileCase {
            id: "async_await_parse",
            source: r#"
async cell fetch_value() -> Int
  return 1
end

cell main() -> Int
  return await fetch_value()
end
"#,
        },
    ];

    for case in &cases {
        assert_compile_ok(case);
    }
}

#[test]
fn spec_core_control_flow_compile() {
    let cases = [
        CompileCase {
            id: "while_loop_continue_break",
            source: r#"
cell main() -> Int
  let mut i = 0
  while i < 10
    i += 1
    if i == 3
      continue
    end
    if i == 5
      break
    end
  end
  return i
end
"#,
        },
        CompileCase {
            id: "for_loop_sum",
            source: r#"
cell main() -> Int
  let mut sum = 0
  for x in [1, 2, 3]
    sum += x
  end
  return sum
end
"#,
        },
        CompileCase {
            id: "loop_and_break",
            source: r#"
cell main() -> Int
  let mut i = 0
  loop
    i += 1
    if i >= 4
      break
    end
  end
  return i
end
"#,
        },
        CompileCase {
            id: "match_result_variants",
            source: r#"
cell inspect(x: result[Int, String]) -> Int
  match x
    ok(v) -> return v
    err(e) -> return length(e)
  end
end

cell main() -> Int
  return inspect(ok(5))
end
"#,
        },
    ];

    for case in &cases {
        assert_compile_ok(case);
    }
}

#[test]
fn spec_negative_compile_diagnostics() {
    let cases = [
        ErrorCase {
            id: "undefined_type",
            source: r#"
record Foo
  x: MissingType
end
"#,
            expect_substring: "undefinedtype",
        },
        ErrorCase {
            id: "undefined_variable",
            source: r#"
cell main() -> Int
  return missing_var
end
"#,
            expect_substring: "undefinedvar",
        },
        ErrorCase {
            id: "non_exhaustive_enum_match",
            source: r#"
enum Color
  Red
  Green
end

cell main(c: Color) -> Int
  match c
    Red() -> return 1
  end
end
"#,
            expect_substring: "incomplete match",
        },
        ErrorCase {
            id: "return_type_mismatch",
            source: r#"
cell main() -> Int
  return "not an int"
end
"#,
            expect_substring: "mismatch",
        },
        ErrorCase {
            id: "invalid_constraint_function",
            source: r#"
record User
  email: String where is_valid_email(email)
end
"#,
            expect_substring: "unknown constraint function",
        },
    ];

    for case in &cases {
        assert_compile_err(case);
    }
}

#[test]
#[ignore = "Addendum V2 coverage target; enable as features land"]
fn spec_v2_addendum_coverage_targets() {
    let cases = [
        CompileCase {
            id: "effect_rows",
            source: r#"
cell fetch(url: String) -> Bytes / {http}
  return HttpGet(url: url).body
end
"#,
        },
        CompileCase {
            id: "effect_declaration_and_binding",
            source: r#"
effect database
  cell query(sql: String) -> list[Json]
end

use tool postgres.query as DbQuery
bind effect database.query to DbQuery
"#,
        },
        CompileCase {
            id: "effect_handler",
            source: r#"
handler MockHttp
  handle http.get(url: String) -> Response
    return Response(status: 200, body: "ok")
  end
end
"#,
        },
        CompileCase {
            id: "agent_declaration",
            source: r#"
agent Assistant
  role:
    You are helpful.
  end

  cell respond(input: String) -> String / {llm}
    return Chat(role user: input)
  end
end
"#,
        },
        CompileCase {
            id: "orchestration",
            source: r#"
orchestration Team
  coordinator: Manager
  workers: [Researcher, Writer]
end
"#,
        },
        CompileCase {
            id: "state_machine",
            source: r#"
machine TicketFlow
  initial: Start
  state Start
    on_enter() / {trace}
      transition Done()
    end
  end
  state Done
    terminal: true
  end
end
"#,
        },
        CompileCase {
            id: "memory_decl",
            source: r#"
memory ConversationBuffer: short_term
  window: 20
end
"#,
        },
        CompileCase {
            id: "approval_checkpoint_escalate_confirm",
            source: r#"
cell main() -> Bool / {approve}
  approve "proceed?"
    on_approve:
      continue
  end
  checkpoint "c1"
    state: {x: 1}
  end
  escalate "need human"
    on_timeout(1h):
      return false
  end
  return confirm "ok?"
end
"#,
        },
        CompileCase {
            id: "guardrail_decl",
            source: r#"
guardrail PIIProtection
  on_output(data: String) -> result[String, GuardrailViolation] / {pure}
    return ok(data)
  end
end
"#,
        },
        CompileCase {
            id: "eval_decl",
            source: r#"
eval InvoiceExtractionAccuracy
  dataset: "test/invoices.jsonl"
  agent: InvoiceExtractor
end
"#,
        },
        CompileCase {
            id: "versioned_schema",
            source: r#"
@version(1)
record Invoice
  id: String
end
"#,
        },
        CompileCase {
            id: "observe_block",
            source: r#"
cell main() -> Int / {trace}
  observe "batch"
    metrics:
      counter items_processed
    end
  in
    return 1
  end
end
"#,
        },
        CompileCase {
            id: "active_and_view_patterns",
            source: r#"
@active_pattern
cell Even(n: Int) -> Bool = n % 2 == 0

match 2
  Even() -> 1
  _ -> 0
end
"#,
        },
        CompileCase {
            id: "distributed_execution_annotations",
            source: r#"
@remote("us-east-1")
cell fetch_data() -> Int / {http}
  return 1
end
"#,
        },
    ];

    for case in &cases {
        assert_compile_ok(case);
    }
}

#[test]
#[ignore = "Unimplemented V1 sections from SPEC.md that should eventually be covered"]
fn spec_v1_unimplemented_targets() {
    let cases = [
        CompileCase {
            id: "parallel_for_and_select",
            source: r#"
cell main() -> Int / {async}
  let values = await parallel for i in 0..10
    i * 2
  end
  return values.length
end
"#,
        },
        CompileCase {
            id: "channels",
            source: r#"
cell main() -> Int / {async}
  let ch = channel[Int](capacity: 8)
  ch.send(1)
  return ch.recv()
end
"#,
        },
        CompileCase {
            id: "macros",
            source: r#"
macro dbg(expr)
  emit(expr)
  expr
end
"#,
        },
        CompileCase {
            id: "comptime",
            source: r#"
const LIMIT: Int = comptime {
  if @target == "wasm"
    10
  else
    100
  end
}
"#,
        },
        CompileCase {
            id: "wasm_export",
            source: r#"
@target("wasm")
@export("sum")
cell add(a: Int, b: Int) -> Int
  return a + b
end
"#,
        },
        CompileCase {
            id: "ffi",
            source: r#"
extern cell c_strlen(ptr: Bytes) -> Int
"#,
        },
    ];

    for case in &cases {
        assert_compile_ok(case);
    }
}
