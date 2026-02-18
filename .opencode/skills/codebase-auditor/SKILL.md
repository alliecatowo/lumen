---
name: codebase-auditor
description: Comprehensive codebase exploration and documentation agent for Lumen - produces exhaustive technical references with Mermaid diagrams
---

# Lumen Codebase Auditor Skill

This skill enables an agent to perform exhaustive exploration of the Lumen codebase and generate comprehensive technical documentation.

## Capabilities

1. **Workspace Analysis**: Explore all crates, directories, and configuration
2. **Compiler Pipeline Deep Dive**: Trace all 7 stages with file-level precision
3. **VM/Runtime Analysis**: Understand register-based execution, value representation, dispatch
4. **Type System Analysis**: Document all type variants, inference mechanisms
5. **Effect System Analysis**: Trace perform→handler→resume flow
6. **Tool System Analysis**: Document provider registry, dispatch, policies
7. **CLI/Package Analysis**: Map commands, module resolution, wares
8. **Dependency Analysis**: Build crate dependency graphs
9. **Mermaid Generation**: Create valid diagrams for all architectures

## Exploration Strategy

### Phase 1: Workspace Discovery
```
1. Find all Cargo.toml files (workspace root + crate members)
2. Map directory structure under rust/
3. Identify key entry points (lib.rs, main.rs, bin/)
4. Read AGENTS.md and CLAUDE.md for context
```

### Phase 2: Compiler Pipeline
```
1. Read lib.rs entry points (compile, compile_raw, compile_with_imports)
2. Trace each stage: extract → lex → parse → resolve → typecheck → constraints → lower
3. Find key structs/functions in each stage
4. Document data flow between stages
```

### Phase 3: VM/Runtime
```
1. Read vm/mod.rs: run_until() hot loop
2. Understand Value enum (lumen-core values.rs)
3. Trace instruction execution (LIR opcodes)
4. Document register allocation, call frames
5. Understand effect handling (Perform, HandlePush, HandlePop, Resume)
```

### Phase 4: Type System
```
1. Find Type enum in typecheck.rs
2. Document inference (infer_expr) vs checking (check_compat)
3. Understand match exhaustiveness checking
4. Document type sugar (T? → T | Null)
```

### Phase 5: Tool System
```
1. Find ProviderRegistry in runtime services
2. Document tool dispatch flow
3. Understand policy enforcement
4. Map built-in providers (fs, http, crypto, etc.)
```

### Phase 6: CLI & Package Manager
```
1. Read bin/lumen.rs for all commands
2. Trace module resolution algorithm
3. Understand wares package manager
4. Document security infra (TUF, OIDC, Ed25519)
```

## Required Tools

The agent MUST use these tools extensively:

- **glob**: Find files by pattern (Cargo.toml, lib.rs, *.rs)
- **grep**: Search for key functions/structs
- **read**: Read source files with line numbers
- **task (explore)**: Launch sub-agents for parallel exploration

## Output Requirements

### Required Mermaid Diagrams

1. **Architecture Overview**: Workspace structure and crate dependencies
2. **Compiler Pipeline**: 7-stage data flow
3. **VM Execution**: Dispatch loop, register management
4. **Full Execution Flow**: `lumen run` → bytecode → result
5. **LIR Instruction Encoding**: ABC, ABx, sAx formats
6. **Effect System**: perform→handler→resume flow
7. **Type System**: inference and checking
8. **Tool Dispatch**: provider lookup and call
9. **CLI Commands**: command hierarchy
10. **Module Dependencies**: crate dependency graph

### Required Documentation Sections

For EACH section, include:
- File paths with line numbers
- Key structs/functions
- Data flow
- Design rationale
- Critical gotchas (marked with ⚠️)

### Valid Mermaid Syntax Rules

⚠️ **CRITICAL**: Mermaid in markdown has strict requirements:

1. **Node labels**: No special chars (quotes, backslashes, parens, newlines)
   - GOOD: `E1["effect Console"]`
   - BAD: `E1["effect Console<br/>cell log(msg: String) -> Null<br/>end"]`

2. **Flowchart directions**: Use TB, LR, BT, RL only

3. **Sequence diagrams**: Use `participant X as "Label"` format

4. **Class diagrams**: Use valid classDiagram syntax only

5. **Code blocks**: Use ```mermaid fences

## Document Structure

Generate `LUMEN_COMPLETE_REFERENCE.md` with:

```markdown
# Lumen Programming Language: Complete Technical Reference

## Table of Contents
1. [Architecture Overview](#1-architecture-overview)
2. [Compiler Pipeline](#2-compiler-pipeline)
...

## 1. Architecture Overview
### 1.1 Workspace Structure
[Mermaid diagram]
### 1.2 Directory Tree
[Tree with file counts]

## 2. Compiler Pipeline
### 2.1 Seven-Stage Pipeline
[Mermaid flowchart]
### 2.2 Stage Details
[Each stage with file:line references]

... (continue for all sections)
```

## Key File Locations (Verify These)

| Component | Expected Path |
|-----------|---------------|
| Compiler entry | `rust/lumen-compiler/src/lib.rs` |
| Lexer | `rust/lumen-compiler/src/compiler/lexer.rs` |
| Parser | `rust/lumen-compiler/src/compiler/parser.rs` |
| Resolver | `rust/lumen-compiler/src/compiler/resolve.rs` |
| Typechecker | `rust/lumen-compiler/src/compiler/typecheck.rs` |
| Lower | `rust/lumen-compiler/src/compiler/lower.rs` |
| LIR definitions | `rust/lumen-core/src/lir.rs` |
| Values | `rust/lumen-core/src/values.rs` |
| VM dispatch | `rust/lumen-rt/src/vm/mod.rs` |
| Intrinsics | `rust/lumen-rt/src/vm/intrinsics.rs` |
| CLI entry | `rust/lumen-cli/src/bin/lumen.rs` |
| Tool registry | `rust/lumen-rt/src/services/tools.rs` |

## Verification Checklist

Before generating output, verify:

- [ ] All file paths exist and match expected locations
- [ ] Mermaid diagrams are syntactically valid (no special chars in labels)
- [ ] Line numbers are accurate (use grep to verify)
- [ ] All 10+ required diagrams are present
- [ ] Each section has file:line references
- [ ] Critical gotchas are documented with ⚠️

## Execution Workflow

1. **Parallel Discovery**: Launch multiple explore agents for different areas
2. **Sequential Documentation**: Write sections after exploration
3. **Diagram Generation**: Create valid Mermaid for each area
4. **Cross-Reference**: Verify all file paths exist
5. **Output**: Write to `LUMEN_COMPLETE_REFERENCE.md`

## Important Notes

- The current reference may be outdated - verify ALL information independently
- Use `grep` to find exact line numbers, don't guess
- Launch parallel explore agents for efficiency
- Keep Mermaid labels simple - no special characters
- Include specific file:line citations for every claim
