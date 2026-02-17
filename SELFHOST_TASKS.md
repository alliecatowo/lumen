# SELFHOST_TASKS.md — Lumen Phase 9: Self-Hosting Execution Plan

> **Goal**: Port the Lumen compiler from Rust to Lumen, achieving full self-hosting.
> The self-hosted compiler must produce bit-identical LIR output to the Rust compiler,
> and benchmark competitively against Go, Rust, Swift, Zig, and TypeScript.
>
> **Branch**: `self-hosting/phase-9`
> **Started**: 2026-02-17
> **Baseline**: v0.5.0 — 5,357 tests, 0 clippy warnings, 195+ tasks done

---

## Phase 0: Foundation & Infrastructure (S001–S050)

### S001–S010: Benchmark Infrastructure
| # | Task | Description |
|---|------|-------------|
| S001 | Benchmark harness crate | Create `rust/lumen-bench/` with criterion-based benchmarks for the Rust compiler (lex, parse, typecheck, lower, full compile) |
| S002 | Benchmark corpus | Create `bench/corpus/` with 10 Lumen programs of varying complexity (tiny, small, medium, large, huge) for consistent measurement |
| S003 | Cross-language benchmark suite | Create `bench/cross-language/` with equivalent programs in Go, Rust, Python, TypeScript, Zig — fibonacci, JSON parse, string manipulation, tree traversal, sort |
| S004 | Benchmark runner script | Create `bench/run_all.sh` that compiles and benchmarks all languages, outputs CSV |
| S005 | Rust compiler baseline metrics | Record baseline: lines/sec lexing, lines/sec parsing, lines/sec full compile for the Rust compiler |
| S006 | Memory profiling infrastructure | Add peak RSS and allocation count tracking to benchmark harness |
| S007 | Benchmark CI integration | Add benchmark comparison to CI — fail if >10% regression |
| S008 | Compile-time benchmark | Measure time to compile each of the 30 examples end-to-end |
| S009 | Runtime benchmark suite | Measure execution speed of compiled Lumen programs vs equivalent in other languages |
| S010 | Benchmark dashboard | Generate markdown report comparing Rust-Lumen, Lumen-Lumen, Go, Rust, Swift, Zig, TypeScript |

### S011–S020: std.compiler Library Foundation
| # | Task | Description |
|---|------|-------------|
| S011 | Create `stdlib/std/compiler/` directory | Set up the standard library module structure for compiler types |
| S012 | Span type | `record Span(file: String, start: Int, end: Int, start_line: Int, start_col: Int)` |
| S013 | Source type | `record Source(filename: String, content: String, lines: list[Int])` with line offset table |
| S014 | Diagnostic types | `enum DiagnosticLevel { Error, Warning, Note, Help }`, `record Diagnostic(level, message, span, notes)` |
| S015 | Diagnostic formatter | Cell that renders diagnostics with source context, carets, line numbers |
| S016 | Token types | Port all 142 TokenKind variants to a Lumen enum |
| S017 | AST node types — Expr | Port all 45 Expr variants to Lumen enums/records |
| S018 | AST node types — Stmt | Port all 19 Stmt variants |
| S019 | AST node types — Pattern | Port all 11 Pattern variants |
| S020 | AST node types — Item | Port all 17 Item variants |

### S021–S030: std.compiler Library (continued)
| # | Task | Description |
|---|------|-------------|
| S021 | AST node types — TypeExpr | Port all 10 TypeExpr variants |
| S022 | AST node types — BinOp/UnaryOp/CompoundOp | Port operator enums (25 + 3 + 10 variants) |
| S023 | AST supporting types | Port RecordDef, EnumDef, CellDef, Param, FieldDef, EnumVariant, GenericParam, etc. (~54 types) |
| S024 | AST Program type | `record Program(items, directives, doc_blocks)` as root node |
| S025 | LIR types — OpCode enum | Port all 70 OpCode variants |
| S026 | LIR types — IntrinsicId enum | Port all 86 IntrinsicId variants |
| S027 | LIR types — Instruction | Port 32-bit instruction encoding/decoding (ABC, ABx, sAx modes) |
| S028 | LIR types — LirModule | Port LirModule, LirCell, LirType, LirField, LirVariant, etc. (16 types) |
| S029 | LIR types — Constant | Port Constant enum (Null, Bool, Int, BigInt, Float, String) |
| S030 | Type system types | Port Type enum (19 variants), TypeError enum (10 variants) |

### S031–S040: Runtime Support & ABI
| # | Task | Description |
|---|------|-------------|
| S031 | ResolveError types | Port all 26 ResolveError variants |
| S032 | ParseError types | Port all 7 ParseError variants |
| S033 | LexError types | Port all 7 LexError variants |
| S034 | CompileError union type | Port CompileError as union of Lex/Parse/Resolve/Type/Constraint errors |
| S035 | SymbolTable type | Port SymbolTable with 14 symbol categories (types, cells, tools, effects, etc.) |
| S036 | TypeInfo/CellInfo/ToolInfo types | Port all symbol info structures |
| S037 | Freeze LIR bytecode ABI | Document and version-lock the instruction format, opcode numbers, intrinsic IDs |
| S038 | Binary serialization helpers | Implement LIR binary writer in Lumen (pack instructions to bytes, write constant pool) |
| S039 | Binary deserialization helpers | Implement LIR binary reader for loading compiled modules |
| S040 | String interning | Implement a string interner in Lumen (map from string to index, dedup) |

### S041–S050: Build System & Testing
| # | Task | Description |
|---|------|-------------|
| S041 | Self-host project structure | Create `self-host/` directory with `main.lm.md`, module layout mirroring compiler pipeline |
| S042 | Differential test harness | Create test framework: compile same source with Rust compiler and Lumen compiler, compare LIR output |
| S043 | Test corpus — trivial programs | 20 minimal programs (literals, arithmetic, hello world) |
| S044 | Test corpus — expressions | 30 programs covering all 45 Expr variants |
| S045 | Test corpus — statements | 20 programs covering all 19 Stmt variants |
| S046 | Test corpus — patterns | 15 programs covering all 11 Pattern variants |
| S047 | Test corpus — items | 20 programs covering all 17 Item variants |
| S048 | Test corpus — complex programs | 10 real-world programs from `examples/` |
| S049 | Hybrid compiler CLI flag | Add `--use-lumen-frontend` flag to `lumen` CLI for staged bootstrap |
| S050 | Self-host CI pipeline | CI step that runs differential tests on every commit |

---

## Phase 1: Lexer (S051–S090)

### S051–S060: Core Lexer
| # | Task | Description |
|---|------|-------------|
| S051 | Lexer module structure | Create `self-host/lexer.lm.md` with Lexer record and public API |
| S052 | Character stream | Implement character-by-character iteration with peek, advance, position tracking |
| S053 | Whitespace and newline handling | Skip whitespace, track line/column numbers |
| S054 | Single-line comments | Lex `#` comments through end of line |
| S055 | Identifier lexing | Lex identifiers `[a-zA-Z_][a-zA-Z0-9_]*`, check against keyword table |
| S056 | Keyword recognition | Map all 66 keywords to their TokenKind variants |
| S057 | Integer literals | Lex decimal integers with optional `_` separators |
| S058 | Hex/binary/octal literals | Lex `0x`, `0b`, `0o` prefixed integers |
| S059 | Float literals | Lex floats with decimal point and optional scientific notation `1.5e10` |
| S060 | BigInt literals | Lex integers with `n` suffix |

### S061–S070: String and Complex Tokens
| # | Task | Description |
|---|------|-------------|
| S061 | Simple string literals | Lex double-quoted strings with basic escape sequences `\n`, `\t`, `\\`, `\"` |
| S062 | String interpolation | Lex `{expr}` inside strings, tracking brace depth |
| S063 | Format specifiers in interpolation | Lex `{value:>10.2f}` format specs (align, fill, width, precision, type) |
| S064 | Triple-quoted strings | Lex `"""..."""` with automatic dedent |
| S065 | Raw strings | Lex `r"..."` without escape processing |
| S066 | Bytes literals | Lex `b"..."` byte string literals |
| S067 | Unicode escapes | Lex `\u{XXXX}` unicode escape sequences |
| S068 | Operator lexing — single char | Lex `+`, `-`, `*`, `/`, `%`, `=`, `<`, `>`, `!`, `&`, `|`, `^`, `~`, `.`, `,`, `:`, `;` |
| S069 | Operator lexing — multi char | Lex `==`, `!=`, `<=`, `>=`, `->`, `=>`, `|>`, `~>`, `..`, `..=`, `//`, `**`, `<<`, `>>`, `<=>`, `??`, `?.`, `?[` |
| S070 | Compound assignment operators | Lex `+=`, `-=`, `*=`, `/=`, `//=`, `%=`, `**=`, `&=`, `\|=`, `^=` |

### S071–S080: Delimiters and Special Tokens
| # | Task | Description |
|---|------|-------------|
| S071 | Delimiter tokens | Lex `(`, `)`, `[`, `]`, `{`, `}` |
| S072 | Directive tokens | Lex `@directive` tokens |
| S073 | Indentation tracking | Implement indent stack, emit Indent/Dedent tokens for significant whitespace |
| S074 | Markdown block handling | Lex triple-backtick blocks as MarkdownBlock tokens |
| S075 | Newline token emission | Emit Newline tokens where syntactically significant |
| S076 | EOF token | Emit EOF at end of input, drain remaining Dedents |
| S077 | Span tracking | Every token gets accurate Span (start, end, line, col) |
| S078 | Error recovery in lexer | On unexpected character, emit error token and continue |
| S079 | Lexer error collection | Accumulate LexErrors with spans, don't abort on first error |
| S080 | Lexer public API | `cell lex(source: String) -> result[list[Token], list[LexError]]` |

### S081–S090: Lexer Testing & Differential
| # | Task | Description |
|---|------|-------------|
| S081 | Lexer unit tests — literals | Test all literal types produce correct tokens |
| S082 | Lexer unit tests — operators | Test all operator tokens |
| S083 | Lexer unit tests — keywords | Test all 66 keywords recognized correctly |
| S084 | Lexer unit tests — strings | Test interpolation, escapes, triple-quoted, raw, bytes |
| S085 | Lexer unit tests — edge cases | Empty input, only whitespace, only comments, nested interpolation |
| S086 | Lexer unit tests — error cases | Unterminated strings, invalid numbers, unexpected chars |
| S087 | Lexer differential test | Compare Lumen lexer output vs Rust lexer on all 30 examples |
| S088 | Lexer performance benchmark | Measure tokens/sec, compare to Rust lexer baseline |
| S089 | Lexer integration with parser | Verify parser can consume Lumen lexer token stream |
| S090 | Lexer markdown extraction | Extract Lumen code blocks from `.lm.md` files before lexing |

---

## Phase 2: Parser (S091–S160)

### S091–S100: Parser Infrastructure
| # | Task | Description |
|---|------|-------------|
| S091 | Parser module structure | Create `self-host/parser.lm.md` with Parser record holding token stream, position, errors |
| S092 | Token stream navigation | Implement peek(), advance(), expect(), match_token(), at_end() |
| S093 | Error recording | Implement record_error() that adds ParseError with span, max 10 errors |
| S094 | Synchronization — top level | Implement synchronize() to skip to next `cell`, `record`, `enum`, `import`, etc. |
| S095 | Synchronization — statement | Implement synchronize_stmt() to skip to next statement boundary |
| S096 | Parse program | Top-level loop: parse directives, then items until EOF |
| S097 | Parse directives | Parse `@directive value` at file start |
| S098 | Generic parameter parsing | Parse `[T, U: Trait]` generic parameter lists |
| S099 | Effect row parsing | Parse `/ {effect1, effect2}` effect annotations |
| S100 | Type expression parsing | Parse all 10 TypeExpr variants (Named, List, Map, Result, Union, Null, Tuple, Set, Fn, Generic) |

### S101–S110: Item Parsing
| # | Task | Description |
|---|------|-------------|
| S101 | Parse record declaration | `record Name(field: Type, ...) where ... end` |
| S102 | Parse enum declaration | `enum Name Variant1, Variant2(payload: Type) end` |
| S103 | Parse cell declaration | `cell name(params) -> ReturnType / {effects} ... end` with body |
| S104 | Parse import declaration | `import path: *, import path: Name1, Name2 as Alias` |
| S105 | Parse type alias | `type Name = TypeExpr` |
| S106 | Parse trait declaration | `trait Name cell method(self, ...) -> Type end` |
| S107 | Parse impl declaration | `impl Trait for Type ... end` |
| S108 | Parse effect declaration | `effect Name operation(params) -> ReturnType end` |
| S109 | Parse handler declaration | `handler Name for EffectName ... end` |
| S110 | Parse use tool declaration | `use tool "name" as alias` |

### S111–S120: Item Parsing (continued)
| # | Task | Description |
|---|------|-------------|
| S111 | Parse grant declaration | `grant tool_name { constraints }` |
| S112 | Parse const declaration | `const NAME = value` |
| S113 | Parse agent declaration | `agent Name ... end` |
| S114 | Parse process declaration | `process Name(kind) ... end` — memory, machine, pipeline |
| S115 | Parse machine states | State declarations with transitions, guards, payloads |
| S116 | Parse pipeline stages | Stage declarations with type flow |
| S117 | Parse effect bind | `bind effect name to tool` |
| S118 | Parse addon declaration | `addon Name extends Base ... end` |
| S119 | Parse macro declaration | `macro name ... end` (currently stub — implement properly) |
| S120 | Parse extern declaration | `extern cell name(params) -> Type` |

### S121–S135: Expression Parsing (Pratt Parser)
| # | Task | Description |
|---|------|-------------|
| S121 | Pratt parser core | Implement parse_expr(min_precedence) with prefix + infix loop |
| S122 | Precedence table | Define precedence levels for all operators (16+ levels from assignment to postfix) |
| S123 | Prefix — literals | Parse IntLit, FloatLit, StringLit, BoolLit, NullLit, BytesLit, BigIntLit, RawStringLit |
| S124 | Prefix — string interpolation | Parse StringInterp with embedded expressions |
| S125 | Prefix — identifiers | Parse Ident, check for keyword-as-value (true, false, null) |
| S126 | Prefix — unary operators | Parse `-expr`, `not expr`, `~expr` |
| S127 | Prefix — grouped expression | Parse `(expr)` |
| S128 | Prefix — list literal | Parse `[expr, expr, ...]` |
| S129 | Prefix — map literal | Parse `{key: value, ...}` |
| S130 | Prefix — tuple literal | Parse `(expr, expr, ...)` — disambiguate from grouped expr |
| S131 | Prefix — set literal | Parse `{expr, expr, ...}` — disambiguate from map |
| S132 | Prefix — lambda | Parse `fn(params) -> expr` and `fn(params) ... end` |
| S133 | Prefix — if expression | Parse `if cond then expr else expr end` |
| S134 | Prefix — match expression | Parse `match expr ... end` with arms |
| S135 | Prefix — when expression | Parse `when cond -> expr ... end` |

### S136–S150: Expression Parsing (continued)
| # | Task | Description |
|---|------|-------------|
| S136 | Prefix — comptime expression | Parse `comptime ... end` |
| S137 | Prefix — perform expression | Parse `perform Effect.operation(args)` |
| S138 | Prefix — handle expression | Parse `handle body with Effect.op(params) -> resume(value) end` |
| S139 | Prefix — resume expression | Parse `resume(value)` |
| S140 | Prefix — record literal | Parse `RecordName(field: value, ...)` |
| S141 | Prefix — comprehension | Parse `[expr for x in coll if pred]` list/set/map comprehensions |
| S142 | Prefix — block expression | Parse `do ... end` block expressions |
| S143 | Prefix — await expression | Parse `await expr` |
| S144 | Infix — binary operators | Parse all 25 BinOp variants with correct precedence |
| S145 | Infix — dot access | Parse `expr.field` |
| S146 | Infix — index access | Parse `expr[index]` |
| S147 | Infix — function call | Parse `expr(args)` with named args support |
| S148 | Infix — pipe operator | Parse `expr |> func()` |
| S149 | Infix — compose operator | Parse `func1 ~> func2` |
| S150 | Infix — range expressions | Parse `expr..expr` and `expr..=expr` |

### S151–S155: Expression Parsing (postfix & special)
| # | Task | Description |
|---|------|-------------|
| S151 | Postfix — null coalesce | Parse `expr ?? default` |
| S152 | Postfix — null-safe access | Parse `expr?.field`, `expr?[index]` |
| S153 | Postfix — null assert | Parse `expr!` |
| S154 | Postfix — is/as type ops | Parse `expr is Type`, `expr as Type` |
| S155 | Postfix — try expression | Parse `try expr` and `try ... else ... end` |

### S156–S160: Statement Parsing
| # | Task | Description |
|---|------|-------------|
| S156 | Parse let statement | `let x = expr`, `let (a, b) = expr`, `let Point(x:, y:) = expr` |
| S157 | Parse if/while/loop/for statements | All control flow with labels (`@label`), for-in with filters |
| S158 | Parse match/return/halt/break/continue | Pattern matching, loop control, function exit |
| S159 | Parse assign/compound assign | `x = expr`, `x += expr`, `x.field = expr`, `x[i] = expr` |
| S160 | Parse defer/yield/emit | Scope cleanup, generator yield, event emission |

---

## Phase 2b: Parser Testing (S161–S175)

| # | Task | Description |
|---|------|-------------|
| S161 | Parser unit tests — items | Test parsing all 17 item types |
| S162 | Parser unit tests — expressions | Test all 45 expression variants |
| S163 | Parser unit tests — statements | Test all 19 statement variants |
| S164 | Parser unit tests — patterns | Test all 11 pattern variants |
| S165 | Parser unit tests — type expressions | Test all 10 TypeExpr variants |
| S166 | Parser unit tests — error recovery | Test that parser recovers from syntax errors and reports multiple |
| S167 | Parser unit tests — precedence | Test operator precedence (all 16+ levels) |
| S168 | Parser unit tests — edge cases | Empty programs, deeply nested, trailing commas, semicolons |
| S169 | Parser differential test | Compare AST output from Lumen parser vs Rust parser on all 30 examples |
| S170 | Parser performance benchmark | Measure nodes/sec, compare to Rust parser baseline |
| S171 | Parser integration — lex+parse | End-to-end: source string → tokens → AST |
| S172 | Parser — markdown-first files | Parse `.lm.md` files (extract then lex then parse) |
| S173 | Parser — raw `.lm` files | Parse raw source files |
| S174 | Parser — `.lumen` files | Parse markdown-native files |
| S175 | Parser — import declarations | Correctly parse and represent all import variants |

---

## Phase 3: Resolver (S176–S220)

### S176–S190: Symbol Resolution
| # | Task | Description |
|---|------|-------------|
| S176 | Resolver module structure | Create `self-host/resolver.lm.md` with Resolver record, scope stack |
| S177 | Scope management | Push/pop scopes, lookup symbols with innermost-first shadowing |
| S178 | Pass 1 — register definitions | Walk all items, register types/cells/tools/effects/agents/processes/traits/impls/consts |
| S179 | Pass 2 — verify references | Walk all expressions/statements, verify every name resolves |
| S180 | Type reference resolution | Resolve type names to TypeInfo, check generic arity |
| S181 | Cell reference resolution | Resolve function calls to CellInfo, check existence |
| S182 | Tool reference resolution | Resolve tool names, verify grants exist |
| S183 | Effect resolution | Infer effects from cell bodies, verify effect rows match grants |
| S184 | Effect provenance | Track *why* an effect is required (which call/tool introduced it) |
| S185 | Import resolution | Resolve `import module: symbols` by loading and compiling imported modules |
| S186 | Circular import detection | Track compilation stack, report chains like `a → b → c → a` |
| S187 | Machine state validation | Verify initial state exists, all transitions target valid states, terminal states exist |
| S188 | Pipeline stage validation | Verify stage arity, type flow between stages |
| S189 | Trait method validation | Verify impl methods match trait signatures |
| S190 | Grant policy resolution | Resolve grant constraints, build per-tool policy |

### S191–S200: Resolver (continued)
| # | Task | Description |
|---|------|-------------|
| S191 | Local definitions | Support `record`/`enum`/`cell` defined inside cell bodies |
| S192 | Deterministic mode checking | In `@deterministic true`, reject uuid, timestamp, random, unknown extern |
| S193 | Undefined type error | Emit ResolveError::UndefinedType with span and suggestion |
| S194 | Undefined cell error | Emit ResolveError::UndefinedCell with suggestion |
| S195 | Duplicate definition error | Emit ResolveError::Duplicate with both spans |
| S196 | Undeclared effect error | Emit with cause chain (e.g., "call to fetch which uses http") |
| S197 | Generic arity mismatch error | "Expected 2 type parameters, got 1" |
| S198 | Const evaluation | Evaluate `const` declarations at resolve time |
| S199 | Resolver unit tests | Test all 26 error variants, symbol resolution, scoping |
| S200 | Resolver differential test | Compare symbol tables from Lumen resolver vs Rust resolver |

---

## Phase 4: Type Checker (S201–S260)

### S201–S215: Core Type Checking
| # | Task | Description |
|---|------|-------------|
| S201 | Type checker module structure | Create `self-host/typecheck.lm.md` with TypeChecker record, type environment |
| S202 | Type environment | Map from variable name → Type, with scope stacking |
| S203 | Literal type inference | IntLit→Int, FloatLit→Float, StringLit→String, BoolLit→Bool, NullLit→Null |
| S204 | Variable type lookup | Look up variable type in environment, error if undefined |
| S205 | Binary operator typing | Type rules for all 25 BinOp variants |
| S206 | Unary operator typing | Type rules for Neg (Int→Int, Float→Float), Not (Bool→Bool), BitNot (Int→Int) |
| S207 | Function call typing | Resolve callee type, check arg count/types, return result type |
| S208 | Record literal typing | Check all fields present with correct types |
| S209 | List/Map/Set/Tuple literal typing | Infer element types, check homogeneity for lists/sets |
| S210 | Dot access typing | Look up field type on records, method type on processes |
| S211 | Index access typing | list[Int]→element, map[K]→V, tuple[Int]→element |
| S212 | If expression typing | Condition must be Bool, branches must unify |
| S213 | Match expression typing | Subject type drives pattern checking, all arms must unify |
| S214 | Lambda typing | Infer parameter types from context, infer return from body |
| S215 | Pipe operator desugaring | `x |> f(a)` → `f(x, a)` — check types through the chain |

### S216–S230: Advanced Type Checking
| # | Task | Description |
|---|------|-------------|
| S216 | Generic instantiation | Substitute type variables with concrete types at call sites |
| S217 | Generic unification | Unify generic types during inference (unify_for_inference) |
| S218 | Type narrowing | In `if x is Int then ... end`, narrow x to Int in then-branch |
| S219 | Union type handling | Compute join of two types (e.g., `Int | String`), subtype checking |
| S220 | Result type checking | `ok(v)` → Result[T, E], `err(e)` → Result[T, E], `try` unwrapping |
| S221 | Exhaustiveness checking — enums | Verify match covers all variants, wildcard makes exhaustive |
| S222 | Exhaustiveness checking — int ranges | Verify integer range patterns are complete |
| S223 | Guard pattern handling | Guards don't contribute to exhaustiveness |
| S224 | Pattern destructure typing | Tuple, record, list destructuring in let and match |
| S225 | Effect type checking | Verify effect rows in cell signatures, check perform/handle types |
| S226 | Builtin function typing | Return types for all 86+ builtins |
| S227 | Type alias expansion | Expand type aliases before comparison |
| S228 | Levenshtein suggestions | On undefined name, suggest similar names using edit distance |
| S229 | Type mismatch error | Clear error with expected vs actual, source location |
| S230 | Immutable assignment error | Error on assigning to non-mut variables |

### S231–S240: Ownership & Advanced Analysis
| # | Task | Description |
|---|------|-------------|
| S231 | Ownership checker port | Port ownership analysis from `ownership.rs` — track moves, borrows |
| S232 | Use-after-move detection | Error when using a variable after it has been moved |
| S233 | Typestate checker port | Port typestate analysis from `typestate.rs` |
| S234 | Session type checker port | Port session type analysis from `session.rs` |
| S235 | Constraint validation | Port where-clause checking from `constraints.rs` |
| S236 | Type checker unit tests — basic | Test literal inference, variable lookup, binary ops |
| S237 | Type checker unit tests — generics | Test generic instantiation, unification |
| S238 | Type checker unit tests — patterns | Test exhaustiveness, narrowing, destructuring |
| S239 | Type checker unit tests — effects | Test effect row checking, perform/handle |
| S240 | Type checker differential test | Compare types inferred by Lumen vs Rust checker |

---

## Phase 5: LIR Lowering (S241–S300)

### S241–S255: Register Allocator & Code Generation
| # | Task | Description |
|---|------|-------------|
| S241 | Lowerer module structure | Create `self-host/lower.lm.md` with Lowerer record, register allocator |
| S242 | Register allocator | Linear scan allocator: alloc_reg(), free_reg(), track live ranges |
| S243 | Constant pool management | Intern constants (Null, Bool, Int, Float, String), return indices |
| S244 | String table management | Intern strings, deduplicate, return indices |
| S245 | Label management | Create labels for jumps, resolve forward references |
| S246 | Instruction emission | Emit 32-bit instructions with ABC/ABx/sAx encoding |
| S247 | Lower literal expressions | IntLit→LoadInt/LoadK, FloatLit→LoadK, StringLit→LoadK, BoolLit→LoadBool, NullLit→LoadNil |
| S248 | Lower arithmetic expressions | BinOp→Add/Sub/Mul/Div/Mod/Pow/FloorDiv |
| S249 | Lower comparison expressions | BinOp→Eq/Lt/Le + Not for !=, >, >= |
| S250 | Lower logical expressions | And/Or with short-circuit (Test+Jmp) |
| S251 | Lower bitwise expressions | BitAnd/BitOr/BitXor/BitNot/Shl/Shr |
| S252 | Lower unary expressions | Neg, Not, BitNot |
| S253 | Lower string concatenation | Concat opcode |
| S254 | Lower variable access | Move from variable register |
| S255 | Lower assignment | Move to variable register, compound assignment desugaring |

### S256–S270: Complex Expression Lowering
| # | Task | Description |
|---|------|-------------|
| S256 | Lower function calls | Call opcode with arg setup, handle return value |
| S257 | Lower method calls (dot) | GetField + Call |
| S258 | Lower tool calls | ToolCall opcode |
| S259 | Lower record construction | NewRecord + SetField for each field |
| S260 | Lower list/map/set/tuple construction | NewList/NewMap/NewSet/NewTuple + Append/SetField |
| S261 | Lower index access | GetIndex |
| S262 | Lower field access | GetField |
| S263 | Lower pipe operator | Desugar `x |> f(a)` → `f(x, a)`, then lower the call |
| S264 | Lower range expressions | Inclusive range emits `end + 1` via Add |
| S265 | Lower string interpolation | Concatenate segments with format spec handling |
| S266 | Lower lambda/closure | Lift to module-level cell, emit Closure opcode with upvalues |
| S267 | Lower if expressions | Test + Jmp with then/else branches, merge result |
| S268 | Lower match expressions | Pattern dispatch: literal→Eq+Test+Jmp, variant→IsVariant+Jmp, destructure→GetField/GetIndex |
| S269 | Lower when expressions | Chain of Test+Jmp for each arm |
| S270 | Lower comprehensions | Loop with accumulator, filter as Test+Jmp |

### S271–S285: Statement & Control Flow Lowering
| # | Task | Description |
|---|------|-------------|
| S271 | Lower let statements | Evaluate init, bind to register, handle destructuring |
| S272 | Lower if statements | Test + Jmp, lower then/else blocks |
| S273 | Lower while loops | Loop opcode, Test + Jmp for condition, Break/Continue patching |
| S274 | Lower for-in loops | ForPrep/ForLoop/ForIn opcodes |
| S275 | Lower loop statements | Infinite loop with Break exit |
| S276 | Lower break/continue | Jmp to loop exit/start, support labeled loops |
| S277 | Lower return statements | Return opcode, evaluate return value |
| S278 | Lower defer statements | Emit defer body before every return/halt in scope (LIFO order) |
| S279 | Lower yield statements | Emit Yield opcode for generator cells |
| S280 | Lower emit statements | Emit opcode for event emission |
| S281 | Lower try expressions | Evaluate, branch on ok/err |
| S282 | Lower null coalesce | Evaluate, test for null, branch to default |
| S283 | Lower perform/handle/resume | Perform/HandlePush/HandlePop/Resume opcodes |
| S284 | Lower await expressions | Await opcode |
| S285 | Lower comptime expressions | Evaluate at compile time via try_const_eval |

### S286–S300: Module-Level Lowering & Testing
| # | Task | Description |
|---|------|-------------|
| S286 | Lower cell definitions | Create LirCell with params, body instructions, locals |
| S287 | Lower record definitions | Create LirType with fields |
| S288 | Lower enum definitions | Create LirType with variants |
| S289 | Lower process definitions | Machine states, pipeline stages → specialized cells |
| S290 | Lower effect definitions | Create LirEffect with operations |
| S291 | Lower import merging | Merge imported LirModules (dedup strings, append cells/types) |
| S292 | Tail call optimization | Detect tail position calls, emit TailCall instead of Call |
| S293 | Implicit return | If cell body ends with expression, emit Return for it |
| S294 | Variadic argument packing | Pack variadic args into list |
| S295 | Lowering unit tests — literals | Test all literal types produce correct LIR |
| S296 | Lowering unit tests — control flow | Test if/while/for/loop/break/continue produce correct jumps |
| S297 | Lowering unit tests — patterns | Test match arm dispatch for all pattern types |
| S298 | Lowering unit tests — closures | Test lambda lifting and upvalue capture |
| S299 | Lowering differential test | Compare LIR output from Lumen lowerer vs Rust lowerer on corpus |
| S300 | Lowering performance benchmark | Measure instructions emitted/sec |

---

## Phase 6: Diagnostics & Error Formatting (S301–S320)

| # | Task | Description |
|---|------|-------------|
| S301 | Diagnostic renderer | Port error formatting: source line context, underline carets, colors |
| S302 | Type diff display | Show expected vs actual types with alignment |
| S303 | Similar name suggestions | Levenshtein distance for "did you mean?" suggestions |
| S304 | Import path suggestions | When import fails, suggest similar module paths |
| S305 | Interpolation span mapping | Map spans within string interpolation to original source |
| S306 | Multi-error output | Format and display all errors from all compiler phases |
| S307 | Warning output | Format warnings (unused variables, unused imports) |
| S308 | Note/help attachments | Attach notes and help text to primary diagnostics |
| S309 | JSON diagnostic output | Machine-readable error format for IDE integration |
| S310 | SARIF output | SARIF format for CI integration |
| S311 | Diagnostic tests — lex errors | Verify formatting for all 7 LexError variants |
| S312 | Diagnostic tests — parse errors | Verify formatting for all 7 ParseError variants |
| S313 | Diagnostic tests — resolve errors | Verify formatting for all 26 ResolveError variants |
| S314 | Diagnostic tests — type errors | Verify formatting for all 10 TypeError variants |
| S315 | Color and style | ANSI color codes for terminal output (red errors, yellow warnings, cyan notes) |
| S316 | Source line caching | Cache source lines for efficient multi-error display |
| S317 | Unicode-aware column display | Handle multi-byte characters in source when placing carets |
| S318 | Error code catalog | Assign error codes (E001, E002, ...) for documentation cross-referencing |
| S319 | Truncation for long lines | Truncate very long source lines in diagnostic display |
| S320 | Diagnostic unit tests | Comprehensive tests for all formatting features |

---

## Phase 7: Markdown Extraction (S321–S335)

| # | Task | Description |
|---|------|-------------|
| S321 | Markdown extractor module | Port markdown code block extraction logic |
| S322 | Fenced block detection | Find ` ``` lumen ` blocks in `.lm.md` files |
| S323 | Directive extraction | Parse `@directive value` from markdown |
| S324 | Block concatenation | Concatenate extracted code blocks with proper line mapping |
| S325 | Source map | Map line numbers in extracted code back to original `.lm.md` line numbers |
| S326 | Raw `.lm` passthrough | Skip extraction for raw source files |
| S327 | `.lumen` format handling | Handle markdown-native format |
| S328 | Docstring extraction | Extract markdown blocks preceding declarations as docstrings |
| S329 | Extraction tests | Test on all 30 examples, verify correct code extraction |
| S330 | Round-trip verification | Extract → compile → verify spans map back correctly |
| S331 | Nested code blocks | Handle code blocks inside markdown list items or blockquotes |
| S332 | Language tag variants | Accept `lumen`, `lm`, `Lumen` language tags |
| S333 | Non-Lumen blocks | Skip blocks with other language tags (json, bash, etc.) |
| S334 | Empty block handling | Handle empty code blocks gracefully |
| S335 | Extraction performance | Benchmark extraction speed on large markdown files |

---

## Phase 8: Integration & Pipeline (S336–S370)

### S336–S350: Compiler Pipeline
| # | Task | Description |
|---|------|-------------|
| S336 | Compile pipeline | Wire together: extract → lex → parse → resolve → typecheck → lower → emit |
| S337 | compile() entry point | `cell compile(source: String) -> result[LirModule, CompileError]` |
| S338 | compile_with_imports() | Support multi-file compilation with import resolution |
| S339 | compile_with_options() | Support CompileOptions (OwnershipCheckMode, etc.) |
| S340 | format_error() | Human-readable error formatting with source context |
| S341 | Module resolver | Find `.lm.md`, `.lm`, `.lumen` files on disk for imports |
| S342 | Recursive compilation | Compile imported modules, merge into main module |
| S343 | Compilation caching | Cache compiled modules by content hash to avoid recompilation |
| S344 | LIR binary serialization | Write LirModule to binary format the VM can load |
| S345 | LIR binary deserialization | Read compiled modules from binary |
| S346 | Pipeline unit tests | Test full compile pipeline on corpus |
| S347 | Pipeline error tests | Test that errors from each phase are properly surfaced |
| S348 | Pipeline performance | Benchmark full compile pipeline vs Rust compiler |
| S349 | Multi-file compilation test | Test imports, circular detection, symbol merging |
| S350 | Deterministic output | Verify same input always produces bit-identical LIR |

### S351–S370: CLI & Tooling Integration
| # | Task | Description |
|---|------|-------------|
| S351 | Self-hosted `check` command | `lumen check <file>` using Lumen compiler |
| S352 | Self-hosted `run` command | `lumen run <file>` using Lumen compiler + VM |
| S353 | Self-hosted `emit` command | `lumen emit <file>` outputting LIR JSON |
| S354 | Self-hosted `fmt` command | Port formatter to Lumen (print AST back to source) |
| S355 | Self-hosted REPL | Port REPL with Lumen compiler backend |
| S356 | Self-hosted `lang-ref` | Generate language reference from compiler data |
| S357 | Module resolver — CLI | Wire module resolver to CLI commands |
| S358 | Error output formatting — CLI | Wire diagnostics to CLI output |
| S359 | --trace-dir support | Trace recording in self-hosted compiler |
| S360 | --cell flag support | Run specific cell from compiled module |
| S361 | Exit code correctness | Return 0 on success, 1 on compile error, 2 on runtime error |
| S362 | Stdin compilation | Support reading source from stdin |
| S363 | Multiple file compilation | `lumen check file1.lm file2.lm` |
| S364 | Verbose/quiet modes | Control diagnostic verbosity |
| S365 | Timing output | `--time` flag showing phase durations |
| S366 | CLI differential test | Same source, same flags → same output from Rust vs Lumen CLI |
| S367 | CLI integration tests | Full CLI tests for check/run/emit/fmt |
| S368 | Self-hosted package build | `lumen pkg build` using Lumen compiler |
| S369 | Self-hosted workspace build | Workspace resolver using Lumen compiler |
| S370 | CLI performance benchmark | Compare CLI response time Rust vs Lumen |

---

## Phase 9: Bootstrap Loop (S371–S400)

### S371–S385: The Ouroboros
| # | Task | Description |
|---|------|-------------|
| S371 | Stage 1 compile | Use Rust compiler to compile `self-host/*.lm.md` → self-host-v1 binary |
| S372 | Stage 1 verify | Run self-host-v1 on test corpus, compare output to Rust compiler |
| S373 | Stage 2 compile | Use self-host-v1 to compile `self-host/*.lm.md` → self-host-v2 binary |
| S374 | Binary comparison | Compare hash of v1 and v2 — must match for deterministic bootstrap |
| S375 | Bootstrap CI pipeline | CI job that runs the full bootstrap loop on every commit |
| S376 | Bootstrap performance tracking | Track compilation time at each bootstrap stage |
| S377 | Triple bootstrap | v1 → v2 → v3, compare v2 and v3 (proves stability) |
| S378 | Regression test suite | Suite of programs that must compile identically in both compilers |
| S379 | Error message parity | Same source error → same diagnostic output from both compilers |
| S380 | Edge case corpus | Programs designed to stress test unusual compiler paths |
| S381 | Unicode source test | Full Unicode support in self-hosted lexer (identifiers, strings) |
| S382 | Large file test | Compile a 10,000+ line Lumen file, verify correctness and timing |
| S383 | Many-file test | Compile a project with 50+ modules, verify import resolution |
| S384 | Adversarial input test | Deeply nested expressions, pathologically long lines, huge string literals |
| S385 | Memory usage comparison | Compare peak RSS of Rust compiler vs Lumen compiler |

### S386–S400: Optimization & Hardening
| # | Task | Description |
|---|------|-------------|
| S386 | Profile self-hosted compiler | Find hotspots: lexing, parsing, type checking, lowering |
| S387 | Optimize hot paths | Apply algorithmic improvements to slowest phases |
| S388 | String allocation optimization | Reduce unnecessary string copies in lexer/parser |
| S389 | Map/lookup optimization | Optimize symbol table lookups (consider sorted arrays vs maps) |
| S390 | Instruction emission optimization | Batch instruction writes, reduce allocations |
| S391 | Constant folding in self-host | Ensure comptime evaluation handles all cases |
| S392 | Dead code elimination | Don't emit code for unreachable branches |
| S393 | Register allocation improvement | Reduce register spills in generated LIR |
| S394 | Inline small cells | Inline trivial helper cells to reduce call overhead |
| S395 | Compile-time string interning | Deduplicate strings at compile time more aggressively |
| S396 | Error path optimization | Avoid allocations on error paths, use pre-allocated buffers |
| S397 | Lazy import compilation | Only compile imported modules when needed |
| S398 | Incremental compilation design | Design (not implement) incremental compilation for future |
| S399 | Self-host stability soak test | Run self-host compiler on all 30 examples + all test corpus for 24 hours |
| S400 | Self-host sign-off | Final human review: correctness, performance, error quality |

---

## Phase 10: Cross-Language Benchmarks (S401–S440)

### S401–S415: Benchmark Programs
| # | Task | Description |
|---|------|-------------|
| S401 | Fibonacci benchmark | Recursive fib(35) in Lumen, Go, Rust, Python, TS, Zig, Swift, C |
| S402 | Matrix multiplication benchmark | 500x500 matrix multiply in all languages |
| S403 | JSON parse/generate benchmark | Parse 1MB JSON, modify, re-serialize in all languages |
| S404 | String manipulation benchmark | 1M string concatenations, splits, replaces in all languages |
| S405 | Binary tree benchmark | Build and traverse balanced binary tree (depth 20) in all languages |
| S406 | Hash map benchmark | 1M insert/lookup/delete on hash map in all languages |
| S407 | Sort benchmark | Sort 1M integers (quicksort) in all languages |
| S408 | Regex/pattern matching benchmark | Match 100K strings against complex patterns in all languages |
| S409 | File I/O benchmark | Read/write 100MB file in all languages |
| S410 | HTTP server benchmark | Simple HTTP server, 10K requests in all languages |
| S411 | Compilation speed benchmark | Compile a 5000-line program: Rust-Lumen, Lumen-Lumen, Go, Rust, Zig, Swift |
| S412 | Startup time benchmark | Time from process start to first output in all languages |
| S413 | Memory usage benchmark | Peak RSS for each benchmark program in all languages |
| S414 | Concurrent task benchmark | Spawn 10K concurrent tasks, join all in all languages |
| S415 | AI inference benchmark | LLM tool call round-trip in Lumen vs Python vs TypeScript |

### S416–S430: Benchmark Infrastructure
| # | Task | Description |
|---|------|-------------|
| S416 | Benchmark runner | Unified script that runs all benchmarks across all languages |
| S417 | Statistical analysis | Run each benchmark N times, compute mean, stddev, min, max |
| S418 | Benchmark result storage | Store results in JSON for historical comparison |
| S419 | Benchmark visualization | Generate charts (bar charts, line graphs) as SVG or HTML |
| S420 | Benchmark regression detection | Alert if any benchmark regresses >5% from baseline |
| S421 | Benchmark CI integration | Run benchmarks in CI, track trends |
| S422 | Language version tracking | Record exact compiler/runtime versions for each language |
| S423 | Hardware normalization | Normalize results by CPU speed for cross-machine comparison |
| S424 | Warm-up handling | JIT warm-up for Go/Lumen, AOT advantage for Rust/Zig/C |
| S425 | Benchmark corpus management | Version-controlled benchmark source files |
| S426 | Benchmark report — Rust-Lumen vs Lumen-Lumen | Side-by-side comparison of both compiler implementations |
| S427 | Benchmark report — vs Go | Lumen vs Go compilation speed, runtime performance, memory |
| S428 | Benchmark report — vs Rust | Lumen vs Rust compilation speed, runtime performance, memory |
| S429 | Benchmark report — vs TypeScript | Lumen vs TS compilation speed, runtime performance |
| S430 | Benchmark report — overall | Combined report ranking all languages across all benchmarks |

### S431–S440: Performance Targets & Optimization
| # | Task | Description |
|---|------|-------------|
| S431 | Target: within 2x of Rust runtime | Optimize until Lumen is within 2x of Rust on compute benchmarks |
| S432 | Target: beat Python by 10x+ | Verify Lumen dramatically outperforms Python on all benchmarks |
| S433 | Target: match or beat Go runtime | Optimize until Lumen matches Go on concurrent/GC-heavy benchmarks |
| S434 | Target: beat TypeScript by 5x+ | Verify Lumen outperforms TS/Node on all benchmarks |
| S435 | Target: compilation speed within 3x of Go | Optimize Lumen-Lumen compiler to compile fast |
| S436 | Target: Rust-Lumen compilation within 1.5x of Go | The Rust compiler should be very fast |
| S437 | Target: memory within 2x of Rust | Optimize memory usage |
| S438 | Target: startup under 50ms | Fast startup for CLI tools |
| S439 | Benchmark-driven optimization | Identify and optimize specific hotspots revealed by benchmarks |
| S440 | Final benchmark report | Publishable benchmark report for website/blog |

---

## Phase 11: VitePress Documentation (S441–S475)

| # | Task | Description |
|---|------|-------------|
| S441 | Update homepage hero | Update hero copy to reflect v0.5.0 capabilities, self-hosting status |
| S442 | Update feature cards | Add cards for verification, effects, self-hosting, benchmarks |
| S443 | Add benchmark page | New page with interactive benchmark charts |
| S444 | Add self-hosting page | Documentation of the bootstrap process |
| S445 | Expand examples — 30 total | Port all 30 `examples/*.lm.md` to the Examples sidebar section |
| S446 | Auto-generate builtin docs | Script that generates `api/builtins.md` from VM intrinsics |
| S447 | Add API section depth | Separate pages for string, list, map, math, I/O builtins |
| S448 | Update language reference | Reflect all v0.5.0 features: effects, ownership, session types, macros |
| S449 | Add type system reference | Detailed page on generics, refinement types, Prob<T>, GADTs |
| S450 | Add effects tutorial | Step-by-step guide to algebraic effects in Lumen |
| S451 | Add verification guide | How to use proof hints, where clauses, SMT integration |
| S452 | Add process tutorial | Memory, machine, pipeline process types with examples |
| S453 | Add concurrency guide | Channels, actors, supervisors, nurseries, parallel combinators |
| S454 | Add durability guide | Checkpoint, replay, time-travel debugging |
| S455 | Fix orphaned ALLCAPS docs | Integrate or redirect 10 orphaned legacy docs into sidebar |
| S456 | Fix reference/grammar link | Ensure grammar page exists and links correctly |
| S457 | Add search enhancement | Improve search with better indexing of code examples |
| S458 | Update playground | Add more preset examples, improve error display |
| S459 | Add dark/light code themes | Ensure code blocks look good in both themes |
| S460 | Add copy-to-clipboard | Copy button on all code blocks |
| S461 | Add architecture page | Visual architecture diagram with mermaid |
| S462 | Add contributing guide | How to contribute to Lumen |
| S463 | Add changelog | v0.5.0 changelog with all new features |
| S464 | Add FAQ page | Common questions and answers |
| S465 | Add comparison page | Lumen vs Rust/Go/Python/TS feature comparison table |
| S466 | Update sidebar navigation | Reorganize for new content |
| S467 | Add breadcrumbs | Navigation breadcrumbs on all pages |
| S468 | Performance optimization | Optimize page load, lazy-load playground |
| S469 | Mobile responsiveness | Test and fix mobile layout issues |
| S470 | SEO optimization | Meta tags, structured data, sitemap |
| S471 | Add RSS feed | For blog/changelog updates |
| S472 | Add version selector | Toggle between v0.4.0 and v0.5.0 docs |
| S473 | WASM playground update | Update to compile with v0.5.0 features |
| S474 | Add interactive tutorials | Step-by-step tutorials with inline execution |
| S475 | Deploy and verify | Build, deploy to GitHub Pages, verify all links work |

---

## Phase 12: Language Gaps Discovered During Self-Hosting (S476–S500)

> These tasks will be filled in as we encounter issues during self-hosting.
> When the self-hosted compiler can't express something the Rust compiler does,
> we add a task here and implement the missing feature.

| # | Task | Description |
|---|------|-------------|
| S476 | *Reserved — discovered gap 1* | |
| S477 | *Reserved — discovered gap 2* | |
| S478 | *Reserved — discovered gap 3* | |
| S479 | *Reserved — discovered gap 4* | |
| S480 | *Reserved — discovered gap 5* | |
| S481 | *Reserved — discovered gap 6* | |
| S482 | *Reserved — discovered gap 7* | |
| S483 | *Reserved — discovered gap 8* | |
| S484 | *Reserved — discovered gap 9* | |
| S485 | *Reserved — discovered gap 10* | |
| S486 | *Reserved — discovered gap 11* | |
| S487 | *Reserved — discovered gap 12* | |
| S488 | *Reserved — discovered gap 13* | |
| S489 | *Reserved — discovered gap 14* | |
| S490 | *Reserved — discovered gap 15* | |
| S491 | *Reserved — discovered gap 16* | |
| S492 | *Reserved — discovered gap 17* | |
| S493 | *Reserved — discovered gap 18* | |
| S494 | *Reserved — discovered gap 19* | |
| S495 | *Reserved — discovered gap 20* | |
| S496 | *Reserved — discovered gap 21* | |
| S497 | *Reserved — discovered gap 22* | |
| S498 | *Reserved — discovered gap 23* | |
| S499 | *Reserved — discovered gap 24* | |
| S500 | *Reserved — discovered gap 25* | |

---

## Summary

| Phase | Tasks | Description |
|-------|-------|-------------|
| Phase 0 | S001–S050 | Foundation: benchmarks, std.compiler, LIR ABI, build system |
| Phase 1 | S051–S090 | Self-hosted Lexer (40 tasks) |
| Phase 2 | S091–S175 | Self-hosted Parser (85 tasks) |
| Phase 3 | S176–S200 | Self-hosted Resolver (25 tasks) |
| Phase 4 | S201–S240 | Self-hosted Type Checker (40 tasks) |
| Phase 5 | S241–S300 | Self-hosted LIR Lowering (60 tasks) |
| Phase 6 | S301–S320 | Diagnostics & Error Formatting (20 tasks) |
| Phase 7 | S321–S335 | Markdown Extraction (15 tasks) |
| Phase 8 | S336–S370 | Integration & CLI (35 tasks) |
| Phase 9 | S371–S400 | Bootstrap Loop (30 tasks) |
| Phase 10 | S401–S440 | Cross-Language Benchmarks (40 tasks) |
| Phase 11 | S441–S475 | VitePress Documentation (35 tasks) |
| Phase 12 | S476–S500 | Reserved for Discovered Gaps (25 slots) |
| **Total** | **500** | |
