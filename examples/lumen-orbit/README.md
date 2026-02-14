# Lumen Orbit Workspace

`lumen-orbit` is a realistic multi-file example for AI-native orchestration with
explicit contracts, planning, execution, and recovery modules.

## Modules

- `src/models.lm.md`: shared records for queue state, snapshots, and run summaries
- `src/contracts.lm.md`: tool declarations, grants, effect bindings, route/provider contracts
- `src/planner.lm.md`: planning-stage snapshot and guardrail decisions
- `src/executor.lm.md`: command assembly and simulation of MCP execution outcomes
- `src/recovery.lm.md`: fallback and escalation logic
- `src/main.lm.md`: entrypoint orchestration, rendering, and executable `test_*` cells

## Commands

From repo root:

```bash
./target/debug/lumen check examples/lumen-orbit/src/main.lm.md
./target/debug/lumen run examples/lumen-orbit/src/main.lm.md
./target/debug/lumen test examples/lumen-orbit
./target/debug/lumen ci examples/lumen-orbit
```
