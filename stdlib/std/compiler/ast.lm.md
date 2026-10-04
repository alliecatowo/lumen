# std.compiler.ast — Abstract Syntax Tree

Complete AST type definitions for the Lumen compiler, ported from
`rust/lumen-compiler/src/compiler/ast.rs`.

```lumen
import std.compiler.span: Span

# ══════════════════════════════════════════════════════════════════
# Program — root AST node
# ══════════════════════════════════════════════════════════════════

pub record Program(
  directives: list[Directive],
  items: list[Item],
  span: Span
)

pub record Directive(
  name: String,
  value: String?,
  span: Span
)

# ══════════════════════════════════════════════════════════════════
# Type expressions
# ══════════════════════════════════════════════════════════════════

pub enum TypeExpr
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

pub record NamedTypeExpr(name: String, span: Span)
pub record ListTypeExpr(element: TypeExpr, span: Span)
pub record MapTypeExpr(key: TypeExpr, value: TypeExpr, span: Span)
pub record ResultTypeExpr(ok: TypeExpr, err: TypeExpr, span: Span)
pub record UnionTypeExpr(members: list[TypeExpr], span: Span)
pub record NullTypeExpr(span: Span)
pub record TupleTypeExpr(elements: list[TypeExpr], span: Span)
pub record SetTypeExpr(element: TypeExpr, span: Span)
pub record FnTypeExpr(params: list[TypeExpr], ret: TypeExpr, effects: list[String], span: Span)
pub record GenericTypeExpr(name: String, args: list[TypeExpr], span: Span)

# ══════════════════════════════════════════════════════════════════
# Generic parameters
# ══════════════════════════════════════════════════════════════════

pub record GenericParam(
  name: String,
  bounds: list[String],
  span: Span
)

# ══════════════════════════════════════════════════════════════════
# Operators
# ══════════════════════════════════════════════════════════════════

pub enum BinOp
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

pub enum UnaryOp
  Neg     # -
  OpNot   # not
  BitNot  # ~
end

pub enum CompoundOp
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

pub enum Item
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

pub record RecordDef(
  name: String,
  generic_params: list[GenericParam],
  fields: list[FieldDef],
  is_pub: Bool,
  span: Span,
  doc: String?
)

pub record FieldDef(
  name: String,
  ty: TypeExpr,
  default_value: Expr?,
  constraint: Expr?,
  span: Span
)

# ── Enums ────────────────────────────────────────────────────────

pub record EnumDef(
  name: String,
  generic_params: list[GenericParam],
  variants: list[EnumVariant],
  methods: list[CellDef],
  is_pub: Bool,
  span: Span,
  doc: String?
)

pub record EnumVariant(
  name: String,
  payload: TypeExpr?,
  span: Span
)

# ── Cells (functions) ────────────────────────────────────────────

pub record CellDef(
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

pub record Param(
  name: String,
  ty: TypeExpr,
  default_value: Expr?,
  variadic: Bool,
  span: Span
)

# ── Agents ───────────────────────────────────────────────────────

pub record AgentDecl(
  name: String,
  cells: list[CellDef],
  grants: list[GrantDecl],
  span: Span
)

# ── Processes ────────────────────────────────────────────────────

pub record ProcessDecl(
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

pub record MachineStateDecl(
  name: String,
  params: list[Param],
  terminal: Bool,
  guard: Expr?,
  transition_to: String?,
  transition_args: list[Expr],
  span: Span
)

# ── Effects ──────────────────────────────────────────────────────

pub record EffectDecl(
  name: String,
  operations: list[CellDef],
  span: Span
)

pub record EffectBindDecl(
  effect_path: String,
  tool_alias: String,
  span: Span
)

pub record HandlerDecl(
  name: String,
  handles: list[CellDef],
  span: Span,
  doc: String?
)

pub record EffectHandler(
  effect_name: String,
  operation: String,
  params: list[Param],
  body: list[Stmt],
  span: Span
)

# ── Addons ───────────────────────────────────────────────────────

pub record AddonDecl(
  kind: String,
  name: String?,
  span: Span
)

# ── Tools and Grants ─────────────────────────────────────────────

pub record UseToolDecl(
  tool_path: String,
  alias: String,
  mcp_url: String?,
  span: Span
)

pub record GrantDecl(
  tool_alias: String,
  constraints: list[GrantConstraint],
  span: Span
)

pub record GrantConstraint(
  key: String,
  value: Expr,
  span: Span
)

# ── Type aliases, traits, impls ──────────────────────────────────

pub record TypeAliasDef(
  name: String,
  generic_params: list[GenericParam],
  type_expr: TypeExpr,
  is_pub: Bool,
  span: Span,
  doc: String?
)

pub record TraitDef(
  name: String,
  parent_traits: list[String],
  methods: list[CellDef],
  is_pub: Bool,
  span: Span
)

pub record ImplDef(
  trait_name: String,
  generic_params: list[GenericParam],
  target_type: String,
  cells: list[CellDef],
  span: Span
)

# ── Imports ──────────────────────────────────────────────────────

pub enum ImportList
  ImportNames(payload: ImportNamesVal)
  ImportWildcard
end

pub record ImportNamesVal(names: list[ImportName])

pub record ImportName(
  name: String,
  alias: String?,
  span: Span
)

pub record ImportDecl(
  path: list[String],
  names: ImportList,
  is_pub: Bool,
  span: Span
)

# ── Constants and Macros ─────────────────────────────────────────

pub record ConstDeclDef(
  name: String,
  type_ann: TypeExpr?,
  value: Expr,
  span: Span
)

pub record MacroDeclDef(
  name: String,
  params: list[String],
  body: list[Stmt],
  span: Span
)

# ══════════════════════════════════════════════════════════════════
# Statements
# ══════════════════════════════════════════════════════════════════

pub enum Stmt
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

pub record LetStmtDef(
  name: String,
  mutable: Bool,
  pattern: Pattern?,
  ty: TypeExpr?,
  value: Expr,
  span: Span
)

pub record IfStmtDef(
  condition: Expr,
  then_body: list[Stmt],
  else_body: list[Stmt]?,
  span: Span
)

pub record ForStmtDef(
  label: String?,
  var: String,
  pattern: Pattern?,
  iter: Expr,
  filter: Expr?,
  body: list[Stmt],
  span: Span
)

pub record MatchStmtDef(
  subject: Expr,
  arms: list[MatchArm],
  span: Span
)

pub record MatchArm(
  pattern: Pattern,
  body: list[Stmt],
  span: Span
)

pub record ReturnStmtDef(
  value: Expr,
  span: Span
)

pub record HaltStmtDef(
  message: Expr,
  span: Span
)

pub record ExprStmtDef(
  expr: Expr,
  span: Span
)

pub record AssignStmtDef(
  target: String,
  value: Expr,
  span: Span
)

pub record WhileStmtDef(
  label: String?,
  condition: Expr,
  body: list[Stmt],
  span: Span
)

pub record LoopStmtDef(
  label: String?,
  body: list[Stmt],
  span: Span
)

pub record BreakStmtDef(
  label: String?,
  value: Expr?,
  span: Span
)

pub record ContinueStmtDef(
  label: String?,
  span: Span
)

pub record EmitStmtDef(
  value: Expr,
  span: Span
)

pub record CompoundAssignStmtDef(
  target: String,
  op: CompoundOp,
  value: Expr,
  span: Span
)

pub record DeferStmtDef(
  body: list[Stmt],
  span: Span
)

pub record YieldStmtDef(
  value: Expr,
  span: Span
)

# ══════════════════════════════════════════════════════════════════
# Patterns
# ══════════════════════════════════════════════════════════════════

pub enum Pattern
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

pub record LiteralPatDef(expr: Expr)
pub record VariantPatDef(name: String, inner: Pattern?, span: Span)
pub record WildcardPatDef(span: Span)
pub record IdentPatDef(name: String, span: Span)
pub record GuardPatDef(inner: Pattern, condition: Expr, span: Span)
pub record OrPatDef(patterns: list[Pattern], span: Span)
pub record ListDestructureDef(elements: list[Pattern], rest: String?, span: Span)
pub record TupleDestructureDef(elements: list[Pattern], span: Span)
pub record RecordDestructureField(name: String, pattern: Pattern?)
pub record RecordDestructureDef(type_name: String, fields: list[RecordDestructureField], open: Bool, span: Span)
pub record TypeCheckPatDef(name: String, type_expr: TypeExpr, span: Span)
pub record RangePatDef(start: Expr, end_val: Expr, inclusive: Bool, span: Span)

# ══════════════════════════════════════════════════════════════════
# Expressions
# ══════════════════════════════════════════════════════════════════

# String interpolation segment
pub enum StringSegment
  LiteralSeg(payload: StringLitVal)
  InterpSeg(payload: InterpExprVal)
  FormattedInterpSeg(payload: FormattedInterpVal)
end

pub record StringLitVal(text: String)
pub record InterpExprVal(expr: Expr)
pub record FormattedInterpVal(expr: Expr, spec: FormatSpec)

# Format spec components
pub enum FormatAlign
  AlignLeft
  AlignRight
  AlignCenter
end

pub enum FormatType
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

pub record FormatSpec(
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
pub enum CallArg
  Positional(payload: PositionalArgDef)
  NamedArg(payload: NamedArgDef)
  RoleArg(payload: RoleArgDef)
end

pub record PositionalArgDef(expr: Expr)
pub record NamedArgDef(name: String, expr: Expr, span: Span)
pub record RoleArgDef(role: String, expr: Expr, span: Span)

# When-expression arm
pub record WhenArm(
  condition: Expr,
  body: Expr,
  span: Span
)

# Comprehension clause
pub record ComprehensionClause(
  var: String,
  iter: Expr
)

pub enum ComprehensionKind
  ListComp
  MapComp
  SetComp
end

# Lambda body variants
pub enum LambdaBody
  LambdaExpr(payload: LambdaExprBody)
  LambdaBlock(payload: LambdaBlockBody)
end

pub record LambdaExprBody(expr: Expr)
pub record LambdaBlockBody(stmts: list[Stmt])

# ── The Expr enum ────────────────────────────────────────────────

pub enum Expr
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

pub record IntLitExprDef(value: Int, span: Span)
pub record BigIntLitExprDef(value: String, span: Span)
pub record FloatLitExprDef(value: Float, span: Span)
pub record StringLitExprDef(value: String, span: Span)
pub record StringInterpExprDef(segments: list[StringSegment], span: Span)
pub record BoolLitExprDef(value: Bool, span: Span)
pub record NullLitExprDef(span: Span)
pub record RawStringLitExprDef(value: String, span: Span)
pub record BytesLitExprDef(value: list[Int], span: Span)

pub record IdentExprDef(name: String, span: Span)

pub record ListLitExprDef(elements: list[Expr], span: Span)
pub record MapLitExprDef(entries: list[MapEntry], span: Span)
pub record MapEntry(key: Expr, value: Expr)
pub record RecordLitExprDef(name: String, fields: list[RecordFieldInit], span: Span)
pub record RecordFieldInit(name: String, value: Expr)
pub record TupleLitExprDef(elements: list[Expr], span: Span)
pub record SetLitExprDef(elements: list[Expr], span: Span)

pub record BinOpExprDef(left: Expr, op: BinOp, right: Expr, span: Span)
pub record UnaryOpExprDef(op: UnaryOp, operand: Expr, span: Span)

pub record CallExprDef(callee: Expr, args: list[CallArg], span: Span)
pub record ToolCallExprDef(callee: Expr, args: list[CallArg], span: Span)

pub record DotAccessExprDef(object: Expr, field: String, span: Span)
pub record IndexAccessExprDef(object: Expr, index: Expr, span: Span)

pub record RoleBlockExprDef(role: String, body: Expr, span: Span)
pub record ExpectSchemaExprDef(expr: Expr, schema_name: String, span: Span)

pub record LambdaExprDef(params: list[Param], return_type: TypeExpr?, body: LambdaBody, span: Span)

pub record RangeExprDef(start: Expr?, end_val: Expr?, inclusive: Bool, step: Expr?, span: Span)

pub record TryExprDef(expr: Expr, span: Span)
pub record TryElseExprDef(expr: Expr, error_binding: String, handler: Expr, span: Span)

pub record NullCoalesceExprDef(lhs: Expr, rhs: Expr, span: Span)
pub record NullSafeAccessExprDef(object: Expr, field: String, span: Span)
pub record NullSafeIndexExprDef(object: Expr, index: Expr, span: Span)
pub record NullAssertExprDef(expr: Expr, span: Span)

pub record SpreadExprDef(expr: Expr, span: Span)

pub record IfExprDef(cond: Expr, then_val: Expr, else_val: Expr, span: Span)
pub record MatchExprDef(subject: Expr, arms: list[MatchArm], span: Span)
pub record WhenExprDef(arms: list[WhenArm], else_body: Expr?, span: Span)
pub record BlockExprDef(stmts: list[Stmt], span: Span)

pub record AwaitExprDef(expr: Expr, span: Span)

pub record ComprehensionExprDef(
  body: Expr,
  var: String,
  iter: Expr,
  extra_clauses: list[ComprehensionClause],
  condition: Expr?,
  kind: ComprehensionKind,
  span: Span
)

pub record PipeExprDef(left: Expr, right: Expr, span: Span)

pub record IsTypeExprDef(expr: Expr, type_name: String, span: Span)
pub record TypeCastExprDef(expr: Expr, target_type: String, span: Span)

pub record ComptimeExprDef(expr: Expr, span: Span)

pub record PerformExprDef(effect_name: String, operation: String, args: list[Expr], span: Span)
pub record HandleExprDef(body: list[Stmt], handlers: list[EffectHandler], span: Span)
pub record ResumeExprDef(value: Expr, span: Span)
```
