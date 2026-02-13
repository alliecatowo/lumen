# Lumen Language Audit (2026-02-13)

This document tracks the gap between compile-surface parity and runtime-semantic parity.
It is the implementation source of truth for closing all known gaps.

## Current State

- SPEC extracted blocks compile: 125/125.
- SPEC_ADDENDUM extracted blocks compile: 53/53.
- This indicates strong syntax coverage, not full semantic/runtime coverage.

## Confirmed Gaps

1. Addendum declarations are not all first-class IR/runtime constructs.
- `effect`, `handler`, `machine`, `memory`, `guardrail`, `eval`, `pipeline`, `orchestration`, `pattern` are still routed through generic addon handling in many cases.

2. Some declarations are skipped in lowering.
- `TypeAlias`, `Trait`, `Impl`, `Import`, `ConstDecl`, `MacroDecl` are currently not fully lowered to executable semantics.

3. Advanced patterns are not fully lowered with true matching semantics.
- Guard/or/list/tuple/record/type-check pattern shapes are partially tolerated but can degrade during lowering.

4. Async/orchestration is still mostly synchronous.
- VM behavior for `Await`/`Spawn` is still V1 synchronous.

5. Compiler permissiveness includes doc-oriented fallback behavior.
- Placeholder variables/types and generic addon fallback reduce strictness.

## Research Inputs (Primary Sources)

- Koka effect types and handlers:
  - https://koka-lang.github.io/koka/doc/index.html
- OCaml effect handlers:
  - https://v2.ocaml.org/manual/effects.html
- OpenAI structured outputs:
  - https://platform.openai.com/docs/guides/structured-outputs
- Constrained decoding papers:
  - https://aclanthology.org/2025.acl-long.911/
  - https://aclanthology.org/2025.coling-main.315/
- Temporal deterministic workflow guidance:
  - https://javadoc.io/static/io.temporal/temporal-sdk/1.23.2/io/temporal/workflow/package-summary.html
- MCP specification:
  - https://modelcontextprotocol.io/specification/2025-11-05/
- WASI 0.2 / component model:
  - https://bytecodealliance.org/articles/WASI-0.2
  - https://component-model.bytecodealliance.org/

## Resolution Plan (No Placeholders End-State)

1. First-class declaration model.
- Add typed AST + lowering for `effect`, `bind effect`, `handler`, then `machine`, `memory`, `guardrail`, `eval`, `pipeline`, `orchestration`, `pattern`.

2. Strict semantic lowering.
- Remove "skip lowering" for declarations that must affect behavior.
- Ensure IR captures declarations with enough structure for runtime.

3. Real runtime semantics.
- Implement effect dispatch/handlers and orchestration primitives.
- Implement deterministic replay model for stateful orchestration constructs.

4. Strict compiler mode as default.
- Replace permissive fallback with real diagnostics.
- Keep doc/demo mode explicit and opt-in if needed.

5. Conformance tests.
- Add semantic tests per language section, not only compile tests.
- Keep SPEC/SPEC_ADDENDUM compile coverage, but gate release on semantic test coverage.

## Progress Log

- 2026-02-13: Baseline parity established for all extracted spec blocks.
- 2026-02-13: Audit document created; implementation phase started.
