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
bind effect http to HttpGet
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
bind effect llm to Chat
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

// ─── Deep regression tests: LIR-level inspection ───

#[test]
fn regression_while_loop_backward_jump_is_negative() {
    // Verify that while loops emit a negative (backward) Jmp offset.
    // The bug was that backward jumps used unsigned ax() instead of sax(),
    // causing the offset to wrap to a large positive value.
    use lumen_compiler::compiler::lir::OpCode;

    let md = markdown_from_code(
        r#"
cell main() -> Int
  let mut x = 0
  let mut i = 0
  while i < 5
    x = x + 1
    i = i + 1
  end
  x
end
"#,
    );
    let module = compile(&md).expect("while loop should compile");
    let main_cell = module
        .cells
        .iter()
        .find(|c| c.name == "main")
        .expect("main cell should exist");

    // Find any Jmp instruction with a negative signed offset (backward jump)
    let has_backward_jump = main_cell.instructions.iter().any(|inst| {
        inst.op == OpCode::Jmp && inst.sax_val() < 0
    });
    assert!(
        has_backward_jump,
        "while loop should contain a backward jump (negative sax_val), instructions: {:?}",
        main_cell.instructions
    );
}

#[test]
fn regression_while_loop_countdown_has_backward_jump() {
    // Another variant: counting down. Ensures backward jump works for countdown loops.
    use lumen_compiler::compiler::lir::OpCode;

    let md = markdown_from_code(
        r#"
cell main() -> Int
  let mut i = 10
  while i > 0
    i = i - 1
  end
  i
end
"#,
    );
    let module = compile(&md).expect("countdown should compile");
    let main_cell = module
        .cells
        .iter()
        .find(|c| c.name == "main")
        .expect("main cell should exist");

    let backward_jumps: Vec<_> = main_cell
        .instructions
        .iter()
        .filter(|inst| inst.op == OpCode::Jmp && inst.sax_val() < 0)
        .collect();
    assert!(
        !backward_jumps.is_empty(),
        "countdown while loop should contain a backward jump"
    );
}

#[test]
fn regression_match_literal_does_not_clobber_param_register() {
    // The bug was Eq(0, subj, lit) writing the bool result into register 0,
    // clobbering the first parameter. Verify no Eq instruction targets r0.
    use lumen_compiler::compiler::lir::OpCode;

    let md = markdown_from_code(
        r#"
cell check(x: Int) -> String
  match x
    1 -> "one"
    2 -> "two"
    _ -> "other"
  end
end

cell main() -> String
  return check(2)
end
"#,
    );
    let module = compile(&md).expect("match should compile");
    let check_cell = module
        .cells
        .iter()
        .find(|c| c.name == "check")
        .expect("check cell should exist");

    // Verify that no Eq instruction writes to register 0 (the parameter register)
    for inst in &check_cell.instructions {
        if inst.op == OpCode::Eq {
            assert_ne!(
                inst.a, 0,
                "Eq should not write to r0 (param register), instruction: {:?}",
                inst
            );
        }
    }
}

#[test]
fn regression_nested_while_loops_both_have_backward_jumps() {
    // Ensure nested while loops both produce backward jumps correctly.
    use lumen_compiler::compiler::lir::OpCode;

    let md = markdown_from_code(
        r#"
cell main() -> Int
  let mut total = 0
  let mut i = 0
  while i < 3
    let mut j = 0
    while j < 3
      total = total + 1
      j = j + 1
    end
    i = i + 1
  end
  total
end
"#,
    );
    let module = compile(&md).expect("nested while should compile");
    let main_cell = module
        .cells
        .iter()
        .find(|c| c.name == "main")
        .expect("main cell should exist");

    let backward_jump_count = main_cell
        .instructions
        .iter()
        .filter(|inst| inst.op == OpCode::Jmp && inst.sax_val() < 0)
        .count();
    assert!(
        backward_jump_count >= 2,
        "nested while loops should produce at least 2 backward jumps, found {}",
        backward_jump_count
    );
}

// ─── Additional compile-ok tests: deeper coverage ───

#[test]
fn compile_ok_while_loop_accumulation() {
    let case = CompileCase {
        id: "while_accumulation",
        source: r#"
cell main() -> Int
  let mut total = 0
  let mut i = 1
  while i <= 100
    total = total + i
    i = i + 1
  end
  return total
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_for_loop_with_range() {
    let case = CompileCase {
        id: "for_range",
        source: r#"
cell main() -> Int
  let mut sum = 0
  for i in 0..10
    sum += i
  end
  return sum
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_string_interpolation_complex() {
    let case = CompileCase {
        id: "string_interpolation_complex",
        source: r#"
cell main() -> String
  let name = "world"
  let count = 42
  return "Hello {name}, you have {count} items!"
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_null_coalesce_nested() {
    let case = CompileCase {
        id: "null_coalesce_nested",
        source: r#"
cell main() -> Int
  let a: Int | Null = null
  let b: Int | Null = null
  let c: Int | Null = 42
  return a ?? b ?? c ?? 0
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_compound_assignment_all() {
    let case = CompileCase {
        id: "compound_assignment_all",
        source: r#"
cell main() -> Int
  let mut x = 100
  x += 10
  x -= 5
  x *= 2
  return x
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_if_else_as_expression() {
    // If/else used as expression returning a value
    let case = CompileCase {
        id: "if_else_expression",
        source: r#"
cell max(a: Int, b: Int) -> Int
  if a > b
    return a
  else
    return b
  end
end

cell main() -> Int
  return max(10, 20)
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_float_arithmetic_operations() {
    let case = CompileCase {
        id: "float_arithmetic_ops",
        source: r#"
cell main() -> Float
  let a = 3.14
  let b = 2.71
  let sum = a + b
  let diff = a - b
  let prod = a * b
  let quot = a / b
  return sum + diff + prod + quot
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_list_operations_all() {
    let case = CompileCase {
        id: "list_operations_all",
        source: r#"
cell main() -> Int
  let xs = [10, 20, 30, 40, 50]
  let len = length(xs)
  let ys = append(xs, 60)
  return length(ys)
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_record_construction_and_access() {
    let case = CompileCase {
        id: "record_construction_access",
        source: r#"
record Point
  x: Int
  y: Int
end

cell distance_sq(p: Point) -> Int
  return p.x * p.x + p.y * p.y
end

cell main() -> Int
  let p = Point(x: 3, y: 4)
  return distance_sq(p)
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_enum_variant_construction() {
    let case = CompileCase {
        id: "enum_variant_construction",
        source: r#"
enum Result
  Ok(value: Int)
  Err(message: String)
end

cell main() -> Int
  let r = Ok(value: 42)
  match r
    Ok(v) -> return v
    Err(e) -> return 0
  end
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_match_with_wildcard_only() {
    let case = CompileCase {
        id: "match_wildcard_only",
        source: r#"
cell main() -> String
  let x = 42
  match x
    _ -> return "always"
  end
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_recursive_fibonacci() {
    let case = CompileCase {
        id: "recursive_fibonacci",
        source: r#"
cell fib(n: Int) -> Int
  if n <= 1
    return n
  end
  return fib(n - 1) + fib(n - 2)
end

cell main() -> Int
  return fib(10)
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_multiple_returns() {
    let case = CompileCase {
        id: "multiple_returns",
        source: r#"
cell classify(x: Int) -> String
  if x < 0
    return "negative"
  end
  if x == 0
    return "zero"
  end
  return "positive"
end

cell main() -> String
  return classify(5)
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_deeply_nested_control_flow() {
    let case = CompileCase {
        id: "deeply_nested_control",
        source: r#"
cell main() -> Int
  let mut result = 0
  let mut i = 0
  while i < 5
    if i % 2 == 0
      let mut j = 0
      while j < 3
        result = result + 1
        j = j + 1
      end
    else
      result = result + 10
    end
    i = i + 1
  end
  return result
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_string_operations() {
    let case = CompileCase {
        id: "string_operations",
        source: r#"
cell main() -> String
  let a = "hello"
  let b = "world"
  let c = a + " " + b
  let upper_c = upper(c)
  return upper_c
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_bool_logic_complex() {
    let case = CompileCase {
        id: "bool_logic_complex",
        source: r#"
cell main() -> Bool
  let a = true
  let b = false
  let c = true
  return (a and b) or (not b and c)
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_negative_int_literal() {
    let case = CompileCase {
        id: "negative_int_literal",
        source: r#"
cell main() -> Int
  let x = -42
  return x
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_empty_list() {
    let case = CompileCase {
        id: "empty_list",
        source: r#"
cell main() -> list[Int]
  return []
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_map_construction() {
    let case = CompileCase {
        id: "map_construction",
        source: r#"
cell main() -> map[String, Int]
  return {"x": 1, "y": 2, "z": 3}
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_loop_with_break_and_continue() {
    let case = CompileCase {
        id: "loop_break_continue",
        source: r#"
cell main() -> Int
  let mut sum = 0
  let mut i = 0
  loop
    i = i + 1
    if i > 10
      break
    end
    if i % 3 == 0
      continue
    end
    sum = sum + i
  end
  return sum
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_const_declaration() {
    let case = CompileCase {
        id: "const_declaration",
        source: r#"
const MAX: Int = 100
const PI: Float = 3.14159

cell main() -> Int
  return MAX
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_multiple_records() {
    let case = CompileCase {
        id: "multiple_records",
        source: r#"
record Point
  x: Int
  y: Int
end

record Rect
  origin: Point
  width: Int
  height: Int
end

cell main() -> Rect
  let p = Point(x: 0, y: 0)
  return Rect(origin: p, width: 100, height: 50)
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_cell_with_many_params() {
    let case = CompileCase {
        id: "cell_many_params",
        source: r#"
cell add3(a: Int, b: Int, c: Int) -> Int
  return a + b + c
end

cell main() -> Int
  return add3(1, 2, 3)
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_chained_cell_calls() {
    let case = CompileCase {
        id: "chained_cell_calls",
        source: r#"
cell inc(x: Int) -> Int
  return x + 1
end

cell double(x: Int) -> Int
  return x * 2
end

cell main() -> Int
  return double(inc(double(inc(0))))
end
"#,
    };
    assert_compile_ok(&case);
}

// ─── Additional compile-error tests ───

#[test]
fn error_duplicate_record_definition() {
    // Two records with the same name should be rejected
    let case = ErrorCase {
        id: "duplicate_record",
        source: r#"
record Foo
  x: Int
end

record Foo
  y: String
end
"#,
        expect_substring: "duplicate",
    };
    assert_compile_err(&case);
}

#[test]
fn error_type_mismatch_int_vs_string_return() {
    let case = ErrorCase {
        id: "type_mismatch_int_string",
        source: r#"
cell main() -> String
  return 42
end
"#,
        expect_substring: "mismatch",
    };
    assert_compile_err(&case);
}

#[test]
fn error_type_mismatch_bool_vs_int_return() {
    let case = ErrorCase {
        id: "type_mismatch_bool_int",
        source: r#"
cell main() -> Int
  return true
end
"#,
        expect_substring: "mismatch",
    };
    assert_compile_err(&case);
}

#[test]
fn error_unknown_variable_in_expression() {
    let case = ErrorCase {
        id: "unknown_variable_expr",
        source: r#"
cell main() -> Int
  let x = 1
  return x + undefined_var
end
"#,
        expect_substring: "undefinedvar",
    };
    assert_compile_err(&case);
}

#[test]
fn error_undefined_type_in_list() {
    let case = ErrorCase {
        id: "undefined_type_in_list",
        source: r#"
cell main() -> list[NonExistentType]
  return []
end
"#,
        expect_substring: "undefinedtype",
    };
    assert_compile_err(&case);
}

#[test]
fn error_incomplete_match_three_variants() {
    let case = ErrorCase {
        id: "incomplete_match_three_variants",
        source: r#"
enum Traffic
  Red
  Yellow
  Green
end

cell main(t: Traffic) -> Int
  match t
    Red() -> return 1
  end
end
"#,
        expect_substring: "incomplete match",
    };
    assert_compile_err(&case);
}

#[test]
fn error_undefined_type_in_cell_param() {
    let case = ErrorCase {
        id: "undefined_type_param",
        source: r#"
cell process(x: NonExistentType) -> Int
  return 1
end
"#,
        expect_substring: "undefinedtype",
    };
    assert_compile_err(&case);
}

#[test]
fn error_undefined_type_in_cell_return() {
    let case = ErrorCase {
        id: "undefined_type_return",
        source: r#"
cell main() -> NonExistentType
  return 1
end
"#,
        expect_substring: "undefinedtype",
    };
    assert_compile_err(&case);
}

// ─── Constraint validation tests ───

#[test]
fn compile_ok_valid_constraint_gte() {
    let case = CompileCase {
        id: "valid_constraint_gte",
        source: r#"
record Config
  min_value: Int where min_value >= 0
end

cell main() -> Config
  return Config(min_value: 10)
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_valid_constraint_length() {
    let case = CompileCase {
        id: "valid_constraint_length",
        source: r#"
record User
  name: String where length(name) > 0
end

cell main() -> User
  return User(name: "alice")
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn compile_ok_valid_constraint_combined() {
    let case = CompileCase {
        id: "valid_constraint_combined",
        source: r#"
record Range
  lo: Int where lo >= 0
  hi: Int where hi >= 0
end

cell main() -> Range
  return Range(lo: 1, hi: 10)
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn error_constraint_unknown_function() {
    let case = ErrorCase {
        id: "constraint_unknown_fn",
        source: r#"
record Data
  value: String where bogus_function(value)
end
"#,
        expect_substring: "unknown constraint function",
    };
    assert_compile_err(&case);
}

// ─── LIR module structure tests ───

#[test]
fn lir_module_has_cells() {
    let md = markdown_from_code(
        r#"
cell add(a: Int, b: Int) -> Int
  return a + b
end

cell main() -> Int
  return add(1, 2)
end
"#,
    );
    let module = compile(&md).expect("should compile");
    assert_eq!(module.cells.len(), 2, "should have 2 cells");

    let add_cell = module.cells.iter().find(|c| c.name == "add").unwrap();
    assert_eq!(add_cell.params.len(), 2, "add should have 2 params");
    assert_eq!(add_cell.params[0].name, "a");
    assert_eq!(add_cell.params[1].name, "b");

    let main_cell = module.cells.iter().find(|c| c.name == "main").unwrap();
    assert_eq!(main_cell.params.len(), 0, "main should have no params");
}

#[test]
fn lir_module_has_types() {
    let md = markdown_from_code(
        r#"
record Point
  x: Int
  y: Int
end

enum Color
  Red
  Green
  Blue
end

cell main() -> Int
  return 1
end
"#,
    );
    let module = compile(&md).expect("should compile");
    assert!(module.types.len() >= 2, "should have at least 2 types");
}

#[test]
fn lir_module_has_constants() {
    let md = markdown_from_code(
        r#"
cell main() -> Int
  let x = 42
  let y = "hello"
  return x
end
"#,
    );
    let module = compile(&md).expect("should compile");
    let main_cell = module.cells.iter().find(|c| c.name == "main").unwrap();
    // Should have constants for 42 and "hello"
    assert!(
        !main_cell.constants.is_empty(),
        "main should have constants"
    );
}

#[test]
fn lir_module_tools_and_policies() {
    let md = markdown_from_code(
        r#"
use tool http.get as HttpGet
grant HttpGet
  domain "*.example.com"
  timeout_ms 5000

cell main() -> Int
  return 1
end
"#,
    );
    let module = compile(&md).expect("should compile");
    assert!(!module.tools.is_empty(), "should have tool declarations");
    assert!(!module.policies.is_empty(), "should have policies");
}

#[test]
fn lir_module_effects() {
    let md = markdown_from_code(
        r#"
effect database
  cell query(sql: String) -> list[String]
end

cell main() -> Int
  return 1
end
"#,
    );
    let module = compile(&md).expect("should compile");
    assert!(!module.effects.is_empty(), "should have effect declarations");
    assert_eq!(module.effects[0].name, "database");
}

#[test]
fn lir_cell_has_instructions() {
    let md = markdown_from_code(
        r#"
cell main() -> Int
  return 42
end
"#,
    );
    let module = compile(&md).expect("should compile");
    let main_cell = module.cells.iter().find(|c| c.name == "main").unwrap();
    assert!(
        !main_cell.instructions.is_empty(),
        "main cell should have instructions"
    );
}

#[test]
fn lir_cell_return_type() {
    let md = markdown_from_code(
        r#"
cell greet(name: String) -> String
  return "hello " + name
end

cell main() -> Int
  return 1
end
"#,
    );
    let module = compile(&md).expect("should compile");
    let greet_cell = module.cells.iter().find(|c| c.name == "greet").unwrap();
    assert_eq!(
        greet_cell.returns.as_deref(),
        Some("String"),
        "greet should return String"
    );
    let main_cell = module.cells.iter().find(|c| c.name == "main").unwrap();
    assert_eq!(
        main_cell.returns.as_deref(),
        Some("Int"),
        "main should return Int"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Additional spec_suite tests: process constructs, advanced features
// ═══════════════════════════════════════════════════════════════════

#[test]
fn spec_process_memory_operations() {
    let case = CompileCase {
        id: "process_memory_ops",
        source: r#"
memory ConversationHistory: short_term
  window: 20
end

cell main() -> Int
  return 1
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn spec_process_machine_transitions() {
    let case = CompileCase {
        id: "process_machine_transitions",
        source: r#"
machine SimpleFlow
  initial: Start
  state Start
    on_enter()
      transition Done()
    end
  end
  state Done
    terminal: true
  end
end

cell main() -> Int
  return 1
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn spec_grant_with_policy_constraints() {
    let case = CompileCase {
        id: "grant_policy_constraints",
        source: r#"
use tool http.get as HttpGet
grant HttpGet
  domain "*.example.com"
  timeout_ms 5000
  max_tokens 1000

cell main() -> Int
  return 1
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn spec_bind_effect_to_tool() {
    let case = CompileCase {
        id: "bind_effect_tool",
        source: r#"
use tool http.get as HttpGet
bind effect http to HttpGet
grant HttpGet

cell main() -> Int
  return 1
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn spec_complex_pattern_matching_nested() {
    let case = CompileCase {
        id: "complex_pattern_nested",
        source: r#"
enum Color
  Red
  Green
  Blue
end

cell is_primary(c: Color) -> Bool
  match c
    Red() -> return true
    Green() -> return true
    Blue() -> return true
  end
end

cell main() -> Bool
  return is_primary(Red)
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn spec_complex_pattern_with_guard() {
    let case = CompileCase {
        id: "pattern_with_guard",
        source: r#"
enum Value
  Number(n: Int)
  Text(s: String)
end

cell is_positive(v: Value) -> Bool
  match v
    Number(n) if n > 0 -> return true
    _ -> return false
  end
end

cell main() -> Bool
  return is_positive(Number(n: 5))
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn spec_string_interpolation_complex() {
    let case = CompileCase {
        id: "string_interp_complex",
        source: r#"
cell format_user(name: String, age: Int, active: Bool) -> String
  return "User: {name}, Age: {age}, Active: {active}"
end

cell main() -> String
  return format_user("Alice", 30, true)
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn spec_pipe_operator_chain() {
    let case = CompileCase {
        id: "pipe_operator",
        source: r#"
cell double(x: Int) -> Int
  return x * 2
end

cell inc(x: Int) -> Int
  return x + 1
end

cell main() -> Int
  return 5 |> double |> inc
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn spec_null_coalesce_chain() {
    let case = CompileCase {
        id: "null_coalesce_chain",
        source: r#"
cell get_value(flag: Int) -> Int | Null
  if flag == 1
    return 10
  end
  return null
end

cell main() -> Int
  let a = get_value(0)
  let b = get_value(0)
  let c = get_value(1)
  return a ?? b ?? c ?? 99
end
"#,
    };
    assert_compile_ok(&case);
}

#[test]
fn spec_import_statement_multiple() {
    let case = CompileCase {
        id: "import_multiple",
        source: r#"
import std.core: *
import std.collections: List, Map
import app.models: User as AppUser

cell main() -> Int
  return 1
end
"#,
    };
    assert_compile_ok(&case);
}
