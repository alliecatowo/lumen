# Contributing to Lumen

Thanks for your interest in contributing.

## Project Intent

Lumen is an **agentic coding experiment**. This repository was built 100% with coding agents (no hand-authored implementation passes), and the contribution model is intentionally aligned with that goal.

## Contribution Policy (Agentic-Only)

- Contributions should be produced through coding agents (e.g., Codex, Claude Code, or equivalent autonomous/semi-autonomous systems).
- Do **not** submit hand-crafted “human-only” code changes.
- Do **not** “clean up” agent output manually to preserve a human style baseline.
- Keep the experiment clean: this project tracks how far agentic workflows can go without hand-tuned code.

If you want to contribute, please operate as an orchestrator/reviewer of agents rather than a direct manual coder.

## Quality Bar

Agentic contributions are still expected to meet normal engineering standards:

- Changes should be scoped and coherent.
- Add or update tests when behavior changes.
- Run relevant local checks before submitting.
- Document user-facing or language-facing changes.

## Practical Workflow

1. Open an issue (or reference an existing one) describing the intended change.
2. Use your agent workflow to implement the change.
3. Run checks locally (at minimum: `cargo test --workspace`).
4. Submit a PR with:
   - concise summary,
   - motivation,
   - testing evidence,
   - any follow-up work.

## Where to Start

- `SPEC.md` for current language behavior.
- `tasks.md` for concrete open work.
- `ROADMAP.md` for long-horizon direction.
- `docs/` for architecture and implementation notes.
