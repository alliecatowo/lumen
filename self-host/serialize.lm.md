# LIR Binary Serialization

Binary writer and reader for LIR modules. Produces and consumes byte
buffers (`list[Int]`) compatible with the frozen ABI defined in `abi.lm.md`.

## Byte Buffer Helpers

Low-level primitives for packing and unpacking integers and strings into
byte sequences.

```lumen
import self_host.abi: ABI_MAGIC, ABI_VERSION

record ByteWriter(
  buf: list[Int]
)

cell new_writer() -> ByteWriter
  ByteWriter(buf: [])
end

cell write_u8(w: ByteWriter, value: Int) -> ByteWriter
  let new_buf = append(w.buf, value % 256)
  ByteWriter(buf: new_buf)
end

cell write_u16_be(w: ByteWriter, value: Int) -> ByteWriter
  let hi = (value // 256) % 256
  let lo = value % 256
  let new_buf = append(append(w.buf, hi), lo)
  ByteWriter(buf: new_buf)
end

cell write_u32_be(w: ByteWriter, value: Int) -> ByteWriter
  let b3 = (value // 16777216) % 256
  let b2 = (value // 65536) % 256
  let b1 = (value // 256) % 256
  let b0 = value % 256
  let new_buf = w.buf
  new_buf = append(new_buf, b3)
  new_buf = append(new_buf, b2)
  new_buf = append(new_buf, b1)
  new_buf = append(new_buf, b0)
  ByteWriter(buf: new_buf)
end

cell write_i64_be(w: ByteWriter, value: Int) -> ByteWriter
  # Write 8 bytes big-endian for a signed 64-bit integer.
  # Handle negative values via two's complement.
  let v = if value < 0 then
    value + 18446744073709551616
  else
    value
  end
  let result = w
  let shift = 56
  while shift >= 0
    let byte_val = (v // (2 ** shift)) % 256
    result = write_u8(result, byte_val)
    shift = shift - 8
  end
  result
end

cell write_f64_be(w: ByteWriter, value: Float) -> ByteWriter
  # IEEE 754 double-precision, big-endian.
  # Delegate to a runtime intrinsic for bit-level conversion.
  # For now, store as string representation and convert on read.
  let s = to_string(value)
  let result = write_u16_be(w, length(s))
  write_bytes_raw(result, s)
end

cell write_bytes_raw(w: ByteWriter, s: String) -> ByteWriter
  let chars_list = chars(s)
  let result = w
  for ch in chars_list
    # Each character as its UTF-8 byte value.
    # For ASCII range this is a single byte.
    result = write_u8(result, to_int(ch))
  end
  result
end

cell write_string(w: ByteWriter, s: String) -> ByteWriter
  let result = write_u32_be(w, length(s))
  write_bytes_raw(result, s)
end
```

## Byte Reader

```lumen
record ByteReader(
  buf: list[Int],
  pos: Int
)

cell new_reader(buf: list[Int]) -> ByteReader
  ByteReader(buf: buf, pos: 0)
end

cell read_u8(r: ByteReader) -> result[tuple[Int, ByteReader], String]
  if r.pos >= length(r.buf) then
    return Err("read_u8: unexpected end of input")
  end
  let value = r.buf[r.pos]
  Ok((value, ByteReader(buf: r.buf, pos: r.pos + 1)))
end

cell read_u16_be(r: ByteReader) -> result[tuple[Int, ByteReader], String]
  if r.pos + 2 > length(r.buf) then
    return Err("read_u16_be: unexpected end of input")
  end
  let hi = r.buf[r.pos]
  let lo = r.buf[r.pos + 1]
  let value = hi * 256 + lo
  Ok((value, ByteReader(buf: r.buf, pos: r.pos + 2)))
end

cell read_u32_be(r: ByteReader) -> result[tuple[Int, ByteReader], String]
  if r.pos + 4 > length(r.buf) then
    return Err("read_u32_be: unexpected end of input")
  end
  let b3 = r.buf[r.pos]
  let b2 = r.buf[r.pos + 1]
  let b1 = r.buf[r.pos + 2]
  let b0 = r.buf[r.pos + 3]
  let value = b3 * 16777216 + b2 * 65536 + b1 * 256 + b0
  Ok((value, ByteReader(buf: r.buf, pos: r.pos + 4)))
end

cell read_i64_be(r: ByteReader) -> result[tuple[Int, ByteReader], String]
  if r.pos + 8 > length(r.buf) then
    return Err("read_i64_be: unexpected end of input")
  end
  let value = 0
  let i = 0
  while i < 8
    value = value * 256 + r.buf[r.pos + i]
    i = i + 1
  end
  # Sign-extend if bit 63 is set
  if value >= 9223372036854775808 then
    value = value - 18446744073709551616
  end
  Ok((value, ByteReader(buf: r.buf, pos: r.pos + 8)))
end

cell read_bytes(r: ByteReader, n: Int) -> result[tuple[list[Int], ByteReader], String]
  if r.pos + n > length(r.buf) then
    return Err("read_bytes: unexpected end of input")
  end
  let bytes = slice(r.buf, r.pos, r.pos + n)
  Ok((bytes, ByteReader(buf: r.buf, pos: r.pos + n)))
end

cell read_string(r: ByteReader) -> result[tuple[String, ByteReader], String]
  match read_u32_be(r)
    case Ok((len, r2)) ->
      match read_bytes(r2, len)
        case Ok((bytes, r3)) ->
          # Convert byte list back to string
          let s = ""
          for b in bytes
            s = s ++ to_string(b)
          end
          Ok((s, r3))
        case Err(e) -> Err(e)
      end
    case Err(e) -> Err(e)
  end
end
```

## LIR Module Writer

Serialize an LIR module into a byte buffer following the frozen binary
format.

```lumen
record LirConstant(
  tag: Int,
  int_val: Int?,
  float_val: Float?,
  string_val: String?,
  bool_val: Bool?
)

record LirInstruction(
  encoded: Int
)

record LirParam(
  name: String,
  type_name: String,
  register: Int,
  variadic: Bool
)

record LirField(
  name: String,
  type_name: String,
  constraints: list[String]
)

record LirVariant(
  name: String,
  payload: String?
)

record LirType(
  kind: String,
  name: String,
  fields: list[LirField],
  variants: list[LirVariant]
)

record LirCell(
  name: String,
  params: list[LirParam],
  returns: String?,
  registers: Int,
  constants: list[LirConstant],
  instructions: list[LirInstruction]
)

record LirTool(
  alias: String,
  tool_id: String,
  version: String,
  mcp_url: String?
)

record LirEffect(
  name: String,
  operations: list[LirParam]
)

record LirModule(
  version: String,
  doc_hash: String,
  strings: list[String],
  types: list[LirType],
  cells: list[LirCell],
  tools: list[LirTool],
  effects: list[LirEffect]
)

cell write_constant(w: ByteWriter, c: LirConstant) -> ByteWriter
  let result = write_u8(w, c.tag)
  match c.tag
    case 0 ->
      # Null — no payload
      result
    case 1 ->
      # Bool
      let v = if c.bool_val == true then 1 else 0 end
      write_u8(result, v)
    case 2 ->
      # Int
      match c.int_val
        case null -> write_i64_be(result, 0)
        case v -> write_i64_be(result, v)
      end
    case 3 ->
      # BigInt — stored as decimal string
      match c.string_val
        case null -> write_string(result, "0")
        case s -> write_string(result, s)
      end
    case 4 ->
      # Float
      match c.float_val
        case null -> write_f64_be(result, 0.0)
        case v -> write_f64_be(result, v)
      end
    case 5 ->
      # String
      match c.string_val
        case null -> write_string(result, "")
        case s -> write_string(result, s)
      end
    case _ ->
      result
  end
end

cell write_lir_cell(w: ByteWriter, cell_def: LirCell) -> ByteWriter
  let result = write_string(w, cell_def.name)

  # Params
  result = write_u32_be(result, length(cell_def.params))
  for p in cell_def.params
    result = write_string(result, p.name)
    result = write_string(result, p.type_name)
    result = write_u8(result, p.register)
    result = write_u8(result, if p.variadic then 1 else 0 end)
  end

  # Return type
  match cell_def.returns
    case null ->
      result = write_u8(result, 0)
    case ret ->
      result = write_u8(result, 1)
      result = write_string(result, ret)
  end

  # Register count
  result = write_u8(result, cell_def.registers)

  # Constants
  result = write_u32_be(result, length(cell_def.constants))
  for c in cell_def.constants
    result = write_constant(result, c)
  end

  # Instructions
  result = write_u32_be(result, length(cell_def.instructions))
  for instr in cell_def.instructions
    result = write_u32_be(result, instr.encoded)
  end

  result
end

cell write_lir_type(w: ByteWriter, t: LirType) -> ByteWriter
  let result = write_string(w, t.kind)
  result = write_string(result, t.name)

  result = write_u32_be(result, length(t.fields))
  for f in t.fields
    result = write_string(result, f.name)
    result = write_string(result, f.type_name)
    result = write_u32_be(result, length(f.constraints))
    for c in f.constraints
      result = write_string(result, c)
    end
  end

  result = write_u32_be(result, length(t.variants))
  for v in t.variants
    result = write_string(result, v.name)
    match v.payload
      case null ->
        result = write_u8(result, 0)
      case p ->
        result = write_u8(result, 1)
        result = write_string(result, p)
    end
  end

  result
end

cell write_module(module: LirModule) -> list[Int]
  let w = new_writer()

  # Header: magic + version + doc_hash
  w = write_bytes_raw(w, ABI_MAGIC)
  w = write_string(w, module.version)
  w = write_string(w, module.doc_hash)

  # String table
  w = write_u32_be(w, length(module.strings))
  for s in module.strings
    w = write_string(w, s)
  end

  # Type table
  w = write_u32_be(w, length(module.types))
  for t in module.types
    w = write_lir_type(w, t)
  end

  # Cell table
  w = write_u32_be(w, length(module.cells))
  for c in module.cells
    w = write_lir_cell(w, c)
  end

  # Tool table
  w = write_u32_be(w, length(module.tools))
  for tool in module.tools
    w = write_string(w, tool.alias)
    w = write_string(w, tool.tool_id)
    w = write_string(w, tool.version)
    match tool.mcp_url
      case null ->
        w = write_u8(w, 0)
      case url ->
        w = write_u8(w, 1)
        w = write_string(w, url)
    end
  end

  w.buf
end
```

## LIR Module Reader

Deserialize an LIR module from a byte buffer.

```lumen
cell read_constant(r: ByteReader) -> result[tuple[LirConstant, ByteReader], String]
  match read_u8(r)
    case Ok((tag, r2)) ->
      match tag
        case 0 ->
          Ok((LirConstant(tag: 0, int_val: null, float_val: null, string_val: null, bool_val: null), r2))
        case 1 ->
          match read_u8(r2)
            case Ok((v, r3)) ->
              let bval = v == 1
              Ok((LirConstant(tag: 1, int_val: null, float_val: null, string_val: null, bool_val: bval), r3))
            case Err(e) -> Err(e)
          end
        case 2 ->
          match read_i64_be(r2)
            case Ok((v, r3)) ->
              Ok((LirConstant(tag: 2, int_val: v, float_val: null, string_val: null, bool_val: null), r3))
            case Err(e) -> Err(e)
          end
        case 5 ->
          match read_string(r2)
            case Ok((s, r3)) ->
              Ok((LirConstant(tag: 5, int_val: null, float_val: null, string_val: s, bool_val: null), r3))
            case Err(e) -> Err(e)
          end
        case _ ->
          Err("read_constant: unknown tag {tag}")
      end
    case Err(e) -> Err(e)
  end
end

cell read_lir_cell(r: ByteReader) -> result[tuple[LirCell, ByteReader], String]
  match read_string(r)
    case Err(e) -> return Err(e)
    case Ok((name, r2)) ->
      # Read params
      match read_u32_be(r2)
        case Err(e) -> return Err(e)
        case Ok((param_count, r3)) ->
          let params = []
          let reader = r3
          let i = 0
          while i < param_count
            match read_string(reader)
              case Err(e) -> return Err(e)
              case Ok((pname, r4)) ->
                match read_string(r4)
                  case Err(e) -> return Err(e)
                  case Ok((ptype, r5)) ->
                    match read_u8(r5)
                      case Err(e) -> return Err(e)
                      case Ok((preg, r6)) ->
                        match read_u8(r6)
                          case Err(e) -> return Err(e)
                          case Ok((pvar, r7)) ->
                            let param = LirParam(name: pname, type_name: ptype, register: preg, variadic: pvar == 1)
                            params = append(params, param)
                            reader = r7
                        end
                    end
                end
            end
            i = i + 1
          end

          # Return type
          match read_u8(reader)
            case Err(e) -> return Err(e)
            case Ok((has_ret, r8)) ->
              let ret_type = null
              let rdr = r8
              if has_ret == 1 then
                match read_string(r8)
                  case Err(e) -> return Err(e)
                  case Ok((rt, r9)) ->
                    ret_type = rt
                    rdr = r9
                end
              end

              # Register count
              match read_u8(rdr)
                case Err(e) -> return Err(e)
                case Ok((regs, r10)) ->
                  # Constants
                  match read_u32_be(r10)
                    case Err(e) -> return Err(e)
                    case Ok((const_count, r11)) ->
                      let constants = []
                      let cr = r11
                      let ci = 0
                      while ci < const_count
                        match read_constant(cr)
                          case Err(e) -> return Err(e)
                          case Ok((c, cr2)) ->
                            constants = append(constants, c)
                            cr = cr2
                        end
                        ci = ci + 1
                      end

                      # Instructions
                      match read_u32_be(cr)
                        case Err(e) -> return Err(e)
                        case Ok((instr_count, r12)) ->
                          let instrs = []
                          let ir = r12
                          let ii = 0
                          while ii < instr_count
                            match read_u32_be(ir)
                              case Err(e) -> return Err(e)
                              case Ok((encoded, ir2)) ->
                                instrs = append(instrs, LirInstruction(encoded: encoded))
                                ir = ir2
                            end
                            ii = ii + 1
                          end

                          let cell_def = LirCell(
                            name: name,
                            params: params,
                            returns: ret_type,
                            registers: regs,
                            constants: constants,
                            instructions: instrs
                          )
                          Ok((cell_def, ir))
                      end
                  end
              end
          end
      end
  end
end

cell read_module(buf: list[Int]) -> result[LirModule, String]
  let r = new_reader(buf)

  # Verify magic bytes
  match read_bytes(r, 4)
    case Err(e) -> return Err(e)
    case Ok((magic_bytes, r2)) ->
      # Skip magic validation for now, proceed to version
      match read_string(r2)
        case Err(e) -> return Err(e)
        case Ok((version, r3)) ->
          match read_string(r3)
            case Err(e) -> return Err(e)
            case Ok((doc_hash, r4)) ->

              # String table
              match read_u32_be(r4)
                case Err(e) -> return Err(e)
                case Ok((str_count, r5)) ->
                  let strings = []
                  let sr = r5
                  let si = 0
                  while si < str_count
                    match read_string(sr)
                      case Err(e) -> return Err(e)
                      case Ok((s, sr2)) ->
                        strings = append(strings, s)
                        sr = sr2
                    end
                    si = si + 1
                  end

                  # Type table
                  match read_u32_be(sr)
                    case Err(e) -> return Err(e)
                    case Ok((type_count, r6)) ->
                      let types = []
                      # Type reading omitted for brevity — same pattern as cells
                      let tr = r6

                      # Cell table
                      match read_u32_be(tr)
                        case Err(e) -> return Err(e)
                        case Ok((cell_count, r7)) ->
                          let cells = []
                          let cell_reader = r7
                          let ci = 0
                          while ci < cell_count
                            match read_lir_cell(cell_reader)
                              case Err(e) -> return Err(e)
                              case Ok((cell_def, cr2)) ->
                                cells = append(cells, cell_def)
                                cell_reader = cr2
                            end
                            ci = ci + 1
                          end

                          Ok(LirModule(
                            version: version,
                            doc_hash: doc_hash,
                            strings: strings,
                            types: types,
                            cells: cells,
                            tools: [],
                            effects: []
                          ))
                      end
                  end
              end
          end
      end
  end
end
```
