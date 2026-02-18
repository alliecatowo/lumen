---
name: debug-lumen
description: Debug Adapter Protocol (DAP) server for debugging Lumen programs in VS Code and other DAP-compatible editors
---

# Debugging Lumen Programs

The DAP server lives in `rust/lumen-cli/src/dap.rs` and is invoked via `lumen debug <file>`.

## Architecture

### Threading Model
- **Main thread**: DAP I/O over stdin/stdout using the `dap` crate (v0.4.1-alpha1)
- **VM thread**: Runs the Lumen program, controlled via `mpsc` channels
- **Communication**: `DapCommand` (main -> VM), `DapEvent` (VM -> main)

### Key Types
| Type | Purpose |
|------|---------|
| `DapCommand` | Commands sent to VM thread (Continue, StepIn, Pause, GetStackTrace, etc.) |
| `DapEvent` | Events sent back from VM (Stopped, Exited, Output, StackTraceResult, etc.) |
| `DebuggerState` | VM thread state: breakpoints, step mode, call depth tracking |
| `StepMode` | Execution mode: Run, StepIn, StepOver, StepOut |
| `CachedFrame` | Snapshot of a call frame for variable inspection |

### DAP Request Handlers (`handle_request`)
All handlers are in a single match on `Command` variants. The function takes `&Request` and returns `Option<Response>`. Since `req.success()` consumes self, each call uses `req.clone().success(...)`.

Supported requests:
- `Initialize` — returns capabilities (function breakpoints, loaded sources, terminate)
- `Launch` — starts VM execution, supports `stopOnEntry` in additional data
- `SetBreakpoints` — returns unverified (no source-line mapping in LIR)
- `SetFunctionBreakpoints` — verifies cell names against module, sets breakpoints on cell entry
- `Threads` — single thread (id=1, name="main")
- `StackTrace` — queries VM for call frames
- `Scopes` — one scope per frame ("Locals" containing registers)
- `Variables` — reads registers from cached frames, formats all Value types
- `Continue`, `Next`, `StepIn`, `StepOut`, `Pause` — stepping control
- `Disconnect`, `Terminate` — cleanup
- `LoadedSources` — returns the source file

### VM Integration
- Uses `VM::set_fuel(1)` for single-step execution (fuel-based)
- `debug_callback` fires `DebugEvent::Step` on every instruction
- `DebugEvent::CallEnter`/`CallExit` track function entry/exit for step-over/step-out
- After fuel exhaustion, execution resumes via `VmDebugExt::continue_execution()`
- Function breakpoints check cell name on `CallEnter` events

### Value Formatting (`format_value_short`)
Formats all Lumen `Value` variants for DAP variable display:
- Scalars: direct representation (Int, Float, Bool, String, Null)
- Collections: summary with length (e.g., `list[3]`, `map{2}`, `set{3}`)
- Records: `TypeName(field1: v1, field2: v2)` with truncation
- Unions: `Tag(payload)` format
- Closures: `fn(arity)` with name if available
- Futures: `future<state>`

### Variable References
Composite values get unique `variables_reference` IDs for drill-down:
- `LOCALS_SCOPE_REF` (1000) + frame_id = scope reference for a frame
- Child variables of composites get incrementing IDs
- `is_composite()` determines if a Value should be expandable (lists, maps, records, tuples, sets)

## Key Constraints
1. **No source-line breakpoints** — LIR bytecode has no source mapping. Only function (cell) breakpoints work.
2. **Single thread** — DAP reports one thread. The VM runs synchronously on its thread.
3. **Fuel-based stepping** — Each `fuel=1` step executes exactly one LIR instruction.
4. **Register-based variables** — Variables are displayed as register contents. Named params use their names; others shown as `r0`, `r1`, etc.

## CLI Usage

```bash
lumen debug program.lm.md              # Launch DAP server for VS Code
lumen debug program.lm --allow-unstable # Allow unstable features
```

## VS Code Configuration

Add to `.vscode/launch.json`:
```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "lumen",
      "request": "launch",
      "name": "Debug Lumen",
      "program": "${file}",
      "stopOnEntry": true
    }
  ]
}
```

## Files
| File | Purpose |
|------|---------|
| `rust/lumen-cli/src/dap.rs` | DAP server implementation (~1400 lines) |
| `rust/lumen-cli/src/bin/lumen.rs` | CLI `debug` subcommand registration |
| `rust/lumen-rt/src/vm/mod.rs` | VM with debug_callback, public accessors |

## Related Skills
- `vm-architecture` — VM internals, dispatch loop, values
- `lir-encoding` — LIR instruction format, opcodes
- `cli-commands` — CLI command structure
