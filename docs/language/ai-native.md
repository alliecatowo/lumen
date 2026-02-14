# AI-Native Features

Lumen includes agent and tool primitives in the language and runtime rather than relying on framework-specific wrappers.

## Tools and grants

Declare tools and constrain behavior with grants:

```lumen
use tool llm.chat as Chat

grant Chat
  model "gpt-4o"
  max_tokens 512
  temperature 0.2
```

Runtime policy enforcement validates configured constraints before tool dispatch.

## Roles and prompt structure

Role prompts can be declared directly in cells:

```lumen
cell summarize(text: String) -> String
  role system: You are a concise assistant.
  role user: Summarize this text: {text}
  return "summary"
end
```

## Orchestration and async execution

Lumen runtime supports futures and orchestration constructs with deterministic semantics.

- `Spawn` creates future handles.
- `Await` resolves futures and propagates errors.
- Scheduling can run in eager or deterministic deferred modes.

See `docs/RUNTIME.md` for runtime details.

## Determinism profile

`@deterministic` enables strict checks that reject nondeterministic operations/effects where required.

This makes runs more reproducible for audit-heavy or safety-critical agent workflows.

## Process runtime objects

Lumen lowers process-family declarations into runtime-backed records with callable methods, including:

- `pipeline`
- `memory`
- `machine`

These behaviors are described in `docs/RUNTIME.md`.
