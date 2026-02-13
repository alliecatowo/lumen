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
  email: String where definitely_not_a_builtin(email)
end
"#,
            expect_substring: "unknown constraint function",
        },
        ErrorCase {
            id: "effect_contract_violation",
            source: r#"
use tool http.get as HttpGet
grant HttpGet

cell fetch() -> Int / {http}
  return 1
end

cell main() -> Int / {emit}
  return fetch()
end
"#,
            expect_substring: "effectcontractviolation",
        },
    ];

    for case in &cases {
        assert_compile_err(case);
    }
}

#[test]
fn spec_v2_addendum_coverage_targets() {
    let cases = [
        CompileCase {
            id: "effect_rows",
            source: r#"
use tool http.get as HttpGet
grant HttpGet

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
record Response
  status: Int
  body: String
end

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
use tool llm.chat as Chat
grant Chat

agent Assistant
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
cell main() -> Bool / {approve, emit}
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
use tool http.get as HttpGet
grant HttpGet

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

// ─── Regression tests for known bugs ───

#[test]
fn regression_while_loop_backward_jumps() {
    // Regression: signed jump offsets were truncated to 24-bit unsigned, making
    // backward jumps wrap to large positive offsets. Must use sax/sax_val.
    let case = CompileCase {
        id: "while_loop_backward_jump",
        source: r#"
cell main() -> Int
  let mut i = 0
  let mut sum = 0
  while i < 5
    sum = sum + i
    i = i + 1
  end
  return sum
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn regression_match_literal_no_clobber_r0() {
    // Regression: Eq(0, subj, lit) clobbered register 0 (often a parameter)
    // with the boolean result. Fix allocates a temp register instead.
    let case = CompileCase {
        id: "match_literal_no_clobber_r0",
        source: r#"
cell classify(x: Int) -> String
  match x
    1 -> return "one"
    2 -> return "two"
    _ -> return "other"
  end
end

cell main() -> String
  return classify(2)
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn regression_builtin_any_in_string_concat() {
    // Regression: builtins return Type::Any, and Any + String with Add fell
    // through to Int inference. Fix checks for Any first.
    let case = CompileCase {
        id: "builtin_any_string_concat",
        source: r#"
cell main() -> String
  let x = to_string(42)
  return "result: " + x
end
"#,
    };
    assert_compile_ok(&case);
}

// ─── Compile-ok tests for language features ───

#[test]
fn feature_record_definition() {
    let case = CompileCase {
        id: "record_with_fields",
        source: r#"
record Point
  x: Int
  y: Int
end

record Person
  name: String
  age: Int = 0
end

cell main() -> Int
  let p = Point(x: 3, y: 4)
  return p.x + p.y
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn feature_enum_with_variants() {
    let case = CompileCase {
        id: "enum_variants",
        source: r#"
enum Shape
  Circle(radius: Float)
  Rectangle(width: Float, height: Float)
  Point
end

cell main() -> Int
  let s = Point
  return 1
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn feature_if_else_expression() {
    let case = CompileCase {
        id: "if_else_expr",
        source: r#"
cell abs(x: Int) -> Int
  if x < 0
    return 0 - x
  else
    return x
  end
end

cell main() -> Int
  return abs(-5)
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn feature_for_loop_over_list() {
    let case = CompileCase {
        id: "for_loop_list",
        source: r#"
cell main() -> Int
  let mut total = 0
  for x in [10, 20, 30]
    total += x
  end
  return total
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn feature_string_interpolation() {
    let case = CompileCase {
        id: "string_interpolation",
        source: r#"
cell main() -> String
  let name = "world"
  let count = 42
  return "Hello, {name}! Count: {count}"
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn feature_null_coalesce() {
    let case = CompileCase {
        id: "null_coalesce_operator",
        source: r#"
cell main() -> Int
  let x: Int | Null = null
  return x ?? 99
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn feature_compound_assignment() {
    let case = CompileCase {
        id: "compound_assignment_ops",
        source: r#"
cell main() -> Int
  let mut x = 10
  x += 5
  x -= 3
  x *= 2
  return x
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn feature_float_arithmetic() {
    let case = CompileCase {
        id: "float_arithmetic",
        source: r#"
cell main() -> Float
  let a = 3.14
  let b = 2.0
  return a * b
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn feature_list_operations() {
    let case = CompileCase {
        id: "list_append_and_length",
        source: r#"
cell main() -> Int
  let xs = [1, 2, 3]
  let ys = append(xs, 4)
  return length(ys)
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn feature_pattern_matching_wildcard() {
    let case = CompileCase {
        id: "pattern_matching_wildcard",
        source: r#"
cell describe(x: Int) -> String
  match x
    0 -> return "zero"
    1 -> return "one"
    _ -> return "many"
  end
end

cell main() -> String
  return describe(99)
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn feature_closure_lambda() {
    let case = CompileCase {
        id: "closure_lambda",
        source: r#"
cell apply(f: fn(Int) -> Int, x: Int) -> Int
  return f(x)
end

cell main() -> Int
  let double = fn(n: Int) => n * 2
  return apply(double, 21)
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn feature_effect_declaration() {
    let case = CompileCase {
        id: "effect_declaration_compile",
        source: r#"
effect storage
  cell save(key: String, value: String) -> Bool
  cell load(key: String) -> String | Null
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn feature_loop_continue() {
    let case = CompileCase {
        id: "loop_continue",
        source: r#"
cell main() -> Int
  let mut sum = 0
  let mut i = 0
  while i < 10
    i += 1
    if i % 2 == 0
      continue
    end
    sum += i
  end
  return sum
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn feature_multiple_cells() {
    let case = CompileCase {
        id: "multiple_cells",
        source: r#"
cell add(a: Int, b: Int) -> Int
  return a + b
end

cell mul(a: Int, b: Int) -> Int
  return a * b
end

cell main() -> Int
  return add(mul(3, 4), mul(5, 6))
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn feature_nested_if() {
    let case = CompileCase {
        id: "nested_if",
        source: r#"
cell classify(x: Int) -> String
  if x > 0
    if x > 100
      return "large"
    else
      return "small"
    end
  else
    return "non-positive"
  end
end

cell main() -> String
  return classify(50)
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn feature_string_builtins() {
    let case = CompileCase {
        id: "string_builtins",
        source: r#"
cell main() -> String
  let s = "  Hello World  "
  let trimmed = trim(s)
  let upper = upper(trimmed)
  let lower = lower(trimmed)
  return upper + " " + lower
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn feature_map_literal() {
    let case = CompileCase {
        id: "map_literal",
        source: r#"
cell main() -> map[String, Int]
  return {"a": 1, "b": 2, "c": 3}
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn feature_tuple_literal() {
    let case = CompileCase {
        id: "tuple_literal",
        source: r#"
cell main() -> tuple[Int, String, Bool]
  return (42, "hello", true)
end
"#,
    };
    assert_compile_ok(&case);
}

// ─── Compile-error tests ───

#[test]
fn error_undefined_variable_reference() {
    let case = ErrorCase {
        id: "undefined_var_ref",
        source: r#"
cell main() -> Int
  return unknown_var + 1
end
"#,
        expect_substring: "undefinedvar",
    };
    assert_compile_err(&case);
}

#[test]
fn error_type_mismatch_in_return() {
    let case = ErrorCase {
        id: "type_mismatch_return",
        source: r#"
cell main() -> Int
  return "this is not an int"
end
"#,
        expect_substring: "mismatch",
    };
    assert_compile_err(&case);
}

#[test]
fn error_undefined_type_in_record() {
    let case = ErrorCase {
        id: "undefined_type_record",
        source: r#"
record Broken
  x: NonExistentType
end
"#,
        expect_substring: "undefinedtype",
    };
    assert_compile_err(&case);
}

#[test]
fn error_invalid_effect_contract() {
    // Calling a cell with http effect from a cell that only declares emit effect
    let case = ErrorCase {
        id: "invalid_effect_contract",
        source: r#"
use tool http.get as HttpGet
grant HttpGet

cell fetch() -> Int / {http}
  return 1
end

cell main() -> Int / {emit}
  return fetch()
end
"#,
        expect_substring: "effectcontractviolation",
    };
    assert_compile_err(&case);
}

#[test]
fn error_non_exhaustive_match() {
    let case = ErrorCase {
        id: "non_exhaustive_match_enum",
        source: r#"
enum Direction
  North
  South
  East
  West
end

cell main(d: Direction) -> Int
  match d
    North() -> return 1
    South() -> return 2
  end
end
"#,
        expect_substring: "incomplete match",
    };
    assert_compile_err(&case);
}

#[test]
fn error_invalid_constraint_fn() {
    let case = ErrorCase {
        id: "invalid_where_constraint_fn",
        source: r#"
record Config
  value: Int where nonexistent_check(value)
end
"#,
        expect_substring: "unknown constraint function",
    };
    assert_compile_err(&case);
}
