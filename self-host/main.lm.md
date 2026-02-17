# Self-Hosted Lumen Compiler

Entry point for the self-hosted Lumen compiler. This module wires together
all compiler phases: markdown extraction, lexing, parsing, name resolution,
type checking, and LIR lowering.

## Imports

```lumen
import self_host.errors: CompileError, LexError, ParseError, ResolveError, TypecheckError, format_compile_error
import self_host.symbols: SymbolTable, new_symbol_table
import self_host.intern: StringInterner, new_interner
import self_host.serialize: LirModule, write_module
```

## Compiler Pipeline

The `compile` cell orchestrates all phases in sequence. Each phase
transforms the output of the previous one, short-circuiting on error.

```lumen
record CompileOptions(
  filename: String,
  source: String,
  emit_lir: Bool,
  trace: Bool
)

record CompileResult(
  module: LirModule?,
  errors: list[String],
  warnings: list[String]
)

cell compile(opts: CompileOptions) -> result[LirModule, CompileError]
  let source = opts.source
  let filename = opts.filename

  # Phase 1: Markdown extraction
  # If the file is .lm.md or .lumen, extract fenced code blocks.
  let code = if ends_with(filename, ".lm.md") then
    extract_markdown(source)
  else
    if ends_with(filename, ".lumen") then
      extract_markdown(source)
    else
      source
    end
  end

  # Phase 2: Lexing
  let tokens = match lex(code)
    case Ok(t) -> t
    case Err(e) -> return Err(CompileError.Lex(error: e))
  end

  # Phase 3: Parsing
  let ast = match parse(tokens)
    case Ok(a) -> a
    case Err(errors) -> return Err(CompileError.Parse(errors: errors))
  end

  # Phase 4: Name resolution
  let symbols = match resolve(ast)
    case Ok(s) -> s
    case Err(errors) -> return Err(CompileError.Resolve(errors: errors))
  end

  # Phase 5: Type checking
  match typecheck(ast, symbols)
    case Ok(_) -> null
    case Err(errors) -> return Err(CompileError.Typecheck(errors: errors))
  end

  # Phase 6: LIR lowering
  let module = match lower(ast, symbols)
    case Ok(m) -> m
    case Err(msg) -> return Err(CompileError.Lower(message: msg, line: 0))
  end

  Ok(module)
end
```

## Stub Phase Functions

These are placeholder cells that will be replaced by the actual
implementations in their respective modules. Each phase has its own
module (lexer.lm.md, parser.lm.md, etc.) that will be created as the
self-host progresses.

```lumen
cell extract_markdown(source: String) -> String
  # Stub: extract fenced ```lumen blocks from markdown.
  # Returns concatenated code block contents.
  let result = ""
  let in_block = false
  let lines = split(source, "\n")
  for line in lines
    if starts_with(line, "```lumen") then
      in_block = true
    else
      if starts_with(line, "```") then
        if in_block then
          in_block = false
          result = result ++ "\n"
        end
      else
        if in_block then
          result = result ++ line ++ "\n"
        end
      end
    end
  end
  result
end

cell lex(source: String) -> result[list[String], LexError]
  # Stub: tokenize source into token list.
  # Will be replaced by self_host.lexer module.
  Ok([])
end

cell parse(tokens: list[String]) -> result[String, list[ParseError]]
  # Stub: parse tokens into AST.
  # Will be replaced by self_host.parser module.
  Ok("ast_placeholder")
end

cell resolve(ast: String) -> result[SymbolTable, list[ResolveError]]
  # Stub: resolve names and build symbol table.
  # Will be replaced by self_host.resolver module.
  Ok(new_symbol_table())
end

cell typecheck(ast: String, symbols: SymbolTable) -> result[Bool, list[TypecheckError]]
  # Stub: type-check the AST against the symbol table.
  # Will be replaced by self_host.typecheck module.
  Ok(true)
end

cell lower(ast: String, symbols: SymbolTable) -> result[LirModule, String]
  # Stub: lower AST to LIR bytecode.
  # Will be replaced by self_host.lower module.
  Err("lowering not yet implemented")
end
```

## Main Entry Point

Reads a source file from the command line and compiles it.

```lumen
cell main() -> Int
  let args = get_env("LUMEN_ARGS")
  if args == "" then
    print("usage: lumen run self-host/main.lm.md -- <source-file>")
    return 1
  end

  let filename = args
  let source = match read_file(filename)
    case Ok(s) -> s
    case Err(e) ->
      print("error: could not read '{filename}': {e}")
      return 1
  end

  let opts = CompileOptions(
    filename: filename,
    source: source,
    emit_lir: false,
    trace: false
  )

  match compile(opts)
    case Ok(module) ->
      print("compiled {filename} successfully")
      print("  cells: {length(module.cells)}")
      print("  types: {length(module.types)}")
      print("  strings: {length(module.strings)}")
      0
    case Err(err) ->
      let msg = format_compile_error(err, source)
      print(msg)
      1
  end
end
```
