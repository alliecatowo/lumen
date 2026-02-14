# Contributing to Lumen

Thanks for your interest in contributing.

## Ground Rules

- Review and follow our [Code of Conduct](https://github.com/alliecatowo/lumen/blob/main/CODE_OF_CONDUCT.md).
- Use the issue templates for [bug reports](https://github.com/alliecatowo/lumen/blob/main/.github/ISSUE_TEMPLATE/bug_report.md) and [feature requests](https://github.com/alliecatowo/lumen/blob/main/.github/ISSUE_TEMPLATE/feature_request.md).
- Contributions are licensed under the [MIT License](https://github.com/alliecatowo/lumen/blob/main/LICENSE).

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
   Use the relevant issue template for consistency.
2. Use your agent workflow to implement the change.
3. Run checks locally (at minimum: `cargo test --workspace`).
4. Submit a PR with:
   - concise summary,
   - motivation,
   - testing evidence,
   - any follow-up work.
   Use the PR template at `.github/pull_request_template.md`.

## Where to Start

- [SPEC.md](https://github.com/alliecatowo/lumen/blob/main/SPEC.md) for current language behavior.
- [tasks.md](https://github.com/alliecatowo/lumen/blob/main/tasks.md) for concrete open work.
- [ROADMAP.md](https://github.com/alliecatowo/lumen/blob/main/ROADMAP.md) for long-horizon direction.
- [docs/](https://github.com/alliecatowo/lumen/tree/main/docs) for architecture and implementation notes.
