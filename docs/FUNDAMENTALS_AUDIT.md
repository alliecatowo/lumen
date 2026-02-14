# Lumen Language Fundamentals Audit

Comprehensive gap analysis comparing Lumen's implemented features against modern language standards.
Assessed by reading the actual compiler source (tokens, AST, parser, LIR opcodes, VM).

**Legend:**
- **[IMPLEMENTED]** -- already in the language and working
- **[PARTIALLY]** -- started but incomplete
- **[MISSING - HIGH PRIORITY]** -- should add; common in modern languages
- **[MISSING - MEDIUM]** -- nice to have
- **[MISSING - LOW/SKIP]** -- doesn't fit Lumen's philosophy

---

## 1. OPERATORS

### Null/Optional Operators

| Feature | Status | Notes |
|---------|--------|-------|
| Null-safe access `?.` | **[IMPLEMENTED]** | `Expr::NullSafeAccess`, `QuestionDot` token, works end-to-end |
| Null coalescing `??` | **[IMPLEMENTED]** | `Expr::NullCoalesce`, `QuestionQuestion` token, `NullCo` opcode |
| Null assertion `!` | **[IMPLEMENTED]** | `Expr::NullAssert`, postfix `Bang` token |
| Null-safe index `?[]` | **[MISSING - MEDIUM]** | No `?[` token or AST node. Only `?.field` exists, not `?[index]`. Would need new postfix handling in parser. |
| Logical nullish assignment `??=` | **[MISSING - LOW/SKIP]** | Compound assignments exist for `+= -= *= /=` but no `??=`. Uncommon outside JS/TS. |

### Arithmetic Operators

| Feature | Status | Notes |
|---------|--------|-------|
| Exponentiation `**` | **[IMPLEMENTED]** | `StarStar` token, `BinOp::Pow`, right-associative, `OpCode::Pow` |
| Floor division `//` | **[MISSING - MEDIUM]** | No token or opcode. Python/Ruby have this. Achievable via `floor(a / b)` intrinsic. |
| Modulo `%` | **[IMPLEMENTED]** | `Percent` token, `BinOp::Mod`, `OpCode::Mod` |

### Bitwise Operators

| Feature | Status | Notes |
|---------|--------|-------|
| Bitwise AND `&` | **[IMPLEMENTED]** | `Ampersand` token, `BinOp::BitAnd`, `OpCode::BitAnd` |
| Bitwise OR `\|` | **[IMPLEMENTED]** | `Pipe` token, `BinOp::BitOr`, `OpCode::BitOr` |
| Bitwise XOR `^` | **[IMPLEMENTED]** | `Caret` token, `BinOp::BitXor`, `OpCode::BitXor` |
| Bitwise NOT `~` | **[IMPLEMENTED]** | `Tilde` token, `UnaryOp::BitNot`, `OpCode::BitNot` |
| Left shift `<<` | **[PARTIALLY]** | `OpCode::Shl` exists in LIR/VM but **no parser token** -- `<<` is not lexed. Cannot be used from source code. |
| Right shift `>>` | **[PARTIALLY]** | `OpCode::Shr` exists in LIR/VM but `>>` is lexed as `Compose` (function composition). Cannot be used as shift from source code. |

### Comparison Operators

| Feature | Status | Notes |
|---------|--------|-------|
| Standard (`==`, `!=`, `<`, `<=`, `>`, `>=`) | **[IMPLEMENTED]** | Full set of tokens, AST ops, opcodes |
| Chained comparisons `1 < x < 10` | **[MISSING - MEDIUM]** | Python-style chained comparisons. Would need parser change to emit `and`-joined comparisons. |

### Assignment Operators

| Feature | Status | Notes |
|---------|--------|-------|
| Compound `+=`, `-=`, `*=`, `/=` | **[IMPLEMENTED]** | `CompoundAssignStmt` in AST |
| Modulo assign `%=` | **[MISSING - HIGH PRIORITY]** | Missing despite `%` existing. Inconsistency. |
| Power assign `**=` | **[MISSING - LOW/SKIP]** | Rare in most languages. |
| Logical assign `&&=`, `\|\|=` | **[MISSING - LOW/SKIP]** | Mainly JS/Ruby. Lumen uses `and`/`or` keywords, making `and=` awkward. |
| Bitwise assign `&=`, `\|=`, `^=` | **[MISSING - MEDIUM]** | Would be consistent with having bitwise operators. |

### Other Operators

| Feature | Status | Notes |
|---------|--------|-------|
| Walrus operator `:=` (assign-and-use) | **[MISSING - LOW/SKIP]** | Python-specific. Lumen has `if let` which covers the main use case. |
| Ternary expression | **[IMPLEMENTED]** | `if cond then a else b` syntax via `Expr::IfExpr`. Covers the same ground as `cond ? a : b`. |
| Spread/rest `...` | **[IMPLEMENTED]** | `DotDotDot` token, `Expr::SpreadExpr`, rest patterns in list destructuring |
| Type test `is`/`instanceof` | **[PARTIALLY]** | `OpCode::Is` exists in VM (compares type name strings). Pattern matching `x: Int` is the primary mechanism. No `is` keyword as an expression operator in the parser. |
| Type cast `as` | **[PARTIALLY]** | `as` keyword exists as token (`As`) but only used for import/tool aliases (`import x: Y as Z`, `use tool x as Y`). No `expr as Type` casting expression. |
| Range step `1..10 step 2` | **[IMPLEMENTED]** | `Step` keyword, `RangeExpr` has `step: Option<Box<Expr>>` field, parser handles `step` after range |
| Concatenation `++` | **[IMPLEMENTED]** | `PlusPlus` token, `BinOp::Concat`, `OpCode::Concat` |
| Pipe `\|>` | **[IMPLEMENTED]** | `PipeForward` token, `Expr::Pipe`, desugars to first-arg function call |
| Membership `in` | **[IMPLEMENTED]** | `BinOp::In`, `OpCode::In` |

---

## 2. CONTROL FLOW

| Feature | Status | Notes |
|---------|--------|-------|
| `if`/`else`/`else if` | **[IMPLEMENTED]** | Full support including `if let` desugared to match |
| `for` loop | **[IMPLEMENTED]** | `for x in iterable`, tuple destructuring, range iteration |
| `for` with filter | **[PARTIALLY]** | Parser accepts `for x in iter if cond` but **discards the filter expression** (`let _ = self.parse_expr(0)?`). The condition is parsed and thrown away. |
| `while` loop | **[IMPLEMENTED]** | `WhileStmt`, `while let` desugared to loop+match |
| `loop` (infinite) | **[IMPLEMENTED]** | `LoopStmt` |
| `break` / `continue` | **[IMPLEMENTED]** | `BreakStmt` (with optional value), `ContinueStmt` |
| `break` with value | **[IMPLEMENTED]** | `break expr` parses and is in AST |
| Labeled breaks `break @label` | **[PARTIALLY]** | Parser accepts `break @label` syntax but **discards the label** (consumes `@` and identifier, returns `BreakStmt { value: None }`). No label tracking in loops. |
| `match` statement | **[IMPLEMENTED]** | Full pattern matching with guards, OR patterns, nested patterns, destructuring |
| `match` expression | **[IMPLEMENTED]** | `Expr::MatchExpr` -- match in expression position |
| `return` | **[IMPLEMENTED]** | `ReturnStmt` with value |
| `halt` | **[IMPLEMENTED]** | `HaltStmt` -- terminate with error |
| `emit` | **[IMPLEMENTED]** | `EmitStmt` -- output side-effect |
| `do-while` / `repeat-until` | **[MISSING - LOW/SKIP]** | Achievable with `loop ... if cond then break end ... end`. Not common in modern langs (Rust, Go, Swift all omit it). |
| `for-else` (Python-style) | **[MISSING - LOW/SKIP]** | Python-specific idiom. Very rare outside Python. |
| `try`/`catch`/`finally` | **[MISSING - HIGH PRIORITY]** | `try` keyword reserved, `TryExpr` (postfix `?`) exists for result unwrapping. But no `try`/`catch`/`finally` block syntax for structured error handling. Lumen uses `result[T, E]` + `match` for error handling, but lacks a structured try-catch construct. |
| `defer` / cleanup blocks | **[MISSING - MEDIUM]** | Go/Swift/Zig have `defer`. Useful for resource cleanup. Lumen has no RAII or defer mechanism. |
| Guard statements (Swift-style) | **[MISSING - LOW/SKIP]** | `guard cond else return`. Achievable with `if not cond then return end`. Swift-specific. |
| `async`/`await` | **[IMPLEMENTED]** | `async cell` parsing, `AwaitExpr`, `OpCode::Await`, `OpCode::Spawn`. Orchestration builtins (`parallel`, `race`, `vote`, `select`, `timeout`). |
| `yield` | **[PARTIALLY]** | `Yield` token reserved as keyword but **no parser, AST, or VM support** for generator/yield semantics. |

---

## 3. DATA STRUCTURES & TYPES

| Feature | Status | Notes |
|---------|--------|-------|
| Lists `list[T]` | **[IMPLEMENTED]** | `TypeExpr::List`, `Expr::ListLit`, `OpCode::NewList` |
| Maps `map[K, V]` | **[IMPLEMENTED]** | `TypeExpr::Map`, `Expr::MapLit`, `OpCode::NewMap` |
| Sets `set[T]` | **[IMPLEMENTED]** | `TypeExpr::Set`, `Expr::SetLit`, `OpCode::NewSet` |
| Tuples `tuple[T1, T2]` | **[IMPLEMENTED]** | `TypeExpr::Tuple`, `Expr::TupleLit`, `OpCode::NewTuple`, destructuring in patterns |
| Records | **[IMPLEMENTED]** | Named structs with typed fields, default values, where constraints |
| Enums | **[IMPLEMENTED]** | Variants with optional payloads, methods on enums |
| Union types `A \| B` | **[IMPLEMENTED]** | `TypeExpr::Union`, runtime `OpCode::NewUnion` |
| Result type `result[T, E]` | **[IMPLEMENTED]** | `TypeExpr::Result`, `ok`/`err` variants, postfix `?` try operator |
| Function types `fn(A) -> B` | **[IMPLEMENTED]** | `TypeExpr::Fn` with parameter types, return type, effect row |
| Null type | **[IMPLEMENTED]** | `TypeExpr::Null`, `Expr::NullLit`, `Value::Null` |
| Bytes type | **[IMPLEMENTED]** | `TypeExpr::Named("Bytes")`, `Expr::BytesLit`, `BytesLit` token |
| JSON type | **[IMPLEMENTED]** | `TypeExpr::Named("Json")` |
| Optional/nullable `T?` sugar | **[MISSING - HIGH PRIORITY]** | Must write `T \| Null` explicitly. `T?` shorthand is very common (Kotlin, Swift, TypeScript, Dart). Could desugar to `T \| Null` in the parser. |
| Generics | **[IMPLEMENTED]** | `GenericParam` with bounds, `TypeExpr::Generic`, generic records/enums/cells/traits |
| Generic constraints `where` | **[PARTIALLY]** | `GenericParam` has `bounds: Vec<String>` for `T: Bound` syntax. `CellDef` has `where_clauses`. Bounds checking has known failures (5 tests). |
| Type aliases | **[IMPLEMENTED]** | `TypeAliasDef`, `type UserId = String` |
| Traits | **[IMPLEMENTED]** | `TraitDef` with parent traits and method signatures |
| Impl blocks | **[IMPLEMENTED]** | `ImplDef` with generic params |
| References/borrowing | **[MISSING - LOW/SKIP]** | Lumen is a high-level AI-native language with GC semantics. Borrowing is a systems-level concern (Rust). |
| Slices | **[MISSING - MEDIUM]** | No slice type. `slice()` intrinsic returns a new list. A dedicated slice view would be more efficient. |
| Fixed-size arrays `[T; N]` | **[MISSING - LOW/SKIP]** | Systems-level feature. Lists serve this purpose in Lumen. |
| Regular expressions | **[MISSING - MEDIUM]** | No regex literal or type. `matches()` intrinsic exists but takes a string pattern. A regex literal (`/pattern/` or `r"pattern"`) would be useful. |
| Date/time types | **[MISSING - MEDIUM]** | No built-in date/time type. Timestamp is available as an intrinsic. A structured datetime type would improve ergonomics. |

---

## 4. SYNTAX SUGAR

| Feature | Status | Notes |
|---------|--------|-------|
| String interpolation `"Hello, {name}"` | **[IMPLEMENTED]** | `StringInterpLit` token, `Expr::StringInterp` with `StringSegment::Interpolation` |
| Raw strings `r"..."` | **[IMPLEMENTED]** | `RawStringLit` token, `Expr::RawStringLit` |
| Multi-line strings `"""..."""` | **[IMPLEMENTED]** | Triple-quoted strings with dedent handling in lexer |
| Pipe operator `\|>` | **[IMPLEMENTED]** | Desugars `x \|> f(y)` to `f(x, y)` |
| Range expressions `1..5`, `1..=5` | **[IMPLEMENTED]** | Both exclusive and inclusive, with optional step |
| Comprehensions `[x for x in list]` | **[IMPLEMENTED]** | List, map, and set comprehensions with optional filter |
| Destructuring in `let` | **[IMPLEMENTED]** | Tuple `let (a, b) = ...`, list `let [a, b] = ...`, record `let {a, b} = ...`, variant `let ok(v) = ...` |
| Destructuring in `for` | **[IMPLEMENTED]** | `for (k, v) in map_entries` |
| Destructuring in `match` | **[IMPLEMENTED]** | Full pattern destructuring in match arms |
| Default parameter values | **[IMPLEMENTED]** | `Param` has `default_value: Option<Expr>` |
| Variadic/rest params `...args` | **[PARTIALLY]** | Parser accepts `...` prefix on parameters but doesn't track the variadic flag in AST (`let _variadic = ...`). The value is discarded. |
| Named/keyword arguments | **[IMPLEMENTED]** | `CallArg::Named(name, expr, span)` |
| Compound assignment `+=` etc. | **[IMPLEMENTED]** | `CompoundAssignStmt` for `+=`, `-=`, `*=`, `/=` |
| Property shorthand `{name}` | **[MISSING - HIGH PRIORITY]** | Record construction requires `Name(field: field)`. Shorthand `Name(field)` when variable name matches field name would reduce boilerplate. Very common in JS/TS, Rust, Kotlin. |
| Implicit returns | **[MISSING - MEDIUM]** | Functions require explicit `return`. Lambda expression body `fn(x) => x + 1` is implicitly returned, but block bodies need `return`. Rust/Ruby/Elixir use last-expression-is-return. |
| Trailing lambdas | **[MISSING - MEDIUM]** | Kotlin/Swift allow `fn(x) { ... }` syntax where last lambda arg can be outside parens. Would improve readability for higher-order functions like `map`, `filter`. |
| Method chaining helpers | **[IMPLEMENTED]** | Pipe operator `\|>` serves this purpose |
| Computed property names | **[MISSING - LOW/SKIP]** | JS-specific `{[expr]: value}`. Maps handle dynamic keys. |
| `if let` | **[IMPLEMENTED]** | Desugared to match in parser |
| `while let` | **[IMPLEMENTED]** | Desugared to loop+match in parser |

---

## 5. FUNCTION FEATURES

| Feature | Status | Notes |
|---------|--------|-------|
| First-class functions | **[IMPLEMENTED]** | Functions are values, can be passed as arguments |
| Closures / lambdas | **[IMPLEMENTED]** | `fn(params) => expr` and `fn(params) ... end`. Captures working with upvalue mechanism. |
| Higher-order functions | **[IMPLEMENTED]** | `map`, `filter`, `reduce`, `flat_map`, `zip`, `any`, `all`, `find`, etc. as intrinsics |
| Named functions (cells) | **[IMPLEMENTED]** | `cell name(params) -> Type ... end` |
| Async functions | **[IMPLEMENTED]** | `async cell`, `await`, `spawn` |
| Currying / partial application | **[MISSING - MEDIUM]** | No built-in curry or partial application. Achievable via lambda wrappers. A `partial(f, arg1)` builtin or `f(_, arg2)` placeholder syntax would help. |
| Generators / yield | **[MISSING - MEDIUM]** | `yield` keyword reserved but unimplemented. Useful for lazy sequences. Lumen's iterator model is eager (lists). |
| Async generators | **[MISSING - LOW/SKIP]** | Depends on generators being implemented first. Very advanced feature. |
| Function composition `>>` | **[IMPLEMENTED]** | `Compose` token, parsed same as pipe |
| Recursion | **[IMPLEMENTED]** | Standard call stack with depth limit (256 frames) |
| Tail call optimization | **[PARTIALLY]** | `OpCode::TailCall` exists in LIR but unclear if parser/lowering emit it. |

---

## 6. MODULE SYSTEM

| Feature | Status | Notes |
|---------|--------|-------|
| Imports | **[IMPLEMENTED]** | `import module.path: Name1, Name2` and `import module.path: *` |
| Import aliases | **[IMPLEMENTED]** | `import module: Name as Alias` |
| Public visibility | **[IMPLEMENTED]** | `pub` modifier on records, enums, cells, traits, type aliases, imports |
| Re-exports | **[PARTIALLY]** | `pub import` exists in parser but unclear if re-export semantics are fully implemented |
| Circular import detection | **[IMPLEMENTED]** | Tracks compilation stack, clear error with import chain |
| Package management | **[PARTIALLY]** | `lumen pkg init`, `lumen pkg build`, `lumen pkg check` commands exist. Dependency resolution TBD. |

---

## 7. PATTERN MATCHING

| Feature | Status | Notes |
|---------|--------|-------|
| Literal patterns | **[IMPLEMENTED]** | Int, Float, String, Bool |
| Variant patterns | **[IMPLEMENTED]** | `ok(v)`, `err(e)`, `Some(x)` with nested sub-patterns |
| Wildcard `_` | **[IMPLEMENTED]** | |
| Identifier binding | **[IMPLEMENTED]** | |
| Guard patterns `p if cond` | **[IMPLEMENTED]** | |
| OR patterns `p1 \| p2` | **[IMPLEMENTED]** | |
| List destructuring `[a, b, ...rest]` | **[IMPLEMENTED]** | Including rest/spread |
| Tuple destructuring `(a, b)` | **[IMPLEMENTED]** | |
| Record destructuring `Type(field: p)` | **[IMPLEMENTED]** | Including open `..` patterns |
| Type-check patterns `x: Int` | **[IMPLEMENTED]** | |
| Nested patterns | **[IMPLEMENTED]** | Arbitrary nesting depth |
| Map patterns `{key: pattern}` | **[MISSING - HIGH PRIORITY]** | Cannot destructure maps in patterns. Elixir/Erlang support this. Would need new `Pattern::MapDestructure` variant. |
| Range patterns `1..10` | **[MISSING - MEDIUM]** | Rust/Swift support range patterns in match. |
| Exhaustiveness checking | **[MISSING - HIGH PRIORITY]** | No compile-time check that match arms cover all cases. Rust, Swift, Haskell enforce this. Very important for correctness. |

---

## 8. ERROR HANDLING

| Feature | Status | Notes |
|---------|--------|-------|
| Result type | **[IMPLEMENTED]** | `result[T, E]` with `ok`/`err` variants |
| Postfix try `?` | **[IMPLEMENTED]** | `Expr::TryExpr` -- unwraps ok, propagates err |
| Match on result | **[IMPLEMENTED]** | Standard pattern matching |
| `try`/`catch`/`finally` blocks | **[MISSING - HIGH PRIORITY]** | No structured exception handling. Result types cover explicit errors, but there is no way to catch panics/halts or ensure cleanup runs. |
| Error propagation | **[IMPLEMENTED]** | Via `?` operator |
| Custom error types | **[IMPLEMENTED]** | Any type can be the `E` in `result[T, E]` |
| `halt` (panic) | **[IMPLEMENTED]** | Terminates execution with message |

---

## 9. TYPE SYSTEM GAPS

| Feature | Status | Notes |
|---------|--------|-------|
| Type inference | **[IMPLEMENTED]** | Return types optional, expression types inferred |
| Generic bounds | **[PARTIALLY]** | `T: Bound` syntax parsed. Runtime enforcement has known test failures. |
| `as` type casting | **[MISSING - HIGH PRIORITY]** | No way to cast between compatible types (e.g., `Int` to `Float`, narrowing unions). `to_int()`, `to_float()`, `to_string()` intrinsics exist but no general casting syntax. |
| `is` type test expression | **[MISSING - HIGH PRIORITY]** | VM `OpCode::Is` exists but no parser support for `expr is Type` expressions. Pattern matching is the only way to test types. An `is` expression would be more ergonomic for simple checks. |
| Intersection types `A & B` | **[MISSING - LOW/SKIP]** | `&` is used for bitwise AND. TypeScript has these but they add complexity. |
| Literal types | **[MISSING - LOW/SKIP]** | TypeScript `type Direction = "north" \| "south"`. Enums serve this purpose. |
| Const generics | **[MISSING - LOW/SKIP]** | `Array<T, N>` style. Very advanced, Rust-level feature. |
| Higher-kinded types | **[MISSING - LOW/SKIP]** | `F<T>` where `F` itself is a type parameter. Haskell-level feature. |

---

## 10. MISCELLANEOUS

| Feature | Status | Notes |
|---------|--------|-------|
| Constants | **[IMPLEMENTED]** | `const NAME: Type = value` |
| Macros | **[PARTIALLY]** | `macro` keyword and `MacroDeclDef` AST node. Parser consumes macro body as raw tokens. No macro expansion system. |
| Decorators/attributes | **[PARTIALLY]** | `@` syntax for directives (`@strict`, `@deterministic`, `@doc_mode`). Generic attribute declarations parsed but not semantically processed. |
| REPL | **[IMPLEMENTED]** | `lumen repl` with history and multi-line support |
| Formatter | **[IMPLEMENTED]** | `lumen fmt` with `--check` mode |
| LSP | **[PARTIALLY]** | `lumen-lsp` crate exists but capabilities are limited/planned |
| WASM target | **[IMPLEMENTED]** | `lumen-wasm` crate, `lumen build wasm` command |
| Computed/dynamic dispatch | **[MISSING - MEDIUM]** | No vtable or dynamic dispatch mechanism. Traits exist but `impl` doesn't generate dispatch tables. |
| Operator overloading | **[MISSING - MEDIUM]** | No way to define `+`, `==`, etc. for custom types. Would work naturally with trait+impl. |

---

## Priority Summary

### HIGH PRIORITY (should implement soon)

1. **`T?` optional type sugar** -- Extremely common, reduces `T | Null` boilerplate
2. **`%=` compound assignment** -- Inconsistency: `%` exists but `%=` does not
3. **Match exhaustiveness checking** -- Critical for correctness in typed languages
4. **`is` type test expression** -- VM support exists, just needs parser wiring
5. **`as` type casting expression** -- Needed for numeric conversions and union narrowing
6. **Property shorthand in records** -- `Point(x, y)` when vars match field names
7. **Map destructuring patterns** -- Complete the destructuring story
8. **`try`/`catch` blocks** -- Structured error handling beyond result types
9. **Fix `for` loop filter** -- Parser accepts `for x in iter if cond` but discards the condition
10. **Fix labeled breaks** -- Parser accepts `break @label` but discards the label
11. **Fix variadic params** -- Parser accepts `...param` but discards the variadic flag

### MEDIUM PRIORITY (nice to have)

1. **Floor division `//`** -- Common in Python/Ruby for integer division
2. **Chained comparisons `1 < x < 10`** -- Python-style, reduces `and` chains
3. **Null-safe index `?[]`** -- Completes the null-safety story alongside `?.`
4. **Bitwise compound assignments `&=`, `|=`, `^=`** -- Consistency with bitwise ops
5. **Shift operators `<<`, `>>`** -- VM opcodes exist but unreachable from source
6. **Implicit returns** -- Last expression as return value in block bodies
7. **Trailing lambdas** -- Better ergonomics for HOF-heavy code
8. **`defer` blocks** -- Resource cleanup guarantee
9. **Currying / partial application** -- Functional programming convenience
10. **Generators / `yield`** -- Lazy sequences (keyword already reserved)
11. **Range patterns in match** -- `1..10 -> ...` in match arms
12. **Regular expression support** -- Regex literals or type
13. **Slice type** -- Efficient sub-list views
14. **Operator overloading** -- Via trait implementations
15. **Dynamic dispatch** -- Trait object / vtable mechanism

### LOW / SKIP (doesn't fit Lumen)

1. **Walrus operator `:=`** -- `if let` covers the use case
2. **`do-while`** -- `loop` + `break` is idiomatic
3. **`for-else`** -- Python-specific, rare elsewhere
4. **Guard statements** -- `if not cond then return` suffices
5. **Logical assignments `&&=`, `||=`** -- Awkward with keyword operators
6. **References/borrowing** -- High-level language, not systems
7. **Fixed-size arrays** -- Lists are sufficient
8. **Intersection types** -- Adds complexity without clear benefit
9. **Const generics** -- Systems-level feature
10. **Higher-kinded types** -- Academic/Haskell-level feature
11. **Computed property names** -- Maps handle dynamic keys
12. **Async generators** -- Depends on generators, very advanced

---

## Bugs / Silent Data Loss (parser accepts but discards)

These are the most urgent issues because they silently accept syntax and do nothing:

1. **`for x in iter if cond`** -- `parser.rs:1536-1538` parses the filter expr then discards it
2. **`break @label`** -- `parser.rs:1925-1931` parses the label then discards it
3. **`...param` variadic** -- `parser.rs:861` parses `...` prefix then discards the flag (`let _variadic = ...`)

These should either be properly implemented or produce a "not yet supported" error.
