# Self-Hosted Lumen Lexer

Indentation-aware lexer for Lumen source code, ported from
`rust/lumen-compiler/src/compiler/lexer.rs`.

Implements Phase 1 (S051–S090) of the self-hosting plan.

## Imports

```lumen
import std.compiler.span: Span, make_span
import std.compiler.tokens: Token, TokenKind, IntLitVal, FloatLitVal,
  StringLitVal, BoolLitVal, BytesLitVal, IdentVal, DirectiveVal,
  SymbolVal, InterpSegment, StringInterpVal, make_token
import self_host.errors: LexError
```

## Lexer State

The `LexerState` record holds all mutable state for a single lex pass.

```lumen
record LexerState(
  source:       list[String],   # source as list of single-char strings
  pos:          Int,            # current index into source
  line:         Int,            # 1-based current line
  col:          Int,            # 1-based current column
  byte_offset:  Int,            # byte offset (same as char offset for ASCII; approximate for UTF-8)
  base_line:    Int,            # line offset for embedded code blocks
  base_offset:  Int,            # byte offset for embedded code blocks
  indent_stack: list[Int],      # stack of indentation levels
  pending:      list[Token],    # tokens queued (INDENT/DEDENT)
  at_line_start: Bool           # true when we are at the start of a logical line
)

cell new_lexer(source: String, base_line: Int, base_offset: Int) -> LexerState
  let chars = string_chars(source)  # splits string into list of single chars
  LexerState(
    source:        chars,
    pos:           0,
    line:          1,
    col:           1,
    byte_offset:   0,
    base_line:     base_line,
    base_offset:   base_offset,
    indent_stack:  [0],
    pending:       [],
    at_line_start: true
  )
end
```

## Character Helpers

```lumen
# Return the character at the current position, or "" if at end.
cell lexer_current(st: LexerState) -> String
  if st.pos < length(st.source) then
    st.source[st.pos]
  else
    ""
  end
end

# Return the character one ahead, or "" if past end.
cell lexer_peek(st: LexerState) -> String
  if st.pos + 1 < length(st.source) then
    st.source[st.pos + 1]
  else
    ""
  end
end

# Return the character two ahead, or "" if past end.
cell lexer_peek2(st: LexerState) -> String
  if st.pos + 2 < length(st.source) then
    st.source[st.pos + 2]
  else
    ""
  end
end

# Advance the cursor by one character, updating line/col/byte_offset.
# Returns (updated_state, consumed_char).
cell lexer_advance(st: LexerState) -> (LexerState, String)
  if st.pos >= length(st.source) then
    return (st, "")
  end
  let ch = st.source[st.pos]
  let new_pos = st.pos + 1
  let new_byte = st.byte_offset + 1
  let (new_line, new_col, new_at_start) = if ch == "\n" then
    (st.line + 1, 1, true)
  else
    (st.line, st.col + 1, false)
  end
  let new_st = LexerState(
    source:        st.source,
    pos:           new_pos,
    line:          new_line,
    col:           new_col,
    byte_offset:   new_byte,
    base_line:     st.base_line,
    base_offset:   st.base_offset,
    indent_stack:  st.indent_stack,
    pending:       st.pending,
    at_line_start: new_at_start
  )
  (new_st, ch)
end

# Build a zero-length span at the current cursor position.
cell span_here(st: LexerState) -> Span
  let abs_off = st.base_offset + st.byte_offset
  let abs_line = st.base_line + st.line - 1
  make_span("", abs_off, abs_off, abs_line, st.col)
end

# Build a span from a saved start position to the current cursor.
cell span_from(st: LexerState, start_off: Int, start_line: Int, start_col: Int) -> Span
  let abs_start = st.base_offset + start_off
  let abs_end   = st.base_offset + st.byte_offset
  let abs_line  = st.base_line + start_line - 1
  make_span("", abs_start, abs_end, abs_line, start_col)
end
```

## Keyword Table

```lumen
# Return the keyword token kind for a given identifier string,
# or null if the string is not a keyword.
cell keyword_kind(word: String) -> TokenKind?
  match word
    case "record"   -> TokenKind.KwRecord
    case "enum"     -> TokenKind.KwEnum
    case "cell"     -> TokenKind.KwCell
    case "let"      -> TokenKind.KwLet
    case "if"       -> TokenKind.KwIf
    case "else"     -> TokenKind.KwElse
    case "for"      -> TokenKind.KwFor
    case "in"       -> TokenKind.KwIn
    case "match"    -> TokenKind.KwMatch
    case "return"   -> TokenKind.KwReturn
    case "halt"     -> TokenKind.KwHalt
    case "end"      -> TokenKind.KwEnd
    case "use"      -> TokenKind.KwUse
    case "tool"     -> TokenKind.KwTool
    case "as"       -> TokenKind.KwAs
    case "grant"    -> TokenKind.KwGrant
    case "expect"   -> TokenKind.KwExpect
    case "schema"   -> TokenKind.KwSchema
    case "role"     -> TokenKind.KwRole
    case "where"    -> TokenKind.KwWhere
    case "and"      -> TokenKind.KwAnd
    case "or"       -> TokenKind.KwOr
    case "not"      -> TokenKind.KwNot
    case "null"     -> TokenKind.KwNull
    case "result"   -> TokenKind.KwResult
    case "ok"       -> TokenKind.KwOk
    case "err"      -> TokenKind.KwErr
    case "list"     -> TokenKind.KwList
    case "map"      -> TokenKind.KwMap
    case "while"    -> TokenKind.KwWhile
    case "loop"     -> TokenKind.KwLoop
    case "break"    -> TokenKind.KwBreak
    case "continue" -> TokenKind.KwContinue
    case "mut"      -> TokenKind.KwMut
    case "const"    -> TokenKind.KwConst
    case "pub"      -> TokenKind.KwPub
    case "import"   -> TokenKind.KwImport
    case "from"     -> TokenKind.KwFrom
    case "async"    -> TokenKind.KwAsync
    case "await"    -> TokenKind.KwAwait
    case "parallel" -> TokenKind.KwParallel
    case "fn"       -> TokenKind.KwFn
    case "trait"    -> TokenKind.KwTrait
    case "impl"     -> TokenKind.KwImpl
    case "type"     -> TokenKind.KwType
    case "set"      -> TokenKind.KwSet
    case "tuple"    -> TokenKind.KwTuple
    case "emit"     -> TokenKind.KwEmit
    case "yield"    -> TokenKind.KwYield
    case "mod"      -> TokenKind.KwMod
    case "self"     -> TokenKind.KwSelf
    case "with"     -> TokenKind.KwWith
    case "try"      -> TokenKind.KwTry
    case "union"    -> TokenKind.KwUnion
    case "step"     -> TokenKind.KwStep
    case "comptime" -> TokenKind.KwComptime
    case "macro"    -> TokenKind.KwMacro
    case "extern"   -> TokenKind.KwExtern
    case "then"     -> TokenKind.KwThen
    case "when"     -> TokenKind.KwWhen
    case "is"       -> TokenKind.KwIs
    case "defer"    -> TokenKind.KwDefer
    case "perform"  -> TokenKind.KwPerform
    case "handle"   -> TokenKind.KwHandle
    case "resume"   -> TokenKind.KwResume
    case "Bool"     -> TokenKind.KwBool
    case "Int"      -> TokenKind.KwInt
    case "Float"    -> TokenKind.KwFloat
    case "String"   -> TokenKind.KwString
    case "Bytes"    -> TokenKind.KwBytes
    case "Json"     -> TokenKind.KwJson
    case "true"     -> TokenKind.BoolLit(payload: BoolLitVal(value: true))
    case "false"    -> TokenKind.BoolLit(payload: BoolLitVal(value: false))
    case _          -> null
  end
end
```

## Indentation Handling

```lumen
# Handle indentation at the start of a new line.
# Counts leading spaces/tabs, then compares with the indent stack
# to emit INDENT or DEDENT tokens into st.pending.
# Returns result[LexerState, LexError].
cell handle_indentation(st: LexerState) -> result[LexerState, LexError]
  # Count leading whitespace
  let indent = 0
  let s = st
  loop
    let ch = lexer_current(s)
    if ch == " " then
      indent = indent + 1
      let (ns, _) = lexer_advance(s)
      s = ns
    else
      if ch == "\t" then
        indent = indent + 2
        let (ns, _) = lexer_advance(s)
        s = ns
      else
        break
      end
    end
  end

  # Skip blank lines and comment-only lines
  let cur = lexer_current(s)
  if cur == "" or cur == "\n" or cur == "#" then
    # At EOF, unwind indent stack
    if cur == "" then
      let pending = s.pending
      let istack = s.indent_stack
      loop
        if length(istack) <= 1 then
          break
        end
        istack = list_pop(istack)
        pending = pending ++ [Token(
          kind:   TokenKind.Dedent,
          lexeme: "",
          span:   span_here(s)
        )]
      end
      return Ok(LexerState(
        source:        s.source,
        pos:           s.pos,
        line:          s.line,
        col:           s.col,
        byte_offset:   s.byte_offset,
        base_line:     s.base_line,
        base_offset:   s.base_offset,
        indent_stack:  istack,
        pending:       pending,
        at_line_start: s.at_line_start
      ))
    end
    return Ok(s)
  end

  let cur_level = s.indent_stack[length(s.indent_stack) - 1]
  let istack = s.indent_stack
  let pending = s.pending

  if indent > cur_level then
    istack = istack ++ [indent]
    pending = pending ++ [Token(
      kind:   TokenKind.Indent,
      lexeme: "",
      span:   span_here(s)
    )]
  else
    if indent < cur_level then
      loop
        if length(istack) == 0 then
          break
        end
        let top = istack[length(istack) - 1]
        if top > indent then
          istack = list_pop(istack)
          pending = pending ++ [Token(
            kind:   TokenKind.Dedent,
            lexeme: "",
            span:   span_here(s)
          )]
        else
          break
        end
      end
      # Check consistent dedent
      let final_top = istack[length(istack) - 1]
      if final_top != indent then
        return Err(LexError.InconsistentIndent(
          line: s.base_line + s.line - 1
        ))
      end
    end
  end

  Ok(LexerState(
    source:        s.source,
    pos:           s.pos,
    line:          s.line,
    col:           s.col,
    byte_offset:   s.byte_offset,
    base_line:     s.base_line,
    base_offset:   s.base_offset,
    indent_stack:  istack,
    pending:       pending,
    at_line_start: false
  ))
end
```

## Number Literals

```lumen
# Read an integer or float literal.
# Handles decimal, hex (0x), binary (0b), octal (0o), and floats with exponents.
cell scan_number(st: LexerState) -> result[(LexerState, Token), LexError]
  let so = st.byte_offset
  let sl = st.line
  let sc = st.col

  let s = st
  let buf = ""

  # Check for 0x / 0b / 0o prefix
  if lexer_current(s) == "0" then
    let next = lexer_peek(s)
    if next == "x" or next == "X" then
      buf = buf ++ "0x"
      let (s1, _) = lexer_advance(s)
      let (s2, _) = lexer_advance(s1)
      s = s2
      loop
        let ch = lexer_current(s)
        if is_hex_digit(ch) or ch == "_" then
          if ch != "_" then
            buf = buf ++ ch
          end
          let (ns, _) = lexer_advance(s)
          s = ns
        else
          break
        end
      end
      let val = parse_int_base(buf, 16)
      match val
        case Ok(n) ->
          let sp = span_from(s, so, sl, sc)
          let tok = Token(kind: TokenKind.IntLit(payload: IntLitVal(value: n)), lexeme: buf, span: sp)
          return Ok((s, tok))
        case Err(_) ->
          return Err(LexError.InvalidNumber(line: s.base_line + sl - 1, col: sc))
      end
    end
    if next == "b" or next == "B" then
      buf = buf ++ "0b"
      let (s1, _) = lexer_advance(s)
      let (s2, _) = lexer_advance(s1)
      s = s2
      loop
        let ch = lexer_current(s)
        if ch == "0" or ch == "1" or ch == "_" then
          if ch != "_" then
            buf = buf ++ ch
          end
          let (ns, _) = lexer_advance(s)
          s = ns
        else
          break
        end
      end
      let val = parse_int_base(strip_prefix(buf, "0b"), 2)
      match val
        case Ok(n) ->
          let sp = span_from(s, so, sl, sc)
          let tok = Token(kind: TokenKind.IntLit(payload: IntLitVal(value: n)), lexeme: buf, span: sp)
          return Ok((s, tok))
        case Err(_) ->
          return Err(LexError.InvalidNumber(line: s.base_line + sl - 1, col: sc))
      end
    end
    if next == "o" or next == "O" then
      buf = buf ++ "0o"
      let (s1, _) = lexer_advance(s)
      let (s2, _) = lexer_advance(s1)
      s = s2
      loop
        let ch = lexer_current(s)
        if ch >= "0" and ch <= "7" or ch == "_" then
          if ch != "_" then
            buf = buf ++ ch
          end
          let (ns, _) = lexer_advance(s)
          s = ns
        else
          break
        end
      end
      let val = parse_int_base(strip_prefix(buf, "0o"), 8)
      match val
        case Ok(n) ->
          let sp = span_from(s, so, sl, sc)
          let tok = Token(kind: TokenKind.IntLit(payload: IntLitVal(value: n)), lexeme: buf, span: sp)
          return Ok((s, tok))
        case Err(_) ->
          return Err(LexError.InvalidNumber(line: s.base_line + sl - 1, col: sc))
      end
    end
  end

  # Decimal integer or float
  loop
    let ch = lexer_current(s)
    if is_digit(ch) or ch == "_" then
      if ch != "_" then
        buf = buf ++ ch
      end
      let (ns, _) = lexer_advance(s)
      s = ns
    else
      break
    end
  end

  # Check for float: decimal point followed by digit
  let is_float = lexer_current(s) == "." and is_digit(lexer_peek(s))
  if is_float then
    buf = buf ++ "."
    let (ns, _) = lexer_advance(s)  # consume '.'
    s = ns
    loop
      let ch = lexer_current(s)
      if is_digit(ch) or ch == "_" then
        if ch != "_" then
          buf = buf ++ ch
        end
        let (ns2, _) = lexer_advance(s)
        s = ns2
      else
        break
      end
    end
    # Check for exponent
    let cur = lexer_current(s)
    if cur == "e" or cur == "E" then
      buf = buf ++ cur
      let (ns, _) = lexer_advance(s)
      s = ns
      let sign = lexer_current(s)
      if sign == "+" or sign == "-" then
        buf = buf ++ sign
        let (ns2, _) = lexer_advance(s)
        s = ns2
      end
      loop
        let ch = lexer_current(s)
        if is_digit(ch) then
          buf = buf ++ ch
          let (ns2, _) = lexer_advance(s)
          s = ns2
        else
          break
        end
      end
    end
    let fval = parse_float(buf)
    match fval
      case Ok(f) ->
        let sp = span_from(s, so, sl, sc)
        let tok = Token(kind: TokenKind.FloatLit(payload: FloatLitVal(value: f)), lexeme: buf, span: sp)
        return Ok((s, tok))
      case Err(_) ->
        return Err(LexError.InvalidNumber(line: s.base_line + sl - 1, col: sc))
    end
  end

  # Plain integer
  match parse_int(buf)
    case Ok(n) ->
      let sp = span_from(s, so, sl, sc)
      let tok = Token(kind: TokenKind.IntLit(payload: IntLitVal(value: n)), lexeme: buf, span: sp)
      Ok((s, tok))
    case Err(_) ->
      Err(LexError.InvalidNumber(line: s.base_line + sl - 1, col: sc))
  end
end
```

## String Literals

```lumen
# Check whether the current position looks like the start of an
# interpolation hole `{expr}` inside a string.
cell looks_like_interp(st: LexerState) -> Bool
  # We've already consumed `{`, so peek ahead (pos+1 is the char after '{')
  let i = st.pos + 1
  loop
    if i >= length(st.source) then
      return false
    end
    let ch = st.source[i]
    if ch == " " or ch == "\t" then
      i = i + 1
    else
      let is_ident_start = (ch >= "a" and ch <= "z") or (ch >= "A" and ch <= "Z") or ch == "_"
      let is_expr_start = ch == "(" or ch == "[" or ch == "-" or (ch >= "0" and ch <= "9")
      return is_ident_start or is_expr_start
    end
  end
  false
end

# Split an interpolation expression from its optional format specifier.
# e.g. "value:.2f" -> ("value", ".2f"), "name" -> ("name", null)
cell split_format_spec(expr: String) -> (String, String?)
  let idx = string_find(expr, ":")
  if idx < 0 then
    (expr, null)
  else
    (string_slice(expr, 0, idx), string_slice(expr, idx + 1, length(expr)))
  end
end

# Read a regular (non-triple-quoted) string literal.
# Handles escape sequences and `{expr}` interpolation.
cell scan_string(st: LexerState) -> result[(LexerState, Token), LexError]
  let so = st.byte_offset
  let sl = st.line
  let sc = st.col
  let s = st

  # Check for triple-quoted string
  if lexer_current(s) == "\"" and lexer_peek(s) == "\"" and lexer_peek2(s) == "\"" then
    return scan_triple_string(s)
  end

  # Consume opening quote
  let (s1, _) = lexer_advance(s)
  s = s1

  let segments: list[(Bool, String, String?)] = []
  let cur_segment = ""
  let is_interp = false

  loop
    let ch = lexer_current(s)
    if ch == "" then
      return Err(LexError.UnterminatedString(line: s.base_line + sl - 1, col: sc))
    end
    if ch == "\"" then
      # End of string
      let (ns, _) = lexer_advance(s)
      s = ns
      break
    end
    if ch == "\n" then
      return Err(LexError.UnterminatedString(line: s.base_line + sl - 1, col: sc))
    end
    if ch == "\\" then
      let (ns, _) = lexer_advance(s)
      s = ns
      match process_escape(s, sl, sc)
        case Ok((ns2, escaped)) ->
          cur_segment = cur_segment ++ escaped
          s = ns2
        case Err(e) -> return Err(e)
      end
    else
      if ch == "{" and looks_like_interp(s) then
        is_interp = true
        if cur_segment != "" then
          segments = segments ++ [(false, cur_segment, null)]
          cur_segment = ""
        end
        let (ns, _) = lexer_advance(s)  # consume '{'
        s = ns
        let expr_buf = ""
        let brace_balance = 1
        loop
          let ic = lexer_current(s)
          if ic == "" then
            return Err(LexError.UnterminatedString(line: s.base_line + sl - 1, col: sc))
          end
          if ic == "}" then
            brace_balance = brace_balance - 1
            if brace_balance == 0 then
              break
            end
            expr_buf = expr_buf ++ ic
            let (ns2, _) = lexer_advance(s)
            s = ns2
          else
            if ic == "{" then
              brace_balance = brace_balance + 1
              expr_buf = expr_buf ++ ic
              let (ns2, _) = lexer_advance(s)
              s = ns2
            else
              expr_buf = expr_buf ++ ic
              let (ns2, _) = lexer_advance(s)
              s = ns2
            end
          end
        end
        let (ns2, _) = lexer_advance(s)  # consume '}'
        s = ns2
        let (expr_text, fmt_spec) = split_format_spec(string_trim(expr_buf))
        segments = segments ++ [(true, expr_text, fmt_spec)]
      else
        cur_segment = cur_segment ++ ch
        let (ns, _) = lexer_advance(s)
        s = ns
      end
    end
  end

  let sp = span_from(s, so, sl, sc)
  if is_interp or length(segments) > 0 then
    if cur_segment != "" then
      segments = segments ++ [(false, cur_segment, null)]
    end
    let interp_segments = list_map(segments, fn(seg: (Bool, String, String?)) -> InterpSegment
      let (is_expr, text, fmt) = seg
      InterpSegment(is_expr: is_expr, text: text, format_spec: fmt)
    end)
    let tok = Token(
      kind:   TokenKind.StringInterpLit(payload: StringInterpVal(segments: interp_segments)),
      lexeme: "",
      span:   sp
    )
    Ok((s, tok))
  else
    let tok = Token(
      kind:   TokenKind.StringLit(payload: StringLitVal(value: cur_segment)),
      lexeme: cur_segment,
      span:   sp
    )
    Ok((s, tok))
  end
end

# Read a triple-quoted string literal (may span multiple lines).
cell scan_triple_string(st: LexerState) -> result[(LexerState, Token), LexError]
  let so = st.byte_offset
  let sl = st.line
  let sc = st.col
  let s = st

  # Consume the three opening quotes
  let (s1, _) = lexer_advance(s)
  let (s2, _) = lexer_advance(s1)
  let (s3, _) = lexer_advance(s2)
  s = s3

  let segments: list[(Bool, String, String?)] = []
  let cur_segment = ""
  let is_interp = false

  loop
    let ch = lexer_current(s)
    if ch == "" then
      return Err(LexError.UnterminatedString(line: s.base_line + sl - 1, col: sc))
    end
    # Check for closing """
    if ch == "\"" and lexer_peek(s) == "\"" and lexer_peek2(s) == "\"" then
      let (s1, _) = lexer_advance(s)
      let (s2, _) = lexer_advance(s1)
      let (s3, _) = lexer_advance(s2)
      s = s3
      break
    end
    if ch == "\\" then
      let (ns, _) = lexer_advance(s)
      s = ns
      match process_escape(s, sl, sc)
        case Ok((ns2, escaped)) ->
          cur_segment = cur_segment ++ escaped
          s = ns2
        case Err(e) -> return Err(e)
      end
    else
      if ch == "{" and looks_like_interp(s) then
        is_interp = true
        if cur_segment != "" then
          segments = segments ++ [(false, cur_segment, null)]
          cur_segment = ""
        end
        let (ns, _) = lexer_advance(s)  # consume '{'
        s = ns
        let expr_buf = ""
        let brace_balance = 1
        loop
          let ic = lexer_current(s)
          if ic == "" then
            return Err(LexError.UnterminatedString(line: s.base_line + sl - 1, col: sc))
          end
          if ic == "}" then
            brace_balance = brace_balance - 1
            if brace_balance == 0 then
              break
            end
            expr_buf = expr_buf ++ ic
            let (ns2, _) = lexer_advance(s)
            s = ns2
          else
            if ic == "{" then
              brace_balance = brace_balance + 1
            end
            expr_buf = expr_buf ++ ic
            let (ns2, _) = lexer_advance(s)
            s = ns2
          end
        end
        let (ns2, _) = lexer_advance(s)  # consume '}'
        s = ns2
        let (expr_text, fmt_spec) = split_format_spec(string_trim(expr_buf))
        segments = segments ++ [(true, expr_text, fmt_spec)]
      else
        cur_segment = cur_segment ++ ch
        let (ns, _) = lexer_advance(s)
        s = ns
      end
    end
  end

  let sp = span_from(s, so, sl, sc)
  if is_interp or length(segments) > 0 then
    if cur_segment != "" then
      segments = segments ++ [(false, cur_segment, null)]
    end
    let interp_segments = list_map(segments, fn(seg: (Bool, String, String?)) -> InterpSegment
      let (is_expr, text, fmt) = seg
      InterpSegment(is_expr: is_expr, text: text, format_spec: fmt)
    end)
    let tok = Token(
      kind:   TokenKind.StringInterpLit(payload: StringInterpVal(segments: interp_segments)),
      lexeme: "",
      span:   sp
    )
    Ok((s, tok))
  else
    # Apply dedent to the raw content
    let dedented = dedent_string(cur_segment)
    let tok = Token(
      kind:   TokenKind.StringLit(payload: StringLitVal(value: dedented)),
      lexeme: dedented,
      span:   sp
    )
    Ok((s, tok))
  end
end

# Strip common leading whitespace from a multi-line string.
cell dedent_string(s: String) -> String
  let lines = string_split(s, "\n")
  if length(lines) <= 1 then
    return s
  end
  # Find minimum indent of non-empty lines (skipping first line)
  let min_indent = 999999
  for line in list_skip(lines, 1)
    let trimmed = string_trim_start(line)
    if length(trimmed) > 0 then
      let indent = length(line) - length(trimmed)
      if indent < min_indent then
        min_indent = indent
      end
    end
  end
  if min_indent == 999999 then
    min_indent = 0
  end
  let result: list[String] = []
  for (i, line) in list_enumerate(lines)
    if i == 0 then
      result = result ++ [line]
    else
      if length(line) >= min_indent then
        result = result ++ [string_slice(line, min_indent, length(line))]
      else
        result = result ++ [string_trim(line)]
      end
    end
  end
  # Trim leading/trailing empty lines
  let joined = string_join(result, "\n")
  string_trim(joined)
end

# Process one escape sequence after the backslash has been consumed.
# Returns (updated_state, unescaped_string).
cell process_escape(st: LexerState, sl: Int, sc: Int) -> result[(LexerState, String), LexError]
  let ch = lexer_current(st)
  match ch
    case "n"  -> let (ns, _) = lexer_advance(st)  in Ok((ns, "\n"))
    case "t"  -> let (ns, _) = lexer_advance(st)  in Ok((ns, "\t"))
    case "r"  -> let (ns, _) = lexer_advance(st)  in Ok((ns, "\r"))
    case "\\" -> let (ns, _) = lexer_advance(st)  in Ok((ns, "\\"))
    case "\"" -> let (ns, _) = lexer_advance(st)  in Ok((ns, "\""))
    case "{"  -> let (ns, _) = lexer_advance(st)  in Ok((ns, "{"))
    case "0"  -> let (ns, _) = lexer_advance(st)  in Ok((ns, "\0"))
    case "u"  ->
      let (ns, _) = lexer_advance(st)  # skip 'u'
      match read_unicode_escape(ns, sl, sc)
        case Ok((ns2, codepoint)) -> Ok((ns2, codepoint))
        case Err(e) -> Err(e)
      end
    case "" ->
      Err(LexError.UnterminatedString(line: st.base_line + sl - 1, col: sc))
    case other ->
      let (ns, _) = lexer_advance(st)
      Ok((ns, "\\" ++ other))
  end
end

# Read a \u{XXXX} unicode escape sequence.
cell read_unicode_escape(st: LexerState, sl: Int, sc: Int) -> result[(LexerState, String), LexError]
  if lexer_current(st) != "{" then
    return Err(LexError.InvalidUnicodeEscape(line: st.base_line + sl - 1, col: sc))
  end
  let (s1, _) = lexer_advance(st)
  let hex = ""
  let s = s1
  loop
    let ch = lexer_current(s)
    if ch == "}" then
      break
    end
    if ch == "" then
      return Err(LexError.InvalidUnicodeEscape(line: st.base_line + sl - 1, col: sc))
    end
    hex = hex ++ ch
    let (ns, _) = lexer_advance(s)
    s = ns
  end
  let (s2, _) = lexer_advance(s)  # consume '}'
  match char_from_unicode(hex)
    case Ok(c) -> Ok((s2, c))
    case Err(_) -> Err(LexError.InvalidUnicodeEscape(line: st.base_line + sl - 1, col: sc))
  end
end
```

## Operator Scanning

```lumen
# Scan a multi-character or single-character operator/delimiter token.
# Returns (updated_state, Token).
cell scan_operator(st: LexerState) -> (LexerState, Token)
  let so = st.byte_offset
  let sl = st.line
  let sc = st.col
  let ch = lexer_current(st)
  let p1 = lexer_peek(st)
  let p2 = lexer_peek2(st)

  # Three-char operators
  if ch == "." and p1 == "." and p2 == "." then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let (s3, _) = lexer_advance(s2)
    let sp = span_from(s3, so, sl, sc)
    return (s3, Token(kind: TokenKind.DotDotDot, lexeme: "...", span: sp))
  end
  if ch == "." and p1 == "." and p2 == "=" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let (s3, _) = lexer_advance(s2)
    let sp = span_from(s3, so, sl, sc)
    return (s3, Token(kind: TokenKind.DotDotEq, lexeme: "..=", span: sp))
  end
  if ch == "*" and p1 == "*" and p2 == "=" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let (s3, _) = lexer_advance(s2)
    let sp = span_from(s3, so, sl, sc)
    return (s3, Token(kind: TokenKind.StarStarAssign, lexeme: "**=", span: sp))
  end
  if ch == "/" and p1 == "/" and p2 == "=" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let (s3, _) = lexer_advance(s2)
    let sp = span_from(s3, so, sl, sc)
    return (s3, Token(kind: TokenKind.FloorDivAssign, lexeme: "//=", span: sp))
  end

  # Two-char operators
  if ch == "=" and p1 == "=" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.Eq, lexeme: "==", span: sp))
  end
  if ch == "!" and p1 == "=" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.NotEq, lexeme: "!=", span: sp))
  end
  if ch == "<" and p1 == "=" and p2 == ">" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let (s3, _) = lexer_advance(s2)
    let sp = span_from(s3, so, sl, sc)
    return (s3, Token(kind: TokenKind.Spaceship, lexeme: "<=>", span: sp))
  end
  if ch == "<" and p1 == "=" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.LtEq, lexeme: "<=", span: sp))
  end
  if ch == ">" and p1 == "=" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.GtEq, lexeme: ">=", span: sp))
  end
  if ch == "<" and p1 == "<" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.LeftShift, lexeme: "<<", span: sp))
  end
  if ch == ">" and p1 == ">" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.RightShift, lexeme: ">>", span: sp))
  end
  if ch == "-" and p1 == ">" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.Arrow, lexeme: "->", span: sp))
  end
  if ch == "=" and p1 == ">" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.FatArrow, lexeme: "=>", span: sp))
  end
  if ch == "|" and p1 == ">" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.PipeForward, lexeme: "|>", span: sp))
  end
  if ch == "~" and p1 == ">" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.ComposeArrow, lexeme: "~>", span: sp))
  end
  if ch == "." and p1 == "." then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.DotDot, lexeme: "..", span: sp))
  end
  if ch == "*" and p1 == "*" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.StarStar, lexeme: "**", span: sp))
  end
  if ch == "+" and p1 == "+" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.PlusPlus, lexeme: "++", span: sp))
  end
  if ch == "?" and p1 == "?" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.QuestionQuestion, lexeme: "??", span: sp))
  end
  if ch == "?" and p1 == "." then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.QuestionDot, lexeme: "?.", span: sp))
  end
  if ch == "?" and p1 == "[" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.QuestionBracket, lexeme: "?[", span: sp))
  end
  if ch == "/" and p1 == "/" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.FloorDiv, lexeme: "//", span: sp))
  end
  # Compound assignments
  if ch == "+" and p1 == "=" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.PlusAssign, lexeme: "+=", span: sp))
  end
  if ch == "-" and p1 == "=" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.MinusAssign, lexeme: "-=", span: sp))
  end
  if ch == "*" and p1 == "=" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.StarAssign, lexeme: "*=", span: sp))
  end
  if ch == "/" and p1 == "=" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.SlashAssign, lexeme: "/=", span: sp))
  end
  if ch == "%" and p1 == "=" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.PercentAssign, lexeme: "%=", span: sp))
  end
  if ch == "&" and p1 == "=" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.AmpAssign, lexeme: "&=", span: sp))
  end
  if ch == "|" and p1 == "=" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.PipeAssign, lexeme: "|=", span: sp))
  end
  if ch == "^" and p1 == "=" then
    let (s1, _) = lexer_advance(st)
    let (s2, _) = lexer_advance(s1)
    let sp = span_from(s2, so, sl, sc)
    return (s2, Token(kind: TokenKind.CaretAssign, lexeme: "^=", span: sp))
  end

  # Single-char tokens
  let (ns, _) = lexer_advance(st)
  let sp = span_from(ns, so, sl, sc)
  let kind = match ch
    case "+"  -> TokenKind.Plus
    case "-"  -> TokenKind.Minus
    case "*"  -> TokenKind.Star
    case "/"  -> TokenKind.Slash
    case "%"  -> TokenKind.Percent
    case "<"  -> TokenKind.Lt
    case ">"  -> TokenKind.Gt
    case "="  -> TokenKind.Assign
    case "."  -> TokenKind.Dot
    case ","  -> TokenKind.Comma
    case ":"  -> TokenKind.Colon
    case ";"  -> TokenKind.Semicolon
    case "|"  -> TokenKind.Pipe
    case "@"  -> TokenKind.At
    case "#"  -> TokenKind.Hash
    case "!"  -> TokenKind.Bang
    case "?"  -> TokenKind.Question
    case "&"  -> TokenKind.Ampersand
    case "~"  -> TokenKind.Tilde
    case "^"  -> TokenKind.Caret
    case "("  -> TokenKind.LParen
    case ")"  -> TokenKind.RParen
    case "["  -> TokenKind.LBracket
    case "]"  -> TokenKind.RBracket
    case "{"  -> TokenKind.LBrace
    case "}"  -> TokenKind.RBrace
    case _    -> TokenKind.Symbol(payload: SymbolVal(ch: ch))
  end
  (ns, Token(kind: kind, lexeme: ch, span: sp))
end
```

## Main Lexer Loop

```lumen
# Tokenize one source line/token from the current position.
# Returns result[(updated_state, Token), LexError].
cell next_token(st: LexerState) -> result[(LexerState, Token?), LexError]
  # Drain pending tokens first (INDENT/DEDENT)
  if length(st.pending) > 0 then
    let tok = st.pending[0]
    let new_pending = list_skip(st.pending, 1)
    let ns = LexerState(
      source:        st.source,
      pos:           st.pos,
      line:          st.line,
      col:           st.col,
      byte_offset:   st.byte_offset,
      base_line:     st.base_line,
      base_offset:   st.base_offset,
      indent_stack:  st.indent_stack,
      pending:       new_pending,
      at_line_start: st.at_line_start
    )
    return Ok((ns, tok))
  end

  let s = st

  # Handle indentation at the start of a line
  if s.at_line_start then
    match handle_indentation(s)
      case Ok(ns) -> s = ns
      case Err(e) -> return Err(e)
    end
    # Drain any pending INDENT/DEDENT emitted
    if length(s.pending) > 0 then
      let tok = s.pending[0]
      let new_pending = list_skip(s.pending, 1)
      let ns = LexerState(
        source:        s.source,
        pos:           s.pos,
        line:          s.line,
        col:           s.col,
        byte_offset:   s.byte_offset,
        base_line:     s.base_line,
        base_offset:   s.base_offset,
        indent_stack:  s.indent_stack,
        pending:       new_pending,
        at_line_start: s.at_line_start
      )
      return Ok((ns, tok))
    end
  end

  # Skip horizontal whitespace
  loop
    let ch = lexer_current(s)
    if ch == " " or ch == "\t" or ch == "\r" then
      let (ns, _) = lexer_advance(s)
      s = ns
    else
      break
    end
  end

  let ch = lexer_current(s)

  # EOF
  if ch == "" then
    # Emit remaining DEDENTs before EOF
    if length(s.indent_stack) > 1 then
      match handle_indentation(s)
        case Ok(ns) -> s = ns
        case Err(e) -> return Err(e)
      end
      if length(s.pending) > 0 then
        let tok = s.pending[0]
        let new_pending = list_skip(s.pending, 1)
        let ns = LexerState(
          source: s.source, pos: s.pos, line: s.line, col: s.col,
          byte_offset: s.byte_offset, base_line: s.base_line,
          base_offset: s.base_offset, indent_stack: s.indent_stack,
          pending: new_pending, at_line_start: s.at_line_start
        )
        return Ok((ns, tok))
      end
    end
    let sp = span_here(s)
    return Ok((s, Token(kind: TokenKind.Eof, lexeme: "", span: sp)))
  end

  # Newline
  if ch == "\n" then
    let so = s.byte_offset
    let sl = s.line
    let sc = s.col
    let (ns, _) = lexer_advance(s)
    let sp = span_from(ns, so, sl, sc)
    return Ok((ns, Token(kind: TokenKind.Newline, lexeme: "\n", span: sp)))
  end

  # Comments: skip to end of line
  if ch == "#" then
    loop
      let c = lexer_current(s)
      if c == "" or c == "\n" then
        break
      end
      let (ns, _) = lexer_advance(s)
      s = ns
    end
    # Recurse to get the next real token
    return next_token(s)
  end

  # String literals
  if ch == "\"" then
    match scan_string(s)
      case Ok((ns, tok)) -> return Ok((ns, tok))
      case Err(e) -> return Err(e)
    end
  end

  # Raw strings: r"..." or r"""..."""
  if ch == "r" and lexer_peek(s) == "\"" then
    let (s1, _) = lexer_advance(s)  # skip 'r'
    let so = s1.byte_offset - 1
    let sl = s.line
    let sc = s.col
    # Just lex as normal string but tag as raw
    match scan_string(s1)
      case Ok((ns, tok)) ->
        # Re-tag as raw string (no escape processing needed for display,
        # but we already processed escapes — acceptable approximation)
        let raw_tok = match tok.kind
          case TokenKind.StringLit(payload: sv) ->
            Token(kind: TokenKind.RawStringLit(payload: sv), lexeme: tok.lexeme, span: tok.span)
          case _ -> tok
        end
        return Ok((ns, raw_tok))
      case Err(e) -> return Err(e)
    end
  end

  # Directive: @directive_name
  if ch == "@" then
    let so = s.byte_offset
    let sl = s.line
    let sc = s.col
    let (s1, _) = lexer_advance(s)  # consume '@'
    let name = ""
    let s2 = s1
    loop
      let c = lexer_current(s2)
      if is_ident_char(c) then
        name = name ++ c
        let (ns, _) = lexer_advance(s2)
        s2 = ns
      else
        break
      end
    end
    if name != "" then
      let sp = span_from(s2, so, sl, sc)
      return Ok((s2, Token(
        kind:   TokenKind.Directive(payload: DirectiveVal(name: name)),
        lexeme: "@" ++ name,
        span:   sp
      )))
    end
    # Bare '@' — fall through to operator scanner
  end

  # Numbers
  if is_digit(ch) then
    match scan_number(s)
      case Ok((ns, tok)) -> return Ok((ns, tok))
      case Err(e) -> return Err(e)
    end
  end

  # Identifiers and keywords
  if is_ident_start(ch) then
    let so = s.byte_offset
    let sl = s.line
    let sc = s.col
    let word = ""
    loop
      let c = lexer_current(s)
      if is_ident_char(c) then
        word = word ++ c
        let (ns, _) = lexer_advance(s)
        s = ns
      else
        break
      end
    end
    let sp = span_from(s, so, sl, sc)
    let kind = match keyword_kind(word)
      case null -> TokenKind.Ident(payload: IdentVal(name: word))
      case k    -> k
    end
    return Ok((s, Token(kind: kind, lexeme: word, span: sp)))
  end

  # Operators and delimiters
  let (ns, tok) = scan_operator(s)
  Ok((ns, tok))
end

# Character class helpers

cell is_digit(ch: String) -> Bool
  ch >= "0" and ch <= "9"
end

cell is_hex_digit(ch: String) -> Bool
  (ch >= "0" and ch <= "9") or (ch >= "a" and ch <= "f") or (ch >= "A" and ch <= "F")
end

cell is_ident_start(ch: String) -> Bool
  (ch >= "a" and ch <= "z") or (ch >= "A" and ch <= "Z") or ch == "_"
end

cell is_ident_char(ch: String) -> Bool
  is_ident_start(ch) or is_digit(ch)
end
```

## Public Entry Point

```lumen
# Tokenize an entire source string.
# Returns the full token list (including the final Eof token) or a LexError.
cell lex(source: String) -> result[list[Token], LexError]
  lex_with_offset(source, 1, 0)
end

# Tokenize source with a custom base_line and base_offset.
# Used when lexing extracted code blocks that started at a non-zero offset.
cell lex_with_offset(source: String, base_line: Int, base_offset: Int) -> result[list[Token], LexError]
  let st = new_lexer(source, base_line, base_offset)
  let tokens: list[Token] = []

  loop
    match next_token(st)
      case Err(e) -> return Err(e)
      case Ok((ns, tok)) ->
        st = ns
        tokens = tokens ++ [tok]
        if tok.kind == TokenKind.Eof then
          break
        end
    end
  end

  Ok(tokens)
end
```

## Lexer Tests

```lumen
import std.testing: assert_eq, assert_true, test

test "lex empty string" do
  match lex("")
    case Ok(toks) ->
      assert_eq(length(toks), 1)
      assert_eq(toks[0].kind, TokenKind.Eof)
    case Err(e) -> assert_true(false, "unexpected lex error")
  end
end

test "lex integer literal" do
  match lex("42")
    case Ok(toks) ->
      assert_eq(length(toks), 2)  # IntLit + Eof
      match toks[0].kind
        case TokenKind.IntLit(payload: v) -> assert_eq(v.value, 42)
        case _ -> assert_true(false, "expected IntLit")
      end
    case Err(e) -> assert_true(false, "unexpected lex error")
  end
end

test "lex keywords" do
  match lex("cell record enum")
    case Ok(toks) ->
      assert_eq(toks[0].kind, TokenKind.KwCell)
      assert_eq(toks[2].kind, TokenKind.KwRecord)
      assert_eq(toks[4].kind, TokenKind.KwEnum)
    case Err(e) -> assert_true(false, "unexpected lex error")
  end
end

test "lex string literal" do
  match lex("\"hello\"")
    case Ok(toks) ->
      match toks[0].kind
        case TokenKind.StringLit(payload: sv) -> assert_eq(sv.value, "hello")
        case _ -> assert_true(false, "expected StringLit")
      end
    case Err(e) -> assert_true(false, "unexpected lex error")
  end
end

test "lex boolean literals" do
  match lex("true false")
    case Ok(toks) ->
      match toks[0].kind
        case TokenKind.BoolLit(payload: bv) -> assert_true(bv.value)
        case _ -> assert_true(false, "expected BoolLit(true)")
      end
      match toks[2].kind
        case TokenKind.BoolLit(payload: bv) -> assert_true(!bv.value)
        case _ -> assert_true(false, "expected BoolLit(false)")
      end
    case Err(e) -> assert_true(false, "unexpected lex error")
  end
end

test "lex arrow operator" do
  match lex("->")
    case Ok(toks) ->
      assert_eq(toks[0].kind, TokenKind.Arrow)
    case Err(e) -> assert_true(false, "unexpected lex error")
  end
end

test "lex pipe forward" do
  match lex("|>")
    case Ok(toks) ->
      assert_eq(toks[0].kind, TokenKind.PipeForward)
    case Err(e) -> assert_true(false, "unexpected lex error")
  end
end

test "lex identifier" do
  match lex("my_var")
    case Ok(toks) ->
      match toks[0].kind
        case TokenKind.Ident(payload: iv) -> assert_eq(iv.name, "my_var")
        case _ -> assert_true(false, "expected Ident")
      end
    case Err(e) -> assert_true(false, "unexpected lex error")
  end
end
```
