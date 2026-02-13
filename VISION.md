# The Lumen Grail v2 Blueprint

**A Markdown-native, AI-first, performance-obsessed, capability-safe programming language.**

*Revised: OCaml removed. Register VM. MCP-native. Sharper competitive positioning. Honest about what's hard.*

---

## 0) My honest take on Lumen before we begin

You asked what I think. Here it is.

**What Lumen gets right that nothing else does:**

The core insight is genuinely novel. Right now, every agent framework — LangChain, CrewAI, AutoGen, the OpenAI Agents SDK, Google ADK — is a *library* bolted onto Python or TypeScript. They inherit all the ambient authority, mutation, and side-effect chaos of their host languages. You cannot look at a LangChain pipeline and know what it *can* do without reading every line of code. You cannot replay it. You cannot diff two runs. You cannot prove it didn't exfiltrate data. Lumen makes those properties *structural*. The language itself enforces them. That's not incremental — that's a category difference.

The Markdown-native format is also smarter than it first appears. Agent workflows are already written as docs — READMEs, runbooks, Notion pages. Making the doc *be* the program means the artifact humans already produce becomes the executable. This is literate programming done right, because the prose isn't decorative — it's the primary authoring surface, and the code blocks are the executable spine.

**The closest competitor is Dana**, announced by the AI Alliance in June 2025. Dana is also an "agent-native programming language" with deterministic execution, built-in concurrency, and production validation. But Dana is a Python-like DSL that transpiles through a Python runtime. It's a DSL, not a compiled language with its own IR. It doesn't have Lumen's content-addressed trace system, capability-scoped tool grants, or the Markdown-native authoring model. Dana proves the market wants this; Lumen can be the version that's actually engineered.

**What's genuinely hard:**

Adoption. You're asking people to learn a new language. TypeScript has 30 million developers and every editor plugin imaginable. Lumen has zero. The LSP and editor experience need to be *exceptional* from day one — not "good for a new language" but actually competitive with TypeScript's developer experience. The schema validation and trace caching need to feel like magic to justify the learning curve.

The other hard thing is the tool ecosystem. Lumen's power depends on having tools to call. If you ship with `http.get` and `llm.chat` and nothing else, the language feels like a toy. The MCP compatibility story (Section 8) is existential, not optional.

**The bottom line:** Lumen is a real idea with real technical merit. It's not a vanity language. The verifiable-trace + capability-scoped-effects combination is genuinely unsolved by anything in production today. But it will live or die on execution speed — both compiler performance and time-to-first-useful-program for a new developer.

Now let's build it.

---

## 1) What Lumen is

### Lumen in one sentence

Lumen is Markdown that executes — with typed schemas, capability-scoped tool calls, and hash-chained traces — compiled to its own IR and run locally at near-instant speed.

### The novel thing Lumen does best

**Verifiable agent workflows, as text.**

Every effectful step is capability-scoped and produces a hash-chained trace that makes "AI did stuff" replayable, diffable, cacheable, and provable. No other language or framework provides this as a structural guarantee rather than an opt-in logging layer.

### The performance promise

| Operation | Target |
|-----------|--------|
| Parsing a doc | < 5ms for typical files |
| Typechecking | < 10ms incremental |
| Running pure cells | Sub-millisecond |
| Running effectful cells | As fast as tools allow, with deterministic caching |
| Editor experience | LSP that feels TypeScript-fast |

These are real targets, not aspirational. The register VM architecture and single-file compilation model make them achievable.

---

## 2) The architecture: one compiler, one VM, one language

### Why we dropped OCaml

The original blueprint proposed an OCaml seed compiler for "bootstrap legitimacy." This was academically appealing but practically wasteful:

1. **Lumen doesn't need a traditional bootstrap.** Bootstrapping matters when language X must compile itself (C needs a C compiler to compile a C compiler). Lumen compiles to LIR, which runs in a VM. The VM is infrastructure, not identity — the same way CPython is written in C but Python is not "a C language."

2. **Two frontends doubles maintenance for a marginal correctness gain.** Cross-checking two compiler outputs sounds rigorous, but you get the same correctness confidence from a comprehensive conformance test suite plus property-based fuzzing, at a fraction of the engineering cost.

3. **The self-hosting story still works.** Write compiler v1 in Rust. Get it solid. Write compiler v2 in Lumen itself. Run v2 on the VM. Compare its output against the conformance suite. If it passes, you have a self-hosting compiler. No OCaml required.

### The architecture

```
                    ┌─────────────────────────────────┐
                    │        .lm.md Source File        │
                    │   (Markdown + fenced Lumen)      │
                    └──────────────┬──────────────────┘
                                   │
                    ┌──────────────▼──────────────────┐
                    │       Rust Compiler Frontend     │
                    │  Extract → Lex → Parse → Type   │
                    │       → Lower → Emit LIR        │
                    └──────────────┬──────────────────┘
                                   │
                    ┌──────────────▼──────────────────┐
                    │           LIR Module             │
                    │  (Canonical JSON v1 / Binary v2) │
                    └──────────────┬──────────────────┘
                                   │
                    ┌──────────────▼──────────────────┐
                    │        Rust Register VM          │
                    │   Execute LIR + Tool Dispatch    │
                    │   + Trace Emission + Caching     │
                    └─────────────────────────────────┘
```

**The language is defined by the LIR spec and the semantics doc, not by the Rust implementation.** If someone wanted to write a conforming Lumen VM in Zig, C, or WASM, the spec should make that possible. Rust is a pragmatic implementation choice — it gives us memory safety without a GC, good async for tool calls, excellent cross-compilation, and easy distribution as a single binary.

### The relationship between Lumen and Rust

This must be crystal clear: **Lumen programs do not know Rust exists.** They compile to LIR bytecode and execute in a VM. The VM happens to be implemented in Rust, the same way:

- Java programs run on a JVM written in C++
- Python programs run on CPython written in C
- Lua programs run on a VM written in C
- Erlang programs run on BEAM written in C

Lumen is not a Rust transpiler. It is not a Rust DSL. It is a language with its own type system, its own execution model, its own IR, and its own semantics. Rust is the scaffolding, not the building.

---

## 3) File format and UX

### Lumen Markdown

A Lumen program is a Markdown file (`.lm.md`) containing:

- **Doc-level directives** — lines starting with `@` that configure the program
- **Fenced code blocks** labeled `lumen` — the executable code
- **Everything else** — standard Markdown prose, headings, lists, links

The prose is not ignored. It is the documentation. The code blocks are the executable spine. Together they form a single artifact that is simultaneously a readable document and a runnable program.

### Example: a complete Lumen program

```markdown
@lumen 1
@package "acme.invoice_agent"
@trace sha256
@cache on
@profile dev

# Invoice Extraction Agent

This agent extracts structured invoice data from raw text using an LLM,
validates the output against a strict schema, and returns a typed record.

## Types

```lumen
record Invoice
  id:       String  where length(id) >= 6
  vendor:   String  where length(vendor) >= 1
  total:    Float   where total >= 0.0
  currency: String  where currency in ["USD", "EUR", "GBP"]
  items:    list[LineItem]
end

record LineItem
  description: String
  quantity:    Int    where quantity > 0
  unit_price:  Float  where unit_price >= 0.0
end
```

## Tool Setup

We need an LLM to do the extraction. We grant it a specific model,
a token budget, and a temperature ceiling.

```lumen
use tool llm.chat as Chat
grant Chat model "claude-sonnet-4-20250514" max_tokens 2000 temperature 0.0
```

## Extraction

The `extract` cell takes raw text and returns a validated Invoice.

```lumen
cell extract(text: String) -> result[Invoice, ValidationError]
  let response = Chat(
    role system:
      You are a strict JSON invoice extractor.
      Return only valid JSON matching the Invoice schema.
      Do not include any explanation.
    end,
    role user:
      Extract the invoice from this text:
      {text}
    end
  ) expect schema Invoice

  return response
end
```

## Entry Point

```lumen
cell run(text: String) -> Invoice
  let result = extract(text)
  match result
    ok(invoice) -> return invoice
    err(e)      -> halt("Validation failed: " + e.message)
  end
end
```
```

### Why `.lm.md` and not `.lumen`

1. GitHub, GitLab, VS Code, and every Markdown renderer already syntax-highlight fenced code blocks
2. You can read a Lumen program in any text viewer, even without tooling
3. AI models already understand Markdown structure deeply — Lumen programs are natively LLM-friendly
4. Documentation is not a separate step; it is the authoring surface

---

## 4) Lumen v1 language surface

We keep the language small and predictable. Every feature earns its place by being essential for agent workflows.

### v1 includes

**Types and data:**
- Primitives: `String`, `Int`, `Float`, `Bool`, `Bytes`, `Json`
- Containers: `list[T]`, `map[String, T]`
- `record` with named typed fields
- `enum` with variants
- Union types: `A | B`
- `result[Ok, Err]` tagged union (built-in)
- `where` constraint clauses on record fields

**Functions:**
- `cell` declarations with typed params and return types
- Cells are the unit of execution — pure or effectful

**Statements:**
- `let` bindings (immutable by default)
- `if` / `else`
- `for ... in` iteration
- `match` with pattern matching on enums, unions, and results
- `return`
- `halt(message)` — terminate execution with an error

**Expressions:**
- Literals: strings, ints, floats, bools, lists, maps, record literals
- Function calls, indexing, dot access
- String interpolation with `{expr}`
- Binary operators: `+`, `-`, `*`, `/`, `==`, `!=`, `<`, `<=`, `>`, `>=`, `and`, `or`
- Unary: `not`

**Tool system:**
- `use tool <id> as <Alias>` — import a tool
- `grant <Alias> <constraints>` — capability grants
- `expect schema T` — runtime validation attached to any expression

**LLM integration:**
- `role <name>:` blocks — typed prompt values
- String interpolation inside role blocks

**Intrinsics (pure, built-in):**
- `validate(schema T, value) -> result[T, ValidationError]`
- `hash(value) -> String`
- `diff(a, b) -> list[Patch]`
- `patch(value, patches) -> value`
- `redact(value, fields) -> value`
- `length(v)`, `count(v)`, `matches(v, pattern)`
- `trace_ref() -> TraceRef`

### v1 does not include

- Macros
- User-defined operator overloading
- Implicit coercions
- Exceptions (we use `result` and `halt`)
- Complex module/package/import system (single-doc compilation; multi-file comes in v1.1)
- Closures or first-class functions (cells are named, not anonymous — this keeps the trace system simple)
- Mutable variables (immutability by default; if mutation is needed, it's explicit via `let mut` in v1.1)
- Generics beyond built-in container types (v2)

### Why no closures in v1

This is an opinionated choice. Every cell is named and traced. If cells could capture variables from enclosing scopes and be passed around as values, the trace system would need to track closures as runtime objects with captured state. This adds significant complexity for a feature that agent workflows rarely need. Named cells with explicit parameters are sufficient for v1 and make traces trivially readable.

---

## 5) Lumen-Core: the bootstrap subset

Lumen-Core is the minimal subset that can compile the full Lumen v1 compiler when written in Lumen. It exists so the self-hosting compiler doesn't need the full language to compile itself.

### Lumen-Core includes exactly

- **Types:** `String`, `Int`, `Float`, `Bool`, `Bytes`, `Json`
- **Containers:** `list[T]`, `map[String, T]`
- **Data:** `record`, `enum`, `result[Ok, Err]`
- **Cells:** `cell` declarations with typed params and returns
- **Statements:** `let`, `if`, `match`, `return`, `halt`
- **Expressions:** calls, indexing, dot access, string interpolation, all operators
- **Tools:** `use tool`, `grant`
- **LLM:** `role` blocks
- **Validation:** `expect schema`
- **Intrinsics:** `hash`, `validate`, `length`, `count`

### What Lumen-Core omits

- `for` loops (use recursion or `match` on lists)
- `where` constraint clauses (compiler doesn't need runtime field validation)
- `diff`, `patch`, `redact` intrinsics
- Union types beyond `result`

**Goal:** Lumen-Core is expressive enough to write a lexer, parser, typechecker, and LIR emitter — the four phases of the compiler — while being small enough that the self-hosting compiler isn't wrestling with its own complexity.

---

## 6) The most important v1 decision: compiled to IR, run in a native VM

Not "interpret AST directly." Not "send JSON to a remote runner." Not "transpile to JavaScript."

### Pipeline

```
1. Extract Lumen blocks from Markdown
2. Lex with indentation tokens
3. Parse to AST (recursive descent + Pratt for expressions)
4. Resolve names
5. Typecheck
6. Lower to LIR
7. Run LIR in the Rust register VM
8. Tool calls dispatched via plugin system
9. Trace and cache keyed by canonical hashes
```

### Why a register VM instead of a stack VM

The original blueprint proposed a stack VM. After research, a **register-based VM** is the better choice for Lumen:

**Performance:** Academic literature consistently shows register VMs eliminate 40-47% of executed instructions compared to stack VMs. The 2005 Ertl & Gregg paper found ~26-32% reduction in execution time. Lua's switch from stack to register VM in 5.0 was a defining performance win. For a language that promises "instant" execution, this matters.

**Instruction density:** Register instructions encode operands directly. A register `ADD r0, r1, r2` is one instruction and one memory fetch. The equivalent stack code (`PUSH r1; PUSH r2; ADD; STORE r0`) is four instructions and four dispatches. Since Lumen's hot path is pure cell execution between tool calls, fewer dispatches = faster.

**Simpler compilation from AST:** Lumen's AST maps naturally to register allocation. Local variables get registers. Expressions compile to register-to-register operations. The compiler is straightforward because the target is close to the source semantics.

**Better fit for future JIT:** If we ever want to JIT-compile hot paths (v3+), register IR is much closer to machine code than stack IR. We're not painting ourselves into a corner.

**The Lua precedent:** Lua 5.0's register VM uses 32-bit fixed-width instructions with a 6-bit opcode and up to 18-bit operands. The entire instruction fits in one machine word. We adopt this proven design almost directly.

---

## 7) LIR (Lumen IR) design

This is the heart of performance and determinism.

### LIR goals

- Small instruction set (under 50 opcodes for v1)
- Fixed-width 32-bit instructions (Lua-style)
- Fast to execute in a register VM
- Pure vs effectful operations are structurally distinct
- Trace hooks at every effect boundary
- Canonical serialization for reproducible hashing

### Value model

LIR runtime values are tagged:

| Tag | Description |
|-----|-------------|
| `Null` | Absence of value |
| `Bool` | `true` / `false` |
| `Int` | 64-bit signed integer |
| `Float` | 64-bit IEEE 754 |
| `String` | Interned, immutable |
| `Bytes` | Content-addressed blob reference |
| `List` | Ordered sequence of values |
| `Map` | Sorted key-value pairs (canonical order) |
| `Record` | `type_id` + fields as sorted vector by `field_id` |
| `Union` | `tag_id` + payload value |
| `TraceRef` | `trace_id` + sequence number |

**Large value handling:** Strings > 1KB and all Bytes values are stored as content-addressed blobs:
- `blobs/sha256/<hash>`
- Runtime holds only the hash reference
- Trace stores only hashes plus small previews (first 128 bytes)

### Instruction format

Following Lua 5.0, we use 32-bit fixed-width instructions:

```
┌────────┬────────┬────────┬────────┐
│ opcode │   A    │   B    │   C    │   Format ABC (8+8+8+8 bits)
├────────┼────────┴────────┴────────┤
│ opcode │   A    │      Bx         │   Format ABx (8+8+16 bits)
├────────┼────────┴────────┴────────┤
│ opcode │         Ax               │   Format Ax  (8+24 bits)
└────────┴──────────────────────────┘
```

Each cell function gets up to 256 registers (8-bit register addresses). This is more than enough for any reasonable cell — if your cell needs 256 local variables, it's too complex.

### Instruction set v1

**Register and constant operations:**
- `LOADK A, Bx` — load constant Bx into register A
- `LOADNIL A, B` — set registers A through A+B to nil
- `LOADBOOL A, B, C` — load boolean B into A; if C, skip next instruction
- `MOVE A, B` — copy register B to register A

**Data construction:**
- `NEWLIST A, B` — create list from B values starting at register A+1
- `NEWMAP A, B` — create map from B key-value pairs starting at A+1
- `NEWRECORD A, Bx` — create record of type Bx, fields from subsequent registers
- `NEWUNION A, B, C` — create union with tag B and payload from register C

**Access:**
- `GETFIELD A, B, C` — A = B.field[C] (C is field_id constant)
- `SETFIELD A, B, C` — A.field[B] = C
- `GETINDEX A, B, C` — A = B[C] (list index or map key)
- `SETINDEX A, B, C` — A[B] = C

**Arithmetic and comparison:**
- `ADD A, B, C` — A = B + C
- `SUB A, B, C` — A = B - C
- `MUL A, B, C` — A = B * C
- `DIV A, B, C` — A = B / C
- `MOD A, B, C` — A = B % C
- `NEG A, B` — A = -B
- `EQ A, B, C` — if (B == C) != A then skip next instruction
- `LT A, B, C` — if (B < C) != A then skip next instruction
- `LE A, B, C` — if (B <= C) != A then skip next instruction
- `NOT A, B` — A = not B
- `AND A, B, C` — A = B and C
- `OR A, B, C` — A = B or C
- `CONCAT A, B, C` — A = B .. C (string concatenation)

**Control flow:**
- `JMP Ax` — unconditional jump by signed offset Ax
- `CALL A, B, C` — call cell in register A with B args, expect C results
- `RETURN A, B` — return B values starting from register A
- `HALT A` — terminate execution with error message in register A

**Intrinsics (pure):**
- `INTRINSIC A, B, C` — A = intrinsic[B](args starting at C)
  - B encodes: `LENGTH`, `COUNT`, `MATCHES`, `HASH`, `DIFF`, `PATCH`, `REDACT`, `VALIDATE`, `TRACEREF`

**Effects (these are the trace boundaries):**
- `TOOLCALL A, B, C, D` — call tool B with policy C, D args starting at A+1; result in A
- `SCHEMA A, B` — validate register A against schema type B; result replaces A

**Why this is enough:** The entire instruction set fits in ~35 opcodes. Every operation a Lumen v1 program can perform maps to one of these. Tool calls and schema validation are the only effect boundaries, so the trace system only needs to hook two instruction types.

### LIR module format

**v1: Canonical JSON** — slower to load but invaluable for debugging, diffing, and cross-implementation verification.

**v2: Compact binary** — when performance warrants it, a binary format with the same semantics.

LIR JSON structure:

```json
{
  "version": "1.0.0",
  "doc_hash": "sha256:...",

  "strings": ["extract", "Invoice", "id", ...],

  "types": [
    {
      "kind": "record",
      "name": "Invoice",
      "fields": [
        { "name": "id", "type": "String", "constraints": [...] },
        { "name": "total", "type": "Float", "constraints": [...] }
      ]
    }
  ],

  "cells": [
    {
      "name": "extract",
      "params": [{ "name": "text", "type": "String", "register": 0 }],
      "returns": { "type": "result", "ok": "Invoice", "err": "ValidationError" },
      "registers": 16,
      "constants": [...],
      "instructions": [
        { "op": "LOADK", "a": 1, "bx": 0 },
        { "op": "TOOLCALL", "a": 2, "b": 0, "c": 0, "d": 1 }
      ]
    }
  ],

  "tools": [
    { "alias": "Chat", "tool_id": "llm.chat", "version": "1.0.0" }
  ],

  "policies": [
    {
      "tool_alias": "Chat",
      "grants": { "model": "claude-sonnet-4-20250514", "max_tokens": 2000, "temperature": 0.0 }
    }
  ]
}
```

### Canonical hashing rules

These rules are non-negotiable. They are what make traces reproducible.

1. **Canonical JSON encoding** for all hashed values
2. Object keys sorted lexicographically
3. No whitespace between tokens
4. UTF-8 encoding, no BOM
5. Integers as JSON integers (no unnecessary `.0`)
6. Floats normalized: no trailing zeros, no leading zeros, scientific notation for very large/small values
7. Large values (> 1KB) referenced by blob hash, not embedded
8. Hash function: SHA-256 for v1 (widely supported, fast, deterministic)

**Cache key formula:**
```
sha256(tool_id + ":" + tool_version + ":" + policy_hash + ":" + canonical_args_hash)
```

This makes re-runs instant when inputs haven't changed.

---

## 8) Tool system: local-first, capability-scoped, MCP-compatible

This is where Lumen becomes "AI-native" without becoming "slow remote JSON."

### The critical insight about MCP

The Model Context Protocol (MCP) became the de facto standard for AI tool integration in 2025. It was adopted by OpenAI, Google DeepMind, Microsoft, and donated to the Linux Foundation. By the time Lumen ships, MCP will be ubiquitous.

**Lumen must be MCP-compatible from day one.** This is not optional. It is the difference between "interesting research language" and "language with an ecosystem."

### Dual tool interface

Lumen supports two tool integration modes:

**1. Native tools (subprocess protocol):**
Fast, local, minimal overhead. For tools that ship with Lumen or are installed locally.

```
Runner ──stdin──▶ Tool Process ──stdout──▶ Runner
         JSON request            JSON response
```

**2. MCP tools (MCP client):**
The Lumen runtime acts as an MCP client. Any MCP server — local or remote — becomes a Lumen tool automatically.

```lumen
use tool mcp "http://localhost:3000/mcp" as GithubTools
grant GithubTools.create_issue repo "myorg/myrepo" timeout_ms 10000
```

The compiler resolves the MCP server's tool manifest at compile time (or first run), validates that the granted capabilities match the server's declared capabilities, and generates the appropriate `TOOLCALL` instructions.

### Why both modes

- **Native tools** are faster (no JSON-RPC overhead, no MCP handshake) and better for core operations like HTTP, file I/O, and LLM calls
- **MCP tools** give Lumen instant access to thousands of existing integrations — Google Drive, Slack, Notion, GitHub, databases, and everything else the MCP ecosystem provides
- Users can choose: bundle critical tools as native for speed, connect optional tools via MCP for breadth

### Native tool manifest (v1)

Each native tool ships as a directory with a manifest:

```json
{
  "tool_id": "http.get",
  "version": "1.0.0",
  "description": "Fetch a URL via HTTP GET",
  "input_schema": {
    "type": "record",
    "fields": {
      "url": { "type": "String", "constraints": [{ "kind": "matches", "pattern": "^https?://" }] },
      "headers": { "type": "map[String, String]", "optional": true }
    }
  },
  "output_schema": {
    "type": "record",
    "fields": {
      "status": { "type": "Int" },
      "body": { "type": "Bytes" },
      "headers": { "type": "map[String, String]" }
    }
  },
  "capabilities": {
    "supports_domain_allowlist": true,
    "supports_timeout_ms": true,
    "supports_rate_limit": true,
    "supports_max_response_bytes": true
  },
  "exec": {
    "command": "lumen-tool-http-get",
    "args": []
  }
}
```

### Capability use in Lumen

```lumen
# Import and constrain a tool
use tool http.get as HttpGet
grant HttpGet
  domain "api.example.com"
  domain "api.backup.com"
  timeout_ms 5000
  rate_limit 10 per_minute
  max_response_bytes 1048576

# Import an MCP tool
use tool mcp "https://mcp.notion.com/sse" as Notion
grant Notion.create_page workspace "my-workspace" timeout_ms 15000

# Use it in a cell
cell fetch_data(url: String) -> Bytes
  let response = HttpGet(url: url)
  match response.status
    200 -> return response.body
    _   -> halt("HTTP error: " + response.status)
  end
end
```

### Compile-time and runtime enforcement

The compiler checks:
- Tool alias exists and resolves to a manifest
- Grant clauses are compatible with the tool's declared capabilities
- Every `TOOLCALL` instruction has a corresponding grant
- No tool is called without being imported

The runtime enforces (even if the compiler is wrong or bypassed):
- Domain allowlists
- Rate limits
- Timeouts
- Budget caps
- Response size limits
- Redaction rules

**Double enforcement is deliberate.** The compiler catches errors early. The runtime prevents exploits. This is the object-capability model applied: possessing a tool reference (via `use tool`) is necessary but not sufficient — you must also hold a valid grant (capability token) that constrains how you can use it.

### v1 bundled tools

Lumen ships with these native tools:

| Tool ID | Description |
|---------|-------------|
| `http.get` | HTTP GET with domain allowlist |
| `http.post` | HTTP POST with domain allowlist |
| `llm.chat` | LLM chat completion (supports multiple providers) |
| `fs.read` | Read a local file (path allowlist) |
| `fs.write` | Write a local file (path allowlist) |
| `json.parse` | Parse JSON string to value |
| `json.emit` | Serialize value to JSON string |

Everything else comes via MCP or community-built native tools.

---

## 9) LLM integration that is fast-feeling

LLM calls are inherently slow (100ms–30s). Lumen makes them feel good through caching, typed prompts, and structured output validation.

### Design rules

1. **Every LLM call is a tool call.** Same capability system, same trace events, same caching.
2. **Every call is cached by:** `tool_id + version + model + temperature + canonical_messages_hash + policy_hash`
3. **Trace stores:** prompt hash, output hash, latency, token counts, optional preview
4. **Offline mode:** forbids re-calling and uses cached outputs or errors. Perfect for testing and CI.
5. **Temperature 0 calls are deterministic-ish.** Same cache key = same output. We lean into this.

### Role blocks are typed values

```lumen
role system:
  You are a strict JSON invoice extractor.
  Return only valid JSON matching the Invoice schema.
end

role user:
  Extract the invoice from this text:
  {raw_text}
end
```

`role system:` produces a value of type `RoleText`. The `Chat` tool expects a sequence of role messages. The compiler desugars this into a canonical structure:

```json
{
  "messages": [
    { "role": "system", "content": "You are a strict JSON invoice extractor..." },
    { "role": "user", "content": "Extract the invoice from this text:\n..." }
  ]
}
```

### Schema-validated structured output

The killer feature:

```lumen
let invoice = Chat(
  role system: system_prompt,
  role user: "Extract: " + text
) expect schema Invoice
```

`expect schema Invoice` does:
1. Parse the LLM output as JSON
2. Validate against the `Invoice` record schema including `where` constraints
3. If valid: return a typed `Invoice` value
4. If invalid: return a `ValidationError` (strict mode halts; soft mode returns `result`)

### Repair loop (v1.1, not v1)

```lumen
let invoice = Chat(...) expect schema Invoice
  on_fail retry 2 with_errors
```

This is powerful but complex. In v1 we keep it strict and deterministic. v1.1 adds `repair(schema, value, errors, via Chat)` as a traceable effectful operation.

---

## 10) Schema system: types that validate at runtime

If TypeScript's play is "types that document," Lumen's play is "types that enforce."

### Schema definitions

Records and enums are schemas by default. Constraints are attached with `where`.

```lumen
record EmailAddress
  local: String where length(local) >= 1 and matches(local, "^[^@]+$")
  domain: String where matches(domain, "^[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$")
end

enum Priority
  Low
  Medium
  High
  Critical
end

record Ticket
  id:       String   where length(id) >= 6
  title:    String   where length(title) >= 1 and length(title) <= 200
  priority: Priority
  assignee: EmailAddress | Null
end
```

### Validation behavior v1

- Unknown fields: **rejected** (no open records in v1)
- Missing required fields: **rejected**
- No implicit null — use `T | Null` explicitly
- Union types must match exactly one variant; ambiguity is a compile error
- Constraints are evaluated at runtime, not compile time (they can reference the field's value)

### Two validation modes

**Strict mode** (default — used by `expect schema`):
```lumen
let ticket = parse_json(data) expect schema Ticket
# If invalid: cell halts with ValidationError
```

**Soft mode** (explicit — used by `validate` intrinsic):
```lumen
let result = validate(schema Ticket, parse_json(data))
match result
  ok(ticket) -> process(ticket)
  err(errors) -> log_errors(errors)
end
```

### Why this matters for AI

LLMs return unstructured text. Every agent framework has some version of "parse the output and hope it matches." Lumen makes validation a first-class language feature:

- Schemas are defined in the same language as the logic
- Validation is a single expression, not a library call
- Errors are typed and actionable (field name, constraint violated, actual value)
- The trace records exactly what was validated, what passed, and what failed

---

## 11) Trace system: auditable and cacheable by construction

This is the feature that makes Lumen worth the learning curve. No other language provides this as a structural guarantee.

### Trace store layout

```
.lumen/
  trace/
    <run_id>.jsonl          # Hash-chained event log
  blobs/
    sha256/
      <hash>                # Content-addressed blob storage
  cache/
    <tool_id>/
      <cache_key>.json      # Cached tool outputs
```

### Trace event kinds v1

| Event | Description |
|-------|-------------|
| `run_start` | Program execution begins |
| `cell_start` | Cell execution begins |
| `cell_end` | Cell returns (includes output hash) |
| `tool_call` | Tool invocation (includes input/output hashes, latency) |
| `schema_validate` | Schema validation (includes result) |
| `error` | Runtime error |
| `run_end` | Program execution completes |

### Every event has

```json
{
  "seq": 7,
  "kind": "tool_call",
  "prev_hash": "sha256:abc123...",
  "hash": "sha256:def456...",
  "timestamp": "2026-02-12T10:30:00.000Z",
  "doc_hash": "sha256:...",
  "cell": "extract",
  "tool_id": "llm.chat",
  "tool_version": "1.0.0",
  "inputs_hash": "sha256:...",
  "outputs_hash": "sha256:...",
  "policy_hash": "sha256:...",
  "latency_ms": 1423,
  "cached": false
}
```

### What you can do with traces

1. **Replay:** Re-run a program using cached tool outputs. Deterministic pure computation + cached effects = identical results.
2. **Diff:** Compare two runs. See exactly which tool call returned different results.
3. **Audit:** Prove what an agent did. Every action is logged with cryptographic hashes.
4. **Debug:** Find the exact tool call that returned unexpected data.
5. **Cost tracking:** Sum LLM token usage, API call counts, latency across runs.
6. **Cache invalidation:** When a tool version changes, invalidate only affected cache entries.

### This is the competitive moat

LangChain has LangSmith. CrewAI has their dashboard. But these are *observability layers bolted onto opaque Python*. Lumen's traces are a *structural property of the language*. You cannot write a Lumen program that doesn't produce a trace. The trace format is specified, canonical, and hash-chained. This is the difference between "we log stuff" and "we prove stuff."

---

## 12) The Rust VM runner

This is what makes Lumen feel native.

### VM architecture

```rust
struct VM {
    // Register file: per-cell, up to 256 registers
    registers: Vec<Value>,

    // Call stack
    frames: Vec<CallFrame>,

    // Interned strings (shared across all execution)
    strings: StringTable,

    // Type registry
    types: TypeTable,

    // Tool dispatcher
    tools: ToolRunner,

    // Trace emitter
    trace: TraceEmitter,

    // Cache store
    cache: CacheStore,
}

struct CallFrame {
    cell_id: u32,
    base_register: u32,
    return_register: u32,
    ip: u32,  // instruction pointer into cell's bytecode
}
```

### Performance strategy

1. **Interned strings:** All string comparisons are pointer equality after interning
2. **Arena allocation:** AST nodes, type descriptors, and compile-time structures use arena allocation — one big allocation, freed at once
3. **Register file on the stack:** The register array for each cell lives on the Rust call stack when possible, avoiding heap allocation for pure cells
4. **Zero-copy blob references:** Large values are never copied, only their hash references
5. **Tight dispatch loop:** The VM core is a single `match` on the opcode byte, unrolled by the Rust compiler into a jump table

### Concurrency model

v1: **Sequential semantics.** Cells execute top-to-bottom. Tool calls block.

This is the right choice for v1 because:
- It's simple to implement
- It's simple to reason about
- Traces are naturally ordered
- Most agent workflows are inherently sequential (step 1 feeds step 2)

v1.1 adds `@parallel` for independent tool calls:

```lumen
@parallel
let weather = HttpGet(url: "https://api.weather.com/...")
let news = HttpGet(url: "https://api.news.com/...")
@end
```

The compiler proves the two calls are independent (no data flow between them) and the runtime executes them concurrently.

### Error model

All runtime errors are typed and traced:

| Error Type | Description |
|------------|-------------|
| `ValidationError` | Schema validation failed |
| `ToolError` | Tool returned an error |
| `PolicyError` | Grant violation (e.g., domain not in allowlist) |
| `TypeError` | Should never happen if compiler is correct |
| `RuntimeError` | Index out of bounds, division by zero, etc. |

Every error is a trace event. You can always find out exactly what went wrong and where.

---

## 13) The compiler frontend

### Phases

```
1. Markdown extraction    → list of (code_block, source_location)
2. Lexing with INDENT/DEDENT tokens
3. Parsing to AST         → recursive descent + Pratt for expressions
4. Name resolution        → resolve cells, types, tool aliases
5. Typechecking           → bidirectional type inference
6. Constraint validation  → verify `where` clauses are well-formed
7. Lowering to LIR        → register allocation + instruction emission
8. Emit LIR module        → canonical JSON (+ optional sourcemap)
```

### Parsing strategy

- **Indentation-sensitive** like Python: the lexer emits `INDENT` and `DEDENT` tokens
- **Statements** parsed with recursive descent (simple, debuggable)
- **Expressions** parsed with a Pratt parser (handles precedence cleanly)
- This combination is fast, produces excellent error messages, and is well-understood

### Register allocation

Since Lumen v1 has no closures and no mutable variables, register allocation is simple:

1. Each cell parameter gets a register (starting from r0)
2. Each `let` binding gets a register (allocated sequentially)
3. Temporary expression results get registers (freed after the expression)
4. The compiler tracks the maximum register count for each cell (stored in LIR)

This is a linear scan over the AST. No graph coloring, no spilling. If a cell exceeds 256 registers, it's a compile error (and also a code smell).

### Diagnostics

Must be excellent. This is where new languages live or die.

- Point to the exact line and column **in the Markdown file** (not just the code block)
- Show the code fence region and cell name
- Give expected tokens, types, and suggestions
- "Did you mean X?" for misspelled identifiers
- "Tool Y requires grant for capability Z" for missing grants

Example:

```
error[E0012]: type mismatch in cell `extract`
  --> invoice_agent.lm.md:42:15
   |
42 |   let total = item.price * item.quantity
   |               ^^^^^^^^^^^^^^^^^^^^^^^^^
   |               expected Float, got Int
   |
   help: use `to_float(item.quantity)` to convert
```

---

## 14) Editor tooling: the TypeScript-competitive LSP

### lumen-lsp (Rust)

The LSP server is written in Rust for speed. It shares the compiler frontend (lexer, parser, typechecker) as a library crate.

### Core features for v1

| Feature | Description |
|---------|-------------|
| Diagnostics | Syntax errors, type errors, grant violations — live as you type |
| Go to definition | Cells, records, enums, roles, tool aliases |
| Hover types | Show inferred types for any expression |
| Document symbols | Outline by cells and types (shows in VS Code sidebar) |
| Formatting | Auto-format Lumen blocks (preserve Markdown) |
| Run cell | Execute a specific cell from the editor |
| Inline results | Show return values as inline comments (optional) |
| Completion | Suggest cell names, field names, tool aliases, intrinsics |

### Incremental performance

The LSP maintains per-document state:

```
Document State:
  - Extracted code blocks (with source mappings)
  - AST for each block
  - Type tables
  - Hash of each cell block
```

On edit:
1. Re-extract affected code block (fast — just find the fenced region)
2. Re-lex and re-parse only the changed block
3. Re-typecheck only cells that depend on changed definitions
4. Since each doc is a single module, the dependency graph is small

**Target:** < 50ms from keystroke to diagnostics for a typical document. This is achievable because Lumen docs are small (single-file programs) and the type system is simple (no generics, no higher-kinded types, no complex inference).

### VS Code extension

Ships as a `.vsix` with:
- Language registration for `.lm.md` files
- Syntax grammar for Lumen blocks inside Markdown fences
- LSP client connecting to `lumen-lsp`
- "Run Cell" command in the command palette and as a code lens
- "Show Trace" command to view the last run's trace

---

## 15) Bootstrapping path to self-hosting

This is the "proper language" story, without OCaml.

### Stage 0: Rust compiler (lumen-rs)

The production compiler, written in Rust. Implements full Lumen v1. Produces LIR. This is the only compiler for the first several milestones.

Alongside the compiler, we build the **conformance test suite**: a corpus of Lumen programs with their expected LIR output, trace shapes, and runtime behavior. This suite is the source of truth for "what is correct Lumen."

### Stage 1: Lumen compiler written in Lumen-Core (lumencc)

Write `lumencc.lm.md` — a Lumen program that:
1. Reads a `.lm.md` file (via `fs.read` tool)
2. Extracts Markdown code blocks
3. Lexes and parses Lumen
4. Typechecks
5. Emits LIR (via `fs.write` tool)

At first, `lumencc` only needs to compile Lumen-Core. It gradually expands.

### Stage 2: Self-hosting loop

```
1. Use lumen-rs to compile lumencc.lm.md → lumencc.lir.json
2. Run lumencc.lir.json on the VM to compile itself → lumencc-2.lir.json
3. Compare: sha256(lumencc.lir.json) == sha256(lumencc-2.lir.json)?
4. If yes: stable fixed point. Self-hosting achieved.
5. Run lumencc-2 against the conformance test suite.
6. If all tests pass: lumencc is a correct Lumen compiler.
```

### Stage 3: Validation

The conformance test suite replaces the OCaml cross-check. It provides:
- **Positive tests:** "This program should produce this LIR and this trace"
- **Negative tests:** "This program should produce this compile error"
- **Property tests:** "For any valid program, the LIR is deterministic" (fuzzing)
- **Round-trip tests:** "Compile → run → trace → replay produces identical results"

This is more rigorous than a second compiler implementation, because it tests *behavior*, not just *output agreement*.

---

## 16) Security model

Lumen must be safe by default. The capability system is not optional.

### Defaults

- No network access without explicit `use tool` and `grant`
- No filesystem access without explicit `use tool` and `grant`
- Grants are allowlist-based (deny by default)
- Tools declare what policy knobs they support; the runtime rejects unsupported grants
- The runtime enforces grants even if the compiler is bypassed

### The object-capability model

Lumen's security model is based on the object-capability tradition (E language, Pony, Monte, Wyvern):

1. **No ambient authority.** A cell cannot access the network, filesystem, or any external resource without a tool capability.
2. **Capabilities are explicit.** Every tool reference is imported via `use tool` and constrained via `grant`.
3. **Capabilities are attenuated.** You can grant a tool less authority (narrower domain list, lower rate limit) but never more than the tool manifest allows.
4. **Capabilities are traceable.** Every use of a capability is a trace event.

This is not just security theater. It's the foundation of the auditability story. When you read a Lumen program, the `use tool` and `grant` declarations at the top tell you *exactly* what the program can do. There are no hidden imports, no ambient globals, no ambient network access.

### Sandboxing roadmap

| Version | Sandboxing |
|---------|-----------|
| v1 | Subprocess tools (OS process isolation) |
| v1.1 | Linux: seccomp-bpf or bubblewrap |
| v1.1 | macOS: sandbox-exec |
| v2 | WASM-based tool sandboxing |

v1 ships without hardened sandboxing because subprocess isolation is already reasonable, and getting the language right matters more than getting the sandbox right first.

---

## 17) Determinism guarantees

We are precise about what is and is not deterministic.

### We guarantee

| Property | Guarantee |
|----------|-----------|
| Compilation | Identical source + identical tool manifests → identical LIR (bit-for-bit) |
| Cache keys | Identical tool + version + policy + args → identical cache key |
| Trace hashes | Identical run inputs + cached outputs → identical trace hash chain |
| Replay | Using cached tool outputs, replay produces identical results |
| Pure cells | Same inputs → same outputs (always) |

### We do not guarantee

| Property | Why not |
|----------|---------|
| LLM output determinism | Even temperature=0 has variation across API calls |
| Tool output stability | External APIs return different data over time |
| Execution timing | Wall-clock time varies |

### The practical upshot

If you run a Lumen program twice with the same inputs and the cache is warm, the second run is instant and produces the same trace. If you clear the cache, tool calls re-execute and may return different results — but the trace records the difference, so you can see exactly what changed.

---

## 18) Repository layout

One repo, monorepo style. Clean separation of concerns.

```
lumen/
  README.md
  LICENSE
  Makefile                          # `make test`, `make bootstrap`, `make release`

  docs/
    spec/
      lumen-v1.md                   # Language specification
      lir-v1.md                     # IR specification
      tool-manifest-v1.md           # Tool plugin specification
      trace-v1.md                   # Trace format specification
      mcp-bridge-v1.md              # MCP integration specification
    tutorials/
      getting-started.md
      first-agent.md
      custom-tool.md

  examples/
    invoice_agent.lm.md
    web_scraper.lm.md
    rag_evaluator.lm.md
    code_reviewer.lm.md

  conformance/                       # The test suite that replaces OCaml
    positive/                        # Programs with expected LIR + traces
    negative/                        # Programs with expected compile errors
    properties/                      # Property-based test generators
    runner.rs                        # Conformance test harness

  rust/
    lumen-compiler/                  # The compiler (library crate)
      Cargo.toml
      src/
        lib.rs
        markdown/
          extract.rs                 # Markdown → code blocks
        compiler/
          lexer.rs                   # Indentation-aware lexer
          tokens.rs
          parser.rs                  # Recursive descent + Pratt
          ast.rs                     # AST data types
          resolve.rs                 # Name resolution
          typecheck.rs               # Bidirectional type inference
          constraints.rs             # Where-clause validation
          lower.rs                   # AST → LIR lowering
          regalloc.rs                # Register allocation
          emit.rs                    # LIR serialization

    lumen-vm/                        # The VM (library crate)
      Cargo.toml
      src/
        lib.rs
        vm.rs                        # Register VM dispatch loop
        values.rs                    # Tagged value representation
        strings.rs                   # String interning
        types.rs                     # Runtime type registry

    lumen-runtime/                   # Trace, cache, tool dispatch
      Cargo.toml
      src/
        lib.rs
        trace/
          store.rs                   # JSONL trace writer
          hasher.rs                  # Canonical hashing
          events.rs                  # Event types
        cache/
          store.rs                   # Content-addressed cache
          keys.rs                    # Cache key computation
        tools/
          manifest.rs                # Tool manifest loader
          runner.rs                  # Subprocess tool runner
          mcp.rs                     # MCP client bridge
          policy.rs                  # Grant enforcement

    lumen-cli/                       # The CLI (binary crate)
      Cargo.toml
      src/
        main.rs
        commands/
          check.rs                   # lumen check
          run.rs                     # lumen run
          fmt.rs                     # lumen fmt
          trace.rs                   # lumen trace
          cache.rs                   # lumen cache

    lumen-lsp/                       # Language server (binary crate)
      Cargo.toml
      src/
        main.rs
        server.rs
        document.rs                  # Per-document state
        diagnostics.rs
        symbols.rs
        completion.rs
        formatting.rs
        hover.rs

  tools/                             # Bundled native tools
    http_get/
      tool.json
      src/main.rs
    http_post/
      tool.json
      src/main.rs
    llm_chat/
      tool.json
      src/main.rs
    fs_read/
      tool.json
      src/main.rs
    fs_write/
      tool.json
      src/main.rs

  editors/
    vscode/
      package.json
      syntaxes/lumen.tmLanguage.json
      src/extension.ts

  bootstrap/                         # Self-hosting compiler (when ready)
    lumencc.lm.md
```

---

## 19) CLI experience

### Commands v1

```bash
# Check a file for errors (fast — no execution)
lumen check invoice_agent.lm.md

# Run a file (executes the `run` cell if present)
lumen run invoice_agent.lm.md

# Run a specific cell
lumen run invoice_agent.lm.md --cell extract

# Run with input from stdin
echo "Invoice #12345..." | lumen run invoice_agent.lm.md --stdin text

# Format Lumen blocks in a file
lumen fmt invoice_agent.lm.md

# View the last trace
lumen trace last

# View a specific trace
lumen trace show <run_id>

# Diff two traces
lumen trace diff <run_id_1> <run_id_2>

# List cached tool outputs
lumen cache ls

# Clear cache
lumen cache clear

# Clear cache for a specific tool
lumen cache clear --tool llm.chat

# Replay a run using cached outputs (deterministic)
lumen replay <run_id>

# Show installed tools
lumen tools ls

# Validate a tool manifest
lumen tools check tools/http_get/tool.json

# Initialize a new Lumen project
lumen init my_agent
```

### Lock file

`.lumen/lock.json`:

```json
{
  "lumen_version": "1.0.0",
  "tools": {
    "http.get": { "version": "1.0.0", "hash": "sha256:..." },
    "llm.chat": { "version": "1.0.0", "hash": "sha256:..." }
  },
  "mcp_servers": {
    "notion": { "url": "https://mcp.notion.com/sse", "manifest_hash": "sha256:..." }
  }
}
```

With `@lock strict`, tool versions and manifests must match the lock file. This ensures reproducible builds.

---

## 20) Implementation milestones

This is the shortest path to a real, usable language. Every milestone produces something you can demo.

### Milestone 0: Spec freeze (2 weeks)

Lock the v1 specifications. These are contracts — changing them later breaks everything.

**Deliverables:**
- `docs/spec/lumen-v1.md` — complete grammar, type system, semantics
- `docs/spec/lir-v1.md` — instruction set, module format, canonical hashing
- `docs/spec/tool-manifest-v1.md` — native tool protocol
- `docs/spec/trace-v1.md` — event types, hash chaining, blob storage

**Exit criteria:** Spec review by at least one person who isn't the author. No ambiguities.

### Milestone 1: VM that executes LIR (3 weeks)

Build the register VM. Test it with hand-authored LIR files.

**Deliverables:**
- `lumen-vm` crate with dispatch loop, value representation, string interning
- `lumen-runtime` crate with trace store, blob store, cache store
- A hand-written LIR file that computes fibonacci and emits a trace
- `lumen run-lir fib.lir.json` works and produces a valid trace

**Exit criteria:** VM passes a suite of hand-written LIR test cases covering all opcodes.

### Milestone 2: Compiler for Lumen-Core → LIR (4 weeks)

Build the compiler frontend. The biggest milestone.

**Deliverables:**
- `lumen-compiler` crate: markdown extraction, lexer, parser, typechecker, lowerer, emitter
- `lumen-cli` with `lumen check` and `lumen run`
- `examples/invoice_agent.lm.md` compiles and runs (with stub tools)

**Exit criteria:** All conformance/positive tests pass. Error messages point to correct Markdown locations.

### Milestone 3: Tool plugin system + LLM integration (3 weeks)

Make tools real. This is where Lumen becomes useful.

**Deliverables:**
- Native tool runner (subprocess protocol)
- MCP client bridge
- Bundled tools: `http.get`, `http.post`, `llm.chat`, `fs.read`, `fs.write`
- Grant enforcement at runtime
- Example doc that fetches data, calls an LLM, validates output, and produces a trace

**Exit criteria:** `examples/invoice_agent.lm.md` runs end-to-end with a real LLM call, cached on second run.

### Milestone 4: Conformance test suite (2 weeks, parallel with M3)

Build the test infrastructure that replaces the OCaml cross-check.

**Deliverables:**
- `conformance/` directory with 100+ test cases
- Positive tests: source → expected LIR hash + expected trace shape
- Negative tests: source → expected error type and location
- Property tests: round-trip determinism, hash stability
- CI integration: `make test-conformance`

**Exit criteria:** 100% pass rate. Any compiler change that breaks a test is a regression.

### Milestone 5: LSP v1 (3 weeks)

Make the editor experience competitive.

**Deliverables:**
- `lumen-lsp` binary
- VS Code extension with syntax highlighting, diagnostics, go-to-def, hover, completion
- "Run Cell" code lens
- < 50ms response time for diagnostics

**Exit criteria:** A developer can write a Lumen program in VS Code with real-time feedback that feels responsive.

### Milestone 6: Documentation and launch (2 weeks)

**Deliverables:**
- `docs/tutorials/getting-started.md`
- `docs/tutorials/first-agent.md`
- `docs/tutorials/custom-tool.md`
- Website with installation instructions
- Pre-built binaries for Linux, macOS, Windows

**Exit criteria:** A developer who has never seen Lumen can install it and run an example in under 5 minutes.

### Milestone 7: Self-hosting compiler in Lumen (ongoing)

This is the "proper language" milestone. It doesn't block v1 launch.

**Deliverables:**
- `bootstrap/lumencc.lm.md`
- `make bootstrap` produces matching hashes
- `lumencc` passes the conformance suite

**Exit criteria:** Lumen can compile itself. The bootstrapping loop is stable.

### Timeline estimate

| Milestone | Duration | Running total |
|-----------|----------|---------------|
| M0: Spec freeze | 2 weeks | 2 weeks |
| M1: VM | 3 weeks | 5 weeks |
| M2: Compiler | 4 weeks | 9 weeks |
| M3: Tools | 3 weeks | 12 weeks |
| M4: Conformance (parallel) | 2 weeks | 12 weeks |
| M5: LSP | 3 weeks | 15 weeks |
| M6: Launch prep | 2 weeks | 17 weeks |

**~4 months to a usable v1.** This is aggressive but realistic for a focused effort, especially with AI-assisted development. The self-hosting milestone (M7) is unbounded and pursued after launch.

---

## 21) The competitive positioning

### What exists today

| Tool | Category | Limitation |
|------|----------|------------|
| LangChain/LangGraph | Python framework | No compile-time checks, ambient authority, no traces |
| CrewAI | Python framework | Role-based but not capability-secured |
| AutoGen/AG2 | Python framework | Multi-agent but opaque execution |
| OpenAI Agents SDK | Python framework | OpenAI-specific, no trace system |
| Google ADK | Python framework | GCP-specific |
| Dana (AI Alliance) | Python-like DSL | Python runtime, no IR, no trace hashing |
| TypeScript + Zod | Manual approach | No capability model, no caching, no traces |

### Where Lumen wins

**Against Python frameworks:** Lumen is compiled, fast, and provides structural guarantees that Python cannot. You don't bolt on observability — it's built into the language. The capability model prevents entire classes of security bugs.

**Against TypeScript:** Lumen is dramatically smaller in surface area, which is a feature, not a bug. TypeScript is a general-purpose language; Lumen is purpose-built for agent workflows. Schema validation, tool caching, and trace emission are zero-config in Lumen; they're DIY in TypeScript.

**Against Dana:** Dana is a DSL that runs on Python. Lumen is a compiled language with its own IR and VM. The performance characteristics are categorically different. Lumen's trace system is hash-chained and content-addressed; Dana's observability is framework-level logging.

### The pitch

For developers: *"Write executable Markdown. Get type-safe schemas, capability-scoped tools, and cryptographic traces for free."*

For engineering leads: *"Stop shipping prompt spaghetti that you can't audit. Lumen makes AI workflows reproducible, cacheable, and provable."*

For security teams: *"Every tool call is capability-scoped, every action is traced, and the trace is hash-chained. You can prove what the agent did."*

---

## 22) The final choices, coherent and final

| Decision | Choice | Rationale |
|----------|--------|-----------|
| File format | Markdown with fenced `lumen` blocks (`.lm.md`) | LLM-friendly, readable, Git-diffable |
| Compilation target | LIR (Lumen IR) | Language identity separate from implementation |
| VM architecture | Register-based (Lua-style, 32-bit fixed-width instructions) | 30-40% faster than stack VM, better JIT path |
| LIR format v1 | Canonical JSON | Debuggable, diffable, cross-implementation friendly |
| Tool system | Native subprocess + MCP client bridge | Performance for core tools, ecosystem for everything else |
| Capability model | Object-capability with explicit grants | Structural security, not bolted-on |
| Trace system | JSONL, hash-chained, content-addressed blobs | Replayable, diffable, provable |
| Cache system | Content-addressed by canonical hash of tool + args + policy | Instant re-runs |
| Compiler | Rust (single implementation) | Fast, safe, distributable as single binary |
| Correctness assurance | Conformance test suite + property-based fuzzing | More rigorous than dual-implementation, less maintenance |
| Self-hosting | Lumen compiler written in Lumen-Core (milestone 7) | Proper language legitimacy, not blocking v1 |
| Editor tooling | Rust LSP + VS Code extension | TypeScript-competitive DX from day one |
| Bootstrap language | None needed (Rust compiler is v1, Lumen compiler is v2) | Clean, simple, no OCaml baggage |

---

## 23) What to build first

If you're ready to start, the immediate next artifacts are:

1. **Lumen-Core formal grammar** — EBNF or PEG, unambiguous, testable
2. **LIR v1 JSON Schema** — machine-readable spec that validators can check against
3. **Tool protocol specification** — exact JSON shapes for request/response, including MCP bridge behavior
4. **Three example programs with expected traces** — the first conformance tests
5. **VM dispatch loop pseudocode** — exact register semantics for every opcode

I'd recommend starting with (1) and (2) simultaneously, because the grammar defines what the compiler must parse and the LIR schema defines what it must emit. Everything flows from those two documents.

Let me know which direction you want to go and I'll start generating specs.