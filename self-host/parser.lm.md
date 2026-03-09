# Self-Hosted Lumen Parser

Pratt parser for Lumen source code, ported from
`rust/lumen-compiler/src/compiler/parser.rs`.

Implements Phase 2 (S091–S160) of the self-hosting plan.

## Imports

```lumen
import std.compiler.span: Span, make_span, span_union
import std.compiler.tokens: Token, TokenKind, IdentVal, IntLitVal,
  FloatLitVal, StringLitVal, BoolLitVal, StringInterpVal
import std.compiler.ast: Program, Directive, Item, Expr, Stmt, Pattern,
  TypeExpr, BinOp, UnaryOp, GenericParam, Param, FieldDef, EnumVariant,
  RecordDef, EnumDef, CellDef, ImportDef, LetStmt, ReturnStmt,
  ForStmt, MatchStmt, WhileStmt, BreakStmt, ContinueStmt, AssignStmt,
  ExprStmt, MatchArm, IfExpr, CallExpr, FieldAccess, IndexExpr,
  BinOpExpr, UnaryOpExpr, LambdaExpr, RecordLiteral, ListLiteral,
  TupleLiteral, MapLiteral, Block
import self_host.errors: ParseError
```

## Parser State

```lumen
# Parser holds the flat token list and a cursor into it.
record Parser(
  tokens:   list[Token],  # all tokens from the lexer (includes Newline/Eof)
  pos:      Int,          # current position
  errors:   list[ParseError]
)

cell new_parser(tokens: list[Token]) -> Parser
  Parser(tokens: tokens, pos: 0, errors: [])
end
```

## Token Stream Navigation

```lumen
# Return the current token (never past Eof).
cell parser_current(p: Parser) -> Token
  # Skip Newline tokens for most navigation; callers that care use peek_raw
  let i = p.pos
  loop
    if i >= length(p.tokens) then
      return p.tokens[length(p.tokens) - 1]  # last token should be Eof
    end
    let tok = p.tokens[i]
    if tok.kind == TokenKind.Newline then
      i = i + 1
    else
      return tok
    end
  end
  p.tokens[length(p.tokens) - 1]
end

# Return the current token including Newline tokens.
cell parser_current_raw(p: Parser) -> Token
  if p.pos >= length(p.tokens) then
    p.tokens[length(p.tokens) - 1]
  else
    p.tokens[p.pos]
  end
end

# Look ahead by n tokens (skipping Newline).
cell parser_peek_n(p: Parser, n: Int) -> Token
  let count = 0
  let i = p.pos
  loop
    if i >= length(p.tokens) then
      return p.tokens[length(p.tokens) - 1]
    end
    let tok = p.tokens[i]
    if tok.kind == TokenKind.Newline then
      i = i + 1
    else
      if count == n then
        return tok
      end
      count = count + 1
      i = i + 1
    end
  end
  p.tokens[length(p.tokens) - 1]
end

# Advance past the current token (including Newlines) and return it.
cell parser_advance_raw(p: Parser) -> (Parser, Token)
  if p.pos >= length(p.tokens) then
    return (p, p.tokens[length(p.tokens) - 1])
  end
  let tok = p.tokens[p.pos]
  let np = Parser(tokens: p.tokens, pos: p.pos + 1, errors: p.errors)
  (np, tok)
end

# Advance past the current token (skipping any leading Newlines), return it.
cell parser_advance(p: Parser) -> (Parser, Token)
  # Skip newlines first
  let sp = skip_newlines(p)
  if sp.pos >= length(sp.tokens) then
    return (sp, sp.tokens[length(sp.tokens) - 1])
  end
  let tok = sp.tokens[sp.pos]
  let np = Parser(tokens: sp.tokens, pos: sp.pos + 1, errors: sp.errors)
  (np, tok)
end

# Skip all Newline tokens at current position.
cell skip_newlines(p: Parser) -> Parser
  let i = p.pos
  loop
    if i >= length(p.tokens) then
      break
    end
    if p.tokens[i].kind == TokenKind.Newline then
      i = i + 1
    else
      break
    end
  end
  Parser(tokens: p.tokens, pos: i, errors: p.errors)
end

# Return true if the current (non-Newline) token has the given kind.
cell parser_check(p: Parser, kind: TokenKind) -> Bool
  let tok = parser_current(p)
  token_kind_eq(tok.kind, kind)
end

# Consume the current token if it matches; return (parser, token?) where
# token is null if the token didn't match.
cell parser_match_tok(p: Parser, kind: TokenKind) -> (Parser, Token?)
  let tok = parser_current(p)
  if token_kind_eq(tok.kind, kind) then
    let (np, t) = parser_advance(p)
    (np, t)
  else
    (p, null)
  end
end

# Expect a token of the given kind; record an error if not found.
cell parser_expect(p: Parser, kind: TokenKind, msg: String) -> (Parser, Token?)
  let tok = parser_current(p)
  if token_kind_eq(tok.kind, kind) then
    let (np, t) = parser_advance(p)
    (np, t)
  else
    let err = ParseError.UnexpectedToken(
      expected: msg,
      found:    token_kind_name(tok.kind),
      span:     tok.span
    )
    let np = Parser(tokens: p.tokens, pos: p.pos, errors: p.errors ++ [err])
    (np, null)
  end
end

# Check if we are at Eof.
cell parser_at_end(p: Parser) -> Bool
  let tok = parser_current(p)
  tok.kind == TokenKind.Eof
end

# Add a parse error without consuming a token.
cell parser_error(p: Parser, msg: String, span: Span) -> Parser
  let err = ParseError.General(message: msg, span: span)
  Parser(tokens: p.tokens, pos: p.pos, errors: p.errors ++ [err])
end

# Synchronize to the next top-level keyword.
cell synchronize_top(p: Parser) -> Parser
  let s = p
  loop
    if parser_at_end(s) then
      break
    end
    let tok = parser_current(s)
    match tok.kind
      case TokenKind.KwCell     -> break
      case TokenKind.KwRecord   -> break
      case TokenKind.KwEnum     -> break
      case TokenKind.KwImport   -> break
      case TokenKind.KwType     -> break
      case TokenKind.KwTrait    -> break
      case TokenKind.KwImpl     -> break
      case TokenKind.KwConst    -> break
      case TokenKind.KwEffect   -> break
      case _ ->
        let (ns, _) = parser_advance(s)
        s = ns
    end
  end
  s
end

# Synchronize to the next statement boundary (newline or `end`).
cell synchronize_stmt(p: Parser) -> Parser
  let s = p
  loop
    if parser_at_end(s) then
      break
    end
    let tok = parser_current_raw(s)
    match tok.kind
      case TokenKind.Newline ->
        let (ns, _) = parser_advance_raw(s)
        s = ns
        break
      case TokenKind.KwEnd -> break
      case TokenKind.KwReturn -> break
      case _ ->
        let (ns, _) = parser_advance_raw(s)
        s = ns
    end
  end
  s
end

# Simple structural equality on TokenKind (discriminant only for most cases).
cell token_kind_eq(a: TokenKind, b: TokenKind) -> Bool
  match (a, b)
    case (TokenKind.KwRecord, TokenKind.KwRecord)         -> true
    case (TokenKind.KwEnum, TokenKind.KwEnum)             -> true
    case (TokenKind.KwCell, TokenKind.KwCell)             -> true
    case (TokenKind.KwLet, TokenKind.KwLet)               -> true
    case (TokenKind.KwIf, TokenKind.KwIf)                 -> true
    case (TokenKind.KwElse, TokenKind.KwElse)             -> true
    case (TokenKind.KwFor, TokenKind.KwFor)               -> true
    case (TokenKind.KwIn, TokenKind.KwIn)                 -> true
    case (TokenKind.KwMatch, TokenKind.KwMatch)           -> true
    case (TokenKind.KwReturn, TokenKind.KwReturn)         -> true
    case (TokenKind.KwEnd, TokenKind.KwEnd)               -> true
    case (TokenKind.KwImport, TokenKind.KwImport)         -> true
    case (TokenKind.KwAs, TokenKind.KwAs)                 -> true
    case (TokenKind.KwFrom, TokenKind.KwFrom)             -> true
    case (TokenKind.KwType, TokenKind.KwType)             -> true
    case (TokenKind.KwConst, TokenKind.KwConst)           -> true
    case (TokenKind.KwTrait, TokenKind.KwTrait)           -> true
    case (TokenKind.KwImpl, TokenKind.KwImpl)             -> true
    case (TokenKind.KwWhile, TokenKind.KwWhile)           -> true
    case (TokenKind.KwLoop, TokenKind.KwLoop)             -> true
    case (TokenKind.KwBreak, TokenKind.KwBreak)           -> true
    case (TokenKind.KwContinue, TokenKind.KwContinue)     -> true
    case (TokenKind.KwMut, TokenKind.KwMut)               -> true
    case (TokenKind.KwPub, TokenKind.KwPub)               -> true
    case (TokenKind.KwAnd, TokenKind.KwAnd)               -> true
    case (TokenKind.KwOr, TokenKind.KwOr)                 -> true
    case (TokenKind.KwNot, TokenKind.KwNot)               -> true
    case (TokenKind.KwNull, TokenKind.KwNull)             -> true
    case (TokenKind.KwHalt, TokenKind.KwHalt)             -> true
    case (TokenKind.KwThen, TokenKind.KwThen)             -> true
    case (TokenKind.KwDo, TokenKind.KwDo)                 -> true
    case (TokenKind.KwWhen, TokenKind.KwWhen)             -> true
    case (TokenKind.KwIs, TokenKind.KwIs)                 -> true
    case (TokenKind.Plus, TokenKind.Plus)                 -> true
    case (TokenKind.Minus, TokenKind.Minus)               -> true
    case (TokenKind.Star, TokenKind.Star)                 -> true
    case (TokenKind.Slash, TokenKind.Slash)               -> true
    case (TokenKind.Percent, TokenKind.Percent)           -> true
    case (TokenKind.Eq, TokenKind.Eq)                     -> true
    case (TokenKind.NotEq, TokenKind.NotEq)               -> true
    case (TokenKind.Lt, TokenKind.Lt)                     -> true
    case (TokenKind.LtEq, TokenKind.LtEq)                 -> true
    case (TokenKind.Gt, TokenKind.Gt)                     -> true
    case (TokenKind.GtEq, TokenKind.GtEq)                 -> true
    case (TokenKind.Assign, TokenKind.Assign)             -> true
    case (TokenKind.Arrow, TokenKind.Arrow)               -> true
    case (TokenKind.Dot, TokenKind.Dot)                   -> true
    case (TokenKind.Comma, TokenKind.Comma)               -> true
    case (TokenKind.Colon, TokenKind.Colon)               -> true
    case (TokenKind.Semicolon, TokenKind.Semicolon)       -> true
    case (TokenKind.Pipe, TokenKind.Pipe)                 -> true
    case (TokenKind.PipeForward, TokenKind.PipeForward)   -> true
    case (TokenKind.ComposeArrow, TokenKind.ComposeArrow) -> true
    case (TokenKind.DotDot, TokenKind.DotDot)             -> true
    case (TokenKind.DotDotEq, TokenKind.DotDotEq)         -> true
    case (TokenKind.StarStar, TokenKind.StarStar)         -> true
    case (TokenKind.PlusPlus, TokenKind.PlusPlus)         -> true
    case (TokenKind.LParen, TokenKind.LParen)             -> true
    case (TokenKind.RParen, TokenKind.RParen)             -> true
    case (TokenKind.LBracket, TokenKind.LBracket)         -> true
    case (TokenKind.RBracket, TokenKind.RBracket)         -> true
    case (TokenKind.LBrace, TokenKind.LBrace)             -> true
    case (TokenKind.RBrace, TokenKind.RBrace)             -> true
    case (TokenKind.Newline, TokenKind.Newline)           -> true
    case (TokenKind.Indent, TokenKind.Indent)             -> true
    case (TokenKind.Dedent, TokenKind.Dedent)             -> true
    case (TokenKind.Eof, TokenKind.Eof)                   -> true
    case (TokenKind.KwFn, TokenKind.KwFn)                 -> true
    case _ -> false
  end
end

# Produce a human-readable name for a token kind (for error messages).
cell token_kind_name(kind: TokenKind) -> String
  match kind
    case TokenKind.Ident(_)     -> "identifier"
    case TokenKind.IntLit(_)    -> "integer literal"
    case TokenKind.FloatLit(_)  -> "float literal"
    case TokenKind.StringLit(_) -> "string literal"
    case TokenKind.BoolLit(_)   -> "boolean literal"
    case TokenKind.KwCell       -> "'cell'"
    case TokenKind.KwRecord     -> "'record'"
    case TokenKind.KwEnum       -> "'enum'"
    case TokenKind.KwEnd        -> "'end'"
    case TokenKind.KwIf         -> "'if'"
    case TokenKind.KwElse       -> "'else'"
    case TokenKind.KwThen       -> "'then'"
    case TokenKind.KwFor        -> "'for'"
    case TokenKind.KwIn         -> "'in'"
    case TokenKind.KwMatch      -> "'match'"
    case TokenKind.KwReturn     -> "'return'"
    case TokenKind.KwLet        -> "'let'"
    case TokenKind.KwImport     -> "'import'"
    case TokenKind.Arrow        -> "'->'"
    case TokenKind.Assign       -> "'='"
    case TokenKind.Colon        -> "':'"
    case TokenKind.Comma        -> "','"
    case TokenKind.LParen       -> "'('"
    case TokenKind.RParen       -> "')'"
    case TokenKind.LBracket     -> "'['"
    case TokenKind.RBracket     -> "']'"
    case TokenKind.LBrace       -> "'{'"
    case TokenKind.RBrace       -> "'}'"
    case TokenKind.Dot          -> "'.'"
    case TokenKind.Eof          -> "end of file"
    case TokenKind.Newline      -> "newline"
    case _                      -> "token"
  end
end
```

## Type Expression Parsing

```lumen
# Parse a type expression (TypeExpr).
# Grammar:
#   type_expr = named_type | list_type | map_type | result_type |
#               union_type | tuple_type | set_type | fn_type | generic_type | nullable
cell parse_type_expr(p: Parser) -> result[(Parser, TypeExpr), ParseError]
  let (p2, base) = parse_type_primary(p)?
  # Check for union: Type | Type
  let result_type = base
  let sp = p2
  loop
    let tok = parser_current(sp)
    if tok.kind == TokenKind.Pipe then
      let (p3, _) = parser_advance(sp)
      let (p4, rhs) = parse_type_primary(p3)?
      result_type = TypeExpr.UnionType(payload: make_union_type(result_type, rhs, tok.span))
      sp = p4
    else
      break
    end
  end
  Ok((sp, result_type))
end

cell make_union_type(left: TypeExpr, right: TypeExpr, span: Span) -> UnionTypeExpr
  # Flatten nested unions into a flat list
  let members: list[TypeExpr] = []
  match left
    case TypeExpr.UnionType(payload: u) -> members = u.members
    case _ -> members = [left]
  end
  members = members ++ [right]
  UnionTypeExpr(members: members, span: span)
end

cell parse_type_primary(p: Parser) -> result[(Parser, TypeExpr), ParseError]
  let tok = parser_current(p)
  match tok.kind
    # Nullable shorthand: Type?
    case TokenKind.KwNull ->
      let (p2, _) = parser_advance(p)
      Ok((p2, TypeExpr.NullType(payload: NullTypeExpr(span: tok.span))))

    # list[T]
    case TokenKind.KwList ->
      let (p2, _) = parser_advance(p)
      let (p3, _) = parser_expect(p2, TokenKind.LBracket, "[")?
      let (p4, elem) = parse_type_expr(p3)?
      let (p5, close) = parser_expect(p4, TokenKind.RBracket, "]")?
      let sp = match close
        case null -> tok.span
        case t    -> span_union(tok.span, t.span)
      end
      Ok((p5, TypeExpr.ListType(payload: ListTypeExpr(element: elem, span: sp))))

    # map[K, V]
    case TokenKind.KwMap ->
      let (p2, _) = parser_advance(p)
      let (p3, _) = parser_expect(p2, TokenKind.LBracket, "[")?
      let (p4, key) = parse_type_expr(p3)?
      let (p5, _) = parser_expect(p4, TokenKind.Comma, ",")?
      let (p6, val) = parse_type_expr(p5)?
      let (p7, close) = parser_expect(p6, TokenKind.RBracket, "]")?
      let sp = match close
        case null -> tok.span
        case t    -> span_union(tok.span, t.span)
      end
      Ok((p7, TypeExpr.MapType(payload: MapTypeExpr(key: key, value: val, span: sp))))

    # result[Ok, Err]
    case TokenKind.KwResult ->
      let (p2, _) = parser_advance(p)
      let (p3, _) = parser_expect(p2, TokenKind.LBracket, "[")?
      let (p4, ok) = parse_type_expr(p3)?
      let (p5, _) = parser_expect(p4, TokenKind.Comma, ",")?
      let (p6, err) = parse_type_expr(p5)?
      let (p7, close) = parser_expect(p6, TokenKind.RBracket, "]")?
      let sp = match close
        case null -> tok.span
        case t    -> span_union(tok.span, t.span)
      end
      Ok((p7, TypeExpr.ResultType(payload: ResultTypeExpr(ok: ok, err: err, span: sp))))

    # set[T]
    case TokenKind.KwSet ->
      let (p2, _) = parser_advance(p)
      let (p3, _) = parser_expect(p2, TokenKind.LBracket, "[")?
      let (p4, elem) = parse_type_expr(p3)?
      let (p5, close) = parser_expect(p4, TokenKind.RBracket, "]")?
      let sp = match close
        case null -> tok.span
        case t    -> span_union(tok.span, t.span)
      end
      Ok((p5, TypeExpr.SetType(payload: SetTypeExpr(element: elem, span: sp))))

    # tuple[T, U]
    case TokenKind.KwTuple ->
      let (p2, _) = parser_advance(p)
      let (p3, _) = parser_expect(p2, TokenKind.LBracket, "[")?
      let (p4, elems) = parse_comma_sep_types(p3, TokenKind.RBracket)?
      let (p5, close) = parser_expect(p4, TokenKind.RBracket, "]")?
      let sp = match close
        case null -> tok.span
        case t    -> span_union(tok.span, t.span)
      end
      Ok((p5, TypeExpr.TupleType(payload: TupleTypeExpr(elements: elems, span: sp))))

    # fn(T, U) -> R / {effects}
    case TokenKind.KwFn ->
      let (p2, _) = parser_advance(p)
      let (p3, _) = parser_expect(p2, TokenKind.LParen, "(")?
      let (p4, params) = parse_comma_sep_types(p3, TokenKind.RParen)?
      let (p5, _) = parser_expect(p4, TokenKind.RParen, ")")?
      let (p6, _) = parser_expect(p5, TokenKind.Arrow, "->")?
      let (p7, ret) = parse_type_expr(p6)?
      # Optional effect annotation / {eff1, eff2}
      let (p8, effects) = try_parse_effects(p7)
      let sp = tok.span
      Ok((p8, TypeExpr.FnType(payload: FnTypeExpr(params: params, ret: ret, effects: effects, span: sp))))

    # Named or generic: Name or Name[T, U]
    case TokenKind.Ident(payload: iv) ->
      let (p2, _) = parser_advance(p)
      let name = iv.name
      if parser_check(p2, TokenKind.LBracket) then
        let (p3, _) = parser_advance(p2)
        let (p4, args) = parse_comma_sep_types(p3, TokenKind.RBracket)?
        let (p5, close) = parser_expect(p4, TokenKind.RBracket, "]")?
        let sp = match close
          case null -> tok.span
          case t    -> span_union(tok.span, t.span)
        end
        Ok((p5, TypeExpr.GenericType(payload: GenericTypeExpr(name: name, args: args, span: sp))))
      else
        Ok((p2, TypeExpr.Named(payload: NamedTypeExpr(name: name, span: tok.span))))
      end

    # Type keywords that map to Named types
    case TokenKind.KwBool ->
      let (p2, _) = parser_advance(p)
      Ok((p2, TypeExpr.Named(payload: NamedTypeExpr(name: "Bool", span: tok.span))))
    case TokenKind.KwInt ->
      let (p2, _) = parser_advance(p)
      Ok((p2, TypeExpr.Named(payload: NamedTypeExpr(name: "Int", span: tok.span))))
    case TokenKind.KwFloat ->
      let (p2, _) = parser_advance(p)
      Ok((p2, TypeExpr.Named(payload: NamedTypeExpr(name: "Float", span: tok.span))))
    case TokenKind.KwString ->
      let (p2, _) = parser_advance(p)
      Ok((p2, TypeExpr.Named(payload: NamedTypeExpr(name: "String", span: tok.span))))
    case TokenKind.KwBytes ->
      let (p2, _) = parser_advance(p)
      Ok((p2, TypeExpr.Named(payload: NamedTypeExpr(name: "Bytes", span: tok.span))))
    case TokenKind.KwJson ->
      let (p2, _) = parser_advance(p)
      Ok((p2, TypeExpr.Named(payload: NamedTypeExpr(name: "Json", span: tok.span))))

    # Parenthesised type / tuple
    case TokenKind.LParen ->
      let (p2, _) = parser_advance(p)
      let (p3, first) = parse_type_expr(p2)?
      if parser_check(p3, TokenKind.RParen) then
        let (p4, _) = parser_advance(p3)
        Ok((p4, first))
      else
        # Multi-element tuple
        let (p4, _) = parser_expect(p3, TokenKind.Comma, ",")?
        let rest_types = [first]
        let ps = p4
        loop
          if parser_check(ps, TokenKind.RParen) then
            break
          end
          let (pn, t) = parse_type_expr(ps)?
          rest_types = rest_types ++ [t]
          ps = pn
          if parser_check(ps, TokenKind.Comma) then
            let (pn2, _) = parser_advance(ps)
            ps = pn2
          else
            break
          end
        end
        let (p5, close) = parser_expect(ps, TokenKind.RParen, ")")?
        let sp = match close
          case null -> tok.span
          case t    -> span_union(tok.span, t.span)
        end
        Ok((p5, TypeExpr.TupleType(payload: TupleTypeExpr(elements: rest_types, span: sp))))
      end

    case _ ->
      Err(ParseError.UnexpectedToken(
        expected: "type expression",
        found:    token_kind_name(tok.kind),
        span:     tok.span
      ))
  end
end

# Parse a comma-separated list of type expressions until `stop_kind`.
cell parse_comma_sep_types(p: Parser, stop_kind: TokenKind) -> result[(Parser, list[TypeExpr]), ParseError]
  let types: list[TypeExpr] = []
  let s = p
  loop
    if parser_check(s, stop_kind) or parser_at_end(s) then
      break
    end
    let (ns, t) = parse_type_expr(s)?
    types = types ++ [t]
    s = ns
    if parser_check(s, TokenKind.Comma) then
      let (ns2, _) = parser_advance(s)
      s = ns2
    else
      break
    end
  end
  Ok((s, types))
end

# Try to parse an effect annotation `/ {eff1, eff2}`. Returns empty list if absent.
cell try_parse_effects(p: Parser) -> (Parser, list[String])
  if not parser_check(p, TokenKind.Slash) then
    return (p, [])
  end
  let (p2, _) = parser_advance(p)
  if not parser_check(p2, TokenKind.LBrace) then
    return (p2, [])
  end
  let (p3, _) = parser_advance(p2)
  let effects: list[String] = []
  let s = p3
  loop
    if parser_check(s, TokenKind.RBrace) or parser_at_end(s) then
      break
    end
    let tok = parser_current(s)
    match tok.kind
      case TokenKind.Ident(payload: iv) ->
        effects = effects ++ [iv.name]
        let (ns, _) = parser_advance(s)
        s = ns
        if parser_check(s, TokenKind.Comma) then
          let (ns2, _) = parser_advance(s)
          s = ns2
        end
      case _ -> break
    end
  end
  let (p4, _) = parser_expect(s, TokenKind.RBrace, "}")
  (p4, effects)
end
```

## Generic Parameter Parsing

```lumen
# Parse `[T, U: Bound1 + Bound2]` generic parameter list.
cell parse_generic_params(p: Parser) -> result[(Parser, list[GenericParam]), ParseError]
  if not parser_check(p, TokenKind.LBracket) then
    return Ok((p, []))
  end
  let (p2, _) = parser_advance(p)
  let params: list[GenericParam] = []
  let s = p2
  loop
    if parser_check(s, TokenKind.RBracket) or parser_at_end(s) then
      break
    end
    let tok = parser_current(s)
    match tok.kind
      case TokenKind.Ident(payload: iv) ->
        let (ns, _) = parser_advance(s)
        let bounds: list[String] = []
        if parser_check(ns, TokenKind.Colon) then
          let (ns2, _) = parser_advance(ns)
          let ns3 = ns2
          loop
            let bt = parser_current(ns3)
            match bt.kind
              case TokenKind.Ident(payload: bv) ->
                bounds = bounds ++ [bv.name]
                let (ns4, _) = parser_advance(ns3)
                ns3 = ns4
                if parser_check(ns3, TokenKind.Plus) then
                  let (ns5, _) = parser_advance(ns3)
                  ns3 = ns5
                else
                  break
                end
              case _ -> break
            end
          end
          params = params ++ [GenericParam(name: iv.name, bounds: bounds, span: tok.span)]
          s = ns3
        else
          params = params ++ [GenericParam(name: iv.name, bounds: [], span: tok.span)]
          s = ns
        end
        if parser_check(s, TokenKind.Comma) then
          let (ns2, _) = parser_advance(s)
          s = ns2
        end
      case _ -> break
    end
  end
  let (p3, _) = parser_expect(s, TokenKind.RBracket, "]")?
  Ok((p3, params))
end
```

## Item Parsing

```lumen
# Parse a single top-level item.
cell parse_item(p: Parser) -> result[(Parser, Item?), ParseError]
  let tok = parser_current(p)
  match tok.kind
    case TokenKind.KwCell   -> parse_cell_item(p)
    case TokenKind.KwRecord -> parse_record_item(p)
    case TokenKind.KwEnum   -> parse_enum_item(p)
    case TokenKind.KwImport -> parse_import_item(p)
    case TokenKind.KwType   -> parse_type_alias_item(p)
    case TokenKind.KwConst  -> parse_const_item(p)
    case TokenKind.KwPub    -> parse_pub_item(p)
    case _ ->
      # Not a recognized item start — synchronize
      let np = synchronize_top(p)
      Ok((np, null))
  end
end

# Parse `record Name[T] (field: Type, ...) where Constraints end`
cell parse_record_item(p: Parser) -> result[(Parser, Item?), ParseError]
  let (p2, kw) = parser_advance(p)  # consume 'record'
  let name_tok = parser_current(p2)
  let name = match name_tok.kind
    case TokenKind.Ident(payload: iv) -> iv.name
    case _ ->
      let np = parser_error(p2, "expected record name", name_tok.span)
      return Ok((synchronize_top(np), null))
  end
  let (p3, _) = parser_advance(p2)
  let (p4, generics) = parse_generic_params(p3)?
  let (p5, _) = parser_expect(p4, TokenKind.LParen, "(")?
  let (p6, fields) = parse_field_defs(p5)?
  let (p7, close) = parser_expect(p6, TokenKind.RParen, ")")?
  let sp = match kw
    case null -> name_tok.span
    case t    -> match close
      case null -> name_tok.span
      case c    -> span_union(t.span, c.span)
    end
  end
  let def = RecordDef(name: name, generics: generics, fields: fields, span: sp)
  Ok((p7, Item.Record(payload: def)))
end

# Parse field definitions `field: Type, ...`
cell parse_field_defs(p: Parser) -> result[(Parser, list[FieldDef]), ParseError]
  let fields: list[FieldDef] = []
  let s = p
  loop
    if parser_check(s, TokenKind.RParen) or parser_at_end(s) then
      break
    end
    let tok = parser_current(s)
    let name = match tok.kind
      case TokenKind.Ident(payload: iv) -> iv.name
      case _ -> break
    end
    let (ns, _) = parser_advance(s)
    let (ns2, _) = parser_expect(ns, TokenKind.Colon, ":")?
    let (ns3, ty) = parse_type_expr(ns2)?
    # Optional default value `= expr`
    let (ns4, default_val) = if parser_check(ns3, TokenKind.Assign) then
      let (ns5, _) = parser_advance(ns3)
      let (ns6, dv) = parse_expr(ns5, 0)?
      (ns6, dv)
    else
      (ns3, null)
    end
    fields = fields ++ [FieldDef(name: name, ty: ty, default: default_val, span: tok.span)]
    s = ns4
    if parser_check(s, TokenKind.Comma) then
      let (ns5, _) = parser_advance(s)
      s = ns5
    end
  end
  Ok((s, fields))
end

# Parse `enum Name[T] ... VariantName | VariantName(payload: Type) ... end`
cell parse_enum_item(p: Parser) -> result[(Parser, Item?), ParseError]
  let (p2, kw) = parser_advance(p)  # consume 'enum'
  let name_tok = parser_current(p2)
  let name = match name_tok.kind
    case TokenKind.Ident(payload: iv) -> iv.name
    case _ ->
      let np = parser_error(p2, "expected enum name", name_tok.span)
      return Ok((synchronize_top(np), null))
  end
  let (p3, _) = parser_advance(p2)
  let (p4, generics) = parse_generic_params(p3)?
  let s = skip_newlines(p4)
  let variants: list[EnumVariant] = []
  loop
    let tok = parser_current(s)
    match tok.kind
      case TokenKind.KwEnd ->
        let (ns, _) = parser_advance(s)
        s = ns
        break
      case TokenKind.Eof -> break
      case TokenKind.Newline ->
        let (ns, _) = parser_advance_raw(s)
        s = ns
      case TokenKind.Ident(payload: iv) ->
        let vname = iv.name
        let (ns, _) = parser_advance(s)
        # Optional payload `(payload: Type)`
        let (ns2, payload_ty) = if parser_check(ns, TokenKind.LParen) then
          let (ns3, _) = parser_advance(ns)
          # Parse one or more field defs
          let (ns4, fields) = parse_field_defs(ns3)?
          let (ns5, _) = parser_expect(ns4, TokenKind.RParen, ")")?
          (ns5, fields)
        else
          (ns, [])
        end
        variants = variants ++ [EnumVariant(name: vname, fields: payload_ty, span: tok.span)]
        s = skip_newlines(ns2)
      case _ ->
        let (ns, _) = parser_advance(s)
        s = ns
    end
  end
  let def = EnumDef(name: name, generics: generics, variants: variants, span: name_tok.span)
  Ok((s, Item.Enum(payload: def)))
end

# Parse `cell name[T](params) -> ReturnType / {effects} ... end`
cell parse_cell_item(p: Parser) -> result[(Parser, Item?), ParseError]
  let (p2, kw_tok) = parser_advance(p)  # consume 'cell'
  let name_tok = parser_current(p2)
  let name = match name_tok.kind
    case TokenKind.Ident(payload: iv) -> iv.name
    case _ ->
      let np = parser_error(p2, "expected cell name", name_tok.span)
      return Ok((synchronize_top(np), null))
  end
  let (p3, _) = parser_advance(p2)
  let (p4, generics) = parse_generic_params(p3)?
  let (p5, _) = parser_expect(p4, TokenKind.LParen, "(")?
  let (p6, params) = parse_params(p5)?
  let (p7, _) = parser_expect(p6, TokenKind.RParen, ")")?
  # Return type
  let (p8, ret_ty) = if parser_check(p7, TokenKind.Arrow) then
    let (ps, _) = parser_advance(p7)
    let (ps2, t) = parse_type_expr(ps)?
    (ps2, t)
  else
    (p7, TypeExpr.Named(payload: NamedTypeExpr(name: "Null", span: name_tok.span)))
  end
  # Effects
  let (p9, effects) = try_parse_effects(p8)
  # Body
  let (p10, body) = parse_block(p9)?
  let def = CellDef(
    name:     name,
    generics: generics,
    params:   params,
    ret_ty:   ret_ty,
    effects:  effects,
    body:     body,
    span:     name_tok.span
  )
  Ok((p10, Item.Cell(payload: def)))
end

# Parse a formal parameter list `name: Type, ...`
cell parse_params(p: Parser) -> result[(Parser, list[Param]), ParseError]
  let params: list[Param] = []
  let s = p
  loop
    if parser_check(s, TokenKind.RParen) or parser_at_end(s) then
      break
    end
    let tok = parser_current(s)
    let name = match tok.kind
      case TokenKind.Ident(payload: iv) -> iv.name
      case _ -> break
    end
    let (ns, _) = parser_advance(s)
    let (ns2, _) = parser_expect(ns, TokenKind.Colon, ":")?
    let (ns3, ty) = parse_type_expr(ns2)?
    let (ns4, default_val) = if parser_check(ns3, TokenKind.Assign) then
      let (ns5, _) = parser_advance(ns3)
      let (ns6, dv) = parse_expr(ns5, 0)?
      (ns6, dv)
    else
      (ns3, null)
    end
    params = params ++ [Param(name: name, ty: ty, default: default_val, span: tok.span)]
    s = ns4
    if parser_check(s, TokenKind.Comma) then
      let (ns5, _) = parser_advance(s)
      s = ns5
    end
  end
  Ok((s, params))
end

# Parse `import path.module: Name1, Name2 as Alias, *`
cell parse_import_item(p: Parser) -> result[(Parser, Item?), ParseError]
  let (p2, _) = parser_advance(p)  # consume 'import'
  # Parse dotted path: a.b.c
  let path_parts: list[String] = []
  let s = p2
  loop
    let tok = parser_current(s)
    match tok.kind
      case TokenKind.Ident(payload: iv) ->
        path_parts = path_parts ++ [iv.name]
        let (ns, _) = parser_advance(s)
        s = ns
        if parser_check(s, TokenKind.Dot) then
          let (ns2, _) = parser_advance(s)
          s = ns2
        else
          break
        end
      case _ -> break
    end
  end
  let path = string_join(path_parts, ".")
  # Optional `: names`
  let names: list[(String, String?)] = []
  if parser_check(s, TokenKind.Colon) then
    let (s2, _) = parser_advance(s)
    s = s2
    loop
      let tok = parser_current(s)
      match tok.kind
        case TokenKind.Star ->
          names = names ++ [("*", null)]
          let (ns, _) = parser_advance(s)
          s = ns
          break
        case TokenKind.Ident(payload: iv) ->
          let iname = iv.name
          let (ns, _) = parser_advance(s)
          s = ns
          let alias = if parser_check(s, TokenKind.KwAs) then
            let (ns2, _) = parser_advance(s)
            let at = parser_current(ns2)
            match at.kind
              case TokenKind.Ident(payload: av) ->
                let (ns3, _) = parser_advance(ns2)
                s = ns3
                av.name
              case _ -> null
            end
          else
            null
          end
          names = names ++ [(iname, alias)]
          if parser_check(s, TokenKind.Comma) then
            let (ns2, _) = parser_advance(s)
            s = ns2
          else
            break
          end
        case _ -> break
      end
    end
  end
  let def = ImportDef(path: path, names: names, span: p2.tokens[p2.pos - 1].span)
  Ok((s, Item.Import(payload: def)))
end

# Parse `type Name = TypeExpr`
cell parse_type_alias_item(p: Parser) -> result[(Parser, Item?), ParseError]
  let (p2, _) = parser_advance(p)  # consume 'type'
  let name_tok = parser_current(p2)
  let name = match name_tok.kind
    case TokenKind.Ident(payload: iv) -> iv.name
    case _ ->
      return Ok((synchronize_top(parser_error(p2, "expected type name", name_tok.span)), null))
  end
  let (p3, _) = parser_advance(p2)
  let (p4, _) = parser_expect(p3, TokenKind.Assign, "=")?
  let (p5, ty) = parse_type_expr(p4)?
  Ok((p5, Item.TypeAlias(payload: TypeAlias(name: name, ty: ty, span: name_tok.span))))
end

# Parse `const NAME = expr`
cell parse_const_item(p: Parser) -> result[(Parser, Item?), ParseError]
  let (p2, _) = parser_advance(p)  # consume 'const'
  let name_tok = parser_current(p2)
  let name = match name_tok.kind
    case TokenKind.Ident(payload: iv) -> iv.name
    case _ ->
      return Ok((synchronize_top(parser_error(p2, "expected const name", name_tok.span)), null))
  end
  let (p3, _) = parser_advance(p2)
  let (p4, _) = parser_expect(p3, TokenKind.Assign, "=")?
  let (p5, val) = parse_expr(p4, 0)?
  Ok((p5, Item.Const(payload: ConstDef(name: name, value: val, span: name_tok.span))))
end

# Parse a `pub` prefixed item
cell parse_pub_item(p: Parser) -> result[(Parser, Item?), ParseError]
  let (p2, _) = parser_advance(p)  # consume 'pub'
  let (p3, item) = parse_item(p2)?
  # Tag item as public if it was successfully parsed
  match item
    case null -> Ok((p3, null))
    case i    -> Ok((p3, item_set_pub(i, true)))
  end
end

cell item_set_pub(item: Item, pub: Bool) -> Item
  # In a full implementation, we'd have a pub field on each item record.
  # For now, just return the item unchanged — pub tracking is a resolver concern.
  item
end
```

## Statement Parsing

```lumen
# Parse a block of statements until `end` keyword.
cell parse_block(p: Parser) -> result[(Parser, Block), ParseError]
  let stmts: list[Stmt] = []
  let s = skip_newlines(p)
  loop
    let tok = parser_current(s)
    match tok.kind
      case TokenKind.KwEnd ->
        let (ns, end_tok) = parser_advance(s)
        let sp = match end_tok
          case null -> tok.span
          case t    -> t.span
        end
        return Ok((ns, Block(stmts: stmts, span: sp)))
      case TokenKind.Eof ->
        let np = parser_error(s, "expected 'end' to close block", tok.span)
        return Ok((np, Block(stmts: stmts, span: tok.span)))
      case TokenKind.Newline ->
        let (ns, _) = parser_advance_raw(s)
        s = ns
      case _ ->
        match parse_stmt(s)
          case Ok((ns, stmt)) ->
            match stmt
              case null -> null  # error recovery
              case st   -> stmts = stmts ++ [st]
            end
            s = ns
          case Err(e) ->
            let np = Parser(tokens: s.tokens, pos: s.pos, errors: s.errors ++ [e])
            s = synchronize_stmt(np)
        end
    end
  end
  Ok((s, Block(stmts: stmts, span: p.tokens[p.pos].span)))
end

# Parse one statement.
cell parse_stmt(p: Parser) -> result[(Parser, Stmt?), ParseError]
  let tok = parser_current(p)
  match tok.kind
    case TokenKind.KwLet    -> parse_let_stmt(p)
    case TokenKind.KwReturn -> parse_return_stmt(p)
    case TokenKind.KwFor    -> parse_for_stmt(p)
    case TokenKind.KwWhile  -> parse_while_stmt(p)
    case TokenKind.KwLoop   -> parse_loop_stmt(p)
    case TokenKind.KwBreak  -> parse_break_stmt(p)
    case TokenKind.KwContinue -> parse_continue_stmt(p)
    case TokenKind.KwMatch  -> parse_match_stmt(p)
    case _ -> parse_expr_or_assign_stmt(p)
  end
end

# Parse `let [mut] name [: Type] = expr`
cell parse_let_stmt(p: Parser) -> result[(Parser, Stmt?), ParseError]
  let (p2, kw) = parser_advance(p)  # consume 'let'
  let is_mut = parser_check(p2, TokenKind.KwMut)
  let p3 = if is_mut then
    let (ps, _) = parser_advance(p2)
    ps
  else
    p2
  end
  let name_tok = parser_current(p3)
  let name = match name_tok.kind
    case TokenKind.Ident(payload: iv) -> iv.name
    case _ ->
      let np = parser_error(p3, "expected variable name after 'let'", name_tok.span)
      return Ok((synchronize_stmt(np), null))
  end
  let (p4, _) = parser_advance(p3)
  # Optional type annotation
  let (p5, ann_ty) = if parser_check(p4, TokenKind.Colon) then
    let (ps, _) = parser_advance(p4)
    let (ps2, t) = parse_type_expr(ps)?
    (ps2, t)
  else
    (p4, null)
  end
  let (p6, _) = parser_expect(p5, TokenKind.Assign, "=")?
  let (p7, val) = parse_expr(p6, 0)?
  let sp = match kw
    case null -> name_tok.span
    case t    -> t.span
  end
  Ok((p7, Stmt.Let(payload: LetStmt(name: name, is_mut: is_mut, ty: ann_ty, value: val, span: sp))))
end

# Parse `return [expr]`
cell parse_return_stmt(p: Parser) -> result[(Parser, Stmt?), ParseError]
  let (p2, kw) = parser_advance(p)
  let kw_span = match kw
    case null -> p.tokens[p.pos].span
    case t    -> t.span
  end
  # Optional return value
  let tok = parser_current(p2)
  let (p3, val) = match tok.kind
    case TokenKind.Newline -> (p2, null)
    case TokenKind.KwEnd   -> (p2, null)
    case TokenKind.Eof     -> (p2, null)
    case _ ->
      let (ps, e) = parse_expr(p2, 0)?
      (ps, e)
  end
  Ok((p3, Stmt.Return(payload: ReturnStmt(value: val, span: kw_span))))
end

# Parse `for [pat in] name in expr ... end`
cell parse_for_stmt(p: Parser) -> result[(Parser, Stmt?), ParseError]
  let (p2, kw) = parser_advance(p)
  let kw_span = match kw
    case null -> p.tokens[p.pos].span
    case t    -> t.span
  end
  # Pattern before 'in'
  let (p3, pat) = parse_pattern(p2)?
  let (p4, _) = parser_expect(p3, TokenKind.KwIn, "in")?
  let (p5, iter) = parse_expr(p4, 0)?
  let (p6, body) = parse_block(p5)?
  Ok((p6, Stmt.For(payload: ForStmt(pattern: pat, iterable: iter, body: body, span: kw_span))))
end

# Parse `while cond ... end`
cell parse_while_stmt(p: Parser) -> result[(Parser, Stmt?), ParseError]
  let (p2, kw) = parser_advance(p)
  let kw_span = match kw
    case null -> p.tokens[p.pos].span
    case t    -> t.span
  end
  let (p3, cond) = parse_expr(p2, 0)?
  let (p4, body) = parse_block(p3)?
  Ok((p4, Stmt.While(payload: WhileStmt(condition: cond, body: body, span: kw_span))))
end

# Parse `loop ... end`
cell parse_loop_stmt(p: Parser) -> result[(Parser, Stmt?), ParseError]
  let (p2, kw) = parser_advance(p)
  let kw_span = match kw
    case null -> p.tokens[p.pos].span
    case t    -> t.span
  end
  let (p3, body) = parse_block(p2)?
  Ok((p3, Stmt.Loop(payload: LoopStmt(body: body, span: kw_span))))
end

# Parse `break [value]`
cell parse_break_stmt(p: Parser) -> result[(Parser, Stmt?), ParseError]
  let (p2, kw) = parser_advance(p)
  let sp = match kw
    case null -> p.tokens[p.pos].span
    case t    -> t.span
  end
  Ok((p2, Stmt.Break(payload: BreakStmt(span: sp))))
end

# Parse `continue`
cell parse_continue_stmt(p: Parser) -> result[(Parser, Stmt?), ParseError]
  let (p2, kw) = parser_advance(p)
  let sp = match kw
    case null -> p.tokens[p.pos].span
    case t    -> t.span
  end
  Ok((p2, Stmt.Continue(payload: ContinueStmt(span: sp))))
end

# Parse `match expr ... case Pattern -> body ... end`
cell parse_match_stmt(p: Parser) -> result[(Parser, Stmt?), ParseError]
  let (p2, kw) = parser_advance(p)
  let kw_span = match kw
    case null -> p.tokens[p.pos].span
    case t    -> t.span
  end
  let (p3, subject) = parse_expr(p2, 0)?
  let s = skip_newlines(p3)
  let arms: list[MatchArm] = []
  loop
    let tok = parser_current(s)
    match tok.kind
      case TokenKind.KwEnd ->
        let (ns, _) = parser_advance(s)
        s = ns
        break
      case TokenKind.Eof -> break
      case TokenKind.Newline ->
        let (ns, _) = parser_advance_raw(s)
        s = ns
      case TokenKind.KwCase ->
        let (ns, _) = parser_advance(s)
        s = ns
        let (ns2, pat) = parse_pattern(s)?
        s = ns2
        # Optional guard `if expr`
        let (ns3, guard) = if parser_check(s, TokenKind.KwIf) then
          let (ps, _) = parser_advance(s)
          let (ps2, g) = parse_expr(ps, 0)?
          (ps2, g)
        else
          (s, null)
        end
        s = ns3
        let (ns4, _) = parser_expect(s, TokenKind.Arrow, "->")?
        s = ns4
        # Arm body: single expr or block
        let (ns5, body) = parse_arm_body(s)?
        s = ns5
        arms = arms ++ [MatchArm(pattern: pat, guard: guard, body: body, span: tok.span)]
      case _ ->
        let (ns, _) = parser_advance(s)
        s = ns
    end
  end
  Ok((s, Stmt.Match(payload: MatchStmt(subject: subject, arms: arms, span: kw_span))))
end

# Parse the body of a match arm: either `-> expr` or a block.
cell parse_arm_body(p: Parser) -> result[(Parser, Expr), ParseError]
  let tok = parser_current(p)
  match tok.kind
    case TokenKind.Newline ->
      # Block arm: indented body until next case/end
      let s = skip_newlines(p)
      parse_expr(s, 0)
    case _ ->
      parse_expr(p, 0)
  end
end

# Parse an expression statement or assignment.
cell parse_expr_or_assign_stmt(p: Parser) -> result[(Parser, Stmt?), ParseError]
  let (p2, lhs) = parse_expr(p, 0)?
  # Check for assignment operators
  let tok = parser_current(p2)
  match tok.kind
    case TokenKind.Assign ->
      let (p3, _) = parser_advance(p2)
      let (p4, rhs) = parse_expr(p3, 0)?
      Ok((p4, Stmt.Assign(payload: AssignStmt(target: lhs, value: rhs, op: null, span: tok.span))))
    case TokenKind.PlusAssign ->
      let (p3, _) = parser_advance(p2)
      let (p4, rhs) = parse_expr(p3, 0)?
      Ok((p4, Stmt.Assign(payload: AssignStmt(target: lhs, value: rhs, op: BinOp.Add, span: tok.span))))
    case TokenKind.MinusAssign ->
      let (p3, _) = parser_advance(p2)
      let (p4, rhs) = parse_expr(p3, 0)?
      Ok((p4, Stmt.Assign(payload: AssignStmt(target: lhs, value: rhs, op: BinOp.Sub, span: tok.span))))
    case _ ->
      Ok((p2, Stmt.Expr(payload: ExprStmt(expr: lhs, span: lhs_span(lhs)))))
  end
end

cell lhs_span(e: Expr) -> Span
  # Helper to extract span from an expression.
  match e
    case Expr.Lit(payload: l)     -> l.span
    case Expr.Var(payload: v)     -> v.span
    case Expr.BinOp(payload: b)   -> b.span
    case Expr.UnaryOp(payload: u) -> u.span
    case Expr.Call(payload: c)    -> c.span
    case Expr.Field(payload: f)   -> f.span
    case Expr.Index(payload: i)   -> i.span
    case Expr.If(payload: i)      -> i.span
    case Expr.Match(payload: m)   -> m.span
    case Expr.Lambda(payload: l)  -> l.span
    case _                        -> make_span("", 0, 0, 0, 0)
  end
end
```

## Pattern Parsing

```lumen
# Parse a pattern.
cell parse_pattern(p: Parser) -> result[(Parser, Pattern), ParseError]
  parse_pattern_or(p)
end

# Parse `p1 | p2 | ...`
cell parse_pattern_or(p: Parser) -> result[(Parser, Pattern), ParseError]
  let (p2, first) = parse_pattern_primary(p)?
  let result_pat = first
  let s = p2
  loop
    let tok = parser_current(s)
    if tok.kind == TokenKind.Pipe then
      let (ns, _) = parser_advance(s)
      let (ns2, rhs) = parse_pattern_primary(ns)?
      result_pat = Pattern.Or(payload: OrPattern(patterns: [result_pat, rhs], span: tok.span))
      s = ns2
    else
      break
    end
  end
  Ok((s, result_pat))
end

cell parse_pattern_primary(p: Parser) -> result[(Parser, Pattern), ParseError]
  let tok = parser_current(p)
  match tok.kind
    # Wildcard: _
    case TokenKind.Ident(payload: iv) if iv.name == "_" ->
      let (p2, _) = parser_advance(p)
      Ok((p2, Pattern.Wildcard(payload: WildcardPattern(span: tok.span))))

    # Variable binding or enum variant
    case TokenKind.Ident(payload: iv) ->
      let (p2, _) = parser_advance(p)
      let name = iv.name
      # Check for qualified variant: Name.Variant or Name(fields)
      if parser_check(p2, TokenKind.Dot) then
        # Enum variant: Type.Variant
        let (p3, _) = parser_advance(p2)
        let vname_tok = parser_current(p3)
        match vname_tok.kind
          case TokenKind.Ident(payload: vv) ->
            let (p4, _) = parser_advance(p3)
            let (p5, fields) = if parser_check(p4, TokenKind.LParen) then
              let (ps, _) = parser_advance(p4)
              let (ps2, fs) = parse_pattern_fields(ps)?
              let (ps3, _) = parser_expect(ps2, TokenKind.RParen, ")")?
              (ps3, fs)
            else
              (p4, [])
            end
            Ok((p5, Pattern.Variant(payload: VariantPattern(
              type_name: name,
              variant_name: vv.name,
              fields: fields,
              span: tok.span
            ))))
          case _ ->
            Ok((p3, Pattern.Var(payload: VarPattern(name: name, span: tok.span))))
        end
      else
        if parser_check(p2, TokenKind.LParen) then
          # Record pattern or constructor
          let (p3, _) = parser_advance(p2)
          let (p4, fields) = parse_pattern_fields(p3)?
          let (p5, _) = parser_expect(p4, TokenKind.RParen, ")")?
          Ok((p5, Pattern.Record(payload: RecordPattern(
            name: name,
            fields: fields,
            span: tok.span
          ))))
        else
          Ok((p2, Pattern.Var(payload: VarPattern(name: name, span: tok.span))))
        end
      end

    # Literal patterns
    case TokenKind.IntLit(payload: v) ->
      let (p2, _) = parser_advance(p)
      Ok((p2, Pattern.Lit(payload: LitPattern(value: Expr.Int(v.value), span: tok.span))))
    case TokenKind.FloatLit(payload: v) ->
      let (p2, _) = parser_advance(p)
      Ok((p2, Pattern.Lit(payload: LitPattern(value: Expr.Float(v.value), span: tok.span))))
    case TokenKind.StringLit(payload: v) ->
      let (p2, _) = parser_advance(p)
      Ok((p2, Pattern.Lit(payload: LitPattern(value: Expr.Str(v.value), span: tok.span))))
    case TokenKind.BoolLit(payload: v) ->
      let (p2, _) = parser_advance(p)
      Ok((p2, Pattern.Lit(payload: LitPattern(value: Expr.Bool(v.value), span: tok.span))))
    case TokenKind.KwNull ->
      let (p2, _) = parser_advance(p)
      Ok((p2, Pattern.Lit(payload: LitPattern(value: Expr.Null, span: tok.span))))

    # Negated literal: -123
    case TokenKind.Minus ->
      let (p2, _) = parser_advance(p)
      let num_tok = parser_current(p2)
      match num_tok.kind
        case TokenKind.IntLit(payload: v) ->
          let (p3, _) = parser_advance(p2)
          Ok((p3, Pattern.Lit(payload: LitPattern(value: Expr.Int(0 - v.value), span: tok.span))))
        case _ ->
          Err(ParseError.UnexpectedToken(expected: "number after '-'", found: token_kind_name(num_tok.kind), span: num_tok.span))
      end

    # List pattern: [a, b, c]
    case TokenKind.LBracket ->
      let (p2, _) = parser_advance(p)
      let (p3, elems) = parse_pattern_list_elems(p2)?
      let (p4, close) = parser_expect(p3, TokenKind.RBracket, "]")?
      let sp = match close
        case null -> tok.span
        case t    -> span_union(tok.span, t.span)
      end
      Ok((p4, Pattern.List(payload: ListPattern(elements: elems, span: sp))))

    # Tuple pattern: (a, b)
    case TokenKind.LParen ->
      let (p2, _) = parser_advance(p)
      let (p3, elems) = parse_pattern_tuple_elems(p2)?
      let (p4, close) = parser_expect(p3, TokenKind.RParen, ")")?
      let sp = match close
        case null -> tok.span
        case t    -> span_union(tok.span, t.span)
      end
      if length(elems) == 1 then
        # Grouping, not a tuple
        Ok((p4, elems[0]))
      else
        Ok((p4, Pattern.Tuple(payload: TuplePattern(elements: elems, span: sp))))
      end

    case _ ->
      Err(ParseError.UnexpectedToken(
        expected: "pattern",
        found:    token_kind_name(tok.kind),
        span:     tok.span
      ))
  end
end

cell parse_pattern_fields(p: Parser) -> result[(Parser, list[(String, Pattern)]), ParseError]
  let fields: list[(String, Pattern)] = []
  let s = p
  loop
    if parser_check(s, TokenKind.RParen) or parser_at_end(s) then
      break
    end
    let tok = parser_current(s)
    match tok.kind
      case TokenKind.Ident(payload: iv) ->
        let fname = iv.name
        let (ns, _) = parser_advance(s)
        if parser_check(ns, TokenKind.Colon) then
          let (ns2, _) = parser_advance(ns)
          let (ns3, pat) = parse_pattern(ns2)?
          fields = fields ++ [(fname, pat)]
          s = ns3
        else
          # Shorthand: field name = binding
          fields = fields ++ [(fname, Pattern.Var(payload: VarPattern(name: fname, span: tok.span)))]
          s = ns
        end
      case _ -> break
    end
    if parser_check(s, TokenKind.Comma) then
      let (ns, _) = parser_advance(s)
      s = ns
    end
  end
  Ok((s, fields))
end

cell parse_pattern_list_elems(p: Parser) -> result[(Parser, list[Pattern]), ParseError]
  let elems: list[Pattern] = []
  let s = p
  loop
    if parser_check(s, TokenKind.RBracket) or parser_at_end(s) then
      break
    end
    let (ns, pat) = parse_pattern(s)?
    elems = elems ++ [pat]
    s = ns
    if parser_check(s, TokenKind.Comma) then
      let (ns2, _) = parser_advance(s)
      s = ns2
    else
      break
    end
  end
  Ok((s, elems))
end

cell parse_pattern_tuple_elems(p: Parser) -> result[(Parser, list[Pattern]), ParseError]
  let elems: list[Pattern] = []
  let s = p
  loop
    if parser_check(s, TokenKind.RParen) or parser_at_end(s) then
      break
    end
    let (ns, pat) = parse_pattern(s)?
    elems = elems ++ [pat]
    s = ns
    if parser_check(s, TokenKind.Comma) then
      let (ns2, _) = parser_advance(s)
      s = ns2
    else
      break
    end
  end
  Ok((s, elems))
end
```

## Expression Parsing — Pratt Parser

Operator precedences:

| Level | Operators |
|-------|-----------|
| 1 | `=` (assignment, handled separately) |
| 2 | `or` |
| 3 | `and` |
| 4 | `not` (prefix) |
| 5 | `==` `!=` `<` `<=` `>` `>=` `<=>` `is` |
| 6 | `in` `not in` |
| 7 | `..` `..=` |
| 8 | `\|` (bitwise or) |
| 9 | `^` (bitwise xor) |
| 10 | `&` (bitwise and) |
| 11 | `<<` `>>` |
| 12 | `++` (concat) |
| 13 | `+` `-` |
| 14 | `*` `/` `//` `%` |
| 15 | `**` (pow, right-assoc) |
| 16 | `\|>` (pipe, left-assoc) |
| 17 | Unary `-` `~` |
| 18 | Postfix: `.field` `[index]` `(call)` `?.` `?[` |

```lumen
# Return the infix binding power for an operator token, or null if not infix.
cell infix_bp(kind: TokenKind) -> (Int, Int)?
  match kind
    case TokenKind.KwOr              -> (2, 3)
    case TokenKind.KwAnd             -> (4, 5)
    case TokenKind.Eq                -> (6, 7)
    case TokenKind.NotEq             -> (6, 7)
    case TokenKind.Lt                -> (6, 7)
    case TokenKind.LtEq              -> (6, 7)
    case TokenKind.Gt                -> (6, 7)
    case TokenKind.GtEq              -> (6, 7)
    case TokenKind.Spaceship         -> (6, 7)
    case TokenKind.KwIn              -> (8, 9)
    case TokenKind.DotDot            -> (10, 11)
    case TokenKind.DotDotEq          -> (10, 11)
    case TokenKind.Pipe              -> (12, 13)
    case TokenKind.Caret             -> (14, 15)
    case TokenKind.Ampersand         -> (16, 17)
    case TokenKind.LeftShift         -> (18, 19)
    case TokenKind.RightShift        -> (18, 19)
    case TokenKind.PlusPlus          -> (20, 21)
    case TokenKind.Plus              -> (22, 23)
    case TokenKind.Minus             -> (22, 23)
    case TokenKind.Star              -> (24, 25)
    case TokenKind.Slash             -> (24, 25)
    case TokenKind.FloorDiv          -> (24, 25)
    case TokenKind.Percent           -> (24, 25)
    case TokenKind.StarStar          -> (27, 26)  # right-associative
    case TokenKind.PipeForward       -> (28, 29)
    case TokenKind.ComposeArrow      -> (30, 31)
    case TokenKind.QuestionQuestion  -> (32, 33)
    case _ -> null
  end
end

# Parse an expression with Pratt parser.
# min_bp: minimum binding power for infix operators to consume.
cell parse_expr(p: Parser, min_bp: Int) -> result[(Parser, Expr), ParseError]
  # Parse prefix / primary
  let (p2, lhs) = parse_prefix(p)?

  let s = p2
  loop
    let tok = parser_current(s)

    # Postfix operators (higher precedence than everything infix)
    match tok.kind
      case TokenKind.Dot ->
        if min_bp > 40 then break end
        let (ns, _) = parser_advance(s)
        let field_tok = parser_current(ns)
        match field_tok.kind
          case TokenKind.Ident(payload: iv) ->
            let (ns2, _) = parser_advance(ns)
            lhs = Expr.Field(payload: FieldAccess(object: lhs, field: iv.name, span: tok.span))
            s = ns2
          case _ ->
            let np = parser_error(ns, "expected field name after '.'", field_tok.span)
            return Ok((np, lhs))
        end
        continue
      case TokenKind.LBracket ->
        if min_bp > 40 then break end
        let (ns, _) = parser_advance(s)
        let (ns2, idx) = parse_expr(ns, 0)?
        let (ns3, _) = parser_expect(ns2, TokenKind.RBracket, "]")?
        lhs = Expr.Index(payload: IndexExpr(object: lhs, index: idx, span: tok.span))
        s = ns3
        continue
      case TokenKind.LParen ->
        if min_bp > 40 then break end
        let (ns, _) = parser_advance(s)
        let (ns2, args) = parse_call_args(ns)?
        let (ns3, close) = parser_expect(ns2, TokenKind.RParen, ")")?
        lhs = Expr.Call(payload: CallExpr(callee: lhs, args: args, span: tok.span))
        s = ns3
        continue
      case TokenKind.QuestionDot ->
        if min_bp > 40 then break end
        let (ns, _) = parser_advance(s)
        let field_tok = parser_current(ns)
        match field_tok.kind
          case TokenKind.Ident(payload: iv) ->
            let (ns2, _) = parser_advance(ns)
            lhs = Expr.OptionalChain(payload: OptionalChainExpr(object: lhs, field: iv.name, span: tok.span))
            s = ns2
          case _ -> break
        end
        continue
      case _ -> null
    end

    # Infix operators
    let bp = infix_bp(tok.kind)
    match bp
      case null -> break
      case (left_bp, right_bp) ->
        if left_bp < min_bp then
          break
        end
        let (ns, op_tok) = parser_advance(s)
        s = ns
        # Map token to BinOp
        let op = token_to_binop(tok.kind)
        let (ns2, rhs) = parse_expr(s, right_bp)?
        let sp = tok.span
        lhs = Expr.BinOp(payload: BinOpExpr(op: op, left: lhs, right: rhs, span: sp))
        s = ns2
    end
  end

  Ok((s, lhs))
end

cell token_to_binop(kind: TokenKind) -> BinOp
  match kind
    case TokenKind.Plus        -> BinOp.Add
    case TokenKind.Minus       -> BinOp.Sub
    case TokenKind.Star        -> BinOp.Mul
    case TokenKind.Slash       -> BinOp.Div
    case TokenKind.FloorDiv    -> BinOp.FloorDiv
    case TokenKind.Percent     -> BinOp.Mod
    case TokenKind.Eq          -> BinOp.OpEq
    case TokenKind.NotEq       -> BinOp.NotEq
    case TokenKind.Lt          -> BinOp.OpLt
    case TokenKind.LtEq        -> BinOp.LtEq
    case TokenKind.Gt          -> BinOp.OpGt
    case TokenKind.GtEq        -> BinOp.GtEq
    case TokenKind.KwAnd       -> BinOp.OpAnd
    case TokenKind.KwOr        -> BinOp.OpOr
    case TokenKind.StarStar    -> BinOp.Pow
    case TokenKind.PipeForward -> BinOp.PipeForward
    case TokenKind.PlusPlus    -> BinOp.Concat
    case TokenKind.KwIn        -> BinOp.OpIn
    case TokenKind.Ampersand   -> BinOp.BitAnd
    case TokenKind.Pipe        -> BinOp.BitOr
    case TokenKind.Caret       -> BinOp.BitXor
    case TokenKind.LeftShift   -> BinOp.Shl
    case TokenKind.RightShift  -> BinOp.Shr
    case TokenKind.Spaceship   -> BinOp.Spaceship
    case TokenKind.DotDot      -> BinOp.Range
    case TokenKind.DotDotEq    -> BinOp.RangeInclusive
    case _                     -> BinOp.Add  # fallback (shouldn't happen)
  end
end

# Parse prefix or primary expressions.
cell parse_prefix(p: Parser) -> result[(Parser, Expr), ParseError]
  let tok = parser_current(p)
  match tok.kind
    # Literals
    case TokenKind.IntLit(payload: v) ->
      let (p2, _) = parser_advance(p)
      Ok((p2, Expr.Lit(payload: LitExpr(value: LitValue.Int(v.value), span: tok.span))))
    case TokenKind.FloatLit(payload: v) ->
      let (p2, _) = parser_advance(p)
      Ok((p2, Expr.Lit(payload: LitExpr(value: LitValue.Float(v.value), span: tok.span))))
    case TokenKind.StringLit(payload: v) ->
      let (p2, _) = parser_advance(p)
      Ok((p2, Expr.Lit(payload: LitExpr(value: LitValue.Str(v.value), span: tok.span))))
    case TokenKind.StringInterpLit(payload: v) ->
      let (p2, _) = parser_advance(p)
      Ok((p2, Expr.StringInterp(payload: StringInterpExpr(segments: v.segments, span: tok.span))))
    case TokenKind.BoolLit(payload: v) ->
      let (p2, _) = parser_advance(p)
      Ok((p2, Expr.Lit(payload: LitExpr(value: LitValue.Bool(v.value), span: tok.span))))
    case TokenKind.KwNull ->
      let (p2, _) = parser_advance(p)
      Ok((p2, Expr.Lit(payload: LitExpr(value: LitValue.Null, span: tok.span))))

    # Identifier or keyword-as-value
    case TokenKind.Ident(payload: iv) ->
      let (p2, _) = parser_advance(p)
      Ok((p2, Expr.Var(payload: VarExpr(name: iv.name, span: tok.span))))

    # Unary operators
    case TokenKind.Minus ->
      let (p2, _) = parser_advance(p)
      let (p3, operand) = parse_expr(p2, 35)?  # high binding power
      Ok((p3, Expr.UnaryOp(payload: UnaryOpExpr(op: UnaryOp.Neg, operand: operand, span: tok.span))))
    case TokenKind.KwNot ->
      let (p2, _) = parser_advance(p)
      let (p3, operand) = parse_expr(p2, 3)?
      Ok((p3, Expr.UnaryOp(payload: UnaryOpExpr(op: UnaryOp.Not, operand: operand, span: tok.span))))
    case TokenKind.Tilde ->
      let (p2, _) = parser_advance(p)
      let (p3, operand) = parse_expr(p2, 35)?
      Ok((p3, Expr.UnaryOp(payload: UnaryOpExpr(op: UnaryOp.BitNot, operand: operand, span: tok.span))))
    case TokenKind.Bang ->
      let (p2, _) = parser_advance(p)
      let (p3, operand) = parse_expr(p2, 35)?
      Ok((p3, Expr.UnaryOp(payload: UnaryOpExpr(op: UnaryOp.Not, operand: operand, span: tok.span))))

    # Grouped expression or tuple literal
    case TokenKind.LParen ->
      let (p2, _) = parser_advance(p)
      let s = skip_newlines(p2)
      if parser_check(s, TokenKind.RParen) then
        # Empty tuple
        let (p3, _) = parser_advance(s)
        Ok((p3, Expr.Tuple(payload: TupleExpr(elements: [], span: tok.span))))
      else
        let (p3, first) = parse_expr(s, 0)?
        if parser_check(p3, TokenKind.Comma) then
          # Tuple literal
          let (p4, _) = parser_advance(p3)
          let elems = [first]
          let ps = p4
          loop
            if parser_check(ps, TokenKind.RParen) or parser_at_end(ps) then
              break
            end
            let (ns, e) = parse_expr(ps, 0)?
            elems = elems ++ [e]
            ps = ns
            if parser_check(ps, TokenKind.Comma) then
              let (ns2, _) = parser_advance(ps)
              ps = ns2
            else
              break
            end
          end
          let (p5, _) = parser_expect(ps, TokenKind.RParen, ")")?
          Ok((p5, Expr.Tuple(payload: TupleExpr(elements: elems, span: tok.span))))
        else
          let (p4, _) = parser_expect(p3, TokenKind.RParen, ")")?
          Ok((p4, first))
        end
      end

    # List literal: [e1, e2, ...]
    case TokenKind.LBracket ->
      let (p2, _) = parser_advance(p)
      let s = skip_newlines(p2)
      if parser_check(s, TokenKind.RBracket) then
        let (p3, _) = parser_advance(s)
        return Ok((p3, Expr.List(payload: ListExpr(elements: [], span: tok.span))))
      end
      let (p3, first) = parse_expr(s, 0)?
      # Check for comprehension: [expr for x in coll]
      if parser_check(p3, TokenKind.KwFor) then
        let (p4, _) = parser_advance(p3)
        let (p5, pat) = parse_pattern(p4)?
        let (p6, _) = parser_expect(p5, TokenKind.KwIn, "in")?
        let (p7, iter) = parse_expr(p6, 0)?
        let (p8, filter) = if parser_check(p7, TokenKind.KwIf) then
          let (ps, _) = parser_advance(p7)
          let (ps2, f) = parse_expr(ps, 0)?
          (ps2, f)
        else
          (p7, null)
        end
        let (p9, _) = parser_expect(p8, TokenKind.RBracket, "]")?
        Ok((p9, Expr.ListComp(payload: ListCompExpr(
          element: first,
          pattern: pat,
          iterable: iter,
          filter: filter,
          span: tok.span
        ))))
      else
        # Regular list
        let elems = [first]
        let ps = p3
        loop
          if parser_check(ps, TokenKind.Comma) then
            let (ns, _) = parser_advance(ps)
            ps = ns
          else
            break
          end
          if parser_check(ps, TokenKind.RBracket) or parser_at_end(ps) then
            break
          end
          let (ns2, e) = parse_expr(ps, 0)?
          elems = elems ++ [e]
          ps = ns2
        end
        let (pend, _) = parser_expect(ps, TokenKind.RBracket, "]")?
        Ok((pend, Expr.List(payload: ListExpr(elements: elems, span: tok.span))))
      end

    # Map or set literal: {k: v, ...} or {e, ...}
    case TokenKind.LBrace ->
      let (p2, _) = parser_advance(p)
      let s = skip_newlines(p2)
      if parser_check(s, TokenKind.RBrace) then
        let (p3, _) = parser_advance(s)
        return Ok((p3, Expr.Map(payload: MapExpr(entries: [], span: tok.span))))
      end
      # Peek to see if it's map (key: value) or set
      let (p3, first_e) = parse_expr(s, 0)?
      if parser_check(p3, TokenKind.Colon) then
        # Map literal
        let (p4, _) = parser_advance(p3)
        let (p5, first_v) = parse_expr(p4, 0)?
        let entries = [(first_e, first_v)]
        let ps = p5
        loop
          if parser_check(ps, TokenKind.Comma) then
            let (ns, _) = parser_advance(ps)
            ps = ns
          else
            break
          end
          if parser_check(ps, TokenKind.RBrace) or parser_at_end(ps) then
            break
          end
          let (nsk, k) = parse_expr(ps, 0)?
          let (nscolon, _) = parser_expect(nsk, TokenKind.Colon, ":")?
          let (nsv, v) = parse_expr(nscolon, 0)?
          entries = entries ++ [(k, v)]
          ps = nsv
        end
        let (pend, _) = parser_expect(ps, TokenKind.RBrace, "}")?
        Ok((pend, Expr.Map(payload: MapExpr(entries: entries, span: tok.span))))
      else
        # Set literal
        let elems = [first_e]
        let ps = p3
        loop
          if parser_check(ps, TokenKind.Comma) then
            let (ns, _) = parser_advance(ps)
            ps = ns
          else
            break
          end
          if parser_check(ps, TokenKind.RBrace) or parser_at_end(ps) then
            break
          end
          let (ns2, e) = parse_expr(ps, 0)?
          elems = elems ++ [e]
          ps = ns2
        end
        let (pend, _) = parser_expect(ps, TokenKind.RBrace, "}")?
        Ok((pend, Expr.Set(payload: SetExpr(elements: elems, span: tok.span))))
      end

    # if expression
    case TokenKind.KwIf ->
      let (p2, _) = parser_advance(p)
      let (p3, cond) = parse_expr(p2, 0)?
      let (p4, _) = parser_expect(p3, TokenKind.KwThen, "then")?
      let (p5, then_e) = parse_expr(p4, 0)?
      let (p6, else_e) = if parser_check(p5, TokenKind.KwElse) then
        let (ps, _) = parser_advance(p5)
        let (ps2, e) = parse_expr(ps, 0)?
        (ps2, e)
      else
        (p5, Expr.Lit(payload: LitExpr(value: LitValue.Null, span: tok.span)))
      end
      # Optional 'end'
      let p7 = if parser_check(p6, TokenKind.KwEnd) then
        let (ps, _) = parser_advance(p6)
        ps
      else
        p6
      end
      Ok((p7, Expr.If(payload: IfExpr(condition: cond, then_branch: then_e, else_branch: else_e, span: tok.span))))

    # match expression (inline form: match expr case X -> y end)
    case TokenKind.KwMatch ->
      let (p2, _) = parser_advance(p)
      let (p3, subject) = parse_expr(p2, 0)?
      let s = skip_newlines(p3)
      let arms: list[MatchArm] = []
      loop
        let t = parser_current(s)
        match t.kind
          case TokenKind.KwEnd ->
            let (ns, _) = parser_advance(s)
            s = ns
            break
          case TokenKind.Eof -> break
          case TokenKind.Newline ->
            let (ns, _) = parser_advance_raw(s)
            s = ns
          case TokenKind.KwCase ->
            let (ns, _) = parser_advance(s)
            let (ns2, pat) = parse_pattern(ns)?
            let (ns3, guard) = if parser_check(ns2, TokenKind.KwIf) then
              let (ps, _) = parser_advance(ns2)
              let (ps2, g) = parse_expr(ps, 0)?
              (ps2, g)
            else
              (ns2, null)
            end
            let (ns4, _) = parser_expect(ns3, TokenKind.Arrow, "->")?
            let (ns5, body) = parse_arm_body(ns4)?
            arms = arms ++ [MatchArm(pattern: pat, guard: guard, body: body, span: t.span)]
            s = ns5
          case _ ->
            let (ns, _) = parser_advance(s)
            s = ns
        end
      end
      Ok((s, Expr.Match(payload: MatchExpr(subject: subject, arms: arms, span: tok.span))))

    # lambda: fn(params) -> expr OR fn(params) ... end
    case TokenKind.KwFn ->
      let (p2, _) = parser_advance(p)
      let (p3, _) = parser_expect(p2, TokenKind.LParen, "(")?
      let (p4, params) = parse_params(p3)?
      let (p5, _) = parser_expect(p4, TokenKind.RParen, ")")?
      let (p6, ret_ty) = if parser_check(p5, TokenKind.Arrow) then
        let (ps, _) = parser_advance(p5)
        let (ps2, t) = parse_type_expr(ps)?
        (ps2, t)
      else
        (p5, null)
      end
      let tok2 = parser_current(p6)
      match tok2.kind
        case TokenKind.Newline ->
          # Block lambda
          let (p7, body) = parse_block(p6)?
          Ok((p7, Expr.Lambda(payload: LambdaExpr(params: params, ret_ty: ret_ty, body: Stmt.Block(body), span: tok.span))))
        case _ ->
          # Expression lambda
          let (p7, body_e) = parse_expr(p6, 0)?
          let (p8, _) = if parser_check(p7, TokenKind.KwEnd) then
            parser_advance(p7)
          else
            (p7, null)
          end
          Ok((p8, Expr.Lambda(payload: LambdaExpr(params: params, ret_ty: ret_ty, body: Stmt.Expr(payload: ExprStmt(expr: body_e, span: tok.span)), span: tok.span))))
      end

    case _ ->
      Err(ParseError.UnexpectedToken(
        expected: "expression",
        found:    token_kind_name(tok.kind),
        span:     tok.span
      ))
  end
end

# Parse call arguments: named args `name: val` or positional `val`.
cell parse_call_args(p: Parser) -> result[(Parser, list[CallArg]), ParseError]
  let args: list[CallArg] = []
  let s = skip_newlines(p)
  loop
    if parser_check(s, TokenKind.RParen) or parser_at_end(s) then
      break
    end
    # Check for named arg: ident: expr
    let tok = parser_current(s)
    let (ns, name, val) = match tok.kind
      case TokenKind.Ident(payload: iv) ->
        let (ns, _) = parser_advance(s)
        if parser_check(ns, TokenKind.Colon) then
          let (ns2, _) = parser_advance(ns)
          let (ns3, e) = parse_expr(ns2, 0)?
          (ns3, iv.name, e)
        else
          # Positional — we consumed the ident, treat it as a var expr
          let e = Expr.Var(payload: VarExpr(name: iv.name, span: tok.span))
          let (ns4, e2) = continue_expr(ns, e, 0)?
          (ns4, null, e2)
        end
      case _ ->
        let (ns, e) = parse_expr(s, 0)?
        (ns, null, e)
    end
    args = args ++ [CallArg(name: name, value: val)]
    s = ns
    if parser_check(s, TokenKind.Comma) then
      let (ns2, _) = parser_advance(s)
      s = ns2
    end
  end
  Ok((s, args))
end

# Continue an already-started expression through the infix/postfix loop.
cell continue_expr(p: Parser, lhs: Expr, min_bp: Int) -> result[(Parser, Expr), ParseError]
  let s = p
  let result_expr = lhs
  loop
    let tok = parser_current(s)
    # Postfix
    match tok.kind
      case TokenKind.Dot ->
        if min_bp > 40 then break end
        let (ns, _) = parser_advance(s)
        let ft = parser_current(ns)
        match ft.kind
          case TokenKind.Ident(payload: iv) ->
            let (ns2, _) = parser_advance(ns)
            result_expr = Expr.Field(payload: FieldAccess(object: result_expr, field: iv.name, span: tok.span))
            s = ns2
          case _ -> break
        end
        continue
      case TokenKind.LBracket ->
        if min_bp > 40 then break end
        let (ns, _) = parser_advance(s)
        let (ns2, idx) = parse_expr(ns, 0)?
        let (ns3, _) = parser_expect(ns2, TokenKind.RBracket, "]")?
        result_expr = Expr.Index(payload: IndexExpr(object: result_expr, index: idx, span: tok.span))
        s = ns3
        continue
      case TokenKind.LParen ->
        if min_bp > 40 then break end
        let (ns, _) = parser_advance(s)
        let (ns2, args) = parse_call_args(ns)?
        let (ns3, _) = parser_expect(ns2, TokenKind.RParen, ")")?
        result_expr = Expr.Call(payload: CallExpr(callee: result_expr, args: args, span: tok.span))
        s = ns3
        continue
      case _ -> null
    end
    # Infix
    let bp = infix_bp(tok.kind)
    match bp
      case null -> break
      case (left_bp, right_bp) ->
        if left_bp < min_bp then break end
        let (ns, _) = parser_advance(s)
        let op = token_to_binop(tok.kind)
        let (ns2, rhs) = parse_expr(ns, right_bp)?
        result_expr = Expr.BinOp(payload: BinOpExpr(op: op, left: result_expr, right: rhs, span: tok.span))
        s = ns2
    end
  end
  Ok((s, result_expr))
end
```

## Directive Parsing

```lumen
# Parse `@name [value]` directives at the top of a file.
cell parse_directives(p: Parser) -> (Parser, list[Directive])
  let dirs: list[Directive] = []
  let s = skip_newlines(p)
  loop
    let tok = parser_current(s)
    match tok.kind
      case TokenKind.Directive(payload: dv) ->
        let (ns, _) = parser_advance(s)
        # Optional string value on same line
        let vtok = parser_current_raw(ns)
        let (ns2, val) = match vtok.kind
          case TokenKind.StringLit(payload: sv) ->
            let (ns3, _) = parser_advance_raw(ns)
            (ns3, sv.value)
          case _ -> (ns, null)
        end
        dirs = dirs ++ [Directive(name: dv.name, value: val, span: tok.span)]
        s = skip_newlines(ns2)
      case _ -> break
    end
  end
  (s, dirs)
end
```

## Program Entry Point

```lumen
# Parse a complete Lumen program from a token list.
cell parse(tokens: list[Token]) -> result[Program, list[ParseError]]
  let p = new_parser(tokens)
  let (p2, directives) = parse_directives(p)
  let items: list[Item] = []
  let s = skip_newlines(p2)
  loop
    if parser_at_end(s) then
      break
    end
    let tok = parser_current(s)
    match tok.kind
      case TokenKind.Newline ->
        let (ns, _) = parser_advance_raw(s)
        s = ns
      case _ ->
        match parse_item(s)
          case Ok((ns, item)) ->
            match item
              case null -> null
              case i    -> items = items ++ [i]
            end
            s = ns
          case Err(e) ->
            s = Parser(tokens: s.tokens, pos: s.pos, errors: s.errors ++ [e])
            s = synchronize_top(s)
        end
    end
  end
  if length(s.errors) > 0 then
    Err(s.errors)
  else
    Ok(Program(
      directives: directives,
      items:      items,
      span:       make_span("", 0, 0, 1, 1)
    ))
  end
end
```

## Parser Tests

```lumen
import std.testing: assert_eq, assert_true, test
import self_host.lexer: lex

cell lex_and_parse(src: String) -> result[Program, list[ParseError]]
  match lex(src)
    case Err(e) -> Err([ParseError.General(message: format("{e}"), span: make_span("", 0, 0, 1, 1))])
    case Ok(toks) -> parse(toks)
  end
end

test "parse empty program" do
  match lex_and_parse("")
    case Ok(prog) -> assert_eq(length(prog.items), 0)
    case Err(e)   -> assert_true(false, "unexpected parse error")
  end
end

test "parse simple cell" do
  let src = """
cell main() -> Int
  42
end
"""
  match lex_and_parse(src)
    case Ok(prog) ->
      assert_eq(length(prog.items), 1)
      match prog.items[0]
        case Item.Cell(payload: cd) -> assert_eq(cd.name, "main")
        case _ -> assert_true(false, "expected Cell item")
      end
    case Err(errs) -> assert_true(false, "parse error: {length(errs)} errors")
  end
end

test "parse record declaration" do
  let src = """
record Point(
  x: Float,
  y: Float
)
"""
  match lex_and_parse(src)
    case Ok(prog) ->
      assert_eq(length(prog.items), 1)
      match prog.items[0]
        case Item.Record(payload: rd) ->
          assert_eq(rd.name, "Point")
          assert_eq(length(rd.fields), 2)
        case _ -> assert_true(false, "expected Record item")
      end
    case Err(errs) -> assert_true(false, "parse error")
  end
end

test "parse enum declaration" do
  let src = """
enum Color
  Red
  Green
  Blue
end
"""
  match lex_and_parse(src)
    case Ok(prog) ->
      assert_eq(length(prog.items), 1)
      match prog.items[0]
        case Item.Enum(payload: ed) ->
          assert_eq(ed.name, "Color")
          assert_eq(length(ed.variants), 3)
        case _ -> assert_true(false, "expected Enum item")
      end
    case Err(errs) -> assert_true(false, "parse error")
  end
end

test "parse binary expression" do
  let src = """
cell add(x: Int, y: Int) -> Int
  x + y
end
"""
  match lex_and_parse(src)
    case Ok(prog) ->
      assert_eq(length(prog.items), 1)
    case Err(errs) -> assert_true(false, "parse error")
  end
end

test "parse let statement" do
  let src = """
cell test() -> Int
  let x = 42
  x
end
"""
  match lex_and_parse(src)
    case Ok(prog) -> assert_eq(length(prog.items), 1)
    case Err(_)   -> assert_true(false, "parse error")
  end
end

test "parse import" do
  let src = "import std.math: sqrt, pow"
  match lex_and_parse(src)
    case Ok(prog) ->
      assert_eq(length(prog.items), 1)
      match prog.items[0]
        case Item.Import(payload: id) -> assert_eq(id.path, "std.math")
        case _ -> assert_true(false, "expected Import")
      end
    case Err(_) -> assert_true(false, "parse error")
  end
end
```
