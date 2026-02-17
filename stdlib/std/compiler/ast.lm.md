# std.compiler.ast — Abstract Syntax Tree

Complete AST type definitions for the Lumen compiler, ported from
`rust/lumen-compiler/src/compiler/ast.rs`.

```lumen
import std.compiler.span: Span

# ══════════════════════════════════════════════════════════════════
# Program — root AST node
# ══════════════════════════════════════════════════════════════════

record Program(
  directives: list[Directive],
  items: list[Item],
  span: Span
)

record Directive(
  name: String,
  value: String?,
  span: Span
)

# ══════════════════════════════════════════════════════════════════
# Type expressions
# ══════════════════════════════════════════════════════════════════

enum TypeExpr
  # Named type: String, Int, user-defined
  Named(payload: NamedTypeExpr)
  # list[T]
  ListType(payload: ListTypeExpr)
  # map[K, V]
  MapType(payload: MapTypeExpr)
  # result[Ok, Err]
  ResultType(payload: ResultTypeExpr)
  # A | B | C
  UnionType(payload: UnionTypeExpr)
  # Null type
  NullType(payload: NullTypeExpr)
  # (A, B, C)
  TupleType(payload: TupleTypeExpr)
  # set[T]
  SetType(payload: SetTypeExpr)
  # fn(A, B) -> C / {effects}
  FnType(payload: FnTypeExpr)
  # Name[T, U]
  GenericType(payload: GenericTypeExpr)
end

record NamedTypeExpr(name: String, span: Span)
record ListTypeExpr(element: TypeExpr, span: Span)
record MapTypeExpr(key: TypeExpr, value: TypeExpr, span: Span)
record ResultTypeExpr(ok: TypeExpr, err: TypeExpr, span: Span)
record UnionTypeExpr(members: list[TypeExpr], span: Span)
record NullTypeExpr(span: Span)
record TupleTypeExpr(elements: list[TypeExpr], span: Span)
record SetTypeExpr(element: TypeExpr, span: Span)
record FnTypeExpr(params: list[TypeExpr], ret: TypeExpr, effects: list[String], span: Span)
record GenericTypeExpr(name: String, args: list[TypeExpr], span: Span)

# ══════════════════════════════════════════════════════════════════
# Generic parameters
# ══════════════════════════════════════════════════════════════════

record GenericParam(
  name: String,
  bounds: list[String],
  span: Span
)

# ══════════════════════════════════════════════════════════════════
# Operators
# ══════════════════════════════════════════════════════════════════

enum BinOp
  Add         # +
  Sub         # -
  Mul         # *
  Div         # /
  FloorDiv    # //
  Mod         # %
  OpEq        # ==
  NotEq       # !=
  OpLt        # <
  LtEq        # <=
  OpGt        # >
  GtEq        # >=
  OpAnd       # and
  OpOr        # or
  Pow         # **
  PipeForward # |>
  Concat      # ++
  OpIn        # in
  BitAnd      # &
  BitOr       # |
  BitXor      # ^
  Shl         # <<
  Shr         # >>
  Compose     # ~>
  Spaceship   # <=>
end

enum UnaryOp
  Neg     # -
  OpNot   # not
  BitNot  # ~
end

enum CompoundOp
  AddAssign       # +=
  SubAssign       # -=
  MulAssign       # *=
  DivAssign       # /=
  FloorDivAssign  # //=
  ModAssign       # %=
  PowAssign       # **=
  BitAndAssign    # &=
  BitOrAssign     # |=
  BitXorAssign    # ^=
end

# ══════════════════════════════════════════════════════════════════
# Item definitions (top-level declarations)
# ══════════════════════════════════════════════════════════════════

enum Item
  RecordItem(payload: RecordDef)
  EnumItem(payload: EnumDef)
  CellItem(payload: CellDef)
  AgentItem(payload: AgentDecl)
  ProcessItem(payload: ProcessDecl)
  EffectItem(payload: EffectDecl)
  EffectBindItem(payload: EffectBindDecl)
  HandlerItem(payload: HandlerDecl)
  AddonItem(payload: AddonDecl)
  UseToolItem(payload: UseToolDecl)
  GrantItem(payload: GrantDecl)
  TypeAliasItem(payload: TypeAliasDef)
  TraitItem(payload: TraitDef)
  ImplItem(payload: ImplDef)
  ImportItem(payload: ImportDecl)
  ConstDeclItem(payload: ConstDeclDef)
  MacroDeclItem(payload: MacroDeclDef)
end

# ── Records ──────────────────────────────────────────────────────

record RecordDef(
  name: String,
  generic_params: list[GenericParam],
  fields: list[FieldDef],
  is_pub: Bool,
  span: Span,
  doc: String?
)

record FieldDef(
  name: String,
  ty: TypeExpr,
  default_value: Expr?,
  constraint: Expr?,
  span: Span
)

# ── Enums ────────────────────────────────────────────────────────

record EnumDef(
  name: String,
  generic_params: list[GenericParam],
  variants: list[EnumVariant],
  methods: list[CellDef],
  is_pub: Bool,
  span: Span,
  doc: String?
)

record EnumVariant(
  name: String,
  payload: TypeExpr?,
  span: Span
)

# ── Cells (functions) ────────────────────────────────────────────

record CellDef(
  name: String,
  generic_params: list[GenericParam],
  params: list[Param],
  return_type: TypeExpr?,
  effects: list[String],
  body: list[Stmt],
  is_pub: Bool,
  is_async: Bool,
  is_extern: Bool,
  must_use: Bool,
  where_clauses: list[Expr],
  span: Span,
  doc: String?
)

record Param(
  name: String,
  ty: TypeExpr,
  default_value: Expr?,
  variadic: Bool,
  span: Span
)

# ── Agents ───────────────────────────────────────────────────────

record AgentDecl(
  name: String,
  cells: list[CellDef],
  grants: list[GrantDecl],
  span: Span
)

# ── Processes ────────────────────────────────────────────────────

record ProcessDecl(
  kind: String,
  name: String,
  configs: map[String, Expr],
  cells: list[CellDef],
  grants: list[GrantDecl],
  pipeline_stages: list[String],
  machine_initial: String?,
  machine_states: list[MachineStateDecl],
  span: Span
)

record MachineStateDecl(
  name: String,
  params: list[Param],
  terminal: Bool,
  guard: Expr?,
  transition_to: String?,
  transition_args: list[Expr],
  span: Span
)

# ── Effects ──────────────────────────────────────────────────────

record EffectDecl(
  name: String,
  operations: list[CellDef],
  span: Span
)

record EffectBindDecl(
  effect_path: String,
  tool_alias: String,
  span: Span
)

record HandlerDecl(
  name: String,
  handles: list[CellDef],
  span: Span,
  doc: String?
)

record EffectHandler(
  effect_name: String,
  operation: String,
  params: list[Param],
  body: list[Stmt],
  span: Span
)

# ── Addons ───────────────────────────────────────────────────────

record AddonDecl(
  kind: String,
  name: String?,
  span: Span
)

# ── Tools and Grants ─────────────────────────────────────────────

record UseToolDecl(
  tool_path: String,
  alias: String,
  mcp_url: String?,
  span: Span
)

record GrantDecl(
  tool_alias: String,
  constraints: list[GrantConstraint],
  span: Span
)

record GrantConstraint(
  key: String,
  value: Expr,
  span: Span
)

# ── Type aliases, traits, impls ──────────────────────────────────

record TypeAliasDef(
  name: String,
  generic_params: list[GenericParam],
  type_expr: TypeExpr,
  is_pub: Bool,
  span: Span,
  doc: String?
)

record TraitDef(
  name: String,
  parent_traits: list[String],
  methods: list[CellDef],
  is_pub: Bool,
  span: Span
)

record ImplDef(
  trait_name: String,
  generic_params: list[GenericParam],
  target_type: String,
  cells: list[CellDef],
  span: Span
)

# ── Imports ──────────────────────────────────────────────────────

enum ImportList
  ImportNames(payload: ImportNamesVal)
  ImportWildcard
end

record ImportNamesVal(names: list[ImportName])

record ImportName(
  name: String,
  alias: String?,
  span: Span
)

record ImportDecl(
  path: list[String],
  names: ImportList,
  is_pub: Bool,
  span: Span
)

# ── Constants and Macros ─────────────────────────────────────────

record ConstDeclDef(
  name: String,
  type_ann: TypeExpr?,
  value: Expr,
  span: Span
)

record MacroDeclDef(
  name: String,
  params: list[String],
  body: list[Stmt],
  span: Span
)

# ══════════════════════════════════════════════════════════════════
# Statements
# ══════════════════════════════════════════════════════════════════

enum Stmt
  LetStmt(payload: LetStmtDef)
  IfStmt(payload: IfStmtDef)
  ForStmt(payload: ForStmtDef)
  MatchStmt(payload: MatchStmtDef)
  ReturnStmt(payload: ReturnStmtDef)
  HaltStmt(payload: HaltStmtDef)
  AssignStmt(payload: AssignStmtDef)
  ExprStmt(payload: ExprStmtDef)
  WhileStmt(payload: WhileStmtDef)
  LoopStmt(payload: LoopStmtDef)
  BreakStmt(payload: BreakStmtDef)
  ContinueStmt(payload: ContinueStmtDef)
  EmitStmt(payload: EmitStmtDef)
  CompoundAssignStmt(payload: CompoundAssignStmtDef)
  DeferStmt(payload: DeferStmtDef)
  YieldStmt(payload: YieldStmtDef)
  LocalRecord(payload: RecordDef)
  LocalEnum(payload: EnumDef)
  LocalCell(payload: CellDef)
end

record LetStmtDef(
  name: String,
  mutable: Bool,
  pattern: Pattern?,
  ty: TypeExpr?,
  value: Expr,
  span: Span
)

record IfStmtDef(
  condition: Expr,
  then_body: list[Stmt],
  else_body: list[Stmt]?,
  span: Span
)

record ForStmtDef(
  label: String?,
  var: String,
  pattern: Pattern?,
  iter: Expr,
  filter: Expr?,
  body: list[Stmt],
  span: Span
)

record MatchStmtDef(
  subject: Expr,
  arms: list[MatchArm],
  span: Span
)

record MatchArm(
  pattern: Pattern,
  body: list[Stmt],
  span: Span
)

record ReturnStmtDef(
  value: Expr,
  span: Span
)

record HaltStmtDef(
  message: Expr,
  span: Span
)

record ExprStmtDef(
  expr: Expr,
  span: Span
)

record AssignStmtDef(
  target: String,
  value: Expr,
  span: Span
)

record WhileStmtDef(
  label: String?,
  condition: Expr,
  body: list[Stmt],
  span: Span
)

record LoopStmtDef(
  label: String?,
  body: list[Stmt],
  span: Span
)

record BreakStmtDef(
  label: String?,
  value: Expr?,
  span: Span
)

record ContinueStmtDef(
  label: String?,
  span: Span
)

record EmitStmtDef(
  value: Expr,
  span: Span
)

record CompoundAssignStmtDef(
  target: String,
  op: CompoundOp,
  value: Expr,
  span: Span
)

record DeferStmtDef(
  body: list[Stmt],
  span: Span
)

record YieldStmtDef(
  value: Expr,
  span: Span
)

# ══════════════════════════════════════════════════════════════════
# Patterns
# ══════════════════════════════════════════════════════════════════

enum Pattern
  # Literal: 200, "hello", true
  LiteralPat(payload: LiteralPatDef)
  # Variant: ok(value), err(e)
  VariantPat(payload: VariantPatDef)
  # Wildcard: _
  WildcardPat(payload: WildcardPatDef)
  # Ident binding
  IdentPat(payload: IdentPatDef)
  # Guard: pattern if condition
  GuardPat(payload: GuardPatDef)
  # Or: pattern1 | pattern2
  OrPat(payload: OrPatDef)
  # List destructure: [a, b, ...rest]
  ListDestructure(payload: ListDestructureDef)
  # Tuple destructure: (a, b, c)
  TupleDestructure(payload: TupleDestructureDef)
  # Record destructure: TypeName(field1:, field2: pat)
  RecordDestructure(payload: RecordDestructureDef)
  # Type check: name: Type
  TypeCheckPat(payload: TypeCheckPatDef)
  # Range: 1..10 or 1..=10
  RangePat(payload: RangePatDef)
end

record LiteralPatDef(expr: Expr)
record VariantPatDef(name: String, inner: Pattern?, span: Span)
record WildcardPatDef(span: Span)
record IdentPatDef(name: String, span: Span)
record GuardPatDef(inner: Pattern, condition: Expr, span: Span)
record OrPatDef(patterns: list[Pattern], span: Span)
record ListDestructureDef(elements: list[Pattern], rest: String?, span: Span)
record TupleDestructureDef(elements: list[Pattern], span: Span)
record RecordDestructureField(name: String, pattern: Pattern?)
record RecordDestructureDef(type_name: String, fields: list[RecordDestructureField], open: Bool, span: Span)
record TypeCheckPatDef(name: String, type_expr: TypeExpr, span: Span)
record RangePatDef(start: Expr, end_val: Expr, inclusive: Bool, span: Span)

# ══════════════════════════════════════════════════════════════════
# Expressions
# ══════════════════════════════════════════════════════════════════

# String interpolation segment
enum StringSegment
  LiteralSeg(payload: StringLitVal)
  InterpSeg(payload: InterpExprVal)
  FormattedInterpSeg(payload: FormattedInterpVal)
end

record StringLitVal(text: String)
record InterpExprVal(expr: Expr)
record FormattedInterpVal(expr: Expr, spec: FormatSpec)

# Format spec components
enum FormatAlign
  AlignLeft
  AlignRight
  AlignCenter
end

enum FormatType
  FmtDecimal
  FmtHex
  FmtHexUpper
  FmtOctal
  FmtBinary
  FmtFixed
  FmtScientific
  FmtScientificUpper
  FmtStr
end

record FormatSpec(
  fill: String?,
  align: FormatAlign?,
  sign: String?,
  alternate: Bool,
  zero_pad: Bool,
  width: Int?,
  precision: Int?,
  fmt_type: FormatType?,
  raw: String
)

# Call argument kinds
enum CallArg
  Positional(payload: PositionalArgDef)
  NamedArg(payload: NamedArgDef)
  RoleArg(payload: RoleArgDef)
end

record PositionalArgDef(expr: Expr)
record NamedArgDef(name: String, expr: Expr, span: Span)
record RoleArgDef(role: String, expr: Expr, span: Span)

# When-expression arm
record WhenArm(
  condition: Expr,
  body: Expr,
  span: Span
)

# Comprehension clause
record ComprehensionClause(
  var: String,
  iter: Expr
)

enum ComprehensionKind
  ListComp
  MapComp
  SetComp
end

# Lambda body variants
enum LambdaBody
  LambdaExpr(payload: LambdaExprBody)
  LambdaBlock(payload: LambdaBlockBody)
end

record LambdaExprBody(expr: Expr)
record LambdaBlockBody(stmts: list[Stmt])

# ── The Expr enum ────────────────────────────────────────────────

enum Expr
  # Literals
  IntLitExpr(payload: IntLitExprDef)
  BigIntLitExpr(payload: BigIntLitExprDef)
  FloatLitExpr(payload: FloatLitExprDef)
  StringLitExpr(payload: StringLitExprDef)
  StringInterpExpr(payload: StringInterpExprDef)
  BoolLitExpr(payload: BoolLitExprDef)
  NullLitExpr(payload: NullLitExprDef)
  RawStringLitExpr(payload: RawStringLitExprDef)
  BytesLitExpr(payload: BytesLitExprDef)

  # References
  IdentExpr(payload: IdentExprDef)

  # Collections
  ListLitExpr(payload: ListLitExprDef)
  MapLitExpr(payload: MapLitExprDef)
  RecordLitExpr(payload: RecordLitExprDef)
  TupleLitExpr(payload: TupleLitExprDef)
  SetLitExpr(payload: SetLitExprDef)

  # Operations
  BinOpExpr(payload: BinOpExprDef)
  UnaryOpExpr(payload: UnaryOpExprDef)

  # Calls
  CallExpr(payload: CallExprDef)
  ToolCallExpr(payload: ToolCallExprDef)

  # Access
  DotAccessExpr(payload: DotAccessExprDef)
  IndexAccessExpr(payload: IndexAccessExprDef)

  # AI-specific
  RoleBlockExpr(payload: RoleBlockExprDef)
  ExpectSchemaExpr(payload: ExpectSchemaExprDef)

  # Lambda
  LambdaExpr(payload: LambdaExprDef)

  # Range
  RangeExpr(payload: RangeExprDef)

  # Error handling
  TryExpr(payload: TryExprDef)
  TryElseExpr(payload: TryElseExprDef)

  # Null handling
  NullCoalesceExpr(payload: NullCoalesceExprDef)
  NullSafeAccessExpr(payload: NullSafeAccessExprDef)
  NullSafeIndexExpr(payload: NullSafeIndexExprDef)
  NullAssertExpr(payload: NullAssertExprDef)

  # Spread
  SpreadExpr(payload: SpreadExprDef)

  # Control flow expressions
  IfExpr(payload: IfExprDef)
  MatchExpr(payload: MatchExprDef)
  WhenExpr(payload: WhenExprDef)
  BlockExpr(payload: BlockExprDef)

  # Async
  AwaitExpr(payload: AwaitExprDef)

  # Comprehension
  ComprehensionExpr(payload: ComprehensionExprDef)

  # Pipe
  PipeExpr(payload: PipeExprDef)

  # Type operations
  IsTypeExpr(payload: IsTypeExprDef)
  TypeCastExpr(payload: TypeCastExprDef)

  # Compile-time
  ComptimeExpr(payload: ComptimeExprDef)

  # Effect operations
  PerformExpr(payload: PerformExprDef)
  HandleExpr(payload: HandleExprDef)
  ResumeExpr(payload: ResumeExprDef)
end

# ── Expr payload records ─────────────────────────────────────────

record IntLitExprDef(value: Int, span: Span)
record BigIntLitExprDef(value: String, span: Span)
record FloatLitExprDef(value: Float, span: Span)
record StringLitExprDef(value: String, span: Span)
record StringInterpExprDef(segments: list[StringSegment], span: Span)
record BoolLitExprDef(value: Bool, span: Span)
record NullLitExprDef(span: Span)
record RawStringLitExprDef(value: String, span: Span)
record BytesLitExprDef(value: list[Int], span: Span)

record IdentExprDef(name: String, span: Span)

record ListLitExprDef(elements: list[Expr], span: Span)
record MapLitExprDef(entries: list[MapEntry], span: Span)
record MapEntry(key: Expr, value: Expr)
record RecordLitExprDef(name: String, fields: list[RecordFieldInit], span: Span)
record RecordFieldInit(name: String, value: Expr)
record TupleLitExprDef(elements: list[Expr], span: Span)
record SetLitExprDef(elements: list[Expr], span: Span)

record BinOpExprDef(left: Expr, op: BinOp, right: Expr, span: Span)
record UnaryOpExprDef(op: UnaryOp, operand: Expr, span: Span)

record CallExprDef(callee: Expr, args: list[CallArg], span: Span)
record ToolCallExprDef(callee: Expr, args: list[CallArg], span: Span)

record DotAccessExprDef(object: Expr, field: String, span: Span)
record IndexAccessExprDef(object: Expr, index: Expr, span: Span)

record RoleBlockExprDef(role: String, body: Expr, span: Span)
record ExpectSchemaExprDef(expr: Expr, schema_name: String, span: Span)

record LambdaExprDef(params: list[Param], return_type: TypeExpr?, body: LambdaBody, span: Span)

record RangeExprDef(start: Expr?, end_val: Expr?, inclusive: Bool, step: Expr?, span: Span)

record TryExprDef(expr: Expr, span: Span)
record TryElseExprDef(expr: Expr, error_binding: String, handler: Expr, span: Span)

record NullCoalesceExprDef(lhs: Expr, rhs: Expr, span: Span)
record NullSafeAccessExprDef(object: Expr, field: String, span: Span)
record NullSafeIndexExprDef(object: Expr, index: Expr, span: Span)
record NullAssertExprDef(expr: Expr, span: Span)

record SpreadExprDef(expr: Expr, span: Span)

record IfExprDef(cond: Expr, then_val: Expr, else_val: Expr, span: Span)
record MatchExprDef(subject: Expr, arms: list[MatchArm], span: Span)
record WhenExprDef(arms: list[WhenArm], else_body: Expr?, span: Span)
record BlockExprDef(stmts: list[Stmt], span: Span)

record AwaitExprDef(expr: Expr, span: Span)

record ComprehensionExprDef(
  body: Expr,
  var: String,
  iter: Expr,
  extra_clauses: list[ComprehensionClause],
  condition: Expr?,
  kind: ComprehensionKind,
  span: Span
)

record PipeExprDef(left: Expr, right: Expr, span: Span)

record IsTypeExprDef(expr: Expr, type_name: String, span: Span)
record TypeCastExprDef(expr: Expr, target_type: String, span: Span)

record ComptimeExprDef(expr: Expr, span: Span)

record PerformExprDef(effect_name: String, operation: String, args: list[Expr], span: Span)
record HandleExprDef(body: list[Stmt], handlers: list[EffectHandler], span: Span)
record ResumeExprDef(value: Expr, span: Span)
```
