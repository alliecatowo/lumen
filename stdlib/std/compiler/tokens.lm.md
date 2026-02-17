# std.compiler.tokens — Lexer Token Types

Complete token type definitions for the Lumen lexer, ported from
`rust/lumen-compiler/src/compiler/tokens.rs`.

```lumen
import std.compiler.span: Span

# ── Token kind ───────────────────────────────────────────────────
#
# Every token produced by the lexer carries one of these kinds.
# Payload-bearing variants use records (e.g. IntLitVal) because
# Lumen enum variants take at most one payload field.

# Payload wrappers for literal variants
record IntLitVal(value: Int)
record FloatLitVal(value: Float)
record StringLitVal(value: String)
record BoolLitVal(value: Bool)
record BytesLitVal(value: list[Int])
record IdentVal(name: String)
record DirectiveVal(name: String)
record SymbolVal(ch: String)

# String interpolation segment: is_expr=true means expression hole
record InterpSegment(
  is_expr: Bool,
  text: String,
  format_spec: String?
)
record StringInterpVal(segments: list[InterpSegment])

enum TokenKind
  # ── Literals ────────────────────────────────────────────────
  IntLit(payload: IntLitVal)
  FloatLit(payload: FloatLitVal)
  StringLit(payload: StringLitVal)
  StringInterpLit(payload: StringInterpVal)
  BoolLit(payload: BoolLitVal)
  RawStringLit(payload: StringLitVal)
  MarkdownBlock(payload: StringLitVal)
  BytesLit(payload: BytesLitVal)
  NullLit

  # ── Identifiers ────────────────────────────────────────────
  Ident(payload: IdentVal)

  # ── Keywords ───────────────────────────────────────────────
  KwRecord
  KwEnum
  KwCell
  KwLet
  KwIf
  KwElse
  KwFor
  KwIn
  KwMatch
  KwReturn
  KwHalt
  KwEnd
  KwUse
  KwTool
  KwAs
  KwGrant
  KwExpect
  KwSchema
  KwRole
  KwWhere
  KwAnd
  KwOr
  KwNot
  KwNull
  KwResult
  KwOk
  KwErr
  KwList
  KwMap
  KwWhile
  KwLoop
  KwBreak
  KwContinue
  KwMut
  KwConst
  KwPub
  KwImport
  KwFrom
  KwAsync
  KwAwait
  KwParallel
  KwFn
  KwTrait
  KwImpl
  KwType
  KwSet
  KwTuple
  KwEmit
  KwYield
  KwMod
  KwSelf
  KwWith
  KwTry
  KwUnion
  KwStep
  KwComptime
  KwMacro
  KwExtern
  KwThen
  KwWhen
  KwIs
  KwDefer
  KwPerform
  KwHandle
  KwResume
  # Type keywords
  KwBool
  KwInt
  KwFloat
  KwString
  KwBytes
  KwJson

  # ── Operators ──────────────────────────────────────────────
  Plus               # +
  Minus              # -
  Star               # *
  Slash              # /
  Percent            # %
  Eq                 # ==
  NotEq              # !=
  Lt                 # <
  LtEq               # <=
  Gt                 # >
  GtEq               # >=
  Assign             # =
  Arrow              # ->
  Dot                # .
  Comma              # ,
  Colon              # :
  Semicolon          # ;
  Pipe               # |
  At                 # @
  Hash               # #

  # Compound assignments
  PlusAssign         # +=
  MinusAssign        # -=
  StarAssign         # *=
  SlashAssign        # /=
  PercentAssign      # %=
  StarStarAssign     # **=
  AmpAssign          # &=
  PipeAssign         # |=
  CaretAssign        # ^=

  # Extended operators
  StarStar           # **
  DotDot             # ..
  DotDotEq           # ..=
  PipeForward        # |>
  ComposeArrow       # ~>
  LeftShift          # <<
  RightShift         # >>
  QuestionQuestion   # ??
  QuestionDot        # ?.
  Bang               # !
  Question           # ?
  DotDotDot          # ...
  FatArrow           # =>
  PlusPlus           # ++
  Ampersand          # &
  Tilde              # ~
  Caret              # ^
  FloorDiv           # //
  FloorDivAssign     # //=
  QuestionBracket    # ?[
  Spaceship          # <=>

  # ── Delimiters ─────────────────────────────────────────────
  Symbol(payload: SymbolVal)
  LParen             # (
  RParen             # )
  LBracket           # [
  RBracket           # ]
  LBrace             # {
  RBrace             # }

  # ── Indentation ────────────────────────────────────────────
  Indent
  Dedent
  Newline

  # ── Special ────────────────────────────────────────────────
  Eof

  # ── Directives ─────────────────────────────────────────────
  Directive(payload: DirectiveVal)
end

# ── Token record ─────────────────────────────────────────────────

record Token(
  kind: TokenKind,
  lexeme: String,
  span: Span
)

# ── Helper cells ─────────────────────────────────────────────────

# Create a token at a given span.
cell make_token(kind: TokenKind, lexeme: String, span: Span) -> Token
  return Token(kind: kind, lexeme: lexeme, span: span)
end

# Check if a token is a keyword.
cell is_keyword(tok: Token) -> Bool
  return match tok.kind
    case KwRecord -> true
    case KwEnum -> true
    case KwCell -> true
    case KwLet -> true
    case KwIf -> true
    case KwElse -> true
    case KwFor -> true
    case KwIn -> true
    case KwMatch -> true
    case KwReturn -> true
    case KwHalt -> true
    case KwEnd -> true
    case KwUse -> true
    case KwTool -> true
    case KwAs -> true
    case KwGrant -> true
    case KwExpect -> true
    case KwSchema -> true
    case KwRole -> true
    case KwWhere -> true
    case KwAnd -> true
    case KwOr -> true
    case KwNot -> true
    case KwNull -> true
    case KwResult -> true
    case KwOk -> true
    case KwErr -> true
    case KwList -> true
    case KwMap -> true
    case KwWhile -> true
    case KwLoop -> true
    case KwBreak -> true
    case KwContinue -> true
    case KwMut -> true
    case KwConst -> true
    case KwPub -> true
    case KwImport -> true
    case KwFrom -> true
    case KwAsync -> true
    case KwAwait -> true
    case KwParallel -> true
    case KwFn -> true
    case KwTrait -> true
    case KwImpl -> true
    case KwType -> true
    case KwSet -> true
    case KwTuple -> true
    case KwEmit -> true
    case KwYield -> true
    case KwMod -> true
    case KwSelf -> true
    case KwWith -> true
    case KwTry -> true
    case KwUnion -> true
    case KwStep -> true
    case KwComptime -> true
    case KwMacro -> true
    case KwExtern -> true
    case KwThen -> true
    case KwWhen -> true
    case KwIs -> true
    case KwDefer -> true
    case KwPerform -> true
    case KwHandle -> true
    case KwResume -> true
    case KwBool -> true
    case KwInt -> true
    case KwFloat -> true
    case KwString -> true
    case KwBytes -> true
    case KwJson -> true
    case _ -> false
  end
end

# Check if a token is a literal.
cell is_literal(tok: Token) -> Bool
  return match tok.kind
    case IntLit(_) -> true
    case FloatLit(_) -> true
    case StringLit(_) -> true
    case StringInterpLit(_) -> true
    case BoolLit(_) -> true
    case RawStringLit(_) -> true
    case BytesLit(_) -> true
    case NullLit -> true
    case _ -> false
  end
end

# Check if a token is an operator.
cell is_operator(tok: Token) -> Bool
  return match tok.kind
    case Plus -> true
    case Minus -> true
    case Star -> true
    case Slash -> true
    case Percent -> true
    case Eq -> true
    case NotEq -> true
    case Lt -> true
    case LtEq -> true
    case Gt -> true
    case GtEq -> true
    case StarStar -> true
    case PipeForward -> true
    case ComposeArrow -> true
    case PlusPlus -> true
    case Ampersand -> true
    case Tilde -> true
    case Caret -> true
    case FloorDiv -> true
    case LeftShift -> true
    case RightShift -> true
    case Spaceship -> true
    case _ -> false
  end
end
```
