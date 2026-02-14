# Lumen Control Center Example

Multi-module showcase for a realistic release-ops control loop:
planning, execution, and recovery with explicit tool contracts.

## Architecture

- `lumen.toml`
  - Maps parser-supported tool IDs to providers.
  - Uses MCP bridge sections for GitHub and Slack tools.
  - MCP tool IDs follow `github.*` / `slack.*` naming (not `mcp.*`).
- `src/main.lm.md`
  - Orchestrates the end-to-end flow:
    - plan selection
    - execution command assembly
    - recovery/escalation decision
  - Builds the final run summary and event log.
- `src/domain/models.lm.md`
  - Shared workspace and work-queue records.
- `src/domain/events.lm.md`
  - Stage event + run summary records.
- `src/providers/contracts.lm.md`
  - `use tool` declarations and grants.
  - Explicit `bind effect` declarations for `llm`, `http`, and `mcp`.
  - Route and provider lookup helpers used by `main`.
- `src/workflows/planner.lm.md`
  - Generates plan steps and planner snapshot.
- `src/workflows/executor.lm.md`
  - Builds execution command and execution snapshot.
  - Simulates retry/fallback behavior in `dry-run` mode.
- `src/workflows/recovery.lm.md`
  - Produces recovery status and escalation level from execution outcomes.

## Example Flow

1. Planner chooses tools and builds staged steps.
2. Executor builds the dispatch command and simulates a research-stage timeout in `dry-run`.
3. Recovery module converts fallback usage into a degraded status plus notification recommendation.
4. Main emits structured stage events and human-readable run output.

## Commands

From repo root:

```bash
./target/debug/lumen check examples/lumen-control-center/src/main.lm.md
./target/debug/lumen run examples/lumen-control-center/src/main.lm.md
./target/debug/lumen ci examples/lumen-control-center
```

From the example directory:

```bash
cd examples/lumen-control-center
../../target/debug/lumen pkg check
```
