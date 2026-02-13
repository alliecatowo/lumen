# Advanced Workspace Example

Realistic multi-file workspace showing how to structure domain models, workflow stages, and provider/tool contracts for LLM + MCP style orchestration.

## Structure

- `lumen.toml`: package metadata plus provider and MCP bridge configuration
- `src/main.lm.md`: runnable entrypoint
- `src/domain/models.lm.md`: shared workspace/task records
- `src/domain/events.lm.md`: stage event and run summary records
- `src/providers/contracts.lm.md`: tool declarations, grants, and invocation contracts
- `src/workflows/planner.lm.md`: planner stage snapshot model
- `src/workflows/executor.lm.md`: executor stage snapshot model

## What This Demonstrates

- Multi-module imports across a package-style `src/` tree
- Provider mappings in `lumen.toml` (`llm.chat`, HTTP, MCP tool aliases)
- LLM/MCP tool usage patterns with grants and stubbed preview cells
- Deterministic local run path in `main` that does not require external API keys

## Run And Check

From repo root:

```bash
./target/debug/lumen check examples/advanced_workspace/src/main.lm.md
./target/debug/lumen run examples/advanced_workspace/src/main.lm.md
```

Optional package-wide validation (from the example directory):

```bash
cd examples/advanced_workspace
../../target/debug/lumen pkg check
```
