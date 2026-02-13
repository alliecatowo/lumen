# Lumen Tasks

This file tracks outstanding implementation work.
Completed work should be removed from this list and reflected in docs/changelog.

## Critical Bugs (P0)

- [ ] Fix `!=` operator: `BinOp::NotEq` maps to `OpCode::Eq` but never emits a `Not` inversion.
  - `lower.rs:1465` -- add `Not` instruction after `Eq` when op is `NotEq`.

- [ ] Fix closure/lambda upvalue capture: lambda bodies get fresh `RegAlloc` with no enclosing scope.
  - `lower.rs:1833-1892` -- build upvalue list, emit `GetUpval`/`SetUpval` for captured variables.

- [ ] Fix set/map comprehension lowering: `kind` field ignored, always emits `NewList`.
  - `lower.rs:2076-2151` -- dispatch on `ComprehensionKind` to emit `NewSet`/`NewMap`.

- [ ] Fix `if let` / `while let`: parser discards pattern, replaces with `BoolLit(true)`.
  - `parser.rs:1056-1060, 1457-1460` -- parse the binding pattern and lower it properly.

- [ ] Fix integer arithmetic overflow: unchecked ops panic in debug, wrap in release.
  - `vm.rs` Add/Sub/Mul/Pow handlers -- use `checked_*` or `wrapping_*` with defined semantics.

- [ ] Fix integer division/modulo by zero: silently returns 0.
  - `vm.rs:911-928` -- return `VmError::Runtime` on zero divisor.

- [ ] Fix bit shift panic on negative amounts: `-1i64 as u32` causes panic.
  - `vm.rs:1018,1026` -- clamp or error on out-of-range shift amounts.

- [ ] Fix string slice panic on non-ASCII: byte-indexed `s[start..end]` panics mid-codepoint.
  - `vm.rs:3157-3169` -- use `char_indices()` for character-based slicing.

- [ ] Fix `hex_decode` panic on odd-length input.
  - `vm.rs:2696-2703` -- guard against odd-length strings.

## Compiler Correctness (P1)

- [ ] Add register bounds checking in VM dispatch loop.
  - `RegisterOOB` error variant exists but is never used. Add bounds checks or debug assertions.

- [ ] Replace `unwrap()` calls in VM with proper error propagation.
  - `vm.rs:658,1150,1275,1297,1306` -- convert to `ok_or(VmError::...)`.

- [ ] Add duplicate definition detection for records, enums, cells, processes, effects, handlers.
  - `resolve.rs` -- currently only agents check for duplicates.

- [ ] Fix `for` loop tuple destructuring: discards all variables except the first.
  - `parser.rs:1128-1137` -- capture all identifiers in the destructuring pattern.

- [ ] Fix expression-position `match`/`if`/`loop`/`try`: returns placeholder `Ident("match_expr")` etc.
  - `parser.rs:3842-3910` -- implement real expression-position lowering for these forms.

- [ ] Remove hardcoded application-specific type names from resolver builtins.
  - `resolve.rs:244-285` -- `Invoice`, `MyRecord`, `LineItem`, `Pair`, `A/B/C` etc. are not builtins.

- [ ] Clean up `is_doc_placeholder_var` whitelist in typechecker.
  - `typecheck.rs:57-153` -- ~100 hardcoded variable names bypass type checking. Replace with proper scoping.

- [ ] Wire intrinsic name mapping for unmapped builtins (51 of 69 unreachable from source).
  - `lower.rs:1670-1686` -- add mappings for `sort`, `reverse`, `map`, `filter`, `reduce`, `trim`, `upper`, `lower`, `replace`, `find`, `zip`, `enumerate`, `flatten`, `unique`, `take`, `drop`, `first`, `last`, `is_empty`, `chars`, `starts_with`, `ends_with`, `index_of`, `round`, `ceil`, `floor`, `sqrt`, `pow`, `log`, `sin`, `cos`, `clamp`, `clone`, `debug`, etc.

- [ ] Fix `url_encode` for multi-byte UTF-8: encodes codepoint instead of UTF-8 bytes.
  - `vm.rs:2706-2718` -- iterate bytes, not chars.

## Type System (P1)

- [ ] Implement type alias resolution: aliases are stored but never expanded.
  - `typecheck.rs:758` and `resolve.rs:562-565` -- substitute alias during type resolution.

- [ ] Implement generic type parameter checking: generics parsed but never instantiated.
  - Path: type aliases -> generic instantiation -> bounded generics.

- [ ] Implement trait conformance checking and method dispatch.
  - `resolve.rs:567-586` -- traits/impls stored, never verified or dispatched.

- [ ] Implement record field default values at construction time.
  - `FieldDef.default_value` is parsed and stored but never applied.

- [ ] Implement runtime `where` constraint evaluation on record construction.
  - Constraints validated for form in `constraints.rs` but never enforced at runtime.

## Test Coverage (P1)

- [ ] Add regression tests for 3 known bugs: signed jumps, match register clobber, Type::Any BinOp.
- [ ] Add example files as automated integration tests (compile + execute the 6 working examples).
- [ ] Expand typechecker tests (currently 2): BinOp inference, record fields, call args, union returns.
- [ ] Expand lowering tests (currently 2): control flow, match, closures, string interp, records.
- [ ] Add constraint validation tests (currently 0).
- [ ] Add VM error path tests: stack overflow, undefined cell, register OOB, halt.
- [ ] Add end-to-end tests: while loop, for loop, string interp, closures, null coalesce, float arithmetic.

## Runtime Infrastructure (P2)

- [ ] Wire trace system into VM: `TraceRef` opcode currently creates dummy values.
  - VM has no `TraceStore` field. Trace infrastructure exists in runtime but is disconnected.

- [ ] Fix closure captures in VM: `Closure` opcode always creates zero captures.
  - `vm.rs:1271-1293` -- `cap_count` is always 0.

- [ ] Fix `Await` potential infinite loop when future never resolves.
  - `vm.rs:1411-1425` -- add fuel/timeout mechanism.

- [ ] Fix NaN handling: `Eq` for `Value::Float(NaN)` violates reflexivity; `Ord` treats NaN as Equal.
  - `values.rs:314,362` -- define consistent NaN semantics.

- [ ] Fix interned string comparison: all interned strings compare as empty string in `Ord`.
  - `values.rs:366-377` -- `Ord` impl needs `StringTable` access or owned-string fallback.

- [ ] Add instruction fuel/step counter for infinite loop protection.
- [ ] Implement disk cache loading on startup (currently memory-only despite writing to disk).
  - `cache.rs:34-36` -- `get()` only checks memory, ignores files from previous runs.

- [ ] Make tool dispatch async-capable.
  - `tools.rs:34` -- trait is synchronous, blocks entire VM.

## Language Semantics (P2)

- [ ] Implement real effect handler semantics with continuations.
  - Support scoped handling (`with <handler> in ...`) with interception/resume.

- [ ] Complete orchestration semantics beyond pipeline stage chains.
  - Coordinator-worker patterns, deterministic scheduling/merge.

- [ ] Implement guardrail and eval as executable runtime structures (currently stubs).

- [ ] Implement `let` destructuring patterns properly.
  - `parser.rs:1047` -- `pattern` field always `None`, uses `__tuple` workarounds.

- [ ] Implement spread operator semantics.
  - `lower.rs:2039` -- `SpreadExpr` just unwraps inner, discards spread.

- [ ] Implement `async cell` as semantically distinct from `cell`.

- [ ] Add machine transition trace events and replay hooks.

## Toolchain and Ecosystem (P3)

- [ ] Implement native tool execution (MCP client or subprocess protocol).
  - Only `StubDispatcher` exists. Tool execution is the language's core purpose.

- [ ] Add package/module system: imports are parsed but nonfunctional.
- [ ] Build first-party LSP server.
- [ ] Build `lumen fmt` formatter.
- [ ] Implement macro expansion.
- [ ] Add compatibility tooling (API/symbol diff, semver checks).
- [ ] Expand semantic conformance tests tied to spec sections.

## Documentation (Ongoing)

- [ ] Keep `SPEC.md` implementation-accurate.
  - Revise Section 2.7: say "parsed" not "supported" for type aliases, traits, impls, imports, macros.
  - Add note that `where` constraints are compile-time validation only, not runtime enforcement.
  - Specify lambda/closure semantics (they exist and work despite VISION saying "no closures in v1").
  - Document the intrinsic stdlib (69 intrinsics, major undocumented surface area).
- [ ] Keep `ROADMAP.md` aligned with major direction.
- [ ] Keep this file limited to concrete outstanding tasks.
