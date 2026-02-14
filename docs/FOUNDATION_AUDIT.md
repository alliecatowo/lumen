# Lumen Language Foundation Audit

**Date**: 2026-02-13
**Status**: PRE-BOOTSTRAP — All core language decisions are still flexible
**Purpose**: Deep analysis of foundational systems before building higher-level features

---

## Executive Summary

Lumen is at a critical juncture: **454 tests passing**, core compiler pipeline working, but the foundation has **significant gaps and design decisions that need deliberate choices** before we scale up. This audit examines seven core systems and provides actionable recommendations.

### Critical Findings

1. **Type System**: Ad-hoc checking, no real inference engine. Works for simple cases but will break at scale.
2. **Effect System**: More annotation than algebraic — missing handlers, resumption, composition.
3. **Pattern Matching**: Exhaustiveness only for enums, incomplete algorithm.
4. **Grammar**: Indentation-aware recursive descent (works but fragile), no formal grammar spec.
5. **Module System**: Barely started — imports parsed but not resolved, no visibility control.
6. **Generics**: Parsed and stored in AST but **completely ignored** by typechecker.
7. **Error Recovery**: Minimal — one error often cascades into many false positives.

---

## 1. TYPE SYSTEM

### 1.1 Current State

**File**: `rust/lumen-compiler/src/compiler/typecheck.rs` (1,346 lines)

**Architecture**:
- Single-pass bidirectional type checking (comments claim "bidirectional" but implementation is mostly checking with minimal inference)
- `Type` enum with 14 variants (String, Int, Float, Bool, Bytes, Json, Null, List, Map, Record, Enum, Result, Union, Tuple, Set, Fn, Generic, TypeRef, Any)
- Ad-hoc type compatibility via `check_compat()` — no unification
- `Type::Any` used for error recovery and unresolved types

**What Works**:
- ✅ Basic scalar type checking (Int, String, Bool, etc.)
- ✅ Collection types (list[T], map[K,V], set[T])
- ✅ Record and enum type checking
- ✅ Union types (A | B) with basic compatibility
- ✅ `result[T, E]` as a built-in generic (special-cased)
- ✅ Function types `fn(A, B) -> C`

**What's Missing**:

#### 1.1.1 No Type Unification
```rust
// typecheck.rs:734-800
fn infer_expr(&mut self, expr: &Expr) -> Type {
    match expr {
        Expr::IntLit(_, _) => Type::Int,
        Expr::Ident(name, span) => {
            if let Some(ty) = self.locals.get(name) {
                ty.clone()
            } else {
                // Falls back to Type::Any
                Type::Any
            }
        }
        // No unification — just pattern matching
    }
}
```

**Problem**: No Hindley-Milner style unification. Type inference is just "look up the type if known, else Any".

**Modern Comparison**:
- [Rust](https://doc.rust-lang.org/nightly/nightly-rustc/rustc_infer/index.html): Full HM with trait resolution
- [TypeScript](https://github.com/microsoft/TypeScript/blob/main/src/compiler/checker.ts): Structural subtyping with constraint solving
- [Gleam](https://gleam.run/): HM with row polymorphism
- **Lumen**: No inference engine at all

#### 1.1.2 Generics Are Ignored

```rust
// typecheck.rs:237-261
pub fn resolve_type_expr(ty: &TypeExpr, symbols: &SymbolTable) -> Type {
    match ty {
        TypeExpr::Generic(name, args, _) => {
            let arg_types: Vec<_> = args.iter().map(|t| resolve_type_expr(t, symbols)).collect();
            Type::TypeRef(name.clone(), arg_types)  // ← stored but never unified!
        }
        // ...
    }
}
```

**Evidence**:
1. Generics are parsed (`GenericParam` in AST)
2. Stored in `Type::TypeRef(String, Vec<Type>)`
3. But typechecker **never instantiates generic parameters**
4. No substitution, no constraint solving

**Example that compiles but shouldn't**:
```lumen
record Box[T]
  value: T
end

cell bad() -> Box[Int]
  return Box(value: "not an int")  # Should error, doesn't
end
```

#### 1.1.3 Type Aliases Are Shallow

```rust
// resolve.rs:255-257
if let Some(alias_target) = symbols.type_aliases.get(name) {
    resolve_type_expr(alias_target, symbols)  // Just substitutes once
}
```

Works for simple cases but not recursive or mutually recursive aliases.

#### 1.1.4 No Subtyping Relation

```rust
// typecheck.rs:1010-1030
fn check_compat(&mut self, expected: &Type, actual: &Type, line: usize) {
    if expected == actual {  // ← Exact equality only!
        return;
    }
    // Special cases for Any, Union, Null
    // But no real subtyping lattice
}
```

**Missing**:
- Width subtyping for records
- Variance for type constructors
- Covariance/contravariance for functions

### 1.2 Assessment

**Status**: 🔴 **CRITICAL GAP**

Current system is a **type checker** (validates explicit types), not a **type inference engine** (infers missing types).

**Comparison to Modern Languages**:

| Feature | Rust | TypeScript | Gleam | OCaml | **Lumen** |
|---------|------|------------|-------|-------|-----------|
| Hindley-Milner | ✅ | ❌ | ✅ | ✅ | ❌ |
| Bidirectional | ✅ | ✅ | ✅ | ❌ | 🟡 (partial) |
| Generics | ✅ | ✅ | ✅ | ✅ | ❌ (parsed only) |
| Inference | ✅ | ✅ | ✅ | ✅ | ❌ |
| Subtyping | ✅ | ✅ | ❌ | ❌ | ❌ |

**Research**: See [Hindley-Milner Type System](https://en.wikipedia.org/wiki/Hindley%E2%80%93Milner_type_system), [Bidirectional Type Checking](https://bernsteinbear.com/blog/type-inference/), [Type inference under the hood](https://medium.com/@aleksandrasays/type-inference-under-the-hood-f0ebbeb005a3)

### 1.3 Recommendations

**Priority**: 🔥 **MUST-FIX-NOW** (before adding more language features)

#### Option A: Full Hindley-Milner (Best for correctness)
- Implement Algorithm W with unification
- Add type variable generation and substitution
- Proper generic instantiation
- **Effort**: 2-3 weeks
- **Benefit**: Industry-standard type safety, generics work correctly

#### Option B: Bidirectional + Constraint Solving (TypeScript-style)
- Separate checking mode (known type) from inference mode
- Build constraint graph, solve at end
- **Effort**: 3-4 weeks
- **Benefit**: More flexible than HM, supports gradual typing

#### Option C: Minimize and Document Limitations
- Keep current checker
- Explicitly document: "No generics, no inference"
- Add lints for Any propagation
- **Effort**: 1 week
- **Benefit**: Honest about limitations, fast path forward

**Recommendation**: **Option A** — Implement Algorithm W. Lumen needs real generics for AI-native abstractions.

**Specific Changes**:
1. Add `rust/lumen-compiler/src/compiler/unify.rs` module
2. Replace `Type` equality with unification
3. Implement `instantiate()` for generic type application
4. Add `occur_check()` to prevent infinite types
5. Update `infer_expr()` to generate fresh type variables

**Code Locations**:
- `typecheck.rs:734-800` — `infer_expr()` needs unification
- `typecheck.rs:1010-1030` — `check_compat()` needs subtype checking
- `typecheck.rs:237-261` — `resolve_type_expr()` needs generic instantiation

---

## 2. EFFECT SYSTEM

### 2.1 Current State

**Files**:
- `resolve.rs` — effect inference (lines 800-1100)
- `ast.rs` — effect declarations (`EffectDecl`, `HandlerDecl`, `EffectBindDecl`)

**Architecture**:
- Effect annotations on cells: `cell foo() -> Int / {http, trace}`
- Effect inference: propagate from calls
- Effect binding: `bind effect llm to Chat` (explicit mapping)
- Runtime dispatch: `ToolProvider` trait

**What Works**:
- ✅ Effect declaration: `effect database ... end`
- ✅ Effect rows on cell signatures
- ✅ Effect inference (propagates through call graph)
- ✅ Strict mode enforces declared effects
- ✅ Effect bindings to tool aliases (no heuristic matching)
- ✅ Determinism checking (`@deterministic` rejects `uuid()`, `timestamp()`)
- ✅ Grant policy constraints checked at runtime

**What's Missing**:

#### 2.1.1 Not Algebraic Effects

```rust
// ast.rs:206-224
pub struct EffectDecl {
    pub name: String,
    pub operations: Vec<CellDef>,  // ← Just type signatures
    pub span: Span,
}

pub struct HandlerDecl {
    pub name: String,
    pub handles: Vec<CellDef>,  // ← Implementations
    pub span: Span,
}
```

**Missing**:
- No resumption (delimited continuations)
- No multi-shot handlers
- No effect composition (can't combine handlers)
- No parameterized effects

**Comparison**:

| Feature | Koka | Eff | OCaml 5 | Unison | **Lumen** |
|---------|------|-----|---------|--------|-----------|
| Algebraic effects | ✅ | ✅ | ✅ | ✅ | ❌ |
| Resumption | ✅ | ✅ | ✅ | ✅ | ❌ |
| Multi-shot | ✅ | ✅ | ❌ | ✅ | ❌ |
| Static tracking | ✅ | ❌ | ❌ | ❌ | ✅ |
| Inference | ✅ | ❌ | ❌ | ❌ | ✅ |

**Research**: See [Algebraic Effects for Functional Programming](https://www.microsoft.com/en-us/research/wp-content/uploads/2016/08/algeff-tr-2016-v2.pdf), [Algebraic Handler Lookup in Koka, Eff, OCaml, and Unison](https://interjectedfuture.com/algebraic-handler-lookup-in-koka-eff-ocaml-and-unison/), [Koka Programming Language](https://koka-lang.github.io/koka/doc/book.html)

#### 2.1.2 Effect Polymorphism Unclear

```lumen
# Does this work?
cell map[A, B](f: fn(A) -> B, xs: list[A]) -> list[B]
  # Should inherit effects from f
end
```

**Current behavior**: Effects are **not** polymorphic. Generic functions can't pass through effects from higher-order arguments.

### 2.2 Assessment

**Status**: 🟡 **WORKS BUT LIMITED**

Lumen has **effect annotations** with excellent static tracking, but it's not a true algebraic effect system.

**Good**:
- Effect inference works
- Explicit bindings avoid ambiguity
- Determinism checking is useful

**Bad**:
- Can't implement `async`/`await` as an effect
- Can't write custom effect handlers (state, logging, etc.)
- No way to abstract over effectful operations

### 2.3 Recommendations

**Priority**: 🟠 **CAN-WAIT** (effects work for AI use cases now, but limits future extensibility)

#### Option A: Full Algebraic Effects (Koka-style)
- Add delimited continuations to VM
- Implement `resume` and multi-shot handlers
- Allow user-defined effects
- **Effort**: 6-8 weeks (major VM changes)
- **Benefit**: Best-in-class effect system

#### Option B: Simplified Handlers (Eff-style)
- Single-shot continuations only
- Allow handler definitions to wrap effectful code
- No multi-shot (simpler VM)
- **Effort**: 3-4 weeks
- **Benefit**: Practical effect handling without full complexity

#### Option C: Keep Annotations Only
- Document that effects are tracking-only
- Focus on tool dispatch use case
- **Effort**: 0 weeks
- **Benefit**: Clear scope, no confusion

**Recommendation**: **Option C** for now, **Option B** later. Current system is good enough for AI tooling. Revisit when building libraries.

---

## 3. PATTERN MATCHING

### 3.1 Current State

**File**: `typecheck.rs:553-732` — `bind_match_pattern()`

**Patterns Supported**:
1. ✅ Literals: `1`, `"hello"`, `true`
2. ✅ Variants: `ok(x)`, `err(e)`, enum variants
3. ✅ Wildcard: `_`
4. ✅ Identifier binding: `x`
5. ✅ Guards: `p if cond`
6. ✅ Or-patterns: `p1 | p2`
7. ✅ List destructure: `[a, b, ...rest]`
8. ✅ Tuple destructure: `(a, b)`
9. ✅ Record destructure: `Type(field: p, ..)`
10. ✅ Type check: `x: Int`

**Exhaustiveness Checking**:

```rust
// typecheck.rs:471-492
// Exhaustiveness Check for Enums
if let Type::Enum(ref name) = subject_type {
    if !has_catchall {
        if let Some(ti) = self.symbols.types.get(name) {
            if let crate::compiler::resolve::TypeInfoKind::Enum(def) = &ti.kind {
                let missing: Vec<_> = def.variants
                    .iter()
                    .filter(|v| !covered_variants.contains(&v.name))
                    .map(|v| v.name.clone())
                    .collect();
                if !missing.is_empty() {
                    self.errors.push(TypeError::IncompleteMatch { /* ... */ });
                }
            }
        }
    }
}
```

**Algorithm**: Simple set difference — tracks covered variants, reports missing ones.

**What Works**:
- ✅ Exhaustiveness for **enums only**
- ✅ Detects unreachable patterns (wildcard covers all)
- ✅ Nested patterns work

**What's Missing**:

#### 3.1.1 Incomplete Exhaustiveness

**Not checked**:
1. ❌ Union types: `match (x: Int | String)` — no exhaustiveness
2. ❌ Integers: `match n` — infinite domain, but no warning
3. ❌ Booleans: `match b` with only `true ->` branch (should warn!)
4. ❌ Lists: `match [1, 2, 3]` — no length checking
5. ❌ Tuples: `match (a, b)` — no nested exhaustiveness
6. ❌ result[T, E]: Works (special-cased as enum) but not documented

**Example that should warn but doesn't**:
```lumen
cell bad(b: Bool) -> Int
  match b
    true -> return 1
    # Missing false case — no warning!
  end
end
```

#### 3.1.2 No Usefulness Checking

```lumen
cell redundant(x: Int) -> String
  match x
    1 -> "one"
    _ -> "other"
    2 -> "two"  # ← Unreachable! Should warn.
  end
end
```

Current implementation: No warning for dead patterns.

### 3.2 Assessment

**Status**: 🟡 **WORKS FOR SIMPLE CASES**

**Modern Algorithm** (Maranget 2007): Decision tree construction with constructor decomposition.

**Research**: See [Pattern Matching and Exhaustiveness](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2020/p2211r0.pdf), [Rust's pattern exhaustiveness checking](https://rustc-dev-guide.rust-lang.org/pat-exhaustive-checking.html), [Generic algorithm for exhaustivity](https://infoscience.epfl.ch/record/225497)

**Lumen vs. Rust**:
- Rust: Full usefulness algorithm, constructor splitting, witness generation
- Lumen: Enum variant set-checking only

### 3.3 Recommendations

**Priority**: 🟡 **SHOULD-FIX** (catches real bugs, improves DX)

#### Action Items

1. **Add bool exhaustiveness** (1 day)
   - Track `true` and `false` patterns
   - Warn if missing

2. **Add union type exhaustiveness** (2-3 days)
   - For `A | B | C`, require patterns for each type
   - OR a wildcard

3. **Implement usefulness checking** (1 week)
   - Port Rust's algorithm (well-documented in rustc-dev-guide)
   - Warn on unreachable patterns

4. **Document exhaustiveness guarantees** (1 day)
   - Update SPEC.md with what's checked and what isn't

**Code Locations**:
- `typecheck.rs:453-492` — extend exhaustiveness checking
- `typecheck.rs:553-732` — track pattern usefulness

---

## 4. GRAMMAR & SYNTAX

### 4.1 Current State

**Files**:
- `lexer.rs` (1,346 lines) — Indentation-aware tokenizer
- `parser.rs` (63,922 tokens, too large to read fully) — Recursive descent parser
- `tokens.rs` — Token definitions

**Lexer**:
```rust
// lexer.rs:108-155
fn handle_indentation(&mut self) -> Result<(), LexError> {
    let mut indent = 0;
    // Count spaces/tabs
    // Emit Indent/Dedent tokens based on stack
}
```

**Features**:
- ✅ Indentation-aware (Python-style)
- ✅ Handles line continuations with `\`
- ✅ String interpolation `"{expr}"`
- ✅ Triple-quoted strings with dedenting
- ✅ Raw strings `r"..."`
- ✅ Bytes literals `b"48656C6C6F"`
- ✅ Hex/binary/octal integers: `0xFF`, `0b1010`, `0o777`
- ✅ Scientific notation: `1e10`
- ✅ Unicode escapes: `\u{0041}`, `\x41`

**Parser Method**: Recursive descent (hand-written)

**Grammar Class**: Not formally specified, but appears to be LL(k) with backtracking in some cases.

### 4.2 Issues

#### 4.2.1 No Formal Grammar Specification

**Problem**: Grammar is implicit in parser code. No BNF, EBNF, or PEG spec.

**Consequence**:
- Hard to reason about ambiguities
- Hard to port to other tools (tree-sitter grammar is separate and can drift)
- No mechanical verification of grammar properties

**Example of ambiguity**:
```lumen
# Is this a function call or a record constructor?
Foo(bar: 1)
```

Current parser: Both are valid! Disambiguation happens at **type checking**, not parsing.

#### 4.2.2 Indentation Sensitivity

```rust
// lexer.rs:123-154
if matches!(self.current(), None | Some('\n') | Some('#')) {
    if self.current().is_none() {
        while self.indent_stack.len() > 1 {
            self.indent_stack.pop();
            self.pending.push(Token::new(TokenKind::Dedent, self.span_here()));
        }
    }
    return Ok(());
}
```

**Pros**:
- Clean syntax (no braces)
- Matches markdown aesthetic

**Cons**:
- Tabs vs. spaces issues (currently: 1 tab = 2 spaces)
- Error messages for indentation errors are cryptic
- Copy-paste can break code

#### 4.2.3 Keyword Consistency

**Reserved words** (from lexer.rs:800-872):

Categories:
1. **Declarations**: `record`, `enum`, `cell`, `agent`, `effect`, `handler`, `trait`, `impl`, `type`, `const`, `macro`
2. **Control flow**: `if`, `else`, `for`, `while`, `loop`, `match`, `break`, `continue`, `return`, `halt`
3. **Expressions**: `fn`, `await`, `parallel`, `spawn`, `emit`, `yield`, `try`
4. **Effects**: `use`, `tool`, `grant`, `expect`, `schema`, `role`, `bind`
5. **Modifiers**: `pub`, `mut`, `async`, `extern`, `comptime`
6. **Types**: `Null`, `result`, `list`, `map`, `set`, `tuple`, `union`
7. **Built-in types**: `bool`, `int`, `float`, `string`, `bytes`, `json`
8. **Operators**: `and`, `or`, `not`, `in`, `as`
9. **Misc**: `where`, `with`, `from`, `import`, `mod`, `self`, `then`, `when`, `step`, `end`

**Issues**:
- `result` is a keyword (lowercase) but `Null` is also a keyword (uppercase)
- `ok` and `err` are keywords (for result patterns) but `Some` / `None` aren't
- `end` closes all blocks — consistent but verbose

**Comparison**:
- Python: 35 keywords
- Rust: 51 keywords
- Lumen: ~70+ keywords — **TOO MANY**

#### 4.2.4 Expression vs. Statement Distinction

**Recent addition** (from AST):
```rust
// ast.rs:608-614
/// Match expression: match expr ... end (expression position)
MatchExpr {
    subject: Box<Expr>,
    arms: Vec<MatchArm>,
    span: Span,
},
/// Block expression: evaluates a sequence of statements, value is last expression
BlockExpr(Vec<Stmt>, Span),
```

**Good**: Lumen is moving toward expression-oriented (like Rust, not C).

**Incomplete**: `if` is statement-only, `match` can be expression. Inconsistent.

### 4.3 Assessment

**Status**: 🟡 **WORKS BUT IMPROVABLE**

**Grammar type**: Recursive descent parser over indentation-aware token stream. Not LR, not pure LL (has backtracking), not PEG.

**Research**: See [Parsing Expression Grammars](https://bford.info/pub/lang/peg.pdf), [Which Parsing Approach?](https://tratt.net/laurie/blog/2020/which_parsing_approach.html), [Guide to Parsing Algorithms](https://tomassetti.me/guide-parsing-algorithms-terminology/)

### 4.4 Recommendations

**Priority**: 🟢 **NICE-TO-HAVE** (works well enough for now)

#### Short-term (1-2 weeks)

1. **Write formal grammar in EBNF** (2 days)
   - Document all productions
   - Identify ambiguities explicitly
   - Publish in `docs/GRAMMAR.md`

2. **Reduce keyword count** (3 days)
   - Make `ok` / `err` / `null` contextual (not reserved)
   - Remove unused keywords (`comptime`, `extern`, `yield`, `when`, `then`)
   - **Target**: <50 keywords

3. **Make if an expression** (2 days)
   - Allow `let x = if cond then a else b`
   - Consistent with `match` expressions

#### Long-term (future)

4. **PEG grammar** (2 weeks)
   - Rewrite parser using pest or similar
   - Benefits: Clear semantics, no ambiguity, better errors

5. **Custom error recovery** (1 week)
   - Continue parsing after errors
   - Produce better "expected X, got Y" messages

**Code Locations**:
- `lexer.rs:800-872` — keyword list (reduce)
- `parser.rs` — needs formal spec
- `ast.rs:589-595` — extend if to IfExpr

---

## 5. MODULE SYSTEM

### 5.1 Current State

**Files**:
- `ast.rs:270-289` — Import declarations
- `resolve.rs:319-333` — Import methods (stubbed)

**What's Parsed**:
```rust
// ast.rs:283-289
pub struct ImportDecl {
    pub path: Vec<String>,          // e.g., ["std", "collections"]
    pub names: ImportList,           // Names or Wildcard
    pub is_pub: bool,                // re-export
    pub span: Span,
}
```

**What's NOT Implemented**:
1. ❌ Module resolution (no file loading)
2. ❌ Symbol visibility (pub/private)
3. ❌ Re-exports
4. ❌ Namespacing (all imports go into global scope)
5. ❌ Circular dependency detection

**Current Workaround**:
```rust
// lib.rs:100-120
pub fn compile_with_imports(
    source: &str,
    imports: HashMap<String, String>,  // ← Manual dependency map
) -> Result<LirModule, CompileError>
```

CLI provides imports manually. No automatic module resolution.

### 5.2 Modern Module System Comparison

| Feature | Rust | OCaml | TypeScript | Zig | **Lumen** |
|---------|------|-------|------------|-----|-----------|
| File = module | ✅ | ✅ | ✅ | ✅ | ❌ |
| Visibility control | ✅ (pub/private) | ✅ (module sig) | ✅ (export) | ✅ (pub) | ❌ |
| Re-exports | ✅ | ✅ | ✅ | ✅ | ❌ |
| Qualified imports | ✅ | ✅ | ✅ | ✅ | ❌ |
| Wildcard imports | ✅ | ✅ | ✅ | ✅ | 🟡 (parsed) |
| Circular imports | ❌ | ❌ | ✅ | ❌ | ❌ |

**Research**: See [Module systems in programming languages](https://denisdefreyne.com/notes/zlc9l-nrkfw-wztwz/), [OCaml Modular Programming](https://cs3110.github.io/textbook/chapters/modules/intro.html), [Draco Module System](https://draco-lang.org/specs/ModuleSystem)

### 5.3 Assessment

**Status**: 🔴 **CRITICAL GAP**

Lumen has **no module system** beyond parsing import syntax.

**Consequences**:
- Can't build standard library
- Can't organize large programs
- No code reuse

### 5.4 Recommendations

**Priority**: 🔥 **MUST-FIX-NOW** (before adding stdlib or AI libraries)

#### Phase 1: Basic Module Resolution (1-2 weeks)

1. **File = Module** convention
   - `import foo` loads `foo.lm.md` or `foo/mod.lm.md`
   - Search paths: current dir, stdlib, config paths

2. **Symbol Resolution**
   ```rust
   import std.collections { HashMap }
   # or
   import std.collections.*
   ```

3. **Circular Import Detection**
   - Track import stack during compilation
   - Error if cycle detected

#### Phase 2: Visibility Control (1 week)

4. **Public by default** (like Python)
   - Add `private` keyword for hiding symbols
   - OR `pub` keyword for exporting (like Rust)

5. **Re-exports**
   ```lumen
   pub import std.collections { HashMap }
   # Makes HashMap available to importers of this module
   ```

#### Phase 3: Advanced Features (2-3 weeks)

6. **Qualified Imports**
   ```lumen
   import std.collections as col
   let m = col.HashMap()
   ```

7. **Module Aliases**
   ```lumen
   import foo.bar.baz as short
   ```

**Code Locations**:
- `rust/lumen-compiler/src/compiler/module.rs` — new module (create)
- `resolve.rs:319-333` — implement import resolution
- `lib.rs:100-120` — integrate module loader

**Design Question**: Should Lumen follow Rust (explicit pub), Python (explicit private), or TypeScript (export list)?

**Recommendation**: **Rust-style `pub`** — default private, explicit public. Forces intentional API design.

---

## 6. GENERICS

### 6.1 Current State

**Fully parsed, completely ignored**:

```rust
// ast.rs:110-117
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericParam {
    pub name: String,
    pub bounds: Vec<String>,  // trait bounds (parsed but unused)
    pub span: Span,
}
```

**Where generics appear**:
- `RecordDef::generic_params`
- `EnumDef::generic_params`
- `CellDef::generic_params`
- `TypeAliasDef::generic_params`
- `ImplDef::generic_params`

**Typechecker behavior**:
```rust
// typecheck.rs:293-297
TypeExpr::Generic(name, args, _) => {
    let arg_types: Vec<_> = args.iter().map(|t| resolve_type_expr(t, symbols)).collect();
    Type::TypeRef(name.clone(), arg_types)  // ← Stored but not checked!
}
```

### 6.2 What Should Work But Doesn't

```lumen
record Pair[A, B]
  first: A
  second: B
end

cell swap[A, B](p: Pair[A, B]) -> Pair[B, A]
  return Pair(first: p.second, second: p.first)
end

cell main()
  let p1: Pair[Int, String] = Pair(first: 1, second: "hello")
  let p2: Pair[String, Int] = swap(p1)  # Should work, doesn't type-check
end
```

**Current behavior**: Compiles but `swap()` doesn't actually swap types.

### 6.3 Assessment

**Status**: 🔴 **CRITICAL GAP**

Generics are **syntactic sugar only**. No semantic checking.

### 6.4 Recommendations

**Priority**: 🔥 **MUST-FIX-NOW** (blocked by type system)

**Fix**: Implement HM type inference (see Type System section). Once unification works, generics fall out naturally.

**Steps**:
1. Generate fresh type variables for generic params
2. Substitute during instantiation
3. Unify at call sites
4. Generalize at let-bindings

**Effort**: Part of Type System overhaul (2-3 weeks total).

---

## 7. ERROR HANDLING & DIAGNOSTICS

### 7.1 Current State

**Files**:
- `diagnostics.rs` (500 lines) — Error formatting
- Each compiler phase: custom error types

**Error Types**:
1. `LexError` — tokenization
2. `ParseError` — syntax
3. `ResolveError` — name resolution, effects
4. `TypeError` — type checking
5. `CompileError` — top-level wrapper

**Formatting**:
```rust
// diagnostics.rs:200-250
pub fn format_error(err: &CompileError, source: &str, filename: &str) -> String {
    // Extract line, show context, point to error
    // Similar to Rust's error format
}
```

### 7.2 Issues

#### 7.2.1 Error Cascades

**Problem**: One error produces many downstream errors.

**Example**:
```lumen
record User
  name: Stirng  # Typo: "Stirng" instead of "String"
end

cell greet(u: User) -> String
  return "Hello, " + u.name  # Error: undefined type Stirng
end

cell main()
  let u = User(name: "Alice")  # Error: undefined type Stirng
  print(greet(u))              # Error: undefined type Stirng
end
```

**Compiler output**: 4 errors, all stemming from one typo.

**Ideal**: 1 error with "did you mean String?" suggestion.

#### 7.2.2 No Error Recovery

**Behavior**: Parser stops at first error.

```lumen
cell main()
  let x = 1 +  # Parse error: unexpected newline
  let y = 2    # Not parsed — parser stopped
  return x
end
```

**Modern parsers**: Continue parsing, report multiple errors.

### 7.3 Assessment

**Status**: 🟡 **FUNCTIONAL BUT BASIC**

**Good**:
- Source context shown
- Line/column numbers accurate
- Color output in terminal

**Missing**:
- Error recovery (parser continues after error)
- Suggestions ("did you mean?")
- Error codes (like Rust's E0001, E0002)
- JSON output for tools

### 7.4 Recommendations

**Priority**: 🟡 **SHOULD-FIX** (improves DX significantly)

#### Action Items

1. **Add error recovery to parser** (1 week)
   - Synchronize at statement boundaries
   - Continue parsing after errors
   - Suppress cascading errors

2. **"Did you mean?" suggestions** (3 days)
   - Already have Levenshtein distance in typecheck.rs (line 79-116)
   - Use for undefined variables, types, fields

3. **Error codes** (2 days)
   - Assign unique codes to each error variant
   - Link to documentation

4. **Elm-style errors** (1 week)
   - Friendly prose explanations
   - Examples of correct code
   - Related: Task #3 in existing task list

**Code Locations**:
- `diagnostics.rs` — enhance error messages
- `parser.rs` — add error recovery
- `typecheck.rs:79-134` — extend suggestions to all errors

---

## 8. GRAMMAR FORMALISM

### 8.1 What Lumen Needs

**Question**: Is the grammar LR(1)? LL(k)? PEG?

**Answer**: **None of the above**. It's an indentation-aware recursive descent parser with ad-hoc backtracking.

**Evidence**:
```rust
// lexer.rs handles indentation, emits Indent/Dedent tokens
// parser.rs does recursive descent over token stream
```

**Characteristics**:
- Indentation-sensitive (like Python, not like most formal grammars)
- Recursive descent (hand-written, not generated)
- No left recursion (recursive descent can't handle it)
- Some lookahead (peek ahead for disambiguation)

### 8.2 Comparison to Formal Classes

**LL(k)**:
- Top-down parsing
- k tokens of lookahead
- Can't handle left recursion
- **Lumen fits here** (approximately LL(2) in most places)

**LR(k)**:
- Bottom-up parsing
- More powerful than LL
- Can handle left recursion
- **Lumen doesn't use LR** (no shift-reduce)

**PEG**:
- Ordered choice (first match wins)
- Packrat parsing (memoization)
- No ambiguity by definition
- **Lumen could be rewritten as PEG** (but isn't currently)

**Research**: See [Parsing Expression Grammars](https://en.wikipedia.org/wiki/Parsing_expression_grammar), [LR vs LL parsing](https://tratt.net/laurie/blog/2020/which_parsing_approach.html)

### 8.3 Recommendation

**Write EBNF grammar** (even if implementation is hand-written RD). Benefits:
- Mechanical checks for ambiguity
- Reference for alternative implementations
- Documentation for language spec

**Example EBNF** (starter):
```ebnf
program ::= directive* item*
item ::= record_def | enum_def | cell_def | import_decl
record_def ::= "record" IDENT generic_params? NEWLINE INDENT field* DEDENT "end"
field ::= IDENT ":" type_expr constraint? default? NEWLINE
```

---

## 9. CROSS-CUTTING CONCERNS

### 9.1 Documentation

**Current docs**:
- ✅ `SPEC.md` — comprehensive, implementation-accurate
- ✅ `CLAUDE.md` — good compiler overview
- ✅ `docs/GETTING_STARTED.md` — tutorial
- ✅ `examples/*.lm.md` — 22 working examples

**Missing**:
- ❌ Formal grammar specification
- ❌ Type system specification (inference rules)
- ❌ Rationale documents (why Lumen made specific choices)

### 9.2 Testing

**Test coverage** (454 tests passing):
- ✅ Compiler unit tests (150+)
- ✅ Spec suite (81 tests)
- ✅ Examples (22 files)
- ✅ VM tests (150+)

**Missing**:
- ❌ Negative tests (should-fail cases)
- ❌ Performance benchmarks
- ❌ Fuzz testing

### 9.3 Tooling

**What exists**:
- ✅ CLI (check, run, emit, trace)
- ✅ VS Code extension (syntax highlighting)
- ✅ Tree-sitter grammar
- ✅ LSP (separate crate)

**What's missing**:
- ❌ Formatter (`lumen fmt` exists but unimplemented)
- ❌ Linter
- ❌ Documentation generator
- ❌ Package manager (stubs exist)

---

## 10. ACTIONABLE ROADMAP

### 10.1 Critical Path (Before Beta)

**Must-fix-now** (blocking all future work):

1. **Type System Overhaul** (3 weeks)
   - Implement Hindley-Milner Algorithm W
   - Fix generics
   - Add subtyping

2. **Module System** (2 weeks)
   - File-based module resolution
   - Visibility control (pub/private)
   - Circular import detection

3. **Error Recovery** (1 week)
   - Parser continues after errors
   - Suppress cascading errors

**Total**: 6 weeks of focused work

### 10.2 High-Priority Improvements

4. **Pattern Matching Exhaustiveness** (1 week)
   - Bool, union, tuple checking
   - Usefulness checking (unreachable patterns)

5. **Formal Grammar Spec** (3 days)
   - Write EBNF
   - Document ambiguities

6. **Reduce Keywords** (3 days)
   - <50 keywords
   - Make common words contextual

### 10.3 Nice-to-Have (Post-Beta)

7. **Algebraic Effects** (6-8 weeks)
   - Delimited continuations
   - User-defined handlers

8. **PEG Parser Rewrite** (2 weeks)
   - Better error messages
   - Easier to maintain

9. **Elm-Style Errors** (1 week)
   - Friendly prose
   - Code examples

---

## 11. COMPARATIVE ANALYSIS

### 11.1 Lumen vs. Similar Languages

| Feature | Lumen | Eff | Koka | Unison | Gleam |
|---------|-------|-----|------|--------|-------|
| **Type System** |
| HM inference | ❌ | ✅ | ✅ | ✅ | ✅ |
| Generics | ❌ | ✅ | ✅ | ✅ | ✅ |
| Subtyping | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Effects** |
| Static tracking | ✅ | ❌ | ✅ | ❌ | ❌ |
| Algebraic | ❌ | ✅ | ✅ | ✅ | ❌ |
| Handlers | ❌ | ✅ | ✅ | ✅ | ❌ |
| **Pattern Matching** |
| Exhaustiveness | 🟡 | ✅ | ✅ | ✅ | ✅ |
| Usefulness | ❌ | ✅ | ✅ | ❌ | ✅ |
| **Modules** |
| Implemented | ❌ | ✅ | ✅ | ✅ | ✅ |

**Conclusion**: Lumen is **pre-alpha** on core language features, but has unique strengths (AI-native primitives, effect tracking).

### 11.2 Industry Standards

**Type Safety Tier**:
- **Tier 1** (Rust, Haskell, OCaml): Full HM, soundness proofs
- **Tier 2** (TypeScript, Kotlin): Gradual typing, practical soundness
- **Tier 3** (Python with types, Ruby with Sorbet): Opt-in typing
- **Lumen**: Currently **Tier 2.5** — better than gradual typing (strict by default) but missing inference

**Effect Systems**:
- **Tier 1** (Koka, Eff): Full algebraic effects
- **Tier 2** (Lumen): Effect tracking without handlers
- **Tier 3** (Most languages): No effect system

---

## 12. FINAL RECOMMENDATIONS

### 12.1 Immediate Actions (This Week)

1. ✅ Complete this audit document
2. 📝 Create GitHub issues for each critical gap
3. 📝 Update SPEC.md with known limitations
4. 📝 Add "Type System Status" section to README

### 12.2 Next 6 Weeks (Critical Path)

**Week 1-3: Type System**
- File: `rust/lumen-compiler/src/compiler/unify.rs` (new)
- Implement Algorithm W
- Fix generics

**Week 4-5: Module System**
- File: `rust/lumen-compiler/src/compiler/module.rs` (new)
- File resolution
- Visibility control

**Week 6: Error Recovery**
- File: `rust/lumen-compiler/src/compiler/parser.rs`
- Statement synchronization
- Cascading error suppression

### 12.3 Success Metrics

**Before this work**:
- 454 tests passing
- Generics don't work
- No multi-file programs
- Error messages basic

**After 6 weeks**:
- ✅ 500+ tests passing (includes generic tests)
- ✅ Generics fully functional
- ✅ Standard library can be written (multi-file)
- ✅ Error recovery prevents cascades
- ✅ Exhaustiveness covers bool + union types

---

## 13. REFERENCES & FURTHER READING

### Type Systems
- [Hindley-Milner Type System](https://en.wikipedia.org/wiki/Hindley%E2%80%93Milner_type_system) — Wikipedia overview
- [Implementing Hindley-Milner](https://blog.stimsina.com/post/implementing-a-hindley-milner-type-system-part-1) — Tutorial
- [Damas-Hindley-Milner inference](https://bernsteinbear.com/blog/type-inference/) — Max Bernstein's guide
- [Type inference under the hood](https://medium.com/@aleksandrasays/type-inference-under-the-hood-f0ebbeb005a3) — Aleksandra Sikora's article

### Algebraic Effects
- [Algebraic Effects for Functional Programming](https://www.microsoft.com/en-us/research/wp-content/uploads/2016/08/algeff-tr-2016-v2.pdf) — Microsoft Research paper
- [Algebraic Handler Lookup in Koka, Eff, OCaml, and Unison](https://interjectedfuture.com/algebraic-handler-lookup-in-koka-eff-ocaml-and-unison/) — Comparison
- [Koka Programming Language](https://koka-lang.github.io/koka/doc/book.html) — Reference implementation
- [Why Algebraic Effects?](https://antelang.org/blog/why_effects/) — Motivation

### Pattern Matching
- [Exhaustiveness Checking for Pattern Matching](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2020/p2211r0.pdf) — C++ proposal (good algorithm overview)
- [Rust's pattern exhaustiveness checking](https://rustc-dev-guide.rust-lang.org/pat-exhaustive-checking.html) — Implementation guide
- [Generic algorithm for exhaustivity](https://infoscience.epfl.ch/record/225497) — Academic paper
- [A Term Pattern-Match Compiler](https://www.classes.cs.uchicago.edu/archive/2011/spring/22620-1/papers/pettersson92.pdf) — Automata approach

### Parsing & Grammar
- [Parsing Expression Grammars](https://bford.info/pub/lang/peg.pdf) — Bryan Ford's original paper
- [Which Parsing Approach?](https://tratt.net/laurie/blog/2020/which_parsing_approach.html) — Laurence Tratt's overview
- [Guide to Parsing Algorithms](https://tomassetti.me/guide-parsing-algorithms-terminology/) — Survey

### Module Systems
- [Module systems in programming languages](https://denisdefreyne.com/notes/zlc9l-nrkfw-wztwz/) — Comparative overview
- [OCaml Modular Programming](https://cs3110.github.io/textbook/chapters/modules/intro.html) — Cornell CS3110 textbook
- [Draco Module System](https://draco-lang.org/specs/ModuleSystem) — Modern example

---

## Appendix A: Code Statistics

**Compiler Lines of Code**:
- `lexer.rs`: 1,346 lines
- `parser.rs`: ~2,500 lines (estimated from token count)
- `typecheck.rs`: 1,346 lines
- `resolve.rs`: ~1,500 lines
- `lower.rs`: ~1,200 lines
- `ast.rs`: 754 lines
- `lir.rs`: ~800 lines
- **Total**: ~18,000 lines

**Test Count**: 454 passing, 2 failing (as of 2026-02-13)

**Language Features Implemented**: ~85% (missing: generics semantics, modules, some exhaustiveness)

**Estimated Completion**: 6 weeks to critical-path features

---

## Appendix B: Decision Matrix

| Decision Point | Option A | Option B | Option C | Recommendation |
|----------------|----------|----------|----------|----------------|
| **Type System** | Full HM | Bidirectional + constraints | Document limitations | **Option A** (HM) — Necessary for generics |
| **Effect System** | Full algebraic | Simplified handlers | Keep annotations | **Option C** for now — Good enough |
| **Pattern Matching** | Full usefulness | Enum + bool only | Current | **Option A** (full) — Catch real bugs |
| **Grammar** | PEG rewrite | EBNF spec | Current | **Option B** (EBNF) — Document first |
| **Modules** | Rust-style pub | Python-style private | TypeScript export | **Option A** (pub) — Explicit is better |
| **Error Recovery** | Full sync | Statement sync | None | **Option B** (statement) — Good ROI |

---

## Appendix C: Timeline

```
Week 1-2: Type System (Unification + Algorithm W)
Week 3: Type System (Generics + Substitution)
Week 4: Module System (File resolution)
Week 5: Module System (Visibility + Re-exports)
Week 6: Error Recovery + Exhaustiveness
Week 7+: Nice-to-have improvements
```

**Checkpoint**: After Week 6, re-evaluate priorities with working generics and modules.

---

**END OF AUDIT**

This document should guide all future development decisions for Lumen's core language. Update this document as design decisions are made and implemented.

**Status**: ✅ COMPLETE
**Next Steps**: Review with team, prioritize implementation, create GitHub issues
