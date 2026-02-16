# Design Decisions

This document comprehensively documents all major design decisions in the Lumen programming language. It explains the rationale behind key choices, what influenced them, and how they differ from other languages.

## Language Influences

Lumen draws inspiration from many languages, but makes deliberate choices about what to adopt, adapt, or reject:

| Feature | Influenced By | What We Borrowed | What We Changed |
|---------|--------------|-----------------|-----------------|
| Cells (functions) | Elixir | Named function blocks with `end` | Static typing, effect declarations |
| Records | Rust, OCaml | Typed struct-like data | Parentheses construction syntax, `where` constraints |
| Algebraic Effects | Koka, Eff, OCaml 5 | perform/handle/resume | Integrated with tool dispatch |
| Pattern Matching | Rust, ML family | Exhaustive match with guards | Expression-level match |
| Pipe Operator | Elixir, F# | `\|>` for data flow | Added `~>` for lazy composition |
| Markdown-Native | Literate programming | Code in documentation | Inverted: code-first with markdown blocks |
| Package Manager | Cargo, npm | Manifest + lockfile + registry | Mandatory namespacing, Sigstore signing |
| VM Architecture | Lua, CPython | Register-based bytecode | Fixed-width 32-bit instructions |
| Type System | TypeScript, Rust | Union types, Result, generics | `T?` sugar, structural records |
| String Interpolation | Python f-strings, Kotlin | `"Hello, {name}!"` | Direct expression embedding |
| Effect System | Koka | Effect rows on function signatures | Full algebraic effects with handlers |

## Syntax Decisions

### Why "cells" instead of "functions"

Lumen is AI-native — cells are computational units that can declare effects, carry policies, and interface with AI tool dispatch. The name reflects that they're more than pure functions. They're units of computation with declared side-effect profiles.

The term "cell" also suggests modularity and composability — cells can be chained, composed, and orchestrated. In AI agent workflows, cells are the atomic units of computation that can be traced, monitored, and reasoned about.

### Why records use parentheses `Point(x: 1, y: 2)` not braces

- **Consistent with constructor call syntax** in most languages — records are constructed like function calls
- **Braces are reserved** for set literals `{1, 2, 3}` and blocks
- **Reduces ambiguity** in the parser between blocks and record construction
- **Named arguments** (`field: value`) make construction self-documenting

This choice makes record construction visually distinct from set literals and block expressions, improving readability and reducing parsing complexity.

### Why `|>` AND `~>`

- **`|>` (pipe forward)**: Eager, immediate — value flows through transformations NOW
  ```lumen
  let result = data |> transform() |> format()
  ```
  The value is immediately passed as the first argument to each function in sequence.

- **`~>` (compose)**: Lazy, deferred — builds a new function from parts, executes later
  ```lumen
  let pipeline = parse ~> validate ~> transform
  let result = pipeline(data)  # Executes all three when called
  ```
  Creates a new composed function that can be passed around, stored, or called later.

Both are essential for different programming patterns: data pipelines vs function building. The distinction prevents confusion about when computation happens.

### Why markdown-native source format

Lumen supports two source formats:

- **`.lm.md` files**: Markdown-first, code in fenced blocks (literate programming)
- **`.lm`/`.lumen` files**: Code-first, triple-backtick blocks are markdown comments/docstrings

**Rationale:**
- AI agents can render and understand both formats naturally
- Documentation is part of the source, not separate
- Docstrings are rich markdown, enabling beautiful hover documentation in editors
- Encourages documentation-first thinking
- Makes code examples in documentation executable

This dual format supports both traditional code-first workflows and literate programming styles, while ensuring documentation is always accessible and executable.

### Why `end` keyword instead of braces

- **More readable at scale**, especially in AI-generated code
- **Self-documenting block closures** — you know exactly which block is closing
- **Fewer token-level ambiguities** (no "dangling else" problems)
- **Lower cognitive load** for reading deeply nested structures
- **Aligns with Ruby/Elixir tradition** that prioritizes readability
- **Light on tokens** — critical when agents are generating code at scale

The `end` keyword makes block boundaries explicit and unambiguous, improving both human and AI code comprehension.

### Why `#` for comments (not `//`)

- **`//` is floor division operator** — using it for comments would create ambiguity
- **`#` is familiar** from Python, Ruby, shell
- **Single character** = fewer tokens for AI to generate
- **Consistent** with the "light on tokens" philosophy

This choice avoids operator/comment ambiguity while keeping the syntax minimal.

### Why no semicolons

- **Newlines are statement separators** (like Python, Go)
- **Reduces visual noise** — fewer tokens means cleaner code
- **AI agents generate cleaner code** without semicolon placement decisions
- **Consistent with modern language trends**

The parser uses newlines as statement boundaries, with explicit line continuation when needed.

## Type System Decisions

### Static typing with inference

Lumen is statically typed but with extensive type inference. You rarely need to annotate types in local variables, but function signatures are explicit. This catches errors at compile time while keeping the syntax clean.

**Rationale:**
- **Compile-time safety** without ceremony
- **Type inference** reduces boilerplate
- **Explicit signatures** document contracts and enable separate compilation
- **Best of both worlds**: safety of static types, ergonomics of dynamic types

### `T?` as sugar for `T | Null`

- **Concise optional types** without importing an Option wrapper
- **Familiar** from TypeScript, Kotlin, Swift
- **Combines with null-coalescing** (`??`), null-safe access (`?.`), null-safe index (`?[]`)
- **Makes nullability explicit** in type signatures

This syntactic sugar makes optional types first-class and ergonomic, while maintaining explicit nullability tracking.

### Result type `result[T, E]`

- **Built-in, not library-defined** — part of the core type system
- **`ok(value)` and `err(error)` constructors** — clear, explicit
- **Pattern matching** for error handling
- **`try` expression** for early error propagation
- **No exceptions** — all errors are explicit in the type system

The Result type makes error handling explicit and type-safe, avoiding the "billion-dollar mistake" of exceptions while providing ergonomic error propagation.

### Why structural records (not nominal)

Records are identified by their field structure, not by name. This enables duck typing for records while maintaining static type safety. A function accepting `{name: String, age: Int}` works with any record that has those fields.

**Rationale:**
- **Flexibility** — functions work with any compatible record shape
- **No inheritance needed** — structural typing covers many OOP use cases
- **Type safety** — still statically checked, just by structure not name
- **Reduces coupling** — functions don't depend on specific record names

This design choice enables polymorphism without inheritance, reducing complexity while maintaining type safety.

### Match exhaustiveness

The compiler validates that match statements on enum subjects cover all variants. Missing variants produce `IncompleteMatch` errors. Wildcard `_` or catch-all identifier patterns make any match exhaustive.

**Rationale:**
- **Prevents runtime errors** from missing cases
- **Forces explicit handling** of all possibilities
- **Makes refactoring safer** — adding enum variants breaks code that doesn't handle them
- **Documentation** — exhaustive matches document all possible values

This is a key safety feature that catches bugs at compile time that would otherwise surface at runtime.

## VM Architecture Decisions

### Register-based (not stack-based)

- **Fewer instructions needed** per operation
- **Better mapping** to modern CPU architectures
- **Direct register addressing** avoids stack manipulation overhead
- **Follows Lua's proven model** for embedded/interpreted languages

Register-based VMs are more efficient for interpreted execution, requiring fewer instructions and better matching modern CPU register architectures.

### 32-bit fixed-width instructions

- **Predictable instruction size** for simple decoding
- **Fields**: `op` (8-bit opcode), `a`/`b`/`c` (8-bit registers), `Bx` (16-bit constant index), `Ax` (24-bit jump offset)
- **Trades code density for decode speed** — appropriate for an interpreted language
- **Lua-style encoding** — proven, well-understood format

Fixed-width instructions simplify the VM implementation and improve decode performance, at the cost of slightly larger bytecode.

### Rc with copy-on-write (not GC, not ownership)

- **`Rc<Vec<Value>>` for lists**, `Rc<BTreeMap<String, Value>>` for maps
- **`Rc::make_mut()` provides copy-on-write**: shared data is only cloned when mutated
- **No garbage collector pauses** — deterministic memory management
- **No borrow checker complexity** for users — simpler mental model
- **Performance profile**: fast reads (shared), pay-on-write for mutations
- **Future path** to gradual ownership system (`ref T`, `mut ref T`)

This design provides a simple memory model without GC pauses or borrow checker complexity, while maintaining good performance characteristics through copy-on-write semantics.

### String interning

All string constants are interned in a global table. Runtime string operations produce new strings but constant strings (field names, identifiers) are shared. Reduces memory for programs with many repeated string literals.

**Rationale:**
- **Memory efficiency** — repeated strings stored once
- **Fast equality** — interned strings can compare by pointer
- **Common pattern** in language implementations

String interning is a standard optimization that reduces memory usage and improves comparison performance for constant strings.

### Call frame stack (max depth 256)

The VM maintains a call frame stack with a maximum depth of 256. This prevents infinite recursion from crashing the VM while providing reasonable depth for most programs.

**Rationale:**
- **Safety** — prevents stack overflow from infinite recursion
- **Reasonable limit** — 256 frames is sufficient for most real programs
- **Clear error** — explicit "call stack overflow" error message

This limit provides a safety net while being generous enough for normal use cases.

## Effect System Decisions

### Full algebraic effects (not just effect tracking)

- **`perform Effect.operation(args)`** — invoke an effect
- **`handle body with Effect.op(params) -> resume(value) end`** — intercept and handle effects
- **One-shot delimited continuations** — handler can resume or abort
- **Effect handler stack** with proper scoping
- **More powerful than Rust's trait-based approach** or Java's checked exceptions
- **Enables**: dependency injection, testing, mocking, custom control flow, async, logging

Algebraic effects provide a unified model for side effects that is more expressive than exceptions or monads, enabling powerful abstractions while maintaining type safety.

### Effects on cell signatures

```
cell fetch_data(url: String) -> String / {http, trace}
```

The `/ {effects}` syntax declares what effects a cell may perform. The compiler tracks effect propagation and warns about undeclared effects.

**Rationale:**
- **Explicit effect declarations** — you know what side effects code can have
- **Effect provenance tracking** — compiler traces where effects come from
- **Enables effect handlers** — can intercept and handle effects
- **Type safety** — effects are part of the type system

This makes side effects explicit and trackable, enabling powerful abstractions like dependency injection and testing through effect handlers.

### Effect bindings to tools

Effects can be bound to tool aliases, enabling AI-native tool dispatch:

```lumen
use tool llm.chat as Chat
bind effect http.get to Chat
```

This integration makes AI tool calls first-class language constructs, not library calls.

## Package Manager Decisions

### Mandatory namespacing (`@namespace/name`)

- **No bare top-level names** like `lodash` or `react`
- **Every package is `@owner/package-name`**
- **Prevents name squatting** and typosquatting
- **You always know the source/owner** before installing
- **Namespace ownership** tied to identity (OIDC/GitHub)

Mandatory namespacing ensures every package has a clear owner and prevents the namespace conflicts that plague other package managers.

### Sigstore-style keyless signing

- **No long-lived signing keys** to manage or rotate
- **Ephemeral certificates** tied to OIDC identity
- **Transparency log** for all publishes
- **Verifiable build provenance**
- **Aligned with modern supply-chain security** (npm provenance, Sigstore)

Keyless signing eliminates key management complexity while providing strong security guarantees through transparency logs and ephemeral certificates.

### SAT/CDCL dependency resolver

- **Full boolean satisfiability solver** with conflict-driven clause learning
- **Single-version enforcement** by default (no diamond dependency drift)
- **Deterministic resolution** with explicit policy knobs
- **Resolution proofs** for auditability
- **`--frozen` and `--locked` modes** for reproducible CI builds

The SAT/CDCL resolver provides world-class dependency resolution with strong guarantees about determinism and conflict resolution.

### Content-addressed lockfile

- **Every dependency locked** by content ID and integrity hash
- **Lockfile itself has a top-level content_hash** for tamper detection
- **Git dependencies locked** to exact commit SHA
- **Compatible with deterministic builds** and supply-chain verification

Content-addressed lockfiles ensure reproducible builds and enable supply-chain security verification.

## What Lumen Does Differently

### vs Rust

- **No borrow checker** (Rc + CoW instead — simpler mental model)
- **No lifetimes in signatures** — easier to learn and use
- **AI-native effects and tool dispatch** built into the language
- **Markdown-native source format** — documentation is code
- **Much lighter syntax** for common operations

Lumen provides Rust-like safety without Rust's complexity, while adding AI-native features Rust doesn't have.

### vs TypeScript

- **True static typing** (not gradually typed)
- **Algebraic effects** instead of async/await spaghetti
- **Compiled to bytecode**, not interpreted JavaScript
- **Built-in package security** (signing, transparency)
- **No `undefined`** — just `Null` with safe operators

Lumen provides stronger type safety than TypeScript while offering more powerful abstractions for side effects and concurrency.

### vs Python

- **Statically typed** (catches errors at compile time)
- **Real concurrency model** via effects and futures
- **Compiled bytecode VM** (not interpreted)
- **Mandatory effect declarations** (know what your code does)
- **Package security from day one**

Lumen provides Python's ergonomics with Rust's safety and modern concurrency primitives.

### vs Koka

- **More practical/batteries-included** (package manager, tool dispatch, AI primitives)
- **Markdown-native source format**
- **Simpler syntax** (no row polymorphism in user-facing syntax)
- **Built for AI agent usage**, not just research

Lumen takes Koka's powerful effect system and makes it practical for real-world AI-native applications.

## Anti-Decisions (Things We Deliberately Avoided)

1. **No classes or inheritance** — Records + enums + effects cover all use cases without OOP complexity
2. **No null billion-dollar mistake** — Null exists but requires explicit `T?` typing and safe operators
3. **No implicit conversions** — All type conversions are explicit (`as` or conversion functions)
4. **No global mutable state** — Effects provide controlled side-effect management
5. **No semicolons** — Newlines are statement separators (like Python, Go)
6. **No header files** — Single-file modules with `pub` visibility
7. **No package.json-style version ranges** — Lockfiles are the source of truth for exact versions
8. **No exceptions** — Result types make errors explicit and type-safe
9. **No garbage collector** — Reference counting with copy-on-write provides deterministic memory management
10. **No gradual typing** — Static typing from the start, no `any` escape hatch
11. **No `undefined`** — Only `null` with explicit optional types
12. **No async/await keywords** — Effects and futures provide a unified concurrency model
13. **No macros at parse time** — Macros are parsed but have limited compile-time expansion (future work)

These anti-decisions reflect Lumen's philosophy: explicit over implicit, safe over convenient, correct over clever. Every avoided feature was a deliberate choice to keep the language focused and safe.

## Future Considerations

Some design decisions may evolve as the language matures:

- **Gradual ownership system** (`ref T`, `mut ref T`) — planned for future versions
- **Self-hosting** — compiler written in Lumen itself (exploratory)
- **Macro system** — full compile-time macro expansion (planned)
- **LSP maturity** — world-class editing experience (in progress)

These represent potential future directions while maintaining the core design principles that make Lumen what it is today.
