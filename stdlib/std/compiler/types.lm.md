# std.compiler.types — Type System Types

Resolved type representation and type error definitions, ported from
`rust/lumen-compiler/src/compiler/typecheck.rs`.

```lumen
import std.compiler.span: Span

# ══════════════════════════════════════════════════════════════════
# Resolved types
# ══════════════════════════════════════════════════════════════════
#
# After type checking, every expression is assigned a resolved Type.
# These mirror the Rust `Type` enum from typecheck.rs.

enum Type
  # Primitive types
  TString
  TInt
  TFloat
  TBool
  TBytes
  TJson
  TNull

  # Collection types
  TList(payload: TypeListPayload)
  TMap(payload: TypeMapPayload)
  TSet(payload: TypeSetPayload)
  TTuple(payload: TypeTuplePayload)

  # Composite types
  TRecord(payload: TypeNamePayload)
  TEnum(payload: TypeNamePayload)
  TResult(payload: TypeResultPayload)
  TUnion(payload: TypeUnionPayload)

  # Function type
  TFn(payload: TypeFnPayload)

  # Generic / parametric
  TGeneric(payload: TypeNamePayload)
  TTypeRef(payload: TypeRefPayload)

  # Fallback
  TAny
end

# Payload records for parameterized Type variants
record TypeListPayload(element: Type)
record TypeMapPayload(key: Type, value: Type)
record TypeSetPayload(element: Type)
record TypeTuplePayload(elements: list[Type])
record TypeNamePayload(name: String)
record TypeResultPayload(ok: Type, err: Type)
record TypeUnionPayload(members: list[Type])
record TypeFnPayload(params: list[Type], ret: Type)
record TypeRefPayload(name: String, args: list[Type])

# ══════════════════════════════════════════════════════════════════
# Type errors
# ══════════════════════════════════════════════════════════════════

enum TypeError
  # Type mismatch: expected vs actual at a line
  Mismatch(payload: MismatchError)
  # Undefined variable reference
  UndefinedVar(payload: UndefinedVarError)
  # Attempting to call a non-callable expression
  NotCallable(payload: NotCallableError)
  # Wrong number of arguments
  ArgCount(payload: ArgCountError)
  # Unknown field on a record/type
  UnknownField(payload: UnknownFieldError)
  # Reference to an undefined type name
  UndefinedType(payload: UndefinedTypeError)
  # Cell missing a return value
  MissingReturn(payload: MissingReturnError)
  # Assignment to immutable variable
  ImmutableAssign(payload: ImmutableAssignError)
  # Non-exhaustive match on an enum
  IncompleteMatch(payload: IncompleteMatchError)
  # Unused result of @must_use cell
  MustUseIgnored(payload: MustUseIgnoredError)
end

record MismatchError(
  expected: String,
  actual: String,
  line: Int
)

record UndefinedVarError(
  name: String,
  line: Int
)

record NotCallableError(
  line: Int
)

record ArgCountError(
  expected: Int,
  actual: Int,
  line: Int
)

record UnknownFieldError(
  field: String,
  ty: String,
  line: Int,
  suggestions: list[String]
)

record UndefinedTypeError(
  name: String,
  line: Int
)

record MissingReturnError(
  name: String,
  line: Int
)

record ImmutableAssignError(
  name: String,
  line: Int
)

record IncompleteMatchError(
  enum_name: String,
  missing: list[String],
  line: Int
)

record MustUseIgnoredError(
  name: String,
  line: Int
)

# ══════════════════════════════════════════════════════════════════
# Type display / formatting
# ══════════════════════════════════════════════════════════════════

# Format a Type as a human-readable string.
cell format_type(ty: Type) -> String
  return match ty
    case TString -> "String"
    case TInt -> "Int"
    case TFloat -> "Float"
    case TBool -> "Bool"
    case TBytes -> "Bytes"
    case TJson -> "Json"
    case TNull -> "Null"
    case TAny -> "Any"
    case TList(payload) -> "list[{format_type(payload.element)}]"
    case TMap(payload) -> "map[{format_type(payload.key)}, {format_type(payload.value)}]"
    case TSet(payload) -> "set[{format_type(payload.element)}]"
    case TTuple(payload) ->
      let parts = map(payload.elements, fn(t: Type) -> String => format_type(t) end)
      "({join(parts, ", ")})"
    case TRecord(payload) -> payload.name
    case TEnum(payload) -> payload.name
    case TResult(payload) -> "result[{format_type(payload.ok)}, {format_type(payload.err)}]"
    case TUnion(payload) ->
      let parts = map(payload.members, fn(t: Type) -> String => format_type(t) end)
      join(parts, " | ")
    case TFn(payload) ->
      let ps = map(payload.params, fn(t: Type) -> String => format_type(t) end)
      "fn({join(ps, ", ")}) -> {format_type(payload.ret)}"
    case TGeneric(payload) -> payload.name
    case TTypeRef(payload) ->
      let args = map(payload.args, fn(t: Type) -> String => format_type(t) end)
      "{payload.name}[{join(args, ", ")}]"
  end
end

# Format a TypeError as a human-readable string.
cell format_type_error(err: TypeError) -> String
  return match err
    case Mismatch(payload) ->
      "type mismatch at line {payload.line}: expected {payload.expected}, got {payload.actual}"
    case UndefinedVar(payload) ->
      "undefined variable '{payload.name}' at line {payload.line}"
    case NotCallable(payload) ->
      "not callable at line {payload.line}"
    case ArgCount(payload) ->
      "wrong number of arguments at line {payload.line}: expected {payload.expected}, got {payload.actual}"
    case UnknownField(payload) ->
      "unknown field '{payload.field}' on type '{payload.ty}' at line {payload.line}"
    case UndefinedType(payload) ->
      "undefined type '{payload.name}' at line {payload.line}"
    case MissingReturn(payload) ->
      "missing return in cell '{payload.name}' at line {payload.line}"
    case ImmutableAssign(payload) ->
      "cannot assign to immutable variable '{payload.name}' at line {payload.line}"
    case IncompleteMatch(payload) ->
      "incomplete match at line {payload.line}: missing variants {join(payload.missing, ", ")}"
    case MustUseIgnored(payload) ->
      "unused result of @must_use cell '{payload.name}' at line {payload.line}"
  end
end
```
