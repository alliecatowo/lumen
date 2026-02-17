# std.compiler.lir — Lumen Intermediate Representation

LIR types for the register-based VM, ported from
`rust/lumen-compiler/src/compiler/lir.rs`. Includes opcodes,
intrinsic IDs, instruction encoding, and module structure.

```lumen
import std.compiler.span: Span

# ══════════════════════════════════════════════════════════════════
# Opcodes
# ══════════════════════════════════════════════════════════════════

enum OpCode
  # ── Misc ────────────────────────────────────────────────────
  Nop             # 0x00  no operation

  # ── Register / constant ops ────────────────────────────────
  LoadK           # 0x01  load constant Bx into register A
  LoadNil         # 0x02  set registers A..A+B to nil
  LoadBool        # 0x03  load bool B into A; if C skip next
  LoadInt         # 0x04  R[A] = sB as i64
  Move            # 0x05  copy register B to A

  # ── Data construction ──────────────────────────────────────
  NewList         # 0x06  create list from B values at A+1..
  NewMap          # 0x07  create map from B kv pairs
  NewRecord       # 0x08  create record of type Bx
  NewUnion        # 0x09  create union tag=B payload=C
  NewTuple        # 0x0A  create tuple from B values
  NewSet          # 0x0B  create set from B values

  # ── Access ─────────────────────────────────────────────────
  GetField        # 0x10  A = B.field[C]
  SetField        # 0x11  A.field[B] = C
  GetIndex        # 0x12  A = B[C]
  SetIndex        # 0x13  A[B] = C
  GetTuple        # 0x14  A = R[B].elements[C]

  # ── Arithmetic ─────────────────────────────────────────────
  OpAdd           # 0x20  A = B + C
  OpSub           # 0x21  A = B - C
  OpMul           # 0x22  A = B * C
  OpDiv           # 0x23  A = B / C
  OpMod           # 0x24  A = B % C
  OpPow           # 0x25  A = B ** C
  OpNeg           # 0x26  A = -B
  OpConcat        # 0x27  A = B ++ C
  OpFloorDiv      # 0x2E  A = B // C

  # ── Bitwise ────────────────────────────────────────────────
  OpBitOr         # 0x28  A = B | C
  OpBitAnd        # 0x29  A = B & C
  OpBitXor        # 0x2A  A = B ^ C
  OpBitNot        # 0x2B  A = ~B
  OpShl           # 0x2C  A = B << C
  OpShr           # 0x2D  A = B >> C

  # ── Comparison / logic ─────────────────────────────────────
  OpEq            # 0x30  if (B == C) != A then skip next
  OpLt            # 0x31  if (B < C) != A then skip next
  OpLe            # 0x32  if (B <= C) != A then skip next
  OpNot           # 0x33  A = not B
  OpAnd           # 0x34  A = B and C
  OpOr            # 0x35  A = B or C
  OpIn            # 0x36  A = B in C
  OpIs            # 0x37  A = typeof(B) == type(C)
  NullCo          # 0x38  A = if B != null then B else C
  Test            # 0x39  if (R[A] truthy) != C then skip next

  # ── Control flow ───────────────────────────────────────────
  Jmp             # 0x40  jump by signed offset
  Call            # 0x41  call A with B args, C results
  TailCall        # 0x42  tail-call A with B args
  Return          # 0x43  return B values starting from A
  Halt            # 0x44  halt with error message in A
  Loop            # 0x45  decrement counter, jump if > 0
  ForPrep         # 0x46  prepare for-loop
  ForLoop         # 0x47  iterate for-loop
  ForIn           # 0x48  for-in iterator step
  Break           # 0x49  break from loop
  Continue        # 0x4A  continue to next iteration

  # ── Intrinsics ─────────────────────────────────────────────
  Intrinsic       # 0x50  A = intrinsic[B](args at C)

  # ── Closures ───────────────────────────────────────────────
  Closure         # 0x51  R[A] = closure(proto=Bx)
  GetUpval        # 0x52  R[A] = upvalue[B]
  SetUpval        # 0x53  upvalue[B] = R[A]

  # ── Effects ────────────────────────────────────────────────
  ToolCall        # 0x60  tool_call(tool=Bx)
  Schema          # 0x61  validate A against schema B
  Emit            # 0x62  emit output R[A]
  TraceRef        # 0x63  R[A] = current trace ref
  Await           # 0x64  R[A] = await future R[B]
  Spawn           # 0x65  R[A] = spawn async(proto=Bx)
  Perform         # 0x66  perform effect B, operation C
  HandlePush      # 0x67  push handler scope at offset Ax
  HandlePop       # 0x68  pop current handler scope
  Resume          # 0x69  resume with value in A

  # ── List ops ───────────────────────────────────────────────
  Append          # 0x70  append B to list A

  # ── Type checks ────────────────────────────────────────────
  IsVariant       # 0x71  if A is variant tag Bx, skip next
  Unbox           # 0x72  A = B.payload (unions)
end

# ══════════════════════════════════════════════════════════════════
# Intrinsic IDs
# ══════════════════════════════════════════════════════════════════

enum IntrinsicId
  ILength         # 0   length/len
  ICount          # 1   count
  IMatches        # 2   matches
  IHash           # 3   hash
  IDiff           # 4   diff
  IPatch          # 5   patch
  IRedact         # 6   redact
  IValidate       # 7   validate
  ITraceRef       # 8   trace_ref
  IPrint          # 9   print
  IToString       # 10  to_string
  IToInt          # 11  to_int
  IToFloat        # 12  to_float
  ITypeOf         # 13  type_of
  IKeys           # 14  keys
  IValues         # 15  values
  IContains       # 16  contains
  IJoin           # 17  join
  ISplit          # 18  split
  ITrim           # 19  trim
  IUpper          # 20  upper
  ILower          # 21  lower
  IReplace        # 22  replace
  ISlice          # 23  slice
  IAppend         # 24  append
  IRange          # 25  range
  IAbs            # 26  abs
  IMin            # 27  min
  IMax            # 28  max
  ISort           # 29  sort
  IReverse        # 30  reverse
  IMap            # 31  map
  IFilter         # 32  filter
  IReduce         # 33  reduce
  IFlatMap        # 34  flat_map
  IZip            # 35  zip
  IEnumerate      # 36  enumerate
  IAny            # 37  any
  IAll            # 38  all
  IFind           # 39  find
  IPosition       # 40  position
  IGroupBy        # 41  group_by
  IChunk          # 42  chunk
  IWindow         # 43  window
  IFlatten        # 44  flatten
  IUnique         # 45  unique
  ITake           # 46  take
  IDrop           # 47  drop
  IFirst          # 48  first
  ILast           # 49  last
  IIsEmpty        # 50  is_empty
  IChars          # 51  chars
  IStartsWith     # 52  starts_with
  IEndsWith       # 53  ends_with
  IIndexOf        # 54  index_of
  IPadLeft        # 55  pad_left
  IPadRight       # 56  pad_right
  IRound          # 57  round
  ICeil           # 58  ceil
  IFloor          # 59  floor
  ISqrt           # 60  sqrt
  IPow            # 61  pow
  ILog            # 62  log
  ISin            # 63  sin
  ICos            # 64  cos
  IClamp          # 65  clamp
  IClone          # 66  clone
  ISizeof         # 67  sizeof
  IDebug          # 68  debug
  IToSet          # 69  to_set
  IHasKey         # 70  has_key
  IMerge          # 71  merge
  ISize           # 72  size
  IAdd            # 73  add
  IRemove         # 74  remove
  IEntries        # 75  entries
  ICompose        # 76  compose
  IFormat         # 77  format
  IPartition      # 78  partition
  IReadDir        # 79  read_dir
  IExists         # 80  exists
  IMkdir          # 81  mkdir
  IEval           # 82  eval
  IGuardrail      # 83  guardrail
  IPattern        # 84  pattern
  IExit           # 85  exit
end

# ══════════════════════════════════════════════════════════════════
# Instructions
# ══════════════════════════════════════════════════════════════════

# 32-bit fixed-width instruction (Lua-style encoding).
# Fields: op (8-bit), a/b/c (8-bit each).
# ABx format: a (8-bit), bx = (b << 8) | c (16-bit).
# Ax  format: ax = (a << 16) | (b << 8) | c (24-bit).
record Instruction(
  op: OpCode,
  a: Int,
  b: Int,
  c: Int
)

# Construct an ABC-format instruction.
cell instr_abc(op: OpCode, a: Int, b: Int, c: Int) -> Instruction
  return Instruction(op: op, a: a, b: b, c: c)
end

# Construct an ABx-format instruction.
cell instr_abx(op: OpCode, a: Int, bx: Int) -> Instruction
  let b = bx // 256
  let c = bx % 256
  return Instruction(op: op, a: a, b: b, c: c)
end

# Construct an Ax-format instruction (unsigned 24-bit).
cell instr_ax(op: OpCode, ax: Int) -> Instruction
  let a = (ax // 65536) % 256
  let b = (ax // 256) % 256
  let c = ax % 256
  return Instruction(op: op, a: a, b: b, c: c)
end

# Construct a signed Ax-format instruction (for jump offsets).
cell instr_sax(op: OpCode, offset: Int) -> Instruction
  # Mask to 24 bits (handles negative via two's complement)
  let bits = offset % 16777216
  if bits < 0
    bits = bits + 16777216
  end
  return instr_ax(op, bits)
end

# Extract Bx (16-bit unsigned) from instruction.
cell get_bx(instr: Instruction) -> Int
  return instr.b * 256 + instr.c
end

# Extract Ax (24-bit unsigned) from instruction.
cell get_ax(instr: Instruction) -> Int
  return instr.a * 65536 + instr.b * 256 + instr.c
end

# Extract signed Ax (24-bit with sign extension) from instruction.
cell get_sax(instr: Instruction) -> Int
  let raw = get_ax(instr)
  if raw >= 8388608
    return raw - 16777216
  end
  return raw
end

# ══════════════════════════════════════════════════════════════════
# Constants
# ══════════════════════════════════════════════════════════════════

enum Constant
  NullConst
  BoolConst(payload: BoolConstVal)
  IntConst(payload: IntConstVal)
  BigIntConst(payload: BigIntConstVal)
  FloatConst(payload: FloatConstVal)
  StringConst(payload: StringConstVal)
end

record BoolConstVal(value: Bool)
record IntConstVal(value: Int)
record BigIntConstVal(value: String)
record FloatConstVal(value: Float)
record StringConstVal(value: String)

# ══════════════════════════════════════════════════════════════════
# LIR Module structure
# ══════════════════════════════════════════════════════════════════

# A type definition in LIR.
record LirType(
  kind: String,
  name: String,
  fields: list[LirField],
  variants: list[LirVariant]
)

record LirField(
  name: String,
  ty: String,
  constraints: list[String]
)

record LirVariant(
  name: String,
  payload: String?
)

# A compiled cell (function) in LIR.
record LirCell(
  name: String,
  params: list[LirParam],
  returns: String?,
  registers: Int,
  constants: list[Constant],
  instructions: list[Instruction],
  effect_handler_metas: list[LirEffectHandlerMeta]
)

record LirParam(
  name: String,
  ty: String,
  register: Int,
  variadic: Bool
)

# Effect handler metadata for handle...with...end expressions.
record LirEffectHandlerMeta(
  effect_name: String,
  operation: String,
  param_count: Int,
  handler_ip: Int
)

# Tool declaration in LIR.
record LirTool(
  alias: String,
  tool_id: String,
  version: String,
  mcp_url: String?
)

# Policy/grant in LIR.
record LirPolicy(
  tool_alias: String,
  grants: String
)

record LirAgent(
  name: String,
  methods: list[String]
)

record LirAddon(
  kind: String,
  name: String?
)

# Effect definitions in LIR.
record LirEffect(
  name: String,
  operations: list[LirEffectOp]
)

record LirEffectOp(
  name: String,
  params: list[LirParam],
  returns: String?,
  effects: list[String]
)

record LirEffectBind(
  effect_path: String,
  tool_alias: String
)

record LirHandler(
  name: String,
  handles: list[LirHandle]
)

record LirHandle(
  operation: String,
  cell: String
)

# Complete LIR module — the output of compilation.
record LirModule(
  version: String,
  doc_hash: String,
  strings: list[String],
  types: list[LirType],
  cells: list[LirCell],
  tools: list[LirTool],
  policies: list[LirPolicy],
  agents: list[LirAgent],
  addons: list[LirAddon],
  effects: list[LirEffect],
  effect_binds: list[LirEffectBind],
  handlers: list[LirHandler]
)

# Create a new empty LIR module.
cell new_module(doc_hash: String) -> LirModule
  return LirModule(
    version: "1.0.0",
    doc_hash: doc_hash,
    strings: [],
    types: [],
    cells: [],
    tools: [],
    policies: [],
    agents: [],
    addons: [],
    effects: [],
    effect_binds: [],
    handlers: []
  )
end
```
