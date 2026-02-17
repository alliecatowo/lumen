# Compiler Error Types

All error types for the self-hosted Lumen compiler. These mirror the Rust
compiler's error enums exactly so that error output is identical between
the two implementations.

## Source Location

Every error carries location information for diagnostic display.

```lumen
record SourceLoc(
  line: Int,
  col: Int
)
```

## Lex Errors

Errors produced during tokenization (7 variants).

```lumen
enum LexError
  UnexpectedChar(ch: String, line: Int, col: Int),
  UnterminatedString(line: Int, col: Int),
  InconsistentIndent(line: Int),
  InvalidNumber(line: Int, col: Int),
  InvalidBytesLiteral(line: Int, col: Int),
  InvalidUnicodeEscape(line: Int, col: Int),
  UnterminatedMarkdownBlock(line: Int, col: Int)
end
```

## Parse Errors

Errors produced during parsing (7 variants).

```lumen
enum ParseError
  Unexpected(found: String, expected: String, line: Int, col: Int),
  UnexpectedEof,
  UnclosedBracket(bracket: String, open_line: Int, open_col: Int, current_line: Int, current_col: Int),
  MissingEnd(construct: String, open_line: Int, open_col: Int, current_line: Int, current_col: Int),
  MissingType(line: Int, col: Int),
  IncompleteExpression(line: Int, col: Int, context: String),
  MalformedConstruct(construct: String, reason: String, line: Int, col: Int)
end
```

## Resolve Errors

Errors produced during name resolution (26 variants). These cover
undefined references, effect violations, machine/pipeline validation,
imports, and trait checking.

```lumen
enum ResolveError
  UndefinedType(name: String, line: Int, suggestions: list[String]),
  GenericArityMismatch(name: String, expected: Int, actual: Int, line: Int),
  UndefinedCell(name: String, line: Int, suggestions: list[String]),
  UndefinedTrait(name: String, line: Int),
  UndefinedTool(name: String, line: Int),
  Duplicate(name: String, line: Int),
  MissingEffectGrant(cell: String, effect: String, line: Int),
  UndeclaredEffect(cell: String, effect: String, line: Int, cause: String),
  EffectContractViolation(caller: String, callee: String, effect: String, line: Int),
  NondeterministicOperation(cell: String, operation: String, line: Int),
  MachineUnknownInitial(machine: String, state: String, line: Int),
  MachineUnknownTransition(machine: String, state: String, target: String, line: Int),
  MachineUnreachableState(machine: String, state: String, initial: String, line: Int),
  MachineMissingTerminal(machine: String, line: Int),
  MachineTransitionArgCount(machine: String, state: String, target: String, expected: Int, actual: Int, line: Int),
  MachineTransitionArgType(machine: String, state: String, target: String, expected: String, actual: String, line: Int),
  MachineUnsupportedExpr(machine: String, state: String, context: String, line: Int),
  MachineGuardType(machine: String, state: String, actual: String, line: Int),
  PipelineUnknownStage(pipeline: String, stage: String, line: Int),
  PipelineStageArity(pipeline: String, stage: String, line: Int),
  PipelineStageTypeMismatch(pipeline: String, from_stage: String, to_stage: String, expected: String, actual: String, line: Int),
  CircularImport(module: String, chain: String),
  ModuleNotFound(module: String, line: Int),
  ImportedSymbolNotFound(symbol: String, module: String, line: Int),
  TraitMissingMethods(trait_name: String, target_type: String, missing: list[String], line: Int),
  TraitMethodSignatureMismatch(trait_name: String, target_type: String, method: String, reason: String, expected: String, actual: String, line: Int)
end
```

## Typecheck Errors

Errors produced during type checking.

```lumen
enum TypecheckError
  TypeMismatch(expected: String, actual: String, line: Int, col: Int),
  UndefinedVariable(name: String, line: Int),
  NotCallable(type_name: String, line: Int),
  WrongArgCount(cell: String, expected: Int, actual: Int, line: Int),
  IncompleteMatch(type_name: String, missing: list[String], line: Int),
  InvalidFieldAccess(type_name: String, field: String, line: Int),
  InvalidIndex(type_name: String, index_type: String, line: Int),
  AmbiguousType(context: String, line: Int)
end
```

## Compile Error

Union error type that wraps all compiler phase errors. Each cell in the
pipeline returns `result[T, CompileError]` so callers can handle any kind
of error uniformly.

```lumen
enum CompileError
  Lex(error: LexError),
  Parse(errors: list[ParseError]),
  Resolve(errors: list[ResolveError]),
  Typecheck(errors: list[TypecheckError]),
  Lower(message: String, line: Int),
  Internal(message: String)
end
```

## Error Formatting

Human-readable error messages with source context.

```lumen
cell format_lex_error(err: LexError) -> String
  match err
    case LexError.UnexpectedChar(ch, line, col) ->
      "unexpected character '{ch}' at line {line}, col {col}"
    case LexError.UnterminatedString(line, col) ->
      "unterminated string at line {line}, col {col}"
    case LexError.InconsistentIndent(line) ->
      "inconsistent indentation at line {line}"
    case LexError.InvalidNumber(line, col) ->
      "invalid number at line {line}, col {col}"
    case LexError.InvalidBytesLiteral(line, col) ->
      "invalid bytes literal at line {line}, col {col}"
    case LexError.InvalidUnicodeEscape(line, col) ->
      "invalid unicode escape at line {line}, col {col}"
    case LexError.UnterminatedMarkdownBlock(line, col) ->
      "unterminated markdown block at line {line}, col {col}"
  end
end

cell format_parse_error(err: ParseError) -> String
  match err
    case ParseError.Unexpected(found, expected, line, col) ->
      "unexpected token {found} at line {line}, col {col}; expected {expected}"
    case ParseError.UnexpectedEof ->
      "unexpected end of input"
    case ParseError.UnclosedBracket(bracket, open_line, open_col, _, _) ->
      "unclosed '{bracket}' opened at line {open_line}, col {open_col}"
    case ParseError.MissingEnd(construct, open_line, open_col, _, _) ->
      "expected 'end' to close '{construct}' at line {open_line}, col {open_col}"
    case ParseError.MissingType(line, col) ->
      "expected type after ':' at line {line}, col {col}"
    case ParseError.IncompleteExpression(line, col, context) ->
      "incomplete expression at line {line}, col {col} ({context})"
    case ParseError.MalformedConstruct(construct, reason, line, col) ->
      "malformed {construct} at line {line}, col {col}: {reason}"
  end
end

cell format_resolve_error(err: ResolveError) -> String
  match err
    case ResolveError.UndefinedType(name, line, _) ->
      "undefined type '{name}' at line {line}"
    case ResolveError.GenericArityMismatch(name, expected, actual, line) ->
      "generic type '{name}' at line {line}: expected {expected} type args, got {actual}"
    case ResolveError.UndefinedCell(name, line, _) ->
      "undefined cell '{name}' at line {line}"
    case ResolveError.UndefinedTrait(name, line) ->
      "undefined trait '{name}' at line {line}"
    case ResolveError.UndefinedTool(name, line) ->
      "undefined tool alias '{name}' at line {line}"
    case ResolveError.Duplicate(name, line) ->
      "duplicate definition '{name}' at line {line}"
    case ResolveError.MissingEffectGrant(cell, effect, line) ->
      "cell '{cell}' requires effect '{effect}' but no grant in scope (line {line})"
    case ResolveError.UndeclaredEffect(cell, effect, line, cause) ->
      "cell '{cell}' performs undeclared effect '{effect}' (line {line}){cause}"
    case ResolveError.EffectContractViolation(caller, callee, effect, line) ->
      "cell '{caller}' calls '{callee}' requiring effect '{effect}' not in caller row (line {line})"
    case ResolveError.NondeterministicOperation(cell, operation, line) ->
      "cell '{cell}' uses nondeterministic '{operation}' under @deterministic (line {line})"
    case ResolveError.MachineUnknownInitial(machine, state, line) ->
      "machine '{machine}' initial state '{state}' is undefined (line {line})"
    case ResolveError.MachineUnknownTransition(machine, state, target, line) ->
      "machine '{machine}' state '{state}' transitions to undefined state '{target}' (line {line})"
    case ResolveError.MachineUnreachableState(machine, state, initial, line) ->
      "machine '{machine}' state '{state}' is unreachable from '{initial}' (line {line})"
    case ResolveError.MachineMissingTerminal(machine, line) ->
      "machine '{machine}' declares no terminal states (line {line})"
    case ResolveError.MachineTransitionArgCount(machine, state, target, expected, actual, line) ->
      "machine '{machine}' state '{state}' -> '{target}' arg count: expected {expected}, got {actual} (line {line})"
    case ResolveError.MachineTransitionArgType(machine, state, target, expected, actual, line) ->
      "machine '{machine}' state '{state}' -> '{target}' arg type: expected {expected}, got {actual} (line {line})"
    case ResolveError.MachineUnsupportedExpr(machine, state, context, line) ->
      "machine '{machine}' state '{state}' unsupported expression in {context} (line {line})"
    case ResolveError.MachineGuardType(machine, state, actual, line) ->
      "machine '{machine}' state '{state}' guard must be Bool, got {actual} (line {line})"
    case ResolveError.PipelineUnknownStage(pipeline, stage, line) ->
      "pipeline '{pipeline}' unknown stage cell '{stage}' (line {line})"
    case ResolveError.PipelineStageArity(pipeline, stage, line) ->
      "pipeline '{pipeline}' stage '{stage}' invalid arity (line {line})"
    case ResolveError.PipelineStageTypeMismatch(pipeline, from_stage, to_stage, expected, actual, line) ->
      "pipeline '{pipeline}' type mismatch '{from_stage}' -> '{to_stage}': expected {expected}, got {actual} (line {line})"
    case ResolveError.CircularImport(module, chain) ->
      "circular import detected: '{module}' (chain: {chain})"
    case ResolveError.ModuleNotFound(module, line) ->
      "module '{module}' not found at line {line}"
    case ResolveError.ImportedSymbolNotFound(symbol, module, line) ->
      "imported symbol '{symbol}' not found in module '{module}' at line {line}"
    case ResolveError.TraitMissingMethods(trait_name, target_type, missing, line) ->
      let missing_str = join(missing, ", ")
      "impl '{trait_name}' for '{target_type}' missing methods: {missing_str} (line {line})"
    case ResolveError.TraitMethodSignatureMismatch(trait_name, target_type, method, reason, _, _, line) ->
      "impl '{trait_name}' for '{target_type}' method '{method}' signature mismatch: {reason} (line {line})"
  end
end

cell format_compile_error(err: CompileError, source: String) -> String
  match err
    case CompileError.Lex(error) ->
      format_lex_error(error)
    case CompileError.Parse(errors) ->
      let msgs = map(errors, format_parse_error)
      join(msgs, "\n")
    case CompileError.Resolve(errors) ->
      let msgs = map(errors, format_resolve_error)
      join(msgs, "\n")
    case CompileError.Typecheck(errors) ->
      let msgs = map(errors, format_typecheck_error)
      join(msgs, "\n")
    case CompileError.Lower(message, line) ->
      "lowering error at line {line}: {message}"
    case CompileError.Internal(message) ->
      "internal compiler error: {message}"
  end
end

cell format_typecheck_error(err: TypecheckError) -> String
  match err
    case TypecheckError.TypeMismatch(expected, actual, line, col) ->
      "type mismatch at line {line}, col {col}: expected {expected}, got {actual}"
    case TypecheckError.UndefinedVariable(name, line) ->
      "undefined variable '{name}' at line {line}"
    case TypecheckError.NotCallable(type_name, line) ->
      "type '{type_name}' is not callable at line {line}"
    case TypecheckError.WrongArgCount(cell, expected, actual, line) ->
      "cell '{cell}' expects {expected} args, got {actual} at line {line}"
    case TypecheckError.IncompleteMatch(type_name, missing, line) ->
      let missing_str = join(missing, ", ")
      "incomplete match on '{type_name}': missing {missing_str} at line {line}"
    case TypecheckError.InvalidFieldAccess(type_name, field, line) ->
      "type '{type_name}' has no field '{field}' at line {line}"
    case TypecheckError.InvalidIndex(type_name, index_type, line) ->
      "cannot index '{type_name}' with '{index_type}' at line {line}"
    case TypecheckError.AmbiguousType(context, line) ->
      "ambiguous type in {context} at line {line}"
  end
end
```
