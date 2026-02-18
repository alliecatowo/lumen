# Lumen Programming Language: Complete Technical Reference

## Table of Contents
1. [Architecture Overview](#1-architecture-overview)
2. [Compiler Pipeline](#2-compiler-pipeline)
3. [VM & Runtime Architecture](#3-vm--runtime-architecture)
4. [Full Execution Flow](#4-full-execution-flow)
5. [LIR Bytecode & Instruction Encoding](#5-lir-bytecode--instruction-encoding)
6. [Effect System](#6-effect-system)
7. [Type System](#7-type-system)
8. [Tool System](#8-tool-system)
9. [CLI & Package Management](#9-cli--package-management)
10. [Module Dependencies](#10-module-dependencies)
11. [Key Algorithms & Data Structures](#11-key-algorithms--data-structures)

---

## 1. Architecture Overview

### 1.1 Workspace Structure

```mermaid
graph TB
    subgraph "Workspace Root"
        ROOT["/home/Allie/develop/lumen/"]
        CONFIG["Cargo.toml<br/>lumen.toml<br/>SPEC.md"]
    end
    
    subgraph "rust/ - 16 Crates"
        CORE["lumen-core<br/>Shared types<br/>LIR, Values, Types"]
        COMP["lumen-compiler<br/>7-stage compiler<br/>Lexer→Parser→Lower"]
        RT["lumen-rt<br/>VM & Runtime<br/>Intrinsics, Processes"]
        CLI["lumen-cli<br/>CLI & Tools<br/>Check, Run, Fmt, Pkg"]
        LSP["lumen-lsp<br/>Language Server<br/>Hover, Goto, Complete"]
        CODEGEN["lumen-codegen<br/>JIT + WASM<br/>Cranelift backend"]
        TENSOR["lumen-tensor<br/>Tensor ops<br/>Autodiff"]
        
        subgraph "Providers (8)"
            HTTP["provider-http"]
            JSON["provider-json"]
            FS["provider-fs"]
            MCP["provider-mcp"]
            GEMINI["provider-gemini"]
            ENV["provider-env"]
            CRYPTO["provider-crypto"]
        end
        
        WASM["lumen-wasm<br/>WASM bindings<br/>(excluded)"]
        BENCH["lumen-bench<br/>Benchmarks"]
    end
    
    ROOT --> CONFIG
    ROOT --> CORE
    ROOT --> COMP
    ROOT --> RT
    ROOT --> CLI
    ROOT --> LSP
    ROOT --> CODEGEN
    ROOT --> TENSOR
    ROOT --> WASM
    ROOT --> BENCH
    
    CLI --> HTTP
    CLI --> JSON
    CLI --> FS
    CLI --> MCP
    CLI --> GEMINI
    CLI --> ENV
    CLI --> CRYPTO
    
    COMP --> CORE
    RT --> CORE
    CLI --> COMP
    CLI --> RT
    LSP --> COMP
    LSP --> RT
    CODEGEN --> RT
    CODEGEN --> CORE
```

### 1.2 Directory Tree

```
/home/Allie/develop/lumen/
├── Cargo.toml                    # Workspace root
├── lumen.toml                    # Runtime config
├── SPEC.md                       # Language specification
├── CLAUDE.md                     # AI agent guidance
├── AGENTS.md                     # Crate-specific docs
│
├── rust/
│   ├── lumen-core/src/
│   │   ├── lir.rs                # LIR bytecode, opcodes, instruction encoding
│   │   ├── values.rs             # Value enum (runtime values)
│   │   ├── strings.rs            # String interning
│   │   └── types.rs              # Type definitions
│   │
│   ├── lumen-compiler/src/
│   │   ├── lib.rs                # Entry point, pipeline orchestration
│   │   ├── markdown/extract.rs   # Code block extraction
│   │   └── compiler/
│   │       ├── lexer.rs           # Indentation-aware tokenizer
│   │       ├── parser.rs          # Recursive descent + Pratt parsing
│   │       ├── ast.rs            # AST node definitions
│   │       ├── resolve.rs         # Name resolution, symbol table
│   │       ├── typecheck.rs       # Bidirectional type inference
│   │       ├── constraints.rs     # Record where-clause validation
│   │       ├── lower.rs           # AST → LIR bytecode
│   │       └── [many more...]
│   │
│   ├── lumen-rt/src/
│   │   ├── lib.rs
│   │   ├── vm/
│   │   │   ├── mod.rs            # VM core, dispatch loop
│   │   │   ├── intrinsics.rs     # 140+ builtin functions
│   │   │   ├── ops.rs            # Arithmetic operations
│   │   │   ├── processes.rs      # Memory/Machine/Pipeline runtimes
│   │   │   └── continuations.rs  # Multi-shot continuations
│   │   └── services/
│   │       ├── tools.rs           # Tool dispatch, provider registry
│   │       ├── scheduler.rs       # M:N work-stealing scheduler
│   │       └── [many more...]
│   │
│   ├── lumen-cli/src/
│   │   ├── bin/lumen.rs          # CLI entry with Clap
│   │   ├── module_resolver.rs    # Import resolution
│   │   ├── repl.rs               # Interactive REPL
│   │   ├── fmt.rs                # Code formatter
│   │   ├── wares/                # Package manager
│   │   ├── auth.rs               # Ed25519 signing
│   │   ├── tuf.rs                # TUF metadata verification
│   │   └── [many more...]
│   │
│   ├── lumen-lsp/src/            # LSP implementation
│   ├── lumen-codegen/src/        # Cranelift JIT
│   └── lumen-provider-*/         # Tool providers
│
├── docs/                         # Documentation
├── examples/                     # 30 example programs
├── stdlib/                       # Standard library
├── tree-sitter-lumen/           # Tree-sitter grammar
└── editors/vscode/              # VS Code extension
```

---

## 2. Compiler Pipeline

### 2.1 Seven-Stage Pipeline

```mermaid
flowchart TB
    subgraph "Stage 1: Markdown Extraction"
        MD1["markdown/extract.rs<br/>extract_blocks(source)"]
        MD2["CodeBlocks + Directives"]
    end
    
    subgraph "Stage 2: Lexing"
        LEX1["compiler/lexer.rs<br/>Lexer::tokenize()"]
        LEX2["Tokens + INDENT/DEDENT"]
    end
    
    subgraph "Stage 3: Parsing"
        PAR1["compiler/parser.rs<br/>Parser::parse_program()"]
        PAR2["AST (Program, Items)"]
    end
    
    subgraph "Stage 4: Resolution"
        RES1["compiler/resolve.rs<br/>resolve()"]
        RES2["SymbolTable<br/>Name resolution, effect inference"]
    end
    
    subgraph "Stage 5: Typechecking"
        TC1["compiler/typecheck.rs<br/>typecheck()"]
        TC2["Type validation<br/>Exhaustiveness checking"]
    end
    
    subgraph "Stage 6: Constraint Validation"
        CONS1["compiler/constraints.rs<br/>validate_constraints()"]
        CONS2["where clause validation"]
    end
    
    subgraph "Stage 7: Lowering"
        LOW1["compiler/lower.rs<br/>lower()"]
        LOW2["LIR Module<br/>Bytecode + metadata"]
    end
    
    SOURCE["Source<br/>(.lm/.lm.md/.lumen)"]
    
    SOURCE --> MD1
    MD1 --> MD2
    MD2 --> LEX1
    LEX1 --> LEX2
    LEX2 --> PAR1
    PAR1 --> PAR2
    PAR2 --> RES1
    RES1 --> RES2
    RES2 --> TC1
    TC1 --> TC2
    TC2 --> CONS1
    CONS1 --> CONS2
    CONS2 --> LOW1
    LOW1 --> LOW2
```

### 2.2 Compiler Entry Points

| Function | Location | Purpose |
|----------|----------|---------|
| `compile(source)` | `lib.rs:781` | Main entry for `.lm.md`/`.lumen` |
| `compile_raw(source)` | `lib.rs:726` | Raw `.lm` files |
| `compile_with_options(source, opts)` | `lib.rs:786` | With compile options |
| `compile_with_imports(source, resolver)` | `lib.rs:214` | Multi-file compilation |

### 2.3 Key Compiler Files

| File | Lines | Purpose |
|------|-------|---------|
| `compiler/parser.rs` | ~8,100 | Recursive descent + Pratt parsing |
| `compiler/lower.rs` | ~6,700 | AST → LIR bytecode |
| `compiler/resolve.rs` | ~4,800 | Name resolution, symbol table |
| `compiler/typecheck.rs` | ~2,900 | Bidirectional type inference |
| `compiler/lexer.rs` | ~1,800 | Indentation-aware tokenizer |
| `compiler/ast.rs` | ~1,000 | AST node definitions |
| `markdown/extract.rs` | ~500 | Code block extraction |

---

## 3. VM & Runtime Architecture

### 3.1 Value Representation

```mermaid
classDiagram
    class Value {
        <<enum>>
        +Null
        +Bool(bool)
        +Int(i64)
        +BigInt(BigInt)
        +Float(f64)
        +String(StringRef)
        +Bytes(Vec~u8~)
        +List(Arc~Vec~Value~~)
        +Tuple(Arc~Vec~Value~~)
        +Set(Arc~BTreeSet~Value~~)
        +Map(Arc~BTreeMap~String, Value~~)
        +Record(Arc~RecordValue~)
        +Union(UnionValue)
        +Closure(ClosureValue)
        +Future(FutureValue)
        +TraceRef(TraceRefValue)
    }
    
    class StringRef {
        <<enum>>
        +Interned(u32)
        +Owned(String)
    }
    
    Value --> StringRef
```

**Design Decisions:**
- **Scalar variants**: Stack-allocated, no heap indirection
- **Collection variants**: `Arc<T>`-wrapped for COW semantics
- **Sets**: `BTreeSet<Value>` for O(log n) membership
- **String interning**: `StringRef::Interned(u32)` vs `Owned(String)`

### 3.2 VM Core Components

```mermaid
flowchart TB
    subgraph "VM Core (vm/mod.rs)"
        DISP["run_until()<br/>Dispatch Loop"]
        REG["Register File<br/>Vec~Value~"]
        FRAMES["Call Frame Stack<br/>Vec~CallFrame~"]
        EH["Effect Handler Stack<br/>Vec~EffectScope~"]
        SC["SuspendedContinuation<br/>Option~SuspendedContinuation~"]
    end
    
    subgraph "CallFrame"
        CF1["cell_idx: usize"]
        CF2["base_register: usize"]
        CF3["ip: usize"]
        CF4["return_register: usize"]
    end
    
    subgraph "Intrinsics (intrinsics.rs)"
        INT1["Core: print, debug, clone"]
        INT2["String: length, upper, split"]
        INT3["Math: abs, sqrt, sin"]
        INT4["Collection: map, filter, reduce"]
        INT5["File I/O: read_file, write_file"]
        INT6["HTTP: http_get, http_post"]
    end
    
    subgraph "Process Runtimes (processes.rs)"
        MEM["MemoryRuntime<br/>entries, kv store"]
        MACH["MachineRuntime<br/>state graph"]
        PIP["PipelineRuntime<br/>stage chaining"]
    end
    
    DISP --> REG
    DISP --> FRAMES
    DISP --> EH
    DISP --> SC
    FRAMES --> CF1
    FRAMES --> CF2
    FRAMES --> CF3
    FRAMES --> CF4
    DISP --> INT1
    DISP --> INT2
    DISP --> INT3
    DISP --> INT4
    DISP --> INT5
    DISP --> INT6
    DISP --> MEM
    DISP --> MACH
    DISP --> PIP
```

### 3.3 Dispatch Loop (run_until)

```mermaid
flowchart LR
    subgraph "Hot Loop"
        START["Entry"] --> FETCH
        
        FETCH["Fetch instruction"] --> DECODE
        DECODE["Decode registers"] --> MATCH
        
        MATCH{"match instr.op"}
        MATCH --> |"Call/TailCall/Intrinsic"| MUT1["Mutable path"]
        MATCH --> |"Pure ops"| PURE["Pure opcode match"]
        
        MUT1 --> EXEC1["Execute Call TailCall Intrinsic"]
        EXEC1 --> RELOAD["Reload frame state"]
        RELOAD --> FETCH
        
        PURE --> EXEC2["Execute opcode"]
        EXEC2 --> CHECK_IP["Check IP end"]
        CHECK_IP --> |Yes| POP["Pop frame return or continue"]
        CHECK_IP --> |No| FETCH
        
        POP --> RET1{"frames at limit"}
        RET1 --> |Yes| RETURN["Return result"]
        RET1 --> |No| RELOAD2["Reload frame state"]
        RELOAD2 --> FETCH
    end
```

**Key Optimizations:**
- Raw pointer caching for module access
- Local variable caching for frame state
- Batch instruction counting (4096 per sync)
- Pre-branch hints for debug/fuel checks

---

## 4. Full Execution Flow

### 4.1 From lumen run to Result

```mermaid
sequenceDiagram
    participant User
    participant CLI as "lumen-cli"
    participant Comp as "lumen-compiler"
    participant VM as "lumen-rt VM"
    participant CPU as "CPU"

    User->>CLI: lumen run file.lm --cell main
    CLI->>CLI: cmd_run() line 1081
    CLI->>CLI: read_source(file)
    
    CLI->>Comp: compile_with_imports(source, resolver)
    Note over Comp: Stage 1: Markdown Extract
    Note over Comp: Stage 2: Lexing
    Note over Comp: Stage 3: Parsing
    Note over Comp: Stage 4: Resolution
    Note over Comp: Stage 5: Typechecking
    Note over Comp: Stage 6: Constraints
    Note over Comp: Stage 7: Lowering to LirModule
    
    Comp-->>CLI: LirModule with cells
    
    CLI->>VM: VM::new()
    CLI->>VM: vm.load(module)
    Note over VM: Intern strings, register types, JIT init
    CLI->>VM: vm.execute(main, vec[])
    
    VM->>VM: run_until()
    Note over VM: Loop: Fetch, Decode, Execute, Control Flow
    
    loop "118 opcodes"
        VM->>VM: Execute current opcode
    end
    
    VM-->>CLI: Result Value
    CLI-->>User: Print result
```

### 4.2 Cell Call Cycle

```mermaid
flowchart TB
    subgraph "Call Setup"
        CS1["Lookup cell by name"]
        CS2["Allocate registers"]
        CS3["Load args into param regs"]
        CS4["Push CallFrame"]
    end
    
    subgraph "Execution"
        EX1["Execute instruction 0"]
        EX2["Execute instruction 1"]
        EX3["..."]
        EXn["Execute instruction N"]
    end
    
    subgraph "Return"
        RET1["Return opcode"]
        RET2["Pop CallFrame"]
        RET3["Shrink registers"]
        RET4["Write to return_register"]
    end
    
    CS1 --> CS2
    CS2 --> CS3
    CS3 --> CS4
    CS4 --> EX1
    EX1 --> EX2
    EX2 --> EX3
    EX3 --> EXn
    EXn --> RET1
    RET1 --> RET2
    RET2 --> RET3
    RET3 --> RET4
```

---

## 5. LIR Bytecode & Instruction Encoding

### 5.1 Instruction Format (64-bit Fixed Width)

```mermaid
graph LR
    subgraph "ABC Format"
        ABC["op pad a b c<br/>8 bytes"]
        ABC_U["R[A] = R[B] op R[C]"]
    end
    
    subgraph "ABx Format"
        ABX["op pad a bx<br/>8 bytes"]
        ABX_U["R[A] = constants[Bx]"]
    end
    
    subgraph "sAx Format"
        SAX["op pad offset<br/>8 bytes signed"]
        SAX_U["ip = ip + offset"]
    end
```

### 5.2 Byte Layout

```
Byte:    [  0  ] [  1  ] [  2-3   ] [ 4-5   ] [ 6-7   ]
Field:     op      pad       a          b          c
Bits:      8       8       16         16         16
Total:    64 bits (8 bytes)

For ABx format:
  bx = (b << 16) | c  → 32-bit constant index

For sAx format (jumps):
  offset = sign_extend((a << 32) | (b << 16) | c)  → 48-bit signed
```

### 5.3 Opcode Categories

| Category | Opcodes | Count |
|----------|---------|-------|
| **Load/Move** | LoadK, LoadNil, LoadBool, LoadInt, Move, MoveOwn | 6 |
| **Data Construction** | NewList, NewMap, NewRecord, NewUnion, NewTuple, NewSet | 6 |
| **Access** | GetField, SetField, GetIndex, SetIndex, GetTuple | 5 |
| **Arithmetic** | Add, Sub, Mul, Div, FloorDiv, Mod, Pow, Neg, Concat | 9 |
| **Bitwise** | BitOr, BitAnd, BitXor, BitNot, Shl, Shr | 6 |
| **Comparison** | Eq, Lt, Le, Not, And, Or, In, Is, NullCo, Test | 10 |
| **Control Flow** | Jmp, Call, TailCall, Return, Halt, Loop, ForPrep, ForLoop, Break, Continue | 10 |
| **Effects** | Perform, HandlePush, HandlePop, Resume | 4 |
| **Async** | Await, Spawn | 2 |
| **Intrinsics** | Intrinsic (140+ builtins) | 1 |
| **Closures** | Closure, GetUpval, SetUpval | 3 |
| **Tools** | ToolCall, Schema, Emit, TraceRef | 4 |

### 5.4 Critical Gotchas

```mermaid
flowchart TB
    subgraph "SIGNED JUMP GOTCHA"
        GOTCHA["NEVER use ax for jumps<br/>ALWAYS use sax"]
        
        WRONG["WRONG: ax() truncates negatives"]
        RIGHT["RIGHT: sax() sign-extends"]
        
        WRONG --> |"Truncates"| BAD["Jumps forward wrongly"]
        RIGHT --> |"Correct"| GOOD["Jumps backward correctly"]
    end
```

**Why it matters:**
- `ax_val()` returns unsigned 48-bit value
- `sax_val()` sign-extends from bit 47
- Backward jumps (loops) have negative offsets
- Using unsigned will cause loops to jump forward → infinite loop or crash

---

## 6. Effect System

### 6.1 Effect Declaration & Usage

```mermaid
flowchart TB
    subgraph "Source Code"
        E1["effect Console<br/>cell log(msg) -> Null end"]
        
        E2["handler MockConsole<br/>handle Console.log(msg)<br/>print(msg) end"]
        
        E3["cell main() -> Null / {Console}<br/>perform Console.log(hi)"]
        
        E4["let result = handle<br/>perform Console.log(hi)<br/>with<br/>Console.log(msg) => resume(null)<br/>end"]
    end
    
    subgraph "Compilation"
        C1["EffectDecl AST"]
        C2["HandlerDecl AST"]
        C3["Perform AST"]
        C4["HandleExpr AST"]
    end
    
    subgraph "LIR Lowering"
        L1["Emit HandlePush constants"]
        L2["Emit body code"]
        L3["Emit HandlePop"]
        L4["Emit Perform opcode"]
    end
    
    E1 --> C1
    E2 --> C2
    E3 --> C3
    E4 --> C4
```

### 6.2 Effect Handler Stack

```mermaid
sequenceDiagram
    participant Code as "User Code"
    participant VM as "VM"
    participant Stack as "effect_handlers"
    participant Cont as "suspended_continuation"

    Code->>VM: perform Effect.operation(args)
    
    VM->>Stack: Search (top → bottom)<br/>find(effect_name, operation)
    
    alt Handler Found
        VM->>Cont: Clone frames + registers
        VM->>Cont: Store resume_ip, result_reg
        VM->>Stack: Push EffectScope {handler_ip, ...}
        VM->>VM: Jump to handler_ip
        VM->>Code: Execute handler body
        
        Code->>VM: resume(value)
        VM->>VM: Restore continuation
        VM->>VM: Continue after perform
    else No Handler
        VM-->>Code: Error: unhandled effect
    end
```

### 6.3 Effect Opcodes

| Opcode | Operation | Description |
|--------|-----------|-------------|
| `HandlePush` | Push handler scope | Install effect handler on stack |
| `HandlePop` | Pop handler scope | Remove effect handler |
| `Perform` | Execute effect | Find handler, save continuation, jump |
| `Resume` | Continue computation | Restore continuation with value |

### 6.4 Effect Scope Structure

```rust
pub(crate) struct EffectScope {
    pub handler_ip: usize,        // Where handler code starts
    pub frame_idx: usize,         // Frame when handler was pushed
    pub base_register: usize,    // Register base for frame
    pub cell_idx: usize,         // Cell containing handler
    pub effect_name: String,     // "Console", "http", etc.
    pub operation: String,       // "log", "get", etc.
}

pub(crate) struct SuspendedContinuation {
    pub frames: Vec<CallFrame>,      // Stack at suspension
    pub registers: Vec<Value>,        // Registers at suspension
    pub resume_ip: usize,            // Where to continue
    pub resume_frame_count: usize,   // Frames to restore
    pub result_reg: usize,           // Where resume value goes
}
```

---

## 7. Type System

### 7.1 Type Variants

```mermaid
classDiagram
    class Type {
        <<enum>>
        +String
        +Int
        +Float
        +Bool
        +Bytes
        +Json
        +Null
        +List
        +Map
        +Record
        +Enum
        +Result
        +Union
        +Tuple
        +Set
        +Fn
        +Generic
        +TypeRef
        +Any
    }
    
    class TypeExpr {
        <<enum>>
        +Named
        +List
        +Map
        +Result
        +Union
        +Null
        +Tuple
        +Set
        +Fn
        +Generic
    }
```

### 7.2 Type Inference Flow

```mermaid
flowchart TB
    subgraph "Inference Mode"
        INF1["infer_expr returns Type"]
        INF2["Literal infers type"]
        INF3["Identifier lookup"]
        INF4["List infers element type"]
    end
    
    subgraph "Checking Mode"
        CHK1["check_compat expected actual"]
        CHK2["exact match"]
        CHK3["Int to Float implicit"]
        CHK4["Null in union"]
        CHK5["union member match"]
    end
    
    subgraph "Generic Inference"
        GEN1["unify for inference"]
        GEN2["build substitution"]
        GEN3["resolve with subst"]
    end
    
    subgraph "Match Exhaustiveness"
        EXH1["track covered variants"]
        EXH2["has catchall"]
        EXH3["report error if missing"]
    end
    
    INF1 --> INF2
    INF1 --> INF3
    INF1 --> INF4
    
    CHK1 --> CHK2
    CHK2 --> |No| CHK3
    CHK3 --> CHK4
    CHK4 --> CHK5
    
    GEN1 --> GEN2
    GEN2 --> GEN3
    
    EXH1 --> EXH2
    EXH2 --> |No| EXH3
```

### 7.3 Type Sugar

| Syntax | Desugars To | Example |
|--------|-------------|---------|
| `T?` | `T \| Null` | `Int?` → `Int \| Null` |
| `list[T]` | `List(Box<Type>)` | `list[Int]` |
| `map[K, V]` | `Map(K, V)` | `map[String, Int]` |
| `set[T]` | `Set(T)` | `set[Int]` |
| `result[T, E]` | `Result(T, E)` | `result[Int, String]` |

---

## 8. Tool System

### 8.1 Tool Declaration & Usage

```mermaid
flowchart TB
    subgraph "Declaration"
        D1["use tool llm.chat as Chat"]
        D2["grant Chat timeout_ms 30000"]
        D3["grant Chat max_tokens 4096"]
    end
    
    subgraph "Runtime Dispatch"
        R1["ToolCall opcode"]
        R2["validate_tool_policy()"]
        R3["Lookup provider"]
        R4["Call provider.call()"]
        R5["Record trace event"]
    end
    
    D1 --> D2
    D2 --> D3
    
    R1 --> R2
    R2 --> R3
    R3 --> R4
    R4 --> R5
```

### 8.2 Provider Registry

```mermaid
classDiagram
    class ProviderRegistry {
        +providers
        +register
        +lookup
        +call
    }
    
    class Provider {
        <<trait>>
        +call
        +capabilities
    }
    
    class ToolError {
        <<enum>>
        +NotFound
        +InvalidArgs
        +ExecutionFailed
        +RateLimit
        +AuthError
        +Timeout
        +ProviderUnavailable
        +OutputValidationFailed
    }
    
    class Capability {
        <<enum>>
        +TextGeneration
        +Chat
        +Embedding
        +Vision
        +ToolUse
        +StructuredOutput
        +Streaming
    }
    
    ProviderRegistry --> Provider
    ProviderRegistry --> ToolError
    ProviderRegistry --> Capability
```

### 8.3 Built-in Providers

| Provider | Tools |
|----------|-------|
| **fs** | fs.read, fs.write, fs.exists, fs.list, fs.mkdir, fs.remove |
| **env** | env.get, env.set, env.list, env.has, env.cwd, env.home |
| **json** | json.parse, json.stringify |
| **crypto** | crypto.sha256, crypto.sha512, crypto.md5, crypto.base64_encode, crypto.base64_decode, crypto.uuid, crypto.random_int, crypto.hmac_sha256 |
| **http** | http.get, http.post, http.put, http.delete |
| **gemini** | gemini.generate, gemini.chat, gemini.embed (if configured) |

---

## 9. CLI & Package Management

### 9.1 CLI Commands

```mermaid
graph TB
    CLI["lumen CLI"]
    
    CHECK["check"]
    RUN["run"]
    EMIT["emit"]
    FMT["fmt"]
    TEST["test"]
    LINT["lint"]
    DOC["doc"]
    REPL["repl"]
    TRACE["trace"]
    CACHE["cache"]
    BUILD["build"]
    MIGRATE["migrate"]
    DEBUG["debug"]
    
    WARES_INIT["wares init"]
    WARES_BUILD["wares build"]
    WARES_ADD["wares add"]
    WARES_REMOVE["wares remove"]
    WARES_PUBLISH["wares publish"]
    
    CLI --> CHECK
    CLI --> RUN
    CLI --> EMIT
    CLI --> FMT
    CLI --> TEST
    CLI --> LINT
    CLI --> DOC
    CLI --> REPL
    CLI --> TRACE
    CLI --> CACHE
    CLI --> BUILD
    CLI --> MIGRATE
    CLI --> DEBUG
    
    CLI --> WARES_INIT
    CLI --> WARES_BUILD
    CLI --> WARES_ADD
    CLI --> WARES_REMOVE
    CLI --> WARES_PUBLISH
```

### 9.2 Module Resolution

```mermaid
flowchart TB
    IMP["import module.path: Symbol"]
    
    subgraph "Resolution"
        R1["Convert dot path to slash<br/>foo.bar → foo/bar"]
        R2["Try extensions (in order)<br/>.lm, .lumen, .lm.md, .lumen.md"]
        R3["Try directory modules<br/>mod.lm, mod.lumen, main.lm"]
        R4["Search paths<br/>1. Source dir<br/>2. src/<br/>3. Root"]
    end
    
    subgraph "Compilation"
        C1["Compile imported module"]
        C2["Extract symbols"]
        C3["Merge into main module"]
    end
    
    IMP --> R1
    R1 --> R2
    R2 --> R3
    R3 --> C1
    C1 --> C2
    C2 --> C3
```

### 9.3 Package Manager (Wares)

```mermaid
flowchart TB
    subgraph "Wares CLI"
        W_INIT["wares init [name]"]
        W_BUILD["wares build"]
        W_ADD["wares add <pkg>"]
        W_REMOVE["wares remove <pkg>"]
        W_UPDATE["wares update"]
        W_SEARCH["wares search"]
        W_PUBLISH["wares publish"]
        W_LOGIN["wares login"]
    end
    
    subgraph "Registry"
        REG["Registry Client<br/>(registry.rs)"]
        INDEX["Global Index"]
        STORAGE["R2 Storage"]
        TRUST["Trust Verification"]
    end
    
    subgraph "Security"
        TUF["TUF Metadata<br/>(4-role)"]
        OIDC["OIDC Auth"]
        TRANS["Transparency Log"]
        AUDIT["Audit Logging"]
    end
    
    W_INIT --> REG
    W_ADD --> REG
    W_SEARCH --> REG
    W_PUBLISH --> REG
    
    REG --> INDEX
    REG --> STORAGE
    REG --> TRUST
    
    TUF --> OIDC
    TUF --> TRANS
    TUF --> AUDIT
```

---

## 10. Module Dependencies

### 10.1 Crate Dependency Graph

```mermaid
flowchart TB
    subgraph "lumen-core (base)"
        CORE[lumen-core<br/>LIR, Values, Types]
    end
    
    subgraph "lumen-compiler"
        COMP[lumen-compiler<br/>7-stage pipeline]
    end
    
    subgraph "lumen-rt"
        RT[lumen-rt<br/>VM, Runtime]
    end
    
    subgraph "lumen-cli"
        CLI[lumen-cli<br/>CLI, Pkg Manager]
    end
    
    subgraph "lumen-lsp"
        LSP[lumen-lsp<br/>LSP Server]
    end
    
    subgraph "lumen-codegen"
        CG[lumen-codegen<br/>JIT, WASM]
    end
    
    COMP --> CORE
    RT --> CORE
    CLI --> COMP
    CLI --> RT
    LSP --> COMP
    LSP --> RT
    CG --> RT
    CG --> CORE
```

### 10.2 Data Flow Between Components

```mermaid
sequenceDiagram
    participant S as "Source (.lm)"
    participant L as "Lexer"
    participant P as "Parser"
    participant R as "Resolver"
    participant T as "Typechecker"
    participant Low as "Lowerer"
    participant VM as "VM"
    
    S->>L: Raw source text
    L->>P: Token stream
    P->>R: AST (Program)
    R->>T: AST + SymbolTable
    T->>Low: Validated AST + Symbols
    Low->>VM: LirModule (bytecode)
    
    VM->>VM: run_until() loop
    VM->>VM: Execute instructions
    
    VM-->>S: Result value
```

---

## 11. Key Algorithms & Data Structures

### 11.1 Register Allocation

```mermaid
flowchart TB
    subgraph "Compile Time"
        C1["Analyze cell body"]
        C2["Count max registers needed"]
        C3["Emit instructions with reg indices"]
    end
    
    subgraph "Runtime"
        R1["Pre-allocate Vec Value capacity 4096"]
        R2["grow_registers returns base index"]
        R3["Write to registers base plus reg"]
        R4["shrink_registers on return"]
    end
    
    C1 --> C2
    C2 --> C3
    
    R1 --> R2
    R2 --> R3
    R3 --> R4
```

### 11.2 String Interning

```mermaid
classDiagram
    class StringTable {
        +interned
        +indexes
        +intern
        +lookup
        +get_interned
    }
    
    class StringRef {
        <<enum>>
        +Interned
        +Owned
    }
    
    StringTable --> StringRef
```

### 11.3 Copy-on-Write Collections

```mermaid
flowchart TB
    subgraph "Mutation"
        M1["Get collection via Arc"]
        M2["Arc make_mut"]
        M3{"Unique?"}
        M4["Clone on write"]
    end
    
    M1 --> M2
    M2 --> M3
    M3 --> |Yes| M5["Mutate in place"]
    M3 --> |No| M4
    
    M4 --> M5
```

---

## Summary

The Lumen programming language is a sophisticated AI-native systems language with:

- **7-stage compiler pipeline**: Markdown extraction → Lexer → Parser → Resolver → Typechecker → Constraints → Lowering
- **Register-based VM**: 64-bit fixed-width instructions, 118 opcodes, watermark-based register allocation
- **Full type system**: Primitives, generics, unions, optionals, bidirectional inference
- **Algebraic effects**: One-shot delimited continuations with handler stack
- **Tool system**: Provider registry, policy enforcement, 140+ builtins
- **Package manager**: Wares with TUF security, OIDC auth, transparency logs
- **Complete tooling**: CLI, REPL, formatter, LSP, debug adapter

The execution flow is: **Source → Compile → LIR → VM Load → Execute → Result**
