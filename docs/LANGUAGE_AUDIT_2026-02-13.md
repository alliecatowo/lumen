# Lumen Language Audit (2026-02-13)

This document tracks the gap between compile-surface parity and runtime-semantic parity.
It is the implementation source of truth for closing all known gaps.

## Current State

- SPEC extracted blocks compile: 125/125.
- SPEC_ADDENDUM extracted blocks compile: 53/53.
- Automated conformance sweep tests are now in-tree:
  - `rust/lumen-compiler/tests/spec_markdown_sweep.rs`
  - SPEC: 125/125
  - SPEC_ADDENDUM: 53/53
- This indicates strong syntax coverage, not full semantic/runtime coverage.

## Resolved in Code

1. Strict-by-default typechecking.
- Placeholder fallback is no longer default behavior.
- `@strict` defaults to `true`.
- `@doc_mode true` is now explicit opt-in for documentation permissiveness.

2. Advanced pattern semantics are lowered and executed.
- `guard`, `or`, list destructure (including `...rest`), tuple destructure, record destructure, and type-check patterns are lowered with real branch/fail control flow.
- Typechecker now binds/validates nested advanced patterns and guard boolean constraints.
- VM runtime tests now cover these pattern forms.

3. Async spawn/await now has first-class future handles.
- `Spawn` returns `Future`.
- `Await` resolves future results from VM-managed completion storage.
- `Return` in spawned frames stores resolved future values.

4. Process-family declarations are now executable runtime objects.
- `pipeline`, `orchestration`, `machine`, `memory`, `guardrail`, `eval`, `pattern` now lower to record type + constructor cells (like agents).
- Dot-call dispatch supports process constructors and instance methods.
- VM now provides concrete built-in runtime behavior for `memory` and `machine` method families.

5. Effect rows now have inference and strict enforcement.
- Resolver infers effects from statement/expression trees, tool calls, and transitive cell calls.
- For cells with explicit effect rows, strict mode now errors on inferred-but-undeclared effects.
- For cells with omitted effect rows, inferred effects are stored and checked against grants.
- Doc mode suppresses strict undeclared-effect errors to keep spec/doc sweeps practical.

6. Field access lowering is now robust for large string tables.
- Compiler field reads/writes now lower through `GetIndex`/`SetIndex` with string constants, avoiding 8-bit field-index overflow in `GetField`/`SetField`.
- VM `GetIndex`/`SetIndex` now support records directly.

## Remaining Gaps (Real, Not Cosmetic)

1. Full machine DSL semantics are not implemented.
- `state`, `transition`, `on_event`, timeout semantics, reachability/terminal verification, and typed transition checking from SPEC_ADDENDUM are still not compiled into executable machine graphs.

2. Algebraic effect handlers are still incomplete.
- `handler` declarations lower metadata and handle cells, but `with <handler> in ...` does not yet implement continuation-based effect interception semantics.

3. Pipeline/orchestration declarative blocks are not yet semantically compiled.
- Stage graph parsing/typeflow checks and orchestration strategy semantics (fan-out/fan-in, debate loops, etc.) remain incomplete.

4. Guardrail/eval declarations are not yet end-to-end semantic runtimes.
- They parse and lower as first-class process declarations, but addendum-specific policy/evaluation execution semantics are not complete.

5. Capability/effect grant checking is still heuristic.
- Grant compatibility currently relies on tool alias/path heuristics and bind mappings; it is not a full formal capability proof system yet.

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
- 2026-02-13: `effect`, `bind effect`, and `handler` added as first-class declarations in AST/resolve/lower/typecheck.
- 2026-02-13: Process-family declarations (`pipeline`, `orchestration`, `machine`, `memory`, `guardrail`, `eval`, `pattern`) moved to first-class parse/resolve/lower path.
- 2026-02-13: Lowering no longer drops `TypeAlias`, `Trait`, `Impl`, `Import`, `ConstDecl`, `MacroDecl`; all preserved in IR metadata.
- 2026-02-13: Parser hardened for multiline/null-coalescing continuations, role-block literal braces vs interpolation, machine/state nested sections, and executable `in` blocks in addendum statements.
- 2026-02-13: Ignored parity targets in `spec_suite.rs` enabled by default.
- 2026-02-13: Added automated SPEC/SPEC_ADDENDUM markdown sweep conformance tests in CI.
- 2026-02-13: Strict-by-default typecheck mode landed; doc-mode permissiveness made explicit (`@doc_mode true`).
- 2026-02-13: Advanced pattern lowering/typechecking upgraded from tolerant fallback to real runtime semantics.
- 2026-02-13: VM futures implemented for `spawn/await` result tracking.
- 2026-02-13: Process declarations (`pipeline/orchestration/machine/memory/...`) now lower to constructor-backed runtime objects with method dispatch.
- 2026-02-13: VM memory/machine runtime method semantics implemented and covered by runtime tests.
- 2026-02-13: Resolver effect inference + strict undeclared-effect diagnostics + inferred grant checks implemented.
- 2026-02-13: Field access lowering switched to index-keyed string constants to avoid 8-bit field index overflow at runtime.
