# LIR Bytecode ABI Reference

This document defines the frozen ABI for Lumen LIR bytecode version 1.0.
Both the Rust compiler and the self-hosted compiler MUST produce identical
binary output for the same source input. Any change to these constants
requires a version bump and migration path.

## Instruction Encoding

All instructions are 32 bits wide (4 bytes), encoded big-endian:

```
ABC format:  [op:8][a:8][b:8][c:8]
ABx format:  [op:8][a:8][bx:16]     — bx = (b << 8) | c
Ax  format:  [op:8][ax:24]          — ax = (a << 16) | (b << 8) | c
sAx format:  [op:8][sax:24]         — signed 24-bit, sign-extended from bit 23
```

Signed jump offsets (Jmp, Break, Continue) use sAx encoding. The sign bit
is bit 23. Values with bit 23 set are negative (OR with 0xFF000000 and
reinterpret as i32).

## Opcode Table

```lumen
# Misc
let OP_NOP = 0x00

# Register and constant ops
let OP_LOAD_K = 0x01
let OP_LOAD_NIL = 0x02
let OP_LOAD_BOOL = 0x03
let OP_LOAD_INT = 0x04
let OP_MOVE = 0x05

# Data construction
let OP_NEW_LIST = 0x06
let OP_NEW_MAP = 0x07
let OP_NEW_RECORD = 0x08
let OP_NEW_UNION = 0x09
let OP_NEW_TUPLE = 0x0A
let OP_NEW_SET = 0x0B

# Access
let OP_GET_FIELD = 0x10
let OP_SET_FIELD = 0x11
let OP_GET_INDEX = 0x12
let OP_SET_INDEX = 0x13
let OP_GET_TUPLE = 0x14

# Arithmetic
let OP_ADD = 0x20
let OP_SUB = 0x21
let OP_MUL = 0x22
let OP_DIV = 0x23
let OP_MOD = 0x24
let OP_POW = 0x25
let OP_NEG = 0x26
let OP_CONCAT = 0x27
let OP_BIT_OR = 0x28
let OP_BIT_AND = 0x29
let OP_BIT_XOR = 0x2A
let OP_BIT_NOT = 0x2B
let OP_SHL = 0x2C
let OP_SHR = 0x2D
let OP_FLOOR_DIV = 0x2E

# Comparison / logic
let OP_EQ = 0x30
let OP_LT = 0x31
let OP_LE = 0x32
let OP_NOT = 0x33
let OP_AND = 0x34
let OP_OR = 0x35
let OP_IN = 0x36
let OP_IS = 0x37
let OP_NULL_CO = 0x38
let OP_TEST = 0x39

# Control flow
let OP_JMP = 0x40
let OP_CALL = 0x41
let OP_TAIL_CALL = 0x42
let OP_RETURN = 0x43
let OP_HALT = 0x44
let OP_LOOP = 0x45
let OP_FOR_PREP = 0x46
let OP_FOR_LOOP = 0x47
let OP_FOR_IN = 0x48
let OP_BREAK = 0x49
let OP_CONTINUE = 0x4A

# Intrinsics
let OP_INTRINSIC = 0x50

# Closures
let OP_CLOSURE = 0x51
let OP_GET_UPVAL = 0x52
let OP_SET_UPVAL = 0x53

# Effects
let OP_TOOL_CALL = 0x60
let OP_SCHEMA = 0x61
let OP_EMIT = 0x62
let OP_TRACE_REF = 0x63
let OP_AWAIT = 0x64
let OP_SPAWN = 0x65
let OP_PERFORM = 0x66
let OP_HANDLE_PUSH = 0x67
let OP_HANDLE_POP = 0x68
let OP_RESUME = 0x69

# List ops
let OP_APPEND = 0x70

# Type checks
let OP_IS_VARIANT = 0x71
let OP_UNBOX = 0x72
```

## Intrinsic ID Table

```lumen
let INTRINSIC_LENGTH = 0
let INTRINSIC_COUNT = 1
let INTRINSIC_MATCHES = 2
let INTRINSIC_HASH = 3
let INTRINSIC_DIFF = 4
let INTRINSIC_PATCH = 5
let INTRINSIC_REDACT = 6
let INTRINSIC_VALIDATE = 7
let INTRINSIC_TRACE_REF = 8
let INTRINSIC_PRINT = 9
let INTRINSIC_TO_STRING = 10
let INTRINSIC_TO_INT = 11
let INTRINSIC_TO_FLOAT = 12
let INTRINSIC_TYPE_OF = 13
let INTRINSIC_KEYS = 14
let INTRINSIC_VALUES = 15
let INTRINSIC_CONTAINS = 16
let INTRINSIC_JOIN = 17
let INTRINSIC_SPLIT = 18
let INTRINSIC_TRIM = 19
let INTRINSIC_UPPER = 20
let INTRINSIC_LOWER = 21
let INTRINSIC_REPLACE = 22
let INTRINSIC_SLICE = 23
let INTRINSIC_APPEND = 24
let INTRINSIC_RANGE = 25
let INTRINSIC_ABS = 26
let INTRINSIC_MIN = 27
let INTRINSIC_MAX = 28
let INTRINSIC_SORT = 29
let INTRINSIC_REVERSE = 30
let INTRINSIC_MAP = 31
let INTRINSIC_FILTER = 32
let INTRINSIC_REDUCE = 33
let INTRINSIC_FLAT_MAP = 34
let INTRINSIC_ZIP = 35
let INTRINSIC_ENUMERATE = 36
let INTRINSIC_ANY = 37
let INTRINSIC_ALL = 38
let INTRINSIC_FIND = 39
let INTRINSIC_POSITION = 40
let INTRINSIC_GROUP_BY = 41
let INTRINSIC_CHUNK = 42
let INTRINSIC_WINDOW = 43
let INTRINSIC_FLATTEN = 44
let INTRINSIC_UNIQUE = 45
let INTRINSIC_TAKE = 46
let INTRINSIC_DROP = 47
let INTRINSIC_FIRST = 48
let INTRINSIC_LAST = 49
let INTRINSIC_IS_EMPTY = 50
let INTRINSIC_CHARS = 51
let INTRINSIC_STARTS_WITH = 52
let INTRINSIC_ENDS_WITH = 53
let INTRINSIC_INDEX_OF = 54
let INTRINSIC_PAD_LEFT = 55
let INTRINSIC_PAD_RIGHT = 56
let INTRINSIC_ROUND = 57
let INTRINSIC_CEIL = 58
let INTRINSIC_FLOOR = 59
let INTRINSIC_SQRT = 60
let INTRINSIC_POW = 61
let INTRINSIC_LOG = 62
let INTRINSIC_SIN = 63
let INTRINSIC_COS = 64
let INTRINSIC_CLAMP = 65
let INTRINSIC_CLONE = 66
let INTRINSIC_SIZEOF = 67
let INTRINSIC_DEBUG = 68
let INTRINSIC_TO_SET = 69
let INTRINSIC_HAS_KEY = 70
let INTRINSIC_MERGE = 71
let INTRINSIC_SIZE = 72
let INTRINSIC_ADD = 73
let INTRINSIC_REMOVE = 74
let INTRINSIC_ENTRIES = 75
let INTRINSIC_COMPOSE = 76
let INTRINSIC_FORMAT = 77
let INTRINSIC_PARTITION = 78
let INTRINSIC_READ_DIR = 79
let INTRINSIC_EXISTS = 80
let INTRINSIC_MKDIR = 81
let INTRINSIC_EVAL = 82
let INTRINSIC_GUARDRAIL = 83
let INTRINSIC_PATTERN = 84
let INTRINSIC_EXIT = 85
```

## Constant Pool Encoding

Constants in the binary format use a 1-byte tag followed by the value:

| Tag | Type    | Payload                           |
|-----|---------|-----------------------------------|
| 0   | Null    | (none)                            |
| 1   | Bool    | 1 byte (0=false, 1=true)          |
| 2   | Int     | 8 bytes, signed, big-endian       |
| 3   | BigInt  | 4-byte length + UTF-8 decimal str |
| 4   | Float   | 8 bytes, IEEE 754, big-endian     |
| 5   | String  | 4-byte length + UTF-8 bytes       |

## LIR Module Binary Format

```
Header:
  magic:       4 bytes  "LIR\x01"
  version_len: 2 bytes  (big-endian)
  version:     N bytes  UTF-8 version string
  hash_len:    2 bytes  (big-endian)
  doc_hash:    N bytes  UTF-8 hash string

String Table:
  count:       4 bytes  (big-endian)
  entries:     repeated { 4-byte length + UTF-8 bytes }

Type Table:
  count:       4 bytes  (big-endian)
  entries:     repeated LirType (see below)

Cell Table:
  count:       4 bytes  (big-endian)
  entries:     repeated LirCell (see below)

Tool Table:
  count:       4 bytes  (big-endian)
  entries:     repeated LirTool

Policy Table:
  count:       4 bytes  (big-endian)
  entries:     repeated LirPolicy

Effect Table:
  count:       4 bytes  (big-endian)
  entries:     repeated LirEffect
```

## Instruction Encoding Helpers

```lumen
cell encode_abc(op: Int, a: Int, b: Int, c: Int) -> Int
  (op * 16777216) + (a * 65536) + (b * 256) + c
end

cell encode_abx(op: Int, a: Int, bx: Int) -> Int
  let b = bx // 256
  let c = bx % 256
  encode_abc(op, a, b, c)
end

cell encode_ax(op: Int, ax: Int) -> Int
  let a = (ax // 65536) % 256
  let b = (ax // 256) % 256
  let c = ax % 256
  encode_abc(op, a, b, c)
end

cell encode_sax(op: Int, offset: Int) -> Int
  let bits = if offset < 0 then
    (offset + 16777216) % 16777216
  else
    offset
  end
  encode_ax(op, bits)
end

cell decode_op(instruction: Int) -> Int
  instruction // 16777216
end

cell decode_a(instruction: Int) -> Int
  (instruction // 65536) % 256
end

cell decode_b(instruction: Int) -> Int
  (instruction // 256) % 256
end

cell decode_c(instruction: Int) -> Int
  instruction % 256
end

cell decode_bx(instruction: Int) -> Int
  instruction % 65536
end

cell decode_ax(instruction: Int) -> Int
  instruction % 16777216
end

cell decode_sax(instruction: Int) -> Int
  let raw = decode_ax(instruction)
  if raw >= 8388608 then
    raw - 16777216
  else
    raw
  end
end
```

## ABI Version

```lumen
let ABI_VERSION = "1.0.0"
let ABI_MAGIC = "LIR\x01"
```
